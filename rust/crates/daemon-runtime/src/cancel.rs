//! Query-path cancellation seam (DAEMON-CANCEL-1, in-loop / Option A).
//!
//! B1 made the daemon concurrent (one thread per connection) but left a heavy
//! query running to completion even after its client disconnected — wasting a
//! handler thread on a result no one will read. This seam closes that for the two
//! genuinely-deep Rust loops: a disconnected peer's in-flight `cycles` (Tarjan SCC)
//! or `path` (LiveGraph BFS) is cancelled *while the heavy work is in flight*, not
//! only at the handler boundary before the work starts.
//!
//! ## The disconnect signal (D-K = K-A, reused D5b seam)
//!
//! Cancellation is driven by the SAME signal D5b uses for index/refresh abort: a
//! **transport-write failure**. While a query computes, the handler periodically
//! writes a heartbeat through the request's [`ProgressEmitter`]. Writing to a
//! peer-closed socket fails (`EPIPE`), which is the "peer is gone" signal. This
//! needs no new cancel protocol and no fd polling (that would be K-B, the named
//! upgrade, deferred) — only the emitter the handler already holds.
//!
//! ## K-A limitation (ratified §14; the K-B upgrade trigger)
//!
//! The heartbeat is a *write*. A peer that is connected but **not reading** its
//! socket can let the kernel send buffer fill; the heartbeat write then blocks
//! instead of failing, so a disconnect that presents this way is not detected and
//! cancel latency is unbounded for that case. The honest fix is K-B (an explicit
//! `POLLHUP`/`MSG_PEEK` fd watcher), the named upgrade, NOT built here. For the
//! normal disconnect (peer closes the fd) the write fails promptly and
//! cancellation fires within one checkpoint interval.
//!
//! Cancellation is **checkpoint-granular, not instruction-granular** (mirroring
//! D5b): the loop bails at its next bounded-interval checkpoint, not at the exact
//! instruction the peer vanished. All paths are read-only ⇒ a cancelled query has
//! no partial state to roll back; it discards its work and returns `Cancelled`.
//!
//! ## Two threading models, one signal
//!
//! * **Transport-thread cooperative checkpoint** ([`pre_work_check`] +
//!   [`loop_checkpoint`]) — the heavy Rust loop runs ON the connection's own thread
//!   (which owns the emitter), so the checkpoint emits a heartbeat directly and
//!   maps the write failure to [`ControlFlow::Break`]. This is what `cycles` and
//!   `path` use in DAEMON-CANCEL-1: their loops (`find_sccs_cancellable`,
//!   `LiveGraph::path_cancellable`) are reachable from Rust and accept a
//!   `CancelCheck` closure.
//! * **Worker-thread supervisor** ([`run_interruptible`] + [`CancelFlag`]) — for
//!   heavy work that CANNOT be checkpointed in-Rust from the transport thread
//!   (a single opaque SQL statement, or multi-signal agent assembly): the work runs
//!   on a worker thread while the transport thread probes the peer. This is the
//!   FOUNDATION for DAEMON-CANCEL-2's worker-thread handlers (stats SQL +
//!   orient/check/trust/explain assembly). It is built and tested here — with the
//!   internal-failure classification corrected (see [`Supervised`]) — but its
//!   production callers and its `sqlite3_interrupt` actuator are CANCEL-2.
//!
//! ## Abstraction ledger (the cross-cutting cancel seam)
//!
//! Per the operating rule — the in-flight cancel seam is a new cross-cutting element,
//! so it is recorded here (greppable: "Abstraction ledger").
//!
//! * **What:** a query-path cancellation seam with ONE disconnect signal
//!   (emitter-`Err`) and two threading models: (1) a cooperative checkpoint — the
//!   `CancelCheck` shape `&mut dyn FnMut() -> ControlFlow<()>` (defined once in
//!   `graph-algorithms`, reused by `livegraph`), produced by [`loop_checkpoint`]
//!   and threaded into the heavy Rust loops on the transport thread; and (2) the
//!   [`run_interruptible`] worker supervisor + [`CancelFlag`] for opaque work that
//!   only a worker thread can run.
//! * **Concrete current users:** `handle_cycles` — the checkpoint is threaded into
//!   EVERY Tarjan-running cycles route (the transport thread runs them all): the
//!   DEFAULT `auto` route in ALL THREE of its phases — the precondition
//!   (`cycles_auto_response` → `LiveGraph::module_import_cycles_cancellable`), the
//!   first-call-per-fingerprint cert build (`build_and_store_cycles_cert_cancellable` →
//!   `module_cycle_compare_data_cancellable`; review iteration 1 closed this gap, the last
//!   uncheckpointed loop on the route), and the `storage::find_cycles_cancellable` fallback —
//!   plus the explicit `--engine sqlite` route (`find_cycles_cancellable`) and the `livegraph`
//!   (module/file) + `compare` routes (`module_import_cycles_cancellable` /
//!   `file_import_cycles_cancellable` / `module_cycle_compare_data_cancellable`) — all reaching
//!   `find_sccs_cancellable`. The NON-cancellable `build_and_store_cycles_cert` (a never-breaking
//!   checkpoint delegate) is retained ONLY for the orient cert-build caller, which is OUT of
//!   CANCEL-1's in-loop scope and so stays byte-identical.
//!   `handle_path` threads it into `LiveGraph::path_cancellable`'s BFS on every served
//!   engine (Auto / LiveGraph / Compare). The worker supervisor ([`run_interruptible`])
//!   has NO production caller in this slice — it is the ratified foundation
//!   (DAEMON-CANCEL-1 deliverable #2) for CANCEL-2's worker-thread handlers; it is
//!   exercised by this module's tests.
//! * **Axis of variation:** cancellable vs not — and within cancellable, which
//!   threading model the heavy-work shape needs (a transport-thread Rust-loop
//!   checkpoint, or a worker-thread supervisor for opaque SQL/assembly).
//! * **Rejected simpler alternatives:** (1) handler-boundary-only checkpoints
//!   (cancel BEFORE heavy work) — Option B, operator-rejected (lets an abandoned
//!   heavy query run to completion). The cheap [`pre_work_check`] layer is KEPT
//!   (necessary, not sufficient); the in-loop layer is what Option A required.
//!   (2) Wrapping a Rust loop in a worker + `sqlite3_interrupt` and returning
//!   `Cancelled` while the loop keeps running — DISHONEST (the interrupt is a no-op
//!   on a CPU-bound Rust loop). cycles/path thread a real checkpoint INTO the loop
//!   instead. (3) A bespoke cancel-token framework / new crate — rejected: the seam
//!   is `std::ops::ControlFlow` + a bare closure + an `Arc<AtomicBool>` flag, no
//!   new type framework. The `sqlite3_interrupt` actuator (for opaque SQL) is
//!   deliberately NOT added here (DAEMON-CANCEL-2; STOP_CONDITION: no stats
//!   SQL-interrupt wiring in this slice).

