//! Documentation semantic fact extraction for repo-graph.
//!
//! This crate extracts semantic facts from documentation and
//! configuration files. It is independent of storage — the outer
//! layer maps `ExtractedFact` to storage DTOs.
//!
//! ## Architecture
//!
//! - `types`: Domain DTOs (ExtractedFact, DocKind, etc.)
//! - `discovery`: Find candidate doc/config files
//! - `classification`: Classify doc kind, detect generated content
//! - `extractors`: Per-format extraction (marker, frontmatter, keyword, config)
//!
//! ## Usage
//!
//! ```ignore
//! use repo_graph_doc_facts::{extract_semantic_facts, ExtractionResult};
//! use std::path::Path;
//!
//! let result = extract_semantic_facts(Path::new("/path/to/repo"))?;
//! println!("Extracted {} facts from {} files", result.facts.len(), result.files_scanned);
//! ```

pub mod classification;
pub mod discovery;
pub mod extractors;
pub(crate) mod release_notes; // DOCS-LIST-2 §2: `release-notes` STRUCTURAL subtree confirmation (extracted per review-1 item 2)
pub mod self_generated;
pub mod types;

pub use self_generated::{
    has_map_sidecar_name, is_os_noise, is_self_generated, is_tool_state_path, GENERATED_MARKER,
};

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub use types::{
    DocFile, DocKind, ExtractedFact, ExtractionMethod, ExtractionResult, ExtractionWarning,
    FactKind, RefKind,
};

/// Error type for documentation extraction operations.
#[derive(Debug, thiserror::Error)]
pub enum DocFactsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("path is not a directory: {0}")]
    NotADirectory(String),
}

/// Documentation inventory entry (primary documentation surface).
///
/// This is the shared DTO for `docs list`, `orient`, and future
/// persisted inventory. Docs are primary; semantic facts are secondary.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DocInventoryEntry {
    /// Path relative to repo root.
    pub path: String,
    /// Classified document kind.
    pub kind: String,
    /// Whether this is a generated document (e.g., MAP.md from rgistr).
    pub generated: bool,
    /// SHA-256 hash of content (optional, computed on demand).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// DOCS-LIST-2 §2 (DOC_FACTS_PUBLIC_API review-0, Option B): the CONFIRMED release/changelog
    /// subtree this doc lives under (e.g. `docs/releases`), or `None` for a non-release doc. Set ONLY
    /// on `release-notes` entries (STRUCTURAL basis — the subtree carries a manifest index, review-1
    /// item 1), from the crate-private [`release_notes`] module, so the renderer GROUPS release notes
    /// by family WITHOUT re-deriving the subtree rule across crates. ADDITIVE + optional:
    /// `skip_serializing_if = None` keeps the `docs list` JSON byte-identical for every non-release
    /// doc (and every pre-slice consumer).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_family: Option<String>,
}

/// Result of documentation inventory discovery.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DocInventoryResult {
    /// Discovered documentation files.
    pub entries: Vec<DocInventoryEntry>,
    /// Files matched by doc kind.
    pub counts_by_kind: HashMap<String, usize>,
    /// Generated docs encountered.
    pub generated_count: usize,
    /// Entries whose CONTENT could not be read (a genuine read failure — permission/IO — NOT a
    /// `NotFound`), so a CONTENT-BASED kind refinement is UNVERIFIABLE. Two disjoint contributors,
    /// one honest meaning ("we could not read N docs, so their content-based classification is
    /// unknown"):
    /// - **sidecar-NAMED** (`MAP.md` / `*_MAP.md`) whose `rmap map` generated-marker could not be
    ///   read (counted in the sidecar pass, works even when `compute_hashes == false`);
    /// - **non-sidecar** docs whose content read failed during the hashing pass (DOCS-LIST-2 review-0
    ///   F4), so the `license` marker check below could not run — a failed license read must NOT be
    ///   silently indistinguishable from "no license marker" (honesty rule #1). Counted only when
    ///   `compute_hashes` (the only path that reads non-sidecar content).
    ///
    /// Such an entry is ADMITTED to the inventory (conservative: never silently excluded) and left at
    /// its location/name-based kind + `generated = false` — but we do NOT assert authorship OR
    /// "not a license": it is UNKNOWN, counted here so the surface can say so out loud
    /// ("+N unreadable, counted"), never a silent claim (operator RULING 3, honesty rule #1). ⊆ the
    /// entry count.
    pub unreadable_count: usize,
}

