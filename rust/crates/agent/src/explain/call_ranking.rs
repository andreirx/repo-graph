//! COHERENCE-LEAF-SERVE-IMPL-2: relevance ranking for explain's caller/callee item lists.
//!
//! **Why this exists (the Output-EXACT contract).** `explain_symbol` emits an ORDERED, budget-TRUNCATED
//! caller/callee `items` list. The daemon serves the underlying caller/callee SET from the LiveGraph on a
//! GREEN `callgraph_cert`, which proves only MULTISET (content) equality with SQLite — NOT row order
//! (SQLite has no `ORDER BY`; the LiveGraph orders by partition then edge). Truncating an order-sensitive
//! list off a merely set-equal source would show a DIFFERENT top-N subset depending on which store served
//! it. Resolution (operator-ratified `2d6d00d`, BY THE VISION — not "match SQLite"): rank the FULL
//! caller/callee set by a TOTAL deterministic order BEFORE truncation. Because the rank is a pure function
//! of the (cert-proven-equal) set, both stores produce the SAME ranked top-N → the multiset cert SUFFICES
//! → the LiveGraph callgraph serve is Output-EXACT, and the truncated view is more relevant than the old
//! rowid-arbitrary subset.
//!
//! **The order** (mirrors the shipped `top_modules` relevance model): (1) module-concentration DESC — how
//! many of THIS symbol's callers/callees share the row's module; (2) `module_path` ASC; (3) `name` ASC;
//! (4) `stable_key` ASC. Keys (2)–(4) are a TOTAL order over DISTINCT symbols; the only ties are between
//! field-identical duplicate edges (same `stable_key` ⇒ same `name`/`module`), so the truncated subset is
//! byte-deterministic regardless of the input order the store happened to return.

use std::collections::HashMap;

use crate::storage_port::{AgentCalleeRow, AgentCallerRow};

/// The module-grouping sentinel for a caller/callee with no owning module. MUST match the value
/// `group_by_module` renders in `top_modules`, so a row's ranking concentration equals its displayed
/// `top_modules` count (the operator's "mirrors the shipped `top_modules` relevance model").
const UNKNOWN_MODULE: &str = "(unknown)";

/// Per-module row count over an iterator of (optional) module paths — the SHARED counting that backs both
/// `top_modules` (`group_by_module`) and this module's ranking concentration. One place for the
/// `UNKNOWN_MODULE` sentinel keeps the two in lockstep (a correctness tie, not a convenience).
pub(super) fn module_counts<'a>(
    module_paths: impl Iterator<Item = Option<&'a str>>,
) -> HashMap<String, u64> {
    let mut counts: HashMap<String, u64> = HashMap::new();
    for mp in module_paths {
        *counts
            .entry(mp.unwrap_or(UNKNOWN_MODULE).to_string())
            .or_insert(0) += 1;
    }
    counts
}

/// Rank caller/callee rows in place by the relevance order documented at the module head. Generic over the
/// row type via field accessors — the ONE shared helper for callers and callees (the two structurally
/// identical row types). Applied to the FULL set BEFORE truncation so the truncated top-N is a pure
/// function of the cert-proven-equal set.
fn rank_call_rows<T>(
    rows: &mut [T],
    module_of: impl Fn(&T) -> Option<&str>,
    name_of: impl Fn(&T) -> &str,
    key_of: impl Fn(&T) -> &str,
) {
    // Concentration over the FULL set (owned keys so the immutable borrow ends before the sort).
    let counts = module_counts(rows.iter().map(&module_of));
    rows.sort_by(|a, b| {
        let ma = module_of(a).unwrap_or(UNKNOWN_MODULE);
        let mb = module_of(b).unwrap_or(UNKNOWN_MODULE);
        let ca = counts[ma];
        let cb = counts[mb];
        cb.cmp(&ca) // (1) module-concentration DESC
            .then_with(|| ma.cmp(mb)) // (2) module_path ASC
            .then_with(|| name_of(a).cmp(name_of(b))) // (3) name ASC
            .then_with(|| key_of(a).cmp(key_of(b))) // (4) stable_key ASC
    });
}

/// Rank caller rows (incoming `CALLS` edges) by relevance, in place.
pub(super) fn rank_caller_rows(rows: &mut [AgentCallerRow]) {
    rank_call_rows(
        rows,
        |r| r.module_path.as_deref(),
        |r| r.name.as_str(),
        |r| r.stable_key.as_str(),
    );
}

