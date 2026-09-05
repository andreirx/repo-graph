//! The `rmap find` response DTO (spec §8B.2) — its OWN struct, NOT
//! `Focus`/`FocusCandidate`. `candidates` is a plain `Vec` → serialized
//! ALWAYS-PRESENT as `[]` when empty (no `skip_serializing_if`); `summary` is
//! always present (the Layer-3 honesty header). Degraded states (§8B.3) return
//! zero candidates + one labeled `summary` line — never an error.

use repo_graph_agent::dto::envelope::{ModuleHint, NextCommand};
use serde_json::{json, Value};

use super::query::{DegradeReason, SemanticResult};
use crate::find_facts::ClassOutcome;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, serde::Serialize)]
pub struct FindResponse {
    pub schema: String,
    pub command: String,
    pub repo: String,
    /// FIND-EVIDENCE-1 (§2.3) additive: the repo's stable-key UID prefix, printed ONCE
    /// in the human header so per-row symbol cursors can drop it (the cursor diet). The
    /// JSON `next`/`key` stay FULL (unchanged), so machine consumers are byte-compatible;
    /// the header uid is what lets the human renderer show relative `explain <suffix>`
    /// cursors (which the daemon's additive alias resolves). Empty string only when the
    /// repo uid is genuinely unknown (never fabricated).
    pub repo_uid: String,
    pub snapshot: String,
    pub query: String,
    /// The DEMOTED semantic-seed tier's honesty header (FIND-FACTS-1 §2.3 renames it
    /// to ranked-guesses wording). ALWAYS present; never a completeness claim.
    pub summary: String,
    /// Semantic-seed candidates. Plain Vec → `[]` when empty, never omitted.
    pub candidates: Vec<FindCandidate>,
    /// FIND-FACTS-1 (§2.1) additive: the FACTS tier — one group PER fact class, in a
    /// fixed order, ALWAYS present (an empty/errored class still appears, so the
    /// searched set is honest). JSON-additive: existing seed consumers ignore it.
    pub facts: Vec<FindFactGroup>,
    /// FIND-FACTS-1 (§2.3): whether the semantic-seed tier was consulted AND
    /// reachable this run. `false` under `--exact` (never consulted) or a degraded
    /// substrate — the facts tier answers regardless.
    pub seeds_available: bool,
    /// The reader-facing reason the seed tier is unavailable (§2.3), when it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seeds_unavailable_reason: Option<String>,
}

/// One fact class's group in the FACTS tier (§2.2). Always present per class so the
/// searched set is honest even when a class is empty or its read failed.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FindFactGroup {
    /// The fact-class tag (`symbol`, `http-surface`, …).
    pub fact_class: String,
    /// The single command that renders this class — the reader's next move (verb only).
    /// ABSENT for a class whose renderer varies per hit (the `boundary` governance-
    /// declaration class: `violations` for a boundary-kind row, `gate` for a
    /// requirement/quality-policy row — review-6 re-home). When absent, each hit's own
    /// `next` carries the move and the group header omits the `→ rmap <cmd>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub render_command: Option<String>,
    /// The certainty LAYER of this class's source (`extracted` | `inferred` | `hint` |
    /// `governance`) — rendered in the label so a Layer 2–3 hit is never presented as an
    /// extracted fact (review-1 honesty defect; VISION § Fact Certainty Model).
    /// `governance` is the Layer-4 label for the boundary declaration class: authored
    /// policy statements, not extracted facts (review-8 doc reconciliation). Always present.
    pub certainty: String,
    /// The capped hits (empty when nothing matched OR the class read failed).
    pub hits: Vec<FindFactHit>,
    /// Total matched BEFORE the display cap. `remainder = matched - hits.len()`.
    pub matched: usize,
    /// `true` when `matched` is a FLOOR (fetch window saturated), not an exact count.
    pub matched_is_floor: bool,
    /// Set when THIS class's read failed — rendered as `unavailable (<reason>)`,
    /// never a silent empty (STANDING HONESTY RULE).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// One fact hit (§2.2).
