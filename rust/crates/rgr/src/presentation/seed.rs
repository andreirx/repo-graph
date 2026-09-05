//! Human rendering of the semantic-seed candidate surfaces. Two families live here
//! because they render two DIFFERENT wire shapes:
//!
//! - **Group A** (`orient`/`explain` focus no-match): a `FocusCandidate`, which
//!   serializes the path under `file` and carries no per-symbol span/qualified-name —
//!   rendered by [`render_candidate_body`] (the `file` field). Unchanged by
//!   CURSOR-ROUNDTRIP-1.
//! - **Group B** (`callers`/`callees`/`path` `symbol not found` fallback): the CURRENT
//!   SEED-CHUNK-1 candidate shape (`path`/`line`/`qualified_name`/`is_test`), the SAME
//!   shape `find` emits. CURSOR-ROUNDTRIP-1 (§2.2) routes it through the ONE current-DTO
//!   renderer [`render_seed_chunk_candidate`] (shared with `find`'s seed tier — not a
//!   second copy; STANDING HONESTY RULE 2). Before this slice Group B rendered through the
//!   pre-SEED-CHUNK-1 `file`-shaped [`render_candidate_body`], so every current-DTO
//!   candidate printed `(malformed candidate: missing file/…)` ×N.
//!
//! **Abstraction one-liner:** `render_seed_chunk_candidate` — a crate-private current-DTO
//! seed-candidate renderer; concrete current users: `find`'s `render_seed_tier`
//! (`commands::find::seed_render`) and Group B's `render_symbol_not_found_semantic`; axis:
//! none claimed — one renderer so the two surfaces cannot diverge (STANDING HONESTY RULE
//! 2). Rejected simpler: leave Group B on `render_candidate_body` (renders the current DTO
//! as malformed) or fork a Group-B copy (two renderers, drift).
//!
//! STANDING HONESTY RULE 1: a candidate that fails validation is COUNTED and STATED — the
//! renderer returns `Err(reason)` and the tier appends ONE honest
//! `N candidate(s) unreadable: <reason>` line ([`render_unreadable_summary`]); NEVER a
//! per-row `(malformed candidate: …)` placeholder, NEVER a fabricated score/model/path.

use serde_json::Value;

/// ECONOMY-2 (§2.1, ruling economy_2_cursor_metric): a row is CURSOR-COMPOSABLE when the
/// reader can reassemble the runnable short cursor `<path>#<qualified_name>:SYMBOL:<KIND>`
/// from the identity fields the row ALREADY prints. When it is, the per-row `→ rmap explain
/// …` cursor line is redundant with the ONE pattern header `find` prints, so it is DROPPED
/// and the row shows `[KIND]` instead (a fraction of a cursor line's bytes). This is how the
/// LITERAL ≤15%-of-bytes-on-cursor-lines target is met BY DESIGN, not by redefinition.
///
/// Returns `Some(kind)` iff `key` is this repo's IN-ROOT symbol stable_key AND its
/// uid-stripped suffix is EXACTLY `<path>#<qualified_name>:SYMBOL:<kind>` for the row's OWN
/// `path` + `qualified_name`, with `kind` a single non-empty token (no `:`/`#`). `None`
/// otherwise (no header uid, out-of-root key, non-symbol key, a missing/mismatched
/// path/name, or an absent kind) — the caller then keeps an explicit per-row cursor, so a
/// non-composable row is NEVER silently stripped of its runnable cursor.
///
/// Deriving `kind` from the SAME suffix that forms `cursor_raw` is what makes the header's
/// promise TRUE: `format!("{path}#{qualified_name}:SYMBOL:{kind}")` reproduces the suffix
/// byte-for-byte, and the daemon's syntax-gated `explain` reattach alias (keyed on the
/// `:SYMBOL` marker) resolves that suffix to the same node the full key does. So "copy a
/// row into the pattern → `explain` resolves" holds by construction; a row whose fields do
/// NOT reassemble the suffix fails the `strip_prefix` and keeps its cursor (fail-safe —
/// never a false runnable-pattern claim; STANDING HONESTY RULE).
///
/// Shared by `find`'s two render tiers (`fact_hit::render_fact_hit`,
/// `render_seed_chunk_candidate`) AND `find`'s pre-count (`diet_eligibility`), so the
/// header-on/off count and the per-row drop decision are computed by the ONE function and
/// cannot drift (a drift would print the header while a row kept its cursor, or vice versa).
pub(crate) fn composable_cursor_kind(
    repo_uid: Option<&str>,
    key: &str,
    path: &str,
    qualified_name: &str,
) -> Option<String> {
    let uid = repo_uid?;
    let suffix = key.strip_prefix(&format!("{uid}:"))?;
    let kind = suffix.strip_prefix(&format!("{path}#{qualified_name}:SYMBOL:"))?;
    if kind.is_empty() || kind.contains(':') || kind.contains('#') {
        return None;
    }
    Some(kind.to_string())
}

