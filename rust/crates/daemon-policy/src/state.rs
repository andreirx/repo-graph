//! Coordinator state machine.
//!
//! The state machine is pure — no locking, no I/O. It defines valid
//! transitions and can be tested deterministically.

use std::fmt;

/// The state of a repo coordinator.
///
/// State transitions:
/// ```text
/// Idle ──acquire_read──> Reading(1)
/// Idle ──acquire_write─> Writing
/// Idle ──acquire_refresh─> Refreshing
///
/// Reading(n) ──acquire_read──> Reading(n+1)
/// Reading(n) ──release_read──> Reading(n-1) or Idle
/// Reading(n) ──acquire_write──> BLOCKED (must wait)
/// Reading(n) ──acquire_refresh──> BLOCKED (must wait)
///
/// Writing ──release_write──> Idle
/// Writing ──acquire_read──> BLOCKED
/// Writing ──acquire_write──> Conflict (coordinator waits)
///
/// Refreshing ──release_refresh──> Idle
/// Refreshing ──acquire_read──> RefreshingWithReaders(1)   (W-B: ADMITTED, not blocked)
/// Refreshing ──acquire_write──> Conflict (coordinator waits)
/// Refreshing ──acquire_refresh──> Conflict (coordinator waits)
///
/// RefreshingWithReaders(n) ──acquire_read──> RefreshingWithReaders(n+1)
/// RefreshingWithReaders(n) ──release_read──> RefreshingWithReaders(n-1) or Refreshing
/// RefreshingWithReaders(n) ──release_refresh──> Reading(n)   (refresh done; readers remain)
/// RefreshingWithReaders(n) ──acquire_write/refresh──> Conflict (refresh still active)
/// ```
///
/// **W-B (DAEMON-W-B-EPOCH-1, "read-during-refresh").** `Refreshing` no longer excludes
/// readers — a refresh (or background enrich) and readers coexist in
/// [`RefreshingWithReaders`](CoordinatorState::RefreshingWithReaders). Each admitted reader
/// proceeds against its captured request epoch (the pinned READY snapshot + the build-then-peek
/// LiveGraph eligibility fingerprint), so a mid-request publish of epoch N+1 is detected by the
/// per-leaf EV-A gate and the reader stays coherent at N. This relaxes ONLY the `Refreshing`
/// reader-block: `Writing` (index/prune) STILL excludes readers, and writers are STILL
/// serialized (a refresh/index/prune cannot start while a refresh is in flight, with or without
/// readers — `try_acquire_write`/`try_acquire_refresh` still `Conflict`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CoordinatorState {
    /// No active readers or writers.
    #[default]
    Idle,

    /// Active readers. Count must be > 0.
    Reading(u32),

    /// A write operation is in progress (index, policy write, etc.).
    Writing,

    /// A refresh operation is in progress with NO readers yet admitted. Semantically similar to
    /// `Writing` for writer-serialization, but tracked separately for observability AND — unlike
    /// `Writing` — it admits concurrent readers (W-B), transitioning to
    /// [`RefreshingWithReaders`](CoordinatorState::RefreshingWithReaders).
    Refreshing,

    /// A refresh operation is in progress AND `n` readers (n > 0) are admitted concurrently
    /// (W-B, DAEMON-W-B-EPOCH-1). Reached from `Refreshing` via `try_acquire_read`. The refresh
    /// holds its own LiveGraph write lock + SQLite connection for the swap; each admitted reader
    /// serves against its captured request epoch. A write/refresh cannot start while in this state
    /// (the refresh is still active → `Conflict`); when the refresh releases with readers still
    /// present the state becomes `Reading(n)`; when the last reader leaves first the state returns
    /// to `Refreshing`.
    RefreshingWithReaders(u32),
}

impl fmt::Display for CoordinatorState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
            Self::Reading(n) => write!(f, "Reading({})", n),
            Self::Writing => write!(f, "Writing"),
            Self::Refreshing => write!(f, "Refreshing"),
            Self::RefreshingWithReaders(n) => write!(f, "RefreshingWithReaders({})", n),
        }
    }
}

/// Result of attempting a state transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionResult {
    /// Transition succeeded. The new state is returned.
    Ok(CoordinatorState),

    /// Transition is blocked. The caller must wait.
    /// Contains the current state for diagnostics.
    Blocked(CoordinatorState),

    /// Transition conflicts with current state (e.g., write while writing).
    /// At the coordinator level, this becomes a wait condition, not an error.
    Conflict(String),

    /// Internal invariant violated (e.g., release without acquire).
    InvariantViolation(String),
}

