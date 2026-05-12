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
/// Refreshing ──acquire_read──> BLOCKED
/// Refreshing ──acquire_write──> Conflict (coordinator waits)
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CoordinatorState {
    /// No active readers or writers.
    #[default]
    Idle,

    /// Active readers. Count must be > 0.
    Reading(u32),

    /// A write operation is in progress (index, policy write, etc.).
    Writing,

    /// A refresh operation is in progress. Semantically similar to
    /// Writing, but tracked separately for observability.
    Refreshing,
}

impl fmt::Display for CoordinatorState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
            Self::Reading(n) => write!(f, "Reading({})", n),
            Self::Writing => write!(f, "Writing"),
            Self::Refreshing => write!(f, "Refreshing"),
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
    /// Succeeds if Idle or already Reading.
    /// Blocks if Writing or Refreshing.
    pub fn try_acquire_read(&self) -> TransitionResult {
        match self {
            Self::Idle => TransitionResult::Ok(Self::Reading(1)),
            Self::Reading(n) => TransitionResult::Ok(Self::Reading(n + 1)),
            Self::Writing | Self::Refreshing => TransitionResult::Blocked(self.clone()),
        }
    }

    /// Release a read permit.
    ///
    /// Decrements the reader count. Returns Idle if count reaches 0.
    /// Invariant violation if not currently reading.
    pub fn release_read(&self) -> TransitionResult {
        match self {
            Self::Reading(1) => TransitionResult::Ok(Self::Idle),
            Self::Reading(n) if *n > 1 => TransitionResult::Ok(Self::Reading(n - 1)),
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
    /// Conflicts if already Writing or Refreshing.
    pub fn try_acquire_write(&self) -> TransitionResult {
        match self {
            Self::Idle => TransitionResult::Ok(Self::Writing),
            Self::Reading(_) => TransitionResult::Blocked(self.clone()),
            Self::Writing | Self::Refreshing => {
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
    /// Conflicts if already Writing or Refreshing.
    pub fn try_acquire_refresh(&self) -> TransitionResult {
        match self {
            Self::Idle => TransitionResult::Ok(Self::Refreshing),
            Self::Reading(_) => TransitionResult::Blocked(self.clone()),
            Self::Writing | Self::Refreshing => {
                TransitionResult::Conflict("another write operation is in progress".to_string())
            }
        }
    }

    /// Release the refresh lock.
    ///
    /// Returns to Idle.
    /// Invariant violation if not currently refreshing.
    pub fn release_refresh(&self) -> TransitionResult {
        match self {
            Self::Refreshing => TransitionResult::Ok(Self::Idle),
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
    #[allow(dead_code)] // Used in tests for state machine verification
    pub(crate) fn can_read(&self) -> bool {
        matches!(self, Self::Idle | Self::Reading(_))
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
            _ => 0,
        }
    }

    /// Check if a write operation is active.
    pub fn is_write_active(&self) -> bool {
        matches!(self, Self::Writing | Self::Refreshing)
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

    // ── Refreshing state transitions ────────────────────────────────

    #[test]
    fn refreshing_acquire_read_blocked() {
        let state = CoordinatorState::Refreshing;
        assert_eq!(
            state.try_acquire_read(),
            TransitionResult::Blocked(CoordinatorState::Refreshing)
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
    fn refreshing_release_refresh_returns_idle() {
        let state = CoordinatorState::Refreshing;
        assert_eq!(
            state.release_refresh(),
            TransitionResult::Ok(CoordinatorState::Idle)
        );
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
    fn can_read_when_idle_or_reading() {
        assert!(CoordinatorState::Idle.can_read());
        assert!(CoordinatorState::Reading(5).can_read());
        assert!(!CoordinatorState::Writing.can_read());
        assert!(!CoordinatorState::Refreshing.can_read());
    }

    #[test]
    fn can_write_only_when_idle() {
        assert!(CoordinatorState::Idle.can_write());
        assert!(!CoordinatorState::Reading(1).can_write());
        assert!(!CoordinatorState::Writing.can_write());
        assert!(!CoordinatorState::Refreshing.can_write());
    }

    #[test]
    fn reader_count_returns_correct_value() {
        assert_eq!(CoordinatorState::Idle.reader_count(), 0);
        assert_eq!(CoordinatorState::Reading(3).reader_count(), 3);
        assert_eq!(CoordinatorState::Writing.reader_count(), 0);
        assert_eq!(CoordinatorState::Refreshing.reader_count(), 0);
    }

    #[test]
    fn is_write_active_detects_write_states() {
        assert!(!CoordinatorState::Idle.is_write_active());
        assert!(!CoordinatorState::Reading(1).is_write_active());
        assert!(CoordinatorState::Writing.is_write_active());
        assert!(CoordinatorState::Refreshing.is_write_active());
    }

    #[test]
    fn display_formats_correctly() {
        assert_eq!(format!("{}", CoordinatorState::Idle), "Idle");
        assert_eq!(format!("{}", CoordinatorState::Reading(3)), "Reading(3)");
        assert_eq!(format!("{}", CoordinatorState::Writing), "Writing");
        assert_eq!(format!("{}", CoordinatorState::Refreshing), "Refreshing");
    }

    #[test]
    fn default_is_idle() {
        assert_eq!(CoordinatorState::default(), CoordinatorState::Idle);
    }
}
