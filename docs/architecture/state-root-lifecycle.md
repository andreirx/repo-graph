# State Root Lifecycle Audit

## Status

AUDIT — documents current reality, identifies gaps vs desired architecture.

## Problem Statement

The sandbox-local state root (`/private/tmp/repo-graph-agent/<uid>/`) is currently a **full alternate state root**, not a clean Tier B cache. All tiers are duplicated there, including Tier A authority data (registry, potential declarations).

This creates lifecycle hazards:
1. Authority data created in sandbox mode is lost on reboot
2. No cleanup mechanism exists for stale sandbox state
3. The semantic distinction between "durable authority" and "rebuildable cache" is not enforced

## Current State Root Inventory

### Global Root: `~/Library/Application Support/repo-graph/`

| Path | Tier | Description | Lost if deleted? |
|------|------|-------------|------------------|
| `registry.json` | A | Repo registry (paths, aliases, UIDs) | Yes — manual reconstruction |
| `databases/*.db` | A + B | Per-repo SQLite DBs containing all tables | A: Yes, B: Reindex restores |
| `install-manifest.json` | — | Installation metadata | Reinstall restores |
| `daemon.sock` | — | Unix socket (runtime) | Daemon recreates |
| `sessions/` | — | Session tracking | Recreatable |

**Lifecycle:** Persistent until explicitly deleted. Unpruned.

### Sandbox Root: `/private/tmp/repo-graph-agent/<uid>/`

| Path | Tier | Description | Lost if deleted? |
|------|------|-------------|------------------|
| `registry.json` | A | Repo registry (paths, aliases, UIDs) | Yes — must reindex |
| `databases/*.db` | A + B | Per-repo SQLite DBs containing all tables | A: Yes, B: Reindex restores |

**Lifecycle:**
- Created on first sandbox stdio command
- Persists across stdio invocations for same user
- Shared across Codex sessions (same UID)
- Cleared on system reboot
- May be cleared by temp-cleanup scripts if old/unused
- **No automatic cleanup by rmap/rmapd**

## Per-Database Table Classification

Each `*.db` file contains all tiers:

### Tier A (Authority) — in every database

| Table | Data | Lost if deleted? |
|-------|------|------------------|
| `repos` | Repo metadata for this DB | Reregistration needed |
| `declarations` | User-authored boundaries, requirements, waivers | Yes — manual recreation |
| `quality_policies` | User-authored quality policies | Yes — manual recreation |
| `schema_migrations` | Migration tracking | Rerun migrations |
| `snapshots` (metadata) | Snapshot manifest | Reindex restores |

### Tier B (Derived Cache) — in every database

| Table | Data | Lost if deleted? |
|-------|------|------------------|
| `nodes` | Extracted symbols | Reindex restores |
| `edges` | Extracted relationships | Reindex restores |
| `files` | File metadata | Reindex restores |
| `file_versions` | Content hashes | Reindex restores |
| `measurements` | Complexity, coverage | Reindex restores |
| `inferences` | Liveness, ownership | Reindex restores |
| `module_candidates` | Discovered modules | Reindex restores |
| `module_file_ownership` | File-to-module mapping | Reindex restores |
| `boundary_*` | Boundary interaction facts | Reindex restores |
| `project_surfaces` | Framework detection | Reindex restores |
| `semantic_facts` | Doc-extracted hints | Reindex restores |
| ... | (all other snapshot-scoped tables) | Reindex restores |

## Current Lifecycle Hazards

### Hazard 1: Authority Data in Temp Root

If a user in sandbox mode:
1. Indexes a repo → creates registry entry + DB
2. Declares a boundary → writes to `declarations` table
3. Reboots machine → `/private/tmp/` cleared

**Result:** Declaration is lost. No warning, no recovery.

### Hazard 2: No Cleanup Mechanism

Sandbox root accumulates state:
- Multiple repos indexed across sessions
- Old registry entries for repos that no longer exist
- No pruning, no TTL

**Result:** Stale state persists until reboot.

### Hazard 3: Semantic Confusion

The architecture doc (STORAGE-ARCH-1) describes:
- Tier A: Durable, authoritative, survives cache loss
- Tier B: Rebuildable cache, retention-limited

But current implementation:
- Both tiers live in the same DB file
- Same DB file can exist in global OR sandbox root
- No code enforces which tier belongs where

