# C Extractor v1 — Design Slice

Rust-native C extractor to unblock `sqlite`, `nginx`, `swupdate` as real
validation targets.

## Scope

**In scope:**
- FILE nodes
- SYMBOL: functions (definitions only), structs, enums, typedefs
- IMPORTS edges from `#include` directives
- CALLS edges from direct function call sites (plain identifiers only)
- Cyclomatic complexity metrics (definitions only)
- `:dupN` stable-key disambiguation (port from ts-extractor)

**Out of scope (explicitly deferred):**
- C++ (separate extractor frontier)
- Macro expansion / preprocessor evaluation
- Function pointer call resolution
- Header/source ownership policy
- compile_commands.json integration
- Typedef-aware semantic resolution
- Linker-level cross-TU symbol resolution

## Locked Decisions

### 1. Declaration vs Definition

**Definitions only.** Function declarations/prototypes do NOT create symbol nodes.

- Symbol nodes created only for `function_definition`, not `declaration`
- Headers with only prototypes: no function symbols, only FILE + includes
- Complexity computed only on definitions

Reason: Otherwise headers duplicate almost every function symbol, muddying
callers/callees and complexity attribution.

### 2. Header Handling

- `.h` files ARE indexed as FILE nodes
- `#include` edges ARE extracted from headers
- Complexity computed only from function definitions (wherever they appear)

Reason: Include graphs depend heavily on headers. Many C repos encode
architecture in header layering.

### 3. Anonymous Type Naming

Explicit stable-key rules for anonymous types:

| Form | Stable Key |
|------|------------|
| Named struct | `repo:file#MyStruct:SYMBOL:STRUCT` |
| Anonymous struct | `repo:file#anon_struct:SYMBOL:STRUCT[:dupN]` |
| Named enum | `repo:file#Status:SYMBOL:ENUM` |
| Anonymous enum | `repo:file#anon_enum:SYMBOL:ENUM[:dupN]` |

Do NOT derive names from parser text fragments. Use explicit `anon_*` prefix.

### 4. CALLS Scope (Tight)

Include:
- Call expressions where callee is a plain identifier: `foo()`, `bar(x, y)`

Exclude / mark unresolved:
- Function pointer calls: `ptr()`, `(*fn)()`
- Macro-generated calls (not syntactically visible as calls)
- Member/dereference calls through pointers: `obj->method()`

Reason: Honest graph quality over fake precision.

### 5. `#include` Resolution

- Extract IMPORTS edges from all `#include` directives
- Local includes (`"foo.h"`): `isRelative=true`, attempt exact path match
- System includes (`<stdio.h>`): `isRelative=false`, keep as external/unresolved
- NO build-system search path emulation
- NO compile_commands.json dependency
- Resolution succeeds only when included path maps to an indexed file

### 6. Complexity Rules (Pinned)

Cyclomatic complexity for C v1:

| Construct | Increment |
|-----------|-----------|
| Base | +1 |
| `if` | +1 |
| `else if` | +1 |
| `for` | +1 |
| `while` | +1 |
| `do while` | +1 |
| `case` | +1 |
| `default` | +0 (no increment) |
| `switch` | +0 (cases counted individually) |
| `&&` | +1 |
| `\|\|` | +1 |
| `?:` (ternary) | +1 |
| `goto` | +1 |

This is pinned before cross-language comparisons start.

### 7. Preprocessing Policy

**Source-truth semantics, NOT compiler-truth.**

- Parse source text as-is with tree-sitter-c
- Do NOT expand macros
- Do NOT evaluate `#ifdef` / `#ifndef` / `#if`
- Extract only syntactically visible code in checked-out file contents
- Code inside `#ifdef` blocks is parsed if syntactically valid C

### 8. File Extensions

**In scope:**
- `.c`
- `.h`

**Out of scope:**
- `.cpp`, `.cc`, `.cxx`, `.hpp`, `.hxx`, `.h++` (C++ extensions)
- `.inc` files: excluded unless explicitly added to path policy later

### 9. Duplicate-Name Stability Caveat

`:dupN` disambiguation is:
- Deterministic within one file snapshot
- Occurrence-order-sensitive (AST preorder)
- Inserting an earlier same-name symbol renumbers later duplicates

**This is technical debt on cross-snapshot symbol identity.** Documented now,
not deferred to discovery.

### 10. AI-Agent Usefulness Goal

For an AI agent, C v1 must make these queries useful on `sqlite`:
- `rmap hotspots` — top churn × complexity files
- `rmap callers` / `rmap callees` — for directly called functions
- Include-layer understanding via IMPORTS edges
- `rmap dead` — only if graph quality supports it; otherwise mark low-confidence

Implementation stays aligned with actual use, not extractor completeness.

## Crate Structure

New crate: `rust/crates/c-extractor/`

Do not create a "native-extractor" umbrella. C and C++ should evolve
separately at first. Shared helpers can be extracted later if needed.

