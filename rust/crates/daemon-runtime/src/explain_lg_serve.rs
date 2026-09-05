//! EXPLAIN-LIVEGRAPH-IMPL: the daemon-side LiveGraph VALUE serving for explain's LG-first leaves.
//!
//! The IMPURE adapter half (Clean Architecture: mechanism). Where the LiveGraph can serve a leaf's VALUE,
//! this builds the explain evidence FROM the migrated LiveGraph surface (NOT from a re-labelled `run_explain`
//! SQLite result — operator decision 2026-06-12: "serve the LG-first leaf VALUES FROM LiveGraph"):
//!
//!   - **EXPLAIN_IDENTITY** — served by the focused sibling [`crate::explain_lg_identity`] (extracted to keep
//!     this module under the 500-line guardrail): serve the ANCHOR (`name`/`subtype`) from current-state
//!     LiveGraph IR (`LiveGraph::node_display`) when the symbol's partition is resident + Fresh + TS, keeping
//!     the snapshot-scoped coordinate fields SQLite → multi-source `{livegraph, sqlite}`; a failed attempt is
//!     a LABELLED SQLite fallback (D-EXPLAIN-IDENTITY).
//!   - **EXPLAIN_IMPORTS** — rebuild the `target_file` list from `LiveGraph::live_import_view` when the
//!     per-file residency precondition + the repo-wide import no-loss cert are green. → `{livegraph}`.
//!   - **EXPLAIN_CYCLES** — rebuild the filtered cycle list from `LiveGraph::module_import_cycles` when the
//!     repo-wide module-cycle no-loss cert is green. → `{livegraph}`.
//!   - **EXPLAIN_CALLERS / EXPLAIN_CALLEES** — the leaf VALUE is genuinely REBUILT from the migrated
//!     `LiveGraph::callers` / `LiveGraph::callees` surfaces (review-4): the caller/callee IDENTITY SET
//!     (`caller_identities` / `callee_identities`) + each item's current-state NAME (`LiveGraph::node_display`
//!     over the resident IR) come FROM the LiveGraph, gated by the per-symbol no-loss key compare. The
//!     per-item owning `module` + the top-3 module grouping + the count + the SQL render order have NO
//!     LiveGraph/IR home (module discovery is a Layer-1/2 SQLite construct — MODULE→FILE `OWNS` edges; the IR
//!     substrate carries no module ownership), so they stay SQLite-rendered. → multi-source
//!     `{livegraph, sqlite}` (the honest D8 split; orient's ratified CALLERS_SUMMARY treatment lifted to a
//!     full item list). NOT a relabel of the SQLite value: the rendered NAME is the live IR name, provably
//!     different from the snapshot name when the symbol drifted.
//!
//! Each `serve_*` returns `(Option<Signal>, Option<OrientLeafLabel>)`: the optional REBUILT signal (the
//! live-served value, with the post-ranking rank + scope re-adopted) and the optional per-leaf POSTURE the
//! pure `explain_to_coherent` labels from. A `Some(SqliteFallback { reason })` label is a labelled SQLite
//! fallback (a committed LG-first attempt that could not serve — the cert-ladder reason records why). A
//! `None` label is the defensive impossible-state guard only (a wrong-typed signal); the no-ATTEMPT
//! unlabelled `{sqlite}` leaf arises when the daemon never calls `serve_*` for that focus (handled in
//! `explain_coherence`, not here). The LiveGraph reads here are gated by the SAME no-loss certs orient/the
//! imports fastpath already build — no new producer.

use std::collections::BTreeMap;

use repo_graph_agent::{
    CycleEvidence, ExplainCalleeItem, ExplainCalleesEvidence, ExplainCallerItem,
    ExplainCallersEvidence, ExplainCyclesEvidence, ExplainImportItem, ExplainImportsEvidence,
    OrientLeafLabel, Signal,
};
use repo_graph_coherence::CoherenceFallbackReason;
use repo_graph_trust_model::Granularity;

