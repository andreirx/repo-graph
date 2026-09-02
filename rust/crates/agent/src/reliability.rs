//! RELIABILITY-REFRAME-1: the ONE reader-frame call-reliability projection.
//!
//! Every reader surface that talks about call-graph reliability — `orient`'s
//! headline caveat + `--full` Degradation, `trust`'s Resolution + Reliability +
//! external coverage map, `check`'s CALL_GRAPH_RELIABILITY condition, and the
//! `explain` trust signal — derives its reader-facing numbers and prose from
//! [`CallReliabilityView`] here, instead of each re-deriving them. That
//! per-surface re-derivation is exactly what let `trust`'s "Calls: X% resolved"
//! drift EXTERNAL-INCLUSIVE (`resolved / (resolved + unresolved)`) while every
//! other surface stayed in-scope — the MODULE-MODEL lesson the slice §4 cites.
//!
//! ## Why this lives in `agent` (not in the `rgr` presentation crate)
//!
//! `check`'s reader surface is [`crate::check::evaluate`], a pure reducer IN THIS
//! CRATE whose `ConditionResult.summary` is rendered ~verbatim by the CLI. The
//! `trust` / `orient` reader surfaces are in `rgr`, which depends on `agent`. So
//! `agent` is the ONLY crate reachable by all three (a helper in `rgr` — where
//! the prior iteration put it — can never reach `check`). `agent` already emits
//! reader-facing prose (`dto::signal`, `check::evaluate`); consolidating the
//! call-reliability vocabulary here is consistent with that, and outer→inner for
//! the `rgr` renderers.
//!
//! This crate does NOT depend on `repo-graph-trust`, so the named-target input is
//! a NEUTRAL [`ExternalTarget`] the caller builds from trust's `top_types` — the
//! trust type never crosses into `agent`.
//!
//! ## Frame (VISION: labels speak the reader's language)
//!
//! We report where the READER's calls go; we never grade repo-graph's pipeline.
//!
//!   - "your code's calls M% resolved" — the IN-SCOPE rate: resolved over
//!     in-scope references only. Calls whose receiver is an external library /
//!     std / primitive type are OUT of source scope (unresolvable by design) and
//!     are EXCLUDED from the denominator. This is `resolved /
//!     (resolved + internal_like)` — the value the trust service already computes
//!     as `call_resolution_rate` and the reliability band already scores. `None`
//!     (no in-scope calls) renders "no in-scope calls measured" — unknown, NOT a
//!     fabricated 100% (slice §3 / architecture rule 6: null = unknown).
//!   - "N% of calls go into external libraries — follow to their crates/docs" —
//!     the external SHARE, named CONTEXT, not a grade. When the heuristic
//!     identifies ZERO external calls (but calls exist) the line reads "no
//!     external-library calls identified (heuristic)" — a heuristic finding, NOT
//!     a measured absence and never a fabricated "0% external" (review-3 §2).
//!   - the NAMED coverage map — the top external receiver types with honest
//!     heuristic-basis markers (EY1-A): orientation, never a Layer-0 edge claim.
//!   - a CONSERVATIVE-rate caveat — the in-scope denominator counts every call
//!     NOT identified as external-library, so it INCLUDES calls the classifier
//!     could not attribute (unclassified). When that unclassified share is
//!     material the rate is a lower bound; [`unclassified_caveat`] says so, in the
//!     reader's frame (review-3 §2 — unknown ≠ known-internal).
//!
//! The reliability BAND (LOW/MEDIUM/HIGH) is computed upstream on the in-scope
//! rate (`repo_graph_trust::rules::compute_call_graph_reliability`); this module
//! only LABELS it. Genuine in-scope failure still reads low (slice §3).

use crate::storage_port::AgentReliabilityLevel;

/// A named external receiver target — Layer-2 ORIENTATION (heuristic), NOT a
/// resolved edge. `type_name` is a receiver type the reader's call lands on
/// (e.g. `serde_json::Value`, `tokio`); the reader follows it to that crate's
/// docs. Built by the caller from trust's `top_types` (external subset only), so
/// the `repo-graph-trust` type never crosses into `agent`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalTarget {
    pub type_name: String,
    pub count: u64,
}

