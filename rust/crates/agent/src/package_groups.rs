//! Directory/package-group roll-up (MODULE-MODEL-1, D2(i) + D4).
//!
//! Pure domain logic: fold the per-leaf-directory file-ownership topology — a
//! Layer-0/1 EXTRACTED fact (`nodes` kind=MODULE ⋈ OWNS, the same physical
//! directory facts `stats` enumerates) — into the *logical package groups* an
//! agent orients by. Two transforms:
//!
//!   1. **Merge** a Maven/Gradle `src/main` + `src/test` split of the SAME
//!      logical package into one group, carrying a test-file count (D4).
//!   2. **Collapse** the meaningless common source-root prefix
//!      (`src/main/java/org/springframework/samples/petclinic`) so the agent
//!      sees `owner`, not the 7-segment path (D4).
//!
//! This is the ONE shared computation behind the *topology* notion
//! (MODULE-MODEL-1 §6/§10). Two concrete current callers fold the SAME
//! leaf-directory set through it:
//!   - the `orient` structure headline (`aggregators::module_summary`), and
//!   - the `stats` presentation (`rgr::presentation::stats`).
//!
//! Because both share this fold, the two commands cannot report divergent
//! topology numbers — the very incoherence this slice closes. The simpler
//! rejected alternative — each command grouping leaf directories its own way —
//! is the current bug (orient "1 module: ." vs stats "11").
//!
//! Honesty / layering: package/directory groups are a Layer-0/1 extracted fact
//! (where files physically sit). They are DISTINCT from the Layer-1/2
//! declared/inferred `module_candidates` notion the count "1 declared module"
//! reports — the two are separately labelled, never collapsed. No inference
//! here; deterministic; path-anchored.

use std::collections::BTreeMap;

/// One leaf directory that owns files — the input row to the roll-up.
///
/// `path` is the directory's repo-relative path (a MODULE node's
/// `qualified_name`); `file_count` is the number of files it directly owns
/// (its OWNS-edge count). This is exactly the `(module, file_count)` projection
/// `stats` already computes (`ModuleStatsResult`) and the new
/// `list_directory_groups` agent-port read returns (`AgentDirectoryGroup`), so
/// both callers feed the roll-up an identical shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirGroup {
    pub path: String,
    pub file_count: u64,
}

/// One logical package group: a `src/main` + `src/test` merge of a logical
/// package, with its reader-facing name (common source-root prefix collapsed)
/// and a test-file count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageGroup {
    /// Reader-facing name: the package tail after collapsing the prefix common
    /// to every group (`owner`, `vet`, …). The root package — whose key IS the
    /// common prefix — shows that prefix's last segment (e.g. `petclinic`).
    pub name: String,
    /// Total files this package owns across `src/main` + `src/test`.
    pub file_count: u64,
    /// How many of those are test files (the `src/test/…` or a top-level
    /// `test(s)/…` branch). `0` is honest "no separate test directory", never
    /// an inference about test adequacy.
    pub test_file_count: u64,
}

/// Fold leaf-directory rows into merged, prefix-collapsed package groups.
///
/// Deterministic: same input set → same output, independent of input row order
/// (the accumulator is keyed and the final sort is total). Sort order: file
/// count DESC, then name ASC (stable on the keyed accumulator order, so two
/// distinct keys that collapse to the same display name keep a deterministic
/// relative order).
pub fn rollup_package_groups(dirs: &[DirGroup]) -> Vec<PackageGroup> {
    // key -> (total_files, test_files). BTreeMap so iteration order is a
    // deterministic function of the key set, not insertion order.
    let mut acc: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for d in dirs {
        let (is_test, key) = classify(&d.path);
        let entry = acc.entry(key).or_insert((0, 0));
        entry.0 += d.file_count;
        if is_test {
            entry.1 += d.file_count;
        }
    }
    if acc.is_empty() {
        return Vec::new();
    }

    let keys: Vec<&str> = acc.keys().map(String::as_str).collect();
    let common = common_segment_prefix(&keys);

    let mut groups: Vec<PackageGroup> = acc
        .iter()
        .map(|(key, &(total, test))| PackageGroup {
            name: display_name(key, &common),
            file_count: total,
            test_file_count: test,
        })
        .collect();

    // Stable sort keeps the BTreeMap key order as the tiebreak past (size, name),
    // so determinism holds even when two keys collapse to the same name.
    groups.sort_by(|a, b| {
        b.file_count
            .cmp(&a.file_count)
            .then_with(|| a.name.cmp(&b.name))
    });
    groups
}

