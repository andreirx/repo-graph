//! Usage help and error formatting.

/// Format a `GateError` using the stderr wording that the
/// pre-relocation `rmap gate` command produced. The
/// relocation changed the error types (gate now returns
/// `GateError` instead of free-form `String` diagnostics), but
/// the CLI test suite pins specific substrings on stderr. This
/// function adapts the new typed errors back to those strings
/// without re-introducing policy coupling in the gate crate.
///
/// When a new operation is added to the gate port, its mapping
/// goes here - not in the gate crate itself, which must stay
/// CLI-agnostic.
pub fn format_gate_error(err: &repo_graph_gate::GateError) -> String {
    use repo_graph_gate::GateError;
    match err {
        GateError::Storage(e) => match e.operation {
            "find_waivers" => format!("failed to read waivers: {}", e.message),
            "get_boundary_declarations" => {
                format!("failed to read boundary declarations: {}", e.message)
            }
            "find_boundary_imports" => {
                format!("failed to query imports between paths: {}", e.message)
            }
            "get_coverage_measurements" => {
                format!("failed to read coverage measurements: {}", e.message)
            }
            "get_complexity_measurements" => {
                format!("failed to read complexity measurements: {}", e.message)
            }
            "get_hotspot_inferences" => {
                format!("failed to read hotspot inferences: {}", e.message)
            }
            // `get_active_requirements` errors bubble up the
            // StorageError's own Display text (which already
            // contains the "malformed requirement ..." wording
            // the old CLI printed).
            _ => e.message.clone(),
        },
        // Malformed measurement/inference rows: the gate
        // assemble layer built the diagnostic string verbatim
        // to match the pre-relocation format
        // ("malformed X measurement for Y: Z" etc.). Passing
        // `reason` directly preserves that.
        GateError::MalformedEvidence { reason, .. } => reason.clone(),
    }
}

/// Bare command form of the `coverage` handler's usage (no `usage: ` prefix). The handler
/// prints it after `usage: `, `--help` prints it as the `coverage` line, and the `risk`
/// command passes it to the risk renderer for its hint (D-HSP-COVERAGE-CONST, D-HSP-CONST-DIRECTION).
pub(crate) const COVERAGE_USAGE: &str = "rmap coverage <report> [--json]";

/// Bare command form of the `declare boundary` handler's usage (no `usage: ` prefix).
pub(crate) const DECLARE_BOUNDARY_USAGE: &str =
    "rmap declare boundary <db_path> <repo_uid> <module_path> --forbids <target> [--reason <text>]";

/// Bare command form of the `declare requirement` handler's usage (no `usage: ` prefix).
pub(crate) const DECLARE_REQUIREMENT_USAGE: &str =
    "rmap declare requirement <db_path> <repo_uid> <req_id> --version <n> --obligation-id <id> --method <method> --obligation <text> [--target <t>] [--threshold <n>] [--operator <op>]";

/// Bare command form of the `declare quality-policy` handler's usage (no `usage: ` prefix;
/// three lines).
pub(crate) const DECLARE_QUALITY_POLICY_USAGE: &str =
    "rmap declare quality-policy <db_path> <repo_uid> <policy_id> \\
  --measurement <kind> --policy-kind <kind> --threshold <n> [--version <n>] \\
  [--severity <fail|advisory>] [--scope-clause <type>:<selector>]... [--description <text>]";

