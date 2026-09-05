//! Unit tests for SEED-CHUNK-2 structural classification (spec §4 unit row).

use super::*;

fn lines(s: &str) -> Vec<&str> {
    s.lines().collect()
}

#[test]
fn rust_test_attribute_directly_above_promotes() {
    // A `#[test] fn` in a PRODUCTION file (regions empty) is test by the attribute.
    let src = "pub fn prod() {}\n\n#[test]\nfn my_test() {\n    assert!(true);\n}\n";
    let fl = lines(src);
    let regions = compute_test_regions(ChunkLang::Rust, src);
    // `fn my_test` is on line 4 (1-indexed).
    assert!(structural_is_test(ChunkLang::Rust, &fl, 4, &regions));
    // `pub fn prod` on line 1 is NOT test.
    assert!(!structural_is_test(ChunkLang::Rust, &fl, 1, &regions));
}

#[test]
fn rust_cfg_test_mod_body_promotes_all_symbols_within() {
    // A production file with an in-file `#[cfg(test)] mod tests { … }`: EVERY symbol in
    // the body is test — including a non-`#[test]` helper (spec §2.1 enclosing).
    let src = "\
pub fn prod() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn helper() -> u32 { 1 }

    #[test]
    fn checks() { assert_eq!(helper(), 1); }
}
";
    let fl = lines(src);
    let regions = compute_test_regions(ChunkLang::Rust, src);
    // `fn helper` is line 7; `fn checks` line 10 — both inside the cfg(test) mod body.
    assert!(
        structural_is_test(ChunkLang::Rust, &fl, 7, &regions),
        "non-#[test] helper inside a cfg(test) mod is test"
    );
    assert!(structural_is_test(ChunkLang::Rust, &fl, 10, &regions));
    // `pub fn prod` line 1 is production.
    assert!(!structural_is_test(ChunkLang::Rust, &fl, 1, &regions));
}

#[test]
fn rust_production_mod_named_tests_is_not_promoted() {
    // The name-trap witness: a mod NAMED `tests` but NOT cfg(test)-gated is production.
    let src = "\
mod tests {
    pub fn util() {}
}
";
    let fl = lines(src);
    let regions = compute_test_regions(ChunkLang::Rust, src);
    assert!(
        !structural_is_test(ChunkLang::Rust, &fl, 2, &regions),
        "a mod named tests without #[cfg(test)] is not a test region"
    );
}

