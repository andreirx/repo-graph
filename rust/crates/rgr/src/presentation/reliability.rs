//! Human rendering for `rmap reliability` (RESOLUTION-BREAKDOWN-CLI-1).
//!
//! The daemon does ALL the projection: each [`ResolutionScopeRow`] already carries
//! the reader-frame `phrase`, `external_line`, `caveat`, and `band` produced by the
//! shared `repo_graph_agent::reliability_breakdown` (which reuses
//! `CallReliabilityView`), and the response carries the shared enrichment summary
//! (check's exact wording). This module is therefore pure FORMATTING — it holds no
//! rate/threshold/wording of its own, so the human and `--json` surfaces can never
//! disagree.
//!
//! Every row renders its full honest basis (review-0 F3): resolved / unresolved / %
//! resolved, plus the external and unclassified counts — INCLUDING rows that are
//! UNKNOWN because all their calls are external (their counts are NOT dropped). A
//! scope whose unclassified share is material carries its own conservative-rate
//! caveat as a sub-line. Calls are split into production and test partitions
//! (review-0 F4) so an agent can separate test-file resolution.

use repo_graph_agent::reliability_breakdown::ResolutionScopeRow;
use serde::Deserialize;

use crate::presentation::{bullet, heading, kv_line, next_steps, sub_heading};

/// Which breakdown axes the human view shows. `--by-language` / `--by-module`
/// select one; the default shows both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisFilter {
    pub language: bool,
    pub module: bool,
}

impl AxisFilter {
    /// Neither flag given → show both axes.
    pub fn from_flags(by_language: bool, by_module: bool) -> Self {
        if !by_language && !by_module {
            Self {
                language: true,
                module: true,
            }
        } else {
            Self {
                language: by_language,
                module: by_module,
            }
        }
    }
}

/// The `reliability` response as the human renderer reads it. Mirrors the daemon
/// handler's object: the shared breakdown DTO (`total`/`by_language`/`by_module`,
/// deserialized straight into the agent types — no drift), identity, and the shared
/// enrichment state (`enrichment_summary` = check's exact wording; F1). Enrichment
/// fields default so an older payload without them still renders.
#[derive(Debug, Clone, Deserialize)]
pub struct ReliabilityResponse {
    pub repo: String,
    pub snapshot: String,
    pub total: ResolutionScopeRow,
    pub by_language: Vec<ResolutionScopeRow>,
    pub by_module: Vec<ResolutionScopeRow>,
    #[serde(default)]
    pub enrichment_state: Option<String>,
    #[serde(default)]
    pub enrichment_summary: Option<String>,
}

impl ReliabilityResponse {
    pub fn from_json(value: serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(value)
    }

    pub fn render_human(&self, axis: AxisFilter) -> String {
        let mut out = String::new();
        out.push_str(&kv_line(
            "Call resolution breakdown",
            &format!("{} @ {}", self.repo, self.snapshot),
        ));
        out.push('\n');

        // ── Overall — the aggregate the parts reconcile to ──────────────
        out.push_str(&heading("Overall"));
        out.push_str(&bullet(&overall_line(&self.total)));
        if let Some(line) = &self.total.external_line {
            out.push_str(&bullet(line));
        }
        if let Some(caveat) = &self.total.caveat {
            out.push_str(&bullet(caveat));
        }
        // Shared enrichment state (F1) — check's exact wording, so the reader sees
        // WHY the numbers are what they are (e.g. pre-enrichment).
        if let Some(summary) = &self.enrichment_summary {
            out.push_str(&bullet(&format!("Enrichment: {summary}")));
        }
        out.push('\n');

        if axis.language {
            out.push_str(&render_section("By language", &self.by_language));
        }
        if axis.module {
            out.push_str(&render_section("By module", &self.by_module));
        }

        // Point at the full posture without re-deriving it here.
        out.push_str(&next_steps(&[
            "rmap trust  # full reliability posture + enrichment detail",
        ]));
        out
    }
}

/// The overall headline: the shared full phrase (a standalone sentence) + band +
/// the full basis.
fn overall_line(row: &ResolutionScopeRow) -> String {
    match row.resolved_pct {
        None => format!("{} — {}", row.phrase, row_basis(row)),
        Some(_) => format!("{}{} — {}", row.phrase, band_suffix(row), row_basis(row)),
    }
}

