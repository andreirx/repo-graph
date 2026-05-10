# BI-1C: SharedArrayBuffer and Atomics Boundaries

Status: SHIPPED
Depends: BI-1A (shipped)
Track: A (Raw Transport)

## Objective

Detect SharedArrayBuffer allocation and Atomics synchronization usage in
JavaScript/TypeScript as boundary interaction mechanisms. This surfaces
where shared-memory concurrency exists in a codebase.

## Problem Context

SharedArrayBuffer is:
- A shared-memory IPC mechanism between JS execution contexts
- Requires `Atomics` for safe synchronization
- Used for high-performance concurrent data processing
- A security-sensitive feature (requires cross-origin isolation)

This is the JS/TS equivalent of POSIX shared memory in native code.

## Scope

### In scope
- SharedArrayBuffer allocation sites (`new SharedArrayBuffer(...)`)
- Atomics usage patterns (`Atomics.wait`, `Atomics.notify`, `Atomics.store`, etc.)

### Explicitly out of scope (Option A decision)
- Worker creation (`new Worker(...)`) — generic worker spawning, no SAB correlation
- postMessage calls (`worker.postMessage(...)`) — generic message passing, no SAB in args proven
- Worker reception (`onmessage` handler) — would require dataflow to prove SAB received
- TypedArray views over SAB — tracked via SAB allocation, not view creation

**Why not Worker/postMessage?** Labeling any `new Worker()` or `postMessage()` as
a SharedArrayBuffer boundary overclaims. Most workers use regular message passing
without SAB. Proving SAB transfer requires dataflow analysis (tracking which variable
is passed to postMessage), which is out of scope for breadth-first hints.

A future BI-1E (Web Worker) slice may cover general worker patterns with a separate
`web_worker` channel kind.

### Also out of scope (future extensions)
- WebAssembly shared memory
- Node.js worker_threads

## Channel Kind

New value: `SharedArrayBuffer`

## Interaction Pattern

```
interaction_pattern = shared_state
```

SharedArrayBuffer is fundamentally shared state, not message passing.

## Detection Patterns

### Allocation Site

```typescript
// Main thread allocates
const sab = new SharedArrayBuffer(1024);

// Typed array view (not tracked directly — SAB allocation is the signal)
const view = new Int32Array(sab);
```

### Atomics Usage

```typescript
// Synchronization primitives
Atomics.wait(view, index, value);      // consumer — waits for signal
Atomics.notify(view, index, count);    // provider — wakes waiters
Atomics.load(view, index);             // bidirectional — reads shared state
Atomics.store(view, index, value);     // bidirectional — writes shared state
Atomics.add(view, index, value);       // bidirectional
Atomics.sub(view, index, value);       // bidirectional
Atomics.and(view, index, value);       // bidirectional
Atomics.or(view, index, value);        // bidirectional
Atomics.xor(view, index, value);       // bidirectional
Atomics.exchange(view, index, value);  // bidirectional
Atomics.compareExchange(view, index, expected, replacement); // bidirectional
```

## Boundary Model Mapping

| Concept | SAB Mapping |
|---------|-------------|
| surface | Code site with SAB allocation/usage |
| channel_kind | SharedArrayBuffer |
| channel_identity | Variable name or allocation site key |
| boundary_scope | intra_process (same OS process, different execution contexts) |
| transport_class | raw_ipc |
| direction | provider (allocator), consumer (receiver), bidirectional (both) |
| interaction_pattern | shared_state |

**Why intra_process, not inter_process:**

Web Workers in browsers and Node.js worker_threads run in the same OS process
but different execution contexts (V8 isolates, separate event loops). This is
fundamentally different from Unix shared memory between separate processes.

- `intra_process`: main thread ↔ Web Worker (same process, shared address space)
- `inter_process`: POSIX shm_open between two separate processes

## Role Detection

