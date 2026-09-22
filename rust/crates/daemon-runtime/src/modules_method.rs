//! MODULES-METHOD-1: compute the per-repo method description and orientation doc
//! recommendation from stored facts.
//!
//! Pure functions — no I/O, no storage access — consuming data the callers
//! already loaded (`module_candidates`/`module_candidate_evidence` and
//! `doc_inventory`): the method-family grouping, the §2.2 orientation-doc
//! selection, and their JSON shaping. Both callers are in this crate:
//! `handle_modules_list` and `orient_additive_fields` (which owns the storage
//! reads and calls these on the results).
//!
//! Abstraction record (crate-private module):
//!   - what: the method-line + orientation-doc computation for the modules surface.
//!   - concrete current users: `dispatch::handle_modules_list`, `orient_additive_fields::inject`.
//!   - axis of variation: which manifest families / doc kinds exist per repo.
//!   - rejected simpler: inlining in each caller (duplicates the family-grouping +
//!     doc-filtering logic across two handlers — earned by 2 concrete callers).

use std::collections::BTreeMap;

/// A module's identity facts relevant to the method computation — the subset of
/// `module_candidates` + `module_candidate_evidence` the method line needs. Raw DTO.
///
/// Uses `module_kind` (the stored Layer-0 fact from `module_candidates`: "declared",
/// "inferred", or legacy "directory") and `source_type` (the stored ecosystem marker
/// from `module_candidate_evidence`: "cargo_toml", "settings_gradle", "package_json",
/// "pnpm_workspace_yaml", "pyproject_toml", "directory_heuristic"). The spec §2.1
/// requires both: "`module_candidates.module_kind` / `module_candidate_evidence.source_type`".
///
/// `source_type` is `None` when no evidence row exists for the module (a legacy
/// MODULE-node fallback module has none). A `declared` module with a `None`
/// `source_type` cannot have its manifest named from stored facts — per spec §2.3
/// the WHOLE method then computes to [`MethodComputation::NotRecorded`] ("method not
/// recorded on this index"), NEVER a fabricated "unknown" family (review-3 #1). An
/// evidence READ FAILURE never reaches this DTO — the caller
/// (`build_modules_method_json`) degrades to `{unavailable}` at the storage boundary,
/// keeping IO failure (Unavailable) distinct from insufficient facts (NotRecorded).
pub(crate) struct ModuleMethodInput<'a> {
    pub module_kind: &'a str,
    pub source_type: Option<&'a str>,
}

/// Per-repo diagnostics that augment the method line (spec §2.1: "with the existing
/// diagnostics"). Raw DTO, no behavior.
#[derive(Debug, Clone)]
pub(crate) struct MethodDiagnostics {
    /// Gradle `projectDir` relocations in an unsupported form. `Ok(Some(n>0))` → the
    /// method line states them. `Ok(None)` = not applicable (no unhandled form).
    /// `Err(reason)` = the extraction diagnostics blob could not be read/parsed — the
    /// method JSON carries the reason honestly (review-2 #4: symmetric with
    /// `maven_manifests_present`, so BOTH surfaces — modules list AND orient — render
    /// the Gradle degradation, never a silent drop; standing honesty rule #1).
    pub gradle_projectdir_unhandled: Result<Option<u64>, String>,
    /// Maven manifests present but not parsed. `Ok(Some(n>0))` → the method line
    /// states "Maven manifests present but not parsed on this build". `Ok(None)` =
    /// key absent or zero (not applicable). `Err(reason)` = the extraction
    /// diagnostics blob could not be read or parsed — the method JSON carries the
    /// reason honestly (review-1 fix #2: never `.ok()`/`.flatten()` on a rendered
    /// fallible read).
    pub maven_manifests_present: Result<Option<u64>, String>,
}

impl Default for MethodDiagnostics {
    fn default() -> Self {
        Self {
            gradle_projectdir_unhandled: Ok(None),
            maven_manifests_present: Ok(None),
        }
    }
}

/// The computed method block: family → count, ready for JSON serialization.
/// Each entry is one manifest family (e.g. "Cargo.toml") or "inferred from
/// top-level directories".
#[derive(Debug, Clone)]
pub(crate) struct MethodEntry {
    pub family: String,
    pub count: u64,
    pub label: String,
}

/// The outcome of computing the per-repo method from stored facts — the two
/// mutually-exclusive DOMAIN states (a sum type, per CLAUDE.md domain modeling).
///
/// IO failure (the evidence read itself errored) is NOT a variant here: it is a
/// boundary concern the caller (`build_modules_method_json`) handles as
/// `{unavailable}` BEFORE this pure computation runs — so an unreadable stored fact
/// (Unavailable) stays distinct from stored facts that do not name the method
/// (NotRecorded) all the way to the wire (review-3 #2; Fact Certainty Model).
#[derive(Debug, Clone)]
pub(crate) enum MethodComputation {
    /// Every module's manifest family was named from stored facts.
    Named(Vec<MethodEntry>),
    /// The method cannot be named from stored facts: either NO module candidates
    /// exist, or ≥1 `declared` module has no evidence `source_type`. `reason`
    /// identifies the missing fact for the builder's DECISION_REQUIRED and for
    /// machine consumers; the human line renders the canonical §2.3 sentence.
    /// Spec §2.3: unknown/empty → "method not recorded on this index" + STOP —
    /// NEVER a fabricated "N from <family>" (review-3 #1).
    NotRecorded { reason: String },
}

