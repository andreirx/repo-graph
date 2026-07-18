//! COHERENCE-LEAF-SERVE-IMPL-1: orient's bounded (b)-leaf SERVE-THEN-FALLBACK decorator.
//!
//! This is the REAL serving mechanism (not a relabel): a [`StoragePort`](AgentStorageRead) decorator
//! that wraps the SQLite storage and, on a GREEN bounded orient cert, serves orient's LG-DERIVABLE (b)
//! leaves from the CURRENT-STATE LiveGraph — performing ZERO eager `nodes`/`edges` reads for those
//! leaves — while DELEGATING every other read (the (c) trust contributor, MODULE_SUMMARY counts,
//! cycles, Authority/gate, boundary, dead-code, docs/FS, snapshot identity) to SQLite VERBATIM. The
//! agent use case (`repo_graph_agent::orient`) is UNTOUCHED: it is generic over `S: AgentStorageRead +
//! GateStorageRead`, so the daemon hands it this decorator instead of the bare `StorageConnection` on
//! green. Clean Architecture: the high-level policy (orient) does not change; a new adapter (this
//! decorator) re-sources the (b) leaves. The trait impls live in [`storage_port_impl`] (the 500-line
//! structural guardrail split); this module holds the struct, the bounded-cert gate, and the
//! focus-resolution native -> agent-DTO mappers.
//!
//! ## What is SERVED from the LiveGraph (on green)
//!
//! - **FOCUS RESOLUTION** (`resolve_path_focus` / `resolve_stable_key_focus` / `resolve_symbol_name` /
//!   `get_symbol_context`) — via the FOCUS-RESOLUTION-LIVEGRAPH producer (`focus_resolver`), gated by
//!   `focus_resolution_cert`. This is the read that makes SYMBOL-focus orient `nodes`-FREE on green:
//!   the focus is resolved from the resident IR, not the SQLite `nodes` table.
//! - **CALLGRAPH** (`find_symbol_callers` / `find_symbol_callees`) — via `callgraph_cert`'s LiveGraph
//!   row builders (`callers`/`callees` + `symbol_context` enrichment), gated by `callgraph_cert`. orient
//!   consumes these ORDER-INSENSITIVELY (`build_callers_evidence` = count + `group_by_module`), so the
//!   multiset-proven LiveGraph rows render byte-identically.
//! - **CYCLE VALUES** (`find_module_cycles*` / `find_cycles_involving_*`) — EC-M2-LEAF-SERVE-1
//!   (CYCLES-B, superseding the CYCLES-A delegate-always posture): served from the LiveGraph
//!   module-cycle SCC when the cycles cert's `values_verdict` is GREEN at the captured fingerprint.
//!   CYCLES-A's blocker (order/naming sensitivity) is dissolved by TWO mechanisms: the agent now
//!   CANONICALIZES cycle values (`ordering::canonicalize_cycles` — members sorted, list length-DESC,
//!   a pure function of the cycle SET on both engines), and the cert build additionally compares the
//!   canonical SERVED shapes (repo short-name + qualified) byte-for-byte (`values_verdict`), so the
//!   short-name rendering can never diverge silently. The IMPORT_CYCLES leaf LABEL keeps the shipped
//!   `orient_cycles_outcome` cert-gated semantics.
//! - **MODULE_SUMMARY structural counts** (`compute_repo_summary` / `compute_path_summary` /
//!   `compute_file_summary`) — EC-M2-LEAF-SERVE-1 (DR-2/DR-E3): served from the LiveGraph
//!   structural inventory when the module-summary IDENTITY-RECONCILIATION cert is GREEN (per-file +
//!   per-module dirname rollup + exact `compute_repo_summary` totals all reconciled; ANY divergence
//!   ⇒ RED ⇒ SQLite — the honest answer to REVISED DR-CLS-2's false-parity finding: the serve
//!   computes ALL-files/ALL-symbols/languages semantics from the IR, and the cert refuses to serve
//!   wherever the substrate cannot reproduce them, e.g. repos with tracked config/contract files).
//!
//! ## What is DELEGATED to SQLite (always)
//!
//! - **(c) TRUST contributor** (`get_trust_summary`) — RETAINED SQLite-LABELLED FOREVER (Contract
//!   Clause 3 / Option A). No LiveGraph producer; orient's `edges`/`unresolved_edges` read stays.
//! - Authority/gate, boundary import edges, dead-code, complexity `measurements`, module discovery,
//!   boundary-links freshness, snapshot/repo identity, doc inventory (FS) — never had a `nodes`/`edges`
//!   LiveGraph home for orient, or are not a `nodes`/`edges` read at all.
//!
//! ## Defensive fallback (per-call)
//!
//! Each served method serves from the LiveGraph ONLY when the answer is `Exact`; otherwise it DELEGATES
//! to the inner SQLite read. Under a held request read lock the bounded cert's GREEN verdict guarantees
//! every (b) answer is `Exact` (the cert proved class==Exact for the whole corpus), so the delegate
//! branch is unreachable on the green path — and the no-eager-read proof (a partial spy that PANICS on
//! the served methods) would catch any regression that took it. The decorator is constructed whenever
//! AT LEAST ONE leaf decision is GREEN ([`OrientServeWitness`] — review-0 #1: the bounded fold and the
//! two M-2 leaves are INDEPENDENT); a leaf whose decision is `false` delegates byte-identically, and
//! when NO leaf serves the daemon calls the bare storage for orient (the unchanged eager path) while
//! explain wraps the pin-only decorator (fingerprint `None` ⇒ byte-transparent).

