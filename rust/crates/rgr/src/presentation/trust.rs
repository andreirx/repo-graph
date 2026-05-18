//! Presentation layer for the `trust` command.
//!
//! # CLI-OUT-2B
//!
//! Transforms daemon TrustReport into human-readable plain text.
//!
//! ## Human Output Structure
//!
//! ```text
//! Trust Report: billing-service
//! Snapshot: snap_01kr...
//!
//! Resolution
//!   - Calls: 78% resolved (1234 of 1582)
//!   - Imports: 95% resolved (2100 of 2210)
//!
//! Reliability
//!   - Call-graph: LOW (unresolved calls exceed threshold)
//!   - Import-graph: HIGH
//!   - Change-impact: MEDIUM (low call resolution affects impact analysis)
//!
//! Unresolved Breakdown
//!   - 234 calls_function_ambiguous_or_missing
//!   - 114 calls_obj_method_needs_type_info
//!
//! Classification
//!   - 45 unknown
//!   - 89 external library candidate
//!   - 214 internal candidate
//!
//! Suspicious Modules (zero connectivity)
//!   - src/legacy/adapter
//!
//! Triggered Downgrades
//!   - framework_heavy_suspicion: Express routes detected, call resolution degraded
//! ```

use serde::Deserialize;

use crate::presentation::{bullet, heading, kv_line};

// ── Response Types ───────────────────────────────────────────────────────────

/// Deserialized trust response from daemon.
#[derive(Debug, Deserialize)]
pub struct TrustResponse {
    pub snapshot_uid: String,
    /// Human-readable repo name for CLI display.
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub basis_commit: Option<String>,
    pub summary: TrustSummary,
    #[serde(default)]
    pub categories: Vec<CategoryRow>,
    #[serde(default)]
    pub classifications: Vec<ClassificationRow>,
    #[serde(default)]
    pub modules: Vec<ModuleRow>,
    #[serde(default)]
    pub caveats: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct TrustSummary {
    pub edges_total: u64,
    pub edges_resolved: u64,
    pub unresolved_total: u64,
    pub resolved_calls: u64,
    pub unresolved_calls: u64,
    #[serde(default)]
    pub unresolved_calls_external: u64,
    #[serde(default)]
    pub unresolved_calls_internal_like: u64,
    pub call_resolution_rate: f64,
    pub reliability: Reliability,
    pub triggered_downgrades: TriggeredDowngrades,
}

#[derive(Debug, Deserialize)]
pub struct Reliability {
    pub import_graph: AxisScore,
    pub call_graph: AxisScore,
    pub change_impact: AxisScore,
}

#[derive(Debug, Deserialize)]
pub struct AxisScore {
    pub level: String,
    #[serde(default)]
    pub reasons: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct TriggeredDowngrades {
    pub framework_heavy_suspicion: DowngradeTrigger,
    pub registry_pattern_suspicion: DowngradeTrigger,
    pub missing_entrypoint_declarations: DowngradeTrigger,
    pub alias_resolution_suspicion: DowngradeTrigger,
}

#[derive(Debug, Deserialize)]
pub struct DowngradeTrigger {
    pub triggered: bool,
    #[serde(default)]
    pub reasons: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CategoryRow {
    pub category: String,
    pub label: String,
    pub unresolved: u64,
}

#[derive(Debug, Deserialize)]
pub struct ClassificationRow {
    pub classification: String,
    pub count: u64,
}

#[derive(Debug, Deserialize)]
pub struct ModuleRow {
    pub module_stable_key: String,
    pub qualified_name: String,
    pub fan_in: u64,
    pub fan_out: u64,
    pub file_count: u64,
    #[serde(default)]
    pub suspicious_zero_connectivity: bool,
    #[serde(default)]
    pub trust_notes: Vec<String>,
}

// ── Human Rendering ──────────────────────────────────────────────────────────

impl TrustResponse {
    /// Render the trust response as human-readable plain text.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // ── Header ─────────────────────────────────────────────────
        // Fallback to snapshot_uid prefix if display_name absent (consistent with cycles)
        let repo_display = self
            .display_name
            .as_deref()
            .unwrap_or_else(|| &self.snapshot_uid);
        out.push_str(&kv_line("Trust Report", repo_display));
        out.push_str(&kv_line("Snapshot", &truncate_uid(&self.snapshot_uid)));
        out.push('\n');

        // ── Resolution ─────────────────────────────────────────────
        out.push_str(&self.render_resolution());
        out.push('\n');

