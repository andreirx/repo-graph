//! EMBED-SEED-IMPL-1 — daemon SEAM-integration proofs (spec §12), driven through
//! the REAL `ServiceDispatcher::dispatch` surface (operator ruling 2026-08-25 /
//! review-1 #2: builder-report claims are not coverage).
//!
//! # What this binary proves that the lib/pure tests do not
//!
//! The pure crate proves ranking/store/freshness/incremental with fakes; the
//! transport module proves the a2 accepted-response contract with byte fixtures.
//! What only a REAL dispatch can prove is the wiring: that a no-match `orient`/
//! `explain` FIRES the tier and fills `focus.candidates` with labeled
//! `source:"embedding"` candidates carrying `next` and a well-formed owning-module
//! hint; that `rmap find` returns the same labeled candidates under its own DTO;
//! that a RESOLVED focus is byte-unperturbed (the tier adds nothing); and that
//! every degraded substrate state renders the honest labeled line and NEVER an
//! error.
//!
//! # Hermeticity — a fake loopback embedding server, no LM Studio
//!
//! Firing needs a model. Instead of the operator's LM Studio we stand up a tiny
//! in-process loopback HTTP server that speaks the a2 OpenAI-embeddings contract
//! and returns a fixed unit vector — so the QUERY embedding travels the REAL a2
//! transport (`EndpointEmbedder` → HTTP → parse/correlate-by-index) end to end,
//! and the store is published in-test with the SAME fixed vector so cosine = 1 and
//! the tier fires deterministically. The background WRITE pass is proven separately
//! (pure `pass::tests` + the live dogfood); here we publish the sidecar directly so
//! the query seam is exercised without a racy detached thread.
//!
//! `RMAP_SEED_ENDPOINT`/`_MODEL_ID`/`_DIM` are process-global env, so every test
//! serializes on [`SEED_SERIAL`] and sets the endpoint it needs while holding it.

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

/// Serializes the process-global seed env across tests in this binary.
static SEED_SERIAL: Mutex<()> = Mutex::new(());

const TEST_MODEL: &str = "test-embed-model";
const TEST_DIM: usize = 8;

struct Quiet;
impl ProgressEmitter for Quiet {
    fn emit(&mut self, _detail: ProgressDetail) -> Result<(), EmitError> {
        Ok(())
    }
}

