# CPP-SB-1: C++ State Boundaries

Status: **SHIPPED**
Depends: C-SB-1 (SHIPPED), cpp-extractor (SHIPPED)
Track: C/C++ Systems Maturation

## Goal

Emit `ResolvedCallsite` facts from the C++ extractor for state-boundary-relevant
APIs, enabling the state-boundary substrate to process C++ code.

## Design Decisions (Locked)

### D1: Binding duplication (Option A)

Duplicate C bindings for `language = "cpp"`. No implicit fallback. Explicit
and auditable. Eight entries is acceptable first-cut cost.

**Not chosen:**
- B (adapter fallback) — implicit coupling, surprising match behavior
- C (dual-emit) — extractor policy coupling, dedup risk

### D2: Scope 2 — constructors + `.open()` member calls

Cover both patterns:
```cpp
std::ifstream file("config.txt");           // constructor with path
std::ifstream file; file.open("config.txt"); // default + .open()
```

Scope 1 (constructors only) rejected as artificially weak.

### D3: Intra-function local type map for `.open()` resolution

The current C++ extractor extracts `file.open(...)` as callee `open` only —
it does not know the receiver type. To resolve `.open()` calls to stream types,
introduce a **bounded local type map** inside function body extraction.

**Mechanism:**

During function body traversal, maintain `HashMap<String, StreamType>` mapping
local variable identifiers to their declared stream type. When encountering
`receiver.open(path)`, look up `receiver` in the map.

**Explicit limits (hard boundaries):**

| Supported | Not supported |
|-----------|---------------|
| Local variable declarations | Member fields |
| Same function body | Cross-function propagation |
| Direct identifier receiver (`file.open()`) | Factory returns (`getStream().open()`) |
| Simple declarations | Aliases (`auto& ref = file`) |
| | References/pointers |
| | Reassignment |

**Rationale:** This is new substrate inside the extractor, but bounded. The
limits are chosen to avoid unbounded dataflow analysis while covering the
common pattern of declare-then-open in the same function.

**Example coverage:**

```cpp
void load() {
    std::ifstream config;        // map: { "config" -> ifstream }
    config.open("/etc/app.ini"); // lookup "config" -> ifstream -> emit
}
```

**Example non-coverage (accepted):**

```cpp
void load(std::ifstream& file) {
    file.open("/etc/app.ini");   // parameter, not local decl -> no emit
}

std::ifstream& getConfig();
void load() {
    getConfig().open("/etc/x");  // factory return -> no emit
}
```

## Scope

### In Scope

1. **C++ standard library file streams:**
   - `std::ifstream` constructor with path → read
   - `std::ofstream` constructor with path → write
   - `std::fstream` constructor with path + mode → direction from mode
   - `.open(path)` and `.open(path, mode)` member calls on stream objects

2. **C-style APIs in C++ files:**
   - `fopen`, `open`, `sqlite3_open*` (via duplicated cpp bindings)
   - Same detection logic as C-SB-1

3. **CppAdapter in state-extractor:**
   - Separate from CAdapter (different actor)
   - Stream type → direction mapping
   - Mode flag parsing for `std::fstream` and `.open()`

### Out of Scope

- `std::filesystem` (C++17, lower priority)
- Boost.Asio, Qt file APIs (third-party)
- `fread`/`fwrite` (need file handle provenance)
- Stream operations after open (need provenance)

## Detection Patterns

### Pattern 1: Constructor with path literal

```cpp
std::ifstream config("settings.ini");        // → read
std::ofstream log("/var/log/app.log");       // → write
std::fstream data("data.bin", std::ios::in); // → read (mode parsed)
```

AST: `declaration` with `init_declarator` containing constructor call.

### Pattern 2: `.open()` member call (via local type map)

```cpp
void load() {
    std::ifstream file;              // declaration tracked in local type map
    file.open("/etc/config");        // receiver "file" looked up → ifstream → read
}
```

AST structure:
```
call_expression
  └─ function: field_expression
       ├─ argument: identifier ("file")  ← lookup in local type map
       └─ field: "open"
  └─ arguments
       └─ string_literal ("/etc/config")
```

**Resolution flow:**
1. During function body extraction, track `file -> ifstream` in local type map
2. On `file.open(path)`, lookup `file` → `ifstream`
3. Emit `ResolvedCallsite` with `resolved_symbol = "ifstream_open"`

