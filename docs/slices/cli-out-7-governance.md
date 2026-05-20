# CLI-OUT-7: Governance Output

**Status:** COMPLETE (2026-05-20)  
**Type:** Product Surface / Implementation  
**Prerequisite:** CLI-OUT-6

## Problem Statement

Governance commands currently dump raw JSON. Users need scannable human output
for assessment summaries, violation diagnostics, and gate evaluation results.

## Scope

Human renderers for 3 governance commands, grouped by output complexity.

### In Scope (3 commands)

**Assessment summary:**
- `rmap assess`

**Violation diagnostics:**
- `rmap violations`

**Gate evaluation:**
- `rmap gate`

### Excluded

- `rmap modules violations` — already has human output (CLI-OUT-4)
- Changes to verdict calculation logic
- Changes to response structure

## Grouping Rationale

Commands grouped by **output complexity**, not contract type.

The primary axis is user-facing presentation semantics:

1. **assess** — compact count/verdict surface
2. **violations** — diagnostic listing with multiple sections
3. **gate** — highest-density decision surface with obligations and evidence

Contract differences (legacy vs REG-1) are mechanism boundaries handled at
command wiring, not the presentation grouping principle.

## Contract Summary

| Command | Contract | Arguments |
|---------|----------|-----------|
| `assess` | Legacy direct-storage | `<db_path> <repo_uid> [--baseline <snap>]` |
| `violations` | Legacy direct-storage | `<db_path> <repo_uid>` |
| `gate` | REG-1 daemon | `[--strict \| --advisory]` |

## Implementation Groups

### Group 1: Assess

**Command:** `assess`

**Why first:**
- Smallest renderer
- Establishes governance vocabulary
- Establishes verdict formatting discipline
- Establishes no-invented-label discipline
- Legacy contract handling proven before denser commands

**Response shape observed:**

```json
{
  "command": "assess",
  "repo": "r1",
  "snapshot": "snap_...",
  "baseline_snapshot": null,
  "assessments": {
    "total": 2,
    "pass": 1,
    "fail": 1,
    "not_applicable": 0,
    "not_comparable": 0
  },
  "baseline_required_count": 0
}
```

**Presentation focus:**
- Totals (total, pass, fail, not_applicable, not_comparable)
- Baseline note if missing but policies require it
- Clean summary without invented severity labels

**Presentation module:** `presentation/assess.rs`

### Group 2: Violations

**Command:** `violations`

**Why second:**
- Still legacy contract (proven from Group 1)
- More structurally complex (multiple sections)
- Natural second step before gate

**Response shape observed:**

```json
{
  "command": "arch violations",
  "repo": "r1",
  "snapshot": "snap_...",
  "results": {
    "declared_boundary_violations": [
      {
        "boundary_module": "src/adapters",
        "forbidden_module": "src/core",
        "source_file": "src/adapters/store.ts",
        "target_file": "src/core/service.ts",
        "line": 1,
        "reason": "adapters must not depend on core"
      }
    ],
    "discovered_module_violations": [
      {
        "declaration_uid": "decl-1",
        "source": "packages/cli",
        "target": "packages/internal",
        "import_count": 3,
        "source_file_count": 1,
        "reason": "internal module"
      }
    ]
  },
  "declared_boundary_count": 1,
  "discovered_module_count": 0,
  "stale_declarations": [
    {
      "declaration_uid": "decl-old",
      "stale_side": "target",
      "missing_paths": ["packages/legacy"]
    }
  ],
  "stale_count": 0,
  "count": 1
}
```

**Presentation focus:**
- Declared boundary violations section
- Discovered module violations section
- Stale declarations section
- Deterministic ordering per section
- Clear counts

**Presentation module:** `presentation/violations.rs`

### Group 3: Gate

**Command:** `gate`

**Why last:**
- Most dangerous to get wrong
- Dense obligation rows
- Intrinsic verdict terms (not invented labels)
- Evidence payloads
- Waiver handling
- Overall gate outcome semantics

**Response shape observed:**

