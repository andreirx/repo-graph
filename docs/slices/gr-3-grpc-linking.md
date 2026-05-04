# GR-3: gRPC Provider/Consumer Linking

Status: GR-3A IMPLEMENTED (CLI pending), GR-3B/GR-3C DEFERRED
Depends: GR-1A (Server Hints), GR-2A (Client Hints), CS-2A (Generated Code Mapping)
Track: B (Schema-Backed RPC)

**Track status (2026-05-04):** The gRPC track has reached orientation sufficiency.
Implemented: CS-1, CS-2A, GR-1A, GR-1B, GR-2A, GR-3A.
Deferred depth slices: GR-1C, GR-2B, GR-3B, GR-3C.
Breadth-first product strategy: move to next mechanism family (BI-1B TCP/UDP sockets).
Return to deeper gRPC slices only if real-repo navigation proves the existing hints insufficient.

## Objective

Link gRPC provider surfaces (GR-1A) to consumer surfaces (GR-2A) when both
reference the same proto service contract. This surfaces the structural
relationship: "these two code locations appear to communicate via the same
gRPC service."

**This is a hint, not connection proof.**

It surfaces:
- "this provider and consumer appear to belong to the same proto service"
- candidate link for agent inspection

NOT:
- definite network path
- deployed communication proof
- live runtime topology

## Phased Implementation

### GR-3A (this slice): Contract-Based Linking
- Match by shared proto service contract only
- Hint-grade confidence (0.80)
- No endpoint matching required

### GR-3B (future): Endpoint-Aligned Linking
- Requires GR-2B (client endpoint) and GR-1C (server endpoint)
- Higher confidence (0.90+) when endpoints align
- Port matching, host resolution

### GR-3C (future): Method-Level Linking
- Link specific RPC calls to handlers
- Requires method-level extraction

## GR-3A Scope

### In scope
- Link provider surface to consumer surface when:
  - Both have `boundary_contracts` associations
  - Both point to the same `contract_element_uid` (proto service)
  - Both are `schema_rpc` transport class
- Emit deterministic linking artifact to `boundary_interaction_links`
- Evidence: shared contract element
- Match basis: `contract`
- Link kind: `contract_match_only`
- Hint-grade confidence (0.80)

### Out of scope (GR-3A)
- Endpoint reconciliation (GR-3B)
- Host/port matching (GR-3B)
- Method-level linking (GR-3C)
- Cross-repo linking
- Same-process vs remote proof
- Runtime service discovery

## GR-3A Detection Logic

### Query providers with contracts
```sql
SELECT 
    bis.surface_uid,
    bc.contract_element_uid,
    ce.full_name AS contract_name,
    bis.basis AS provider_basis
FROM boundary_interaction_surfaces bis
JOIN boundary_contracts bc ON bc.surface_uid = bis.surface_uid
JOIN contract_elements ce ON ce.element_uid = bc.contract_element_uid
WHERE bis.snapshot_uid = ?
  AND bis.direction = 'provider'
  AND bis.transport_class = 'schema_rpc'
  AND bc.contract_kind = 'grpc_service'
```

### Query consumers with contracts
```sql
SELECT 
    bis.surface_uid,
    bc.contract_element_uid,
    ce.full_name AS contract_name,
    bis.basis AS consumer_basis
FROM boundary_interaction_surfaces bis
JOIN boundary_contracts bc ON bc.surface_uid = bis.surface_uid
JOIN contract_elements ce ON ce.element_uid = bc.contract_element_uid
WHERE bis.snapshot_uid = ?
  AND bis.direction = 'consumer'
  AND bis.transport_class = 'schema_rpc'
  AND bc.contract_kind = 'grpc_service'
```

### Join by contract
For each `(provider, consumer)` pair sharing the same `contract_element_uid`:
- Generate deterministic `link_uid`
- Insert into `boundary_interaction_links` with:
  - `link_kind = 'contract_match_only'`
  - `match_basis = 'contract'`
  - `confidence = 0.80`

