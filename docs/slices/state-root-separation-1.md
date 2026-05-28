# STATE-ROOT-SEPARATION-1: Authority vs Sandbox-Local State Boundaries

## Status

**COMPLETE** (2026-05-28)

Core enforcement implemented and validated via execution.

## Problem Statement

The storage architecture (STORAGE-ARCH-1) originally defined a single "Tier A: Durable Authority Store"
that lumped together:
- User-authored policy (declarations, baselines, aliases)
- Operational bookkeeping (repo registration, snapshot metadata)

This conflation creates a contradiction when enforcing sandbox-local boundaries:
- Blocking all "Tier A" writes in sandbox mode would break indexing
- Allowing all "Tier A" writes creates split-brain user authority

## Architecture Refinement

**Decision:** Split the original Tier A into two sub-classes with different sandbox behavior.

### A1: User Authority (Global-Only)

User-authored, policy-bearing state representing explicit human decisions.

| Data | Example |
|------|---------|
| Declarations | Boundaries, requirements, waivers, quality policies |
| Explicit baselines | `retention_class = 'baseline_user'` |
| Aliases | User-assigned repo nicknames |

**Sandbox behavior:** Writes BLOCKED with explicit error.

**Rationale:** These represent user intent that cannot be automatically recovered.
Silent loss on daemon restart would violate user expectations.

### A2: Operational Local State

System bookkeeping needed for daemon operation.

| Data | Example |
|------|---------|
| Repo registration | Path → database mapping in registry |
| Snapshot metadata | Manifest, status, timestamps, auto retention class |
| Schema migrations | Migration tracking |

**Sandbox behavior:** Writes ALLOWED.

**Rationale:** Required for index/refresh to function. Rebuilds automatically
when user re-runs index. Loss is inconvenient (reindex), not catastrophic
(lost user decisions).

### B: Derived Cache

Rebuildable extracted/inferred state.

| Data | Example |
|------|---------|
| Graph facts | nodes, edges, unresolved_edges |
| Measurements | complexity, coverage, churn |
| Inferences | framework detection, liveness |

**Sandbox behavior:** Writes ALLOWED.

## Policy

**Enforce global-only writes for A1 (User Authority).**
**Allow A2 (Operational Local State) and B (Derived Cache) in sandbox mode.**

This preserves the agent workflow (index → orient → check) in sandbox mode
while preventing silent loss of user-authored policy data.

## Scope

### Guarded Operations (A1 - blocked in sandbox)

| Handler | Location | What it writes |
|---------|----------|----------------|
| `handle_mark_baseline` | baseline.rs | `retention_class = 'baseline_user'` |
| `handle_unmark_baseline` | baseline.rs | Removes user baseline |
| `handle_repo_alias` | dispatch.rs | `registry.alias` field |
| Future: `handle_declare_*` | TBD | `declarations` table |

### Allowed Operations (A2 + B - allowed in sandbox)

| Handler | Location | What it writes |
|---------|----------|----------------|
| `handle_index` | dispatch.rs | Repo registration, snapshot, cache |
| `handle_refresh` | dispatch.rs | Snapshot, cache |
| `handle_repo_remove` | dispatch.rs | Removes from registry (cleanup) |
| All read handlers | various | Nothing |

### Infrastructure

1. **State-root mode classification**
   - `StateRootMode::Global` — normal operation
   - `StateRootMode::SandboxLocal` — sandbox fallback active

2. **Detection**
   - `DaemonState::state_root_mode()` — returns current mode
   - `DaemonState::is_sandbox_mode()` — convenience predicate

3. **Guard helper**
   - `require_global_mode(state, request, operation)` → `Result<(), DispatchResult>`
   - Returns explicit error with actionable message in sandbox mode

4. **Doctor visibility**
   - `state_root_mode: global | sandbox-local`
   - `authority_writes: allowed | blocked`

5. **Startup warning**
   - Stdio daemon emits warning when sandbox mode detected
   - Explains what is blocked and why

## Definition of Done

1. [x] `RepoRegistry::state_root()` accessor implemented
2. [x] `StateRootMode` enum: `Global`, `SandboxLocal`
3. [x] `DaemonState::state_root_mode()` implemented
4. [x] `DaemonState::is_sandbox_mode()` convenience method
5. [x] `require_global_mode_for_authority_write()` guard helper implemented
6. [x] `handle_mark_baseline` guarded
7. [x] `handle_unmark_baseline` guarded
8. [x] `handle_repo_alias` guarded
9. [x] Doctor reports `authority_policy` probe (baselines, aliases, declarations: allowed/blocked)
10. [x] Stdio daemon warns on sandbox startup
11. [x] Test: sandbox mode detected from state root path (4 unit tests, including real DaemonState construction)
12. [x] Test: A1 write (mark_baseline) blocked in sandbox mode with correct error — EXECUTED
13. [x] Test: A2 write (index registry) allowed in sandbox mode — EXECUTED
14. [x] Test: B write (index cache) allowed in sandbox mode — EXECUTED

**Validation notes:**
- A1 blocking: `mark_baseline_blocked_in_sandbox_mode`, `unmark_baseline_blocked_in_sandbox_mode`, `mark_baseline_allowed_in_global_mode`
- A2/B allowed: `index_allowed_in_sandbox_mode_proves_a2_and_b_writes` (macOS-only) — integration test that:
  - Creates sandbox state root under `/private/tmp/`
  - Indexes a real repo through `ServiceDispatcher`
  - Verifies registration (A2) and cache data (B) written successfully