## Dependencies

- `tree-sitter` (0.24.x) — parser runtime
- `tree-sitter-c` (0.24.x) — C grammar

No `tree-sitter-cpp`. C++ is a separate slice.

## Symbol Types

| AST Node | Symbol Subtype | Stable Key |
|----------|----------------|------------|
| `function_definition` | FUNCTION | `repo:file#name:SYMBOL:FUNCTION` |
| `struct_specifier` | STRUCT | `repo:file#name:SYMBOL:STRUCT` |
| `enum_specifier` | ENUM | `repo:file#name:SYMBOL:ENUM` |
| `type_definition` | TYPE_ALIAS | `repo:file#name:SYMBOL:TYPE_ALIAS` |

Visibility:
- `static` functions/variables → PRIVATE
- Everything else → EXPORT (C default external linkage)

## Edge Types

### IMPORTS (from `#include`)

```c
#include <stdio.h>      // system include, isRelative=false
#include "myheader.h"   // local include, isRelative=true
```

Target key: the include path (e.g., `stdio.h`, `myheader.h`)

### CALLS (from function calls)

```c
int main() {
    foo();           // target_key = "foo"
    bar(1, 2);       // target_key = "bar"
    ptr->method();   // UNRESOLVED (function pointer)
}
```

Direct identifier calls only. Function pointer calls and macro-generated
calls are marked unresolved. Honest about what can't be resolved syntactically.

## Stable Key Contract

Same format as ts-extractor:
```
repo:file_path#name:SYMBOL:subtype
repo:file_path#name:SYMBOL:subtype:dup2  (second occurrence)
repo:file_path#name:SYMBOL:subtype:dup3  (third occurrence)
```

Port the `:dupN` disambiguation logic from ts-extractor. C files can have
multiple same-name static functions (in different `#ifdef` blocks, etc.).

## Complexity Metrics

Port cyclomatic complexity calculation from ts-extractor.

C control flow constructs:
- `if` / `else if` / `else`
- `for` / `while` / `do-while`
- `switch` / `case`
- `&&` / `||` / `?:`
- `goto` (adds complexity)

## Integration

Wire into `repo-index/compose.rs`:
- Add `CExtractor` alongside `TsExtractor`
- Route `.c` / `.h` files to `CExtractor`
- Keep `.ts` / `.js` / `.tsx` / `.jsx` routing to `TsExtractor`

## Validation Targets

After implementation:
1. `sqlite` — compact, pure C, well-structured
2. `nginx` — systems code, module architecture
3. `swupdate` — embedded/safety-critical patterns

## Success Criteria

1. `rmap index` completes on sqlite/nginx/swupdate
2. `rmap hotspots` produces credible rankings
3. `rmap callers` / `rmap callees` return results (if CALLS included)
4. No stable-key collisions
5. Unresolved-edge rate is measurable and documented

## C++ Plan (Explicit Deferral)

C++ is NOT in this slice. It is a separate extractor frontier because:
- Materially different symbol model (classes, methods, namespaces, templates)
- Call resolution is harder (overloads, ADL, member dispatch)
- Would blur validation signal if combined with C v1

C++ sequence (after C validation):
1. C++ design slice — symbol identity, call resolution posture, template policy
2. C++ extractor v1 — separate `cpp-extractor` crate
3. C++ validation — repos TBD

## Validation Fixtures (Before Real Repos)

Create focused extractor tests before jumping to `sqlite`:

1. **Duplicate same-name functions** — repeated `static` functions in one file
2. **Header + source** — prototype in `.h`, definition in `.c`, only definition creates symbol
3. **Nested control flow** — verify complexity calculation
4. **Local vs system include** — `"foo.h"` vs `<stdio.h>` edge differences
5. **Function pointer exclusion** — `ptr()` marked unresolved, not as CALLS
6. **Anonymous struct/enum** — `anon_struct`, `anon_enum` naming
7. **Macro-heavy file** — parses partially, extracts what's visible
8. **Multiple definitions same name** — `:dupN` disambiguation

These tests will save time before the first real large-repo run.

## Implementation Substeps

1. **Crate skeleton** — Cargo.toml, lib.rs, tree-sitter-c dependency
2. **Parser setup** — initialize tree-sitter, parse C source
3. **FILE node extraction** — one per file
4. **Function extraction** — function_definition only (not declarations)
5. **Struct/enum/typedef extraction** — tagged types, anonymous naming
6. **Include extraction** — preproc_include → IMPORTS edge
7. **Call extraction** — plain identifier calls only, exclude function pointers
8. **Complexity metrics** — cyclomatic complexity per pinned rules
9. **Stable-key disambiguation** — :dupN for duplicates
10. **Validation fixtures** — unit tests per fixture list above
11. **Integration** — wire into compose.rs, route `.c`/`.h` files
12. **Validation** — sqlite, nginx, swupdate
