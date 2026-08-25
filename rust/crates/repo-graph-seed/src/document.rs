//! The exact serialized documents sent to the model (spec §3.2). The role
//! prefixes are the nomic model's required document/query discriminators —
//! mixing them degrades the model, so they are not decorative.

/// nomic document-role prefix (spec §3.2; `tools/embed-seed-spike/spike.py:79`).
pub const DOCUMENT_ROLE_PREFIX: &str = "search_document: ";
/// nomic query-role prefix (`spike.py:146`).
pub const QUERY_ROLE_PREFIX: &str = "search_query: ";
/// First up-to-60 physical lines (spike (F) format, hit@5 = 14/16).
pub const MAX_BODY_LINES: usize = 60;
/// Character cap on the serialized document before it is sent (`spike.py:101`,
/// `texts[i][:6000]`). A **char** cap — we cut on a `char_indices` boundary so
/// the byte length is ≤ the byte length of 6000 chars and we never split a
/// scalar. Fixed mechanism constant (not a ratification cell) — the exact spike
/// value that produced the 14/16 result.
pub const MAX_DOC_CHARS: usize = 6000;

/// Build the file-level document: `search_document: {path}\n{body}` where
/// `{body}` is the first ≤60 physical lines joined by `\n`, then the whole
/// string is truncated to the first 6000 characters (spec §3.2).
pub fn build_document(path: &str, content: &str) -> String {
    let body = content
        .lines()
        .take(MAX_BODY_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    let doc = format!("{DOCUMENT_ROLE_PREFIX}{path}\n{body}");
    truncate_chars(&doc, MAX_DOC_CHARS)
}

/// Build the query document: `search_query: {query}` (same char cap, so a
/// pathological seam input can never blow the request size).
pub fn build_query(query: &str) -> String {
    let q = format!("{QUERY_ROLE_PREFIX}{query}");
    truncate_chars(&q, MAX_DOC_CHARS)
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
    fn document_has_prefix_path_and_body() {
        let doc = build_document("src/a.ts", "line1\nline2\nline3");
        assert_eq!(doc, "search_document: src/a.ts\nline1\nline2\nline3");
    }

    #[test]
    fn body_capped_at_60_lines() {
        let content: String = (0..100)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let doc = build_document("f", &content);
        // prefix + path + "\n" + 60 lines
        let body_lines = doc.lines().skip(1).count();
        assert_eq!(body_lines, 60);
        assert!(doc.contains("\nl59"));
        assert!(!doc.contains("\nl60\n") && !doc.ends_with("l60"));
    }

    #[test]
    fn char_cap_never_splits_a_scalar_and_bounds_length() {
        // A multi-byte scalar right at the boundary must not be split.
        let content = "é".repeat(10_000); // 2 bytes each
        let doc = build_document("p", &content);
        assert_eq!(doc.chars().count(), MAX_DOC_CHARS);
        // valid UTF-8 (would panic on a mid-scalar slice)
        assert!(std::str::from_utf8(doc.as_bytes()).is_ok());
    }

    #[test]
    fn query_has_role_prefix() {
        assert_eq!(build_query("bnr rates"), "search_query: bnr rates");
    }
}
