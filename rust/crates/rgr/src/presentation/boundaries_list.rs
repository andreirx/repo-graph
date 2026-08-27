//! Presentation layer for boundaries list command.
//!
//! # CLI-OUT-4 Group 5
//!
//! Response DTO and human renderer for `boundaries list`.
//!
//! ## Change Axis
//!
//! This file changes when:
//! - Boundary catalog format changes
//! - Filter display changes
//! - List row formatting changes
//!
//! It does NOT change when:
//! - boundaries show changes
//! - boundaries summary changes

use serde::Deserialize;

use super::module_shared::format_count;

// =============================================================================
// BOUNDARIES LIST RESPONSE
// =============================================================================

/// A boundary entry in the list response.
#[derive(Debug, Clone, Deserialize)]
pub struct BoundaryListEntry {
    #[serde(default, rename = "surfaceUid")]
    pub boundary_channel_uid: String,
    #[serde(default, rename = "channelKind")]
    pub channel_kind: String,
    #[serde(default, rename = "boundaryScope")]
    pub boundary_scope: String,
    #[serde(default)]
    pub direction: String,
    #[serde(default, rename = "protocolFamily")]
    pub protocol_family: Option<String>,
    #[serde(default)]
    pub service_name: Option<String>,
    #[serde(default, rename = "sourceFile")]
    pub file_path: Option<String>,
    #[serde(default, rename = "symbolStableKey")]
    pub symbol_key: Option<String>,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub basis: Option<String>,
    #[serde(default)]
    pub surface_uid: Option<String>,
    #[serde(default)]
    pub surface_display_name: Option<String>,
}

/// Response structure for boundaries list command.
#[derive(Debug, Deserialize)]
pub struct BoundariesListResponse {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub snapshot: String,
    #[serde(default)]
    pub results: Vec<BoundaryListEntry>,
    #[serde(default)]
    pub count: u64,
    // Filter echo fields (if present in response)
    #[serde(default)]
    pub filter_kind: Option<String>,
    #[serde(default)]
    pub filter_scope: Option<String>,
    #[serde(default)]
    pub filter_direction: Option<String>,
    #[serde(default)]
    pub filter_family: Option<String>,
    #[serde(default)]
    pub filter_file: Option<String>,
    #[serde(default)]
    pub filter_file_prefix: Option<String>,
    #[serde(default)]
    pub filter_symbol: Option<String>,
}

impl BoundariesListResponse {
    /// Render as human-readable text.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // -- Header --
        out.push_str("Boundaries\n\n");

        // -- Count --
        out.push_str(&format!(
            "{}\n",
            format_count(self.count as usize, "boundary", "boundaries")
        ));

        // -- Active filters --
        let mut filters = Vec::new();
        if let Some(ref k) = self.filter_kind {
            filters.push(format!("kind={}", k));
        }
        if let Some(ref s) = self.filter_scope {
            filters.push(format!("scope={}", s));
        }
        if let Some(ref d) = self.filter_direction {
            filters.push(format!("direction={}", d));
        }
        if let Some(ref f) = self.filter_family {
            filters.push(format!("family={}", f));
        }
        if let Some(ref f) = self.filter_file {
            filters.push(format!("file={}", f));
        }
        if let Some(ref p) = self.filter_file_prefix {
            filters.push(format!("file-prefix={}", p));
        }
        if let Some(ref s) = self.filter_symbol {
            filters.push(format!("symbol={}", s));
        }
        if !filters.is_empty() {
            out.push_str(&format!("Filtered by: {}\n", filters.join(", ")));
        }

        // -- Empty case --
        if self.results.is_empty() {
            out.push_str(
                "\nhint: boundaries are interactions between code and external systems.\n",
            );
            out.push_str("      No recognized boundary patterns found in this codebase.\n");
            return out;
        }

        out.push_str(&render_grouped(&self.results));
        out
    }
}

/// Max methods/routes listed per group before summarizing the tail as `+K more`.
const MAX_ROUTES_PER_GROUP: usize = 6;

