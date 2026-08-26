//! Dependency reconciliation DTOs.
//!
//! These types are the contract between the reconciliation engine
//! and its consumers (CLI, future APIs).

use serde::{Deserialize, Serialize};

/// Dependency category in a reconciliation summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyCategory {
    /// In manifest AND observed in source imports.
    DeclaredAndUsed,
    /// In manifest but no source imports found.
    DeclaredButUnobserved,
    /// Source imports exist but not in manifest.
    ObservedButUndeclared,
    /// Runtime builtin module (fs, path, std::*).
    RuntimeBuiltin,
    /// External-looking specifier, classification unclear.
    UnknownExternalLike,
}

/// A single package usage in a dependency summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageUsage {
    /// Normalized package name (e.g., "react", "lodash", "tokio").
    pub package: String,
    /// Number of import statements referencing this package.
    pub import_count: usize,
    /// Dependency class from manifest (prod, dev, peer, optional).
    /// `None` if not declared or manifest doesn't provide class.
    pub dependency_class: Option<String>,
    /// Confidence score (1.0 for exact match, <1.0 for ambiguous).
    pub confidence: f64,
}

/// Provenance of the manifest a module's declared dependencies were parsed from
/// (DEPS-LIST-REWRITE-1 §2.2; operator ruling 2026-08-26).
///
/// Orthogonal to [`ModuleDependencySummary::manifest_scope_available`] (which gates
/// reconciliation): a module can carry declared deps to reconcile against while its exact
/// manifest FILE is unknown (a snapshot indexed before provenance tracking). Sum type rather
/// than `Option<String> + bool` because a "note" written into a path string is a
/// fabricated-looking value — the exact defect this slice removes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ManifestContext {
    /// The exact manifest file parsed at index time (`path` is repo-relative, e.g.
    /// `spring-petclinic/build.gradle.kts` — rendered as itself, never a fixed-name guess).
    Parsed { path: String },
    /// Declared deps were parsed, but the exact manifest FILE could not be pinned. `reason`
    /// is the SPECIFIC honest cause (operator ruling 3, item 2) — never collapsed to one label:
    /// "indexed before provenance tracking" (old snapshot), "extraction diagnostics unreadable:
    /// …" (read failure), "provenance record malformed: …" (corruption), or "declared deps parsed
    /// but no manifest record covers this module". Always unknown-with-reason, never a fabricated
    /// default and never a query-time filesystem rescan.
    ProvenanceUnavailable { reason: String },
    /// No manifest reader ran / no owning manifest for this module.
    Absent,
}

impl ManifestContext {
    /// The exact parsed manifest path, if known. `None` for both unavailable and absent —
    /// callers distinguish those via [`Self::unavailable_note`].
    pub fn path(&self) -> Option<&str> {
        match self {
            ManifestContext::Parsed { path } => Some(path.as_str()),
            _ => None,
        }
    }

    /// The specific unknown-with-reason note when deps were parsed but the exact manifest file
    /// could not be pinned (operator ruling 3, item 2 — the cause is carried verbatim, not
    /// collapsed to a single "predates tracking" label that would lie about a read failure).
    pub fn unavailable_note(&self) -> Option<&str> {
        match self {
            ManifestContext::ProvenanceUnavailable { reason } => Some(reason.as_str()),
            _ => None,
        }
    }
}

/// Outcome of reading the persisted parsed-manifest provenance for a snapshot (operator ruling 3,
/// item 2). A quad-state rather than `Option<Vec<_>>` because the three "no records" causes are NOT
/// the same fact — an old snapshot (absent) must not be reported with a corrupt-blob's reason, and
/// vice versa. Produced by the query layer's diagnostics read, consumed by [`super::compose`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvenanceRead {
    /// Provenance was tracked; these are the manifests actually parsed (possibly empty = tracked,
    /// none parsed). Each module attaches its exact manifest by longest-ancestor-`dir` match.
    Tracked(Vec<ManifestProvenance>),
    /// The snapshot predates manifest-provenance tracking (no `deps_manifests` diagnostics record).
    /// Renders "indexed before provenance tracking".
    Absent,
    /// The provenance record could not be read/parsed (diagnostics blob unreadable, not valid JSON,
    /// or a malformed `deps_manifests` value). `reason` is the specific honest cause — NEVER
    /// conflated with the old-snapshot case.
    Unavailable { reason: String },
}

