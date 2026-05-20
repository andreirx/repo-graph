# CLI-OUT-4: Module/Architecture Output

**Status:** COMPLETE (2026-05-20)  
**Type:** Product Surface / Implementation  
**Prerequisite:** CLI-OUT-3

## Problem Statement

Module and architecture commands currently dump raw JSON. Users need scannable
human output for architecture exploration queries.

## Scope

All read-side module and architecture commands. Write commands excluded.

### In Scope (11 commands)

**Module catalog/detail:**
- `rmap modules list`
- `rmap modules show <module>`

**Ownership inventory:**
- `rmap modules files <module>`
- `rmap modules unowned`

**Dependency/violation analysis:**
- `rmap modules deps [module] [--outbound|--inbound]`
- `rmap modules violations`

**Architectural surfaces:**
- `rmap surfaces list`
- `rmap surfaces show <ref>`

**Architectural boundaries:**
- `rmap boundaries list`
- `rmap boundaries show <uid>`
- `rmap boundaries summary`

### Excluded

- `rmap modules boundary` — legacy write command (not read-side query)

## Implementation Groups

Commands grouped by output role, not command family name. Implement in order.

### Group 1: Module Catalog/Detail

**Commands:** `modules list`, `modules show`

**Why first:** Establishes module identity contract. Later commands reuse
module header/summary formatting.

**Shared presentation:** `presentation/module_shared.rs`, `modules_list.rs`, `modules_show.rs`

### Group 2: Ownership Inventory

**Commands:** `modules files`, `modules unowned`

**Why second:** Inventory renderers naturally follow module identity contract.

**Shared presentation:** `presentation/module_inventory.rs`

### Group 3: Dependency/Violation Analysis

**Commands:** `modules deps`, `modules violations`

**Why third:** Relationship/diagnostic surfaces need more careful presentation.
Depend on module identity established in Group 1.

**Presentation:** `presentation/modules_deps.rs`, `presentation/modules_violations.rs`
(Split: different change axes — relationship reporting vs policy breach surface)

### Group 4: Architectural Surfaces

**Commands:** `surfaces list`, `surfaces show`

**Separate layer:** Not the same as modules. Own presentation logic.

**Presentation:** `presentation/surfaces.rs`

### Group 5: Architectural Boundaries

**Commands:** `boundaries list`, `boundaries show`, `boundaries summary`

**Separate layer:** Not the same as surfaces.

**Presentation:** `presentation/boundaries.rs`

## Structural Guardrail Note

**File:** `rust/crates/rgr/src/commands/boundaries.rs` — 472 lines

This file is approaching the 500-line guardrail. Adding `--json` parsing,
response parsing, and mode switching will push it over.

**Required:** Evaluate refactor-before-expansion when implementing Group 5.
Options:
1. Extract subcommands into separate files (like modules family)
2. Factor presentation logic into separate presentation module (preferred)

## Data Available (from daemon responses)

### modules list

```json
{
  "results": [
    {
      "module_uid": "inferred-mod-...",
      "module_key": "inferred:repo_...:src",
      "canonical_root_path": "src",
      "module_kind": "inferred",
      "display_name": "src",
      "confidence": 0.7,
      "owned_file_count": 646,
      "owned_test_file_count": 0,
      "outbound_dependency_count": 0,
      "inbound_dependency_count": 0,
      "violation_count": 0,
      "dead_symbol_count": 3270
    }
  ]
}
```

### modules show

```json
{
  "module": {
    "module_uid": "...",
    "canonical_root_path": "src",
    "module_kind": "inferred",
    "display_name": "src",
    "confidence": 0.7
  },
  "rollups": {
    "owned_file_count": 646,
    "outbound_dependency_count": 0,
    "inbound_dependency_count": 0,
    "violation_count": 0,
    "dead_symbol_count": 3270
  },
  "outbound_dependencies": [],
  "inbound_dependencies": [],
  "violations": [],
  "evidence": [...],
  "trust": {...}
}
```