/// The reader's call-resolution rate: `resolved` over the "in-scope OR unclassified"
/// denominator ([`Self::in_scope_or_unclassified_total`]). Only calls the classifier
/// KNOWS are external-library receivers are excluded; every other call — genuinely
/// in-scope AND unclassified (target undetermined) — stays IN. That makes the
/// denominator CONSERVATIVE: unclassified calls are never dropped, so the rate is a
/// lower bound and can never be inflated. Because the denominator is NOT purely
/// "known in-scope", reader labels say "in-scope or unclassified" and, when the
/// unclassified share is material, [`unclassified_caveat`] fires (review-3 §2 /
/// review-5 §1).
///
/// Named `ResolvedRate` — NOT `InScopeResolution` — precisely because the
/// denominator is not purely in-scope (review-5 §1: a name that claims "in-scope"
/// for a denominator that also holds unclassified calls is a false-certainty
/// defect). Present only when there is at least one such call; a zero-call repo has
/// `None` (unknown, never a fabricated 100%).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedRate {
    pub resolved: u64,
    /// The rate's denominator: resolved + all unresolved calls EXCEPT known-external
    /// = the genuinely in-scope calls PLUS the unclassified ones. NOT purely in-scope
    /// (hence the field name); the reader label spells "in-scope or unclassified" out.
    pub in_scope_or_unclassified_total: u64,
    pub pct: f64,
}

impl ResolvedRate {
    /// `None` when there are no in-scope-or-unclassified calls at all (nothing to measure).
    fn derive(resolved: u64, internal_like: u64) -> Option<Self> {
        let in_scope_or_unclassified_total = resolved + internal_like;
        if in_scope_or_unclassified_total == 0 {
            return None;
        }
        Some(Self {
            resolved,
            in_scope_or_unclassified_total,
            pct: resolved as f64 / in_scope_or_unclassified_total as f64 * 100.0,
        })
    }
}

/// The external-library share of ALL calls — reader CONTEXT, not a grade. The
/// denominator is every CALLS edge (resolved or not): "of everything your code
/// calls, this fraction leaves your source". Distinct from the resolved rate's
/// denominator.
///
/// review-5 §2 (architecture rule 6: `null` = unknown, `0` = known-zero — never
/// conflate): a KNOWN-ZERO external share (`external == 0` but calls exist) is
/// PRESERVED as `Some(ExternalShare { external: 0, pct: 0.0, .. })` — the heuristic
/// ran and matched none, a measured finding — NOT collapsed to `None`. Only a repo
/// with NO calls at all (`total_calls == 0`) is `None` (unknown, nothing to
/// measure). [`CallReliabilityView::external_line`] renders the known-zero case as
/// the honest "none identified (heuristic)", never a fabricated "0% external".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExternalShare {
    pub external: u64,
    pub total_calls: u64,
    pub pct: f64,
}

impl ExternalShare {
    /// `Some` whenever there is at least one call — INCLUDING the known-zero external
    /// case (`external == 0`, preserved with `pct == 0.0`; rule 6). `None` ONLY when
    /// there are no calls at all (`total_calls == 0`) — the sole genuinely-unknown case.
    fn derive(external: u64, total_calls: u64) -> Option<Self> {
        if total_calls == 0 {
            return None;
        }
        Some(Self {
            external,
            total_calls,
            pct: external as f64 / total_calls as f64 * 100.0,
        })
    }
}

/// The ONE reader-frame call-reliability projection (RELIABILITY-REFRAME-1).
///
/// Derived ONCE by [`CallReliabilityView::derive`]; consumed by `orient`,
/// `trust`, and `check` — all three carry the FULL projection (in-scope rate,
/// external share, named targets). Fields are optional so each surface supplies
/// the facts it HAS, not to ration the projection: a zero-call repo has no
/// in-scope resolution, and an overlay predating `call_coverage` (an older
/// daemon) has no counts to build a named list. The render helpers below are the
/// single source of the reader-frame WORDING; each surface composes them into its
/// own (density-appropriate) layout.
#[derive(Debug, Clone, PartialEq)]
pub struct CallReliabilityView {
    /// The reader's call-resolution rate. `None` = no in-scope-or-unclassified calls
    /// measured (unknown, never a fabricated 100%).
    pub resolution: Option<ResolvedRate>,
    /// The external-library share of all calls. A KNOWN-ZERO share (heuristic ran,
    /// matched none) is PRESERVED as `Some(ExternalShare { external: 0, .. })`
    /// (review-5 §2 / rule 6); `None` ONLY when there are no calls at all. The
    /// reader-facing "identified none (heuristic)" vs "0% external" distinction is
    /// drawn by [`Self::external_line`], never rendered as a fabricated "0% external".
    pub external: Option<ExternalShare>,
    /// Every CALLS edge (resolved or not). Retained for the surfaces that branch on
    /// "are there any calls at all?" (e.g. orient's zero-in-scope headline).
    pub total_calls: u64,
    /// Top named external receiver targets (already external-filtered, count-desc).
    pub named_targets: Vec<ExternalTarget>,
    /// The reliability band (LOW/MEDIUM/HIGH). `None` = unavailable.
    pub band: Option<AgentReliabilityLevel>,
}

