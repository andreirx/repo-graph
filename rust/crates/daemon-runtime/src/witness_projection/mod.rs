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

use repo_graph_ir::{CanonicalKey, EdgeType};
use repo_graph_livegraph::PartitionState;
use repo_graph_trust::storage_port::UnresolvedCallSite;
use repo_graph_trust_model::{FreshnessState, LanguageSupport};
use serde_json::{json, Value};

use crate::callgraph_cert::ledger::{
    classify_state, CallClassification, ContestedResolution, LedgerBuildFailure, PinState,
    StateClass, WOneReason, WitnessLedger,
};
use crate::livegraph_feed::import_cert_fingerprint;
use crate::livegraph_refresh::discover_scip_typescript;
use crate::state::RepoState;

/// The one shipped SCIP producer's reader-facing name (the target of
/// [`discover_scip_typescript`] — the projection names what discovery probes).
const PRODUCER_NAME: &str = "scip-typescript";

/// RECON-M-R3b: the reference tier's per-answer budget (recon-design-1 §5.2 —
/// "truncate-with-count per orient's budget-ladder precedent; S-4 sizes the budget"). Rationale
/// RECORDED from the fixture-scale evidence (§3.0b): amodx incoming references mean 5.8 (the vast
/// majority of symbols list in FULL), top-8 ≥ 268, max fan-in 456 (`ui/label.tsx#Label`) — a hot
/// symbol truncates with a NAMED count, never silently. 25 shows a useful orientation sample
/// while bounding a hot symbol's output; S-4 (the monorepo field measurement) sizes the
/// production budget before any default flip — this is the M-R3b gate's fixture-scale bound.
const REFERENCE_TIER_BUDGET: usize = 25;

/// RECON-M-R4 (§5.5): the per-answer budget for Layer-2 hint lists (likely resolutions +
/// contested resolutions), truncate-with-a-NAMED-count per the reference-tier precedent. 25
/// samples orient without flooding a hot caller; S-2 (the monorepo field measurement) sizes the
/// production budget before any default flip. Named, never silent (the §5.2 truncation rule).
const LAYER2_BUDGET: usize = 25;

/// RECON-M-R3b: which reference direction a tier surfaces (recon-design-1 §5.2 — the reference
/// tier on callers/callees/explain SYMBOL focus). `callers`/explain ⇒ incoming ("which symbols
/// reference this"); `callees` ⇒ outgoing ("which symbols this references"). Two variants, three
/// concrete call sites (the callers + explain arms pass `Incoming`, the callees arm `Outgoing`);
/// the simpler alternative rejected — a bare `bool` — loses the self-documenting call sites and
/// the reader-frame `direction` string this owns.
#[derive(Clone, Copy)]
pub enum ReferenceDirection {
    /// Incoming references — edges whose `dst` is the target (who references it).
    Incoming,
    /// Outgoing references — edges whose `src` is the target (what it references).
    Outgoing,
}

impl ReferenceDirection {
    /// The machine-readable direction id carried on the block (reader-frame phrasing lives in the
    /// client renderer — one phrasing owner).
    fn as_str(self) -> &'static str {
        match self {
            Self::Incoming => "incoming",
            Self::Outgoing => "outgoing",
        }
    }
}

/// RECON-M-R4 (§5.5 case 1): the outcome of joining ONE unresolved SITE `(caller, target head)`
/// against the ledger's `semantic`-class (compiler-only) call targets — BOTH sub-classes
/// (`new_pair` AND S-excess `multiplicity`, review-2). The AMBIGUITY GUARD is
/// structural (`Ambiguous` ≠ a guess): a site with ≥ 2 same-named compiler-resolved candidates is
/// NEVER annotated (§5.5 "Ambiguous (≥2 candidates) → NOT landed; unknown stays unknown").
enum Layer2Match<'a> {
    /// No `semantic` target of this name in this caller — no hint (the dominant case).
    NoCandidate,
    /// Exactly ONE — the likely resolution (the resolved callee's canonical key).
    Likely(&'a str),
    /// ≥ 2 same-named candidates — AMBIGUOUS; refuse to annotate (name guard alone can't pick).
    Ambiguous,
}