### modules unowned

```json
{
  "results": [
    {
      "file_path": "deps/include/SDL/SDL.h",
      "language": "c",
      "reason": "excluded_directory:deps"
    }
  ]
}
```

## Proposed Human Output Formats

### modules list

Direct attention to module health, not raw counters.

```
Modules: OpenXcom

2 modules

  src       646 files   3270 dead symbols   0 violations   inferred (0.7)
  install     1 files      2 dead symbols   0 violations   inferred (0.7)

No cross-module dependencies detected.

hint: all imports are intra-module. Module boundaries may not be meaningful yet.
```

### modules show

```
Module: src

Kind: inferred (confidence 0.7)
Root: src/

Ownership:
  646 files (0 test files)

Relationships:
  0 outbound dependencies
  0 inbound dependencies
  0 violations

Symbols:
  3270 dead symbols (0 in tests)

Evidence:
  directory_heuristic  src/  basic

No dependencies detected. This module appears isolated.
```

### modules files

Full output, no truncation. Caller can pipe to `head`.

```
Files: src

646 files

  src/Basescape/BaseInfoState.cpp  cpp  manifest_prefix
  src/Basescape/BaseInfoState.h  c  manifest_prefix
  src/Basescape/BaseView.cpp  cpp  manifest_prefix
  src/Basescape/BaseView.h  c  manifest_prefix
  ... (full list continues)
```

### modules unowned

Full grouped output. Caller can pipe to `head`.

```
Unowned Files

143 files not assigned to any module

By reason:
  excluded_directory:deps  143 files

excluded_directory:deps:
  deps/include/SDL/SDL.h
  deps/include/SDL/SDL_active.h
  deps/include/SDL/SDL_audio.h
  deps/include/SDL/SDL_blendmode.h
  ... (full list continues for all 143 files)

hint: excluded directories are intentional. Check 'rmap modules list' for true gaps.
```

### modules deps

Direct attention to relationship health:

```
Module Dependencies

Queried: all directions

Summary:
  0 cross-module dependencies
  3775 intra-module imports
  143 imports from unowned sources

No cross-module dependencies exist.

hint: if this is unexpected, module boundaries may need refinement.
      Run 'rmap modules list' to see module coverage.
```

### modules violations

```
Module Violations

0 violations
0 stale declarations

Import analysis:
  3918 total import edges
  3775 intra-module (96%)
  0 cross-module
  143 from unowned sources (4%)

No boundary violations detected.
```

### surfaces list (empty case)

```
Surfaces

0 surfaces detected

hint: surfaces are extracted from code patterns (HTTP routes, CLI handlers, etc.).
      No recognized patterns found in this codebase.
```

### boundaries summary (empty case)

```
Boundaries Summary

0 surfaces
0 channels

No architectural boundaries detected.

hint: boundaries connect surfaces to resources. Without detected surfaces,
      no boundaries can be established.
```

## Design Principles

1. **Direct attention** — Don't just restate JSON keys. Tell the reader what matters.
2. **Empty is valid** — Many legacy codebases have no declared surfaces/boundaries.
3. **Hints guide action** — When results are empty or unexpected, suggest next steps.
4. **No silent clipping** — Full output, caller can pipe to `head`.
5. **No threshold verdicts** — Don't invent "healthy/unhealthy" labels in renderer.
6. **Group to show patterns** — e.g., unowned files grouped by reason.

## Definition of Done

### Group 1: Module Catalog/Detail — COMPLETE (2026-05-20)

**Files (refactored, all under 500-line guardrail):**
- [x] `presentation/module_shared.rs` — shared formatting helpers (109 lines)
- [x] `presentation/modules_list.rs` — list DTO + renderer (264 lines)
- [x] `presentation/modules_show.rs` — show DTO + renderer (454 lines)

**Functionality:**
- [x] `modules list` human renderer + `--json` flag
- [x] `modules show` human renderer + `--json` flag

