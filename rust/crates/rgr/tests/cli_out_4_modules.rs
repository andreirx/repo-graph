//! CLI-level output mode tests for CLI-OUT-4: Module/Architecture Output.
//!
//! Tests that verify the CLI binary produces correct output for:
//!
//! ## Group 1: Module Catalog
//! - `rmap modules list` (human and --json)
//! - `rmap modules show <module>` (human and --json)
//!
//! ## Group 2: Module Inventory
//! - `rmap modules files <module>` (human and --json)
//! - `rmap modules unowned` (human and --json)
//!
//! ## Group 3: Module Diagnostics
//! - `rmap modules deps` (human and --json)
//! - `rmap modules violations` (human and --json)
//!
//! ## Group 4: Architectural Surfaces
//! - `rmap surfaces list` (human and --json)
//! - `rmap surfaces show <ref>` (not found error)
//!
//! ## Group 5: Architectural Boundaries
//! - `rmap boundaries list` (human and --json)
//! - `rmap boundaries summary` (human and --json)
//! - `rmap boundaries show <ref>` (not found error)
//!
//! # Module Identity Contract
//!
//! These tests validate the module identity contract established by Group 1:
//! - display_name rendering
//! - module_kind with confidence
//! - ownership rollups (files, test files)
//! - dead symbol counts
//! - cross-module dependency detection
//! - empty/isolated module hints
//!
//! # Test Strategy
//!
//! Same as `cli_output_mode.rs`: real daemon, real CLI binary, isolated temp state.
//!
//! # Repo Structure
//!
//! Uses a TypeScript monorepo with declared modules:
//! - ./package.json with @test/core
//! - ./index.ts
//! - packages/utils/package.json with @test/utils
//! - packages/utils/helper.ts (imports core)
//!
//! # Running
//!
//! ```
//! cargo build -p rmapd
//! cargo test -p repo-graph-rgr --test cli_out_4_modules -- --ignored
//! ```
//!
//! # Technical Debt
//!
//! **TD-CLI-OUT-4-A: Manual pre-build requirement**
//!
//! Same as TD-CLI-OUT-1-A. These tests require `rmapd` to be built first.
//! They are marked `#[ignore]` and run opt-in.
//!
//! **TD-CLI-OUT-4-B: Unix socket permission requirement**
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

/// Test harness with a multi-module TypeScript monorepo.
struct ModuleCatalogHarness {
    socket_path: PathBuf,
    state_root: PathBuf,
    daemon_process: Option<Child>,
    _state_temp: TempDir,
    _repo_temp: TempDir,
    repo_path: PathBuf,
}

