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
//! - **Human render is BOUNDED** (DOCS-LIST-2 §2–3): vendored docs demoted to a stated count line,
//!   release notes grouped one line per family, and ALL default inventory rows (the family group
//!   lines + the reader's individual docs) capped together at [`HUMAN_ROW_BUDGET`] with one truthful
//!   "(+N more — --full)" remainder. `--full` uncaps and lists every doc individually.
//! - **`--json` stays COMPLETE** — budgets/demotion/grouping are a human-render concern only; the
//!   machine view carries every entry (`filtered_json_view` only drops rmap's own generated maps,
//!   and states how many).

use super::{budget_remainder_line, HUMAN_ROW_BUDGET};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// DOCS-LIST-2 §2: the daemon-overlaid kind marking a doc as vendored dependency content (set in
/// `handle_docs_list` from the shared `is_vendored_path` fact). The presentation layer DEMOTES these
/// from the headline. Kept as a named constant so the daemon's write and this read agree on the token.
const VENDORED_KIND: &str = "vendored";
/// DOCS-LIST-2 §2: the location-classified kind for release/changelog docs — GROUPED one line per
/// family on the human render (django ships 190 under `docs/releases/`). Matches `DocKind::as_str`.
const RELEASE_NOTES_KIND: &str = "release-notes";

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
    /// DOCS-LIST-2 §2 (DOC_FACTS_PUBLIC_API review-0, Option B): the CONFIRMED release/changelog
    /// subtree the daemon attached, or `None` for a non-release doc. STRUCTURAL basis, not location:
    /// the subtree is confirmed only when its `index.{txt,rst,md}` manifest's INSPECTED CONTENT carries
    /// a Sphinx `toctree` directive (doc-facts' crate-private `release_notes` module — `is_manifest_
    /// index_content` + `release_subtree_of`, review-2 item 1). Set ONLY on `release-notes` entries;
    /// the daemon's vendored overlay CLEARS it when it demotes an entry to `vendored`, so a present
    /// value always means release-notes. Grouping reads THIS instead of re-deriving the subtree rule in
    /// `rgr`. `#[serde(default)]` keeps pre-slice / non-release payloads (no key) parsing as `None`, and
    /// `skip_serializing_if` keeps `filtered_json_view` byte-identical when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_family: Option<String>,
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
    /// SELF-POLLUTION-1 §3: rmap's OWN `map` sidecars (the `generated` entries) are EXCLUDED from the
    /// listing by default so `docs list` shows the reader's docs, not rmap's exhaust.
    /// `include_generated` (the `--include-generated` flag) opts them back in.
    ///
    /// DOCS-LIST-2 §2–3 layers three more HUMAN-render concerns on top (the daemon JSON stays
    /// COMPLETE — budgets/demotion/grouping are a human concern only):
    /// - **vendored demoted** — entries the daemon marked `vendored` (from the shared vendor fact) are
    ///   dropped from the headline + the listing, replaced by one honest "N vendored docs (excluded)"
    ///   line. Never silently hidden; the complete set rides `--json`.
    /// - **release-notes grouped** — `release-notes` entries collapse to ONE line per family subtree
    ///   ("<repo> release notes: N files under docs/releases/ — --full to list"), so django's 190
    ///   release files do not flood the surface. `--full` lists them individually.
    /// - **budget** — ALL default display rows (the release-family group lines AND the reader's
    ///   individual docs) cap TOGETHER at [`HUMAN_ROW_BUDGET`] with one truthful "(+N more — --full)"
    ///   remainder (review-2 item 2: many release families can never emit unbounded group lines);
    ///   `--full` uncaps and lists every doc individually.
    pub fn render_human(&self, include_generated: bool, full: bool) -> String {
        let mut out = String::new();

        // Header
        out.push_str("Documentation\n\n");

        let (visible, excluded_generated) = self.visible_view(include_generated);

        // DOCS-LIST-2 §2: partition the visible set — vendored (demoted from the headline),
        // release-notes (grouped), and the rest (the reader's docs the headline is for).
        let vendored: Vec<&DocEntry> = visible.iter().filter(|e| e.kind == VENDORED_KIND).collect();
        let release: Vec<&DocEntry> = visible
            .iter()
            .filter(|e| e.kind == RELEASE_NOTES_KIND)
            .collect();
        let others: Vec<&DocEntry> = visible
            .iter()
            .filter(|e| e.kind != VENDORED_KIND && e.kind != RELEASE_NOTES_KIND)
            .collect();

        // Headline count = the reader's docs (vendored excluded; release-notes counted, grouped).
        let headline_count = others.len() + release.len();
        let doc_word = if headline_count == 1 {
            "document"
        } else {
            "documents"
        };
        out.push_str(&format!("{} {}\n", headline_count, doc_word));

        // What we ignored, said out loud (honesty — never silently hidden).
        if excluded_generated > 0 {
            let map_word = if excluded_generated == 1 {
                "map"
            } else {
                "maps"
            };
            out.push_str(&format!(
                "{} generated {} excluded (tool-generated map summaries; use --include-generated to show)\n",
                excluded_generated, map_word
            ));
        }

        // DOCS-LIST-2 §2: vendored dependency docs demoted from the headline — stated, never hidden
        // (the complete set is in `--json`). Basis: the shared vendor-path fact (node_modules /
        // site-packages / vendor / …), applied by the daemon.
        if !vendored.is_empty() {
            let doc_word = if vendored.len() == 1 { "doc" } else { "docs" };
            // Contract form (DOCS-LIST-2 §2.2, review-0 F5): "+N vendored docs (excluded)".
            out.push_str(&format!(
                "+{} vendored {} (excluded; third-party/dependency directories — see --json for the full set)\n",
                vendored.len(),
                doc_word
            ));
        }

        // Sidecar-named files the daemon could not read to check the marker: admitted
        // (shown, conservative) but UNKNOWN — said out loud, never silently asserted
        // authored (operator RULING 3). Surfaced regardless of the exclusions above.
        if self.unreadable > 0 {
            out.push_str(&format!(
                "+{} unreadable, counted (content unreadable — kind refinement unverifiable)\n",
                self.unreadable
            ));
        }

        if headline_count == 0 {
            if excluded_generated > 0 {
                // Docs DO exist — they are all rmap's own maps, now hidden. Do NOT
                // claim "no documentation": that would misrepresent the repo.
                out.push_str(
                    "\nhint: all documentation here is tool-generated maps; \
                     use --include-generated to list it.\n",
                );
            } else if !vendored.is_empty() {
                // Docs exist but are all vendored dependency docs — do not claim "no documentation".
                out.push_str(
                    "\nhint: all documentation here is vendored dependency docs; \
                     see --json for the full set.\n",
                );
            } else {
                out.push_str("\nhint: no documentation files detected in this repository.\n");
            }
            return out;
        }

        // By kind breakdown over the reader's docs (vendored excluded — it has its own line above;
        // release-notes counted here so the reader sees the family exists).
        let mut counts_by_kind: BTreeMap<&str, usize> = BTreeMap::new();
        for e in others.iter().chain(release.iter()) {
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

        // Generated count among the reader's docs (only when opted in — else 0).
        let visible_generated = others.iter().filter(|e| e.generated).count();
        if visible_generated > 0 {
            out.push_str(&format!("\n{} generated\n", visible_generated));
        }

        // DOCS-LIST-2 §2–3: the default inventory listing. Under `--full` every doc is listed flat
        // (release notes folded in), uncapped. By default the display rows are (1) one grouped line
        // per release family — the family comes from the daemon-attached `release_family` field
        // (doc-facts' single manifest-confirmed source), so grouping and classification agree without
        // a cross-crate call — followed by (2) the reader's individual non-release docs. Review-2
        // item 2: ALL default rows (family lines + entries) are budgeted TOGETHER to
        // [`HUMAN_ROW_BUDGET`] with ONE truthful remainder, so a repo with many release families can
        // never emit unbounded group lines. `--full` renders the COMPLETE set; `--json` always does.
        out.push('\n');
        if full {
            // Every doc, listed individually (release notes are not grouped under --full), uncapped.
            let mut listed: Vec<&DocEntry> = others.iter().chain(release.iter()).copied().collect();
            listed.sort_by(|a, b| a.path.cmp(&b.path));
            for entry in &listed {
                let generated_marker = if entry.generated { "  [generated]" } else { "" };
                out.push_str(&format!(
                    "  {}  {}{}\n",
                    entry.path, entry.kind, generated_marker
                ));
            }
        } else {
            // Build ALL default display rows, then apply ONE combined budget. Release-family group
            // lines come first (each stands in for a whole family), then the reader's individual docs
            // sorted by path.
            let mut rows: Vec<String> = Vec::new();

            let mut by_family: BTreeMap<&str, usize> = BTreeMap::new();
            for e in &release {
                // A release-notes entry always carries its release subtree (that is why it is one);
                // fall back to the whole path only if the daemon omitted it (never silently mislabel —
                // show the path rather than an empty family).
                let family = e.release_family.as_deref().unwrap_or(e.path.as_str());
                *by_family.entry(family).or_insert(0) += 1;
            }
            for (family, n) in &by_family {
                let file_word = if *n == 1 { "file" } else { "files" };
                rows.push(format!(
                    "  {} release notes: {} {} under {}/ — --full to list\n",
                    self.repo, n, file_word, family
                ));
            }

            let mut others_sorted = others.clone();
            others_sorted.sort_by(|a, b| a.path.cmp(&b.path));
            for entry in &others_sorted {
                let generated_marker = if entry.generated { "  [generated]" } else { "" };
                rows.push(format!(
                    "  {}  {}{}\n",
                    entry.path, entry.kind, generated_marker
                ));
            }

            let shown = rows.len().min(HUMAN_ROW_BUDGET);
            for row in rows.iter().take(shown) {
                out.push_str(row);
            }
            if let Some(remainder) = budget_remainder_line(rows.len(), shown) {
                out.push_str(&remainder);
            }
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
