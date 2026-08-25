//! Dense-headline + depth section renderers for the `orient` command (ORIENT-DENSITY-1).
//!
//! Split out of `orient.rs` to keep each module under the 500-line structural
//! guardrail — the `explain.rs` / `explain_sections.rs` idiom. This is a SECOND
//! `impl OrientResponse` block (inherent impls may span modules within the crate):
//! `orient.rs` keeps the structs, the `OrientDepth` map, the
//! `render_orient_envelope` wrapper, and the `render_human` orchestrator; the
//! per-section renderers it calls live here.
//!
//! Visibility: methods `render_human` invokes are `pub(super)`; their internal
//! helpers stay private. Pure relocation — no behavior changed.

use repo_graph_agent::reliability::{self, CallReliabilityView, ExternalTarget};

use super::orient::{OrientDepth, OrientResponse, ReliabilityAxis, Signal};
use super::{bullet, heading, sub_heading, DisplaySeverity};

/// Headline signal codes — the load-bearing facts the dense headline
/// synthesizes (the presentation mirror of the agent's
/// `HEADLINE_SIGNAL_CODES`). Kept here as a string slice so the
/// detail-section renderer can EXCLUDE them (they already surfaced in
/// the headline) when listing the remaining signals at `--full`.
const HEADLINE_CODES: &[&str] = &[
    "MODULE_SUMMARY",
    "HIGH_COMPLEXITY",
    "IMPORT_CYCLES",
    "GATE_FAIL",
    "GATE_INCOMPLETE",
    "BOUNDARY_VIOLATIONS",
];

/// Plural suffix helper (`""` for 1, `"s"` otherwise).
fn plural(n: u64) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Short label for a module: its last path segment (`src/http` → `http`).
/// Collisions are fine for the dense headline (fuzzy-by-design, VISION); full
/// paths appear in the breakdown.
fn module_short_name(path: &str) -> &str {
    path.rsplit('/').find(|s| !s.is_empty()).unwrap_or(path)
}

/// The labelled declared/inferred-module count phrase from a MODULE_SUMMARY
/// payload — e.g. `1 declared module`, `5 inferred modules`, `3 modules`. The
/// `module_candidates` notion (Layer 1/2), kept DISTINCT from the package
/// topology (MODULE-MODEL-1). The kind word applies only when the WHOLE set is
/// one kind (else the bare `module(s)`). `None` when no module-discovery data.
fn declared_module_phrase(ev: &serde_json::Value) -> Option<String> {
    let count = ev.get("discovered_module_count").and_then(|v| v.as_u64())?;
    let kind_count = |k: &str| -> u64 {
        ev.get("module_kinds")
            .and_then(|m| m.get(k))
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
    };
    let label = if count > 0 && kind_count("declared") == count {
        "declared module"
    } else if count > 0 && kind_count("inferred") == count {
        "inferred module"
    } else if count > 0 && kind_count("operational") == count {
        "operational module"
    } else {
        "module"
    };
    Some(format!("{} {}{}", count, label, plural(count)))
}

impl OrientResponse {
    /// The MODULE_SUMMARY signal's evidence payload, if present.
    fn module_summary_evidence(&self) -> Option<&serde_json::Value> {
        self.signal_evidence("MODULE_SUMMARY")
    }

    /// The evidence payload of the first signal with `code`, if present.
    fn signal_evidence(&self, code: &str) -> Option<&serde_json::Value> {
        self.signals
            .iter()
            .map(|leaf| &leaf.value)
            .find(|s| s.code == code)
            .and_then(|s| s.evidence.as_ref())
    }

    /// High-severity alert lines (gate failure / boundary violations) —
    /// load-bearing governance facts surfaced FIRST, rendered verbatim from each
    /// signal's summary. Empty for a clean repo.
    pub(super) fn headline_alerts(&self) -> Vec<String> {
        let mut out = Vec::new();
        for leaf in &self.signals {
            let s = &leaf.value;
            if matches!(
                s.code.as_str(),
                "GATE_FAIL" | "GATE_INCOMPLETE" | "BOUNDARY_VIOLATIONS"
            ) {
                out.push(format!("Alert: {}", s.summary));
            }
        }
        out
    }