**Non-covered patterns (by design):**
```cpp
void load(std::ifstream& file) {
    file.open("/etc/config");        // parameter, not local → no emit
}
```

### Pattern 3: C-style APIs

```cpp
FILE* f = fopen("/tmp/data", "r");           // → fopen_read
int fd = open("/dev/tty", O_RDONLY);         // → open_read
```

Same detection as C-SB-1.

## Mode Mapping

### Stream types (no mode argument)

| Type | Direction |
|------|-----------|
| `std::ifstream` | read |
| `std::ofstream` | write |
| `std::fstream` | read_write (default) |

### Mode flags (std::ios)

| Flag pattern | Direction |
|--------------|-----------|
| `std::ios::in` only | read |
| `std::ios::out` only | write |
| `std::ios::in | std::ios::out` | read_write |
| `std::ios::app` | write |
| `std::ios::trunc` | write |

Note: Mode parsing for `std::fstream` is best-effort. If mode cannot be
determined statically, default to `read_write`.

## Implementation Plan

### Phase 1: C++ Bindings

Add to `state-bindings/bindings.toml`:

```toml
# ── C++ duplicates of C APIs ───────────────────────────────────────
#
# CPP-SB-1: Same APIs, language = "cpp" for C++ files.

[[binding]]
language      = "cpp"
module        = "libc:stdio"
symbol_path   = "fopen_read"
resource_kind = "fs"
driver        = "libc"
direction     = "read"
basis         = "stdlib_api"

[[binding]]
language      = "cpp"
module        = "libc:stdio"
symbol_path   = "fopen_write"
resource_kind = "fs"
driver        = "libc"
direction     = "write"
basis         = "stdlib_api"

[[binding]]
language      = "cpp"
module        = "libc:stdio"
symbol_path   = "fopen_read_write"
resource_kind = "fs"
driver        = "libc"
direction     = "read_write"
basis         = "stdlib_api"

[[binding]]
language      = "cpp"
module        = "libc:fcntl"
symbol_path   = "open_read"
resource_kind = "fs"
driver        = "libc"
direction     = "read"
basis         = "stdlib_api"

[[binding]]
language      = "cpp"
module        = "libc:fcntl"
symbol_path   = "open_write"
resource_kind = "fs"
driver        = "libc"
direction     = "write"
basis         = "stdlib_api"

[[binding]]
language      = "cpp"
module        = "libc:fcntl"
symbol_path   = "open_read_write"
resource_kind = "fs"
driver        = "libc"
direction     = "read_write"
basis         = "stdlib_api"

[[binding]]
language      = "cpp"
module        = "sqlite3"
symbol_path   = "sqlite3_open"
resource_kind = "db"
driver        = "sqlite3"
direction     = "read_write"
basis         = "stdlib_api"

[[binding]]
language      = "cpp"
module        = "sqlite3"
symbol_path   = "sqlite3_open_v2"
resource_kind = "db"
driver        = "sqlite3"
direction     = "read_write"
basis         = "stdlib_api"

# ── C++ standard library streams ───────────────────────────────────
#
# CPP-SB-1: std::fstream family.

[[binding]]
language      = "cpp"
module        = "std:fstream"
symbol_path   = "ifstream"
resource_kind = "fs"
driver        = "libstdc++"
direction     = "read"
basis         = "stdlib_api"

[[binding]]
language      = "cpp"
module        = "std:fstream"
symbol_path   = "ifstream_open"
resource_kind = "fs"
driver        = "libstdc++"
direction     = "read"
basis         = "stdlib_api"

[[binding]]
language      = "cpp"
module        = "std:fstream"
symbol_path   = "ofstream"
resource_kind = "fs"
driver        = "libstdc++"
direction     = "write"
basis         = "stdlib_api"

[[binding]]
language      = "cpp"
module        = "std:fstream"
symbol_path   = "ofstream_open"
resource_kind = "fs"
driver        = "libstdc++"
direction     = "write"
basis         = "stdlib_api"

[[binding]]
language      = "cpp"
module        = "std:fstream"
symbol_path   = "fstream"
resource_kind = "fs"
driver        = "libstdc++"
direction     = "read_write"
basis         = "stdlib_api"

[[binding]]
language      = "cpp"
module        = "std:fstream"
symbol_path   = "fstream_open"
resource_kind = "fs"
driver        = "libstdc++"
direction     = "read_write"
basis         = "stdlib_api"

[[binding]]
language      = "cpp"
module        = "std:fstream"
symbol_path   = "fstream_read"
resource_kind = "fs"
driver        = "libstdc++"
direction     = "read"
basis         = "stdlib_api"

[[binding]]
language      = "cpp"
module        = "std:fstream"
symbol_path   = "fstream_write"
resource_kind = "fs"
driver        = "libstdc++"
direction     = "write"
basis         = "stdlib_api"
```

