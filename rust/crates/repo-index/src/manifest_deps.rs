//! Manifest dependency provenance + the pyproject.toml reader (DEPS-LIST-REWRITE-1 §2.2;
//! operator ruling 2026-08-26).
//!
//! Two crate-private concerns, factored OUT of the 1844-line `config.rs` (gap 7 — new logic goes
//! in crate-private modules, never grows the god-file):
//!
//! 1. [`ManifestProvenanceCollector`] — accumulates `{path, dir, ecosystem}` for every manifest a
//!    deps resolver actually PARSED, so query time can render the exact file (`build.gradle.kts` as
//!    itself) instead of a fabricated fixed-name guess. Serialized into the extraction-diagnostics
//!    blob (the `deps_manifests` key) BEFORE the Ready flip, riding the same key-agnostic merge as
//!    `index_basis`. Sole current users: the four `RepoConfigContext` deps resolvers (write) and
//!    `compose::index_options_diagnostic` (serialize). Axis: one truth for "what manifest was parsed".
//!
//! 2. [`extract_pyproject_dependencies`] — the PEP 621 / Poetry line reader (no TOML dep, matching
//!    the Cargo/Gradle readers). Sole user: `RepoConfigContext::resolve_pyproject_deps`.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use repo_graph_classification::types::PackageDependencySet;
use serde::Serialize;

use crate::config::{extract_gradle_dependencies, parent_dir, RepoConfigContext};

// ── Relocated manifest resolvers (guardrail: config.rs is not grown by this slice) ──────────────
//
// The Java (Gradle) and Python (pyproject) nearest-manifest RESOLVERS live here rather than in
// config.rs. They keep the exact npm/cargo nearest-manifest contract (first owning manifest wins; a
// dependency-less leaf does NOT inherit parent deps) and reach `RepoConfigContext`'s `gradle_cache`
// / `pyproject_cache` (both `pub(crate)`) plus its `record_parsed_manifest` / `parent_dir`. Call
// sites in `compose::prepare_repo_inputs` are unchanged — these are methods on the same struct.
impl RepoConfigContext {
    /// Resolve Gradle-declared dependencies for a Java file. Walks upward to the nearest owning
    /// build script (`build.gradle` Groovy DSL, else `build.gradle.kts` Kotlin DSL) and returns the
    /// declared dependency group ids (see [`extract_gradle_dependencies`]).
    ///
    /// STANDING HONESTY RULE (sweep): gate on the READ, not `.exists()` + `.ok()`. `NotFound` on
    /// BOTH candidates in a directory → keep walking; a present-but-unreadable script → stop, warn,
    /// degrade to empty WITH a reason, and record the ACTUAL file as a FAILED manifest (review-4
    /// item 1 — `build.gradle.kts` renders as itself with its unreadable reason, never a fixed-name
    /// guess and never a fabricated parsed zero-dep).
    pub(crate) fn resolve_gradle_deps(
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
            // Prefer the Groovy DSL, then the Kotlin DSL. `NotFound` on the first tries the second;
            // any other error on either is a present-but-unreadable manifest (stop here).
            let mut chosen: Option<(std::path::PathBuf, std::io::Result<String>)> = None;
            for cand in [
                abs_dir.join("build.gradle"),
                abs_dir.join("build.gradle.kts"),
            ] {
                match std::fs::read_to_string(&cand) {
                    Ok(content) => {
                        chosen = Some((cand, Ok(content)));
                        break;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(e) => {
                        chosen = Some((cand, Err(e)));
                        break;
                    }
                }
            }
            match chosen {
                Some((path, Ok(content))) => {
                    // `None` = the build script parsed but declared no `group:artifact` coordinates
                    // → a measured zero-dep. KNOWN best-effort limit (VISION 80%): a script that
                    // declares deps ONLY via a version catalog (`libs.foo`) reads as zero here, so
                    // those imports render `observed_but_undeclared` (a false-NEGATIVE attribution,
                    // never a fabricated file or a false-certainty "unused"). Unlike pyproject, a
                    // build.gradle has no off-manifest dep source (requirements.txt/setup.py), so
                    // this is a resolution gap, not the metadata-eligibility question pyproject faces.
                    let deps =
                        extract_gradle_dependencies(&content).unwrap_or_else(|| empty.clone());
                    let file_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("build.gradle");
                    self.record_parsed_manifest(&probe, file_name, "java");
                    self.gradle_cache.insert(probe.clone(), deps.clone());
                    self.gradle_cache.insert(dir.clone(), deps.clone());
                    return deps;
                }
                Some((path, Err(e))) => {
                    eprintln!(
                        "warning: {} unreadable ({}); declared deps unknown",
                        path.display(),
                        e
                    );
                    let file_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("build.gradle");
                    // review-4 item 1: PRESENT but unreadable — record as FAILED (not parsed), so
                    // query time renders unknown-with-reason, never a fabricated parsed zero-dep.
                    self.record_failed_manifest(
                        &probe,
                        file_name,
                        "java",
                        format!("unreadable: {e}"),
                    );
                    self.gradle_cache.insert(probe.clone(), empty.clone());
                    self.gradle_cache.insert(dir.clone(), empty.clone());
                    return empty;
                }
                None => {}
            }

            if probe.is_empty() {
                break;
            }
            probe = parent_dir(&probe);
        }

        self.gradle_cache.insert(dir, empty.clone());
        empty
    }

