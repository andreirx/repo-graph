//! RECON-M-R2: UNION SERVING for callers/callees in W-BOTH — **FLAG-GATED, NON-DEFAULT**
//! (recon-design-1 §5.2, ratified §8; the §6.1 M-R2 row is the binding contract).
//!
//! With the flag OFF (the default) this module is never entered and every served byte everywhere
//! is byte-identical to today. With the flag ON, ONLY the callers/callees `Auto` arm changes: a
//! W-BOTH-activated answer serves the UNION of the two witnesses — P's SQLite rows VERBATIM (all
//! their fields; their `line`/`column` being the opposite-endpoint symbol's DEFINITION location,
//! §3.3a, never a call site) tagged with their per-row witness class, PLUS S-minted `Calls`-kind
//! rows (one row per S-only instance: `new_pair` AND S-excess `multiplicity`, one mechanism —
//! §3.3 iteration 6) with null-not-zero locations (§3.7-4 retired on this path). Every answer
//! outside W-BOTH activation serves today's exact fallback bytes through the SAME
//! `callers_auto_or_sqlite`/`callees_auto_or_sqlite` builders (no drift by construction).
//!
//! **Where the data comes from (one computation, no re-derivation).** The per-pair union facts —
//! exact `(p, s_calls)` multiplicities, dual-measurability (§3.6), collision-withholding (§3.5
//! guard 2) — are read from the WITNESS LEDGER at the request's pinned fingerprint
//! (`CallClassification::pairs`, "the M-R2 serving substrate"). The union call projection is
//! thereby KIND-PARTITIONED (§3.4-3): the ledger's `s_calls` admits ONLY `EdgeType::Calls`
//! (basis `SyntaxConfirmedCall`) IR edges from W-BOTH-ELIGIBLE partitions, and collision-withheld
//! pairs are structurally absent — so `References`/`Imports` edges and withheld pairs can never
//! mint a served row. The kind-blind `LiveGraph::callers`/`callees` traversal is NOT used for
//! union rows; it remains the byte-frozen cert-compare substrate (§3.7-1's fix is exactly that
//! union serving no longer flows through it). Recorded decision: no new kind-filter API was added
//! to `repo-graph-livegraph` — it would have zero consumers (the ledger already holds the
//! kind-partitioned projection under the same fingerprint the request pins).
//!
//! **The contracts this module enforces:**
//! - `union ⊇ P` verbatim (guard 1 — no-loss: P rows are the `sqlite_fetch` bytes, untouched);
//! - `count == rows.len()` (the preserved boundary invariant, §5.2) with per-identity served
//!   multiplicity = MAX(p, s) (§3.2): p P-rows + max(0, s−p) S-minted rows per pair;
//! - per-row `witness` present ONLY on classified (dual-measured) pairs; `mixed` +
//!   `occurrences: {confirmed, total}` on P-excess delta pairs, never a false `both`; `both`
//!   RESERVED for fully-corroborated rows (R-RAT-5);
//! - `witness_counts {both, semantic_only, syntactic_only, unmeasured}` — instance counts, 1:1
//!   with the row multiset (§5.2);
//! - PER-SYMBOL unanswerability inside W-BOTH SERVES (§3.6; iterations 2–3): BOTH per-symbol
//!   unanswerable classes — `Partial` AND `Unavailable` — are a separate axis from the regime
//!   (§4.2), never a fallback cause by themselves. A `Fresh`, TS-only, fully-resident `Partial`
//!   projection serves. An `Unavailable`-class anchor (S cannot ground the symbol: not in the
//!   xref, or no identity basis) carries NO regime evidence in its own envelope
//!   (`FreshnessState::Unavailable`, empty languages), so the regime is decided by its FILE's
//!   partition state (`LiveGraph::file_partition_status` — the §4.2 eligibility predicate at its
//!   native partition granularity): resident ∧ `Fresh` ∧ TS ⇒ the anchor lives INSIDE W-BOTH and
//!   the union serves; anything else (file unknown to S / stale / non-TS) is a genuine
//!   W-ONE/W-NONE answer and keeps today's fallback bytes + reason — the R-1 uncovered-answer
//!   shape. Served rows are labeled per the ledger's per-pair dual-measurability: a pair some
//!   projection measured keeps its witness class (measured from the OTHER endpoint when the
//!   anchor's own projection could not answer); a pair NEITHER projection measured carries NO
//!   witness field and counts `unmeasured` — unmeasured rows exist exactly where an
//!   anchor-touching projection is unanswerable (an `Exact` anchor's own projection
//!   dual-measures all its pairs);
//! - null-not-zero locations on S-minted rows (`file` from `symbol_context` when Exact, else
//!   null; `line`/`column` ALWAYS null — the LG carries no definition locations);
//! - transient fail-softs (§4.2): pin moved → pipeline at the pinned snapshot + the named
//!   `LiveGraphEpochMoved` — a reason OWNED BY THIS MODULE ([`UnionFallback::EpochMoved`] /
//!   [`EPOCH_MOVED_REASON`]), not by the shared `FallbackReason` vocabulary (COHERENCE-SCOPE
//!   resolution, 2026-07-18); capture failed → pipeline + `LiveGraphUnavailable` (the ledger
//!   genuinely is not available), the failure retained on `witness_ledger_build_failure`.
//!
//! **Placement** (abstraction ledger, per the operating rule; module boundary RATIFIED by the
//! operator 2026-07-18, post-escalate UNION-MODULE-BOUNDARY): a NEW serve module beside
//! `orient_serve`. Concrete current users: the two dispatch arms (`handle_callers`/
//! `handle_callees`) on the flag-ON `Auto` path. Axis of variation: union-vs-byte-substitute
//! serving for the callgraph drilldowns. Simpler alternative rejected: extending
//! `livegraph_feed` — it is the documented LOWEST serve module (depends on neither
//! `callgraph_cert` nor `orient_serve`), and the union path must consume
//! `callgraph_cert::ledger` types; placing it there would invert that documented layering.
//! This module reads `RepoState.livegraph` (the EV-A pin re-check + `symbol_context`
//! enrichment) and is listed in the EC-M1 reader-set witness manifest for exactly the
//! `callers`/`callees` sanctioned surfaces (explicit manifest edit, this slice).

