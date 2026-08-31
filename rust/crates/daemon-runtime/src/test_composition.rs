//! FIXTURE-POLLUTION-1 — the test-composition of a STRUCTURE-surface row/group/cycle,
//! as a THREE-state fact (never a boolean).
//!
//! # Why a sum type, not a `bool`
//!
//! The v0.11.0-audit fix labels + demotes STRUCTURALLY TEST-ONLY surfaces on the
//! boundaries/cycles surfaces. The basis is the stored `is_test` fact ONLY — a NEUTRAL
//! test-only classification, NOT a provenance claim (there is no fact distinguishing the
//! tool's own fixtures from a user repo's test code; operator ruling fixture-test-scope =
//! Option 1, 2026-08-31). The FIRST cut used `is_test_only: bool` and collapsed "we have
//! no `is_test` evidence for this surface" into `false` = production. The operator's
//! BINDING DIRECTION RULE (2026-08-31, 17th recurrence of the unknown-collapse class)
//! forbids that: DEMOTION requires POSITIVE test-only evidence; UNKNOWN test-composition
//! is NEVER demoted — it stays in the MAIN listing carrying an explicit
//! `test-composition unknown (<reason>)` marker; production is the conservative default
//! ONLY when it is positively evidenced. Three mutually-exclusive states ⇒ a sum type, so
//! the compiler forces every renderer to handle `Unknown` distinctly from `Production`.
//!
//! # Growth axis
//!
//! Variants are FIXED (the domain has exactly three certainty states for "is this surface
//! positively test-only, positively production, or unprovable?"); the OPERATIONS over them
//! grow (JSON emit here; two renderers in `rgr`). Fixed variants + growing operations ⇒
//! sum type + exhaustive match, per the dispatch rule. Adding a fourth state would
//! deliberately break every match — the intended signal.
//!
//! Abstraction record — module: `test_composition`; concrete current users:
//! `boundaries_list_read` (per-row) and `cycle_output::composition` (per-cycle, over the
//! module directory aggregation); axis: the tri-state test-composition classification
//! RESULT + its additive-JSON shape, shared so the two daemon surfaces cannot diverge on
//! what "unknown" renders as; rejected simpler alternative: a `bool`/`Option<bool>` per
//! surface (the review-0 defect — collapsed `Unknown` into `false`/production).

use serde_json::{Map, Value};

/// The additive JSON key carrying the discriminant (`test_only` / `production` /
/// `unknown`). The `rgr` renderers read the SAME key by string (cross-crate DTO contract).
pub(crate) const COMPOSITION_KEY: &str = "test_composition";
/// The additive JSON key carrying the reader-framed reason, present ONLY when the
/// discriminant is `unknown`.
pub(crate) const UNKNOWN_REASON_KEY: &str = "test_composition_unknown_reason";
/// The additive per-row boolean the FIXTURE-POLLUTION-1 contract (§2) names: `true` =
/// positively test-only, `false` = positively production, and JSON `null` = UNKNOWN. The
/// `null` is load-bearing — an absent/unprovable `is_test` fact must NOT read as `false`
/// (production) to a JSON consumer (the binding direction rule, at the wire). Paired with
/// [`UNKNOWN_REASON_KEY`], which carries the reason whenever this is `null`.
pub(crate) const IS_TEST_ONLY_KEY: &str = "is_test_only";

/// The test-composition of a structure surface — is it positively test-only, positively
/// production, or unprovable from the stored `is_test` fact?
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TestComposition {
    /// POSITIVE evidence that every owned file carries the stored `is_test` fact. The ONLY
    /// state that demotes.
    TestOnly,
    /// POSITIVE evidence of ≥1 production (non-test) owned file. Stays in the main listing,
    /// no marker (the conservative default is only reached with real evidence).
    Production,
    /// No reachable `is_test` evidence for this surface (file untracked, fact absent,
    /// malformed node, no owned files). NEVER demoted — stays in the main listing carrying
    /// this reader-framed reason.
    Unknown(String),
}

impl TestComposition {
    /// Classify a single surface from its stored `is_test` fact. `Some(true)` ⇒ test-only;
    /// `Some(false)` ⇒ production; `None` (no tracked-files row for this path) ⇒ UNKNOWN
    /// with a reason naming the subject — NEVER a `false`/production default.
    pub(crate) fn from_is_test_fact(fact: Option<bool>, subject: &str) -> TestComposition {
        match fact {
            Some(true) => TestComposition::TestOnly,
            Some(false) => TestComposition::Production,
            None => TestComposition::Unknown(format!("no stored is_test fact for {subject}")),
        }
    }

