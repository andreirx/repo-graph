//! C++ language adapter: `ResolvedCallsite` → emitter input.
//!
//! CPP-SB-1: Converts `ResolvedCallsite` facts from the C++ extractor
//! into `StateBoundaryCallsite` inputs for the state-boundary emitter.
//!
//! Key responsibilities:
//!
//! 1. **Synthetic import construction**: C++ has no runtime module system
//!    (only compile-time includes); the extractor synthesizes module names
//!    (`libc:stdio`, `libc:fcntl`, `sqlite3`, `std:fstream`). This adapter
//!    creates matching `ImportView` entries for Form-A matcher compatibility.
//!
//! 2. **Direction symbol passthrough**: The C++ extractor already normalizes
//!    mode/flags/stream types to direction-specific symbols (e.g., `fopen_read`,
//!    `ifstream`, `ofstream_open`). This adapter passes them through to the
//!    binding table lookup.
//!
//! 3. **Memory DB filtering**: Skip SQLite `:memory:` databases (not persistent
//!    state boundaries, and conflicts with stable-key grammar).
//!
//! Design notes:
//! - CppAdapter is separate from CAdapter per C-SB-1/CPP-SB-1 slice separation
//!   (different actors, different change reasons).
//! - Duplicated C bindings for `language = "cpp"` means this adapter doesn't
//!   need fallback logic (Decision D1: no implicit cross-language coupling).

use repo_graph_indexer::types::{CallArgPayload, ResolvedCallsite};
use repo_graph_state_bindings::{
    CalleePath, FsPathOrLogical, ImportView, Language, LogicalName,
};

use crate::adapter::{AdapterContext, LanguageStateAdapter};
use crate::emit::{CallsiteLogicalName, StateBoundaryCallsite};
use crate::evidence::LogicalNameSource;

// ── CppAdapter (CPP-SB-1) ─────────────────────────────────────────

/// C++ language adapter.
///
/// Implements `LanguageStateAdapter` to convert `ResolvedCallsite`
/// facts from the cpp-extractor into `StateBoundaryCallsite` DTOs.
#[derive(Debug, Clone, Default)]
pub struct CppAdapter;

impl CppAdapter {
    /// Create a new C++ adapter.
    pub fn new() -> Self {
        Self
    }
}

impl LanguageStateAdapter for CppAdapter {
    fn language(&self) -> Language {
        Language::Cpp
    }

    fn adapt_callsites(
        &self,
        _ctx: &AdapterContext<'_>,
        callsites: &[ResolvedCallsite],
    ) -> Vec<StateBoundaryCallsite> {
        callsites
            .iter()
            .filter_map(adapt_cpp_callsite)
            .collect()
    }
}

// ── Adapter logic ─────────────────────────────────────────────────

