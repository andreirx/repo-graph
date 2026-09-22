//! The exact serialized documents sent to the model (spec §2.1). SEED-CHUNK-1
//! embeds per-SYMBOL **chunks**, not files, and uses `potion-code-16M-v2`, which is
//! NOT instruction-tuned — so the nomic `search_document:`/`search_query:` role
//! prefixes are DROPPED (they would be embedded as literal tokens, diluting a code
//! model). The chunk recipe is the spike's: `qualified_name` header + `doc_comment`
//! + capped span source (`docs/audits/2026-09-03-seed-chunk-spike-1.md`).
//!
//! SEED-DOCUMENT-1 (RG-REQ-010-L10): a METHOD chunk whose enclosing type is documented
//! also carries the FIRST SENTENCE of that type's doc comment, between its qualified name
//! and its own doc (`docs/audits/2026-09-21-seed-document-spike-1.md`).

/// First up-to-60 physical lines of the span body (spike recipe).
pub const MAX_BODY_LINES: usize = 60;
/// Character cap on the assembled document before it is sent (spike `texts[i][:6000]`).
/// A **char** cap — we cut on a `char_indices` boundary so we never split a scalar.
pub const MAX_DOC_CHARS: usize = 6000;
/// Scalar bound on the enclosing type's first sentence carried into a method's document
/// (RG-REQ-010-L10 names this constant; the value is this slice's, fixed at ratification and
/// never tuned to a query). A longer first sentence is cut on a char boundary.
pub const MAX_ENCLOSING_SENTENCE_CHARS: usize = 160;

/// Leading comment markers stripped from each doc line before the first sentence is read,
/// longest first so `/**` is not read as `/*` + `*`. The stored `doc_comment` shapes differ by
/// extractor (TypeScript keeps the raw `/** … */` block, Rust its raw `///` lines, Python the
/// docstring with its quotes stripped, Java the text with `/** */` and the leading `*` stripped),
/// so the stripping lives here, not in the extractors.
const LEADING_COMMENT_MARKERS: [&str; 9] = ["/**", "/*", "*/", "///", "//!", "//", "*", "#", "!"];

/// The first sentence of a type's doc comment, as carried into its methods' documents.
///
/// Per line: trim, strip ONE leading comment marker ([`LEADING_COMMENT_MARKERS`]) and a
/// trailing block closer `*/`, trim again. Leading empty lines are skipped; the first
/// paragraph ends at the first empty line, at a line starting with `@` (a JSDoc/Javadoc
/// tag) or at the end. Its lines are joined with one space and cut after the first `.`
/// that is followed by whitespace or ends the text; the result is truncated to
/// [`MAX_ENCLOSING_SENTENCE_CHARS`] scalars on a char boundary. `None` when nothing
/// remains (an empty, whitespace-only or marker-only doc). Pure.
pub fn enclosing_doc_sentence(doc: &str) -> Option<String> {
    let mut kept: Vec<&str> = Vec::new();
    for raw in doc.lines() {
        let mut line = raw.trim();
        if let Some(m) = LEADING_COMMENT_MARKERS
            .iter()
            .find(|m| line.starts_with(**m))
        {
            line = &line[m.len()..];
        }
        if let Some(rest) = line.trim_end().strip_suffix("*/") {
            line = rest;
        }
        let line = line.trim();
        if line.is_empty() {
            if kept.is_empty() {
                continue; // leading blank / marker-only line
            }
            break; // end of the first paragraph
        }
        if line.starts_with('@') {
            break; // a doc tag ends the prose
        }
        kept.push(line);
    }
    if kept.is_empty() {
        return None;
    }
    let paragraph = kept.join(" ");
    let mut chars = paragraph.char_indices().peekable();
    let mut end = paragraph.len();
    while let Some((i, c)) = chars.next() {
        if c == '.' && chars.peek().is_none_or(|(_, next)| next.is_whitespace()) {
            end = i + c.len_utf8();
            break;
        }
    }
    let sentence = truncate_chars(paragraph[..end].trim(), MAX_ENCLOSING_SENTENCE_CHARS);
    if sentence.is_empty() {
        None
    } else {
        Some(sentence)
    }
}

