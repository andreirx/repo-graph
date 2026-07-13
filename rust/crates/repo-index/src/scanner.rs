//! Filesystem scanner — walks a repo directory, applies ignore + extension
//! filters, reads source files, and computes content hashes.
//!
//! Traversal + ignore handling use [`ignore::WalkBuilder`] (the crate ripgrep is
//! built on) for full, git-faithful ignore semantics; see [`scan_repo`] for the
//! exact configuration and [`is_scanner_pruned_dir`] for the hardcoded
//! build/vendor prune policy.
//!
//!   - Full git ignore semantics: anchoring, negation, nested `.gitignore`,
//!     `.git/info/exclude` (was: root `.gitignore` only, no nesting, no
//!     anchoring for the always-excluded set — SCANNER-GITIGNORE-1)
//!   - Hardcoded `ALWAYS_EXCLUDED` directory pruning applied ONLY at
//!     repository-root depth (`.git` at any depth); NESTED same-named dirs are
//!     governed by the repo's own git ignore semantics, so a git-tracked nested
//!     source dir such as `rust/crates/coverage/` is never silently dropped
//!     (SCANNER-PRUNE-AUTHORITY, ratified 2026-07-13)
//!   - `ALL_SOURCE_EXTENSIONS` file filtering (unchanged)
//!   - Symlinked source files are followed to their target and read (unchanged
//!     from the prior loader); unreadable files tracked as `ReadFailed`, never
//!     silently dropped

use std::path::Path;

use repo_graph_indexer::routing;
use sha2::{Digest, Sha256};

/// A source file discovered on disk.
#[derive(Debug, Clone)]
pub enum ScannedFile {
    /// Successfully read file.
    Ok(ScannedFileOk),
    /// File discovered but could not be read (permissions, binary, etc.).
    ReadFailed { rel_path: String },
}

impl ScannedFile {
    /// Repo-relative path, regardless of read outcome. Used to sort scan
    /// output deterministically by path.
    fn rel_path(&self) -> &str {
        match self {
            ScannedFile::Ok(ok) => &ok.rel_path,
            ScannedFile::ReadFailed { rel_path } => rel_path,
        }
    }
}

/// A successfully scanned source file.
#[derive(Debug, Clone)]
pub struct ScannedFileOk {
    /// Repo-relative path (forward slashes).
    pub rel_path: String,
    /// UTF-8 source text.
    pub content: String,
    /// SHA-256 hex truncated to 16 chars. Matches TS `hashContent`.
    pub content_hash: String,
    /// File size in bytes.
    pub size_bytes: usize,
    /// Line count (TS `source.split("\n").length` convention).
    pub line_count: usize,
    /// Detected language (from routing policy).
    pub language: Option<&'static str>,
    /// Whether this is a test file (from routing policy).
    pub is_test: bool,
}

/// Compute the content hash matching TS `hashContent`:
/// `SHA-256(content_bytes).hex()[0..16]`.
pub fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let digest = hasher.finalize();
    let hex = format!("{:x}", digest);
    hex[..16].to_string()
}

