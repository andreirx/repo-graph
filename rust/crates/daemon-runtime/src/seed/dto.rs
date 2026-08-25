//! The `rmap find` response DTO (spec §8B.2) — its OWN struct, NOT
//! `Focus`/`FocusCandidate`. `candidates` is a plain `Vec` → serialized
//! ALWAYS-PRESENT as `[]` when empty (no `skip_serializing_if`); `summary` is
//! always present (the Layer-3 honesty header). Degraded states (§8B.3) return
//! zero candidates + one labeled `summary` line — never an error.

use repo_graph_agent::dto::envelope::{ModuleHint, NextCommand};
use serde_json::{json, Value};

use super::query::{DegradeReason, SemanticResult};

#[derive(Debug, Clone, serde::Serialize)]
pub struct FindResponse {
    pub schema: String,
    pub command: String,
    pub repo: String,
    pub snapshot: String,
    pub query: String,
    /// ALWAYS present — the Layer-3 honesty header (I1/I2); never a completeness claim.
    pub summary: String,
    /// Plain Vec → `[]` when empty, never omitted (spec §8B.3).
    pub candidates: Vec<FindCandidate>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FindCandidate {
    pub stable_key: String,
    /// Repo-relative path — a plain String (NOT `FocusCandidate.file: Option`).
    pub path: String,
    pub score: f64,
    /// Always "embedding" (I2).
    pub source: String,
    pub model_id: String,
    /// Owning-module hint — a GENUINE module or explicit unavailable-with-reason
    /// (§8.2a; operator ruling 2026-08-25).
    pub module: ModuleHint,
    pub next: FindNext,
}

/// `find`'s follow-up command. `cwd` is the registry's absolute canonical root,
/// present iff the lookup resolved; when unavailable it is OMITTED and
/// `cwd_unavailable` carries the honest reason — never a fabricated empty cwd
/// (operator ruling 2, 2026-08-25). Mutually exclusive by construction.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FindNext {
    pub cmd: String,
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd_unavailable: Option<String>,
}

impl FindNext {
    /// Build an `explain <stable_key>` follow-up, honestly recording whether the
    /// working directory is known (`root = Some(canonical_path)`) or unavailable
    /// (`None` ⇒ `cwd` omitted + a reason, never an empty string).
    fn explain(stable_key: String, root: Option<&str>) -> Self {
        match root {
            Some(cwd) => Self {
                cmd: "explain".to_string(),
                args: vec![stable_key],
                cwd: Some(cwd.to_string()),
                cwd_unavailable: None,
            },
            None => Self {
                cmd: "explain".to_string(),
                args: vec![stable_key],
                cwd: None,
                cwd_unavailable: Some(
                    "repository working directory unavailable (registry lookup failed)".to_string(),
                ),
            },
        }
    }
}

/// `find`'s own degraded-state summary (spec §8B.3) — same causes as §8.3 but
/// `find`'s own always-present rendering (it has nothing deterministic to fall
/// back to, so it returns zero candidates + one labeled line, never an error).
fn find_degrade_summary(reason: DegradeReason) -> String {
    match reason {
        DegradeReason::NoStore => {
            "semantic index not built yet — hints will be available after indexing"
        }
        DegradeReason::StoreUnreadable => {
            "semantic index present but unreadable — it rebuilds on next index"
        }
        DegradeReason::FreshnessUnknown => {
            "cannot verify the semantic index is current — hints withheld this run"
        }
        DegradeReason::ModelUnavailable => {
            "no local embedding model reachable — semantic hints unavailable (find is optional)"
        }
        DegradeReason::PinsMismatch => {
            "semantic index was built with a different model — rebuild on next index"
        }
        DegradeReason::StoreTooLarge => {
            "vector store exceeds the seed budget — semantic hints declined"
        }
        DegradeReason::ResolveUnavailable => {
            "could not resolve semantic candidates (snapshot read failed) — hints withheld this run"
        }
        DegradeReason::InvalidConfig => {
            "seed configuration is invalid (RMAP_SEED_DIM) — set a valid positive integer"
        }
    }
    .to_string()
}