    /// The dense STRUCTURE line (MODULE-MODEL-1 D2(i)): `<repo> · <N> files[,
    /// <M> symbols] · <P> package groups: a, b, c · <D> declared modules`.
    /// LEADS with the directory/package TOPOLOGY (Layer 0/1), NAMED; package
    /// names are capped by depth, `package_groups.len()` drives the `+N more`
    /// tail. The declared/inferred `module_candidates` count rides as a SEPARATE,
    /// self-labelled fact — never collapsed into the topology (MODULE-MODEL-1).
    pub(super) fn structure_line(&self, depth: OrientDepth) -> String {
        let repo = self.display_name.as_deref().unwrap_or(&self.repo);
        let mut line = repo.to_string();

        let Some(ev) = self.module_summary_evidence() else {
            return line;
        };

        if let Some(files) = ev.get("file_count").and_then(|v| v.as_u64()) {
            line.push_str(&format!(" · {} file{}", files, plural(files)));
            if let Some(symbols) = ev.get("symbol_count").and_then(|v| v.as_u64()) {
                line.push_str(&format!(", {} symbol{}", symbols, plural(symbols)));
            }
        }

        // STRUCTURE (Layer 0/1 topology): NAME the package groups. The headline is
        // a one-liner — BOUNDED at every tier (§13 D7). `total` counts ALL groups
        // (never only the displayed ones), so the count stays TRUE at scale. The
        // "+N more" pointer shows only at `small` (the sole topology surface
        // there); at `medium`+ the dedicated package-group section carries the
        // honest omission line, so the headline stays clean — no double pointer
        // (mirrors `complexity_line`).
        if let Some(groups) = ev.get("package_groups").and_then(|v| v.as_array()) {
            let total = groups.len() as u64;
            if total > 0 {
                let names: Vec<&str> = groups
                    .iter()
                    .take(depth.package_group_name_cap())
                    .filter_map(|g| g.get("name").and_then(|n| n.as_str()))
                    .collect();
                line.push_str(&format!(" · {} package group{}", total, plural(total)));
                if !names.is_empty() {
                    line.push_str(&format!(": {}", names.join(", ")));
                    let shown = names.len() as u64;
                    if total > shown && !depth.shows_detail() {
                        line.push_str(&format!(", +{} more", total - shown));
                    }
                }
            }
        }

        // DECLARED/INFERRED MODULE notion (Layer 1/2): a SEPARATE, self-labelled
        // count — never collapsed into the topology above.
        if let Some(phrase) = declared_module_phrase(ev) {
            line.push_str(&format!(" · {}", phrase));
        }
        line
    }

    /// The dense COMPLEXITY-CENTERS line: NAMED files (deduped, highest
    /// complexity per file), capped by depth, with an honest "+N more"
    /// pointer to `rmap hotspots`. `None` when no symbol exceeds the
    /// threshold (or measurements are unavailable).
    pub(super) fn complexity_line(&self, depth: OrientDepth) -> Option<String> {
        let ev = self.signal_evidence("HIGH_COMPLEXITY")?;
        let top = ev.get("top_complex").and_then(|v| v.as_array())?;
        let cap = depth.complexity_center_cap();

        let mut shown: Vec<String> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for entry in top {
            if shown.len() >= cap {
                break;
            }
            let cx = entry
                .get("complexity")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let label = match entry.get("file").and_then(|v| v.as_str()) {
                Some(f) => f.to_string(),
                None => match entry.get("symbol").and_then(|v| v.as_str()) {
                    Some(sym) => sym.to_string(),
                    None => continue,
                },
            };
            if seen.contains(&label) {
                continue;
            }
            seen.push(label.clone());
            shown.push(format!("{} (cx {})", label, cx));
        }
        if shown.is_empty() {
            return None;
        }

        let mut line = format!("Complexity centers: {}", shown.join(", "));
        // The headline "+N more" pointer is honest ONLY at `small` (the sole
        // complexity surface there); at `medium`+ the dedicated section carries
        // the tail, so the headline stays clean (no double "+N more").
        if !depth.shows_detail() {
            if let Some(total) = ev.get("high_complexity_count").and_then(|v| v.as_u64()) {
                let shown_n = shown.len() as u64;
                if total > shown_n {
                    line.push_str(&format!(
                        " (+{} more above threshold — rmap hotspots)",
                        total - shown_n
                    ));
                }
            }
        }
        Some(line)
    }

