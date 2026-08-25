//! The pure embed pipeline (spec §3.5 + §5 + §4.3). [`build_store`] ties the two
//! ports together with a caller-supplied **file reader** and **cancel token**, so
//! the entire pass — corpus enumeration, the source/snapshot-race re-hash, batch
//! embedding, cancellation, and envelope encoding — is testable with fakes (no
//! model, no daemon, no DB). The daemon's `seed_pass.rs` supplies the real
//! reader (`std::fs::read_to_string`), the real cancel flag, and the real ports.

use crate::document::build_document;
use crate::hash::content_hash;
use crate::ports::{EmbedError, Embedder, SeedCorpusEntry};
use crate::rank::l2_normalize;
use crate::store::{encode, SeedStoreError, SeedStoreKey, SeedVectorBodyV1, SeedVectorEntryV1};

/// Batch size — 32 documents per embed request (`spike.py:98`). The cancel token
/// is consulted at each batch boundary (spec §5.1).
pub const EMBED_BATCH_SIZE: usize = 32;
/// Corpus admission cap (spec §8.4). Above it, the first `CORPUS_CAP` files by
/// `path` order are embedded and the remainder is reported as an honest omission
/// (never silently dropped). A tunable safety bound for the 160k-file monorepo
/// target — INFERRED default, adjustable.
pub const CORPUS_CAP: usize = 50_000;

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

/// What a completed (or aborted) build produced. The daemon pass matches this and
/// records an honest doctor/oplog fact for each arm — the tier *declines*, it
/// never errors the index.
#[derive(Debug)]
pub enum BuildOutcome {
    /// A store was built. `bytes` are ready for [`crate::store::atomic_write`].
    Built { bytes: Vec<u8>, report: BuildReport },
    /// The corpus was empty (repo not indexed / no seedable files) — nothing to build.
    NoCorpus,
    /// The cancel token fired at a batch boundary — the caller must NOT publish
    /// (a newer index's pass wins; the prior store stays valid).
    Cancelled,
    /// The model seam declined (endpoint down, malformed, dim/model mismatch) →
    /// honest "no hints" skip; the prior store (if any) is untouched.
    Embed(EmbedError),
    /// The assembled store exceeded the budget → "seeding declined" (spec §4.3).
    Store(SeedStoreError),
}

/// Honest counts for the doctor/oplog line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildReport {
    /// Files admitted and stored (passed the source/snapshot-race check). This is
    /// the total store size = `reused` + freshly embedded.
    pub admitted: usize,
    /// Of `admitted`, files whose vector was **copied forward** from the prior
    /// sidecar because their `content_hash` was unchanged (spec §5 incremental
    /// refresh) — these made NO embed call this pass.
    pub reused: usize,
    /// Files omitted because the working tree drifted from the snapshot pin, or
    /// could not be read (spec §3.5 — omit, never store under a wrong pin).
    pub drifted: usize,
    /// Files beyond the corpus cap, embedded by nobody this pass (spec §8.4).
    pub corpus_omitted: usize,
}

/// Source/snapshot-race admission (spec §3.5): re-hash the working-tree content
/// with the SAME function the scanner used and admit only when it equals the
/// snapshot's recorded `content_hash`. Returns the admitted entry + its document,
/// or `None` when the tree drifted (omit — never store a fresh body under an old
/// pin). The stored vector's pin therefore always equals both the bytes that
/// produced it and the snapshot's `content_hash` — a stale-body-under-fresh-pin
/// state is unrepresentable.
pub fn admit(entry: &SeedCorpusEntry, content: &str) -> Option<(SeedCorpusEntry, String)> {
    if content_hash(content) == entry.content_hash {
        Some((entry.clone(), build_document(&entry.path, content)))
    } else {
        None
    }
}

