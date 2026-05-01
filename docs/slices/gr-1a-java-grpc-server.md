# GR-1A: Java gRPC Server Implementation Hints

Status: IMPLEMENTED (2026-05-01)
Depends: CS-1 (Protobuf Schema Extraction), CS-2A (Java Generated Code Mapping)
Track: B (Schema-Backed RPC)

## Objective

Surface hints for Java gRPC server implementations. This is a **discovery slice**,
not a runtime-proof slice. The goal is to tell an agent:

- "this class strongly suggests gRPC server-side implementation"
- "it corresponds to this proto service"
- "this file/class is worth investigation"

This is NOT answering "what servers are actually running?" — that requires
registration proof (GR-1B) and endpoint evidence (GR-1C).

## Scope

### In scope
- Detect Java classes extending `*Grpc.*ImplBase`
- Link to proto service via CS-2A generated-code mappings
- Anchor hint on implementation class (not registration site)
- Surface as provider candidate with `boundary_scope = unknown`
- Explicit hint-grade provenance (not runtime certainty)

### Out of scope (deferred to later slices)
- Registration detection: `addService()` / `bindService()` (GR-1B)
- Endpoint/topology evidence (GR-1C)
- Python, TypeScript, Rust, C++ (GR-1D+)
- Client detection (GR-2)
- Provider/consumer linking (GR-3)

## Detection Pattern

### Inheritance-based detection (GR-1A)

Service implementations extend generated base classes:
```java
public class GreeterImpl extends GreeterGrpc.GreeterImplBase {
    @Override
    public void sayHello(HelloRequest req, StreamObserver<HelloReply> resp) {
        // ...
    }
}
```

Detection: find classes where IMPLEMENTS edge target matches `*Grpc.*ImplBase`.

This is sufficient for a hint because:
- Type relationship is deterministic (extends is syntax-proven)
- CS-2A already mapped `*ImplBase` classes to proto services
- The implementation class is the stable, discoverable artifact
- Agent can inspect this class to understand server behavior

### Evidence Chain

```
Java class extends *Grpc.*ImplBase
    ↓ (IMPLEMENTS edge, relation=extends)
*ImplBase class in generated_code_mappings
    ↓ (schema_element_uid)
Proto service in contract_elements
```

### What this does NOT prove

- Registration: class may exist but never be wired into a server
- Endpoint: no bind address, port, or exposure evidence
- Liveness: implementation may be dead code

These are refinement slices (GR-1B, GR-1C), not prerequisites.

## Storage Model

### boundary_interaction_surfaces

```sql
INSERT INTO boundary_interaction_surfaces (
    surface_uid,          -- deterministic hash
    snapshot_uid,
    repo_uid,
    boundary_scope,       -- 'unknown' (no endpoint evidence)
    channel_kind,         -- 'grpc'
    direction,            -- 'provider'
    transport_class,      -- 'schema_rpc'
    provenance,           -- 'inferred' (hint, not proven registration)
    confidence_basis,     -- 'heuristic' (inheritance pattern)
    protocol,             -- 'grpc'
    protocol_family,      -- 'rpc'
    interaction_pattern,  -- 'unknown' (not proven)
    endpoint_locality,    -- 'unknown'
    symbol_stable_key,    -- implementation class symbol (anchor)
    source_file,
    line_start, line_end, col_start, col_end,
    extractor,            -- 'grpc_impl_hint_java'
    basis,                -- 'extends_impl_base'
    confidence,           -- 0.85 (inheritance hint, not registration proof)
    evidence_json         -- includes impl class, base class, proto service
);
```

Key provenance markers:
- `provenance = 'inferred'` — not direct extraction, pattern-based
- `confidence_basis = 'heuristic'` — inheritance pattern, not runtime proof
- `confidence = 0.85` — high-confidence hint, not certainty
- `interaction_pattern = 'unknown'` — cannot determine from inheritance alone

### boundary_contracts

```sql
INSERT INTO boundary_contracts (
    association_uid,        -- deterministic hash
    surface_uid,            -- FK to boundary_interaction_surfaces
    contract_element_uid,   -- FK to contract_elements (proto service)
    contract_kind,          -- 'grpc_service'
    association_basis,      -- 'generated_code_mapping' (via CS-2A)
    confidence,             -- inherits from CS-2A mapping confidence
    evidence_json           -- includes mapping_uid, element details
);
```

