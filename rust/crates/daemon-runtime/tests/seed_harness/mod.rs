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

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::thread;

use repo_graph_daemon_runtime::{DaemonState, RepoRegistry, ServiceDispatcher};
use repo_graph_daemon_transport::{
    DispatchResult, Dispatcher, EmitError, ProgressDetail, ProgressEmitter, Request,
};
use repo_graph_seed::pass::{build_store, BuildConfig, BuildOutcome};
use repo_graph_seed::ports::{EmbedError, Embedder};
use repo_graph_seed::SeedCorpusRead;
use repo_graph_storage::StorageConnection;
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};

/// Serializes the process-global seed env across tests in ONE binary. Each test
/// binary that includes this module gets its own instance (separate processes), so
/// cross-binary env collisions cannot occur — only within-binary serialization.
static SEED_SERIAL: Mutex<()> = Mutex::new(());

pub const TEST_MODEL: &str = "test-embed-model";
const TEST_DIM: usize = 8;

struct Quiet;
impl ProgressEmitter for Quiet {
    fn emit(&mut self, _detail: ProgressDetail) -> Result<(), EmitError> {
        Ok(())
    }
}

/// Hold the serial lock + set the seed endpoint env for the test's lifetime.
pub struct SeedEnv<'a> {
    _guard: MutexGuard<'a, ()>,
}
impl SeedEnv<'_> {
    pub fn with_endpoint(endpoint: &str) -> Self {
        let guard = SEED_SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        // The background embed pass stays OFF (we publish the sidecar directly), and
        // the background enrich pass stays OFF too: this binary drives `index` only
        // for a READY snapshot + corpus and asserts the query seam, so the enrich
        // WRITER is incidental — leaving it on lets its write transaction contend for
        // the DB while the test opens its own connection (`publish_store`), a
        // non-hermetic race under a saturated full-workspace run. Same override the
        // enrich/retention seam tests use (common/mod.rs, forget_repo, …).
        repo_graph_daemon_runtime::seed::set_auto_seed_for_test(false);
        repo_graph_daemon_runtime::enrich_pass::set_auto_enrich_for_test(false);
        // Retention too (2026-09-01): the chained retention pass held the DB open while a
        // foreground `find` dispatched — "database is locked" InternalError, the 1-in-5
        // find_facts_seam flake (and the mechanism behind the earlier "unnamed" full-suite
        // flakes). The FOREGROUND-LOCK-1 product fix adds open patience; tests stay hermetic
        // regardless: no incidental background writer.
        repo_graph_daemon_runtime::retention_pass::set_auto_retention_for_test(false);
        std::env::set_var("RMAP_SEED_ENDPOINT", endpoint);
        std::env::set_var("RMAP_SEED_MODEL_ID", TEST_MODEL);
        std::env::set_var("RMAP_SEED_DIM", TEST_DIM.to_string());
        Self { _guard: guard }
    }
}
impl Drop for SeedEnv<'_> {
    fn drop(&mut self) {
        std::env::remove_var("RMAP_SEED_ENDPOINT");
        std::env::remove_var("RMAP_SEED_MODEL_ID");
        std::env::remove_var("RMAP_SEED_DIM");
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

// ── in-test store publish (same vector the fake server returns) ───────────────

/// A fake embedder that returns a FIXED unit vector for every document — the same
/// vector the loopback server returns for the query, so cosine = 1 and the tier
/// fires deterministically.
pub struct FixedEmbedder;
impl Embedder for FixedEmbedder {
    fn model_id(&self) -> &str {
        TEST_MODEL
    }
    fn dim(&self) -> usize {
        TEST_DIM
    }
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        Ok(texts.iter().map(|_| fixed_vector()).collect())
    }
}

fn fixed_vector() -> Vec<f32> {
    let mut v = vec![0.0f32; TEST_DIM];
    v[0] = 1.0;
    v
}

/// Publish a `.vec` sidecar for the indexed repo using the real store format +
/// real corpus, so freshness + `resolve_path_focus` succeed at query time.
pub fn publish_store(db_path: &str, repo_uid: &str, repo_root: &Path) {
    // The daemon's post-index maintenance chain (enrich -> seed -> retention) may
    // still hold this DB when the test proceeds; under full-suite parallelism the
    // 5s busy budget can expire. A bounded retry here is TEST serialization with
    // that chain — not product behavior (the product surfaces the honest busy
    // error; the test must simply wait its turn). Flaked twice before this guard.
    let conn = {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            match StorageConnection::open(db_path) {
                Ok(c) => break c,
                Err(e) if std::time::Instant::now() < deadline => {
                    let msg = format!("{e}");
                    assert!(
                        msg.contains("locked") || msg.contains("busy"),
                        "non-busy open failure must not be retried: {msg}"
                    );
                    thread::sleep(std::time::Duration::from_millis(250));
                }
                Err(e) => panic!("storage open still busy after 30s: {e}"),
            }
        }
    };
    let entries = conn.seed_corpus(repo_uid).unwrap();
    assert!(!entries.is_empty(), "the TS repo must yield a seed corpus");
    let key = repo_graph_daemon_runtime::seed::SeedEndpointConfig::from_env().store_key();
    let read_file = |rel: &str| std::fs::read_to_string(repo_root.join(rel));
    let outcome = build_store(
        entries,
        &FixedEmbedder,
        read_file,
        || false,
        &key,
        7,
        BuildConfig::default(),
        None,
    );
    let bytes = match outcome {
        BuildOutcome::Built { bytes, .. } => bytes,
        other => panic!("build_store did not build: {other:?}"),
    };
    let sidecar =
        repo_graph_daemon_runtime::seed::sidecar_path(Path::new(db_path)).expect("sidecar path");
    repo_graph_seed::store::atomic_write(&sidecar, &bytes).unwrap();
}

// ── fake loopback embedding server (a2 OpenAI-embeddings contract) ────────────

/// Bind a loopback server that answers `/v1/embeddings` with a fixed unit vector
/// per input, correlated by `index`, echoing `TEST_MODEL`. Returns the port. The
/// listener thread is detached (leaks for the short test) — no shutdown needed.
pub fn spawn_embed_server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let _ = handle_embed_conn(stream);
        }
    });
    port
}

fn handle_embed_conn(mut stream: TcpStream) -> std::io::Result<()> {
    // Read headers + body (client sends the whole request then reads). Each pass
    // re-checks whether the full `Content-Length`-delimited body has arrived.
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(pos) = find(&buf, b"\r\n\r\n") {
            let header_end = pos + 4;
            let headers = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
            let content_length = headers
                .split("\r\n")
                .find_map(|line| line.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse::<usize>().ok());
            if let Some(cl) = content_length {
                if buf.len() >= header_end + cl {
                    return respond(&mut stream, &buf[header_end..header_end + cl]);
                }
            }
        }
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Ok(())
}

fn respond(stream: &mut TcpStream, body: &[u8]) -> std::io::Result<()> {
    let parsed: Value = serde_json::from_slice(body).unwrap_or(json!({}));
    let n = parsed
        .get("input")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(1);
    let data: Vec<Value> = (0..n)
        .map(|i| json!({ "index": i, "embedding": fixed_vector() }))
        .collect();
    let payload = json!({ "model": TEST_MODEL, "data": data }).to_string();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        payload.len(),
        payload
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
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
