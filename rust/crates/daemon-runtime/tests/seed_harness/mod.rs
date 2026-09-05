//! Shared EMBED-SEED / FIND-FACTS seam harness — a real `ServiceDispatcher` under a
//! throwaway state root, real-git TS repo builders, the fake loopback embedding
//! server (a2 OpenAI-embeddings contract), in-test store publish, and JSON response
//! readers.
//!
//! Extracted from `seed_seam.rs` (FIND-FACTS-1 review-7 item 2) so the FACTS-tier
//! proofs can live in their own `tests/find_facts_seam.rs` binary WITHOUT duplicating
//! ~340 lines of harness and WITHOUT re-expanding the already-oversized `seed_seam.rs`.
//!
//! ABSTRACTION (test-support module, NOT a production abstraction — never compiled into
//! a shipped artifact):
//!   - what: an isolated seam harness (dispatcher + real-git repo builders + fake embed
//!     server + sidecar publish + response readers).
//!   - concrete users: `tests/seed_seam.rs` (the embedding seed-tier proofs) +
//!     `tests/find_facts_seam.rs` (the FACTS-tier proofs).
//!   - axis: two cohesive integration-test binaries sharing one harness — the split
//!     forced by review-7 item 2 (move FIND-FACTS coverage out of the mixed seed file).
//!   - rejected simpler alternative: leaving both suites in one 2000+-line file (the
//!     guardrail breach review-7 flagged), or duplicating the harness across both files.
//!
//! `#![allow(dead_code)]`: this module is compiled INTO EACH integration-test binary,
//! and each binary uses only the subset of helpers its tests need — the standard Rust
//! `tests/common` pattern. Unused-in-one-binary is expected, not a defect.
#![allow(dead_code)]

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use repo_graph_daemon_runtime::{DaemonState, RepoRegistry, ServiceDispatcher};
use repo_graph_daemon_transport::{
    DispatchResult, Dispatcher, EmitError, ProgressDetail, ProgressEmitter, Request,
};
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};

/// Serializes the process-global background-pass overrides across tests in ONE
/// binary. Each test binary that includes this module gets its own instance
/// (separate processes), so cross-binary collisions cannot occur.
static SEED_SERIAL: Mutex<()> = Mutex::new(());

struct Quiet;
impl ProgressEmitter for Quiet {
    fn emit(&mut self, _detail: ProgressDetail) -> Result<(), EmitError> {
        Ok(())
    }
}

/// Hold the serial lock + keep the background maintenance passes OFF for the test's
/// lifetime, so `index` drives ONLY a READY snapshot + corpus and no incidental
/// background writer contends for the DB (the FOREGROUND-LOCK flake class). The seed
/// serving tier reads the per-snapshot `seed_vectors` table; with the passes off and
/// no vectors published, it renders NoStore — the FACTS tier still answers, which is
/// what these seams assert. SEED-CHUNK-1 retired the lmstudio endpoint, so there is
/// no endpoint env to set.
pub struct SeedEnv<'a> {
    _guard: MutexGuard<'a, ()>,
}
impl SeedEnv<'_> {
    pub fn quiet() -> Self {
        let guard = SEED_SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        repo_graph_daemon_runtime::seed::set_auto_seed_for_test(false);
        repo_graph_daemon_runtime::enrich_pass::set_auto_enrich_for_test(false);
        repo_graph_daemon_runtime::retention_pass::set_auto_retention_for_test(false);
        Self { _guard: guard }
    }
}

// ── isolated dispatcher + real-git TS repo ───────────────────────────────────

pub fn isolated() -> (ServiceDispatcher, TempDir) {
    let state_root = tempdir().expect("state root tempdir");
    let registry = RepoRegistry::with_state_root(state_root.path())
        .expect("isolated registry under temp root");
    let state = std::sync::Arc::new(DaemonState::with_registry(registry));
    (ServiceDispatcher::new(state), state_root)
}

/// `isolated()` with the REAL background maintenance passes (enrich -> seed -> retention)
/// DISABLED process-globally — for test binaries whose EVERY test wants a quiet index (the
/// live-LM-Studio lock-flake class, 5th recurrence 2026-08-31). NOT used by `seed_seam.rs`,
/// which toggles the flags per-test (a global disable here races its enable-tests in the
/// same process — bitten 2026-08-31, first placement attempt).
pub fn isolated_quiet() -> (ServiceDispatcher, TempDir) {
    repo_graph_daemon_runtime::seed::set_auto_seed_for_test(false);
    repo_graph_daemon_runtime::enrich_pass::set_auto_enrich_for_test(false);
    repo_graph_daemon_runtime::retention_pass::set_auto_retention_for_test(false);
    isolated()
}

