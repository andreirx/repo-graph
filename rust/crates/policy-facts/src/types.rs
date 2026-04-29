//! Policy-facts DTOs.
//!
//! PF-1: STATUS_MAPPING
//! PF-2: BEHAVIORAL_MARKER (RETRY_LOOP, RESUME_OFFSET)

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

// =============================================================================
// PF-2: BEHAVIORAL_MARKER
// =============================================================================

/// A behavioral marker indicating policy-relevant control flow.
///
/// Extracted from C code patterns that indicate retry behavior or
/// resume-from-offset handling. These are local behavior semantics
/// visible from AST structure alone.
///
/// See `docs/slices/pf-2-behavioral-marker.md` for extraction rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BehavioralMarker {
    /// Stable key of the enclosing function.
    /// Format: `{repo}:{file}#{name}:SYMBOL:FUNCTION`
    pub symbol_key: String,

    /// Function name (for display).
    pub function_name: String,

    /// Source file path (repo-relative).
    pub file_path: String,

    /// Line number where the marker pattern starts (1-indexed).
    pub line_start: u32,

    /// Line number where the marker pattern ends (1-indexed).
    pub line_end: u32,

    /// Kind of behavioral marker.
    pub kind: MarkerKind,

    /// Kind-specific evidence.
    pub evidence: MarkerEvidence,
}

/// Classification of behavioral marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MarkerKind {
    /// Loop with sleep/delay that retries an operation.
    RetryLoop,

    /// Resume-from-offset pattern (curl CURLOPT_RESUME_FROM*).
    ResumeOffset,
}

/// Kind-specific evidence for a behavioral marker.
///
/// Tagged union: serializes with a `type` discriminator field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MarkerEvidence {
    /// Evidence for a RETRY_LOOP marker.
    #[serde(rename = "retry_loop")]
    RetryLoop {
        /// Loop construct kind: "while", "for", "do_while".
        loop_kind: String,

        /// Sleep/delay call detected in loop body.
        /// Function name (e.g., "sleep", "usleep", "nanosleep").
        #[serde(skip_serializing_if = "Option::is_none")]
        sleep_call: Option<String>,

        /// Retry delay in milliseconds if statically determinable.
        /// None if dynamic or not determinable.
        #[serde(skip_serializing_if = "Option::is_none")]
        delay_ms: Option<u64>,

        /// Max attempts if statically determinable from loop condition.
        /// None if unbounded or not determinable.
        #[serde(skip_serializing_if = "Option::is_none")]
        max_attempts: Option<u32>,

        /// Break/return condition expression as source text.
        /// None if not a simple success-check pattern.
        #[serde(skip_serializing_if = "Option::is_none")]
        break_condition: Option<String>,
    },

    /// Evidence for a RESUME_OFFSET marker.
    #[serde(rename = "resume_offset")]
    ResumeOffset {
        /// The API call that sets the resume offset.
        /// e.g., "curl_easy_setopt"
        api_call: String,

        /// The option/flag name if applicable.
        /// e.g., "CURLOPT_RESUME_FROM_LARGE"
        #[serde(skip_serializing_if = "Option::is_none")]
        option_name: Option<String>,

        /// Variable holding the offset if identifiable.
        #[serde(skip_serializing_if = "Option::is_none")]
        offset_source: Option<String>,
    },
}

impl std::fmt::Display for MarkerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MarkerKind::RetryLoop => write!(f, "RETRY_LOOP"),
            MarkerKind::ResumeOffset => write!(f, "RESUME_OFFSET"),
        }
    }
}