**Provider:**
- `new SharedArrayBuffer(size)` — creates shared memory region
- `Atomics.notify(view, index, count)` — signals waiting consumers

**Consumer:**
- `Atomics.wait(view, index, value)` — blocks until signaled

**Bidirectional:**
- `Atomics.load/store/add/sub/and/or/xor/exchange/compareExchange` — shared state access

## Dual Projection (future)

SharedArrayBuffer creates BOTH:
1. **Boundary interaction fact** (channel between contexts)
2. **State/resource fact** (shared mutable state)

The state projection should create:
- Node kind: `STATE` with subtype `SHARED_BUFFER`
- Edges: `READS`, `WRITES` from accessor sites

This requires coordination with the state-boundary model.

## Implementation (shipped)

### What is detected
- SAB allocation (`new SharedArrayBuffer(...)`)
- Atomics synchronization calls
- Enclosing function context for stable keys

### What is NOT detected (Option A)
- Worker creation — no SAB correlation
- postMessage — no SAB in arguments proven
- TypedArray view creation — tracked via SAB allocation
- onmessage handlers — would require dataflow

## Binding Table (shipped)

| Language | Pattern | Direction |
|----------|---------|-----------|
| typescript | `new SharedArrayBuffer(...)` | provider |
| typescript | `Atomics.wait(...)` | consumer |
| typescript | `Atomics.notify(...)` | provider |
| typescript | `Atomics.load(...)` | bidirectional |
| typescript | `Atomics.store(...)` | bidirectional |
| typescript | `Atomics.add/sub/and/or/xor(...)` | bidirectional |
| typescript | `Atomics.exchange(...)` | bidirectional |
| typescript | `Atomics.compareExchange(...)` | bidirectional |

## Evidence Structure

```json
{
  "version": 1,
  "mechanism": "SharedArrayBuffer",
  "function_name": "Atomics.wait",
  "enclosing_function": "processData"
}
```

## Test Matrix (shipped)

1. SAB allocation detection (`new SharedArrayBuffer(...)`)
2. Atomics.wait/notify/load/store/add/sub/and/or/xor/exchange/compareExchange detection
3. Provider/consumer/bidirectional role classification
4. Enclosing function context extraction
5. Negative tests: Worker/postMessage do NOT emit SAB surfaces

## Validation

- Dedicated fixture: `test/fixtures/shared-array-buffer/`
  - `main.ts`: SAB allocation + Atomics.store + Atomics.notify
  - `worker.ts`: Atomics.wait + Atomics.load + Atomics.store
- 9 unit tests (boundary_detector.rs)
- 3 integration tests (bi_shared_array_buffer.rs)
- 4 CLI adapter tests (boundaries_command.rs)

## Future Extensions

### Node.js worker_threads

```javascript
const { Worker, isMainThread, parentPort, workerData } = require('worker_threads');

if (isMainThread) {
  const sab = new SharedArrayBuffer(1024);
  const worker = new Worker(__filename, { workerData: { sab } });
} else {
  const { sab } = workerData;
  // Use shared buffer
}
```

Same Atomics detection applies; worker_threads-specific patterns could
be added as additional bindings.

### BI-1E: Web Worker slice (deferred)

General worker detection with separate `web_worker` channel kind:
- `new Worker(...)`
- `postMessage(...)`
- `onmessage` handlers

This would NOT claim SAB usage — just worker communication presence.

## Deliverables (shipped)

- SAB/Atomics detector in `ts-extractor/src/boundary_detector.rs`
- 12 binding table entries in `boundary-interaction/bindings.toml`
- Integration through `repo-index/src/compose.rs`
- CLI filter support (`--kind shared_array_buffer`, `--kind sab`, `--kind atomics`)
- Fixture + 16 total tests

## Success Criteria (met)

- SAB allocation sites detected
- Atomics synchronization patterns detected
- Provider/consumer/bidirectional roles correctly classified
- Worker/postMessage correctly NOT detected (Option A semantic honesty)