#[derive(Debug, Clone, serde::Serialize)]
pub struct FindFactHit {
    pub display: String,
    /// The owning path when KNOWN. Mutually exclusive with `path_unknown_reason`;
    /// both absent = the class has no path dimension (dependency, framework).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// FIND-FACTS-1 additive (review-4 item 2): set when the class HAS a path
    /// dimension but THIS hit's path is unknown — the reason, rendered as
    /// `path unknown (<reason>)`, never a silent omission (STANDING HONESTY RULE).
    /// Optional + skip-serialized → byte-compatible for existing consumers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_unknown_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// FIND-EVIDENCE-1 (§2.1) additive: the stored start line for the `path:line` anchor
    /// (SYMBOL hits). ABSENT (skip-serialized) when the class carries no per-symbol span
    /// OR the stored span was NULL — the row then renders WITHOUT a line (visibly absent),
    /// never a fabricated 0/guess. Optional + skip → byte-compatible for existing consumers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<i64>,
    /// FIND-EVIDENCE-1 (§2.2) additive: the ONE evidence line (doc-comment first line,
    /// else signature) derived from STORED facts only — never file I/O, never an invented
    /// preview. ABSENT when neither is stored. Optional + skip → byte-compatible.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    /// The runnable `rmap` invocation (WITHOUT the `rmap` prefix) that takes the
    /// reader from THIS hit to its rendering — `explain <key>` / `map <path>` for the
    /// argument-taking classes, the whole-listing command for the `… list` classes.
    /// Always present and executable (review-1 item 1): the renderer prints it and
    /// the e2e proof runs it verbatim, exit 0. FULL form (the JSON stays byte-stable);
    /// the human renderer derives the relative short cursor from `repo_uid` + `key`.
    /// SHELL-QUOTED when the key carries spaces/metacharacters (copy-paste safe for a human).
    pub next: String,
    /// CURSOR-ROUNDTRIP-1 (§2.3) additive: the RAW cursor ARGUMENT — the verb-less,
    /// UNQUOTED token an agent passes straight to any cursor-taking command
    /// (`explain`/`callers`/`callees`/`path`), so it never strips the shell quotes `next`
    /// may carry nor the verb prefix (the self-model experiment's finding; revision 1
    /// review). NOT a runnable command — the cursor alone. uid-STRIPPED for the
    /// explain-folding symbol/file classes whose key carries this repo's `<uid>:` prefix
    /// (byte-for-byte the short cursor the human render prints), else the full key.
    /// ABSENT (skip-serialized) for the whole-listing classes and boundary, whose
    /// renderers take no cursor argument — a cursor field only where a cursor exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_raw: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FindCandidate {
    pub stable_key: String,
    /// Repo-relative path — a plain String (NOT `FocusCandidate.file: Option`).
    pub path: String,
    /// SEED-CHUNK-1 (FIND-EVIDENCE-1 anchor): the SYMBOL chunk's start line for the
    /// `path:line` anchor. ABSENT (skip-serialized) when the node had no stored span
    /// — the row then renders WITHOUT a line, never a fabricated 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<i64>,
    /// The symbol's qualified name (the anchor's human label). ABSENT when unstored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qualified_name: Option<String>,
    /// SEED-CHUNK-1 (spec §5, the moat): `true` ⇒ a DEMOTED test-classified chunk,
    /// ranked below production and labeled so the reader is never misled. Always
    /// present so the partition is honest in the JSON, not just the human render.
    pub is_test: bool,
    /// SEED-CHUNK-2 (spec §2.2): `true` ⇒ a declaration without a body, ranked below
    /// its own implementation and labeled `(decl)`. Always present so the decl/impl
    /// kind is honest in the JSON, not just the human render.
    pub is_decl: bool,
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
fn find_degrade_summary(reason: &DegradeReason) -> String {
    match reason {
        DegradeReason::NoStore => {
            "semantic seeds not built yet — hints will be available after indexing".to_string()
        }
        DegradeReason::StoreUnreadable => {
            "semantic seeds present but unreadable — they rebuild on next index".to_string()
        }
        DegradeReason::ModelUnavailable => {
            "embedding model not cached and not fetchable — semantic hints unavailable (find is optional)".to_string()
        }
        DegradeReason::ModelUnreadable { detail } => {
            format!("cached embedding model unreadable ({detail}) — it rebuilds on next index")
        }
        DegradeReason::PinsMismatch => {
            "semantic seeds were built with a different model — rebuild on next index".to_string()
        }
        DegradeReason::SeedsReembedding => {
            "semantic seeds re-embedding for per-chunk facts (pending) — hints return once it completes".to_string()
        }
        DegradeReason::SeedsStaleSeedingDisabled => {
            "semantic seeds predate per-chunk facts and seeding is disabled — enable seeding (unset RMAP_SEED_VECTORS) to rebuild them".to_string()
        }
    }
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
                        // SEED-CHUNK-1: the store is now SYMBOL-chunk-level, so the
                        // hint carries the `path:line` anchor + qualified name, not a
                        // bare file. `line`/`qualified_name` skip when unstored (never
                        // a fabricated 0); `is_test` labels the demoted test block.
                        "path": c.path,
                        "line": c.line,
                        "qualified_name": c.qualified_name,
                        "is_test": c.is_test,
                        "is_decl": c.is_decl,
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
                    "no such symbol; these symbols are semantically near your query — \
                     explain one (test-classified hits are labeled and ranked below production)"
                ),
            })
        }
        // Genuine known-zero: no candidates, only the honest hint (§8.3).
        SemanticResult::NothingScored { best } => json!({
            "hint": match best {
                Some(b) => format!(
                    "no such symbol; nothing scored above the floor (best: {b:.3}) — re-run {verb} on a symbol name"
                ),
                None => format!(
                    "no such symbol; no seed vectors to score — re-run {verb} on a symbol name"
                ),
            },
        }),
        // Degraded substrate: the SAME causes/taxonomy as the fallback tier (§8.3) —
        // the reason string rides `data.hint`; no candidates.
        SemanticResult::Unavailable(reason) => json!({
            "hint": reason.reason(),
        }),
    }
}

