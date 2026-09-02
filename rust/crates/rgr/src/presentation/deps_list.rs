//! Presentation layer for `deps list` (DEPS-LIST-REWRITE-1 §2.5).
//!
//! Renders the daemon's dependency reconciliation as a ≤20-line, one-screen human table:
//! the unattributed headline FIRST (§2.3), then totals, then one row per manifest with the four
//! reconciled counts (declared+used / declared-unused / observed-undeclared / builtins) and top
//! examples. The `--json` path prints the daemon payload verbatim (same truth, additive) and does
//! not go through this renderer.
//!
//! This is a pure view over the JSON DTO — no daemon/storage/business logic. Deserialize is lenient
//! (`#[serde(default)]`) so a payload from a slightly older/newer daemon still renders.

use serde::Deserialize;

use super::deps_list_secondary::{render_other_ecosystem, OtherEcosystem};

/// One reconciled dependency entry (the machine detail; the table shows counts + examples).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DepEntry {
    #[serde(default)]
    pub package: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub import_count: u64,
}

/// One manifest's reconciliation summary.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DepModule {
    #[serde(default)]
    pub module: String,
    #[serde(default)]
    pub manifest_path: Option<String>,
    /// §2.2 unknown-with-reason note (e.g. "unavailable (indexed before provenance tracking)")
    /// present only when deps were parsed but the exact file could not be pinned.
    #[serde(default)]
    pub manifest_context: Option<String>,
    #[serde(default)]
    pub manifest_scope_available: bool,
    #[serde(default)]
    pub declared_and_used: u64,
    #[serde(default)]
    pub declared_but_unobserved: u64,
    #[serde(default)]
    pub observed_but_undeclared: u64,
    /// DEPS-SELF-1 (FINAL-POLISH-1 §2.2): observed specifiers equal to THIS repo's own parsed
    /// manifest package name — first-party self-references, excluded from `observed_but_undeclared`.
    /// Additive; an older daemon omits it (defaults to 0 → no `self` note, byte-identical output).
    #[serde(default)]
    pub first_party_self: u64,
    #[serde(default)]
    pub runtime_builtins: u64,
    /// External-looking specifiers with no manifest scope to classify against (none-detected).
    #[serde(default)]
    pub unknown_external_like: u64,
    #[serde(default)]
    pub entries: Vec<DepEntry>,
}

/// The `deps list` daemon response.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DepsListResponse {
    #[serde(default)]
    pub ecosystem: String,
    #[serde(default)]
    pub unattributed_external_imports: u64,
    #[serde(default)]
    pub unattributed_reason: String,
    /// §2.4 tri-state posture (operator ruling 3 item 1): "downgraded" | "clean" | "unknown".
    /// Empty (older daemon) falls back to the `resolution_downgraded` bool below.
    #[serde(default)]
    pub resolution_state: String,
    /// The specific reason when `resolution_state == "unknown"` (a failed trust-overlay read).
    #[serde(default)]
    pub resolution_note: String,
    #[serde(default)]
    pub resolution_downgraded: bool,
    #[serde(default)]
    pub total_external_imports: u64,
    #[serde(default)]
    pub rejected_non_specifier_total: u64,
    /// §2.2 / ruling-3 item-4 workspace coverage: manifests of this ecosystem PRESENT (scanned on
    /// disk) in total, and how many were attributed to a reconciled module. `present > attributed`
    /// = reported shortfall. Absent from the payload (both default 0) when the denominator is
    /// unknown (old snapshot / unreadable) — the shortfall line then does not render.
    #[serde(default)]
    pub manifests_present: u64,
    /// Parsed manifests of this ecosystem whose subtree contains ≥1 indexed file (govern indexed
    /// source). DEPS-ATTRIB-2 §2.3: computed from file containment, NOT module attribution.
    #[serde(default)]
    pub manifests_attributed: u64,
    /// DEPS-ATTRIB-2 §2.3 additive field: parsed manifests COMPUTED to govern ZERO indexed files —
    /// the ONLY count that may render as "govern no indexed source". Absent (default 0) on an older
    /// daemon; the remainder of `present - attributed` then renders as the honest "present, no
    /// dependency record" clause instead of a false excuse.
    #[serde(default)]
    pub manifests_no_indexed_source: u64,
    /// DEPS-ATTRIB-2 review-4 blocker 2 additive field: parsed manifests whose subtree contains ≥1
    /// INDEXED source file that no module owns — indexed source present, attribution absent. The
    /// §2.3 excuse "govern no indexed source" is FALSE for these (indexed source IS present), so they
    /// render their own honest clause, NEVER the excuse. Absent (default 0) on an older daemon / the
    /// all-attributed happy path.
    #[serde(default)]
    pub manifests_indexed_unattributed: u64,
    /// The total indexed source files under those `manifests_indexed_unattributed` (the "N files
    /// indexed, not attributed" count the honest §2.3 clause states). Absent (default 0) when there
    /// are none.
    #[serde(default)]
    pub manifests_indexed_unattributed_files: u64,
    /// DEPS-ATTRIB-2 review-0 item 1 / operator binding: present ONLY when the owned-files read that
    /// feeds the coverage split FAILED. When set, the coverage line renders unknown-with-reason
    /// instead of a computed split — never a silent omission, never a false 0.
    #[serde(default)]
    pub manifests_coverage_unavailable: String,
    /// DEPS-ATTRIB-2 §2.4 (ruling Option 2): the truth of every materially-present ecosystem OTHER
    /// than the rendered one, in the DEFAULT view — so a material ecosystem (glamCRM's Java half) is
    /// never silently absent. Empty on a single-ecosystem repo / a targeted view.
    // `pub(crate)`, not `pub` (DEPS-ATTRIB-2 review-2): `OtherEcosystem` is a crate-private
    // view type, so this field is crate-private too — a `pub` field of a `pub(crate)` type in
    // the externally-reachable `DepsListResponse` would trip `private_interfaces`. Serde still
    // populates it (deserialize needs no `pub`); `render_human` reads it in-crate.
    #[serde(default)]
    pub(crate) other_ecosystems: Vec<OtherEcosystem>,
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub results: Vec<DepModule>,
}

