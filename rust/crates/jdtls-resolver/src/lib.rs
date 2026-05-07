//! Java receiver type resolver using Eclipse JDT Language Server.
//!
//! This crate implements `ReceiverTypeResolver` for Java files using
//! jdtls (Eclipse JDT Language Server) as the backend.
//!
//! # Architecture
//!
//! - **Direct jdtls**: No wrapper. Rust owns the subprocess lifecycle
//!   and LSP protocol handling.
//! - **Shared transport**: Uses `lsp-subprocess` crate for LSP framing,
//!   reader thread, timeout enforcement, and request correlation.
//! - **Project contexts**: One jdtls session per Java workspace
//!   (Maven, Gradle, or Eclipse project).
//!
//! # Project Detection
//!
//! Files are grouped by nearest build file with workspace promotion:
//!
//! **Module root** (nearest build file):
//! 1. `pom.xml` (Maven)
//! 2. `build.gradle.kts` (Gradle Kotlin DSL)
//! 3. `build.gradle` (Gradle Groovy DSL)
//! 4. `.project` (Eclipse)
//! 5. Repo root (fallback)
//!
//! **Workspace launch root** (for jdtls startup):
//! - For Gradle: promoted to enclosing `settings.gradle*` if exists
//! - For others: same as module root
//!
//! # Configuration
//!
//! jdtls requires an explicit path — no automatic discovery.
//! This is intentional: jdtls packaging is inconsistent.
//!
//! ```ignore
//! use jdtls_resolver::{JdtlsResolver, JdtlsConfig};
//! use enrichment::ReceiverTypeResolver;
//!
//! let config = JdtlsConfig {
//!     jdtls_path: Some("/path/to/jdtls".to_string()),
//!     ..Default::default()
//! };
//! let resolver = JdtlsResolver::with_config(config);
//! ```
//!
//! # Known Limitations
//!
//! - `calls_this_wildcard_method_needs_type_info` category is unsupported
//!   (same limitation as TypeScript resolver)
//! - Build system maturity: Maven fully supported, Gradle operational,
//!   Eclipse minimal testing
//! - Timeout defaults are provisional and environment-sensitive

mod client;
mod project;

pub use client::{JdtlsConfig, JdtlsResolver};
pub use project::{BuildSystem, JavaProjectContext};
