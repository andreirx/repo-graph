//! Repo coordinator with readers-writer semantics.
//!
//! The coordinator wraps the state machine with actual locking and
//! provides FIFO writer queue semantics.

use parking_lot::{Condvar, Mutex};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::error::CoordinatorError;
use crate::state::{CoordinatorState, TransitionResult};

/// A unique identifier for queued write requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WriteTicket(u64);

/// The type of write operation being requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteKind {
    /// A general write operation (index, policy update, etc.).
    Write,
    /// A refresh operation.
    Refresh,
}

/// A repo coordinator that enforces readers-writer semantics.
///
/// Key properties:
/// - Multiple readers can proceed concurrently
/// - Writers are exclusive (block readers and other writers)
/// - Writers are served FIFO (first queued, first granted)
/// - Readers arriving while a writer is queued must wait
///
/// The FIFO property is important for fairness: without it, a steady
/// stream of readers could starve writers indefinitely.
pub struct RepoCoordinator {
    /// The current state, protected by a mutex.
    inner: Mutex<CoordinatorInner>,

    /// Condition variable for state changes.
    state_changed: Condvar,

    /// Counter for generating unique write tickets.
    next_ticket: AtomicU64,
}

struct CoordinatorInner {
    /// The current coordinator state.
    state: CoordinatorState,

    /// Queue of pending write requests (FIFO).
    /// Each entry is (ticket, kind).
    write_queue: VecDeque<(WriteTicket, WriteKind)>,
}

/// A guard that releases a read permit when dropped.
pub struct ReadGuard<'a> {
    coordinator: &'a RepoCoordinator,
}

impl Drop for ReadGuard<'_> {
    fn drop(&mut self) {
        self.coordinator.release_read_internal();
    }
}

/// A guard that releases a write permit when dropped.
pub struct WriteGuard<'a> {
    coordinator: &'a RepoCoordinator,
    kind: WriteKind,
}

impl Drop for WriteGuard<'_> {
    fn drop(&mut self) {
        self.coordinator.release_write_internal(self.kind);
    }
}

