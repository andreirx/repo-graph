# BI-LX-2: SysV Message Queues Detection

**Status:** SHIPPED  
**Slice:** BI-LX-2  
**Family:** Linux IPC  
**Language:** C (first)

## Problem

SysV message queues are a fundamental Linux/Unix IPC mechanism missing from boundary
interaction coverage. Agents need to see where message-queue-based IPC exists in
legacy C codebases. This is discovery-oriented: we surface the presence and location
of SysV msgq usage, not runtime behavior or cross-process pairing.

SysV message queues predate POSIX mq and remain common in:
- Legacy Unix systems
- Embedded Linux
- System daemons with priority-based messaging
- IPC-heavy system software
- Process coordination with typed message discrimination

## Scope

Detect these SysV message queue APIs in C source:

| Function | Signature | Role |
|----------|-----------|------|
| `msgget` | `msgget(key_t key, int msgflg)` | Queue creation/lookup |
| `msgsnd` | `msgsnd(int msqid, const void *msgp, size_t msgsz, int msgflg)` | Send message |
| `msgrcv` | `msgrcv(int msqid, void *msgp, size_t msgsz, long msgtyp, int msgflg)` | Receive message |
| `msgctl` | `msgctl(int msqid, int cmd, struct msqid_ds *buf)` | Control/remove/query |

**In scope:**
- C syntax-level detection
- Evidence extraction (key, flags/cmd where visible)
- Boundary surface emission per callsite

**Out of scope (deferred):**
- Queue identity correlation across callsites
- Ownership/lifecycle reconstruction
- Producer/consumer inference beyond send/receive
- Priority/type semantics interpretation
- Runtime success verification
- Cross-process pairing
- Wrapper function tracking

## Surface Semantics

### Channel Kind Decision: Reuse `message_queue`

**Decision:** Reuse existing `message_queue` channel kind with `api_family = sysv_msgq`.

**Rationale:**
- Preserves one user-facing concept for message queues
- Simpler CLI/read surface (`--kind message_queue` finds both POSIX and SysV)
- Lower taxonomy sprawl
- Better breadth-first fit
- Evidence payload carries the family distinction when needed

### Surface Properties

| Property | Value |
|----------|-------|
| `channel_kind` | `message_queue` |
| `api_family` | `sysv_msgq` |
| `protocol_family` | `local_ipc` |
| `boundary_scope` | `inter_process` (fixed) |
| `interaction_pattern` | `fire_and_forget` |

### Direction Semantics

Direction follows explicit API semantics:

| Function | Direction | Rationale |
|----------|-----------|-----------|
| `msgget` | `bidirectional` | Creates or opens queue; role unclear |
| `msgsnd` | `provider` | Explicitly sends message |
| `msgrcv` | `consumer` | Explicitly receives message |
| `msgctl` | `bidirectional` | Control ops; no directional semantics |

Unlike shared memory (where direction is fundamentally unclear), message queues
have explicit send/receive APIs with clear directionality. The `provider` role
indicates message production; `consumer` indicates message consumption.

## Evidence Payload

Minimum evidence per surface:

```json
{
  "function": "msgget",
  "api_family": "sysv_msgq",
  "key": "0x5678",           // if literal extractable
  "flags": "IPC_CREAT|0644", // if extractable
  "basis": "api_call"
}
```

For `msgsnd`/`msgrcv`, include `msgflg` if extractable (IPC_NOWAIT, MSG_NOERROR, etc.).
For `msgctl`, include `cmd` if extractable (IPC_RMID, IPC_STAT, etc.).

Evidence is best-effort extraction. Missing fields indicate non-literal or
complex expressions.

## Known Limits

1. **Runtime queue identity:** `msgsnd(msqid, ...)` uses a runtime `msqid`
   returned from `msgget`. The `msqid` is not a stable source-level identifier.

2. **Key indirection:** `msgget(key, ...)` key may be a variable, macro, or
   `ftok()` result. Only literal keys are extractable.

3. **No callsite correlation:** This slice does not link `msgget` -> `msgsnd` ->
   `msgrcv` sequences. Each callsite is a separate surface.

