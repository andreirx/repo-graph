//! Language adapter trait and registry for state-boundary extraction.
//!
//! SB-7A support substrate. Defines the clean boundary between:
//! - **Extractors**: emit `ResolvedCallsite` facts
//! - **Adapters**: convert to `StateBoundaryCallsite` DTOs (this module)
//! - **Emitter**: consumes DTOs, produces edges
//!
//! Adapters return DTOs; they do NOT write to the emitter directly.
//! This keeps the adapter layer testable without emitter setup.

use std::collections::HashMap;

use repo_graph_indexer::types::ResolvedCallsite;
use repo_graph_state_bindings::Language;

use crate::emit::StateBoundaryCallsite;

// ── Adapter context ───────────────────────────────────────────────

/// File-level context passed to adapters.
///
/// Provides minimal metadata that may be needed for conversion.
/// Adapters can ignore fields they don't need.
#[derive(Debug, Clone)]
pub struct AdapterContext<'a> {
    /// UID of the file being processed.
    pub file_uid: &'a str,
    /// Path of the file being processed.
    pub file_path: &'a str,
}

// ── Adapter trait ─────────────────────────────────────────────────

/// Language-specific adapter for converting extractor facts to
/// state-boundary callsites.
///
/// Each language implements this trait to map its `ResolvedCallsite`
/// facts to `StateBoundaryCallsite` DTOs. The adapter owns the
/// conversion logic; it does NOT write to the emitter.
///
/// # Contract
///
/// - `adapt_callsites` returns DTOs; the hook feeds them to the emitter
/// - Adapters may filter callsites (return fewer DTOs than inputs)
/// - Invalid payloads should be silently skipped, not error
pub trait LanguageStateAdapter: Send + Sync {
    /// Language this adapter handles.
    fn language(&self) -> Language;

    /// Convert resolved callsites to state-boundary callsites.
    ///
    /// Returns a vector of DTOs. The caller (hook) feeds these to
    /// the emitter. Callsites that fail validation are filtered out
    /// (not included in the result).
    fn adapt_callsites(
        &self,
        ctx: &AdapterContext<'_>,
        callsites: &[ResolvedCallsite],
    ) -> Vec<StateBoundaryCallsite>;
}

// ── Adapter registry ──────────────────────────────────────────────

/// Registry of language adapters.
///
/// Holds adapters keyed by `Language`. The hook queries this to
/// dispatch to the correct adapter for each file's language.
pub struct AdapterRegistry {
    adapters: HashMap<Language, Box<dyn LanguageStateAdapter>>,
}

impl AdapterRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
        }
    }

    /// Register an adapter for a language.
    ///
    /// If an adapter was already registered for this language,
    /// it is replaced.
    pub fn register(&mut self, adapter: Box<dyn LanguageStateAdapter>) {
        let lang = adapter.language();
        self.adapters.insert(lang, adapter);
    }

    /// Get the adapter for a language, if registered.
    pub fn get(&self, language: Language) -> Option<&dyn LanguageStateAdapter> {
        self.adapters.get(&language).map(|b| b.as_ref())
    }

    /// Check if an adapter is registered for a language.
    pub fn has(&self, language: Language) -> bool {
        self.adapters.contains_key(&language)
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a registry with all built-in adapters pre-registered.
///
/// Currently registers:
/// - `TypeScriptAdapter` for `Language::Typescript`
/// - `PythonAdapter` for `Language::Python` (SB-7C)
/// - `JavaAdapter` for `Language::Java` (SB-7B)
/// - `CAdapter` for `Language::C` (C-SB-1)
/// - `CppAdapter` for `Language::Cpp` (CPP-SB-1)
pub fn default_registry() -> AdapterRegistry {
    use crate::languages::{CAdapter, CppAdapter, JavaAdapter, PythonAdapter, TypeScriptAdapter};

    let mut registry = AdapterRegistry::new();
    registry.register(Box::new(TypeScriptAdapter::new()));
    registry.register(Box::new(PythonAdapter::new()));
    registry.register(Box::new(JavaAdapter::new()));
    registry.register(Box::new(CAdapter::new()));
    registry.register(Box::new(CppAdapter::new()));
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockAdapter {
        lang: Language,
    }

    impl LanguageStateAdapter for MockAdapter {
        fn language(&self) -> Language {
            self.lang
        }

        fn adapt_callsites(
            &self,
            _ctx: &AdapterContext<'_>,
            _callsites: &[ResolvedCallsite],
        ) -> Vec<StateBoundaryCallsite> {
            vec![] // Mock returns empty
        }
    }

    #[test]
    fn registry_register_and_get() {
        let mut registry = AdapterRegistry::new();
        registry.register(Box::new(MockAdapter { lang: Language::Typescript }));

        assert!(registry.has(Language::Typescript));
        assert!(!registry.has(Language::Python));

        let adapter = registry.get(Language::Typescript);
        assert!(adapter.is_some());
        assert_eq!(adapter.unwrap().language(), Language::Typescript);
    }

    #[test]
    fn registry_missing_language_returns_none() {
        let registry = AdapterRegistry::new();
        assert!(registry.get(Language::Python).is_none());
    }
}
