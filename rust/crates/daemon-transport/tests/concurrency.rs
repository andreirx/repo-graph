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