impl CallReliabilityView {
    /// The ONE derivation. `resolved`/`internal_like` → in-scope resolution;
    /// `external`/`total_calls` → external share; `named_targets` are the caller's
    /// external-filtered receivers; `band` is the upstream reliability level.
    ///
    /// A surface with no external facts at all passes `external = 0, total_calls = 0`
    /// (→ `external: None`, nothing to measure); `external = 0` WITH calls present
    /// yields the preserved known-zero share (review-5 §2). A surface with no named
    /// list passes `named_targets = vec![]`.
    pub fn derive(
        resolved: u64,
        internal_like: u64,
        external: u64,
        total_calls: u64,
        named_targets: Vec<ExternalTarget>,
        band: Option<AgentReliabilityLevel>,
    ) -> Self {
        Self {
            resolution: ResolvedRate::derive(resolved, internal_like),
            external: ExternalShare::derive(external, total_calls),
            total_calls,
            named_targets,
            band,
        }
    }

    /// The reader-frame in-scope phrase: "your code's calls M% resolved", or
    /// "no in-scope calls measured" when there is nothing to measure (slice §3 —
    /// unknown, never a fabricated 100%).
    pub fn resolved_phrase(&self) -> String {
        match &self.resolution {
            Some(r) => resolved_phrase_pct(r.pct),
            None => NO_IN_SCOPE_CALLS.to_string(),
        }
    }

    /// The in-scope phrase with the band appended: "your code's calls M% resolved
    /// (LOW)" (orient's compressed caveat, check's condition). The band rides the
    /// line only when there IS an in-scope rate to band; the no-in-scope-calls
    /// case stays bare (a band over zero calls is vacuous). Routes through
    /// [`resolved_phrase_with_band`] so the "(BAND)" convention has ONE home.
    pub fn resolved_with_band(&self) -> String {
        match (&self.resolution, self.band) {
            (Some(r), Some(b)) => resolved_phrase_with_band(r.pct, band_label(b)),
            _ => self.resolved_phrase(),
        }
    }

    /// The external-coverage reader line.
    ///
    ///   - external calls identified → "N% of calls go into external libraries …";
    ///   - ZERO external identified but calls exist → "no external-library calls
    ///     identified (heuristic …)" — a heuristic FINDING, never a fabricated "0%"
    ///     and never a silent omission the reader would read as measured absence
    ///     (review-3 §2 / operator iteration-4 §2);
    ///   - no calls at all → `None` (nothing to say).
    pub fn external_line(&self) -> Option<String> {
        match self.external {
            // Positive external share — the reader-facing context line.
            Some(e) if e.external > 0 => Some(format!(
                "{:.0}% of calls go into external libraries — follow to their crates/docs",
                e.pct
            )),
            // KNOWN-ZERO (heuristic ran, matched none; review-5 §2 / rule 6) — a
            // measured finding, never a fabricated "0%" and never a silent omission.
            Some(_) => Some(NO_EXTERNAL_IDENTIFIED.to_string()),
            // No calls at all — nothing to say (unknown).
            None => None,
        }
    }

    /// One named-target bullet: "call on likely-external receiver `X` (N calls)"
    /// (trust's detailed EY1-A section). Singular "call" at count 1.
    pub fn named_target_line(t: &ExternalTarget) -> String {
        let calls = if t.count == 1 { "call" } else { "calls" };
        format!(
            "call on likely-external receiver `{}` ({} {})",
            t.type_name, t.count, calls
        )
    }