/// Render the semantic `hint` + `semantic_candidates` (if any) carried on a
/// `symbol not found` error's `data`. Returns `None` when `data` carries no seed
/// semantic keys at all (a plain not-found from a daemon without the tier) so the
/// caller renders exactly today's error. The `hint` is always printed when present;
/// the candidate block only when `semantic_candidates` is a non-empty array (§8.3
/// omit-when-empty).
pub fn render_symbol_not_found_semantic(data: Option<&Value>) -> Option<String> {
    let data = data?.as_object()?;
    let hint = data.get("hint").and_then(|v| v.as_str());
    let candidates = data
        .get("semantic_candidates")
        .and_then(|v| v.as_array())
        .filter(|a| !a.is_empty());
    // Nothing seed-related on this error ⇒ leave today's rendering untouched.
    if hint.is_none() && candidates.is_none() {
        return None;
    }

    let mut out = String::new();
    if let Some(hint) = hint {
        out.push_str(&format!("hint: {hint}\n"));
    }
    if let Some(cands) = candidates {
        out.push_str("Semantic candidates (Layer-3 embedding hints, not resolved facts):\n");
        // CURSOR-ROUNDTRIP-1 (§2.2): render through the CURRENT SEED-CHUNK-1 renderer
        // (`path`/`line`/`qualified_name`/`is_test`) — the SAME one `find` uses. A
        // candidate that fails validation is COUNTED (not rendered as a placeholder) and
        // stated ONCE below (STANDING HONESTY RULE 1).
        let mut unreadable: Vec<String> = Vec::new();
        for c in cands {
            // Group B has NO once-per-output repo-uid header (it renders inside a
            // `symbol not found` error, not `find`'s framed output), so it passes
            // `None`: every seed cursor stays in its full, self-contained `cd … &&`
            // form here — byte-identical to before ECONOMY-2. Only `find`'s seed tier,
            // which prints the uid header once, passes `Some(uid)` to shorten rows.
            match render_seed_chunk_candidate(c, None) {
                Ok(row) => out.push_str(&row),
                Err(reason) => unreadable.push(reason),
            }
        }
        if !unreadable.is_empty() {
            out.push_str(&render_unreadable_summary(&unreadable));
        }
    }
    Some(out)
}

/// Render the semantic-hint header line + the daemon's per-cause `reasons`
/// (EMBED-SEED-IMPL-1 review-9 #2). The fixed `summary` is the Layer-3 honesty
/// contract line; each `reason` is the SPECIFIC cause (a dead endpoint, a pins
/// mismatch, the stale-subset count) surfaced verbatim underneath — never collapsed
/// into the generic summary. Shared by the `orient` and `explain` no-match renderers
/// (two concrete current callers, real duplication); axis: none claimed — a shared
/// honesty-preserving formatter so both surfaces render the cause identically.
pub(crate) fn render_semantic_header(summary: &str, reasons: &[String]) -> String {
    let mut out = format!("Semantic hints: {summary}\n");
    for reason in reasons {
        out.push_str(&format!("  ({reason})\n"));
    }
    out
}

