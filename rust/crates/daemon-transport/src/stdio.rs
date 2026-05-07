//! Stdio transport for NDJSON communication.
//!
//! Reads requests from stdin (one JSON object per line), dispatches
//! them, and writes responses to stdout.

use std::io::{BufRead, BufReader, Write};

use crate::dispatch::{DispatchResult, Dispatcher};
use crate::envelope::{ErrorDetail, ErrorResponse, Request};
use crate::error::TransportError;

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

        // Parse and dispatch
        let response_json = match parse_and_dispatch(&line, dispatcher) {
            Ok(json) => json,
            Err(json) => json,
        };

        // Write response
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
fn parse_and_dispatch<D: Dispatcher>(line: &str, dispatcher: &D) -> Result<String, String> {
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

    // Dispatch the request
    let result = dispatcher.dispatch(&request);

    // Serialize the response
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
}
