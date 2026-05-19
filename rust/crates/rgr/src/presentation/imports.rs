//! Presentation layer for the `imports` command.
//!
//! # CLI-OUT-3
//!
//! Renders file/module dependency listing as human-readable text.
//! Shows direct imports with resolution status.
//!
//! ## Human Output Structure
//!
//! ```text
//! Imports: src/Engine/State.cpp
//!
//! 19 imports
//!
//!   src/Engine/Game.h                  depth=1  static
//!   src/Engine/InteractiveSurface.h    depth=1  static
//!   src/Engine/Language.h              depth=1  static
//!   ...
//! ```

use serde::Deserialize;

// ── Response Types ───────────────────────────────────────────────────────────

/// An import edge in the response.
#[derive(Debug, Clone, Deserialize)]
pub struct ImportEntry {
    #[serde(default)]
    pub node_id: String,
    /// The imported symbol/file path.
    pub symbol: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub subtype: String,
    /// The file being imported (often same as symbol for file imports).
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub line: u32,
    #[serde(default)]
    pub column: u32,
    #[serde(default)]
    pub edge_type: String,
    #[serde(default)]
    pub resolution: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub depth: u32,
}

/// Response structure for imports command.
#[derive(Debug, Deserialize)]
pub struct ImportsResponse {
    /// The file being queried.
    pub file: String,
    /// List of imports.
    pub imports: Vec<ImportEntry>,
}

// ── Human Rendering ──────────────────────────────────────────────────────────

impl ImportsResponse {
    /// Render as human-readable text.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // ── Header ─────────────────────────────────────────────────
        out.push_str(&format!("Imports: {}\n\n", self.file));

        // ── Count ──────────────────────────────────────────────────
        let count = self.imports.len();
        if count == 1 {
            out.push_str("1 import\n");
        } else {
            out.push_str(&format!("{} imports\n", count));
        }

        if self.imports.is_empty() {
            return out;
        }

        out.push('\n');

        // ── Import list ────────────────────────────────────────────
        for imp in &self.imports {
            // Use symbol as the display name (usually the imported file path)
            let name = &imp.symbol;
            let depth = imp.depth;
            let resolution = if imp.resolution.is_empty() {
                "-"
            } else {
                &imp.resolution
            };

            out.push_str(&format!("  {}  depth={}  {}\n", name, depth, resolution));
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_imports() -> ImportsResponse {
        ImportsResponse {
            file: "src/main.cpp".to_string(),
            imports: vec![
                ImportEntry {
                    node_id: "n1".to_string(),
                    symbol: "src/foo.h".to_string(),
                    kind: "FILE".to_string(),
                    subtype: "SOURCE".to_string(),
                    file: "src/foo.h".to_string(),
                    line: 1,
                    column: 0,
                    edge_type: "IMPORTS".to_string(),
                    resolution: "static".to_string(),
                    evidence: vec!["cpp-core:0.1.0".to_string()],
                    depth: 1,
                },
                ImportEntry {
                    node_id: "n2".to_string(),
                    symbol: "src/bar.h".to_string(),
                    kind: "FILE".to_string(),
                    subtype: "SOURCE".to_string(),
                    file: "src/bar.h".to_string(),
                    line: 1,
                    column: 0,
                    edge_type: "IMPORTS".to_string(),
                    resolution: "static".to_string(),
                    evidence: vec![],
                    depth: 1,
                },
                ImportEntry {
                    node_id: "n3".to_string(),
                    symbol: "external/lib.h".to_string(),
                    kind: "FILE".to_string(),
                    subtype: "EXTERNAL".to_string(),
                    file: "external/lib.h".to_string(),
                    line: 1,
                    column: 0,
                    edge_type: "IMPORTS".to_string(),
                    resolution: "unresolved".to_string(),
                    evidence: vec![],
                    depth: 1,
                },
            ],
        }
    }

    fn sample_empty_imports() -> ImportsResponse {
        ImportsResponse {
            file: "src/standalone.cpp".to_string(),
            imports: vec![],
        }
    }

    #[test]
    fn render_imports_shows_header() {
        let resp = sample_imports();
        let output = resp.render_human();
        assert!(output.contains("Imports: src/main.cpp"));
    }

    #[test]
    fn render_imports_shows_count() {
        let resp = sample_imports();
        let output = resp.render_human();
        assert!(output.contains("3 imports"));
    }

    #[test]
    fn render_imports_singular_count() {
        let mut resp = sample_imports();
        resp.imports.truncate(1);
        let output = resp.render_human();
        assert!(output.contains("1 import"));
        assert!(!output.contains("imports")); // no plural
    }

    #[test]
    fn render_imports_shows_entries() {
        let resp = sample_imports();
        let output = resp.render_human();
        assert!(output.contains("src/foo.h"));
        assert!(output.contains("src/bar.h"));
        assert!(output.contains("external/lib.h"));
    }

    #[test]
    fn render_imports_shows_depth() {
        let resp = sample_imports();
        let output = resp.render_human();
        assert!(output.contains("depth=1"));
    }

    #[test]
    fn render_imports_shows_resolution() {
        let resp = sample_imports();
        let output = resp.render_human();
        assert!(output.contains("static"));
        assert!(output.contains("unresolved"));
    }

    #[test]
    fn render_empty_imports() {
        let resp = sample_empty_imports();
        let output = resp.render_human();
        assert!(output.contains("0 imports"));
        // No import lines after count
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 3); // header, blank, count
    }
}
