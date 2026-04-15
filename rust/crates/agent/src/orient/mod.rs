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

use crate::dto::budget::Budget;
use crate::dto::envelope::OrientResult;
use crate::errors::OrientError;
use crate::storage_port::AgentStorageRead;

/// Entry point for the orient use case.
///
/// Takes a `&dyn AgentStorageRead` (or any `S: AgentStorageRead`)
/// so CLI wiring and daemon transport can share the same
/// function without changes.
pub fn orient<S: AgentStorageRead + ?Sized>(
	storage: &S,
	repo_uid: &str,
	focus: Option<&str>,
	budget: Budget,
) -> Result<OrientResult, OrientError> {
	match focus {
		None => repo::orient_repo(storage, repo_uid, budget),
		Some(f) => Err(OrientError::FocusNotImplementedYet {
			focus: f.to_string(),
		}),
	}
}