    /// A compact one-line named coverage map for orient's `--full` Degradation:
    /// "External coverage (heuristic): `Value` (30), `Vec` (12) — follow to their
    /// crates/docs". `None` when no external target is named. `limit` caps the
    /// listed targets (the rest are summarised as "+N more") so the compressed
    /// surface never turns into a wall of names.
    pub fn named_coverage_map_line(&self, limit: usize) -> Option<String> {
        if self.named_targets.is_empty() {
            return None;
        }
        let shown: Vec<String> = self
            .named_targets
            .iter()
            .take(limit)
            .map(|t| format!("`{}` ({})", t.type_name, t.count))
            .collect();
        let more = self.named_targets.len().saturating_sub(limit);
        let tail = if more > 0 {
            format!(", +{more} more")
        } else {
            String::new()
        };
        Some(format!(
            "External coverage (heuristic): {}{} — follow to their crates/docs",
            shown.join(", "),
            tail
        ))
    }
}

/// The reader-frame in-scope resolution phrase from a raw percentage — the SINGLE
/// source of the "your code's calls M% resolved" wording. Both the counts-based
/// [`CallReliabilityView::resolved_phrase`] and the `explain` trust signal (which
/// carries only a pre-computed rate, no counts) call THIS, so the wording never
/// forks across surfaces.
pub fn resolved_phrase_pct(pct: f64) -> String {
    format!("your code's calls {pct:.0}% resolved")
}

/// The reader-frame in-scope phrase with a parenthesized band label — "your code's
/// calls M% resolved (LOW)". The ONE home for the "(BAND)" convention, so check
/// (which has a typed [`AgentReliabilityLevel`] via [`CallReliabilityView`]) and
/// `orient` (which sometimes has only the serialized band string, no counts to
/// build a full view) render the banded line identically. Builds on the SAME
/// [`resolved_phrase_pct`] wording — the band is the only thing appended.
pub fn resolved_phrase_with_band(pct: f64, band: &str) -> String {
    format!("{} ({band})", resolved_phrase_pct(pct))
}

/// Capitalise the first character so a mid-phrase reader-frame string ("your
/// code's calls 39% resolved (LOW)") reads as a standalone sentence ("Your code's
/// calls 39% resolved (LOW).") when a surface emits it as its own sentence rather
/// than a bullet. The rest of the string is untouched (Unicode-aware first char).
///
/// Lives here beside the reader-frame wording it capitalises, with two concrete
/// callers in this crate: `check::evaluate` (the CALL_GRAPH_RELIABILITY condition
/// summary) and `dto::signal::trust_low_resolution` (the TRUST_LOW_RESOLUTION
/// signal summary). Consolidating the byte-identical copies keeps the reader-frame
/// presentation from forking (the same reason `resolved_phrase*` co-live here).
pub fn sentence_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Map a serialized reliability band ("HIGH"/"MEDIUM"/"LOW", any case) back to
/// [`AgentReliabilityLevel`]. `orient` deserializes the band as a wire STRING but
/// builds a [`CallReliabilityView`] (which carries the typed band) from its trust
/// overlay; this is the single adapter so orient's string→enum mapping does not
/// fork the band vocabulary out of this module. `None` for an unrecognized token
/// (unknown ≠ a fabricated band).
pub fn band_from_wire(level: &str) -> Option<AgentReliabilityLevel> {
    match level.to_ascii_uppercase().as_str() {
        "HIGH" => Some(AgentReliabilityLevel::High),
        "MEDIUM" => Some(AgentReliabilityLevel::Medium),
        "LOW" => Some(AgentReliabilityLevel::Low),
        _ => None,
    }
}

