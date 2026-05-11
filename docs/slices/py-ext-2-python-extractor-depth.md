# PY-EXT-2: Python Extractor Depth

Status: IMPLEMENTED (functional only; performance validation deferred to PY-EXT-2-PERF)
Depends: None
Follow-on: `sb-7c-python-state-boundaries.md` (benefits from improved callsite resolution)

## Goal

Bring the Rust `python-extractor` crate to structural parity with the legacy TS-based Python extractor. This slice adds variable extraction, constructor identification, and code metrics without regressing streaming performance.

## Certainty Layer

**Layer 0-1 (Deterministic Extracted Fact)**

This slice exclusively parses source code via tree-sitter and emits factual structural data. No bounded inferences, framework detection, or resource boundaries.

### Degradation Policy

Not applicable — Layer 0-1 extraction is deterministic. If tree-sitter fails to parse a file, emit file-level error diagnostic; do not emit partial/guessed nodes.

## Scope

### In Scope

**Variables:**
- Emit `VARIABLE` nodes for local variable assignments (`x = 1`)
- Emit `VARIABLE` nodes for annotated assignments (`x: int = 1`)
- Do NOT emit for instance attribute assignments (`self.x = 1`) — these are FIELD nodes, not VARIABLE
- Do NOT emit for loop variables, comprehension variables, or import bindings (captured by other node kinds)

**Constructors:**
- Emit `CONSTRUCTOR` classification on `__init__` method nodes
- `__new__` is OUT for first cut (static method semantics differ)

**Metrics:**
- `cyclomatic_complexity`: computed on function/method nodes only
  - Count: `if`, `elif`, `for`, `while`, `and`, `or`, `except`, `with`, `assert`, `match/case`
  - Base complexity: 1
- `nesting_depth`: maximum nesting level within function body
- Do NOT aggregate metrics on class nodes (per-method only)

**Performance:**
- Peak memory: ≤ 1.1x current baseline on 100k LOC corpus
- Throughput: ≥ 0.95x current baseline (no more than 5% regression)

### Out of Scope

- Framework detection (Flask/Django routes) — Layer 3
- State boundaries (file/DB) — Layer 2
- `__new__` constructor detection — deferred
- Class-level metric aggregation — deferred

## Crate Layout

```
rust/crates/python-extractor/
├── src/
│   ├── lib.rs                    # Entry point
│   ├── extractor.rs              # Main extraction logic
│   ├── variables.rs              # NEW: variable detection
│   ├── constructors.rs           # NEW: __init__ classification
│   ├── metrics.rs                # NEW: complexity/nesting
│   └── queries/
│       ├── mod.rs
│       ├── variables.scm         # NEW: tree-sitter query
│       └── metrics.scm           # NEW: branching query
└── tests/
    ├── variables.rs              # Variable extraction tests
    ├── constructors.rs           # Constructor tests
    ├── metrics.rs                # Metric computation tests
    └── fixtures/
        ├── simple_class.py
        ├── nested_functions.py
        └── complex_control_flow.py
```

## Prerequisites

- tree-sitter-python grammar is current
- Existing function/class/import extraction works correctly

## Validation Corpus

Primary: `test/fixtures/python/extractor-depth-corpus/`

Fixtures required:
- `simple_class.py` — class with `__init__`, instance variables
- `nested_functions.py` — closures, nested defs
- `complex_control_flow.py` — high cyclomatic complexity

Secondary: `requests` library (external, for performance validation)

## Validation Commands

```bash
# 1. Build
cd rust && cargo build -p repo-graph-python-extractor

# 2. Unit tests
cargo test -p repo-graph-python-extractor

# 3. Index fixture corpus (product surface)
rmap index test/fixtures/python/extractor-depth-corpus ./test-artifacts/py-ext-2.db

# 4. Primary validation: query extracted nodes
rmap stats ./test-artifacts/py-ext-2.db extractor-depth-corpus
# Verify node counts in output

# 5. Semantic check: verify specific named symbols exist
rmap callers ./test-artifacts/py-ext-2.db extractor-depth-corpus "MyClass.__init__"
# Must find the constructor

# 6. Secondary diagnostic (crate-level)
cargo test -p repo-graph-python-extractor fixture_simple_class
cargo test -p repo-graph-python-extractor metrics_complex_control_flow

# 7. Performance validation (DEFERRED - see notes below)
# The crate is a library; direct cargo run is invalid.
# Correct method requires product-path benchmark:
#   1. Checkout parent commit (pre-PY-EXT-2)
#   2. Build: cargo build --release -p repo-graph-rgr
#   3. Baseline: /usr/bin/time -l rmap index <pinned-corpus> ./baseline.db
#   4. Checkout PY-EXT-2 commit
#   5. Rebuild and repeat measurement
#   6. Compare wall-clock and peak RSS
# Corpus: Django or CPython subset (~100k LOC)
# No baseline currently exists; validation deferred.
```

## Acceptance Criteria

### Functional (required for IMPLEMENTED)

1. `VARIABLE` nodes emitted for local assignments in fixture corpus
2. `VARIABLE` nodes emitted for annotated assignments (`x: int = 1`)
3. `__init__` methods classified as `CONSTRUCTOR` kind
4. `cyclomatic_complexity` metric present on function/method nodes
5. `nesting_depth` metric present on function/method nodes
6. **Semantic example:** `simple_class.py` contains `class MyClass` with `__init__` → node named `MyClass.__init__` with kind `CONSTRUCTOR`
7. **Semantic example:** `simple_class.py` contains `count = 0` in function → `VARIABLE` node named `count`
8. **Negative example:** `self.value = x` in `__init__` → NOT a VARIABLE node (instance attribute)
9. Fixture `complex_control_flow.py`: cyclomatic_complexity ≥ 5
10. `cargo test -p repo-graph-python-extractor` — all pass