impl CoordinatorState {
    /// Attempt to acquire a read permit.
    ///
    /// Succeeds if Idle, already Reading, or Refreshing/RefreshingWithReaders (W-B: a refresh
    /// admits readers). Blocks only if Writing (index/prune stays read-excluding).
    pub fn try_acquire_read(&self) -> TransitionResult {
        match self {
            Self::Idle => TransitionResult::Ok(Self::Reading(1)),
            Self::Reading(n) => TransitionResult::Ok(Self::Reading(n + 1)),
            // W-B (DAEMON-W-B-EPOCH-1): a refresh no longer blocks readers. Admit the reader
            // alongside the in-flight refresh; it proceeds against its captured request epoch (the
            // §6 whole-request join-coherence proof shows it stays coherent at N even as the refresh
            // publishes N+1).
            Self::Refreshing => TransitionResult::Ok(Self::RefreshingWithReaders(1)),
            Self::RefreshingWithReaders(n) => {
                TransitionResult::Ok(Self::RefreshingWithReaders(n + 1))
            }
            // `Writing` (index/prune) STILL excludes readers — only `Refreshing` is relaxed.
            Self::Writing => TransitionResult::Blocked(self.clone()),
        }
    }

    /// Release a read permit.
    ///
    /// Decrements the reader count. Returns Idle if count reaches 0. Under W-B the last reader
    /// leaving a concurrent refresh returns to the readerless `Refreshing` state (the refresh is
    /// still in flight). Invariant violation if not currently reading.
    pub fn release_read(&self) -> TransitionResult {
        match self {
            Self::Reading(1) => TransitionResult::Ok(Self::Idle),
            Self::Reading(n) if *n > 1 => TransitionResult::Ok(Self::Reading(n - 1)),
            // W-B: a reader admitted during a refresh leaving — if it is the last, the refresh
            // continues alone (`Refreshing`); otherwise decrement the admitted-reader count.
            Self::RefreshingWithReaders(1) => TransitionResult::Ok(Self::Refreshing),
            Self::RefreshingWithReaders(n) if *n > 1 => {
                TransitionResult::Ok(Self::RefreshingWithReaders(n - 1))
            }
            Self::Reading(0) => {
                TransitionResult::InvariantViolation("Reading(0) is invalid state".to_string())
            }
            _ => {
                TransitionResult::InvariantViolation(format!("cannot release read while {}", self))
            }
        }
    }

    /// Attempt to acquire a write lock.
    ///
    /// Succeeds only if Idle.
    /// Blocks if Reading.
    /// Conflicts if already Writing, Refreshing, or RefreshingWithReaders (a refresh is in flight —
    /// writers stay serialized even when readers were admitted, W-B).
    pub fn try_acquire_write(&self) -> TransitionResult {
        match self {
            Self::Idle => TransitionResult::Ok(Self::Writing),
            Self::Reading(_) => TransitionResult::Blocked(self.clone()),
            Self::Writing | Self::Refreshing | Self::RefreshingWithReaders(_) => {
                TransitionResult::Conflict("another write operation is in progress".to_string())
            }
        }
    }

    /// Release the write lock.
    ///
    /// Returns to Idle.
    /// Invariant violation if not currently writing.
    pub fn release_write(&self) -> TransitionResult {
        match self {
            Self::Writing => TransitionResult::Ok(Self::Idle),
            _ => {
                TransitionResult::InvariantViolation(format!("cannot release write while {}", self))
            }
        }
    }

    /// Attempt to acquire a refresh lock.
    ///
    /// Semantically identical to write, but tracked separately.
    /// Succeeds only if Idle.
    /// Blocks if Reading.
    /// Conflicts if already Writing, Refreshing, or RefreshingWithReaders (refreshes stay
    /// serialized even when readers were admitted, W-B).
    pub fn try_acquire_refresh(&self) -> TransitionResult {
        match self {
            Self::Idle => TransitionResult::Ok(Self::Refreshing),
            Self::Reading(_) => TransitionResult::Blocked(self.clone()),
            Self::Writing | Self::Refreshing | Self::RefreshingWithReaders(_) => {
                TransitionResult::Conflict("another write operation is in progress".to_string())
            }
        }
    }

