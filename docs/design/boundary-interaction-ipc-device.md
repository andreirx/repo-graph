# Boundary Interaction Support Module: IPC and Inter-Device Communication

Status: LOCKED (2026-04-30)
Created: 2026-04-30
Maturity target: PROTOTYPE -> MATURE

## Design Review Status

**v1 draft:** 2026-04-30
**Corrections applied:** 2026-04-30 (all)
**Status:** LOCKED

Corrections applied from design review:
1. [x] Primary output is local observations, not paired links
2. [x] Reserve `in_process` scope; emit only `inter_process`, `inter_device`, `unknown` in v1
3. [x] Shared memory requires dual projection (boundary-interaction AND state-boundary) - see 4.4
4. [x] Slice 1 split into 1A (local IPC) and 1B (generic sockets) - see section 7
5. [x] Mechanism naming is protocol-focused, not `ipc_`-prefixed - see Q1
6. [x] `endpoint_locality` replaces redundant `locality` field
7. [x] `shared_state` added to interaction_pattern enum

**First slice decision (2026-04-30):** Slice 1A (Local IPC) — architecture confidence over embedded product value

## 1. Problem Statement

repo-graph currently models two classes of boundary interactions:

1. **API boundaries** (HTTP, CLI) — request/response interactions between
   services or between user and service. Provider/consumer fact pairs with
   mechanism-specific matching.

2. **State boundaries** (FS, DB, cache, blob) — READS/WRITES edges to
   resource nodes representing persistent or cached data.

Neither model captures **inter-process communication (IPC)** or
**inter-device communication** relationships. These are not ordinary call
edges, not API boundaries, and not state/resource touchpoints. They are a
distinct class of runtime interaction that crosses:

- Process boundaries
- Device/machine boundaries
- Serialization/deserialization boundaries
- Ownership/failure boundaries
- Protocol/compatibility boundaries

### Why this matters for legacy-code change

The core business logic center of repo-graph is legacy-code relationship
modeling. IPC and device communication boundaries are among the most
critical relationships for understanding how legacy systems can be
changed safely:

- Changing a struct used on a socket boundary
- Changing a CAN message ID
- Changing a pipe payload format
- Changing a shared memory layout
- Changing a protobuf/IDL schema
- Changing a serial framing protocol

These are exactly the kinds of hidden dependencies that burn teams. They
are invisible to import/call analysis and are rarely documented.

### Gap in current model

The existing `BoundaryMechanism` type includes placeholders:

```typescript
| "ipc_shared_memory"
| "ioctl"
| "socket"
| "device_protocol"
| "register_map"
```

But no extractors, matchers, or storage support exists for these mechanisms.
The model also conflates interaction boundaries (request/response, command/exit)
with state boundaries (read/write to persisted data). IPC and device
communication need explicit architectural treatment.

## 2. Scope

### In scope

- IPC (inter-process communication) boundary interactions:
  - Unix domain sockets
  - Named pipes / FIFOs
  - Anonymous pipes
  - Shared memory
  - Message queues (POSIX, SysV)
  - Signals
  - Local RPC (D-Bus, COM, Binder)
  - Loopback TCP/UDP (when used as internal process seam)

- Inter-device communication boundary interactions:
  - TCP/UDP to remote hosts
  - Serial/UART
  - CAN bus
  - I2C, SPI
  - USB protocol layers
  - BLE
  - Modbus
  - MQTT
  - Custom binary framing protocols

### Out of scope

- HTTP API boundaries (already modeled)
- CLI command boundaries (already modeled)
- State/resource boundaries (FS, DB, cache, blob — already modeled)
- gRPC (deferred to API boundary expansion)
- Queue/event boundaries (Kafka, SQS, RabbitMQ — separate slice)
- Enforcement/policy layer (discovery first)

### Explicit non-goals

- Do not collapse IPC/device boundaries into state/resource boundaries
- Do not treat sockets/pipes as "file handles" in the state model
- Do not implement all protocols at once
- Do not require compiler/semantic analysis for first extraction slice

## 3. Architectural Position

### Relationship to existing models

