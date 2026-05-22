# RMAP-IO-1: Client Transport Timeout Classification

**Status:** COMPLETE (2026-05-21)  
**Type:** Bug / Transport Layer  
**Priority:** P2  
**Discovered:** ORIENT-BUG-1 investigation (2026-05-21)

## Problem Statement

Client-to-daemon communication fails with cryptic error on socket timeout:

```
Error: failed to read response: Resource temporarily unavailable (os error 35)
```

macOS returns `EAGAIN` (error 35) when a socket read times out. The client maps all I/O errors to `ReadFailed`, losing the distinction between:
- Actual read failures (daemon crashed, socket closed)
- Timeouts (slow query, long operation)

## Root Cause

`connection.rs` line 193-195:

```rust
self.reader
    .read_line(&mut line)
    .map_err(|e| DaemonClientError::ReadFailed(e.to_string()))?;
```

This unconditionally maps `io::Error` to `ReadFailed`. On macOS with `set_read_timeout`:
- Timeout expiration returns `io::ErrorKind::WouldBlock` or `io::ErrorKind::TimedOut`
- Both manifest as os error 35 (EAGAIN)
- Should be classified as `Timeout`, not `ReadFailed`

## Scope

**In scope:**
- Add `Timeout` variant to `DaemonClientError`
- Classify `WouldBlock` and `TimedOut` as `Timeout` in `read_line` error handling
- Display appropriate message: "daemon response timed out after 300s"

**Out of scope:**
- Automatic retry (requires product decision on retry semantics)
- Heartbeat-based timeout detection (complex, separate slice)
- Adjustable timeout per-command (nice-to-have, not critical)

## Implementation Plan

### Phase 1: Error Classification (COMPLETE)
- [x] Identify error mapping location
- [x] Add `Timeout` variant to `DaemonClientError`
- [x] Map `WouldBlock` and `TimedOut` to `Timeout`
- [x] Format timeout message with duration

### Phase 2: Validation (COMPLETE)
- [x] Unit test: `timeout_error_displays_correctly`
- [x] Unit test: `timeout_error_is_distinct_from_read_failed`
- [x] Unit test: `classify_read_error_maps_would_block_to_timeout` — exercises actual mapping path
- [x] Unit test: `classify_read_error_maps_timed_out_to_timeout` — exercises actual mapping path
- [x] Unit test: `classify_read_error_maps_other_errors_to_read_failed` — verifies other errors remain ReadFailed

## Files to Modify

- `rust/crates/rgr/src/daemon_client/connection.rs`

## Definition of Done

- [x] Timeout errors display "daemon response timed out after 300s" (or configured timeout)
- [x] Read failures display "failed to read response: <actual error>"
- [x] Classification logic tested via `classify_read_error` helper with synthetic `io::Error` values
- [x] WouldBlock → Timeout (tested)
- [x] TimedOut → Timeout (tested)
- [x] Other errors → ReadFailed (tested)

## Notes

This is a classification-only fix. Retry logic (if needed) is a separate decision.
The current 300s timeout (RMAPD-PERF-1) is sufficient for most queries post-performance fixes.
