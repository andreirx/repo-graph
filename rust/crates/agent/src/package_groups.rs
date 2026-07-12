//! Directory/package-group roll-up (MODULE-MODEL-1 D2(i)/D4; MODULE-MODEL-2 §13).
//!
//! Pure domain logic: fold the per-leaf-directory file-ownership topology — a
//! Layer-0/1 EXTRACTED fact (`nodes` kind=MODULE ⋈ OWNS, the same physical
//! directory facts `stats` enumerates) — into the *logical package groups* an
//! agent orients by. The fold is toolchain-aware (D4, §13): the merge key for a
//! leaf directory is
//!
//!   1. **its enclosing crate / workspace package** when a manifest root
//!      (`Cargo.toml` / `package.json`) covers it — so a Rust workspace names
//!      `agent`, `rgr`, `indexer`, … (whole crate = one group), and a TS
//!      workspace names its packages, not raw `src/...` fragments; else
//!   2. the **JVM `src/main`↔`src/test` logical package** (both halves merged,
//!      carrying a test-file count); else
//!   3. the **leaf directory verbatim** (C/C++ and manifest-less trees).
//!
//! Two display transforms then run over the merged keys:
//!   - a manifest-rooted key shows its crate/package name (the root's last path
//!     segment); a non-manifest key collapses the meaningless common source-root
//!     prefix (`src/main/java/org/springframework/samples/petclinic` → `owner`);
//!   - collision-safe naming (§13 D7): if two keys would render identically,
//!     each is disambiguated with the shortest distinguishing path suffix.
//!
//! This is the ONE shared computation behind the *topology* notion
//! (MODULE-MODEL-1 §6/§10). Two concrete current callers fold the SAME
//! leaf-directory set (with the SAME `list_manifest_roots` facts) through it,
//! BOTH daemon-side (so the two commands cannot ship divergent topology):
//!   - the `orient` structure headline (`agent::aggregators::module_summary`), and
//!   - the `stats` response (`daemon-runtime::dispatch::inject_stats_summary_fields`),
//!     which folds the per-directory `stats` rows here and ships the COMPLETE
//!     `package_groups` set on the response. The `rgr::presentation::stats` client
//!     renderer does NOT fold — it only bounds + displays the pre-folded set.
//!
//! Because both share this fold AND the same directory population + manifest
//! facts, the two commands cannot report divergent topology numbers — the very
//! incoherence this slice closes. The simpler rejected alternative — each command
//! grouping leaf directories its own way — is the current bug (orient "1 module:
//! ." vs stats "11"; a Rust workspace shown as directory fragments).
//!
//! Bounding at scale (§13 D7) is a PRESENTATION concern, NOT a fold concern: this
//! fold always returns the COMPLETE group set (the headline count + JSON consume
//! it whole); the renderers apply the top-N cap + omission line. Keeping the cap
//! out of the fold is what lets orient's headline count and JSON stay TRUE while
//! the human table is bounded.
//!
//! Honesty / layering: package/directory groups are a Layer-0/1 extracted fact
//! (where files physically sit; manifest roots are Layer-0/1 too — a `Cargo.toml`
//! physically exists at that path). They are DISTINCT from the Layer-1/2
//! declared/inferred `module_candidates` notion the count "1 declared module"
//! reports — the two are separately labelled, never collapsed. No inference here;
//! deterministic; path-anchored.

use std::collections::BTreeMap;

/// One leaf directory that owns files — the input row to the roll-up.
///
/// `path` is the directory's repo-relative path (a MODULE node's
/// `qualified_name`); `file_count` is the number of files it directly owns
/// (its OWNS-edge count). This is exactly the `(module, file_count)` projection
/// `stats` already computes (`ModuleStatsResult`) and the
/// `list_directory_groups` agent-port read returns (`AgentDirectoryGroup`), so
/// both callers feed the roll-up an identical shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirGroup {
    pub path: String,
    pub file_count: u64,
}

/// The detected toolchain of a manifest root — the axis of variation that
/// decides how a leaf directory folds (D4, §13).
///
/// Only the two toolchains the ratified D4 groups by *manifest boundary* are
/// modelled; JVM (`src/main|test` collapse), Python, C/C++ and manifest-less
/// trees fold by the directory heuristic and never produce a `ManifestRoot`.
/// The simpler alternative — passing raw `(path, source_type)` string tuples —
/// was rejected: the kind drives distinct fold behaviour (crate-fold vs
/// package-fold + test detection), and a 2-variant enum keeps that dispatch
/// total and typo-proof at the fold boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestKind {
    /// Nearest `Cargo.toml` — the crate; sub-crate directories fold into it.
    RustCrate,
    /// Nearest `package.json` / `pnpm-workspace.yaml` — the workspace package.
    TsPackage,
}

