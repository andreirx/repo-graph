//! JAVA-SYMBOL-AMBIGUITY-HINT-1 (RG-REQ-005-L05 revision 2, RG-REQ-012-L03; D-JSAH-PLACEMENT-1 A
//! with Corrections 1–2): the human listing of an `AmbiguousSymbol` error from `callers`,
//! `callees` or `path`.
//!
//! The daemon lists every candidate the resolver returned with its stored row (stable key,
//! qualified name, kind, file, line, signature). This renders one row per candidate and, under
//! it, one cursor in the failing command with the candidate's stable key — the identity `find`
//! prints and every symbol command accepts — whenever that command can be formed. The byte form
//! is SLICE_DOC §2.2. Phrases and their predicates:
//! - `each matches '<query>'` — every candidate came back from `resolve_symbol` for this query
//!   (`storage/src/queries.rs:1047`); the header drops it when `data.query` is absent.
//! - `a stable key names at most one symbol` — the UNIQUE index on `(snapshot_uid, stable_key)`
//!   (`storage/src/migrations/001-initial.sql:97`) proves uniqueness, never existence
//!   (D-JSAH-READ-STATE-1 Correction 1).
//! - `? — no stored row for this key in the snapshot read` — the candidate's `lookup.kind` is
//!   `missing`: `symbol_row_by_stable_key` returned `Ok(None)` (`dispatch_ambiguous_matches.rs`).
//! - `? — store read failed: <reason>` — `lookup.kind` is `read-failed`: the same read returned
//!   `Err(e)`; `<reason>` is `e.to_string()` as the daemon sent it.
//! - `this listing is incomplete — <reasons>` (D-JSAH-INCOMPLETE-CURSOR-1 with Corrections 1–2),
//!   one reason per condition this renderer tests, in this order:
//!   `the daemon did not say which path argument was ambiguous` — `cursor` is `None` and
//!   `data.query` is absent; `the error's query matches neither path argument` — `cursor` is
//!   `None` and `data.query` is present (`path_cursor_affixes` returned `None` for it);
//!   `a candidate has no stable key` — a candidate's `stable_key` is absent.
//!
//! Abstraction record — `render_ambiguous_matches` + `path_cursor_affixes`: one renderer and the
//! `path` side rule; concrete current user: the `AmbiguousSymbol` branch of
//! `commands/graph.rs::handle_daemon_error` (for `run_callers`, `run_callees`, `run_path`);
//! force: the 500-line guardrail on `graph.rs` and a value-test seam; simpler alternative
//! rejected: an in-place loop in `graph.rs`, testable only through the CLI.

use serde_json::Value;

use crate::presentation::{anchor, shell_quote_arg};

/// Reason (i): `cursor` is `None` and the error carries no `query`.
const REASON_NO_QUERY: &str = "the daemon did not say which path argument was ambiguous";
/// Reason (ii): `cursor` is `None` and the error's `query` equals neither `path` argument.
const REASON_QUERY_MATCHES_NEITHER: &str = "the error's query matches neither path argument";
/// Reason (iii): a candidate carries no `stable_key`.
const REASON_NO_STABLE_KEY: &str = "a candidate has no stable key";

