//! Presentation layer for documentation commands.
//!
//! # CLI-OUT-5 Group 1
//!
//! Two commands, two response shapes:
//! - `docs list`: inventory of known documentation files
//! - `docs extract`: operation summary for semantic fact extraction
//!
//! Both share documentation vocabulary but have different payloads:
//! - list is inventory (array of entries)
//! - extract is operation summary (counts + warnings)
//!
//! # Output Contract
//!
//! - Deterministic ordering (by path for list)
//! - Full output, no truncation
//! - `--json` preserved for machine mode

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ── docs list ────────────────────────────────────────────────────────────────

/// Response DTO for `docs list`.
#[derive(Debug, Deserialize)]
pub struct DocsListResponse {
    pub command: String,
    pub repo: String,
    pub repo_path: String,
    pub entries: Vec<DocEntry>,
    pub count: usize,
    pub counts_by_kind: BTreeMap<String, usize>,
    pub generated_count: usize,
    /// Sidecar-named files the daemon could not read to check the `rmap map` marker
    /// (UNKNOWN — admitted but not asserted authored; operator RULING 3). The daemon
    /// emits this key ONLY when > 0, so `#[serde(default)]` keeps older/clean payloads
    /// (no key) parsing as 0 — and keeps `docs list --json` byte-identical to pre-slice
    /// when nothing is unreadable (review-5 finding 1).
    #[serde(default)]
    pub unreadable: usize,
}

/// Individual documentation entry.
///
/// `Serialize` is derived so the filtered `--json` view (`filtered_json_view`) can
/// re-emit the VISIBLE subset with the same field shape the daemon produced.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DocEntry {
    pub path: String,
    pub kind: String,
    pub generated: bool,
    pub content_hash: String,
}

impl DocsListResponse {
    /// The default-exclusion VIEW shared by human rendering and `--json` (§2.3):
    /// the entries actually listed — sorted by path, with rmap's OWN generated
    /// maps dropped unless `include_generated` — and the count that was excluded.
    ///
    /// ONE computation decides "what the listing is" for BOTH surfaces so they can
    /// never diverge on it (the exact class of drift SELF-POLLUTION-1 fixes). The
    /// excluded count is derived from the entries themselves (never the daemon's
    /// total blindly), so it is exact for the set actually rendered.
    fn visible_view(&self, include_generated: bool) -> (Vec<DocEntry>, usize) {
        let mut entries = self.entries.clone();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        let excluded_generated = if include_generated {
            0
        } else {
            entries.iter().filter(|e| e.generated).count()
        };
        let visible: Vec<DocEntry> = entries
            .into_iter()
            .filter(|e| include_generated || !e.generated)
            .collect();
        (visible, excluded_generated)
    }

    /// The machine-readable (`--json`) view, OR `None` when nothing is filtered.
    ///
    /// SELF-POLLUTION-1 §2.3 + review-5 finding 1 (byte-parity): when there is nothing
    /// for the slice to exclude — `--include-generated`, or no generated maps present —
    /// this returns `None` and the caller prints the RAW daemon value UNCHANGED, so
    /// `docs list --json` stays byte-identical to the pre-slice output on any repo with
    /// no rmap exhaust. Only in the AFFECTED case (generated maps actually dropped) does
    /// it build the filtered view: rmap's own maps removed, `excluded_generated` stating
    /// how many (always > 0 here, so the field is present exactly when it is meaningful),
    /// and `unreadable` when > 0 (UNKNOWN sidecars, surfaced not hidden — operator
    /// RULING 3). `count`/`counts_by_kind`/`generated_count` reflect the VISIBLE set,
    /// parallel to the human render.
    pub fn filtered_json_view(&self, include_generated: bool) -> Option<serde_json::Value> {
        let excluded_generated = if include_generated {
            0
        } else {
            self.entries.iter().filter(|e| e.generated).count()
        };
        if excluded_generated == 0 {
            // Nothing excluded → raw passthrough (byte-parity). The daemon payload
            // already carries `unreadable` when > 0, so that honesty is preserved too.
            return None;
        }

        // Affected case (include_generated is necessarily false here): drop rmap's maps.
        let mut visible = self.entries.clone();
        visible.sort_by(|a, b| a.path.cmp(&b.path));
        visible.retain(|e| !e.generated);

        let mut counts_by_kind: BTreeMap<String, usize> = BTreeMap::new();
        for e in &visible {
            *counts_by_kind.entry(e.kind.clone()).or_insert(0) += 1;
        }

        let mut view = serde_json::json!({
            "command": self.command,
            "repo": self.repo,
            "repo_path": self.repo_path,
            "entries": visible,
            "count": visible.len(),
            "counts_by_kind": counts_by_kind,
            "generated_count": 0,
            "excluded_generated": excluded_generated,
        });
        if self.unreadable > 0 {
            view["unreadable"] = serde_json::json!(self.unreadable);
        }
        Some(view)
    }

