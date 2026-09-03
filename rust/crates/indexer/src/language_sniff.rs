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
//! AXIS OF VARIATION: content-ambiguous source extensions — extensions whose
//!   language cannot be decided from the name alone. TWO concrete cases today:
//!   (a) `.ts`/`.mts`/`.cts` → TypeScript by extension, but a Qt Linguist
//!   translation catalog with that extension is XML (TS-LINGUIST-1); (b) `.h` →
//!   C by extension, but a C++ header is C++ (FIND-KIND-MISLABEL-1). Each
//!   ambiguity is resolved by content evidence, never by a name heuristic.
//!   Operations are fixed (one: classify); this is a plain function, NOT a trait
//!   — there is no growing set of implementations to invert behind polymorphism.
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
//! calls the table, then adjusts ONLY the content-ambiguous cases on positive
//! content evidence (the TypeScript XML downgrade; the `.h` C→C++ promotion).
//! Keeping the two apart is why the routing-table callers are untouched — the
//! extractor still routes through this same classified fact via
//! `routing::route_file_content_aware`.

use crate::routing::{detect_language, get_extension, MAX_FILE_SIZE_BYTES};

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
/// - **Content-ambiguous C header** (`.h` → `"c"` by extension; FIND-KIND-
///   MISLABEL-1):
///     - `Some(bytes)` carrying C++-only structural markers (`::`, or the
///       keywords `class`/`namespace`/`template`/`typename` as whole tokens) →
///       `Some("cpp")`. The header is C++; routing it to the C++ extractor is
///       what makes a `class` in a `.h` label as `CLASS`, not `FUNCTION`.
///     - `Some(bytes)` with no C++ markers → `Some("c")` (a genuine C header).
///     - `None` → `Some("c")`. Here the extension answer is the SAFE default:
///       both C and C++ extract, so with no content we do NOT promote to C++
///       (we never invent it), we leave the header as C — byte-identical to the
///       prior extension-only behavior. This differs from the TS case, where
///       the extension answer (`"typescript"`) was the RISKY one.
pub fn classify_file_language(rel_path: &str, content: Option<&[u8]>) -> Option<&'static str> {
    match detect_language(rel_path) {
        Some("typescript") => match content {
            // Positive XML evidence downgrades the false-positive TypeScript label.
            Some(bytes) if content_is_xml_document(bytes) => None,
            // Read, and not XML → genuine TypeScript (byte-identical to the table).
            Some(_) => Some("typescript"),
            // No content to sniff → extension is not evidence; do not assert TS.
            None => None,
        },
        // `.h` is the one ambiguous C extension: a C header OR a C++ header.
        // (`.c` is unambiguously C; `detect_language` maps BOTH to "c", so the
        // extension check is what isolates `.h`.) The C++ header case is the
        // FIND-KIND-MISLABEL-1 defect: routed to the C extractor, a C++ `class`
        // in a `.h` was misparsed and stamped `FUNCTION`.
        Some("c") if get_extension(rel_path) == ".h" => match content {
            // Positive C++ content evidence promotes the header to C++.
            Some(bytes) if content_has_cpp_markers(bytes) => Some("cpp"),
            // No C++ markers, or no content in hand → the extension's C default.
            // Unlike the TS case, the extension answer here ("c") is the SAFE,
            // non-promoted default (both C and C++ are code and both extract);
            // with no content we simply do not PROMOTE — we never invent C++.
            _ => Some("c"),
        },
        // Unambiguous extension — deterministic from the name in our model.
        other => other,
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

/// C++-only keyword markers, matched as WHOLE tokens (identifier boundaries on
/// both sides). Reserved in C++ but absent from C's grammar as constructs; their
/// presence in a `.h` is content evidence that the header is C++.
///
/// This is the documented, testable marker set ratified for FIND-KIND-MISLABEL-1
/// (operator ruling 2026-09-03, FKM1-FIX-SITE — "`class`/`namespace`/`template`/
/// `typename`/`::` and the like"). Deliberately narrow — every token here is a
/// keyword a C header would not idiomatically contain.
///
/// `operator` (the "and the like") is added on corpus evidence: vcmi's
/// `CompoundMapObjectID.h` is a C++ value-struct whose ONLY C++ signals are
/// `operator<`/`operator==` overloads and constructor initializer lists — no
/// `::`/`class`/`namespace`/`template`/`typename`. Without it that struct's
/// constructor stays on the C extractor and mislabels `FUNCTION`. `operator` is
/// a C++ keyword absent from idiomatic C (verified: 0 occurrences in OpenXcom's
/// genuine-C `Scalers/common.h`).
const CPP_MARKER_KEYWORDS: &[&[u8]] = &[
    b"class",
    b"namespace",
    b"template",
    b"typename",
    b"operator",
];

/// Does a `.h` header's content carry C++-only structural markers **in code**?
///
/// The scan runs over [`code_bytes_only`] — the content with every comment and
/// string / char literal blanked to spaces — so a marker is evidence only when
/// it appears in ACTUAL CODE, never inside a comment or a string (review-1: the
/// prior scan read raw bytes, so `::` or `class` in a comment/string promoted a
/// genuine C header). This is the "structural C++ markers only" the operator
/// ratified: structural = lexical position is code, not prose.
///
/// Two evidence forms, both CONTENT (never filename):
/// - the scope-resolution operator `::` — has no spelling in C at all, so it is
///   the strongest single signal (vcmi's `EntityIdentifiers.h` carries 200+);
/// - a [`CPP_MARKER_KEYWORDS`] keyword appearing as a whole token — identifier
///   boundaries on both sides, so `classifier` or a field named `template_id`
///   does NOT match.
///
/// BOUNDED AT THE SCAN SITE (review-1): only the first
/// [`crate::routing::MAX_FILE_SIZE_BYTES`] bytes are ever stripped + scanned,
/// enforced HERE by slicing `content` before [`code_bytes_only`] — NOT relying
/// on the caller. This matters because `classify_file_language`'s orchestrator
/// caller runs BEFORE its oversized-file skip (`orchestrator.rs`), so an
/// oversized `.h`'s full content reaches this function; without the local slice
/// the strip would allocate and scan the whole (possibly multi-MB) body only to
/// have the file skipped anyway. The slice makes the cost O(cap), independent of
/// caller ordering. It changes NO in-scope classification: every file that is
/// actually extracted is already `<= MAX_FILE_SIZE_BYTES` (it passed the skip),
/// so `content.len().min(cap) == content.len()` and the full body is scanned; an
/// oversized file is skipped regardless of the value computed here, so capping
/// its (cosmetic) classification is harmless.
///
/// Scanned over the WHOLE (capped, stripped) content: headers place these
/// markers anywhere, unlike the XML declaration which is a fixed prefix.
///
/// Residual (documented, accepted): the keywords are not RESERVED in C, so a C
/// header that uses one as a bare identifier IN CODE (`int class;`) is still a
/// false positive. Harm is bounded — the C++ grammar is a near-superset of C, so
/// such a header still extracts correctly under the C++ extractor in the common
/// case; a genuine C identifier collision (`int class;`) is rare in real
/// headers. `::`-only detection would avoid it but would miss C++ headers that
/// declare a class with no qualified name, so the fuller marker set is preferred
/// (a false positive on C keeps kinds correct; a false negative leaves a C++
/// class mislabeled). The comment/string false positives the raw-byte scan had
/// are now eliminated by the strip.
///
/// Strip residual (rare, documented): a C++ raw string literal (`R"(...)"`)
/// whose delimited body contains an embedded `"` closes the string scan early,
/// so bytes after it are re-read as code; a marker sitting in that tail could be
/// seen. A file using raw strings is itself C++, so this only ever risks an
/// extra (correct-direction) promotion, never a missed C++ header.
///
/// Known limitation (documented follow-up candidate, NOT observed in the vcmi /
/// OpenXcom / leveldb corpora after adding `operator`): a C++ value-struct whose
/// ONLY C++ signal is a constructor initializer list (`Foo() : x_(0) {}`) with
/// no `operator`, template, qualified name, or class/namespace/typename keyword
/// would still classify as C. Detecting a bare initializer list needs
/// paren/precedence context this lexical scan deliberately avoids; if such a
/// header surfaces, the honest next step is the C-extractor backstop (emit an
/// unknown/generic subtype for a construct it cannot truly classify, never
/// `FUNCTION`), tracked separately rather than widened here.
fn content_has_cpp_markers(content: &[u8]) -> bool {
    // Hard cap the inspected window at the scan site (review-1): see the doc
    // above. Every extracted file is already within the cap, so this is a no-op
    // for real classifications and bounds only the oversized-and-skipped case.
    let head = &content[..content.len().min(MAX_FILE_SIZE_BYTES)];
    let code = code_bytes_only(head);
    contains_subslice(&code, b"::")
        || CPP_MARKER_KEYWORDS
            .iter()
            .any(|kw| contains_token(&code, kw))
}

/// Lexical states of the C/C++ comment + literal stripper.
#[derive(Clone, Copy)]
enum ScanState {
    /// Ordinary code — markers here are real evidence.
    Code,
    /// Inside a `// …` line comment (until end of line).
    LineComment,
    /// Inside a `/* … */` block comment (until `*/`).
    BlockComment,
    /// Inside a `"…"` string literal (until an unescaped `"`).
    StringLit,
    /// Inside a `'…'` char literal (until an unescaped `'`).
    CharLit,
}

/// Return a copy of `content` with every comment and string / char literal
/// replaced by ASCII spaces, leaving code bytes — and therefore token
/// boundaries — intact.
///
/// Blanking to spaces (rather than deleting) preserves offsets and, crucially,
/// keeps a whitespace boundary where a comment/string used to be, so
/// [`contains_token`] cannot be fooled into fusing two identifiers across a
/// removed region. The single linear pass is bounded by the passed slice's
/// length; [`content_has_cpp_markers`] caps that slice at
/// [`crate::routing::MAX_FILE_SIZE_BYTES`] before calling, so the strip is
/// bounded by the cap regardless of the caller's file-size ordering.
///
/// Handles: `//` line comments, `/* */` block comments (nesting is not a thing
/// in C/C++), `"…"` strings and `'…'` char literals with `\`-escapes. Digit
/// separators (`1'000`) are mis-lexed as char literals, and an early-closing raw
/// string is the residual noted on [`content_has_cpp_markers`]; both only ever
/// risk a correct-direction over-promotion, never a missed C++ header.
fn code_bytes_only(content: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(content.len());
    let mut state = ScanState::Code;
    let mut i = 0;
    while i < content.len() {
        let b = content[i];
        let next = content.get(i + 1).copied();
        match state {
            ScanState::Code => match (b, next) {
                (b'/', Some(b'/')) => {
                    out.push(b' ');
                    out.push(b' ');
                    i += 2;
                    state = ScanState::LineComment;
                }
                (b'/', Some(b'*')) => {
                    out.push(b' ');
                    out.push(b' ');
                    i += 2;
                    state = ScanState::BlockComment;
                }
                (b'"', _) => {
                    out.push(b' ');
                    i += 1;
                    state = ScanState::StringLit;
                }
                (b'\'', _) => {
                    out.push(b' ');
                    i += 1;
                    state = ScanState::CharLit;
                }
                _ => {
                    out.push(b);
                    i += 1;
                }
            },
            ScanState::LineComment => {
                out.push(b' ');
                if b == b'\n' {
                    state = ScanState::Code;
                }
                i += 1;
            }
            ScanState::BlockComment => {
                if b == b'*' && next == Some(b'/') {
                    out.push(b' ');
                    out.push(b' ');
                    i += 2;
                    state = ScanState::Code;
                } else {
                    out.push(b' ');
                    i += 1;
                }
            }
            ScanState::StringLit => {
                if b == b'\\' {
                    // Escape: blank this byte and the escaped one together.
                    out.push(b' ');
                    if next.is_some() {
                        out.push(b' ');
                        i += 2;
                    } else {
                        i += 1;
                    }
                } else if b == b'"' {
                    out.push(b' ');
                    i += 1;
                    state = ScanState::Code;
                } else {
                    out.push(b' ');
                    i += 1;
                }
            }
            ScanState::CharLit => {
                if b == b'\\' {
                    out.push(b' ');
                    if next.is_some() {
                        out.push(b' ');
                        i += 2;
                    } else {
                        i += 1;
                    }
                } else if b == b'\'' {
                    out.push(b' ');
                    i += 1;
                    state = ScanState::Code;
                } else {
                    out.push(b' ');
                    i += 1;
                }
            }
        }
    }
    out
}

/// Is `haystack` containing `needle` anywhere? (Bounded by `haystack` length.)
fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// Identifier byte in the C/C++ lexical sense: `[A-Za-z0-9_]`.
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Does `token` occur in `haystack` as a WHOLE token — no identifier byte
/// immediately before or after? (Start/end of content counts as a boundary.)
fn contains_token(haystack: &[u8], token: &[u8]) -> bool {
    if token.is_empty() {
        return false;
    }
    haystack.windows(token.len()).enumerate().any(|(i, w)| {
        w == token
            && i.checked_sub(1).is_none_or(|p| !is_ident_byte(haystack[p]))
            && haystack
                .get(i + token.len())
                .is_none_or(|&b| !is_ident_byte(b))
    })
}

#[cfg(test)]
mod tests;
