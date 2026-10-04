//! Presentation layer for `deps list` (DEPS-LIST-REWRITE-1 §2.5).
//!
//! Renders the daemon's dependency reconciliation as a ≤20-line, one-screen human table:
//! the unattributed headline FIRST (§2.3), then totals, then one row per manifest with the four
//! reconciled counts (declared+used / declared-unobserved / observed-undeclared / builtins) and top
//! examples. The declared-unobserved column is BASIS-DEPENDENT (HONESTY-GATE-1 §2.2): it renders the
//! word "unused" (as the header `declared-unused`) ONLY when the ecosystem's import evidence
//! establishes absence; otherwise it reads "no static import found" with a caveat naming what was not
//! checked. On this build the basis is `NotEstablished` for every ecosystem (the ratified honest
//! floor), so the column never renders "unused" — that header appears only behind an `Established`
//! basis a future evidence slice supplies.
//! The `--json` path prints the daemon payload verbatim (same truth, additive) and does not go
//! through this renderer.
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
    /// DEPS-CLASSIFIER-1 §2.3: import-site / call-site split behind the per-package
    /// `used (N import sites, M call sites)` basis. Additive; an older daemon omits them (both
    /// default 0 → the basis reads `used (0 import sites, 0 call sites)`, never a fabricated count).
    #[serde(default)]
    pub import_sites: u64,
    #[serde(default)]
    pub call_sites: u64,
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
    /// DEPS-CLASSIFIER-1B §2.2 item 3: declared packages whose only evidence of use is a type-only
    /// import (`import type … from "pkg"`). Rendered as a `type-only N` clause + examples. Additive;
    /// an older daemon omits it (0 → no clause, byte-identical).
    #[serde(default)]
    pub type_only_import: u64,
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
    /// HONESTY-GATE-1 §2.2 (arithmetic reconciliation): the distinct parsed manifests that
    /// contributed this module's declared deps. Length >1 means a coarse module aggregates several
    /// nested manifests (storybook root) — the manifest cell then names the span so the declared
    /// count is never cited against a single manifest it exceeds. Empty/len==1 → the single cited
    /// manifest (byte-parity). Additive; an older daemon omits it.
    #[serde(default)]
    pub declared_manifest_paths: Vec<String>,
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
    /// DEPS-ECOSYSTEM-PARTITION-1 §2.1 (012-L06): observed references skipped because their source
    /// file belongs to a different ecosystem than the view — the same count the human ⚠ line states.
    /// Additive: `#[serde(default)]` so an envelope from an older daemon (no key) deserializes to 0
    /// and renders exactly as today (the ⚠ line is the daemon's `unattributed_reason`, verbatim).
    #[serde(default)]
    pub cross_ecosystem: u64,
    #[serde(default)]
    pub rejected_non_specifier_total: u64,
    /// HONESTY-GATE-1 §2.3: the Maven capability-limit sentence (java view, pom.xml present, no
    /// reader). Non-empty ⇒ rendered as the trust-ceiling line AND the transient "resolution
    /// downgraded" posture suffix is suppressed (an architectural gap, not an index state). Empty for
    /// every non-Maven view.
    #[serde(default)]
    pub maven_capability_limit: String,
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
    /// HONESTY-GATE-1 §2.1 (the invariant): whether the ecosystem's import evidence is COMPLETE
    /// enough to assert ABSENCE of use — static resolved AND dynamic-import literals extracted AND
    /// root config files in scope. `"established"` → the declared-unobserved column may render the
    /// word "unused". Anything else (currently always `"no_static_import_found"`, since the index does
    /// not establish dynamic-import resolution or root-config import coverage) → the column renders "no
    /// static import found" with the caveat below. Absent (older daemon) → treated as NOT established:
    /// the honesty-preserving default never asserts "unused" without the basis. The word "unused"
    /// never renders without this being `"established"`.
    #[serde(default)]
    pub declared_unobserved_basis: String,
    /// HONESTY-GATE-1 §2.1: the caveat naming the coverage the "no static import found" column has NOT
    /// established (dynamic-import resolution, root-config import coverage, ecosystem-specific static-
    /// resolution gaps, active resolution downgrade). Reader-facing and honest — it states what is not
    /// established, never a false "not scanned"/"not extracted" mechanism claim (review-0). Rendered
    /// once when any row carries a no-static-import count and the basis is not established. Empty → no
    /// caveat line.
    #[serde(default)]
    pub declared_unobserved_caveat: String,
    /// DEPS-GRADLE-CATALOG-1A (D-DGC-CONDITIONAL-1; RG-REQ-002-L11): the number of Gradle
    /// `dependencies` blocks the index could not attribute statically in this view's builds — the
    /// declared set may be missing their declarations. Additive, `#[serde(default)]`: an older
    /// daemon (or untracked provenance) omits it → 0 → today's output. Nonzero → the note line and
    /// the ` (declared set incomplete)` suffix on every nonzero `undeclared` count.
    #[serde(default)]
    pub declared_undetermined_blocks: u64,
    /// The daemon's sentence for `declared_undetermined_blocks` (`declared set may be incomplete:
    /// N dependency block(s) not statically attributed (<path>:<line>) — investigate`). Empty with a
    /// nonzero count → the count-only form renders instead (never dropped).
    #[serde(default)]
    pub declared_undetermined_note: String,
    /// DEPS-GRADLE-CATALOG-1B (D-DGC1B-ALIAS-MARKING-1; RG-REQ-002-L11): the number of Gradle
    /// version-catalog alias references in this view's builds that the index could not bind to a
    /// group — the declared set may be missing what they name. Additive, `#[serde(default)]`: an
    /// older daemon (or untracked provenance) omits it → 0 → today's output. Nonzero → one note line
    /// after the undetermined-blocks line (it adds no row suffix).
    #[serde(default)]
    pub declared_unresolved_alias_refs: u64,
    /// The daemon's sentence for `declared_unresolved_alias_refs` (`declared set may be incomplete:
    /// N alias reference(s) could not be resolved (<first>) — investigate`). Empty with a nonzero
    /// count → the count-only form renders instead (never dropped).
    #[serde(default)]
    pub declared_unresolved_alias_note: String,
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

/// HONESTY-GATE-1 §2.1 (operator pin 2026-09-04): the resolved absence-evidence basis for the
/// declared-but-unobserved column.
///
/// Abstraction one-liner — WHAT: an internal renderer sum type for the unused-basis. CURRENT USERS:
/// `render_human` (column label + posture suffix + the caveat line) and `examples_line` (the example
/// label). AXIS OF VARIATION: whether the ecosystem's import evidence is complete enough to assert
/// ABSENCE of use — variants FIXED, render operations GROWING → sum type + exhaustive match; adding a
/// third evidence state is a deliberate compile break at every match site. REJECTED SIMPLER
/// ALTERNATIVE: a `bool` + a parallel `declared_unobserved_caveat` field — the "flag + nullable whose
/// validity depends on the flag" defect-shape this pin exists to forbid (the `missing` caveat is only
/// meaningful on `NotEstablished`, so it lives ON that variant, not beside a bool).
///
/// The `Established` arm is the NAMED re-enable point for the future dynamic-import + root-config
/// evidence slice: it flips ONE variant, not a rewrite. There is no current producer of `Established`
/// — the index emits `NotEstablished` for every ecosystem until that evidence ships. The raw DTO
/// stays a `String` tag (boundary-raw across the JSON boundary); this type is reconstructed from it.
enum DeclaredUnobservedBasis {
    /// Import evidence is complete enough to assert ABSENCE — the column may print the word "unused".
    Established,
    /// Evidence is incomplete; `missing` names what was NOT checked (dynamic-import literals, root
    /// config files, …). The column prints "no static import found" and the caveat is emitted once.
    NotEstablished { missing: String },
}