/// CHECK-LANG-SPLIT-1 (§2): ONE cell of the per-language reliability breakdown line —
/// "TypeScript 24% of 99 calls", or the shared UNKNOWN phrase ("TypeScript no in-scope
/// calls measured") when the language has no in-scope calls to measure. `resolved` /
/// `internal_like` are that language's summed CALLS counts (the SAME quantities
/// `reliability --by-language` feeds through [`ResolvedRate::derive`]); the "% of M
/// calls" is the in-scope resolved rate over its `in_scope_or_unclassified_total`
/// denominator, so the figure agrees with what `reliability --by-language` renders for
/// the same facts. Lives HERE (not in the daemon) so the "% of M calls" reader-frame
/// wording and the UNKNOWN decision have ONE home beside `resolved_phrase_pct` — the
/// same anti-fork reason those helpers co-live here. `display` is the reader display
/// name the caller (daemon) supplies from its language vocabulary (this crate stays
/// toolchain-agnostic).
///
/// `pub` (review-0 `public-language-cell-api`, operator ruling 2026-09-02): its sole cross-crate caller,
/// `daemon-runtime::reliability_breakdown_line::reliability_by_language_line`, genuinely needs it — the private
/// `ResolvedRate::derive` rate math + the "% of M calls" / UNKNOWN reader-frame wording live HERE, and the
/// rejected alternative (compose the cell in the daemon) would have to publicize `ResolvedRate::derive` +
/// its fields AND fork that wording into the daemon: strictly MORE surface, not less. RATIFIED under the
/// read-only reader-frame-wording precedent chain (8th instance) — the sibling `resolved_phrase_pct` /
/// `resolved_phrase_with_band` / `sentence_case` / `band_from_wire` helpers in this module are already
/// `pub` and consumed cross-crate (daemon + rgr) for the identical anti-fork reason.
pub fn language_reliability_cell(display: &str, resolved: u64, internal_like: u64) -> String {
    match ResolvedRate::derive(resolved, internal_like) {
        Some(r) => format!(
            "{display} {:.0}% of {} calls",
            r.pct, r.in_scope_or_unclassified_total
        ),
        None => format!("{display} {NO_IN_SCOPE_CALLS}"),
    }
}

/// The reader-frame text rendered when there are no in-scope calls to measure —
/// unknown, NOT a fabricated 100% (slice §3).
pub const NO_IN_SCOPE_CALLS: &str = "no in-scope calls measured";

/// The reader-frame text rendered when the external-attribution heuristic
/// identified ZERO external-library calls (but calls exist). This is a heuristic
/// FINDING ("we looked, by name-set match, and matched none"), NOT a measured
/// absence and NOT a fabricated "0% external" (review-3 §2). The name-set basis is
/// stated inline so the reader knows it is not compiler-verified.
pub const NO_EXTERNAL_IDENTIFIED: &str =
    "no external-library calls identified (heuristic name-set match, not compiler-verified)";

/// TRUST-FIRSTPARTY-1 (spec §2.3): the basis line stating the external/first-party split behind
/// the "% of calls go into external libraries" figure (CONTRADICTION-SWEEP-1 pattern — state the
/// basis inline so a reader never mistakes the corrected external figure for a contradiction).
/// `external` = the calls counted toward the external %; `internal_workspace` = the repo-own
/// workspace-crate calls EXCLUDED from it. Rendered only when there IS a first-party split
/// (`internal_workspace > 0`), so repos without repo-own workspace references stay byte-identical.
pub fn external_first_party_split_line(external: u64, internal_workspace: u64) -> String {
    format!(
        "basis: {external} external, {internal_workspace} internal workspace references \
         (this repo's own crates, excluded from the external figure above)"
    )
}

/// TRUST-FIRSTPARTY-1 (review-1 §2): the external SHARE is UNKNOWN because the counted first-party
/// (repo-own workspace) call count EXCEEDS the total external-import call count — the two do not
/// reconcile, so NO honest external figure exists. Rendered instead of a saturated-to-zero share
/// that would fabricate a measured "no external calls" (architecture rule 6 / STANDING HONESTY
/// RULE 1: unknown renders as unknown WITH REASON, never as a measured 0). Unreachable in a
/// coherent report — a coherent snapshot always has `first_party_calls <= unresolved_calls_external`
/// (first-party is a subset of the external-import calls); it guards a corrupt or cross-version
/// snapshot.
pub fn external_share_unreconciled_line(
    external_import_calls: u64,
    first_party_calls: u64,
) -> String {
    format!(
        "external-library share unavailable: {first_party_calls} first-party (repo-own workspace) \
         calls exceed the {external_import_calls} external-import calls counted — the snapshot is \
         internally inconsistent (possibly cross-version or corrupt); no honest external figure can \
         be derived"
    )
}

