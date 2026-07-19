//! RECON-M-R3a: the SHARED WITNESS PROJECTION — divergence posture + the union accounting's
//! read surfaces (recon-design-1 §5.3.2-4/§5.4; the §6.1 M-R3a row is the binding contract).
//!
//! ONE computation feeds every witness read surface (the ratified RELIABILITY-REFRAME
//! shared-projection rule; `CallReliabilityView` is the precedent): the trust `witnesses` block,
//! the doctor operational block, orient/stats g1u lines, the modules g2u "unref?" reduction +
//! explain's g2u union-degree second figure, and map's g3u sketch pairs. No surface re-derives
//! any of it — each consumes a [`WitnessProjection`] method.
//!
//! **What it reads (PEEK-only — it NEVER builds the ledger, never walks, never touches SQLite):**
//! `RepoState.witness_ledger` + `witness_ledger_build_failure` (the M-R1/M-R2 substance), the
//! LiveGraph partition inventory ([`LiveGraph::partition_states`] + the resident fingerprint via
//! [`import_cert_fingerprint`]), and producer discovery ([`discover_scip_typescript`] — read-only
//! filesystem/env probe). D-R8-A: a surface finding no ledger renders unknown/absent — it never
//! triggers the walk.
//!
//! **Honesty invariants enforced here:**
//! - *Never a stale number*: ledger figures render ONLY when the stored ledger's fingerprint
//!   equals the CURRENT resident fingerprint. The inventory/fingerprint capture AND the
//!   ledger-currency selection happen under ONE LiveGraph read guard (the cert's peek
//!   discipline — `callgraph_cert::callgraph_union_eligibility` is the precedent): a W-B refresh
//!   swap needs `livegraph.write()`, so it cannot interleave the two reads and pair a
//!   pre-swap fingerprint with the retained pre-swap ledger (review-1's race). Any witness
//!   movement supersedes the ledger → `measured: null` + an unknown reason, never
//!   yesterday's figures.
//! - *Unknown is never zero*: absent measurements are `null`/absent with a reason;
//!   `agreement_pct` is `null` when nothing was dual-measured.
//! - *Data-driven absence (R-0)*: on a repo with no LiveGraph slots, no ledger, and no retained
//!   build failure, [`WitnessProjection::compute`] returns `None` and every surface renders
//!   exactly today's bytes. The dominant zero-SCIP case pays nothing (the check precedes even
//!   producer discovery).
//! - *Regimes are partition-EVIDENCE-scoped*: W-BOTH/W-ONE rows render for partitions the
//!   LiveGraph KNOWS (resident or summary-retained slots). A repo the daemon has never
//!   SCIP-touched has no partition-level fact to state — unknown stays unstated, never guessed
//!   (recorded decision; §4.2's coverage-is-evidence rule at its read-time floor). W-NONE
//!   capability truth stays on doctor's existing toolchain surface, never a discovery-output
//!   line (R-0; D-R1 carve-out).
//! - *Labels carry their accounting*: every union value this module serves carries
//!   `accounting: "union"` + its coverage basis (languages, partitions, fingerprint) — §5.3.0's
//!   labeling rule; pipeline values are never relabeled (R-0/R-1 byte-identity outside W-BOTH
//!   blocks is the §5.3.1 named invariance).
//! - *Deterministic ordering*: partition rows sorted by id ([`LiveGraph::partition_states`]
//!   sorts), pair enumerations in BTreeMap key order, JSON maps in serde_json's sorted map.
//!
//! **The three W-ONE reasons (R-RAT-6, §4.2 ladder)** render as three DISTINCT posture lines
//! with concrete next actions — stale is never "available but not loaded" (review-4's pinned
//! defect), and the measured stale∧producer-absent compound names its blocker on the next
//! action, never a fourth regime. The regime classifier is M-R1's [`ledger::classify_state`] —
//! this module is its named production consumer.
//!
//! **Placement** (abstraction ledger, per the operating rule): a NEW read-side module beside
//! `union_serve`, consuming `callgraph_cert::ledger` types downward exactly as `union_serve`
//! does (`livegraph_feed` stays the documented lowest serve module). Concrete current users:
//! the trust/orient/stats/explain/modules_show/modules_list/map/storage_health read paths (8
//! surfaces). Axis of variation: witness/ledger state → per-surface presentation blocks.
//! Simpler alternative rejected: per-surface re-derivation in each handler — barred by the
//! ratified shared-projection rule and by the never-stale rule (eight hand-rolled fingerprint
//! checks would drift). This module reads `RepoState.livegraph` and is listed in the EC-M1
//! reader-set witness manifest for exactly its 8 surfaces — the sanctioned-surface set gains
//! `modules_show`, `modules_list`, `map`, `storage_health` (explicit manifest + witness-constant
//! edit, recorded in the slice report; the ratified M-R3a row mandates these read surfaces and
//! ledger-figure currency requires the fingerprint check, so the reader-set amendment is the
//! row's direct implication).

