//! ORIENT-SEGMENT-2 section renderers for the `orient` command.
//!
//! Split out of `orient_sections.rs` to keep each module under the 500-line
//! structural guardrail — the `orient.rs` / `orient_sections.rs` /
//! `orient_reliability.rs` split idiom, extended. A FOURTH `impl OrientResponse`
//! block (inherent impls may span modules within the crate). It owns the four
//! ORIENT-SEGMENT-2 presentation surfaces; each renders NOTHING unless its data is
//! present, so a repo that trips none of them is byte-identical to today:
//!   - §2.5 `http_surfaces_line` — the HTTP architecture headline (from the daemon's
//!     additive `http_surfaces` field).
//!   - §2.1 `directory_groups_section` — the promoted directory-group fan-in view on
//!     package-group collapse (from the daemon's additive `directory_group_fallback`).
//!   - §2.4 `budget_saturated` — the "output complete" ladder signal at `--full`.
//!   - §2.2 `ModuleRow` / `module_row_label` — `name [manifest]` on a module name
//!     collision / divergence, from the agent's per-row `top_modules` evidence.

use serde::Deserialize;

use super::module_disambiguation::{collision_disambiguator, ModuleRow};
use super::orient::{OrientDepth, OrientResponse};
use super::orient_sections::plural;
use super::{bullet, heading};

/// ORIENT-SEGMENT-2 §2.1: reader-side mirror of the daemon's
/// `orient_topology_fallback::DirectoryGroupFallback`. The producer emits exactly one
/// valid shape: SUCCESS (`groups` + `total` together) or FAILURE (`unavailable`).
/// `total` is the COMPLETE directory-group count so the omission line stays TRUE.
///
/// The fields stay lenient `Option`s here (a scalar-typed `DirGroupRow` still HARD-fails
/// the parse on a malformed row — see below), and `directory_groups_section` VALIDATES the
/// block SHAPE at render time: any attached-but-incomplete shape (`groups` without `total`,
/// `total` without `groups`, or an empty object) renders as unknown-WITH-REASON, never a
/// silent partial/empty section (review-4 §2; standing honesty rule #1).
#[derive(Debug, Deserialize)]
pub struct DirectoryGroupFallback {
    #[serde(default)]
    pub groups: Option<Vec<DirGroupRow>>,
    #[serde(default)]
    pub total: Option<usize>,
    #[serde(default)]
    pub unavailable: Option<String>,
}

/// Every field is REQUIRED (no `#[serde(default)]`): the daemon producer
/// (`orient_topology_fallback::DirGroupRow`) always serializes all four as plain
/// scalars, so a missing field is a protocol violation, NOT a `0` to render. A
/// `#[serde(default)]` here would mint a false `fan-in 0` from absent data — exactly
/// the RENDERED false-zero standing honesty rule #1 forbids. A malformed row instead
/// fails the whole `directory_group_fallback` parse (the field is then treated as
/// absent), so no fabricated count ever reaches the reader.
#[derive(Debug, Deserialize)]
pub struct DirGroupRow {
    pub name: String,
    pub fan_in: i64,
    pub fan_out: i64,
    pub file_count: i64,
}

/// ORIENT-SEGMENT-2 §2.5: reader-side mirror of the daemon's injected HTTP surface
/// counts. The producer emits exactly one valid shape: SUCCESS (`total`/`providers`/
/// `consumers` together, `total > 0`) or FAILURE (`unavailable` with the union read's
/// reason). `http_surfaces_line` VALIDATES the shape at render time: an attached-but-
/// incomplete success (partial counts) renders as unknown-WITH-REASON at the detail
/// tiers, never a silent drop nor a fabricated count from the partial (review-4 §2).
#[derive(Debug, Deserialize)]
pub struct HttpSurfaces {
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub providers: Option<u64>,
    #[serde(default)]
    pub consumers: Option<u64>,
    #[serde(default)]
    pub unavailable: Option<String>,
}

/// MODULE-EDGES-1 §2.3: reader-side mirror of the daemon's injected top cross-module
/// edges. The producer emits exactly one valid shape: SUCCESS (`edges`, non-empty) or
/// FAILURE (`unavailable` with the graph read's reason). Every field is a lenient
/// `Option` (an attached-but-malformed block never fails the whole orient parse — the
/// `http_surfaces` idiom, not the `DirGroupRow` hard-scalar idiom), and
/// `top_module_edges_line` VALIDATES the shape at render time: a failed/malformed block
/// renders as unknown-WITH-REASON at the detail tiers, never a silent drop nor a
/// fabricated edge (standing honesty rule #1). Absent on the wire (no cross-module
/// edges) → no line, byte-identical.
#[derive(Debug, Deserialize)]
pub struct TopModuleEdges {
    #[serde(default)]
    pub edges: Option<Vec<TopModuleEdgeRow>>,
    #[serde(default)]
    pub unavailable: Option<String>,
}