```
                              +-----------------------+
                              |   BOUNDARY            |
                              |   INTERACTIONS        |
                              +-----------------------+
                              /           |           \
                             /            |            \
          +------------------+   +------------------+   +------------------+
          |   API            |   |   IPC / DEVICE   |   |   QUEUE / EVENT  |
          |   BOUNDARIES     |   |   BOUNDARIES     |   |   BOUNDARIES     |
          +------------------+   +------------------+   +------------------+
          | http             |   | ipc_socket       |   | kafka_topic      |
          | grpc             |   | ipc_pipe         |   | sqs_queue        |
          | cli_command      |   | ipc_shm          |   | rabbitmq_exchange|
          |                  |   | tcp_endpoint     |   | pubsub_topic     |
          |                  |   | serial           |   |                  |
          |                  |   | can_bus          |   |                  |
          |                  |   | i2c, spi         |   |                  |
          +------------------+   +------------------+   +------------------+

          +------------------+
          |   STATE          |
          |   BOUNDARIES     |
          +------------------+
          | filesystem       |
          | database_sql     |
          | database_nosql   |
          | cache            |
          | object_store     |
          +------------------+
```

IPC/device boundaries are sibling to API boundaries and queue/event boundaries.
They are NOT a subtype of state boundaries.

Key distinctions:

| Dimension | API Boundary | IPC/Device Boundary | State Boundary |
|-----------|--------------|---------------------|----------------|
| Primary relationship | request -> response | message -> ack/effect | read -> data / write -> mutation |
| Failure model | HTTP errors, retries | connection loss, timeouts, framing errors | file errors, DB errors |
| Identity | operation + path | channel + protocol + endpoint | resource key |
| Schema/contract | OpenAPI, protobuf | IDL, binary format, register map | table schema, file format |
| Ownership boundary | service | process / device | none (shared state) |

## 4. Domain Model

### 4.1 Boundary Scope

The model needs an explicit **boundary scope** dimension:

```
boundary_scope (v1 emitted values):
  - inter_process  # Cross-process on same host (IPC)
  - inter_device   # Cross-device/host (network, bus, serial)
  - unknown        # Scope cannot be determined statically

boundary_scope (reserved, not emitted in v1):
  - in_process     # Cross-module within same process
```

**Why `in_process` is reserved:**
- This track is specifically IPC and inter-device
- `in_process` overlaps with call graph, state boundaries, ordinary module relationships
- Emitting it would muddy the semantics immediately
- Keep it in the abstract model for future expansion, but do not emit in v1

This is a first-class attribute, not inferred from mechanism type. The same
mechanism (e.g., TCP socket) can be used for inter-process (loopback) or
inter-device (remote host) communication. The scope determines the failure
model and ownership boundary.

### 4.2 Two-Level Model (Recommended)

**Level 1: Boundary Interaction Surface**

High-level architectural relationship between communicating parties.

```typescript
interface BoundaryInteractionSurface {
  // Identity
  surfaceUid: string;
  snapshotUid: string;
  repoUid: string;

  // Classification
  boundaryScope: "in_process" | "inter_process" | "inter_device";
  mechanism: BoundaryMechanism;
  direction: "provider" | "consumer" | "bidirectional";
  
  // Protocol
  protocol: string;           // e.g., "tcp", "udp", "can", "uart", "shm", "pipe"
  protocolFamily: string;     // e.g., "socket", "serial", "bus", "ipc"
  interactionPattern: "request_response" | "publish_subscribe" | "stream" | "fire_and_forget" | "shared_state";
  
  // Endpoint locality (observable from this callsite)
  endpointLocality: "loopback" | "same_host_named" | "remote_literal" | "unknown";
  
  // Source provenance
  symbolStableKey: string;
  sourceFile: string;
  lineStart: number;
  lineEnd: number;
  
  // Extraction provenance
  extractor: string;
  basis: InteractionBasis;
  confidence: number;
  evidenceJson: string;       // Structured per-mechanism evidence
}

type InteractionBasis =
  | "api_call"         // socket(), bind(), connect(), listen(), send(), recv()
  | "wrapper_call"     // Library wrapper over raw API (ZeroMQ, nanomsg)
  | "annotation"       // Attribute/decorator declaring the boundary
  | "convention"       // Naming pattern (e.g., *_handler, *_callback)
  | "declaration"      // User-declared via `rmap declare`
  | "inferred";        // Heuristic-derived
```

**Level 2: Channel/Resource Detail**

Low-level mechanism-specific facts about the communication channel.

