//! Manifest dependency provenance + the pyproject.toml reader (DEPS-LIST-REWRITE-1 §2.2;
//! operator ruling 2026-08-26).
//!
//! Crate-private concerns, factored OUT of `config.rs` (gap 7 — new logic goes in crate-private
//! modules, never grows the god-file):
//!
//! 1. The Java (Gradle) and Python (pyproject) declared-dependency RESOLVERS (methods on
//!    `RepoConfigContext`, below). pyproject keeps the nearest-manifest contract; Gradle attributes
//!    by project within its build (DEPS-GRADLE-CATALOG-1A; RG-REQ-006-L13): a Java file's declared
//!    set is what its settings build declares for the file's Gradle project, read from every
//!    project script of that build, with the nearest build script kept as its provenance.
//!
//! 2. [`ManifestProvenanceCollector`] — accumulates one [`ManifestRecord`] per manifest path a deps
//!    resolver recorded: PARSED, or FAILED with its reason and kind (the manifest's own read or
//!    parse failed, or Gradle declaration attribution could not be established), plus the
//!    per-build marking of the Gradle `dependencies` blocks the reader could not attribute. Query
//!    time renders the exact file (`build.gradle.kts` as itself) instead of a fabricated fixed-name
//!    guess, and a FAILED record as unknown-with-reason. Serialized into the extraction-diagnostics
//!    blob (the `deps_manifests` key) BEFORE the Ready flip, riding the same key-agnostic merge as
//!    `index_basis`. Users: the `RepoConfigContext` deps resolvers (write) and
//!    `compose::index_options_diagnostic` (serialize).
//!
//! 3. [`extract_pyproject_dependencies`] — the PEP 621 / Poetry line reader (no TOML dep, matching
//!    the Cargo/Gradle readers). Sole user: `RepoConfigContext::resolve_pyproject_deps`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use repo_graph_classification::types::PackageDependencySet;
use serde::Serialize;

use crate::config::{
    extract_gradle_dependencies, parent_dir, GradleScriptScopes, RepoConfigContext,
};

// ── Relocated manifest resolvers (guardrail: config.rs is not grown by this slice) ──────────────
//
// The Java (Gradle) and Python (pyproject) declared-dependency RESOLVERS live here rather than in
// config.rs. pyproject keeps the exact npm/cargo nearest-manifest contract (first owning manifest
// wins; a dependency-less leaf does NOT inherit parent deps); Gradle attributes by project within
// its settings build (see `resolve_gradle_deps`). They reach `RepoConfigContext`'s `gradle_cache`
// / `pyproject_cache` (both `pub(crate)`) plus its `record_parsed_manifest` / `parent_dir`. Call
// sites in `compose::prepare_repo_inputs` are unchanged — these are methods on the same struct.
impl RepoConfigContext {
    /// Resolve the Gradle-declared dependency groups of a Java file (DEPS-GRADLE-CATALOG-1A;
    /// RG-REQ-006-L13): what the file's Gradle BUILD declares FOR THE FILE'S PROJECT.
    ///
    /// - PROVENANCE is unchanged: the nearest ancestor build script (`build.gradle`, else
    ///   `build.gradle.kts`) is recorded as the file's manifest. A present-but-unreadable nearest
    ///   script is a FAILED record with its reason (STANDING HONESTY RULE: gate on the READ).
    /// - BUILD: among the settings files (`settings.gradle`, else `settings.gradle.kts`) at the
    ///   file's ancestor directories, outermost first, the first in which the deepest project
    ///   containing the file is a NON-root project; else the nearest ancestor settings file (the
    ///   file then sits in that build's root project). Projects and their roots come from the
    ///   public `repo_graph_indexer::parse_settings_gradle`, `projectDir` relocations honoured.
    /// - PROJECT R: the deepest project of that build whose root contains the file. Its declared
    ///   set is R's script's `own` ∪ the `allprojects` of R and of its Gradle-path ancestors ∪ the
    ///   `subprojects` of R's strict ancestors ∪ every project script's `project('<path>')` blocks
    ///   whose path (absolute, or relative to that script's project) is R's.
    /// - A file with no settings file above it, or whose nearest script lies below R's root in a
    ///   directory that is no project of the build (grpc-java `buildSrc/`), is a single-project
    ///   build rooted at the nearest script: its `own` ∪ its `allprojects`.
    /// - UNKNOWN, never guessed (RG-REQ-002-L11): a present-but-unreadable settings file, a
    ///   settings file with an unhandled `projectDir` form, or an unreadable project script of the
    ///   build makes the nearest script a FAILED record naming the cause; the set is empty.
    /// - MARKING (D-DGC-CONDITIONAL-1): the build's counted blocks (every project script of the
    ///   build, root project's first) ride exactly one record of the build — see
    ///   [`ManifestProvenanceCollector::offer_build_marking`].
    /// - NO ANCESTOR SCRIPT (D-DGC-ATTRIBUTION-1): a file with no ancestor build script but an
    ///   ancestor settings file is never answered by the absence of a script. It goes through the
    ///   same BUILD/PROJECT attribution and reads its project's declared set (another script's
    ///   `project(':P')` block included — the DECLARED option; there is no nearest script to record
    ///   as provenance). When that build's attribution is unknown, the build's settings file is a
    ///   FAILED record naming the cause. A file with no build script and no settings file above it
    ///   is in no Gradle build: the empty set, no record.
    pub(crate) fn resolve_gradle_deps(
        &mut self,
        file_rel_path: &str,
        repo_root: &Path,
    ) -> PackageDependencySet {
        let dir = parent_dir(file_rel_path);
        if let Some(cached) = self.gradle_cache.get(&dir) {
            return cached.clone();
        }
        let names = self.resolve_gradle_dir(&dir, repo_root);
        let deps = PackageDependencySet { names };
        self.gradle_cache.insert(dir, deps.clone());
        deps
    }

    /// The declared set of every Java file in directory `dir` (see [`Self::resolve_gradle_deps`]).
    fn resolve_gradle_dir(&mut self, dir: &str, repo_root: &Path) -> Vec<String> {
        // 1. Provenance: the nearest ancestor build script, exactly as before this slice.
        let mut probe = dir.to_string();
        let nearest = loop {
            match self.gradle_script_at(&probe, repo_root) {
                GradleScriptRead::Absent => {}
                found => break Some((probe.clone(), found)),
            }
            if probe.is_empty() {
                break None;
            }
            probe = parent_dir(&probe);
        };
        let Some((script_dir, script)) = nearest else {
            return self.resolve_gradle_dir_without_script(dir, repo_root);
        };
        let (file_name, scopes) = match script {
            GradleScriptRead::Read {
                file_name, scopes, ..
            } => (file_name, scopes),
            GradleScriptRead::Unreadable {
                path,
                file_name,
                reason,
            } => {
                eprintln!("warning: {path} {reason}; declared deps unknown");
                // review-4 item 1: PRESENT but unreadable — FAILED (not parsed), so query time
                // renders unknown-with-reason, never a fabricated parsed zero-dep.
                self.record_failed_manifest(&script_dir, &file_name, "java", reason);
                return Vec::new();
            }
            GradleScriptRead::Absent => return Vec::new(),
        };
        let script_path = join_rel(&script_dir, &file_name);

        // 2. Attribution by Gradle project within the file's Gradle build.
        match self.gradle_attribution(dir, Some(&script_dir), repo_root) {
            GradleAttribution::Failed { reason, .. } => {
                self.fail_gradle_record(&script_dir, &file_name, reason)
            }
            GradleAttribution::SingleScript => {
                self.record_parsed_manifest(&script_dir, &file_name, "java");
                if let Some(marking) =
                    marking_of(&[(script_path.as_str(), &scopes.undetermined_block_lines)])
                {
                    self.manifest_provenance.offer_build_marking(
                        &script_path,
                        &marking,
                        &script_path,
                    );
                }
                let mut names: BTreeSet<String> = scopes.own.iter().cloned().collect();
                names.extend(scopes.allprojects.iter().cloned());
                names.into_iter().collect()
            }
            GradleAttribution::Project {
                settings_dir,
                project,
            } => match self.gradle_build(&settings_dir, repo_root) {
                Err(reason) => self.fail_gradle_record(&script_dir, &file_name, reason),
                Ok(()) => {
                    let (build_key, names, marking) = {
                        let build = self.gradle_reads.built_mut(&settings_dir);
                        (
                            build.key.clone(),
                            build.declared_for(project),
                            build.marking.clone(),
                        )
                    };
                    self.record_parsed_manifest(&script_dir, &file_name, "java");
                    if let Some(marking) = marking {
                        self.manifest_provenance.offer_build_marking(
                            &build_key,
                            &marking,
                            &script_path,
                        );
                    }
                    names
                }
            },
        }
    }