/// Project the FACTS-tier outcomes (§2.2) into the additive JSON groups — one per
/// fact class, in the fixed [`crate::find_facts::FactClass::ALL`] order. A class
/// whose read FAILED carries its `error` (rendered `unavailable (<reason>)`),
/// never a silent empty (STANDING HONESTY RULE).
fn fact_groups(facts: &[ClassOutcome], repo_uid: &str) -> Vec<FindFactGroup> {
    facts
        .iter()
        .map(|o| match &o.result {
            Ok(hits) => FindFactGroup {
                fact_class: o.class.label().to_string(),
                render_command: o.class.render_command().map(str::to_string),
                certainty: o.class.certainty_tag().to_string(),
                hits: hits
                    .hits
                    .iter()
                    .map(|h| {
                        // Project the HitPath sum type into the two mutually-exclusive
                        // wire fields: a KNOWN path, an UNKNOWN-with-reason, or neither
                        // (no path dimension) — never a silent omission standing in for
                        // an unknown (review-4 item 2).
                        let (path, path_unknown_reason) = match &h.path {
                            crate::find_facts::HitPath::Known(p) => (Some(p.clone()), None),
                            crate::find_facts::HitPath::Unknown(reason) => {
                                (None, Some(reason.clone()))
                            }
                            crate::find_facts::HitPath::None => (None, None),
                        };
                        // The runnable next command: a boundary hit carries its OWN
                        // per-declaration-kind renderer (`violations`/`gate`); every other
                        // class derives it from (class, key) here — the single site that
                        // owns both, never re-derived in the CLI. Matched over BOTH sources
                        // as a tuple (not an `unwrap_or_default`, which the STANDING HONESTY
                        // RULE forbids on a rendered field): a boundary hit takes its
                        // `next_command`; a single-renderer class takes its `hit_command`
                        // (always `Some` for those six by construction). The final arm is
                        // unreachable by construction — only boundary yields a `None`
                        // `hit_command`, and every boundary hit sets `next_command`; if a
                        // future change ever produced neither, it emits the empty string the
                        // CLI surfaces as a malformed hit (loud fail-safe, never a fabricated
                        // command).
                        let next = match (h.next_command, o.class.hit_command(h.key.as_deref())) {
                            (Some(cmd), _) => cmd.to_string(),
                            (None, Some(cmd)) => cmd,
                            (None, None) => String::new(),
                        };
                        // CURSOR-ROUNDTRIP-1 (§2.3): the RAW cursor ARGUMENT (verb-less,
                        // unquoted) an agent passes to any cursor-taking command. Derived
                        // from (class, key, repo_uid) — uid-stripped to the short cursor for
                        // the explain-folding symbol/file classes, else the full key; `None`
                        // for the whole-listing/boundary classes whose renderer takes no
                        // cursor argument (a boundary hit's per-kind renderer, like the
                        // `… list` classes, has no cursor to carry).
                        let cursor_raw = o.class.cursor_arg(h.key.as_deref(), repo_uid);
                        FindFactHit {
                            next,
                            cursor_raw,
                            display: h.display.clone(),
                            path,
                            path_unknown_reason,
                            key: h.key.clone(),
                            // FIND-EVIDENCE-1: the stored anchor line + evidence line,
                            // carried straight through from the fact hit (symbol class
                            // only today; `None` elsewhere → skip-serialized).
                            line: h.line,
                            evidence: h.evidence.clone(),
                        }
                    })
                    .collect(),
                matched: hits.matched,
                matched_is_floor: hits.matched_is_floor,
                error: None,
            },
            Err(reason) => FindFactGroup {
                fact_class: o.class.label().to_string(),
                render_command: o.class.render_command().map(str::to_string),
                certainty: o.class.certainty_tag().to_string(),
                hits: Vec::new(),
                matched: 0,
                matched_is_floor: false,
                error: Some(reason.clone()),
            },
        })
        .collect()
}