/// Classify a leaf-directory path as (is_test, merge_key).
///
/// The merge key folds the Maven/Gradle `src/main` ↔ `src/test` distinction so
/// the two halves of one logical package coincide: both
/// `src/main/java/.../owner` and `src/test/java/.../owner` map to the key
/// `java/.../owner`. Top-level test roots (`test/`, `tests/`, `__tests__/`,
/// `spec/`) are flagged test but NOT folded (no `src/main` twin to merge with).
/// Everything else keeps its path verbatim as the key.
fn classify(path: &str) -> (bool, String) {
    if let Some(rest) = path.strip_prefix("src/test/") {
        return (true, rest.to_string());
    }
    if let Some(rest) = path.strip_prefix("src/main/") {
        return (false, rest.to_string());
    }
    let first = path.split('/').next().unwrap_or("");
    let is_test = matches!(first, "test" | "tests" | "__tests__" | "spec");
    (is_test, path.to_string())
}

/// The longest run of leading whole path segments shared by every key.
///
/// Segment-wise (not byte-wise), so `a/bc` and `a/bd` share `a`, not `a/b`.
/// Empty when the keys share no leading segment.
fn common_segment_prefix(keys: &[&str]) -> Vec<String> {
    let mut iter = keys.iter();
    let first: Vec<&str> = match iter.next() {
        Some(k) => k.split('/').collect(),
        None => return Vec::new(),
    };
    let mut prefix_len = first.len();
    for k in iter {
        let segs: Vec<&str> = k.split('/').collect();
        let mut i = 0;
        while i < prefix_len && i < segs.len() && segs[i] == first[i] {
            i += 1;
        }
        prefix_len = i;
        if prefix_len == 0 {
            break;
        }
    }
    first[..prefix_len].iter().map(|s| s.to_string()).collect()
}

