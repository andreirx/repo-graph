//! TEST-EDGE-SCOPE-1A — the ONE home of RG-REQ-001-L07's UNDETERMINED test-status wording
//! (D-TESA-04) and of the universe nouns (D-TESA-05).
//!
//! Every partitioned surface's daemon response carries a `test_status_undetermined` block:
//! `{"count":N,"paths":[…],"universe":u,"universe_count":M,"unknown_count":K}` or, when a read
//! the block needs failed, `{"unavailable":reason,"universe":u}`. Each renderer calls
//! [`undetermined_files_line`] at its site; `explain` calls [`explain_test_status_line`]. No
//! other renderer types the sentence.
//!
//! Honesty: an absent block (a pre-1A daemon) renders nothing — no statement is made; a count
//! of 0 renders nothing; a failed read renders a named unknown; a malformed block or an
//! unknown universe renders "unreadable on this response" — never a defaulted zero
//! (RG-REQ-002-L04).

use serde_json::Value;

/// `explain <file>`'s line for a file whose test status can't be determined (L07, verbatim).
const EXPLAIN_UNDETERMINED: &str = "test status: can't determine — open it and look inside";

/// The fixed L07 count sentence, plural and singular (D-TESA-04).
const PLURAL: &str = "files whose test status can't be determined — open them and look inside";
const SINGULAR: &str = "file whose test status can't be determined — open it and look inside";
/// The subject used for a named unknown or an unreadable block.
const SUBJECT: &str = "files whose test status can't be determined";

/// The reader noun for each closed universe (D-TESA-05); `None` for an unknown universe.
fn universe_noun(universe: &str) -> Option<&'static str> {
    Some(match universe {
        "ranked_files" => "ranked files",
        "hotspot_files" => "hotspot files",
        "call_files" => "files with measured calls",
        "surface_files" => "files with HTTP surfaces",
        "boundary_files" => "files with boundary surfaces",
        "inference_files" => "files with inferences",
        "candidate_files" => "candidate files",
        "owned_files" => "owned files",
        "grouped_files" => "grouped files",
        _ => return None,
    })
}

fn unreadable() -> String {
    format!("{SUBJECT}: unreadable on this response")
}

/// The counted-shape fields; a block carrying any of them is not the unavailable shape.
const COUNTED_FIELDS: [&str; 4] = ["count", "paths", "universe_count", "unknown_count"];

/// One of the two contracted block shapes, validated (review-0 F-1: a block renders only
/// when its evidence is complete and internally consistent — otherwise it is unreadable).
enum Block<'a> {
    /// `count` undetermined files (the `paths`), of `universe_count`, `unknown_count` unknown.
    Counted {
        count: u64,
        universe_count: u64,
        unknown_count: u64,
        noun: &'static str,
    },
    /// A read the block needs failed, with its reason.
    Unavailable { reason: &'a str, noun: &'static str },
}

/// Validate a `test_status_undetermined` block against the contracted shape. `Err(())` for
/// anything else — a non-object, an unknown universe, a mixed or partial shape, a non-string
/// or duplicate path, `paths` disagreeing with `count`, or `count + unknown_count` exceeding
/// `universe_count` (impossible bounds). Additive unknown keys are tolerated.
fn parse_block(block: &Value) -> Result<Block<'_>, ()> {
    let obj = block.as_object().ok_or(())?;
    let noun = obj
        .get("universe")
        .and_then(Value::as_str)
        .and_then(universe_noun)
        .ok_or(())?;
    if let Some(reason) = obj.get("unavailable") {
        let reason = reason.as_str().filter(|r| !r.is_empty()).ok_or(())?;
        if COUNTED_FIELDS.iter().any(|f| obj.contains_key(*f)) {
            return Err(());
        }
        return Ok(Block::Unavailable { reason, noun });
    }
    let count = obj.get("count").and_then(Value::as_u64).ok_or(())?;
    let universe_count = obj
        .get("universe_count")
        .and_then(Value::as_u64)
        .ok_or(())?;
    let unknown_count = obj.get("unknown_count").and_then(Value::as_u64).ok_or(())?;
    let paths = obj.get("paths").and_then(Value::as_array).ok_or(())?;
    let mut seen = std::collections::BTreeSet::new();
    for p in paths {
        let p = p.as_str().filter(|p| !p.is_empty()).ok_or(())?;
        if !seen.insert(p) {
            return Err(());
        }
    }
    // `count + unknown_count` must fit the universe; an overflowing sum is impossible, never
    // "fits" (review-1 F-1: checked, not saturating, addition).
    let accounted = count.checked_add(unknown_count).ok_or(())?;
    if paths.len() as u64 != count || accounted > universe_count {
        return Err(());
    }
    Ok(Block::Counted {
        count,
        universe_count,
        unknown_count,
        noun,
    })
}

