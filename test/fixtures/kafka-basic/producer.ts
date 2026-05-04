// MB-2A fixture: Kafka producer
// Expected: 1 kafka_topic surface (send)
//
// Detection requirements (triple guard):
//   1. kafkajs import present
//   2. `producer` assigned from `kafka.producer()` (factory provenance)
//   3. `send({ topic: ... })` has extractable topic

import { Kafka } from 'kafkajs';

const kafka = new Kafka({
    clientId: 'order-service',
    brokers: ['localhost:9092']
});

const producer = kafka.producer();

async function publishOrder(orderId: string, amount: number) {
    await producer.connect();

    await producer.send({
        topic: 'orders',
        messages: [
            { key: orderId, value: JSON.stringify({ orderId, amount }) }
        ]
    });

    console.log(`Order ${orderId} published`);
}

async function main() {
    await publishOrder('order-123', 99.99);
    await producer.disconnect();
}

main();