/// One per-site Layer-2 "likely" hint before JSON assembly:
/// `(caller key, call head name, resolved callee key, call-site line, call-site col)`. A tuple
/// behind an alias per clippy's type-complexity lint; `Ord`-sorted for output determinism (§5.5 is
/// per SITE, so distinct sites of one call are distinct hints — review-1 #2).
type LikelyHint = (String, String, String, Option<i64>, Option<i64>);

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

    // ── Reference tier (M-R3b): the §3.4-2 / §5.2 "compiler-verified references" enrichment ──

    /// RECON-M-R3b: the reference tier for one symbol — the SCIP semantic overlay's non-`Calls`
    /// `References` edges (reads / writes / type references, §3.4-2) INCOMING to (callers/explain)
    /// or OUTGOING from (callees) `target`, coverage-labeled through the shared §5.3.0 gate and
    /// budget-truncated with a NAMED count. `Some` ONLY in W-BOTH with a current measured ledger
    /// AND ≥1 non-withheld reference — absent otherwise (R-0/R-1 byte-identical; an empty tier
    /// adds no information, the M-R3a data-driven-absence rule). Additive beside the call rows:
    /// it never touches `count`, the call multiset, or the trust denominator (§3.4-2).
    ///
    /// **Where the edges come from.** The witness LEDGER holds only reference COUNTS
    /// (`s_kind_totals.references`, a trust/doctor aggregate) — not per-symbol reference edges —
    /// so the tier reads the LIVE resident IRs (the S witness). It reads under ONE LiveGraph read
    /// guard with the SAME never-stale peek every witness surface shares
    /// (`compute_with_seam_probe` / `callgraph_union_eligibility` / `union_serve`): recompute the
    /// resident fingerprint, require the stored ledger MEASURED at exactly it. A reference served
    /// here is thereby pinned to the same `(snapshot, livegraph_fingerprint)` witness pair the
    /// call surfaces pin — no stale row. It is DATA-DRIVEN (not `RMAP_RECON_UNION`-gated): a
    /// §5.3.0 union-accounting read surface like every M-R3a block; the M-R2 flag governs the
    /// call-union ROWS, which this tier never joins.
    ///
    /// **What the ledger contributes (not re-derived):** the R-RAT-4 COLLISION set (§3.5 guard 2
    /// — an endpoint under a detected identity collision is WITHHELD, never attributed to the
    /// pipeline's entity under a byte-equal key) and the ELIGIBLE partition set, which is exactly
    /// the coverage basis — so the tier's partition scope and its coverage label can never
    /// disagree (R-1 mixed-repo scoping falls out: only covered partitions contribute).
    ///
    /// **Population:** DISTINCT endpoint symbols (deduped; self-references excluded — the ledger's
    /// own g2u convention, a self-reference does not make a symbol "referenced by something
    /// else"). This answers the orientation question "which symbols reference this" — the instance
    /// magnitude ("456 references") stays the trust/doctor aggregate, never re-counted here.
    /// Deterministic order: the endpoints' canonical-key order (`BTreeSet`).
    pub fn reference_tier_block(
        repo_state: &RepoState,
        snapshot_uid: &str,
        target: &str,
        direction: ReferenceDirection,
    ) -> Option<Value> {
        let guard = repo_state.livegraph.read();
        let lg = guard.as_ref()?;
        // The never-stale peek (one guard; the swap needs `livegraph.write()`): the ledger must be
        // MEASURED at exactly the current resident fingerprint, or the tier is absent.
        let current_fp = import_cert_fingerprint(&lg.live_partitions(), snapshot_uid);
        let stored = repo_state.witness_ledger.read();
        let ledger = stored.as_ref()?;
        if ledger.fingerprint != current_fp {
            return None;
        }
        let c = ledger.classification.as_ref()?;

        // The R-RAT-4 collision set (§3.5 guard 2), flattened from the ledger's per-partition keys.
        let collisions: BTreeSet<&str> = c
            .colliding_keys
            .values()
            .flat_map(|ks| ks.iter().map(String::as_str))
            .collect();

        // Distinct, non-self, collision-guarded endpoint keys — scoped to the ledger's ELIGIBLE
        // partitions (== the coverage basis; guaranteed equal to the ledger's own edge iteration
        // by the pinned fingerprint). `BTreeSet` ⇒ deterministic order.
        let mut endpoints: BTreeSet<String> = BTreeSet::new();
        for view in lg.resident_irs() {
            if !c.eligible.contains_key(view.id) {
                continue;
            }
            for e in &view.ir.edges {
                if e.edge_type != EdgeType::References {
                    continue;
                }
                let (src, dst) = (e.src.as_str(), e.dst.as_str());
                if src == dst {
                    continue; // self-reference — the ledger's g2u exclusion, kept here for parity
                }
                let (anchor, other) = match direction {
                    ReferenceDirection::Incoming => (dst, src),
                    ReferenceDirection::Outgoing => (src, dst),
                };
                if anchor != target {
                    continue;
                }
                // Guard 2: never surface an S fact under a colliding identity (either endpoint).
                if collisions.contains(anchor) || collisions.contains(other) {
                    continue;
                }
                endpoints.insert(other.to_string());
            }
        }

        let total = endpoints.len();
        if total == 0 {
            return None; // data-driven absence — an empty tier is not a claim of zero
        }
        // Budget-truncate; resolve display for ONLY the shown ≤budget endpoints (name from the
        // live IR — cross-partition; file from the canonical key's own path segment).
        let items: Vec<Value> = endpoints
            .iter()
            .take(REFERENCE_TIER_BUDGET)
            .map(|key| {
                let name = lg
                    .node_display(&CanonicalKey::from_existing(key.clone()))
                    .map(|(n, _)| n);
                json!({ "stable_key": key, "name": name, "file": key_file_path(key) })
            })
            .collect();
        let shown = items.len();
        Some(json!({
            "accounting": "union",
            "coverage": coverage_json_parts(&c.eligible, &current_fp),
            "direction": direction.as_str(),
            "total": total,
            "shown": shown,
            "truncated": total - shown,
            "references": items,
        }))
    }

    /// RECON-M-R3b: attach the reference tier (incoming — "which symbols reference this") to a
    /// serialized explain response, for a SYMBOL-focus answer only (mirrors
    /// [`Self::attach_explain_union_degrees`]). No-op outside W-BOTH-with-current-ledger, on a
    /// non-symbol focus, or with no non-withheld references — byte-identical, R-0.
    pub fn attach_explain_reference_tier(
        repo_state: &RepoState,
        snapshot_uid: &str,
        response: &mut Value,
    ) {
        let symbol = match (
            response["value"]["focus"]["resolved_kind"].as_str(),
            response["value"]["focus"]["resolved_key"].as_str(),
        ) {
            (Some("symbol"), Some(key)) => key.to_string(),
            _ => return,
        };
        if let Some(block) = Self::reference_tier_block(
            repo_state,
            snapshot_uid,
            &symbol,
            ReferenceDirection::Incoming,
        ) {
            if let Some(obj) = response["value"].as_object_mut() {
                obj.insert("references".to_string(), block);
            }
        }
    }

    /// RECON-M-R4 (§5.5): attach the Layer-2 landing (likely resolutions + contested signals) to
    /// a serialized explain response, for a SYMBOL-focus answer only (mirrors
    /// [`Self::attach_explain_reference_tier`]). `sites` are the FOCUS CALLER's unresolved CALL
    /// rows, read from SQLite by the handler — this module never touches SQLite (its PEEK-only
    /// contract). No-op outside W-BOTH-with-current-ledger, on a non-symbol focus, or with no hint
    /// — byte-identical, R-0/R-1. Additive: inserts only the labeled `layer2_resolution` block.
    pub fn attach_explain_layer2(
        repo_state: &RepoState,
        snapshot_uid: &str,
        sites: &[UnresolvedCallSite],
        response: &mut Value,
    ) {
        let symbol = match (
            response["value"]["focus"]["resolved_kind"].as_str(),
            response["value"]["focus"]["resolved_key"].as_str(),
        ) {
            (Some("symbol"), Some(key)) => key.to_string(),
            _ => return,
        };
        let Some(projection) = Self::compute(repo_state, snapshot_uid) else {
            return;
        };
        if let Some(block) = projection.layer2_attribution_block(sites, Some(&symbol)) {
            if let Some(obj) = response["value"].as_object_mut() {
                obj.insert("layer2_resolution".to_string(), block);
            }
        }
    }

    // ── Layer-2 landing (M-R4): §5.5 unresolved-site attribution + contested resolutions ──────

    /// RECON-M-R4 (§5.5 case 1): join ONE unresolved SITE `(caller, target expression HEAD)`
    /// against the ledger's `semantic`-class (compiler-only) call targets — every pair with
    /// compiler-only excess (`s_calls > p`: `new_pair` and `multiplicity` alike, review-2; a
    /// fully corroborated pair never candidates). EXACT name match (the
    /// §5.5 guard — no stemming/fuzzing); ≥ 2 same-named candidates → [`Layer2Match::Ambiguous`]
    /// (refuse to annotate). `NoCandidate` outside a current measured ledger (W-BOTH-only, R-0/R-1
    /// — the caller never annotates without one). Pure: reads only the retained index.
    fn layer2_match(&self, caller_key: &str, target_head: &str) -> Layer2Match<'_> {
        let Some(m) = self.measured() else {
            return Layer2Match::NoCandidate;
        };
        let Some(keys) = m
            .classification
            .semantic_call_targets
            .get(&(caller_key.to_string(), target_head.to_string()))
        else {
            return Layer2Match::NoCandidate;
        };
        let mut it = keys.iter();
        match (it.next(), it.next()) {
            (Some(k), None) => Layer2Match::Likely(k),
            (Some(_), Some(_)) => Layer2Match::Ambiguous,
            // An empty set is never stored (`or_default().insert(..)` always adds) — treated as
            // no candidate for total safety.
            _ => Layer2Match::NoCandidate,
        }
    }

    /// RECON-M-R4 (§5.5 case 2): the retained contested resolutions, optionally scoped to one
    /// caller (explain SYMBOL focus passes `Some(focus)`; trust passes `None` for the whole repo).
    /// Empty outside a current measured ledger (W-BOTH-only). Deterministic (ledger order).
    fn contested_resolutions(&self, focus: Option<&str>) -> Vec<&ContestedResolution> {
        let Some(m) = self.measured() else {
            return Vec::new();
        };
        m.classification
            .contested
            .iter()
            .filter(|c| focus.is_none_or(|f| c.caller == f))
            .collect()
    }

    /// RECON-M-R4 (§5.5): the Layer-2 attribution block over a set of unresolved CALL sites — the
    /// "likely resolves to `X`" hints (case 1, name-guarded + ambiguity-refusing) and the
    /// contested-resolution signals (case 2), coverage-labeled and budget-truncated with NAMED
    /// counts. `Some` ONLY in W-BOTH with a current measured ledger AND ≥ 1 hint of any kind
    /// (data-driven absence otherwise — an empty block is never a claim of zero; R-0/R-1). `focus`
    /// scopes the contested list AND (defensively) the sites to one caller for explain SYMBOL
    /// focus; `None` = the whole repo (trust). ADDITIVE: reads only the ledger + the passed sites,
    /// touches NO counter — the §5.5 denominator-invariance non-negotiable (the trust ratio and
    /// unresolved count are computed elsewhere and never see this block).
    pub fn layer2_attribution_block(
        &self,
        sites: &[UnresolvedCallSite],
        focus: Option<&str>,
    ) -> Option<Value> {
        let m = self.measured()?;

        // Case 1 — the exact-name-head join, PER unresolved SITE (§5.5: the unit is the SITE, an
        // `unresolved_edges` row). Every site whose (caller, target head) resolves to EXACTLY ONE
        // same-named compiler target lands its OWN "likely" hint carrying THAT site's location
        // (never an arbitrary first — review-1 #2); a site whose head has ≥ 2 same-named candidates
        // is AMBIGUOUS → REFUSED and counted (never a guess, §5.5 name guard). Every count below is
        // a SITE count. Tuple `(caller, head, callee key, line, col)`, sorted for output
        // determinism independent of the read order (the module's deterministic-ordering invariant).
        let mut likely: Vec<LikelyHint> = Vec::new();
        let mut ambiguous_sites = 0usize;
        for site in sites {
            if focus.is_some_and(|f| site.caller_key != f) {
                continue;
            }
            let Some(head) = head_name(&site.target_key) else {
                continue;
            };
            match self.layer2_match(&site.caller_key, head) {
                Layer2Match::Likely(callee) => likely.push((
                    site.caller_key.clone(),
                    head.to_string(),
                    callee.to_string(),
                    site.line_start,
                    site.col_start,
                )),
                Layer2Match::Ambiguous => ambiguous_sites += 1,
                Layer2Match::NoCandidate => {}
            }
        }
        likely.sort();

        // Case 2 — contested resolutions (ledger-computed; project-scoped + ambiguity-guarded in
        // the ledger — §5.5 case 2, review-1 #1).
        let contested = self.contested_resolutions(focus);

        if likely.is_empty() && ambiguous_sites == 0 && contested.is_empty() {
            return None; // data-driven absence — never a claim of zero
        }

        // Budget-truncate both lists with NAMED counts (§5.2 — never silent). Counts are SITES.
        let likely_total = likely.len();
        let likely_items: Vec<Value> = likely
            .iter()
            .take(LAYER2_BUDGET)
            .map(|(caller, head, callee, line, col)| {
                json!({
                    "caller": caller,
                    "caller_name": key_symbol_name(caller),
                    "call": head,
                    "resolves_to": target_json(callee),
                    "line": line,
                    "col": col,
                })
            })
            .collect();
        let likely_shown = likely_items.len();

        let contested_total = contested.len();
        let contested_items: Vec<Value> = contested
            .iter()
            .take(LAYER2_BUDGET)
            .map(|c| {
                json!({
                    "caller": c.caller,
                    "caller_name": key_symbol_name(&c.caller),
                    "call": c.name,
                    "syntax_target": target_json(&c.syntactic_key),
                    "compiler_target": target_json(&c.semantic_key),
                })
            })
            .collect();
        let contested_shown = contested_items.len();

        Some(json!({
            "accounting": "layer2",
            "coverage": coverage_json(m),
            "likely": likely_items,
            "likely_total": likely_total,
            "likely_shown": likely_shown,
            "likely_truncated": likely_total - likely_shown,
            "ambiguous": ambiguous_sites,
            "contested": contested_items,
            "contested_total": contested_total,
            "contested_shown": contested_shown,
            "contested_truncated": contested_total - contested_shown,
        }))
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
    coverage_json_parts(&m.classification.eligible, &m.fingerprint)
}