/// One manifest-declared package boundary (a crate / workspace-package root).
///
/// `path` is the root's repo-relative directory (`module_candidates
/// .canonical_root_path`, e.g. `rust/crates/agent`); `kind` is derived from the
/// stored `module_candidate_evidence.source_type`. Produced by the
/// `list_manifest_roots` agent-port read; consumed only by [`rollup_package_groups`].
/// An empty slice reproduces the pre-D4 JVM/directory behaviour exactly (so a
/// repo with no indexed manifests degrades honestly to directory grouping).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestRoot {
    pub path: String,
    pub kind: ManifestKind,
}

/// One logical package group: a merge of a crate/package (D4) or a `src/main` +
/// `src/test` logical package, with its reader-facing name (prefix collapsed or
/// crate/package name, collision-disambiguated) and a test-file count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageGroup {
    /// Reader-facing name: a manifest group's crate/package name (`agent`), or a
    /// non-manifest group's package tail after collapsing the prefix common to
    /// every non-manifest group (`owner`, `vet`, …). Collision-safe: two groups
    /// never render the same name (§13 D7).
    pub name: String,
    /// Total files this group owns (across `src/main` + `src/test`, or the whole
    /// crate/package).
    pub file_count: u64,
    /// How many of those are test files (the `src/test/…`, a top-level
    /// `test(s)/…` branch, or a crate/package `tests/…` branch). `0` is honest
    /// "no separate test directory", never an inference about test adequacy.
    pub test_file_count: u64,
}

/// Fold leaf-directory rows into merged, named package groups (D4 toolchain-aware).
///
/// `manifest_roots` reshapes only the merge KEY (which directories fold together),
/// never the file counts — those always derive from `dirs`, the authoritative
/// population (§13 D2). Pass `&[]` for the pure JVM/directory behaviour.
///
/// Deterministic: same input set → same output, independent of input row order
/// (the accumulator is keyed and the final sort is total). Sort order: file
/// count DESC, then name ASC. Returns the COMPLETE set — the presentation layer
/// bounds it (§13 D7).
pub fn rollup_package_groups(
    dirs: &[DirGroup],
    manifest_roots: &[ManifestRoot],
) -> Vec<PackageGroup> {
    // Conservative root-manifest rule (ROOT-MANIFEST-POLYGLOT — ratified 2026-07-12):
    // a repo-root manifest ("." — the indexer's encoding for a root `Cargo.toml` /
    // `package.json`) folds the WHOLE tree ONLY when there are NO nested manifest
    // roots. When nested roots ALSO exist, a whole-tree "." would swallow sibling
    // directories that belong to manifests we could NOT resolve (e.g. workspace-
    // inheriting crates with no `module_candidate`) — a FALSE ownership claim (their
    // code shown as part of the root package) that also hides the crates the ratified
    // honest-degradation contract says must show as directory groups. So "." covers
    // the tree only when `no_nested_manifest_roots`; otherwise the root package's own
    // files degrade to directory groups too. Nearest-root absorption (folding the
    // unresolved nested toolchains into "." anyway) is REJECTED permanently — it
    // fabricates ownership. The suppression is not silent: when it fires, the
    // reader-frame line from [`root_manifest_limitation`] renders on orient + stats.
    // The name states exactly what the flag tests (the reviewer's correction: it is
    // "no non-dot roots", NOT "'.' is the sole manifest family" — two manifest kinds
    // both at "." still satisfy it). Computed ONCE (O(roots)); `classify` reads it.
    let no_nested_manifest_roots = !manifest_roots.iter().any(|r| r.path != ".");
    // key -> (total_files, test_files, is_manifest_rooted). BTreeMap so iteration
    // order is a deterministic function of the key set, not insertion order.
    let mut acc: BTreeMap<String, (u64, u64, bool)> = BTreeMap::new();
    for d in dirs {
        let (is_test, key, is_manifest) =
            classify(&d.path, manifest_roots, no_nested_manifest_roots);
        let entry = acc.entry(key).or_insert((0, 0, false));
        entry.0 += d.file_count;
        if is_test {
            entry.1 += d.file_count;
        }
        entry.2 |= is_manifest;
    }
    if acc.is_empty() {
        return Vec::new();
    }

    // Display names: manifest keys show their crate/package name (last segment);
    // non-manifest keys collapse the prefix common to the OTHER non-manifest keys
    // (so a stray manifest root cannot drag a JVM package's collapse, and vice
    // versa). Then disambiguate any collisions (§13 D7).
    let non_manifest_keys: Vec<&str> = acc
        .iter()
        .filter(|(_, &(_, _, is_manifest))| !is_manifest)
        .map(|(k, _)| k.as_str())
        .collect();
    let common = common_segment_prefix(&non_manifest_keys);

    // (key, display, total, test) — key retained for collision disambiguation.
    let mut named: Vec<(String, String, u64, u64)> = acc
        .iter()
        .map(|(key, &(total, test, is_manifest))| {
            let display = if is_manifest {
                last_segment(key).to_string()
            } else {
                display_name(key, &common)
            };
            (key.clone(), display, total, test)
        })
        .collect();
    disambiguate(&mut named);

    let mut groups: Vec<PackageGroup> = named
        .into_iter()
        .map(|(_, name, total, test)| PackageGroup {
            name,
            file_count: total,
            test_file_count: test,
        })
        .collect();

    // Stable sort keeps the disambiguated-name order as the tiebreak past
    // (size, name), so determinism holds.
    groups.sort_by(|a, b| {
        b.file_count
            .cmp(&a.file_count)
            .then_with(|| a.name.cmp(&b.name))
    });
    groups
}