/// `isolated_quiet()` that ALSO hands back the `Arc<DaemonState>` the dispatcher was built on, so a
/// test can reach the same in-process `DatabaseState` the handlers see and hold its write mutex —
/// the seam DAEMON-RESIDUALS-1 D1-A needs to exercise `acquire_foreground_write`'s IN-PROCESS block
/// site through the real dispatcher (a raw SQLite file lock, as the other seam tests use, trips the
/// storage open first and never reaches the write-mutex layer). Passes are disabled process-globally
/// as in `isolated_quiet()` so the ONLY writer contending is the test's held guard.
pub fn isolated_quiet_with_state() -> (ServiceDispatcher, std::sync::Arc<DaemonState>, TempDir) {
    repo_graph_daemon_runtime::seed::set_auto_seed_for_test(false);
    repo_graph_daemon_runtime::enrich_pass::set_auto_enrich_for_test(false);
    repo_graph_daemon_runtime::retention_pass::set_auto_retention_for_test(false);
    let state_root = tempdir().expect("state root tempdir");
    let registry = RepoRegistry::with_state_root(state_root.path())
        .expect("isolated registry under temp root");
    let state = std::sync::Arc::new(DaemonState::with_registry(registry));
    (ServiceDispatcher::new(state.clone()), state, state_root)
}

pub fn run_git(cwd: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run git");
    assert!(out.status.success(), "git {args:?} failed");
}

pub fn make_repo() -> TempDir {
    make_repo_files(&[
        (
            "helper.ts",
            "export function helperFunction() {\n    return 1;\n}\n",
        ),
        (
            "main.ts",
            "import { helperFunction } from './helper';\n\nexport function mainEntry() {\n    helperFunction();\n}\n",
        ),
    ])
}

/// A repo where the SAME symbol name is defined in two files — a bare focus on
/// that name resolves AMBIGUOUSLY (a deterministic multi-candidate result, NOT a
/// no-match), so the semantic tier must NOT fire on it (parity coverage).
pub fn make_ambiguous_repo() -> TempDir {
    make_repo_files(&[
        (
            "alpha.ts",
            "export function sharedName() {\n    return 1;\n}\n",
        ),
        (
            "beta.ts",
            "export function sharedName() {\n    return 2;\n}\n",
        ),
    ])
}

pub fn make_repo_files(files: &[(&str, &str)]) -> TempDir {
    let repo = tempdir().expect("repo tempdir");
    for (name, body) in files {
        std::fs::write(repo.path().join(name), body).unwrap();
    }
    run_git(repo.path(), &["init"]);
    run_git(repo.path(), &["config", "user.email", "t@e.com"]);
    run_git(repo.path(), &["config", "user.name", "T"]);
    run_git(repo.path(), &["checkout", "-B", "main"]);
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "init"]);
    repo
}

pub fn dispatch(d: &ServiceDispatcher, method: &str, params: Value) -> DispatchResult {
    let request = Request {
        id: "t".to_string(),
        method: method.to_string(),
        params,
    };
    d.dispatch(&request, &mut Quiet)
}

pub fn dispatch_ok(d: &ServiceDispatcher, method: &str, params: Value) -> Value {
    match dispatch(d, method, params) {
        DispatchResult::Success(s) => s.result,
        DispatchResult::Error(e) => panic!("{method} failed {}: {}", e.error.code, e.error.message),
    }
}

/// The error of a dispatch that MUST fail, as a comparable `{code, message, data}`
/// value — the Group-B tier rides the error's `data`, so tests need the whole detail
/// (not just code/message). Panics if the dispatch unexpectedly succeeded.
pub fn dispatch_error(d: &ServiceDispatcher, method: &str, params: Value) -> Value {
    match dispatch(d, method, params) {
        DispatchResult::Error(e) => json!({
            "code": e.error.code.to_string(),
            "message": e.error.message,
            "data": e.error.data,
        }),
        DispatchResult::Success(s) => {
            panic!("{method} unexpectedly succeeded: {}", s.result)
        }
    }
}

/// A COMPARABLE value for any dispatch outcome: the success result, or a stable
/// error object. Byte-parity then holds across success AND identical-error runs —
/// used by the deterministic-command parity matrix (review-4 #3).
pub fn dispatch_value(d: &ServiceDispatcher, method: &str, params: Value) -> Value {
    match dispatch(d, method, params) {
        DispatchResult::Success(s) => s.result,
        DispatchResult::Error(e) => {
            json!({ "error": { "code": e.error.code.to_string(), "message": e.error.message } })
        }
    }
}

// ── helpers to read the response shape ───────────────────────────────────────

pub fn coords(index_payload: &Value) -> (String, String) {
    (
        index_payload["db_path"].as_str().unwrap().to_string(),
        index_payload["repo_uid"].as_str().unwrap().to_string(),
    )
}

/// The `focus` object from an orient response (through the coherence envelope).
pub fn focus_of(orient: &Value) -> &Value {
    orient
        .get("value")
        .and_then(|v| v.get("focus"))
        .or_else(|| orient.get("focus"))
        .expect("orient response carries a focus")
}

pub fn limits_of(orient: &Value) -> Vec<&Value> {
    orient
        .get("value")
        .and_then(|v| v.get("limits"))
        .or_else(|| orient.get("limits"))
        .and_then(|v| v.as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

pub fn has_limit_code(orient: &Value, code: &str) -> bool {
    limits_of(orient)
        .iter()
        .any(|l| l.get("code").and_then(|c| c.as_str()) == Some(code))
}
