//! Config readers — package.json dependencies, tsconfig.json
//! path aliases, Cargo.toml dependencies, and build.gradle[.kts]
//! (Gradle) dependencies, with nearest-ancestor directory lookup.
//!
//! Mirrors the TS indexer's `resolveNearestPackageDeps` and
//! `resolveNearestTsconfigAliases` from `repo-indexer.ts`.
//!
//! Lookup rule (locked): walk from file's parent directory upward
//! to repo root. First matching config file wins. Cached by
//! directory so sibling files resolve in O(1).
//!
//! ## Cargo.toml dependency resolution (Rust-A3)
//!
//! For `.rs` files, resolves nearest owning Cargo.toml. Extracts
//! dependency names from [dependencies], [dev-dependencies],
//! [build-dependencies]. Sorted unique names, hyphen-normalized
//! to match Rust's `foo-bar` → `foo_bar` crate naming convention.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use repo_graph_classification::types::{
    PackageDependencySet, StoredTsconfigAliases, TsconfigAliasEntry, TsconfigAliasMapping,
    TsconfigAliases,
};

/// Pre-computed config context for a repo. Caches config lookups
/// by directory so each directory is resolved at most once.
pub struct RepoConfigContext {
    /// Directory → PackageDependencySet cache (for JS/TS via package.json).
    pkg_cache: HashMap<String, PackageDependencySet>,
    /// Directory → TsconfigAliases cache.
    tsconfig_cache: HashMap<String, TsconfigAliases>,
    /// TS-ALIAS-RESOLUTION-1: directory → the repo-relative path of its nearest `tsconfig.json`
    /// (`None` = none up to the repository root).
    tsconfig_nearest_cache: HashMap<String, Option<String>>,
    /// TS-ALIAS-RESOLUTION-1: repo-relative config path → the config as read (or why it could not
    /// be read). Each config is read once; membership is evaluated per file, never cached.
    tsconfig_project_cache: HashMap<String, Result<TsconfigFile, String>>,
    /// Directory → PackageDependencySet cache (for Rust via Cargo.toml).
    cargo_cache: HashMap<String, PackageDependencySet>,
    /// Java FILE directory → its Gradle declared set (DEPS-GRADLE-CATALOG-1A: keyed by the file's
    /// OWN directory only — the declared set is a function of that directory; a walked-up probe
    /// directory is never given another file's result). `pub(crate)`: the Gradle resolver body lives
    /// in `manifest_deps.rs` (guardrail — config.rs is not grown), and reaches this cache from there.
    pub(crate) gradle_cache: HashMap<String, PackageDependencySet>,
    /// DEPS-GRADLE-CATALOG-1A: the parsed Gradle settings files, build-script scopes, builds and
    /// per-project declared sets the Gradle resolver has read (each read once per path). Owned by
    /// `manifest_deps.rs`, where the resolver lives.
    pub(crate) gradle_reads: crate::manifest_deps::GradleReadCache,
    /// Directory → PackageDependencySet cache (for Python via pyproject.toml). `pub(crate)` for the
    /// same reason as `gradle_cache` (the pyproject resolver lives in `manifest_deps.rs`).
    pub(crate) pyproject_cache: HashMap<String, PackageDependencySet>,
    /// Manifests the deps resolvers encountered (§2.2 provenance) — parsed and present-but-failed.
    /// Populated as a side effect of `resolve_*_deps`; serialized into the diagnostics blob by
    /// `compose`. `pub(crate)`: the `record_*_manifest` recorders that mutate it live in
    /// `manifest_deps.rs` (guardrail — config.rs is not grown), same as `gradle_cache`.
    pub(crate) manifest_provenance: crate::manifest_deps::ManifestProvenanceCollector,
}

impl Default for RepoConfigContext {
    fn default() -> Self {
        Self::new()
    }
}

impl RepoConfigContext {
    /// Build config context by pre-scanning the repo root.
    /// The actual per-directory resolution is lazy (on first lookup).
    pub fn new() -> Self {
        Self {
            pkg_cache: HashMap::new(),
            tsconfig_cache: HashMap::new(),
            tsconfig_nearest_cache: HashMap::new(),
            tsconfig_project_cache: HashMap::new(),
            cargo_cache: HashMap::new(),
            gradle_cache: HashMap::new(),
            gradle_reads: crate::manifest_deps::GradleReadCache::default(),
            pyproject_cache: HashMap::new(),
            manifest_provenance: crate::manifest_deps::ManifestProvenanceCollector::default(),
        }
    }

    /// Resolve package dependencies for a file.
    /// Walks from file's directory upward to repo root.
    pub fn resolve_package_deps(
        &mut self,
        file_rel_path: &str,
        repo_root: &Path,
    ) -> PackageDependencySet {
        let empty = PackageDependencySet { names: vec![] };
        let dir = parent_dir(file_rel_path);

        // Check cache chain upward.
        let mut probe = dir.clone();
        loop {
            if let Some(cached) = self.pkg_cache.get(&probe) {
                // Backfill cache for unchecked dirs.
                let result = cached.clone();
                self.pkg_cache.insert(dir.clone(), result.clone());
                return result;
            }

            // Try reading package.json at this directory. TS behavior: if the file EXISTS, stop here
            // regardless of parse success (a broken leaf manifest must NOT inherit parent deps).
            let abs_dir = if probe.is_empty() {
                repo_root.to_path_buf()
            } else {
                repo_root.join(&probe)
            };
            let pkg_path = abs_dir.join("package.json");
            // STANDING HONESTY RULE (sweep): gate on the READ, not `.exists()` + `.ok()`. `NotFound`
            // is the only "truly absent → keep walking" case. A present-but-unreadable manifest (any
            // other io error) and a present-but-MALFORMED manifest (valid read but `extract` returns
            // `None` — for package.json `None` means the JSON did not parse, distinct from a valid
            // file with zero deps which returns `Some(empty)`) are BOTH recorded as FAILED manifests
            // (review-4 item 1), so query time renders unknown-with-reason instead of a fabricated
            // parsed zero-dep. The directory still OWNS the manifest either way — the walk stops here.
            match std::fs::read_to_string(&pkg_path) {
                Ok(content) => match extract_package_dependencies(&content) {
                    Some(deps) => {
                        self.record_parsed_manifest(&probe, "package.json", "npm");
                        self.pkg_cache.insert(probe.clone(), deps.clone());
                        self.pkg_cache.insert(dir.clone(), deps.clone());
                        return deps;
                    }
                    None => {
                        eprintln!(
                            "warning: package.json at {} malformed (not valid JSON object); \
                             declared deps unknown",
                            pkg_path.display(),
                        );
                        self.record_failed_manifest(
                            &probe,
                            "package.json",
                            "npm",
                            "malformed: not a valid JSON object".to_string(),
                        );
                        self.pkg_cache.insert(probe.clone(), empty.clone());
                        self.pkg_cache.insert(dir.clone(), empty.clone());
                        return empty;
                    }
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    eprintln!(
                        "warning: package.json at {} unreadable ({}); declared deps unknown",
                        pkg_path.display(),
                        e
                    );
                    self.record_failed_manifest(
                        &probe,
                        "package.json",
                        "npm",
                        format!("unreadable: {e}"),
                    );
                    self.pkg_cache.insert(probe.clone(), empty.clone());
                    self.pkg_cache.insert(dir.clone(), empty.clone());
                    return empty;
                }
            }

            if probe.is_empty() {
                break;
            }
            probe = parent_dir(&probe);
        }

        self.pkg_cache.insert(dir, empty.clone());
        empty
    }

    /// TS-ALIAS-RESOLUTION-1 (RG-REQ-006-L04; D-TSA-BOUNDED-SCOPE-1): the alias signal stored for
    /// a file. `entries` are HEAD's discovery result, unchanged (the nearest `tsconfig.json`'s own
    /// `paths` through its relative extends chain — the classifier's input); the mapping is the
    /// effective `paths` mapping of the ONE inspected project that covers the file, when exactly one
    /// covers it and has `paths` ([`Self::sole_inspected_covering_project_mapping`]). A config the reader
    /// cannot use leaves the file without a mapping and prints one warning naming the file and the
    /// config.
    pub fn resolve_tsconfig_aliases(
        &mut self,
        file_rel_path: &str,
        repo_root: &Path,
    ) -> StoredTsconfigAliases {
        let entries = self
            .resolve_nearest_tsconfig_entries(file_rel_path, repo_root)
            .entries;
        let sole_inspected_covering_project_mapping =
            match self.sole_inspected_covering_project_mapping(file_rel_path, repo_root) {
                Ok(mapping) => mapping,
                Err(why) => {
                    eprintln!(
                        "warning: {why}; no tsconfig paths mapping for {file_rel_path} \
                         (its tsconfig paths imports are not resolved)"
                    );
                    None
                }
            };
        StoredTsconfigAliases {
            entries,
            sole_inspected_covering_project_mapping,
        }
    }

    /// Resolve tsconfig aliases for a file.
    /// Walks from file's directory upward to repo root.
    fn resolve_nearest_tsconfig_entries(
        &mut self,
        file_rel_path: &str,
        repo_root: &Path,
    ) -> TsconfigAliases {
        let empty = TsconfigAliases { entries: vec![] };
        let dir = parent_dir(file_rel_path);

        let mut probe = dir.clone();
        loop {
            if let Some(cached) = self.tsconfig_cache.get(&probe) {
                let result = cached.clone();
                self.tsconfig_cache.insert(dir.clone(), result.clone());
                return result;
            }

            let abs_dir = if probe.is_empty() {
                repo_root.to_path_buf()
            } else {
                repo_root.join(&probe)
            };
            let tsconfig_path = abs_dir.join("tsconfig.json");
            // STANDING HONESTY RULE sweep (ruling 3 item 6): gate on the READ, not `.exists()`.
            // Aliases steer import RESOLUTION (which feeds classification), so a present-but-
            // unreadable tsconfig must not silently become "no aliases" (which would misresolve
            // aliased imports as external). NotFound → keep walking; any other error → warn + empty.
            match std::fs::read_to_string(&tsconfig_path) {
                Ok(_) => {
                    // Present + readable — parse (the reader re-reads and also follows `extends`).
                    // A readable-but-UNPARSEABLE tsconfig is not "no aliases": say so (review-6 #3
                    // — the same silent-default class as the unreadable branch below).
                    let aliases = match read_tsconfig_aliases_from_path(&tsconfig_path) {
                        Some(a) => a,
                        None => {
                            eprintln!(
                                "warning: tsconfig.json at {} did not parse; import aliases unknown",
                                tsconfig_path.display()
                            );
                            empty.clone()
                        }
                    };
                    self.tsconfig_cache.insert(probe.clone(), aliases.clone());
                    self.tsconfig_cache.insert(dir.clone(), aliases.clone());
                    return aliases;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    eprintln!(
                        "warning: tsconfig.json at {} unreadable ({}); import aliases unknown",
                        tsconfig_path.display(),
                        e
                    );
                    self.tsconfig_cache.insert(probe.clone(), empty.clone());
                    self.tsconfig_cache.insert(dir.clone(), empty.clone());
                    return empty;
                }
            }

            if probe.is_empty() {
                break;
            }
            probe = parent_dir(&probe);
        }

        self.tsconfig_cache.insert(dir, empty.clone());
        empty
    }

    /// Resolve Cargo dependencies for a Rust file.
    /// Walks from file's directory upward to repo root.
    /// Returns normalized dependency names (hyphen → underscore).
    pub fn resolve_cargo_deps(
        &mut self,
        file_rel_path: &str,
        repo_root: &Path,
    ) -> PackageDependencySet {
        let empty = PackageDependencySet { names: vec![] };
        let dir = parent_dir(file_rel_path);

        let mut probe = dir.clone();
        loop {
            if let Some(cached) = self.cargo_cache.get(&probe) {
                let result = cached.clone();
                self.cargo_cache.insert(dir.clone(), result.clone());
                return result;
            }

            let abs_dir = if probe.is_empty() {
                repo_root.to_path_buf()
            } else {
                repo_root.join(&probe)
            };
            let cargo_path = abs_dir.join("Cargo.toml");
            // STANDING HONESTY RULE (sweep): gate on the READ (NotFound → keep walking; any other
            // error → present-but-unreadable, so stop+warn+empty-with-reason+record), not `.exists()`
            // + `.ok()` which silently coerces an unreadable manifest into a clean measured-empty.
            match std::fs::read_to_string(&cargo_path) {
                Ok(content) => {
                    let deps =
                        extract_cargo_dependencies(&content).unwrap_or_else(|| empty.clone());
                    self.record_parsed_manifest(&probe, "Cargo.toml", "cargo");
                    self.cargo_cache.insert(probe.clone(), deps.clone());
                    self.cargo_cache.insert(dir.clone(), deps.clone());
                    return deps;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    eprintln!(
                        "warning: Cargo.toml at {} unreadable ({}); declared deps unknown",
                        cargo_path.display(),
                        e
                    );
                    // review-4 item 1: PRESENT but unreadable → FAILED record, not parsed zero-dep.
                    self.record_failed_manifest(
                        &probe,
                        "Cargo.toml",
                        "cargo",
                        format!("unreadable: {e}"),
                    );
                    self.cargo_cache.insert(probe.clone(), empty.clone());
                    self.cargo_cache.insert(dir.clone(), empty.clone());
                    return empty;
                }
            }

            if probe.is_empty() {
                break;
            }
            probe = parent_dir(&probe);
        }

        self.cargo_cache.insert(dir, empty.clone());
        empty
    }

    // `resolve_gradle_deps` (Java) and `resolve_pyproject_deps` (Python, DEPS-LIST-REWRITE-1 §2.2),
    // plus the `record_parsed_manifest` / `record_failed_manifest` provenance recorders, live in the
    // crate-private `manifest_deps` module (an `impl RepoConfigContext` block there) so this slice
    // does NOT grow config.rs — the operator's binding guardrail. Same struct, same fields; call
    // sites in the npm/cargo readers above are unchanged (methods resolve across the split impl).

    /// The manifests this context's deps resolvers encountered (§2.2) — parsed AND present-but-
    /// failed (review-4 item 1). Consumed by `compose` to serialize into the extraction-diagnostics
    /// blob before the Ready flip. `pub(crate)` (the element type is crate-private).
    pub(crate) fn manifest_records(&self) -> &[crate::manifest_deps::ManifestRecord] {
        self.manifest_provenance.records()
    }
}

