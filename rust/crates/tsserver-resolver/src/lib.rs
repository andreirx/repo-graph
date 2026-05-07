//! TypeScript/JavaScript type resolver using direct tsserver integration.
//!
//! This crate implements `ReceiverTypeResolver` for TypeScript and JavaScript
//! files using tsserver (TypeScript's language server) as the backend.
//!
//! # Architecture
//!
//! - **Direct tsserver**: No `typescript-language-server` wrapper. Rust owns
//!   the subprocess lifecycle and protocol handling.
//! - **TSServer protocol**: Custom protocol over stdin/stdout, not LSP.
//!   Uses seq numbers for request/response correlation.
//! - **Reader thread**: Background thread with channel for timeout enforcement.
//! - **Project contexts**: One tsserver session per tsconfig/jsconfig/package.json.
//!
//! # Project Detection
//!
//! Files are grouped by nearest config file:
//! 1. `tsconfig.json` (TypeScript project)
//! 2. `jsconfig.json` (JavaScript project with TS tooling)
//! 3. `package.json` (Node.js package boundary)
//! 4. Repo root (standalone fallback)
//!
//! # Usage
//!
//! ```ignore
//! use tsserver_resolver::TsServerResolver;
//! use enrichment::ReceiverTypeResolver;
//!
//! let resolver = TsServerResolver::new();
//! // Register with ResolverRegistry, then run through pipeline
//! ```

mod project;
mod protocol;
mod transport;
mod client;

pub use client::TsServerResolver;