use std::collections::{BTreeMap, BTreeSet};

use repo_graph_livegraph::PartitionState;
use repo_graph_trust_model::{FreshnessState, LanguageSupport};
use serde_json::{json, Value};

use crate::callgraph_cert::ledger::{
    classify_state, CallClassification, LedgerBuildFailure, PinState, StateClass, WOneReason,
    WitnessLedger,
};
use crate::livegraph_feed::import_cert_fingerprint;
use crate::livegraph_refresh::discover_scip_typescript;
use crate::state::RepoState;

/// The one shipped SCIP producer's reader-facing name (the target of
/// [`discover_scip_typescript`] — the projection names what discovery probes).
const PRODUCER_NAME: &str = "scip-typescript";

/// Reader-frame language name (labels speak the reader's language — the enum's internal
/// maturity names never ship).
fn language_label(l: LanguageSupport) -> &'static str {
    match l {
        LanguageSupport::TypeScriptPrimary => "TypeScript",
        LanguageSupport::CppGuarded => "C/C++",
        LanguageSupport::RustPartialBeta => "Rust",
    }
}

/// One known partition's coverage-regime row (the §4.2 rendering substrate).
struct RegimeRow {
    partition: String,
    language: LanguageSupport,
    class: StateClass,
    /// The slot freshness detail behind a `stale` reason (pending / in-flight / failed — one
    /// reason, distinguished rendering per the §4.2 ladder note).
    freshness: FreshnessState,
}

/// The ledger's read-time validity at the CURRENT resident fingerprint (never-stale rule).
enum LedgerView {
    /// A MEASURED ledger at exactly the current fingerprint — figures may render.
    Measured(Box<MeasuredView>),
    /// A ledger exists at the current fingerprint but measured nothing (degenerate build).
    Degenerate { reason: String },
    /// A ledger exists only at a SUPERSEDED fingerprint (witness movement since the store) —
    /// unknown, never the old numbers. `last_failure` is the retained MOST-RECENT build
    /// outcome when it failed (review-0 defect (a)): a failed rebuild RETAINS the old ledger
    /// (`callgraph_cert::build_and_store_callgraph_cert` clears the failure only on success),
    /// so "superseded" alone would MASK a current build failure — both facts render. Carries
    /// the ledger's own retained type (no mirror struct — its presence always describes the
    /// latest attempt, because success clears it).
    Superseded {
        last_failure: Option<LedgerBuildFailure>,
    },
    /// No ledger; the most recent build attempt FAILED (retained transient-2 fact).
    BuildFailed { fingerprint: String, reason: String },
    /// No ledger, no retained failure — not yet computed.
    NeverBuilt,
}

/// The cloned measured state (one clone per request, only on ledger-bearing repos — recorded
/// cost class; S-1 prices monorepo scale before any default flip).
struct MeasuredView {
    fingerprint: String,
    classification: CallClassification,
    /// Population: symbol×direction projections (`2 × corpus`), NEVER mixed with edge counts.
    projections_total: usize,
    projections_unanswerable: usize,
}

/// The shared witness projection for one request (compute once, consume per surface).
pub struct WitnessProjection {
    producer_provisioned: bool,
    regimes: Vec<RegimeRow>,
    ledger: LedgerView,
}

impl WitnessProjection {
    /// Compute the projection, or `None` when there is NOTHING to say (no LiveGraph slots, no
    /// ledger, no retained failure) — the R-0 data-driven absence; callers render exactly
    /// today's bytes on `None`. Producer discovery runs only past that check (the dominant
    /// zero-SCIP case pays nothing).
    pub fn compute(repo_state: &RepoState, snapshot_uid: &str) -> Option<Self> {
        Self::compute_with_producer(repo_state, snapshot_uid, || {
            discover_scip_typescript().is_ok()
        })
    }

