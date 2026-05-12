# BI-1B: TCP/UDP Socket Detection

Status: **SHIPPED** (Phase 1: Presence Hints + Phase 2: FD Role Tracking)
Depends: BI-1A (shipped), storage schema migration
Track: A (Raw Transport)

## What Shipped

### Phase 1: Presence Hints (2026-05-11)

Presence hints. The system detects `socket(AF_INET, SOCK_STREAM)` and
`socket(AF_INET, SOCK_DGRAM)` calls and emits surfaces with correct channel_kind
(`tcp_socket` / `udp_socket`) and protocol (`tcp` / `udp`).

### Phase 2: FD Role Tracking (2026-05-12)

Function-local fd tracking to refine TCP/UDP socket surfaces with provider/consumer
role detection. Implements D1-D3 locked design decisions.

**Implementation:**
- C extractor: `assigned_identifier` for socket LHS, `fd_argument` for bind/listen/connect/accept
- `socket_lineage.rs`: FdRegistry, RoleEvidence, TrackedChannelKind
- Emitter: `update_surface_direction()` for direction refinement
- Compose: function-grouped processing with FdRegistry per function

**Validation:**
- 96 C extractor unit tests (11 new BI-1B substrate tests)
- 14 socket_lineage unit tests (includes UDP connect() bidirectional test)
- 45 boundary-interaction-extractor unit tests (3 new direction update tests)
- 7 TCP/UDP E2E integration tests (3 original + 4 refresh tests)

**Implementation Mechanism:**
Phase 2 role detection is implemented via compose-phase lineage suppression, NOT
binding table narrowing. The binding table still contains per-call entries for
bind/listen/connect/accept, but the compose phase:
1. Tracks socket() calls with assigned identifiers in FdRegistry
2. Intercepts bind/listen/connect/accept on tracked fds (accumulates evidence, skips emission)
3. At function boundary, drains FdRegistry and calls `update_surface_direction()`

This is deliberate: the lower-layer binding semantics remain compatible with
non-tracked paths (e.g., Unix sockets), while the compose layer implements the
FD-lineage model for TCP/UDP.

## Shipped Behavior

**Shipped capabilities (Phase 1 + Phase 2):**
- TCP socket detection: `socket(AF_INET/AF_INET6, SOCK_STREAM)` → `tcp_socket`
- UDP socket detection: `socket(AF_INET/AF_INET6, SOCK_DGRAM)` → `udp_socket`
- TCP-only function classification: `listen`, `accept`, `send`, `recv`
- UDP-only function classification: `sendto`, `recvfrom`
- Multi-binding table with guard predicates (prevents TCP-by-precedence)
- InteractionPattern::Datagram for UDP surfaces
- **Phase 2:** FD role tracking with direction refinement:
  - TCP server (bind+listen) → direction = Provider
  - TCP client (connect) → direction = Consumer
  - UDP → direction = Bidirectional (no strong role semantics)

## Phase 2: FD Role Tracking (SHIPPED)

### Locked Design Decisions

**D1: C-only first cut**

Do NOT include C++ in this slice. The actors diverge:
- C socket API: POSIX calls, integer fd identity, direct identifier tracking
- C++ socket usage: wrapper classes, RAII, object ownership, aliases

If C++ later needs similar fd/role tracking, create a separate slice. This slice
is strictly about POSIX socket calls in C code.

**D2: Refine existing surface, do not duplicate**

When role evidence is detected, refine the direction metadata on the existing
surface. Do NOT emit separate "presence hint" and "role hint" surfaces.

- `socket()` alone → `direction = bidirectional` (role unknown)
- `socket()` + later `connect` → same surface, `direction = consumer`
- `socket()` + `bind` + `listen` → same surface, `direction = provider`

One surface per socket lineage. Role is a refinement, not a second boundary.

**D3: bind alone is insufficient evidence**

`bind()` alone does NOT classify as provider. Reasons:
- UDP sockets often bind without "server" semantics
- Clients can bind local addresses/ports
- Raw/native patterns bind for control, privilege, or address selection

Evidence requirements:
| Pattern | Classification |
|---------|----------------|
| `connect(fd, ...)` | consumer |
| `bind(fd, ...) + listen(fd, ...)` | provider (TCP stream) |
| `listen(fd, ...)` on known fd | provider (TCP stream) |
| `accept(fd, ...)` | reinforces provider |
| `bind(fd, ...)` alone | NOT sufficient — stay bidirectional |
| UDP with `bind` only | stay bidirectional (no listen concept) |

