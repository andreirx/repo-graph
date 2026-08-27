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

use std::collections::BTreeMap;

use repo_graph_boundary_interaction::{
    find_http_links, BoundaryInteractionReadPort, HttpSurfaceRow,
};
use repo_graph_storage::crud::project_surfaces::SurfaceFilter;
use repo_graph_storage::StorageConnection;

use crate::http_surface_union::{self, HttpSurfaceFamily, HttpSurfaceInput, UnifiedHttpSurface};

/// Reader-framed degradation message for a failed HTTP boundary read. The
/// message names that the READER hit a read failure (a degradation), so the
/// renderer never presents it as a zero/empty fact.
fn read_degraded(context: &str, err: impl std::fmt::Display) -> String {
    format!("HTTP boundary {context} read failed (degraded): {err}")
}

/// HTTP-SURFACE-COHERENCE-1 §2.3 — the UNIFIED HTTP surfaces for a snapshot:
/// the read-time union of the boundary-interaction family and the legacy
/// `project_surfaces` HTTP family (operator ruling 2026-08-26, Option B). Returns
/// the projected JSON rows PLUS the single provider/consumer count both the
/// surfaces footer and the boundaries-summary HTTP line print — so a
/// headline/footer/summary contradiction is impossible by construction.
///
/// `Ok((rows, providers, consumers))`; `Err(reason)` when ANY of the three
/// reads fails (reader-framed) — the caller renders the degradation, never a
/// false count. A genuinely EMPTY ownership table is NOT an error: it leaves
/// each row's module `None` (rendered as the explicit unknown), honoring
/// "ownership absent → the explicit unavailable representation".
pub(crate) fn unified_http_surfaces_json(
    storage: &StorageConnection,
    repo_uid: &str,
    snapshot_uid: &str,
) -> Result<(Vec<serde_json::Value>, usize, usize), String> {
    let unified = unified_http_surfaces(storage, repo_uid, snapshot_uid)?;
    let (providers, consumers) = http_surface_union::counts(&unified);
    Ok((unified_to_json(&unified), providers, consumers))
}

/// The typed unified HTTP surfaces (before JSON projection) — the SINGLE read the
/// three HTTP sinks share so they cannot disagree (§2.3): `surfaces list`
/// (via `unified_http_surfaces_json`), `boundaries summary` (via
/// `http_summary_fields`), and `boundaries list` (via `boundaries_list_read`,
/// which folds these rows into its file×direction groups). `Err(reason)` when any
/// of the three underlying reads fails (reader-framed) — never a false count.
pub(crate) fn unified_http_surfaces(
    storage: &StorageConnection,
    repo_uid: &str,
    snapshot_uid: &str,
) -> Result<Vec<UnifiedHttpSurface>, String> {
    // ONE read of the tracked-files table feeds BOTH the project-family `is_test`
    // labels (§2.5) and the module-ownership file→path join — the boundary family
    // gets `is_test` from its own LEFT JOIN in `query_http_surfaces`, so this is
    // the parity source for the project family, keyed by the same (path) identity.
    let files = storage
        .get_files_by_repo(repo_uid)
        .map_err(|e| read_degraded("tracked files", e))?;
    // path → files.is_test. ABSENT path = no `files` row = no positive test
    // evidence (`None` at the call site), NEVER asserted non-test — mirrors the
    // boundary family's LEFT-JOIN NULL semantics exactly.
    let is_test_by_path: BTreeMap<&str, bool> =
        files.iter().map(|f| (f.path.as_str(), f.is_test)).collect();
    let boundary = read_boundary_inputs(storage, snapshot_uid)?;
    let project = read_project_http_inputs(storage, snapshot_uid, &is_test_by_path)?;
    let module_by_file = read_module_by_file(storage, snapshot_uid, &files)?;
    Ok(http_surface_union::unify(
        boundary,
        project,
        &module_by_file,
    ))
}

