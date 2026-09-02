//! Compose-side Rust `#[cfg(test)]` inclusion-chain resolver (IS-TEST-RUST-1).
//!
//! The rust-extractor emits, per file, its EXTERNAL `mod <name>;` declarations
//! with their `#[cfg(test)]` gating and `#[path]` overrides (onto FILE-node
//! metadata; see `repo-graph-rust-extractor::mod_decls`). This module is the
//! RESOLUTION half: given every Rust file's declarations for a snapshot, it
//! walks the cross-file inclusion chain and returns the set of files that are
//! test-only by STRUCTURAL evidence — a file whose path to a crate root crosses
//! a `#[cfg(test)]` gate (directly, or transitively via a test ancestor).
//!
//! ## Promote-only (deliberate, honest scope)
//!
//! This resolver only ADDS `is_test = true` from cfg(test) structural evidence.
//! It never DEMOTES a file the prior (path-based) classification marked test.
//! Rationale:
//!   - The measured gap (slice §1) is UNDER-counting: in-crate test modules
//!     (`src/**/tests.rs`, `#[cfg(test)] mod tests;`) carry `is_test = 0`.
//!     Promotion closes exactly that gap.
//!   - Demoting would require distinguishing library/binary crate roots
//!     (`src/lib.rs`, `src/main.rs`, `src/bin/*`) from integration-test / bench
//!     / example roots (`tests/`, `benches/`, `examples/`), because a
//!     `mod common;` inside `tests/foo.rs` is NOT `#[cfg(test)]`-gated yet its
//!     target IS test code (the whole target is a test binary). Modeling cargo
//!     target roots to avoid corrupting that classification is real machinery
//!     the measured gap does not require — a named future extension, not built
//!     speculatively.
//!   - Honesty: we assert a test label ONLY where cfg(test) structure proves
//!     it; files we do not touch retain their existing stored fact. We never
//!     introduce a name-based test label.
//!
//! The path-resolution + chain-walk logic mirrors the ratified narrow resolver
//! in `daemon-runtime/tests/consolidation_witness.rs` (proven against this
//! crate's own source), adapted to consume emitted facts instead of re-reading
//! files, and to report unresolved inclusions as diagnostics rather than
//! asserting.

use std::collections::{BTreeMap, BTreeSet};

/// Consumer-side DTO for a single EXTERNAL `mod <name>;` declaration fact, read
/// back off a FILE node's `metadata_json` (produced by the rust-extractor).
///
/// This is the CONSUMER half of the extractor→compose JSON boundary. The
/// rust-extractor owns the crate-private PRODUCER DTO (`mod_decls::RustModDecl`)
/// that serializes this exact shape; repo-index owns THIS deserializer. Two
/// small DTOs on the two sides of a documented JSON boundary — never a shared
/// public type (a shared type would be a cross-crate public API the slice packet
/// forbids). The wire contract is the JSON field names + the object key below;
/// both sides must agree on those, and only those. Rejected simpler: importing
/// the extractor's type (was the iteration-1 shape; review-1 required removing
/// the resulting public API surface).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct RustModDecl {
    /// Module identifier in `mod <name>;`.
    pub name: String,
    /// True iff this inclusion is compiled only under `#[cfg(test)]`.
    pub cfg_test: bool,
    /// `#[path = "…"]` override, verbatim (relative to the declaring file's
    /// directory, or — when `inline_path` is non-empty — to the inline context
    /// directory). `None` when the module uses the conventional file layout.
    #[serde(rename = "path", default)]
    pub path_override: Option<String>,
    /// Directory segments contributed by enclosing INLINE `mod <seg> { … }`
    /// blocks, OUTERMOST first (see the producer). Empty for a top-level
    /// declaration; `["scope"]` for `mod scope { mod child; }`, which makes the
    /// resolver seek the target under `<submodule_dir>/scope/`.
    #[serde(default)]
    pub inline_path: Vec<String>,
}

/// The `metadata_json` object key the rust-extractor rides the [`RustModDecl`]
/// array under. MUST match the producer's `mod_decls::MOD_DECLS_METADATA_KEY`.
const MOD_DECLS_METADATA_KEY: &str = "rust_mod_decls";

