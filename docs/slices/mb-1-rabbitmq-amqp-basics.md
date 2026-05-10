# MB-1: RabbitMQ / AMQP Basic Detection

Status: PARTIAL — Rust path shipped for TS/JS source patterns; Python/Java/C# pending
Depends: Foundation (migration 025), BI-1B TCP/UDP (network context)
Track: Message Broker

## Shipped (2025-Q1)

**Rust runtime detects TS/JS amqplib patterns:**
- `channel.sendToQueue()` — provider, queue publish
- `channel.publish()` — provider, exchange publish
- `channel.consume()` — consumer, queue consumption
- `channel.assertQueue()` — bidirectional, queue declaration
- `channel.assertExchange()` — bidirectional, exchange declaration
- `channel.bindQueue()` — bidirectional, binding

Implementation: `rust/crates/ts-extractor/src/amqp_detector.rs`
Bindings: `rust/crates/boundary-interaction/bindings.toml` (amqplib section)
Tests: `rust/crates/repo-index/tests/mb_1a_amqp.rs`

Channel kind: `amqp_queue`
Transport class: `message_broker`
Validation: `rmap boundaries list --transport message_broker`

## Remaining Work

**Python (pika, kombu):** NOT STARTED
**Java (RabbitMQ client):** NOT STARTED
**C# (RabbitMQ.Client):** NOT STARTED

Blocker: Language extractors need message-broker detection integration. Python extractor exists but lacks AMQP call pattern detection.

## Original Objective

Detect RabbitMQ / AMQP producer and consumer patterns across Python, Java,
Node.js, and C#. Map exchange/queue/binding topology as boundary interaction
surfaces with broker-specific channel details.

## Strategic Value

RabbitMQ is often the integration backbone for:
- Microservice communication
- Event-driven architectures
- Task queues (Celery, etc.)
- Cross-language integration

The topology is explicit in AMQP:
- Exchanges (routing layer)
- Queues (storage/delivery layer)
- Bindings (routing rules)
- Routing keys

This makes RabbitMQ a strong proving ground for broker modeling.

## Scope

### In scope
- Connection establishment detection
- Channel creation detection
- Exchange declaration (`exchange_declare`)
- Queue declaration (`queue_declare`)
- Queue binding (`queue_bind`)
- Basic publish (`basic_publish`)
- Basic consume (`basic_consume`)
- Exchange type classification (direct, fanout, topic, headers)
- Routing key extraction
- Queue name extraction (including server-generated)

### Out of scope
- Dead-letter exchange configuration (future)
- TTL/expiration analysis
- Consumer acknowledgment flow analysis
- Connection pooling patterns
- Clustering/federation topology
- RPC-over-AMQP patterns (future)

## Boundary Classification

### Scope
- `boundary_scope = inter_process` (always)

### Transport class
- `transport_class = message_broker`

### Channel kind
Add new variant:
```rust
pub enum ChannelKind {
    // ... existing ...
    AmqpExchange,
    AmqpQueue,
}
```

Or single:
```rust
    AmqpChannel,  // with exchange/queue detail in ChannelDetail
```

**Recommendation:** Use single `AmqpChannel` kind with topology in metadata.

### Direction
- `direction = provider` for publishers
- `direction = consumer` for consumers
- `direction = bidirectional` for declare/bind (topology definition)

### Interaction pattern
- `interaction_pattern = publish_subscribe` for fanout/topic exchanges
- `interaction_pattern = fire_and_forget` for direct with no reply
- `interaction_pattern = request_response` for RPC patterns (future)

## API Detection Patterns

### Python (pika)
```python
# Connection
connection = pika.BlockingConnection(pika.ConnectionParameters('localhost'))
channel = connection.channel()

# Exchange
channel.exchange_declare(exchange='logs', exchange_type='fanout')

# Queue
channel.queue_declare(queue='task_queue', durable=True)

# Binding
channel.queue_bind(exchange='logs', queue='my_queue', routing_key='*.error')

# Publish
channel.basic_publish(
    exchange='',
    routing_key='task_queue',
    body=message
)

# Consume
channel.basic_consume(
    queue='task_queue',
    on_message_callback=callback
)
```

### Python (kombu/Celery)
```python
from kombu import Connection, Exchange, Queue

exchange = Exchange('tasks', type='direct')
queue = Queue('tasks', exchange, routing_key='tasks')

with Connection('amqp://guest:guest@localhost//') as conn:
    producer = conn.Producer()
    producer.publish(message, exchange=exchange, routing_key='tasks')
```

### Java (RabbitMQ client)
```java
ConnectionFactory factory = new ConnectionFactory();
factory.setHost("localhost");
Connection connection = factory.newConnection();
Channel channel = connection.createChannel();

channel.exchangeDeclare("logs", "fanout");
channel.queueDeclare("task_queue", true, false, false, null);
channel.queueBind("my_queue", "logs", "");

channel.basicPublish("", "task_queue", null, message.getBytes());
channel.basicConsume("task_queue", true, consumer);
```