**Proof surfaces:**
- [x] Unit tests: 26 tests (8 shared + 7 list + 11 show)
- [x] Daemon dispatch tests: 7 tests in `daemon_dispatch.rs`
- [x] CLI integration tests: 7 tests in `cli_out_4_modules.rs` (opt-in)
- [x] Corpus validation: OpenXcom, django, duckdb

**Test commands:**

Unit tests (26):
```
cargo test -p repo-graph-rgr --lib -- module_shared modules_list modules_show
```

Daemon dispatch tests (7):
```
cargo test -p repo-graph-rgr --test daemon_dispatch -- modules_list modules_show
```

CLI integration tests (7, opt-in):
```
cargo build -p rmapd
cargo test -p repo-graph-rgr --test cli_out_4_modules -- --ignored
```

### Group 2: Ownership Inventory — COMPLETE (2026-05-20)

**Files (all under 500-line guardrail):**
- [x] `presentation/module_inventory.rs` — inventory DTOs + renderers (422 lines)

**Functionality:**
- [x] `modules files` human renderer + `--json` flag
- [x] `modules unowned` human renderer + `--json` flag (deterministic sort)

**Proof surfaces:**
- [x] Unit tests: 14 tests in module_inventory.rs (includes determinism test)
- [x] Daemon dispatch tests: 5 tests (3 files + 2 unowned)
- [x] CLI integration tests: 5 tests (opt-in)

### Group 3: Dependency/Violation Analysis — COMPLETE (2026-05-20)

**Files (all under 500-line guardrail):**
- [x] `presentation/modules_deps.rs` — deps DTO + renderer (263 lines)
- [x] `presentation/modules_violations.rs` — violations DTO + renderer (319 lines)

Note: Split into separate files because `modules deps` (relationship reporting)
and `modules violations` (policy breach surface) have different change axes.
ImportDiagnostics (~10 lines) duplicated rather than creating a shared module.

**Functionality:**
- [x] `modules deps` human renderer + `--json` flag (deterministic sort)
- [x] `modules violations` human renderer + `--json` flag (deterministic sort)

**Proof surfaces:**
- [x] Unit tests: 15 tests (7 deps + 8 violations)
- [x] Daemon dispatch tests: 8 tests (5 deps + 3 violations)
- [x] CLI integration tests: 4 tests (opt-in)

### Group 4: Architectural Surfaces — COMPLETE (2026-05-20, empty-case corpus, populated-case fixture)

**Files:**
- [x] `presentation/surfaces.rs` — list + show DTOs + renderers (594 lines)
- [x] `commands/surfaces.rs` — --json flag + human mode (342 lines)

**500-line guardrail note:** `presentation/surfaces.rs` exceeds 500 lines (594 total).
Kept as single file: list/show share surface identity domain, same actor, same
degradation model, same terminology. Split not required unless change axes diverge.

**Functionality:**
- [x] `surfaces list` human renderer + `--json` flag (deterministic sort by kind, name, uid)
- [x] `surfaces show` human renderer + `--json` flag (deterministic evidence sort)
- [x] Degradation warning when surfaces not populated
- [x] Full output, no truncation

**Proof surfaces:**
- [x] Unit tests: 14 tests (7 list + 7 show)
- [x] CLI integration tests: 4 tests (list human, list json, list empty hint, show not found)
- [x] Review packet: `docs/audits/cli-out-4/group-4-surfaces-review.md`

**Corpus validation note:** All indexed repos (OpenXcom, django, duckdb) have 0
surfaces (C++/Python codebases, no TypeScript). Empty-case and error-path behavior
validated. Populated-case covered by unit tests with synthetic data only.

### Group 5: Architectural Boundaries — COMPLETE (2026-05-20, empty-case corpus, populated-case fixture)

**Command refactor (COMPLETE):**
- [x] `commands/boundaries/mod.rs` — dispatcher (75 lines)
- [x] `commands/boundaries/list.rs` — list handler (210 lines)
- [x] `commands/boundaries/show.rs` — show handler (135 lines)
- [x] `commands/boundaries/summary.rs` — summary handler (119 lines)
- [x] `commands/boundaries/links.rs` — preserved, out of scope (113 lines)

