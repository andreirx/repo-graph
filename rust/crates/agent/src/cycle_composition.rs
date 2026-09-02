//! ORIENT-CYCLES-DISAGREE-1 — the ONE cycle test-composition classifier.
//!
//! # Why this module exists
//!
//! Two surfaces render a module-cycle headline: `cycles` ("N module-level cycles found (+M
//! test-only excluded)") and `orient`'s cycle leaf. Before this slice each derived its own
//! count — `cycles` split test-only cycles out (daemon-side, from the stored `is_test` fact)
//! while `orient` reported the RAW total — so the two disagreed for one snapshot
//! (repo-graph self-index: `cycles` = "1 (+1 test-only)", `orient` = "2 import cycles").
//!
//! The FIXTURE-POLLUTION-1 test-only partition previously lived ONLY in `daemon-runtime`
//! (`cycle_output::composition`), unreachable from the `storage` adapter that serves
//! `orient`'s cycles (`daemon-runtime` depends on `storage`, never the reverse). This module
//! hoists the PURE classification into the port-owner `agent` crate, which BOTH `storage`
//! (adapter → `orient`) and `daemon-runtime` (`cycle_output` → `cycles`) already depend on.
//! One classifier, called from both serving computations ⇒ the two headlines cannot drift.
//!
//! Abstraction record — module: `cycle_composition`; concrete current users: the `storage`
//! adapter's `find_module_cycles` (labels `orient`'s cycles) AND `daemon-runtime`'s
//! `cycle_output::composition::label_test_only_cycles` (labels `cycles`); axis: the ONE
//! test-only/production/unknown classification of a module cycle from the stored `is_test`
//! fact, shared so the two headlines derive from a single function; rejected simpler
//! alternative: leaving the classifier in `daemon-runtime` and duplicating it in the
//! `storage` adapter — two copies of honesty-critical logic that could drift back into the
//! exact disagreement this slice removes.
//!
//! # Growth axis
//!
//! Variants FIXED (a cycle is positively test-only, positively production, or unprovable —
//! exactly three certainty states), operations GROWING (JSON emit in `daemon-runtime`;
//! headline counting in the `agent` aggregator; two renderers). Fixed variants + growing
//! operations ⇒ sum type + exhaustive match. A fourth state deliberately breaks every match.
//!
//! The classification basis is the stored `is_test` fact ONLY — NEVER a path/name heuristic
//! (STANDING HONESTY RULE #2 / FIXTURE-POLLUTION-1 binding direction rule). An unprovable
//! composition is `Unknown(reason)`, NEVER collapsed to production.

/// The test-composition of a MODULE, under the conservative aggregation
/// (CONTRADICTION-SWEEP-1 §2.3): the module owns the tracked files under its directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModuleComposition {
    /// Owns ≥1 file and EVERY owned file carries the stored `is_test` fact.
    TestOnly,
    /// Owns ≥1 production (non-test) file.
    Production,
    /// Owns NO tracked file — no reachable `is_test` evidence for this module.
    Unknown,
}

/// The test-composition of a module CYCLE — the shared classification RESULT the
/// serving computations attach to their cycle DTOs. Three mutually-exclusive certainty
/// states ⇒ a sum type; `Unknown` carries a reader-framed reason and is NEVER demoted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CycleTestComposition {
    /// POSITIVE evidence: every member module is wholly test-owned. The ONLY state that is
    /// excluded from the production headline (demoted).
    TestOnly,
    /// POSITIVE evidence: ≥1 member module owns a production (non-test) file. A real cycle.
    Production,
    /// No reachable `is_test` evidence for ≥1 member (owns no tracked file / a malformed
    /// member) and no positive production member. NEVER demoted — counted in the headline
    /// carrying this reason.
    Unknown(String),
}

/// FIXTURE-POLLUTION-1 §2.1/§2.3 + binding direction rule: classify a MODULE (whose canonical
/// path is the directory `qualified`) from the stored `is_test` fact ONLY — never the path
/// string. Ownership is by directory containment (the module IS the directory
/// `packages/a/src`). ANY production owned file ⇒ `Production`; ≥1 owned file all-test ⇒
/// `TestOnly`; NO owned file ⇒ `Unknown` (distinct from production).
fn classify_module(qualified: &str, files: &[(&str, bool)]) -> ModuleComposition {
    let prefix = format!("{qualified}/");
    let mut owned_any = false;
    for (path, is_test) in files {
        let owned = *path == qualified || path.starts_with(&prefix);
        if owned {
            owned_any = true;
            if !*is_test {
                return ModuleComposition::Production; // a production owned file ⇒ production
            }
        }
    }
    if owned_any {
        ModuleComposition::TestOnly
    } else {
        ModuleComposition::Unknown
    }
}

