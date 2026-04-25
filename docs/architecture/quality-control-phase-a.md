# Quality Control Phase A: Measurement Contract

Date: 2026-04-25
Status: Implemented
Scope: Support module only (no policies, no gates, no orient/check integration)

## Overview

Phase A adds two new measurements to the Rust extraction pipeline:

- `function_length` — line count of full function node span (signature + body)
- `cognitive_complexity` — Sonar-style weighted complexity

Both are deterministic AST-derived facts. They use the existing `measurements`
table and follow the four-layer truth model (measurements are layer 2, not
inferences).

Language scope: TypeScript/JavaScript only. Cross-language rollout is
explicitly deferred.

## 1. Function Length Semantics

### Definition

```
function_length = line_end - line_start + 1
```

Where `line_start` and `line_end` come from the existing `SourceLocation`
attached to the extracted symbol node.

### What the span includes

The span is defined by tree-sitter's node range for the function/method
AST node. This includes:

- **Signature**: function name, parameters, return type annotation
- **Body**: the statement block `{ ... }`
- **Inline comments**: any comments inside the body that fall within the
  node's byte range

### What the span excludes

- **Leading doc comments**: JSDoc `/** ... */` above the function is a
  sibling node in tree-sitter, not a child of the function node
- **Leading decorators**: `@decorator` lines are sibling nodes in
  tree-sitter (for TS class methods)
- **Trailing whitespace**: beyond the closing brace

### No normalization

The measurement is the raw line span. No blank-line stripping, no
comment-line filtering, no SLOC derivation. Raw span is the stable,
reproducible fact. Derived metrics (SLOC, non-blank lines) can be
computed later if needed, but are not part of this measurement kind.

### Rationale

Raw span is:
- Deterministic (same parse = same number)
- Cheap (already computed during extraction)
- Interpretable (matches what a developer sees in the editor)

Filtered counts (SLOC) require additional heuristics that vary by
convention and are better left to downstream tooling.

## 2. Cognitive Complexity Semantics

### Baseline

SonarQube cognitive complexity rules as published in:
https://www.sonarsource.com/docs/CognitiveComplexity.pdf

### Counting rules

**Structural increments (+1 each)**:
- `if` statement
- `else if` (as a separate increment, not nesting)
- `else` clause
- Ternary `? :`
- `switch` statement (not per-case)
- `for`, `for...in`, `for...of` loops
- `while`, `do...while` loops
- `catch` clause
- Logical operators `&&`, `||`, `??` (each occurrence)

**Nesting penalty (+1 per nesting level)**:
Applied to: `if`, `else if`, `else`, ternary, `switch`, loops, `catch`
Nesting level = depth inside other control structures

**Breaks in linear flow (+1 each)**:
- `break` with label
- `continue` with label
- Early `return` (return not at end of function — NOT INCLUDED in Phase A,
  requires control flow analysis beyond tree-sitter)

### Deviations from Sonar spec

| Sonar rule | Phase A decision | Rationale |
|------------|------------------|-----------|
| Recursion penalty (+1) | **Deferred** | See below |
| Early return penalty | **Deferred** | Requires CFG analysis |
| Nested function penalty | **Excluded** | Metrics scope boundary at nested fn |
| `goto` penalty | N/A | JS/TS has no goto |

### Recursion penalty: deferred

Sonar adds +1 for recursive calls. Detecting recursion precisely requires:

1. Resolving the callee to the enclosing function
2. Handling aliased references (`const f = function g() { g(); }`)
3. Handling method recursion (`this.foo()` inside `foo()`)

Tree-sitter provides syntax only. The following cases are ambiguous:

```typescript
function foo() {
  foo(); // Direct self-call — detectable
}

const bar = function baz() {
  baz(); // Named function expression — detectable but rare
};

class C {
  method() {
    this.method(); // Requires receiver type — NOT detectable
  }
}
```

Conservative decision: **defer recursion penalty entirely** rather than
implement partial detection that produces false negatives on common
patterns (method recursion) while claiming precision.

Technical debt: Add recursion penalty when type-enrichment-backed call
resolution is available for the metrics path.

### Nesting calculation

Nesting level is tracked during AST walk. The walker maintains a depth
counter incremented when entering a nesting structure, decremented when
exiting.

`else if` is a special case: in tree-sitter, `else if (x) {}` is parsed
as `else_clause` containing an `if_statement`. The walker treats this as
continuation at the same level, not deeper nesting. This matches Sonar
semantics.

### Scope boundaries

Nested functions (arrow functions, function expressions, inner function
declarations) are **separate measurement targets**. The outer function's
cognitive complexity does not include the inner function's complexity.
Each function produces its own measurement row.

This matches the existing `cyclomatic_complexity` behavior.

## 3. Target Set

