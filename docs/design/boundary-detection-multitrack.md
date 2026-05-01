# Multi-Track Boundary Detection Architecture

Status: DESIGN
Created: 2026-05-01

## Problem Statement

Raw transport mechanisms (sockets, shared memory) and schema-backed RPC systems
(protobuf, gRPC, eRPC) are fundamentally different problem classes that share
the same underlying business model. Forcing them into a single implementation
track distorts both.

This document defines a two-track architecture over a unified boundary model.

## Problem Class Separation

### Class A: Raw Transport/Mechanism

Examples:
- Unix/TCP/UDP sockets
- POSIX shared memory, mmap
- SharedArrayBuffer (JS/TS)
- Named pipes, message queues

Detection sources:
- API usage patterns
- Runtime configuration
- Protocol framing conventions
- Build/runtime context

Characteristics:
- Noisier detection
- Weaker contract association
- Endpoint identity often in config
- Protocol contract external to callsite

### Class B: Schema-Backed RPC/Message Contracts

Examples:
- Protocol Buffers
- gRPC
- eRPC (embedded RPC)
- Cap'n Proto, FlatBuffers (future)

Detection sources:
- IDL/schema files (.proto, .erpc, etc.)
- Generated code patterns
- Provider registration
- Client stub invocation
- Transport wiring

Characteristics:
- Precise contract extraction
- Cross-language by design
- Explicit versioning surface
- Strong provider/consumer linking

## Unified Boundary Model

Both tracks feed the same business model. Extensions to `BoundaryInteractionSurface`:

**Critical design constraint:** `boundary_scope` and `transport_class` are orthogonal.

- `boundary_scope` answers: "What physical/process boundary does this cross?"
  - intra_process: same OS process, different execution contexts (threads, workers, isolates)
  - inter_process: same device, different OS processes
  - inter_device: different devices (network, serial, etc.)
  - unknown: cannot determine

- `transport_class` answers: "What kind of mechanism carries this interaction?"
  - raw_socket: bare TCP/UDP/Unix without higher-level framing
  - raw_ipc: pipes, shared memory, message queues
  - schema_rpc: protobuf, gRPC, eRPC (contract-backed)
  - message_broker: Kafka, RabbitMQ (future)
  - custom_protocol: application-specific framing

A gRPC call over TCP to a remote host:
- scope = inter_device
- transport_class = schema_rpc

A gRPC call over Unix socket to local service:
- scope = inter_process
- transport_class = schema_rpc

A raw TCP socket to localhost:
- scope = inter_process
- transport_class = raw_socket

SharedArrayBuffer between main thread and Web Worker:
- scope = intra_process (same OS process, different execution contexts)
- transport_class = raw_ipc

POSIX shared memory between two processes:
- scope = inter_process
- transport_class = raw_ipc

This separation enables uniform queries:
- "All inter-device boundaries" (regardless of mechanism)
- "All schema-backed RPC" (regardless of physical topology)
- "Raw sockets crossing device boundaries" (intersection)
- "All intra-process concurrency boundaries" (SAB, thread shared memory)

```
surface:
  - who exposes/consumes the boundary
  - symbol_stable_key, source_file, line range

channel:
  - socket path, host:port, shm key, topic, service name
  - channel_kind, channel_identity

contract:                           # NEW
  - protobuf message type
  - RPC method signature
  - stream shape
  - shared-memory layout reference
  - contract_kind: protobuf_message | grpc_method | erpc_method | 
                   shm_layout | custom_binary | none

scope:
  - intra_process                   # same OS process, different execution contexts
  - inter_process                   # same device, different OS processes
  - inter_device                    # different devices
  - unknown

transport_class:                    # NEW: orthogonal to scope
  - raw_socket                      # bare TCP/UDP/Unix socket
  - raw_ipc                         # pipes, shm, mqueue
  - schema_rpc                      # protobuf, gRPC, eRPC
  - message_broker                  # Kafka, RabbitMQ, etc. (future)
  - custom_protocol                 # application-specific framing

interaction_pattern:
  - request_response
  - unary_stream
  - bidirectional_stream
  - publish_subscribe
  - shared_state

provenance:                         # NEW
  - extracted      (from source code)
  - configured     (from config files)
  - generated      (from IDL/schema)
  - inferred       (from naming/patterns)
  - declared       (from annotations/markers)

confidence:
  - syntax_proven  (AST extraction)
  - schema_proven  (IDL parsing)
  - config_proven  (config file extraction)
  - heuristic      (pattern matching)
```

