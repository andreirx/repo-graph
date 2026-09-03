//! The `find` FACTS tier renderer (FIND-FACTS-1 §2.1/§2.2). Consumes the `facts`
//! array of our OWN `FindResponse` DTO ACROSS the daemon boundary and renders one
//! block per fact class — but does NOT trust it: the group triple, the envelope
//! invariant, the per-hit path, and the per-hit `next` command are each re-validated
//! here; anything outside the ratified shape is surfaced as MALFORMED, never rendered
//! as an actionable fact (STANDING HONESTY RULE 1).
//!
//! Abstraction record — module: `find::facts_render`; concrete current user:
//! `find::render_find_human` (the sole caller of [`render_facts_tier`]); axis: the
//! ≤500-line guardrail (review-4 item 1) — the FACTS-tier rendering + boundary
//! validation is its own responsibility, split off the former 988-line `find.rs`;
//! rejected simpler alternative: leaving it inline (file stays >500).

/// One RATIFIED fact class's teaching contract (FIND-FACTS-1 §2.1/§2.2), MIRRORING the
/// daemon's `FactClass` sum type (the witness manifest
/// `daemon-runtime/witness/dispatch_fact_classes.txt` is the shared anchor). This CLI
/// deserializes the `find` response ACROSS the daemon boundary and must not trust it:
/// a group whose class/certainty/command shape is not EXACTLY one of these is malformed
/// action text — surfaced as malformed, NEVER rendered as an actionable next move
/// (review-2 item 2; STANDING HONESTY RULE 1). Plain data table, not an abstraction:
/// its only user is [`render_facts_tier`]'s validation; the simpler rejected
/// alternative — trusting whatever `render_command`/`next` the payload carries — is
/// exactly the defect this closes.
struct RatifiedClass {
    class: &'static str,
    certainty: &'static str,
    /// The runnable render command(s) for this class. ONE for the six single-renderer
    /// classes (mirrored in the payload's group `render_command`); the governance set
    /// `{violations, gate}` for the `boundary` declarations class, whose renderer varies
    /// per hit (review-6 re-home) — that group therefore carries NO single
    /// `render_command`, and each hit's `next` must be one of these.
    commands: &'static [&'static str],
    /// `true` for the classes whose per-hit `next` folds the hit key into the command
    /// (`explain <key>` / `map --dry-run <path>`); `false` for the whole-listing classes
    /// (including boundary's `violations`/`gate`), whose `next` IS the bare command.
    folds_key: bool,
}

const RATIFIED_CLASSES: [RatifiedClass; 7] = [
    RatifiedClass {
        class: "symbol",
        certainty: "extracted",
        commands: &["explain"],
        folds_key: true,
    },
    RatifiedClass {
        class: "file",
        certainty: "extracted",
        commands: &["explain"],
        folds_key: true,
    },
    RatifiedClass {
        class: "module",
        certainty: "inferred",
        commands: &["map --dry-run"],
        folds_key: true,
    },
    RatifiedClass {
        class: "http-surface",
        certainty: "inferred",
        commands: &["boundaries list"],
        folds_key: false,
    },
    RatifiedClass {
        class: "dependency",
        certainty: "extracted",
        commands: &["deps list"],
        folds_key: false,
    },
    RatifiedClass {
        class: "framework",
        certainty: "hint",
        commands: &["inferences list"],
        folds_key: false,
    },
    // review-6 re-home: the governance DECLARATIONS class. Per-hit renderer — a
    // boundary-kind declaration → `violations`, a requirement/quality-policy one →
    // `gate`; Layer-4 governance certainty. No single group `render_command`.
    RatifiedClass {
        class: "boundary",
        certainty: "governance",
        commands: &["violations", "gate"],
        folds_key: false,
    },
];

/// The ratified contract for `class`, or `None` if the payload named a class outside
/// the seven-class taxonomy (a malformed envelope).
fn ratified_for(class: &str) -> Option<&'static RatifiedClass> {
    RATIFIED_CLASSES.iter().find(|r| r.class == class)
}

