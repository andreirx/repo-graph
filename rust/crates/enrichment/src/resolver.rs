//! Language-neutral resolver trait for receiver type resolution.
//!
//! This module defines the contract that language-specific resolvers must
//! implement. The trait is designed to be:
//!
//! - Language-agnostic at the interface level
//! - Batch-oriented for efficiency
//! - Failure-tolerant (partial results are acceptable)
//! - Lifecycle-aware (workspace/session management)
//!
//! # Implementation Notes
//!
//! Each language resolver is expected to:
//! 1. Initialize any necessary compiler/LSP infrastructure
//! 2. Process edges in batches (grouped by workspace/project)
//! 3. Return results for every input edge (success or failure)
//! 4. Clean up resources on drop

use crate::contracts::{EligibleEdge, EnrichmentLanguage, ReceiverTypeResult};

// ─────────────────────────────────────────────────────────────────────────────
// Resolver Trait
// ─────────────────────────────────────────────────────────────────────────────

/// A resolver that can determine receiver types for unresolved call edges.
///
/// Each implementation handles one language family and uses the appropriate
/// compiler/LSP infrastructure to resolve types.
pub trait ReceiverTypeResolver: Send + Sync {
    /// The language(s) this resolver handles.
    fn language(&self) -> EnrichmentLanguage;

    /// Resolve receiver types for a batch of eligible edges.
    ///
    /// # Contract
    ///
    /// - Returns exactly one result per input edge
    /// - Results are in the same order as inputs
    /// - Failures are reported via `ReceiverTypeResult::failed()`
    /// - Partial success is acceptable
    ///
    /// # Arguments
    ///
    /// * `repo_root` - Absolute path to the repository root
    /// * `edges` - Eligible edges to resolve (all matching this resolver's language)
    /// * `progress` - Optional callback for progress reporting
    fn resolve_batch(
        &self,
        repo_root: &std::path::Path,
        edges: &[EligibleEdge],
        progress: Option<&dyn ResolverProgress>,
    ) -> Vec<ReceiverTypeResult>;

    /// Initialize the resolver for a repository.
    ///
    /// Called once before `resolve_batch`. Implementations may:
    /// - Start LSP servers
    /// - Build compiler contexts
    /// - Index project structure
    ///
    /// Returns an error if initialization fails.
    fn initialize(&mut self, repo_root: &std::path::Path) -> Result<(), ResolverError>;

    /// Shut down the resolver, releasing resources.
    ///
    /// Called after all batches are processed. Implementations should:
    /// - Stop LSP servers
    /// - Release memory
    /// - Clean up temporary files
    fn shutdown(&mut self);
}

// ─────────────────────────────────────────────────────────────────────────────
// Progress Reporting
// ─────────────────────────────────────────────────────────────────────────────

/// Progress callback for resolver operations.
pub trait ResolverProgress: Send + Sync {
    /// Report progress during resolution.
    fn report(&self, progress: Progress);
}

/// Progress information from a resolver.
#[derive(Debug, Clone)]
pub struct Progress {
    /// Current phase of resolution.
    pub phase: ProgressPhase,

    /// Number of items processed so far.
    pub current: usize,

    /// Total number of items to process.
    pub total: usize,
}

/// Phases of the resolution process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressPhase {
    /// Initializing compiler/LSP.
    Initializing,

    /// Loading project structure.
    LoadingProject,

    /// Resolving types.
    ResolvingTypes,

    /// Completed.
    Done,
}