use repo_graph_agent::{
    AgentFocusCandidate, AgentFocusKind, AgentPathResolution, AgentStorageRead, AgentSymbolContext,
};
use repo_graph_gate::GateStorageRead;
use repo_graph_livegraph::focus_resolver::{
    FocusCandidate, FocusKind, PathResolutionAnswer, SymbolContext,
};
use repo_graph_livegraph::LiveGraph;

use crate::callgraph_cert::callgraph_is_green;
use crate::focus_resolution_cert::focus_resolution_is_green;
use crate::livegraph_feed::{import_cert_fingerprint, RequestEpoch};
use crate::state::RepoState;

mod storage_port_impl;

/// The BOUNDED orient no-loss cert: the AND-fold (MEET) of the (b) LG-derivable leaves orient can
/// REAL-serve no-loss — **FOCUS-RESOLUTION ∧ CALLGRAPH**. GREEN iff BOTH sub-certs are GREEN at the
/// current SHARED fingerprint (each enforces its own resident+Fresh+TS precondition + a field-exact
/// compare, so a non-resident/non-TS/stale/RED contributor forces RED). RED -> the daemon serves the
/// (b) leaves from SQLite (the unchanged eager path).
///
/// EXCLUDED from the fold (each distinct in kind):
/// - the **(c) trust contributor** — EXCLUDED PERMANENTLY (Contract Clause 3); served SQLite-LABELLED.
/// - **MODULE_SUMMARY counts** + **cycle VALUES** — NOT in this AND-fold; EC-M2-LEAF-SERVE-1 gates
///   each behind its OWN cert as an INDEPENDENT leaf decision ([`M2LeafServe`], captured by
///   [`orient_serve_witness`]): a RED module-summary or cycle-VALUES cert degrades ONLY that leaf
///   to SQLite, never the focus-resolution/callgraph serve — and the independence is BOTH ways
///   (review-0 #1): a RED bounded fold degrades only the six (b) methods; a GREEN M-2 leaf still
///   serves through the decorator with [`OrientServeWitness::bounded`]` == false` (coherence
///   F1–F4 independence, per-leaf).
///
/// SHARED BY orient + explain (COHERENCE-LEAF-SERVE-IMPL-2): `handle_explain` reuses this SAME bounded
/// cert to gate the SAME decorator (two consumers now; name kept orient-scoped — a rename is cosmetic).
pub fn orient_bounded_cert_is_green(repo_state: &RepoState, snapshot_uid: &str) -> bool {
    focus_resolution_is_green(repo_state, snapshot_uid)
        && callgraph_is_green(repo_state, snapshot_uid)
}