    /// The declared set of every Java file in directory `dir`, which has NO ancestor build script
    /// (D-DGC-ATTRIBUTION-1; DGC-A01's ATTRIBUTION HONESTY rule (a) and (c)).
    ///
    /// - No settings file above `dir`: no Gradle build — the empty set, no record (rule (c)).
    /// - The file's project in a readable build: DECLARED — [`GradleBuild::declared_for`] of that
    ///   project, which already reads every project script's `project('<path>')` blocks and its
    ///   Gradle-path ancestors' `allprojects`/`subprojects` blocks. No provenance record is written:
    ///   provenance is the nearest build script, and there is none.
    /// - The build's attribution unknown (an unreadable settings file or project script, an
    ///   unhandled `projectDir` form): the settings file that defines (or fails to define) the
    ///   build is a FAILED `java` record naming the cause; its directory contains `dir`, so query
    ///   time renders the file's module unknown-with-reason — never a certain empty set.
    fn resolve_gradle_dir_without_script(&mut self, dir: &str, repo_root: &Path) -> Vec<String> {
        match self.gradle_attribution(dir, None, repo_root) {
            GradleAttribution::SingleScript => Vec::new(),
            GradleAttribution::Failed {
                reason,
                settings_dir,
                settings_file,
            } => self.fail_gradle_record(&settings_dir, &settings_file, reason),
            GradleAttribution::Project {
                settings_dir,
                project,
            } => match self.gradle_build(&settings_dir, repo_root) {
                Err(reason) => {
                    let settings_file = self.gradle_settings_file_name(&settings_dir, repo_root);
                    self.fail_gradle_record(&settings_dir, &settings_file, reason)
                }
                Ok(()) => self
                    .gradle_reads
                    .built_mut(&settings_dir)
                    .declared_for(project),
            },
        }
    }

    /// The basename of the settings file at `settings_dir` (`settings.gradle`, else
    /// `settings.gradle.kts`), as the cached read found it.
    fn gradle_settings_file_name(&mut self, settings_dir: &str, repo_root: &Path) -> String {
        match self.gradle_settings_at(settings_dir, repo_root) {
            GradleSettingsRead::Parsed { path, .. } => {
                path.rsplit('/').next().unwrap_or(&path).to_string()
            }
            GradleSettingsRead::Failed { file_name, .. } => file_name,
            GradleSettingsRead::Absent => "settings.gradle".to_string(),
        }
    }

    /// Record the manifest at `dir/file_name` as FAILED with an attribution `reason` (kind
    /// `attribution`, D-DGC-BOUNDARY-1 (9): Gradle project/declaration attribution could not be
    /// established — an unreadable settings file or project script of the build, or an unhandled
    /// `projectDir` form; the recorded file may be that settings file itself, so the kind makes no
    /// claim that it was read) and return the empty declared set —
    /// unknown-with-reason, never a guessed owner. A failure wins over an earlier PARSED record of
    /// the same path (D-DGC-ATTRIBUTION-1 rule (b)); the warning is printed once per path, when the
    /// path first becomes FAILED.
    fn fail_gradle_record(&mut self, dir: &str, file_name: &str, reason: String) -> Vec<String> {
        let path = join_rel(dir, file_name);
        if !self
            .manifest_provenance
            .records()
            .iter()
            .any(|r| r.path == path && r.error.is_some())
        {
            eprintln!("warning: {path} — {reason}; declared deps unknown");
        }
        self.record_attribution_failed_manifest(dir, file_name, "java", reason);
        Vec::new()
    }

