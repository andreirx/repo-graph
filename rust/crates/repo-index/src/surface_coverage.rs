//! HTTP surface-detector coverage — the honest answer to "which HTTP surface
//! detectors does this build ship?" for the `surfaces list` zero-state.
//!
//! # MODULES-IDENTITY-2 §2.2
//!
//! `surfaces list` on a repo with real routes but no shipped detector for its
//! framework (django URLconf on the audit's django run) returned "0 surfaces … No
//! recognized patterns" — blaming the repo for the tool's blind spot, the same class
//! RESOURCE-HONESTY-1 killed on `resource list`. The zero-state must instead state the
//! TOOL's coverage.
//!
//! Operator ruling (2026-09-01, Option A with honest scoping): enumerate the HTTP
//! surface-detector families this build ships (build-static, from the http_boundary
//! detector set — [`crate::http_boundary::HTTP_SURFACE_DETECTOR_FAMILIES`]) with wording
//! that never claims totality.
//!
//! ZEROSTATE-SCOPE-1 §2.1: the "no detector for X" clause is NO LONGER a build-static
//! blob (which pasted "Django URLconf routes" onto every repo, so leveldb — a pure C++
//! repo — wore django's sentence). The clause is now PER-REPO: the daemon consults
//! [`http_surface_detection_covers`] (which languages the HTTP surface detectors handle)
//! against the repo's materially-present code languages, and names only the uncovered
//! ones — leveldb says its C/C++ truth, django keeps URLconf. This module owns only the
//! BUILD-STATIC facts (which languages the detectors cover, and the framework-specific
//! display for a covered-language gap); the per-repo materiality gate lives in
//! `daemon-runtime::reader_context` (`surface_uncovered_material_gaps`), which composes
//! the two.
//!
//! Abstraction one-liner: `http_surface_detector_families` / `http_surface_detection_covers`
//! / `http_surface_named_gap_for` — build-static coverage accessors mirroring the
//! `resource_coverage` read-only-accessor precedent; sole callers
//! `daemon-runtime::surface_coverage_read` (populates the additive `surface_coverage`
//! DTO the `surfaces list` + `boundaries list/summary` presenters render); axis = the
//! HTTP surface detectors' languages/frameworks (the covered set + the notable Python
//! gap grow with the http_boundary composition; the colocated const + pin tests here
//! catch drift); rejected simpler = a const in the rgr presenter, which would duplicate
//! the truth across a crate boundary and drift from the detectors — repo-index owns them
//! and daemon-runtime already depends on repo-index, so this reuses the existing edge
//! (the resource_coverage home rationale) rather than adding one.

use repo_graph_state_bindings::Language;

use crate::http_boundary::HTTP_SURFACE_DETECTOR_FAMILIES;
use crate::state_boundary_hook::{classify_language, LanguageClassification};

/// The HTTP surface-detector families this build ships, as sorted reader display
/// names — the build-static detector set
/// ([`crate::http_boundary::HTTP_SURFACE_DETECTOR_FAMILIES`]), sorted + deduped so the
/// rendered coverage statement is deterministic.
pub fn http_surface_detector_families() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = HTTP_SURFACE_DETECTOR_FAMILIES.to_vec();
    names.sort_unstable();
    names.dedup();
    names
}

/// Whether this build detects HTTP SURFACES in the given indexer language token
/// (`indexer::routing::detect_language`'s output — `"typescript"`, `"java"`, `"python"`,
/// `"c"`, …).
///
/// The HTTP surface detectors composed in `http_boundary::persist_http_boundary_interactions`
/// handle exactly two language families: Java (Spring `@RestController`/`@Controller`,
/// RestTemplate/WebClient/HttpClient) and TypeScript/JavaScript (AWS CDK, axios/fetch,
/// Next.js App Router). So a token classifying to [`Language::Java`] or
/// [`Language::Typescript`] (the whole TS/JS family) is covered; every other language
/// (Python, C, C++, Rust, Go, …) is NOT — its HTTP surfaces are invisible to this build.
///
/// This is the per-repo scoping gate ZEROSTATE-SCOPE-1 §2.1 needs: the daemon calls it
/// for each materially-present code language to decide which ones to NAME as gaps. It is
/// a claim about THIS build's DETECTORS keyed on a LANGUAGE fact — never a name-based
/// classification of the repo (STANDING HONESTY RULE 2). KEEP the covered set IN SYNC
/// with the detector composition; the `detector_families` pin test guards the family
/// list, and [`covers_tracks_the_family_languages`] guards this predicate.
pub fn http_surface_detection_covers(token: &str) -> bool {
    let lang = match classify_language(Some(token)) {
        LanguageClassification::Supported(lang) | LanguageClassification::Unsupported(lang) => lang,
        LanguageClassification::Unknown => return false,
    };
    matches!(lang, Language::Java | Language::Typescript)
}

