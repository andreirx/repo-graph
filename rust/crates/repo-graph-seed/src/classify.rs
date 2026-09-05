//! Structural per-CHUNK classification (SEED-CHUNK-2): test-ness and
//! declaration-vs-implementation, computed from the file text the embed pass already
//! reads. STANDING HONESTY RULE 2 backing: **structural markers ONLY** — never a
//! filename or function-name rule. Language is chosen by extension solely to pick
//! WHICH structural markers to look for; the classification itself is always a
//! structural fact (an attribute, an enclosing call/mod, a body brace), never the
//! name or the extension.
//!
//! The Rust extractor puts attributes as PREV-SIBLINGS of the item (so a symbol's
//! stored span does NOT contain its own `#[test]`) and sets `parent_node_uid = None`
//! for impl methods / free functions (so the parent chain is unavailable in storage).
//! The only reliable source of the structural evidence is therefore the file text —
//! which the pass has. These functions operate on that text.
//!
//! Abstraction one-liner: pure functions `structural_is_test` / `is_declaration` +
//! the per-file `TestRegions` scan; concrete user: `pass::build_store`; axis: none
//! (operations fixed, one caller — plain functions, no trait); rejected simpler:
//! extractor-side per-symbol markers (cross-language surgery outside the frozen seed
//! slice) and `parent_node_uid` walking (unpopulated for the exact target symbols).

/// The language family of a chunk's file. Chosen by extension ONLY to select the
/// structural marker set — every classification decision below is structural.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkLang {
    Rust,
    /// TypeScript / JavaScript (+ TSX/JSX).
    TsJs,
    /// Any other language — no per-symbol structural test rule (the file fact stands).
    Other,
}

/// Pick the marker set from the path extension (never a classification, only a parser
/// selector — see the module doc).
pub fn lang_for_path(path: &str) -> ChunkLang {
    let lower = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    match ext {
        "rs" => ChunkLang::Rust,
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "mts" | "cts" => ChunkLang::TsJs,
        _ => ChunkLang::Other,
    }
}

/// Detect a Rust raw-string opener (`r"…"`, `r#"…"#`, `br#"…"#`, …) starting at byte
/// `i`, given that `i` is at a token boundary. Returns `(prefix_len, hash_count)` where
/// `prefix_len` is the byte length of the `[b]r#*"` opener (through the quote). `None`
/// when no raw-string opener starts here. The optional leading `b` covers raw BYTE
/// strings; plain `"…"` (no `r`) is handled by the ordinary `Str` path, not here.
fn raw_string_opener(bytes: &[u8], i: usize) -> Option<(usize, usize)> {
    let mut j = i;
    if j < bytes.len() && bytes[j] == b'b' {
        j += 1; // raw byte string: br"…"
    }
    if j < bytes.len() && bytes[j] == b'r' {
        j += 1;
    } else {
        return None;
    }
    let mut hashes = 0usize;
    while j < bytes.len() && bytes[j] == b'#' {
        hashes += 1;
        j += 1;
    }
    if j < bytes.len() && bytes[j] == b'"' {
        Some((j + 1 - i, hashes))
    } else {
        None
    }
}

/// Is `b` an identifier byte (so a preceding one means an `r`/`b`/backtick is NOT a
/// literal opener but part of a longer identifier — e.g. `four` must not start a raw
/// string at its trailing `r`)? ASCII-only is sufficient: a raw-string/template opener
/// is always ASCII, and a UTF-8 continuation byte (≥ 0x80) is conservatively treated as
/// "identifier" so a multibyte char never precedes a spuriously-detected opener.
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

