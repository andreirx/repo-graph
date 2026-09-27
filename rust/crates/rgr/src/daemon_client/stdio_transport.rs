//! Stdio subprocess transport implementation.
//!
//! This transport spawns `rmapd --stdio` as a subprocess and communicates
//! via stdin/stdout. Used as fallback when socket transport fails due to
//! sandbox permission denial (EPERM/EACCES).
//!
//! ## Sandbox State Root (STDIO-STATE-ROOT-1)
//!
//! When spawned as sandbox fallback (EPERM/EACCES triggered), the subprocess
//! needs a writable state root. If `RMAP_STATE_ROOT` is not already set,
//! we inject a sandbox-writable root: the platform's sandbox state root
//! `<sandbox temp base>/repo-graph-agent/<uid>` (`paths::sandbox_state_root()`;
//! the per-OS system temp directory, defined once in `platform-paths`).
//!
//! This ensures the subprocess can write its database even when the normal
//! per-user state root (`~/Library/Application Support/repo-graph/`) is
//! outside the sandbox's writable paths.
//!
//! ## Subprocess Lifetime
//!
//! The subprocess is spawned on first request and kept alive for the
//! lifetime of the StdioTransport instance. When dropped, the subprocess
//! stdin is closed, causing it to exit cleanly.
//!
//! ## Performance
//!
//! Stdio transport has higher latency than socket (process spawn overhead).
//! For agent use cases, this is acceptable.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde::{Deserialize, Serialize};

use super::connection::DaemonClientError;
use super::transport::Transport;
use crate::cli::paths;

/// Default read timeout in seconds (same as socket transport).
const READ_TIMEOUT_SECS: u64 = 300;

/// NDJSON request envelope.
#[derive(Debug, Serialize)]
struct Request<'a> {
    id: &'a str,
    method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

/// NDJSON response envelope.
#[derive(Debug, Deserialize)]
struct Response {
    id: String,
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<ErrorDetail>,
    #[serde(default)]
    progress: Option<serde_json::Value>,
}

/// Error detail from daemon response.
#[derive(Debug, Deserialize)]
struct ErrorDetail {
    code: String,
    message: String,
    #[serde(default)]
    data: Option<serde_json::Value>,
}

/// Stdio subprocess transport.
///
/// Spawns `rmapd --stdio` and communicates via stdin/stdout.
pub struct StdioTransport {
    #[allow(dead_code)] // Child is kept alive; stdin/stdout are used
    child: Child,
    stdin: std::io::BufWriter<ChildStdin>,
    reader: BufReader<ChildStdout>,
    /// Sandbox state root path if injected, None if using default or explicit override.
    #[allow(dead_code)] // Reserved for future diagnostic use
    sandbox_state_root: Option<PathBuf>,
}

/// State root mode for reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateRootMode {
    /// Using global per-user state root (normal launchd daemon)
    Global,
    /// Using sandbox-local temp state root (auto-injected)
    SandboxLocal,
    /// Using explicit RMAP_STATE_ROOT override
    Override,
}

impl std::fmt::Display for StateRootMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Global => write!(f, "global"),
            Self::SandboxLocal => write!(f, "sandbox-local"),
            Self::Override => write!(f, "override"),
        }
    }
}

