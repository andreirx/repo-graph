//! Modules command family.
//!
//! Discovered-module analysis and boundary management:
//! - `list` — module catalog with rollup statistics
//! - `show` — single module detail view
//! - `files` — files owned by a module
//! - `deps` — module dependency edges
//! - `violations` — boundary violation report
//! - `boundary` — create boundary declarations
//! - `unowned` — ownership gap analysis (Phase 3.1B)
//!
//! Also exports unified `violations` command that combines
//! declared boundaries (legacy) with discovered-module boundaries.
//!
//! # Boundary rules
//!
//! This module owns modules command-family behavior:
//! - command handlers
//! - family-local DTOs
//! - family-local argument parsing
//! - family-local helpers
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - module graph loading (lives in `repo-graph-module-queries`)
//! - classification algorithms (belong in `repo-graph-classification`)

mod boundary;
mod deps;
mod files;
mod list;
mod shared;
mod show;
mod unowned;
mod violations;

use std::process::ExitCode;

use boundary::run_modules_boundary;
use deps::run_modules_deps;
use files::run_modules_files;
use list::run_modules_list;
use show::run_modules_show;
use unowned::run_modules_unowned;
use violations::run_modules_violations;

// Re-export unified violations command
pub use violations::run_violations;

/// Dispatcher for `rmap modules <subcommand>`.
///
/// REG-1 Migration Status:
/// - list, show, files, deps, violations, unowned: REG-1 (cwd-based)
/// - boundary: legacy write command (defer to Batch 4)
pub fn run_modules(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("usage:");
        eprintln!("  rmap modules list [--include-tests] [--include-inferred]  (REG-1: from cwd)");
        eprintln!("  rmap modules show <module>                     (REG-1: from cwd)");
        eprintln!("  rmap modules files <module>                    (REG-1: from cwd)");
        eprintln!("  rmap modules deps [module] [--outbound|--inbound] [--include-tests] [--include-inferred]  (REG-1: from cwd)");
        eprintln!("  rmap modules violations                        (REG-1: from cwd)");
        eprintln!("  rmap modules unowned                           (REG-1: from cwd)");
        eprintln!("  rmap modules boundary <db_path> <repo_uid> <source> --forbids <target> [--reason <text>]");
        return ExitCode::from(crate::daemon_command::EXIT_USAGE_ERROR);
    }

    match args[0].as_str() {
        "list" => run_modules_list(&args[1..]),
        "show" => run_modules_show(&args[1..]),
        "files" => run_modules_files(&args[1..]),
        "deps" => run_modules_deps(&args[1..]),
        "violations" => run_modules_violations(&args[1..]),
        "boundary" => run_modules_boundary(&args[1..]),
        "unowned" => run_modules_unowned(&args[1..]),
        other => {
            eprintln!("unknown modules subcommand: {}", other);
            eprintln!("usage:");
            eprintln!(
                "  rmap modules list [--include-tests] [--include-inferred]  (REG-1: from cwd)"
            );
            eprintln!("  rmap modules show <module>                     (REG-1: from cwd)");
            eprintln!("  rmap modules files <module>                    (REG-1: from cwd)");
            eprintln!("  rmap modules deps [module] [--outbound|--inbound] [--include-tests] [--include-inferred]  (REG-1: from cwd)");
            eprintln!("  rmap modules violations                        (REG-1: from cwd)");
            eprintln!("  rmap modules unowned                           (REG-1: from cwd)");
            eprintln!("  rmap modules boundary <db_path> <repo_uid> <source> --forbids <target> [--reason <text>]");
            ExitCode::from(crate::daemon_command::EXIT_USAGE_ERROR)
        }
    }
}

