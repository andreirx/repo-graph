//! Policy-facts DTOs.
//!
//! PF-1 scope: STATUS_MAPPING only.

use serde::{Deserialize, Serialize};

/// A function that translates one status/error code type to another.
///
/// Extracted from C switch statements that:
/// - Switch on any qualifying parameter (including pointer dereference)
/// - Return a status/result-like type
/// - Map input codes to output codes via case arms
///
/// See `docs/slices/pf-1-status-mapping.md` for extraction rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusMapping {
    /// Stable key of the function performing the translation.
    /// Format: `{repo}:{file}#{name}:SYMBOL:FUNCTION`
    pub symbol_key: String,

    /// Function name (for display).
    pub function_name: String,

    /// Source file path (repo-relative).
    pub file_path: String,

    /// Line number where the function starts (1-indexed).
    pub line_start: u32,

    /// Line number where the function ends (1-indexed).
    pub line_end: u32,

    /// Name of the input type (first parameter type).
    /// May be enum-like (`channel_op_res_t`) or numeric (`long`).
    pub source_type: String,

    /// Name of the output type (return type).
    /// Should be status/result-like (`server_op_res_t`).
    pub target_type: String,

    /// Individual case mappings from input codes to output codes.
    pub mappings: Vec<CaseMapping>,

    /// Default/fallback output when no case matches.
    /// None if no default arm or if switch is exhaustive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_output: Option<String>,
}

/// A single case mapping in a status translation function.
///
/// Represents one or more input codes that map to a single output code.
/// Multiple inputs occur when case arms fall through:
/// ```c
/// case CHANNEL_ENONET:
/// case CHANNEL_EAGAIN:
///     return SERVER_EAGAIN;
/// ```
/// becomes `CaseMapping { inputs: ["CHANNEL_ENONET", "CHANNEL_EAGAIN"], output: "SERVER_EAGAIN" }`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseMapping {
    /// Input enum variant(s) or numeric literal(s) for this case arm.
    /// Multiple values when case arms fall through without a return.
    pub inputs: Vec<String>,

    /// Output value returned by this arm.
    /// May be an identifier or an expression (preserved as source text).
    pub output: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping_serializes_correctly() {
        let mapping = StatusMapping {
            symbol_key: "swupdate:corelib/server_utils.c#map_channel_retcode:SYMBOL:FUNCTION"
                .to_string(),
            function_name: "map_channel_retcode".to_string(),
            file_path: "corelib/server_utils.c".to_string(),
            line_start: 72,
            line_end: 98,
            source_type: "channel_op_res_t".to_string(),
            target_type: "server_op_res_t".to_string(),
            mappings: vec![
                CaseMapping {
                    inputs: vec![
                        "CHANNEL_ENONET".to_string(),
                        "CHANNEL_EAGAIN".to_string(),
                    ],
                    output: "SERVER_EAGAIN".to_string(),
                },
                CaseMapping {
                    inputs: vec!["CHANNEL_OK".to_string()],
                    output: "SERVER_OK".to_string(),
                },
            ],
            default_output: Some("SERVER_EERR".to_string()),
        };

        let json = serde_json::to_string_pretty(&mapping).unwrap();
        assert!(json.contains("map_channel_retcode"));
        assert!(json.contains("CHANNEL_ENONET"));
        assert!(json.contains("SERVER_EAGAIN"));
        assert!(json.contains("SERVER_EERR"));

        // Round-trip
        let parsed: StatusMapping = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, mapping);
    }

    #[test]
    fn case_mapping_with_fallthrough() {
        let case = CaseMapping {
            inputs: vec![
                "CHANNEL_ENONET".to_string(),
                "CHANNEL_EAGAIN".to_string(),
                "CHANNEL_ESSLCERT".to_string(),
            ],
            output: "SERVER_EAGAIN".to_string(),
        };

        assert_eq!(case.inputs.len(), 3);
        assert_eq!(case.output, "SERVER_EAGAIN");
    }

    #[test]
    fn status_mapping_omits_none_default() {
        let mapping = StatusMapping {
            symbol_key: "test:file.c#func:SYMBOL:FUNCTION".to_string(),
            function_name: "func".to_string(),
            file_path: "file.c".to_string(),
            line_start: 1,
            line_end: 10,
            source_type: "enum_a".to_string(),
            target_type: "enum_b".to_string(),
            mappings: vec![],
            default_output: None,
        };

        let json = serde_json::to_string(&mapping).unwrap();
        assert!(!json.contains("default_output"));
    }
}