/// One row of [`TopModuleEdges`]. Fields are lenient `Option`s: a row missing any of
/// them is UNKNOWN (the daemon always serializes all three as scalars), so the render
/// treats a malformed row as unknown-with-reason rather than fabricate a `0`/empty
/// endpoint.
#[derive(Debug, Deserialize)]
pub struct TopModuleEdgeRow {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub import_count: Option<u64>,
}

/// ORIENT-SEGMENT-2 §2.6: how many docs the headline Docs line names. A headline is
/// a one-liner; the full ranked set stays on `documentation` (JSON) and `rmap docs`
/// lists it whole, so this cap never overclaims. README / architecture rank first
/// (the agent's relevance order), so the cap keeps the MOST orienting docs.
pub(super) const DOC_HEADLINE_CAP: usize = 6;

/// Row-level completeness gate for a RENDERED evidence list (review-3 §1 + review-4 §1):
/// the array at `ev[key]` must be PRESENT and readable as an array, COVER the known
/// `total` (`len >= total` — nothing elided), AND have EVERY row well-formed per `row_ok`.
/// The row check is the review-4 addition: a list whose length covers the total but holds
/// a row the renderer would DEGRADE to a false `0` / `(unknown)` (via its `unwrap_or`
/// defaults) is NOT affirmatively complete — the completeness claim must see well-formed
/// rows, not merely a covering count. Absent / non-array → `false` (UNKNOWN, never a
/// "vacuously complete" zero; the conservative direction on unknown is the pre-slice
/// rendering: withhold "output complete"). Pass `total = 0` for a list that is not
/// length-capped at `--full` (e.g. `package_groups`) — then only presence + row
/// well-formedness gate the claim.
fn array_rows_complete(
    key: &str,
    ev: &serde_json::Value,
    total: u64,
    row_ok: impl Fn(&serde_json::Value) -> bool,
) -> bool {
    ev.get(key)
        .and_then(|v| v.as_array())
        .is_some_and(|list| list.len() as u64 >= total && list.iter().all(row_ok))
}

/// A `top_modules` row is well-formed for rendering when it carries the fields
/// `module_breakdown_section` READS as load-bearing: a `path` (string) and a `file_count`
/// (u64). `name` / `manifest` are legitimately optional (the §2.2 label degrades to the
/// path), so they are NOT required. A row missing `path` renders `(unknown)` and one
/// missing `file_count` renders `0 files` — both false-from-`unwrap_or`, so their absence
/// must block the completeness claim (review-4 §1).
fn module_row_wellformed(m: &serde_json::Value) -> bool {
    m.get("path").and_then(|v| v.as_str()).is_some()
        && m.get("file_count").and_then(|v| v.as_u64()).is_some()
}

/// A `package_groups` row is well-formed for rendering when it carries the fields
/// `package_groups_section` READS as load-bearing: a `name` (string) and a `file_count`
/// (u64). `test_file_count` is optional by design (absent → no test suffix). A row missing
/// `name`/`file_count` renders `(unknown) — 0 files` (false-from-`unwrap_or`), so it blocks
/// the completeness claim.
fn package_group_row_wellformed(g: &serde_json::Value) -> bool {
    g.get("name").and_then(|v| v.as_str()).is_some()
        && g.get("file_count").and_then(|v| v.as_u64()).is_some()
}

/// A `top_complex` row is well-formed for rendering when it carries a readable
/// `complexity` (u64) AND at least one label (`file` or `symbol`, string) —
/// `complexity_breakdown_section` renders `cx 0` from a missing complexity (false-zero)
/// and SKIPS a label-less row entirely (silently shrinking the shown set), so both defects
/// must block the completeness claim (review-4 §1).
fn complexity_row_wellformed(e: &serde_json::Value) -> bool {
    e.get("complexity").and_then(|v| v.as_u64()).is_some()
        && (e.get("file").and_then(|v| v.as_str()).is_some()
            || e.get("symbol").and_then(|v| v.as_str()).is_some())
}

