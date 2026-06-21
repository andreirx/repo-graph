//! CYCLES-COMPLETENESS-ENUMERATION-1: SHARED TypeScript partition-root discovery for BOTH the (read-only)
//! completeness audit and the (mutating) `livegraph-refresh --all-discovered`. ONE function so the audit's
//! EXPECTED set and the refresh's LOAD PLAN cannot drift.
//!
//! Discovery (D1, ratified): every `tsconfig.json` directory (bounded `std::fs` walk; skips deps/VCS/build +
//! dotfiles; no symlink follow), MINUS directories under a CLOSED fixture-segment set. The exclusion is
//! REPO-ROOT-RELATIVE (D3), so auditing a fixture AS its own repo keeps its child packages.
//!
//! Exclusion is the FALSE-COMPLETE risk (drop a real partition -> the cert stops requiring it -> it may reach
//! `Complete` with a real partition unloaded). So the set is NARROW + CLOSED: explicit corpus/test-DATA names
//! ONLY. `__tests__`/`__mocks__` are deliberately NOT excluded (real source-adjacent test trees that may
//! import production code). Over-inclusion is safe (`IncompleteMissingPartitions`); under-inclusion is not.

use std::path::{Path, PathBuf};

/// Directories never descended (deps / VCS / build output / our own artifacts).
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "dist",
    "build",
    "target",
    ".rgr",
    "out",
    "coverage",
];

/// CLOSED fixture-segment exclusion set (D1, ratified NARROW). A discovered tsconfig directory whose
/// repo-relative path has ANY path SEGMENT in this set is EXCLUDED from repo-level completeness. Corpus /
/// test-DATA names ONLY. NOT `__tests__`/`__mocks__` (real test source can import production code, so
/// excluding it would risk a false `Complete`). NO fuzzy "test" substring matching.
const FIXTURE_SEGMENTS: &[&str] = &["fixtures", "__fixtures__", "testdata"];

/// The discovered partition roots, split by the fixture policy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiscoveredPartitions {
    /// Repo-relative source roots to certify/load (feed to `derive_partition_target` / `run_refresh_multi`).
    /// The repo root itself appears as `"."` (-> the `"default"` partition).
    pub included: Vec<String>,
    /// `(repo-relative dir, reason)` for EXCLUDED fixture tsconfigs -- listed in the audit report so an
    /// exclusion is never silent.
    pub excluded: Vec<(String, String)>,
}

/// If `rel_path` (repo-relative, `/`-separated) has a fixture segment, return its exclusion reason; else None.
fn fixture_exclusion_reason(rel_path: &str) -> Option<String> {
    for seg in rel_path.split('/') {
        if FIXTURE_SEGMENTS.contains(&seg) {
            return Some(format!("fixture segment '{seg}'"));
        }
    }
    None
}