/// Build the `find` response DTO (FIND-FACTS-1 §2): the FACTS tier ALWAYS present,
/// the DEMOTED semantic-seed tier below it (renamed to ranked-guesses wording, or
/// unavailable-with-reason). `seed` is `None` under `--exact` — the endpoint is
/// never touched and the seed tier renders as not-consulted.
pub(crate) fn build_find_response(
    repo: &str,
    repo_uid: &str,
    snapshot: &str,
    query: &str,
    facts: &[ClassOutcome],
    seed: Option<SemanticResult>,
    repo_root: Option<&str>,
) -> FindResponse {
    let (summary, candidates, seeds_available, seeds_unavailable_reason) = match seed {
        // `--exact`: the endpoint is never consulted (§2.4).
        None => (
            format!("semantic seeds not consulted for \"{query}\" (--exact — facts only)"),
            Vec::new(),
            false,
            Some("not consulted (--exact — facts only)".to_string()),
        ),
        Some(SemanticResult::Fired { candidates, .. }) => {
            let cands: Vec<FindCandidate> = candidates
                .into_iter()
                .map(|c| {
                    let stable_key = c.stable_key;
                    FindCandidate {
                        next: FindNext::explain(stable_key.clone(), repo_root),
                        stable_key,
                        path: c.path,
                        line: c.line,
                        qualified_name: c.qualified_name,
                        is_test: c.is_test,
                        is_decl: c.is_decl,
                        score: c.score,
                        source: "embedding".to_string(),
                        model_id: c.model_id,
                        module: c.module,
                    }
                })
                .collect();
            // Honesty header: name the partition when any test-classified chunk is in
            // the list, so the reader knows the demotion is in effect (spec §5).
            let has_test = cands.iter().any(|c| c.is_test);
            let summary = if has_test {
                format!(
                    "ranked guesses for \"{query}\" (embedding similarity — not facts; \
                     production ranked above test, test labeled)"
                )
            } else {
                format!("ranked guesses for \"{query}\" (embedding similarity — not facts)")
            };
            (summary, cands, true, None)
        }
        Some(SemanticResult::NothingScored { best }) => {
            let summary = match best {
                Some(b) => format!(
                    "no seeds above the floor for \"{query}\" (best: {b:.3}, embedding similarity)"
                ),
                None => format!("no seed vectors to score for \"{query}\" (embedding similarity)"),
            };
            (summary, Vec::new(), true, None)
        }
        // Degraded substrate: the facts tier still answered above; the seed tier
        // says unavailable-with-reason — the verb no longer dies with the endpoint.
        Some(SemanticResult::Unavailable(reason)) => (
            find_degrade_summary(&reason),
            Vec::new(),
            false,
            Some(reason.reason()),
        ),
    };

    FindResponse {
        schema: "rgr.agent.v1".to_string(),
        command: "find".to_string(),
        repo: repo.to_string(),
        repo_uid: repo_uid.to_string(),
        snapshot: snapshot.to_string(),
        query: query.to_string(),
        summary,
        candidates,
        facts: fact_groups(facts, repo_uid),
        seeds_available,
        seeds_unavailable_reason,
    }
}
