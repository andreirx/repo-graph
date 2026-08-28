//! `dead_render.rs` — DEAD-CAUSES-1 pure renderer for `rmap dead`'s refusal.
//!
//! Turns the daemon's `dead_causes` facts (or an error reason) into the "Root causes"
//! block of the refusal. Kept pure and daemon-free so the four honesty cases the slice
//! pins (§4) are unit-tested here, not through a live socket:
//!   (a) snapshot WITH framework inferences → counts named, NO "missing detector" claim
//!   (b) snapshot with NONE               → honest zero-state line (derived from langs)
//!   (c) no snapshot for cwd              → generic block, labelled, with the reason
//!   (d) daemon unreachable               → generic block, labelled, with the reason
//!
//! Abstraction (DEAD-CAUSES-1): crate-private module.
//!   - what: (facts | error-reason) → Root-causes text.
//!   - current user: `commands::dead::run_dead` (sole caller).
//!   - axis: the honesty-critical sentence composition (a–d) must be tested daemon-free.
//!   - rejected simpler: compose inline in `run_dead` — rejected (no test seam for a–d).

use serde::{Deserialize, Deserializer};

/// Preserve field PRESENCE for `uncovered_note`: distinguish an OMITTED field from an
/// explicit `null`. A plain `Option<String>` collapses both to `None` — serde treats a
/// missing field and a `null` value identically — which would let a MALFORMED
/// `total_inferences > 0` payload that simply DROPPED the field render as
/// snapshot-derived with the no-detector gap silently missing (DEAD-CAUSES-1 review #1).
/// The double-`Option` keeps the distinction:
///   - field omitted        → `None`          (malformed when total > 0 — see `validate`)
///   - field present, `null` → `Some(None)`    (explicit "no detector gap on this snapshot")
///   - field present, string → `Some(Some(s))` (the derived no-detector-gap clause)
///
/// Paired with `#[serde(default)]`: when the field is absent serde uses the `Default`
/// (`None`, outer-absent); when present it calls this, which reads an `Option<String>`
/// (`null → None`, `s → Some(s)`) and wraps it in `Some` to record presence.
fn double_option<'de, D>(de: D) -> Result<Option<Option<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    Deserialize::deserialize(de).map(Some)
}

/// The `dead_causes` response, parsed defensively. A missing/mistyped field makes the
/// whole parse fail (serde), which `run_dead` maps to the LABELLED generic block with a
/// reason — never a silently-defaulted cause line.
#[derive(Deserialize)]
pub struct DeadCausesFacts {
    pub framework: FrameworkFacts,
    pub coverage: PresenceFacts,
    pub entrypoints: PresenceFacts,
}

impl DeadCausesFacts {
    /// Enforce the cross-field response invariant serde derive cannot express: a
    /// zero-inference snapshot MUST carry the honest `empty` message (built server-side
    /// from the language mix). A missing `empty` field on an `Option` silently
    /// deserialises to `None` regardless of `#[serde(default)]`, so without this gate a
    /// MALFORMED zero-inference payload would render the weak "No framework liveness
    /// inferences recorded" line instead of the LABELLED generic-with-reason fallback
    /// (DEAD-CAUSES-1 review #2). A violated invariant is a malformed response → the
    /// caller shows the generic block with this reason, never a defaulted cause line.
    pub fn validate(&self) -> Result<(), String> {
        if self.framework.total_inferences == 0 && self.framework.empty.is_none() {
            return Err(
                "malformed daemon response: zero-inference snapshot is missing the \
                 framework empty-state message"
                    .to_string(),
            );
        }
        // Symmetric cross-field invariant for the mixed-language gap (DEAD-CAUSES-1
        // review #1): a `total_inferences > 0` snapshot MUST carry `uncovered_note`
        // (as an explicit `null` when there is no detector gap, or the gap clause).
        // An OMITTED field (outer `None`, distinguished from present-`null` by
        // `double_option`) would silently drop the "no detector covers X" cause while
        // still rendering as snapshot-derived — so it is a malformed response and the
        // caller shows the LABELLED generic-with-reason block instead.
        if self.framework.total_inferences > 0 && self.framework.uncovered_note.is_none() {
            return Err(
                "malformed daemon response: snapshot with inferences is missing the \
                 framework uncovered-note field (an explicit null means no detector gap)"
                    .to_string(),
            );
        }
        Ok(())
    }
}

