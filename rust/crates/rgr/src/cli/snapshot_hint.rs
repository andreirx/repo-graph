//! DAEMON-VISIBILITY-1 (F2), client side: the honest "no READY snapshot" message for the
//! DIRECT-STORAGE commands (`rmap enrich`, `rmap metrics`, `rmap modules boundary`).
//!
//! # Why this module exists (abstraction ledger)
//!
//! - **What:** one reader-frame message for the three direct-storage client commands that require a
//!   READY snapshot. When none exists it NAMES any interrupted partial (its state, when it was
//!   created, and the repo's on-disk size) and gives BOTH next actions (`rmap index` /
//!   `rmap maintenance prune`) — never a bare "no snapshot found".
//! - **Concrete current users:** `commands::enrich`, `commands::quality::metrics`,
//!   `commands::modules::boundary` — the three `open_storage` + `get_latest_snapshot` → `Ok(None)`
//!   sites (deterministic grep over `rust/crates/rgr/src/`: these are the only direct-storage client
//!   paths that emit the bare none-message).
//! - **Named axis of variation:** none imagined; it exists only because THREE commands need the same
//!   non-trivial mapping (`status` + `created_at` + DB size → reader-frame message) and inlining
//!   would triplicate the reader-facing honesty wording.
//! - **Rejected simpler alternatives:** (a) inline in all three → triplicates the wording (drift
//!   risk on an honesty message); (b) call `daemon_runtime::snapshot_facts::no_ready_snapshot_message`
//!   → rejected per review-5's ratified guidance ("smallest LOCAL implementation … a shared
//!   cross-crate formatter is not required"), and because these commands are DIRECT-STORAGE, not
//!   daemon-mediated: coupling their error text to a module named `daemon_runtime` would be a
//!   misleading dependency (name-semantics). The F2 CONTRACT — state + size + both next actions —
//!   is what the client and daemon surfaces keep aligned; each is test-guarded, not the exact bytes.
//!
//! ## Honesty notes (VISION)
//!
//! - **Reader frame, not our pipeline.** The message describes the reader's repo ("a snapshot … was
//!   not completed", "this repo holds 4.0 GB on disk"), never "get_latest_snapshot returned None".
//! - **State is a Layer-1 derived label** from `snapshot.status` alone. A direct-storage client has
//!   no daemon activity registry to consult, so a `building` row is reported as *interrupted* — the
//!   overwhelmingly common cause when a direct-storage read finds a non-READY snapshot (an index
//!   that never finalized). This mirrors the daemon's own F2 message, so the reader sees the same
//!   language whichever surface answered.
//! - **Size is the whole-repo DB file**, stated as such ("this repo holds … on disk") — we do not
//!   claim a per-snapshot byte figure the schema does not track.
//! - **Never-indexed is not gaslit as a partial.** With no snapshot at all, the message is the plain
//!   "index it first" — the honest thing to say when there is genuinely nothing to reclaim.

use std::path::Path;

use repo_graph_storage::types::Snapshot;
use repo_graph_storage::StorageConnection;

/// The honest "no READY snapshot" message for a direct-storage command whose latest-READY lookup
/// returned `Ok(None)`. Queries the newest snapshot of ANY state and stats the DB file, then
/// delegates the wording to the pure [`partial_snapshot_hint`].
///
/// Read-only. A failed any-state query (corrupt DB) degrades to the plain "index it first" fallback
/// rather than surfacing an internal error — the caller has already opened the DB successfully, so
/// this path is reached only when the repo simply has no READY snapshot.
pub fn no_ready_snapshot_hint(
    storage: &StorageConnection,
    db_path: &Path,
    repo_uid: &str,
) -> String {
    let latest = storage
        .get_latest_snapshot_any_state(repo_uid)
        .ok()
        .flatten();
    let db_size = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);
    partial_snapshot_hint(latest.as_ref(), repo_uid, db_size)
}

