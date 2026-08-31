//! FIND-RANK-1 (§2.1) — the deterministic SYMBOL rank comparator: the facts tier's
//! ordering IS part of the contract, so it lives in one pure, unit-tested function
//! rather than diffusing into SQL and the row mapper. Given the user's query and a
//! symbol's ranking inputs it produces a TOTAL-ORDER key; sorting by that key yields
//! the ratified order:
//!
//!   (a) non-test before test — the STORED `is_test` FACT of the defining file, NEVER
//!       a path string (STANDING HONESTY RULE 2). Unknown ranks in the NON-TEST
//!       partition (§2.4 — never demoted on unknown; the FIXTURE-POLLUTION-1 direction).
//!   (b) kind weight — type-defining/callable symbols before variables/constants/
//!       properties. The LESSER set is the one the contract NAMES; unknown kind ranks
//!       Prominent (never demoted on unknown).
//!   (c) match quality — exact name match, then prefix, then substring (over the short
//!       `name`; a hit that entered only via `qualified_name` is the weakest, substring).
//!   (d) shorter qualified name before longer.
//!   (e) path ASC, then stable_key ASC — the final deterministic tiebreaks (stable_key
//!       is unique per snapshot, so the order is TOTAL: no pair is ever "equal", which
//!       is what makes `--exact` reproducible).
//!
//! Abstraction record — module: `find_facts::rank`; concrete current user:
//! `find_facts::queries::symbols` (the only caller); axis: the ranking CONTRACT is a
//! pure domain rule with a documented unit-test obligation (§4) — it gets a pure seam
//! off the I/O-bound query body; rejected simpler alternative: inlining a `sort_by`
//! closure in `symbols` (no unit seam for the pair/determinism tests §4 mandates, and
//! it would push the query body's concerns together).

/// The ranking inputs for ONE candidate symbol (a borrow-only view; the comparator
/// allocates nothing). Everything here is a stored FACT or the user's query — no
/// path-string classification (STANDING HONESTY RULE 2).
pub(super) struct SymbolRank<'a> {
    /// The symbol's short name (`nodes.name`).
    pub name: &'a str,
    /// The symbol's qualified name, when stored — its length is tiebreak (d).
    pub qualified_name: Option<&'a str>,
    /// The STORED `is_test` fact of the defining file; `None` = unknown (ranks non-test).
    pub is_test: Option<bool>,
    /// The stored symbol subtype (`CLASS`, `VARIABLE`, …); `None` = unknown (ranks Prominent).
    pub subtype: Option<&'a str>,
    /// The owning file path when known — tiebreak (e). `None`/unknown sorts after known.
    pub path: Option<&'a str>,
    /// The symbol's stable key — the unique final tiebreak (total order).
    pub stable_key: &'a str,
}

/// The subtypes the contract NAMES as the demoted tier: "variables/constants/
/// properties" (§2.1b). `ENUM_MEMBER` is a named enum value (constant-like), so it
/// rides with constants. These are the STORED `nodes.subtype` SCREAMING_SNAKE spellings
/// (`NodeSubtype` serde). We whitelist the LESSER set — never the Prominent set — so a
/// new or unknown producer subtype is ranked Prominent by default, never SILENTLY
/// demoted (the never-demote-on-unknown rule applied to kind).
const LESSER_SUBTYPES: [&str; 4] = ["VARIABLE", "CONSTANT", "PROPERTY", "ENUM_MEMBER"];

/// Kind weight (§2.1b): `0` = Prominent (type-defining/callable OR unknown), `1` = the
/// named data-holding tier. Lower sorts first.
fn kind_weight(subtype: Option<&str>) -> u8 {
    match subtype {
        Some(s) if LESSER_SUBTYPES.contains(&s) => 1,
        _ => 0,
    }
}

/// Test partition (§2.1a): `0` = non-test OR unknown, `1` = a known test file. Lower
/// sorts first. Unknown (`None`) ranks with non-test — never demoted on unknown (§2.4).
fn test_partition(is_test: Option<bool>) -> u8 {
    match is_test {
        Some(true) => 1,
        Some(false) | None => 0,
    }
}

/// Match quality (§2.1c) of the QUERY against the symbol's SHORT name, case-insensitive:
/// `0` exact, `1` prefix, `2` substring. A symbol that matched only on its qualified
/// name (its short name does not contain the query) falls to `2` — the weakest tier,
/// which is the honest classification (the strong signal is a short-name match).
fn match_quality(name: &str, query_lower: &str) -> u8 {
    let name_lower = name.to_lowercase();
    if name_lower == query_lower {
        0
    } else if name_lower.starts_with(query_lower) {
        1
    } else {
        2
    }
}