```typescript
interface ChannelDetail {
  // FK to surface
  surfaceUid: string;
  
  // Channel identity (mechanism-specific)
  channelKind: ChannelKind;
  channelIdentity: string;    // Normalized key for matching
  
  // Addressing (one of these populated based on channelKind)
  socketPath?: string;        // Unix socket path
  tcpEndpoint?: string;       // host:port or *:port
  udpEndpoint?: string;       // host:port or *:port
  canId?: number;             // CAN message ID
  i2cAddress?: number;        // I2C device address
  spiDevice?: string;         // SPI device path
  serialDevice?: string;      // /dev/ttyUSB0, COM3
  shmKey?: string;            // Shared memory key/name
  pipeIdentity?: string;      // Named pipe path or descriptor context
  mqueueName?: string;        // POSIX message queue name
  
  // Protocol details
  baudRate?: number;          // Serial/CAN
  canExtended?: boolean;      // CAN extended ID flag
  frameFormat?: string;       // Binary framing description
  payloadContract?: string;   // IDL/schema reference if known
  
  // Extracted metadata
  metadataJson: string;
}

type ChannelKind =
  | "unix_socket"
  | "tcp_socket"
  | "udp_socket"
  | "named_pipe"
  | "anonymous_pipe"
  | "shared_memory"
  | "message_queue"
  | "signal"
  | "can_message"
  | "serial_port"
  | "i2c_device"
  | "spi_device"
  | "usb_endpoint"
  | "ble_characteristic"
  | "mqtt_topic"
  | "modbus_register"
  | "custom_protocol";
```

### 4.3 Why Two Levels

**Architecture-level queries:**
- "Which modules cross a process boundary?"
- "Which symbols talk to external devices?"
- "What boundaries are undocumented?"

These operate on `BoundaryInteractionSurface`.

**Protocol-level queries:**
- "Which functions use CAN ID 0x123?"
- "Which symbols write to shared memory region 'engine_state'?"
- "Which serial ports are used by this module?"

These operate on `ChannelDetail`.

**Change impact analysis:**
- "If I change the CAN message format for ID 0x456, what is affected?"
- "If I move this service to a different host, which boundaries break?"

These require joining both levels.

### 4.4 Dual-Projection Rules

Some mechanisms have characteristics of both boundary-interaction and
state-boundary. They require dual projection to capture both aspects.

**Shared memory (POSIX shm, mmap MAP_SHARED):**

Shared memory is simultaneously:
- A **boundary interaction** (crosses process ownership boundary)
- A **state resource** (persistent/cached data with READS/WRITES semantics)

**Extraction rule:** Emit to BOTH models:
1. `boundary_interaction_surfaces` with `interaction_pattern: "shared_state"`
2. `state_boundary_edges` with direction `reads` or `writes`

The boundary-interaction fact captures the ownership/failure boundary crossing.
The state-boundary fact captures the data touchpoint semantics (what is read,
what is written, potential races).

**Why dual projection matters:**

Change impact differs by question type:
- "What processes share this memory region?" -> boundary-interaction query
- "What symbols write to this memory region?" -> state-boundary query
- "If I change the struct layout, what breaks?" -> needs both

Do not single-home shared memory. Both perspectives are useful.

**Memory-mapped files:**

Similar dual projection applies to `mmap()` on regular files with `MAP_SHARED`.
The file path becomes a state-boundary resource AND the mapped region becomes
a boundary-interaction surface if multiple processes access it.

## 5. Trade-Off Analysis

### Option A: Provider/Consumer Interaction Model (Extend Existing)

Extend the existing `BoundaryProviderFact`/`BoundaryConsumerFact` model
with new mechanism types for IPC and device communication.

**Pros:**
- Consistent with existing HTTP/CLI boundary model
- Reuses existing storage schema (migration 008)
- Reuses existing `BoundaryMatchStrategy` pattern
- Proven model for request/response patterns

**Cons:**
- Provider/consumer role is not always clear for IPC
  (e.g., shared memory has readers and writers, not providers and consumers)
- Loses the scope distinction (inter-process vs inter-device)
- Loses the channel/resource detail layer
- Forces all IPC mechanisms into request/response framing

### Option B: Resource/Channel Model (State-Boundary Extension)

Treat IPC channels as resources (like FS paths or DB connections) and
model READS/WRITES edges to them.

**Pros:**
- Conceptually simple: sockets and pipes are "resources"
- Reuses state-boundary extraction pattern
- Unifies with file/state/resource touchpoints

**Cons:**
- Semantic mismatch: a socket is not a resource to be read/written,
  it is a communication channel with a remote party
- Loses interaction semantics (request/response, pub/sub, stream)
- Loses protocol/contract information
- Loses the failure boundary distinction
- Conflates very different architectural relationships

### Option C: Two-Level Model (Recommended)

Separate boundary interaction surface (Level 1) from channel/resource
detail (Level 2). The surface captures the architectural relationship;
the detail captures the mechanism-specific facts.

**Pros:**
- Preserves both architecture-level relationship and low-level mechanism
- Good for legacy-change reasoning
- Good for transport-specific later enrichment
- Allows different query granularities
- Explicit scope (inter-process vs inter-device)
- Does not force IPC into provider/consumer framing