## Confidence Model

GR-1A produces a single confidence tier:

| Basis | Confidence | Description |
|-------|------------|-------------|
| `extends_impl_base` | 0.85 | Class extends *Grpc.*ImplBase (hint) |

This is deliberately lower than CS-2A mappings (0.95) because:
- Inheritance proves type relationship, not registration
- Implementation may be dead code
- No runtime/wiring evidence

Higher confidence requires additional evidence (future slices):
- Registration proof (GR-1B): +0.05 → 0.90
- Endpoint evidence (GR-1C): +0.05 → 0.95

## Implementation Steps

### Phase 1: Query Infrastructure

1. **Add storage query for IMPLEMENTS edges**
   - Query edges where `edge_type = 'IMPLEMENTS'`
   - Filter where `metadata_json` contains `"relation":"extends"`
   - Filter where `target_key` matches `*ImplBase` pattern
   - Return: source_node_uid, target_key, location

2. **Add storage query for ImplBase mappings**
   - Query `generated_code_mappings` where symbol matches `*ImplBase`
   - Return: schema_element_uid, evidence

### Phase 2: Detection Module

1. **Create `grpc-impl-hint` support module**
   - Location: `rust/crates/grpc-impl-hint/`
   - Pure detection logic, no storage dependency
   - Input: IMPLEMENTS edges + generated_code_mappings
   - Output: `GrpcImplHint` facts

2. **Data structure**
   ```rust
   pub struct GrpcImplHint {
       pub impl_class_key: String,      // stable_key of implementation class
       pub impl_base_class: String,     // e.g., "GreeterGrpc.GreeterImplBase"
       pub proto_service_uid: String,   // contract_element UID
       pub proto_service_name: String,  // e.g., "Greeter"
       pub source_file: String,
       pub line_start: i64,
       pub confidence: f64,             // 0.85
   }
   ```

### Phase 3: Storage Integration

1. **Emit boundary_interaction_surfaces**
   - Use existing `insert_boundary_surfaces()`
   - Anchor on implementation class

2. **Emit boundary_contracts**
   - Use existing `insert_boundary_contracts()` (if exists) or add
   - Link surface to proto service element

3. **Integrate with indexer orchestrator**
   - Run after CS-2A mapping phase
   - Add to `IndexResult` for visibility

### Phase 4: CLI Surface

- Existing `rmap boundaries` command should surface these
- Filter by `channel_kind = grpc` and `transport_class = schema_rpc`

### Phase 5: Tests

1. **Unit tests for pattern matching**
2. **Integration test with fixture**
   - Java class extending generated ImplBase
   - Proto service definition
   - Verify hint emitted with correct proto service link
3. **Smoke test if suitable repo found**

## Test Matrix

1. Direct ImplBase extension → hint emitted
2. Class extends intermediate that extends ImplBase → hint emitted (if detectable)
3. ImplBase class mapped via CS-2A → contract association correct
4. ImplBase class NOT mapped → no hint (no orphan hints)
5. Multiple implementations in same file → multiple hints
6. Class extends non-gRPC base → no hint

## Validation

Requires repo with:
- Proto files with service definitions
- Generated *Grpc.java files checked in
- Java class extending *ImplBase

Hadoop has limited gRPC. Better candidates:
- grpc-java examples repo
- Spring gRPC sample projects
- Or create minimal fixture

## Deliverables

- `grpc-impl-hint` support module (pure logic)
- Storage queries for IMPLEMENTS edges + ImplBase mappings
- Boundary surface + contract emission
- Orchestrator integration
- Unit tests for pattern matching
- Integration test with fixture
- Slice documentation

## Success Criteria

- Classes extending `*Grpc.*ImplBase` surfaced as hints
- Contract association links to correct proto service element
- `boundary_scope = 'unknown'` (no false scope inference)
- `transport_class = 'schema_rpc'`
- `direction = 'provider'`
- `provenance = 'inferred'` (hint-grade, not registration proof)
- `confidence = 0.85` (not overclaiming)
- 0 hints emitted for classes with no CS-2A mapping

## Modeling Constraints

1. **Hint, not proof**
   - This is discovery-grade, not runtime-proof
   - Do not claim registration, endpoint, or liveness

2. **No orphan hints**
   - Only emit hint if CS-2A mapping exists for the ImplBase class
   - No "unlinked implementation" hints in this slice