/// W-B-EPOCH-IMPL-1 (D-EP; `daemon-w-b-epoch-1.md` §5.1/§6.4): the BOUNDED-cert LG-serve eligibility,
/// captured BUILD-THEN-PEEK — the sibling of `callgraph_cert::callgraph_cert_eligibility` over the
/// bounded (FOCUS-RESOLUTION ∧ CALLGRAPH) cert. Returns `Some(current_fp)` iff BOTH sub-certs are GREEN
/// at EXACTLY the resident fingerprint for `snapshot_uid` (so the decorator-served (b) leaves are
/// cert-proven no-loss-equal to SQLite@`snapshot_uid`).
///
/// SINCE EC-M2 review-0 #1 the PRODUCTION capture is [`orient_serve_witness`], which folds this same
/// bounded peek in as ONE of three independent leaf decisions — a `None` here no longer implies bare
/// SQLite (a GREEN M-2 leaf can still serve). This fn remains the bounded fold's build-then-peek unit,
/// exercised directly by the W-B epoch tests and the bounded-only test epochs (`green_epoch`).
///
/// `Some(_).is_some()` is the SAME serve decision the prior `orient_bounded_cert_is_green(...)` produced
/// (both sub-certs green at the current fingerprint) — under the W-A coordinator no swap can occur
/// mid-request, so warming builds both certs at the resident fingerprint and the peek confirms them, giving
/// byte-identical steady-state behavior. Build-then-peek (one livegraph read guard across BOTH the
/// fingerprint computation and the two cert peeks) is what makes the captured witness honest under a future
/// W-B relax: see `callgraph_cert_eligibility` for the lazy-rebuild TOCTOU it closes.
pub fn orient_bounded_cert_eligibility(
    repo_state: &RepoState,
    snapshot_uid: &str,
) -> Option<String> {
    // 1. WARM both sub-certs (each lazily (re)builds if stale/missing, dropping its own guards).
    let _ = orient_bounded_cert_is_green(repo_state, snapshot_uid);
    // 2. PEEK under ONE read guard so "(both green) at (this exact resident fingerprint)" is atomic w.r.t.
    //    a swap (a swap needs `livegraph.write()`, excluded while we hold `read()`).
    let guard = repo_state.livegraph.read();
    let current_fp = import_cert_fingerprint(&guard.as_ref()?.live_partitions(), snapshot_uid);
    let fr_green = matches!(
        repo_state.focus_resolution_cert.read().as_ref(),
        Some(c) if c.fingerprint == current_fp && c.verdict == "GREEN"
    );
    let cg_green = matches!(
        repo_state.callgraph_cert.read().as_ref(),
        Some(c) if c.fingerprint == current_fp && c.verdict == "GREEN"
    );
    (fr_green && cg_green).then_some(current_fp)
}

/// EC-M2-LEAF-SERVE-1: the per-request M-2 leaf-serve decisions, captured ONCE at epoch capture
/// (BUILD-THEN-PEEK, atomic with the bounded witness) and carried by the decorator. Each leaf
/// degrades INDEPENDENTLY (coherence F1–F4): a RED cycle-VALUES cert never blocks the summary
/// serve and vice versa; a `false` simply means that leaf delegates to SQLite exactly as before.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct M2LeafServe {
    /// Serve orient/explain cycle VALUES (`find_module_cycles*` / `find_cycles_involving_*`) from
    /// the LiveGraph: the cycles cert's `values_verdict` is GREEN at the captured fingerprint.
    pub cycle_values: bool,
    /// Serve MODULE_SUMMARY structural counts (`compute_{repo,path,file}_summary`) from the
    /// LiveGraph: the module-summary identity-reconciliation cert is GREEN at the captured
    /// fingerprint.
    pub module_summary: bool,
}

