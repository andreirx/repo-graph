# BI-LX-3: SysV and Named POSIX Semaphores Detection

**Status:** PLANNED  
**Slice:** BI-LX-3  
**Family:** Linux IPC  
**Language:** C (first)

## Problem

Semaphores are fundamental synchronization primitives for inter-process
coordination. Both SysV semaphores and named POSIX semaphores are used in
legacy C codebases, embedded systems, and system software for IPC. Agents need
to see where semaphore-based IPC exists.

This is discovery-oriented: we surface the presence and location of semaphore
usage, not runtime behavior or deadlock analysis.

## Scope

### SysV Semaphores

| Function | Signature | Role |
|----------|-----------|------|
| `semget` | `semget(key_t key, int nsems, int semflg)` | Create/open semaphore set |
| `semop` | `semop(int semid, struct sembuf *sops, size_t nsops)` | Perform semaphore operations |
| `semtimedop` | `semtimedop(int semid, struct sembuf *sops, size_t nsops, const struct timespec *timeout)` | Semaphore ops with timeout |
| `semctl` | `semctl(int semid, int semnum, int cmd, ...)` | Control semaphore set |

### Named POSIX Semaphores

| Function | Signature | Role |
|----------|-----------|------|
| `sem_open` | `sem_open(const char *name, int oflag, ...)` | Open/create named semaphore |
| `sem_close` | `sem_close(sem_t *sem)` | Close named semaphore |
| `sem_unlink` | `sem_unlink(const char *name)` | Remove named semaphore |

**In scope:**
- SysV semaphores (always IPC-capable)
- Named POSIX semaphores (always IPC-capable)
- C syntax-level detection
- Evidence extraction (name, key, flags where visible)
- Boundary surface emission per callsite

**Deferred:**
- Unnamed POSIX semaphore operations (`sem_init`, `sem_destroy`, `sem_wait`,
  `sem_trywait`, `sem_timedwait`, `sem_post`, `sem_getvalue`)

**Rationale for deferral:** Unnamed POSIX semaphores are ambiguous. With
`pshared=0`, they are thread-local synchronization, not IPC. With `pshared=1`,
they are IPC-capable. Without correlating `sem_wait`/`sem_post` back to their
initialization site or a named `sem_open`, we cannot distinguish thread
synchronization from IPC. Including them unconditionally would flood ordinary
threaded code with false IPC hints, violating the discovery contract.

**Future enablement:** Unnamed POSIX semaphore operations can be enabled when:
- Identity correlation links operations to `sem_open`, or
- `pshared` argument analysis confirms `sem_init(..., 1, ...)`

## Surface Semantics

### Channel Kind

**Decision:** Create new `semaphore` channel kind.

**Rationale:**
- Semaphores are semantically distinct from shared memory and message queues
- Different interaction pattern (acquire/release vs read/write vs send/receive)
- Separate channel kind enables targeted queries (`--kind semaphore`)
- `api_family` distinguishes SysV (`sysv_sem`) from POSIX (`posix_sem`)

### Surface Properties

| Property | Value |
|----------|-------|
| `channel_kind` | `semaphore` |
| `api_family` | `posix_sem` or `sysv_sem` |
| `protocol_family` | `local_ipc` |
| `boundary_scope` | `inter_process` |
| `interaction_pattern` | `synchronization` |

### Boundary Scope

All in-scope semaphore APIs are `inter_process`:
- SysV semaphores are always inter-process capable
- Named POSIX semaphores are always inter-process capable

### Direction Semantics

| Function | Direction | Rationale |
|----------|-----------|-----------|
| `sem_open` | `bidirectional` | Creates/opens; role unclear |
| `sem_close` | `bidirectional` | Cleanup; no direction |
| `sem_unlink` | `bidirectional` | Removal; no direction |
| `semget` | `bidirectional` | Creates/opens set |
| `semop` | `bidirectional` | Can increment or decrement |
| `semtimedop` | `bidirectional` | Can increment or decrement |
| `semctl` | `bidirectional` | Control operations |

