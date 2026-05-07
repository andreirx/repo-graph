//! Stdio transport for NDJSON communication.
//!
//! Reads requests from stdin (one JSON object per line), dispatches
//! them, and writes responses to stdout.
//!
//! ## Progress Streaming (D5b)
//!
//! Long-running operations can emit progress events during execution.
//! These are written as separate NDJSON lines before the final response.
//!
//! Event ordering contract:
//! 1. Request line in
//! 2. Zero or more progress event lines out
//! 3. One final response line out (success or error)

use std::io::{BufRead, BufReader, Write};

use crate::dispatch::{DispatchResult, Dispatcher, EmitError, ProgressEmitter};
use crate::envelope::{ErrorDetail, ErrorResponse, ProgressDetail, ProgressResponse, Request};
use crate::error::TransportError;

/// Request-scoped progress emitter that writes to an output stream.
///
/// Bound to a specific request ID. All emitted progress events are
/// correlated to that request and written as NDJSON lines.
///
/// ## Abort checkpoint semantics
///
/// If writing to the output stream fails (serialization or I/O), emit
/// returns an error. The caller should treat this as a transport failure
/// and abort the operation rather than continue with a broken control channel.
struct StdioEmitter<'a, W: Write> {
    request_id: &'a str,
    output: &'a mut W,
}

impl<'a, W: Write> StdioEmitter<'a, W> {
    fn new(request_id: &'a str, output: &'a mut W) -> Self {
        Self { request_id, output }
    }
}

impl<W: Write> ProgressEmitter for StdioEmitter<'_, W> {
    fn emit(&mut self, detail: ProgressDetail) -> Result<(), EmitError> {
        let response = ProgressResponse {
            id: self.request_id.to_string(),
            progress: detail,
        };

        // Serialize the progress event
        let json = serde_json::to_string(&response)
            .map_err(EmitError::from_serialize)?;

        // Write to output stream
        writeln!(self.output, "{}", json)
            .map_err(EmitError::from_io)?;

        // Flush to ensure immediate delivery
        self.output.flush()
            .map_err(EmitError::from_io)?;

        Ok(())
    }
}

/// Run the stdio transport loop.
///
/// Reads NDJSON requests from `input`, dispatches them via `dispatcher`,
/// and writes responses to `output`.
///
/// Returns when input reaches EOF or an I/O error occurs.
///
/// # Arguments
/// * `input` - The input stream (typically stdin)
/// * `output` - The output stream (typically stdout)
/// * `dispatcher` - The request dispatcher
///
/// # Returns
/// * `Ok(())` on graceful EOF
/// * `Err(TransportError)` on I/O error
///
/// # Progress events
///
/// During dispatch, long-running operations may emit progress events.
/// These are written as separate NDJSON lines before the final response.
/// The event ordering contract is:
/// 1. Zero or more progress lines (same request ID)
/// 2. One final response line (success or error)
pub fn run_transport<R, W, D>(
    input: R,
    mut output: W,
    dispatcher: &D,
) -> Result<(), TransportError>
where
    R: BufRead,
    W: Write,
    D: Dispatcher,
{
    for line_result in input.lines() {
        let line = line_result?;

        // Skip empty lines
        if line.trim().is_empty() {
            continue;
        }

        // Parse and dispatch (progress events written to output during dispatch)
        let response_json = match parse_and_dispatch(&line, dispatcher, &mut output) {
            Ok(json) => json,
            Err(json) => json,
        };

        // Write final response
        writeln!(output, "{}", response_json)
            .map_err(TransportError::OutputWrite)?;

        // Flush to ensure immediate delivery
        output.flush().map_err(TransportError::OutputWrite)?;
    }

    Ok(())
}