/// Discover documentation inventory without extracting semantic facts.
///
/// This is the primary documentation surface. Returns doc file paths,
/// kinds, and generated flags. Content hashing is optional.
///
/// ## Generated flag semantics (marker-gated — SELF-POLLUTION-1 §2.1 + FIXTURE-POLLUTION-1 §2.4)
///
/// The `generated` flag is **evidence-based** — a CONTENT marker, never the filename.
/// For a sidecar-NAMED file (`MAP.md` / `*_MAP.md`) the content is read ONLY for the
/// sidecar candidates (a large doc tree is never fully read here) and classified
/// generated by EITHER stamped marker via [`sidecar_is_generated`]: the current
/// `rmap map` first-line [`self_generated::GENERATED_MARKER`] (rmap's OWN exhaust,
/// shared with drift + seed), OR any generator's YAML frontmatter generation marker
/// (`generated_by` / `generated: true` / `kind: synthesized_summary`) — which is what
/// catches the legacy `rgistr` LLM maps foreign smoke-runs drop into the tree (§2.4).
/// THREE read outcomes, never collapsed (operator RULING 3):
/// - **marker present** (first-line HTML OR frontmatter) → `generated = true` (excluded
///   by default; a foreign generated map no longer poses as architecture).
/// - **read OK, no marker** → `generated = false`, authored (a user's hand-authored
///   `MAP.md` name-collision, or an explicit `generated: false`, stays in the inventory —
///   a bare name is not evidence).
/// - **`NotFound`** → the file is gone; no marker to read → `generated = false`.
/// - **unreadable** (permission/IO, NOT `NotFound`) → UNKNOWN: `generated = false` so
///   it is ADMITTED (conservative, never silently excluded), but it is NOT asserted
///   authored — it is counted in `unreadable_count` so the surface can say
///   "+N unreadable, counted". Never a silent "not generated" from a failed read.
///
/// For richer semantic fact extraction, use `extract_semantic_facts` instead.
pub fn discover_doc_inventory(
    repo_path: &Path,
    compute_hashes: bool,
) -> Result<DocInventoryResult, DocFactsError> {
    if !repo_path.is_dir() {
        return Err(DocFactsError::NotADirectory(
            repo_path.display().to_string(),
        ));
    }

    // SELF-POLLUTION-1 §3: `.env*` is never a *document* — secrets-adjacent, zero
    // doc value — so it never enters the inventory (and thus never orient's Docs
    // line). It remains a discovery candidate for `docs extract`'s env-surface
    // hint; only the inventory surface drops it.
    let mut doc_files: Vec<DocFile> = discovery::discover_doc_files(repo_path)
        .into_iter()
        .filter(|d| !is_env_path(&d.relative_path))
        .collect();

    // Count of docs whose content could not be read → content-based kind refinement UNVERIFIABLE.
    // The sidecar generated-marker pass below adds to it too (disjoint sets, no double count).
    let mut unreadable_count = 0usize;

    // Optionally read and hash content (for staleness detection). DOCS-LIST-2 review-0 F4: do NOT
    // discard the fallible read (`let _ = …`) — its result FEEDS the `license` content classification
    // below. Only `NotFound` is genuine absence (silent); any other IO error means the file EXISTS
    // but is unreadable, so the license-marker check cannot run and the doc must be surfaced as
    // UNKNOWN, never silently treated as "no license marker" (honesty rule #1). Sidecar-named docs
    // are counted by the sidecar pass instead (they get a marker check there), so we exclude them
    // here to keep the two contributors disjoint.
    if compute_hashes {
        for doc in &mut doc_files {
            match read_and_hash(doc) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => {
                    if !self_generated::has_map_sidecar_name(&doc.relative_path) {
                        unreadable_count += 1;
                    }
                }
            }
        }
    }

    // SELF-POLLUTION-1 §2.1 + FIXTURE-POLLUTION-1 §2.4: the `generated` flag is
    // EVIDENCE-based, shared with drift + seed via `is_self_generated`. For a
    // sidecar-NAMED file we read the file's COMPLETE content (`read_doc_content`) and
    // classify generated by EITHER stamped marker (`sidecar_is_generated`): the current
    // `rmap map` first-line HTML marker (rmap's own exhaust) OR any generator's YAML
    // frontmatter generation marker (`generated_by` / `generated: true` /
    // `kind: synthesized_summary`, which catches the foreign `rgistr` maps). A
    // marker-less MAP.md (name collision) or an explicit `generated: false` stays
    // `generated = false` and is NOT excluded. A read that FAILS (permission/IO, not
    // `NotFound`) is UNKNOWN — admitted (conservative) but counted in `unreadable_count`,
    // never silently asserted authored (operator RULING 3).
    for doc in &mut doc_files {
        if self_generated::has_map_sidecar_name(&doc.relative_path) {
            match read_doc_content(doc) {
                MarkerRead::Content(content) => {
                    doc.generated = sidecar_is_generated(&doc.relative_path, &content);
                }
                // The file is gone (`NotFound`): no marker to read, honestly authored.
                MarkerRead::Absent => doc.generated = false,
                // A genuine read failure: we cannot prove exhaust NOR authorship.
                // Admit it (generated = false) but flag it as unknown, never a silent
                // "not generated" assertion.
                MarkerRead::Unreadable => {
                    doc.generated = false;
                    unreadable_count += 1;
                }
            }
        } else {
            // `.rgr/` tool-state paths (never reached by discovery today) and all
            // other docs: not self-generated (name-definitional, no read).
            doc.generated = self_generated::is_self_generated(&doc.relative_path, None);
        }
    }

    // DOCS-LIST-2 §2 (review-1 item 1 + review-2 item 1): the `release-notes` kind's STRUCTURAL basis.
    // A doc under a release-family-named subtree is `release-notes` ONLY when that subtree's own
    // `index.{txt,rst,md}` manifest is present in THIS doc set AND that index's CONTENT is INSPECTED
    // and carries a Sphinx `toctree` directive — the manifest relationship that makes it a real
    // documentation section. The file's NAME (directory named `releases`, file named `index.*`) is not
    // evidence (review-2 item 1); the toctree directive is. A release-named directory whose index lacks
    // a toctree — or that has no index — keeps `architecture` (no deterministic basis → old kind).
    //
    // The candidate index's CONTENT is consulted here only when the `compute_hashes` pass already
    // loaded it (the `docs list` path) — the SAME no-read discipline the `license` kind uses below
    // (review-4 item 1: an on-demand read on the `compute_hashes == false` path silently swallowed
    // `Unreadable` as "not confirmed", leaving docs `architecture` with no unknown rendered — the
    // zero-collapse class). With no loaded content there is NO BASIS, so the kind is unchanged
    // ("no basis keeps the old kind"); a genuine read failure at `compute_hashes == true` is already
    // surfaced via `unreadable_count` (the `read_and_hash` pass counted it), never double-counted.
    // Determined over the whole doc set because a single path cannot see the manifest
    // (why `classify_doc_kind` does not emit this kind). Precedence: upgrades only the catch-all kinds
    // (`architecture` / `config`); a `readme` / `map` under the subtree stays intact.
    let mut confirmed_release_subtrees: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for doc in &doc_files {
        if let Some(dir) = release_notes::manifest_index_subtree(&doc.relative_path) {
            if let Some(content) = &doc.content {
                if release_notes::is_manifest_index_content(content) {
                    confirmed_release_subtrees.insert(dir.to_string());
                }
            }
        }
    }
    for doc in &mut doc_files {
        if matches!(doc.kind, DocKind::Architecture | DocKind::Config)
            && release_notes::release_subtree_of(&doc.relative_path, &confirmed_release_subtrees)
                .is_some()
        {
            doc.kind = DocKind::ReleaseNotes;
        }
    }

    // DOCS-LIST-2 §2: the `license` kind is a CONTENT basis (SPDX / license-header marker), so it
    // is decided HERE where content is in hand (the `compute_hashes` pass above already read it),
    // never from the `LICENSE` filename. Precedence: it upgrades only the catch-all kinds
    // (`architecture` / `config`) — a `readme`, `map`, or `release-notes` classification (each a
    // stronger, location/structure-stable signal) is left intact (a release note already moved off
    // `architecture` above, so it is never re-labeled `license`). When content is absent
    // (compute_hashes == false) there is no basis, so the kind is unchanged — "no basis keeps the old
    // kind" (STANDING HONESTY RULE). This never re-reads: it consults `doc.content` already loaded.
    for doc in &mut doc_files {
        if matches!(doc.kind, DocKind::Architecture | DocKind::Config) {
            if let Some(content) = &doc.content {
                if classification::has_license_marker(content) {
                    doc.kind = DocKind::License;
                }
            }
        }
    }

    // Build entries
    let entries: Vec<DocInventoryEntry> = doc_files
        .iter()
        .map(|f| DocInventoryEntry {
            path: f.relative_path.clone(),
            kind: f.kind.as_str().to_string(),
            generated: f.generated,
            content_hash: f.content_hash.clone(),
            // DOCS-LIST-2 §2: attach the CONFIRMED release subtree so the renderer groups release
            // notes by family from this DTO field, not a cross-crate call. Set only on `release-notes`
            // entries (`None` otherwise, skipped in JSON). Single source of truth = `release_notes`.
            release_family: (f.kind == DocKind::ReleaseNotes)
                .then(|| {
                    release_notes::release_subtree_of(&f.relative_path, &confirmed_release_subtrees)
                })
                .flatten()
                .map(str::to_string),
        })
        .collect();

    // Count by kind
    let mut counts_by_kind: HashMap<String, usize> = HashMap::new();
    for entry in &entries {
        *counts_by_kind.entry(entry.kind.clone()).or_insert(0) += 1;
    }

    let generated_count = entries.iter().filter(|e| e.generated).count();

    Ok(DocInventoryResult {
        entries,
        counts_by_kind,
        generated_count,
        unreadable_count,
    })
}