#[derive(Deserialize)]
pub struct FrameworkFacts {
    pub detectors: Vec<DetectorFact>,
    pub total_inferences: u64,
    /// Present (as an object with a `message`) ONLY when `total_inferences == 0` — the
    /// honest zero-state line built server-side from the snapshot's language mix. When
    /// `total_inferences == 0` this MUST be `Some`; `DeadCausesFacts::validate` enforces
    /// that (serde cannot, since a missing `Option` field deserialises to `None`).
    pub empty: Option<EmptyFact>,
    /// The "no inference detector covers X" clause for materially-present languages that
    /// no detector covers, present ONLY when `total_inferences > 0` (the mixed-language
    /// case: one family produced inferences yet other present languages have no
    /// detector). Mutually exclusive with `empty` by construction — `empty` already
    /// carries this fact in the zero-inference case.
    ///
    /// Double-`Option` (see `double_option`) preserves field PRESENCE: `Some(None)` is
    /// the explicit "no detector gap" value; outer `None` (field omitted) is MALFORMED
    /// for a `total_inferences > 0` payload and is rejected by `validate`.
    #[serde(default, deserialize_with = "double_option")]
    pub uncovered_note: Option<Option<String>>,
}

#[derive(Deserialize)]
pub struct DetectorFact {
    pub label: String,
    #[allow(dead_code)]
    pub applicable: bool,
    pub count: u64,
}

#[derive(Deserialize)]
pub struct EmptyFact {
    pub message: String,
}

#[derive(Deserialize)]
pub struct PresenceFacts {
    pub present: bool,
    pub count: u64,
}

/// The framework cause line, derived from the snapshot's inference facts.
///
/// - Any detector with a produced count → name it with its count and state the true
///   defect (evidence EXISTS but is not consumed). This is the anti-stale line: it can
///   NEVER say "missing React detector" for a snapshot that carries React inferences.
/// - Zero inferences → the server's honest zero-state message (names the reader's
///   languages + which detectors this build has), verbatim.
fn framework_line(f: &FrameworkFacts) -> String {
    if f.total_inferences > 0 {
        let named: Vec<String> = f
            .detectors
            .iter()
            .filter(|d| d.count > 0)
            .map(|d| format!("{}: {}", d.label, d.count))
            .collect();
        if named.is_empty() {
            // total>0 but no per-detector attribution (e.g. an inference kind outside
            // the catalog): state the total honestly rather than a fabricated family.
            format!(
                "Framework liveness inferences exist ({} total) but are not wired \
                 into deadness scoring — runtime-owned symbols would read as dead.",
                f.total_inferences
            )
        } else {
            format!(
                "Framework liveness inferences exist ({}) but are not wired into \
                 deadness scoring — runtime-owned symbols would read as dead.",
                named.join(", ")
            )
        }
    } else if let Some(empty) = &f.empty {
        empty.message.clone()
    } else {
        "No framework liveness inferences recorded for this snapshot.".to_string()
    }
}

fn coverage_line(c: &PresenceFacts) -> String {
    if c.present {
        format!(
            "Coverage-backed evidence present ({} file(s) measured) but not wired \
             into deadness scoring.",
            c.count
        )
    } else {
        "No coverage-backed evidence recorded for this snapshot.".to_string()
    }
}

fn entrypoint_line(e: &PresenceFacts) -> String {
    if e.present {
        format!(
            "{} entrypoint declaration(s) recorded but not wired into deadness scoring.",
            e.count
        )
    } else {
        "No entrypoint declarations recorded for this snapshot.".to_string()
    }
}