Measurements are emitted for the following node subtypes:

| Subtype | Included | Notes |
|---------|----------|-------|
| `FUNCTION` | Yes | Top-level and exported functions |
| `METHOD` | Yes | Class methods |
| `ARROW_FUNCTION` | Yes | If extracted as a named symbol |
| `FUNCTION_EXPRESSION` | Yes | If extracted as a named symbol |
| `CONSTRUCTOR` | Yes | Class constructors |
| `GETTER` | Yes | Property getters |
| `SETTER` | Yes | Property setters |

These are exactly the symbol subtypes that already receive
`cyclomatic_complexity` measurements. Phase A adds `function_length`
and `cognitive_complexity` to the same target set.

### File-level aggregates

**Not included in Phase A.** File-level aggregates (sum, max, avg of
function metrics) are derived facts. They can be computed at query time
or stored as separate measurements in a later phase. Phase A stores only
per-function facts.

## 4. Persistence

### Table

Reuse existing `measurements` table (migration 012, already shipped).

### Schema reminder

```sql
CREATE TABLE measurements (
  measurement_uid     TEXT PRIMARY KEY,
  snapshot_uid        TEXT NOT NULL REFERENCES snapshots(snapshot_uid),
  repo_uid            TEXT NOT NULL REFERENCES repos(repo_uid),
  target_stable_key   TEXT NOT NULL,
  kind                TEXT NOT NULL,
  value_json          TEXT NOT NULL,
  source              TEXT NOT NULL,
  created_at          TEXT NOT NULL
);
```

### Measurement identity

A measurement is uniquely identified by the semantic tuple:

```
(snapshot_uid, target_stable_key, kind)
```

The `measurement_uid` primary key is **deterministic**, derived from
this tuple. The existing compose path uses the pattern:

```
measurement_uid = "{snapshot_uid}-{kind_abbrev}-{target_stable_key}"
```

Where `kind_abbrev` is a short code per kind:
- `cc` = cyclomatic_complexity
- `pc` = parameter_count
- `mnd` = max_nesting_depth
- `fl` = function_length (Phase A)
- `cog` = cognitive_complexity (Phase A)

**Duplicate prevention mechanism:**

Extractor-computed measurements (cyclomatic_complexity, parameter_count,
max_nesting_depth, function_length, cognitive_complexity) use plain
`insert_measurements()`. No collision occurs because:

1. Each index run creates a NEW snapshot with a fresh `snapshot_uid`
2. `measurement_uid` includes `snapshot_uid`, so UIDs are unique across snapshots
3. Within a single index run, each `(target_stable_key, kind)` is processed once

Imported measurements (coverage, churn) use `replace_measurements_by_kind()`
because they can be re-imported multiple times for the same snapshot. The
atomic replace deletes existing rows of that kind before inserting new ones.

**Why deterministic UIDs matter:**

Random UUIDs are NOT used. Deterministic UIDs provide:
- Traceability: can reconstruct UID from semantic tuple for debugging
- Consistency: same logical measurement has same UID if somehow recomputed

The schema has no UNIQUE constraint on `(snapshot_uid, target_stable_key,
kind)`. Duplicate prevention relies on the single-pass extraction discipline
for extractor-computed metrics and atomic replace for imported metrics.

### Measurement kinds (Phase A additions)

| Kind | value_json shape | source |
|------|------------------|--------|
| `function_length` | `{"value": 42}` | `"ts-extractor:1.0.0"` |
| `cognitive_complexity` | `{"value": 15}` | `"ts-extractor:1.0.0"` |

The `source` field uses the extractor name and version. This allows
distinguishing measurements from different extractor versions when
comparing snapshots.

### value_json contract

```typescript
interface FunctionLengthValue {
  value: number;  // positive integer, line count
}

interface CognitiveComplexityValue {
  value: number;  // non-negative integer
}
```

Both use the same `{"value": N}` shape as existing metrics
(`cyclomatic_complexity`, `parameter_count`, `max_nesting_depth`).
Uniform shape simplifies query logic.

### Persistence path

1. Extractor computes metrics during AST walk
2. Orchestrator accumulates metrics keyed by node stable key
3. Compose layer converts to `MeasurementInput` and calls `insert_measurements`

This is the existing path for `cyclomatic_complexity`. Phase A adds
two more metric kinds to the same pipeline.

## 5. CLI Surface

### Command

```
rmap metrics <db_path> <repo_uid> [--kind <kind>] [--limit <n>] [--sort <field>]
```

### Arguments

| Arg | Required | Default | Description |
|-----|----------|---------|-------------|
| `db_path` | Yes | — | Path to SQLite database |
| `repo_uid` | Yes | — | Repository identifier |
| `--kind` | No | all | Filter by measurement kind |
| `--limit` | No | 50 | Max rows to return |
| `--sort` | No | `value` | Sort field: `value` (desc), `target` (asc) |

