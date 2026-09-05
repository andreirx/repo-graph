//! HTTP-SURFACE-COHERENCE-1 §2.4 — read-time assembly of the `boundaries list`
//! daemon response.
//!
//! `boundaries list` is the GROUPED (file × direction) view of the SAME HTTP
//! truth `surfaces list` prints (operator ruling 2026-08-26, ruling (b) + §2.4).
//! Before this slice its HTTP rows came only from the boundary-interaction store,
//! so a repo whose HTTP providers live in the legacy `project_surfaces` family
//! (FRAKTAG's 47 Express routes) rendered them in `surfaces list` but NOT here —
//! two commands telling different stories about one snapshot.
//!
//! This module folds the read-time UNIFIED HTTP surfaces (boundary ⋈ legacy
//! `project_surfaces`, deduped — the SAME `unified_http_surfaces` the surfaces
//! footer and the boundaries-summary line count) into the boundaries-list result
//! set, so the three renderers describe one row set by construction. The
//! non-HTTP boundary rows (DB, broker, IPC) pass through unchanged.
//!
//! Abstraction record — module: `boundaries_list_read`; concrete current user:
//! `ServiceDispatcher::handle_boundaries_list`; axis: `boundaries list` response
//! assembly (union merge + filter echo + count) kept OFF the 8.9k-line
//! `dispatch.rs` per the structural guardrail, and made to share the ONE HTTP
//! aggregation §2.3 mandates; rejected simpler alternative: enriching only the
//! boundary-interaction items inline in dispatch (the review-2 finding — legacy
//! HTTP rows omitted, dispatch grown).

use std::collections::BTreeMap;

use repo_graph_boundary_interaction::{BoundaryInteractionFilter, BoundaryInteractionListItem};
use repo_graph_storage::StorageConnection;

use crate::http_boundary_read::{http_surface_display_by_uid, unified_http_surfaces};
use crate::http_surface_union::UnifiedHttpSurface;
use crate::test_composition::TestComposition;

/// Build the full `boundaries list` response (command, repo, snapshot, results,
/// count, and the filter echo). `items` are the boundary-interaction rows the DB
/// already filtered; the HTTP portion is REPLACED by the unified set unless a
/// filter selects a dimension the unified rows cannot express (`union_applies`).
///
/// `Err(reason)` when a required read fails (reader-framed) — the caller renders
/// the degradation, never a false/partial count.
pub(crate) fn boundaries_list_response_json(
    repo_uid: &str,
    snapshot_uid: &str,
    filter: &BoundaryInteractionFilter,
    items: Vec<BoundaryInteractionListItem>,
    storage: &StorageConnection,
) -> Result<serde_json::Value, String> {
    // FIXTURE-POLLUTION-1 §2.1: one read of the tracked files → path→is_test, the SAME
    // parity source the HTTP union uses (`http_boundary_read`). A failed read degrades
    // honestly (never a silent "no test rows"). ABSENT path = no `files` row = no
    // reachable evidence → `test_composition=unknown` (binding direction rule: unknown is
    // NEVER demoted, stays in the main listing WITH a reason — never a production default).
    let files = storage
        .get_files_by_repo(repo_uid)
        .map_err(|e| degraded("tracked files", e))?;
    let is_test_by_path: BTreeMap<&str, bool> =
        files.iter().map(|f| (f.path.as_str(), f.is_test)).collect();

    let mut filtered_out_note: Option<String> = None;
    let results = if union_applies(filter) {
        build_union_results(
            repo_uid,
            snapshot_uid,
            filter,
            items,
            &is_test_by_path,
            storage,
        )?
    } else {
        // A scope/family/symbol/min-confidence filter is active — dimensions the
        // legacy `project_surfaces` and unified rows genuinely do NOT carry, so
        // they could never match. Preserve the pre-slice behavior exactly: enrich
        // the already-filtered boundary-interaction items with their route labels.
        // review-4 #3: those unified-only rows must not VANISH silently — when any
        // exist for this snapshot, say so (a labeled omission, not a hidden one).
        let legacy_only = unified_http_surfaces(storage, repo_uid, snapshot_uid)?
            .iter()
            .filter(|r| r.provenance == ["project_surfaces"])
            .count();
        if legacy_only > 0 {
            filtered_out_note = Some(format!(
                "{legacy_only} HTTP surface(s) from the project-surfaces family do not carry the                  filtered dimension and are omitted from this filtered view — run without                  scope/family/symbol/confidence filters for the full set"
            ));
        }
        enrich_boundary_items(snapshot_uid, &items, &is_test_by_path, storage)?
    };

    let count = results.len();
    let mut response = serde_json::json!({
        "command": "boundaries list",
        "repo": repo_uid,
        "snapshot": snapshot_uid,
        "results": results,
        "count": count,
        // ZEROSTATE-SCOPE-1 §2.2: boundaries adopt the SAME per-repo coverage roster as
        // `surfaces list` (no second roster). The presenter renders it in the zero-state so
        // boundaries stops blaming the codebase.
        "surface_coverage":
            crate::surface_coverage_read::surface_coverage_json(storage, snapshot_uid),
    });
    if let Some(note) = filtered_out_note {
        if let serde_json::Value::Object(ref mut map) = response {
            map.insert("filtered_out".to_string(), serde_json::json!(note));
        }
    }
    if let serde_json::Value::Object(ref mut map) = response {
        add_filter_echo(map, filter);
    }
    Ok(response)
}