    /// The build script at directory `dir` (cached per directory). `NotFound` on `build.gradle`
    /// tries `build.gradle.kts`; any other read error is a present-but-unreadable script.
    fn gradle_script_at(&mut self, dir: &str, repo_root: &Path) -> GradleScriptRead {
        if let Some(read) = self.gradle_reads.scripts.get(dir) {
            return read.clone();
        }
        let abs_dir = if dir.is_empty() {
            repo_root.to_path_buf()
        } else {
            repo_root.join(dir)
        };
        let mut read = GradleScriptRead::Absent;
        for file_name in ["build.gradle", "build.gradle.kts"] {
            match std::fs::read_to_string(abs_dir.join(file_name)) {
                Ok(content) => {
                    read = GradleScriptRead::Read {
                        path: join_rel(dir, file_name),
                        file_name: file_name.to_string(),
                        scopes: extract_gradle_dependencies(&content).unwrap_or_default(),
                    };
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => {
                    read = GradleScriptRead::Unreadable {
                        path: join_rel(dir, file_name),
                        file_name: file_name.to_string(),
                        reason: format!("unreadable: {e}"),
                    };
                    break;
                }
            }
        }
        self.gradle_reads
            .scripts
            .insert(dir.to_string(), read.clone());
        read
    }

    /// The settings file at directory `dir` (cached per directory): absent, parsed into its
    /// projects (root first, repo-relative roots), or FAILED (unreadable, or an unhandled
    /// `projectDir` form — every project root of that build is then unproven).
    fn gradle_settings_at(&mut self, dir: &str, repo_root: &Path) -> GradleSettingsRead {
        if let Some(read) = self.gradle_reads.settings.get(dir) {
            return read.clone();
        }
        let abs_dir = if dir.is_empty() {
            repo_root.to_path_buf()
        } else {
            repo_root.join(dir)
        };
        let mut read = GradleSettingsRead::Absent;
        for file_name in ["settings.gradle", "settings.gradle.kts"] {
            let path = join_rel(dir, file_name);
            match std::fs::read_to_string(abs_dir.join(file_name)) {
                Ok(content) => {
                    let parsed = repo_graph_indexer::parse_settings_gradle(&content, &path);
                    read = if parsed.unhandled_project_dirs > 0 {
                        GradleSettingsRead::Failed {
                            file_name: file_name.to_string(),
                            reason: format!(
                                "gradle project attribution unknown: {path} has {} unhandled `projectDir` form(s) — project roots unproven",
                                parsed.unhandled_project_dirs
                            ),
                        }
                    } else {
                        GradleSettingsRead::Parsed {
                            path,
                            projects: settings_projects(dir, &parsed),
                        }
                    };
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => {
                    read = GradleSettingsRead::Failed {
                        file_name: file_name.to_string(),
                        reason: format!(
                            "gradle project attribution unknown: {path} unreadable ({e})"
                        ),
                    };
                    break;
                }
            }
        }
        self.gradle_reads
            .settings
            .insert(dir.to_string(), read.clone());
        read
    }

    /// Which Gradle build and project own the Java files of directory `dir`, whose nearest build
    /// script lives at `script_dir` — `None` when no ancestor script exists (see
    /// [`Self::resolve_gradle_deps`] for the rule).
    fn gradle_attribution(
        &mut self,
        dir: &str,
        script_dir: Option<&str>,
        repo_root: &Path,
    ) -> GradleAttribution {
        let mut nearest_settings: Option<(String, usize)> = None;
        let mut chosen: Option<(String, usize)> = None;
        for settings_dir in ancestor_dirs_outermost_first(dir) {
            match self.gradle_settings_at(&settings_dir, repo_root) {
                GradleSettingsRead::Absent => {}
                GradleSettingsRead::Failed { reason, file_name } => {
                    return GradleAttribution::Failed {
                        reason,
                        settings_dir,
                        settings_file: file_name,
                    }
                }
                GradleSettingsRead::Parsed { projects, .. } => {
                    // The settings directory is an ancestor of `dir`, so its root project always
                    // contains the file; `deepest` is therefore always `Some`.
                    let Some(deepest) = deepest_project_containing(&projects, dir) else {
                        continue;
                    };
                    if !projects[deepest].is_root() {
                        chosen = Some((settings_dir, deepest));
                        break;
                    }
                    nearest_settings = Some((settings_dir, deepest));
                }
            }
        }
        let Some((settings_dir, project)) = chosen.or(nearest_settings) else {
            return GradleAttribution::SingleScript;
        };
        // The nearest script lies strictly below R's root, in a directory that is no project of
        // the build (R is the DEEPEST containing project) → its own single-project build.
        let r_dir = match self.gradle_settings_at(&settings_dir, repo_root) {
            GradleSettingsRead::Parsed { projects, .. } => projects[project].dir.clone(),
            _ => return GradleAttribution::SingleScript,
        };
        if let Some(script_dir) = script_dir {
            if script_dir != r_dir && dir_contains(&r_dir, script_dir) {
                return GradleAttribution::SingleScript;
            }
        }
        GradleAttribution::Project {
            settings_dir,
            project,
        }
    }

    /// Read every project script of the build defined at `settings_dir` once and cache the build
    /// (its projects, their scopes and its marking). `Err` when a project script is present but
    /// unreadable — its scope blocks are unknown, so the build's attribution is unknown.
    fn gradle_build(&mut self, settings_dir: &str, repo_root: &Path) -> Result<(), String> {
        if let Some(done) = self.gradle_reads.builds.get(settings_dir) {
            return done.as_ref().map(|_| ()).map_err(|e| e.clone());
        }
        let (key, projects) = match self.gradle_settings_at(settings_dir, repo_root) {
            GradleSettingsRead::Parsed { path, projects } => (path, projects),
            GradleSettingsRead::Failed { reason, .. } => return Err(reason),
            GradleSettingsRead::Absent => {
                return Err(format!(
                    "gradle project attribution unknown: settings file at `{settings_dir}` vanished"
                ))
            }
        };
        let mut scripts: Vec<Option<(String, GradleScriptScopes)>> =
            Vec::with_capacity(projects.len());
        let mut failure: Option<String> = None;
        for p in &projects {
            match self.gradle_script_at(&p.dir, repo_root) {
                GradleScriptRead::Absent => scripts.push(None),
                GradleScriptRead::Read { path, scopes, .. } => scripts.push(Some((path, scopes))),
                GradleScriptRead::Unreadable { path, reason, .. } => {
                    failure = Some(format!(
                        "gradle project attribution unknown: build script {path} of project {} {reason}",
                        p.gradle_path
                    ));
                    break;
                }
            }
        }
        let result = match failure {
            Some(reason) => Err(reason),
            None => Ok(GradleBuild::new(key, projects, scripts)),
        };
        let outcome = result.as_ref().map(|_| ()).map_err(|e| e.clone());
        self.gradle_reads
            .builds
            .insert(settings_dir.to_string(), result);
        outcome
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

    /// Record the manifest at `dir/file_name` as FAILED because Gradle project/declaration
    /// ATTRIBUTION could not be established (D-DGC-BOUNDARY-1 (9)) — a settings file or project
    /// script of the build is unreadable, or a `projectDir` form is unhandled. The record carries
    /// `error_kind: attribution`, so query time words it as an attribution failure, never "not
    /// parsed"; the kind makes no claim about whether the recorded file was read or where the failed
    /// input resides. Same `dir/file_name` path derivation as [`Self::record_failed_manifest`].
    pub(crate) fn record_attribution_failed_manifest(
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
        self.manifest_provenance.record_failed_attribution(
            path,
            dir.to_string(),
            ecosystem,
            reason,
        );
    }
}

// ── Gradle attribution machinery (DEPS-GRADLE-CATALOG-1A; RG-REQ-006-L13) ─────────────────────

/// The build script at one directory, as read for Gradle attribution.
#[derive(Debug, Clone)]
pub(crate) enum GradleScriptRead {
    /// Neither `build.gradle` nor `build.gradle.kts` exists there.
    Absent,
    /// Read and scanned (`scopes` is empty when the script declares and counts nothing).
    Read {
        path: String,
        file_name: String,
        scopes: GradleScriptScopes,
    },
    /// Present but unreadable (`reason` is `unreadable: <io error>`).
    Unreadable {
        path: String,
        file_name: String,
        reason: String,
    },
}

/// The settings file at one directory, as read for Gradle attribution.
#[derive(Debug, Clone)]
pub(crate) enum GradleSettingsRead {
    /// Neither `settings.gradle` nor `settings.gradle.kts` exists there.
    Absent,
    /// Parsed: `path` is the settings file; `projects` its projects, root first.
    Parsed {
        path: String,
        projects: Vec<GradleProject>,
    },
    /// Present but its project roots are unproven (unreadable, or an unhandled `projectDir`
    /// form); `reason` names the settings file and the cause; `file_name` is its basename.
    Failed { reason: String, file_name: String },
}

/// One project of a Gradle build: its normalized Gradle path (`:` for the root, `:a:b`) and its
/// repo-relative root directory (`""` = the repository root).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GradleProject {
    gradle_path: String,
    dir: String,
}

impl GradleProject {
    fn is_root(&self) -> bool {
        self.gradle_path == ":"
    }
}

/// Where the Java files of one directory belong.
enum GradleAttribution {
    /// Project `project` (an index into the settings file's projects) of the build defined by the
    /// settings file at `settings_dir`.
    Project {
        settings_dir: String,
        project: usize,
    },
    /// A single-project build rooted at the nearest script (no settings file above it, or a
    /// script in a directory no project of the build names). With no nearest script: no Gradle
    /// build at all.
    SingleScript,
    /// Unknown — `reason` names the settings file and the cause; `settings_dir`/`settings_file`
    /// locate that settings file (the record a file with no ancestor script fails on).
    Failed {
        reason: String,
        settings_dir: String,
        settings_file: String,
    },
}

/// One settings-defined Gradle build, read once: its projects (root first), each project's script
/// (parallel to `projects`), its marking, and the declared sets computed so far.
pub(crate) struct GradleBuild {
    /// The build's identity for the marking carrier: its settings file's repo-relative path.
    key: String,
    projects: Vec<GradleProject>,
    scripts: Vec<Option<(String, GradleScriptScopes)>>,
    marking: Option<UndeterminedBlocks>,
    declared: HashMap<usize, Vec<String>>,
}

impl GradleBuild {
    fn new(
        key: String,
        projects: Vec<GradleProject>,
        scripts: Vec<Option<(String, GradleScriptScopes)>>,
    ) -> Self {
        // Counted blocks of every project script of the build, each script once, the root
        // project's script first, then the other scripts by repo-relative path (then line).
        let mut ordered: Vec<(bool, &str, &Vec<u32>)> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for (p, script) in projects.iter().zip(&scripts) {
            if let Some((path, scopes)) = script {
                if seen.insert(path.as_str()) {
                    ordered.push((
                        !p.is_root(),
                        path.as_str(),
                        &scopes.undetermined_block_lines,
                    ));
                }
            }
        }
        ordered.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
        let pairs: Vec<(&str, &Vec<u32>)> = ordered.iter().map(|(_, p, l)| (*p, *l)).collect();
        let marking = marking_of(&pairs);
        Self {
            key,
            projects,
            scripts,
            marking,
            declared: HashMap::new(),
        }
    }

    /// The declared set of project `r`: its script's `own` ∪ the `allprojects` of `r` and of its
    /// Gradle-path ancestors ∪ the `subprojects` of its strict ancestors ∪ every project script's
    /// `project('<path>')` blocks whose path resolves to `r`'s. Cached per project.
    fn declared_for(&mut self, r: usize) -> Vec<String> {
        if let Some(d) = self.declared.get(&r) {
            return d.clone();
        }
        let target = self.projects[r].gradle_path.as_str();
        let mut names: BTreeSet<String> = BTreeSet::new();
        if let Some((_, own)) = &self.scripts[r] {
            names.extend(own.own.iter().cloned());
        }
        for (p, script) in self.projects.iter().zip(&self.scripts) {
            let Some((_, scopes)) = script else {
                continue;
            };
            if p.gradle_path == target {
                names.extend(scopes.allprojects.iter().cloned());
            } else if is_gradle_ancestor(&p.gradle_path, target) {
                names.extend(scopes.allprojects.iter().cloned());
                names.extend(scopes.subprojects.iter().cloned());
            }
            for (written, groups) in &scopes.projects {
                if resolve_gradle_path(written, &p.gradle_path) == target {
                    names.extend(groups.iter().cloned());
                }
            }
        }
        let names: Vec<String> = names.into_iter().collect();
        self.declared.insert(r, names.clone());
        names
    }
}

/// The Gradle resolver's read cache (one entry per path read; lives on `RepoConfigContext`).
#[derive(Default)]
pub(crate) struct GradleReadCache {
    /// Directory → its build script.
    scripts: HashMap<String, GradleScriptRead>,
    /// Directory → its settings file.
    settings: HashMap<String, GradleSettingsRead>,
    /// Settings directory → the build it defines (or why its attribution is unknown).
    builds: HashMap<String, Result<GradleBuild, String>>,
}

impl GradleReadCache {
    /// The cached build at `settings_dir`. Only called after `gradle_build` returned `Ok`.
    fn built_mut(&mut self, settings_dir: &str) -> &mut GradleBuild {
        match self.builds.get_mut(settings_dir) {
            Some(Ok(build)) => build,
            _ => unreachable!(
                "gradle_build returned Ok for {settings_dir:?}, so the build is cached"
            ),
        }
    }
}

/// `dir/file_name`, or `file_name` at the repository root.
fn join_rel(dir: &str, file_name: &str) -> String {
    if dir.is_empty() {
        file_name.to_string()
    } else {
        format!("{dir}/{file_name}")
    }
}

/// The projects of a parsed settings file at `settings_dir`, root first, with normalized Gradle
/// paths (the parser prefixes a `:` to an include written with one, so leading colons collapse to
/// one) and repo-relative roots (`project_root` is relative to the settings directory).
fn settings_projects(
    settings_dir: &str,
    parsed: &repo_graph_indexer::settings_gradle::SettingsGradleParseResult,
) -> Vec<GradleProject> {
    let root_of = |project_root: &str| -> String {
        let rel = project_root.trim_start_matches("./").trim_end_matches('/');
        if rel.is_empty() || rel == "." {
            settings_dir.to_string()
        } else {
            join_rel(settings_dir, rel)
        }
    };
    let mut projects = vec![GradleProject {
        gradle_path: ":".to_string(),
        dir: settings_dir.to_string(),
    }];
    for m in &parsed.subprojects {
        projects.push(GradleProject {
            gradle_path: format!(":{}", m.gradle_path.trim_start_matches(':')),
            dir: root_of(&m.project_root),
        });
    }
    projects
}

/// The ancestor directories of `dir` (itself included), outermost (the repository root) first.
fn ancestor_dirs_outermost_first(dir: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    if !dir.is_empty() {
        let mut acc = String::new();
        for seg in dir.split('/') {
            if !acc.is_empty() {
                acc.push('/');
            }
            acc.push_str(seg);
            out.push(acc.clone());
        }
    }
    out
}

/// True iff directory `outer` is `inner` or one of its ancestors (`""` contains everything).
fn dir_contains(outer: &str, inner: &str) -> bool {
    outer.is_empty()
        || inner == outer
        || (inner.len() > outer.len()
            && inner.starts_with(outer)
            && inner.as_bytes()[outer.len()] == b'/')
}

/// The index of the deepest project whose root contains `dir` (ties: the first listed, so the
/// root project wins a tie with a project relocated onto its own directory).
fn deepest_project_containing(projects: &[GradleProject], dir: &str) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (i, p) in projects.iter().enumerate() {
        if dir_contains(&p.dir, dir) && best.is_none_or(|b| p.dir.len() > projects[b].dir.len()) {
            best = Some(i);
        }
    }
    best
}

/// True iff Gradle path `a` is a strict ancestor of `b` (`:` of every other project; `:a` of `:a:b`).
fn is_gradle_ancestor(a: &str, b: &str) -> bool {
    if a == b {
        return false;
    }
    a == ":" || (b.len() > a.len() && b.starts_with(a) && b.as_bytes()[a.len()] == b':')
}

/// The absolute Gradle path a `project('<written>')` block in the script of project `base` names:
/// an absolute path as written, a relative one against `base`.
fn resolve_gradle_path(written: &str, base: &str) -> String {
    if written.starts_with(':') {
        format!(":{}", written.trim_start_matches(':'))
    } else if base == ":" {
        format!(":{written}")
    } else {
        format!("{base}:{written}")
    }
}

/// The marking of a build whose scripts are `scripts` (already in carrier order): the number of
/// counted blocks and the first as `<path>:<line>`; `None` when there are none (no key written).
fn marking_of(scripts: &[(&str, &Vec<u32>)]) -> Option<UndeterminedBlocks> {
    let count: usize = scripts.iter().map(|(_, lines)| lines.len()).sum();
    let (path, lines) = scripts.iter().find(|(_, lines)| !lines.is_empty())?;
    Some(UndeterminedBlocks {
        count: u32::try_from(count).unwrap_or(u32::MAX),
        first: format!("{path}:{}", lines[0]),
    })
}

/// One manifest-provenance record. Field names (`path`/`dir`/`ecosystem`/`error`) are the wire
/// contract read back by `repo_graph_module_queries::ManifestProvenance` at query time.
///
/// Renamed from `ParsedManifestRecord` (review-4 item 1): the collector holds BOTH successfully
/// parsed manifests (`error == None`) AND FAILED records (`error == Some(reason)`) — a manifest
/// whose own read or parse failed, or (D-DGC-BOUNDARY-1 (9), `error_kind`) a record whose Gradle
/// declaration attribution could not be established — so the old "Parsed" name lied about the
/// failed entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManifestRecord {
    /// Repo-relative path of the manifest file the resolver encountered.
    pub path: String,
    /// Repo-relative directory the manifest governs (module attribution key at query time).
    pub dir: String,
    /// Ecosystem the reader belongs to (`npm`/`cargo`/`python`/`java`).
    pub ecosystem: String,
    /// `None` = read AND parsed (declared deps possibly empty — a legitimate measured-empty).
    /// `Some(reason)` = FAILED: the declared set this record stands for is unknown. [`Self::error_kind`]
    /// says which failure — the manifest's own read or parse (an io read error, or malformed content
    /// the reader could detect), or Gradle declaration attribution for this record's files. Query
    /// time renders the `Some` case as unknown-with-reason, never a `Parsed` zero-dep (review-4 item
    /// 1). Omitted from the wire when `None` (backward-compatible).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// D-DGC-BOUNDARY-1 (9): which failure `error` states. `Some(Attribution)` (`"attribution"` on
    /// the wire) = Gradle project/declaration attribution could not be established (an unreadable
    /// settings file or project script of the build, an unhandled `projectDir` form), so the
    /// declared set is unknown. It states the outcome only — not whether the recorded file was read,
    /// nor where the failed input resides: the record may be the build's `settings.gradle` itself,
    /// unreadable.
    /// `None` = a parsed record, or a failure of the manifest's OWN read or parse — the `parse` kind,
    /// which writes NO key so its record keeps today's bytes (query time reads an absent key on a
    /// FAILED record as `parse`). Only ever `Some` beside `error: Some(_)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<ManifestErrorKind>,
    /// DEPS-GRADLE-CATALOG-1A (D-DGC-CONDITIONAL-1; RG-REQ-002-L11): the Gradle `dependencies`
    /// blocks the reader skipped in this record's BUILD because it cannot attribute them statically.
    /// Exactly one record per build carries its build's marking (the build's recorded, non-FAILED
    /// manifest with the smallest path), so a sum over records counts distinct blocks. `None` (no
    /// key on the wire — today's bytes) for every other record, a build without such a block, and a
    /// build whose attribution is FAILED. Direct declarations stay known: a marking never makes a
    /// record FAILED.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undetermined_blocks: Option<UndeterminedBlocks>,
}