use repo_graph_storage::error::StorageError;
use repo_graph_storage::queries::{CalleeResult, CallerResult, ResolvedSymbol};
use repo_graph_trust_model::{AnswerClass, FreshnessState, Granularity};
use serde_json::{json, Value};

use crate::callgraph_cert::ledger::CallClassification;
use crate::livegraph_feed::{
    callees_auto_or_sqlite, callers_auto_or_sqlite, import_cert_fingerprint, ts_only,
    FallbackReason, RequestEpoch,
};
use crate::state::RepoState;

/// The union-serving flag (RECON-M-R2): `RMAP_RECON_UNION=1` enables union serving for the
/// callers/callees `Auto` arm AND flips their capture to the ledger-validity-gated
/// `callgraph_union_eligibility`. Exactly `"1"` is ON (the smallest unambiguous contract —
/// recorded); unset or any other value is OFF. **The default flip is NOT this flag's job**: it
/// stays non-default until the S-1..S-3 monorepo gates pass (recon-design-1 §6.2), and flipping
/// it is its own recorded step.
pub const UNION_SERVING_ENV: &str = "RMAP_RECON_UNION";

/// True iff union serving is enabled for THIS process (read per request in the dispatch arms —
/// one `var_os` lookup, mirroring the `RMAP_CALLGRAPH_DIFF` gating pattern).
pub fn union_serving_enabled() -> bool {
    std::env::var_os(UNION_SERVING_ENV).is_some_and(|v| v == "1")
}

/// Which projection a union answer serves — decides the pair filter, the S-minted key side, and
/// the response field name.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Callers,
    Callees,
}

/// The per-row witness label computed from a pair's ledger record + the SERVED row count
/// (§3.3's served-label rule).
enum RowLabel {
    /// The pair is not classified (not dual-measured / not in the ledger): NO witness field —
    /// unknown corroboration is not a row property (§3.6).
    Unmeasured,
    /// `s == 0` on a dual-measured pair: the compiler measured here and holds no such call.
    Syntactic,
    /// `s >= p`: every P occurrence is corroborated.
    Both,
    /// P-excess delta pair (`0 < s < p`): the summarizing `mixed` + exact occurrences.
    Mixed { confirmed: usize, total: usize },
}

/// Instance-count accumulator for `witness_counts` (population: union call instances of THIS
/// answer — 1:1 with its row multiset, §5.2).
#[derive(Default)]
struct WitnessCounts {
    both: usize,
    semantic_only: usize,
    syntactic_only: usize,
    unmeasured: usize,
}

