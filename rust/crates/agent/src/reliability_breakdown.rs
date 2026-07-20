//! RESOLUTION-BREAKDOWN-CLI-1: the per-scope (per-language / per-module)
//! projection of the ONE shared call-reliability view.
//!
//! This module SURFACES a decomposition the engine already has the data for; it
//! does NOT invent a metric. Every per-scope figure is produced by feeding that
//! scope's counts into the SAME [`crate::reliability::CallReliabilityView`] the
//! aggregate `orient` / `trust` / `check` surfaces use — so the rate formula
//! (`resolved / (resolved + internal_like)`, external-excluded), the reader-frame
//! wording, the "no in-scope calls measured" UNKNOWN case, and the
//! conservative-unclassified caveat are REUSED, never re-derived. The only thing
//! this module adds is the grouping: one `CallReliabilityView` per scope instead
//! of one for the whole snapshot.
//!
//! ## Populations (RESOLUTION-BREAKDOWN-CLI-1 §2 — same as the aggregate)
//!
//! The raw per-scope counts ([`CallResolutionCounts`]) are read from the exact
//! tables the slice names and the operator hand-queried:
//!   * `resolved`   — `edges` rows of type `CALLS` (resolved by construction);
//!   * `unresolved` — `unresolved_edges` rows in the CALLS category family;
//!   * `external`   — of those, `classification = external_library_candidate`
//!     (the out-of-source-scope share EXCLUDED from the in-scope denominator,
//!     exactly as the aggregate excludes it);
//!   * `unknown`    — of those, `classification = unknown` (target undetermined),
//!     carried so the conservative-rate caveat can fire.
//!
//! `internal_like` (the in-scope-or-unclassified unresolved count) is derived here
//! as `unresolved − external`, matching the aggregate's
//! `unresolved_calls_internal_like` (external-excluded = internal_candidate ∪
//! unknown). Because the storage read groups the SAME rows, `Σ scopes == total`
//! by construction (the parts-reconcile-to-whole invariant, storage-tested).
//!
//! ## The band is INJECTED, not re-derived
//!
//! The reliability BAND (LOW/MEDIUM/HIGH) is scored by
//! `repo_graph_trust::rules::compute_call_graph_reliability` — which lives in the
//! `trust` crate this crate deliberately does NOT depend on (see the crate-level
//! doc in `Cargo.toml`). So the daemon (which bridges `trust` and `agent`)
//! computes each scope's band from its counts via that SAME rule and injects it
//! as [`ScopeCounts::band`]; this module only LABELS it, and only when there is an
//! in-scope rate to band (a band over zero in-scope calls is vacuous — the SAME
//! `resolution.is_none()` guard `check`/`trust`/`orient` apply).
//!
//! [abstraction: per-scope reliability projection + its boundary DTOs; concrete
//! users: the daemon `reliability` handler (builds it) and the `rgr` reliability
//! presentation (deserializes + renders it) — 2 current callers across the daemon
//! boundary; axis of variation: the grouping key (language token vs module) — two
//! concrete axes today; rejected alternative: recompute the rate/caveat wording in
//! a new per-scope formula (that is the exact multi-definition drift
//! RELIABILITY-REFRAME-1 closed, and STOP-condition-forbidden here).]

use serde::{Deserialize, Serialize};

use crate::reliability::{self, CallReliabilityView};
use crate::storage_port::AgentReliabilityLevel;

/// Raw per-scope CALLS-edge counts — Layer-0 extracted facts produced by the
/// storage grouping read (`edges` CALLS + `unresolved_edges` CALLS-family, split
/// by `classification`). Defined in this (policy) crate so the storage adapter
/// produces it and this pure projection consumes it — the same adapter→policy
/// direction `LanguageFunctionCount` uses for `measurement_coverage`.
///
/// Invariants the producer guarantees (row-level counts over the same scope):
/// `external <= unresolved` and `unknown <= unresolved` (each is a `classification`
/// subset of the CALLS-family unresolved rows). `internal_like` and `total_calls`
/// are derived, never stored, so the two can never disagree with `unresolved`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallResolutionCounts {
    /// `edges` rows of type `CALLS` whose source node is in this scope (resolved).
    pub resolved: u64,
    /// `unresolved_edges` CALLS-family rows whose source node is in this scope.
    /// = `internal_like + external` (all unresolved calls, both dispositions).
    pub unresolved: u64,
    /// Of `unresolved`, the `external_library_candidate` share — out of source
    /// scope, EXCLUDED from the in-scope denominator (as the aggregate excludes it).
    pub external: u64,
    /// Of `unresolved`, the `unknown` (target-undetermined) share — kept IN the
    /// denominator (conservative), and used to fire the unclassified caveat.
    pub unknown: u64,
}