## Detection Maturity Ladder

### Level 1: Mechanism Presence
"We saw socket/bind/connect."
"We saw SharedArrayBuffer."
"We saw protobuf/gRPC artifacts."

Cheap, broad, low semantic power.

### Level 2: Boundary Surfaces
"We know provider/consumer roles and channel identity."

This is where current BI-1A sits.

### Level 3: Contract Association
"We know what message/service/layout crosses the boundary."

This is where protobuf/gRPC/eRPC become high-value.

### Level 4: Provider/Consumer Linking
"We know this client calls this service/method over this channel."

Strongest level. Requires levels 1-3 to be stable first.

## Architecture: Two Tracks, One Model

```
                    ┌─────────────────────────────────────────┐
                    │     Unified Boundary Model              │
                    │  (BoundaryInteractionSurface + Channel  │
                    │   + Contract + Provenance)              │
                    └──────────────┬──────────────────────────┘
                                   │
              ┌────────────────────┴────────────────────┐
              │                                         │
    ┌─────────▼─────────┐                   ┌──────────▼──────────┐
    │  Track A:         │                   │  Track B:           │
    │  Raw Transport    │                   │  Schema-Backed RPC  │
    └─────────┬─────────┘                   └──────────┬──────────┘
              │                                        │
    ┌─────────┴─────────┐               ┌──────────────┴──────────────┐
    │                   │               │                             │
    ▼                   ▼               ▼                             ▼
┌────────┐        ┌─────────┐    ┌───────────┐              ┌──────────────┐
│Sockets │        │ Shared  │    │ Protobuf  │              │   gRPC       │
│        │        │ Memory  │    │ Schema    │              │   Framework  │
└────────┘        └─────────┘    └───────────┘              └──────────────┘
    │                   │               │                          │
    ▼                   ▼               ▼                          ▼
┌─────────────────────────────┐  ┌────────────────────────────────────────┐
│ Language Adapters:          │  │ Language Adapters:                     │
│ C, C++, Rust, Python,       │  │ C++, Rust, Python, Java, Kotlin,       │
│ Java, TypeScript            │  │ TypeScript, Go (future)                │
└─────────────────────────────┘  └────────────────────────────────────────┘
```

## Required Support Modules

### Module 1: Contract/IDL Substrate (`repo-graph-contract-schema`)

Owns:
- Protobuf schema model (packages, messages, enums, services, methods)
- gRPC service/method model (unary, client-stream, server-stream, bidi)
- eRPC IDL model (future)
- Generated-code provenance mapping contracts

Zero runtime dependencies. Pure domain model.

```rust
// Core types
pub struct ProtoFile {
    pub path: String,
    pub package: String,
    pub messages: Vec<ProtoMessage>,
    pub enums: Vec<ProtoEnum>,
    pub services: Vec<ProtoService>,
}

pub struct ProtoMessage {
    pub name: String,
    pub full_name: String,  // package.MessageName
    pub fields: Vec<ProtoField>,
    pub nested_messages: Vec<ProtoMessage>,
    pub line_start: u32,
    pub line_end: u32,
}

pub struct ProtoService {
    pub name: String,
    pub full_name: String,
    pub methods: Vec<ProtoMethod>,
}

pub struct ProtoMethod {
    pub name: String,
    pub input_type: String,
    pub output_type: String,
    pub client_streaming: bool,
    pub server_streaming: bool,
}

// Generated code mapping
pub struct GeneratedCodeMapping {
    pub schema_file: String,
    pub schema_element: String,  // full_name of message/service/method
    pub generated_symbol: String, // stable key of generated code
    pub language: String,
}
```

### Module 2: Transport Interaction Expansion (`repo-graph-boundary-interaction`)

Extends existing crate with:
- Raw socket expansion (TCP/UDP beyond Unix)
- SharedArrayBuffer/shared-memory detection
- Endpoint/config association
- Serial/CAN/device transport facts (future)

New channel kinds:
```rust
pub enum ChannelKind {
    // Existing (BI-1A)
    UnixSocket,
    NamedPipe,
    AnonymousPipe,
    SharedMemory,
    MessageQueue,
    
    // Track A expansion
    TcpSocket,
    UdpSocket,
    SharedArrayBuffer,  // JS/TS specific
    MemoryMappedFile,
    
    // Track B (schema-backed)
    GrpcChannel,
    ProtobufStream,
    ErpcChannel,
    
    // Device (future)
    SerialPort,
    CanBus,
    I2c,
    Spi,
}
```