### Performance (required for SHIPPED)

11. Throughput ≥ 0.95x baseline on ~100k LOC corpus
12. Peak memory ≤ 1.1x baseline on same corpus

**Note:** Performance criteria require a baseline from pre-PY-EXT-2 commit. No baseline currently exists. Performance validation is deferred to follow-on slice `PY-EXT-2-PERF`.

## Definition of Parity

"Parity" for this slice means:
- **Node kind match:** Same node kinds emitted (VARIABLE, CONSTRUCTOR, FUNCTION, etc.)
- **Count tolerance:** Variable/constructor counts within ±5% on validation corpus
- **Metric presence:** All functions have cyclomatic_complexity and nesting_depth

NOT required for parity:
- Exact metric values (algorithm may differ slightly)
- Exact variable locations (line numbers may vary by ±1)

## Alternatives Considered

### A. Include `__new__` as constructor
Rejected for first cut: `__new__` has static method semantics and different invocation patterns. Adds complexity without clear value.

### B. Aggregate metrics on class nodes
Rejected: Class-level aggregation is a presentation concern, not extraction. Consumers can aggregate if needed.

### C. Include comprehension variables
Rejected: Comprehension variables are scoped to the expression, not useful for most analysis. Adds noise.

## Validation Evidence (2026-05-11)

### Implementation Notes

The implementation diverged from the proposed crate layout:
- Variables, constructors, and metrics logic are inline in `extractor.rs` rather than separate modules
- No tree-sitter `.scm` query files; logic uses direct tree-sitter API traversal
- This is acceptable for first cut; refactoring to separate modules is TECH_DEBT

### Product-Surface Validation (Primary)

All primary validation uses `rmap` CLI, not direct SQL.

**Index corpus:**
```bash
rmap index test/fixtures/python/extractor-depth-corpus ./test-artifacts/py-ext-2.db
# Result: indexed 3 files, 38 nodes, 0 edges (10 unresolved)
```

**Constructor classification (criterion 3):**
```bash
rmap callers ./test-artifacts/py-ext-2.db extractor-depth-corpus "MyClass.__init__"
# target.subtype: "CONSTRUCTOR"
# target.qualified_name: "MyClass.__init__"
# target.stable_key: "extractor-depth-corpus:simple_class.py#MyClass.__init__:SYMBOL:CONSTRUCTOR"
```

**Annotated assignment - module level (criterion 2):**
```bash
rmap callers ./test-artifacts/py-ext-2.db extractor-depth-corpus "DEBUG_MODE"
# target.subtype: "VARIABLE"
# target.qualified_name: "DEBUG_MODE"
# Validates: `DEBUG_MODE: bool = False` extracted as VARIABLE
```

**Annotated assignment - local (criterion 2):**
```bash
rmap callers ./test-artifacts/py-ext-2.db extractor-depth-corpus "standalone_function.final"
# target.subtype: "VARIABLE"
# target.qualified_name: "standalone_function.final"
# Validates: `final: int = temp + 1` extracted as scoped VARIABLE
```

**Unit tests (criterion 10):**
```bash
cargo test -p repo-graph-python-extractor
# Result: 51 passed; 0 failed
# Includes: extracts_annotated_assignment_module_level, extracts_annotated_assignment_in_function
```

### Storage Diagnostic (Secondary)

SQL queries used only for aggregate verification after product-surface validation:

```bash
sqlite3 ./test-artifacts/py-ext-2.db "SELECT COUNT(*) FROM measurements WHERE kind='cyclomatic_complexity'"
# Result: 14 (all functions/methods have complexity metrics)
```

### Acceptance Criteria Status

| # | Criterion | Status | Evidence Type |
|---|-----------|--------|---------------|
| 1 | VARIABLE for local assignments | EXECUTED | rmap callers |
| 2 | VARIABLE for annotated assignments | EXECUTED | rmap callers (DEBUG_MODE, standalone_function.final) |
| 3 | `__init__` as CONSTRUCTOR | EXECUTED | rmap callers (MyClass.__init__) |
| 4 | cyclomatic_complexity present | EXECUTED | SQL count = 14 |
| 5 | nesting_depth present | EXECUTED | SQL count = 14 |
| 6 | MyClass.__init__ semantic example | EXECUTED | rmap callers |
| 7 | count VARIABLE semantic example | EXECUTED | unit test |
| 8 | self.value negative example | EXECUTED | unit test |
| 9 | cyclomatic_complexity >= 5 | EXECUTED | SQL: highly_complex=9, deeply_nested=6 |
| 10 | Unit tests pass | EXECUTED | 51 passed |
| 11 | Throughput >= 0.95x | **DEFERRED** | No baseline exists |
| 12 | Memory <= 1.1x | **DEFERRED** | No baseline exists |

### Deferred Work

**PY-EXT-2-PERF:** Performance validation follow-on slice
- Establish baseline from pre-PY-EXT-2 commit
- Pin corpus (Django or CPython subset, ~100k LOC)
- Benchmark via `rmap index` product path
- Compare wall-clock and peak RSS
- Document results

### Tech Debt

- TD-PY-EXT-2-A: Refactor variables/constructors/metrics into separate modules per proposed crate layout
