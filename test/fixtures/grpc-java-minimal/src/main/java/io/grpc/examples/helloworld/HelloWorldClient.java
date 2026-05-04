// GR-2A smoke test fixture: gRPC client stub creation
package io.grpc.examples.helloworld;

import io.grpc.ManagedChannel;
import io.grpc.ManagedChannelBuilder;

/**
 * Client that calls the Greeter gRPC service.
 * This class contains the canonical GR-2A pattern: GreeterGrpc.newBlockingStub(channel).
 */
public class HelloWorldClient {

    private final GreeterGrpc.GreeterBlockingStub blockingStub;

    /**
     * GR-2A detection target: this constructor calls GreeterGrpc.newBlockingStub(channel).
     */
    public HelloWorldClient(ManagedChannel channel) {
        blockingStub = GreeterGrpc.newBlockingStub(channel);
    }

    public void greet(String name) {
        HelloRequest request = HelloRequest.newBuilder()
                .setName(name)
                .build();
        HelloReply response = blockingStub.sayHello(request);
        System.out.println("Greeting: " + response.getMessage());
    }

    public static void main(String[] args) {
        String target = "localhost:50051";
        ManagedChannel channel = ManagedChannelBuilder.forTarget(target)
                .usePlaintext()
                .build();

        HelloWorldClient client = new HelloWorldClient(channel);
        client.greet("world");

        channel.shutdown();
    }
}