/// Scan a repository directory for source files.
///
/// Traversal and ignore handling are delegated to [`ignore::WalkBuilder`] (the
/// crate ripgrep is built on), configured for full, git-faithful ignore
/// semantics — the thing the prior hand-rolled loader could not do:
///   - **anchoring**: `/coverage/` matches only the repo-root `coverage/`, NOT a
///     nested `rust/crates/coverage/` (the SCANNER-GITIGNORE-1 casualty);
///   - **negation**: `!keep.ts` re-includes (subject to git's rule that a file
///     under a fully-excluded directory cannot be re-included);
///   - **nested `.gitignore`** files at any depth;
///   - **`.git/info/exclude`**.
///
/// Configuration rationale (each flag is load-bearing):
///   - `require_git(false)` — the repo-root `.gitignore` is authoritative even
///     with no `.git` directory present (preserves the prior scanner contract
///     and the TS indexer's `loadGitignore`).
///   - `git_global(false)` + `ignore(false)` — the user's global `~/.gitignore`
///     and non-git `.ignore` files are NOT consulted, so a scan is reproducible
///     across machines (VISION determinism) and reflects exactly what THIS repo's
///     committed ignore files say.
///   - `parents(false)` — only this repo's ignore files, nothing above the root.
///   - `hidden(false)` — dotfiles/dotdirs are scanned (the prior loader did not
///     skip them); `.git/` is still pruned explicitly via
///     [`is_scanner_pruned_dir`] (with `hidden(false)`, `WalkBuilder` would
///     otherwise descend it), and other dot vendor dirs (`.venv/`, `.next/`, …)
///     are pruned when they sit at the repository root.
///
/// On top of git semantics, [`is_scanner_pruned_dir`] prunes well-known
/// build/vendor directories at traversal time — a belt-and-suspenders perf guard
/// so the walk never descends a root `node_modules/`, and `.git/` at any depth.
/// Per SCANNER-PRUNE-AUTHORITY (2026-07-13) this hardcoded list applies ONLY to
/// repository-ROOT-level directories (except `.git`, pruned at any depth); every
/// nested directory is governed by the repo's own git ignore semantics, so a
/// git-tracked nested `coverage/`, `out/`, `venv/`, … is never silently dropped.
///
/// Extension filtering and `ReadFailed` tracking are unchanged: a file that
/// passes the filters but cannot be read as UTF-8 is surfaced as
/// `ScannedFile::ReadFailed`, never silently dropped.
///
/// Returns files sorted by `rel_path` for deterministic ordering.
pub fn scan_repo(repo_path: &Path) -> Result<Vec<ScannedFile>, ScanError> {
    let mut builder = ignore::WalkBuilder::new(repo_path);
    builder
        .hidden(false)
        .parents(false)
        .ignore(false)
        .git_global(false)
        .git_ignore(true)
        .git_exclude(true)
        .require_git(false)
        .follow_links(false)
        .filter_entry(|entry| {
            // Prune hardcoded build/vendor dirs at traversal time (never descend
            // them). Directories only, and never the walk root itself (depth 0) —
            // the repo's own directory name must not exclude the whole repo. The
            // traversal depth is passed through so the policy can be root-only
            // (see `is_scanner_pruned_dir`).
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if !is_dir || entry.depth() == 0 {
                return true;
            }
            !is_scanner_pruned_dir(&entry.file_name().to_string_lossy(), entry.depth())
        });

    let mut files = Vec::new();

    for result in builder.build() {
        let entry = match result {
            Ok(entry) => entry,
            Err(err) => {
                // A genuine IO failure (missing root, unreadable directory)
                // aborts the scan — matching the prior walkdir loader, which
                // propagated walk errors. Non-IO issues (e.g. a malformed
                // `.gitignore` line) are absorbed by WalkBuilder and do not reach
                // here; if one ever did, skip it rather than lose the whole index.
                if err.io_error().is_some() {
                    return Err(ScanError {
                        message: format!("walk error: {}", err),
                    });
                }
                continue;
            }
        };

        // Skip directories — they are traversal structure, not content. Regular
        // files AND symlinks flow through: `read_to_string` (below) follows a
        // symlink to its target, so a symlinked SOURCE file is read exactly as
        // the prior walkdir loader read it (that loader skipped only `is_dir()`
        // entries). A symlink to a directory, or a dangling one, fails that read
        // and is tracked as `ReadFailed` — never silently dropped.
        // (SCANNER-GITIGNORE-1: build-0's `is_file()` filter silently omitted
        // symlinked source; this restores parity.)
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }

        let path = entry.path();
        let rel_path = match path.strip_prefix(repo_path) {
            Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        if rel_path.is_empty() {
            continue;
        }

        // Source, contract, or config file. Source files go to language
        // extractors; contract files (e.g., .proto) go to the contract indexing
        // subpipeline; config files are tracked for invalidation widening but not
        // extracted. (Unchanged from the prior loader.)
        let ext = routing::get_extension(&rel_path);
        let is_scannable = routing::is_source_extension(ext)
            || routing::is_contract_extension(ext)
            || routing::is_config_file(&rel_path);
        if !is_scannable {
            continue;
        }

        // Read content. Unreadable files are tracked, not dropped.
        match std::fs::read_to_string(path) {
            Ok(content) => {
                let size_bytes = content.len();
                let line_count = content.split('\n').count();
                let content_hash = hash_content(&content);
                let language = routing::detect_language(&rel_path);
                let is_test = routing::is_test_file(&rel_path);

                files.push(ScannedFile::Ok(ScannedFileOk {
                    rel_path,
                    content,
                    content_hash,
                    size_bytes,
                    line_count,
                    language,
                    is_test,
                }));
            }
            Err(_) => {
                files.push(ScannedFile::ReadFailed { rel_path });
            }
        }
    }

    // `ignore::Walk` yields entries in OS-dependent directory order; sort by
    // rel_path so output is deterministic (VISION: same input → same output) and
    // matches the documented ordering contract.
    files.sort_by(|a, b| a.rel_path().cmp(b.rel_path()));

    Ok(files)
}

