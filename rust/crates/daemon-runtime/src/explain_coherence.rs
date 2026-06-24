//! EXPLAIN-LIVEGRAPH-IMPL: assemble explain's `CoherenceEnvelope<CoherentOrientResult>` response (operator
//! decision 2026-06-12 — REAL LiveGraph serving, NOT a re-labelled SQLite result).
//!
//! The IMPURE adapter (Clean Architecture: mechanism), mirroring `orient_coherence.rs`. It reads the daemon
//! `RepoState` (the in-memory LiveGraph + the no-loss certs + SQLite) and ORCHESTRATES explain's FIVE LG-first
//! leaves: it GENUINELY SERVES each green leaf's VALUE from the LiveGraph (the value-construction lives in
//! [`crate::explain_lg_serve`]), swaps it into the bare [`OrientResult`] before the pure
//! [`repo_graph_agent::explain_to_coherent`] labels provenance, and supplies the per-leaf POSTURE:
//!   - **EXPLAIN_IDENTITY** — `explain_lg_identity::serve_identity` (focused sibling module) reads the anchor
//!     `name`/`subtype` from current-state LiveGraph IR (`node_display`) when the symbol's partition is
//!     resident + Fresh + TS → `{livegraph, sqlite}`; a failed attempt is a LABELLED SQLite fallback (the
//!     cert-ladder reason — never an unlabelled `{sqlite}`, which is reserved for the no-attempt file/path
//!     listings identity, D-EXPLAIN-LISTINGS).
//!   - **EXPLAIN_IMPORTS** — `serve_imports` rebuilds the `target_file` list from `live_import_view`, gated by
//!     the repo-wide import no-loss cert ([`explain_imports_outcome`]) + the per-file residency precondition →
//!     `{livegraph}`; else a labelled SQLite fallback.
//!   - **EXPLAIN_CYCLES** — `serve_cycles` rebuilds the filtered cycle list from `module_import_cycles`, gated
//!     by the repo-wide module-cycle field-exact cert (`orient_cycles_outcome`) → `{livegraph}`; a live filter
//!     that cannot reproduce the SQLite-rendered subset (the Layer-1/2 vs LiveGraph module-identity gap) falls
//!     back to `{sqlite}` + a divergence reason (NEVER a false LiveGraph value).
//!   - **EXPLAIN_CALLERS / EXPLAIN_CALLEES** — `serve_callers`/`serve_callees` REUSE orient's
//!     `orient_callers_outcome`/`orient_callees_outcome` per-symbol no-loss KEY-SET compare: the LiveGraph
//!     genuinely supplies/corroborates the caller/callee key set, while the rendered per-item `name` + owning
//!     `module` + grouping have no LiveGraph/IR home and stay SQLite-rendered → `{livegraph, sqlite}` (orient's
//!     ratified CALLERS_SUMMARY treatment); else a labelled SQLite fallback.
//!
//! Also supplies the degraded-state `trust_briefing` overlay via the SHARED
//! `orient_coherence::compute_trust_briefing` (the SAME `"CALLS+IMPORTS"` overlay + degraded-only gate
//! `handle_explain` injected before). explain is the SECOND populator of the shared field
//! (D-EXPLAIN-TRUST-BRIEFING), unlike check (which leaves it `None`).
//!
//! A SEPARATE focused module (not appended to the ~6800-line `dispatch.rs`) per the structural guardrail;
//! `handle_explain` just calls [`build_explain_envelope`]. Every `livegraph` member in the assembled
//! provenance is gated by a daemon-side NO-LOSS proof / a current-state IR read — never a bare relabel of a
//! SQLite-built value.

use std::collections::BTreeSet;

use repo_graph_agent::{
    explain_to_coherent, CoherentOrientResult, ExplainLgDecisions, Focus, OrientResult,
    ResolvedKind, Signal, SignalCode,
};
use repo_graph_coherence::CoherenceEnvelope;
use repo_graph_trust_model::{AnswerClass, FreshnessState, LanguageSupport, QueryCompleteness};

use crate::explain_lg_identity;
use crate::explain_lg_serve;
use crate::livegraph_feed::{build_and_store_import_cert, import_cert_fingerprint, FallbackReason};
use crate::orient_coherence::compute_trust_briefing;
use crate::orient_lg_decisions::OrientLgOutcome;
use crate::state::RepoState;