impl WitnessCounts {
    fn to_json(&self) -> Value {
        json!({
            "both": self.both,
            "semantic_only": self.semantic_only,
            "syntactic_only": self.syntactic_only,
            "unmeasured": self.unmeasured,
        })
    }
}

/// Label one pair's SERVED P rows and accumulate its instance counts. `served_p` is the count of
/// P rows actually served for the pair (at a matched pin it equals the ledger's `p` — both read
/// the same `edges` table CALLS multiset at the same snapshot; served rows govern the labels,
/// belt-and-suspenders, so a phantom claim can never outrun the served bytes).
fn label_pair(
    cls: &CallClassification,
    pair: &(String, String),
    served_p: usize,
    counts: &mut WitnessCounts,
) -> RowLabel {
    match cls.pairs.get(pair) {
        Some(rec) if rec.dual_measured => {
            let s = rec.s_calls;
            if s == 0 {
                counts.syntactic_only += served_p;
                RowLabel::Syntactic
            } else if s >= served_p {
                counts.both += served_p;
                RowLabel::Both
            } else {
                counts.both += s;
                counts.syntactic_only += served_p - s;
                RowLabel::Mixed {
                    confirmed: s,
                    total: served_p,
                }
            }
        }
        _ => {
            counts.unmeasured += served_p;
            RowLabel::Unmeasured
        }
    }
}

/// Attach the witness fields to one serialized P row per its pair's label.
fn tag_row(mut row: Value, label: &RowLabel) -> Value {
    match label {
        RowLabel::Unmeasured => {}
        RowLabel::Syntactic => {
            row["witness"] = json!("syntactic");
        }
        RowLabel::Both => {
            row["witness"] = json!("both");
        }
        RowLabel::Mixed { confirmed, total } => {
            row["witness"] = json!("mixed");
            row["occurrences"] = json!({ "confirmed": confirmed, "total": total });
        }
    }
    row
}

/// Build ONE S-minted `Calls`-kind row (§5.2): `new_pair` and S-excess `multiplicity` instances
/// share this ONE mechanism. `name`/`file` from `symbol_context` when Exact (the cert's shared
/// enrichment join); a FALLBACK-keyed endpoint has no context row, so its unknowns stay null and
/// `name` falls back to the key (today's LG-row convention). `line`/`column` are ALWAYS null —
/// the field's meaning is the endpoint symbol's DEFINITION location (§3.3a), which the LiveGraph
/// does not carry: unknown, never 0 (retiring defect §3.7-4 on this path). `edge_type: "CALLS"`
/// is honest HERE — the ledger projection admits only strict-`Calls` instances (§3.4), unlike the
/// legacy kind-blind key builder this replaces (§3.7-3). `resolution: "livegraph"` keeps the
/// shipped LG-row value (compat — recorded).
fn minted_row(lg: &repo_graph_livegraph::LiveGraph, key: &str) -> Value {
    let (name, file) = {
        let env = lg.symbol_context(key);
        let ctx = if env.class() == AnswerClass::Exact {
            env.data().and_then(|d| d.as_ref()).cloned()
        } else {
            None
        };
        match ctx {
            Some(c) => (c.name, c.file_path),
            None => (key.to_string(), None),
        }
    };
    json!({
        "stable_key": key,
        "name": name,
        "qualified_name": null,
        "kind": "",
        "subtype": null,
        "file": file,
        "line": null,
        "column": null,
        "edge_type": "CALLS",
        "resolution": "livegraph",
        "witness": "semantic",
    })
}

/// The S-only instances to mint for one direction: every ledger pair on this projection whose
/// `s_calls` exceeds `p` mints `s_calls − p` rows (`new_pair` when `p == 0`, `multiplicity`
/// excess otherwise — one mechanism). Collision-withheld pairs are structurally absent from the
/// ledger's `s_calls` (§3.5 guard 2), so they can NEVER serve. Deterministic order: the pairs
/// BTreeMap's key order.
fn minted_keys_for(cls: &CallClassification, direction: Direction, target: &str) -> Vec<String> {
    let mut out = Vec::new();
    for ((caller, callee), rec) in &cls.pairs {
        let (anchor, minted) = match direction {
            Direction::Callers => (callee, caller),
            Direction::Callees => (caller, callee),
        };
        if anchor != target {
            continue;
        }
        if rec.s_calls > rec.p {
            for _ in 0..(rec.s_calls - rec.p) {
                out.push(minted.clone());
            }
        }
    }
    out
}

