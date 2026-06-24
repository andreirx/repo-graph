//! NDJSON envelope types for daemon protocol.
//!
//! The daemon uses newline-delimited JSON for request/response communication.
//! Each line is a complete JSON object with an `id` field for correlation.
//!
//! ## Request format
//! ```json
//! {"id":"req-1","method":"orient","params":{"repo":"myrepo","focus":"src/core"}}
//! ```
//!
//! ## Success response format
//! ```json
//! {"id":"req-1","result":{...}}
//! ```
//!
//! ## Error response format
//! ```json
//! {"id":"req-1","error":{"code":"RepoNotFound","message":"repo 'foo' not registered"}}
//! ```
//!
//! ## Progress format (D4+)
//! ```json
//! {"id":"req-1","progress":{"phase":"extracting","current":42,"total":100}}
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A request from the client.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Request {
    /// Unique request identifier for correlation.
    pub id: String,

    /// The method to invoke (e.g., "orient", "callers", "index").
    pub method: String,

    /// Method parameters. Optional, defaults to empty object `{}`.
    #[serde(default = "default_params")]
    pub params: Value,
}

/// Default params value: empty object.
fn default_params() -> Value {
    Value::Object(serde_json::Map::new())
}

/// A successful response to the client.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SuccessResponse {
    /// Request identifier (echoed from request).
    pub id: String,

    /// The result payload.
    pub result: Value,
}

/// An error response to the client.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ErrorResponse {
    /// Request identifier (echoed from request, or "unknown" if unparseable).
    pub id: String,

    /// The error details.
    pub error: ErrorDetail,
}

/// Error details within an error response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorDetail {
    /// Machine-readable error code.
    pub code: String,

    /// Human-readable error message.
    pub message: String,

    /// Optional structured data for errors that need additional context.
    /// Used by AmbiguousSymbol to provide match candidates for CLI rendering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Progress update during long-running operations (D4+).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProgressResponse {
    /// Request identifier.
    pub id: String,

    /// Progress details.
    pub progress: ProgressDetail,
}

/// Progress details.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProgressDetail {
    /// Current phase name.
    pub phase: String,

    /// Current progress count.
    pub current: u64,

    /// Total expected count (0 if unknown).
    pub total: u64,
}

/// Standard error codes used by the daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// The request JSON was malformed.
    ParseError,

    /// Required field missing or invalid type.
    InvalidRequest,

    /// The requested method does not exist.
    UnknownMethod,

    /// The specified repo was not found.
    RepoNotFound,

    /// Symbol query matched multiple targets.
    AmbiguousSymbol,

    /// The specified snapshot was not found.
    SnapshotNotFound,

    /// The repo is not in a queryable state.
    StateUnavailable,

    /// Operation timed out.
    Timeout,

    /// Operation was cancelled.
    Cancelled,

    /// Progress delivery failed — client transport is broken.
    ///
    /// The operation was aborted at a progress checkpoint because the
    /// transport channel failed. This prevents completing operations
    /// whose results cannot be delivered to the client.
    ProgressDeliveryFailed,

    /// The daemon is at its concurrent-connection capacity (over-cap).
    ///
    /// DAEMON-CONCURRENCY-IMPL-1 (D-C = BP-BUSY): the concurrent accept loop
    /// bounds in-flight connection handlers with a counting semaphore
    /// (`RMAP_DAEMON_MAX_CONNS`, default 64 — an arbitrary policy default, not
    /// source-derived). When no permit is free the daemon returns this code on
    /// the existing error envelope and closes the connection, rather than
    /// silently queueing the client into an opaque hang. This is honest
    /// degradation: an explicit, machine-readable "at capacity" the client can
    /// back off / retry on.
    Busy,

    /// Internal error (bug or unexpected failure).
    InternalError,
}