**Cons:**
- More design work up front
- Two tables instead of one
- Requires join for full picture

### Recommendation

**Use Option C (Two-Level Model).**

The two-level model matches the product center better than either a pure
interaction model or a pure resource model. IPC and device communication
are architecturally significant relationships that deserve explicit
treatment, not shoehorning into existing HTTP or state-boundary patterns.

## 6. Storage Schema

### New Tables

```sql
-- Level 1: Boundary interaction surfaces
CREATE TABLE boundary_interaction_surfaces (
  surface_uid         TEXT PRIMARY KEY,
  snapshot_uid        TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
  repo_uid            TEXT NOT NULL REFERENCES repos(repo_uid),
  
  -- Classification
  boundary_scope      TEXT NOT NULL,    -- in_process | inter_process | inter_device
  mechanism           TEXT NOT NULL,    -- ipc_socket | serial | can_bus | ...
  direction           TEXT NOT NULL,    -- provider | consumer | bidirectional
  
  -- Protocol
  protocol            TEXT NOT NULL,    -- tcp | udp | can | uart | shm | pipe | ...
  protocol_family     TEXT NOT NULL,    -- socket | serial | bus | ipc | ...
  interaction_pattern TEXT NOT NULL,    -- request_response | pub_sub | stream | fire_and_forget | shared_state
  
  -- Endpoint locality (observable from this callsite)
  endpoint_locality   TEXT NOT NULL,    -- loopback | same_host_named | remote_literal | unknown
  
  -- Source provenance
  symbol_stable_key   TEXT NOT NULL,
  source_file         TEXT NOT NULL,
  line_start          INTEGER NOT NULL,
  line_end            INTEGER NOT NULL,
  
  -- Extraction provenance
  extractor           TEXT NOT NULL,
  basis               TEXT NOT NULL,    -- api_call | wrapper_call | annotation | ...
  confidence          REAL NOT NULL,
  evidence_json       TEXT NOT NULL
);

CREATE INDEX idx_bis_snapshot_scope ON boundary_interaction_surfaces(snapshot_uid, boundary_scope);
CREATE INDEX idx_bis_snapshot_mechanism ON boundary_interaction_surfaces(snapshot_uid, mechanism);
CREATE INDEX idx_bis_snapshot_symbol ON boundary_interaction_surfaces(snapshot_uid, symbol_stable_key);
CREATE INDEX idx_bis_snapshot_protocol ON boundary_interaction_surfaces(snapshot_uid, protocol_family, protocol);

-- Level 2: Channel details
CREATE TABLE boundary_channel_details (
  channel_uid         TEXT PRIMARY KEY,
  surface_uid         TEXT NOT NULL REFERENCES boundary_interaction_surfaces(surface_uid) ON DELETE CASCADE,
  
  -- Channel identity
  channel_kind        TEXT NOT NULL,    -- unix_socket | tcp_socket | can_message | ...
  channel_identity    TEXT NOT NULL,    -- Normalized key for matching
  
  -- Addressing (nullable, mechanism-specific)
  socket_path         TEXT,
  tcp_endpoint        TEXT,
  udp_endpoint        TEXT,
  can_id              INTEGER,
  i2c_address         INTEGER,
  spi_device          TEXT,
  serial_device       TEXT,
  shm_key             TEXT,
  pipe_identity       TEXT,
  mqueue_name         TEXT,
  
  -- Protocol details
  baud_rate           INTEGER,
  can_extended        INTEGER,          -- boolean as 0/1
  frame_format        TEXT,
  payload_contract    TEXT,
  
  -- Extracted metadata
  metadata_json       TEXT
);

CREATE INDEX idx_bcd_surface ON boundary_channel_details(surface_uid);
CREATE INDEX idx_bcd_channel_kind ON boundary_channel_details(channel_kind);
CREATE INDEX idx_bcd_channel_identity ON boundary_channel_details(channel_identity);
```

### Relationship to Existing Tables

These tables are siblings to `boundary_provider_facts` and `boundary_consumer_facts`,
not replacements. The existing tables remain for HTTP/CLI/gRPC boundaries.

Future consideration: unify the two models under a common base abstraction
if the overlap proves greater than expected. For now, explicit separation
prevents premature generalization.

## 7. Extraction Strategy

### Slice Prioritization

Do not attempt all protocols at once. Build in slices with clear validation.

#### Slice 1A: Local IPC Mechanisms (C/C++)

**Rationale:** Pure local IPC mechanisms have unambiguous `inter_process` scope.
No heuristics needed for scope classification. Cleaner proof of the model.

