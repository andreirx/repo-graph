//! CLI tests for the `check` command.
//!
//! # REG-1 + CLI-OUT-1 Contract
//!
//! With REG-1, the `check` command requires daemon and resolves repo from cwd.
//! With CLI-OUT-1, output defaults to human-readable; `--json` returns full envelope.
//!
//! New contract: `rmap check [--json]`
//!
//! Full integration tests for the new contract are in daemon_dispatch.rs
//! since they require daemon coordination.

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

/// Returns a non-existent socket path for test isolation.
fn isolated_socket_path(dir: &std::path::Path) -> PathBuf {
    dir.join("nonexistent-daemon.sock")
}

fn run_cmd_isolated(args: &[&str]) -> std::process::Output {
    let dir = tempfile::tempdir().unwrap();
    Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(dir.path()))
        .args(args)
        .output()
        .expect("failed to spawn rmap")
}

// -- REG-1: Usage tests ---------------------------------------------------

#[test]
fn check_no_args_is_valid() {
    // With REG-1, check takes no positional arguments - repo comes from cwd.
    // Use isolated socket to ensure daemon unavailable.
    let output = run_cmd_isolated(&["check"]);

    // Exit code 2 = runtime error (daemon unavailable)
    assert_eq!(
        output.status.code(),
        Some(2),
        "Expected runtime error (2), not usage error (1). stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify we got some error output
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "Should have error message on stderr");
}

#[test]
fn check_unexpected_args_is_usage_error() {
    // With REG-1, check takes no positional arguments
    let output = Command::new(binary_path())
        .args(["check", "unexpected_arg"])
        .output()
        .unwrap();

    // Exit code 1 = usage error
    assert_eq!(
        output.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected") || stderr.contains("usage"),
        "stderr should mention unexpected args or usage: {}",
        stderr
    );
}

// -- CLI-OUT-1: --json flag tests ---------------------------------------------

#[test]
fn check_json_flag_accepted() {
    // --json is a valid flag; should not cause usage error (exit 1)
    // Use isolated socket to ensure daemon unavailable.
    let output = run_cmd_isolated(&["check", "--json"]);

    assert_eq!(
        output.status.code(),
        Some(2),
        "Should be runtime error (2), not usage error (1). stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_unknown_flag_usage_error() {
    let output = Command::new(binary_path())
        .args(["check", "--bogus"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "Unknown flag should be usage error. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// CHECK-LIVEGRAPH-IMPL — SUCCESS-PATH / COHERENCE-ENVELOPE COVERAGE MAP
// ══════════════════════════════════════════════════════════════════════════════
//
// The tests above drive the real `rmap` binary for the NO-DAEMON surfaces (usage errors → exit 1;
// daemon-unavailable → exit 2). The success-path coherence contract requires a daemon, so — following the
// established orient precedent (see orient_command.rs's trailer and `orient_returns_coherence_envelope_shape`)
// and the slice's §5 guidance (off-target fixtures for the CLI-wrapper cases) — it is pinned where the daemon
// and the wire shape are available, NOT via a flaky cross-process socket harness:
//
//   - rmapd-level envelope shape (top-level `CoherenceEnvelope`, `value.signals[*].value` nesting, the
//     MULTI-SOURCE verdict provenance {sqlite, declaration}, root Fresh freshness/trust, ABSENT
//     `trust_briefing`, never-LiveGraph): tests/daemon_dispatch.rs → `check_returns_coherence_envelope_shape`
//     (real `handle_check` dispatch + real serialization through the in-process transport).
//   - Stale / no-snapshot (Unavailable, single-source {sqlite}) degradation: daemon-runtime
//     `check_coherence` tests (real `RepoState` + SQLite) and agent `check::coherent` tests (pure folds).
//   - CLI exit-code parity (CHECK_PASS=0 / CHECK_FAIL=1 / CHECK_INCOMPLETE=2 / not-found=2) over the EXACT
//     wrapped `value.signals[*].value.code` path + the anti-silent-break regression guard (§3e CRITICAL /
//     §5 CW5): rgr `presentation::check` → `check_exit_code` tests. The human/`--json` render projection of
//     the wrapped `value` (incl. the `Verdict: PASS@Fresh` freshness suffix) is pinned in the same module.