#[test]
fn braces_inside_comments_and_strings_do_not_open_a_region() {
    // A `#[cfg(test)] mod` whose body contains a string/comment with stray braces must
    // still close at the real `}` — the sanitizer blanks the fakes.
    let src = "\
#[cfg(test)]
mod tests {
    fn a() { let s = \"} not a brace {\"; /* } */ }
}
pub fn after() {}
";
    let fl = lines(src);
    let regions = compute_test_regions(ChunkLang::Rust, src);
    assert!(
        structural_is_test(ChunkLang::Rust, &fl, 3, &regions),
        "inside body"
    );
    assert!(
        !structural_is_test(ChunkLang::Rust, &fl, 5, &regions),
        "`pub fn after` after the mod close is production, not swallowed by a fake brace"
    );
}

#[test]
fn tsjs_describe_block_promotes_enclosed_symbols() {
    let src = "\
export function prod() {}

describe('suite', () => {
  function helper() { return 1; }
  it('works', () => { expect(helper()).toBe(1); });
});
";
    let fl = lines(src);
    let regions = compute_test_regions(ChunkLang::TsJs, src);
    // helper is line 4 — inside the describe block.
    assert!(structural_is_test(ChunkLang::TsJs, &fl, 4, &regions));
    // prod is line 1 — outside.
    assert!(!structural_is_test(ChunkLang::TsJs, &fl, 1, &regions));
}

#[test]
fn other_language_has_no_per_symbol_rule() {
    let src = "def helper():\n    pass\n";
    let fl = lines(src);
    let regions = compute_test_regions(ChunkLang::Other, src);
    assert!(!structural_is_test(ChunkLang::Other, &fl, 1, &regions));
}

#[test]
fn is_declaration_true_for_bodyless_callable_signatures() {
    assert!(
        is_declaration("t.rs", Some("METHOD"), "fn foo(&self) -> u32;"),
        "rust trait method decl"
    );
    assert!(
        is_declaration("db.h", Some("METHOD"), "virtual void Foo(int) = 0;"),
        "c++ pure virtual"
    );
    assert!(
        is_declaration("db.h", Some("FUNCTION"), "void DoThing(const Slice& key);"),
        "c/c++ prototype"
    );
    assert!(
        is_declaration("t.ts", Some("METHOD"), "bar(x: number): void;"),
        "ts interface member"
    );
}

#[test]
fn is_declaration_false_for_body_bearing_spans() {
    assert!(
        !is_declaration("t.rs", Some("FUNCTION"), "fn foo() -> u32 { 1 }"),
        "rust impl"
    );
    assert!(
        !is_declaration(
            "db.cc",
            Some("METHOD"),
            "void DoThing(const Slice& k) {\n  Work();\n}"
        ),
        "c++ impl"
    );
    assert!(
        !is_declaration(
            "t.ts",
            Some("METHOD"),
            "bar(x: number): void {\n  this.x = x;\n}"
        ),
        "ts method impl"
    );
}

#[test]
fn is_declaration_ignores_braces_in_default_args_when_commented() {
    // A body brace is required for "impl"; a `{` only inside a comment must not read as
    // a body (sanitizer blanks it).
    assert!(
        is_declaration("db.h", Some("FUNCTION"), "void f(int x /* = Foo{} */);"),
        "a brace inside a comment is not a body"
    );
}

#[test]
fn is_declaration_false_for_non_callable_subtypes() {
    // A CONSTANT/VARIABLE/TYPE_ALIAS whose initializer merely contains a `(` is NOT a
    // declaration-without-a-body — never labeled `(decl)` (the served-output defect this
    // gate fixes: `const EMBEDDED_TABLE_TOML = include_str!(...)` is not a decl).
    assert!(!is_declaration(
        "t.rs",
        Some("CONSTANT"),
        "const TABLE: &str = include_str!(\"t.toml\");"
    ));
    assert!(!is_declaration("t.rs", Some("VARIABLE"), "let x = foo();"));
    assert!(!is_declaration(
        "t.rs",
        Some("TYPE_ALIAS"),
        "type T = fn(u32);"
    ));
}

#[test]
fn is_declaration_false_for_no_brace_language() {
    // Python has no brace bodies and no forward-declaration concept: a `def foo():` is
    // NEVER a declaration, even though its span has `(` and no `{`.
    assert!(!is_declaration(
        "m.py",
        Some("FUNCTION"),
        "def foo():\n    return 1"
    ));
    assert!(!is_declaration(
        "m.py",
        Some("VARIABLE"),
        "v = embed(batch)"
    ));
}

#[test]
fn rust_same_line_test_attribute_promotes() {
    // `#[test] fn x() {}` — attribute inline on the item's own start line (the
    // preceding-lines scan alone misses it; reviewer edge-case).
    let src = "pub fn prod() {}\n#[test] fn my_test() { assert!(true); }\n";
    let fl = lines(src);
    let regions = compute_test_regions(ChunkLang::Rust, src);
    assert!(
        structural_is_test(ChunkLang::Rust, &fl, 2, &regions),
        "same-line #[test] fn is test"
    );
    assert!(!structural_is_test(ChunkLang::Rust, &fl, 1, &regions));
}

#[test]
fn rust_same_line_cfg_test_mod_body_promotes() {
    // `#[cfg(test)] mod tests { … }` all on one line's gate — the region must still open.
    let src = "\
pub fn prod() {}
#[cfg(test)] mod tests {
    fn helper() -> u32 { 1 }
}
";
    let fl = lines(src);
    let regions = compute_test_regions(ChunkLang::Rust, src);
    // `fn helper` is line 3 — inside the inline-gated cfg(test) mod body.
    assert!(
        structural_is_test(ChunkLang::Rust, &fl, 3, &regions),
        "symbol inside a same-line #[cfg(test)] mod is test"
    );
    assert!(!structural_is_test(ChunkLang::Rust, &fl, 1, &regions));
}

#[test]
fn rust_cfg_not_test_is_production_not_test() {
    // `#[cfg(not(test))]` is compiled when test is OFF — production, NEVER test. The old
    // `contains("test")` rule wrongly demoted it (reviewer honesty fix).
    let src = "#[cfg(not(test))]\nfn only_in_prod() {}\n";
    let fl = lines(src);
    let regions = compute_test_regions(ChunkLang::Rust, src);
    assert!(
        !structural_is_test(ChunkLang::Rust, &fl, 2, &regions),
        "cfg(not(test)) is production"
    );
}

