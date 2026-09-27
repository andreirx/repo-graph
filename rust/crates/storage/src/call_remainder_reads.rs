//! PYTHON-RECEIVER-BINDING-1 (RG-REQ-002-L11, RG-REQ-005-L09): what the certain caller/callee
//! sets of one symbol leave out.
//!
//! The certain sets (`find_symbol_callers`/`find_symbol_callees`, the daemon's default
//! `callers`/`callees` rows) hold only CALLS edges whose resolution is `static` or `dynamic`
//! (D-PSI-R1-VOCAB). This module reads the REMAINDER an agent must be told about:
//!
//!   * the INFERRED CALLS edges into / out of the symbol (a name-only binding on an untyped
//!     receiver, or a legacy compiler-promoted row), each anchored at its CALL SITE (the edge's
//!     file and line — one source) and carrying its evidence through ONE decoder,
//!     [`inferred_call_evidence`];
//!   * the UNRESOLVED CALLS naming the symbol (target key's last `.`/`::` segment equals the
//!     symbol's name) or leaving it (source is the symbol), counted per classifier basis code,
//!     with the first [`SITES_LIMIT`] sites and — where the resolver recorded one — each site's
//!     candidate pool (bounded to [`CANDIDATES_LIMIT`] names, the full count stated).
//!
//! It lives beside `queries.rs` (over the CLAUDE.md size guardrail) because the reads share one
//! responsibility — the remainder — and two consumers (the agent port for `explain`/orient, the
//! daemon's `callers`/`callees`). No read here swallows a failure: a failed read is an error, an
//! absent or malformed evidence carrier is a NAMED state (`unrecorded`, `unreadable`), never a
//! drop and never a guess.

use repo_graph_agent::{
    AgentBasisCount, AgentCallRemainders, AgentInferredCallRow, AgentUnresolvedCallSite,
};

use crate::connection::StorageConnection;
use crate::error::StorageError;

/// The `basis` an inferred row is served with when its evidence is absent or malformed.
pub const UNRECORDED_BASIS: &str = "unrecorded";
/// The `candidate_reason` of a site whose recorded pool cannot be read.
pub const UNREADABLE_POOL: &str = "unreadable";
/// The reason stated for a site carrying `mroCandidates` (a declined MRO collision).
const MRO_POOL_REASON: &str = "self_call_ambiguous_mro";
/// At most this many unresolved sites are listed per remainder (ordered by file, line, column,
/// target key); the count is always the full count.
pub const SITES_LIMIT: usize = 20;
/// At most this many candidates are listed per site (sorted); `candidate_count` is the full size.
pub const CANDIDATES_LIMIT: usize = 50;

/// The evidence of an inferred CALLS row, decoded ONCE for every reader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferredCallEvidence {
    /// The recorded basis (`receiver_untyped_name_only`), or [`UNRECORDED_BASIS`].
    pub basis: String,
    /// The recorded receiver text; `None` when unrecorded.
    pub receiver: Option<String>,
    /// The row's writer (`edges.extractor`) — the diagnostic that names an unrecorded row's source.
    pub extractor: String,
}

/// Decode an inferred CALLS row's evidence in its three states (D-PRB-ENRICH-1):
///
/// - present-valid (`metadata_json` is an object whose `basis` and `receiver` are strings) —
///   served as recorded;
/// - absent (`None` or empty) — `unrecorded`;
/// - present-malformed (any other shape: unparsable, not an object, a missing or wrong-typed
///   `basis`/`receiver` — the legacy `compiler-promotion:0.1.0` rows carry
///   `{promotedFrom, receiverType, methodName}`) — `unrecorded`.
///
/// Never an error, never a drop, never the name-only label for an unrecorded row.
pub fn inferred_call_evidence(
    extractor: &str,
    metadata_json: Option<&str>,
) -> InferredCallEvidence {
    let recorded = metadata_json
        .filter(|raw| !raw.trim().is_empty())
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .and_then(|value| {
            let basis = value.get("basis")?.as_str()?.to_string();
            let receiver = value.get("receiver")?.as_str()?.to_string();
            Some((basis, receiver))
        });
    match recorded {
        Some((basis, receiver)) => InferredCallEvidence {
            basis,
            receiver: Some(receiver),
            extractor: extractor.to_string(),
        },
        None => InferredCallEvidence {
            basis: UNRECORDED_BASIS.to_string(),
            receiver: None,
            extractor: extractor.to_string(),
        },
    }
}