/// Build a `.vec` store from an ALREADY-READ corpus. Pure given its embedder +
/// closures. The caller reads the corpus under whatever DB lock it needs and then
/// releases it BEFORE calling this — the embed phase (working-tree reads + model
/// calls) can take tens of seconds and must NOT hold a DB lock (else it would
/// block `forget`/`index`).
///
/// - `read_file(path)` reads the current working-tree content for a repo-relative
///   path; an `Err` (missing/unreadable) counts as drift and the file is omitted.
/// - `cancel()` is checked at each batch boundary; a `true` aborts without
///   publishing.
/// - `prior` is the previously-published store (decoded under the SAME pin, so
///   its vectors match the current `dim`), or `None` for a first build. Spec §5
///   incremental refresh: any admitted file whose `content_hash` equals its prior
///   entry's `content_hash` copies that vector **forward** (no embed call); only
///   changed/new files are embedded. A cancelled/failed embed still never
///   publishes, so a stale-under-fresh-pin store stays unrepresentable.
// Each argument is a distinct, irreducible input (the corpus, the embedder, two
// closures, the pins, the caller-supplied clock, the tunables, and the prior store).
#[allow(clippy::too_many_arguments)]
pub fn build_store<R, C>(
    mut entries: Vec<SeedCorpusEntry>,
    embedder: &dyn Embedder,
    read_file: R,
    cancel: C,
    key: &SeedStoreKey,
    created_at: u64,
    cfg: BuildConfig,
    prior: Option<&SeedVectorBodyV1>,
) -> BuildOutcome
where
    R: Fn(&str) -> std::io::Result<String>,
    C: Fn() -> bool,
{
    if entries.is_empty() {
        return BuildOutcome::NoCorpus;
    }

    // Corpus cap: keep the first `corpus_cap` by path order (the port already
    // returns path-ascending), report the rest as an honest omission.
    let corpus_omitted = entries.len().saturating_sub(cfg.corpus_cap);
    if corpus_omitted > 0 {
        entries.truncate(cfg.corpus_cap);
    }

    // Read + admit (source/snapshot-race check).
    let mut admitted_entries: Vec<SeedCorpusEntry> = Vec::new();
    let mut docs: Vec<String> = Vec::new();
    let mut drifted = 0usize;
    for entry in &entries {
        match read_file(&entry.path) {
            Ok(content) => match admit(entry, &content) {
                Some((e, doc)) => {
                    admitted_entries.push(e);
                    docs.push(doc);
                }
                None => drifted += 1,
            },
            Err(_) => drifted += 1,
        }
    }

    // Copy-forward index (spec §5): file_uid → prior entry. Because `prior` was
    // decoded under the current pin, any prior vector already matches `dim` and is
    // already L2-normalized (stored normalized) — a reused vector is byte-identical
    // to the one a re-embed would produce for unchanged content.
    let dim = embedder.dim();
    let prior_by_uid: std::collections::HashMap<&str, &SeedVectorEntryV1> = prior
        .map(|b| b.entries.iter().map(|e| (e.file_uid.as_str(), e)).collect())
        .unwrap_or_default();

    // Decide reuse-vs-embed per admitted file, in admitted order.
    enum Slot {
        Reused(Vec<f32>),
        Pending,
    }
    let mut slots: Vec<Slot> = Vec::with_capacity(admitted_entries.len());
    let mut to_embed: Vec<String> = Vec::new();
    let mut reused = 0usize;
    for (i, entry) in admitted_entries.iter().enumerate() {
        match prior_by_uid.get(entry.file_uid.as_str()) {
            Some(p) if p.content_hash == entry.content_hash && p.vector.len() == dim => {
                slots.push(Slot::Reused(p.vector.clone()));
                reused += 1;
            }
            _ => {
                slots.push(Slot::Pending);
                to_embed.push(docs[i].clone());
            }
        }
    }

    // Embed ONLY the changed/new docs, in batches, checking cancel at each
    // boundary. An empty `to_embed` (a no-change refresh) makes ZERO embed calls.
    let batch_size = cfg.batch_size.max(1);
    let mut embedded: Vec<Vec<f32>> = Vec::with_capacity(to_embed.len());
    for batch in to_embed.chunks(batch_size) {
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

    // Weave reused + freshly-embedded vectors back into admitted order.
    let mut embedded_iter = embedded.into_iter();
    let vectors: Vec<Vec<f32>> = slots
        .into_iter()
        .map(|s| match s {
            Slot::Reused(v) => v,
            Slot::Pending => embedded_iter
                .next()
                .expect("one embedded vector per pending slot"),
        })
        .collect();

    // Assemble body (admitted entries paired positionally with their vectors).
    let entries_v1: Vec<SeedVectorEntryV1> = admitted_entries
        .iter()
        .zip(vectors)
        .map(|(e, vector)| SeedVectorEntryV1 {
            file_uid: e.file_uid.clone(),
            path: e.path.clone(),
            content_hash: e.content_hash.clone(),
            vector,
        })
        .collect();

    let body = SeedVectorBodyV1 {
        entries: entries_v1,
    };
    match encode(&body, key, created_at) {
        Ok(bytes) => BuildOutcome::Built {
            bytes,
            report: BuildReport {
                admitted: admitted_entries.len(),
                reused,
                drifted,
                corpus_omitted,
            },
        },
        Err(e) => BuildOutcome::Store(e),
    }
}

/// Unit tests live in `pass_tests.rs` (split via `#[path]` to keep this file
/// under the 500-line guardrail — review-2 #7).
#[cfg(test)]
#[path = "pass_tests.rs"]
mod tests;
