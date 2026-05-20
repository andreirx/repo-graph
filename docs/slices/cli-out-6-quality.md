# CLI-OUT-6: Quality/Risk Output

**Status:** COMPLETE (2026-05-20)  
**Type:** Product Surface / Implementation  
**Prerequisite:** CLI-OUT-5

## Problem Statement

Quality and risk commands currently dump raw JSON. Users need scannable
human output for churn analysis, hotspot identification, risk assessment,
and coverage import results.

## Scope

All read-side quality/risk commands plus the coverage import command.

### In Scope (4 commands)

**Volatility/hotspot surfaces:**
- `rmap churn`
- `rmap hotspots`

**Risk analysis:**
- `rmap risk`

**Coverage import:**
- `rmap coverage`

### Excluded

- Any commands not listed above
- Changes to scoring algorithms
- Changes to response structure

## Legacy Contract Exception

**All four commands use legacy direct-storage contract.**

They require explicit `db_path` and `repo_uid` arguments, not the REG-1
daemon contract with cwd auto-discovery. This is preserved, not migrated.

Usage pattern: `rmap <command> <db_path> <repo_uid> [options]`

Same exception class as `policy` from CLI-OUT-5.

## Implementation Groups

Commands grouped by output semantics and change axis. Implement in order.

### Group 1: Volatility/Hotspots

**Commands:** `churn`, `hotspots`

**Why first:**
- Same time-window semantics (`--since` flag)
- Same ranking/inventory style (file-centric lists)
- Shared vocabulary: commit_count, lines_changed
- Likely shared sorting and rollup vocabulary

**Response shapes observed:**

`churn`:
```json
{
  "command": "churn",
  "repo": "repo_...",
  "snapshot": "...",
  "since": "90.days.ago",
  "results": [
    { "file_path": "src/foo.c", "commit_count": 12, "lines_changed": 450 }
  ],
  "count": N
}
```

`hotspots`:
```json
{
  "command": "hotspots",
  "repo": "repo_...",
  "snapshot": "...",
  "since": "90.days.ago",
  "formula": "lines_changed * sum_complexity",
  "filtering": {
    "exclude_tests": true,
    "exclude_vendored": false,
    "excluded_count": 5,
    "excluded_tests_count": 5,
    "excluded_vendored_count": 0
  },
  "results": [
    {
      "file_path": "src/foo.c",
      "commit_count": 12,
      "lines_changed": 450,
      "sum_complexity": 87,
      "hotspot_score": 39150
    }
  ],
  "count": N
}
```

**Presentation module decision point:**

Payloads share base vocabulary but hotspots extends significantly (complexity,
score, filtering). Two options:

1. **Single file `presentation/churn_hotspots.rs`**: Shared DTO base, two render
   functions. Risk: if hotspots filtering metadata grows, file balloons.

2. **Separate files**: `presentation/churn.rs`, `presentation/hotspots.rs`.
   Cleaner separation but more files.

**Decision deferred.** Do not commit to single vs split before seeing actual
DTO shapes and test volume. Evaluate after first renderer sketch.

### Group 2: Risk

**Commands:** `risk`

**Why second:**
- Derived/interpretive surface (hotspot × coverage gap)
- Should come after raw volatility/hotspot surfaces are rendered cleanly
- Different formula semantics and join metadata

**Response shape observed:**

```json
{
  "command": "risk",
  "repo": "repo_...",
  "snapshot": "...",
  "since": "90.days.ago",
  "formula": "hotspot_score * (1 - line_coverage)",
  "hotspot_files": 150,
  "coverage_files": 80,
  "joined_files": 75,
  "results": [
    {
      "file_path": "src/foo.c",
      "risk_score": 15660.0,
      "hotspot_score": 39150,
      "line_coverage": 0.6,
      "lines_changed": 450,
      "sum_complexity": 87
    }
  ],
  "count": N
}
```

**Presentation module:** `presentation/risk.rs`

**Caution:** Risk is the most likely place for invented verdict language.
Keep output evidence-bearing and rank-oriented, not policy-theatrical.

- Show: risk_score, contributing factors (hotspot_score, line_coverage)
- Show: join metadata (how many files had both hotspot and coverage data)
- DO NOT: add labels like "CRITICAL", "HIGH", "MEDIUM", "LOW"
- DO NOT: add pass/fail judgments
- DO: let the numbers speak, sorted by risk_score descending

### Group 3: Coverage

**Commands:** `coverage`

**Why last:**
- Different contract (write command, takes report_path)
- Report-input oriented (import operation, not query)
- More operational than discovery-oriented
- Match/unmatch diagnostics need careful rendering

