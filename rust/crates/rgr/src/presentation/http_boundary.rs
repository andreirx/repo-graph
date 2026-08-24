//! HTTP-BOUNDARY-1: presentation for the HTTP/REST boundary map.
//!
//! Crate-private module holding the HTTP-specific DTO + rendering that the
//! `surfaces list` and `modules list` presenters need, so those two already
//! large files (`surfaces.rs`, `modules_list.rs`) do not grow an HTTP
//! responsibility inline (operator refactor ruling 2026-08-24 + review-5 item 4;
//! the crate-private-module allowance is pre-ratified for this slice).
//!
//! Abstraction record — module: `presentation::http_boundary`; concrete current
//! users: `SurfacesListResponse::render_human` (surface section + degraded read)
//! and `ModulesListResponse::render_human` (the Layer-3 boundary note); axis:
//! one cohesive HTTP-render concern, two presenters share it across the empty/
//! degraded honesty rules; rejected simpler alternative: leaving the ~300 lines
//! inline in the two 500+-line presentation files (violates the guardrail the
//! ruling enforces).

use serde::Deserialize;

/// One HTTP/REST provider or consumer surface from the boundary-interaction
/// store (`channel_kind='http'`). Rendered as a distinct section from
/// `project_surfaces`, never mixed into them.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct HttpBoundarySurfaceEntry {
    #[serde(default)]
    pub(crate) direction: String,
    #[serde(default, rename = "httpMethod")]
    pub(crate) http_method: String,
    /// `None` = dynamic/unreadable URL — shown as `<dynamic>`, never fabricated.
    #[serde(default)]
    pub(crate) route: Option<String>,
    #[serde(default, rename = "sourceFile")]
    pub(crate) source_file: String,
}

/// Render the HTTP/REST provider & consumer map as a distinct section. Each
/// surface reads `METHOD /route  file  [provider|consumer]`; a dynamic URL shows
/// `<dynamic>`, never fabricated. Empty input → empty string (caller decides the
/// empty/degraded messaging).
pub(crate) fn render_surfaces(surfaces: &[HttpBoundarySurfaceEntry]) -> String {
    if surfaces.is_empty() {
        return String::new();
    }
    let providers = surfaces
        .iter()
        .filter(|s| s.direction == "provider")
        .count();
    let consumers = surfaces
        .iter()
        .filter(|s| s.direction == "consumer")
        .count();

    let mut out = String::new();
    out.push_str(&format!(
        "\nHTTP/REST API surfaces: {} provider{}, {} consumer{}\n",
        providers,
        if providers == 1 { "" } else { "s" },
        consumers,
        if consumers == 1 { "" } else { "s" },
    ));

    let mut entries = surfaces.to_vec();
    entries.sort_by(|a, b| {
        (&a.direction, &a.http_method, &a.route, &a.source_file).cmp(&(
            &b.direction,
            &b.http_method,
            &b.route,
            &b.source_file,
        ))
    });
    for s in &entries {
        let route = s.route.as_deref().unwrap_or("<dynamic>");
        out.push_str(&format!(
            "  {:6} {}  {}  [{}]\n",
            s.http_method, route, s.source_file, s.direction
        ));
    }
    out
}

/// Render a FAILED HTTP-surface read as UNKNOWN (review-4 item 2) — never an
/// empty REST map presented as fact.
pub(crate) fn render_surfaces_degraded(reason: &str) -> String {
    format!(
        "\nHTTP/REST API surfaces: unknown — {} (not reporting 0; rerun after reindex).\n",
        reason
    )
}