/// Extract semantic facts from a repository's documentation.
///
/// This is the top-level facade that orchestrates:
/// 1. Discovery of doc/config files
/// 2. Classification of each file
/// 3. Reading and hashing content
/// 4. Running extractors
/// 5. Aggregating results
///
/// For finer control, use the individual modules directly.
pub fn extract_semantic_facts(repo_path: &Path) -> Result<ExtractionResult, DocFactsError> {
    if !repo_path.is_dir() {
        return Err(DocFactsError::NotADirectory(
            repo_path.display().to_string(),
        ));
    }

    // Step 1: Discover candidate files
    let mut doc_files = discovery::discover_doc_files(repo_path);

    // Step 2: Read content and compute hashes
    let mut warnings = Vec::new();
    for doc in &mut doc_files {
        match read_and_hash(doc) {
            Ok(()) => {}
            Err(e) => {
                warnings.push(ExtractionWarning {
                    file: doc.relative_path.clone(),
                    message: format!("failed to read: {}", e),
                });
            }
        }
    }

    // Step 3: Set the generated flag from content evidence.
    //
    // Discovery leaves `generated = false` (a bare name is not evidence); content
    // analysis is the authoritative signal here:
    //   - Explicit frontmatter (generated: true/false) → use that value
    //   - Readable content but silent on generated → NOT generated (no evidence)
    //   - Unreadable content → stays `false` (the conservative default; we cannot
    //     prove generation from a file we could not read — honesty rule #1)
    for doc in &mut doc_files {
        if let Some(content) = &doc.content {
            match classification::get_generated_from_frontmatter(content) {
                Some(explicit) => {
                    // Frontmatter is explicit, use it
                    doc.generated = explicit;
                }
                None => {
                    // Content is readable but silent on generated status → no evidence.
                    doc.generated = false;
                }
            }
        }
        // Unreadable content: keep the conservative `false` default (never asserted
        // generated from an unread file).
    }

    // Step 4: Run extractors on each file
    let mut all_facts = Vec::new();
    for doc in &doc_files {
        let facts = extractors::extract_from_file(doc);
        all_facts.extend(facts);
    }

    // Step 5: Compute summary statistics
    let files_scanned = doc_files.len();
    let mut files_by_kind: HashMap<DocKind, usize> = HashMap::new();
    let mut generated_docs_count = 0;

    for doc in &doc_files {
        *files_by_kind.entry(doc.kind).or_insert(0) += 1;
        if doc.generated {
            generated_docs_count += 1;
        }
    }

    Ok(ExtractionResult {
        facts: all_facts,
        files_scanned,
        files_by_kind,
        generated_docs_count,
        warnings,
    })
}