/// Get the parent directory of a repo-relative path. `pub(crate)` so the relocated Gradle/pyproject
/// readers in `manifest_deps.rs` share the exact same nearest-manifest walk step.
pub(crate) fn parent_dir(rel_path: &str) -> String {
    match rel_path.rfind('/') {
        Some(pos) => rel_path[..pos].to_string(),
        None => String::new(), // Root directory.
    }
}

// ── Package.json reader ──────────────────────────────────────────

/// Extract dependency names from package.json content.
/// Reads dependencies, devDependencies, peerDependencies,
/// optionalDependencies. Returns sorted unique names.
///
/// Mirrors TS `extractPackageDependencies` from `package-json.ts:67`.
pub fn extract_package_dependencies(content: &str) -> Option<PackageDependencySet> {
    let parsed: serde_json::Value = serde_json::from_str(content).ok()?;
    let obj = parsed.as_object()?;

    let mut names = BTreeSet::new();
    for field in &[
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        if let Some(serde_json::Value::Object(deps)) = obj.get(*field) {
            for key in deps.keys() {
                names.insert(key.clone());
            }
        }
    }

    Some(PackageDependencySet {
        names: names.into_iter().collect(),
    })
}

// ── Cargo.toml reader ────────────────────────────────────────────

/// Extract dependency names from Cargo.toml content.
/// Reads [dependencies], [dev-dependencies], [build-dependencies].
/// Returns sorted unique names with hyphen normalization.
///
/// **Hyphen normalization:** Cargo allows both `foo-bar` and `foo_bar`
/// in dependency names, but Rust code uses underscores in `use` statements.
/// The classifier expects the underscore form for matching, so this
/// function normalizes hyphens to underscores in the returned set.
///
/// TOML parsing note: Uses basic line parsing, not a full TOML parser,
/// to avoid adding a TOML dependency. Handles:
///   - `name = "version"` inline deps
///   - `name = { version = "X" }` table deps
///   - `[dependencies.name]` sub-tables
///
/// Does NOT handle:
///   - Renamed deps (`foo = { package = "actual-name" }`)
///   - Workspace deps (`foo.workspace = true`)
///   - Target-specific deps (`[target.'cfg(...)'.dependencies]`)
///
/// These are edge cases for classification; the primary use is
/// identifying external crate names for import bucketing.
pub fn extract_cargo_dependencies(content: &str) -> Option<PackageDependencySet> {
    let mut names = BTreeSet::new();
    let mut current_section = "";

    for line in content.lines() {
        let line = line.trim();

        // Track section headers.
        if line.starts_with('[') && line.ends_with(']') {
            current_section = &line[1..line.len() - 1];
            // Handle sub-table syntax: [dependencies.foo] → dep name "foo".
            for prefix in &["dependencies.", "dev-dependencies.", "build-dependencies."] {
                if let Some(dep_name) = current_section.strip_prefix(prefix) {
                    // Normalize hyphen to underscore.
                    names.insert(dep_name.replace('-', "_"));
                }
            }
            continue;
        }

        // Only process lines in dependency sections.
        // Note: target-specific deps ([target.'cfg(...)'.dependencies]) are NOT
        // supported by this simple line parser.
        let is_dep_section = current_section == "dependencies"
            || current_section == "dev-dependencies"
            || current_section == "build-dependencies"
            || current_section.starts_with("dependencies.")
            || current_section.starts_with("dev-dependencies.")
            || current_section.starts_with("build-dependencies.");

        if !is_dep_section {
            continue;
        }

        // Skip sub-table lines like [dependencies.foo].
        if current_section.contains('.') {
            continue;
        }

        // Parse `name = "version"` or `name = { ... }`.
        if let Some(eq_pos) = line.find('=') {
            let key = line[..eq_pos].trim();
            // Skip empty keys or non-identifier keys.
            if key.is_empty() || key.contains(' ') || key.contains('[') {
                continue;
            }
            // Normalize hyphen to underscore.
            names.insert(key.replace('-', "_"));
        }
    }

    if names.is_empty() {
        return None;
    }

    Some(PackageDependencySet {
        names: names.into_iter().collect(),
    })
}

// ── build.gradle / build.gradle.kts reader (GRADLE-DEP-READER-1) ──

/// Declared dependency GROUP ids of ONE Gradle build script, by the Gradle scope that declares
/// them (DEPS-GRADLE-CATALOG-1A; RG-REQ-006-L13), plus the lines of the `dependencies` blocks the
/// reader skipped because it cannot attribute them statically (D-DGC-CONDITIONAL-1;
/// RG-REQ-002-L11). Every group list is sorted and unique. Which PROJECT each scope reaches is the
/// resolver's decision (`manifest_deps::resolve_gradle_deps`, through the build's settings file),
/// not this reader's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GradleScriptScopes {
    /// Groups of the `dependencies {` blocks at the script's TOP LEVEL — the script's own project.
    pub own: Vec<String>,
    /// Groups of the `dependencies {` blocks that are direct children of a top-level
    /// `allprojects {` block.
    pub allprojects: Vec<String>,
    /// Groups of the `dependencies {` blocks that are direct children of a top-level
    /// `subprojects {` block.
    pub subprojects: Vec<String>,
    /// Groups of the `dependencies {` blocks that are direct children of a top-level
    /// `project('<path>') {` block, keyed by the Gradle path AS WRITTEN (absolute `:a:b` or
    /// relative `b`). Only paths whose blocks carry ≥1 coordinate are present.
    pub projects: BTreeMap<String, Vec<String>>,
    /// 1-based lines of every COUNTED `dependencies` block, in line order: a `dependencies` head
    /// that is not DIRECT (bare under any other enclosing block, or receiver-qualified) and whose
    /// chain of enclosing heads holds no tooling head and no task/extension head. A count of skipped
    /// BLOCKS, not of declarations.
    pub undetermined_block_lines: Vec<u32>,
}

impl GradleScriptScopes {
    /// True iff the script declares no group in any scope and has no counted block.
    fn is_empty(&self) -> bool {
        self.own.is_empty()
            && self.allprojects.is_empty()
            && self.subprojects.is_empty()
            && self.projects.is_empty()
            && self.undetermined_block_lines.is_empty()
    }
}

/// The scope a DIRECT `dependencies` block declares for.
#[derive(Debug, Clone)]
enum DirectScope {
    Own,
    AllProjects,
    SubProjects,
    Project(String),
}

/// The group sets under construction, one per scope.
#[derive(Default)]
struct ScopeSets {
    own: BTreeSet<String>,
    allprojects: BTreeSet<String>,
    subprojects: BTreeSet<String>,
    projects: BTreeMap<String, BTreeSet<String>>,
}

impl ScopeSets {
    /// Mine one buffered in-block line segment into the set of `scope`, then clear the buffer.
    fn mine(&mut self, buffered: &mut String, scope: &DirectScope) {
        if buffered.is_empty() {
            return;
        }
        let set = match scope {
            DirectScope::Own => &mut self.own,
            DirectScope::AllProjects => &mut self.allprojects,
            DirectScope::SubProjects => &mut self.subprojects,
            DirectScope::Project(path) => self.projects.entry(path.clone()).or_default(),
        };
        extract_gradle_coordinates(buffered, set);
        buffered.clear();
    }
}

/// One open `{` of the brace walk: the trimmed text on its line before it (its HEAD) and whether
/// the word right before it is `dependencies` (bare or receiver-qualified).
struct GradleFrame {
    head: String,
    is_dependencies_head: bool,
}

/// A head that encloses the build tool's own classpath — never a project declaration (L13 excludes
/// the tooling classpath by name). Closed list.
fn is_tooling_head(head: &str) -> bool {
    matches!(head, "buildscript" | "pluginManagement")
}

/// A task or plugin-extension head — a `dependencies` block under it is a filter (shadow-jar
/// include/exclude), not a declaration. Closed list: `shadowJar`, `tasks.…`, `task <name>`,
/// `task(<…>)`.
fn is_task_head(head: &str) -> bool {
    head == "shadowJar"
        || head.starts_with("tasks.")
        || head.starts_with("task ")
        || head.starts_with("task(")
}

/// The Gradle path of a `project('<path>')` / `project("<path>")` scope head — `project`, `(`, one
/// quoted path, `)` and nothing else — kept as written. `from(project(':tools').jar)` is not one.
fn project_block_path(head: &str) -> Option<String> {
    let rest = head.strip_prefix("project")?.trim_start();
    let rest = rest.strip_prefix('(')?.trim_start();
    let quote = rest.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let rest = &rest[1..];
    let end = rest.find(quote)?;
    let path = &rest[..end];
    if path.is_empty() || path.contains(['\'', '"']) {
        return None;
    }
    let tail = rest[end + 1..].trim_start().strip_prefix(')')?;
    tail.trim().is_empty().then(|| path.to_string())
}

/// The scope of a `dependencies` block whose head is exactly `dependencies`, given the heads of
/// its enclosing blocks: DIRECT only at the script top level or as a direct child of a TOP-LEVEL
/// `allprojects` / `subprojects` / `project('<path>')` block; `None` otherwise.
fn direct_scope(head: &str, enclosing: &[GradleFrame]) -> Option<DirectScope> {
    if head != "dependencies" {
        return None;
    }
    match enclosing {
        [] => Some(DirectScope::Own),
        [only] => match only.head.as_str() {
            "allprojects" => Some(DirectScope::AllProjects),
            "subprojects" => Some(DirectScope::SubProjects),
            h => project_block_path(h).map(DirectScope::Project),
        },
        _ => None,
    }
}

/// Extract the declared dependency GROUP ids of a Gradle build script — `build.gradle` (Groovy
/// DSL) or `build.gradle.kts` (Kotlin DSL) — BY SCOPE (DEPS-GRADLE-CATALOG-1A; RG-REQ-006-L13).
///
/// ## What it captures, and why the GROUP ID
///
/// A Gradle coordinate is `group:artifact:version`
/// (`com.google.guava:guava:31.0`), but a Java `import` is a PACKAGE namespace
/// (`com.google.common.collect.ImmutableList`). The consumer that names an
/// unresolved Java reference — `resolve_declared_dependency` in
/// `repo-graph-classification` (ATTRIBUTION-1) — matches a declared name
/// against an import specifier by PACKAGE SEGMENT: a dotted declared name
/// matches when the import equals it or extends it on a `.` boundary
/// (`com.foo` matches `com.foo` and `com.foo.Bar`, never `com.foobar`), and the
/// LONGEST matching declared group wins. Only the **group id** can be such a
/// package prefix (the full coordinate carries a `:` no import contains; the
/// artifact id is a single token, never a package prefix). So the captured name
/// is the group id.
///
/// This matches Java imports where the group prefixes the package
/// (`org.springframework.boot` → `org.springframework.boot.autoconfigure.*`)
/// and DEGRADES HONESTLY where it does not: `com.google.guava`'s packages live
/// under `com.google.common.*`, so guava imports fall to the honest
/// "dependency not identified" bucket rather than being force-attributed.
///
/// ## Which blocks declare — only DIRECT blocks (RG-REQ-006-L13)
///
/// Comments (`//`, `/* */`) are stripped first. The cleaned source is walked in ONE character pass
/// that keeps a brace stack; each `{` records its HEAD (the trimmed text before it on its line,
/// after the last `{`, `}` or `;`) and whether the word right before it is `dependencies`. A
/// `dependencies {` block is DIRECT only when `dependencies` is its whole head AND it sits at the
/// script's top level (`own`) or directly inside a TOP-LEVEL `allprojects {` / `subprojects {` /
/// `project('<path>') {` block (those scopes). Everything inside a direct block — nested closures
/// included — is buffered per line and mined for coordinates exactly as before, in two forms:
///   - **string form** — `implementation 'g:a:v'` / `api("g:a:v")`: any quoted token shaped
///     `group:artifact[:version…]` contributes its group;
///   - **map form** — `group: 'g', name: 'a'` / `group = "g", name = "a"`: the `group` value is
///     captured ONLY when a `name` key is also present, so `exclude group: 'g', module: 'm'` is not.
///
/// The scan is verb-agnostic inside the block (every configuration verb is covered) and a
/// one-line block is mined (only the text between its braces is buffered).
///
/// Every OTHER `dependencies` block declares nothing — the index cannot establish that it applies
/// to any project:
///   - under a TOOLING head (`buildscript`, `pluginManagement`) or a TASK/EXTENSION head
///     (`shadowJar`, `tasks.…`, `task <name>`, `task(<…>)`): the build tool's classpath or a
///     shadow-jar filter — excluded SILENTLY (not a declaration site of any project);
///   - anywhere else — under a condition (`if`/`else`), a callback (`afterEvaluate`,
///     `plugins.withId(…)`, `gradle.projectsEvaluated`), control flow (`try`), a scope head below
///     the top level (`subprojects { project(':x') { … } }`), any unlisted head, or with a
///     receiver-qualified head (`subproject.dependencies {` — receivers are not resolved): COUNTED
///     in [`GradleScriptScopes::undetermined_block_lines`], so an unknown head is over-marked,
///     never silently lost (D-DGC-CONDITIONAL-1; `deps list` states the declared set may be
///     incomplete).
///
/// The cost of not declaring a counted block is a false NEGATIVE on the declared set — stated on
/// `deps list`, never a fabricated declaration.
///
/// Returns `None` when:
///   - a `dependencies` block (of any kind) is UNCLOSED at end of input — the block extents are
///     untrustworthy, so nothing is trusted (honest degradation);
///   - the script has neither a direct block carrying a coordinate nor a counted block (a
///     buildscript-only script, an empty or catalog-only script).
///
/// ## Known limitations (honest degradation, documented)
///   - Version catalogs (`implementation libs.guava` / `libraries.guava`),
///     `project(':core')` deps, and the `kotlin("stdlib")` helper carry no
///     literal coordinate on the line → not resolved (no fabrication; DEPS-GRADLE-CATALOG-1B).
///   - A head is read from its own line only: an Allman-style `{` on the next line has an empty
///     head, so a `dependencies` word on the line above it is not a block head (as before).
///   - Brace tracking is not string-aware: a `{`/`}` inside a coordinate's `${…}` version
///     interpolation is balanced and harmless, but a lone brace inside a string literal would
///     miscount. Coordinate strings never contain lone braces.
pub fn extract_gradle_dependencies(content: &str) -> Option<GradleScriptScopes> {
    let cleaned = strip_gradle_comments(content);
    let mut sets = ScopeSets::default();
    let mut undetermined_block_lines: Vec<u32> = Vec::new();

    let mut stack: Vec<GradleFrame> = Vec::new();
    // The open DIRECT block: its stack index and the scope it declares for. At most one is open
    // at a time — anything nested inside a direct block is its content.
    let mut direct: Option<(usize, DirectScope)> = None;
    // In-block characters of the open direct block seen so far on the current line, mined at each
    // line boundary and when the direct block closes.
    let mut in_block_line = String::new();
    // Word-boundary tracking to recognize a `dependencies` head (`last_word` is the completed word
    // immediately before the current position); reset at each newline (same-line heads only).
    let mut cur_word = String::new();
    let mut last_word = String::new();
    // The text since the last `{`, `}`, `;` or newline — the head of the next `{`.
    let mut head = String::new();
    let mut line: u32 = 1;

    for ch in cleaned.chars() {
        match ch {
            '\n' => {
                if let Some((_, scope)) = &direct {
                    sets.mine(&mut in_block_line, scope);
                }
                cur_word.clear();
                last_word.clear();
                head.clear();
                line += 1;
            }
            '{' => {
                if !cur_word.is_empty() {
                    last_word = std::mem::take(&mut cur_word);
                }
                let is_dependencies_head = last_word == "dependencies";
                let frame_head = head.trim().to_string();
                head.clear();
                if is_dependencies_head && direct.is_none() {
                    match direct_scope(&frame_head, &stack) {
                        Some(scope) => direct = Some((stack.len(), scope)),
                        None => {
                            let excluded = stack
                                .iter()
                                .any(|f| is_tooling_head(&f.head) || is_task_head(&f.head));
                            if !excluded {
                                undetermined_block_lines.push(line);
                            }
                        }
                    }
                }
                stack.push(GradleFrame {
                    head: frame_head,
                    is_dependencies_head,
                });
                last_word.clear();
            }
            '}' => {
                if !cur_word.is_empty() {
                    last_word = std::mem::take(&mut cur_word);
                }
                if stack.pop().is_some() {
                    if let Some((depth, scope)) = &direct {
                        if *depth == stack.len() {
                            sets.mine(&mut in_block_line, scope);
                            direct = None;
                        }
                    }
                }
                last_word.clear();
                head.clear();
            }
            c => {
                if c.is_ascii_alphanumeric() || c == '_' {
                    cur_word.push(c);
                } else if !cur_word.is_empty() {
                    last_word = std::mem::take(&mut cur_word);
                }
                if direct.is_some() {
                    in_block_line.push(c);
                }
                if c == ';' {
                    head.clear();
                } else {
                    head.push(c);
                }
            }
        }
    }
    // Flush a trailing (newline-less) final line.
    if let Some((_, scope)) = &direct {
        sets.mine(&mut in_block_line, scope);
    }

    // An unclosed `dependencies` block (still open at end of input) is malformed: its extent is
    // untrustworthy, so discard everything rather than trust coordinates from a guessed end.
    if stack.iter().any(|f| f.is_dependencies_head) {
        return None;
    }
    let scopes = GradleScriptScopes {
        own: sets.own.into_iter().collect(),
        allprojects: sets.allprojects.into_iter().collect(),
        subprojects: sets.subprojects.into_iter().collect(),
        projects: sets
            .projects
            .into_iter()
            .filter(|(_, groups)| !groups.is_empty())
            .map(|(path, groups)| (path, groups.into_iter().collect()))
            .collect(),
        undetermined_block_lines,
    };
    if scopes.is_empty() {
        return None;
    }
    Some(scopes)
}

