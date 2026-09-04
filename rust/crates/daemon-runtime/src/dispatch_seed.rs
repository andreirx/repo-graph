//! Seed dispatch surface (EMBED-SEED-IMPL-1 §8/§8B) — the `find` handler, the
//! semantic fallback tier, the `find`-response serializer, and the `next.cwd`
//! canonical-root resolver.
//!
//! **Abstraction note (per repo structural guardrail):** extracted from `dispatch.rs`
//! (review-6 #2) so the new seed responsibility no longer grows the already-oversized
//! dispatcher. A CHILD module of `dispatch`: it reaches the dispatcher's private request
//! helpers (`get_string_param`, `get_optional_string_param`,
//! `resolve_and_load_repo_with_display_name`, the `state` field) via parent visibility,
//! and exposes only what the parent wires — `handle_find`, `apply_semantic_fallback`,
//! `canonical_root`, and the two candidate caps — as `pub(super)`. Concrete current
//! callers, all in `super::`: the `"find"` router arm (`handle_find`), the orient +
//! explain no-match branches (`apply_semantic_fallback`, `SEMANTIC_FALLBACK_CAP`).
//! Axis of variation: none claimed — a cohesion/size split; no call site's path changed
//! except the added `seed::` prefix inside the parent.

use std::path::Path;

use repo_graph_daemon_transport::{
    DispatchResult, ErrorCode, ErrorDetail, ProgressEmitter, Request,
};
use repo_graph_storage::StorageConnection;
use serde_json::Value;

use super::ServiceDispatcher;

/// EMBED-SEED-IMPL-1 (spec §8.4): the fallback tier's candidate cap (≤5, VISION bound).
pub(super) const SEMANTIC_FALLBACK_CAP: usize = 5;
/// EMBED-SEED-IMPL-1 (spec §8B): the affirmative `find` verb's candidate cap (≤10, HUMAN DIRECTIVE 2).
pub(super) const FIND_CANDIDATE_CAP: usize = 10;

/// EMBED-SEED-IMPL-1 (spec §8, Group A): the semantic fallback tier. Fires ONLY
/// on the deterministic-zero no-match branch (an empty `candidates` list under
/// `reason: no_match`), filling `focus.candidates` with labeled Layer-3 embedding
/// hints + a `Limit`. Every resolved/ambiguous result is left byte-unchanged
/// (this is never reached on those branches). A degraded substrate (no store /
/// model down / pins mismatch) appends one `SemanticFallbackUnavailable` `Limit`
/// and leaves the candidates empty — exactly today's no-match plus one line.
pub(super) fn apply_semantic_fallback(
    result: &mut repo_graph_agent::dto::envelope::OrientResult,
    storage: &StorageConnection,
    snapshot_uid: &str,
    repo_uid: &str,
    db_path: &Path,
    repo_root: Option<&str>,
    top_n: usize,
) {
    use repo_graph_agent::dto::envelope::{FocusCandidate, FocusFailureReason, NextCommand};
    use repo_graph_agent::dto::limit::{Limit, LimitCode};

    if result.focus.reason != Some(FocusFailureReason::NoMatch)
        || !result.focus.candidates.is_empty()
    {
        return;
    }
    let query = match result.focus.input.as_deref() {
        Some(q) if !q.is_empty() => q.to_string(),
        _ => return,
    };

    let _ = repo_uid; // no longer needed (per-snapshot vectors carry their own identity)
    match crate::seed::run_semantic_query(storage, snapshot_uid, db_path, &query, top_n) {
        crate::seed::SemanticResult::Fired {
            candidates,
            stale_count,
            ..
        } => {
            let n = candidates.len();
            for c in candidates {
                let next = NextCommand::new(
                    "explain".to_string(),
                    vec![c.stable_key.clone()],
                    repo_root.map(str::to_string),
                );
                result.focus.candidates.push(FocusCandidate::semantic(
                    c.stable_key,
                    c.path,
                    c.module,
                    c.score,
                    c.model_id,
                    next,
                ));
            }
            let _ = stale_count; // always 0 in the per-snapshot model (kept for DTO shape)
            let reasons = vec![format!(
                "{n} candidate(s) (model {}); run each candidate's `next` (explain <key>) for its \
                 imports + symbols, then explain a listed symbol for callers",
                crate::seed::MODEL_ID
            )];
            result.limits.push(Limit::from_code_with_reasons(
                LimitCode::SemanticFallback,
                reasons,
            ));
        }
        crate::seed::SemanticResult::NothingScored { .. } => {
            result.limits.push(Limit::from_code_with_reasons(
                LimitCode::SemanticFallback,
                vec!["no candidate scored above zero".to_string()],
            ));
        }
        crate::seed::SemanticResult::Unavailable(reason) => {
            result.limits.push(Limit::from_code_with_reasons(
                LimitCode::SemanticFallbackUnavailable,
                vec![reason.reason().to_string()],
            ));
        }
    }
}