use crate::explain_coherence::explain_imports_outcome;
use crate::orient_coherence::map_outcome;
use crate::orient_lg_decisions::{
    orient_callees_outcome, orient_callers_outcome, orient_cycles_outcome, OrientLgOutcome,
};
use crate::state::RepoState;

/// The item cap mirrors the agent's `items_cap` (explain/mod.rs): Medium = 15, Large = 50. explain floors
/// Small → Medium, so the effective minimum is 15.
pub(crate) fn explain_items_cap(budget_large: bool) -> usize {
    if budget_large {
        50
    } else {
        15
    }
}

/// Apply the agent's `truncate_items` contract: cap the vec, return `(items_truncated, items_omitted_count)`
/// (both `None` when within cap). Keeps the served value byte-identical to the SQLite pipeline's truncation.
fn truncate<T>(items: &mut Vec<T>, cap: usize) -> (Option<bool>, Option<u64>) {
    if items.len() <= cap {
        (None, None)
    } else {
        let omitted = (items.len() - cap) as u64;
        items.truncate(cap);
        (Some(true), Some(omitted))
    }
}

/// EXPLAIN_IMPORTS (file focus): rebuild the `target_file` list FROM `LiveGraph::live_import_view`.
///
/// The repo-wide import no-loss cert + the per-file residency precondition (`explain_imports_outcome`) gate
/// the `livegraph` posture; when green the value is BUILT from the live import edges (the importing file's
/// captured FILE→FILE edges, in the view's sorted order) → single-source `{livegraph}`. On fallback the SQLite
/// primary is kept, labelled with the cert reason.
pub(crate) fn serve_imports(
    repo_state: &RepoState,
    file: &str,
    repo_uid: &str,
    snapshot_uid: &str,
    original: &Signal,
    budget_large: bool,
) -> (Option<Signal>, Option<OrientLeafLabel>) {
    let outcome = explain_imports_outcome(repo_state, file, repo_uid, snapshot_uid);
    match &outcome {
        OrientLgOutcome::Livegraph { .. } => {}
        OrientLgOutcome::Fallback { .. } => return (None, Some(map_outcome(outcome))),
    }

    // Build the value from the live import view (the importing file's captured FILE -> FILE edges).
    let targets: Vec<String> = {
        let guard = repo_state.livegraph.read();
        let Some(lg) = guard.as_ref() else {
            // Raced to None after the cert gate; fall back to the SQLite primary, labelled.
            return (
                None,
                Some(OrientLeafLabel::SqliteFallback {
                    reason: repo_graph_coherence::CoherenceFallbackReason::LiveGraphUnavailable,
                }),
            );
        };
        let view = lg.live_import_view(Some(file));
        view.edges
            .into_iter()
            .filter(|e| e.src_file == file)
            .map(|e| e.dst_file)
            .collect()
    };
    let count = targets.len() as u64;
    let cap = explain_items_cap(budget_large);
    let mut items: Vec<ExplainImportItem> = targets
        .into_iter()
        .map(|target_file| ExplainImportItem { target_file })
        .collect();
    let (items_truncated, items_omitted_count) = truncate(&mut items, cap);
    let served = Signal::explain_imports(ExplainImportsEvidence {
        count,
        items,
        items_truncated,
        items_omitted_count,
    });
    (
        Some(original.adopt_rank_and_scope(served)),
        Some(lg_posture()),
    )
}