**Response shape observed:**

```json
{
  "command": "coverage",
  "repo": "repo_...",
  "snapshot": "...",
  "imported_count": 45,
  "unnormalized_count": 3,
  "unmatched_indexed_count": 12,
  "unnormalized_paths_sample": ["../outside/foo.js"],
  "unmatched_indexed_paths_sample": ["src/old.js"],
  "results": [
    {
      "file_path": "src/foo.js",
      "line_coverage": 0.85,
      "covered_statements": 170,
      "total_statements": 200
    }
  ],
  "count": N
}
```

**Presentation module:** `presentation/coverage.rs`

Import summary should emphasize:
- How many files matched
- How many couldn't be normalized (paths outside repo)
- How many indexed files have no coverage data

**Backend-bounded diagnostics note:**

The `unnormalized_paths_sample` and `unmatched_indexed_paths_sample` fields
are already bounded by the backend (max 10 each). This is not renderer-side
clipping — the backend provides bounded diagnostic samples for debugging.

The renderer:
- Outputs full imported file rows (no clipping)
- Renders backend-provided sample paths as-is (already bounded)

This does not violate the no-clipping rule. The renderer does not truncate;
the backend contract defines bounded diagnostic fields for operational use.

## Structural Assessment

**Command file sizes (before implementation):**
- `commands/quality/churn.rs` — 212 lines
- `commands/quality/hotspots.rs` — 342 lines
- `commands/quality/risk.rs` — 296 lines
- `commands/quality/coverage_cmd.rs` — 239 lines

All under 500-line guardrail. No refactoring required before adding renderer logic.

## Proposed Human Output Formats

Examples show complete output for small result sets. Full output, no truncation.

### churn

```
File Churn (last 90 days)

3 files changed

  src/Engine/State/Game.cpp     45 commits   2,340 lines
  src/Battlescape/Map.cpp       38 commits   1,890 lines
  src/Menu/MainMenu.cpp         22 commits     650 lines
```

### hotspots

```
Hotspots (last 90 days)

Formula: lines_changed * sum_complexity

3 files scored

Filtering:
  excluded 2 test files
  0 vendored files excluded

  Score    Churn     Complexity  File
  39,150   450       87          src/Engine/State/Game.cpp
  28,800   320       90          src/Battlescape/Map.cpp
  12,000   200       60          src/Menu/MainMenu.cpp
```

### risk

```
Risk Analysis (last 90 days)

Formula: hotspot_score * (1 - line_coverage)

Join coverage:
  3 files with hotspot data
  3 files with coverage data
  3 files with both (shown below)

  Risk       Hotspot   Coverage  File
  15,660.0   39,150    60.0%     src/Engine/State/Game.cpp
  11,520.0   28,800    60.0%     src/Battlescape/Map.cpp
   4,800.0   12,000    60.0%     src/Menu/MainMenu.cpp
```

### coverage (import summary)

```
Coverage Import

3 files imported
1 file could not be normalized (paths outside repo)
2 indexed files have no coverage data

Imported files:
  src/foo.js     85.0%   170/200 statements
  src/bar.js     72.5%   145/200 statements
  src/baz.js     91.2%   182/200 statements

Unnormalized paths (1, backend sample):
  ../outside/foo.js

Unmatched indexed files (2, backend sample):
  src/old.js
  src/legacy.js
```

## Output Contract (preserved from CLI-OUT-5)

1. **No clipping** — Full output, caller can pipe to `head`
2. **No arbitrary top-N** — Don't sample or truncate
3. **Deterministic ordering** — Sort by primary score descending, alphabetical tie-breakers
4. **`--json` preserved** — Machine mode outputs raw JSON
5. **No invented verdicts** — Risk especially: no "CRITICAL"/"HIGH"/"LOW" labels
6. **Hints guide action** — When results are empty or unexpected, suggest next steps

## Definition of Done

### Group 1: Volatility/Hotspots — COMPLETE (2026-05-20)

**Files:**
- [x] `presentation/churn.rs` (238 lines) — churn renderer
- [x] `presentation/hotspots.rs` (355 lines) — hotspots renderer
- [x] `commands/quality/churn.rs` (266 lines) — `--json` flag + human mode
- [x] `commands/quality/hotspots.rs` (379 lines) — `--json` flag + human mode

**Decision:** Split files chosen after DTO sketch. Rationale:
- Hotspots has formula + filtering metadata that churn lacks
- Filtering section is own rendering subsection (5 fields, conditionally shown)
- Row columns differ (3 vs 5)
- Separate change axes justify separate files

