//! The exact serialized documents sent to the model (spec §2.1). SEED-CHUNK-1
//! embeds per-SYMBOL **chunks**, not files, and uses `potion-code-16M-v2`, which is
//! NOT instruction-tuned — so the nomic `search_document:`/`search_query:` role
//! prefixes are DROPPED (they would be embedded as literal tokens, diluting a code
//! model). The chunk recipe is the spike's: `qualified_name` header + `doc_comment`
//! + capped span source (`docs/audits/2026-09-03-seed-chunk-spike-1.md`).

/// First up-to-60 physical lines of the span body (spike recipe).
pub const MAX_BODY_LINES: usize = 60;
/// Character cap on the assembled document before it is sent (spike `texts[i][:6000]`).
/// A **char** cap — we cut on a `char_indices` boundary so we never split a scalar.
pub const MAX_DOC_CHARS: usize = 6000;

/// Build a chunk document from a SYMBOL's stored facts + its span source text.
///
/// Layout (each present part on its own line, in order):
/// 1. `qualified_name` (when stored) — the strongest signal the code model keys on;
/// 2. `doc_comment` (when stored) — the authored intent;
/// 3. the span source, capped to the first [`MAX_BODY_LINES`] physical lines.
///
/// The whole string is then truncated to [`MAX_DOC_CHARS`] characters. A symbol
/// with neither a qualified name nor a doc comment still contributes its span
/// source (never an empty document — the caller only calls this for nodes WITH a
/// span, spec §2.1).
pub fn build_chunk_document(
    qualified_name: Option<&str>,
    doc_comment: Option<&str>,
    span_source: &str,
) -> String {
    let body: String = span_source
        .lines()
        .take(MAX_BODY_LINES)
        .collect::<Vec<_>>()
        .join("\n");

    let mut parts: Vec<&str> = Vec::with_capacity(3);
    if let Some(q) = qualified_name {
        if !q.is_empty() {
            parts.push(q);
        }
    }
    if let Some(d) = doc_comment {
        if !d.is_empty() {
            parts.push(d);
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
        let doc = build_chunk_document(None, None, "fn f() {}");
        assert_eq!(doc, "fn f() {}");
    }

    #[test]
    fn empty_qualified_name_and_doc_are_not_blank_lines() {
        let doc = build_chunk_document(Some(""), Some(""), "body");
        assert_eq!(doc, "body");
    }

    #[test]
    fn no_role_prefix_is_emitted() {
        let doc = build_chunk_document(Some("q"), None, "body");
        assert!(!doc.contains("search_document"));
        assert!(!doc.contains("search_query"));
    }

    #[test]
    fn body_capped_at_60_lines() {
        let content: String = (0..100)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let doc = build_chunk_document(None, None, &content);
        assert_eq!(doc.lines().count(), 60);
        assert!(doc.contains("l59"));
        assert!(!doc.contains("l60"));
    }

    #[test]
    fn char_cap_never_splits_a_scalar_and_bounds_length() {
        let content = "é".repeat(10_000); // 2 bytes each
        let doc = build_chunk_document(None, None, &content);
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
}