3. **Anchor on implementation class**
   - The implementation class is the primary artifact
   - Registration sites are refinement (GR-1B)

## Implementation Notes (2026-05-01)

### Complete: Support Module

**Indexer crate (`repo-graph-indexer`)**:
- `grpc_impl_hint.rs`: Detection module with `find_grpc_impl_hints()`, UID generators
- `run_grpc_impl_hint_detection()`: Top-level orchestration function
- Port traits: `GrpcImplHintReadPort`, `GrpcImplHintStorePort`
- Input DTOs: `GrpcImplSurfaceInput`, `GrpcImplContractInput`
- Public exports in `lib.rs`

**Storage crate (`repo-graph-storage`)**:
- `grpc_impl_hint_impl.rs`: Raw queries (`query_impl_base_extensions_raw`, `query_impl_base_mappings_raw`)
- `grpc_impl_hint_port_impl.rs`: Port implementations with type conversion

**Evidence chain preserved in `evidence_json`**:
- `impl_base_target`: the extended ImplBase class name
- `proto_service_name`: the proto service name
- `mapping_uid`: CS-2A mapping UID
- `mapping_confidence`: confidence from CS-2A

### Test Coverage (20 tests)

Storage layer (4 tests):
- `query_impl_base_extensions_finds_extends_impl_base`
- `query_impl_base_extensions_excludes_non_impl_base`
- `query_impl_base_mappings_finds_impl_base_mappings`
- `insert_boundary_contracts_persists_rows`

Port implementation (8 tests):
- `query_impl_base_extensions_via_port`
- `query_impl_base_mappings_via_port`
- `insert_grpc_impl_surfaces_via_port`
- `insert_grpc_impl_contracts_via_port`
- `empty_inserts_return_zero`
- `end_to_end_detection_via_run_grpc_impl_hint_detection`
- `detection_is_idempotent`
- `detection_with_no_mapping_match_emits_nothing`

Indexer (8 tests):
- `extract_class_name_from_symbol_key_works`
- `extract_class_name_returns_none_for_invalid_key`
- `extract_service_name_from_impl_base_works`
- `extract_service_name_returns_none_for_non_impl_base`
- `find_grpc_impl_hints_joins_extensions_with_mappings`
- `find_grpc_impl_hints_skips_unmatched_extensions`
- `surface_uid_is_deterministic`
- `association_uid_is_deterministic`

### Complete: Integration (2026-05-01)

1. **Orchestrator wiring**: `run_grpc_impl_hint_detection()` called after CS-2A
   - Added to both `index_repo()` and `refresh_repo()` in orchestrator.rs
   - Gated on: `generated_code_mappings.mappings_persisted > 0` and no CS-2A error
   - Explicit degradation via `GrpcImplHintResult` in `IndexResult`

2. **Explicit degradation**: Option A implemented
   - `grpc_impl_hints: Option<GrpcImplHintResult>` added to `IndexResult`
   - Reports: `hints_emitted`, `contracts_emitted`, query errors, storage errors
   - Library-level: serializable in `IndexResult` for programmatic consumers
   - CLI-level: NOT yet surfaced in `rmap index`/`refresh` stderr summary
     (current summary only shows CS-1/CS-2A, not GR-1A)

### Partial: CLI visibility

Hints are stored in `boundary_interaction_surfaces` and `boundary_contracts` tables.
Existing `rmap boundaries` command exposes **partial** information:

**Exposed via `rmap boundaries list`**:
- `channel_kind = grpc_channel`
- `transport_class = schema_rpc`
- `provenance = inferred`
- `confidence_basis = extends_impl_base`
- `symbol_stable_key`, `source_file`, location

**NOT exposed**:
- `evidence_json` — only in `BoundaryInteractionDetail`, not `BoundaryInteractionListItem`
- `boundary_contracts` associations — read path doesn't join contract elements
- Proto service identity — stored but not queryable through boundary surface

**Visibility gap**: An agent can find "there's a gRPC provider hint at this class" but
cannot see "it corresponds to proto service X" without direct DB query or new read-side work.

### Pending

1. **Smoke validation**: Real gRPC repo test (grpc-java examples or similar)
2. **Contract visibility**: Extend boundary read path to join `boundary_contracts`
3. **CLI test coverage**: No `rmap boundaries` test proves GR-1A hints visible
4. **CLI summary**: `rmap index`/`refresh` stderr summary doesn't include GR-1A counts