impl RepoCoordinator {
    /// Create a new coordinator in the Idle state.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(CoordinatorInner {
                state: CoordinatorState::Idle,
                write_queue: VecDeque::new(),
            }),
            state_changed: Condvar::new(),
            next_ticket: AtomicU64::new(1),
        }
    }

    /// Get the current state (snapshot).
    pub fn state(&self) -> CoordinatorState {
        self.inner.lock().state.clone()
    }

    /// Get the number of queued writers.
    pub fn queued_writers(&self) -> usize {
        self.inner.lock().write_queue.len()
    }

    /// Acquire a read permit, blocking until available.
    ///
    /// Blocks if:
    /// - A writer is active
    /// - Writers are queued (to prevent writer starvation)
    ///
    /// Returns a guard that releases the permit on drop.
    pub fn acquire_read(&self) -> ReadGuard<'_> {
        let mut inner = self.inner.lock();

        loop {
            // Block if writers are queued (FIFO fairness)
            if !inner.write_queue.is_empty() {
                self.state_changed.wait(&mut inner);
                continue;
            }

            match inner.state.try_acquire_read() {
                TransitionResult::Ok(new_state) => {
                    inner.state = new_state;
                    return ReadGuard { coordinator: self };
                }
                TransitionResult::Blocked(_) => {
                    self.state_changed.wait(&mut inner);
                }
                TransitionResult::Conflict(msg) | TransitionResult::InvariantViolation(msg) => {
                    panic!("acquire_read invariant violation: {}", msg);
                }
            }
        }
    }

    /// Try to acquire a read permit without blocking.
    ///
    /// Returns `None` if a writer is active or writers are queued.
    pub fn try_acquire_read(&self) -> Option<ReadGuard<'_>> {
        let mut inner = self.inner.lock();

        // Don't acquire if writers are queued
        if !inner.write_queue.is_empty() {
            return None;
        }

        match inner.state.try_acquire_read() {
            TransitionResult::Ok(new_state) => {
                inner.state = new_state;
                Some(ReadGuard { coordinator: self })
            }
            TransitionResult::Blocked(_) => None,
            TransitionResult::Conflict(msg) | TransitionResult::InvariantViolation(msg) => {
                panic!("try_acquire_read invariant violation: {}", msg);
            }
        }
    }

    /// Acquire a read permit with timeout.
    ///
    /// Returns `Err(Timeout)` if the timeout expires before access is granted.
    pub fn acquire_read_timeout(
        &self,
        timeout: Duration,
    ) -> Result<ReadGuard<'_>, CoordinatorError> {
        let deadline = Instant::now() + timeout;
        let mut inner = self.inner.lock();

        loop {
            // Block if writers are queued (FIFO fairness)
            if !inner.write_queue.is_empty() {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(CoordinatorError::timeout("read access"));
                }
                if self
                    .state_changed
                    .wait_for(&mut inner, remaining)
                    .timed_out()
                {
                    return Err(CoordinatorError::timeout("read access"));
                }
                continue;
            }

            match inner.state.try_acquire_read() {
                TransitionResult::Ok(new_state) => {
                    inner.state = new_state;
                    return Ok(ReadGuard { coordinator: self });
                }
                TransitionResult::Blocked(_) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        return Err(CoordinatorError::timeout("read access"));
                    }
                    if self
                        .state_changed
                        .wait_for(&mut inner, remaining)
                        .timed_out()
                    {
                        return Err(CoordinatorError::timeout("read access"));
                    }
                }
                TransitionResult::Conflict(msg) | TransitionResult::InvariantViolation(msg) => {
                    panic!("acquire_read_timeout invariant violation: {}", msg);
                }
            }
        }
    }

    /// Acquire a write permit, blocking until available.
    ///
    /// Queues the request and waits for FIFO turn.
    /// Returns a guard that releases the permit on drop.
    pub fn acquire_write(&self) -> WriteGuard<'_> {
        self.acquire_write_kind(WriteKind::Write)
    }

    /// Acquire a refresh permit, blocking until available.
    ///
    /// Semantically identical to write but tracked separately.
    pub fn acquire_refresh(&self) -> WriteGuard<'_> {
        self.acquire_write_kind(WriteKind::Refresh)
    }

    fn acquire_write_kind(&self, kind: WriteKind) -> WriteGuard<'_> {
        let ticket = WriteTicket(self.next_ticket.fetch_add(1, Ordering::SeqCst));

        let mut inner = self.inner.lock();
        inner.write_queue.push_back((ticket, kind));

        loop {
            // Only proceed if we're at the front of the queue
            if inner.write_queue.front().map(|(t, _)| *t) != Some(ticket) {
                self.state_changed.wait(&mut inner);
                continue;
            }

            let try_fn = match kind {
                WriteKind::Write => CoordinatorState::try_acquire_write,
                WriteKind::Refresh => CoordinatorState::try_acquire_refresh,
            };

            match try_fn(&inner.state) {
                TransitionResult::Ok(new_state) => {
                    inner.state = new_state;
                    inner.write_queue.pop_front();
                    return WriteGuard {
                        coordinator: self,
                        kind,
                    };
                }
                TransitionResult::Blocked(_) | TransitionResult::Conflict(_) => {
                    // Blocked: readers are active, wait for them to finish.
                    // Conflict: another writer is active (the previous front-of-queue
                    // writer acquired but hasn't released yet). Wait for release.
                    self.state_changed.wait(&mut inner);
                }
                TransitionResult::InvariantViolation(msg) => {
                    panic!("acquire_write invariant violation: {}", msg);
                }
            }
        }
    }

    /// Try to acquire a write permit without blocking.
    ///
    /// Returns `None` if readers are active or another writer is queued/active.
    pub fn try_acquire_write(&self) -> Option<WriteGuard<'_>> {
        self.try_acquire_write_kind(WriteKind::Write)
    }

    /// Try to acquire a refresh permit without blocking.
    pub fn try_acquire_refresh(&self) -> Option<WriteGuard<'_>> {
        self.try_acquire_write_kind(WriteKind::Refresh)
    }

    fn try_acquire_write_kind(&self, kind: WriteKind) -> Option<WriteGuard<'_>> {
        let mut inner = self.inner.lock();

        // Don't acquire if other writers are queued
        if !inner.write_queue.is_empty() {
            return None;
        }

        let try_fn = match kind {
            WriteKind::Write => CoordinatorState::try_acquire_write,
            WriteKind::Refresh => CoordinatorState::try_acquire_refresh,
        };

        match try_fn(&inner.state) {
            TransitionResult::Ok(new_state) => {
                inner.state = new_state;
                Some(WriteGuard {
                    coordinator: self,
                    kind,
                })
            }
            TransitionResult::Blocked(_) | TransitionResult::Conflict(_) => None,
            TransitionResult::InvariantViolation(msg) => {
                panic!("try_acquire_write invariant violation: {}", msg);
            }
        }
    }

    /// Acquire a write permit with timeout.
    ///
    /// Returns `Err(Timeout)` if the timeout expires before access is granted.
    /// With FIFO queuing, write conflicts are handled by waiting in the queue
    /// rather than returning an error.
    pub fn acquire_write_timeout(
        &self,
        timeout: Duration,
    ) -> Result<WriteGuard<'_>, CoordinatorError> {
        self.acquire_write_kind_timeout(WriteKind::Write, timeout)
    }

    /// Acquire a refresh permit with timeout.
    pub fn acquire_refresh_timeout(
        &self,
        timeout: Duration,
    ) -> Result<WriteGuard<'_>, CoordinatorError> {
        self.acquire_write_kind_timeout(WriteKind::Refresh, timeout)
    }

    fn acquire_write_kind_timeout(
        &self,
        kind: WriteKind,
        timeout: Duration,
    ) -> Result<WriteGuard<'_>, CoordinatorError> {
        let deadline = Instant::now() + timeout;
        let ticket = WriteTicket(self.next_ticket.fetch_add(1, Ordering::SeqCst));

        let mut inner = self.inner.lock();
        inner.write_queue.push_back((ticket, kind));

        loop {
            // Only proceed if we're at the front of the queue
            if inner.write_queue.front().map(|(t, _)| *t) != Some(ticket) {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    // Remove ourselves from the queue on timeout
                    inner.write_queue.retain(|(t, _)| *t != ticket);
                    self.state_changed.notify_all();
                    return Err(CoordinatorError::timeout("write access"));
                }
                if self
                    .state_changed
                    .wait_for(&mut inner, remaining)
                    .timed_out()
                {
                    inner.write_queue.retain(|(t, _)| *t != ticket);
                    self.state_changed.notify_all();
                    return Err(CoordinatorError::timeout("write access"));
                }
                continue;
            }

            let try_fn = match kind {
                WriteKind::Write => CoordinatorState::try_acquire_write,
                WriteKind::Refresh => CoordinatorState::try_acquire_refresh,
            };

            match try_fn(&inner.state) {
                TransitionResult::Ok(new_state) => {
                    inner.state = new_state;
                    inner.write_queue.pop_front();
                    return Ok(WriteGuard {
                        coordinator: self,
                        kind,
                    });
                }
                TransitionResult::Blocked(_) | TransitionResult::Conflict(_) => {
                    // Blocked: readers are active, wait for them to finish.
                    // Conflict: another writer is active (the previous front-of-queue
                    // writer acquired but hasn't released yet). Wait for release.
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        inner.write_queue.retain(|(t, _)| *t != ticket);
                        self.state_changed.notify_all();
                        return Err(CoordinatorError::timeout("write access"));
                    }
                    if self
                        .state_changed
                        .wait_for(&mut inner, remaining)
                        .timed_out()
                    {
                        inner.write_queue.retain(|(t, _)| *t != ticket);
                        self.state_changed.notify_all();
                        return Err(CoordinatorError::timeout("write access"));
                    }
                }
                TransitionResult::InvariantViolation(msg) => {
                    panic!("acquire_write_timeout invariant violation: {}", msg);
                }
            }
        }
    }

    /// Internal: release a read permit.
    fn release_read_internal(&self) {
        let mut inner = self.inner.lock();
        match inner.state.release_read() {
            TransitionResult::Ok(new_state) => {
                inner.state = new_state;
                self.state_changed.notify_all();
            }
            TransitionResult::InvariantViolation(msg) => {
                panic!("release_read invariant violation: {}", msg);
            }
            _ => panic!("release_read unexpected result"),
        }
    }

    /// Internal: release a write/refresh permit.
    fn release_write_internal(&self, kind: WriteKind) {
        let mut inner = self.inner.lock();
        let release_fn = match kind {
            WriteKind::Write => CoordinatorState::release_write,
            WriteKind::Refresh => CoordinatorState::release_refresh,
        };

        match release_fn(&inner.state) {
            TransitionResult::Ok(new_state) => {
                inner.state = new_state;
                self.state_changed.notify_all();
            }
            TransitionResult::InvariantViolation(msg) => {
                panic!("release_write invariant violation: {}", msg);
            }
            _ => panic!("release_write unexpected result"),
        }
    }
}