/// Blank the contents of `//` line comments, `/* */` block comments, and string/char
/// literals (ordinary `"…"`/`'…'`, Rust raw strings `r#"…"#`, and TS/JS template
/// literals `` `…` ``) with spaces, preserving line structure and structural delimiters —
/// so a brace/keyword/attribute inside a comment or string is never mistaken for code. A
/// single pass over the whole file. Not a full lexer (no nested block comments beyond
/// depth tracking, template `${…}` interpolations are blanked WHOLE rather than
/// re-lexed): enough to keep the brace/attribute scan honest for discovery-grade
/// classification (VISION 80/20). Because it only ever affects PROMOTE-ONLY test evidence
/// and the decl body-brace check, a miss degrades to the file fact / not-a-decl, never a
/// false demotion — review-2 item 3: literal text (a raw string or template holding fake
/// `#[cfg(test)] mod {` / `describe( {`) can no longer open a test region or a body.
fn sanitize(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    #[derive(PartialEq)]
    enum S {
        Code,
        Line,
        Block,
        Str(u8),       // the delimiter byte (" or ')
        RawStr(usize), // Rust raw string; closes on `"` followed by this many `#`
        Template,      // TS/JS backtick template literal
    }
    let mut state = S::Code;
    while i < bytes.len() {
        let b = bytes[i];
        match state {
            S::Code => {
                // A raw-string / template opener only starts at a token boundary, so a
                // trailing `r`/`b`/backtick inside a longer identifier is never mistaken
                // for a literal opener.
                let at_boundary = i == 0 || !is_ident_byte(bytes[i - 1]);
                // A raw-string opener only if we are at a token boundary on an `r`/`b`.
                let raw_open = if at_boundary && (b == b'r' || b == b'b') {
                    raw_string_opener(bytes, i)
                } else {
                    None
                };
                if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    state = S::Line;
                    out.push(' ');
                    out.push(' ');
                    i += 2;
                } else if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    state = S::Block;
                    out.push(' ');
                    out.push(' ');
                    i += 2;
                } else if let Some((prefix_len, hashes)) = raw_open {
                    // Enter the raw string; blank the whole opener but keep newline count
                    // (openers are single-line, so just push spaces for its bytes).
                    for _ in 0..prefix_len {
                        out.push(' ');
                    }
                    state = S::RawStr(hashes);
                    i += prefix_len;
                } else if b == b'`' {
                    // A backtick is NEVER an identifier byte, so — unlike `r`/`b`, which
                    // can be the tail of a longer identifier — it ALWAYS opens a template
                    // literal, INCLUDING a TAGGED template `tag`…`` where an identifier byte
                    // precedes the backtick (review-3 item 2: those contents are string data,
                    // not code, and must never promote a chunk). No boundary check.
                    state = S::Template;
                    out.push('`'); // keep the opening delimiter (structural, like " )
                    i += 1;
                } else if b == b'"' || b == b'\'' {
                    state = S::Str(b);
                    out.push(b as char); // keep the opening delimiter
                    i += 1;
                } else {
                    // Preserve the byte (multibyte UTF-8 passes through unchanged: a
                    // continuation byte is never one of the ASCII markers above).
                    out.push(b as char);
                    i += 1;
                }
            }
            S::RawStr(hashes) => {
                // Close only on `"` followed by exactly `hashes` `#` (raw strings have no
                // escape processing — `\` is literal). Everything else is blanked.
                if b == b'"'
                    && bytes
                        .get(i + 1..i + 1 + hashes)
                        .is_some_and(|h| h.iter().all(|&c| c == b'#'))
                {
                    for _ in 0..(1 + hashes) {
                        out.push(' ');
                    }
                    state = S::Code;
                    i += 1 + hashes;
                } else {
                    out.push(if b == b'\n' { '\n' } else { ' ' });
                    i += 1;
                }
            }
            S::Template => {
                // Backtick template literal. `\` escapes the next byte (incl. a backtick);
                // `${…}` interpolations are blanked whole (not re-lexed) — a conservative
                // miss, never a false promotion. Closes on an unescaped backtick.
                if b == b'\\' && i + 1 < bytes.len() {
                    out.push(' ');
                    out.push(if bytes[i + 1] == b'\n' { '\n' } else { ' ' });
                    i += 2;
                } else if b == b'`' {
                    state = S::Code;
                    out.push('`'); // keep the closing delimiter
                    i += 1;
                } else {
                    out.push(if b == b'\n' { '\n' } else { ' ' });
                    i += 1;
                }
            }
            S::Line => {
                if b == b'\n' {
                    state = S::Code;
                    out.push('\n');
                } else {
                    out.push(' ');
                }
                i += 1;
            }
            S::Block => {
                if b == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    state = S::Code;
                    out.push(' ');
                    out.push(' ');
                    i += 2;
                } else {
                    out.push(if b == b'\n' { '\n' } else { ' ' });
                    i += 1;
                }
            }
            S::Str(delim) => {
                if b == b'\\' && i + 1 < bytes.len() {
                    // Escape: blank both the backslash and the escaped byte (keeps a
                    // `\"` from closing the string). Preserve a newline for line counts.
                    out.push(' ');
                    out.push(if bytes[i + 1] == b'\n' { '\n' } else { ' ' });
                    i += 2;
                } else if b == delim {
                    state = S::Code;
                    out.push(b as char); // keep the closing delimiter
                    i += 1;
                } else {
                    out.push(if b == b'\n' { '\n' } else { ' ' });
                    i += 1;
                }
            }
        }
    }
    out
}