/// Whether the scanner prunes this directory by hardcoded default (independent of
/// `.gitignore`), given its traversal `depth` (repo root = 0, its immediate
/// children = 1).
///
/// Two rules, per SCANNER-PRUNE-AUTHORITY (ratified 2026-07-13):
///
///   - **`.git` at ANY depth** — repo metadata (also submodule / worktree
///     gitdirs), never source. With `hidden(false)` set, `WalkBuilder` would
///     otherwise descend it; reading `.git/info/exclude` is internal to the
///     matcher and unaffected by this traversal prune, so excluding `.git` here
///     is both correct and load-bearing.
///   - **Every other [`routing::is_always_excluded_dir`] name ONLY at
///     repository-root depth (1)** — `node_modules`, `dist`, `build`, `out`,
///     `venv`, … are a performance/vendor guard for the common root-level case.
///     A NESTED directory of the same name is governed by the repo's own git
///     ignore semantics (via `WalkBuilder`), NOT this hardcoded list — so a
///     git-TRACKED nested source dir that merely shares one of these names (the
///     live casualty `rust/crates/coverage/`, or a tracked nested `out/`,
///     `venv/`, …) is never silently dropped. That honesty guarantee — "this is
///     what exists" must be true — is the whole point of the slice.
///
/// Accepted residual (ratified): a NESTED build/vendor dir that git neither
/// ignores nor tracks becomes scannable. Repos that care already ignore those;
/// honest inclusion beats silent exclusion.
///
/// The shared `routing::is_always_excluded_dir` (a mirror of the TS indexer's
/// `ALWAYS_EXCLUDED`) is intentionally left unchanged — root-depth gating is a
/// file-inventory traversal policy local to the scanner, not a change to that set.
fn is_scanner_pruned_dir(name: &str, depth: usize) -> bool {
    if name == ".git" {
        return true;
    }
    depth == 1 && routing::is_always_excluded_dir(name)
}

/// Error from the filesystem scanner.
#[derive(Debug)]
pub struct ScanError {
    pub message: String,
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "scan error: {}", self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Extract only Ok files for convenience.
    fn ok_files(files: &[ScannedFile]) -> Vec<&ScannedFileOk> {
        files
            .iter()
            .filter_map(|f| match f {
                ScannedFile::Ok(ok) => Some(ok),
                _ => None,
            })
            .collect()
    }

    fn failed_paths(files: &[ScannedFile]) -> Vec<&str> {
        files
            .iter()
            .filter_map(|f| match f {
                ScannedFile::ReadFailed { rel_path } => Some(rel_path.as_str()),
                _ => None,
            })
            .collect()
    }

    // ── hash_content ─────────────────────────────────────────

    #[test]
    fn hash_matches_ts_hashcontent() {
        // SHA-256("hello world") = b94d27b9934d3e08...
        assert_eq!(hash_content("hello world"), "b94d27b9934d3e08");
    }

    #[test]
    fn hash_empty_string() {
        // SHA-256("") = e3b0c44298fc1c14...
        assert_eq!(hash_content(""), "e3b0c44298fc1c14");
    }

