//! JAVA-SYMBOL-AMBIGUITY-HINT-1 (RG-REQ-005-L05 revision 2; D-JSAH-PLACEMENT-1 A with
//! Correction 1): the `matches` payload of the `AmbiguousSymbol` error that `callers`,
//! `callees` and `path` return.
//!
//! Each candidate key the shared resolver returned is listed, in the resolver's order, with
//! the row the store holds for that key (`StorageConnection::symbol_row_by_stable_key`). The
//! key text is never parsed for identity: an overload key ends `:dupN`, and a split of the key
//! text mis-reads it (the deleted `parse_ambiguous_matches` printed `METHOD:dup2` as a kind).
//! A key whose row is missing, and a key whose store read failed, are still listed, with the key
//! and nulls, each under its own `lookup` state (D-JSAH-READ-STATE-1 B): `{"kind":"found"}`,
//! `{"kind":"missing"}` (`Ok(None)`), `{"kind":"read-failed","reason":…}` (`Err(e)`). The two
//! non-found states are never folded together (RG-REQ-002-L04: a failed read is unknown with its
//! reason, not absence).
//!
//! Abstraction record — `ambiguous_matches`: one payload builder; concrete current users: the
//! four `AmbiguousSymbol` sites of `dispatch.rs` (`handle_callers`, `handle_callees`,
//! `handle_path` from/to); force: the 500-line guardrail on `dispatch.rs` and a unit-test seam
//! over an in-memory store; simpler alternative rejected: an in-place edit of `dispatch.rs`
//! (D-JSAH-PLACEMENT-1 option B).

use repo_graph_storage::StorageConnection;
use serde_json::{json, Value};

/// Build the `matches` array for an ambiguous symbol query: one object per key, in `keys`
/// order, with `stable_key`, `qualified_name` (the stored one), `name`, `kind`
/// (`<kind>:<subtype>`, or `<kind>` when the subtype is not stored), `file`, `line` and
/// `signature` (verbatim), and `lookup` = `{"kind":"found"}`. A key without a stored row gives
/// the key, nulls and `lookup` = `{"kind":"missing"}`; a failed read gives the key, nulls and
/// `lookup` = `{"kind":"read-failed","reason":<error text>}` — the candidate is never dropped.
pub(super) fn ambiguous_matches(
    storage: &StorageConnection,
    snapshot_uid: &str,
    keys: &[String],
) -> Value {
    let matches: Vec<Value> = keys
        .iter()
        .map(
            |key| match storage.symbol_row_by_stable_key(snapshot_uid, key) {
                Ok(Some(row)) => json!({
                    "stable_key": key,
                    "qualified_name": row.qualified_name,
                    "name": row.name,
                    "kind": match row.subtype {
                        Some(subtype) => format!("{}:{}", row.kind, subtype),
                        None => row.kind,
                    },
                    "file": row.file,
                    "line": row.line,
                    "signature": row.signature,
                    "lookup": {"kind": "found"},
                }),
                // No stored row for this key in the snapshot read: every other field is unknown,
                // so it is null — never parsed from the key text.
                Ok(None) => unknown_candidate(key, json!({"kind": "missing"})),
                // The read failed: the same nulls under a distinct state carrying the reason; the
                // error is also logged to the daemon's stderr.
                Err(e) => {
                    eprintln!("warning: could not read the ambiguous candidate {key}: {e}");
                    unknown_candidate(key, json!({"kind": "read-failed", "reason": e.to_string()}))
                }
            },
        )
        .collect();
    Value::Array(matches)
}

