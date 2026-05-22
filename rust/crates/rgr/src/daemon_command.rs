//! Shared CLI support for REG-1-style daemon commands.
//!
//! LEGACY-CONTRACT-MIGRATION-1A: This module provides reusable infrastructure
//! for CLI commands that communicate with the daemon using the REG-1 pattern:
//!
//! - Repo resolution from cwd (auto-discovery)
//! - Daemon availability handling
//! - Request execution with error classification
//! - Repo-not-found handling with actionable hints
//! - Timeout and runtime error mapping
//! - JSON passthrough vs human-render branching
//! - Consistent exit code policy
//!
//! # Exit Code Policy
//!
//! | Code | Meaning |
//! |------|---------|
//! | 0    | Success |
//! | 1    | Usage error (bad args, unknown flag) |
//! | 2    | Runtime error (daemon unavailable, repo not found, timeout) |
//!
//! # Design Constraints
//!
//! This module handles transport and routing only. Commands still own:
//! - Their request params
//! - Their response DTO
//! - Their renderer choice
//!
//! The support layer is narrow and stable; command-specific logic stays outside.

use std::process::ExitCode;

use crate::daemon_client::{daemon_unavailable_message, DaemonClient, DaemonClientError};

// ── Exit code constants ──────────────────────────────────────────────────────

/// Exit code for successful execution.
pub const EXIT_SUCCESS: u8 = 0;

/// Exit code for usage errors (bad arguments, unknown flags).
pub const EXIT_USAGE_ERROR: u8 = 1;

/// Exit code for runtime errors (daemon unavailable, repo not found, timeout).
pub const EXIT_RUNTIME_ERROR: u8 = 2;

// ── Repo resolution ──────────────────────────────────────────────────────────

/// Resolve the repository path from the current working directory.
///
/// Returns the canonicalized absolute path as a string, suitable for
/// passing to daemon requests as the `repo` parameter.
///
/// # Errors
///
/// Returns a human-readable error message if:
/// - Cannot determine current directory
/// - Cannot canonicalize the path
pub fn resolve_repo_from_cwd() -> Result<String, String> {
    let cwd =
        std::env::current_dir().map_err(|e| format!("cannot get current directory: {}", e))?;

    cwd.canonicalize()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| format!("cannot canonicalize current directory: {}", e))
}

// ── Error classification ─────────────────────────────────────────────────────

/// Classified daemon error for consistent handling.
///
/// Commands can match on this to provide appropriate user feedback.
#[derive(Debug)]
pub enum DaemonError {
    /// Daemon is not running or socket unreachable.
    Unavailable { socket_path: String },
    /// Repository not found in daemon registry.
    RepoNotFound { message: String },
    /// Request timed out waiting for daemon response.
    Timeout { timeout_secs: u64 },
    /// Other runtime error from daemon.
    RuntimeError {
        code: String,
        message: String,
        /// Optional structured data (e.g., ambiguous symbol matches).
        data: Option<serde_json::Value>,
    },
}

impl DaemonError {
    /// Create from a DaemonClientError.
    pub fn from_client_error(err: DaemonClientError, socket_path: &std::path::Path) -> Self {
        match err {
            DaemonClientError::ConnectionFailed(_) => DaemonError::Unavailable {
                socket_path: socket_path.to_string_lossy().to_string(),
            },
            DaemonClientError::Timeout { timeout_secs } => DaemonError::Timeout { timeout_secs },
            DaemonClientError::DaemonError {
                code,
                message,
                data,
            } => {
                if code == "RepoNotFound" {
                    DaemonError::RepoNotFound { message }
                } else {
                    DaemonError::RuntimeError {
                        code,
                        message,
                        data,
                    }
                }
            }
            DaemonClientError::SendFailed(msg) => DaemonError::RuntimeError {
                code: "SendFailed".to_string(),
                message: msg,
                data: None,
            },
            DaemonClientError::ReadFailed(msg) => DaemonError::RuntimeError {
                code: "ReadFailed".to_string(),
                message: msg,
                data: None,
            },
            DaemonClientError::InvalidResponse(msg) => DaemonError::RuntimeError {
                code: "InvalidResponse".to_string(),
                message: msg,
                data: None,
            },
        }
    }

