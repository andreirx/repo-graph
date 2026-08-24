//! HTTP-BOUNDARY-1: read `channel_kind = 'http'` boundary surfaces for
//! route-based linking.
//!
//! Crate-private helper backing
//! `BoundaryInteractionReadPort::query_http_surfaces` (in
//! `boundary_interaction_read_impl.rs`). Kept in its own module so neither of
//! the two already-over-500-line storage files
//! (`boundary_interaction_read_impl.rs`, `grpc_impl_hint_port_impl.rs`) grows
//! further, and so the HTTP-surface SQL + `evidence_json` parsing stay one
//! cohesive concern.
//!
//! HTTP surfaces have no proto `boundary_contracts` association to join on —
//! their (method, route) ride in `evidence_json`, so we parse them out into the
//! raw `HttpSurfaceRow` the pure matcher
//! (`repo_graph_boundary_interaction::http_link::find_http_links`) consumes.

use repo_graph_boundary_interaction::HttpSurfaceRow;
use rusqlite::Connection;

/// Query all `channel_kind = 'http'` surfaces for a snapshot, parsing the
/// (method, route) out of each surface's `evidence_json`. A missing/null
/// `route` (dynamic URL) yields `None` — never fabricated.
pub(crate) fn query_http_surfaces(
    conn: &Connection,
    snapshot_uid: &str,
) -> Result<Vec<HttpSurfaceRow>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        r#"
        SELECT surface_uid, direction, source_file, symbol_stable_key, evidence_json
        FROM boundary_interaction_surfaces
        WHERE snapshot_uid = ? AND channel_kind = 'http'
        "#,
    )?;

    let rows = stmt.query_map(rusqlite::params![snapshot_uid], |row| {
        let surface_uid: String = row.get(0)?;
        let direction: String = row.get(1)?;
        let source_file: String = row.get(2)?;
        let symbol_stable_key: String = row.get(3)?;
        let evidence_json: String = row.get(4)?;
        Ok((
            surface_uid,
            direction,
            source_file,
            symbol_stable_key,
            evidence_json,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (surface_uid, direction, source_file, symbol_stable_key, evidence_json) = row?;
        // review-5 item 3: a surface whose `evidence_json` cannot be parsed (or
        // lacks the required `httpMethod`) is CORRUPT, not "a dynamic-route
        // consumer". Silently substituting (UNKNOWN, None) would classify it as
        // dynamic data (STANDING HONESTY RULE 1 forbids this). Propagate a typed
        // conversion failure so the caller degrades the whole HTTP map to
        // UNKNOWN (via the collected `surface_query_error`) rather than serving a
        // false-complete map. Our writer always emits valid evidence, so this
        // only fires on genuine corruption.
        let (http_method, route) = parse_http_evidence(&evidence_json).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                4, // evidence_json column
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("http surface {surface_uid} evidence_json {e}"),
                )),
            )
        })?;
        out.push(HttpSurfaceRow {
            surface_uid,
            direction,
            http_method,
            route,
            source_file,
            symbol_stable_key,
        });
    }
    Ok(out)
}

/// A malformed HTTP surface `evidence_json` — distinct from a valid one with a
/// dynamic (`null`) route.
#[derive(Debug, PartialEq)]
enum HttpEvidenceError {
    /// The `evidence_json` column is not valid JSON.
    NotJson,
    /// Valid JSON but the required `httpMethod` string is absent.
    MissingMethod,
    /// Valid JSON but `route` is present as a non-string, non-null value.
    RouteNotString,
}

impl std::fmt::Display for HttpEvidenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpEvidenceError::NotJson => write!(f, "is not valid JSON"),
            HttpEvidenceError::MissingMethod => write!(f, "is missing the httpMethod field"),
            HttpEvidenceError::RouteNotString => write!(f, "has a non-string route field"),
        }
    }
}

