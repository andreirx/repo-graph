//! Java language adapter: `ResolvedCallsite` → emitter input.
//!
//! SB-7B: Converts `ResolvedCallsite` facts from the Java extractor
//! into `StateBoundaryCallsite` inputs for the state-boundary emitter.
//!
//! First-cut scope: `DriverManager.getConnection(String)` only.
//!
//! Key responsibilities:
//!
//! 1. **JDBC URL passthrough**: The Java extractor emits callsites with
//!    `resolved_module = "java.sql"` and `resolved_symbol = "DriverManager.getConnection"`.
//!    This adapter passes them through for Form-A matcher binding lookup.
//!
//! 2. **Logical name classification**: The JDBC URL in `arg0_payload` is
//!    classified as URL-shaped (contains `://`) and becomes the resource key.

use repo_graph_indexer::types::{CallArgPayload, ResolvedCallsite};
use repo_graph_state_bindings::{CalleePath, FsPathOrLogical, ImportView, Language, LogicalName};

use crate::adapter::{AdapterContext, LanguageStateAdapter};
use crate::emit::{CallsiteLogicalName, StateBoundaryCallsite};
use crate::evidence::LogicalNameSource;

// ── JavaAdapter (SB-7B) ───────────────────────────────────────────

/// Java language adapter.
///
/// Implements `LanguageStateAdapter` to convert `ResolvedCallsite`
/// facts from the java-extractor into `StateBoundaryCallsite` DTOs.
#[derive(Debug, Clone, Default)]
pub struct JavaAdapter;

impl JavaAdapter {
    /// Create a new Java adapter.
    pub fn new() -> Self {
        Self
    }
}

impl LanguageStateAdapter for JavaAdapter {
    fn language(&self) -> Language {
        Language::Java
    }

    fn mechanism(&self) -> &'static str {
        // Java's resource detection is JDBC-only today (DriverManager.getConnection) — NO file I/O
        // detector. Naming it plainly stops "covers Java" from implying file-access coverage.
        "JDBC DriverManager.getConnection calls"
    }

    fn adapt_callsites(
        &self,
        _ctx: &AdapterContext<'_>,
        callsites: &[ResolvedCallsite],
    ) -> Vec<StateBoundaryCallsite> {
        callsites.iter().filter_map(adapt_java_callsite).collect()
    }
}

// ── Adapter logic ─────────────────────────────────────────────────

/// Adapt a single Java `ResolvedCallsite` into a
/// `StateBoundaryCallsite`. Returns `None` if the payload fails
/// validation.
fn adapt_java_callsite(rc: &ResolvedCallsite) -> Option<StateBoundaryCallsite> {
    let (logical_name, logical_name_source) = classify_java_payload(&rc.arg0_payload)?;

    // For Java, resolved_symbol is already "DriverManager.getConnection"
    // Pass through without modification.
    let resolved_symbol = rc.resolved_symbol.clone();

    // Synthetic imports_in_file: satisfies Form-A matcher's import-
    // presence check. Java imports are explicit (not builtin), so this
    // mirrors actual import structure.
    let imports_in_file = vec![ImportView {
        module_path: rc.resolved_module.clone(),
        imported_symbol: resolved_symbol.clone(),
        import_alias: None,
    }];

    Some(StateBoundaryCallsite {
        source_node_uid: rc.enclosing_symbol_node_uid.clone(),
        file_uid: String::new(),
        source_location: rc.source_location,
        imports_in_file,
        callee: CalleePath {
            resolved_module: Some(rc.resolved_module.clone()),
            resolved_symbol,
        },
        logical_name,
        logical_name_source,
    })
}

/// Classify a Java payload into a logical name and source.
///
/// For JDBC URLs like `jdbc:h2:mem:testdb` or `jdbc:postgresql://localhost/db`,
/// the entire URL becomes the logical name. Colons are URL-encoded to satisfy
/// the `LogicalName` invariant (no `:` in stable-key segments).
fn classify_java_payload(
    payload: &CallArgPayload,
) -> Option<(CallsiteLogicalName, LogicalNameSource)> {
    match payload {
        CallArgPayload::StringLiteral { value } => {
            // JDBC URLs are URL-shaped (contain scheme prefix like "jdbc:")
            // but may not contain "://". Encode colons to satisfy LogicalName.
            if value.starts_with("jdbc:") {
                // URL-encode colons: `jdbc:h2:mem:testdb` → `jdbc%3Ah2%3Amem%3Atestdb`
                // This preserves all info while satisfying the stable-key segment grammar.
                let encoded = encode_colons(value);
                let ln = LogicalName::new(encoded).ok()?;
                Some((
                    CallsiteLogicalName::Generic(ln),
                    LogicalNameSource::NormalizedUrl,
                ))
            } else if is_url_shaped(value) {
                // Other URL-shaped strings (non-JDBC)
                let fs_path = FsPathOrLogical::new(value.clone()).ok()?;
                Some((
                    CallsiteLogicalName::Fs(fs_path),
                    LogicalNameSource::NormalizedUrl,
                ))
            } else {
                // Path-shaped strings (unlikely for JDBC, but handle gracefully)
                let fs_path = FsPathOrLogical::new(value.clone()).ok()?;
                Some((
                    CallsiteLogicalName::Fs(fs_path),
                    LogicalNameSource::NormalizedPath,
                ))
            }
        }
        CallArgPayload::EnvKeyRead { key_name } => {
            let ln = LogicalName::new(key_name.clone()).ok()?;
            Some((CallsiteLogicalName::Generic(ln), LogicalNameSource::EnvKey))
        }
    }
}