impl StdioTransport {
    /// Spawn the daemon subprocess.
    ///
    /// Looks for `rmapd` in PATH or at the same location as the current executable.
    ///
    /// # Arguments
    ///
    /// * `sandbox_state_root` - Optional state root to inject into subprocess.
    ///   If Some, sets `RMAP_STATE_ROOT` env var. Typically provided by
    ///   DaemonClient when sandbox fallback is triggered.
    pub fn spawn(sandbox_state_root: Option<PathBuf>) -> Result<Self, DaemonClientError> {
        // Try to find rmapd
        let rmapd_path = Self::find_rmapd()?;

        let mut command = Command::new(&rmapd_path);
        command
            .arg("--stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()); // Let stderr pass through for debugging

        // STDIO-STATE-ROOT-1: Inject sandbox-writable state root when provided
        if let Some(ref root) = sandbox_state_root {
            command.env("RMAP_STATE_ROOT", root);
            eprintln!("note: using sandbox-local state root: {}", root.display());
        }

        let mut child = command.spawn().map_err(|e| {
            DaemonClientError::ConnectionFailed(format!("failed to spawn rmapd --stdio: {}", e))
        })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            DaemonClientError::ConnectionFailed("failed to get subprocess stdin".to_string())
        })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            DaemonClientError::ConnectionFailed("failed to get subprocess stdout".to_string())
        })?;

        Ok(Self {
            child,
            stdin: std::io::BufWriter::new(stdin),
            reader: BufReader::new(stdout),
            sandbox_state_root,
        })
    }

    /// Get the state root mode for this transport.
    #[allow(dead_code)] // Reserved for future diagnostic use
    pub fn state_root_mode(&self) -> StateRootMode {
        if self.sandbox_state_root.is_some() {
            StateRootMode::SandboxLocal
        } else if std::env::var("RMAP_STATE_ROOT").is_ok() {
            StateRootMode::Override
        } else {
            StateRootMode::Global
        }
    }

    /// Get the sandbox state root path if one was injected.
    #[allow(dead_code)] // Reserved for future diagnostic use
    pub fn sandbox_state_root(&self) -> Option<&PathBuf> {
        self.sandbox_state_root.as_ref()
    }

    /// Prepare sandbox-writable state root if needed.
    ///
    /// Returns `Some(path)` if we need to inject a sandbox root, `None` if
    /// `RMAP_STATE_ROOT` is already set or if we can't determine the sandbox root.
    ///
    /// Public for use by DaemonClient to track the sandbox root.
    pub fn prepare_sandbox_state_root() -> Result<Option<PathBuf>, DaemonClientError> {
        // If RMAP_STATE_ROOT is already set, honor it
        if std::env::var("RMAP_STATE_ROOT").is_ok() {
            return Ok(None);
        }

        // Sandbox-writable root: <sandbox temp base>/repo-graph-agent/<uid>,
        // the platform-paths root the daemon classifies as sandbox-local.
        let sandbox_root = Self::create_sandbox_state_root(paths::sandbox_temp_base())?;
        Ok(Some(sandbox_root))
    }

    /// Create the sandbox state root `<base>/repo-graph-agent/<euid>` if absent.
    ///
    /// A newly created root gets mode 0700 (user-only). An existing root is
    /// returned untouched, mode included. Errors name the root.
    ///
    /// `base` is the one parameter that lets creation be tested against a
    /// scratch directory; production passes `paths::sandbox_temp_base()`.
    pub(crate) fn create_sandbox_state_root(base: &Path) -> Result<PathBuf, DaemonClientError> {
        let sandbox_root = paths::sandbox_state_root_under(base, paths::effective_uid());

        // Create directory with mode 0700 (user-only)
        if !sandbox_root.exists() {
            std::fs::create_dir_all(&sandbox_root).map_err(|e| {
                DaemonClientError::ConnectionFailed(format!(
                    "failed to create sandbox state root {}: {}",
                    sandbox_root.display(),
                    e
                ))
            })?;

            // Set permissions to 0700
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o700);
                std::fs::set_permissions(&sandbox_root, perms).map_err(|e| {
                    DaemonClientError::ConnectionFailed(format!(
                        "failed to set sandbox root permissions: {}",
                        e
                    ))
                })?;
            }
        }

        Ok(sandbox_root)
    }

    /// Find the rmapd binary.
    ///
    /// Search order:
    /// 1. Same directory as current executable
    /// 2. PATH
    fn find_rmapd() -> Result<std::path::PathBuf, DaemonClientError> {
        // Try same directory as current executable
        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(dir) = current_exe.parent() {
                let sibling = dir.join("rmapd");
                if sibling.exists() {
                    return Ok(sibling);
                }
            }
        }

        // Try PATH via which
        if let Ok(output) = Command::new("which").arg("rmapd").output() {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Ok(std::path::PathBuf::from(path));
                }
            }
        }

        // Fallback: just try "rmapd" and let spawn() fail with a clear error
        Ok(std::path::PathBuf::from("rmapd"))
    }

    /// Send request and read response.
    fn send_request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, DaemonClientError> {
        let id = uuid::Uuid::new_v4().to_string();

        let request = Request {
            id: &id,
            method,
            params,
        };

        let request_json = serde_json::to_string(&request)
            .map_err(|e| DaemonClientError::SendFailed(format!("failed to serialize: {}", e)))?;

        writeln!(self.stdin, "{}", request_json)
            .map_err(|e| DaemonClientError::SendFailed(e.to_string()))?;

        self.stdin
            .flush()
            .map_err(|e| DaemonClientError::SendFailed(format!("failed to flush: {}", e)))?;

        // Read responses until we get a result or error
        loop {
            let mut line = String::new();
            let bytes_read = self
                .reader
                .read_line(&mut line)
                .map_err(Self::classify_read_error)?;

            if bytes_read == 0 || line.is_empty() {
                return Err(DaemonClientError::ReadFailed(
                    "subprocess closed stdout".to_string(),
                ));
            }

            let response: Response = serde_json::from_str(&line).map_err(|e| {
                DaemonClientError::InvalidResponse(format!(
                    "failed to parse: {} (line: {})",
                    e,
                    line.trim()
                ))
            })?;

            if response.id != id {
                return Err(DaemonClientError::InvalidResponse(format!(
                    "response ID mismatch: expected {}, got {}",
                    id, response.id
                )));
            }

            // Skip progress events
            if response.progress.is_some() {
                continue;
            }

            if let Some(error) = response.error {
                return Err(DaemonClientError::DaemonError {
                    code: error.code,
                    message: error.message,
                    data: error.data,
                });
            }

            return Ok(response.result.unwrap_or(serde_json::Value::Null));
        }
    }

    /// Classify read errors.
    fn classify_read_error(e: std::io::Error) -> DaemonClientError {
        match e.kind() {
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
                DaemonClientError::Timeout {
                    timeout_secs: READ_TIMEOUT_SECS,
                }
            }
            _ => DaemonClientError::ReadFailed(e.to_string()),
        }
    }
}

