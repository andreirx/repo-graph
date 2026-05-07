//! Compiler-assisted enrichment subsystem.
//!
//! This crate provides the infrastructure for resolving receiver types on
//! unresolved `obj.method()` calls using compiler/LSP assistance, and
//! optionally promoting a safe subset to resolved graph edges.
//!
//! # Architecture
//!
//! The enrichment subsystem is structured as:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                         Pipeline                                 │
//! │  (orchestrates eligibility → resolution → persistence → promo)   │
//! └─────────────────────────────────────────────────────────────────┘
//!                              │
//!          ┌───────────────────┼───────────────────┐
//!          ▼                   ▼                   ▼
//! ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
//! │  Resolver   │     │  Resolver   │     │  Resolver   │
//! │  (Rust)     │     │  (TS)       │     │  (Java)     │
//! └─────────────┘     └─────────────┘     └─────────────┘
//!          │                   │                   │
//!          └───────────────────┼───────────────────┘
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                       Promotion Filter                           │
//! │  (8-gate safety check: category, origin, type uniqueness, etc.)  │
//! └─────────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                      Storage Port                                │
//! │  (query edges, persist metadata, insert promoted)                │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Modules
//!
//! - [`contracts`]: Core data types (DTOs, results, candidates)
//! - [`status`]: Enrichment state and execution reporting
//! - [`eligibility`]: Storage port for querying/persisting
//! - [`promotion`]: 8-gate safety filter for edge promotion
//! - [`resolver`]: Language-neutral resolver trait
//! - [`pipeline`]: Orchestration of the full enrichment flow
//!
//! # Usage
//!
//! ```ignore
//! use enrichment::{
//!     EnrichmentPipeline, EnrichmentConfig,
//!     eligibility::EnrichmentStoragePort,
//!     resolver::ResolverRegistry,
//! };
//!
//! // Create pipeline with storage adapter
//! let storage = MyStorageAdapter::new(db);
//! let mut pipeline = EnrichmentPipeline::new(storage);
//!
//! // Register resolvers
//! pipeline.registry_mut().register(Box::new(RustAnalyzerResolver::new()));
//!
//! // Run enrichment
//! let config = EnrichmentConfig::new()
//!     .with_limit(10_000)
//!     .with_promotion();
//!
//! let report = pipeline.run("repo-uid", "snapshot-uid", &config)?;
//!
//! println!("Enriched: {}/{}", report.enriched_count, report.eligible_count);
//! ```
//!
//! # Design Principles
//!
//! 1. **Pure core logic**: Promotion and contracts are I/O-free, fully testable.
//! 2. **Storage as port**: Database access is abstracted behind a trait.
//! 3. **Language-neutral pipeline**: Resolvers plug in via trait implementation.
//! 4. **Explicit failure handling**: Every edge gets a result, success or failure.
//! 5. **Idempotent promotion**: Re-running promotion doesn't create duplicates.

pub mod contracts;
pub mod eligibility;
pub mod pipeline;
pub mod promotion;
pub mod resolver;
pub mod status;

// Re-export commonly used types at crate root
pub use contracts::{
    EligibleEdge, EnrichmentLanguage, EnrichmentMetadata, PromotedEdge, PromotionCandidate,
    ReceiverTypeOrigin, ReceiverTypeResult, SymbolInfo, SymbolSubtype, UnresolvedCategory,
};

pub use eligibility::{EligibilityQuery, EnrichmentStoragePort, StorageError};

pub use pipeline::{EnrichmentConfig, EnrichmentPipeline, PipelineError};

pub use promotion::{promote_edges, PromotionContext, PromotionResult};

pub use resolver::{
    NullResolver, Progress, ProgressPhase, ReceiverTypeResolver, ResolverError, ResolverProgress,
    ResolverRegistry,
};

pub use status::{
    EnrichmentReport, EnrichmentState, FailureCount, LanguageReport, PromotionReport, ReportBuilder,
    TypeCount,
};
