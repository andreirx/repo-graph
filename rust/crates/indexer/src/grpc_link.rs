//! gRPC provider/consumer contract-based linking (GR-3A).
//!
//! Links provider and consumer boundary surfaces when both reference the
//! same proto service contract. This is a **discovery slice**, not a
//! connection-proof slice.
//!
//! # What GR-3A surfaces
//!
//! - "provider surface X and consumer surface Y appear to belong to the
//!    same proto service contract"
//! - candidate link for agent inspection
//!
//! # What GR-3A does NOT claim
//!
//! - definite network path
//! - deployed communication proof
//! - live runtime topology
//!
//! # Detection logic
//!
//! For each provider surface (GR-1A) and consumer surface (GR-2A) that:
//! - both have `transport_class = schema_rpc`
//! - both have `boundary_contracts` associations with `contract_kind = grpc_service`
//! - both point to the same `contract_element_uid` (proto service)
//!
//! Emit a `boundary_interaction_link` with:
//! - `link_kind = contract_match_only`
//! - `match_basis = contract`
//! - `confidence = 0.80` (hint-grade)
//!
//! # Evidence chain
//!
//! ```text
//! GR-1A surface (provider, grpc_service contract)
//!     ↓ (contract_element_uid = X)
//! Proto service X
//!     ↑ (contract_element_uid = X)
//! GR-2A surface (consumer, grpc_service contract)
//!     ↓
//! boundary_interaction_link (provider ↔ consumer)
//! ```

use std::collections::HashMap;

use sha2::{Digest, Sha256};

use crate::storage_port::{
    BoundaryInteractionLinkInput, GrpcLinkReadPort, GrpcLinkStorePort, SurfaceWithContract,
};

/// A detected provider/consumer link.
#[derive(Debug, Clone)]
pub struct GrpcLink {
    /// Provider surface UID
    pub provider_surface_uid: String,
    /// Consumer surface UID
    pub consumer_surface_uid: String,
    /// Contract element UID (proto service)
    pub contract_element_uid: String,
    /// Contract full name (e.g., "helloworld.Greeter")
    pub contract_full_name: String,
    /// Provider source file (for evidence)
    pub provider_source_file: String,
    /// Consumer source file (for evidence)
    pub consumer_source_file: String,
    /// Provider basis (e.g., "impl_extension")
    pub provider_basis: String,
    /// Consumer basis (e.g., "stub_creation")
    pub consumer_basis: String,
}

/// Find links between provider and consumer surfaces that share the same contract.
///
/// For each (provider, consumer) pair with matching `contract_element_uid`:
/// - Generate a link candidate
///
/// This is N×M complexity but typically small N and M (few surfaces per contract).
pub fn find_grpc_links(
    providers: &[SurfaceWithContract],
    consumers: &[SurfaceWithContract],
) -> Vec<GrpcLink> {
    // Index providers by contract_element_uid
    let mut providers_by_contract: HashMap<&str, Vec<&SurfaceWithContract>> = HashMap::new();
    for provider in providers {
        providers_by_contract
            .entry(&provider.contract_element_uid)
            .or_default()
            .push(provider);
    }

    let mut links = Vec::new();

    for consumer in consumers {
        // Find providers with matching contract
        if let Some(matching_providers) = providers_by_contract.get(consumer.contract_element_uid.as_str()) {
            for provider in matching_providers {
                links.push(GrpcLink {
                    provider_surface_uid: provider.surface_uid.clone(),
                    consumer_surface_uid: consumer.surface_uid.clone(),
                    contract_element_uid: consumer.contract_element_uid.clone(),
                    contract_full_name: consumer.contract_full_name.clone(),
                    provider_source_file: provider.source_file.clone(),
                    consumer_source_file: consumer.source_file.clone(),
                    provider_basis: provider.basis.clone(),
                    consumer_basis: consumer.basis.clone(),
                });
            }
        }
    }

    links
}

/// Generate a deterministic link UID.
///
/// Identity includes: snapshot, provider_surface, consumer_surface, contract_element.
/// The contract_element_uid is critical: the same provider/consumer pair may share
/// multiple different gRPC service contracts. Each (provider, consumer, contract)
/// triple must produce a distinct link.
pub fn generate_link_uid(
    snapshot_uid: &str,
    provider_surface_uid: &str,
    consumer_surface_uid: &str,
    contract_element_uid: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"grpc_link:");
    hasher.update(snapshot_uid.as_bytes());
    hasher.update(b":");
    hasher.update(provider_surface_uid.as_bytes());
    hasher.update(b":");
    hasher.update(consumer_surface_uid.as_bytes());
    hasher.update(b":");
    hasher.update(contract_element_uid.as_bytes());
    let hash = hasher.finalize();
    format!(
        "grpc-link-{:x}",
        &hash[..8].iter().fold(0u64, |acc, &b| acc << 8 | b as u64)
    )
}