/// Is `rel_path` a `.env*` file (by basename)? Such files are secrets-adjacent
/// and never a document (SELF-POLLUTION-1 §3). Public so the seed corpus pass
/// applies the SAME `.env` rule as the inventory (§2.4 — one truth); it is a
/// pure name predicate (the extension defines it — no content to read).
pub fn is_env_path(rel_path: &str) -> bool {
    rel_path
        .rsplit('/')
        .next()
        .unwrap_or(rel_path)
        .starts_with(".env")
}

/// The outcome of reading a sidecar candidate's content for the generated-marker check.
/// THREE outcomes, never collapsed (operator RULING 3, honesty rule #1) — the same
/// distinction the drift path draws with its `CandidateLine`:
///   - `Content` — the file was read; carries its full content (empty for an empty file).
///   - `Absent` — `io::NotFound`: the file is genuinely gone (only `NotFound` means
///     absent).
///   - `Unreadable` — any other IO error: the file exists (or its state is unknown)
///     but could not be read; NOT evidence either way, so it is UNKNOWN.
enum MarkerRead {
    Content(String),
    Absent,
    Unreadable,
}

/// FIXTURE-POLLUTION-1 §2.4 — is a sidecar-NAMED (`MAP.md`/`*_MAP.md`) file GENERATED
/// content, by its stamped marker (never its name)? TWO generator conventions carry a
/// marker and both count as evidence — the current `rmap map` first-line HTML comment
/// ([`self_generated::GENERATED_MARKER`], rmap's OWN exhaust), and any generator's YAML
/// frontmatter generation marker (`generated_by` / `generated: true` /
/// `kind: synthesized_summary`), which is what catches the legacy `rgistr` LLM maps that
/// pollute `smoke-runs/**`.
///
/// An explicit `generated: false` in frontmatter is authoritative AUTHORED (a hand-
/// authored `MAP.md` name-collision stays listed). Silence + no rmap marker ⇒ authored
/// (a bare name is never evidence). This is content-marker based, repo-agnostic — it
/// catches foreign generated maps the old first-line-only check missed, WITHOUT ever
/// classifying from the filename.
fn sidecar_is_generated(relative_path: &str, content: &str) -> bool {
    let first_line = content.lines().next().unwrap_or("");
    // rmap's own first-line HTML marker (definitive: rmap's exhaust).
    if self_generated::is_self_generated(relative_path, Some(first_line)) {
        return true;
    }
    // Any generator's frontmatter marker. `Some(false)` (explicit authored) and `None`
    // (silent) both fall through to authored — never asserted generated from a name.
    matches!(
        classification::get_generated_from_frontmatter(content),
        Some(true)
    )
}