/// Build the Group-B semantic `data` payload (spec §8.2 Group B / §8.3) that rides
/// the EXISTING `symbol not found` error for `callers`/`callees`/`path`. The error
/// code (`InvalidRequest`), message (`symbol not found: <q>`), and exit are UNCHANGED;
/// this only fills the previously-`None` `data` with the same candidate FIELDS the
/// fallback tier produces (minus `kind` — FILE by construction, §8.2) plus a labeled
/// `hint`. On a degraded / nothing-scored substrate the `semantic_candidates` key is
/// OMITTED (never an empty array) and only the honest `hint` rides — mirroring §8.3's
/// omit-when-empty discipline. `verb` is the seam name, woven into the fired hint so
/// the reader knows which command to re-run on a file's symbol.
pub fn build_group_b_data(verb: &str, result: SemanticResult, repo_root: Option<&str>) -> Value {
    match result {
        SemanticResult::Fired { candidates, .. } => {
            let cands: Vec<Value> = candidates
                .into_iter()
                .map(|c| {
                    let next = NextCommand::new(
                        "explain".to_string(),
                        vec![c.stable_key.clone()],
                        repo_root.map(str::to_string),
                    );
                    json!({
                        "stable_key": c.stable_key,
                        // §8.2 Group B uses `file` (the store is file-level, D-ES-5),
                        // NOT the `find` DTO's `path`; `kind` is omitted (FILE by
                        // construction).
                        "file": c.path,
                        "score": c.score,
                        "source": "embedding",
                        "model_id": c.model_id,
                        "module": c.module,
                        "next": next,
                    })
                })
                .collect();
            json!({
                "semantic_candidates": cands,
                "hint": format!(
                    "no such symbol; these files are semantically near your query — \
                     open one, then re-run {verb} on a symbol inside it"
                ),
            })
        }
        // Genuine known-zero: no candidates, only the honest hint (§8.3).
        SemanticResult::NothingScored => json!({
            "hint": format!(
                "no such symbol; no files scored above zero for a semantic hint (re-run {verb} on a symbol name)"
            ),
        }),
        // Degraded substrate: the SAME causes/taxonomy as the fallback tier (§8.3) —
        // the reason string rides `data.hint`; no candidates.
        SemanticResult::Unavailable(reason) => json!({
            "hint": reason.reason(),
        }),
    }
}

/// Build the `find` response DTO from a semantic query outcome (spec §8B.2/§8B.3).
pub fn build_find_response(
    repo: &str,
    snapshot: &str,
    query: &str,
    result: SemanticResult,
    repo_root: Option<&str>,
) -> FindResponse {
    let (summary, candidates) = match result {
        SemanticResult::Fired { candidates, .. } => {
            let cands = candidates
                .into_iter()
                .map(|c| {
                    let stable_key = c.stable_key;
                    FindCandidate {
                        next: FindNext::explain(stable_key.clone(), repo_root),
                        stable_key,
                        path: c.path,
                        score: c.score,
                        source: "embedding".to_string(),
                        model_id: c.model_id,
                        module: c.module,
                    }
                })
                .collect();
            (
                format!("likely areas for \"{query}\" (semantic hints — open the files)"),
                cands,
            )
        }
        SemanticResult::NothingScored => (
            format!("no area scored above zero for \"{query}\""),
            Vec::new(),
        ),
        SemanticResult::Unavailable(reason) => (find_degrade_summary(reason), Vec::new()),
    };

    FindResponse {
        schema: "rgr.agent.v1".to_string(),
        command: "find".to_string(),
        repo: repo.to_string(),
        snapshot: snapshot.to_string(),
        query: query.to_string(),
        summary,
        candidates,
    }
}