/// Assemble the W-BOTH-activated union answer for one direction: P rows verbatim (tagged), then
/// the S-minted rows (deterministic pair order). `count == rows.len()` by construction; per pair
/// the served multiplicity is `p + max(0, s−p) = max(p, s)` — the §3.2 MAX rule.
fn assemble_union(
    lg: &repo_graph_livegraph::LiveGraph,
    cls: &CallClassification,
    direction: Direction,
    target: &ResolvedSymbol,
    p_rows_json: Vec<(String, Value)>,
) -> (Vec<Value>, WitnessCounts) {
    let mut counts = WitnessCounts::default();

    // Group the served P rows by endpoint key to label per PAIR (rows of one pair share a label —
    // nothing in a row identifies which occurrence it denotes, §3.3a).
    let mut per_key: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for (key, _) in &p_rows_json {
        *per_key.entry(key.clone()).or_default() += 1;
    }
    let mut labels: std::collections::BTreeMap<String, RowLabel> =
        std::collections::BTreeMap::new();
    for (key, &served_p) in &per_key {
        let pair = match direction {
            Direction::Callers => (key.clone(), target.stable_key.clone()),
            Direction::Callees => (target.stable_key.clone(), key.clone()),
        };
        labels.insert(key.clone(), label_pair(cls, &pair, served_p, &mut counts));
    }

    let mut rows: Vec<Value> = Vec::with_capacity(p_rows_json.len());
    for (key, row) in p_rows_json {
        let label = labels.get(&key).unwrap_or(&RowLabel::Unmeasured);
        rows.push(tag_row(row, label));
    }

    // S-minted rows (new_pair + S-excess multiplicity — one mechanism, §3.3 iteration 6).
    let minted = minted_keys_for(cls, direction, &target.stable_key);
    counts.semantic_only += minted.len();
    for key in minted {
        rows.push(minted_row(lg, &key));
    }

    (rows, counts)
}

/// The §4.2 transient-1 movement reason STRING, owned by this module. It reaches the reader only
/// through the union answers this module serves — the shared `FallbackReason` vocabulary and its
/// cross-crate `CoherenceFallbackReason` mirror stay at today's variant set (COHERENCE-SCOPE
/// resolution, 2026-07-18: a union-only value with zero coherence producers would be dormant
/// capability on a frozen cross-crate shape).
const EPOCH_MOVED_REASON: &str = "LiveGraphEpochMoved";

/// A union-ladder fallback reason. LOCAL two-case discriminator (abstraction ledger): concrete
/// current users are the two response fns' fallback arms; the axis of variation is
/// shared-vocabulary reasons vs the union-path-only movement transient; the simpler alternative —
/// widening the shared `FallbackReason` enum (+ its forced 1:1 coherence mirror) — was rejected
/// by the COHERENCE-SCOPE resolution and reverted (iteration 1).
enum UnionFallback {
    /// One of today's reasons — served through the shared builder VERBATIM (unchanged bytes).
    Shared(FallbackReason),
    /// §4.2 transient 1: the request CAPTURED a valid witness-ledger fingerprint, but the
    /// resident fingerprint MOVED before the data read (any witness movement: snapshot advance,
    /// swap, load/unload, `mark_stale`). THIS answer fails soft to pipeline rows at the pinned
    /// snapshot with NO witness fields; the next request re-captures and self-heals. Named
    /// distinctly ([`EPOCH_MOVED_REASON`]) because the old fold into `LiveGraphUnavailable` was
    /// FALSE for a resident-and-available graph whose pin moved (name-vs-semantics,
    /// recon-design-1 §4.2). The capture-FAILED transient keeps `LiveGraphUnavailable` — there
    /// the ledger genuinely is not available.
    EpochMoved,
}

/// The union ladder's non-serve outcomes. Every fallback serves TODAY'S bytes through the shared
/// `*_auto_or_sqlite` builder; `EpochMoved` is the §4.2 transient-1 named reason.
enum UnionOutcome {
    /// W-BOTH activated: the assembled union answer (rows + counts), ready to wrap.
    Serve(Vec<Value>, WitnessCounts),
    /// Not activated / not W-BOTH for this answer: today's pipeline fallback with this reason.
    Fallback(UnionFallback),
}

