//! `rmap map` — deterministic MAP.md generation from the index (MAP-FROM-INDEX-1).
//!
//! Renders per-directory + per-file `MAP.md` files from the current READY
//! snapshot's extracted facts, with NO model call anywhere (VISION commitment
//! #1). This handler is thin: it requests the flat facts from the daemon (rule
//! #8 — the daemon owns snapshot state), hands them to the pure renderer
//! (`presentation::map`), and writes/prints the results. All the fact assembly,
//! ordering, coverage honesty, and markdown live in the renderer, which is
//! unit-tested without a daemon.
//!
//! # Usage
//!
//! ```text
//! rmap map [path]            # write MAP.md files for <path> (repo-root-relative;
//!                            # default: whole repo). Files land at their
//!                            # repo-root-relative locations regardless of cwd.
//! rmap map [path] --dry-run  # print the rendered files to stdout, write nothing
//! rmap map [path] --dry-run --full        # dry-run with the content cap removed
//! rmap map [path] --dry-run --limit <n>   # dry-run capped at <n> content lines
//! rmap map [path] --json     # machine envelope: the rendered file set
//! ```
//!
//! `--dry-run` writes nothing and emits (to stdout) a count header, the complete
//! bytes-per-file manifest, and the rendered file CONTENT capped at
//! `DRY_RUN_DEFAULT_LINE_CAP` lines (ECONOMY-2 §2.4); the extraction-coverage summary
//! goes to stderr. `--full` removes the content cap — the isolated, tree-safe path used
//! for live determinism checks (render twice, diff empty); `--limit <n>` raises it.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::daemon_command::{execute_repo_request, print_daemon_error};
use crate::presentation::map::{
    render_maps, render_summary, MapFacts, RenderedMapFile, DRY_RUN_DEFAULT_LINE_CAP,
};

struct MapOpts {
    /// Repo-root-relative directory to render; empty = whole repo.
    path: String,
    dry_run: bool,
    json: bool,
    /// ECONOMY-2 (§2.4): the `--dry-run` CONTENT-line cap. `Some(n)` caps at `n` lines
    /// (the default `DRY_RUN_DEFAULT_LINE_CAP`, or `--limit <n>`); `None` = `--full`
    /// (uncapped — the determinism-diff path). Ignored unless `dry_run` (and `--json`,
    /// which emits the full machine envelope, is unaffected).
    dry_run_cap: Option<usize>,
}

/// Entry point for `rmap map`.
pub fn run_map(args: &[String]) -> ExitCode {
    let opts = match parse_map_args(args) {
        Ok(o) => o,
        Err(msg) => {
            eprintln!("error: {}", msg);
            eprintln!("usage: rmap map [path] [--dry-run [--full | --limit <n>]] [--json]");
            return ExitCode::from(1);
        }
    };

    let params = serde_json::json!({ "path": opts.path });
    let result = match execute_repo_request("map", Some(params)) {
        Ok(v) => v,
        Err(e) => {
            print_daemon_error(&e, "map");
            return ExitCode::from(e.exit_code());
        }
    };

    let facts: MapFacts = match serde_json::from_value(result) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: failed to parse map facts from daemon: {}", e);
            return ExitCode::from(2);
        }
    };

    let rendered = render_maps(&facts);

    if opts.json {
        return output_json(&facts, &rendered);
    }

    if opts.dry_run {
        // ECONOMY-2 (§2.4): count header + complete bytes-per-file manifest + the rendered
        // CONTENT capped at `dry_run_cap` (deterministic; safe to diff across two runs at
        // `--full`). The extraction-coverage summary still goes to stderr so stdout is the
        // pure dry-run artifact.
        print!(
            "{}",
            crate::presentation::map::render_dry_run(&facts, &rendered, opts.dry_run_cap)
        );
        eprint!("{}", render_summary(&facts, &rendered));
        return ExitCode::SUCCESS;
    }

    match write_maps(&facts.repo_root, &rendered) {
        Ok(()) => {
            print!("{}", render_summary(&facts, &rendered));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: failed to write MAP.md files: {}", e);
            ExitCode::from(2)
        }
    }
}

/// On-disk output path for one rendered file. The daemon supplies an absolute
/// `repo_root`, so each map lands at its repo-root-relative location no matter
/// where the caller invoked `rmap map` from — a nested cwd must NOT recreate
/// whole-repo paths beneath it. When `repo_root` is empty (repo record absent),
/// fall back to cwd-relative (the pre-fix behavior, now only a degenerate case).
fn output_path(repo_root: &str, rel_path: &str) -> PathBuf {
    if repo_root.is_empty() {
        PathBuf::from(rel_path)
    } else {
        Path::new(repo_root).join(rel_path)
    }
}

/// Write each rendered file to disk under the resolved repo root. Parent
/// directories are created as needed. Overwrites unconditionally: a MAP is a
/// regenerable artifact, and the marker warns humans not to hand-edit it
/// (mirrors rgistr's overwrite semantics).
fn write_maps(repo_root: &str, rendered: &[RenderedMapFile]) -> std::io::Result<()> {
    for r in rendered {
        let path = output_path(repo_root, &r.rel_path);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(&path, &r.contents)?;
    }
    Ok(())
}