/// Render the candidate listing of an ambiguous symbol error (`data` is the error's `data`:
/// `{query, matches: [...]}`).
///
/// `cursor` is `Some((cursor_prefix, cursor_suffix))` when the failing command is known; each
/// candidate with a stable key then gets the line `→ <cursor_prefix> <quoted key><cursor_suffix>`.
/// `cursor` is `None` when the failing command cannot be formed (an unknown `path` side): no
/// candidate gets a `→` line. A candidate without a stable key never gets one. When any `→` line
/// is withheld the last line is `hint: this listing is incomplete — <reasons>` instead of the
/// run hint. The returned text ends with a newline; the caller prints it after the unchanged
/// `error:` line.
pub(crate) fn render_ambiguous_matches(cursor: Option<(&str, &str)>, data: &Value) -> String {
    let query = data.get("query").and_then(Value::as_str);
    let matches: &[Value] = data
        .get("matches")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let text = |m: &Value, field: &str| m.get(field).and_then(Value::as_str).map(str::to_string);

    let mut out = match query {
        Some(q) => format!(
            "Candidates ({}) — each matches '{q}'; pick one by its stable key:\n",
            matches.len()
        ),
        None => format!(
            "Candidates ({}) — pick one by its stable key:\n",
            matches.len()
        ),
    };
    let mut a_candidate_has_no_stable_key = false;
    for (i, m) in matches.iter().enumerate() {
        // A candidate whose row is unknown says why (D-JSAH-READ-STATE-1): the two non-found
        // states keep distinct rows; the cursor line follows either way.
        let lookup_kind = m
            .get("lookup")
            .and_then(|l| l.get("kind"))
            .and_then(Value::as_str);
        let row = match lookup_kind {
            Some("missing") => "? — no stored row for this key in the snapshot read".to_string(),
            Some("read-failed") => {
                let reason = m
                    .get("lookup")
                    .and_then(|l| l.get("reason"))
                    .and_then(Value::as_str)
                    .unwrap_or("?");
                format!("? — store read failed: {reason}")
            }
            // `found` (or a payload without `lookup`): the row's stored columns.
            _ => stored_columns(m),
        };
        out.push_str(&format!("  {}. {row}\n", i + 1));
        match (cursor, text(m, "stable_key")) {
            (Some((cursor_prefix, cursor_suffix)), Some(key)) => out.push_str(&format!(
                "     → {cursor_prefix} {}{cursor_suffix}\n",
                shell_quote_arg(&key)
            )),
            // The command cannot be formed: no line shaped like a command (RG-REQ-012-L03).
            (None, Some(_)) => {}
            (_, None) => a_candidate_has_no_stable_key = true,
        }
    }

    let mut reasons: Vec<&str> = Vec::new();
    if cursor.is_none() {
        reasons.push(if query.is_none() {
            REASON_NO_QUERY
        } else {
            REASON_QUERY_MATCHES_NEITHER
        });
    }
    if a_candidate_has_no_stable_key {
        reasons.push(REASON_NO_STABLE_KEY);
    }
    if reasons.is_empty() {
        out.push_str("hint: run one of the cursors above; a stable key names at most one symbol\n");
    } else {
        out.push_str(&format!(
            "hint: this listing is incomplete — {}\n",
            reasons.join("; ")
        ));
    }
    out
}

/// The columns of a candidate row from its stored fields:
/// `<file>:<line>  <qualified_name>  <kind>  <signature>`, each omitted when not stored.
fn stored_columns(m: &Value) -> String {
    let text = |field: &str| m.get(field).and_then(Value::as_str).map(str::to_string);
    // `<file>:<line>` through the one anchor chokepoint: `<file>` alone without a positive line
    // (a negative `i64` fails `u64::try_from`; `0` converts and `anchor` suppresses it — never
    // `:0`); `?` without a stored file — never an invented line (RG-REQ-012-L07).
    let line = m
        .get("line")
        .and_then(Value::as_i64)
        .and_then(|l| u64::try_from(l).ok());
    let mut columns = vec![match text("file") {
        Some(file) => anchor(&file, line),
        None => "?".to_string(),
    }];
    // The stored qualified name, else the name; the suffix tier can list candidates whose
    // qualified names differ, and this column tells them apart.
    if let Some(name) = text("qualified_name").or_else(|| text("name")) {
        columns.push(name);
    }
    if let Some(kind) = text("kind") {
        columns.push(kind);
    }
    // The signature as one column: every whitespace run (C++ declarators, multi-line Java
    // parameter lists) collapsed to one space. An empty stored signature adds no column.
    if let Some(signature) = text("signature") {
        let collapsed = signature.split_whitespace().collect::<Vec<_>>().join(" ");
        if !collapsed.is_empty() {
            columns.push(collapsed);
        }
    }
    columns.join("  ")
}

