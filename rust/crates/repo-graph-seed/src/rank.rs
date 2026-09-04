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

    // Production (is_test=false) sorts before test (is_test=true); within a block,
    // descending score, then path, then node_uid — a deterministic total order.
    scored.sort_by(|a, b| {
        a.1.is_test
            .cmp(&b.1.is_test)
            .then_with(|| b.0.total_cmp(&a.0))
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
            content_hash: "h".to_string(),
            vector: v,
        }
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