/// Compute the per-repo method description from the module candidates + their evidence.
///
/// Groups modules by manifest family derived from `module_kind` (declared / inferred /
/// directory) + evidence `source_type` (the stored ecosystem marker from
/// `module_candidate_evidence`). The spec §2.1 requires both columns.
///
/// Returns [`MethodComputation::NotRecorded`] when the stored facts cannot name the
/// method — zero candidates, or any module that [`resolve_family`] cannot attribute to
/// a manifest/inference (a `declared` module with no `source_type`). Otherwise
/// [`MethodComputation::Named`] with one [`MethodEntry`] per family, sorted by count
/// DESC then family ASC; the `label` is the reader-framed sentence fragment.
///
/// The data comes from `module_candidates` + `module_candidate_evidence` — no new
/// inference, no new discovery (frozen invariant).
pub(crate) fn compute_method(modules: &[ModuleMethodInput<'_>]) -> MethodComputation {
    if modules.is_empty() {
        return MethodComputation::NotRecorded {
            reason: "no module candidates on this index".to_string(),
        };
    }

    // Group by manifest family: `module_kind` decides declared vs inferred; for
    // declared, evidence `source_type` names the manifest ecosystem. A module that
    // cannot be attributed (declared without evidence) is UNNAMEABLE — its presence
    // means the method is not fully recorded (§2.3), never a fabricated family.
    let mut by_family: BTreeMap<String, u64> = BTreeMap::new();
    let mut unnameable: u64 = 0;
    for m in modules {
        match resolve_family(m.module_kind, m.source_type) {
            Some(family) => *by_family.entry(family).or_insert(0) += 1,
            None => unnameable += 1,
        }
    }
    if unnameable > 0 {
        return MethodComputation::NotRecorded {
            reason: format!(
                "{unnameable} declared module{} with no manifest evidence (source_type)",
                if unnameable == 1 { "" } else { "s" }
            ),
        };
    }

    let mut entries: Vec<MethodEntry> = by_family
        .into_iter()
        .map(|(family, count)| {
            let label = method_label(&family, count);
            MethodEntry {
                family,
                count,
                label,
            }
        })
        .collect();

    // Sort by count DESC, then family ASC for determinism.
    entries.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.family.cmp(&b.family)));
    MethodComputation::Named(entries)
}

/// Resolve the manifest family key from the stored `module_kind` and evidence
/// `source_type`, or `None` when the module cannot be attributed from stored facts.
///
/// - `module_kind == "inferred"` or `"directory"` → `Some("inferred")` (review-1 fix #4:
///   `"directory"` is a legacy MODULE-node fallback kind, also inferred — both carry
///   the "boundaries are a guess from directory names" caveat)
/// - `module_kind == "declared"` → manifest family from evidence `source_type`:
///   `cargo_toml` → "Cargo.toml", `settings_gradle` → "settings.gradle",
///   `package_json`/`pnpm_workspace_yaml` → "package.json",
///   `pyproject_toml` → "pyproject.toml", `directory_heuristic` → "inferred";
///   an unrecognized-but-present `source_type` is kept verbatim (a real manifest
///   marker we lack a friendly label for — still a NAMED method).
/// - `None` (no evidence) for a `declared`/unknown kind → `None`: the manifest cannot
///   be named from stored facts → NotRecorded (review-3 #1; §2.3). Never "unknown".
fn resolve_family(module_kind: &str, source_type: Option<&str>) -> Option<String> {
    match module_kind {
        "inferred" | "directory" => Some("inferred".to_string()),
        "declared" => match source_type {
            Some("cargo_toml") => Some("Cargo.toml".to_string()),
            Some("settings_gradle") => Some("settings.gradle".to_string()),
            Some("package_json") | Some("pnpm_workspace_yaml") => Some("package.json".to_string()),
            Some("pyproject_toml") => Some("pyproject.toml".to_string()),
            Some("directory_heuristic") => Some("inferred".to_string()),
            Some(other) => Some(other.to_string()),
            None => None, // declared but no manifest evidence → unnameable
        },
        // Unknown module_kind (future-proof): directory_heuristic evidence → inferred;
        // any other present source_type is kept verbatim; no evidence → unnameable
        // (never fabricate a family from the kind string, which §2.3 forbids).
        _other => match source_type {
            Some("directory_heuristic") => Some("inferred".to_string()),
            Some(other) => Some(other.to_string()),
            None => None,
        },
    }
}

/// Map a manifest family + count to a reader-framed label.
///
/// The labels match the spec §2.1 examples:
///   - `Cargo.toml` → "N declared in Cargo.toml (workspace members)"
///   - `settings.gradle` → "N Gradle projects from settings.gradle"
///   - `package.json` → "N npm workspaces from package.json"
///   - `pyproject.toml` → "N declared in pyproject.toml"
///   - `inferred` → "N inferred from top-level directories"
///   - anything else → "N from <family>"
fn method_label(family: &str, count: u64) -> String {
    let s = if count == 1 { "" } else { "s" };
    match family {
        "Cargo.toml" => {
            format!("{count} declared in Cargo.toml (workspace member{s})")
        }
        "settings.gradle" => {
            format!("{count} Gradle project{s} from settings.gradle")
        }
        "package.json" => {
            format!("{count} npm workspace{s} from package.json")
        }
        "pyproject.toml" => {
            format!("{count} declared in pyproject.toml")
        }
        "inferred" => {
            format!(
                "{count} inferred from top-level director{}",
                if count == 1 { "y" } else { "ies" }
            )
        }
        other => format!("{count} from {other}"),
    }
}

/// Build the method line from a [`MethodComputation`] (test helper — the production
/// human line is rendered from JSON by `modules_list::render_method_line_from_json`,
/// which this mirrors for the two DOMAIN states).
///
/// - `Named`: a single-family repo renders one sentence: "Modules: 59 declared in
///   Cargo.toml (workspace members)"; a mixed repo lists each family separated by " · ".
///   §2.3: when ALL modules are inferred, the label carries "boundaries are a guess
///   from directory names".
/// - `NotRecorded`: the canonical §2.3 sentence "Modules: method not recorded on this
///   index" (the missing fact rides in the JSON / DECISION_REQUIRED, not the line).
#[cfg(test)]
pub(crate) fn render_method_line(computation: &MethodComputation) -> String {
    let entries = match computation {
        MethodComputation::Named(entries) => entries,
        MethodComputation::NotRecorded { .. } => {
            return "Modules: method not recorded on this index".to_string();
        }
    };
    let parts: Vec<&str> = entries.iter().map(|e| e.label.as_str()).collect();
    let line = format!("Modules: {}", parts.join(" · "));
    if all_inferred(entries) {
        format!("{line} (boundaries are a guess from directory names)")
    } else {
        line
    }
}

