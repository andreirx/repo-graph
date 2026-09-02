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

    /// RESOURCE-CPP-INERT-1 (FINAL-POLISH-1 §2.3): a reader-frame phrase naming the resource-access
    /// MECHANISM this adapter's language actually detects — the specific access CALLS it recognizes,
    /// NOT the language it parses. `resource list` renders this so its coverage statement describes
    /// what the detector DOES (e.g. "fopen/open/sqlite3 calls") instead of overclaiming full-language
    /// coverage (the original "covers C, C++" defect: a bare-language claim wider than the specific
    /// access calls actually recognized). Static per adapter — the mechanism is a property of the
    /// detector, not of any repo.
    ///
    /// Ratified as a public API addition (FINAL-POLISH-1 operator ruling 2026-09-02, precedent
    /// chain 9th instance). Carries a DEFAULT so the addition is **non-breaking in form**: an
    /// existing/external implementer that does not override it still compiles, and — per the honesty
    /// mission — renders unknown-with-reason ("mechanism not declared") rather than a fabricated
    /// coverage claim. Every built-in adapter (`default_registry`) overrides it with its true
    /// detected call-family; the default is unreachable in the shipped registry.
    fn mechanism(&self) -> &'static str {
        "resource-access detection (mechanism not declared by this adapter)"
    }

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

    /// The languages this registry has a resource-access adapter for.
    ///
    /// Order is unspecified (HashMap iteration); callers that render this
    /// must impose their own deterministic ordering. Used by
    /// `repo-index`'s resource-coverage accessor (RESOURCE-HONESTY-1) so a
    /// coverage statement enumerates the ACTUALLY-registered languages from
    /// this one registry — never a hand-maintained list that could drift
    /// from `default_registry`.
    pub fn registered_languages(&self) -> Vec<Language> {
        self.adapters.keys().copied().collect()
    }

    /// RESOURCE-CPP-INERT-1 (§2.3): each registered adapter's `(language, mechanism)` — the ONE
    /// registry the coverage statement reads so the per-language mechanism naming can never drift
    /// from what actually runs. Order is unspecified (HashMap iteration); the caller imposes a
    /// deterministic order (repo-index's `resource_detector_mechanisms`).
    pub fn language_mechanisms(&self) -> Vec<(Language, &'static str)> {
        self.adapters
            .iter()
            .map(|(lang, adapter)| (*lang, adapter.mechanism()))
            .collect()
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

        fn mechanism(&self) -> &'static str {
            "mock calls"
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
        registry.register(Box::new(MockAdapter {
            lang: Language::Typescript,
        }));

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

    /// An adapter that does NOT override `mechanism()` — proves the trait addition is
    /// non-breaking in form (this compiles without the method) and that the default is
    /// honest degradation, not a fabricated call-family claim (FINAL-POLISH-1 §2.3).
    struct MechanismlessAdapter;

    impl LanguageStateAdapter for MechanismlessAdapter {
        fn language(&self) -> Language {
            Language::Rust
        }

        fn adapt_callsites(
            &self,
            _ctx: &AdapterContext<'_>,
            _callsites: &[ResolvedCallsite],
        ) -> Vec<StateBoundaryCallsite> {
            vec![]
        }
    }

    #[test]
    fn default_mechanism_is_honest_unknown_not_an_overclaim() {
        let m = MechanismlessAdapter.mechanism();
        // Reads as unknown-with-reason, never as a specific detected call family.
        assert!(
            m.contains("not declared"),
            "default must state it is unknown: {m}"
        );
        assert!(
            !m.contains("fopen") && !m.contains("open()") && !m.contains("JDBC"),
            "default must not fabricate a mechanism it does not detect: {m}"
        );
    }
}