    /// Release the refresh lock.
    ///
    /// Returns to Idle when no readers were admitted; under W-B, returns to `Reading(n)` when `n`
    /// readers are still admitted (the refresh finished first; the readers continue as plain
    /// readers, the refresh's exclusion lifted). Invariant violation if not currently refreshing.
    pub fn release_refresh(&self) -> TransitionResult {
        match self {
            Self::Refreshing => TransitionResult::Ok(Self::Idle),
            Self::RefreshingWithReaders(n) => TransitionResult::Ok(Self::Reading(*n)),
            _ => TransitionResult::InvariantViolation(format!(
                "cannot release refresh while {}",
                self
            )),
        }
    }

    /// Check if the state machine allows reads (ignores queue fairness).
    ///
    /// **Note:** This is a state-machine-level predicate only. The coordinator
    /// may block reads even when this returns true (e.g., if writers are queued).
    /// Use `RepoCoordinator::try_acquire_read()` for actual admission checks.
    ///
    /// Under W-B, `Refreshing`/`RefreshingWithReaders` admit reads — only `Writing`
    /// (index/prune) is read-excluding.
    #[allow(dead_code)] // Used in tests for state machine verification
    pub(crate) fn can_read(&self) -> bool {
        matches!(
            self,
            Self::Idle | Self::Reading(_) | Self::Refreshing | Self::RefreshingWithReaders(_)
        )
    }

    /// Check if the state machine allows writes (ignores queue fairness).
    ///
    /// **Note:** This is a state-machine-level predicate only. The coordinator
    /// may block writes even when this returns true (e.g., if other writers are queued).
    /// Use `RepoCoordinator::try_acquire_write()` for actual admission checks.
    #[allow(dead_code)] // Used in tests for state machine verification
    pub(crate) fn can_write(&self) -> bool {
        matches!(self, Self::Idle)
    }

    /// Get the current reader count.
    pub fn reader_count(&self) -> u32 {
        match self {
            Self::Reading(n) => *n,
            Self::RefreshingWithReaders(n) => *n,
            _ => 0,
        }
    }