/// EXPLAIN_CYCLES (symbol module-context / path focus): rebuild the filtered cycle list FROM
/// `LiveGraph::module_import_cycles`.
///
/// The repo-wide module-cycle no-loss cert (`orient_cycles_outcome`) is a FIELD-EXACT whole-value proof; when
/// green the value is BUILT from the live module cycles filtered to those involving `target` (membership for
/// the symbol module-context focus, OR a path-prefix member for the path focus) → single-source `{livegraph}`.
/// On fallback the SQLite primary is kept, labelled with the cert reason.
pub(crate) fn serve_cycles(
    repo_state: &RepoState,
    snapshot_uid: &str,
    target: &str,
    is_path_focus: bool,
    original: &Signal,
    budget_large: bool,
) -> (Option<Signal>, Option<OrientLeafLabel>) {
    let outcome = orient_cycles_outcome(repo_state, snapshot_uid);
    match &outcome {
        OrientLgOutcome::Livegraph { .. } => {}
        OrientLgOutcome::Fallback { .. } => return (None, Some(map_outcome(outcome))),
    }

    let mut cycles: Vec<CycleEvidence> = {
        let guard = repo_state.livegraph.read();
        let Some(lg) = guard.as_ref() else {
            return (
                None,
                Some(OrientLeafLabel::SqliteFallback {
                    reason: repo_graph_coherence::CoherenceFallbackReason::LiveGraphUnavailable,
                }),
            );
        };
        let answer = lg.module_import_cycles();
        let all = answer.data().map(|d| d.cycles.clone()).unwrap_or_default();
        // EC-M2-LEAF-SERVE-1 (CYCLES-B): CANONICALIZE through the SAME `canonicalize_cycles` the
        // agent applies to its own (SQLite- or decorator-served) cycle value — members sorted, list
        // length-DESC — BEFORE the budget cut. Without this the rebuild rendered raw Tarjan member
        // order, so a green rebuild could differ byte-wise from the agent's canonical value; with
        // it, both are the same pure function of the cert-proven-equal cycle set.
        let mut agent_cycles: Vec<repo_graph_agent::AgentCycle> = all
            .into_iter()
            .filter(|c| cycle_involves(&c.members, target, is_path_focus))
            .map(|c| repo_graph_agent::AgentCycle {
                length: c.members.len(),
                modules: c.members,
                // ORIENT-CYCLES-DISAGREE-1: explain's focus-scoped LiveGraph cycle serve — no
                // is_test reach (§2.3) and not the repo headline; no test-only split claimed.
                test_composition: None,
                type_only: None,
            })
            .collect();
        repo_graph_agent::ordering::canonicalize_cycles(&mut agent_cycles);
        agent_cycles
            .into_iter()
            .map(|c| CycleEvidence {
                length: c.length,
                modules: c.modules,
                // TYPE-ONLY-IMPORTS-1: LiveGraph explain serve — the fact is not reachable here
                // (`None`), carried through honestly (the packet forbids the warm path).
                type_only: c.type_only,
            })
            .collect()
    };
    if cycles.is_empty() {
        // `serve_cycles` is only called when the agent EMITTED a (non-empty) EXPLAIN_CYCLES section, so an
        // empty live filter means the live module-cycle representation (dirname paths) did not reproduce the
        // SQLite-rendered subset (e.g. the symbol-focus filter target is a DISCOVERED module qualified name,
        // not a dirname — the Layer-1/2 vs LiveGraph module-identity gap). The HONESTY LAW forbids labelling
        // the kept SQLite value `{livegraph}` (the rendered cycles were built by SQLite). Serve the proven
        // SQLite primary, labelled `{sqlite}` + a divergence reason. NEVER a false LiveGraph value.
        return (
            None,
            Some(OrientLeafLabel::SqliteFallback {
                reason: repo_graph_coherence::CoherenceFallbackReason::LiveGraphCycleDivergence,
            }),
        );
    }
    let count = cycles.len() as u64;
    let cap = explain_items_cap(budget_large);
    let (items_truncated, items_omitted_count) = truncate(&mut cycles, cap);
    let served = Signal::explain_cycles(ExplainCyclesEvidence {
        count,
        items: cycles,
        items_truncated,
        items_omitted_count,
    });
    (
        Some(original.adopt_rank_and_scope(served)),
        Some(lg_posture()),
    )
}