### Module 3: Schema Parser (`repo-graph-proto-parser`)

Owns:
- `.proto` file parsing (tree-sitter-protobuf or custom)
- Schema graph construction
- Import resolution
- Option extraction

Output: `ProtoFile` structures for indexing.

### Module 4: Framework Adapters (per-language crates)

Each language needs adapters for:
- Registration point detection (server-side)
- Stub/client construction detection
- Method invocation detection
- Transport builder configuration

## Cross-Language Capability Matrix

### Capability 1: Schema/IDL Extraction

| Mechanism | Parser | Output |
|-----------|--------|--------|
| Protobuf | tree-sitter-protobuf | ProtoFile |
| gRPC | (reuses protobuf) | ProtoService + transport |
| eRPC | custom IDL parser | ErpcService |

### Capability 2: Generated-Code Provenance Mapping

| Language | Protobuf Pattern | gRPC Pattern |
|----------|------------------|--------------|
| C++ | `*.pb.h`, `*.pb.cc` | `*.grpc.pb.h` |
| Rust | `*.rs` in OUT_DIR | tonic-generated |
| Python | `*_pb2.py` | `*_pb2_grpc.py` |
| Java | `*.java` in generated | `*Grpc.java` |
| TypeScript | `*_pb.ts` | `*_grpc_pb.ts` |

### Capability 3: Framework Adapter Detection

| Language | gRPC Server | gRPC Client |
|----------|-------------|-------------|
| C++ | `grpc::Service` impl | `Stub` usage |
| Rust | `tonic::async_trait` | `XxxClient::new()` |
| Python | `add_XxxServicer_to_server` | `XxxStub()` |
| Java | `bindService()` | `newBlockingStub()` |
| TypeScript | `server.addService()` | `new XxxClient()` |

### Capability 4: Config/Runtime Endpoint Extraction

Sources:
- Literals in code
- Local constants
- Config files (YAML, TOML, JSON)
- Environment variables
- Startup wiring / builder patterns

### Capability 5: Contract-to-Boundary Association

Examples:
- Socket carries protobuf message X
- gRPC method Y uses protobuf request/response Z
- Shared memory region R uses layout L
- eRPC transport T exposes service S

This requires correlation across detection layers.

## Per-Mechanism Requirements

### Sockets (Track A)

**Must detect:**
- Socket creation (family/type/protocol)
- Bind/listen/accept/connect
- Send/recv/read/write wrappers
- Endpoint identity (path, host:port, loopback vs remote)
- Direction (provider, consumer, bidirectional)
- Framing hints (raw, length-prefixed, protobuf, custom)

**Language coverage:**

| Language | API | Wrapper Libraries |
|----------|-----|-------------------|
| C/C++ | POSIX socket API | - |
| Rust | `std::net`, `tokio::net`, `mio` | `uds` |
| Python | `socket`, `asyncio` | - |
| Java | `java.net.Socket`, `ServerSocket` | Netty |
| TypeScript | `net`, `dgram` | - |

### SharedArrayBuffer / Shared Memory (Track A)

**JS/TS SharedArrayBuffer:**
- Allocation sites
- Worker producer/consumer roles
- Atomics usage
- Typed array views over SAB
- Message-passing bridge patterns

**Native shared memory:**
- Region creation/open (`shm_open`, `mmap`)
- Region identity
- Synchronization primitives nearby
- Layout contract if explicit

**Key insight:** Shared memory is both a channel AND shared state. Detection
must produce both boundary-interaction facts AND state-boundary facts.

### Protobuf (Track B)

**Must detect:**
- `.proto` files
- Packages, messages, enums, services, methods
- Generated code mapping back to schema
- Serializer/deserializer usage
- Which boundaries use which contracts

**Why high-value:**
- Explicit contract
- Cross-language bridge
- Versioning surface
- Strong provider/consumer linking

### gRPC (Track B)

**Must detect:**
- Service definitions in `.proto`
- Server-side registration/implementation
- Client stub creation/invocation
- Unary vs streaming methods
- Transport endpoint if configured
- Interceptor/middleware layers

