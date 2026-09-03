//! TYPE-ONLY-IMPORTS-1 — the ONE cycle runtime-vs-type-only verdict derivation.
//!
//! # Why this module exists
//!
//! Two surfaces render module-cycle info: `cycles` (labels each type-only cycle "type-only
//! (vanishes at runtime)") and `orient`'s cycle leaf. The per-cycle verdict is honesty-critical
//! — a wrong label tells an agent a real runtime coupling is a compile-time phantom (or the
//! reverse), the exact false-orientation the slice exists to remove. So the two surfaces MUST
//! derive the verdict from ONE function, never two that could drift.
//!
//! The verdict + the §5 TS/JS membership gate previously lived ONLY in `daemon-runtime`
//! (`cycle_output::{type_only,ts_caveat}`), unreachable from the `storage` adapter that serves
//! `orient`'s cycles (`daemon-runtime` depends on `storage`, never the reverse). This module
//! hoists the PURE derivations into the port-owner `agent` crate, which BOTH `storage`
//! (adapter → `orient`) and `daemon-runtime` (`cycle_output` → `cycles`) already depend on —
//! the SAME move [`crate::cycle_composition`] made for the test-only split. One kernel, called
//! from both serving computations ⇒ the two verdicts cannot drift.
//!
//! Abstraction record — module: `cycle_type_only`; concrete current users: the `storage`
//! adapter's `agent_cycle_labeling::label_module_cycles` (labels `orient`'s cycles) AND
//! `daemon-runtime`'s `cycle_output::type_only::attach_type_only_labels` (labels `cycles`);
//! axis: none — the ONE conjunctive verdict + the ONE TS/JS membership gate, shared so the two
//! surfaces derive from a single function; rejected simpler alternative: leaving them in
//! `daemon-runtime` and duplicating in the `storage` adapter — two copies of honesty-critical
//! logic that could drift back into a per-surface disagreement.
//!
//! # Growth axis
//!
//! Variants FIXED (a cycle vanishes at runtime, has ≥1 runtime edge, or is unprovable — exactly
//! three certainty states), operations GROWING (JSON emit in `daemon-runtime`; the `orient` leaf
//! in the `agent` aggregator; the `cycles` + `orient` renderers). Fixed variants + growing
//! operations ⇒ sum type + exhaustive match. A fourth state deliberately breaks every match.
//!
//! The verdict basis is the stored per-module-edge `is_type_only` fact ONLY — NEVER a path/name
//! heuristic (STANDING HONESTY RULE #2). A failed/absent/corrupt fact is `Unknown{reason}`, each
//! cause its OWN reason string, NEVER demoted to `HasRuntimeEdges`.

use std::collections::{BTreeSet, HashMap};

use serde::Serialize;

/// The `import type` disposition of ONE contributing MODULE→MODULE IMPORTS edge — the pure
/// domain input to [`classify_cycle_type_only`]. A BOUNDARY DTO: callers (`daemon-runtime`,
/// `storage`) map their `indexer::storage_port::TypeOnlyDisposition` into this before calling.
///
/// Abstraction one-liner — WHAT: pure per-edge type-only disposition DTO; USERS: the shared
/// verdict + membership kernel, fed by both the `cycles` (daemon) and `orient` (storage) serving
/// computations; AXIS: none; REJECTED SIMPLER: `agent` depending on `indexer::TypeOnlyDisposition`
/// directly — a domain→mechanism dependency-rule inversion (the domain crate must not depend on
/// the extraction/persistence mechanism). Absence of the fact is `Option::None` around this enum,
/// never a variant (Option is the one representation of absence).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeTypeOnly {
    /// A TS/JS `import type` — the edge VANISHES at runtime.
    TypeOnly,
    /// A confirmed runtime import edge.
    Runtime,
    /// The stored fact was PRESENT but UNREADABLE (corrupt) — a distinct truth from absent.
    Unreadable,
}