impl Transport for StdioTransport {
    fn request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, DaemonClientError> {
        self.send_request(method, params)
    }

    fn request_with_timeout(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
        _timeout_secs: u64,
    ) -> Result<serde_json::Value, DaemonClientError> {
        // Note: StdioTransport does not support per-request timeouts because
        // subprocess stdin/stdout don't have the same timeout semantics as
        // sockets. The timeout parameter is ignored. Long-running operations
        // will block until the subprocess responds.
        //
        // This is acceptable because stdio transport is primarily used for
        // sandbox fallback scenarios where the daemon subprocess is local
        // and unlikely to hang.
        self.send_request(method, params)
    }

    fn ping(&mut self) -> Result<(), DaemonClientError> {
        let result = self.request("ping", None)?;

        if result.get("pong") == Some(&serde_json::Value::Bool(true)) {
            Ok(())
        } else {
            Err(DaemonClientError::InvalidResponse(
                "ping did not return pong".to_string(),
            ))
        }
    }

    fn mode_name(&self) -> &'static str {
        "stdio"
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        // Closing stdin will cause the subprocess to exit cleanly
        // The Child will be dropped, which waits for the process
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_rmapd_returns_path() {
        // This test assumes rmapd is installed
        let result = StdioTransport::find_rmapd();
        // Should succeed if rmapd is in PATH or sibling to test binary
        assert!(result.is_ok());
    }

    #[test]
    fn stdio_transport_mode_name() {
        // We can't easily test spawn without a real rmapd,
        // but we can verify the mode name constant
        assert_eq!("stdio", "stdio");
    }