    /// [`Self::compute`] with producer discovery injected (the test seam — discovery reads env
    /// + PATH, which a unit test must control without mutating process state).
    pub fn compute_with_producer(
        repo_state: &RepoState,
        snapshot_uid: &str,
        producer_probe: impl FnOnce() -> bool,
    ) -> Option<Self> {
        Self::compute_with_seam_probe(repo_state, snapshot_uid, producer_probe, || {})
    }

    /// [`Self::compute_with_producer`] with a probe injected at the SEAM between the
    /// inventory/fingerprint capture and the ledger-currency selection — review-1's race
    /// window. The probe runs while the ONE LiveGraph read guard is HELD, so the regression
    /// tests can prove deterministically that a refresh swap (which needs `livegraph.write()`,
    /// `livegraph_refresh.rs`) cannot interleave there, and that ledger movement landing at
    /// the seam still classifies against the PINNED fingerprint. Production passes a no-op;
    /// only the child test module injects behavior (the seam is unobtainable more simply —
    /// a timing-based concurrent test would be nondeterministic).
    fn compute_with_seam_probe(
        repo_state: &RepoState,
        snapshot_uid: &str,
        producer_probe: impl FnOnce() -> bool,
        at_seam: impl FnOnce(),
    ) -> Option<Self> {
        // Partition inventory + the CURRENT resident fingerprint + the ledger-currency
        // selection, ALL under ONE LiveGraph read guard (review-1 item 1). The guard excludes
        // the W-B refresh swap (it requires `livegraph.write()`), so `Measured` can only pair
        // a ledger with the fingerprint that is current at the same consistent instant —
        // iteration 1 released the guard between the two reads, and a swap in that window
        // could render the retained pre-swap ledger as current (a stale number). Lock order
        // livegraph → witness_ledger matches every other dual-lock site
        // (`callgraph_union_eligibility`, `union_serve`); no site acquires them in reverse.
        let (states, ledger_view) = {
            let guard = repo_state.livegraph.read();
            let (states, current_fp) = match guard.as_ref() {
                Some(lg) => (
                    lg.partition_states(),
                    Some(import_cert_fingerprint(&lg.live_partitions(), snapshot_uid)),
                ),
                None => (Vec::new(), None),
            };
            at_seam();
            let stored = repo_state.witness_ledger.read();
            let ledger_view = match (stored.as_ref(), current_fp.as_deref()) {
                (Some(l), Some(fp)) if l.fingerprint == fp => match &l.classification {
                    Some(_) => LedgerView::Measured(Box::new(measured_view(l))),
                    None => LedgerView::Degenerate {
                        reason: l
                            .precondition
                            .clone()
                            .unwrap_or_else(|| "unmeasured".to_string()),
                    },
                },
                // Review-0 defect (a): a superseded ledger must NOT mask a current build
                // failure — a failed rebuild retains the OLD ledger and records the failure
                // beside it (cleared only on success), so the retained failure IS the latest
                // attempt's outcome and renders alongside the superseded fact.
                (Some(_), _) => LedgerView::Superseded {
                    last_failure: repo_state.witness_ledger_build_failure.read().clone(),
                },
                (None, _) => {
                    let failure = repo_state.witness_ledger_build_failure.read();
                    match failure.as_ref() {
                        Some(f) => LedgerView::BuildFailed {
                            fingerprint: f.fingerprint.clone(),
                            reason: f.reason.clone(),
                        },
                        None => LedgerView::NeverBuilt,
                    }
                }
            };
            (states, ledger_view)
        };

        // R-0 data-driven absence: nothing known, nothing stored, nothing failed → None.
        if states.is_empty() && matches!(ledger_view, LedgerView::NeverBuilt) {
            return None;
        }

        let producer_provisioned = producer_probe();
        let regimes = states
            .into_iter()
            .filter_map(|s| regime_row(s, producer_provisioned))
            .collect();

        Some(Self {
            producer_provisioned,
            regimes,
            ledger: ledger_view,
        })
    }

    /// True iff a MEASURED ledger at the current fingerprint backs this projection (the only
    /// state in which union figures exist).
    fn measured(&self) -> Option<&MeasuredView> {
        match &self.ledger {
            LedgerView::Measured(m) => Some(m),
            _ => None,
        }
    }

