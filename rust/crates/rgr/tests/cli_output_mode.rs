//! CLI-level output mode tests for CLI-OUT-1.
//!
//! Tests that verify the CLI binary produces correct output in both:
//! - Human mode (default): plain text with structured markers
//! - JSON mode (`--json`): full daemon envelope
//!
//! # Test Strategy
//!
//! These tests start an actual `rmapd` daemon process in the background,
//! then invoke the `rmap` binary against it. This tests the full CLI code path
//! including presentation layer rendering.
//!
//! # Isolation
//!
//! Each test uses:
//! - Isolated temp directory for daemon state (via RMAP_STATE_ROOT)
//! - Isolated temp directory for test repo
//! - Isolated socket path via `RMAP_SOCKET_PATH`
//!
//! # Technical Debt
//!
//! **TD-CLI-OUT-1-A: Manual pre-build requirement**
//!
//! These tests require `rmapd` to be built before running:
//! ```
//! cargo build -p rmapd
//! cargo test -p repo-graph-rgr --test cli_output_mode
//! ```
//!
//! This is a divergence from ideal test automation. Cargo does not automatically
//! build binaries from other packages when running `cargo test`. The harness
//! assumes `rmapd` is a sibling of `rmap` in the target directory.
//!
//! If the package layout changes, this harness becomes fragile. A proper fix
//! would require either:
//! - A workspace-level test runner script
//! - Converting to in-process testing (avoiding subprocess spawning)
//! - Using `cargo test --workspace` with proper dependency ordering
//!
//! **TD-CLI-OUT-1-B: Unix socket permission requirement**
//!
//! These tests require Unix socket bind permission. They will fail in sandboxed
//! environments that restrict socket operations. This is an environmental
//! constraint, not a code defect.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use tempfile::{tempdir, TempDir};

fn rmap_binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

fn rmapd_binary_path() -> PathBuf {
    // rmapd is in the same target directory as rmap
    let rmap_path = PathBuf::from(env!("CARGO_BIN_EXE_rmap"));
    let parent = rmap_path
        .parent()
        .expect("rmap binary should have parent dir");
    let rmapd_path = parent.join("rmapd");

    // Verify the binary exists to give a clear error message
    if !rmapd_path.exists() {
        panic!(
            "rmapd binary not found at {:?}. Run `cargo build -p rmapd` first.",
            rmapd_path
        );
    }

    rmapd_path
}

/// Test harness that manages a daemon process for CLI testing.
struct DaemonHarness {
    socket_path: PathBuf,
    state_root: PathBuf,
    daemon_process: Option<Child>,
    _state_temp: TempDir, // Keep alive for lifetime of test
    _repo_temp: TempDir,  // Keep alive for lifetime of test
    repo_path: PathBuf,
}

