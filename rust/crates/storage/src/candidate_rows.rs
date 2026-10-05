//! JAVA-SYMBOL-AMBIGUITY-HINT-1 (D-JSAH-STORAGE-READ-1 A): one narrow read of a node row by
//! its stable key.
//!
//! The daemon lists each candidate of an ambiguous symbol query with the identity, anchor and
//! signature the store holds for it. The shared resolver (`queries.rs`: `resolve_symbol`,
//! `ResolvedSymbol`) carries no signature and is not changed; this file adds one raw read
//! beside it instead.
//!
//! Abstraction record — `SymbolRow` + `symbol_row_by_stable_key`: one raw DTO and one query;
//! concrete current user: `daemon-runtime/src/dispatch_ambiguous_matches.rs`; force: the
//! storage boundary (the daemon cannot run SQL) and the missing `signature` on the resolver's
//! DTO; simpler alternatives rejected in D-JSAH-STORAGE-READ-1 (reuse `find_fact_symbols`:
//! couples the listing to `find`'s LIKE search; widen `ResolvedSymbol`: changes the shared
//! resolver's DTO).

use crate::connection::StorageConnection;
use crate::error::StorageError;

/// The stored row of one node, as `nodes` (joined to `files` for the path) holds it.
/// Every optional column stays `None` when the store has no value — never a default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolRow {
    pub stable_key: String,
    pub name: String,
    pub qualified_name: Option<String>,
    pub kind: String,
    pub subtype: Option<String>,
    /// Repo-relative path of the node's file (`files.path`).
    pub file: Option<String>,
    /// `nodes.line_start`; `None` when the store holds no positive line — `0` is the store's
    /// "no span" sentinel and is never a line (RG-REQ-012-L07; the same `> 0` filter as
    /// `agent_impl.rs`'s symbol reads).
    pub line: Option<i64>,
    /// `nodes.signature`, verbatim (whitespace as stored).
    pub signature: Option<String>,
}

impl StorageConnection {
    /// Read the node whose `stable_key` equals `stable_key` in `snapshot_uid`.
    /// `Ok(None)` when the snapshot holds no such key. A stored `line_start` of `0` (or any
    /// non-positive value) reads as `line: None`.
    pub fn symbol_row_by_stable_key(
        &self,
        snapshot_uid: &str,
        stable_key: &str,
    ) -> Result<Option<SymbolRow>, StorageError> {
        let result = self.connection().query_row(
            "SELECT n.stable_key, n.name, n.qualified_name, n.kind, n.subtype, f.path, n.line_start, n.signature FROM nodes n LEFT JOIN files f ON n.file_uid = f.file_uid WHERE n.snapshot_uid = ? AND n.stable_key = ? LIMIT 1",
            rusqlite::params![snapshot_uid, stable_key],
            |row| {
                Ok(SymbolRow {
                    stable_key: row.get(0)?,
                    name: row.get(1)?,
                    qualified_name: row.get(2)?,
                    kind: row.get(3)?,
                    subtype: row.get(4)?,
                    file: row.get(5)?,
                    line: row.get::<_, Option<i64>>(6)?.filter(|l| *l > 0),
                    signature: row.get(7)?,
                })
            },
        );
        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StorageError::Sqlite(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SNAP: &str = "snap-jsah-1";

    fn store_with_overloads() -> StorageConnection {
        let storage = StorageConnection::open_in_memory().unwrap();
        storage
            .connection()
            .execute_batch(&format!(
                "INSERT INTO repos (repo_uid, name, root_path, created_at) \
                   VALUES ('r1', 'test-repo', '/tmp/r1', '2024-01-01T00:00:00Z'); \
                 INSERT INTO snapshots (snapshot_uid, repo_uid, status, kind, created_at) \
                   VALUES ('{SNAP}', 'r1', 'ready', 'full', '2024-01-01T00:00:00Z'); \
                 INSERT INTO files (file_uid, repo_uid, path, language) \
                   VALUES ('r1:src/K.java', 'r1', 'src/K.java', 'java'); \
                 INSERT INTO nodes (node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype, name, \
                                    qualified_name, file_uid, line_start, signature) VALUES \
                   ('n1', '{SNAP}', 'r1', 'r1:src/K.java#K.send:SYMBOL:METHOD', 'SYMBOL', 'METHOD', \
                    'send', 'org.example.K.send', 'r1:src/K.java', 942, '(Rec record)'), \
                   ('n2', '{SNAP}', 'r1', 'r1:src/K.java#K.send:SYMBOL:METHOD:dup2', 'SYMBOL', 'METHOD', \
                    'send', 'org.example.K.send', 'r1:src/K.java', 1061, '(Rec record, Callback cb)'), \
                   ('n3', '{SNAP}', 'r1', 'r1:src/K.java#K.send:SYMBOL:METHOD:dup3', 'SYMBOL', 'METHOD', \
                    'send', 'org.example.K.send', 'r1:src/K.java', 0, NULL);"
            ))
            .unwrap();
        storage
    }

    #[test]
    fn row_by_stable_key_carries_signature_file_and_line() {
        let storage = store_with_overloads();
        let row = storage
            .symbol_row_by_stable_key(SNAP, "r1:src/K.java#K.send:SYMBOL:METHOD:dup2")
            .unwrap()
            .expect("the :dup2 overload is stored");
        assert_eq!(
            row,
            SymbolRow {
                stable_key: "r1:src/K.java#K.send:SYMBOL:METHOD:dup2".into(),
                name: "send".into(),
                qualified_name: Some("org.example.K.send".into()),
                kind: "SYMBOL".into(),
                subtype: Some("METHOD".into()),
                file: Some("src/K.java".into()),
                line: Some(1061),
                signature: Some("(Rec record, Callback cb)".into()),
            }
        );
        // The other overload is a different row: the key, not the name, selects it.
        let first = storage
            .symbol_row_by_stable_key(SNAP, "r1:src/K.java#K.send:SYMBOL:METHOD")
            .unwrap()
            .unwrap();
        assert_eq!(first.line, Some(942));
        assert_eq!(first.signature.as_deref(), Some("(Rec record)"));
    }

    #[test]
    fn missing_key_is_none() {
        let storage = store_with_overloads();
        assert_eq!(
            storage
                .symbol_row_by_stable_key(SNAP, "r1:src/K.java#K.send:SYMBOL:METHOD:dup4")
                .unwrap(),
            None
        );
        // The same key in another snapshot is not this snapshot's row.
        assert_eq!(
            storage
                .symbol_row_by_stable_key("other-snap", "r1:src/K.java#K.send:SYMBOL:METHOD")
                .unwrap(),
            None
        );
    }

    #[test]
    fn zero_line_start_reads_as_none() {
        // `line_start = 0` is the store's no-span sentinel: the row is read, its line is absent —
        // so neither JSON nor a human row can carry `:0` (RG-REQ-012-L07).
        let storage = store_with_overloads();
        let row = storage
            .symbol_row_by_stable_key(SNAP, "r1:src/K.java#K.send:SYMBOL:METHOD:dup3")
            .unwrap()
            .expect("the row with the 0 sentinel is stored");
        assert_eq!(row.line, None);
        assert_eq!(row.file.as_deref(), Some("src/K.java"));
        assert_eq!(row.signature, None);
    }
}