/// Whether ALL modules are inferred (no manifest family). Used by the renderer
/// to decide whether to render the recommendation BEFORE or AFTER the rows
/// (spec §2.3: all-inferred → recommendation first) and to append the "guess"
/// caveat.
pub(crate) fn all_inferred(entries: &[MethodEntry]) -> bool {
    !entries.is_empty() && entries.iter().all(|e| e.family == "inferred")
}

/// Build the method block as a JSON value for the wire protocol, from a
/// [`MethodComputation`] (one of the two DOMAIN states — IO failure is the caller's
/// `{unavailable}`, never reaching here).
///
/// - `Named` → `{"families": [{"family","count","label"}], "all_inferred": bool,
///   "diagnostics": {...}}`. The `diagnostics` object is always present (possibly
///   empty); it carries the existing Gradle projectDir and Maven presence facts.
/// - `NotRecorded` → `{"not_recorded": "<missing-fact reason>"}` — a DISTINCT wire
///   shape from `{unavailable}` so a machine consumer (and the presenter) can tell an
///   absent stored fact from an unreadable one (review-3 #1/#2). Diagnostics are
///   omitted (the edge case where the method itself is unknown).
pub(crate) fn method_to_json(
    computation: &MethodComputation,
    diagnostics: &MethodDiagnostics,
) -> serde_json::Value {
    let entries = match computation {
        MethodComputation::Named(entries) => entries,
        MethodComputation::NotRecorded { reason } => {
            return serde_json::json!({ "not_recorded": reason });
        }
    };
    let families: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| {
            serde_json::json!({
                "family": e.family,
                "count": e.count,
                "label": e.label,
            })
        })
        .collect();

    let is_all_inferred = all_inferred(entries);

    let mut diag = serde_json::Map::new();
    match &diagnostics.gradle_projectdir_unhandled {
        Ok(Some(n)) if *n > 0 => {
            diag.insert(
                "gradle_projectdir_unhandled".to_string(),
                serde_json::json!(n),
            );
        }
        Err(reason) => {
            // review-2 #4: a FAILED Gradle diagnostic read carries the reason, never
            // silently collapsed to "not present" (standing honesty rule #1). Symmetric
            // with `maven_manifests_present_degraded` below.
            diag.insert(
                "gradle_projectdir_unhandled_degraded".to_string(),
                serde_json::json!(reason),
            );
        }
        _ => {} // Ok(None) or Ok(Some(0)): not applicable, nothing to carry
    }
    match &diagnostics.maven_manifests_present {
        Ok(Some(n)) if *n > 0 => {
            diag.insert("maven_manifests_present".to_string(), serde_json::json!(n));
        }
        Err(reason) => {
            // review-1 fix #2: a FAILED diagnostics read carries the reason, never
            // silently collapsed to "not present" (standing honesty rule #1).
            diag.insert(
                "maven_manifests_present_degraded".to_string(),
                serde_json::json!(reason),
            );
        }
        _ => {} // Ok(None) or Ok(Some(0)): not applicable, nothing to carry
    }

    serde_json::json!({
        "families": families,
        "all_inferred": is_all_inferred,
        "diagnostics": diag,
    })
}

// ═══════════════════════════════════════════════════════════════════════════════
// ORIENTATION DOCS
// ═══════════════════════════════════════════════════════════════════════════════

/// A doc entry relevant to the orientation recommendation — the subset of
/// `AgentDocEntry` the recommendation line needs. Raw DTO, no behavior.
pub(crate) struct OrientationDocInput<'a> {
    pub path: &'a str,
    pub kind: &'a str,
    pub generated: bool,
}

/// Select the repo's top orientation docs from the full doc inventory, per the
/// spec §2.2 target list EXACTLY (review-2 #2). Three target classes only:
///
/// 1. **Root README** — a `readme`-kind doc with no `/` in its path (nested
///    READMEs are module-level, not repo-level orientation).
/// 2. **Named orientation files** — a file whose basename stem is (case-insensitive)
///    ARCHITECTURE / DESIGN / OVERVIEW / CONTRIBUTING, located at the repo root OR
///    directly inside a top-level `docs/` or `design/` directory (depth ≤ 1 under
///    those roots). A file buried deeper (`docs/faq/contributing.txt`) is NOT a
///    named target — it is arbitrary prose the classifier merely stamped
///    `architecture` because it lives under `docs/`.
/// 3. **Directory targets** — the literal `docs/` and/or `design/` directories,
///    synthesized when any live (non-generated, non-vendored) doc sits under them.
///    This points the agent at the folder without recommending arbitrary files.
///
/// Why NOT filter on the `architecture` kind: doc-facts `classify_doc_kind` stamps
/// EVERY `.md`/`.rst`/`.txt` and EVERYTHING under `docs/`/`design/` as `architecture`
/// (it is a name/path catch-all, not a semantic judgement). Trusting that kind
/// recommended `docs/faq/admin.txt`, `docs/developers/Bonus_System.md`, etc.
/// (review-2 #2). Selection is by PATH SHAPE, over the SAME classified facts
/// `docs list` produced — no reclassification (frozen invariant).
///
/// Generated docs (MAP.md) and vendored docs (kind `"vendored"`, demoted by the
/// caller via `is_vendored_path`) are excluded FIRST, so a repo whose only `docs/`
/// content is vendored does not spuriously get a `docs/` target.
///
/// Ranked root-first (no `/` first), then path ASC; capped at 4.
pub(crate) fn select_orientation_docs<'a>(docs: &[OrientationDocInput<'a>]) -> Vec<&'a str> {
    let live: Vec<&OrientationDocInput> = docs
        .iter()
        .filter(|d| !d.generated && d.kind != "vendored")
        .collect();

    let mut out: Vec<&'a str> = Vec::new();

    // 1. Root README (kind readme, at the repo root).
    for d in &live {
        if d.kind == "readme" && !d.path.contains('/') {
            out.push(d.path);
        }
    }
    // 2. Named orientation files at the root or one level under docs/design.
    for d in &live {
        if is_named_orientation_file(d.path) {
            out.push(d.path);
        }
    }
    // 3. Directory targets: the docs/ and design/ directories themselves — but
    //    ONLY when no more-specific NAMED file already points inside that directory.
    //    The named file subsumes the folder pointer, matching the §2.2 example
    //    `README.md, docs/ARCHITECTURE.md, docs/design/` where a bare `docs/` is NOT
    //    listed because `docs/ARCHITECTURE.md` already identifies the authors' doc.
    let named_under = |root: &str| {
        live.iter()
            .any(|d| d.path.starts_with(root) && is_named_orientation_file(d.path))
    };
    if live.iter().any(|d| d.path.starts_with("docs/")) && !named_under("docs/") {
        out.push("docs/");
    }
    if live.iter().any(|d| d.path.starts_with("design/")) && !named_under("design/") {
        out.push("design/");
    }

    // Dedup (a path could match README AND a named-file rule only in pathological
    // cases, but dedup defensively so a doc is never listed twice).
    out.sort_unstable();
    out.dedup();

    // Rank root-first (no `/`) then path ASC; cap at 4.
    out.sort_by(|a, b| {
        let a_root = !a.contains('/');
        let b_root = !b.contains('/');
        b_root.cmp(&a_root).then_with(|| a.cmp(b))
    });
    out.truncate(4);
    out
}

