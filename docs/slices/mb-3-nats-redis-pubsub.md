# MB-3: NATS / Redis Pub-Sub Detection

Status: PLANNED
Depends: MB-1 (broker foundation)
Track: Message Broker

## Objective

Detect NATS and Redis pub-sub patterns as lighter-weight message broker
alternatives. These are simpler than RabbitMQ/Kafka but widely used for
real-time messaging and cache invalidation.

## Strategic Value

NATS and Redis pub-sub cover different niches:

**NATS:**
- Cloud-native messaging
- Kubernetes service mesh communication
- Request-reply patterns
- Low-latency pub-sub

**Redis pub-sub:**
- Cache invalidation
- Real-time notifications
- Simple event broadcasting
- Session/presence updates

These are often used alongside or instead of heavier brokers.

## Scope

### In scope

**NATS:**
- Connection establishment
- Subject subscription
- Message publish
- Request-reply pattern
- Queue groups (competing consumers)
- JetStream (durable) basic detection

**Redis pub-sub:**
- SUBSCRIBE command
- PUBLISH command
- PSUBSCRIBE (pattern subscribe)
- Channel name extraction

### Out of scope
- NATS JetStream stream/consumer config
- NATS KV/Object store
- Redis Streams (XREAD/XADD) - different model
- Redis cluster pub-sub sharding

## Boundary Classification

### Scope
- `boundary_scope = inter_process` (always)

### Transport class
- `transport_class = message_broker`

### Channel kind
```rust
pub enum ChannelKind {
    // ... existing ...
    NatsSubject,
    RedisPubsub,
}
```

### Direction
- `direction = provider` for publish
- `direction = consumer` for subscribe

### Interaction pattern
- `interaction_pattern = publish_subscribe` (default)
- `interaction_pattern = request_response` for NATS request-reply

## NATS API Detection Patterns

### Python (nats-py)
```python
import nats

nc = await nats.connect("nats://localhost:4222")

# Publish
await nc.publish("events.user.created", payload)

# Subscribe
sub = await nc.subscribe("events.>")
async for msg in sub.messages:
    process(msg)

# Request-Reply
response = await nc.request("service.users.get", request_data)

# Queue group
sub = await nc.subscribe("tasks", queue="workers")
```

### Go (nats.go)
```go
nc, _ := nats.Connect(nats.DefaultURL)

// Publish
nc.Publish("events.user.created", []byte(payload))

// Subscribe
nc.Subscribe("events.>", func(m *nats.Msg) {
    // handle
})

// Queue subscribe
nc.QueueSubscribe("tasks", "workers", func(m *nats.Msg) {
    // handle
})

// Request
msg, _ := nc.Request("service.get", []byte(request), time.Second)
```

### Node.js (nats.js)
```javascript
const { connect } = require('nats');

const nc = await connect({ servers: 'nats://localhost:4222' });

// Publish
nc.publish('events.user.created', payload);

// Subscribe
const sub = nc.subscribe('events.>');
for await (const m of sub) {
    // handle
}

// Request
const response = await nc.request('service.get', request);
```

### Java (jnats)
```java
Connection nc = Nats.connect("nats://localhost:4222");

// Publish
nc.publish("events.user.created", payload);

// Subscribe
Dispatcher d = nc.createDispatcher((msg) -> {
    // handle
});
d.subscribe("events.>");

// Request
Message reply = nc.request("service.get", request, Duration.ofSeconds(1));
```

## Redis Pub-Sub API Detection Patterns

### Python (redis-py)
```python
import redis

r = redis.Redis()
pubsub = r.pubsub()

# Subscribe
pubsub.subscribe('channel1')
pubsub.psubscribe('events.*')

for message in pubsub.listen():
    process(message)

# Publish
r.publish('channel1', 'message')
```

### Node.js (ioredis)
```javascript
const Redis = require('ioredis');

const redis = new Redis();
const sub = new Redis();

// Subscribe
sub.subscribe('channel1');
sub.psubscribe('events.*');

sub.on('message', (channel, message) => {
    // handle
});

// Publish
redis.publish('channel1', 'message');
```

### Go (go-redis)
```go
rdb := redis.NewClient(&redis.Options{Addr: "localhost:6379"})

// Subscribe
pubsub := rdb.Subscribe(ctx, "channel1")
ch := pubsub.Channel()
for msg := range ch {
    // handle
}

// Publish
rdb.Publish(ctx, "channel1", "message")
```