/// Build the main usage help text (`rmap --help`).
///
/// Every top-level command of the dispatcher in `main.rs` has a line that starts
/// `rmap <name>` (the test `help_lists_every_dispatched_command` checks the command names, not
/// the arguments). The `coverage` line and the three lines under
/// `Declarations (positional <db_path> <repo_uid>):` print the usage constants above, which the
/// handlers also print after `usage: `; the shared constant keeps the help and handler TEXT
/// identical, it does not keep that text aligned with the handler's argument parser.
pub fn usage_text() -> String {
    format!(
        "\
usage:

Indexing (daemon required):
  rmap index [repo_path] [--alias <name>] [--include-root <path>]... [--progress]
  rmap refresh [--include-root <path>]... [--progress]
    --progress   stream inline progress; default is quiet — follow live progress with `rmap doctor`

Enrichment:
  rmap enrich [options]

Repo management:
  rmap repo list                         List all registered repos
  rmap repo info [repo] [--json]         Show repo details (default: cwd)
  rmap repo alias <repo> <alias>         Set or change alias
  rmap repo remove <repo> [--keep-db]    Forget repo: registry + database + .rgr/ (destructive; --keep-db keeps the DB file)
  rmap repo rebuild <repo> [--yes]       Discard the store and reindex from scratch (destructive; keeps the registry entry)

Agent orientation (resolve repo from cwd):
  rmap orient [--focus <path>] [--budget small|medium|large] [--full] [--include-all]
  rmap check [--full]
  rmap explain <target> [--budget medium|large] [--full]
  rmap find \"<concept>\" [--exact] [--full] [--json]
    Fact tables FIRST (deterministic; each hit certainty-tagged + a runnable next command),
    then demoted semantic (embedding) guesses. Answers even when the model is down.
    --exact  facts tables only; the embedding endpoint is never consulted (scriptable form)
    --full   uncap output (no budget / per-class truncation; no-op on check)
  rmap map [path] [--dry-run] [--json]    Deterministic MAP.md docs from the index (no LLM)
    --dry-run  print rendered maps to stdout, write nothing

Graph queries (resolve repo from cwd):
  rmap callers <symbol> [--edge-types <types>] [--include-inferred]
  rmap callees <symbol> [--edge-types <types>] [--include-inferred]
    --include-inferred  also list the calls bound by an inferred (name-only) binding, marked
  rmap path <from> <to> [--include-inferred]
  rmap imports <file_path> [--include-inferred]
  rmap cycles [--include-tests] [--include-inferred]
    --include-tests     also count imports from test files (default: production files only)
    --include-inferred  also count inferred imports / walk inferred edges (default: certain only)
  rmap stats

Quality and risk (resolve repo from cwd):
  rmap trust
  rmap reliability [--by-language] [--by-module] [--json]
  rmap churn [--since <expr>]
  rmap hotspots [--since <expr>] [--exclude-tests] [--exclude-vendored]
  rmap risk
  {COVERAGE_USAGE}
  rmap assess [--baseline <snapshot>]

Governance (resolve repo from cwd):
  rmap violations
  rmap gate

Documentation inventory (resolve repo from cwd):
  rmap docs list
  rmap docs extract

Modules (resolve repo from cwd):
  rmap modules list [--include-tests] [--include-inferred]
  rmap modules files <module>
  rmap modules deps [module] [--outbound|--inbound] [--include-tests] [--include-inferred]
  rmap modules violations

Dependencies:
  rmap deps list [module] [--ecosystem npm|cargo|python|java] [--json]

Surfaces and boundaries (resolve repo from cwd):
  rmap surfaces list [--kind <kind>] [--runtime <rt>] [--module <m>]
  rmap surfaces show <surface_ref>
  rmap boundaries list [--kind <kind>] [--scope <scope>] [--direction <dir>]
  rmap boundaries show <surface_uid>
  rmap boundaries summary

Contracts:
  rmap contracts list [--kind protobuf]

Resources (resolve repo from cwd):
  rmap resource list [--kind <kind>]
  rmap resource readers <resource_key>
  rmap resource writers <resource_key>

Inferences:
  rmap inferences list [--kind <kind>] [--limit <N>] [--json]

Policy (resolve repo from cwd):
  rmap policy [--kind STATUS_MAPPING|BEHAVIORAL_MARKER] [--file <path>]

Declarations (positional <db_path> <repo_uid>):
  {DECLARE_BOUNDARY_USAGE}
  {DECLARE_REQUIREMENT_USAGE}
  {DECLARE_QUALITY_POLICY_USAGE}

Agent host integration (HOOK-1/HOOK-1A):
  rmap hook session-start [--from-stdin | --from-env | --db <path> --repo <path>]
  rmap hook prompt-submit [--from-stdin | --from-env | --db <path> --repo <path>]
  rmap hook post-edit [--from-stdin | --from-env | --db <path> --repo <path> --files <paths>]
  rmap hook pre-compact [--from-stdin | --from-env | --db <path> --repo <path>]
  rmap hook stop [--from-stdin | --from-env | --db <path> --repo <path>]
  rmap hook status

Installation management (MAC-1):
  rmap doctor [--json]
  rmap uninstall [--dry-run] [--force] [--remove-data]

Maintenance (daemon storage):
  rmap maintenance prune                 Prune prunable snapshots (current repo)
  rmap maintenance gc [--dry-run] [--json]  Reclaim orphan DB files + stray sidecars (all repos)

Host integrations (CLAUDE-1, CODEX-1):
  rmap integrate claude-code install [--global|--project] [--full] [--dry-run] [--force]
  rmap integrate claude-code remove [--global|--project]
  rmap integrate claude-code status [--global|--project] [--json]
  rmap integrate codex install [--global|--project] [--full] [--dry-run] [--force]
  rmap integrate codex remove [--global|--project]
  rmap integrate codex status [--global|--project] [--json]

Developer and diagnostics:
  rmap dev <livegraph-preload|livegraph-refresh|cycle-completeness-audit> ...
  rmap perf [--json]
  rmap metrics <db_path> <repo_uid> [--kind <k>] [--limit <n>] [--sort <value|target>]

Deprecated and disabled:
  rmap daemon                            deprecated — use rmapd
  rmap dead [--json]                     disabled
"
    )
}