Note: `semop` direction could be refined by analyzing `sembuf.sem_op` sign,
but this requires argument structure inspection (deferred).

## Evidence Payload

For named POSIX:
```json
{
  "function": "sem_open",
  "api_family": "posix_sem",
  "name": "/my_semaphore",     // if literal extractable
  "flags": "O_CREAT|O_EXCL",   // if extractable
  "basis": "api_call"
}
```

For SysV:
```json
{
  "function": "semget",
  "api_family": "sysv_sem",
  "key": "0x1234",             // if literal extractable
  "nsems": "3",                // if literal extractable
  "flags": "IPC_CREAT|0644",   // if extractable
  "basis": "api_call"
}
```

## Known Limits

1. **semop direction inference:** `semop` can increment or decrement based
   on `sembuf.sem_op` sign. Would require struct field analysis (deferred).

2. **No callsite correlation:** This slice does not link semget -> semop
   sequences.

3. **No cross-file correlation:** Processes sharing a named semaphore via
   the same name are not linked.

4. **No deadlock detection:** Lock ordering and potential deadlocks are
   out of scope.

5. **Unnamed POSIX deferred:** `sem_wait`, `sem_post`, etc. are not detected
   because they cannot be reliably classified as IPC without `pshared` analysis.

## Implementation

### Phase 1: Add Channel Kind

Add `Semaphore` variant to `ChannelKind` enum in
`rust/crates/boundary-interaction/src/types.rs`.

Add `Synchronization` variant to `InteractionPattern` enum if not present.

### Phase 2: Binding Table

Add to `rust/crates/boundary-interaction/bindings.toml`:

```toml
# ════════════════════════════════════════════════════════════════════════
# NAMED POSIX SEMAPHORES (BI-LX-3)
# ════════════════════════════════════════════════════════════════════════

[[binding]]
language = "c"
api_family = "posix_sem"
function = "sem_open"
role = "bidirectional"
channel_kind = "semaphore"
scope_heuristic = "fixed"
fixed_scope = "inter_process"
interaction_pattern = "synchronization"
basis = "api_call"
arg_index = 0
notes = "Open/create named semaphore. Arg0 is name (starts with /)."

[[binding]]
language = "c"
api_family = "posix_sem"
function = "sem_close"
role = "bidirectional"
channel_kind = "semaphore"
scope_heuristic = "fixed"
fixed_scope = "inter_process"
interaction_pattern = "synchronization"
basis = "api_call"
notes = "Close named semaphore."

[[binding]]
language = "c"
api_family = "posix_sem"
function = "sem_unlink"
role = "bidirectional"
channel_kind = "semaphore"
scope_heuristic = "fixed"
fixed_scope = "inter_process"
interaction_pattern = "synchronization"
basis = "api_call"
arg_index = 0
notes = "Remove named semaphore. Arg0 is name."

# ════════════════════════════════════════════════════════════════════════
# SYSV SEMAPHORES (BI-LX-3)
# ════════════════════════════════════════════════════════════════════════

[[binding]]
language = "c"
api_family = "sysv_sem"
function = "semget"
role = "bidirectional"
channel_kind = "semaphore"
scope_heuristic = "fixed"
fixed_scope = "inter_process"
interaction_pattern = "synchronization"
basis = "api_call"
arg_index = 0
notes = "Create/open SysV semaphore set. Arg0 is key."

[[binding]]
language = "c"
api_family = "sysv_sem"
function = "semop"
role = "bidirectional"
channel_kind = "semaphore"
scope_heuristic = "fixed"
fixed_scope = "inter_process"
interaction_pattern = "synchronization"
basis = "api_call"
notes = "Perform semaphore operations. Direction depends on sembuf.sem_op sign."

[[binding]]
language = "c"
api_family = "sysv_sem"
function = "semtimedop"
role = "bidirectional"
channel_kind = "semaphore"
scope_heuristic = "fixed"
fixed_scope = "inter_process"
interaction_pattern = "synchronization"
basis = "api_call"
notes = "Perform semaphore operations with timeout."

[[binding]]
language = "c"
api_family = "sysv_sem"
function = "semctl"
role = "bidirectional"
channel_kind = "semaphore"
scope_heuristic = "fixed"
fixed_scope = "inter_process"
interaction_pattern = "synchronization"
basis = "api_call"
notes = "Control SysV semaphore set. Arg1 is semnum, arg2 is cmd."
```