    /// The complexity centers, NAMED with cyclomatic complexity (ORIENT-DENSITY-1).
    /// `cap` is the per-tier render limit (`OrientDepth::complexity_breakdown_cap`):
    /// `Some(n)` a top-`n` slice (`medium` / `large`), `None` every carried center
    /// (`--full`). When more remain above threshold than shown, an honest
    /// "+N more — rmap hotspots" tail follows, keyed on `high_complexity_count` to
    /// match the headline. Empty when no complexity signal is present.
    pub(super) fn complexity_breakdown_section(&self, cap: Option<usize>) -> String {
        let Some(ev) = self.signal_evidence("HIGH_COMPLEXITY") else {
            return String::new();
        };
        let Some(top) = ev.get("top_complex").and_then(|v| v.as_array()) else {
            return String::new();
        };
        if top.is_empty() {
            return String::new();
        }

        let limit = cap.unwrap_or(top.len());
        let mut out = heading("Complexity centers (by cyclomatic complexity)");
        let mut shown: u64 = 0;
        for entry in top.iter().take(limit) {
            let cx = entry
                .get("complexity")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let file = entry.get("file").and_then(|v| v.as_str());
            let symbol = entry.get("symbol").and_then(|v| v.as_str());
            // The detailed view names BOTH the file (orientation target) and the
            // symbol when present — richer than the headline, which names only the
            // file/symbol label.
            let line = match (file, symbol) {
                (Some(f), Some(s)) => format!("{f} — {s} (cx {cx})"),
                (Some(f), None) => format!("{f} (cx {cx})"),
                (None, Some(s)) => format!("{s} (cx {cx})"),
                (None, None) => continue,
            };
            out.push_str(&bullet(&line));
            shown += 1;
        }
        // Honest tail: centers still above threshold beyond what this section
        // shows, keyed on `high_complexity_count` (absent count → no tail).
        if let Some(total) = ev.get("high_complexity_count").and_then(|v| v.as_u64()) {
            if total > shown {
                out.push_str(&bullet(&format!(
                    "+{} more above threshold — rmap hotspots",
                    total - shown
                )));
            }
        }
        out
    }

    /// METRIC-LANG-COVERAGE-1 (part A): the per-language measurement-coverage caveat
    /// shown beside the complexity centers. The reader-frame sentence is built in
    /// `classification` (e.g. "Complexity is measured for C and TypeScript only on this
    /// snapshot — Rust (72% of functions) is not yet measured; rankings omit it."), so
    /// the wording is identical across orient / hotspots / metrics. `None` when coverage
    /// is complete or absent — the caveat disappears by itself once every significant
    /// language is measured (the data-driven, no-hardcoded-list contract). When coverage
    /// could not be read at all, the block's `caveat_line` states that instead of `None`,
    /// so the surface never silently reads as fully measured.
    pub(super) fn measurement_coverage_caveat_line(&self) -> Option<String> {
        self.measurement_coverage.as_ref()?.caveat_line()
    }

    /// The combined CYCLES + DOCS line. `None` when neither is present.
    pub(super) fn cycles_docs_line(&self) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();

