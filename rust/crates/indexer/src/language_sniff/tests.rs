//! Unit tests for the content-aware file-language classifier (`super`).
//! Split out of `language_sniff.rs` (FIND-KIND-MISLABEL-1 review-2) so the
//! production module stays under the 500-line module guardrail; as a child
//! module it still reaches `super`'s crate-private items (`classify_file_language`,
//! `XML_SNIFF_CAP_BYTES`, the re-exported `MAX_FILE_SIZE_BYTES`).

use super::*;

/// The exact VCMI Qt Linguist header (verified 2026-09-01): XML decl then
/// `<!DOCTYPE TS>`.
const QT_LINGUIST_HEAD: &str =
    "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE TS>\n<TS version=\"2.1\" language=\"cs_CZ\">\n";

/// Helper: classify with readable content.
fn with(path: &str, content: &str) -> Option<&'static str> {
    classify_file_language(path, Some(content.as_bytes()))
}

// ── content-present path ─────────────────────────────────────

#[test]
fn qt_linguist_ts_is_not_typescript() {
    assert_eq!(
        with("mapeditor/translation/czech.ts", QT_LINGUIST_HEAD),
        None
    );
}

#[test]
fn real_typescript_unchanged() {
    let code = "export function greet(name: string): string {\n  return `hi ${name}`;\n}\n";
    assert_eq!(with("src/index.ts", code), Some("typescript"));
}

#[test]
fn doctype_ts_without_xml_decl_is_not_typescript() {
    // A Qt catalog that omits the `<?xml` prologue still declares `<!DOCTYPE TS`.
    assert_eq!(
        with("translation/de.ts", "<!DOCTYPE TS>\n<TS></TS>\n"),
        None
    );
}

#[test]
fn leading_bom_before_xml_decl() {
    let with_bom = format!("\u{feff}{QT_LINGUIST_HEAD}");
    assert_eq!(with("translation/fr.ts", &with_bom), None);
}

#[test]
fn leading_whitespace_before_xml_decl() {
    let padded = format!("  \n\t{QT_LINGUIST_HEAD}");
    assert_eq!(with("translation/it.ts", &padded), None);
}

#[test]
fn ts_leading_comment_stays_typescript() {
    // A TS file may open with a comment; it does not begin with an XML marker,
    // so it is untouched. (Guards against over-eager `<`-anywhere matching.)
    let code = "// header comment\nexport const x = 1;\n";
    assert_eq!(with("src/util.ts", code), Some("typescript"));
}

#[test]
fn empty_or_blank_readable_ts_stays_typescript() {
    // Empty / whitespace-only READABLE content carries no XML evidence: the
    // sniff must not claim non-code, so the extension classification stands.
    assert_eq!(with("src/empty.ts", ""), Some("typescript"));
    assert_eq!(with("src/blank.ts", "   \n  \n"), Some("typescript"));
}

#[test]
fn mts_cts_qt_content_is_not_typescript() {
    // The whole `.ts`/`.mts`/`.cts` family (all → "typescript") is sniffed.
    assert_eq!(with("a.mts", QT_LINGUIST_HEAD), None);
    assert_eq!(with("a.cts", QT_LINGUIST_HEAD), None);
}

#[test]
fn tsx_with_leading_angle_is_never_sniffed() {
    // `.tsx` maps to "tsx", not "typescript" — its JSX legitimately begins
    // with `<`, so it is never a sniff candidate and stays "tsx".
    assert_eq!(with("src/App.tsx", "<App />\n"), Some("tsx"));
}

#[test]
fn non_ts_extension_untouched_by_content() {
    // A `.py`/etc. never enters the typescript gate regardless of content.
    assert_eq!(with("x.py", QT_LINGUIST_HEAD), Some("python"));
    assert_eq!(with("README.txt", QT_LINGUIST_HEAD), None);
}

#[test]
fn sniff_is_bounded_marker_beyond_cap_not_detected() {
    // BOUND (review-1): the sniff inspects only the first XML_SNIFF_CAP_BYTES.
    // A genuine XML declaration is always at byte 0 (the XML spec forbids
    // content before it), so this case cannot occur in a real Qt catalog; the
    // test proves the cap is a HARD limit, not "bounded in practice". A `.ts`
    // padded with more leading whitespace than the cap, then `<?xml`, is NOT
    // downgraded — the marker is past the cap.
    let padded = format!(
        "{}{}",
        " ".repeat(XML_SNIFF_CAP_BYTES + 8),
        QT_LINGUIST_HEAD
    );
    assert_eq!(
        with("translation/pathological.ts", &padded),
        Some("typescript"),
        "a marker beyond the sniff cap is not inspected (bound is hard)",
    );
}

#[test]
fn sniff_is_bounded_megabyte_whitespace_terminates_o1() {
    // A pathological file that is ~1 MiB of leading whitespace before any
    // marker must be classified by inspecting only the capped prefix.
    let mut big = " ".repeat(1024 * 1024);
    big.push_str(QT_LINGUIST_HEAD);
    assert_eq!(with("translation/huge.ts", &big), Some("typescript"));
}

