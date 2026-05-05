// MB-3A fixture: NATS subscriber
// Expected: 1 nats_subject surface (subscribe)
//
// Detection requirements (triple guard):
//   1. nats import present
//   2. `nc` assigned from `connect()` (connection provenance)
//   3. `subscribe(subject, ...)` has extractable subject
//
// request() is NOT detected - mixed semantics (deferred to MB-3B)

import { connect, StringCodec } from 'nats';

async function processOrders() {
    const nc = await connect({ servers: 'localhost:4222' });
    const sc = StringCodec();

    const sub = nc.subscribe('orders.created');

    console.log('Listening for orders...');

    for await (const msg of sub) {
        const order = JSON.parse(sc.decode(msg.data));
        console.log(`Processing order: ${order.orderId}`);
    }

    await nc.drain();
}

async function main() {
    await processOrders();
}

main();