### Mechanism: Local FD Registry + Role Evidence State Machine

This is NOT just a lookup table like CPP-SB-1's D3 type map. It is:
1. **Local fd registry**: maps identifier → socket family
2. **Role evidence accumulation**: tracks which operations have been seen on each fd

```
Registry entry:
  identifier: "fd"
  family: tcp_socket | udp_socket
  evidence: { has_bind: bool, has_listen: bool, has_connect: bool, has_accept: bool }
```

State machine per fd:
```
initial: bidirectional (no role evidence)

on connect(fd):
  → consumer

on bind(fd):
  → still bidirectional (insufficient)

on bind(fd) + listen(fd):
  → provider

on listen(fd) [fd already in registry]:
  → provider

on accept(fd):
  → provider (reinforcement)
```

### In Scope (Phase 2)

- C only
- Function-local fd map (cleared at function boundary)
- Direct declarations: `int fd = socket(...)`
- Direct identifier use in: `bind(fd, ...)`, `listen(fd, ...)`, `accept(fd, ...)`, `connect(fd, ...)`
- Refine existing socket surfaces with detected role
- TCP stream: consumer (connect) or provider (bind+listen)
- UDP datagram: remains bidirectional (no strong role heuristics in first cut)

### Out of Scope (Phase 2)

- C++ wrappers/objects
- Cross-function fd propagation
- Aliases/reassignments (`int fd2 = fd`)
- Parameters, globals, member fields
- Endpoint extraction (host:port)
- Loopback/scope classification
- UDP role semantics beyond presence

### Explicit Limits

| Supported | Not Supported |
|-----------|---------------|
| Local variable declarations | Parameters |
| Same function body | Cross-function propagation |
| Direct identifier receiver (`bind(fd, ...)`) | Aliases (`bind(fd2, ...)` where `fd2 = fd`) |
| Simple int declarations | Pointer indirection |
| | Reassignment tracking |
| | Global/static fd variables |

## Deferred (Phase 3+)

- Endpoint extraction: host:port from bind/connect arguments
- Scope classification: inter_process (loopback) vs inter_device (remote)
- Endpoint locality: SameHostNamed vs Remote
- UDP role heuristics (if ever needed)
- C++ socket wrapper support (separate slice)
- Cross-function fd propagation (requires dataflow)

## Objective

Extend boundary interaction detection from local IPC (BI-1A) to network sockets.
TCP and UDP sockets cross process boundaries like Unix sockets, but add
inter-service scope when endpoints are non-loopback.

## Scope

### In scope
- TCP socket creation, bind, listen, accept, connect
- UDP socket creation, bind, sendto, recvfrom
- Endpoint identity extraction (host:port literals, loopback detection)
- Scope classification: inter_process (loopback) vs inter_device (remote)
- Direction classification: provider (bind/listen) vs consumer (connect)
- C, C++, Rust, Python, Java, TypeScript coverage

### Out of scope
- TLS/SSL layer detection (future slice)
- Protocol framing detection (future slice)
- Config-based endpoint extraction (future slice)
- HTTP-over-TCP (already covered by HTTP boundary slice)

## Channel Kinds

New values for `ChannelKind`:
- `TcpSocket`
- `UdpSocket`

## Scope Heuristics

```
endpoint analysis:
  127.0.0.1, localhost, ::1  -> inter_process (same host)
  0.0.0.0, ::, INADDR_ANY    -> unknown (listener accepts any source)
  other IP/hostname          -> inter_device (remote)
  unresolved/dynamic         -> unknown (degrade gracefully)
```

**Note:** `boundary_scope` is the physical boundary. A listener on `0.0.0.0`
*could* receive connections from the same host (inter_process) or remote
hosts (inter_device). Without runtime knowledge, mark as `unknown`.

`transport_class` for raw TCP/UDP is `raw_socket`, regardless of scope.

## Detection Patterns

### C / C++

```c
// TCP server (provider)
socket(AF_INET, SOCK_STREAM, 0);
bind(fd, &addr, sizeof(addr));
listen(fd, backlog);
accept(fd, ...);

// TCP client (consumer)
socket(AF_INET, SOCK_STREAM, 0);
connect(fd, &addr, sizeof(addr));

// UDP
socket(AF_INET, SOCK_DGRAM, 0);
bind(fd, &addr, sizeof(addr));  // provider
sendto(fd, ...);                // either
recvfrom(fd, ...);              // either
```

