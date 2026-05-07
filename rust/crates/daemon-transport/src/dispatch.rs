//! Request dispatch abstraction.
//!
//! The dispatcher trait defines how requests are handled. The transport
//! layer calls the dispatcher for each parsed request.
//!
//! D2 provides a mock dispatcher for testing. D3 will provide a real
//! dispatcher that invokes application services.
//!
//! ## Progress Emission (D5b)
//!
//! Long-running operations can emit progress events via the `ProgressEmitter`
//! trait. The transport layer provides a request-scoped emitter that writes
//! progress events to the output stream.
//!
//! Event ordering contract:
//! - Progress events preserve source order
//! - Final response is always last
//! - No progress events after final response

use serde_json::Value;

use crate::envelope::{ErrorCode, ErrorDetail, ErrorResponse, ProgressDetail, Request, SuccessResponse};

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

/// Error from progress emission.
///
/// Indicates the transport channel has failed and the operation should abort.
#[derive(Debug)]
pub struct EmitError {
    /// Human-readable description of what failed.
    pub message: String,
}

impl EmitError {
    /// Create a new emit error.
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }

    /// Create an emit error from an I/O error.
    pub fn from_io(e: std::io::Error) -> Self {
        Self { message: format!("transport write failed: {}", e) }
    }

    /// Create an emit error from a serialization error.
    pub fn from_serialize(e: serde_json::Error) -> Self {
        Self { message: format!("progress serialization failed: {}", e) }
    }
}

impl std::fmt::Display for EmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for EmitError {}

/// Request-scoped progress emitter.
///
/// The transport layer provides an implementation that writes progress
/// events to the output stream. Dispatchers use this to emit progress
/// during long-running operations.
///
/// The emitter is bound to a specific request ID. All emitted progress
/// events are correlated to that request.
///
/// ## Abort checkpoint semantics
///
/// Emit is fallible. If it returns `Err`, the caller should treat this as
/// an abort signal: the transport channel has failed and continuing the
/// operation would produce results that cannot be delivered to the client.
pub trait ProgressEmitter {
    /// Emit a progress event for the current request.
    ///
    /// Returns `Ok(())` if the event was delivered successfully.
    /// Returns `Err(EmitError)` if the transport failed — the operation
    /// should abort at this checkpoint.
    fn emit(&mut self, detail: ProgressDetail) -> Result<(), EmitError>;
}

/// A no-op progress emitter for operations that don't need progress.
///
/// Used by default for methods that complete quickly. Always succeeds.
#[derive(Debug, Default)]
pub struct NoOpEmitter;