/// The one-line, reader-frame limitation marker for the package-groups surface
/// (ROOT-MANIFEST-POLYGLOT — ratified 2026-07-12): `Some(line)` when a repo-root
/// manifest EXISTS but the conservative rule in [`rollup_package_groups`]
/// SUPPRESSED it (a nested non-root manifest root also exists, so "." folds
/// nothing and the root package's own directories show as directory groups), else
/// `None`.
///
/// The deep-vertical rule requires that deliberate degradation to RENDER, not hide
/// in a comment — so both daemon fold sites (orient's `module_summary` aggregator
/// and stats' `inject_stats_summary_fields`) call this and ship the line, and both
/// human surfaces + JSON carry it. Building the string HERE (the fold's home) keeps
/// ONE source of truth for both the suppression CONDITION and its wording, so the
/// two surfaces render the identical line — the same coherence guarantee the shared
/// fold gives the groups themselves.
///
/// Pure function of `manifest_roots` (the suppression decision does not read the
/// directory rows). The named manifest(s) speak the reader's language — the file
/// the root would have folded (`Cargo.toml` / `package.json`), not our internal
/// `ManifestKind`. The simpler rejected alternative — a bare bool the presentation
/// turns into prose — would duplicate the wording across orient + stats and risk
/// the two lines drifting; the concrete second caller (stats) is what earns this
/// shared string builder over an inline `if`.
pub fn root_manifest_limitation(manifest_roots: &[ManifestRoot]) -> Option<String> {
    // Suppression fires only when a NESTED (non-dot) root coexists with a root ".".
    if !manifest_roots.iter().any(|r| r.path != ".") {
        return None;
    }
    let dot_kind = |k: ManifestKind| manifest_roots.iter().any(|r| r.path == "." && r.kind == k);
    let manifest = match (
        dot_kind(ManifestKind::RustCrate),
        dot_kind(ManifestKind::TsPackage),
    ) {
        (true, true) => "Cargo.toml / package.json",
        (true, false) => "Cargo.toml",
        (false, true) => "package.json",
        // No root manifest at all — nothing was suppressed, so no marker.
        (false, false) => return None,
    };
    Some(format!(
        "root {manifest} not folded — nested toolchains present; \
         root-owned directories shown as directory groups"
    ))
}

/// Classify a leaf-directory path as (is_test, merge_key, is_manifest_rooted).
///
/// Precedence (D4, §13):
///   1. **Manifest root** — if a crate/package root covers this directory, the
///      whole crate/package is ONE group (merge key = the root path). Rust test
///      files live under the crate's `tests/`; TS under `test(s)/`, `__tests__/`
///      or `spec/` at the package root (co-located tests fall through as source —
///      honest under-count, never a fabricated test number).
///   2. **JVM `src/main`↔`src/test`** — folds the two halves of one logical
///      package (both `src/main/.../owner` and `src/test/.../owner` → key
///      `java/.../owner`), the delivered spring-petclinic shape (UNCHANGED).
///   3. **Top-level test roots** (`test/`, `tests/`, `__tests__/`, `spec/`) —
///      flagged test but not folded (no `src/main` twin).
///   4. Everything else keeps its path verbatim as the key.
fn classify(
    path: &str,
    manifest_roots: &[ManifestRoot],
    no_nested_manifest_roots: bool,
) -> (bool, String, bool) {
    if let Some(root) = nearest_manifest_root(path, manifest_roots, no_nested_manifest_roots) {
        let rel = relative_to(path, &root.path);
        let is_test = manifest_test_dir(rel, root.kind);
        return (is_test, root.path.clone(), true);
    }
    let (is_test, key) = classify_directory(path);
    (is_test, key, false)
}

