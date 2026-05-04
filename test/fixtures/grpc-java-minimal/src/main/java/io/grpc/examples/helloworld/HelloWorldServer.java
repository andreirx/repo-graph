// GR-1A smoke test fixture: gRPC server implementation
package io.grpc.examples.helloworld;

import io.grpc.Server;
import io.grpc.ServerBuilder;
import io.grpc.stub.StreamObserver;

/**
 * Server that manages a Greeter gRPC service.
 * This class contains the canonical GR-1A pattern: GreeterImpl extends GreeterImplBase.
 */
public class HelloWorldServer {

    private Server server;

    private void start() throws Exception {
        int port = 50051;
        server = ServerBuilder.forPort(port)
                .addService(new GreeterImpl())
                .build()
                .start();
        System.out.println("Server started on port " + port);
    }

    public static void main(String[] args) throws Exception {
        HelloWorldServer server = new HelloWorldServer();
        server.start();
    }

    /**
     * The gRPC service implementation.
     * GR-1A detection target: this class extends GreeterGrpc.GreeterImplBase
     */
    static class GreeterImpl extends GreeterGrpc.GreeterImplBase {
        @Override
        public void sayHello(HelloRequest req, StreamObserver<HelloReply> responseObserver) {
            HelloReply reply = HelloReply.newBuilder()
                    .setMessage("Hello " + req.getName())
                    .build();
            responseObserver.onNext(reply);
            responseObserver.onCompleted();
        }
    }
}