/// The qualified name of a symbol's name-parent: the prefix before the LAST `::` or `.`
/// separator, whichever occurs later (`ConversationManager.createSession` →
/// `ConversationManager`; `leveldb::DBImpl::Recover` → `leveldb::DBImpl`; `a.b.c` → `a.b`).
/// `None` when the name has no separator or nothing precedes it (`f`, `::f`). This is a
/// NAME split only — whether that parent is an enclosing TYPE is decided by the caller.
pub fn parent_qualified_name(qn: &str) -> Option<&str> {
    let colons = qn.rfind("::");
    let dot = qn.rfind('.');
    let cut = match (colons, dot) {
        (Some(c), Some(d)) => c.max(d),
        (Some(c), None) => c,
        (None, Some(d)) => d,
        (None, None) => return None,
    };
    let parent = &qn[..cut];
    if parent.is_empty() {
        None
    } else {
        Some(parent)
    }
}

/// Build a chunk document from a SYMBOL's stored facts + its span source text.
///
/// Layout (each present, non-empty part on its own line, in order):
/// 1. `qualified_name` (when stored) — the strongest signal the code model keys on;
/// 2. `enclosing_sentence` (when supplied) — the first sentence of the enclosing type's doc
///    comment ([`enclosing_doc_sentence`]); the pass supplies it for METHOD chunks only;
/// 3. `doc_comment` (when stored) — the authored intent;
/// 4. the span source, capped to the first [`MAX_BODY_LINES`] physical lines.
///
/// The whole string is then truncated to [`MAX_DOC_CHARS`] characters. With
/// `enclosing_sentence = None` the output is byte-identical to the pre-SEED-DOCUMENT-1
/// document. A symbol with neither a qualified name nor a doc comment still contributes its
/// span source (never an empty document — the caller only calls this for nodes WITH a span,
/// spec §2.1).
///
/// # Name domination on one-line chunks (SEED-CHUNK-3, spec §2.2)
///
/// For a ONE-LINE chunk with no doc comment, this document is essentially just its
/// qualified name twice: the `qualified_name` header line plus a span line that is the
/// declaration itself (e.g. `ConversationSession.updatedAt` + `updatedAt: string;` — the
/// name is ≈ 90% of the ~40 characters embedded). The static mean-pooled code model then
/// lets the parent-type words ("conversation", "session") dominate the vector. The FIELD tier
/// in [`crate::rank`] sinks these one-line data declarations below every body-bearing chunk
/// of their partition, and the TS extractor KEEPS a property's JSDoc so a documented field
/// carries an authored counter-signal. Fields never receive the enclosing sentence.
///
/// # The enclosing sentence (SEED-DOCUMENT-1, RG-REQ-010-L10)
///
/// The converse problem: a method that does the work often carries none of a task query's
/// words (FRAKTAG `ConversationManager.createSession`, doc "Creates a new conversation session
/// as a separate Tree.", scored 0.274 under the 0.30 floor on "where are conversations
/// persisted to disk") while its class doc states the effect ("ConversationManager handles
/// conversation persistence."). The measurement (`docs/audits/2026-09-21-seed-document-spike-1.md`)
/// found the enclosing type's FIRST sentence lifts the method above the floor (0.333) while the
/// whole class doc dilutes precise matches and turns short members into class-doc echoes, and an
/// identifier summary never raised a target — so the composition carries that one bounded
/// sentence and nothing more.
pub fn build_chunk_document(
    qualified_name: Option<&str>,
    enclosing_sentence: Option<&str>,
    doc_comment: Option<&str>,
    span_source: &str,
) -> String {
    let body: String = span_source
        .lines()
        .take(MAX_BODY_LINES)
        .collect::<Vec<_>>()
        .join("\n");

    let mut parts: Vec<&str> = Vec::with_capacity(3);
    for part in [qualified_name, enclosing_sentence, doc_comment]
        .into_iter()
        .flatten()
    {
        if !part.is_empty() {
            parts.push(part);
        }
    }
    // `body` is owned; push it last via a joined assembly.
    let head = parts.join("\n");
    let doc = if head.is_empty() {
        body
    } else if body.is_empty() {
        head
    } else {
        format!("{head}\n{body}")
    };
    truncate_chars(&doc, MAX_DOC_CHARS)
}

/// Build the query document — SEED-CHUNK-1 drops the role prefix (potion is not
/// instruction-tuned); the query is embedded verbatim, capped so a pathological
/// input cannot blow the request size.
pub fn build_query(query: &str) -> String {
    truncate_chars(query, MAX_DOC_CHARS)
}