/// §2.4 (operator ruling (b)): `boundaries list` is the GROUPED view of the same
/// boundary rows, keyed literally on **file × direction**, with `×N` counts, the
/// methods/routes summarized, and constant-valued columns (kind, scope, family)
/// lifted out and stated ONCE. Strictly a summary of `surfaces list` (it shares
/// its HTTP-surface read for the route detail), so the audit's "74%
/// verbatim-duplicate rows / scope unknown on every row" collapses to signal.
/// Channel-kind-AGNOSTIC — gRPC / DB / broker rows group the same way.
fn render_grouped(rows: &[BoundaryListEntry]) -> String {
    // Accumulate one group per (file, direction); the other columns are collected
    // as per-group value SETS (a file×direction may legitimately span >1 kind).
    let mut groups: std::collections::BTreeMap<GroupKey, GroupAgg> =
        std::collections::BTreeMap::new();
    for r in rows {
        groups.entry(GroupKey::from_entry(r)).or_default().add(r);
    }
    let groups: Vec<(&GroupKey, &GroupAgg)> = groups.iter().collect();

    // A column is constant iff its value SET across every group has exactly one
    // member — then it is dropped from the rows and stated once.
    let const_kind = single_across(groups.iter().map(|(_, g)| &g.kinds));
    let const_scope = single_across(groups.iter().map(|(_, g)| &g.scopes));
    let const_family = single_across(groups.iter().map(|(_, g)| &g.families));
    let const_dir = single_value(groups.iter().map(|(k, _)| k.direction.as_str()));

    let mut out = String::new();

    let mut context = Vec::new();
    if let Some(k) = &const_kind {
        context.push(format!("kind={}", k));
    }
    if let Some(d) = const_dir {
        context.push(format!("direction={}", d));
    }
    if let Some(s) = &const_scope {
        context.push(format!("scope={}", s));
    }
    if let Some(f) = &const_family {
        context.push(format!("protocol={}", f));
    }
    if !context.is_empty() {
        out.push_str(&format!("\nAll boundaries: {}\n", context.join(", ")));
    }
    out.push_str(&format!(
        "\n{} file×direction group{} (detail: `rmap surfaces list`):\n",
        groups.len(),
        if groups.len() == 1 { "" } else { "s" }
    ));

    // Deterministic order: (kind, direction, file) so a kind-sorted read is
    // stable even though the grouping key is (file, direction).
    let mut ordered = groups;
    ordered.sort_by(|(ka, ga), (kb, gb)| {
        set_repr(&ga.kinds)
            .cmp(&set_repr(&gb.kinds))
            .then_with(|| ka.direction.cmp(&kb.direction))
            .then_with(|| ka.file.cmp(&kb.file))
    });

    for (key, agg) in &ordered {
        let mut cols = Vec::new();
        if const_kind.is_none() {
            cols.push(join_set(&agg.kinds));
        }
        if const_dir.is_none() {
            cols.push(key.direction.clone());
        }
        if const_scope.is_none() {
            cols.push(join_set(&agg.scopes));
        }
        if const_family.is_none() {
            cols.push(join_set(&agg.families));
        }
        cols.push(key.file.clone());
        out.push_str(&format!("  {}  ×{}", cols.join("  "), agg.n));
        // §2.4: the methods/routes summary (from `surface_display_name`), the
        // signal that lived only in `surfaces list` before.
        let routes = summarize_routes(&agg.routes);
        if !routes.is_empty() {
            out.push_str(&format!("  {}", routes));
        }
        out.push('\n');
    }

    out
}

/// Per-group accumulator: the `×N` count plus the value SETS of the columns that
/// are no longer part of the grouping key.
#[derive(Debug, Default)]
struct GroupAgg {
    n: usize,
    kinds: std::collections::BTreeSet<String>,
    scopes: std::collections::BTreeSet<String>,
    families: std::collections::BTreeSet<String>,
    /// The methods/routes (`surface_display_name`) seen in this group.
    routes: std::collections::BTreeSet<String>,
}

impl GroupAgg {
    fn add(&mut self, e: &BoundaryListEntry) {
        self.n += 1;
        self.kinds.insert(e.channel_kind.clone());
        self.scopes.insert(e.boundary_scope.clone());
        self.families
            .insert(e.protocol_family.clone().unwrap_or_else(|| "-".to_string()));
        if let Some(name) = &e.surface_display_name {
            if !name.trim().is_empty() {
                self.routes.insert(name.clone());
            }
        }
    }
}

/// Join a small value set for a per-row column (already sorted by `BTreeSet`).
fn join_set(set: &std::collections::BTreeSet<String>) -> String {
    set.iter().cloned().collect::<Vec<_>>().join("/")
}

/// A stable representative (the min) of a set, for ordering groups by kind.
fn set_repr(set: &std::collections::BTreeSet<String>) -> String {
    set.iter().next().cloned().unwrap_or_default()
}