/// The union merge applies unless a filter selects on a dimension only the
/// boundary-interaction family carries (scope, protocol family, enclosing symbol,
/// min-confidence). Under such a filter the legacy/unified rows are inexpressible,
/// so the pre-slice boundary-only path is kept — no silent, unfilterable rows.
fn union_applies(filter: &BoundaryInteractionFilter) -> bool {
    filter.boundary_scope.is_none()
        && filter.protocol_family.is_none()
        && filter.symbol.is_none()
        && filter.min_confidence.is_none()
}

/// Non-HTTP boundary items (passed through) + the unified HTTP rows (filtered
/// in-memory to mirror the DB filter, since they are read unfiltered). HTTP rows
/// from the boundary family are dropped from `items` first, because the unified
/// set already includes them (deduped against the legacy family) — so no route is
/// counted twice.
fn build_union_results(
    repo_uid: &str,
    snapshot_uid: &str,
    filter: &BoundaryInteractionFilter,
    items: Vec<BoundaryInteractionListItem>,
    is_test_by_path: &BTreeMap<&str, bool>,
    storage: &StorageConnection,
) -> Result<Vec<serde_json::Value>, String> {
    let mut results: Vec<serde_json::Value> = Vec::new();
    for it in &items {
        if it.channel_kind.as_str() == "http" {
            continue; // replaced by the unified HTTP rows below
        }
        let mut v = serde_json::to_value(it).map_err(|e| degraded("boundary row", e))?;
        classify_path(is_test_by_path, &it.source_file).write_json(&mut v);
        results.push(v);
    }

    let unified = unified_http_surfaces(storage, repo_uid, snapshot_uid)?;
    for r in &unified {
        if unified_matches_filter(r, filter) {
            results.push(unified_row_to_entry_json(r));
        }
    }
    Ok(results)
}

/// FIXTURE-POLLUTION-1 §2.1 + binding direction rule: classify a source file's
/// test-composition from the stored `is_test` fact ONLY (never the path string). `Some(true)`
/// ⇒ `TestOnly` (demote); `Some(false)` ⇒ `Production`; ABSENT (no tracked-files row) ⇒
/// `Unknown` with a reason — NEVER collapsed to production.
fn classify_path(is_test_by_path: &BTreeMap<&str, bool>, source_file: &str) -> TestComposition {
    TestComposition::from_is_test_fact(is_test_by_path.get(source_file).copied(), source_file)
}

/// Pre-slice path: serialize every already-filtered boundary item, enriching HTTP
/// rows with a `surface_display_name` = "METHOD route" via the surface-uid join.
fn enrich_boundary_items(
    snapshot_uid: &str,
    items: &[BoundaryInteractionListItem],
    is_test_by_path: &BTreeMap<&str, bool>,
    storage: &StorageConnection,
) -> Result<Vec<serde_json::Value>, String> {
    let route_by_uid = http_surface_display_by_uid(storage, snapshot_uid)?;
    let mut results = Vec::with_capacity(items.len());
    for it in items {
        let mut v = serde_json::to_value(it).map_err(|e| degraded("boundary row", e))?;
        if let Some(obj) = v.as_object_mut() {
            let uid = obj
                .get("surfaceUid")
                .and_then(|x| x.as_str())
                .map(str::to_string);
            if let Some(disp) = uid.and_then(|u| route_by_uid.get(&u)) {
                obj.insert("surface_display_name".to_string(), serde_json::json!(disp));
            }
        }
        classify_path(is_test_by_path, &it.source_file).write_json(&mut v);
        results.push(v);
    }
    Ok(results)
}

