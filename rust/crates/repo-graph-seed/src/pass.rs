//! The pure chunk-embed pipeline (spec §2/§3/§5). [`build_store`] turns an
//! already-read corpus of SYMBOL chunks into a set of [`SeedVectorEntry`] the
//! daemon persists to the per-snapshot `seed_vectors` table. It ties the model
//! seam together with a caller-supplied **file reader** + **cancel token**, so the
//! whole pass — span slicing, the source/snapshot-race re-hash, copy-forward,
//! batch embedding, cancellation — is testable with fakes (no model, no daemon, no
//! DB). The daemon's `seed_pass.rs` supplies the real reader and ports.

use std::collections::HashMap;

use crate::classify;
use crate::document::{build_chunk_document, MAX_BODY_LINES};
use crate::hash::content_hash;
use crate::ports::{EmbedError, Embedder, SeedCorpusEntry, SeedVectorEntry};
use crate::rank::l2_normalize;

/// Batch size — 32 documents per embed call (spike `spike.py:98`). The cancel token
/// is consulted at each batch boundary (spec §5.1).
pub const EMBED_BATCH_SIZE: usize = 32;
/// Chunk admission cap (spec §8.4). Above it, the first `CORPUS_CAP` chunks by the
/// corpus's `(path, line)` order are embedded and the remainder is an honest
/// omission (never silently dropped). INFERRED default for the large-monorepo target.
pub const CORPUS_CAP: usize = 200_000;

/// Tunables for a build (defaults are the spec constants).
#[derive(Debug, Clone, Copy)]
pub struct BuildConfig {
    pub batch_size: usize,
    pub corpus_cap: usize,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            batch_size: EMBED_BATCH_SIZE,
            corpus_cap: CORPUS_CAP,
        }
    }
}

/// What a completed (or aborted) build produced.
#[derive(Debug)]
pub enum BuildOutcome {
    /// A vector set was built, ready for the daemon to write into `seed_vectors`.
    Built {
        entries: Vec<SeedVectorEntry>,
        report: BuildReport,
    },
    /// The corpus was empty (repo not indexed / no seedable chunks) — nothing to build.
    NoCorpus,
    /// The cancel token fired at a batch boundary — the caller must NOT publish.
    Cancelled,
    /// The model seam declined → honest "no hints" skip; the prior vectors are untouched.
    Embed(EmbedError),
}

/// Honest counts for the doctor/oplog line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildReport {
    /// Chunks admitted and stored (passed the source/snapshot-race check).
    pub admitted: usize,
    /// Of `admitted`, chunks whose vector was **copied forward** from the prior
    /// snapshot because their `(stable_key, content_hash)` was unchanged (spec §5) —
    /// these made NO embed call this pass.
    pub reused: usize,
    /// Chunks omitted because their file drifted from the snapshot pin, could not be
    /// read, or had no span (spec §2.1/§3.5 — omit, never store under a wrong pin).
    pub drifted: usize,
    /// Chunks beyond the corpus cap, embedded by nobody this pass (spec §8.4).
    pub corpus_omitted: usize,
}

/// Slice the span source for a node from its file's lines. 1-indexed inclusive
/// `[line_start, line_end]`, bounded by the file length and capped at
/// [`MAX_BODY_LINES`] (the document builder caps again; this bounds the allocation
/// for a pathologically large span). Returns `None` when the node has no start line
/// (no span → no chunk, spec §2.1).
fn span_source(lines: &[&str], line_start: Option<i64>, line_end: Option<i64>) -> Option<String> {
    let start = line_start?.max(1) as usize; // 1-indexed
    let start_idx = start - 1;
    if start_idx >= lines.len() {
        return None; // span points past the current file content → treat as no span
    }
    let end_1 = line_end.unwrap_or(line_start?).max(line_start?) as usize;
    let end_idx = end_1.min(lines.len()); // inclusive end, clamped
    let capped_end = end_idx.min(start_idx + MAX_BODY_LINES);
    Some(lines[start_idx..capped_end].join("\n"))
}