        if let Some(ev) = self.signal_evidence("IMPORT_CYCLES") {
            if let Some(cycles) = ev.get("cycles").and_then(|v| v.as_array()) {
                let count = ev
                    .get("cycle_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(cycles.len() as u64);
                if count > 0 {
                    let anchor = cycles
                        .first()
                        .and_then(|c| c.get("modules").and_then(|m| m.as_array()))
                        .map(|mods| self.format_cycle_anchor(mods, 0));
                    match anchor {
                        Some(a) => {
                            parts.push(format!("{} import cycle{} ({})", count, plural(count), a))
                        }
                        None => parts.push(format!("{} import cycle{}", count, plural(count))),
                    }
                }
            }
        }

        let docs = self.doc_basenames();
        if !docs.is_empty() {
            parts.push(format!("Docs: {}", docs.join(", ")));
        }

        if parts.is_empty() {
            None
        } else {
            Some(format!("{}.", parts.join(". ")))
        }
    }

    /// Documentation file basenames (relevant docs), order preserved.
    fn doc_basenames(&self) -> Vec<String> {
        let Some(docs) = &self.documentation else {
            return Vec::new();
        };
        docs.relevant_files
            .iter()
            .map(|d| module_short_name(&d.path).to_string())
            .collect()
    }

    /// The compressed RELIABILITY caveat (ORIENT-DENSITY-1 §3.5) — the reader-frame in-scope
    /// rate + band + "verify against source", NOT the three-axis Degradation block. Rendered at
    /// EVERY budget, so per iteration-5 §2 it honors the ratified unknown/unclassified rules: a
    /// zero-in-scope (all-external) repo reads "no in-scope calls measured" + compact external
    /// context (never silence), and a material unclassified share appends a compact second caveat
    /// line. `None` when trust is high with nothing degraded, or there is no briefing. The full
    /// per-axis breakdown + named coverage map still render at `--full` (`render_degradation` /
    /// `render_external_coverage`).
    pub(super) fn reliability_caveat_line(&self) -> Option<String> {
        let trust = self.trust_briefing.as_ref()?;

        if let Some(rel) = &trust.reliability {
            if let Some(cg) = &rel.call_graph {
                // RELIABILITY-REFRAME-1 (iteration-5 §2): the ratified unknown/unclassified rules
                // apply at EVERY budget, so this compressed headline honors them from the SAME
                // shared projection the `--full` surface uses (built once here, from the overlay's
                // real call-coverage COUNTS — not a rate parsed out of prose).
                let view = self.call_reliability_view(Some(&cg.level));

                // Zero in-scope calls (an all-external repo → the vacuous 0-of-0 HIGH band):
                // `resolution == None` distinguishes it from a genuine HIGH. It must NOT fall
                // silent — render the honest "no in-scope calls measured" (unknown, never a
                // fabricated 100%) + the external share as compact context, at every budget.
                if let Some(v) = &view {
                    // review-6 §1: an EMPTY call graph (total_calls == 0) is still a
                    // zero-in-scope measurement — the vacuous HIGH band must not fall
                    // silent either. No `total_calls > 0` gate.
                    if v.resolution.is_none() {
                        return Some(self.zero_in_scope_caveat_line(v));
                    }
                }

                if cg.level != "HIGH" {
                    // The reader-frame in-scope rate + band. Falls back to the rate-only path when
                    // the overlay predates `call_coverage`; both go through the same reader-frame
                    // wording, so the one-liner and the full Degradation agree and neither grades
                    // repo-graph.
                    let detail = if let Some(v) = &view {
                        v.resolved_with_band()
                    } else if let Some(pct) = self.call_resolution_pct(cg) {
                        reliability::resolved_phrase_with_band(pct, &cg.level)
                    } else {
                        format!("your code's call resolution is {}", cg.level)
                    };
                    let mut line = format!(
                        "Reliability: {} — verify call/dead claims against source.",
                        detail
                    );
                    // The material-unclassified qualification (review-3 §2) rides the headline as a
                    // compact second line — from the SAME shared helper the `--full` External calls
                    // section uses, so the rate's honest lower-bound caveat cannot fork or vanish at
                    // the default budget.
                    if let Some(caveat) = self.material_unclassified_caveat(view.as_ref()) {
                        line.push('\n');
                        line.push_str(&caveat);
                    }
                    return Some(line);
                }
            }
            // call-graph is fine; surface the worst remaining degraded axis.
            for (name, axis) in [
                ("import-graph", &rel.import_graph),
                ("change-impact", &rel.change_impact),
            ] {
                if let Some(ax) = axis {
                    if ax.level != "HIGH" {
                        return Some(format!(
                            "Reliability: {} reliability {} — verify against source.",
                            name, ax.level
                        ));
                    }
                }
            }
            return None;
        }

        // Legacy TrustOverlay fields (backward compatibility). Same in-scope
        // `call_resolution_rate` value, reframed to the reader's terms.
        if let Some(rate) = trust.call_resolution_rate {
            if rate < 0.95 {
                return Some(format!(
                    "Reliability: {} — verify call/dead claims against source.",
                    reliability::resolved_phrase_pct(rate * 100.0)
                ));
            }
        }
        if let Some(level) = &trust.call_graph_reliability {
            if level != "high" {
                return Some(format!(
                    "Reliability: your code's call resolution is {} — verify call/dead claims against source.",
                    level
                ));
            }
        }
        None
    }

