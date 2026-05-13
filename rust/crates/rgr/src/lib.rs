//! Library entry point for `repo-graph-rgr`.
//!
//! This lib.rs exists to expose internal modules for integration testing.
//! The primary entry point is the `rmap` binary defined in main.rs.
//!
//! # Integration Test Support
//!
//! The `commands` module is exposed publicly so that integration tests
//! in `tests/` can call command handlers directly without spawning
//! a subprocess. This enables testing exit codes, argument parsing,
//! and database interactions at the function level.
//!
//! # Daemon
//!
//! Daemon functionality has been moved to the `repo-graph-daemon-runtime`
//! crate. The `rmap daemon` command is a deprecated compatibility shim
//! that calls into daemon-runtime. Use `rmapd` binary instead.

pub mod cli;
pub mod commands;
pub mod coverage;
pub mod platform;
