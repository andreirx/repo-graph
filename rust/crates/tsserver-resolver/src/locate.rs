//! tsserver binary location — the ONE locator shared by the resolver and the enrich-pass probe.
//!
//! TypeScript packages in a polyglot/monorepo layout keep their `typescript` (and therefore
//! `node_modules/.bin/tsserver`) beside the package that uses it — `frontend/web/node_modules/...`,
//! not the repo root. The historical lookup checked only the repo-root `node_modules` or `$PATH`
//! (`find_tsserver`, `enrich_pass::resolver_toolchain_available`), so every nested-package TS repo
//! lost the TS semantic witness even though tsserver was present (TSSERVER-LOCATE-1).
//!
//! [`locate_tsserver`] resolves tsserver FOR A GIVEN PROJECT CONTEXT (a tsconfig/jsconfig/package.json
//! root the resolver already discovers, `project.rs`) by walking UP from that context directory to the
//! repo root — the NEAREST `node_modules/.bin/tsserver` wins — then the config-specified path, then
//! `$PATH`. Both callers use this one function so the enrich-pass "skipped" verdict and the resolver's
//! session start can never disagree about where tsserver is.

use std::path::Path;
use std::process::{Command, Stdio};

/// Resolve the tsserver executable for one TS project context.
///
/// Search order (TSSERVER-LOCATE-1 §2.1):
/// 1. Walk UP from `context_dir` to `repo_root`; the nearest `node_modules/.bin/tsserver` wins.
/// 2. The config-specified path (returned as-is — an absolute path or a bare command name).
/// 3. `tsserver` on `$PATH`.
///
/// Returns the resolved command (an absolute path for a `node_modules` hit, or the bare `"tsserver"`
/// for a `$PATH` hit) or `None` when no context up-chain, config, or `$PATH` yields one.
///
/// The config path precedes `$PATH` but follows the local `node_modules` walk: a package's own pinned
/// tsserver is the correct type authority for that package, so it wins over a global override. (In the
/// shipped daemon `config_path` is always `None` — no caller sets `TsServerConfig::tsserver_path` — so
/// this ordering is byte-neutral there; it only orders a hypothetical explicit override.)
pub fn locate_tsserver(
    context_dir: &Path,
    repo_root: &Path,
    config_path: Option<&str>,
) -> Option<String> {
    locate_tsserver_with(context_dir, repo_root, config_path, tsserver_on_path)
}

/// [`locate_tsserver`] with the `$PATH` probe injected, so the ordering and the `$PATH` fallback are
/// unit-testable host-independently (the real [`tsserver_on_path`] shells out to `which`).
///
/// Abstraction ledger — **What:** a one-argument seam over the `$PATH` probe. **Concrete current
/// users:** `locate_tsserver` (production, passes the real probe) + the `locate.rs` unit tests (§4:
/// PATH-only and none-found cases, which must not depend on whether the test host has tsserver on
/// `$PATH`). **Axis of variation:** test host `$PATH` presence. **Rejected simpler alternative:** call
/// `tsserver_on_path` directly inside `locate_tsserver` — then the §4 "PATH-only" / "none" cases are
/// not hermetic (a host with a global tsserver flips the "none" case to Some).
fn locate_tsserver_with(
    context_dir: &Path,
    repo_root: &Path,
    config_path: Option<&str>,
    path_probe: impl Fn() -> bool,
) -> Option<String> {
    // 1. Nearest node_modules/.bin/tsserver walking context_dir -> repo_root.
    if let Some(local) = nearest_node_modules_tsserver(context_dir, repo_root) {
        return Some(local);
    }
    // 2. Config-specified path (returned verbatim, matching the historical find_tsserver contract:
    //    an existing path OR a bare command name tsserver's spawn will resolve).
    if let Some(path) = config_path {
        return Some(path.to_string());
    }
    // 3. $PATH.
    if path_probe() {
        return Some("tsserver".to_string());
    }
    None
}

/// Walk UP from `context_dir` to `repo_root` (inclusive), returning the first (nearest)
/// `node_modules/.bin/tsserver` that is a real file. Bounded to the repo tree: never ascends ABOVE
/// `repo_root` (an unrelated `node_modules` outside the indexed repo must not be adopted).
fn nearest_node_modules_tsserver(context_dir: &Path, repo_root: &Path) -> Option<String> {
    let mut current = Some(context_dir);
    while let Some(dir) = current {
        let candidate = dir.join("node_modules/.bin/tsserver");
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().to_string());
        }
        if dir == repo_root {
            break;
        }
        // Ascend, but stay within the repo tree (the parent must still contain repo_root).
        current = dir.parent().filter(|p| p.starts_with(repo_root));
    }
    None
}

