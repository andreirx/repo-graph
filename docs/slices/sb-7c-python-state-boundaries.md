# SB-7C: Python State Boundaries

Status: PLANNED
Depends: `sb-7a-state-boundaries-support-substrate.md` (REQUIRED)
Soft-depends: `py-ext-2-python-extractor-depth.md` (improves callsite resolution)
Follow-on: None

## Goal

Implement state-boundary extraction for Python using the SB-7A substrate. Detect file-system interactions and DB-API 2.0 database usage, emitting READS/WRITES edges to resource nodes.

## Certainty Layer

**Layer 2 (Derived Architecture)**

Interprets deterministic Python AST nodes into explicit state-resource graph edges based on binding patterns.

### Degradation Policy

When a callsite matches a known API but arguments are unresolvable:
- Emit edge with `resource_key = "unknown:{api_name}"`
- Set `confidence = 0.5`
- Log to extraction diagnostics

When callsite resolution is weak (PY-EXT-2 not shipped):
- Accept higher false-negative rate
- Document in extraction diagnostics: "callsite resolution limited"

## Scope

### In Scope

**Filesystem APIs (first cut):**
- `open(path, mode)` — built-in
- `os.open(path, flags)`
- `os.read(fd, n)` / `os.write(fd, data)`
- `pathlib.Path.open(mode)`
- `pathlib.Path.read_text()` / `Path.read_bytes()`
- `pathlib.Path.write_text(data)` / `Path.write_bytes(data)`

**Explicitly OUT for first cut:**
- `shutil.*` (copy, move, rmtree)
- `tempfile.*`
- `io.open` (alias detection complex)

**DB-API 2.0 APIs (first cut):**
- `sqlite3.connect(database)`
- `psycopg2.connect(dsn)`
- `mysql.connector.connect(**kwargs)`
- `cursor.execute(sql)`
- `cursor.executemany(sql, params)`

**Call Provenance:**
- Track call location (file, line, column)
- Track containing function/method
- Track mode argument for open() to determine READS vs WRITES

**Mode Classification:**
- `'r'`, `'rb'` → READS
- `'w'`, `'wb'`, `'a'`, `'ab'`, `'x'` → WRITES
- `'r+'`, `'w+'` → both READS and WRITES edges

### Out of Scope

- SQLAlchemy ORM (session.query, model.save)
- Django ORM (Model.objects.*)
- Flask/FastAPI/Django route detection
- shutil, tempfile, io.open
- Indirect calls through wrappers

## Crate Layout

```
rust/crates/state-extractor/src/languages/
├── mod.rs                    # Adapter registry (updated)
├── typescript.rs             # Existing TS adapter
├── java.rs                   # SB-7B
└── python.rs                 # NEW: PythonStateAdapter

rust/crates/state-bindings/
└── bindings.toml             # Extended with Python entries
```

## Prerequisites

- SB-7A shipped: `LanguageStateAdapter` trait exists
- `python-extractor` emits `CALL` nodes with:
  - `callee_name` or `callee_qualified_name`
  - `arguments` array
- If PY-EXT-2 not shipped: accept degraded callsite resolution

## Validation Corpus

Repository: `test/fixtures/python/state-boundaries-corpus/`

Must contain:
- `open()` with literal path
- `pathlib.Path.read_text()`
- `sqlite3.connect()` with literal path
- At least 5 files, 500+ LOC

Fallback: Any FastAPI/Flask app with file upload and SQLite.

## Validation Commands

```bash
# 1. Build
cd rust && cargo build -p repo-graph-state-extractor

# 2. Unit tests
cargo test -p repo-graph-state-extractor python

# 3. Index validation corpus (product surface)
rmap index test/fixtures/python/state-boundaries-corpus ./test-artifacts/sb-7c.db

# 4. Primary validation: list state boundaries
rmap boundaries list ./test-artifacts/sb-7c.db state-boundaries-corpus --kind state_boundary

# 5. Semantic check: verify open() detected
rmap boundaries list ./test-artifacts/sb-7c.db state-boundaries-corpus --kind state_boundary \
  | jq '.results[] | select(.resource_key | contains("config.json"))'
# Must return FS_PATH with READS edge

# 6. Semantic check: verify DB resource
rmap boundaries list ./test-artifacts/sb-7c.db state-boundaries-corpus --kind state_boundary \
  | jq '.results[] | select(.resource_kind == "DB_RESOURCE")'

# 7. Dual-edge check for r+ mode
rmap boundaries list ./test-artifacts/sb-7c.db state-boundaries-corpus --kind state_boundary \
  | jq '[.results[] | select(.resource_key | contains("data.json"))] | length'
# If opened with 'r+', must return 2 (one READS, one WRITES)
```

## Acceptance Criteria

1. `PythonStateAdapter` implements `LanguageStateAdapter` trait
2. `bindings.toml` contains ≥6 Python filesystem entries, ≥4 DB-API entries
3. `rmap boundaries list --kind state_boundary` returns results (not empty)
4. **Semantic example:** `open('config.json', 'r')` → FS_PATH with READS edge
5. **Semantic example:** `Path('output.txt').write_text(data)` → FS_PATH with WRITES edge
6. **Semantic example:** `sqlite3.connect('app.db')` → DB_RESOURCE with `resource_key` containing `app.db`
7. **Dual-edge example:** `open('data.json', 'r+')` → TWO edges to same resource (READS + WRITES)
8. **Negative example:** `shutil.copy(src, dst)` → no edges (explicitly out of scope)
9. Extraction diagnostics: unresolved arguments logged
10. `cargo test -p repo-graph-state-extractor` — all pass

## Definition of Parity

"Parity" for this slice means:
- **API coverage:** Listed APIs detected when called with literal arguments
- **Mode classification:** Correct READS/WRITES based on open mode
- **Resource key accuracy:** Literal paths extracted correctly

NOT required:
- Detection of wrapped IO utilities
- Resolution of f-string paths
- ORM operation mapping

## Alternatives Considered

### A. Include shutil/tempfile
Rejected: shutil operations are often bulk (copy trees), tempfile paths are runtime-generated. Low value for complexity.

### B. Include SQLAlchemy
Rejected: ORM has complex session/model semantics. Needs schema inference. Separate slice.

### C. Require PY-EXT-2 as hard dependency
Rejected: Basic call extraction works without PY-EXT-2. Accept degraded resolution rather than blocking.