/// The flag-ON `Auto` ladder for one direction. TWO AXES, kept distinct (recon-design-1 §4.2:
/// "per-symbol ANSWERABILITY inside W-BOTH (§3.6) is a separate axis at a different granularity,
/// not a fourth regime" — iterations 2–3, the review-1/review-2 fixes):
///
/// **Regime facts fall back** — an answer whose own regime is not activated-W-BOTH serves today's
/// exact fallback bytes through the shared builder, and every still-falling-back case carries
/// TODAY'S exact reason (arm-order parity with today's `auto_outcome` reduction): no LG →
/// `Unavailable`; no pin → `Unavailable` (capture failed, transient 2); pin moved → `EpochMoved`
/// (the ONE renamed case — transient 1); a per-symbol-`Unavailable` anchor whose FILE supplies no
/// eligible-partition evidence (below) → `Unavailable`; ¬`Fresh` → `Stale` (W-ONE `stale`);
/// non-`Exact` justified by RESIDENCY (`missing_partitions` non-empty — W-ONE `not_resident`
/// territory) or an out-of-scope language mix (¬TS, the D4 scope) → `Partial` (today's arm-3
/// reason for BOTH, because today's class check precedes its language check); `Exact` ∧ ¬TS →
/// `UnsupportedLanguage`.
///
/// **Per-symbol unanswerability inside W-BOTH serves** (§3.6-i/ii), both unanswerable classes:
///
/// - **`Partial`, per-symbol causes only** — identity/answerability degradation (structural
///   file-scope nodes, fallback-identity endpoints, unresolved callees) with `Fresh` ∧ TS-only ∧
///   no missing partition: the anchor's own envelope proves an eligible regime, so the union
///   SERVES. (`Fresh` ∧ per-symbol-`Partial` excludes every regime cause: `ProducerUnavailable`
///   pairs only with non-`Fresh` [PRODUCER-ABSENT-1 contract], `PrecisionPending` IS a
///   freshness, residency rides `missing_partitions`.)
/// - **`Unavailable`** — S cannot ground the anchor (the two construction sites: not in the xref
///   at all, or no identity basis), so its envelope carries NO regime evidence at all
///   (`FreshnessState::Unavailable`, empty language set — reducing regime arms over it would
///   conflate the axes, review-2's finding). The regime evidence lives at the regime's OWN
///   granularity: the anchor FILE's partition state ([`repo_graph_livegraph::LiveGraph::
///   file_partition_status`], read under the SAME guard). A file in a resident ∧ `Fresh` ∧ TS
///   partition means the anchor lives INSIDE W-BOTH — a pipeline symbol S's producer did not
///   emit — and the union SERVES. A file with no eligible partition (uncovered language,
///   non-resident, stale, non-TS — or no pipeline file coordinate to look up) is a genuine
///   W-ONE/W-NONE answer: today's fallback, today's `LiveGraphUnavailable` reason (the R-1
///   uncovered-answer shape — the pipeline-only fixture's `rustFn` gate).
///
/// Served rows are labeled per the LEDGER's per-pair dual-measurability at the pinned
/// fingerprint: a pair some projection measured keeps its witness class (measured from the OTHER
/// endpoint when the anchor's own projection could not answer); a pair NEITHER projection
/// measured carries NO witness field and counts `unmeasured` — measurable-side facts serve,
/// unknown corroboration is never a row claim, and the composition never hides. Unmeasured rows
/// exist exactly where an anchor-touching projection is unanswerable: an `Exact` anchor's pairs
/// are all dual-measured via its own projection (the ledger's `measured_pair` `cm`/`em`
/// disjunction). Then the ledger must still be at the pinned fingerprint with a MEASURED
/// classification (defensive — capture peeked exactly this).
fn union_outcome(
    repo_state: &RepoState,
    epoch: &RequestEpoch,
    direction: Direction,
    target: &ResolvedSymbol,
    fetch_pairs: impl FnOnce() -> Result<Vec<(String, Value)>, StorageError>,
) -> Result<UnionOutcome, StorageError> {
    let guard = repo_state.livegraph.read();
    let Some(lg) = guard.as_ref() else {
        return Ok(UnionOutcome::Fallback(UnionFallback::Shared(
            FallbackReason::LiveGraphUnavailable,
        )));
    };
    let Some(captured) = epoch.fingerprint.as_ref() else {
        // Capture failed / no valid ledger at capture (§4.2 transient 2) — the ledger genuinely
        // is not available; the retained build-failure record is doctor's food (M-R3a renders).
        return Ok(UnionOutcome::Fallback(UnionFallback::Shared(
            FallbackReason::LiveGraphUnavailable,
        )));
    };
    // EV-A: recompute the resident fingerprint under the SAME read guard the data reads use.
    let current_fp = import_cert_fingerprint(&lg.live_partitions(), epoch.snapshot_uid());
    if &current_fp != captured {
        return Ok(UnionOutcome::Fallback(UnionFallback::EpochMoved));
    }
    // This answer's own regime reduction (today's arm ORDER — reason parity for every fallback).
    let (class, freshness, ts, missing_empty) = match direction {
        Direction::Callers => {
            let env = lg.callers(&target.stable_key, Granularity::CallerDetail);
            (
                env.class(),
                env.freshness(),
                ts_only(env.contributing_languages()),
                env.missing_partitions().is_empty(),
            )
        }
        Direction::Callees => {
            let env = lg.callees(&target.stable_key, Granularity::CallerDetail);
            (
                env.class(),
                env.freshness(),
                ts_only(env.contributing_languages()),
                env.missing_partitions().is_empty(),
            )
        }
    };
    if class == AnswerClass::Unavailable {
        // Per-symbol `Unavailable` (§3.6): S cannot GROUND this anchor, so its envelope carries
        // NO regime evidence — the (freshness, ts, missing) reduction above describes nothing
        // here. The regime is decided at ITS granularity: the anchor FILE's partition state
        // (review-2's fix). Eligible (resident ∧ `Fresh` ∧ TS — the §4.2 predicate) ⇒ the anchor
        // lives inside W-BOTH and the union serves below, rows labeled by the ledger (its pairs
        // are typically unmeasured unless the OTHER endpoint's projection measured them).
        // Not eligible (file unknown to S / stale / non-TS / no pipeline file coordinate) ⇒ a
        // genuine W-ONE/W-NONE answer: today's exact bytes + reason (R-1 — today's ladder maps
        // every `Unavailable` anchor to `LiveGraphUnavailable` before its freshness check).
        let file_eligible = target
            .file
            .as_deref()
            .and_then(|f| lg.file_partition_status(f))
            .is_some_and(|s| s.fresh && s.ts_primary);
        if !file_eligible {
            return Ok(UnionOutcome::Fallback(UnionFallback::Shared(
                FallbackReason::LiveGraphUnavailable,
            )));
        }
    } else {
        // A GROUNDED anchor's envelope IS regime evidence — today's arm order, reason parity.
        if freshness != FreshnessState::Fresh {
            return Ok(UnionOutcome::Fallback(UnionFallback::Shared(
                FallbackReason::LiveGraphStale,
            )));
        }
        if class != AnswerClass::Exact && !(missing_empty && ts) {
            // Regime-degraded non-Exact: residency (W-ONE not_resident) or the D4 language
            // scope — today's arm-3 reason for both. The let-through set (Fresh ∧ TS ∧ fully
            // resident, degraded by per-symbol reasons alone) is §3.6's separate axis and
            // SERVES below.
            return Ok(UnionOutcome::Fallback(UnionFallback::Shared(
                FallbackReason::LiveGraphPartial,
            )));
        }
        if !ts {
            return Ok(UnionOutcome::Fallback(UnionFallback::Shared(
                FallbackReason::LiveGraphUnsupportedLanguage,
            )));
        }
    }
    // The ledger at exactly the pinned fingerprint, measured (capture peeked this; defensive).
    let ledger_guard = repo_state.witness_ledger.read();
    let cls = match ledger_guard.as_ref() {
        Some(l) if &l.fingerprint == captured => l.classification.as_ref(),
        _ => None,
    };
    let Some(cls) = cls else {
        return Ok(UnionOutcome::Fallback(UnionFallback::Shared(
            FallbackReason::LiveGraphUnavailable,
        )));
    };

    // W-BOTH ACTIVATED: the union pays the P read (the union contains P by definition).
    let p_rows = fetch_pairs()?;
    let (rows, counts) = assemble_union(lg, cls, direction, target, p_rows);
    Ok(UnionOutcome::Serve(rows, counts))
}

