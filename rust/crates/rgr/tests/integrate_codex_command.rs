//! Integration tests for `rmap integrate codex` commands.
//!
//! Tests behavioral scenarios:
//! - install into fresh/existing config
//! - remove preserving non-repo-graph hooks
//! - dry-run non-mutation
//! - invalid JSON handling
//! - status on absent/minimal/full installs

use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn rmap_binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rmap"))
}

/// Create a temp directory with optional initial hooks.json content.
fn setup_codex_config(content: Option<&str>) -> TempDir {
    let temp = TempDir::new().unwrap();
    let codex_dir = temp.path().join(".codex");
    fs::create_dir_all(&codex_dir).unwrap();

    if let Some(json) = content {
        fs::write(codex_dir.join("hooks.json"), json).unwrap();
    }

    temp
}

// ============================================================================
// Install tests
// ============================================================================

#[test]
fn install_fresh_minimal_creates_hooks_json() {
    let temp = TempDir::new().unwrap();
    let codex_dir = temp.path().join(".codex");

    let output = rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "install", "--project"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // hooks.json should exist
    let hooks_path = codex_dir.join("hooks.json");
    assert!(hooks_path.exists(), "hooks.json should be created");

    // Should contain SessionStart and Stop
    let content = fs::read_to_string(&hooks_path).unwrap();
    assert!(content.contains("SessionStart"));
    assert!(content.contains("Stop"));
    assert!(content.contains("rmap hook"));

    // Should NOT contain full profile events
    assert!(!content.contains("UserPromptSubmit"));
    assert!(!content.contains("PostToolUse"));
}

#[test]
fn install_fresh_full_includes_all_events() {
    let temp = TempDir::new().unwrap();
    let codex_dir = temp.path().join(".codex");

    let output = rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "install", "--project", "--full"])
        .output()
        .unwrap();

    assert!(output.status.success());

    let content = fs::read_to_string(codex_dir.join("hooks.json")).unwrap();
    assert!(content.contains("SessionStart"));
    assert!(content.contains("UserPromptSubmit"));
    assert!(content.contains("PostToolUse"));
    assert!(content.contains("Stop"));
    // Codex does NOT have PreCompact
    assert!(!content.contains("PreCompact"));
}

#[test]
fn install_preserves_existing_hooks() {
    let existing = r#"{
        "hooks": {
            "SessionStart": [
                {"hooks": [{"type": "command", "command": "my-custom-hook.sh", "timeout": 5}]}
            ]
        }
    }"#;

    let temp = setup_codex_config(Some(existing));
    let hooks_path = temp.path().join(".codex/hooks.json");

    let output = rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "install", "--project"])
        .output()
        .unwrap();

    assert!(output.status.success());

    let content = fs::read_to_string(&hooks_path).unwrap();
    // Should have both repo-graph and custom hook
    assert!(content.contains("rmap hook session-start"));
    assert!(content.contains("my-custom-hook.sh"));
}

#[test]
fn install_existing_requires_force() {
    // First install
    let temp = TempDir::new().unwrap();
    rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "install", "--project"])
        .output()
        .unwrap();

    // Second install without --force should fail
    let output = rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "install", "--project"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--force") || stderr.contains("already installed"));
}

#[test]
fn install_with_force_updates_existing() {
    // First install minimal
    let temp = TempDir::new().unwrap();
    rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "install", "--project"])
        .output()
        .unwrap();

    // Second install full with --force should succeed
    let output = rmap_binary()
        .current_dir(temp.path())
        .args([
            "integrate",
            "codex",
            "install",
            "--project",
            "--full",
            "--force",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());

    let content = fs::read_to_string(temp.path().join(".codex/hooks.json")).unwrap();
    assert!(content.contains("UserPromptSubmit"));
}

#[test]
fn install_creates_backup() {
    let existing = r#"{"hooks": {}}"#;
    let temp = setup_codex_config(Some(existing));

    rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "install", "--project"])
        .output()
        .unwrap();

    // Backup should exist
    let backup_path = temp.path().join(".codex/hooks.json.rmap-backup");
    assert!(backup_path.exists(), "backup should be created");
}

// ============================================================================
// Dry-run tests
// ============================================================================

#[test]
fn dry_run_does_not_create_file() {
    let temp = TempDir::new().unwrap();
    let hooks_path = temp.path().join(".codex/hooks.json");

    let output = rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "install", "--project", "--dry-run"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(!hooks_path.exists(), "dry-run should not create file");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Dry run") || stdout.contains("dry run"));
}

#[test]
fn dry_run_does_not_modify_existing() {
    let existing = r#"{"hooks": {"Other": []}}"#;
    let temp = setup_codex_config(Some(existing));
    let hooks_path = temp.path().join(".codex/hooks.json");

    let before = fs::read_to_string(&hooks_path).unwrap();

    rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "install", "--project", "--dry-run"])
        .output()
        .unwrap();

    let after = fs::read_to_string(&hooks_path).unwrap();
    assert_eq!(before, after, "dry-run should not modify file");
}

// ============================================================================
// Remove tests
// ============================================================================

