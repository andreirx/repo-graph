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
//! - **Project contexts**: One tsserver session per owning tsconfig/jsconfig.
//!
//! # Project Ownership
//!
//! The `ownership` module determines which tsconfig.json owns each file
//! by evaluating actual config semantics:
//!
//! - `files` (explicit file list)
//! - `include` / `exclude` (glob patterns)
//! - `extends` (config inheritance)
//! - `references` (composite project structure)
//!
//! This replaces the naive "nearest config by directory" heuristic that
//! fails in monorepos with project references.
//!
//! Ownership outcomes:
//! - **Owned**: Exactly one config claims the file.
//! - **Ambiguous**: Multiple configs claim the file (explicit failure).
//! - **Unowned**: No config covers the file (explicit failure).
//!
//! # Receiver Localization
//!
//! The `receiver_locator` module uses tree-sitter to extract receiver
//! expressions from call sites. This enables resolution of the
//! `calls_this_wildcard_method_needs_type_info` category where the edge
//! position points at the method name, but we need the type of the
//! intermediate receiver (e.g., `this.field` in `this.field.method()`).
//!
//! Architecture split:
//! - **tree-sitter**: Deterministic syntax extraction (receiver span)
//! - **tsserver**: Semantic type authority (type at receiver span)
//!
//! # Legacy Grouping
//!
//! The `project` module contains the original directory-ancestry grouping.
//! It remains available for fallback comparison but is not the primary
//! ownership source.
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

pub mod ownership;
pub mod receiver_locator;

mod client;
mod locate;
mod project;
mod protocol;
mod transport;

pub use client::TsServerResolver;
pub use locate::locate_tsserver;
pub use ownership::{OwnershipError, ProjectOwnership, TsProjectOwnershipResolver};
pub use project::group_by_project_root;
pub use receiver_locator::{ReceiverLocation, ReceiverLocator};
