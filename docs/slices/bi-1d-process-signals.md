# BI-1D: Process Signal Detection

Status: PLANNED
Depends: BI-1A (foundation), TransportClass foundation
Track: A (Raw Transport)

## Objective

Detect POSIX signal sending and handling patterns across C, C++, Python, and
Rust codebases. Map signal-based IPC as boundary interaction surfaces with
appropriate scope and role classification.

## Strategic Value

Signals encode architecturally important control flow:
- Shutdown choreography (`SIGTERM`, `SIGINT`)
- Watchdog control (`SIGALRM`, `SIGUSR1`)
- Child-process supervision (`SIGCHLD`)
- Out-of-band failure handling (`SIGPIPE`, `SIGSEGV`)
- Mode switches and coordination

These are usually underdocumented and critical for understanding system behavior
during shutdown, restart, and error conditions.

## Scope

### In scope
- `kill()`, `killpg()`, `sigqueue()` sender detection
- `pthread_kill()`, `raise()` intra-process/thread signals
- `signal()`, `sigaction()` handler registration
- `sigwait()`, `sigwaitinfo()`, `sigtimedwait()` blocking signal receipt
- `signalfd()` fd-based signal handling (Linux)
- Signal constant extraction (`SIGTERM`, `SIGUSR1`, etc.)
- Python `signal.signal()`, `os.kill()` patterns
- Rust `libc::kill`, `signal-hook` patterns
- C++ signal wrappers and RAII handlers

### Out of scope
- Real-time signal semantics (future expansion)
- Signal mask manipulation analysis
- sigprocmask flow analysis
- Cross-process linking (weak static evidence)

## Boundary Classification

### Scope
- `boundary_scope = inter_process` for `kill()`, `killpg()`, `sigqueue()`
- `boundary_scope = intra_process` for `raise()`, `pthread_kill()` to self

### Transport class
- `transport_class = raw_ipc`

### Channel kind
Add new variant:
```rust
pub enum ChannelKind {
    // ... existing ...
    ProcessSignal,
}
```

### Direction
- `direction = provider` for signal senders (`kill`, `raise`)
- `direction = consumer` for signal handlers (`signal`, `sigaction`, `sigwait`)

### Interaction pattern
- `interaction_pattern = fire_and_forget` (default)
- Signals are inherently asynchronous, no response

## API Binding Table

### C/POSIX sender APIs
| Function | Signal Arg | Target | Notes |
|----------|-----------|--------|-------|
| `kill` | arg 1 | pid arg 0 | Standard send |
| `killpg` | arg 1 | pgrp arg 0 | Process group |
| `sigqueue` | arg 1 | pid arg 0 | With payload |
| `raise` | arg 0 | self | Intra-process |
| `pthread_kill` | arg 1 | thread arg 0 | Thread-directed |
| `tgkill` | arg 2 | tgid/tid args 0,1 | Linux specific |

### C/POSIX handler APIs
| Function | Signal Arg | Handler | Notes |
|----------|-----------|---------|-------|
| `signal` | arg 0 | arg 1 | Legacy |
| `sigaction` | arg 0 | struct arg 1 | Modern |
| `sigwait` | sigset arg 0 | blocking | Synchronous |
| `sigwaitinfo` | sigset arg 0 | blocking | With info |
| `sigtimedwait` | sigset arg 0 | blocking | With timeout |
| `signalfd` | sigset arg 1 | fd-based | Linux |

### Python APIs
| Function | Signal Arg | Notes |
|----------|-----------|-------|
| `signal.signal` | arg 0 | Handler registration |
| `os.kill` | arg 1 | Send signal |
| `os.killpg` | arg 1 | Process group |
| `signal.sigwait` | sigset arg 0 | Blocking wait |

### Rust patterns
| Pattern | Notes |
|---------|-------|
| `libc::kill` | Raw FFI |
| `libc::raise` | Raw FFI |
| `signal_hook::register` | Safe wrapper |
| `nix::sys::signal::kill` | nix crate |
| `tokio::signal` | Async signal handling |