impl OrientResponse {
    /// ORIENT-SEGMENT-2 §2.5: the HTTP architecture headline — rendered where the
    /// repo HAS HTTP surfaces (> 0), from the HSC-1 unified counts the daemon
    /// attached (READ-only; the SAME union `surfaces list` / `boundaries summary`
    /// consume). A FAILED union read is surfaced as unknown-with-reason at the
    /// detailed tiers only (large / --full) — it never mints a fabricated count nor
    /// pollutes the small/medium headline. `None` when the daemon attached nothing
    /// (non-HTTP repo → byte-identical).
    pub(super) fn http_surfaces_line(&self, depth: OrientDepth) -> Option<String> {
        // Absent field (non-HTTP repo) → byte-identical, no line.
        let http = self.http_surfaces.as_ref()?;
        // Valid SUCCESS shape: all three counts present (the daemon producer emits them
        // together). A KNOWN zero-surface count renders no headline (preserved — the
        // producer never actually emits `total == 0`, but the guard keeps that honest).
        if let (Some(total), Some(providers), Some(consumers)) =
            (http.total, http.providers, http.consumers)
        {
            if total > 0 {
                return Some(format!(
                    "{} HTTP surface{} ({} provider{} / {} consumer{}) — rmap surfaces",
                    total,
                    plural(total),
                    providers,
                    plural(providers),
                    consumers,
                    plural(consumers),
                ));
            }
            return None;
        }
        // Stated FAILURE shape (the union read's reason): unknown-with-reason, rendered at
        // the detail tiers only — it never pollutes the small/medium headline.
        if let Some(reason) = &http.unavailable {
            return depth
                .shows_full_detail()
                .then(|| format!("HTTP surfaces: unavailable ({reason})"));
        }
        // ATTACHED but neither a complete success nor a stated failure → a MALFORMED
        // success shape (partial counts — e.g. `total` without `consumers`). It is
        // unknown-WITH-REASON at the detail tiers, NEVER a silent drop of a repo that
        // carries the field nor a fabricated count from the present partial (review-4 §2;
        // standing honesty rule #1 — attached-but-malformed is unknown, not absent).
        depth
            .shows_full_detail()
            .then(|| "HTTP surfaces: unavailable (malformed surface counts)".to_string())
    }

    /// MODULE-EDGES-1 §2.3: the top cross-module edges headline — `a → b (N), c → d (M)`
    /// (top 3 by reference count), from the daemon's injected `top_module_edges`. This
    /// is the VISION primary use case ("how modules relate / where the boundaries are")
    /// on the first-60-seconds surface. Rendered at EVERY depth when edges exist (a
    /// load-bearing architecture fact at any budget). A FAILED graph read is
    /// unknown-with-reason at the detail tiers only (never a fabricated line, never a
    /// small/medium-headline pollutant); an attached-but-malformed block is likewise
    /// unknown-with-reason (standing honesty rule #1). `None` when the daemon attached
    /// nothing (no cross-module edges → byte-identical).
    pub(super) fn top_module_edges_line(&self, depth: OrientDepth) -> Option<String> {
        let block = self.top_module_edges.as_ref()?;
        // Stated FAILURE shape: unknown-with-reason at the detail tiers only.
        if let Some(reason) = &block.unavailable {
            return depth
                .shows_full_detail()
                .then(|| format!("Module edges: unavailable ({reason})"));
        }
        // SUCCESS shape: a non-empty, fully-formed edge list. A malformed row (any
        // missing endpoint/count) is unknown-with-reason at the detail tiers — never a
        // fabricated endpoint.
        let Some(edges) = &block.edges else {
            return depth
                .shows_full_detail()
                .then(|| "Module edges: unavailable (malformed edge block)".to_string());
        };
        let mut parts = Vec::with_capacity(edges.len());
        for e in edges {
            match (e.source.as_deref(), e.target.as_deref(), e.import_count) {
                (Some(s), Some(t), Some(n)) => parts.push(format!("{s} \u{2192} {t} ({n})")),
                _ => {
                    return depth
                        .shows_full_detail()
                        .then(|| "Module edges: unavailable (malformed edge row)".to_string())
                }
            }
        }
        // Empty success shape is not injected by the producer; guard keeps that honest.
        if parts.is_empty() {
            return None;
        }
        Some(format!("Module edges: {}", parts.join(", ")))
    }