/// CURSOR-ROUNDTRIP-1 (§2.2): render ONE current-DTO SEED-CHUNK-1 seed candidate — the
/// SAME shape `find` emits (`path`/`line`/`qualified_name`/`is_test`) and the SAME renderer
/// `find`'s seed tier uses. Every identity field is validated; a genuinely-absent required
/// field returns `Err(reason)` so the caller can COUNT it and state ONE honest line
/// ([`render_unreadable_summary`]) — never a per-row placeholder, never a fabricated value
/// (STANDING HONESTY RULE 1). The `Ok` row is byte-identical to `find`'s prior
/// `render_seed_candidate` output (2-space anchor indent, `path:line` + qualified name,
/// validated `source == "embedding"` label, `[test]`/`[is_test unknown]` partition,
/// 4-space `next` line).
///
/// ECONOMY-2 (§1) seed-row cursor diet: `repo_uid` is the uid printed ONCE in `find`'s
/// seeds header (`Some`), or `None` when there is no header to anchor to (Group B, or the
/// no-snapshot degraded state). When present AND this row's `stable_key` carries the
/// `<uid>:` prefix (an IN-ROOT row — the common case, since seeds are drawn from the
/// indexed repo), the follow-up renders the SHORT runnable cursor `→ rmap explain <suffix>`
/// (uid dropped, `cd … &&` dropped — you are already in the repo, matching the fact tier's
/// diet). An OUT-OF-ROOT key (no `<uid>:` prefix — old/foreign daemon) or `None` keeps the
/// full self-contained `(cd <cwd> && rmap explain <key>)` form, so a non-current-repo cursor
/// still runs verbatim.
pub(crate) fn render_seed_chunk_candidate(
    c: &Value,
    repo_uid: Option<&str>,
) -> Result<String, String> {
    let path = c.get("path").and_then(|v| v.as_str());
    let key = c.get("stable_key").and_then(|v| v.as_str());
    let score = c.get("score").and_then(|v| v.as_f64());
    let model = c.get("model_id").and_then(|v| v.as_str());
    let source = c.get("source").and_then(|v| v.as_str());
    let (Some(path), Some(key), Some(score), Some(model), Some(source)) =
        (path, key, score, model, source)
    else {
        return Err("missing required field — path/stable_key/score/model_id/source".to_string());
    };
    // STANDING HONESTY RULE: the provenance label is the daemon's own VALIDATED `source`,
    // never a hardcoded "embedding" pasted over a foreign/old-daemon payload.
    if source != "embedding" {
        return Err(format!("source {source:?} is not a Layer-3 embedding hint"));
    }
    let module = render_seed_chunk_module_hint(c.get("module")).ok_or_else(|| {
        "missing or invalid module hint — expected owning/unavailable".to_string()
    })?;
    // SEED-CHUNK-1 anchor: `path:line` when a span is stored, else bare path (never a
    // fabricated 0). `qualified_name` is the human label when stored.
    let anchor = match c.get("line").and_then(|v| v.as_i64()) {
        Some(line) => format!("{path}:{line}"),
        None => path.to_string(),
    };
    let qualified_name = c.get("qualified_name").and_then(|v| v.as_str());
    let symbol = qualified_name.map(|q| format!("  {q}")).unwrap_or_default();
    // ECONOMY-2 (§2.1): a row whose OWN path + qualified_name reassemble the runnable short
    // cursor (`<path>#<qualified_name>:SYMBOL:<KIND>`) is CURSOR-COMPOSABLE — the ONE pattern
    // header `find` prints covers it, so its per-row `→ rmap explain …` line is DROPPED and
    // the row shows `[KIND]` instead. A row that does NOT compose (out-of-root key, no header
    // uid, absent qualified_name, or a mismatched suffix) keeps its explicit runnable cursor
    // — never silently stripped (STANDING HONESTY RULE). `cursor_raw` in the JSON is unchanged.
    let composable_kind =
        qualified_name.and_then(|q| composable_cursor_kind(repo_uid, key, path, q));
    let kind_label = match &composable_kind {
        Some(k) => format!("  [{k}]"),
        None => String::new(),
    };
    // `is_test` is always serialized (SEED-CHUNK-1 §5 moat): production is the unlabeled
    // default; a test chunk is `[test]`; a MISSING/non-bool value is UNKNOWN classification,
    // rendered explicitly — never left blank to masquerade as production.
    let test_label = match c.get("is_test") {
        Some(Value::Bool(true)) => "  [test]",
        Some(Value::Bool(false)) => "",
        _ => "  [is_test unknown]",
    };
    // SEED-CHUNK-2 (spec §2.2): a declaration-without-a-body chunk is labeled `(decl)`
    // so the agent knows it is not the implementation; a body-bearing chunk is the
    // unlabeled default (implementation). `is_decl` is always serialized (bool) by the
    // current DTO, so a MISSING/non-bool value means an UNKNOWN classification (old /
    // foreign daemon) — rendered EXPLICITLY as `[is_decl unknown]`, symmetric with
    // `[is_test unknown]` (review-3 item 3: unknown must never masquerade as the
    // unlabeled implementation default; VISION unknown≠zero, STANDING HONESTY RULE 1).
    let decl_label = match c.get("is_decl") {
        Some(Value::Bool(true)) => "  (decl)",
        Some(Value::Bool(false)) => "",
        _ => "  [is_decl unknown]",
    };
    let mut row = format!(
        "  {anchor}{symbol}  (score {score:.2}, {source}, model {model}{module}){decl_label}{test_label}{kind_label}\n"
    );
    // Composable ⇒ the pattern header covers this row; emit NO per-row cursor line. Otherwise
    // render the explicit follow-up (the short in-root cursor, or the full self-contained
    // out-of-root/`None` form) — validating `next` as before so a malformed payload surfaces.
    if composable_kind.is_none() {
        row.push_str(&render_seed_chunk_next(c.get("next"), key, repo_uid)?);
    }
    Ok(row)
}

/// Render a current-DTO candidate's `next` follow-up (4-space indent — `find`'s form).
/// `cwd` present ⇒ the `cd <cwd> && …` hint; absent-with-reason ⇒ the honest reason;
/// a `next` carrying neither, or no `next` object at all, is `Err` (malformed).
///
/// ECONOMY-2 (§1): when `repo_uid` is `Some(uid)` and `key` carries this repo's `<uid>:`
/// prefix (an IN-ROOT row), the short cursor `→ rmap explain <suffix>` replaces the full
/// `(cd <cwd> && rmap explain <key>)` form — the uid rides the once-per-output header and
/// `cd` is redundant when the target is in the current repo (the same discipline the fact
/// tier's `relative_cursor` applies). The `explain` reattach alias
/// (`daemon-runtime::dispatch::explain_alias::reattach_repo_uid_prefix`) reattaches the uid
/// to a prefix-less `:SYMBOL`-bearing suffix, so the short cursor runs verbatim. An
/// out-of-root key (no `<uid>:` prefix), a NON-symbol in-root key (a suffix WITHOUT the
/// `:SYMBOL` marker the alias gates on — it would not round-trip), or `None` keeps the full
/// self-contained form (review-1 finding 2d).
fn render_seed_chunk_next(
    next: Option<&Value>,
    key: &str,
    repo_uid: Option<&str>,
) -> Result<String, String> {
    let Some(n) = next.and_then(|v| v.as_object()) else {
        return Err("missing next follow-up".to_string());
    };
    // IN-ROOT short cursor: only when a header uid exists AND the key carries its prefix
    // (leaving a non-empty suffix) AND that suffix is a form the `explain` reattach alias
    // ACCEPTS. The alias (`dispatch::explain_alias::reattach_repo_uid_prefix`) reattaches the
    // uid ONLY to a prefix-less suffix carrying the `:SYMBOL` fact-class marker; a non-symbol
    // in-root key (no `:SYMBOL` — e.g. a file/path-level or malformed seed key) shortened
    // here would be read by `explain` as a PATH, miss the reattach, and NOT round-trip. So we
    // shorten only `:SYMBOL`-bearing suffixes and keep the known-good full self-contained
    // follow-up for anything else (review-1 finding 2d; STANDING HONESTY RULE: never print a
    // cursor that does not run). The `:SYMBOL` literal is the SAME printed-cursor SYNTAX
    // `composable_cursor_kind` derives from above and the marker the alias gates on — the
    // gate here mirrors the alias's acceptance contract exactly. (This branch is reached only
    // for NON-composable rows; a composable row already emitted no cursor at all.)
    if let Some(uid) = repo_uid {
        if let Some(suffix) = key.strip_prefix(&format!("{uid}:")) {
            if !suffix.is_empty() && suffix.contains(":SYMBOL") {
                // review-2 finding 2: shell-quote the suffix with the SAME render-side POSIX
                // encoder the fact tier uses (`presentation::shell_quote_arg`). A `:SYMBOL`
                // suffix ALWAYS carries a `#` (`<path>#<qualified_name>:SYMBOL:<KIND>`) — a
                // shell COMMENT char — so an unquoted `rmap explain <suffix>` would be
                // truncated at the `#` when copy-pasted and NOT round-trip. The shell strips
                // the single quotes before `rmap` sees the arg, so the reattach alias still
                // resolves the same node (STANDING HONESTY RULE: never print a cursor that
                // does not run).
                return Ok(format!(
                    "    → rmap explain {}\n",
                    crate::presentation::shell_quote_arg(suffix)
                ));
            }
        }
    }
    let cwd = n.get("cwd").and_then(|v| v.as_str());
    let unavailable = n.get("cwd_unavailable").and_then(|v| v.as_str());
    // review-3: the full-form fallback is a copy-paste command too, so BOTH the `cwd`
    // (a path may carry a space) and the `key` (a symbol stable_key ALWAYS carries `#`, a
    // shell COMMENT char — `uid:path#name:SYMBOL:KIND`) go through the SAME POSIX encoder
    // the short cursor uses. A safe token (no `#`/space) is left bare, so an in-root
    // non-symbol key or a plain `cwd` renders byte-identical to before; the shell strips the
    // quotes before `rmap` sees the arg, so the reattach/explain resolves the same node
    // (STANDING HONESTY RULE 2: never print a cursor that does not run).
    let key = crate::presentation::shell_quote_arg(key);
    match (cwd, unavailable) {
        (Some(cwd), _) => Ok(format!(
            "    → (cd {} && rmap explain {key})\n",
            crate::presentation::shell_quote_arg(cwd)
        )),
        (None, Some(reason)) => Ok(format!(
            "    → rmap explain {key}  (run from the repo root — working directory {reason})\n"
        )),
        (None, None) => Err("next has neither cwd nor a reason".to_string()),
    }
}