/// The `(cursor_prefix, cursor_suffix)` pair for an ambiguous `rmap path <from> <to>`: the
/// ambiguous side is the argument equal to `query` (the error's `data.query`, which the daemon
/// sets to the raw argument it failed to resolve). `from` is compared first because the daemon
/// resolves `from` before `to` and returns at the first ambiguity. The other argument is quoted
/// with `shell_quote_arg`. `None` when `query` is absent or equals neither argument: the side is
/// not known, so no command can be formed — never a silent default to `from` or `to`.
pub(crate) fn path_cursor_affixes(
    from: &str,
    to: &str,
    query: Option<&str>,
) -> Option<(String, String)> {
    match query {
        Some(q) if q == from => {
            Some(("rmap path".to_string(), format!(" {}", shell_quote_arg(to))))
        }
        Some(q) if q == to => Some((
            format!("rmap path {}", shell_quote_arg(from)),
            String::new(),
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const KEY1: &str = "repo_1:clients/src/main/java/org/apache/kafka/clients/producer/KafkaProducer.java#KafkaProducer.send:SYMBOL:METHOD";
    const KEY2: &str = "repo_1:clients/src/main/java/org/apache/kafka/clients/producer/KafkaProducer.java#KafkaProducer.send:SYMBOL:METHOD:dup2";
    const FILE: &str = "clients/src/main/java/org/apache/kafka/clients/producer/KafkaProducer.java";
    const QN: &str = "org.apache.kafka.clients.producer.KafkaProducer.send";

    fn full_match(key: &str, line: i64, signature: &str) -> Value {
        json!({
            "stable_key": key,
            "qualified_name": QN,
            "name": "send",
            "kind": "SYMBOL:METHOD",
            "file": FILE,
            "line": line,
            "signature": signature,
        })
    }

    #[test]
    fn listing_prints_one_runnable_cursor_per_candidate_and_never_says_use_qualified_name() {
        let data = json!({
            "query": "KafkaProducer.send",
            "matches": [
                full_match(KEY1, 942, "(ProducerRecord<K, V> record)"),
                // Stored row without a line and without a signature: `<file>` alone, no
                // signature column.
                {
                    "stable_key": KEY2,
                    "qualified_name": QN,
                    "name": "send",
                    "kind": "SYMBOL:METHOD",
                    "file": FILE,
                    "line": null,
                    "signature": null,
                },
                // A key whose row is missing: `?`, no other column, the cursor still printed.
                {
                    "stable_key": "repo_1:gone.java#Gone.send:SYMBOL:METHOD:dup3",
                    "qualified_name": null,
                    "name": null,
                    "kind": null,
                    "file": null,
                    "line": null,
                    "signature": null,
                },
            ],
        });
        let out = render_ambiguous_matches(Some(("rmap callers", "")), &data);
        let expected = format!(
            "Candidates (3) — each matches 'KafkaProducer.send'; pick one by its stable key:\n\
             \x20 1. {FILE}:942  {QN}  SYMBOL:METHOD  (ProducerRecord<K, V> record)\n\
             \x20    → rmap callers '{KEY1}'\n\
             \x20 2. {FILE}  {QN}  SYMBOL:METHOD\n\
             \x20    → rmap callers '{KEY2}'\n\
             \x20 3. ?\n\
             \x20    → rmap callers 'repo_1:gone.java#Gone.send:SYMBOL:METHOD:dup3'\n\
             hint: run one of the cursors above; a stable key names at most one symbol\n"
        );
        // Exact bytes: the whole listing is pinned, so the old qualified-name hint cannot appear.
        assert_eq!(out, expected);
        assert_eq!(
            out.matches("→ rmap callers '").count(),
            3,
            "one cursor per candidate"
        );
    }

    #[test]
    fn missing_and_failed_rows_render_their_state_before_the_cursor() {
        const MISSING: &str = "repo_1:gone.java#Gone.send:SYMBOL:METHOD:dup3";
        let data = json!({
            "query": "KafkaProducer.send",
            "matches": [
                full_match(KEY1, 942, "(ProducerRecord<K, V> record)"),
                {
                    "stable_key": MISSING,
                    "qualified_name": null, "name": null, "kind": null,
                    "file": null, "line": null, "signature": null,
                    "lookup": {"kind": "missing"},
                },
                {
                    "stable_key": KEY2,
                    "qualified_name": null, "name": null, "kind": null,
                    "file": null, "line": null, "signature": null,
                    "lookup": {"kind": "read-failed", "reason": "no such table: nodes"},
                },
            ],
        });
        let mut found = full_match(KEY1, 942, "(ProducerRecord<K, V> record)");
        found["lookup"] = json!({"kind": "found"});
        let mut data_found = data.clone();
        data_found["matches"][0] = found;
        // A `found` row renders the same with or without the `lookup` field.
        assert_eq!(
            render_ambiguous_matches(Some(("rmap callers", "")), &data),
            render_ambiguous_matches(Some(("rmap callers", "")), &data_found)
        );
        let out = render_ambiguous_matches(Some(("rmap callers", "")), &data_found);
        let expected = format!(
            "Candidates (3) — each matches 'KafkaProducer.send'; pick one by its stable key:\n\
             \x20 1. {FILE}:942  {QN}  SYMBOL:METHOD  (ProducerRecord<K, V> record)\n\
             \x20    → rmap callers '{KEY1}'\n\
             \x20 2. ? — no stored row for this key in the snapshot read\n\
             \x20    → rmap callers '{MISSING}'\n\
             \x20 3. ? — store read failed: no such table: nodes\n\
             \x20    → rmap callers '{KEY2}'\n\
             hint: run one of the cursors above; a stable key names at most one symbol\n"
        );
        assert_eq!(out, expected);
    }

    #[test]
    fn path_cursors_carry_the_other_argument_shell_quoted() {
        let other = "Foo's bar";
        // `from` is the ambiguous side: the cursor replaces `from`, `to` follows quoted.
        let (prefix, suffix) =
            path_cursor_affixes("KafkaProducer.send", other, Some("KafkaProducer.send"))
                .expect("the from side is known");
        assert_eq!(prefix, "rmap path");
        assert_eq!(suffix, " 'Foo'\\''s bar'");
        let data = json!({
            "query": "KafkaProducer.send",
            "matches": [full_match(KEY1, 942, "(ProducerRecord<K, V> record)")],
        });
        let out = render_ambiguous_matches(Some((&prefix, &suffix)), &data);
        assert!(
            out.contains(&format!("     → rmap path '{KEY1}' 'Foo'\\''s bar'\n")),
            "{out}"
        );

        // `to` is the ambiguous side: `from` comes first, quoted; the cursor ends the command.
        let (prefix, suffix) =
            path_cursor_affixes(other, "KafkaProducer.send", Some("KafkaProducer.send"))
                .expect("the to side is known");
        assert_eq!(prefix, "rmap path 'Foo'\\''s bar'");
        assert_eq!(suffix, "");
        let out = render_ambiguous_matches(Some((&prefix, &suffix)), &data);
        assert!(
            out.contains(&format!("     → rmap path 'Foo'\\''s bar' '{KEY1}'\n")),
            "{out}"
        );

        // Both arguments equal the query: `from` is compared first (the daemon resolves it first).
        let (prefix, suffix) = path_cursor_affixes("send", "send", Some("send")).unwrap();
        assert_eq!((prefix.as_str(), suffix.as_str()), ("rmap path", " send"));
    }

    #[test]
    fn key_with_apostrophe_is_shell_quoted() {
        // A Rust-shaped key: lifetimes put an apostrophe in the qualified name
        // (D-JSAH-CURSOR-1 Correction 2: 1204 such SYMBOL keys in the retained stores).
        let key = "repo_1:src/extract.rs#ExtractionCtx<'a>.make_stable_key:SYMBOL:METHOD";
        let data = json!({
            "query": "make_stable_key",
            "matches": [{
                "stable_key": key,
                "qualified_name": "ExtractionCtx<'a>.make_stable_key",
                "name": "make_stable_key",
                "kind": "SYMBOL:METHOD",
                "file": "src/extract.rs",
                "line": 12,
                "signature": null,
            }],
        });
        let out = render_ambiguous_matches(Some(("rmap callees", "")), &data);
        assert!(
            out.contains(
                "     → rmap callees 'repo_1:src/extract.rs#ExtractionCtx<'\\''a>.make_stable_key:SYMBOL:METHOD'\n"
            ),
            "{out}"
        );
    }

    #[test]
    fn signature_renders_as_one_field_with_collapsed_whitespace() {
        let cpp_key = "repo_2:db/db_impl.cc#leveldb::DBImpl::Get:SYMBOL:METHOD";
        let data = json!({
            "query": "DBImpl::Get",
            "matches": [
                {
                    "stable_key": cpp_key,
                    "qualified_name": "leveldb::DBImpl::Get",
                    "name": "Get",
                    "kind": "SYMBOL:METHOD",
                    "file": "db/db_impl.cc",
                    "line": 1120,
                    "signature": "leveldb::DBImpl::Get(const ReadOptions& options,\n                    const Slice& key,   std::string* value)",
                },
                full_match(KEY2, 1061, "(\n        ProducerRecord<K, V> record,\n\tCallback callback\n    )"),
            ],
        });
        let out = render_ambiguous_matches(Some(("rmap callers", "")), &data);
        assert!(
            out.contains(
                "  1. db/db_impl.cc:1120  leveldb::DBImpl::Get  SYMBOL:METHOD  leveldb::DBImpl::Get(const ReadOptions& options, const Slice& key, std::string* value)\n"
            ),
            "{out}"
        );
        assert!(
            out.contains(&format!(
                "  2. {FILE}:1061  {QN}  SYMBOL:METHOD  ( ProducerRecord<K, V> record, Callback callback )\n"
            )),
            "{out}"
        );
    }

    #[test]
    fn incomplete_payload_prints_no_cursor_and_names_the_gap() {
        // The `path` side is unknown for an absent query and for a query equal to neither
        // argument: no command can be formed.
        assert_eq!(path_cursor_affixes("a", "b", None), None);
        assert_eq!(path_cursor_affixes("a", "b", Some("c")), None);

        let keyless = json!({
            "qualified_name": QN, "name": "send", "kind": "SYMBOL:METHOD",
            "file": FILE, "line": 1061, "signature": null,
        });

        // (i) + (iii): absent `query` — the header has no `each matches`; no `→` line for any
        // candidate; every applicable reason in its fixed order.
        let data = json!({
            "matches": [full_match(KEY1, 942, "(ProducerRecord<K, V> record)"), keyless.clone()],
        });
        let out = render_ambiguous_matches(
            path_cursor_affixes("a", "b", None)
                .as_ref()
                .map(|(p, s)| (p.as_str(), s.as_str())),
            &data,
        );
        let expected = format!(
            "Candidates (2) — pick one by its stable key:\n\
             \x20 1. {FILE}:942  {QN}  SYMBOL:METHOD  (ProducerRecord<K, V> record)\n\
             \x20 2. {FILE}:1061  {QN}  SYMBOL:METHOD\n\
             hint: this listing is incomplete — the daemon did not say which path argument was ambiguous; a candidate has no stable key\n"
        );
        assert_eq!(out, expected);

        // (ii): a present `query` equal to neither argument — the header keeps `each matches`.
        let data = json!({
            "query": "c",
            "matches": [full_match(KEY1, 942, "(ProducerRecord<K, V> record)")],
        });
        let out = render_ambiguous_matches(None, &data);
        let expected = format!(
            "Candidates (1) — each matches 'c'; pick one by its stable key:\n\
             \x20 1. {FILE}:942  {QN}  SYMBOL:METHOD  (ProducerRecord<K, V> record)\n\
             hint: this listing is incomplete — the error's query matches neither path argument\n"
        );
        assert_eq!(out, expected);

        // (iii) alone: the command is known; the keyed candidate keeps its cursor, the keyless
        // one gets none, and the run hint is replaced.
        let data = json!({
            "query": "KafkaProducer.send",
            "matches": [full_match(KEY1, 942, "(ProducerRecord<K, V> record)"), keyless],
        });
        let out = render_ambiguous_matches(Some(("rmap callers", "")), &data);
        let expected = format!(
            "Candidates (2) — each matches 'KafkaProducer.send'; pick one by its stable key:\n\
             \x20 1. {FILE}:942  {QN}  SYMBOL:METHOD  (ProducerRecord<K, V> record)\n\
             \x20    → rmap callers '{KEY1}'\n\
             \x20 2. {FILE}:1061  {QN}  SYMBOL:METHOD\n\
             hint: this listing is incomplete — a candidate has no stable key\n"
        );
        assert_eq!(out, expected);
        assert!(!out.contains("run one of the cursors above"), "{out}");
    }

    #[test]
    fn zero_line_renders_bare_file() {
        // `line: 0` (the store's no-span sentinel, from a foreign payload) and a negative line
        // render `<file>` alone — never `<file>:0`, never an invented line.
        for line in [0_i64, -3] {
            let data = json!({
                "query": "KafkaProducer.send",
                "matches": [full_match(KEY1, line, "(ProducerRecord<K, V> record)")],
            });
            let out = render_ambiguous_matches(Some(("rmap callers", "")), &data);
            assert!(
                out.contains(&format!(
                    "  1. {FILE}  {QN}  SYMBOL:METHOD  (ProducerRecord<K, V> record)\n"
                )),
                "{out}"
            );
            assert!(!out.contains(".java:"), "{out}");
        }
    }
}