/// Rank callee rows (outgoing `CALLS` edges) by relevance, in place.
pub(super) fn rank_callee_rows(rows: &mut [AgentCalleeRow]) {
    rank_call_rows(
        rows,
        |r| r.module_path.as_deref(),
        |r| r.name.as_str(),
        |r| r.stable_key.as_str(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A caller row in module `module` with `name`/`stable_key` — the fields the rank reads.
    fn caller(name: &str, module: Option<&str>, key: &str) -> AgentCallerRow {
        AgentCallerRow {
            stable_key: key.to_string(),
            name: name.to_string(),
            file: None,
            line: None,
            module_path: module.map(str::to_string),
            module_stable_key: module.map(|m| format!("repo:{m}:MODULE")),
        }
    }

    /// Build N callers split across two modules: the first `alpha_n` in `alpha` (high concentration), the
    /// rest in `beta`. `name`/`stable_key` are zero-padded so string order == numeric order.
    fn fanin(alpha_n: usize, beta_n: usize) -> Vec<AgentCallerRow> {
        let mut rows = Vec::new();
        for i in 0..alpha_n {
            let n = format!("a{i:02}");
            rows.push(caller(
                &n,
                Some("alpha"),
                &format!("repo:alpha/f.ts#{n}:SYMBOL:FUNCTION"),
            ));
        }
        for i in 0..beta_n {
            let n = format!("b{i:02}");
            rows.push(caller(
                &n,
                Some("beta"),
                &format!("repo:beta/f.ts#{n}:SYMBOL:FUNCTION"),
            ));
        }
        rows
    }

    fn keys(rows: &[AgentCallerRow]) -> Vec<String> {
        rows.iter().map(|r| r.stable_key.clone()).collect()
    }

    /// The load-bearing property: the ranked order is a PURE FUNCTION of the SET — feeding the same rows in
    /// any input order yields byte-identical output. This is exactly what makes the multiset-cert serve
    /// Output-EXACT (SQLite order ≠ LiveGraph order, but both rank to the same sequence).
    #[test]
    fn rank_is_order_independent() {
        let forward = fanin(12, 6);
        let mut reversed = forward.clone();
        reversed.reverse();
        let mut shuffled = forward.clone();
        shuffled.swap(0, 17);
        shuffled.swap(3, 9);

        let mut a = forward.clone();
        let mut b = reversed;
        let mut c = shuffled;
        rank_caller_rows(&mut a);
        rank_caller_rows(&mut b);
        rank_caller_rows(&mut c);

        assert_eq!(keys(&a), keys(&b), "reversed input ranks identically");
        assert_eq!(keys(&a), keys(&c), "shuffled input ranks identically");
    }

    /// Concentration DESC dominates: every `alpha` caller (concentration 12) ranks before every `beta`
    /// caller (concentration 6) regardless of name; within a module, name ASC then key ASC.
    #[test]
    fn rank_orders_by_concentration_then_name() {
        let mut rows = fanin(12, 6);
        rows.reverse(); // start from a non-ranked order
        rank_caller_rows(&mut rows);

        // alpha block (12) first, names a00..a11 ascending; then beta block (6), b00..b05.
        let expected: Vec<String> = (0..12)
            .map(|i| format!("repo:alpha/f.ts#a{i:02}:SYMBOL:FUNCTION"))
            .chain((0..6).map(|i| format!("repo:beta/f.ts#b{i:02}:SYMBOL:FUNCTION")))
            .collect();
        assert_eq!(keys(&rows), expected);
    }

    /// The ranking is LOAD-BEARING for truncation: with fan-in > the cap, the ranked top-N is a different
    /// SUBSET than the raw (pre-rank) top-N — so without ranking the truncated explain output would depend
    /// on which store served the rows. Here raw top-15 (alpha a00..a11 + beta b00..b02) already equals the
    /// ranked top-15, so to exhibit a real subset change we start from a beta-first raw order.
    #[test]
    fn rank_changes_the_truncated_subset() {
        const CAP: usize = 15;
        // Raw order: beta first (b00..b05) then alpha (a00..a11). Raw top-15 = {b00..b05, a00..a08}.
        let mut raw = Vec::new();
        for i in 0..6 {
            let n = format!("b{i:02}");
            raw.push(caller(
                &n,
                Some("beta"),
                &format!("repo:beta/f.ts#{n}:SYMBOL:FUNCTION"),
            ));
        }
        for i in 0..12 {
            let n = format!("a{i:02}");
            raw.push(caller(
                &n,
                Some("alpha"),
                &format!("repo:alpha/f.ts#{n}:SYMBOL:FUNCTION"),
            ));
        }
        let raw_top: Vec<String> = keys(&raw).into_iter().take(CAP).collect();

        let mut ranked = raw.clone();
        rank_caller_rows(&mut ranked);
        let ranked_top: Vec<String> = keys(&ranked).into_iter().take(CAP).collect();

        // Ranked top-15 = {a00..a11, b00..b02}; it DROPS b03..b05 and ADDS a09..a11 vs the raw subset.
        assert_ne!(
            raw_top, ranked_top,
            "ranking must change the truncated subset (else it is not load-bearing)"
        );
        assert!(
            ranked_top.iter().any(|k| k.contains("#a11:")),
            "ranked top-15 includes the high-concentration tail a09..a11"
        );
        assert!(
            !ranked_top.iter().any(|k| k.contains("#b05:")),
            "ranked top-15 drops the low-concentration overflow b03..b05"
        );
    }

    /// Duplicate edges (same `stable_key`) are field-identical, so ties between them never perturb the
    /// rendered bytes — the multiset (multiplicity-preserving) serve stays exact.
    #[test]
    fn rank_duplicate_edges_are_deterministic() {
        let dup = caller("dup", Some("m"), "repo:m/f.ts#dup:SYMBOL:FUNCTION");
        let other = caller("zed", Some("m"), "repo:m/f.ts#zed:SYMBOL:FUNCTION");
        let mut a = vec![dup.clone(), dup.clone(), other.clone()];
        let mut b = vec![other, dup.clone(), dup];
        rank_caller_rows(&mut a);
        rank_caller_rows(&mut b);
        assert_eq!(keys(&a), keys(&b));
        // dup (name "dup") sorts before zed; both copies of dup are adjacent and identical.
        assert_eq!(
            keys(&a),
            vec![
                "repo:m/f.ts#dup:SYMBOL:FUNCTION".to_string(),
                "repo:m/f.ts#dup:SYMBOL:FUNCTION".to_string(),
                "repo:m/f.ts#zed:SYMBOL:FUNCTION".to_string(),
            ]
        );
    }
}