/// A persisted parsed-manifest record (DEPS-LIST-REWRITE-1 §2.2; operator ruling 2026-08-26).
///
/// Written at index time through the extraction-diagnostics channel (the `deps_manifests` key
/// beside `index_basis`) BEFORE the Ready flip, read back at query time. Raw strings — a boundary
/// DTO, no framework/hardware types. `dir` is the manifest's repo-relative directory; the query
/// layer attaches a manifest to a module by longest-ancestor-`dir` match against these records
/// (the same nearest-manifest semantics the index used), never a filesystem rescan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestProvenance {
    /// Repo-relative path of the manifest file the resolver ENCOUNTERED (`build.gradle.kts`,
    /// `pyproject.toml`). Present whether the read+parse succeeded or the file was present-but-
    /// unreadable — [`Self::error`] distinguishes the two.
    pub path: String,
    /// Repo-relative directory the manifest governs (used for module attribution).
    pub dir: String,
    /// Ecosystem the reader belongs to (`npm`/`cargo`/`python`/`java`).
    pub ecosystem: String,
    /// `None` = the manifest was read AND parsed (declared deps possibly empty — a legitimate
    /// measured-empty, ruling-3 item 3). `Some(reason)` = the manifest was PRESENT but could not be
    /// parsed (an io read error, or malformed content the reader could detect) — review-4 item 1: a
    /// failed read is NEVER laundered into a parsed zero-dep. `#[serde(default)]` so a snapshot
    /// written before this field decodes as parsed (backward-compatible).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A single entry in a dependency summary.
/// Note: No Eq impl because confidence is f64.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DependencyEntry {
    /// Normalized package name.
    pub package: String,
    /// Dependency category.
    pub category: DependencyCategory,
    /// Number of import statements referencing this package.
    pub import_count: usize,
    /// Dependency class from manifest (prod, dev, peer, optional).
    pub dependency_class: Option<String>,
    /// Confidence score for classification.
    pub confidence: f64,
    /// Raw specifiers that normalized to this package (for debugging).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub raw_specifiers: Vec<String>,
}

/// Module-level dependency summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleDependencySummary {
    /// Module identifier (canonical_root_path).
    pub module: String,
    /// Provenance of the manifest this module's declared deps were parsed from (§2.2). Never a
    /// fabricated fixed-name path: exact file when provenance was tracked, else unavailable/absent.
    pub manifest_context: ManifestContext,
    /// Whether manifest dependency context is available for RECONCILIATION (declared deps exist to
    /// join against). Orthogonal to `manifest_context`: an old snapshot can have scope available
    /// (declared deps present) while its exact manifest file is `ProvenanceUnavailable`.
    pub manifest_scope_available: bool,
    /// Dependencies by category.
    pub entries: Vec<DependencyEntry>,
    /// Count of observed external references dropped as non-import call-expression text
    /// (DEPS-LIST-REWRITE-1 §2.1) — kept for honest diagnostics, never hoisted into a
    /// package category. `0` means the classifier rejected nothing for this module.
    pub rejected_non_specifier: usize,
}

impl ModuleDependencySummary {
    /// Get entries by category.
    pub fn by_category(&self, category: DependencyCategory) -> Vec<&DependencyEntry> {
        self.entries
            .iter()
            .filter(|e| e.category == category)
            .collect()
    }

    /// Count of declared and used dependencies.
    pub fn declared_and_used_count(&self) -> usize {
        self.by_category(DependencyCategory::DeclaredAndUsed).len()
    }

    /// Count of declared but unobserved dependencies.
    pub fn declared_but_unobserved_count(&self) -> usize {
        self.by_category(DependencyCategory::DeclaredButUnobserved)
            .len()
    }

    /// Count of observed but undeclared dependencies.
    pub fn observed_but_undeclared_count(&self) -> usize {
        self.by_category(DependencyCategory::ObservedButUndeclared)
            .len()
    }

    /// Count of runtime builtin usages.
    pub fn runtime_builtins_count(&self) -> usize {
        self.by_category(DependencyCategory::RuntimeBuiltin).len()
    }

    /// Count of external-looking specifiers that could not be classified declared/undeclared
    /// because no manifest scope was available (the `none-detected` per-module case). Surfaced so a
    /// scope-unavailable module row is NOT rendered as a deceptive `0/0/0/0` when it in fact has
    /// external references (leveldb's C/C++ `<algorithm>`/`<cassert>` includes).
    pub fn unknown_external_like_count(&self) -> usize {
        self.by_category(DependencyCategory::UnknownExternalLike)
            .len()
    }
}

/// Kind of dependency drift anomaly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftKind {
    /// Dependency declared but never imported.
    UnusedDeclared,
    /// Import observed but dependency not declared.
    UndeclaredUsage,
    /// External-looking specifier that couldn't be classified.
    UnknownExternal,
}

/// A single drift anomaly entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriftEntry {
    /// Module where the anomaly was detected.
    pub module: String,
    /// Package or specifier involved.
    pub package: String,
    /// Kind of drift.
    pub kind: DriftKind,
    /// Number of imports (for usage-based drift).
    pub import_count: usize,
    /// Hint for resolution (e.g., "likely devDependency missing").
    pub hint: Option<String>,
}