impl DepsListResponse {
    /// HONESTY-GATE-1 §2.1 (operator pin 2026-09-04): reconstruct the absence-evidence basis as a
    /// SUM TYPE from the raw DTO strings. Only the explicit `"established"` tag yields `Established`;
    /// absence / any other value is the honesty-preserving `NotEstablished` default (the word
    /// "unused" never renders without the basis). The `missing` caveat rides on the `NotEstablished`
    /// variant — where it is the only place it is valid — instead of a decoupled parallel field.
    fn resolved_basis(&self) -> DeclaredUnobservedBasis {
        if self.declared_unobserved_basis == "established" {
            DeclaredUnobservedBasis::Established
        } else {
            DeclaredUnobservedBasis::NotEstablished {
                missing: self.declared_unobserved_caveat.clone(),
            }
        }
    }

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
        // HONESTY-GATE-1 §2.3: when the Maven capability limit applies, the "resolution downgraded"/
        // "unknown" suffix is SUPPRESSED — the gap is architectural (no Maven parser), not a transient
        // index state. The capability sentence (below / in the headline) is the honest explanation.
        let maven_limit = !self.maven_capability_limit.is_empty();
        if !maven_limit {
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
        }
        out.push_str(&totals);
        out.push('\n');

        // HONESTY-GATE-1 §2.3: name the Maven capability limit as its own line WHEN the unattributed
        // headline did not already carry it (the daemon routes the sentence into the ⚠ headline when
        // there are unattributed imports to explain — hadoop's 72016 — so this avoids duplication).
        if maven_limit && !has_headline {
            out.push_str(&format!("⚠ {}\n", self.maven_capability_limit));
        }

        // HONESTY-GATE-1 §2.1 (the invariant): when the ecosystem's import evidence is NOT complete
        // enough to assert absence (the declared-unobserved column renders "no static import found",
        // not "unused"), state — ONCE, at ecosystem level — the caveat naming what was not checked.
        // Emitted only when a row actually carries a no-static-import count, so a fully-used repo adds
        // no line. The word "unused" is never printed for these rows; this caveat is why.
        let basis = self.resolved_basis();
        if let DeclaredUnobservedBasis::NotEstablished { missing } = &basis {
            if !missing.is_empty() && self.results.iter().any(|m| m.declared_but_unobserved > 0) {
                out.push_str(&format!(
                    "ⓘ \"no static import found\" ≠ unused: {missing}\n"
                ));
            }
        }

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

        // DEPS-GRADLE-CATALOG-1A (D-DGC-CONDITIONAL-1): dependency blocks the index could not
        // attribute statically make the declared set possibly incomplete — stated once, after the
        // coverage line, so an agent reads every `undeclared` count below as possibly a declaration
        // the index did not evaluate, not a certain absence.
        if self.declared_undetermined_blocks > 0 {
            if self.declared_undetermined_note.is_empty() {
                out.push_str(&format!(
                    "declared set may be incomplete: {} dependency block(s) not statically attributed — investigate\n",
                    self.declared_undetermined_blocks
                ));
            } else {
                out.push_str(&self.declared_undetermined_note);
                out.push('\n');
            }
        }