    /// Get the exit code for this error.
    pub fn exit_code(&self) -> u8 {
        EXIT_RUNTIME_ERROR
    }
}

impl std::fmt::Display for DaemonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DaemonError::Unavailable { socket_path } => {
                write!(f, "daemon unavailable (socket: {})", socket_path)
            }
            DaemonError::RepoNotFound { message } => {
                write!(f, "repo not indexed: {}", message)
            }
            DaemonError::Timeout { timeout_secs } => {
                write!(f, "daemon response timed out after {}s", timeout_secs)
            }
            DaemonError::RuntimeError { code, message, .. } => {
                write!(f, "{}: {}", code, message)
            }
        }
    }
}

// ── Daemon request execution ─────────────────────────────────────────────────

/// Result of a daemon request execution.
pub type DaemonResult = Result<serde_json::Value, DaemonError>;

/// Execute a daemon request with standard error handling.
///
/// This is the core request execution helper. It:
/// 1. Connects to the daemon
/// 2. Checks availability
/// 3. Sends the request
/// 4. Classifies any errors
///
/// # Arguments
///
/// * `method` - The daemon method to call (e.g., "orient", "trust")
/// * `params` - Optional JSON parameters for the request
///
/// # Returns
///
/// * `Ok(Value)` - The daemon's JSON response on success
/// * `Err(DaemonError)` - Classified error on failure
pub fn execute_daemon_request(method: &str, params: Option<serde_json::Value>) -> DaemonResult {
    // Create client
    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            // Fallback socket path for error message
            let socket_path = crate::cli::paths::daemon_socket_path()
                .unwrap_or_else(|| std::path::PathBuf::from("/unknown/socket.sock"));
            return Err(DaemonError::from_client_error(e, &socket_path));
        }
    };

    let socket_path = client.socket_path().clone();

    // Check availability
    if !client.is_available() {
        return Err(DaemonError::Unavailable {
            socket_path: socket_path.to_string_lossy().to_string(),
        });
    }

    // Execute request
    client
        .request(method, params)
        .map_err(|e| DaemonError::from_client_error(e, &socket_path))
}

/// Execute a daemon request for a repo resolved from cwd.
///
/// Convenience wrapper that:
/// 1. Resolves repo from cwd
/// 2. Adds `repo` to params
/// 3. Calls `execute_daemon_request`
///
/// # Arguments
///
/// * `method` - The daemon method to call
/// * `extra_params` - Additional parameters (merged with `repo`)
///
/// # Returns
///
/// * `Ok(Value)` - The daemon's JSON response
/// * `Err(DaemonError)` - On repo resolution or daemon error
pub fn execute_repo_request(method: &str, extra_params: Option<serde_json::Value>) -> DaemonResult {
    let repo_path = resolve_repo_from_cwd().map_err(|msg| DaemonError::RuntimeError {
        code: "RepoResolutionFailed".to_string(),
        message: msg,
        data: None,
    })?;

    let params = match extra_params {
        Some(serde_json::Value::Object(mut map)) => {
            map.insert("repo".to_string(), serde_json::Value::String(repo_path));
            Some(serde_json::Value::Object(map))
        }
        Some(other) => {
            // Non-object params: wrap in object with repo
            let mut map = serde_json::Map::new();
            map.insert("repo".to_string(), serde_json::Value::String(repo_path));
            map.insert("params".to_string(), other);
            Some(serde_json::Value::Object(map))
        }
        None => Some(serde_json::json!({ "repo": repo_path })),
    };

    execute_daemon_request(method, params)
}

// ── Error printing ───────────────────────────────────────────────────────────