4. **No cross-file correlation:** Processes sharing a queue via the same key
   are not linked in this slice.

5. **No message type inference:** `msgtyp` parameter in `msgrcv` enables
   selective reception; not interpreted in this slice.

## Implementation

### Phase 1: Binding Table

Add to `rust/crates/boundary-interaction/bindings.toml`:

```toml
# ════════════════════════════════════════════════════════════════════════
# SYSV MESSAGE QUEUES (BI-LX-2)
# ════════════════════════════════════════════════════════════════════════
#
# Detection: msgget/msgsnd/msgrcv/msgctl API usage.
# Scope: always inter_process.
# Direction: msgsnd=provider, msgrcv=consumer, others=bidirectional.
# Interaction: fire_and_forget.
#
# Reuses channel_kind = message_queue with api_family = sysv_msgq.

[[binding]]
language = "c"
api_family = "sysv_msgq"
function = "msgget"
role = "bidirectional"
channel_kind = "message_queue"
scope_heuristic = "fixed"
fixed_scope = "inter_process"
interaction_pattern = "fire_and_forget"
basis = "api_call"
arg_index = 0
notes = "Create/open message queue. Arg0 is key, arg1 is flags."

[[binding]]
language = "c"
api_family = "sysv_msgq"
function = "msgsnd"
role = "provider"
channel_kind = "message_queue"
scope_heuristic = "fixed"
fixed_scope = "inter_process"
interaction_pattern = "fire_and_forget"
basis = "api_call"
notes = "Send message to queue. Arg0 is msqid (runtime value)."

[[binding]]
language = "c"
api_family = "sysv_msgq"
function = "msgrcv"
role = "consumer"
channel_kind = "message_queue"
scope_heuristic = "fixed"
fixed_scope = "inter_process"
interaction_pattern = "fire_and_forget"
basis = "api_call"
notes = "Receive message from queue. Arg0 is msqid, arg3 is msgtyp selector."

[[binding]]
language = "c"
api_family = "sysv_msgq"
function = "msgctl"
role = "bidirectional"
channel_kind = "message_queue"
scope_heuristic = "fixed"
fixed_scope = "inter_process"
interaction_pattern = "fire_and_forget"
basis = "api_call"
notes = "Control message queue. Arg0 is msqid, arg1 is cmd (IPC_RMID, IPC_STAT, etc.)."
```

### Phase 2: Extractor Support

Add `SYSV_MSGQ_FUNCTIONS` to `rust/crates/c-extractor/src/boundary_detector.rs`:

```rust
const SYSV_MSGQ_FUNCTIONS: &[&str] = &["msgget", "msgsnd", "msgrcv", "msgctl"];
```

Update `is_boundary_function()` to include these.

### Phase 3: Storage/Read Surface

Reuse existing `boundary_interaction_surfaces` and `boundary_channel_details`
tables. No schema changes required.

The `api_family` field in evidence payload distinguishes SysV from POSIX mq.

### Phase 4: CLI

Use existing `rmap boundaries list` with:
- `--kind message_queue` (includes both POSIX and SysV)
- `--family local_ipc`

No new command or alias required.

### Phase 5: Tests

#### Fixture

Create: `test/fixtures/sysv-message-queues/`

**sender.c:**
```c
#include <sys/ipc.h>
#include <sys/msg.h>
#include <string.h>

#define MSG_KEY 0x5678

struct message {
    long mtype;
    char mtext[256];
};

int main() {
    int msqid = msgget(MSG_KEY, IPC_CREAT | 0644);
    if (msqid < 0) return 1;

    struct message msg;
    msg.mtype = 1;
    strcpy(msg.mtext, "Hello from sender");

    if (msgsnd(msqid, &msg, sizeof(msg.mtext), 0) < 0) {
        return 1;
    }

    return 0;
}
```

**receiver.c:**
```c
#include <sys/ipc.h>
#include <sys/msg.h>
#include <stdio.h>

#define MSG_KEY 0x5678

struct message {
    long mtype;
    char mtext[256];
};

int main() {
    int msqid = msgget(MSG_KEY, 0);
    if (msqid < 0) return 1;

    struct message msg;
    if (msgrcv(msqid, &msg, sizeof(msg.mtext), 0, 0) < 0) {
        return 1;
    }

    printf("Received: %s\n", msg.mtext);
    return 0;
}
```