**Platform scope:**
- Enforcement model (A1/A2/B classification, guard helper): platform-agnostic
- Sandbox detection (path-prefix heuristic): macOS-specific (`/private/tmp/`)
- Integration test: macOS-only due to detection mechanism
- Linux sandbox scenarios: not modeled (see TECH-DEBT.md)
15. [x] storage-architecture-v2.md updated with A1/A2 split
16. [x] state-root-lifecycle.md updated with implementation status

## Implementation Plan

### Phase 1: Detection Infrastructure

```rust
// rust/crates/daemon-runtime/src/state.rs

/// State root operation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateRootMode {
    /// Normal operation: global state root, all writes allowed.
    Global,
    /// Sandbox fallback: sandbox-local state root, A1 writes blocked.
    SandboxLocal,
}

impl DaemonState {
    /// Returns the current state root mode.
    pub fn state_root_mode(&self) -> StateRootMode {
        let state_root = self.registry.borrow().state_root();
        if state_root.starts_with("/private/tmp/") {
            StateRootMode::SandboxLocal
        } else {
            StateRootMode::Global
        }
    }
    
    /// Convenience: returns true if in sandbox-local mode.
    pub fn is_sandbox_mode(&self) -> bool {
        self.state_root_mode() == StateRootMode::SandboxLocal
    }
}
```

### Phase 2: Guard Helper

```rust
// rust/crates/daemon-runtime/src/lib.rs (or dispatch.rs)

/// Require global state root mode for A1 (user authority) writes.
///
/// Returns Ok(()) in global mode, or an error DispatchResult in sandbox mode.
pub fn require_global_mode(
    state: &DaemonState,
    request: &Request,
    operation: &str,
) -> Result<(), DispatchResult> {
    if state.is_sandbox_mode() {
        let state_root = state.registry().state_root().display().to_string();
        Err(DispatchResult::error(
            &request.id,
            ErrorDetail::new(
                ErrorCode::InvalidRequest,
                format!(
                    "cannot modify authority data in sandbox mode: {} \
                     (state root: {}, use socket daemon for durable writes)",
                    operation, state_root
                ),
            ),
        ))
    } else {
        Ok(())
    }
}
```

### Phase 3: Apply Guards

```rust
// rust/crates/daemon-runtime/src/handlers/inventory/baseline.rs

pub fn handle_mark_baseline(state: &DaemonState, request: &Request) -> DispatchResult {
    // A1 authority write guard
    if let Err(e) = require_global_mode(state, request, "mark_baseline") {
        return e;
    }
    
    // ... existing implementation
}

pub fn handle_unmark_baseline(state: &DaemonState, request: &Request) -> DispatchResult {
    // A1 authority write guard
    if let Err(e) = require_global_mode(state, request, "unmark_baseline") {
        return e;
    }
    
    // ... existing implementation
}
```

```rust
// rust/crates/daemon-runtime/src/dispatch.rs

fn handle_repo_alias(&self, request: &Request) -> DispatchResult {
    // A1 authority write guard
    if let Err(e) = require_global_mode(&self.state, request, "repo_alias") {
        return e;
    }
    
    // ... existing implementation
}
```

### Phase 4: Doctor and Startup Warning

Doctor response includes:
```json
{
  "state": {
    "state_root": "/Users/apple/Library/Application Support/repo-graph",
    "state_root_mode": "global",
    "authority_writes": "allowed"
  }
}
```

or in sandbox:
```json
{
  "state": {
    "state_root": "/private/tmp/repo-graph-agent/501",
    "state_root_mode": "sandbox-local",
    "authority_writes": "blocked"
  }
}
```

Startup warning in `run_daemon_stdio()`:
```rust
if state.is_sandbox_mode() {
    eprintln!("note: running in sandbox-local mode");
    eprintln!("note: authority writes (baselines, aliases, declarations) are blocked");
    eprintln!("note: cache operations (index, refresh, queries) are allowed");
}
```

## Error Message Design

When A1 write is blocked:

```
error: cannot modify authority data in sandbox mode

  operation:   mark_baseline
  state root:  /private/tmp/repo-graph-agent/501
  mode:        sandbox-local

Authority data (baselines, aliases, declarations) must be written via
the socket daemon. Sandbox mode is ephemeral and cleared on daemon restart.

To use the socket daemon:
  1. Ensure launchd daemon is running: rmap doctor
  2. Re-run command without sandbox restrictions
```

## Files in Scope

- `rust/crates/daemon-runtime/src/registry.rs` — add `state_root()` accessor
- `rust/crates/daemon-runtime/src/state.rs` — add `StateRootMode`, `state_root_mode()`, `is_sandbox_mode()`
- `rust/crates/daemon-runtime/src/lib.rs` — add `require_global_mode()`, startup warning
- `rust/crates/daemon-runtime/src/dispatch.rs` — guard `handle_repo_alias`, update doctor
- `rust/crates/daemon-runtime/src/handlers/inventory/baseline.rs` — guard both handlers
- `agent_docs/storage-architecture-v2.md` — A1/A2 split (DONE)
- `docs/architecture/state-root-lifecycle.md` — update with implementation

## Dependencies

- STDIO-STATE-ROOT-1 (COMPLETE) — sandbox state root infrastructure
- CACHE-SEMANTICS-1 (COMPLETE) — retention class model
- RETENTION-POLICY-1 (COMPLETE) — lifecycle enforcement

## Risk Assessment

**Low risk.** The enforcement is narrow and explicit:
- Only three handlers are guarded (current A1 write surfaces)
- Clear error messages explain the block
- A2 and B operations remain unaffected
- Guards are testable in isolation
- Model B preserves sandbox workflow (index → orient → check)