impl std::fmt::Display for ProgressPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Initializing => write!(f, "initializing"),
            Self::LoadingProject => write!(f, "loading_project"),
            Self::ResolvingTypes => write!(f, "resolving_types"),
            Self::Done => write!(f, "done"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────────────

/// Errors that can occur during resolution.
#[derive(Debug, thiserror::Error)]
pub enum ResolverError {
    /// The required tool (compiler, LSP server) is not available.
    #[error("tool not available: {tool}")]
    ToolNotAvailable { tool: String },

    /// Failed to start the LSP server or compiler.
    #[error("failed to start: {reason}")]
    StartupFailed { reason: String },

    /// Project structure is invalid or unsupported.
    #[error("invalid project: {reason}")]
    InvalidProject { reason: String },

    /// Timeout waiting for response.
    #[error("timeout: {operation}")]
    Timeout { operation: String },

    /// I/O error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Other error.
    #[error("{0}")]
    Other(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// Resolver Registry
// ─────────────────────────────────────────────────────────────────────────────

/// A registry of resolvers for different languages.
pub struct ResolverRegistry {
    resolvers: Vec<Box<dyn ReceiverTypeResolver>>,
}

impl ResolverRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            resolvers: Vec::new(),
        }
    }

    /// Register a resolver.
    pub fn register(&mut self, resolver: Box<dyn ReceiverTypeResolver>) {
        self.resolvers.push(resolver);
    }

    /// Get a resolver for a language.
    pub fn get(&self, language: EnrichmentLanguage) -> Option<&dyn ReceiverTypeResolver> {
        self.resolvers
            .iter()
            .find(|r| r.language() == language)
            .map(|r| r.as_ref())
    }

    /// Get a mutable resolver for a language.
    pub fn get_mut(
        &mut self,
        language: EnrichmentLanguage,
    ) -> Option<&mut dyn ReceiverTypeResolver> {
        for resolver in &mut self.resolvers {
            if resolver.language() == language {
                return Some(resolver.as_mut());
            }
        }
        None
    }

    /// Get all registered languages.
    pub fn languages(&self) -> Vec<EnrichmentLanguage> {
        self.resolvers.iter().map(|r| r.language()).collect()
    }

    /// Shutdown all resolvers.
    pub fn shutdown_all(&mut self) {
        for resolver in &mut self.resolvers {
            resolver.shutdown();
        }
    }
}

impl Default for ResolverRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Null Resolver (for testing)
// ─────────────────────────────────────────────────────────────────────────────

/// A resolver that always fails. Useful for testing.
#[derive(Debug)]
pub struct NullResolver {
    language: EnrichmentLanguage,
}

impl NullResolver {
    pub fn new(language: EnrichmentLanguage) -> Self {
        Self { language }
    }
}

impl ReceiverTypeResolver for NullResolver {
    fn language(&self) -> EnrichmentLanguage {
        self.language
    }

    fn resolve_batch(
        &self,
        _repo_root: &std::path::Path,
        edges: &[EligibleEdge],
        _progress: Option<&dyn ResolverProgress>,
    ) -> Vec<ReceiverTypeResult> {
        edges
            .iter()
            .map(|e| ReceiverTypeResult::failed(e.edge_uid.clone(), "null resolver"))
            .collect()
    }

    fn initialize(&mut self, _repo_root: &std::path::Path) -> Result<(), ResolverError> {
        Ok(())
    }

    fn shutdown(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::UnresolvedCategory;

    fn make_edge(edge_uid: &str, language: EnrichmentLanguage) -> EligibleEdge {
        EligibleEdge {
            edge_uid: edge_uid.to_string(),
            snapshot_uid: "snap-1".to_string(),
            repo_uid: "repo-1".to_string(),
            source_node_uid: "source-1".to_string(),
            target_key: "obj.method".to_string(),
            source_file_path: match language {
                EnrichmentLanguage::TypeScript => "src/main.ts".to_string(),
                EnrichmentLanguage::Rust => "src/main.rs".to_string(),
                EnrichmentLanguage::Java => "src/Main.java".to_string(),
            },
            line_start: 10,
            col_start: 5,
            category: UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            language,
        }
    }

    #[test]
    fn test_null_resolver() {
        let resolver = NullResolver::new(EnrichmentLanguage::TypeScript);
        assert_eq!(resolver.language(), EnrichmentLanguage::TypeScript);

        let edges = vec![
            make_edge("edge-1", EnrichmentLanguage::TypeScript),
            make_edge("edge-2", EnrichmentLanguage::TypeScript),
        ];

        let results = resolver.resolve_batch(std::path::Path::new("/repo"), &edges, None);

        assert_eq!(results.len(), 2);
        assert!(!results[0].is_success());
        assert!(!results[1].is_success());
        assert_eq!(results[0].failure_reason, Some("null resolver".to_string()));
    }

    #[test]
    fn test_resolver_registry() {
        let mut registry = ResolverRegistry::new();

        registry.register(Box::new(NullResolver::new(EnrichmentLanguage::TypeScript)));
        registry.register(Box::new(NullResolver::new(EnrichmentLanguage::Rust)));

        assert!(registry.get(EnrichmentLanguage::TypeScript).is_some());
        assert!(registry.get(EnrichmentLanguage::Rust).is_some());
        assert!(registry.get(EnrichmentLanguage::Java).is_none());

        let languages = registry.languages();
        assert_eq!(languages.len(), 2);
        assert!(languages.contains(&EnrichmentLanguage::TypeScript));
        assert!(languages.contains(&EnrichmentLanguage::Rust));
    }
}
