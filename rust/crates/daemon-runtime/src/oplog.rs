//! DAEMON-CRASH-RECOVERY-1 (F8): operation-lifecycle lines in the daemon LOG.
//!
//! # Why this module exists (abstraction ledger)
//!
//! - **What:** the small set of reader-frame lines a WRITE op (`index` / `refresh` / `enrich` /
//!   `retention`) and the boot/load reconciliation write to the daemon stderr log — a `started`
//!   line, optional coarse `phase` lines, and a terminal `outcome` line
//!   (completed / interrupted / failed+reason) — PLUS a parallel-safe test-capture seam so a named
//!   test can DIRECTLY observe the lines (the same shape as `detached.rs`).
//! - **Concrete current users:** `dispatch::handle_index`, `dispatch::handle_refresh`,
//!   `retention_pass`, `enrich_pass` (a boundary log line only — NOT its semantics), and
//!   `reconcile` (the boot/load repair line). Five call sites, one line shape.
//! - **Named axis of variation:** the op label + the outcome text; nothing else varies.
//! - **Rejected simpler alternative:** inline `eprintln!` at each site — duplicates the exact
//!   reader-frame format across five files AND has nowhere parallel-safe to host the capture seam
//!   except the 8000-line `dispatch.rs` (structural guardrail: do not grow mixed-responsibility
//!   files). Process-global fd-2 capture was ALSO rejected (parallel-unsafe under cargo's in-process
//!   test threads) — same reasoning as `detached.rs`.
//!
//! ## Why the LOG, not doctor (VISION honesty note)
//!
//! The field incident's daemon log for the WHOLE incident was "startup + three broken-pipe lines":
//! forensics of a crashed op depended on `doctor` being reachable, and a crashed daemon's doctor is
//! not. These lines make the op lifecycle legible from the log alone — the ONE surface that survives
//! the daemon. Low-volume by construction (start + outcome per op; no per-file spam).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// The reader-frame line logged when a write op STARTS. `snapshot_uid` is `None` for `index`, whose
/// snapshot is created inside the extractor (not known at handler entry) — the outcome line carries it.
pub fn op_start_line(op: &str, repo: &str, snapshot_uid: Option<&str>) -> String {
    match snapshot_uid {
        Some(uid) => format!("op {op} started (repo {repo}, snapshot {uid})"),
        None => format!("op {op} started (repo {repo})"),
    }
}

/// A coarse phase-transition line (e.g. `extracting` → `postpass` → `finalizing`). Optional and
/// low-volume: only the handful of coarse phases, never per-file.
pub fn op_phase_line(op: &str, repo: &str, phase: &str) -> String {
    format!("op {op} phase {phase} (repo {repo})")
}

/// The reader-frame line logged when a write op ENDS. `outcome` is the terminal disposition — e.g.
/// `"completed"`, `"interrupted (daemon restart)"`, or `"failed: <reason>"`.
pub fn op_outcome_line(op: &str, repo: &str, snapshot_uid: Option<&str>, outcome: &str) -> String {
    match snapshot_uid {
        Some(uid) => format!("op {op} {outcome} (repo {repo}, snapshot {uid})"),
        None => format!("op {op} {outcome} (repo {repo})"),
    }
}

// ── Emit + test-capture seam (mirrors `detached.rs`) ────────────────────────────────────────────
//
// Production leaves capture OFF: each `log_*` call then costs one relaxed atomic load and NEVER
// touches the recorder, so a long-lived daemon has no growth. A test opts in with
// `enable_oplog_capture_for_test`, drives an op, and reads the exact recorded lines back — filtering
// by its own UNIQUE `repo` label, which is what makes the shared recorder parallel-safe.
static CAPTURE_ENABLED: AtomicBool = AtomicBool::new(false);
static CAPTURED: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn emit(line: String) {
    eprintln!("{line}");
    if CAPTURE_ENABLED.load(Ordering::Relaxed) {
        if let Ok(mut buf) = CAPTURED.lock() {
            buf.push(line);
        }
    }
}

/// Log an op-START line to the daemon stderr log (best-effort).
pub fn log_op_start(op: &str, repo: &str, snapshot_uid: Option<&str>) {
    emit(op_start_line(op, repo, snapshot_uid));
}

/// Log a coarse op-PHASE line to the daemon stderr log (best-effort).
pub fn log_op_phase(op: &str, repo: &str, phase: &str) {
    emit(op_phase_line(op, repo, phase));
}

/// Log an op-OUTCOME line to the daemon stderr log (best-effort).
pub fn log_op_outcome(op: &str, repo: &str, snapshot_uid: Option<&str>, outcome: &str) {
    emit(op_outcome_line(op, repo, snapshot_uid, outcome));
}

/// TEST SEAM — start recording op-lifecycle lines for later inspection. Idempotent. `#[doc(hidden)]`
/// and `_for_test`-named: no production caller (the daemon never enables capture, so production stays
/// at one atomic load with no recording).
#[doc(hidden)]
pub fn enable_oplog_capture_for_test() {
    CAPTURE_ENABLED.store(true, Ordering::Relaxed);
}

/// TEST SEAM — a NON-draining snapshot of every recorded op-lifecycle line so far. Kept non-draining
/// so parallel tests never steal each other's lines; each test filters by its own unique `repo`.
#[doc(hidden)]
pub fn oplog_lines_for_test() -> Vec<String> {
    CAPTURED.lock().map(|b| b.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_line_names_op_repo_and_optional_snapshot() {
        assert_eq!(
            op_start_line("index", "repo_a", None),
            "op index started (repo repo_a)"
        );
        assert_eq!(
            op_start_line("refresh", "repo_b", Some("repo_b/ts/abc")),
            "op refresh started (repo repo_b, snapshot repo_b/ts/abc)"
        );
    }

    #[test]
    fn outcome_line_names_the_disposition() {
        assert_eq!(
            op_outcome_line("index", "repo_a", Some("s1"), "completed"),
            "op index completed (repo repo_a, snapshot s1)"
        );
        assert_eq!(
            op_outcome_line(
                "index",
                "repo_a",
                Some("s1"),
                "interrupted (daemon restart)"
            ),
            "op index interrupted (daemon restart) (repo repo_a, snapshot s1)"
        );
        assert_eq!(
            op_outcome_line("refresh", "repo_a", None, "failed: disk full"),
            "op refresh failed: disk full (repo repo_a)"
        );
    }

    #[test]
    fn phase_line_is_coarse() {
        assert_eq!(
            op_phase_line("index", "repo_a", "postpass"),
            "op index phase postpass (repo repo_a)"
        );
    }
}
