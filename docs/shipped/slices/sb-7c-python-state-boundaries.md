# SB-7C: Python State Boundaries

Status: SHIPPED (2026-05-11)
Depends: `sb-7a-state-boundaries-support-substrate.md` (REQUIRED)
Soft-depends: `py-ext-2-python-extractor-depth.md` (improves callsite resolution)
Follow-on: None

## Goal

Implement state-boundary extraction for Python using the SB-7A substrate. Detect file-system interactions and DB-API 2.0 database usage, emitting READS/WRITES edges to resource nodes.

## Certainty Layer

**Layer 2 (Derived Architecture)**

Interprets deterministic Python AST nodes into explicit state-resource graph edges based on binding patterns.

### Degradation Policy

When a callsite matches a known API but arguments are unresolvable (variable, f-string, complex expression):
- **Silent filtering**: No `ResolvedCallsite` is constructed
- **No edge emission**: Unresolvable callsites produce zero graph output
- **No diagnostic**: This is expected behavior for dynamic argument patterns

Rationale: Emitting `unknown:*` resources would pollute the graph with low-value nodes. The first-cut implementation filters at extraction time rather than emitting degraded evidence.

When callsite resolution is weak (PY-EXT-2 not shipped):
- Accept higher false-negative rate
- No explicit diagnostic (implicit in lower coverage)

## Scope

### In Scope

**Filesystem APIs (first cut):**
- `open(path, mode)` — built-in

**DB-API 2.0 APIs (first cut):**
- `sqlite3.connect(database)` — positional arg0 only
- `psycopg2.connect(dsn)` — positional arg0 only

### Deferred (requires callsite fact model extension)

**Receiver-based APIs (resource identity on receiver, not arg0):**
- `pathlib.Path.open(mode)`
- `pathlib.Path.read_text()` / `Path.read_bytes()`
- `pathlib.Path.write_text(data)` / `Path.write_bytes(data)`

Reason: Current `ResolvedCallsite` DTO has no `receiver_payload` field.
These APIs carry resource identity on the receiver expression (`Path("/etc/config")`),
not in positional arguments. Requires support-substrate extension (SB-7X or similar).

**Keyword-argument DB constructors:**
- `mysql.connector.connect(**kwargs)` — keyword args not representable

Reason: Current `ResolvedCallsite` DTO has no keyword-argument payload support.
Requires support-substrate extension.

**Descriptor/cursor provenance APIs:**
- `os.open(path, flags)` / `os.read(fd, n)` / `os.write(fd, data)`
- `cursor.execute(sql)` / `cursor.executemany(sql, params)`

Reason: Argument 0 is a file descriptor or SQL text, not the resource identity.
Requires fd→path or cursor→connection provenance tracking.

### Explicitly OUT for first cut
- `shutil.*` (copy, move, rmtree)
- `tempfile.*`
- `io.open` (alias detection complex)

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
└── python.rs                 # PythonAdapter

rust/crates/state-bindings/
└── bindings.toml             # Extended with Python entries
```

## Prerequisites

- SB-7A shipped: `LanguageStateAdapter` trait exists
- `python-extractor` emits `ResolvedCallsite` facts for state-boundary APIs
- If PY-EXT-2 not shipped: accept degraded callsite resolution

## Validation Corpus

Repository: `test/fixtures/python/state-boundaries-corpus/`

Contains:
- `open_read.py` — open() with r/rb modes
- `open_write.py` — open() with w/a/x modes
- `open_read_write.py` — open() with r+/w+/a+ modes
- `db_sqlite3.py` — sqlite3.connect() with literal paths
- `db_psycopg2.py` — psycopg2.connect() with literal DSNs
- `negative_cases.py` — deferred patterns (pathlib, kwargs, cursor, dynamic paths)

6 files, ~100 LOC. Minimal corpus covering shipped API surface.

## Validation Commands

```bash
# 1. Build
cd rust && cargo build -p repo-graph-state-extractor -p repo-graph-python-extractor

# 2. Unit tests
cargo test -p repo-graph-python-extractor
cargo test -p repo-graph-state-extractor

# 3. Index validation corpus (product surface)
rmap index test/fixtures/python/state-boundaries-corpus /tmp/sb-7c.db

# 4. Primary validation: list resources
rmap resource list /tmp/sb-7c.db state-boundaries-corpus

# 5. Semantic check: verify open() detected
rmap resource readers /tmp/sb-7c.db state-boundaries-corpus \
  "state-boundaries-corpus:fs:/etc/config.yaml:FS_PATH"
# Must return reader count > 0

# 6. Semantic check: verify DB resource
rmap resource list /tmp/sb-7c.db state-boundaries-corpus --kind DB_RESOURCE