    /// ORIENT-SEGMENT-2 §2.1: the PROMOTED directory-group fan-in view, rendered only
    /// when the daemon injected it (package-group collapse). Honestly labelled "no
    /// manifest topology at this depth" so the reader knows the manifest fold gave
    /// nothing. A failed read renders unknown-with-reason (never a fabricated empty
    /// view). Empty (byte-identical) when the daemon attached nothing.
    pub(super) fn directory_groups_section(&self) -> String {
        let Some(fb) = &self.directory_group_fallback else {
            // Absent field (non-collapsed repo) → byte-identical empty section.
            return String::new();
        };
        let head = heading("Directory groups (no manifest topology at this depth)");
        // Stated FAILURE shape (the producer's `unavailable`): unknown-with-reason.
        if let Some(reason) = &fb.unavailable {
            return format!("{head}{}", bullet(&format!("unavailable ({reason})")));
        }
        // A valid SUCCESS shape carries BOTH `groups` and `total` (the daemon producer
        // serializes them together). An ATTACHED block that is neither a stated failure
        // nor a complete success — `groups` without `total`, `total` without `groups`, or
        // an empty object — is a MALFORMED protocol shape. It is unknown-WITH-REASON, NOT
        // absence: rendering the groups without an honest omission count, or rendering
        // nothing at all, would silently hide a defect (review-4 §2; standing honesty rule
        // #1 — an attached-but-malformed field is unknown, never silently absent).
        let (Some(groups), Some(total)) = (&fb.groups, fb.total) else {
            return format!(
                "{head}{}",
                bullet(
                    "unavailable (malformed directory-group fallback: incomplete success shape)"
                )
            );
        };
        if groups.is_empty() {
            // review-5 #2: empty `groups` is an honest nothing ONLY when `total == 0`.
            // `groups: [] , total > 0` is a CONTRADICTORY success shape — the total
            // claims groups exist that the list does not carry: unknown-with-reason.
            if total > 0 {
                return format!(
                    "{head}{}",
                    bullet(&format!(
                        "unavailable (contradictory fallback shape: total={total} but no group rows)"
                    ))
                );
            }
            return String::new();
        }
        let mut out = head;
        for g in groups {
            out.push_str(&bullet(&format!(
                "{} — fan-in {}, fan-out {} ({} file{})",
                g.name,
                g.fan_in,
                g.fan_out,
                g.file_count,
                plural(g.file_count.max(0) as u64),
            )));
        }
        // `total` is now KNOWN — render the honest omission line when groups were elided.
        let shown = groups.len();
        if total > shown {
            out.push_str(&bullet(&format!(
                "… and {} more group{} — see `stats`",
                total - shown,
                plural((total - shown) as u64),
            )));
        }
        out
    }

