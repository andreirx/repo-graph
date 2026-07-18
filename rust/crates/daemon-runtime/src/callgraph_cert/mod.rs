//! COHERENCE-LEAF-SERVE-IMPL-1: the daemon-side, CACHEABLE, repo-wide CALLGRAPH NO-LOSS certificate.
//!
//! This is the zero-read serve mechanism orient's bounded (b)-leaf fastpath needs for the
//! CALLERS_SUMMARY / CALLEES_SUMMARY leaves. The SHIPPED per-call gate
//! (`orient_lg_decisions::gate_callgraph_no_loss`) reads SQLite `find_symbol_callers`/`callees` on
//! EVERY call to prove value-equivalence — disqualifying for "ZERO eager `edges` read on green". This
//! cert mirrors `cycles_cert` / `focus_resolution_cert` instead: it compares the LiveGraph rows to the
//! SQLite rows ONCE per fingerprint over the resident∪SQLite symbol corpus, caches the verdict, and the
//! consumer serves the LiveGraph rows on a cached GREEN WITHOUT touching SQLite.
//!
//! **What the cert proves (the no-loss contract).** GREEN iff, for EVERY symbol key in the UNION corpus
//! (the LiveGraph's AST-adopted symbol keys `focus_corpus().symbol_keys` ∪ the SQLite `SYMBOL` node
//! keys `query_all_nodes`), the LiveGraph caller rows AND callee rows are field-exact equal — as
//! MULTISETS (sorted-vector compare, so multiplicity AND content are proven, never a set that hides a
//! repeated edge) — to the SQLite `find_symbol_callers` / `find_symbol_callees` rows. Driving the
//! compare from the UNION makes it BIDIRECTIONAL: an LG caller SQLite lacks AND a SQLite caller the LG
//! lacks (e.g. an unresolved / non-resident callee the LG cannot enrich) each force RED -> the SQLite
//! fallback. This is the multiset-equivalence proof that licenses the COHERENCE-LEAF-SERVE consumer
//! (`orient_serve`) to serve a LiveGraph-built caller/callee row list (and skip the eager `edges` read)
//! on green; RED -> the existing SQLite read.
//!
//! **The row enrichment is no-loss by construction (+ re-proven here).** SQLite `find_symbol_callers`
//! derives `(name, file, module_path, module_stable_key)` via a FILE->OWNS->MODULE join
//! (`storage::agent_impl`). The LiveGraph reproduces EXACTLY that join in `symbol_context`
//! (`focus_resolver`: `name` from the node, `file`/`module` from the key's path segment + the derived
//! directory-MODULE model). `focus_resolution_cert` already proves `symbol_context` field-equal to
//! SQLite `get_symbol_context`; this cert RE-PROVES it for every caller/callee (the full-row compare
//! subsumes the enrichment), so the serve is self-contained no-loss. The orient bounded cert AND-folds
//! BOTH certs, so the enrichment is doubly gated.
//!
//! **The serve-ladder accessor.** [`callgraph_is_green`] mirrors `focus_resolution_is_green` /
//! `cycles_auto_response`'s cert-state ladder EXACTLY — fingerprint reuse via the SHARED
//! [`import_cert_fingerprint`], cached-verdict reuse at a matching fingerprint, lazy (re)build on a
//! stale/missing cert, fingerprint-mismatch invalidation. SQLite is read ONLY to (re)build the cert; a
//! cached GREEN/RED at the current fingerprint reads NO SQLite.
//!
//! **Boundaries.** NO new dependency edge: `daemon-runtime` already depends on `repo-graph-agent` (the
//! `AgentStorageRead` SQLite parity target + the `AgentCallerRow`/`AgentCalleeRow` DTOs) and
//! `repo-graph-livegraph` (the `callers`/`callees`/`symbol_context` surfaces). NO new SQLite surface:
//! the compare reuses `find_symbol_callers`/`callees` + `query_all_nodes` (the SAME read
//! `focus_resolution_cert` uses).
//!
//! **RECON-M-R1.** The compare is now the [`ledger`] module's FULL exhaustive walk (no
//! first-divergence short-circuit): the stored GREEN/RED verdict is DERIVED from the resulting
//! witness ledger — byte-compatible on every path that completes — and the ledger (the
//! instance-level, kind-aligned witness classification M-R2/M-R3 consume) is stored beside the
//! cert under the same fingerprint key and lifecycle. Serving is UNCHANGED at M-R1: GREEN still
//! licenses the byte-substitute serve, RED still forces the SQLite fallback, and the
//! `callgraph_cert_eligibility` capture stays GREEN-gated byte-exact (the capture contract flips
//! only at M-R2 — recon-design-1 §4.2/§5.1).
//!
//! **RECON-M-R2 (flag-gated).** [`callgraph_union_eligibility`] is the redefined capture for the
//! callers/callees UNION path: LEDGER-validity-gated, verdict-independent (§4.2 activation) — it
//! rides the `union_serve` flag and is called ONLY on the flag-ON `Auto` arm. The GREEN-gated
//! capture above stays byte-exact for every other consumer (the default path + the bounded-orient
//! cert's LG-as-byte-substitute serving keep their GREEN gate — §5.1 "untouched" list). A ledger
//! build failure is retained on `RepoState::witness_ledger_build_failure` (transient 2; doctor's
//! rendering lands with M-R3a).

