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
    #[serde(default)]
    pub manifests_attributed: u64,
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

        // §2.2 / ruling-3 item-4: reported workspace-coverage shortfall — manifests present but not
        // attributed to any reconciled module (the amodx 9-of-43 case), stated, never silent.
        if self.manifests_present > self.manifests_attributed {
            out.push_str(&format!(
                "{} of {} {} manifest{} attributed to a module ({} govern no indexed source)\n",
                self.manifests_attributed,
                self.manifests_present,
                if self.ecosystem.is_empty() {
                    "workspace"
                } else {
                    &self.ecosystem
                },
                if self.manifests_present == 1 { "" } else { "s" },
                self.manifests_present - self.manifests_attributed,
            ));
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
            out.push_str(&format!(
                "  used {} · declared-unused {}{} · undeclared {} · builtins {}{}\n",
                m.declared_and_used,
                m.declared_but_unobserved,
                unused_suffix,
                m.observed_but_undeclared,
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
    fn workspace_coverage_shortfall_is_reported() {
        // Ruling 3 item 4: manifests present but not attributed to any module are stated, not silent.
        let r = resp(serde_json::json!({
            "ecosystem": "npm",
            "total_external_imports": 100,
            "manifests_present": 43,
            "manifests_attributed": 9,
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
        assert!(out.contains("9 of 43 npm manifests attributed"), "{out}");
        assert!(out.contains("34 govern no indexed source"), "{out}");
        assert!(out.lines().count() <= 20, "too long: {out}");
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