/// Boundary-interaction HTTP surfaces as union inputs. Failure → degrade.
fn read_boundary_inputs(
    storage: &StorageConnection,
    snapshot_uid: &str,
) -> Result<Vec<HttpSurfaceInput>, String> {
    let rows = storage
        .query_http_surfaces(snapshot_uid)
        .map_err(|e| read_degraded("surfaces", e))?;
    Ok(rows
        .into_iter()
        .map(|s| HttpSurfaceInput {
            direction: s.direction,
            http_method: s.http_method,
            route: s.route,
            source_file: s.source_file,
            is_test: s.is_test,
            framework: s.framework,
            route_unknown_reason: s.route_unknown_reason,
            family: HttpSurfaceFamily::Boundary,
        })
        .collect())
}

/// Legacy `project_surfaces` HTTP providers/consumers (e.g. Express) as union
/// inputs, parsing method/route out of `display_name` + `metadata_json`. Failure
/// → degrade. Non-HTTP surface kinds are ignored here (they stay in the project
/// surface catalog section).
fn read_project_http_inputs(
    storage: &StorageConnection,
    snapshot_uid: &str,
    is_test_by_path: &BTreeMap<&str, bool>,
) -> Result<Vec<HttpSurfaceInput>, String> {
    let surfaces = storage
        .get_project_surfaces_for_snapshot(snapshot_uid, &SurfaceFilter::default())
        .map_err(|e| read_degraded("project surfaces", e))?;
    let mut out = Vec::new();
    for s in &surfaces {
        // `?` propagates a malformed-metadata degradation — a rendered/classified
        // HTTP fact is never synthesized from silently-swallowed bad data.
        if let Some(input) = project_surface_to_input(s, is_test_by_path)? {
            out.push(input);
        }
    }
    Ok(out)
}

/// Direction implied by an HTTP `project_surfaces` kind, or `None` for a
/// non-HTTP kind (which this union ignores).
fn http_kind_direction(surface_kind: &str) -> Option<&'static str> {
    match surface_kind {
        "http_provider" => Some("provider"),
        "http_consumer" => Some("consumer"),
        _ => None,
    }
}

