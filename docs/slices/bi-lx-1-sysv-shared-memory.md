# BI-LX-1: SysV Shared Memory Detection

**Status:** SHIPPED  
**Slice:** BI-LX-1  
**Family:** Linux IPC  
**Language:** C (first)

## Problem

SysV shared memory is a fundamental Linux/Unix IPC mechanism missing from boundary
interaction coverage. Agents need to see where shared-memory coordination exists
in legacy C codebases. This is discovery-oriented: we surface the presence and
location of SysV shm usage, not runtime behavior or cross-process pairing.

SysV shared memory predates POSIX shm and remains common in:
- Legacy Unix systems
- Embedded Linux
- Database internals
- Graphics/multimedia pipelines
- IPC-heavy system software

## Scope

Detect these SysV shared memory APIs in C source:

| Function | Signature | Role |
|----------|-----------|------|
| `shmget` | `shmget(key_t key, size_t size, int shmflg)` | Segment creation/lookup |
| `shmat` | `shmat(int shmid, const void *shmaddr, int shmflg)` | Attach to segment |
| `shmdt` | `shmdt(const void *shmaddr)` | Detach from segment |
| `shmctl` | `shmctl(int shmid, int cmd, struct shmid_ds *buf)` | Control/remove/query |

**In scope:**
- C syntax-level detection
- Evidence extraction (key, size, flags/cmd where visible)
- Boundary surface emission per callsite

**Out of scope (deferred):**
- Segment identity correlation across callsites
- Ownership/lifecycle reconstruction
- Producer/consumer inference
- Permission semantics interpretation
- Runtime attach success verification
- Cross-process pairing
- Wrapper function tracking

## Surface Semantics

### Channel Kind Decision: Option A (reuse `shared_memory`)

**Decision:** Reuse existing `shared_memory` channel kind with `api_family = sysv_shm`.

**Rationale:**
- Preserves one user-facing concept for shared memory
- Simpler CLI/read surface (`--kind shared_memory` finds both POSIX and SysV)
- Lower taxonomy sprawl
- Better breadth-first fit
- Evidence payload carries the family distinction when needed

**Rejected alternative:** Separate `sysv_shared_memory` channel kind would create
premature taxonomy depth and weaker discovery ergonomics.

### Surface Properties

| Property | Value |
|----------|-------|
| `channel_kind` | `shared_memory` |
| `api_family` | `sysv_shm` |
| `protocol_family` | `local_ipc` |
| `boundary_scope` | `inter_process` (fixed) |
| `interaction_pattern` | `shared_state` |

### Direction Semantics

All SysV shm functions emit as `bidirectional`. Do not overclaim
provider/consumer roles:

| Function | Direction | Rationale |
|----------|-----------|-----------|
| `shmget` | `bidirectional` | Creates or opens segment; role unclear |
| `shmat` | `bidirectional` | Attaches for read/write; direction unclear |
| `shmdt` | `bidirectional` | Detaches; no directional semantics |
| `shmctl` | `bidirectional` | Control ops; no directional semantics |

Shared-state coordination is fundamentally bidirectional at the API level.
Provider/consumer inference would require dataflow analysis beyond this slice.

## Evidence Payload

Minimum evidence per surface:

```json
{
  "function": "shmget",
  "api_family": "sysv_shm",
  "key": "0x1234",           // if literal extractable
  "size": "4096",            // if literal extractable (shmget only)
  "flags": "IPC_CREAT|0644", // if extractable
  "basis": "api_call"
}
```

For `shmctl`, include `cmd` if extractable (IPC_RMID, IPC_STAT, etc.).

Evidence is best-effort extraction. Missing fields indicate non-literal or
complex expressions.

## Known Limits

1. **Runtime segment identity:** `shmat(shmid, ...)` uses a runtime `shmid`
   returned from `shmget`. The `shmid` is not a stable source-level identifier.

2. **Key indirection:** `shmget(key, ...)` key may be a variable, macro, or
   `ftok()` result. Only literal keys are extractable.