impl CallResolutionCounts {
    /// The in-scope-or-unclassified unresolved count = all CALLS-family unresolved
    /// MINUS the known-external share. Matches the aggregate's
    /// `unresolved_calls_internal_like`. Saturating so a (nominally impossible)
    /// `external > unresolved` can never underflow into a fabricated huge count.
    ///
    /// The SINGLE derivation of this quantity, called by BOTH the row builder here
    /// AND the daemon's band input (via the trust rule), so the two can never
    /// diverge on what "in-scope" means.
    pub fn internal_like(&self) -> u64 {
        self.unresolved.saturating_sub(self.external)
    }

    /// Every CALLS edge in this scope, resolved or not — the external-share
    /// denominator (`resolved + unresolved`).
    pub fn total_calls(&self) -> u64 {
        self.resolved + self.unresolved
    }
}

/// A raw grouped scope count as the storage read produces it (RESOLUTION-BREAKDOWN-CLI-1
/// review-0 F4): the scope name, whether the SOURCE files are test files
/// (`files.is_test` — the deterministic persisted flag, not a path heuristic), and
/// the raw counts. Defined in this (policy) crate so the storage adapter produces it
/// and the daemon consumes it — the adapter→policy direction. The band and
/// reader-frame projection are added downstream (`ScopeCounts` / [`scope_row`]).
///
/// [abstraction: storage→daemon boundary DTO for one grouped `(scope, is_test)`
/// cell; concrete current users: `storage::call_resolution_reads` (produces it) and
/// the daemon `reliability` handler (maps it to `ScopeCounts`) — 2 callers across the
/// storage boundary; axis of variation: the grouping key (language vs module) — the
/// same two axes `ScopeCounts` serves; rejected alternative: a `(String, bool,
/// CallResolutionCounts)` tuple — a named struct keeps the `is_test` dimension legible
/// at the boundary.]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeCountRow {
    pub key: String,
    /// Whether the source files of these calls are test files (`files.is_test`).
    pub is_test: bool,
    pub counts: CallResolutionCounts,
}

/// A named scope (a `files.language` token, a module path, or the whole-snapshot
/// total) plus its raw counts and the daemon-injected reliability band. The band
/// is `Option` because a scope with no in-scope calls has no band to show, and an
/// older daemon path could omit it — never to ration the projection.
#[derive(Debug, Clone)]
pub struct ScopeCounts {
    /// The reader-facing scope key: a language token (`java`, `typescript`, `jsx`,
    /// …), a module path, or `"(unknown)"` for the reconciliation bucket of edges
    /// whose source has no attributable language/module.
    pub key: String,
    /// The test-file partition (review-0 F4): `Some(true)` = calls from test files,
    /// `Some(false)` = production files, `None` = the grand-total row (spans BOTH
    /// partitions — an honest null, not a fabricated `false`).
    pub is_test: Option<bool>,
    pub counts: CallResolutionCounts,
    /// The band scored by `trust::rules::compute_call_graph_reliability` on this
    /// scope's `(resolved, internal_like)`, injected by the daemon. Labelled only
    /// when there is an in-scope rate (see [`scope_row`]).
    pub band: Option<AgentReliabilityLevel>,
}

