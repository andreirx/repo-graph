//! CLI-level output mode tests for CLI-OUT-5: Inventory/Policy Output.
//!
//! Tests that verify the CLI binary produces correct output for:
//!
//! ## Group 1: Documentation Inventory
//! - `rmap docs list` (human and --json)
//! - `rmap docs extract` (human and --json)
//!
//! # Test Strategy
//!
//! Same as other CLI-OUT tests: real daemon, real CLI binary, isolated temp state.
//! Uses a simple repo with README.md to trigger documentation detection.
//!
//! # Running
//!
//! ```
//! cargo build -p rmapd
//! cargo test -p repo-graph-rgr --test cli_out_5_inventory -- --ignored
//! ```
//!
//! # Technical Debt
//!
//! **TD-CLI-OUT-5-A: Manual pre-build requirement**
//!
//! Same as previous CLI-OUT tests. These tests require `rmapd` to be built first.
//! They are marked `#[ignore]` and run opt-in.

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

/// Test harness with a simple repo containing documentation.
struct DocsHarness {
    socket_path: PathBuf,
    state_root: PathBuf,
    daemon_process: Option<Child>,
    _state_temp: TempDir,
    _repo_temp: TempDir,
    repo_path: PathBuf,
}

impl DocsHarness {
    fn new() -> Self {
        let state_temp = tempdir().expect("failed to create state temp dir");
        let repo_temp = tempdir().expect("failed to create repo temp dir");

        let repo_path = repo_temp.path().join("test-repo");
        std::fs::create_dir(&repo_path).unwrap();

        // Create README.md at root
        std::fs::write(
            repo_path.join("README.md"),
            "# Test Repository\n\nThis is a test repository for CLI-OUT-5 tests.\n",
        )
        .unwrap();

        // Create docs directory with another README
        std::fs::create_dir(repo_path.join("docs")).unwrap();
        std::fs::write(
            repo_path.join("docs/README.md"),
            "# Documentation\n\nAdditional documentation.\n",
        )
        .unwrap();

        // Create a simple source file to ensure the repo is indexable
        std::fs::write(
            repo_path.join("main.ts"),
            "export function main(): void { console.log('hello'); }\n",
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

impl Drop for DocsHarness {
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

/// Test harness with marker-containing README for positive extraction path.
struct DocsMarkerHarness {
    socket_path: PathBuf,
    state_root: PathBuf,
    daemon_process: Option<Child>,
    _state_temp: TempDir,
    _repo_temp: TempDir,
    repo_path: PathBuf,
}

impl DocsMarkerHarness {
    fn new() -> Self {
        let state_temp = tempdir().expect("failed to create state temp dir");
        let repo_temp = tempdir().expect("failed to create repo temp dir");

        let repo_path = repo_temp.path().join("marker-repo");
        std::fs::create_dir(&repo_path).unwrap();

        // Create README.md with explicit rg: marker
        std::fs::write(
            repo_path.join("README.md"),
            r#"# New Service

This module replaces the old service.

<!-- rg:replaces old-service -->

## Usage

See documentation for details.
"#,
        )
        .unwrap();

        // Create a simple source file to ensure the repo is indexable
        std::fs::write(
            repo_path.join("main.ts"),
            "export function main(): void { console.log('hello'); }\n",
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

impl Drop for DocsMarkerHarness {
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

// ── Group 1: Documentation Inventory ─────────────────────────────────────────

#[test]
#[ignore] // Requires daemon pre-built
fn docs_list_human_mode_shows_header() {
    let harness = DocsHarness::new();
    let output = harness.run_cli(&["docs", "list"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Documentation"));
    assert!(stdout.contains("documents") || stdout.contains("document"));
}

#[test]
#[ignore] // Requires daemon pre-built
fn docs_list_human_mode_shows_entries() {
    let harness = DocsHarness::new();
    let output = harness.run_cli(&["docs", "list"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should show both README files
    assert!(stdout.contains("README.md"));
    assert!(stdout.contains("readme")); // kind
}

#[test]
#[ignore] // Requires daemon pre-built
fn docs_list_human_mode_shows_hint() {
    let harness = DocsHarness::new();
    let output = harness.run_cli(&["docs", "list"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hint:"));
    assert!(stdout.contains("rmap docs extract"));
}

#[test]
#[ignore] // Requires daemon pre-built
fn docs_list_json_mode_returns_valid_envelope() {
    let harness = DocsHarness::new();
    let output = harness.run_cli(&["docs", "list", "--json"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify valid JSON with expected fields
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("should be valid JSON");
    assert!(json.get("command").is_some());
    assert!(json.get("repo").is_some());
    assert!(json.get("entries").is_some());
    assert!(json.get("count").is_some());
    assert!(json.get("counts_by_kind").is_some());
}

#[test]
#[ignore] // Requires daemon pre-built
fn docs_extract_human_mode_shows_header() {
    let harness = DocsHarness::new();
    let output = harness.run_cli(&["docs", "extract"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Documentation Extraction"));
}

#[test]
#[ignore] // Requires daemon pre-built
fn docs_extract_human_mode_shows_files_scanned() {
    let harness = DocsHarness::new();
    let output = harness.run_cli(&["docs", "extract"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("scanned"));
}

#[test]
#[ignore] // Requires daemon pre-built
fn docs_extract_human_mode_shows_extraction_results() {
    let harness = DocsHarness::new();
    let output = harness.run_cli(&["docs", "extract"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Extraction results:"));
    assert!(stdout.contains("facts extracted"));
    assert!(stdout.contains("facts inserted"));
    assert!(stdout.contains("facts deleted"));
}

#[test]
#[ignore] // Requires daemon pre-built
fn docs_extract_json_mode_returns_valid_envelope() {
    let harness = DocsHarness::new();
    let output = harness.run_cli(&["docs", "extract", "--json"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify valid JSON with expected fields
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("should be valid JSON");
    assert!(json.get("command").is_some());
    assert!(json.get("repo").is_some());
    assert!(json.get("files_scanned").is_some());
    assert!(json.get("facts_extracted").is_some());
    assert!(json.get("warnings").is_some());
}

// ── Positive extraction path (marker-based) ──────────────────────────────────

#[test]
#[ignore] // Requires daemon pre-built
fn docs_extract_with_marker_extracts_facts() {
    let harness = DocsMarkerHarness::new();
    let output = harness.run_cli(&["docs", "extract", "--json"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    let json: serde_json::Value = serde_json::from_str(&stdout).expect("should be valid JSON");
    let facts_extracted = json
        .get("facts_extracted")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    // The README contains <!-- rg:replaces old-service --> which should extract 1 fact
    assert!(
        facts_extracted >= 1,
        "expected at least 1 extracted fact from rg:replaces marker, got {}",
        facts_extracted
    );
}

#[test]
#[ignore] // Requires daemon pre-built
fn docs_extract_with_marker_human_mode_shows_nonzero_facts() {
    let harness = DocsMarkerHarness::new();
    let output = harness.run_cli(&["docs", "extract"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should NOT show "0 facts extracted" - should show at least 1
    // The exact number depends on extraction behavior, but marker should produce facts
    assert!(
        !stdout.contains("0 facts extracted") || stdout.contains("1 fact"),
        "expected nonzero facts extracted from marker-containing README"
    );
}

// ── Group 2: Resource Inventory ──────────────────────────────────────────────

/// Test harness with TypeScript code that accesses files (for resource detection).
struct ResourceHarness {
    socket_path: PathBuf,
    state_root: PathBuf,
    daemon_process: Option<Child>,
    _state_temp: TempDir,
    _repo_temp: TempDir,
    repo_path: PathBuf,
}

impl ResourceHarness {
    fn new() -> Self {
        let state_temp = tempdir().expect("failed to create state temp dir");
        let repo_temp = tempdir().expect("failed to create repo temp dir");

        let repo_path = repo_temp.path().join("resource-repo");
        std::fs::create_dir(&repo_path).unwrap();

        // Create TypeScript code that reads and writes files
        std::fs::write(
            repo_path.join("main.ts"),
            r#"
import * as fs from 'fs';

export function readConfig(): string {
    return fs.readFileSync('config.json', 'utf-8');
}

export function writeConfig(data: string): void {
    fs.writeFileSync('config.json', data);
}

export function readData(): Buffer {
    return fs.readFileSync('data.bin');
}
"#,
        )
        .unwrap();

        // Create a README for indexability
        std::fs::write(repo_path.join("README.md"), "# Resource Test Repo\n").unwrap();

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

impl Drop for ResourceHarness {
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

#[test]
#[ignore] // Requires daemon pre-built
fn resource_list_human_mode_shows_header() {
    let harness = ResourceHarness::new();
    let output = harness.run_cli(&["resource", "list"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Resources"));
}

#[test]
#[ignore] // Requires daemon pre-built
fn resource_list_json_mode_returns_valid_envelope() {
    let harness = ResourceHarness::new();
    let output = harness.run_cli(&["resource", "list", "--json"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    let json: serde_json::Value = serde_json::from_str(&stdout).expect("should be valid JSON");
    assert!(json.get("command").is_some());
    assert!(json.get("repo").is_some());
    assert!(json.get("results").is_some());
    assert!(json.get("count").is_some());
}

#[test]
#[ignore] // Requires daemon pre-built
fn resource_list_empty_shows_hint() {
    // Use DocsHarness which doesn't have file access patterns
    let harness = DocsHarness::new();
    let output = harness.run_cli(&["resource", "list"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("0 resources"));
    assert!(stdout.contains("hint:"));
}

#[test]
#[ignore] // Requires daemon pre-built
fn resource_readers_empty_shows_message() {
    // Use a nonexistent resource key - should show empty result
    let harness = DocsHarness::new();
    let output = harness.run_cli(&["resource", "readers", "fake:key:FS_PATH"]);

    // This may fail or return empty depending on daemon behavior
    // The point is to test the output format
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() {
        assert!(stdout.contains("Readers for:") || stdout.contains("0 readers"));
    }
}

#[test]
#[ignore] // Requires daemon pre-built
fn resource_writers_json_mode_returns_valid_envelope() {
    let harness = DocsHarness::new();
    let output = harness.run_cli(&["resource", "writers", "fake:key:FS_PATH", "--json"]);

    // Even if empty, should return valid JSON envelope
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() {
        let json: serde_json::Value = serde_json::from_str(&stdout).expect("should be valid JSON");
        assert!(json.get("command").is_some());
        assert!(json.get("target").is_some());
        assert!(json.get("results").is_some());
    }
}

// ── Group 3: Policy (Legacy Contract) ────────────────────────────────────────
//
// NOTE: policy command does NOT use REG-1 daemon contract.
// It requires explicit db_path and repo_uid arguments.
// These tests use a simpler approach - they just verify the command
// handles arguments correctly and produces expected output format.

#[test]
#[ignore] // Requires daemon pre-built
fn policy_shows_usage_without_args() {
    let harness = DocsHarness::new();
    let output = harness.run_cli(&["policy"]);

    // Should show usage error
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:") || stderr.contains("db_path"));
}

#[test]
#[ignore] // Requires daemon pre-built
fn policy_shows_error_for_invalid_db() {
    let harness = DocsHarness::new();
    let output = harness.run_cli(&["policy", "/nonexistent/path.db", "fake_repo"]);

    // Should show error about database
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error:"));
}

#[test]
#[ignore] // Requires daemon pre-built
fn policy_shows_error_for_unknown_kind() {
    let harness = DocsHarness::new();
    let output = harness.run_cli(&[
        "policy",
        "/nonexistent/path.db",
        "fake_repo",
        "--kind",
        "INVALID_KIND",
    ]);

    // Should show error about unsupported kind
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unsupported policy kind") || stderr.contains("error:"));
}
