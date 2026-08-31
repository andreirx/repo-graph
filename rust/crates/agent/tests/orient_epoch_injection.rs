//! W-B-EPOCH-IMPL-1: the orient double-resolve removal.
//!
//! The orient use case used to call `get_latest_snapshot` itself (`orient/repo.rs:99`), so a daemon orient
//! request resolved the snapshot TWICE (the handler's serve-decision + the use case). This slice deletes
//! the use case's resolve and threads the pinned `&AgentSnapshot` in. These tests prove:
//!   1. `orient_cancellable` (the daemon's entry) consumes the INJECTED snapshot and never resolves
//!      "latest" (count stays 0) — the deleted re-resolve is genuinely gone, and the answer is coherent at
//!      the injected snapshot (stamp + SNAPSHOT_INFO both ride it, not a fallback re-resolve).
//!   2. The `orient()` wrapper (the CLI/test boundary) resolves exactly once.

mod common;

use common::FakeAgentStorage;
use repo_graph_agent::{orient, orient_cancellable, AgentSnapshot, Budget};

/// A pinned snapshot DISTINCT from whatever the fake resolves as "latest" — distinct uid AND a unique
/// `basis_commit` — so any re-resolve (which would read the fake's `snap-LATEST`, whose basis_commit is
/// `None`) is observable in BOTH the stamp and the SNAPSHOT_INFO evidence.
fn pinned_snapshot() -> AgentSnapshot {
    AgentSnapshot {
        snapshot_uid: "snap-PINNED".to_string(),
        repo_uid: "r1".to_string(),
        scope: "full".to_string(),
        basis_commit: Some("commit-PINNED".to_string()),
        created_at: "2026-01-02T00:00:00Z".to_string(),
        files_total: 7,
        nodes_total: 11,
        edges_total: 13,
    }
}

#[test]
fn orient_use_case_consumes_injected_snapshot_without_re_resolving() {
    let mut fake = FakeAgentStorage::new();
    // The fake's "latest" is a DIFFERENT snapshot (basis_commit None) — a re-resolve would stamp this.
    fake.seed_minimal_repo("r1", "my-repo", "snap-LATEST");

    let pinned = pinned_snapshot();
    let result = orient_cancellable(
        &fake,
        "r1",
        &pinned,
        None,
        Budget::Small,
        common::TEST_NOW,
        None, // enrich_state_override (ORIENT-FACT-COHERENCE-1: no daemon coordinator in this unit test)
        &mut || std::ops::ControlFlow::Continue(()),
    )
    .expect("orient_cancellable ok");

    // (1) The use case did NOT resolve the snapshot — the double-resolve (orient/repo.rs:99) is GONE.
    assert_eq!(
        fake.get_latest_snapshot_calls.get(),
        0,
        "the orient use case must consume the injected snapshot, NEVER call get_latest_snapshot"
    );
    // (2) The answer is stamped with the INJECTED (pinned) snapshot, not the fake's "latest".
    assert_eq!(
        result.snapshot, "snap-PINNED",
        "the stamp is the injected snapshot's uid (no re-resolve to snap-LATEST)"
    );
    // (3) SNAPSHOT_INFO rides the injected snapshot's metadata — `snapshot::aggregate` aggregated the
    //     injected DTO, not a re-resolved/fetched-latest one (whose basis_commit is None).
    let json = serde_json::to_value(&result).expect("serialize");
    let snap_info = json["signals"]
        .as_array()
        .expect("signals array")
        .iter()
        .find(|s| s["code"] == "SNAPSHOT_INFO")
        .expect("SNAPSHOT_INFO signal present");
    assert_eq!(
        snap_info["evidence"]["basis_commit"], "commit-PINNED",
        "SNAPSHOT_INFO carries the INJECTED snapshot's basis_commit (proves no fallback re-resolve)"
    );
}

#[test]
fn orient_wrapper_resolves_the_snapshot_exactly_once() {
    let mut fake = FakeAgentStorage::new();
    fake.seed_minimal_repo("r1", "my-repo", "snap-1");

    let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).expect("orient ok");

    // The non-cancellable wrapper is the single resolve boundary: ONE get_latest_snapshot for the whole
    // request (was two — wrapper-less use-case resolve + the daemon serve-decision — in the daemon path).
    assert_eq!(
        fake.get_latest_snapshot_calls.get(),
        1,
        "the orient() wrapper resolves the pinned snapshot exactly once"
    );
    assert_eq!(result.snapshot, "snap-1");
}