use repo_graph_agent::{AgentCalleeRow, AgentCallerRow};
use repo_graph_livegraph::LiveGraph;
use repo_graph_trust_model::{AnswerClass, Granularity};

use crate::livegraph_feed::import_cert_fingerprint;
use crate::state::RepoState;

/// RECON-SPIKE-1: additive, env-gated per-symbol divergence emission (off by default; the GREEN/RED
/// verdict below is UNCHANGED). See [`diff`] for the gating + artifact contract.
mod diff;

/// RECON-M-R1: the WITNESS LEDGER — the cert compare generalized into the full-walk, per-fingerprint
/// witness-agreement classification. The stored GREEN/RED verdict is now DERIVED from it
/// ([`ledger::WitnessLedger::derived_green`] — behavior byte-unchanged); the shared comparison
/// primitives live there and [`diff`] consumes them for its env-gated artifact.
pub(crate) mod ledger;

/// RECON-M-R1 gate tests (instance fixtures, regime matrix, collision guard, capture parity, the
/// committed-fixture 7/0/2/9 reproduction + per-kind record).
#[cfg(test)]
mod ledger_tests;
#[cfg(test)]
pub(crate) mod test_fixture;
#[cfg(test)]
mod tests;

/// COHERENCE-LEAF-SERVE-IMPL-1: the in-memory repo-wide CALLGRAPH NO-LOSS certificate (mirrors
/// `CycleNoLossCert` / `FocusResolutionNoLossCert`). `verdict == "GREEN"` iff the LiveGraph
/// caller/callee rows are field-exact equal (as multisets) to the SQLite rows over the corpus;
/// `fingerprint` is the SHARED SQLite-free fingerprint it was built at — a fingerprint mismatch
/// invalidates + rebuilds. NOT durable (rebuilt on restart).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallgraphNoLossCert {
    /// The repo-wide field-exact compare verdict (`GREEN` / `RED`).
    pub verdict: String,
    /// The SQLite-free fingerprint this verdict was computed at (the invalidation key).
    pub fingerprint: String,
}

/// The callgraph cert's status for the CURRENT fingerprint (mirrors `FocusCertState` / `CycleCertState`).
enum CallgraphCertState {
    /// A valid cert at the current fingerprint, verdict GREEN -> the consumer may serve LiveGraph.
    ValidGreen,
    /// A valid cert at the current fingerprint, verdict != GREEN -> SQLite.
    ValidNotGreen,
    /// No cert, or a cert at a DIFFERENT fingerprint -> (re)build, else SQLite.
    StaleOrMissing,
}

