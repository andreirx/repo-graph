//! Rust receiver type resolver using rust-analyzer LSP.
//!
//! This crate implements `ReceiverTypeResolver` for Rust code by spawning
//! rust-analyzer as a subprocess and communicating via LSP JSON-RPC.
//!
//! # Architecture
//!
//! This is a **volatile outer mechanism** adapter. It depends on:
//! - rust-analyzer binary on PATH
//! - Cargo.toml in the project
//! - LSP protocol stability
//!
//! The enrichment crate owns the stable policy/contracts. This crate
//! implements the mechanism.
//!
//! # Execution Model
//!
//! Blocking/synchronous:
//! - `std::process::Command` for subprocess
//! - Blocking stdin/stdout JSON-RPC
//! - One rust-analyzer session per Cargo context
//! - Sequential hover queries within a session
//!
//! This is appropriate for batch CLI enrichment passes.

mod cargo;
mod client;
mod receiver_locator;
mod transport;
mod types;

pub use client::RustAnalyzerResolver;

// Re-export for convenience
pub use enrichment::{ReceiverTypeResolver, ResolverError};