/// Wrap the served union rows in the response envelope. `backend_used: "union"` names the
/// composition (both witnesses were consulted at the pinned fingerprint; per-ROW measurability
/// follows the ledger — §3.6: an unanswerable-anchor answer serves with unmeasured rows, never a
/// false claim); `fallback_reason` stays null (nothing fell back). Additive beside the shipped
/// `livegraph`/`sqlite` vocabulary — flag-ON only (recorded).
fn union_value(
    field: &str,
    target: &ResolvedSymbol,
    rows: Vec<Value>,
    counts: &WitnessCounts,
) -> Value {
    let count = rows.len();
    json!({
        "target": target,
        field: rows,
        "count": count,
        "backend_used": "union",
        "fallback_reason": null,
        "witness_counts": counts.to_json(),
    })
}

/// RECON-M-R2: the flag-ON `Auto` callers response — union rows in W-BOTH activation, today's
/// exact fallback bytes everywhere else. The dispatch arm calls this ONLY when the flag is ON and
/// the engine is `Auto`; every other combination flows through the unchanged
/// `callers_engine_response`.
pub fn callers_union_response(
    repo_state: &RepoState,
    epoch: &RequestEpoch,
    target: &ResolvedSymbol,
    sqlite_fetch: impl FnOnce() -> Result<Vec<CallerResult>, StorageError>,
) -> Result<Value, StorageError> {
    let mut fetch = Some(sqlite_fetch);
    let outcome = union_outcome(repo_state, epoch, Direction::Callers, target, || {
        let rows = (fetch.take().expect("fetch consumed once"))()?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let key = r.stable_key.clone();
                (key, serde_json::to_value(r).unwrap_or(Value::Null))
            })
            .collect())
    })?;
    match outcome {
        UnionOutcome::Serve(rows, counts) => Ok(union_value("callers", target, rows, &counts)),
        UnionOutcome::Fallback(UnionFallback::Shared(reason)) => callers_auto_or_sqlite(
            target,
            None,
            Some(reason),
            fetch.take().expect("fetch not consumed on fallback"),
        ),
        UnionOutcome::Fallback(UnionFallback::EpochMoved) => {
            // Today's fallback bytes through the SAME shared builder; only the reason NAME is
            // this module's own. The builder always emits the `fallback_reason` key (`null` for
            // `None`), so writing the module-owned string into it changes no shape — same key,
            // byte-identical to a builder-carried reason.
            let mut v = callers_auto_or_sqlite(
                target,
                None,
                None,
                fetch.take().expect("fetch not consumed on fallback"),
            )?;
            v["fallback_reason"] = json!(EPOCH_MOVED_REASON);
            Ok(v)
        }
    }
}