**Requires three correlated detectors:**
1. `.proto` schema detector
2. Generated-code mapper
3. Framework registration/invocation detector

Without all three: "protobuf exists" but not "who talks to whom."

### eRPC (Track B)

**Must detect:**
- IDL/service definitions
- Generated client/server artifacts
- Transport selection
- Service registration
- Method invocation
- Serialization buffer boundaries
- Device/process scope

**Strategic importance:** eRPC often sits at:
- Core/device boundary
- Processor/RTOS boundary
- MCU/host boundary

This aligns directly with the legacy-code relationship product center.

## Storage Schema Extensions

### New tables

```sql
-- Contract/schema storage
CREATE TABLE contract_schemas (
    schema_uid TEXT PRIMARY KEY,
    snapshot_uid TEXT NOT NULL,
    schema_kind TEXT NOT NULL,  -- 'protobuf', 'grpc', 'erpc'
    file_path TEXT NOT NULL,
    package_name TEXT,
    content_hash TEXT NOT NULL,
    FOREIGN KEY (snapshot_uid) REFERENCES snapshots(snapshot_uid)
);

CREATE TABLE contract_elements (
    element_uid TEXT PRIMARY KEY,
    schema_uid TEXT NOT NULL,
    element_kind TEXT NOT NULL,  -- 'message', 'enum', 'service', 'method', 'field'
    name TEXT NOT NULL,
    full_name TEXT NOT NULL,
    parent_element_uid TEXT,  -- for nested elements
    line_start INTEGER,
    line_end INTEGER,
    metadata_json TEXT,
    FOREIGN KEY (schema_uid) REFERENCES contract_schemas(schema_uid)
);

CREATE TABLE generated_code_mappings (
    mapping_uid TEXT PRIMARY KEY,
    snapshot_uid TEXT NOT NULL,
    schema_element_uid TEXT NOT NULL,
    generated_symbol_key TEXT NOT NULL,
    language TEXT NOT NULL,
    confidence REAL NOT NULL,
    FOREIGN KEY (snapshot_uid) REFERENCES snapshots(snapshot_uid),
    FOREIGN KEY (schema_element_uid) REFERENCES contract_elements(element_uid)
);

-- Contract-to-boundary association
CREATE TABLE boundary_contracts (
    association_uid TEXT PRIMARY KEY,
    surface_uid TEXT NOT NULL,
    contract_element_uid TEXT,  -- NULL for raw transport
    contract_kind TEXT NOT NULL,
    association_basis TEXT NOT NULL,  -- 'schema_type', 'usage_site', 'config', 'inferred'
    confidence REAL NOT NULL,
    evidence_json TEXT,
    FOREIGN KEY (surface_uid) REFERENCES boundary_interaction_surfaces(surface_uid),
    FOREIGN KEY (contract_element_uid) REFERENCES contract_elements(element_uid)
);
```

### Extended boundary_interaction_surfaces

```sql
ALTER TABLE boundary_interaction_surfaces ADD COLUMN transport_class TEXT;
-- 'raw_socket', 'raw_ipc', 'schema_rpc', 'message_broker', 'custom_protocol'

ALTER TABLE boundary_interaction_surfaces ADD COLUMN provenance TEXT;
-- 'extracted', 'configured', 'generated', 'inferred', 'declared'

ALTER TABLE boundary_interaction_surfaces ADD COLUMN confidence_basis TEXT;
-- 'syntax_proven', 'schema_proven', 'config_proven', 'heuristic'
```

**Query examples with orthogonal dimensions:**

```sql
-- All inter-device boundaries regardless of mechanism
SELECT * FROM boundary_interaction_surfaces WHERE boundary_scope = 'inter_device';

-- All schema-backed RPC regardless of physical topology
SELECT * FROM boundary_interaction_surfaces WHERE transport_class = 'schema_rpc';

-- Raw sockets crossing device boundaries
SELECT * FROM boundary_interaction_surfaces 
WHERE transport_class = 'raw_socket' AND boundary_scope = 'inter_device';

-- Local IPC (scope is inter_process, class is raw_ipc)
SELECT * FROM boundary_interaction_surfaces 
WHERE boundary_scope = 'inter_process' AND transport_class = 'raw_ipc';
```

## CLI Surface

### Schema commands

```
rmap contracts list <db> <repo> [--kind protobuf|grpc|erpc]
rmap contracts show <db> <repo> <schema_ref>
rmap contracts usages <db> <repo> <element_ref>
```