/// Render the FACTS tier: one block per fact class WITH hits or a per-class
/// `unavailable (<reason>)`, and a single compact "no matches" line naming the
/// classes that were searched and found nothing (honest searched set, §2.5).
///
/// Returns the [`super::FactTierOutcome`] this single validating traversal observed — the
/// authoritative signal (no second classifier to drift from what was rendered) the seed
/// tier gates its §2.4 capability close on (review-1). A MISS is established ONLY when the
/// payload is well-formed, envelope-complete, and every class matched nothing; a rendered
/// hit, ANY malformed marker, or an unavailable class read yields `MissNotEstablished`.
pub(super) fn render_facts_tier(
    result: &serde_json::Value,
    repo_uid: Option<&str>,
    out: &mut String,
) -> super::FactTierOutcome {
    use super::FactTierOutcome;
    // Deterministic lexical RETRIEVAL — but the CONTENT certainty varies by source
    // layer, so each class is tagged `extracted` / `inferred` / `hint` / `governance`
    // (review-1 honesty defect; VISION § Fact Certainty Model). The retrieval is
    // deterministic; an inferred module boundary or a Layer-4 governance declaration is
    // NOT an extracted fact.
    out.push_str(
        "Facts (deterministic lexical match over the indexed tables; each class tagged by certainty — extracted / inferred / hint / governance):\n",
    );

    // `facts` is our OWN DTO field, ALWAYS serialized (one group per class). A
    // missing key / non-array is a MALFORMED response — surface it, never render a
    // false "no facts" (STANDING HONESTY RULE).
    let groups = match result.get("facts") {
        Some(serde_json::Value::Array(a)) => a,
        _ => {
            out.push_str("  (malformed find response: facts missing or not a list)\n");
            // A non-array `facts` is malformed — a miss cannot be honestly established
            // (malformed ≠ empty; review-1).
            return FactTierOutcome::MissNotEstablished;
        }
    };

    // §2.4 miss accounting (review-1). `matched_any`: a class rendered ≥1 hit.
    // `clean_groups`: groups that resolved to a VALID hit-or-empty state. Every malformed
    // marker and every unavailable-class read `continue`s WITHOUT incrementing it, so
    // `clean_groups == groups.len()` is a structural proof that NO marker fired (no need to
    // flag each of the 13 marker sites — one drifting flag could falsely establish a miss,
    // the exact honesty bug). A miss is established below ONLY when nothing matched, every
    // group was clean, and the envelope is complete.
    let mut matched_any = false;
    let mut clean_groups = 0usize;

    let mut empty_classes: Vec<String> = Vec::new();
    // The ratified classes that appeared as a VALID group this response, in payload
    // order. Used to enforce the envelope invariant (§2.1: EXACTLY one group per
    // class, in `FactClass::ALL` order) across the daemon boundary — a duplicate is
    // rejected mid-loop, and any class absent from this set after the loop is a
    // MALFORMED envelope (never a silent shrink of the searched set).
    let mut seen_classes: Vec<&str> = Vec::new();

    for g in groups {
        // ── Class + certainty (§2.2): both present as strings AND a ratified pair. A
        // missing/mistyped field, or an unrecognized class/certainty, is malformed
        // ACTION TEXT — surfaced, never rendered actionable (review-2 item 2; STANDING
        // HONESTY RULE 1). The certainty check enforces the layer tag (review-1 honesty
        // defect): a Layer 2–4 class can only render under its ratified
        // `inferred`/`hint`/`governance` tag.
        let class = g.get("fact_class").and_then(|v| v.as_str());
        let certainty = g.get("certainty").and_then(|v| v.as_str());
        let (Some(class), Some(certainty)) = (class, certainty) else {
            out.push_str("  (malformed fact group: missing fact_class/certainty)\n");
            continue;
        };
        let Some(ratified) = ratified_for(class) else {
            out.push_str(&format!(
                "  (malformed fact group: unrecognized fact class: {class})\n"
            ));
            continue;
        };
        if certainty != ratified.certainty {
            out.push_str(&format!(
                "  (malformed fact group: class {class} carries certainty {certainty}, ratified {})\n",
                ratified.certainty
            ));
            continue;
        }
        // The group `render_command` (§2.2): PRESENT and EXACTLY the single command for
        // a single-renderer class; ABSENT for the per-hit boundary class (whose renderer
        // varies by declaration kind — review-6). A present-but-wrong command, an absent
        // one on a single-renderer class, or a present one on the per-hit class is
        // malformed action text — surfaced, never rendered as a next move. `header_cmd`
        // is the group-header verb (`Some` for a single renderer; `None` renders
        // `[class · certainty]` with each hit carrying its own `→ rmap <next>`).
        let payload_cmd = g.get("render_command").and_then(|v| v.as_str());
        let header_cmd: Option<&str> = match (ratified.commands, payload_cmd) {
            ([single], Some(cmd)) if cmd == *single => Some(*single),
            ([_single], _) => {
                out.push_str(&format!(
                    "  (malformed fact group: class {class} render_command missing or not the ratified command)\n"
                ));
                continue;
            }
            // Multi-renderer (boundary): the group must carry NO single render_command.
            (_, Some(cmd)) => {
                out.push_str(&format!(
                    "  (malformed fact group: per-hit-renderer class {class} carries a single render_command {cmd})\n"
                ));
                continue;
            }
            (_, None) => None,
        };
        // Envelope invariant (§2.1): the response carries EXACTLY one group per
        // ratified class (the daemon emits one each in `FactClass::ALL` order). A
        // SECOND valid group for a class already seen is a MALFORMED envelope —
        // surfaced, NEVER rendered twice as if two independent fact sets existed for
        // one class (review-3 item 2; STANDING HONESTY RULE 1).
        if seen_classes.contains(&class) {
            out.push_str(&format!(
                "  (malformed find response: duplicate fact group for class {class})\n"
            ));
            continue;
        }
        seen_classes.push(class);
        let label = match header_cmd {
            Some(cmd) => format!("[{class} · {certainty} → rmap {cmd}]"),
            // Per-hit renderer (boundary): no single group verb — each hit line shows
            // its own `→ rmap <next>` (review-6 re-home).
            None => format!("[{class} · {certainty}]"),
        };

        // ── error (§2 STANDING HONESTY RULE): ABSENT, or a NON-EMPTY string that is
        // rendered `unavailable (<reason>)`. A present-but-non-string / empty error is
        // MALFORMED — NOT silently treated as "no error" (review-2 item 3), which would
        // render a class whose read actually failed as a live, actionable group.
        match g.get("error") {
            None => {}
            Some(serde_json::Value::String(reason)) if !reason.is_empty() => {
                out.push_str(&format!("  {label}  unavailable ({reason})\n"));
                continue;
            }
            Some(_) => {
                out.push_str(&format!(
                    "  {label}  (malformed fact group: error present but not a non-empty string)\n"
                ));
                continue;
            }
        }

        let hits = match g.get("hits") {
            Some(serde_json::Value::Array(a)) => a,
            _ => {
                out.push_str(&format!(
                    "  {label}  (malformed fact group: hits missing or not a list)\n"
                ));
                continue;
            }
        };

        // ── Remainder metadata (§2.2), validated BEFORE the group is interpreted as
        // empty OR actionable (review-2 item 3). `matched` (pre-cap total) and
        // `matched_is_floor` are our OWN DTO fields, ALWAYS serialized; a missing /
        // mistyped one is MALFORMED, surfaced — NEVER collapsed to zero (which would
        // hide a real remainder as "all shown"; STANDING HONESTY RULE 1). `matched`
        // must also be CONSISTENT with the shown hits (≥ the count actually rendered):
        // a `matched` below `hits.len()` is an incoherent group, not a fact.
        let shown = hits.len();
        let matched = match g.get("matched").and_then(|v| v.as_u64()) {
            // `matched` is a wire `u64`; the renderer works in `usize`. A REPRESENTABLE
            // conversion is checked, never a lossy `as usize` (review-3 item 3): on a
            // narrower target (`usize` < 64-bit) a `u64` above `usize::MAX` would
            // TRUNCATE a real remainder into a false, smaller count — surfaced as
            // malformed instead. On a 64-bit target `usize == u64`, so the reject arm
            // is unreachable there; the guard makes the code correct on any target and
            // preserves the full value (no wraparound) where it does fit.
            Some(m) => match usize::try_from(m) {
                Ok(m) => m,
                Err(_) => {
                    out.push_str(&format!(
                        "  {label}  (malformed fact group: matched {m} exceeds this platform's addressable range)\n"
                    ));
                    continue;
                }
            },
            None => {
                out.push_str(&format!(
                    "  {label}  (malformed fact group: matched missing or not a number)\n"
                ));
                continue;
            }
        };
        let Some(floor) = g.get("matched_is_floor").and_then(|v| v.as_bool()) else {
            out.push_str(&format!(
                "  {label}  (malformed fact group: matched_is_floor missing or not a bool)\n"
            ));
            continue;
        };
        if matched < shown {
            out.push_str(&format!(
                "  {label}  (malformed fact group: matched {matched} < shown {shown})\n"
            ));
            continue;
        }

        if hits.is_empty() {
            // review-3 finding 2: an empty group is a CLEAN no-match only when its own
            // remainder metadata AGREES — `matched == 0` and not a saturated floor. A
            // group with `hits: []` but `matched > 0` (or `matched == 0` flagged as a
            // FLOOR, i.e. "at least 0" saturated with nothing shown) is INTERNALLY
            // CONTRADICTORY: the remainder claims matches the group did not carry. The
            // daemon never emits this — `finalize` sets `matched = hits.len()` and
            // `matched_is_floor = saturated && !full` (empty ⇒ 0 / false) — so it can
            // only be a garbled / fabricated payload. Rendering it as a clean empty class
            // would let it establish a fact-table MISS (`clean_groups == groups.len()`)
            // and unlock the "nothing matched" capability close. Surface it and leave the
            // miss UNESTABLISHED — do NOT increment `clean_groups`, do NOT list it as an
            // honest empty class (STANDING HONESTY RULE 1 — contradiction ≠ absence).
            if matched != 0 || floor {
                let claim = if floor {
                    format!("at least {matched}")
                } else {
                    matched.to_string()
                };
                out.push_str(&format!(
                    "  {label}  (malformed fact group: no hits but matched claims {claim})\n"
                ));
                continue;
            }
            empty_classes.push(class.to_string());
            clean_groups += 1; // a valid, empty group — cleanly accounted (§2.4 miss).
            continue;
        }
        matched_any = true; // this class matched: a fact-table miss is NOT established.
        clean_groups += 1; // a valid group with hits — cleanly accounted.
        out.push_str(&format!("  {label}\n"));
        for h in hits {
            // The hit renderer validates each hit's `next` against THIS class's ratified
            // command set (one command for the single-renderer classes, the governance
            // set {violations, gate} for boundary) so a malformed payload cannot emit
            // arbitrary text after `rmap ` (review-4 item 3; review-6 per-hit renderer).
            out.push_str(&super::fact_hit::render_fact_hit(
                h,
                ratified.commands,
                ratified.folds_key,
                repo_uid,
            ));
        }
        if matched > shown {
            // FIND-RANK-1 (§2.2): the cap is NAMED and EXACT — `showing 8 of 200 —
            // --full for all`, using the real shown/matched numbers, never the former
            // unexplained `(+N+ more)`. When `matched` is a FLOOR (the fetch window
            // saturated — `matched_is_floor`), the total renders as `at least N` (the
            // honest lower bound), never a fabricated exact count.
            let total = if floor {
                format!("at least {matched}")
            } else {
                matched.to_string()
            };
            out.push_str(&format!(
                "    showing {shown} of {total} — --full for all\n"
            ));
        }
    }

    if !empty_classes.is_empty() {
        // Whether or not any class had hits, the classes that matched nothing are
        // named explicitly — the honest searched set (§2.5), never a silent gap.
        out.push_str(&format!("  no matches: {}\n", empty_classes.join(", ")));
    }

    // Envelope completeness (§2.1): every ratified class must have appeared as a
    // valid group. A class ABSENT from the payload (empty `facts`, an omitted class,
    // or a class whose only group was rejected as malformed) is a MALFORMED envelope
    // — surfaced here so the searched set never SILENTLY shrinks. Presenting six
    // searched classes as though all seven were searched is exactly the false-known
    // rendering the STANDING HONESTY RULE forbids (review-3 item 2).
    let missing: Vec<&str> = RATIFIED_CLASSES
        .iter()
        .map(|r| r.class)
        .filter(|class| !seen_classes.contains(class))
        .collect();
    if !missing.is_empty() {
        out.push_str(&format!(
            "  (malformed find response: fact group(s) missing for class(es): {})\n",
            missing.join(", ")
        ));
    }

    // §2.4 (review-1): a fact-table MISS is established ONLY when nothing matched, every
    // group resolved cleanly (no malformed marker / no unavailable read fired — proven by
    // `clean_groups == groups.len()`), AND the envelope is complete. Anything else — a
    // hit, a malformed payload, an unavailable class, an omitted class — leaves the miss
    // UNESTABLISHED, so the seed tier will NOT render the "nothing matched" capability
    // close (malformed/unknown ≠ absent; STANDING HONESTY RULE 1).
    if !matched_any && clean_groups == groups.len() && missing.is_empty() {
        FactTierOutcome::EstablishedMiss
    } else {
        FactTierOutcome::MissNotEstablished
    }
}

#[cfg(test)]
#[path = "facts_render_tests.rs"]
mod tests;
