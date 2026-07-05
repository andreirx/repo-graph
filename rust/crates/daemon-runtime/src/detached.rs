//! INDEX-DISCONNECT-1: the "client vanished; write op continues detached" notice.
//!
//! # Why this module exists (abstraction ledger)
//!
//! - **What:** the single reader-frame line a WRITE op logs ONCE when its client disconnects
//!   mid-op and the op continues detached, PLUS a parallel-safe test-capture seam so a named test
//!   can DIRECTLY observe that line (review-0 required change #3: the emit-call count alone is not
//!   the required log proof).
//! - **Concrete current users:** `dispatch::handle_index` (op `"index"`) and
//!   `dispatch::handle_refresh` (op `"refresh"`) — two write handlers, one message shape.
//! - **Named axis of variation:** the op label (`"index"` vs `"refresh"`); nothing else varies.
//! - **Rejected simpler alternative:** inline `eprintln!` in both handlers — duplicates the exact
//!   reader-frame format across two sites AND has nowhere parallel-safe to host the capture seam
//!   except the 8000-line `dispatch.rs` (structural guardrail: do not grow mixed-responsibility
//!   files). In-process fd-2 stderr capture was ALSO rejected: fd 2 is process-global, so it is
//!   parallel-unsafe under cargo's in-process test threads (it would capture other tests' stderr).
//!
//! ## Honesty note (VISION — "labels speak the reader's language")
//!
//! The line describes the READER's situation ("client disconnected; index continues detached"),
//! not our pipeline internals. It is the ratified reader-frame from the slice contract (§3.1).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// The exact reader-frame line logged when a write op's client disconnects and the op continues
/// detached. Pure (no I/O) so its wording is unit-testable without capturing stderr.
pub fn detached_continuation_notice(op: &str, repo_uid: &str) -> String {
    format!("client disconnected; {op} continues detached (repo {repo_uid})")
}

// ── Test-capture seam (mirrors `cancel::set_heartbeat_interval_ms_for_test`) ────────────────────
//
// Production leaves capture OFF: `log_detached_continuation` then costs one relaxed atomic load and
// NEVER touches the recorder, so a long-lived daemon has no growth. A test opts in with
// `enable_detached_capture_for_test`, runs the op, and reads the exact recorded lines back —
// filtering by its own UNIQUE `repo_uid`, which is what makes the shared recorder parallel-safe.
static CAPTURE_ENABLED: AtomicBool = AtomicBool::new(false);
static CAPTURED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Emit the detached-continuation notice to the daemon stderr log (best-effort, ONCE per op — the
/// caller gates it behind its `client_gone` latch), and — only when a test has enabled capture —
/// record the exact line for direct observation.
pub fn log_detached_continuation(op: &str, repo_uid: &str) {
    let line = detached_continuation_notice(op, repo_uid);
    eprintln!("{line}");
    if CAPTURE_ENABLED.load(Ordering::Relaxed) {
        if let Ok(mut buf) = CAPTURED.lock() {
            buf.push(line);
        }
    }
}

/// TEST SEAM — start recording detached-continuation lines for later inspection. Idempotent.
/// `#[doc(hidden)]` and `_for_test`-named: no production caller (the daemon never enables capture,
/// so production stays at one atomic load with no recording).
#[doc(hidden)]
pub fn enable_detached_capture_for_test() {
    CAPTURE_ENABLED.store(true, Ordering::Relaxed);
}

/// TEST SEAM — a NON-draining snapshot of every recorded detached-continuation line so far. Kept
/// non-draining so parallel tests never steal each other's lines; each test filters by its own
/// `repo_uid`.
#[doc(hidden)]
pub fn detached_continuations_for_test() -> Vec<String> {
    CAPTURED.lock().map(|b| b.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notice_is_reader_framed_and_names_op_and_repo() {
        // The ratified reader-frame text (slice §3.1), for both write ops.
        assert_eq!(
            detached_continuation_notice("index", "repo_abc"),
            "client disconnected; index continues detached (repo repo_abc)"
        );
        assert_eq!(
            detached_continuation_notice("refresh", "repo_xyz"),
            "client disconnected; refresh continues detached (repo repo_xyz)"
        );
    }
}