/// The `modules list` Layer-3 boundary note for the "no cross-module imports,
/// >1 module" case (review-0 item 2 + review-1 honesty + review-4 item 2).
///
/// Returns the note to append given the persisted HTTP-link count and any
/// reader-framed degradation:
/// - a degraded read OR a `None` count (both signal a failed read) → UNKNOWN,
///   which must NOT restore "boundaries may not be meaningful";
/// - `Some(n>0)` → the modules are LIKELY connected via HTTP route match, spoken
///   as the Layer-3 heuristic it is (route match, not runtime-proven);
/// - `Some(0)` → genuinely no boundaries → the original "may not be meaningful"
///   hint.
pub(crate) fn render_modules_note(link_count: Option<usize>, degraded: Option<&str>) -> String {
    match (degraded.is_some(), link_count) {
        (true, _) | (_, None) => {
            "\nnote: whether these modules talk over HTTP/REST is UNKNOWN — the \
             HTTP boundary link read degraded (not asserting the import graph is \
             complete); rerun after reindex, or see `rmap boundaries links`.\n"
                .to_string()
        }
        (false, Some(n)) if n > 0 => {
            let plural = if n == 1 { "" } else { "s" };
            format!(
                "\nnote: imports are intra-module, but these modules are likely \
                 connected via HTTP route match (heuristic, {} link{}; Layer-3 \
                 discovery, not runtime-proven) — see `rmap boundaries links`.\n",
                n, plural,
            )
        }
        (false, Some(_)) => {
            "\nhint: all imports are intra-module. Module boundaries may not be meaningful yet.\n"
                .to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
        direction: &str,
        method: &str,
        route: Option<&str>,
        file: &str,
    ) -> HttpBoundarySurfaceEntry {
        HttpBoundarySurfaceEntry {
            direction: direction.to_string(),
            http_method: method.to_string(),
            route: route.map(str::to_string),
            source_file: file.to_string(),
        }
    }

    #[test]
    fn render_surfaces_shows_providers_consumers_and_dynamic() {
        let surfaces = vec![
            entry(
                "provider",
                "GET",
                Some("/api/v2/offers/{id}"),
                "backend/OfferController.java",
            ),
            entry("consumer", "GET", None, "frontend/api.ts"),
        ];
        let out = render_surfaces(&surfaces);
        assert!(
            out.contains("HTTP/REST API surfaces: 1 provider, 1 consumer"),
            "{out}"
        );
        assert!(out.contains("GET    /api/v2/offers/{id}"), "{out}");
        assert!(
            out.contains("<dynamic>"),
            "dynamic route shown honestly: {out}"
        );
    }

    #[test]
    fn render_surfaces_empty_is_empty_string() {
        assert!(render_surfaces(&[]).is_empty());
    }

    #[test]
    fn render_surfaces_degraded_is_unknown() {
        let out = render_surfaces_degraded("db locked");
        assert!(out.contains("HTTP/REST API surfaces: unknown"), "{out}");
        assert!(out.contains("db locked"), "{out}");
    }

    #[test]
    fn modules_note_heuristic_is_honest_layer3() {
        let out = render_modules_note(Some(3), None);
        assert!(
            out.contains("likely connected via HTTP route match"),
            "{out}"
        );
        assert!(out.contains("heuristic, 3 links"), "{out}");
        assert!(out.contains("not runtime-proven"), "{out}");
        assert!(
            !out.contains("at runtime"),
            "must not claim runtime connection: {out}"
        );
    }

    #[test]
    fn modules_note_zero_links_keeps_meaningless_hint() {
        let out = render_modules_note(Some(0), None);
        assert!(
            out.contains("Module boundaries may not be meaningful"),
            "{out}"
        );
    }

    #[test]
    fn modules_note_degraded_or_none_is_unknown() {
        let out = render_modules_note(None, Some("db locked"));
        assert!(out.contains("UNKNOWN") && out.contains("degraded"), "{out}");
        assert!(
            !out.contains("Module boundaries may not be meaningful"),
            "{out}"
        );
        // A `None` count with no reason string is also a failed read → unknown.
        let out2 = render_modules_note(None, None);
        assert!(out2.contains("UNKNOWN"), "{out2}");
    }
}