/// Truncate to at most `max_chars` Unicode scalars, always on a char boundary.
fn truncate_chars(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => s[..byte_idx].to_string(),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_has_qualified_name_doc_and_body_in_order() {
        let doc = build_chunk_document(
            Some("db::DBImpl::recover"),
            None,
            Some("Recover the database from the write-ahead log."),
            "void Recover() {\n  replay_log();\n}",
        );
        assert_eq!(
            doc,
            "db::DBImpl::recover\nRecover the database from the write-ahead log.\n\
             void Recover() {\n  replay_log();\n}"
        );
    }

    #[test]
    fn missing_qualified_name_and_doc_still_embeds_the_span() {
        let doc = build_chunk_document(None, None, None, "fn f() {}");
        assert_eq!(doc, "fn f() {}");
    }

    #[test]
    fn empty_qualified_name_and_doc_are_not_blank_lines() {
        let doc = build_chunk_document(Some(""), None, Some(""), "body");
        assert_eq!(doc, "body");
    }

    #[test]
    fn no_role_prefix_is_emitted() {
        let doc = build_chunk_document(Some("q"), None, None, "body");
        assert!(!doc.contains("search_document"));
        assert!(!doc.contains("search_query"));
    }

    #[test]
    fn body_capped_at_60_lines() {
        let content: String = (0..100)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let doc = build_chunk_document(None, None, None, &content);
        assert_eq!(doc.lines().count(), 60);
        assert!(doc.contains("l59"));
        assert!(!doc.contains("l60"));
    }

    #[test]
    fn char_cap_never_splits_a_scalar_and_bounds_length() {
        let content = "é".repeat(10_000); // 2 bytes each
        let doc = build_chunk_document(None, None, None, &content);
        assert_eq!(doc.chars().count(), MAX_DOC_CHARS);
        assert!(std::str::from_utf8(doc.as_bytes()).is_ok());
    }

    #[test]
    fn query_is_verbatim_and_capped() {
        assert_eq!(build_query("crash recovery"), "crash recovery");
        assert_eq!(
            build_query(&"x".repeat(7000)).chars().count(),
            MAX_DOC_CHARS
        );
    }

    #[test]
    fn enclosing_sentence_follows_the_qualified_name_and_precedes_the_own_doc() {
        let doc = build_chunk_document(
            Some("ConversationManager.createSession"),
            Some("ConversationManager handles conversation persistence."),
            Some("Creates a new conversation session as a separate Tree."),
            "async createSession(title: string) {\n  return tree;\n}",
        );
        assert_eq!(
            doc,
            "ConversationManager.createSession\n\
             ConversationManager handles conversation persistence.\n\
             Creates a new conversation session as a separate Tree.\n\
             async createSession(title: string) {\n  return tree;\n}"
        );
        // Without an own doc the sentence still sits between the name and the body.
        let doc = build_chunk_document(Some("C.m"), Some("C stores things."), None, "m() {}");
        assert_eq!(doc, "C.m\nC stores things.\nm() {}");
    }

    #[test]
    fn enclosing_sentence_is_the_first_sentence_stripped_of_comment_markers() {
        // TypeScript keeps the raw JSDoc block (FRAKTAG ConversationManager.ts:48-60 shape).
        let ts = "/**\n * ConversationManager handles conversation persistence.\n * Each conversation is its own Tree (conv-{uuid}), stored in Internal KB.\n * Turns are logged as leaf nodes.\n */";
        assert_eq!(
            enclosing_doc_sentence(ts).as_deref(),
            Some("ConversationManager handles conversation persistence.")
        );
        // Java (markers already stripped) / Python docstring (quotes stripped): plain text.
        assert_eq!(
            enclosing_doc_sentence("S1. S2 continues.").as_deref(),
            Some("S1.")
        );
        // A sentence wrapped over lines is joined with one space.
        assert_eq!(
            enclosing_doc_sentence("Stores the tree\non disk. Then more.").as_deref(),
            Some("Stores the tree on disk.")
        );
        // A `.` not followed by whitespace does not end the sentence.
        assert_eq!(
            enclosing_doc_sentence("Speaks v1.2 of the protocol. Other.").as_deref(),
            Some("Speaks v1.2 of the protocol.")
        );
        // Rust keeps its raw `///` / `//!` lines.
        assert_eq!(
            enclosing_doc_sentence("/// The write-ahead log.\n/// Replayed on open.").as_deref(),
            Some("The write-ahead log.")
        );
        assert_eq!(
            enclosing_doc_sentence("//! Crate root docs.").as_deref(),
            Some("Crate root docs.")
        );
        // Python `#` comment lines; a single-line block comment with its closer.
        assert_eq!(
            enclosing_doc_sentence("# Holds sessions").as_deref(),
            Some("Holds sessions")
        );
        assert_eq!(
            enclosing_doc_sentence("/** Holds sessions */").as_deref(),
            Some("Holds sessions")
        );
        // A JSDoc `@tag` line ends the first paragraph; so does a blank line.
        assert_eq!(
            enclosing_doc_sentence("/**\n * Does a thing\n * @param x the x\n */").as_deref(),
            Some("Does a thing")
        );
        assert_eq!(
            enclosing_doc_sentence("\n\nFirst para\n\nSecond para.").as_deref(),
            Some("First para")
        );
        // No sentence end at all: the whole first paragraph.
        assert_eq!(
            enclosing_doc_sentence(" * A registry of stores\n * keyed by id").as_deref(),
            Some("A registry of stores keyed by id")
        );
    }

    #[test]
    fn enclosing_sentence_truncates_at_the_named_bound_on_a_char_boundary() {
        assert_eq!(MAX_ENCLOSING_SENTENCE_CHARS, 160);
        let long = format!("{} end.", "é".repeat(400)); // 2-byte scalars, one 400+-char sentence
        let s = enclosing_doc_sentence(&long).expect("non-empty");
        assert_eq!(s.chars().count(), MAX_ENCLOSING_SENTENCE_CHARS);
        assert!(std::str::from_utf8(s.as_bytes()).is_ok());
        assert!(s.chars().all(|c| c == 'é'));
        // A sentence under the bound is untouched.
        let short = "a".repeat(MAX_ENCLOSING_SENTENCE_CHARS - 1) + ".";
        assert_eq!(
            enclosing_doc_sentence(&short).as_deref(),
            Some(short.as_str())
        );
    }

    #[test]
    fn enclosing_sentence_is_none_for_an_empty_or_marker_only_doc() {
        for doc in [
            "",
            "   ",
            "\n\n",
            "/** */",
            "/**\n *\n */",
            "///",
            "//",
            "#",
            "@deprecated",
        ] {
            assert_eq!(enclosing_doc_sentence(doc), None, "doc {doc:?}");
        }
    }

    #[test]
    fn no_enclosing_doc_embeds_exactly_as_before() {
        // The bytes a chunk embedded before SEED-DOCUMENT-1 (qualified name, own doc, body).
        let today = "db::DBImpl::recover\nRecover the database.\nvoid Recover() {}";
        assert_eq!(
            build_chunk_document(
                Some("db::DBImpl::recover"),
                None,
                Some("Recover the database."),
                "void Recover() {}"
            ),
            today
        );
        // An empty sentence is not a blank line either.
        assert_eq!(
            build_chunk_document(
                Some("db::DBImpl::recover"),
                Some(""),
                Some("Recover the database."),
                "void Recover() {}"
            ),
            today
        );
        assert_eq!(
            build_chunk_document(Some("q"), None, None, "body"),
            "q\nbody"
        );
    }

    #[test]
    fn parent_qualified_name_splits_on_the_last_separator() {
        assert_eq!(parent_qualified_name("a.b.c"), Some("a.b"));
        assert_eq!(parent_qualified_name("x::y::z"), Some("x::y"));
        assert_eq!(parent_qualified_name("f"), None);
        assert_eq!(
            parent_qualified_name("ConversationManager.createSession"),
            Some("ConversationManager")
        );
        assert_eq!(
            parent_qualified_name("leveldb::DBImpl::Recover"),
            Some("leveldb::DBImpl")
        );
        // The LAST separator wins whichever kind it is.
        assert_eq!(parent_qualified_name("ns::T.m"), Some("ns::T"));
        assert_eq!(parent_qualified_name("pkg.T::m"), Some("pkg.T"));
        // No name before the separator → no parent.
        assert_eq!(parent_qualified_name("::f"), None);
        assert_eq!(parent_qualified_name(".f"), None);
    }
}