**Presentation modules (COMPLETE):**
- [x] `presentation/boundaries_list.rs` — list DTO + renderer (309 lines)
- [x] `presentation/boundaries_show.rs` — show DTO + renderer (338 lines)
- [x] `presentation/boundaries_summary.rs` — summary DTO + renderer (332 lines)

**Functionality:**
- [x] `boundaries list` human renderer + `--json` flag (deterministic sort)
- [x] `boundaries show` human renderer + `--json` flag (evidence sort)
- [x] `boundaries summary` human renderer + `--json` flag (count-desc sort)
- [x] Full output, no truncation

**Proof surfaces:**
- [x] Unit tests: 27 (8 list + 11 show + 8 summary)
- [x] Daemon dispatch tests: 10 (pre-existing)
- [x] CLI integration tests: 6 (list human/json/empty, summary human/json, show not-found)
- [x] Review packet: `docs/audits/cli-out-4/group-5-boundaries-review.md`

### Integration Tests
- [x] Group 1: `cli_out_4_modules.rs` (7 tests, opt-in)
- [x] Group 2: `cli_out_4_modules.rs` (5 tests, opt-in)
- [x] Group 3: `cli_out_4_modules.rs` (4 tests, opt-in)
- [x] Group 4: `cli_out_4_modules.rs` (4 tests, opt-in)
- [x] Group 5: `cli_out_4_modules.rs` (6 tests, opt-in)

### Validation
- [x] Group 4 review packet: `docs/audits/cli-out-4/group-4-surfaces-review.md`
- [x] Group 4 corpus validation: OpenXcom, django, duckdb (empty-case only; populated-case fixture-validated)
- [x] Group 5 review packet: `docs/audits/cli-out-4/group-5-boundaries-review.md`
- [x] Group 5 corpus validation: OpenXcom, django, duckdb (empty-case only; populated-case fixture-validated)

## Files in Scope

### CLI (presentation) — new files

**Group 1 (DELIVERED):**
- `presentation/module_shared.rs` — shared formatting helpers (109 lines)
- `presentation/modules_list.rs` — list DTO + renderer (264 lines)
- `presentation/modules_show.rs` — show DTO + renderer (454 lines)

**Group 2 (DELIVERED):**
- `presentation/module_inventory.rs` — files + unowned DTOs + renderers (422 lines)

**Group 3 (DELIVERED):**
- `presentation/modules_deps.rs` — deps DTO + renderer (263 lines)
- `presentation/modules_violations.rs` — violations DTO + renderer (319 lines)

**Group 4 (DELIVERED):**
- `presentation/surfaces.rs` — list + show DTOs + renderers (594 lines)

**Group 5 (DELIVERED):**
- `presentation/boundaries_list.rs` — list DTO + renderer (309 lines)
- `presentation/boundaries_show.rs` — show DTO + renderer (338 lines)
- `presentation/boundaries_summary.rs` — summary DTO + renderer (332 lines)

**Command refactor (DELIVERED):**
- `commands/boundaries/` directory replacing `commands/boundaries.rs`
- mod.rs (75), list.rs (210), show.rs (135), summary.rs (119), links.rs (113)

### CLI (commands) — updates
- `commands/modules/list.rs` (add --json)
- `commands/modules/show.rs` (add --json)
- `commands/modules/files.rs` (add --json)
- `commands/modules/unowned.rs` (add --json)
- `commands/modules/deps.rs` (add --json)
- `commands/modules/violations.rs` (add --json)
- `commands/surfaces.rs` (--json added, 342 lines)
- `commands/boundaries.rs` (add --json, likely needs refactor)

## Explicit Non-Goals

- Do not change daemon response structure
- Do not add new query capabilities
- Do not add colors/styling (future slice)
- Do not implement `modules boundary` (legacy write command)