/// The per-cycle runtime-vs-type-only verdict — the shared classification RESULT the serving
/// computations attach to their cycle DTOs. Three mutually-exclusive certainty states ⇒ a sum
/// type; `Unknown` carries the reader-framed reason (each cause its own string) and is NEVER
/// demoted to a runtime claim.
///
/// Serialized as `{ "kind": <snake_case>[, "reason": <string>] }` — the EXACT JSON the `cycles`
/// renderer (`rgr::presentation::cycles::CycleTypeOnly`) deserializes and the `orient` leaf
/// (`CycleEvidence::type_only`) carries. `daemon-runtime` and `agent` serialize through THIS one
/// type, so the two routes emit byte-identical JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CycleTypeOnly {
    /// EVERY edge in the cycle's walk is a TS/JS `import type` — the whole cycle vanishes at runtime.
    TypeOnly,
    /// ≥1 edge is a confirmed runtime import — a real runtime cycle (rendered WITHOUT a label).
    HasRuntimeEdges,
    /// The verdict could not be computed. NEVER demoted to `HasRuntimeEdges`.
    Unknown { reason: String },
}

/// Is an indexer `files.language` token a member of the TypeScript/JavaScript family
/// (`typescript` | `tsx` | `javascript` | `jsx`)? The ONE home of that token vocabulary, so the
/// `cycles` (daemon) and `orient` (storage) membership gates never re-spell it. Hoisted here from
/// `daemon-runtime::reader_context` (which now re-exports this) so `storage` reaches the SAME
/// vocabulary — required for the two routes to agree on WHICH cycles are TS/JS (§5).
pub fn is_ts_js_language_token(token: &str) -> bool {
    matches!(token, "typescript" | "tsx" | "javascript" | "jsx")
}

/// Is `path` inside module directory `dir` (equal to it, or under it on a path-segment
/// boundary)? The empty string is the repo-root module and contains every path.
fn path_in_module(path: &str, dir: &str) -> bool {
    dir.is_empty() || path == dir || path.starts_with(&format!("{dir}/"))
}

/// TYPE-ONLY-IMPORTS-1 §5: the subset of `member_dirs` that DEEPEST-own ≥1 TS/JS file — the cycle
/// member modules that are TypeScript/JavaScript. `member_dirs` is the union of the rendered
/// cycles' member module directories; `all_module_dirs` is every module's qualified directory
/// (for deepest-ownership resolution — MUST be a superset of `member_dirs`); `files` is
/// `(path, language_token)` from the tracked-files table (a `None` language is never TS/JS).
///
/// Deepest-ownership (the longest module dir that path-contains the file) over the FULL module set
/// — not just the cycle members — keeps a nested non-cycle module's TS file from being wrongly
/// attributed to a cycle-member ancestor (VISION: precision matters for cycles). Returns the
/// per-member set so the per-cycle type-only verdict is attached ONLY to cycles with a TS/JS
/// member; a non-TS cycle gets NO verdict (§5: other languages' import edges are runtime by
/// definition — label ABSENT, not Unknown), keeping its output byte-stable.
pub fn ts_js_cycle_member_dirs(
    member_dirs: &BTreeSet<String>,
    all_module_dirs: &[String],
    files: &[(&str, Option<&str>)],
) -> BTreeSet<String> {
    let mut ts_members = BTreeSet::new();
    if member_dirs.is_empty() {
        return ts_members;
    }
    for (path, lang) in files {
        if !lang.is_some_and(is_ts_js_language_token) {
            continue;
        }
        let owner = all_module_dirs
            .iter()
            .filter(|d| path_in_module(path, d))
            .max_by_key(|d| d.len());
        if let Some(owner) = owner {
            if member_dirs.contains(owner.as_str()) {
                ts_members.insert(owner.clone());
            }
        }
    }
    ts_members
}

