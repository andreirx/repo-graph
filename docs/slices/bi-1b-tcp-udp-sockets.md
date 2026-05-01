# BI-1B: TCP/UDP Socket Detection

Status: PLANNED
Depends: BI-1A (shipped), storage schema migration
Track: A (Raw Transport)

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