/// Render one breakdown section, split into production and test partitions
/// (review-0 F4), each ordered most-unresolved-first (risk concentration).
fn render_section(title: &str, rows: &[ResolutionScopeRow]) -> String {
    let mut out = heading(title);
    if rows.is_empty() {
        out.push_str(&bullet("no calls in this snapshot"));
        out.push('\n');
        return out;
    }
    // is_test == Some(true) → test; everything else (Some(false)/None) → production.
    let production: Vec<&ResolutionScopeRow> =
        rows.iter().filter(|r| r.is_test != Some(true)).collect();
    let test: Vec<&ResolutionScopeRow> = rows.iter().filter(|r| r.is_test == Some(true)).collect();

    render_rows(&mut out, &production);
    if !test.is_empty() {
        out.push_str(&sub_heading("Test files"));
        render_rows(&mut out, &test);
    }
    out.push('\n');
    out
}

/// Render a set of rows most-unresolved-first (tie-broken by key), each with its own
/// basis and — when present — its own conservative-rate caveat as a sub-line (F3).
fn render_rows(out: &mut String, rows: &[&ResolutionScopeRow]) {
    let mut ordered: Vec<&ResolutionScopeRow> = rows.to_vec();
    ordered.sort_by(|a, b| {
        b.unresolved
            .cmp(&a.unresolved)
            .then_with(|| a.key.cmp(&b.key))
    });
    for row in ordered {
        out.push_str(&bullet(&scope_line(row)));
        if let Some(caveat) = &row.caveat {
            out.push_str(&caveat_subline(caveat));
        }
    }
}

/// One per-scope line: `key: M% resolved (BAND) — R resolved / U unresolved (basis)`,
/// or the shared UNKNOWN phrase (still with the R/U/external/unclassified basis — the
/// counts are never dropped, F3).
fn scope_line(row: &ResolutionScopeRow) -> String {
    match row.resolved_pct {
        None => format!("{}: {} — {}", row.key, row.phrase, row_basis(row)),
        Some(pct) => format!(
            "{}: {:.0}% resolved{} — {}",
            row.key,
            pct,
            band_suffix(row),
            row_basis(row)
        ),
    }
}

/// The honest count basis carried by EVERY row (F3): `R resolved / U unresolved`
/// plus a parenthesised detail — the in-scope denominator (only when a rate exists),
/// the external share, and the unclassified share — each shown only when it applies.
fn row_basis(row: &ResolutionScopeRow) -> String {
    let mut detail: Vec<String> = Vec::new();
    if row.resolved_pct.is_some() {
        detail.push(format!(
            "{} in-scope or unclassified",
            row.in_scope_or_unclassified_total
        ));
    }
    if row.external > 0 {
        detail.push(format!("{} external", row.external));
    }
    if row.unknown > 0 {
        detail.push(format!("{} unclassified", row.unknown));
    }
    let suffix = if detail.is_empty() {
        String::new()
    } else {
        format!(" ({})", detail.join("; "))
    };
    format!(
        "{} resolved / {} unresolved{}",
        row.resolved, row.unresolved, suffix
    )
}

/// A per-scope caveat sub-line, indented under its row (F3).
fn caveat_subline(caveat: &str) -> String {
    format!("      {caveat}\n")
}