/// Does a module cycle (its `members`, repo-relative dir paths) involve the explain `target`?
///
/// - symbol module-context focus: the target module is a member of the cycle.
/// - path focus: some member is at-or-under the path prefix (`m == target` or `m` starts with `target/`),
///   mirroring the agent's path-scoped cycle filter.
fn cycle_involves(members: &[String], target: &str, is_path_focus: bool) -> bool {
    if is_path_focus {
        let prefix = format!("{target}/");
        members
            .iter()
            .any(|m| m == target || m.starts_with(&prefix))
    } else {
        members.iter().any(|m| m == target)
    }
}

/// EXPLAIN_CALLERS (symbol focus): REBUILD the leaf VALUE from the migrated `LiveGraph::callers` surface.
///
/// When the per-symbol no-loss key compare (`orient_callers_outcome`) is GREEN — proving the LiveGraph caller
/// key set equals the SQLite key set — the value is rebuilt: the caller IDENTITY SET and each item's
/// current-state NAME come from `LiveGraph::callers` (`caller_identities`) and `LiveGraph::node_display` (the
/// resident IR symbol-attributes substrate). The per-item owning `module`, the `top_modules` grouping, the
/// full `count`, and the SQL render order stay SQLite (module discovery has no LiveGraph/IR home), so the leaf
/// is multi-source `{livegraph, sqlite}` — the served NAME is the live IR name, provably NOT a relabelled
/// SQLite value. On the ladder/gate fallback the proven SQLite primary is kept, labelled with the cert reason;
/// on a live-vs-SQLite divergence after the gate (TOCTOU) the SQLite primary is kept, labelled a callgraph
/// divergence — never a false LiveGraph value.
pub(crate) fn serve_callers(
    repo_state: &RepoState,
    target: &str,
    snapshot_uid: &str,
    original: &Signal,
) -> (Option<Signal>, Option<OrientLeafLabel>) {
    let outcome = orient_callers_outcome(repo_state, target, snapshot_uid);
    match &outcome {
        OrientLgOutcome::Livegraph { .. } => {}
        OrientLgOutcome::Fallback { .. } => return (None, Some(map_outcome(outcome))),
    }
    let Some(ev) = original.explain_callers_evidence() else {
        return (None, None);
    };
    // The LIVE caller identity set (the migrated `callers` surface) -> per-key current-state IR name.
    let Some(live_names) = live_callgraph_names(repo_state, target, CallgraphDirection::Callers)
    else {
        return (
            None,
            Some(sqlite_fallback(
                CoherenceFallbackReason::LiveGraphUnavailable,
            )),
        );
    };
    // The SQLite primary's rendered rows (SQL order + module) — the snapshot half of the multi-source leaf.
    let sqlite_rows: Vec<RebuiltRow> = ev
        .items
        .iter()
        .map(|it| {
            (
                it.stable_key.clone(),
                it.name.clone(),
                it.module.clone(),
                // ANCHORS-EVERYWHERE-1: file + line ride from the SQLite base evidence.
                it.file.clone(),
                it.line,
            )
        })
        .collect();
    // Rebuild the rendered rows: LIVE name per item (fallback to the SQLite name when the IR cannot name it),
    // SQLite module + file + line + SQL order preserved. `None` => the live set diverged from the SQLite primary.
    let Some(rows) = rebuild_identity_rows(&live_names, ev.count, &sqlite_rows) else {
        return (
            None,
            Some(sqlite_fallback(
                CoherenceFallbackReason::LiveGraphCallgraphDivergence,
            )),
        );
    };
    let items: Vec<ExplainCallerItem> = rows
        .into_iter()
        .map(|(stable_key, name, module, file, line)| ExplainCallerItem {
            stable_key,
            name,
            module,
            file,
            line,
        })
        .collect();
    let served = Signal::explain_callers(ExplainCallersEvidence {
        count: ev.count,
        top_modules: ev.top_modules,
        items,
        items_truncated: ev.items_truncated,
        items_omitted_count: ev.items_omitted_count,
    });
    (
        Some(original.adopt_rank_and_scope(served)),
        Some(lg_posture()),
    )
}

