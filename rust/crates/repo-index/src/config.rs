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

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use repo_graph_classification::types::{PackageDependencySet, TsconfigAliasEntry, TsconfigAliases};

/// Pre-computed config context for a repo. Caches config lookups
/// by directory so each directory is resolved at most once.
pub struct RepoConfigContext {
    /// Directory → PackageDependencySet cache (for JS/TS via package.json).
    pkg_cache: HashMap<String, PackageDependencySet>,
    /// Directory → TsconfigAliases cache.
    tsconfig_cache: HashMap<String, TsconfigAliases>,
    /// Directory → PackageDependencySet cache (for Rust via Cargo.toml).
    cargo_cache: HashMap<String, PackageDependencySet>,
    /// Directory → PackageDependencySet cache (for Java via build.gradle[.kts]).
    gradle_cache: HashMap<String, PackageDependencySet>,
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
            cargo_cache: HashMap::new(),
            gradle_cache: HashMap::new(),
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

            // Try reading package.json at this directory.
            // TS behavior: if the file EXISTS, stop here regardless of
            // parse success. Extract deps if parseable, else empty.
            // A broken leaf manifest should NOT inherit parent deps.
            let abs_dir = if probe.is_empty() {
                repo_root.to_path_buf()
            } else {
                repo_root.join(&probe)
            };
            let pkg_path = abs_dir.join("package.json");
            if pkg_path.exists() {
                let deps = std::fs::read_to_string(&pkg_path)
                    .ok()
                    .and_then(|content| extract_package_dependencies(&content))
                    .unwrap_or_else(|| empty.clone());
                self.pkg_cache.insert(probe.clone(), deps.clone());
                self.pkg_cache.insert(dir.clone(), deps.clone());
                return deps;
            }

            if probe.is_empty() {
                break;
            }
            probe = parent_dir(&probe);
        }

        self.pkg_cache.insert(dir, empty.clone());
        empty
    }

    /// Resolve tsconfig aliases for a file.
    /// Walks from file's directory upward to repo root.
    pub fn resolve_tsconfig_aliases(
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
            if tsconfig_path.exists() {
                let aliases = read_tsconfig_aliases_from_path(&tsconfig_path)
                    .unwrap_or_else(|| empty.clone());
                self.tsconfig_cache.insert(probe.clone(), aliases.clone());
                self.tsconfig_cache.insert(dir.clone(), aliases.clone());
                return aliases;
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
            if cargo_path.exists() {
                let deps = std::fs::read_to_string(&cargo_path)
                    .ok()
                    .and_then(|content| extract_cargo_dependencies(&content))
                    .unwrap_or_else(|| empty.clone());
                self.cargo_cache.insert(probe.clone(), deps.clone());
                self.cargo_cache.insert(dir.clone(), deps.clone());
                return deps;
            }

            if probe.is_empty() {
                break;
            }
            probe = parent_dir(&probe);
        }

        self.cargo_cache.insert(dir, empty.clone());
        empty
    }

    /// Resolve Gradle-declared dependencies for a Java file.
    /// Walks from the file's directory upward to repo root, stopping at the
    /// nearest owning Gradle build script (`build.gradle` Groovy DSL, or
    /// `build.gradle.kts` Kotlin DSL). Returns the declared dependency group
    /// ids (see [`extract_gradle_dependencies`] for the coordinate→name rule).
    ///
    /// Same lookup contract as [`Self::resolve_cargo_deps`] /
    /// [`Self::resolve_package_deps`]: first build script found wins; a broken
    /// or dependency-less leaf script does NOT inherit parent deps. This mirrors
    /// the cargo/npm nearest-manifest rule, so a Gradle subproject resolves to
    /// its own `build.gradle` and does not merge the root's
    /// `subprojects {}`/`allprojects {}` blocks — the same limitation the base
    /// cargo reader has for workspace-inherited deps (see the module note).
    pub fn resolve_gradle_deps(
        &mut self,
        file_rel_path: &str,
        repo_root: &Path,
    ) -> PackageDependencySet {
        let empty = PackageDependencySet { names: vec![] };
        let dir = parent_dir(file_rel_path);

        let mut probe = dir.clone();
        loop {
            if let Some(cached) = self.gradle_cache.get(&probe) {
                let result = cached.clone();
                self.gradle_cache.insert(dir.clone(), result.clone());
                return result;
            }

            let abs_dir = if probe.is_empty() {
                repo_root.to_path_buf()
            } else {
                repo_root.join(&probe)
            };
            // Prefer the Groovy DSL (`build.gradle`); fall back to the Kotlin
            // DSL (`build.gradle.kts`) in the same directory. Either one marks
            // this directory as the owning manifest (walk stops here).
            let groovy = abs_dir.join("build.gradle");
            let kotlin = abs_dir.join("build.gradle.kts");
            let manifest = if groovy.exists() {
                Some(groovy)
            } else if kotlin.exists() {
                Some(kotlin)
            } else {
                None
            };
            if let Some(manifest_path) = manifest {
                let deps = std::fs::read_to_string(&manifest_path)
                    .ok()
                    .and_then(|content| extract_gradle_dependencies(&content))
                    .unwrap_or_else(|| empty.clone());
                self.gradle_cache.insert(probe.clone(), deps.clone());
                self.gradle_cache.insert(dir.clone(), deps.clone());
                return deps;
            }

            if probe.is_empty() {
                break;
            }
            probe = parent_dir(&probe);
        }

        self.gradle_cache.insert(dir, empty.clone());
        empty
    }
}

