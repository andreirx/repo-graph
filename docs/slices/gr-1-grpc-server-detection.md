# GR-1: gRPC Server Detection

Status: PLANNED
Depends: CS-1 (Protobuf Schema), CS-2 (Generated Code Mapping)
Track: B (Schema-Backed RPC)

## Objective

Detect gRPC server implementations: service registration, handler methods,
and transport configuration. This identifies boundary providers in the
gRPC interaction model.

## Why This Matters

gRPC servers are boundary providers that:
- Expose RPC methods to clients
- Implement contracts defined in .proto
- Bridge network boundaries (inter-service scope)
- Often represent critical architectural seams

## Scope

### In scope
- Service implementation detection
- Server builder/registration patterns
- Handler method identification
- Transport endpoint extraction (port)
- Interceptor/middleware registration
- C++, Rust, Python, Java, TypeScript coverage

### Out of scope
- Client detection (GR-2)
- Provider/consumer linking (GR-3)
- Streaming implementation details
- Authentication/TLS configuration

## Detection Patterns

### C++ (grpc++)

```cpp
// Service implementation
class MyServiceImpl final : public MyService::Service {
  grpc::Status MyMethod(grpc::ServerContext* context,
                        const MyRequest* request,
                        MyResponse* response) override {
    // handler implementation
  }
}

// Server registration
grpc::ServerBuilder builder;
builder.AddListeningPort("0.0.0.0:50051", grpc::InsecureServerCredentials());
builder.RegisterService(&service);
std::unique_ptr<grpc::Server> server = builder.BuildAndStart();
```

**Detection signals:**
- Class inheriting `Service::Service`
- `grpc::ServerBuilder` usage
- `AddListeningPort` call (endpoint)
- `RegisterService` call

### Rust (tonic)

```rust
// Service implementation
#[tonic::async_trait]
impl MyService for MyServiceImpl {
    async fn my_method(
        &self,
        request: Request<MyRequest>,
    ) -> Result<Response<MyResponse>, Status> {
        // handler implementation
    }
}

// Server registration
Server::builder()
    .add_service(MyServiceServer::new(my_service))
    .serve("[::1]:50051".parse()?)
    .await?;
```

**Detection signals:**
- `#[tonic::async_trait]` attribute
- `impl ServiceTrait for Type` pattern
- `Server::builder()` usage
- `.add_service()` call
- `.serve()` call (endpoint)

### Python (grpcio)

```python
# Service implementation
class MyServiceServicer(my_service_pb2_grpc.MyServiceServicer):
    def MyMethod(self, request, context):
        # handler implementation
        return my_service_pb2.MyResponse()

# Server registration
server = grpc.server(futures.ThreadPoolExecutor(max_workers=10))
my_service_pb2_grpc.add_MyServiceServicer_to_server(MyServiceServicer(), server)
server.add_insecure_port('[::]:50051')
server.start()
```

**Detection signals:**
- Class inheriting `*Servicer`
- `grpc.server()` call
- `add_*Servicer_to_server()` call
- `add_insecure_port()` / `add_secure_port()` (endpoint)

### Java (grpc-java)

```java
// Service implementation
public class MyServiceImpl extends MyServiceGrpc.MyServiceImplBase {
    @Override
    public void myMethod(MyRequest request, StreamObserver<MyResponse> responseObserver) {
        // handler implementation
        responseObserver.onNext(response);
        responseObserver.onCompleted();
    }
}

// Server registration
Server server = ServerBuilder.forPort(50051)
    .addService(new MyServiceImpl())
    .build()
    .start();
```

**Detection signals:**
- Class extending `*Grpc.*ImplBase`
- `ServerBuilder.forPort()` (endpoint)
- `.addService()` call

### TypeScript (grpc-js)

```typescript
// Service implementation
const myServiceImpl: IMyServiceServer = {
  myMethod(call: ServerUnaryCall<MyRequest, MyResponse>, callback: sendUnaryData<MyResponse>) {
    // handler implementation
    callback(null, response);
  }
};

// Server registration
const server = new grpc.Server();
server.addService(MyServiceService, myServiceImpl);
server.bindAsync('0.0.0.0:50051', grpc.ServerCredentials.createInsecure(), () => {
  server.start();
});
```

**Detection signals:**
- Object implementing `I*Server` interface
- `new grpc.Server()` call
- `.addService()` call
- `.bindAsync()` call (endpoint)

## Boundary Model Mapping

Each detected server produces a `BoundaryInteractionSurface`:

| Field | Value |
|-------|-------|
| channel_kind | `GrpcChannel` |
| boundary_scope | (from endpoint: inter_process if localhost, inter_device if remote, unknown if dynamic) |
| transport_class | `schema_rpc` |
| direction | `provider` |
| interaction_pattern | (from method: unary/stream) |
| protocol | `grpc` |
| protocol_family | `rpc` |
| provenance | `extracted` |
| confidence_basis | `syntax_proven` |

Channel identity: `grpc://host:port/package.Service`

**Scope determination:**
- `0.0.0.0`, `::`, `INADDR_ANY` → scope depends on actual clients (mark as `unknown`)
- `127.0.0.1`, `localhost`, `::1` → `inter_process`
- Other IP/hostname → `inter_device`
- Dynamic/config-based → `unknown`

## Contract Association

Link server to schema:
1. Identify which service is being implemented
2. Look up service in `contract_elements`
3. Create `boundary_contracts` association
4. Link handler methods to RPC methods

## Storage

Uses existing tables:
- `boundary_interaction_surfaces` (server facts)
- `boundary_channel_details` (endpoint info)
- `boundary_contracts` (service/method links)

## CLI Extensions

```
rmap boundaries list <db> <repo> --kind grpc_channel --direction provider
  List gRPC servers

rmap boundaries show <db> <repo> <surface_uid>
  Show server details including service/methods
```

## Implementation Steps

1. **Language-specific detectors**
   - C++ grpc++ patterns
   - Rust tonic patterns
   - Python grpcio patterns
   - Java grpc-java patterns
   - TypeScript grpc-js patterns

2. **Service identification**
   - Extract service name from inheritance/implementation
   - Map to schema service element

3. **Handler method extraction**
   - Identify overridden/implemented methods
   - Map to schema RPC methods

4. **Endpoint extraction**
   - Parse port/address from builder calls
   - Handle dynamic/config-based endpoints

5. **Contract association**
   - Link surfaces to schema elements
   - Store in `boundary_contracts`

## Test Matrix

1. C++ service implementation detection
2. C++ ServerBuilder registration detection
3. Rust tonic trait implementation detection
4. Rust Server::builder detection
5. Python Servicer class detection
6. Python add_*_to_server detection
7. Java ImplBase extension detection
8. Java ServerBuilder detection
9. TypeScript server interface detection
10. TypeScript Server.addService detection
11. Endpoint extraction (literal)
12. Endpoint extraction (variable)
13. Service-to-schema linking
14. Handler-to-method linking

## Validation Repos

- gRPC examples repository
- Any repo with gRPC services
- Consider multi-language fixture

## Deliverables

- Language-specific server detectors (5 languages)
- Service-to-schema linking logic
- Handler method extraction
- Endpoint extraction
- Storage integration
- 25+ integration tests

## Success Criteria

- gRPC servers detected across all target languages
- Service implementations linked to schema services
- Handler methods linked to RPC methods
- Endpoints extracted when literal
- Correct boundary model facts produced