3. **No callsite correlation:** This slice does not link `shmget` → `shmat` →
   `shmdt` sequences. Each callsite is a separate surface.

4. **No cross-file correlation:** Processes sharing a segment via the same key
   are not linked in this slice.

5. **No permission inference:** `shmflg` permission bits are not interpreted.

## Implementation

### Phase 1: Binding Table

Add to `rust/crates/boundary-interaction/bindings.toml`:

```toml
# ════════════════════════════════════════════════════════════════════════
# SYSV SHARED MEMORY (BI-LX-1)
# ════════════════════════════════════════════════════════════════════════
#
# Detection: shmget/shmat/shmdt/shmctl API usage.
# Scope: always inter_process.
# Direction: bidirectional (shared-state, no provider/consumer inference).
# Interaction: shared_state.
#
# Reuses channel_kind = shared_memory with api_family = sysv_shm.

[[binding]]
language = "c"
api_family = "sysv_shm"
function = "shmget"
role = "bidirectional"
channel_kind = "shared_memory"
scope_heuristic = "fixed"
fixed_scope = "inter_process"
interaction_pattern = "shared_state"
basis = "api_call"
arg_index = 0
notes = "Create/open shared memory segment. Arg0 is key, arg1 is size, arg2 is flags."

[[binding]]
language = "c"
api_family = "sysv_shm"
function = "shmat"
role = "bidirectional"
channel_kind = "shared_memory"
scope_heuristic = "fixed"
fixed_scope = "inter_process"
interaction_pattern = "shared_state"
basis = "api_call"
notes = "Attach to shared memory segment. Arg0 is shmid (runtime value)."

[[binding]]
language = "c"
api_family = "sysv_shm"
function = "shmdt"
role = "bidirectional"
channel_kind = "shared_memory"
scope_heuristic = "fixed"
fixed_scope = "inter_process"
interaction_pattern = "shared_state"
basis = "api_call"
notes = "Detach from shared memory segment. Arg0 is shmaddr."

[[binding]]
language = "c"
api_family = "sysv_shm"
function = "shmctl"
role = "bidirectional"
channel_kind = "shared_memory"
scope_heuristic = "fixed"
fixed_scope = "inter_process"
interaction_pattern = "shared_state"
basis = "api_call"
notes = "Control shared memory segment. Arg0 is shmid, arg1 is cmd (IPC_RMID, IPC_STAT, etc.)."
```

### Phase 2: Extractor Support

The existing C boundary detector should handle these bindings declaratively.
No custom detector logic required if the binding table drives emission.

Verify the C extractor emits callsites for these function names and the
boundary-interaction-extractor matches them against bindings.

### Phase 3: Storage/Read Surface

Reuse existing `boundary_interaction_surfaces` and `boundary_channel_details`
tables. No schema changes required.

The `api_family` field in evidence payload distinguishes SysV from POSIX shm.

### Phase 4: CLI

Use existing `rmap boundaries list` with:
- `--kind shared_memory` (includes both POSIX and SysV)
- `--family local_ipc`

No new command or alias required.

### Phase 5: Tests

#### Fixture

Create: `test/fixtures/sysv-shared-memory/`

**creator.c:**
```c
#include <sys/ipc.h>
#include <sys/shm.h>

#define SHM_KEY 0x1234
#define SHM_SIZE 4096

int main() {
    int shmid = shmget(SHM_KEY, SHM_SIZE, IPC_CREAT | 0644);
    if (shmid < 0) return 1;

    char *data = shmat(shmid, NULL, 0);
    if (data == (char *)-1) return 1;

    // Write data...

    shmdt(data);
    return 0;
}
```

**worker.c:**
```c
#include <sys/ipc.h>
#include <sys/shm.h>

#define SHM_KEY 0x1234

int main() {
    int shmid = shmget(SHM_KEY, 0, 0);
    if (shmid < 0) return 1;

    char *data = shmat(shmid, NULL, SHM_RDONLY);
    if (data == (char *)-1) return 1;

    // Read data...

    shmdt(data);
    return 0;
}
```