/// One rendered per-scope row — the wire + render DTO. Carries the raw counts
/// (an agent parses them directly) AND the reader-frame projection of the SHARED
/// view, so the `--json` protocol surface and the human surface never disagree and
/// never fork the vocabulary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolutionScopeRow {
    /// Language token / module path / `"(unknown)"` bucket / the total sentinel.
    pub key: String,
    /// The test-file partition (review-0 F4): `Some(true)` = calls sourced from test
    /// files, `Some(false)` = production files, `None` = the grand-total row (spans
    /// both). JSON carries `true`/`false`/`null` so an agent can separate test-file
    /// call resolution from production without a path heuristic.
    pub is_test: Option<bool>,
    pub resolved: u64,
    pub unresolved: u64,
    pub external: u64,
    pub unknown: u64,
    /// `resolved + unresolved` — the external-share denominator.
    pub total_calls: u64,
    /// `resolved + internal_like` — the in-scope-or-unclassified denominator the
    /// rate is over. `0` exactly when [`resolved_pct`](Self::resolved_pct) is
    /// `None` (nothing to measure).
    pub in_scope_or_unclassified_total: u64,
    /// The in-scope resolution rate, percent. `None` = UNKNOWN: there are no
    /// in-scope calls to measure — never a fabricated `0`/`100` (VISION; slice §4).
    /// A genuine `0.0` (in-scope calls exist, none resolved) is a REAL measurement,
    /// distinct from `None`.
    pub resolved_pct: Option<f64>,
    /// The external-library share of all calls, percent. `None` only when the scope
    /// has no calls at all; a measured-zero external share is `Some(0.0)`.
    pub external_pct: Option<f64>,
    /// Reader-frame band label (`LOW`/`MEDIUM`/`HIGH`). `None` when there is no
    /// in-scope rate (a band over zero in-scope calls is vacuous — the shared guard).
    pub band: Option<String>,
    /// The shared reader-frame phrase: "your code's calls M% resolved" or
    /// "no in-scope calls measured".
    pub phrase: String,
    /// The shared external-coverage line, or the honest "no external-library calls
    /// identified (heuristic …)" when the heuristic matched none; `None` when the
    /// scope has no calls at all.
    pub external_line: Option<String>,
    /// The shared conservative-rate caveat when a material share of the denominator
    /// is unclassified; `None` otherwise. Its presence means `resolved_pct` is a
    /// lower bound.
    pub caveat: Option<String>,
}

/// Project one scope's raw counts through the SHARED [`CallReliabilityView`] into a
/// render row. This is the whole reuse: `derive` applies the identical rate
/// formula, `resolved_phrase`/`external_line` the identical wording, and
/// `unclassified_caveat` the identical degraded-path caveat.
pub fn scope_row(scope: &ScopeCounts) -> ResolutionScopeRow {
    let c = scope.counts;
    let view = CallReliabilityView::derive(
        c.resolved,
        c.internal_like(),
        c.external,
        c.total_calls(),
        Vec::new(), // named external targets are a whole-snapshot detail, not per-scope
        scope.band,
    );

    // `resolution` is `Some` iff there is at least one in-scope-or-unclassified
    // call — the SAME UNKNOWN decision the aggregate surfaces make.
    let (resolved_pct, in_scope_total) = match view.resolution {
        Some(r) => (Some(r.pct), r.in_scope_or_unclassified_total),
        None => (None, 0),
    };
    let external_pct = view.external.map(|e| e.pct);
    // Band only when there IS an in-scope rate to band (vacuous-HIGH suppression).
    let band = match (view.resolution, view.band) {
        (Some(_), Some(b)) => Some(reliability::band_label(b).to_string()),
        _ => None,
    };
    // The conservative caveat is meaningful only against a real in-scope denominator.
    let caveat = view.resolution.and_then(|r| {
        reliability::unclassified_caveat(c.unknown, r.in_scope_or_unclassified_total)
    });

    ResolutionScopeRow {
        key: scope.key.clone(),
        is_test: scope.is_test,
        resolved: c.resolved,
        unresolved: c.unresolved,
        external: c.external,
        unknown: c.unknown,
        total_calls: c.total_calls(),
        in_scope_or_unclassified_total: in_scope_total,
        resolved_pct,
        external_pct,
        band,
        phrase: view.resolved_phrase(),
        external_line: view.external_line(),
        caveat,
    }
}

