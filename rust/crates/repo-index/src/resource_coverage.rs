//! Resource-access detector coverage — the one honest answer to "what does this
//! build's resource-access (state-boundary) detection cover?".
//!
//! # RESOURCE-HONESTY-1
//!
//! `resource list` must state its coverage so its zero-state never blames the
//! codebase for the tool's blind spots and a single detected file never poses as
//! a repo inventory (slice `docs/slices/resource-honesty-1.md`). Both facts derive
//! from the SAME detector registry the extraction hook wires
//! ([`default_registry`]) — never a hand-maintained list here that could silently
//! drift from what actually runs:
//!
//! - [`resource_detector_language_names`] enumerates the registered adapter
//!   languages (registry membership) as reader display names.
//! - [`resource_detection_covers`] answers, for one indexer language token,
//!   whether this build detects resource access in it — the DECISION is the
//!   registry's [`AdapterRegistry::has`], with [`classify_language`] supplying
//!   only the token→`Language` family dictionary (ts/tsx/js/jsx → Typescript).
//!
//! Adding a resource adapter to `default_registry` therefore updates BOTH answers
//! automatically, with no edit to this module or to the `resource list` surface —
//! the invariant the slice's registry-propagation test pins.
//!
//! Home rationale: `repo-index` is the composition root that already owns the hook
//! wiring (`state_boundary_hook`) and the `classify_language` family dictionary, and
//! `daemon-runtime` already depends on it — so this reuses the existing edge
//! (daemon-runtime → repo-index → state-extractor) rather than adding a new one.

use repo_graph_state_bindings::Language;
use repo_graph_state_extractor::default_registry;

use crate::state_boundary_hook::{classify_language, LanguageClassification};

/// Reader-frame display name for a state-boundary detector language.
///
/// Exhaustive over [`Language`] (a new variant breaks this match — the deliberate
/// signal that a new detector language needs a reader name). The `Typescript`
/// variant covers the whole TS/JS family (its own enum doc), so it reads as
/// `TypeScript/JavaScript`.
fn detector_language_display(lang: Language) -> &'static str {
    match lang {
        Language::Rust => "Rust",
        Language::Typescript => "TypeScript/JavaScript",
        Language::Python => "Python",
        Language::Java => "Java",
        Language::C => "C",
        Language::Cpp => "C++",
    }
}

/// The languages this build's resource-access detection covers, as sorted, deduped
/// reader display names — derived from the detector registry
/// ([`default_registry`]'s registered adapters), never a hardcoded list.
///
/// Sorted so the rendered coverage statement is deterministic (the registry's
/// iteration order is not).
pub fn resource_detector_language_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = default_registry()
        .registered_languages()
        .into_iter()
        .map(detector_language_display)
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

/// RESOURCE-CPP-INERT-1 (FINAL-POLISH-1 §2.3): each covered language paired with the reader-frame
/// MECHANISM its detector actually recognizes (`[("C", "fopen/open/sqlite3 calls"), …]`), sorted by
/// display name for deterministic rendering. Derived from the SAME detector registry as
/// [`resource_detector_language_names`] (each adapter's `mechanism()`), so a registry change
/// propagates here untouched.
///
/// `resource list` renders this so its coverage line describes what the detector DOES per language
/// (specific access CALLS) instead of claiming full-language coverage — the original "covers C, C++"
/// defect (a bare-language claim wider than the specific calls recognized).
pub fn resource_detector_mechanisms() -> Vec<(&'static str, &'static str)> {
    let mut pairs: Vec<(&'static str, &'static str)> = default_registry()
        .language_mechanisms()
        .into_iter()
        .map(|(lang, mechanism)| (detector_language_display(lang), mechanism))
        .collect();
    pairs.sort_unstable_by(|a, b| a.0.cmp(b.0));
    pairs
}

/// Whether this build detects resource access in the given indexer language token
/// (`indexer::routing::detect_language`'s output — `"typescript"`, `"rust"`, `"c"`,
/// …).
///
/// The coverage DECISION is the registry's ([`AdapterRegistry::has`]); a token with
/// no `Language` in the family dictionary (`"go"`, `"kotlin"`, …) is not covered.
/// A token that maps to a `Language` with no registered adapter (`"rust"` today) is
/// likewise not covered — and flips to covered the moment an adapter is registered
/// for it, with no change here.
pub fn resource_detection_covers(token: &str) -> bool {
    let lang = match classify_language(Some(token)) {
        LanguageClassification::Supported(lang) | LanguageClassification::Unsupported(lang) => lang,
        LanguageClassification::Unknown => return false,
    };
    default_registry().has(lang)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detector_languages_are_the_registered_adapters_sorted() {
        // The five shipped adapters (TS/JS, Python, Java, C, C++), reader-framed,
        // sorted + deduped. This asserts the CURRENT registry; if an adapter is
        // added the list grows here WITHOUT any edit to the resource surface.
        assert_eq!(
            resource_detector_language_names(),
            vec!["C", "C++", "Java", "Python", "TypeScript/JavaScript"]
        );
    }

    #[test]
    fn mechanisms_name_what_each_detector_does_not_the_language() {
        // RESOURCE-CPP-INERT-1 §2.3: every covered language is paired with the MECHANISM its
        // detector recognizes (specific access calls), sorted by display name — the same five
        // registered adapters as the languages list. This pins the CURRENT registry; a new adapter
        // grows this list from its own `mechanism()` with no edit to the resource surface.
        assert_eq!(
            resource_detector_mechanisms(),
            vec![
                ("C", "fopen/open/sqlite3 calls"),
                ("C++", "fopen/open/sqlite3 and std::fstream calls"),
                ("Java", "JDBC DriverManager.getConnection calls"),
                ("Python", "open() and sqlite3/psycopg2 calls"),
                ("TypeScript/JavaScript", "Node fs read/write calls"),
            ]
        );
        // The mechanism strings NEVER claim bare language coverage ("covers C++") — they name calls.
        for (_lang, mech) in resource_detector_mechanisms() {
            assert!(
                mech.contains("call"),
                "mechanism must name access calls, not the language: {mech}"
            );
        }
    }

    #[test]
    fn covers_reports_registry_membership_via_family_tokens() {
        // Every family-member token of a registered adapter is covered.
        for tok in [
            "typescript",
            "tsx",
            "javascript",
            "jsx",
            "python",
            "java",
            "c",
            "cpp",
        ] {
            assert!(resource_detection_covers(tok), "{tok} must be covered");
        }
        // Rust maps to a Language variant but has NO registered adapter → not covered.
        assert!(!resource_detection_covers("rust"));
        // A token with no detector Language at all → not covered.
        assert!(!resource_detection_covers("go"));
        assert!(!resource_detection_covers("kotlin"));
        assert!(!resource_detection_covers(""));
    }

    #[test]
    fn covers_agrees_with_the_enumerated_coverage() {
        // The predicate and the enumeration are two reads of the SAME registry: a
        // covered token's display name must appear in the enumerated list.
        let names = resource_detector_language_names();
        assert!(resource_detection_covers("c"));
        assert!(names.contains(&"C"));
        assert!(!resource_detection_covers("rust"));
        assert!(!names.contains(&"Rust"));
    }
}