/// The delivered (pre-D4) JVM/directory classification — used when no manifest
/// root covers the directory.
fn classify_directory(path: &str) -> (bool, String) {
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

/// The nearest (deepest) manifest root that covers `path` — the crate/package a
/// directory belongs to. Longest match wins, so a member crate/package captures
/// its subtree over an enclosing workspace root.
///
/// Coverage has two cases:
///   - **Repo-root manifest** (`root == "."`) — a root `Cargo.toml` / `package.json`
///     whose crate root the indexer encodes as the literal `"."` (`cargo_manifest
///     .rs` `"Cargo.toml" -> "."`; `package_json.rs` `"package.json" -> "."`). Its
///     directory paths carry no `"./"` prefix, so the ordinary segment-boundary test
///     never matches them — review-1 #1: without a special case a single-package
///     root repo never folds, its nested dirs wrongly fall back to directory
///     grouping, violating D4. It therefore covers every `path` — but ONLY when
///     `no_nested_manifest_roots` (no non-dot root exists; see the caller). When
///     nested roots coexist, `no_nested_manifest_roots` is false and "." covers
///     nothing, so sibling directories under manifests we could not resolve degrade
///     to directory groups instead of being swallowed into the root package.
///   - **Nested manifest** (`root != "."`) — covers `path` when `path == root` or
///     `path` starts with `root` at a segment boundary (`root/`), so
///     `rust/crates/agent` never falsely covers `rust/crates/agentx`.
fn nearest_manifest_root<'a>(
    path: &str,
    roots: &'a [ManifestRoot],
    no_nested_manifest_roots: bool,
) -> Option<&'a ManifestRoot> {
    roots
        .iter()
        .filter(|r| {
            if r.path == "." {
                no_nested_manifest_roots
            } else {
                path == r.path || path.starts_with(&format!("{}/", r.path))
            }
        })
        .max_by_key(|r| r.path.len())
}

/// `path` with the leading `root` (and its `/`) removed. `""` when `path == root`.
fn relative_to<'a>(path: &'a str, root: &str) -> &'a str {
    if path == root {
        return "";
    }
    path.strip_prefix(root)
        .and_then(|r| r.strip_prefix('/'))
        .unwrap_or(path)
}

/// Whether a directory RELATIVE to its manifest root is a test directory, by the
/// per-toolchain convention. Convention-based and honest: unrecognised layouts
/// count as source (test_file_count stays a truthful `0`).
fn manifest_test_dir(rel: &str, kind: ManifestKind) -> bool {
    let first = rel.split('/').next().unwrap_or("");
    match kind {
        // Cargo integration tests live in `<crate>/tests/`. Unit tests are inline
        // (`#[cfg(test)]`) → same source file → correctly counted as source.
        ManifestKind::RustCrate => first == "tests",
        // TS/JS test dirs at the package root.
        ManifestKind::TsPackage => matches!(first, "test" | "tests" | "__tests__" | "spec"),
    }
}

/// The last path segment of a key (a crate/package name, e.g. `agent`).
fn last_segment(key: &str) -> &str {
    key.rsplit('/').next().unwrap_or(key)
}

/// The longest run of leading whole path segments shared by every key.
///
/// Segment-wise (not byte-wise), so `a/bc` and `a/bd` share `a`, not `a/b`.
/// Empty when the keys share no leading segment (or there are none).
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

/// The reader-facing name for a NON-manifest key given the collapsed common
/// prefix.
///
/// A key strictly longer than the common prefix → the tail after it
/// (`…/petclinic/owner` with common `…/petclinic` → `owner`). A key that IS the
/// common prefix (the root package) → that prefix's last segment (`petclinic`).
/// `common` is a prefix of every non-manifest key by construction, so a key can
/// never be shorter than it.
fn display_name(key: &str, common: &[String]) -> String {
    let segs: Vec<&str> = key.split('/').collect();
    if segs.len() > common.len() {
        segs[common.len()..].join("/")
    } else {
        common.last().cloned().unwrap_or_else(|| key.to_string())
    }
}

