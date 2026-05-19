//! CLI-level output mode tests for CLI-OUT-3 Graph Drilldown.
//!
//! Tests that verify the CLI binary produces correct output for:
//! - `rmap callers <symbol>` (human and --json)
//! - `rmap callees <symbol>` (human and --json)
//! - `rmap path <from> <to>` (human and --json)
//! - `rmap imports <file>` (human and --json)
//! - Ambiguous symbol error formatting
//!
//! # Test Strategy
//!
//! Same as `cli_output_mode.rs`: real daemon, real CLI binary, isolated temp state.
//!
//! # Repo Structure
//!
//! Uses a multi-file TypeScript repo with function calls and imports:
//! - main.ts: imports helper.ts, calls helperFunction
//! - helper.ts: exports helperFunction
//! - ambig_a.ts: exports process()
//! - ambig_b.ts: exports process() (duplicate for ambiguity test)
//!
//! # Running
//!
//! ```
//! cargo build -p rmapd
//! cargo test -p repo-graph-rgr --test cli_out_3_drilldown -- --ignored
//! ```
//!
//! # Technical Debt
//!
//! **TD-CLI-OUT-3-A: Manual pre-build requirement**
//!
//! Same as TD-CLI-OUT-1-A in `cli_output_mode.rs`. These tests require `rmapd`
//! to be built before running. They are marked `#[ignore]` and run opt-in.
//!
//! This is consistent with the existing CLI integration test pattern. The tests
//! exist as proof surface but are not part of the default `cargo test` path.
//!
//! **TD-CLI-OUT-3-B: Unix socket permission requirement**
//!
//! Same as TD-CLI-OUT-1-B. These tests require Unix socket bind permission.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use tempfile::{tempdir, TempDir};

fn rmap_binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

fn rmapd_binary_path() -> PathBuf {
    let rmap_path = PathBuf::from(env!("CARGO_BIN_EXE_rmap"));
    let parent = rmap_path
        .parent()
        .expect("rmap binary should have parent dir");
    let rmapd_path = parent.join("rmapd");

    if !rmapd_path.exists() {
        panic!(
            "rmapd binary not found at {:?}. Run `cargo build -p rmapd` first.",
            rmapd_path
        );
    }

    rmapd_path
}

/// Test harness with a multi-file TypeScript repo for graph drilldown tests.
struct DrilldownHarness {
    socket_path: PathBuf,
    state_root: PathBuf,
    daemon_process: Option<Child>,
    _state_temp: TempDir,
    _repo_temp: TempDir,
    repo_path: PathBuf,
}