**Target APIs:**
- Unix domain sockets: `socket(AF_UNIX)`, `bind()`, `connect()` with `sockaddr_un`
- Named pipes: `mkfifo()`, `open()` on FIFOs
- Anonymous pipes: `pipe()`, `pipe2()`
- Shared memory: `shm_open()`, `mmap()` with `MAP_SHARED`
- POSIX message queues: `mq_open()`, `mq_send()`, `mq_receive()`

**Detection pattern:** API call with known function name from binding table,
plus argument inspection for AF_UNIX or path-based addressing.

**Scope classification:**
- All mechanisms in this slice -> `inter_process` (by definition)
- `endpoint_locality`: `same_host_named` (Unix socket path, shm key, mqueue name)

**Dual projection (shared memory only):**
- Emit to `boundary_interaction_surfaces` with `interaction_pattern: "shared_state"`
- Emit to `state_boundary_edges` with direction `reads` or `writes`

**Validation repos:**
- swupdate (uses Unix sockets for IPC in update/ipc_interface.c)
- sqlite (uses pipes and shared memory for WAL)
- nginx (Unix socket listener support)

**Acceptance criteria:**
- Precision: < 10% false positives
- All Unix socket bind/connect sites detected
- All shm_open sites detected with dual projection

#### Slice 1B: Generic Socket Transport (C/C++)

**Rationale:** TCP/UDP sockets require scope heuristics (loopback vs remote).
Separated from 1A to isolate heuristic complexity.

**Target APIs:**
- `socket(AF_INET)`, `socket(AF_INET6)`
- `bind()`, `connect()`, `listen()`, `accept()` with `sockaddr_in`/`sockaddr_in6`
- `send()`, `recv()`, `sendto()`, `recvfrom()`
- `getaddrinfo()`, `gethostbyname()` (for endpoint resolution)

**Detection pattern:** API call with known function name from binding table.

**Scope classification (heuristic):**
- Literal `127.0.0.1`, `::1`, `localhost` -> `inter_process`, `endpoint_locality: loopback`
- Literal non-loopback IP -> `inter_device`, `endpoint_locality: remote_literal`
- Variable/config-sourced address -> `unknown`, `endpoint_locality: unknown`
- `INADDR_ANY` / `*` (bind) -> scope depends on intended use, default `unknown`

**Validation repos:**
- swupdate (TCP socket for web interface)
- nginx (extensive TCP/UDP socket usage)
- curl (TCP client patterns)

**Acceptance criteria:**
- Precision: < 15% false positives (higher than 1A due to heuristics)
- Loopback addresses correctly classified as `inter_process`
- Remote literals correctly classified as `inter_device`
- Unknown cases explicitly marked, not guessed

#### Slice 2: Serial/CAN APIs (C/C++)

**Target APIs:**
- `open("/dev/tty*")`, `read()`, `write()` on serial
- Linux CAN: `socket(AF_CAN)`, `struct can_frame`
- termios: `tcsetattr()`, `tcgetattr()`

**Detection pattern:**
- String argument to `open()` matching `/dev/tty*` or `/dev/serial*`
- Socket creation with `AF_CAN` family
- Use of `struct can_frame` or `struct canfd_frame`

**Scope classification:**
- Serial device -> inter_device
- CAN bus -> inter_device

**Validation repos:**
- swupdate (has CAN support in handlers/)
- Buildroot packages with serial/CAN usage
- Automotive codebases (if accessible)

#### Slice 3: MQTT/Local RPC (Language-Specific)

**Target APIs:**
- Paho MQTT client (C/C++, Python, Java)
- Eclipse Mosquitto client
- D-Bus (C, Python)
- ZeroMQ (multi-language)

**Detection pattern:**
- Library-specific API calls
- Import of known MQTT/ZeroMQ/D-Bus modules

**Scope classification:**
- MQTT with localhost broker -> inter_process
- MQTT with remote broker -> inter_device
- D-Bus -> inter_process
- ZeroMQ with ipc:// -> inter_process
- ZeroMQ with tcp:// -> depends on host

**Validation repos:**
- swupdate (has MQTT support)
- Home automation projects (MQTT-heavy)
- Linux desktop services (D-Bus-heavy)

#### Slice 4: I2C/SPI/USB (C/C++)

**Target APIs:**
- Linux I2C: `/dev/i2c-*`, `ioctl(I2C_RDWR)`
- Linux SPI: `/dev/spi*`, `ioctl(SPI_IOC_*)`
- libusb: `libusb_*` functions

**Detection pattern:**
- Device path matching
- ioctl command constants
- Library function names

**Scope classification:**
- All -> inter_device

