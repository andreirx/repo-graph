# C-SB-1: C State Boundaries

Status: SHIPPED (2026-05-12)
Depends: c-extractor (SHIPPED), state-bindings infrastructure (SHIPPED)
Unblocks: CPP-SB-1 (C++ state boundaries, separate slice)
Track: C/C++ Systems Maturation

## Goal

Emit `ResolvedCallsite` facts from the C extractor for state-boundary-relevant
function calls, enabling the state-boundary substrate to process C code.

**Rationale:** Native/systems codebases (embedded, kernel modules, CLI tools)
use C for file I/O and database access. State-boundary extraction for C is
a distinct capability from TypeScript/Python/Java—different APIs (`fopen` vs
`fs.readFile`), different idioms (mode strings vs flags), no import system.

C and C++ are separate actors with different change reasons:
- C: POSIX APIs, global function names, mode strings
- C++: `std::` namespaces, RAII constructors, stream objects

This slice covers C only. C++ is a follow-on slice (CPP-SB-1) with separate
`CppAdapter`.

## Problem Analysis

### Current State

The C extractor:
- Extracts symbols (functions, structs, enums, typedefs)
- Emits CALLS edges for function calls
- Returns `resolved_callsites: Vec::new()` (empty)

Other extractors with ResolvedCallsite emission:
- **TypeScript:** import bindings → module resolution → emit if fs/path API
- **Python:** synthetic `builtins` module → emit if open/sqlite3/psycopg2
- **Java:** import bindings → emit if `java.sql` module

### C Architectural Reality

C has NO module-import model:
- `#include` copies header content, does not create import bindings
- Standard library functions (`fopen`, `printf`) are globally visible
- Third-party APIs (`sqlite3_open`) are globally visible after include

**Key insight:** For C, the "module" is synthetic—derived from function name
patterns. We do NOT need include tracking.

### Design Decision: Direct Name Matching

Unlike TS/Java where we resolve `identifier → import binding → module`, for C
we use **direct function name matching**:

```
fopen("config.txt", "r")
  → function_name = "fopen"
  → matches binding: module="libc:stdio", symbol_path="fopen_read"
  → emit ResolvedCallsite with resolved_module="libc:stdio", resolved_symbol="fopen_read"
```

This is valid because:
1. C function names are globally unique by convention (no namespaces)
2. Standard library functions have well-known names
3. Include guards prevent redefinition

## Scope

### In Scope

1. **State-boundary-relevant C APIs (narrow first-cut):**
   - `fopen` with mode parsing → direction-specific symbol
   - `open` (POSIX) with flags parsing → direction-specific symbol
   - `sqlite3_open`, `sqlite3_open_v2` → read_write

2. **Mode argument parsing (IN SCOPE NOW):**
   - `fopen(path, "r")` → `fopen_read`
   - `fopen(path, "w")` → `fopen_write`
   - `fopen(path, "a")` → `fopen_write`
   - `fopen(path, "r+")` → `fopen_read_write`
   - `fopen(path, "w+")` → `fopen_read_write`
   - Missing/dynamic mode → `fopen_read` (default like Python)

3. **ResolvedCallsite emission in c-extractor:**
   - Hook into call extraction
   - Match function name against state-boundary-relevant set
   - Extract arg0 payload (filename/path)
   - Extract arg1 payload for mode (fopen)
   - Emit `ResolvedCallsite` with synthetic module based on function family

4. **Bindings table extension:**
   - Add C entries to `state-bindings/bindings.toml`
   - Use `libc:stdio` for `fopen` family
   - Use `libc:fcntl` for POSIX `open`
   - Use `sqlite3` for SQLite C API

5. **CAdapter in state-extractor:**
   - New `CAdapter` implementing `LanguageStateAdapter`
   - Register in `AdapterRegistry`
   - Mode-to-symbol normalization (like PythonAdapter)

### Out of Scope (This Slice)

- **C++ code** → separate CPP-SB-1 slice with CppAdapter
- `fread`, `fwrite`, etc. (need file handle provenance)
- Network socket APIs
- Database APIs beyond SQLite
- Memory-mapped file APIs (`mmap`)
- Macro-wrapped calls (`FOPEN(path)`)
- Dynamic paths (non-string-literal arg0)

## Detection Patterns

### fopen (libc:stdio)

