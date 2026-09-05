//! Ranking (spec §7.2) + the is_test partition (spec §5, the moat). Ranking is a
//! **pure function of the stored `f32` vectors + the query vector** — exactly
//! reproducible within one snapshot's vectors, with no order jitter.
//!
//! SEED-CHUNK-1: candidates are per-SYMBOL chunks keyed to node identity, and the
//! rendered order places **production-classified chunks above test-classified**
//! ones (each block ordered by score, then path), test seeds labeled downstream.
//! Freshness partitioning is gone: vectors are per-snapshot, so a served snapshot's
//! rows are current by construction (a superseded snapshot is simply never served).

use crate::ports::SeedVectorEntry;

/// Near-tie advisory threshold (spec §7.3): two adjacent candidates whose scores
/// differ by ≤ ε are flagged so a consumer knows the order between them carries no
/// information. Advisory only — never a cross-machine stability claim.
pub const NEAR_TIE_EPSILON: f32 = 1e-5;

/// L2-normalize in place (`v / (‖v‖ + 1e-9)`). Both stored and query vectors are
/// normalized, so cosine reduces to a dot product.
pub fn l2_normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    let denom = norm + 1e-9;
    for x in v.iter_mut() {
        *x /= denom;
    }
}

/// A ranked semantic candidate (Layer-3 hint), carrying the anchor material.
#[derive(Debug, Clone, PartialEq)]
pub struct RankedCandidate {
    pub node_uid: String,
    pub stable_key: String,
    pub file_uid: String,
    pub path: String,
    pub line: Option<i64>,
    pub qualified_name: Option<String>,
    /// `true` ⇒ this candidate is in the DEMOTED test block (labeled downstream).
    pub is_test: bool,
    /// SEED-CHUNK-2 (spec §2.2): `true` ⇒ a declaration without a body (labeled
    /// `(decl)` downstream), ranked below any body-bearing chunk of the same
    /// qualified name.
    pub is_decl: bool,
    pub score: f32,
    /// Within [`NEAR_TIE_EPSILON`] of the next-lower candidate in the returned
    /// order — advisory only (spec §7.3).
    pub near_tie: bool,
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// The highest cosine score among `entries`, or `None` when there are no entries.
/// Used for the honest "nothing above the floor (best: X)" line when the ranked set
/// is empty because everything scored ≤ 0 (spec §4 floor honesty).
pub fn best_score(query_vec: &[f32], entries: &[&SeedVectorEntry]) -> Option<f32> {
    entries
        .iter()
        .map(|e| dot(query_vec, &e.vector))
        .max_by(|a, b| a.total_cmp(b))
}

/// Rank chunk vectors against a (normalized) query vector by cosine, keeping the
/// top `top_n` **with production ranked above test** (spec §5). Within each block
/// the order is `(-score, path, node_uid)`. Only candidates scoring strictly above
/// zero are returned — an all-≤0 result is the honest "nothing scored above zero"
/// state (spec §8.3), distinct from an empty/absent store.
///
/// The top_n cap applies to the COMBINED list after the production→test ordering,
/// so a query with many strong test hits and few production hits still surfaces the
/// production hits first (the moat), then fills with test hits up to the cap.
pub fn rank(query_vec: &[f32], entries: &[&SeedVectorEntry], top_n: usize) -> Vec<RankedCandidate> {
    // Score everything above the zero floor. No dimension filter: the storage read
    // validated `dim` uniformity, and the query is embedded by the same pinned model
    // — a silent length filter would mask a defect by serving a partial subset.
    let mut scored: Vec<(f32, &SeedVectorEntry)> = entries
        .iter()
        .inspect(|e| {
            debug_assert_eq!(
                e.vector.len(),
                query_vec.len(),
                "unvalidated store reached rank()"
            )
        })
        .map(|e| (dot(query_vec, &e.vector), *e))
        .filter(|(s, _)| *s > 0.0)
        .collect();

    // SEED-CHUNK-2 (spec §2.2) declaration demotion: a declaration ranks BELOW *any*
    // body-bearing chunk of the SAME qualified name, regardless of raw cosine (the
    // measured leveldb case — a 0.45 decl must sink below its 0.30 impl). We give each
    // decl the score of the WORST-scoring impl of its qualified name (`impl_min_score`),
    // then break the resulting tie with `is_decl` (impl before decl): the decl lands just
    // UNDER the lowest-scoring impl, so it is below every impl even when impls straddle
    // the decl's own score (impls 0.80 & 0.20, decl 0.90 ⇒ 0.80, 0.20, decl). A decl with
    // no matching impl in the scored set keeps its own score (it may be the only hit —
    // still labeled, never suppressed). Computed over the whole scored set BEFORE the
    // top_n cut so the pairing survives truncation. `None` qualified_name never pairs.
    //
    // PRECEDENCE (ratified 2026-09-05, review-2 item 1): the production/test PARTITION is
    // applied FIRST; decl-below-impl holds only WITHIN a partition. So the pairing key is
    // `(is_test, qualified_name)`, NOT the name alone — a TEST implementation must never
    // lower the effective score of a PRODUCTION declaration of the same name (that would
    // let a test hit reorder production-internal ranking across the partition boundary).
    let mut impl_min_score: std::collections::HashMap<(bool, &str), f32> =
        std::collections::HashMap::new();
    for (s, e) in &scored {
        if e.is_decl {
            continue;
        }
        if let Some(q) = e.qualified_name.as_deref() {
            impl_min_score
                .entry((e.is_test, q))
                .and_modify(|cur| {
                    if *s < *cur {
                        *cur = *s;
                    }
                })
                .or_insert(*s);
        }
    }
    // The effective sort score: a decl inherits its WORST same-partition impl's score when
    // one exists, so the is_decl tie-break places it below every impl of the same qualified
    // name IN ITS OWN PARTITION (production decls pair only with production impls).
    let effective = |s: f32, e: &SeedVectorEntry| -> f32 {
        if e.is_decl {
            if let Some(q) = e.qualified_name.as_deref() {
                if let Some(impl_s) = impl_min_score.get(&(e.is_test, q)) {
                    return *impl_s;
                }
            }
        }
        s
    };

    // Production (is_test=false) sorts before test (is_test=true); within a block,
    // descending EFFECTIVE score, then impl-before-decl (so a decl sits just under its
    // impl), then path, then node_uid — a deterministic total order.
    scored.sort_by(|a, b| {
        let ea = effective(a.0, a.1);
        let eb = effective(b.0, b.1);
        a.1.is_test
            .cmp(&b.1.is_test)
            .then_with(|| eb.total_cmp(&ea))
            .then_with(|| a.1.is_decl.cmp(&b.1.is_decl))
            .then_with(|| a.1.path.cmp(&b.1.path))
            .then_with(|| a.1.node_uid.cmp(&b.1.node_uid))
    });
    scored.truncate(top_n);

    let mut out: Vec<RankedCandidate> = scored
        .into_iter()
        .map(|(score, e)| RankedCandidate {
            node_uid: e.node_uid.clone(),
            stable_key: e.stable_key.clone(),
            file_uid: e.file_uid.clone(),
            path: e.path.clone(),
            line: e.line,
            qualified_name: e.qualified_name.clone(),
            is_test: e.is_test,
            is_decl: e.is_decl,
            score,
            near_tie: false,
        })
        .collect();

    // Near-tie flag between adjacent rows ONLY within the same is_test block (a
    // production→test boundary is not a tie even if scores are close — the blocks
    // carry different certainty).
    for i in 0..out.len().saturating_sub(1) {
        if out[i].is_test == out[i + 1].is_test
            && (out[i].score - out[i + 1].score).abs() <= NEAR_TIE_EPSILON
        {
            out[i].near_tie = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(node: &str, path: &str, is_test: bool, v: Vec<f32>) -> SeedVectorEntry {
        SeedVectorEntry {
            node_uid: node.to_string(),
            stable_key: format!("k:{node}"),
            file_uid: format!("fu:{path}"),
            path: path.to_string(),
            line: Some(1),
            qualified_name: Some(node.to_string()),
            is_test,
            is_decl: false,
            content_hash: "h".to_string(),
            vector: v,
        }
    }

    /// An entry with an explicit qualified_name + is_decl, for decl-demotion tests.
    fn named(node: &str, path: &str, qname: &str, is_decl: bool, v: Vec<f32>) -> SeedVectorEntry {
        SeedVectorEntry {
            node_uid: node.to_string(),
            stable_key: format!("k:{node}"),
            file_uid: format!("fu:{path}"),
            path: path.to_string(),
            line: Some(1),
            qualified_name: Some(qname.to_string()),
            is_test: false,
            is_decl,
            content_hash: "h".to_string(),
            vector: v,
        }
    }

    #[test]
    fn impl_outranks_its_own_decl_even_when_decl_scores_higher() {
        // The measured leveldb case: decl (db_impl.h) cosine 0.45 must sink BELOW its
        // impl (db_impl.cc) cosine 0.30 because an impl always outranks its own decl.
        // Cosine against [1,0] is just the first component: decl 0.45, impl 0.30.
        let decl = named(
            "decl",
            "db/db_impl.h",
            "leveldb.DBImpl.Recover",
            true,
            vec![0.45, 0.0],
        );
        let impl_ = named(
            "impl",
            "db/db_impl.cc",
            "leveldb.DBImpl.Recover",
            false,
            vec![0.30, 0.0],
        );
        let refs = vec![&decl, &impl_];
        let ranked = rank(&[1.0, 0.0], &refs, 5);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].node_uid, "impl", "impl first despite lower score");
        assert!(!ranked[0].is_decl);
        assert_eq!(ranked[1].node_uid, "decl");
        assert!(
            ranked[1].is_decl,
            "decl demoted below its impl, still present"
        );
    }

    #[test]
    fn decl_ranks_below_every_impl_of_its_name_even_a_lower_scoring_one() {
        // Reviewer §2.2 case: with two impls (0.80 and 0.20) and a decl scoring 0.90, the
        // decl must land below BOTH impls — not merely below the best. Cosine against
        // [1,0] is the first component.
        let decl = named("decl", "a.h", "N.F", true, vec![0.90, 0.0]);
        let impl_hi = named("hi", "a.cc", "N.F", false, vec![0.80, 0.0]);
        let impl_lo = named("lo", "b.cc", "N.F", false, vec![0.20, 0.0]);
        let refs = vec![&decl, &impl_hi, &impl_lo];
        let ranked = rank(&[1.0, 0.0], &refs, 5);
        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0].node_uid, "hi", "highest impl first");
        assert_eq!(ranked[1].node_uid, "lo", "lower impl still above the decl");
        assert_eq!(
            ranked[2].node_uid, "decl",
            "decl below EVERY impl of its name, despite the highest raw score"
        );
        assert!(ranked[2].is_decl);
    }

    #[test]
    fn production_decl_outranks_a_test_impl_of_the_same_name() {
        // RATIFIED PRECEDENCE (operator 2026-09-05, answering review-2
        // SC2-DECL-TEST-PRECEDENCE): the production/test PARTITION is applied FIRST;
        // decl-below-impl holds only WITHIN a partition. So a PRODUCTION declaration
        // renders ABOVE a TEST-partition implementation of the same qualified name — the
        // test impl is a double, not the answer — while the production decl is still
        // labeled `(decl)`. This pins the crossing-partition case the reviewer flagged:
        // the unqualified "impl always ranks above its own decl" is scoped to a partition,
        // it does NOT override the test moat.
        //
        // The test impl scores HIGHER (0.90) than the production decl (0.40) on purpose:
        // partition dominance must hold even when the test hit is the stronger cosine.
        let prod_decl = SeedVectorEntry {
            node_uid: "prod_decl".to_string(),
            stable_key: "k:prod_decl".to_string(),
            file_uid: "fu:api.h".to_string(),
            path: "src/api.h".to_string(),
            line: Some(1),
            qualified_name: Some("N.F".to_string()),
            is_test: false,
            is_decl: true,
            content_hash: "h".to_string(),
            vector: vec![0.40, 0.0],
        };
        let test_impl = SeedVectorEntry {
            node_uid: "test_impl".to_string(),
            stable_key: "k:test_impl".to_string(),
            file_uid: "fu:api_test.cc".to_string(),
            path: "test/api_test.cc".to_string(),
            line: Some(1),
            qualified_name: Some("N.F".to_string()),
            is_test: true,
            is_decl: false,
            content_hash: "h".to_string(),
            vector: vec![0.90, 0.0],
        };
        let refs = vec![&prod_decl, &test_impl];
        let ranked = rank(&[1.0, 0.0], &refs, 5);
        assert_eq!(ranked.len(), 2);
        assert_eq!(
            ranked[0].node_uid, "prod_decl",
            "the production declaration ranks ABOVE the test implementation (partition first)"
        );
        assert!(
            ranked[0].is_decl && !ranked[0].is_test,
            "the production decl is still labeled (decl), and it is production"
        );
        assert_eq!(
            ranked[1].node_uid, "test_impl",
            "the higher-scoring test impl is demoted below production — a double, not the answer"
        );
        assert!(ranked[1].is_test);
    }

    #[test]
    fn a_test_impl_does_not_reorder_production_internal_ranking() {
        // review-2 item 1: the pairing key is (is_test, qualified_name), so a TEST-partition
        // implementation must NOT lower the effective score of a PRODUCTION declaration of
        // the same name. Set-up: an unrelated production candidate (0.50), a production decl
        // "N.F" (0.40, no production impl of its name in the set), and a higher-scoring TEST
        // impl "N.F" (0.90). Correct: within production, the unrelated 0.50 ranks ABOVE the
        // 0.40 decl (the decl keeps its own score — no PRODUCTION impl to sink under); the
        // test impl lands in the demoted test partition last. If the key were name-only, the
        // test impl's 0.90 would become the decl's effective score and wrongly hoist the
        // production decl above the unrelated production candidate.
        let unrelated = named("u", "src/other.cc", "N.G", false, vec![0.50, 0.0]);
        let prod_decl = named("d", "src/api.h", "N.F", true, vec![0.40, 0.0]);
        let test_impl = SeedVectorEntry {
            node_uid: "ti".to_string(),
            stable_key: "k:ti".to_string(),
            file_uid: "fu:t".to_string(),
            path: "test/api_test.cc".to_string(),
            line: Some(1),
            qualified_name: Some("N.F".to_string()),
            is_test: true,
            is_decl: false,
            content_hash: "h".to_string(),
            vector: vec![0.90, 0.0],
        };
        let refs = vec![&unrelated, &prod_decl, &test_impl];
        let ranked = rank(&[1.0, 0.0], &refs, 5);
        assert_eq!(ranked.len(), 3);
        assert_eq!(
            ranked[0].node_uid, "u",
            "unrelated production candidate (0.50) ranks above the production decl (0.40)"
        );
        assert_eq!(
            ranked[1].node_uid, "d",
            "the production decl keeps its OWN score — the test impl did not hoist it"
        );
        assert!(!ranked[1].is_test && ranked[1].is_decl);
        assert_eq!(
            ranked[2].node_uid, "ti",
            "the test impl is demoted to the test partition, last"
        );
        assert!(ranked[2].is_test);
    }

    #[test]
    fn a_decl_with_no_matching_impl_keeps_its_own_score_and_is_labeled() {
        // Only a decl is present (no body-bearing sibling) → it still appears, ranked by
        // its own score, labeled `(decl)` downstream.
        let decl = named("d", "a.h", "Foo.bar", true, vec![0.6, 0.0]);
        let other = named("o", "b.cc", "Baz.qux", false, vec![0.5, 0.0]);
        let refs = vec![&decl, &other];
        let ranked = rank(&[1.0, 0.0], &refs, 5);
        assert_eq!(
            ranked[0].node_uid, "d",
            "decl keeps its higher own score, no impl to sink under"
        );
        assert!(ranked[0].is_decl);
        assert_eq!(ranked[1].node_uid, "o");
    }

    #[test]
    fn production_ranks_above_test_even_when_test_scores_higher() {
        // A test chunk with a PERFECT match and a production chunk with a weaker
        // match: production still comes first (the moat).
        let prod = entry("p", "src/db.cc", false, vec![0.8, 0.6]); // cos with [1,0] = 0.8
        let test = entry("t", "test/db_test.cc", true, vec![1.0, 0.0]); // cos = 1.0
        let refs = vec![&prod, &test];
        let ranked = rank(&[1.0, 0.0], &refs, 5);
        assert_eq!(ranked.len(), 2);
        assert_eq!(
            ranked[0].node_uid, "p",
            "production first despite lower score"
        );
        assert!(!ranked[0].is_test);
        assert_eq!(ranked[1].node_uid, "t");
        assert!(ranked[1].is_test);
    }

    #[test]
    fn within_block_orders_by_score_then_path() {
        let a = entry("a", "b.rs", false, vec![1.0, 0.0]); // 1.0
        let b = entry("b", "a.rs", false, vec![1.0, 0.0]); // 1.0 tie → path a.rs first
        let c = entry("c", "c.rs", false, vec![0.5, 0.5]); // lower
        let refs = vec![&a, &b, &c];
        let ranked = rank(&[1.0, 0.0], &refs, 5);
        assert_eq!(ranked[0].path, "a.rs");
        assert_eq!(ranked[1].path, "b.rs");
        assert_eq!(ranked[2].path, "c.rs");
        assert!(ranked[0].near_tie); // 1.0 vs 1.0
    }

    #[test]
    fn nothing_above_zero_is_empty_but_best_score_is_visible() {
        let e = entry("f", "a", false, vec![-1.0, 0.0]);
        let refs = vec![&e];
        assert!(rank(&[1.0, 0.0], &refs, 5).is_empty());
        // The honest sub-floor best is still computable for the "(best: X)" line.
        assert_eq!(best_score(&[1.0, 0.0], &refs), Some(-1.0));
    }

    #[test]
    fn top_n_caps_the_combined_list() {
        let mut es = Vec::new();
        for i in 0..8 {
            es.push(entry(
                &format!("p{i}"),
                &format!("p{i:02}"),
                false,
                vec![1.0, 0.0],
            ));
        }
        for i in 0..8 {
            es.push(entry(
                &format!("t{i}"),
                &format!("t{i:02}"),
                true,
                vec![1.0, 0.0],
            ));
        }
        let refs: Vec<&SeedVectorEntry> = es.iter().collect();
        let ranked = rank(&[1.0, 0.0], &refs, 10);
        assert_eq!(ranked.len(), 10);
        // First 8 are production (all production ranked above test), then 2 test.
        assert_eq!(ranked.iter().filter(|c| !c.is_test).count(), 8);
        assert_eq!(ranked.iter().filter(|c| c.is_test).count(), 2);
    }
}