/// Get the parent directory of a repo-relative path.
fn parent_dir(rel_path: &str) -> String {
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

/// Extract declared dependency GROUP IDs from a Gradle build script —
/// `build.gradle` (Groovy DSL) or `build.gradle.kts` (Kotlin DSL).
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
/// "dependency not identified" bucket rather than being force-attributed. This
/// is the deliberate, VISION-aligned degradation (unknown never fabricated),
/// not a bug — the group-vs-namespace gap is real and named.
///
/// ## Parsing (single character pass, no build-tool dependency — mirrors cargo)
///
/// Comments (`//`, `/* */`) are stripped first (a commented-out dependency must
/// never be captured). The cleaned source is then walked in ONE character pass
/// that maintains a brace stack — each `{` tagged with whether the word right
/// before it is `dependencies` — so at every character we know whether we are
/// inside a `dependencies { … }` block (nesting handled:
/// `subprojects { dependencies { … } }`). Characters that fall INSIDE a
/// dependencies block are buffered per line and mined for coordinates at each
/// line boundary, in two forms:
///   - **string form** — `implementation 'g:a:v'` / `api("g:a:v")` (Groovy or
///     Kotlin, single or double quotes, parens optional). Any quoted token
///     shaped `group:artifact[:version…]` contributes its group.
///   - **map form** — Groovy `group: 'g', name: 'a', version: 'v'` or Kotlin
///     `group = "g", name = "a"`. The `group` value is captured ONLY when a
///     `name` key is also present, which distinguishes a dependency declaration
///     from an `exclude group: 'g', module: 'm'` closure (exclusions use
///     `module`, never `name`) — so excluded groups are not mistaken for deps.
///
/// Buffering the in-block characters (rather than testing block state only at
/// each line's START) is what lets a ONE-LINE block —
/// `dependencies { implementation 'g:a:v' }` — be mined: the coordinate sits
/// between the `{` and `}` on the same line, and only the text between them is
/// buffered, so a coordinate-shaped token OUTSIDE the block on that line is not
/// captured.
///
/// The scan is verb-agnostic inside the block: any line with a valid coordinate
/// is a dependency, so ALL configuration verbs (`implementation`, `api`,
/// `compileOnly`, `runtimeOnly`, `testImplementation`, `checkstyle`,
/// `annotationProcessor`, and project-defined custom configurations) are
/// covered without an enumerated allowlist.
///
/// Returns `None` when the source is malformed or carries no coordinate:
///   - an UNCLOSED dependencies block (a `dependencies {` with no matching `}`
///     at end of input) — the block's extent is untrustworthy, so its
///     coordinates are discarded rather than guessed (honest degradation);
///   - no coordinate found at all (empty/absent `dependencies` block, or a
///     script using only version catalogs / project deps).
///
/// Either way a broken or dependency-less leaf script yields no set, and the
/// walk-up resolver treats that as "owns the manifest, empty deps" — never
/// inherits a parent (same broken-leaf rule as cargo/npm).
///
/// ## Known limitations (honest degradation, documented)
///   - Version catalogs (`implementation libs.guava` / `libraries.guava`),
///     `project(':core')` deps, and the `kotlin("stdlib")` helper carry no
///     literal coordinate on the line → not resolved (no fabrication).
///   - A subproject script does not inherit the root's `subprojects {}` /
///     `allprojects {}` dependency blocks (nearest-manifest rule, same as the
///     base cargo reader).
///   - Brace tracking is not string-aware (matches the prior behavior): a
///     `{`/`}` inside a coordinate's `${…}` version interpolation is balanced
///     and harmless, but a lone brace inside a string literal would miscount.
///     Coordinate strings never contain lone braces, so real dependency blocks
///     are unaffected.
pub fn extract_gradle_dependencies(content: &str) -> Option<PackageDependencySet> {
    let cleaned = strip_gradle_comments(content);
    let mut names = BTreeSet::new();

    // Brace stack: one bool per open `{`, true iff it opened a `dependencies`
    // block. `deps_frames` counts the `true` frames on the stack, so it is > 0
    // exactly while we are inside ≥1 dependencies block (nesting handled).
    let mut brace_stack: Vec<bool> = Vec::new();
    let mut deps_frames: usize = 0;
    // In-block characters seen so far on the current line, flushed to the
    // coordinate miner at each line boundary. Only the portion of a line inside
    // a dependencies block is buffered — this is what makes a one-line block
    // work and keeps out-of-block tokens on a block-opening line out.
    let mut in_block_line = String::new();
    // Word-boundary tracking for the sole purpose of tagging a `{` as a
    // dependencies block: `cur_word` accumulates the current word, `last_word`
    // holds the completed word immediately before the current position. Reset
    // at each newline, preserving the documented "same-line `dependencies {`
    // only" rule (an Allman-style `{` on the next line is not a deps block).
    let mut cur_word = String::new();
    let mut last_word = String::new();

    for ch in cleaned.chars() {
        match ch {
            '\n' => {
                if !in_block_line.is_empty() {
                    extract_gradle_coordinates(&in_block_line, &mut names);
                    in_block_line.clear();
                }
                cur_word.clear();
                last_word.clear();
            }
            '{' => {
                if !cur_word.is_empty() {
                    last_word = std::mem::take(&mut cur_word);
                }
                let is_deps = last_word == "dependencies";
                brace_stack.push(is_deps);
                if is_deps {
                    deps_frames += 1;
                }
                last_word.clear();
            }
            '}' => {
                if !cur_word.is_empty() {
                    last_word = std::mem::take(&mut cur_word);
                }
                if let Some(was_deps) = brace_stack.pop() {
                    if was_deps {
                        deps_frames = deps_frames.saturating_sub(1);
                    }
                }
                last_word.clear();
            }
            c => {
                if c.is_ascii_alphanumeric() || c == '_' {
                    cur_word.push(c);
                } else if !cur_word.is_empty() {
                    last_word = std::mem::take(&mut cur_word);
                }
                if deps_frames > 0 {
                    in_block_line.push(c);
                }
            }
        }
    }
    // Flush a trailing (newline-less) final line.
    if !in_block_line.is_empty() {
        extract_gradle_coordinates(&in_block_line, &mut names);
    }

    // An unclosed dependencies block (still inside one at end of input) is
    // malformed: the block's extent is untrustworthy, so discard everything and
    // degrade honestly rather than trust coordinates from a block whose end we
    // had to guess.
    if deps_frames > 0 {
        return None;
    }
    if names.is_empty() {
        return None;
    }
    Some(PackageDependencySet {
        names: names.into_iter().collect(),
    })
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
            deps.names,
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
            deps.names,
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
        assert_eq!(deps.names, vec!["com.google.guava"]);
        assert!(!deps.names.contains(&"guava".to_string()));
        assert!(!deps
            .names
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
            deps.names,
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
            extract_gradle_dependencies(groovy).unwrap().names,
            vec!["com.google.guava"]
        );

        let kotlin = r#"
dependencies {
    implementation(group = "org.apache.commons", name = "commons-lang3", version = "3.14.0")
}
"#;
        assert_eq!(
            extract_gradle_dependencies(kotlin).unwrap().names,
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
        assert_eq!(deps.names, vec!["org.mockito"]);
        assert!(
            !deps.names.contains(&"net.bytebuddy".to_string()),
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
        assert_eq!(deps.names, vec!["real"]);
        assert!(!deps.names.iter().any(|n| n == "commented"));
        assert!(!deps.names.iter().any(|n| n == "not"));
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
        assert_eq!(deps.names, vec!["org.real"]);
        // Plugin id (no colon), project group (no colon), and the repo URL
        // (`https` split, artifact segment has `/`) are all excluded.
        assert!(!deps.names.iter().any(|n| n == "https"));
        assert!(!deps
            .names
            .iter()
            .any(|n| n == "org.springframework.samples"));
    }

    /// A dependencies block nested under `subprojects { … }` is still mined.
    #[test]
    fn gradle_nested_subprojects_block() {
        let content = r#"
subprojects {
    dependencies {
        implementation 'io.grpc:grpc-core:1.60.0'
    }
}
"#;
        assert_eq!(
            extract_gradle_dependencies(content).unwrap().names,
            vec!["io.grpc"]
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
                .names,
            vec!["com.example"]
        );
        // Kotlin DSL, parens + double quotes, no trailing newline (final flush).
        assert_eq!(
            extract_gradle_dependencies(
                "dependencies { implementation(\"io.grpc:grpc-core:1.60\") }"
            )
            .unwrap()
            .names,
            vec!["io.grpc"]
        );
        // A coordinate-shaped token OUTSIDE the block, on the same line as the
        // one-line dependencies block, must NOT be captured.
        assert_eq!(
            extract_gradle_dependencies(
                "task x { doFirst { println 'a:b:c' } } ; dependencies { implementation 'org.real:d:1.0' }\n"
            )
            .unwrap()
            .names,
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
            extract_gradle_dependencies(closed).unwrap().names,
            vec!["com.example"]
        );
    }

    // ── RepoConfigContext for Gradle ─────────────────────────

    /// Nearest owning build script wins; `build.gradle.kts` (Kotlin DSL) is
    /// resolved as well as `build.gradle` (Groovy).
    #[test]
    fn nearest_ancestor_gradle() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Root Groovy build script.
        fs::write(
            root.join("build.gradle"),
            "dependencies {\n  implementation 'org.root:dep:1.0'\n}\n",
        )
        .unwrap();

        // Nested subproject with a Kotlin DSL build script.
        fs::create_dir_all(root.join("web/src/main/java/app")).unwrap();
        fs::write(
            root.join("web/build.gradle.kts"),
            "dependencies {\n  implementation(\"org.web:dep:1.0\")\n}\n",
        )
        .unwrap();

        let mut ctx = RepoConfigContext::new();

        // File under root → root's Groovy deps.
        let root_deps = ctx.resolve_gradle_deps("src/main/java/App.java", root);
        assert_eq!(root_deps.names, vec!["org.root"]);

        // File under web/ → the nearest (Kotlin DSL) build script's deps.
        let web_deps = ctx.resolve_gradle_deps("web/src/main/java/app/Web.java", root);
        assert_eq!(web_deps.names, vec!["org.web"]);
    }

    /// A build script with no resolvable dependencies does NOT inherit the
    /// parent's deps — the broken-leaf-no-inherit rule shared with cargo/npm.
    #[test]
    fn gradle_leaf_without_deps_does_not_inherit_parent() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::write(
            root.join("build.gradle"),
            "dependencies {\n  implementation 'org.root:dep:1.0'\n}\n",
        )
        .unwrap();

        // Subproject build script that declares only catalog refs (no literal
        // coordinate) — its own manifest, so it stops the walk with empty deps.
        fs::create_dir_all(root.join("leaf/src/main/java")).unwrap();
        fs::write(
            root.join("leaf/build.gradle"),
            "dependencies {\n  implementation libs.guava\n}\n",
        )
        .unwrap();

        let mut ctx = RepoConfigContext::new();
        let leaf_deps = ctx.resolve_gradle_deps("leaf/src/main/java/Leaf.java", root);
        assert!(
            leaf_deps.names.is_empty(),
            "leaf build.gradle owns the manifest; must not inherit root's org.root, got {:?}",
            leaf_deps.names
        );
    }
}