/// EXPLAIN_CALLEES (symbol focus): the dual of [`serve_callers`] over the migrated `LiveGraph::callees`
/// surface. A callee whose defining partition is non-resident has no live IR node, so its NAME falls back to
/// the SQLite name (the callee SET is still LiveGraph-derived; the residency asymmetry only affects naming) —
/// an honest multi-source leaf.
pub(crate) fn serve_callees(
    repo_state: &RepoState,
    target: &str,
    snapshot_uid: &str,
    original: &Signal,
) -> (Option<Signal>, Option<OrientLeafLabel>) {
    let outcome = orient_callees_outcome(repo_state, target, snapshot_uid);
    match &outcome {
        OrientLgOutcome::Livegraph { .. } => {}
        OrientLgOutcome::Fallback { .. } => return (None, Some(map_outcome(outcome))),
    }
    let Some(ev) = original.explain_callees_evidence() else {
        return (None, None);
    };
    let Some(live_names) = live_callgraph_names(repo_state, target, CallgraphDirection::Callees)
    else {
        return (
            None,
            Some(sqlite_fallback(
                CoherenceFallbackReason::LiveGraphUnavailable,
            )),
        );
    };
    let sqlite_rows: Vec<RebuiltRow> = ev
        .items
        .iter()
        .map(|it| {
            (
                it.stable_key.clone(),
                it.name.clone(),
                it.module.clone(),
                // ANCHORS-EVERYWHERE-1: file + line ride from the SQLite base evidence.
                it.file.clone(),
                it.line,
            )
        })
        .collect();
    let Some(rows) = rebuild_identity_rows(&live_names, ev.count, &sqlite_rows) else {
        return (
            None,
            Some(sqlite_fallback(
                CoherenceFallbackReason::LiveGraphCallgraphDivergence,
            )),
        );
    };
    let items: Vec<ExplainCalleeItem> = rows
        .into_iter()
        .map(|(stable_key, name, module, file, line)| ExplainCalleeItem {
            stable_key,
            name,
            module,
            file,
            line,
        })
        .collect();
    let served = Signal::explain_callees(ExplainCalleesEvidence {
        count: ev.count,
        top_modules: ev.top_modules,
        items,
        items_truncated: ev.items_truncated,
        items_omitted_count: ev.items_omitted_count,
    });
    (
        Some(original.adopt_rank_and_scope(served)),
        Some(lg_posture()),
    )
}

/// Which callgraph direction to read from the LiveGraph (the tuple key position differs).
#[derive(Clone, Copy)]
enum CallgraphDirection {
    Callers,
    Callees,
}

/// The LIVE callgraph identity set: each caller/callee key from the migrated `LiveGraph::callers`/`callees`
/// surface mapped to its current-state IR display name (`node_display`, `None` when no resident partition
/// defines it — the callees residency asymmetry). Reads the LiveGraph ONCE under the read lock (both the
/// identity set and the per-key names), then drops it. `None` when no LiveGraph is resident or the answer
/// carries no data (raced to `None`/Unavailable after the gate) → the caller labels a SQLite fallback.
fn live_callgraph_names(
    repo_state: &RepoState,
    target: &str,
    direction: CallgraphDirection,
) -> Option<BTreeMap<String, Option<String>>> {
    let guard = repo_state.livegraph.read();
    let lg = guard.as_ref()?;
    // Each key: callers => the `(partition, key)` tuple's `.1`; callees => the `(key, partition)` tuple's `.0`.
    let keys: Vec<String> = match direction {
        CallgraphDirection::Callers => {
            let env = lg.callers(target, Granularity::CallerDetail);
            env.data()?
                .caller_identities
                .iter()
                .map(|(_, k)| k.clone())
                .collect()
        }
        CallgraphDirection::Callees => {
            let env = lg.callees(target, Granularity::CallerDetail);
            env.data()?
                .callee_identities
                .iter()
                .map(|(k, _)| k.clone())
                .collect()
        }
    };
    let mut out = BTreeMap::new();
    for key in keys {
        let name = lg
            .node_display(&repo_graph_ir::CanonicalKey::from_existing(&key))
            .map(|(n, _)| n);
        out.insert(key, name);
    }
    Some(out)
}