/// A call in the in-scope denominator is "unclassified" when the classifier could
/// not attribute its target. The rate keeps these IN (conservative — never inflate),
/// but at or above this fraction of the denominator the rate is materially a lower
/// bound, so [`unclassified_caveat`] fires.
///
/// **"Material" is defined as ≥ 20% of the in-scope denominator** (iteration-5 §2:
/// record what "material" means). 0.20 mirrors the aggregator's
/// `LOW_RESOLUTION_THRESHOLD` — a share this size can move the rate across a whole
/// reliability band, so it is not cosmetic; below it, the caveat would be noise. The
/// operator's iteration-5 note SUGGESTED >25%; 20% is retained deliberately because
/// it is the ONE principled anchor (the band-width threshold) rather than a free
/// number, and because a lower gate surfaces the honesty caveat MORE often — the
/// safe direction for a tool whose VISION is honesty about certainty. Raising it
/// would suppress honest caveats on repos between 20% and 25% unclassified.
pub const MATERIAL_UNCLASSIFIED_FRACTION: f64 = 0.20;

/// The reader-frame conservative-rate caveat (review-3 §2 / slice §2 degraded path):
/// the in-scope denominator INCLUDES `unclassified` calls whose target (the reader's
/// own code vs an external library) the classifier could not determine, so the rate
/// is a lower bound. `None` when there is nothing to measure or the unclassified
/// share is immaterial (< [`MATERIAL_UNCLASSIFIED_FRACTION`]) — the label alone
/// ("in-scope or unclassified") then carries the honesty without a noisy caveat.
///
/// One home for this wording, called by every surface that renders the rate
/// (`trust` Resolution, `orient` External-calls, `check` CALL_GRAPH_RELIABILITY),
/// so the degraded-path caveat cannot fork.
pub fn unclassified_caveat(
    unclassified: u64,
    in_scope_or_unclassified_total: u64,
) -> Option<String> {
    if in_scope_or_unclassified_total == 0 || unclassified == 0 {
        return None;
    }
    let share = unclassified as f64 / in_scope_or_unclassified_total as f64;
    if share < MATERIAL_UNCLASSIFIED_FRACTION {
        return None;
    }
    Some(format!(
        "conservative: {unclassified} of these {in_scope_or_unclassified_total} calls are \
         unclassified (target undetermined — your code or an external library), so the true \
         resolved share may be higher"
    ))
}

/// EY1-A honesty basis markers for the named coverage map — the two heuristics
/// have DISTINCT provenance and must not be conflated into one claim. Shared by
/// trust's detailed section (each on its own bullet) so the honesty labels never
/// fork. Internal constant names (STD_TYPES/PRIMITIVES/NODE_TYPES) stay OFF the
/// reader surface (VISION: labels speak the reader's language).
pub const RECEIVER_TYPE_BASIS: &str =
    "receiver-type basis: inferred from a language-server type hover, heuristically parsed";
pub const EXTERNAL_CLASSIFICATION_BASIS: &str =
    "external-classification basis: matched a static name-set of well-known std/library type \
     names and language primitives — not compiler-verified";
pub const ORIENTATION_ONLY_BASIS: &str =
    "orientation only, not resolved call-graph edges (never a Layer-0 CALLS edge)";

/// A COMPACT single-line rendering of BOTH EY1-A heuristic bases, for the
/// compressed `orient --full` coverage map (RELIABILITY-REFRAME-1 review-1 §2).
/// `trust` renders the two bases as separate verbose bullets; `orient`'s dense
/// surface folds them into ONE line — but still names BOTH distinct provenances
/// (the receiver TYPE from a language-server hover; the EXTERNAL classification
/// from a static std/library name-set, NOT compiler-verified) plus the
/// orientation-only framing, so orient's map is as honest as trust's detailed
/// section, just denser. Lives here (not in `orient`) so the two densities share
/// ONE home and cannot fork — the same reason `resolved_phrase` and
/// `resolved_with_band` co-live here.
pub const COMPACT_HEURISTIC_BASES: &str =
    "basis (heuristic, orientation only): receiver types inferred from a language-server type \
     hover; externality matched to a static std/library name-set, not compiler-verified";

/// The uppercase reader-facing band label ("LOW"/"MEDIUM"/"HIGH").
pub fn band_label(b: AgentReliabilityLevel) -> &'static str {
    match b {
        AgentReliabilityLevel::Low => "LOW",
        AgentReliabilityLevel::Medium => "MEDIUM",
        AgentReliabilityLevel::High => "HIGH",
    }
}

/// Convert a machine reliability reason token to reader-frame prose.
///
/// Consolidates the two byte-for-byte-divergent copies that lived in `trust.rs`
/// and `orient_reliability.rs`. The `call_resolution_rate=..` reason is reframed
/// to the reader's terms ("your code's calls M% resolved (below N% target)") via
/// the SAME [`resolved_phrase_pct`] wording; the other tokens (imports / alias /
/// entrypoints / registry) keep their existing prose.
pub fn humanize_reason(reason: &str) -> String {
    humanize_reason_impl(reason, false)
}

