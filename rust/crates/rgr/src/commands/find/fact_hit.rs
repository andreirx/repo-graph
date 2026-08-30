//! One fact hit's rendering + boundary validation (FIND-FACTS-1 §2.2). Splitting
//! this off `facts_render` keeps both files under the 500-line guardrail (review-4
//! item 1) and isolates the two per-hit honesty checks the reviewer hardened:
//!   - the PATH dimension (review-4 item 2): a KNOWN path, an UNKNOWN-with-reason, or
//!     an absent path dimension — never a silent omission standing in for an unknown.
//!   - the NEXT command (review-4 item 3): re-validated against the ratified runnable
//!     form for the hit's class, so a malformed payload cannot emit arbitrary text
//!     after `rmap `.
//!
//! Abstraction record — module: `find::fact_hit`; concrete current user:
//! `facts_render::render_facts_tier` (the sole caller of [`render_fact_hit`]); axis:
//! the ≤500-line guardrail; rejected simpler alternative: inlining in `facts_render`
//! (pushes that file back over 500 with the new validation + tests).

/// Render one fact hit: an identity line `    <display>  — <path>` (path rendered per
/// its known / unknown-with-reason / absent state), THEN the runnable next command
/// `      → rmap <next>` for THIS hit — but ONLY after `next` is validated against the
/// ratified runnable form(s) for the class (review-4 item 3). `commands` is the class's
/// already-validated ratified command set (one for the single-renderer classes, the
/// governance set `{violations, gate}` for boundary — review-6); `folds_key` is whether
/// the class folds the hit key into an argument (`explain <key>`) or renders a bare
/// whole-listing command.
pub(super) fn render_fact_hit(h: &serde_json::Value, commands: &[&str], folds_key: bool) -> String {
    let display = h.get("display").and_then(|v| v.as_str());
    let Some(display) = display else {
        return "    (malformed fact hit: missing display)\n".to_string();
    };
    // `path` (KNOWN) is OPTIONAL (skip-serialized when the class carries none or the
    // hit's path is unknown): ABSENT, or a string. A present-but-non-string `path` is
    // MALFORMED — surfaced, NEVER silently dropped (review-2 item 3).
    let path = match h.get("path") {
        None => None,
        Some(serde_json::Value::String(p)) => Some(p.as_str()),
        Some(_) => return "    (malformed fact hit: path present but not a string)\n".to_string(),
    };
    // `path_unknown_reason` (review-4 item 2) is OPTIONAL: ABSENT, or a NON-EMPTY
    // string. Present with `path` is contradictory (a path is either known or not) →
    // MALFORMED; a present-but-empty / non-string reason is MALFORMED. When set it
    // renders `path unknown (<reason>)` — the unknown owning file, never a silent
    // omission that reads as "this class has no path".
    let path_unknown_reason = match h.get("path_unknown_reason") {
        None => None,
        Some(serde_json::Value::String(r)) if !r.is_empty() => Some(r.as_str()),
        Some(_) => {
            return "    (malformed fact hit: path_unknown_reason present but not a non-empty string)\n"
                .to_string()
        }
    };
    let mut line = match (path, path_unknown_reason) {
        (Some(_), Some(_)) => {
            return "    (malformed fact hit: both path and path_unknown_reason present)\n"
                .to_string()
        }
        // A concrete owning path distinct from the display (the `file` class's path
        // equals its display, so it is not repeated).
        (Some(p), None) if p != display => format!("    {display}  — {p}\n"),
        (Some(_), None) => format!("    {display}\n"),
        // The class HAS a path dimension but this hit's path is unknown — shown WITH
        // its reason (review-4 item 2), never omitted.
        (None, Some(reason)) => format!("    {display}  — path unknown ({reason})\n"),
        // No path dimension at all (dependency, framework): a clean identity line.
        (None, None) => format!("    {display}\n"),
    };
    // `key` is OPTIONAL (the argument-taking classes carry one; the list classes do
    // not). A present-but-non-string key is MALFORMED — it feeds the `next`
    // validation, so a corrupt key must not silently pass.
    let key = match h.get("key") {
        None => None,
        Some(serde_json::Value::String(k)) => Some(k.as_str()),
        Some(_) => {
            line.push_str("      (malformed fact hit: key present but not a string)\n");
            return line;
        }
    };
    // `next` is our OWN DTO field, ALWAYS serialized (the runnable invocation). It
    // must be a NON-EMPTY string AND the ratified runnable form for this class+key
    // (review-4 item 3): a missing / non-string / EMPTY / non-ratified `next` is
    // MALFORMED — surfaced, NEVER rendered as `→ rmap <arbitrary text>`.
    match h.get("next").and_then(|v| v.as_str()) {
        Some("") | None => {
            line.push_str("      (malformed fact hit: missing or empty next command)\n");
        }
        Some(next) if next_is_ratified(next, commands, folds_key, key) => {
            line.push_str(&format!("      → rmap {next}\n"));
        }
        Some(_) => {
            line.push_str(
                "      (malformed fact hit: next command is not a ratified runnable form)\n",
            );
        }
    }
    line
}

