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
    /// Manifest file path (if available).
    pub manifest_path: Option<String>,
    /// Whether manifest dependency context is available.
    /// `false` for Python/Java until compose.rs attaches their contexts.
    pub manifest_scope_available: bool,
    /// Dependencies by category.
    pub entries: Vec<DependencyEntry>,
}

impl ModuleDependencySummary {
    /// Get entries by category.
    pub fn by_category(&self, category: DependencyCategory) -> Vec<&DependencyEntry> {
        self.entries.iter().filter(|e| e.category == category).collect()
    }

    /// Count of declared and used dependencies.
    pub fn declared_and_used_count(&self) -> usize {
        self.by_category(DependencyCategory::DeclaredAndUsed).len()
    }

    /// Count of declared but unobserved dependencies.
    pub fn declared_but_unobserved_count(&self) -> usize {
        self.by_category(DependencyCategory::DeclaredButUnobserved).len()
    }

    /// Count of observed but undeclared dependencies.
    pub fn observed_but_undeclared_count(&self) -> usize {
        self.by_category(DependencyCategory::ObservedButUndeclared).len()
    }

    /// Count of runtime builtin usages.
    pub fn runtime_builtins_count(&self) -> usize {
        self.by_category(DependencyCategory::RuntimeBuiltin).len()
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
