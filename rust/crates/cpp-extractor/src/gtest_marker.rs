//! IS-TEST-CPP-1: STRUCTURAL gtest/gmock test-marker detection for a parsed C++
//! translation unit.
//!
//! The whole point of this slice is that a file's test status is EVIDENCE, never
//! its filename (a production file named `foo_test.cc` is not a test; a test file
//! named `db_bench.cc` might be). Detection therefore runs over the tree-sitter
//! parse tree, not a substring scan: a `TEST(` in a `//` comment parses as a
//! `comment` node and a `#include <gtest/gtest.h>` inside a string literal parses
//! as a `string_literal` node — neither is a `preproc_include` nor a top-level
//! macro-style `function_definition`, so the comment/string-literal trap
//! (validation fixture (d)) is excluded for free.
//!
//! A file carries the marker iff it has, at translation-unit top level, either:
//!   - a `#include` of a `gtest/…` or `gmock/…` header (`<…>` or `"…"` form), OR
//!   - a `TEST` / `TEST_F` / `TEST_P` / `TYPED_TEST` macro invocation.
//!
//! gtest/gmock family ONLY (spec §2.2); Catch2/doctest/CppUnit are named
//! follow-up candidates, never built speculatively.

use tree_sitter::Node;

/// Top-level gtest/gmock macro identifiers that mark a file as a test.
/// Fixed set (spec §2.1) — an operation-vs-variant note: this is a closed
/// vocabulary, so a literal slice, not an abstraction.
const TEST_MACROS: &[&str] = &["TEST", "TEST_F", "TEST_P", "TYPED_TEST"];

/// Header-path prefixes (after the delimiter is stripped) that identify a
/// gtest/gmock include.
const GTEST_INCLUDE_PREFIXES: &[&str] = &["gtest/", "gmock/"];

/// Return true iff the translation unit rooted at `root` carries a gtest/gmock
/// STRUCTURAL marker. `src` is the file's UTF-8 source bytes (for node text).
///
/// "Top-level" (spec §2.1) means the scope where a gtest macro is legal — global
/// OR namespace scope, NOT inside a function body. gtest's `TEST`/`TEST_F`/… are
/// commonly wrapped in a `namespace test { … }` (the dominant vcmi idiom;
/// corpus-verified 2026-09-03), so the scan descends through `namespace_definition`
/// bodies, and through preprocessor conditionals (`#ifndef` include guards,
/// `#if` feature gates) — a header's gtest include sits inside its include guard.
/// It does NOT descend into function bodies — an `EXPECT_EQ(…)` call site inside a
/// test body is not itself a marker.
///
/// tree-sitter-cpp parses a top-level `TEST_F(Suite, Name) { … }` as a
/// `function_definition` (no return type) whose `declarator` is a
/// `function_declarator` whose inner `declarator` is the `identifier` `TEST_F`
/// (probe-verified against tree-sitter-cpp 0.23, 2026-09-03).
pub fn detect_gtest_marker(root: &Node, src: &[u8]) -> bool {
    scope_has_marker(root, src)
}

