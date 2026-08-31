//! FIXTURE-POLLUTION-1 / §2.4 — the `boundaries list` GROUPING + test-only partition.
//!
//! Split out of the sibling [`super`] DTO/orchestration module so the (file × direction)
//! rollup, the production/test-only partition, and their renderers stay off the DTO file
//! and BOTH stay under the 500-line structural guardrail (the review-1 finding: the flat
//! file had grown to 860 lines).
//!
//! Abstraction record — module: `presentation::boundaries_list::group`; concrete current
//! user: [`super::BoundariesListResponse::render_human`]; axis: the grouped-view rollup +
//! the §2.2 production/test-only/unknown partition, kept off the DTO file to hold the
//! guardrail; rejected simpler alternative: leaving it inline in `boundaries_list.rs` (the
//! over-guardrail state review-1 flagged). It accesses the parent's private
//! [`super::BoundaryListEntry`]/[`super::RowComposition`] directly (descendant visibility),
//! so no wider `pub` leak is introduced.

use std::collections::{BTreeMap, BTreeSet};

use super::{BoundaryListEntry, RowComposition};

/// Max methods/routes listed per group before summarizing the tail as `+K more`.
const MAX_ROUTES_PER_GROUP: usize = 6;

/// A borrowed grouped row: its `(file, direction)` key and accumulated columns. Named so
/// the production/test-only partition (§2.2) reads cleanly and clippy's type-complexity
/// lint stays quiet.
type GroupRef<'a> = (&'a GroupKey, &'a GroupAgg);

/// The rendered grouped body plus the GROUP-based counts the headline needs. The headline
/// counts GROUPS (file × direction), not rows (review-1 finding #2a: the "4 real groups"
/// contract fails when multiple rows share one group and the headline counts rows).
pub(super) struct GroupedRender {
    /// Production + unknown groups — the headline number (never demoted).
    pub main_group_count: usize,
    /// Positively test-only groups — demoted below, excluded from the headline.
    pub test_only_group_count: usize,
    /// The rendered body: the main section, then the labeled demoted section.
    pub body: String,
}

/// §2.4 (operator ruling (b)): `boundaries list` is the GROUPED view of the same boundary
/// rows, keyed literally on **file × direction**, with `×N` counts, the methods/routes
/// summarized, and constant-valued columns (kind, scope, family) lifted out and stated
/// ONCE. Strictly a summary of `surfaces list` (it shares its HTTP-surface read for the
/// route detail). Channel-kind-AGNOSTIC — gRPC / DB / broker rows group the same way.
///
/// FIXTURE-POLLUTION-1 §2.2 + binding direction rule: the groups are partitioned into the
/// MAIN listing and the demoted test-only section. A group is DEMOTED iff it is positively
/// `TestOnly` (every row test-only). A group with any production row is `Production`; a
/// group with any UNKNOWN row (and no production) is `Unknown` — it stays in the main
/// listing carrying an explicit marker, NEVER demoted on unproven evidence.
pub(super) fn group_and_render(rows: &[BoundaryListEntry]) -> GroupedRender {
    // Accumulate one group per (file, direction); the other columns are collected as
    // per-group value SETS (a file×direction may legitimately span >1 kind).
    let mut groups: BTreeMap<GroupKey, GroupAgg> = BTreeMap::new();
    for r in rows {
        groups.entry(GroupKey::from_entry(r)).or_default().add(r);
    }

    let (test_only, main): (Vec<GroupRef>, Vec<GroupRef>) = groups
        .iter()
        .partition(|(_, g)| g.composition() == GroupComposition::TestOnly);

    let mut body = String::new();
    body.push_str(&render_group_section(
        &main,
        "\nAll boundaries: ",
        "file×direction group",
        "(detail: `rmap surfaces list`)",
    ));

    if !test_only.is_empty() {
        body.push_str(&format!(
            "\ntest-only surfaces ({} group{} — excluded from the headline counts):\n",
            test_only.len(),
            if test_only.len() == 1 { "" } else { "s" }
        ));
        // The demoted rows render with FULL columns (no constant-lifting) so the section
        // is self-describing when read in isolation.
        body.push_str(&render_demoted_rows(&test_only));
    }

    GroupedRender {
        main_group_count: main.len(),
        test_only_group_count: test_only.len(),
        body,
    }
}

/// Render one production headline section: the constant-column context line, the
/// `N …group(s)` header, and the `×N` rows (with methods/routes summary). Extracted so
/// the headline stays a single coherent unit after the fixture split (§2.2).
fn render_group_section(
    groups: &[(&GroupKey, &GroupAgg)],
    context_prefix: &str,
    group_noun: &str,
    header_suffix: &str,
) -> String {
    // A column is constant iff its value SET across every group has exactly one member —
    // then it is dropped from the rows and stated once.
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
        out.push_str(&format!("{}{}\n", context_prefix, context.join(", ")));
    }
    out.push_str(&format!(
        "\n{} {}{} {}:\n",
        groups.len(),
        group_noun,
        if groups.len() == 1 { "" } else { "s" },
        header_suffix
    ));

    // Deterministic order: (kind, direction, file) so a kind-sorted read is stable even
    // though the grouping key is (file, direction).
    let mut ordered = groups.to_vec();
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
        // §2.4: the methods/routes summary (from `surface_display_name`), the signal that
        // lived only in `surfaces list` before.
        let routes = summarize_routes(&agg.routes);
        if !routes.is_empty() {
            out.push_str(&format!("  {}", routes));
        }
        // Binding direction rule: a group we could NOT prove test-only or production stays
        // in the main listing carrying an explicit unknown marker (never a silent
        // production placement).
        if let GroupComposition::Unknown(reason) = agg.composition() {
            out.push_str(&format!("  [test-composition unknown: {reason}]"));
        }
        out.push('\n');
    }

    out
}