/// Pure formatter for the F2 message (unit-testable without a DB).
///
/// `latest` is the newest snapshot of ANY state for the repo, or `None` when the repo was never
/// indexed.
fn partial_snapshot_hint(latest: Option<&Snapshot>, repo_uid: &str, db_size_bytes: u64) -> String {
    match latest {
        Some(snap) if snap.status != "ready" => format!(
            "no READY snapshot for repo '{repo_uid}', but a snapshot from {created} exists that was \
             not completed (state: {state}; this repo holds {size} on disk). The last index did not \
             finalize. Re-run `rmap index` to build a fresh snapshot; the interrupted snapshot is \
             listed by `rmap maintenance prune`.",
            created = snap.created_at,
            state = non_ready_state_label(&snap.status),
            size = format_size_bytes(db_size_bytes),
        ),
        // Genuinely never indexed (no snapshot at all), or a race that left only a READY row.
        _ => format!("no snapshot for repo '{repo_uid}' yet. Index it first with `rmap index`."),
    }
}

/// Reader-frame label for a non-READY snapshot seen by a direct-storage client. The `status` column
/// is unconstrained TEXT; an unrecognised value is surfaced as an honest "unknown" rather than
/// guessed. (No `building`-as-"in progress" case: a direct-storage client has no activity registry,
/// so it cannot claim a live index — the daemon's status surface owns that distinction.)
fn non_ready_state_label(status: &str) -> &'static str {
    match status {
        // A `building`/`failed` row with no daemon writing it never finalized — the day-2 field case.
        "building" | "failed" => "interrupted",
        "stale" => "superseded",
        _ => "unknown",
    }
}

/// Humanise a byte count (1024-based), mirroring the daemon/doctor size scale so the reader sees a
/// consistent magnitude across surfaces.
fn format_size_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(status: &str) -> Snapshot {
        Snapshot {
            snapshot_uid: format!("uid/{status}"),
            repo_uid: "r1".to_string(),
            parent_snapshot_uid: None,
            kind: "full".to_string(),
            basis_ref: None,
            basis_commit: None,
            dirty_hash: None,
            status: status.to_string(),
            files_total: 160_000,
            nodes_total: 5,
            edges_total: 7,
            created_at: "2026-07-02T10:00:00Z".to_string(),
            completed_at: None,
            label: None,
            toolchain_json: None,
        }
    }

    // F2: a non-READY partial is NAMED (state + when + size) with BOTH next actions — never the bare
    // "no snapshot found" the three commands used to print.
    #[test]
    fn partial_hint_names_the_interrupted_snapshot_and_both_actions() {
        let msg = partial_snapshot_hint(Some(&snap("building")), "r1", 4_000_000_000);
        assert!(msg.contains("interrupted"), "names the state: {msg}");
        assert!(msg.contains("2026-07-02"), "names when created: {msg}");
        assert!(msg.contains("GB"), "names the on-disk size: {msg}");
        assert!(msg.contains("on disk"), "reader-frame size phrase: {msg}");
        assert!(
            msg.contains("rmap index"),
            "next action 1 (re-index): {msg}"
        );
        assert!(
            msg.contains("rmap maintenance prune"),
            "next action 2 (reclaim): {msg}"
        );
        assert!(
            !msg.contains("no snapshot found"),
            "must NOT be the bare gaslighting message: {msg}"
        );
    }

    #[test]
    fn failed_status_is_interrupted() {
        let msg = partial_snapshot_hint(Some(&snap("failed")), "r1", 1024);
        assert!(msg.contains("interrupted"), "{msg}");
    }

    #[test]
    fn stale_status_is_superseded() {
        let msg = partial_snapshot_hint(Some(&snap("stale")), "r1", 1024);
        assert!(msg.contains("superseded"), "{msg}");
    }

    // A repo that was never indexed has no partial to reclaim → the plain "index it first", not a
    // fabricated interrupted-partial and not the bare "no snapshot found".
    #[test]
    fn never_indexed_falls_back_to_plain_index_first() {
        let msg = partial_snapshot_hint(None, "r1", 0);
        assert!(msg.contains("no snapshot for repo 'r1'"), "{msg}");
        assert!(msg.contains("rmap index"), "{msg}");
        assert!(!msg.contains("interrupted"), "no partial to name: {msg}");
        assert!(!msg.contains("on disk"), "no partial to size: {msg}");
    }

    #[test]
    fn format_size_bytes_scales() {
        assert_eq!(format_size_bytes(4_000_000_000), "3.7 GB");
        assert_eq!(format_size_bytes(2_097_152), "2.0 MB");
        assert_eq!(format_size_bytes(16_384), "16.0 KB");
        assert_eq!(format_size_bytes(512), "512 B");
    }
}