    /// Resolve pyproject-declared dependencies for a Python file (DEPS-LIST-REWRITE-1 §2.2). Walks
    /// upward to the nearest owning `pyproject.toml` and returns the declared distribution names
    /// from `[project].dependencies` (PEP 621) and `[tool.poetry.dependencies]` (see
    /// [`extract_pyproject_dependencies`]). `requirements*.txt` / `setup.py` are named extension
    /// points, not read here.
    ///
    /// STANDING HONESTY RULE (review-5 item 1): `io NotFound` (truly absent) → keep walking; any
    /// other read error (the manifest EXISTS but is unreadable) → stop, warn, record a FAILED
    /// manifest, degrade to empty. A PRESENT manifest is then metadata-gated by
    /// [`extract_pyproject_dependencies`]: [`PyprojectDeps::Declared`] (a dep-declaring construct was
    /// found, possibly empty) records PARSED — a legitimate measured-empty; [`PyprojectDeps::
    /// Ineligible`] (no readable construct, or `dynamic` deps) records FAILED with the reason. The
    /// declared-empty and the metadata-unknown cases are NEVER collapsed to the same value.
    pub(crate) fn resolve_pyproject_deps(
        &mut self,
        file_rel_path: &str,
        repo_root: &Path,
    ) -> PackageDependencySet {
        let empty = PackageDependencySet { names: vec![] };
        let dir = parent_dir(file_rel_path);

        let mut probe = dir.clone();
        loop {
            if let Some(cached) = self.pyproject_cache.get(&probe) {
                let result = cached.clone();
                self.pyproject_cache.insert(dir.clone(), result.clone());
                return result;
            }

            let abs_dir = if probe.is_empty() {
                repo_root.to_path_buf()
            } else {
                repo_root.join(&probe)
            };
            let pyproject_path = abs_dir.join("pyproject.toml");
            match std::fs::read_to_string(&pyproject_path) {
                Ok(content) => {
                    // This directory OWNS the manifest — the walk stops here regardless of the
                    // outcome (a broken/ineligible leaf does not inherit a parent's deps).
                    match extract_pyproject_dependencies(&content) {
                        // Construct present (possibly empty = a real measured zero-dep) → PARSED.
                        PyprojectDeps::Declared(deps) => {
                            self.record_parsed_manifest(&probe, "pyproject.toml", "python");
                            self.pyproject_cache.insert(probe.clone(), deps.clone());
                            self.pyproject_cache.insert(dir.clone(), deps.clone());
                            return deps;
                        }
                        // review-5 item 1: metadata-ineligible / dynamic → the manifest is PRESENT
                        // but its declared deps are UNKNOWN. Record FAILED with the reason so query
                        // time renders unknown-with-reason, NEVER a fabricated parsed zero-dep.
                        PyprojectDeps::Ineligible { reason } => {
                            eprintln!(
                                "warning: pyproject.toml at {} — {reason}; declared deps unknown",
                                pyproject_path.display()
                            );
                            self.record_failed_manifest(&probe, "pyproject.toml", "python", reason);
                            self.pyproject_cache.insert(probe.clone(), empty.clone());
                            self.pyproject_cache.insert(dir.clone(), empty.clone());
                            return empty;
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    eprintln!(
                        "warning: pyproject.toml at {} unreadable ({}); declared deps unknown",
                        pyproject_path.display(),
                        e
                    );
                    // review-4 item 1: PRESENT but unreadable → FAILED record (unknown-with-reason),
                    // never a fabricated parsed zero-dep.
                    self.record_failed_manifest(
                        &probe,
                        "pyproject.toml",
                        "python",
                        format!("unreadable: {e}"),
                    );
                    self.pyproject_cache.insert(probe.clone(), empty.clone());
                    self.pyproject_cache.insert(dir.clone(), empty.clone());
                    return empty;
                }
            }

            if probe.is_empty() {
                break;
            }
            probe = parent_dir(&probe);
        }

        self.pyproject_cache.insert(dir, empty.clone());
        empty
    }

    /// Record one successfully PARSED manifest for provenance (§2.2). `dir` is the manifest's
    /// repo-relative directory (empty = repo root); `file_name` is the manifest's basename. The
    /// repo-relative manifest path is `dir/file_name` (or just `file_name` at root). Lives here
    /// beside the collector it wraps (relocated from config.rs so this slice does not grow that
    /// god-file); called from the npm/cargo readers in config.rs and the Gradle/pyproject readers
    /// above — all methods on the same struct.
    pub(crate) fn record_parsed_manifest(&mut self, dir: &str, file_name: &str, ecosystem: &str) {
        let path = if dir.is_empty() {
            file_name.to_string()
        } else {
            format!("{dir}/{file_name}")
        };
        self.manifest_provenance
            .record(path, dir.to_string(), ecosystem);
    }

    /// Record one manifest that was PRESENT but could NOT be parsed (review-4 item 1) — an io read
    /// error or malformed content. `reason` rides the `deps_manifests` wire record so query time
    /// renders unknown-with-reason instead of a fabricated parsed zero-dep. Same `dir/file_name`
    /// path derivation as [`Self::record_parsed_manifest`].
    pub(crate) fn record_failed_manifest(
        &mut self,
        dir: &str,
        file_name: &str,
        ecosystem: &str,
        reason: String,
    ) {
        let path = if dir.is_empty() {
            file_name.to_string()
        } else {
            format!("{dir}/{file_name}")
        };
        self.manifest_provenance
            .record_failed(path, dir.to_string(), ecosystem, reason);
    }
}

/// One manifest-provenance record. Field names (`path`/`dir`/`ecosystem`/`error`) are the wire
/// contract read back by `repo_graph_module_queries::ManifestProvenance` at query time.
///
/// Renamed from `ParsedManifestRecord` (review-4 item 1): the collector now holds BOTH successfully
/// parsed manifests (`error == None`) AND manifests that were PRESENT but unreadable/malformed
/// (`error == Some(reason)`), so the old "Parsed" name lied about the failed entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManifestRecord {
    /// Repo-relative path of the manifest file the resolver encountered.
    pub path: String,
    /// Repo-relative directory the manifest governs (module attribution key at query time).
    pub dir: String,
    /// Ecosystem the reader belongs to (`npm`/`cargo`/`python`/`java`).
    pub ecosystem: String,
    /// `None` = read AND parsed (declared deps possibly empty — a legitimate measured-empty).
    /// `Some(reason)` = PRESENT but not parseable (io read error, or malformed content the reader
    /// could detect). Query time renders the `Some` case as unknown-with-reason, never a `Parsed`
    /// zero-dep (review-4 item 1). Omitted from the wire when `None` (backward-compatible).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Accumulates the manifests a repo's deps resolvers encountered, deduped by path.
#[derive(Debug, Clone, Default)]
pub struct ManifestProvenanceCollector {
    seen: HashSet<String>,
    records: Vec<ManifestRecord>,
}

impl ManifestProvenanceCollector {
    /// Record one successfully PARSED manifest (idempotent by `path`). `dir` is the manifest's
    /// repo-relative directory (empty string = repo root). A parsed manifest with zero declared
    /// deps is still recorded here (ruling-3 item 3: parsed ≠ produced-rows).
    pub fn record(&mut self, path: String, dir: String, ecosystem: &str) {
        if self.seen.insert(path.clone()) {
            self.records.push(ManifestRecord {
                path,
                dir,
                ecosystem: ecosystem.to_string(),
                error: None,
            });
        }
    }

    /// Record one manifest that was PRESENT but could NOT be parsed (review-4 item 1). The `reason`
    /// (io error / malformed) rides the same `deps_manifests` wire record so query time can render
    /// it as unknown-with-reason instead of a fabricated parsed zero-dep. Idempotent by `path`.
    pub fn record_failed(&mut self, path: String, dir: String, ecosystem: &str, reason: String) {
        if self.seen.insert(path.clone()) {
            self.records.push(ManifestRecord {
                path,
                dir,
                ecosystem: ecosystem.to_string(),
                error: Some(reason),
            });
        }
    }

    /// Borrow the accumulated records in insertion order.
    pub fn records(&self) -> &[ManifestRecord] {
        &self.records
    }
}

// ── pyproject.toml reader (DEPS-LIST-REWRITE-1 §2.2) ──────────────

/// Outcome of reading `pyproject.toml` dependency metadata (review-5 item 1 — metadata-gated,
/// unknown-with-reason).
///
/// Why an enum and not `Option`: a bare `None` cannot distinguish a manifest that DECLARES zero
/// deps (`dependencies = []` — a real measured empty) from a manifest whose deps this line reader
/// simply cannot see. Cargo.toml/build.gradle are self-contained formats where an absent dependency
/// section genuinely means zero deps; a `pyproject.toml` is NOT — its deps may live in constructs
/// this reader does not parse (`requirements*.txt`, `setup.py`/`setup.cfg`, or PEP 621 `dynamic`
/// metadata). Collapsing "declared nothing" and "declared elsewhere" to the same value would let a
/// read failure be laundered into a `Parsed` zero-dep provenance (the reviewer's OBSERVED defect).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PyprojectDeps {
    /// A dependency-declaring construct we read WAS present: a PEP 621 `[project].dependencies`
    /// array or a `[tool.poetry.dependencies]` table. The set is the parsed distribution names —
    /// possibly EMPTY, which is a legitimate measured zero-dep (`dependencies = []`).
    Declared(PackageDependencySet),
    /// No construct we read was present, or dependencies were declared `dynamic`. The manifest is
    /// metadata-ineligible for this line reader; its declared deps are UNKNOWN (`reason` says why),
    /// so the caller records it as a FAILED manifest — never a fabricated parsed zero-dep.
    Ineligible { reason: String },
}

/// Extract declared distribution names from `pyproject.toml` (metadata-gated — see [`PyprojectDeps`]).
///
/// Reads two sources, both line-parsed (no TOML dependency, matching the Cargo/Gradle readers):
///   - PEP 621 `[project]` `dependencies = [ "asgiref>=3.8.1", ... ]` — a (possibly multi-line)
///     array of PEP 508 requirement strings; the distribution name is the leading token before any
///     version/extra/marker/url delimiter.
///   - `[tool.poetry.dependencies]` — a table of `name = "^ver"` lines (the `python` entry, which
///     is the interpreter constraint not a dependency, is skipped).
///
/// Returns [`PyprojectDeps::Declared`] (possibly empty) when either construct is PRESENT, else
/// [`PyprojectDeps::Ineligible`] with the reason — including PEP 621 `dynamic = ["dependencies"]`,
/// which explicitly defers deps to a build backend this reader cannot evaluate.
///
/// Distribution names are lower-cased so they line up with `normalize_python_specifier`'s
/// lower-cased import heads. NOT read (named extension points, not scope): `[project.optional-
/// dependencies]` extras, `requirements*.txt`, `setup.py`/`setup.cfg`.
///
/// Best-effort by design (VISION: 80% right for module discovery): PyPI distribution names and
/// import module names diverge for some packages (`beautifulsoup4` → `bs4`), which no line parser
/// can bridge — such a dep renders `declared_but_unobserved`, honestly.
pub fn extract_pyproject_dependencies(content: &str) -> PyprojectDeps {
    let mut names: BTreeSet<String> = BTreeSet::new();
    let mut current_section = "";
    // Tracks whether we are inside the `dependencies = [ ... ]` array of `[project]`.
    let mut in_project_deps_array = false;
    // Eligibility gate (review-5 item 1): did we SEE a construct that declares deps?
    let mut saw_project_deps_array = false;
    let mut saw_poetry_table = false;
    // PEP 621 `dynamic = [ ... "dependencies" ... ]` — deps deferred to the build backend.
    let mut deps_are_dynamic = false;

    for raw_line in content.lines() {
        let line = raw_line.trim();

        // Section header.
        if line.starts_with('[') && line.ends_with(']') {
            current_section = &line[1..line.len() - 1];
            in_project_deps_array = false;
            if current_section == "tool.poetry.dependencies" {
                saw_poetry_table = true;
            }
            continue;
        }

        // PEP 621: [project] dependencies = [ ... ]
        if current_section == "project" {
            // `dynamic = [...]` listing "dependencies" means the static array is intentionally
            // absent — deps are UNKNOWN to a static reader (never a measured zero-dep).
            if let Some(rest) = line.strip_prefix("dynamic") {
                if let Some(after_eq) = rest.trim_start().strip_prefix('=') {
                    if after_eq.contains("\"dependencies\"") || after_eq.contains("'dependencies'")
                    {
                        deps_are_dynamic = true;
                    }
                    continue;
                }
            }
            if let Some(rest) = line.strip_prefix("dependencies") {
                // `dependencies = [` — possibly with entries on the same line.
                if let Some(after_eq) = rest.trim_start().strip_prefix('=') {
                    let after = after_eq.trim_start();
                    if let Some(rest) = after.strip_prefix('[') {
                        saw_project_deps_array = true;
                        in_project_deps_array = true;
                        // Entries may follow `[` on the same line.
                        collect_pep508_names(rest, &mut names, &mut in_project_deps_array);
                    }
                    continue;
                }
            }
            if in_project_deps_array {
                collect_pep508_names(line, &mut names, &mut in_project_deps_array);
                continue;
            }
        }

        // Poetry: [tool.poetry.dependencies] name = "..."
        if current_section == "tool.poetry.dependencies" {
            if let Some(eq_pos) = line.find('=') {
                let key = line[..eq_pos].trim().trim_matches('"');
                if !key.is_empty() && key != "python" && !key.contains(' ') {
                    names.insert(key.to_ascii_lowercase());
                }
            }
            continue;
        }
    }

    // A construct was present (even if empty) → measured, possibly zero. `dynamic` deps with no
    // static array present is NOT measured — it is ineligible.
    if saw_project_deps_array || saw_poetry_table {
        return PyprojectDeps::Declared(PackageDependencySet {
            names: names.into_iter().collect(),
        });
    }
    if deps_are_dynamic {
        return PyprojectDeps::Ineligible {
            reason: "dependencies declared `dynamic` — deferred to the build backend, not \
                     statically readable"
                .to_string(),
        };
    }
    PyprojectDeps::Ineligible {
        reason: "no [project].dependencies array or [tool.poetry.dependencies] table found \
                 (deps may live in requirements*.txt / setup.py — named extension points)"
            .to_string(),
    }
}

/// Collect PEP 508 distribution names from a fragment of a `dependencies` array. Sets
/// `in_array = false` when the closing `]` is seen. Each comma/quote-delimited entry contributes
/// its leading requirement name (up to the first version/extra/marker/url character).
fn collect_pep508_names(fragment: &str, names: &mut BTreeSet<String>, in_array: &mut bool) {
    let mut frag = fragment;
    if let Some(close) = frag.find(']') {
        *in_array = false;
        frag = &frag[..close];
    }
    for raw_entry in frag.split(',') {
        let entry = raw_entry
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .trim();
        if entry.is_empty() {
            continue;
        }
        // Distribution name = leading run up to the first PEP 508 delimiter.
        let name: String = entry
            .chars()
            .take_while(|c| {
                !c.is_whitespace()
                    && !matches!(
                        *c,
                        '<' | '>' | '=' | '!' | '~' | ';' | '[' | '(' | '@' | ','
                    )
            })
            .collect();
        if !name.is_empty() {
            names.insert(name.to_ascii_lowercase());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collector_dedupes_by_path() {
        let mut c = ManifestProvenanceCollector::default();
        c.record("b/pyproject.toml".into(), "b".into(), "python");
        c.record("a/pyproject.toml".into(), "a".into(), "python");
        c.record("b/pyproject.toml".into(), "b".into(), "python"); // dup ignored
        let paths: Vec<&str> = c.records().iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths.len(), 2, "duplicate path must be recorded once");
        assert!(paths.contains(&"a/pyproject.toml"));
        assert!(paths.contains(&"b/pyproject.toml"));
    }

    #[test]
    fn collector_record_failed_carries_reason() {
        // review-4 item 1: a present-but-unreadable/malformed manifest is recorded with its reason,
        // NOT as a clean parsed record.
        let mut c = ManifestProvenanceCollector::default();
        c.record_failed(
            "a/package.json".into(),
            "a".into(),
            "npm",
            "malformed: not a valid JSON object".into(),
        );
        let r = &c.records()[0];
        assert_eq!(r.path, "a/package.json");
        assert_eq!(
            r.error.as_deref(),
            Some("malformed: not a valid JSON object")
        );
    }

    #[test]
    fn malformed_package_json_records_failed_not_parsed() {
        // review-4 item 1 REGRESSION: a package.json that does not parse as a JSON object must be
        // recorded as a FAILED manifest (error present), never a parsed zero-dep. The walk still
        // stops here (the directory owns the manifest) so it does not inherit a parent's deps.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("package.json"), "{ this is not valid json ").unwrap();

        let mut ctx = RepoConfigContext::new();
        let deps = ctx.resolve_package_deps("src/app.ts", root);
        assert!(deps.names.is_empty(), "malformed manifest yields no deps");
        let rec = ctx
            .manifest_records()
            .iter()
            .find(|r| r.path == "package.json")
            .expect("failed manifest recorded");
        assert!(
            rec.error.is_some(),
            "malformed package.json must record a failure reason, got {rec:?}"
        );
    }

    #[test]
    fn unreadable_manifest_records_failed_not_parsed() {
        // review-4 item 1 REGRESSION: an io read error that is NOT NotFound (here: `package.json`
        // is a DIRECTORY, so `read_to_string` returns an IsADirectory-class error) is a present-but-
        // unreadable manifest → recorded as FAILED with reason, never a parsed zero-dep.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join("package.json")).unwrap();

        let mut ctx = RepoConfigContext::new();
        let deps = ctx.resolve_package_deps("src/app.ts", root);
        assert!(deps.names.is_empty());
        let rec = ctx
            .manifest_records()
            .iter()
            .find(|r| r.path == "package.json")
            .expect("failed manifest recorded");
        let reason = rec.error.as_deref().expect("failure reason present");
        assert!(
            reason.starts_with("unreadable:"),
            "expected unreadable reason, got {reason:?}"
        );
    }

    /// Helper: unwrap a `Declared` outcome or panic (keeps the dep-content assertions terse).
    fn declared(content: &str) -> PackageDependencySet {
        match extract_pyproject_dependencies(content) {
            PyprojectDeps::Declared(d) => d,
            other => panic!("expected Declared, got {other:?}"),
        }
    }

    #[test]
    fn pyproject_pep621_project_dependencies_extracted() {
        // django's real shape: a `[project]` dependencies array of PEP 508 strings.
        let content = r#"
[project]
name = "Django"
dependencies = [
    "asgiref>=3.8.1",
    "sqlparse>=0.3.1",
    'tzdata; sys_platform == "win32"',
]
"#;
        let deps = declared(content);
        assert!(
            deps.names.contains(&"asgiref".to_string()),
            "{:?}",
            deps.names
        );
        assert!(
            deps.names.contains(&"sqlparse".to_string()),
            "{:?}",
            deps.names
        );
        assert!(
            deps.names.contains(&"tzdata".to_string()),
            "{:?}",
            deps.names
        );
    }

    #[test]
    fn pyproject_inline_array_and_poetry_table() {
        let content = r#"
[project]
dependencies = ["requests>=2", "click"]

[tool.poetry.dependencies]
python = "^3.11"
httpx = "^0.27"
"#;
        let deps = declared(content);
        assert!(
            deps.names.contains(&"requests".to_string()),
            "{:?}",
            deps.names
        );
        assert!(
            deps.names.contains(&"click".to_string()),
            "{:?}",
            deps.names
        );
        assert!(
            deps.names.contains(&"httpx".to_string()),
            "{:?}",
            deps.names
        );
        // The interpreter constraint is NOT a dependency.
        assert!(
            !deps.names.contains(&"python".to_string()),
            "{:?}",
            deps.names
        );
    }

    #[test]
    fn pyproject_empty_dependencies_array_is_declared_zero() {
        // review-5 item 1: `dependencies = []` is a real MEASURED zero-dep — the construct is
        // present, so this is Declared(empty), NOT Ineligible. It renders a `Parsed` provenance.
        let content = "[project]\nname = \"x\"\ndependencies = []\n";
        let deps = declared(content);
        assert!(deps.names.is_empty(), "{:?}", deps.names);
    }

    #[test]
    fn pyproject_without_any_deps_construct_is_ineligible() {
        // review-5 item 1: no `dependencies` array and no poetry table — the reader CANNOT know the
        // deps (they may be in requirements.txt/setup.py). UNKNOWN-with-reason, never a zero-dep.
        let content = "[project]\nname = \"x\"\n\n[build-system]\nrequires = [\"setuptools\"]\n";
        match extract_pyproject_dependencies(content) {
            PyprojectDeps::Ineligible { reason } => {
                assert!(reason.contains("no [project].dependencies"), "{reason}");
            }
            other => panic!("expected Ineligible, got {other:?}"),
        }
    }

    #[test]
    fn pyproject_dynamic_dependencies_is_ineligible() {
        // review-5 item 1: PEP 621 `dynamic = ["dependencies"]` defers deps to the build backend —
        // a static reader cannot see them. Ineligible with the dynamic reason, never a zero-dep.
        let content = "[project]\nname = \"x\"\ndynamic = [\"dependencies\"]\n";
        match extract_pyproject_dependencies(content) {
            PyprojectDeps::Ineligible { reason } => {
                assert!(reason.contains("dynamic"), "{reason}");
            }
            other => panic!("expected Ineligible, got {other:?}"),
        }
    }

    #[test]
    fn resolve_pyproject_ineligible_records_failed_not_parsed() {
        // review-5 item 1 REGRESSION (resolver level): a PRESENT pyproject.toml whose deps this
        // reader cannot see must record a FAILED manifest (unknown-with-reason), never a parsed
        // zero-dep. The walk still stops here (the directory owns the manifest).
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("pyproject.toml"),
            "[build-system]\nrequires = [\"setuptools\"]\n",
        )
        .unwrap();

        let mut ctx = RepoConfigContext::new();
        let deps = ctx.resolve_pyproject_deps("pkg/mod.py", root);
        assert!(deps.names.is_empty(), "ineligible manifest yields no deps");
        let rec = ctx
            .manifest_records()
            .iter()
            .find(|r| r.path == "pyproject.toml")
            .expect("failed manifest recorded");
        assert!(
            rec.error.is_some(),
            "metadata-ineligible pyproject must record a failure reason, got {rec:?}"
        );
    }

    #[test]
    fn resolve_pyproject_zero_dep_array_records_parsed() {
        // The measured-zero counterpart: `dependencies = []` is PARSED (error == None), so query
        // time renders its exact path — parsed ≠ produced-rows (ruling-3 item 3).
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("pyproject.toml"),
            "[project]\nname = \"x\"\ndependencies = []\n",
        )
        .unwrap();

        let mut ctx = RepoConfigContext::new();
        let deps = ctx.resolve_pyproject_deps("pkg/mod.py", root);
        assert!(deps.names.is_empty());
        let rec = ctx
            .manifest_records()
            .iter()
            .find(|r| r.path == "pyproject.toml")
            .expect("manifest recorded");
        assert!(
            rec.error.is_none(),
            "a parsed zero-dep manifest must NOT carry an error, got {rec:?}"
        );
    }

