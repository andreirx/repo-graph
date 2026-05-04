# GR-2A: Java gRPC Client Stub Hints

Status: FIXTURE-VALIDATED
Depends: CS-1 (Protobuf Schema), CS-2A (Java Generated Code Mapping)
Track: B (Schema-Backed RPC)

## Objective

Detect Java gRPC client stub creation patterns. This is the consumer-side
symmetric to GR-1A (server implementation hints).

**This is a hint slice.** It surfaces:
- "this code constructs a gRPC client stub for this proto service"
- "inspect this file/class/method next"

NOT:
- "this client definitely calls the service at runtime"
- endpoint/host/port truth claims

## Scope

### In scope
- Detect `*Grpc.newBlockingStub(channel)`
- Detect `*Grpc.newFutureStub(channel)`
- Detect `*Grpc.newStub(channel)`
- Emit consumer hints (direction = consumer)
- Link to proto service via CS-2A mappings
- Hint-grade confidence (0.85)

### Out of scope
- Channel endpoint parsing (GR-2B)
- Host/port extraction (GR-2B)
- RPC method invocation tracking (future)
- Interceptor/decorator flow (future)
- Other languages (GR-2C+)

## Detection Patterns

### Java gRPC (io.grpc)

```java
// Generated stub class: MyServiceGrpc
// Contains: newBlockingStub, newFutureStub, newStub

// Blocking stub (synchronous)
MyServiceGrpc.MyServiceBlockingStub blockingStub = 
    MyServiceGrpc.newBlockingStub(channel);

// Future stub (ListenableFuture)
MyServiceGrpc.MyServiceFutureStub futureStub = 
    MyServiceGrpc.newFutureStub(channel);

// Async stub (StreamObserver)
MyServiceGrpc.MyServiceStub asyncStub = 
    MyServiceGrpc.newStub(channel);
```

**Detection signals:**
- CALLS edge where `target_key` contains `Grpc.newBlockingStub`
- CALLS edge where `target_key` contains `Grpc.newFutureStub`
- CALLS edge where `target_key` contains `Grpc.newStub`

**Extract from pattern:**
- Service name from the Grpc class prefix (e.g., `MyServiceGrpc` → `MyService`)

## Boundary Model Mapping

Each detected stub creation produces a `BoundaryInteractionSurface`:

| Field | Value |
|-------|-------|
| channel_kind | `grpc_channel` |
| boundary_scope | `unknown` (no endpoint info yet) |
| transport_class | `schema_rpc` |
| direction | `consumer` |
| interaction_pattern | `unknown` |
| protocol | `grpc` |
| protocol_family | `rpc` |
| provenance | `inferred` |
| confidence_basis | `stub_creation` |
| extractor | `grpc_client_hint_java` |
| confidence | 0.85 |

## Contract Association

Link to proto service via CS-2A:
1. Extract service name from `*Grpc.newBlockingStub` (e.g., `GreeterGrpc` → `Greeter`)
2. Find CS-2A mapping for `*Grpc.java` file
3. Create `boundary_contracts` association to proto service element

Same mechanism as GR-1A, different direction.

## Evidence Schema

```json
{
  "grpc_class": "GreeterGrpc",
  "stub_method": "newBlockingStub",
  "stub_type": "blocking"
}
```

## Implementation Approach

### Query for stub creation calls

```sql
SELECT
    n.stable_key,
    n.name,
    f.path,
    ee.line_start,
    ee.target_key
FROM extraction_edges ee
JOIN nodes n ON ee.source_node_uid = n.node_uid
JOIN files f ON n.file_uid = f.file_uid
WHERE ee.snapshot_uid = ?
  AND ee.type = 'CALLS'
  AND f.language = 'java'
  AND (
    ee.target_key LIKE '%Grpc.newBlockingStub%'
    OR ee.target_key LIKE '%Grpc.newFutureStub%'
    OR ee.target_key LIKE '%Grpc.newStub%'
  )
```

### Extract service name

From `GreeterGrpc.newBlockingStub` → extract `Greeter`

Regex: `(\w+)Grpc\.new(Blocking|Future)?Stub`

### Link to proto service

Use CS-2A mapping: find `generated_code_mappings` where `generated_symbol_key`
contains the Grpc class name → get `schema_element_uid` for the service.

## Substrate Assumption

Same as GR-1A: requires generated `*Grpc.java` stubs to be present in the
indexed tree. Without generated stubs, 0 client hints will be produced.

## Implementation Steps

