# CLI-OUT-2C: Stats Renderer

**Status:** IMPLEMENTED  
**Type:** Product Surface / Implementation  
**Prerequisite:** RMAPD-PERF-1 stats query fixed

## Problem Statement

`rmap stats` currently dumps raw JSON. Users need human-readable architecture
health summary.

## Scope Constraint

**Primarily renderer work.** One daemon change: added `display_name` field to the
stats response envelope for human-readable repo identity (consistent with orient/trust).

## Data Available (from daemon response)

Per module:
- `module` — qualified path (e.g., `src/Engine`)
- `fan_in` — count of modules that import this module
- `fan_out` — count of modules this module imports
- `instability` — fan_out / (fan_in + fan_out), range [0, 1]
- `abstractness` — abstract_count / type_count, range [0, 1]
- `distance_from_main_sequence` — |abstractness + instability - 1|
- `file_count` — files owned by this module
- `symbol_count` — exported symbols in this module

Envelope:
- `repo_uid`, `snapshot_uid`, `count`

## Human Output Format (Implemented)

```
Module Stats: OpenXcom

Summary
  modules: 19
  total_files: 732
  total_symbols: 5177

By size
  src/Battlescape  files=99  symbols=739
  src/Mod  files=86  symbols=805
  ...

By fan-in
  src  fan_in=9  fan_out=2
  src/Engine  fan_in=9  fan_out=7
  ...

By fan-out
  src/Geoscape  fan_out=9  fan_in=4
  src/Menu  fan_out=9  fan_in=5
  ...

By distance from main sequence
  deps/include/yaml-cpp  D=1.00  I=0.00  A=0.00
  deps/include/yaml-cpp/node  D=1.00  I=0.00  A=0.00
  ...
```

## Design Rationale

1. **Bounded human tables, complete JSON** — MODULE-MODEL-2 §13 D7 (ratified
   2026-07-11) superseded the original "full sorted sections, no arbitrary top-N
   clipping": on a large monorepo these per-group tables run to thousands of rows,
   so each human table is now bounded to the top-N by its metric (file count for
   "By size"; fan-in / fan-out / distance for the others; lexicographic module-path
   tie-break) followed by an honest "… and N more …" omission line. The COMPLETE
   set always rides `stats --json`. Bounded human output, complete machine output.
2. **Ordering points the reader** — highest values first
3. **Compact rows** — caller can pipe to `head` or redirect to file
4. **No threshold-based labeling** — no "at risk" verdicts
5. **Metrics are Robert C. Martin's component metrics**:
   - I = instability = fan_out / (fan_in + fan_out)
   - A = abstractness = abstract_types / all_types
   - D = distance from main sequence = |A + I - 1|

## Definition of Done

- [x] `presentation/stats.rs` created with human renderer
- [x] `run_stats` updated to support `--json` flag
- [x] Human output is default, `--json` returns full envelope
- [x] Validated on corpus: OpenXcom, django, duckdb, grpc-java, buildroot
- [x] Tests for renderer (8 unit tests + 2 integration tests)

## Files in Scope

- `rust/crates/rgr/src/presentation/stats.rs` (create)
- `rust/crates/rgr/src/presentation/mod.rs` (add `pub mod stats`)
- `rust/crates/rgr/src/commands/graph.rs` (`run_stats` function)

## Explicit Non-Goals

- Do not add new metrics (coverage, complexity, etc.)
- Do not add module-level drilldown (that's CLI-OUT-4)
- Do not fix ORIENT-BUG-1 module count mismatch
- Do not add colors/styling (future slice)
