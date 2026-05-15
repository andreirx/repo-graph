# RMAPD-2: Unix Socket Transport

**Status:** PLANNING  
**Priority:** BLOCKING (blocks MAC-1, LINUX-1 validation)  
**Type:** Support Module  

## Problem Statement

`rmapd` is currently implemented as a stdio NDJSON server that exits on stdin EOF.
The installer and service model treat it as a resident background daemon.
These are incompatible runtime contracts.

Linux validation (v0.1.2) exposed this:
- systemd starts `rmapd`
- stdin is empty/EOF
- `rmapd` exits immediately with status 0
- health check fails
- log is empty

The daemon model is correct. The transport is wrong.

## Architectural Decisions (Locked)

| ID | Decision | Choice |
|----|----------|--------|
| D1 | Transport | Unix domain socket + NDJSON |
| D2 | Lifetime | Always-on resident service |
| D3 | Stdio mode | Keep as `--stdio` flag, test/debug only |
| D4 | Fallback | Read-only degraded mode only |

## D4 Elaboration: Read-Only Fallback

If daemon is unavailable, CLI may perform **read-only** operations directly.

### Explicitly Allowed in Fallback

| Command | Rationale |
|---------|-----------|
| `rmap --version` | Static binary metadata |
| `rmap --help` | Static help text |
| `rmap doctor` (partial) | File/path checks only; daemon probes report "unavailable" |
| `rmap repo list` | Read-only DB query, no coordination needed |
| `rmap graph <query>` | Read-only DB query, no coordination needed |
| `rmap boundaries list` | Read-only DB query |
| `rmap inferences list` | Read-only DB query |
| `rmap resource list` | Read-only DB query |
| Config file inspection | Static file read |

### Explicitly Not Allowed in Fallback (Require Daemon)

| Command | Rationale |
|---------|-----------|
| `rmap repo add` | DB mutation |
| `rmap repo remove` | DB mutation |
| `rmap refresh` | Coordination, DB mutation |
| `rmap index` | Coordination, DB mutation |
| `rmap hook *` | Daemon state dependency |
| Any write to DB | Daemon owns write coordination |
| Any operation depending on daemon in-memory state | Truth requires daemon |

### Fallback Behavior Contract

1. CLI attempts socket connection (2s timeout)
2. If connection fails:
   - For allowed operations: proceed with direct DB read
   - For disallowed operations: fail with actionable error
3. Actionable error must include:
   - What operation was attempted
   - Why it requires daemon
   - How to start daemon (`systemctl --user start rmapd` or `launchctl ...`)

### Doctor Behavior in Fallback

`rmap doctor` operates in degraded mode:
- File existence checks: execute normally
- Path validation: execute normally
- DB schema checks: execute normally (read-only)
- Daemon socket check: report "daemon unavailable" (not failure)
- Daemon ping check: skip, report "skipped (daemon unavailable)"

Doctor does not fail in fallback. It reports what it can verify.

## Socket Path Contract

Single source of truth: `rust/crates/rgr/src/cli/paths.rs`

| Platform | Socket Path |
|----------|-------------|
| Linux | `~/.local/share/rmap/daemon.sock` |
| macOS | `~/Library/Application Support/repo-graph/daemon.sock` |

All consumers (daemon, CLI, doctor, installer) read from `paths.rs`.

## Daemon Runtime Modes

```
rmapd              # default: bind socket, stay alive, accept connections
rmapd --stdio      # debug/test: stdin/stdout NDJSON, exit on EOF
```

### Default Mode (Socket)

1. Daemon starts
2. Check socket path:
   - If exists, attempt connection
   - If connection succeeds: another instance running, exit with error
   - If connection fails: stale socket, remove it
3. Bind Unix socket at path
4. Listen for connections
5. Per connection: NDJSON request/response loop
6. Stay alive with no clients attached
7. On SIGTERM/SIGINT: clean shutdown, remove socket

### Stdio Mode (`--stdio`)

1. Explicit `--stdio` flag required
2. Read NDJSON from stdin
3. Write NDJSON to stdout
4. Exit on stdin EOF
5. **Never used by services**
6. **Never used by CLI client in production**
7. Documented as "debug/test only"
8. Help text should mark as debug mode

## Stale Socket Handling

Correct sequence on daemon startup:

```
if socket_path.exists():
    try:
        connect(socket_path)
        # success = another daemon is alive
        exit_error("daemon already running")
    except connection_refused:
        # stale socket from crashed daemon
        remove(socket_path)

bind(socket_path)
listen()
```

**Do not** blindly delete socket path without connection test.

## Shutdown Cleanup

