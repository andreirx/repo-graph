//! Per-signal section renderers for the `explain` command. Split out of `explain.rs` to keep each module
//! under the 500-line structural guardrail. A second `impl ExplainResponse` block (inherent impls may span
//! modules within the defining crate); `explain.rs` keeps the response struct, header, and target/candidate
//! render.

use super::explain::{ExplainResponse, ExplainSignal};
use super::{bullet, heading};

impl ExplainResponse {
    pub(super) fn render_signal_section(&self, signal: &ExplainSignal) -> Option<String> {
        let evidence = signal.evidence.as_ref()?;

        match signal.code.as_str() {
            "EXPLAIN_CALLERS" => Some(self.render_callers(evidence)),
            "EXPLAIN_CALLEES" => Some(self.render_callees(evidence)),
            "EXPLAIN_IMPORTS" => Some(self.render_imports(evidence)),
            "EXPLAIN_SYMBOLS" => Some(self.render_symbols(evidence)),
            "EXPLAIN_FILES" => Some(self.render_files(evidence)),
            "EXPLAIN_CYCLES" => self.render_cycles(evidence),
            "EXPLAIN_BOUNDARY" => self.render_boundary(evidence),
            "EXPLAIN_GATE" => self.render_gate(evidence),
            "EXPLAIN_TRUST" => Some(self.render_trust(evidence)),
            "EXPLAIN_IDENTITY" => None, // Handled in header
            "EXPLAIN_MEASUREMENTS" => self.render_measurements(evidence),
            _ => None,
        }
    }

    fn render_callers(&self, evidence: &serde_json::Value) -> String {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut out = heading(&format!("Callers ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            for item in items.iter().take(10) {
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
            if items.len() > 10 {
                out.push_str(&format!("  ... ({} more)\n", items.len() - 10));
            }
        }

        out
    }

    fn render_callees(&self, evidence: &serde_json::Value) -> String {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut out = heading(&format!("Callees ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            for item in items.iter().take(10) {
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
            if items.len() > 10 {
                out.push_str(&format!("  ... ({} more)\n", items.len() - 10));
            }
        }

        out
    }

    fn render_imports(&self, evidence: &serde_json::Value) -> String {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut out = heading(&format!("Imports ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            for item in items.iter().take(15) {
                let target = item
                    .get("target_file")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unknown)");
                out.push_str(&bullet(target));
            }
            if items.len() > 15 {
                out.push_str(&format!("  ... ({} more)\n", items.len() - 15));
            }
        }

        out
    }

    fn render_symbols(&self, evidence: &serde_json::Value) -> String {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut out = heading(&format!("Symbols ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            for item in items.iter().take(15) {
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
            if items.len() > 15 {
                out.push_str(&format!("  ... ({} more)\n", items.len() - 15));
            }
        }

        out
    }

    fn render_files(&self, evidence: &serde_json::Value) -> String {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut out = heading(&format!("Files ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            for item in items.iter().take(15) {
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
            if items.len() > 15 {
                out.push_str(&format!("  ... ({} more)\n", items.len() - 15));
            }
        }

        out
    }

    fn render_cycles(&self, evidence: &serde_json::Value) -> Option<String> {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        if count == 0 {
            return None;
        }

        let mut out = heading(&format!("Import cycles ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            for (i, item) in items.iter().take(5).enumerate() {
                if let Some(modules) = item.get("modules").and_then(|v| v.as_array()) {
                    let cycle_str: Vec<&str> = modules.iter().filter_map(|m| m.as_str()).collect();
                    out.push_str(&bullet(&format!(
                        "Cycle {}: {}",
                        i + 1,
                        cycle_str.join(" -> ")
                    )));
                }
            }
            if items.len() > 5 {
                out.push_str(&format!("  ... ({} more)\n", items.len() - 5));
            }
        }

        Some(out)
    }

    fn render_boundary(&self, evidence: &serde_json::Value) -> Option<String> {
        let count = evidence
            .get("violation_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if count == 0 {
            return None;
        }

        let mut out = heading(&format!("Boundary violations ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            for item in items.iter().take(10) {
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

    fn render_gate(&self, evidence: &serde_json::Value) -> Option<String> {
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
            for item in items.iter().take(10) {
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

        if let Some(rate) = evidence
            .get("call_resolution_rate")
            .and_then(|v| v.as_f64())
        {
            out.push_str(&bullet(&format!("Call resolution: {:.0}%", rate * 100.0)));
        }
        if let Some(reliability) = evidence
            .get("call_graph_reliability")
            .and_then(|v| v.as_str())
        {
            out.push_str(&bullet(&format!("Call graph reliability: {}", reliability)));
        }
        if let Some(enrichment) = evidence.get("enrichment_state").and_then(|v| v.as_str()) {
            out.push_str(&bullet(&format!("Enrichment: {}", enrichment)));
        }

        out
    }

    fn render_measurements(&self, evidence: &serde_json::Value) -> Option<String> {
        let items = evidence.get("items").and_then(|v| v.as_array())?;
        if items.is_empty() {
            return None;
        }

        let mut out = heading("Measurements");

        for item in items.iter().take(10) {
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