/// The framework-specific reader display for an UNCOVERED language's HTTP-surface gap, or
/// `None` to fall back to the plain language name.
///
/// Python is the one language with a NOTABLE, audit-motivating uncovered HTTP framework:
/// "Django URLconf routes" (the sentence leveldb used to wear). A materially-Python repo
/// names that framework; every other uncovered language (C, C++, Rust, …) names itself
/// (its `MaterialLanguage` display, supplied by the caller). Keyed on the LANGUAGE fact,
/// not a repo-name classification — "Django URLconf routes" is a build-static example of
/// a Python HTTP framework this build cannot see, paired by the renderer with the
/// non-totality clause, never a claim that django itself is present.
pub fn http_surface_named_gap_for(token: &str) -> Option<&'static str> {
    match classify_language(Some(token)) {
        LanguageClassification::Supported(Language::Python)
        | LanguageClassification::Unsupported(Language::Python) => Some("Django URLconf routes"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detector_families_are_the_shipped_set_sorted() {
        // Pins the CURRENT http_boundary detector set (sorted, deduped). Adding a
        // detector family to `HTTP_SURFACE_DETECTOR_FAMILIES` grows this list here —
        // the deliberate signal that the coverage statement must be re-reviewed. If a
        // detector is added there without a family, or a family drifts from the
        // detectors, this fails.
        assert_eq!(
            http_surface_detector_families(),
            vec![
                "AWS CDK API Gateway v2",
                "Java HTTP client calls (RestTemplate/WebClient/HttpClient)",
                "Java Spring (@RestController/@Controller)",
                "Next.js App Router",
                "TS/JS HTTP client calls (axios/fetch)",
            ]
        );
    }

    #[test]
    fn families_are_non_empty_and_unique() {
        // The zero-state cannot state coverage from an empty set; and a duplicate
        // family would double-print. Both are structural guarantees the renderer relies
        // on.
        let fams = http_surface_detector_families();
        assert!(!fams.is_empty(), "coverage statement needs ≥1 family");
        let mut deduped = fams.clone();
        deduped.dedup();
        assert_eq!(fams, deduped, "families must already be unique");
    }

    #[test]
    fn covers_tracks_the_family_languages() {
        // The HTTP surface detectors handle exactly Java + the TS/JS family; every family
        // member token of those is covered.
        for tok in ["java", "typescript", "tsx", "javascript", "jsx"] {
            assert!(
                http_surface_detection_covers(tok),
                "{tok} must be covered by the HTTP surface detectors"
            );
        }
        // Languages with NO HTTP surface detector on this build → not covered (so the
        // daemon names them as gaps when materially present): leveldb's C/C++, django's
        // Python, a Rust repo, an unknown token.
        for tok in ["python", "c", "cpp", "rust", "go", "kotlin", ""] {
            assert!(
                !http_surface_detection_covers(tok),
                "{tok} must NOT be covered (named as a per-repo gap when material)"
            );
        }
    }

    #[test]
    fn named_gap_is_django_for_python_only() {
        // Python — the audit's motivating framework gap — names Django URLconf; every
        // other uncovered language falls back to its own display name (caller-supplied).
        for tok in ["python"] {
            assert_eq!(
                http_surface_named_gap_for(tok),
                Some("Django URLconf routes")
            );
        }
        for tok in ["c", "cpp", "rust", "go", "java", "typescript", ""] {
            assert_eq!(
                http_surface_named_gap_for(tok),
                None,
                "{tok} has no framework-specific gap name (falls back to its language name)"
            );
        }
    }
}