/// The count line for a surface's `test_status_undetermined` block.
///
/// - absent → `None` (an older daemon; no statement);
/// - count 0 → `None`;
/// - count 1 → `1 file whose test status can't be determined — open it and look inside (of M <noun>)`;
/// - count N ≥ 2 → `N files whose test status can't be determined — open them and look inside (of M <noun>)`;
/// - with K ≥ 1 unknown-status files the parenthesis reads `(of M <noun>; K with unknown test status)`;
/// - `unavailable` → `files whose test status can't be determined: unknown — <reason> (among the <noun>)`;
/// - anything not exactly one contracted, consistent shape ([`parse_block`]) →
///   `files whose test status can't be determined: unreadable on this response`.
pub(crate) fn undetermined_files_line(block: Option<&Value>) -> Option<String> {
    let block = block?;
    match parse_block(block) {
        Err(()) => Some(unreadable()),
        Ok(Block::Unavailable { reason, noun }) => {
            Some(format!("{SUBJECT}: unknown — {reason} (among the {noun})"))
        }
        Ok(Block::Counted { count: 0, .. }) => None,
        Ok(Block::Counted {
            count,
            universe_count,
            unknown_count,
            noun,
        }) => {
            let sentence = if count == 1 { SINGULAR } else { PLURAL };
            let unknown = if unknown_count >= 1 {
                format!("; {unknown_count} with unknown test status")
            } else {
                String::new()
            };
            Some(format!(
                "{count} {sentence} (of {universe_count} {noun}{unknown})"
            ))
        }
    }
}