### Phase 2: C++ Extractor ResolvedCallsite

Modify `cpp-extractor/src/extractor.rs`:

**2.1 Core imports and context:**

1. Add `ResolvedCallsite` import from `repo_graph_classification::types`
2. Add `resolved_callsites: Vec<ResolvedCallsite>` to `ExtractionCtx`
3. Populate `resolved_callsites` in `ExtractionResult`

**2.2 C-style API detection (Pattern 3):**

4. Add `try_resolve_callsite_cpp()` — reuse C-SB-1 logic for `fopen`, `open`,
   `sqlite3_open*` detection with mode/flag parsing

**2.3 Stream constructor detection (Pattern 1):**

5. Add `try_resolve_stream_constructor()` for stream constructor detection

AST pattern:
```
declaration
  └─ type: qualified_identifier ("std::ifstream") or type_identifier
  └─ declarator: init_declarator
       ├─ declarator: identifier (variable name)
       └─ value: argument_list
            └─ string_literal (path)
```

Check type specifier against: `std::ifstream`, `std::ofstream`, `std::fstream`,
`ifstream`, `ofstream`, `fstream`.

**2.4 Local type map substrate (new):**

6. Add `StreamType` enum: `{ Ifstream, Ofstream, Fstream }`
7. Add per-function `local_stream_types: HashMap<String, StreamType>` tracking
8. On stream declaration (with or without path), record `identifier -> StreamType`
9. Clear map at function boundary (start of each function body extraction)

**2.5 `.open()` member call detection (Pattern 2):**

10. Add `try_resolve_stream_open()` for `.open()` member call detection

AST pattern:
```
call_expression
  └─ function: field_expression
       ├─ argument: identifier (receiver_name)
       └─ field: "open"
  └─ arguments
       └─ string_literal (path)
       └─ [optional] binary_expression (mode flags)
```

Resolution flow:
1. Check `field == "open"`
2. Check `argument.kind() == "identifier"`
3. Extract `receiver_name = argument.utf8_text()`
4. Lookup `receiver_name` in `local_stream_types`
5. If found → emit `ResolvedCallsite` with appropriate symbol
6. If not found → no emit (receiver not a tracked local stream)

**Explicit limits enforced:**
- Map is function-scoped (cleared on function entry)
- Only simple `identifier` receivers (not `getStream().open()`)
- Only local variable declarations tracked (not parameters, members, returns)

### Phase 3: CppAdapter

Create `state-extractor/src/languages/cpp.rs`:

1. `CppAdapter` implementing `LanguageStateAdapter`
2. `language()` → `Language::Cpp`
3. `adapt_callsites()`:
   - For C-style APIs: same logic as CAdapter
   - For streams: map stream type to direction-specific symbol
   - For `.open()`: extract mode if present, map to symbol

Register in `default_registry()`.

### Phase 4: Hook Promotion

Update `state_boundary_hook.rs`:
- Add `Some("cpp") => LanguageClassification::Supported(Language::Cpp)`

## Validation Strategy

E2E integration tests using temp repos (`cpp_sb_1_integration.rs`).
No persistent fixture corpus — test cases are inline in the integration test file.

This approach was chosen over a fixture corpus because:
- Temp repos provide isolated, reproducible test environments
- Test cases are self-documenting (source and assertions co-located)
- No fixture maintenance burden
- Same validation rigor as C-SB-1

Deferred: `test/fixtures/cpp/state-boundaries-corpus/` for CLI smoke validation.
The E2E tests cover all contracts; CLI corpus is polish, not correctness.

## Test Matrix

### Positive cases (emit ResolvedCallsite)