### Java (Jedis)
```java
Jedis jedis = new Jedis("localhost");

// Subscribe
jedis.subscribe(new JedisPubSub() {
    @Override
    public void onMessage(String channel, String message) {
        // handle
    }
}, "channel1");

// Publish
jedis.publish("channel1", "message");
```

## Channel Detail Extension

### NATS metadata_json
```json
{
  "nats": {
    "subject": "events.user.created",
    "queue_group": "workers",
    "is_request_reply": false,
    "is_jetstream": false
  }
}
```

### Redis metadata_json
```json
{
  "redis_pubsub": {
    "channel": "events.user.created",
    "pattern": false
  }
}
```

For pattern subscribe:
```json
{
  "redis_pubsub": {
    "channel": "events.*",
    "pattern": true
  }
}
```

## Evidence Structure

### NATS Publish
```json
{
  "binding_key": "nats:core:publish",
  "api_family": "nats",
  "function_name": "publish",
  "subject": "events.user.created",
  "direction": "provider"
}
```

### NATS Subscribe
```json
{
  "binding_key": "nats:core:subscribe",
  "api_family": "nats",
  "function_name": "subscribe",
  "subject": "events.>",
  "queue_group": null,
  "direction": "consumer"
}
```

### NATS Request-Reply
```json
{
  "binding_key": "nats:core:request",
  "api_family": "nats",
  "function_name": "request",
  "subject": "service.users.get",
  "direction": "bidirectional"
}
```

### Redis Publish
```json
{
  "binding_key": "redis:pubsub:publish",
  "api_family": "redis",
  "function_name": "publish",
  "channel": "notifications",
  "direction": "provider"
}
```

### Redis Subscribe
```json
{
  "binding_key": "redis:pubsub:subscribe",
  "api_family": "redis",
  "function_name": "subscribe",
  "channel": "notifications",
  "pattern": false,
  "direction": "consumer"
}
```

## Implementation Steps

1. **Add `NatsSubject` and `RedisPubsub` to `ChannelKind`**
   - Update types.rs
   - Update `default_transport_class()` -> `MessageBroker`
   - Protocol mapping: "nats", "redis-pubsub"

2. **Define metadata_json schemas**
   - NATS: subject, queue_group, is_request_reply
   - Redis: channel, pattern

3. **Create binding table entries**
   - nats-py, nats.go, nats.js, jnats
   - redis-py, ioredis, go-redis, Jedis

4. **Implement Python extractor**
   - nats-py patterns
   - redis-py pubsub patterns

5. **Implement Go extractor**
   - nats.go patterns
   - go-redis patterns

6. **Implement TypeScript extractor**
   - nats.js patterns
   - ioredis patterns

7. **Add CLI query support**
   - Filter by nats/redis channel kinds
   - Show subject/channel topology

## Test Matrix

### NATS
1. nats-py connect + publish
2. nats-py subscribe with wildcard
3. nats-py queue subscribe
4. nats-py request-reply
5. nats.go Publish
6. nats.go Subscribe
7. nats.go QueueSubscribe
8. nats.js publish
9. nats.js subscribe
10. Subject extraction from variable

### Redis
1. redis-py subscribe
2. redis-py psubscribe (pattern)
3. redis-py publish
4. ioredis subscribe
5. ioredis publish
6. go-redis Subscribe
7. go-redis Publish
8. Channel name extraction
9. Pattern vs literal distinction

## Validation Repos

- NATS examples repo
- Redis pub-sub examples
- Any real-time notification service
- Cache invalidation systems
- Kubernetes operators using NATS

## Limitations

### Subjects/channels are often dynamic
- String interpolation
- Configuration
- Runtime construction

### Linking
- Same subject/channel = linkable
- Wildcards complicate matching
- Pattern subscribes require semantic analysis

### NATS-specific
- JetStream adds durability complexity
- Request-reply is bidirectional
- Queue groups are competing consumers

### Redis-specific
- Pub-sub is fire-and-forget
- No persistence
- Pattern subscribes are glob-like

## Deliverables

- `ChannelKind::NatsSubject` variant
- `ChannelKind::RedisPubsub` variant
- NATS and Redis metadata_json schemas
- Binding table entries for Python, Go, Node.js, Java
- Python extractor for nats-py, redis-py
- Go extractor for nats.go, go-redis
- TypeScript extractor for nats.js, ioredis
- CLI filtering by NATS/Redis kinds
- 20+ unit tests
- 5+ integration tests

## Success Criteria

- Detect publish/subscribe across Python, Go, Node.js
- Extract subject/channel name
- Detect NATS queue groups
- Detect NATS request-reply pattern
- Detect Redis pattern subscribe
- Correct direction classification
- Working CLI queries