/// The §2.4 resolution posture, resolved from the tri-state tag with the legacy bool as fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Posture {
    Downgraded,
    Clean,
    Unknown,
}

impl DepsListResponse {
    fn posture(&self) -> Posture {
        match self.resolution_state.as_str() {
            "downgraded" => Posture::Downgraded,
            "unknown" => Posture::Unknown,
            "clean" => Posture::Clean,
            // Older daemon without the tag: honour the legacy bool.
            _ => {
                if self.resolution_downgraded {
                    Posture::Downgraded
                } else {
                    Posture::Clean
                }
            }
        }
    }
}

/// Max manifest rows shown before the rollup line (§2.5 ≤20-line bound). With a 4-line header
/// block, 7 two-line rows (14) + a rollup line (1) = 19 ≤ 20.
const MAX_ROWS: usize = 7;
/// At/below this many manifests each row can afford a 3rd examples line: 5×3 + 4 header = 19 ≤ 20.
const EXAMPLES_THRESHOLD: usize = 5;
/// The §2.4 per-entry resolution-honesty label (active alias/workspace downgrade).
const RESOLUTION_LABEL: &str = "imports not resolved on this index";
/// The §2.4 per-entry label when the resolution state itself is UNKNOWN (ruling 3 item 1).
const UNKNOWN_RESOLUTION_LABEL: &str = "resolution state unknown on this index";

impl DepsListResponse {
    /// Render the ≤20-line human table (§2.5). The unattributed headline is literally line 1 when
    /// present (§2.3 — leveldb's reader-context line moves from position 632 to position 1).
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // §2.3: the unattributed headline is FIRST (line 1) whenever anything is unattributed.
        let has_headline =
            self.unattributed_external_imports > 0 && !self.unattributed_reason.is_empty();
        if has_headline {
            out.push_str(&format!("⚠ {}\n", self.unattributed_reason));
        }

        // Command + ecosystem + MODULE count (line 1 when no headline, else line 2). `count` is the
        // number of reconciled module rows, not manifests — a none-detected repo (leveldb) has zero
        // manifests, so calling these "manifests" would be a name-behaviour mismatch.
        out.push_str(&format!(
            "deps · {} · {} module{}\n",
            if self.ecosystem.is_empty() {
                "unknown"
            } else {
                &self.ecosystem
            },
            self.count,
            if self.count == 1 { "" } else { "s" }
        ));

        // Totals + honest drops + resolution posture.
        let mut totals = format!(
            "{} external ref{}",
            self.total_external_imports,
            if self.total_external_imports == 1 {
                ""
            } else {
                "s"
            }
        );
        if self.rejected_non_specifier_total > 0 {
            totals.push_str(&format!(
                " · {} non-import fragment{} dropped",
                self.rejected_non_specifier_total,
                if self.rejected_non_specifier_total == 1 {
                    ""
                } else {
                    "s"
                }
            ));
        }
        match self.posture() {
            Posture::Downgraded => totals.push_str(" · resolution downgraded on this index"),
            // Ruling 3 item 1: a failed overlay read is UNKNOWN-with-reason, never silent "clean".
            Posture::Unknown => {
                totals.push_str(" · resolution state unknown");
                if !self.resolution_note.is_empty() {
                    totals.push_str(&format!(" ({})", self.resolution_note));
                }
            }
            Posture::Clean => {}
        }
        out.push_str(&totals);
        out.push('\n');