/// COHERENCE-POLISH-1 §2: the reader-frame reason humanizer for an AT-CEILING repo — identical to
/// [`humanize_reason`] EXCEPT it DROPS the "(below N% target)" clause from the `call_resolution_rate`
/// reason. On a permanent no-resolver ceiling the rate cannot approach that target (there is no
/// resolver to close the gap), so naming a target the reader can never meet implies an unimprovable
/// number can improve. The ceiling sentence ([`call_graph_ceiling_note`]) carries the WHY beside it.
/// Only the Call-graph axis carries a `call_resolution_rate` reason, so the caller applies this only
/// there; every other reason renders byte-identically to [`humanize_reason`].
pub fn humanize_reason_at_ceiling(reason: &str) -> String {
    humanize_reason_impl(reason, true)
}

/// The shared body of [`humanize_reason`] / [`humanize_reason_at_ceiling`]. `suppress_target` drops
/// the "(below N% target)" clause on the `call_resolution_rate` reason (the ONLY reason that carries
/// one) — nothing else differs, so the two public entry points cannot fork.
fn humanize_reason_impl(reason: &str, suppress_target: bool) -> String {
    // "call_resolution_rate=33.5%_below_50%" → the in-scope rate, reader-framed.
    if let Some(rest) = reason.strip_prefix("call_resolution_rate=") {
        let parts: Vec<&str> = rest.split("_below_").collect();
        if parts.len() == 2 {
            let rate = parts[0].trim_end_matches('%');
            let threshold = parts[1].trim_end_matches('%');
            if let (Ok(r), Ok(t)) = (rate.parse::<f64>(), threshold.parse::<f64>()) {
                if suppress_target {
                    // At-ceiling: state the rate, never a target the reader cannot approach.
                    return resolved_phrase_pct(r);
                }
                return format!("{} (below {t}% target)", resolved_phrase_pct(r));
            }
        }
    }

    // "unresolved_imports=944"
    if let Some(count) = reason.strip_prefix("unresolved_imports=") {
        if let Ok(n) = count.parse::<u64>() {
            return format!("{n} unresolved imports");
        }
    }

    match reason {
        "alias_resolution_suspicion" => "alias resolution suspected".to_string(),
        "missing_entrypoint_declarations" => "no entrypoints declared".to_string(),
        "registry_pattern_suspicion" => "registry/factory patterns detected".to_string(),
        // Unknown token — clean up underscores rather than leak a raw machine code.
        other => other.replace('_', " "),
    }
}

/// COHERENCE-POLISH-1 §2: the reader-frame ceiling sentence for a repo whose call-graph resolution is
/// at a PERMANENT no-resolver ceiling (`CeilingReport::Ceiling`). `languages` are the ceilinged
/// languages; they are joined with `/` (the SAME separator `check`'s `ceiling_language_list` uses, so
/// the two surfaces name the set identically). States, in the reader's frame, that the resolved share
/// is a CAPABILITY limit — not a fixable gap — so an agent does not chase a target it can never meet.
/// Lives here beside the reader-frame call-graph vocabulary (`resolved_phrase_pct`, the humanizers) so
/// trust's ceiling wording cannot fork.
pub fn call_graph_ceiling_note(languages: &[String]) -> String {
    format!(
        "call-graph resolution is at this build's ceiling for {} (no resolver exists) — the resolved \
         share reflects a capability limit, not a fixable gap; verify call/dead claims against source",
        languages.join("/")
    )
}

/// COHERENCE-POLISH-1 §2 (STANDING HONESTY RULE 1): the reader-frame line for the case where the
/// ceiling capability read FAILED (`CeilingReport::Unknown`) — whether the resolved share is at a
/// permanent limit could not be determined, surfaced WITH its reason (never swallowed, never a false
/// "not a ceiling"). The "below N% target" clause is NOT suppressed in this case: a read failure may
/// never soften the posture.
pub fn call_graph_ceiling_unknown_note(reason: &str) -> String {
    format!(
        "whether call-graph resolution is at a permanent capability ceiling is unknown ({reason})"
    )
}

#[cfg(test)]
#[path = "reliability_tests.rs"]
mod tests;