**Functionality:**
- [x] `churn` human renderer + `--json` flag
- [x] `hotspots` human renderer + `--json` flag (with filtering section)
- [x] Deterministic ordering (by lines_changed/score descending, path asc tie-breaker)
- [x] Full output, no truncation

**Proof surfaces:**
- [x] Unit tests: 7 churn + 8 hotspots = 15 total
- [x] CLI error-path tests: 13 (usage errors, flag acceptance, invalid DB handling)
- [ ] CLI success-path tests: NOT IMPLEMENTED — accepted debt, see TECH-DEBT.md

**Corpus validation (OBSERVED, documented in review packet):**
- [x] repo-graph corpus (868 files churn, 289 files hotspots)
- [x] Human output format verified
- [x] Filtering section appears when --exclude-tests active
- [x] --json mode produces raw JSON envelope

**Out-of-scope side effect:**
- `commands/quality/risk.rs` touched to preserve compilation after `parse_since_args`
  signature change. Minimal adjustment: destructure `SinceArgs` struct instead of tuple.
  Risk human output NOT implemented (Group 2 scope).

### Group 2: Risk — COMPLETE (2026-05-20)

**Files:**
- [x] `presentation/risk.rs` (321 lines) — risk renderer
- [x] `commands/quality/risk.rs` (332 lines) — `--json` flag + human mode

**Functionality:**
- [x] `risk` human renderer + `--json` flag
- [x] Join metadata visible (hotspot_files, coverage_files, joined_files)
- [x] No invented verdict labels (explicitly tested)
- [x] Deterministic ordering (by risk_score descending, path asc tie-breaker)

**Proof surfaces:**
- [x] Unit tests: 7 (including no_verdict_labels test)
- [x] CLI error-path tests: 5 (usage, invalid db, unknown arg, flag acceptance)

**Corpus validation (OBSERVED):**
- [x] Empty-join path: repo-graph (289 hotspot files, 0 coverage files)
- [x] Hint displays correctly when no coverage data
- [ ] Positive-path (nonzero join): NOT CORPUS-VALIDATED — no corpus has coverage data

**Evidence note:** Positive risk path (files with both hotspot and coverage) validated
by unit tests only. Current corpus lacks coverage data. This is an evidence limitation,
not a defect.

### Group 3: Coverage — COMPLETE (2026-05-20)

**Files:**
- [x] `presentation/coverage.rs` (353 lines) — coverage renderer
- [x] `commands/quality/coverage_cmd.rs` (286 lines) — `--json` flag + human mode

**Functionality:**
- [x] `coverage` human renderer + `--json` flag
- [x] Import summary (imported_count, unnormalized_count, unmatched_indexed_count)
- [x] Backend-bounded sample paths labeled as "(N of M, backend sample)"
- [x] Deterministic ordering (by file_path)
- [x] Full imported file output (no renderer clipping)

**Proof surfaces:**
- [x] Unit tests: 9 (including backend-bounded sample labeling test)
- [x] CLI error-path tests: 5 (usage, missing args, missing report, unknown arg, flag acceptance)

**Corpus validation (OBSERVED):**
- [x] Positive import path: repo-graph (13 files imported)
- [x] Unmatched indexed path: 1 file without coverage (sample displayed)
- [ ] Unnormalized paths: NOT OBSERVED — current corpus has no paths outside repo

**Evidence note:** Unnormalized paths rendering validated by unit tests only.
Current corpus coverage report has all paths within repo root.

## Files in Scope

### Presentation (new files)

**Group 1:**
- `presentation/churn.rs` (238 lines) — churn renderer (IMPLEMENTED)
- `presentation/hotspots.rs` (355 lines) — hotspots renderer (IMPLEMENTED)

**Group 2:**
- `presentation/risk.rs` (321 lines) — risk renderer (IMPLEMENTED)

**Group 3:**
- `presentation/coverage.rs` (353 lines) — coverage import renderer (IMPLEMENTED)

### Commands (updates)

- `commands/quality/churn.rs` (212 lines) — add --json + human mode
- `commands/quality/hotspots.rs` (342 lines) — add --json + human mode
- `commands/quality/risk.rs` (296 lines) — add --json + human mode
- `commands/quality/coverage_cmd.rs` (239 lines) — add --json + human mode

## Explicit Non-Goals

- Do not change response structure
- Do not change scoring algorithms
- Do not migrate to REG-1 daemon contract
- Do not add pass/fail judgments or verdict labels
- Do not add colors/styling (future slice)