/// EC-M2-LEAF-SERVE-1 (review-0 #1): the FULL orient/explain serve witness — the EV-A epoch pin
/// plus THREE MUTUALLY INDEPENDENT per-leaf serve decisions peeked at that pin: the bounded
/// (FOCUS-RESOLUTION ∧ CALLGRAPH) fold for the six (b) methods, and the two M-2 leaves. A RED
/// anywhere degrades ONLY its own leaf; in particular a GREEN module-summary or cycle-VALUES cert
/// serves even when the UNRELATED bounded fold is RED (the review-0 #1 decoupling).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OrientServeWitness {
    /// The EV-A epoch pin ([`crate::livegraph_feed::RequestEpoch::fingerprint`]): `Some(fp)` iff
    /// AT LEAST ONE leaf decision below is true at `fp` (all decisions were peeked under ONE
    /// livegraph read guard at exactly this resident fingerprint). `None` ⇒ NO leaf serves ⇒
    /// orient runs the unchanged bare-SQLite path; explain wraps the pin-only decorator whose
    /// `epoch_resident` short-circuit keeps it byte-transparent.
    pub fingerprint: Option<String>,
    /// The bounded fr∧cg fold GREEN at `fingerprint` — gates the six (b) focus/callgraph methods
    /// in the decorator AND the callgraph leaf LABEL (`serve_from_lg` in `build_orient_envelope`,
    /// the review-3 item-1 honesty gate). `false` with a `Some` fingerprint = an M-2-only serve.
    pub bounded: bool,
    /// The per-leaf M-2 decisions at `fingerprint` — each independent of `bounded` and of each
    /// other.
    pub m2: M2LeafServe,
}