// ── LiveGraph caller/callee ROW builders (SHARED with the `orient_serve` decorator) ─────────────
//
// These build the EXACT `AgentCallerRow`/`AgentCalleeRow` the SQLite `find_symbol_callers`/`callees`
// return, from the LiveGraph `callers`/`callees` key sets + `symbol_context` enrichment. The cert
// compares them to SQLite (the no-loss proof); the decorator serves them VERBATIM on green. Sharing
// the builder makes "what the cert proved" and "what the consumer serves" the SAME bytes — no drift.

/// The FILE->OWNS->MODULE enrichment a caller/callee row carries (name + owning file + module
/// identity) — the SAME fields SQLite `find_symbol_callers`/`callees` join in, reproduced by the
/// LiveGraph `symbol_context`.
struct RowEnrichment {
    name: String,
    file: Option<String>,
    module_path: Option<String>,
    module_stable_key: Option<String>,
}

/// The [`RowEnrichment`] for a symbol key, from the LiveGraph `symbol_context`. `None` when the key is
/// not a resident AST-adopted symbol OR the answer is non-Exact -> the cert treats it as a divergence
/// (RED -> SQLite fallback); never served wrong.
fn lg_symbol_enrichment(lg: &LiveGraph, key: &str) -> Option<RowEnrichment> {
    let env = lg.symbol_context(key);
    if env.class() != AnswerClass::Exact {
        return None;
    }
    let ctx = env.data()?.as_ref()?;
    Some(RowEnrichment {
        name: ctx.name.clone(),
        file: ctx.file_path.clone(),
        module_path: ctx.module_path.clone(),
        module_stable_key: ctx.module_key.clone(),
    })
}

/// Build the LiveGraph CALLER rows for `target` — one row per incoming LiveGraph edge, KIND-BLIND
/// (RECON-M-R1 §3.7-2 correction: `LiveGraph::callers` traverses `ir.edges` with NO `EdgeType`
/// filter, so `References`/`Imports` edges mint rows too — while the SQLite side it mirrors IS
/// `CALLS`-filtered; on any semantically-enriched graph the byte-equality compare therefore reads
/// the kind surplus as divergence -> RED. The kind filter is the named M-R2 prerequisite,
/// recon-design-1 §3.4-3). Multiplicity preserved (mirroring the un-DISTINCT SQLite
/// `find_symbol_callers`). `None` when the `callers` answer is non-Exact or any caller cannot be
/// enriched (the cert reads that as a divergence -> RED).
pub(crate) fn lg_caller_rows(lg: &LiveGraph, target: &str) -> Option<Vec<AgentCallerRow>> {
    let env = lg.callers(target, Granularity::CallerDetail);
    if env.class() != AnswerClass::Exact {
        return None;
    }
    let data = env.data()?;
    let mut rows = Vec::with_capacity(data.caller_identities.len());
    for (_partition, caller_key) in &data.caller_identities {
        let e = lg_symbol_enrichment(lg, caller_key)?;
        rows.push(AgentCallerRow {
            stable_key: caller_key.clone(),
            name: e.name,
            file: e.file,
            module_path: e.module_path,
            module_stable_key: e.module_stable_key,
        });
    }
    Some(rows)
}

/// Build the LiveGraph CALLEE rows for `target` — one row per outgoing LiveGraph edge, KIND-BLIND
/// (RECON-M-R1 §3.7-2 correction, exactly as [`lg_caller_rows`]: no `EdgeType` filter in
/// `LiveGraph::callees`; the M-R2 kind filter is the named fix). Multiplicity preserved (mirroring
/// the un-DISTINCT SQLite `find_symbol_callees`). `None` when the `callees` answer is non-Exact or
/// any callee cannot be enriched (the cert reads that as a divergence -> RED).
pub(crate) fn lg_callee_rows(lg: &LiveGraph, target: &str) -> Option<Vec<AgentCalleeRow>> {
    let env = lg.callees(target, Granularity::CallerDetail);
    if env.class() != AnswerClass::Exact {
        return None;
    }
    let data = env.data()?;
    let mut rows = Vec::with_capacity(data.callee_identities.len());
    for (callee_key, _owner) in &data.callee_identities {
        let e = lg_symbol_enrichment(lg, callee_key)?;
        rows.push(AgentCalleeRow {
            stable_key: callee_key.clone(),
            name: e.name,
            file: e.file,
            module_path: e.module_path,
            module_stable_key: e.module_stable_key,
        });
    }
    Some(rows)
}

