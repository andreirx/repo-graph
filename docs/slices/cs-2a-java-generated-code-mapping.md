# CS-2A: Java Generated-Code Provenance Mapping

Status: VALIDATED (index path only; refresh path has known limitation)
Depends: CS-1 (Protobuf Schema Extraction)
Track: B (Schema-Backed RPC)

## Objective

Map checked-in Java generated protobuf/gRPC artifacts to top-level contract
elements (messages, enums, services). This enables cross-language boundary
linking: "this Java client uses this protobuf message defined in api.proto."

## Scope

### In scope
- Java protobuf generated classes (`*Protos.java`, `*OuterClass.java`)
- Java gRPC generated classes (`*Grpc.java`)
- Top-level elements only: messages, enums, services
- Checked-in generated files only (present in indexed snapshot)
- Explicit basis and confidence recording

### Out of scope (deferred)
- Build-output-only generated code (OUT_DIR, target/, build/)
- Field-level mapping
- Method-level provenance (beyond service-class association)
- Nested message/enum resolution
- Other languages (Python, C++, Rust, TypeScript → CS-2B, CS-2C)
- Build manifest inference

## Evidence Sources (Priority Order)

1. **Generated file naming conventions**
   - `*Protos.java` → protobuf outer class
   - `*OuterClass.java` → protobuf outer class (explicit naming)
   - `*Grpc.java` → gRPC service stubs

2. **`java_package` option**
   - Maps proto package to Java package namespace
   - From contract_elements metadata (CS-1)

3. **`java_outer_classname` option**
   - Determines outer wrapper class name
   - Default: CamelCase of proto filename

4. **Package + symbol normalization**
   - CamelCase conversion from snake_case
   - Proto package → Java package mapping

5. **gRPC stub naming patterns**
   - `ServiceNameGrpc.ServiceNameImplBase` → server
   - `ServiceNameGrpc.ServiceNameBlockingStub` → client
   - `ServiceNameGrpc.ServiceNameFutureStub` → async client

## Confidence Tiers

| Basis | Confidence | Description |
|-------|------------|-------------|
| `exact_option_match` | 0.95 | java_package + java_outer_classname match exactly |
| `option_package_match` | 0.90 | java_package matches, classname follows convention |
| `filename_convention` | 0.85 | Generated file pattern + symbol name match |
| `symbol_normalized_match` | 0.75 | Symbol name normalizes to schema element |
| `weak_wrapper_match` | 0.50 | Partial match via outer class wrapper |

**Policy:** Persist mappings at 0.50+ floor. Prefer 0.75+ for high-confidence linking.

## Java Protobuf Generation Patterns

### Standard protoc output

```protobuf
// api.proto
syntax = "proto3";
package api.v1;
option java_package = "com.example.api";
option java_outer_classname = "ApiProtos";

message User {
  string id = 1;
}

enum Status {
  UNKNOWN = 0;
  ACTIVE = 1;
}
```

Generates:
```
com/example/api/ApiProtos.java
  public final class ApiProtos {
    public static final class User extends GeneratedMessageV3 { }
    public enum Status implements ProtocolMessageEnum { }
  }
```

### Without java_outer_classname

Proto filename `user_service.proto` → outer class `UserService` (CamelCase).

### gRPC generation

```protobuf
service UserService {
  rpc GetUser(GetUserRequest) returns (User);
}
```

Generates:
```
com/example/api/UserServiceGrpc.java
  public final class UserServiceGrpc {
    public static abstract class UserServiceImplBase { }
    public static final class UserServiceBlockingStub { }
    public static final class UserServiceFutureStub { }
    public static final class UserServiceStub { }
  }
```

## Storage Schema

```sql
CREATE TABLE generated_code_mappings (
    mapping_uid TEXT PRIMARY KEY,
    snapshot_uid TEXT NOT NULL,
    schema_element_uid TEXT NOT NULL,
    generated_symbol_key TEXT NOT NULL,
    language TEXT NOT NULL,
    element_kind TEXT NOT NULL,  -- 'message', 'enum', 'service'
    mapping_basis TEXT NOT NULL,
    confidence REAL NOT NULL,
    evidence_json TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (snapshot_uid) REFERENCES snapshots(snapshot_uid),
    FOREIGN KEY (schema_element_uid) REFERENCES contract_elements(element_uid),
    UNIQUE (snapshot_uid, generated_symbol_key)
);

CREATE INDEX idx_generated_mappings_element ON generated_code_mappings(schema_element_uid);
CREATE INDEX idx_generated_mappings_symbol ON generated_code_mappings(generated_symbol_key);
CREATE INDEX idx_generated_mappings_snapshot ON generated_code_mappings(snapshot_uid);
```

## Mapping Algorithm

### Phase 1: Identify Generated Files

```rust
fn is_java_generated_proto(path: &str) -> bool {
    // Filename patterns
    path.ends_with("Protos.java") ||
    path.ends_with("OuterClass.java") ||
    // Content markers (fallback)
    // - extends GeneratedMessageV3
    // - @javax.annotation.Generated
}

fn is_java_generated_grpc(path: &str) -> bool {
    path.ends_with("Grpc.java")
}
```

### Phase 2: Extract Java Package from Symbols

For each Java class symbol in generated file:
- Extract package from qualified_name
- Extract class name hierarchy

### Phase 3: Match to Schema Elements