```c
FILE* f = fopen("config.txt", "r");      // → fopen_read
FILE* f = fopen(path, "w");              // → fopen_write
FILE* f = fopen(path, "a");              // → fopen_write
FILE* f = fopen(path, "r+");             // → fopen_read_write
FILE* f = fopen(path, "w+");             // → fopen_read_write
FILE* f = fopen(path, "rb");             // → fopen_read (binary stripped)
FILE* f = fopen(path, NULL);             // → fopen_read (default)
```

Mode normalization matches Python adapter pattern:
- Strip `'b'` (binary mode indicator)
- `'r'` or empty → read
- `'w'`, `'a'` → write
- Contains `'+'` → read_write

### open (libc:fcntl)

```c
int fd = open("data.bin", O_RDONLY);     // → open_read
int fd = open("data.bin", O_WRONLY);     // → open_write
int fd = open("data.bin", O_RDWR);       // → open_read_write
```

Flag parsing:
- `O_RDONLY` → read
- `O_WRONLY` → write
- `O_RDWR` → read_write

Note: Flags are integer constants. First-cut may use static analysis of
flag names in source; runtime values are out of scope.

### SQLite C API

```c
sqlite3* db;
sqlite3_open("app.db", &db);             // → read_write
sqlite3_open_v2("app.db", &db, flags, NULL);  // → read_write
```

SQLite open is always `read_write` (direction determined by subsequent
execute calls, which are out of scope).

## Implementation Plan

### Phase 1: C Extractor ResolvedCallsite

Add ResolvedCallsite emission to C extractor.

1. Add `is_state_boundary_function(name: &str)` matcher
2. In call extraction, check if function name matches
3. Extract arg0 (filename) as string literal payload
4. Extract arg1 (mode) as string literal payload for `fopen`
5. Emit `ResolvedCallsite` with:
   - `resolved_module`: synthetic (`libc:stdio`, `libc:fcntl`, `sqlite3`)
   - `resolved_symbol`: direction-specific (e.g., `fopen_read`)
   - `arg0_payload`: path string
   - `arg1_payload`: mode string (for fopen)

### Phase 2: C Bindings

Add C bindings to `state-bindings/bindings.toml`:

```toml
# ── C fopen family ─────────────────────────────────────────────────
#
# C-SB-1: fopen with mode-normalized direction symbols.

[[binding]]
language      = "c"
module        = "libc:stdio"
symbol_path   = "fopen_read"
resource_kind = "fs"
driver        = "libc"
direction     = "read"
basis         = "stdlib_api"

[[binding]]
language      = "c"
module        = "libc:stdio"
symbol_path   = "fopen_write"
resource_kind = "fs"
driver        = "libc"
direction     = "write"
basis         = "stdlib_api"

[[binding]]
language      = "c"
module        = "libc:stdio"
symbol_path   = "fopen_read_write"
resource_kind = "fs"
driver        = "libc"
direction     = "read_write"
basis         = "stdlib_api"

# ── C POSIX open family ────────────────────────────────────────────

[[binding]]
language      = "c"
module        = "libc:fcntl"
symbol_path   = "open_read"
resource_kind = "fs"
driver        = "libc"
direction     = "read"
basis         = "stdlib_api"

[[binding]]
language      = "c"
module        = "libc:fcntl"
symbol_path   = "open_write"
resource_kind = "fs"
driver        = "libc"
direction     = "write"
basis         = "stdlib_api"

[[binding]]
language      = "c"
module        = "libc:fcntl"
symbol_path   = "open_read_write"
resource_kind = "fs"
driver        = "libc"
direction     = "read_write"
basis         = "stdlib_api"

# ── SQLite C API ───────────────────────────────────────────────────

[[binding]]
language      = "c"
module        = "sqlite3"
symbol_path   = "sqlite3_open"
resource_kind = "db"
driver        = "sqlite3"
direction     = "read_write"
basis         = "stdlib_api"

[[binding]]
language      = "c"
module        = "sqlite3"
symbol_path   = "sqlite3_open_v2"
resource_kind = "db"
driver        = "sqlite3"
direction     = "read_write"
basis         = "stdlib_api"
```

### Phase 3: CAdapter

Create `state-extractor/src/languages/c.rs`:

1. `CAdapter` implementing `LanguageStateAdapter`
2. `language()` → `Language::C`
3. `adapt_callsites()`:
   - For `fopen`: mode normalization → direction-specific symbol
   - For `open`: flag pattern matching → direction-specific symbol
   - For `sqlite3_open*`: passthrough
4. Synthetic `ImportView` construction (like Python builtins)

Register in `default_registry()`.

### Phase 4: Hook Promotion

Update `state_boundary_hook.rs`:
- Add `"c"` to `classify_language()` as `Supported(Language::C)`

