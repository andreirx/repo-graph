//! Orient use case — public entry point.
//!
//! `orient()` is the only function callers should ever use. It
//! dispatches on the `focus` argument:
//!
//!   - `None` → repo-level pipeline (`orient_repo`), implemented
//!     in Rust-42.
//!   - `Some(_)` → returns `OrientError::FocusNotImplementedYet`.
//!     Module focus ships in Rust-44, symbol focus in Rust-45.
//!     This is deliberately an error, not a silent degrade: a
//!     caller passing a focus string must know immediately that
//!     their request was not honored.

pub mod repo;

use repo_graph_gate::GateStorageRead;

use crate::dto::budget::Budget;
use crate::dto::envelope::OrientResult;
use crate::errors::OrientError;
use crate::storage_port::AgentStorageRead;

/// Entry point for the orient use case.
///
/// Generic over a single storage handle that satisfies both
/// `AgentStorageRead` (the agent-orient port) and
/// `GateStorageRead` (the gate policy port). One concrete
/// adapter — `repo_graph_storage::StorageConnection` in
/// production, or a test fake that implements both traits —
/// fulfills both bounds, so the call site passes one value.
///
/// `now` is an ISO 8601 timestamp used for waiver expiry
/// evaluation. The agent crate is deliberately clock-free:
/// callers must supply `now` explicitly. Production callers
/// (CLI, daemon) read the system clock at their own boundary;
/// tests pass a fixed string so their outcomes are
/// deterministic. Passing a far-future or far-past sentinel
/// silently distorts waiver semantics — do not.
pub fn orient<S: AgentStorageRead + GateStorageRead + ?Sized>(
	storage: &S,
	repo_uid: &str,
	focus: Option<&str>,
	budget: Budget,
	now: &str,
) -> Result<OrientResult, OrientError> {
	match focus {
		None => repo::orient_repo(storage, repo_uid, budget, now),
		Some(f) => Err(OrientError::FocusNotImplementedYet {
			focus: f.to_string(),
		}),
	}
}