    /// Render human-readable output.
    ///
    /// SELF-POLLUTION-1 §3: rmap's OWN `map` sidecars (the `generated` entries) are
    /// EXCLUDED from the listing by default so `docs list` shows the reader's docs,
    /// not rmap's exhaust. `include_generated` (the `--include-generated` flag) opts
    /// them back in; either way the excluded count is stated so nothing is silently
    /// hidden. `--json` applies the SAME filter (see `filtered_json_view`).
    pub fn render_human(&self, include_generated: bool) -> String {
        let mut out = String::new();

        // Header
        out.push_str("Documentation\n\n");

        let (visible, excluded_generated) = self.visible_view(include_generated);

        // Count line (visible set).
        let doc_word = if visible.len() == 1 {
            "document"
        } else {
            "documents"
        };
        out.push_str(&format!("{} {}\n", visible.len(), doc_word));

        // What we ignored, said out loud (honesty — never silently hidden).
        if excluded_generated > 0 {
            let map_word = if excluded_generated == 1 {
                "map"
            } else {
                "maps"
            };
            out.push_str(&format!(
                "{} generated {} excluded (rmap's own; use --include-generated to show)\n",
                excluded_generated, map_word
            ));
        }

        // Sidecar-named files the daemon could not read to check the marker: admitted
        // (shown, conservative) but UNKNOWN — said out loud, never silently asserted
        // authored (operator RULING 3). Surfaced regardless of the exclusion above.
        if self.unreadable > 0 {
            out.push_str(&format!(
                "+{} unreadable, counted (sidecar-named; rmap marker unverifiable)\n",
                self.unreadable
            ));
        }

        if visible.is_empty() {
            if excluded_generated > 0 {
                // Docs DO exist — they are all rmap's own maps, now hidden. Do NOT
                // claim "no documentation": that would misrepresent the repo.
                out.push_str(
                    "\nhint: all documentation here is rmap-generated; \
                     use --include-generated to list it.\n",
                );
            } else {
                out.push_str("\nhint: no documentation files detected in this repository.\n");
            }
            return out;
        }

        // By kind breakdown over the VISIBLE set (sorted by count desc, then kind asc).
        let mut counts_by_kind: BTreeMap<&str, usize> = BTreeMap::new();
        for e in &visible {
            *counts_by_kind.entry(e.kind.as_str()).or_insert(0) += 1;
        }
        if !counts_by_kind.is_empty() {
            out.push_str("\nBy kind:\n");
            let mut by_kind: Vec<_> = counts_by_kind.iter().collect();
            by_kind.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
            for (kind, count) in by_kind {
                out.push_str(&format!("  {}  {}\n", kind, count));
            }
        }

        // Generated count among the VISIBLE set (only when opted in — else 0).
        let visible_generated = visible.iter().filter(|e| e.generated).count();
        if visible_generated > 0 {
            out.push_str(&format!("\n{} generated\n", visible_generated));
        }

        // Entry list.
        out.push('\n');
        for entry in &visible {
            let generated_marker = if entry.generated { "  [generated]" } else { "" };
            out.push_str(&format!(
                "  {}  {}{}\n",
                entry.path, entry.kind, generated_marker
            ));
        }

        // Hint
        out.push_str("\nhint: run 'rmap docs extract' to scan for explicit rg: markers and config patterns.\n");

        out
    }
}

// ── docs extract ─────────────────────────────────────────────────────────────

/// Response DTO for `docs extract`.
#[derive(Debug, Deserialize)]
pub struct DocsExtractResponse {
    pub command: String,
    pub repo: String,
    pub repo_path: String,
    pub files_scanned: usize,
    pub files_by_kind: BTreeMap<String, usize>,
    pub facts_extracted: usize,
    pub facts_inserted: usize,
    pub facts_deleted: usize,
    pub counts_by_kind: BTreeMap<String, usize>,
    pub generated_docs_count: usize,
    pub warnings: Vec<String>,
}

impl DocsExtractResponse {
    /// Render human-readable output.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // Header
        out.push_str("Documentation Extraction\n\n");

        // Files scanned
        let file_word = if self.files_scanned == 1 {
            "file"
        } else {
            "files"
        };
        out.push_str(&format!("{} {} scanned\n", self.files_scanned, file_word));

        // Files by kind (sorted by count desc, then kind asc)
        if !self.files_by_kind.is_empty() {
            out.push_str("\nBy kind:\n");
            let mut by_kind: Vec<_> = self.files_by_kind.iter().collect();
            by_kind.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
            for (kind, count) in by_kind {
                out.push_str(&format!("  {}  {}\n", kind, count));
            }
        }

        // Extraction results
        out.push_str("\nExtraction results:\n");
        out.push_str(&format!("  {} facts extracted\n", self.facts_extracted));
        out.push_str(&format!("  {} facts inserted\n", self.facts_inserted));
        out.push_str(&format!("  {} facts deleted\n", self.facts_deleted));
        out.push_str(&format!("  {} generated docs\n", self.generated_docs_count));

        // Facts by kind if any
        if !self.counts_by_kind.is_empty() {
            out.push_str("\nFacts by kind:\n");
            let mut by_kind: Vec<_> = self.counts_by_kind.iter().collect();
            by_kind.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
            for (kind, count) in by_kind {
                out.push_str(&format!("  {}  {}\n", kind, count));
            }
        }

        // Warnings
        if self.warnings.is_empty() {
            out.push_str("\nNo warnings.\n");
        } else {
            out.push_str(&format!("\n{} warnings:\n", self.warnings.len()));
            for warning in &self.warnings {
                out.push_str(&format!("  - {}\n", warning));
            }
        }

        out
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "docs_tests.rs"]
mod tests;
