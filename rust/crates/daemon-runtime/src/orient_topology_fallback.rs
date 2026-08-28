//! ORIENT-SEGMENT-2 §2.1 — directory-group topology FALLBACK for `orient`.
//!
//! When a repo's manifest-folded PACKAGE-GROUP view collapses to a single
//! dominant group ("1 package group: ." — django), the package topology gives an
//! agent zero fan-out to orient by, while `stats` on the SAME snapshot already
//! holds the correct directory-group fan-in ranking (django `django/db 340`,
//! `django/test 242`, `django/core 195`). This module promotes that view.
//!
//! Locus (operator ruling 2026-08-28, Option A — answers review-0's
//! `seg2-directory-group-source`): the daemon injects the EXISTING `stats`
//! directory-group read into an ADDITIVE orient response field, populated ONLY
//! when the collapse trigger fires and rendered by rgr only then. A NON-collapsed
//! orient stays BYTE-IDENTICAL (leveldb's gold-standard output protected by
//! construction). Nothing is recomputed differently — same facts, better choice.
//!
//! Abstraction record (crate-private module, pre-ratified under the packet's
//! 500-line guardrail carve-out):
//!   - what: the collapse predicate + the directory-group fallback assembly.
//!   - concrete current users: `dispatch::handle_orient` (sole caller — producer).
//!   - axis of variation: topology-collapse fallback data (a collapse-only read).
//!   - rejected alternative: a new public `AgentStorageRead` port method (Option B
//!     — a wider cross-boundary contract for a collapse-only, daemon-owned need)
//!     and inlining into the 5k-line `dispatch.rs` (500-line guardrail).

use repo_graph_daemon_transport::ProgressEmitter;
use serde::Serialize;
use serde_json::Value;

use crate::livegraph_feed::{cancellable_module_stats, SqlStats};
use crate::state::RepoState;

/// A repo's package-group view "collapses" (§2.1) when the manifest fold yielded
/// ONE bucket that swamps the view — detected by EITHER of two INDEPENDENT triggers,
/// the slice's literal OR rule: "one group covering ≥90% of files, OR ≥N files in a
/// single group".
///
/// - **Trigger 1 — relative dominance (`>= 90%`)**: the largest group owns `>=
///   COLLAPSE_DOMINANCE_RATIO` of the files (django "1 package group: ." is the
///   100% subset). 90% (not strict 100%) so a sole real package beside a tiny stray
///   directory still promotes. Materiality floor `COLLAPSE_MIN_FILES`: on a
///   handful-of-files repo the directory view is noise, so trigger 1 needs a tree
///   large enough to hold internal structure.
/// - **Trigger 2 — absolute monolith (`>= N` files in one group)**: the largest
///   group ALONE holds `>= COLLAPSE_SINGLE_GROUP_FILES` files — an undifferentiated
///   bucket the manifest fold failed to break down. Per OPERATOR RULING 2
///   (2026-08-28) this is the SPEC's literal `>= N` trigger with NO share gate: a
///   ≥500-file single group collapses regardless of the surrounding ratio, so it
///   fires INDEPENDENTLY of trigger 1 across the whole `[0, 90%)` dominance band.
///
/// `N = 500` (`COLLAPSE_SINGLE_GROUP_FILES`): a single package group of ≥500 files is
/// a monolith worth promoting the directory view for; ≥500 is ~11× leveldb's largest
/// real group (44 — OBSERVED on the indexed gold-standard snapshot), so leveldb and
/// every healthy small/medium multi-group repo stay byte-identical by construction.
const COLLAPSE_DOMINANCE_RATIO: f64 = 0.90;
const COLLAPSE_MIN_FILES: u64 = 50;
const COLLAPSE_SINGLE_GROUP_FILES: u64 = 500;

/// How many directory groups the fallback promotes (top-by-fan-in). Bounded — the
/// promoted view is an orientation headline, not a dump; the omitted count rides an
/// honest "+N more" line (-> `rmap stats`).
const FALLBACK_TOP_N: usize = 12;

