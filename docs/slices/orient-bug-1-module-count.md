# ORIENT-BUG-1: Module Count Mismatch

**Status:** QUEUED  
**Type:** Bug / Data Correctness  
**Priority:** After CLI-OUT-2B (does not block renderer work)  
**Discovered:** CLI-OUT-2A audit (2026-05-18)

## Problem Statement

`orient` reports dramatically wrong module counts compared to `trust`.

| Repo | orient Shows | trust Shows | Delta |
|------|-------------|-------------|-------|
| OpenXcom | 2 | 19 | -89% |
| DuckDB | 17 | 240+ | -93% |
| Django | 2 | 100+ | -98% |
| Buildroot | 5 | ~20 | -75% |
| grpc-java | 42 | 42+ | OK? |

grpc-java is the only repo where orient's count is plausible.

## Root Cause Hypothesis

Unknown. Likely candidates:
- orient uses a different module discovery query than trust
- orient filters modules by some criteria not applied in trust
- orient counts top-level modules only while trust counts all

## Investigation Required

1. Compare orient module query vs trust module enumeration
2. Identify filtering difference
3. Determine correct behavior (should they match? which is right?)

## Why This Is Not a Renderer Issue

The orient renderer displays whatever the daemon returns. If the daemon
returns "2 modules" when 19 exist, the bug is in the query/response,
not the renderer.

CLI-OUT-2B should not attempt to fix this. The renderer cannot invent
modules that aren't in the response.

## Definition of Done

- [ ] Root cause identified
- [ ] Decision: should orient and trust module counts match?
- [ ] If yes: fix query/response
- [ ] Smoke validation on audit corpus

## Files Likely in Scope

- `rust/crates/daemon-runtime/src/` (orient handler)
- `rust/crates/module-queries/src/` (module discovery)
- `rust/crates/storage/src/` (module schema)

## Files Out of Scope

- `rust/crates/rgr/src/presentation/` (renderer)
