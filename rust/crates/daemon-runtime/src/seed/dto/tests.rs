//! Wire-level tests for the `find` DTO projection ([`super`] = `seed::dto`) — the
//! CURSOR-ROUNDTRIP-1 (§2.3) `cursor_raw` field in particular.
//!
//! A child test module (NOT a production abstraction): compiles only under
//! `#[cfg(test)]`, reaches `super`'s crate-private `fact_groups` and the
//! `find_facts` taxonomy via `use super::*`, and lives in its own file so the
//! parent stays under the 500-line structural guardrail (the same convention as
//! `find_facts/tests.rs`).

use super::*;
use crate::find_facts::{ClassHits, FactClass, FactHit, HitPath};

fn one_class(class: FactClass, hit: FactHit) -> ClassOutcome {
    ClassOutcome {
        class,
        result: Ok(ClassHits {
            hits: vec![hit],
            matched: 1,
            matched_is_floor: false,
        }),
    }
}

fn symbol_hit(key: &str) -> FactHit {
    FactHit {
        display: "BNRService".to_string(),
        path: HitPath::Known("src/bnr.ts".to_string()),
        key: Some(key.to_string()),
        next_command: None,
        line: Some(1),
        evidence: None,
    }
}

#[test]
fn cursor_raw_serializes_the_uid_stripped_verbless_unquoted_cursor() {
    // CURSOR-ROUNDTRIP-1 (§2.3, revision 1): the SERIALIZED `cursor_raw` is the raw cursor
    // TOKEN — uid-stripped, verb-less, unquoted — NOT `explain <full-key>`. `next` keeps
    // the full, self-contained runnable command.
    let uid = "repo_abc123";
    let key = format!("{uid}:src/bnr.ts:BNRService:SYMBOL:CLASS");
    let v = serde_json::to_value(fact_groups(
        &[one_class(FactClass::Symbol, symbol_hit(&key))],
        uid,
    ))
    .expect("serialize fact groups");
    let hit = &v[0]["hits"][0];
    assert_eq!(
        hit["cursor_raw"],
        json!("src/bnr.ts:BNRService:SYMBOL:CLASS"),
        "cursor_raw is the uid-stripped raw cursor: {hit}"
    );
    // The full runnable command is untouched (byte-stable for existing consumers).
    assert_eq!(hit["next"], json!(format!("explain {key}")), "{hit}");
}

#[test]
fn cursor_raw_stays_unquoted_for_a_suffix_with_whitespace() {
    // A symbol whose path carries a space: `cursor_raw` is the bare suffix (the agent passes
    // it as ONE argument); only the human render shell-quotes. The whitespace case the
    // revision-1 reviewer required at the serialization layer.
    let uid = "repo_abc123";
    let key = format!("{uid}:src/my file.ts:foo:SYMBOL:FUNCTION");
    let v = serde_json::to_value(fact_groups(
        &[one_class(FactClass::Symbol, symbol_hit(&key))],
        uid,
    ))
    .expect("serialize fact groups");
    assert_eq!(
        v[0]["hits"][0]["cursor_raw"],
        json!("src/my file.ts:foo:SYMBOL:FUNCTION"),
        "{v}"
    );
}

#[test]
fn listing_class_hit_omits_cursor_raw() {
    // A whole-listing class (dependency) has no cursor argument → `cursor_raw` is
    // skip-serialized entirely: a cursor field only where a cursor exists, never an empty
    // or fabricated placeholder (STANDING HONESTY RULE 1).
    let dep = one_class(
        FactClass::Dependency,
        FactHit {
            display: "lodash".to_string(),
            path: HitPath::None,
            key: Some("lodash".to_string()),
            next_command: None,
            line: None,
            evidence: None,
        },
    );
    let v = serde_json::to_value(fact_groups(&[dep], "repo_abc123")).expect("serialize");
    let hit = &v[0]["hits"][0];
    assert!(
        hit.get("cursor_raw").is_none(),
        "listing class carries no cursor_raw: {hit}"
    );
    // `next` is still the bare whole-listing command (unchanged).
    assert_eq!(hit["next"], json!("deps list"), "{hit}");
}
