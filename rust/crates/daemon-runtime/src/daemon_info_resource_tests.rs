//! DOCTOR-RESOURCE-REPORT: `daemon_info` resource fields, proven through the full
//! dispatch path (not the handler in isolation).
//!
//! Asserts that a live `daemon_info` dispatch returns `rss_bytes` (the daemon's REAL
//! current resident memory — non-zero on macOS/Linux; the headline "did the LiveGraph
//! substrate balloon?" figure, proven real, not a placeholder), `rss_peak_bytes`
//! (non-zero peak via getrusage on unix), and `databases_total_bytes` + `repo_count`
//! (sourced from the daemon's own registry/state root) — and that the pre-existing
//! fields are still present (additive, no regression).

use std::sync::Arc;

use serde_json::{json, Value};
use tempfile::tempdir;

use repo_graph_daemon_transport::{DispatchResult, Dispatcher, NoOpEmitter, Request};

use crate::dispatch::ServiceDispatcher;
use crate::registry::RepoRegistry;
use crate::state::DaemonState;

/// Dispatch a real `daemon_info` request against a hermetic state root and return
/// the success result value.
fn dispatch_daemon_info(state_root: &std::path::Path) -> Value {
    let registry = RepoRegistry::with_state_root(state_root).expect("registry");
    // DaemonState is !Send/!Sync (interior mutability); Arc is shared ownership for a
    // single-threaded daemon, matching `run_daemon`.
    #[allow(clippy::arc_with_non_send_sync)]
    let state = Arc::new(DaemonState::with_registry(registry));
    let dispatcher = ServiceDispatcher::new(state);

    let request = Request {
        id: "test-daemon-info".to_string(),
        method: "daemon_info".to_string(),
        params: json!({}),
    };

    let mut emitter = NoOpEmitter;
    match dispatcher.dispatch(&request, &mut emitter) {
        DispatchResult::Success(resp) => resp.result,
        DispatchResult::Error(e) => panic!("daemon_info errored: {:?}", e.error),
    }
}

#[test]
fn daemon_info_carries_real_resource_fields() {
    let dir = tempdir().unwrap();
    let result = dispatch_daemon_info(dir.path());

    // ── Additive: pre-existing fields preserved (no regression) ─────────────
    assert!(
        result
            .get("authority_writes_allowed")
            .and_then(Value::as_bool)
            .is_some(),
        "authority_writes_allowed must remain present: {result}"
    );
    assert!(
        result.get("state_root").and_then(Value::as_str).is_some(),
        "state_root must remain present: {result}"
    );

    // ── databases_total_bytes + repo_count: from the daemon's own state root ─
    // `with_state_root` creates an empty `databases/`, so the sum is a real zero
    // (known-zero, NOT null) and the registry is empty.
    assert_eq!(
        result.get("databases_total_bytes").and_then(Value::as_u64),
        Some(0),
        "empty databases/ must report a real 0, not null: {result}"
    );
    assert_eq!(
        result.get("repo_count").and_then(Value::as_u64),
        Some(0),
        "empty registry must report 0 repos: {result}"
    );

    // ── rss_peak_bytes: getrusage works on every unix target ────────────────
    #[cfg(unix)]
    {
        let peak = result
            .get("rss_peak_bytes")
            .and_then(Value::as_u64)
            .expect("rss_peak_bytes present on unix");
        assert!(
            peak > 0,
            "peak RSS must be a real non-zero value, got {peak}"
        );
    }

    // ── rss_bytes: REAL current resident memory on the supported platforms ──
    // This is the slice's headline proof: the field is populated and non-zero
    // when the daemon process runs.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let rss = result
            .get("rss_bytes")
            .and_then(Value::as_u64)
            .expect("rss_bytes present on macOS/Linux");
        assert!(
            rss > 0,
            "current RSS must be a real non-zero footprint, got {rss}"
        );
    }
}

#[test]
fn databases_total_reflects_real_db_files() {
    // A populated `databases/` must sum to the real on-disk size (not a stub).
    let dir = tempdir().unwrap();
    let db_dir = dir.path().join("databases");
    std::fs::create_dir_all(&db_dir).unwrap();
    std::fs::write(db_dir.join("aaaa.db"), vec![0u8; 4096]).unwrap();
    std::fs::write(db_dir.join("aaaa.db-wal"), vec![0u8; 1024]).unwrap();

    let result = dispatch_daemon_info(dir.path());
    assert_eq!(
        result.get("databases_total_bytes").and_then(Value::as_u64),
        Some(5120),
        "total must equal the real summed db-dir size: {result}"
    );
}