/// The core collapse predicate on the two counts the triggers need — `total` files
/// and the `largest` package group's file count. Single-sourced so the typed and the
/// serialized-output entry points cannot diverge on the thresholds.
fn is_collapsed_counts(total: u64, largest: u64) -> bool {
    if largest == 0 || total == 0 {
        return false;
    }
    let share = largest as f64 / total as f64;

    // Trigger 1: relative dominance, above the materiality floor.
    if total >= COLLAPSE_MIN_FILES && share >= COLLAPSE_DOMINANCE_RATIO {
        return true;
    }
    // Trigger 2: absolute monolith — the spec's literal `>= N` trigger, no share gate
    // (operator ruling 2). Fires independently of trigger 1.
    if largest >= COLLAPSE_SINGLE_GROUP_FILES {
        return true;
    }
    false
}

/// Does this repo's manifest-folded package-group view collapse to one dominant
/// group? Reads the SAME `MODULE_SUMMARY` evidence the headline is built from off the
/// SERIALIZED orient envelope (`value.signals[].value.evidence` — `file_count` +
/// `package_groups`), so no `OrientResult` need be held past the envelope move. `false`
/// when the repo carries no module-summary signal (a file/path/symbol focus, or no
/// indexed files) — the fallback is a repo-topology concern only. A missing/malformed
/// count reads as absent (not collapsed), never a fabricated trigger.
pub(crate) fn detect_collapse(output: &Value) -> bool {
    let Some(ev) = module_summary_evidence(output) else {
        return false;
    };
    // `file_count` is REQUIRED to classify. Absent/malformed → UNKNOWN → the
    // conservative direction is NOT to collapse (leaves the pre-slice manifest
    // rendering; standing honesty rule #1 — no fabricated trigger from a fallible read).
    let Some(total) = ev.get("file_count").and_then(Value::as_u64) else {
        return false;
    };
    // `package_groups` must be a PRESENT array to reason about topology. Absent or
    // malformed → UNKNOWN → do not collapse. (No `.flatten()`/`.unwrap_or(0)`: a
    // missing array must never silently read as `largest = 0`, which would be a
    // classification from absent data.)
    let Some(groups) = ev.get("package_groups").and_then(Value::as_array) else {
        return false;
    };
    // EVERY package-group row must carry a readable `file_count` before either trigger
    // may fire (OPERATOR RULING 2, 2026-08-28 — NINTH recurrence of the honesty class).
    // A `filter_map().max()` would silently DROP a malformed row and collapse from the
    // surviving valid ones — a classification from PARTIALLY-absent data. A single
    // unreadable row makes the fold's size distribution UNKNOWN, and the conservative
    // direction on unknown is NOT to collapse (leaves the pre-slice manifest rendering;
    // standing honesty rule #1 — never a fabricated trigger from a fallible read). An
    // empty array (no rows) leaves `largest = 0`, which `is_collapsed_counts` reads as
    // not collapsed.
    let mut largest = 0u64;
    for g in groups {
        let Some(fc) = g.get("file_count").and_then(Value::as_u64) else {
            return false;
        };
        largest = largest.max(fc);
    }
    is_collapsed_counts(total, largest)
}

/// The MODULE_SUMMARY signal's `evidence` object on the serialized orient envelope.
fn module_summary_evidence(output: &Value) -> Option<&Value> {
    output
        .get("value")?
        .get("signals")?
        .as_array()?
        .iter()
        .filter_map(|leaf| leaf.get("value"))
        .find(|s| s.get("code").and_then(Value::as_str) == Some("MODULE_SUMMARY"))?
        .get("evidence")
}

/// One promoted directory group (a `stats` per-directory row): its path, import
/// fan-in / fan-out, and owned-file count. Raw DTO data (operator ruling: "raw DTO
/// data only") — no derived posture.
#[derive(Debug, Serialize)]
pub(crate) struct DirGroupRow {
    pub name: String,
    pub fan_in: i64,
    pub fan_out: i64,
    pub file_count: i64,
}