    /// Extract the raw call-resolution percentage from a reliability axis's
    /// machine reasons (`call_resolution_rate=42.0%_below_50%`). Returned
    /// unrounded so the caller can format it identically to `humanize_reason`
    /// (`{:.0}`), keeping the headline and the full Degradation consistent.
    pub(super) fn call_resolution_pct(&self, axis: &ReliabilityAxis) -> Option<f64> {
        for r in &axis.reasons {
            if let Some(rest) = r.strip_prefix("call_resolution_rate=") {
                let num = rest.split('%').next().unwrap_or("");
                if let Ok(v) = num.parse::<f64>() {
                    return Some(v);
                }
            }
        }
        None
    }

    /// Build the ONE shared reader-frame projection
    /// ([`repo_graph_agent::reliability::CallReliabilityView`]) from the trust overlay's
    /// call-coverage COUNTS — the same derivation `trust` and `check` consume, so orient's
    /// in-scope rate / external share / named coverage map cannot diverge from theirs.
    /// `band` is the serialized call-graph level ("LOW"/…), mapped to the typed band.
    /// `None` when the overlay predates `call_coverage` (older daemon) — callers fall back
    /// to the rate-only legacy path.
    pub(super) fn call_reliability_view(&self, band: Option<&str>) -> Option<CallReliabilityView> {
        let cov = self.trust_briefing.as_ref()?.call_coverage.as_ref()?;
        let total_calls = cov.resolved_calls + cov.unresolved_calls;
        // Already external-filtered + count-desc at the producer; re-filter defensively so a
        // non-external target can never leak into the reader's coverage map.
        let named: Vec<ExternalTarget> = cov
            .external_targets
            .iter()
            .filter(|t| t.is_external && t.count > 0)
            .map(|t| ExternalTarget {
                type_name: t.type_name.clone(),
                count: t.count,
            })
            .collect();
        Some(CallReliabilityView::derive(
            cov.resolved_calls,
            cov.unresolved_calls_internal_like,
            cov.unresolved_calls_external,
            total_calls,
            named,
            band.and_then(reliability::band_from_wire),
        ))
    }