/// Machine envelope: the rendered file set (path + contents) plus provenance.
fn output_json(facts: &MapFacts, rendered: &[RenderedMapFile]) -> ExitCode {
    let files: Vec<serde_json::Value> = rendered
        .iter()
        .map(|r| serde_json::json!({ "rel_path": r.rel_path, "contents": r.contents }))
        .collect();
    let unmapped = facts
        .files
        .iter()
        .filter(|f| f.parse_status != "parsed")
        .count();
    let env = serde_json::json!({
        "command": "map",
        "snapshot": facts.snapshot,
        "path": facts.path,
        "unmapped_count": unmapped,
        "files": files,
    });
    match serde_json::to_string_pretty(&env) {
        Ok(json) => {
            println!("{}", json);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: failed to serialize result: {}", e);
            ExitCode::from(2)
        }
    }
}

fn parse_map_args(args: &[String]) -> Result<MapOpts, String> {
    let mut path = String::new();
    let mut dry_run = false;
    let mut json = false;
    let mut full = false;
    // `--limit <n>`; `None` until the flag is seen. ECONOMY-2 (§2.4).
    let mut limit: Option<usize> = None;
    let mut have_path = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dry-run" => {
                dry_run = true;
                i += 1;
            }
            "--json" => {
                json = true;
                i += 1;
            }
            "--full" => {
                full = true;
                i += 1;
            }
            "--limit" => {
                let raw = args
                    .get(i + 1)
                    .ok_or_else(|| "--limit requires a value (a line count)".to_string())?;
                let n: usize = raw
                    .parse()
                    .map_err(|_| format!("--limit expects a non-negative integer, got: {raw}"))?;
                limit = Some(n);
                i += 2;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown flag: {}", other));
            }
            _ => {
                if have_path {
                    return Err(format!("unexpected argument: {}", args[i]));
                }
                path = args[i].clone();
                have_path = true;
                i += 1;
            }
        }
    }
    // ECONOMY-2 (§2.4): the two cap controls are mutually exclusive — `--full` means
    // uncapped, `--limit` sets a finite cap; asking for both is contradictory.
    if full && limit.is_some() {
        return Err("--full and --limit are mutually exclusive".to_string());
    }
    // Effective CONTENT cap: `--full` → None (uncapped); `--limit n` → Some(n); neither →
    // the default cap. Only meaningful with `--dry-run` (the sole content-dumping path).
    let dry_run_cap = if full {
        None
    } else {
        Some(limit.unwrap_or(DRY_RUN_DEFAULT_LINE_CAP))
    };
    Ok(MapOpts {
        path,
        dry_run,
        json,
        dry_run_cap,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_defaults_to_whole_repo() {
        let o = parse_map_args(&[]).unwrap();
        assert_eq!(o.path, "");
        assert!(!o.dry_run && !o.json);
    }

    #[test]
    fn parse_positional_path_and_flags() {
        let args: Vec<String> = ["rust/crates/rgr", "--dry-run", "--json"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let o = parse_map_args(&args).unwrap();
        assert_eq!(o.path, "rust/crates/rgr");
        assert!(o.dry_run);
        assert!(o.json);
    }

    #[test]
    fn parse_rejects_unknown_flag_and_second_positional() {
        assert!(parse_map_args(&["--bogus".to_string()]).is_err());
        let two: Vec<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        assert!(parse_map_args(&two).is_err());
    }

    // ── ECONOMY-2 §2.4: --dry-run content cap controls ──────────────────────────

    #[test]
    fn parse_dry_run_defaults_to_the_default_cap() {
        let o = parse_map_args(&["--dry-run".to_string()]).unwrap();
        assert!(o.dry_run);
        assert_eq!(o.dry_run_cap, Some(DRY_RUN_DEFAULT_LINE_CAP));
    }

    #[test]
    fn parse_full_uncaps_the_dry_run_content() {
        let args: Vec<String> = ["--dry-run", "--full"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let o = parse_map_args(&args).unwrap();
        assert_eq!(o.dry_run_cap, None);
    }

    #[test]
    fn parse_limit_sets_a_finite_cap() {
        let args: Vec<String> = ["--dry-run", "--limit", "50"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let o = parse_map_args(&args).unwrap();
        assert_eq!(o.dry_run_cap, Some(50));
    }

    #[test]
    fn parse_full_and_limit_together_is_rejected() {
        let args: Vec<String> = ["--dry-run", "--full", "--limit", "50"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(parse_map_args(&args).is_err());
    }

    #[test]
    fn parse_limit_requires_a_numeric_value() {
        assert!(parse_map_args(&["--limit".to_string()]).is_err());
        let args: Vec<String> = ["--limit", "abc"].iter().map(|s| s.to_string()).collect();
        assert!(parse_map_args(&args).is_err());
    }

    #[test]
    fn output_path_is_anchored_at_repo_root_not_cwd() {
        // REVIEWER-4: with an absolute repo_root, a whole-repo rel_path resolves
        // under the repo root — never beneath the (possibly nested) cwd. This is
        // cwd-independent (a pure join), so it holds from any working directory.
        assert_eq!(
            output_path("/abs/repo", "rust/crates/rgr/src/MAP.md"),
            PathBuf::from("/abs/repo/rust/crates/rgr/src/MAP.md")
        );
        assert_eq!(
            output_path("/abs/repo", "MAP.md"),
            PathBuf::from("/abs/repo/MAP.md")
        );
        // Degenerate fallback (repo record absent): cwd-relative.
        assert_eq!(output_path("", "src/MAP.md"), PathBuf::from("src/MAP.md"));
    }
}