/// Summarize the methods/routes in a group: up to `MAX_ROUTES_PER_GROUP`, then
/// `+K more` — a summary, not the full per-route list (that is `surfaces list`).
fn summarize_routes(routes: &std::collections::BTreeSet<String>) -> String {
    if routes.is_empty() {
        return String::new();
    }
    let shown: Vec<&String> = routes.iter().take(MAX_ROUTES_PER_GROUP).collect();
    let mut s = shown
        .iter()
        .map(|r| r.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if routes.len() > MAX_ROUTES_PER_GROUP {
        s.push_str(&format!(", +{} more", routes.len() - MAX_ROUTES_PER_GROUP));
    }
    s
}

/// `Some(v)` iff every group's value SET is the same single value; `None`
/// otherwise. Drives constant-column detection over per-group sets (§2.4).
fn single_across<'a>(
    mut sets: impl Iterator<Item = &'a std::collections::BTreeSet<String>>,
) -> Option<String> {
    let first = sets.next()?;
    if first.len() != 1 {
        return None;
    }
    let val = first.iter().next().cloned();
    if sets.all(|s| s.len() == 1 && s.iter().next() == val.as_ref()) {
        val
    } else {
        None
    }
}

/// `Some(v)` iff every element equals the same `v`; `None` if the iterator is
/// empty or holds ≥2 distinct values. Drives constant-direction detection (§2.4).
fn single_value<'a>(mut it: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let first = it.next()?;
    if it.all(|v| v == first) {
        Some(first)
    } else {
        None
    }
}

/// The grouping key for the §2.4 rollup: literally **file × direction**. Ordered
/// so `BTreeMap` iteration is deterministic.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct GroupKey {
    direction: String,
    file: String,
}

