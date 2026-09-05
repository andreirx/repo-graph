//! HTTP-SURFACE-COHERENCE-1 §2.3 — read-time union of the two HTTP surface
//! families (operator ruling 2026-08-26, Option B, read-time only).
//!
//! Two families independently record HTTP surfaces:
//! - the **boundary-interaction** store (`channel_kind='http'`), populated by the
//!   Spring/App-Router/axios detectors (HTTP-BOUNDARY-1 + this slice §2.1/§2.2);
//! - the legacy **`project_surfaces`** store, whose `http_provider`/`http_consumer`
//!   kinds the TypeScript project-surface extractor emits for Express routes.
//!
//! Before this slice they rendered as two disjoint sections, so FRAKTAG's 47
//! `project_surfaces` providers sat under a boundary footer that said
//! "0 providers". This module reconciles them at READ TIME with NO storage write
//! and NO family migration, under an EXPLICIT dedup identity:
//! `(http_method, normalized route template, source file path)`.
//!
//! Reconciliation-over-adjudication (the codebase's ratified doctrine): a row
//! present in both families with the SAME direction renders ONCE (preferring the
//! richer record, carrying provenance); an identity collision with a CONFLICTING
//! direction renders BOTH rows with a labeled conflict — never a silent drop.
//!
//! Abstraction record — module: `http_surface_union`; concrete current users:
//! `http_boundary_read::unified_http_surfaces_json` (feeding `surfaces list`,
//! `boundaries summary`, and the modules dual-implementation note); axis: two
//! storage families that must reconcile into ONE HTTP count at read time;
//! rejected simpler alternative: each renderer counting its own family (the exact
//! headline-vs-footer drift the v0.9.0 audit measured). Pure (no I/O) so the
//! dedup/conflict identity is unit-tested without a database.

use std::collections::BTreeMap;

/// Which storage family an input surface came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HttpSurfaceFamily {
    /// `boundary_interaction_surfaces` (`channel_kind='http'`).
    Boundary,
    /// `project_surfaces` (`http_provider`/`http_consumer` kinds).
    Project,
}

impl HttpSurfaceFamily {
    fn label(self) -> &'static str {
        match self {
            HttpSurfaceFamily::Boundary => "boundary",
            HttpSurfaceFamily::Project => "project_surfaces",
        }
    }

    /// Richness rank for "prefer the richer record" when two rows of the SAME
    /// direction share an identity. Boundary rows carry `is_test` + the
    /// unknown-route reason off the shared read, so they win ties; missing labels
    /// are still back-filled from the loser so no fact is lost.
    fn richness(self) -> u8 {
        match self {
            HttpSurfaceFamily::Boundary => 1,
            HttpSurfaceFamily::Project => 0,
        }
    }
}

/// One HTTP surface normalized from either family, before union.
#[derive(Debug, Clone)]
pub(crate) struct HttpSurfaceInput {
    pub direction: String,
    pub http_method: String,
    /// `None` = dynamic/unreadable URL — never fabricated, never dedup-merged.
    pub route: Option<String>,
    pub source_file: String,
    /// ANCHORS-EVERYWHERE-1 (Tier 1): the surface's start line, for the `path:line`
    /// anchor. `None` for the project family (no stored line) or an absent line — never
    /// fabricated. On dedup the richest record's line wins, back-filled from the other.
    pub line: Option<u64>,
    pub is_test: Option<bool>,
    pub framework: Option<String>,
    pub route_unknown_reason: Option<String>,
    pub family: HttpSurfaceFamily,
}

/// A unified HTTP surface after read-time union + dedup (§2.3 / Option B).
#[derive(Debug, Clone)]
pub(crate) struct UnifiedHttpSurface {
    pub direction: String,
    pub http_method: String,
    pub route: Option<String>,
    pub source_file: String,
    /// ANCHORS-EVERYWHERE-1 (Tier 1): the surface's start line for the `path:line` anchor
    /// on individual rows (`surfaces list`) — NOT rendered on grouped `boundaries list`
    /// headlines. `None` = no single-source line (never fabricated).
    pub line: Option<u64>,
    pub is_test: Option<bool>,
    pub framework: Option<String>,
    pub route_unknown_reason: Option<String>,
    /// Owning module (from `module_file_ownership`); `None` = ownership
    /// unavailable — rendered as the explicit unknown, never a path-segment proxy.
    pub module: Option<String>,
    /// Families that contributed this row (for the union's witness provenance).
    pub provenance: Vec<&'static str>,
    /// Set when this identity ALSO appears with a conflicting direction; both
    /// rows render, each labeled — never a silent drop.
    pub conflict: Option<String>,
}