/// The conjunctive verdict for ONE cycle, over its intra-SCC MODULE→MODULE IMPORTS edges'
/// dispositions (`None` = the fact is absent / SQL `NULL`). A cycle is type-only iff EVERY edge in
/// its walk is type-only; runtime dominates; a corrupt fact outranks a merely-absent one:
///   - empty bucket ⇒ `Unknown` "cycle import edges unavailable" (cannot verify),
///   - any `Some(Runtime)` ⇒ `HasRuntimeEdges` (a real runtime cycle),
///   - else any `Some(Unreadable)` ⇒ `Unknown` "type-only fact unreadable" (a CORRUPT fact — a
///     distinct truth from an absent one, operator ruling 2026-09-03 item 2a),
///   - else any `None` ⇒ `Unknown` "indexed before type-only tracking" (never demoted — RULE #2),
///   - else (all `Some(TypeOnly)`) ⇒ `TypeOnly` (the whole cycle vanishes at runtime).
///
/// The `Unknown` reason CARRIED here is what the renderer prints verbatim (operator ruling item
/// 2b — no reason invention at the render site).
pub fn classify_cycle_type_only(dispositions: &[Option<EdgeTypeOnly>]) -> CycleTypeOnly {
    use EdgeTypeOnly::*;
    if dispositions.is_empty() {
        CycleTypeOnly::Unknown {
            reason: "cycle import edges unavailable".to_string(),
        }
    } else if dispositions.contains(&Some(Runtime)) {
        CycleTypeOnly::HasRuntimeEdges
    } else if dispositions.contains(&Some(Unreadable)) {
        CycleTypeOnly::Unknown {
            reason: "type-only fact unreadable".to_string(),
        }
    } else if dispositions.contains(&None) {
        CycleTypeOnly::Unknown {
            reason: "indexed before type-only tracking".to_string(),
        }
    } else {
        CycleTypeOnly::TypeOnly
    }
}