/// True iff `next` is EXACTLY a ratified runnable form the daemon emits for a hit of
/// this class (review-4 item 3; review-6 per-hit renderer). The argument-folding
/// classes (`folds_key` — `explain`, `map --dry-run`) render `<cmd> <shell(key)>` and
/// REQUIRE a key (their render is non-runnable without a target — a keyless `explain`
/// exits 1), and always have exactly one ratified command. The whole-listing classes
/// render a bare command that must be ONE of the ratified set (a singleton for the
/// six fixed classes; the governance set `{violations, gate}` for boundary).
/// Reconstructs/matches the expected string exactly, so any deviation — injected text,
/// a wrong verb, a second argument, an unquoted metacharacter, a command outside the
/// set — fails.
fn next_is_ratified(next: &str, commands: &[&str], folds_key: bool, key: Option<&str>) -> bool {
    if folds_key {
        // Argument-folding classes: exactly one ratified command, `<cmd> <shell(key)>`,
        // key required. A keyless form is non-runnable — rejected.
        match (commands, key) {
            ([cmd], Some(k)) => next == format!("{cmd} {}", shell_quote_arg(k)),
            _ => false,
        }
    } else {
        // Whole-listing classes: `next` is the bare command, and must be ONE of the
        // ratified set (boundary's per-hit renderer is `violations` OR `gate`).
        commands.contains(&next)
    }
}