#[test]
fn rust_cfg_feature_named_test_is_not_test() {
    // A feature whose NAME contains the text "test" is not the `test` cfg atom — a
    // production feature flag, not test code.
    let src = "#[cfg(feature = \"test_helpers\")]\nfn helpers() {}\n";
    let fl = lines(src);
    let regions = compute_test_regions(ChunkLang::Rust, src);
    assert!(
        !structural_is_test(ChunkLang::Rust, &fl, 2, &regions),
        "cfg(feature = \"test_helpers\") is not the test cfg atom"
    );
}

#[test]
fn rust_cfg_any_with_test_does_not_require_test() {
    // `#[cfg(any(test, feature = "x"))]` can be true WITHOUT test (via feature x), so its
    // truth does not REQUIRE test — not promoted (conservative, promote-only precision).
    let src = "#[cfg(any(test, feature = \"x\"))]\nfn maybe() {}\n";
    let fl = lines(src);
    let regions = compute_test_regions(ChunkLang::Rust, src);
    assert!(
        !structural_is_test(ChunkLang::Rust, &fl, 2, &regions),
        "any(test, …) does not require test"
    );
}

#[test]
fn rust_cfg_all_with_test_requires_test() {
    // `#[cfg(all(test, unix))]` is false whenever test is off ⇒ test-only ⇒ promoted.
    let src = "#[cfg(all(test, unix))]\nfn only_test_unix() {}\n";
    let fl = lines(src);
    let regions = compute_test_regions(ChunkLang::Rust, src);
    assert!(
        structural_is_test(ChunkLang::Rust, &fl, 2, &regions),
        "all(test, unix) requires test"
    );
    // And a cfg(all(test, unix)) gating a mod opens a region.
    let src2 = "#[cfg(all(test, unix))]\nmod t {\n    fn h() {}\n}\n";
    let fl2 = lines(src2);
    let regions2 = compute_test_regions(ChunkLang::Rust, src2);
    assert!(structural_is_test(ChunkLang::Rust, &fl2, 3, &regions2));
}

#[test]
fn lang_selection_is_by_extension_only() {
    assert_eq!(lang_for_path("src/a.rs"), ChunkLang::Rust);
    assert_eq!(lang_for_path("src/a.tsx"), ChunkLang::TsJs);
    assert_eq!(lang_for_path("src/a.py"), ChunkLang::Other);
}

// ---------------------------------------------------------------------------
// review-2 item 3: literal text (raw strings, template literals) must NOT be
// read as code — a fake `#[cfg(test)] mod {` / `describe( {` inside a literal
// can neither open a test region nor mask a real body.
// ---------------------------------------------------------------------------

#[test]
fn rust_raw_string_with_fake_cfg_test_mod_opens_no_region() {
    // A production function whose body holds a RAW STRING containing a fully-formed
    // `#[cfg(test)] mod fake { … }`. Its braces/attribute are literal data, not code, so
    // no test region opens and the production symbol below it stays production.
    let src = "\
pub fn emit() -> String {
    let s = r#\"
        #[cfg(test)]
        mod fake {
            fn planted() {}
        }
    \"#;
    s.to_string()
}

pub fn also_prod() -> u32 { 1 }
";
    let fl = lines(src);
    let regions = compute_test_regions(ChunkLang::Rust, src);
    assert!(
        regions.ranges.is_empty(),
        "a #[cfg(test)] mod inside a raw string must not open a test region: {:?}",
        regions
    );
    // `pub fn also_prod` is the last symbol — well past the literal — still production.
    let line = fl.iter().position(|l| l.contains("also_prod")).unwrap() + 1;
    assert!(
        !structural_is_test(ChunkLang::Rust, &fl, line, &regions),
        "production symbol after a literal fake test mod stays production"
    );
}

#[test]
fn rust_hashed_raw_string_does_not_terminate_early_on_inner_quote() {
    // `r#"…"#` must survive an inner `"` (only `"#` closes it). An inner `"` followed by
    // real-looking code must still be treated as literal.
    let src = "\
pub fn q() -> String {
    let s = r#\"a \" #[cfg(test)] mod x { fn y() {} }\"#;
    s.to_string()
}
pub fn prod() {}
";
    let fl = lines(src);
    let regions = compute_test_regions(ChunkLang::Rust, src);
    assert!(
        regions.ranges.is_empty(),
        "hashed raw string content is not code: {:?}",
        regions
    );
    let line = fl.iter().position(|l| l.contains("fn prod")).unwrap() + 1;
    assert!(!structural_is_test(ChunkLang::Rust, &fl, line, &regions));
}