/// Parse the `rust_mod_decls` facts off one FILE node's `metadata_json`.
///
/// Returns the declarations plus whether parsing a NON-null blob failed. A file
/// with no `mod` statements has `metadata_json = None` (or an object without the
/// key) → no decls, no failure — the common, correct case, not a swallow. A
/// non-null blob that fails to parse is a genuine anomaly: we surface it (the
/// caller counts it into a diagnostic) and treat the file as declaring nothing,
/// which can only MISS a promotion (honest degradation toward the prior
/// path-based value) — never fabricate a test label.
pub fn parse_file_mod_decls(metadata_json: Option<&str>) -> (Vec<RustModDecl>, bool) {
    let Some(raw) = metadata_json else {
        return (Vec::new(), false);
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return (Vec::new(), true);
    };
    match value.get(MOD_DECLS_METADATA_KEY) {
        None => (Vec::new(), false),
        Some(arr) => match serde_json::from_value::<Vec<RustModDecl>>(arr.clone()) {
            Ok(decls) => (decls, false),
            Err(_) => (Vec::new(), true),
        },
    }
}

/// One Rust file's inclusion facts: its repo-relative path and the external
/// `mod` declarations it makes.
#[derive(Debug, Clone)]
pub struct RustFileFacts {
    pub rel_path: String,
    pub mod_decls: Vec<RustModDecl>,
}

/// A `mod` inclusion that could not be resolved to a unique existing file. The
/// declaring file keeps its existing classification; this is surfaced as an
/// extraction diagnostic (never a guessed classification).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedInclusion {
    pub declaring_file: String,
    pub mod_name: String,
    /// Why this inclusion did not yield a clean promote edge. All four are
    /// FAIL-CLOSED: the target keeps its existing classification, never a guess.
    ///   - `"no_candidate_file"`: no existing file matches the declaration.
    ///   - `"ambiguous_target"`: two conventional candidates exist (`foo.rs`
    ///     AND `foo/mod.rs`) — real Rust rejects this too; we refuse to pick.
    ///   - `"multiple_parents"`: the target file is declared as a module from
    ///     two different parents — a shape this narrow resolver does not model.
    ///   - `"duplicate_declaration"`: the same parent declares the module more
    ///     than once (e.g. cfg variants) — its cfg(test) status is ambiguous.
    pub reason: &'static str,
}

/// Result of structural classification over one snapshot's Rust files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClassifyOutcome {
    /// Files to promote to `is_test = true` by cfg(test) structural evidence.
    pub test_files: BTreeSet<String>,
    /// Inclusions this resolver could not resolve; declaring files keep their
    /// existing classification. Deterministic order.
    pub unresolved: Vec<UnresolvedInclusion>,
}

/// One resolved inclusion edge: the file it pulls in is compiled by `parent`,
/// and `cfg_test` is true iff that `mod` declaration is `#[cfg(test)]`-gated.
#[derive(Debug, Clone)]
struct ModEdge {
    parent: String,
    cfg_test: bool,
}

