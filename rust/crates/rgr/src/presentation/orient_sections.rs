//! Dense-headline + depth section renderers for the `orient` command (ORIENT-DENSITY-1).
//!
//! Split out of `orient.rs` to keep each module under the 500-line structural
//! guardrail — the `explain.rs` / `explain_sections.rs` idiom. This is a SECOND
//! `impl OrientResponse` block (inherent impls may span modules within the crate):
//! `orient.rs` keeps the `OrientDepth` map, the `render_orient_envelope` wrapper, and
//! the `render_human` orchestrator (the response DTOs live in `orient_types`); the
//! per-section renderers it calls live here.
//!
//! Visibility: methods `render_human` invokes are `pub(super)`; their internal
//! helpers stay private. Pure relocation — no behavior changed.

use super::orient::{OrientDepth, OrientResponse};
use super::{anchor, bullet, heading, sub_heading};

/// Plural suffix helper (`""` for 1, `"s"` otherwise). `pub(super)` so the sibling
/// `orient_seg2` section renderers (split out under the 500-line guardrail) share it.
pub(super) fn plural(n: u64) -> &'static str {
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
    /// The MODULE_SUMMARY signal's evidence payload, if present. `pub(super)` so the
    /// sibling `orient_seg2` renderers share it (guardrail split).
    pub(super) fn module_summary_evidence(&self) -> Option<&serde_json::Value> {
        self.signal_evidence("MODULE_SUMMARY")
    }

    /// The evidence payload of the first signal with `code`, if present. `pub(super)`
    /// so the sibling `orient_seg2` renderers share it (guardrail split).
    pub(super) fn signal_evidence(&self, code: &str) -> Option<&serde_json::Value> {
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
            // ANCHORS-EVERYWHERE-1 (Tier 1): anchor the file at the symbol's start line
            // (`file:line`) on this SYMBOL-level row — the line shares the SQLite `nodes`
            // row with `file`. Absent line → bare path (never a fabricated 0/1). The
            // file-deduped headline (`complexity_line`) stays unanchored: a file rollup
            // spans many symbols and has no single line.
            let line_no = entry.get("line").and_then(|v| v.as_u64());
            let row = match (file, symbol) {
                (Some(f), Some(s)) => format!("{} — {s} (cx {cx})", anchor(f, line_no)),
                (Some(f), None) => format!("{} (cx {cx})", anchor(f, line_no)),
                (None, Some(s)) => format!("{s} (cx {cx})"),
                (None, None) => continue,
            };
            out.push_str(&bullet(&row));
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
                let raw_total = ev
                    .get("cycle_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(cycles.len() as u64);
                // ORIENT-CYCLES-DISAGREE-1: PREFER the exclusion-aware `production_count` — the
                // SAME integer `cycles` renders as "N module-level cycle(s) found" (both derive
                // from the shared classifier over the same cycle set). It is present ONLY when
                // the serving computation reached the stored `is_test` fact (the SQLite path);
                // ABSENT (LiveGraph/focus path) ⇒ fall back to the raw total, exactly as `cycles`
                // does there. `production_count` is optional — its absence means "not split",
                // NEVER "zero production cycles" (Fact Certainty Model).
                let production = ev.get("production_count").and_then(|v| v.as_u64());
                let test_only = ev.get("test_only_count").and_then(|v| v.as_u64());
                // ORIENT-CYCLES-DISAGREE-1 (operator ruling review-3 #2): the unknown subset kept in
                // `headline` (never demoted), disclosed so it is not counted invisibly. Present only
                // with the split (SQLite path); absent ⇒ no clause.
                let unknown = ev.get("unknown_count").and_then(|v| v.as_u64());
                let headline = production.unwrap_or(raw_total);
                // review-6 #1: a zero production headline must STILL render when a split
                // disclosure exists (all-test-only cycles: cycles says "0 … (+N test-only
                // excluded)" — orient may not silently drop the same fact). Suppress only
                // the truly-empty case: no cycles AND nothing disclosed.
                let has_disclosure =
                    matches!(test_only, Some(n) if n > 0) || matches!(unknown, Some(n) if n > 0);
                if headline > 0 || has_disclosure {
                    // Anchor honesty: `cycles[]` carries no per-cycle composition, so with a split
                    // its first entry may be a demoted test-only/unknown ring — misrepresenting the
                    // example beside a production headline. Draw ONLY when there is no split
                    // (`production` absent = unsplit LiveGraph/focus path) OR the split POSITIVELY
                    // confirms zero test-only AND zero unknown; explicit `Option` match (review-4 #4:
                    // never `unwrap_or(0)`, which reads an ABSENT count as known-zero — RULE #1).
                    let draw_anchor = match production {
                        None => true,
                        Some(_) => test_only == Some(0) && unknown == Some(0),
                    };
                    let anchor = if draw_anchor {
                        cycles
                            .first()
                            .and_then(|c| c.get("modules").and_then(|m| m.as_array()))
                            .map(|mods| self.format_cycle_anchor(mods, 0))
                    } else {
                        None
                    };
                    let mut line = match anchor {
                        Some(a) => format!("{} import cycle{} ({})", headline, plural(headline), a),
                        None => format!("{} import cycle{}", headline, plural(headline)),
                    };
                    // Mirror `cycles`' "+M test-only (excluded)" AND unknown disclosures so the two
                    // surfaces tell ONE story about the same snapshot — via the SHARED clause helper
                    // (review-4 #3), so the wording cannot drift between the two headlines.
                    if let Some(clause) =
                        crate::presentation::cycle_exclusion_clause(test_only, unknown)
                    {
                        line.push_str(&format!(" ({clause})"));
                    }
                    // COHERENCE-2 §2.2: render the SAME type-only verdict label `cycles` renders,
                    // for the anchor cycle we are showing as the example — via the SHARED
                    // `cycles::type_only_label`, so the two surfaces render the state IDENTICALLY
                    // (seam-pinned). Only when we drew the anchor (we are pointing at a specific
                    // cycle, `cycles.first()`); the per-cycle verdict for every cycle also rides in
                    // the JSON leaf. ABSENCE of the field is legitimate (LiveGraph route / non-TS §5
                    // cycle) ⇒ no label — never a false claim.
                    //
                    // A PRESENT-but-unparseable verdict is producer/mirror schema drift. Unlike the
                    // `cycles` renderer — where `type_only` is a TYPED field decoded at the transport
                    // boundary, so a mismatch fails the whole response parse into a handled `Result`
                    // (`error: failed to parse …`) — orient's evidence is an already-decoded raw
                    // `Value`, so the mismatch surfaces only HERE, at render time. It must NOT
                    // `.expect()`-panic the CLI on recoverable wire input (review-0 #1): per RULE #1
                    // the unknown is made VISIBLE with its reason (never `.ok()`-swallowed), rendered
                    // as an explicit type-only-unavailable clause the reader can act on.
                    if draw_anchor {
                        if let Some(tv) = cycles.first().and_then(|c| c.get("type_only")) {
                            match serde_json::from_value::<super::cycles::CycleTypeOnly>(tv.clone()) {
                                Ok(verdict) => {
                                    if let Some(label) = super::cycles::type_only_label(&verdict) {
                                        line.push_str(&format!(" — {label}"));
                                    }
                                }
                                Err(_) => line.push_str(
                                    " — `import type` status unavailable (cycle verdict unreadable \
                                     on this snapshot — run `rmap cycles` for the per-cycle detail)",
                                ),
                            }
                        }
                    }
                    parts.push(line);
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

    /// Documentation file basenames (relevant docs), order preserved (the agent
    /// already ranks README / architecture first, then by structural relevance —
    /// never alphabetical). ORIENT-SEGMENT-2 §2.6: the headline Docs line is CAPPED
    /// at the most-orienting [`DOC_HEADLINE_CAP`] so a repo with hundreds of docs
    /// does not dump them into the headline; the `.env*` / self-generated exhaust is
    /// already excluded upstream (SELF-POLLUTION-1's classifier, consumed by the
    /// agent's documentation section). The cap is a PRESENTATION bound — the full set
    /// stays on `documentation` in the JSON, and `rmap docs` lists it whole.
    fn doc_basenames(&self) -> Vec<String> {
        let Some(docs) = &self.documentation else {
            return Vec::new();
        };
        docs.relevant_files
            .iter()
            .take(super::orient_seg2::DOC_HEADLINE_CAP)
            .map(|d| module_short_name(&d.path).to_string())
            .collect()
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
        // `None` is the "uncapped" sentinel this renderer honors (`unwrap_or(total)`
        // renders every group). ORIENT-SEGMENT-2 §2.4 (operator ruling 2, 2026-08-28):
        // `medium` caps at 20, `large` at 50, and `--full` now passes `None` — the
        // COMPLETE breakdown the small-budget footer advertises, so a >50-group repo's
        // `--full` renders EVERY group (no elision). `small` also produces `None` but
        // never reaches this renderer (`shows_detail()` gates the section off). `total`
        // is the COMPLETE set — the fold never caps and the JSON carries it whole — so
        // the omission count on the capped tiers is always TRUE at scale.
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
        let mut malformed_groups = 0usize;
        for g in groups.iter().take(limit) {
            // review-5 #1: a row missing its name or file_count is MALFORMED — counted
            // and reported below, never rendered as a fabricated "(unknown) — 0 files".
            let (Some(name), Some(files)) = (
                g.get("name").and_then(|p| p.as_str()),
                g.get("file_count").and_then(|v| v.as_u64()),
            ) else {
                malformed_groups += 1;
                continue;
            };
            // review-2 (COHERENCE-2 iter 2): `test_file_count` is a REQUIRED field in
            // the producer contract — `agent::PackageGroupEvidence.test_file_count: u64`
            // is `#[derive(Serialize)]` with no `skip_serializing_if`, and BOTH producers
            // (the daemon LiveGraph route `dispatch.rs` `json!{…,"test_file_count":…}` and
            // the agent aggregator `module_summary`) emit it unconditionally, even for a
            // zero-test group. So an ABSENT or non-u64 value is schema drift, NOT "no test
            // files recorded". The prior `.unwrap_or(0)` rendered that drift as a silent
            // no-suffix — a false Layer-0 "zero tests" claim (STANDING HONESTY RULE #1).
            // Make the unknown VISIBLE with its reason; a truthful `0` stays suffix-less.
            // The completeness gate (`package_group_row_wellformed`) also refuses to stamp
            // "complete" for such a row, so the two surfaces agree on the degradation.
            let test_suffix = match g.get("test_file_count").and_then(|v| v.as_u64()) {
                Some(0) => String::new(),
                Some(test) => format!(" ({test} test)"),
                None => " (test count unavailable)".to_string(),
            };
            out.push_str(&bullet(&format!(
                "{} — {} file{}{}",
                name,
                files,
                plural(files),
                test_suffix
            )));
        }
        if malformed_groups > 0 {
            out.push_str(&sub_heading(&format!(
                "{malformed_groups} group row(s) malformed in this evidence — omitted (unknown, not zero)"
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

        // ORIENT-SEGMENT-2 §2.2: each row is self-describing — the agent carries the
        // declared `name` + owning `manifest` PER ROW (two modules can share a
        // `canonical_root_path`: django declares TWO `Django` modules both rooted at
        // `.`). Pre-compute paths, per-row names/manifests, and the effective display
        // name (declared name, else path) so a name COLLISION can be detected.
        let rows: Vec<super::module_disambiguation::ModuleRow> = modules
            .iter()
            .map(super::module_disambiguation::ModuleRow::from_json)
            .collect();
        let effective_names: Vec<&str> = rows.iter().map(|r| r.effective_name()).collect();

        let mut out = heading("Modules (declared/inferred, by size)");
        let mut malformed_modules = 0usize;
        for (i, m) in modules.iter().enumerate() {
            // review-5 #1: same rule as group rows — no fabricated 0-file modules.
            let Some(files) = m.get("file_count").and_then(|v| v.as_u64()) else {
                malformed_modules += 1;
                continue;
            };
            let label = OrientResponse::module_row_label(&rows, &effective_names, i);
            out.push_str(&bullet(&format!(
                "{} — {} file{}",
                label,
                files,
                plural(files)
            )));
        }
        if malformed_modules > 0 {
            out.push_str(&sub_heading(&format!(
                "{malformed_modules} module row(s) malformed in this evidence — omitted (unknown, not zero)"
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
}