        let coverage_eco = if self.ecosystem.is_empty() {
            "workspace"
        } else {
            &self.ecosystem
        };
        // DEPS-ATTRIB-2 review-1 item 2 / operator binding: a read that FEEDS the coverage split
        // failed → coverage is UNKNOWN-with-reason, NEVER a silent omission. The scanned denominator
        // is stated only when we still know it (`manifests_present > 0`); when even that read failed
        // the reason stands ALONE (never a fabricated `0 manifests`). Takes precedence over the
        // computed split (absent here).
        if !self.manifests_coverage_unavailable.is_empty() {
            if self.manifests_present > 0 {
                out.push_str(&format!(
                    "{} {} manifest{} present; coverage unknown ({})\n",
                    self.manifests_present,
                    coverage_eco,
                    if self.manifests_present == 1 { "" } else { "s" },
                    self.manifests_coverage_unavailable,
                ));
            } else {
                out.push_str(&format!(
                    "{} manifest coverage unknown ({})\n",
                    coverage_eco, self.manifests_coverage_unavailable,
                ));
            }
        } else if self.manifests_present > self.manifests_attributed {
            let gap = self.manifests_present - self.manifests_attributed;
            // Decompose the shortfall by cause: computed no-indexed-source, indexed-but-unattributed
            // (review-4 blocker 2), and the scanned-but-unparsed remainder whose containment is unknown.
            let no_dep_record = gap
                .saturating_sub(self.manifests_no_indexed_source)
                .saturating_sub(self.manifests_indexed_unattributed);
            let plural = if self.manifests_present == 1 { "" } else { "s" };
            if no_dep_record == 0 && self.manifests_indexed_unattributed == 0 {
                // The ENTIRE shortfall is COMPUTED no-indexed-source: parsed manifests that own no
                // source of their own (a zero-dependency workspace ROOT — FRAKTAG). The facts are
                // unchanged from the pre-slice output, so the legacy wording is preserved VERBATIM
                // (review-1 item 1 — FRAKTAG byte-parity). `manifests_no_indexed_source` equals the
                // gap here and is > 0. glamCRM (attributed == present) never reaches this branch, so
                // its false excuse still cannot render.
                out.push_str(&format!(
                    "{} of {} {} manifest{} attributed to a module ({} govern no indexed source)\n",
                    self.manifests_attributed,
                    self.manifests_present,
                    coverage_eco,
                    plural,
                    self.manifests_no_indexed_source,
                ));
            } else {
                // The shortfall has a cause the legacy single-excuse wording cannot express — a
                // scanned-but-unparsed remainder (containment UNKNOWN) and/or indexed-but-unattributed
                // manifests (§2.3 — indexed source IS present, so "govern no indexed source" is FALSE
                // for them). Render each cause as its own honest clause; "govern no indexed source" is
                // claimed ONLY for the computed-zero count, never for the other two.
                let mut clauses: Vec<String> = Vec::new();
                if self.manifests_no_indexed_source > 0 {
                    clauses.push(format!(
                        "{} govern no indexed source",
                        self.manifests_no_indexed_source
                    ));
                }
                if self.manifests_indexed_unattributed > 0 {
                    // review-4 blocker 2 / §2.3: "N files indexed under this manifest, not attributed".
                    clauses.push(format!(
                        "{} present with indexed source not attributed to a module ({} file{})",
                        self.manifests_indexed_unattributed,
                        self.manifests_indexed_unattributed_files,
                        if self.manifests_indexed_unattributed_files == 1 {
                            ""
                        } else {
                            "s"
                        },
                    ));
                }
                if no_dep_record > 0 {
                    clauses.push(format!(
                        "{} present, no dependency record on this build",
                        no_dep_record
                    ));
                }
                out.push_str(&format!(
                    "{} of {} {} manifest{} govern indexed source ({})\n",
                    self.manifests_attributed,
                    self.manifests_present,
                    coverage_eco,
                    plural,
                    clauses.join("; "),
                ));
            }
        }

        // DEPS-ATTRIB-2 §2.4 (ruling Option 2): every materially-present secondary ecosystem states
        // its truth here — attributed deps, unknown-with-reason, or computed-true absence — so a
        // material ecosystem (glamCRM's Java half) is never silently absent from the default view.
        // Rendered BEFORE the empty-results guard so the secondary truth survives even when the
        // dominant ecosystem has no module rows.
        for e in &self.other_ecosystems {
            out.push_str(&render_other_ecosystem(e));
        }