/// A parsed Rust `cfg(...)` predicate, distinguishing ONLY the `test` cfg atom from
/// every other atom — enough to decide whether the predicate's truth REQUIRES `test`.
/// `Other` covers unrelated atoms (`unix`, `feature = "…"`, custom names, name=value).
#[derive(Debug)]
enum CfgPred {
    Test,
    Other,
    All(Vec<CfgPred>),
    Any(Vec<CfgPred>),
    Not(Box<CfgPred>),
}

/// Parse one cfg predicate from `b` starting at `*p` (recursive descent). Advances `*p`
/// past the parsed predicate. Robust to malformed input: an unparseable fragment
/// degrades to [`CfgPred::Other`] (never falsely classified as test — promote-only).
fn parse_cfg_pred(b: &[u8], p: &mut usize) -> CfgPred {
    let skip_ws = |b: &[u8], p: &mut usize| {
        while *p < b.len() && (b[*p] as char).is_whitespace() {
            *p += 1;
        }
    };
    skip_ws(b, p);
    // Read an identifier (combinator name or atom name).
    let start = *p;
    while *p < b.len() {
        let c = b[*p] as char;
        if c.is_alphanumeric() || c == '_' {
            *p += 1;
        } else {
            break;
        }
    }
    let ident = &b[start..*p];
    skip_ws(b, p);
    if *p < b.len() && b[*p] == b'(' {
        // Combinator: all(...) / any(...) / not(...).
        *p += 1; // consume '('
        let mut children = Vec::new();
        loop {
            skip_ws(b, p);
            if *p >= b.len() {
                break;
            }
            if b[*p] == b')' {
                *p += 1;
                break;
            }
            if b[*p] == b',' {
                *p += 1;
                continue;
            }
            let before = *p;
            children.push(parse_cfg_pred(b, p));
            if *p == before {
                *p += 1; // guarantee progress on unparseable input
            }
        }
        match ident {
            b"all" => CfgPred::All(children),
            b"any" => CfgPred::Any(children),
            b"not" => children
                .into_iter()
                .next()
                .map(|c| CfgPred::Not(Box::new(c)))
                .unwrap_or(CfgPred::Other),
            _ => CfgPred::Other, // unknown combinator name
        }
    } else if *p < b.len() && b[*p] == b'=' {
        // `name = value` atom — consume the value up to this level's `,`/`)`.
        *p += 1;
        while *p < b.len() && b[*p] != b',' && b[*p] != b')' {
            *p += 1;
        }
        CfgPred::Other
    } else if ident == b"test" {
        CfgPred::Test
    } else {
        CfgPred::Other
    }
}

