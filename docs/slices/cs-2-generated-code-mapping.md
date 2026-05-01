# CS-2: Generated Code Provenance Mapping

Status: PLANNED
Depends: CS-1 (Protobuf Schema Extraction)
Track: B (Schema-Backed RPC)

## Objective

Map generated code symbols (classes, functions, types) back to their source
schema elements. This enables cross-language boundary linking: "this Python
client uses this protobuf message defined in api.proto."

## Problem Statement

When code uses protobuf/gRPC, it typically uses generated stubs, not the raw
`.proto` definitions. Without mapping generated code back to schema:

- We know "code uses MyMessage class"
- We don't know "code uses package.MyMessage from api.proto"
- Cross-language linking remains weak

## Scope

### In scope
- Protobuf generated code patterns per language
- gRPC generated code patterns per language
- Mapping symbols to schema elements
- Confidence scoring for mappings
- C++, Rust, Python, Java, TypeScript coverage

### Out of scope
- Custom protobuf plugins
- Non-standard code generation
- Runtime-only protobuf usage (no codegen)

## Generated Code Patterns

### C++

**Protobuf:**
```
schema: package.proto
  message MyMessage

generated: package.pb.h, package.pb.cc
  class MyMessage : public ::google::protobuf::Message
  class MyMessage_NestedType

naming: CamelCase from snake_case
```

**gRPC:**
```
schema: service.proto
  service MyService
    rpc MyMethod

generated: service.grpc.pb.h, service.grpc.pb.cc
  class MyService::Stub
  class MyService::Service
```

### Rust

**Protobuf (prost):**
```
schema: package.proto
  message MyMessage

generated: package.rs (typically in OUT_DIR)
  pub struct MyMessage

naming: CamelCase from snake_case
```

**gRPC (tonic):**
```
schema: service.proto
  service MyService
    rpc MyMethod

generated: service.rs
  pub mod my_service_client
  pub mod my_service_server
  pub trait MyService
```

### Python

**Protobuf:**
```
schema: package.proto
  message MyMessage

generated: package_pb2.py
  class MyMessage

naming: preserved
```

**gRPC:**
```
schema: service.proto
  service MyService

generated: service_pb2_grpc.py
  class MyServiceServicer
  class MyServiceStub
```

### Java

**Protobuf:**
```
schema: package.proto
  option java_package = "com.example";
  option java_outer_classname = "PackageProtos";
  message MyMessage

generated: com/example/PackageProtos.java
  public static final class MyMessage extends GeneratedMessageV3

naming: CamelCase, respects java_package option
```

**gRPC:**
```
schema: service.proto
  service MyService

generated: MyServiceGrpc.java
  public static class MyServiceImplBase
  public static class MyServiceBlockingStub
  public static class MyServiceFutureStub
```

### TypeScript

**Protobuf (protobuf-ts, protoc-gen-ts):**
```
schema: package.proto
  message MyMessage

generated: package.ts or package_pb.ts
  export interface IMyMessage
  export class MyMessage

naming: varies by generator
```

**gRPC (grpc-web, grpc-js):**
```
schema: service.proto
  service MyService

generated: service_grpc_pb.ts or service_grpc_web_pb.ts
  export class MyServiceClient
```

## Mapping Strategy

### Phase 1: Filename Convention Matching

```
*.pb.h, *.pb.cc     -> protobuf C++ generated
*.grpc.pb.h         -> gRPC C++ generated
*_pb2.py            -> protobuf Python generated
*_pb2_grpc.py       -> gRPC Python generated
*Grpc.java          -> gRPC Java generated
*.pb.ts, *_pb.ts    -> protobuf TypeScript generated
```

### Phase 2: Symbol Name Mapping

For each generated symbol, compute candidate schema elements:

```
// C++ example
class MyMessage -> candidate: "MyMessage" message
class MyService::Stub -> candidate: "MyService" service

// Python example
class MyMessage -> candidate: "MyMessage" message
class MyServiceServicer -> candidate: "MyService" service

// Java example
class PackageProtos.MyMessage -> candidate: "MyMessage" message
  (also check java_outer_classname option)
```

### Phase 3: Package/Option Resolution

Use proto options to refine mapping:

```protobuf
option java_package = "com.example.api";
option java_outer_classname = "ApiProtos";
```

Map `com.example.api.ApiProtos.MyMessage` -> schema `package.MyMessage`

## Storage Schema

```sql
CREATE TABLE generated_code_mappings (
    mapping_uid TEXT PRIMARY KEY,
    snapshot_uid TEXT NOT NULL,
    schema_element_uid TEXT NOT NULL,
    generated_symbol_key TEXT NOT NULL,  -- stable key of generated code
    language TEXT NOT NULL,
    mapping_kind TEXT NOT NULL,  -- 'message', 'enum', 'service', 'method', 'field'
    mapping_basis TEXT NOT NULL,  -- 'filename_convention', 'name_match', 'option_match'
    confidence REAL NOT NULL,
    evidence_json TEXT,
    FOREIGN KEY (snapshot_uid) REFERENCES snapshots(snapshot_uid),
    FOREIGN KEY (schema_element_uid) REFERENCES contract_elements(element_uid),
    UNIQUE (snapshot_uid, generated_symbol_key)
);

CREATE INDEX idx_generated_mappings_element ON generated_code_mappings(schema_element_uid);
CREATE INDEX idx_generated_mappings_symbol ON generated_code_mappings(generated_symbol_key);
```

## Confidence Scoring

| Basis | Confidence |
|-------|------------|
| Exact name + filename convention | 0.95 |
| Name match + java_package option | 0.90 |
| Name match + filename convention | 0.85 |
| Name match only | 0.70 |
| Partial name match | 0.50 |

## Implementation Steps

1. **Filename convention detector**
   - Identify generated files by naming patterns
   - Associate with likely source .proto files

2. **Symbol name normalizer**
   - CamelCase/snake_case conversion
   - Language-specific suffix stripping (Servicer, Stub, etc.)

3. **Option-aware mapping**
   - Parse java_package, go_package options from schema
   - Use options to refine Java/Go mappings

4. **Cross-reference builder**
   - For each generated symbol, find candidate schema elements
   - Score candidates by matching basis
   - Store best match above confidence threshold

5. **CLI extensions**
   - `rmap contracts usages <db> <repo> <element_ref>`
   - Shows which code uses a given schema element

## Test Matrix

1. C++ message class mapping
2. C++ gRPC stub mapping
3. Rust prost struct mapping
4. Rust tonic trait mapping
5. Python pb2 class mapping
6. Python grpc servicer mapping
7. Java message class mapping (with java_package)
8. Java gRPC stub mapping
9. TypeScript message interface mapping
10. Nested message mapping
11. Enum mapping
12. Service method mapping
13. Confidence scoring accuracy

## Edge Cases

### Generated code not in repo
Some projects generate code at build time (OUT_DIR, build/).
- Detect via build manifest hints
- May require build-time integration (future)

### Multiple generators
Same proto, different generators (protobuf-ts vs protoc-gen-ts).
- Accept multiple mappings per schema element
- Distinguish by filename pattern

### Custom outer class names
Java `java_outer_classname` option changes class structure.
- Parse option from schema
- Apply to mapping resolution

## Deliverables

- Filename convention detector
- Symbol name normalizer
- Option-aware mapping resolver
- Storage schema migration
- CLI `contracts usages` command
- 20+ integration tests

## Success Criteria

- Generated code correctly mapped to schema elements
- Cross-language mappings working (Python client -> proto message)
- Confidence scores reflect mapping quality
- CLI usages query returns accurate results
- Nested elements mapped correctly
