//! Request dispatch abstraction.
//!
//! The dispatcher trait defines how requests are handled. The transport
//! layer calls the dispatcher for each parsed request.
//!
//! D2 provides a mock dispatcher for testing. D3 will provide a real
//! dispatcher that invokes application services.

use serde_json::Value;

use crate::envelope::{ErrorDetail, ErrorResponse, Request, SuccessResponse};

/// Result of dispatching a request.
#[derive(Debug)]
pub enum DispatchResult {
    /// Request succeeded.
    Success(SuccessResponse),

    /// Request failed with an error.
    Error(ErrorResponse),
}

impl DispatchResult {
    /// Create a success result.
    pub fn success(id: impl Into<String>, result: Value) -> Self {
        Self::Success(SuccessResponse::new(id, result))
    }

    /// Create an error result.
    pub fn error(id: impl Into<String>, detail: ErrorDetail) -> Self {
        Self::Error(ErrorResponse::new(id, detail))
    }

    /// Create an unknown method error.
    pub fn unknown_method(id: impl Into<String>, method: &str) -> Self {
        Self::error(id, ErrorDetail::unknown_method(method))
    }
}

/// Trait for request dispatchers.
///
/// Implement this to handle requests. The transport layer calls `dispatch`
/// for each valid request parsed from the input stream.
pub trait Dispatcher {
    /// Dispatch a request and return the result.
    ///
    /// The dispatcher should:
    /// 1. Validate the method name
    /// 2. Validate/parse the params
    /// 3. Execute the method
    /// 4. Return success or error
    ///
    /// The dispatcher should NOT:
    /// - Parse JSON (already done)
    /// - Write to stdout (transport handles this)
    /// - Handle transport-level errors
    fn dispatch(&self, request: &Request) -> DispatchResult;
}

/// A mock dispatcher for testing.
///
/// Responds to:
/// - "ping" -> {"pong": true}
/// - "echo" -> echoes back params
/// - anything else -> UnknownMethod error
#[derive(Debug, Default)]
pub struct MockDispatcher;

impl MockDispatcher {
    /// Create a new mock dispatcher.
    pub fn new() -> Self {
        Self
    }
}

impl Dispatcher for MockDispatcher {
    fn dispatch(&self, request: &Request) -> DispatchResult {
        match request.method.as_str() {
            "ping" => DispatchResult::success(&request.id, serde_json::json!({"pong": true})),

            "echo" => DispatchResult::success(&request.id, request.params.clone()),

            "error" => {
                // For testing error responses
                let code = request.params.get("code")
                    .and_then(|v| v.as_str())
                    .unwrap_or("TestError");
                let message = request.params.get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("test error");
                DispatchResult::error(
                    &request.id,
                    ErrorDetail {
                        code: code.to_string(),
                        message: message.to_string(),
                    },
                )
            }

            _ => DispatchResult::unknown_method(&request.id, &request.method),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_ping_returns_pong() {
        let dispatcher = MockDispatcher::new();
        let request = Request {
            id: "1".to_string(),
            method: "ping".to_string(),
            params: Value::Null,
        };
        let result = dispatcher.dispatch(&request);
        match result {
            DispatchResult::Success(resp) => {
                assert_eq!(resp.id, "1");
                assert_eq!(resp.result["pong"], true);
            }
            _ => panic!("expected success"),
        }
    }

    #[test]
    fn mock_echo_returns_params() {
        let dispatcher = MockDispatcher::new();
        let request = Request {
            id: "2".to_string(),
            method: "echo".to_string(),
            params: serde_json::json!({"hello": "world"}),
        };
        let result = dispatcher.dispatch(&request);
        match result {
            DispatchResult::Success(resp) => {
                assert_eq!(resp.id, "2");
                assert_eq!(resp.result["hello"], "world");
            }
            _ => panic!("expected success"),
        }
    }

    #[test]
    fn mock_unknown_method_returns_error() {
        let dispatcher = MockDispatcher::new();
        let request = Request {
            id: "3".to_string(),
            method: "bogus".to_string(),
            params: Value::Null,
        };
        let result = dispatcher.dispatch(&request);
        match result {
            DispatchResult::Error(resp) => {
                assert_eq!(resp.id, "3");
                assert_eq!(resp.error.code, "UnknownMethod");
                assert!(resp.error.message.contains("bogus"));
            }
            _ => panic!("expected error"),
        }
    }

    #[test]
    fn mock_error_returns_custom_error() {
        let dispatcher = MockDispatcher::new();
        let request = Request {
            id: "4".to_string(),
            method: "error".to_string(),
            params: serde_json::json!({"code": "CustomError", "message": "custom message"}),
        };
        let result = dispatcher.dispatch(&request);
        match result {
            DispatchResult::Error(resp) => {
                assert_eq!(resp.id, "4");
                assert_eq!(resp.error.code, "CustomError");
                assert_eq!(resp.error.message, "custom message");
            }
            _ => panic!("expected error"),
        }
    }
}