/// EMBED-SEED-IMPL-1: serialize a `find` response into a success `DispatchResult`.
fn finish_find(request_id: &str, resp: &crate::seed::FindResponse) -> DispatchResult {
    match serde_json::to_value(resp) {
        Ok(v) => DispatchResult::success(request_id, v),
        Err(e) => DispatchResult::error(
            request_id,
            ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
        ),
    }
}

impl ServiceDispatcher {
    /// EMBED-SEED-IMPL-1 (spec §8, Group B): build the `symbol not found` error for a
    /// `callers`/`callees`/`path` deterministic-zero (`SymbolResolveError::NotFound`),
    /// additively attaching the semantic tier's candidates + a labeled `hint` to the
    /// error's `data`. The error CODE (`InvalidRequest`), MESSAGE (`symbol not found:
    /// <q>`), and exit are byte-unchanged — the same additive-`data` mechanism
    /// `ambiguous_symbol` already uses for its `matches`. Fires ONLY here (never on the
    /// `Ambiguous`/`Storage` arms). A degraded substrate attaches only the honest
    /// `hint` (no candidates) — never an error, never a fabricated match. `verb` names
    /// the seam so the hint tells the reader which command to re-run.
    #[allow(clippy::too_many_arguments)] // mirrors `run_semantic_query`: the seam
                                         // context (storage/snapshot/repo/db/params) is irreducible.
    pub(super) fn symbol_not_found_with_semantic(
        &self,
        storage: &StorageConnection,
        snapshot_uid: &str,
        repo_uid: &str,
        db_path: &Path,
        params: &Value,
        verb: &str,
        query: &str,
    ) -> ErrorDetail {
        let message = format!("symbol not found: {query}");
        // Canonical registry root for each candidate's `next.cwd` (review-2 #2); `None`
        // ⇒ `cwd` omitted with an honest reason (operator ruling 2), never fabricated.
        let repo_root = self.canonical_root(params);
        let _ = repo_uid; // per-snapshot vectors carry their own identity
                          // The seam's own resolution input (the symbol the agent typed) is the query
                          // (spec §8.0) — no new argument. The chunk store answers with SYMBOL chunks
                          // (path:line + qualified name), so the hint points the reader at near symbols.
        let result = crate::seed::run_semantic_query(
            storage,
            snapshot_uid,
            db_path,
            query,
            SEMANTIC_FALLBACK_CAP,
        );
        let data = crate::seed::build_group_b_data(verb, result, repo_root.as_deref());
        ErrorDetail::with_data(ErrorCode::InvalidRequest, message, data)
    }

    /// The absolute canonical repo root for a semantic candidate's `next.cwd`
    /// (spec §8.2a, review-2 #2). It is the REGISTRY entry's `canonical_path` —
    /// NEVER the raw `repo` request param, which may be an alias ("pmc") or a
    /// relative path, neither of which is a valid working directory for the
    /// follow-up `explain`/`orient` command. Returns `None` only when the `repo`
    /// param is absent or unresolvable; every seed handler resolves the repo
    /// first, so the fallback/find paths always get `Some`.
    pub(super) fn canonical_root(&self, params: &Value) -> Option<String> {
        let repo_ref = Self::get_optional_string_param(params, "repo")?;
        self.state
            .resolve_alias_or_path(repo_ref)
            .map(|e| e.canonical_path.to_string_lossy().to_string())
    }

