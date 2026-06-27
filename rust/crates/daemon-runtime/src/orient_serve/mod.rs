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
//!
//! ## What is DELEGATED to SQLite (always)
//!
//! - **(c) TRUST contributor** (`get_trust_summary`) — RETAINED SQLite-LABELLED FOREVER (Contract
//!   Clause 3 / Option A). No LiveGraph producer; orient's `edges`/`unresolved_edges` read stays.
//! - **MODULE_SUMMARY counts** (`compute_repo_summary` / `compute_path_summary` / `compute_file_summary`)
//!   — SQLite-LABELLED (REVISED DR-CLS-2 -> A: `module_stats` is TS-only/exports-only/root-excluded and
//!   CANNOT reproduce the all-files/all-symbols/all-languages counts). REPO/PATH/FILE focus keep this
//!   `nodes` read as a PERMANENT SQLite contributor — they are NOT `nodes`-free, by design.
//! - **CYCLES** (`find_module_cycles` / `find_cycles_involving_path` / `find_cycles_involving_module`) —
//!   delegated (RATIFIED CYCLES-A, spec `e363c55`). The existing `cycles_cert` is SET-based
//!   (order/rotation-independent qualified path sets), but orient's cycle output is ORDER-SENSITIVE
//!   (`take(3)` selection + ordered `modules` + the `n.name` basename rendering for repo focus); the
//!   LiveGraph SCC order and the SQLite Tarjan/uid order are independent artifacts, so the existing cert does
//!   NOT license a byte-identical cycle serve. Cycles STAY SQLite-served; CYCLES-B (canonicalize orient's
//!   cycle output, then serve from the LiveGraph) is a deferred follow-up. SCOPE NOTE — this is the VALUE
//!   layer: the decorator delegates the cycle reads to SQLite. The IMPORT_CYCLES leaf LABEL is a SEPARATE,
//!   UNCHANGED concern owned by `orient_coherence::build_orient_envelope` (the shipped hybrid
//!   `orient_cycles_outcome`: `livegraph` on a GREEN `cycles_cert`, else SQLite); cycles are excluded from
//!   the bounded orient cert below either way.
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
//! the served methods) would catch any regression that took it. The decorator is only constructed on a
//! GREEN bounded cert; on RED/non-resident/non-TS the daemon calls the bare storage (the unchanged eager
//! path), so this decorator is never the fallback path itself.

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
/// - **MODULE_SUMMARY counts** — SQLite-LABELLED (REVISED DR-CLS-2 -> A); not LG-derivable no-loss.
/// - **cycles** — EXCLUDED (RATIFIED CYCLES-A `e363c55`): the set-based `cycles_cert` does not license a
///   byte-identical serve into orient's order-sensitive cycle output; cycles stay SQLite-served. See the
///   module header.
///
/// SHARED BY orient + explain (COHERENCE-LEAF-SERVE-IMPL-2): `handle_explain` reuses this SAME bounded
/// cert to gate the SAME decorator (two consumers now; name kept orient-scoped — a rename is cosmetic).
pub fn orient_bounded_cert_is_green(repo_state: &RepoState, snapshot_uid: &str) -> bool {
    focus_resolution_is_green(repo_state, snapshot_uid)
        && callgraph_is_green(repo_state, snapshot_uid)
}

/// W-B-EPOCH-IMPL-1 (D-EP capture for orient/explain; `daemon-w-b-epoch-1.md` §5.1/§6.4): the BOUNDED-cert
/// LG-serve eligibility WITNESS, captured BUILD-THEN-PEEK — the sibling of
/// `callgraph_cert::callgraph_cert_eligibility` over the bounded (FOCUS-RESOLUTION ∧ CALLGRAPH) cert.
/// Returns `Some(current_fp)` iff BOTH sub-certs are GREEN at EXACTLY the resident fingerprint for
/// `snapshot_uid` (so the decorator-served (b) leaves are cert-proven no-loss-equal to SQLite@`snapshot_uid`);
/// otherwise `None` ⇒ orient/explain run over BARE SQLite (the unchanged eager path).
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
}

impl<'a, S: AgentStorageRead + GateStorageRead + ?Sized> OrientServeDecorator<'a, S> {
    /// Wrap `inner` (the SQLite port) with `livegraph` (the (b)-leaf value source) + the captured request
    /// `epoch` (the EV-A serve-time validation pin). Constructed by the daemon ONLY when the bounded orient
    /// cert eligibility is `Some` (the same GREEN-bounded-cert precondition as before).
    pub fn new(
        livegraph: &'a parking_lot::RwLock<Option<LiveGraph>>,
        inner: &'a S,
        epoch: &'a RequestEpoch,
    ) -> Self {
        Self {
            livegraph,
            inner,
            epoch,
        }
    }

    /// W-B-EPOCH-IMPL-1 (D-EV = EV-A): is the resident LiveGraph still the captured green-validated epoch?
    /// `true` iff `import_cert_fingerprint(resident partitions, epoch.snapshot_uid()) == epoch.fingerprint`.
    /// Computed INSIDE each serve method's read guard (passed `lg`), so the gate and the data read are
    /// atomic w.r.t. a swap (a swap takes `livegraph.write()`). On `false` the serve method delegates to the
    /// pinned SQLite snapshot. Partition epochs are monotonic, so once a swap moves the fingerprint the
    /// match can never spuriously re-appear.
    fn epoch_resident(&self, lg: &LiveGraph) -> bool {
        // RED path (no eligibility witness): never serve the LiveGraph, and SKIP the fingerprint
        // computation. `handle_explain` wraps this decorator even on a RED bounded cert (to pin
        // `get_latest_snapshot`), so this short-circuit keeps the red (b)-leaf serve byte-for-byte the
        // bare-SQLite path — the decorator does no LiveGraph work beyond the (negligible) read guard.
        let Some(captured_fp) = self.epoch.fingerprint.as_ref() else {
            return false;
        };
        let current_fp = import_cert_fingerprint(&lg.live_partitions(), self.epoch.snapshot_uid());
        &current_fp == captured_fp
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