/// Parse `(httpMethod, route)` out of an HTTP surface's `evidence_json`.
///
/// - a missing/`null` `route` (dynamic URL) yields `route: None` — the ONE
///   legitimate absence, never fabricated;
/// - a parse failure, a missing `httpMethod`, or a non-string `route` is
///   CORRUPTION → `Err`, never silently coerced to (UNKNOWN, None).
fn parse_http_evidence(evidence_json: &str) -> Result<(String, Option<String>), HttpEvidenceError> {
    let value: serde_json::Value =
        serde_json::from_str(evidence_json).map_err(|_| HttpEvidenceError::NotJson)?;
    let method = value
        .get("httpMethod")
        .and_then(|m| m.as_str())
        .ok_or(HttpEvidenceError::MissingMethod)?
        .to_string();
    // route: absent key OR explicit JSON null → dynamic (None). A present
    // non-string/non-null value is malformed.
    let route = match value.get("route") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(_) => return Err(HttpEvidenceError::RouteNotString),
    };
    Ok((method, route))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StorageConnection;

    /// In-memory storage with a repo + ready snapshot, for the surface/link
    /// round-trip (moved here from `grpc_impl_hint_port_impl.rs` per review-5
    /// item 4 — the HTTP storage concern lives with this module).
    fn setup_db() -> StorageConnection {
        let mut conn = StorageConnection::open_in_memory().unwrap();
        conn.connection_mut()
            .execute_batch(
                r#"
                INSERT INTO repos (repo_uid, name, root_path, created_at)
                VALUES ('r1', 'test', '/test', datetime('now'));
                INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at)
                VALUES ('s1', 'r1', 'full', 'ready', datetime('now'));
                "#,
            )
            .unwrap();
        conn
    }

    /// An http surface pair round-trips through `query_http_surfaces`, and an
    /// http link with an EMPTY `contract_element_uid` persists with SQL NULL (FK
    /// satisfied) via the public `GrpcLinkStorePort` write path.
    #[test]
    fn http_surfaces_and_null_contract_link_round_trip() {
        use repo_graph_boundary_interaction::BoundaryInteractionReadPort;
        use repo_graph_indexer::storage_port::{BoundaryInteractionLinkInput, GrpcLinkStorePort};
        let mut conn = setup_db();
        let insert_surface = |conn: &mut StorageConnection, uid: &str, dir: &str, ev: &str| {
            conn.connection_mut()
                .execute(
                    r#"INSERT INTO boundary_interaction_surfaces (
                        surface_uid, snapshot_uid, repo_uid, boundary_scope, channel_kind,
                        direction, transport_class, protocol, protocol_family,
                        interaction_pattern, endpoint_locality, symbol_stable_key, source_file,
                        line_start, line_end, col_start, col_end, extractor, basis, confidence,
                        evidence_json
                    ) VALUES (?, 's1', 'r1', 'unknown', 'http', ?, 'custom_protocol', 'http',
                        'http', 'request_response', 'unknown', 'r1:f#s:SYMBOL:METHOD',
                        'f.ts', 1, 1, 0, 0, 'http-boundary:1.0', 'api_call', 0.95, ?)"#,
                    rusqlite::params![uid, dir, ev],
                )
                .unwrap();
        };
        insert_surface(
            &mut conn,
            "p1",
            "provider",
            r#"{"httpMethod":"GET","route":"/api/v2/clients/{id}"}"#,
        );
        insert_surface(
            &mut conn,
            "c1",
            "consumer",
            r#"{"httpMethod":"GET","route":"/api/v2/clients/{param}"}"#,
        );

        let surfaces = conn.query_http_surfaces("s1").unwrap();
        assert_eq!(surfaces.len(), 2);
        let provider = surfaces.iter().find(|s| s.direction == "provider").unwrap();
        assert_eq!(provider.http_method, "GET");
        assert_eq!(provider.route.as_deref(), Some("/api/v2/clients/{id}"));

        let link = BoundaryInteractionLinkInput {
            link_uid: "http-link-1".to_string(),
            snapshot_uid: "s1".to_string(),
            provider_surface_uid: "p1".to_string(),
            consumer_surface_uid: "c1".to_string(),
            link_kind: "http_route_match".to_string(),
            contract_element_uid: String::new(), // -> NULL
            match_basis: "route_and_method".to_string(),
            confidence: 0.75,
            evidence_json: "{}".to_string(),
            provenance: None,
        };
        let inserted = conn.insert_boundary_interaction_links(&[link]).unwrap();
        assert_eq!(inserted, 1);

        let is_null: bool = conn
            .connection()
            .query_row(
                "SELECT contract_element_uid IS NULL FROM boundary_interaction_links WHERE link_uid = 'http-link-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(is_null, "http link contract_element_uid must be SQL NULL");
    }

    #[test]
    fn parses_method_and_route() {
        let (m, r) = parse_http_evidence(r#"{"httpMethod":"GET","route":"/api/v2/clients/{id}"}"#)
            .expect("valid evidence");
        assert_eq!(m, "GET");
        assert_eq!(r.as_deref(), Some("/api/v2/clients/{id}"));
    }

    #[test]
    fn null_route_stays_none() {
        // A dynamic URL persists `route: null` — the one legitimate None.
        let (m, r) = parse_http_evidence(r#"{"httpMethod":"POST","route":null}"#)
            .expect("valid dynamic-route evidence");
        assert_eq!(m, "POST");
        assert_eq!(r, None);
    }

    #[test]
    fn missing_route_key_is_dynamic_none() {
        let (m, r) =
            parse_http_evidence(r#"{"httpMethod":"GET"}"#).expect("route key may be absent");
        assert_eq!(m, "GET");
        assert_eq!(r, None);
    }

    #[test]
    fn missing_method_is_typed_error_not_fabricated() {
        // review-5 item 3: `{}` lacks httpMethod → a typed failure, NOT
        // (UNKNOWN, None) classified as a dynamic consumer.
        assert_eq!(
            parse_http_evidence("{}"),
            Err(HttpEvidenceError::MissingMethod)
        );
    }

    #[test]
    fn malformed_json_is_typed_error_not_fabricated() {
        // review-5 item 3: corrupt evidence propagates, never coerced to UNKNOWN.
        assert_eq!(
            parse_http_evidence("not json"),
            Err(HttpEvidenceError::NotJson)
        );
    }

    #[test]
    fn non_string_route_is_typed_error() {
        assert_eq!(
            parse_http_evidence(r#"{"httpMethod":"GET","route":123}"#),
            Err(HttpEvidenceError::RouteNotString)
        );
    }
}
