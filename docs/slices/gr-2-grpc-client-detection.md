# GR-2: gRPC Client Detection

Status: PLANNED
Depends: CS-1 (Protobuf Schema), CS-2 (Generated Code Mapping), GR-1 (Server Detection)
Track: B (Schema-Backed RPC)

## Objective

Detect gRPC client usage: stub creation, channel configuration, and RPC
method invocations. This identifies boundary consumers in the gRPC
interaction model.

## Why This Matters

gRPC clients are boundary consumers that:
- Invoke RPC methods on remote services
- Use contracts defined in .proto
- Cross network boundaries (inter-service scope)
- Form the consumer side of provider/consumer relationships

## Scope

### In scope
- Client stub creation patterns
- Channel creation/configuration
- RPC method invocation detection
- Target endpoint extraction
- Streaming call patterns
- C++, Rust, Python, Java, TypeScript coverage

### Out of scope
- Server detection (GR-1, already done)
- Provider/consumer linking (GR-3)
- Connection pooling details
- Retry/timeout configuration

## Detection Patterns

### C++ (grpc++)

```cpp
// Channel creation
std::shared_ptr<grpc::Channel> channel = 
    grpc::CreateChannel("localhost:50051", grpc::InsecureChannelCredentials());

// Stub creation
std::unique_ptr<MyService::Stub> stub = MyService::NewStub(channel);

// RPC invocation
grpc::ClientContext context;
MyRequest request;
MyResponse response;
grpc::Status status = stub->MyMethod(&context, request, &response);
```

**Detection signals:**
- `grpc::CreateChannel()` (endpoint)
- `Service::NewStub()` (stub creation)
- `stub->Method()` (RPC invocation)

### Rust (tonic)

```rust
// Channel creation
let channel = Channel::from_static("http://[::1]:50051")
    .connect()
    .await?;

// Client creation
let mut client = MyServiceClient::new(channel);

// RPC invocation
let request = tonic::Request::new(MyRequest { ... });
let response = client.my_method(request).await?;
```

**Detection signals:**
- `Channel::from_*()` (endpoint)
- `*Client::new()` (client creation)
- `client.method()` (RPC invocation)

### Python (grpcio)

```python
# Channel creation
channel = grpc.insecure_channel('localhost:50051')

# Stub creation
stub = my_service_pb2_grpc.MyServiceStub(channel)

# RPC invocation
request = my_service_pb2.MyRequest()
response = stub.MyMethod(request)
```

**Detection signals:**
- `grpc.insecure_channel()` / `grpc.secure_channel()` (endpoint)
- `*Stub()` (stub creation)
- `stub.Method()` (RPC invocation)

### Java (grpc-java)

```java
// Channel creation
ManagedChannel channel = ManagedChannelBuilder
    .forAddress("localhost", 50051)
    .usePlaintext()
    .build();

// Stub creation (multiple types)
MyServiceGrpc.MyServiceBlockingStub blockingStub = MyServiceGrpc.newBlockingStub(channel);
MyServiceGrpc.MyServiceFutureStub futureStub = MyServiceGrpc.newFutureStub(channel);
MyServiceGrpc.MyServiceStub asyncStub = MyServiceGrpc.newStub(channel);

// RPC invocation
MyResponse response = blockingStub.myMethod(request);
```

**Detection signals:**
- `ManagedChannelBuilder.forAddress()` (endpoint)
- `*Grpc.newBlockingStub()` / `newFutureStub()` / `newStub()` (stub creation)
- `stub.method()` (RPC invocation)

### TypeScript (grpc-js)

```typescript
// Client creation (combines channel + stub)
const client = new MyServiceClient(
  'localhost:50051',
  grpc.credentials.createInsecure()
);

// RPC invocation
client.myMethod(request, (error, response) => {
  // callback
});

// Or with promises
const response = await client.myMethod(request);
```

**Detection signals:**
- `new *Client()` (client + endpoint)
- `client.method()` (RPC invocation)

## Boundary Model Mapping

Each detected client produces a `BoundaryInteractionSurface`:

| Field | Value |
|-------|-------|
| channel_kind | `GrpcChannel` |
| boundary_scope | (from target: inter_process if localhost, inter_device if remote, unknown if dynamic) |
| transport_class | `schema_rpc` |
| direction | `consumer` |
| interaction_pattern | (from call: unary/stream) |
| protocol | `grpc` |
| protocol_family | `rpc` |
| provenance | `extracted` |
| confidence_basis | `syntax_proven` |

Channel identity: `grpc://host:port/package.Service`

**Scope determination:**
- `127.0.0.1`, `localhost`, `::1` → `inter_process`
- Other IP/hostname → `inter_device`
- Dynamic/config-based → `unknown`

## RPC Call Tracking

For each detected stub, track method invocations:

```rust
pub struct GrpcClientCall {
    pub stub_symbol: String,
    pub method_name: String,
    pub source_file: String,
    pub line: u32,
    pub is_streaming: Option<bool>,
}
```

Store as evidence in `boundary_channel_details`.

## Contract Association

Link client to schema:
1. Identify which service stub is being used
2. Look up service in `contract_elements`
3. Create `boundary_contracts` association
4. Link RPC calls to RPC methods

## Endpoint Extraction

Endpoint sources (in priority order):
1. Literal strings in channel creation
2. Constants/variables (follow binding)
3. Environment variables
4. Config file references (mark as `configured`)

When unresolvable: `channel_identity = "<dynamic>"`

## Implementation Steps

1. **Language-specific detectors**
   - C++ grpc++ client patterns
   - Rust tonic client patterns
   - Python grpcio client patterns
   - Java grpc-java client patterns
   - TypeScript grpc-js client patterns

2. **Stub identification**
   - Extract service name from stub type
   - Map to schema service element

3. **RPC call extraction**
   - Identify method calls on stubs
   - Map to schema RPC methods
   - Track streaming patterns

4. **Endpoint extraction**
   - Parse target from channel creation
   - Handle dynamic endpoints

5. **Contract association**
   - Link surfaces to schema elements
   - Store in `boundary_contracts`

## Test Matrix

1. C++ channel creation detection
2. C++ stub creation detection
3. C++ RPC call detection
4. Rust channel creation detection
5. Rust client creation detection
6. Rust RPC call detection
7. Python channel creation detection
8. Python stub creation detection
9. Python RPC call detection
10. Java channel builder detection
11. Java stub creation (all types)
12. Java RPC call detection
13. TypeScript client creation detection
14. TypeScript RPC call detection
15. Endpoint extraction (literal)
16. Endpoint extraction (constant)
17. Streaming call classification
18. Service-to-schema linking
19. Method call-to-RPC linking

## Validation Repos

- gRPC examples repository
- Any repo with gRPC clients
- Multi-language client fixtures

## Deliverables

- Language-specific client detectors (5 languages)
- Stub-to-schema linking logic
- RPC call extraction
- Endpoint extraction
- Storage integration
- 25+ integration tests

## Success Criteria

- gRPC clients detected across all target languages
- Stubs linked to schema services
- RPC calls linked to RPC methods
- Endpoints extracted when possible
- Streaming calls correctly classified
- Correct boundary model facts produced