/// A candidate whose stored row is unknown: the key, null fields and the `lookup` state saying why.
fn unknown_candidate(key: &str, lookup: Value) -> Value {
    json!({
        "stable_key": key,
        "qualified_name": null,
        "name": null,
        "kind": null,
        "file": null,
        "line": null,
        "signature": null,
        "lookup": lookup,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SNAP: &str = "snap-jsah-dr";
    const KEY1: &str =
        "r1:clients/src/main/java/org/example/KafkaProducer.java#KafkaProducer.send:SYMBOL:METHOD";
    const KEY2: &str = "r1:clients/src/main/java/org/example/KafkaProducer.java#KafkaProducer.send:SYMBOL:METHOD:dup2";

    fn store_with_overloads() -> StorageConnection {
        let storage = StorageConnection::open_in_memory().unwrap();
        // `execute_raw` runs one statement per call.
        for sql in [
            "INSERT INTO repos (repo_uid, name, root_path, created_at) \
               VALUES ('r1', 'test-repo', '/tmp/r1', '2024-01-01T00:00:00Z')"
                .to_string(),
            format!(
                "INSERT INTO snapshots (snapshot_uid, repo_uid, status, kind, created_at) \
                   VALUES ('{SNAP}', 'r1', 'ready', 'full', '2024-01-01T00:00:00Z')"
            ),
            "INSERT INTO files (file_uid, repo_uid, path, language) \
               VALUES ('r1:kp', 'r1', 'clients/src/main/java/org/example/KafkaProducer.java', 'java')"
                .to_string(),
            format!(
                "INSERT INTO nodes (node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype, name, \
                                    qualified_name, file_uid, line_start, signature) VALUES \
                   ('n1', '{SNAP}', 'r1', '{KEY1}', 'SYMBOL', 'METHOD', 'send', \
                    'org.example.KafkaProducer.send', 'r1:kp', 942, '(ProducerRecord<K, V> record)'), \
                   ('n2', '{SNAP}', 'r1', '{KEY2}', 'SYMBOL', 'METHOD', 'send', \
                    'org.example.KafkaProducer.send', 'r1:kp', 1061, \
                    '(ProducerRecord<K, V> record,\n     Callback callback)')"
            ),
        ] {
            storage.execute_raw(&sql).unwrap();
        }
        storage
    }

    #[test]
    fn dup_key_match_carries_stable_key_stored_name_file_line_and_signature() {
        let storage = store_with_overloads();
        // Resolver order is kept: the :dup2 key first here because the caller passed it first.
        let matches = ambiguous_matches(&storage, SNAP, &[KEY2.to_string(), KEY1.to_string()]);
        let rows = matches.as_array().expect("matches is an array");
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            json!({
                "stable_key": KEY2,
                "qualified_name": "org.example.KafkaProducer.send",
                "name": "send",
                "kind": "SYMBOL:METHOD",
                "file": "clients/src/main/java/org/example/KafkaProducer.java",
                "line": 1061,
                "signature": "(ProducerRecord<K, V> record,\n     Callback callback)",
                "lookup": {"kind": "found"},
            })
        );
        assert_eq!(rows[1]["stable_key"], KEY1);
        assert_eq!(rows[1]["line"], 942);
        // The kind never carries the overload suffix (the mis-split `METHOD:dup2`).
        for row in rows {
            assert!(!row["kind"].as_str().unwrap().contains("dup"), "{row}");
        }
    }

    #[test]
    fn missing_row_is_listed_with_its_key_and_nulls() {
        let storage = store_with_overloads();
        let absent = "r1:gone.java#Gone.run:SYMBOL:METHOD:dup3".to_string();
        let matches = ambiguous_matches(&storage, SNAP, &[KEY1.to_string(), absent.clone()]);
        let rows = matches.as_array().unwrap();
        assert_eq!(rows.len(), 2, "a missing row is listed, never dropped");
        assert_eq!(
            rows[1],
            json!({
                "stable_key": absent,
                "qualified_name": null,
                "name": null,
                "kind": null,
                "file": null,
                "line": null,
                "signature": null,
                "lookup": {"kind": "missing"},
            })
        );
    }

    #[test]
    fn read_failure_is_marked_distinct_from_a_missing_row() {
        let storage = store_with_overloads();
        // The read itself fails: with `nodes` dropped, the SELECT errors (`no such table`).
        storage.execute_raw("DROP TABLE nodes").unwrap();
        let matches = ambiguous_matches(&storage, SNAP, &[KEY2.to_string()]);
        let rows = matches.as_array().unwrap();
        assert_eq!(
            rows.len(),
            1,
            "a candidate whose read failed is listed, never dropped"
        );
        let row = &rows[0];
        assert_eq!(row["stable_key"], KEY2, "the key is kept");
        for field in [
            "qualified_name",
            "name",
            "kind",
            "file",
            "line",
            "signature",
        ] {
            assert!(
                row[field].is_null(),
                "{field} is unknown after a failed read: {row}"
            );
        }
        assert_eq!(row["lookup"]["kind"], "read-failed", "{row}");
        assert_ne!(
            row["lookup"]["kind"], "missing",
            "never folded into a missing row"
        );
        let reason = row["lookup"]["reason"]
            .as_str()
            .expect("a failed read carries its reason");
        assert!(!reason.is_empty(), "{row}");
    }
}