**Validation repos:**
- Embedded Linux projects
- Device driver codebases

### Extraction Architecture

```
rust/crates/boundary-interaction/
  src/
    lib.rs
    types.rs              # BoundaryInteractionSurface, ChannelDetail
    extractors/
      mod.rs
      posix_socket.rs     # socket/bind/connect/listen/send/recv
      posix_pipe.rs       # pipe/mkfifo
      posix_shm.rs        # shm_open/mmap
      posix_mqueue.rs     # mq_open/mq_send/mq_receive
      serial.rs           # termios, /dev/tty*
      can.rs              # AF_CAN, struct can_frame
      mqtt.rs             # Paho, Mosquitto
      zeromq.rs           # ZeroMQ
      dbus.rs             # D-Bus
      i2c.rs              # Linux I2C
      spi.rs              # Linux SPI
      usb.rs              # libusb
    matchers/
      mod.rs
      c.rs                # C-specific pattern matchers
      cpp.rs              # C++ matchers
      python.rs           # Python matchers (future)
      rust.rs             # Rust matchers (future)
    binding_table.rs      # API binding table (similar to state-bindings)
```

### Binding Table Format

Similar to state-boundary `bindings.toml`, but with boundary-interaction semantics:

```toml
[[binding]]
language = "c"
api_family = "posix_socket"
function = "socket"
direction = "setup"
boundary_scope_heuristic = "arg_family"   # Derive scope from AF_* argument
protocol_family = "socket"
notes = "Socket creation"

[[binding]]
language = "c"
api_family = "posix_socket"
function = "connect"
direction = "consumer"
boundary_scope_heuristic = "sockaddr"     # Derive scope from address struct
protocol_family = "socket"

[[binding]]
language = "c"
api_family = "posix_socket"
function = "bind"
direction = "provider"
boundary_scope_heuristic = "sockaddr"
protocol_family = "socket"

[[binding]]
language = "c"
api_family = "can"
function = "socket"
direction = "setup"
boundary_scope = "inter_device"           # Always inter-device for CAN
protocol_family = "bus"
protocol = "can"

[[binding]]
language = "c"
api_family = "serial"
function = "open"
direction = "bidirectional"
path_pattern = "/dev/tty.*|/dev/serial.*"
boundary_scope = "inter_device"
protocol_family = "serial"
protocol = "uart"
```

## 8. Matching Strategy

### Provider/Consumer Pairing

Unlike HTTP boundaries, IPC/device boundaries don't always have clear
provider/consumer roles. The matching strategy depends on mechanism:

**Socket-based:**
- `bind()` + `listen()` -> provider
- `connect()` -> consumer
- Match by endpoint (host:port or socket path)

**Pipe/FIFO:**
- `mkfifo()` creator -> provider (of the channel)
- `open()` for read -> consumer
- `open()` for write -> producer
- Match by pipe path

**Shared memory:**
- `shm_open()` creator -> provider
- `mmap()` reader -> consumer
- `mmap()` writer -> producer
- Match by shm key/name

**CAN bus:**
- All participants are peers (bus topology)
- Match by CAN ID (message identifier)
- Direction determined by `read()` vs `write()` on socket

**Serial:**
- Usually point-to-point
- Match by device path
- Both sides are bidirectional

### Match Strategy Interface

```typescript
interface BoundaryInteractionMatchStrategy {
  readonly protocolFamily: string;
  
  computeChannelKey(detail: ChannelDetail): string;
  
  match(
    providers: BoundaryInteractionSurface[],
    consumers: BoundaryInteractionSurface[],
  ): BoundaryInteractionLink[];
}
```

### Cross-Repo Matching (Future)

IPC boundaries are inherently intra-repo (inter-process on same host).
Device boundaries can span repos (e.g., firmware and host-side driver).

Cross-repo matching requires:
- Fleet-level supergraph (Horizon 3)
- Protocol/contract versioning
- Endpoint identity normalization

Deferred to fleet-level slice.

## 9. CLI Surface

### New Commands

```bash
# List all boundary interaction surfaces
rmap boundaries list <db> <repo> [--scope inter_process|inter_device] [--protocol tcp|can|...]

# Show detail for a specific surface
rmap boundaries show <db> <repo> <surface_uid>

# List channel details
rmap boundaries channels <db> <repo> [--kind unix_socket|can_message|...]

# Find surfaces by symbol
rmap boundaries for <db> <repo> <symbol>

# Find surfaces by channel identity
rmap boundaries by-channel <db> <repo> <channel_identity>

# Summary statistics
rmap boundaries summary <db> <repo>
```

### JSON Output Contract