    /// Write the additive discriminant (+ reason when `Unknown`) onto a row/cycle JSON
    /// object. A non-object `Value` (never produced by our serializers) is left untouched
    /// rather than silently reshaped.
    pub(crate) fn write_json(&self, value: &mut Value) {
        let Some(obj) = value.as_object_mut() else {
            return;
        };
        self.write_into(obj);
    }

    /// As [`write_json`], for a caller that already holds the object map. Emits BOTH the
    /// contracted additive keys: the string discriminant [`COMPOSITION_KEY`] (the tri-state
    /// the renderers read) AND the boolean [`IS_TEST_ONLY_KEY`] the §2 contract names —
    /// `true`/`false`/`null`, where `null` (never `false`) is the wire form of `Unknown`.
    pub(crate) fn write_into(&self, obj: &mut Map<String, Value>) {
        let discriminant = match self {
            TestComposition::TestOnly => "test_only",
            TestComposition::Production => "production",
            TestComposition::Unknown(_) => "unknown",
        };
        obj.insert(COMPOSITION_KEY.to_string(), Value::from(discriminant));
        // The contracted per-row boolean. UNKNOWN ⇒ JSON `null`, NEVER `false`: an
        // unprovable is_test fact must not read as production to a JSON consumer.
        let is_test_only = match self {
            TestComposition::TestOnly => Value::Bool(true),
            TestComposition::Production => Value::Bool(false),
            TestComposition::Unknown(_) => Value::Null,
        };
        obj.insert(IS_TEST_ONLY_KEY.to_string(), is_test_only);
        if let TestComposition::Unknown(reason) = self {
            obj.insert(UNKNOWN_REASON_KEY.to_string(), Value::from(reason.clone()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_fact_maps_three_states_distinctly() {
        assert_eq!(
            TestComposition::from_is_test_fact(Some(true), "x"),
            TestComposition::TestOnly
        );
        assert_eq!(
            TestComposition::from_is_test_fact(Some(false), "x"),
            TestComposition::Production
        );
        // None is UNKNOWN with a reason — NOT production (the binding direction rule).
        match TestComposition::from_is_test_fact(None, "vendor/x.ts") {
            TestComposition::Unknown(r) => assert!(r.contains("vendor/x.ts"), "{r}"),
            other => panic!("absent fact must be Unknown, got {other:?}"),
        }
    }

    #[test]
    fn write_json_emits_discriminant_and_reason() {
        let mut v = serde_json::json!({});
        TestComposition::TestOnly.write_json(&mut v);
        assert_eq!(v[COMPOSITION_KEY], serde_json::json!("test_only"));
        assert!(v.get(UNKNOWN_REASON_KEY).is_none());

        let mut v = serde_json::json!({});
        TestComposition::Production.write_json(&mut v);
        assert_eq!(v[COMPOSITION_KEY], serde_json::json!("production"));
        assert!(v.get(UNKNOWN_REASON_KEY).is_none());

        let mut v = serde_json::json!({});
        TestComposition::Unknown("no owned files".to_string()).write_json(&mut v);
        assert_eq!(v[COMPOSITION_KEY], serde_json::json!("unknown"));
        assert_eq!(v[UNKNOWN_REASON_KEY], serde_json::json!("no owned files"));
    }

    #[test]
    fn write_json_emits_contracted_is_test_only_with_null_unknown() {
        // §2 contract: an additive per-row `is_test_only`. UNKNOWN must be JSON `null`,
        // NEVER `false` — an unprovable fact cannot read as production at the wire.
        let mut v = serde_json::json!({});
        TestComposition::TestOnly.write_json(&mut v);
        assert_eq!(v[IS_TEST_ONLY_KEY], serde_json::json!(true));

        let mut v = serde_json::json!({});
        TestComposition::Production.write_json(&mut v);
        assert_eq!(v[IS_TEST_ONLY_KEY], serde_json::json!(false));

        let mut v = serde_json::json!({});
        TestComposition::Unknown("no owned files".to_string()).write_json(&mut v);
        assert!(
            v[IS_TEST_ONLY_KEY].is_null(),
            "unknown ⇒ null, not false: {v}"
        );
        // The reason accompanies the null so the consumer knows WHY it is unknown.
        assert_eq!(v[UNKNOWN_REASON_KEY], serde_json::json!("no owned files"));
    }
}