/// Build a per-snapshot seed-vector set from an ALREADY-READ chunk corpus. Pure
/// given its embedder + closures.
///
/// - `read_file(path)` reads the current working-tree content for a repo-relative
///   path; an `Err` counts as drift and ALL that file's chunks are omitted.
/// - `cancel()` is checked at each batch boundary; a `true` aborts without publishing.
/// - `prior` is the previous snapshot's vectors (already filtered by the daemon to
///   the CURRENT model — a model change hands `&[]`, forcing a full re-embed). Any
///   admitted chunk whose `(stable_key, content_hash)` matches a prior entry copies
///   that vector forward (no embed); only changed/new chunks are embedded.
#[allow(clippy::too_many_arguments)]
pub fn build_store<R, C>(
    entries: Vec<SeedCorpusEntry>,
    embedder: &dyn Embedder,
    read_file: R,
    cancel: C,
    cfg: BuildConfig,
    prior: &[SeedVectorEntry],
) -> BuildOutcome
where
    R: Fn(&str) -> std::io::Result<String>,
    C: Fn() -> bool,
{
    if entries.is_empty() {
        return BuildOutcome::NoCorpus;
    }

    // Corpus cap: keep the first `corpus_cap` in the corpus's (path, line) order.
    let corpus_omitted = entries.len().saturating_sub(cfg.corpus_cap);
    let mut entries = entries;
    if corpus_omitted > 0 {
        entries.truncate(cfg.corpus_cap);
    }

    // Group chunks by file path (the corpus is ordered by path, so this is a single
    // pass), read each file ONCE, admit via the source/snapshot-race re-hash, and
    // slice each node's span from the shared file content.
    let dim = embedder.dim();
    let mut admitted: Vec<SeedVectorEntry> = Vec::new();
    let mut docs: Vec<String> = Vec::new(); // parallel to `admitted`, for pending slots
    let mut drifted = 0usize;

    // Prior index: (stable_key, content_hash) → &vector (dim-matched only).
    let prior_by_key: HashMap<(&str, &str), &Vec<f32>> = prior
        .iter()
        .filter(|e| e.vector.len() == dim)
        .map(|e| ((e.stable_key.as_str(), e.content_hash.as_str()), &e.vector))
        .collect();

    enum Slot {
        Reused,  // vector already set on the row (copy-forward)
        Pending, // its doc is at the same index in `docs`
    }
    let mut slots: Vec<Slot> = Vec::new();
    let mut reused = 0usize;

    let mut i = 0usize;
    while i < entries.len() {
        let path = entries[i].path.clone();
        // Collect the contiguous run of chunks for this path.
        let run_start = i;
        while i < entries.len() && entries[i].path == path {
            i += 1;
        }
        let run = &entries[run_start..i];

        // Read the file once; a read error drifts the whole run.
        let content = match read_file(&path) {
            Ok(c) => c,
            Err(_) => {
                drifted += run.len();
                continue;
            }
        };
        // Source/snapshot-race admission: the file's content_hash (shared by all its
        // chunks) must equal the snapshot pin, else omit the whole run.
        let file_hash = content_hash(&content);
        let file_lines: Vec<&str> = content.lines().collect();
        // SEED-CHUNK-2: per-file structural test regions computed ONCE per file (the
        // `#[cfg(test)] mod` / `describe(` bodies), reused for every chunk in the run.
        let lang = classify::lang_for_path(&path);
        let test_regions = classify::compute_test_regions(lang, &content);
        for chunk in run {
            if file_hash != chunk.content_hash {
                drifted += 1;
                continue;
            }
            let span = match span_source(&file_lines, chunk.line_start, chunk.line_end) {
                Some(s) => s,
                None => {
                    drifted += 1; // no span → no chunk
                    continue;
                }
            };
            // SEED-CHUNK-2 per-chunk classification (spec §2.1/§2.2). is_test is
            // PROMOTE-ONLY: the file fact OR structural per-symbol evidence, so a chunk
            // never DROPS below its file classification on weak evidence. is_decl is a
            // pure structural read of the span (a bodyless callable signature).
            let structural_test = chunk.line_start.is_some_and(|ls| {
                classify::structural_is_test(lang, &file_lines, ls as usize, &test_regions)
            });
            let is_test = chunk.is_test || structural_test;
            let is_decl = classify::is_declaration(&chunk.path, chunk.subtype.as_deref(), &span);
            let vector_row = SeedVectorEntry {
                node_uid: chunk.node_uid.clone(),
                stable_key: chunk.stable_key.clone(),
                file_uid: chunk.file_uid.clone(),
                path: chunk.path.clone(),
                line: chunk.line_start,
                qualified_name: chunk.qualified_name.clone(),
                is_test,
                is_decl,
                content_hash: chunk.content_hash.clone(),
                vector: Vec::new(), // filled below
            };
            match prior_by_key.get(&(chunk.stable_key.as_str(), chunk.content_hash.as_str())) {
                Some(v) => {
                    let mut row = vector_row;
                    row.vector = (*v).clone();
                    admitted.push(row);
                    slots.push(Slot::Reused); // vector already set on the row
                    reused += 1;
                }
                None => {
                    let doc = build_chunk_document(
                        chunk.qualified_name.as_deref(),
                        chunk.doc_comment.as_deref(),
                        &span,
                    );
                    admitted.push(vector_row);
                    slots.push(Slot::Pending);
                    docs.push(doc);
                }
            }
        }
    }

    if admitted.is_empty() {
        return BuildOutcome::Built {
            entries: Vec::new(),
            report: BuildReport {
                admitted: 0,
                reused,
                drifted,
                corpus_omitted,
            },
        };
    }

    // Embed ONLY the pending docs, in batches, checking cancel at each boundary.
    let batch_size = cfg.batch_size.max(1);
    let mut embedded: Vec<Vec<f32>> = Vec::with_capacity(docs.len());
    for batch in docs.chunks(batch_size) {
        if cancel() {
            return BuildOutcome::Cancelled;
        }
        let batch_vecs = match embedder.embed(batch) {
            Ok(v) => v,
            Err(e) => return BuildOutcome::Embed(e),
        };
        if batch_vecs.len() != batch.len() {
            return BuildOutcome::Embed(EmbedError::Malformed {
                detail: format!(
                    "batch cardinality mismatch: sent {}, got {}",
                    batch.len(),
                    batch_vecs.len()
                ),
            });
        }
        for mut v in batch_vecs {
            if v.len() != dim {
                return BuildOutcome::Embed(EmbedError::DimMismatch {
                    expected: dim,
                    got: v.len(),
                });
            }
            l2_normalize(&mut v);
            embedded.push(v);
        }
    }

    // Weave freshly-embedded vectors into the pending slots (reused slots already
    // carry their vector).
    let mut embedded_iter = embedded.into_iter();
    for (row, slot) in admitted.iter_mut().zip(slots.into_iter()) {
        if let Slot::Pending = slot {
            row.vector = embedded_iter
                .next()
                .expect("one embedded vector per pending slot");
        }
    }

    let report = BuildReport {
        admitted: admitted.len(),
        reused,
        drifted,
        corpus_omitted,
    };
    BuildOutcome::Built {
        entries: admitted,
        report,
    }
}

/// Unit tests live in `pass_tests.rs` (split via `#[path]` to keep this file under
/// the 500-line guardrail).
#[cfg(test)]
#[path = "pass_tests.rs"]
mod tests;