/// Print a daemon error with appropriate hints.
///
/// Provides actionable guidance based on error type:
/// - Unavailable: "Start with: rmapd"
/// - RepoNotFound: "hint: run 'rmap index .' to index this repo"
/// - Timeout: explains the timeout duration
/// - RuntimeError: shows error code and message
pub fn print_daemon_error(err: &DaemonError, command_name: &str) {
    match err {
        DaemonError::Unavailable { socket_path } => {
            eprintln!(
                "{}",
                daemon_unavailable_message(std::path::Path::new(socket_path), command_name)
            );
        }
        DaemonError::RepoNotFound { .. } => {
            eprintln!("error: repo not indexed");
            eprintln!("hint: run 'rmap index .' to index this repo");
        }
        DaemonError::Timeout { timeout_secs } => {
            eprintln!("error: daemon response timed out after {}s", timeout_secs);
            eprintln!("hint: the operation may still be running on the daemon");
        }
        DaemonError::RuntimeError { code, message, .. } => {
            eprintln!("error: {}: {}", code, message);
        }
    }
}

// ── Output handling ──────────────────────────────────────────────────────────

/// Output a daemon result as JSON.
///
/// Pretty-prints the JSON value to stdout.
///
/// # Returns
///
/// * `ExitCode::SUCCESS` on success
/// * `ExitCode::from(EXIT_RUNTIME_ERROR)` on serialization failure
pub fn output_json(result: &serde_json::Value) -> ExitCode {
    match serde_json::to_string_pretty(result) {
        Ok(json) => {
            println!("{}", json);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: failed to serialize result: {}", e);
            ExitCode::from(EXIT_RUNTIME_ERROR)
        }
    }
}

/// Output a daemon result in human or JSON mode.
///
/// # Type Parameters
///
/// * `T` - The response DTO type (must implement Deserialize)
///
/// # Arguments
///
/// * `result` - The raw JSON result from daemon
/// * `json_mode` - If true, output raw JSON; if false, parse and render
/// * `render` - Function that renders the parsed DTO to a human-readable string
///
/// # Returns
///
/// * `ExitCode::SUCCESS` on success
/// * `ExitCode::from(EXIT_RUNTIME_ERROR)` on parse/serialize failure
///
/// # Design Note
///
/// The `render` function is provided by the caller, keeping DTO parsing
/// and rendering outside this support module. This maintains the boundary:
/// support handles transport, commands own their DTOs and renderers.
pub fn output_result<T, F>(result: serde_json::Value, json_mode: bool, render: F) -> ExitCode
where
    T: serde::de::DeserializeOwned,
    F: FnOnce(T) -> String,
{
    if json_mode {
        output_json(&result)
    } else {
        match serde_json::from_value::<T>(result) {
            Ok(response) => {
                print!("{}", render(response));
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: failed to parse response: {}", e);
                ExitCode::from(EXIT_RUNTIME_ERROR)
            }
        }
    }
}

/// Output a daemon result with custom exit code extraction.
///
/// Same as `output_result`, but allows extracting the exit code from the
/// response (e.g., for `gate` which returns exit code in the response).
///
/// # Arguments
///
/// * `result` - The raw JSON result from daemon
/// * `json_mode` - If true, output raw JSON; if false, parse and render
/// * `render` - Function that renders the parsed DTO to a human-readable string
/// * `extract_exit_code` - Function that extracts exit code from the result
pub fn output_result_with_exit_code<T, F, E>(
    result: serde_json::Value,
    json_mode: bool,
    render: F,
    extract_exit_code: E,
) -> ExitCode
where
    T: serde::de::DeserializeOwned,
    F: FnOnce(&T) -> String,
    E: FnOnce(&serde_json::Value) -> u8,
{
    let exit_code = extract_exit_code(&result);

    if json_mode {
        match serde_json::to_string_pretty(&result) {
            Ok(json) => {
                println!("{}", json);
                ExitCode::from(exit_code)
            }
            Err(e) => {
                eprintln!("error: failed to serialize result: {}", e);
                ExitCode::from(EXIT_RUNTIME_ERROR)
            }
        }
    } else {
        match serde_json::from_value::<T>(result) {
            Ok(response) => {
                print!("{}", render(&response));
                ExitCode::from(exit_code)
            }
            Err(e) => {
                eprintln!("error: failed to parse response: {}", e);
                ExitCode::from(EXIT_RUNTIME_ERROR)
            }
        }
    }
}

