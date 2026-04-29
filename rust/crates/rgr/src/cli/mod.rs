//! Shared CLI infrastructure for rmap commands.
//!
//! # Boundary rules
//!
//! This module owns **shared infrastructure only**:
//! - Storage connection opening and repo resolution
//! - Query result envelope construction
//! - Trust overlay computation
//! - Usage and error formatting
//! - Time utilities
//! - Future: shared argument parsing helpers (only if reused across families)
//!
//! This module does **not** own:
//! - command handlers (live in `crate::commands`)
//! - family-specific DTOs (live in `crate::commands`)
//! - family-specific parsing or helpers (live in `crate::commands`)
//!
//! If a helper is only used by one command family, it belongs in that
//! family's module, not here.

mod context;
mod envelope;
mod time;
mod usage;

pub use context::{open_storage, resolve_repo_ref};
pub use envelope::{build_envelope, compute_trust_overlay_for_snapshot};
pub use time::{chrono_now, utc_now_iso8601};
pub use usage::{format_gate_error, print_usage};