/// Discover TS partition roots under `repo_root`, split into `included` / `excluded` by the fixture policy.
/// `include_fixtures` disables the exclusion (the `--include-fixtures` opt-in: deliberately certify a fixture
/// corpus). Deterministic (sorted, deduped). Bounded `std::fs` walk; no new dependency; no symlink follow.
pub fn discover_partition_roots(repo_root: &str, include_fixtures: bool) -> DiscoveredPartitions {
    let root = Path::new(repo_root);
    let mut dirs: Vec<String> = Vec::new();
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let mut has_tsconfig = false;
        for entry in entries.flatten() {
            let file_type = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with('.') || SKIP_DIRS.contains(&name.as_ref()) {
                    continue;
                }
                stack.push(entry.path());
            } else if entry.file_name() == "tsconfig.json" {
                has_tsconfig = true;
            }
        }
        if has_tsconfig {
            if let Ok(rel) = dir.strip_prefix(root) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                dirs.push(if rel.is_empty() { ".".to_string() } else { rel });
            }
        }
    }
    dirs.sort();
    dirs.dedup();

    let mut included = Vec::new();
    let mut excluded = Vec::new();
    for d in dirs {
        let reason = if include_fixtures {
            None
        } else {
            fixture_exclusion_reason(&d)
        };
        match reason {
            Some(r) => excluded.push((d, r)),
            None => included.push(d),
        }
    }
    DiscoveredPartitions { included, excluded }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_tsconfig(root: &Path, rel: &str) {
        let dir = root.join(rel);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("tsconfig.json"), "{}").unwrap();
    }

    #[test]
    fn includes_real_roots_skips_node_modules() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        mk_tsconfig(root, "packages/a");
        mk_tsconfig(root, "packages/b");
        mk_tsconfig(root, "node_modules/dep"); // skipped during the walk
        std::fs::create_dir_all(root.join("packages/c")).unwrap(); // no tsconfig -> absent

        let d = discover_partition_roots(&root.to_string_lossy(), false);
        assert!(d.included.contains(&"packages/a".to_string()));
        assert!(d.included.contains(&"packages/b".to_string()));
        assert!(!d.included.iter().any(|p| p.contains("node_modules")));
        assert!(!d.included.contains(&"packages/c".to_string()));
    }

    #[test]
    fn excludes_only_the_closed_fixture_segments() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        mk_tsconfig(root, "src"); // real
        mk_tsconfig(root, "test/fixtures/corpus"); // fixtures -> excluded
        mk_tsconfig(root, "pkg/__fixtures__/x"); // __fixtures__ -> excluded
        mk_tsconfig(root, "data/testdata"); // testdata -> excluded
        mk_tsconfig(root, "pkg/__tests__/unit"); // __tests__ -> NOT excluded (narrow policy)
        mk_tsconfig(root, "pkg/__mocks__/m"); // __mocks__ -> NOT excluded (narrow policy)

        let d = discover_partition_roots(&root.to_string_lossy(), false);
        assert!(d.included.contains(&"src".to_string()));
        assert!(
            d.included.contains(&"pkg/__tests__/unit".to_string()),
            "__tests__ must NOT be excluded (false-Complete risk); got {:?}",
            d.included
        );
        assert!(d.included.contains(&"pkg/__mocks__/m".to_string()));
        let excluded_dirs: Vec<&String> = d.excluded.iter().map(|(p, _)| p).collect();
        assert!(excluded_dirs.contains(&&"test/fixtures/corpus".to_string()));
        assert!(excluded_dirs.contains(&&"pkg/__fixtures__/x".to_string()));
        assert!(excluded_dirs.contains(&&"data/testdata".to_string()));
        // reasons name the matched segment
        assert!(d.excluded.iter().any(|(_, r)| r.contains("fixtures")));
    }

    #[test]
    fn include_fixtures_opt_in_keeps_everything() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        mk_tsconfig(root, "src");
        mk_tsconfig(root, "test/fixtures/corpus");

        let d = discover_partition_roots(&root.to_string_lossy(), true);
        assert!(d.excluded.is_empty(), "opt-in disables exclusion");
        assert!(d.included.contains(&"test/fixtures/corpus".to_string()));
    }

    #[test]
    fn fixture_root_keeps_its_own_child_packages() {
        // Auditing a fixture AS its own repo: child packages are NOT under a fixture segment relative to the
        // fixture root, so they are INCLUDED (D3 "unless the repo itself is a fixture root").
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path(); // pretend this IS xpart-monorepo
        mk_tsconfig(root, "packages/a");
        mk_tsconfig(root, "packages/b");

        let d = discover_partition_roots(&root.to_string_lossy(), false);
        assert_eq!(
            d.included,
            vec!["packages/a".to_string(), "packages/b".to_string()]
        );
        assert!(d.excluded.is_empty());
    }

    #[test]
    fn repo_root_tsconfig_is_dot() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        mk_tsconfig(root, "."); // root tsconfig
        let d = discover_partition_roots(&root.to_string_lossy(), false);
        assert!(d.included.contains(&".".to_string()));
    }
}