/// Adapt a single C++ `ResolvedCallsite` into a `StateBoundaryCallsite`.
/// Returns `None` if the payload fails validation or represents a
/// non-persistent resource.
fn adapt_cpp_callsite(rc: &ResolvedCallsite) -> Option<StateBoundaryCallsite> {
    // Skip SQLite :memory: databases — in-memory only, not persistent.
    // Also conflicts with stable-key segment grammar (colon).
    if let CallArgPayload::StringLiteral { value } = &rc.arg0_payload {
        if value == ":memory:" {
            return None;
        }
    }

    let (logical_name, logical_name_source) = classify_cpp_payload(&rc.arg0_payload)?;

    // The C++ extractor already normalizes mode/flags/stream types to
    // direction-specific symbols (e.g., fopen_read, ifstream, ofstream_open).
    // Pass through directly.
    let resolved_symbol = rc.resolved_symbol.clone();

    // Synthetic imports_in_file: satisfies Form-A matcher's import-
    // presence check. For C++, imports are synthetic (includes don't
    // create module-specifier bindings at runtime).
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

/// Classify a C++ payload into a logical name and source.
fn classify_cpp_payload(
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

/// Classify a string-literal payload as URL-shaped vs path-shaped.
/// (Same logic as C/TypeScript/Python adapters.)
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
    fn fopen_read_adapted() {
        let rc = ResolvedCallsite {
            enclosing_symbol_node_uid: "sym-1".to_string(),
            resolved_module: "libc:stdio".to_string(),
            resolved_symbol: "fopen_read".to_string(),
            arg0_payload: CallArgPayload::StringLiteral {
                value: "/etc/config".to_string(),
            },
            arg1_payload: Some(CallArgPayload::StringLiteral {
                value: "r".to_string(),
            }),
            source_location: loc(),
        };
        let adapted = adapt_cpp_callsite(&rc).expect("valid");
        assert_eq!(adapted.callee.resolved_module.as_deref(), Some("libc:stdio"));
        assert_eq!(adapted.callee.resolved_symbol, "fopen_read");
    }

    #[test]
    fn ifstream_constructor_adapted() {
        let rc = ResolvedCallsite {
            enclosing_symbol_node_uid: "sym-1".to_string(),
            resolved_module: "std:fstream".to_string(),
            resolved_symbol: "ifstream".to_string(),
            arg0_payload: CallArgPayload::StringLiteral {
                value: "/etc/app.ini".to_string(),
            },
            arg1_payload: None,
            source_location: loc(),
        };
        let adapted = adapt_cpp_callsite(&rc).expect("valid");
        assert_eq!(
            adapted.callee.resolved_module.as_deref(),
            Some("std:fstream")
        );
        assert_eq!(adapted.callee.resolved_symbol, "ifstream");
    }

    #[test]
    fn ofstream_open_adapted() {
        let rc = ResolvedCallsite {
            enclosing_symbol_node_uid: "sym-1".to_string(),
            resolved_module: "std:fstream".to_string(),
            resolved_symbol: "ofstream_open".to_string(),
            arg0_payload: CallArgPayload::StringLiteral {
                value: "/var/log/output.log".to_string(),
            },
            arg1_payload: None,
            source_location: loc(),
        };
        let adapted = adapt_cpp_callsite(&rc).expect("valid");
        assert_eq!(adapted.callee.resolved_symbol, "ofstream_open");
    }

    #[test]
    fn fstream_read_mode_adapted() {
        let rc = ResolvedCallsite {
            enclosing_symbol_node_uid: "sym-1".to_string(),
            resolved_module: "std:fstream".to_string(),
            resolved_symbol: "fstream_read".to_string(),
            arg0_payload: CallArgPayload::StringLiteral {
                value: "/data/file.bin".to_string(),
            },
            arg1_payload: Some(CallArgPayload::StringLiteral {
                value: "std::ios::in".to_string(),
            }),
            source_location: loc(),
        };
        let adapted = adapt_cpp_callsite(&rc).expect("valid");
        assert_eq!(adapted.callee.resolved_symbol, "fstream_read");
    }

    #[test]
    fn sqlite3_open_adapted() {
        let rc = ResolvedCallsite {
            enclosing_symbol_node_uid: "sym-1".to_string(),
            resolved_module: "sqlite3".to_string(),
            resolved_symbol: "sqlite3_open".to_string(),
            arg0_payload: CallArgPayload::StringLiteral {
                value: "app.db".to_string(),
            },
            arg1_payload: None,
            source_location: loc(),
        };
        let adapted = adapt_cpp_callsite(&rc).expect("valid");
        assert_eq!(adapted.callee.resolved_module.as_deref(), Some("sqlite3"));
        assert_eq!(adapted.callee.resolved_symbol, "sqlite3_open");
    }

    #[test]
    fn sqlite3_memory_skipped() {
        let rc = ResolvedCallsite {
            enclosing_symbol_node_uid: "sym-1".to_string(),
            resolved_module: "sqlite3".to_string(),
            resolved_symbol: "sqlite3_open".to_string(),
            arg0_payload: CallArgPayload::StringLiteral {
                value: ":memory:".to_string(),
            },
            arg1_payload: None,
            source_location: loc(),
        };
        let adapted = adapt_cpp_callsite(&rc);
        assert!(adapted.is_none(), ":memory: should be skipped");
    }

    #[test]
    fn synthetic_import_view_created() {
        let rc = ResolvedCallsite {
            enclosing_symbol_node_uid: "sym-1".to_string(),
            resolved_module: "std:fstream".to_string(),
            resolved_symbol: "ifstream".to_string(),
            arg0_payload: CallArgPayload::StringLiteral {
                value: "/x".to_string(),
            },
            arg1_payload: None,
            source_location: loc(),
        };
        let adapted = adapt_cpp_callsite(&rc).expect("valid");
        assert_eq!(adapted.imports_in_file.len(), 1);
        assert_eq!(adapted.imports_in_file[0].module_path, "std:fstream");
        assert_eq!(adapted.imports_in_file[0].imported_symbol, "ifstream");
    }
}
