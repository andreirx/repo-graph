//! TYPE-ONLY-IMPORTS-1 rules, one definition (TEST-EDGE-SCOPE-1B, D-TESB-04).
//!
//! The per-import `import type` disposition parse and the conjunctive module-edge
//! aggregate, moved verbatim from `resolver.rs` (`import_edge_type_only`'s body) and
//! `orchestrator.rs` (`aggregate_module_edge_type_only`) so the indexer and the
//! query-time directory-module derivation in storage read ONE rule. No body changed.

use crate::storage_port::TypeOnlyDisposition;

/// TYPE-ONLY-IMPORTS-1: the `import type` disposition of a resolved IMPORTS edge, read from the
/// `isTypeOnly` key its `metadata_json` carries. The key is injected at extraction
/// (`orchestrator::inject_import_type_only`) from the parallel `ImportObservation` — the extractor fact
/// at `ts-extractor:1350` is the single source; this is PLUMBING, not new extraction.
///
/// `Some(TypeOnly)` = a TS/JS `import type` / `export type … from` (vanishes at runtime);
/// `Some(Runtime)` = a runtime import (every non-TS/JS import is runtime by definition, stamped at
/// injection). `None` = the key is ABSENT (no `metadata_json`, or valid JSON without the key — a snapshot
/// indexed before type-only tracking, copied forward without the fact) — unknown.
///
/// `Some(Unreadable)` = the carrier was PRESENT but could not be read: `metadata_json` did not parse as
/// JSON, or its `isTypeOnly` value was not a boolean. This is a DISTINCT truth from an absent fact
/// (operator ruling 2026-09-03 item 2a — a corrupt fact and a pre-migration row are different truths);
/// every non-`Runtime` case is carried through as its own state, NEVER demoted to runtime.
pub fn type_only_disposition_of(metadata_json: Option<&str>) -> Option<TypeOnlyDisposition> {
    // No carrier at all ⇒ the fact is ABSENT (not present, not corrupt).
    let raw = metadata_json?;
    // A carrier that does not parse is CORRUPT, distinct from absent — NOT silently swallowed.
    let value: serde_json::Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return Some(TypeOnlyDisposition::Unreadable),
    };
    match value.get("isTypeOnly") {
        // Valid JSON without the key ⇒ the fact was never stamped ⇒ ABSENT (indexed before tracking).
        None => None,
        Some(serde_json::Value::Bool(true)) => Some(TypeOnlyDisposition::TypeOnly),
        Some(serde_json::Value::Bool(false)) => Some(TypeOnlyDisposition::Runtime),
        // Key present but not a boolean ⇒ a CORRUPT value, distinct from absent.
        Some(_) => Some(TypeOnlyDisposition::Unreadable),
    }
}

/// TYPE-ONLY-IMPORTS-1: the conjunctive aggregate for a MODULE→MODULE IMPORTS edge over its contributing
/// file-level import dispositions. A module edge is type-only iff EVERY contributing import is type-only.
/// The precedence encodes "runtime dominates; a corrupt fact is louder than an absent one":
///   - any `Some(Runtime)` present ⇒ `Some(Runtime)` — a confirmed runtime coupling (dominates all),
///   - else any `Some(Unreadable)` present ⇒ `Some(Unreadable)` — a corrupt contributor blocks a
///     type-only verdict AND is a distinct truth from an absent one (surfaces its own Unknown reason),
///   - else any `None` present ⇒ `None` — an absent contributor (can't confirm ALL type-only ⇒ unknown,
///     left NULL in the store: "indexed before type-only tracking"),
///   - else (all `Some(TypeOnly)`) ⇒ `Some(TypeOnly)`.
///
/// The per-file-specific parse error cannot survive the aggregate + the NULL/int column; `Unreadable`
/// carries the CATEGORY (corrupt) forward, which the serve renders as its own Unknown reason.
pub fn aggregate_module_edge_type_only(
    contributors: &[Option<TypeOnlyDisposition>],
) -> Option<TypeOnlyDisposition> {
    use TypeOnlyDisposition::*;
    if contributors.is_empty() {
        // No contributor ⇒ cannot confirm "all type-only" ⇒ unknown (left NULL). Not reachable from
        // `create_module_edges` (a pair exists only because ≥1 file import fed it), but correct here.
        None
    } else if contributors.contains(&Some(Runtime)) {
        Some(Runtime)
    } else if contributors.contains(&Some(Unreadable)) {
        Some(Unreadable)
    } else if contributors.contains(&None) {
        None
    } else {
        Some(TypeOnly)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_only_disposition_of_metadata_reads_absent_typed_and_unreadable() {
        use TypeOnlyDisposition::*;
        assert_eq!(type_only_disposition_of(None), None, "no carrier is absent");
        assert_eq!(
            type_only_disposition_of(Some(r#"{"other":1}"#)),
            None,
            "valid JSON without the key is absent"
        );
        assert_eq!(
            type_only_disposition_of(Some(r#"{"isTypeOnly":true}"#)),
            Some(TypeOnly)
        );
        assert_eq!(
            type_only_disposition_of(Some(r#"{"isTypeOnly":false}"#)),
            Some(Runtime)
        );
        assert_eq!(
            type_only_disposition_of(Some("{not json")),
            Some(Unreadable),
            "a carrier that does not parse is corrupt, not absent"
        );
        assert_eq!(
            type_only_disposition_of(Some(r#"{"isTypeOnly":"yes"}"#)),
            Some(Unreadable),
            "a non-boolean value is corrupt, not absent"
        );
    }
}