/// Whether `path` is a §2.2 *named* orientation file: its documentation stem
/// (doc-facts' `doc_name_stem` — case-insensitive, one documentation extension stripped)
/// is in doc-facts' `ORIENTATION_STEMS` {architecture, design, overview, contributing}
/// (DOCS-DISCOVERY-1: the ONE stem constant discovery and classification read), located
/// at the repo root OR directly under a top-level `docs/` or `design/` directory.
///
/// The depth bound is what excludes `docs/faq/contributing.txt` (stem "contributing"
/// but two levels deep — arbitrary prose, not the authors' architecture doc) while
/// admitting `docs/ARCHITECTURE.md` and root `CONTRIBUTING.md`.
fn is_named_orientation_file(path: &str) -> bool {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    if !repo_graph_doc_facts::doc_name_stem(file_name)
        .is_some_and(|s| repo_graph_doc_facts::ORIENTATION_STEMS.contains(&s))
    {
        return false;
    }
    match path.matches('/').count() {
        0 => true,                                                     // repo root
        1 => path.starts_with("docs/") || path.starts_with("design/"), // docs/NAME, design/NAME
        _ => false, // buried deeper → not a target
    }
}

/// Render the orientation recommendation line.
///
/// When docs exist: "For module boundaries as the authors describe them, read:
/// README.md, docs/ARCHITECTURE.md — or the tree."
///
/// When none: "No README or architecture doc found — the tree is the best
/// orientation."
pub(crate) fn render_recommendation_line(paths: &[&str]) -> String {
    if paths.is_empty() {
        return "No README or architecture doc found — the tree is the best orientation."
            .to_string();
    }
    format!(
        "For module boundaries as the authors describe them, read: {} — or the tree.",
        paths.join(", ")
    )
}

/// The result of computing orientation docs, carrying both the paths and the
/// availability status of the underlying read.
pub(crate) enum OrientationDocsResult {
    /// Docs were successfully read (may be empty — a valid result, not a failure).
    Ok { paths: Vec<String> },
    /// The doc inventory read failed — carry the reason for honest degradation.
    Unavailable { reason: String },
}