/// Is `pred` false whenever the `test` cfg is OFF (test=false), for EVERY assignment of
/// the other atoms? That is the exact "the code is compiled only under test" property.
fn unsat_when_no_test(pred: &CfgPred) -> bool {
    match pred {
        CfgPred::Test => true, // test=false ⇒ atom false ⇒ always false
        CfgPred::Other => false,
        CfgPred::All(xs) => xs.iter().any(unsat_when_no_test), // any always-false ⇒ ∧ false
        CfgPred::Any(xs) => xs.iter().all(unsat_when_no_test), // ∨ false iff all false
        CfgPred::Not(x) => taut_when_no_test(x),               // ¬x false iff x always true
    }
}

/// Is `pred` true whenever the `test` cfg is OFF, for EVERY assignment of the other
/// atoms? (The dual used to evaluate `not(...)` correctly.)
fn taut_when_no_test(pred: &CfgPred) -> bool {
    match pred {
        CfgPred::Test => false, // test=false ⇒ atom false, never always-true
        CfgPred::Other => false,
        CfgPred::All(xs) => xs.iter().all(taut_when_no_test),
        CfgPred::Any(xs) => xs.iter().any(taut_when_no_test),
        CfgPred::Not(x) => unsat_when_no_test(x),
    }
}

/// True iff a `cfg(...)` attribute inner (e.g. `cfg(test)`, `cfg(all(test, unix))`)
/// gates code that is compiled ONLY under test — its predicate is false whenever
/// `test` is off. Correctly REJECTS `cfg(not(test))`, `cfg(feature = "test_helpers")`,
/// and `cfg(any(test, …))` (satisfiable without `test`) — the reviewer honesty fix:
/// never demote production evidence on a cfg that merely mentions the text "test".
fn cfg_requires_test(cfg_attr_inner: &str) -> bool {
    let Some(pred) = cfg_attr_inner
        .trim()
        .strip_prefix("cfg(")
        .and_then(|s| s.strip_suffix(')'))
    else {
        return false;
    };
    let bytes = pred.as_bytes();
    let mut pos = 0usize;
    unsat_when_no_test(&parse_cfg_pred(bytes, &mut pos))
}

/// Is a trimmed attribute inner (the text between `#[` and `]`) a Rust test marker?
/// `#[test]`, runner variants ending `::test` (e.g. `#[tokio::test]`), or a
/// `cfg`-gate that requires `test` ([`cfg_requires_test`]) — the spec §2.1 set.
fn attr_inner_is_test(inner: &str) -> bool {
    let inner = inner.trim();
    inner == "test"
        || inner.ends_with("::test")
        || (inner.starts_with("cfg(") && cfg_requires_test(inner))
}

/// Scan the CONTIGUOUS leading `#[...]` attribute groups at the start of ONE line and
/// return true if any is a test marker. `cfg_only` restricts to cfg-gates (for `mod`
/// lines, which are never `#[test]`); otherwise the full item marker set applies.
/// Stops at the first non-attribute token (the item keyword). This captures the
/// SAME-LINE forms the preceding-lines scan misses: `#[test] fn x() {}` and
/// `#[cfg(test)] mod tests { … }` (reviewer edge-case fix).
fn leading_line_test_attr(line: &str, cfg_only: bool) -> bool {
    let mut rest = line.trim_start();
    while let Some(after_open) = rest.strip_prefix("#[") {
        let Some(close) = after_open.find(']') else {
            break;
        };
        let inner = after_open[..close].trim();
        let hit = if cfg_only {
            inner.starts_with("cfg(") && cfg_requires_test(inner)
        } else {
            attr_inner_is_test(inner)
        };
        if hit {
            return true;
        }
        rest = after_open[close + 1..].trim_start();
    }
    false
}

/// 1-indexed inclusive line ranges that are structurally inside a test region — Rust
/// `#[cfg(test)] mod { … }` bodies, or TS/JS `describe(`/`it(`/`test(` callback bodies.
/// A symbol whose start line falls in any range is test (spec §2.1 "enclosing").
#[derive(Debug, Default, Clone)]
pub struct TestRegions {
    ranges: Vec<(usize, usize)>,
}

