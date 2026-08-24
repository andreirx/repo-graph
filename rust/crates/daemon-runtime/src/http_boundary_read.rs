//! HTTP-BOUNDARY-1: read-time rendering helpers for the HTTP/REST boundary map.
//!
//! Crate-private helpers used by three `ServiceDispatcher` handlers
//! (`surfaces list`, `boundaries links`, `modules list`). They read the
//! persisted `channel_kind='http'` surfaces + links through the
//! `boundary-interaction` policy crate's read port and re-run its PURE matcher
//! (`find_http_links`) — the SAME matcher the index-time linker uses — so the
//! per-consumer UNLINKED reasons are recomputed honestly at read time with NO
//! extra storage write and NO `daemon-runtime → indexer` dependency (operator
//! ruling 2026-08-24). Kept off `dispatch.rs` (8.9k lines) per the structural
//! guardrail.
//!
//! ## Honest degradation (STANDING HONESTY RULE, 2026-08-24; review-4 item 2)
//!
//! A failed storage read is UNKNOWN, never zero/empty/absent. Each helper
//! returns a `Result`: `Err(reason)` on a read failure (reader-framed), which
//! the dispatch handlers surface as a labelled degradation the renderer prints
//! — instead of an empty REST map, a silent footer, or a restored "boundaries
//! may not be meaningful" claim. Only a genuine empty read (no HTTP surfaces)
//! renders as absence.

use repo_graph_boundary_interaction::{
    find_http_links, BoundaryInteractionLinkFilter, BoundaryInteractionReadPort, HttpSurfaceRow,
};
use repo_graph_storage::StorageConnection;

/// Reader-framed degradation message for a failed HTTP boundary read. The
/// message names that the READER hit a read failure (a degradation), so the
/// renderer never presents it as a zero/empty fact.
fn read_degraded(context: &str, err: impl std::fmt::Display) -> String {
    format!("HTTP boundary {context} read failed (degraded): {err}")
}

/// The HTTP/REST provider & consumer surfaces for a snapshot, as JSON rows for
/// the `surfaces list` response. Rendered as a distinct section, never mixed
/// into `project_surfaces`. A dynamic URL keeps `route: null` — never
/// fabricated.
///
/// `Ok(rows)` (possibly empty = genuinely no HTTP surfaces); `Err(reason)` when
/// the read failed — the caller renders the degradation, never an empty map.
pub(crate) fn http_boundary_surfaces_json(
    storage: &StorageConnection,
    snapshot_uid: &str,
) -> Result<Vec<serde_json::Value>, String> {
    let rows = storage
        .query_http_surfaces(snapshot_uid)
        .map_err(|e| read_degraded("surfaces", e))?;
    Ok(surfaces_to_json(&rows))
}

/// Pure projection of surface rows to the `surfaces list` JSON shape.
fn surfaces_to_json(rows: &[HttpSurfaceRow]) -> Vec<serde_json::Value> {
    rows.iter()
        .map(|s| {
            serde_json::json!({
                "direction": s.direction,
                "httpMethod": s.http_method,
                "route": s.route,
                "sourceFile": s.source_file,
            })
        })
        .collect()
}

/// The honest per-consumer link/unlinked breakdown for `boundaries links`,
/// recomputed at read time from the persisted HTTP surfaces via the shared
/// matcher. A consumer whose route matched >1 provider or none is UNLINKED WITH
/// a reason, never guessed.
///
/// `Ok(value)` always (an empty-surface repo yields the all-zero object the
/// renderer suppresses); `Err(reason)` when the read failed — the caller then
/// renders "unknown" rather than a silent (absent) footer.
pub(crate) fn http_unlinked_json(
    storage: &StorageConnection,
    snapshot_uid: &str,
) -> Result<serde_json::Value, String> {
    let surfaces = storage
        .query_http_surfaces(snapshot_uid)
        .map_err(|e| read_degraded("link", e))?;
    Ok(unlinked_json_from_surfaces(&surfaces))
}

/// Pure projection of surface rows to the `httpUnlinked` breakdown object.
fn unlinked_json_from_surfaces(surfaces: &[HttpSurfaceRow]) -> serde_json::Value {
    let (links, counts) = find_http_links(surfaces);
    let providers = surfaces
        .iter()
        .filter(|s| s.direction == "provider")
        .count();
    let consumers = surfaces
        .iter()
        .filter(|s| s.direction == "consumer")
        .count();
    serde_json::json!({
        "providers": providers,
        "consumers": consumers,
        "linked": links.len(),
        "ambiguous": counts.ambiguous,
        "unmatched": counts.unmatched,
        "dynamicRoute": counts.dynamic_route,
    })
}

/// Count of persisted HTTP provider↔consumer links for a snapshot. Used by
/// `modules list` to decide whether the "boundaries may not be meaningful" hint
/// should be suppressed: nonzero means the modules demonstrably talk over
/// HTTP/REST even when the import graph is intra-module.
///
/// `Ok(count)`; `Err(reason)` when the read failed — the caller then renders
/// "unknown" and must NOT restore the "may not be meaningful" claim off a
/// failed read (that would present a read error as a zero fact).
pub(crate) fn http_boundary_link_count(
    storage: &StorageConnection,
    snapshot_uid: &str,
) -> Result<usize, String> {
    let mut filter = BoundaryInteractionLinkFilter::new();
    filter.link_kind = Some("http_route_match".to_string());
    storage
        .list_boundary_interaction_links(snapshot_uid, &filter)
        .map(|v| v.len())
        .map_err(|e| read_degraded("link count", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(direction: &str, method: &str, route: Option<&str>) -> HttpSurfaceRow {
        HttpSurfaceRow {
            surface_uid: format!("{direction}:{method}:{route:?}"),
            direction: direction.to_string(),
            http_method: method.to_string(),
            route: route.map(|r| r.to_string()),
            source_file: "f.rs".to_string(),
            symbol_stable_key: "k".to_string(),
        }
    }

    #[test]
    fn read_degraded_is_reader_framed() {
        // Not a bare error — a labelled degradation the renderer can present as
        // UNKNOWN, never zero.
        let msg = read_degraded("surfaces", "db locked");
        assert!(msg.contains("degraded"), "{msg}");
        assert!(msg.contains("db locked"), "{msg}");
    }

    #[test]
    fn surfaces_to_json_preserves_dynamic_route_as_null() {
        let rows = vec![
            row("provider", "GET", Some("/a/{id}")),
            row("consumer", "POST", None),
        ];
        let json = surfaces_to_json(&rows);
        assert_eq!(json.len(), 2);
        assert_eq!(json[0]["route"], serde_json::json!("/a/{id}"));
        // Dynamic URL stays null — never fabricated.
        assert_eq!(json[1]["route"], serde_json::Value::Null);
    }

    #[test]
    fn unlinked_json_counts_link_and_ambiguity() {
        // One consumer matches exactly one provider (linked); another matches two
        // (ambiguous). Proves the read-time breakdown is honest.
        let surfaces = vec![
            row("provider", "GET", Some("/offers")),
            row("consumer", "GET", Some("/offers")),
            row("provider", "GET", Some("/x/{id}")),
            row("provider", "GET", Some("/x/{name}")),
            row("consumer", "GET", Some("/x/5")),
        ];
        let v = unlinked_json_from_surfaces(&surfaces);
        assert_eq!(v["consumers"], serde_json::json!(2));
        assert_eq!(v["linked"], serde_json::json!(1));
        assert_eq!(v["ambiguous"], serde_json::json!(1));
    }
}
