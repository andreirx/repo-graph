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
//! # Daemon Client
//!
//! The `daemon_client` module provides the CLI-to-daemon transport adapter.
//! It handles socket communication, fallback policy enforcement, and
//! daemon availability checking.
//!
//! # Daemon Runtime
//!
//! Daemon functionality lives in the `repo-graph-daemon-runtime` crate.
//! The `rmap daemon` command is a deprecated compatibility shim.
//! Use `rmapd` binary instead.

pub mod cli;
pub mod commands;
pub mod coverage;
pub mod daemon_client;
pub mod platform;
pub mod presentation;