On clean shutdown (SIGTERM, SIGINT, or programmatic stop):

1. Stop accepting new connections
2. Drain or terminate active connections
3. Remove socket file
4. Exit 0

On crash/kill -9:
- Socket file may remain (stale)
- Next startup handles via stale socket logic

## Client Connection Behavior

### CLI Client

```
try:
    connect(socket_path, timeout=2s)
    send_request()
    receive_response()
except connection_failed:
    if operation.is_read_only():
        fallback_direct_mode()
    else:
        error("daemon unavailable, start with: systemctl --user start rmapd")
```

### Timeout and Retry

- Connection timeout: 2 seconds
- No automatic retry on connection failure
- No automatic daemon start (explicit user action required)

## Health Check Contract

Health check must verify full stack, not just process existence.

| Check | Method |
|-------|--------|
| Process alive | PID file or service query |
| Socket exists | `stat(socket_path)` |
| Socket accepts | `connect(socket_path)` succeeds |
| Daemon responds | `{"method":"ping"}` returns success |

All four must pass for daemon to be considered healthy.

### Doctor Output

```
[ok] daemon process: running (pid 12345)
[ok] socket file: exists
[ok] socket connection: accepted
[ok] daemon ping: responded in 2ms
```

Or degraded:

```
[ok] daemon process: running (pid 12345)
[ok] socket file: exists
[FAIL] socket connection: refused
```

## Protocol

Existing NDJSON protocol preserved. Transport changes, framing does not.

### Request

```json
{"id":"uuid","method":"ping","params":{}}
```

### Response

```json
{"id":"uuid","result":{"status":"ok"}}
```

### Progress (optional)

```json
{"id":"uuid","progress":{"message":"indexing...","percent":45}}
```

One JSON object per line. Same as current stdio protocol.

## File Impact

| File | Change |
|------|--------|
| `rust/crates/rgr/src/cli/paths.rs` | Add `daemon_socket_path()` |
| `rust/crates/daemon-transport/src/lib.rs` | Add socket module |
| `rust/crates/daemon-transport/src/socket.rs` | NEW: socket transport |
| `rust/crates/daemon-transport/src/stdio.rs` | Retained for `--stdio` |
| `rust/crates/rmapd/src/main.rs` | Socket default, `--stdio` flag |
| `rust/crates/rgr/src/client.rs` | NEW or modified: daemon client |
| `scripts/lib/linux.sh` | Health check update |
| `scripts/lib/macos.sh` | Health check update |

## Impact on Platform Slices

### LINUX-1

Must update:
- Health check: socket ping, not just process alive
- Doctor: socket status reporting
- Service validation: socket reachable after start

### MAC-1

Same updates as LINUX-1.

Both slices are **blocked** until this support module is implemented.

## Definition of Done

1. [ ] `rmapd` default mode binds Unix socket and stays alive with no client
2. [ ] `rmapd --stdio` preserves current behavior, marked as debug/test
3. [ ] Socket path defined in `paths.rs`, used by all consumers
4. [ ] Stale socket detection and cleanup on startup
5. [ ] Clean socket removal on shutdown
6. [ ] `rmap` CLI connects via socket for all operations
7. [ ] `rmap` CLI falls back to direct mode for read-only operations only
8. [ ] `rmap` CLI fails with actionable error for daemon-required operations when unavailable
9. [ ] Health check validates: process + socket exists + connection + ping
10. [ ] Doctor reports socket status
11. [ ] NDJSON protocol works identically over socket
12. [ ] Unit tests for socket transport
13. [ ] Integration test: service start, socket connect, request, response

## Test Plan

### Unit Tests

- Socket bind/listen/accept
- Stale socket detection
- NDJSON framing over socket
- Shutdown cleanup

### Integration Tests

```bash
# Start daemon
rmapd &
sleep 1

# Verify socket
test -S ~/.local/share/rmap/daemon.sock

# Ping via CLI
rmap doctor  # should show socket healthy

# Kill daemon
kill %1

# Verify stale socket handled on restart
rmapd &
# should start without error

# Cleanup
kill %1
```

### Platform Validation (after this slice)

- Linux: systemd start, health check passes, socket reachable
- macOS: launchd start, health check passes, socket reachable

## Technical Debt

- Stdio mode retained for compatibility; consider deprecation timeline
- No socket activation support (systemd/launchd socket activation deferred)
- No automatic daemon start from CLI (explicit user action required)

## References

- Linux validation failure: v0.1.2 exposed transport/model mismatch
- Architecture discussion: daemon owns DB coordination, CLI is client
- Prior art: Unix socket IPC is standard for local daemons (Docker, systemd, etc.)