    // ── Trust: the §5.4 `witnesses` block ────────────────────────────────────────────────────

    /// The trust `witnesses` block (posture surface): regimes with reason-specific posture
    /// lines + the full union-accounting measurement (or `measured: null` + the unknown reason —
    /// never a stale number).
    pub fn trust_block(&self) -> Value {
        let mut block = self.posture_core();
        match self.measured() {
            Some(m) => {
                block["measured"] = measurement_block(m, MeasurementDetail::Trust);
            }
            None => {
                block["measured"] = Value::Null;
                block["unknown_reason"] = json!(self.unknown_reason());
            }
        }
        block
    }

    // ── Doctor: the §5.4 operational block ───────────────────────────────────────────────────

    /// The doctor operational block (ops surface): ledger presence/currency/fingerprint, the
    /// last build failure when absent, regimes with next actions, and — when measured — the
    /// operational detail (per-partition adoption counts, colliding keys, delta-pair
    /// enumeration, unmeasured counts by population).
    pub fn doctor_block(&self) -> Value {
        let mut block = self.posture_core();
        block["ledger"] = match &self.ledger {
            LedgerView::Measured(m) => json!({
                "present": true,
                "current": true,
                "fingerprint": m.fingerprint,
            }),
            LedgerView::Degenerate { reason } => json!({
                "present": true,
                "current": true,
                "measured": false,
                "reason": reason,
            }),
            LedgerView::Superseded { last_failure } => {
                let mut obj = json!({
                    "present": true,
                    "current": false,
                    "note": "superseded by witness movement; rebuilt on the next call-graph read",
                });
                // Defect (a): the latest attempt's failure renders BESIDE the superseded fact
                // (the retained failure is cleared on success, so it is always current news).
                if let Some(f) = last_failure {
                    obj["last_build_outcome"] = json!("failed");
                    obj["failed_fingerprint"] = json!(f.fingerprint);
                    obj["failure_reason"] = json!(f.reason);
                }
                obj
            }
            LedgerView::BuildFailed {
                fingerprint,
                reason,
            } => json!({
                "present": false,
                "last_build_outcome": "failed",
                "failed_fingerprint": fingerprint,
                "failure_reason": reason,
            }),
            LedgerView::NeverBuilt => json!({
                "present": false,
                "last_build_outcome": "not yet attempted (built on the next call-graph read)",
            }),
        };
        if let Some(m) = self.measured() {
            block["measured"] = measurement_block(m, MeasurementDetail::Doctor);
        }
        block
    }

    // ── Orient/stats: the §5.3.2 g1u block ───────────────────────────────────────────────────

    /// The lean g1u union-call block for orientation surfaces — `Some` ONLY in W-BOTH with a
    /// current measured ledger (absent outside it, R-0/R-1; §5.3.2: additive beside the
    /// pipeline figure, never replacing it).
    pub fn g1u_block(&self) -> Option<Value> {
        let m = self.measured()?;
        if m.classification.eligible.is_empty() {
            return None;
        }
        let c = &m.classification;
        Some(json!({
            "accounting": "union",
            "coverage": coverage_json(m),
            "union_calls": c.union_calls,
            "pipeline_calls": c.pipeline_calls,
            "dual_measured": c.dual_measured,
            "agreement_pct": c.agreement_pct(),
            "both": c.both,
            "semantic_only_calls": c.semantic.total(),
            "syntactic_only": c.syntactic.total(),
            "unmeasured_edges": c.unmeasured_edges,
        }))
    }

    // ── Modules: the §5.3.3(a) g2u "unref?" reduction ────────────────────────────────────────

