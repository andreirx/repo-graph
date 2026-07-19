//! Per-signal section renderers for the `explain` command. Split out of `explain.rs` to keep each module
//! under the 500-line structural guardrail. A second `impl ExplainResponse` block (inherent impls may span
//! modules within the defining crate); `explain.rs` keeps the response struct, header, and target/candidate
//! render.
//!
//! # TRUNCATION-AUDIT-1 — the SECOND (presentation) cap
//!
//! explain truncates in TWO independent places. (1) The agent data layer caps each section's item list at
//! `items_cap(budget)` and reports the cut via `*_truncated` — `--full` maps to `Budget::Full`, whose cap is
//! `usize::MAX`, so the daemon then emits EVERY item (proven at the daemon boundary by
//! `explain_full_budget_uncaps_over_cap_file_listing`). (2) These renderers cap the HUMAN display AGAIN with a
//! per-section `.take(N)` (10/15/5) plus a "... (N more)" note — a fixed readability limit independent of the
//! budget. Without honoring `--full` here, the JSON uncaps but the human render still truncates, defeating the
//! slice's acceptance use case `rmap explain <target> --full | grep <x>` (review-1 #1).
//!
//! So `full` threads in from `run_explain_cmd` (it already parsed the `--full` flag) through
//! [`ExplainResponse::render_human`] to each section renderer. Each computes `shown = if full { items.len() }
//! else { N }`: when `full`, the cap lifts to the full length, `take(shown)` keeps everything, and the
//! overflow guard `items.len() > shown` is naturally false (no "... more" line). When not `full`, behaviour is
//! byte-identical to before. We thread a `bool` rather than extract a closure-based helper: the per-section
//! formatting varies (overflow-note vs not; cycles enumerates and skips empty rings), so the shared helper
//! would carry more parameters than the one-line `shown` it would remove.

use repo_graph_agent::reliability;

use super::explain::{ExplainResponse, ExplainSignal};
use super::{bullet, heading};

/// RECON-M-R3a (g2u-b): the union-degree second-figure heading suffix — present ONLY when the
/// daemon attached the additive `union` object (W-BOTH with a current measured ledger AND the
/// union degree differs, §5.3.3b) AND the object passes the §5.3.0 labeling gate (review-2
/// item 1: `accounting: "union"` + a derivable coverage basis, via the ONE shared gate). The
/// coverage renders beside the reconciled value — the mandated human frame "reconciled —
/// combined analyses (coverage: …)"; a missing/malformed label SUPPRESSES the union figure
/// (the heading stays exactly the pipeline heading), never renders it unlabeled.
fn union_degree_suffix(evidence: &serde_json::Value) -> String {
    evidence
        .get("union")
        .and_then(|u| {
            let coverage = crate::presentation::witnesses::union_coverage_phrase(u)?;
            let n = u.get("count").and_then(|v| v.as_u64())?;
            Some(format!(
                " · reconciled {n} — combined analyses (coverage: {coverage})"
            ))
        })
        .unwrap_or_default()
}

impl ExplainResponse {
    /// Render a single signal's section. `full` lifts the per-section display cap (see the module doc):
    /// it is the `--full` flag, threaded so the human render is uncapped for grep.
    pub(super) fn render_signal_section(
        &self,
        signal: &ExplainSignal,
        full: bool,
    ) -> Option<String> {
        let evidence = signal.evidence.as_ref()?;

        match signal.code.as_str() {
            "EXPLAIN_CALLERS" => Some(self.render_callers(evidence, full)),
            "EXPLAIN_CALLEES" => Some(self.render_callees(evidence, full)),
            "EXPLAIN_IMPORTS" => Some(self.render_imports(evidence, full)),
            "EXPLAIN_SYMBOLS" => Some(self.render_symbols(evidence, full)),
            "EXPLAIN_FILES" => Some(self.render_files(evidence, full)),
            "EXPLAIN_CYCLES" => self.render_cycles(evidence, full),
            "EXPLAIN_BOUNDARY" => self.render_boundary(evidence, full),
            "EXPLAIN_GATE" => self.render_gate(evidence, full),
            "EXPLAIN_TRUST" => Some(self.render_trust(evidence)),
            "EXPLAIN_IDENTITY" => None, // Handled in header
            "EXPLAIN_MEASUREMENTS" => self.render_measurements(evidence, full),
            _ => None,
        }
    }