#[test]
fn tsjs_template_literal_with_fake_describe_opens_no_region() {
    // A TS module whose exported constant is a TEMPLATE LITERAL containing a fake
    // `describe('x', () => { … })`. It is string data; no test region opens.
    let src = "\
export const doc = `
  describe('planted', () => {
    it('is not real', () => {});
  });
`;

export function real() { return 1; }
";
    let fl = lines(src);
    let regions = compute_test_regions(ChunkLang::TsJs, src);
    assert!(
        regions.ranges.is_empty(),
        "describe(...) inside a template literal must not open a region: {:?}",
        regions
    );
    let line = fl.iter().position(|l| l.contains("function real")).unwrap() + 1;
    assert!(
        !structural_is_test(ChunkLang::TsJs, &fl, line, &regions),
        "a symbol after a template-literal fake describe stays production"
    );
}

#[test]
fn tsjs_real_describe_still_opens_region_after_the_literal_fix() {
    // Guard against over-blanking: a GENUINE describe() block (not in a literal) still
    // opens a region — the raw/template handling must not suppress real evidence.
    let src = "\
describe('real suite', () => {
  it('works', () => {});
});
";
    let regions = compute_test_regions(ChunkLang::TsJs, src);
    assert!(
        !regions.ranges.is_empty(),
        "a real describe() must still open a region"
    );
    let fl = lines(src);
    assert!(structural_is_test(ChunkLang::TsJs, &fl, 2, &regions));
}

#[test]
fn tsjs_bare_test_call_does_not_swallow_following_function() {
    // review-3 item 1: a test call with NO callback body (`describe("metadata only");`)
    // must NOT adopt a LATER, unrelated `{` — here a following PRODUCTION function's body.
    // The body search is bounded to WITHIN the call's own parentheses.
    let src = "\
describe('metadata only');

function production() {
  return 1;
}
";
    let fl = lines(src);
    let regions = compute_test_regions(ChunkLang::TsJs, src);
    assert!(
        regions.ranges.is_empty(),
        "a bodyless describe() call opens no region: {:?}",
        regions
    );
    let line = fl
        .iter()
        .position(|l| l.contains("function production"))
        .unwrap()
        + 1;
    assert!(
        !structural_is_test(ChunkLang::TsJs, &fl, line, &regions),
        "a production function after a bodyless describe() stays production"
    );
}

#[test]
fn tsjs_tagged_template_with_fake_describe_opens_no_region() {
    // review-3 item 2: a TAGGED template literal (an identifier byte precedes the
    // backtick) is still STRING data. A fake `describe('x', () => { … })` inside it must
    // not open a region, and a following production symbol stays production.
    let src = "\
const q = sql`describe('planted', () => { it('x', () => {}); })`;

export function real() { return 1; }
";
    let fl = lines(src);
    let regions = compute_test_regions(ChunkLang::TsJs, src);
    assert!(
        regions.ranges.is_empty(),
        "a tagged-template fake describe must not open a region: {:?}",
        regions
    );
    let line = fl.iter().position(|l| l.contains("function real")).unwrap() + 1;
    assert!(
        !structural_is_test(ChunkLang::TsJs, &fl, line, &regions),
        "a symbol after a tagged-template fake describe stays production"
    );
}

#[test]
fn raw_string_body_brace_is_not_a_decl_body() {
    // `is_declaration` uses the sanitized span: a raw string containing `{` must NOT count
    // as a body. A bodyless prototype whose signature holds a raw string with a `{` is
    // still a declaration. (Extension picks the syntax family; the brace is structural.)
    let span = "fn proto(msg: &str = r#\"note { not a body }\"#);";
    assert!(
        is_declaration("src/api.rs", Some("FUNCTION"), span),
        "a brace inside a raw string is literal, not a real body — still a decl"
    );
    // And a real body still reads as an implementation.
    let impl_span = "fn real() -> u32 { 1 }";
    assert!(!is_declaration("src/api.rs", Some("FUNCTION"), impl_span));
}