/// Render the current-DTO owning-module hint (`ModuleHint`, externally tagged):
/// `Some(", module <path>")` when genuine, `Some(", module: <reason>")` when explicitly
/// unavailable, `None` (malformed) otherwise — the caller surfaces it, never "no module".
fn render_seed_chunk_module_hint(module: Option<&Value>) -> Option<String> {
    let m = module?.as_object()?;
    if let Some(path) = m.get("owning").and_then(|v| v.as_str()) {
        return Some(format!(", module {path}"));
    }
    if let Some(reason) = m.get("unavailable").and_then(|v| v.as_str()) {
        return Some(format!(", module: {reason}"));
    }
    None
}

/// CURSOR-ROUNDTRIP-1 (§2.2, STANDING HONESTY RULE 1): the ONE honest line for candidates
/// that failed validation — the count plus the distinct reasons (order-preserving), never
/// a per-row placeholder. Shared by Group B and `find`'s seed tier.
pub(crate) fn render_unreadable_summary(reasons: &[String]) -> String {
    let mut distinct: Vec<&String> = Vec::new();
    for r in reasons {
        if !distinct.contains(&r) {
            distinct.push(r);
        }
    }
    let joined = distinct
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    let n = reasons.len();
    let plural = if n == 1 { "" } else { "s" };
    format!("  {n} candidate{plural} unreadable: {joined}\n")
}

/// Shared candidate body (identity line + `next` follow-up) for the Group-A `orient`/
/// `explain` no-match candidate list (`orient_sections`), whose `FocusCandidate`
/// serializes the path under `file` and carries no per-symbol span/qualified-name. `c`
/// uses the §8.2 field `file`. (Group B moved to [`render_seed_chunk_candidate`] in
/// CURSOR-ROUNDTRIP-1 §2.2; this now has ONE concrete caller.) A genuinely-absent required
/// field is surfaced as a malformed candidate, NEVER fabricated (STANDING HONESTY RULE).
pub(crate) fn render_candidate_body(c: &Value, file_field: &str) -> String {
    let file = c.get(file_field).and_then(|v| v.as_str());
    let key = c.get("stable_key").and_then(|v| v.as_str());
    let score = c.get("score").and_then(|v| v.as_f64());
    let model = c.get("model_id").and_then(|v| v.as_str());
    // STANDING HONESTY RULE (review-10 #4): the provenance label is the daemon's own
    // `source`, VALIDATED — never a hardcoded "embedding" pasted over the payload. Group A
    // (`orient_sections`/`explain`) pre-filters `source == "embedding"`, so for it this
    // guard always passes (byte-identical); Group B (the `symbol not found` error `data`,
    // not our DTO) has NO such filter — a malformed / foreign / old-daemon candidate whose
    // `source` is absent or not `"embedding"` is surfaced as malformed here, never relabeled
    // as an embedding hint.
    let source = c.get("source").and_then(|v| v.as_str());
    let (Some(file), Some(key), Some(score), Some(model), Some(source)) =
        (file, key, score, model, source)
    else {
        return format!(
            "(malformed candidate: missing {file_field}/stable_key/score/model_id/source)\n"
        );
    };
    if source != "embedding" {
        return format!(
            "(malformed candidate: source {source:?} is not a Layer-3 embedding hint)\n"
        );
    }
    let module = render_module_hint(c.get("module"));
    // `source` is validated == "embedding"; render the daemon's own label, not a literal.
    let mut out = format!("{file}  (score {score:.2}, {source}, model {model}{module})\n");
    out.push_str(&render_next(c.get("next"), key));
    out
}