/// In-memory equivalent of the DB filter, for the unified rows (which are read
/// unfiltered). Only the dimensions unified rows CAN express are checked; the
/// others gate the whole union off (`union_applies`).
fn unified_matches_filter(r: &UnifiedHttpSurface, filter: &BoundaryInteractionFilter) -> bool {
    if let Some(k) = filter.channel_kind {
        if k.as_str() != "http" {
            return false; // unified rows are all HTTP
        }
    }
    if let Some(d) = filter.direction {
        if d.as_str() != r.direction {
            return false;
        }
    }
    if let Some(f) = &filter.file {
        if &r.source_file != f {
            return false;
        }
    }
    if let Some(p) = &filter.file_prefix {
        if !r.source_file.starts_with(p) {
            return false;
        }
    }
    true
}

/// Project a unified HTTP surface to the `BoundaryListEntry` JSON shape the
/// presentation deserializes. `surface_display_name` carries the "METHOD route"
/// summary (a dynamic route → `METHOD <dynamic>`), so the grouped view shows the
/// methods/routes that previously lived only in `surfaces list`.
fn unified_row_to_entry_json(r: &UnifiedHttpSurface) -> serde_json::Value {
    let route = r.route.clone().unwrap_or_else(|| "<dynamic>".to_string());
    let display = format!("{} {}", r.http_method, route);
    // A stable synthetic uid (the grouped view keys on file×direction, not this).
    let uid = format!("http:{}:{}:{}", r.direction, r.http_method, r.source_file);
    // FIXTURE-POLLUTION-1 §2.1: the unified HTTP row already carries the stored `is_test`
    // fact (its own LEFT JOIN / project-family parity). `Some(true)` ⇒ TestOnly (demote);
    // `Some(false)` ⇒ Production; `None` ⇒ UNKNOWN with a reason (binding direction rule —
    // never a production default). Written additively by the shared classifier.
    let mut entry = serde_json::json!({
        "surfaceUid": uid,
        "channelKind": "http",
        "boundaryScope": "unknown",
        "direction": r.direction,
        "protocolFamily": "http",
        "sourceFile": r.source_file,
        // ANCHORS-EVERYWHERE-1: additive `lineStart`. Carried in the boundaries-list JSON
        // for machine consumers; the HUMAN grouped view (file × direction ×N) never renders
        // a line on a group headline (a group spans many rows / lines — never pick one).
        "lineStart": r.line,
        "surface_display_name": display,
    });
    TestComposition::from_is_test_fact(r.is_test, &r.source_file).write_json(&mut entry);
    entry
}

/// Echo the active filters into the response (verbatim from the pre-slice dispatch
/// block — moved here so `dispatch.rs` does not grow the boundaries-list assembly).
fn add_filter_echo(
    map: &mut serde_json::Map<String, serde_json::Value>,
    filter: &BoundaryInteractionFilter,
) {
    if filter.channel_kind.is_some() {
        map.insert(
            "filter_kind".to_string(),
            serde_json::json!(filter.channel_kind.map(|k| k.as_str())),
        );
    }
    if filter.boundary_scope.is_some() {
        map.insert(
            "filter_scope".to_string(),
            serde_json::json!(filter.boundary_scope.map(|s| s.as_str())),
        );
    }
    if filter.direction.is_some() {
        map.insert(
            "filter_direction".to_string(),
            serde_json::json!(filter.direction.map(|d| d.as_str())),
        );
    }
    if filter.protocol_family.is_some() {
        map.insert(
            "filter_family".to_string(),
            serde_json::json!(filter.protocol_family.map(|f| f.as_str())),
        );
    }
    if let Some(ref f) = filter.file {
        map.insert("filter_file".to_string(), serde_json::json!(f));
    }
    if let Some(ref p) = filter.file_prefix {
        map.insert("filter_file_prefix".to_string(), serde_json::json!(p));
    }
    if let Some(ref s) = filter.symbol {
        map.insert("filter_symbol".to_string(), serde_json::json!(s));
    }
}