/// The derived "Root causes" block (facts came back from the reader's snapshot).
pub fn render_derived(facts: &DeadCausesFacts) -> String {
    let mut s = String::new();
    s.push_str("Root causes (derived from this repo's current snapshot):\n");
    s.push_str(&format!("  - {}\n", framework_line(&facts.framework)));
    // Mixed-language gap: a separate cause line when a family produced inferences yet
    // other materially-present languages have no detector at all (server sets this only
    // in the total>0 case; the zero-inference case carries it inside the framework line).
    if let Some(Some(note)) = &facts.framework.uncovered_note {
        s.push_str(&format!("  - {note}\n"));
    }
    s.push_str(&format!("  - {}\n", coverage_line(&facts.coverage)));
    s.push_str(&format!("  - {}\n", entrypoint_line(&facts.entrypoints)));
    s
}

/// The LABELLED generic block, used when causes could NOT be derived (daemon
/// unreachable, repo not indexed, or a read/parse failure). Never presented as truth
/// about the repo — the reason and the "not derived from a snapshot" label are explicit.
pub fn render_generic(reason: &str) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "Root causes could not be derived for this directory ({reason}).\n"
    ));
    s.push_str("Generic causes follow (NOT derived from a snapshot):\n");
    s.push_str(
        "  - Framework-owned symbols read as dead when liveness inferences are not \
         wired into deadness scoring\n",
    );
    s.push_str(
        "  - Entrypoint-owned symbols read as dead without entrypoint declarations \
         wired in\n",
    );
    s.push_str("  - No coverage-backed evidence\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts_from(json: serde_json::Value) -> DeadCausesFacts {
        serde_json::from_value(json).expect("valid dead_causes payload")
    }

    /// Wrap a `framework` object with absent coverage/entrypoints (the common test shape).
    fn fw_facts(framework: serde_json::Value) -> DeadCausesFacts {
        facts_from(serde_json::json!({
            "framework": framework,
            "coverage": {"present": false, "count": 0},
            "entrypoints": {"present": false, "count": 0},
        }))
    }

    /// (a) glamCRM-shaped: React + Spring inferences present. The framework line MUST
    /// name the counts and MUST NOT claim a missing React/Spring detector.
    #[test]
    fn derived_with_framework_inferences_names_counts_no_missing_claim() {
        let facts = fw_facts(serde_json::json!({
            "detectors": [
                {"label": "React", "applicable": true, "count": 226},
                {"label": "Spring", "applicable": true, "count": 14},
            ],
            "total_inferences": 240, "empty": null, "uncovered_note": null,
        }));
        let out = render_derived(&facts);
        assert!(out.contains("React: 226"), "names React count: {out}");
        assert!(out.contains("Spring: 14"), "names Spring count: {out}");
        assert!(
            !out.to_lowercase().contains("missing"),
            "must not claim missing machinery the snapshot disproves: {out}"
        );
        assert!(
            out.contains("not wired into deadness scoring"),
            "states the true defect: {out}"
        );
    }

    /// A detector filtered out at count==0 is NOT named (no "React: 0").
    #[test]
    fn derived_omits_zero_count_detectors_from_the_named_list() {
        let facts = fw_facts(serde_json::json!({
            "detectors": [
                {"label": "React", "applicable": true, "count": 5},
                {"label": "Spring", "applicable": false, "count": 0},
            ],
            "total_inferences": 5, "empty": null, "uncovered_note": null,
        }));
        let out = render_derived(&facts);
        assert!(out.contains("React: 5"));
        assert!(
            !out.contains("Spring"),
            "zero-count detector not named: {out}"
        );
    }

    /// Mixed-language gap (review #1): Spring inferences present AND C/C++ files with no
    /// detector. The refusal MUST name Spring's count AND, as a separate cause, state that
    /// no detector covers C/C++ — the fact the old total>0 path silently dropped.
    #[test]
    fn derived_mixed_language_names_family_and_states_no_detector_gap() {
        let facts = fw_facts(serde_json::json!({
            "detectors": [
                {"label": "React", "applicable": false, "count": 0},
                {"label": "Spring", "applicable": true, "count": 14},
            ],
            "total_inferences": 14, "empty": null,
            "uncovered_note": "No inference detector on this build covers C, C++.",
        }));
        let out = render_derived(&facts);
        assert!(
            out.contains("Spring: 14"),
            "names the produced family: {out}"
        );
        assert!(
            out.contains("No inference detector on this build covers C, C++"),
            "states the no-detector gap for present languages: {out}"
        );
        assert!(
            !out.to_lowercase().contains("missing"),
            "still no stale 'missing' claim about the family that ran: {out}"
        );
    }

    /// No mixed-language gap → no extra cause line. `uncovered_note` present as an
    /// explicit `null` is the "no detector gap on this snapshot" value (`Some(None)`),
    /// distinct from the omitted-field case which `validate` rejects as malformed.
    #[test]
    fn derived_with_null_uncovered_note_emits_no_gap_line() {
        let facts = fw_facts(serde_json::json!({
            "detectors": [{"label": "React", "applicable": true, "count": 3}],
            "total_inferences": 3, "empty": null, "uncovered_note": null,
        }));
        assert!(
            facts.validate().is_ok(),
            "present-null uncovered_note is the valid no-gap value"
        );
        let out = render_derived(&facts);
        assert!(
            !out.contains("No inference detector on this build covers"),
            "no gap line when there is no gap: {out}"
        );
    }

    /// (b) snapshot with NO inferences → the server's honest zero-state message, verbatim.
    #[test]
    fn derived_with_no_inferences_uses_honest_zero_state_message() {
        let facts = fw_facts(serde_json::json!({
            "detectors": [
                {"label": "React", "applicable": false, "count": 0},
                {"label": "Spring", "applicable": false, "count": 0},
            ],
            "total_inferences": 0,
            "empty": {"message": "No inference detector on this build covers C, C++ (this build's inference detectors: React → JS/TS, Spring → Java)."},
            "uncovered_note": null,
        }));
        let out = render_derived(&facts);
        assert!(
            out.contains("No inference detector on this build covers C, C++"),
            "uses the honest zero-state line: {out}"
        );
        assert!(out.contains("No coverage-backed evidence recorded"));
        assert!(out.contains("No entrypoint declarations recorded"));
    }

    /// Coverage + entrypoint present → both reported with counts, "not wired" defect.
    #[test]
    fn derived_reports_present_coverage_and_entrypoints_with_counts() {
        let facts = facts_from(serde_json::json!({
            "framework": {
                "detectors": [{"label": "React", "applicable": true, "count": 3}],
                "total_inferences": 3, "empty": null, "uncovered_note": null,
            },
            "coverage": {"present": true, "count": 42},
            "entrypoints": {"present": true, "count": 7},
        }));
        let out = render_derived(&facts);
        assert!(
            out.contains("Coverage-backed evidence present (42 file(s) measured)"),
            "{out}"
        );
        assert!(
            out.contains("7 entrypoint declaration(s) recorded"),
            "{out}"
        );
    }

    /// (c)/(d) generic block: labelled, carries the reason, makes NO repo-specific claim.
    #[test]
    fn generic_block_is_labelled_and_carries_reason() {
        let out = render_generic("daemon not running");
        assert!(out.contains("could not be derived"), "{out}");
        assert!(
            out.contains("daemon not running"),
            "carries the reason: {out}"
        );
        assert!(
            out.contains("NOT derived from a snapshot"),
            "explicit generic label: {out}"
        );
        // Must NOT resurrect the stale specific-detector claim.
        assert!(
            !out.contains("Spring, React"),
            "no stale detector list: {out}"
        );
    }

    /// A malformed payload (missing `coverage`) fails to parse — `run_dead` will then
    /// use the generic block. Proven here: serde rejects it (no silent default).
    #[test]
    fn malformed_payload_fails_to_parse_not_silently_defaulted() {
        let bad: Result<DeadCausesFacts, _> = serde_json::from_value(serde_json::json!({
            "framework": {"detectors": [], "total_inferences": 0, "empty": null},
            "entrypoints": {"present": false, "count": 0},
        }));
        assert!(
            bad.is_err(),
            "missing `coverage` must not default to absent"
        );
    }

    /// (review #2) A well-formed VALID payload (framework inferences present, empty null)
    /// passes validation.
    #[test]
    fn validate_accepts_a_wellformed_nonempty_payload() {
        let facts = fw_facts(serde_json::json!({
            "detectors": [], "total_inferences": 3, "empty": null, "uncovered_note": null,
        }));
        assert!(facts.validate().is_ok());
    }

    /// (review #2) A zero-inference payload WITHOUT the required `empty` message parses
    /// but MUST fail `validate` — the generic block renders, not the weak default line.
    #[test]
    fn validate_rejects_zero_inference_payload_missing_empty_message() {
        let facts = fw_facts(serde_json::json!({"detectors": [], "total_inferences": 0}));
        let err = facts
            .validate()
            .expect_err("must reject the malformed zero-inference payload");
        assert!(
            err.contains("empty-state message"),
            "reason names the missing invariant: {err}"
        );
    }

    /// (review #1) A `total_inferences > 0` payload OMITTING `uncovered_note` parses
    /// (absent → outer `None`) but MUST fail `validate` — the generic block renders
    /// instead of silently dropping the gap cause. Explicit `null` is accepted.
    #[test]
    fn validate_rejects_nonempty_payload_omitting_uncovered_note() {
        let facts = fw_facts(serde_json::json!({
            "detectors": [{"label": "Spring", "applicable": true, "count": 14}],
            "total_inferences": 14, "empty": null,
            // uncovered_note field OMITTED — the malformed shape review #1 flagged.
        }));
        let err = facts
            .validate()
            .expect_err("must reject a nonempty payload that omits uncovered_note");
        assert!(
            err.contains("uncovered-note"),
            "reason names the missing invariant: {err}"
        );
    }

    /// The three-way distinction the double-`Option` preserves: omitted → `None`;
    /// present-`null` → `Some(None)`; string → `Some(Some(_))`.
    #[test]
    fn uncovered_note_presence_is_preserved_through_parse() {
        let omitted = fw_facts(serde_json::json!({
            "detectors": [], "total_inferences": 1, "empty": null,
        }));
        assert!(
            omitted.framework.uncovered_note.is_none(),
            "omitted field → outer None"
        );

        let present_null = fw_facts(serde_json::json!({
            "detectors": [], "total_inferences": 1, "empty": null, "uncovered_note": null,
        }));
        assert_eq!(
            present_null.framework.uncovered_note,
            Some(None),
            "present-null → Some(None), NOT collapsed to outer None"
        );

        let present_str = fw_facts(serde_json::json!({
            "detectors": [], "total_inferences": 1, "empty": null,
            "uncovered_note": "No inference detector on this build covers C.",
        }));
        assert_eq!(
            present_str
                .framework
                .uncovered_note
                .as_ref()
                .map(|o| o.as_deref()),
            Some(Some("No inference detector on this build covers C.")),
            "present string → Some(Some(_))"
        );
    }

    /// (review #2) A zero-inference payload whose `empty` object is INVALID (missing the
    /// required `message`) fails at PARSE — never a silently-defaulted `EmptyFact`.
    #[test]
    fn malformed_empty_object_fails_to_parse() {
        let bad: Result<DeadCausesFacts, _> = serde_json::from_value(serde_json::json!({
            "framework": {"detectors": [], "total_inferences": 0, "empty": {"reason": "x"}},
            "coverage": {"present": false, "count": 0},
            "entrypoints": {"present": false, "count": 0},
        }));
        assert!(
            bad.is_err(),
            "an `empty` object without `message` must not parse"
        );
    }
}
