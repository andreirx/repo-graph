//! repo-graph-detectors — Rust port of the operational dependency
//! seam detector substrate.
//!
//! This crate is the Rust-side mirror of the TypeScript detector
//! substrate at `src/core/seams/detectors/` and the comment masker at
//! `src/core/seams/comment-masker.ts`. It is built incrementally as
//! the Rust-1 detector parity slice progresses.
//!
//! Slice substep state (Rust-1):
//!   - R1-A workspace skeleton ........... done
//!   - R1-B types ......................... done
//!   - R1-C loader ........................ done
//!   - R1-D comment masker ................ done
//!   - R1-E hooks ......................... done
//!   - R1-F walker ........................ done
//!   - R1-G public wrapper ................ done
//!   - R1-H curated parity fixtures ....... done
//!   - R1-I TS parity harness ............. done
//!   - R1-J Rust parity harness ........... done
//!   - R1-K CI integration ................ done
//!   - R1-L final acceptance gate ......... done
//!
//! The public surface is `detect_env_accesses` and
//! `detect_fs_mutations` re-exported from `pipeline`. Both are
//! drop-in replacements for the TS `detectEnvAccesses` /
//! `detectFsMutations` functions. The substep modules remain
//! publicly exposed so tests and advanced callers can reference
//! the contract shapes directly.

pub mod comment_masker;
pub mod hooks;
pub mod loader;
pub mod pipeline;
pub mod types;
pub mod walker;

// Convenience re-exports for the public detector API.
pub use pipeline::{detect_env_accesses, detect_fs_mutations, production_pipeline};
