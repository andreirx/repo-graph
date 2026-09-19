//! EXPLAIN-CYCLES-HONEST-1 (§2.1.3): the ONE strict validation of a carried cycle `walk`, shared
//! by `orient`'s headline chain formatter and `explain`'s Import-cycles ring renderer.
//!
//! Abstraction one-liner — WHAT: validate a JSON `walk` array into a directed ring of ≥2 non-empty
//! display names, or `None` (drift). CONCRETE CURRENT USERS: `orient_guidance::format_cycle_anchor`
//! (the headline `A -> B -> C -> ... -> A` chain) and `explain_sections::render_cycles` (the
//! Import-cycles ring). AXIS: the honesty rule "a walk is a directed ring of ≥2 non-empty display
//! names or it is drift" (COHERENCE-3 review-1 #1 / STANDING HONESTY RULE #1) — a correctness rule
//! copied is a correctness rule that drifts. REJECTED SIMPLER: leaving the 10-line validation inline
//! in `orient_guidance` and re-implementing it in `explain` — two copies of the exact rule
//! (`explain` had NONE, and drew a `->` ring from the lexically-sorted member set: RC-4).

use serde_json::Value;

/// Validate a carried `walk` into its ordered ring of DISPLAY names, or `None` on drift.
///
/// The producer (`storage::agent_cycle_labeling` via `agent::find_cycle_walk`) only ever emits a
/// ring of ≥2 non-empty DISPLAY strings, or `None` (which serializes as an absent/`null` leaf the
/// caller routes to the unordered form). So ANY non-string element, ANY empty string, or fewer than
/// 2 members reaching here is wire/schema DRIFT, not a walk → return `None` so the caller makes the
/// unknown VISIBLE with its reason, NEVER a fabricated ring. The prior `orient` code's
/// `filter_map(as_str)` silently dropped non-strings, turning a two-element `["A", 42]` into the
/// invented self-cycle `A -> A`; that is exactly the fabrication this rejects.
pub(crate) fn validate_walk(walk: &[Value]) -> Option<Vec<&str>> {
    let mut names: Vec<&str> = Vec::with_capacity(walk.len());
    for m in walk {
        let s = m.as_str()?; // non-string element => drift => None (no silent drop)
        if s.is_empty() {
            return None; // empty display name => drift => None
        }
        names.push(s);
    }
    if names.len() < 2 {
        // A real directed ring closes over ≥2 distinct members (a self-import is not a cycle
        // edge). A one-element walk is the fabricated `A -> A` the reviewer flagged.
        return None;
    }
    Some(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_ring_of_two_or_more_passes() {
        let walk = vec![Value::from("util"), Value::from("helpers/memenv")];
        assert_eq!(validate_walk(&walk), Some(vec!["util", "helpers/memenv"]));
    }

    #[test]
    fn non_string_element_is_drift_not_dropped() {
        // The RC-4 fabrication shape: `["A", 42]` must NOT collapse to a one-name (self) ring.
        let walk = vec![Value::from("A"), Value::from(42)];
        assert_eq!(validate_walk(&walk), None);
    }

    #[test]
    fn empty_string_member_is_drift() {
        let walk = vec![Value::from("A"), Value::from("")];
        assert_eq!(validate_walk(&walk), None);
    }

    #[test]
    fn one_element_walk_is_not_a_ring() {
        let walk = vec![Value::from("A")];
        assert_eq!(validate_walk(&walk), None);
    }

    #[test]
    fn empty_walk_is_none() {
        assert_eq!(validate_walk(&[]), None);
    }
}