**cleanup.c:**
```c
#include <sys/ipc.h>
#include <sys/shm.h>

#define SHM_KEY 0x1234

int main() {
    int shmid = shmget(SHM_KEY, 0, 0);
    if (shmid >= 0) {
        shmctl(shmid, IPC_RMID, NULL);
    }
    return 0;
}
```

#### Integration Tests

Add `rust/crates/repo-index/tests/bi_lx_1_sysv_shm.rs`:

- Index fixture
- Query surfaces with `channel_kind = shared_memory`
- Verify expected count (shmget x3, shmat x2, shmdt x2, shmctl x1 = 8 surfaces)
- Verify `api_family = sysv_shm` in evidence
- Verify `boundary_scope = inter_process`
- Verify `interaction_pattern = shared_state`

#### CLI Adapter Tests

Add tests to `rust/crates/rgr/tests/boundaries_command.rs`:

- `boundaries_list_sysv_shm_included_in_shared_memory_kind`
- `boundaries_list_sysv_shm_has_inter_process_scope`

## Validation Plan

### Fixture Validation

Run integration tests against `test/fixtures/sysv-shared-memory/`.

### Smoke Validation

Per `docs/testing/rmap-test-protocol.md`:

1. **swupdate** — check for SysV shm usage (may not exist)
2. **Linux kernel** — likely has SysV shm examples in IPC subsystem
3. **sqlite** — check for SysV shm usage

Fixture-only is acceptable for initial merge if no convenient real-repo
examples are found. Real-repo smoke can follow.

## Claims

### This slice claims

- This code uses SysV shared memory APIs
- This file/symbol is an anchor for shared-memory IPC
- This is inter-process shared-state coordination

### This slice does NOT claim

- These two processes definitely communicate
- This specific segment survives runtime
- This is producer vs consumer
- This is safe/correct synchronization
- This exact shmid links all these callsites

## Smoke Validation

### swupdate (2026-05-05)

**Result:** Known-zero. swupdate does not use SysV shared memory APIs.

```bash
rmap boundaries list /private/tmp/repo-graph-tests/bi-lx-smoke/swupdate.db swupdate --kind shared_memory
# count: 2 (both POSIX mmap/munmap, zero SysV)
grep -rn "shmget\|shmat" swupdate --include="*.c"
# (no matches)
```

**Interpretation:** No false positives generated. Repo simply doesn't use these APIs.

### Linux kernel (2026-05-05)

**Result:** 142 SysV shared memory surfaces across ~30 files.

```bash
rmap boundaries list /private/tmp/repo-graph-tests/bi-lx-smoke/linux.db linux --kind shared_memory
# count: 936 total shared_memory (142 SysV, 794 POSIX)
```

**Sample files with SysV shm usage:**
- `tools/testing/selftests/futex/functional/futex_wait.c`
- `tools/testing/selftests/futex/functional/futex_waitv.c`
- `tools/testing/selftests/mm/hugepage-shm.c`
- `tools/testing/selftests/powerpc/ptrace/*.c`
- `tools/testing/selftests/proc/setns-sysvipc.c`

**Manual verification:** Line numbers match actual `shmget`/`shmat`/`shmdt`/`shmctl` calls.

**Properties verified:**
- `channel_kind = shared_memory`
- `boundary_scope = inter_process`
- `interaction_pattern = shared_state`
- `direction = bidirectional`
- `provenance = api:sysv_shm:*`

**Caveats:** Kernel `ipc/shm.c` (syscall implementation) does not produce hits because
it *defines* shmget/etc, not *calls* them. This is correct behavior.

## Future Work

- **BI-LX-1B:** Segment identity correlation (link shmget → shmat → shmdt sequences)
- **BI-LX-1C:** Cross-file segment pairing via shared key
- **BI-LX-1D:** ftok() key resolution

These are depth refinements, not breadth. Return only if real navigation proves
the presence-only surface insufficient.