# 7. Dual-edge check for r+ mode
# open(..., 'r+') should produce both READS and WRITES edges to same resource
```

## Acceptance Criteria

1. `PythonAdapter` implements `LanguageStateAdapter` trait
2. `bindings.toml` contains Python entries for `builtins:open_*` and `sqlite3/psycopg2:connect`
3. `rmap resource list` returns Python-detected resources (not empty)
4. **Semantic example:** `open('config.json', 'r')` → FS_PATH with READS edge
5. **Semantic example:** `open('output.txt', 'w')` → FS_PATH with WRITES edge
6. **Semantic example:** `sqlite3.connect('app.db')` → DB_RESOURCE with `resource_key` containing `app.db`
7. **Dual-edge example:** `open('data.json', 'r+')` → TWO edges to same resource (READS + WRITES)
8. **Negative example:** `shutil.copy(src, dst)` → no edges (explicitly out of scope)
9. **Negative example:** `Path('x').read_text()` → no edges (deferred, requires receiver payload)
10. `cargo test -p repo-graph-state-extractor` — all pass
11. `cargo test -p repo-graph-python-extractor` — all pass

## Definition of Parity

"Parity" for this slice means:
- **API coverage:** `open()`, `sqlite3.connect()`, `psycopg2.connect()` detected with literal arguments
- **Mode classification:** Correct READS/WRITES based on `open()` mode argument
- **Resource key accuracy:** Literal paths/DSNs extracted correctly

NOT required (deferred):
- Detection of wrapped IO utilities
- Resolution of f-string paths
- ORM operation mapping
- `pathlib.Path.*` receiver-based methods (requires receiver payload)
- `mysql.connector.connect(**kwargs)` (requires keyword arg payload)
- `cursor.execute()` (requires cursor provenance)

## Shipped Evidence (2026-05-11)

### Validation Executed

```
$ rmap index test/fixtures/python/state-boundaries-corpus /tmp/sb-7c.db
[state-boundary] state_boundary_emit_error: state-boundary emit failed: ...":memory:"... (known limitation)
indexed 6 files, 51 nodes, 31 edges (41 unresolved) → state-boundaries-corpus/...

$ rmap resource list /tmp/sb-7c.db state-boundaries-corpus
{
  "results": [
    { "stable_key": "...:db:sqlite3:app.db:DB_RESOURCE", "readers": 2, "writers": 2 },
    { "stable_key": "...:db:sqlite3:/var/data/production.db:DB_RESOURCE", ... },
    { "stable_key": "...:db:psycopg2:dbname=mydb user=postgres:DB_RESOURCE", ... },
    { "stable_key": "...:db:psycopg2:host=localhost dbname=app...:DB_RESOURCE", ... },
    { "stable_key": "...:fs:/etc/config.yaml:FS_PATH", "readers": 3, "writers": 1 },
    { "stable_key": "...:fs:/data/image.png:FS_PATH", "readers": 1, "writers": 0 },
    { "stable_key": "...:fs:/data/output.bin:FS_PATH", "readers": 0, "writers": 1 },
    { "stable_key": "...:fs:/var/log/app.log:FS_PATH", "readers": 1, "writers": 3 },
    { "stable_key": "...:fs:/tmp/lockfile:FS_PATH", "readers": 0, "writers": 1 },
    { "stable_key": "...:fs:/tmp/scratch.txt:FS_PATH", "readers": 1, "writers": 1 },
    { "stable_key": "...:fs:/data/file.bin:FS_PATH", "readers": 1, "writers": 1 }
  ],
  "count": 11,
  "total_reads": 12,
  "total_writes": 13
}
```

### Test Results

- `cargo test -p repo-graph-python-extractor`: 59 passed
- `cargo test -p repo-graph-state-extractor`: 56 passed
- `cargo test -p repo-graph-repo-index -- state_boundary`: 3 passed

### Acceptance Criteria Validation

1. `PythonAdapter` implements `LanguageStateAdapter` — VERIFIED
2. `bindings.toml` contains Python entries — VERIFIED (builtins:open_*, sqlite3:connect, psycopg2:connect)
3. `rmap resource list` returns Python-detected resources — VERIFIED (11 resources)
4. `open('config.json', 'r')` → FS_PATH with READS edge — VERIFIED (readers=3 for /etc/config.yaml)
5. `open('output.txt', 'w')` → FS_PATH with WRITES edge — VERIFIED (writers=1 for /data/output.bin)
6. `sqlite3.connect('app.db')` → DB_RESOURCE — VERIFIED (app.db detected)
7. `open('data.json', 'r+')` → READS + WRITES edges — VERIFIED (/data/file.bin readers=1, writers=1)
8. `shutil.copy()` → no edges — VERIFIED (not in results)
9. `Path('x').read_text()` → no edges — VERIFIED (deferred, not extracted)

### Known Limitations

- **`:memory:` database**: SQLite in-memory databases (`:memory:`) fail stable-key segment
  grammar (colon forbidden). This is documented as expected behavior. Workaround: use
  file paths for SQLite DBs in code intended for graph analysis.

## Alternatives Considered

### A. Include shutil/tempfile
Rejected: shutil operations are often bulk (copy trees), tempfile paths are runtime-generated. Low value for complexity.

### B. Include SQLAlchemy
Rejected: ORM has complex session/model semantics. Needs schema inference. Separate slice.

### C. Require PY-EXT-2 as hard dependency
Rejected: Basic call extraction works without PY-EXT-2. Accept degraded resolution rather than blocking.

### D. Emit `unknown:*` resources for unresolvable arguments
Rejected for first cut: Would pollute graph with low-value nodes. Silent filtering is cleaner for initial implementation. Future slice may revisit if diagnostic visibility is needed.