/// The candidate pool a site recorded: `(reason, full count, first candidates)`; `None` when the
/// row records no pool. A present-but-malformed pool (or unparsable metadata) is
/// `(UNREADABLE_POOL, None, None)` — never a guessed list.
type SitePool = Option<(String, Option<u64>, Option<Vec<String>>)>;

fn site_pool(metadata_json: Option<&str>) -> SitePool {
    let raw = metadata_json?;
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Some((UNREADABLE_POOL.to_string(), None, None));
    };
    let unreadable = || Some((UNREADABLE_POOL.to_string(), None, None));
    let names = |items: &serde_json::Value| -> Option<Vec<String>> {
        items
            .as_array()?
            .iter()
            .map(|i| i.as_str().map(str::to_string))
            .collect()
    };
    if let Some(pool) = value.get("nameOnlyCandidates") {
        let reason = value.get("nameOnlyReason").and_then(|r| r.as_str());
        return match (names(pool), reason) {
            (Some(mut list), Some(reason)) => {
                list.sort();
                let count = list.len() as u64;
                list.truncate(CANDIDATES_LIMIT);
                Some((reason.to_string(), Some(count), Some(list)))
            }
            _ => unreadable(),
        };
    }
    if let Some(pool) = value.get("mroCandidates") {
        return match names(pool) {
            Some(mut list) => {
                list.sort();
                let count = list.len() as u64;
                list.truncate(CANDIDATES_LIMIT);
                Some((MRO_POOL_REASON.to_string(), Some(count), Some(list)))
            }
            None => unreadable(),
        };
    }
    None
}

/// Which unresolved calls a remainder counts.
#[derive(Clone, Copy)]
enum UnresolvedScope {
    /// Calls whose target key's last `.`/`::` segment equals the symbol's name.
    Naming,
    /// Calls whose source is the symbol.
    From,
}

impl UnresolvedScope {
    /// The WHERE clause over `unresolved_edges u` (`?1` snapshot, `?2` symbol stable key, `?3`
    /// symbol name). The naming test is a suffix match at a `.`/`::` boundary, or the whole key.
    fn predicate(self) -> &'static str {
        match self {
            Self::Naming => {
                "u.snapshot_uid = ?1 AND u.type = 'CALLS' AND ?2 = ?2 AND ( \
                   u.target_key = ?3 \
                   OR (length(u.target_key) > length(?3) + 1 \
                       AND substr(u.target_key, -(length(?3) + 1)) = '.' || ?3) \
                   OR (length(u.target_key) > length(?3) + 2 \
                       AND substr(u.target_key, -(length(?3) + 2)) = '::' || ?3))"
            }
            Self::From => {
                "u.snapshot_uid = ?1 AND u.type = 'CALLS' AND ?3 = ?3 AND u.source_node_uid = ( \
                   SELECT node_uid FROM nodes WHERE snapshot_uid = ?1 AND stable_key = ?2 LIMIT 1)"
            }
        }
    }
}

/// Which side of the symbol an inferred edge is on.
#[derive(Clone, Copy)]
enum InferredSide {
    /// Edges INTO the symbol (its inferred callers).
    Callers,
    /// Edges OUT of the symbol (its inferred callees).
    Callees,
}