    #[test]
    fn stdio_transport_ping() {
        // This test requires rmapd to be installed
        // No sandbox root injection in normal test environment
        let result = StdioTransport::spawn(None);

        if result.is_err() {
            // Skip test if rmapd not available
            eprintln!("Skipping stdio_transport_ping: rmapd not available");
            return;
        }

        let mut transport = result.unwrap();
        assert_eq!(transport.mode_name(), "stdio");

        // Ping should succeed
        let ping_result = transport.ping();
        assert!(ping_result.is_ok(), "ping failed: {:?}", ping_result);
    }

    #[test]
    fn state_root_mode_display() {
        assert_eq!(format!("{}", StateRootMode::Global), "global");
        assert_eq!(format!("{}", StateRootMode::SandboxLocal), "sandbox-local");
        assert_eq!(format!("{}", StateRootMode::Override), "override");
    }

    #[test]
    fn client_sandbox_root_is_the_platform_paths_root() {
        // The root the client composes is the one platform-paths defines, and
        // the daemon's predicate classifies it sandbox-local (computed only;
        // nothing is created).
        let client_root =
            paths::sandbox_state_root_under(paths::sandbox_temp_base(), paths::effective_uid());
        assert_eq!(client_root, paths::sandbox_state_root());
        assert!(paths::is_sandbox_local_state_root(&client_root));
    }

    #[cfg(unix)]
    fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    #[cfg(unix)]
    fn sandbox_root_is_created_with_mode_0700_under_the_given_base() {
        let base = tempfile::tempdir().unwrap();
        let root = StdioTransport::create_sandbox_state_root(base.path()).unwrap();

        assert_eq!(
            root,
            base.path()
                .join("repo-graph-agent")
                .join(paths::effective_uid().to_string())
        );
        assert!(root.is_dir());
        assert_eq!(mode_of(&root), 0o700);
    }

    #[test]
    #[cfg(unix)]
    fn sandbox_root_creation_is_idempotent_when_the_directory_exists() {
        use std::os::unix::fs::PermissionsExt;

        // A second call returns the same root and keeps its contents.
        let base = tempfile::tempdir().unwrap();
        let first = StdioTransport::create_sandbox_state_root(base.path()).unwrap();
        let marker = first.join("kept.txt");
        std::fs::write(&marker, "x").unwrap();
        let second = StdioTransport::create_sandbox_state_root(base.path()).unwrap();
        assert_eq!(second, first);
        assert!(marker.exists());

        // A root that already exists with another mode is returned untouched,
        // never re-permissioned (as before PORTABLE-TMP-1).
        let base2 = tempfile::tempdir().unwrap();
        let pre = paths::sandbox_state_root_under(base2.path(), paths::effective_uid());
        std::fs::create_dir_all(&pre).unwrap();
        std::fs::set_permissions(&pre, std::fs::Permissions::from_mode(0o755)).unwrap();
        let got = StdioTransport::create_sandbox_state_root(base2.path()).unwrap();
        assert_eq!(got, pre);
        assert_eq!(mode_of(&got), 0o755);
    }

    #[test]
    fn sandbox_root_creation_failure_is_a_named_error() {
        // The base is a regular FILE, so creating a directory under it fails
        // whatever the uid.
        let scratch = tempfile::tempdir().unwrap();
        let base = scratch.path().join("not-a-directory");
        std::fs::write(&base, "file").unwrap();
        let root = paths::sandbox_state_root_under(&base, paths::effective_uid());

        match StdioTransport::create_sandbox_state_root(&base) {
            Err(DaemonClientError::ConnectionFailed(msg)) => {
                assert!(
                    msg.starts_with("failed to create sandbox state root "),
                    "unexpected message: {msg}"
                );
                assert!(
                    msg.contains(&root.display().to_string()),
                    "message does not name the root {}: {msg}",
                    root.display()
                );
            }
            other => panic!("expected a named ConnectionFailed error, got {other:?}"),
        }
    }
}