/// Decorate a WHOLE cycle set with the per-cycle type-only verdict — the single function BOTH the
/// `cycles` (daemon) and `orient` (storage) serving computations call, so they cannot disagree.
///
/// - `cycle_members[i]` — cycle `i`'s members as `(node_id, qualified_dir)` pairs (node_id maps
///   the edges; qualified_dir gates §5 membership).
/// - `module_edges` — the snapshot's MODULE→MODULE IMPORTS set as `(from_id, to_id, disposition)`.
/// - `files` — `(path, language_token)` for §5 TS/JS membership.
/// - `all_module_dirs` — every module's qualified directory (deepest-ownership resolution).
///
/// Returns one `Option<CycleTypeOnly>` per input cycle, in order: `None` for a non-TS/JS cycle
/// (§5 — label absent, byte-stable); `Some(verdict)` otherwise.
pub fn classify_cycles_type_only(
    cycle_members: &[Vec<(&str, &str)>],
    module_edges: &[(&str, &str, Option<EdgeTypeOnly>)],
    files: &[(&str, Option<&str>)],
    all_module_dirs: &[String],
) -> Vec<Option<CycleTypeOnly>> {
    // The union of every rendered cycle's member module directories — the membership universe.
    let mut member_dirs = BTreeSet::new();
    for members in cycle_members {
        for (_, qual) in members {
            member_dirs.insert((*qual).to_string());
        }
    }
    let ts_member_dirs = ts_js_cycle_member_dirs(&member_dirs, all_module_dirs, files);

    // node_id -> the index of the (single) cycle it belongs to (SCCs partition the nodes).
    let mut owner: HashMap<&str, usize> = HashMap::new();
    for (i, members) in cycle_members.iter().enumerate() {
        for (id, _) in members {
            owner.insert(id, i);
        }
    }

    // Collect each cycle's intra-SCC edge dispositions (the FULL set — no render cap; the verdict
    // must see every edge to claim "ALL are type-only").
    let mut dispositions: Vec<Vec<Option<EdgeTypeOnly>>> = vec![Vec::new(); cycle_members.len()];
    for (from, to, disp) in module_edges {
        if from == to {
            continue;
        }
        if let (Some(&fi), Some(&ti)) = (owner.get(*from), owner.get(*to)) {
            if fi == ti {
                dispositions[fi].push(*disp);
            }
        }
    }

    cycle_members
        .iter()
        .zip(dispositions.iter())
        .map(|(members, disp)| {
            // §5 gate: only TS/JS-member cycles carry a verdict.
            let is_ts = members.iter().any(|(_, q)| ts_member_dirs.contains(*q));
            if is_ts {
                Some(classify_cycle_type_only(disp))
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::EdgeTypeOnly::*;
    use super::*;

    fn ts(members: &[&str]) -> BTreeSet<String> {
        members.iter().map(|s| s.to_string()).collect()
    }

    // ── verdict precedence (the honesty-critical kernel) ──────────────────────

    #[test]
    fn pure_type_only_is_type_only() {
        assert_eq!(
            classify_cycle_type_only(&[Some(TypeOnly), Some(TypeOnly)]),
            CycleTypeOnly::TypeOnly
        );
    }

    #[test]
    fn one_runtime_edge_dominates() {
        assert_eq!(
            classify_cycle_type_only(&[Some(TypeOnly), Some(Runtime), Some(Unreadable), None]),
            CycleTypeOnly::HasRuntimeEdges
        );
    }

    #[test]
    fn unreadable_is_its_own_reason_and_outranks_absent() {
        // Operator ruling item 2a: a CORRUPT contributor surfaces its OWN reason, never collapsed
        // into the pre-tracking one, and outranks a merely-absent edge.
        assert_eq!(
            classify_cycle_type_only(&[Some(TypeOnly), Some(Unreadable), None]),
            CycleTypeOnly::Unknown {
                reason: "type-only fact unreadable".to_string()
            }
        );
    }

    #[test]
    fn absent_without_runtime_is_pre_tracking_unknown() {
        assert_eq!(
            classify_cycle_type_only(&[Some(TypeOnly), None]),
            CycleTypeOnly::Unknown {
                reason: "indexed before type-only tracking".to_string()
            }
        );
    }

    #[test]
    fn empty_bucket_is_unavailable_unknown() {
        assert_eq!(
            classify_cycle_type_only(&[]),
            CycleTypeOnly::Unknown {
                reason: "cycle import edges unavailable".to_string()
            }
        );
    }

    #[test]
    fn verdict_serializes_to_the_cycles_json_shape() {
        // The JSON the `cycles` renderer + the `orient` leaf consume — must be exactly this shape.
        assert_eq!(
            serde_json::to_value(CycleTypeOnly::TypeOnly).unwrap(),
            serde_json::json!({ "kind": "type_only" })
        );
        assert_eq!(
            serde_json::to_value(CycleTypeOnly::HasRuntimeEdges).unwrap(),
            serde_json::json!({ "kind": "has_runtime_edges" })
        );
        assert_eq!(
            serde_json::to_value(CycleTypeOnly::Unknown {
                reason: "cycle import edges unavailable".to_string()
            })
            .unwrap(),
            serde_json::json!({ "kind": "unknown", "reason": "cycle import edges unavailable" })
        );
    }

    // ── §5 TS/JS membership ───────────────────────────────────────────────────

    #[test]
    fn dominant_rust_with_one_ts_cycle_yields_ts_members() {
        let members = ts(&["tools/rgistr/src", "tools/rgistr/lib"]);
        let all_modules = vec![
            "src".to_string(),
            "tools/rgistr/src".to_string(),
            "tools/rgistr/lib".to_string(),
        ];
        let files = vec![
            ("src/main.rs", Some("rust")),
            ("tools/rgistr/src/index.ts", Some("typescript")),
            ("tools/rgistr/lib/util.ts", Some("typescript")),
        ];
        assert_eq!(
            ts_js_cycle_member_dirs(&members, &all_modules, &files),
            ts(&["tools/rgistr/src", "tools/rgistr/lib"])
        );
    }

    #[test]
    fn nested_non_cycle_ts_module_does_not_leak_to_ancestor() {
        let members = ts(&["src", "src/core"]);
        let all_modules = vec![
            "src".to_string(),
            "src/core".to_string(),
            "src/web".to_string(),
        ];
        let files = vec![
            ("src/main.rs", Some("rust")),
            ("src/web/app.ts", Some("typescript")),
        ];
        assert!(ts_js_cycle_member_dirs(&members, &all_modules, &files).is_empty());
    }

    #[test]
    fn js_family_tokens_all_count_others_do_not() {
        let members = ts(&["pkg"]);
        let all = vec!["pkg".to_string()];
        for tok in ["typescript", "tsx", "javascript", "jsx"] {
            assert!(!ts_js_cycle_member_dirs(&members, &all, &[("pkg/f", Some(tok))]).is_empty());
        }
        assert!(ts_js_cycle_member_dirs(
            &members,
            &all,
            &[("pkg/f", Some("python")), ("pkg/g", None)]
        )
        .is_empty());
    }

    // ── the whole-set orchestrator (the ONE derivation both routes call) ──────

    fn members<'a>(pairs: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
        pairs.to_vec()
    }

    #[test]
    fn orchestrator_type_only_cycle() {
        let cyc = vec![members(&[("a", "pkg/a"), ("b", "pkg/b")])];
        let edges = vec![("a", "b", Some(TypeOnly)), ("b", "a", Some(TypeOnly))];
        let files = vec![
            ("pkg/a/x.ts", Some("typescript")),
            ("pkg/b/y.ts", Some("tsx")),
        ];
        let all = vec!["pkg/a".to_string(), "pkg/b".to_string()];
        assert_eq!(
            classify_cycles_type_only(&cyc, &edges, &files, &all),
            vec![Some(CycleTypeOnly::TypeOnly)]
        );
    }

    #[test]
    fn orchestrator_runtime_cycle() {
        let cyc = vec![members(&[("a", "pkg/a"), ("b", "pkg/b")])];
        let edges = vec![("a", "b", Some(TypeOnly)), ("b", "a", Some(Runtime))];
        let files = vec![("pkg/a/x.ts", Some("typescript"))];
        let all = vec!["pkg/a".to_string(), "pkg/b".to_string()];
        assert_eq!(
            classify_cycles_type_only(&cyc, &edges, &files, &all),
            vec![Some(CycleTypeOnly::HasRuntimeEdges)]
        );
    }

    #[test]
    fn orchestrator_non_ts_cycle_gets_no_verdict() {
        // §5: a cycle with no TS/JS member carries NO verdict (byte-stable), even all-runtime.
        let cyc = vec![members(&[("a", "crates/a"), ("b", "crates/b")])];
        let edges = vec![("a", "b", Some(Runtime)), ("b", "a", Some(Runtime))];
        let files = vec![("crates/a/x.rs", Some("rust"))];
        let all = vec!["crates/a".to_string(), "crates/b".to_string()];
        assert_eq!(
            classify_cycles_type_only(&cyc, &edges, &files, &all),
            vec![None]
        );
    }

    #[test]
    fn orchestrator_absent_fact_is_unknown_not_runtime() {
        // A TS cycle whose contributing edges predate the fact ⇒ Unknown (never a silent runtime).
        let cyc = vec![members(&[("a", "pkg/a"), ("b", "pkg/b")])];
        let edges = vec![("a", "b", None), ("b", "a", None)];
        let files = vec![("pkg/a/x.ts", Some("typescript"))];
        let all = vec!["pkg/a".to_string(), "pkg/b".to_string()];
        assert_eq!(
            classify_cycles_type_only(&cyc, &edges, &files, &all),
            vec![Some(CycleTypeOnly::Unknown {
                reason: "indexed before type-only tracking".to_string()
            })]
        );
    }
}