/// Is `tsserver` resolvable on `$PATH`? Shells out to `which` (macOS/Linux — the VISION platform
/// priority). Moved here from `client.rs` so the whole tsserver-location concern is one module.
fn tsserver_on_path() -> bool {
    Command::new("which")
        .arg("tsserver")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Create a `<dir>/node_modules/.bin/tsserver` real file so `is_file()` sees it.
    fn install_tsserver(dir: &Path) {
        let bin = dir.join("node_modules/.bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("tsserver"), "#!/bin/sh\n").unwrap();
    }

    #[test]
    fn nested_context_nearest_node_modules_wins_over_a_farther_one() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();
        // tsserver at BOTH the repo root and the nested package; the nested (nearer) one must win.
        install_tsserver(repo_root);
        let ctx = repo_root.join("frontend/web");
        fs::create_dir_all(&ctx).unwrap();
        install_tsserver(&ctx);

        let found = locate_tsserver_with(&ctx, repo_root, None, || false).unwrap();
        assert_eq!(
            found,
            ctx.join("node_modules/.bin/tsserver")
                .to_string_lossy()
                .to_string(),
            "the nearest node_modules must win over the farther repo-root one"
        );
    }

    #[test]
    fn nested_context_walks_up_to_an_ancestor_node_modules() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();
        // tsserver only at an intermediate ancestor (frontend/), not at the context (frontend/web/).
        let mid = repo_root.join("frontend");
        let ctx = mid.join("web");
        fs::create_dir_all(&ctx).unwrap();
        install_tsserver(&mid);

        let found = locate_tsserver_with(&ctx, repo_root, None, || false).unwrap();
        assert_eq!(
            found,
            mid.join("node_modules/.bin/tsserver")
                .to_string_lossy()
                .to_string()
        );
    }

    #[test]
    fn root_node_modules_is_found_for_a_root_context() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();
        install_tsserver(repo_root);

        let found = locate_tsserver_with(repo_root, repo_root, None, || false).unwrap();
        assert_eq!(
            found,
            repo_root
                .join("node_modules/.bin/tsserver")
                .to_string_lossy()
                .to_string()
        );
    }

    #[test]
    fn path_only_falls_back_to_bare_command() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();
        let ctx = repo_root.join("pkg");
        fs::create_dir_all(&ctx).unwrap();
        // No node_modules anywhere; $PATH has tsserver (injected true).
        let found = locate_tsserver_with(&ctx, repo_root, None, || true);
        assert_eq!(found, Some("tsserver".to_string()));
    }

    #[test]
    fn none_when_no_node_modules_no_config_no_path() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();
        let ctx = repo_root.join("pkg");
        fs::create_dir_all(&ctx).unwrap();
        // No node_modules, no config, tsserver NOT on $PATH (injected false).
        let found = locate_tsserver_with(&ctx, repo_root, None, || false);
        assert_eq!(found, None, "no context, config, or $PATH tsserver → None");
    }

    #[test]
    fn config_path_precedes_path_but_follows_local_node_modules() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();
        let ctx = repo_root.join("pkg");
        fs::create_dir_all(&ctx).unwrap();

        // No node_modules → config path is used (ahead of $PATH, even when $PATH would resolve).
        let via_config = locate_tsserver_with(&ctx, repo_root, Some("/opt/ts/tsserver"), || true);
        assert_eq!(via_config, Some("/opt/ts/tsserver".to_string()));

        // With a local node_modules, the pinned local tsserver wins over the config override.
        install_tsserver(&ctx);
        let via_local = locate_tsserver_with(&ctx, repo_root, Some("/opt/ts/tsserver"), || true);
        assert_eq!(
            via_local,
            Some(
                ctx.join("node_modules/.bin/tsserver")
                    .to_string_lossy()
                    .to_string()
            )
        );
    }

    #[test]
    fn does_not_ascend_above_repo_root() {
        let tmp = TempDir::new().unwrap();
        // tsserver ABOVE the repo root must be ignored (bounded to the indexed tree).
        install_tsserver(tmp.path());
        let repo_root = tmp.path().join("repo");
        let ctx = repo_root.join("pkg");
        fs::create_dir_all(&ctx).unwrap();

        let found = locate_tsserver_with(&ctx, &repo_root, None, || false);
        assert_eq!(
            found, None,
            "a node_modules above repo_root must not be adopted"
        );
    }
}
