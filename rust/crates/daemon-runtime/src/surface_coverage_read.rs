//! ZEROSTATE-SCOPE-1 (§2.1/§2.2): the ONE per-repo HTTP-surface-detector coverage payload
//! rendered in the `surfaces list`, `boundaries list`, and `boundaries summary` zero-states.
//!
//! # Why this module
//!
//! Before this slice the `surfaces list` zero-state pasted a build-static "no HTTP detector
//! for Django URLconf routes" onto EVERY repo — so leveldb, a pure C++ repo, wore django's
//! sentence — and `boundaries list/summary` blamed the codebase ("No recognized boundary
//! patterns found in this codebase"). The fix (contract §2.1/§2.2): the gap clause is
//! PER-REPO — it names only the materially-present languages/frameworks of THIS repo the
//! HTTP surface detectors cannot see — and boundaries adopt the SAME coverage form from the
//! SAME source (no second roster).
//!
//! This function composes the two facts:
//! - BUILD-STATIC (from `repo_graph_repo_index::surface_coverage`): which detector families
//!   this build ships, which languages they cover, and the framework-specific gap name for a
//!   covered-language gap (Python → "Django URLconf routes").
//! - PER-REPO (from `reader_context::surface_uncovered_material_gaps` over the stored
//!   per-language file counts): this repo's materially-present code languages with no HTTP
//!   surface detector.
//!
//! # Honesty (STANDING HONESTY RULE 1)
//!
//! The per-language count read is fallible and CLASSIFIED (it decides which gaps are named),
//! so a failed read renders `material_gap.status = "unknown"` WITH the reason — NEVER a
//! silent empty gap (which the presenter would read as "full coverage", a false claim).
//! `http_detector_families` is build-static and infallible, so this function itself never
//! errors: it always returns a payload, degrading the per-repo arm to unknown-with-reason.
//!
//! # Wire additivity (review-1 item 1)
//!
//! The payload emits BOTH the tagged `material_gap` (known/unknown) AND the pre-slice flat
//! `named_uncovered` field, so the `surface_coverage` object stays byte-additive: consumers
//! that read `surface_coverage.named_uncovered` directly keep working, now with the CORRECTED
//! per-repo names instead of the build-static Django blob. Both arms come from the one read
//! below — no second roster, no second computation.
//!
//! # Abstraction one-liner
//!
//! `surface_coverage_json` — the read+assembly of the additive `surface_coverage` DTO;
//! three callers (`dispatch::handle_surfaces_list`, `boundaries_list_read`,
//! `boundaries_summary_read`); axis = none (composes the existing build-static accessors ×
//! the existing per-repo materiality gate); rejected simpler = duplicate the read+assembly
//! at each of the three call sites — rejected because it would re-spell the same fallible
//! read and honesty branch three times and let them drift.

use repo_graph_storage::StorageConnection;
use serde_json::{json, Value};

/// Build the additive `surface_coverage` payload for a snapshot (see the module docs).
/// Always returns a value; the per-repo gap arm degrades to unknown-with-reason on a read
/// failure rather than erroring or fabricating full coverage.
pub(crate) fn surface_coverage_json(storage: &StorageConnection, snapshot_uid: &str) -> Value {
    let http_detector_families =
        repo_graph_repo_index::surface_coverage::http_surface_detector_families();

    // Emit BOTH representations from the ONE per-repo computation:
    //  - `material_gap` (this slice): the tagged known/unknown arm the current renderer reads.
    //  - `named_uncovered` (LEGACY flat, MODULES-IDENTITY-2 wire): retained so the payload stays
    //    byte-additive for any consumer that reads `surface_coverage.named_uncovered` directly.
    //    It mirrors the KNOWN gap names; a failed read leaves it empty (the flat form has no
    //    unknown state — `material_gap` carries the failure). No second computation: both arms
    //    derive from the single `query_file_count_by_language` read below.
    let (material_gap, legacy_named_uncovered) =
        match repo_graph_agent::AgentStorageRead::query_file_count_by_language(
            storage,
            snapshot_uid,
        ) {
            Ok(counts) => {
                let names = crate::reader_context::surface_uncovered_material_gaps(
                    &counts,
                    repo_graph_repo_index::surface_coverage::http_surface_detection_covers,
                    repo_graph_repo_index::surface_coverage::http_surface_named_gap_for,
                );
                (
                    json!({ "status": "known", "named_uncovered": names.clone() }),
                    names,
                )
            }
            Err(e) => (
                json!({ "status": "unknown", "reason": e.to_string() }),
                Vec::<String>::new(),
            ),
        };

    json!({
        "http_detector_families": http_detector_families,
        "named_uncovered": legacy_named_uncovered,
        "material_gap": material_gap,
    })
}