/// Parse a request line and dispatch it.
///
/// Returns Ok(json) for success, Err(json) for error.
/// Both cases return a JSON string to write to output.
///
/// Progress events are written directly to `output` during dispatch.
fn parse_and_dispatch<D: Dispatcher, W: Write>(
    line: &str,
    dispatcher: &D,
    output: &mut W,
) -> Result<String, String> {
    // Try to parse the request
    let request: Request = match serde_json::from_str(line) {
        Ok(req) => req,
        Err(e) => {
            // For malformed JSON, we cannot reliably extract the request ID.
            // The response uses "unknown" as the correlation ID.
            let error_resp = ErrorResponse::for_unparseable(
                ErrorDetail::parse_error(e.to_string()),
            );
            let json = serde_json::to_string(&error_resp)
                .unwrap_or_else(|_| r#"{"id":"unknown","error":{"code":"InternalError","message":"failed to serialize error"}}"#.to_string());
            return Err(json);
        }
    };

    // Validate required fields
    if request.id.is_empty() {
        let error_resp = ErrorResponse::for_unparseable(
            ErrorDetail::invalid_request("missing or empty 'id' field"),
        );
        let json = serde_json::to_string(&error_resp).unwrap();
        return Err(json);
    }

    if request.method.is_empty() {
        let error_resp = ErrorResponse::new(
            &request.id,
            ErrorDetail::invalid_request("missing or empty 'method' field"),
        );
        let json = serde_json::to_string(&error_resp).unwrap();
        return Err(json);
    }

    // Create request-scoped progress emitter
    let mut emitter = StdioEmitter::new(&request.id, output);

    // Dispatch the request (progress events written via emitter)
    let result = dispatcher.dispatch(&request, &mut emitter);

    // Serialize the final response
    let json = match result {
        DispatchResult::Success(resp) => serde_json::to_string(&resp).unwrap(),
        DispatchResult::Error(resp) => serde_json::to_string(&resp).unwrap(),
    };

    Ok(json)
}

/// Create a stdio transport runner using actual stdin/stdout.
///
/// This is the main entry point for the daemon's stdio mode.
pub fn run_stdio<D: Dispatcher>(dispatcher: &D) -> Result<(), TransportError> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    run_transport(BufReader::new(stdin.lock()), stdout.lock(), dispatcher)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch::MockDispatcher;
    use std::io::Cursor;

    fn run_with_input(input: &str) -> String {
        let input = Cursor::new(input);
        let mut output = Vec::new();
        let dispatcher = MockDispatcher::new();

        run_transport(BufReader::new(input), &mut output, &dispatcher).unwrap();

        String::from_utf8(output).unwrap()
    }

    #[test]
    fn ping_returns_pong() {
        let output = run_with_input(r#"{"id":"1","method":"ping"}"#);
        assert!(output.contains(r#""id":"1""#));
        assert!(output.contains(r#""pong":true"#));
    }

    #[test]
    fn echo_returns_params() {
        let output = run_with_input(r#"{"id":"2","method":"echo","params":{"x":42}}"#);
        assert!(output.contains(r#""id":"2""#));
        assert!(output.contains(r#""x":42"#));
    }

    #[test]
    fn unknown_method_returns_error() {
        let output = run_with_input(r#"{"id":"3","method":"bogus"}"#);
        assert!(output.contains(r#""id":"3""#));
        assert!(output.contains(r#""code":"UnknownMethod""#));
    }

    #[test]
    fn malformed_json_returns_parse_error_with_unknown_id() {
        // Malformed JSON can't be parsed to extract the ID
        let output = run_with_input(r#"{"id":"4", not valid json"#);
        assert!(output.contains(r#""id":"unknown""#));
        assert!(output.contains(r#""code":"ParseError""#));
    }

    #[test]
    fn completely_invalid_json_returns_unknown_id() {
        let output = run_with_input(r#"not json at all"#);
        assert!(output.contains(r#""id":"unknown""#));
        assert!(output.contains(r#""code":"ParseError""#));
    }

    #[test]
    fn valid_json_missing_required_field_returns_parse_error() {
        // Valid JSON structure but missing required 'method' field.
        // serde fails to parse because method has no default, so we
        // get ParseError with unknown id (can't extract id from parse failure).
        let output = run_with_input(r#"{"id":"5"}"#);
        assert!(output.contains(r#""id":"unknown""#));
        assert!(output.contains(r#""code":"ParseError""#));
    }

    #[test]
    fn empty_id_returns_invalid_request() {
        let output = run_with_input(r#"{"id":"","method":"ping"}"#);
        assert!(output.contains(r#""code":"InvalidRequest""#));
        assert!(output.contains("empty 'id'"));
    }

    #[test]
    fn empty_method_returns_invalid_request() {
        let output = run_with_input(r#"{"id":"5","method":""}"#);
        assert!(output.contains(r#""id":"5""#));
        assert!(output.contains(r#""code":"InvalidRequest""#));
        assert!(output.contains("empty 'method'"));
    }

    #[test]
    fn multiple_requests_processed_sequentially() {
        let input = r#"{"id":"1","method":"ping"}
{"id":"2","method":"echo","params":"hello"}
{"id":"3","method":"ping"}"#;
        let output = run_with_input(input);

        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains(r#""id":"1""#));
        assert!(lines[1].contains(r#""id":"2""#));
        assert!(lines[2].contains(r#""id":"3""#));
    }

    #[test]
    fn empty_lines_are_skipped() {
        let input = r#"{"id":"1","method":"ping"}

{"id":"2","method":"ping"}"#;
        let output = run_with_input(input);

        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn eof_causes_graceful_shutdown() {
        // Empty input = immediate EOF
        let output = run_with_input("");
        assert!(output.is_empty());
    }

    #[test]
    fn each_response_is_on_its_own_line() {
        let input = r#"{"id":"1","method":"ping"}
{"id":"2","method":"ping"}"#;
        let output = run_with_input(input);

        // Each response should end with newline
        for line in output.lines() {
            // Verify it's valid JSON
            let _: serde_json::Value = serde_json::from_str(line).unwrap();
        }
    }

    // ── Progress emission tests (D5b) ────────────────────────────────

    #[test]
    fn progress_events_emitted_before_final_response() {
        let output = run_with_input(r#"{"id":"p1","method":"progress_test","params":{"count":3}}"#);
        let lines: Vec<&str> = output.lines().collect();

        // Should have 3 progress events + 1 final response = 4 lines
        assert_eq!(lines.len(), 4, "expected 3 progress + 1 response, got: {:?}", lines);

        // First 3 lines should be progress events
        for (i, line) in lines[0..3].iter().enumerate() {
            let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(parsed["id"], "p1", "progress event {} should have correct id", i);
            assert!(parsed.get("progress").is_some(), "line {} should be progress event", i);
            assert_eq!(parsed["progress"]["current"], i as u64 + 1);
            assert_eq!(parsed["progress"]["total"], 3);
            assert_eq!(parsed["progress"]["phase"], "testing");
        }

        // Last line should be final response
        let final_resp: serde_json::Value = serde_json::from_str(lines[3]).unwrap();
        assert_eq!(final_resp["id"], "p1");
        assert!(final_resp.get("result").is_some(), "final line should be result");
        assert_eq!(final_resp["result"]["emitted"], 3);
    }

    #[test]
    fn progress_events_have_correct_request_id() {
        let output = run_with_input(r#"{"id":"unique-req-id","method":"progress_test","params":{"count":1}}"#);
        let lines: Vec<&str> = output.lines().collect();

        // Both progress and response should have the same request ID
        for line in &lines {
            let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(parsed["id"], "unique-req-id");
        }
    }

    #[test]
    fn zero_progress_events_when_count_is_zero() {
        let output = run_with_input(r#"{"id":"z1","method":"progress_test","params":{"count":0}}"#);
        let lines: Vec<&str> = output.lines().collect();

        // Should have only 1 final response
        assert_eq!(lines.len(), 1);
        let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed["id"], "z1");
        assert!(parsed.get("result").is_some());
    }

    #[test]
    fn non_progress_methods_emit_no_progress_events() {
        let output = run_with_input(r#"{"id":"np1","method":"ping"}"#);
        let lines: Vec<&str> = output.lines().collect();

        // Should have only 1 final response
        assert_eq!(lines.len(), 1);
        let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert!(parsed.get("progress").is_none(), "ping should not emit progress");
        assert!(parsed.get("result").is_some(), "ping should emit result");
    }

    #[test]
    fn multiple_requests_with_progress_interleave_correctly() {
        // Two progress_test requests in sequence
        let input = r#"{"id":"r1","method":"progress_test","params":{"count":2}}
{"id":"r2","method":"progress_test","params":{"count":1}}"#;
        let output = run_with_input(input);
        let lines: Vec<&str> = output.lines().collect();

        // r1: 2 progress + 1 response = 3 lines
        // r2: 1 progress + 1 response = 2 lines
        // Total = 5 lines
        assert_eq!(lines.len(), 5);

        // Lines 0,1 should be r1 progress, line 2 should be r1 response
        assert!(lines[0].contains(r#""id":"r1""#));
        assert!(lines[0].contains(r#""progress""#));
        assert!(lines[1].contains(r#""id":"r1""#));
        assert!(lines[1].contains(r#""progress""#));
        assert!(lines[2].contains(r#""id":"r1""#));
        assert!(lines[2].contains(r#""result""#));

        // Lines 3 should be r2 progress, line 4 should be r2 response
        assert!(lines[3].contains(r#""id":"r2""#));
        assert!(lines[3].contains(r#""progress""#));
        assert!(lines[4].contains(r#""id":"r2""#));
        assert!(lines[4].contains(r#""result""#));
    }

    // ── Abort checkpoint tests (D5b transport integrity) ────────────

    /// Writer that fails after N successful writes.
    struct FailingWriter {
        buffer: Vec<u8>,
        writes_remaining: usize,
    }

    impl FailingWriter {
        fn new(writes_until_failure: usize) -> Self {
            Self {
                buffer: Vec::new(),
                writes_remaining: writes_until_failure,
            }
        }

        fn output_so_far(&self) -> String {
            String::from_utf8_lossy(&self.buffer).to_string()
        }
    }

    impl Write for FailingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if self.writes_remaining == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "simulated transport failure",
                ));
            }
            self.writes_remaining -= 1;
            self.buffer.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            if self.writes_remaining == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "simulated transport failure on flush",
                ));
            }
            Ok(())
        }
    }

    #[test]
    fn broken_writer_aborts_progress_emission() {
        // Writer that fails after 2 writes (first progress event write + flush = 2)
        let mut output = FailingWriter::new(2);
        let input = Cursor::new(r#"{"id":"abort-1","method":"progress_test","params":{"count":5}}"#);
        let dispatcher = MockDispatcher::new();

        // Transport will fail when trying to write the second progress event
        let result = run_transport(BufReader::new(input), &mut output, &dispatcher);

        // The transport should encounter an error writing the final response
        // (since the emitter failure causes the dispatch to return an error,
        // and writing that error response will also fail)
        assert!(result.is_err(), "transport should fail when writer is broken");

        // Whatever was written before failure should be valid NDJSON
        let partial_output = output.output_so_far();
        if !partial_output.is_empty() {
            for line in partial_output.lines() {
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(line);
                assert!(parsed.is_ok(), "partial output should be valid JSON: {}", line);
            }
        }
    }

    #[test]
    fn emitter_failure_returns_error_not_success() {
        // This tests the MockDispatcher's abort behavior via a capturing approach.
        // We use a custom dispatcher that tracks what happened.

        // For this test, we use a writer that works for progress but fails
        // at a specific point. The key assertion is that if emit() returns
        // Err, the dispatcher should not return a success result.

        // Since MockDispatcher.progress_test checks emit result and returns
        // error on failure, we verify this through the existing test infrastructure.
        // The test `progress_test_aborts_on_emitter_failure` in dispatch::tests
        // covers this case directly.

        // Here we just verify the end-to-end: if writer eventually fails,
        // we don't get a success result for the operation.

        // Writer that fails on 4th write (allows 1 progress + flush, then fails on 2nd progress)
        let mut output = FailingWriter::new(3);
        let input = Cursor::new(r#"{"id":"e2e-abort","method":"progress_test","params":{"count":5}}"#);
        let dispatcher = MockDispatcher::new();

        let _ = run_transport(BufReader::new(input), &mut output, &dispatcher);

        let partial_output = output.output_so_far();
        // Should NOT contain a success result with "emitted": 5
        assert!(
            !partial_output.contains(r#""emitted":5"#),
            "should not complete successfully when emitter fails: {}",
            partial_output
        );
    }
}