/// Build explain's coherence-wrapped response from the agent's bare [`OrientResult`].
///
/// `repo_uid` is the resolved repo uid; `display_name` is already set on `result` by the handler;
/// `budget_large` selects the item cap (Large = 50, else 15) for the rebuilt LiveGraph values. The returned
/// envelope is what the daemon serializes for `rmap explain`.
///
/// EXPLAIN-LIVEGRAPH-IMPL (operator 2026-06-12): each green LG-first leaf is genuinely SERVED from the
/// LiveGraph — the daemon rebuilds the IMPORTS / CYCLES values from `live_import_view` / `module_import_cycles`
/// and the IDENTITY anchor from `node_display`, and the live caller/callee key-set no-loss compare gates the
/// callgraph leaves — then this swaps the live-served values into the bare result and hands the per-leaf
/// postures to the pure [`explain_to_coherent`], which labels provenance by true construction. NOT a relabel
/// of the SQLite result: the values come from the LiveGraph (or the proven SQLite primary on fallback).
pub(crate) fn build_explain_envelope(
    repo_state: &RepoState,
    repo_uid: &str,
    mut result: OrientResult,
    budget_large: bool,
) -> CoherenceEnvelope<CoherentOrientResult> {
    let snapshot_uid = result.snapshot.clone();

    // `stale` = the backing index is stale. AUTHORITATIVE source: a direct `get_stale_files` read — the
    // SAME budget-/ranking-independent condition orient/check use, so the freshness label is faithful
    // regardless of which signals survived ranking/truncation (the honesty requirement: never derive
    // staleness from a post-budget/truncated signal list; never mint a false `Fresh`). CONSERVATIVE on read
    // error -> STALE (a failed read cannot vouch for freshness). Ambiguous/no-match emit no signals and take
    // the resolution-only path where `stale` is ignored, so this read is harmless there.
    // D-S = S-A: open a fresh per-operation connection (the request's read guard keeps it
    // snapshot-consistent). Open failure cannot vouch for freshness -> conservative STALE.
    let stale = match repo_state.storage() {
        Ok(conn) => match conn.get_stale_files(&snapshot_uid) {
            Ok(files) => !files.is_empty(),
            Err(_) => true,
        },
        Err(_) => true,
    };

    // Which LG-first signals did this focus emit? Only serve those (avoid needless LiveGraph reads + cert
    // builds). Capture the identity's owning module for the symbol-focus cycle filter.
    let mut present = LgPresence::default();
    let mut identity_module: Option<String> = None;
    for s in &result.signals {
        match s.code() {
            SignalCode::ExplainIdentity => {
                present.identity = true;
                identity_module = s.explain_identity_evidence().and_then(|e| e.module_path);
            }
            SignalCode::ExplainCallers => present.callers = true,
            SignalCode::ExplainCallees => present.callees = true,
            SignalCode::ExplainImports => present.imports = true,
            SignalCode::ExplainCycles => present.cycles = true,
            _ => {}
        }
    }

    let mut decisions = ExplainLgDecisions::default();
    // The live-served replacement VALUES; swapped into `result.signals` (by code) after the reads complete.
    let mut replacements: Vec<Signal> = Vec::new();

    // EXPLAIN_IDENTITY (symbol focus): serve the anchor name/subtype from current-state LiveGraph IR.
    if present.identity {
        if let (Some(symbol_key), Some(identity_sig)) = (
            symbol_target(&result.focus),
            find_signal(&result.signals, SignalCode::ExplainIdentity),
        ) {
            let (replacement, label) =
                explain_lg_identity::serve_identity(repo_state, symbol_key, identity_sig);
            decisions.identity = label;
            replacements.extend(replacement);
        }
    }

    // EXPLAIN_CALLERS / EXPLAIN_CALLEES (symbol focus): REBUILD the value from the migrated callers/callees
    // surface (the identity set + live IR names FROM LiveGraph; the per-item module + grouping + count + SQL
    // order from SQLite) → multi-source {livegraph, sqlite}. The per-symbol no-loss key compare gates serving;
    // a non-green ladder / a live divergence keeps the proven SQLite primary, labelled.
    if let Some(target) = symbol_target(&result.focus) {
        if present.callers {
            if let Some(callers_sig) = find_signal(&result.signals, SignalCode::ExplainCallers) {
                let (replacement, label) =
                    explain_lg_serve::serve_callers(repo_state, target, &snapshot_uid, callers_sig);
                decisions.callers = label;
                replacements.extend(replacement);
            }
        }
        if present.callees {
            if let Some(callees_sig) = find_signal(&result.signals, SignalCode::ExplainCallees) {
                let (replacement, label) =
                    explain_lg_serve::serve_callees(repo_state, target, &snapshot_uid, callees_sig);
                decisions.callees = label;
                replacements.extend(replacement);
            }
        }
    }

    // EXPLAIN_IMPORTS (file focus): rebuild the value from `live_import_view` under the import no-loss cert.
    if present.imports {
        if let (Some(file), Some(imports_sig)) = (
            file_target(&result.focus),
            find_signal(&result.signals, SignalCode::ExplainImports),
        ) {
            let (replacement, label) = explain_lg_serve::serve_imports(
                repo_state,
                file,
                repo_uid,
                &snapshot_uid,
                imports_sig,
                budget_large,
            );
            decisions.imports = label;
            replacements.extend(replacement);
        }
    }

    // EXPLAIN_CYCLES (symbol module-context / path focus): rebuild from `module_import_cycles` filtered to the
    // target module / path, under the field-exact module-cycle no-loss cert.
    if present.cycles {
        if let (Some((target, is_path)), Some(cycles_sig)) = (
            cycles_target(&result.focus, identity_module.as_deref()),
            find_signal(&result.signals, SignalCode::ExplainCycles),
        ) {
            let (replacement, label) = explain_lg_serve::serve_cycles(
                repo_state,
                &snapshot_uid,
                &target,
                is_path,
                cycles_sig,
                budget_large,
            );
            decisions.cycles = label;
            replacements.extend(replacement);
        }
    }

    // Swap the live-served VALUES into the bare result (by code) before the pure conversion labels them.
    for replacement in replacements {
        let code = replacement.code();
        if let Some(slot) = result.signals.iter_mut().find(|s| s.code() == code) {
            *slot = replacement;
        }
    }

    let trust_briefing = compute_trust_briefing(repo_state, repo_uid, &snapshot_uid);

    explain_to_coherent(result, &decisions, trust_briefing, stale)
}

