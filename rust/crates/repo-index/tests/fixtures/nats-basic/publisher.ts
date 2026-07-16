// MB-3A fixture: NATS publisher
// Expected: 1 nats_subject surface (publish)
//
// Detection requirements (triple guard):
//   1. nats import present
//   2. `nc` assigned from `connect()` (connection provenance)
//   3. `publish(subject, ...)` has extractable subject

import { connect } from 'nats';

async function publishOrder(orderId: string, amount: number) {
    const nc = await connect({ servers: 'localhost:4222' });

    nc.publish('orders.created', JSON.stringify({ orderId, amount }));

    console.log(`Order ${orderId} published`);

    await nc.drain();
}

async function main() {
    await publishOrder('order-123', 99.99);
}

main();