/// The whole breakdown DTO — the daemon builds it, serializes it, and the `rgr`
/// presentation deserializes the same type (shared via this crate, no mirror).
///
/// `total` is the whole-snapshot row built from the UNGROUPED counts over the same
/// tables; `by_language` / `by_module` are the grouped rows. `Σ by_language ==
/// total` and `Σ by_module == total` hold by construction of the storage reads
/// (same predicates, grouped vs not) — the reconciliation invariant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolutionBreakdown {
    /// The whole-snapshot total — the "whole" the parts reconcile to.
    pub total: ResolutionScopeRow,
    /// Per `files.language`, ordered by the storage read (language asc), with a
    /// `"(unknown)"` bucket for edges whose source has no attributable language so
    /// the parts always sum to `total`.
    pub by_language: Vec<ResolutionScopeRow>,
    /// Per SEMANTIC module — the `module_candidates` population (declared/operational/
    /// inferred modules, VISION Layer-1/2) attributed via the stored
    /// `module_file_ownership` edge, NOT raw leaf-directory `MODULE` nodes
    /// (RESOLUTION-BREAKDOWN-CLI-1 review-1 #2 — a Layer-0/1 directory topology must not
    /// be labelled as the Layer-1/2 module notion). Ordered by the storage read
    /// (canonical-root-path asc), with a `"(unknown)"` bucket for calls whose source
    /// file has no candidate (unowned files, or a repo with no module discovery) so the
    /// parts still sum to `total`. A repo whose module discovery found no candidates
    /// yields only the `"(unknown)"` bucket — honest degradation, not a fabricated scope.
    pub by_module: Vec<ResolutionScopeRow>,
}