```json
{
  "command": "gate",
  "repo": "...",
  "snapshot": "snap_...",
  "toolchain": null,
  "obligations": [
    {
      "req_id": "REQ-001",
      "req_version": 1,
      "obligation_id": "obl-1",
      "obligation": "core must not depend on adapters",
      "method": "arch_violations",
      "target": "src/core",
      "threshold": null,
      "operator": null,
      "computed_verdict": "PASS",
      "effective_verdict": "PASS",
      "evidence": { "violation_count": 0 },
      "waiver_basis": null
    }
  ],
  "gate": {
    "outcome": "pass",
    "exit_code": 0,
    "mode": "default",
    "counts": {
      "total": 1,
      "pass": 1,
      "fail": 0,
      "waived": 0,
      "missing_evidence": 0,
      "unsupported": 0
    }
  }
}
```

**Presentation focus:**
- Gate outcome summary (outcome, mode, exit_code)
- Counts breakdown
- Obligation list with verdicts
- Evidence visibility (method-specific)
- Waiver basis visibility (when effective_verdict != computed_verdict)
- Preserve domain verdicts exactly

**Domain verdicts (render as-is, do not soften):**
- `PASS`
- `FAIL`
- `WAIVED`
- `MISSING_EVIDENCE`
- `UNSUPPORTED`

**Presentation module:** `presentation/gate.rs`

## Structural Assessment

**Command file sizes (before implementation):**
- `commands/assess.rs` — 138 lines
- `commands/modules/violations.rs` — 374 lines (includes `modules violations` subcommand)
- `commands/gate.rs` — 141 lines

All under 500-line guardrail. No refactoring required before adding renderer logic.

## Proposed Human Output Formats

### assess

```
Quality Assessment

Snapshot: snap_abc123
Baseline: snap_def456

2 policies evaluated

  PASS             1
  FAIL             1
  not applicable   0
  not comparable   0

hint: 1 policy requires baseline for comparison.
```

Empty case:
```
Quality Assessment

Snapshot: snap_abc123

0 policies evaluated
```

### violations

```
Architectural Violations

1 declared boundary violation
0 discovered module violations
1 stale declaration

Declared boundary violations:
  src/adapters -> src/core
    source: src/adapters/store.ts:1
    target: src/core/service.ts
    reason: adapters must not depend on core

Stale declarations:
  decl-old (stale: target)
    missing: packages/legacy
```

Empty case:
```
Architectural Violations

0 declared boundary violations
0 discovered module violations
0 stale declarations

No violations detected.
```

### gate

```
Gate Evaluation

Outcome: pass
Mode: strict
Exit code: 0

Counts:
  total              3
  pass               2
  fail               0
  waived             1
  missing evidence   0
  unsupported        0

Obligations:

  REQ-001 v1 / obl-1
    core must not depend on adapters
    method: arch_violations
    target: src/core
    computed: PASS
    effective: PASS
    evidence: violation_count=0

  REQ-002 v1 / obl-2
    adapters clean
    method: arch_violations
    target: src/adapters
    computed: FAIL
    effective: WAIVED
    evidence: violation_count=1
    waiver: known dependency, tracked for removal (expires: 2024-06-01)
```

## Output Contract

1. **No clipping** — Full output, caller can pipe to `head`
2. **No arbitrary top-N** — Don't sample or truncate
3. **Deterministic ordering** — Sort by primary key, alphabetical tie-breakers
4. **`--json` preserved** — Machine mode outputs raw JSON
5. **Domain verdicts preserved** — Do not remap PASS/FAIL/WAIVED/etc to softer language
6. **Hints guide action** — When results are empty or unexpected, suggest next steps

## Definition of Done

### Group 1: Assess — COMPLETE (2026-05-20)

**Files:**
- [x] `presentation/assess.rs` (265 lines) — assess renderer
- [x] `commands/assess.rs` (173 lines) — `--json` flag + human mode

**Functionality:**
- [x] Human renderer with count breakdown
- [x] Baseline note when missing but required
- [x] `--json` flag for machine mode
- [x] Deterministic output
- [x] Domain verdicts preserved (pass, fail, not_applicable, not_comparable)