/// The reader-facing name for a key given the collapsed common prefix.
///
/// A key strictly longer than the common prefix → the tail after it
/// (`…/petclinic/owner` with common `…/petclinic` → `owner`). A key that IS the
/// common prefix (the root package) → that prefix's last segment (`petclinic`).
/// `common` is a prefix of every key by construction, so a key can never be
/// shorter than it.
fn display_name(key: &str, common: &[String]) -> String {
    let segs: Vec<&str> = key.split('/').collect();
    if segs.len() > common.len() {
        segs[common.len()..].join("/")
    } else {
        common.last().cloned().unwrap_or_else(|| key.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dg(path: &str, n: u64) -> DirGroup {
        DirGroup {
            path: path.to_string(),
            file_count: n,
        }
    }

    /// The load-bearing case: spring-petclinic's 11 java leaf directories fold
    /// into the 6 §4 package groups with the exact merged counts
    /// (17/8/8/7/5/2 = 47, test 5/2/3/4/1/2). Ground-truthed against the real
    /// repo tree.
    #[test]
    fn petclinic_eleven_dirs_fold_to_six_packages() {
        let base = "src/main/java/org/springframework/samples/petclinic";
        let tbase = "src/test/java/org/springframework/samples/petclinic";
        let dirs = vec![
            dg(base, 3),
            dg(&format!("{base}/owner"), 12),
            dg(&format!("{base}/vet"), 6),
            dg(&format!("{base}/system"), 5),
            dg(&format!("{base}/model"), 4),
            dg(tbase, 4),
            dg(&format!("{tbase}/owner"), 5),
            dg(&format!("{tbase}/system"), 3),
            dg(&format!("{tbase}/service"), 2),
            dg(&format!("{tbase}/vet"), 2),
            dg(&format!("{tbase}/model"), 1),
        ];

        let groups = rollup_package_groups(&dirs);

        // 6 merged groups; total files conserved at 47.
        assert_eq!(groups.len(), 6, "got {groups:?}");
        assert_eq!(groups.iter().map(|g| g.file_count).sum::<u64>(), 47);
        assert_eq!(groups.iter().map(|g| g.test_file_count).sum::<u64>(), 17);

        let by_name: std::collections::HashMap<&str, &PackageGroup> =
            groups.iter().map(|g| (g.name.as_str(), g)).collect();
        let expect = [
            ("owner", 17, 5),
            ("vet", 8, 2),
            ("system", 8, 3),
            ("petclinic", 7, 4), // the root package: prefix's last segment
            ("model", 5, 1),
            ("service", 2, 2),
        ];
        for (name, files, test) in expect {
            let g = by_name
                .get(name)
                .unwrap_or_else(|| panic!("missing package {name}: {groups:?}"));
            assert_eq!(g.file_count, files, "{name} files");
            assert_eq!(g.test_file_count, test, "{name} test");
        }

        // Names are collapsed — the meaningless prefix is gone.
        assert!(
            groups.iter().all(|g| !g.name.contains("springframework")),
            "prefix not collapsed: {groups:?}"
        );
    }

    /// Deterministic order: file count DESC then name ASC, regardless of input
    /// row order. `owner` (17) first; the two 8-file packages tie-break to
    /// `system` < `vet`.
    #[test]
    fn order_is_size_desc_then_name_asc() {
        let base = "src/main/java/org/app";
        let tbase = "src/test/java/org/app";
        let dirs = vec![
            dg(&format!("{tbase}/vet"), 2),
            dg(&format!("{base}/owner"), 12),
            dg(&format!("{base}/system"), 5),
            dg(&format!("{tbase}/system"), 3),
            dg(&format!("{base}/vet"), 6),
            dg(&format!("{tbase}/owner"), 5),
        ];
        let names: Vec<String> = rollup_package_groups(&dirs)
            .into_iter()
            .map(|g| g.name)
            .collect();
        assert_eq!(names, vec!["owner", "system", "vet"]);
    }

    /// A non-Maven layout (no `src/main` / `src/test`): each leaf dir is its own
    /// group; the common `src` prefix collapses; no test files detected (honest
    /// zero, not an overclaim).
    #[test]
    fn flat_src_layout_no_main_test_split() {
        let dirs = vec![
            dg("src/handlers", 45),
            dg("src/models", 38),
            dg("src/utils", 10),
        ];
        let groups = rollup_package_groups(&dirs);
        assert_eq!(groups.len(), 3);
        assert_eq!(
            groups.iter().map(|g| g.name.clone()).collect::<Vec<_>>(),
            vec!["handlers", "models", "utils"]
        );
        assert_eq!(groups.iter().map(|g| g.test_file_count).sum::<u64>(), 0);
    }

    /// Top-level `tests/` is flagged test but not folded with a same-named
    /// source dir (different physical layout, no `src/main` twin).
    #[test]
    fn top_level_test_dir_counts_as_test() {
        let dirs = vec![dg("src/foo", 10), dg("tests/foo", 4)];
        let groups = rollup_package_groups(&dirs);
        // No common prefix between `src/foo` and `tests/foo` → full keys as names.
        assert_eq!(groups.len(), 2);
        let test_total: u64 = groups.iter().map(|g| g.test_file_count).sum();
        assert_eq!(
            test_total, 4,
            "tests/foo contributes test files: {groups:?}"
        );
    }

    /// A single package: its key IS the whole common prefix → name is the last
    /// segment.
    #[test]
    fn single_package_uses_last_segment() {
        let dirs = vec![dg("src/main/java/org/app/core", 9)];
        let groups = rollup_package_groups(&dirs);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "core");
        assert_eq!(groups[0].file_count, 9);
    }

    #[test]
    fn empty_input_yields_no_groups() {
        assert!(rollup_package_groups(&[]).is_empty());
    }
}