```json
{
  "surfaces": [
    {
      "surface_uid": "...",
      "boundary_scope": "inter_process",
      "mechanism": "ipc_socket",
      "direction": "provider",
      "protocol": "tcp",
      "protocol_family": "socket",
      "interaction_pattern": "request_response",
      "endpoint_locality": "loopback",
      "symbol_stable_key": "swupdate:core/network_ipc.c#start_listener:SYMBOL:function",
      "source_file": "core/network_ipc.c",
      "line_start": 142,
      "line_end": 156,
      "extractor": "posix-socket:0.1.0",
      "basis": "api_call",
      "confidence": 0.9,
      "channels": [
        {
          "channel_kind": "tcp_socket",
          "channel_identity": "*:5050",
          "tcp_endpoint": "*:5050"
        }
      ]
    }
  ]
}
```

## 10. Integration with Existing Surfaces

### orient

```json
{
  "signals": [
    {
      "code": "IPC_BOUNDARIES",
      "severity": "info",
      "evidence": {
        "inter_process_count": 5,
        "inter_device_count": 2,
        "protocols": ["tcp", "unix_socket", "can"]
      }
    }
  ]
}
```

### check

```json
{
  "conditions": [
    {
      "code": "BOUNDARY_UNDOCUMENTED",
      "status": "warn",
      "evidence": {
        "undocumented_boundaries": 3,
        "total_boundaries": 7
      }
    }
  ]
}
```

### explain

```json
{
  "symbol": "...",
  "boundary_interactions": [
    {
      "role": "provider",
      "scope": "inter_process",
      "protocol": "tcp",
      "channel": "*:5050"
    }
  ]
}
```

## 11. Validation Plan

### Per-Slice Validation Repos

| Slice | Validation Repo | Expected Findings |
|-------|-----------------|-------------------|
| 1A: Local IPC | swupdate | Unix socket IPC in update/ipc_interface.c |
| 1A: Local IPC | sqlite | WAL shared memory, pipe usage |
| 1A: Local IPC | nginx | Unix socket listener support |
| 1B: Generic Sockets | swupdate | TCP socket for web interface |
| 1B: Generic Sockets | nginx | Extensive TCP/UDP socket usage |
| 1B: Generic Sockets | curl | TCP client patterns |
| 2: Serial/CAN | swupdate | CAN handler support |
| 2: Serial/CAN | Linux kernel (drivers/) | Serial/CAN drivers |
| 3: MQTT/RPC | swupdate | MQTT handler |
| 3: MQTT/RPC | D-Bus services | D-Bus IPC |
| 4: I2C/SPI/USB | Embedded Linux | Device communication |

### Acceptance Criteria Per Slice

- Precision: < 10% false positives on validation repo
- Recall: > 80% of known boundaries detected
- Deterministic: same input produces same output
- Provenance: every fact carries line number and basis

## 12. Assumptions

1. **C/C++ is the proving ground.** Most IPC and device communication
   code is in C/C++. Language expansion follows after the model proves.

2. **API-pattern extraction is sufficient for first slice.** We do not
   need compiler/semantic analysis to detect `socket()` calls. Binding
   tables work.

3. **Scope classification can be heuristic.** Distinguishing inter_process
   from inter_device based on address arguments is imperfect but useful.
   Unknown is acceptable.

4. **Cross-repo matching is out of scope.** IPC is intra-host by definition.
   Device boundaries that span repos require fleet-level work.

5. **The existing boundary matcher pattern generalizes.** The
   `BoundaryMatchStrategy` interface can be extended for new mechanisms.

## 13. Non-Goals

1. **Full protocol parsing.** We extract that communication exists, not
   what the messages contain (unless schema/IDL is declared).

2. **Runtime verification.** We do not verify that declared endpoints
   actually match at runtime.

3. **Dynamic analysis.** All extraction is static/syntactic.

4. **Security analysis.** We surface boundaries for understanding change
   impact, not for security audit (though findings may inform security).

## 14. Technical Risks

### Risk 1: Scope misclassification

Heuristics for distinguishing loopback from remote TCP are imperfect.
`connect("127.0.0.1:8080")` is inter_process, but
`connect(getenv("API_HOST"):8080)` is unknown at static analysis time.

**Mitigation:** Accept "unknown" as a valid scope value. Provide
declaration mechanism for user to clarify.

### Risk 2: False positives from utility code

Generic socket wrappers, logging utilities, and library code may produce
spurious boundary facts that obscure real boundaries.

**Mitigation:** Confidence scoring. Test-file exclusion. Vendored-code
exclusion.

### Risk 3: CAN/I2C/SPI extraction requires embedded toolchain context