/// Render the owning-module hint (`ModuleHint`, externally tagged) — `, module <path>`
/// when genuine, `, module: <reason>` when explicitly unavailable, and an explicit
/// malformed marker otherwise (never "no module").
pub(crate) fn render_module_hint(module: Option<&Value>) -> String {
    let Some(m) = module.and_then(|v| v.as_object()) else {
        return ", module: (malformed — no hint)".to_string();
    };
    if let Some(path) = m.get("owning").and_then(|v| v.as_str()) {
        return format!(", module {path}");
    }
    if let Some(reason) = m.get("unavailable").and_then(|v| v.as_str()) {
        return format!(", module: {reason}");
    }
    ", module: (malformed — expected owning/unavailable)".to_string()
}

/// Render a candidate's `next` follow-up. `cwd` optional (operator ruling 2): present
/// ⇒ the `cd <cwd> && …` hint; absent ⇒ the honest reason, never a fabricated cwd.
pub(crate) fn render_next(next: Option<&Value>, key: &str) -> String {
    let Some(n) = next.and_then(|v| v.as_object()) else {
        return "     (malformed candidate: missing next follow-up)\n".to_string();
    };
    let cwd = n.get("cwd").and_then(|v| v.as_str());
    let unavailable = n.get("cwd_unavailable").and_then(|v| v.as_str());
    // review-3: same runnable-cursor discipline as `render_seed_chunk_next` — quote both
    // `cwd` and `key` through `shell_quote_arg`, including the no-`cwd` key form. A Group-A
    // FILE key (`uid:path:FILE`, no `#`) stays bare (byte-identical); a `#`-bearing key or a
    // space-bearing path is single-quoted so the copy-paste runs verbatim.
    let key = crate::presentation::shell_quote_arg(key);
    match (cwd, unavailable) {
        (Some(cwd), _) => format!(
            "     → (cd {} && rmap explain {key})\n",
            crate::presentation::shell_quote_arg(cwd)
        ),
        (None, Some(reason)) => format!(
            "     → rmap explain {key}  (run from the repo root — working directory {reason})\n"
        ),
        (None, None) => {
            "     (malformed candidate: next has neither cwd nor a reason)\n".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_fired_candidates_with_labels() {
        // CURSOR-ROUNDTRIP-1 (§2.2): the Group-B fallback now renders the CURRENT
        // SEED-CHUNK-1 shape (`path`/`line`/`qualified_name`/`is_test`) through the same
        // renderer `find` uses — the `path:line` anchor + qualified name, not a bare file.
        let data = json!({
            "semantic_candidates": [{
                "stable_key": "glamCRM:src/a.ts#svc:SYMBOL:FUNCTION",
                "path": "src/a.ts",
                "line": 12,
                "qualified_name": "svc",
                "is_test": false,
                "score": 0.71,
                "source": "embedding",
                "model_id": "m",
                "module": {"owning": "backend/services"},
                "next": {"cmd": "explain", "args": ["glamCRM:src/a.ts#svc:SYMBOL:FUNCTION"], "cwd": "/repo"}
            }],
            "hint": "no such symbol; these symbols are semantically near your query"
        });
        let out = render_symbol_not_found_semantic(Some(&data)).expect("renders");
        assert!(out.contains("hint: no such symbol"));
        assert!(out.contains("src/a.ts:12  svc"));
        assert!(out.contains("embedding"));
        assert!(out.contains("module backend/services"));
        // review-3: the `#`-bearing symbol key is single-quoted in the full-form fallback
        // (Group B, repo_uid=None) so the copy-paste command runs — an unquoted `#` would
        // start a shell comment and truncate the key.
        assert!(out.contains("(cd /repo && rmap explain 'glamCRM:src/a.ts#svc:SYMBOL:FUNCTION')"));
        assert!(
            !out.contains("rmap explain glamCRM:src/a.ts#svc:SYMBOL:FUNCTION)"),
            "must NOT print the unquoted `#`-bearing key: {out}"
        );
        // The bug this slice fixes: a well-formed current-DTO candidate must NOT render as
        // a malformed placeholder (the pre-slice `file`-shaped renderer printed exactly that).
        assert!(
            !out.contains("malformed"),
            "no placeholder for a valid candidate: {out}"
        );
        assert!(
            !out.contains("unreadable"),
            "nothing unreadable here: {out}"
        );
    }

    #[test]
    fn test_classified_chunk_is_labeled_and_anchored() {
        // SEED-CHUNK-1 §5 moat: a test-classified chunk is anchored AND labeled `[test]`.
        let data = json!({
            "semantic_candidates": [{
                "stable_key": "r:src/a_test.ts#t:SYMBOL:FUNCTION",
                "path": "src/a_test.ts", "line": 5, "qualified_name": "t", "is_test": true,
                "score": 0.63, "source": "embedding", "model_id": "m",
                "module": {"owning": "db"},
                "next": {"cmd": "explain", "args": ["r:src/a_test.ts#t:SYMBOL:FUNCTION"], "cwd": "/repo"}
            }],
            "hint": "h"
        });
        let out = render_symbol_not_found_semantic(Some(&data)).expect("renders");
        assert!(out.contains("src/a_test.ts:5  t"), "{out}");
        assert!(out.contains("[test]"), "test chunk labeled: {out}");
    }

    #[test]
    fn missing_is_decl_renders_unknown_marker_never_unlabeled_impl() {
        // review-3 item 3: an old/foreign-daemon candidate with NO `is_decl` field must
        // render `[is_decl unknown]`, symmetric with `[is_test unknown]` — never the
        // unlabeled implementation default (which would hide an unknown classification).
        let data = json!({
            "semantic_candidates": [{
                "stable_key": "r:src/a.rs#s:SYMBOL:FUNCTION",
                "path": "src/a.rs", "line": 9, "qualified_name": "s", "is_test": false,
                // no is_decl
                "score": 0.7, "source": "embedding", "model_id": "m",
                "module": {"owning": "db"},
                "next": {"cmd": "explain", "args": ["r:src/a.rs#s:SYMBOL:FUNCTION"], "cwd": "/repo"}
            }],
            "hint": "h"
        });
        let out = render_symbol_not_found_semantic(Some(&data)).expect("renders");
        assert!(
            out.contains("[is_decl unknown]"),
            "missing is_decl renders explicit unknown marker: {out}"
        );
        assert!(
            !out.contains("(decl)"),
            "no fabricated (decl) over unknown: {out}"
        );
    }

    #[test]
    fn is_decl_true_labeled_decl_false_unlabeled() {
        // A body-bearing chunk (is_decl=false) is the unlabeled implementation default; a
        // bodyless one (is_decl=true) is labeled `(decl)`. Neither renders the unknown marker.
        let base = |is_decl: bool| {
            json!({
                "semantic_candidates": [{
                    "stable_key": "r:src/a.rs#s:SYMBOL:FUNCTION",
                    "path": "src/a.rs", "line": 9, "qualified_name": "s", "is_test": false,
                    "is_decl": is_decl,
                    "score": 0.7, "source": "embedding", "model_id": "m",
                    "module": {"owning": "db"},
                    "next": {"cmd": "explain", "args": ["r:src/a.rs#s:SYMBOL:FUNCTION"], "cwd": "/repo"}
                }],
                "hint": "h"
            })
        };
        let decl = render_symbol_not_found_semantic(Some(&base(true))).expect("renders");
        assert!(
            decl.contains("(decl)"),
            "is_decl=true labeled (decl): {decl}"
        );
        assert!(!decl.contains("unknown"), "not unknown: {decl}");
        let imp = render_symbol_not_found_semantic(Some(&base(false))).expect("renders");
        assert!(
            !imp.contains("(decl)"),
            "is_decl=false is unlabeled impl: {imp}"
        );
        assert!(!imp.contains("[is_decl unknown]"), "not unknown: {imp}");
    }

    #[test]
    fn degraded_hint_only_no_candidate_block() {
        let data = json!({ "hint": "no local embedding model reachable" });
        let out = render_symbol_not_found_semantic(Some(&data)).expect("renders");
        assert!(out.contains("hint: no local embedding model reachable"));
        assert!(!out.contains("Semantic candidates"));
    }

    #[test]
    fn no_seed_data_returns_none_so_todays_error_is_untouched() {
        // An error with no seed keys (e.g. AmbiguousSymbol's `matches`, or no data).
        assert!(render_symbol_not_found_semantic(None).is_none());
        assert!(render_symbol_not_found_semantic(Some(&json!({ "matches": [] }))).is_none());
    }

    #[test]
    fn absent_required_field_is_counted_one_line_never_per_row_placeholder() {
        // STANDING HONESTY RULE 1: a candidate missing required fields is COUNTED and
        // stated on ONE honest line — never a per-row `(malformed candidate: …)` placeholder.
        let data = json!({
            "semantic_candidates": [{ "path": "src/a.ts" }], // missing score/model/key/source
            "hint": "h"
        });
        let out = render_symbol_not_found_semantic(Some(&data)).expect("renders");
        assert!(
            out.contains("1 candidate unreadable: missing required field"),
            "one honest counted line: {out}"
        );
        assert!(
            !out.contains("(malformed candidate"),
            "no per-row placeholder (RULE 1): {out}"
        );
    }

    #[test]
    fn multiple_unreadable_are_counted_once_with_distinct_reasons() {
        // Two candidates fail for the SAME reason → one line, count 2, reason stated once.
        let data = json!({
            "semantic_candidates": [{ "path": "src/a.ts" }, { "path": "src/b.ts" }],
            "hint": "h"
        });
        let out = render_symbol_not_found_semantic(Some(&data)).expect("renders");
        assert!(
            out.contains("2 candidates unreadable: missing required field"),
            "counted once with count 2: {out}"
        );
    }

    #[test]
    fn non_embedding_source_is_counted_never_relabeled_embedding() {
        // A fully-formed candidate whose `source` is NOT "embedding" (a foreign/old-daemon
        // payload) must NOT be presented as an embedding hint; it is counted unreadable with
        // the offending source named.
        let data = json!({
            "semantic_candidates": [{
                "stable_key": "glamCRM:src/a.ts#s:SYMBOL:FUNCTION",
                "path": "src/a.ts", "line": 1, "qualified_name": "s", "is_test": false,
                "score": 0.71,
                "source": "lexical",
                "model_id": "m",
                "module": {"owning": "db"},
                "next": {"cmd": "explain", "args": ["glamCRM:src/a.ts#s:SYMBOL:FUNCTION"], "cwd": "/repo"}
            }],
            "hint": "h"
        });
        let out = render_symbol_not_found_semantic(Some(&data)).expect("renders");
        assert!(out.contains("unreadable"), "surfaced as unreadable: {out}");
        assert!(
            out.contains("\"lexical\""),
            "names the offending source: {out}"
        );
        assert!(
            !out.contains("score 0.71, embedding"),
            "must NOT relabel a non-embedding source as an embedding hint: {out}"
        );
    }

    #[test]
    fn missing_source_is_counted_never_relabeled_embedding() {
        // A candidate with every other field but no `source` at all is unreadable, never a
        // fabricated "embedding" label.
        let data = json!({
            "semantic_candidates": [{
                "stable_key": "glamCRM:src/a.ts#s:SYMBOL:FUNCTION",
                "path": "src/a.ts", "line": 1, "qualified_name": "s", "is_test": false,
                "score": 0.71,
                "model_id": "m",
                "module": {"owning": "db"},
                "next": {"cmd": "explain", "args": ["glamCRM:src/a.ts#s:SYMBOL:FUNCTION"], "cwd": "/repo"}
            }],
            "hint": "h"
        });
        let out = render_symbol_not_found_semantic(Some(&data)).expect("renders");
        assert!(out.contains("unreadable"), "surfaced as unreadable: {out}");
        assert!(
            !out.contains("embedding, model"),
            "no fabricated embedding label: {out}"
        );
    }

    #[test]
    fn in_root_non_symbol_key_keeps_full_cursor_because_alias_rejects_short_form() {
        // review-1 finding 2d (boundary): the `explain` reattach alias reattaches the uid
        // ONLY to a prefix-less suffix carrying `:SYMBOL`. An in-root key WITHOUT that marker
        // (a file/path-level or malformed seed key), if shortened, would be read by `explain`
        // as a PATH, miss the reattach, and NOT round-trip. So `render_seed_chunk_next` keeps
        // the known-good full self-contained `(cd … && rmap explain <key>)` follow-up for it.
        let uid = "leveldb-abc123";
        let key = format!("{uid}:db/version_set.cc"); // in-root prefix, but NO :SYMBOL marker
        let next = json!({ "cwd": "/repo" });
        let out = render_seed_chunk_next(Some(&next), &key, Some(uid)).expect("renders");
        assert!(
            out.contains(&format!("(cd /repo && rmap explain {key})")),
            "a non-symbol in-root key keeps the full round-tripping cursor: {out}"
        );
        assert!(
            !out.contains("→ rmap explain db/version_set.cc\n"),
            "must NOT emit a shortened cursor the reattach alias would reject: {out}"
        );
    }

    #[test]
    fn in_root_symbol_key_is_shortened_and_round_trips() {
        // The complement: a prefix-less suffix carrying `:SYMBOL` is EXACTLY what the alias
        // reattaches, so shortening is safe and `rmap explain <suffix>` runs verbatim.
        //
        // review-2 finding 2: the suffix ALWAYS carries a `#` (a shell COMMENT char), so the
        // short cursor is single-quoted by `presentation::shell_quote_arg` — the SAME encoder
        // the fact tier uses. The shell strips the quotes before `rmap` sees the arg, so the
        // reattach alias resolves the identical node; an UNQUOTED form would be truncated at
        // the `#` and would not round-trip.
        let uid = "leveldb-abc123";
        let suffix = "db/version_set.cc#leveldb::Builder::MaybeAddFile:SYMBOL:METHOD";
        let key = format!("{uid}:{suffix}");
        let next = json!({ "cwd": "/repo" });
        let out = render_seed_chunk_next(Some(&next), &key, Some(uid)).expect("renders");
        assert_eq!(
            out,
            format!(
                "    → rmap explain {}\n",
                crate::presentation::shell_quote_arg(suffix)
            )
        );
        // The rendered suffix is single-quoted (the `#` forces quoting), so the `#` is inside
        // the quotes — no early comment truncation.
        assert!(
            out.contains(&format!("'{suffix}'")),
            "the `#`-bearing suffix is single-quoted so it runs verbatim: {out}"
        );
        assert!(
            !out.contains(&format!("explain {suffix}\n")),
            "must NOT print the unquoted suffix (the `#` would start a shell comment): {out}"
        );
    }

    #[test]
    fn in_root_short_cursor_shell_quotes_a_metacharacter_bearing_suffix() {
        // review-2 finding 2 (metacharacter round-trip): a suffix whose path carries a SPACE
        // (on top of the always-present `#`) must be wrapped as ONE single-quoted argument, so
        // the copy-pasted cursor runs as a single non-injecting `rmap explain <arg>` — the
        // space never splits the argument, the `#` never starts a comment.
        let uid = "leveldb-abc123";
        let suffix = "db/my file.cc#leveldb::Recover:SYMBOL:METHOD";
        let key = format!("{uid}:{suffix}");
        let next = json!({ "cwd": "/repo" });
        let out = render_seed_chunk_next(Some(&next), &key, Some(uid)).expect("renders");
        assert_eq!(
            out, "    → rmap explain 'db/my file.cc#leveldb::Recover:SYMBOL:METHOD'\n",
            "the whole suffix is one single-quoted argument: {out}"
        );
    }

    #[test]
    fn full_form_seed_next_quotes_hash_key_and_space_cwd() {
        // review-3: the full-form fallback (`repo_uid = None` — Group B / degraded) renders a
        // COPY-PASTE command. The key ALWAYS carries `#` (a shell comment char) and the cwd
        // may carry a space, so BOTH are single-quoted; an unquoted form would truncate at the
        // `#` and word-split the path — a non-runnable cursor (STANDING HONESTY RULE 2).
        let key = "leveldb-uid:db/my file.cc#leveldb::Recover:SYMBOL:METHOD";
        let next = json!({ "cwd": "/my repo/leveldb" });
        let out = render_seed_chunk_next(Some(&next), key, None).expect("renders");
        assert_eq!(
            out,
            "    → (cd '/my repo/leveldb' && rmap explain 'leveldb-uid:db/my file.cc#leveldb::Recover:SYMBOL:METHOD')\n",
            "both cwd (space) and key (# + space) are single-quoted: {out}"
        );
    }

    #[test]
    fn full_form_seed_next_out_of_root_key_is_quoted() {
        // A header uid EXISTS but the key is out-of-root (no `<uid>:` prefix), so the short
        // cursor is not taken and the full form is rendered — the `#`-bearing foreign key is
        // still quoted so it runs verbatim.
        let key = "OTHER-uid:x.ts#f:SYMBOL:FUNCTION";
        let next = json!({ "cwd": "/other/repo" });
        let out = render_seed_chunk_next(Some(&next), key, Some("leveldb-uid")).expect("renders");
        assert_eq!(
            out, "    → (cd /other/repo && rmap explain 'OTHER-uid:x.ts#f:SYMBOL:FUNCTION')\n",
            "out-of-root key quoted (cwd safe, left bare): {out}"
        );
    }

    #[test]
    fn full_form_seed_next_no_cwd_reason_form_quotes_key() {
        // The no-`cwd` reason arm is a copy-paste command too — quote the `#`-bearing key.
        let key = "leveldb-uid:db/db_impl.cc#Recover:SYMBOL:FUNCTION";
        let next = json!({ "cwd_unavailable": "was deleted" });
        let out = render_seed_chunk_next(Some(&next), key, None).expect("renders");
        assert_eq!(
            out,
            "    → rmap explain 'leveldb-uid:db/db_impl.cc#Recover:SYMBOL:FUNCTION'  (run from the repo root — working directory was deleted)\n",
            "no-cwd key form is quoted: {out}"
        );
    }

    #[test]
    fn full_form_seed_next_embedded_quote_key_is_escaped() {
        // Embedded single quote in the key → the `'\''` POSIX escape, wrapped, so the whole
        // key is one argument (no injection, no early termination).
        let key = "r:db/o'brien.cc#f:SYMBOL:FUNCTION";
        let next = json!({ "cwd": "/repo" });
        let out = render_seed_chunk_next(Some(&next), key, None).expect("renders");
        assert_eq!(
            out, "    → (cd /repo && rmap explain 'r:db/o'\\''brien.cc#f:SYMBOL:FUNCTION')\n",
            "embedded quote is escaped: {out}"
        );
    }

    #[test]
    fn render_next_group_a_quotes_hash_key_and_space_cwd() {
        // review-3: the Group-A full-form (`render_next`, used by `orient`/`explain` no-match
        // candidate lists) has the SAME copy-paste contract — a `#`-bearing key and a
        // space-bearing cwd are both single-quoted; a plain FILE key (below) stays bare.
        let next = json!({ "cwd": "/my repo" });
        let out = render_next(Some(&next), "r:src/a.ts#svc:SYMBOL:FUNCTION");
        assert_eq!(
            out, "     → (cd '/my repo' && rmap explain 'r:src/a.ts#svc:SYMBOL:FUNCTION')\n",
            "Group-A full form quotes both cwd and key: {out}"
        );
    }

    #[test]
    fn render_next_group_a_safe_file_key_stays_bare() {
        // A FILE key (`uid:path:FILE`, no `#`/space) is a POSIX-safe token, so the encoder
        // leaves it bare — the pre-slice output is byte-identical (no gratuitous quoting).
        let next = json!({ "cwd": "/repo" });
        let out = render_next(Some(&next), "glamCRM:src/price.ts:FILE");
        assert_eq!(
            out, "     → (cd /repo && rmap explain glamCRM:src/price.ts:FILE)\n",
            "safe file key is not quoted: {out}"
        );
    }

    #[test]
    fn valid_and_unreadable_mix_renders_row_then_one_counted_line() {
        // A valid candidate renders its row; a malformed sibling is counted on one line —
        // the valid row is never suppressed, the bad one never a placeholder.
        let data = json!({
            "semantic_candidates": [
                {
                    "stable_key": "r:src/a.ts#ok:SYMBOL:FUNCTION",
                    "path": "src/a.ts", "line": 3, "qualified_name": "ok", "is_test": false,
                    "score": 0.7, "source": "embedding", "model_id": "m",
                    "module": {"owning": "db"},
                    "next": {"cmd": "explain", "args": ["r:src/a.ts#ok:SYMBOL:FUNCTION"], "cwd": "/repo"}
                },
                { "path": "src/bad.ts" }
            ],
            "hint": "h"
        });
        let out = render_symbol_not_found_semantic(Some(&data)).expect("renders");
        assert!(out.contains("src/a.ts:3  ok"), "valid row rendered: {out}");
        assert!(
            out.contains("1 candidate unreadable"),
            "one counted line for the bad one: {out}"
        );
    }
}