/// EC-M2-LEAF-SERVE-1: capture the [`OrientServeWitness`].
///
/// Sequencing (the drilldown build-then-peek discipline):
/// 1. WARM the bounded sub-certs ([`orient_bounded_cert_is_green`] — each lazily (re)builds if
///    stale/missing, dropping its own guards).
/// 2. Compute the current resident fingerprint (one livegraph read guard). No resident LiveGraph
///    ⇒ nothing can serve ⇒ the all-off witness (the unchanged pre-M-2 RED path).
/// 3. WARM the two M-2 leaf certs at that fingerprint iff stale/missing — INDEPENDENT of the
///    bounded outcome (review-0 #1; previously skipped on a RED fold, which silently coupled the
///    leaves). Each build reads SQLite ONCE per fingerprint — the same invariant as every sibling
///    cert; the cycles build is the SAME `build_and_store_cycles_cert` the envelope label path
///    uses (no second build there), and the module-summary build is the one NEW
///    once-per-fingerprint SQLite read a RED-fold state now pays for its independent leaf.
/// 4. FINAL PEEK under ONE livegraph read guard: recompute the fingerprint (a swap between steps
///    moves it, flipping every stale-keyed decision to `false` — EV-A honesty) and peek all four
///    certs at exactly that fingerprint, each into its OWN independent decision.
pub fn orient_serve_witness(repo_state: &RepoState, snapshot_uid: &str) -> OrientServeWitness {
    // 1. WARM fr + cg (own locks; cheap no-ops when already warm at the fingerprint).
    let _ = orient_bounded_cert_is_green(repo_state, snapshot_uid);
    // 2. The current resident fingerprint — the M-2 warm key.
    let fp = {
        let guard = repo_state.livegraph.read();
        match guard.as_ref() {
            Some(lg) => import_cert_fingerprint(&lg.live_partitions(), snapshot_uid),
            None => return OrientServeWitness::default(),
        }
    };
    // 3. WARM the M-2 leaf certs (no livegraph read guard held across the builds — they take
    //    their own locks). A build failure leaves the slot stale ⇒ the final peek reads `false`
    //    (that leaf serves SQLite).
    let cycles_stale = !matches!(
        repo_state.cycles_cert.read().as_ref(),
        Some(c) if c.fingerprint == fp
    );
    if cycles_stale {
        let _ = crate::livegraph_feed::build_and_store_cycles_cert(
            repo_state,
            snapshot_uid,
            Some(fp.clone()),
        );
    }
    let ms_stale = !matches!(
        repo_state.module_summary_cert.read().as_ref(),
        Some(c) if c.fingerprint == fp
    );
    if ms_stale {
        let _ = crate::module_summary_cert::build_and_store_module_summary_cert(
            repo_state,
            snapshot_uid,
            Some(fp.clone()),
        );
    }
    // 4. FINAL PEEK under ONE read guard — all four certs at the CURRENT resident fingerprint,
    //    each an INDEPENDENT decision (no fold gates another leaf).
    let guard = repo_state.livegraph.read();
    let Some(lg) = guard.as_ref() else {
        return OrientServeWitness::default();
    };
    let current_fp = import_cert_fingerprint(&lg.live_partitions(), snapshot_uid);
    let fr_green = matches!(
        repo_state.focus_resolution_cert.read().as_ref(),
        Some(c) if c.fingerprint == current_fp && c.verdict == "GREEN"
    );
    let cg_green = matches!(
        repo_state.callgraph_cert.read().as_ref(),
        Some(c) if c.fingerprint == current_fp && c.verdict == "GREEN"
    );
    let bounded = fr_green && cg_green;
    let cycle_values = matches!(
        repo_state.cycles_cert.read().as_ref(),
        Some(c) if c.fingerprint == current_fp && c.values_verdict == "GREEN"
    );
    let module_summary = matches!(
        repo_state.module_summary_cert.read().as_ref(),
        Some(c) if c.fingerprint == current_fp && c.verdict == "GREEN"
    );
    OrientServeWitness {
        fingerprint: (bounded || cycle_values || module_summary).then_some(current_fp),
        bounded,
        m2: M2LeafServe {
            cycle_values,
            module_summary,
        },
    }
}

/// EC-M2-LEAF-SERVE-1: is the captured request epoch STILL the resident LiveGraph state? The
/// POST-serve label revalidation for the MODULE_SUMMARY / explain-identity leaf labels: the serve
/// decision (`M2LeafServe`) is captured BEFORE the use case runs, but a mid-request LiveGraph swap
/// makes the decorator's EV-A gate delegate those leaves to the pinned SQLite snapshot — so a label
/// derived from the pre-captured decision alone would mint FALSE `{livegraph}` provenance on the
/// swap race. Dispatch calls this AFTER the use case and ANDs it into the label argument: on a swap
/// the leaf is labelled sqlite (the value really was SQLite-delegated). The residual window (a swap
/// AFTER the decorator's summary reads but BEFORE this check) errs in the UNDER-claim direction
/// only — the same conservative asymmetry the shipped callgraph label accepts (its cert peek misses
/// after a swap and re-compares) — never an over-claim.
pub fn epoch_still_resident(
    livegraph: &parking_lot::RwLock<Option<LiveGraph>>,
    epoch: &crate::livegraph_feed::RequestEpoch,
) -> bool {
    let Some(captured_fp) = epoch.fingerprint.as_ref() else {
        return false;
    };
    let guard = livegraph.read();
    let Some(lg) = guard.as_ref() else {
        return false;
    };
    let current_fp = import_cert_fingerprint(&lg.live_partitions(), epoch.snapshot_uid());
    &current_fp == captured_fp
}