    /// REDUCTION-ONLY (§5.3.3a): of `flagged` (pipeline-flagged unreferenced symbol keys), how
    /// many have a compiler-witnessed incoming edge (union call OR compiler reference) — known
    /// false positives of the syntax-only view. `None` outside a current measured ledger
    /// (ledger absent → exactly today's hedged answer). Can only REMOVE flags, never add.
    pub fn unref_reduction<'a>(&self, flagged: impl IntoIterator<Item = &'a str>) -> Option<usize> {
        let m = self.measured()?;
        let set = &m.classification.s_incoming_witnessed;
        Some(flagged.into_iter().filter(|k| set.contains(*k)).count())
    }

    /// The g2u reduction's labeled JSON object (attach beside — never instead of — the pipeline
    /// rollup): count + the §5.3.0 accounting/coverage labels. `None` when nothing measured or
    /// the reduction is 0 (a zero reduction adds no information — data-driven absence).
    pub fn unref_reduction_block<'a>(
        &self,
        flagged: impl IntoIterator<Item = &'a str>,
    ) -> Option<Value> {
        let n = self.unref_reduction(flagged)?;
        if n == 0 {
            return None;
        }
        let m = self.measured()?;
        Some(json!({
            "accounting": "union",
            "coverage": coverage_json(m),
            "fewer_flagged": n,
            "basis": "compiler-verified references found",
        }))
    }

    // ── Explain: the §5.3.3(b) g2u union-degree second figure ────────────────────────────────

    /// The union call fan-in of `symbol` (Σ max(p, s) over its incoming pairs), beside the
    /// pipeline fan-in (Σ p). `Some` ONLY when measured AND the two differ (§5.3.3b: "a labeled
    /// second figure where it differs"). Returns `(pipeline, union)`.
    pub fn union_fan_in(&self, symbol: &str) -> Option<(usize, usize)> {
        self.union_degree(|pair| pair.1 == symbol)
    }

    /// The union call fan-out of `symbol` (outgoing pairs). See [`Self::union_fan_in`].
    pub fn union_fan_out(&self, symbol: &str) -> Option<(usize, usize)> {
        self.union_degree(|pair| pair.0 == symbol)
    }

    fn union_degree(&self, side: impl Fn(&(String, String)) -> bool) -> Option<(usize, usize)> {
        let m = self.measured()?;
        let mut pipeline = 0usize;
        let mut union = 0usize;
        for (pair, rec) in &m.classification.pairs {
            if side(pair) {
                pipeline += rec.p;
                union += rec.p.max(rec.s_calls);
            }
        }
        (union != pipeline).then_some((pipeline, union))
    }

    /// The coverage label for a served union-degree figure (§5.3.0: a union value never ships
    /// without its accounting + coverage basis).
    pub fn union_degree_label(&self) -> Option<Value> {
        let m = self.measured()?;
        Some(json!({ "accounting": "union", "coverage": coverage_json(m) }))
    }

    /// RECON-M-R3a (g2u-b, §5.3.3b): attach the union-degree second figure to a serialized
    /// explain response — for a SYMBOL-focus answer, each EXPLAIN_CALLERS / EXPLAIN_CALLEES
    /// signal's `evidence` gains an additive `union` object (`{count, pipeline_count,
    /// accounting, coverage}`) WHERE the union degree differs from the pipeline degree.
    /// No-op outside W-BOTH-with-current-ledger, on non-symbol focus, or when the degrees
    /// agree ("a labeled second figure where it differs" — nothing else changes; the
    /// pipeline `count` field is never touched).
    pub fn attach_explain_union_degrees(
        repo_state: &RepoState,
        snapshot_uid: &str,
        response: &mut Value,
    ) {
        let Some(projection) = Self::compute(repo_state, snapshot_uid) else {
            return;
        };
        let Some(label) = projection.union_degree_label() else {
            return;
        };
        let value = &mut response["value"];
        let symbol = match (
            value["focus"]["resolved_kind"].as_str(),
            value["focus"]["resolved_key"].as_str(),
        ) {
            (Some("symbol"), Some(key)) => key.to_string(),
            _ => return,
        };
        let Some(signals) = value["signals"].as_array_mut() else {
            return;
        };
        for leaf in signals {
            let signal = &mut leaf["value"];
            let degree = match signal["code"].as_str() {
                Some("EXPLAIN_CALLERS") => projection.union_fan_in(&symbol),
                Some("EXPLAIN_CALLEES") => projection.union_fan_out(&symbol),
                _ => continue,
            };
            let Some((pipeline, union)) = degree else {
                continue;
            };
            if let Some(evidence) = signal["evidence"].as_object_mut() {
                let mut block = json!({ "count": union, "pipeline_count": pipeline });
                if let (Some(b), Some(l)) = (block.as_object_mut(), label.as_object()) {
                    for (k, v) in l {
                        b.insert(k.clone(), v.clone());
                    }
                }
                evidence.insert("union".to_string(), block);
            }
        }
    }

    // ── Map: the §5.3.4 g3u sketch pairs ─────────────────────────────────────────────────────

    /// The union-only CALL file pairs (`semantic`/`new_pair` — the ONLY class that can add a
    /// sketch pair, §5.3.4): `(src_file, dst_file)` derived from the canonical keys' path
    /// segment. Pairs whose key paths cannot be derived are SKIPPED (no claim). `None` outside
    /// a current measured ledger. The caller (map handler) subtracts pairs the pipeline sketch
    /// already holds and RECORDS the delta.
    pub fn g3u_new_call_file_pairs(&self) -> Option<BTreeSet<(String, String)>> {
        let m = self.measured()?;
        let mut out = BTreeSet::new();
        for (pair, rec) in &m.classification.pairs {
            if rec.p == 0 && rec.s_calls > 0 && rec.dual_measured {
                if let (Some(a), Some(b)) = (key_file_path(&pair.0), key_file_path(&pair.1)) {
                    if a != b {
                        out.insert((a.to_string(), b.to_string()));
                    }
                }
            }
        }
        Some(out)
    }

    /// The map overlay's accounting/coverage label (present iff measured).
    pub fn g3u_label(&self) -> Option<Value> {
        let m = self.measured()?;
        Some(json!({ "accounting": "union", "coverage": coverage_json(m) }))
    }

    // ── Shared assembly ──────────────────────────────────────────────────────────────────────

    /// The posture core every block shares: producer presence + the regime rows.
    fn posture_core(&self) -> Value {
        let regimes: Vec<Value> = self.regimes.iter().map(regime_json).collect();
        json!({
            "producer": { "name": PRODUCER_NAME, "provisioned": self.producer_provisioned },
            "regimes": regimes,
        })
    }

    /// Why no measurement renders (trust's `unknown_reason` — reader-actionable, never a stale
    /// number).
    fn unknown_reason(&self) -> String {
        match &self.ledger {
            LedgerView::Measured(_) => unreachable!("only called when unmeasured"),
            LedgerView::Degenerate { reason } => format!("nothing measured ({reason})"),
            // Defect (a): a masked failure would leave the reader expecting the next read to
            // heal a state whose latest rebuild actually FAILED — both facts render.
            LedgerView::Superseded {
                last_failure: Some(f),
            } => format!(
                "superseded by witness movement, and the latest re-measurement attempt \
                 failed ({}); retried on the next call-graph read",
                f.reason
            ),
            LedgerView::Superseded { last_failure: None } => {
                "superseded by witness movement; re-measured on the next call-graph read"
                    .to_string()
            }
            LedgerView::BuildFailed { reason, .. } => {
                format!("last measurement attempt failed ({reason})")
            }
            LedgerView::NeverBuilt => {
                "not yet measured (computed on the next call-graph read)".to_string()
            }
        }
    }
}

