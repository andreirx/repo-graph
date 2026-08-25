//! Orphan `.vec` seed-sidecar reclaim (EMBED-SEED-IMPL-1 §4.1, class D).
//!
//! **Abstraction note (per repo structural guardrail):** extracted from `reclaim.rs`
//! (review-6 #2) so the new seed-reclaim MECHANISM does not grow the already-oversized
//! reclaim module. A CHILD module of `reclaim`: it reaches the parent's private
//! `FileEntry` / `ReclaimUnit` / `OrphanReport` / `stat_len_or_record` via parent
//! visibility and exposes `scan_orphan_seed_vectors` + `attach_orphan_seed_units` as
//! `pub(super)`. Two concrete current callers, both in `super::`: [`super::scan_orphans`]
//! populates `orphan_seed_vectors`; [`super::reclaim_units`] attaches them to per-`.db`
//! units. Axis of variation: none claimed — a cohesion/size split. The `OrphanReport`
//! FIELD + its aggregation methods, the `forget` deletion, and the boot-log line stay
//! woven in the parent (they are format/field wiring on an existing struct, not a
//! separable mechanism — moving them would fragment the struct's cohesion).

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::Path;

use super::{stat_len_or_record, FileEntry, OrphanReport, ReclaimUnit};

/// Scan `<state_root>/seed-vectors/` for orphan `.vec` sidecars — those whose base
/// `<hash16>.db` is not registry-`referenced`. A missing dir is NORMAL (no seeding yet);
/// any other listing/stat fault is folded into `scan_errors` (unknown-is-never-zero).
pub(super) fn scan_orphan_seed_vectors(
    db_dir: &Path,
    referenced: &BTreeSet<OsString>,
    scan_errors: &mut Vec<String>,
) -> Vec<FileEntry> {
    let mut orphan_seed_vectors: Vec<FileEntry> = Vec::new();
    if let Some(seed_dir) = db_dir.parent().map(|p| p.join("seed-vectors")) {
        match fs::read_dir(&seed_dir) {
            Ok(read) => {
                for entry in read {
                    let dirent = match entry {
                        Ok(d) => d,
                        Err(e) => {
                            scan_errors.push(e.to_string());
                            continue;
                        }
                    };
                    let path = dirent.path();
                    let name = match path.file_name() {
                        Some(n) => n.to_string_lossy().into_owned(),
                        None => continue,
                    };
                    if !name.ends_with(".vec") {
                        continue; // not our sidecar (e.g. a leftover .tmp) → not our concern
                    }
                    match vec_base_db_name(&path) {
                        // Orphan iff its base `<hash16>.db` is not registry-referenced.
                        Some(base) if !referenced.contains(&base) => {
                            let bytes = stat_len_or_record(&path, scan_errors);
                            orphan_seed_vectors.push(FileEntry { path, bytes });
                        }
                        // Base referenced (live repo) → leave it alone.
                        Some(_) => {}
                        // No derivable base → conservatively leave it (do not reclaim).
                        None => {}
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => { /* no seeding yet — clean */ }
            Err(e) => scan_errors.push(format!("cannot list {}: {e}", seed_dir.display())),
        }
    }
    orphan_seed_vectors
}

/// Attach each orphan `.vec` to the [`ReclaimUnit`] for its base `<hash16>.db`, so it
/// is unlinked under the SAME DB write-slot guard + registry recheck as its DB. A
/// vec-only base (no orphan-DB / stray unit) gets its own unit (base `.db` absent).
pub(super) fn attach_orphan_seed_units(
    units: &mut Vec<ReclaimUnit>,
    scan: &OrphanReport,
    db_dir: &Path,
) {
    for v in &scan.orphan_seed_vectors {
        let base_name = match vec_base_db_name(&v.path) {
            Some(b) => b,
            None => continue,
        };
        if let Some(u) = units.iter_mut().find(|u| u.base_name == base_name) {
            u.files
                .push(("orphan-seed-vector", v.path.clone(), v.bytes));
        } else {
            let base_db = db_dir.join(&base_name);
            units.push(ReclaimUnit {
                base_db,
                base_name,
                files: vec![("orphan-seed-vector", v.path.clone(), v.bytes)],
            });
        }
    }
}

/// The base `<hash16>.db` name for a `.vec` seed sidecar (`<hash16>.vec`), so an
/// orphan `.vec` is classified/guarded against the SAME registry key + DB write
/// slot as its (possibly-gone) snapshot DB — never reclaimed while a re-index owns
/// that hash.
fn vec_base_db_name(path: &Path) -> Option<OsString> {
    let stem = path.file_stem()?; // "<hash16>" from "<hash16>.vec"
    let mut name = stem.to_os_string();
    name.push(".db");
    Some(name)
}
