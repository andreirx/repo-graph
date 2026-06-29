//! Daemon concurrency policy.
//!
//! This crate defines the readers-writer semantics for the rmap daemon.
//! It is a pure support module with no transport knowledge.
//!
//! # Architecture
//!
//! The policy is split into two layers:
//!
//! 1. **State machine** (`state` module): Pure state transitions, testable
//!    without actual concurrency. Defines valid transitions and their results.
//!
//! 2. **Coordinator** (`coordinator` module): Wraps the state machine with
//!    actual locking and FIFO writer queue semantics.
//!
//! # Key Properties
//!
//! - Multiple readers can proceed concurrently
//! - `Writing` (index/prune) is exclusive — it blocks readers and other writers
//! - **W-B (DAEMON-W-B-EPOCH-1): a `Refreshing` writer admits concurrent readers** — a refresh
//!   (or background enrich) and readers coexist; each reader proceeds against its captured request
//!   epoch so a mid-request publish of N+1 stays coherent at N. Writers remain serialized against
//!   the refresh (a second refresh/index still waits).
//! - Writers are served FIFO (first queued, first granted)
//! - Readers arriving while a writer is queued must wait (prevents writer starvation)
//!
//! # Usage
//!
//! ```
//! use repo_graph_daemon_policy::RepoCoordinator;
//!
//! let coordinator = RepoCoordinator::new();
//!
//! // Readers can proceed concurrently
//! {
//!     let _read1 = coordinator.acquire_read();
//!     let _read2 = coordinator.acquire_read();
//!     // Both active
//! }
//!
//! // Writers are exclusive
//! {
//!     let _write = coordinator.acquire_write();
//!     // Only writer active
//! }
//!
//! // Refresh is a special write
//! {
//!     let _refresh = coordinator.acquire_refresh();
//!     // Only refresher active
//! }
//! ```

mod coordinator;
mod error;
mod state;

pub use coordinator::{ReadGuard, RepoCoordinator, WriteGuard, WriteKind};
pub use error::CoordinatorError;
pub use state::{CoordinatorState, TransitionResult};
