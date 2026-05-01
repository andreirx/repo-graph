# CS-1: Protobuf Schema Extraction

Status: COMPLETE (full pipeline + CLI wired, smoke test passed)
Depends: Storage schema migration for contract tables
Track: B (Schema-Backed RPC)

## Objective

Parse `.proto` files and extract the schema graph: packages, messages, enums,
services, and methods. This is the foundation for all schema-backed RPC
detection (protobuf, gRPC, eRPC).

## Strategic Value

Protobuf provides:
- Explicit cross-language contract
- Machine-readable API definition
- Versioning/evolution surface
- Strong provider/consumer linking potential

This is the highest-value single addition for multi-language boundary detection.

## Scope

### In scope
- Proto3 syntax parsing
- Proto2 syntax parsing
- Package/namespace extraction
- Message and field extraction
- Enum extraction
- Service and RPC method extraction
- Import statement tracking
- Option extraction (package, java_package, go_package, etc.)
- Nested message/enum handling
- Line/column source anchoring

### Out of scope
- Import resolution across files (future slice)
- Generated code mapping (CS-2)
- gRPC-specific detection (GR-1, GR-2)
- Schema validation/linting
- Schema evolution analysis

## Schema Model

```rust
pub struct ProtoFile {
    pub path: String,
    pub syntax: ProtoSyntax,  // Proto2 | Proto3
    pub package: Option<String>,
    pub imports: Vec<ProtoImport>,
    pub options: Vec<ProtoOption>,
    pub messages: Vec<ProtoMessage>,
    pub enums: Vec<ProtoEnum>,
    pub services: Vec<ProtoService>,
    pub extensions: Vec<ProtoExtension>,  // proto2
}

pub struct ProtoImport {
    pub path: String,
    pub kind: ImportKind,  // Default | Public | Weak
    pub line: u32,
}

pub struct ProtoMessage {
    pub name: String,
    pub full_name: String,  // package.MessageName
    pub fields: Vec<ProtoField>,
    pub oneofs: Vec<ProtoOneof>,
    pub nested_messages: Vec<ProtoMessage>,
    pub nested_enums: Vec<ProtoEnum>,
    pub reserved: Vec<ProtoReserved>,
    pub options: Vec<ProtoOption>,
    pub line_start: u32,
    pub line_end: u32,
}

pub struct ProtoField {
    pub name: String,
    pub number: i32,
    pub label: FieldLabel,  // Optional | Required | Repeated
    pub type_name: String,
    pub type_kind: FieldTypeKind,  // Scalar | Message | Enum | Map
    pub default_value: Option<String>,  // proto2
    pub options: Vec<ProtoOption>,
    pub line: u32,
}

pub struct ProtoEnum {
    pub name: String,
    pub full_name: String,
    pub values: Vec<ProtoEnumValue>,
    pub options: Vec<ProtoOption>,
    pub line_start: u32,
    pub line_end: u32,
}

pub struct ProtoService {
    pub name: String,
    pub full_name: String,
    pub methods: Vec<ProtoMethod>,
    pub options: Vec<ProtoOption>,
    pub line_start: u32,
    pub line_end: u32,
}

pub struct ProtoMethod {
    pub name: String,
    pub input_type: String,
    pub output_type: String,
    pub client_streaming: bool,
    pub server_streaming: bool,
    pub options: Vec<ProtoOption>,
    pub line_start: u32,
    pub line_end: u32,
}
```

## Parser Implementation

### Option A: tree-sitter-protobuf

Use existing tree-sitter grammar for protobuf:
- https://github.com/tree-sitter/tree-sitter-proto (community)
- Consistent with other extractors
- WASM-compatible for TS path

### Option B: pest/nom parser

Custom parser in Rust:
- More control over error recovery
- Potentially better performance
- Harder to maintain

**Recommendation:** Start with tree-sitter-protobuf for consistency.

## Storage Schema

```sql
CREATE TABLE contract_schemas (
    schema_uid TEXT PRIMARY KEY,
    snapshot_uid TEXT NOT NULL,
    repo_uid TEXT NOT NULL,
    schema_kind TEXT NOT NULL,  -- 'protobuf'
    file_path TEXT NOT NULL,
    syntax_version TEXT,  -- 'proto2', 'proto3'
    package_name TEXT,
    content_hash TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (snapshot_uid) REFERENCES snapshots(snapshot_uid),
    UNIQUE (snapshot_uid, file_path)
);

CREATE TABLE contract_elements (
    element_uid TEXT PRIMARY KEY,
    schema_uid TEXT NOT NULL,
    element_kind TEXT NOT NULL,  -- 'message', 'enum', 'service', 'method', 'field'
    name TEXT NOT NULL,
    full_name TEXT NOT NULL,  -- package.Name or package.Parent.Name
    parent_element_uid TEXT,  -- for nested elements
    line_start INTEGER,
    line_end INTEGER,
    metadata_json TEXT,  -- element-specific data
    FOREIGN KEY (schema_uid) REFERENCES contract_schemas(schema_uid),
    FOREIGN KEY (parent_element_uid) REFERENCES contract_elements(element_uid)
);

CREATE INDEX idx_contract_elements_schema ON contract_elements(schema_uid);
CREATE INDEX idx_contract_elements_full_name ON contract_elements(full_name);
CREATE INDEX idx_contract_elements_kind ON contract_elements(element_kind);
```

## Element Metadata