    /// The compressed headline for a repo with NO in-scope calls to grade — an all-external
    /// repo, which yields the vacuous 0-of-0 HIGH band. Renders the honest
    /// "no in-scope calls measured" (unknown, never silence, never a fabricated 100%) followed
    /// by the external share as compact context, so even the small-budget reader learns WHERE
    /// the calls go. Both strings come from the ONE shared projection; the fuller NAMED map is
    /// the `--full` External calls section. `view.external_line()` is guaranteed `Some` here —
    /// zero in-scope with calls present means every call is external.
    fn zero_in_scope_caveat_line(&self, view: &CallReliabilityView) -> String {
        let mut line = format!("Reliability: {}", reliability::NO_IN_SCOPE_CALLS);
        if let Some(external) = view.external_line() {
            line.push_str("; ");
            line.push_str(&external);
        }
        line.push('.');
        line
    }

    /// The conservative-rate caveat (review-3 §2) shared by the compressed headline and the
    /// `--full` External calls section — ONE home, so the two surfaces cannot fork. `None` when
    /// there is no in-scope rate, no `call_coverage` counts, or the unclassified share is
    /// immaterial ([`reliability::unclassified_caveat`] is the single materiality gate). The
    /// unclassified count and in-scope denominator both come from the SAME overlay the view is
    /// built from, so the caveat can never contradict the rate it qualifies.
    pub(super) fn material_unclassified_caveat(
        &self,
        view: Option<&CallReliabilityView>,
    ) -> Option<String> {
        let res = view?.resolution?;
        let cov = self.trust_briefing.as_ref()?.call_coverage.as_ref()?;
        reliability::unclassified_caveat(
            cov.unresolved_calls_unknown,
            res.in_scope_or_unclassified_total,
        )
    }

    /// The full package-group breakdown (NAMED, with file + test counts), shown
    /// at `medium` and up — the scannable KEY STRUCTURE (ORIENT-DENSITY-1;
    /// MODULE-MODEL-1 D4): the directory/package TOPOLOGY (Layer 0/1). DISTINCT
    /// from `module_breakdown_section` below (the declared/inferred notion).
    /// Empty when no directory owns files.
    pub(super) fn package_groups_section(&self, cap: Option<usize>) -> String {
        let Some(ev) = self.module_summary_evidence() else {
            return String::new();
        };
        let Some(groups) = ev.get("package_groups").and_then(|v| v.as_array()) else {
            return String::new();
        };
        if groups.is_empty() {
            return String::new();
        }

        // §13 D7: top-N by file count (the fold returns them size-DESC), then an
        // honest omission line. `cap` is `OrientDepth::package_group_section_cap`:
        // `None` is the generic "uncapped" sentinel this renderer still honors
        // (`unwrap_or(total)` renders every group), but NO rendered detail tier
        // passes it — `medium` caps at 20, `large`/`--full` at 50 (`--full` renders
        // the SAME capped section as `large`; the complexity table is the only
        // section `--full` uncaps, NOT this one). The lone `None` producer is
        // `small`, which never reaches this renderer (`shows_detail()` gates the
        // section off). `total` is the COMPLETE set — the fold never caps and the
        // JSON carries it whole — so the omission count is always TRUE at scale.
        let total = groups.len();
        let limit = cap.unwrap_or(total);
        let mut out = heading("Package groups (directory/package topology — Layer 0/1)");
        // ROOT-MANIFEST-POLYGLOT (ratified 2026-07-12): when a repo-root manifest was
        // suppressed by the conservative rule, the deliberate degradation RENDERS here
        // as one reader-frame line (before the groups, so the reader knows what to
        // expect) — not hidden in a comment. The daemon aggregator ships the exact
        // string via `root_manifest_limitation` (shared with the `stats` surface, so
        // the two agree). A plain indented note (not a bullet) so it never reads as a
        // group row.
        if let Some(note) = ev.get("root_manifest_limitation").and_then(|v| v.as_str()) {
            out.push_str(&sub_heading(note));
        }
        for g in groups.iter().take(limit) {
            let name = g
                .get("name")
                .and_then(|p| p.as_str())
                .unwrap_or("(unknown)");
            let files = g.get("file_count").and_then(|v| v.as_u64()).unwrap_or(0);
            let test = g
                .get("test_file_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let test_suffix = if test > 0 {
                format!(" ({test} test)")
            } else {
                String::new()
            };
            out.push_str(&bullet(&format!(
                "{} — {} file{}{}",
                name,
                files,
                plural(files),
                test_suffix
            )));
        }
        let shown = limit.min(total);
        if total > shown {
            out.push_str(&bullet(&format!(
                "… and {} more group{} — see `stats --json` / `modules`",
                total - shown,
                plural((total - shown) as u64)
            )));
        }
        out
    }

