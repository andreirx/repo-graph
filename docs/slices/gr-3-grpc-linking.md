# GR-3: gRPC Provider/Consumer Linking

Status: PLANNED
Depends: GR-1 (Server Detection), GR-2 (Client Detection)
Track: B (Schema-Backed RPC)

## Objective

Link gRPC clients to servers: match consumer stubs to provider implementations
based on service contracts and channel identity. This produces cross-component
and cross-language boundary links.

## Why This Matters

Provider/consumer linking is the highest-value outcome of boundary detection:
- "This Python client calls this Java server"
- "This service method is consumed by these 5 clients"
- "This service has no consumers (dead boundary?)"
- "This client targets a service we don't implement (external dependency)"

## Scope

### In scope
- Intra-repo linking (client and server in same repo)
- Service-level matching (contract-based)
- Method-level matching (RPC-based)
- Cross-language linking
- Unmatched consumer reporting (external dependencies)
- Unmatched provider reporting (unused services)

### Out of scope
- Cross-repo linking (requires multi-repo indexing)
- Runtime service discovery (Consul, etcd, etc.)
- Load balancer resolution
- Network topology inference

## Linking Strategy

### Level 1: Contract-Based Matching

Match by protobuf service identity:

```
Server implements: package.MyService
Client uses stub: package.MyService

-> Match if same service full_name
```

This works regardless of:
- Language difference
- File location
- Endpoint configuration

### Level 2: Endpoint-Based Matching

When both have literal endpoints:

```
Server binds: 0.0.0.0:50051
Client targets: localhost:50051

-> Match if ports align and server binds INADDR_ANY
```

Endpoint matching adds confidence but is not required.

### Level 3: Method-Level Linking

Within a service match, link specific method calls:

```
Server implements: MyService.GetUser, MyService.CreateUser
Client calls: stub.GetUser()

-> Link client call to server handler
```

## Link Classification

| Scenario | Classification |
|----------|---------------|
| Server + Client, same repo | `internal_link` |
| Server only, no client | `exposed_boundary` |
| Client only, no server | `external_dependency` |
| Client + Server, endpoint mismatch | `contract_match_only` |

## Matching Algorithm

```
1. Group all gRPC servers by service full_name
2. Group all gRPC clients by service full_name

3. For each service full_name:
   - If servers exist and clients exist:
     - Create link records between each client/server pair
     - Link at method level where calls match handlers
   - If servers exist but no clients:
     - Mark servers as exposed_boundary
   - If clients exist but no servers:
     - Mark clients as external_dependency

4. For clients with literal endpoints:
   - Check if any server binds that endpoint
   - Add endpoint_match_confidence if aligned
```

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