/// Build the [`MeasuredView`] clone from a stored ledger (classification presence checked by
/// the caller).
fn measured_view(l: &WitnessLedger) -> MeasuredView {
    let compare = l.compare.as_ref();
    MeasuredView {
        fingerprint: l.fingerprint.clone(),
        classification: l
            .classification
            .clone()
            .expect("caller checked classification presence"),
        projections_total: compare.map(|c| 2 * c.corpus_size).unwrap_or(0),
        projections_unanswerable: compare.map(|c| c.unanswerable_projections).unwrap_or(0),
    }
}

/// Classify one known partition into its regime row. `None` = the row renders NOWHERE (the
/// W-NONE cell: no shipped producer for the language and nothing resident — capability truth
/// stays on doctor's existing toolchain surface, never a new default line; unreachable today
/// since every fed slot is TypeScript, kept for exhaustiveness).
fn regime_row(s: PartitionState, producer_provisioned: bool) -> Option<RegimeRow> {
    // Coverage evidence: a shipped producer exists for the language (TS today — exactly what
    // discovery probes), residency overriding (resident S data IS coverage evidence, §4.2).
    let covered = matches!(s.language, LanguageSupport::TypeScriptPrimary);
    let fresh = s.freshness == FreshnessState::Fresh;
    let class = classify_state(
        covered,
        s.resident,
        fresh,
        producer_provisioned,
        // The read surfaces describe PARTITION state, not one request's activation — the
        // transient pin states are request-scoped and never posture (§4.2); `Match` selects
        // the regime-describing W-BOTH cell.
        PinState::Match,
    );
    if matches!(class, StateClass::WNone) {
        return None;
    }
    Some(RegimeRow {
        partition: s.id,
        language: s.language,
        class,
        freshness: s.freshness,
    })
}