#[cfg(test)]
mod partition_flag_tests {
    //! TEST-EDGE-SCOPE-1B (RG-REQ-004-L12, RG-REQ-012-L03): the partition flags of the module
    //! surfaces, and every next-action command the partition renderers print parsing as printed.
    use super::deps::parse_deps_args;
    use super::list::parse_list_args;
    use crate::commands::graph::{
        parse_cycles_args, parse_imports_args, parse_path_args, PartitionFlags,
    };
    use crate::presentation::import_partition as ip;

    fn argv(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn modules_list_and_deps_accept_include_tests_and_include_inferred() {
        let both = PartitionFlags {
            include_tests: true,
            include_inferred: true,
        };
        let (json, full, flags) =
            parse_list_args(&argv("--include-tests --full --include-inferred")).unwrap();
        assert!(!json && full);
        assert_eq!(flags, both);
        let (_, _, none) = parse_list_args(&argv("--json")).unwrap();
        assert_eq!(none, PartitionFlags::default());
        assert!(parse_list_args(&argv("--include-everything")).is_err());

        let (module, direction, json, flags) =
            parse_deps_args(&argv("table --include-tests --outbound --json")).unwrap();
        assert_eq!(module.as_deref(), Some("table"));
        assert_eq!(direction, "outbound");
        assert!(json);
        assert!(flags.include_tests && !flags.include_inferred);
        let (module, _, _, flags) = parse_deps_args(&argv("--include-inferred db")).unwrap();
        assert_eq!(module.as_deref(), Some("db"));
        assert!(flags.include_inferred && !flags.include_tests);
        let mut params = serde_json::json!({"repo": "/r", "direction": "all"});
        both.add_to_params(&mut params);
        assert_eq!(params["include_tests"], true);
        assert_eq!(params["include_inferred"], true);
    }

    /// Every `rmap …` command in `text`, split into argv as a POSIX shell would for the quoting
    /// `shell_quote` emits (single quotes, and `\'` between them). A command ends at ` — `, `)`, `;` or end of line.
    fn printed_commands(text: &str) -> Vec<Vec<String>> {
        let mut out = Vec::new();
        for line in text.lines() {
            let mut rest = line;
            while let Some(i) = rest.find("rmap ") {
                let tail = &rest[i + 5..];
                let mut args: Vec<String> = Vec::new();
                let mut cur = String::new();
                let mut in_quote = false;
                let mut consumed = tail.len();
                let mut has_token = false;
                let mut escaped = false;
                for (k, c) in tail.char_indices() {
                    if escaped {
                        cur.push(c);
                        escaped = false;
                        continue;
                    }
                    if in_quote {
                        if c == '\'' {
                            in_quote = false;
                        } else {
                            cur.push(c);
                        }
                        continue;
                    }
                    match c {
                        '\\' => {
                            escaped = true;
                            has_token = true;
                        }
                        '\'' => {
                            in_quote = true;
                            has_token = true;
                        }
                        ' ' => {
                            if has_token {
                                args.push(std::mem::take(&mut cur));
                                has_token = false;
                            }
                            if tail[k..].starts_with(" — ") || tail[k..].starts_with(" shows ") {
                                consumed = k;
                                break;
                            }
                        }
                        ')' | ';' | '`' => {
                            consumed = k;
                            break;
                        }
                        _ => {
                            cur.push(c);
                            has_token = true;
                        }
                    }
                }
                if has_token {
                    args.push(cur);
                }
                out.push(args);
                rest = &tail[consumed..];
            }
        }
        out
    }

    /// Parse one printed command with rgr's own parser for it; `Err` names the failure.
    fn parse_printed(argv: &[String]) -> Result<(), String> {
        let (cmd, rest) = argv.split_first().ok_or("empty command")?;
        match cmd.as_str() {
            "cycles" => parse_cycles_args(rest).map(|_| ()),
            "imports" => parse_imports_args(rest).map(|_| ()),
            "path" => parse_path_args(rest).map(|_| ()),
            "modules" => {
                let (sub, rest) = rest.split_first().ok_or("modules without subcommand")?;
                match sub.as_str() {
                    "list" => parse_list_args(rest).map(|_| ()),
                    "deps" => parse_deps_args(rest).map(|_| ()),
                    other => Err(format!("unexpected modules subcommand {other}")),
                }
            }
            other => Err(format!("unexpected command {other}")),
        }
    }

    #[test]
    fn printed_include_flag_commands_parse_as_printed() {
        let g = |imports: u64, edges: u64| ip::Group { imports, edges };
        let all = ip::Remainder {
            tests: g(89, 3),
            inferred: g(2, 1),
            tests_and_inferred: g(1, 1),
        };
        let cycle = |flags: &[&str]| ip::ExcludedCycle {
            members: vec!["a".into(), "b".into()],
            flags: flags.iter().map(|f| f.to_string()).collect(),
            contains_shown: 0,
        };
        let mut rendered: Vec<String> = Vec::new();
        // orient's module-edges clause and explain's / map's Imports clause (command form).
        rendered.extend(ip::remainder_command_clause(&all, "modules list"));
        rendered.extend(ip::imports_command_clause(
            2,
            "tests/kafkatest/services/streams.py",
        ));
        rendered.extend(ip::imports_command_clause(1, "src/my file.py"));
        // `imports`' own flag-form lines never offer a flag `imports` lacks.
        for line in ip::remainder_lines(&ip::per_file_remainder(&all), ip::EdgeNoun::None) {
            assert!(!line.contains("--include-tests"), "{line}");
            assert!(line.ends_with("— --include-inferred"), "{line}");
        }
        // orient's cycle clause and explain's Import-cycles bullet, each flag set.
        for flags in [
            &["include_tests"][..],
            &["include_inferred"][..],
            &["include_tests", "include_inferred"][..],
        ] {
            rendered.extend(ip::excluded_cycle_clause(&[cycle(flags)]));
        }
        // cycles' excluded-cycle elision line.
        rendered.push(ip::excluded_cycle_lines(&vec![cycle(&["include_tests"]); 6]).join("\n"));
        // Governance next actions, including a path and a module needing quotes.
        rendered.push(ip::not_judged_boundary_line(
            2,
            "src/core -> src/adapters",
            &["src/core/a.py".to_string()],
        ));
        rendered.push(ip::not_judged_boundary_line(
            1,
            "the boundaries of src",
            &["src/it's here.py".to_string()],
        ));
        rendered.push(ip::not_judged_module_line(1, "db"));
        rendered.push(ip::not_judged_module_line(1, "packages/my pkg"));
        // path's no-route next action with the user's two queries.
        rendered.push(ip::path_no_route_line(1, "A.f", "B.g"));
        rendered.push(ip::path_no_route_line(3, "pkg mod::f", "B.g"));

        let mut seen = 0;
        for text in &rendered {
            let commands = printed_commands(text);
            assert!(!commands.is_empty(), "no command in `{text}`");
            for argv in commands {
                seen += 1;
                assert!(
                    !argv.iter().any(|a| a.starts_with('<')),
                    "placeholder in `{text}`: {argv:?}"
                );
                parse_printed(&argv)
                    .unwrap_or_else(|e| panic!("`{text}` prints {argv:?}, which fails: {e}"));
            }
        }
        // Every printed command names a partition flag the parser accepted.
        assert!(seen >= 15, "{seen} commands checked");
        // The quoted forms come back as the original argument.
        let quoted = printed_commands(&ip::path_no_route_line(3, "pkg mod::f", "B.g"));
        assert_eq!(
            quoted[0],
            vec!["path", "pkg mod::f", "B.g", "--include-inferred"]
        );
        let quoted = printed_commands(&ip::not_judged_boundary_line(
            1,
            "b",
            &["src/it's here.py".to_string()],
        ));
        assert_eq!(
            quoted[0],
            vec!["imports", "src/it's here.py", "--include-inferred"]
        );
    }
}
