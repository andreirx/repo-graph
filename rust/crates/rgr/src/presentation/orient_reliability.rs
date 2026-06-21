//! Verbose per-axis "Degradation" rendering for the `orient` command.
//!
//! Split out of `orient_sections.rs` to keep each module under the 500-line
//! structural guardrail (ORIENT-DENSITY-1 review-1 #3) — the `orient.rs` /
//! `orient_sections.rs` / `orient_tests.rs` split idiom, extended. This is a
//! THIRD `impl OrientResponse` block (inherent impls may span modules within the
//! crate); it owns ONLY the `--full` Degradation block — the machine-reason →
//! human-prose expansion of each reliability axis.
//!
//! The COMPRESSED one-line reliability caveat that leads the dense headline
//! stays in `orient_sections.rs` (`reliability_caveat_line`); this module is the
//! expanded depth shown at `large`/`--full`. No behavior changed in the split.

use super::orient::{OrientResponse, ReliabilityAxis, TrustOverlay};
use super::{bullet_list, heading};

impl OrientResponse {
    /// The full per-axis "Degradation" block (call-graph / import-graph /
    /// change-impact), shown at `large` / `--full`. Empty when nothing is
    /// degraded. Renders the new `reliability` structure when present, else the
    /// legacy `TrustOverlay` fields (backward compatibility).
    pub(super) fn render_degradation(&self, trust: &TrustOverlay) -> String {
        let mut items: Vec<String> = Vec::new();

        // Render from new reliability structure if present
        if let Some(reliability) = &trust.reliability {
            if let Some(cg) = &reliability.call_graph {
                if cg.level != "HIGH" {
                    items.push(self.format_reliability_axis("Call-graph", cg));
                }
            }
            if let Some(ig) = &reliability.import_graph {
                if ig.level != "HIGH" {
                    items.push(self.format_reliability_axis("Import-graph", ig));
                }
            }
            if let Some(ci) = &reliability.change_impact {
                if ci.level != "HIGH" {
                    items.push(self.format_reliability_axis("Change-impact", ci));
                }
            }
        } else {
            // Legacy fallback: use old fields
            if let Some(rate) = trust.call_resolution_rate {
                if rate < 0.95 {
                    items.push(format!("Call resolution rate: {:.0}%", rate * 100.0));
                }
            }
            if let Some(reliability) = &trust.call_graph_reliability {
                if reliability != "high" {
                    items.push(format!("Call graph reliability: {}", reliability));
                }
            }
            // Caveats from legacy path
            for caveat in &trust.caveats {
                items.push(caveat.clone());
            }
        }

        if items.is_empty() {
            return String::new();
        }

        let mut out = heading("Degradation");
        out.push_str(&bullet_list(&items));
        out
    }

    /// Format a reliability axis as human-readable prose.
    ///
    /// Converts machine tokens like "call_resolution_rate=33.5%_below_50%"
    /// into "33% call resolution (below 50% threshold)".
    fn format_reliability_axis(&self, name: &str, axis: &ReliabilityAxis) -> String {
        let level = &axis.level;

        if axis.reasons.is_empty() {
            return format!("{} reliability is {} on this repo. Do not use for safety-critical decisions without verification.", name, level);
        }

        // Convert machine reasons to human prose
        let human_reasons: Vec<String> = axis
            .reasons
            .iter()
            .map(|r| self.humanize_reason(r))
            .collect();

        format!(
            "{} reliability is {} ({})",
            name,
            level,
            human_reasons.join("; ")
        )
    }

    /// Convert a machine-format reason to human-readable prose.
    fn humanize_reason(&self, reason: &str) -> String {
        // Pattern: "call_resolution_rate=33.5%_below_50%"
        if reason.starts_with("call_resolution_rate=") {
            if let Some(rest) = reason.strip_prefix("call_resolution_rate=") {
                // Extract rate and threshold
                // Format: "33.5%_below_50%"
                let parts: Vec<&str> = rest.split("_below_").collect();
                if parts.len() == 2 {
                    let rate = parts[0].trim_end_matches('%');
                    let threshold = parts[1].trim_end_matches('%');
                    if let (Ok(r), Ok(t)) = (rate.parse::<f64>(), threshold.parse::<f64>()) {
                        return format!("{:.0}% call resolution, below {}% threshold", r, t);
                    }
                }
            }
        }

        // Pattern: "unresolved_imports=944"
        if reason.starts_with("unresolved_imports=") {
            if let Some(count) = reason.strip_prefix("unresolved_imports=") {
                if let Ok(n) = count.parse::<u64>() {
                    return format!("{} unresolved imports", n);
                }
            }
        }

        // Pattern: "alias_resolution_suspicion"
        if reason == "alias_resolution_suspicion" {
            return "alias resolution suspected".to_string();
        }

        // Pattern: "missing_entrypoint_declarations"
        if reason == "missing_entrypoint_declarations" {
            return "no entrypoints declared".to_string();
        }

        // Pattern: "registry_pattern_suspicion"
        if reason == "registry_pattern_suspicion" {
            return "registry/factory patterns detected".to_string();
        }

        // Unknown pattern - return as-is but cleaned up
        reason.replace('_', " ")
    }
}