    // ── relocated resolver tests (moved with the resolvers from config.rs) ──

    /// Nearest owning build script wins; `build.gradle.kts` (Kotlin DSL) is resolved as well as
    /// `build.gradle` (Groovy).
    #[test]
    fn nearest_ancestor_gradle() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::write(
            root.join("build.gradle"),
            "dependencies {\n  implementation 'org.root:dep:1.0'\n}\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("web/src/main/java/app")).unwrap();
        std::fs::write(
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

    /// A build script with no resolvable dependencies does NOT inherit the parent's deps — the
    /// broken-leaf-no-inherit rule shared with cargo/npm.
    #[test]
    fn gradle_leaf_without_deps_does_not_inherit_parent() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::write(
            root.join("build.gradle"),
            "dependencies {\n  implementation 'org.root:dep:1.0'\n}\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("leaf/src/main/java")).unwrap();
        std::fs::write(
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

    #[test]
    fn resolve_pyproject_nearest_manifest_wins() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("pyproject.toml"),
            "[project]\ndependencies = [\"rootdep\"]\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("pkg")).unwrap();
        std::fs::write(
            root.join("pkg/pyproject.toml"),
            "[project]\ndependencies = [\"leafdep\"]\n",
        )
        .unwrap();

        let mut ctx = RepoConfigContext::new();
        let deps = ctx.resolve_pyproject_deps("pkg/mod.py", root);
        assert_eq!(deps.names, vec!["leafdep".to_string()]);
    }
}