        if self.results.is_empty() {
            out.push_str("\n(no manifest-scoped modules; see the headline above)\n");
            return out;
        }

        let with_examples = self.results.len() <= EXAMPLES_THRESHOLD;
        out.push('\n');
        for m in self.results.iter().take(MAX_ROWS) {
            out.push_str(&format!("{}  [{}]\n", module_label(m), manifest_label(m)));
            // §2.4: when resolution is downgraded OR unknown, the declared-unused count carries the
            // honesty label — those declared deps may be unresolved (or resolution is unknown) on
            // this index, not necessarily truly unused. Clean state asserts no such caveat.
            let unused_suffix = if m.declared_but_unobserved > 0 {
                match self.posture() {
                    Posture::Downgraded => format!(" ({RESOLUTION_LABEL})"),
                    Posture::Unknown => format!(" ({UNKNOWN_RESOLUTION_LABEL})"),
                    Posture::Clean => String::new(),
                }
            } else {
                String::new()
            };
            // A scope-unavailable module (none-detected) has no declared context, so its externals
            // land in `unknown_external_like`; show that count so the row is never a deceptive
            // `0/0/0/0` beside real imports (leveldb's C/C++ includes).
            let unknown_suffix = if m.unknown_external_like > 0 {
                format!(" · unknown-external {}", m.unknown_external_like)
            } else {
                String::new()
            };
            // DEPS-SELF-1 (§2.2): first-party self-references are noted as `self N` — never folded
            // into `undeclared` (django importing `django` was the false "undeclared: django"). The
            // clause is omitted when there are none (byte-identical to the pre-slice row).
            let self_suffix = if m.first_party_self > 0 {
                format!(" · self {}", m.first_party_self)
            } else {
                String::new()
            };
            out.push_str(&format!(
                "  used {} · declared-unused {}{} · undeclared {}{} · builtins {}{}\n",
                m.declared_and_used,
                m.declared_but_unobserved,
                unused_suffix,
                m.observed_but_undeclared,
                self_suffix,
                m.runtime_builtins,
                unknown_suffix,
            ));
            if with_examples {
                if let Some(line) = examples_line(m) {
                    out.push_str(&format!("  {}\n", line));
                }
            }
        }

        // §2.5 rollup — density by design, never truncated silence: state HOW MANY modules were
        // rolled up and the total declared deps across them (what was rolled up), not just "+N more".
        if self.results.len() > MAX_ROWS {
            let rolled = &self.results[MAX_ROWS..];
            let rolled_deps: u64 = rolled
                .iter()
                .map(|m| m.declared_and_used + m.declared_but_unobserved)
                .sum();
            out.push_str(&format!(
                "(+{} more module{}: {} declared dep{} — `--json` for all)\n",
                rolled.len(),
                if rolled.len() == 1 { "" } else { "s" },
                rolled_deps,
                if rolled_deps == 1 { "" } else { "s" },
            ));
        }

        out
    }
}

/// The module label (`.` for the repo root).
fn module_label(m: &DepModule) -> &str {
    if m.module.is_empty() {
        "."
    } else {
        m.module.as_str()
    }
}

/// The manifest cell: the exact parsed file, else the §2.2 unknown-with-reason note, else an
/// honest "no manifest" marker — NEVER a fabricated fixed-name path.
fn manifest_label(m: &DepModule) -> String {
    if let Some(p) = m.manifest_path.as_deref().filter(|p| !p.is_empty()) {
        return p.to_string();
    }
    if let Some(note) = m.manifest_context.as_deref().filter(|n| !n.is_empty()) {
        return format!("manifest {}", note);
    }
    if m.manifest_scope_available {
        "manifest file unknown".to_string()
    } else {
        "no manifest — imports unattributed".to_string()
    }
}