/// Extract a union input from a project surface iff it is an HTTP kind. Method is
/// taken from `metadata_json.httpMethod` when present (authoritative), else the
/// first token of `display_name`; the route is the remainder.
///
/// `Ok(None)` = not an HTTP-kind surface (ignored by the union). `Err(reason)` =
/// the surface HAS a `metadata_json` that failed to parse: a data-integrity
/// failure on a fact we would render/classify, so it degrades honestly (STANDING
/// HONESTY RULE) instead of being silently swallowed to `UNKNOWN`. A genuinely
/// ABSENT `metadata_json` (`None`) is not an error — the method/route fall back to
/// `display_name`.
fn project_surface_to_input(
    s: &repo_graph_storage::types::ProjectSurface,
    is_test_by_path: &BTreeMap<&str, bool>,
) -> Result<Option<HttpSurfaceInput>, String> {
    let Some(direction) = http_kind_direction(&s.surface_kind) else {
        return Ok(None);
    };
    let meta: Option<serde_json::Value> = match s.metadata_json.as_deref() {
        Some(raw) => Some(
            serde_json::from_str(raw).map_err(|e| read_degraded("project surface metadata", e))?,
        ),
        None => None,
    };
    let meta_method = meta
        .as_ref()
        .and_then(|m| m.get("httpMethod"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let framework = meta
        .as_ref()
        .and_then(|m| m.get("framework"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let (method, route) = split_method_route(s.display_name.as_deref(), meta_method.as_deref());
    let source_file = s
        .entrypoint_path
        .clone()
        .unwrap_or_else(|| s.root_path.clone());
    // §2.5 (review-3 item 2): a project-family HTTP consumer in a test file must
    // render `[test]` too. `files.is_test` for `source_file`; ABSENT path = no
    // `files` row = `None` (no positive test evidence), never asserted non-test.
    let is_test = is_test_by_path.get(source_file.as_str()).copied();
    Ok(Some(HttpSurfaceInput {
        direction: direction.to_string(),
        http_method: method,
        route,
        source_file,
        is_test,
        framework,
        route_unknown_reason: None,
        family: HttpSurfaceFamily::Project,
    }))
}

/// Split `display_name` ("GET /api/x") into (method, route), preferring an
/// explicit `meta_method`. A `display_name` that is only a method yields no
/// route (`None`); an absent method falls back to "UNKNOWN" (never fabricated as
/// a real verb).
fn split_method_route(
    display_name: Option<&str>,
    meta_method: Option<&str>,
) -> (String, Option<String>) {
    let name = display_name.unwrap_or("").trim();
    let (first, rest) = match name.split_once(char::is_whitespace) {
        Some((a, b)) => (a.to_string(), b.trim().to_string()),
        None => (name.to_string(), String::new()),
    };
    let method = meta_method
        .map(str::to_string)
        .or_else(|| {
            if first.is_empty() {
                None
            } else {
                Some(first.clone())
            }
        })
        .unwrap_or_else(|| "UNKNOWN".to_string());
    // If meta supplied the method and the display_name's first token is NOT that
    // method, the whole display_name is the route; otherwise `rest` is the route.
    let route = if meta_method.is_some() && !first.eq_ignore_ascii_case(&method) {
        if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        }
    } else if rest.is_empty() {
        None
    } else {
        Some(rest)
    };
    (method, route)
}

/// Real owning-module label per source file, from `module_file_ownership`
/// (operator ruling (a) — never a path-segment proxy). Composed from existing
/// public reads: file→module_candidate ownership ⋈ module display name, keyed by
/// the file path from the tracked-files table. An EMPTY ownership table yields an
/// empty map (every module then renders as the explicit unknown); only a genuine
/// read ERROR degrades.
fn read_module_by_file(
    storage: &StorageConnection,
    snapshot_uid: &str,
    files: &[repo_graph_storage::types::TrackedFile],
) -> Result<BTreeMap<String, String>, String> {
    let ownership = storage
        .get_file_ownership_for_snapshot(snapshot_uid)
        .map_err(|e| read_degraded("module ownership", e))?;
    if ownership.is_empty() {
        return Ok(BTreeMap::new());
    }
    let modules = storage
        .get_module_candidates_for_snapshot(snapshot_uid)
        .map_err(|e| read_degraded("module candidates", e))?;

    let label_by_module: BTreeMap<&str, String> = modules
        .iter()
        .map(|m| {
            let label = m
                .display_name
                .clone()
                .unwrap_or_else(|| m.canonical_root_path.clone());
            (m.module_candidate_uid.as_str(), label)
        })
        .collect();
    let path_by_file: BTreeMap<&str, &str> = files
        .iter()
        .map(|f| (f.file_uid.as_str(), f.path.as_str()))
        .collect();

    let mut out = BTreeMap::new();
    for o in &ownership {
        if let (Some(path), Some(label)) = (
            path_by_file.get(o.file_uid.as_str()),
            label_by_module.get(o.module_candidate_uid.as_str()),
        ) {
            out.insert((*path).to_string(), label.clone());
        }
    }
    Ok(out)
}

/// Project the unified rows to the `surfaces list` JSON shape the presentation
/// `HttpBoundarySurfaceEntry` deserializes. Carries the union additions
/// (`module`, `conflict`, `provenance`) alongside the §2.1/§2.5/§3 labels.
fn unified_to_json(rows: &[UnifiedHttpSurface]) -> Vec<serde_json::Value> {
    rows.iter()
        .map(|r| {
            serde_json::json!({
                "direction": r.direction,
                "httpMethod": r.http_method,
                "route": r.route,
                "sourceFile": r.source_file,
                "isTest": r.is_test,
                "framework": r.framework,
                "routeUnknownReason": r.route_unknown_reason,
                "module": r.module,
                "conflict": r.conflict,
                "provenance": r.provenance,
            })
        })
        .collect()
}

/// A `surface_uid → "METHOD route"` map for a snapshot's HTTP surfaces. A dynamic
/// route summarizes as `METHOD <dynamic>`. Failure → degrade. Used by
/// `boundaries_list_read` to enrich the NON-HTTP boundary rows (the HTTP rows come
/// pre-summarized from the unified set).
pub(crate) fn http_surface_display_by_uid(
    storage: &StorageConnection,
    snapshot_uid: &str,
) -> Result<std::collections::HashMap<String, String>, String> {
    let rows = storage
        .query_http_surfaces(snapshot_uid)
        .map_err(|e| read_degraded("surface routes", e))?;
    Ok(rows
        .into_iter()
        .map(|s| {
            let route = s.route.unwrap_or_else(|| "<dynamic>".to_string());
            (s.surface_uid, format!("{} {}", s.http_method, route))
        })
        .collect())
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

/// HTTP provider↔consumer link count for a snapshot over the SAME unified surface
/// set (§2.3, review-3 item 1) that feeds `surfaces list`/`boundaries summary` —
/// boundary family AND legacy `project_surfaces` — recomputed at read time via the
/// shared matcher (`find_http_links`). Used by `modules list` to decide whether the
/// "boundaries may not be meaningful" hint should be suppressed: nonzero means the
/// modules demonstrably talk over HTTP/REST even when the import graph is
/// intra-module. Feeding it from the union (not the boundary-only persisted links)
/// is what lets a legacy Express provider/consumer pair suppress the hint too.
///
/// `Ok(count)`; `Err(reason)` when any underlying read failed — the caller then
/// renders "unknown" and must NOT restore the "may not be meaningful" claim off a
/// failed read (that would present a read error as a zero fact).
pub(crate) fn unified_http_link_count(
    storage: &StorageConnection,
    repo_uid: &str,
    snapshot_uid: &str,
) -> Result<usize, String> {
    let unified = unified_http_surfaces(storage, repo_uid, snapshot_uid)?;
    Ok(count_http_links_over_unified(&unified))
}

/// Pure link count over the unified rows (§2.3): project to the matcher's row
/// shape and run the SHARED matcher. Only (direction, method, route) drive
/// matching; the uid/key fields are provenance the link COUNT ignores, so a
/// positional uid and empty key are sufficient and fabricate no false fact.
/// Because the unified set already folds in the legacy `project_surfaces` family,
/// a legacy Express provider/consumer pair counts here exactly as a boundary pair.
fn count_http_links_over_unified(unified: &[UnifiedHttpSurface]) -> usize {
    let rows: Vec<HttpSurfaceRow> = unified
        .iter()
        .enumerate()
        .map(|(i, r)| HttpSurfaceRow {
            surface_uid: i.to_string(),
            direction: r.direction.clone(),
            http_method: r.http_method.clone(),
            route: r.route.clone(),
            source_file: r.source_file.clone(),
            symbol_stable_key: String::new(),
            is_test: r.is_test,
            framework: r.framework.clone(),
            route_unknown_reason: r.route_unknown_reason.clone(),
        })
        .collect();
    let (links, _counts) = find_http_links(&rows);
    links.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_storage::types::ProjectSurface;

    fn row(direction: &str, method: &str, route: Option<&str>) -> HttpSurfaceRow {
        HttpSurfaceRow {
            surface_uid: format!("{direction}:{method}:{route:?}"),
            direction: direction.to_string(),
            http_method: method.to_string(),
            route: route.map(|r| r.to_string()),
            source_file: "f.rs".to_string(),
            symbol_stable_key: "k".to_string(),
            is_test: None,
            framework: None,
            route_unknown_reason: None,
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

    // ── review-3 item 2: project-family HTTP surfaces carry files.is_test ──────

    fn project_surface(kind: &str, display: &str, entrypoint: &str) -> ProjectSurface {
        ProjectSurface {
            project_surface_uid: format!("ps:{display}:{entrypoint}"),
            snapshot_uid: "s1".to_string(),
            repo_uid: "r1".to_string(),
            module_candidate_uid: "mc1".to_string(),
            surface_kind: kind.to_string(),
            display_name: Some(display.to_string()),
            root_path: entrypoint.to_string(),
            entrypoint_path: Some(entrypoint.to_string()),
            build_system: "npm".to_string(),
            runtime_kind: "node".to_string(),
            confidence: 0.9,
            metadata_json: Some(r#"{"httpMethod":"GET","framework":"express"}"#.to_string()),
            source_type: Some("express_route".to_string()),
            source_specific_id: None,
            stable_surface_key: Some(format!("surface:express_route:{display}")),
        }
    }

    #[test]
    fn project_consumer_in_test_file_carries_is_test_true() {
        // review-3 item 2: a legacy `project_surfaces` http_consumer whose file is a
        // test file must carry is_test=Some(true) (rendered `[test]`), exactly like a
        // boundary-family consumer. Before the fix this was hard-coded None.
        let mut is_test = BTreeMap::new();
        is_test.insert("test/api.test.ts", true);
        is_test.insert("src/api.ts", false);

        let consumer = project_surface("http_consumer", "GET /api/x", "test/api.test.ts");
        let input = project_surface_to_input(&consumer, &is_test)
            .expect("valid metadata")
            .expect("http kind");
        assert_eq!(input.is_test, Some(true), "test-file consumer is [test]");

        // A non-test file → Some(false); a file absent from `files` → None (no
        // positive test evidence, never asserted non-test).
        let provider = project_surface("http_provider", "GET /api/x", "src/api.ts");
        let pin = project_surface_to_input(&provider, &is_test)
            .unwrap()
            .unwrap();
        assert_eq!(pin.is_test, Some(false));

        let untracked = project_surface("http_provider", "GET /api/y", "vendor/z.ts");
        let uin = project_surface_to_input(&untracked, &is_test)
            .unwrap()
            .unwrap();
        assert_eq!(uin.is_test, None, "no files row ⇒ None, not false");
    }

    // ── review-3 item 1: a legacy project-family pair feeds the modules note ──

    #[test]
    fn legacy_project_pair_produces_a_link_for_modules_note() {
        // The modules note ("do these modules talk over HTTP?") is fed by
        // `unified_http_link_count`, which counts links over the UNIFIED set. A
        // provider+consumer that BOTH originate in the legacy `project_surfaces`
        // family (different files, same GET /api/x route) must yield one link — so
        // an all-legacy repo can suppress the "boundaries may not be meaningful"
        // hint just as a boundary-family repo does. Before review-3 the note read
        // boundary-only persisted links and missed this entirely.
        use crate::http_surface_union::{unify, HttpSurfaceFamily, HttpSurfaceInput};
        let project = |direction: &str, file: &str| HttpSurfaceInput {
            direction: direction.to_string(),
            http_method: "GET".to_string(),
            route: Some("/api/x".to_string()),
            source_file: file.to_string(),
            is_test: None,
            framework: Some("express".to_string()),
            route_unknown_reason: None,
            family: HttpSurfaceFamily::Project,
        };
        let unified = unify(
            vec![],
            vec![
                project("provider", "src/routes.ts"),
                project("consumer", "web/client.ts"),
            ],
            &BTreeMap::new(),
        );
        assert_eq!(
            count_http_links_over_unified(&unified),
            1,
            "legacy project provider↔consumer must count as one HTTP link"
        );
    }
}