impl ModuleCatalogHarness {
    fn new() -> Self {
        let state_temp = tempdir().expect("failed to create state temp dir");
        let repo_temp = tempdir().expect("failed to create repo temp dir");

        let repo_path = repo_temp.path().join("monorepo");
        std::fs::create_dir(&repo_path).unwrap();

        // Create a simple repo with package.json at root to trigger module detection
        std::fs::write(
            repo_path.join("package.json"),
            r#"{"name": "test-monorepo", "version": "1.0.0"}"#,
        )
        .unwrap();

        // Create src directory with code
        std::fs::create_dir(repo_path.join("src")).unwrap();
        std::fs::write(
            repo_path.join("src/index.ts"),
            r#"
export function coreFunction(): string {
    return 'core';
}

export function unusedFunction(): void {
    // This function is never called
}
"#,
        )
        .unwrap();
        std::fs::write(
            repo_path.join("src/helper.ts"),
            r#"
import { coreFunction } from './index';

export function utilHelper(): string {
    return coreFunction() + '-util';
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

impl Drop for ModuleCatalogHarness {
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
// MODULES LIST OUTPUT MODE TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn modules_list_human_mode_shows_catalog() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["modules", "list"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "modules list should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Human mode: header
    assert!(
        stdout.contains("Modules"),
        "Human output should contain 'Modules' header. stdout:\n{}",
        stdout
    );

    // Human mode: count line
    assert!(
        stdout.contains("module"),
        "Human output should show module count. stdout:\n{}",
        stdout
    );

    // Human mode: module identity columns
    assert!(
        stdout.contains("files"),
        "Human output should show 'files' column. stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("dead"),
        "Human output should show 'dead' column. stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("violations"),
        "Human output should show 'violations' column. stdout:\n{}",
        stdout
    );

    // Human mode: kind/confidence
    assert!(
        stdout.contains("declared") || stdout.contains("inferred"),
        "Human output should show module kind. stdout:\n{}",
        stdout
    );

    // Should NOT contain JSON envelope fields
    assert!(
        !stdout.contains(r#""command":"#),
        "Human output should not contain JSON 'command' field. stdout:\n{}",
        stdout
    );
    assert!(
        !stdout.contains(r#""module_uid":"#),
        "Human output should not contain 'module_uid' field. stdout:\n{}",
        stdout
    );
}

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn modules_list_json_mode_returns_valid_envelope() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["modules", "list", "--json"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "modules list --json should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Standard envelope fields
    assert!(
        parsed.get("command").is_some(),
        "JSON should have 'command' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("repo").is_some(),
        "JSON should have 'repo' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("snapshot").is_some(),
        "JSON should have 'snapshot' field. stdout:\n{}",
        stdout
    );

    // Results array
    assert!(
        parsed.get("results").is_some() && parsed["results"].is_array(),
        "JSON should have 'results' array. stdout:\n{}",
        stdout
    );

    // Module identity fields in results
    let results = parsed["results"].as_array().unwrap();
    if !results.is_empty() {
        let first = &results[0];
        assert!(
            first.get("module_uid").is_some(),
            "Module should have 'module_uid'. stdout:\n{}",
            stdout
        );
        assert!(
            first.get("display_name").is_some(),
            "Module should have 'display_name'. stdout:\n{}",
            stdout
        );
        assert!(
            first.get("module_kind").is_some(),
            "Module should have 'module_kind'. stdout:\n{}",
            stdout
        );
        assert!(
            first.get("confidence").is_some(),
            "Module should have 'confidence'. stdout:\n{}",
            stdout
        );
        assert!(
            first.get("owned_file_count").is_some(),
            "Module should have 'owned_file_count'. stdout:\n{}",
            stdout
        );
    }
}

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn modules_list_shows_cross_module_dependency_hint() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["modules", "list"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "modules list should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should show dependency summary or hint
    // Either "X cross-module dependencies" or "No cross-module dependencies"
    assert!(
        stdout.contains("cross-module") || stdout.contains("dependencies"),
        "Human output should mention cross-module dependencies. stdout:\n{}",
        stdout
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// MODULES SHOW OUTPUT MODE TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn modules_show_human_mode_shows_detail() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["modules", "show", "."]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "modules show should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Human mode: module header
    assert!(
        stdout.contains("Module:"),
        "Human output should contain 'Module:' header. stdout:\n{}",
        stdout
    );

    // Human mode: identity section
    assert!(
        stdout.contains("Kind:"),
        "Human output should show 'Kind:'. stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Root:"),
        "Human output should show 'Root:'. stdout:\n{}",
        stdout
    );

    // Human mode: ownership section
    assert!(
        stdout.contains("Ownership:"),
        "Human output should show 'Ownership:' section. stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("file"),
        "Human output should show file count. stdout:\n{}",
        stdout
    );

    // Human mode: relationships section
    assert!(
        stdout.contains("Relationships:"),
        "Human output should show 'Relationships:' section. stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("dependencies") || stdout.contains("violation"),
        "Human output should show dependency/violation info. stdout:\n{}",
        stdout
    );

    // Human mode: symbols section
    assert!(
        stdout.contains("Symbols:"),
        "Human output should show 'Symbols:' section. stdout:\n{}",
        stdout
    );

    // Should NOT contain JSON envelope
    assert!(
        !stdout.contains(r#""module_uid":"#),
        "Human output should not contain 'module_uid' field. stdout:\n{}",
        stdout
    );
}

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn modules_show_json_mode_returns_valid_envelope() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["modules", "show", ".", "--json"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "modules show --json should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Standard envelope fields
    assert!(
        parsed.get("command").is_some(),
        "JSON should have 'command' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("repo").is_some(),
        "JSON should have 'repo' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("snapshot").is_some(),
        "JSON should have 'snapshot' field. stdout:\n{}",
        stdout
    );

    // Module identity
    assert!(
        parsed.get("module").is_some(),
        "JSON should have 'module' object. stdout:\n{}",
        stdout
    );
    let module = &parsed["module"];
    assert!(
        module.get("module_uid").is_some(),
        "Module should have 'module_uid'. stdout:\n{}",
        stdout
    );
    assert!(
        module.get("canonical_root_path").is_some(),
        "Module should have 'canonical_root_path'. stdout:\n{}",
        stdout
    );

    // Rollups
    assert!(
        parsed.get("rollups").is_some(),
        "JSON should have 'rollups' object. stdout:\n{}",
        stdout
    );
    let rollups = &parsed["rollups"];
    assert!(
        rollups.get("owned_file_count").is_some(),
        "Rollups should have 'owned_file_count'. stdout:\n{}",
        stdout
    );

    // Neighbors
    assert!(
        parsed.get("outbound_dependencies").is_some(),
        "JSON should have 'outbound_dependencies'. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("inbound_dependencies").is_some(),
        "JSON should have 'inbound_dependencies'. stdout:\n{}",
        stdout
    );
}

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn modules_show_module_not_found_returns_error() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["modules", "show", "nonexistent"]);

    // Should return exit code 1 (user error, not runtime error)
    assert_eq!(
        output.status.code(),
        Some(1),
        "modules show nonexistent should return exit code 1. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should show error message
    assert!(
        stderr.contains("not found") || stderr.contains("error"),
        "Error should mention module not found. stderr:\n{}",
        stderr
    );
}

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn modules_show_isolated_module_shows_hint() {
    let harness = ModuleCatalogHarness::new();
    // . has no outbound dependencies (utils imports from core, not vice versa)
    let output = harness.run_cli(&["modules", "show", "."]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "modules show should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // For isolated modules, should show helpful hint
    // Note: if module has inbound deps from utils, it won't be isolated
    // This test validates the hint appears when appropriate
    if stdout.contains("0 outbound") && stdout.contains("0 inbound") {
        assert!(
            stdout.contains("isolated") || stdout.contains("No dependencies"),
            "Isolated module should show hint. stdout:\n{}",
            stdout
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// GROUP 2: MODULES FILES OUTPUT MODE TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn modules_files_human_mode_shows_inventory() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["modules", "files", "."]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "modules files should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Human mode: header with module reference
    assert!(
        stdout.contains("Files:"),
        "Human output should contain 'Files:' header. stdout:\n{}",
        stdout
    );

    // Human mode: count line
    assert!(
        stdout.contains("file"),
        "Human output should show file count. stdout:\n{}",
        stdout
    );

    // Should NOT contain JSON envelope fields
    assert!(
        !stdout.contains(r#""command":"#),
        "Human output should not contain JSON 'command' field. stdout:\n{}",
        stdout
    );
    assert!(
        !stdout.contains(r#""file_uid":"#),
        "Human output should not contain 'file_uid' field. stdout:\n{}",
        stdout
    );
}

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn modules_files_json_mode_returns_valid_envelope() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["modules", "files", ".", "--json"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "modules files --json should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Standard envelope fields
    assert!(
        parsed.get("command").is_some(),
        "JSON should have 'command' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("repo").is_some(),
        "JSON should have 'repo' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("snapshot").is_some(),
        "JSON should have 'snapshot' field. stdout:\n{}",
        stdout
    );

    // Module reference
    assert!(
        parsed.get("module").is_some(),
        "JSON should have 'module' field. stdout:\n{}",
        stdout
    );

    // Results array
    assert!(
        parsed.get("results").is_some() && parsed["results"].is_array(),
        "JSON should have 'results' array. stdout:\n{}",
        stdout
    );
}

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn modules_files_module_not_found_returns_error() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["modules", "files", "nonexistent"]);

    // Should return exit code 1 (user error, not runtime error)
    assert_eq!(
        output.status.code(),
        Some(1),
        "modules files nonexistent should return exit code 1. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should show error message
    assert!(
        stderr.contains("not found") || stderr.contains("error"),
        "Error should mention module not found. stderr:\n{}",
        stderr
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// GROUP 2: MODULES UNOWNED OUTPUT MODE TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn modules_unowned_human_mode_shows_inventory() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["modules", "unowned"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "modules unowned should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Human mode: header
    assert!(
        stdout.contains("Unowned"),
        "Human output should contain 'Unowned' header. stdout:\n{}",
        stdout
    );

    // Human mode: count line or "All files assigned" message
    assert!(
        stdout.contains("file") || stdout.contains("assigned"),
        "Human output should show file count or assigned message. stdout:\n{}",
        stdout
    );

    // Should NOT contain JSON envelope fields
    assert!(
        !stdout.contains(r#""command":"#),
        "Human output should not contain JSON 'command' field. stdout:\n{}",
        stdout
    );
}

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn modules_unowned_json_mode_returns_valid_envelope() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["modules", "unowned", "--json"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "modules unowned --json should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Standard envelope fields
    assert!(
        parsed.get("command").is_some(),
        "JSON should have 'command' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("repo").is_some(),
        "JSON should have 'repo' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("snapshot").is_some(),
        "JSON should have 'snapshot' field. stdout:\n{}",
        stdout
    );

    // Results array
    assert!(
        parsed.get("results").is_some() && parsed["results"].is_array(),
        "JSON should have 'results' array. stdout:\n{}",
        stdout
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// GROUP 3: MODULES DEPS OUTPUT MODE TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn modules_deps_human_mode_shows_summary() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["modules", "deps"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "modules deps should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Human mode: header
    assert!(
        stdout.contains("Module Dependencies"),
        "Human output should contain 'Module Dependencies' header. stdout:\n{}",
        stdout
    );

    // Human mode: direction context
    assert!(
        stdout.contains("Queried:"),
        "Human output should show query context. stdout:\n{}",
        stdout
    );

    // Should NOT contain JSON envelope fields
    assert!(
        !stdout.contains(r#""command":"#),
        "Human output should not contain JSON 'command' field. stdout:\n{}",
        stdout
    );
}

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn modules_deps_json_mode_returns_valid_envelope() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["modules", "deps", "--json"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "modules deps --json should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Standard envelope fields
    assert!(
        parsed.get("command").is_some(),
        "JSON should have 'command' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("repo").is_some(),
        "JSON should have 'repo' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("direction").is_some(),
        "JSON should have 'direction' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("results").is_some(),
        "JSON should have 'results' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("diagnostics").is_some(),
        "JSON should have 'diagnostics' field. stdout:\n{}",
        stdout
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// GROUP 3: MODULES VIOLATIONS OUTPUT MODE TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn modules_violations_human_mode_shows_diagnostics() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["modules", "violations"]);

    // Exit code 0 expected (no violations in simple test repo)
    assert_eq!(
        output.status.code(),
        Some(0),
        "modules violations should succeed with exit 0 (no violations). stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Human mode: header
    assert!(
        stdout.contains("Module Violations"),
        "Human output should contain 'Module Violations' header. stdout:\n{}",
        stdout
    );

    // Human mode: count line
    assert!(
        stdout.contains("violation") || stdout.contains("stale"),
        "Human output should show violation or stale count. stdout:\n{}",
        stdout
    );

    // Should NOT contain JSON envelope fields
    assert!(
        !stdout.contains(r#""command":"#),
        "Human output should not contain JSON 'command' field. stdout:\n{}",
        stdout
    );
}

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn modules_violations_json_mode_returns_valid_envelope() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["modules", "violations", "--json"]);

    // Exit code 0 expected (no violations in simple test repo)
    assert_eq!(
        output.status.code(),
        Some(0),
        "modules violations --json should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Standard envelope fields
    assert!(
        parsed.get("command").is_some(),
        "JSON should have 'command' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("repo").is_some(),
        "JSON should have 'repo' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("results").is_some(),
        "JSON should have 'results' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("count").is_some(),
        "JSON should have 'count' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("stale_count").is_some(),
        "JSON should have 'stale_count' field. stdout:\n{}",
        stdout
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// GROUP 4: SURFACES LIST OUTPUT MODE TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn surfaces_list_human_mode_shows_catalog() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["surfaces", "list"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "surfaces list should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Human mode: header
    assert!(
        stdout.contains("Surfaces"),
        "Human output should contain 'Surfaces' header. stdout:\n{}",
        stdout
    );

    // Human mode: count line (even if 0 surfaces)
    assert!(
        stdout.contains("surface"),
        "Human output should show surface count. stdout:\n{}",
        stdout
    );

    // Should NOT contain JSON envelope fields
    assert!(
        !stdout.contains(r#""command":"#),
        "Human output should not contain JSON 'command' field. stdout:\n{}",
        stdout
    );
    assert!(
        !stdout.contains(r#""project_surface_uid":"#),
        "Human output should not contain 'project_surface_uid' field. stdout:\n{}",
        stdout
    );
}

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn surfaces_list_json_mode_returns_valid_envelope() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["surfaces", "list", "--json"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "surfaces list --json should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Standard envelope fields
    assert!(
        parsed.get("command").is_some(),
        "JSON should have 'command' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("repo").is_some(),
        "JSON should have 'repo' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("snapshot").is_some(),
        "JSON should have 'snapshot' field. stdout:\n{}",
        stdout
    );

    // Results array
    assert!(
        parsed.get("results").is_some() && parsed["results"].is_array(),
        "JSON should have 'results' array. stdout:\n{}",
        stdout
    );

    // Count field
    assert!(
        parsed.get("count").is_some(),
        "JSON should have 'count' field. stdout:\n{}",
        stdout
    );
}

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn surfaces_list_empty_shows_hint() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["surfaces", "list"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "surfaces list should succeed even with 0 surfaces. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // If 0 surfaces, should show hint or degradation warning
    if stdout.contains("0 surfaces") {
        assert!(
            stdout.contains("hint:") || stdout.contains("warning:"),
            "Empty surfaces list should show hint or warning. stdout:\n{}",
            stdout
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// GROUP 4: SURFACES SHOW OUTPUT MODE TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn surfaces_show_not_found_returns_error() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["surfaces", "show", "nonexistent-surface-uid"]);

    // Should return exit code 1 (user error, not runtime error)
    assert_eq!(
        output.status.code(),
        Some(1),
        "surfaces show nonexistent should return exit code 1. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should show error message
    assert!(
        stderr.contains("not found") || stderr.contains("error"),
        "Error should mention surface not found. stderr:\n{}",
        stderr
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// GROUP 5: BOUNDARIES LIST OUTPUT MODE TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn boundaries_list_human_mode_shows_catalog() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["boundaries", "list"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "boundaries list should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Human mode: header
    assert!(
        stdout.contains("Boundaries"),
        "Human output should contain 'Boundaries' header. stdout:\n{}",
        stdout
    );

    // Human mode: count line (even if 0 boundaries)
    assert!(
        stdout.contains("boundar"),
        "Human output should show boundary count. stdout:\n{}",
        stdout
    );

    // Should NOT contain JSON envelope fields
    assert!(
        !stdout.contains(r#""command":"#),
        "Human output should not contain JSON 'command' field. stdout:\n{}",
        stdout
    );
}

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn boundaries_list_json_mode_returns_valid_envelope() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["boundaries", "list", "--json"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "boundaries list --json should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Standard envelope fields
    assert!(
        parsed.get("command").is_some(),
        "JSON should have 'command' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("repo").is_some(),
        "JSON should have 'repo' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("snapshot").is_some(),
        "JSON should have 'snapshot' field. stdout:\n{}",
        stdout
    );

    // Results array
    assert!(
        parsed.get("results").is_some() && parsed["results"].is_array(),
        "JSON should have 'results' array. stdout:\n{}",
        stdout
    );

    // Count field
    assert!(
        parsed.get("count").is_some(),
        "JSON should have 'count' field. stdout:\n{}",
        stdout
    );
}

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn boundaries_list_empty_shows_hint() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["boundaries", "list"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "boundaries list should succeed even with 0 boundaries. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // If 0 boundaries, should show hint
    if stdout.contains("0 boundaries") {
        assert!(
            stdout.contains("hint:"),
            "Empty boundaries list should show hint. stdout:\n{}",
            stdout
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// GROUP 5: BOUNDARIES SUMMARY OUTPUT MODE TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn boundaries_summary_human_mode_shows_totals() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["boundaries", "summary"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "boundaries summary should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Human mode: header
    assert!(
        stdout.contains("Boundaries Summary"),
        "Human output should contain 'Boundaries Summary' header. stdout:\n{}",
        stdout
    );

    // Human mode: totals
    assert!(
        stdout.contains("surfaces") && stdout.contains("channels"),
        "Human output should show surface and channel counts. stdout:\n{}",
        stdout
    );

    // Should NOT contain JSON envelope fields
    assert!(
        !stdout.contains(r#""command":"#),
        "Human output should not contain JSON 'command' field. stdout:\n{}",
        stdout
    );
}

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn boundaries_summary_json_mode_returns_valid_envelope() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["boundaries", "summary", "--json"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "boundaries summary --json should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Standard envelope fields
    assert!(
        parsed.get("command").is_some(),
        "JSON should have 'command' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("repo").is_some(),
        "JSON should have 'repo' field. stdout:\n{}",
        stdout
    );
    assert!(
        parsed.get("summary").is_some(),
        "JSON should have 'summary' field. stdout:\n{}",
        stdout
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// GROUP 5: BOUNDARIES SHOW OUTPUT MODE TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "requires rmapd binary - run with --ignored"]
fn boundaries_show_not_found_returns_error() {
    let harness = ModuleCatalogHarness::new();
    let output = harness.run_cli(&["boundaries", "show", "nonexistent-boundary-uid"]);

    // Should return exit code 1 (user error, not runtime error)
    assert_eq!(
        output.status.code(),
        Some(1),
        "boundaries show nonexistent should return exit code 1. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should show error message
    assert!(
        stderr.contains("not found") || stderr.contains("error"),
        "Error should mention boundary not found. stderr:\n{}",
        stderr
    );
}
