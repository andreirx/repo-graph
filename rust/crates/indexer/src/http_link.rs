//! HTTP-BOUNDARY-1: index-time orchestration of route-template-aware
//! provider↔consumer linking.
//!
//! Sibling of `grpc_link` (GR-3A) — same "discovery, not connection-proof"
//! posture. The PURE matcher ([`find_http_links`], [`route_matches`]) and its
//! raw DTOs ([`HttpSurfaceRow`], [`HttpLink`], [`UnlinkedCounts`]) live in the
//! `repo-graph-boundary-interaction` policy crate so BOTH this index-time
//! linker and the read-time unlinked-counts renderer in `daemon-runtime` call
//! ONE matcher (operator ruling 2026-08-24). This module owns only the
//! index-time ORCHESTRATION: read the http surfaces via the read port, run the
//! pure matcher, and persist the unambiguous links via the write port.

use artifact_contracts::{Provenance, ProvenanceAnchor};
use repo_graph_boundary_interaction::{find_http_links, BoundaryInteractionReadPort};
use sha2::{Digest, Sha256};

use crate::storage_port::{BoundaryInteractionLinkInput, GrpcLinkStorePort};

/// Deterministic link UID for an HTTP route link.
pub fn generate_http_link_uid(
    snapshot_uid: &str,
    provider_surface_uid: &str,
    consumer_surface_uid: &str,
    http_method: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"http_link:");
    hasher.update(snapshot_uid.as_bytes());
    hasher.update(b":");
    hasher.update(provider_surface_uid.as_bytes());
    hasher.update(b":");
    hasher.update(consumer_surface_uid.as_bytes());
    hasher.update(b":");
    hasher.update(http_method.as_bytes());
    let hash = hasher.finalize();
    format!(
        "http-link-{:x}",
        &hash[..8].iter().fold(0u64, |acc, &b| acc << 8 | b as u64)
    )
}

/// Result of running HTTP link detection (attached to `IndexResult`).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpLinkResult {
    /// Number of links emitted (unambiguous route matches).
    pub links_emitted: usize,
    /// HTTP provider surfaces read.
    pub providers_queried: usize,
    /// HTTP consumer surfaces read.
    pub consumers_queried: usize,
    /// Consumers left unlinked because their route matched >1 provider.
    pub ambiguous_consumers: usize,
    /// Consumers left unlinked because their route matched no provider.
    pub unmatched_consumers: usize,
    /// Consumers with a dynamic/unreadable route (never linkable).
    pub dynamic_route_consumers: usize,
    /// Error reading http surfaces.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface_query_error: Option<String>,
    /// Error persisting links.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_storage_error: Option<String>,
}

impl HttpLinkResult {
    /// Whether any error occurred during detection.
    pub fn has_error(&self) -> bool {
        self.surface_query_error.is_some() || self.link_storage_error.is_some()
    }
}

