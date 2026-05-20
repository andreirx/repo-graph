//! Presentation layer for policy command.
//!
//! # CLI-OUT-5 Group 3
//!
//! **Legacy contract exception:** `policy` does NOT use REG-1 daemon contract.
//! Requires explicit `db_path` and `repo_uid` arguments. This is preserved,
//! not migrated.
//!
//! Three policy fact kinds with different structures:
//! - `STATUS_MAPPING`: function-level status code translation tables
//! - `BEHAVIORAL_MARKER`: control flow patterns (retry loops, resume offsets)
//! - `RETURN_FATE`: call-site return value handling classification
//!
//! # Output Contract
//!
//! - Deterministic ordering (by file, line)
//! - Full output, no truncation
//! - `--json` preserved for machine mode
//! - Kind-specific rendering (not a single generic row model)

use serde::Deserialize;
use std::collections::BTreeMap;

// ── STATUS_MAPPING ───────────────────────────────────────────────────────────

/// Response DTO for `policy --kind STATUS_MAPPING`.
#[derive(Debug, Deserialize)]
pub struct StatusMappingResponse {
    pub repo: String,
    pub snapshot: String,
    pub kind: String,
    pub facts: Vec<StatusMappingFact>,
    pub count: usize,
}

/// Individual STATUS_MAPPING fact.
#[derive(Debug, Deserialize, Clone)]
pub struct StatusMappingFact {
    pub symbol_key: String,
    pub function_name: String,
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub source_type: String,
    pub target_type: String,
    pub mappings: Vec<CaseMapping>,
    #[serde(default)]
    pub default_output: Option<String>,
}

/// Case mapping in a STATUS_MAPPING.
#[derive(Debug, Deserialize, Clone)]
pub struct CaseMapping {
    pub inputs: Vec<String>,
    pub output: String,
}

impl StatusMappingResponse {
    /// Render human-readable output.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // Header
        out.push_str("Policy Facts: STATUS_MAPPING\n\n");

        // Count
        let word = if self.count == 1 { "fact" } else { "facts" };
        out.push_str(&format!("{} {}\n", self.count, word));

        if self.count == 0 {
            out.push_str("\nNo status mapping functions detected.\n");
            out.push_str(
                "\nhint: STATUS_MAPPING extracts from C switch statements that translate\n",
            );
            out.push_str("      error/status codes between types.\n");
            return out;
        }

        // Facts sorted by file, then line
        out.push('\n');
        let mut facts = self.facts.clone();
        facts.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.line_start.cmp(&b.line_start))
        });

        for fact in &facts {
            out.push_str(&format!(
                "{}  {}:{}-{}\n",
                fact.function_name, fact.file_path, fact.line_start, fact.line_end
            ));
            out.push_str(&format!("  {} -> {}\n", fact.source_type, fact.target_type));

            // Show mappings (limit to first 5 for readability, with note if more)
            let mapping_count = fact.mappings.len();
            let show_count = mapping_count.min(5);
            for mapping in fact.mappings.iter().take(show_count) {
                let inputs = mapping.inputs.join(", ");
                out.push_str(&format!("    {} -> {}\n", inputs, mapping.output));
            }
            if mapping_count > show_count {
                out.push_str(&format!(
                    "    ... and {} more mappings\n",
                    mapping_count - show_count
                ));
            }

            if let Some(default) = &fact.default_output {
                out.push_str(&format!("    default -> {}\n", default));
            }
            out.push('\n');
        }

        out
    }
}

// ── BEHAVIORAL_MARKER ────────────────────────────────────────────────────────

/// Response DTO for `policy --kind BEHAVIORAL_MARKER`.
#[derive(Debug, Deserialize)]
pub struct BehavioralMarkerResponse {
    pub repo: String,
    pub snapshot: String,
    pub kind: String,
    pub facts: Vec<BehavioralMarkerFact>,
    pub count: usize,
}

