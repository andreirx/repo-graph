# INDEX-QUIET-1 — Progress belongs to doctor, not the index invocation

Status: SPECIFIED (2026-07-10) · Track: Product-surface UX (operator-directed)
Ratified (operator, 2026-07-10): "I don't want progress shown inline — I want
rmap doctor to report current progress each time you call it."

## 1. Problem

DAEMON-VISIBILITY-1 gave `rmap index` inline stderr progress (phase lines,
throttled). Field verdict: unwanted — progress is a PULL concern (`rmap
doctor` already renders "indexing <repo>: <phase> N/M files, started X ago"
per call, shipped in v0.5.0). The inline stream clutters the invoking
terminal and duplicates the doctor surface.

## 2. Contract

1. **Default quiet:** `rmap index`/`refresh` print NO inline progress. One
   honest line at start ("indexing <repo> — follow progress with `rmap
   doctor`"), then the completion report (unchanged) or the still-running
   handoff (unchanged).
2. **Frames still consumed:** the client MUST keep consuming progress frames
   silently — they reset the stall deadline (DAEMON-VISIBILITY C2) and drive
   the still-running honesty. Display-only change; transport and timeout
   semantics untouched.
3. **Opt-in flag:** `--progress` restores the current inline rendering
   (unchanged behavior) for users who want it; help text says progress is
   on doctor by default.
4. Doctor's per-call progress line: already shipped — verify with a named
   test that phase + current/total render mid-index (exists per
   daemon_info.rs; cite or add).

## 3. Stop conditions

Display-only; no daemon/protocol changes; no timeout-semantics changes.
Do NOT commit.

## 4. Validation (SYNCHRONOUS; TEST REPORT INLINED)

- Cargo gates green, inlined.
- **Quiet proof (named test):** default index run emits no progress lines;
  the start line + completion report render; stall deadline still resets on
  frames (existing tests keep passing).
- **Opt-in proof (named test):** `--progress` renders as today.
- Isolated self-dogfood: index repo-graph with default quiet + `rmap doctor`
  mid-index showing phase/counters (transcript inlined).

## 5. Definition of done

An index invocation is quiet by default (one start line + the completion
report), `--progress` opts into inline rendering, doctor remains the live
progress surface — proven by the named tests + transcript.