#[test]
fn marker_within_cap_after_short_whitespace() {
    // Whitespace shorter than the cap, then the marker → within the window →
    // downgraded to non-code.
    let padded = format!("{}{}", " ".repeat(16), QT_LINGUIST_HEAD);
    assert_eq!(with("translation/short-pad.ts", &padded), None);
}

// ── no-content path (§2.4) ───────────────────────────────────

#[test]
fn no_content_ts_family_is_none_not_typescript() {
    assert_eq!(classify_file_language("translation/czech.ts", None), None);
    assert_eq!(classify_file_language("src/index.ts", None), None);
    assert_eq!(classify_file_language("a.mts", None), None);
    assert_eq!(classify_file_language("a.cts", None), None);
}

#[test]
fn no_content_unambiguous_extensions_unchanged() {
    // Non-TS extensions are deterministic from the extension, so an
    // unreadable file keeps its classification (only TypeScript is ambiguous).
    assert_eq!(classify_file_language("src/app.py", None), Some("python"));
    assert_eq!(classify_file_language("src/main.rs", None), Some("rust"));
    assert_eq!(classify_file_language("src/App.tsx", None), Some("tsx"));
    assert_eq!(classify_file_language("a.mjs", None), Some("javascript"));
    assert_eq!(classify_file_language("Foo.java", None), Some("java"));
}

#[test]
fn no_content_unknown_extension_is_none() {
    assert_eq!(classify_file_language("README.txt", None), None);
    assert_eq!(classify_file_language("data.bin", None), None);
}

// ── distinctness of the None cases (why the pair collapsed cleanly) ──

// ── `.h` C-vs-C++ header classification (FIND-KIND-MISLABEL-1) ──

#[test]
fn cpp_header_with_class_is_cpp() {
    // The reproducing construct: a C++ `class` in a `.h` (vcmi's
    // EntityIdentifiers.h shape). Pre-fix routed to C → FUNCTION; now cpp.
    let code = "#pragma once\nclass HeroClassID : public EntityIdentifier<HeroClassID> {};\n";
    assert_eq!(with("lib/constants/EntityIdentifiers.h", code), Some("cpp"));
}

#[test]
fn h_header_with_scope_resolution_is_cpp() {
    // `::` has no C spelling — strongest single marker.
    let code = "int f() { return std::max(1, 2); }\n";
    assert_eq!(with("include/util.h", code), Some("cpp"));
}

#[test]
fn h_header_with_namespace_is_cpp() {
    assert_eq!(with("include/api.h", "namespace foo { }\n"), Some("cpp"));
}

#[test]
fn h_header_with_template_is_cpp() {
    assert_eq!(
        with("include/vec.h", "template<typename T> struct Vec {};\n"),
        Some("cpp")
    );
}

#[test]
fn value_struct_with_operator_overload_is_cpp() {
    // vcmi's CompoundMapObjectID.h shape: a C++ value-struct whose only C++
    // signal is `operator` overloading + ctor init lists — no `::`/`class`/
    // template. `operator` promotes it so the constructor labels correctly.
    let code = "struct Id {\n  Id() : a(0) {}\n  bool operator<(const Id& o) const { return a < o.a; }\n  int a;\n};\n";
    assert_eq!(
        with("lib/mapObjects/CompoundMapObjectID.h", code),
        Some("cpp")
    );
}

#[test]
fn genuine_c_static_inline_header_stays_c() {
    // OpenXcom's Scalers/common.h shape: genuine C with `static inline`
    // functions and NO C++ markers → stays C (FUNCTION labels are correct).
    let code = "#ifndef H_\n#define H_\n#include <stdint.h>\nstatic inline uint32_t rgb_to_yuv(uint32_t c) { return c; }\n#endif\n";
    assert_eq!(with("src/Engine/Scalers/common.h", code), Some("c"));
}

#[test]
fn plain_c_header_stays_c() {
    // A genuine C header with no C++ markers is unchanged.
    let code =
        "#ifndef FOO_H\n#define FOO_H\nint add(int a, int b);\nstruct point { int x; };\n#endif\n";
    assert_eq!(with("include/foo.h", code), Some("c"));
}

#[test]
fn c_source_file_never_promoted() {
    // `.c` is unambiguously C: even with `::` in a comment it stays C, because
    // only `.h` is the ambiguous extension (a `.c` is C by definition).
    assert_eq!(with("src/impl.c", "// see Foo::bar\nint x;\n"), Some("c"));
}

#[test]
fn class_substring_does_not_match() {
    // Whole-token match: `classifier`/`classify` are C identifiers, not the
    // `class` keyword. No other marker here → stays C.
    let code = "int classifier(int classify_mode);\n";
    assert_eq!(with("include/classify.h", code), Some("c"));
}

#[test]
fn template_as_field_name_does_not_match() {
    // A C header with a field literally named `template_id` must not match.
    let code = "struct rec { int template_id; int namespace_id; };\n";
    assert_eq!(with("include/rec.h", code), Some("c"));
}