## Link Classification (GR-3A)

| Scenario | link_kind |
|----------|-----------|
| Provider + Consumer, same contract | `contract_match_only` |

Future slices (GR-3B) will add:
- `internal_link` (when endpoints also match)
- Exposed boundary reporting
- External dependency reporting

## Storage Schema

**Note:** The table is named `boundary_interaction_links` to avoid collision with
the existing `boundary_links` table (migration 008), which references the
`boundary_provider_facts` / `boundary_consumer_facts` model used for HTTP/CLI
boundaries. This table operates on `boundary_interaction_surfaces` from migration 024.

```sql
CREATE TABLE boundary_interaction_links (
    link_uid TEXT PRIMARY KEY,
    snapshot_uid TEXT NOT NULL,
    provider_surface_uid TEXT NOT NULL,
    consumer_surface_uid TEXT NOT NULL,
    link_kind TEXT NOT NULL,  -- 'internal_link', 'contract_match_only'
    contract_element_uid TEXT,  -- service or method
    match_basis TEXT NOT NULL,  -- 'contract', 'contract_and_endpoint'
    confidence REAL NOT NULL,
    evidence_json TEXT,
    FOREIGN KEY (snapshot_uid) REFERENCES snapshots(snapshot_uid),
    FOREIGN KEY (provider_surface_uid) REFERENCES boundary_interaction_surfaces(surface_uid),
    FOREIGN KEY (consumer_surface_uid) REFERENCES boundary_interaction_surfaces(surface_uid),
    FOREIGN KEY (contract_element_uid) REFERENCES contract_elements(element_uid)
);

CREATE INDEX idx_bil_provider ON boundary_interaction_links(provider_surface_uid);
CREATE INDEX idx_bil_consumer ON boundary_interaction_links(consumer_surface_uid);
CREATE INDEX idx_bil_contract ON boundary_interaction_links(contract_element_uid);
```

## CLI Surface

```
rmap boundaries links <db> <repo> [--service <name>] [--unmatched]
  List boundary links with optional filtering

rmap boundaries consumers <db> <repo> <server_surface>
  List consumers of a specific server

rmap boundaries providers <db> <repo> <client_surface>
  List providers that a client might target

rmap boundaries unmatched <db> <repo>
  List exposed boundaries and external dependencies
```

## Evidence Structure

```json
{
  "match_basis": "contract_and_endpoint",
  "service_full_name": "package.MyService",
  "methods_linked": [
    {
      "method_name": "GetUser",
      "provider_handler": "src/server.py:42",
      "consumer_calls": ["src/client.ts:15", "src/client.ts:28"]
    }
  ],
  "endpoint_match": {
    "provider_endpoint": "0.0.0.0:50051",
    "consumer_endpoint": "localhost:50051",
    "match_confidence": 0.90
  }
}
```

## Confidence Scoring

| Match Basis | Confidence |
|-------------|------------|
| Contract + endpoint + method calls | 0.95 |
| Contract + endpoint | 0.90 |
| Contract + method calls | 0.85 |
| Contract only | 0.75 |

## Implementation Steps

1. **Collector pass**
   - Gather all gRPC server surfaces
   - Gather all gRPC client surfaces
   - Index by service full_name

2. **Contract matching**
   - For each service, find matching providers/consumers
   - Create link candidates

3. **Endpoint refinement**
   - For links with endpoints, check alignment
   - Boost confidence for endpoint matches

4. **Method-level linking**
   - Within service matches, link RPC calls to handlers
   - Record method-level evidence

5. **Unmatched classification**
   - Tag exposed boundaries (no consumers)
   - Tag external dependencies (no providers)

6. **Storage persistence**
   - Write to boundary_interaction_links
   - Update surfaces with link counts

## Test Matrix