    /// ORIENT-SEGMENT-2 §2.4: is every rendered section AFFIRMATIVELY COMPLETE at
    /// `--full` (nothing elided)? This gates the honest "budget not reached — output
    /// complete" terminal line, so it may claim completeness ONLY from POSITIVE
    /// evidence, never from silence: an ABSENT structural signal or a FAILED read is
    /// UNKNOWN, and unknown is never "complete" (standing honesty rule #1; VISION:
    /// unknown is never a fabricated positive). It returns `false` — refusing the
    /// claim — the moment any section elides OR any completeness input is missing /
    /// malformed / unavailable.
    pub(super) fn budget_saturated(&self) -> bool {
        // Gate on the MODULE_SUMMARY structural evidence: without it there is nothing
        // to prove the sections are whole (a degenerate / signal-less response must
        // not read as "complete"). A repo small enough to saturate the ladder always
        // carries this signal, so this refuses only the no-evidence claim.
        let Some(ms) = self.module_summary_evidence() else {
            return false;
        };

        // Declared/inferred modules (review-3 §1 + review-4 §1): completeness requires
        // AFFIRMATIVE, well-formed evidence of BOTH the true module count AND the shown
        // `top_modules` list — the list PRESENT, COVERING the count, and every row
        // well-formed for rendering (a readable `path` + `file_count`; else the renderer
        // mints a `(unknown)` / `0 files` yet we would stamp "complete"). A MISSING
        // `discovered_module_count` (module discovery unavailable — the Rust-indexer /
        // no-candidates path carries a MODULE_DATA_UNAVAILABLE limit) or a MISSING / short /
        // malformed-row `top_modules` is UNKNOWN, and unknown is never "complete". No
        // zero-total exception: an absent list cannot affirm the module section is whole.
        let Some(module_total) = ms.get("discovered_module_count").and_then(|v| v.as_u64()) else {
            return false;
        };
        if !array_rows_complete("top_modules", ms, module_total, module_row_wellformed) {
            return false;
        }

        // Package groups (review-3 §1 + review-4 §1): at `--full` `package_group_section_cap`
        // is `None` (ORIENT-SEGMENT-2 §2.4, operator ruling 2) — the section renders the
        // COMPLETE, budget-independent `package_groups` evidence array, so it is whole IFF
        // that array is PRESENT, readable, and every row well-formed for rendering (a
        // readable `name` + `file_count`). Absent / malformed-row → UNKNOWN → refuse (never
        // "complete" from silence or from a false-zero row). `total = 0`: `package_groups`
        // is neither agent-capped nor render-capped at `--full`, so presence + row
        // well-formedness — not a length cover — gate the claim.
        if !array_rows_complete("package_groups", ms, 0, package_group_row_wellformed) {
            return false;
        }

        // Directory-group fallback (collapse only). Whole IFF the read SUCCEEDED
        // (`unavailable` absent), BOTH `groups` and `total` are present, and the shown
        // groups COVER the complete total (`len >= total` — exactly the negation of the
        // section's "… and N more" elision condition). A FAILED read, a missing `groups`,
        // OR a missing `total` (review-3 §1: "groups but no total") is UNKNOWN → refuse.
        if let Some(fb) = &self.directory_group_fallback {
            if fb.unavailable.is_some() {
                return false;
            }
            match (fb.groups.as_ref(), fb.total) {
                (Some(groups), Some(total)) if groups.len() >= total => {}
                _ => return false,
            }
        }

        // Complexity: a PRESENT HIGH_COMPLEXITY signal is complete only when its true count
        // is KNOWN and the shown list is READABLE, covers it, and every RENDERED row is
        // well-formed (review-4 §1: a `cx 0` false-zero or a silently-skipped label-less row
        // would make the section degrade while we claim "complete"). Any unreadable input is
        // UNKNOWN → refuse the completeness claim (standing honesty rule #1). Unlike the
        // MODULE list, a KNOWN `high_complexity_count == 0` with a producer-omitted empty
        // list is affirmative (nothing to elide) — the zero-tolerant absent-list exception,
        // retained here inline (`array_rows_complete` has no such exception by design).
        if let Some(ev) = self.signal_evidence("HIGH_COMPLEXITY") {
            let Some(total) = ev.get("high_complexity_count").and_then(|v| v.as_u64()) else {
                return false;
            };
            match ev.get("top_complex").and_then(|v| v.as_array()) {
                Some(list) => {
                    if (list.len() as u64) < total || !list.iter().all(complexity_row_wellformed) {
                        return false;
                    }
                }
                // Absent list with a KNOWN zero count: complete-empty (the producer omits
                // the empty list). A positive count with no list is elided → refuse.
                None => {
                    if total != 0 {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// ORIENT-SEGMENT-2 §2.2: the rendered label for module row `idx` (`rows[idx]`).
    /// Renders `name [disambiguator]` ONLY when there is a genuine disambiguation need
    /// — a name COLLISION with another shown row, or (for a manifest-DECLARED module)
    /// a name/path DIVERGENCE. Otherwise renders the path verbatim, so a repo whose
    /// module names already equal their paths (leveldb's inferred C++ dirs, which
    /// carry no manifest) is BYTE-IDENTICAL to today.
    ///
    /// Disambiguator choice (honesty): the MANIFEST when it actually distinguishes
    /// this row from every collider (`Django [pyproject.toml]` / `Django
    /// [package.json]`); otherwise the unique canonical PATH — when two namesakes
    /// share a manifest the manifest alone would leave them label-identical, so the
    /// path is the honest tie-break.
    ///
    /// A free associated fn (no `self`): every input comes from the per-row evidence,
    /// so there is nothing on the response to read.
    pub(super) fn module_row_label(
        rows: &[ModuleRow<'_>],
        effective_names: &[&str],
        idx: usize,
    ) -> String {
        let row = &rows[idx];
        let path = row.path;
        let diverges = row.name.is_some_and(|n| n != path);

        // The shared collision→token decision (MODULES-IDENTITY-2 §2.1): `Some` iff
        // this row's effective name collides with another shown row. `modules list`
        // consumes the SAME helper — one implementation, never a second copy.
        if let Some(token) = collision_disambiguator(rows, effective_names, idx) {
            let base = row.name.unwrap_or(path);
            return format!("{base} [{token}]");
        }

        // No collision: orient renders the PATH by default (leveldb's inferred C++
        // dirs, which carry no manifest, stay byte-identical) — UNLESS a
        // manifest-DECLARED module's name diverges from its path, which surfaces
        // `name [manifest]` so the declared identity is visible.
        if diverges {
            if let Some(m) = row.manifest {
                let base = row.name.unwrap_or(path);
                return format!("{base} [{m}]");
            }
        }
        path.to_string()
    }
}

#[cfg(test)]
#[path = "orient_seg2_tests.rs"]
mod tests;