use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

use repo_graph_daemon_transport::{ProgressDetail, ProgressEmitter};

/// How often [`run_interruptible`]'s supervisor probes the peer during a long
/// worker-thread query. Bounds cancel latency (after a real disconnect) to one
/// interval. Fast queries are unaffected: the supervisor wakes the instant the
/// worker finishes, so a sub-interval query never emits a heartbeat at all.
const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(100);

/// A heartbeat progress event. `current/total = 0/1` marks it as a liveness probe
/// rather than real progress; its only job is to attempt a transport write so a
/// peer disconnect surfaces as an emit error.
fn heartbeat(phase: &str) -> ProgressDetail {
    ProgressDetail {
        phase: phase.to_string(),
        current: 0,
        total: 1,
    }
}

/// Cheap handler-boundary cancel check: emit one heartbeat BEFORE heavy work
/// begins. A write failure means the peer is already gone, so the handler can skip
/// the heavy work entirely. This is the "cancel before heavy work starts" layer the
/// ratified design keeps — necessary but NOT sufficient (it cannot catch a
/// disconnect that happens DURING the work; that is what the in-loop
/// [`loop_checkpoint`] adds). Returns [`ControlFlow::Break`] iff the peer is gone.
pub fn pre_work_check(emitter: &mut dyn ProgressEmitter, phase: &str) -> ControlFlow<()> {
    match emitter.emit(heartbeat(phase)) {
        Ok(()) => ControlFlow::Continue(()),
        Err(_) => ControlFlow::Break(()),
    }
}