#[test]
fn remove_cleans_repo_graph_hooks() {
    // Install first
    let temp = TempDir::new().unwrap();
    rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "install", "--project"])
        .output()
        .unwrap();

    // Remove
    let output = rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "remove", "--project"])
        .output()
        .unwrap();

    assert!(output.status.success());

    let content = fs::read_to_string(temp.path().join(".codex/hooks.json")).unwrap();
    assert!(!content.contains("rmap hook"));
}

#[test]
fn remove_preserves_non_repo_graph_hooks() {
    // Create config with both repo-graph and custom hooks
    let temp = TempDir::new().unwrap();

    // Install repo-graph hooks
    rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "install", "--project"])
        .output()
        .unwrap();

    // Manually add a custom hook
    let hooks_path = temp.path().join(".codex/hooks.json");
    let content = fs::read_to_string(&hooks_path).unwrap();
    let mut parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

    // Add custom hook to SessionStart
    if let Some(hooks) = parsed.get_mut("hooks") {
        if let Some(session_start) = hooks.get_mut("SessionStart") {
            if let Some(arr) = session_start.as_array_mut() {
                arr.push(serde_json::json!({
                    "hooks": [{"type": "command", "command": "my-custom.sh", "timeout": 5}]
                }));
            }
        }
    }
    fs::write(&hooks_path, serde_json::to_string_pretty(&parsed).unwrap()).unwrap();

    // Remove repo-graph hooks
    let output = rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "remove", "--project"])
        .output()
        .unwrap();

    assert!(output.status.success());

    let final_content = fs::read_to_string(&hooks_path).unwrap();
    // Custom hook preserved
    assert!(final_content.contains("my-custom.sh"));
    // repo-graph hooks removed
    assert!(!final_content.contains("rmap hook"));
}

#[test]
fn remove_nonexistent_is_noop() {
    let temp = TempDir::new().unwrap();

    let output = rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "remove", "--project"])
        .output()
        .unwrap();

    // Should succeed (no error for missing file)
    assert!(output.status.success());
}

// ============================================================================
// Status tests
// ============================================================================

#[test]
fn status_absent_shows_not_installed() {
    let temp = TempDir::new().unwrap();

    let output = rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "status", "--project"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("not found") || stdout.contains("not installed"));
}

#[test]
fn status_minimal_shows_minimal() {
    let temp = TempDir::new().unwrap();

    // Install minimal
    rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "install", "--project"])
        .output()
        .unwrap();

    let output = rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "status", "--project"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("minimal") || stdout.contains("Minimal"));
}

#[test]
fn status_full_shows_full() {
    let temp = TempDir::new().unwrap();

    // Install full
    rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "install", "--project", "--full"])
        .output()
        .unwrap();

    let output = rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "status", "--project"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Note: Codex full is 4 events, different from Claude's 5
    assert!(stdout.contains("full") || stdout.contains("Full") || stdout.contains("custom"));
}

#[test]
fn status_json_output() {
    let temp = TempDir::new().unwrap();

    // Install
    rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "install", "--project"])
        .output()
        .unwrap();

    let output = rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "status", "--project", "--json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(parsed.is_ok(), "output should be valid JSON: {}", stdout);

    let json = parsed.unwrap();
    assert!(json.get("scope").is_some());
    assert!(json.get("status").is_some());
}

// ============================================================================
// Invalid JSON tests
// ============================================================================

#[test]
fn install_invalid_json_fails() {
    let temp = setup_codex_config(Some("{not valid json"));

    let output = rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "install", "--project"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid") || stderr.contains("JSON") || stderr.contains("error"));
}

#[test]
fn status_invalid_json_reports_error() {
    let temp = setup_codex_config(Some("{broken"));

    let output = rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "status", "--project"])
        .output()
        .unwrap();

    // Status should still succeed but report the error
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("invalid") || stdout.contains("JSON") || stdout.contains("error"));
}

// ============================================================================
// Codex-specific behavior tests
// ============================================================================

#[test]
fn codex_session_start_has_matcher() {
    let temp = TempDir::new().unwrap();

    rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "install", "--project"])
        .output()
        .unwrap();

    let content = fs::read_to_string(temp.path().join(".codex/hooks.json")).unwrap();
    // Codex SessionStart should have "startup|resume" matcher
    assert!(content.contains("startup|resume") || content.contains("startup\\|resume"));
}

#[test]
fn codex_post_tool_use_includes_apply_patch() {
    let temp = TempDir::new().unwrap();

    rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "install", "--project", "--full"])
        .output()
        .unwrap();

    let content = fs::read_to_string(temp.path().join(".codex/hooks.json")).unwrap();
    // Codex PostToolUse matcher should include apply_patch
    assert!(content.contains("apply_patch"));
}

#[test]
fn codex_does_not_have_precompact() {
    let temp = TempDir::new().unwrap();

    rmap_binary()
        .current_dir(temp.path())
        .args(["integrate", "codex", "install", "--project", "--full"])
        .output()
        .unwrap();

    let content = fs::read_to_string(temp.path().join(".codex/hooks.json")).unwrap();
    // Codex should NOT have PreCompact (Claude Code has it, Codex doesn't)
    assert!(!content.contains("PreCompact"));
}