/// Classify the snapshot's Rust files. Returns the cfg(test)-reachable set to
/// promote and any unresolved inclusions to diagnose. Pure — no I/O.
///
/// FAIL-CLOSED (review-1 item 2): a promote edge is created ONLY when a target
/// is compiled by exactly ONE unambiguous declaration. Any ambiguity — two
/// conventional candidate files, two parents, or the same parent declaring a
/// module twice — POISONS the target: no edge, never promoted, and the
/// ambiguity is surfaced as a diagnostic. A poisoned target therefore keeps its
/// existing classification even if one of the colliding declarations was
/// `#[cfg(test)]`-gated; we never promote on a guess.
pub fn classify(files: &[RustFileFacts]) -> ClassifyOutcome {
    let file_set: BTreeSet<String> = files.iter().map(|f| f.rel_path.clone()).collect();
    let mut unresolved: Vec<UnresolvedInclusion> = Vec::new();

    // Deterministic: iterate declaring files in path order.
    let mut ordered: Vec<&RustFileFacts> = files.iter().collect();
    ordered.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

    // Pass 1 — resolve each declaration to its target file (or a diagnostic).
    // A declaration that resolves cleanly becomes one candidate edge; we do NOT
    // commit it yet, because a second declaration may still resolve to the same
    // target and force a poison.
    struct CandidateEdge {
        parent: String,
        mod_name: String,
        cfg_test: bool,
    }
    let mut by_target: BTreeMap<String, Vec<CandidateEdge>> = BTreeMap::new();
    for f in &ordered {
        for decl in &f.mod_decls {
            match resolve_mod_target(&f.rel_path, decl, &file_set) {
                ResolveOutcome::NoCandidate => unresolved.push(UnresolvedInclusion {
                    declaring_file: f.rel_path.clone(),
                    mod_name: decl.name.clone(),
                    reason: "no_candidate_file",
                }),
                ResolveOutcome::Ambiguous => unresolved.push(UnresolvedInclusion {
                    declaring_file: f.rel_path.clone(),
                    mod_name: decl.name.clone(),
                    reason: "ambiguous_target",
                }),
                ResolveOutcome::Resolved(target) => {
                    by_target.entry(target).or_default().push(CandidateEdge {
                        parent: f.rel_path.clone(),
                        mod_name: decl.name.clone(),
                        cfg_test: decl.cfg_test,
                    });
                }
            }
        }
    }

    // Pass 2 — commit unambiguous edges; poison targets with >1 declaration.
    let mut edges: BTreeMap<String, ModEdge> = BTreeMap::new();
    let mut poisoned: BTreeSet<String> = BTreeSet::new();
    for (target, mut cands) in by_target {
        if cands.len() == 1 {
            let c = cands.pop().expect("len == 1");
            edges.insert(
                target,
                ModEdge {
                    parent: c.parent,
                    cfg_test: c.cfg_test,
                },
            );
        } else {
            // Multiple declarations compile the same file: ambiguous inclusion.
            // Never promote; diagnose each contributing declaration so every
            // declaring file learns its inclusion was not cleanly resolved.
            poisoned.insert(target);
            let distinct_parents: BTreeSet<&str> =
                cands.iter().map(|c| c.parent.as_str()).collect();
            let reason = if distinct_parents.len() > 1 {
                "multiple_parents"
            } else {
                "duplicate_declaration"
            };
            for c in cands {
                unresolved.push(UnresolvedInclusion {
                    declaring_file: c.parent,
                    mod_name: c.mod_name,
                    reason,
                });
            }
        }
    }

    let mut test_files = BTreeSet::new();
    for f in files {
        // Poisoned targets keep their existing classification (fail-closed).
        if !poisoned.contains(&f.rel_path) && is_cfg_test_reachable(&f.rel_path, &edges) {
            test_files.insert(f.rel_path.clone());
        }
    }

    unresolved.sort_by(|a, b| {
        (a.declaring_file.as_str(), a.mod_name.as_str(), a.reason).cmp(&(
            b.declaring_file.as_str(),
            b.mod_name.as_str(),
            b.reason,
        ))
    });

    ClassifyOutcome {
        test_files,
        unresolved,
    }
}

/// True iff `file`'s inclusion chain crosses a `#[cfg(test)]` gate on its way to
/// a crate root (no incoming edge). A chain that reaches a root with no cfg(test)
/// edge → not test. Cycle (impossible in valid Rust) → false, fail-closed.
fn is_cfg_test_reachable(file: &str, edges: &BTreeMap<String, ModEdge>) -> bool {
    let mut cur = file.to_string();
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(cur.clone()) {
            return false; // cycle — cannot prove test
        }
        match edges.get(&cur) {
            Some(e) if e.cfg_test => return true,
            Some(e) => cur = e.parent.clone(),
            None => return false, // reached a crate root, no cfg(test) on path
        }
    }
}

/// Outcome of resolving one `mod` declaration to the repo-relative file it
/// compiles. FAIL-CLOSED: only `Resolved` yields a promote edge; `Ambiguous`
/// and `NoCandidate` are diagnostics, never a guessed target.
enum ResolveOutcome {
    /// Exactly one candidate file exists — the target.
    Resolved(String),
    /// More than one conventional candidate exists (both `foo.rs` and
    /// `foo/mod.rs`) — real Rust rejects this too; we refuse to pick one.
    Ambiguous,
    /// No candidate file exists in the snapshot.
    NoCandidate,
}