/// Which LG-first signals a focus emitted (only those are served).
#[derive(Default)]
struct LgPresence {
    identity: bool,
    callers: bool,
    callees: bool,
    imports: bool,
    cycles: bool,
}

/// The first signal with `code` in `signals` (each LG-first code appears at most once).
fn find_signal(signals: &[Signal], code: SignalCode) -> Option<&Signal> {
    signals.iter().find(|s| s.code() == code)
}

/// The symbol stable key when the focus resolved to a SYMBOL node (IDENTITY/CALLERS/CALLEES); else `None`.
fn symbol_target(focus: &Focus) -> Option<&str> {
    if focus.resolved_kind == Some(ResolvedKind::Symbol) {
        focus.resolved_key.as_deref()
    } else {
        None
    }
}

/// The file path when the focus resolved to a FILE node (the IMPORTS target); else `None`.
fn file_target(focus: &Focus) -> Option<&str> {
    if focus.resolved_kind == Some(ResolvedKind::File) {
        focus.resolved_path.as_deref()
    } else {
        None
    }
}

/// The CYCLES filter target: `(target, is_path_focus)`. Symbol focus filters by the owning module
/// (`identity_module`, exact membership); path/module focus filters by the path prefix.
fn cycles_target(focus: &Focus, identity_module: Option<&str>) -> Option<(String, bool)> {
    match focus.resolved_kind {
        Some(ResolvedKind::Symbol) => identity_module.map(|m| (m.to_string(), false)),
        Some(ResolvedKind::Module) => focus.resolved_path.as_ref().map(|p| (p.clone(), true)),
        _ => None,
    }
}