/// The orient bounded (b)-leaf serve decorator. `livegraph` is the daemon's resident LiveGraph (the
/// (b)-leaf value source on green); `inner` is the SQLite read port everything else delegates to. Held
/// by reference (no clone) for the lifetime of one orient request.
///
/// SHARED BY orient + explain (COHERENCE-LEAF-SERVE-IMPL-2): `handle_explain` wraps the SAME decorator
/// around `run_explain` on a GREEN bounded cert so explain SYMBOL-focus is `nodes`-free too. The four
/// focus-resolution methods serve explain's focus identically; the callgraph methods are multiset-no-loss.
/// orient consumes them order-insensitively (count + `group_by_module`); explain — the FIRST order-sensitive
/// consumer — RANKS the full caller/callee set by relevance before truncating (`agent::explain::call_ranking`,
/// DR-EXPLAIN-CALLER-ORDER resolution `2d6d00d`), so the multiset cert SUFFICES for explain too: both stores
/// rank the cert-proven-equal set to the SAME ordered top-N. No decorator change.
pub struct OrientServeDecorator<'a, S: ?Sized> {
    livegraph: &'a parking_lot::RwLock<Option<LiveGraph>>,
    inner: &'a S,
    /// W-B-EPOCH-IMPL-1 (D-EV = EV-A): the captured request epoch. Each (b)-leaf serve re-validates the
    /// resident LiveGraph fingerprint against `epoch.fingerprint` under the data read guard; on a mismatch
    /// (a mid-request swap) the leaf fails soft to the pinned SQLite snapshot — never a cross-epoch mix.
    epoch: &'a RequestEpoch,
    /// EC-M2-LEAF-SERVE-1 (review-0 #1): the captured BOUNDED (fr∧cg) decision — gates the six (b)
    /// focus/callgraph methods. `false` makes them DELEGATE even when the epoch carries a
    /// fingerprint (an M-2-only serve: a GREEN M-2 leaf serving while an unrelated bounded
    /// sub-cert is RED).
    bounded: bool,
    /// EC-M2-LEAF-SERVE-1: the captured per-leaf M-2 serve decisions (cycle VALUES / MODULE_SUMMARY
    /// counts). `Default` (all false) on every pre-M-2 construction path — those leaves then
    /// delegate to SQLite byte-identically.
    m2: M2LeafServe,
}

impl<'a, S: AgentStorageRead + GateStorageRead + ?Sized> OrientServeDecorator<'a, S> {
    /// Wrap `inner` (the SQLite port) with `livegraph` (the (b)-leaf value source) + the captured request
    /// `epoch` (the EV-A serve-time validation pin). The PRE-M-2 shape: the six (b) methods serve iff the
    /// epoch carries a fingerprint (`bounded` on — every pre-M-2 construction path minted a fingerprint
    /// only on a GREEN bounded fold); the M-2 leaves DELEGATE. Dispatch uses
    /// [`with_leaf_serves`](Self::with_leaf_serves) with the captured witness; this constructor remains
    /// for the explain pin-only path (fingerprint `None` ⇒ nothing serves) and the pre-M-2 tests.
    pub fn new(
        livegraph: &'a parking_lot::RwLock<Option<LiveGraph>>,
        inner: &'a S,
        epoch: &'a RequestEpoch,
    ) -> Self {
        Self {
            livegraph,
            inner,
            epoch,
            bounded: true,
            m2: M2LeafServe::default(),
        }
    }

    /// EC-M2-LEAF-SERVE-1: [`new`](Self::new) plus the captured INDEPENDENT leaf-serve decisions
    /// ([`orient_serve_witness`]): `bounded` gates the six (b) focus/callgraph methods; each `true`
    /// M-2 leaf serves from the LiveGraph when the epoch is still resident (EV-A); a `false` leaf
    /// delegates to SQLite byte-identically (review-0 #1: the three decisions never gate each
    /// other).
    pub fn with_leaf_serves(
        livegraph: &'a parking_lot::RwLock<Option<LiveGraph>>,
        inner: &'a S,
        epoch: &'a RequestEpoch,
        bounded: bool,
        m2: M2LeafServe,
    ) -> Self {
        Self {
            livegraph,
            inner,
            epoch,
            bounded,
            m2,
        }
    }