1. Same-language server/client match
2. Cross-language server/client match (Java server, Python client)
3. Multiple clients for one server
4. Multiple servers implementing same service
5. Exposed boundary detection (server, no client)
6. External dependency detection (client, no server)
7. Endpoint-confirmed match
8. Contract-only match (different endpoints)
9. Method-level linking
10. Unmatched reporting CLI

## Validation Repos

- Multi-service gRPC repo
- Cross-language gRPC example
- Microservices fixture

## Deliverables

- Contract-based matching logic
- Endpoint refinement logic
- Method-level linking
- Unmatched classification
- boundary_interaction_links storage
- CLI commands
- 20+ integration tests

## Success Criteria

- Cross-language links working
- Method-level linking accurate
- Exposed boundaries detected
- External dependencies detected
- Confidence scores meaningful
- CLI queries useful

## Implementation Notes (2026-05-04)

### GR-3A Implementation

Files added/modified:
- `rust/crates/indexer/src/grpc_link.rs` — NEW: Detection logic and orchestration
- `rust/crates/indexer/src/storage_port.rs` — Added `GrpcLinkReadPort`, `GrpcLinkStorePort` traits, DTOs
- `rust/crates/indexer/src/lib.rs` — Module and re-exports
- `rust/crates/indexer/src/types.rs` — Added `grpc_links` field to `IndexResult`
- `rust/crates/indexer/src/orchestrator.rs` — Wired GR-3A after GR-1A and GR-2A
- `rust/crates/storage/src/grpc_impl_hint_port_impl.rs` — Port implementations for StorageConnection

Key implementation details:
- Queries provider surfaces where `direction='provider'`, `transport_class='schema_rpc'`, `contract_kind='grpc_service'`
- Queries consumer surfaces where `direction='consumer'`, `transport_class='schema_rpc'`, `contract_kind='grpc_service'`
- Joins by `contract_element_uid` to find matching (provider, consumer) pairs
- Links emitted with `link_kind='contract_match_only'`, `match_basis='contract'`, `confidence=0.80`
- Link UID is deterministic: hash of `snapshot_uid:provider_surface_uid:consumer_surface_uid:contract_element_uid`
  - **Critical:** includes `contract_element_uid` so multi-service pairs produce distinct links
- Evidence JSON includes: contract_full_name, provider_file, consumer_file, provider_basis, consumer_basis

**Orchestration wiring:**
- Runs after GR-1A (provider hints) and GR-2A (consumer hints) complete
- Only runs if both GR-1A and GR-2A emitted at least one hint
- Uses same storage connection (no new DB transaction)

**Tests added:**
- `grpc_link::tests::find_links_matches_by_contract` — basic contract matching
- `grpc_link::tests::find_links_no_match_different_contracts` — different contracts, no links
- `grpc_link::tests::find_links_multiple_consumers_one_provider` — N:1 links
- `grpc_link::tests::find_links_multiple_providers_one_consumer` — 1:N links
- `grpc_link::tests::find_links_multi_service_pair_produces_distinct_links` — multi-service pairs
- `grpc_link::tests::link_uid_is_deterministic` — UID stability (includes contract in identity)
- `grpc_link::tests::link_uid_starts_with_prefix` — UID format
- `gr3a_query_provider_surfaces_with_contracts` — storage port test
- `gr3a_query_consumer_surfaces_with_contracts` — storage port test
- `gr3a_end_to_end_link_detection` — full chain test
- `gr3a_no_links_without_matching_contracts` — negative test
- `gr3a_link_is_idempotent` — INSERT OR IGNORE behavior

**Pending:**
- CLI command `rmap boundaries links` (GR-3A has no CLI exposure yet)
- Fixture validation test with real indexed grpc-java-minimal run
- Cross-language fixture test (Java server + Python/TS client)

### What GR-3A Does NOT Do

- Does NOT extract endpoint information (GR-2B, GR-1C scope)
- Does NOT match host:port (GR-3B scope)
- Does NOT link individual RPC method calls (GR-3C scope)
- Does NOT detect exposed/unmatched boundaries (GR-3B scope)
- Does NOT work cross-repo