/// RECON-M-R2: the flag-ON `Auto` callees response (symmetric to [`callers_union_response`]).
pub fn callees_union_response(
    repo_state: &RepoState,
    epoch: &RequestEpoch,
    target: &ResolvedSymbol,
    sqlite_fetch: impl FnOnce() -> Result<Vec<CalleeResult>, StorageError>,
) -> Result<Value, StorageError> {
    let mut fetch = Some(sqlite_fetch);
    let outcome = union_outcome(repo_state, epoch, Direction::Callees, target, || {
        let rows = (fetch.take().expect("fetch consumed once"))()?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let key = r.stable_key.clone();
                (key, serde_json::to_value(r).unwrap_or(Value::Null))
            })
            .collect())
    })?;
    match outcome {
        UnionOutcome::Serve(rows, counts) => Ok(union_value("callees", target, rows, &counts)),
        UnionOutcome::Fallback(UnionFallback::Shared(reason)) => callees_auto_or_sqlite(
            target,
            None,
            Some(reason),
            fetch.take().expect("fetch not consumed on fallback"),
        ),
        UnionOutcome::Fallback(UnionFallback::EpochMoved) => {
            // See the callers twin: shared-builder bytes, module-owned reason name.
            let mut v = callees_auto_or_sqlite(
                target,
                None,
                None,
                fetch.take().expect("fetch not consumed on fallback"),
            )?;
            v["fallback_reason"] = json!(EPOCH_MOVED_REASON);
            Ok(v)
        }
    }
}

#[cfg(test)]
mod tests;