### Message metadata
```json
{
  "fields_count": 5,
  "oneofs": ["result"],
  "reserved_numbers": [4, 5],
  "reserved_names": ["old_field"]
}
```

### Field metadata
```json
{
  "number": 1,
  "label": "repeated",
  "type_name": "string",
  "type_kind": "scalar",
  "default_value": null
}
```

### Service metadata
```json
{
  "methods_count": 3
}
```

### Method metadata
```json
{
  "input_type": "package.RequestMessage",
  "output_type": "package.ResponseMessage",
  "client_streaming": false,
  "server_streaming": true
}
```

## CLI Surface

```
rmap contracts list <db> <repo> [--kind protobuf]
  List all contract schemas

rmap contracts show <db> <repo> <file_path>
  Show schema details with all elements

rmap contracts elements <db> <repo> [--kind message|enum|service|method]
  List contract elements with filtering

rmap contracts search <db> <repo> <query>
  Search elements by name pattern
```

## Architecture: Dual-Pipeline Model

Contract files (`.proto`) are **not** treated as another language in the source
extraction pipeline. Instead, the orchestrator runs two parallel subpipelines
under a single snapshot lifecycle:

```
files ──┬── source_files ──→ language extractors ──→ nodes/edges
        │
        └── contract_files ─→ proto indexer ───────→ schemas/elements
```

### Key Design Decisions

1. **Parallel detection, not fake language routing**: `routing.rs` provides
   `is_contract_extension()` and `detect_contract_kind()` parallel to the
   existing `is_source_extension()` and `detect_language()`. Proto files are
   never routed through the language extraction pipeline.

2. **Orchestrator owns partitioning**: The scan phase partitions files into
   source vs. contract sets. Both are passed to `index_repo()` / `refresh_repo()`.

3. **Shared snapshot lifecycle**: Contract indexing runs within the same snapshot
   transaction as source extraction. Both succeed or fail atomically.

4. **Separate storage ports**: `ProtoSchemaStorePort` is independent from
   `NodeStorePort`/`EdgeStorePort`. Contract schemas/elements go to
   `contract_schemas`/`contract_elements` tables, not the node/edge tables.

5. **Unified result**: `IndexResult.contracts` reports schema and element counts
   plus any parse failures, alongside the existing source extraction metrics.

### File Classification

```rust
// routing.rs
pub fn is_contract_extension(ext: &str) -> bool {
    matches!(ext, ".proto")
}

pub fn detect_contract_kind(file_path: &str) -> Option<ContractKind> {
    match get_extension(file_path) {
        ".proto" => Some(ContractKind::Protobuf),
        _ => None,
    }
}
```

Future contract kinds (OpenAPI, GraphQL, Thrift) will extend `ContractKind` and
add detection branches here.

## Implementation Steps

1. **Create `contract-schema` crate** [DONE]
   - Proto AST types in `contract-schema/src/proto.rs`
   - Proto2 and Proto3 parsing via `tree-sitter-proto`
   - ProtoFile AST construction with full element extraction

2. **Create storage schema migration** [DONE]
   - contract_schemas table
   - contract_elements table
   - Indexes for efficient queries
   - `ProtoSchemaStorePort` trait in indexer

3. **Integrate with indexer** [DONE]
   - Scanner admits contract extensions alongside source extensions
   - Compose layer partitions files via `is_contract_extension()`
   - Contract detection via `is_contract_extension()` / `detect_contract_kind()`
   - Dual-pipeline architecture in orchestrator
   - Contract files tracked in file catalog (`tracked_files`, `file_versions`)
   - Contract files included in `files_total` and `all_file_paths`
   - File version `parse_status` reflects actual parse outcome
   - `proto_indexer.rs` module with `index_proto_files()` function
   - `ContractIndexResult` reported in `IndexResult.contracts`
   - All storage errors surfaced via `ContractIndexResult.storage_error`
     (schema storage + file catalog writes)

4. **Add CLI commands** [DONE]
   - `rmap contracts list` - list schemas with optional kind filter
   - `rmap contracts show` - show schema with elements by file path
   - `rmap contracts elements` - list elements with kind/file filters
   - Wire through rmap command handlers in `commands/contracts.rs`

5. **Add import tracking** [PARTIAL]
   - Import paths stored in ProtoFile.imports
   - Cross-file resolution deferred to future slice

## Test Matrix

1. Proto3 message parsing
2. Proto2 message parsing with required/optional
3. Nested message handling
4. Enum parsing
5. Service and method parsing
6. Streaming method classification
7. Package extraction
8. Import statement extraction
9. Option extraction (java_package, go_package)
10. Reserved field handling
11. Oneof parsing
12. Map field parsing
13. Error recovery on malformed protos

## Validation Repos

- Any repo with `.proto` files
- Consider using protobuf's own test fixtures
- gRPC examples repo

## Deliverables

- `contract-schema` crate (parser + AST types) [DONE]
- Storage migration for contract tables [DONE]
- Indexer integration for `.proto` files [DONE]
- CLI commands for contract queries [DONE]
- CLI contract tests in `rgr/tests/contracts_command.rs` [DONE: 23 tests]
- Parser tests in `contract-schema` [DONE]
- Proto indexer tests in `indexer/proto_indexer.rs` [DONE: 5 tests]

## Success Criteria

- All protobuf language features parsed (proto2 + proto3)
- Schema graph persisted with proper nesting
- Full names computed correctly (package.Parent.Element)
- Source anchoring (line numbers) accurate
- CLI queries working
- Import statements captured for future resolution