// ── Full command execution helper ────────────────────────────────────────────

/// Execute a full daemon command with standard error handling.
///
/// This is the highest-level helper that combines:
/// 1. Repo resolution from cwd
/// 2. Request execution
/// 3. Error handling with hints
/// 4. Output in JSON or human mode
///
/// # Type Parameters
///
/// * `T` - The response DTO type
///
/// # Arguments
///
/// * `method` - The daemon method to call
/// * `extra_params` - Additional parameters (merged with `repo`)
/// * `json_mode` - Output mode
/// * `command_name` - For error messages
/// * `render` - Function to render DTO to human-readable string
///
/// # Returns
///
/// Appropriate exit code based on success/failure.
pub fn run_daemon_command<T, F>(
    method: &str,
    extra_params: Option<serde_json::Value>,
    json_mode: bool,
    command_name: &str,
    render: F,
) -> ExitCode
where
    T: serde::de::DeserializeOwned,
    F: FnOnce(T) -> String,
{
    match execute_repo_request(method, extra_params) {
        Ok(result) => output_result(result, json_mode, render),
        Err(err) => {
            print_daemon_error(&err, command_name);
            ExitCode::from(err.exit_code())
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Repo resolution tests ────────────────────────────────────────────

    #[test]
    fn resolve_repo_from_cwd_returns_canonical_path() {
        // This test runs in the repo directory, so cwd should be resolvable
        let result = resolve_repo_from_cwd();
        assert!(result.is_ok(), "should resolve cwd: {:?}", result);

        let path = result.unwrap();
        assert!(path.starts_with('/'), "should be absolute path: {}", path);
        assert!(
            !path.contains(".."),
            "should be canonical (no ..): {}",
            path
        );
    }

    // ── Error classification tests ───────────────────────────────────────

    #[test]
    fn daemon_error_from_connection_failed() {
        let client_err = DaemonClientError::ConnectionFailed("test".to_string());
        let err =
            DaemonError::from_client_error(client_err, std::path::Path::new("/test/socket.sock"));

        assert!(matches!(err, DaemonError::Unavailable { .. }));
        if let DaemonError::Unavailable { socket_path } = err {
            assert_eq!(socket_path, "/test/socket.sock");
        }
    }

    #[test]
    fn daemon_error_from_timeout() {
        let client_err = DaemonClientError::Timeout { timeout_secs: 300 };
        let err =
            DaemonError::from_client_error(client_err, std::path::Path::new("/test/socket.sock"));

        assert!(matches!(err, DaemonError::Timeout { timeout_secs: 300 }));
    }

    #[test]
    fn daemon_error_from_repo_not_found() {
        let client_err = DaemonClientError::DaemonError {
            code: "RepoNotFound".to_string(),
            message: "no such repo".to_string(),
            data: None,
        };
        let err =
            DaemonError::from_client_error(client_err, std::path::Path::new("/test/socket.sock"));

        assert!(matches!(err, DaemonError::RepoNotFound { .. }));
        if let DaemonError::RepoNotFound { message } = err {
            assert_eq!(message, "no such repo");
        }
    }

    #[test]
    fn daemon_error_from_other_daemon_error() {
        let client_err = DaemonClientError::DaemonError {
            code: "SomeOtherError".to_string(),
            message: "something went wrong".to_string(),
            data: Some(serde_json::json!({"detail": "extra info"})),
        };
        let err =
            DaemonError::from_client_error(client_err, std::path::Path::new("/test/socket.sock"));

        assert!(matches!(err, DaemonError::RuntimeError { .. }));
        if let DaemonError::RuntimeError {
            code,
            message,
            data,
        } = err
        {
            assert_eq!(code, "SomeOtherError");
            assert_eq!(message, "something went wrong");
            assert!(data.is_some());
        }
    }

    #[test]
    fn daemon_error_from_send_failed() {
        let client_err = DaemonClientError::SendFailed("write error".to_string());
        let err =
            DaemonError::from_client_error(client_err, std::path::Path::new("/test/socket.sock"));

        assert!(matches!(err, DaemonError::RuntimeError { .. }));
        if let DaemonError::RuntimeError { code, .. } = err {
            assert_eq!(code, "SendFailed");
        }
    }

    #[test]
    fn daemon_error_from_read_failed() {
        let client_err = DaemonClientError::ReadFailed("read error".to_string());
        let err =
            DaemonError::from_client_error(client_err, std::path::Path::new("/test/socket.sock"));

        assert!(matches!(err, DaemonError::RuntimeError { .. }));
        if let DaemonError::RuntimeError { code, .. } = err {
            assert_eq!(code, "ReadFailed");
        }
    }

    #[test]
    fn daemon_error_from_invalid_response() {
        let client_err = DaemonClientError::InvalidResponse("parse error".to_string());
        let err =
            DaemonError::from_client_error(client_err, std::path::Path::new("/test/socket.sock"));

        assert!(matches!(err, DaemonError::RuntimeError { .. }));
        if let DaemonError::RuntimeError { code, .. } = err {
            assert_eq!(code, "InvalidResponse");
        }
    }

    // ── Exit code tests ──────────────────────────────────────────────────

    #[test]
    fn all_daemon_errors_return_runtime_exit_code() {
        let errors = vec![
            DaemonError::Unavailable {
                socket_path: "/test".to_string(),
            },
            DaemonError::RepoNotFound {
                message: "test".to_string(),
            },
            DaemonError::Timeout { timeout_secs: 60 },
            DaemonError::RuntimeError {
                code: "Test".to_string(),
                message: "test".to_string(),
                data: None,
            },
        ];

        for err in errors {
            assert_eq!(err.exit_code(), EXIT_RUNTIME_ERROR, "error: {:?}", err);
        }
    }

    // ── Display tests ────────────────────────────────────────────────────

    #[test]
    fn daemon_error_display_unavailable() {
        let err = DaemonError::Unavailable {
            socket_path: "/var/run/rmapd.sock".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("daemon unavailable"));
        assert!(msg.contains("/var/run/rmapd.sock"));
    }

    #[test]
    fn daemon_error_display_repo_not_found() {
        let err = DaemonError::RepoNotFound {
            message: "not in registry".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("repo not indexed"));
    }

    #[test]
    fn daemon_error_display_timeout() {
        let err = DaemonError::Timeout { timeout_secs: 300 };
        let msg = format!("{}", err);
        assert!(msg.contains("timed out"));
        assert!(msg.contains("300s"));
    }

    #[test]
    fn daemon_error_display_runtime_error() {
        let err = DaemonError::RuntimeError {
            code: "TestCode".to_string(),
            message: "test message".to_string(),
            data: None,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("TestCode"));
        assert!(msg.contains("test message"));
    }

    // ── Output tests ─────────────────────────────────────────────────────

    #[test]
    fn output_json_serializes_value() {
        // Can't easily test stdout, but we can test the logic doesn't panic
        let value = serde_json::json!({"test": true});
        // This would print to stdout, which is fine in tests
        let _ = output_json(&value);
    }

    // ── Integration-style test ───────────────────────────────────────────

    #[test]
    fn execute_repo_request_handles_daemon_states() {
        // Test that execute_repo_request properly classifies errors.
        // The actual result depends on whether daemon is running.
        let result = execute_repo_request("__nonexistent_test_method__", None);

        match result {
            Err(DaemonError::Unavailable { .. }) => {
                // Expected if daemon not running
            }
            Err(DaemonError::RuntimeError { code, .. }) if code == "RepoResolutionFailed" => {
                // Acceptable: cwd resolution failed (unusual but possible)
            }
            Err(DaemonError::RuntimeError { code, .. }) if code == "UnknownMethod" => {
                // Acceptable: daemon is running but method doesn't exist
                // This proves the request went through correctly
            }
            Err(DaemonError::RepoNotFound { .. }) => {
                // Acceptable: daemon is running, method exists, but repo not indexed
            }
            Ok(_) => {
                panic!("test method should not succeed");
            }
            other => {
                panic!("unexpected error type: {:?}", other);
            }
        }
    }
}