## Validation Corpus

Create `test/fixtures/c/state-boundaries-corpus/`:

```
src/
  fopen_read.c        # fopen("x", "r")
  fopen_write.c       # fopen("x", "w"), fopen("x", "a")
  fopen_read_write.c  # fopen("x", "r+"), fopen("x", "w+")
  open_posix.c        # open("x", O_RDONLY), O_WRONLY, O_RDWR
  sqlite_basic.c      # sqlite3_open("app.db", &db)
  negative_cases.c    # printf (not state boundary), dynamic paths
```

## Validation Commands

```bash
# 1. Build
cd rust && cargo build -p repo-graph-c-extractor -p repo-graph-state-extractor

# 2. Unit tests
cargo test -p repo-graph-c-extractor resolved_callsite
cargo test -p repo-graph-state-extractor c_adapter

# 3. Index corpus
rmap index test/fixtures/c/state-boundaries-corpus ./test-artifacts/c-sb-1.db

# 4. Verify resources
rmap resource list ./test-artifacts/c-sb-1.db state-boundaries-corpus
# Expected: file resources from fopen/open calls, DB resources from sqlite3

# 5. Smoke validation on swupdate
cd ../legacy-codebases/swupdate
rmap index . ./test-artifacts/swupdate.db
rmap resource list ./test-artifacts/swupdate.db swupdate
# Expected: real C file I/O patterns detected
```

## Test Matrix

| Input | Expected Output |
|-------|-----------------|
| `fopen("x", "r")` | ResolvedCallsite: `libc:stdio`, `fopen_read` |
| `fopen("x", "w")` | ResolvedCallsite: `libc:stdio`, `fopen_write` |
| `fopen("x", "a")` | ResolvedCallsite: `libc:stdio`, `fopen_write` |
| `fopen("x", "r+")` | ResolvedCallsite: `libc:stdio`, `fopen_read_write` |
| `fopen("x", "rb")` | ResolvedCallsite: `libc:stdio`, `fopen_read` |
| `fopen(var, "r")` | No callsite (dynamic path) |
| `open("x", O_RDONLY)` | ResolvedCallsite: `libc:fcntl`, `open_read` |
| `sqlite3_open("x", &db)` | ResolvedCallsite: `sqlite3`, `sqlite3_open` |
| `printf(...)` | No callsite (not state boundary) |

## Acceptance Criteria

1. C extractor emits `ResolvedCallsite` for `fopen`, `open`, `sqlite3_open` with string literal arg0
2. Mode parsing works: `fopen("x", "r")` → `fopen_read`
3. State-bindings has C entries for file I/O and SQLite
4. `CAdapter` exists and is registered in `AdapterRegistry`
5. `rmap resource list` shows resources from C corpus
6. All unit tests pass
7. Smoke validation on swupdate succeeds
8. E2E integration test with refresh coverage

## Definition of Done

- All phases complete
- Mode parsing implemented and tested
- Validation on swupdate (real C codebase)
- E2E integration test (`c_sb_1_integration.rs` — 10 tests)
- Refresh-path coverage (unchanged preservation, mixed files, dedup)
- Documentation updated
- Slice promoted to SHIPPED

## Follow-on Slice

CPP-SB-1 (C++ State Boundaries):
- `CppAdapter` separate from `CAdapter` (different actors)
- `std::fstream` family
- Qualified name matching (`std::ifstream::open`)
- Separate bindings for C++ standard library

## Risk Assessment

| Risk | Mitigation |
|------|------------|
| Flag parsing complexity (`O_RDONLY`) | Start with string pattern matching on flag names |
| Mode argument not string literal | Skip callsite, emit diagnostic |
| Macro-wrapped calls | Out of scope for first-cut, document limitation |

## Design Decisions

### Q1: Why separate CAdapter and CppAdapter?

C and C++ are different actors with different change reasons:
- C: POSIX APIs, function names, no namespaces
- C++: `std::` namespace, constructors, RAII patterns

Separate adapters follow SRP. Shared logic can be extracted to common module.

### Q2: Why mode parsing in scope now?

Mode parsing was marked "refine later" in original draft. User correction:
mode parsing is essential for direction accuracy. Without it, every `fopen`
would be `read_write`, which defeats the purpose of state-boundary analysis.

Python adapter already implements mode parsing. C adapter should match.

### Q3: Why swupdate as validation target?

swupdate is a real C codebase with:
- File I/O patterns (`fopen`, `open`)
- SQLite usage
- No C++ contamination
- Reasonable size for smoke testing