/// Extract dependency group ids from one comment-free line inside a
/// `dependencies { … }` block, in both coordinate forms.
fn extract_gradle_coordinates(line: &str, names: &mut BTreeSet<String>) {
    // Map form: capture `group` only when a `name` key is present (a real
    // declaration), so `exclude group: 'g', module: 'm'` is skipped.
    if map_key_sep_index(line, "name").is_some() {
        if let Some(group) = map_string_value(line, "group") {
            if is_coordinate_segment(&group) {
                names.insert(group);
            }
        }
    }
    // String form: any quoted `group:artifact[:…]` token.
    for quoted in quoted_strings(line) {
        if let Some(group) = coordinate_group(&quoted) {
            names.insert(group);
        }
    }
}

/// The group of a coordinate string `group:artifact[:version[:classifier]]`, or
/// `None` if the token is not a coordinate (needs ≥2 colon-separated segments
/// whose group and artifact are valid coordinate segments). This rejects
/// `project(':core')` refs (`:core` → empty group), bare `exclude` args (no
/// colon), and URLs (`https://…` → artifact segment contains `/`).
fn coordinate_group(coord: &str) -> Option<String> {
    let mut parts = coord.split(':');
    let group = parts.next()?;
    let artifact = parts.next()?;
    if !is_coordinate_segment(group) || !is_coordinate_segment(artifact) {
        return None;
    }
    Some(group.to_string())
}

/// True iff `s` is a non-empty Maven coordinate segment: ASCII alphanumerics
/// plus `.`, `-`, `_`. Excludes whitespace, `/`, `$`, `{` — so an interpolated
/// version (`${v}`) or a URL never passes as a group/artifact.
fn is_coordinate_segment(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

/// All single- and double-quoted string contents on a line (quote-type agnostic;
/// escapes not interpreted — coordinates never contain escaped quotes).
fn quoted_strings(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\'' || c == b'"' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && bytes[j] != c {
                j += 1;
            }
            // Quote bytes are ASCII, so start..min(j,len) is a char boundary.
            out.push(line[start..j.min(bytes.len())].to_string());
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// Byte index just past the `:` or `=` separator of map key `key` on `line`
/// (Groovy `key: …` / Kotlin `key = …`), or `None`. `key` must be a whole word
/// (word boundaries on both sides), so `name` does not match `moduleName` and
/// `group` does not match `groupId`.
fn map_key_sep_index(line: &str, key: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let klen = key.len();
    let mut from = 0;
    while let Some(rel) = line[from..].find(key) {
        let idx = from + rel;
        let before_ok = idx == 0 || !is_word_byte(bytes[idx - 1]);
        let after = idx + klen;
        let after_ok = after >= bytes.len() || !is_word_byte(bytes[after]);
        if before_ok && after_ok {
            let mut j = after;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if j < bytes.len() && (bytes[j] == b':' || bytes[j] == b'=') {
                return Some(j + 1);
            }
        }
        from = idx + klen;
    }
    None
}

/// The quoted value of map key `key` on `line` (e.g. `group: 'com.google.guava'`
/// → `com.google.guava`), or `None` if the key is absent or its value is not a
/// string literal.
fn map_string_value(line: &str, key: &str) -> Option<String> {
    let sep = map_key_sep_index(line, key)?;
    let bytes = line.as_bytes();
    let mut j = sep;
    while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
        j += 1;
    }
    if j < bytes.len() && (bytes[j] == b'\'' || bytes[j] == b'"') {
        let quote = bytes[j];
        let vstart = j + 1;
        let mut k = vstart;
        while k < bytes.len() && bytes[k] != quote {
            k += 1;
        }
        return Some(line[vstart..k.min(bytes.len())].to_string());
    }
    None
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Strip `//` line comments and `/* */` block comments from Gradle DSL source,
/// respecting single- and double-quoted strings (a `//` inside a string is not
/// a comment). Newlines are preserved so line structure survives. Triple-quoted
/// GStrings are not special-cased (not used for coordinates). Sibling of
/// [`strip_json_comments`], generalized to Groovy/Kotlin single-quote strings.
fn strip_gradle_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut quote: Option<char> = None;
    let mut escape = false;

    while let Some(ch) = chars.next() {
        if let Some(q) = quote {
            out.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                out.push(ch);
            }
            '/' => match chars.peek() {
                Some('/') => {
                    for c in chars.by_ref() {
                        if c == '\n' {
                            out.push('\n');
                            break;
                        }
                    }
                }
                Some('*') => {
                    chars.next(); // consume '*'
                    let mut prev = '\0';
                    for c in chars.by_ref() {
                        if c == '\n' {
                            out.push('\n');
                        }
                        if prev == '*' && c == '/' {
                            break;
                        }
                        prev = c;
                    }
                }
                _ => out.push(ch),
            },
            _ => out.push(ch),
        }
    }
    out
}

// ── Tsconfig.json reader ─────────────────────────────────────────

const MAX_EXTENDS_DEPTH: usize = 10;

/// Strip JSONC comments (// line and /* block */) from source.
///
/// **Locked divergence from TS:** The TS reader uses a conservative
/// regex that does not distinguish comment markers inside string
/// values. This Rust scanner correctly handles strings, so it is a
/// strict superset: any input that parses under TS also parses here,
/// but inputs with `//` or `/*` inside string values parse correctly
/// in Rust and may break in TS. This is accepted as a safe
/// improvement — it cannot produce fewer aliases than TS, only more
/// (and only for pathological inputs with comment syntax in strings).
fn strip_json_comments(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut in_string = false;
    let mut escape_next = false;

    while let Some(ch) = chars.next() {
        if escape_next {
            result.push(ch);
            escape_next = false;
            continue;
        }
        if in_string {
            if ch == '\\' {
                escape_next = true;
            } else if ch == '"' {
                in_string = false;
            }
            result.push(ch);
            continue;
        }
        if ch == '"' {
            in_string = true;
            result.push(ch);
            continue;
        }
        if ch == '/' {
            match chars.peek() {
                Some('/') => {
                    // Line comment — skip to end of line.
                    for c in chars.by_ref() {
                        if c == '\n' {
                            result.push('\n');
                            break;
                        }
                    }
                    continue;
                }
                Some('*') => {
                    // Block comment — skip to */.
                    chars.next(); // consume *
                    let mut prev = ' ';
                    for c in chars.by_ref() {
                        if prev == '*' && c == '/' {
                            break;
                        }
                        prev = c;
                    }
                    continue;
                }
                _ => {}
            }
        }
        result.push(ch);
    }
    result
}

/// Read tsconfig.json at `path`, following extends chains.
/// Returns the effective TsconfigAliases, or None if file missing/unparseable.
pub fn read_tsconfig_aliases_from_path(path: &Path) -> Option<TsconfigAliases> {
    let empty = TsconfigAliases { entries: vec![] };
    let mut visited = std::collections::HashSet::new();
    let mut current = path.to_path_buf();

    for depth in 0..MAX_EXTENDS_DEPTH {
        let canonical = current.canonicalize().unwrap_or_else(|_| current.clone());
        if visited.contains(&canonical) {
            break; // Circular.
        }
        visited.insert(canonical.clone());

        let raw = match std::fs::read_to_string(&current) {
            Ok(c) => c,
            Err(_) => {
                return if depth == 0 { None } else { Some(empty) };
            }
        };

        let stripped = strip_json_comments(&raw);
        let parsed: serde_json::Value = match serde_json::from_str(&stripped) {
            Ok(v) => v,
            Err(_) => {
                return if depth == 0 { None } else { Some(empty) };
            }
        };

        // Check for compilerOptions.paths.
        if let Some(paths) = parsed
            .get("compilerOptions")
            .and_then(|co| co.get("paths"))
            .and_then(|p| p.as_object())
        {
            let entries: Vec<TsconfigAliasEntry> = paths
                .iter()
                .map(|(pattern, subs)| {
                    let substitutions = subs
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|s| s.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    TsconfigAliasEntry {
                        pattern: pattern.clone(),
                        substitutions,
                    }
                })
                .collect();
            return Some(TsconfigAliases { entries });
        }

        // Follow extends.
        let extends = match parsed.get("extends").and_then(|e| e.as_str()) {
            Some(e) => e.to_string(),
            None => return Some(empty),
        };

        // Only follow relative extends paths.
        if !extends.starts_with('.') && !extends.starts_with('/') {
            return Some(empty);
        }

        let parent_dir_path = current.parent().unwrap_or(Path::new(""));
        let mut next = parent_dir_path.join(&extends);
        if !next.extension().map(|e| e == "json").unwrap_or(false) {
            next.set_extension("json");
        }
        current = next;
    }

    Some(empty)
}

// ── Tsconfig project membership (TS-ALIAS-RESOLUTION-1) ──────────
//
// D-TSA-CONFIG-APPLICABILITY-1 as corrected by D-TSA-BOUNDED-SCOPE-1: a tsconfig project's `paths`
// apply to a file only when the project's MEMBERSHIP covers the file. The inspected projects are the
// nearest `tsconfig.json` and the targets of its own `references`, one level deep; each project's
// fields are its own, else inherited through its RELATIVE `extends` chain (a string or an array, the
// later entry winning; the child over every config it extends). The reader's scope is a stated
// limit: a referenced project's own `references`, a package `extends` and a tsconfig elsewhere in
// the repository are not read. FAIL CLOSED: a config that cannot be read or parsed, or a malformed
// value of a field that decides membership or target selection, leaves the file with no mapping; a
// default applies only when a field is ABSENT. HEAD's lenient `entries` reader
// (`read_tsconfig_aliases_from_path`) is separate and unchanged; it feeds only the classifier.

/// TypeScript's default `exclude` when a project declares none (relative to the project directory).
const TSCONFIG_DEFAULT_EXCLUDE: [&str; 3] = ["node_modules", "bower_components", "jspm_packages"];

/// The JavaScript extensions a project covers only with `allowJs: true`.
const TSCONFIG_JS_EXTENSIONS: [&str; 4] = [".js", ".jsx", ".mjs", ".cjs"];

/// A value declared by a config, with the repo-relative directory of the config that declares it
/// (the directory its relative paths resolve against).
#[derive(Debug, Clone)]
struct Declared<T> {
    value: T,
    dir: String,
}

