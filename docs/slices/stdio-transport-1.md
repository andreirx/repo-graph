# STDIO-TRANSPORT-1: Agent-Safe Stdio Subprocess Transport

**Status:** IMPLEMENTED (2026-05-26)

## Problem

Evidence from glamCRM in Codex shell (2026-05-26):

```
socket_file:    pass
socket_connect: fail - Operation not permitted (os error 1)
```

The Codex/app sandbox denies `connect()` syscall to Unix domain sockets.
This is a sandbox permission boundary, not a daemon health issue.

Socket transport cannot work from sandboxed agent shells.

## Existing Infrastructure

The daemon already supports stdio mode:

```bash
rmapd --stdio
```

This reads NDJSON from stdin, writes to stdout, uses the same dispatcher.
It works without Unix socket connection.

## Design

### 1. Transport Abstraction

```rust
pub trait Transport {
    fn request(&mut self, method: &str, params: Option<Value>) -> Result<Value, Error>;
    fn ping(&mut self) -> Result<(), Error>;
}
```

Implementations:
- `SocketTransport` - existing Unix socket connection
- `StdioTransport` - spawn `rmapd --stdio` subprocess

### 2. Transport Selection Policy

```
1. If RMAP_TRANSPORT=stdio → use StdioTransport unconditionally
2. If RMAP_TRANSPORT=socket → use SocketTransport (fail if unavailable)
3. Default (no override):
   a. Try SocketTransport
   b. If connect fails with EPERM/EACCES → fall back to StdioTransport
   c. If connect fails with other error → return socket error (no fallback)
```

**Critical constraint:** Fallback ONLY on permission-denied errors.

| Error | Fallback? | Reason |
|-------|-----------|--------|
| EPERM (errno 1) | YES | Sandbox denial |
| EACCES (errno 13) | YES | Permission denied |
| ECONNREFUSED (errno 61/111) | NO | Daemon not running - health issue |
| ETIMEDOUT | NO | Daemon wedged - health issue |
| Protocol/parse error | NO | Version mismatch - health issue |
| Socket missing | NO | Path resolution issue |

Do NOT make stdio a silent catch-all. Permission-denied is the only
justified auto-fallback trigger. Other failures indicate daemon/socket
problems that should surface as errors, not be masked by transport switch.

### 3. StdioTransport Implementation

```rust
pub struct StdioTransport {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl StdioTransport {
    pub fn spawn() -> Result<Self, Error> {
        let mut child = Command::new("rmapd")
            .arg("--stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        // ...
    }
}
```

### 4. Diagnostic Reporting

`rmap doctor` should report:
- `transport: socket` or `transport: stdio`
- If stdio fallback was triggered, show why

Error messages should indicate which transport was attempted.

## Deliverables

| Item | Description |
|------|-------------|
| Transport trait | Abstract interface for daemon communication |
| SocketTransport | Existing Unix socket transport (refactored) |
| StdioTransport | Subprocess stdio transport |
| Explicit override | `RMAP_TRANSPORT=stdio\|socket` env var |
| Bounded fallback | Auto-fallback ONLY on EPERM/EACCES |
| Doctor probe | Report which transport is in use |
| glamCRM validation | Verify stdio works from Codex shell |

## Out of Scope

- TCP transport (not needed for this use case)
- WebSocket transport (not needed)
- Named pipe transport (Windows, deferred)
- Persistent stdio connection pooling (v2 optimization)

## Definition of Done

1. `rmap check` works from glamCRM Codex shell
2. `rmap index .` works from glamCRM Codex shell
3. `rmap doctor` reports transport method used
4. Automatic fallback from socket to stdio on EPERM
5. No daemon restart required for stdio mode

## Why This Matters

Agent environments (Codex, sandboxed shells) are primary use cases.
If `rmap` cannot communicate with daemon state from agent context,
the tool is unusable for its core purpose: agent orientation.

## Architecture Constraints

### Why Bounded Fallback

The fallback must be bounded because different failures have different causes:

**Permission denied (EPERM/EACCES):**
- Caused by sandbox/container execution context
- Not fixable by daemon restart
- Stdio transport is the correct solution
- Auto-fallback is appropriate

**Connection refused (ECONNREFUSED):**
- Caused by daemon not running or crashed
- Fixable by daemon restart
- Masking with stdio hides the real problem
- Should fail with actionable diagnostic

**Timeout/protocol errors:**
- Caused by daemon bugs or version mismatch
- Stdio would have the same bugs
- Masking delays proper fix
- Should fail with diagnostic

If stdio becomes a catch-all fallback, real daemon issues get hidden.
Agents would silently spawn subprocesses when the daemon is broken,
and the broken daemon would never be diagnosed.

## Technical Notes

### Subprocess Lifetime

StdioTransport spawns a fresh `rmapd --stdio` per client instance.
This is acceptable because:
- Subprocess exits on stdin EOF (client drop)
- No long-lived connection state needed
- Each request is self-contained

### Performance

Stdio transport has higher latency than socket (process spawn overhead).
For agent use cases, this is acceptable. Agents are not latency-sensitive
in the same way interactive CLI is.

### State Consistency

Both transports use the same dispatcher and same storage.
The resident daemon and stdio subprocess read the same database.
No state synchronization issues.

## Related

- SOCKET-RENDEZVOUS-1: Path resolution (completed)
- DAEMON-SOCKET-HEALTH-1: Diagnostic improvements (completed)
- This slice addresses the actual transport failure mode

---

## Implementation Notes (2026-05-26)

### Files Created

| File | Lines | Purpose |
|------|-------|---------|
| `daemon_client/transport.rs` | 155 | Transport trait, TransportMode enum, permission detection |
| `daemon_client/socket_transport.rs` | 258 | Unix socket Transport implementation |
| `daemon_client/stdio_transport.rs` | 288 | Subprocess stdio Transport implementation |

### Files Modified

| File | Lines | Changes |
|------|-------|---------|
| `daemon_client/mod.rs` | 382 | DaemonClient refactored to use Transport abstraction |
| `platform/mod.rs` | +20 | Added transport probe to granular_socket_probes() |
| `commands/doctor.rs` | +1 | Include transport in service probes filter |

### All Files Under 500-Line Guardrail

```
connection.rs:      421
fallback.rs:        339
mod.rs:             382
reachability.rs:    136
socket_transport.rs: 258
stdio_transport.rs:  288
transport.rs:       155
```

### Test Results

- 442 tests passing
- `stdio_transport_ping` test verifies real subprocess communication
- Transport selection tests verify EPERM/EACCES detection

### Verification Commands

```bash
# Normal socket transport
rmap doctor
# transport: auto (active: socket)

# Explicit stdio transport
RMAP_TRANSPORT=stdio rmap check
# warning: --stdio mode is for debug/test only, not production
# (command succeeds)

# JSON output shows transport probe
rmap doctor --json | jq '.probes[] | select(.name == "transport")'
```

### glamCRM Validation

Pending test from glamCRM Codex shell:
```bash
rmap doctor  # Should show: transport: auto (active: stdio)
rmap check   # Should succeed
rmap index . # Should succeed
```

Expected: auto-fallback to stdio transport on EPERM, commands succeed.
