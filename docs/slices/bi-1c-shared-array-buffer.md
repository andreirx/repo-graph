# BI-1C: SharedArrayBuffer and Worker Boundaries

Status: PLANNED
Depends: BI-1A (shipped)
Track: A (Raw Transport)

## Objective

Detect SharedArrayBuffer usage in JavaScript/TypeScript as a boundary
interaction mechanism. SAB enables shared-memory concurrency between
the main thread and Web Workers, requiring synchronization discipline.

## Problem Context

SharedArrayBuffer is:
- A shared-memory IPC mechanism between JS execution contexts
- Requires `Atomics` for safe synchronization
- Used for high-performance concurrent data processing
- A security-sensitive feature (requires cross-origin isolation)

This is the JS/TS equivalent of POSIX shared memory in native code.

## Scope

### In scope
- SharedArrayBuffer allocation sites
- Worker creation with SAB transfer
- Atomics usage patterns
- TypedArray views over SAB
- Main thread vs worker role detection
- postMessage transfer patterns

### Out of scope
- Worker thread creation without SAB (regular message passing)
- WebAssembly shared memory (future extension)
- Node.js worker_threads (could extend this slice)

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

// Typed array view
const view = new Int32Array(sab);
```

### Worker Transfer

```typescript
// Main thread sends to worker
worker.postMessage({ buffer: sab }, [sab]);

// Or without transfer list (shared, not transferred)
worker.postMessage({ buffer: sab });
```

### Atomics Usage

```typescript
// Synchronization primitives
Atomics.wait(view, index, value);
Atomics.notify(view, index, count);
Atomics.load(view, index);
Atomics.store(view, index, value);
Atomics.add(view, index, value);
Atomics.compareExchange(view, index, expected, replacement);
```

### Worker Reception

```typescript
// In worker
self.onmessage = (e) => {
  const sab = e.data.buffer;
  const view = new Int32Array(sab);
  // Use Atomics for synchronization
};
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

**Provider (allocator):**
- `new SharedArrayBuffer(size)`
- First `postMessage` sender of a SAB

**Consumer (receiver):**
- `onmessage` handler receiving SAB
- Worker accessing SAB without allocating

**Bidirectional:**
- When both roles coexist in same file/symbol

## Dual Projection

SharedArrayBuffer creates BOTH:
1. **Boundary interaction fact** (channel between contexts)
2. **State/resource fact** (shared mutable state)

The state projection should create:
- Node kind: `STATE` with subtype `SHARED_BUFFER`
- Edges: `READS`, `WRITES` from accessor sites

This requires coordination with the state-boundary model.

## Implementation Approach

### Phase 1: Boundary Detection
- Detect SAB allocation
- Detect postMessage with SAB
- Detect onmessage receiving SAB
- Classify provider/consumer roles
- Emit `BoundaryInteractionSurface` facts

### Phase 2: State Projection
- Detect TypedArray views over SAB
- Detect Atomics operations
- Emit `STATE` nodes with SHARED_BUFFER subtype
- Emit READS/WRITES edges

### Phase 3: Synchronization Evidence
- Detect Atomics.wait/notify patterns
- Annotate surfaces with synchronization evidence
- Flag unsynchronized access as potential hazard

## Binding Table

| Language | Pattern | Detection |
|----------|---------|-----------|
| typescript | `new SharedArrayBuffer` | Allocation (provider) |
| typescript | `postMessage(*, [sab])` | Transfer (provider→consumer) |
| typescript | `new Worker(*)` | Worker creation context |
| typescript | `onmessage = *` | Message handler (potential consumer) |
| typescript | `Atomics.wait` | Synchronization wait |
| typescript | `Atomics.notify` | Synchronization signal |
| typescript | `Atomics.load/store/add/...` | Atomic operation |
| typescript | `new Int32Array(sab)` | View creation |

## Evidence Structure

```json
{
  "version": 1,
  "mechanism": "SharedArrayBuffer",
  "allocation_site": "src/main.ts:42",
  "buffer_size": 1024,
  "transfer_targets": ["worker.ts"],
  "atomics_usage": true,
  "sync_primitives": ["wait", "notify"],
  "typed_array_views": ["Int32Array", "Float64Array"]
}
```

## Test Matrix

1. SAB allocation detection
2. postMessage transfer detection
3. Worker onmessage reception detection
4. Atomics usage detection
5. TypedArray view detection
6. Provider/consumer role classification
7. Dual projection (boundary + state)
8. Multiple workers sharing same SAB

## Validation Repos

- Any repo using SharedArrayBuffer for worker communication
- Consider creating a dedicated fixture

## Node.js Extension (Future)

Node.js `worker_threads` with SharedArrayBuffer:

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

This follows the same pattern and can reuse the same detection logic.

## Deliverables

- SAB allocation detector in TS extractor
- postMessage/onmessage pattern detection
- Atomics usage detection
- Role classification logic
- Dual projection to boundary + state models
- CLI filter support (`--kind shared_array_buffer`)
- 15+ integration tests

## Success Criteria

- SAB allocation sites detected
- Worker transfer patterns detected
- Provider/consumer roles correctly classified
- Atomics usage captured in evidence
- Dual projection working (boundary + state facts)
- TypedArray view tracking
