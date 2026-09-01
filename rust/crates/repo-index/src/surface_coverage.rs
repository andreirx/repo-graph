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
//! detector set — [`crate::http_boundary::HTTP_SURFACE_DETECTOR_FAMILIES`]) and name a
//! materially-present framework it has NO detector for (django URLconf), with wording
//! that never claims totality. A cross-path all-detector runtime registry
//! (RESOURCE-HONESTY-1's `default_registry` shape) was rejected as boundary-sized for
//! one sentence — if a second consumer ever appears, THAT earns it.
//!
//! Abstraction one-liner: `http_surface_detector_families` / `http_surface_named_gaps`
//! — build-static coverage accessors; sole caller `daemon-runtime::handle_surfaces_list`
//! (populates the additive `surface_coverage` DTO field the `surfaces list` presenter
//! renders); axis = HTTP surface detectors added to the http_boundary composition
//! (the family set grows; the colocated const + a pin test here catch drift); rejected
//! simpler = a const in the rgr presenter, which would duplicate the truth across a
//! crate boundary and drift from the detectors — repo-index owns them and daemon-runtime
//! already depends on repo-index, so this reuses the existing edge (the resource_coverage
//! home rationale) rather than adding one.

use crate::http_boundary::HTTP_SURFACE_DETECTOR_FAMILIES;

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

/// Known HTTP frameworks that are materially present in real repos but have NO
/// detector on this build, named so the `surfaces list` zero-state is concrete rather
/// than a bare "no patterns".
///
/// Build-static: a claim about THIS build's detectors, never a per-repo classification
/// (STANDING HONESTY RULE 2 — we do not infer "django is present" from names). "Django
/// URLconf routes" is true regardless of the repo, and the renderer pairs it with an
/// explicit non-totality clause ("other surface kinds may exist without detectors") so
/// the sentence never reads as an exhaustive gap list.
pub fn http_surface_named_gaps() -> Vec<&'static str> {
    vec!["Django URLconf routes"]
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
    fn named_gaps_name_django_urlconf() {
        // The motivating gap from the audit — surfaced as a concrete, build-static
        // example, not a per-repo claim.
        assert_eq!(http_surface_named_gaps(), vec!["Django URLconf routes"]);
    }
}