/// Which failure a FAILED manifest record states (D-DGC-BOUNDARY-1 (9)). The wire values are
/// `"parse"` and `"attribution"`; the writer never writes `"parse"` (an absent key reads as parse,
/// so every record a store holds from before this field keeps its meaning).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ManifestErrorKind {
    /// The manifest's own read or parse failed (an unreadable build script, malformed JSON, …).
    Parse,
    /// Gradle project/declaration attribution could not be established (a settings-file failure
    /// included). No claim about whether the recorded file was read or where the failed input
    /// resides.
    Attribution,
}

/// The count and first location of the `dependencies` blocks one Gradle build's reader skipped
/// (DEPS-GRADLE-CATALOG-1A). `count` counts BLOCKS, not declarations; `first` is
/// `<repo-relative script path>:<line>` of the first one — the root project's script first, then
/// the other project scripts by path, then by line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UndeterminedBlocks {
    /// Number of skipped `dependencies` blocks in the build.
    pub count: u32,
    /// `<repo-relative path>:<line>` of the first skipped block.
    pub first: String,
}

/// Accumulates the manifests a repo's deps resolvers encountered: one record per path, a FAILED
/// outcome winning over a PARSED one (see [`Self::record_failed`]).
#[derive(Debug, Clone, Default)]
pub struct ManifestProvenanceCollector {
    seen: HashSet<String>,
    records: Vec<ManifestRecord>,
    /// DEPS-GRADLE-CATALOG-1A: per Gradle build (its settings file, or its one script for a
    /// single-project build) the build's marking and the record path currently carrying it.
    build_markings: BTreeMap<String, (UndeterminedBlocks, String)>,
    /// D-DGC-ATTRIBUTION-1: per Gradle build, every record path offered its marking — the
    /// candidates the marking moves to when its carrier is replaced by a failure.
    build_members: BTreeMap<String, BTreeSet<String>>,
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
                error_kind: None,
                undetermined_blocks: None,
            });
        }
    }

    /// Record one manifest whose OWN read or parse failed (review-4 item 1). The `reason`
    /// (io error / malformed) rides the same `deps_manifests` wire record so query time can render
    /// it as unknown-with-reason instead of a fabricated parsed zero-dep. One record per `path`.
    ///
    /// FAILURE WINS (D-DGC-ATTRIBUTION-1 rule (b)): a failure replaces an earlier PARSED record of
    /// the same path in place (its position kept, the FAILED reason, no marking) — a certainty is
    /// never kept where a failure was found; a failure after a failure keeps the first reason. A
    /// replaced record's build marking moves to the build's smallest-path recorded non-FAILED
    /// record, if one exists. The cost is over-marking, never a claimed certainty.
    ///
    /// This is a failure of the manifest's OWN read or parse (kind `parse`, no `error_kind` key);
    /// see [`Self::record_failed_attribution`] for an attribution failure.
    pub fn record_failed(&mut self, path: String, dir: String, ecosystem: &str, reason: String) {
        self.record_failure(path, dir, ecosystem, reason, ManifestErrorKind::Parse);
    }

    /// Record one manifest path for which Gradle project/declaration ATTRIBUTION could not be
    /// established (D-DGC-BOUNDARY-1 (9)) — no claim about whether the recorded file was read or
    /// where the failed input resides (it may be this very settings file). Same
    /// one-record-per-path and failure-wins rules as [`Self::record_failed`]; the record carries
    /// `error_kind: attribution`.
    pub fn record_failed_attribution(
        &mut self,
        path: String,
        dir: String,
        ecosystem: &str,
        reason: String,
    ) {
        self.record_failure(path, dir, ecosystem, reason, ManifestErrorKind::Attribution);
    }

    /// The shared FAILED-record rule: one record per path; a failure replaces a PARSED record of
    /// the path in place; the FIRST failure of a path keeps both its reason and its kind.
    fn record_failure(
        &mut self,
        path: String,
        dir: String,
        ecosystem: &str,
        reason: String,
        kind: ManifestErrorKind,
    ) {
        // The `parse` kind is written as no key (today's bytes); only `attribution` is persisted.
        let error_kind = (kind == ManifestErrorKind::Attribution).then_some(kind);
        if self.seen.insert(path.clone()) {
            self.records.push(ManifestRecord {
                path,
                dir,
                ecosystem: ecosystem.to_string(),
                error: Some(reason),
                error_kind,
                undetermined_blocks: None,
            });
            return;
        }
        let Some(existing) = self.records.iter_mut().find(|r| r.path == path) else {
            return;
        };
        if existing.error.is_some() {
            return;
        }
        existing.error = Some(reason);
        existing.error_kind = error_kind;
        existing.undetermined_blocks = None;
        let carried: Vec<String> = self
            .build_markings
            .iter()
            .filter(|(_, (_, carrier))| *carrier == path)
            .map(|(key, _)| key.clone())
            .collect();
        for key in carried {
            self.rehome_build_marking(&key);
        }
    }

    /// Move build `build_key`'s marking to its smallest-path recorded non-FAILED member record, or
    /// drop it when none is left (every record of the build FAILED — its answer is already
    /// unknown-with-reason).
    fn rehome_build_marking(&mut self, build_key: &str) {
        let Some((marking, old)) = self.build_markings.remove(build_key) else {
            return;
        };
        let next = self.build_members.get(build_key).and_then(|members| {
            members
                .iter()
                .find(|m| {
                    self.records
                        .iter()
                        .any(|r| &r.path == *m && r.error.is_none())
                })
                .cloned()
        });
        if let Some(next) = &next {
            self.build_markings
                .insert(build_key.to_string(), (marking, next.clone()));
        }
        self.refresh_record_marking(&old);
        if let Some(next) = next {
            self.refresh_record_marking(&next);
        }
    }

    /// Borrow the accumulated records in insertion order.
    pub fn records(&self) -> &[ManifestRecord] {
        &self.records
    }

    /// DEPS-GRADLE-CATALOG-1A (D-DGC-CONDITIONAL-1): offer one Gradle build's `marking` to the
    /// recorded manifest at `path`, a record of that build. The marking rides EXACTLY ONE record per
    /// build — the build's recorded, non-FAILED manifest with the smallest repo-relative path —
    /// and moves here from a larger-path carrier recorded earlier, so the result does not depend on
    /// the order files are resolved in and a sum over records counts each block once. A FAILED or
    /// unrecorded `path` is never a carrier.
    pub(crate) fn offer_build_marking(
        &mut self,
        build_key: &str,
        marking: &UndeterminedBlocks,
        path: &str,
    ) {
        self.build_members
            .entry(build_key.to_string())
            .or_default()
            .insert(path.to_string());
        if !self
            .records
            .iter()
            .any(|r| r.path == path && r.error.is_none())
        {
            return;
        }
        let previous = match self.build_markings.get(build_key) {
            Some((_, carrier)) if carrier.as_str() <= path => return,
            Some((_, carrier)) => Some(carrier.clone()),
            None => None,
        };
        self.build_markings
            .insert(build_key.to_string(), (marking.clone(), path.to_string()));
        if let Some(old) = previous {
            self.refresh_record_marking(&old);
        }
        self.refresh_record_marking(path);
    }

    /// Recompute the marking of the record at `path` from the builds it carries (normally one;
    /// several builds sharing one nearest script sum their counts, the first build's location
    /// kept).
    fn refresh_record_marking(&mut self, path: &str) {
        let mut carried = self
            .build_markings
            .values()
            .filter(|(_, carrier)| carrier == path)
            .map(|(m, _)| m);
        let combined = carried.next().map(|first| {
            let mut m = first.clone();
            for more in carried {
                m.count = m.count.saturating_add(more.count);
            }
            m
        });
        if let Some(r) = self.records.iter_mut().find(|r| r.path == path) {
            r.undetermined_blocks = combined;
        }
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

    // ── DEPS-GRADLE-CATALOG-1A: per-project attribution within a Gradle build (RG-REQ-006-L13) ──

    /// Write `content` at repo-relative `rel` under `root`, creating parent directories.
    fn put(root: &Path, rel: &str, content: &str) {
        let p = root.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, content).unwrap();
    }

    fn java(ctx: &mut RepoConfigContext, root: &Path, file: &str) -> Vec<String> {
        ctx.resolve_gradle_deps(file, root).names
    }

    fn record<'a>(ctx: &'a RepoConfigContext, path: &str) -> &'a ManifestRecord {
        ctx.manifest_records()
            .iter()
            .find(|r| r.path == path)
            .unwrap_or_else(|| panic!("no record {path}: {:?}", ctx.manifest_records()))
    }

    /// The kafka shape: one root script, no module scripts; the root's `project(':a')` block
    /// declares for `a` only, `project(':b')` for `b` only, and the buildscript classpath for no one.
    const KAFKA_SHAPE_SETTINGS: &str = "include 'a', 'b'\n";
    const KAFKA_SHAPE_ROOT: &str = r#"
