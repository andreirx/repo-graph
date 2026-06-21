//! Dense-headline + depth section renderers for the `orient` command (ORIENT-DENSITY-1).
//!
//! Split out of `orient.rs` to keep each module under the 500-line structural
//! guardrail — the same idiom `explain.rs` / `explain_sections.rs` already use.
//! This is a SECOND `impl OrientResponse` block: inherent impls may span modules
//! within the defining crate, so `orient.rs` keeps the response structs, the
//! `OrientDepth` budget→depth map, the `render_orient_envelope` wrapper, and the
//! `render_human` orchestrator, while the per-section renderers it calls live here.
//!
//! Visibility contract: methods invoked by `render_human` (in the sibling `orient`
//! module) are `pub(super)` — visible to `presentation` and its descendants, no
//! wider; the helpers they call internally stay private. No behavior changed in the
//! split; it is pure relocation, verified by the orient render tests.

use super::orient::{OrientDepth, OrientResponse, ReliabilityAxis, Signal};
use super::{bullet, heading, DisplaySeverity};

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
/// Collisions are acceptable for the dense headline (module discovery is
/// fuzzy-by-design, VISION); the full paths appear in the breakdown at
/// `--full`.
fn module_short_name(path: &str) -> &str {
    path.rsplit('/').find(|s| !s.is_empty()).unwrap_or(path)
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
    /// load-bearing governance facts surfaced FIRST. Empty for a clean
    /// repo (e.g. nginx). Honest: these are the truth-audit's own signals,
    /// rendered verbatim from each signal's summary.
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

    /// The dense STRUCTURE line: `<repo> · <N> files[, <M> symbols] · <K>
    /// modules: a, b, c`. Module names are capped by depth; the true total
    /// (`discovered_module_count`) drives the count and the `+N more` tail.
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

        let names: Vec<&str> = ev
            .get("top_modules")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .take(depth.module_name_cap())
                    .filter_map(|m| m.get("path").and_then(|p| p.as_str()))
                    .map(module_short_name)
                    .collect()
            })
            .unwrap_or_default();
        let total = ev.get("discovered_module_count").and_then(|v| v.as_u64());

        if !names.is_empty() {
            let total = total.unwrap_or(names.len() as u64);
            line.push_str(&format!(
                " · {} module{}: {}",
                total,
                plural(total),
                names.join(", ")
            ));
            let shown = names.len() as u64;
            if total > shown {
                line.push_str(&format!(", +{} more", total - shown));
            }
        } else if let Some(total) = total {
            line.push_str(&format!(" · {} module{}", total, plural(total)));
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
        // The "+N more" pointer is honest at small/medium (the headline is the
        // ONLY complexity surface there). At large/--full it is SUPPRESSED: the
        // dedicated `complexity_breakdown_section` below renders the COMPLETE set
        // (review-1 #2 — `--full` is complete, not "+338 more").
        if !depth.shows_full_detail() {
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

    /// The COMPLETE complexity centers, NAMED with cyclomatic complexity, shown
    /// at `large` / `--full` (ORIENT-DENSITY-1 §5, review-1 #2). The agent now
    /// carries EVERY above-threshold center in the evidence at these budgets, so
    /// this section is the full set — no truncation pointer. Empty when no
    /// complexity signal is present (clean repo / measurements unavailable).
    /// Mirrors `module_breakdown_section`: the headline names the top few; this
    /// section is the authoritative complete list.
    pub(super) fn complexity_breakdown_section(&self) -> String {
        let Some(ev) = self.signal_evidence("HIGH_COMPLEXITY") else {
            return String::new();
        };
        let Some(top) = ev.get("top_complex").and_then(|v| v.as_array()) else {
            return String::new();
        };
        if top.is_empty() {
            return String::new();
        }

        let mut out = heading("Complexity centers (by cyclomatic complexity)");
        for entry in top {
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
        }
        out
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

    /// The single compressed RELIABILITY caveat (ORIENT-DENSITY-1 §3.5):
    /// one honest line (the real call-resolution rate + level + "verify
    /// against source"), NOT three degradation lines. `None` when trust is
    /// high (clean) or no briefing is present. The full per-axis breakdown
    /// still renders at `--full` via [`render_degradation`].
    pub(super) fn reliability_caveat_line(&self) -> Option<String> {
        let trust = self.trust_briefing.as_ref()?;

        if let Some(rel) = &trust.reliability {
            if let Some(cg) = &rel.call_graph {
                if cg.level != "HIGH" {
                    let detail = match self.call_resolution_pct(cg) {
                        // `{:.0}` matches `humanize_reason`'s formatting so the one-line
                        // caveat and the full per-axis Degradation report the SAME number
                        // for the same fact (no 42-vs-43 split at a .5 boundary).
                        Some(pct) => format!("call-graph {pct:.0}% resolved ({})", cg.level),
                        None => format!("call-graph reliability {}", cg.level),
                    };
                    return Some(format!(
                        "Reliability: {} — verify call/dead claims against source.",
                        detail
                    ));
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

        // Legacy TrustOverlay fields (backward compatibility).
        if let Some(rate) = trust.call_resolution_rate {
            if rate < 0.95 {
                return Some(format!(
                    "Reliability: call-graph {:.0}% resolved — verify call/dead claims against source.",
                    rate * 100.0
                ));
            }
        }
        if let Some(level) = &trust.call_graph_reliability {
            if level != "high" {
                return Some(format!(
                    "Reliability: call-graph reliability {} — verify call/dead claims against source.",
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
    fn call_resolution_pct(&self, axis: &ReliabilityAxis) -> Option<f64> {
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

    /// The full per-module breakdown (NAMED, with file counts), shown at
    /// `large` / `--full`. Empty when no module discovery data exists.
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

        let mut out = heading("Modules (by size)");
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
        let mut out = heading("Limits");
        for limit in &self.limits {
            out.push_str(&bullet(&limit.summary));
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