Bare-metal embedded code may use vendor-specific HALs that don't match
standard Linux APIs.

**Mitigation:** Start with Linux user-space APIs. HAL-specific extractors
are future slices, informed by real embedded repo validation.

### Risk 4: Large binding tables

Unlike state-boundary FS APIs (22 entries), IPC/device APIs span many
libraries and protocols.

**Mitigation:** Slice by protocol family. Each slice adds a small,
validated binding table increment.

## 15. Phased Roadmap

### Phase 1: Model and First Slice

1. **Design doc** (this document)
2. **Types and storage migration**
3. **Slice 1A: Local IPC extraction for C** (Unix sockets, pipes, shm, mqueue)
4. **Validation on swupdate, sqlite**
5. **Slice 1B: Generic socket extraction for C** (TCP/UDP with scope heuristics)
6. **Validation on nginx, curl**

### Phase 2: Device Communication

1. **Slice 2: Serial/CAN extraction for C**
2. **Validation on embedded repos**

### Phase 3: Library Wrappers

1. **Slice 3: MQTT/ZeroMQ/D-Bus extraction**
2. **Validation on IoT/desktop repos**

### Phase 4: CLI Surface and Integration

1. **`rmap boundaries` command family**
2. **Integration with orient/check/explain**
3. **Documentation**

### Phase 5: Language Expansion

1. **Python socket/serial extraction**
2. **Rust async/tokio networking**
3. **Java NIO/Netty**

## 16. Open Questions

### Q1: Should `ipc_` prefix be used for all inter-process mechanisms?

**Answer:** No. Use protocol-focused names.

Mechanism names should reflect the protocol/transport (`unix_socket`,
`tcp_socket`, `serial`, `can_message`), not the scope.

Scope is a separate first-class dimension (`boundary_scope`), not encoded
in mechanism name. The same TCP socket can be `inter_process` (loopback)
or `inter_device` (remote host).

### Q2: Should shared memory be in this model or state-boundary?

Shared memory has characteristics of both:
- Boundary interaction: crosses process boundary
- State resource: persistent/cached data

**Answer:** Both. Dual projection required. See section 4.4.

Emit to `boundary_interaction_surfaces` with `interaction_pattern: "shared_state"`.
Also emit to `state_boundary_edges` for READS/WRITES tracking.

The boundary-interaction fact captures ownership boundary crossing.
The state-boundary fact captures data touchpoint semantics.

### Q3: How to handle bidirectional channels?

Sockets, serial ports, and shared memory are inherently bidirectional.
The current model uses `direction: "bidirectional"` but this loses
information about which side initiates.

**Tentative answer:** Accept bidirectional as a valid direction.
For matching, pair bidirectional with bidirectional on matching channels.
Direction is about the symbol's role, not the channel's capability.

## 17. Definition of Done (This Design Slice)

- [x] One stable conceptual model (two-level: surface + channel detail)
- [x] One explicit first implementation slice proposal (POSIX socket/pipe/shm)
- [x] One phased roadmap from syntax/API evidence to richer contract association
- [x] No hidden architecture decisions left in code
- [x] Trade-off analysis with clear recommendation
- [x] Storage schema draft
- [x] Validation repo recommendations
- [x] CLI surface sketch
- [x] Integration points with existing surfaces
- [x] Explicit assumptions, non-goals, and risks

## 18. Next Step

Design is locked. First slice is binding.

### First Slice: 1A — Local IPC (LOCKED)

**Decision rationale:** Architecture confidence over embedded product value.
Local IPC mechanisms have unambiguous `inter_process` scope. No heuristics.
Cleaner proof of the two-level model before expanding to device boundaries.

**Scope:**
- Unix domain socket detection (AF_UNIX, sockaddr_un)
- pipe/mkfifo detection
- shm_open/mmap detection with dual projection
- mq_open/mq_send/mq_receive detection

**Validation repos:**
- swupdate (Unix socket IPC)
- sqlite (shm, pipes for WAL)

**Acceptance criteria:**
- All Unix socket bind/connect sites detected
- All shm_open sites detected with dual projection to state-boundary
- Precision < 10% false positives
- Deterministic output

### Subsequent Slices (not yet scheduled)

| Order | Slice | Rationale |
|-------|-------|-----------|
| 2 | 1B: Generic sockets | Adds TCP/UDP with scope heuristics |
| 3 | 2: Serial/CAN | Device boundaries, embedded value |
| 4 | 3: MQTT/RPC | Library wrappers |
| 5 | 4: I2C/SPI/USB | Low-level device protocols |

Do not mix slices. Complete 1A validation before starting 1B.