/// A compact examples line: up to 3 example package names per non-empty reconciled category.
fn examples_line(m: &DepModule) -> Option<String> {
    let pick = |cat: &str| -> Vec<&str> {
        m.entries
            .iter()
            .filter(|e| e.category == cat)
            .map(|e| e.package.as_str())
            .take(3)
            .collect()
    };
    let mut parts: Vec<String> = Vec::new();
    for (label, cat) in [
        ("used", "declared_and_used"),
        ("unused", "declared_but_unobserved"),
        ("undeclared", "observed_but_undeclared"),
    ] {
        let names = pick(cat);
        if !names.is_empty() {
            parts.push(format!("{}: {}", label, names.join(", ")));
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(format!("e.g. {}", parts.join("  ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resp(json: serde_json::Value) -> DepsListResponse {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn coverage_read_failure_renders_unknown_with_reason_not_silent() {
        // DEPS-ATTRIB-2 review-0 item 1 / operator binding: a failed owned-files read renders the
        // coverage as UNKNOWN-with-reason over the known denominator — never a silent omission and
        // never a false split (`manifests_attributed`/`no_indexed_source` are absent in this payload).
        let r = resp(serde_json::json!({
            "ecosystem": "npm",
            "total_external_imports": 10,
            "manifests_present": 7,
            "manifests_coverage_unavailable": "could not read owned files: disk error",
            "count": 0,
            "results": []
        }));
        let out = r.render_human();
        assert!(
            out.contains("7 npm manifests present; coverage unknown (could not read owned files: disk error)"),
            "coverage read failure not surfaced with reason: {out}"
        );
        assert!(
            !out.contains("govern no indexed source"),
            "must not fabricate a coverage split on a failed read: {out}"
        );
    }

    #[test]
    fn coverage_unavailable_without_denominator_renders_reason_alone() {
        // DEPS-ATTRIB-2 review-1 item 2: the shared diagnostics blob failed → BOTH the present-count
        // denominator AND the provenance split are unknown. The coverage line must STILL render the
        // reason (no denominator, no fabricated `0 manifests`) — never a silent omission.
        let r = resp(serde_json::json!({
            "ecosystem": "npm",
            "total_external_imports": 10,
            "manifests_coverage_unavailable": "extraction diagnostics not valid JSON: expected value",
            "count": 0,
            "results": []
        }));
        let out = r.render_human();
        assert!(
            out.contains(
                "npm manifest coverage unknown (extraction diagnostics not valid JSON: expected value)"
            ),
            "coverage unknown without a denominator not surfaced: {out}"
        );
        assert!(
            !out.contains("0 npm manifest"),
            "must not fabricate a zero denominator: {out}"
        );
    }

    #[test]
    fn parsed_zero_source_manifest_keeps_legacy_govern_no_indexed_source_wording() {
        // When the ENTIRE shortfall is COMPUTED no-indexed-source — parsed manifests whose subtree
        // truly contains zero indexed files (`manifests_no_indexed_source == gap`) — the claim is
        // computed-true (§2.3-honest) AND matches the pre-slice wording, so the legacy line is
        // preserved VERBATIM (review-1 item 1, for the case the audit-line's assumption happened to be
        // computable). NOTE: this is NOT FRAKTAG's live shape — FRAKTAG's workspace-root package.json
        // is present-but-UNPARSED (absent from provenance → `no_indexed_source == 0`), so it renders
        // the honest no-dependency-record line below, not this one. See build report + DECISION_REQUIRED
        // DR-FRAKTAG-BYTEPARITY.
        let r = resp(serde_json::json!({
            "ecosystem": "npm",
            "total_external_imports": 635,
            "manifests_present": 4,
            "manifests_attributed": 3,
            "manifests_no_indexed_source": 1,
            "count": 3,
            "results": [{
                "module": "packages/api",
                "manifest_path": "packages/api/package.json",
                "manifest_scope_available": true,
                "declared_and_used": 2,
                "entries": []
            }]
        }));
        let out = r.render_human();
        assert!(
            out.contains(
                "3 of 4 npm manifests attributed to a module (1 govern no indexed source)"
            ),
            "computed zero-source manifest must keep the legacy verbatim wording: {out}"
        );
    }

    #[test]
    fn present_but_unparsed_manifest_renders_honest_no_record_never_assumed_no_source() {
        // FRAKTAG's ACTUAL live shape (VERIFIED 2026-08-31 isolated index): 4 npm manifests present, 3
        // parsed leaves (all govern indexed source), the workspace ROOT present-but-UNPARSED → absent
        // from provenance → `manifests_no_indexed_source == 0`. §2.3 FORBIDS claiming "govern no indexed
        // source" for that unparsed remainder (we never computed its subtree is empty). The honest line
        // states what actually failed instead. This diverges from the audit capture BY DESIGN — the
        // audit line was itself the §2.3 assumed-not-computed bug (DR-FRAKTAG-BYTEPARITY).
        let r = resp(serde_json::json!({
            "ecosystem": "npm",
            "total_external_imports": 635,
            "manifests_present": 4,
            "manifests_attributed": 3,
            "manifests_no_indexed_source": 0,
            "count": 3,
            "results": [{
                "module": "packages/api",
                "manifest_path": "packages/api/package.json",
                "manifest_scope_available": true,
                "declared_and_used": 2,
                "entries": []
            }]
        }));
        let out = r.render_human();
        assert!(
            out.contains(
                "3 of 4 npm manifests govern indexed source (1 present, no dependency record on this build)"
            ),
            "unparsed remainder must render the honest no-record line, never an assumed no-source claim: {out}"
        );
        assert!(
            !out.contains("govern no indexed source"),
            "must NOT claim 'govern no indexed source' for a manifest whose emptiness was not computed: {out}"
        );
    }

    #[test]
    fn indexed_but_unattributed_manifest_renders_honest_clause_never_no_indexed_source() {
        // review-4 blocker 2 / §2.3: a parsed manifest whose subtree has INDEXED source that no module
        // owns must render the honest "indexed source not attributed" clause with the file count —
        // NEVER "govern no indexed source" (the excuse is false; indexed source IS present).
        let r = resp(serde_json::json!({
            "ecosystem": "npm",
            "total_external_imports": 10,
            "manifests_present": 3,
            "manifests_attributed": 2,
            "manifests_no_indexed_source": 0,
            "manifests_indexed_unattributed": 1,
            "manifests_indexed_unattributed_files": 4,
            "count": 2,
            "results": [{
                "module": "serverless",
                "manifest_path": "serverless/package.json",
                "manifest_scope_available": true,
                "declared_and_used": 1,
                "entries": []
            }]
        }));
        let out = r.render_human();
        assert!(
            out.contains(
                "2 of 3 npm manifests govern indexed source \
                 (1 present with indexed source not attributed to a module (4 files))"
            ),
            "indexed-but-unattributed manifest must render the honest clause: {out}"
        );
        assert!(
            !out.contains("govern no indexed source"),
            "must NOT claim 'govern no indexed source' when indexed source IS present: {out}"
        );
    }

    #[test]
    fn secondary_ecosystem_java_states_truth_in_default_view() {
        // §2.4 (ruling Option 2): the DEFAULT npm view names Java's attributed Gradle deps — the
        // audit's "zero mention of Java" is gone, and NO no-reader sentence is emitted for a reader.
        let r = resp(serde_json::json!({
            "ecosystem": "npm",
            "total_external_imports": 100,
            "count": 1,
            "results": [{
                "module": "serverless",
                "manifest_path": "serverless/package.json",
                "manifest_scope_available": true,
                "declared_and_used": 2,
                "entries": []
            }],
            "other_ecosystems": [
                { "ecosystem": "java", "state": "attributed", "declared_dependencies": 18, "manifests": 1 }
            ]
        }));
        let out = r.render_human();
        assert!(
            out.contains("java: 18 declared dependencies across 1 manifest — `deps list --ecosystem java` for detail"),
            "Java truth missing from default view: {out}"
        );
        assert!(
            !out.to_lowercase()
                .contains("no dependency-manifest reader for java")
                && !out.to_lowercase().contains("no gradle reader"),
            "must NOT emit a no-reader sentence for an ecosystem that has a reader: {out}"
        );
    }

    #[test]
    fn secondary_ecosystem_unavailable_and_absence_render_honestly() {
        // Unknown-with-reason and computed-true absence are each stated, never silent.
        let unavail = resp(serde_json::json!({
            "ecosystem": "npm", "count": 0, "results": [],
            "other_ecosystems": [
                { "ecosystem": "java", "state": "unavailable", "reason": "manifest backend/build.gradle present but not parsed: permission denied" }
            ]
        }));
        assert!(
            unavail.render_human().contains(
                "java: dependency truth unavailable (manifest backend/build.gradle present but not parsed: permission denied)"
            ),
            "{}", unavail.render_human()
        );
        let absent = resp(serde_json::json!({
            "ecosystem": "npm", "count": 0, "results": [],
            "other_ecosystems": [
                { "ecosystem": "java", "state": "no_manifest_parsed", "source_files": 267 }
            ]
        }));
        assert!(
            absent
                .render_human()
                .contains("java: 267 source files indexed, no manifest parsed on this index"),
            "{}",
            absent.render_human()
        );
    }

    #[test]
    fn headline_is_first_and_present_when_unattributed() {
        let r = resp(serde_json::json!({
            "ecosystem": "none-detected",
            "unattributed_external_imports": 56,
            "unattributed_reason": "no dependency-manifest reader for C++ on this build; 56 external includes observed, not attributed to packages",
            "total_external_imports": 56,
            "count": 0,
            "results": []
        }));
        let out = r.render_human();
        let lines: Vec<&str> = out.lines().collect();
        // §2.3 / operator clarification (1): the headline is LITERALLY line 1 (index 0).
        assert!(lines[0].starts_with("⚠"), "headline not first: {out}");
        assert!(
            lines[0].contains("no dependency-manifest reader for C++"),
            "{out}"
        );
    }

    #[test]
    fn rollup_states_what_was_rolled_up_and_stays_short() {
        // 10 manifests → 7 rows + a rollup line stating the 3 rolled-up manifests + their deps.
        let mods: Vec<serde_json::Value> = (0..10)
            .map(|i| {
                serde_json::json!({
                    "module": format!("pkg{i}"),
                    "manifest_path": format!("pkg{i}/package.json"),
                    "manifest_scope_available": true,
                    "declared_and_used": 2,
                    "declared_but_unobserved": 1,
                    "observed_but_undeclared": 0,
                    "runtime_builtins": 0,
                    "entries": []
                })
            })
            .collect();
        let r = resp(serde_json::json!({
            "ecosystem": "npm",
            "total_external_imports": 50,
            "count": 10,
            "results": mods
        }));
        let out = r.render_human();
        assert!(
            out.lines().count() <= 20,
            "too long ({}): {out}",
            out.lines().count()
        );
        // 3 modules rolled up, 3 declared deps each = 9 total — printed, not silent.
        assert!(out.contains("+3 more modules: 9 declared deps"), "{out}");
    }

    #[test]
    fn self_import_renders_as_self_note_not_undeclared() {
        // DEPS-SELF-1 (§2.2): django's shape — `django` self-import counted as first-party self,
        // rendered `· self 1`, and NOT in the undeclared count/examples.
        let r = resp(serde_json::json!({
            "ecosystem": "python",
            "total_external_imports": 50,
            "count": 1,
            "results": [{
                "module": ".",
                "manifest_path": "pyproject.toml",
                "manifest_scope_available": true,
                "declared_and_used": 3,
                "declared_but_unobserved": 0,
                "observed_but_undeclared": 0,
                "first_party_self": 1,
                "runtime_builtins": 0,
                "entries": [
                    {"package": "django", "category": "first_party_self", "import_count": 42}
                ]
            }]
        }));
        let out = r.render_human();
        assert!(out.contains("· self 1"), "self note missing: {out}");
        assert!(out.contains("undeclared 0"), "undeclared must be 0: {out}");
        // The self package is NOT listed under an `undeclared:` example.
        assert!(
            !out.contains("undeclared: django"),
            "self-import must not render as undeclared: {out}"
        );
    }

    #[test]
    fn no_self_imports_omits_the_self_clause() {
        // Byte-parity: a module with zero self-references renders no `self` clause.
        let r = resp(serde_json::json!({
            "ecosystem": "npm",
            "total_external_imports": 5,
            "count": 1,
            "results": [{
                "module": "app",
                "manifest_path": "app/package.json",
                "manifest_scope_available": true,
                "declared_and_used": 2,
                "observed_but_undeclared": 1,
                "entries": []
            }]
        }));
        let out = r.render_human();
        assert!(!out.contains("· self"), "no self clause expected: {out}");
    }

    #[test]
    fn downgrade_label_renders_per_entry_in_human() {
        let r = resp(serde_json::json!({
            "ecosystem": "npm",
            "resolution_downgraded": true,
            "total_external_imports": 5,
            "count": 1,
            "results": [{
                "module": "app",
                "manifest_path": "app/package.json",
                "manifest_scope_available": true,
                "declared_and_used": 1,
                "declared_but_unobserved": 2,
                "observed_but_undeclared": 0,
                "runtime_builtins": 0,
                "entries": []
            }]
        }));
        let out = r.render_human();
        assert!(out.contains("imports not resolved on this index"), "{out}");
    }

    #[test]
    fn unknown_resolution_state_renders_as_unknown_not_clean() {
        // Ruling 3 item 1: a failed trust-overlay read must render UNKNOWN-with-reason, never silent
        // "clean" certainty (the audit's false-1.0 case).
        let r = resp(serde_json::json!({
            "ecosystem": "npm",
            "resolution_state": "unknown",
            "resolution_note": "resolution-state unknown (overlay read failed: extraction diagnostics unreadable)",
            "total_external_imports": 5,
            "count": 1,
            "results": [{
                "module": "app",
                "manifest_path": "app/package.json",
                "manifest_scope_available": true,
                "declared_and_used": 1,
                "declared_but_unobserved": 2,
                "observed_but_undeclared": 0,
                "runtime_builtins": 0,
                "entries": []
            }]
        }));
        let out = r.render_human();
        assert!(out.contains("resolution state unknown"), "{out}");
        assert!(
            out.contains("resolution state unknown on this index"),
            "{out}"
        );
    }

    #[test]
    fn workspace_coverage_shortfall_splits_no_source_from_unparsed() {
        // DEPS-ATTRIB-2 §2.3: the shortfall line claims "govern no indexed source" ONLY for the
        // computed-zero count; the rest of the gap is labelled "no dependency record". Here 43
        // present, 9 govern indexed source, 30 computed to govern zero indexed files → the remaining 4
        // are scanned-but-unparsed.
        let r = resp(serde_json::json!({
            "ecosystem": "npm",
            "total_external_imports": 100,
            "manifests_present": 43,
            "manifests_attributed": 9,
            "manifests_no_indexed_source": 30,
            "count": 9,
            "results": [{
                "module": "pkg0",
                "manifest_path": "pkg0/package.json",
                "manifest_scope_available": true,
                "declared_and_used": 1,
                "declared_but_unobserved": 0,
                "observed_but_undeclared": 0,
                "runtime_builtins": 0,
                "entries": []
            }]
        }));
        let out = r.render_human();
        assert!(
            out.contains("9 of 43 npm manifests govern indexed source"),
            "{out}"
        );
        assert!(out.contains("30 govern no indexed source"), "{out}");
        assert!(
            out.contains("4 present, no dependency record on this build"),
            "{out}"
        );
        assert!(out.lines().count() <= 20, "too long: {out}");
    }

    #[test]
    fn all_manifests_governing_source_render_no_shortfall_line() {
        // glamCRM's shape: every present manifest governs indexed source (attributed == present) →
        // the false "N govern no indexed source" excuse cannot render at all.
        let r = resp(serde_json::json!({
            "ecosystem": "npm",
            "total_external_imports": 100,
            "manifests_present": 7,
            "manifests_attributed": 7,
            "manifests_no_indexed_source": 0,
            "count": 3,
            "results": [{
                "module": "serverless",
                "manifest_path": "serverless/package.json",
                "manifest_scope_available": true,
                "declared_and_used": 2,
                "declared_but_unobserved": 0,
                "observed_but_undeclared": 0,
                "runtime_builtins": 0,
                "entries": []
            }]
        }));
        let out = r.render_human();
        assert!(
            !out.contains("govern no indexed source"),
            "false excuse rendered: {out}"
        );
        assert!(
            !out.contains("attributed to a module"),
            "stale wording: {out}"
        );
    }

    #[test]
    fn provenance_unavailable_renders_note_not_fabricated_path() {
        let r = resp(serde_json::json!({
            "ecosystem": "java",
            "total_external_imports": 3,
            "count": 1,
            "results": [{
                "module": "svc",
                "manifest_path": serde_json::Value::Null,
                "manifest_context": "unavailable (indexed before provenance tracking)",
                "manifest_scope_available": true,
                "declared_and_used": 1,
                "declared_but_unobserved": 0,
                "observed_but_undeclared": 0,
                "runtime_builtins": 0,
                "entries": []
            }]
        }));
        let out = r.render_human();
        assert!(
            out.contains("unavailable (indexed before provenance tracking)"),
            "{out}"
        );
        assert!(
            !out.contains("build.gradle"),
            "must not fabricate a manifest name: {out}"
        );
    }

    #[test]
    fn false_zero_cannot_render_as_full_coverage() {
        // glamCRM shape: many external imports, no manifest-scoped modules.
        let r = resp(serde_json::json!({
            "ecosystem": "npm",
            "unattributed_external_imports": 4336,
            "unattributed_reason": "4336 of 4336 external references not attributed to a declared manifest (imported files outside a parsed manifest scope)",
            "total_external_imports": 4336,
            "count": 0,
            "results": []
        }));
        let out = r.render_human();
        assert!(out.contains("⚠"), "must warn: {out}");
        assert!(out.contains("4336 of 4336"), "{out}");
        assert!(!out.contains("count: 0"), "{out}");
    }

    #[test]
    fn table_shows_counts_and_examples_and_stays_short() {
        let r = resp(serde_json::json!({
            "ecosystem": "python",
            "unattributed_external_imports": 0,
            "unattributed_reason": "all external references attributed or classified",
            "total_external_imports": 20,
            "rejected_non_specifier_total": 3,
            "count": 1,
            "results": [{
                "module": ".",
                "manifest_path": "pyproject.toml",
                "manifest_scope_available": true,
                "declared_and_used": 3,
                "declared_but_unobserved": 1,
                "observed_but_undeclared": 0,
                "runtime_builtins": 5,
                "entries": [
                    {"package": "asgiref", "category": "declared_and_used", "import_count": 4},
                    {"package": "sqlparse", "category": "declared_and_used", "import_count": 2},
                    {"package": "tzdata", "category": "declared_and_used", "import_count": 1}
                ]
            }]
        }));
        let out = r.render_human();
        assert!(out.contains("pyproject.toml"), "{out}");
        assert!(out.contains("used 3"), "{out}");
        assert!(out.contains("asgiref"), "{out}");
        assert!(out.contains("3 non-import fragments dropped"), "{out}");
        // No unattributed headline line when nothing is unattributed.
        assert!(!out.contains("⚠"), "{out}");
        assert!(out.lines().count() <= 20, "too long: {out}");
    }
}