/// The fields of one tsconfig the membership reader reads, each `None` when absent. As read from
/// one file, or effective after its extends chain.
#[derive(Debug, Clone, Default)]
struct TsconfigFields {
    paths: Option<Declared<Vec<TsconfigAliasEntry>>>,
    base_url: Option<Declared<String>>,
    allow_js: Option<bool>,
    out_dir: Option<Declared<String>>,
    files: Option<Declared<Vec<String>>>,
    include: Option<Declared<Vec<String>>>,
    exclude: Option<Declared<Vec<String>>>,
}

impl TsconfigFields {
    /// `self` overridden, field by field, by every field `over` declares (TypeScript's inheritance:
    /// `paths` is replaced as a whole, never merged).
    fn overridden_by(mut self, over: TsconfigFields) -> TsconfigFields {
        if over.paths.is_some() {
            self.paths = over.paths;
        }
        if over.base_url.is_some() {
            self.base_url = over.base_url;
        }
        if over.allow_js.is_some() {
            self.allow_js = over.allow_js;
        }
        if over.out_dir.is_some() {
            self.out_dir = over.out_dir;
        }
        if over.files.is_some() {
            self.files = over.files;
        }
        if over.include.is_some() {
            self.include = over.include;
        }
        if over.exclude.is_some() {
            self.exclude = over.exclude;
        }
        self
    }
}

/// One tsconfig file as read: its own fields, its relative `extends` targets (repo-relative, in
/// declaration order) and its `references` targets (repo-relative config paths, in order).
#[derive(Debug, Clone)]
struct TsconfigFile {
    own: TsconfigFields,
    extends: Vec<String>,
    references: Vec<String>,
}

impl RepoConfigContext {
    /// TS-ALIAS-RESOLUTION-1 (D-TSA-BOUNDED-SCOPE-1): the effective `paths` mapping of the ONE
    /// inspected tsconfig project whose membership covers `file_rel_path` — `Ok(None)` when no
    /// inspected project, or two or more (whatever their `paths`), cover the file, or when the one
    /// inspected covering project has no effective `paths`; never a pick, a merge or a union. `Err` names the
    /// config the reader could not use (fail closed: the file gets no mapping).
    fn sole_inspected_covering_project_mapping(
        &mut self,
        file_rel_path: &str,
        repo_root: &Path,
    ) -> Result<Option<TsconfigAliasMapping>, String> {
        let Some(nearest) = self.nearest_tsconfig_path(&parent_dir(file_rel_path), repo_root)
        else {
            return Ok(None);
        };
        let mut projects = vec![nearest.clone()];
        for reference in self.tsconfig_file(&nearest, repo_root)?.references {
            if !projects.contains(&reference) {
                projects.push(reference);
            }
        }
        let mut effective = Vec::with_capacity(projects.len());
        for project in &projects {
            effective.push((
                parent_dir(project),
                self.effective_tsconfig(project, repo_root, &mut Vec::new())?,
            ));
        }
        // No inspected project declares `paths`: no mapping, whatever the membership.
        if effective.iter().all(|(_, fields)| fields.paths.is_none()) {
            return Ok(None);
        }
        let mut covering = Vec::new();
        for (dir, fields) in &effective {
            if tsconfig_project_covers(fields, dir, file_rel_path)? {
                covering.push(fields);
            }
        }
        let [sole] = covering.as_slice() else {
            return Ok(None);
        };
        let Some(paths) = &sole.paths else {
            return Ok(None);
        };
        let (anchor_dir, base_url) = match &sole.base_url {
            Some(base) => (base.dir.clone(), base.value.clone()),
            None => (paths.dir.clone(), ".".to_string()),
        };
        let mut entries = paths.value.clone();
        entries.sort_by(|a, b| a.pattern.cmp(&b.pattern));
        Ok(Some(TsconfigAliasMapping {
            anchor_dir,
            base_url,
            entries,
        }))
    }

    /// The repo-relative path of the nearest `tsconfig.json` at or above `dir` (the walk HEAD's
    /// entries reader takes: the first directory whose `tsconfig.json` read is not `NotFound`).
    fn nearest_tsconfig_path(&mut self, dir: &str, repo_root: &Path) -> Option<String> {
        if let Some(cached) = self.tsconfig_nearest_cache.get(dir) {
            return cached.clone();
        }
        let candidate = join_config_dir(dir, "tsconfig.json");
        let found = match std::fs::metadata(repo_root.join(&candidate)) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if dir.is_empty() {
                    None
                } else {
                    self.nearest_tsconfig_path(&parent_dir(dir), repo_root)
                }
            }
            _ => Some(candidate),
        };
        self.tsconfig_nearest_cache
            .insert(dir.to_string(), found.clone());
        found
    }

    /// One config as read, cached by path. `Err` says what made it unusable.
    fn tsconfig_file(&mut self, rel: &str, repo_root: &Path) -> Result<TsconfigFile, String> {
        if let Some(cached) = self.tsconfig_project_cache.get(rel) {
            return cached.clone();
        }
        let read = read_tsconfig_file(rel, repo_root);
        self.tsconfig_project_cache
            .insert(rel.to_string(), read.clone());
        read
    }

    /// The effective fields of the config at `rel` after its relative extends chain: each relative
    /// `extends` target in order (a later one over an earlier one), then the config's own fields.
    fn effective_tsconfig(
        &mut self,
        rel: &str,
        repo_root: &Path,
        chain: &mut Vec<String>,
    ) -> Result<TsconfigFields, String> {
        if chain.iter().any(|c| c == rel) {
            return Err(format!("tsconfig {rel} extends itself through its chain"));
        }
        if chain.len() >= MAX_EXTENDS_DEPTH {
            return Err(format!(
                "tsconfig {rel} is more than {MAX_EXTENDS_DEPTH} extends deep"
            ));
        }
        let file = self.tsconfig_file(rel, repo_root)?;
        chain.push(rel.to_string());
        let mut effective = TsconfigFields::default();
        for base in &file.extends {
            effective = effective.overridden_by(self.effective_tsconfig(base, repo_root, chain)?);
        }
        chain.pop();
        Ok(effective.overridden_by(file.own))
    }
}

/// Read and validate one tsconfig (repo-relative `rel`). JSONC: comments and trailing commas are
/// accepted, as TypeScript accepts them. Every field the membership reader reads is validated; a
/// default is never applied in place of a malformed value.
fn read_tsconfig_file(rel: &str, repo_root: &Path) -> Result<TsconfigFile, String> {
    let raw = std::fs::read_to_string(repo_root.join(rel))
        .map_err(|e| format!("tsconfig {rel} unreadable ({e})"))?;
    let parsed: serde_json::Value =
        serde_json::from_str(&strip_json_trailing_commas(&strip_json_comments(&raw)))
            .map_err(|e| format!("tsconfig {rel} did not parse ({e})"))?;
    let obj = parsed
        .as_object()
        .ok_or_else(|| format!("tsconfig {rel} is not a JSON object"))?;
    let dir = parent_dir(rel);
    let malformed = |field: &str, what: &str| format!("tsconfig {rel}: {field} {what}");
    let string_list = |field: &str, value: &serde_json::Value| -> Result<Vec<String>, String> {
        value
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .map(|i| i.as_str().map(str::to_string))
                    .collect::<Option<Vec<String>>>()
            })
            .ok_or_else(|| malformed(field, "is not a list of strings"))
    };
    let declared_list = |field: &str| -> Result<Option<Declared<Vec<String>>>, String> {
        obj.get(field)
            .map(|v| {
                string_list(field, v).map(|value| Declared {
                    value,
                    dir: dir.clone(),
                })
            })
            .transpose()
    };

    let extends_raw: Vec<String> = match obj.get("extends") {
        None => Vec::new(),
        Some(serde_json::Value::String(e)) => vec![e.clone()],
        Some(v) => string_list("extends", v)
            .map_err(|_| malformed("extends", "is not a string or a list of strings"))?,
    };
    // Only relative `extends` are followed (a package `extends` stays unread — a stated limit).
    let mut extends = Vec::new();
    for e in extends_raw.iter().filter(|e| e.starts_with('.')) {
        let target = join_repo_relative(&dir, e)
            .ok_or_else(|| malformed("extends", &format!("{e} leaves the repository")))?;
        extends.push(if target.ends_with(".json") {
            target
        } else {
            format!("{target}.json")
        });
    }

    let references = match obj.get("references") {
        None => Vec::new(),
        Some(serde_json::Value::Array(items)) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                let path = item
                    .get("path")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        malformed("references", "holds an entry without a string path")
                    })?;
                let target = join_repo_relative(&dir, path).ok_or_else(|| {
                    malformed("references", &format!("{path} leaves the repository"))
                })?;
                out.push(if target.ends_with(".json") {
                    target
                } else {
                    join_config_dir(&target, "tsconfig.json")
                });
            }
            out
        }
        Some(_) => return Err(malformed("references", "is not a list")),
    };

    let empty = serde_json::Map::new();
    let compiler_options = match obj.get("compilerOptions") {
        None => &empty,
        Some(serde_json::Value::Object(co)) => co,
        Some(_) => return Err(malformed("compilerOptions", "is not an object")),
    };
    let paths = match compiler_options.get("paths") {
        None => None,
        Some(serde_json::Value::Object(map)) => {
            let mut entries = Vec::with_capacity(map.len());
            for (pattern, subs) in map {
                let substitutions = string_list("paths", subs).map_err(|_| {
                    malformed("paths", &format!("{pattern} is not a list of strings"))
                })?;
                entries.push(TsconfigAliasEntry {
                    pattern: pattern.clone(),
                    substitutions,
                });
            }
            Some(Declared {
                value: entries,
                dir: dir.clone(),
            })
        }
        Some(_) => return Err(malformed("paths", "is not an object")),
    };
    let declared_string = |field: &str| -> Result<Option<Declared<String>>, String> {
        match compiler_options.get(field) {
            None => Ok(None),
            Some(serde_json::Value::String(v)) => Ok(Some(Declared {
                value: v.clone(),
                dir: dir.clone(),
            })),
            Some(_) => Err(malformed(field, "is not a string")),
        }
    };
    let base_url = declared_string("baseUrl")?;
    let out_dir = declared_string("outDir")?;
    let allow_js = match compiler_options.get("allowJs") {
        None => None,
        Some(serde_json::Value::Bool(b)) => Some(*b),
        Some(_) => return Err(malformed("allowJs", "is not a boolean")),
    };
    Ok(TsconfigFile {
        own: TsconfigFields {
            paths,
            base_url,
            allow_js,
            out_dir,
            files: declared_list("files")?,
            include: declared_list("include")?,
            exclude: declared_list("exclude")?,
        },
        extends,
        references,
    })
}

/// Does the project (effective `fields`, its config in `project_dir`) cover `file`? A JavaScript
/// file needs `allowJs: true`; a file under `outDir` is never a member; then a `files` entry names
/// the file, or `include` (default: everything under the project directory, unless `files` is set
/// without `include`) matches it and `exclude` (default: TypeScript's) does not — `exclude` narrows
/// `include` only, never `files`.
fn tsconfig_project_covers(
    fields: &TsconfigFields,
    project_dir: &str,
    file: &str,
) -> Result<bool, String> {
    if TSCONFIG_JS_EXTENSIONS.iter().any(|ext| file.ends_with(ext)) && fields.allow_js != Some(true)
    {
        return Ok(false);
    }
    if let Some(out_dir) = &fields.out_dir {
        if tsconfig_entry_matches(&out_dir.value, &out_dir.dir, file, false)? {
            return Ok(false);
        }
    }
    if let Some(files) = &fields.files {
        for entry in &files.value {
            if join_repo_relative(&files.dir, entry).as_deref() == Some(file) {
                return Ok(true);
            }
        }
        if fields.include.is_none() {
            return Ok(false);
        }
    }
    let included = match &fields.include {
        Some(include) => any_tsconfig_entry_matches(&include.value, &include.dir, file)?,
        // Neither `files` nor `include`: everything under the project directory.
        None => project_dir.is_empty() || file.starts_with(&format!("{project_dir}/")),
    };
    if !included {
        return Ok(false);
    }
    let excluded = match &fields.exclude {
        Some(exclude) => any_tsconfig_entry_matches(&exclude.value, &exclude.dir, file)?,
        None => {
            let defaults: Vec<String> = TSCONFIG_DEFAULT_EXCLUDE.map(str::to_string).to_vec();
            any_tsconfig_entry_matches(&defaults, project_dir, file)?
        }
    };
    Ok(!excluded)
}