// ── Multiset (sorted-vector) row equality ───────────────────────────────────────────────────────
//
// orient consumes callers/callees ORDER-INSENSITIVELY (`build_callers_evidence`: `count = len` +
// `group_by_module`, a HashMap count), so a MULTISET compare is the faithful no-loss test: it proves
// the COUNT (multiplicity) and the per-row fields without demanding the SQLite query's (undefined) row
// order. A bare set would hide a repeated `CALLS` edge -> a wrong count served as no-loss; sorting +
// element compare preserves multiplicity.
//
// RECON-M-R1: the PRODUCTION compare now runs through `ledger::classify` (whose buckets are all
// empty iff these multiset compares return true — the documented equivalence); these helpers remain
// as the SPECIFICATION oracle the tests assert that equivalence against (test-only compiled).

/// The total order over a caller row for the multiset compare (all fields, so the compare is field-exact).
#[cfg(test)]
fn caller_key(r: &AgentCallerRow) -> (&str, &str, Option<&str>, Option<&str>, Option<&str>) {
    (
        r.stable_key.as_str(),
        r.name.as_str(),
        r.file.as_deref(),
        r.module_path.as_deref(),
        r.module_stable_key.as_deref(),
    )
}

/// The total order over a callee row for the multiset compare (all fields, so the compare is field-exact).
#[cfg(test)]
fn callee_key(r: &AgentCalleeRow) -> (&str, &str, Option<&str>, Option<&str>, Option<&str>) {
    (
        r.stable_key.as_str(),
        r.name.as_str(),
        r.file.as_deref(),
        r.module_path.as_deref(),
        r.module_stable_key.as_deref(),
    )
}

/// Caller rows equal as MULTISETS (same length + element-wise equal after a canonical sort).
#[cfg(test)]
pub(crate) fn callers_multiset_eq(lg: &[AgentCallerRow], sq: &[AgentCallerRow]) -> bool {
    if lg.len() != sq.len() {
        return false;
    }
    let mut a: Vec<&AgentCallerRow> = lg.iter().collect();
    let mut b: Vec<&AgentCallerRow> = sq.iter().collect();
    a.sort_by(|x, y| caller_key(x).cmp(&caller_key(y)));
    b.sort_by(|x, y| caller_key(x).cmp(&caller_key(y)));
    a == b
}

/// Callee rows equal as MULTISETS (same length + element-wise equal after a canonical sort).
#[cfg(test)]
pub(crate) fn callees_multiset_eq(lg: &[AgentCalleeRow], sq: &[AgentCalleeRow]) -> bool {
    if lg.len() != sq.len() {
        return false;
    }
    let mut a: Vec<&AgentCalleeRow> = lg.iter().collect();
    let mut b: Vec<&AgentCalleeRow> = sq.iter().collect();
    a.sort_by(|x, y| callee_key(x).cmp(&callee_key(y)));
    b.sort_by(|x, y| callee_key(x).cmp(&callee_key(y)));
    a == b
}