/// Run HTTP link detection for a snapshot.
///
/// Reads all `channel_kind = 'http'` surfaces, links unambiguous
/// (method, route) matches, and persists them to `boundary_interaction_links`
/// (reusing the link write path; `contract_element_uid` empty → SQL NULL).
/// Errors are collected, not fail-fast.
pub fn run_http_link_detection<S>(storage: &mut S, snapshot_uid: &str) -> HttpLinkResult
where
    S: BoundaryInteractionReadPort + GrpcLinkStorePort,
    <S as GrpcLinkStorePort>::Error: ToString,
{
    let mut result = HttpLinkResult::default();

    let surfaces = match storage.query_http_surfaces(snapshot_uid) {
        Ok(s) => s,
        Err(e) => {
            result.surface_query_error = Some(e.to_string());
            return result;
        }
    };

    result.providers_queried = surfaces
        .iter()
        .filter(|s| s.direction == "provider")
        .count();
    result.consumers_queried = surfaces
        .iter()
        .filter(|s| s.direction == "consumer")
        .count();

    if result.providers_queried == 0 || result.consumers_queried == 0 {
        return result;
    }

    let (links, counts) = find_http_links(&surfaces);
    result.ambiguous_consumers = counts.ambiguous;
    result.unmatched_consumers = counts.unmatched;
    result.dynamic_route_consumers = counts.dynamic_route;

    if links.is_empty() {
        return result;
    }

    let link_inputs: Vec<BoundaryInteractionLinkInput> = links
        .iter()
        .map(|link| {
            let link_uid = generate_http_link_uid(
                snapshot_uid,
                &link.provider_surface_uid,
                &link.consumer_surface_uid,
                &link.http_method,
            );
            let evidence = serde_json::json!({
                "httpMethod": link.http_method,
                "providerRoute": link.provider_route,
                "consumerRoute": link.consumer_route,
                "provider_file": link.provider_source_file,
                "consumer_file": link.consumer_source_file,
                "match_basis": "route_and_method",
            });
            let provenance = Provenance::from_layer0_items(vec![
                ProvenanceAnchor::new("BoundaryInteractionSurfaces", &link.provider_stable_key),
                ProvenanceAnchor::new("BoundaryInteractionSurfaces", &link.consumer_stable_key),
            ])
            .with_extractor("http_link:1.0");

            BoundaryInteractionLinkInput {
                link_uid,
                snapshot_uid: snapshot_uid.to_string(),
                provider_surface_uid: link.provider_surface_uid.clone(),
                consumer_surface_uid: link.consumer_surface_uid.clone(),
                link_kind: "http_route_match".to_string(),
                // No contract element for HTTP — empty maps to SQL NULL.
                contract_element_uid: String::new(),
                match_basis: "route_and_method".to_string(),
                // Hint-grade: a route match is weaker evidence than a shared
                // proto contract (gRPC uses 0.80).
                confidence: 0.75,
                evidence_json: evidence.to_string(),
                provenance: Some(provenance),
            }
        })
        .collect();

    match storage.insert_boundary_interaction_links(&link_inputs) {
        Ok(count) => result.links_emitted = count,
        Err(e) => {
            result.link_storage_error = Some(e.to_string());
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // The pure matcher's tests (route templating, ambiguous/unmatched/dynamic
    // outcomes) live with the matcher in `repo-graph-boundary-interaction`
    // (`http_link` module). This module owns only the index-time orchestration;
    // its persistence-side concern is the deterministic link UID.

    #[test]
    fn link_uid_is_deterministic() {
        let a = generate_http_link_uid("s", "p", "c", "GET");
        let b = generate_http_link_uid("s", "p", "c", "GET");
        assert_eq!(a, b);
        assert!(a.starts_with("http-link-"));
        assert_ne!(a, generate_http_link_uid("s", "p", "c", "POST"));
    }

    // ── Degradation collection (review-3 item 1) ──────────────────────────
    //
    // `run_http_link_detection` COLLECTS (never fail-fast) a surface-query or a
    // link-write failure into the result; the compose postpass then propagates
    // `has_error()` so the partial HTTP facts are isolated. These tests inject
    // each failure at the two ports and assert it lands in the right field. The
    // read port's other four methods are unused by the linker, so the fake
    // panics if they are reached — proving the linker touches only these paths.

    use crate::storage_port::BoundaryInteractionLinkInput;
    use repo_graph_boundary_interaction::{
        BoundaryInteractionDetail, BoundaryInteractionFilter, BoundaryInteractionLinkFilter,
        BoundaryInteractionLinkListItem, BoundaryInteractionListItem, BoundaryInteractionReadError,
        BoundaryInteractionSummary, HttpSurfaceRow,
    };

    struct FakePorts {
        surfaces: Result<Vec<HttpSurfaceRow>, BoundaryInteractionReadError>,
        insert: Result<usize, String>,
    }

    impl BoundaryInteractionReadPort for FakePorts {
        fn list_boundary_interactions(
            &self,
            _snapshot_uid: &str,
            _filter: &BoundaryInteractionFilter,
        ) -> Result<Vec<BoundaryInteractionListItem>, BoundaryInteractionReadError> {
            unimplemented!("not used by run_http_link_detection")
        }
        fn get_boundary_interaction_detail(
            &self,
            _surface_uid: &str,
        ) -> Result<Option<BoundaryInteractionDetail>, BoundaryInteractionReadError> {
            unimplemented!("not used by run_http_link_detection")
        }
        fn get_boundary_interaction_summary(
            &self,
            _snapshot_uid: &str,
        ) -> Result<BoundaryInteractionSummary, BoundaryInteractionReadError> {
            unimplemented!("not used by run_http_link_detection")
        }
        fn list_boundary_interaction_links(
            &self,
            _snapshot_uid: &str,
            _filter: &BoundaryInteractionLinkFilter,
        ) -> Result<Vec<BoundaryInteractionLinkListItem>, BoundaryInteractionReadError> {
            unimplemented!("not used by run_http_link_detection")
        }
        fn query_http_surfaces(
            &self,
            _snapshot_uid: &str,
        ) -> Result<Vec<HttpSurfaceRow>, BoundaryInteractionReadError> {
            self.surfaces.clone()
        }
    }

    impl GrpcLinkStorePort for FakePorts {
        type Error = String;
        fn insert_boundary_interaction_links(
            &mut self,
            _links: &[BoundaryInteractionLinkInput],
        ) -> Result<usize, String> {
            self.insert.clone()
        }
    }

    fn row(uid: &str, direction: &str, method: &str, route: Option<&str>) -> HttpSurfaceRow {
        HttpSurfaceRow {
            surface_uid: uid.to_string(),
            direction: direction.to_string(),
            http_method: method.to_string(),
            route: route.map(str::to_string),
            source_file: format!("{uid}.rs"),
            symbol_stable_key: format!("r:{uid}:FILE"),
            is_test: None,
            framework: None,
            route_unknown_reason: None,
        }
    }

    /// A matching provider+consumer pair — `find_http_links` emits one link, so
    /// the link-write path actually runs.
    fn matching_pair() -> Vec<HttpSurfaceRow> {
        vec![
            row("p1", "provider", "GET", Some("/offers/{id}")),
            row("c1", "consumer", "GET", Some("/offers/123")),
        ]
    }

    #[test]
    fn surface_query_failure_is_collected() {
        let mut ports = FakePorts {
            surfaces: Err(BoundaryInteractionReadError::Storage(
                "db locked".to_string(),
            )),
            insert: Ok(0),
        };
        let result = run_http_link_detection(&mut ports, "snap-1");
        assert!(result.has_error());
        assert!(result
            .surface_query_error
            .as_deref()
            .unwrap()
            .contains("db locked"));
        assert!(result.link_storage_error.is_none());
        assert_eq!(result.links_emitted, 0);
    }

    #[test]
    fn link_storage_failure_is_collected() {
        let mut ports = FakePorts {
            surfaces: Ok(matching_pair()),
            insert: Err("write failed".to_string()),
        };
        let result = run_http_link_detection(&mut ports, "snap-1");
        assert!(result.has_error());
        assert!(result
            .link_storage_error
            .as_deref()
            .unwrap()
            .contains("write failed"));
        assert!(result.surface_query_error.is_none());
        assert_eq!(result.links_emitted, 0);
    }

    #[test]
    fn clean_run_collects_no_error() {
        let mut ports = FakePorts {
            surfaces: Ok(matching_pair()),
            insert: Ok(1),
        };
        let result = run_http_link_detection(&mut ports, "snap-1");
        assert!(!result.has_error());
        assert_eq!(result.links_emitted, 1);
        assert_eq!(result.providers_queried, 1);
        assert_eq!(result.consumers_queried, 1);
    }
}