        // DEPS-GRADLE-CATALOG-1B (D-DGC1B-ALIAS-MARKING-1): alias references the index could not
        // bind to a group make the declared set possibly incomplete — one line, right after the
        // undetermined-blocks line, naming the first site and the test that failed for it. It adds
        // no row suffix (only the undetermined-blocks marking does, as before).
        if self.declared_unresolved_alias_refs > 0 {
            if self.declared_unresolved_alias_note.is_empty() {
                out.push_str(&format!(
                    "declared set may be incomplete: {} alias reference(s) could not be resolved — investigate\n",
                    self.declared_unresolved_alias_refs
                ));
            } else {
                out.push_str(&self.declared_unresolved_alias_note);
                out.push('\n');
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
            out.push_str(&format!(
                "{}  [{}]\n",
                module_label(m),
                manifest_label(m, &self.ecosystem)
            ));
            // HONESTY-GATE-1 §2.1: the declared-but-unobserved column names its BASIS. Only an
            // established basis (dynamic-import literals + root config files evidenced) may print the
            // word "unused"; otherwise the honest label is "no static import found" and the caveat
            // above states what was not checked. In the (future) established path, an active resolution
            // downgrade/unknown still tags the row — a declared dep may be an unresolved import.
            let (unused_label, unused_suffix) = match &basis {
                // Established: the column may print "unused"; an active resolution downgrade/unknown
                // still tags the row (a declared dep may be an unresolved import).
                DeclaredUnobservedBasis::Established => {
                    let suffix = if m.declared_but_unobserved > 0 {
                        match self.posture() {
                            Posture::Downgraded => format!(" ({RESOLUTION_LABEL})"),
                            Posture::Unknown => format!(" ({UNKNOWN_RESOLUTION_LABEL})"),
                            Posture::Clean => String::new(),
                        }
                    } else {
                        String::new()
                    };
                    ("declared-unused", suffix)
                }
                DeclaredUnobservedBasis::NotEstablished { .. } => {
                    ("no static import found", String::new())
                }
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
            // DEPS-CLASSIFIER-1B §2.2 item 3: a `type-only N` clause when a declared package is
            // imported only type-only (compile-time reference, not a runtime dep). Omitted when 0
            // (byte-parity with pre-slice rows).
            let type_only_suffix = if m.type_only_import > 0 {
                format!(" · type-only {}", m.type_only_import)
            } else {
                String::new()
            };
            // DEPS-GRADLE-CATALOG-1A: a nonzero `undeclared` count may hold declarations the index
            // did not evaluate (the note above). `undeclared 0` cannot be inflated by a missing
            // declaration, so it carries no suffix (the `(resolution downgraded)` placement).
            let undeclared_suffix =
                if self.declared_undetermined_blocks > 0 && m.observed_but_undeclared > 0 {
                    " (declared set incomplete)"
                } else {
                    ""
                };
            out.push_str(&format!(
                "  used {}{} · {} {}{} · undeclared {}{}{} · builtins {}{}\n",
                m.declared_and_used,
                type_only_suffix,
                unused_label,
                m.declared_but_unobserved,
                unused_suffix,
                m.observed_but_undeclared,
                undeclared_suffix,
                self_suffix,
                m.runtime_builtins,
                unknown_suffix,
            ));
            if with_examples {
                if let Some(line) = examples_line(m, &basis) {
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
                // DEPS-CLASSIFIER-1B §2.2 item 3: a type-only-imported package is still a DECLARED
                // dependency (reconcile's declared-not-value-observed branch), so it counts toward
                // "declared deps". Omitting it understated the rollup total by the type-only count
                // once this slice split that bucket out of declared_but_unobserved.
                .map(|m| m.declared_and_used + m.type_only_import + m.declared_but_unobserved)
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
///
/// HONESTY-GATE-1 §2.2 (arithmetic reconciliation): when the module's declared deps come from more
/// than one PARSED manifest (`declared_manifest_paths.len() > 1` — a coarse module that owns files
/// under several nested manifests, storybook's root `.`), the cell NAMES the span and the total
/// declared count, so the count is reconciled to the M manifests that produced it rather than cited
/// against a single manifest it exceeds (the arithmetically-impossible 111-vs-13 defect). A single
/// contributing manifest renders exactly as before (byte-parity).
fn manifest_label(m: &DepModule, ecosystem: &str) -> String {
    if m.declared_manifest_paths.len() > 1 {
        // DEPS-CLASSIFIER-1B §2.2 item 3: type-only-imported packages are declared dependencies too;
        // include them so "N declared across M manifests" stays the TRUE declared count (before this
        // slice the two-way split made used+unobserved == declared; the new bucket must be re-added).
        let declared_total = m.declared_and_used + m.type_only_import + m.declared_but_unobserved;
        let cited = m
            .manifest_path
            .as_deref()
            .filter(|p| !p.is_empty())
            .unwrap_or(&m.declared_manifest_paths[0]);
        let eco = if ecosystem.is_empty() {
            "manifests"
        } else {
            ecosystem
        };
        return format!(
            "{} (+{} nested {} manifest{}, {} declared across {})",
            cited,
            m.declared_manifest_paths.len() - 1,
            eco,
            if m.declared_manifest_paths.len() - 1 == 1 {
                ""
            } else {
                "s"
            },
            declared_total,
            m.declared_manifest_paths.len(),
        );
    }
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
///
/// HONESTY-GATE-1 §2.1: the declared-but-unobserved examples are labelled "no static import" unless
/// the ecosystem's absence basis is established — the word "unused" never appears without the basis.
fn examples_line(m: &DepModule, basis: &DeclaredUnobservedBasis) -> Option<String> {
    // DEPS-CLASSIFIER-1 §2.3: each `used` example carries its COMPUTED basis — the import-site /
    // call-site split (`asgiref (29 import sites, 136 call sites)`) — so the reader sees the evidence
    // that a declared package is used, not just its name. Other categories keep the name-only form.
    let pick_used = |cat: &str| -> Vec<String> {
        m.entries
            .iter()
            .filter(|e| e.category == cat)
            .map(|e| format!("{} ({})", e.package, used_basis(e)))
            .take(3)
            .collect()
    };
    let pick_names = |cat: &str| -> Vec<&str> {
        m.entries
            .iter()
            .filter(|e| e.category == cat)
            .map(|e| e.package.as_str())
            .take(3)
            .collect()
    };
    let unobserved_label = match basis {
        DeclaredUnobservedBasis::Established => "unused",
        DeclaredUnobservedBasis::NotEstablished { .. } => "no static import",
    };
    let mut parts: Vec<String> = Vec::new();
    let used = pick_used("declared_and_used");
    if !used.is_empty() {
        parts.push(format!("used: {}", used.join(", ")));
    }
    // §2.2 item 3: type-only imports get their own labelled example group.
    let type_only = pick_names("type_only_import");
    if !type_only.is_empty() {
        parts.push(format!("type-only import: {}", type_only.join(", ")));
    }
    for (label, cat) in [
        (unobserved_label, "declared_but_unobserved"),
        ("undeclared", "observed_but_undeclared"),
    ] {
        let names = pick_names(cat);
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

/// The per-package computed basis for a `declared_and_used` row (DEPS-CLASSIFIER-1 §2.3):
/// `N import sites, M call sites`, pluralized. `import_sites`/`call_sites` are additive daemon
/// fields; a pre-slice daemon omits them (both 0) — the basis then honestly reads `0 import sites,
/// 0 call sites` rather than inventing a figure.
fn used_basis(e: &DepEntry) -> String {
    let site = |n: u64, noun: &str| format!("{} {}{}", n, noun, if n == 1 { "" } else { "s" });
    format!(
        "{}, {}",
        site(e.import_sites, "import site"),
        site(e.call_sites, "call site")
    )
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
        // The per-entry posture label is now reachable only when the unused BASIS is established
        // (HONESTY-GATE-1 §2.1) — otherwise the column is "no static import found" and the posture
        // reason lives in the ecosystem caveat. Established here to exercise the preserved path.
        let r = resp(serde_json::json!({
            "ecosystem": "npm",
            "declared_unobserved_basis": "established",
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
            "declared_unobserved_basis": "established",
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

    #[test]
    fn asgiref_reads_used_with_computed_basis_tzdata_stays_unobserved() {
        // DEPS-CLASSIFIER-1 §2.3 (replaces the false fixture that canonized `no static import:
        // asgiref`). django's TRUE row: `asgiref` IS imported — 29 import sites + 136 call sites —
        // so it reads `used (…)` with its computed basis and is NEVER in the "no static import"
        // column; `tzdata` is genuinely unused, so it alone carries the "no static import" basis and
        // the ≠-unused caveat. The false half (asgiref) is gone; the true half (tzdata) survives.
        let r = resp(serde_json::json!({
            "ecosystem": "python",
            "declared_unobserved_basis": "no_static_import_found",
            "declared_unobserved_caveat": "a declared package with no resolved static import may still be used at runtime — dynamic imports … are not resolved to a declared package; import coverage from root config files … is not established",
            "total_external_imports": 165,
            "count": 1,
            "results": [{
                "module": ".",
                "manifest_path": "pyproject.toml",
                "manifest_scope_available": true,
                "declared_and_used": 1,
                "declared_but_unobserved": 1,
                "entries": [
                    {"package": "asgiref", "category": "declared_and_used", "import_count": 165, "import_sites": 29, "call_sites": 136},
                    {"package": "tzdata", "category": "declared_but_unobserved", "import_count": 0, "import_sites": 0, "call_sites": 0}
                ]
            }]
        }));
        let out = r.render_human();
        // asgiref reads `used` with its computed import-site / call-site basis.
        assert!(
            out.contains("used: asgiref (29 import sites, 136 call sites)"),
            "asgiref must render its computed used-basis: {out}"
        );
        // The false negative is GONE: asgiref never appears under "no static import".
        assert!(
            !out.contains("no static import: asgiref")
                && !out.contains("no static import: asgiref, tzdata"),
            "asgiref must NOT be reported as no-static-import: {out}"
        );
        // tzdata is genuinely unobserved → the "no static import" column + example still apply to it.
        assert!(
            out.contains("no static import found 1"),
            "tzdata's genuine absence must still render: {out}"
        );
        assert!(
            out.contains("no static import: tzdata"),
            "tzdata example must carry the no-static-import basis: {out}"
        );
        // The ≠-unused caveat still fires (a real declared-but-unobserved row exists).
        assert!(
            out.contains("ⓘ \"no static import found\" ≠ unused:"),
            "caveat line missing: {out}"
        );
    }

    #[test]
    fn established_basis_renders_the_word_unused() {
        // HONESTY-GATE-1 operator pin (2026-09-04): the `Established` arm of the basis SUM TYPE is the
        // NAMED re-enable point. When a future dynamic-import + root-config evidence slice sets the
        // basis to "established", declared-but-unobserved packages read the word "unused" (column AND
        // examples), NEVER "no static import found", and the caveat line is suppressed. This guards the
        // flip — one variant, not a rewrite — so the re-enable point is real, not aspirational.
        let r = resp(serde_json::json!({
            "ecosystem": "npm",
            "declared_unobserved_basis": "established",
            "declared_unobserved_caveat": "should be ignored once the basis is established",
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
                "entries": [
                    {"package": "left-pad", "category": "declared_but_unobserved", "import_count": 0}
                ]
            }]
        }));
        let out = r.render_human();
        assert!(
            out.contains("declared-unused 2"),
            "established basis must render the 'unused' column: {out}"
        );
        assert!(
            out.contains("unused: left-pad"),
            "established basis must render 'unused' examples: {out}"
        );
        assert!(
            !out.contains("no static import"),
            "established basis must NOT render the no-static-import label: {out}"
        );
        assert!(
            !out.contains("ⓘ"),
            "established basis must suppress the not-established caveat line: {out}"
        );
    }

    #[test]
    fn multi_manifest_module_reconciles_declared_count_not_cited_against_one() {
        // HONESTY-GATE-1 §2.2 (arithmetic): storybook root shape — a coarse module aggregates deps
        // from many nested manifests. The row must NOT cite a 124-declared count against the single
        // root package.json (13 deps); the manifest cell names the M-manifest span + declared total.
        let r = resp(serde_json::json!({
            "ecosystem": "npm",
            "total_external_imports": 6220,
            "count": 1,
            "results": [{
                "module": ".",
                "manifest_path": "package.json",
                "manifest_scope_available": true,
                "declared_and_used": 13,
                "declared_but_unobserved": 111,
                "declared_manifest_paths": ["package.json", "test-storybooks/a/package.json", "test-storybooks/b/package.json"],
                "entries": []
            }]
        }));
        let out = r.render_human();
        assert!(
            out.contains("+2 nested npm manifests, 124 declared across 3"),
            "declared count not reconciled to its manifests: {out}"
        );
        // The single-manifest cite of a 124-count is gone.
        assert!(
            !out.contains(".  [package.json]\n"),
            "must not cite 124 declared against the single root manifest: {out}"
        );
    }

    #[test]
    fn maven_capability_limit_names_the_gap_and_suppresses_downgraded() {
        // HONESTY-GATE-1 §2.3 (hadoop): pom.xml present, no Maven parser. The capability limit is
        // named (trust ceiling) and the transient "resolution downgraded" suffix is SUPPRESSED.
        let r = resp(serde_json::json!({
            "ecosystem": "java",
            "resolution_downgraded": true,
            "unattributed_external_imports": 72016,
            "unattributed_reason": "Maven manifests are not parsed on this build (119 pom.xml present) — Java dependency attribution unavailable",
            "maven_capability_limit": "Maven manifests are not parsed on this build (119 pom.xml present) — Java dependency attribution unavailable",
            "total_external_imports": 72016,
            "count": 0,
            "results": []
        }));
        let out = r.render_human();
        assert!(
            out.contains("Maven manifests are not parsed on this build (119 pom.xml present)"),
            "capability limit not named: {out}"
        );
        assert!(
            !out.contains("resolution downgraded on this index"),
            "transient downgrade suffix must be suppressed for the Maven capability limit: {out}"
        );
        // The sentence rides the ⚠ headline (unattributed present); no duplicate line.
        assert_eq!(
            out.matches("Maven manifests are not parsed").count(),
            1,
            "capability sentence must not duplicate: {out}"
        );
    }

    #[test]
    fn cross_ecosystem_field_defaults_to_zero_from_an_older_daemon() {
        // DEPS-ECOSYSTEM-PARTITION-1 §2.1 (012-L06): the `cross_ecosystem` field is additive — an
        // envelope from an older daemon that never emitted the key deserializes to 0 and renders
        // exactly as today (the ⚠ line is the daemon's `unattributed_reason`, verbatim).
        let older = resp(serde_json::json!({
            "ecosystem": "npm",
            "unattributed_external_imports": 4,
            "unattributed_reason": "4 of 10 external references not attributed to a declared manifest (imported files outside a parsed manifest scope)",
            "total_external_imports": 10,
            "count": 0,
            "results": []
        }));
        assert_eq!(older.cross_ecosystem, 0, "absent key defaults to 0");
        let out = older.render_human();
        assert!(
            out.contains(
                "⚠ 4 of 10 external references not attributed to a declared manifest (imported files outside a parsed manifest scope)"
            ),
            "older-daemon envelope still renders its ⚠ headline verbatim: {out}"
        );

        // A current envelope carrying the cross-ecosystem headline populates the field and renders
        // the daemon reason verbatim.
        let current = resp(serde_json::json!({
            "ecosystem": "npm",
            "unattributed_external_imports": 13967,
            "unattributed_reason": "13956 of 13967 external references are imports from files outside the npm ecosystem (13956 python) — see `deps list --ecosystem python`",
            "cross_ecosystem": 13956,
            "total_external_imports": 13967,
            "count": 0,
            "results": []
        }));
        assert_eq!(current.cross_ecosystem, 13956);
        assert!(
            current.render_human().contains(
                "⚠ 13956 of 13967 external references are imports from files outside the npm ecosystem (13956 python) — see `deps list --ecosystem python`"
            ),
            "current envelope renders the cross-ecosystem headline verbatim"
        );
    }

    // ── DEPS-GRADLE-CATALOG-1A (D-DGC-CONDITIONAL-1; RG-REQ-002-L11) — the declared-set marking ──

    const MARK_NOTE: &str = "declared set may be incomplete: 5 dependency block(s) not statically attributed (build.gradle:190) — investigate";

    /// A two-row java payload with a coverage shortfall (so a coverage line renders) and rows with
    /// `undeclared 2` and `undeclared 0`; `extra` adds envelope keys.
    fn marking_payload(extra: serde_json::Value) -> serde_json::Value {
        let mut v = serde_json::json!({
            "ecosystem": "java",
            "total_external_imports": 40,
            "manifests_present": 3,
            "manifests_attributed": 2,
            "manifests_no_indexed_source": 1,
            "count": 2,
            "results": [
                {"module": "core", "manifest_path": "core/build.gradle", "declared_and_used": 1,
                 "declared_but_unobserved": 0, "observed_but_undeclared": 2, "runtime_builtins": 3},
                {"module": "api", "manifest_path": "api/build.gradle", "declared_and_used": 1,
                 "declared_but_unobserved": 0, "observed_but_undeclared": 0, "runtime_builtins": 1}
            ]
        });
        if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
            for (k, x) in e {
                o.insert(k.clone(), x.clone());
            }
        }
        v
    }

    #[test]
    fn declared_undetermined_renders_the_note_after_coverage_and_suffixes_nonzero_undeclared() {
        let out = resp(marking_payload(serde_json::json!({
            "declared_undetermined_blocks": 5,
            "declared_undetermined_note": MARK_NOTE
        })))
        .render_human();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(
            lines.iter().filter(|l| **l == MARK_NOTE).count(),
            1,
            "the note renders exactly once: {out}"
        );
        let note = lines.iter().position(|l| *l == MARK_NOTE).unwrap();
        let coverage = lines
            .iter()
            .position(|l| l.contains("java manifests attributed to a module"))
            .expect("coverage line present");
        let first_row = lines
            .iter()
            .position(|l| l.starts_with("  used "))
            .expect("row present");
        assert!(coverage < note && note < first_row, "{out}");
        assert!(
            out.contains("· undeclared 2 (declared set incomplete) · builtins 3"),
            "a nonzero undeclared count carries the suffix: {out}"
        );
        assert!(
            out.contains("· undeclared 0 · builtins 1"),
            "undeclared 0 cannot be inflated by a missing declaration — no suffix: {out}"
        );
    }

    #[test]
    fn declared_undetermined_zero_renders_neither_line_nor_suffix() {
        let with_zero = resp(marking_payload(serde_json::json!({
            "declared_undetermined_blocks": 0,
            "declared_undetermined_note": ""
        })))
        .render_human();
        let without = resp(marking_payload(serde_json::json!({}))).render_human();
        assert_eq!(with_zero, without, "a zero count renders byte-identically");
        assert!(!with_zero.contains("declared set"), "{with_zero}");
    }

    #[test]
    fn envelope_without_declared_undetermined_keys_renders_as_today() {
        let r = resp(marking_payload(serde_json::json!({})));
        assert_eq!(r.declared_undetermined_blocks, 0);
        assert_eq!(r.declared_undetermined_note, "");
        let out = r.render_human();
        assert!(!out.contains("declared set may be incomplete"), "{out}");
        assert!(out.contains("· undeclared 2 · builtins 3"), "{out}");
    }

    /// A count with an empty note (a malformed envelope) still marks the view: the count-only line,
    /// never dropped, and the suffix.
    #[test]
    fn declared_undetermined_count_without_note_still_renders_a_line() {
        let out = resp(marking_payload(serde_json::json!({
            "declared_undetermined_blocks": 2
        })))
        .render_human();
        assert!(
            out.lines().any(|l| l
                == "declared set may be incomplete: 2 dependency block(s) not statically attributed — investigate"),
            "{out}"
        );
        assert!(
            out.contains("· undeclared 2 (declared set incomplete) ·"),
            "{out}"
        );
    }

    // ── D-DGC-ATTRIBUTION-1: the `deps list` boundary, through the REAL daemon in-process ──
    //
    // Harness of `daemon-runtime/tests/deps_attrib_nested_workspace.rs`: an isolated temp state
    // root (never the operator's registry — RG-REQ-011-L06), background passes forced off. The JSON
    // is the dispatch result `deps list --json` prints verbatim; the human text is that JSON decoded
    // into `DepsListResponse` and rendered, exactly as `commands/deps.rs` does.

    struct QuietEmitter;
    impl repo_graph_daemon_transport::ProgressEmitter for QuietEmitter {
        fn emit(
            &mut self,
            _detail: repo_graph_daemon_transport::ProgressDetail,
        ) -> Result<(), repo_graph_daemon_transport::EmitError> {
            Ok(())
        }
    }

    /// Index `fixture` into a throwaway state root and return `deps list --ecosystem java`'s JSON
    /// and human render.
    fn deps_list_java_through_the_daemon(fixture: &std::path::Path) -> (serde_json::Value, String) {
        use repo_graph_daemon_runtime::{
            auto_reindex, enrich_pass, retention_pass, seed, DaemonState, RepoRegistry,
            ServiceDispatcher,
        };
        use repo_graph_daemon_transport::{DispatchResult, Dispatcher, Request};
        seed::set_auto_seed_for_test(false);
        enrich_pass::set_auto_enrich_for_test(false);
        retention_pass::set_auto_retention_for_test(false);
        auto_reindex::set_auto_reindex_for_test(Some(false));
        let state_root = tempfile::tempdir().expect("state root tempdir");
        let registry = RepoRegistry::with_state_root(state_root.path()).expect("isolated registry");
        let dispatcher =
            ServiceDispatcher::new(std::sync::Arc::new(DaemonState::with_registry(registry)));
        let call = |id: &str, method: &str, params: serde_json::Value| -> serde_json::Value {
            let request = Request {
                id: id.to_string(),
                method: method.to_string(),
                params,
            };
            match dispatcher.dispatch(&request, &mut QuietEmitter) {
                DispatchResult::Success(s) => s.result,
                DispatchResult::Error(e) => {
                    panic!("{method} failed: {} {}", e.error.code, e.error.message)
                }
            }
        };
        let indexed = call(
            "idx",
            "index",
            serde_json::json!({ "repo_path": fixture.to_string_lossy() }),
        );
        let repo = indexed["canonical_path"]
            .as_str()
            .expect("index returns canonical_path")
            .to_string();
        let json = call(
            "deps",
            "deps_list",
            serde_json::json!({ "repo": repo, "ecosystem": "java" }),
        );
        let human = serde_json::from_value::<DepsListResponse>(json.clone())
            .expect("the envelope decodes as `deps list` renders it")
            .render_human();
        (json, human)
    }

    /// Write `body` at repo-relative `rel` under `root`.
    fn put_file(root: &std::path::Path, rel: &str, body: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    /// The human block of the row whose module label is `module`: its header line
    /// (`<module>  [<manifest>]`) through the line before the next unindented line.
    fn row_block<'a>(human: &'a str, module: &str) -> Option<Vec<&'a str>> {
        let lines: Vec<&str> = human.lines().collect();
        let start = lines
            .iter()
            .position(|l| l.starts_with(&format!("{module}  [")))?;
        let mut block = vec![lines[start]];
        for l in &lines[start + 1..] {
            if !l.starts_with(' ') {
                break;
            }
            block.push(l);
        }
        Some(block)
    }

    /// Every row header of the human render (an unindented `<module>  [<manifest>]` line).
    fn row_headers(human: &str, json: &serde_json::Value) -> Vec<String> {
        json["results"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|r| {
                let m = r["module"].as_str().unwrap_or("");
                let label = if m.is_empty() { "." } else { m };
                row_block(human, label).map(|b| b[0].to_string())
            })
            .collect()
    }

    /// Does an `e.g.` example group labelled one of `labels` in `block` name `package`?
    fn block_names_in_group(block: &[&str], labels: &[&str], package: &str) -> bool {
        block.iter().any(|l| {
            l.trim_start().strip_prefix("e.g. ").is_some_and(|groups| {
                groups.split("  ").any(|g| {
                    labels.iter().any(|label| {
                        g.strip_prefix(&format!("{label}: ")).is_some_and(|names| {
                            names
                                .split(", ")
                                .any(|n| n == package || n.starts_with(&format!("{package} (")))
                        })
                    })
                })
            })
        })
    }

    /// D-DGC-ATTRIBUTION-1 case (a), the reviewer's first counterexample, at the `deps list`
    /// boundary: `settings.gradle` includes `a`, `b`; no build script at the root or in `a/`;
    /// `b/build.gradle` declares `org.acme` for `:a`. Project `a`'s own row carries either the
    /// declared coordinate (DECLARED) or its `undeclared` count suffixed and the note naming
    /// `b/build.gradle:2` (UNDETERMINED); no other row ever holds `org.acme`.
    #[test]
    fn deps_list_gradle_project_without_an_ancestor_script_is_declared_or_marked_in_json_and_human()
    {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path();
        put_file(root, "settings.gradle", "include 'a', 'b'\n");
        put_file(
            root,
            "b/build.gradle",
            "project(':a') {\n  dependencies {\n    implementation 'org.acme:lib:1.0'\n  }\n}\n",
        );
        put_file(
            root,
            "a/src/main/java/app/A.java",
            "package app;\nimport org.acme.Lib;\npublic class A { Lib l; }\n",
        );
        put_file(
            root,
            "b/src/main/java/b/B.java",
            "package b;\npublic class B {}\n",
        );
        let (json, human) = deps_list_java_through_the_daemon(root);
        let rows = json["results"].as_array().expect("results[]");
        let row_a = rows
            .iter()
            .find(|r| r["module"] == "a")
            .unwrap_or_else(|| panic!("project a's row is present: {json:#}\n{human}"));
        let acme = |r: &serde_json::Value| -> Vec<String> {
            r["entries"]
                .as_array()
                .map(|es| {
                    es.iter()
                        .filter(|e| e["package"] == "org.acme")
                        .map(|e| e["category"].as_str().unwrap_or("").to_string())
                        .collect()
                })
                .unwrap_or_default()
        };
        let block_a = row_block(&human, "a")
            .unwrap_or_else(|| panic!("row a renders in the human output:\n{human}"));
        const DECLARED_CATS: [&str; 3] = [
            "declared_and_used",
            "declared_but_unobserved",
            "type_only_import",
        ];
        let declared = acme(row_a)
            .iter()
            .any(|c| DECLARED_CATS.contains(&c.as_str()))
            && block_names_in_group(
                &block_a,
                &["used", "no static import", "type-only import"],
                "org.acme",
            );
        let note = json["declared_undetermined_note"].as_str().unwrap_or("");
        let undetermined = row_a["manifest_scope_available"] == true
            && !acme(row_a)
                .iter()
                .any(|c| DECLARED_CATS.contains(&c.as_str()))
            && acme(row_a).iter().any(|c| c == "observed_but_undeclared")
            && json["declared_undetermined_blocks"].as_u64().unwrap_or(0) >= 1
            && note.starts_with("declared set may be incomplete: ")
            && note.contains("b/build.gradle:2")
            && note.ends_with("— investigate")
            && human.lines().any(|l| l == note)
            && block_a
                .iter()
                .any(|l| l.contains(" · undeclared ") && l.contains(" (declared set incomplete)"));
        assert!(
            declared ^ undetermined,
            "exactly one of DECLARED / UNDETERMINED on row a (declared={declared}, \
             undetermined={undetermined}):\n{json:#}\n{human}"
        );
        for r in rows.iter().filter(|r| r["module"] != "a") {
            assert!(
                acme(r).is_empty(),
                "org.acme appears on another row {}: {json:#}",
                r["module"]
            );
            let label = r["module"]
                .as_str()
                .filter(|m| !m.is_empty())
                .unwrap_or(".");
            if let Some(block) = row_block(&human, label) {
                assert!(
                    !block.iter().any(|l| l.contains("org.acme")),
                    "row {label}'s human block names org.acme:\n{human}"
                );
            }
        }
    }

    /// D-DGC-ATTRIBUTION-1 case (b), the reviewer's second counterexample, at the `deps list`
    /// boundary: root `build.gradle` with a direct block, no root settings file, and
    /// `zz/settings.gradle` present but unreadable. The index resolves `src/…` first (path order),
    /// recording `build.gradle` parsed before `zz/…` meets the failure; the failure wins, so no
    /// row reads `build.gradle` as parsed and a row states the reason in JSON and human.
    #[test]
    fn deps_list_gradle_failure_behind_a_parsed_script_is_unknown_with_reason_in_json_and_human() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path();
        put_file(
            root,
            "build.gradle",
            "dependencies {\n  implementation 'org.root:core:1.0'\n}\n",
        );
        std::fs::create_dir_all(root.join("zz/settings.gradle")).unwrap();
        put_file(
            root,
            "src/main/java/r/R.java",
            "package r;\nimport org.root.core.Core;\npublic class R { Core c; }\n",
        );
        put_file(
            root,
            "zz/src/main/java/z/Z.java",
            "package z;\nimport org.nested.Thing;\npublic class Z { Thing t; }\n",
        );
        let (json, human) = deps_list_java_through_the_daemon(root);
        let rows = json["results"].as_array().expect("results[]");
        assert!(!rows.is_empty(), "rows render: {json:#}\n{human}");
        assert!(
            !rows.iter().any(|r| r["manifest_path"] == "build.gradle"),
            "no row reads build.gradle as parsed: {json:#}"
        );
        assert!(
            rows.iter()
                .any(|r| r["manifest_context"].as_str().is_some_and(|c| {
                    c.contains("build.gradle")
                        && c.contains("zz/settings.gradle")
                        && c.contains("unreadable")
                })),
            "a row states the settings failure: {json:#}"
        );
        let headers = row_headers(&human, &json);
        assert!(
            !headers.iter().any(|h| h.ends_with("[build.gradle]")),
            "no human row header reads [build.gradle]:\n{human}"
        );
        // D-DGC-BOUNDARY-1 (9), CHANGED ORACLE (fixture unchanged): `build.gradle` was read and its
        // declaration retained, so the failure is an ATTRIBUTION failure — worded as such on EVERY
        // row and header (every file's nearest script is `build.gradle`), never "not parsed".
        let states_attribution = |c: &str| {
            c.contains("build.gradle — dependency attribution failed")
                && c.contains("zz/settings.gradle")
                && c.contains("unreadable")
                && !c.contains("not parsed")
        };
        for r in rows {
            let c = r["manifest_context"].as_str().unwrap_or("");
            assert!(
                states_attribution(c),
                "row {} states the attribution failure: {json:#}",
                r["module"]
            );
        }
        assert_eq!(headers.len(), rows.len(), "every row renders:\n{human}");
        for h in &headers {
            assert!(
                states_attribution(h),
                "human row header {h:?} states the attribution failure:\n{human}"
            );
        }
        assert!(
            rows.iter().any(
                |r| r["entries"].as_array().is_some_and(|es| es.iter().any(|e| {
                    e["package"] == "org.root"
                        && DECLARED_CATEGORIES.contains(&e["category"].as_str().unwrap_or(""))
                }))
            ),
            "a row holds the retained declaration org.root: {json:#}"
        );
    }

    /// The categories that assert a manifest declaration.
    const DECLARED_CATEGORIES: [&str; 3] = [
        "declared_and_used",
        "declared_but_unobserved",
        "type_only_import",
    ];

    /// Does row `r` hold an entry of a declared category, or one `observed_but_undeclared`?
    fn holds_declared_or_undeclared(r: &serde_json::Value) -> bool {
        r["entries"].as_array().is_some_and(|es| {
            es.iter().any(|e| {
                let c = e["category"].as_str().unwrap_or("");
                DECLARED_CATEGORIES.contains(&c) || c == "observed_but_undeclared"
            })
        })
    }

    /// D-DGC-BOUNDARY-1 (8), the ancestor case: a module covered ONLY by a failed manifest record
    /// (a root `build.gradle` present but unreadable — a parse failure) is in the java view and
    /// renders unknown with the reason, never parsed and never a measured-empty manifest.
    /// OC-7: the row rests on the `System` call (a `java.lang` runtime global, stored as an
    /// `external_library_candidate` reference); with its governing manifest failed, the file's
    /// declared set is empty, so the import is `unknown` and admits no row on its own. The same
    /// fact carries the two fixtures below.
    #[test]
    fn deps_list_module_covered_only_by_a_failed_manifest_is_in_the_view_unknown_with_reason_in_json_and_human(
    ) {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path();
        std::fs::create_dir_all(root.join("build.gradle")).unwrap();
        put_file(
            root,
            "src/main/java/p/P.java",
            "package p;\nimport org.x.Y;\npublic class P { Y y; void m() { System.out.println(\"x\"); } }\n",
        );
        let (json, human) = deps_list_java_through_the_daemon(root);
        let rows = json["results"].as_array().expect("results[]");
        assert_eq!(rows.len(), 1, "exactly one row: {json:#}\n{human}");
        let row = &rows[0];
        assert!(row["manifest_path"].is_null(), "{json:#}");
        let c = row["manifest_context"].as_str().unwrap_or("");
        assert!(
            c.contains("build.gradle")
                && c.contains("present but not parsed")
                && c.contains("unreadable"),
            "the row states the parse failure: {json:#}"
        );
        assert_eq!(row["manifest_scope_available"], false, "{json:#}");
        assert!(!holds_declared_or_undeclared(row), "{json:#}");
        let headers = row_headers(&human, &json);
        assert_eq!(headers.len(), 1, "exactly one row header:\n{human}");
        assert!(
            headers[0].contains("build.gradle present but not parsed")
                && headers[0].contains("unreadable"),
            "the header states the failure:\n{human}"
        );
        assert!(
            !headers.iter().any(|h| h.ends_with("[build.gradle]")),
            "no header reads [build.gradle] as parsed:\n{human}"
        );
    }

    /// D-DGC-BOUNDARY-1 (8), the nested case (document review-0 of PREP-7, F-1): a failed manifest
    /// record strictly inside the coarse inferred module `frontend` admits it to the view, and its
    /// row names the nested record with its path and reason — never `no manifest`.
    #[test]
    fn deps_list_module_over_a_nested_failed_manifest_states_its_path_and_reason_in_json_and_human()
    {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path();
        std::fs::create_dir_all(root.join("frontend/web/build.gradle")).unwrap();
        put_file(
            root,
            "frontend/web/src/main/java/w/W.java",
            "package w;\nimport org.x.Y;\npublic class W { Y y; void m() { System.out.println(\"x\"); } }\n",
        );
        let (json, human) = deps_list_java_through_the_daemon(root);
        let rows = json["results"].as_array().expect("results[]");
        assert_eq!(rows.len(), 1, "exactly one row: {json:#}\n{human}");
        let row = &rows[0];
        assert_eq!(row["module"], "frontend", "{json:#}");
        assert!(row["manifest_path"].is_null(), "{json:#}");
        let c = row["manifest_context"].as_str().unwrap_or("");
        assert!(
            c.contains("nested")
                && c.contains("frontend/web/build.gradle")
                && c.contains("present but not parsed")
                && c.contains("unreadable"),
            "the row names the nested failure: {json:#}"
        );
        assert_eq!(row["manifest_scope_available"], false, "{json:#}");
        assert!(!holds_declared_or_undeclared(row), "{json:#}");
        let headers = row_headers(&human, &json);
        assert_eq!(headers.len(), 1, "exactly one row header:\n{human}");
        assert!(
            headers[0].starts_with("frontend  [")
                && headers[0].contains("frontend/web/build.gradle present but not parsed")
                && headers[0].contains("unreadable"),
            "the header names the nested failure:\n{human}"
        );
        assert!(
            !human.contains("no manifest — imports unattributed"),
            "no row reads `no manifest`:\n{human}"
        );
    }

    /// D-DGC-BOUNDARY-1 (9), the same-build failure (document review-2 of PREP-7, F-2): the root
    /// script of the settings build is unreadable, so project `svc`'s declared set — which the
    /// root's scope blocks can feed — is unknown even though `svc/build.gradle` is readable. Row
    /// `svc` reads unknown with the attribution wording, never `[svc/build.gradle]` with `org.svc`.
    #[test]
    fn deps_list_gradle_unreadable_root_script_makes_the_child_project_row_unknown_with_attribution_wording_in_json_and_human(
    ) {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path();
        put_file(root, "settings.gradle", "include ':svc'\n");
        std::fs::create_dir_all(root.join("build.gradle")).unwrap();
        put_file(
            root,
            "svc/build.gradle",
            "dependencies {\n  implementation 'org.svc:lib:1.0'\n}\n",
        );
        put_file(
            root,
            "svc/src/main/java/s/S.java",
            "package s;\nimport org.svc.Lib;\npublic class S { Lib l; void m() { System.out.println(\"x\"); } }\n",
        );
        let (json, human) = deps_list_java_through_the_daemon(root);
        let rows = json["results"].as_array().expect("results[]");
        let row = rows
            .iter()
            .find(|r| r["module"] == "svc")
            .unwrap_or_else(|| panic!("row svc is present: {json:#}\n{human}"));
        assert!(row["manifest_path"].is_null(), "{json:#}");
        let c = row["manifest_context"].as_str().unwrap_or("");
        assert!(
            c.contains("svc/build.gradle")
                && c.contains("dependency attribution failed")
                && c.contains("unreadable")
                && !c.contains("not parsed"),
            "row svc states the attribution failure: {json:#}"
        );
        assert!(
            !row["entries"].as_array().is_some_and(|es| es
                .iter()
                .any(|e| DECLARED_CATEGORIES.contains(&e["category"].as_str().unwrap_or("")))),
            "no certain declaration from the readable child script: {json:#}"
        );
        let block = row_block(&human, "svc").unwrap_or_else(|| panic!("row svc renders:\n{human}"));
        assert!(
            block[0].contains("svc/build.gradle — dependency attribution failed")
                && block[0].contains("unreadable"),
            "the svc header states the attribution failure:\n{human}"
        );
        assert!(
            !row_headers(&human, &json)
                .iter()
                .any(|h| h.ends_with("[svc/build.gradle]")),
            "no header reads [svc/build.gradle] as parsed:\n{human}"
        );
    }

    // ── DEPS-GRADLE-CATALOG-1B (D-DGC1B-ALIAS-MARKING-1; RG-REQ-002-L11) — unbound alias references ──

    const ALIAS_NOTE: &str = "declared set may be incomplete: 1 alias reference(s) could not be resolved (build.gradle:5 libs.nope: no catalog entry) — investigate";

    /// The alias line renders once, right after the undetermined-blocks line; it does not add the
    /// row suffix (only the undetermined-blocks marking drives ` (declared set incomplete)`); without
    /// the undetermined line it still renders after the coverage line, before the first row.
    #[test]
    fn declared_unresolved_alias_refs_renders_one_line_after_the_undetermined_blocks_line() {
        let out = resp(marking_payload(serde_json::json!({
            "declared_undetermined_blocks": 5,
            "declared_undetermined_note": MARK_NOTE,
            "declared_unresolved_alias_refs": 1,
            "declared_unresolved_alias_note": ALIAS_NOTE
        })))
        .render_human();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(
            lines.iter().filter(|l| **l == ALIAS_NOTE).count(),
            1,
            "the alias line renders exactly once: {out}"
        );
        let mark = lines.iter().position(|l| *l == MARK_NOTE).unwrap();
        let alias = lines.iter().position(|l| *l == ALIAS_NOTE).unwrap();
        assert_eq!(
            alias,
            mark + 1,
            "right after the undetermined-blocks line: {out}"
        );

        let alone = resp(marking_payload(serde_json::json!({
            "declared_unresolved_alias_refs": 1,
            "declared_unresolved_alias_note": ALIAS_NOTE
        })))
        .render_human();
        let lines: Vec<&str> = alone.lines().collect();
        let alias = lines
            .iter()
            .position(|l| *l == ALIAS_NOTE)
            .expect("line present");
        let coverage = lines
            .iter()
            .position(|l| l.contains("java manifests attributed to a module"))
            .expect("coverage line present");
        let first_row = lines
            .iter()
            .position(|l| l.starts_with("  used "))
            .expect("row present");
        assert!(coverage < alias && alias < first_row, "{alone}");
        assert!(
            alone.contains("· undeclared 2 · builtins 3")
                && !alone.contains("(declared set incomplete)"),
            "the alias marking alone adds no row suffix: {alone}"
        );
    }

    /// An envelope without the two alias keys (an older daemon, untracked provenance) renders
    /// exactly as today; a zero count renders byte-identically to an absent one.
    #[test]
    fn envelope_without_alias_keys_renders_as_today() {
        let r = resp(marking_payload(serde_json::json!({})));
        assert_eq!(r.declared_unresolved_alias_refs, 0);
        assert_eq!(r.declared_unresolved_alias_note, "");
        let today = r.render_human();
        assert!(!today.contains("alias reference"), "{today}");
        let zero = resp(marking_payload(serde_json::json!({
            "declared_unresolved_alias_refs": 0,
            "declared_unresolved_alias_note": ""
        })))
        .render_human();
        assert_eq!(zero, today);
    }

    /// The fixture of DGC-B02 / DGC-B04 (vii): `settings.gradle`, a TOML catalog with `slf`, and a
    /// `build.gradle` whose lines 1-6 hold a literal, `libs.slf` and `libs.nope`.
    fn alias_fixture(root: &std::path::Path, toml: &str) {
        put_file(root, "settings.gradle", "rootProject.name = \"fx\"\n");
        put_file(root, "gradle/libs.versions.toml", toml);
        put_file(
            root,
            "build.gradle",
            "plugins { id \"java\" }\ndependencies {\n  implementation \"org.apache.commons:commons-lang3:3.14.0\"\n  implementation libs.slf\n  implementation libs.nope\n}\n",
        );
        put_file(
            root,
            "src/main/java/app/A.java",
            "package app;\nimport org.apache.commons.lang3.StringUtils;\nimport org.slf4j.LoggerFactory;\npublic class A {\n  boolean b() { LoggerFactory.getLogger(A.class); return StringUtils.isBlank(\"\"); }\n}\n",
        );
    }

    /// Is `group` on a row under a declared category (the entry's package equal to the group or
    /// extending it on a `.` boundary)?
    fn declared_on_a_row(json: &serde_json::Value, group: &str) -> bool {
        const DECLARED_CATS: [&str; 3] = [
            "declared_and_used",
            "declared_but_unobserved",
            "type_only_import",
        ];
        json["results"].as_array().into_iter().flatten().any(|r| {
            r["entries"].as_array().into_iter().flatten().any(|e| {
                let p = e["package"].as_str().unwrap_or("");
                DECLARED_CATS.contains(&e["category"].as_str().unwrap_or(""))
                    && (p == group || p.starts_with(&format!("{group}.")))
            })
        })
    }

    /// End to end through the real daemon: an alias with no catalog entry is counted and named —
    /// the JSON keys and the same human line — while the resolved alias and the literal beside it
    /// are declared.
    #[test]
    fn deps_list_alias_with_no_catalog_entry_is_counted_and_named_in_json_and_human() {
        // Two catalog sources answer `slf`: the TOML catalog, and (review-1 F2 and review-2 F2 of
        // admission 3) a readable Groovy alias map whose script also holds `println "libs"`, a
        // proven entry read and the map rendered as text (`println libs`, `"${libs}"`) — none
        // writes the map, so `slf` binds and only `libs.nope` is counted, with the reason
        // actually detected (`no catalog entry`).
        for groovy in [false, true] {
            let fixture = tempfile::tempdir().unwrap();
            let root = fixture.path();
            if groovy {
                alias_fixture(root, "");
                std::fs::remove_file(root.join("gradle/libs.versions.toml")).unwrap();
                let script = std::fs::read_to_string(root.join("build.gradle")).unwrap();
                put_file(
                    root,
                    "build.gradle",
                    &format!("{script}apply from: \"gradle/deps.gradle\"\n"),
                );
                put_file(
                root,
                "gradle/deps.gradle",
                "ext { libs = [:] }\nlibs += [ slf: \"org.slf4j:slf4j-api:2.0.9\" ]\nprintln \"libs\"\nprintln libs.slf\nprintln libs\nprintln \"${libs}\"\n",
            );
            } else {
                alias_fixture(root, "[libraries]\nslf = \"org.slf4j:slf4j-api:2.0.9\"\n");
            }
            let (json, human) = deps_list_java_through_the_daemon(root);
            assert_eq!(
                json["declared_unresolved_alias_refs"],
                serde_json::json!(1),
                "{json:#}"
            );
            assert_eq!(
                json["declared_unresolved_alias_note"],
                serde_json::json!(ALIAS_NOTE)
            );
            assert!(human.lines().any(|l| l == ALIAS_NOTE), "{human}");
            assert!(declared_on_a_row(&json, "org.slf4j"), "{json:#}\n{human}");
            assert!(
                declared_on_a_row(&json, "org.apache.commons"),
                "{json:#}\n{human}"
            );
        }
    }

    /// End to end through the real daemon: the aliases of a build whose catalog cannot be read are
    /// all counted with the `could not be read` reason; no alias binds, and the literal declaration
    /// stays declared.
    #[test]
    fn deps_list_alias_of_a_build_whose_catalog_cannot_be_read_is_counted_and_named_in_json_and_human(
    ) {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path();
        alias_fixture(root, "[libraries]\nslf = { module = \n");
        let (json, human) = deps_list_java_through_the_daemon(root);
        assert_eq!(
            json["declared_unresolved_alias_refs"],
            serde_json::json!(2),
            "{json:#}"
        );
        let note = json["declared_unresolved_alias_note"]
            .as_str()
            .unwrap_or("");
        assert!(
            note.starts_with(
                "declared set may be incomplete: 2 alias reference(s) could not be resolved (build.gradle:4 libs.slf: catalog gradle/libs.versions.toml could not be read ("
            ) && note.ends_with(") — investigate"),
            "{note}"
        );
        assert!(human.lines().any(|l| l == note), "{human}");
        assert!(!declared_on_a_row(&json, "org.slf4j"), "{json:#}\n{human}");
        assert!(
            declared_on_a_row(&json, "org.apache.commons"),
            "{json:#}\n{human}"
        );
    }

    /// Is `group` under a declared category on the row whose `module` is `module`?
    fn declared_on_module_row(json: &serde_json::Value, module: &str, group: &str) -> bool {
        let row: Vec<&serde_json::Value> = json["results"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| r["module"] == serde_json::json!(module))
            .collect();
        assert_eq!(row.len(), 1, "one row for module {module}: {json:#}");
        declared_on_a_row(&serde_json::json!({ "results": row }), group)
    }

    /// End to end through the real daemon (D-DGC1B-ALIAS-APPLICABILITY-1, the record's
    /// counterexample): a rename in project `a`'s script applies to `a` only — `a`'s reference binds,
    /// `b`'s is counted naming the rename, in the JSON keys and the same human line; `b`'s literal
    /// stays declared.
    #[test]
    fn deps_list_alias_whose_rename_does_not_apply_to_its_project_is_counted_and_named_in_json_and_human(
    ) {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path();
        put_file(
            root,
            "settings.gradle",
            "rootProject.name = \"fx\"\ninclude 'a', 'b'\n",
        );
        put_file(
            root,
            "gradle/libs.versions.toml",
            "[libraries]\nfoo = \"org.slf4j:slf4j-api:2.0.9\"\n",
        );
        put_file(
            root,
            "a/build.gradle",
            "ext {\n  libraries = libs\n}\ndependencies {\n  implementation libraries.foo\n}\n",
        );
        put_file(
            root,
            "b/build.gradle",
            "dependencies {\n  implementation \"org.apache.commons:commons-lang3:3.14.0\"\n  implementation libraries.foo\n}\n",
        );
        put_file(
            root,
            "a/src/main/java/a/A.java",
            "package a;\nimport org.slf4j.LoggerFactory;\npublic class A {\n  void f() { LoggerFactory.getLogger(A.class); }\n}\n",
        );
        put_file(
            root,
            "b/src/main/java/b/B.java",
            "package b;\nimport org.apache.commons.lang3.StringUtils;\nimport org.slf4j.LoggerFactory;\npublic class B {\n  boolean f() { LoggerFactory.getLogger(B.class); return StringUtils.isBlank(\"\"); }\n}\n",
        );
        let (json, human) = deps_list_java_through_the_daemon(root);
        let line = "declared set may be incomplete: 1 alias reference(s) could not be resolved (b/build.gradle:3 libraries.foo: rename at a/build.gradle:2 not shown to apply to this project) — investigate";
        assert_eq!(
            json["declared_unresolved_alias_refs"],
            serde_json::json!(1),
            "{json:#}"
        );
        assert_eq!(
            json["declared_unresolved_alias_note"],
            serde_json::json!(line)
        );
        assert!(human.lines().any(|l| l == line), "{human}");
        assert!(
            declared_on_module_row(&json, "a", "org.slf4j"),
            "{json:#}\n{human}"
        );
        assert!(
            declared_on_module_row(&json, "b", "org.apache.commons"),
            "{json:#}\n{human}"
        );
        assert!(
            !declared_on_module_row(&json, "b", "org.slf4j"),
            "{json:#}\n{human}"
        );
    }

    /// End to end through the real daemon (D-DGC1B-MAP-GRAMMAR-1): a Groovy alias map one of whose
    /// assignments sits under a condition is unreadable as a whole — its unconditional `slf` entry
    /// binds nothing — and the reference is counted naming that statement, in the JSON keys and the
    /// same human line; the literal beside it stays declared.
    #[test]
    fn deps_list_alias_whose_map_is_not_a_top_level_literal_assignment_is_counted_and_named_in_json_and_human(
    ) {
        // The record's fixture (a conditional `+=`), and review-0 F1's two write forms of admission
        // 3 (safe navigation, a quoted key) after an entry that would otherwise bind `slf`.
        for (deps, line_no) in [
            (
                "ext { libs = [:] }\nlibs += [ slf: \"org.slf4j:slf4j-api:2.0.9\" ]\nif (project.hasProperty(\"extra\")) {\n  libs += [ extra: \"com.acme:extra:1.0\" ]\n}\n",
                4,
            ),
            (
                "ext { libs = [:] }\nlibs += [ slf: \"org.slf4j:slf4j-api:2.0.9\" ]\nlibs?.put(\"slf\", \"com.other:x:1\")\n",
                3,
            ),
            (
                "ext { libs = [:] }\nlibs += [ slf: \"org.slf4j:slf4j-api:2.0.9\" ]\nlibs.\"slf\" = \"com.other:x:1\"\n",
                3,
            ),
            // Review-2 F3 of admission 3: a quoted property write replaces the map; neither the
            // old group nor the new one binds.
            (
                "ext { libs = [:] }\nlibs += [ slf: \"org.slf4j:slf4j-api:2.0.9\" ]\next.\"libs\" = [ slf: \"com.other:x:1\" ]\n",
                3,
            ),
            (
                "ext { libs = [:] }\nlibs += [ slf: \"org.slf4j:slf4j-api:2.0.9\" ]\next.'libs' = [ slf: \"com.other:x:1\" ]\n",
                3,
            ),
        ] {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path();
        put_file(root, "settings.gradle", "rootProject.name = \"fx\"\n");
        put_file(
            root,
            "build.gradle",
            "plugins { id \"java\" }\napply from: \"gradle/deps.gradle\"\ndependencies {\n  implementation \"org.apache.commons:commons-lang3:3.14.0\"\n  implementation libs.slf\n}\n",
        );
        put_file(root, "gradle/deps.gradle", deps);
        put_file(
            root,
            "src/main/java/app/A.java",
            "package app;\nimport org.apache.commons.lang3.StringUtils;\nimport org.slf4j.LoggerFactory;\npublic class A {\n  boolean b() { LoggerFactory.getLogger(A.class); return StringUtils.isBlank(\"\"); }\n}\n",
        );
        let (json, human) = deps_list_java_through_the_daemon(root);
        let line = format!("declared set may be incomplete: 1 alias reference(s) could not be resolved (build.gradle:5 libs.slf: alias map in gradle/deps.gradle:{line_no} is not a top-level literal assignment) — investigate");
        assert_eq!(
            json["declared_unresolved_alias_refs"],
            serde_json::json!(1),
            "{json:#}"
        );
        assert_eq!(
            json["declared_unresolved_alias_note"],
            serde_json::json!(line)
        );
        assert!(human.lines().any(|l| l == line), "{human}");
        assert!(!declared_on_a_row(&json, "org.slf4j"), "{json:#}\n{human}");
        assert!(
            declared_on_a_row(&json, "org.apache.commons"),
            "{json:#}\n{human}"
        );
        assert!(!declared_on_a_row(&json, "com.other"), "{json:#}\n{human}");
        }
    }
}