impl TestRegions {
    /// Does 1-indexed `line` fall inside any test region?
    pub fn contains(&self, line: usize) -> bool {
        self.ranges.iter().any(|(s, e)| line >= *s && line <= *e)
    }
}

/// Compute the file's test regions once (spec §2.1 enclosing evidence). Rust:
/// `#[cfg(test)] mod NAME { … }` bodies. TS/JS: `describe(`/`it(`/`test(` blocks.
/// Other: none. Operates on the SANITIZED text so braces/keywords in comments and
/// strings never open a false region.
pub fn compute_test_regions(lang: ChunkLang, src: &str) -> TestRegions {
    match lang {
        ChunkLang::Rust => rust_test_regions(src),
        ChunkLang::TsJs => tsjs_test_regions(src),
        ChunkLang::Other => TestRegions::default(),
    }
}

/// Brace-match forward from the FIRST `{` at or after byte offset `from` in the
/// sanitized text, returning (open_line, close_line) 1-indexed inclusive, or `None`
/// if no balanced body is found. Lines are 1-indexed by counting `\n` up to each
/// offset (the sanitized text preserves newlines).
fn brace_region(sane: &str, from: usize, line_at: &[usize]) -> Option<(usize, usize)> {
    let bytes = sane.as_bytes();
    let mut i = from;
    while i < bytes.len() && bytes[i] != b'{' {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let open_line = line_at[i];
    let mut depth = 0i32;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((open_line, line_at[i]));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Match the `(` at byte offset `open` in the sanitized text to its closing `)`,
/// returning the `)`'s byte offset, or `None` if unbalanced before end-of-input. Braces
/// and quotes inside are already blanked by [`sanitize`], so only parens count. Used to
/// bound a test-call's callback-body search to WITHIN the call's own arguments (review-3
/// item 1): `describe("x");` has NO `{` before its `)`, so it must not adopt a LATER
/// unrelated `{` (a following production function's body) as its region.
fn matching_paren(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// A per-byte 1-indexed line number table for the sanitized text.
fn line_table(sane: &str) -> Vec<usize> {
    let mut table = Vec::with_capacity(sane.len() + 1);
    let mut line = 1usize;
    for b in sane.bytes() {
        table.push(line);
        if b == b'\n' {
            line += 1;
        }
    }
    table.push(line); // sentinel for an offset == len
    table
}

fn rust_test_regions(src: &str) -> TestRegions {
    let sane = sanitize(src);
    let line_at = line_table(&sane);
    let sane_lines: Vec<&str> = sane.lines().collect();
    let mut ranges = Vec::new();

    // Byte offset of the start of each 1-indexed line, so a `mod` found by line index
    // maps back to a byte offset for the brace scan.
    let mut line_start_off = Vec::with_capacity(sane_lines.len() + 1);
    {
        let mut off = 0usize;
        for l in sane.split_inclusive('\n') {
            line_start_off.push(off);
            off += l.len();
        }
        line_start_off.push(off);
    }

    for (idx, line) in sane_lines.iter().enumerate() {
        // A `mod NAME {`-style declaration whose contiguous preceding attribute lines
        // include a `cfg(test)` gate. `mod` must be a standalone keyword (word-bounded).
        if !line_has_mod_keyword(line) {
            continue;
        }
        // The `#[cfg(test)]` gate may sit on the PRECEDING attribute lines, or inline on
        // the `mod` line itself (`#[cfg(test)] mod tests { … }` — the same-line form the
        // preceding-lines scan alone would miss).
        if !leading_line_test_attr(line, true) && !preceding_attrs_have_cfg_test(&sane_lines, idx) {
            continue;
        }
        let from = line_start_off.get(idx).copied().unwrap_or(0);
        if let Some((open, close)) = brace_region(&sane, from, &line_at) {
            ranges.push((open, close));
        }
    }
    TestRegions { ranges }
}

/// Does the sanitized line contain a standalone `mod` keyword (module declaration)?
fn line_has_mod_keyword(line: &str) -> bool {
    let mut rest = line;
    while let Some(pos) = rest.find("mod") {
        let before = rest[..pos].chars().next_back();
        let after = rest[pos + 3..].chars().next();
        let word_start = before.is_none_or(|c| !c.is_alphanumeric() && c != '_');
        let word_end = after.is_none_or(|c| !c.is_alphanumeric() && c != '_');
        if word_start && word_end {
            return true;
        }
        rest = &rest[pos + 3..];
    }
    false
}

/// Walk contiguous attribute/doc lines immediately above `idx` (1-indexed line =
/// `idx+1`); return true if any is a `cfg(test)` gate. Stops at the first line that is
/// not an attribute/doc/blank (the attribute block boundary).
fn preceding_attrs_have_cfg_test(lines: &[&str], idx: usize) -> bool {
    let mut j = idx;
    while j > 0 {
        j -= 1;
        let t = lines[j].trim();
        if t.is_empty() {
            continue; // blank lines may separate attrs from a doc block; keep scanning
        }
        if t.starts_with("#[") {
            let inner = t
                .strip_prefix("#[")
                .and_then(|s| s.strip_suffix(']'))
                .unwrap_or("")
                .trim();
            if inner.starts_with("cfg(") && cfg_requires_test(inner) {
                return true;
            }
            continue; // another attribute — keep walking up
        }
        if t.starts_with("///") || t.starts_with("//!") || t.starts_with("//") {
            continue; // doc / comment line inside the attribute block
        }
        break; // real code above — end of the attribute block
    }
    false
}

fn tsjs_test_regions(src: &str) -> TestRegions {
    let sane = sanitize(src);
    let line_at = line_table(&sane);
    let bytes = sane.as_bytes();
    let mut ranges = Vec::new();
    for kw in ["describe", "it", "test"] {
        let mut search_from = 0usize;
        while let Some(rel) = sane[search_from..].find(kw) {
            let pos = search_from + rel;
            search_from = pos + kw.len();
            // Word boundary before the keyword.
            let before = sane[..pos].chars().next_back();
            if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.') {
                continue;
            }
            // The next non-space char must be `(` — a call, not a bare identifier.
            let mut k = pos + kw.len();
            while k < bytes.len() && (bytes[k] == b' ' || bytes[k] == b'\t') {
                k += 1;
            }
            if k >= bytes.len() || bytes[k] != b'(' {
                continue;
            }
            // The test call's callback body brace lives INSIDE the call's parentheses
            // (`describe("x", () => { … })`). Bound the body search to the matching `)`
            // (review-3 item 1): a call with NO `{` before its close paren
            // (`describe("x");`) has no callback body here and must NOT adopt a later,
            // unrelated `{` (e.g. a following `function production(){}`).
            let Some(close_paren) = matching_paren(bytes, k) else {
                continue;
            };
            let mut brace = k;
            while brace < close_paren && bytes[brace] != b'{' {
                brace += 1;
            }
            if brace >= close_paren {
                continue; // no callback body inside this call
            }
            if let Some((open, close)) = brace_region(&sane, brace, &line_at) {
                ranges.push((open, close));
            }
        }
    }
    TestRegions { ranges }
}

/// Structural test evidence for ONE symbol chunk (spec §2.1). PROMOTE-ONLY: the caller
/// ORs this with the file fact, so a `false` here never demotes a file-test chunk.
///
/// - Rust: a `#[test]`/`#[cfg(test)]`-family attribute on the item (scanned from the
///   contiguous attribute block immediately above the symbol's start line, which is
///   where the extractor's prev-sibling attributes live), OR the start line falling
///   inside a `#[cfg(test)] mod` body (`regions`).
/// - TS/JS: the start line falling inside a `describe(`/`it(`/`test(` block (`regions`).
/// - Other: no per-symbol rule (`false`) — the file fact stands.
///
/// `file_lines` is the file split into lines (0-indexed slice); `line_start` is the
/// symbol's 1-indexed start line.
pub fn structural_is_test(
    lang: ChunkLang,
    file_lines: &[&str],
    line_start: usize,
    regions: &TestRegions,
) -> bool {
    if line_start == 0 {
        return false;
    }
    match lang {
        ChunkLang::Rust => {
            if regions.contains(line_start) {
                return true;
            }
            // 0-indexed line of the symbol start.
            let idx = line_start - 1;
            // SAME-LINE form: `#[test] fn x() {}` / `#[cfg(test)] fn …` — the attribute
            // sits inline on the item's own start line (missed by the preceding scan).
            if let Some(l) = file_lines.get(idx) {
                if leading_line_test_attr(l, false) {
                    return true;
                }
            }
            // PRECEDING attribute block: the extractor stores attributes as prev-siblings,
            // so a `#[test]` on its own line above the item lives here.
            let mut j = idx;
            while j > 0 {
                j -= 1;
                let raw = match file_lines.get(j) {
                    Some(l) => *l,
                    None => break,
                };
                let t = raw.trim();
                if t.is_empty() {
                    continue;
                }
                if t.starts_with("#[") {
                    if leading_line_test_attr(t, false) {
                        return true;
                    }
                    continue;
                }
                if t.starts_with("///") || t.starts_with("//!") || t.starts_with("//") {
                    continue;
                }
                break;
            }
            false
        }
        ChunkLang::TsJs => regions.contains(line_start),
        ChunkLang::Other => false,
    }
}

/// Does this file's language use brace-delimited bodies, so that a bodyless callable
/// signature (`;`, no `{`) is a genuine DECLARATION distinct from its implementation?
/// True for Rust, TS/JS, C/C++, Java. FALSE for languages with no forward-declaration
/// concept and no `{}` bodies (Python, Ruby, …) — there a `def foo():` is never a
/// "declaration" and must never be labeled `(decl)`. Extension only SELECTS the syntax
/// family; the decl/impl decision itself stays structural (the body brace).
fn uses_brace_bodies(path: &str) -> bool {
    let lower = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    matches!(
        ext,
        "rs" | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "mjs"
            | "cjs"
            | "mts"
            | "cts"
            | "c"
            | "h"
            | "cc"
            | "cpp"
            | "cxx"
            | "hpp"
            | "hh"
            | "hxx"
            | "java"
    )
}

/// Is `subtype` a CALLABLE (function-like) node — the only axis the decl/impl
/// distinction applies to (spec §2.2: C/C++ .h vs .cc, TS interface members, Rust
/// trait method decls)? A CONSTANT/VARIABLE/TYPE_ALIAS whose initializer merely
/// contains a `(` is NOT a declaration-without-a-body, so it must never be labeled
/// `(decl)`. Matches the stored uppercase `nodes.subtype` values.
fn is_callable_subtype(subtype: Option<&str>) -> bool {
    matches!(
        subtype,
        Some("FUNCTION" | "METHOD" | "CONSTRUCTOR" | "GETTER" | "SETTER")
    )
}

/// Is the chunk's span a DECLARATION without a body (spec §2.2)? True only when ALL of:
/// the file's language uses brace bodies ([`uses_brace_bodies`]); the symbol is a
/// callable ([`is_callable_subtype`]); and the sanitized span has a signature `(` but
/// NO body-opening `{`. Bodyless prototypes / trait-method decls / interface members /
/// pure virtuals return true; a body-bearing callable, a non-callable, or a
/// no-brace-language symbol returns false. Structural — the extension only selects the
/// syntax family (STANDING HONESTY RULE 2); the body decision is the brace, and the
/// callable gate keeps a const/variable/type-alias from being mislabeled `(decl)`.
pub fn is_declaration(path: &str, subtype: Option<&str>, span_source: &str) -> bool {
    if !uses_brace_bodies(path) || !is_callable_subtype(subtype) {
        return false;
    }
    let s = sanitize(span_source);
    let has_paren = s.contains('(');
    let has_body = s.contains('{');
    has_paren && !has_body
}

#[cfg(test)]
#[path = "classify_tests.rs"]
mod tests;