/// One regime row's JSON: machine-readable regime/reason + the reader-frame posture line and
/// concrete next action (three DISTINCT lines for the three W-ONE reasons; the stale∧producer-
/// absent compound names its blocker on the next action — §4.2 ladder verbatim).
fn regime_json(r: &RegimeRow) -> Value {
    let partition = r.partition.as_str();
    let mut row = json!({
        "partition": partition,
        "language": language_label(r.language),
    });
    match r.class {
        StateClass::WBothActivated
        | StateClass::WBothTransientPinMoved
        | StateClass::WBothTransientCaptureFailed => {
            // Partition-state eligibility holds (the transients are request-scoped, never
            // partition posture — regime_row always passes `Match`, so only the first arm is
            // reachable; folded for exhaustiveness).
            row["regime"] = json!("W-BOTH");
            row["posture"] = json!("compiler-side analysis is current here — corroboration active");
        }
        StateClass::WOne {
            reason,
            refresh_blocked_producer_absent,
        } => {
            row["regime"] = json!("W-ONE");
            let (reason_id, posture, next_action) = match reason {
                WOneReason::Stale => (
                    "stale",
                    match r.freshness {
                        FreshnessState::PrecisionPending => {
                            "compiler-side analysis here is refreshing — corroboration resumes \
                             when it completes"
                                .to_string()
                        }
                        FreshnessState::RefreshFailed => {
                            "the last compiler-side refresh here failed — analysis is out of date"
                                .to_string()
                        }
                        _ => "compiler-side analysis here is out of date (the source changed \
                              after the compiler last ran)"
                            .to_string(),
                    },
                    if refresh_blocked_producer_absent {
                        format!("refresh requires `{PRODUCER_NAME}`, which is not provisioned")
                    } else {
                        format!("refresh `{partition}` to re-enable corroboration")
                    },
                ),
                WOneReason::NotResident => (
                    "not_resident",
                    "compiler analysis here is available but not loaded".to_string(),
                    format!("load `{partition}` to enable corroboration"),
                ),
                WOneReason::ProducerUnavailable => (
                    "producer_unavailable",
                    format!(
                        "no compiler analysis is loaded here, and its producer \
                         (`{PRODUCER_NAME}`) is not provisioned"
                    ),
                    format!("provision `{PRODUCER_NAME}` to enable corroboration"),
                ),
            };
            row["reason"] = json!(reason_id);
            row["posture"] = json!(posture);
            row["next_action"] = json!(next_action);
            if refresh_blocked_producer_absent {
                row["refresh_blocked_producer_absent"] = json!(true);
            }
        }
        StateClass::WNone => unreachable!("regime_row filters WNone"),
    }
    row
}

/// Which detail tier a measurement block carries.
enum MeasurementDetail {
    /// Trust: the §5.4 union-call + projection fields + the reference-tier line.
    Trust,
    /// Doctor: trust's fields PLUS the operational detail (adoption, colliding keys, delta
    /// pairs, per-population unmeasured counts).
    Doctor,
}

/// The §5.3.0 coverage basis of a measured ledger: languages + partitions + fingerprint.
fn coverage_json(m: &MeasuredView) -> Value {
    let languages: BTreeSet<&'static str> = m
        .classification
        .eligible
        .values()
        .map(|l| language_label(*l))
        .collect();
    let partitions: Vec<&String> = m.classification.eligible.keys().collect();
    json!({
        "languages": languages.into_iter().collect::<Vec<_>>(),
        "partitions": partitions,
        "fingerprint": m.fingerprint,
    })
}