        // ── Reliability ────────────────────────────────────────────
        out.push_str(&self.render_reliability());
        out.push('\n');

        // ── Unresolved Breakdown ───────────────────────────────────
        let breakdown = self.render_unresolved_breakdown();
        if !breakdown.is_empty() {
            out.push_str(&breakdown);
            out.push('\n');
        }

        // ── Classification ─────────────────────────────────────────
        let classification = self.render_classification();
        if !classification.is_empty() {
            out.push_str(&classification);
            out.push('\n');
        }

        // ── Suspicious Modules ─────────────────────────────────────
        let suspicious = self.render_suspicious_modules();
        if !suspicious.is_empty() {
            out.push_str(&suspicious);
            out.push('\n');
        }

        // ── Triggered Downgrades ───────────────────────────────────
        let downgrades = self.render_downgrades();
        if !downgrades.is_empty() {
            out.push_str(&downgrades);
            out.push('\n');
        }

        // ── Caveats ────────────────────────────────────────────────
        if !self.caveats.is_empty() {
            out.push_str(&heading("Caveats"));
            for caveat in &self.caveats {
                out.push_str(&bullet(caveat));
            }
            out.push('\n');
        }

        out.trim_end().to_string()
    }

    fn render_resolution(&self) -> String {
        let mut out = heading("Resolution");

        let s = &self.summary;

        // Call resolution
        let total_calls = s.resolved_calls + s.unresolved_calls;
        let call_pct = if total_calls > 0 {
            (s.resolved_calls as f64 / total_calls as f64 * 100.0).round() as u64
        } else {
            100
        };
        out.push_str(&bullet(&format!(
            "Calls: {}% resolved ({} of {})",
            call_pct, s.resolved_calls, total_calls
        )));

        // Edge resolution (imports + calls combined)
        let edge_pct = if s.edges_total > 0 {
            (s.edges_resolved as f64 / s.edges_total as f64 * 100.0).round() as u64
        } else {
            100
        };
        out.push_str(&bullet(&format!(
            "Edges: {}% resolved ({} of {})",
            edge_pct, s.edges_resolved, s.edges_total
        )));

        out
    }

    fn render_reliability(&self) -> String {
        let mut out = heading("Reliability");
        let r = &self.summary.reliability;

        out.push_str(&bullet(&format_axis("Call-graph", &r.call_graph)));
        out.push_str(&bullet(&format_axis("Import-graph", &r.import_graph)));
        out.push_str(&bullet(&format_axis("Change-impact", &r.change_impact)));

        out
    }

    fn render_unresolved_breakdown(&self) -> String {
        // Filter to non-zero categories
        let non_zero: Vec<_> = self
            .categories
            .iter()
            .filter(|c| c.unresolved > 0)
            .collect();

        if non_zero.is_empty() {
            return String::new();
        }

        let mut out = heading("Unresolved Breakdown");
        for cat in non_zero {
            out.push_str(&bullet(&format!("{} {}", cat.unresolved, cat.label)));
        }
        out
    }

    fn render_classification(&self) -> String {
        // Filter to non-zero classifications
        let non_zero: Vec<_> = self
            .classifications
            .iter()
            .filter(|c| c.count > 0)
            .collect();

        if non_zero.is_empty() {
            return String::new();
        }

        let mut out = heading("Classification");
        for cls in non_zero {
            out.push_str(&bullet(&format!("{} {}", cls.count, cls.classification)));
        }
        out
    }

    fn render_suspicious_modules(&self) -> String {
        let suspicious: Vec<_> = self
            .modules
            .iter()
            .filter(|m| m.suspicious_zero_connectivity)
            .collect();

        if suspicious.is_empty() {
            return String::new();
        }

        let mut out = heading("Suspicious Modules (zero connectivity)");
        for m in suspicious.iter().take(10) {
            out.push_str(&bullet(&m.qualified_name));
        }
        if suspicious.len() > 10 {
            out.push_str(&bullet(&format!("... ({} more)", suspicious.len() - 10)));
        }
        out
    }