/// Build a cooperative cancellation checkpoint for a transport-thread Rust loop.
///
/// The returned closure emits a heartbeat each time the loop consults it and
/// reports [`ControlFlow::Break`] when the write fails (peer gone). Pass it as a
/// `&mut` `CancelCheck` into `find_sccs_cancellable` / `LiveGraph::path_cancellable`
/// (and their daemon-side wrappers). It borrows `emitter` for its lifetime — drop
/// it (or let it fall out of scope) before using `emitter` again.
///
/// The loop controls cadence (it calls the closure at a bounded interval), so each
/// call here is one heartbeat write; the heavy loop is responsible for not calling
/// it on every instruction.
pub fn loop_checkpoint<'a>(
    emitter: &'a mut dyn ProgressEmitter,
    phase: &'a str,
) -> impl FnMut() -> ControlFlow<()> + 'a {
    move || match emitter.emit(heartbeat(phase)) {
        Ok(()) => ControlFlow::Continue(()),
        Err(_) => ControlFlow::Break(()),
    }
}

/// A thread-crossing cancellation flag for worker-thread query handlers
/// (DAEMON-CANCEL-2 foundation).
///
/// [`run_interruptible`]'s supervisor sets it from the transport thread when the
/// peer disconnects; the worker's deep loops poll it via a [`checkpoint`] closure
/// (the `CancelCheck` shape). It is the worker-thread counterpart of
/// [`loop_checkpoint`]: the worker cannot touch the request's `ProgressEmitter`
/// (that lives on, and is borrowed by, the transport thread), so the disconnect
/// signal crosses the thread boundary as this `Arc<AtomicBool>` instead.
///
/// `Relaxed` ordering is sufficient: the flag is a single one-way latch (false →
/// true) with no other memory it must synchronize; the worker only needs to observe
/// the set *eventually* (within a checkpoint or two), not synchronized against
/// other writes.
///
/// [`checkpoint`]: CancelFlag::checkpoint
#[derive(Clone, Default)]
pub struct CancelFlag(Arc<AtomicBool>);

impl CancelFlag {
    /// A fresh, un-cancelled flag.
    pub fn new() -> Self {
        Self::default()
    }

    /// Latch the flag to "cancelled". Idempotent.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// True once [`cancel`](Self::cancel) has been called.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// Build a `CancelCheck`-compatible checkpoint (`FnMut() -> ControlFlow<()>`)
    /// that reports [`ControlFlow::Break`] once the flag is latched. The returned
    /// closure owns its own cheap `Arc` clone, so it is `'static` and can be
    /// threaded into the worker's heavy loop as `&mut dyn FnMut`.
    pub fn checkpoint(&self) -> impl FnMut() -> ControlFlow<()> + 'static {
        let flag = self.clone();
        move || {
            if flag.is_cancelled() {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        }
    }
}