/// Build the orientation docs block as a JSON value for the wire protocol.
///
/// Shape: `{"paths": ["README.md", "docs/ARCHITECTURE.md"], "recommendation": "..."}`
/// or `{"unavailable": "reason"}` on read failure.
pub(crate) fn orientation_docs_to_json(result: &OrientationDocsResult) -> serde_json::Value {
    match result {
        OrientationDocsResult::Ok { paths } => {
            let path_refs: Vec<&str> = paths.iter().map(String::as_str).collect();
            serde_json::json!({
                "paths": paths,
                "recommendation": render_recommendation_line(&path_refs),
            })
        }
        OrientationDocsResult::Unavailable { reason } => {
            serde_json::json!({ "unavailable": reason })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Method computation tests ──────────────────────────────────────

    /// Test helper: assert the computation is `Named` and return its entries. A
    /// `NotRecorded` result panics with the missing-fact reason (so a test that
    /// expects a named method fails loudly if the domain state regressed).
    fn named(computation: &MethodComputation) -> &[MethodEntry] {
        match computation {
            MethodComputation::Named(entries) => entries,
            MethodComputation::NotRecorded { reason } => {
                panic!("expected Named, got NotRecorded: {reason}")
            }
        }
    }

    #[test]
    fn method_empty_modules() {
        let computation = compute_method(&[]);
        // review-3 #1: empty candidate set → NotRecorded (a DOMAIN state), not an
        // empty Named list — so the presenter's not-recorded branch (distinct from
        // the unavailable IO-failure branch) fires.
        match &computation {
            MethodComputation::NotRecorded { reason } => {
                assert!(reason.contains("no module candidates"), "reason: {reason}");
            }
            MethodComputation::Named(_) => panic!("empty modules must be NotRecorded"),
        }
        assert_eq!(
            render_method_line(&computation),
            "Modules: method not recorded on this index"
        );
    }

    #[test]
    fn method_single_cargo_family() {
        let modules: Vec<ModuleMethodInput> = (0..59)
            .map(|_| ModuleMethodInput {
                module_kind: "declared",
                source_type: Some("cargo_toml"),
            })
            .collect();
        let computation = compute_method(&modules);
        let entries = named(&computation);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].family, "Cargo.toml");
        assert_eq!(entries[0].count, 59);
        assert_eq!(
            render_method_line(&computation),
            "Modules: 59 declared in Cargo.toml (workspace members)"
        );
        assert!(!all_inferred(entries));
    }

    #[test]
    fn method_single_gradle_family() {
        let modules: Vec<ModuleMethodInput> = (0..67)
            .map(|_| ModuleMethodInput {
                module_kind: "declared",
                source_type: Some("settings_gradle"),
            })
            .collect();
        let computation = compute_method(&modules);
        assert_eq!(named(&computation).len(), 1);
        assert_eq!(
            render_method_line(&computation),
            "Modules: 67 Gradle projects from settings.gradle"
        );
    }

    #[test]
    fn method_single_npm_family() {
        let modules: Vec<ModuleMethodInput> = (0..5)
            .map(|_| ModuleMethodInput {
                module_kind: "declared",
                source_type: Some("package_json"),
            })
            .collect();
        let computation = compute_method(&modules);
        assert_eq!(
            render_method_line(&computation),
            "Modules: 5 npm workspaces from package.json"
        );
    }

    #[test]
    fn method_pnpm_maps_to_package_json_family() {
        let modules = vec![ModuleMethodInput {
            module_kind: "declared",
            source_type: Some("pnpm_workspace_yaml"),
        }];
        let computation = compute_method(&modules);
        assert_eq!(named(&computation)[0].family, "package.json");
    }

    #[test]
    fn method_single_pyproject_family() {
        let modules = vec![ModuleMethodInput {
            module_kind: "declared",
            source_type: Some("pyproject_toml"),
        }];
        let computation = compute_method(&modules);
        assert_eq!(
            render_method_line(&computation),
            "Modules: 1 declared in pyproject.toml"
        );
    }

    #[test]
    fn method_all_inferred() {
        let modules: Vec<ModuleMethodInput> = (0..8)
            .map(|_| ModuleMethodInput {
                module_kind: "inferred",
                source_type: Some("directory_heuristic"),
            })
            .collect();
        let computation = compute_method(&modules);
        assert_eq!(
            render_method_line(&computation),
            "Modules: 8 inferred from top-level directories (boundaries are a guess from directory names)"
        );
        assert!(all_inferred(named(&computation)));
    }

    /// review-1 fix #4: the legacy `"directory"` module_kind (from the MODULE-node
    /// fallback path) is also inferred — it must carry the "boundaries are a guess"
    /// caveat and be placed in the "inferred" family, not rendered as "N from directory".
    #[test]
    fn method_directory_kind_treated_as_inferred() {
        let modules = vec![ModuleMethodInput {
            module_kind: "directory",
            source_type: None, // legacy fallback has no evidence
        }];
        let computation = compute_method(&modules);
        assert_eq!(named(&computation)[0].family, "inferred");
        assert!(all_inferred(named(&computation)));
        assert!(render_method_line(&computation).contains("boundaries are a guess"));
    }

    /// Declared module with `directory_heuristic` evidence (edge case — the evidence
    /// says directory, but the candidate record says declared).
    #[test]
    fn method_declared_with_directory_heuristic_evidence() {
        let modules = vec![ModuleMethodInput {
            module_kind: "declared",
            source_type: Some("directory_heuristic"),
        }];
        let computation = compute_method(&modules);
        assert_eq!(named(&computation)[0].family, "inferred");
    }

    /// review-3 #1: a `declared` module with NO evidence `source_type` is UNNAMEABLE —
    /// its manifest cannot be named from stored facts. Per spec §2.3 the whole method
    /// computes to `NotRecorded` ("method not recorded on this index"), NEVER a
    /// fabricated "unknown" family / "1 from unknown" label. (The prior test
    /// `method_declared_no_evidence_is_unknown` asserted the now-forbidden behavior.)
    #[test]
    fn method_declared_no_evidence_is_not_recorded() {
        let modules = vec![ModuleMethodInput {
            module_kind: "declared",
            source_type: None,
        }];
        let computation = compute_method(&modules);
        match &computation {
            MethodComputation::NotRecorded { reason } => {
                assert!(
                    reason.contains("no manifest evidence"),
                    "reason must identify the missing fact: {reason}"
                );
            }
            MethodComputation::Named(entries) => {
                panic!("declared-without-evidence must be NotRecorded, got: {entries:?}")
            }
        }
        assert_eq!(
            render_method_line(&computation),
            "Modules: method not recorded on this index"
        );
        // The JSON must carry a DISTINCT `not_recorded` key (not `families`, not
        // `unavailable`) so a machine consumer can tell absent-fact from IO-failure.
        let json = method_to_json(&computation, &MethodDiagnostics::default());
        assert!(json.get("not_recorded").is_some(), "json: {json}");
        assert!(json.get("families").is_none(), "json: {json}");
        assert!(json.get("unavailable").is_none(), "json: {json}");
    }

    /// review-3 #1: even a SINGLE unnameable module among otherwise-named ones makes
    /// the whole method NotRecorded — we cannot honestly say "N declared in Cargo.toml"
    /// if one declared module's manifest is unaccounted for (§2.3 unknown → not recorded).
    #[test]
    fn method_mixed_with_one_unnameable_is_not_recorded() {
        let modules = vec![
            ModuleMethodInput {
                module_kind: "declared",
                source_type: Some("cargo_toml"),
            },
            ModuleMethodInput {
                module_kind: "declared",
                source_type: None, // unnameable
            },
        ];
        let computation = compute_method(&modules);
        assert!(
            matches!(computation, MethodComputation::NotRecorded { .. }),
            "one unnameable module → NotRecorded"
        );
    }

    #[test]
    fn method_mixed_families() {
        let modules = vec![
            ModuleMethodInput {
                module_kind: "declared",
                source_type: Some("cargo_toml"),
            },
            ModuleMethodInput {
                module_kind: "declared",
                source_type: Some("cargo_toml"),
            },
            ModuleMethodInput {
                module_kind: "inferred",
                source_type: Some("directory_heuristic"),
            },
        ];
        let computation = compute_method(&modules);
        let entries = named(&computation);
        assert_eq!(entries.len(), 2);
        // cargo(2) first (count DESC), then inferred(1)
        assert_eq!(entries[0].family, "Cargo.toml");
        assert_eq!(entries[0].count, 2);
        assert_eq!(entries[1].family, "inferred");
        assert_eq!(entries[1].count, 1);
        let line = render_method_line(&computation);
        assert!(line.contains("2 declared in Cargo.toml"));
        assert!(line.contains("1 inferred from top-level directory"));
        assert!(line.contains(" · "));
        // Mixed = NOT all-inferred, so no "guess" caveat
        assert!(!line.contains("boundaries are a guess"));
    }

    #[test]
    fn method_unrecognized_source_type() {
        // A declared module with an unrecognized-but-PRESENT evidence source_type
        // names a real manifest marker (we just lack a friendly label) → still a
        // NAMED method, kept verbatim. Distinct from a MISSING source_type (None),
        // which is NotRecorded.
        let modules = vec![ModuleMethodInput {
            module_kind: "declared",
            source_type: Some("bazel_build"),
        }];
        let computation = compute_method(&modules);
        let entries = named(&computation);
        assert_eq!(entries[0].family, "bazel_build");
        assert_eq!(entries[0].label, "1 from bazel_build");
    }

    #[test]
    fn method_to_json_structure() {
        let modules = vec![
            ModuleMethodInput {
                module_kind: "declared",
                source_type: Some("cargo_toml"),
            },
            ModuleMethodInput {
                module_kind: "inferred",
                source_type: Some("directory_heuristic"),
            },
        ];
        let computation = compute_method(&modules);
        let json = method_to_json(&computation, &MethodDiagnostics::default());
        assert!(json["families"].is_array());
        assert_eq!(json["all_inferred"], false);
        // Diagnostics present but empty
        assert!(json["diagnostics"].is_object());
    }

    #[test]
    fn method_to_json_with_diagnostics() {
        let computation = compute_method(&[ModuleMethodInput {
            module_kind: "declared",
            source_type: Some("settings_gradle"),
        }]);
        let diag = MethodDiagnostics {
            gradle_projectdir_unhandled: Ok(Some(3)),
            maven_manifests_present: Ok(Some(8)),
        };
        let json = method_to_json(&computation, &diag);
        assert_eq!(json["diagnostics"]["gradle_projectdir_unhandled"], 3);
        assert_eq!(json["diagnostics"]["maven_manifests_present"], 8);
    }

    /// review-2 #4: a FAILED Gradle projectDir diagnostic read carries the reason on
    /// the wire (symmetric with the Maven degraded path), never a silent absence.
    #[test]
    fn method_to_json_gradle_read_failure_carries_reason() {
        let computation = compute_method(&[ModuleMethodInput {
            module_kind: "declared",
            source_type: Some("settings_gradle"),
        }]);
        let diag = MethodDiagnostics {
            gradle_projectdir_unhandled: Err("extraction diagnostics unreadable".to_string()),
            maven_manifests_present: Ok(None),
        };
        let json = method_to_json(&computation, &diag);
        assert_eq!(
            json["diagnostics"]["gradle_projectdir_unhandled_degraded"],
            "extraction diagnostics unreadable",
            "failed gradle read must carry reason: {json}"
        );
        assert!(
            json["diagnostics"]
                .get("gradle_projectdir_unhandled")
                .is_none(),
            "failed read must NOT carry a count: {json}"
        );
    }

    /// review-1 fix #2: a FAILED Maven diagnostics read carries the reason on the
    /// wire, not a silent absence.
    #[test]
    fn method_to_json_maven_read_failure_carries_reason() {
        let computation = compute_method(&[ModuleMethodInput {
            module_kind: "declared",
            source_type: Some("settings_gradle"),
        }]);
        let diag = MethodDiagnostics {
            gradle_projectdir_unhandled: Ok(None),
            maven_manifests_present: Err("extraction blob not valid JSON".to_string()),
        };
        let json = method_to_json(&computation, &diag);
        assert_eq!(
            json["diagnostics"]["maven_manifests_present_degraded"],
            "extraction blob not valid JSON",
            "failed read must carry reason: {json}"
        );
        assert!(
            json["diagnostics"].get("maven_manifests_present").is_none(),
            "failed read must NOT carry a count: {json}"
        );
    }

    #[test]
    fn method_to_json_zero_diagnostics_omitted() {
        let computation = compute_method(&[ModuleMethodInput {
            module_kind: "declared",
            source_type: Some("cargo_toml"),
        }]);
        let diag = MethodDiagnostics {
            gradle_projectdir_unhandled: Ok(Some(0)),
            maven_manifests_present: Ok(Some(0)),
        };
        let json = method_to_json(&computation, &diag);
        assert!(json["diagnostics"]
            .as_object()
            .map(|m| m.is_empty())
            .unwrap_or(false));
    }

    // ── Inequality property: no two repos render the same method line ──

    #[test]
    fn method_lines_differ_across_families() {
        let cargo_modules: Vec<ModuleMethodInput> = (0..59)
            .map(|_| ModuleMethodInput {
                module_kind: "declared",
                source_type: Some("cargo_toml"),
            })
            .collect();

        let gradle_modules: Vec<ModuleMethodInput> = (0..67)
            .map(|_| ModuleMethodInput {
                module_kind: "declared",
                source_type: Some("settings_gradle"),
            })
            .collect();

        let npm_modules: Vec<ModuleMethodInput> = (0..5)
            .map(|_| ModuleMethodInput {
                module_kind: "declared",
                source_type: Some("package_json"),
            })
            .collect();

        let inferred_modules: Vec<ModuleMethodInput> = (0..8)
            .map(|_| ModuleMethodInput {
                module_kind: "inferred",
                source_type: Some("directory_heuristic"),
            })
            .collect();

        let line_cargo = render_method_line(&compute_method(&cargo_modules));
        let line_gradle = render_method_line(&compute_method(&gradle_modules));
        let line_npm = render_method_line(&compute_method(&npm_modules));
        let line_inferred = render_method_line(&compute_method(&inferred_modules));

        // Each line differs from every other — the acceptance test.
        assert_ne!(line_cargo, line_gradle, "cargo vs gradle must differ");
        assert_ne!(line_cargo, line_npm, "cargo vs npm must differ");
        assert_ne!(line_cargo, line_inferred, "cargo vs inferred must differ");
        assert_ne!(line_gradle, line_npm, "gradle vs npm must differ");
        assert_ne!(line_gradle, line_inferred, "gradle vs inferred must differ");
        assert_ne!(line_npm, line_inferred, "npm vs inferred must differ");
    }

    // ── Orientation docs tests ────────────────────────────────────────

    #[test]
    fn orientation_no_docs() {
        let docs: Vec<OrientationDocInput> = vec![];
        let paths = select_orientation_docs(&docs);
        assert!(paths.is_empty());
        assert_eq!(
            render_recommendation_line(&paths),
            "No README or architecture doc found — the tree is the best orientation."
        );
    }

    #[test]
    fn orientation_root_readme_included() {
        let docs = vec![OrientationDocInput {
            path: "README.md",
            kind: "readme",
            generated: false,
        }];
        let paths = select_orientation_docs(&docs);
        assert_eq!(paths, vec!["README.md"]);
    }

    #[test]
    fn orientation_nested_readme_excluded() {
        // A nested readme (not at root) should NOT be included — it's
        // module-level, not repo-level orientation.
        let docs = vec![OrientationDocInput {
            path: "packages/core/README.md",
            kind: "readme",
            generated: false,
        }];
        let paths = select_orientation_docs(&docs);
        assert!(
            paths.is_empty(),
            "nested readme must be excluded: {paths:?}"
        );
    }

    // DOCS-DISCOVERY-1 (RG-REQ-008-L07): the named-file rule reads doc-facts' ONE stem constant
    // (`doc_name_stem` + `ORIENTATION_STEMS`) — any documentation extension, case-insensitive;
    // documentation stems that are not orientation stems (INSTALL, AUTHORS) are not named targets;
    // the depth rule is unchanged.
    #[test]
    fn orientation_named_files_match_by_stem_with_any_doc_extension() {
        for p in [
            "CONTRIBUTING.rst",
            "docs/ARCHITECTURE.adoc",
            "design/overview.txt",
            "Design.markdown",
            "OVERVIEW",
        ] {
            assert!(is_named_orientation_file(p), "{p} must be a named target");
        }
        for p in [
            "INSTALL",
            "AUTHORS",
            "README.rst",
            "docs/deep/ARCHITECTURE.md",
            "OVERVIEW.html",
            "docs/design.yaml",
        ] {
            assert!(
                !is_named_orientation_file(p),
                "{p} must NOT be a named target"
            );
        }
        let docs = vec![
            OrientationDocInput {
                path: "CONTRIBUTING.rst",
                kind: "doc",
                generated: false,
            },
            OrientationDocInput {
                path: "README.rst",
                kind: "readme",
                generated: false,
            },
            OrientationDocInput {
                path: "INSTALL",
                kind: "doc",
                generated: false,
            },
            OrientationDocInput {
                path: "docs/ARCHITECTURE.adoc",
                kind: "architecture",
                generated: false,
            },
            OrientationDocInput {
                path: "docs/deep/ARCHITECTURE.md",
                kind: "architecture",
                generated: false,
            },
        ];
        assert_eq!(
            select_orientation_docs(&docs),
            vec!["CONTRIBUTING.rst", "README.rst", "docs/ARCHITECTURE.adoc"]
        );
    }

    #[test]
    fn orientation_named_files_root_and_docs_level() {
        // A named file at root (CONTRIBUTING.md) and one directly under docs/
        // (docs/ARCHITECTURE.md) are BOTH §2.2 named targets. The named file under
        // docs/ suppresses the bare `docs/` directory target (it subsumes it).
        let docs = vec![
            OrientationDocInput {
                path: "docs/ARCHITECTURE.md",
                kind: "architecture",
                generated: false,
            },
            OrientationDocInput {
                path: "CONTRIBUTING.md",
                kind: "architecture",
                generated: false,
            },
        ];
        let paths = select_orientation_docs(&docs);
        assert_eq!(
            paths,
            vec!["CONTRIBUTING.md", "docs/ARCHITECTURE.md"],
            "named files only; docs/ suppressed by the named file under it: {paths:?}"
        );
    }

    #[test]
    fn orientation_multiple_docs_capped_at_four() {
        let docs = vec![
            OrientationDocInput {
                path: "README.md",
                kind: "readme",
                generated: false,
            },
            OrientationDocInput {
                path: "docs/ARCHITECTURE.md",
                kind: "architecture",
                generated: false,
            },
            OrientationDocInput {
                path: "CONTRIBUTING.md",
                kind: "architecture",
                generated: false,
            },
            OrientationDocInput {
                path: "docs/DESIGN.md",
                kind: "architecture",
                generated: false,
            },
            OrientationDocInput {
                path: "docs/OVERVIEW.md",
                kind: "architecture",
                generated: false,
            },
        ];
        let paths = select_orientation_docs(&docs);
        assert_eq!(paths.len(), 4, "capped at 4: {paths:?}");
        // Root files first (no `/`), then docs/ named files (alpha). The bare `docs/`
        // target is suppressed (named files under docs/ subsume it). docs/OVERVIEW.md
        // drops off the 4-cap.
        assert_eq!(paths[0], "CONTRIBUTING.md");
        assert_eq!(paths[1], "README.md");
        assert_eq!(paths[2], "docs/ARCHITECTURE.md");
        assert_eq!(paths[3], "docs/DESIGN.md");
    }

    #[test]
    fn orientation_root_first_ranking() {
        let docs = vec![
            OrientationDocInput {
                path: "docs/ARCHITECTURE.md",
                kind: "architecture",
                generated: false,
            },
            OrientationDocInput {
                path: "README.md",
                kind: "readme",
                generated: false,
            },
        ];
        let paths = select_orientation_docs(&docs);
        assert_eq!(paths[0], "README.md", "root files rank first");
        assert_eq!(paths[1], "docs/ARCHITECTURE.md");
        assert_eq!(
            paths.len(),
            2,
            "no bare docs/ (named file subsumes it): {paths:?}"
        );
    }

    /// §2.2 directory target: a `docs/` folder with only ARBITRARY prose (no named
    /// orientation file, no root README) yields ONLY the `docs/` directory pointer —
    /// never the arbitrary files themselves. This is the django/vcmi shape.
    #[test]
    fn orientation_docs_dir_target_when_only_arbitrary_prose() {
        let docs = vec![
            OrientationDocInput {
                path: "docs/faq/admin.txt",
                kind: "architecture", // classifier catch-all for docs/ + .txt
                generated: false,
            },
            OrientationDocInput {
                path: "docs/developers/Bonus_System.md",
                kind: "architecture",
                generated: false,
            },
            OrientationDocInput {
                path: "docs/FUTURE-ITERATIONS.md",
                kind: "architecture",
                generated: false,
            },
        ];
        let paths = select_orientation_docs(&docs);
        assert_eq!(
            paths,
            vec!["docs/"],
            "only the docs/ directory target, never arbitrary files under it: {paths:?}"
        );
    }

    /// review-2 #2 NEGATIVE tests: arbitrary files the classifier stamped `architecture`
    /// (because they live under `docs/` or end in `.md`/`.txt`) must NEVER be recommended
    /// as individual paths. Buried "named-looking" files (docs/faq/contributing.txt) and
    /// LICENSE are excluded too.
    #[test]
    fn orientation_never_recommends_arbitrary_docs_files() {
        let docs = vec![
            OrientationDocInput {
                path: "docs/faq/admin.txt",
                kind: "architecture",
                generated: false,
            },
            OrientationDocInput {
                path: "docs/faq/contributing.txt", // stem "contributing" BUT depth 2 → not a target
                kind: "architecture",
                generated: false,
            },
            OrientationDocInput {
                path: "docs/developers/Bonus_System.md",
                kind: "architecture",
                generated: false,
            },
            OrientationDocInput {
                path: "docs/developers/Building_Android.md",
                kind: "architecture",
                generated: false,
            },
            OrientationDocInput {
                path: "docs/FUTURE-ITERATIONS.md",
                kind: "architecture",
                generated: false,
            },
            OrientationDocInput {
                path: "LICENSE.txt",
                kind: "license",
                generated: false,
            },
        ];
        let paths = select_orientation_docs(&docs);
        // The docs/ directory target is the ONLY recommendation; not one arbitrary file.
        assert_eq!(paths, vec!["docs/"], "got: {paths:?}");
        for arbitrary in [
            "docs/faq/admin.txt",
            "docs/faq/contributing.txt",
            "docs/developers/Bonus_System.md",
            "docs/developers/Building_Android.md",
            "docs/FUTURE-ITERATIONS.md",
            "LICENSE.txt",
        ] {
            assert!(
                !paths.contains(&arbitrary),
                "{arbitrary} must NOT be recommended: {paths:?}"
            );
        }
    }

    /// A `design/` directory target is synthesized the same way (top-level design/).
    #[test]
    fn orientation_design_dir_target() {
        let docs = vec![OrientationDocInput {
            path: "design/rfc-0001.md",
            kind: "architecture",
            generated: false,
        }];
        let paths = select_orientation_docs(&docs);
        assert_eq!(paths, vec!["design/"], "got: {paths:?}");
    }

    /// A vendored doc under docs/ must NOT trip the `docs/` directory target (the caller
    /// demotes vendored paths to kind "vendored"; select filters them FIRST).
    #[test]
    fn orientation_vendored_under_docs_excluded() {
        let docs = vec![OrientationDocInput {
            path: "docs/_theme/vendor/LICENSE.txt",
            kind: "vendored",
            generated: false,
        }];
        let paths = select_orientation_docs(&docs);
        assert!(
            paths.is_empty(),
            "a vendored-only docs/ tree yields no target: {paths:?}"
        );
    }

    /// A `doc/` (singular) directory is NOT a §2.2 target — only `docs/` and `design/`.
    #[test]
    fn orientation_singular_doc_dir_not_a_target() {
        let docs = vec![
            OrientationDocInput {
                path: "doc/impl.md",
                kind: "architecture",
                generated: false,
            },
            OrientationDocInput {
                path: "doc/index.md",
                kind: "architecture",
                generated: false,
            },
        ];
        let paths = select_orientation_docs(&docs);
        assert!(
            paths.is_empty(),
            "doc/ (singular) is not docs/ or design/: {paths:?}"
        );
    }

    #[test]
    fn orientation_excludes_generated_docs() {
        let docs = vec![
            OrientationDocInput {
                path: "README.md",
                kind: "readme",
                generated: false,
            },
            OrientationDocInput {
                path: "MAP.md",
                kind: "map",
                generated: true,
            },
        ];
        let paths = select_orientation_docs(&docs);
        assert_eq!(paths, vec!["README.md"]);
    }

    #[test]
    fn orientation_excludes_config_and_license() {
        let docs = vec![
            OrientationDocInput {
                path: "docker-compose.yml",
                kind: "config",
                generated: false,
            },
            OrientationDocInput {
                path: "LICENSE",
                kind: "license",
                generated: false,
            },
        ];
        let paths = select_orientation_docs(&docs);
        assert!(paths.is_empty());
    }

    #[test]
    fn orientation_recommendation_lines_differ_across_repos() {
        // Repo with README only
        let line1 = render_recommendation_line(&["README.md"]);
        // Repo with README + ARCHITECTURE
        let line2 = render_recommendation_line(&["README.md", "docs/ARCHITECTURE.md"]);
        // Repo with nothing
        let line3 = render_recommendation_line(&[]);

        assert_ne!(line1, line2, "different doc sets must render differently");
        assert_ne!(line1, line3, "docs vs no-docs must differ");
        assert_ne!(line2, line3, "docs vs no-docs must differ");
    }

    // ── JSON serialization tests ──────────────────────────────────────

    #[test]
    fn orientation_docs_ok_to_json() {
        let result = OrientationDocsResult::Ok {
            paths: vec!["README.md".to_string(), "docs/ARCHITECTURE.md".to_string()],
        };
        let json = orientation_docs_to_json(&result);
        assert_eq!(json["paths"].as_array().unwrap().len(), 2);
        assert!(json["recommendation"]
            .as_str()
            .unwrap()
            .contains("README.md"));
    }

    #[test]
    fn orientation_docs_empty_ok_to_json() {
        let result = OrientationDocsResult::Ok { paths: vec![] };
        let json = orientation_docs_to_json(&result);
        assert_eq!(json["paths"].as_array().unwrap().len(), 0);
        assert!(json["recommendation"]
            .as_str()
            .unwrap()
            .contains("No README"));
    }

    #[test]
    fn orientation_docs_unavailable_to_json() {
        let result = OrientationDocsResult::Unavailable {
            reason: "storage read failed: disk full".to_string(),
        };
        let json = orientation_docs_to_json(&result);
        assert_eq!(json["unavailable"], "storage read failed: disk full");
        assert!(json.get("paths").is_none());
    }
}
