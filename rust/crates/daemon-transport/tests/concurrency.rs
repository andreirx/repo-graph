//! DAEMON-CONCURRENCY-IMPL-1 — headless concurrency tests for the socket transport.
//!
//! These drive the REAL `run_socket` accept loop with a custom `Dispatcher`, over real Unix-socket
//! clients, with NO wall-clock sleeps used for a correctness assertion (a `Condvar` rendezvous the
//! test controls gates the "slow" handler; the only sleeps are connect-retries waiting for the
//! server to bind, which gate nothing). They cover:
//!
//!   * `concurrent_no_head_of_line_blocking` (behavior 1) — a slow request parked in its handler does
//!     NOT block a second client's fast read. This FAILS on the old serial accept loop (the fast read
//!     could not even be served until the slow request returned).
//!   * `over_cap_connection_gets_busy_then_closed` (behavior 3 / BP-BUSY) — at the connection cap, an
//!     extra connection receives `ErrorCode::Busy` and is closed, rather than hanging silently.
//!   * `prompt_shutdown_with_connections_open` (DAEMON-CONCURRENCY-1 §3) — the shutdown flag stops the
//!     accept loop promptly even while a handler is parked mid-request and an idle connection is open;
//!     the in-flight handler is detached and still completes. FAILS on the old serial loop (the accept
//!     thread was parked INSIDE `handle_connection`, so it could never observe the flag).
//!   * `no_cross_connection_response_interleaving` (DAEMON-CONCURRENCY-1 §3) — with two connections
//!     served concurrently, each stream carries exactly its own responses, in request order;
//!     per-connection NDJSON pipelining stays serial (a queued second request is not dispatched while
//!     the first is in flight).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use repo_graph_daemon_transport::{
    bind_socket, cleanup_socket, run_socket, BindResult, DispatchResult, Dispatcher,
    ProgressEmitter, Request,
};

/// A rendezvous the test uses to (a) learn when the slow handler is parked and (b) release it.
/// `std::sync::{Mutex, Condvar}` (both `Sync`) so the dispatcher holding it stays `Send + Sync`.
#[derive(Clone, Default)]
struct Rendezvous {
    inner: Arc<(Mutex<RvState>, Condvar)>,
}

#[derive(Default)]
struct RvState {
    slow_entered: bool,
    slow_released: bool,
}

impl Rendezvous {
    /// Called from inside the slow handler: mark it entered, then block until the test releases it.
    fn mark_entered_and_wait(&self) {
        let (lock, cv) = &*self.inner;
        let mut st = lock.lock().unwrap();
        st.slow_entered = true;
        cv.notify_all();
        while !st.slow_released {
            st = cv.wait(st).unwrap();
        }
    }

    /// Block (deterministically, no timer) until the slow handler is provably parked.
    fn wait_until_entered(&self) {
        let (lock, cv) = &*self.inner;
        let mut st = lock.lock().unwrap();
        while !st.slow_entered {
            st = cv.wait(st).unwrap();
        }
    }

    fn release(&self) {
        let (lock, cv) = &*self.inner;
        let mut st = lock.lock().unwrap();
        st.slow_released = true;
        cv.notify_all();
    }
}

/// Test dispatcher: `slow` parks on the rendezvous; `fast` returns immediately.
struct ConcDispatcher {
    rv: Rendezvous,
}

impl Dispatcher for ConcDispatcher {
    fn dispatch(&self, request: &Request, _emitter: &mut dyn ProgressEmitter) -> DispatchResult {
        match request.method.as_str() {
            "slow" => {
                self.rv.mark_entered_and_wait();
                DispatchResult::success(&request.id, serde_json::json!({"slow": "done"}))
            }
            "fast" => DispatchResult::success(&request.id, serde_json::json!({"fast": true})),
            other => DispatchResult::unknown_method(&request.id, other),
        }
    }
}

/// Connect to the socket, retrying while the server thread finishes binding. The retry loop is
/// SETUP only — it gates no behavioral assertion (those are gated on the rendezvous).
fn connect(path: &Path) -> UnixStream {
    for _ in 0..200 {
        if let Ok(s) = UnixStream::connect(path) {
            return s;
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!("could not connect to {}", path.display());
}

/// Read one NDJSON line; `None` on EOF/closed connection.
fn read_one_line(stream: &UnixStream) -> Option<String> {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) => None,
        Ok(_) => Some(line),
        Err(_) => None,
    }
}