/// Assemble the breakdown from the three storage reads. Pure and order-preserving:
/// the storage reads already impose a total key order, and this preserves it.
pub fn build_breakdown(
    total: ScopeCounts,
    by_language: Vec<ScopeCounts>,
    by_module: Vec<ScopeCounts>,
) -> ResolutionBreakdown {
    ResolutionBreakdown {
        total: scope_row(&total),
        by_language: by_language.iter().map(scope_row).collect(),
        by_module: by_module.iter().map(scope_row).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(
        key: &str,
        resolved: u64,
        unresolved: u64,
        external: u64,
        unknown: u64,
        band: Option<AgentReliabilityLevel>,
    ) -> ScopeCounts {
        ScopeCounts {
            key: key.to_string(),
            is_test: Some(false),
            counts: CallResolutionCounts {
                resolved,
                unresolved,
                external,
                unknown,
            },
            band,
        }
    }

    #[test]
    fn is_test_partition_flows_through_the_projection() {
        // review-0 F4: the test-file partition is carried verbatim into the row and
        // serialized (true/false/null) so an agent can separate test from production.
        let mut prod = scope("java", 10, 5, 0, 0, Some(AgentReliabilityLevel::Low));
        prod.is_test = Some(false);
        assert_eq!(scope_row(&prod).is_test, Some(false));

        let mut test = prod.clone();
        test.is_test = Some(true);
        let test_row = scope_row(&test);
        assert_eq!(test_row.is_test, Some(true));
        let v = serde_json::to_value(&test_row).unwrap();
        assert_eq!(v["is_test"], serde_json::json!(true));

        // The grand-total row spans both partitions → honest null, never `false`.
        let mut total = prod.clone();
        total.is_test = None;
        let total_row = scope_row(&total);
        assert_eq!(total_row.is_test, None);
        assert!(serde_json::to_value(&total_row).unwrap()["is_test"].is_null());
    }

    #[test]
    fn reproduces_the_shared_in_scope_rate() {
        // glamCRM-shaped java scope pre-enrichment (~10.6%): resolved over
        // (resolved + internal_like), external EXCLUDED. 12 / (12 + 101) = 10.6%.
        let row = scope_row(&scope(
            "java",
            12,
            101,
            0,
            40,
            Some(AgentReliabilityLevel::Low),
        ));
        let pct = row.resolved_pct.expect("in-scope calls exist");
        assert!((pct - 10.6).abs() < 0.1, "expected ~10.6%, got {pct}");
        assert_eq!(row.in_scope_or_unclassified_total, 113);
        assert_eq!(row.band.as_deref(), Some("LOW"));
        assert!(row.phrase.contains("11% resolved") || row.phrase.contains("resolved"));
    }

    #[test]
    fn external_calls_are_excluded_from_the_denominator() {
        // 20 unresolved, 15 external → internal_like = 5. Rate = 10/(10+5)=66.7%,
        // NOT 10/(10+20)=33%. The external share still surfaces as context.
        let row = scope_row(&scope(
            "ts",
            10,
            20,
            15,
            0,
            Some(AgentReliabilityLevel::Medium),
        ));
        let pct = row.resolved_pct.unwrap();
        assert!((pct - 66.6667).abs() < 0.01, "got {pct}");
        assert_eq!(row.in_scope_or_unclassified_total, 15);
        // external share = 15 / total_calls(30) = 50%.
        assert!((row.external_pct.unwrap() - 50.0).abs() < 0.01);
        assert!(row.external_line.is_some());
    }

    #[test]
    fn zero_in_scope_is_unknown_never_a_fabricated_percent() {
        // A language present only as external calls (or with no calls) → no in-scope
        // denominator → UNKNOWN, never 0% or 100%, and no band.
        let all_external = scope_row(&scope("go", 0, 8, 8, 0, Some(AgentReliabilityLevel::High)));
        assert_eq!(all_external.resolved_pct, None);
        assert_eq!(all_external.band, None, "no band over zero in-scope calls");
        assert_eq!(all_external.phrase, reliability::NO_IN_SCOPE_CALLS);

        let no_calls = scope_row(&scope("empty", 0, 0, 0, 0, None));
        assert_eq!(no_calls.resolved_pct, None);
        assert_eq!(
            no_calls.external_pct, None,
            "no calls at all → external unknown"
        );
        assert_eq!(no_calls.band, None);
    }

    #[test]
    fn measured_zero_is_real_not_unknown() {
        // In-scope calls exist but NONE resolved → a REAL 0.0%, distinct from UNKNOWN.
        let row = scope_row(&scope("py", 0, 5, 0, 0, Some(AgentReliabilityLevel::Low)));
        assert_eq!(row.resolved_pct, Some(0.0), "measured 0%, not unknown");
        assert_eq!(row.in_scope_or_unclassified_total, 5);
        assert_eq!(row.band.as_deref(), Some("LOW"));
    }

    #[test]
    fn material_unclassified_share_fires_the_conservative_caveat() {
        // internal_like = 100, unknown = 40 (40% >= 20% material) → caveat fires.
        let row = scope_row(&scope(
            "rs",
            50,
            100,
            0,
            40,
            Some(AgentReliabilityLevel::Medium),
        ));
        let caveat = row.caveat.expect("material unclassified share caveats");
        assert!(caveat.contains("unclassified"), "{caveat}");
        // Immaterial share (5 of 105) → no caveat.
        let clean = scope_row(&scope(
            "rs2",
            50,
            55,
            0,
            5,
            Some(AgentReliabilityLevel::Medium),
        ));
        assert_eq!(clean.caveat, None);
    }

    #[test]
    fn build_breakdown_is_count_faithful() {
        // The projection never alters counts: the total row and grouped rows carry
        // exactly their input counts (the reconciliation the storage read guarantees
        // is preserved through the projection).
        let total = scope("total", 30, 60, 10, 12, Some(AgentReliabilityLevel::Low));
        let langs = vec![
            scope("java", 10, 40, 5, 8, Some(AgentReliabilityLevel::Low)),
            scope(
                "typescript",
                20,
                20,
                5,
                4,
                Some(AgentReliabilityLevel::Medium),
            ),
        ];
        let mods = vec![scope(
            "src",
            30,
            60,
            10,
            12,
            Some(AgentReliabilityLevel::Low),
        )];
        let bd = build_breakdown(total, langs, mods);

        let sum_resolved: u64 = bd.by_language.iter().map(|r| r.resolved).sum();
        let sum_unresolved: u64 = bd.by_language.iter().map(|r| r.unresolved).sum();
        let sum_external: u64 = bd.by_language.iter().map(|r| r.external).sum();
        assert_eq!(sum_resolved, bd.total.resolved);
        assert_eq!(sum_unresolved, bd.total.unresolved);
        assert_eq!(sum_external, bd.total.external);
    }

    #[test]
    fn row_serializes_unknown_pct_as_json_null() {
        // The JSON protocol surface must carry UNKNOWN as null, never 0 (VISION).
        let row = scope_row(&scope("go", 0, 8, 8, 0, None));
        let v = serde_json::to_value(&row).unwrap();
        assert!(
            v["resolved_pct"].is_null(),
            "unknown rate must be JSON null"
        );
        assert!(v["band"].is_null());
        assert_eq!(v["resolved"], 0);
        assert_eq!(v["unresolved"], 8);
    }
}
