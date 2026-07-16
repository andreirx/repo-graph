// MB-2A fixture: Kafka consumer
// Expected: 1 kafka_topic surface (subscribe only — run excluded per topic evidence guard)
//
// Detection requirements (triple guard):
//   1. kafkajs import present
//   2. `consumer` assigned from `kafka.consumer()` (factory provenance)
//   3. `subscribe({ topic: ... })` has extractable topic
//
// run() is NOT detected — no topic evidence (deferred to future correlation)

import { Kafka } from 'kafkajs';

const kafka = new Kafka({
    clientId: 'billing-service',
    brokers: ['localhost:9092']
});

const consumer = kafka.consumer({ groupId: 'billing-group' });

async function processOrders() {
    await consumer.connect();

    await consumer.subscribe({ topic: 'orders' });

    await consumer.run({
        eachMessage: async ({ topic, partition, message }) => {
            const order = JSON.parse(message.value?.toString() || '{}');
            console.log(`Processing order: ${order.orderId}`);
        }
    });
}

async function main() {
    await processOrders();
}

main();
