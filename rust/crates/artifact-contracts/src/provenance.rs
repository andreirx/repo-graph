//! Provenance policy enumeration.
//!
//! Defines how artifact provenance is tracked and anchored to Layer 0.

use serde::{Deserialize, Serialize};

/// How artifact provenance is tracked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenancePolicy {
    /// Provenance is the source file itself.
    ///
    /// The artifact is directly extracted from a source file.
    /// Provenance is implicit: file path + content hash.
    ///
    /// Applies to: All Layer 0-1 extracted facts
    DirectFromSourceFile,

    /// Provenance is specific Layer 0 stable keys.
    ///
    /// The artifact is derived from specific Layer 0 items.
    /// Provenance must be recorded per-row as a list of
    /// stable keys from the Layer 0 families it depends on.
    ///
    /// Applies to: Deterministic relationships, hints/inferences
    DerivedFromLayer0Items,

    /// Provenance is other artifact families (transitive).
    ///
    /// The artifact depends on multiple layers of derived data.
    /// Provenance traces through intermediate families back to Layer 0.
    ///
    /// Applies to: Complex projections
    DerivedFromArtifactFamilies,

    /// No automated provenance. Human-authored content.
    ///
    /// The artifact was created by human declaration, not extraction.
    /// Provenance is the declaration itself, not source code.
    ///
    /// Applies to: Governance overlays
    HumanAuthored,
}

impl ProvenancePolicy {
    /// Returns true if this policy requires per-row provenance tracking.
    pub fn requires_row_provenance(&self) -> bool {
        matches!(
            self,
            Self::DerivedFromLayer0Items | Self::DerivedFromArtifactFamilies
        )
    }

    /// Returns true if provenance is implicit from source file.
    pub fn is_implicit(&self) -> bool {
        matches!(self, Self::DirectFromSourceFile)
    }

    /// Returns true if this is human-authored content.
    pub fn is_human_authored(&self) -> bool {
        matches!(self, Self::HumanAuthored)
    }
}

impl std::fmt::Display for ProvenancePolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DirectFromSourceFile => write!(f, "direct_from_source_file"),
            Self::DerivedFromLayer0Items => write!(f, "derived_from_layer0_items"),
            Self::DerivedFromArtifactFamilies => write!(f, "derived_from_artifact_families"),
            Self::HumanAuthored => write!(f, "human_authored"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Provenance data structures (for storage and serialization)
// ═══════════════════════════════════════════════════════════════════════════

/// A reference to a Layer 0 artifact that this row depends on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProvenanceAnchor {
    /// The artifact family of the dependency.
    pub family: String,
    /// The stable key of the dependency.
    pub stable_key: String,
}

impl ProvenanceAnchor {
    /// Create a new provenance anchor.
    pub fn new(family: &str, stable_key: &str) -> Self {
        Self {
            family: family.to_string(),
            stable_key: stable_key.to_string(),
        }
    }
}

/// Provenance record for a derived artifact row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    /// Schema version for forward compatibility.
    pub version: u32,

    /// Layer 0 items this artifact depends on.
    pub depends_on: Vec<ProvenanceAnchor>,

    /// Optional: extractor/detector that created this artifact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extractor: Option<String>,

    /// Optional: additional extraction context (confidence, version, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extraction_context: Option<serde_json::Value>,
}

impl Provenance {
    /// Create provenance from Layer 0 item references.
    pub fn from_layer0_items(anchors: Vec<ProvenanceAnchor>) -> Self {
        Self {
            version: 1,
            depends_on: anchors,
            extractor: None,
            extraction_context: None,
        }
    }

    /// Add extractor information.
    pub fn with_extractor(mut self, extractor: &str) -> Self {
        self.extractor = Some(extractor.to_string());
        self
    }

    /// Add extraction context.
    pub fn with_context(mut self, context: serde_json::Value) -> Self {
        self.extraction_context = Some(context);
        self
    }

    /// Check if any anchor matches a given stable key.
    pub fn depends_on_key(&self, stable_key: &str) -> bool {
        self.depends_on.iter().any(|a| a.stable_key == stable_key)
    }

    /// Get all stable keys this provenance depends on.
    pub fn all_stable_keys(&self) -> Vec<&str> {
        self.depends_on.iter().map(|a| a.stable_key.as_str()).collect()
    }
}

impl Default for Provenance {
    fn default() -> Self {
        Self {
            version: 1,
            depends_on: Vec::new(),
            extractor: None,
            extraction_context: None,
        }
    }
}