    /// The full per-module breakdown (NAMED, with file counts) for the declared/
    /// inferred `module_candidates` notion (Layer 1/2), shown at `medium` and up.
    /// DISTINCT from `package_groups_section` above (the physical topology) —
    /// separately labelled, never collapsed (MODULE-MODEL-1). Empty when none.
    pub(super) fn module_breakdown_section(&self) -> String {
        let Some(ev) = self.module_summary_evidence() else {
            return String::new();
        };
        let Some(modules) = ev.get("top_modules").and_then(|v| v.as_array()) else {
            return String::new();
        };
        if modules.is_empty() {
            return String::new();
        }

        let mut out = heading("Modules (declared/inferred, by size)");
        for m in modules {
            let path = m
                .get("path")
                .and_then(|p| p.as_str())
                .unwrap_or("(unknown)");
            let files = m.get("file_count").and_then(|v| v.as_u64()).unwrap_or(0);
            out.push_str(&bullet(&format!(
                "{} — {} file{}",
                path,
                files,
                plural(files)
            )));
        }
        if let Some(total) = ev.get("discovered_module_count").and_then(|v| v.as_u64()) {
            let shown = modules.len() as u64;
            if total > shown {
                out.push_str(&bullet(&format!(
                    "+{} more — rmap modules list",
                    total - shown
                )));
            }
        }
        out
    }

    /// The remaining (non-headline) signals, grouped by severity — shown
    /// at `--full` so the complete signal set is preserved (the headline
    /// already covered the load-bearing codes).
    pub(super) fn other_signals_section(&self) -> String {
        let others: Vec<&Signal> = self
            .signals
            .iter()
            .map(|leaf| &leaf.value)
            .filter(|s| !HEADLINE_CODES.contains(&s.code.as_str()))
            .collect();
        if others.is_empty() {
            return String::new();
        }

        let mut out = heading("Other signals");
        for sev in [
            DisplaySeverity::High,
            DisplaySeverity::Medium,
            DisplaySeverity::Low,
        ] {
            for s in others
                .iter()
                .filter(|s| DisplaySeverity::parse(&s.severity) == sev)
            {
                out.push_str(&bullet(&s.summary));
            }
        }
        out
    }
    /// Format cycle anchor as "A -> B -> C -> ... -> A"
    fn format_cycle_anchor(&self, modules: &[serde_json::Value], _length: usize) -> String {
        let names: Vec<&str> = modules.iter().filter_map(|m| m.as_str()).collect();

        if names.is_empty() {
            return "(empty cycle)".to_string();
        }

        if names.len() <= 4 {
            // Show full chain
            let mut chain = names.join(" -> ");
            chain.push_str(&format!(" -> {}", names[0]));
            chain
        } else {
            // Truncate: first 3 -> ... -> last -> first
            let mut chain = names[..3].join(" -> ");
            chain.push_str(" -> ...");
            chain.push_str(&format!(" -> {} -> {}", names[names.len() - 1], names[0]));
            chain
        }
    }