/// POSIX single-quote encoder — a deliberate MIRROR of the daemon's
/// `find_facts::shell_arg` (the emitter), so this validator reconstructs the exact
/// `next` the daemon produced. The two MUST encode the identical rule; a divergence
/// renders a valid hit as malformed (loud, fail-safe), never a wrong command as
/// runnable. A small, security-relevant duplication kept inline rather than crossing
/// a crate boundary for a shared helper (that would be a new dependency edge — a
/// boundary decision out of this focused close's scope).
fn shell_quote_arg(arg: &str) -> String {
    let safe = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | ':' | '@'));
    if safe {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── PATH dimension (review-4 item 2) ────────────────────────────────────────

    #[test]
    fn known_path_distinct_from_display_is_shown() {
        let h = json!({"display": "bnrService", "path": "src/bnr.ts", "key": "k1", "next": "explain k1"});
        let out = render_fact_hit(&h, &["explain"], true);
        assert!(out.contains("bnrService  — src/bnr.ts"), "{out}");
        assert!(out.contains("→ rmap explain k1"), "{out}");
    }

    #[test]
    fn path_equal_to_display_is_not_repeated() {
        // The `file` class: path == display, so no `— <path>` suffix.
        let h = json!({"display": "src/f.ts", "path": "src/f.ts", "key": "src/f.ts", "next": "explain src/f.ts"});
        let out = render_fact_hit(&h, &["explain"], true);
        assert_eq!(
            out, "    src/f.ts\n      → rmap explain src/f.ts\n",
            "{out}"
        );
    }

    #[test]
    fn unknown_path_renders_reason_never_silent_omission() {
        // review-4 item 2: an unknown owning file is shown WITH its reason, not
        // omitted as if the class had no path dimension.
        let h = json!({
            "display": "orphanSym", "key": "k9", "next": "explain k9",
            "path_unknown_reason": "owning file unresolved (no files row for this symbol's file_uid)"
        });
        let out = render_fact_hit(&h, &["explain"], true);
        assert!(
            out.contains("orphanSym  — path unknown (owning file unresolved"),
            "unknown path shows reason: {out}"
        );
    }

    #[test]
    fn no_path_dimension_is_a_clean_identity_line() {
        // dependency/framework: neither `path` nor `path_unknown_reason` → clean line.
        let h = json!({"display": "lodash", "key": "lodash", "next": "deps list"});
        let out = render_fact_hit(&h, &["deps list"], false);
        assert_eq!(out, "    lodash\n      → rmap deps list\n", "{out}");
    }

    #[test]
    fn both_path_and_reason_is_malformed() {
        let h = json!({
            "display": "x", "path": "src/x.ts",
            "path_unknown_reason": "somehow", "key": "k", "next": "explain k"
        });
        let out = render_fact_hit(&h, &["explain"], true);
        assert!(
            out.contains("malformed fact hit: both path and path_unknown_reason present"),
            "{out}"
        );
    }

    #[test]
    fn non_string_path_is_malformed_never_dropped() {
        let h = json!({"display": "x", "path": 42, "key": "k", "next": "explain k"});
        let out = render_fact_hit(&h, &["explain"], true);
        assert!(
            out.contains("malformed fact hit: path present but not a string"),
            "{out}"
        );
    }

    #[test]
    fn empty_path_unknown_reason_is_malformed() {
        let h = json!({"display": "x", "path_unknown_reason": "", "key": "k", "next": "explain k"});
        let out = render_fact_hit(&h, &["explain"], true);
        assert!(
            out.contains(
                "malformed fact hit: path_unknown_reason present but not a non-empty string"
            ),
            "{out}"
        );
    }

    // ── NEXT command validation (review-4 item 3) ───────────────────────────────

    #[test]
    fn arbitrary_next_text_is_rejected_never_pasted_after_rmap() {
        // A tampered / old-daemon payload puts arbitrary text in `next`; it must NOT
        // be rendered as `→ rmap <text>` (review-4 item 3).
        let h = json!({"display": "x", "path": "src/x.ts", "key": "k1", "next": "explain k1; rm -rf /"});
        let out = render_fact_hit(&h, &["explain"], true);
        assert!(
            out.contains("malformed fact hit: next command is not a ratified runnable form"),
            "arbitrary next rejected: {out}"
        );
        assert!(
            !out.contains("rm -rf"),
            "no arbitrary text after rmap: {out}"
        );
    }

    #[test]
    fn next_with_wrong_verb_is_rejected() {
        // `next` verb disagrees with the group's ratified render command.
        let h = json!({"display": "x", "path": "src/x.ts", "key": "k1", "next": "callers k1"});
        let out = render_fact_hit(&h, &["explain"], true);
        assert!(
            out.contains("not a ratified runnable form"),
            "wrong verb rejected: {out}"
        );
    }

    #[test]
    fn list_class_next_must_be_the_bare_command() {
        // A list class whose `next` carries a spurious argument is rejected.
        let h =
            json!({"display": "provider GET /x", "path": "src/x.ts", "next": "boundaries list x"});
        let out = render_fact_hit(&h, &["boundaries list"], false);
        assert!(out.contains("not a ratified runnable form"), "{out}");
        // The valid bare form is accepted.
        let ok =
            json!({"display": "provider GET /x", "path": "src/x.ts", "next": "boundaries list"});
        let out_ok = render_fact_hit(&ok, &["boundaries list"], false);
        assert!(out_ok.contains("→ rmap boundaries list\n"), "{out_ok}");
    }

    #[test]
    fn boundary_per_hit_next_must_be_in_the_governance_set() {
        // review-6 re-home: the boundary class has a per-hit renderer set
        // {violations, gate}. Each is accepted as a bare whole-listing next; a command
        // OUTSIDE the set (e.g. the dropped `surfaces list`) is rejected.
        let viol =
            json!({"display": "boundary declaration · r:src/core:MODULE", "next": "violations"});
        assert!(
            render_fact_hit(&viol, &["violations", "gate"], false).contains("→ rmap violations\n"),
            "violations accepted for a boundary-kind declaration"
        );
        let gate =
            json!({"display": "requirement declaration · r:requirement:REQ-1:1", "next": "gate"});
        assert!(
            render_fact_hit(&gate, &["violations", "gate"], false).contains("→ rmap gate\n"),
            "gate accepted for a requirement declaration"
        );
        // The dropped entrypoint renderer is NOT in the set → rejected, never pasted.
        let stale =
            json!({"display": "boundary declaration · r:src/core:MODULE", "next": "surfaces list"});
        let out = render_fact_hit(&stale, &["violations", "gate"], false);
        assert!(out.contains("not a ratified runnable form"), "{out}");
        assert!(
            !out.contains("→ rmap surfaces list"),
            "stale renderer not pasted: {out}"
        );
    }

    #[test]
    fn argument_class_without_key_is_rejected() {
        // `explain`/`map --dry-run` REQUIRE a key; a keyless bare `explain` is
        // non-runnable and must not render (review-4 item 3, "required key").
        let h = json!({"display": "x", "path": "src/x.ts", "next": "explain"});
        let out = render_fact_hit(&h, &["explain"], true);
        assert!(out.contains("not a ratified runnable form"), "{out}");
    }

    #[test]
    fn shell_quoted_key_next_is_accepted() {
        // A key with a space is single-quoted by the daemon; the validator
        // reconstructs the SAME encoding and accepts it.
        let h = json!({"display": "x", "path": "src/my file.ts", "key": "src/my file.ts", "next": "explain 'src/my file.ts'"});
        let out = render_fact_hit(&h, &["explain"], true);
        assert!(out.contains("→ rmap explain 'src/my file.ts'\n"), "{out}");
    }

    #[test]
    fn map_dry_run_next_with_key_is_accepted() {
        let h = json!({"display": "api", "path": "packages/api", "key": "packages/api", "next": "map --dry-run packages/api"});
        let out = render_fact_hit(&h, &["map --dry-run"], true);
        assert!(out.contains("→ rmap map --dry-run packages/api\n"), "{out}");
    }

    #[test]
    fn missing_next_is_malformed_never_unactionable() {
        let h = json!({"display": "x", "path": "src/x.ts", "key": "k1"});
        let out = render_fact_hit(&h, &["explain"], true);
        assert!(
            out.contains("malformed fact hit: missing or empty next command"),
            "{out}"
        );
    }

    #[test]
    fn empty_next_is_malformed_never_dangling_command() {
        let h = json!({"display": "x", "path": "src/x.ts", "key": "k1", "next": ""});
        let out = render_fact_hit(&h, &["explain"], true);
        assert!(
            out.contains("malformed fact hit: missing or empty next command"),
            "{out}"
        );
        assert!(!out.contains("→ rmap \n"), "no dangling command: {out}");
    }

    #[test]
    fn non_string_key_is_malformed() {
        let h = json!({"display": "x", "path": "src/x.ts", "key": 7, "next": "explain 7"});
        let out = render_fact_hit(&h, &["explain"], true);
        assert!(
            out.contains("malformed fact hit: key present but not a string"),
            "{out}"
        );
    }

    #[test]
    fn shell_quote_arg_mirrors_the_daemon_encoder() {
        // Safe keys stay bare; spaces / metacharacters are single-quoted with the
        // embedded-quote escape — identical to `find_facts::shell_arg`.
        assert_eq!(
            shell_quote_arg("glamCRM:src/bnr.ts:BNRService"),
            "glamCRM:src/bnr.ts:BNRService"
        );
        assert_eq!(shell_quote_arg("a b"), "'a b'");
        assert_eq!(shell_quote_arg("a'b"), "'a'\\''b'");
    }
}