/// Hold the serial lock + set the seed endpoint env for the test's lifetime.
struct SeedEnv<'a> {
    _guard: MutexGuard<'a, ()>,
}
impl<'a> SeedEnv<'a> {
    fn with_endpoint(endpoint: &str) -> Self {
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

fn isolated() -> (ServiceDispatcher, TempDir) {
    let state_root = tempdir().expect("state root tempdir");
    let registry = RepoRegistry::with_state_root(state_root.path())
        .expect("isolated registry under temp root");
    let state = std::sync::Arc::new(DaemonState::with_registry(registry));
    (ServiceDispatcher::new(state), state_root)
}

fn run_git(cwd: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run git");
    assert!(out.status.success(), "git {args:?} failed");
}

fn make_repo() -> TempDir {
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
fn make_ambiguous_repo() -> TempDir {
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

fn make_repo_files(files: &[(&str, &str)]) -> TempDir {
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

fn dispatch(d: &ServiceDispatcher, method: &str, params: Value) -> DispatchResult {
    let request = Request {
        id: "t".to_string(),
        method: method.to_string(),
        params,
    };
    d.dispatch(&request, &mut Quiet)
}

fn dispatch_ok(d: &ServiceDispatcher, method: &str, params: Value) -> Value {
    match dispatch(d, method, params) {
        DispatchResult::Success(s) => s.result,
        DispatchResult::Error(e) => panic!("{method} failed {}: {}", e.error.code, e.error.message),
    }
}

/// The error of a dispatch that MUST fail, as a comparable `{code, message, data}`
/// value — the Group-B tier rides the error's `data`, so tests need the whole detail
/// (not just code/message). Panics if the dispatch unexpectedly succeeded.
fn dispatch_error(d: &ServiceDispatcher, method: &str, params: Value) -> Value {
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
fn dispatch_value(d: &ServiceDispatcher, method: &str, params: Value) -> Value {
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
struct FixedEmbedder;
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
fn publish_store(db_path: &str, repo_uid: &str, repo_root: &Path) {
    let conn = StorageConnection::open(db_path).unwrap();
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
fn spawn_embed_server() -> u16 {
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

fn coords(index_payload: &Value) -> (String, String) {
    (
        index_payload["db_path"].as_str().unwrap().to_string(),
        index_payload["repo_uid"].as_str().unwrap().to_string(),
    )
}

/// The `focus` object from an orient response (through the coherence envelope).
fn focus_of(orient: &Value) -> &Value {
    orient
        .get("value")
        .and_then(|v| v.get("focus"))
        .or_else(|| orient.get("focus"))
        .expect("orient response carries a focus")
}

fn limits_of(orient: &Value) -> Vec<&Value> {
    orient
        .get("value")
        .and_then(|v| v.get("limits"))
        .or_else(|| orient.get("limits"))
        .and_then(|v| v.as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

fn has_limit_code(orient: &Value, code: &str) -> bool {
    limits_of(orient)
        .iter()
        .any(|l| l.get("code").and_then(|c| c.as_str()) == Some(code))
}

// ════════════════════════════════════════════════════════════════════════════
// (i) FIRING — a no-match focus fills focus.candidates with embedding hints + next
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn no_match_focus_fires_tier_with_labeled_embedding_candidates() {
    let port = spawn_embed_server();
    let _env = SeedEnv::with_endpoint(&format!("http://127.0.0.1:{port}/v1/embeddings"));
    let (d, _root) = isolated();
    let repo = make_repo();
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let (db_path, repo_uid) = coords(&idx);
    publish_store(&db_path, &repo_uid, repo.path());

    let orient = dispatch_ok(
        &d,
        "orient",
        json!({ "repo": repo.path().to_string_lossy(),
                "focus": "where the quarterly revenue reconciliation runs" }),
    );
    let focus = focus_of(&orient);
    // Still a deterministic no-match reason; the candidates are labeled Layer-3.
    assert_eq!(
        focus.get("reason").and_then(|r| r.as_str()),
        Some("no_match"),
        "reason stays no_match: {focus}"
    );
    let cands = focus
        .get("candidates")
        .and_then(|c| c.as_array())
        .expect("candidates present after firing");
    assert!(!cands.is_empty(), "tier fired ⇒ candidates non-empty");
    assert!(cands.len() <= 5, "fallback cap ≤5");
    for c in cands {
        assert_eq!(c["source"].as_str(), Some("embedding"), "labeled I2: {c}");
        assert_eq!(c["model_id"].as_str(), Some(TEST_MODEL));
        assert!(c.get("score").and_then(|s| s.as_f64()).is_some());
        assert!(c.get("next").is_some(), "carries the explain follow-up");
        // Owning-module hint is a well-formed ModuleHint — NEVER a directory string.
        let module = c.get("module").expect("module hint present");
        assert!(
            module.is_object(),
            "module is a tagged hint object: {module}"
        );
        let known = module.get("owning").and_then(|v| v.as_str());
        let unavail = module.get("unavailable").and_then(|v| v.as_str());
        assert!(
            known.is_some() ^ unavail.is_some(),
            "exactly one of owning/unavailable: {module}"
        );
    }
    assert!(
        has_limit_code(&orient, "SEMANTIC_FALLBACK"),
        "a labeled SEMANTIC_FALLBACK limit rides the response: {orient}"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// (v) find — labeled candidates under the honesty header
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn find_returns_labeled_candidates_under_summary() {
    let port = spawn_embed_server();
    let _env = SeedEnv::with_endpoint(&format!("http://127.0.0.1:{port}/v1/embeddings"));
    let (d, _root) = isolated();
    let repo = make_repo();
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let (db_path, repo_uid) = coords(&idx);
    publish_store(&db_path, &repo_uid, repo.path());

    let resp = dispatch_ok(
        &d,
        "find",
        json!({ "repo": repo.path().to_string_lossy(), "query": "user login and authentication" }),
    );
    assert_eq!(resp["command"].as_str(), Some("find"));
    assert!(
        resp.get("summary").and_then(|s| s.as_str()).is_some(),
        "summary honesty header always present"
    );
    let cands = resp["candidates"]
        .as_array()
        .expect("candidates:[] present");
    assert!(!cands.is_empty(), "find fired ⇒ candidates");
    assert!(cands.len() <= 10, "find cap ≤10");
    for c in cands {
        assert_eq!(c["source"].as_str(), Some("embedding"));
        assert!(c["path"].as_str().is_some());
        assert!(c["module"].is_object(), "module is a tagged hint: {c}");
    }
}

// ════════════════════════════════════════════════════════════════════════════
// (ii)/(iii) PARITY — a resolved focus is byte-unperturbed (tier adds nothing)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn resolved_focus_carries_no_semantic_markers_store_present_or_absent() {
    let port = spawn_embed_server();
    let _env = SeedEnv::with_endpoint(&format!("http://127.0.0.1:{port}/v1/embeddings"));
    let (d, _root) = isolated();
    let repo = make_repo();
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let (db_path, repo_uid) = coords(&idx);

    // A concrete existing file resolves — the tier must NOT fire on it.
    let params = json!({ "repo": repo.path().to_string_lossy(), "focus": "helper.ts" });

    // Build the store right after index (the raw-connection order `no_match` proves
    // safe), then toggle the SIDECAR FILE's presence around the two orients — so
    // neither orient opens a raw DB connection that could contend with the daemon's
    // cached one. The sidecar is a plain file; the tier's with/without-store parity
    // is exactly what varies.
    publish_store(&db_path, &repo_uid, repo.path());
    let sidecar =
        repo_graph_daemon_runtime::seed::sidecar_path(Path::new(&db_path)).expect("sidecar path");
    let store_bytes = std::fs::read(&sidecar).expect("store published");
    std::fs::remove_file(&sidecar).expect("remove sidecar for the no-store run");

    // No store present.
    let without = dispatch_ok(&d, "orient", params.clone());
    // Restore the store (a plain file write — no DB access) and re-run.
    std::fs::write(&sidecar, &store_bytes).expect("restore sidecar");
    let with = dispatch_ok(&d, "orient", params);

    for (label, r) in [("no-store", &without), ("store", &with)] {
        let focus = focus_of(r);
        assert_ne!(
            focus.get("reason").and_then(|x| x.as_str()),
            Some("no_match"),
            "{label}: helper.ts should resolve, not no_match: {focus}"
        );
        assert!(
            !has_limit_code(r, "SEMANTIC_FALLBACK")
                && !has_limit_code(r, "SEMANTIC_FALLBACK_UNAVAILABLE"),
            "{label}: no semantic limit on a resolved focus"
        );
        if let Some(cands) = focus.get("candidates").and_then(|c| c.as_array()) {
            for c in cands {
                assert_ne!(
                    c.get("source").and_then(|s| s.as_str()),
                    Some("embedding"),
                    "{label}: resolved focus carries no embedding candidate"
                );
            }
        }
    }
    // The tier only ever touches `focus.candidates` + `limits`; on a resolved focus
    // both are byte-identical whether or not a seed store exists (the top-level
    // envelope carries per-run timing, so parity is asserted on exactly the surfaces
    // the tier can perturb).
    assert_eq!(
        serde_json::to_string(focus_of(&without)).unwrap(),
        serde_json::to_string(focus_of(&with)).unwrap(),
        "resolved focus object is byte-identical with and without a seed store"
    );
    assert_eq!(
        serde_json::to_string(&limits_of(&without)).unwrap(),
        serde_json::to_string(&limits_of(&with)).unwrap(),
        "limits are byte-identical with and without a seed store"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// (iv) DEGRADED — dead endpoint on a no-match ⇒ empty candidates + one labeled line
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn dead_endpoint_no_match_is_honestly_degraded() {
    // Point at a closed loopback port (nothing listening) — the model is "down".
    let _env = SeedEnv::with_endpoint("http://127.0.0.1:9/v1/embeddings");
    let (d, _root) = isolated();
    let repo = make_repo();
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let (db_path, repo_uid) = coords(&idx);
    publish_store(&db_path, &repo_uid, repo.path()); // store exists, model does not

    let orient = dispatch_ok(
        &d,
        "orient",
        json!({ "repo": repo.path().to_string_lossy(),
                "focus": "where the quarterly revenue reconciliation runs" }),
    );
    let focus = focus_of(&orient);
    let cands = focus
        .get("candidates")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        cands.is_empty(),
        "no candidates when the model is unreachable"
    );
    assert!(
        has_limit_code(&orient, "SEMANTIC_FALLBACK_UNAVAILABLE"),
        "one labeled unavailable limit: {orient}"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// (vi) find DEGRADED — always-present candidates:[] under the labeled summary
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn find_dead_endpoint_returns_empty_candidates_with_summary() {
    let _env = SeedEnv::with_endpoint("http://127.0.0.1:9/v1/embeddings");
    let (d, _root) = isolated();
    let repo = make_repo();
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let (db_path, repo_uid) = coords(&idx);
    publish_store(&db_path, &repo_uid, repo.path());

    let resp = dispatch_ok(
        &d,
        "find",
        json!({ "repo": repo.path().to_string_lossy(), "query": "exchange rate conversion" }),
    );
    // candidates key ALWAYS present as [] (never omitted), plus a labeled summary.
    let cands = resp.get("candidates").and_then(|c| c.as_array());
    assert_eq!(
        cands.map(|a| a.len()),
        Some(0),
        "candidates:[] present + empty"
    );
    let summary = resp["summary"].as_str().unwrap_or("");
    assert!(
        summary.contains("no local embedding model reachable"),
        "honest model-down summary, got: {summary}"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// (ii) PARITY — an AMBIGUOUS focus is byte-unperturbed (tier fires only on no_match)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn ambiguous_focus_parity_tier_does_not_fire() {
    let port = spawn_embed_server();
    let _env = SeedEnv::with_endpoint(&format!("http://127.0.0.1:{port}/v1/embeddings"));
    let (d, _root) = isolated();
    let repo = make_ambiguous_repo();
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let (db_path, repo_uid) = coords(&idx);

    // `sharedName` is defined in two files ⇒ deterministic AMBIGUOUS resolution.
    let params = json!({ "repo": repo.path().to_string_lossy(), "focus": "sharedName" });

    let without = dispatch_ok(&d, "orient", params.clone());
    publish_store(&db_path, &repo_uid, repo.path());
    let with = dispatch_ok(&d, "orient", params);

    // Ambiguous is a RESOLVED-class outcome — reason is `ambiguous`, never `no_match`,
    // so the tier is never consulted.
    assert_eq!(
        focus_of(&without).get("reason").and_then(|r| r.as_str()),
        Some("ambiguous"),
        "sharedName must resolve ambiguously: {}",
        focus_of(&without)
    );
    for (label, r) in [("no-store", &without), ("store", &with)] {
        assert!(
            !has_limit_code(r, "SEMANTIC_FALLBACK")
                && !has_limit_code(r, "SEMANTIC_FALLBACK_UNAVAILABLE"),
            "{label}: no semantic limit on an ambiguous focus"
        );
        for c in focus_of(r)
            .get("candidates")
            .and_then(|c| c.as_array())
            .into_iter()
            .flatten()
        {
            assert_ne!(
                c.get("source").and_then(|s| s.as_str()),
                Some("embedding"),
                "{label}: ambiguous candidate is deterministic, never an embedding hint"
            );
        }
    }
    // Byte-parity on the surfaces the tier could perturb, with and without a store.
    assert_eq!(
        serde_json::to_string(focus_of(&without)).unwrap(),
        serde_json::to_string(focus_of(&with)).unwrap(),
        "ambiguous focus object is byte-identical with and without a seed store"
    );
    assert_eq!(
        serde_json::to_string(&limits_of(&without)).unwrap(),
        serde_json::to_string(&limits_of(&with)).unwrap(),
        "limits are byte-identical with and without a seed store"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// (i) FIRING — explain's no-match branch fires the SAME tier as orient
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn explain_no_match_fires_tier_with_labeled_candidates() {
    let port = spawn_embed_server();
    let _env = SeedEnv::with_endpoint(&format!("http://127.0.0.1:{port}/v1/embeddings"));
    let (d, _root) = isolated();
    let repo = make_repo();
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let (db_path, repo_uid) = coords(&idx);
    publish_store(&db_path, &repo_uid, repo.path());

    // A target that resolves to no symbol/file/module ⇒ explain no-match ⇒ tier fires
    // (explain's focus-resolution no-match shares orient's contract).
    let explain = dispatch_ok(
        &d,
        "explain",
        json!({ "repo": repo.path().to_string_lossy(),
                "target": "where the quarterly revenue reconciliation runs" }),
    );
    let focus = focus_of(&explain);
    assert_eq!(
        focus.get("reason").and_then(|r| r.as_str()),
        Some("no_match"),
        "explain no-match reason preserved: {focus}"
    );
    let cands = focus
        .get("candidates")
        .and_then(|c| c.as_array())
        .expect("explain fallback fills candidates");
    assert!(!cands.is_empty(), "explain tier fired ⇒ candidates");
    for c in cands {
        assert_eq!(c["source"].as_str(), Some("embedding"), "labeled I2: {c}");
        assert!(c.get("next").is_some(), "carries the explain follow-up");
        assert!(c["module"].is_object(), "owning-module hint is tagged: {c}");
    }
    assert!(
        has_limit_code(&explain, "SEMANTIC_FALLBACK"),
        "explain carries the labeled SEMANTIC_FALLBACK limit: {explain}"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// (i) FIRING — Group B: callers/callees/path NotFound ride the tier on the error
// `data` (spec §8.1/§8.2 Group B). The error CODE + MESSAGE + exit stay byte-stable;
// the previously-`None` `data` gains `semantic_candidates` + a labeled `hint`. This is
// the operator-required wiring of EVERY enumerated seam (2026-08-25 finding).
// ════════════════════════════════════════════════════════════════════════════

/// Assert a Group-B error is the UNCHANGED `symbol not found` error PLUS additive
/// labeled semantic candidates + a hint on `data`.
fn assert_group_b_fired(err: &Value, query: &str) {
    assert_eq!(
        err["code"].as_str(),
        Some("InvalidRequest"),
        "code unchanged: {err}"
    );
    assert_eq!(
        err["message"].as_str(),
        Some(format!("symbol not found: {query}").as_str()),
        "message unchanged: {err}"
    );
    let cands = err["data"]["semantic_candidates"]
        .as_array()
        .expect("semantic_candidates present on the error data");
    assert!(!cands.is_empty(), "tier fired ⇒ candidates: {err}");
    assert!(cands.len() <= 5, "Group-B cap ≤5: {err}");
    for c in cands {
        assert_eq!(c["source"].as_str(), Some("embedding"), "labeled I2: {c}");
        assert_eq!(c["model_id"].as_str(), Some(TEST_MODEL));
        assert!(c.get("score").and_then(|s| s.as_f64()).is_some());
        assert!(
            c.get("file").and_then(|f| f.as_str()).is_some(),
            "Group-B candidate carries `file` (§8.2): {c}"
        );
        assert!(c.get("kind").is_none(), "Group-B omits `kind` (§8.2): {c}");
        assert!(c.get("next").is_some(), "carries the explain follow-up");
        let module = c.get("module").expect("module hint present");
        let known = module.get("owning").and_then(|v| v.as_str());
        let unavail = module.get("unavailable").and_then(|v| v.as_str());
        assert!(
            known.is_some() ^ unavail.is_some(),
            "exactly one of owning/unavailable: {module}"
        );
    }
    assert!(
        err["data"]["hint"].as_str().is_some(),
        "a labeled hint always rides the fired error: {err}"
    );
}

#[test]
fn group_b_callers_callees_path_not_found_fire_the_tier() {
    let port = spawn_embed_server();
    let _env = SeedEnv::with_endpoint(&format!("http://127.0.0.1:{port}/v1/embeddings"));
    let (d, _root) = isolated();
    let repo = make_repo();
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let (db_path, repo_uid) = coords(&idx);
    publish_store(&db_path, &repo_uid, repo.path());
    let repo_s = repo.path().to_string_lossy().to_string();

    // A concept string that names no symbol ⇒ SymbolResolveError::NotFound on each seam.
    let q = "where the quarterly revenue reconciliation runs";

    let callers = dispatch_error(&d, "callers", json!({ "repo": repo_s, "symbol": q }));
    assert_group_b_fired(&callers, q);

    let callees = dispatch_error(&d, "callees", json!({ "repo": repo_s, "symbol": q }));
    assert_group_b_fired(&callees, q);

    // `path` fires on the FROM endpoint's NotFound (spec §8.1); `to` is never reached.
    let path = dispatch_error(
        &d,
        "path",
        json!({ "repo": repo_s, "from": q, "to": "mainEntry" }),
    );
    assert_group_b_fired(&path, q);
}

#[test]
fn group_b_dead_endpoint_not_found_is_honestly_degraded() {
    // Store present, model down (closed port) ⇒ the error stays the SAME not-found,
    // gains only an honest `hint` (no fabricated candidates), never an error-of-errors.
    let _env = SeedEnv::with_endpoint("http://127.0.0.1:9/v1/embeddings");
    let (d, _root) = isolated();
    let repo = make_repo();
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let (db_path, repo_uid) = coords(&idx);
    publish_store(&db_path, &repo_uid, repo.path());
    let repo_s = repo.path().to_string_lossy().to_string();

    let err = dispatch_error(
        &d,
        "callers",
        json!({ "repo": repo_s, "symbol": "nonexistent_concept_xyz" }),
    );
    assert_eq!(err["code"].as_str(), Some("InvalidRequest"));
    assert_eq!(
        err["message"].as_str(),
        Some("symbol not found: nonexistent_concept_xyz")
    );
    assert!(
        err["data"]["semantic_candidates"].is_null(),
        "model down ⇒ no candidates (omit-when-empty, §8.3): {err}"
    );
    let hint = err["data"]["hint"].as_str().unwrap_or("");
    assert!(
        hint.contains("no local embedding model reachable"),
        "honest model-down hint rides the error: {err}"
    );
}

#[test]
fn group_b_no_store_not_found_is_honestly_degraded() {
    // No store on disk (NoStore) ⇒ the not-found error carries only the honest
    // "not built yet" hint — never an empty candidate list, never a fabricated match.
    let port = spawn_embed_server();
    let _env = SeedEnv::with_endpoint(&format!("http://127.0.0.1:{port}/v1/embeddings"));
    let (d, _root) = isolated();
    let repo = make_repo();
    dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    // No store published.
    let err = dispatch_error(
        &d,
        "callees",
        json!({ "repo": repo.path().to_string_lossy(), "symbol": "whatever" }),
    );
    assert_eq!(err["code"].as_str(), Some("InvalidRequest"));
    assert!(err["data"]["semantic_candidates"].is_null());
    assert!(
        err["data"]["hint"]
            .as_str()
            .unwrap_or("")
            .contains("no seed vectors yet"),
        "no-store hint rides the error: {err}"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// CANCELLATION — a cancelled/superseded pass never overwrites a prior sidecar
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn cancelled_pass_preserves_prior_published_sidecar() {
    let _env = SeedEnv::with_endpoint("http://127.0.0.1:9/v1/embeddings"); // unused here
    let (d, _root) = isolated();
    let repo = make_repo();
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let (db_path, repo_uid) = coords(&idx);

    // Publish generation-1 store.
    publish_store(&db_path, &repo_uid, repo.path());
    let sidecar =
        repo_graph_daemon_runtime::seed::sidecar_path(Path::new(&db_path)).expect("sidecar path");
    let before = std::fs::read(&sidecar).expect("sidecar present after publish");

    // A pass that is cancelled at the first batch boundary must NOT publish — the
    // prior (valid) sidecar stays byte-identical (spec §4.3/§5.1 atomic publication).
    let conn = StorageConnection::open(&db_path).unwrap();
    let entries = conn.seed_corpus(&repo_uid).unwrap();
    let key = repo_graph_daemon_runtime::seed::SeedEndpointConfig::from_env().store_key();
    let outcome = build_store(
        entries,
        &FixedEmbedder,
        |rel: &str| std::fs::read_to_string(repo.path().join(rel)),
        || true, // cancelled before the first batch
        &key,
        99,
        BuildConfig::default(),
        None,
    );
    assert!(
        matches!(outcome, BuildOutcome::Cancelled),
        "a cancelled pass yields Cancelled and never produces bytes to publish"
    );
    let after = std::fs::read(&sidecar).expect("prior sidecar still present");
    assert_eq!(
        before, after,
        "a cancelled pass leaves the prior published sidecar byte-identical"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// MAINTENANCE ORDERING — the REAL background pass runs after index; doctor sees it
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn background_seed_pass_runs_after_index_and_doctor_shows_present() {
    let port = spawn_embed_server();
    let _env = SeedEnv::with_endpoint(&format!("http://127.0.0.1:{port}/v1/embeddings"));
    // Enable the REAL background embed pass (serialized by SEED_SERIAL); it is
    // spawned by the maintenance chain AFTER index (enrich → seed → retention).
    repo_graph_daemon_runtime::seed::set_auto_seed_for_test(true);
    let (d, _root) = isolated();
    let repo = make_repo();
    dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );

    // Poll storage_health until the background pass publishes the sidecar. The loop
    // breaks the instant the state is `present` (≈0.4s when run alone), so the high
    // iteration ceiling costs nothing on the happy path; it exists only so a full
    // `cargo test --workspace` run — dozens of daemon integration processes competing
    // for CPU — cannot starve this process's async seed thread past the bound and
    // report a FALSE negative for a pass that is merely slow to be scheduled. A 60s
    // ceiling for a tiny-repo embed against an in-process server is comfortably above
    // any real scheduling delay while still failing honestly if the pass never runs.
    let mut present = false;
    for _ in 0..600 {
        let health = dispatch_ok(
            &d,
            "storage_health",
            json!({ "path": repo.path().to_string_lossy() }),
        );
        let state = health
            .get("seed")
            .and_then(|s| s.get("state"))
            .and_then(|v| v.as_str());
        // The block is ALWAYS present (review-2 #4) — never omitted.
        assert!(
            state.is_some(),
            "seed doctor block always present: {health}"
        );
        if state == Some("present") {
            present = true;
            break;
        }
        thread::sleep(std::time::Duration::from_millis(100));
    }
    // Reset the override so later tests in this process are unaffected (SEED_SERIAL
    // is still held, so this cannot race another test).
    repo_graph_daemon_runtime::seed::set_auto_seed_for_test(false);
    assert!(
        present,
        "the background seed pass should run after index and doctor should show it present"
    );
}

#[test]
fn find_with_no_store_reports_not_built() {
    let _env = SeedEnv::with_endpoint("http://127.0.0.1:9/v1/embeddings");
    let (d, _root) = isolated();
    let repo = make_repo();
    dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    // No store published.
    let resp = dispatch_ok(
        &d,
        "find",
        json!({ "repo": repo.path().to_string_lossy(), "query": "anything" }),
    );
    assert_eq!(
        resp.get("candidates")
            .and_then(|c| c.as_array())
            .map(|a| a.len()),
        Some(0)
    );
    assert!(
        resp["summary"]
            .as_str()
            .unwrap_or("")
            .contains("not built yet"),
        "not-built summary: {}",
        resp["summary"]
    );
}

// ════════════════════════════════════════════════════════════════════════════
// (iv) DEGRADED — a PIN MISMATCH (store built with a different model) is honest
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn pin_mismatch_degrades_orient_and_find() {
    let port = spawn_embed_server();
    let _env = SeedEnv::with_endpoint(&format!("http://127.0.0.1:{port}/v1/embeddings"));
    let (d, _root) = isolated();
    let repo = make_repo();
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let (db_path, repo_uid) = coords(&idx);
    // Publish under the current pin (model = TEST_MODEL) ...
    publish_store(&db_path, &repo_uid, repo.path());
    // ... then change the model pin so the on-disk store's key no longer matches the
    // runtime config. `store::decode` raises `KeyMismatch` BEFORE any embedding, so
    // the fake server is never consulted — this exercises the store-pin path, not the
    // wire-echo path (which `http`/`transport` tests cover).
    std::env::set_var("RMAP_SEED_MODEL_ID", "a-different-model");

    // orient no-match ⇒ labeled UNAVAILABLE (pins mismatch), zero candidates.
    let orient = dispatch_ok(
        &d,
        "orient",
        json!({ "repo": repo.path().to_string_lossy(),
                "focus": "where the quarterly revenue reconciliation runs" }),
    );
    let focus = focus_of(&orient);
    let cands = focus
        .get("candidates")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(cands.is_empty(), "pin mismatch ⇒ no candidates: {focus}");
    assert!(
        has_limit_code(&orient, "SEMANTIC_FALLBACK_UNAVAILABLE"),
        "pin mismatch ⇒ one labeled unavailable limit: {orient}"
    );

    // find ⇒ zero candidates + the honest "different model" summary (§8B.3).
    let resp = dispatch_ok(
        &d,
        "find",
        json!({ "repo": repo.path().to_string_lossy(), "query": "anything" }),
    );
    assert_eq!(
        resp.get("candidates")
            .and_then(|c| c.as_array())
            .map(|a| a.len()),
        Some(0),
        "pin mismatch ⇒ find returns candidates:[]"
    );
    let summary = resp["summary"].as_str().unwrap_or("");
    assert!(
        summary.contains("different model"),
        "find pin-mismatch summary names the cause: {summary}"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// (iv) DEGRADED — orient AND explain with NO store on disk ⇒ labeled unavailable
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn no_store_orient_and_explain_fire_labeled_unavailable() {
    let port = spawn_embed_server();
    let _env = SeedEnv::with_endpoint(&format!("http://127.0.0.1:{port}/v1/embeddings"));
    let (d, _root) = isolated();
    let repo = make_repo();
    dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    // No store published — the sidecar file is absent (NoStore), the model is up.

    for (method, focus_key) in [("orient", "focus"), ("explain", "target")] {
        let mut params = serde_json::Map::new();
        params.insert("repo".into(), json!(repo.path().to_string_lossy()));
        params.insert(
            focus_key.into(),
            json!("where the quarterly revenue reconciliation runs"),
        );
        let resp = dispatch_ok(&d, method, Value::Object(params));
        let focus = focus_of(&resp);
        assert_eq!(
            focus.get("reason").and_then(|r| r.as_str()),
            Some("no_match"),
            "{method}: still a deterministic no_match with no store: {focus}"
        );
        let cands = focus
            .get("candidates")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();
        assert!(
            cands.is_empty(),
            "{method}: no store ⇒ no candidates: {focus}"
        );
        assert!(
            has_limit_code(&resp, "SEMANTIC_FALLBACK_UNAVAILABLE"),
            "{method}: no store ⇒ one labeled unavailable limit: {resp}"
        );
    }
}

// ════════════════════════════════════════════════════════════════════════════
// (ii)/(iii) PARITY — deterministic commands are byte-identical regardless of the
// seed store (the tier is unreachable from callers/callees/path/trust, and never
// fires on a RESOLVED explain). Doctor's PRE-EXISTING facts are unchanged; only the
// additive `seed` block differs. (review-4 #3)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn deterministic_commands_are_byte_identical_with_and_without_a_seed_store() {
    let port = spawn_embed_server();
    let _env = SeedEnv::with_endpoint(&format!("http://127.0.0.1:{port}/v1/embeddings"));
    let (d, _root) = isolated();
    let repo = make_repo();
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let (db_path, repo_uid) = coords(&idx);
    let repo_s = repo.path().to_string_lossy().to_string();

    // Commands that NEVER invoke the semantic tier (callers/callees/path/trust) plus a
    // RESOLVED explain (the tier can fire on explain, but ONLY on no_match — a resolved
    // target must be byte-unperturbed). Each must be byte-identical whether or not a
    // seed store exists on disk.
    let cases: Vec<(&str, Value)> = vec![
        ("explain", json!({ "repo": repo_s, "target": "helper.ts" })),
        (
            "callers",
            json!({ "repo": repo_s, "symbol": "helperFunction" }),
        ),
        ("callees", json!({ "repo": repo_s, "symbol": "mainEntry" })),
        (
            "path",
            json!({ "repo": repo_s, "from": "mainEntry", "to": "helperFunction" }),
        ),
        ("trust", json!({ "repo": repo_s })),
    ];

    // Publish the store, snapshot the bytes, then toggle the sidecar FILE around the
    // two runs (a plain file op — no raw DB connection to contend with the daemon's
    // cached one; the tier's with/without-store behavior is exactly what varies).
    publish_store(&db_path, &repo_uid, repo.path());
    let sidecar =
        repo_graph_daemon_runtime::seed::sidecar_path(Path::new(&db_path)).expect("sidecar path");
    let store_bytes = std::fs::read(&sidecar).expect("store published");

    for (method, params) in &cases {
        std::fs::write(&sidecar, &store_bytes).expect("restore sidecar");
        let with = dispatch_value(&d, method, params.clone());
        std::fs::remove_file(&sidecar).expect("remove sidecar");
        let without = dispatch_value(&d, method, params.clone());
        assert_eq!(
            serde_json::to_string(&with).unwrap(),
            serde_json::to_string(&without).unwrap(),
            "{method} output must be byte-identical with and without a seed store"
        );
    }

    // Doctor (storage_health): the PRE-EXISTING facts must be byte-identical; only the
    // additive `seed` block flips with store presence (that is the whole feature). So
    // strip `seed` and compare the remainder.
    let strip_seed = |mut v: Value| -> Value {
        if let Some(obj) = v.as_object_mut() {
            obj.remove("seed");
        }
        v
    };
    std::fs::write(&sidecar, &store_bytes).expect("restore sidecar");
    let health_with = strip_seed(dispatch_value(
        &d,
        "storage_health",
        json!({ "path": repo_s }),
    ));
    std::fs::remove_file(&sidecar).expect("remove sidecar");
    let health_without = strip_seed(dispatch_value(
        &d,
        "storage_health",
        json!({ "path": repo_s }),
    ));
    assert_eq!(
        serde_json::to_string(&health_with).unwrap(),
        serde_json::to_string(&health_without).unwrap(),
        "pre-existing doctor facts must be byte-identical; only the additive seed block may differ"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// (v) find OWNING MODULE — a GENUINE `module_file_ownership` row surfaces as
// ModuleHint::Owning (operator ruling 2026-08-25), not the unavailable shape.
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn find_attaches_genuine_owning_module_from_ownership_row() {
    let port = spawn_embed_server();
    let _env = SeedEnv::with_endpoint(&format!("http://127.0.0.1:{port}/v1/embeddings"));
    let (d, _root) = isolated();
    let repo = make_repo();
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let (db_path, repo_uid) = coords(&idx);
    let snapshot_uid = idx["snapshot_uid"]
        .as_str()
        .expect("index returns snapshot_uid")
        .to_string();

    // Resolve helper.ts's real file_uid from the corpus, then insert a GENUINE
    // ownership row (+ its module_candidate) mapping it to a module whose display
    // path is "backend/core" — exactly what the enrich pass would populate. We insert
    // it directly (enrich is off in this binary) to keep the seam hermetic. No schema
    // change: this is DATA, in-scope per the operator ruling.
    let corpus = StorageConnection::open(&db_path).unwrap();
    let entries = corpus.seed_corpus(&repo_uid).unwrap();
    let helper_uid = entries
        .iter()
        .find(|e| e.path.ends_with("helper.ts"))
        .expect("helper.ts in corpus")
        .file_uid
        .clone();
    drop(corpus);
    // Raw connection for the insert (`StorageConnection::connection` is pub(crate) — a
    // deliberate storage boundary we do not widen). WAL + a busy_timeout let this write
    // commit while the daemon's cached idle connection is open; the daemon sees the
    // committed rows on its next read. The FK parents (repo, snapshot) already exist
    // from `index`; we add only the two module rows.
    let raw = rusqlite::Connection::open(&db_path).expect("raw open for ownership insert");
    raw.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
    raw.execute_batch(&format!(
        "INSERT INTO module_candidates \
           (module_candidate_uid, snapshot_uid, repo_uid, module_key, module_kind, canonical_root_path, confidence) \
         VALUES ('mc-seed-test', '{snapshot_uid}', '{repo_uid}', 'backend/core', 'inferred', 'backend/core', 1.0); \
         INSERT INTO module_file_ownership \
           (snapshot_uid, repo_uid, file_uid, module_candidate_uid, assignment_kind, confidence) \
         VALUES ('{snapshot_uid}', '{repo_uid}', '{helper_uid}', 'mc-seed-test', 'exact', 1.0);"
    ))
    .expect("insert genuine ownership rows");
    drop(raw); // release before the daemon reads

    publish_store(&db_path, &repo_uid, repo.path());

    let resp = dispatch_ok(
        &d,
        "find",
        json!({ "repo": repo.path().to_string_lossy(), "query": "helper" }),
    );
    let cands = resp["candidates"].as_array().expect("candidates present");
    let helper_cand = cands
        .iter()
        .find(|c| {
            c["path"]
                .as_str()
                .map(|p| p.ends_with("helper.ts"))
                .unwrap_or(false)
        })
        .expect("helper.ts is among the find candidates");
    // The GENUINE ownership row must surface as ModuleHint::Owning("backend/core") —
    // the externally-tagged `{"owning": "..."}` shape, NEVER the unavailable shape and
    // NEVER a directory guess.
    assert_eq!(
        helper_cand["module"]["owning"].as_str(),
        Some("backend/core"),
        "a real ownership row becomes ModuleHint::Owning: {helper_cand}"
    );
    assert!(
        helper_cand["module"].get("unavailable").is_none(),
        "an owned file is never also unavailable: {helper_cand}"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// (§9) DOCTOR HONESTY — a DEGRADED seed store never fabricates measured-zero counts
// (review-5 #1). Only a successful decode + corpus evaluation yields stale/total.
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn doctor_degraded_store_omits_fabricated_counts() {
    let port = spawn_embed_server();
    let _env = SeedEnv::with_endpoint(&format!("http://127.0.0.1:{port}/v1/embeddings"));
    let (d, _root) = isolated();
    let repo = make_repo();
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let (db_path, repo_uid) = coords(&idx);

    // ── (a) PRESENT store: counts ARE reported (measured). ──
    publish_store(&db_path, &repo_uid, repo.path());
    let health = dispatch_ok(
        &d,
        "storage_health",
        json!({ "path": repo.path().to_string_lossy() }),
    );
    let seed = health.get("seed").expect("seed block present");
    assert_eq!(seed.get("state").and_then(|v| v.as_str()), Some("present"));
    assert!(
        seed.get("total").and_then(|v| v.as_u64()).is_some(),
        "a present store reports a measured total: {seed}"
    );

    // ── (b) DEGRADE it via a pin mismatch — `store::decode` fails BEFORE any corpus
    //     freshness evaluation, so stale/total are UNKNOWN. They must be OMITTED
    //     (serialized as absent), never rendered as a fabricated measured-zero. ──
    std::env::set_var("RMAP_SEED_MODEL_ID", "a-different-model");
    let health = dispatch_ok(
        &d,
        "storage_health",
        json!({ "path": repo.path().to_string_lossy() }),
    );
    let seed = health.get("seed").expect("seed block present");
    assert_eq!(
        seed.get("state").and_then(|v| v.as_str()),
        Some("degraded"),
        "pin mismatch ⇒ degraded: {seed}"
    );
    assert!(
        seed.get("degraded_reason")
            .and_then(|v| v.as_str())
            .is_some(),
        "a degraded state always carries an honest reason: {seed}"
    );
    // The load-bearing honesty assertion (review-5 #1): NO fabricated 0-of-0.
    assert!(
        seed.get("stale_count").map(|v| v.is_null()).unwrap_or(true),
        "degraded ⇒ stale_count absent/null (unknown, never measured-zero): {seed}"
    );
    assert!(
        seed.get("total").map(|v| v.is_null()).unwrap_or(true),
        "degraded ⇒ total absent/null (unknown, never measured-zero): {seed}"
    );
}
