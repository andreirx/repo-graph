//! Git CLI wrapper for repo-graph analytical surfaces.
//!
//! RS-MS-1: External mechanism boundary for git operations.
//!
//! This crate wraps git CLI commands behind a typed Rust interface.
//! Git is the authoritative source of history; this crate provides
//! derived analytical views for consumption by higher-level surfaces.
//!
//! # Contract
//!
//! - All paths are repo-relative with forward slashes
//! - No automatic persistence — callers decide what to do with results
//! - Deterministic ordering on all collection returns
//!
//! # Current Surfaces
//!
//! - [`get_file_churn`]: Per-file commit count and line change totals
//!
//! # Example
//!
//! ```no_run
//! use std::path::Path;
//! use repo_graph_git::{get_file_churn, ChurnWindow};
//!
//! let repo = Path::new("/path/to/repo");
//! let window = ChurnWindow::new("90.days.ago");
//!
//! let churn = get_file_churn(repo, &window).expect("git churn");
//! for entry in churn.iter().take(10) {
//!     println!(
//!         "{}: {} commits, {} lines",
//!         entry.file_path, entry.commit_count, entry.lines_changed
//!     );
//! }
//! ```

mod churn;
mod error;

pub use churn::{get_file_churn, ChurnWindow, FileChurnEntry};
pub use error::GitError;