### Node.js (amqplib)
```javascript
const connection = await amqp.connect('amqp://localhost');
const channel = await connection.createChannel();

await channel.assertExchange('logs', 'fanout');
await channel.assertQueue('task_queue', { durable: true });
await channel.bindQueue('my_queue', 'logs', '');

channel.publish('', 'task_queue', Buffer.from(message));
channel.consume('task_queue', callback);
```

### C# (RabbitMQ.Client)
```csharp
var factory = new ConnectionFactory() { HostName = "localhost" };
using var connection = factory.CreateConnection();
using var channel = connection.CreateModel();

channel.ExchangeDeclare("logs", ExchangeType.Fanout);
channel.QueueDeclare("task_queue", durable: true, ...);
channel.QueueBind("my_queue", "logs", "");

channel.BasicPublish("", "task_queue", null, body);
channel.BasicConsume("task_queue", true, consumer);
```

## Channel Detail Extension

Add broker-specific fields:
```rust
pub struct ChannelDetail {
    // ... existing fields ...

    // Message broker fields
    pub broker_exchange: Option<String>,
    pub broker_queue: Option<String>,
    pub broker_routing_key: Option<String>,
    pub broker_exchange_type: Option<String>,  // direct, fanout, topic, headers
    pub broker_durable: Option<bool>,
}
```

Or use `metadata_json`:
```json
{
  "amqp": {
    "exchange": "logs",
    "exchange_type": "fanout",
    "queue": "task_queue",
    "routing_key": "*.error",
    "durable": true
  }
}
```

## Evidence Structure

### Exchange declaration
```json
{
  "binding_key": "rabbitmq:amqp:exchange_declare",
  "api_family": "rabbitmq",
  "function_name": "exchange_declare",
  "exchange_name": "logs",
  "exchange_type": "fanout",
  "durable": false,
  "auto_delete": false,
  "direction": "bidirectional"
}
```

### Publish
```json
{
  "binding_key": "rabbitmq:amqp:basic_publish",
  "api_family": "rabbitmq",
  "function_name": "basic_publish",
  "exchange": "",
  "routing_key": "task_queue",
  "direction": "provider"
}
```

### Consume
```json
{
  "binding_key": "rabbitmq:amqp:basic_consume",
  "api_family": "rabbitmq",
  "function_name": "basic_consume",
  "queue": "task_queue",
  "auto_ack": true,
  "direction": "consumer"
}
```

## Implementation Steps

1. **Add `AmqpChannel` to `ChannelKind`**
   - Update types.rs
   - Update `default_transport_class()` -> `MessageBroker`
   - Update protocol mapping -> "amqp"

2. **Extend `ChannelDetail` for broker fields**
   - Add optional broker fields, or
   - Define `metadata_json` schema

3. **Create binding table entries**
   - pika (Python)
   - kombu (Python)
   - RabbitMQ Java client
   - amqplib (Node.js)
   - RabbitMQ.Client (C#)

4. **Implement Python extractor**
   - pika pattern detection
   - kombu/Celery pattern detection
   - Exchange/queue/routing key extraction

5. **Implement Java extractor**
   - RabbitMQ client detection
   - Method call argument extraction

6. **Implement TypeScript extractor**
   - amqplib pattern detection

7. **Add CLI query support**
   - Filter by channel_kind = amqp_channel
   - Show exchange/queue/routing topology

## Test Matrix

1. pika connection + publish
2. pika connection + consume
3. pika exchange_declare with type extraction
4. pika queue_declare with durability
5. pika queue_bind with routing key
6. kombu Exchange/Queue/Producer
7. Java channel.basicPublish
8. Java channel.basicConsume
9. Node.js amqplib assertExchange
10. Node.js amqplib publish
11. C# BasicPublish
12. Exchange type classification (direct/fanout/topic/headers)
13. Routing key extraction from variable
14. Server-generated queue name handling

## Validation Repos

- Celery (extensive RabbitMQ usage)
- Any Django/Flask app with Celery
- RabbitMQ tutorials repo
- MassTransit (C# message bus)
- Spring AMQP examples

## Limitations

### Topology is often runtime
- Queue names can be server-generated
- Routing keys can be dynamic
- Exchange bindings may come from config

### Linking is possible but limited
- Publisher + consumer on same queue = linkable
- Routing key patterns require semantic analysis
- Config/code split complicates detection

### Best for
- Producer/consumer role detection
- Exchange/queue inventory
- Routing key surface

### Not for
- Full topology reconstruction
- Dead-letter flow analysis
- Message content analysis

## Deliverables

- `ChannelKind::AmqpChannel` variant
- Broker fields in ChannelDetail or metadata_json schema
- Binding table entries for pika, kombu, Java, Node.js, C#
- Python extractor for pika/kombu
- Java extractor for RabbitMQ client
- TypeScript extractor for amqplib
- CLI filtering by AMQP kind
- 20+ unit tests
- 5+ integration tests on real repos

## Success Criteria

- Detect publish/consume across Python, Java, Node.js
- Extract exchange name, queue name, routing key
- Classify exchange type (direct/fanout/topic/headers)
- Correct direction classification
- Working CLI queries
- Validated on Celery patterns