/// Reader-framed degradation message (mirrors `http_boundary_read::read_degraded`).
fn degraded(context: &str, err: impl std::fmt::Display) -> String {
    format!("boundaries list {context} read failed (degraded): {err}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_surface_union::{HttpSurfaceFamily, HttpSurfaceInput};

    fn unified(
        direction: &str,
        method: &str,
        route: Option<&str>,
        file: &str,
    ) -> UnifiedHttpSurface {
        // Build via the public union so the row shape stays honest.
        let input = HttpSurfaceInput {
            direction: direction.to_string(),
            http_method: method.to_string(),
            route: route.map(str::to_string),
            source_file: file.to_string(),
            line: None,
            is_test: None,
            framework: None,
            route_unknown_reason: None,
            family: HttpSurfaceFamily::Boundary,
        };
        crate::http_surface_union::unify(vec![input], vec![], &std::collections::BTreeMap::new())
            .pop()
            .expect("one row")
    }

    #[test]
    fn union_applies_only_without_boundary_only_filters() {
        let mut f = BoundaryInteractionFilter::new();
        assert!(union_applies(&f));
        f.symbol = Some("Foo.bar".to_string());
        assert!(!union_applies(&f), "symbol filter gates the union off");
    }

    #[test]
    fn unified_row_projects_method_route_display() {
        let v = unified_row_to_entry_json(&unified("provider", "GET", Some("/api/x"), "a.ts"));
        assert_eq!(v["channelKind"], serde_json::json!("http"));
        assert_eq!(v["direction"], serde_json::json!("provider"));
        assert_eq!(v["surface_display_name"], serde_json::json!("GET /api/x"));
        assert_eq!(v["sourceFile"], serde_json::json!("a.ts"));
    }

    #[test]
    fn unified_dynamic_route_summarizes_as_dynamic() {
        let v = unified_row_to_entry_json(&unified("consumer", "POST", None, "b.ts"));
        assert_eq!(
            v["surface_display_name"],
            serde_json::json!("POST <dynamic>")
        );
    }

    #[test]
    fn filter_by_direction_and_file_applies_in_memory() {
        let provider = unified("provider", "GET", Some("/a"), "keep.ts");
        let consumer = unified("consumer", "GET", Some("/a"), "keep.ts");
        let other_file = unified("provider", "GET", Some("/a"), "drop.ts");

        let mut f = BoundaryInteractionFilter::new();
        f.direction = Some(repo_graph_boundary_interaction::Direction::Provider);
        assert!(unified_matches_filter(&provider, &f));
        assert!(!unified_matches_filter(&consumer, &f), "consumer excluded");

        let mut g = BoundaryInteractionFilter::new();
        g.file = Some("keep.ts".to_string());
        assert!(unified_matches_filter(&provider, &g));
        assert!(
            !unified_matches_filter(&other_file, &g),
            "wrong file excluded"
        );
    }

    #[test]
    fn unified_row_carries_test_composition_from_stored_fact() {
        // FIXTURE-POLLUTION-1 §2.1: the additive per-row discriminant is driven by the
        // stored is_test fact the unified row carries, never a path string.
        let mut test_row = unified("provider", "GET", Some("/a"), "tests/fixtures/api.ts");
        test_row.is_test = Some(true);
        let v = unified_row_to_entry_json(&test_row);
        assert_eq!(v["test_composition"], serde_json::json!("test_only"));

        let mut prod_row = unified("provider", "GET", Some("/b"), "src/api.ts");
        prod_row.is_test = Some(false);
        assert_eq!(
            unified_row_to_entry_json(&prod_row)["test_composition"],
            serde_json::json!("production")
        );

        // Unknown (None) is NOT demoted AND NOT production — it renders as unknown WITH a
        // reason (binding direction rule; never a false/production default).
        let mut unknown_row = unified("provider", "GET", Some("/c"), "vendor/x.ts");
        unknown_row.is_test = None;
        let uv = unified_row_to_entry_json(&unknown_row);
        assert_eq!(uv["test_composition"], serde_json::json!("unknown"));
        assert!(
            uv["test_composition_unknown_reason"]
                .as_str()
                .expect("unknown reason present")
                .contains("vendor/x.ts"),
            "{uv}"
        );
    }

    #[test]
    fn classify_path_uses_stored_fact_three_states() {
        let mut m: BTreeMap<&str, bool> = BTreeMap::new();
        m.insert("rust/crates/x/tests/fixtures/a.rs", true);
        m.insert("src/a.rs", false);
        assert_eq!(
            classify_path(&m, "rust/crates/x/tests/fixtures/a.rs"),
            TestComposition::TestOnly
        );
        assert_eq!(classify_path(&m, "src/a.rs"), TestComposition::Production);
        // Absent path ⇒ Unknown with a reason (never a production default; never demoted).
        match classify_path(&m, "unknown/file.rs") {
            TestComposition::Unknown(r) => assert!(r.contains("unknown/file.rs"), "{r}"),
            other => panic!("absent path must be Unknown, got {other:?}"),
        }
    }

    #[test]
    fn non_http_kind_filter_excludes_all_unified() {
        let provider = unified("provider", "GET", Some("/a"), "a.ts");
        let mut f = BoundaryInteractionFilter::new();
        f.channel_kind = Some(repo_graph_boundary_interaction::ChannelKind::TcpSocket);
        assert!(
            !unified_matches_filter(&provider, &f),
            "a non-http kind filter drops the http unified rows"
        );
    }
}