    fn render_downgrades(&self) -> String {
        let d = &self.summary.triggered_downgrades;
        let mut items = Vec::new();

        if d.framework_heavy_suspicion.triggered {
            let reason = d
                .framework_heavy_suspicion
                .reasons
                .first()
                .map(|s| s.as_str())
                .unwrap_or("framework patterns detected");
            items.push(format!("framework_heavy_suspicion: {}", reason));
        }
        if d.registry_pattern_suspicion.triggered {
            let reason = d
                .registry_pattern_suspicion
                .reasons
                .first()
                .map(|s| s.as_str())
                .unwrap_or("registry patterns detected");
            items.push(format!("registry_pattern_suspicion: {}", reason));
        }
        if d.missing_entrypoint_declarations.triggered {
            let reason = d
                .missing_entrypoint_declarations
                .reasons
                .first()
                .map(|s| s.as_str())
                .unwrap_or("entrypoints not declared");
            items.push(format!("missing_entrypoint_declarations: {}", reason));
        }
        if d.alias_resolution_suspicion.triggered {
            let reason = d
                .alias_resolution_suspicion
                .reasons
                .first()
                .map(|s| s.as_str())
                .unwrap_or("alias resolution issues");
            items.push(format!("alias_resolution_suspicion: {}", reason));
        }

        if items.is_empty() {
            return String::new();
        }

        let mut out = heading("Triggered Downgrades");
        for item in items {
            out.push_str(&bullet(&item));
        }
        out
    }
}

fn format_axis(name: &str, axis: &AxisScore) -> String {
    if axis.reasons.is_empty() {
        format!("{}: {}", name, axis.level)
    } else {
        let humanized: Vec<String> = axis.reasons.iter().map(|r| humanize_reason(r)).collect();
        format!("{}: {} ({})", name, axis.level, humanized.join("; "))
    }
}

/// Convert machine-format reasons to human-readable prose.
fn humanize_reason(reason: &str) -> String {
    // Pattern: "call_resolution_rate=33.5%_below_50%"
    if reason.starts_with("call_resolution_rate=") {
        if let Some(rest) = reason.strip_prefix("call_resolution_rate=") {
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

    // Unknown pattern - clean up underscores
    reason.replace('_', " ")
}

fn truncate_uid(uid: &str) -> String {
    if uid.len() > 20 {
        format!("{}...", &uid[..17])
    } else {
        uid.to_string()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_response() -> TrustResponse {
        TrustResponse {
            snapshot_uid: "snap_01kr12345678".to_string(),
            display_name: Some("test-repo".to_string()),
            basis_commit: None,
            summary: TrustSummary {
                edges_total: 100,
                edges_resolved: 80,
                unresolved_total: 20,
                resolved_calls: 50,
                unresolved_calls: 10,
                unresolved_calls_external: 5,
                unresolved_calls_internal_like: 5,
                call_resolution_rate: 0.833,
                reliability: Reliability {
                    import_graph: AxisScore {
                        level: "HIGH".to_string(),
                        reasons: vec![],
                    },
                    call_graph: AxisScore {
                        level: "MEDIUM".to_string(),
                        reasons: vec!["some unresolved calls".to_string()],
                    },
                    change_impact: AxisScore {
                        level: "HIGH".to_string(),
                        reasons: vec![],
                    },
                },
                triggered_downgrades: TriggeredDowngrades {
                    framework_heavy_suspicion: DowngradeTrigger {
                        triggered: false,
                        reasons: vec![],
                    },
                    registry_pattern_suspicion: DowngradeTrigger {
                        triggered: false,
                        reasons: vec![],
                    },
                    missing_entrypoint_declarations: DowngradeTrigger {
                        triggered: false,
                        reasons: vec![],
                    },
                    alias_resolution_suspicion: DowngradeTrigger {
                        triggered: false,
                        reasons: vec![],
                    },
                },
            },
            categories: vec![],
            classifications: vec![],
            modules: vec![],
            caveats: vec![],
        }
    }

    #[test]
    fn render_shows_repo_display_name() {
        let r = minimal_response();
        let out = r.render_human();
        assert!(out.contains("Trust Report: test-repo"));
    }

    #[test]
    fn render_shows_resolution_rates() {
        let r = minimal_response();
        let out = r.render_human();
        assert!(out.contains("Calls: 83% resolved"));
        assert!(out.contains("Edges: 80% resolved"));
    }

    #[test]
    fn render_shows_reliability_levels() {
        let r = minimal_response();
        let out = r.render_human();
        assert!(out.contains("Call-graph: MEDIUM"));
        assert!(out.contains("Import-graph: HIGH"));
    }

    #[test]
    fn render_falls_back_to_snapshot_uid_when_no_display_name() {
        let mut r = minimal_response();
        r.display_name = None;
        let out = r.render_human();
        // Falls back to snapshot_uid when display_name absent
        assert!(out.contains("Trust Report: snap_01kr12345678"));
    }
}