#[test]
fn cpp_marker_beyond_size_cap_not_detected() {
    // BOUND (review-1): the C++ marker scan inspects only the first
    // MAX_FILE_SIZE_BYTES, enforced at the scan site (not by the caller's
    // later oversized-file skip). A `.h` that is more than a cap's worth of
    // marker-free bytes followed by a real `class` keyword PAST the cap is
    // NOT promoted — the marker is outside the inspected window. (Such a file
    // is also oversized and skipped by the orchestrator, so its cosmetic
    // classification staying `c` is correct; this proves the bound is hard,
    // not "bounded in practice".)
    let mut big = "a".repeat(MAX_FILE_SIZE_BYTES);
    big.push_str(" class Foo {};\n");
    assert_eq!(
        classify_file_language("include/beyond_cap.h", Some(big.as_bytes())),
        Some("c"),
        "a C++ marker beyond the size cap must not promote (bound is hard)",
    );
}

#[test]
fn cpp_marker_within_cap_detected_despite_huge_tail() {
    // Positive control for the bound: a real `class` WITHIN the cap promotes
    // even when followed by a cap's worth of trailing bytes — proves the cap
    // suppresses only the region past it, not the inspected prefix.
    let mut big = String::from("#pragma once\nclass Bar {};\n");
    big.push_str(&"a".repeat(MAX_FILE_SIZE_BYTES));
    assert_eq!(
        classify_file_language("include/within_cap.h", Some(big.as_bytes())),
        Some("cpp"),
        "a C++ marker within the cap must still promote",
    );
}

#[test]
fn no_content_h_stays_c_not_promoted() {
    // §2.4 analogue: with no content we do not PROMOTE — the safe C default
    // holds (byte-identical to the prior extension-only behavior).
    assert_eq!(classify_file_language("include/foo.h", None), Some("c"));
    assert_eq!(classify_file_language("src/impl.c", None), Some("c"));
}

#[test]
fn hpp_stays_cpp_regardless_of_content() {
    // `.hpp`/`.hxx`/`.cc`/`.cxx` are unambiguous C++ by extension; they never
    // enter the `.h` sniff and are C++ even with no markers.
    assert_eq!(with("include/foo.hpp", "int x;\n"), Some("cpp"));
    assert_eq!(classify_file_language("include/foo.hpp", None), Some("cpp"));
}

// ── markers must be CODE, not comments/strings (review-1 item 2) ──

#[test]
fn c_header_with_all_markers_only_in_comments_stays_c() {
    // Every supported marker appears — but only inside a block comment and a
    // line comment. A genuine C header MUST stay C: comment prose is not
    // structural C++ evidence.
    let code = "/* mentions class, namespace, template, typename, operator and Foo::bar */\n\
                // class Widget : public Base  (also a comment)\n\
                int add(int a, int b);\n";
    assert_eq!(with("include/c_comment.h", code), Some("c"));
}

#[test]
fn c_header_with_all_markers_only_in_string_stays_c() {
    // Every supported marker inside a string literal — a diagnostic message,
    // not code. Must stay C.
    let code = "const char *usage =\n  \"use class / namespace / template / typename / operator / A::b\";\n\
                int parse(const char *s);\n";
    assert_eq!(with("include/c_string.h", code), Some("c"));
}

#[test]
fn c_header_with_scope_resolution_only_in_comment_stays_c() {
    // The strongest single marker `::` inside a comment must NOT promote.
    let code = "// resolves to std::vector when compiled as C++\nint use(int n);\n";
    assert_eq!(with("include/c_scope_comment.h", code), Some("c"));
}

#[test]
fn cpp_marker_in_code_promotes_despite_comment_and_string_noise() {
    // Positive control: a REAL `::` in code still promotes even when comments
    // and strings are present — proves the strip does not over-suppress code.
    let code = "// leading comment, no markers\nconst char *m = \"plain text\";\n\
                int f() { return std::max(1, 2); }\n";
    assert_eq!(with("include/mix.h", code), Some("cpp"));
}

#[test]
fn char_literal_with_quote_does_not_desync_marker_scan() {
    // A char literal holding a double-quote must not throw the lexer into a
    // spurious string state and hide a following real `class` keyword.
    let code = "char q = '\"';\nclass Foo {};\n";
    assert_eq!(with("include/q.h", code), Some("cpp"));
}

#[test]
fn escaped_quote_in_string_does_not_end_it_early() {
    // A `\"` inside a string must not terminate it; the `class` token that
    // follows is still inside the string and must NOT promote.
    let code = "const char *s = \"a \\\" b class namespace\";\nint g(int x);\n";
    assert_eq!(with("include/esc.h", code), Some("c"));
}

#[test]
fn empty_readable_ts_differs_from_no_content_ts() {
    // A legitimately empty, READABLE `.ts` is TypeScript; an UNREADABLE `.ts`
    // is unknown. The single `Option` shape distinguishes them: `Some(b"")`
    // vs `None`. (This is why one function suffices where a pair was rejected.)
    assert_eq!(
        classify_file_language("src/e.ts", Some(b"")),
        Some("typescript")
    );
    assert_eq!(classify_file_language("src/e.ts", None), None);
}