/// Scan a scope node's direct children — a `translation_unit`, a namespace's
/// `declaration_list`, or a conditional-compilation block — for a gtest marker,
/// recursing into nested namespaces and preprocessor conditionals. Recursion
/// depth is bounded by real namespace / guard nesting (shallow); each level uses
/// its own cursor.
///
/// Descending into `preproc_ifdef`/`preproc_if`/`preproc_else`/`preproc_elif` is
/// load-bearing, not cosmetic: a C/C++ HEADER wraps its whole body — gtest
/// include included — in an `#ifndef … #define … #endif` include guard, so the
/// gtest `#include` is a child of a `preproc_ifdef`, NOT a direct child of the
/// translation unit. Without this descent every guarded gtest header (e.g.
/// leveldb's `util/testutil.h`) is missed (review-0 item 2). This mirrors the
/// extractor's own `walk_top_level`, which already recurses these same nodes to
/// find guarded declarations — a `#include`/`TEST` in a conditional block is
/// still at namespace/global scope. A gtest macro is legal there; `EXPECT_EQ`
/// inside a function body is still excluded because we never descend function
/// bodies.
fn scope_has_marker(scope: &Node, src: &[u8]) -> bool {
    let mut cursor = scope.walk();
    for child in scope.children(&mut cursor) {
        match child.kind() {
            "preproc_include" if include_is_gtest(&child, src) => return true,
            "function_definition" if function_def_is_test_macro(&child, src) => return true,
            "namespace_definition" => {
                if let Some(body) = child.child_by_field_name("body") {
                    if scope_has_marker(&body, src) {
                        return true;
                    }
                }
            }
            // Conditional-compilation blocks hold top-level declarations directly
            // as children (the guarded include / macro / nested namespace), so
            // scanning the block node's own children is exactly the same scope.
            "preproc_ifdef" | "preproc_if" | "preproc_else" | "preproc_elif" => {
                if scope_has_marker(&child, src) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// A `preproc_include` whose `path` child names a `gtest/…` or `gmock/…` header.
/// The `path` node is a `system_lib_string` (`<gtest/gtest.h>`) or a
/// `string_literal` (`"gtest/gtest.h"`); its text still carries the delimiter,
/// so strip the single leading delimiter char before the prefix check.
fn include_is_gtest(include: &Node, src: &[u8]) -> bool {
    let Some(path) = include.child_by_field_name("path") else {
        return false;
    };
    let Ok(text) = path.utf8_text(src) else {
        return false;
    };
    // Strip the opening delimiter (`<` or `"`); the header name follows.
    let inner = text.trim_start_matches(['<', '"']);
    GTEST_INCLUDE_PREFIXES
        .iter()
        .any(|prefix| inner.starts_with(prefix))
}

/// A `function_definition` that is actually a top-level gtest macro invocation:
/// its declarator chain resolves to an `identifier` whose text is one of
/// [`TEST_MACROS`]. A normal function definition (`void f() {}`) resolves to an
/// identifier `f`, which is not in the set, so it does not match.
fn function_def_is_test_macro(func_def: &Node, src: &[u8]) -> bool {
    let Some(declarator) = func_def.child_by_field_name("declarator") else {
        return false;
    };
    if declarator.kind() != "function_declarator" {
        return false;
    }
    let Some(inner) = declarator.child_by_field_name("declarator") else {
        return false;
    };
    if inner.kind() != "identifier" {
        return false;
    }
    let Ok(name) = inner.utf8_text(src) else {
        return false;
    };
    TEST_MACROS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_marker(src: &str) -> bool {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_cpp::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(src, None).unwrap();
        detect_gtest_marker(&tree.root_node(), src.as_bytes())
    }

    #[test]
    fn system_include_is_marker() {
        assert!(has_marker("#include <gtest/gtest.h>\nvoid f() {}\n"));
    }

    #[test]
    fn quote_include_is_marker() {
        // The corpus form (leveldb, vcmi): #include "gtest/gtest.h".
        assert!(has_marker("#include \"gtest/gtest.h\"\nvoid f() {}\n"));
    }

    #[test]
    fn gmock_include_is_marker() {
        assert!(has_marker("#include \"gmock/gmock.h\"\n"));
    }

    #[test]
    fn each_test_macro_form_is_marker() {
        for macro_name in ["TEST", "TEST_F", "TEST_P", "TYPED_TEST"] {
            let src = format!("{macro_name}(Suite, Name) {{\n  EXPECT_EQ(1, 1);\n}}\n");
            assert!(has_marker(&src), "{macro_name} must be detected");
        }
    }

    #[test]
    fn test_macro_without_include_is_marker() {
        // Fixture (b): the dominant vcmi shape — TEST_F but no in-file gtest
        // include (it comes via a shared header).
        assert!(has_marker(
            "TEST_F(MySuite, DoesThing) {\n  ASSERT_TRUE(true);\n}\n"
        ));
    }

    #[test]
    fn namespace_wrapped_test_macro_is_marker() {
        // vcmi's actual idiom: the TEST_F lives inside `namespace test { … }`, so
        // it is not a direct child of the translation unit — the scan must descend
        // into namespace bodies.
        assert!(has_marker(
            "namespace test {\nusing namespace ::testing;\n\
             TEST_F(BonusSystem, Applies) {\n  EXPECT_EQ(1, 1);\n}\n}\n"
        ));
    }

    #[test]
    fn anonymous_namespace_test_macro_is_marker() {
        assert!(has_marker(
            "namespace {\nTEST(Anon, Works) {\n  ASSERT_TRUE(true);\n}\n}\n"
        ));
    }

    #[test]
    fn nested_namespace_test_macro_is_marker() {
        assert!(has_marker(
            "namespace a {\nnamespace b {\nTEST(Deep, Works) {}\n}\n}\n"
        ));
    }

    #[test]
    fn expect_call_inside_function_body_is_not_marker() {
        // A gtest assertion inside an ordinary function body is a call site, not a
        // top-level macro — the scan must NOT descend into function bodies.
        assert!(!has_marker(
            "void helper() {\n  TEST_F(x, y);\n  EXPECT_EQ(1, 1);\n}\n"
        ));
    }

    #[test]
    fn marker_in_comment_is_not_evidence() {
        // Fixture (d): a marker mentioned only in a comment/string is production.
        assert!(!has_marker(
            "// TEST(A, B) is documented here\nvoid real() {}\n"
        ));
    }

    #[test]
    fn marker_in_string_literal_is_not_evidence() {
        // Fixture (d): the include text appears only inside a string constant.
        assert!(!has_marker(
            "const char* s = \"#include <gtest/gtest.h>\";\nvoid real() {}\n"
        ));
    }

    #[test]
    fn gtest_include_inside_header_guard_is_marker() {
        // The real corpus shape (leveldb's `util/testutil.h`): a header wraps its
        // gtest include in an `#ifndef … #define … #endif` include guard, so the
        // `#include` is a child of a `preproc_ifdef`, not a direct child of the
        // translation unit. The scan MUST descend into conditional-compilation
        // blocks or every guarded gtest header is missed (review-0 item 2).
        assert!(has_marker(
            "#ifndef FOO_UTIL_H_\n#define FOO_UTIL_H_\n\
             #include \"gtest/gtest.h\"\n\
             namespace foo { class Helper {}; }\n\
             #endif  // FOO_UTIL_H_\n"
        ));
    }

    #[test]
    fn test_macro_inside_preproc_if_is_marker() {
        // A `TEST` macro guarded by a feature `#if` is still a top-level test
        // marker once the conditional block is entered.
        assert!(has_marker(
            "#if defined(ENABLE_TESTS)\nTEST(Guarded, Works) {}\n#endif\n"
        ));
    }

    #[test]
    fn plain_include_is_not_marker() {
        assert!(!has_marker(
            "#include <vector>\n#include \"db/db_impl.h\"\n"
        ));
    }

    #[test]
    fn ordinary_function_is_not_marker() {
        // The name-trap witness at the detector level: a production translation
        // unit with an ordinary function is not a test, whatever its filename.
        assert!(!has_marker(
            "int compute(int a, int b) {\n  return a + b;\n}\n"
        ));
    }
}