```rust
fn find_schema_match(
    java_class: &str,
    java_package: &str,
    schema_elements: &[ContractElement],
) -> Option<(ElementUid, MappingBasis, f64)> {
    // 1. Try exact option match
    //    - schema has java_package option matching java_package
    //    - schema has java_outer_classname option matching outer class
    
    // 2. Try option package match
    //    - schema java_package matches, infer classname from filename
    
    // 3. Try filename convention
    //    - outer class name derives from proto filename
    
    // 4. Try symbol normalization
    //    - CamelCase(schema.name) == java_class
}
```

### Phase 4: Persist Mappings

Store mappings above confidence floor with explicit basis.

## Implementation Steps

1. **Add storage migration** (migration_026)
   - `generated_code_mappings` table
   - Indexes

2. **Create `generated-code-mapper` support module**
   - Java generated file detection
   - Symbol name normalization
   - Option-aware matching
   - Confidence scoring

3. **Integrate with indexer orchestrator**
   - Run after source extraction completes
   - Requires both contract_elements and source symbols
   - Add to IndexResult reporting

4. **Add CLI surface**
   - `rmap contracts usages <db> <repo> <element_ref>`
   - Shows which generated code uses a schema element

5. **Tests**
   - Unit tests for normalization/matching
   - Integration tests with fixture protos + generated Java

## Test Matrix

1. Message class mapping with java_package option
2. Message class mapping with java_outer_classname option
3. Message class mapping without options (filename convention)
4. Enum mapping
5. Service class mapping (gRPC)
6. Multiple messages in same proto
7. Nested outer class structure
8. Package name normalization
9. CamelCase conversion accuracy
10. Confidence tier assignment
11. No-match scenarios (symbol exists but no schema)
12. Duplicate handling (same symbol, multiple candidates)

## Validation

### Hadoop validation (completed 2026-05-01)

Results:
- 28 mappings found from 3 checked-in generated Java files
- 28/28 high-confidence (0.95 exact_option_match)
- 0 false positives above 0.75 confidence

Note: Hadoop has 81 proto files but only 3 generated Java files checked in
(in `proto2-generated` directories). The remaining protos generate code at
build time only (gitignored `target/` directories). This limits validation
coverage but confirms mapping accuracy for the available files.

Files mapped:
- `ProtobufRpcEngineProtos.java` → 1 mapping
- `TestProtosLegacy.java` → 20 mappings
- `TestRpcServiceProtosLegacy.java` → 7 mappings

## Deliverables

- Storage migration for `generated_code_mappings` [DONE - migration_025 + migration_026]
  - migration_025: creates table (fresh installs with created_at)
  - migration_026: adds created_at column (upgrades existing DBs)
- `java_code_mapper` module in indexer crate [DONE]
  - `MappingBasis` enum with confidence tiers
  - `find_java_mappings()` core algorithm
  - Generated file detection patterns
  - Proto options parsing
  - gRPC stub pattern matching (ImplBase, BlockingStub, FutureStub, Stub)
  - 12 unit tests (8 original + 4 gRPC)
- Indexer orchestrator integration [DONE]
  - Runs after contract indexing + source extraction
  - `GeneratedCodeMappingReadPort` for querying elements and symbols
  - `GeneratedCodeMappingStorePort` for persisting mappings
  - Deterministic mapping UIDs via SHA-256 hash
  - `GeneratedCodeMappingResult` surfaced in `IndexResult` for explicit degradation
    - `mappings_persisted`, `high_confidence_count`
    - `element_query_error`, `symbol_query_error`, `storage_error`
- CLI `contracts usages` command [DONE]
  - `--element` filter by element UID
  - `--min-confidence` threshold filter
  - Evidence JSON in output
- CLI `index`/`refresh` mapping summary [DONE]
  - `print_mapping_summary()` surfaces persisted count and errors
  - Displayed after contract summary in stderr output
- Storage port implementations [DONE]
  - `query_contract_elements_with_options()` for mapper input
  - `query_java_symbols()` for mapper input
  - `list_generated_code_mappings()` for CLI query
  - `count_generated_code_mappings()` for stats
  - 4 storage tests, 4 read port tests
- Unit tests for matching logic [DONE - 12 tests]
- CLI tests for `contracts usages` [DONE - 11 tests]
  - Usage errors, DB errors, repo not found
  - Empty results, envelope contract shape
  - End-to-end mapping verification (proto+java fixture yields mappings)
  - gRPC service mapping verification (UserServiceGrpc.java required)
  - Mapping entry shape validation (uid, element_uid, symbol_key, confidence)
  - Filter reflection (--element, --min-confidence)
  - Argument validation (requires value, invalid value)
- CLI `format_mapping_summary` tests [DONE - 5 tests]
  - None/empty returns empty
  - Success with counts
  - Individual and multiple error surfacing
- Integration tests with fixture protos + generated Java [DONE - in contracts_command.rs]
- Hadoop validation [DONE - 28 mappings, 0 false positives]

## Success Criteria

- Java generated classes correctly mapped to schema elements [VERIFIED]
- Confidence scores reflect mapping quality [VERIFIED - all 0.95]
- Basis explicitly recorded for each mapping [VERIFIED - exact_option_match]
- CLI usages query returns accurate results [VERIFIED - index path]
- 0 false positives above 0.75 confidence on hadoop validation [VERIFIED - 0/28]

Note: Success criteria verified on `rmap index` path only. The `rmap refresh`
path has a known limitation where contract schemas are not copied forward
to the new snapshot. See TECH-DEBT.md "Contract schemas not copied forward".