| Input | Expected Output |
|-------|-----------------|
| `std::ifstream f("x")` | `std:fstream`, `ifstream` |
| `std::ofstream f("x")` | `std:fstream`, `ofstream` |
| `std::fstream f("x")` | `std:fstream`, `fstream` |
| `std::fstream f("x", std::ios::in)` | `std:fstream`, `fstream_read` |
| `ifstream f; f.open("x")` (local decl) | `std:fstream`, `ifstream_open` |
| `fopen("x", "r")` | `libc:stdio`, `fopen_read` |
| `open("x", O_RDONLY)` | `libc:fcntl`, `open_read` |
| `sqlite3_open("x", &db)` | `sqlite3`, `sqlite3_open` |

### Negative cases (no emit — by design)

| Input | Reason |
|-------|--------|
| `std::cout << x` | Not state boundary |
| `void f(ifstream& s) { s.open("x"); }` | Parameter, not local decl |
| `getStream().open("x")` | Factory return, not identifier receiver |
| `auto& ref = file; ref.open("x")` | Alias, not direct identifier |
| `file.open(pathVar)` | Dynamic path, not string literal |

## Acceptance Criteria

1. C++ extractor emits `ResolvedCallsite` for stream constructors with path literal
2. C++ extractor emits `ResolvedCallsite` for `.open()` calls with path literal
3. C++ extractor emits `ResolvedCallsite` for C-style APIs (fopen, open, sqlite3_open)
4. State-bindings has C++ entries (duplicated C + stream family)
5. `CppAdapter` exists and is registered
6. All unit tests pass
7. E2E integration test with refresh coverage

## Definition of Done

- All phases complete
- Stream constructor and `.open()` detection implemented
- C-style API detection in C++ files working
- E2E integration test (`cpp_sb_1_integration.rs` — target: 10+ tests) ✓ (20 tests)
- Refresh-path coverage (3 tests minimum) ✓ (4 tests)
- Documentation updated
- Slice promoted to SHIPPED

## Validation (EXECUTED)

E2E integration test: `rust/crates/repo-index/tests/cpp_sb_1_integration.rs`

**Test results: 20 passed, 0 failed**

| Category | Tests | Coverage |
|----------|-------|----------|
| Constructor path | 5 | ifstream, ofstream, fstream + modes (in, out, in\|out) |
| .open() via D3 | 3 | ifstream.open, ofstream.open, fstream.open with mode |
| C-style APIs | 3 | fopen, open, sqlite3_open in .cpp files |
| Negative limits | 5 | parameter receiver, factory return, member field, dynamic path, :memory: |
| Refresh path | 4 | unchanged preservation, mixed changed/unchanged, dedup, D3 facts survive |

All contracts validated:
- Constructor path: stream constructors emit correct direction edges
- D3 local type map: .open() calls on local stream variables resolve correctly
- C-style APIs: duplicated cpp bindings work correctly
- Negative limits: parameter/factory/member/dynamic cases correctly emit nothing
- Refresh: state-boundary facts survive refresh via copy-forward, dedup works

## Risk Assessment

| Risk | Mitigation |
|------|------------|
| Local type map new substrate | Bounded by D3 limits; function-scoped, no dataflow |
| Mode flag parsing complex | Best-effort; default to read_write if ambiguous |
| Constructor AST patterns vary | Test against multiple C++ codebases |
| C++ template metaprogramming | Out of scope; only concrete stream types |

## Technical Debt

Document as known limitations in TECH-DEBT.md:

- **Local type map limits (D3)**: First cut tracks only local variable declarations
  in the same function body. Does not cover:
  - Parameters (`void f(ifstream& s)`)
  - Member fields (`this->file_.open()`)
  - Factory returns (`getStream().open()`)
  - Aliases/references (`auto& ref = file`)
  - Cross-function propagation
  
  These are explicit design limits, not bugs. Generalized receiver-type resolution
  is future substrate work (not part of CPP-SB-1).

- **Mode parsing**: Limited to literal `std::ios::*` patterns. Variable modes
  or complex expressions default to read_write.

- **Binding duplication**: 8 C bindings duplicated for `language = "cpp"`. If
  this becomes maintenance burden, future slice can introduce binding-table
  substrate extension (language families, shared bindings, alias groups).