#[test]
fn concurrent_no_head_of_line_blocking() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("conc.sock");
    let listener = match bind_socket(&path).unwrap() {
        BindResult::Bound(l) => l,
        _ => panic!("bind failed"),
    };

    let rv = Rendezvous::default();
    let dispatcher = Arc::new(ConcDispatcher { rv: rv.clone() });
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = Arc::clone(&shutdown);
    // Cap of 8: plenty for two concurrent connections.
    let server = thread::spawn(move || run_socket(&listener, dispatcher, &shutdown_clone, 8));

    // Client A: send a slow request and let it park inside the handler (on its own worker thread).
    let mut a = connect(&path);
    writeln!(a, r#"{{"id":"a","method":"slow"}}"#).unwrap();
    a.flush().unwrap();
    rv.wait_until_entered(); // A is now provably parked, occupying one worker thread.

    // Client B: a fast request must return WHILE A is still parked. On the old serial loop, B could
    // not be served until A returned (which only happens after we release it below) -> this read
    // would block. Concurrent dispatch serves B on its own thread.
    let b = connect(&path);
    b.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    {
        let mut bw = b.try_clone().unwrap();
        writeln!(bw, r#"{{"id":"b","method":"fast"}}"#).unwrap();
        bw.flush().unwrap();
    }
    let b_resp =
        read_one_line(&b).expect("B must receive its fast response while A is still parked");
    assert!(
        b_resp.contains(r#""id":"b""#) && b_resp.contains(r#""fast":true"#),
        "B's fast response should arrive independent of A: {b_resp}"
    );

    // Now release A; it completes.
    rv.release();
    a.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let a_resp = read_one_line(&a).expect("A completes after release");
    assert!(a_resp.contains(r#""id":"a""#), "A's response: {a_resp}");

    shutdown.store(true, Ordering::Relaxed);
    drop(a);
    drop(b);
    server.join().unwrap().unwrap();
    cleanup_socket(&path);
}

#[test]
fn over_cap_connection_gets_busy_then_closed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cap.sock");
    let listener = match bind_socket(&path).unwrap() {
        BindResult::Bound(l) => l,
        _ => panic!("bind failed"),
    };

    let rv = Rendezvous::default();
    let dispatcher = Arc::new(ConcDispatcher { rv: rv.clone() });
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = Arc::clone(&shutdown);
    // Cap of 1: exactly one in-flight connection handler is permitted.
    let server = thread::spawn(move || run_socket(&listener, dispatcher, &shutdown_clone, 1));

    // Client A occupies the single permit (parked in the slow handler).
    let mut a = connect(&path);
    writeln!(a, r#"{{"id":"a","method":"slow"}}"#).unwrap();
    a.flush().unwrap();
    rv.wait_until_entered(); // the one permit is now held by A's handler thread.

    // Client B connects while the cap is full. The accept loop keeps draining accept(), fails to get
    // a permit, writes a typed Busy error, and closes — no silent hang. B does not even send a request.
    let b = connect(&path);
    b.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let b_resp = read_one_line(&b).expect("over-cap client must receive a Busy line");
    assert!(
        b_resp.contains(r#""code":"Busy""#),
        "over-cap connection should get ErrorCode::Busy: {b_resp}"
    );
    // The daemon closes the over-cap connection: the next read is EOF.
    assert!(
        read_one_line(&b).is_none(),
        "the daemon must close the over-cap connection after the Busy response"
    );

    rv.release();
    shutdown.store(true, Ordering::Relaxed);
    drop(a);
    drop(b);
    server.join().unwrap().unwrap();
    cleanup_socket(&path);
}

/// DAEMON-CONCURRENCY-1 §3 "prompt shutdown with connections open".
///
/// Mechanism under test: `run_socket`'s loop condition polls the shutdown flag between
/// `accept()` attempts (the listener is non-blocking; `WouldBlock` sleeps ≤100ms). Because every
/// connection is handled on its OWN thread, a handler parked mid-request never holds the accept
/// loop — so setting the flag stops the loop within one poll interval regardless of open
/// connections. On the OLD serial loop this test hangs forever: the accept thread is parked
/// INSIDE `handle_connection`, the flag is never observed, and the timed join below fires.
///
/// Also pins the documented detached-handler semantics (`run_socket` doc: outstanding threads
/// "run to completion"): after the accept loop has returned, the parked handler is released and
/// its client STILL receives the response — shutdown is prompt for the daemon, not lossy for the
/// in-flight request.
///
/// Determinism: the parked state is gated on the `Condvar` rendezvous, not a timer. The single
/// `recv_timeout` is a LIVENESS bound only (hang = failure), the same class as the file's 10s
/// read timeouts; it gates no ordering assertion. The proof that the loop returned WHILE the
/// handler was still parked is structural: `rv.release()` is only called AFTER the join result
/// arrived, and A's response cannot be produced before release.
#[test]
fn prompt_shutdown_with_connections_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shutdown.sock");
    let listener = match bind_socket(&path).unwrap() {
        BindResult::Bound(l) => l,
        _ => panic!("bind failed"),
    };

    let rv = Rendezvous::default();
    let dispatcher = Arc::new(ConcDispatcher { rv: rv.clone() });
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = Arc::clone(&shutdown);
    let server = thread::spawn(move || run_socket(&listener, dispatcher, &shutdown_clone, 8));

    // Connection 1: parked mid-request (the strictest "open connection" — a handler in flight).
    let mut a = connect(&path);
    writeln!(a, r#"{{"id":"a","method":"slow"}}"#).unwrap();
    a.flush().unwrap();
    rv.wait_until_entered();

    // Connection 2: open but idle (its handler thread is blocked in read_line, holding a permit).
    let idle = connect(&path);

    // Signal shutdown while both connections are open and one handler is parked.
    shutdown.store(true, Ordering::Relaxed);

    // The accept loop must return promptly. JoinHandle has no timed join, so a watcher thread
    // forwards the join result over a channel; recv_timeout is the liveness bound.
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(server.join());
    });
    let joined = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("run_socket must return promptly on shutdown while a handler is parked (serial-loop regression)");
    joined
        .expect("server thread must not panic")
        .expect("run_socket returns Ok on clean shutdown");

    // The accept loop is gone, but the detached in-flight handler completes: release the parked
    // request and its client still receives the response.
    rv.release();
    a.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let a_resp = read_one_line(&a).expect(
        "the in-flight request must complete after shutdown (detached handler runs to completion)",
    );
    assert!(
        a_resp.contains(r#""id":"a""#) && a_resp.contains(r#""slow":"done""#),
        "the detached handler's response must be the parked request's answer: {a_resp}"
    );

    drop(a);
    drop(idle);
    cleanup_socket(&path);
}

/// DAEMON-CONCURRENCY-1 §3 "no cross-connection response interleaving (each connection's
/// responses ordered)".
///
/// Structure under test: each connection's handler thread writes ONLY to its own stream clone,
/// and per-connection request handling stays SERIAL (the NDJSON loop reads the next request only
/// after the previous response is written — §1.1 "per-connection request handling stays serial").
/// So concurrency exists ACROSS connections while each connection keeps strict request-order
/// responses on its own stream. This pins that structure against refactors that would share a
/// response path across connections.
///
/// Overlap is proven, not assumed: A's first request parks (rendezvous-gated), then B pipelines
/// three requests and receives ALL THREE responses while A is still parked — the two connections
/// were provably being served at the same time. A's second, already-pipelined request must NOT
/// have been dispatched during the park (per-connection serial); after release, A receives its
/// two responses in request order.
///
/// Reader discipline: ONE persistent `BufReader` per connection. A fresh `BufReader` per read
/// (as `read_one_line` creates) could buffer-and-discard a following line; multi-line assertions
/// need the buffer kept alive.
#[test]
fn no_cross_connection_response_interleaving() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("interleave.sock");
    let listener = match bind_socket(&path).unwrap() {
        BindResult::Bound(l) => l,
        _ => panic!("bind failed"),
    };

    let rv = Rendezvous::default();
    let dispatcher = Arc::new(ConcDispatcher { rv: rv.clone() });
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = Arc::clone(&shutdown);
    let server = thread::spawn(move || run_socket(&listener, dispatcher, &shutdown_clone, 8));

    // One persistent buffered reader per connection (see doc comment).
    let next_line = |reader: &mut BufReader<UnixStream>, who: &str| -> String {
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .unwrap_or_else(|e| panic!("read from {who} failed: {e}"));
        assert!(n > 0, "unexpected EOF on {who}");
        line
    };

    // Client A: pipeline slow (a1) then fast (a2) in ONE flush on ONE connection.
    let a = connect(&path);
    a.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    {
        let mut aw = a.try_clone().unwrap();
        writeln!(aw, r#"{{"id":"a1","method":"slow"}}"#).unwrap();
        writeln!(aw, r#"{{"id":"a2","method":"fast"}}"#).unwrap();
        aw.flush().unwrap();
    }
    let mut a_reader = BufReader::new(a.try_clone().unwrap());
    rv.wait_until_entered(); // a1 is parked; a2 is queued on A's connection, NOT dispatched.

    // Client B: pipeline three fasts while A is parked; all three answered on B's own stream.
    let b = connect(&path);
    b.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    {
        let mut bw = b.try_clone().unwrap();
        writeln!(bw, r#"{{"id":"b1","method":"fast"}}"#).unwrap();
        writeln!(bw, r#"{{"id":"b2","method":"fast"}}"#).unwrap();
        writeln!(bw, r#"{{"id":"b3","method":"fast"}}"#).unwrap();
        bw.flush().unwrap();
    }
    let mut b_reader = BufReader::new(b.try_clone().unwrap());
    for expected in ["b1", "b2", "b3"] {
        let line = next_line(&mut b_reader, "B");
        assert!(
            line.contains(&format!(r#""id":"{expected}""#)),
            "B's responses must arrive in request order while A is parked; expected {expected}, got: {line}"
        );
        assert!(
            !line.contains(r#""id":"a"#),
            "no response for A's requests may appear on B's connection: {line}"
        );
    }

    // Release A: a1 completes first, then a2 is dispatched — strict per-connection order.
    rv.release();
    for (expected, marker) in [("a1", r#""slow":"done""#), ("a2", r#""fast":true"#)] {
        let line = next_line(&mut a_reader, "A");
        assert!(
            line.contains(&format!(r#""id":"{expected}""#)) && line.contains(marker),
            "A's responses must arrive in request order on A's own connection; expected {expected}, got: {line}"
        );
        assert!(
            !line.contains(r#""id":"b"#),
            "no response for B's requests may appear on A's connection: {line}"
        );
    }

    shutdown.store(true, Ordering::Relaxed);
    drop(a);
    drop(b);
    server.join().unwrap().unwrap();
    cleanup_socket(&path);
}