/// The dedup identity (§2.3): `(uppercased method, normalized route template,
/// source file)`. Only rows with a concrete route are eligible; a dynamic route
/// (`None`) is never merged.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Identity {
    method: String,
    route: String,
    source_file: String,
}

/// Normalize a route template so `{id}` and `{param}` (or `[slug]`, `:name`) at
/// the same position compare equal, while literal segments stay distinct. Trailing
/// slash dropped; empty → `/`.
fn normalize_route(route: &str) -> String {
    let core = route.trim().trim_end_matches('/');
    if core.is_empty() {
        return "/".to_string();
    }
    core.split('/')
        .map(|seg| {
            let is_param = (seg.starts_with('{') && seg.ends_with('}'))
                || (seg.starts_with('[') && seg.ends_with(']'))
                || seg.starts_with(':');
            if is_param {
                "{}".to_string()
            } else {
                seg.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn identity_of(input: &HttpSurfaceInput) -> Option<Identity> {
    input.route.as_ref().map(|r| Identity {
        method: input.http_method.to_uppercase(),
        route: normalize_route(r),
        source_file: input.source_file.clone(),
    })
}

fn to_unified(input: &HttpSurfaceInput, conflict: Option<String>) -> UnifiedHttpSurface {
    UnifiedHttpSurface {
        direction: input.direction.clone(),
        http_method: input.http_method.clone(),
        route: input.route.clone(),
        source_file: input.source_file.clone(),
        line: input.line,
        is_test: input.is_test,
        framework: input.framework.clone(),
        route_unknown_reason: input.route_unknown_reason.clone(),
        module: None,
        provenance: vec![input.family.label()],
        conflict,
    }
}

/// Pick the richest input for a given direction within a conflict/dup set.
fn richest_for_direction<'a>(
    inputs: &'a [&'a HttpSurfaceInput],
    direction: &str,
) -> Option<&'a HttpSurfaceInput> {
    inputs
        .iter()
        .filter(|i| i.direction == direction)
        .copied()
        .max_by_key(|i| i.family.richness())
}

/// Reconcile the two families into one deduped list (§2.3). `module_by_file`
/// supplies the real owning module per source file (`module_file_ownership`);
/// an absent entry leaves `module = None` (rendered as the explicit unknown).
pub(crate) fn unify(
    boundary: Vec<HttpSurfaceInput>,
    project: Vec<HttpSurfaceInput>,
    module_by_file: &BTreeMap<String, String>,
) -> Vec<UnifiedHttpSurface> {
    let all: Vec<HttpSurfaceInput> = boundary.into_iter().chain(project).collect();

    // Rows with a dynamic (None) route are never merged — index them straight
    // through. Rows with a concrete route group by dedup identity.
    let mut groups: BTreeMap<Identity, Vec<usize>> = BTreeMap::new();
    let mut dynamic: Vec<usize> = Vec::new();
    for (i, input) in all.iter().enumerate() {
        match identity_of(input) {
            Some(id) => groups.entry(id).or_default().push(i),
            None => dynamic.push(i),
        }
    }

    let mut out: Vec<UnifiedHttpSurface> = Vec::new();

    for (_id, idxs) in groups {
        let inputs: Vec<&HttpSurfaceInput> = idxs.iter().map(|&i| &all[i]).collect();
        let mut directions: Vec<String> = inputs.iter().map(|i| i.direction.clone()).collect();
        directions.sort();
        directions.dedup();

        if directions.len() <= 1 {
            // Same (or single) direction → render ONCE, preferring the richer
            // record, back-filling any label the richest lacks (so a dedup never
            // loses a fact), and unioning provenance across all duplicates.
            let rich = *inputs
                .iter()
                .max_by_key(|i| i.family.richness())
                .expect("group is non-empty");
            let mut row = to_unified(rich, None);
            for other in &inputs {
                if row.is_test.is_none() {
                    row.is_test = other.is_test;
                }
                if row.framework.is_none() {
                    row.framework = other.framework.clone();
                }
                if row.route_unknown_reason.is_none() {
                    row.route_unknown_reason = other.route_unknown_reason.clone();
                }
                if row.route.is_none() {
                    row.route = other.route.clone();
                }
                // ANCHORS-EVERYWHERE-1: the richest record's line wins; back-fill from the
                // other family (typically project → boundary) so a dedup never loses the line.
                if row.line.is_none() {
                    row.line = other.line;
                }
            }
            let mut prov: Vec<&'static str> = inputs.iter().map(|i| i.family.label()).collect();
            prov.sort_unstable();
            prov.dedup();
            row.provenance = prov;
            out.push(row);
        } else {
            // Conflicting directions at the same identity → render BOTH (one row
            // per direction), each labeled. Never a silent drop.
            for dir in &directions {
                let others: Vec<&str> = directions
                    .iter()
                    .filter(|d| *d != dir)
                    .map(String::as_str)
                    .collect();
                let mut fams: Vec<&'static str> = inputs.iter().map(|i| i.family.label()).collect();
                fams.sort_unstable();
                fams.dedup();
                let label = format!(
                    "identity also recorded as {} (families: {})",
                    others.join(", "),
                    fams.join(", ")
                );
                if let Some(rich) = richest_for_direction(&inputs, dir) {
                    out.push(to_unified(rich, Some(label)));
                }
            }
        }
    }

    for i in dynamic {
        out.push(to_unified(&all[i], None));
    }

    // Attach the real owning module per source file.
    for row in &mut out {
        row.module = module_by_file.get(&row.source_file).cloned();
    }

    // Total order for deterministic rendering/counting.
    out.sort_by(|a, b| {
        (&a.direction, &a.http_method, &a.route, &a.source_file).cmp(&(
            &b.direction,
            &b.http_method,
            &b.route,
            &b.source_file,
        ))
    });
    out
}

/// Provider/consumer counts over the UNIFIED rows — the single source of truth
/// both the surfaces footer and the boundaries-summary HTTP line print.
pub(crate) fn counts(rows: &[UnifiedHttpSurface]) -> (usize, usize) {
    let providers = rows.iter().filter(|r| r.direction == "provider").count();
    let consumers = rows.iter().filter(|r| r.direction == "consumer").count();
    (providers, consumers)
}

/// COHERENCE-3 (§2.2): the provider/consumer counts PARTITIONED by the stored `is_test` fact —
/// the shape `orient`'s HTTP headline needs so it excludes test fixtures exactly as the `surfaces`
/// command and the `boundaries summary` HTTP line do (all three then state the same production
/// count). Test-fixture rows (`is_test == Some(true)`) are excluded from `providers`/`consumers`
/// and tallied in `test_fixture_excluded`; unknown-`is_test` rows (`None`) STAY counted (never
/// demoted) but are tallied in `test_status_unknown` so they are disclosed, never invisible
/// (RULE #1). Classified ONLY from the stored fact — never a path heuristic (RULE #2).
pub(crate) struct HttpSurfacePartition {
    pub providers: usize,
    pub consumers: usize,
    pub test_fixture_excluded: usize,
    pub test_status_unknown: usize,
}

pub(crate) fn counts_partitioned(rows: &[UnifiedHttpSurface]) -> HttpSurfacePartition {
    let mut providers = 0;
    let mut consumers = 0;
    let mut test_fixture_excluded = 0;
    let mut test_status_unknown = 0;
    for r in rows {
        if r.is_test == Some(true) {
            test_fixture_excluded += 1;
            continue;
        }
        if r.is_test.is_none() {
            test_status_unknown += 1;
        }
        match r.direction.as_str() {
            "provider" => providers += 1,
            "consumer" => consumers += 1,
            _ => {}
        }
    }
    HttpSurfacePartition {
        providers,
        consumers,
        test_fixture_excluded,
        test_status_unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(
        family: HttpSurfaceFamily,
        direction: &str,
        method: &str,
        route: Option<&str>,
        file: &str,
    ) -> HttpSurfaceInput {
        HttpSurfaceInput {
            direction: direction.to_string(),
            http_method: method.to_string(),
            route: route.map(str::to_string),
            source_file: file.to_string(),
            line: None,
            is_test: None,
            framework: None,
            route_unknown_reason: None,
            family,
        }
    }

    #[test]
    fn normalize_route_makes_param_names_positional() {
        assert_eq!(
            normalize_route("/api/v2/clients/{id}"),
            "/api/v2/clients/{}"
        );
        assert_eq!(
            normalize_route("/api/v2/clients/{param}/"),
            "/api/v2/clients/{}"
        );
        assert_eq!(normalize_route("/users/[slug]"), "/users/{}");
        assert_eq!(normalize_route("/users/:name"), "/users/{}");
        // Literal segments stay distinct.
        assert_ne!(normalize_route("/a/x"), normalize_route("/a/y"));
    }

    #[test]
    fn same_identity_same_direction_renders_once_with_provenance() {
        // The SAME route in both families (same method+route+file) → ONE row.
        let boundary = vec![input(
            HttpSurfaceFamily::Boundary,
            "provider",
            "GET",
            Some("/api/v2/clients/{id}"),
            "backend/Ctrl.java",
        )];
        let project = vec![input(
            HttpSurfaceFamily::Project,
            "provider",
            "GET",
            Some("/api/v2/clients/{param}"), // different param name, same template
            "backend/Ctrl.java",
        )];
        let rows = unify(boundary, project, &BTreeMap::new());
        assert_eq!(rows.len(), 1, "deduped to one: {rows:?}");
        assert_eq!(rows[0].provenance, vec!["boundary", "project_surfaces"]);
        assert_eq!(rows[0].conflict, None);
    }

    #[test]
    fn richer_boundary_record_wins_and_backfills_labels() {
        let mut b = input(
            HttpSurfaceFamily::Boundary,
            "provider",
            "GET",
            Some("/x"),
            "f.java",
        );
        b.is_test = Some(true);
        let mut p = input(
            HttpSurfaceFamily::Project,
            "provider",
            "GET",
            Some("/x"),
            "f.java",
        );
        p.framework = Some("express".to_string());
        let rows = unify(vec![b], vec![p], &BTreeMap::new());
        assert_eq!(rows.len(), 1);
        // is_test from boundary (richer) AND framework back-filled from project.
        assert_eq!(rows[0].is_test, Some(true));
        assert_eq!(rows[0].framework.as_deref(), Some("express"));
    }

    #[test]
    fn conflicting_direction_renders_both_with_label() {
        // Same identity, one family says provider, the other consumer → BOTH
        // render, each labeled — the union surfaces divergence, never drops it.
        let boundary = vec![input(
            HttpSurfaceFamily::Boundary,
            "provider",
            "GET",
            Some("/api/x"),
            "svc.ts",
        )];
        let project = vec![input(
            HttpSurfaceFamily::Project,
            "consumer",
            "GET",
            Some("/api/x"),
            "svc.ts",
        )];
        let rows = unify(boundary, project, &BTreeMap::new());
        assert_eq!(rows.len(), 2, "both render: {rows:?}");
        assert!(rows.iter().all(|r| r.conflict.is_some()));
        assert!(rows.iter().any(|r| r.direction == "provider"));
        assert!(rows.iter().any(|r| r.direction == "consumer"));
    }

    #[test]
    fn dynamic_routes_never_merge() {
        // Two dynamic (None-route) rows in the same file+method are NOT merged —
        // fabricating an identity for an unknown URL is forbidden.
        let boundary = vec![
            input(HttpSurfaceFamily::Boundary, "consumer", "GET", None, "a.ts"),
            input(HttpSurfaceFamily::Boundary, "consumer", "GET", None, "a.ts"),
        ];
        let rows = unify(boundary, vec![], &BTreeMap::new());
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn module_attached_from_ownership_absent_is_none() {
        let boundary = vec![input(
            HttpSurfaceFamily::Boundary,
            "provider",
            "GET",
            Some("/x"),
            "backend/A.java",
        )];
        let mut map = BTreeMap::new();
        map.insert("backend/A.java".to_string(), "core-api".to_string());
        let rows = unify(boundary, vec![], &map);
        assert_eq!(rows[0].module.as_deref(), Some("core-api"));

        // Absent ownership → None (explicit unknown at render, not a proxy).
        let b2 = vec![input(
            HttpSurfaceFamily::Boundary,
            "provider",
            "GET",
            Some("/y"),
            "unowned/B.java",
        )];
        let rows2 = unify(b2, vec![], &map);
        assert_eq!(rows2[0].module, None);
    }

    #[test]
    fn counts_reflect_unified_rows() {
        let boundary = vec![
            input(
                HttpSurfaceFamily::Boundary,
                "provider",
                "GET",
                Some("/a"),
                "f",
            ),
            input(
                HttpSurfaceFamily::Boundary,
                "consumer",
                "GET",
                Some("/b"),
                "g",
            ),
        ];
        let project = vec![input(
            HttpSurfaceFamily::Project,
            "provider",
            "POST",
            Some("/c"),
            "h",
        )];
        let rows = unify(boundary, project, &BTreeMap::new());
        assert_eq!(counts(&rows), (2, 1));
    }
}