/// `explain <file>`'s test-status line from the EXPLAIN_IDENTITY evidence: the L07 sentence
/// when `test_status` is `"undetermined"` AND `test_status_word` (its evidence) is a non-empty
/// string; nothing when neither key is present (a determined file, a non-file target, or a
/// pre-1A daemon); unreadable for anything else (a missing or malformed word, an unknown
/// status value, or a word without a status).
pub(crate) fn explain_test_status_line(identity: &Value) -> Option<String> {
    let status = identity.get("test_status");
    let word = identity.get("test_status_word");
    match (status, word) {
        (None, None) => None,
        (Some(s), Some(w))
            if s.as_str() == Some("undetermined") && w.as_str().is_some_and(|w| !w.is_empty()) =>
        {
            Some(EXPLAIN_UNDETERMINED.to_string())
        }
        _ => Some("test status: unreadable on this response".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A contract-consistent counted block: `count` distinct paths.
    fn blk(count: u64, universe: &str, m: u64, k: u64) -> Value {
        let paths: Vec<String> = (0..count).map(|i| format!("src/test_{i}.c")).collect();
        json!({"count": count, "paths": paths, "universe": universe,
               "universe_count": m, "unknown_count": k})
    }

    #[test]
    fn undetermined_line_is_plural_singular_and_absent_at_zero() {
        assert_eq!(
            undetermined_files_line(Some(&blk(4, "owned_files", 120, 0))).unwrap(),
            "4 files whose test status can't be determined — open them and look inside (of 120 owned files)"
        );
        assert_eq!(
            undetermined_files_line(Some(&blk(1, "owned_files", 120, 0))).unwrap(),
            "1 file whose test status can't be determined — open it and look inside (of 120 owned files)"
        );
        assert_eq!(
            undetermined_files_line(Some(&blk(0, "owned_files", 120, 0))),
            None
        );
    }

    #[test]
    fn undetermined_line_carries_each_universe_with_its_count() {
        for (u, noun) in [
            ("ranked_files", "ranked files"),
            ("hotspot_files", "hotspot files"),
            ("call_files", "files with measured calls"),
            ("surface_files", "files with HTTP surfaces"),
            ("boundary_files", "files with boundary surfaces"),
            ("inference_files", "files with inferences"),
            ("candidate_files", "candidate files"),
            ("owned_files", "owned files"),
            ("grouped_files", "grouped files"),
        ] {
            let line = undetermined_files_line(Some(&blk(2, u, 9, 0))).unwrap();
            assert!(line.ends_with(&format!("(of 9 {noun})")), "{line}");
        }
    }

    #[test]
    fn undetermined_line_names_an_unavailable_reason_and_universe() {
        let b = json!({"unavailable": "call-source files could not be read: db locked",
                       "universe": "call_files"});
        assert_eq!(
            undetermined_files_line(Some(&b)).unwrap(),
            "files whose test status can't be determined: unknown — call-source files could \
             not be read: db locked (among the files with measured calls)"
        );
    }

    #[test]
    fn undetermined_line_malformed_or_unknown_universe_is_unreadable_never_zero() {
        let unreadable = "files whose test status can't be determined: unreadable on this response";
        for b in [
            json!("nope"),
            json!({"count": 2, "universe": "mystery_files", "universe_count": 3, "unknown_count": 0}),
            json!({"count": "2", "universe": "owned_files", "universe_count": 3, "unknown_count": 0}),
            json!({"universe": "owned_files", "universe_count": 3, "unknown_count": 0}),
            json!({"count": 2, "universe": "owned_files", "universe_count": 3}),
            json!({"unavailable": 7, "universe": "owned_files"}),
            json!({"unavailable": "x", "universe": "mystery_files"}),
            // review-0 F-1 — the malformed-evidence class, together:
            // absent paths
            json!({"count": 2, "universe": "owned_files", "universe_count": 3, "unknown_count": 0}),
            // count/path disagreement (both directions, incl. a zero count with paths)
            json!({"count": 2, "paths": [], "universe": "owned_files", "universe_count": 3, "unknown_count": 0}),
            json!({"count": 1, "paths": ["a/test.c", "b/test.c"], "universe": "owned_files", "universe_count": 3, "unknown_count": 0}),
            json!({"count": 0, "paths": ["a/test.c"], "universe": "owned_files", "universe_count": 3, "unknown_count": 0}),
            // non-string, empty or duplicate paths
            json!({"count": 1, "paths": [7], "universe": "owned_files", "universe_count": 3, "unknown_count": 0}),
            json!({"count": 1, "paths": [""], "universe": "owned_files", "universe_count": 3, "unknown_count": 0}),
            json!({"count": 2, "paths": ["a/test.c", "a/test.c"], "universe": "owned_files", "universe_count": 3, "unknown_count": 0}),
            // impossible count bounds
            json!({"count": 2, "paths": ["a/test.c", "b/test.c"], "universe": "owned_files", "universe_count": 1, "unknown_count": 0}),
            json!({"count": 1, "paths": ["a/test.c"], "universe": "owned_files", "universe_count": 2, "unknown_count": 2}),
            // review-1 F-1: `count + unknown_count` overflowing u64 is impossible, never "fits"
            json!({"count": 1, "paths": ["a/test.c"], "universe": "owned_files", "universe_count": u64::MAX, "unknown_count": u64::MAX}),
            // a mixed shape, and an empty reason
            json!({"unavailable": "db locked", "count": 0, "paths": [], "universe": "owned_files", "universe_count": 0, "unknown_count": 0}),
            json!({"unavailable": "", "universe": "owned_files"}),
        ] {
            assert_eq!(
                undetermined_files_line(Some(&b)).as_deref(),
                Some(unreadable),
                "{b}"
            );
        }
    }

    #[test]
    fn undetermined_line_absent_field_renders_nothing() {
        assert_eq!(undetermined_files_line(None), None);
    }

    #[test]
    fn undetermined_line_states_the_unknown_status_files_of_its_universe() {
        assert_eq!(
            undetermined_files_line(Some(&blk(1, "surface_files", 3, 1))).unwrap(),
            "1 file whose test status can't be determined — open it and look inside \
             (of 3 files with HTTP surfaces; 1 with unknown test status)"
        );
        // K alone (count 0) states nothing: the surface's own unknown disclosure covers it.
        assert_eq!(
            undetermined_files_line(Some(&blk(0, "surface_files", 3, 1))),
            None
        );
    }

    /// review-0 F-1: `explain`'s line requires the status AND its word; any other combination is
    /// unreadable, never the investigative sentence; neither key → nothing.
    #[test]
    fn explain_test_status_line_requires_status_and_word_together() {
        let ok = json!({"test_status": "undetermined", "test_status_word": "testutil"});
        assert_eq!(
            explain_test_status_line(&ok).as_deref(),
            Some("test status: can't determine — open it and look inside")
        );
        assert_eq!(
            explain_test_status_line(&json!({"target_kind": "file"})),
            None
        );
        for bad in [
            json!({"test_status": "undetermined"}),
            json!({"test_status": "undetermined", "test_status_word": ""}),
            json!({"test_status": "undetermined", "test_status_word": 3}),
            json!({"test_status": "determined", "test_status_word": "test"}),
            json!({"test_status_word": "test"}),
        ] {
            assert_eq!(
                explain_test_status_line(&bad).as_deref(),
                Some("test status: unreadable on this response"),
                "{bad}"
            );
        }
    }

    /// The producer's serialized blocks (the agent DTO `UndeterminedTestFiles`, whose counts the
    /// daemon's `block` is pinned equal to) pass the renderer's validation: a counted block renders
    /// its line, a zero block renders nothing, an unavailable block renders its named unknown.
    #[test]
    fn producer_serialized_blocks_pass_the_renderer_validation() {
        use repo_graph_agent::{TestStatusUniverse, UndeterminedTestFiles};
        let counted = UndeterminedTestFiles::over_partition(
            TestStatusUniverse::OwnedFiles,
            [
                ("db/c_test.c", Some(false)),
                ("util/testutil.cc", Some(false)),
                ("db/db_impl.cc", Some(false)),
                ("x/unknown_test.c", None),
            ],
        );
        let v = serde_json::to_value(&counted).unwrap();
        assert_eq!(
            undetermined_files_line(Some(&v)).as_deref(),
            Some(
                "2 files whose test status can't be determined — open them and look inside \
                 (of 4 owned files; 1 with unknown test status)"
            )
        );
        let zero = UndeterminedTestFiles::over_partition(
            TestStatusUniverse::CallFiles,
            [("db/db_impl.cc", Some(false))],
        );
        assert_eq!(
            undetermined_files_line(Some(&serde_json::to_value(&zero).unwrap())),
            None
        );
        let grouped = UndeterminedTestFiles::over_grouped_files(
            [("util/testutil.cc", false), ("util/env.cc", false)],
            3,
            1,
        );
        assert_eq!(
            undetermined_files_line(Some(&serde_json::to_value(&grouped).unwrap())).as_deref(),
            Some(
                "1 file whose test status can't be determined — open it and look inside \
                 (of 2 grouped files)"
            )
        );
        let unavailable =
            UndeterminedTestFiles::unavailable(TestStatusUniverse::CallFiles, "db locked");
        assert_eq!(
            undetermined_files_line(Some(&serde_json::to_value(&unavailable).unwrap())).as_deref(),
            Some(
                "files whose test status can't be determined: unknown — db locked \
                 (among the files with measured calls)"
            )
        );
    }
}
