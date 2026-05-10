# BI-LX-4: memfd_create Detection

**Status:** SHIPPED  
**Slice:** BI-LX-4  
**Family:** Linux IPC  
**Language:** C (first)

## Problem

`memfd_create()` is a Linux-specific syscall for creating anonymous file-backed
shared memory. It enables IPC via file descriptor passing without filesystem
artifacts. Agents need to see where memfd-based shared memory exists in Linux
C codebases.

Common uses:
- Zero-copy IPC between processes
- Wayland compositors (buffer sharing)
- Container runtimes
- Sandbox isolation with memory sealing
- Graphics/multimedia pipelines

## Background

`memfd_create()` (Linux 3.17+) creates an anonymous file in RAM:

```c
int memfd_create(const char *name, unsigned int flags);
```

The returned file descriptor can be:
- Sized with `ftruncate()`
- Mapped with `mmap()`
- Passed to other processes via `sendmsg()` / `SCM_RIGHTS`
- Sealed with `fcntl()` / `F_ADD_SEALS` to prevent modifications

Unlike SysV or POSIX shm, memfd requires no namespace (no `/dev/shm/` path,
no IPC key). The fd itself is the identity.

## Scope

Detect `memfd_create` API in C source:

| Function | Signature | Role |
|----------|-----------|------|
| `memfd_create` | `memfd_create(const char *name, unsigned int flags)` | Creates anonymous memory fd |

**In scope:**
- C syntax-level detection of `memfd_create` calls
- Evidence extraction (name, flags where literal)
- Boundary surface emission per callsite

**Out of scope:**
- Subsequent `mmap()` / `ftruncate()` / `fcntl()` calls (not the boundary itself)
- Memory sealing analysis
- FD passing correlation (requires cross-process analysis)
- Producer/consumer inference
- Wrapper function tracking

## Surface Semantics

### Channel Kind

**Decision:** Reuse existing `shared_memory` channel kind with `api_family = memfd`.

**Rationale:**
- memfd is fundamentally shared memory, just with different identity mechanism
- `--kind shared_memory` should find all shared memory variants (SysV, POSIX, memfd)
- Evidence payload carries `api_family = memfd` for specific queries
- Consistent with BI-LX-1 (SysV shm) decision

### Surface Properties

| Property | Value |
|----------|-------|
| `channel_kind` | `shared_memory` |
| `api_family` | `memfd` |
| `protocol_family` | `shared_memory` |
| `boundary_scope` | `inter_process` (fixed) |
| `interaction_pattern` | `shared_state` |

### Direction Semantics

| Function | Direction | Rationale |
|----------|-----------|-----------|
| `memfd_create` | `bidirectional` | Creates shared state; role unclear |

The creator may be producer or consumer; determination requires dataflow analysis
beyond this slice.

## Evidence Payload

```json
{
  "function": "memfd_create",
  "api_family": "memfd",
  "name": "my_buffer",      // if literal extractable
  "flags": "MFD_CLOEXEC",   // if extractable
  "basis": "api_call"
}
```

## Known Limits

1. **Single function:** Only `memfd_create` is detected. The FD lifecycle
   (mmap, ftruncate, seal, pass) is not tracked.

2. **No pairing:** Cannot correlate creator with receiver. FD passing via
   `sendmsg()` is a separate mechanism.

3. **Wrapper functions:** Direct calls only. Wrappers like glib's
   `g_unix_fd_list_append()` are not detected.

4. **Flags interpretation:** `MFD_CLOEXEC`, `MFD_ALLOW_SEALING`, `MFD_HUGETLB`
   are not semantically interpreted.

## Implementation

### Phase 1: Binding

Add to `rust/crates/boundary-interaction/bindings.toml`:

```toml
# ════════════════════════════════════════════════════════════════════════
# MEMFD (BI-LX-4)
# ════════════════════════════════════════════════════════════════════════

[[binding]]
language = "c"
api_family = "memfd"
function = "memfd_create"
role = "bidirectional"
channel_kind = "shared_memory"
scope_heuristic = "fixed"
fixed_scope = "inter_process"
interaction_pattern = "shared_state"
basis = "api_call"
arg_index = 0
notes = "Creates anonymous file-backed shared memory. Arg0 is name."
```

### Phase 2: Extractor

Add to `rust/crates/c-extractor/src/boundary_detector.rs`:

```rust
const MEMFD_FUNCTIONS: &[&str] = &["memfd_create"];
```

### Phase 3: Tests

Create fixture and integration test verifying detection.

## Validation Plan

### Validation Corpus

1. **Primary:** Linux kernel (`mm/memfd.c`, `samples/`)
2. **Secondary:** Wayland compositors (weston, sway)
3. **Tertiary:** Container runtimes (runc, crun)

### Smoke Validation

```bash
./scripts/smoke-rmap.sh bi-lx-4 ../legacy-codebases/linux "boundaries list" --kind shared_memory
```

Filter for memfd:
```bash
# Grep provenance for memfd api_family
```

### Acceptance Criteria

- Fixture tests pass
- Linux kernel produces memfd hits (mm/memfd.c, samples/)
- Properties verified: channel_kind=shared_memory, scope=inter_process
- No obvious false positives
- smoke-runs/ artifacts produced (protocol v3)

## Claims

### This slice claims

- This code uses memfd_create to create anonymous shared memory
- This file is an anchor for memory-based IPC

### This slice does NOT claim

- The memfd is successfully shared with another process
- The memory is actually mapped or used
- The sealing state
- Producer vs consumer role

## Validation Results

**Date:** 2026-05-05

### Fixture Tests

5 integration tests pass (`rust/crates/repo-index/tests/bi_lx_4_memfd.rs`):
- `index_memfd_fixture_produces_shared_memory_surfaces`
- `memfd_surfaces_have_inter_process_scope`
- `memfd_surfaces_are_bidirectional`
- `memfd_surfaces_have_shared_state_pattern`
- `memfd_surfaces_have_memfd_provenance`

3 unit tests pass (`rust/crates/c-extractor/src/boundary_detector.rs`):
- `memfd_create_is_detected`
- `memfd_create_extracts_name`
- `memfd_create_with_variable_name_has_none`

### CLI Adapter Tests

5 CLI adapter tests pass (`rust/crates/rgr/tests/boundaries_command.rs`):
- `boundaries_list_memfd_included_in_shared_memory_kind`
- `boundaries_list_memfd_has_memfd_provenance`
- `boundaries_list_memfd_has_inter_process_scope`
- `boundaries_list_memfd_are_bidirectional`
- `boundaries_list_memfd_has_shared_state_pattern`

### Smoke Validation

**Corpus:** Linux kernel `tools/testing/selftests/mm/`

```
smoke-runs/2026-05-05T14-46-12Z/
├── 00-meta.json
├── 91-memfd-summary.json          # memfd-specific summary
├── 92-tool-latency.json
├── boundaries-list.json           # full shared_memory listing
└── boundaries-list-memfd-only.json # filtered memfd surfaces
```

**memfd-specific results** (from `91-memfd-summary.json`):
- 17 memfd_create surfaces with provenance `api:memfd:memfd_create`
- 13 unique source files across Linux kernel selftests/mm
- Properties verified: channel_kind=shared_memory, scope=inter_process, direction=bidirectional, pattern=shared_state

**Note:** The `selftests/memfd/` directory uses `sys_memfd_create()` wrapper (direct syscall) rather than libc `memfd_create()`, so no hits there. This is expected per known limits (wrapper functions not detected).
