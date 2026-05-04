# GR-1B: Java gRPC Server Registration Proof

Status: FIXTURE-VALIDATED
Depends: GR-1A (gRPC server impl hints)
Track: B (Schema-Backed RPC)

## Objective

Detect `addService()` / `bindService()` calls that register gRPC service
implementations with a server. This provides registration proof that an
implementation class is actually wired into a server configuration.

**This is hint-strengthening, not a new discovery surface.**

GR-1A surfaces have confidence 0.85 (implementation exists but may be dead code).
GR-1B boosts confidence to 0.90 (implementation is registered somewhere).

## Scope

### In scope
- Detect `.addService(new FooImpl())` inline instantiation patterns
- Match registered class to existing GR-1A surface
- Boost surface confidence from 0.85 to 0.90
- Append registration site evidence to evidence_json

### Out of scope
- Variable reference patterns like `.addService(fooImpl)` — requires type resolution
- Creating separate registration-site entries (rejected in design)
- Endpoint detection (port, bind address) — GR-1C
- Runtime proof (only static registration evidence)
- TypeScript/Python/Rust registration detection — future slices

## Detection Patterns

### Java gRPC (io.grpc)

```java
// Supported: Inline instantiation
server = ServerBuilder.forPort(port)
    .addService(new GreeterImpl())  // GR-1B detects this
    .build()
    .start();

// NOT supported: Variable reference (requires type resolution)
GreeterImpl greeter = new GreeterImpl();
server = ServerBuilder.forPort(port)
    .addService(greeter)  // NOT detected - variable name != class name
    .build()
    .start();
```

**Detection signals:**
- CALLS edge where `target_key` contains `.addService(` or `bindService(`
- Regex extracts class name from `new ClassName()` pattern
- Delimiter-aware match to GR-1A surfaces (`#ClassName:` or `.ClassName:`)

## Implementation Approach

### Option A: Hint-Strengthening (SELECTED)

1. **Query addService calls:**
   - Find CALLS edges where `target_key LIKE '%addService(%'`
   - Extract the argument using regex: `addService\((new\s+)?(\w+)`

2. **Match to GR-1A surfaces:**
   - For each extracted class name, find boundary_interaction_surfaces
     where `symbol_stable_key` contains the class name
   - OR find via INSTANTIATES edges → nodes → surfaces

3. **Boost confidence:**
   - UPDATE boundary_interaction_surfaces SET confidence = 0.90
   - Only if current basis is "extends_impl_base" (GR-1A)

4. **Append registration evidence:**
   - Add to evidence_json: `registration_sites: [{file, line, method, pattern}]`

### Why Not Separate Entries

The product philosophy is orientation, not reconstruction. A separate
registration-site entry would:
- Add surface complexity without discovery value
- Require extra joins for basic queries
- Drift toward topology reconstruction

Registration proof strengthens an existing hint. It is not a primary artifact.

## Confidence Model

| Stage | Confidence | Basis |
|-------|------------|-------|
| GR-1A only | 0.85 | extends_impl_base |
| GR-1A + GR-1B | 0.90 | extends_impl_base + registration |

Note: Still not runtime proof. The implementation is registered in source,
not necessarily executed.

## Evidence Schema

GR-1A evidence_json:
```json
{
  "impl_base": "GreeterGrpc.GreeterImplBase"
}
```

GR-1B appends:
```json
{
  "impl_base": "GreeterGrpc.GreeterImplBase",
  "registration_sites": [
    {
      "file": "src/main/java/.../HelloWorldServer.java",
      "line": 18,
      "pattern": "addService(new GreeterImpl())",
      "method": "start"
    }
  ]
}
```

## Storage Changes

No schema changes. Uses existing columns:
- `confidence` column (0.85 → 0.90)
- `evidence_json` column (append registration_sites array)

## Substrate Assumption

Same as GR-1A: requires generated `*Grpc.java` stubs to be present in the
indexed tree. This is inherited — if GR-1A found surfaces, GR-1B can boost them.

## Implementation Steps

1. Add `query_add_service_calls` to storage
2. Add `boost_grpc_impl_confidence` to storage
3. Create `GrpcRegistrationProofPort` trait in indexer
4. Implement port for StorageConnection
5. Wire into orchestrator (after GR-1A, before refresh completion)
6. Add tests on grpc-java-minimal fixture
7. Validate confidence boost visible in `rmap boundaries list`

## Validation Criteria

- [x] Fixture: GreeterImpl surface confidence = 0.90 after GR-1B
- [x] Fixture: evidence_json contains registration_sites array
- [x] CLI: `rmap boundaries list` shows boosted confidence
- [x] No new surfaces created (only existing surfaces modified)

## Implementation Notes (2026-05-04)

Files added/modified:
- `rust/crates/indexer/src/grpc_registration_proof.rs` — GR-1B pass function
- `rust/crates/indexer/src/storage_port.rs` — GrpcRegistrationProofPort trait
- `rust/crates/storage/src/grpc_impl_hint_impl.rs` — storage queries
- `rust/crates/storage/src/grpc_impl_hint_port_impl.rs` — port implementation
- `rust/crates/indexer/src/orchestrator.rs` — wiring after GR-1A

Key implementation details:
- Uses CALLS edges where `target_key LIKE '%addService(%'` to find registration calls
- Regex extracts class name from `new ClassName()` patterns only
- **Disambiguation strategy** for same-name classes in different files:
  1. First try same-file match (inner class pattern, most common for gRPC)
  2. Fall back to cross-file match ONLY if exactly one surface matches
  3. If multiple surfaces match across files, refuse to boost (no false positives)
- Delimiter-aware matching: `#ClassName:SYMBOL:CLASS` or `.ClassName:SYMBOL:CLASS`
  - Prevents `GreeterImpl` from matching `MyGreeterImpl`
- Basis changes from "extends_impl_base" to "extends_impl_base_registered" in storage
- Read path maps both to `InteractionBasis::ExtendsImplBase` (distinction via confidence)
- Confidence boost is idempotent (0.90 regardless of how many registrations found)
- Variable references NOT supported (would require type resolution from INSTANTIATES edges)

**Degradation on ambiguity:** When multiple top-level classes share the same simple name
across different files, GR-1B skips the boost rather than risking a false positive.
The GR-1A surface remains at confidence 0.85.