buildscript {
  dependencies {
    classpath "org.tool:plugin:1.0"
  }
}
dependencies {
  implementation 'org.rootown:x:1'
}
project(':a') {
  dependencies {
    implementation 'org.a:x:1'
  }
}
project(':b') {
  dependencies {
    implementation 'org.b:x:1'
  }
}
"#;

    #[test]
    fn gradle_project_without_its_own_script_declares_only_its_root_project_block() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", KAFKA_SHAPE_SETTINGS);
        put(root, "build.gradle", KAFKA_SHAPE_ROOT);
        let mut ctx = RepoConfigContext::new();
        assert_eq!(
            java(&mut ctx, root, "a/src/main/java/A.java"),
            vec!["org.a"]
        );
        assert_eq!(
            java(&mut ctx, root, "b/src/main/java/B.java"),
            vec!["org.b"]
        );
        // Provenance keeps recording the nearest ancestor script exactly as today.
        let rec = record(&ctx, "build.gradle");
        assert!(rec.error.is_none(), "{rec:?}");
    }

    /// Attribution is per project, never per shared script: resolving the two modules in either
    /// order yields the same sets (the walked-up probe directory is never handed another file's
    /// result), and a file in the root project's own directory reads only the root's `own`.
    #[test]
    fn gradle_projects_sharing_one_script_resolve_independently_of_order() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", KAFKA_SHAPE_SETTINGS);
        put(root, "build.gradle", KAFKA_SHAPE_ROOT);

        let mut ab = RepoConfigContext::new();
        let a1 = java(&mut ab, root, "a/src/main/java/A.java");
        let b1 = java(&mut ab, root, "b/src/main/java/B.java");
        let mut ba = RepoConfigContext::new();
        let b2 = java(&mut ba, root, "b/src/main/java/B.java");
        let a2 = java(&mut ba, root, "a/src/main/java/A.java");
        assert_eq!(a1, a2);
        assert_eq!(b1, b2);
        assert_eq!(a1, vec!["org.a"]);
        assert_eq!(b1, vec!["org.b"]);

        let mut r = RepoConfigContext::new();
        let _ = java(&mut r, root, "a/src/main/java/A.java");
        assert_eq!(
            java(&mut r, root, "src/main/java/Root.java"),
            vec!["org.rootown"]
        );
    }

    /// A root `allprojects` block reaches the root and every project; a root `subprojects` block
    /// every project but the root.
    #[test]
    fn gradle_allprojects_reaches_every_project_and_subprojects_every_non_root_project() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", "include 'a', 'b'\n");
        put(
            root,
            "build.gradle",
            "allprojects {\n  dependencies {\n    implementation 'org.all:x:1'\n  }\n}\n\
             subprojects {\n  dependencies {\n    implementation 'org.sub:x:1'\n  }\n}\n",
        );
        put(
            root,
            "b/build.gradle",
            "dependencies {\n  implementation 'org.bown:x:1'\n}\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert_eq!(java(&mut ctx, root, "src/Root.java"), vec!["org.all"]);
        assert_eq!(
            java(&mut ctx, root, "a/src/A.java"),
            vec!["org.all", "org.sub"]
        );
        assert_eq!(
            java(&mut ctx, root, "b/src/B.java"),
            vec!["org.all", "org.bown", "org.sub"]
        );
    }

    /// A root script's top-level block applies to the root project only — never to a subproject,
    /// whether or not the subproject has a script of its own.
    #[test]
    fn gradle_root_top_level_block_never_reaches_a_subproject() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(
            root,
            "settings.gradle",
            "include 'withscript', 'noscript'\n",
        );
        put(
            root,
            "build.gradle",
            "dependencies {\n  implementation 'org.root:x:1'\n}\n",
        );
        put(
            root,
            "withscript/build.gradle",
            "dependencies {\n  implementation 'org.ws:x:1'\n}\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert_eq!(
            java(&mut ctx, root, "withscript/src/W.java"),
            vec!["org.ws"]
        );
        assert!(
            java(&mut ctx, root, "noscript/src/N.java").is_empty(),
            "the root's own block never reaches a subproject without a script"
        );
        assert_eq!(java(&mut ctx, root, "src/R.java"), vec!["org.root"]);
    }

    /// The Gradle path is mapped through the settings file WITH its `projectDir` relocations: the
    /// root's `project(':grpc-core')` block reaches files under `core/` (the grpc-java form).
    #[test]
    fn gradle_project_path_follows_the_settings_projectdir_relocation() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(
            root,
            "settings.gradle",
            "include ':grpc-core'\nproject(':grpc-core').projectDir = \"$rootDir/core\" as File\n",
        );
        put(
            root,
            "build.gradle",
            "project(':grpc-core') {\n  dependencies {\n    implementation 'org.core:x:1'\n  }\n}\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert_eq!(
            java(&mut ctx, root, "core/src/main/java/io/grpc/Core.java"),
            vec!["org.core"]
        );
        assert!(java(&mut ctx, root, "grpc-core/src/X.java").is_empty());
    }

    /// Scope blocks of an INTERMEDIATE project's script reach its descendants: `:a`'s allprojects
    /// reaches `:a` and `:a:b`, its subprojects `:a:b` only, its relative `project('b')` resolves
    /// against `:a` and its absolute `project(':c')` names `:c`; none of it reaches the root or `:d`.
    #[test]
    fn gradle_intermediate_project_scope_blocks_reach_their_descendants() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", "include 'a', 'a:b', 'c', 'd'\n");
        put(
            root,
            "build.gradle",
            "dependencies {\n  implementation 'org.root:x:1'\n}\n",
        );
        put(
            root,
            "a/build.gradle",
            r#"
allprojects {
  dependencies {
    implementation 'org.a.all:x:1'
  }
}
subprojects {
  dependencies {
    implementation 'org.a.sub:x:1'
  }
}
project('b') {
  dependencies {
    implementation 'org.a.relb:x:1'
  }
}
project(':c') {
  dependencies {
    implementation 'org.a.absc:x:1'
  }
}
"#,
        );
        let mut ctx = RepoConfigContext::new();
        assert_eq!(java(&mut ctx, root, "a/src/A.java"), vec!["org.a.all"]);
        assert_eq!(
            java(&mut ctx, root, "a/b/src/B.java"),
            vec!["org.a.all", "org.a.relb", "org.a.sub"]
        );
        assert_eq!(java(&mut ctx, root, "c/src/C.java"), vec!["org.a.absc"]);
        assert_eq!(java(&mut ctx, root, "src/R.java"), vec!["org.root"]);
        assert!(java(&mut ctx, root, "d/src/D.java").is_empty());
    }

    /// A nested settings file defines its own build: `ex/app` reads its own block and `ex`'s
    /// allprojects — never the outer root's blocks and never `ex`'s buildscript classpath.
    #[test]
    fn gradle_nested_settings_file_defines_its_own_build() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", "include 'lib'\n");
        put(
            root,
            "build.gradle",
            "allprojects {\n  dependencies {\n    implementation 'org.outer.all:x:1'\n  }\n}\n\
             dependencies {\n  implementation 'org.outer:x:1'\n}\n",
        );
        put(root, "ex/settings.gradle", "include ':app'\n");
        put(
            root,
            "ex/build.gradle",
            "buildscript {\n  dependencies {\n    classpath 'org.ex.tool:x:1'\n  }\n}\n\
             allprojects {\n  dependencies {\n    implementation 'org.ex.all:x:1'\n  }\n}\n",
        );
        put(
            root,
            "ex/app/build.gradle",
            "dependencies {\n  implementation 'org.ex.app:x:1'\n}\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert_eq!(
            java(&mut ctx, root, "ex/app/src/main/java/App.java"),
            vec!["org.ex.all", "org.ex.app"]
        );
    }

    /// A script in a directory no settings project names (grpc-java's `buildSrc/`) is its own
    /// single-project build: its `own` ∪ its `allprojects`, nothing from the enclosing build.
    #[test]
    fn gradle_script_outside_every_settings_project_is_its_own_build() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", "include 'a'\n");
        put(
            root,
            "build.gradle",
            "allprojects {\n  dependencies {\n    implementation 'org.root.all:x:1'\n  }\n}\n",
        );
        put(
            root,
            "tool/build.gradle",
            "allprojects {\n  dependencies {\n    implementation 'org.tool.all:x:1'\n  }\n}\n\
             subprojects {\n  dependencies {\n    implementation 'org.tool.sub:x:1'\n  }\n}\n\
             dependencies {\n  implementation 'org.tool:x:1'\n}\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert_eq!(
            java(&mut ctx, root, "tool/src/T.java"),
            vec!["org.tool", "org.tool.all"]
        );
    }

    /// A present-but-unreadable settings file (here a DIRECTORY) leaves project attribution
    /// unknown: the owning script is a FAILED manifest whose reason names `settings.gradle`, and
    /// the declared set is empty — never a guessed project.
    #[test]
    fn gradle_unreadable_settings_is_a_failed_manifest_never_a_guessed_project() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join("settings.gradle")).unwrap();
        put(
            root,
            "build.gradle",
            "dependencies {\n  implementation 'org.root:x:1'\n}\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert!(java(&mut ctx, root, "src/R.java").is_empty());
        let rec = record(&ctx, "build.gradle");
        let reason = rec.error.as_deref().expect("FAILED record");
        assert!(reason.contains("settings.gradle"), "{reason}");
        assert!(reason.contains("unreadable"), "{reason}");
    }

    /// A settings file with a `projectDir` assignment in a form the parser does not read leaves
    /// every project root of that build unproven: FAILED record naming the form, empty set.
    #[test]
    fn gradle_unhandled_projectdir_form_is_a_failed_manifest_never_a_guessed_owner() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(
            root,
            "settings.gradle",
            "include ':a'\nproject(':a').projectDir = file('x')\n",
        );
        put(
            root,
            "build.gradle",
            "project(':a') {\n  dependencies {\n    implementation 'org.a:x:1'\n  }\n}\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert!(java(&mut ctx, root, "a/src/A.java").is_empty());
        assert!(java(&mut ctx, root, "x/src/X.java").is_empty());
        let rec = record(&ctx, "build.gradle");
        let reason = rec.error.as_deref().expect("FAILED record");
        assert!(reason.contains("settings.gradle"), "{reason}");
        assert!(reason.contains("unhandled `projectDir` form"), "{reason}");
    }

    // ── D-DGC-CONDITIONAL-1: the build's marking rides exactly one provenance record ──

    #[test]
    fn gradle_build_marking_rides_one_record_per_build_and_keeps_direct_declarations() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", "include ':a', ':b'\n");
        put(
            root,
            "build.gradle",
            "subprojects {\n  if (x) {\n    dependencies {\n      implementation 'org.cond:x:1'\n    }\n  }\n}\n",
        );
        put(
            root,
            "a/build.gradle",
            "dependencies {\n  implementation 'org.a:x:1'\n}\n",
        );
        put(
            root,
            "b/build.gradle",
            "dependencies {\n  implementation 'org.b:x:1'\n}\n",
        );
        for order in [
            ["a/src/A.java", "b/src/B.java"],
            ["b/src/B.java", "a/src/A.java"],
        ] {
            let mut ctx = RepoConfigContext::new();
            for f in order {
                let _ = java(&mut ctx, root, f);
            }
            assert_eq!(java(&mut ctx, root, "a/src/A.java"), vec!["org.a"]);
            assert_eq!(java(&mut ctx, root, "b/src/B.java"), vec!["org.b"]);
            let a = record(&ctx, "a/build.gradle");
            let b = record(&ctx, "b/build.gradle");
            assert!(a.error.is_none() && b.error.is_none(), "{a:?} {b:?}");
            assert_eq!(
                a.undetermined_blocks,
                Some(UndeterminedBlocks {
                    count: 1,
                    first: "build.gradle:3".to_string()
                }),
                "order {order:?}"
            );
            assert_eq!(b.undetermined_blocks, None, "order {order:?}");
        }
    }

    #[test]
    fn gradle_build_marking_counts_every_project_script_root_first() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(
            root,
            "settings.gradle",
            "include ':p'\nproject(':p').projectDir = \"$rootDir/a0\" as File\n",
        );
        put(
            root,
            "build.gradle",
            "\n\nafterEvaluate {\n  dependencies {\n    implementation libs.x\n  }\n}\n",
        );
        put(
            root,
            "a0/build.gradle",
            "gradle.projectsEvaluated {\n  dependencies {\n    implementation libraries.y\n  }\n}\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert!(java(&mut ctx, root, "src/Main.java").is_empty());
        assert_eq!(
            record(&ctx, "build.gradle").undetermined_blocks,
            Some(UndeterminedBlocks {
                count: 2,
                first: "build.gradle:4".to_string()
            })
        );
    }

    #[test]
    fn gradle_separate_builds_carry_separate_markings() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", "include 'lib'\n");
        put(
            root,
            "build.gradle",
            "if (x) {\n  dependencies {\n  }\n}\nafterEvaluate {\n  dependencies {\n  }\n}\n",
        );
        put(root, "ex/settings.gradle", "rootProject.name = 'ex'\n");
        put(
            root,
            "ex/build.gradle",
            "plugins.withId('java') {\n  dependencies {\n  }\n}\n",
        );
        let mut ctx = RepoConfigContext::new();
        let _ = java(&mut ctx, root, "lib/src/L.java");
        let _ = java(&mut ctx, root, "ex/src/E.java");
        assert_eq!(
            record(&ctx, "build.gradle").undetermined_blocks,
            Some(UndeterminedBlocks {
                count: 2,
                first: "build.gradle:2".to_string()
            })
        );
        assert_eq!(
            record(&ctx, "ex/build.gradle").undetermined_blocks,
            Some(UndeterminedBlocks {
                count: 1,
                first: "ex/build.gradle:2".to_string()
            })
        );
    }

    #[test]
    fn gradle_failed_attribution_carries_no_marking() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(
            root,
            "settings.gradle",
            "include ':a'\nproject(':a').projectDir = file('x')\n",
        );
        put(
            root,
            "build.gradle",
            "if (x) {\n  dependencies {\n    implementation 'org.c:x:1'\n  }\n}\n",
        );
        let mut ctx = RepoConfigContext::new();
        let _ = java(&mut ctx, root, "src/R.java");
        let rec = record(&ctx, "build.gradle");
        assert!(rec.error.is_some(), "{rec:?}");
        assert_eq!(rec.undetermined_blocks, None, "{rec:?}");
        assert!(ctx
            .manifest_records()
            .iter()
            .all(|r| r.undetermined_blocks.is_none()));
    }

    #[test]
    fn manifest_record_serializes_the_marking_only_when_present() {
        let none = ManifestRecord {
            path: "build.gradle".into(),
            dir: String::new(),
            ecosystem: "java".into(),
            error: None,
            error_kind: None,
            undetermined_blocks: None,
        };
        assert_eq!(
            serde_json::to_string(&none).unwrap(),
            r#"{"path":"build.gradle","dir":"","ecosystem":"java"}"#,
            "no marking → today's bytes, no key"
        );
        let some = ManifestRecord {
            undetermined_blocks: Some(UndeterminedBlocks {
                count: 1,
                first: "build.gradle:3".into(),
            }),
            ..none
        };
        let json = serde_json::to_string(&some).unwrap();
        assert!(
            json.contains(r#""undetermined_blocks":{"count":1,"first":"build.gradle:3"}"#),
            "{json}"
        );
    }

    // ── D-DGC-ATTRIBUTION-1: an attribution the nearest script cannot decide is never certain ──

    /// Rule (b): a FAILED record wins over a PARSED record of the same path, in either order; a
    /// second failure keeps the first reason; one record per path, its position kept.
    #[test]
    fn collector_failure_wins_over_parsed_for_the_same_path_in_either_order() {
        // parsed, then failed → one record, FAILED, with the reason, at its original position.
        let mut c = ManifestProvenanceCollector::default();
        c.record("a/build.gradle".into(), "a".into(), "java");
        c.record("build.gradle".into(), String::new(), "java");
        c.record_failed(
            "build.gradle".into(),
            String::new(),
            "java",
            "attribution unknown".into(),
        );
        let paths: Vec<&str> = c.records().iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, vec!["a/build.gradle", "build.gradle"]);
        assert_eq!(c.records()[1].error.as_deref(), Some("attribution unknown"));
        assert!(c.records()[0].error.is_none(), "another path is untouched");

        // failed, then parsed → one record, still FAILED.
        let mut c = ManifestProvenanceCollector::default();
        c.record_failed(
            "build.gradle".into(),
            String::new(),
            "java",
            "attribution unknown".into(),
        );
        c.record("build.gradle".into(), String::new(), "java");
        assert_eq!(c.records().len(), 1);
        assert_eq!(c.records()[0].error.as_deref(), Some("attribution unknown"));

        // two failures → the first reason.
        let mut c = ManifestProvenanceCollector::default();
        c.record_failed("build.gradle".into(), String::new(), "java", "first".into());
        c.record_failed(
            "build.gradle".into(),
            String::new(),
            "java",
            "second".into(),
        );
        assert_eq!(c.records().len(), 1);
        assert_eq!(c.records()[0].error.as_deref(), Some("first"));
    }

    /// Which admissible answer project `a`'s file got in the no-ancestor-script counterexample.
    #[derive(Debug, PartialEq, Eq)]
    enum NoScriptAnswer {
        /// `A.java` reads exactly `["org.acme"]`.
        Declared,
        /// `A.java` reads `[]`, marked at `b/build.gradle:2` on a record that covers `a`'s files.
        Undetermined,
        /// Anything else (a certain empty set, a mislocated or unreachable marking, …).
        Neither(String),
    }

    fn no_script_answer(ctx: &RepoConfigContext, a_set: &[String]) -> NoScriptAnswer {
        if a_set == ["org.acme".to_string()] {
            return NoScriptAnswer::Declared;
        }
        let java_ok: Vec<&ManifestRecord> = ctx
            .manifest_records()
            .iter()
            .filter(|r| r.ecosystem == "java" && r.error.is_none())
            .collect();
        let marked: Vec<&&ManifestRecord> = java_ok
            .iter()
            .filter(|r| r.undetermined_blocks.is_some())
            .collect();
        let covers_a = java_ok
            .iter()
            .any(|r| dir_contains(&r.dir, "a/src/main/java/app"));
        match marked.as_slice() {
            [only] if a_set.is_empty() && covers_a => {
                let m = only.undetermined_blocks.as_ref().unwrap();
                if m.count >= 1 && m.first == "b/build.gradle:2" {
                    NoScriptAnswer::Undetermined
                } else {
                    NoScriptAnswer::Neither(format!("marking mislocated: {m:?}"))
                }
            }
            _ => NoScriptAnswer::Neither(format!(
                "A.java → {a_set:?}; records {:?}",
                ctx.manifest_records()
            )),
        }
    }

    /// Rule (a), the reviewer's first counterexample: `settings.gradle` includes `a` and `b`; no
    /// build script at the root or in `a/`; `b/build.gradle` declares for `:a`. `A.java` is never
    /// a certain empty set — DECLARED (`["org.acme"]`) or UNDETERMINED (marked at the block's head,
    /// on a record that reaches `a`'s row) — the same in both resolution orders; `B.java` reads `[]`.
    #[test]
    fn gradle_project_reached_by_no_ancestor_script_is_declared_or_marked_never_a_certain_empty_set(
    ) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", "include 'a', 'b'\n");
        put(
            root,
            "b/build.gradle",
            "project(':a') {\n  dependencies {\n    implementation 'org.acme:lib:1.0'\n  }\n}\n",
        );
        const A: &str = "a/src/main/java/app/A.java";
        const B: &str = "b/src/main/java/b/B.java";

        // Context 1: A.java in a fresh context.
        let mut ctx1 = RepoConfigContext::new();
        let a1 = java(&mut ctx1, root, A);
        let answer1 = no_script_answer(&ctx1, &a1);
        let b1 = java(&mut ctx1, root, B);
        assert!(b1.is_empty(), "project(':a') never reaches :b — {b1:?}");

        // Context 2: B.java first, then A.java.
        let mut ctx2 = RepoConfigContext::new();
        let b2 = java(&mut ctx2, root, B);
        assert!(b2.is_empty(), "project(':a') never reaches :b — {b2:?}");
        let a2 = java(&mut ctx2, root, A);
        let answer2 = no_script_answer(&ctx2, &a2);

        assert!(
            !matches!(answer1, NoScriptAnswer::Neither(_)),
            "context 1: {answer1:?}"
        );
        assert_eq!(
            answer1, answer2,
            "both resolution orders give the same answer"
        );
    }

    /// Rule (a) under an unknown build: an unhandled `projectDir` form and no build script
    /// anywhere → `A.java` reads `[]` beside a FAILED `java` record naming the cause, whose `dir`
    /// covers the file — unknown-with-reason, never a certain empty set.
    #[test]
    fn gradle_unknown_build_reached_by_no_ancestor_script_is_a_failed_record_never_a_certain_empty_set(
    ) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(
            root,
            "settings.gradle",
            "include ':a'\nproject(':a').projectDir = file('x')\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert!(java(&mut ctx, root, "a/src/A.java").is_empty());
        let failed: Vec<&ManifestRecord> = ctx
            .manifest_records()
            .iter()
            .filter(|r| {
                r.ecosystem == "java"
                    && dir_contains(&r.dir, "a/src")
                    && r.error.as_deref().is_some_and(|e| {
                        e.contains("settings.gradle") && e.contains("unhandled `projectDir` form")
                    })
            })
            .collect();
        assert_eq!(failed.len(), 1, "records: {:?}", ctx.manifest_records());
        assert!(failed[0].undetermined_blocks.is_none());
    }

    /// Rule (b), the reviewer's second counterexample: root `build.gradle` with a direct block, no
    /// root settings file, `zz/settings.gradle` present but unreadable. Whichever file resolves
    /// first, the only `build.gradle` record is FAILED naming `zz/settings.gradle`, unmarked.
    #[test]
    fn gradle_attribution_failure_wins_over_an_earlier_parsed_record_of_the_same_script() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(
            root,
            "build.gradle",
            "dependencies {\n  implementation 'org.root:core:1.0'\n}\n",
        );
        std::fs::create_dir_all(root.join("zz/settings.gradle")).unwrap();
        const R: &str = "src/main/java/r/R.java";
        const Z: &str = "zz/src/main/java/z/Z.java";

        let check = |ctx: &RepoConfigContext, label: &str| {
            let recs: Vec<&ManifestRecord> = ctx
                .manifest_records()
                .iter()
                .filter(|r| r.path == "build.gradle")
                .collect();
            assert_eq!(recs.len(), 1, "{label}: {:?}", ctx.manifest_records());
            let reason = recs[0]
                .error
                .as_deref()
                .unwrap_or_else(|| panic!("{label}: build.gradle must be FAILED: {recs:?}"));
            assert!(reason.contains("zz/settings.gradle"), "{label}: {reason}");
            assert!(reason.contains("unreadable"), "{label}: {reason}");
            assert!(recs[0].undetermined_blocks.is_none(), "{label}");
        };

        // Context 1: R.java (records build.gradle parsed), then Z.java.
        let mut ctx1 = RepoConfigContext::new();
        assert_eq!(java(&mut ctx1, root, R), vec!["org.root".to_string()]);
        assert!(java(&mut ctx1, root, Z).is_empty());
        check(&ctx1, "R then Z");

        // Context 2: the reverse order.
        let mut ctx2 = RepoConfigContext::new();
        assert!(java(&mut ctx2, root, Z).is_empty());
        assert_eq!(java(&mut ctx2, root, R), vec!["org.root".to_string()]);
        check(&ctx2, "Z then R");
    }

    /// Rule (c), the regression guard: no build script and no settings file anywhere → the empty
    /// set with no provenance record and no marking (not uncertainty — no Gradle build).
    #[test]
    fn java_file_in_no_gradle_build_keeps_the_empty_set_unmarked() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "src/main/java/p/P.java", "package p;\n");
        let mut ctx = RepoConfigContext::new();
        assert!(java(&mut ctx, root, "src/main/java/p/P.java").is_empty());
        assert!(
            ctx.manifest_records().is_empty(),
            "{:?}",
            ctx.manifest_records()
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

    // ── D-DGC-BOUNDARY-1 (9): a FAILED record states which failure it is ──────────────────────

    /// The writer: a parsed record and a parse failure keep today's bytes (no `error_kind` key);
    /// only an attribution failure writes `"error_kind":"attribution"`; the first failure of a
    /// path keeps its reason AND its kind, and an attribution failure replacing a parsed record
    /// carries its kind.
    #[test]
    fn manifest_record_serializes_error_kind_only_for_an_attribution_failure() {
        let parsed = ManifestRecord {
            path: "build.gradle".into(),
            dir: String::new(),
            ecosystem: "java".into(),
            error: None,
            error_kind: None,
            undetermined_blocks: None,
        };
        assert_eq!(
            serde_json::to_string(&parsed).unwrap(),
            r#"{"path":"build.gradle","dir":"","ecosystem":"java"}"#
        );
        let parse = ManifestRecord {
            error: Some("unreadable: x".into()),
            ..parsed.clone()
        };
        assert_eq!(
            serde_json::to_string(&parse).unwrap(),
            r#"{"path":"build.gradle","dir":"","ecosystem":"java","error":"unreadable: x"}"#
        );
        let attribution = ManifestRecord {
            error: Some("gradle project attribution unknown: y".into()),
            error_kind: Some(ManifestErrorKind::Attribution),
            ..parsed
        };
        assert_eq!(
            serde_json::to_string(&attribution).unwrap(),
            r#"{"path":"build.gradle","dir":"","ecosystem":"java","error":"gradle project attribution unknown: y","error_kind":"attribution"}"#
        );

        // Through the collector: an attribution failure replacing a parsed record carries its kind.
        let mut c = ManifestProvenanceCollector::default();
        c.record("build.gradle".into(), String::new(), "java");
        c.record_failed_attribution(
            "build.gradle".into(),
            String::new(),
            "java",
            "gradle project attribution unknown: y".into(),
        );
        assert_eq!(c.records().len(), 1);
        assert_eq!(
            c.records()[0].error_kind,
            Some(ManifestErrorKind::Attribution)
        );
        assert_eq!(
            c.records()[0].error.as_deref(),
            Some("gradle project attribution unknown: y")
        );

        // A parse failure followed by an attribution failure keeps the parse reason and no kind.
        let mut c = ManifestProvenanceCollector::default();
        c.record_failed(
            "build.gradle".into(),
            String::new(),
            "java",
            "unreadable: x".into(),
        );
        c.record_failed_attribution(
            "build.gradle".into(),
            String::new(),
            "java",
            "gradle project attribution unknown: y".into(),
        );
        assert_eq!(c.records().len(), 1);
        assert_eq!(c.records()[0].error.as_deref(), Some("unreadable: x"));
        assert_eq!(c.records()[0].error_kind, None);
        assert_eq!(
            serde_json::to_string(&c.records()[0]).unwrap(),
            r#"{"path":"build.gradle","dir":"","ecosystem":"java","error":"unreadable: x"}"#
        );
    }

    /// The reader: every FAILED record the Gradle attribution writes is kind `attribution`; a
    /// failure of the recorded script's own read is `parse` (no key), and an unreadable NEAREST
    /// script is never skipped for a farther readable one.
    #[test]
    fn gradle_failures_record_attribution_for_the_build_and_parse_for_the_script() {
        // (i) readable root script, no settings file, `sub/build.gradle` a DIRECTORY.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(
            root,
            "build.gradle",
            "dependencies {\n  implementation 'org.root:core:1.0'\n}\n",
        );
        std::fs::create_dir_all(root.join("sub/build.gradle")).unwrap();
        let mut ctx = RepoConfigContext::new();
        assert!(
            java(&mut ctx, root, "sub/src/main/java/p/P.java").is_empty(),
            "an unreadable nearest script is never skipped for the farther readable root script"
        );
        let r = record(&ctx, "sub/build.gradle");
        assert!(
            r.error
                .as_deref()
                .is_some_and(|e| e.starts_with("unreadable:")),
            "{r:?}"
        );
        assert_eq!(r.error_kind, None, "a script's own read failure is `parse`");

        // (ii) readable root script with a coordinate, `settings.gradle` a DIRECTORY.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(
            root,
            "build.gradle",
            "dependencies {\n  implementation 'org.root:core:1.0'\n}\n",
        );
        std::fs::create_dir_all(root.join("settings.gradle")).unwrap();
        let mut ctx = RepoConfigContext::new();
        assert!(java(&mut ctx, root, "src/main/java/r/R.java").is_empty());
        let r = record(&ctx, "build.gradle");
        assert!(
            r.error
                .as_deref()
                .is_some_and(|e| e.contains("settings.gradle")),
            "{r:?}"
        );
        assert_eq!(r.error_kind, Some(ManifestErrorKind::Attribution), "{r:?}");

        // (iii) an unhandled `projectDir` form, no build script: the settings record.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(
            root,
            "settings.gradle",
            "include ':a'\nproject(':a').projectDir = file('x')\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert!(java(&mut ctx, root, "a/src/A.java").is_empty());
        let r = record(&ctx, "settings.gradle");
        assert!(r.error.is_some(), "{r:?}");
        assert_eq!(r.error_kind, Some(ManifestErrorKind::Attribution), "{r:?}");
    }

    /// Document review-2 of PREP-7, F-2: in one settings build the root script's scope blocks can
    /// declare for `:svc`, so an unreadable root script makes `:svc`'s attribution unknown even
    /// though `svc/build.gradle` is readable — recorded on the child's own nearest script as an
    /// ATTRIBUTION failure that names the root script, never a certain set from the child alone.
    #[test]
    fn gradle_unreadable_root_script_makes_a_child_projects_attribution_a_failure() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", "include ':svc'\n");
        std::fs::create_dir_all(root.join("build.gradle")).unwrap();
        put(
            root,
            "svc/build.gradle",
            "dependencies {\n  implementation 'org.svc:lib:1.0'\n}\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert!(
            java(&mut ctx, root, "svc/src/main/java/s/S.java").is_empty(),
            "never [org.svc] from the readable child script alone"
        );
        let covering: Vec<&ManifestRecord> = ctx
            .manifest_records()
            .iter()
            .filter(|r| r.ecosystem == "java" && dir_contains(&r.dir, "svc/src/main/java/s"))
            .collect();
        assert_eq!(covering.len(), 1, "{:?}", ctx.manifest_records());
        let r = covering[0];
        assert_eq!(r.path, "svc/build.gradle");
        assert_eq!(r.error_kind, Some(ManifestErrorKind::Attribution), "{r:?}");
        assert!(r.undetermined_blocks.is_none(), "{r:?}");
        let reason = r.error.as_deref().expect("FAILED");
        assert!(reason.contains("unreadable"), "{reason}");
        assert!(
            reason
                .replace("svc/build.gradle", "")
                .contains("build.gradle"),
            "the reason names the root script, not only the child's: {reason}"
        );
    }
}