impl DrilldownHarness {
    fn new() -> Self {
        let state_temp = tempdir().expect("failed to create state temp dir");
        let repo_temp = tempdir().expect("failed to create repo temp dir");

        let repo_path = repo_temp.path().join("drilldown-repo");
        std::fs::create_dir(&repo_path).unwrap();

        // main.ts: imports and calls helper
        std::fs::write(
            repo_path.join("main.ts"),
            r#"
import { helperFunction } from './helper';

export function mainEntry(): void {
    helperFunction();
}

export function standalone(): void {
    console.log('no calls');
}
"#,
        )
        .unwrap();

        // helper.ts: exports helperFunction
        std::fs::write(
            repo_path.join("helper.ts"),
            r#"
export function helperFunction(): void {
    console.log('helper called');
}
"#,
        )
        .unwrap();

        // ambig_a.ts: exports process() - for ambiguity test
        std::fs::write(
            repo_path.join("ambig_a.ts"),
            r#"
export function process(): void {
    console.log('process A');
}
"#,
        )
        .unwrap();

        // ambig_b.ts: exports process() - duplicate name
        std::fs::write(
            repo_path.join("ambig_b.ts"),
            r#"
export function process(): void {
    console.log('process B');
}
"#,
        )
        .unwrap();

        let socket_path = state_temp.path().join("daemon.sock");
        let state_root = state_temp.path().to_path_buf();

        let mut daemon_process = Command::new(rmapd_binary_path())
            .env("RMAP_SOCKET_PATH", &socket_path)
            .env("RMAP_STATE_ROOT", &state_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to start rmapd");

        // Wait for daemon
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(10);
        let mut daemon_ready = false;

        while start.elapsed() < timeout {
            if let Ok(Some(status)) = daemon_process.try_wait() {
                let mut stderr_output = String::new();
                if let Some(ref mut stderr) = daemon_process.stderr {
                    let _ = stderr.read_to_string(&mut stderr_output);
                }
                panic!(
                    "daemon exited prematurely with status {:?}\nstderr:\n{}",
                    status, stderr_output
                );
            }

            if socket_path.exists() && std::os::unix::net::UnixStream::connect(&socket_path).is_ok()
            {
                daemon_ready = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        if !daemon_ready {
            let mut stderr_output = String::new();
            if let Some(ref mut stderr) = daemon_process.stderr {
                let _ = stderr.read_to_string(&mut stderr_output);
            }
            let _ = daemon_process.kill();
            let _ = daemon_process.wait();
            panic!(
                "daemon socket not created within timeout\nstderr:\n{}",
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

        harness.index_repo();
        harness
    }

    fn index_repo(&mut self) {
        let output = Command::new(rmap_binary_path())
            .env("RMAP_SOCKET_PATH", &self.socket_path)
            .env("RMAP_STATE_ROOT", &self.state_root)
            .args(["index", self.repo_path.to_str().unwrap()])
            .output()
            .expect("failed to run index command");

        if output.status.code() != Some(0) {
            panic!(
                "index command failed.\nstderr: {}\nstdout: {}",
                String::from_utf8_lossy(&output.stderr),
                String::from_utf8_lossy(&output.stdout)
            );
        }
    }

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

impl Drop for DrilldownHarness {
    fn drop(&mut self) {
        if let Some(mut child) = self.daemon_process.take() {
            #[cfg(unix)]
            unsafe {
                libc::kill(child.id() as i32, libc::SIGTERM);
            }
            std::thread::sleep(Duration::from_millis(100));
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// CALLERS OUTPUT MODE TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn callers_human_mode_contains_structured_markers() {
    let harness = DrilldownHarness::new();
    let output = harness.run_cli(&["callers", "helperFunction"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "callers should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Human mode markers
    assert!(
        stdout.contains("Callers of"),
        "Human output should contain 'Callers of'. stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("File:"),
        "Human output should contain 'File:'. stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("found"),
        "Human output should contain count with 'found'. stdout:\n{}",
        stdout
    );

    // Should NOT contain JSON envelope
    assert!(
        !stdout.contains(r#""target":"#),
        "Human output should not contain JSON 'target' field. stdout:\n{}",
        stdout
    );
    assert!(
        !stdout.contains(r#""stable_key":"#),
        "Human output should not contain 'stable_key' field. stdout:\n{}",
        stdout
    );
}

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn callers_json_mode_returns_valid_envelope() {
    let harness = DrilldownHarness::new();
    let output = harness.run_cli(&["callers", "helperFunction", "--json"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "callers --json should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Required fields for machine parsing
    assert!(
        parsed.get("target").is_some(),
        "JSON should have 'target' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("callers").is_some(),
        "JSON should have 'callers' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("count").is_some(),
        "JSON should have 'count' field. stdout:\n{}",
        stdout
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// CALLEES OUTPUT MODE TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn callees_human_mode_contains_structured_markers() {
    let harness = DrilldownHarness::new();
    let output = harness.run_cli(&["callees", "mainEntry"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "callees should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Human mode markers
    assert!(
        stdout.contains("Callees of"),
        "Human output should contain 'Callees of'. stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("File:"),
        "Human output should contain 'File:'. stdout:\n{}",
        stdout
    );

    // Should NOT contain JSON envelope
    assert!(
        !stdout.contains(r#""target":"#),
        "Human output should not contain JSON 'target' field. stdout:\n{}",
        stdout
    );
}

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn callees_json_mode_returns_valid_envelope() {
    let harness = DrilldownHarness::new();
    let output = harness.run_cli(&["callees", "mainEntry", "--json"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "callees --json should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    assert!(
        parsed.get("target").is_some(),
        "JSON should have 'target' field"
    );
    assert!(
        parsed.get("callees").is_some(),
        "JSON should have 'callees' field"
    );
    assert!(
        parsed.get("count").is_some(),
        "JSON should have 'count' field"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// PATH OUTPUT MODE TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn path_human_mode_shows_route() {
    let harness = DrilldownHarness::new();
    let output = harness.run_cli(&["path", "mainEntry", "helperFunction"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "path should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Human mode markers
    assert!(
        stdout.contains("Path:"),
        "Human output should contain 'Path:'. stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("->"),
        "Human output should contain '->' arrow. stdout:\n{}",
        stdout
    );

    // Should NOT contain JSON envelope
    assert!(
        !stdout.contains(r#""path_length":"#),
        "Human output should not contain JSON 'path_length' field. stdout:\n{}",
        stdout
    );
}

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn path_not_found_preserves_query_terms_in_human_output() {
    let harness = DrilldownHarness::new();
    // Query in reverse direction - no path should exist
    let output = harness.run_cli(&["path", "helperFunction", "mainEntry"]);

    // May return 0 (with "No path found") or success - both are valid
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Key test: query terms preserved in header even when path not found
    assert!(
        stdout.contains("helperFunction") && stdout.contains("mainEntry"),
        "Header should preserve query terms even when no path. stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Path:"),
        "Should still have Path: header. stdout:\n{}",
        stdout
    );
}

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn path_json_mode_returns_valid_envelope() {
    let harness = DrilldownHarness::new();
    let output = harness.run_cli(&["path", "mainEntry", "helperFunction", "--json"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "path --json should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    assert!(
        parsed.get("path").is_some(),
        "JSON should have 'path' field"
    );
    assert!(
        parsed.get("found").is_some(),
        "JSON should have 'found' field"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// IMPORTS OUTPUT MODE TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn imports_human_mode_shows_file_imports() {
    let harness = DrilldownHarness::new();
    let output = harness.run_cli(&["imports", "main.ts"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "imports should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Human mode markers
    assert!(
        stdout.contains("Imports:"),
        "Human output should contain 'Imports:'. stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("import"),
        "Human output should contain import count. stdout:\n{}",
        stdout
    );

    // Should NOT contain JSON envelope
    assert!(
        !stdout.contains(r#""node_id":"#),
        "Human output should not contain JSON 'node_id' field. stdout:\n{}",
        stdout
    );
}

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn imports_json_mode_returns_valid_envelope() {
    let harness = DrilldownHarness::new();
    let output = harness.run_cli(&["imports", "main.ts", "--json"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "imports --json should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    assert!(
        parsed.get("file").is_some(),
        "JSON should have 'file' field"
    );
    assert!(
        parsed.get("imports").is_some(),
        "JSON should have 'imports' field"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// AMBIGUOUS SYMBOL ERROR RENDERING
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn ambiguous_symbol_renders_numbered_list_with_hint() {
    let harness = DrilldownHarness::new();
    // "process" exists in both ambig_a.ts and ambig_b.ts
    let output = harness.run_cli(&["callers", "process"]);

    // Should fail with exit code 2 (runtime error)
    assert_eq!(
        output.status.code(),
        Some(2),
        "ambiguous symbol should return exit code 2. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should show error header
    assert!(
        stderr.contains("ambiguous"),
        "Error should mention 'ambiguous'. stderr:\n{}",
        stderr
    );

    // Should show numbered matches
    assert!(
        stderr.contains("1.") && stderr.contains("2."),
        "Should show numbered match list. stderr:\n{}",
        stderr
    );

    // Should show hint
    assert!(
        stderr.contains("hint:"),
        "Should show hint for resolution. stderr:\n{}",
        stderr
    );

    // Should NOT be JSON (human error mode)
    assert!(
        !stderr.contains(r#""code":"#),
        "Error should not be JSON formatted. stderr:\n{}",
        stderr
    );
}