/// The union-accounting measurement block (§5.4 field groups — DISTINCT POPULATIONS, each
/// labeled; instance counts with identity sub-counts where the two differ).
fn measurement_block(m: &MeasuredView, detail: MeasurementDetail) -> Value {
    let c = &m.classification;
    let mut block = json!({
        "accounting": "union",
        "coverage": coverage_json(m),
        "union_calls": c.union_calls,
        "pipeline_calls": c.pipeline_calls,
        "dual_measured": c.dual_measured,
        "agreement_pct": c.agreement_pct(),
        "both": { "instances": c.both, "identities": c.both_identities },
        "semantic_only_calls": {
            "new_pair": c.semantic.new_pair,
            "multiplicity": c.semantic.multiplicity,
            "identities": c.semantic.identities,
        },
        "syntactic_only": {
            "boundary": c.syntactic.boundary,
            "file_scope": c.syntactic.file_scope,
            "uncorroborated": c.syntactic.uncorroborated,
            "multiplicity": c.syntactic.multiplicity,
            "identities": c.syntactic.identities,
        },
        "unmeasured_edges": {
            "instances": c.unmeasured_edges,
            "identities": c.unmeasured_identities,
        },
        "identity_suspect": c.identity_suspect,
        // Review-0 defect (b) — unit truth: the ledger's `identity_collision` counts WITHHELD
        // S strict-`Calls` INSTANCES (ledger.rs), not colliding identities. Rendered in the
        // block's sibling `{instances, identities}` convention (identities = distinct withheld
        // (caller, callee) pairs, the same identity unit every other field uses). The colliding
        // KEY population is a THIRD unit and stays doctor-only (`colliding_keys` + its line).
        "identity_collision": {
            "instances": c.identity_collision,
            "identities": c.withheld_pairs.len(),
        },
        // Population: symbol×direction projections — separately named, never summed or ratioed
        // against edge fields (§5.4).
        "projections": {
            "total": m.projections_total,
            "unanswerable": m.projections_unanswerable,
        },
        // The reference TIER — a separately-labeled line with its own population (S `References`
        // instances); NOT a term of the call closure equation (§5.4).
        "references": c.s_kind_totals.references,
    });
    if let MeasurementDetail::Doctor = detail {
        let adoption: BTreeMap<String, Value> = c
            .rollups
            .iter()
            .map(|((lang, partition), r)| {
                (
                    partition.clone(),
                    json!({
                        "language": language_label(*lang),
                        "adopted": r.adoption_adopted,
                        "fallback": r.adoption_fallback,
                        "file_scope": r.adoption_file_scope,
                    }),
                )
            })
            .collect();
        block["adoption"] = json!(adoption);
        block["fallback_key_count"] = json!(c.fallback_key_count);
        if !c.colliding_keys.is_empty() {
            block["colliding_keys"] = json!(c.colliding_keys);
            // Review-0 defect (b) — unit truth: the reader-frame line carries BOTH populations,
            // each with its own unit: the colliding KEYS ("symbol identities" in the reader's
            // frame — the §5.4 KEY population, distinct across partitions) and the withheld
            // call INSTANCES (what the ledger's `identity_collision` actually counts).
            let distinct_keys: BTreeSet<&String> = c.colliding_keys.values().flatten().collect();
            block["collision_line"] = json!(format!(
                "{} between the syntax index and the compiler index — {} compiler-witnessed \
                 call instance{} withheld; shown separately, never merged",
                if distinct_keys.len() == 1 {
                    "1 symbol identity collides".to_string()
                } else {
                    format!("{} symbol identities collide", distinct_keys.len())
                },
                c.identity_collision,
                if c.identity_collision == 1 { "" } else { "s" },
            ));
        }
        if !c.delta_pairs.is_empty() {
            let pairs: Vec<Value> = c
                .delta_pairs
                .iter()
                .map(|d| {
                    json!({
                        "caller": d.caller,
                        "callee": d.callee,
                        "p": d.p,
                        "s_calls": d.s_calls,
                    })
                })
                .collect();
            block["occurrence_delta_pairs"] = json!(pairs);
        }
    }
    block
}

/// The repo-relative file path segment of a canonical key
/// (`{repo_uid}:{path}#{name}:SYMBOL:{segment}` — path lies between the FIRST `:` and the
/// FIRST `#`). `None` when the key does not carry that shape (no claim — the pair is skipped).
fn key_file_path(key: &str) -> Option<&str> {
    let after_uid = &key[key.find(':')? + 1..];
    let path = &after_uid[..after_uid.find('#')?];
    (!path.is_empty()).then_some(path)
}

#[cfg(test)]
mod tests;
