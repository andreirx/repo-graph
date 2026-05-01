# MB-2: Kafka Topic Detection

Status: PLANNED
Depends: MB-1 (broker foundation)
Track: Message Broker

## Objective

Detect Kafka producer and consumer patterns across Python, Java, and Node.js.
Map topic/partition/consumer-group topology as boundary interaction surfaces.

## Strategic Value

Kafka is the dominant choice for:
- High-throughput event streaming
- Event sourcing architectures
- Log aggregation
- Real-time data pipelines
- Microservice event buses

The topology has unique characteristics:
- Topics (partitioned log)
- Partitions (parallelism unit)
- Consumer groups (competing consumers)
- Serializers/deserializers (often schema-backed)

Kafka detection complements RabbitMQ to cover the two dominant broker families.

## Scope

### In scope
- Producer creation and configuration
- Consumer creation and configuration
- Topic subscription
- Message send/produce
- Message poll/consume
- Consumer group extraction
- Topic name extraction
- Serializer/deserializer class detection
- Key/value type hints

### Out of scope
- Partition assignment analysis
- Offset management analysis
- Schema Registry integration (future - connects to CS track)
- Kafka Streams topology
- KSQL patterns
- Exactly-once semantics analysis

## Boundary Classification

### Scope
- `boundary_scope = inter_process` (always)

### Transport class
- `transport_class = message_broker`

### Channel kind
```rust
pub enum ChannelKind {
    // ... existing ...
    KafkaTopic,
}
```

### Direction
- `direction = provider` for producers
- `direction = consumer` for consumers

### Interaction pattern
- `interaction_pattern = publish_subscribe` (default)
- `interaction_pattern = stream` for streaming consumption

## API Detection Patterns

### Python (kafka-python)
```python
from kafka import KafkaProducer, KafkaConsumer

# Producer
producer = KafkaProducer(
    bootstrap_servers=['localhost:9092'],
    value_serializer=lambda v: json.dumps(v).encode('utf-8')
)
producer.send('my-topic', value={'key': 'value'})

# Consumer
consumer = KafkaConsumer(
    'my-topic',
    bootstrap_servers=['localhost:9092'],
    group_id='my-group',
    value_deserializer=lambda m: json.loads(m.decode('utf-8'))
)
for message in consumer:
    process(message)
```

### Python (confluent-kafka)
```python
from confluent_kafka import Producer, Consumer

# Producer
producer = Producer({'bootstrap.servers': 'localhost:9092'})
producer.produce('my-topic', key='key', value='value')

# Consumer
consumer = Consumer({
    'bootstrap.servers': 'localhost:9092',
    'group.id': 'my-group'
})
consumer.subscribe(['my-topic'])
while True:
    msg = consumer.poll(1.0)
```

### Java (kafka-clients)
```java
// Producer
Properties props = new Properties();
props.put("bootstrap.servers", "localhost:9092");
props.put("key.serializer", StringSerializer.class.getName());
props.put("value.serializer", StringSerializer.class.getName());

Producer<String, String> producer = new KafkaProducer<>(props);
producer.send(new ProducerRecord<>("my-topic", key, value));

// Consumer
props.put("group.id", "my-group");
props.put("key.deserializer", StringDeserializer.class.getName());
props.put("value.deserializer", StringDeserializer.class.getName());

Consumer<String, String> consumer = new KafkaConsumer<>(props);
consumer.subscribe(Arrays.asList("my-topic"));
ConsumerRecords<String, String> records = consumer.poll(Duration.ofMillis(100));
```

### Java (Spring Kafka)
```java
@KafkaListener(topics = "my-topic", groupId = "my-group")
public void listen(String message) {
    // consume
}

@Autowired
private KafkaTemplate<String, String> kafkaTemplate;

public void send(String message) {
    kafkaTemplate.send("my-topic", message);
}
```

### Node.js (kafkajs)
```javascript
const { Kafka } = require('kafkajs');

const kafka = new Kafka({
    clientId: 'my-app',
    brokers: ['localhost:9092']
});

// Producer
const producer = kafka.producer();
await producer.send({
    topic: 'my-topic',
    messages: [{ value: 'Hello' }]
});

// Consumer
const consumer = kafka.consumer({ groupId: 'my-group' });
await consumer.subscribe({ topic: 'my-topic' });
await consumer.run({
    eachMessage: async ({ topic, partition, message }) => {
        // process
    }
});
```

