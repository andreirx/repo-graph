//! Python language adapter: `ResolvedCallsite` → emitter input.
//!
//! SB-7C: Converts `ResolvedCallsite` facts from the Python extractor
//! into `StateBoundaryCallsite` inputs for the state-boundary emitter.
//!
//! Key responsibilities:
//!
//! 1. **Builtin normalization**: The Python extractor synthesizes
//!    `resolved_module = "builtins"` for `open()`. This adapter accepts
//!    that synthetic module and creates a matching `ImportView` for
//!    Form-A matcher compatibility. This is NOT evidence of an actual
//!    import; it is a deliberate builtin normalization rule.
//!
//! 2. **Mode-to-direction normalization**: For `builtins:open`, the
//!    adapter interprets `arg1_payload` (the mode string) to determine
//!    the direction and rewrites `resolved_symbol` to a direction-
//!    specific binding key:
//!    - `'r'`, `'rb'` → `open_read`
//!    - `'w'`, `'wb'`, `'a'`, `'ab'`, `'x'` → `open_write`
//!    - `'r+'`, `'w+'` → `open_read_write`
//!    - Missing/unknown mode → `open_read` (Python default)
//!
//! 3. **DB connection passthrough**: `sqlite3:connect` and
//!    `psycopg2:connect` pass through with `resolved_symbol` unchanged.

use repo_graph_indexer::types::{CallArgPayload, ResolvedCallsite};
use repo_graph_state_bindings::{
    CalleePath, FsPathOrLogical, ImportView, Language, LogicalName,
};

use crate::adapter::{AdapterContext, LanguageStateAdapter};
use crate::emit::{CallsiteLogicalName, StateBoundaryCallsite};
use crate::evidence::LogicalNameSource;

// ── PythonAdapter (SB-7C) ─────────────────────────────────────────

/// Python language adapter.
///
/// Implements `LanguageStateAdapter` to convert `ResolvedCallsite`
/// facts from the python-extractor into `StateBoundaryCallsite` DTOs.
#[derive(Debug, Clone, Default)]
pub struct PythonAdapter;

impl PythonAdapter {
    /// Create a new Python adapter.
    pub fn new() -> Self {
        Self
    }
}

impl LanguageStateAdapter for PythonAdapter {
    fn language(&self) -> Language {
        Language::Python
    }

    fn adapt_callsites(
        &self,
        _ctx: &AdapterContext<'_>,
        callsites: &[ResolvedCallsite],
    ) -> Vec<StateBoundaryCallsite> {
        callsites
            .iter()
            .filter_map(adapt_python_callsite)
            .collect()
    }
}

// ── Adapter logic ─────────────────────────────────────────────────

