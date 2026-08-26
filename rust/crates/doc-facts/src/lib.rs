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
    /// Sidecar-NAMED entries (`MAP.md` / `*_MAP.md`) that could NOT be read to check
    /// the `rmap map` marker (a genuine read failure — permission/IO — NOT a
    /// `NotFound`). Such an entry is ADMITTED to the inventory (conservative: never
    /// silently excluded) and left `generated = false` — but we do NOT assert it is
    /// authored: it is UNKNOWN, counted here so the surface can say so out loud
    /// ("+N unreadable, counted"), never a silent "not generated" (operator RULING 3,
    /// honesty rule #1). ⊆ the entry count.
    pub unreadable_count: usize,
}

/// Discover documentation inventory without extracting semantic facts.
///
/// This is the primary documentation surface. Returns doc file paths,
/// kinds, and generated flags. Content hashing is optional.
///
/// ## Generated flag semantics (marker-gated — SELF-POLLUTION-1 §2.1)
///
/// The `generated` flag is **evidence-based**, shared with drift + seed through the
/// one `self_generated::is_self_generated` predicate so the three surfaces cannot
/// diverge on what "rmap's own exhaust" means. A sidecar-NAMED file (`MAP.md` /
/// `*_MAP.md`) is classified from the first-line `rmap map`
/// [`self_generated::GENERATED_MARKER`], read ONLY for sidecar names (a large doc tree
/// is never fully read here). THREE outcomes, never collapsed (operator RULING 3):
/// - **marker present** → `generated = true` (rmap's own exhaust; excluded by default).
/// - **read OK, no marker** → `generated = false`, authored (a user's hand-authored
///   `MAP.md` name-collision stays in the inventory — a bare name is not evidence).
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

    // Optionally read and hash content (for staleness detection)
    if compute_hashes {
        for doc in &mut doc_files {
            let _ = read_and_hash(doc);
        }
    }

    // SELF-POLLUTION-1 §2.1: the `generated` flag is EVIDENCE-based, shared with
    // drift + seed via `is_self_generated`. For a sidecar-NAMED file we read the first
    // line and require the `rmap map` marker; a user's marker-less MAP.md (name
    // collision) stays `generated = false` and is NOT excluded. A read that FAILS
    // (permission/IO, not `NotFound`) is UNKNOWN — admitted (conservative) but counted
    // in `unreadable_count`, never silently asserted authored (operator RULING 3).
    let mut unreadable_count = 0usize;
    for doc in &mut doc_files {
        if self_generated::has_map_sidecar_name(&doc.relative_path) {
            match first_line_of(doc) {
                MarkerRead::Line(line) => {
                    doc.generated =
                        self_generated::is_self_generated(&doc.relative_path, Some(&line));
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

    // Build entries
    let entries: Vec<DocInventoryEntry> = doc_files
        .iter()
        .map(|f| DocInventoryEntry {
            path: f.relative_path.clone(),
            kind: f.kind.as_str().to_string(),
            generated: f.generated,
            content_hash: f.content_hash.clone(),
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

/// The outcome of reading a sidecar candidate's first line for the `rmap map` marker
/// check. THREE outcomes, never collapsed (operator RULING 3, honesty rule #1) — the
/// same distinction the drift path draws with its `CandidateLine`:
///   - `Line` — the file was read; carries its first line (empty for an empty file).
///   - `Absent` — `io::NotFound`: the file is genuinely gone (only `NotFound` means
///     absent).
///   - `Unreadable` — any other IO error: the file exists (or its state is unknown)
///     but could not be read; NOT evidence either way, so it is UNKNOWN.
enum MarkerRead {
    Line(String),
    Absent,
    Unreadable,
}

/// The first line of a doc file, for the `rmap map` marker check. Uses already-loaded
/// content when present (hash pass); otherwise reads the file, distinguishing
/// genuinely-absent (`NotFound`) from unreadable (permission/IO). The `NotFound`-vs-
/// unreadable split is the honesty distinction operator RULING 3 requires (a bare
/// `.ok()`/`Err => None` collapse would erase it and silently assert "not generated").
fn first_line_of(doc: &DocFile) -> MarkerRead {
    if let Some(content) = &doc.content {
        return MarkerRead::Line(content.lines().next().unwrap_or("").to_string());
    }
    match fs::read_to_string(&doc.path) {
        Ok(content) => MarkerRead::Line(content.lines().next().unwrap_or("").to_string()),
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