impl ErrorCode {
    /// Get the string code for serialization.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ParseError => "ParseError",
            Self::InvalidRequest => "InvalidRequest",
            Self::UnknownMethod => "UnknownMethod",
            Self::RepoNotFound => "RepoNotFound",
            Self::AmbiguousSymbol => "AmbiguousSymbol",
            Self::SnapshotNotFound => "SnapshotNotFound",
            Self::StateUnavailable => "StateUnavailable",
            Self::Timeout => "Timeout",
            Self::Cancelled => "Cancelled",
            Self::ProgressDeliveryFailed => "ProgressDeliveryFailed",
            Self::Busy => "Busy",
            Self::InternalError => "InternalError",
        }
    }
}

impl ErrorDetail {
    /// Create an error detail from code and message.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code: code.as_str().to_string(),
            message: message.into(),
            data: None,
        }
    }

    /// Create an error detail with additional structured data.
    pub fn with_data(code: ErrorCode, message: impl Into<String>, data: Value) -> Self {
        Self {
            code: code.as_str().to_string(),
            message: message.into(),
            data: Some(data),
        }
    }

    /// Create a parse error.
    pub fn parse_error(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::ParseError, message)
    }

    /// Create an invalid request error.
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidRequest, message)
    }

    /// Create an unknown method error.
    pub fn unknown_method(method: &str) -> Self {
        Self::new(
            ErrorCode::UnknownMethod,
            format!("unknown method: {}", method),
        )
    }

    /// Create an internal error.
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InternalError, message)
    }

    /// Create an ambiguous symbol error with structured match data.
    ///
    /// `matches` should be a JSON array of match objects, each containing:
    /// - qualified_name: String
    /// - kind: String (e.g., "CONSTRUCTOR", "METHOD")
    /// - file: String
    /// - line: u32
    pub fn ambiguous_symbol(query: &str, matches: Value) -> Self {
        Self::with_data(
            ErrorCode::AmbiguousSymbol,
            format!("symbol '{}' is ambiguous", query),
            serde_json::json!({
                "query": query,
                "matches": matches
            }),
        )
    }
}

impl SuccessResponse {
    /// Create a success response.
    pub fn new(id: impl Into<String>, result: Value) -> Self {
        Self {
            id: id.into(),
            result,
        }
    }
}

impl ErrorResponse {
    /// Create an error response.
    pub fn new(id: impl Into<String>, error: ErrorDetail) -> Self {
        Self {
            id: id.into(),
            error,
        }
    }

    /// Create an error response for an unparseable request.
    pub fn for_unparseable(error: ErrorDetail) -> Self {
        Self {
            id: "unknown".to_string(),
            error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_deserializes_with_params() {
        let json = r#"{"id":"req-1","method":"orient","params":{"repo":"test"}}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert_eq!(req.id, "req-1");
        assert_eq!(req.method, "orient");
        assert_eq!(req.params["repo"], "test");
    }

    #[test]
    fn request_deserializes_without_params() {
        let json = r#"{"id":"req-2","method":"ping"}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert_eq!(req.id, "req-2");
        assert_eq!(req.method, "ping");
        // Default is empty object, not null
        assert!(req.params.is_object());
        assert!(req.params.as_object().unwrap().is_empty());
    }

    #[test]
    fn success_response_serializes_correctly() {
        let resp = SuccessResponse::new("req-1", serde_json::json!({"count": 42}));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""id":"req-1""#));
        assert!(json.contains(r#""result":{"count":42}"#));
    }

    #[test]
    fn error_response_serializes_correctly() {
        let resp = ErrorResponse::new("req-1", ErrorDetail::unknown_method("bogus"));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""id":"req-1""#));
        assert!(json.contains(r#""code":"UnknownMethod""#));
        assert!(json.contains(r#""message":"unknown method: bogus""#));
    }

    #[test]
    fn error_response_for_unparseable_uses_unknown_id() {
        let resp = ErrorResponse::for_unparseable(ErrorDetail::parse_error("bad json"));
        assert_eq!(resp.id, "unknown");
    }

    #[test]
    fn error_codes_have_string_representation() {
        assert_eq!(ErrorCode::ParseError.as_str(), "ParseError");
        assert_eq!(ErrorCode::UnknownMethod.as_str(), "UnknownMethod");
        assert_eq!(ErrorCode::RepoNotFound.as_str(), "RepoNotFound");
    }
}