**cleanup.c:**
```c
#include <sys/ipc.h>
#include <sys/msg.h>
#include <stdio.h>

#define MSG_KEY 0x5678

int main() {
    int msqid = msgget(MSG_KEY, 0);
    if (msqid >= 0) {
        msgctl(msqid, IPC_RMID, NULL);
        printf("Message queue removed\n");
    }
    return 0;
}
```

#### Integration Tests

Add `rust/crates/repo-index/tests/bi_lx_2_sysv_msgq.rs`:

- Index fixture
- Query surfaces with `channel_kind = message_queue`
- Verify expected count (msgget x3, msgsnd x1, msgrcv x1, msgctl x1 = 6 surfaces)
- Verify `api_family = sysv_msgq` in evidence
- Verify `boundary_scope = inter_process`
- Verify `interaction_pattern = fire_and_forget`
- Verify direction: msgsnd=provider, msgrcv=consumer, others=bidirectional

## Validation Plan

### Fixture Validation

Run integration tests against `test/fixtures/sysv-message-queues/`.

### Smoke Validation

Per `docs/testing/rmap-test-protocol.md`:

1. **swupdate** - check for SysV msgq usage
2. **Linux kernel** - likely has SysV msgq examples in IPC subsystem
3. **PostgreSQL** - may use SysV message queues

Fixture-only is acceptable for initial merge if no convenient real-repo
examples are found. Real-repo smoke can follow.

## Claims

### This slice claims

- This code uses SysV message queue APIs
- This file/symbol is an anchor for message-queue IPC
- This is inter-process message-passing coordination
- This callsite sends (msgsnd) or receives (msgrcv) messages

### This slice does NOT claim

- These two processes definitely communicate
- This specific queue survives runtime
- This message type filtering is correct
- This is safe/correct synchronization
- This exact msqid links all these callsites

## Smoke Validation

### swupdate (2026-05-05)

**Result:** Known-zero. swupdate does not use SysV message queue APIs.

```bash
rmap boundaries list /private/tmp/repo-graph-tests/bi-lx-smoke/swupdate.db swupdate --kind message_queue
# count: 0
grep -rn "msgget\|msgsnd\|msgrcv" swupdate --include="*.c"
# (no matches)
```

**Interpretation:** No false positives generated. Repo simply doesn't use these APIs.

### Linux kernel (2026-05-05)

**Result:** 11 SysV message queue surfaces in 1 file, 15 POSIX mqueue surfaces in 3 files.

```bash
rmap boundaries list /private/tmp/repo-graph-tests/bi-lx-smoke/linux.db linux --kind message_queue
# count: 26 total (11 SysV msgq, 15 POSIX mq)
```

**Files with SysV msgq usage:**
- `tools/testing/selftests/ipc/msgque.c` (11 surfaces)

**Files with POSIX mqueue usage:**
- `tools/testing/selftests/mqueue/mq_open_tests.c`
- `tools/testing/selftests/mqueue/mq_perf_tests.c`
- `tools/testing/selftests/timers/mqueue-lat.c`

**Manual verification:** Line numbers match actual API calls.

**Properties verified:**
- `channel_kind = message_queue`
- `boundary_scope = inter_process`
- `interaction_pattern = fire_and_forget`
- `direction`: msgget/msgctl = bidirectional, msgsnd = provider, msgrcv = consumer
- `provenance = api:sysv_msgq:*`

**Caveats:** Kernel `ipc/msg.c` (syscall implementation) does not produce hits because
it *defines* msgget/etc, not *calls* them. This is correct behavior.

## Future Work

- **BI-LX-2B:** Queue identity correlation (link msgget -> msgsnd/msgrcv sequences)
- **BI-LX-2C:** Cross-file queue pairing via shared key
- **BI-LX-2D:** ftok() key resolution
- **BI-LX-2E:** Message type (mtype) analysis

These are depth refinements, not breadth. Return only if real navigation proves
the presence-only surface insufficient.