### Rust

```rust
// std::net
TcpListener::bind("127.0.0.1:8080");
TcpStream::connect("127.0.0.1:8080");
UdpSocket::bind("0.0.0.0:9000");

// tokio::net
TcpListener::bind(&addr).await;
TcpStream::connect(&addr).await;
```

### Python

```python
# TCP server
sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
sock.bind(('0.0.0.0', 8080))
sock.listen()

# asyncio
server = await asyncio.start_server(handler, '0.0.0.0', 8080)
reader, writer = await asyncio.open_connection('127.0.0.1', 8080)
```

### Java

```java
// TCP server
ServerSocket server = new ServerSocket(8080);
Socket client = server.accept();

// TCP client
Socket socket = new Socket("127.0.0.1", 8080);

// NIO
ServerSocketChannel.open().bind(new InetSocketAddress(8080));
SocketChannel.open().connect(new InetSocketAddress("127.0.0.1", 8080));
```

### TypeScript / Node

```typescript
// TCP server
const server = net.createServer();
server.listen(8080, '0.0.0.0');

// TCP client
const client = net.createConnection({ port: 8080, host: '127.0.0.1' });

// UDP
const socket = dgram.createSocket('udp4');
socket.bind(9000);
```

## Binding Table Extensions

Add to `BindingTable`:

| Language | Function | Channel Kind | Direction | Guard |
|----------|----------|--------------|-----------|-------|
| c | socket | tcp_socket | bidirectional | socket_family=AF_INET, socket_type=SOCK_STREAM |
| c | socket | udp_socket | bidirectional | socket_family=AF_INET, socket_type=SOCK_DGRAM |
| c | bind | (inherit) | provider | - |
| c | listen | tcp_socket | provider | - |
| c | accept | tcp_socket | provider | - |
| c | connect | (inherit) | consumer | - |
| rust | TcpListener::bind | tcp_socket | provider | - |
| rust | TcpStream::connect | tcp_socket | consumer | - |
| rust | UdpSocket::bind | udp_socket | bidirectional | - |
| python | socket.socket | (from args) | bidirectional | - |
| java | ServerSocket | tcp_socket | provider | - |
| java | Socket | tcp_socket | consumer | - |
| typescript | net.createServer | tcp_socket | provider | - |
| typescript | net.createConnection | tcp_socket | consumer | - |
| typescript | dgram.createSocket | udp_socket | bidirectional | - |

## Channel Identity

For TCP/UDP, channel identity is `host:port` when extractable:

```
channel_identity = "127.0.0.1:8080"
channel_identity = "0.0.0.0:9000"
channel_identity = "<dynamic>"  // when endpoint is variable
```

## Implementation Steps

1. **Extend binding table** with TCP/UDP entries for all languages
2. **Add scope heuristics** to emitter (loopback detection)
3. **Extract endpoint arguments** from bind/connect/listen calls
4. **Add IPv6 support** (AF_INET6, sockaddr_in6)
5. **Add wrapper library patterns** (tokio, asyncio, Netty)
6. **CLI filter extension** for `--kind tcp_socket`, `--kind udp_socket`

## Test Matrix

1. TCP server detection (C, Rust, Python, Java, TypeScript)
2. TCP client detection (all languages)
3. UDP socket detection (all languages)
4. Loopback scope classification
5. Remote endpoint scope classification
6. Dynamic endpoint degradation
7. IPv6 endpoint handling

## Validation Repos

- nginx (TCP listeners)
- swupdate (if any TCP usage)
- repo-graph daemon (when implemented)

## Deliverables

- Extended `BindingTable` in `repo-graph-boundary-interaction-extractor`
- Scope heuristic logic in emitter
- Endpoint extraction for TCP/UDP
- CLI filter support
- 20+ integration tests
- Validation on nginx

## Success Criteria

- TCP/UDP sockets detected across all target languages
- Scope correctly classified (inter_process vs inter_device vs unknown)
- Direction correctly classified (provider vs consumer)
- Endpoint identity extracted when literal
- Graceful degradation when endpoint is dynamic
- transport_class = raw_socket for all TCP/UDP facts