fn any_tsconfig_entry_matches(entries: &[String], dir: &str, file: &str) -> Result<bool, String> {
    for entry in entries {
        if tsconfig_entry_matches(entry, dir, file, true)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// One `include`/`exclude`/`outDir` entry, resolved against `dir`: an entry without a wildcard
/// names a file or a directory and everything under it; a wildcard entry (`*`, `?`, `**`; only
/// when `wildcards` is allowed) is matched against the whole path with `glob::Pattern`, `*` never
/// crossing `/`.
fn tsconfig_entry_matches(
    entry: &str,
    dir: &str,
    file: &str,
    wildcards: bool,
) -> Result<bool, String> {
    let Some(joined) = join_repo_relative(dir, entry) else {
        return Ok(false); // An entry outside the repository names no indexed file.
    };
    if wildcards && (entry.contains('*') || entry.contains('?')) {
        let pattern = glob::Pattern::new(&joined).map_err(|e| {
            format!("tsconfig pattern {entry} in {dir} is not a valid pattern ({e})")
        })?;
        let options = glob::MatchOptions {
            case_sensitive: true,
            require_literal_separator: true,
            require_literal_leading_dot: false,
        };
        return Ok(pattern.matches_with(file, options));
    }
    Ok(joined.is_empty() || file == joined || file.starts_with(&format!("{joined}/")))
}

/// `dir` joined with the relative path `rel` (`.` and `..` resolved, `/`-separated, no trailing
/// slash; `""` = the repository root). `None` when `rel` is absolute or a `..` leaves the
/// repository.
fn join_repo_relative(dir: &str, rel: &str) -> Option<String> {
    if rel.starts_with('/') {
        return None;
    }
    let mut parts: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

/// `<dir>/<name>`, or `<name>` at the repository root.
fn join_config_dir(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// Remove a JSON trailing comma (a `,` followed only by whitespace before `}` or `]`) outside
/// strings — the JSONC form TypeScript accepts in a tsconfig. Run after [`strip_json_comments`].
fn strip_json_trailing_commas(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut in_string = false;
    let mut escape_next = false;
    for (i, &ch) in chars.iter().enumerate() {
        if in_string {
            out.push(ch);
            if escape_next {
                escape_next = false;
            } else if ch == '\\' {
                escape_next = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
        } else if ch == ',' {
            let next = chars[i + 1..].iter().find(|c| !c.is_whitespace());
            if matches!(next, Some('}') | Some(']')) {
                continue;
            }
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // ── extract_package_dependencies ─────────────────────────

    #[test]
    fn extracts_all_dep_fields() {
        let content = r#"{
			"dependencies": {"express": "^4.18.0"},
			"devDependencies": {"vitest": "^1.0.0"},
			"peerDependencies": {"react": "^18.0.0"},
			"optionalDependencies": {"fsevents": "^2.3.0"}
		}"#;
        let deps = extract_package_dependencies(content).unwrap();
        assert_eq!(deps.names, vec!["express", "fsevents", "react", "vitest"]);
    }

    #[test]
    fn returns_sorted_unique_names() {
        let content = r#"{
			"dependencies": {"b-pkg": "1", "a-pkg": "2"},
			"devDependencies": {"a-pkg": "3"}
		}"#;
        let deps = extract_package_dependencies(content).unwrap();
        assert_eq!(deps.names, vec!["a-pkg", "b-pkg"]);
    }

    #[test]
    fn returns_none_on_invalid_json() {
        assert!(extract_package_dependencies("{invalid").is_none());
    }

    // ── strip_json_comments ──────────────────────────────────

    #[test]
    fn strips_line_comments() {
        let input = "{\n  // comment\n  \"key\": 1\n}";
        let stripped = strip_json_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(parsed["key"], 1);
    }

    #[test]
    fn strips_block_comments() {
        let input = "{ /* block */ \"key\": 1 }";
        let stripped = strip_json_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(parsed["key"], 1);
    }

    // ── read_tsconfig_aliases_from_path ───────────────────────

    #[test]
    fn reads_paths_from_tsconfig() {
        let dir = tempfile::tempdir().unwrap();
        let tsconfig = dir.path().join("tsconfig.json");
        fs::write(
            &tsconfig,
            r#"{
			"compilerOptions": {
				"paths": {
					"@/*": ["./src/*"],
					"@lib/*": ["./lib/*"]
				}
			}
		}"#,
        )
        .unwrap();

        let aliases = read_tsconfig_aliases_from_path(&tsconfig).unwrap();
        assert_eq!(aliases.entries.len(), 2);
        let at = aliases.entries.iter().find(|e| e.pattern == "@/*").unwrap();
        assert_eq!(at.substitutions, vec!["./src/*"]);
    }

    #[test]
    fn follows_extends_chain() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("base.json");
        fs::write(
            &base,
            r#"{
			"compilerOptions": {
				"paths": { "@/*": ["./src/*"] }
			}
		}"#,
        )
        .unwrap();

        let child = dir.path().join("tsconfig.json");
        fs::write(&child, r#"{ "extends": "./base.json" }"#).unwrap();

        let aliases = read_tsconfig_aliases_from_path(&child).unwrap();
        assert_eq!(aliases.entries.len(), 1);
        assert_eq!(aliases.entries[0].pattern, "@/*");
    }

    #[test]
    fn returns_none_for_missing_file() {
        let result = read_tsconfig_aliases_from_path(Path::new("/nonexistent/tsconfig.json"));
        assert!(result.is_none());
    }

    #[test]
    fn handles_jsonc_comments_in_tsconfig() {
        let dir = tempfile::tempdir().unwrap();
        let tsconfig = dir.path().join("tsconfig.json");
        fs::write(
            &tsconfig,
            r#"{
			// This is a comment
			"compilerOptions": {
				/* block comment */
				"paths": { "@/*": ["./src/*"] }
			}
		}"#,
        )
        .unwrap();

        let aliases = read_tsconfig_aliases_from_path(&tsconfig).unwrap();
        assert_eq!(aliases.entries.len(), 1);
    }

    // ── RepoConfigContext ────────────────────────────────────

    #[test]
    fn nearest_ancestor_package_json() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Root package.json.
        fs::write(
            root.join("package.json"),
            r#"{"dependencies":{"express":"1"}}"#,
        )
        .unwrap();
        // Nested package.json.
        fs::create_dir_all(root.join("packages/web")).unwrap();
        fs::write(
            root.join("packages/web/package.json"),
            r#"{"dependencies":{"react":"18"}}"#,
        )
        .unwrap();

        let mut ctx = RepoConfigContext::new();

        // File in root → gets root deps.
        let root_deps = ctx.resolve_package_deps("src/index.ts", root);
        assert_eq!(root_deps.names, vec!["express"]);

        // File in packages/web → gets nested deps.
        let web_deps = ctx.resolve_package_deps("packages/web/src/App.tsx", root);
        assert_eq!(web_deps.names, vec!["react"]);
    }

    #[test]
    fn malformed_package_json_stops_walk_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Root has valid deps.
        fs::write(
            root.join("package.json"),
            r#"{"dependencies":{"express":"1"}}"#,
        )
        .unwrap();
        // Nested has malformed package.json.
        fs::create_dir_all(root.join("packages/broken")).unwrap();
        fs::write(root.join("packages/broken/package.json"), "{invalid json}").unwrap();

        let mut ctx = RepoConfigContext::new();
        // File under broken → should get empty deps (malformed stops walk),
        // NOT inherit root's "express".
        let deps = ctx.resolve_package_deps("packages/broken/src/index.ts", root);
        assert!(
            deps.names.is_empty(),
            "malformed package.json should stop walk with empty deps, got {:?}",
            deps.names
        );
    }

    #[test]
    fn nearest_ancestor_tsconfig() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::write(
            root.join("tsconfig.json"),
            r#"{
			"compilerOptions": { "paths": { "@/*": ["./src/*"] } }
		}"#,
        )
        .unwrap();

        let mut ctx = RepoConfigContext::new();
        let aliases = ctx.resolve_tsconfig_aliases("src/index.ts", root);
        assert_eq!(aliases.entries.len(), 1);
        assert_eq!(aliases.entries[0].pattern, "@/*");
    }

    // ── Tsconfig project membership (TS-ALIAS-RESOLUTION-1) ─────

    /// (case name, the files written, the config the warning must name).
    type MalformedConfigCase<'a> = (&'a str, Vec<(&'a str, String)>, &'a str);
    /// (the files written, the importing file, its nearest tsconfig.json).
    type TsconfigLayout<'a> = (Vec<(&'a str, &'a str)>, &'a str, &'a str);

    /// Write `(path, content)` files under a fresh temp repository.
    fn ts_repo(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (rel, content) in files {
            let path = dir.path().join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        dir
    }

    /// The stored mapping of `file` (a fresh context, so nothing is cached across calls).
    fn mapping_of(root: &Path, file: &str) -> Option<TsconfigAliasMapping> {
        RepoConfigContext::new()
            .resolve_tsconfig_aliases(file, root)
            .sole_inspected_covering_project_mapping
    }

    /// The reader's own answer: `Err` is the warning's reason (the file then gets no mapping).
    fn reader_of(root: &Path, file: &str) -> Result<Option<TsconfigAliasMapping>, String> {
        RepoConfigContext::new().sole_inspected_covering_project_mapping(file, root)
    }

    fn alias(pattern: &str, subs: &[&str]) -> TsconfigAliasEntry {
        TsconfigAliasEntry {
            pattern: pattern.into(),
            substitutions: subs.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn mapping(anchor: &str, base: &str, entries: Vec<TsconfigAliasEntry>) -> TsconfigAliasMapping {
        TsconfigAliasMapping {
            anchor_dir: anchor.into(),
            base_url: base.into(),
            entries,
        }
    }

    const AMODX_ROOT: &str = r#"{
  "files": [],
  "references": [ { "path": "./tsconfig.app.json" }, { "path": "./tsconfig.node.json" } ],
  "compilerOptions": { "baseUrl": ".", "paths": { "@/*": ["./root/*"] } }
}"#;
    const AMODX_APP: &str = r#"{
  "compilerOptions": {
    /* Bundler mode */
    "baseUrl": ".",
    "paths": { "@/*": ["./src/*"], },
  },
  "include": ["src"],
  "exclude": ["src/**/*.test.ts"],
}"#;
    const NODE_PROJECT: &str = r#"{ "include": ["vite.config.ts"] }"#;
    const HEX_ROOT: &str = r#"{ "files": [], "references": [ { "path": "./tsconfig.app.json" }, { "path": "./tsconfig.node.json" } ] }"#;

    #[test]
    fn tsconfig_alias_mapping_carries_its_anchor_directory_and_base_url() {
        // baseUrl declared beside `paths`: the anchor is that config's directory.
        let repo = ts_repo(&[(
            "pkg/tsconfig.json",
            r#"{ "compilerOptions": { "baseUrl": "./src", "paths": { "@/*": ["./*"] } } }"#,
        )]);
        assert_eq!(
            mapping_of(repo.path(), "pkg/src/a.ts"),
            Some(mapping("pkg", "./src", vec![alias("@/*", &["./*"])]))
        );
        // baseUrl inherited from an extended config: the anchor is the config that supplies it.
        let repo = ts_repo(&[
            (
                "configs/base.json",
                r#"{ "compilerOptions": { "baseUrl": ".." } }"#,
            ),
            (
                "app/tsconfig.json",
                r#"{ "extends": "../configs/base.json", "compilerOptions": { "paths": { "@/*": ["app/src/*"] } } }"#,
            ),
        ]);
        assert_eq!(
            mapping_of(repo.path(), "app/src/a.ts"),
            Some(mapping("configs", "..", vec![alias("@/*", &["app/src/*"])]))
        );
        // No baseUrl in the chain: the anchor is the config that supplies the `paths`, base ".".
        let repo = ts_repo(&[
            (
                "configs/base.json",
                r#"{ "compilerOptions": { "paths": { "@/*": ["./shared/*"] } } }"#,
            ),
            ("app/tsconfig.json", r#"{ "extends": "../configs/base" }"#),
        ]);
        assert_eq!(
            mapping_of(repo.path(), "app/src/a.ts"),
            Some(mapping("configs", ".", vec![alias("@/*", &["./shared/*"])]))
        );
    }

    #[test]
    fn tsconfig_paths_from_a_referenced_project_are_discovered_when_the_nearest_tsconfig_has_only_references(
    ) {
        let repo = ts_repo(&[
            ("frontend/tsconfig.json", HEX_ROOT),
            (
                "frontend/tsconfig.app.json",
                r#"{ "compilerOptions": { "baseUrl": ".", "paths": { "@/*": ["./src/*"] } }, "include": ["src"] }"#,
            ),
            ("frontend/tsconfig.node.json", NODE_PROJECT),
        ]);
        let mut ctx = RepoConfigContext::new();
        let app = ctx.resolve_tsconfig_aliases("frontend/src/App.tsx", repo.path());
        assert!(
            app.entries.is_empty(),
            "entries stay HEAD's: the nearest has no paths"
        );
        assert_eq!(
            app.sole_inspected_covering_project_mapping,
            Some(mapping("frontend", ".", vec![alias("@/*", &["./src/*"])]))
        );
        // tsconfig.node.json is the sole inspected covering project of vite.config.ts and has no paths.
        assert_eq!(mapping_of(repo.path(), "frontend/vite.config.ts"), None);
    }

    #[test]
    fn a_file_no_candidate_project_covers_carries_no_mapping() {
        let repo = ts_repo(&[
            ("frontend/tsconfig.json", HEX_ROOT),
            (
                "frontend/tsconfig.app.json",
                r#"{ "compilerOptions": { "paths": { "@/*": ["./src/*"] } }, "include": ["src"] }"#,
            ),
            ("frontend/tsconfig.node.json", NODE_PROJECT),
        ]);
        assert_eq!(reader_of(repo.path(), "frontend/scripts/seed.ts"), Ok(None));
        let stored = RepoConfigContext::new()
            .resolve_tsconfig_aliases("frontend/scripts/seed.ts", repo.path());
        assert!(stored.entries.is_empty());
        assert_eq!(stored.sole_inspected_covering_project_mapping, None);
        // No tsconfig.json at all: no mapping.
        let repo = ts_repo(&[("src/a.ts", "")]);
        assert_eq!(reader_of(repo.path(), "src/a.ts"), Ok(None));
    }

    #[test]
    fn tsconfig_paths_from_an_array_extends_chain_are_discovered() {
        let repo = ts_repo(&[
            ("cfg/a.json", r#"{ "compilerOptions": { "strict": true } }"#),
            (
                "cfg/b.json",
                r#"{ "compilerOptions": { "paths": { "~/*": ["../lib/*"] } } }"#,
            ),
            (
                "tsconfig.json",
                r#"{ "extends": ["./cfg/a.json", "./cfg/b.json", "@tsconfig/node20"] }"#,
            ),
        ]);
        assert_eq!(
            mapping_of(repo.path(), "src/a.ts"),
            Some(mapping("cfg", ".", vec![alias("~/*", &["../lib/*"])]))
        );
    }

    #[test]
    fn array_extends_later_entry_overrides_earlier_paths_and_base_url() {
        let repo = ts_repo(&[
            (
                "cfg/a.json",
                r#"{ "compilerOptions": { "baseUrl": "a", "paths": { "@/*": ["./a/*"], "~a": ["./a/x"] } } }"#,
            ),
            (
                "cfg/b.json",
                r#"{ "compilerOptions": { "baseUrl": "b", "paths": { "@/*": ["./b/*"] } } }"#,
            ),
            (
                "tsconfig.json",
                r#"{ "extends": ["./cfg/a.json", "./cfg/b.json"] }"#,
            ),
        ]);
        assert_eq!(
            mapping_of(repo.path(), "src/a.ts"),
            Some(mapping("cfg", "b", vec![alias("@/*", &["./b/*"])])),
            "the later entry wins; `paths` is replaced whole, never merged"
        );
        // The child overrides every config it extends.
        let repo = ts_repo(&[
            (
                "cfg/a.json",
                r#"{ "compilerOptions": { "baseUrl": "a", "paths": { "@/*": ["./a/*"] } } }"#,
            ),
            (
                "tsconfig.json",
                r#"{ "extends": ["./cfg/a.json"], "compilerOptions": { "paths": { "~/*": ["./c/*"] } } }"#,
            ),
        ]);
        assert_eq!(
            mapping_of(repo.path(), "src/a.ts"),
            Some(mapping("cfg", "a", vec![alias("~/*", &["./c/*"])]))
        );
    }

    #[test]
    fn same_directory_files_covered_by_different_referenced_projects_get_their_own_mappings() {
        let repo = ts_repo(&[
            (
                "tsconfig.json",
                r#"{ "files": [], "references": [ { "path": "./tsconfig.one.json" }, { "path": "./tsconfig.two.json" } ] }"#,
            ),
            (
                "tsconfig.one.json",
                r#"{ "files": ["src/one.ts"], "compilerOptions": { "paths": { "@/*": ["./one/*"] } } }"#,
            ),
            (
                "tsconfig.two.json",
                r#"{ "files": ["src/two.ts"], "compilerOptions": { "paths": { "@/*": ["./two/*"] } } }"#,
            ),
        ]);
        let one = Some(mapping("", ".", vec![alias("@/*", &["./one/*"])]));
        let two = Some(mapping("", ".", vec![alias("@/*", &["./two/*"])]));
        for order in [["src/one.ts", "src/two.ts"], ["src/two.ts", "src/one.ts"]] {
            let mut ctx = RepoConfigContext::new();
            for file in order {
                let got = ctx
                    .resolve_tsconfig_aliases(file, repo.path())
                    .sole_inspected_covering_project_mapping;
                let want = if file == "src/one.ts" { &one } else { &two };
                assert_eq!(&got, want, "{order:?} {file}");
            }
        }
    }

    #[test]
    fn malformed_or_unreadable_project_config_leaves_the_file_without_mappings_and_one_warning() {
        let app = |extra: &str| {
            format!(
                r#"{{ "compilerOptions": {{ "paths": {{ "@/*": ["./src/*"] }}{extra} }}, "include": ["src"]{} }}"#,
                ""
            )
        };
        let cases: Vec<MalformedConfigCase> = vec![
            ("unparseable nearest", vec![("tsconfig.json", "{ not json".into())], "tsconfig.json"),
            ("not an object", vec![("tsconfig.json", "[1, 2]".into())], "tsconfig.json"),
            (
                "missing reference",
                vec![("tsconfig.json", r#"{ "files": [], "references": [ { "path": "./tsconfig.app.json" } ] }"#.into())],
                "tsconfig.app.json",
            ),
            (
                "missing extends target",
                vec![("tsconfig.json", r#"{ "extends": "./base.json", "compilerOptions": { "paths": { "@/*": ["./src/*"] } } }"#.into())],
                "base.json",
            ),
            ("baseUrl a number", vec![("tsconfig.json", app(r#", "baseUrl": 7"#))], "tsconfig.json"),
            ("outDir a number", vec![("tsconfig.json", app(r#", "outDir": 7"#))], "tsconfig.json"),
            (
                "files not a list",
                vec![("tsconfig.json", r#"{ "files": "src/a.ts", "compilerOptions": { "paths": { "@/*": ["./src/*"] } } }"#.into())],
                "tsconfig.json",
            ),
            (
                "include not a list",
                vec![("tsconfig.json", r#"{ "include": "src", "compilerOptions": { "paths": { "@/*": ["./src/*"] } } }"#.into())],
                "tsconfig.json",
            ),
            (
                "exclude holds a number",
                vec![("tsconfig.json", r#"{ "exclude": ["dist", 3], "compilerOptions": { "paths": { "@/*": ["./src/*"] } } }"#.into())],
                "tsconfig.json",
            ),
        ];
        for (name, files, config) in cases {
            let owned: Vec<(&str, &str)> = files.iter().map(|(p, c)| (*p, c.as_str())).collect();
            let repo = ts_repo(&owned);
            let why = reader_of(repo.path(), "src/a.ts").expect_err(name);
            assert!(
                why.contains(config),
                "{name}: the warning names the config: {why}"
            );
            let stored = RepoConfigContext::new().resolve_tsconfig_aliases("src/a.ts", repo.path());
            assert_eq!(
                stored.sole_inspected_covering_project_mapping, None,
                "{name}"
            );
            assert_eq!(
                stored.entries,
                read_tsconfig_aliases_from_path(&repo.path().join("tsconfig.json"))
                    .map(|a| a.entries)
                    .unwrap_or_default(),
                "{name}: entries exactly HEAD's"
            );
        }
        // `baseUrl` ABSENT: the default "." applies and the mapping is stored.
        let repo = ts_repo(&[("tsconfig.json", &app(""))]);
        assert_eq!(
            mapping_of(repo.path(), "src/a.ts"),
            Some(mapping("", ".", vec![alias("@/*", &["./src/*"])]))
        );
    }

    #[test]
    fn a_malformed_paths_references_extends_or_allow_js_value_leaves_the_file_without_a_mapping() {
        // The covering project `app` (extends `base`), another inspected project `other`.
        let layout = |app_extra: &str, other_extra: &str, base_extra: &str| {
            let root = r#"{ "files": [], "references": [ { "path": "./app.json" }, { "path": "./other.json" } ] }"#.to_string();
            let app = format!(
                r#"{{ "extends": "./base.json", "include": ["src"], "compilerOptions": {{ "paths": {{ "@/*": ["./src/*"] }} }}{app_extra} }}"#
            );
            let other = format!(r#"{{ "include": ["tools"]{other_extra} }}"#);
            let base = format!(r#"{{ "compilerOptions": {{ "strict": true }}{base_extra} }}"#);
            ts_repo(&[
                ("tsconfig.json", &root),
                ("app.json", &app),
                ("other.json", &other),
                ("base.json", &base),
            ])
        };
        let malformed = [
            (
                "paths an array",
                r#", "compilerOptions": { "paths": ["./src/*"] }"#,
            ),
            (
                "paths with a non-string substitution",
                r#", "compilerOptions": { "paths": { "~/*": ["./x/*", 7] } }"#,
            ),
            ("references not a list", r#", "references": "./x.json""#),
            (
                "a reference without a string path",
                r#", "references": [ { "path": 1 } ]"#,
            ),
            ("extends a number", r#", "extends": 5"#),
            (
                "extends a list holding a non-string",
                r#", "extends": ["./base.json", 5]"#,
            ),
            (
                "allowJs a string",
                r#", "compilerOptions": { "allowJs": "true" }"#,
            ),
            (
                "compilerOptions a string",
                r#", "compilerOptions": "strict""#,
            ),
        ];
        for (name, field) in malformed {
            for position in ["covering", "other", "extends parent"] {
                // A second key of the same name in a JSON object: serde keeps the last, so the
                // malformed value replaces the valid one in the covering project.
                let repo = match position {
                    "covering" => layout(field, "", ""),
                    "other" => layout("", field, ""),
                    _ => layout("", "", field),
                };
                assert!(
                    reader_of(repo.path(), "src/a.ts").is_err(),
                    "{name} in {position}: fail closed, never a default or a filtered value"
                );
                let stored =
                    RepoConfigContext::new().resolve_tsconfig_aliases("src/a.ts", repo.path());
                assert_eq!(
                    stored.sole_inspected_covering_project_mapping, None,
                    "{name} in {position}"
                );
                assert!(
                    stored.entries.is_empty(),
                    "{name} in {position}: HEAD's lenient entries"
                );
            }
        }
        // Every field ABSENT: the defaults apply and the mapping is stored.
        let repo = layout("", "", "");
        assert_eq!(
            mapping_of(repo.path(), "src/a.ts"),
            Some(mapping("", ".", vec![alias("@/*", &["./src/*"])]))
        );
    }

    #[test]
    fn exclude_narrows_include_and_never_removes_a_file_listed_in_files() {
        let repo = ts_repo(&[(
            "tsconfig.json",
            r#"{ "files": ["src/keep.ts"], "include": ["src"], "exclude": ["src/keep.ts", "src/drop.ts", "src/gen/**/*.ts"], "compilerOptions": { "paths": { "@/*": ["./src/*"] } } }"#,
        )]);
        let some = Some(mapping("", ".", vec![alias("@/*", &["./src/*"])]));
        assert_eq!(
            mapping_of(repo.path(), "src/keep.ts"),
            some,
            "listed in files"
        );
        assert_eq!(mapping_of(repo.path(), "src/other.ts"), some, "included");
        assert_eq!(
            mapping_of(repo.path(), "src/drop.ts"),
            None,
            "excluded from include"
        );
        assert_eq!(
            mapping_of(repo.path(), "src/gen/a/b.ts"),
            None,
            "a wildcard exclude"
        );
    }

    #[test]
    fn a_file_two_inspected_projects_cover_carries_no_mapping() {
        let two_refs = |first: &str, second: &str| {
            format!(
                r#"{{ "files": [], "references": [ {{ "path": "./{first}" }}, {{ "path": "./{second}" }} ] }}"#
            )
        };
        let with_paths = |dir: &str| {
            format!(
                r#"{{ "include": ["src"], "compilerOptions": {{ "paths": {{ "@/*": ["./{dir}/*"] }} }} }}"#
            )
        };
        let without_paths = r#"{ "include": ["src"] }"#.to_string();
        let variants: Vec<(&str, String, String)> = vec![
            ("different paths", with_paths("one"), with_paths("two")),
            ("equal paths", with_paths("one"), with_paths("one")),
            (
                "paths in only one",
                with_paths("one"),
                without_paths.clone(),
            ),
        ];
        for (name, a, b) in &variants {
            for (first, second) in [("a.json", "b.json"), ("b.json", "a.json")] {
                let repo = ts_repo(&[
                    ("tsconfig.json", &two_refs(first, second)),
                    ("a.json", a),
                    ("b.json", b),
                ]);
                assert_eq!(
                    reader_of(repo.path(), "src/main.ts"),
                    Ok(None),
                    "{name}, {first} first: a decline, never a pick, a merge or a union, and no warning"
                );
                let stored =
                    RepoConfigContext::new().resolve_tsconfig_aliases("src/main.ts", repo.path());
                assert!(stored.entries.is_empty(), "{name}: entries as at HEAD");
            }
        }
        // A nearest tsconfig.json without `files` and `include` (it covers everything under it)
        // beside a reference that also covers the file.
        let repo = ts_repo(&[
            (
                "tsconfig.json",
                r#"{ "references": [ { "path": "./app.json" } ], "compilerOptions": { "paths": { "@/*": ["./root/*"] } } }"#,
            ),
            ("app.json", &with_paths("child")),
        ]);
        assert_eq!(reader_of(repo.path(), "src/main.ts"), Ok(None));
        assert_eq!(
            RepoConfigContext::new()
                .resolve_tsconfig_aliases("src/main.ts", repo.path())
                .entries,
            vec![alias("@/*", &["./root/*"])],
            "entries stay the nearest's, as at HEAD"
        );
    }

    #[test]
    fn a_sole_inspected_covering_project_without_paths_gives_the_file_no_mapping() {
        let repo = ts_repo(&[
            (
                "tsconfig.json",
                r#"{ "files": [], "references": [ { "path": "./app.json" }, { "path": "./tools.json" } ] }"#,
            ),
            ("app.json", r#"{ "include": ["src"] }"#),
            (
                "tools.json",
                r#"{ "include": ["tools"], "compilerOptions": { "paths": { "@/*": ["./tools/*"] } } }"#,
            ),
        ]);
        assert_eq!(reader_of(repo.path(), "src/main.ts"), Ok(None));
        assert_eq!(
            mapping_of(repo.path(), "tools/run.ts"),
            Some(mapping("", ".", vec![alias("@/*", &["./tools/*"])]))
        );
    }

    #[test]
    fn a_root_tsconfig_with_empty_files_and_its_own_paths_supplies_no_mapping_and_the_covering_reference_does(
    ) {
        let repo = ts_repo(&[
            ("admin/tsconfig.json", AMODX_ROOT),
            ("admin/tsconfig.app.json", AMODX_APP),
            ("admin/tsconfig.node.json", NODE_PROJECT),
        ]);
        let stored = RepoConfigContext::new()
            .resolve_tsconfig_aliases("admin/src/components/editor/Toolbar.tsx", repo.path());
        assert_eq!(
            stored.sole_inspected_covering_project_mapping,
            Some(mapping("admin", ".", vec![alias("@/*", &["./src/*"])])),
            "the child's mapping (JSONC comments and trailing commas read as TypeScript reads them)"
        );
        assert_eq!(
            stored.entries,
            vec![alias("@/*", &["./root/*"])],
            "entries stay the root's, as at HEAD"
        );
        // The child's `exclude` narrows its `include`.
        assert_eq!(mapping_of(repo.path(), "admin/src/a.test.ts"), None);
    }

    #[test]
    fn the_nearest_tsconfig_paths_apply_only_to_files_its_membership_covers() {
        let repo = ts_repo(&[(
            "tsconfig.json",
            r#"{ "include": ["src"], "compilerOptions": { "paths": { "@/*": ["./src/*"] } } }"#,
        )]);
        assert_eq!(
            mapping_of(repo.path(), "src/a.ts"),
            Some(mapping("", ".", vec![alias("@/*", &["./src/*"])]))
        );
        assert_eq!(mapping_of(repo.path(), "scripts/build.ts"), None);
        assert_eq!(
            RepoConfigContext::new()
                .resolve_tsconfig_aliases("scripts/build.ts", repo.path())
                .entries,
            vec![alias("@/*", &["./src/*"])],
            "the classifier's entries are unchanged"
        );
    }

    #[test]
    fn a_js_file_is_a_member_only_of_a_project_with_allow_js() {
        for (allow_js, member) in [
            ("", false),
            (r#", "allowJs": false"#, false),
            (r#", "allowJs": true"#, true),
        ] {
            let repo = ts_repo(&[(
                "tsconfig.json",
                &format!(
                    r#"{{ "compilerOptions": {{ "paths": {{ "@/*": ["./src/*"] }}{allow_js} }} }}"#
                ),
            )]);
            for file in ["src/a.js", "src/a.jsx", "src/a.mjs", "src/a.cjs"] {
                assert_eq!(
                    mapping_of(repo.path(), file).is_some(),
                    member,
                    "{allow_js} {file}"
                );
            }
            assert!(mapping_of(repo.path(), "src/a.ts").is_some(), "{allow_js}");
        }
    }

    #[test]
    fn a_file_under_the_project_out_dir_is_not_a_member() {
        for include in ["", r#", "include": ["src", "dist"]"#] {
            let repo = ts_repo(&[(
                "tsconfig.json",
                &format!(
                    r#"{{ "compilerOptions": {{ "outDir": "./dist", "paths": {{ "@/*": ["./src/*"] }} }}{include} }}"#
                ),
            )]);
            assert_eq!(mapping_of(repo.path(), "dist/a.ts"), None, "{include}");
            assert!(mapping_of(repo.path(), "src/a.ts").is_some(), "{include}");
        }
    }

    #[test]
    fn head_entries_stay_the_nearest_tsconfig_aliases_for_the_classifier() {
        let layouts: Vec<TsconfigLayout> = vec![
            (
                vec![
                    ("admin/tsconfig.json", AMODX_ROOT),
                    ("admin/tsconfig.app.json", AMODX_APP),
                    ("admin/tsconfig.node.json", NODE_PROJECT),
                ],
                "admin/src/a.ts",
                "admin/tsconfig.json",
            ),
            (
                vec![
                    ("frontend/tsconfig.json", HEX_ROOT),
                    ("frontend/tsconfig.app.json", AMODX_APP),
                    ("frontend/tsconfig.node.json", NODE_PROJECT),
                ],
                "frontend/src/a.ts",
                "frontend/tsconfig.json",
            ),
            (
                vec![
                    (
                        "cfg/base.json",
                        r#"{ "compilerOptions": { "paths": { "@/*": ["./x/*"] } } }"#,
                    ),
                    ("pkg/tsconfig.json", r#"{ "extends": "../cfg/base.json" }"#),
                ],
                "pkg/src/a.ts",
                "pkg/tsconfig.json",
            ),
        ];
        for (files, file, nearest) in layouts {
            let repo = ts_repo(&files);
            let stored = RepoConfigContext::new().resolve_tsconfig_aliases(file, repo.path());
            assert_eq!(
                stored.entries,
                read_tsconfig_aliases_from_path(&repo.path().join(nearest))
                    .map(|a| a.entries)
                    .unwrap_or_default(),
                "{file}"
            );
        }
    }

    // ── extract_cargo_dependencies ───────────────────────────

    #[test]
    fn extracts_cargo_all_dep_sections() {
        let content = r#"
[package]
name = "my-crate"
version = "0.1.0"

[dependencies]
serde = "1.0"
tokio = { version = "1.0", features = ["full"] }

[dev-dependencies]
tempfile = "3"

[build-dependencies]
cc = "1.0"
"#;
        let deps = extract_cargo_dependencies(content).unwrap();
        assert!(deps.names.contains(&"serde".to_string()));
        assert!(deps.names.contains(&"tokio".to_string()));
        assert!(deps.names.contains(&"tempfile".to_string()));
        assert!(deps.names.contains(&"cc".to_string()));
    }

    #[test]
    fn cargo_normalizes_hyphen_to_underscore() {
        let content = r#"
[dependencies]
my-crate = "1.0"
some_other = "2.0"
"#;
        let deps = extract_cargo_dependencies(content).unwrap();
        // Both should be normalized to underscore form.
        assert!(
            deps.names.contains(&"my_crate".to_string()),
            "hyphenated dep should be normalized: {:?}",
            deps.names
        );
        assert!(deps.names.contains(&"some_other".to_string()));
    }

    #[test]
    fn cargo_handles_subtable_syntax() {
        let content = r#"
[package]
name = "foo"

[dependencies.serde]
version = "1.0"
features = ["derive"]

[dependencies.tokio]
version = "1.0"
"#;
        let deps = extract_cargo_dependencies(content).unwrap();
        assert!(deps.names.contains(&"serde".to_string()));
        assert!(deps.names.contains(&"tokio".to_string()));
    }

    #[test]
    fn cargo_target_specific_deps_not_extracted() {
        // Target-specific dependencies like [target.'cfg(unix)'.dependencies]
        // are NOT extracted by the simple line parser. This is a known
        // limitation documented in the function. The primary use case is
        // identifying external crate names for import classification, and
        // target-specific deps are edge cases.
        let content = r#"
[package]
name = "foo"

[target.'cfg(unix)'.dependencies]
nix = "0.26"
"#;
        // Should return None since there are no standard dependency sections.
        assert!(extract_cargo_dependencies(content).is_none());
    }

    #[test]
    fn cargo_returns_none_for_no_deps() {
        let content = r#"
[package]
name = "lib"
version = "0.1.0"
"#;
        assert!(extract_cargo_dependencies(content).is_none());
    }

    #[test]
    fn cargo_returns_sorted_unique() {
        let content = r#"
[dependencies]
zebra = "1"
alpha = "1"

[dev-dependencies]
alpha = "2"
"#;
        let deps = extract_cargo_dependencies(content).unwrap();
        assert_eq!(deps.names, vec!["alpha", "zebra"]);
    }

    // ── RepoConfigContext for Cargo ──────────────────────────

    #[test]
    fn nearest_ancestor_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Root Cargo.toml.
        fs::write(
            root.join("Cargo.toml"),
            r#"
[package]
name = "workspace"

[dependencies]
serde = "1"
"#,
        )
        .unwrap();

        // Nested crate Cargo.toml.
        fs::create_dir_all(root.join("crates/api")).unwrap();
        fs::write(
            root.join("crates/api/Cargo.toml"),
            r#"
[package]
name = "api"

[dependencies]
tokio = "1"
"#,
        )
        .unwrap();

        let mut ctx = RepoConfigContext::new();

        // File in root → gets root deps.
        let root_deps = ctx.resolve_cargo_deps("src/lib.rs", root);
        assert_eq!(root_deps.names, vec!["serde"]);

        // File in crates/api → gets nested deps.
        let api_deps = ctx.resolve_cargo_deps("crates/api/src/lib.rs", root);
        assert_eq!(api_deps.names, vec!["tokio"]);
    }

    // ── extract_gradle_dependencies ──────────────────────────

    /// Groovy DSL, string-literal coordinates, mixed configuration verbs and
    /// quote styles, `${…}` version interpolation — the spring-petclinic shape.
    /// The captured name is the GROUP ID (the load-bearing decision), so
    /// `org.springframework.boot:spring-boot-starter-cache` → `org.springframework.boot`.
    #[test]
    fn gradle_groovy_string_form_captures_group() {
        let content = r#"
dependencies {
  implementation 'org.springframework.boot:spring-boot-starter-cache'
  implementation "jakarta.xml.bind:jakarta.xml.bind-api"
  runtimeOnly "org.webjars:webjars-locator-lite:${webjarsLocatorLiteVersion}"
  testImplementation 'org.testcontainers:testcontainers-junit-jupiter'
  checkstyle "io.spring.javaformat:spring-javaformat-checkstyle:${v}"
}
"#;
        let deps = extract_gradle_dependencies(content).unwrap();
        assert_eq!(
            deps.own,
            vec![
                "io.spring.javaformat",
                "jakarta.xml.bind",
                "org.springframework.boot",
                "org.testcontainers",
                "org.webjars",
            ],
            "group ids captured (not artifact ids, not full coordinates), sorted+unique"
        );
    }

    /// Binding evidence for the configuration-verb surface (slice §4). The
    /// standard Gradle configurations — including `compileOnly` and
    /// `annotationProcessor`, which the string-form/Kotlin tests above do not
    /// exercise — each carry a DISTINCT coordinate group, so a per-verb
    /// regression would drop exactly that verb's group from the captured set.
    /// The reader is verb-agnostic (any coordinate inside `dependencies { … }`
    /// is mined regardless of the verb), so this pins the named surface, not
    /// the mechanism.
    #[test]
    fn gradle_captures_all_standard_configuration_verbs() {
        let content = r#"
dependencies {
  implementation 'g.impl:a:1.0'
  api 'g.api:a:1.0'
  compileOnly 'g.compileonly:a:1.0'
  runtimeOnly 'g.runtimeonly:a:1.0'
  testImplementation 'g.testimpl:a:1.0'
  annotationProcessor 'g.annotationprocessor:a:1.0'
  checkstyle 'g.checkstyle:a:1.0'
}
"#;
        let deps = extract_gradle_dependencies(content).unwrap();
        assert_eq!(
            deps.own,
            vec![
                "g.annotationprocessor",
                "g.api",
                "g.checkstyle",
                "g.compileonly",
                "g.impl",
                "g.runtimeonly",
                "g.testimpl",
            ],
            "every standard configuration verb's coordinate is mined (sorted+unique)"
        );
    }

    /// The GROUP is captured, never the artifact id or the full coordinate —
    /// asserted directly on the guava counterexample coordinate. (The consumer's
    /// honest degradation for guava — `com.google.guava` group vs
    /// `com.google.common.*` packages, which the prefix rule cannot match — is
    /// asserted in `unresolved_classifier` test `guava_group_degrades_honestly`.)
    #[test]
    fn gradle_captures_group_not_artifact_or_full_coordinate() {
        let content = r#"
dependencies {
  implementation 'com.google.guava:guava:31.0'
}
"#;
        let deps = extract_gradle_dependencies(content).unwrap();
        assert_eq!(deps.own, vec!["com.google.guava"]);
        assert!(!deps.own.contains(&"guava".to_string()));
        assert!(!deps
            .own
            .contains(&"com.google.guava:guava:31.0".to_string()));
    }

    /// Kotlin DSL: `verb("g:a:v")` string form (parens + double quotes).
    #[test]
    fn gradle_kotlin_string_form() {
        let content = r#"
dependencies {
    implementation("org.springframework.boot:spring-boot-starter-web")
    testImplementation("org.junit.jupiter:junit-jupiter:5.10.0")
    api(platform("io.grpc:grpc-bom:1.60.0"))
}
"#;
        let deps = extract_gradle_dependencies(content).unwrap();
        assert_eq!(
            deps.own,
            vec!["io.grpc", "org.junit.jupiter", "org.springframework.boot"]
        );
    }

    /// Groovy map form `group:, name:, version:` and Kotlin map form
    /// `group =, name =` both capture the group value.
    #[test]
    fn gradle_map_form_both_dsls() {
        let groovy = r#"
dependencies {
    implementation group: 'com.google.guava', name: 'guava', version: '31.0'
}
"#;
        assert_eq!(
            extract_gradle_dependencies(groovy).unwrap().own,
            vec!["com.google.guava"]
        );

        let kotlin = r#"
dependencies {
    implementation(group = "org.apache.commons", name = "commons-lang3", version = "3.14.0")
}
"#;
        assert_eq!(
            extract_gradle_dependencies(kotlin).unwrap().own,
            vec!["org.apache.commons"]
        );
    }

    /// `exclude group: 'g', module: 'm'` (uses `module`, not `name`) must NOT be
    /// captured — only the real declaration on the same closure is. This is the
    /// guard that keeps an excluded transitive group out of the declared set.
    #[test]
    fn gradle_exclude_group_not_captured() {
        let content = r#"
dependencies {
    implementation('org.mockito:mockito-core:5.0') {
        exclude group: 'net.bytebuddy', module: 'byte-buddy'
    }
}
"#;
        let deps = extract_gradle_dependencies(content).unwrap();
        assert_eq!(deps.own, vec!["org.mockito"]);
        assert!(
            !deps.own.contains(&"net.bytebuddy".to_string()),
            "excluded group must not be captured as a declared dependency"
        );
    }

    /// Version-catalog refs (`libraries.guava`), `project(':core')` deps, and the
    /// `kotlin("stdlib")` helper carry no literal coordinate → honestly not
    /// captured (no fabrication). A block of only these yields `None`.
    #[test]
    fn gradle_non_literal_forms_not_fabricated() {
        let content = r#"
dependencies {
    implementation libraries.guava
    api libraries.jsr305, libraries.errorprone.annotations
    testImplementation project(':grpc-core')
    implementation(kotlin("stdlib"))
}
"#;
        assert!(
            extract_gradle_dependencies(content).is_none(),
            "no literal coordinates → None, never a fabricated group"
        );
    }

    /// Commented-out dependencies (`//` line, `/* */` block, incl. multi-line
    /// where the dep line does not itself start with a comment marker) must not
    /// be captured.
    #[test]
    fn gradle_commented_deps_not_captured() {
        let content = r#"
dependencies {
    // implementation 'commented:line-form:1.0'
    implementation 'real:string-dep:1.0'
    /*
    implementation 'commented:block-form:2.0'
    */
    runtimeOnly 'real:runtime-dep:1.0' // trailing note 'not:a:dep'
}
"#;
        let deps = extract_gradle_dependencies(content).unwrap();
        assert_eq!(deps.own, vec!["real"]);
        assert!(!deps.own.iter().any(|n| n == "commented"));
        assert!(!deps.own.iter().any(|n| n == "not"));
    }

    /// Only coordinates INSIDE a `dependencies { … }` block are mined: plugin
    /// ids, `group =`, and repository URLs elsewhere are not dependencies.
    #[test]
    fn gradle_scopes_to_dependencies_block() {
        let content = r#"
plugins {
    id 'org.springframework.boot' version '4.0.3'
}
group = 'org.springframework.samples'
repositories {
    maven { url 'https://repo.example.com:8443/maven2/' }
}
dependencies {
    implementation 'org.real:dep:1.0'
}
"#;
        let deps = extract_gradle_dependencies(content).unwrap();
        assert_eq!(deps.own, vec!["org.real"]);
        // Plugin id (no colon), project group (no colon), and the repo URL
        // (`https` split, artifact segment has `/`) are all excluded.
        assert!(!deps.own.iter().any(|n| n == "https"));
        assert!(!deps.own.iter().any(|n| n == "org.springframework.samples"));
    }

    /// A dependencies block nested under `subprojects { … }` is still mined — into the
    /// `subprojects` scope, NOT the script's own (DEPS-GRADLE-CATALOG-1A, stated expectation
    /// change: RG-REQ-006-L13 — a `subprojects` block does not declare for the root project).
    #[test]
    fn gradle_nested_subprojects_block() {
        let content = r#"
subprojects {
    dependencies {
        implementation 'io.grpc:grpc-core:1.60.0'
    }
}
"#;
        let scopes = extract_gradle_dependencies(content).unwrap();
        assert_eq!(scopes.subprojects, vec!["io.grpc"]);
        assert!(
            scopes.own.is_empty(),
            "a subprojects block never declares for the script's own project, got {:?}",
            scopes.own
        );
    }

    /// Malformed / empty / absent dependency blocks → `None` (mirrors the cargo
    /// `returns_none_for_no_deps` shape).
    #[test]
    fn gradle_malformed_and_empty_return_none() {
        assert!(extract_gradle_dependencies("").is_none());
        assert!(extract_gradle_dependencies("dependencies {\n}\n").is_none());
        assert!(extract_gradle_dependencies("plugins { id 'java' }\n").is_none());
        // Unbalanced/garbage — no coordinate found → None, not a panic.
        assert!(extract_gradle_dependencies("dependencies { {{{ ").is_none());
    }

    /// A one-line block — `dependencies { implementation 'g:a:v' }` — IS mined:
    /// the coordinate lies between the `{` and `}` on the same line, and only
    /// the text between them is buffered. Both DSLs / quote styles, and an
    /// out-of-block coordinate on a block-opening line is not captured.
    #[test]
    fn gradle_one_line_block_is_mined() {
        assert_eq!(
            extract_gradle_dependencies("dependencies { implementation 'com.example:lib:1.0' }\n")
                .unwrap()
                .own,
            vec!["com.example"]
        );
        // Kotlin DSL, parens + double quotes, no trailing newline (final flush).
        assert_eq!(
            extract_gradle_dependencies(
                "dependencies { implementation(\"io.grpc:grpc-core:1.60\") }"
            )
            .unwrap()
            .own,
            vec!["io.grpc"]
        );
        // A coordinate-shaped token OUTSIDE the block, on the same line as the
        // one-line dependencies block, must NOT be captured.
        assert_eq!(
            extract_gradle_dependencies(
                "task x { doFirst { println 'a:b:c' } } ; dependencies { implementation 'org.real:d:1.0' }\n"
            )
            .unwrap()
            .own,
            vec!["org.real"],
            "only the in-block coordinate is mined, not the one in the task closure"
        );
    }

    /// An UNCLOSED dependencies block that contains a coordinate returns `None`
    /// (the block extent is untrustworthy → honest degradation, not a guess),
    /// even though a coordinate was seen. The paired closed-block assertion
    /// proves the `None` is caused by the missing `}`, not by the coordinate.
    #[test]
    fn gradle_unclosed_block_returns_none_even_with_coordinate() {
        let unclosed = "dependencies {\n    implementation 'com.example:lib:1.0'\n";
        assert!(
            extract_gradle_dependencies(unclosed).is_none(),
            "unclosed dependencies block must degrade to None, not return its coordinate"
        );
        let closed = "dependencies {\n    implementation 'com.example:lib:1.0'\n}\n";
        assert_eq!(
            extract_gradle_dependencies(closed).unwrap().own,
            vec!["com.example"]
        );
    }

    // ── DEPS-GRADLE-CATALOG-1A: scope-aware extraction (RG-REQ-006-L13) ──

    fn scopes(content: &str) -> GradleScriptScopes {
        extract_gradle_dependencies(content).expect("script yields scopes")
    }

    /// A `buildscript { dependencies { classpath … } }` block is the build tool's classpath, never
    /// a declaration of any project — beside a real top-level block only the real block is
    /// declared (kafka build.gradle:25-28), also when the buildscript sits under `allprojects`.
    #[test]
    fn gradle_buildscript_dependencies_are_never_declared() {
        let content = r#"
buildscript {
  dependencies {
    classpath "org.ajoberstar.grgit:grgit-core:$versions.grgit"
  }
}
allprojects {
  buildscript {
    dependencies {
      classpath 'com.tool:plugin:1.0'
    }
  }
}
dependencies {
  implementation 'org.real:dep:1.0'
}
"#;
        let s = scopes(content);
        assert_eq!(s.own, vec!["org.real"]);
        assert!(s.allprojects.is_empty(), "{:?}", s.allprojects);
        assert!(s.subprojects.is_empty() && s.projects.is_empty());
        assert!(
            s.undetermined_block_lines.is_empty(),
            "a tooling classpath block is excluded by name, never counted: {:?}",
            s.undetermined_block_lines
        );
    }

    /// `pluginManagement { dependencies { … } }` (settings-style tooling classpath) likewise.
    #[test]
    fn gradle_plugin_management_dependencies_are_never_declared() {
        let content = r#"
pluginManagement {
    dependencies {
        classpath("com.plugin:thing:2.0")
    }
}
dependencies {
    implementation("org.real:dep:1.0")
}
"#;
        let s = scopes(content);
        assert_eq!(s.own, vec!["org.real"]);
        assert!(s.undetermined_block_lines.is_empty());
    }

    /// A `dependencies` block at the script's top level is the script's OWN project scope.
    #[test]
    fn gradle_top_level_block_is_the_scripts_own_scope() {
        let s = scopes("dependencies {\n  implementation 'org.own:a:1'\n}\n");
        assert_eq!(s.own, vec!["org.own"]);
        assert!(s.allprojects.is_empty() && s.subprojects.is_empty() && s.projects.is_empty());
    }

    /// A direct child of a top-level `allprojects {` block lands in the `allprojects` scope only.
    #[test]
    fn gradle_allprojects_block_is_recorded_in_the_allprojects_scope() {
        let s =
            scopes("allprojects {\n  dependencies {\n    implementation 'org.all:a:1'\n  }\n}\n");
        assert_eq!(s.allprojects, vec!["org.all"]);
        assert!(s.own.is_empty() && s.subprojects.is_empty() && s.projects.is_empty());
    }

    /// A direct child of a top-level `subprojects {` block lands in the `subprojects` scope only.
    #[test]
    fn gradle_subprojects_block_is_recorded_in_the_subprojects_scope() {
        let s =
            scopes("subprojects {\n  dependencies {\n    implementation 'org.sub:a:1'\n  }\n}\n");
        assert_eq!(s.subprojects, vec!["org.sub"]);
        assert!(s.own.is_empty() && s.allprojects.is_empty() && s.projects.is_empty());
    }

    /// A top-level `project('<path>') {` block keys its direct `dependencies` under the Gradle path
    /// AS WRITTEN — both quote styles and the Kotlin form; a relative path stays relative; and
    /// `from(project(':tools').jar) { … }` is not a scope head (the `{` does not follow the call).
    #[test]
    fn gradle_project_block_is_recorded_under_its_gradle_path() {
        let content = r#"
project(':a:b') {
  dependencies {
    implementation 'org.ab.single:x:1'
  }
}
project(":a:b") {
  dependencies {
    implementation "org.ab.double:x:1"
  }
}
project('b') {
  dependencies {
    implementation 'org.rel:x:1'
  }
}
tasks.register('dist') {
  from(project(':tools').jar) {
    into 'lib'
  }
}
"#;
        let s = scopes(content);
        assert_eq!(
            s.projects.get(":a:b").cloned().unwrap_or_default(),
            vec!["org.ab.double", "org.ab.single"]
        );
        assert_eq!(
            s.projects.get("b").cloned().unwrap_or_default(),
            vec!["org.rel"]
        );
        assert!(!s.projects.contains_key(":tools"), "{:?}", s.projects);
        assert_eq!(s.projects.len(), 2, "{:?}", s.projects);
        assert!(s.own.is_empty());

        let kts = scopes("project(\":a:b\") {\n    dependencies {\n        implementation(\"org.kts:x:1\")\n    }\n}\n");
        assert_eq!(
            kts.projects.get(":a:b").cloned().unwrap_or_default(),
            vec!["org.kts"]
        );
    }

    /// A `dependencies` block under any enclosing block other than the three scope heads — a
    /// condition, a callback, a plugin hook, a task or a plugin extension — declares NOTHING: the
    /// index cannot establish that it applies.
    #[test]
    fn gradle_dependencies_under_any_other_block_are_never_declared() {
        let content = r#"
if (x) {
  dependencies {
    implementation 'org.cond:a:1'
  }
}
afterEvaluate {
  dependencies {
    implementation 'org.after:a:1'
  }
}
subprojects {
  plugins.withId("java") {
    dependencies {
      implementation 'org.withid:a:1'
    }
  }
}
project(':c') {
  shadowJar {
    dependencies {
      include(dependency('org.shadow:a:1'))
    }
  }
}
task t {
  dependencies {
    implementation 'org.task:a:1'
  }
}
dependencies {
  implementation 'org.real:a:1'
}
"#;
        let s = scopes(content);
        assert_eq!(s.own, vec!["org.real"]);
        assert!(s.allprojects.is_empty(), "{:?}", s.allprojects);
        assert!(s.subprojects.is_empty(), "{:?}", s.subprojects);
        assert!(s.projects.is_empty(), "{:?}", s.projects);
    }

    /// A scope head that is itself NOT at the top level is never attributed (never guessed).
    #[test]
    fn gradle_scope_heads_below_the_top_level_are_never_attributed() {
        let content = r#"
subprojects {
  project(':x') {
    dependencies {
      implementation 'org.nested:a:1'
    }
  }
}
if (c) {
  subprojects {
    dependencies {
      implementation 'org.condsub:a:1'
    }
  }
}
"#;
        let s = scopes(content);
        assert!(s.own.is_empty(), "{:?}", s.own);
        assert!(s.subprojects.is_empty(), "{:?}", s.subprojects);
        assert!(s.allprojects.is_empty(), "{:?}", s.allprojects);
        assert!(s.projects.is_empty(), "{:?}", s.projects);
    }

    /// D-DGC-CONDITIONAL-1 (RG-REQ-002-L11): a `dependencies` block the index cannot evaluate is
    /// COUNTED with its 1-based line, in line order, never dropped silently — and a direct block
    /// beside it is still declared.
    #[test]
    fn gradle_evaluation_dependent_blocks_are_counted_with_their_lines() {
        let content = "dependencies {\n\
  implementation 'org.direct:a:1'\n\
}\n\
if (x) {\n\
  dependencies {\n\
    implementation 'org.c1:a:1'\n\
  }\n\
}\n\
afterEvaluate {\n\
  dependencies {\n\
    implementation 'org.c2:a:1'\n\
  }\n\
}\n\
plugins.withId(\"java\") {\n\
  dependencies {\n\
    implementation 'org.c3:a:1'\n\
  }\n\
}\n\
gradle.projectsEvaluated {\n\
  dependencies {\n\
    implementation 'org.c4:a:1'\n\
  }\n\
}\n\
subprojects {\n\
  plugins.withId(\"java\") {\n\
    try {\n\
      dependencies {\n\
        implementation 'org.c5:a:1'\n\
      }\n\
    } finally {\n\
    }\n\
  }\n\
}\n";
        let s = scopes(content);
        assert_eq!(s.undetermined_block_lines, vec![5, 10, 15, 20, 27]);
        assert_eq!(s.own, vec!["org.direct"]);
        assert!(s.subprojects.is_empty(), "{:?}", s.subprojects);
    }

    /// The two closed exclusion lists — tooling (`buildscript`, `pluginManagement`) and
    /// task/extension (`shadowJar`, `tasks.…`, `task <name>`) — are never counted; a
    /// buildscript-only script is `None` (neither a direct coordinate nor a counted block).
    #[test]
    fn gradle_tooling_and_task_blocks_are_not_counted() {
        let content = r#"
buildscript {
  dependencies {
    classpath 'g:a:1'
  }
}
pluginManagement {
  dependencies {
    classpath 'g:b:1'
  }
}
shadowJar {
  dependencies {
    include(dependency('g:c:1'))
  }
}
tasks.named("shadowJar").configure {
  dependencies {
    exclude(dependency('g:d:1'))
  }
}
task t {
  dependencies {
    implementation 'g:e:1'
  }
}
dependencies {
  implementation 'org.real:a:1'
}
"#;
        let s = scopes(content);
        assert!(
            s.undetermined_block_lines.is_empty(),
            "{:?}",
            s.undetermined_block_lines
        );
        assert_eq!(s.own, vec!["org.real"]);
        assert!(
            extract_gradle_dependencies(
                "buildscript {\n  dependencies {\n    classpath 'g:a:1'\n  }\n}\n"
            )
            .is_none(),
            "a buildscript-only script declares nothing and counts nothing"
        );
    }

    /// A receiver-qualified head (`<expr>.dependencies {`) is never direct (receivers are not
    /// resolved) and is counted; so is a composed scope head. None of them declares a group.
    #[test]
    fn gradle_receiver_qualified_and_composed_scope_blocks_are_counted_never_declared() {
        let content = "subprojects {\n\
  afterEvaluate { subproject ->\n\
    if (c) {\n\
      subproject.dependencies {\n\
        mockitoAgent 'g:a:1'\n\
      }\n\
    }\n\
  }\n\
}\n\
rootProject.dependencies {\n\
  implementation 'g:b:1'\n\
}\n\
subprojects {\n\
  project(':x') {\n\
    dependencies {\n\
      implementation 'g:c:1'\n\
    }\n\
  }\n\
}\n\
if (c) {\n\
  subprojects {\n\
    dependencies {\n\
      implementation 'g:d:1'\n\
    }\n\
  }\n\
}\n";
        let s = scopes(content);
        assert_eq!(s.undetermined_block_lines, vec![4, 10, 15, 22]);
        assert!(s.own.is_empty(), "{:?}", s.own);
        assert!(s.allprojects.is_empty(), "{:?}", s.allprojects);
        assert!(s.subprojects.is_empty(), "{:?}", s.subprojects);
        assert!(s.projects.is_empty(), "{:?}", s.projects);
    }

    // ── RepoConfigContext for Gradle ─────────────────────────

    // The Gradle + pyproject RESOLVER tests (`nearest_ancestor_gradle`,
    // `gradle_leaf_without_deps_does_not_inherit_parent`, `resolve_pyproject_nearest_manifest_wins`)
    // moved with their resolvers to `crate::manifest_deps` (guardrail: config.rs is not grown). The
    // Gradle/pyproject PARSER tests (`extract_*`) stay here beside the parsers they exercise.
}