/// RECON-M-R1: build the WITNESS LEDGER for the current graph state (the generalized cert
/// compare — see [`ledger`]). Returns the ledger, or `None` ONLY on a storage error (the caller
/// treats it as "could not reach a verdict" -> nothing stored -> safe SQLite fallback, today's
/// exact `None` contract). The DEGENERATE paths keep today's verdict semantics as data:
///
/// - no resident LiveGraph -> a degenerate ledger, derived verdict RED (the producer cannot
///   corroborate anything -> never GREEN);
/// - no resident partitions -> degenerate, RED (an empty-corpus compare would vacuously pass
///   while SQLite may hold callees -> conservatively NOT green);
/// - both walk-free: no measurement is minted (unknown ≠ zero — every ledger measurement is
///   `None` there).
///
/// Reads SQLite ONCE per fingerprint (`query_all_nodes` + a point `find_symbol_callers`/`callees`
/// per corpus symbol — the cert's exact read set); the GREEN SERVE path reads no `edges` (proven
/// in `orient_serve`). The ONE LiveGraph read guard is held across the whole walk (the same
/// snapshot-consistency discipline the old compare had).
fn build_witness_ledger_outcome(
    repo_state: &RepoState,
    snapshot_uid: &str,
    fingerprint: &str,
) -> Option<ledger::WitnessLedger> {
    let guard = repo_state.livegraph.read();
    let lg = match guard.as_ref() {
        Some(lg) => lg,
        None => {
            return Some(ledger::WitnessLedger::degenerate(
                fingerprint,
                snapshot_uid,
                "no_resident_livegraph",
            ))
        }
    };
    if lg.live_partitions().is_empty() {
        return Some(ledger::WitnessLedger::degenerate(
            fingerprint,
            snapshot_uid,
            "no_resident_partitions",
        ));
    }
    // D-S = S-A: one fresh per-operation connection for the cert-build reads; open failure -> None
    // (NOT green; safe SQLite fallback). The read guard keeps these reads snapshot-consistent.
    let storage = repo_state.storage().ok()?;
    ledger::build_witness_ledger(lg, &storage, snapshot_uid, fingerprint)
}

/// Build the callgraph no-loss cert -> verdict, STORE it keyed by `fingerprint`, return `Some(is_green)`
/// (or `None` if no fingerprint / a storage error -> the caller falls back to SQLite). Mirrors
/// `build_and_store_focus_resolution_cert` / `build_and_store_cycles_cert`.
///
/// RECON-M-R1: the verdict is DERIVED from the witness ledger (`GREEN ⟺ zero divergent symbols ∧
/// zero unanswerable projections ∧ zero field mismatches` on the measured path; degenerate paths
/// RED — behavior byte-unchanged; the ONE walk now always runs exhaustively, the §5.1 priced cost
/// of retaining the classification the one-bit verdict used to discard). The ledger is stored
/// beside the cert under the SAME fingerprint key + lifecycle.
pub(crate) fn build_and_store_callgraph_cert(
    repo_state: &RepoState,
    snapshot_uid: &str,
    fingerprint: Option<String>,
) -> Option<bool> {
    let fingerprint = fingerprint?;
    let built = match build_witness_ledger_outcome(repo_state, snapshot_uid, &fingerprint) {
        Some(b) => b,
        None => {
            // RECON-M-R2 (§4.2 transient 2): RETAIN the build failure so doctor can report
            // "ledger absent + reason" (rendering = M-R3a). The M-R1 `None` contract is
            // SQLite-error-only, so the reason is that class. Serving is unchanged: `None` still
            // stores nothing and the caller falls back to SQLite.
            *repo_state.witness_ledger_build_failure.write() = Some(ledger::LedgerBuildFailure {
                fingerprint,
                reason: "sqlite_error_during_ledger_walk".to_string(),
            });
            return None;
        }
    };
    let is_green = built.derived_green();
    // RECON-SPIKE-1: additive, env-gated (`RMAP_CALLGRAPH_DIFF`) diff emission — off by default (a single
    // `var_os` lookup then return), best-effort, and independent of the verdict derived above/stored
    // below. When the comparison ran (fingerprint present), this captures the per-symbol divergence detail
    // the one-bit verdict discards, plus (M-R1) the ledger summary block. It reads only; it never changes
    // `is_green`.
    diff::maybe_emit(repo_state, snapshot_uid, &fingerprint, is_green, &built);
    let verdict = if is_green { "GREEN" } else { "RED" }.to_string();
    // A successful store supersedes any retained build failure (the transient healed).
    *repo_state.witness_ledger_build_failure.write() = None;
    *repo_state.witness_ledger.write() = Some(built);
    *repo_state.callgraph_cert.write() = Some(CallgraphNoLossCert {
        verdict,
        fingerprint,
    });
    Some(is_green)
}

