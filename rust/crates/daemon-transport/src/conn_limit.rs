//! Bounded-concurrency permit for the concurrent accept loop.
//!
//! DAEMON-CONCURRENCY-IMPL-1 (D-C = C-B): the socket transport handles each
//! accepted connection on its own thread so `accept()` returns immediately,
//! removing the serial accept loop's head-of-line blocking (TECH-DEBT #1). To
//! keep a connection storm from spawning unbounded handler threads, the accept
//! loop bounds the number of in-flight connection-handler threads with this
//! counting semaphore.
//!
//! ── Abstraction ledger (per the operating rule) ───────────────────────────
//!
//! - **What:** a counting semaphore with a non-blocking `try_acquire` that hands
//!   out an RAII [`ConnectionPermit`].
//! - **Concrete current user:** the single accept loop in `socket::run_socket`
//!   (one permit ≈ one in-flight connection handler).
//! - **Axis of variation:** the maximum concurrency (`RMAP_DAEMON_MAX_CONNS`).
//! - **Rejected simpler:** a bare `Arc<AtomicUsize>` decremented/incremented by
//!   hand at every accept-loop exit path. Rejected because manual increment is
//!   NOT panic-safe (a panicking handler thread would leak a permit) and would
//!   be duplicated across exit paths. The RAII permit guarantees release on
//!   drop — including thread panic — at one site.
//!
//! Uses only `std::sync::atomic` (the transport crate has no `parking_lot`
//! dependency, and a counting semaphore over an atomic needs no mutex).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// A counting semaphore that hands out RAII permits via a non-blocking
/// `try_acquire`.
///
/// Shared as `Arc<ConnectionLimiter>` between the accept loop and every
/// outstanding [`ConnectionPermit`] so a permit can release (increment) on drop
/// even after the accept loop has moved on to later connections.
pub struct ConnectionLimiter {
    /// Permits currently available. Starts at the configured maximum; a
    /// successful `try_acquire` decrements it, dropping a permit increments it.
    available: AtomicUsize,
}

impl ConnectionLimiter {
    /// Create a limiter with `max_permits` available permits.
    pub fn new(max_permits: usize) -> Arc<Self> {
        Arc::new(Self {
            available: AtomicUsize::new(max_permits),
        })
    }

    /// Try to take one permit without blocking.
    ///
    /// Returns `Some(permit)` (decrementing the available count) when a permit
    /// is free, or `None` when the limiter is at capacity. The accept loop uses
    /// `None` as the BP-BUSY over-cap signal: it writes a typed `Busy` error and
    /// closes the connection rather than queueing the client into an opaque hang.
    pub fn try_acquire(self: &Arc<Self>) -> Option<ConnectionPermit> {
        let mut current = self.available.load(Ordering::Acquire);
        loop {
            if current == 0 {
                return None;
            }
            // CAS-decrement; retry on contention (compare_exchange_weak may spuriously fail).
            match self.available.compare_exchange_weak(
                current,
                current - 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Some(ConnectionPermit {
                        limiter: Arc::clone(self),
                    })
                }
                Err(observed) => current = observed,
            }
        }
    }

    /// Permits currently available. Test-only: the accept loop drives capacity
    /// solely through `try_acquire`/permit-drop, so this observer exists for the
    /// unit tests below (gated to avoid a dead-code warning under `-D warnings`).
    #[cfg(test)]
    pub fn available(&self) -> usize {
        self.available.load(Ordering::Acquire)
    }
}

/// RAII permit: holding it counts against the concurrency cap; dropping it
/// returns the permit to the limiter.
///
/// Released on EVERY drop path — normal handler return, early error, or thread
/// panic — which is why the accept loop is panic-safe without a manual release.
pub struct ConnectionPermit {
    limiter: Arc<ConnectionLimiter>,
}

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.limiter.available.fetch_add(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquires_up_to_capacity_then_refuses() {
        let limiter = ConnectionLimiter::new(2);
        assert_eq!(limiter.available(), 2);

        let p1 = limiter.try_acquire();
        assert!(p1.is_some());
        assert_eq!(limiter.available(), 1);

        let p2 = limiter.try_acquire();
        assert!(p2.is_some());
        assert_eq!(limiter.available(), 0);

        // At capacity: over-cap acquire is refused (the BP-BUSY signal).
        assert!(limiter.try_acquire().is_none());
        assert_eq!(limiter.available(), 0);
    }

    #[test]
    fn dropping_a_permit_releases_capacity() {
        let limiter = ConnectionLimiter::new(1);
        let p = limiter.try_acquire();
        assert!(p.is_some());
        assert!(limiter.try_acquire().is_none(), "cap=1 is full");

        drop(p);
        assert_eq!(limiter.available(), 1);
        assert!(
            limiter.try_acquire().is_some(),
            "permit freed after drop is reusable"
        );
    }

    #[test]
    fn permit_releases_on_thread_panic() {
        // Panic-safety: a handler thread that panics must still return its permit.
        let limiter = ConnectionLimiter::new(1);
        let permit = limiter.try_acquire().expect("acquire");
        let moved = limiter.clone();
        let h = std::thread::spawn(move || {
            let _p = permit; // moved in; dropped on unwind
            panic!("simulated handler panic");
        });
        assert!(h.join().is_err(), "thread should have panicked");
        assert_eq!(moved.available(), 1, "permit returned despite panic");
    }
}