impl StorageConnection {
    /// Fail with a named error when any CALLS edge of `snapshot_uid` carries a `resolution`
    /// outside the edge vocabulary `static | dynamic | inferred` (D-PSI-R1-VOCAB).
    ///
    /// ONE RULE for every read that splits the STORED CALLS rows by certainty (the certain
    /// caller/callee sets, the inferred remainder, the live dead-liveness and `map` CALLS-pair
    /// fallbacks, `path`'s hops, the rate counts): such a value is UNREADABLE — never certain,
    /// never inferred, and never silently left out of both (which would turn it into a measured
    /// absence: "0 callers", a complete rate). The read is refused with the count and one
    /// example value instead (RG-REQ-002-L04/L11). The persisted aggregates (the resolved-call
    /// count, the call degrees, the file pairs) are counted by the indexer from the resolver's
    /// own typed output, not from stored rows, and are not reads this check covers. No writer
    /// produces such a value; the check guards stores from any other source. `op` names the
    /// read in the error.
    pub(crate) fn reject_unreadable_call_resolutions(
        &self,
        snapshot_uid: &str,
        op: &str,
    ) -> Result<(), StorageError> {
        let (count, example): (i64, Option<String>) = self.connection().query_row(
            "SELECT COUNT(*), MIN(resolution) FROM edges \
             WHERE snapshot_uid = ?1 AND type = 'CALLS' \
               AND resolution NOT IN ('static', 'dynamic', 'inferred')",
            rusqlite::params![snapshot_uid],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if count == 0 {
            return Ok(());
        }
        Err(StorageError::SerializationError(format!(
            "{op}: {count} CALLS edge(s) of snapshot {snapshot_uid} carry an unreadable resolution \
             (e.g. {example:?}; readable: static | dynamic | inferred) — the certain and inferred \
             call sets cannot be told apart; run `rmap repo rebuild <path>`"
        )))
    }

    /// The call remainder of one symbol (see the module doc). `symbol_name` is the node's name
    /// (the naming test's segment).
    pub fn find_symbol_call_remainders(
        &self,
        snapshot_uid: &str,
        symbol_stable_key: &str,
        symbol_name: &str,
    ) -> Result<AgentCallRemainders, StorageError> {
        self.reject_unreadable_call_resolutions(snapshot_uid, "find_symbol_call_remainders")?;
        Ok(AgentCallRemainders {
            inferred_callers: self.inferred_calls(
                snapshot_uid,
                symbol_stable_key,
                InferredSide::Callers,
            )?,
            inferred_callees: self.inferred_calls(
                snapshot_uid,
                symbol_stable_key,
                InferredSide::Callees,
            )?,
            unresolved_naming: self.unresolved_counts(
                snapshot_uid,
                symbol_stable_key,
                symbol_name,
                UnresolvedScope::Naming,
            )?,
            unresolved_naming_sites: self.unresolved_sites(
                snapshot_uid,
                symbol_stable_key,
                symbol_name,
                UnresolvedScope::Naming,
            )?,
            unresolved_from: self.unresolved_counts(
                snapshot_uid,
                symbol_stable_key,
                symbol_name,
                UnresolvedScope::From,
            )?,
            unresolved_from_sites: self.unresolved_sites(
                snapshot_uid,
                symbol_stable_key,
                symbol_name,
                UnresolvedScope::From,
            )?,
        })
    }

    /// The inferred CALLS rows on one side of the symbol. The row names the OTHER end (the caller
    /// or the callee) with its module, and is anchored at the CALL SITE: the calling node's file
    /// and the edge's line (one source — RG-REQ-005-L08).
    fn inferred_calls(
        &self,
        snapshot_uid: &str,
        symbol_stable_key: &str,
        side: InferredSide,
    ) -> Result<Vec<AgentInferredCallRow>, StorageError> {
        let (other_end, focus_end) = match side {
            InferredSide::Callers => ("e.source_node_uid", "e.target_node_uid"),
            InferredSide::Callees => ("e.target_node_uid", "e.source_node_uid"),
        };
        let sql = format!(
            "SELECT other.stable_key, other.name, other.qualified_name, \
                    site_f.path, e.line_start, \
                    mod_n.qualified_name, mod_n.stable_key, e.extractor, e.metadata_json \
             FROM edges e \
             JOIN nodes other ON {other_end} = other.node_uid \
             JOIN nodes caller ON e.source_node_uid = caller.node_uid \
             LEFT JOIN files site_f ON caller.file_uid = site_f.file_uid \
             LEFT JOIN nodes file_node ON file_node.file_uid = other.file_uid \
                AND file_node.kind = 'FILE' AND file_node.snapshot_uid = e.snapshot_uid \
             LEFT JOIN edges own ON own.type = 'OWNS' \
                AND own.target_node_uid = file_node.node_uid AND own.snapshot_uid = e.snapshot_uid \
             LEFT JOIN nodes mod_n ON own.source_node_uid = mod_n.node_uid \
             WHERE e.snapshot_uid = ?1 AND e.type = 'CALLS' AND e.resolution = 'inferred' \
               AND {focus_end} = ( \
                 SELECT node_uid FROM nodes WHERE snapshot_uid = ?1 AND stable_key = ?2 LIMIT 1) \
             ORDER BY site_f.path, e.line_start, e.col_start, other.stable_key"
        );
        let mut stmt = self.connection().prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params![snapshot_uid, symbol_stable_key], |row| {
            let extractor: String = row.get(7)?;
            let metadata: Option<String> = row.get(8)?;
            let evidence = inferred_call_evidence(&extractor, metadata.as_deref());
            Ok(AgentInferredCallRow {
                stable_key: row.get(0)?,
                name: row.get(1)?,
                qualified_name: row.get(2)?,
                file: row.get(3)?,
                // A NULL or non-positive line is no anchor (never a fabricated `:0`).
                line: row
                    .get::<_, Option<i64>>(4)?
                    .filter(|v| *v > 0)
                    .map(|v| v as u64),
                module_path: row.get(5)?,
                module_stable_key: row.get(6)?,
                basis: evidence.basis,
                receiver: evidence.receiver,
                extractor: evidence.extractor,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    /// The unresolved CALLS of one scope, counted per basis code (ordered by basis code).
    fn unresolved_counts(
        &self,
        snapshot_uid: &str,
        symbol_stable_key: &str,
        symbol_name: &str,
        scope: UnresolvedScope,
    ) -> Result<Vec<AgentBasisCount>, StorageError> {
        let sql = format!(
            "SELECT u.basis_code, COUNT(*) FROM unresolved_edges u WHERE {} \
             GROUP BY u.basis_code ORDER BY u.basis_code",
            scope.predicate()
        );
        let mut stmt = self.connection().prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params![snapshot_uid, symbol_stable_key, symbol_name],
            |row| {
                Ok(AgentBasisCount {
                    basis_code: row.get(0)?,
                    count: row.get::<_, i64>(1)? as u64,
                })
            },
        )?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    /// The first [`SITES_LIMIT`] unresolved sites of one scope, ordered by (file, line, column,
    /// target key), each with its recorded candidate pool.
    fn unresolved_sites(
        &self,
        snapshot_uid: &str,
        symbol_stable_key: &str,
        symbol_name: &str,
        scope: UnresolvedScope,
    ) -> Result<Vec<AgentUnresolvedCallSite>, StorageError> {
        let sql = format!(
            "SELECT f.path, u.line_start, u.target_key, u.basis_code, u.metadata_json \
             FROM unresolved_edges u \
             JOIN nodes s ON u.source_node_uid = s.node_uid \
             LEFT JOIN files f ON s.file_uid = f.file_uid \
             WHERE {} \
             ORDER BY f.path, u.line_start, u.col_start, u.target_key \
             LIMIT {SITES_LIMIT}",
            scope.predicate()
        );
        let mut stmt = self.connection().prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params![snapshot_uid, symbol_stable_key, symbol_name],
            |row| {
                let metadata: Option<String> = row.get(4)?;
                let pool = site_pool(metadata.as_deref());
                let (candidate_reason, candidate_count, candidates) = match pool {
                    Some((reason, count, list)) => (Some(reason), count, list),
                    None => (None, None, None),
                };
                Ok(AgentUnresolvedCallSite {
                    file: row.get(0)?,
                    line: row
                        .get::<_, Option<i64>>(1)?
                        .filter(|v| *v > 0)
                        .map(|v| v as u64),
                    target_key: row.get(2)?,
                    basis: row.get(3)?,
                    candidate_reason,
                    candidate_count,
                    candidates,
                })
            },
        )?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inferred_call_evidence_absent_is_unrecorded_with_its_extractor() {
        for meta in [None, Some(""), Some("   ")] {
            let e = inferred_call_evidence("compiler-promotion:0.1.0", meta);
            assert_eq!(e.basis, "unrecorded", "{meta:?}");
            assert_eq!(e.receiver, None);
            assert_eq!(e.extractor, "compiler-promotion:0.1.0");
        }
    }

    #[test]
    fn inferred_call_evidence_present_valid_carries_basis_and_receiver() {
        let e = inferred_call_evidence(
            "python-core:0.2.0",
            Some(
                r#"{"basis":"receiver_untyped_name_only","receiver":"dependencies","candidates":["ListMixin.extend"]}"#,
            ),
        );
        assert_eq!(e.basis, "receiver_untyped_name_only");
        assert_eq!(e.receiver.as_deref(), Some("dependencies"));
        assert_eq!(e.extractor, "python-core:0.2.0");
    }

    #[test]
    fn inferred_call_evidence_present_malformed_is_unrecorded_with_its_extractor_never_an_error() {
        for meta in [
            // The legacy compiler-promotion:0.1.0 shape.
            r#"{"promotedFrom":"1d4279fa","receiverType":"ArtifactFamily","methodName":"all"}"#,
            r#"{"basis":7,"receiver":"x"}"#,
            r#"{"basis":"receiver_untyped_name_only"}"#,
            r#"["not","an","object"]"#,
            "{not json",
        ] {
            let e = inferred_call_evidence("compiler-promotion:0.1.0", Some(meta));
            assert_eq!(e.basis, "unrecorded", "{meta}");
            assert_eq!(e.receiver, None, "{meta}");
            assert_eq!(e.extractor, "compiler-promotion:0.1.0");
        }
    }

    fn fixture() -> StorageConnection {
        let s = StorageConnection::open_in_memory().unwrap();
        let c = s.connection();
        c.execute_batch(
            "INSERT INTO repos (repo_uid, name, root_path, created_at) VALUES ('r','r','/r','t');
             INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at) VALUES ('s','r','full','ready','t');
             INSERT INTO files (file_uid, repo_uid, path, language, is_test) VALUES
               ('r:a.py','r','a.py','python',0), ('r:b.py','r','b.py','python',0);
             INSERT INTO nodes (node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype, name, qualified_name, file_uid, line_start)
               VALUES ('ext','s','r','r:m.py#ListMixin.extend:SYMBOL:METHOD','SYMBOL','METHOD','extend','ListMixin.extend','r:a.py',192),
                      ('f','s','r','r:b.py#f:SYMBOL:FUNCTION','SYMBOL','FUNCTION','f','f','r:b.py',1),
                      ('g','s','r','r:b.py#g:SYMBOL:FUNCTION','SYMBOL','FUNCTION','g','g','r:b.py',40);",
        )
        .unwrap();
        s
    }

    fn unresolved(
        s: &StorageConnection,
        uid: &str,
        src: &str,
        key: &str,
        line: i64,
        basis: &str,
        meta: Option<&str>,
    ) {
        s.connection()
            .execute(
                "INSERT INTO unresolved_edges (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_key, type, resolution, extractor, line_start, col_start, metadata_json, category, classification, classifier_version, basis_code, observed_at)
                 VALUES (?1,'s','r',?2,?3,'CALLS','static','python-core:0.2.0',?4,0,?5,'calls_obj_method_needs_type_info','unknown',6,?6,'t')",
                rusqlite::params![uid, src, key, line, meta, basis],
            )
            .unwrap();
    }

    #[test]
    fn unresolved_calls_naming_matches_the_last_dot_or_colon_segment_only() {
        let s = fixture();
        unresolved(
            &s,
            "u1",
            "f",
            "self.extend",
            3,
            "self_call_hierarchy_miss",
            None,
        );
        unresolved(&s, "u2", "f", "a.b.extend", 4, "no_supporting_signal", None);
        unresolved(&s, "u3", "f", "ns::extend", 5, "no_supporting_signal", None);
        unresolved(&s, "u4", "f", "extend", 6, "no_supporting_signal", None);
        // Not naming it: a longer segment, a middle segment, a prefix.
        unresolved(
            &s,
            "u5",
            "f",
            "self.xextend",
            7,
            "no_supporting_signal",
            None,
        );
        unresolved(&s, "u6", "f", "a.extend.b", 8, "no_supporting_signal", None);
        unresolved(&s, "u7", "f", "extend_all", 9, "no_supporting_signal", None);
        let r = s
            .find_symbol_call_remainders("s", "r:m.py#ListMixin.extend:SYMBOL:METHOD", "extend")
            .unwrap();
        assert_eq!(
            r.unresolved_naming,
            vec![
                AgentBasisCount {
                    basis_code: "no_supporting_signal".into(),
                    count: 3
                },
                AgentBasisCount {
                    basis_code: "self_call_hierarchy_miss".into(),
                    count: 1
                },
            ]
        );
        assert_eq!(r.unresolved_naming_total(), 4);
        assert_eq!(r.unresolved_naming_sites.len(), 4);
    }

    #[test]
    fn unresolved_calls_from_counts_only_rows_whose_source_is_the_symbol() {
        let s = fixture();
        unresolved(&s, "u1", "f", "x.values", 3, "no_supporting_signal", None);
        unresolved(&s, "u2", "f", "y.keys", 4, "self_call_hierarchy_miss", None);
        unresolved(&s, "u3", "g", "z.values", 5, "no_supporting_signal", None);
        let r = s
            .find_symbol_call_remainders("s", "r:b.py#f:SYMBOL:FUNCTION", "f")
            .unwrap();
        assert_eq!(r.unresolved_from_total(), 2);
        assert_eq!(
            r.unresolved_from_sites
                .iter()
                .map(|x| x.target_key.as_str())
                .collect::<Vec<_>>(),
            vec!["x.values", "y.keys"]
        );
    }

    #[test]
    fn unresolved_sites_are_ordered_capped_and_carry_their_recorded_candidates() {
        let s = fixture();
        // 25 rows naming `extend`: the sites stop at 20, the count does not.
        for i in 0..25 {
            unresolved(
                &s,
                &format!("u{i:02}"),
                "f",
                "q.extend",
                100 + i,
                "no_supporting_signal",
                None,
            );
        }
        // An ambiguous pool of 60 candidates on the first line.
        let pool: Vec<String> = (0..60).rev().map(|i| format!("C{i:02}.extend")).collect();
        let meta =
            serde_json::json!({"nameOnlyCandidates": pool, "nameOnlyReason": "ambiguous_name"})
                .to_string();
        unresolved(
            &s,
            "a0",
            "f",
            "p.extend",
            1,
            "no_supporting_signal",
            Some(&meta),
        );
        // A malformed pool and an MRO pool.
        unresolved(
            &s,
            "a1",
            "f",
            "self.extend",
            2,
            "no_supporting_signal",
            Some(r#"{"nameOnlyCandidates":"x","nameOnlyReason":"ambiguous_name"}"#),
        );
        unresolved(
            &s,
            "a2",
            "g",
            "self.extend",
            50,
            "self_call_ambiguous_mro",
            Some(r#"{"mroCandidates":["B.extend","A.extend"]}"#),
        );
        let r = s
            .find_symbol_call_remainders("s", "r:m.py#ListMixin.extend:SYMBOL:METHOD", "extend")
            .unwrap();
        assert_eq!(r.unresolved_naming_total(), 28);
        assert_eq!(r.unresolved_naming_sites.len(), SITES_LIMIT);
        let first = &r.unresolved_naming_sites[0];
        assert_eq!(
            (first.line, first.target_key.as_str()),
            (Some(1), "p.extend")
        );
        assert_eq!(first.candidate_reason.as_deref(), Some("ambiguous_name"));
        assert_eq!(first.candidate_count, Some(60));
        let names = first.candidates.as_ref().unwrap();
        assert_eq!(names.len(), CANDIDATES_LIMIT);
        assert_eq!(names[0], "C00.extend", "sorted");
        let second = &r.unresolved_naming_sites[1];
        assert_eq!(second.candidate_reason.as_deref(), Some("unreadable"));
        assert_eq!(
            (second.candidate_count, second.candidates.clone()),
            (None, None)
        );
        let mro = r
            .unresolved_naming_sites
            .iter()
            .find(|x| x.line == Some(50))
            .expect("the MRO site (b.py:50) is among the first 20");
        assert_eq!(
            mro.candidate_reason.as_deref(),
            Some("self_call_ambiguous_mro")
        );
        assert_eq!(
            mro.candidates.as_deref(),
            Some(&["A.extend".to_string(), "B.extend".to_string()][..])
        );
        // A site without a recorded pool carries no candidate fields.
        let plain = r
            .unresolved_naming_sites
            .iter()
            .find(|x| x.line == Some(100))
            .unwrap();
        assert_eq!(plain.candidate_reason, None);
    }

    /// PYTHON-RECEIVER-BINDING-1 review F-2 (RG-REQ-002-L04/L11, D-PSI-R1-VOCAB): a stored CALLS
    /// `resolution` outside `static | dynamic | inferred` is UNREADABLE. Every public read that
    /// splits CALLS by certainty refuses it with the named error — none answers "0 callers", a
    /// dead symbol or a complete-looking rate — and the same store with the value corrected
    /// answers every read.
    #[test]
    fn unreadable_call_resolution_is_refused_by_every_read_that_splits_calls_by_certainty() {
        use repo_graph_agent::AgentStorageRead;
        use repo_graph_trust::storage_port::TrustStorageRead;

        let s = fixture();
        s.connection()
            .execute_batch(
                "INSERT INTO edges (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_node_uid, type, resolution, extractor, line_start, col_start)
                 VALUES ('e1','s','r','f','ext','CALLS','resolved','test',5,4),
                        ('e2','s','r','g','ext','CALLS','static','test',41,4);",
            )
            .unwrap();
        const EXT: &str = "r:m.py#ListMixin.extend:SYMBOL:METHOD";
        const F: &str = "r:b.py#f:SYMBOL:FUNCTION";

        fn refused<T: std::fmt::Debug, E: std::fmt::Display>(read: &str, r: Result<T, E>) {
            match r {
                Ok(v) => panic!("{read} answered {v:?} over an unreadable resolution"),
                Err(e) => {
                    let msg = e.to_string();
                    assert!(
                        msg.contains("unreadable resolution") && msg.contains("\"resolved\""),
                        "{read}: the error names the unreadable value: {msg}"
                    );
                }
            }
        }
        fn answered<T, E: std::fmt::Display>(read: &str, r: Result<T, E>) {
            if let Err(e) = r {
                panic!("{read} refused a readable store: {e}");
            }
        }

        let check = |expect_refusal: bool| {
            let out: Vec<(&str, Result<(), String>)> = vec![
                (
                    "find_symbol_callers",
                    AgentStorageRead::find_symbol_callers(&s, "s", EXT)
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                ),
                (
                    "find_symbol_callees",
                    AgentStorageRead::find_symbol_callees(&s, "s", F)
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                ),
                (
                    "find_symbol_call_remainders",
                    s.find_symbol_call_remainders("s", EXT, "extend")
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                ),
                (
                    "find_dead_nodes_in_path",
                    AgentStorageRead::find_dead_nodes_in_path(&s, "s", "r", "b")
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                ),
                (
                    "find_dead_nodes_in_file",
                    AgentStorageRead::find_dead_nodes_in_file(&s, "s", "r", "b.py")
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                ),
                (
                    "find_direct_callers",
                    s.find_direct_callers("s", EXT, &["CALLS"])
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                ),
                (
                    "find_direct_callees",
                    s.find_direct_callees("s", F, &["CALLS"])
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                ),
                (
                    "find_dead_nodes",
                    s.find_dead_nodes("s", "r", None)
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                ),
                (
                    "find_shortest_path",
                    s.find_shortest_path("s", F, EXT, 8, false)
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                ),
                (
                    "map_resolved_dep_edges_in_path",
                    s.map_resolved_dep_edges_in_path("s", "")
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                ),
                (
                    "query_call_resolution_total",
                    s.query_call_resolution_total("s")
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                ),
                (
                    "query_call_resolution_by_language",
                    s.query_call_resolution_by_language("s")
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                ),
                (
                    "query_call_resolution_by_module",
                    s.query_call_resolution_by_module("s")
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                ),
                (
                    "count_call_edges_by_certainty",
                    TrustStorageRead::count_call_edges_by_certainty(&s, "s")
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                ),
            ];
            for (read, r) in out {
                if expect_refusal {
                    refused(read, r);
                } else {
                    answered(read, r);
                }
            }
        };
        check(true);

        // A read that does not split CALLS (IMPORTS rows only) is not refused.
        answered(
            "find_direct_callers IMPORTS",
            s.find_direct_callers("s", EXT, &["IMPORTS"]),
        );

        // The same store with the value in the vocabulary answers every read.
        s.connection()
            .execute(
                "UPDATE edges SET resolution = 'static' WHERE edge_uid = 'e1'",
                [],
            )
            .unwrap();
        check(false);
    }
}