/// URL-encode colons in a string. Used to make JDBC URLs safe for
/// stable-key segments which forbid `:` characters.
fn encode_colons(s: &str) -> String {
    s.replace(':', "%3A")
}

/// Classify a string-literal payload as URL-shaped vs path-shaped.
/// (Same logic as TypeScript/Python adapters.)
fn is_url_shaped(s: &str) -> bool {
    let Some(scheme_end) = s.find("://") else {
        return false;
    };
    if scheme_end == 0 {
        return false;
    }
    let scheme = &s[..scheme_end];
    let mut chars = scheme.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_classification::types::SourceLocation;

    fn loc() -> SourceLocation {
        SourceLocation {
            line_start: 1,
            col_start: 0,
            line_end: 1,
            col_end: 50,
        }
    }

    #[test]
    fn jdbc_h2_url_becomes_logical_name() {
        let rc = ResolvedCallsite {
            enclosing_symbol_node_uid: "sym-1".to_string(),
            resolved_module: "java.sql".to_string(),
            resolved_symbol: "DriverManager.getConnection".to_string(),
            arg0_payload: CallArgPayload::StringLiteral {
                value: "jdbc:h2:mem:testdb".to_string(),
            },
            arg1_payload: None,
            source_location: loc(),
        };
        let adapted = adapt_java_callsite(&rc).expect("valid");
        assert_eq!(adapted.callee.resolved_module.as_deref(), Some("java.sql"));
        assert_eq!(
            adapted.callee.resolved_symbol,
            "DriverManager.getConnection"
        );
        assert!(matches!(
            adapted.logical_name,
            CallsiteLogicalName::Generic(_)
        ));
        // Colons are URL-encoded to satisfy LogicalName invariant.
        assert_eq!(adapted.logical_name.as_str(), "jdbc%3Ah2%3Amem%3Atestdb");
        assert_eq!(
            adapted.logical_name_source,
            LogicalNameSource::NormalizedUrl
        );
    }

    #[test]
    fn jdbc_postgresql_url_becomes_logical_name() {
        let rc = ResolvedCallsite {
            enclosing_symbol_node_uid: "sym-1".to_string(),
            resolved_module: "java.sql".to_string(),
            resolved_symbol: "DriverManager.getConnection".to_string(),
            arg0_payload: CallArgPayload::StringLiteral {
                value: "jdbc:postgresql://localhost:5432/mydb".to_string(),
            },
            arg1_payload: None,
            source_location: loc(),
        };
        let adapted = adapt_java_callsite(&rc).expect("valid");
        assert!(matches!(
            adapted.logical_name,
            CallsiteLogicalName::Generic(_)
        ));
        // Colons are URL-encoded.
        assert_eq!(
            adapted.logical_name.as_str(),
            "jdbc%3Apostgresql%3A//localhost%3A5432/mydb"
        );
    }

    #[test]
    fn synthetic_import_view_created() {
        let rc = ResolvedCallsite {
            enclosing_symbol_node_uid: "sym-1".to_string(),
            resolved_module: "java.sql".to_string(),
            resolved_symbol: "DriverManager.getConnection".to_string(),
            arg0_payload: CallArgPayload::StringLiteral {
                value: "jdbc:h2:mem:testdb".to_string(),
            },
            arg1_payload: None,
            source_location: loc(),
        };
        let adapted = adapt_java_callsite(&rc).expect("valid");
        assert_eq!(adapted.imports_in_file.len(), 1);
        assert_eq!(adapted.imports_in_file[0].module_path, "java.sql");
        assert_eq!(
            adapted.imports_in_file[0].imported_symbol,
            "DriverManager.getConnection"
        );
    }

    #[test]
    fn env_key_payload_becomes_generic_variant() {
        let rc = ResolvedCallsite {
            enclosing_symbol_node_uid: "sym-1".to_string(),
            resolved_module: "java.sql".to_string(),
            resolved_symbol: "DriverManager.getConnection".to_string(),
            arg0_payload: CallArgPayload::EnvKeyRead {
                key_name: "DATABASE_URL".to_string(),
            },
            arg1_payload: None,
            source_location: loc(),
        };
        let adapted = adapt_java_callsite(&rc).expect("valid");
        assert!(matches!(
            adapted.logical_name,
            CallsiteLogicalName::Generic(_)
        ));
        assert_eq!(adapted.logical_name_source, LogicalNameSource::EnvKey);
    }
}