### Extended boundaries commands

```
rmap boundaries list <db> <repo> --contract <element_ref>
rmap boundaries links <db> <repo>  # provider/consumer linking
```

## Recommended Attack Order

### Option C: Split Track (Recommended)

Run both tracks in parallel, sharing the same boundary model.

**Track A (Raw Transport) sequence:**
1. BI-1B: TCP/UDP sockets (scope heuristics for inter_process vs inter_device vs unknown)
2. BI-1C: SharedArrayBuffer (JS/TS worker boundaries, intra_process scope)
3. BI-1D: Shared memory dual projection (boundary + state)

**Track B (Schema-Backed RPC) sequence:**
1. CS-1: Protobuf schema extraction (`.proto` parser)
2. CS-2: Generated-code mapping (C++, Rust, Python first)
3. GR-1: gRPC server detection (registration patterns)
4. GR-2: gRPC client detection (stub invocation)
5. GR-3: gRPC provider/consumer linking
6. ER-1: eRPC IDL extraction (later, after gRPC is mature)

**Shared infrastructure:**
- Contract/IDL substrate module (before CS-1)
- Transport expansion module (before BI-1B)
- Storage schema migration (before both tracks)

## Slice Dependency Graph

```
                        ┌─────────────────────┐
                        │ Schema Migration    │
                        │ (storage extension) │
                        └──────────┬──────────┘
                                   │
              ┌────────────────────┴────────────────────┐
              │                                         │
    ┌─────────▼─────────┐                   ┌──────────▼──────────┐
    │ BI-1B: TCP/UDP    │                   │ CS-1: Protobuf      │
    │ sockets           │                   │ schema extraction   │
    └─────────┬─────────┘                   └──────────┬──────────┘
              │                                        │
    ┌─────────▼─────────┐                   ┌──────────▼──────────┐
    │ BI-1C: SAB/worker │                   │ CS-2: Generated     │
    │ boundaries        │                   │ code mapping        │
    └─────────┬─────────┘                   └──────────┬──────────┘
              │                                        │
    ┌─────────▼─────────┐                   ┌──────────▼──────────┐
    │ BI-1D: Shared     │                   │ GR-1: gRPC server   │
    │ memory dual proj  │                   │ detection           │
    └─────────┴─────────┘                   └──────────┬──────────┘
                                                       │
                                            ┌──────────▼──────────┐
                                            │ GR-2: gRPC client   │
                                            │ detection           │
                                            └──────────┬──────────┘
                                                       │
                                            ┌──────────▼──────────┐
                                            │ GR-3: Provider/     │
                                            │ consumer linking    │
                                            └──────────┬──────────┘
                                                       │
                                            ┌──────────▼──────────┐
                                            │ ER-1: eRPC IDL      │
                                            │ (future)            │
                                            └─────────────────────┘
```

## Success Criteria

### Track A success (raw transport):
- Sockets detected across C, C++, Rust, Python, Java, TypeScript
- SharedArrayBuffer/worker boundaries detected in JS/TS
- Shared memory dual projection (boundary + state) working
- Endpoint identity extracted from code and config
- Provider/consumer roles correctly classified

### Track B success (schema-backed RPC):
- `.proto` files parsed with full schema graph
- Generated code mapped back to schema elements
- gRPC servers and clients detected across languages
- Provider/consumer linking produces cross-language edges
- Contract versioning surface enables change-impact analysis

### Unified model success:
- Both tracks produce compatible `BoundaryInteractionSurface` facts
- Contract association works for both raw and schema-backed
- `rmap boundaries` commands work uniformly
- Provider/consumer linking spans mechanism types
- Confidence/provenance fields enable trust stratification

## Non-Goals (Explicit Exclusions)

- **Wire protocol parsing:** We detect API usage, not packet inspection.
- **Dynamic endpoint discovery:** We extract static/configured endpoints only.
- **Runtime tracing integration:** Out of scope for static analysis.
- **Proprietary RPC frameworks:** Focus on open standards first.
- **Schema evolution analysis:** Future slice after linking is stable.

## References

- `docs/design/boundary-interaction-ipc-device.md` (BI-1A design)
- `docs/architecture/state-boundary-contract.txt` (state-boundary model)
- Protocol Buffers Language Guide: https://protobuf.dev/programming-guides/proto3/
- gRPC documentation: https://grpc.io/docs/