### Phase 3: Extractor Support

Add to `rust/crates/c-extractor/src/boundary_detector.rs`:

```rust
const POSIX_NAMED_SEM_FUNCTIONS: &[&str] = &[
    "sem_open", "sem_close", "sem_unlink",
];
const SYSV_SEM_FUNCTIONS: &[&str] = &["semget", "semop", "semtimedop", "semctl"];
```

Update `is_boundary_function()` to include these.

### Phase 4: Storage/Read Surface

Requires adding `Semaphore` to `ChannelKind` enum. Otherwise reuses existing
`boundary_interaction_surfaces` table.

### Phase 5: CLI

Use existing `rmap boundaries list` with:
- `--kind semaphore`
- `--family local_ipc`

No new command required.

### Phase 6: Tests

#### Fixture

Create: `test/fixtures/semaphores/`

**posix_named.c:**
```c
#include <semaphore.h>
#include <fcntl.h>

int main() {
    sem_t *sem = sem_open("/test_sem", O_CREAT, 0644, 1);
    // Operations would use sem_wait/sem_post but those are deferred
    sem_close(sem);
    sem_unlink("/test_sem");
    return 0;
}
```

**sysv_sem.c:**
```c
#include <sys/ipc.h>
#include <sys/sem.h>

int main() {
    int semid = semget(0x1234, 1, IPC_CREAT | 0644);
    struct sembuf sop = {0, -1, 0};  // wait
    semop(semid, &sop, 1);
    sop.sem_op = 1;  // signal
    semop(semid, &sop, 1);
    semctl(semid, 0, IPC_RMID);
    return 0;
}
```

#### Integration Tests

Add `rust/crates/repo-index/tests/bi_lx_3_semaphores.rs`

#### CLI Adapter Tests

Add tests to `rust/crates/rgr/tests/boundaries_command.rs`

## Validation Plan

### Validation Corpus

Per corpus guidance, semaphores are generic Linux IPC:

1. **Primary:** `../linux` — upstream kernel
2. **Targeted:** `linux/tools/testing/selftests/` — clean syscall usage
3. **Secondary:** Medium userspace C repo if kernel alone insufficient

### Expected Kernel Locations

Named POSIX semaphores:
- `tools/testing/selftests/` (various test harnesses)

SysV semaphores:
- `ipc/sem.c` — implementation (will NOT produce hits, defines not calls)
- `tools/testing/selftests/ipc/` — test code (WILL produce hits)

### Smoke Validation Sequence

1. **Fixture validation:** Run integration tests against fixture
2. **Linux kernel smoke:**
   ```bash
   ./scripts/smoke-rmap.sh bi-lx-3 ../linux boundaries list --kind semaphore
   ```
3. **Targeted inspection:** Verify hits in `tools/testing/selftests/`
4. **Manual spot-check:** Confirm line numbers match actual API calls

### Acceptance Criteria

- Fixture tests pass (integration + CLI adapter)
- Linux kernel produces nonzero semaphore hits
- Hits are in expected locations (selftests, not ipc/sem.c implementation)
- Properties verified: channel_kind, scope, direction, interaction_pattern
- No obvious false positives
- `smoke-runs/` artifacts produced

## Claims

### This slice claims

- This code uses SysV or named POSIX semaphore APIs
- This file/symbol is an anchor for semaphore-based IPC
- These semaphores are inter-process capable

### This slice does NOT claim

- These semaphores are correctly paired
- No deadlock exists
- The semop direction (requires sembuf analysis)
- Unnamed POSIX semaphore operations are IPC (deferred)

## Future Work

- **BI-LX-3B:** semop direction inference via sembuf.sem_op sign
- **BI-LX-3C:** Cross-file semaphore pairing via shared name/key
- **BI-LX-3D:** Unnamed POSIX semaphore correlation (pshared analysis)