/// Render the demoted test-only groups with full (never constant-lifted) columns and the
/// same deterministic `(kind, direction, file)` order the headline uses.
fn render_demoted_rows(groups: &[(&GroupKey, &GroupAgg)]) -> String {
    let mut ordered = groups.to_vec();
    ordered.sort_by(|(ka, ga), (kb, gb)| {
        set_repr(&ga.kinds)
            .cmp(&set_repr(&gb.kinds))
            .then_with(|| ka.direction.cmp(&kb.direction))
            .then_with(|| ka.file.cmp(&kb.file))
    });
    let mut out = String::new();
    for (key, agg) in &ordered {
        let cols = [
            join_set(&agg.kinds),
            key.direction.clone(),
            join_set(&agg.scopes),
            join_set(&agg.families),
            key.file.clone(),
        ];
        out.push_str(&format!("  {}  ×{}", cols.join("  "), agg.n));
        let routes = summarize_routes(&agg.routes);
        if !routes.is_empty() {
            out.push_str(&format!("  {}", routes));
        }
        out.push('\n');
    }
    out
}

/// A group's aggregated test-composition (see [`GroupAgg::composition`]).
#[derive(Debug, Clone, PartialEq, Eq)]
enum GroupComposition {
    TestOnly,
    Production,
    Unknown(String),
}

/// Per-group accumulator: the `×N` count plus the value SETS of the columns that are no
/// longer part of the grouping key.
#[derive(Debug, Default)]
struct GroupAgg {
    n: usize,
    kinds: BTreeSet<String>,
    scopes: BTreeSet<String>,
    families: BTreeSet<String>,
    /// The methods/routes (`surface_display_name`) seen in this group.
    routes: BTreeSet<String>,
    /// Rows in this group positively classified test-only (§2.2).
    test_only_n: usize,
    /// Rows positively classified production.
    production_n: usize,
    /// Rows with no reachable `is_test` evidence (binding direction rule — never demoted).
    unknown_n: usize,
    /// A representative reason for the unknown rows (first seen), for the main-listing marker.
    unknown_reason: Option<String>,
}

impl GroupAgg {
    fn add(&mut self, e: &BoundaryListEntry) {
        self.n += 1;
        match e.composition() {
            RowComposition::TestOnly => self.test_only_n += 1,
            RowComposition::Production => self.production_n += 1,
            RowComposition::Unknown(reason) => {
                self.unknown_n += 1;
                self.unknown_reason.get_or_insert(reason);
            }
        }
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

    /// Conservative aggregation (§2.1 + binding direction rule): ANY production row ⇒
    /// `Production`; else ANY unknown row ⇒ `Unknown` (main listing + marker, never demoted
    /// on unproven evidence); else (all rows test-only) ⇒ `TestOnly` (demoted).
    fn composition(&self) -> GroupComposition {
        if self.n == 0 || self.production_n > 0 {
            GroupComposition::Production
        } else if self.unknown_n > 0 {
            let reason = match &self.unknown_reason {
                Some(r) => r.clone(),
                None => "no stored is_test fact".to_string(),
            };
            GroupComposition::Unknown(reason)
        } else {
            GroupComposition::TestOnly
        }
    }
}

/// Join a small value set for a per-row column (already sorted by `BTreeSet`).
fn join_set(set: &BTreeSet<String>) -> String {
    set.iter().cloned().collect::<Vec<_>>().join("/")
}

/// A stable representative (the min) of a set, for ordering groups by kind.
fn set_repr(set: &BTreeSet<String>) -> String {
    set.iter().next().cloned().unwrap_or_default()
}

/// Summarize the methods/routes in a group: up to `MAX_ROUTES_PER_GROUP`, then `+K more` —
/// a summary, not the full per-route list (that is `surfaces list`).
fn summarize_routes(routes: &BTreeSet<String>) -> String {
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

/// `Some(v)` iff every group's value SET is the same single value; `None` otherwise. Drives
/// constant-column detection over per-group sets (§2.4).
fn single_across<'a>(mut sets: impl Iterator<Item = &'a BTreeSet<String>>) -> Option<String> {
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

/// `Some(v)` iff every element equals the same `v`; `None` if the iterator is empty or holds
/// ≥2 distinct values. Drives constant-direction detection (§2.4).
fn single_value<'a>(mut it: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let first = it.next()?;
    if it.all(|v| v == first) {
        Some(first)
    } else {
        None
    }
}

/// The grouping key for the §2.4 rollup: literally **file × direction**. Ordered so
/// `BTreeMap` iteration is deterministic.
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