impl std::str::FromStr for MarkerKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "RETRY_LOOP" => Ok(MarkerKind::RetryLoop),
            "RESUME_OFFSET" => Ok(MarkerKind::ResumeOffset),
            _ => Err(format!("unknown marker kind: {}", s)),
        }
    }
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

    // =========================================================================
    // PF-2: BEHAVIORAL_MARKER tests
    // =========================================================================

    #[test]
    fn behavioral_marker_retry_loop_serializes_correctly() {
        let marker = BehavioralMarker {
            symbol_key: "swupdate:corelib/channel_curl.c#channel_get_file:SYMBOL:FUNCTION"
                .to_string(),
            function_name: "channel_get_file".to_string(),
            file_path: "corelib/channel_curl.c".to_string(),
            line_start: 1359,
            line_end: 1364,
            kind: MarkerKind::RetryLoop,
            evidence: MarkerEvidence::RetryLoop {
                loop_kind: "for".to_string(),
                sleep_call: Some("sleep".to_string()),
                delay_ms: Some(1000),
                max_attempts: Some(4),
                break_condition: Some("file_handle > 0".to_string()),
            },
        };

        let json = serde_json::to_string_pretty(&marker).unwrap();
        assert!(json.contains("\"kind\": \"RETRY_LOOP\""));
        assert!(json.contains("\"type\": \"retry_loop\""));
        assert!(json.contains("\"loop_kind\": \"for\""));
        assert!(json.contains("\"sleep_call\": \"sleep\""));
        assert!(json.contains("\"delay_ms\": 1000"));
        assert!(json.contains("\"max_attempts\": 4"));

        // Round-trip
        let parsed: BehavioralMarker = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, marker);
    }

    #[test]
    fn behavioral_marker_resume_offset_serializes_correctly() {
        let marker = BehavioralMarker {
            symbol_key: "swupdate:corelib/channel_curl.c#channel_get_file:SYMBOL:FUNCTION"
                .to_string(),
            function_name: "channel_get_file".to_string(),
            file_path: "corelib/channel_curl.c".to_string(),
            line_start: 1437,
            line_end: 1439,
            kind: MarkerKind::ResumeOffset,
            evidence: MarkerEvidence::ResumeOffset {
                api_call: "curl_easy_setopt".to_string(),
                option_name: Some("CURLOPT_RESUME_FROM_LARGE".to_string()),
                offset_source: Some("total_bytes_downloaded".to_string()),
            },
        };

        let json = serde_json::to_string_pretty(&marker).unwrap();
        assert!(json.contains("\"kind\": \"RESUME_OFFSET\""));
        assert!(json.contains("\"type\": \"resume_offset\""));
        assert!(json.contains("\"api_call\": \"curl_easy_setopt\""));
        assert!(json.contains("CURLOPT_RESUME_FROM_LARGE"));

        // Round-trip
        let parsed: BehavioralMarker = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, marker);
    }

    #[test]
    fn behavioral_marker_omits_none_fields() {
        let marker = BehavioralMarker {
            symbol_key: "test:file.c#func:SYMBOL:FUNCTION".to_string(),
            function_name: "func".to_string(),
            file_path: "file.c".to_string(),
            line_start: 10,
            line_end: 20,
            kind: MarkerKind::RetryLoop,
            evidence: MarkerEvidence::RetryLoop {
                loop_kind: "do_while".to_string(),
                sleep_call: Some("sleep".to_string()),
                delay_ms: None, // dynamic
                max_attempts: None, // unbounded
                break_condition: None,
            },
        };

        let json = serde_json::to_string(&marker).unwrap();
        assert!(!json.contains("delay_ms"));
        assert!(!json.contains("max_attempts"));
        assert!(!json.contains("break_condition"));
        // But sleep_call should be present
        assert!(json.contains("sleep_call"));
    }

    #[test]
    fn marker_kind_display_and_parse() {
        assert_eq!(format!("{}", MarkerKind::RetryLoop), "RETRY_LOOP");
        assert_eq!(format!("{}", MarkerKind::ResumeOffset), "RESUME_OFFSET");

        assert_eq!(
            "RETRY_LOOP".parse::<MarkerKind>().unwrap(),
            MarkerKind::RetryLoop
        );
        assert_eq!(
            "retry_loop".parse::<MarkerKind>().unwrap(),
            MarkerKind::RetryLoop
        );
        assert_eq!(
            "RESUME_OFFSET".parse::<MarkerKind>().unwrap(),
            MarkerKind::ResumeOffset
        );
        assert_eq!(
            "Resume_Offset".parse::<MarkerKind>().unwrap(),
            MarkerKind::ResumeOffset
        );

        assert!("UNKNOWN".parse::<MarkerKind>().is_err());
    }
}