/// The §5.3.0 coverage basis from its parts (eligible partition→language map + fingerprint). Two
/// concrete callers: [`coverage_json`] (the measurement blocks) and [`WitnessProjection::
/// reference_tier_block`] (M-R3b), which holds the classification under a guard and never clones
/// a [`MeasuredView`]. Extracting this keeps ONE coverage-basis constructor, so a reference tier
/// and a measurement block over the same ledger can never label coverage differently.
fn coverage_json_parts(eligible: &BTreeMap<String, LanguageSupport>, fingerprint: &str) -> Value {
    let languages: BTreeSet<&'static str> = eligible.values().map(|l| language_label(*l)).collect();
    let partitions: Vec<&String> = eligible.keys().collect();
    json!({
        "languages": languages.into_iter().collect::<Vec<_>>(),
        "partitions": partitions,
        "fingerprint": fingerprint,
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

/// RECON-M-R4 (§5.5 name guard): the called identifier of a raw CALLS `target_key` — the LAST
/// dotted segment (`foo.bar` → `bar`; a bare `cn` → `cn`). The extractor may rewrite the receiver
/// head to a resolved type but ALWAYS preserves the method name as the final segment
/// [ts-extractor `resolve_receiver_type`], so this equals the resolved callee symbol's own `name`
/// — the exact join key, no stemming/fuzzing. `None` for an empty/degenerate key (no claim).
fn head_name(target_key: &str) -> Option<&str> {
    let head = target_key.rsplit('.').next()?.trim();
    (!head.is_empty()).then_some(head)
}

/// The symbol NAME segment of a canonical key (`{repo_uid}:{path}#{name}:SYMBOL:{seg}` — the
/// text between the FIRST `#` and the following `:`). `None` for a key without that shape (e.g. a
/// FILE key) — renders as unknown, never a guessed name.
fn key_symbol_name(key: &str) -> Option<&str> {
    let after_hash = &key[key.find('#')? + 1..];
    let name = &after_hash[..after_hash.find(':')?];
    (!name.is_empty()).then_some(name)
}

/// A resolved-target descriptor for a Layer-2 hint (§5.5): name + file (both derived from the
/// canonical key) + the key itself (the agent's follow anchor). Name/file are `null` when the key
/// shape does not carry them (unknown ≠ empty).
fn target_json(key: &str) -> Value {
    json!({
        "name": key_symbol_name(key),
        "file": key_file_path(key),
        "stable_key": key,
    })
}

#[cfg(test)]
mod tests;