/// FIXTURE-POLLUTION-1 §2.2/§2.3 + binding direction rule: classify EACH cycle in a set from
/// its member modules' qualified paths (a `None` member = a malformed/unaddressable node) and
/// the repo's tracked `(path, is_test)` rows. The per-distinct-module classification is memoized
/// across the whole set (cost O(distinct members × files)); the memo is internal so callers do
/// not touch the crate-private [`ModuleComposition`]. Returns one [`CycleTestComposition`] per
/// input cycle, in order.
pub fn classify_cycles(
    cycles: &[Vec<Option<&str>>],
    files: &[(&str, bool)],
) -> Vec<CycleTestComposition> {
    let mut memo: std::collections::HashMap<String, ModuleComposition> =
        std::collections::HashMap::new();
    cycles
        .iter()
        .map(|members| classify_one(members, files, &mut memo))
        .collect()
}

/// Conservative aggregation over one cycle's members:
/// - ANY member `Production` ⇒ `Production` (a real cycle);
/// - else ANY member `Unknown` (owns no tracked file, or a `None` member) ⇒ `Unknown` WITH a
///   reason — NOT demoted (we cannot prove it wholly test-only);
/// - else (every member `TestOnly`) ⇒ `TestOnly` (demoted).
///
/// An EMPTY / all-absent member list is `Unknown` with a reason — NEVER a silent production
/// default.
fn classify_one(
    member_quals: &[Option<&str>],
    files: &[(&str, bool)],
    memo: &mut std::collections::HashMap<String, ModuleComposition>,
) -> CycleTestComposition {
    if member_quals.is_empty() {
        return CycleTestComposition::Unknown("cycle has no members".to_string());
    }
    let mut any_production = false;
    let mut unknown_reason: Option<String> = None;
    for member in member_quals {
        match member {
            None => {
                unknown_reason.get_or_insert_with(|| {
                    "a cycle member has no qualified module path".to_string()
                });
            }
            Some(q) => {
                let comp = *memo
                    .entry((*q).to_string())
                    .or_insert_with(|| classify_module(q, files));
                match comp {
                    ModuleComposition::Production => any_production = true,
                    ModuleComposition::Unknown => {
                        unknown_reason.get_or_insert_with(|| {
                            format!("member module `{q}` owns no tracked file (is_test unknown)")
                        });
                    }
                    ModuleComposition::TestOnly => {}
                }
            }
        }
    }
    if any_production {
        CycleTestComposition::Production
    } else if let Some(reason) = unknown_reason {
        CycleTestComposition::Unknown(reason)
    } else {
        CycleTestComposition::TestOnly
    }
}

/// The THREE headline integers BOTH surfaces render for one classified cycle set — the single
/// partition (operator ruling review-3, 2026-09-03 #2). Named (not a bare tuple) because the
/// counts are honesty-critical and position-swappable: mislabeling `unknown` as `test_only`
/// would DEMOTE an unprovable cycle, mislabeling it as pure production would HIDE it.
///
/// - `production_count` — the NOT-EXCLUDED headline figure: `Production` + `Unknown` cycles, the
///   SAME integer `cycles` renders as "N module-level cycle(s) found" (its `main` listing keeps
///   `Unknown` cycles, never demoting them).
/// - `test_only_count` — positively-test-only cycles EXCLUDED from the headline.
/// - `unknown_count` — the SUBSET of `production_count` whose composition is unprovable, carried
///   so an `Unknown` cycle is never counted INVISIBLY inside the production figure (never demoted,
///   per the direction rule; never hidden, per the visibility half — operator ruling #2).
///
/// `production_count` INCLUDES `unknown_count` by design: unknown "stays in the production figure".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CyclePartition {
    pub production_count: u64,
    pub test_only_count: u64,
    pub unknown_count: u64,
}