/// The ADDITIVE orient field the daemon injects on collapse. Exactly one of
/// `groups` (the read succeeded) or `unavailable` (the read failed — honest
/// unknown-with-reason, never a silent empty; standing honesty rule #1) is
/// populated. `total` is the COMPLETE directory-group count (>= `groups.len()`)
/// so the omission line rgr renders stays TRUE.
#[derive(Debug, Serialize)]
pub(crate) struct DirectoryGroupFallback {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<DirGroupRow>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<String>,
}

impl DirectoryGroupFallback {
    fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            groups: None,
            total: None,
            unavailable: Some(reason.into()),
        }
    }
}

/// Assemble the directory-group fallback: run the SAME `stats` per-directory read
/// (`cancellable_module_stats`, the shared chokepoint) on a fresh connection, rank
/// by fan-in DESC (ties broken by name for determinism), and keep the top
/// [`FALLBACK_TOP_N`]. A read failure / mid-request cancel is surfaced as
/// unknown-with-reason — never a fabricated empty view.
pub(crate) fn build(
    repo_state: &RepoState,
    emitter: &mut dyn ProgressEmitter,
    snapshot_uid: &str,
) -> DirectoryGroupFallback {
    let conn = match repo_state.storage() {
        Ok(c) => c,
        Err(e) => return DirectoryGroupFallback::unavailable(e),
    };
    let mut rows = match cancellable_module_stats(emitter, conn, snapshot_uid) {
        Ok(SqlStats::Stats(rows)) => rows,
        Ok(SqlStats::Cancelled) => {
            return DirectoryGroupFallback::unavailable(
                "directory-group read cancelled (client disconnected)",
            )
        }
        Err(e) => return DirectoryGroupFallback::unavailable(e.to_string()),
    };

    let total = rows.len();
    // Rank by fan-in DESC, then name ASC — a TOTAL order, so the top-N cut is a
    // pure function of the SET, not of SQL row order.
    rows.sort_by(|a, b| {
        b.fan_in
            .cmp(&a.fan_in)
            .then_with(|| a.module.cmp(&b.module))
    });
    let groups: Vec<DirGroupRow> = rows
        .into_iter()
        .take(FALLBACK_TOP_N)
        .map(|r| DirGroupRow {
            name: r.module,
            fan_in: r.fan_in,
            fan_out: r.fan_out,
            file_count: r.file_count,
        })
        .collect();

    DirectoryGroupFallback {
        groups: Some(groups),
        total: Some(total),
        unavailable: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// `(total_files, largest_group_files)` → collapse verdict. `groups` is the raw
    /// per-group file counts; `largest` is their max (the fold is size-DESC, but we
    /// take the max defensively — mirroring `detect_collapse`).
    fn collapses(total: u64, groups: &[u64]) -> bool {
        let largest = groups.iter().copied().max().unwrap_or(0);
        is_collapsed_counts(total, largest)
    }

    #[test]
    fn single_dominant_group_collapses() {
        // django "1 package group: ." — one group owning ~everything.
        assert!(collapses(2500, &[2500]));
    }

    #[test]
    fn dominant_group_beside_fringe_collapses() {
        // >= 90% dominance with a tiny stray directory beside it.
        assert!(collapses(1000, &[950, 50]));
    }

    #[test]
    fn healthy_multi_group_does_not_collapse() {
        // Real leveldb shape (OBSERVED on the indexed gold-standard snapshot): 133
        // files across 8 groups, largest `db`=44 (33% share). Neither trigger fires
        // (33% < 90%; 44 < 500) — the gold-standard byte-identity guard.
        assert!(!collapses(133, &[44, 42, 18, 15]));
    }

    #[test]
    fn tiny_repo_below_floor_does_not_collapse() {
        // A 10-file single-group repo is below the materiality floor — the
        // directory view would be noise. (trigger 1 floor; 10 < 500 for trigger 2.)
        assert!(!collapses(10, &[10]));
    }

    #[test]
    fn exactly_ninety_percent_at_floor_collapses() {
        // Boundary: 90% of exactly the file floor (trigger 1).
        assert!(collapses(50, &[45, 5]));
    }

    #[test]
    fn absolute_monolith_below_ninety_percent_collapses_independently() {
        // Trigger 2 in isolation: an 800-file group at 80% share — BELOW trigger 1's
        // 90% dominance, so ONLY the absolute-monolith trigger fires (largest 800 >=
        // 500). This is the independent, share-gate-free >= N trigger.
        assert!(collapses(1000, &[800, 200]));
    }

    #[test]
    fn large_group_collapses_regardless_of_share() {
        // Operator ruling 2: the >= N trigger has NO share gate. Two comparably-large
        // groups (600 each, 50% share) — trigger 1 (90%) does not fire, but each group
        // is >= 500 so trigger 2 collapses the view. The directory-group fan-in read is
        // still a true Layer-1 fact; the spec's OR rule promotes it here.
        assert!(collapses(1200, &[600, 600]));
    }

    #[test]
    fn below_absolute_floor_and_below_dominance_does_not_collapse() {
        // A 400-file group at 80% share: 400 < 500 (absolute floor) and 80% < 90%
        // (dominance) — neither trigger fires.
        assert!(!collapses(500, &[400, 100]));
    }

    #[test]
    fn detect_collapse_reads_serialized_envelope() {
        // End-to-end wire shape: the MODULE_SUMMARY evidence off `value.signals` drives
        // the verdict (django-shape: one `.` group at ~100%).
        let output = json!({
            "value": { "signals": [
                { "value": { "code": "OTHER", "evidence": {} } },
                { "value": { "code": "MODULE_SUMMARY", "evidence": {
                    "file_count": 3019,
                    "package_groups": [{"name": ".", "file_count": 3014}]
                } } }
            ] }
        });
        assert!(detect_collapse(&output));
    }

    #[test]
    fn detect_collapse_false_when_package_groups_absent() {
        // Honesty guard (operator ruling 2, 2026-08-28): a MODULE_SUMMARY with a known
        // file_count but NO package_groups array is UNKNOWN topology → do NOT collapse
        // (conservative direction = pre-slice manifest rendering). Never a fabricated
        // `largest = 0` trigger from an absent read.
        let output = json!({
            "value": { "signals": [
                { "value": { "code": "MODULE_SUMMARY", "evidence": { "file_count": 3019 } } }
            ] }
        });
        assert!(!detect_collapse(&output));
    }

    #[test]
    fn detect_collapse_false_when_package_groups_malformed() {
        // A present package_groups array whose entries carry NO readable file_count is
        // unreadable → UNKNOWN → do NOT collapse (never a fabricated trigger).
        let output = json!({
            "value": { "signals": [
                { "value": { "code": "MODULE_SUMMARY", "evidence": {
                    "file_count": 3019,
                    "package_groups": [{"name": "."}]
                } } }
            ] }
        });
        assert!(!detect_collapse(&output));
    }

    #[test]
    fn detect_collapse_false_when_any_group_row_malformed() {
        // MIXED array (review-4 §1): one well-formed DOMINANT row AND one row with NO
        // readable `file_count`. `filter_map().max()` would drop the malformed row and
        // collapse from the survivor (the `.` group at ~100%); the row-complete rule
        // refuses — a malformed MODULE_SUMMARY must NEVER trigger a collapse (operator
        // ruling 2). UNKNOWN fold → do NOT collapse.
        let output = json!({
            "value": { "signals": [
                { "value": { "code": "MODULE_SUMMARY", "evidence": {
                    "file_count": 3019,
                    "package_groups": [
                        {"name": ".", "file_count": 3014},
                        {"name": "vendor"}
                    ]
                } } }
            ] }
        });
        assert!(!detect_collapse(&output));
    }

    #[test]
    fn detect_collapse_false_without_module_summary() {
        // A file/symbol-focus orient carries no MODULE_SUMMARY → never collapses.
        let output = json!({ "value": { "signals": [
            { "value": { "code": "HIGH_COMPLEXITY", "evidence": {} } }
        ] } });
        assert!(!detect_collapse(&output));
    }
}