/// EXPLAIN_IMPORTS (file focus) leaf decision — reuse the repo-wide import no-loss cert (the SAME cert the
/// imports fastpath builds) gated by the per-file residency precondition. Mirrors [`orient_cycles_outcome`]:
/// only labels `livegraph` when the file's owning partition is resident + Fresh + TS AND the repo-wide
/// import cert is GREEN at the current fingerprint (proving no SQLite resolved-local import is LOST). Else a
/// labelled SQLite fallback. SQLite-FREE on the decision path except the one-per-fingerprint cert build.
///
/// This decides the cert GATE only; when GREEN, `explain_lg_serve::serve_imports` BUILDS the EXPLAIN_IMPORTS
/// value from `live_import_view` (genuinely LiveGraph-served → single-source `{livegraph}`). The LiveGraph
/// read guard is DROPPED before the cert build (which re-reads the livegraph + write-locks `import_cert`).
pub(crate) fn explain_imports_outcome(
    repo_state: &RepoState,
    file_path: &str,
    repo_uid: &str,
    snapshot_uid: &str,
) -> OrientLgOutcome {
    let (precond, current_fp) = {
        let guard = repo_state.livegraph.read();
        match guard.as_ref() {
            None => {
                return OrientLgOutcome::Fallback {
                    reason: FallbackReason::LiveGraphUnavailable,
                }
            }
            Some(lg) => {
                let precond = lg.file_partition_status(file_path);
                let fp = import_cert_fingerprint(&lg.live_partitions(), snapshot_uid);
                (precond, fp)
            }
        }
    };

    // The per-file residency precondition: the owning partition must be resident + Fresh + TS. Decompose it
    // into the specific cert-ladder fallback reason (mirrors the `orient_outcome_from_env` freshness-before-
    // language ordering): non-resident -> Partial; non-TS -> UnsupportedLanguage; stale -> Stale.
    match &precond {
        None => {
            return OrientLgOutcome::Fallback {
                reason: FallbackReason::LiveGraphPartial,
            }
        }
        Some(p) if !p.fresh => {
            return OrientLgOutcome::Fallback {
                reason: FallbackReason::LiveGraphStale,
            }
        }
        Some(p) if !p.ts_primary => {
            return OrientLgOutcome::Fallback {
                reason: FallbackReason::LiveGraphUnsupportedLanguage,
            }
        }
        Some(_) => {} // resident + Fresh + TS -> precondition met; the cert gates the final label.
    }

    // The repo-wide import no-loss cert gates the FINAL livegraph label. Stale/missing -> (re)build once per
    // fingerprint. A non-GREEN repo-wide cert means at least one TS file has a lost/ambiguous import, so the
    // CONSERVATIVE gate falls back to the proven SQLite primary for this file too (labelled).
    let cert_green = {
        let cached = repo_state.import_cert.read();
        match cached.as_ref() {
            Some(c) if c.fingerprint == current_fp => Some(c.verdict == "GREEN"),
            _ => None,
        }
    };
    let green = match cert_green {
        Some(g) => g,
        None => build_and_store_import_cert(repo_state, repo_uid, snapshot_uid, Some(current_fp))
            .unwrap_or(false),
    };
    if green {
        OrientLgOutcome::Livegraph {
            class: AnswerClass::Exact,
            completeness: QueryCompleteness::Complete,
            freshness: FreshnessState::Fresh,
            degradation_reasons: Vec::new(),
            contributing_languages: BTreeSet::from([LanguageSupport::TypeScriptPrimary]),
        }
    } else {
        // Conservative repo-wide-cert-not-GREEN label: the proven SQLite primary is served (never a loss);
        // the reason records that the repo carries an import no-loss regression.
        OrientLgOutcome::Fallback {
            reason: FallbackReason::LiveGraphImportRegression,
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════════════════════════
// EXPLAIN-LIVEGRAPH-IMPL: daemon-half LG-SERVED end-to-end proof (the §5 validation: LG-served +
// SQLite-fallback source labels through the REAL `build_explain_envelope`).
//
// Mirrors `orient_lg_decisions::orient_lg_served_e2e`: an in-process `RepoState` with a REAL LiveGraph (the
// committed `synthetic/index.scip`, ingested producer-FREE) + a SQLite that MIRRORS the LiveGraph
// caller/callee key sets, so the reused per-symbol no-loss key compare is GENUINELY GREEN. It proves that
// explain's EXPLAIN_CALLERS / EXPLAIN_CALLEES leaves are assembled as multi-source `{livegraph, sqlite}`
// when the LiveGraph serves (key set live-corroborated, name/module SQLite-rendered), and as a labelled
// `{sqlite}` fallback when no LiveGraph is loaded; EXPLAIN_IDENTITY collapses to `{sqlite}` without an anchor.
// ════════════════════════════════════════════════════════════════════════════════════════════════
#[cfg(test)]
#[path = "explain_coherence_tests.rs"]
mod explain_lg_served_e2e;