/// A single doc's full content, for a content-marker check (renamed from `sidecar_content` —
/// review-2 broadened it to a SECOND caller, the release-notes manifest inspection, so the old
/// `sidecar_` name would mislead). Uses already-loaded content when present (the hash pass);
/// otherwise reads the file, distinguishing genuinely-absent (`NotFound`) from unreadable
/// (permission/IO). The `NotFound`-vs-unreadable split is the honesty distinction operator RULING 3
/// requires (a bare `.ok()`/`Err => None` collapse would erase it and silently assert absence).
/// Each caller gates it to a BOUNDED candidate set (sidecar-NAMED files, or release-named
/// `<dir>/index.*` manifests), so a large doc tree is never fully read here.
fn read_doc_content(doc: &DocFile) -> MarkerRead {
    if let Some(content) = &doc.content {
        return MarkerRead::Content(content.clone());
    }
    match fs::read_to_string(&doc.path) {
        Ok(content) => MarkerRead::Content(content),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => MarkerRead::Absent,
        Err(_) => MarkerRead::Unreadable,
    }
}

/// Read file content and compute SHA-256 hash.
fn read_and_hash(doc: &mut DocFile) -> Result<(), std::io::Error> {
    let content = fs::read_to_string(&doc.path)?;

    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let hash = hasher.finalize();
    let hash_hex = hex::encode(hash);

    doc.content = Some(content);
    doc.content_hash = Some(hash_hex);

    Ok(())
}

// Re-export hex for internal use
mod hex {
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        let bytes = bytes.as_ref();
        let mut result = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            result.push(HEX_CHARS[(b >> 4) as usize] as char);
            result.push(HEX_CHARS[(b & 0x0f) as usize] as char);
        }
        result
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