/// CALLGRAPH serve-ladder accessor — the production primitive the COHERENCE-LEAF-SERVE consumer
/// (`orient_serve`) calls to decide whether its CALLERS_SUMMARY/CALLEES_SUMMARY serve may use the
/// LiveGraph rows (and skip the eager `edges` read). Returns `true` iff the callgraph no-loss cert is
/// GREEN at the CURRENT fingerprint.
///
/// Mirrors `focus_resolution_is_green` EXACTLY: the current fingerprint is the SHARED SQLite-free
/// [`import_cert_fingerprint`] over the resident partitions (NO new invalidation key); a cert whose
/// stored fingerprint equals it is reused as-is (no SQLite read); else a lazy (re)build. Returns
/// `false` (the safe SQLite default) when there is no LiveGraph, no resident partition, or a storage
/// error during the build.
pub fn callgraph_is_green(repo_state: &RepoState, snapshot_uid: &str) -> bool {
    // SQLite-FREE: the current fingerprint from the resident partition snapshot. The read guard is
    // dropped before the lazy build so the build can re-lock without deadlock.
    let current_fp = {
        let guard = repo_state.livegraph.read();
        guard
            .as_ref()
            .map(|lg| import_cert_fingerprint(&lg.live_partitions(), snapshot_uid))
    };
    let state = {
        let cached = repo_state.callgraph_cert.read();
        match (&current_fp, cached.as_ref()) {
            (Some(fp), Some(c)) if &c.fingerprint == fp => {
                if c.verdict == "GREEN" {
                    CallgraphCertState::ValidGreen
                } else {
                    CallgraphCertState::ValidNotGreen
                }
            }
            _ => CallgraphCertState::StaleOrMissing,
        }
    };
    match state {
        CallgraphCertState::ValidGreen => true,
        CallgraphCertState::ValidNotGreen => false,
        CallgraphCertState::StaleOrMissing => {
            build_and_store_callgraph_cert(repo_state, snapshot_uid, current_fp).unwrap_or(false)
        }
    }
}

/// PEEK the callgraph cert WITHOUT building it: `true` iff a cert is ALREADY cached at the CURRENT
/// fingerprint with verdict GREEN. Unlike [`callgraph_is_green`] this NEVER triggers a (re)build — so a
/// consumer can take a zero-read GREEN fast path when the verdict is already known, but NEVER kicks off a
/// repo-wide cert build (and its corpus-wide `callers`/`callees` producer queries) ITSELF.
///
/// This is the accessor the orient LABEL path ([`crate::orient_lg_decisions`]'s `gate_callgraph_label`)
/// uses: on the served-green path `handle_orient`'s bounded-cert precheck already built the cert GREEN, so
/// the peek hits and the leaf labels `livegraph` with zero per-call read; on EVERY other path the peek
/// misses and the label path runs the per-symbol compare (the shipped granularity) — the label path must
/// never own the (build-time, producer-querying) cert construction, only the precheck may.
pub fn callgraph_cached_green(repo_state: &RepoState, snapshot_uid: &str) -> bool {
    let current_fp = {
        let guard = repo_state.livegraph.read();
        match guard.as_ref() {
            Some(lg) => import_cert_fingerprint(&lg.live_partitions(), snapshot_uid),
            None => return false,
        }
    };
    let cached = repo_state.callgraph_cert.read();
    matches!(cached.as_ref(), Some(c) if c.fingerprint == current_fp && c.verdict == "GREEN")
}

