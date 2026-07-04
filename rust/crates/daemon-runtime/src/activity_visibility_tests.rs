//! DAEMON-VISIBILITY-1 named proofs (D status surface + E contention truth), through the real
//! dispatch/handler path.
//!
//! - **Status proof:** an in-flight write op is reported by `daemon_info` (op kind / repo / phase /
//!   counters); idle reports an empty activity list.
//! - **Doctor-contention proof:** while the daemon holds a DB (an active write op), the snapshot
//!   facts report healthy "in use by daemon" — NOT a busy-open error.

use std::sync::Arc;

use serde_json::{json, Value};
use tempfile::tempdir;

use repo_graph_daemon_transport::{DispatchResult, Dispatcher, NoOpEmitter, Request};

use crate::activity::OpKind;
use crate::dispatch::ServiceDispatcher;
use crate::registry::RepoRegistry;
use crate::state::DaemonState;

fn dispatch_daemon_info(dispatcher: &ServiceDispatcher) -> Value {
    let request = Request {
        id: "test-activity".to_string(),
        method: "daemon_info".to_string(),
        params: json!({}),
    };
    let mut emitter = NoOpEmitter;
    match dispatcher.dispatch(&request, &mut emitter) {
        DispatchResult::Success(resp) => resp.result,
        DispatchResult::Error(e) => panic!("daemon_info errored: {:?}", e.error),
    }
}

/// Status proof: `daemon_info` reports the in-flight op (kind/repo/phase/counters) while it runs,
/// and an empty activity list when idle. This is the fact `rmap doctor` renders as
/// "indexing <repo>: extraction 42k/160k, started …" vs "idle".
#[test]
fn daemon_info_reports_in_flight_activity_then_idle() {
    let dir = tempdir().unwrap();
    let registry = RepoRegistry::with_state_root(dir.path()).expect("registry");
    let state = Arc::new(DaemonState::with_registry(registry));
    let dispatcher = ServiceDispatcher::new(Arc::clone(&state));

    // Idle first: known-empty activity (never a false "nothing happening" omission).
    let idle = dispatch_daemon_info(&dispatcher);
    assert_eq!(
        idle["active_operations"].as_array().map(|a| a.len()),
        Some(0),
        "idle daemon reports an empty activity list: {idle}"
    );

    // In-flight: stamp an index op with live progress, then observe it via the dispatch path.
    {
        let op = state.activity().begin(
            OpKind::Index,
            "/repos/big",
            Some("uid-1".to_string()),
            "/db/big.db",
        );
        op.update("extracting", 42_000, 160_000);

        let busy = dispatch_daemon_info(&dispatcher);
        let ops = busy["active_operations"].as_array().expect("array");
        assert_eq!(ops.len(), 1, "the in-flight index is reported: {busy}");
        assert_eq!(ops[0]["kind"], "index");
        assert_eq!(ops[0]["repo"], "/repos/big");
        assert_eq!(ops[0]["phase"], "extracting");
        assert_eq!(ops[0]["current"], 42_000);
        assert_eq!(ops[0]["total"], 160_000);
        assert!(
            ops[0].get("started_secs_ago").is_some(),
            "carries an elapsed for the 'started N ago' line"
        );
    }

    // Guard dropped → back to idle (completion is observable; the record does not leak).
    let idle_again = dispatch_daemon_info(&dispatcher);
    assert_eq!(
        idle_again["active_operations"].as_array().map(|a| a.len()),
        Some(0),
        "after the op completes the activity list is empty again: {idle_again}"
    );
}

/// Doctor-contention proof: while the daemon writes a DB (an active op on it), the snapshot facts
/// report healthy "in use by daemon" and do NOT attempt the (busy) open — the fix for the field bug
/// where a live daemon's own lock produced "error opening database". No real DB file is needed: the
/// in-use short-circuit fires before any open.
#[test]
fn snapshot_facts_report_in_use_by_daemon_not_error() {
    let dir = tempdir().unwrap();
    let registry = RepoRegistry::with_state_root(dir.path()).expect("registry");
    let state = DaemonState::with_registry(registry);
    let db_path = dir.path().join("big.db"); // deliberately not created

    let _op = state.activity().begin(
        OpKind::Index,
        "/repos/big",
        Some("uid-1".to_string()),
        db_path.clone(),
    );

    let facts = crate::snapshot_facts::collect_snapshot_facts(&state, &db_path, "uid-1");
    assert_eq!(
        facts["in_use_by_daemon"], true,
        "a DB held by a live daemon op is healthy-in-use, not an error: {facts}"
    );
    assert_eq!(facts["operation"]["kind"], "index");
    assert_eq!(facts["operation"]["repo"], "/repos/big");
    assert!(
        facts["snapshots"].is_null(),
        "snapshot detail is UNKNOWN (null) while the DB is written — not a false zero: {facts}"
    );
}
