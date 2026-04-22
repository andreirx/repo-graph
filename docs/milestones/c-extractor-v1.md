# C Extractor v1 — Design Slice

Rust-native C extractor to unblock `sqlite`, `nginx`, `swupdate` as real
validation targets.

## Scope

**In scope:**
- FILE nodes
- SYMBOL: functions, structs, enums, typedefs
- IMPORTS edges from `#include` directives
- CALLS edges from direct function call sites
- Cyclomatic complexity metrics
- `:dupN` stable-key disambiguation (port from ts-extractor)

**Out of scope (explicitly deferred):**
- C++ (separate extractor frontier)
- Macro expansion / preprocessor evaluation
- Function pointer call resolution
- Header/source ownership policy
- compile_commands.json integration

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

## Implementation Substeps

1. **Crate skeleton** — Cargo.toml, lib.rs, tree-sitter-c dependency
2. **Parser setup** — initialize tree-sitter, parse C source
3. **FILE node extraction** — one per file
4. **Function extraction** — function_definition → SYMBOL:FUNCTION
5. **Struct/enum/typedef extraction** — tagged types
6. **Include extraction** — preproc_include → IMPORTS edge
7. **Call extraction** — call_expression → CALLS edge
8. **Complexity metrics** — cyclomatic complexity
9. **Stable-key disambiguation** — :dupN for duplicates
10. **Integration** — wire into compose.rs, route C files
11. **Validation** — sqlite, nginx, swupdate