/// W-B-EPOCH-IMPL-1 (D-EP capture for callers/callees; `daemon-w-b-epoch-1.md` §5.4/§6.4): the
/// CALLGRAPH-cert LG-serve eligibility WITNESS, captured BUILD-THEN-PEEK. Returns `Some(current_fp)` iff
/// a GREEN callgraph cert exists at EXACTLY the resident fingerprint for `snapshot_uid` — i.e. the
/// resident partitions are cert-proven no-loss-equal to SQLite@`snapshot_uid`, so their callers/callees
/// rows are substitutable for SQLite@`snapshot_uid`; otherwise `None` ⇒ the request serves eager SQLite at
/// the pinned snapshot (the EV-A fail-soft).
///
/// **Why build-then-peek and not just [`callgraph_is_green`].** `callgraph_is_green` computes
/// `current_fp` under one guard, DROPS it, then lazily (re)builds the cert under a RE-locked guard
/// (parking_lot is non-reentrant). Under a future W-B relax a publish could land in that window, so the
/// rebuilt cert can be keyed at the pre-swap fingerprint while its verdict was computed over post-swap
/// partitions — a mislabel. Build-then-peek closes this: (1) WARM (lazy build if stale/missing), then
/// (2) under ONE livegraph read guard — which excludes a concurrent swap (a swap needs `livegraph.write()`)
/// — compute `current_fp` AND peek a GREEN cert at EXACTLY `current_fp`. So `Some(fp)` is the EXACT
/// resident-and-validated state, or `None`. (Under the W-A coordinator this slice ships, no swap can occur
/// mid-request anyway; build-then-peek is the foundation IMPL-3's relax relies on.)
pub fn callgraph_cert_eligibility(repo_state: &RepoState, snapshot_uid: &str) -> Option<String> {
    // 1. WARM: lazy (re)build the cert if stale/missing (drops its own guards before returning).
    let _ = callgraph_is_green(repo_state, snapshot_uid);
    // 2. PEEK under ONE read guard so "(green cert) at (this exact resident fingerprint)" is atomic
    //    w.r.t. any swap.
    let guard = repo_state.livegraph.read();
    let current_fp = import_cert_fingerprint(&guard.as_ref()?.live_partitions(), snapshot_uid);
    let cached = repo_state.callgraph_cert.read();
    match cached.as_ref() {
        Some(c) if c.fingerprint == current_fp && c.verdict == "GREEN" => Some(current_fp),
        _ => None,
    }
}

/// RECON-M-R2: the CAPTURE-CONTRACT FLIP — the LEDGER-VALIDITY-gated, VERDICT-INDEPENDENT capture
/// (recon-design-1 §4.2 activation / §5.1). `Some(current_fp)` ⟺ a MEASURED witness ledger exists
/// at EXACTLY the current resident fingerprint for `snapshot_uid` — the GREEN/RED verdict is NOT
/// consulted, because semantic enrichment GUARANTEES RED and a GREEN-gated capture would make
/// W-BOTH unrepresentable on exactly the repos union serving exists for.
///
/// **Rides the M-R2 union-serving flag** (`union_serve::union_serving_enabled`): the dispatch arms
/// call THIS capture only on the flag-ON `Auto` path; every other path keeps the GREEN-gated
/// [`callgraph_cert_eligibility`] byte-exact (the default flip is its own recorded step, gated on
/// S-1..S-3 — recon-design-1 §6.2).
///
/// Mechanism (build-then-peek, verbatim from the GREEN twin): (1) WARM via `callgraph_is_green` —
/// the SAME lazy build that stores ledger + cert together, so a cert at the current fingerprint
/// implies a ledger at it (one store event); a SQLite error during the build stores NOTHING and is
/// retained on `witness_ledger_build_failure` (§4.2 transient 2). (2) PEEK under ONE livegraph
/// read guard: recompute the fingerprint and require a ledger at exactly it. "Valid ledger" =
/// `classification.is_some()` — a DEGENERATE ledger (no LiveGraph / no partitions) measured
/// nothing and licenses no union serve (decide-and-record; those states fall back through today's
/// `LiveGraphUnavailable` channel anyway).
pub fn callgraph_union_eligibility(repo_state: &RepoState, snapshot_uid: &str) -> Option<String> {
    // 1. WARM (same step as the GREEN twin; verdict ignored — ledger validity is the gate).
    let _ = callgraph_is_green(repo_state, snapshot_uid);
    // 2. PEEK under ONE read guard (atomic w.r.t. a swap — a swap needs `livegraph.write()`).
    let guard = repo_state.livegraph.read();
    let current_fp = import_cert_fingerprint(&guard.as_ref()?.live_partitions(), snapshot_uid);
    let stored = repo_state.witness_ledger.read();
    match stored.as_ref() {
        Some(l) if l.fingerprint == current_fp && l.classification.is_some() => Some(current_fp),
        _ => None,
    }
}