/// Individual BEHAVIORAL_MARKER fact.
#[derive(Debug, Deserialize, Clone)]
pub struct BehavioralMarkerFact {
    #[allow(dead_code)]
    pub symbol_key: String,
    pub function_name: String,
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub kind: String,                // "RETRY_LOOP" or "RESUME_OFFSET"
    pub evidence: serde_json::Value, // Flexible evidence structure
}

impl BehavioralMarkerResponse {
    /// Render human-readable output.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // Header
        out.push_str("Policy Facts: BEHAVIORAL_MARKER\n\n");

        // Count
        let word = if self.count == 1 { "fact" } else { "facts" };
        out.push_str(&format!("{} {}\n", self.count, word));

        if self.count == 0 {
            out.push_str("\nNo behavioral markers detected.\n");
            out.push_str(
                "\nhint: BEHAVIORAL_MARKER extracts retry loops and resume-offset patterns\n",
            );
            out.push_str("      from C code.\n");
            return out;
        }

        // Group by marker kind
        let mut by_kind: BTreeMap<&str, usize> = BTreeMap::new();
        for fact in &self.facts {
            *by_kind.entry(&fact.kind).or_insert(0) += 1;
        }

        out.push_str("\nBy kind:\n");
        for (kind, count) in &by_kind {
            out.push_str(&format!("  {}  {}\n", kind, count));
        }

        // Facts sorted by file, then line
        out.push('\n');
        let mut facts = self.facts.clone();
        facts.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.line_start.cmp(&b.line_start))
        });

        for fact in &facts {
            out.push_str(&format!(
                "{}  {}:{}-{}  {}\n",
                fact.function_name, fact.file_path, fact.line_start, fact.line_end, fact.kind
            ));

            // Show key evidence details based on kind
            if let Some(evidence_type) = fact.evidence.get("type").and_then(|v| v.as_str()) {
                match evidence_type {
                    "retry_loop" => {
                        if let Some(loop_kind) =
                            fact.evidence.get("loop_kind").and_then(|v| v.as_str())
                        {
                            out.push_str(&format!("  loop: {}\n", loop_kind));
                        }
                        if let Some(sleep_call) =
                            fact.evidence.get("sleep_call").and_then(|v| v.as_str())
                        {
                            out.push_str(&format!("  sleep: {}\n", sleep_call));
                        }
                        if let Some(delay_ms) =
                            fact.evidence.get("delay_ms").and_then(|v| v.as_u64())
                        {
                            out.push_str(&format!("  delay: {}ms\n", delay_ms));
                        }
                    }
                    "resume_offset" => {
                        if let Some(api_call) =
                            fact.evidence.get("api_call").and_then(|v| v.as_str())
                        {
                            out.push_str(&format!("  api: {}\n", api_call));
                        }
                        if let Some(option) =
                            fact.evidence.get("option_name").and_then(|v| v.as_str())
                        {
                            out.push_str(&format!("  option: {}\n", option));
                        }
                    }
                    _ => {}
                }
            }
        }

        out
    }
}

// ── RETURN_FATE ──────────────────────────────────────────────────────────────

/// Response DTO for `policy --kind RETURN_FATE`.
#[derive(Debug, Deserialize)]
pub struct ReturnFateResponse {
    pub repo: String,
    pub snapshot: String,
    pub kind: String,
    pub facts: Vec<ReturnFateFact>,
    pub count: usize,
    pub summary: ReturnFateSummary,
}

/// Summary for RETURN_FATE.
#[derive(Debug, Deserialize)]
pub struct ReturnFateSummary {
    pub by_fate: BTreeMap<String, usize>,
}

/// Individual RETURN_FATE fact.
#[derive(Debug, Deserialize, Clone)]
pub struct ReturnFateFact {
    pub callee_name: String,
    #[allow(dead_code)]
    pub caller_key: String,
    pub caller_name: String,
    pub file_path: String,
    pub line: u32,
    #[allow(dead_code)]
    pub column: u32,
    pub fate: String,
    pub evidence: serde_json::Value,
}