/// Make display names collision-safe (§13 D7): when two keys render the same
/// name, replace each with the shortest suffix (in path segments) of its own key
/// that is unique among the colliding keys.
///
/// Single-pass, per-colliding-group: keys are distinct, so the whole key is a
/// guaranteed-unique fallback. A second-order collision (an extended name equal
/// to an unrelated group's name) is not resolved — vanishingly rare and still
/// deterministic; the spec's contract is "distinguish the colliding groups".
fn disambiguate(named: &mut [(String, String, u64, u64)]) {
    let mut by_display: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, e) in named.iter().enumerate() {
        by_display.entry(e.1.clone()).or_default().push(i);
    }
    for (_, idxs) in by_display {
        if idxs.len() < 2 {
            continue;
        }
        let keys: Vec<String> = idxs.iter().map(|&i| named[i].0.clone()).collect();
        for (pos, &i) in idxs.iter().enumerate() {
            named[i].1 = shortest_distinguishing_suffix(&keys[pos], &keys, pos);
        }
    }
}

/// The shortest whole-segment suffix of `key` not shared (at equal segment
/// length) by any OTHER key in `others`. Falls back to the whole key (unique,
/// since keys are distinct).
fn shortest_distinguishing_suffix(key: &str, others: &[String], self_pos: usize) -> String {
    let segs: Vec<&str> = key.split('/').collect();
    for k in 1..segs.len() {
        let suffix = segs[segs.len() - k..].join("/");
        let unique = others.iter().enumerate().all(|(j, other)| {
            if j == self_pos {
                return true;
            }
            let osegs: Vec<&str> = other.split('/').collect();
            let osuffix = if k <= osegs.len() {
                osegs[osegs.len() - k..].join("/")
            } else {
                other.clone()
            };
            osuffix != suffix
        });
        if unique {
            return suffix;
        }
    }
    key.to_string()
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

    fn crate_root(path: &str) -> ManifestRoot {
        ManifestRoot {
            path: path.to_string(),
            kind: ManifestKind::RustCrate,
        }
    }

    fn ts_root(path: &str) -> ManifestRoot {
        ManifestRoot {
            path: path.to_string(),
            kind: ManifestKind::TsPackage,
        }
    }

    /// The load-bearing JVM case: spring-petclinic's 11 java leaf directories
    /// fold into the 6 §4 package groups with the exact merged counts
    /// (17/8/8/7/5/2 = 47, test 5/2/3/4/1/2). Ground-truthed against the real
    /// repo tree. NO manifest roots (JVM stays on the src/main|test heuristic) —
    /// the delivered shape is UNCHANGED (§13 D4 JVM: unchanged; regression guard).
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

        let groups = rollup_package_groups(&dirs, &[]);

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
        let names: Vec<String> = rollup_package_groups(&dirs, &[])
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
        let groups = rollup_package_groups(&dirs, &[]);
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
        let groups = rollup_package_groups(&dirs, &[]);
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
        let groups = rollup_package_groups(&dirs, &[]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "core");
        assert_eq!(groups[0].file_count, 9);
    }

    #[test]
    fn empty_input_yields_no_groups() {
        assert!(rollup_package_groups(&[], &[]).is_empty());
    }

    // ── MODULE-MODEL-2 §13 D4: per-toolchain grouping ─────────────────────────

    /// Rust workspace with 2+ crates + nested sub-crate directories → crate-level
    /// groups (D4 Rust: the crate is the group; sub-crate dirs fold into it). The
    /// self-dogfood shape: `rust/crates/{agent,rgr,indexer}` name as `agent`,
    /// `rgr`, `indexer`, NOT `agent/src`, `agent/src/aggregators`, … fragments.
    #[test]
    fn rust_workspace_folds_subdirs_into_crate_groups() {
        let dirs = vec![
            dg("rust/crates/agent", 1), // owns the crate Cargo.toml
            dg("rust/crates/agent/src", 20),
            dg("rust/crates/agent/src/aggregators", 8),
            dg("rust/crates/agent/tests", 4),
            dg("rust/crates/rgr", 1),
            dg("rust/crates/rgr/src", 30),
            dg("rust/crates/rgr/src/presentation", 12),
            dg("rust/crates/indexer/src", 15),
        ];
        let roots = vec![
            crate_root("rust/crates/agent"),
            crate_root("rust/crates/rgr"),
            crate_root("rust/crates/indexer"),
        ];
        let groups = rollup_package_groups(&dirs, &roots);

        let by_name: std::collections::HashMap<&str, &PackageGroup> =
            groups.iter().map(|g| (g.name.as_str(), g)).collect();
        assert_eq!(groups.len(), 3, "one group per crate: {groups:?}");
        // agent: 1 + 20 + 8 + 4 = 33, of which tests/ = 4 test files.
        assert_eq!(by_name["agent"].file_count, 33, "{groups:?}");
        assert_eq!(by_name["agent"].test_file_count, 4, "{groups:?}");
        // rgr: 1 + 30 + 12 = 43.
        assert_eq!(by_name["rgr"].file_count, 43);
        assert_eq!(by_name["rgr"].test_file_count, 0);
        // indexer: 15.
        assert_eq!(by_name["indexer"].file_count, 15);
        // Crate names, not raw directory fragments.
        assert!(
            groups.iter().all(|g| !g.name.contains('/')),
            "crate groups must name the crate, not a path fragment: {groups:?}"
        );
    }

    /// A stray non-manifest directory (a `docs/` tree, or the workspace-root dir
    /// owning the top `Cargo.toml`) must NOT drag the crate names down to
    /// `crates/agent`: manifest keys name their crate regardless of what
    /// non-manifest keys share.
    #[test]
    fn crate_names_survive_a_stray_non_manifest_dir() {
        let dirs = vec![
            dg("rust", 1), // workspace-root Cargo.toml, NOT a crate candidate
            dg("rust/crates/agent/src", 20),
            dg("rust/crates/rgr/src", 30),
        ];
        let roots = vec![
            crate_root("rust/crates/agent"),
            crate_root("rust/crates/rgr"),
        ];
        let groups = rollup_package_groups(&dirs, &roots);
        let names: Vec<&str> = groups.iter().map(|g| g.name.as_str()).collect();
        assert!(names.contains(&"agent"), "{names:?}");
        assert!(names.contains(&"rgr"), "{names:?}");
        // The stray `rust` dir is its own group, and does not turn agent into
        // `crates/agent`.
        assert!(names.contains(&"rust"), "{names:?}");
        assert!(!names.iter().any(|n| n.contains("crates/")), "{names:?}");
    }

    /// TS workspace: packages fold at the nearest `package.json`; `src/` and a
    /// `__tests__/` dir inside a package fold into it, tests counted (D4 TS).
    #[test]
    fn ts_workspace_folds_into_package_groups() {
        let dirs = vec![
            dg("packages/api/src", 12),
            dg("packages/api/src/routes", 6),
            dg("packages/api/__tests__", 3),
            dg("packages/core/src", 20),
            dg("packages/core/test", 5),
        ];
        let roots = vec![ts_root("packages/api"), ts_root("packages/core")];
        let groups = rollup_package_groups(&dirs, &roots);

        let by_name: std::collections::HashMap<&str, &PackageGroup> =
            groups.iter().map(|g| (g.name.as_str(), g)).collect();
        assert_eq!(groups.len(), 2, "{groups:?}");
        assert_eq!(by_name["api"].file_count, 21); // 12 + 6 + 3
        assert_eq!(by_name["api"].test_file_count, 3); // __tests__
        assert_eq!(by_name["core"].file_count, 25); // 20 + 5
        assert_eq!(by_name["core"].test_file_count, 5); // test/
    }

    /// Nearest-root wins: a member package inside a workspace captures its own
    /// subtree; only directories under the workspace root but outside any member
    /// fold into the workspace-root group.
    #[test]
    fn nearest_manifest_root_wins_over_enclosing_workspace() {
        let dirs = vec![
            dg("app", 1),         // workspace root package.json
            dg("app/scripts", 2), // under root only
            dg("app/packages/ui/src", 10),
        ];
        let roots = vec![ts_root("app"), ts_root("app/packages/ui")];
        let groups = rollup_package_groups(&dirs, &roots);
        let by_name: std::collections::HashMap<&str, &PackageGroup> =
            groups.iter().map(|g| (g.name.as_str(), g)).collect();
        // ui captures its src (nearest root); app captures root + scripts.
        assert_eq!(by_name["ui"].file_count, 10, "{groups:?}");
        assert_eq!(
            by_name["app"].file_count, 3,
            "app = root manifest + scripts: {groups:?}"
        );
    }

    /// Collision-safe naming (§13 D7): two crates with the SAME last segment
    /// (`foo`) must NOT both render `foo` — each gets the shortest distinguishing
    /// path suffix.
    #[test]
    fn collision_safe_display_names() {
        let dirs = vec![dg("services/a/foo/src", 5), dg("services/b/foo/src", 7)];
        let roots = vec![crate_root("services/a/foo"), crate_root("services/b/foo")];
        let names: Vec<String> = rollup_package_groups(&dirs, &roots)
            .into_iter()
            .map(|g| g.name)
            .collect();
        // Disambiguated to the shortest distinguishing suffix.
        assert!(names.contains(&"a/foo".to_string()), "{names:?}");
        assert!(names.contains(&"b/foo".to_string()), "{names:?}");
        assert!(!names.contains(&"foo".to_string()), "{names:?}");
        // Exactly two distinct names.
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 2, "names must be distinct: {names:?}");
    }

    /// A manifest root and a non-manifest directory that collide on the same
    /// display name are BOTH disambiguated (mixed-source collision).
    #[test]
    fn collision_across_manifest_and_directory_groups() {
        let dirs = vec![
            dg("crates/util/src", 9), // manifest → last segment "util"
            dg("vendor/util", 4),     // non-manifest → collapses to "util"
        ];
        let roots = vec![crate_root("crates/util")];
        let names: Vec<String> = rollup_package_groups(&dirs, &roots)
            .into_iter()
            .map(|g| g.name)
            .collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            2,
            "mixed-source collision unresolved: {names:?}"
        );
        assert!(
            names.iter().filter(|n| n.as_str() == "util").count() <= 1,
            "bare `util` must not survive the collision: {names:?}"
        );
    }

    /// The COMPLETE set is always returned (no cap in the fold) — the headline
    /// count + JSON depend on this; bounding is the renderer's job (§13 D7).
    #[test]
    fn fold_returns_complete_set_uncapped() {
        let dirs: Vec<DirGroup> = (0..150).map(|i| dg(&format!("pkg/m{i:03}"), 1)).collect();
        let groups = rollup_package_groups(&dirs, &[]);
        assert_eq!(groups.len(), 150, "fold must not cap");
    }

    /// review-1 #1: a repo-root `Cargo.toml` — the indexer stores its crate root as
    /// the literal `"."` (`cargo_manifest.rs` `"Cargo.toml" -> "."`) — must fold the
    /// WHOLE tree into ONE crate group, with counts conserved. Before the fix the
    /// coverage test (`path.starts_with("./")`) never matched a real directory, so a
    /// single-package root repo never folded (its nested dirs fell back to directory
    /// grouping, violating D4). The group's name is the honest root token `"."` (the
    /// crate genuinely roots at the repo root; the fold has only the path, not the
    /// package name — a nicer name would need the crate name plumbed from
    /// `module_candidates`, out of this fix's coverage-only scope).
    #[test]
    fn root_cargo_manifest_folds_whole_tree() {
        let dirs = vec![
            dg(".", 1), // the root Cargo.toml's own dir (e.g. build.rs)
            dg("src", 20),
            dg("src/handlers", 8),
            dg("tests", 4), // Cargo integration tests
        ];
        let roots = vec![crate_root(".")];
        let groups = rollup_package_groups(&dirs, &roots);
        assert_eq!(
            groups.len(),
            1,
            "root crate folds the whole tree: {groups:?}"
        );
        assert_eq!(groups[0].file_count, 33, "counts conserved: 1+20+8+4");
        assert_eq!(groups[0].test_file_count, 4, "tests/ counted as test");
        assert_eq!(groups[0].name, ".", "honest repo-root token");
    }

    /// review-1 #1 (TS twin): a repo-root `package.json` — `package_json.rs`
    /// `"package.json" -> "."` — folds the whole tree into ONE package group. A
    /// package-root `test/` dir is counted as test (TS convention).
    #[test]
    fn root_package_json_folds_whole_tree() {
        let dirs = vec![
            dg(".", 2), // the root package.json's own dir
            dg("src", 15),
            dg("src/routes", 6),
            dg("test", 3), // TS test dir at the package root
        ];
        let roots = vec![ts_root(".")];
        let groups = rollup_package_groups(&dirs, &roots);
        assert_eq!(
            groups.len(),
            1,
            "root package folds the whole tree: {groups:?}"
        );
        assert_eq!(groups[0].file_count, 26, "counts conserved: 2+15+6+3");
        assert_eq!(groups[0].test_file_count, 3, "test/ counted as test");
        assert_eq!(groups[0].name, ".", "honest repo-root token");
    }

    /// review-1 #1 REGRESSION GUARD (the polyglot case discovered on repo-graph's own
    /// self-index): a root `"."` manifest must NOT swallow the whole tree when OTHER
    /// manifest roots exist. repo-graph has a root `package.json` (TS) AND a nested
    /// Rust workspace; some crates (workspace-inheriting) have no `module_candidate`
    /// and are RATIFIED to degrade to directory groups until the upstream indexer
    /// slice lands. A naive "." = covers-everything folds those candidate-less crate
    /// dirs INTO the root TS package — a FALSE ownership claim (Rust code shown as TS)
    /// that also hides the crates the ratified contract says must show as directory
    /// groups. So when nested roots exist, "." folds nothing: its own files AND the
    /// candidate-less crates degrade to directory groups.
    #[test]
    fn root_dot_manifest_does_not_swallow_when_nested_roots_exist() {
        let dirs = vec![
            dg(".", 1),                      // root package.json's own dir
            dg("src", 10),                   // root TS source
            dg("rust/crates/agent/src", 20), // an explicit crate (has a candidate)
            dg("rust/crates/rgr/src", 30),   // a candidate-less crate (must degrade)
        ];
        // "." is the root TS package; only `agent` has a resolvable crate candidate.
        let roots = vec![ts_root("."), crate_root("rust/crates/agent")];
        let groups = rollup_package_groups(&dirs, &roots);
        let by_name: std::collections::HashMap<&str, &PackageGroup> =
            groups.iter().map(|g| (g.name.as_str(), g)).collect();
        // agent folds (nearest crate) — the resolvable crate still names correctly.
        assert_eq!(by_name["agent"].file_count, 20, "{groups:?}");
        // The candidate-less crate degrades to a directory group — NOT swallowed.
        assert!(
            by_name.keys().any(|k| k.ends_with("rgr/src")),
            "candidate-less crate must degrade to a directory group, not fold into '.': {groups:?}"
        );
        // No "." group balloons past its own single directory (no swallowing): the
        // root package's `src` also degrades to a directory group here.
        let dot_files = by_name.get(".").map(|g| g.file_count).unwrap_or(0);
        assert!(
            dot_files <= 1,
            "'.' must not swallow sibling territory (got {dot_files} files): {groups:?}"
        );
        assert!(
            by_name.contains_key("src"),
            "root src degrades to a dir group: {groups:?}"
        );
    }

    /// The sole-family case the regression guard above scopes AGAINST: with `"."` as
    /// the ONLY manifest family, it DOES cover the whole tree (the genuine
    /// single-package repo — reviewer's core case), same as
    /// `root_cargo_manifest_folds_whole_tree` but asserting the sole-family boundary
    /// explicitly alongside its negative twin.
    #[test]
    fn root_dot_manifest_folds_tree_when_sole_family() {
        let dirs = vec![dg(".", 1), dg("src", 10), dg("src/inner", 5)];
        let roots = vec![crate_root(".")];
        let groups = rollup_package_groups(&dirs, &roots);
        assert_eq!(groups.len(), 1, "sole '.' folds the whole tree: {groups:?}");
        assert_eq!(groups[0].file_count, 16, "1 + 10 + 5");
    }

    // ── ROOT-MANIFEST-POLYGLOT (ratified 2026-07-12): the VISIBLE limitation marker ──

    /// The operator's named case (present half): a root manifest ("." package.json)
    /// coexisting with a NESTED crate root is SUPPRESSED by the conservative rule, so
    /// the marker fires and names the suppressed manifest in the reader's terms. This
    /// is repo-graph's own shape (root package.json + nested Cargo crates).
    #[test]
    fn root_manifest_limitation_present_when_root_and_nested() {
        let roots = vec![ts_root("."), crate_root("rust/crates/agent")];
        let line = root_manifest_limitation(&roots).expect("marker present when root suppressed");
        assert!(line.contains("root package.json not folded"), "{line}");
        assert!(line.contains("nested toolchains present"), "{line}");
        assert!(line.contains("shown as directory groups"), "{line}");
    }

    /// The operator's named case (absent half): a root manifest ALONE (no nested
    /// root) DOES fold the tree, so nothing is suppressed → no marker.
    #[test]
    fn root_manifest_limitation_absent_when_root_alone() {
        assert!(root_manifest_limitation(&[ts_root(".")]).is_none());
        assert!(root_manifest_limitation(&[crate_root(".")]).is_none());
    }

    /// Nested roots only (no "." root — e.g. a Cargo workspace rooted at `rust/`):
    /// there is NO root manifest to suppress, so no marker (the marker is about the
    /// ROOT manifest's non-folding, not about nested degradation generally).
    #[test]
    fn root_manifest_limitation_absent_when_nested_only() {
        let roots = vec![
            crate_root("rust/crates/agent"),
            crate_root("rust/crates/rgr"),
        ];
        assert!(root_manifest_limitation(&roots).is_none());
    }

    /// No manifest facts at all (C/manifest-less tree) → no marker.
    #[test]
    fn root_manifest_limitation_absent_when_no_manifests() {
        assert!(root_manifest_limitation(&[]).is_none());
    }

    /// A root Cargo.toml (Rust) suppressed by a nested root names Cargo.toml — the
    /// marker speaks the actual suppressed manifest, not a fixed string.
    #[test]
    fn root_manifest_limitation_names_cargo_for_rust_root() {
        let roots = vec![crate_root("."), crate_root("crates/inner")];
        let line = root_manifest_limitation(&roots).expect("marker present");
        assert!(line.contains("root Cargo.toml not folded"), "{line}");
    }
}