/// Result of running gRPC link detection.
///
/// Surfaces detection statistics and any failures for explicit
/// degradation reporting. Attached to `IndexResult` for visibility.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcLinkResult {
    /// Number of links emitted.
    pub links_emitted: usize,
    /// Number of provider surfaces found (for diagnostics).
    pub providers_queried: usize,
    /// Number of consumer surfaces found (for diagnostics).
    pub consumers_queried: usize,
    /// Query error when reading provider surfaces.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_query_error: Option<String>,
    /// Query error when reading consumer surfaces.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumer_query_error: Option<String>,
    /// Storage error when persisting links.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_storage_error: Option<String>,
}

impl GrpcLinkResult {
    pub fn has_error(&self) -> bool {
        self.provider_query_error.is_some()
            || self.consumer_query_error.is_some()
            || self.link_storage_error.is_some()
    }
}

/// Run gRPC link detection for a snapshot.
///
/// This is the top-level orchestration function for GR-3A. It:
/// 1. Queries provider surfaces with gRPC service contracts (GR-1A outputs)
/// 2. Queries consumer surfaces with gRPC service contracts (GR-2A outputs)
/// 3. Joins by contract_element_uid to find matching pairs
/// 4. Persists links to boundary_interaction_links
///
/// # Arguments
///
/// * `storage` - Storage connection implementing both read and write ports
/// * `snapshot_uid` - The snapshot to process
///
/// # Returns
///
/// A `GrpcLinkResult` summarizing counts and any errors encountered.
/// Errors are collected rather than fail-fast, allowing partial progress.
pub fn run_grpc_link_detection<S>(storage: &mut S, snapshot_uid: &str) -> GrpcLinkResult
where
    S: GrpcLinkReadPort + GrpcLinkStorePort,
    <S as GrpcLinkReadPort>::Error: ToString,
    <S as GrpcLinkStorePort>::Error: ToString,
{
    let mut result = GrpcLinkResult::default();

    // Step 1: Query provider surfaces with contracts
    let providers = match storage.query_provider_surfaces_with_contracts(snapshot_uid) {
        Ok(p) => {
            result.providers_queried = p.len();
            p
        }
        Err(e) => {
            result.provider_query_error = Some(e.to_string());
            return result;
        }
    };

    // Step 2: Query consumer surfaces with contracts
    let consumers = match storage.query_consumer_surfaces_with_contracts(snapshot_uid) {
        Ok(c) => {
            result.consumers_queried = c.len();
            c
        }
        Err(e) => {
            result.consumer_query_error = Some(e.to_string());
            return result;
        }
    };

    // Early exit if no surfaces
    if providers.is_empty() || consumers.is_empty() {
        return result;
    }

    // Step 3: Find links by contract match
    let links = find_grpc_links(&providers, &consumers);
    if links.is_empty() {
        return result;
    }

    // Step 4: Convert to link inputs
    let link_inputs: Vec<BoundaryInteractionLinkInput> = links
        .iter()
        .map(|link| {
            let link_uid = generate_link_uid(
                snapshot_uid,
                &link.provider_surface_uid,
                &link.consumer_surface_uid,
                &link.contract_element_uid,
            );

            let evidence = serde_json::json!({
                "contract_full_name": link.contract_full_name,
                "provider_file": link.provider_source_file,
                "consumer_file": link.consumer_source_file,
                "provider_basis": link.provider_basis,
                "consumer_basis": link.consumer_basis,
            });

            BoundaryInteractionLinkInput {
                link_uid,
                snapshot_uid: snapshot_uid.to_string(),
                provider_surface_uid: link.provider_surface_uid.clone(),
                consumer_surface_uid: link.consumer_surface_uid.clone(),
                link_kind: "contract_match_only".to_string(),
                contract_element_uid: link.contract_element_uid.clone(),
                match_basis: "contract".to_string(),
                confidence: 0.80,
                evidence_json: evidence.to_string(),
            }
        })
        .collect();

    // Step 5: Persist links
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

    fn make_provider(
        uid: &str,
        contract_uid: &str,
        contract_name: &str,
        file: &str,
    ) -> SurfaceWithContract {
        SurfaceWithContract {
            surface_uid: uid.to_string(),
            contract_element_uid: contract_uid.to_string(),
            contract_full_name: contract_name.to_string(),
            direction: "provider".to_string(),
            source_file: file.to_string(),
            basis: "impl_extension".to_string(),
        }
    }

    fn make_consumer(
        uid: &str,
        contract_uid: &str,
        contract_name: &str,
        file: &str,
    ) -> SurfaceWithContract {
        SurfaceWithContract {
            surface_uid: uid.to_string(),
            contract_element_uid: contract_uid.to_string(),
            contract_full_name: contract_name.to_string(),
            direction: "consumer".to_string(),
            source_file: file.to_string(),
            basis: "stub_creation".to_string(),
        }
    }

    #[test]
    fn find_links_matches_by_contract() {
        let providers = vec![make_provider(
            "p1",
            "ce-greeter",
            "helloworld.Greeter",
            "Server.java",
        )];

        let consumers = vec![make_consumer(
            "c1",
            "ce-greeter",
            "helloworld.Greeter",
            "Client.java",
        )];

        let links = find_grpc_links(&providers, &consumers);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].provider_surface_uid, "p1");
        assert_eq!(links[0].consumer_surface_uid, "c1");
        assert_eq!(links[0].contract_element_uid, "ce-greeter");
        assert_eq!(links[0].contract_full_name, "helloworld.Greeter");
    }

    #[test]
    fn find_links_no_match_different_contracts() {
        let providers = vec![make_provider(
            "p1",
            "ce-greeter",
            "helloworld.Greeter",
            "Server.java",
        )];

        let consumers = vec![make_consumer(
            "c1",
            "ce-user",
            "user.UserService",
            "Client.java",
        )];

        let links = find_grpc_links(&providers, &consumers);

        assert!(links.is_empty());
    }

    #[test]
    fn find_links_multiple_consumers_one_provider() {
        let providers = vec![make_provider(
            "p1",
            "ce-greeter",
            "helloworld.Greeter",
            "Server.java",
        )];

        let consumers = vec![
            make_consumer("c1", "ce-greeter", "helloworld.Greeter", "Client1.java"),
            make_consumer("c2", "ce-greeter", "helloworld.Greeter", "Client2.java"),
        ];

        let links = find_grpc_links(&providers, &consumers);

        assert_eq!(links.len(), 2);
        assert_eq!(links[0].consumer_surface_uid, "c1");
        assert_eq!(links[1].consumer_surface_uid, "c2");
    }

    #[test]
    fn find_links_multiple_providers_one_consumer() {
        // Scenario: multiple servers implementing the same service (unusual but possible)
        let providers = vec![
            make_provider("p1", "ce-greeter", "helloworld.Greeter", "Server1.java"),
            make_provider("p2", "ce-greeter", "helloworld.Greeter", "Server2.java"),
        ];

        let consumers = vec![make_consumer(
            "c1",
            "ce-greeter",
            "helloworld.Greeter",
            "Client.java",
        )];

        let links = find_grpc_links(&providers, &consumers);

        assert_eq!(links.len(), 2);
        assert_eq!(links[0].provider_surface_uid, "p1");
        assert_eq!(links[1].provider_surface_uid, "p2");
    }

    #[test]
    fn link_uid_is_deterministic() {
        let uid1 = generate_link_uid("snap-1", "p1", "c1", "ce1");
        let uid2 = generate_link_uid("snap-1", "p1", "c1", "ce1");
        assert_eq!(uid1, uid2);

        // Different snapshot
        let uid3 = generate_link_uid("snap-2", "p1", "c1", "ce1");
        assert_ne!(uid1, uid3);

        // Different provider
        let uid4 = generate_link_uid("snap-1", "p2", "c1", "ce1");
        assert_ne!(uid1, uid4);

        // Different consumer
        let uid5 = generate_link_uid("snap-1", "p1", "c2", "ce1");
        assert_ne!(uid1, uid5);

        // Different contract — critical: same provider/consumer pair with
        // different services must produce different link UIDs
        let uid6 = generate_link_uid("snap-1", "p1", "c1", "ce2");
        assert_ne!(uid1, uid6, "Different contracts must produce different UIDs");
    }

    #[test]
    fn link_uid_starts_with_prefix() {
        let uid = generate_link_uid("snap-1", "p1", "c1", "ce1");
        assert!(uid.starts_with("grpc-link-"));
    }

    #[test]
    fn find_links_multi_service_pair_produces_distinct_links() {
        // Scenario: same provider and consumer surfaces implement/use TWO different
        // gRPC services. This can happen with multi-service server classes or
        // client factories. Each service contract must produce a separate link.
        let providers = vec![
            make_provider("p1", "ce-greeter", "helloworld.Greeter", "Server.java"),
            make_provider("p1", "ce-health", "grpc.health.v1.Health", "Server.java"),
        ];

        let consumers = vec![
            make_consumer("c1", "ce-greeter", "helloworld.Greeter", "Client.java"),
            make_consumer("c1", "ce-health", "grpc.health.v1.Health", "Client.java"),
        ];

        let links = find_grpc_links(&providers, &consumers);

        // Should produce 2 links: (p1, c1, Greeter) and (p1, c1, Health)
        assert_eq!(links.len(), 2, "Multi-service pair should produce 2 distinct links");

        // Verify different contracts
        let contracts: std::collections::HashSet<_> = links
            .iter()
            .map(|l| l.contract_element_uid.as_str())
            .collect();
        assert!(contracts.contains("ce-greeter"));
        assert!(contracts.contains("ce-health"));

        // Verify link UIDs are distinct
        let link_uids: std::collections::HashSet<_> = links
            .iter()
            .map(|l| {
                generate_link_uid(
                    "snap-1",
                    &l.provider_surface_uid,
                    &l.consumer_surface_uid,
                    &l.contract_element_uid,
                )
            })
            .collect();
        assert_eq!(link_uids.len(), 2, "Link UIDs must be distinct per contract");
    }
}