/// Partition a cycle-set's classifications into the [`CyclePartition`] both headlines render.
/// `TestOnly` ⇒ excluded (`test_only_count`); `Production` ⇒ headline only; `Unknown` ⇒ headline
/// AND `unknown_count` (counted in `production_count`, disclosed separately). One classification,
/// one partition, two renderers of the same three integers.
pub fn partition_counts<'a>(
    comps: impl IntoIterator<Item = &'a CycleTestComposition>,
) -> CyclePartition {
    let mut production = 0u64;
    let mut test_only = 0u64;
    let mut unknown = 0u64;
    for c in comps {
        match c {
            CycleTestComposition::TestOnly => test_only += 1,
            CycleTestComposition::Production => production += 1,
            CycleTestComposition::Unknown(_) => {
                production += 1;
                unknown += 1;
            }
        }
    }
    CyclePartition {
        production_count: production,
        test_only_count: test_only,
        unknown_count: unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify(members: &[Option<&str>], files: &[(&str, bool)]) -> CycleTestComposition {
        classify_cycles(&[members.to_vec()], files)
            .into_iter()
            .next()
            .expect("one cycle in, one out")
    }

    #[test]
    fn module_conservative_aggregation() {
        let files = [
            ("tests/fixtures/pkg/a.rs", true),
            ("tests/fixtures/pkg/b.rs", true),
            ("src/core/x.rs", false),
            ("src/core/y_test.rs", true), // a lone test file in a production module
        ];
        assert_eq!(
            classify_module("tests/fixtures/pkg", &files),
            ModuleComposition::TestOnly
        );
        assert_eq!(
            classify_module("src/core", &files),
            ModuleComposition::Production
        );
        assert_eq!(
            classify_module("vendor/lib", &files),
            ModuleComposition::Unknown
        );
    }

    #[test]
    fn cycle_tristate() {
        let files = [
            ("tests/fixtures/pkg-a/mod.rs", true),
            ("tests/fixtures/pkg-b/mod.rs", true),
            ("src/core/x.rs", false),
        ];
        // wholly-test ⇒ TestOnly
        assert_eq!(
            classify(
                &[Some("tests/fixtures/pkg-a"), Some("tests/fixtures/pkg-b")],
                &files
            ),
            CycleTestComposition::TestOnly
        );
        // touches production ⇒ Production
        assert_eq!(
            classify(&[Some("src/core"), Some("tests/fixtures/pkg-a")], &files),
            CycleTestComposition::Production
        );
        // an untracked member ⇒ Unknown WITH reason (never demoted, never production)
        match classify(
            &[Some("tests/fixtures/pkg-a"), Some("vendor/untracked")],
            &files,
        ) {
            CycleTestComposition::Unknown(r) => assert!(r.contains("vendor/untracked"), "{r}"),
            other => panic!("expected Unknown, got {other:?}"),
        }
        // a malformed (None) member ⇒ Unknown, never a silent production default
        match classify(&[None], &files) {
            CycleTestComposition::Unknown(r) => assert!(r.contains("no qualified"), "{r}"),
            other => panic!("expected Unknown, got {other:?}"),
        }
        // empty ⇒ Unknown
        assert!(matches!(
            classify(&[], &files),
            CycleTestComposition::Unknown(_)
        ));
    }

    #[test]
    fn partition_counts_headline_excludes_only_test_only() {
        // production + unknown count toward the headline; only test_only is excluded; the unknown
        // subset is disclosed separately (operator ruling #2: never invisible).
        let comps = vec![
            CycleTestComposition::Production,
            CycleTestComposition::TestOnly,
            CycleTestComposition::Unknown("x".to_string()),
        ];
        assert_eq!(
            partition_counts(&comps),
            CyclePartition {
                production_count: 2,
                test_only_count: 1,
                unknown_count: 1,
            }
        );
    }

    #[test]
    fn headline_partition_equals_renderer_partition_seam() {
        // ORIENT-CYCLES-DISAGREE-1 SEAM: `orient`'s headline count (`partition_counts`) and the
        // `cycles` renderer's split (`main` = not-test-only, `fixtures` = test-only) MUST be the
        // SAME two integers for the SAME classified set. Both derive from THIS module's
        // `CycleTestComposition`, so a divergence would require the two rules to disagree — this
        // test makes that disagreement observable (it is otherwise unrepresentable, since there
        // is exactly one classifier). If a future edit skews either rule, this fails.
        let files = [
            ("src/core/x.rs", false),
            ("tests/fx/a/mod.rs", true),
            ("tests/fx/b/mod.rs", true),
        ];
        let cycles = vec![
            vec![Some("src/core"), Some("src/graph")],    // production
            vec![Some("tests/fx/a"), Some("tests/fx/b")], // test-only
            vec![Some("vendor/x")],                       // unknown (untracked)
        ];
        let comps = classify_cycles(&cycles, &files);

        // orient's derivation.
        let part = partition_counts(&comps);

        // the cycles renderer's derivation (rgr `presentation::cycles`): fixtures are exactly the
        // `TestOnly` cycles; `main` (the "N module-level cycles found" headline) is the rest; the
        // unknown DISCLOSURE is the `Unknown` subset of `main` the renderer marks per-cycle.
        let fixtures = comps
            .iter()
            .filter(|c| matches!(c, CycleTestComposition::TestOnly))
            .count() as u64;
        let main = comps.len() as u64 - fixtures;
        let unknown = comps
            .iter()
            .filter(|c| matches!(c, CycleTestComposition::Unknown(_)))
            .count() as u64;

        assert_eq!(
            part,
            CyclePartition {
                production_count: main,
                test_only_count: fixtures,
                unknown_count: unknown,
            },
            "orient headline split must equal the cycles renderer split for one classified set"
        );
        assert_eq!(
            part,
            CyclePartition {
                production_count: 2,
                test_only_count: 1,
                unknown_count: 1,
            }
        );
    }
}