    /// Check if a write operation is active.
    ///
    /// `RefreshingWithReaders` counts as write-active: a refresh IS in flight (it merely coexists
    /// with admitted readers under W-B).
    pub fn is_write_active(&self) -> bool {
        matches!(
            self,
            Self::Writing | Self::Refreshing | Self::RefreshingWithReaders(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Idle state transitions ──────────────────────────────────────

    #[test]
    fn idle_acquire_read_succeeds() {
        let state = CoordinatorState::Idle;
        assert_eq!(
            state.try_acquire_read(),
            TransitionResult::Ok(CoordinatorState::Reading(1))
        );
    }

    #[test]
    fn idle_acquire_write_succeeds() {
        let state = CoordinatorState::Idle;
        assert_eq!(
            state.try_acquire_write(),
            TransitionResult::Ok(CoordinatorState::Writing)
        );
    }

    #[test]
    fn idle_acquire_refresh_succeeds() {
        let state = CoordinatorState::Idle;
        assert_eq!(
            state.try_acquire_refresh(),
            TransitionResult::Ok(CoordinatorState::Refreshing)
        );
    }

    // ── Reading state transitions ───────────────────────────────────

    #[test]
    fn reading_acquire_read_increments_count() {
        let state = CoordinatorState::Reading(2);
        assert_eq!(
            state.try_acquire_read(),
            TransitionResult::Ok(CoordinatorState::Reading(3))
        );
    }

    #[test]
    fn reading_release_read_decrements_count() {
        let state = CoordinatorState::Reading(3);
        assert_eq!(
            state.release_read(),
            TransitionResult::Ok(CoordinatorState::Reading(2))
        );
    }

    #[test]
    fn reading_release_last_read_returns_idle() {
        let state = CoordinatorState::Reading(1);
        assert_eq!(
            state.release_read(),
            TransitionResult::Ok(CoordinatorState::Idle)
        );
    }

    #[test]
    fn reading_acquire_write_blocked() {
        let state = CoordinatorState::Reading(2);
        assert_eq!(
            state.try_acquire_write(),
            TransitionResult::Blocked(CoordinatorState::Reading(2))
        );
    }

    #[test]
    fn reading_acquire_refresh_blocked() {
        let state = CoordinatorState::Reading(1);
        assert_eq!(
            state.try_acquire_refresh(),
            TransitionResult::Blocked(CoordinatorState::Reading(1))
        );
    }

    // ── Writing state transitions ───────────────────────────────────

    #[test]
    fn writing_acquire_read_blocked() {
        let state = CoordinatorState::Writing;
        assert_eq!(
            state.try_acquire_read(),
            TransitionResult::Blocked(CoordinatorState::Writing)
        );
    }

    #[test]
    fn writing_acquire_write_conflicts() {
        let state = CoordinatorState::Writing;
        match state.try_acquire_write() {
            TransitionResult::Conflict(msg) => {
                assert!(msg.contains("another write operation"));
            }
            other => panic!("expected Conflict, got {:?}", other),
        }
    }

    #[test]
    fn writing_acquire_refresh_conflicts() {
        let state = CoordinatorState::Writing;
        match state.try_acquire_refresh() {
            TransitionResult::Conflict(msg) => {
                assert!(msg.contains("another write operation"));
            }
            other => panic!("expected Conflict, got {:?}", other),
        }
    }

    #[test]
    fn writing_release_write_returns_idle() {
        let state = CoordinatorState::Writing;
        assert_eq!(
            state.release_write(),
            TransitionResult::Ok(CoordinatorState::Idle)
        );
    }

    // ── Refreshing state transitions (W-B: read-during-refresh) ──────

    /// THE W-B flip at the state-machine level: a `Refreshing` writer no longer BLOCKS a reader —
    /// it ADMITS it, transitioning to `RefreshingWithReaders(1)`. (Under W-A this returned
    /// `Blocked(Refreshing)`; that test is replaced by this one.)
    #[test]
    fn refreshing_acquire_read_admits_reader() {
        let state = CoordinatorState::Refreshing;
        assert_eq!(
            state.try_acquire_read(),
            TransitionResult::Ok(CoordinatorState::RefreshingWithReaders(1))
        );
    }

    #[test]
    fn refreshing_acquire_write_conflicts() {
        let state = CoordinatorState::Refreshing;
        match state.try_acquire_write() {
            TransitionResult::Conflict(msg) => {
                assert!(msg.contains("another write operation"));
            }
            other => panic!("expected Conflict, got {:?}", other),
        }
    }

    #[test]
    fn refreshing_acquire_refresh_conflicts() {
        // Refreshes stay SERIALIZED: a second refresh cannot start while one is in flight.
        let state = CoordinatorState::Refreshing;
        match state.try_acquire_refresh() {
            TransitionResult::Conflict(msg) => {
                assert!(msg.contains("another write operation"));
            }
            other => panic!("expected Conflict, got {:?}", other),
        }
    }

    #[test]
    fn refreshing_release_refresh_returns_idle() {
        let state = CoordinatorState::Refreshing;
        assert_eq!(
            state.release_refresh(),
            TransitionResult::Ok(CoordinatorState::Idle)
        );
    }

    // ── RefreshingWithReaders transitions (W-B) ──────────────────────

    #[test]
    fn refreshing_with_readers_acquire_read_increments() {
        let state = CoordinatorState::RefreshingWithReaders(2);
        assert_eq!(
            state.try_acquire_read(),
            TransitionResult::Ok(CoordinatorState::RefreshingWithReaders(3))
        );
    }

    #[test]
    fn refreshing_with_readers_release_last_reader_returns_to_refreshing() {
        // The refresh is still in flight; only the reader left.
        let state = CoordinatorState::RefreshingWithReaders(1);
        assert_eq!(
            state.release_read(),
            TransitionResult::Ok(CoordinatorState::Refreshing)
        );
    }

    #[test]
    fn refreshing_with_readers_release_one_of_many_decrements() {
        let state = CoordinatorState::RefreshingWithReaders(3);
        assert_eq!(
            state.release_read(),
            TransitionResult::Ok(CoordinatorState::RefreshingWithReaders(2))
        );
    }

    #[test]
    fn refreshing_with_readers_release_refresh_leaves_plain_readers() {
        // The refresh finished first; the admitted readers continue as plain `Reading(n)`.
        let state = CoordinatorState::RefreshingWithReaders(2);
        assert_eq!(
            state.release_refresh(),
            TransitionResult::Ok(CoordinatorState::Reading(2))
        );
    }

    #[test]
    fn refreshing_with_readers_acquire_write_conflicts() {
        // Writers stay serialized against the in-flight refresh even with readers admitted.
        let state = CoordinatorState::RefreshingWithReaders(1);
        match state.try_acquire_write() {
            TransitionResult::Conflict(msg) => {
                assert!(msg.contains("another write operation"));
            }
            other => panic!("expected Conflict, got {:?}", other),
        }
    }

    #[test]
    fn refreshing_with_readers_acquire_refresh_conflicts() {
        let state = CoordinatorState::RefreshingWithReaders(1);
        match state.try_acquire_refresh() {
            TransitionResult::Conflict(msg) => {
                assert!(msg.contains("another write operation"));
            }
            other => panic!("expected Conflict, got {:?}", other),
        }
    }

    // ── Invariant violations ────────────────────────────────────────

    #[test]
    fn idle_release_read_is_invariant_violation() {
        let state = CoordinatorState::Idle;
        match state.release_read() {
            TransitionResult::InvariantViolation(msg) => {
                assert!(msg.contains("cannot release read"));
            }
            other => panic!("expected InvariantViolation, got {:?}", other),
        }
    }

    #[test]
    fn idle_release_write_is_invariant_violation() {
        let state = CoordinatorState::Idle;
        match state.release_write() {
            TransitionResult::InvariantViolation(msg) => {
                assert!(msg.contains("cannot release write"));
            }
            other => panic!("expected InvariantViolation, got {:?}", other),
        }
    }

    #[test]
    fn writing_release_refresh_is_invariant_violation() {
        let state = CoordinatorState::Writing;
        match state.release_refresh() {
            TransitionResult::InvariantViolation(msg) => {
                assert!(msg.contains("cannot release refresh"));
            }
            other => panic!("expected InvariantViolation, got {:?}", other),
        }
    }

    #[test]
    fn refreshing_release_write_is_invariant_violation() {
        let state = CoordinatorState::Refreshing;
        match state.release_write() {
            TransitionResult::InvariantViolation(msg) => {
                assert!(msg.contains("cannot release write"));
            }
            other => panic!("expected InvariantViolation, got {:?}", other),
        }
    }

    // ── Helper methods ──────────────────────────────────────────────

    #[test]
    fn can_read_when_idle_reading_or_refreshing() {
        assert!(CoordinatorState::Idle.can_read());
        assert!(CoordinatorState::Reading(5).can_read());
        // W-B: a refresh (with or without readers) admits reads; only Writing excludes them.
        assert!(CoordinatorState::Refreshing.can_read());
        assert!(CoordinatorState::RefreshingWithReaders(2).can_read());
        assert!(!CoordinatorState::Writing.can_read());
    }

    #[test]
    fn can_write_only_when_idle() {
        assert!(CoordinatorState::Idle.can_write());
        assert!(!CoordinatorState::Reading(1).can_write());
        assert!(!CoordinatorState::Writing.can_write());
        assert!(!CoordinatorState::Refreshing.can_write());
        assert!(!CoordinatorState::RefreshingWithReaders(1).can_write());
    }

    #[test]
    fn reader_count_returns_correct_value() {
        assert_eq!(CoordinatorState::Idle.reader_count(), 0);
        assert_eq!(CoordinatorState::Reading(3).reader_count(), 3);
        assert_eq!(CoordinatorState::Writing.reader_count(), 0);
        assert_eq!(CoordinatorState::Refreshing.reader_count(), 0);
        // W-B: admitted readers during a refresh are counted.
        assert_eq!(CoordinatorState::RefreshingWithReaders(4).reader_count(), 4);
    }

    #[test]
    fn is_write_active_detects_write_states() {
        assert!(!CoordinatorState::Idle.is_write_active());
        assert!(!CoordinatorState::Reading(1).is_write_active());
        assert!(CoordinatorState::Writing.is_write_active());
        assert!(CoordinatorState::Refreshing.is_write_active());
        // W-B: a refresh coexisting with readers is still write-active.
        assert!(CoordinatorState::RefreshingWithReaders(1).is_write_active());
    }

    #[test]
    fn display_formats_correctly() {
        assert_eq!(format!("{}", CoordinatorState::Idle), "Idle");
        assert_eq!(format!("{}", CoordinatorState::Reading(3)), "Reading(3)");
        assert_eq!(format!("{}", CoordinatorState::Writing), "Writing");
        assert_eq!(format!("{}", CoordinatorState::Refreshing), "Refreshing");
        assert_eq!(
            format!("{}", CoordinatorState::RefreshingWithReaders(2)),
            "RefreshingWithReaders(2)"
        );
    }

    #[test]
    fn default_is_idle() {
        assert_eq!(CoordinatorState::default(), CoordinatorState::Idle);
    }
}