impl GroupKey {
    fn from_entry(e: &BoundaryListEntry) -> Self {
        GroupKey {
            direction: e.direction.clone(),
            file: e
                .file_path
                .clone()
                .or_else(|| e.service_name.clone())
                .unwrap_or_else(|| e.boundary_channel_uid.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_list_response() -> BoundariesListResponse {
        BoundariesListResponse {
            command: "boundaries list".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            results: vec![
                BoundaryListEntry {
                    boundary_channel_uid: "bc-1".to_string(),
                    channel_kind: "http_client".to_string(),
                    boundary_scope: "external".to_string(),
                    direction: "outbound".to_string(),
                    protocol_family: Some("REST".to_string()),
                    service_name: Some("UserService".to_string()),
                    file_path: Some("src/api/client.ts".to_string()),
                    symbol_key: None,
                    confidence: 0.9,
                    basis: Some("pattern".to_string()),
                    surface_uid: Some("surf-1".to_string()),
                    surface_display_name: Some("api".to_string()),
                },
                BoundaryListEntry {
                    boundary_channel_uid: "bc-2".to_string(),
                    channel_kind: "database".to_string(),
                    boundary_scope: "internal".to_string(),
                    direction: "bidirectional".to_string(),
                    protocol_family: Some("SQL".to_string()),
                    service_name: None,
                    file_path: Some("src/db/pool.ts".to_string()),
                    symbol_key: Some("DbPool.query".to_string()),
                    confidence: 0.8,
                    basis: Some("import".to_string()),
                    surface_uid: None,
                    surface_display_name: None,
                },
            ],
            count: 2,
            filter_kind: None,
            filter_scope: None,
            filter_direction: None,
            filter_family: None,
            filter_file: None,
            filter_file_prefix: None,
            filter_symbol: None,
        }
    }

    fn sample_empty_response() -> BoundariesListResponse {
        BoundariesListResponse {
            command: "boundaries list".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            results: vec![],
            count: 0,
            filter_kind: None,
            filter_scope: None,
            filter_direction: None,
            filter_family: None,
            filter_file: None,
            filter_file_prefix: None,
            filter_symbol: None,
        }
    }

    #[test]
    fn list_render_shows_header() {
        let resp = sample_list_response();
        let output = resp.render_human();
        assert!(output.contains("Boundaries"));
    }

    #[test]
    fn list_render_shows_count() {
        let resp = sample_list_response();
        let output = resp.render_human();
        assert!(output.contains("2 boundaries"));
    }

    #[test]
    fn list_render_shows_boundaries() {
        // §2.4: the grouped view is keyed on file × direction (per-service /
        // contract detail lives in `boundaries show`), so it shows the channel
        // kinds and the FILES, one `×N` row per file×direction group.
        let resp = sample_list_response();
        let output = resp.render_human();
        assert!(output.contains("http_client"));
        assert!(output.contains("database"));
        assert!(output.contains("src/api/client.ts"), "{output}");
        assert!(output.contains("src/db/pool.ts"), "{output}");
        assert!(
            output.contains('×'),
            "grouped rows carry a ×N count:\n{output}"
        );
    }

    #[test]
    fn list_render_shows_direction() {
        let resp = sample_list_response();
        let output = resp.render_human();
        assert!(output.contains("outbound"));
        assert!(output.contains("bidirectional"));
    }

    #[test]
    fn list_render_groups_duplicate_rows_with_count() {
        // The audit defect: N verbatim-duplicate rows. The grouped view collapses
        // them to ONE row with `×N`, and lifts the constant columns out.
        let mut resp = sample_empty_response();
        let dup = |uid: &str| BoundaryListEntry {
            boundary_channel_uid: uid.to_string(),
            channel_kind: "http".to_string(),
            boundary_scope: "unknown".to_string(),
            direction: "provider".to_string(),
            protocol_family: Some("http".to_string()),
            service_name: None,
            file_path: Some("src/app/api/x/route.ts".to_string()),
            symbol_key: None,
            confidence: 0.9,
            basis: None,
            surface_uid: None,
            surface_display_name: None,
        };
        resp.results = vec![dup("a"), dup("b"), dup("c")];
        resp.count = 3;
        let output = resp.render_human();
        // Constant columns stated once, not per row.
        assert!(output.contains("kind=http"), "{output}");
        assert!(output.contains("scope=unknown"), "{output}");
        // The three duplicates collapse to a single ×3 row.
        assert!(output.contains("×3"), "{output}");
        assert_eq!(
            output.matches("src/app/api/x/route.ts").count(),
            1,
            "{output}"
        );
    }

    #[test]
    fn list_render_summarizes_methods_routes_per_file_direction() {
        // §2.4: the grouped view keys on (file, direction) and summarizes the
        // methods/routes (surface_display_name) — the signal that lived only in
        // `surfaces list`. Two routes in one provider file collapse to ONE group
        // with both routes summarized.
        let mut resp = sample_empty_response();
        let route = |uid: &str, disp: &str| BoundaryListEntry {
            boundary_channel_uid: uid.to_string(),
            channel_kind: "http".to_string(),
            boundary_scope: "unknown".to_string(),
            direction: "provider".to_string(),
            protocol_family: Some("http".to_string()),
            service_name: None,
            file_path: Some("src/app/api/x/route.ts".to_string()),
            symbol_key: None,
            confidence: 0.9,
            basis: None,
            surface_uid: None,
            surface_display_name: Some(disp.to_string()),
        };
        resp.results = vec![route("a", "GET /api/x"), route("b", "POST /api/x")];
        resp.count = 2;
        let output = resp.render_human();
        // One file×direction group, ×2, with BOTH methods/routes summarized.
        assert!(output.contains("×2"), "{output}");
        assert!(output.contains("GET /api/x"), "{output}");
        assert!(output.contains("POST /api/x"), "{output}");
        // The file appears once (grouped, not two verbatim rows).
        assert_eq!(
            output.matches("src/app/api/x/route.ts").count(),
            1,
            "{output}"
        );
    }

    #[test]
    fn list_render_shows_scope() {
        let resp = sample_list_response();
        let output = resp.render_human();
        assert!(output.contains("external"));
        assert!(output.contains("internal"));
    }

    #[test]
    fn list_render_empty_shows_hint() {
        let resp = sample_empty_response();
        let output = resp.render_human();
        assert!(output.contains("hint:"));
        assert!(output.contains("boundaries are interactions"));
    }

    #[test]
    fn list_render_is_deterministic() {
        let resp = sample_list_response();
        let output = resp.render_human();
        // database comes before http_client alphabetically by channel_kind
        let db_pos = output.find("database").unwrap();
        let http_pos = output.find("http_client").unwrap();
        assert!(
            db_pos < http_pos,
            "Boundaries should be sorted by (kind, direction, ...)"
        );
    }

    #[test]
    fn list_render_shows_filter() {
        let mut resp = sample_empty_response();
        resp.filter_kind = Some("http_client".to_string());
        let output = resp.render_human();
        assert!(output.contains("Filtered by: kind=http_client"));
    }
}