1. Add `query_grpc_stub_creations` to storage
2. Add `GrpcClientHintReadPort` trait to indexer
3. Create `grpc_client_hint.rs` with `run_grpc_client_hint_detection`
4. Wire into orchestrator (after CS-2A, parallel to GR-1A)
5. Add fixture with client stub creation
6. Add CLI adapter tests
7. Validate on fixture

## Validation Criteria

- [x] Fixture: client stub creation detected
- [x] Fixture: direction = consumer
- [x] Fixture: contract association to proto service
- [x] CLI: `rmap boundaries list` shows consumer surfaces
- [x] No overlap with GR-1A surfaces (different symbol, different direction)

## Test Fixture

Extended `grpc-java-minimal` with client code:

- `test/fixtures/grpc-java-minimal/src/main/java/io/grpc/examples/helloworld/HelloWorldClient.java`
- Updated `GreeterGrpc.java` with stub inner classes (`GreeterBlockingStub`, `GreeterFutureStub`, `GreeterStub`)

Fixture validation test: `gr2a_fixture_validated_full_indexed_run` in `boundaries_command.rs`

## Implementation Notes (2026-05-04)

Files added/modified:
- `rust/crates/boundary-interaction/src/types.rs` — Added `StubCreation` variant to `InteractionBasis`
- `rust/crates/storage/src/boundary_interaction_read_impl.rs` — Added parser for `stub_creation` basis
- `rust/crates/storage/src/grpc_impl_hint_impl.rs` — Added `query_grpc_stub_creations_raw()` storage query
- `rust/crates/indexer/src/storage_port.rs` — Added `GrpcClientHintReadPort`, `GrpcClientHintStorePort` traits
- `rust/crates/indexer/src/grpc_client_hint.rs` — NEW: Detection logic and orchestration
- `rust/crates/indexer/src/lib.rs` — Module and re-exports
- `rust/crates/indexer/src/types.rs` — Added `grpc_client_hints` field to `IndexResult`
- `rust/crates/storage/src/grpc_impl_hint_port_impl.rs` — Port implementations for StorageConnection
- `rust/crates/indexer/src/orchestrator.rs` — Wired GR-2A after CS-2A (parallel to GR-1A)

Key implementation details:
- Detects CALLS edges where `target_key` matches `*Grpc.newBlockingStub`, `*Grpc.newFutureStub`, `*Grpc.newStub`
- Regex pattern: `(\w+Grpc)\.new(Blocking|Future)?Stub`
- Surfaces emitted with `direction = consumer` (unlike GR-1A's `provider`)
- Surfaces emitted with `basis = stub_creation` and `extractor = grpc_client_hint_java`
- Confidence 0.85 (hint-grade)

**CS-2A join strategy (FIXED 2026-05-04):**
- CS-2A maps inner classes (`GreeterGrpc.GreeterImplBase`, `GreeterGrpc.GreeterBlockingStub`, etc.), NOT the outer `*Grpc` class
- GR-2A extracts service name from stub call: `GreeterGrpc.newBlockingStub` → `Greeter`
- Joins through `contract_elements` by service name, not by matching `generated_symbol_key`
- Query: `query_grpc_service_mappings` returns proto services with at least one CS-2A gRPC mapping
- DTO: `GrpcServiceMappingInput { service_element_uid, service_name, mapping_uid, confidence }`

**Disambiguation strategy:**
- When multiple proto services share the same simple name (e.g., `api.v1.Greeter` and `legacy.Greeter`),
  refuse to link rather than risk binding to the wrong service. Same "refuse-on-ambiguity" pattern as GR-1B.

**Surface UID identity:**
- Includes: `snapshot_uid`, `creator_stable_key`, `grpc_class`, `stub_method`, `line_start`
- Ensures each distinct stub creation call site produces a unique surface,
  even when the same method creates multiple stubs of the same type for
  different services.

**Substrate assumption:** Same as GR-1A — requires generated `*Grpc.java` stubs to be present.

**CLI tests added:**
- `boundaries_list_shows_gr2a_consumer_direction` — verifies direction=consumer, basis=stub_creation
- `boundaries_show_includes_stub_info_for_gr2a` — verifies evidence_json contains grpc_class, stub_method, stub_type
- `boundaries_show_includes_gr2a_contract_association` — verifies contract link to proto service

**Fixture validation test (2026-05-04):**
- `gr2a_fixture_validated_full_indexed_run` in `boundaries_command.rs`
- Indexes real `grpc-java-minimal` fixture with `HelloWorldClient.java`
- Validates full chain: extractor → CS-2A → GR-2A → CLI
- Confirms consumer hint links to `helloworld.Greeter` proto service