impl DaemonHarness {
    /// Create a new test harness with daemon running on isolated socket.
    ///
    /// Indexes the provided repo so queries will succeed.
    fn new() -> Self {
        let state_temp = tempdir().expect("failed to create state temp dir");
        let repo_temp = tempdir().expect("failed to create repo temp dir");

        // Create minimal test repo
        let repo_path = repo_temp.path().join("test-repo");
        std::fs::create_dir(&repo_path).unwrap();
        std::fs::write(
            repo_path.join("main.ts"),
            r#"
export function greet(name: string): string {
    return `Hello, ${name}`;
}

export function processData(items: string[]): number {
    return items.length;
}
"#,
        )
        .unwrap();

        // Set up isolated paths
        let socket_path = state_temp.path().join("daemon.sock");
        let state_root = state_temp.path().to_path_buf();

        // Start daemon process
        let mut daemon_process = Command::new(rmapd_binary_path())
            .env("RMAP_SOCKET_PATH", &socket_path)
            .env("RMAP_STATE_ROOT", &state_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to start rmapd");

        // Wait for daemon to be ready (socket exists and accepts connections)
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(10);
        let mut daemon_ready = false;

        while start.elapsed() < timeout {
            // Check if daemon process has exited (failed to start)
            if let Ok(Some(status)) = daemon_process.try_wait() {
                // Daemon exited prematurely - capture stderr for diagnostics
                let mut stderr_output = String::new();
                if let Some(ref mut stderr) = daemon_process.stderr {
                    let _ = stderr.read_to_string(&mut stderr_output);
                }
                panic!(
                    "daemon exited prematurely with status {:?}\n\
                     socket_path: {:?}\n\
                     state_root: {:?}\n\
                     stderr:\n{}",
                    status, socket_path, state_root, stderr_output
                );
            }

            if socket_path.exists() {
                // Try to connect
                if std::os::unix::net::UnixStream::connect(&socket_path).is_ok() {
                    daemon_ready = true;
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        if !daemon_ready {
            // Timeout - capture stderr for diagnostics
            let mut stderr_output = String::new();
            if let Some(ref mut stderr) = daemon_process.stderr {
                let _ = stderr.read_to_string(&mut stderr_output);
            }
            // Kill the daemon process before panicking
            let _ = daemon_process.kill();
            let _ = daemon_process.wait();

            panic!(
                "daemon socket not created within timeout\n\
                 socket_path: {:?}\n\
                 state_root: {:?}\n\
                 socket_exists: {}\n\
                 stderr:\n{}",
                socket_path,
                state_root,
                socket_path.exists(),
                stderr_output
            );
        }

        let mut harness = Self {
            socket_path,
            state_root,
            daemon_process: Some(daemon_process),
            _state_temp: state_temp,
            _repo_temp: repo_temp,
            repo_path,
        };

        // Index the repo
        harness.index_repo();

        harness
    }

    /// Index the test repo via the daemon.
    fn index_repo(&mut self) {
        let output = Command::new(rmap_binary_path())
            .env("RMAP_SOCKET_PATH", &self.socket_path)
            .env("RMAP_STATE_ROOT", &self.state_root)
            .args(["index", self.repo_path.to_str().unwrap()])
            .output()
            .expect("failed to run index command");

        if output.status.code() != Some(0) {
            panic!(
                "index command failed with exit code {:?}.\nstderr: {}\nstdout: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr),
                String::from_utf8_lossy(&output.stdout)
            );
        }
    }

    /// Run a CLI command with this harness's daemon.
    fn run_cli(&self, args: &[&str]) -> std::process::Output {
        Command::new(rmap_binary_path())
            .env("RMAP_SOCKET_PATH", &self.socket_path)
            .env("RMAP_STATE_ROOT", &self.state_root)
            .current_dir(&self.repo_path)
            .args(args)
            .output()
            .expect("failed to spawn rmap")
    }
}

impl Drop for DaemonHarness {
    fn drop(&mut self) {
        if let Some(mut child) = self.daemon_process.take() {
            // Send SIGTERM to the daemon for clean shutdown
            #[cfg(unix)]
            unsafe {
                libc::kill(child.id() as i32, libc::SIGTERM);
            }
            // Wait briefly for clean shutdown
            std::thread::sleep(Duration::from_millis(100));
            // Force kill if still running
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// ORIENT OUTPUT MODE TESTS
//
// These tests require `rmapd` to be built first: `cargo build -p rmapd`
// Run with: `cargo test -p repo-graph-rgr --test cli_output_mode -- --ignored`
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "requires rmapd binary and Unix socket permission - run with --ignored"]
fn orient_human_mode_contains_structured_markers() {
    let harness = DaemonHarness::new();
    let output = harness.run_cli(&["orient"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "orient should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Human mode markers
    assert!(
        stdout.contains("Repo:"),
        "Human output should contain 'Repo:' marker. stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Confidence:"),
        "Human output should contain 'Confidence:' marker. stdout:\n{}",
        stdout
    );

    // Should NOT contain JSON envelope fields
    assert!(
        !stdout.contains(r#""command":"#),
        "Human output should not contain JSON 'command' field. stdout:\n{}",
        stdout
    );
    assert!(
        !stdout.contains(r#""snapshot_uid":"#),
        "Human output should not contain 'snapshot_uid' field. stdout:\n{}",
        stdout
    );
    assert!(
        !stdout.contains(r#""schema":"#),
        "Human output should not contain 'schema' field. stdout:\n{}",
        stdout
    );
}

#[test]
#[ignore = "requires rmapd binary and Unix socket permission - run with --ignored"]
fn orient_json_mode_returns_valid_envelope() {
    let harness = DaemonHarness::new();
    let output = harness.run_cli(&["orient", "--json"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "orient --json should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Should have envelope fields (the CLI extracts "result" from daemon response)
    assert!(
        parsed.get("command").is_some(),
        "JSON output should have 'command' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("schema").is_some(),
        "JSON output should have 'schema' field. stdout:\n{}",
        stdout
    );
    // Orient-specific fields
    assert!(
        parsed.get("confidence").is_some(),
        "JSON output should have 'confidence' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("signals").is_some(),
        "JSON output should have 'signals' field. stdout:\n{}",
        stdout
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// CHECK OUTPUT MODE TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "requires rmapd binary and Unix socket permission - run with --ignored"]
fn check_human_mode_contains_verdict() {
    let harness = DaemonHarness::new();
    let output = harness.run_cli(&["check"]);

    // check may return 0 (pass), 1 (fail), or 2 (error) but should not be usage error
    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "check should return 0 or 1 (not usage error). exit={}, stderr: {}",
        code,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Human mode should contain verdict
    assert!(
        stdout.contains("Verdict:") || stdout.contains("PASS") || stdout.contains("FAIL"),
        "Human output should contain verdict. stdout:\n{}",
        stdout
    );

    // Should NOT contain JSON envelope
    assert!(
        !stdout.contains(r#""command":"#),
        "Human output should not contain JSON 'command' field. stdout:\n{}",
        stdout
    );
}

#[test]
#[ignore = "requires rmapd binary and Unix socket permission - run with --ignored"]
fn check_json_mode_returns_valid_envelope() {
    let harness = DaemonHarness::new();
    let output = harness.run_cli(&["check", "--json"]);

    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "check --json should return 0 or 1. exit={}, stderr: {}",
        code,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Should have envelope fields
    assert!(
        parsed.get("command").is_some(),
        "JSON output should have 'command' field"
    );
    assert!(
        parsed.get("schema").is_some(),
        "JSON output should have 'schema' field"
    );
}

// ═══════════���═══════════════════════════════���══════════════════════��═══════════
// EXPLAIN OUTPUT MODE TESTS
// ═══════════════════════════���═════════════════════════���════════════════════════

#[test]
#[ignore = "requires rmapd binary and Unix socket permission - run with --ignored"]
fn explain_human_mode_contains_target() {
    let harness = DaemonHarness::new();
    let output = harness.run_cli(&["explain", "main.ts"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "explain should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Human mode should contain target info
    assert!(
        stdout.contains("Target:") || stdout.contains("main.ts"),
        "Human output should identify target. stdout:\n{}",
        stdout
    );

    // Should NOT contain JSON envelope
    assert!(
        !stdout.contains(r#""command":"#),
        "Human output should not contain JSON 'command' field. stdout:\n{}",
        stdout
    );
    assert!(
        !stdout.contains(r#""schema":"#),
        "Human output should not contain 'schema' field. stdout:\n{}",
        stdout
    );
}

#[test]
#[ignore = "requires rmapd binary and Unix socket permission - run with --ignored"]
fn explain_json_mode_returns_valid_envelope() {
    let harness = DaemonHarness::new();
    let output = harness.run_cli(&["explain", "main.ts", "--json"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "explain --json should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Should have envelope fields (the CLI extracts "result" from daemon response)
    assert!(
        parsed.get("command").is_some(),
        "JSON output should have 'command' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("schema").is_some(),
        "JSON output should have 'schema' field. stdout:\n{}",
        stdout
    );
    // Explain-specific fields
    assert!(
        parsed.get("focus").is_some(),
        "JSON output should have 'focus' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("signals").is_some(),
        "JSON output should have 'signals' field. stdout:\n{}",
        stdout
    );
}