## Channel Identity

Signal identity is the signal name/number:
- `SIGTERM`
- `SIGINT`
- `SIGUSR1`
- `SIGCHLD`
- Numeric fallback if not resolvable

Channel detail example:
```rust
ChannelDetail {
    channel_kind: ChannelKind::ProcessSignal,
    channel_identity: "SIGTERM".to_string(),
    // no socket_path, tcp_endpoint, etc.
    // all mechanism fields are None
    metadata_json: Some(r#"{"signal_num": 15}"#.to_string()),
}
```

## Evidence Structure

```json
{
  "binding_key": "posix:signal:kill",
  "api_family": "posix",
  "function_name": "kill",
  "signal_name": "SIGTERM",
  "signal_number": 15,
  "target_kind": "pid",
  "target_value": null,
  "direction": "provider",
  "notes": "Signal sender"
}
```

For handlers:
```json
{
  "binding_key": "posix:signal:sigaction",
  "api_family": "posix",
  "function_name": "sigaction",
  "signal_name": "SIGTERM",
  "signal_number": 15,
  "handler_kind": "function_pointer",
  "direction": "consumer",
  "notes": "Signal handler registration"
}
```

## Implementation Steps

1. **Add `ProcessSignal` to `ChannelKind`**
   - Update types.rs
   - Update `default_transport_class()` -> `RawIpc`
   - Update protocol mapping -> "signal"

2. **Create signal binding table entries**
   - Add to boundary_interactions.toml
   - C/POSIX entries
   - Python entries
   - Rust entries

3. **Implement signal constant extraction**
   - Map `SIGTERM` -> 15
   - Handle symbolic and numeric forms
   - Platform-aware signal numbers

4. **Extend emitter for signal context**
   - Add `signal_name` field to `BoundaryCallsite`
   - Add `signal_number` field
   - Validation for signal APIs

5. **Add C extractor signal detection**
   - Detect kill/signal/sigaction calls
   - Extract signal argument
   - Resolve symbolic constants

6. **Add Python extractor signal detection**
   - os.kill, signal.signal patterns

7. **Add CLI query support**
   - Filter by channel_kind = process_signal
   - Signal-specific formatting

## Test Matrix

1. `kill(pid, SIGTERM)` detection
2. `sigaction(SIGTERM, &act, NULL)` detection
3. `signal(SIGINT, handler)` detection
4. `raise(SIGUSR1)` as intra_process
5. `pthread_kill(thread, SIGUSR2)` detection
6. `sigwait(&set, &sig)` detection
7. Python `os.kill(pid, signal.SIGTERM)` detection
8. Python `signal.signal(signal.SIGINT, handler)` detection
9. Numeric signal argument fallback
10. Signal constant resolution from defines
11. Multiple signals in same function

## Validation Repos

- nginx (signal-based process control)
- redis (shutdown handling)
- systemd (extensive signal usage)
- Linux kernel (signal handling code)
- Any daemon with graceful shutdown

## Limitations

### Linking is weak
- Static analysis cannot reliably link signal sender to handler
- PID targets are often dynamic
- Process relationships are runtime

### Best for
- Mechanism inventory
- Shutdown surface discovery
- Handler registration mapping

### Not for
- Precise sender-receiver linking
- Dynamic PID analysis

## Deliverables

- `ChannelKind::ProcessSignal` variant
- Binding table entries for signal APIs
- Signal constant resolution
- C extractor signal detection
- Python extractor signal detection
- CLI filtering by signal kind
- 15+ unit tests
- 5+ integration tests on real repos

## Success Criteria

- All major signal APIs detected (C, Python, Rust)
- Signal names extracted when possible
- Correct scope classification (inter vs intra process)
- Correct direction classification (sender vs handler)
- Working CLI queries
- Validated on nginx, redis signal patterns