    pub(super) fn render_limits(&self) -> String {
        // EMBED-SEED-IMPL-1: the SEMANTIC_FALLBACK[_UNAVAILABLE] limit is rendered by
        // the dedicated top-of-output semantic section at EVERY depth
        // (`render_semantic_fallback`); it is NOT rendered here. When it is the ONLY
        // limit, emit NOTHING — never a bare "Limits" heading with no items (that
        // spurious empty section otherwise appears on `no_match --full`, and would
        // break resolved/ambiguous byte-parity if a semantic limit ever rode along).
        let renderable: Vec<_> = self
            .limits
            .iter()
            .filter(|l| !l.code.starts_with("SEMANTIC_FALLBACK"))
            .collect();
        if renderable.is_empty() {
            return String::new();
        }
        let mut out = heading("Limits");
        for limit in renderable {
            out.push_str(&bullet(&limit.summary));
        }
        out
    }

    /// EMBED-SEED-IMPL-1 (spec §8.2 Group A): render the semantic fallback tier for
    /// HUMAN mode — the labeled Layer-3 candidate list (or the honest degraded/
    /// known-zero line) that a `no_match` orient/explain now carries. Rendered at
    /// EVERY depth (the candidates ARE the load-bearing answer on a no-match), right
    /// after the focus line. Returns empty for any resolved/ambiguous focus so those
    /// remain byte-identical (the tier is unreachable there); empty too when a
    /// no-match carries no seed candidates AND no seed limit (an old daemon / seeding
    /// never consulted) — today's output untouched.
    pub(super) fn render_semantic_fallback(&self) -> String {
        // Only the deterministic-zero branch (§8.1) — never resolved/ambiguous.
        if self.focus.resolved || self.focus.reason.as_deref() != Some("no_match") {
            return String::new();
        }
        // Labeled embedding candidates only (a deterministic ambiguity candidate has no
        // `source`); on a no-match the tier is the only candidate producer, but this
        // guard keeps the render honest regardless.
        let embedding: Vec<&serde_json::Value> = self
            .focus
            .candidates
            .iter()
            .filter(|c| c.get("source").and_then(|s| s.as_str()) == Some("embedding"))
            .collect();

        // The honesty header: the SEMANTIC_FALLBACK limit's fixed summary (§8.2). The
        // degraded/known-zero line rides SEMANTIC_FALLBACK_UNAVAILABLE / SEMANTIC_FALLBACK
        // with no candidates. Read it from the limits the daemon attached (never fabricate).
        let semantic_limit = self
            .limits
            .iter()
            .find(|l| l.code.starts_with("SEMANTIC_FALLBACK"));

        if embedding.is_empty() {
            // No candidates: honest degraded / known-zero line (if the tier was
            // consulted), WITH the specific cause from `reasons` (review-9 #2 — a
            // dead endpoint reads "no local embedding model reachable", not the
            // generic summary). Nothing at all ⇒ today's output (old daemon).
            return match semantic_limit {
                Some(limit) => crate::presentation::seed::render_semantic_header(
                    &limit.summary,
                    &limit.reasons,
                ),
                None => String::new(),
            };
        }

        // Fired: the honesty header + the per-cause reasons (which carry the model id
        // and, when present, the stale-subset "N files changed since last embed"
        // detail — review-9 #2), then the labeled candidate list.
        let mut out = match semantic_limit {
            Some(limit) => {
                crate::presentation::seed::render_semantic_header(&limit.summary, &limit.reasons)
            }
            None => "Semantic hints: No exact match — the candidates below are Layer-3 embedding hints, not resolved facts.\n".to_string(),
        };
        for (i, c) in embedding.iter().enumerate() {
            // Group A's `FocusCandidate` serializes the path under `file` — the shared
            // formatter (with Group B) reads that field; honesty rules preserved.
            out.push_str(&format!("  {}. ", i + 1));
            out.push_str(&crate::presentation::seed::render_candidate_body(c, "file"));
        }
        out
    }

    pub(super) fn render_next_steps(&self) -> String {
        let mut out = heading("Next steps");
        for action in &self.next {
            let cmd = match &action.target {
                Some(target) => format!("rmap {} {}", action.kind, target),
                None => format!("rmap {}", action.kind),
            };
            out.push_str(&bullet(&cmd));
        }
        out
    }
}