    /// EMBED-SEED-IMPL-1 (spec §8B): the affirmative `rmap find "<concept>"` verb.
    /// Always consults the store (never a fallback tier); returns ≤10 labeled
    /// Layer-3 candidates under an always-present honesty `summary`, or the
    /// always-present `candidates: []` + a labeled summary when the substrate is
    /// unavailable (no error, no fabricated "0 results as if measured").
    pub(super) fn handle_find(
        &self,
        request: &Request,
        _emitter: &mut dyn ProgressEmitter,
    ) -> DispatchResult {
        let (repo_state, repo_uid, display_name) =
            match self.resolve_and_load_repo_with_display_name(&request.params) {
                Ok(r) => r,
                Err(e) => return DispatchResult::error(&request.id, e),
            };
        let query = match Self::get_string_param(&request.params, "query") {
            Ok(q) => q.to_string(),
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        // FIND-FACTS-1 request flags — INDEPENDENT (ratified contract, revision 1
        // operator ruling item 3). These are request inputs with a well-defined
        // absent-default (a flag not passed = false), NOT rendered facts, so the
        // `.unwrap_or(false)` here is the CLI contract, not a silent zero-collapse.
        //   `exact` = facts tier ALONE, the endpoint never touched (§2.4).
        //   `full`  = lift the DEFAULT display caps. In the facts branch it uncaps the
        //             per-class fact renders; in the FIND-GREP-1 `--text` branch it lifts
        //             the live-scan hit-line cap. review-0 finding 3: the binding is
        //             named `full` (not `full_facts`) because it now governs BOTH find
        //             modes, not just facts.
        // They do NOT imply each other: `--exact` alone applies the default per-class
        // caps; `--exact --full` is facts-only AND uncapped; `--full` alone caps
        // nothing but still consults the endpoint.
        let exact = request
            .params
            .get("exact")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let full = request
            .params
            .get("full")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        // FIND-GREP-1: `--text` selects the LIVE working-tree scan instead of the
        // fact-table + seed tiers; `-F`/`--fixed` makes the pattern a literal. Both are
        // request inputs with a well-defined absent-default (flag not passed = false),
        // NOT rendered facts — the `.unwrap_or(false)` is the CLI contract here.
        let text = request
            .params
            .get("text")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let fixed = request
            .params
            .get("fixed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        // `next.cwd` is the registry's absolute canonical root (review-2 #2), never
        // the raw `repo` param (which may be an alias / relative path). The repo
        // resolved above, so this is normally `Some`; when a registry lookup is
        // unavailable it stays `None` and `next.cwd` is OMITTED with an honest reason
        // (operator ruling 2 — never a fabricated empty cwd).
        let repo_root = self.canonical_root(&request.params);

        let _read_guard = repo_state.coordinator.acquire_read();
        // FOREGROUND-LOCK-1 (§1.2, §2.2): `find` is the exact foreground read the flake family
        // root-caused (retention pass held the DB while this open failed fast). Route it through the
        // shared bounded-patience + honest-Busy choke, not the zero-patience `storage()` +
        // `InternalError` wrap that surfaced a routine transient as an internal fault.
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // FIND-GREP-1: the `--text` LIVE scan branch. It tolerates a missing snapshot
        // (still greps the working tree; symbol context + staleness then withheld with
        // a reason), so it resolves the snapshot as an OPTION rather than sharing the
        // facts branch's hard "not built" degrade. The scan needs the working-tree ROOT
        // — the registry canonical path (`repo_root`); its absence is an honest error,
        // never a fabricated empty root.
        if text {
            let snapshot_uid = match repo_graph_agent::AgentStorageRead::get_latest_snapshot(
                &storage, &repo_uid,
            ) {
                Ok(s) => s.map(|s| s.snapshot_uid),
                Err(e) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                    )
                }
            };
            let root =
                match &repo_root {
                    Some(r) => std::path::PathBuf::from(r),
                    None => return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(
                            ErrorCode::InvalidRequest,
                            "repository working directory unavailable (registry lookup failed) — \
                             cannot run a live text scan"
                                .to_string(),
                        ),
                    ),
                };
            let resp = crate::find_text::run_text_scan(
                &storage,
                snapshot_uid.as_deref(),
                &display_name,
                &root,
                &query,
                fixed,
                full,
            );
            return match serde_json::to_value(&resp) {
                Ok(v) => DispatchResult::success(&request.id, v),
                Err(e) => DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                ),
            };
        }

        // Resolve the READY snapshot (freshness scope). No READY snapshot ⇒ the
        // store cannot exist yet ⇒ the "not built" degraded state.
        let snapshot_uid =
            match repo_graph_agent::AgentStorageRead::get_latest_snapshot(&storage, &repo_uid) {
                Ok(Some(s)) => s.snapshot_uid,
                Ok(None) => {
                    // No READY snapshot ⇒ nothing to search. Every fact class is named
                    // as unavailable-with-reason (honest searched set), and the seed
                    // tier is unavailable too (store cannot exist). `--exact` still
                    // never consults the endpoint.
                    let facts = crate::find_facts::unavailable_all(
                        "repo not indexed yet — run `rmap index .`",
                    );
                    let seed = if exact {
                        None
                    } else {
                        Some(crate::seed::SemanticResult::Unavailable(
                            crate::seed::DegradeReason::NoStore,
                        ))
                    };
                    let resp = crate::seed::build_find_response(
                        &display_name,
                        &repo_uid,
                        "",
                        &query,
                        &facts,
                        seed,
                        repo_root.as_deref(),
                    );
                    return finish_find(&request.id, &resp);
                }
                Err(e) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                    )
                }
            };

        // FACTS tier FIRST (§2.1) — always, deterministically, whether or not the
        // embedding endpoint is reachable. This is the answer that outranks guesses.
        let facts =
            crate::find_facts::gather_facts(&storage, &repo_uid, &snapshot_uid, &query, full);

        // DEMOTED semantic-seed tier (§2.3) — skipped entirely under `--exact` so the
        // endpoint is never touched (§2.4). Otherwise it degrades to
        // unavailable-with-reason when the model is down; the facts above still answer.
        let seed = if exact {
            None
        } else {
            Some(crate::seed::run_semantic_query(
                &storage,
                &snapshot_uid,
                repo_state.db_path(),
                &query,
                FIND_CANDIDATE_CAP,
            ))
        };
        let response = crate::seed::build_find_response(
            &display_name,
            &repo_uid,
            &snapshot_uid,
            &query,
            &facts,
            seed,
            repo_root.as_deref(),
        );
        finish_find(&request.id, &response)
    }
}