impl ReturnFateResponse {
    /// Render human-readable output.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // Header
        out.push_str("Policy Facts: RETURN_FATE\n\n");

        // Count
        let word = if self.count == 1 { "fact" } else { "facts" };
        out.push_str(&format!("{} {}\n", self.count, word));

        if self.count == 0 {
            out.push_str("\nNo return fate facts detected.\n");
            out.push_str("\nhint: RETURN_FATE classifies what happens to function return values\n");
            out.push_str("      at each call site (IGNORED, CHECKED, PROPAGATED, etc.).\n");
            return out;
        }

        // Summary by fate
        if !self.summary.by_fate.is_empty() {
            out.push_str("\nBy fate:\n");
            // Sort by count desc, then fate name asc
            let mut by_fate: Vec<_> = self.summary.by_fate.iter().collect();
            by_fate.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
            for (fate, count) in by_fate {
                out.push_str(&format!("  {}  {}\n", fate, count));
            }
        }

        // Facts sorted by file, then line
        out.push('\n');
        let mut facts = self.facts.clone();
        facts.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.line.cmp(&b.line))
        });

        for fact in &facts {
            out.push_str(&format!(
                "{}:{}  {} -> {}  {}\n",
                fact.file_path, fact.line, fact.callee_name, fact.caller_name, fact.fate
            ));
        }

        out
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── STATUS_MAPPING tests ─────────────────────────────────────────────────

    #[test]
    fn status_mapping_render_shows_header() {
        let resp = StatusMappingResponse {
            repo: "repo_test".to_string(),
            snapshot: "snapshot".to_string(),
            kind: "STATUS_MAPPING".to_string(),
            facts: vec![],
            count: 0,
        };
        let out = resp.render_human();
        assert!(out.starts_with("Policy Facts: STATUS_MAPPING"));
    }

    #[test]
    fn status_mapping_render_empty_shows_hint() {
        let resp = StatusMappingResponse {
            repo: "repo_test".to_string(),
            snapshot: "snapshot".to_string(),
            kind: "STATUS_MAPPING".to_string(),
            facts: vec![],
            count: 0,
        };
        let out = resp.render_human();
        assert!(out.contains("0 facts"));
        assert!(out.contains("No status mapping functions detected"));
        assert!(out.contains("hint:"));
    }

    #[test]
    fn status_mapping_render_shows_facts() {
        let resp = StatusMappingResponse {
            repo: "repo_test".to_string(),
            snapshot: "snapshot".to_string(),
            kind: "STATUS_MAPPING".to_string(),
            facts: vec![StatusMappingFact {
                symbol_key: "key".to_string(),
                function_name: "translate_error".to_string(),
                file_path: "src/error.c".to_string(),
                line_start: 100,
                line_end: 150,
                source_type: "sys_error_t".to_string(),
                target_type: "app_error_t".to_string(),
                mappings: vec![
                    CaseMapping {
                        inputs: vec!["SYS_OK".to_string()],
                        output: "APP_OK".to_string(),
                    },
                    CaseMapping {
                        inputs: vec!["SYS_EINVAL".to_string(), "SYS_ENOENT".to_string()],
                        output: "APP_BAD_INPUT".to_string(),
                    },
                ],
                default_output: Some("APP_UNKNOWN".to_string()),
            }],
            count: 1,
        };
        let out = resp.render_human();
        assert!(out.contains("1 fact"));
        assert!(out.contains("translate_error  src/error.c:100-150"));
        assert!(out.contains("sys_error_t -> app_error_t"));
        assert!(out.contains("SYS_OK -> APP_OK"));
        assert!(out.contains("SYS_EINVAL, SYS_ENOENT -> APP_BAD_INPUT"));
        assert!(out.contains("default -> APP_UNKNOWN"));
    }

    #[test]
    fn status_mapping_render_sorted_by_file_line() {
        let resp = StatusMappingResponse {
            repo: "repo_test".to_string(),
            snapshot: "snapshot".to_string(),
            kind: "STATUS_MAPPING".to_string(),
            facts: vec![
                StatusMappingFact {
                    symbol_key: "k1".to_string(),
                    function_name: "func_b".to_string(),
                    file_path: "src/z.c".to_string(),
                    line_start: 10,
                    line_end: 20,
                    source_type: "t1".to_string(),
                    target_type: "t2".to_string(),
                    mappings: vec![],
                    default_output: None,
                },
                StatusMappingFact {
                    symbol_key: "k2".to_string(),
                    function_name: "func_a".to_string(),
                    file_path: "src/a.c".to_string(),
                    line_start: 50,
                    line_end: 60,
                    source_type: "t1".to_string(),
                    target_type: "t2".to_string(),
                    mappings: vec![],
                    default_output: None,
                },
            ],
            count: 2,
        };
        let out = resp.render_human();
        let pos_a = out.find("func_a  src/a.c").unwrap();
        let pos_b = out.find("func_b  src/z.c").unwrap();
        assert!(pos_a < pos_b, "should be sorted by file path");
    }

    // ── BEHAVIORAL_MARKER tests ──────────────────────────────────────────────

    #[test]
    fn behavioral_marker_render_shows_header() {
        let resp = BehavioralMarkerResponse {
            repo: "repo_test".to_string(),
            snapshot: "snapshot".to_string(),
            kind: "BEHAVIORAL_MARKER".to_string(),
            facts: vec![],
            count: 0,
        };
        let out = resp.render_human();
        assert!(out.starts_with("Policy Facts: BEHAVIORAL_MARKER"));
    }

    #[test]
    fn behavioral_marker_render_empty_shows_hint() {
        let resp = BehavioralMarkerResponse {
            repo: "repo_test".to_string(),
            snapshot: "snapshot".to_string(),
            kind: "BEHAVIORAL_MARKER".to_string(),
            facts: vec![],
            count: 0,
        };
        let out = resp.render_human();
        assert!(out.contains("0 facts"));
        assert!(out.contains("No behavioral markers detected"));
        assert!(out.contains("hint:"));
    }

    #[test]
    fn behavioral_marker_render_shows_facts() {
        let resp = BehavioralMarkerResponse {
            repo: "repo_test".to_string(),
            snapshot: "snapshot".to_string(),
            kind: "BEHAVIORAL_MARKER".to_string(),
            facts: vec![BehavioralMarkerFact {
                symbol_key: "key".to_string(),
                function_name: "retry_connect".to_string(),
                file_path: "src/net.c".to_string(),
                line_start: 200,
                line_end: 220,
                kind: "RETRY_LOOP".to_string(),
                evidence: serde_json::json!({
                    "type": "retry_loop",
                    "loop_kind": "while",
                    "sleep_call": "sleep",
                    "delay_ms": 1000
                }),
            }],
            count: 1,
        };
        let out = resp.render_human();
        assert!(out.contains("1 fact"));
        assert!(out.contains("By kind:"));
        assert!(out.contains("RETRY_LOOP  1"));
        assert!(out.contains("retry_connect  src/net.c:200-220  RETRY_LOOP"));
        assert!(out.contains("loop: while"));
        assert!(out.contains("sleep: sleep"));
        assert!(out.contains("delay: 1000ms"));
    }

    // ── RETURN_FATE tests ────────────────────────────────────────────────────

    #[test]
    fn return_fate_render_shows_header() {
        let resp = ReturnFateResponse {
            repo: "repo_test".to_string(),
            snapshot: "snapshot".to_string(),
            kind: "RETURN_FATE".to_string(),
            facts: vec![],
            count: 0,
            summary: ReturnFateSummary {
                by_fate: BTreeMap::new(),
            },
        };
        let out = resp.render_human();
        assert!(out.starts_with("Policy Facts: RETURN_FATE"));
    }

    #[test]
    fn return_fate_render_empty_shows_hint() {
        let resp = ReturnFateResponse {
            repo: "repo_test".to_string(),
            snapshot: "snapshot".to_string(),
            kind: "RETURN_FATE".to_string(),
            facts: vec![],
            count: 0,
            summary: ReturnFateSummary {
                by_fate: BTreeMap::new(),
            },
        };
        let out = resp.render_human();
        assert!(out.contains("0 facts"));
        assert!(out.contains("No return fate facts detected"));
        assert!(out.contains("hint:"));
    }

    #[test]
    fn return_fate_render_shows_summary() {
        let mut by_fate = BTreeMap::new();
        by_fate.insert("IGNORED".to_string(), 5);
        by_fate.insert("CHECKED".to_string(), 3);
        by_fate.insert("PROPAGATED".to_string(), 2);

        let resp = ReturnFateResponse {
            repo: "repo_test".to_string(),
            snapshot: "snapshot".to_string(),
            kind: "RETURN_FATE".to_string(),
            facts: vec![],
            count: 10,
            summary: ReturnFateSummary { by_fate },
        };
        let out = resp.render_human();
        assert!(out.contains("By fate:"));
        assert!(out.contains("IGNORED  5"));
        assert!(out.contains("CHECKED  3"));
        assert!(out.contains("PROPAGATED  2"));
    }

    #[test]
    fn return_fate_render_shows_facts() {
        let mut by_fate = BTreeMap::new();
        by_fate.insert("IGNORED".to_string(), 1);

        let resp = ReturnFateResponse {
            repo: "repo_test".to_string(),
            snapshot: "snapshot".to_string(),
            kind: "RETURN_FATE".to_string(),
            facts: vec![ReturnFateFact {
                callee_name: "open".to_string(),
                caller_key: "key".to_string(),
                caller_name: "init".to_string(),
                file_path: "src/main.c".to_string(),
                line: 42,
                column: 5,
                fate: "IGNORED".to_string(),
                evidence: serde_json::json!({"type": "ignored"}),
            }],
            count: 1,
            summary: ReturnFateSummary { by_fate },
        };
        let out = resp.render_human();
        assert!(out.contains("1 fact"));
        assert!(out.contains("src/main.c:42  open -> init  IGNORED"));
    }

    #[test]
    fn return_fate_render_sorted_by_file_line() {
        let mut by_fate = BTreeMap::new();
        by_fate.insert("IGNORED".to_string(), 2);

        let resp = ReturnFateResponse {
            repo: "repo_test".to_string(),
            snapshot: "snapshot".to_string(),
            kind: "RETURN_FATE".to_string(),
            facts: vec![
                ReturnFateFact {
                    callee_name: "b".to_string(),
                    caller_key: "k".to_string(),
                    caller_name: "caller".to_string(),
                    file_path: "z.c".to_string(),
                    line: 10,
                    column: 0,
                    fate: "IGNORED".to_string(),
                    evidence: serde_json::json!({}),
                },
                ReturnFateFact {
                    callee_name: "a".to_string(),
                    caller_key: "k".to_string(),
                    caller_name: "caller".to_string(),
                    file_path: "a.c".to_string(),
                    line: 50,
                    column: 0,
                    fate: "IGNORED".to_string(),
                    evidence: serde_json::json!({}),
                },
            ],
            count: 2,
            summary: ReturnFateSummary { by_fate },
        };
        let out = resp.render_human();
        let pos_a = out.find("a.c:50").unwrap();
        let pos_z = out.find("z.c:10").unwrap();
        assert!(pos_a < pos_z, "should be sorted by file path");
    }
}