/// Print the main usage help to stderr.
pub fn print_usage() {
    eprint!("{}", usage_text());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dispatcher source, read at compile time. The test reads it; it never edits it.
    const MAIN_RS: &str = include_str!("../main.rs");

    const MATCH_HEAD: &str = "match args[1].as_str() {";

    /// For each byte of `src`: `true` when the byte is code, `false` when it is inside a
    /// string literal, a character literal, a line comment or a block comment. A bracket
    /// inside a string or a comment therefore never moves the nesting depth.
    fn code_mask(src: &str) -> Vec<bool> {
        let b = src.as_bytes();
        let mut code = vec![true; b.len()];
        let mut i = 0;
        while i < b.len() {
            let start = i;
            match b[i] {
                b'"' => {
                    i += 1;
                    while i < b.len() && b[i] != b'"' {
                        i += if b[i] == b'\\' { 2 } else { 1 };
                    }
                }
                // A character literal (`'x'` or `'\x'`); a lifetime is not skipped.
                b'\'' if i + 1 < b.len() && b[i + 1] == b'\\' => {
                    i += 2;
                    while i < b.len() && b[i] != b'\'' {
                        i += 1;
                    }
                }
                b'\'' if i + 2 < b.len() && b[i + 2] == b'\'' => i += 2,
                b'/' if b.get(i + 1) == Some(&b'/') => {
                    while i + 1 < b.len() && b[i + 1] != b'\n' {
                        i += 1;
                    }
                }
                b'/' if b.get(i + 1) == Some(&b'*') => {
                    i += 2;
                    while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                        i += 1;
                    }
                    i += 1;
                }
                _ => {
                    i += 1;
                    continue;
                }
            }
            let end = (i + 1).min(b.len());
            code[start..end].iter_mut().for_each(|c| *c = false);
            i = end;
        }
        code
    }

    /// Change of nesting depth (braces, parentheses, brackets) contributed by byte `i`.
    fn depth_step(src: &[u8], code: &[bool], i: usize) -> isize {
        match (code[i], src[i]) {
            (true, b'{' | b'(' | b'[') => 1,
            (true, b'}' | b')' | b']') => -1,
            _ => 0,
        }
    }

    fn unrecognised(line: &str) -> String {
        format!("unrecognised command-match arm form: {line}")
    }

    /// The command names of the top-level dispatcher in `main.rs`, or the reason the parse
    /// failed.
    ///
    /// The command match is the `match args[1].as_str() {` whose body holds the arm
    /// `"index" =>` (never located by line number: `main.rs` also holds the help/version
    /// match). Only the match's own arms — the lines that start at depth 1 of its body —
    /// are read; deeper lines (the `daemon` block's inner `Ok`/`Err` arms) are ignored. At
    /// depth 1 exactly two arm forms are accepted, `"<name>" => run_…(` and
    /// `"<name>" => {`, plus one trailing fallthrough (`other =>` or `_ =>`). Each depth-1
    /// line is consumed whole: after `"<name>" => run_<ident>(` the line holds the
    /// arguments, the call's one closing `)` and an optional `,`; after `"<name>" => {`
    /// nothing; and no depth-1 line carries a second `=>`. Any other depth-1 line fails the
    /// parse, so a command written in a new form, or a second arm on one line, cannot be
    /// skipped.
    fn dispatched_command_names(main_rs: &str) -> Result<Vec<String>, String> {
        let bytes = main_rs.as_bytes();
        let code = code_mask(main_rs);

        // The command match: (byte of its opening brace, byte of its closing brace).
        let mut chosen: Option<(usize, usize)> = None;
        for (pos, _) in main_rs.match_indices(MATCH_HEAD) {
            let open = pos + MATCH_HEAD.len() - 1;
            if !code[open] {
                continue;
            }
            let mut depth = 0isize;
            let close = (open..bytes.len())
                .find(|&i| {
                    depth += depth_step(bytes, &code, i);
                    depth == 0
                })
                .ok_or_else(|| format!("unbalanced match body at byte {open}"))?;
            if main_rs[open + 1..close].contains("\"index\" =>") {
                if chosen.is_some() {
                    return Err("two command matches hold the \"index\" arm".to_string());
                }
                chosen = Some((open, close));
            }
        }
        let (open, close) =
            chosen.ok_or("no `match args[1].as_str() {` holds the \"index\" arm")?;

        let mut names = Vec::new();
        let mut fallthrough_seen = false;
        let mut depth = 0isize; // depth relative to the match body
        let mut line_start = open + 1;
        while line_start < close {
            let line_end = main_rs[line_start..close]
                .find('\n')
                .map_or(close, |n| line_start + n + 1);
            let start_depth = depth;
            for i in line_start..line_end {
                depth += depth_step(bytes, &code, i);
            }
            let line_at = line_start;
            let line = main_rs[line_start..line_end].trim();
            line_start = line_end;
            if start_depth != 0 || line.is_empty() || line.starts_with("//") {
                continue;
            }
            // Every `=>` outside strings and comments starts an arm; a depth-1 line holds one.
            let arrows = (line_at..line_end.saturating_sub(1))
                .filter(|&i| code[i] && code[i + 1] && bytes[i] == b'=' && bytes[i + 1] == b'>')
                .count();
            if arrows > 1 {
                return Err(format!("two arms on one line in the command match: {line}"));
            }
            if fallthrough_seen {
                return Err(format!(
                    "an arm follows the fallthrough arm in the command match: {line}"
                ));
            }
            if line.starts_with("other =>") || line.starts_with("_ =>") {
                fallthrough_seen = true;
                continue;
            }
            let rest = line.strip_prefix('"').ok_or_else(|| unrecognised(line))?;
            let (name, after) = rest.split_once('"').ok_or_else(|| unrecognised(line))?;
            let target = after
                .trim_start()
                .strip_prefix("=>")
                .ok_or_else(|| unrecognised(line))?
                .trim();
            if name.is_empty() {
                return Err(unrecognised(line));
            }
            let run_ident_len = target.strip_prefix("run_").and_then(|f| {
                let ident_len = f
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .unwrap_or(f.len());
                (ident_len > 0 && f[ident_len..].starts_with('(')).then_some(ident_len)
            });
            match run_ident_len {
                Some(ident_len) => {
                    // `target` is a slice of `main_rs`; its byte offset locates the call's `(`.
                    let target_at = target.as_ptr() as usize - main_rs.as_ptr() as usize;
                    let paren = target_at + "run_".len() + ident_len;
                    let mut call_depth = 0isize;
                    let call_close = (paren..line_end)
                        .find(|&i| {
                            call_depth += depth_step(bytes, &code, i);
                            call_depth == 0
                        })
                        .ok_or_else(|| {
                            format!("the call does not close on its arm line: {line}")
                        })?;
                    let tail = main_rs[call_close + 1..line_end].trim();
                    if !(tail.is_empty() || tail == ",") {
                        return Err(format!("text follows the call on its arm line: {line}"));
                    }
                }
                None if target == "{" => {}
                None => return Err(unrecognised(line)),
            }
            names.push(name.to_string());
        }
        if !fallthrough_seen {
            return Err("the command match has no trailing fallthrough arm".to_string());
        }
        Ok(names)
    }

    #[test]
    fn help_lists_every_dispatched_command() {
        let names = dispatched_command_names(MAIN_RS).unwrap_or_else(|reason| {
            panic!("the command match of main.rs did not parse: {reason}")
        });
        assert!(!names.is_empty(), "the command match parse found no arms");
        let help = usage_text();
        let missing: Vec<&String> = names
            .iter()
            .filter(|name| {
                let head = format!("rmap {name}");
                !help.lines().any(|l| {
                    l.trim_start()
                        .strip_prefix(head.as_str())
                        .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
                })
            })
            .collect();
        assert!(
            missing.is_empty(),
            "dispatched commands absent from `rmap --help`: {missing:?}"
        );
    }

    /// Fail-closed counterexample: a depth-1 line holding two arms
    /// (`"a" => run_a(), "b" => run_b(),`) fails the parse instead of recording only `a`.
    #[test]
    fn help_parser_rejects_two_arms_on_one_line() {
        let one_arm_per_line = "fn main() {\n    match args[1].as_str() {\n        \"index\" => run_index(&args[2..]),\n        \"a\" => run_a(),\n        other => unknown(other),\n    }\n}\n";
        assert_eq!(
            dispatched_command_names(one_arm_per_line),
            Ok(vec!["index".to_string(), "a".to_string()]),
            "the fixture with one arm per line must parse"
        );
        let two_arms_on_one_line =
            one_arm_per_line.replace("\"a\" => run_a(),", "\"a\" => run_a(), \"b\" => run_b(),");
        let reason = dispatched_command_names(&two_arms_on_one_line)
            .expect_err("a line holding two arms must fail the parse");
        assert!(
            reason.contains("two arms on one line"),
            "the parse failed for another reason: {reason}"
        );
    }

    #[test]
    fn help_contains_each_usage_constant() {
        let help = usage_text();
        for constant in [
            COVERAGE_USAGE,
            DECLARE_BOUNDARY_USAGE,
            DECLARE_REQUIREMENT_USAGE,
            DECLARE_QUALITY_POLICY_USAGE,
        ] {
            assert!(
                help.contains(constant),
                "`rmap --help` does not contain the usage text:\n{constant}"
            );
        }
    }
}
