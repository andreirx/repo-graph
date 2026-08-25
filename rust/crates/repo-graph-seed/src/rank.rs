//! Ranking (spec §7.2) and freshness partitioning (I3). Ranking is a **pure
//! function of the stored `f32` vectors + the query vector** — exactly
//! reproducible within one store (spec §7.3(1)), with no order jitter
//! (architecture Rule 5).

use std::collections::HashMap;

use crate::store::{SeedVectorBodyV1, SeedVectorEntryV1};

/// Near-tie advisory threshold (spec §7.3): two candidates whose scores differ
/// by ≤ ε are flagged so a consumer knows the order between them carries no
/// information. It **guarantees nothing** about candidates farther apart, and
/// nothing across machines — it is advisory only, never a stability claim.
pub const NEAR_TIE_EPSILON: f32 = 1e-5;

/// L2-normalize in place (`v / (‖v‖ + 1e-9)`, the spike's exact form,
/// `spike.py:104`). Both stored and query vectors are normalized, so cosine
/// reduces to a dot product.
pub fn l2_normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    let denom = norm + 1e-9;
    for x in v.iter_mut() {
        *x /= denom;
    }
}

/// A ranked semantic candidate (Layer-3 hint).
#[derive(Debug, Clone, PartialEq)]
pub struct RankedCandidate {
    pub file_uid: String,
    pub path: String,
    pub content_hash: String,
    pub score: f32,
    /// This candidate is within [`NEAR_TIE_EPSILON`] of the next-lower candidate
    /// in the returned order — advisory only (spec §7.3).
    pub near_tie: bool,
}

/// The result of splitting a loaded store against the current corpus (I3): only
/// fresh entries are rankable; stale entries are excluded and counted for the
/// doctor staleness line (spec §9).
#[derive(Debug, Clone)]
pub struct FreshnessPartition<'a> {
    pub fresh: Vec<&'a SeedVectorEntryV1>,
    /// Entries whose file is still in the corpus but whose `content_hash`
    /// changed since embed (the honest "N of M changed" numerator).
    pub stale_count: usize,
    /// Total entries in the store (the "of M" denominator).
    pub total: usize,
}

/// Partition a loaded store by freshness against the current READY corpus.
///
/// `current` maps `file_uid → current file_versions.content_hash`. An entry is:
/// - **fresh** iff `current[file_uid] == entry.content_hash`;
/// - **stale** iff the file is still present but the hash differs (counted);
/// - **dropped** (not fresh, not counted) iff the file is no longer in the
///   corpus (removed / newly test/generated/excluded) — it simply cannot be a
///   candidate anymore.
pub fn partition_fresh<'a>(
    body: &'a SeedVectorBodyV1,
    current: &HashMap<String, String>,
) -> FreshnessPartition<'a> {
    let mut fresh = Vec::new();
    let mut stale_count = 0usize;
    for entry in &body.entries {
        match current.get(&entry.file_uid) {
            Some(h) if *h == entry.content_hash => fresh.push(entry),
            Some(_) => stale_count += 1,
            None => { /* file gone from corpus → drop, do not count as stale */ }
        }
    }
    FreshnessPartition {
        fresh,
        stale_count,
        total: body.entries.len(),
    }
}

/// Rank fresh entries against a (normalized) query vector by cosine, keep the
/// top `top_n` under the `(-score, path)` order (spec §7.2). Only candidates
/// scoring strictly above zero are returned — an all-≤0 result is the honest
/// "nothing scored above zero" known-zero state (spec §8.3), distinct from an
/// empty/absent store.
pub fn rank(
    query_vec: &[f32],
    entries: &[&SeedVectorEntryV1],
    top_n: usize,
) -> Vec<RankedCandidate> {
    // No dimension filter here: the store is validated on load (store::decode ⇒
    // validate_entries) so every entry already matches the pinned `dim`, and the
    // query is embedded by that same pinned model — so `query_vec` and every
    // `e.vector` share a length. A silent `.filter(len == …)` would mask a
    // dimension defect by serving a partial subset (review-2 #6); the store-level
    // reject is the honest gate instead. `debug_assert` catches a contract breach
    // in tests without a release-mode silent drop.
    let mut scored: Vec<(f32, &SeedVectorEntryV1)> = entries
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

    // Descending score; ties broken by repo-relative path (byte-lexicographic).
    // total_cmp gives a deterministic total order even in the presence of any
    // pathological float (transport already rejects non-finite vectors).
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.path.cmp(&b.1.path)));
    scored.truncate(top_n);

    let mut out: Vec<RankedCandidate> = scored
        .into_iter()
        .map(|(score, e)| RankedCandidate {
            file_uid: e.file_uid.clone(),
            path: e.path.clone(),
            content_hash: e.content_hash.clone(),
            score,
            near_tie: false,
        })
        .collect();

    for i in 0..out.len().saturating_sub(1) {
        if (out[i].score - out[i + 1].score).abs() <= NEAR_TIE_EPSILON {
            out[i].near_tie = true;
        }
    }
    out
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(file_uid: &str, path: &str, hash: &str, v: Vec<f32>) -> SeedVectorEntryV1 {
        SeedVectorEntryV1 {
            file_uid: file_uid.to_string(),
            path: path.to_string(),
            content_hash: hash.to_string(),
            vector: v,
        }
    }

    #[test]
    fn ranks_by_cosine_then_path() {
        let e1 = entry("f1", "b.ts", "h1", vec![1.0, 0.0]);
        let e2 = entry("f2", "a.ts", "h2", vec![1.0, 0.0]); // tie with e1 on score
        let e3 = entry("f3", "c.ts", "h3", vec![0.0, 1.0]); // orthogonal → 0, filtered
        let refs = vec![&e1, &e2, &e3];
        let ranked = rank(&[1.0, 0.0], &refs, 5);
        // e1 and e2 both score 1.0; path tie-break puts a.ts before b.ts.
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].path, "a.ts");
        assert_eq!(ranked[1].path, "b.ts");
        assert!(ranked[0].near_tie); // 1.0 vs 1.0 within epsilon
    }

    #[test]
    fn nothing_above_zero_is_empty() {
        let e = entry("f", "a", "h", vec![-1.0, 0.0]);
        let ranked = rank(&[1.0, 0.0], &[&e], 5);
        assert!(ranked.is_empty());
    }

    #[test]
    fn top_n_caps() {
        let es: Vec<SeedVectorEntryV1> = (0..20)
            .map(|i| entry(&format!("f{i}"), &format!("{i:02}.ts"), "h", vec![1.0, 0.0]))
            .collect();
        let refs: Vec<&SeedVectorEntryV1> = es.iter().collect();
        assert_eq!(rank(&[1.0, 0.0], &refs, 5).len(), 5);
        assert_eq!(rank(&[1.0, 0.0], &refs, 10).len(), 10);
    }

    #[test]
    fn freshness_partition() {
        let body = SeedVectorBodyV1 {
            entries: vec![
                entry("f1", "a", "hash_fresh", vec![1.0]),
                entry("f2", "b", "hash_old", vec![1.0]),
                entry("f3", "c", "hash_gone", vec![1.0]),
            ],
        };
        let mut current = HashMap::new();
        current.insert("f1".to_string(), "hash_fresh".to_string());
        current.insert("f2".to_string(), "hash_new".to_string()); // changed
                                                                  // f3 absent from corpus → dropped
        let p = partition_fresh(&body, &current);
        assert_eq!(p.fresh.len(), 1);
        assert_eq!(p.fresh[0].file_uid, "f1");
        assert_eq!(p.stale_count, 1);
        assert_eq!(p.total, 3);
    }
}