/// Resolve the repo-relative FILE a `mod` declaration in `declaring` pulls in,
/// choosing the candidate that EXISTS in `files` (so we need not perfectly
/// re-derive Rust's `mod.rs`-vs-`foo.rs` rule — existence disambiguates).
///
/// The submodule DIRECTORY a declaration resolves in is the declaring file's own
/// dir for a root-like file (`mod.rs`/`lib.rs`/`main.rs`) or its `foo/` subdir for
/// a plain `foo.rs`, PLUS one directory segment per enclosing inline module
/// (`decl.inline_path`) — Rust nests inline-module names as directories, so
/// `mod scope { mod child; }` in `src/lib.rs` resolves `child` under `src/scope/`.
///
/// `#[path]` OVERRIDES the conventional lookup entirely (as in real Rust — the
/// conventional `foo.rs`/`foo/mod.rs` are not consulted when `#[path]` is
/// present), so a `#[path]` inclusion has exactly one candidate. A TOP-LEVEL
/// `#[path]` is relative to the declaring file's own directory; a `#[path]`
/// nested in inline modules is relative to the inline context directory (the
/// Rust reference's rule for path attributes inside inline blocks). For the
/// conventional case BOTH candidates are considered: if BOTH exist the target is
/// `Ambiguous` (fail-closed), if exactly one exists it is `Resolved`.
fn resolve_mod_target(
    declaring: &str,
    decl: &RustModDecl,
    files: &BTreeSet<String>,
) -> ResolveOutcome {
    let dir = parent_dir(declaring);
    let file_name = declaring.rsplit('/').next().unwrap_or(declaring);
    // Directory the declaring file's OWN submodules live in. `file_stem` carries
    // no `..`, so this join cannot escape; if it somehow did, fail closed.
    let base_dir = if matches!(file_name, "mod.rs" | "lib.rs" | "main.rs") {
        dir.clone()
    } else {
        match join_rel(&dir, &file_stem(declaring)) {
            Some(d) => d,
            None => return ResolveOutcome::NoCandidate,
        }
    };
    // Enclosing inline modules nest as directory segments on top of that.
    // Inline segments are module identifiers (no `..`); a None here is likewise
    // fail-closed.
    let mut context_dir = base_dir;
    for seg in &decl.inline_path {
        match join_rel(&context_dir, seg) {
            Some(d) => context_dir = d,
            None => return ResolveOutcome::NoCandidate,
        }
    }
    // Candidate target file(s). A `#[path]` override is the ONLY place a
    // user-authored `..` can appear; an override that escapes the repo root
    // yields `None` from `join_rel` → no in-repo candidate → NoCandidate. We
    // never normalize an escaping path back onto a same-tail in-repo file
    // (review-3). `flatten()` drops any `None`, so an escaping conventional
    // candidate is likewise excluded.
    let candidates: Vec<String> = if let Some(p) = &decl.path_override {
        // Top-level #[path] is relative to the file's dir; nested-in-inline
        // #[path] is relative to the inline context dir.
        let path_base = if decl.inline_path.is_empty() {
            &dir
        } else {
            &context_dir
        };
        join_rel(path_base, p).into_iter().collect()
    } else {
        [
            join_rel(&context_dir, &format!("{}.rs", decl.name)),
            join_rel(&context_dir, &format!("{}/mod.rs", decl.name)),
        ]
        .into_iter()
        .flatten()
        .collect()
    };
    let existing: Vec<String> = candidates
        .into_iter()
        .filter(|c| files.contains(c))
        .collect();
    match existing.len() {
        0 => ResolveOutcome::NoCandidate,
        1 => ResolveOutcome::Resolved(existing.into_iter().next().expect("len == 1")),
        _ => ResolveOutcome::Ambiguous,
    }
}

/// `/`-relative parent directory of a `/`-joined relpath (`""` for a top-level
/// file).
fn parent_dir(rel: &str) -> String {
    match rel.rfind('/') {
        Some(i) => rel[..i].to_string(),
        None => String::new(),
    }
}

/// File stem (final component without extension) of a `/`-joined relpath.
fn file_stem(rel: &str) -> String {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    match name.rfind('.') {
        Some(i) => name[..i].to_string(),
        None => name.to_string(),
    }
}

/// Join `rel` onto directory `dir`, lexically resolving `.`/`..`/empty segments
/// in `/`-relpath space (no filesystem access; platform-stable keys).
///
/// FAIL-CLOSED (review-3): returns `None` when a `..` segment would ascend ABOVE
/// the repo-relative root (a `..` with nothing left to pop). Such a target
/// escapes the repository, so it can never be a valid in-repo file. The prior
/// implementation `pop()`ed and silently DISCARDED the un-poppable `..`, which
/// clamped an escaping `#[path = "../../src/x.rs"]` back onto an in-repo
/// same-tail file (`src/x.rs`) and could promote it on a phantom inclusion —
/// exactly the false-fact the VISION's certainty model forbids. An escaping
/// target is no in-repo target: the caller maps `None` to `NoCandidate`.
fn join_rel(dir: &str, rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for seg in dir.split('/').chain(rel.split('/')) {
        match seg {
            "" | "." => {}
            ".." => {
                // Nothing to pop → the path ascends above repo root → escape.
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests;
