//! Call-graph-resolution CAPABILITY fact (CHECK-SIGNAL-1).
//!
//! `CeilingFact` is the daemon-injected capability fact the pure `check` reducer
//! consumes to decide, for a DEGRADING `CALL_GRAPH_RELIABILITY` (LOW / no in-scope
//! calls) and a "did not run" `ENRICHMENT_STATE`, whether the reader is looking at:
//!   - a PERMANENT no-resolver ceiling → render a PASSING stated limitation
//!     (figures unchanged); or
//!   - an ACTIONABLE gap → keep the pre-CHECK-SIGNAL-1 failing classification + CTA; or
//!     - an UNKNOWN capability (the language-breakdown read failed) → the affected
//!       condition renders unknown-WITH-REASON and contributes to the verdict EXACTLY
//!       as the actionable/no-fact case does (failing). A read failure may NEVER mint a
//!       Pass (Fact Certainty Model: a Layer-0 read failure cannot manufacture a green
//!       verdict).
//!
//! It is computed daemon-side from the SAME materiality × resolver facts the D5
//! next-action CTA reads (`daemon_runtime::reader_context::call_graph_ceiling_languages`)
//! — one source, never re-derived — and injected exactly like `index_drift` /
//! `enrich_state_override` (daemon → agent), keeping the reducer I/O-free.
//!
//! ## Why a three-variant sum, not `Option<ResolutionCeiling>` (build-1)
//!
//! review-1 + operator ruling (2026-08-31 `ceiling-read-unknown`): build-1's
//! `Option<ResolutionCeiling>` used `None` for BOTH "authoritatively not a ceiling"
//! AND "the capability read failed". A storage failure therefore rendered the
//! actionable/degrading verdict with NO unknown-with-reason condition — a fallible
//! read that is CLASSIFIED, swallowed to a sentinel (STANDING HONESTY RULE #1). The
//! three states are now DISTINCT and each carries only its own data:
//!   - [`CeilingFact::Ceiling`] — affirmatively computed permanent ceiling.
//!   - [`CeilingFact::NoCeiling`] — affirmatively computed: at least one materially-
//!     present language HAS a resolution path → the gap is closable (actionable).
//!   - [`CeilingFact::Unknown`] — the read failed; whether this is a ceiling is
//!     unknown, carried WITH its reason.
//! Exhaustive `match` at both consumers, no wildcard arm: adding a variant is meant
//! to break every site whose assumptions changed.
//!
//! The `CheckInput` field wrapping this stays `Option<CeilingFact>` — `None` means the
//! CALLER performed no ceiling analysis at all (the simple `run_check` entry, the
//! no-snapshot branch, unit tests), which is a DIFFERENT axis from the three capability
//! outcomes and is modelled exactly like the sibling `Option<IndexDrift>` fact (`None`
//! = "not computed by this caller" → pre-slice behavior, byte-identical). It is NOT a
//! read failure (that is `Some(Unknown)`), so no honesty conflation remains.
//!
//! Pure data. The daemon (composition root) constructs instances; this crate never
//! performs I/O. No serde: this is a reducer INPUT fact, never serialized to the check
//! JSON output (only the additive `CheckConditionEvidence.ceiling: bool` marker crosses
//! the wire).
//!
//! Abstraction one-liner (architecture rule):
//!   - WHAT: a raw boundary DTO — an exhaustive 3-variant capability sum — on the
//!     daemon→agent injected-fact boundary.
//!   - CURRENT USERS: constructed in `daemon-runtime::dispatch::handle_check` (from
//!     `reader_context::call_graph_ceiling_languages` + the read result); consumed by
//!     `agent::check::evaluate` (the CALL_GRAPH_RELIABILITY + ENRICHMENT_STATE
//!     conditions) via `CheckInput.ceiling_fact`.
//!   - AXIS: variants FIXED (three capability outcomes: ceiling / no-ceiling / unknown),
//!     operations GROWING (two conditions consume it today) → sum + exhaustive match
//!     (dispatch-by-growth-axis: adding a fourth capability outcome must break every
//!     match, which is the feature).
//!   - REJECTED SIMPLER: `Option<ResolutionCeiling>` (build-1) — rejected by review-1 +
//!     operator because `None` conflated authoritative-no-ceiling with read-failure, so
//!     a classified fallible read was swallowed to a sentinel with no unknown-with-reason.

/// The call-graph-resolution capability of a repo, as one exhaustive fact.
///
/// See the module docs for the verdict semantics of each variant. Every consumer
/// matches ALL THREE variants — no wildcard arm — so a future capability outcome
/// deliberately breaks each site whose assumptions changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CeilingFact {
    /// AUTHORITATIVE permanent ceiling: EVERY materially-present code language has NO
    /// call-graph resolver on ANY build (C / C++ / Python / Go / …). A LOW / no-in-scope
    /// `CALL_GRAPH_RELIABILITY` and a "did not run" `ENRICHMENT_STATE` render as PASSING
    /// stated limitations naming `languages`; the reliability FIGURES are untouched, only
    /// the classification + wording change.
    Ceiling {
        /// Reader-facing display names of the ceilinged languages, sorted + deduped for
        /// determinism (e.g. `["C++"]`, `["C", "C++"]`, `["Python"]`).
        ///
        /// INVARIANT: non-empty. The constructor
        /// (`reader_context::call_graph_ceiling_languages`) yields this variant only when
        /// ≥1 code language is materially present AND all lack a resolver; a ceiling with
        /// zero languages is not a representable state.
        languages: Vec<String>,
    },
    /// AUTHORITATIVE non-ceiling: at least one materially-present code language HAS a
    /// resolution path (enrichable now, or configurable like Java+JDTLS), so the gap is
    /// CLOSABLE — the pre-CHECK-SIGNAL-1 degrading classification + CTA stands. Carries no
    /// data: the affirmative "no ceiling" outcome needs none.
    NoCeiling,
    /// UNKNOWN capability: the language-breakdown read failed, so whether this repo is at a
    /// ceiling could not be determined. The affected condition renders unknown-WITH-REASON
    /// and contributes to the verdict exactly as `NoCeiling` does (failing) — a read failure
    /// may never IMPROVE a verdict (Fact Certainty Model). Constructed at the daemon read
    /// site so the reason (not a stderr line) is the record.
    Unknown {
        /// Why the capability is unknown — the failed-read error, surfaced in-band on the
        /// affected condition's summary rather than swallowed to stderr.
        reason: String,
    },
}
