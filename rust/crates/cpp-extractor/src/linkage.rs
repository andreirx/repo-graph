//! C ABI boundary detection via `extern "C"` linkage specifications.
//!
//! Detects direct syntax for:
//! - `extern "C" { ... }` blocks
//! - `extern "C" void func();` single declarations
//! - `extern "C++"` (rare, but exists)
//!
//! Does NOT detect:
//! - Macro-wrapped linkage (`BEGIN_EXTERN_C`, `__BEGIN_DECLS`, etc.)
//! - Inferred linkage from header patterns

use serde::{Deserialize, Serialize};

/// Language linkage for a symbol or declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LanguageLinkage {
    /// `extern "C"` linkage
    C,
    /// `extern "C++"` linkage (explicit, rare)
    Cpp,
}

/// Linkage metadata for a symbol.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LinkageMetadata {
    /// Explicit language linkage if declared via `extern "C"` or `extern "C++"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_linkage: Option<LanguageLinkage>,

    /// True if symbol is inside an `extern "C" { }` block.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub declared_in_extern_c_block: bool,
}

#[allow(dead_code)]
impl LinkageMetadata {
    /// Create metadata for a symbol inside an `extern "C"` block.
    pub fn in_extern_c_block() -> Self {
        Self {
            language_linkage: Some(LanguageLinkage::C),
            declared_in_extern_c_block: true,
        }
    }

    /// Create metadata for a symbol with direct `extern "C"` declaration.
    pub fn extern_c_declaration() -> Self {
        Self {
            language_linkage: Some(LanguageLinkage::C),
            declared_in_extern_c_block: false,
        }
    }

    /// Create metadata for a symbol with `extern "C++"` declaration.
    pub fn extern_cpp_declaration() -> Self {
        Self {
            language_linkage: Some(LanguageLinkage::Cpp),
            declared_in_extern_c_block: false,
        }
    }

    /// Returns true if this symbol has any C ABI boundary evidence.
    pub fn is_c_abi_boundary(&self) -> bool {
        matches!(self.language_linkage, Some(LanguageLinkage::C))
    }

    /// Merge with parent scope linkage (for symbols inside extern blocks).
    pub fn with_parent_linkage(mut self, parent: Option<LanguageLinkage>) -> Self {
        if self.language_linkage.is_none() {
            if let Some(LanguageLinkage::C) = parent {
                self.language_linkage = Some(LanguageLinkage::C);
                self.declared_in_extern_c_block = true;
            }
        }
        self
    }

    /// Convert to JSON for metadata_json field.
    pub fn to_json(&self) -> Option<String> {
        if self.language_linkage.is_none() && !self.declared_in_extern_c_block {
            None
        } else {
            serde_json::to_string(self).ok()
        }
    }
}

/// File-level linkage statistics for quick queries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileLinkageStats {
    /// True if file contains any `extern "C"` declarations or blocks.
    pub has_extern_c_declarations: bool,
    /// Count of symbols with C linkage in this file.
    pub extern_c_symbol_count: u32,
}

impl FileLinkageStats {
    /// Record a symbol with C linkage.
    pub fn record_extern_c_symbol(&mut self) {
        self.has_extern_c_declarations = true;
        self.extern_c_symbol_count += 1;
    }

    /// Convert to JSON for file metadata.
    pub fn to_json(&self) -> Option<String> {
        if !self.has_extern_c_declarations {
            None
        } else {
            serde_json::to_string(self).ok()
        }
    }
}

/// Extract linkage from a `linkage_specification` node.
///
/// tree-sitter-cpp represents:
/// - `extern "C" { ... }` as linkage_specification with string_literal "C"
/// - `extern "C" void func();` as linkage_specification wrapping the declaration
pub fn extract_linkage_from_spec(node: &tree_sitter::Node, src: &[u8]) -> Option<LanguageLinkage> {
    // Find the string literal child that specifies the language
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "string_literal" {
            let text = child.utf8_text(src).ok()?;
            // Strip quotes
            let inner = text.trim_matches('"');
            return match inner {
                "C" => Some(LanguageLinkage::C),
                "C++" => Some(LanguageLinkage::Cpp),
                _ => None,
            };
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linkage_metadata_json_empty_when_no_linkage() {
        let meta = LinkageMetadata::default();
        assert!(meta.to_json().is_none());
    }

    #[test]
    fn linkage_metadata_json_present_for_extern_c() {
        let meta = LinkageMetadata::in_extern_c_block();
        let json = meta.to_json().unwrap();
        assert!(json.contains("\"language_linkage\":\"c\""));
        assert!(json.contains("\"declared_in_extern_c_block\":true"));
    }

    #[test]
    fn is_c_abi_boundary_true_for_c_linkage() {
        let meta = LinkageMetadata::extern_c_declaration();
        assert!(meta.is_c_abi_boundary());
    }

    #[test]
    fn is_c_abi_boundary_false_for_cpp_linkage() {
        let meta = LinkageMetadata::extern_cpp_declaration();
        assert!(!meta.is_c_abi_boundary());
    }

    #[test]
    fn file_stats_json_empty_when_no_extern_c() {
        let stats = FileLinkageStats::default();
        assert!(stats.to_json().is_none());
    }

    #[test]
    fn file_stats_tracks_extern_c_symbols() {
        let mut stats = FileLinkageStats::default();
        stats.record_extern_c_symbol();
        stats.record_extern_c_symbol();
        assert!(stats.has_extern_c_declarations);
        assert_eq!(stats.extern_c_symbol_count, 2);
        let json = stats.to_json().unwrap();
        assert!(json.contains("\"extern_c_symbol_count\":2"));
    }
}