## Channel Detail Extension

Use `metadata_json` for Kafka-specific fields:
```json
{
  "kafka": {
    "topic": "my-topic",
    "group_id": "my-group",
    "key_serializer": "StringSerializer",
    "value_serializer": "JsonSerializer",
    "bootstrap_servers": "localhost:9092"
  }
}
```

## Evidence Structure

### Producer
```json
{
  "binding_key": "kafka:producer:send",
  "api_family": "kafka",
  "function_name": "send",
  "topic": "my-topic",
  "key_type": "string",
  "value_type": "json",
  "direction": "provider"
}
```

### Consumer
```json
{
  "binding_key": "kafka:consumer:subscribe",
  "api_family": "kafka",
  "function_name": "subscribe",
  "topics": ["my-topic"],
  "group_id": "my-group",
  "direction": "consumer"
}
```

### Spring @KafkaListener
```json
{
  "binding_key": "spring:kafka:listener",
  "api_family": "spring-kafka",
  "annotation": "@KafkaListener",
  "topics": ["my-topic"],
  "group_id": "my-group",
  "direction": "consumer"
}
```

## Implementation Steps

1. **Add `KafkaTopic` to `ChannelKind`**
   - Update types.rs
   - Update `default_transport_class()` -> `MessageBroker`
   - Update protocol mapping -> "kafka"

2. **Define metadata_json schema for Kafka**
   - Topic, group_id, serializers, bootstrap_servers

3. **Create binding table entries**
   - kafka-python
   - confluent-kafka
   - kafka-clients (Java)
   - Spring Kafka
   - kafkajs (Node.js)

4. **Implement Python extractor**
   - kafka-python patterns
   - confluent-kafka patterns
   - Topic name extraction from constructor/method

5. **Implement Java extractor**
   - KafkaProducer/KafkaConsumer detection
   - ProducerRecord topic extraction
   - @KafkaListener annotation detection

6. **Implement TypeScript extractor**
   - kafkajs patterns

7. **Add CLI query support**
   - Filter by channel_kind = kafka_topic
   - Show topic/group topology

## Test Matrix

1. kafka-python Producer creation
2. kafka-python producer.send() with topic
3. kafka-python Consumer with group_id
4. confluent-kafka Producer.produce()
5. confluent-kafka Consumer.subscribe()
6. Java KafkaProducer instantiation
7. Java producer.send(ProducerRecord)
8. Java KafkaConsumer.subscribe()
9. Spring @KafkaListener detection
10. Spring KafkaTemplate.send()
11. kafkajs producer.send()
12. kafkajs consumer.subscribe()
13. Topic name from variable
14. Group ID extraction
15. Serializer class detection

## Validation Repos

- Any microservice app using Kafka
- Spring Kafka examples
- Confluent examples
- Debezium (CDC to Kafka)
- Apache Flink Kafka connectors

## Limitations

### Topic names can be dynamic
- Template strings
- Configuration injection
- Environment variables

### Linking is topic-based
- Producer + consumer on same topic = linkable
- Group ID matters for consumer behavior
- Partitioning is runtime

### Serializers hint at schema
- JsonSerializer, AvroSerializer, ProtobufSerializer
- Can connect to CS track for schema association

### Best for
- Producer/consumer inventory
- Topic surface discovery
- Group membership mapping

### Not for
- Partition analysis
- Offset/lag analysis
- Exactly-once flow verification

## Deliverables

- `ChannelKind::KafkaTopic` variant
- Kafka metadata_json schema
- Binding table entries for Python, Java, Node.js
- Python extractor for kafka-python, confluent-kafka
- Java extractor for kafka-clients, Spring Kafka
- TypeScript extractor for kafkajs
- CLI filtering by Kafka kind
- 20+ unit tests
- 5+ integration tests on real repos

## Success Criteria

- Detect produce/consume across Python, Java, Node.js
- Extract topic name
- Extract consumer group ID
- Detect serializer/deserializer hints
- Correct direction classification
- Working CLI queries
- Validated on Spring Kafka patterns