**Proof surfaces:**
- [x] Unit tests: 9 (renderer tests in presentation/assess.rs)
- [x] CLI error-path tests: 7 (in cli_out_7_governance.rs)
- [x] CLI positive-path tests: 10 (in assess_command.rs, with fixtures)
- [x] Corpus validation: empty case OBSERVED (duckdb)

**Evidence:**
- Empty policies case: OBSERVED on duckdb corpus (0 policies evaluated)
- Positive case (pass/fail): EXECUTED via assess_command.rs fixture tests

### Group 2: Violations — COMPLETE (2026-05-20)

**Files:**
- [x] `presentation/violations.rs` (457 lines) — violations renderer
- [x] `commands/modules/violations.rs` (417 lines) — `--json` flag + human mode for top-level command

**Functionality:**
- [x] Human renderer with three sections (declared, discovered, stale)
- [x] Deterministic ordering per section
- [x] `--json` flag for machine mode
- [x] Singular/plural grammar handling
- [x] Optional line numbers and reasons

**Proof surfaces:**
- [x] Unit tests: 10 (renderer tests in presentation/violations.rs)
- [x] CLI error-path tests: 4 (in cli_out_7_governance.rs)
- [x] CLI positive-path tests: 12 (in violations_command.rs, with fixtures)
- [x] Human output tests: 2 (violations_human_output_format, violations_human_output_empty_case)

**Evidence:**
- Empty case: EXECUTED via violations_human_output_empty_case fixture test
- Positive case (declared violation): EXECUTED via violations_human_output_format fixture test
- JSON mode: EXECUTED via existing violations_exact_results fixture test

### Group 3: Gate — COMPLETE (2026-05-20)

**Files:**
- [x] `presentation/gate.rs` (430 lines) — gate renderer
- [x] `commands/gate.rs` (156 lines) — `--json` flag + human mode

**Functionality:**
- [x] Human renderer with outcome + obligations
- [x] Evidence display (method-specific key=value pairs)
- [x] Waiver basis display (when effective != computed)
- [x] Domain verdicts preserved exactly (PASS/FAIL/WAIVED/MISSING_EVIDENCE/UNSUPPORTED)
- [x] Quality assessment rendering (when present)
- [x] `--json` flag for machine mode

**Proof surfaces:**
- [x] Unit tests: 13 (renderer tests in presentation/gate.rs)
- [x] CLI error-path tests: 7 (in cli_out_7_governance.rs)
- [x] CLI parsing tests: 3 non-ignored (in gate_command.rs)
- [x] Corpus observation: OBSERVED via live daemon (django, repo-graph)

**Evidence (OBSERVED via daemon):**
- Empty case: OBSERVED (django, exit 0)
- PASS verdict: OBSERVED (repo-graph arch_violations, exit 0)
- MISSING_EVIDENCE verdict: OBSERVED (repo-graph coverage_threshold, exit 1)
- UNSUPPORTED verdict: OBSERVED (repo-graph unknown method, exit 1)
- Advisory mode: OBSERVED (exit 0 despite MISSING_EVIDENCE + UNSUPPORTED)
- FAIL verdict: NOT OBSERVED (no corpus data triggers actual failure)
- WAIVED verdict: NOT OBSERVED (no waiver declarations in corpus)

## Files in Scope

### Presentation (new files)

- `presentation/assess.rs` — assess renderer
- `presentation/violations.rs` — violations renderer (top-level command)
- `presentation/gate.rs` — gate renderer

### Commands (updates)

- `commands/assess.rs` (138 lines) — add --json + human mode
- `commands/modules/violations.rs` (374 lines) — add --json + human mode to top-level `run_violations`
- `commands/gate.rs` (141 lines) — add --json + human mode

## Explicit Non-Goals

- Do not change response structure
- Do not change verdict calculation logic
- Do not migrate legacy commands to REG-1
- Do not add colors/styling (future slice)
- Do not soften or remap domain verdict terms