/// The total-order rank key for a symbol under `query`. `Ord` on the tuple encodes the
/// rule precedence (a) → (e); lower is better. `path` maps to `(is_none, value)` so a
/// KNOWN path sorts before an unknown/absent one, then lexicographically; `stable_key`
/// is the unique final key, so no two distinct symbols ever compare equal.
#[allow(clippy::type_complexity)] // a rank tuple consumed only by `cmp`; naming each
                                  // component as a struct would obscure the Ord precedence.
fn rank_key<'a>(
    s: &SymbolRank<'a>,
    query_lower: &str,
) -> (u8, u8, u8, usize, (bool, &'a str), &'a str) {
    let qname_len = s.qualified_name.unwrap_or(s.name).len();
    (
        test_partition(s.is_test),
        kind_weight(s.subtype),
        match_quality(s.name, query_lower),
        qname_len,
        (s.path.is_none(), s.path.unwrap_or("")),
        s.stable_key,
    )
}

/// Sort `symbols` IN PLACE into the ratified display order for `query` (§2.1). The
/// query is lowercased ONCE. Total order (ends in the unique `stable_key`), so the sort
/// is deterministic and stable-independent.
pub(super) fn sort_symbols(symbols: &mut [SymbolRank<'_>], query: &str) {
    let query_lower = query.to_lowercase();
    symbols.sort_by(|a, b| rank_key(a, &query_lower).cmp(&rank_key(b, &query_lower)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s<'a>(
        name: &'a str,
        qualified_name: Option<&'a str>,
        is_test: Option<bool>,
        subtype: Option<&'a str>,
        path: Option<&'a str>,
        stable_key: &'a str,
    ) -> SymbolRank<'a> {
        SymbolRank {
            name,
            qualified_name,
            is_test,
            subtype,
            path,
            stable_key,
        }
    }

    /// Order two symbols under `query` and return the winning (first) stable_key.
    fn winner(a: SymbolRank<'_>, b: SymbolRank<'_>, query: &str) -> String {
        let mut v = vec![a, b];
        sort_symbols(&mut v, query);
        v[0].stable_key.to_string()
    }

    // ── (a) non-test before test — the is_test FACT, never a path string ──────────

    #[test]
    fn rule_a_non_test_beats_test_all_else_equal() {
        let prod = s(
            "FormSet",
            None,
            Some(false),
            Some("CLASS"),
            Some("f/a.py"),
            "k_prod",
        );
        let test = s(
            "FormSet",
            None,
            Some(true),
            Some("CLASS"),
            Some("f/b.py"),
            "k_test",
        );
        assert_eq!(winner(prod, test, "formset"), "k_prod");
    }

    #[test]
    fn rule_a_unknown_is_test_ranks_with_non_test_never_demoted() {
        // Unknown (None) must NOT be demoted below a known test symbol (§2.4).
        let unknown = s(
            "FormSet",
            None,
            None,
            Some("CLASS"),
            Some("f/a.py"),
            "k_unknown",
        );
        let test = s(
            "FormSet",
            None,
            Some(true),
            Some("CLASS"),
            Some("f/b.py"),
            "k_test",
        );
        assert_eq!(winner(unknown, test, "formset"), "k_unknown");
    }

    // ── (b) kind weight — prominent before the named lesser set; unknown prominent ─

    #[test]
    fn rule_b_class_beats_variable_all_else_equal() {
        let class = s(
            "FormSet",
            None,
            Some(false),
            Some("CLASS"),
            Some("f/a.py"),
            "k_class",
        );
        let var = s(
            "FormSet",
            None,
            Some(false),
            Some("VARIABLE"),
            Some("f/b.py"),
            "k_var",
        );
        assert_eq!(winner(class, var, "formset"), "k_class");
    }

    #[test]
    fn rule_b_lesser_set_is_the_named_data_tier() {
        for lesser in ["VARIABLE", "CONSTANT", "PROPERTY", "ENUM_MEMBER"] {
            assert_eq!(kind_weight(Some(lesser)), 1, "{lesser} is the demoted tier");
        }
        for prominent in [
            "CLASS",
            "INTERFACE",
            "ENUM",
            "STRUCT",
            "FUNCTION",
            "METHOD",
            "TYPE_ALIAS",
        ] {
            assert_eq!(kind_weight(Some(prominent)), 0, "{prominent} is prominent");
        }
    }

    #[test]
    fn rule_b_unknown_subtype_ranks_prominent_never_demoted() {
        assert_eq!(kind_weight(None), 0, "unknown kind ranks prominent");
        let unknown = s(
            "FormSet",
            None,
            Some(false),
            None,
            Some("f/a.py"),
            "k_unknown",
        );
        let var = s(
            "FormSet",
            None,
            Some(false),
            Some("VARIABLE"),
            Some("f/b.py"),
            "k_var",
        );
        assert_eq!(winner(unknown, var, "formset"), "k_unknown");
    }

    // ── (c) match quality — exact, then prefix, then substring ────────────────────

    #[test]
    fn rule_c_exact_beats_prefix_beats_substring() {
        assert_eq!(match_quality("FormSet", "formset"), 0);
        assert_eq!(match_quality("FormSetFactory", "formset"), 1);
        assert_eq!(match_quality("BaseFormSet", "formset"), 2);
        let exact = s(
            "FormSet",
            None,
            Some(false),
            Some("CLASS"),
            Some("f/a.py"),
            "k_exact",
        );
        let prefix = s(
            "FormSetX",
            None,
            Some(false),
            Some("CLASS"),
            Some("f/b.py"),
            "k_prefix",
        );
        assert_eq!(winner(exact, prefix, "formset"), "k_exact");
        let prefix2 = s(
            "FormSetX",
            None,
            Some(false),
            Some("CLASS"),
            Some("f/a.py"),
            "k_prefix",
        );
        let sub = s(
            "BaseFormSet",
            None,
            Some(false),
            Some("CLASS"),
            Some("f/b.py"),
            "k_sub",
        );
        assert_eq!(winner(prefix2, sub, "formset"), "k_prefix");
    }

    #[test]
    fn rule_c_qualified_only_match_is_substring_tier() {
        // The query is nowhere in the short name — it entered via qualified_name — so
        // match quality is the weakest tier (2), not promoted to exact/prefix.
        assert_eq!(match_quality("handler", "offer"), 2);
    }

    // ── (d) shorter qualified name before longer ──────────────────────────────────

    #[test]
    fn rule_d_shorter_qualified_name_first() {
        let short = s(
            "offer",
            Some("app.offer"),
            Some(false),
            Some("CLASS"),
            Some("f/a.py"),
            "k_short",
        );
        let long = s(
            "offer",
            Some("app.sub.module.offer"),
            Some(false),
            Some("CLASS"),
            Some("f/b.py"),
            "k_long",
        );
        assert_eq!(winner(short, long, "offer"), "k_short");
    }

    // ── (e) path ASC, then stable_key — deterministic total order ─────────────────

    #[test]
    fn rule_e_path_then_stable_key_break_ties() {
        let a = s(
            "offer",
            Some("offer"),
            Some(false),
            Some("CLASS"),
            Some("f/a.py"),
            "k2",
        );
        let b = s(
            "offer",
            Some("offer"),
            Some(false),
            Some("CLASS"),
            Some("f/b.py"),
            "k1",
        );
        // Same partition/kind/quality/qname-len → path ASC decides: f/a.py before f/b.py.
        assert_eq!(winner(a, b, "offer"), "k2");
    }

    #[test]
    fn total_order_is_deterministic_regardless_of_input_order() {
        // A property test in miniature: the SAME set sorts to the SAME sequence no
        // matter the starting permutation (total order → no ambiguity). This is the
        // determinism the facts-tier contract requires (§2.1 / §4).
        let make = || {
            vec![
                s(
                    "BaseFormSet",
                    None,
                    Some(false),
                    Some("CLASS"),
                    Some("django/forms/formsets.py"),
                    "k_base",
                ),
                s(
                    "FormSet",
                    None,
                    Some(false),
                    Some("CLASS"),
                    Some("django/forms/formsets.py"),
                    "k_formset",
                ),
                s(
                    "AbsoluteMaxFavoriteDrinksFormSet",
                    None,
                    Some(true),
                    Some("VARIABLE"),
                    Some("tests/forms_tests/x.py"),
                    "k_test_var",
                ),
                s(
                    "formset_factory",
                    None,
                    Some(false),
                    Some("FUNCTION"),
                    Some("django/forms/formsets.py"),
                    "k_factory",
                ),
                s(
                    "_FORMSET_CONST",
                    None,
                    Some(false),
                    Some("CONSTANT"),
                    Some("django/forms/formsets.py"),
                    "k_const",
                ),
            ]
        };
        let mut v1 = make();
        sort_symbols(&mut v1, "formset");
        let order1: Vec<&str> = v1.iter().map(|x| x.stable_key).collect();

        let mut v2 = make();
        v2.reverse();
        sort_symbols(&mut v2, "formset");
        let order2: Vec<&str> = v2.iter().map(|x| x.stable_key).collect();

        assert_eq!(order1, order2, "same set → same order from any permutation");
        // And the ratified shape: exact-match production class first, test variable last.
        assert_eq!(
            order1.first(),
            Some(&"k_formset"),
            "exact production class leads"
        );
        assert_eq!(
            order1.last(),
            Some(&"k_test_var"),
            "test variable demoted last"
        );
        // BaseFormSet (prefix, prod class) beats formset_factory (prefix, prod fn)? Both
        // are prominent + prefix; qualified_name len equal (None → name len): BaseFormSet
        // (11) vs formset_factory (15) → BaseFormSet first, then the constant is lesser.
        assert!(
            order1.iter().position(|k| *k == "k_base").unwrap()
                < order1.iter().position(|k| *k == "k_const").unwrap(),
            "prominent prefix class before the constant: {order1:?}"
        );
    }
}