impl ProgressEmitter for NoOpEmitter {
    fn emit(&mut self, _detail: ProgressDetail) -> Result<(), EmitError> {
        Ok(())
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
    /// 3. Execute the method (optionally emitting progress via `emitter`)
    /// 4. Return success or error
    ///
    /// The dispatcher should NOT:
    /// - Parse JSON (already done)
    /// - Write to stdout (transport handles this)
    /// - Handle transport-level errors
    ///
    /// ## Progress emission
    ///
    /// Long-running operations can call `emitter.emit()` to send progress
    /// events. The transport writes these as separate NDJSON lines before
    /// the final response.
    fn dispatch(&self, request: &Request, emitter: &mut dyn ProgressEmitter) -> DispatchResult;
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
    fn dispatch(&self, request: &Request, emitter: &mut dyn ProgressEmitter) -> DispatchResult {
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

            "progress_test" => {
                // For testing progress emission
                let count = request.params.get("count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(3);
                for i in 1..=count {
                    if emitter.emit(ProgressDetail {
                        phase: "testing".to_string(),
                        current: i,
                        total: count,
                    }).is_err() {
                        // Abort on transport failure
                        return DispatchResult::error(
                            &request.id,
                            ErrorDetail::new(ErrorCode::InternalError, "progress delivery failed"),
                        );
                    }
                }
                DispatchResult::success(&request.id, serde_json::json!({"emitted": count}))
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
        let mut emitter = NoOpEmitter;
        let request = Request {
            id: "1".to_string(),
            method: "ping".to_string(),
            params: Value::Null,
        };
        let result = dispatcher.dispatch(&request, &mut emitter);
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
        let mut emitter = NoOpEmitter;
        let request = Request {
            id: "2".to_string(),
            method: "echo".to_string(),
            params: serde_json::json!({"hello": "world"}),
        };
        let result = dispatcher.dispatch(&request, &mut emitter);
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
        let mut emitter = NoOpEmitter;
        let request = Request {
            id: "3".to_string(),
            method: "bogus".to_string(),
            params: Value::Null,
        };
        let result = dispatcher.dispatch(&request, &mut emitter);
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
        let mut emitter = NoOpEmitter;
        let request = Request {
            id: "4".to_string(),
            method: "error".to_string(),
            params: serde_json::json!({"code": "CustomError", "message": "custom message"}),
        };
        let result = dispatcher.dispatch(&request, &mut emitter);
        match result {
            DispatchResult::Error(resp) => {
                assert_eq!(resp.id, "4");
                assert_eq!(resp.error.code, "CustomError");
                assert_eq!(resp.error.message, "custom message");
            }
            _ => panic!("expected error"),
        }
    }

    /// Test emitter that captures progress events for verification.
    #[derive(Debug, Default)]
    struct CapturingEmitter {
        events: Vec<ProgressDetail>,
    }

    impl ProgressEmitter for CapturingEmitter {
        fn emit(&mut self, detail: ProgressDetail) -> Result<(), EmitError> {
            self.events.push(detail);
            Ok(())
        }
    }

    #[test]
    fn mock_progress_test_emits_events() {
        let dispatcher = MockDispatcher::new();
        let mut emitter = CapturingEmitter::default();
        let request = Request {
            id: "5".to_string(),
            method: "progress_test".to_string(),
            params: serde_json::json!({"count": 3}),
        };
        let result = dispatcher.dispatch(&request, &mut emitter);

        // Verify progress events were emitted
        assert_eq!(emitter.events.len(), 3);
        assert_eq!(emitter.events[0].current, 1);
        assert_eq!(emitter.events[0].total, 3);
        assert_eq!(emitter.events[1].current, 2);
        assert_eq!(emitter.events[2].current, 3);

        // Verify final result
        match result {
            DispatchResult::Success(resp) => {
                assert_eq!(resp.id, "5");
                assert_eq!(resp.result["emitted"], 3);
            }
            _ => panic!("expected success"),
        }
    }

    #[test]
    fn noop_emitter_does_nothing() {
        let mut emitter = NoOpEmitter;
        let result = emitter.emit(ProgressDetail {
            phase: "test".to_string(),
            current: 1,
            total: 10,
        });
        // NoOpEmitter always succeeds
        assert!(result.is_ok());
    }

    /// Test emitter that fails after N successful emissions.
    struct FailingEmitter {
        fail_after: u64,
        count: u64,
    }

    impl FailingEmitter {
        fn new(fail_after: u64) -> Self {
            Self { fail_after, count: 0 }
        }
    }

    impl ProgressEmitter for FailingEmitter {
        fn emit(&mut self, _detail: ProgressDetail) -> Result<(), EmitError> {
            self.count += 1;
            if self.count > self.fail_after {
                Err(EmitError::new("simulated transport failure"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn progress_test_aborts_on_emitter_failure() {
        let dispatcher = MockDispatcher::new();
        // Emitter that fails after 1 successful emission
        let mut emitter = FailingEmitter::new(1);
        let request = Request {
            id: "fail-1".to_string(),
            method: "progress_test".to_string(),
            params: serde_json::json!({"count": 5}),
        };
        let result = dispatcher.dispatch(&request, &mut emitter);

        // Should return error, not success with emitted: 5
        match result {
            DispatchResult::Error(resp) => {
                assert_eq!(resp.id, "fail-1");
                assert_eq!(resp.error.code, "InternalError");
                assert!(resp.error.message.contains("progress delivery failed"));
            }
            DispatchResult::Success(_) => panic!("expected error due to emitter failure"),
        }
    }

    #[test]
    fn progress_test_succeeds_if_emitter_never_fails() {
        let dispatcher = MockDispatcher::new();
        // Emitter that never fails (fails after 100, but we only emit 3)
        let mut emitter = FailingEmitter::new(100);
        let request = Request {
            id: "ok-1".to_string(),
            method: "progress_test".to_string(),
            params: serde_json::json!({"count": 3}),
        };
        let result = dispatcher.dispatch(&request, &mut emitter);

        match result {
            DispatchResult::Success(resp) => {
                assert_eq!(resp.id, "ok-1");
                assert_eq!(resp.result["emitted"], 3);
            }
            DispatchResult::Error(_) => panic!("expected success"),
        }
    }
}
