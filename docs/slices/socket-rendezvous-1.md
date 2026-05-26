# SOCKET-RENDEZVOUS-1: Canonical Daemon Socket Path Resolution

**Status:** IMPLEMENTED (2026-05-26)

## Problem

The daemon socket path is derived from `dirs::data_dir()`, which reads `$HOME`.

When `$HOME` differs between execution contexts (sandboxed shells, agent environments,
container views), the client and daemon disagree on the socket location despite:
- Same user
- Same machine
- Same installed binaries

This breaks agent product use cases where tools like Codex run in modified environments.

## Root Cause

Using environment-derived home (`$HOME`) for daemon socket location conflates:
- **User preference directory**: session-local, may vary per app/container
- **System infrastructure rendezvous point**: per-user identity, must be stable

For IPC infrastructure, the socket path should derive from the **actual account home**
for the effective uid, not the session environment.

## Current State

Path resolution is duplicated:
- `rust/crates/rgr/src/cli/paths.rs` (client)
- `rust/crates/daemon-runtime/src/lib.rs` (daemon)

Both use `dirs::data_dir()` on macOS, which ultimately reads `$HOME`.

## Design

### 1. Shared Path Resolution Crate

Create `rust/crates/platform-paths/` with:
- `config_dir()` - user config location
- `data_dir()` - user data location
- `logs_dir()` - user logs location
- `databases_dir()` - repo databases location
- `daemon_socket_path()` - daemon socket location

Both `rgr` and `daemon-runtime` depend on this crate.

### 2. Canonical Home Lookup

On macOS, resolve the per-user base directory from the **actual account home**:

```rust
// Primary: OS-level home for effective uid
fn canonical_home() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        // getpwuid_r(geteuid()) equivalent
        let uid = unsafe { libc::geteuid() };
        let passwd = unsafe { libc::getpwuid(uid) };
        if !passwd.is_null() {
            let home = unsafe { std::ffi::CStr::from_ptr((*passwd).pw_dir) };
            return home.to_str().ok().map(PathBuf::from);
        }
        None
    }
    #[cfg(not(unix))]
    {
        dirs::home_dir()
    }
}
```

This returns `/Users/apple` regardless of what `$HOME` is set to in the environment.

### 3. Resolution Order

```
daemon_socket_path():
1. RMAP_SOCKET_PATH env var (explicit override)
2. Canonical home → ~/Library/Application Support/repo-graph/daemon.sock
3. (migration) Fallback probe of dirs::data_dir() path if canonical path missing
```

The fallback probe is temporary for migration. It allows existing installs where
the daemon started with old logic to still be found by new clients.

### 4. Migration Behavior

During transition:
- Compute canonical socket path
- If not found, probe legacy env-derived path
- If legacy exists but canonical does not: connect to legacy, emit warning
- If both exist: prefer canonical, emit warning about stale legacy socket

After migration period (1-2 releases): remove fallback, canonical only.

### 5. Diagnostic Improvements

`rmap doctor` output additions:
```
Socket Resolution:
  [ok] effective_uid: 501
  [ok] HOME: /Users/apple
  [ok] canonical_home: /Users/apple
  [ok] computed_socket: /Users/apple/Library/Application Support/repo-graph/daemon.sock
  [ok] socket_exists: true
  [ok] socket_connected: true
  [--] override_active: false
```

Daemon-unavailable errors include:
- Effective uid
- `$HOME` value
- Canonical home used
- Computed socket path
- Whether socket exists vs connection failed
- Whether override is active

## Deliverables

| Item | Description |
|------|-------------|
| `platform-paths` crate | Shared path resolution with canonical home lookup |
| `rgr` migration | Remove `cli/paths.rs`, use `platform-paths` |
| `daemon-runtime` migration | Remove inline `daemon_socket_path()`, use `platform-paths` |
| Fallback probing | Temporary dual-path probe during migration |
| Diagnostic expansion | `rmap doctor` and error messages show full resolution chain |
| Tests | Regression tests with altered `$HOME` |

## Tests

1. **Unit: canonical_home()** - returns actual passwd entry, not `$HOME`
2. **Unit: daemon_socket_path() with override** - `RMAP_SOCKET_PATH` wins
3. **Unit: daemon_socket_path() canonical** - uses canonical home when no override
4. **Integration: altered HOME** - set `HOME=/tmp/fake`, verify canonical path still resolves correctly
5. **Integration: fallback probe** - legacy socket exists, canonical missing, client finds it with warning

## Out of Scope

- Windows support (already deferred)
- Changing socket location (same paths, different resolution method)
- Multi-user daemon (still per-user socket)

## Definition of Done

1. Single source of truth for path resolution (`platform-paths` crate)
2. Socket path derived from canonical home, not `$HOME`
3. `rmap doctor` shows full resolution diagnostics
4. Daemon-unavailable errors are actionable with path details
5. Existing installs work via fallback probe
6. Tests pass with altered `$HOME`

## Why This Matters

For an agent product, execution contexts are unpredictable:
- Codex sandboxes
- Claude Code hooks
- Container-based CI
- IDE extensions with modified environments

The daemon socket is infrastructure. It must be findable regardless of session
environment. Using `$HOME` for rendezvous was a shortcut that breaks in agent contexts.

## Verification

**EXECUTED:**
```bash
rmap doctor --json | jq '.probes[] | select(.name | startswith("socket") or .name == "canonical_home")'
```

**OBSERVED:**
```json
{
  "name": "canonical_home",
  "passed": true,
  "message": "/Users/apple"
}
{
  "name": "socket_path",
  "passed": true,
  "message": "/Users/apple/Library/Application Support/repo-graph/daemon.sock (exists)"
}
{
  "name": "socket_resolution",
  "passed": true,
  "message": "canonical (passwd home)"
}
```

**Tests:** 11 unit tests in `platform-paths` crate, all passing.