/// Adapt a single Python `ResolvedCallsite` into a
/// `StateBoundaryCallsite`. Returns `None` if the payload fails
/// validation or represents a non-persistent resource.
fn adapt_python_callsite(rc: &ResolvedCallsite) -> Option<StateBoundaryCallsite> {
    // Skip SQLite :memory: databases — these are in-memory only, not
    // persistent state boundaries. The `:` character also conflicts
    // with the Db-kind stable-key segment grammar.
    if let CallArgPayload::StringLiteral { value } = &rc.arg0_payload {
        if value == ":memory:" {
            return None;
        }
    }

    let (logical_name, logical_name_source) = classify_python_payload(&rc.arg0_payload)?;

    // Determine the binding symbol path. For builtins:open, normalize
    // mode to direction-specific symbol. For others, pass through.
    let resolved_symbol = if rc.resolved_module == "builtins" && rc.resolved_symbol == "open" {
        normalize_open_mode_to_symbol(&rc.arg1_payload)
    } else {
        rc.resolved_symbol.clone()
    };

    // Synthetic imports_in_file: satisfies Form-A matcher's import-
    // presence check. For `builtins`, this is a synthetic entry that
    // never appeared in source (documented as builtin normalization).
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

/// Classify a Python payload into a logical name and source.
fn classify_python_payload(
    payload: &CallArgPayload,
) -> Option<(CallsiteLogicalName, LogicalNameSource)> {
    match payload {
        CallArgPayload::StringLiteral { value } => {
            let fs_path = FsPathOrLogical::new(value.clone()).ok()?;
            let source = if is_url_shaped(value) {
                LogicalNameSource::NormalizedUrl
            } else {
                LogicalNameSource::NormalizedPath
            };
            Some((CallsiteLogicalName::Fs(fs_path), source))
        }
        CallArgPayload::EnvKeyRead { key_name } => {
            let ln = LogicalName::new(key_name.clone()).ok()?;
            Some((CallsiteLogicalName::Generic(ln), LogicalNameSource::EnvKey))
        }
    }
}

/// Normalize Python `open()` mode argument to direction-specific
/// binding symbol.
///
/// Mode interpretation:
/// - `'r'`, `'rb'` → `open_read`
/// - `'w'`, `'wb'`, `'a'`, `'ab'`, `'x'`, `'xb'` → `open_write`
/// - `'r+'`, `'rb+'`, `'r+b'`, `'w+'`, `'wb+'`, `'w+b'`, `'a+'`, `'ab+'`, `'a+b'` → `open_read_write`
/// - Missing or unknown → `open_read` (Python default is 'r')
fn normalize_open_mode_to_symbol(arg1: &Option<CallArgPayload>) -> String {
    let mode = match arg1 {
        Some(CallArgPayload::StringLiteral { value }) => value.as_str(),
        _ => return "open_read".to_string(), // Default mode is 'r'
    };

    // Normalize mode: strip 'b' (binary) and check for '+' (read-write).
    let mode_normalized: String = mode.chars().filter(|c| *c != 'b').collect();

    if mode_normalized.contains('+') {
        // Any mode with '+' is read-write.
        "open_read_write".to_string()
    } else if mode_normalized.starts_with('r') || mode_normalized.is_empty() {
        "open_read".to_string()
    } else if mode_normalized.starts_with('w')
        || mode_normalized.starts_with('a')
        || mode_normalized.starts_with('x')
    {
        "open_write".to_string()
    } else {
        // Unknown mode, default to read.
        "open_read".to_string()
    }
}

/// Classify a string-literal payload as URL-shaped vs path-shaped.
/// (Same logic as TypeScript adapter.)
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
            col_end: 10,
        }
    }

    #[test]
    fn open_read_mode_normalizes_to_open_read() {
        let rc = ResolvedCallsite {
            enclosing_symbol_node_uid: "sym-1".to_string(),
            resolved_module: "builtins".to_string(),
            resolved_symbol: "open".to_string(),
            arg0_payload: CallArgPayload::StringLiteral {
                value: "/etc/config".to_string(),
            },
            arg1_payload: Some(CallArgPayload::StringLiteral {
                value: "r".to_string(),
            }),
            source_location: loc(),
        };
        let adapted = adapt_python_callsite(&rc).expect("valid");
        assert_eq!(adapted.callee.resolved_symbol, "open_read");
    }

    #[test]
    fn open_write_mode_normalizes_to_open_write() {
        let rc = ResolvedCallsite {
            enclosing_symbol_node_uid: "sym-1".to_string(),
            resolved_module: "builtins".to_string(),
            resolved_symbol: "open".to_string(),
            arg0_payload: CallArgPayload::StringLiteral {
                value: "/var/log/app.log".to_string(),
            },
            arg1_payload: Some(CallArgPayload::StringLiteral {
                value: "w".to_string(),
            }),
            source_location: loc(),
        };
        let adapted = adapt_python_callsite(&rc).expect("valid");
        assert_eq!(adapted.callee.resolved_symbol, "open_write");
    }

    #[test]
    fn open_read_write_mode_normalizes_to_open_read_write() {
        let rc = ResolvedCallsite {
            enclosing_symbol_node_uid: "sym-1".to_string(),
            resolved_module: "builtins".to_string(),
            resolved_symbol: "open".to_string(),
            arg0_payload: CallArgPayload::StringLiteral {
                value: "/data/file.json".to_string(),
            },
            arg1_payload: Some(CallArgPayload::StringLiteral {
                value: "r+".to_string(),
            }),
            source_location: loc(),
        };
        let adapted = adapt_python_callsite(&rc).expect("valid");
        assert_eq!(adapted.callee.resolved_symbol, "open_read_write");
    }

    #[test]
    fn open_binary_modes_handled() {
        // 'rb' → open_read
        let mode_rb = normalize_open_mode_to_symbol(&Some(CallArgPayload::StringLiteral {
            value: "rb".to_string(),
        }));
        assert_eq!(mode_rb, "open_read");

        // 'wb' → open_write
        let mode_wb = normalize_open_mode_to_symbol(&Some(CallArgPayload::StringLiteral {
            value: "wb".to_string(),
        }));
        assert_eq!(mode_wb, "open_write");

        // 'r+b' → open_read_write
        let mode_rpb = normalize_open_mode_to_symbol(&Some(CallArgPayload::StringLiteral {
            value: "r+b".to_string(),
        }));
        assert_eq!(mode_rpb, "open_read_write");
    }

    #[test]
    fn open_append_mode_normalizes_to_open_write() {
        let mode_a = normalize_open_mode_to_symbol(&Some(CallArgPayload::StringLiteral {
            value: "a".to_string(),
        }));
        assert_eq!(mode_a, "open_write");
    }

    #[test]
    fn open_exclusive_mode_normalizes_to_open_write() {
        let mode_x = normalize_open_mode_to_symbol(&Some(CallArgPayload::StringLiteral {
            value: "x".to_string(),
        }));
        assert_eq!(mode_x, "open_write");
    }

    #[test]
    fn open_missing_mode_defaults_to_open_read() {
        let mode_none = normalize_open_mode_to_symbol(&None);
        assert_eq!(mode_none, "open_read");
    }

    #[test]
    fn sqlite3_connect_passes_through() {
        let rc = ResolvedCallsite {
            enclosing_symbol_node_uid: "sym-1".to_string(),
            resolved_module: "sqlite3".to_string(),
            resolved_symbol: "connect".to_string(),
            arg0_payload: CallArgPayload::StringLiteral {
                value: "app.db".to_string(),
            },
            arg1_payload: None,
            source_location: loc(),
        };
        let adapted = adapt_python_callsite(&rc).expect("valid");
        assert_eq!(adapted.callee.resolved_module.as_deref(), Some("sqlite3"));
        assert_eq!(adapted.callee.resolved_symbol, "connect");
    }

    #[test]
    fn sqlite3_memory_db_skipped() {
        // :memory: is an in-memory database, not a persistent state boundary.
        // It should be filtered out to avoid stable-key grammar conflicts.
        let rc = ResolvedCallsite {
            enclosing_symbol_node_uid: "sym-1".to_string(),
            resolved_module: "sqlite3".to_string(),
            resolved_symbol: "connect".to_string(),
            arg0_payload: CallArgPayload::StringLiteral {
                value: ":memory:".to_string(),
            },
            arg1_payload: None,
            source_location: loc(),
        };
        let adapted = adapt_python_callsite(&rc);
        assert!(adapted.is_none(), ":memory: should be skipped");
    }

    #[test]
    fn env_key_payload_becomes_generic_variant() {
        let rc = ResolvedCallsite {
            enclosing_symbol_node_uid: "sym-1".to_string(),
            resolved_module: "builtins".to_string(),
            resolved_symbol: "open".to_string(),
            arg0_payload: CallArgPayload::EnvKeyRead {
                key_name: "CONFIG_PATH".to_string(),
            },
            arg1_payload: None,
            source_location: loc(),
        };
        let adapted = adapt_python_callsite(&rc).expect("valid");
        assert!(matches!(
            adapted.logical_name,
            CallsiteLogicalName::Generic(_)
        ));
        assert_eq!(adapted.logical_name_source, LogicalNameSource::EnvKey);
    }

    #[test]
    fn synthetic_import_view_created() {
        let rc = ResolvedCallsite {
            enclosing_symbol_node_uid: "sym-1".to_string(),
            resolved_module: "builtins".to_string(),
            resolved_symbol: "open".to_string(),
            arg0_payload: CallArgPayload::StringLiteral {
                value: "/x".to_string(),
            },
            arg1_payload: Some(CallArgPayload::StringLiteral {
                value: "r".to_string(),
            }),
            source_location: loc(),
        };
        let adapted = adapt_python_callsite(&rc).expect("valid");
        assert_eq!(adapted.imports_in_file.len(), 1);
        assert_eq!(adapted.imports_in_file[0].module_path, "builtins");
        // Symbol should be normalized to open_read
        assert_eq!(adapted.imports_in_file[0].imported_symbol, "open_read");
    }
}
