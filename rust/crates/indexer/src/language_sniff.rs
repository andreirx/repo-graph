//! Content-aware file-language classification (TS-LINGUIST-1).
//!
//! ── Abstraction one-liner ────────────────────────────────────
//! WHAT: one public function [`classify_file_language`] plus a private bounded
//!   XML content sniff. It answers "what code language do we PERSIST for this
//!   file?", given the path AND — when the indexer holds it — the file's bytes.
//! CURRENT USERS (2, concrete): `crate::orchestrator` (primary index write +
//!   refresh copy-forward fallback) and `repo_graph_repo_index::compose` (the
//!   readable-file dependency-signal gate + the read-failure persist). Both
//!   persist `files.language` / language-derived signals, so both must classify
//!   from the SAME fact — this module is that one fact.
//! AXIS OF VARIATION: a single content-ambiguous extension family
//!   (`.ts`/`.mts`/`.cts` → TypeScript by extension, but a Qt Linguist
//!   translation catalog with that extension is XML). The ambiguity is resolved
//!   by BOUNDED content evidence, never by a name heuristic. Operations are
//!   fixed (one: classify); this is a plain function, NOT a trait — there is no
//!   growing set of implementations to invert behind polymorphism.
//! REJECTED SIMPLER: (a) the `detect_language_with_content` / `_no_content`
//!   PAIR — two functions for ONE operation, which can drift; collapsed here to
//!   one function whose `content: Option<&[u8]>` parameter IS the single shape
//!   (`None` = no content available). (b) inlining the sniff into
//!   `routing.rs` — that file is already over the 500-line guardrail (784 lines
//!   before this slice); a dedicated module keeps it from growing.
//!
//! ── Why this is separate from `routing::detect_language` ─────
//! `routing::detect_language(path)` is a DIFFERENT operation: the pure
//! extension → language ROUTING TABLE (which extractor runs; which files an npm
//! / Cargo / Gradle module OWNS). Its callers do set-membership on the
//! extension and legitimately want `.ts → typescript` with no content in hand.
//! This module is the FILE-FACT classifier layered on top of that table: it
//! calls the table, then downgrades ONLY the content-ambiguous TypeScript case
//! on positive content evidence. Keeping the two apart is why the routing-table
//! callers are untouched by this slice.

use crate::routing::detect_language;

/// Classify the language we PERSIST for a file, using content evidence when the
/// caller holds it.
///
/// `content` is the single shape for "do we have the bytes?":
/// - `Some(bytes)` — the file was read; the bytes decide the ambiguous case.
/// - `None` — no content available (read failure, or a copy-forward with no
///   stored fact). There is nothing to sniff, so the ambiguous case cannot be
///   asserted.
///
/// Behavior, by extension class:
/// - **Unambiguous extension** (`.rs`, `.py`, `.tsx`, `.js`, …): returns the
///   routing-table language regardless of `content`. These extensions are a
///   deterministic function of the name in our model; content cannot change
///   them, and `.tsx` (whose JSX legitimately begins with `<`) is deliberately
///   NOT in the sniffed family.
/// - **Content-ambiguous family** (`.ts`/`.mts`/`.cts` → `"typescript"`):
///     - `Some(bytes)` beginning (after an optional BOM + leading whitespace)
///       with an XML declaration or `<!DOCTYPE TS` → `None`. This is a Qt
///       Linguist / XML catalog, not TypeScript. `None` is the schema's
///       EXISTING "not one of our code languages" value (config files carry the
///       same `None`); no new language token is introduced.
///     - `Some(bytes)` that is NOT an XML document → `Some("typescript")`. A
///       genuine `.ts` cannot begin with `<?xml` / `<!DOCTYPE` (not valid
///       TypeScript), so real TypeScript is classified byte-identically to
///       [`detect_language`].
///     - `None` → `None`. The extension alone is NOT evidence of TypeScript (it
///       could be the XML catalog), so with nothing to sniff we do not assert a
///       language rather than silently defaulting to `"typescript"`
///       (TS-LINGUIST-1 §2.4). The read failure itself is surfaced separately.
///
/// The single keyword `"typescript"` is the one source of "which family is
/// content-ambiguous"; a future second ambiguous extension changes only this
/// one match.
pub fn classify_file_language(rel_path: &str, content: Option<&[u8]>) -> Option<&'static str> {
    let extension_lang = detect_language(rel_path);
    if extension_lang != Some("typescript") {
        // Not the ambiguous family — deterministic from the extension.
        return extension_lang;
    }
    match content {
        // Positive XML evidence downgrades the false-positive TypeScript label.
        Some(bytes) if content_is_xml_document(bytes) => None,
        // Read, and not XML → genuine TypeScript (byte-identical to the table).
        Some(_) => Some("typescript"),
        // No content to sniff → extension is not evidence; do not assert TS.
        None => None,
    }
}

/// Maximum number of leading bytes the XML sniff inspects. A well-formed XML
/// declaration must be the very first content (only an optional BOM and
/// insignificant whitespace may precede it), so any genuine `<?xml` / `<!DOCTYPE
/// TS` marker sits within the first few bytes. This HARD cap is what makes the
/// sniff bounded: even a pathological `.ts` that is megabytes of leading
/// whitespace (or has no marker at all) is inspected in O(1), not O(file size).
const XML_SNIFF_CAP_BYTES: usize = 256;
/// UTF-8 byte sequence of the byte-order mark (U+FEFF).
const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// Bounded content sniff: does `content` begin — after an optional UTF-8 BOM and
/// leading whitespace — with an XML declaration (`<?xml`) or a Qt Linguist
/// document-type marker (`<!DOCTYPE TS`)?
///
/// BOUNDED: only the first [`XML_SNIFF_CAP_BYTES`] bytes are ever examined; the
/// whitespace skip and prefix compare both operate on that fixed-size window, so
/// the cost is independent of file size (review-1 fix — the prior `trim_start()`
/// scanned an unbounded leading-whitespace run). Working on the raw byte prefix
/// is safe here: the BOM, every ASCII-whitespace byte that may legally precede
/// an XML declaration, and both marker prefixes are all ASCII; slicing a `&[u8]`
/// at a fixed offset cannot panic on a char boundary the way slicing a `&str`
/// would.
///
/// Empty / whitespace-only / marker-less content returns `false`, leaving the
/// extension classification intact — the sniff only ever downgrades on POSITIVE
/// evidence, never defaults anything to a language.
fn content_is_xml_document(content: &[u8]) -> bool {
    let head = &content[..content.len().min(XML_SNIFF_CAP_BYTES)];
    let head = head.strip_prefix(&UTF8_BOM).unwrap_or(head);
    let head = head.trim_ascii_start();
    head.starts_with(b"<?xml") || head.starts_with(b"<!DOCTYPE TS")
}

#[cfg(test)]
mod tests {
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
}
