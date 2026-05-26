# DAEMON-SOCKET-HEALTH-1: Daemon Socket Health and Stale Recovery

**Status:** PARTIAL (2026-05-26) — Diagnostics delivered, health fix pending

## Problem

Evidence from glamCRM and repo-graph contexts shows a failure class where:

1. launchd reports daemon running
2. Socket file exists at canonical path
3. Client connect fails
4. Daemon log shows "listening" but also intermittent "Broken pipe"

This is **not** a path resolution issue (SOCKET-RENDEZVOUS-1 addresses that).
This is a daemon lifecycle / health / stale-socket bug.

## Observed Symptoms

```
rmap doctor:
  daemon_service: running (pid: XXXX)
  daemon_socket: socket exists but not responding

daemon.log:
  daemon listening on .../daemon.sock
  [intermittent] Broken pipe
```

## Root Cause Hypothesis

The daemon process is alive but the socket accept loop is wedged or the socket
file is stale (left over from a crashed daemon). The current code does not
distinguish these failure modes or provide recovery.

## Design

### 1. Distinguish socket states in doctor

Current doctor output:
- `daemon_socket: socket not found` (path missing)
- `daemon_socket: socket exists but not responding` (connect fails)
- `daemon_socket: connected` (success)

Needed granularity:
- `socket_path: resolved` (path computation succeeded)
- `socket_file: exists` / `missing`
- `socket_connect: succeeded` / `failed`
- `socket_ping: succeeded` / `failed` / `timeout`

These are different failure layers.

### 2. Stale socket recovery on daemon startup

On daemon startup:
1. Check if socket file already exists
2. If exists, try to connect to it
3. If connect succeeds, another daemon is running — exit with error
4. If connect fails, socket is stale — remove it
5. Bind fresh socket
6. Verify accept loop is actually live before logging "listening"

### 3. Improved daemon unavailable errors

Current error:
```
Daemon unavailable for 'check'
Socket: /path/to/daemon.sock
```

Needed detail:
```
Daemon unavailable for 'check'

Socket path:    /path/to/daemon.sock
Socket exists:  yes
Connect:        failed (connection refused)
Resolution:     canonical (passwd home)

Possible causes:
- Daemon process crashed but socket file remains
- Daemon is starting up and not ready yet
- Socket permissions issue

To recover:
  launchctl bootout gui/$(id -u)/com.repo-graph.rmapd
  rm -f "/path/to/daemon.sock"
  launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.repo-graph.rmapd.plist
```

### 4. Optional: `rmap daemon restart` command

A convenience command that performs the bootout/bootstrap cycle.
Not strictly necessary if doctor provides actionable remediation steps.

## Deliverables

| Item | Description | Status |
|------|-------------|--------|
| Doctor granularity | Separate probes for path/file/connect/ping | COMPLETE |
| Stale socket cleanup | Daemon startup removes stale socket | PRE-EXISTING |
| Error message improvement | Actionable diagnostics in unavailable errors | COMPLETE |
| Recovery documentation | Clear steps in error messages | COMPLETE |
| Accept-loop hardening | Prevent daemon from wedging | NOT DELIVERED |
| Health watchdog | Automatic restart on failure | NOT DELIVERED |
| Self-recovery | Client-side automatic reconnection | NOT DELIVERED |

## Out of Scope

- Automatic daemon restart on failure (too aggressive for v1)
- Watchdog / health check daemon (separate infrastructure)
- Windows support (already deferred)

## Definition of Done

1. [x] `rmap doctor` shows distinct probes for file existence vs connectivity
2. [x] Daemon startup removes stale socket files (pre-existing in daemon-transport)
3. [x] Daemon unavailable errors include connect failure reason (with errno)
4. [x] Error messages provide actionable recovery steps (platform-specific)
5. [x] glamCRM-style failure is diagnosable and recoverable

## Why This Matters

For agent environments, daemon reliability is critical. An agent cannot
proceed if `rmap` commands fail. Currently, the failure mode is opaque:
"daemon unavailable" with no actionable path. The agent cannot self-recover
or even report the root cause accurately.

## Related

- SOCKET-RENDEZVOUS-1: Path resolution (different failure class)
- RMAPD-PERF-1: Query performance (not socket health)

---

## Implementation Notes (2026-05-26)

**This is a diagnostic improvement, not a health fix.**

The underlying glamCRM failure mode (daemon alive but not accepting connections)
may still exist. The new diagnostics enable identifying which layer is failing.

### Delivered Components

1. **SocketConnectResult enum** (`daemon_client/reachability.rs`, NEW file):
   - `SocketMissing`: file doesn't exist
   - `ConnectFailed { error: String, code: Option<i32> }`: connect failed with errno
   - `Connected`: daemon accepting connections

2. **Granular socket probes** (`platform/mod.rs::granular_socket_probes()`):
   - `socket_file`: Socket file existence check
   - `socket_connect`: TCP connect test with errno capture
   - `socket_ping`: Full daemon ping request via DaemonClient

3. **Improved error messages** (`daemon_client/fallback.rs::daemon_unavailable_message()`):
   - Shows socket path and existence status
   - Shows connect failure with errno (e.g., "Connection refused (os error 61)")
   - Shows resolution method (canonical vs legacy)
   - Lists possible causes based on failure mode
   - Provides platform-specific recovery commands (macOS launchctl / Linux systemctl)

### File Split (Guardrail Compliance)

`connection.rs` was 505 lines, exceeding 500-line guardrail.
Split into:
- `connection.rs` (421 lines): DaemonConnection, request/response handling
- `reachability.rs` (136 lines): SocketConnectResult, connectivity checks

### Pre-existing: Stale Socket Cleanup

The `daemon-transport` crate already implements stale socket handling in `bind_socket()`:
- If socket exists and connect succeeds → another daemon running, refuse to start
- If socket exists and connect fails → stale socket, remove and rebind

This was already complete; no changes needed.

### Verification

```bash
# Happy path
rmap doctor
# Shows: socket_file ok, socket_connect succeeded, socket_ping pong received

# Failure case (daemon stopped, socket stale)
launchctl bootout gui/$(id -u)/com.repo-graph.rmapd
rmap doctor
# Shows: socket_file ok, socket_connect failed (errno 61), socket_ping skipped

# Error message
rmap index
# Shows: full diagnostics with recovery steps
```

### What Is NOT Fixed

- Daemon accept-loop wedging (if that's the cause)
- Automatic daemon restart on failure
- Client-side automatic reconnection
- Any actual socket health bug — only the visibility into failure modes

### glamCRM Test Result (2026-05-26)

From Codex shell in glamCRM:
```
socket_file:    pass
socket_connect: fail - Operation not permitted (os error 1)
socket_ping:    skipped
```

**Diagnosis:** This is EPERM on `connect()` syscall. The Codex/app sandbox is denying
Unix socket connection from that execution context.

This is NOT:
- Socket path resolution (fixed by SOCKET-RENDEZVOUS-1)
- Stale socket or daemon health
- Version mismatch
- Daemon protocol issue

This IS:
- OS/sandbox permission boundary denial

**Implication:** Socket transport cannot work from sandboxed agent shells.
The fix requires an alternative transport that doesn't rely on Unix socket connect.

### Next Slice: STDIO-TRANSPORT-1

The correct fix is stdio subprocess transport fallback:
- `rmapd --stdio` mode already exists
- Spawn subprocess, communicate via stdin/stdout
- No Unix socket connect required
- Same protocol semantics

See `docs/slices/stdio-transport-1.md`.