impl Default for RepoCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn new_coordinator_is_idle() {
        let coord = RepoCoordinator::new();
        assert_eq!(coord.state(), CoordinatorState::Idle);
        assert_eq!(coord.queued_writers(), 0);
    }

    #[test]
    fn single_reader_succeeds() {
        let coord = RepoCoordinator::new();
        let guard = coord.acquire_read();
        assert_eq!(coord.state(), CoordinatorState::Reading(1));
        drop(guard);
        assert_eq!(coord.state(), CoordinatorState::Idle);
    }

    #[test]
    fn multiple_readers_concurrent() {
        let coord = RepoCoordinator::new();
        let g1 = coord.acquire_read();
        let g2 = coord.acquire_read();
        let g3 = coord.acquire_read();
        assert_eq!(coord.state(), CoordinatorState::Reading(3));
        drop(g1);
        assert_eq!(coord.state(), CoordinatorState::Reading(2));
        drop(g2);
        drop(g3);
        assert_eq!(coord.state(), CoordinatorState::Idle);
    }

    #[test]
    fn single_writer_succeeds() {
        let coord = RepoCoordinator::new();
        let guard = coord.acquire_write();
        assert_eq!(coord.state(), CoordinatorState::Writing);
        drop(guard);
        assert_eq!(coord.state(), CoordinatorState::Idle);
    }

    #[test]
    fn refresh_uses_refreshing_state() {
        let coord = RepoCoordinator::new();
        let guard = coord.acquire_refresh();
        assert_eq!(coord.state(), CoordinatorState::Refreshing);
        drop(guard);
        assert_eq!(coord.state(), CoordinatorState::Idle);
    }

    #[test]
    fn try_acquire_read_when_idle_succeeds() {
        let coord = RepoCoordinator::new();
        let guard = coord.try_acquire_read();
        assert!(guard.is_some());
    }

    #[test]
    fn try_acquire_read_when_writing_fails() {
        let coord = RepoCoordinator::new();
        let _write = coord.acquire_write();
        assert!(coord.try_acquire_read().is_none());
    }

    #[test]
    fn try_acquire_write_when_reading_fails() {
        let coord = RepoCoordinator::new();
        let _read = coord.acquire_read();
        assert!(coord.try_acquire_write().is_none());
    }

    #[test]
    fn try_acquire_write_when_writing_fails() {
        let coord = RepoCoordinator::new();
        let _write = coord.acquire_write();
        assert!(coord.try_acquire_write().is_none());
    }

    #[test]
    fn timeout_returns_error_when_blocked() {
        let coord = RepoCoordinator::new();
        let _write = coord.acquire_write();

        let result = coord.acquire_read_timeout(Duration::from_millis(10));
        assert!(matches!(result, Err(CoordinatorError::Timeout { .. })));
    }

    #[test]
    fn write_timeout_removes_from_queue_on_timeout() {
        let coord = Arc::new(RepoCoordinator::new());
        let coord2 = Arc::clone(&coord);
        let timed_out = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let timed_out2 = Arc::clone(&timed_out);

        // Hold write lock
        let _write = coord.acquire_write();
        assert_eq!(coord.queued_writers(), 0);

        // Try to acquire another write with long timeout (we'll check queue before it expires)
        let handle = thread::spawn(move || {
            let result = coord2.acquire_write_timeout(Duration::from_secs(2));
            timed_out2.store(
                matches!(result, Err(CoordinatorError::Timeout { .. })),
                Ordering::SeqCst,
            );
        });

        // Poll until we see the writer in the queue (timeout after 500ms)
        let deadline = Instant::now() + Duration::from_millis(500);
        while coord.queued_writers() == 0 {
            if Instant::now() > deadline {
                panic!("timed out waiting for writer to queue");
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(coord.queued_writers(), 1);

        // Drop the write lock - this will let the queued writer proceed
        drop(_write);

        // Wait for thread to complete (it should succeed now, not timeout)
        handle.join().unwrap();

        // Since we released the lock, the writer should have succeeded, not timed out
        assert!(!timed_out.load(Ordering::SeqCst));

        // Queue should be empty
        assert_eq!(coord.queued_writers(), 0);
    }

    #[test]
    fn write_timeout_actually_times_out() {
        let coord = Arc::new(RepoCoordinator::new());
        let coord2 = Arc::clone(&coord);
        let timed_out = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let timed_out2 = Arc::clone(&timed_out);

        // Hold write lock for the duration
        let write_guard = coord.acquire_write();

        // Try to acquire another write with short timeout
        let handle = thread::spawn(move || {
            let result = coord2.acquire_write_timeout(Duration::from_millis(50));
            timed_out2.store(
                matches!(result, Err(CoordinatorError::Timeout { .. })),
                Ordering::SeqCst,
            );
        });

        // Wait for thread to complete (should timeout)
        handle.join().unwrap();
        assert!(timed_out.load(Ordering::SeqCst));

        // Queue should be empty (thread removed itself on timeout)
        assert_eq!(coord.queued_writers(), 0);

        // Original lock still held
        assert_eq!(coord.state(), CoordinatorState::Writing);

        drop(write_guard);
    }

    #[test]
    fn readers_block_when_writer_queued() {
        let coord = Arc::new(RepoCoordinator::new());
        let writer_acquired = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writer_acquired2 = Arc::clone(&writer_acquired);

        // Acquire a read
        let read1 = coord.acquire_read();
        assert_eq!(coord.state(), CoordinatorState::Reading(1));

        // Queue a writer (will block waiting for reader)
        let coord2 = Arc::clone(&coord);
        let write_handle = thread::spawn(move || {
            let _write = coord2.acquire_write();
            writer_acquired2.store(true, Ordering::SeqCst);
            // Hold briefly then release
            thread::sleep(Duration::from_millis(10));
        });

        // Give the writer time to queue
        thread::sleep(Duration::from_millis(10));
        assert_eq!(coord.queued_writers(), 1);

        // New reader should not be able to acquire (writer is queued)
        assert!(coord.try_acquire_read().is_none());

        // Release the first read
        drop(read1);

        // Writer should now acquire
        write_handle.join().unwrap();
        assert!(writer_acquired.load(Ordering::SeqCst));

        // After writer releases, we should be back to idle
        assert_eq!(coord.state(), CoordinatorState::Idle);
    }

    #[test]
    fn writers_are_fifo() {
        let coord = Arc::new(RepoCoordinator::new());
        let order = Arc::new(Mutex::new(Vec::new()));

        // Hold initial write lock
        let write1 = coord.acquire_write();

        // Queue multiple writers
        let mut handles = Vec::new();
        for i in 1..=3 {
            let coord_clone = Arc::clone(&coord);
            let order_clone = Arc::clone(&order);
            handles.push(thread::spawn(move || {
                let _guard = coord_clone.acquire_write();
                order_clone.lock().push(i);
            }));
            // Stagger the spawns slightly to ensure queue order
            thread::sleep(Duration::from_millis(5));
        }

        // All writers should be queued
        thread::sleep(Duration::from_millis(10));
        assert_eq!(coord.queued_writers(), 3);

        // Release initial write
        drop(write1);

        // Wait for all writers to complete
        for h in handles {
            h.join().unwrap();
        }

        // Writers should have executed in FIFO order
        let final_order = order.lock().clone();
        assert_eq!(final_order, vec![1, 2, 3]);
    }

    #[test]
    fn concurrent_readers_with_writer_waiting() {
        let coord = Arc::new(RepoCoordinator::new());
        let reader_count = Arc::new(std::sync::atomic::AtomicU32::new(0));

        // Start multiple readers
        let mut read_guards = Vec::new();
        for _ in 0..3 {
            read_guards.push(coord.acquire_read());
        }
        assert_eq!(coord.state(), CoordinatorState::Reading(3));

        // Queue a writer
        let coord2 = Arc::clone(&coord);
        let reader_count2 = Arc::clone(&reader_count);
        let write_handle = thread::spawn(move || {
            let _guard = coord2.acquire_write();
            // Record how many readers we saw during write
            reader_count2.store(coord2.state().reader_count(), Ordering::SeqCst);
        });

        // Give writer time to queue
        thread::sleep(Duration::from_millis(10));

        // Release all readers
        drop(read_guards);

        // Wait for writer
        write_handle.join().unwrap();

        // Writer should have had exclusive access (no readers)
        assert_eq!(reader_count.load(Ordering::SeqCst), 0);
    }
}