/// ` (BAND)` when the row carries a band, else empty. The band rides the line only
/// when there is an in-scope rate (the daemon already suppressed it otherwise).
fn band_suffix(row: &ResolutionScopeRow) -> String {
    row.band
        .as_deref()
        .map(|b| format!(" ({b})"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    fn row(
        key: &str,
        is_test: Option<bool>,
        resolved: u64,
        unresolved: u64,
        external: u64,
        unknown: u64,
        in_scope: u64,
        pct: Option<f64>,
        band: Option<&str>,
        caveat: Option<&str>,
    ) -> ResolutionScopeRow {
        ResolutionScopeRow {
            key: key.to_string(),
            is_test,
            resolved,
            unresolved,
            external,
            unknown,
            total_calls: resolved + unresolved,
            in_scope_or_unclassified_total: in_scope,
            resolved_pct: pct,
            external_pct: None,
            band: band.map(|s| s.to_string()),
            phrase: match pct {
                None => "no in-scope calls measured".to_string(),
                Some(p) => format!("your code's calls {p:.0}% resolved"),
            },
            external_line: None,
            caveat: caveat.map(|s| s.to_string()),
        }
    }

    fn resp() -> ReliabilityResponse {
        ReliabilityResponse {
            repo: "glamCRM".into(),
            snapshot: "snap1".into(),
            total: row(
                "(total)",
                None,
                50,
                400,
                20,
                300,
                450,
                Some(11.1),
                Some("LOW"),
                None,
            ),
            by_language: vec![
                row(
                    "java",
                    Some(false),
                    12,
                    101,
                    5,
                    88,
                    113,
                    Some(10.6),
                    Some("LOW"),
                    None,
                ),
                row(
                    "jsx",
                    Some(false),
                    24,
                    75,
                    0,
                    0,
                    99,
                    Some(24.2),
                    Some("LOW"),
                    None,
                ),
                row(
                    "typescript",
                    Some(true),
                    4,
                    10,
                    0,
                    0,
                    14,
                    Some(28.6),
                    Some("LOW"),
                    None,
                ),
                row("go", Some(false), 0, 8, 8, 0, 0, None, None, None),
                row("(unknown)", Some(false), 0, 0, 0, 0, 0, None, None, None),
            ],
            by_module: vec![row(
                "src/api",
                Some(false),
                30,
                200,
                0,
                0,
                230,
                Some(13.0),
                Some("LOW"),
                None,
            )],
            enrichment_state: Some("ran".into()),
            enrichment_summary: Some("Enrichment phase executed.".into()),
        }
    }

    #[test]
    fn renders_both_axes_by_default() {
        let out = resp().render_human(AxisFilter::from_flags(false, false));
        assert!(out.contains("Overall"));
        assert!(out.contains("By language"));
        assert!(out.contains("By module"));
        assert!(out.contains("java: 11% resolved (LOW)"), "{out}");
        // F3: resolved AND unresolved both explicit.
        assert!(out.contains("12 resolved / 101 unresolved"), "{out}");
        // The denominator INCLUDES unclassified calls — the label must not claim
        // bare "in-scope" (false certainty); it is "in-scope or unclassified".
        assert!(out.contains("113 in-scope or unclassified"), "{out}");
    }

    #[test]
    fn enrichment_state_renders_with_shared_wording() {
        // review-0 F1: the shared enrichment summary (check's exact wording) shows.
        let out = resp().render_human(AxisFilter::from_flags(false, false));
        assert!(out.contains("Enrichment phase executed."), "{out}");
    }

    #[test]
    fn by_language_flag_hides_module_section() {
        let out = resp().render_human(AxisFilter::from_flags(true, false));
        assert!(out.contains("By language"));
        assert!(!out.contains("By module"), "{out}");
    }

    #[test]
    fn unknown_scope_keeps_counts_never_a_percent() {
        // review-0 F3: an all-external UNKNOWN scope shows the shared UNKNOWN text
        // AND keeps its external count — the counts are not discarded.
        let out = resp().render_human(AxisFilter::from_flags(true, false));
        assert!(
            out.contains("go: no in-scope calls measured — 0 resolved / 8 unresolved (8 external)"),
            "{out}"
        );
        assert!(!out.contains("go: 0%"));
        assert!(!out.contains("go: 100%"));
    }

    #[test]
    fn test_files_render_in_their_own_partition() {
        // review-0 F4: the typescript TEST row appears under a "Test files" heading,
        // not mixed into the production rows.
        let out = resp().render_human(AxisFilter::from_flags(true, false));
        let test_hdr = out.find("Test files").expect("test partition heading");
        let ts = out.find("typescript: 29% resolved").expect("test row");
        assert!(
            test_hdr < ts,
            "the test row renders under the Test files heading"
        );
        // java (production) precedes the Test files heading.
        assert!(out.find("java:").unwrap() < test_hdr, "{out}");
    }

    #[test]
    fn rows_ordered_most_unresolved_first() {
        // java (101 unresolved) must precede jsx (75) despite alphabetical order.
        let out = resp().render_human(AxisFilter::from_flags(true, false));
        let java = out.find("java:").unwrap();
        let jsx = out.find("jsx:").unwrap();
        assert!(java < jsx, "most-unresolved-first ordering");
    }

    #[test]
    fn per_scope_conservative_caveat_renders_as_a_subline() {
        // review-0 F3: a per-scope caveat renders (not only the JSON, not only Overall).
        let mut r = resp();
        r.by_language[0].caveat = Some("conservative: 88 of 113 calls are unclassified".into());
        let out = r.render_human(AxisFilter::from_flags(true, false));
        assert!(
            out.contains("conservative: 88 of 113 calls are unclassified"),
            "the per-scope caveat renders under its row: {out}"
        );
    }

    #[test]
    fn per_scope_row_carries_its_own_unclassified_and_external_counts() {
        let out = resp().render_human(AxisFilter::from_flags(true, false));
        assert!(out.contains("java: 11% resolved (LOW)"), "{out}");
        assert!(out.contains("5 external"), "{out}");
        assert!(out.contains("88 unclassified"), "{out}");
    }
}