    /// W-B-EPOCH-IMPL-1 (D-EV = EV-A): is the resident LiveGraph still the captured green-validated epoch?
    /// `true` iff `import_cert_fingerprint(resident partitions, epoch.snapshot_uid()) == epoch.fingerprint`.
    /// Computed INSIDE each serve method's read guard (passed `lg`), so the gate and the data read are
    /// atomic w.r.t. a swap (a swap takes `livegraph.write()`). On `false` the serve method delegates to the
    /// pinned SQLite snapshot. Partition epochs are monotonic, so once a swap moves the fingerprint the
    /// match can never spuriously re-appear.
    fn epoch_resident(&self, lg: &LiveGraph) -> bool {
        // All-off path (no serve witness): never serve the LiveGraph, and SKIP the fingerprint
        // computation. `handle_explain` wraps this decorator even when NO leaf serves (to pin
        // `get_latest_snapshot`), so this short-circuit keeps that path byte-for-byte the
        // bare-SQLite path — the decorator does no LiveGraph work beyond the (negligible) read guard.
        let Some(captured_fp) = self.epoch.fingerprint.as_ref() else {
            return false;
        };
        let current_fp = import_cert_fingerprint(&lg.live_partitions(), self.epoch.snapshot_uid());
        &current_fp == captured_fp
    }

    /// EC-M2-LEAF-SERVE-1 (review-0 #1): the six (b) focus/callgraph methods' gate — the captured
    /// BOUNDED (fr∧cg) decision ∧ [`epoch_resident`](Self::epoch_resident). The M-2 gates
    /// (`m2_summary_inventory` / `m2_module_cycles*`) use `epoch_resident` with their OWN `m2`
    /// flags instead — the per-leaf independence: a RED bounded fold delegates the (b) methods
    /// while a GREEN M-2 cert keeps serving, and vice versa.
    pub(super) fn bounded_epoch_resident(&self, lg: &LiveGraph) -> bool {
        self.bounded && self.epoch_resident(lg)
    }
}

// ── FOCUS-RESOLUTION native -> agent-DTO mappers (the inverse of `focus_resolution_cert`'s *_eq) ─────
//
// `repo-graph-livegraph` must never depend on the agent crate, so the producer returns native types
// that mirror the agent DTOs field-for-field. This is the consumer adapter the producer's `types.rs`
// named ("the native-result -> agent-DTO mapping is the LATER COHERENCE-LEAF-SERVE consumer adapter").
// `pub(super)` so [`storage_port_impl`] (the trait impls) reaches them.

pub(super) fn map_focus_kind(kind: FocusKind) -> AgentFocusKind {
    match kind {
        FocusKind::File => AgentFocusKind::File,
        FocusKind::Module => AgentFocusKind::Module,
        FocusKind::Symbol => AgentFocusKind::Symbol,
    }
}

pub(super) fn map_path_resolution(d: &PathResolutionAnswer) -> AgentPathResolution {
    AgentPathResolution {
        has_exact_file: d.has_exact_file,
        file_stable_key: d.file_key.clone(),
        has_content_under_prefix: d.has_content_under_prefix,
        module_stable_key: d.module_key.clone(),
    }
}

pub(super) fn map_candidate(c: &FocusCandidate) -> AgentFocusCandidate {
    AgentFocusCandidate {
        stable_key: c.key.clone(),
        kind: map_focus_kind(c.kind),
        file: c.file.clone(),
    }
}

pub(super) fn map_symbol_context(c: &SymbolContext) -> AgentSymbolContext {
    AgentSymbolContext {
        file_path: c.file_path.clone(),
        module_path: c.module_path.clone(),
        module_stable_key: c.module_key.clone(),
        name: c.name.clone(),
        qualified_name: c.qualified_name.clone(),
        subtype: c.subtype.clone(),
        line_start: c.line_start,
    }
}

#[cfg(test)]
mod tests;