### Output shape

```json
{
  "command": "metrics",
  "repo": "repo-graph",
  "snapshot": "<snapshot_uid>",
  "snapshot_scope": "full",
  "basis_commit": "<hash>",
  "results": [
    {
      "target": "repo-graph:src/foo.ts#bar:SYMBOL:FUNCTION",
      "kind": "cognitive_complexity",
      "value": 15,
      "source": "ts-extractor:1.0.0"
    }
  ],
  "count": 1,
  "stale": false
}
```

Uses the standard QueryResult envelope for consistency with other
read-side commands.

### Filtering examples

```bash
# All cognitive complexity measurements, sorted by value descending
rmap metrics db.sqlite repo-graph --kind cognitive_complexity --sort value --limit 20

# All function_length measurements
rmap metrics db.sqlite repo-graph --kind function_length

# All measurements for a specific kind, no limit (limit 0 means unlimited)
rmap metrics db.sqlite repo-graph --kind cyclomatic_complexity --limit 0

# All measurements, sorted by target (alphabetical)
rmap metrics db.sqlite repo-graph --sort target --limit 100
```

### Available kinds

| Kind | Description | Computed by |
|------|-------------|-------------|
| `cyclomatic_complexity` | McCabe cyclomatic complexity | ts-extractor |
| `parameter_count` | Number of function parameters | ts-extractor |
| `max_nesting_depth` | Maximum control-flow nesting | ts-extractor |
| `function_length` | Line span (end - start + 1) | ts-extractor |
| `cognitive_complexity` | Sonar-style cognitive complexity | ts-extractor |
| `line_coverage` | Test coverage (from imported reports) | coverage import |

### Why this command exists

Prevents the "stored but invisible" anti-pattern. Measurements are
first-class queryable facts. An agent or human can inspect what was
measured without writing raw SQL.

## 6. Validation Plan

### Fixture tests

**function_length**:
- Empty function: `function f() {}` → 1 line
- Multi-line function: 10-line body → 10 lines
- Function with inline comments: comments inside body counted
- Nested function: outer and inner measured separately

**cognitive_complexity**:
- Empty function: 0
- Single if: 1
- Nested if: 1 (outer) + 2 (inner with +1 nesting)
- if-else-if chain: 1 + 1 + 1 (no nesting penalty for else-if)
- Logical operators: `a && b || c` → 2
- Ternary inside if: 1 (if) + 2 (ternary with +1 nesting)
- Loop with nested if: 1 (loop) + 2 (if with +1 nesting)

### Real-repo validation

Index `repo-graph` itself and produce a validation report:

1. Top 20 functions by `cognitive_complexity`
2. Top 20 functions by `function_length`
3. Spot-check 5 functions manually against Sonar's web analyzer
4. Document any discrepancies

### Determinism test

Index the same fixture twice. Assert identical measurement values.

## 7. Implementation Plan

### Slice 1: function_length

1. Add `function_length` to `ExtractedMetrics` struct
2. Compute in `ts-extractor/src/metrics.rs` from node location
3. Persist via existing orchestrator → compose path
4. Add unit tests

### Slice 2: cognitive_complexity

1. Add `cognitive_complexity` to `ExtractedMetrics` struct
2. Implement `compute_cognitive_complexity()` in `ts-extractor/src/metrics.rs`
3. Wire into extraction path
4. Add unit tests for all counting rules

### Slice 3: CLI surface

1. Add `rmap metrics` command to `rgr/src/main.rs`
2. Add `query_measurements` helper (or reuse `query_measurements_by_kind`)
3. Add CLI integration tests

### Slice 4: Validation

1. Run on repo-graph
2. Produce validation report
3. Update TECH-DEBT.md with any limitations discovered

## 8. Technical Debt (anticipated)

| Item | Description | Severity |
|------|-------------|----------|
| Recursion penalty deferred | Cognitive complexity does not detect recursive calls | P3 |
| Early return penalty deferred | Requires CFG analysis | P3 |
| TS/JS only | Java, Rust, Python, C++ not covered | P2 |
| No file-level aggregates | Query-time aggregation only | P3 |

## 9. Non-goals (explicit exclusions)

- Policy declarations
- Gate integration
- No-new/worsened mode
- orient/check integration
- NPath complexity
- Cross-language rollout
- Risk scoring integration

These are Phase B+ scope.

## 10. Success criteria

1. `function_length` persisted for all TS/JS functions
2. `cognitive_complexity` persisted for all TS/JS functions
3. `rmap metrics` command works
4. Validation report on repo-graph with no major anomalies
5. Deterministic tests pass
6. Technical debt documented