## Desired End State

### Option A: Clean Tier Separation (Full Solution)

- Tier A (authority) lives ONLY in global root
- Tier B (cache) can live in either root
- Sandbox root is truly rebuildable cache only
- Declarations/policies written in sandbox mode either:
  - Fail with error ("cannot write authority data in sandbox mode")
  - Or redirect to global root (requires socket access)

**Complexity:** High. Requires DB split or cross-root coordination.

### Option B: Ephemeral Sandbox (Simpler)

- Sandbox root is cleared on daemon startup (socket mode)
- Sandbox root is cleared on each stdio subprocess start
- Forces reindex every sandbox session
- Authority data in sandbox is implicitly temporary

**Complexity:** Low. Single cleanup call on startup.

**Tradeoff:** Slower sandbox UX (always reindex), but semantically clean.

### Option C: Document and Warn (Minimum)

- Document current behavior clearly
- Warn user when authority data is written to sandbox root
- No automatic cleanup
- User responsibility to understand lifecycle

**Complexity:** Minimal. Just docs and warnings.

## Recommendation

**Immediate (this audit):** Option C — document reality, update architecture docs.

**Short-term (before CACHE-SEMANTICS-1):** Option B — add cleanup of sandbox root on launchd daemon startup. This ensures:
- Global daemon starting = authoritative state restored
- Stale sandbox state doesn't persist across daemon restarts
- Sandbox mode is clearly "temporary workspace"

**Long-term (CACHE-SEMANTICS-1+):** Consider Option A — clean tier separation.

## Implementation Status (2026-05-27)

**Option B implemented:** Sandbox cleanup on launchd daemon startup.

Location: `rust/crates/daemon-runtime/src/lib.rs` in `clear_stale_sandbox_state()`.

```rust
#[cfg(unix)]
fn clear_stale_sandbox_state() {
    let uid = unsafe { libc::geteuid() };
    let sandbox_root = PathBuf::from(format!("/private/tmp/repo-graph-agent/{}", uid));
    if sandbox_root.exists() {
        eprintln!("note: clearing stale sandbox state: {}", sandbox_root.display());
        let _ = std::fs::remove_dir_all(&sandbox_root);
    }
}
```

Called from `run_daemon()` at socket-mode startup, before bind.

**THIS IS TEMPORARY DEBT, NOT ARCHITECTURAL RESOLUTION.**

The cleanup is lifecycle coercion by deletion. It does not address:
- Tier A authority data still created in sandbox root during stdio sessions
- No warning when authority writes happen in sandbox mode
- Same DB schema in both roots (no tier separation at storage layer)
- User experience: every daemon restart forces reindex in subsequent sandbox sessions

The architectural goal remains Option A: clean tier separation where Tier A cannot be written to sandbox root. This cleanup is a stop-gap to prevent stale state accumulation until proper tier semantics are implemented.

## Proposed Cleanup Behavior

Add to `run_daemon()` (socket mode only):

```rust
// Clear stale sandbox state on launchd daemon startup
// Sandbox root is ephemeral; launchd daemon is authoritative
let sandbox_root = PathBuf::from(format!(
    "/private/tmp/repo-graph-agent/{}",
    unsafe { libc::geteuid() }
));
if sandbox_root.exists() {
    eprintln!("note: clearing stale sandbox state: {}", sandbox_root.display());
    let _ = std::fs::remove_dir_all(&sandbox_root);
}
```

**Why socket mode only:**
- Stdio mode IS sandbox mode — shouldn't clear itself
- Socket mode is the "real" daemon — can assert authority over temp state

## Files to Update

1. `agent_docs/storage-architecture-v2.md` — add lifecycle section
2. `docs/slices/perf-obs-1.md` — note comparison axis is global vs sandbox root
3. `rust/crates/daemon-runtime/src/lib.rs` — add sandbox cleanup on socket startup
4. `docs/slices/cache-semantics-1.md` — note dependency on lifecycle audit

## Open Questions

1. Should stdio subprocess startup also clear sandbox root? (Forces reindex every command — probably too aggressive)
2. Should we warn when `declare` commands run in sandbox mode?
3. Should sandbox mode be explicitly opt-in (`RMAP_SANDBOX=1`) rather than auto-fallback?