/// Outcome of a supervised, interruptible worker-thread query
/// ([`run_interruptible`]).
pub enum Supervised<T> {
    /// The worker finished and produced its result (the query's own value).
    Completed(T),
    /// The peer disconnected; the [`CancelFlag`] was set, so the worker is
    /// abandoning its (discarded) work. The caller maps this to
    /// `ErrorCode::Cancelled`.
    Cancelled,
    /// The worker thread dropped its sender WITHOUT sending a value — a panic or
    /// unexpected teardown while the peer was still connected. This is an INTERNAL
    /// failure, distinct from a client cancel: callers map it to
    /// `ErrorCode::InternalError`, NEVER `Cancelled` (DAEMON-CANCEL-1 deliverable
    /// #2 — the prior WIP mislabelled this as `Cancelled`, conflating an internal
    /// bug with "the client disconnected"; this is the fix).
    WorkerVanished,
}

/// Run heavy query work on a worker thread, supervised from the calling
/// (transport) thread, cancellable via the peer-disconnect signal.
///
/// The worker receives a [`CancelFlag`]; it threads a `CancelFlag::checkpoint` into
/// its heavy loop / multi-signal assembly so cooperative cancellation can fire
/// between steps. Each [`HEARTBEAT_INTERVAL`] the supervisor emits a heartbeat; the
/// first write failure means the peer is gone, so it latches the `CancelFlag` (the
/// worker stops at its next checkpoint) and returns [`Supervised::Cancelled`]
/// WITHOUT waiting for the worker. The worker unwinds, drops its connection, and
/// exits; its discarded `send` no-ops once the receiver is gone.
///
/// `work` takes ownership of everything it touches, so it is `'static` and `Send`.
///
/// Fast queries pay no fixed latency: `recv_timeout` returns the moment the worker
/// sends, so a sub-interval query never emits a heartbeat at all.
///
/// NOTE (DAEMON-CANCEL-1 scope): this is the cooperative-flag supervisor only. The
/// `sqlite3_interrupt` actuator that aborts a single in-flight SQL statement
/// mid-execution (needed by the stats/agent worker handlers, which disconnect while
/// blocked *inside* an opaque statement) is DAEMON-CANCEL-2 — it adds an interrupt
/// handle parameter alongside the flag. There is NO production caller of this
/// supervisor in DAEMON-CANCEL-1 (cycles/path use the transport-thread
/// [`loop_checkpoint`] instead); it is the ratified foundation, validated by the
/// tests below.
pub fn run_interruptible<T, F>(
    emitter: &mut dyn ProgressEmitter,
    phase: &str,
    work: F,
) -> Supervised<T>
where
    T: Send + 'static,
    F: FnOnce(CancelFlag) -> T + Send + 'static,
{
    let flag = CancelFlag::new();
    let worker_flag = flag.clone();
    let (tx, rx) = mpsc::channel();
    // Detached worker (like B1's connection threads): on cancel we return without
    // joining; the worker finishes quickly once cancel-flagged and its `send` no-ops.
    std::thread::spawn(move || {
        let _ = tx.send(work(worker_flag));
    });

    loop {
        match rx.recv_timeout(HEARTBEAT_INTERVAL) {
            Ok(result) => return Supervised::Completed(result),
            Err(RecvTimeoutError::Timeout) => {
                if emitter.emit(heartbeat(phase)).is_err() {
                    // Peer gone (K-A). Latch the flag so the worker's loop bails at
                    // its next checkpoint, then return without joining.
                    flag.cancel();
                    return Supervised::Cancelled;
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                // The worker dropped its sender without a value (panic) while the
                // peer was still connected — an INTERNAL failure, NOT a client
                // cancel. This is the deliverable-#2 fix: the prior WIP returned
                // Cancelled here, masquerading a worker panic as "client disconnected".
                return Supervised::WorkerVanished;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_daemon_transport::EmitError;

    /// Emitter that succeeds `ok_for` times, then fails every emit after — models a
    /// peer that disconnects mid-query (the D5b `FailingEmitter` shape).
    struct FailAfter {
        ok_for: usize,
        emits: usize,
    }
    impl ProgressEmitter for FailAfter {
        fn emit(&mut self, _d: ProgressDetail) -> Result<(), EmitError> {
            self.emits += 1;
            if self.emits > self.ok_for {
                Err(EmitError::new("simulated disconnect"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn pre_work_check_breaks_when_peer_already_gone() {
        let mut gone = FailAfter {
            ok_for: 0,
            emits: 0,
        };
        assert!(pre_work_check(&mut gone, "x").is_break());
        let mut live = FailAfter {
            ok_for: 10,
            emits: 0,
        };
        assert!(pre_work_check(&mut live, "x").is_continue());
    }

    #[test]
    fn loop_checkpoint_breaks_on_emit_failure() {
        // Continues while the peer is live, then breaks once the emitter fails —
        // exactly how a transport-thread Rust-loop query learns to abandon work.
        let mut emitter = FailAfter {
            ok_for: 2,
            emits: 0,
        };
        let mut chk = loop_checkpoint(&mut emitter, "loop");
        assert!(chk().is_continue());
        assert!(chk().is_continue());
        assert!(chk().is_break());
        assert!(chk().is_break());
    }

    #[test]
    fn cancel_flag_checkpoint_breaks_after_cancel() {
        // The worker-thread cooperative checkpoint: Continue while live, Break once
        // the supervisor latches the flag. This is how a worker's loop learns to
        // abandon work (the emitter is unreachable from the worker).
        let flag = CancelFlag::new();
        let mut chk = flag.checkpoint();
        assert!(chk().is_continue());
        assert!(chk().is_continue());
        flag.cancel();
        assert!(chk().is_break());
        assert!(flag.is_cancelled());
    }

    #[test]
    fn run_interruptible_completes_when_peer_stays() {
        // A fast worker returns its value; no heartbeat failure ⇒ Completed.
        let mut emitter = FailAfter {
            ok_for: 1000,
            emits: 0,
        };
        let out = run_interruptible(&mut emitter, "p", |_flag| 7u32);
        assert!(matches!(out, Supervised::Completed(7)));
    }

    #[test]
    fn run_interruptible_cancels_on_disconnect_while_work_in_flight() {
        use std::sync::mpsc as m;
        // The worker blocks until released, so it is provably IN FLIGHT when the
        // emitter starts failing. The supervisor must observe the emit failure,
        // latch the flag, and return Cancelled WITHOUT waiting for the (still-blocked)
        // worker. The worker also witnesses the flag being set (proves the
        // cooperative signal crosses the thread boundary).
        let (release_tx, release_rx) = m::channel::<()>();
        let (saw_tx, saw_rx) = m::channel::<bool>();
        let mut emitter = FailAfter {
            ok_for: 0,
            emits: 0,
        }; // fails on the first heartbeat
        let out = run_interruptible(&mut emitter, "p", move |flag| {
            let _ = release_rx.recv(); // still running at cancel time
            let _ = saw_tx.send(flag.is_cancelled());
            42u32
        });
        assert!(matches!(out, Supervised::Cancelled));
        let _ = release_tx.send(()); // let the detached worker finish and exit
        assert!(
            saw_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            "the worker must observe the flag the supervisor latched on disconnect"
        );
    }

    #[test]
    fn run_interruptible_reports_worker_panic_as_vanished_not_cancelled() {
        // DAEMON-CANCEL-1 deliverable #2: a worker that drops its sender without a
        // value (panic) while the peer is LIVE must surface as WorkerVanished
        // (→ InternalError), NEVER Cancelled (→ "client disconnected"). The emitter
        // never fails, so the only way the channel disconnects is the worker dying.
        let mut emitter = FailAfter {
            ok_for: 1_000_000,
            emits: 0,
        };
        let out: Supervised<u32> =
            run_interruptible(&mut emitter, "p", |_flag| panic!("worker boom"));
        assert!(
            matches!(out, Supervised::WorkerVanished),
            "a worker panic must be WorkerVanished (internal failure), not Cancelled"
        );
    }
}