    #[test]
    fn hash_is_16_chars() {
        assert_eq!(hash_content("any content").len(), 16);
    }

    // ── scan_repo ────────────────────────────────────────────

    #[test]
    fn scan_finds_ts_files_and_excludes_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/index.ts"), "const x = 1;").unwrap();
        fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        fs::write(root.join("node_modules/pkg/index.ts"), "const y = 2;").unwrap();
        fs::write(root.join("README.md"), "# Hello").unwrap();

        let files = scan_repo(root).unwrap();
        let ok = ok_files(&files);
        assert_eq!(ok.len(), 1);
        assert_eq!(ok[0].rel_path, "src/index.ts");
        assert_eq!(ok[0].content, "const x = 1;");
        assert_eq!(ok[0].language, Some("typescript"));
        assert!(!ok[0].is_test);
    }

    #[test]
    fn scan_respects_root_gitignore_without_git_dir() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // No .git directory — gitignore still works (matches TS).
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/keep.ts"), "const a = 1;").unwrap();
        fs::write(root.join("src/ignored.ts"), "const b = 2;").unwrap();
        fs::write(root.join(".gitignore"), "src/ignored.ts\n").unwrap();

        let files = scan_repo(root).unwrap();
        let ok = ok_files(&files);
        assert_eq!(ok.len(), 1);
        assert_eq!(ok[0].rel_path, "src/keep.ts");
    }

    #[test]
    fn scan_detects_test_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("test")).unwrap();
        fs::write(root.join("src/app.ts"), "export {}").unwrap();
        fs::write(root.join("test/app.test.ts"), "it('works', () => {})").unwrap();

        let files = scan_repo(root).unwrap();
        let ok = ok_files(&files);
        let app = ok.iter().find(|f| f.rel_path == "src/app.ts").unwrap();
        assert!(!app.is_test);
        let test = ok
            .iter()
            .find(|f| f.rel_path == "test/app.test.ts")
            .unwrap();
        assert!(test.is_test);
    }

    #[test]
    fn scan_computes_correct_hash_and_line_count() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let content = "line1\nline2\n";
        fs::write(root.join("file.ts"), content).unwrap();

        let files = scan_repo(root).unwrap();
        let ok = ok_files(&files);
        assert_eq!(ok.len(), 1);
        assert_eq!(ok[0].content_hash, hash_content(content));
        assert_eq!(ok[0].line_count, 3);
    }

    #[test]
    fn scan_excludes_build_and_dist_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("dist")).unwrap();
        fs::create_dir_all(root.join("build")).unwrap();
        fs::write(root.join("src/app.ts"), "export {}").unwrap();
        fs::write(root.join("dist/app.js"), "var x;").unwrap();
        fs::write(root.join("build/app.js"), "var y;").unwrap();

        let files = scan_repo(root).unwrap();
        let ok = ok_files(&files);
        assert_eq!(ok.len(), 1);
        assert_eq!(ok[0].rel_path, "src/app.ts");
    }

    #[test]
    fn scan_returns_sorted_by_path() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::create_dir_all(root.join("src/b")).unwrap();
        fs::create_dir_all(root.join("src/a")).unwrap();
        fs::write(root.join("src/b/z.ts"), "").unwrap();
        fs::write(root.join("src/a/a.ts"), "").unwrap();
        fs::write(root.join("src/a/b.ts"), "").unwrap();

        let files = scan_repo(root).unwrap();
        let ok = ok_files(&files);
        let paths: Vec<&str> = ok.iter().map(|f| f.rel_path.as_str()).collect();
        assert_eq!(paths, vec!["src/a/a.ts", "src/a/b.ts", "src/b/z.ts"]);
    }

    // ── Read failure tracking ────────────────────────────────

    #[test]
    fn unreadable_file_tracked_as_read_failed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/good.ts"), "ok").unwrap();

        // Make a real source file unreadable via permissions so the READ (not the
        // walk) fails — it must surface as `ReadFailed`, never silently vanish.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let bad_path = root.join("src/bad.ts");
            fs::write(&bad_path, "secret").unwrap();
            fs::set_permissions(&bad_path, fs::Permissions::from_mode(0o000)).unwrap();

            let files = scan_repo(root).unwrap();
            let ok = ok_files(&files);
            let failed = failed_paths(&files);

            assert_eq!(ok.len(), 1);
            assert_eq!(ok[0].rel_path, "src/good.ts");
            assert_eq!(failed.len(), 1);
            assert_eq!(failed[0], "src/bad.ts");

            // Restore permissions for cleanup.
            fs::set_permissions(&bad_path, fs::Permissions::from_mode(0o644)).unwrap();
        }
    }

    // ── SCANNER-GITIGNORE-1 fixtures (full git ignore semantics) ─────────────

    fn ok_paths(files: &[ScannedFile]) -> Vec<&str> {
        ok_files(files)
            .iter()
            .map(|f| f.rel_path.as_str())
            .collect()
    }

    /// Fixture (a) — the live casualty. A root-anchored `/coverage/` ignore must
    /// exclude ONLY the root-level `coverage/`, while a nested, git-tracked
    /// `rust/crates/coverage/` SOURCE dir stays indexed. Under the prior loader
    /// (`is_always_excluded_dir("coverage")` segment match) the nested crate was
    /// silently dropped — zero FILE nodes, no caveat.
    #[test]
    fn scan_indexes_nested_coverage_dir_despite_root_anchored_ignore() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join(".gitignore"), "/coverage/\n").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("coverage")).unwrap();
        fs::create_dir_all(root.join("rust/crates/coverage/src")).unwrap();
        fs::write(root.join("src/keep.ts"), "export const a = 1;").unwrap();
        fs::write(root.join("coverage/report.ts"), "// root coverage output").unwrap();
        fs::write(
            root.join("rust/crates/coverage/src/lib.rs"),
            "pub fn cov() {}",
        )
        .unwrap();

        let files = scan_repo(root).unwrap();
        let paths = ok_paths(&files);

        // Nested source crate INDEXED (the fix).
        assert!(
            paths.contains(&"rust/crates/coverage/src/lib.rs"),
            "nested coverage crate must be indexed, got {paths:?}"
        );
        // Root-level coverage output EXCLUDED (anchored /coverage/).
        assert!(
            !paths.contains(&"coverage/report.ts"),
            "root coverage output must be excluded, got {paths:?}"
        );
        // Control file present.
        assert!(paths.contains(&"src/keep.ts"));
    }

    /// Fixture (b) — negation. `keepdir/*` ignores the directory's contents;
    /// `!keepdir/keep.ts` re-includes one file. (Contents form, not `keepdir/`,
    /// because git cannot re-include a file under a fully-excluded directory.)
    #[test]
    fn scan_honors_gitignore_negation() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join(".gitignore"), "keepdir/*\n!keepdir/keep.ts\n").unwrap();
        fs::create_dir_all(root.join("keepdir")).unwrap();
        fs::write(root.join("keepdir/keep.ts"), "export const k = 1;").unwrap();
        fs::write(root.join("keepdir/drop.ts"), "export const d = 2;").unwrap();

        let files = scan_repo(root).unwrap();
        let paths = ok_paths(&files);
        assert!(
            paths.contains(&"keepdir/keep.ts"),
            "negation must re-include keep.ts, got {paths:?}"
        );
        assert!(
            !paths.contains(&"keepdir/drop.ts"),
            "keepdir/drop.ts must stay excluded, got {paths:?}"
        );
    }

    /// Fixture (c) — nested `.gitignore`. A `.gitignore` under `src/a/` ignores a
    /// file only within that subtree; the prior root-only loader ignored nested
    /// `.gitignore` files entirely.
    #[test]
    fn scan_honors_nested_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::create_dir_all(root.join("src/a")).unwrap();
        fs::create_dir_all(root.join("src/b")).unwrap();
        fs::write(root.join("src/a/.gitignore"), "secret.ts\n").unwrap();
        fs::write(root.join("src/a/secret.ts"), "// hidden by nested ignore").unwrap();
        fs::write(root.join("src/a/ok.ts"), "export const a = 1;").unwrap();
        // Same basename outside the subtree is unaffected by the nested rule.
        fs::write(root.join("src/b/secret.ts"), "export const b = 1;").unwrap();

        let files = scan_repo(root).unwrap();
        let paths = ok_paths(&files);
        assert!(
            !paths.contains(&"src/a/secret.ts"),
            "nested .gitignore must exclude src/a/secret.ts, got {paths:?}"
        );
        assert!(paths.contains(&"src/a/ok.ts"));
        assert!(
            paths.contains(&"src/b/secret.ts"),
            "nested rule must not leak outside its subtree, got {paths:?}"
        );
    }

    /// Fixture (d) — parity. On a tree with only plain patterns (no anchoring,
    /// nesting, or negation) the new scanner yields exactly the file set the prior
    /// walkdir + root-only `Gitignore` loader produced. The other tests in this
    /// module (which encode the prior behavior and remain unchanged) are the
    /// regression net; this fixture pins the full set explicitly.
    #[test]
    fn scan_parity_on_plain_patterns() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join(".gitignore"), "src/ignored.ts\n").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        fs::create_dir_all(root.join("dist")).unwrap();
        fs::write(root.join("src/app.ts"), "export {}").unwrap();
        fs::write(root.join("src/util.ts"), "export {}").unwrap();
        fs::write(root.join("src/ignored.ts"), "export {}").unwrap(); // plain gitignore
        fs::write(root.join("node_modules/pkg/index.ts"), "export {}").unwrap(); // pruned dir
        fs::write(root.join("dist/bundle.ts"), "export {}").unwrap(); // pruned dir
        fs::write(root.join("README.md"), "# hi").unwrap(); // non-source ext

        let files = scan_repo(root).unwrap();
        let paths = ok_paths(&files);
        assert_eq!(
            paths,
            vec!["src/app.ts", "src/util.ts"],
            "plain-pattern set must equal the prior loader's output"
        );
    }

    /// Root-depth-only hardcoded prune policy (SCANNER-PRUNE-AUTHORITY,
    /// 2026-07-13): `.git` at any depth; every other `ALWAYS_EXCLUDED` name at
    /// repository-root depth (1) only; nested same-named dirs are NOT
    /// hardcode-pruned (git governs them).
    #[test]
    fn scanner_prune_policy_is_root_depth_only_except_dot_git() {
        // `.git` — pruned at every depth (repo metadata / submodule gitdir).
        assert!(is_scanner_pruned_dir(".git", 1));
        assert!(is_scanner_pruned_dir(".git", 2));
        assert!(is_scanner_pruned_dir(".git", 5));

        // Other vendor/build names — pruned ONLY at repository-root depth (1);
        // a nested dir of the same name is left for git semantics to govern.
        for name in [
            "node_modules",
            "dist",
            "build",
            "out",
            "coverage",
            "venv",
            "__pycache__",
        ] {
            assert!(
                is_scanner_pruned_dir(name, 1),
                "{name} must be pruned at repository-root depth"
            );
            assert!(
                !is_scanner_pruned_dir(name, 2),
                "nested {name} must NOT be hardcode-pruned (git governs it)"
            );
            assert!(
                !is_scanner_pruned_dir(name, 3),
                "deeply-nested {name} must NOT be hardcode-pruned"
            );
        }

        // Ordinary source dir names are never pruned, at any depth.
        assert!(!is_scanner_pruned_dir("src", 1));
        assert!(!is_scanner_pruned_dir("src", 2));
    }

    /// Root-depth-only, end-to-end. For EVERY hardcoded `ALWAYS_EXCLUDED` name
    /// except `.git`: a root-level dir of that name is pruned, while a
    /// git-tracked NESTED source dir of the same name survives. No `.gitignore`
    /// here, so the hardcoded prune is the only thing that could exclude these —
    /// this is the exact silent-drop class the slice closes (out, .cache, venv,
    /// .next, … — not just `coverage`). (SCANNER-PRUNE-AUTHORITY revise #1.)
    #[test]
    fn scan_prunes_hardcoded_dirs_at_root_only_not_nested() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Every hardcoded name EXCEPT `.git` (pruned at any depth — see the
        // dedicated test). These mirror `routing::is_always_excluded_dir`.
        let names = [
            "node_modules",
            "dist",
            "build",
            "out",
            ".next",
            ".nuxt",
            "coverage",
            ".turbo",
            ".cache",
            "venv",
            ".venv",
            "__pycache__",
            "cdk.out",
        ];

        // Control source that must always survive.
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/keep.ts"), "export const k = 1;").unwrap();

        for name in names {
            // Root-level dir (depth 1) — pruned.
            fs::create_dir_all(root.join(name)).unwrap();
            fs::write(root.join(name).join("root.ts"), "export const r = 1;").unwrap();
            // Nested tracked source dir of the SAME name (depth 2) — survives.
            fs::create_dir_all(root.join("src").join(name)).unwrap();
            fs::write(
                root.join("src").join(name).join("nested.ts"),
                "export const n = 1;",
            )
            .unwrap();
        }

        let files = scan_repo(root).unwrap();
        let paths = ok_paths(&files);

        for name in names {
            let root_path = format!("{name}/root.ts");
            let nested_path = format!("src/{name}/nested.ts");
            assert!(
                !paths.contains(&root_path.as_str()),
                "root-level {name}/ must be pruned, got {paths:?}"
            );
            assert!(
                paths.contains(&nested_path.as_str()),
                "nested tracked src/{name}/ must survive root-depth-only prune, got {paths:?}"
            );
        }
        assert!(paths.contains(&"src/keep.ts"));
    }

    /// `.git` is pruned at ANY depth (repo metadata / submodule gitdir), while
    /// source living beside a nested `.git` stays indexed.
    #[test]
    fn scan_prunes_dot_git_at_any_depth() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/app.ts"), "export {}").unwrap();

        // Root-level `.git` — its (source-extensioned) internals must not appear.
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/hook.ts"), "// git internal").unwrap();

        // `.git` nested under a plain directory — also excluded at depth > 1.
        fs::create_dir_all(root.join("pkg/.git")).unwrap();
        fs::write(root.join("pkg/.git/hook.ts"), "// nested git internal").unwrap();
        // Real source in that same subtree stays indexed.
        fs::write(root.join("pkg/lib.ts"), "export const p = 1;").unwrap();

        let files = scan_repo(root).unwrap();
        let paths = ok_paths(&files);

        assert!(
            !paths
                .iter()
                .any(|p| p.starts_with(".git/") || p.contains("/.git/")),
            "no .git contents at any depth, got {paths:?}"
        );
        assert!(paths.contains(&"src/app.ts"));
        assert!(
            paths.contains(&"pkg/lib.ts"),
            "source beside a nested .git must stay indexed, got {paths:?}"
        );
    }

    /// Symlinked source files are followed to their target and read — parity with
    /// the prior walkdir loader (which skipped only `is_dir()` entries). Build-0's
    /// `is_file()` filter dropped symlinks silently; this pins the restored
    /// behavior. (SCANNER-GITIGNORE-1 revise #2.)
    #[test]
    #[cfg(unix)]
    fn scan_yields_symlinked_source_files() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::create_dir_all(root.join("src")).unwrap();
        // A real source file, and a symlink to it under a scanned dir.
        fs::write(root.join("real.ts"), "export const real = 1;").unwrap();
        symlink(root.join("real.ts"), root.join("src/link.ts")).unwrap();

        let files = scan_repo(root).unwrap();
        let ok = ok_files(&files);

        let link = ok
            .iter()
            .find(|f| f.rel_path == "src/link.ts")
            .expect("symlinked source file must be yielded (parity with prior loader)");
        // Content is read THROUGH the link (follow_links(false) on the walk;
        // read_to_string follows the symlink to its target).
        assert_eq!(link.content, "export const real = 1;");
        assert_eq!(link.language, Some("typescript"));
    }
}