    fn render_callers(&self, evidence: &serde_json::Value, full: bool) -> String {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut out = heading(&format!(
            "Callers ({}{})",
            count,
            union_degree_suffix(evidence)
        ));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            let shown = if full { items.len() } else { 10 };
            for item in items.iter().take(shown) {
                let name = item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unknown)");
                let module = item.get("module").and_then(|v| v.as_str());
                if let Some(m) = module {
                    out.push_str(&bullet(&format!("{} ({})", name, m)));
                } else {
                    out.push_str(&bullet(name));
                }
            }
            if items.len() > shown {
                out.push_str(&format!("  ... ({} more)\n", items.len() - shown));
            }
        }

        out
    }

    fn render_callees(&self, evidence: &serde_json::Value, full: bool) -> String {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut out = heading(&format!(
            "Callees ({}{})",
            count,
            union_degree_suffix(evidence)
        ));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            let shown = if full { items.len() } else { 10 };
            for item in items.iter().take(shown) {
                let name = item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unknown)");
                let module = item.get("module").and_then(|v| v.as_str());
                if let Some(m) = module {
                    out.push_str(&bullet(&format!("{} ({})", name, m)));
                } else {
                    out.push_str(&bullet(name));
                }
            }
            if items.len() > shown {
                out.push_str(&format!("  ... ({} more)\n", items.len() - shown));
            }
        }

        out
    }

    fn render_imports(&self, evidence: &serde_json::Value, full: bool) -> String {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut out = heading(&format!("Imports ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            let shown = if full { items.len() } else { 15 };
            for item in items.iter().take(shown) {
                let target = item
                    .get("target_file")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unknown)");
                out.push_str(&bullet(target));
            }
            if items.len() > shown {
                out.push_str(&format!("  ... ({} more)\n", items.len() - shown));
            }
        }

        out
    }

    fn render_symbols(&self, evidence: &serde_json::Value, full: bool) -> String {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut out = heading(&format!("Symbols ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            let shown = if full { items.len() } else { 15 };
            for item in items.iter().take(shown) {
                let name = item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unknown)");
                let subtype = item.get("subtype").and_then(|v| v.as_str());
                if let Some(st) = subtype {
                    out.push_str(&bullet(&format!("{} ({})", name, st)));
                } else {
                    out.push_str(&bullet(name));
                }
            }
            if items.len() > shown {
                out.push_str(&format!("  ... ({} more)\n", items.len() - shown));
            }
        }

        out
    }

    fn render_files(&self, evidence: &serde_json::Value, full: bool) -> String {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut out = heading(&format!("Files ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            let shown = if full { items.len() } else { 15 };
            for item in items.iter().take(shown) {
                let path = item
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unknown)");
                let symbol_count = item
                    .get("symbol_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                out.push_str(&bullet(&format!("{} ({} symbols)", path, symbol_count)));
            }
            if items.len() > shown {
                out.push_str(&format!("  ... ({} more)\n", items.len() - shown));
            }
        }

        out
    }

    fn render_cycles(&self, evidence: &serde_json::Value, full: bool) -> Option<String> {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        if count == 0 {
            return None;
        }

        let mut out = heading(&format!("Import cycles ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            let shown = if full { items.len() } else { 5 };
            for (i, item) in items.iter().take(shown).enumerate() {
                if let Some(modules) = item.get("modules").and_then(|v| v.as_array()) {
                    let cycle_str: Vec<&str> = modules.iter().filter_map(|m| m.as_str()).collect();
                    out.push_str(&bullet(&format!(
                        "Cycle {}: {}",
                        i + 1,
                        cycle_str.join(" -> ")
                    )));
                }
            }
            if items.len() > shown {
                out.push_str(&format!("  ... ({} more)\n", items.len() - shown));
            }
        }

        Some(out)
    }

    fn render_boundary(&self, evidence: &serde_json::Value, full: bool) -> Option<String> {
        let count = evidence
            .get("violation_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if count == 0 {
            return None;
        }

        let mut out = heading(&format!("Boundary violations ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            let shown = if full { items.len() } else { 10 };
            for item in items.iter().take(shown) {
                let source = item
                    .get("source_module")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let target = item
                    .get("target_module")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let edges = item.get("edge_count").and_then(|v| v.as_u64()).unwrap_or(0);
                out.push_str(&bullet(&format!(
                    "{} -> {} ({} edges)",
                    source, target, edges
                )));
            }
        }

        Some(out)
    }

    fn render_gate(&self, evidence: &serde_json::Value, full: bool) -> Option<String> {
        let outcome = evidence
            .get("outcome")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let count = evidence
            .get("obligation_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let mut out = heading(&format!(
            "Gate ({}: {} obligations)",
            outcome.to_uppercase(),
            count
        ));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            let shown = if full { items.len() } else { 10 };
            for item in items.iter().take(shown) {
                let req_id = item.get("req_id").and_then(|v| v.as_str()).unwrap_or("?");
                let method = item.get("method").and_then(|v| v.as_str()).unwrap_or("?");
                let verdict = item
                    .get("effective_verdict")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                out.push_str(&bullet(&format!("{}: {} ({})", req_id, method, verdict)));
            }
        }

        Some(out)
    }

    fn render_trust(&self, evidence: &serde_json::Value) -> String {
        let mut out = heading("Trust");

        // RELIABILITY-REFRAME-1: the reader's frame, not repo-graph's pipeline — via the ONE shared
        // projection, so this explain surface never forks from orient/trust/check. review-1 §1: prefer
        // the in-scope COUNTS so a 0-of-0 repo renders the honest "no in-scope calls measured"; the
        // `call_resolution_rate` field carries the trust service's 1.0 sentinel for 0-of-0 (a fabricated
        // 100% if trusted). The band folds into the same line rather than a standalone
        // "Call graph reliability: …" (grades-us) bullet. Falls back to the rate-only path only when the
        // additive counts are absent (evidence from a daemon that predates them).
        let band = evidence
            .get("call_graph_reliability")
            .and_then(|v| v.as_str());
        let counts = evidence
            .get("resolved_in_scope")
            .and_then(|v| v.as_u64())
            .zip(
                evidence
                    .get("in_scope_or_unclassified_total")
                    .and_then(|v| v.as_u64()),
            );
        if let Some((resolved, total)) = counts {
            let view = reliability::CallReliabilityView::derive(
                resolved,
                total.saturating_sub(resolved),
                0,
                total,
                Vec::new(),
                band.and_then(reliability::band_from_wire),
            );
            out.push_str(&bullet(&view.resolved_with_band()));
        } else {
            let rate = evidence
                .get("call_resolution_rate")
                .and_then(|v| v.as_f64());
            match (rate, band) {
                (Some(r), Some(b)) => out.push_str(&bullet(
                    &reliability::resolved_phrase_with_band(r * 100.0, &b.to_uppercase()),
                )),
                (Some(r), None) => {
                    out.push_str(&bullet(&reliability::resolved_phrase_pct(r * 100.0)))
                }
                (None, Some(b)) => {
                    out.push_str(&bullet(&format!("your code's call resolution is {}", b)))
                }
                (None, None) => {}
            }
        }
        if let Some(enrichment) = evidence.get("enrichment_state").and_then(|v| v.as_str()) {
            out.push_str(&bullet(&format!("Enrichment: {}", enrichment)));
        }

        out
    }

    fn render_measurements(&self, evidence: &serde_json::Value, full: bool) -> Option<String> {
        let items = evidence.get("items").and_then(|v| v.as_array())?;
        if items.is_empty() {
            return None;
        }

        let mut out = heading("Measurements");

        let shown = if full { items.len() } else { 10 };
        for item in items.iter().take(shown) {
            let kind = item.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
            let value = item.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let aggregation = item
                .get("aggregation")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            out.push_str(&bullet(&format!(
                "{} ({}): {:.2}",
                kind, aggregation, value
            )));
        }

        Some(out)
    }
}