/// Apply the LIVE callgraph identity set to the SQLite-rendered rows: serve each item's LIVE name
/// (`node_display`, falling back to the SQLite name when the IR cannot name it) while preserving the SQLite
/// `module` + SQL render order. Returns the rebuilt `(stable_key, name, module)` rows, or `None` when the live
/// set DIVERGES from the SQLite primary (the full count differs, or a rendered key is absent from the live
/// set) — the caller then keeps the proven SQLite primary, labelled a callgraph divergence (never a false
/// LiveGraph value). The no-loss gate already proved set-equality, so `None` is a TOCTOU-defensive guard.
///
/// ANCHORS-EVERYWHERE-1 (source-of-truth rule): each row also carries `file` + `line`. Both
/// come UNCHANGED from the SQLite base evidence (`sqlite_rows`) — the live IR recomputes only
/// the NAME. So the rendered `file:line` anchor is always a single-source SQLite pair, never a
/// live-IR name spliced onto a foreign line (STANDING HONESTY RULE #2).
fn rebuild_identity_rows(
    live_names: &BTreeMap<String, Option<String>>,
    sqlite_count: u64,
    sqlite_rows: &[RebuiltRow],
) -> Option<Vec<RebuiltRow>> {
    // The full caller/callee count (pre-truncation) must match the live identity-set size.
    if sqlite_count != live_names.len() as u64 {
        return None;
    }
    let mut rows = Vec::with_capacity(sqlite_rows.len());
    for (key, sqlite_name, module, file, line) in sqlite_rows {
        // A rendered key absent from the live set => divergence (no false LiveGraph value).
        let live = live_names.get(key)?;
        let name = live.clone().unwrap_or_else(|| sqlite_name.clone());
        // file + line stay the SQLite pair; only `name` is the live-IR value.
        rows.push((key.clone(), name, module.clone(), file.clone(), *line));
    }
    Some(rows)
}

/// ANCHORS-EVERYWHERE-1: one rebuilt caller/callee row — `(stable_key, name, module, file, line)`.
/// `name` is the live-IR value on the rebuild path; `module`/`file`/`line` are the SQLite base pair
/// (the anchor's single source). Aliased so the 5-tuple has one name across the three use sites.
type RebuiltRow = (String, String, Option<String>, Option<String>, Option<u64>);

/// A labelled SQLite fallback leaf decision (the proven SQLite primary is kept; the reason records why the
/// LiveGraph could not serve the rebuilt value).
fn sqlite_fallback(reason: CoherenceFallbackReason) -> OrientLeafLabel {
    OrientLeafLabel::SqliteFallback { reason }
}

/// The `Livegraph` posture for a leaf served from a green field-exact cert / resident anchor: Exact + Fresh +
/// Complete + TS-only. The pure conversion projects these axes verbatim; the per-code single-vs-multi source
/// split is owned by the agent (`explain_livegraph_served_is_multi_source`).
///
/// `pub(crate)` so the focused [`crate::explain_lg_identity`] module reuses the IDENTICAL served posture for
/// the identity anchor leaf (no forked posture per leaf).
pub(crate) fn lg_posture() -> OrientLeafLabel {
    use repo_graph_trust_model::{AnswerClass, FreshnessState, LanguageSupport, QueryCompleteness};
    OrientLeafLabel::Livegraph {
        class: AnswerClass::Exact,
        completeness: QueryCompleteness::Complete,
        freshness: FreshnessState::Fresh,
        degradation_reasons: Vec::new(),
        contributing_languages: std::collections::BTreeSet::from([
            LanguageSupport::TypeScriptPrimary,
        ]),
    }
}

#[cfg(test)]
#[path = "explain_lg_serve_tests.rs"]
mod tests;
