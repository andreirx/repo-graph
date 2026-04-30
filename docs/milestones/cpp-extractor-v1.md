# C++ Extractor v1 — Design Slice

Rust-native C++ extractor with first-class C ABI boundary evidence.

## Strategic Framing

This slice is NOT "just parse `.cpp` files." It is C++ as another source of
legacy-code relationships:
- seams (especially C/C++ interop boundaries)
- ABI boundaries (`extern "C"`)
- mixed-language ownership
- wrapper/glue modules
- testability barriers

The important architectural fact is often not "there is a class here" but
"this file/module exposes or consumes a C ABI seam."

## Scope

**In scope:**
- FILE nodes for `.cpp`, `.hpp`, `.cc`, `.cxx`, `.hxx` files
- SYMBOL: namespaces, classes, methods, constructors, destructors, functions
- IMPORTS edges from `#include` directives
- CALLS edges from direct call sites (same rules as C extractor)
- IMPLEMENTS edges from base class clauses
- Cyclomatic complexity metrics
- `:dupN` stable-key disambiguation
- **C ABI boundary evidence** via `extern "C"` detection

**Out of scope (explicitly deferred):**
- compile_commands.json integration (Layer 2, separate slice)
- Macro expansion / preprocessor evaluation
- Macro-wrapped linkage detection (`BEGIN_EXTERN_C` / `END_EXTERN_C` patterns)
- Template instantiation tracking
- Overload resolution
- Clangd/libclang semantic enrichment (Layer 3, future)
- Header/source ownership heuristics
- Function pointer call resolution
- ADL (argument-dependent lookup) resolution

## Locked Decisions

### 1. Separate Crate

Create `rust/crates/cpp-extractor/`, separate from `c-extractor`.

Rationale:
- C and C++ have different volatility profiles
- AST shapes diverge significantly (namespaces, classes, constructors, templates, linkage specs)
- Keeping Rust-primary C extractor stable matters
- Cleaner release and test boundaries
- Shared helpers can be extracted to `c-family-common` later if duplication becomes significant

### 2. Declaration vs Definition

**Definitions preferred, but declarations extracted for classes/methods.**

- Function definitions: symbol node created
- Function declarations (prototypes): NO symbol node (same as C extractor)
- Class declarations with body: symbol node created
- Method declarations inside class: symbol node created (forward declarations common in headers)
- Out-of-line method definitions: symbol node created

Rationale: C++ headers commonly contain full class definitions with inline
method bodies. Unlike C, extracting only definitions would miss most of the
class structure.

### 3. Qualified Name Handling

C++ uses `::` for namespace/class scoping. Stable keys preserve qualification:

| Form | Stable Key |
|------|------------|
| Free function | `repo:file#foo:SYMBOL:FUNCTION` |
| Namespaced function | `repo:file#ns::foo:SYMBOL:FUNCTION` |
| Class | `repo:file#MyClass:SYMBOL:CLASS` |
| Namespaced class | `repo:file#ns::MyClass:SYMBOL:CLASS` |
| Method | `repo:file#MyClass::method:SYMBOL:METHOD` |
| Constructor | `repo:file#MyClass::MyClass:SYMBOL:CONSTRUCTOR` |
| Destructor | `repo:file#MyClass::~MyClass:SYMBOL:DESTRUCTOR` |
| Nested class | `repo:file#Outer::Inner:SYMBOL:CLASS` |

Qualified names include all namespace/class prefixes. Out-of-line definitions
(e.g., `void MyClass::method() { }`) use the qualified name from the declarator.

### 4. Inheritance / IMPLEMENTS Edges

Extract IMPLEMENTS edges from `base_class_clause`:

```cpp
class Derived : public Base, private Mixin { };
//              ^^^^^^^^^^^  ^^^^^^^^^^^^^
//              IMPLEMENTS   IMPLEMENTS
```

Edge metadata includes:
- `access_specifier`: `public` | `protected` | `private` | `none`
- `is_virtual`: bool

Multiple inheritance produces multiple IMPLEMENTS edges.

### 5. Template Handling

**Template declarations are traversed, not semantically modeled.**

- `template_declaration` nodes are walked to find the underlying class/function
- Template parameters are NOT extracted
- Template specializations are treated as separate symbols (`:dupN` if same name)
- No instantiation tracking

Rationale: Template instantiation requires semantic analysis. Syntax extraction
captures the template definition; instantiation is a Layer 3 concern.

### 6. C ABI Boundary Evidence

**First-class detection of `extern "C"` linkage specifications.**

Tree-sitter-cpp provides `linkage_specification` nodes for:
```cpp
extern "C" void c_api_function();

extern "C" {
    void another_c_function();
    int c_variable;
}

extern "C++" {
    // explicit C++ linkage (rare, but exists)
}
```

Persist as symbol/file metadata:

| Field | Type | Description |
|-------|------|-------------|
| `language_linkage` | `"c"` \| `"cpp"` \| `null` | Explicit linkage if declared |
| `declared_in_extern_c_block` | `bool` | Symbol is inside `extern "C" { }` block |

**What this enables later:**
- Query: files with C ABI boundaries
- Query: modules with mixed C/C++ interop surfaces
- Heuristic: likely shim/wrapper files (high `extern "C"` density)
- Hot interop zones in a repo

**What this does NOT attempt:**
- Macro-wrapped linkage detection (e.g., `__BEGIN_DECLS`, `EXTERN_C_BEGIN`)
- Inferring linkage from header file patterns
- Tracking `extern "C"` across includes

Direct syntax only. Macro-wrapped patterns require preprocessor awareness.

### 7. CALLS Scope (Same as C)

Include:
- Call expressions where callee is a plain identifier or qualified identifier
- `foo()`, `bar(x)`, `ns::func()`, `obj.method()`, `ptr->method()`

Exclude (no edge emitted):
- Function pointer calls: `ptr()`, `(*fn)()`
- Calls through templates with dependent names

Method calls (`obj.method()`, `ptr->method()`) are extracted with the method
name as target key. Receiver type resolution is a Layer 3 enrichment concern.

### 8. Complexity Rules

Same as C extractor, with C++ additions:

| Construct | Increment |
|-----------|-----------|
| Base | +1 |
| `if` | +1 |
| `else if` | +1 |
| `for` (including range-for) | +1 |
| `while` | +1 |
| `do while` | +1 |
| `case` | +1 |
| `default` | +0 |
| `switch` | +0 |
| `&&` | +1 |
| `\|\|` | +1 |
| `?:` (ternary) | +1 |
| `catch` | +1 |
| `throw` | +0 (not a branch) |

### 9. File Extensions

**In scope:**
- `.cpp`
- `.hpp`
- `.cc`
- `.cxx`
- `.hxx`
- `.h++`

**Out of scope:**
- `.c`, `.h` (handled by C extractor)
- `.inc` files
- `.ipp` files (implementation includes)

**Ambiguous case: `.h` with C++ content**

`.h` files are routed to the C extractor. If a `.h` file contains C++ constructs
(classes, namespaces, templates), the C extractor will fail to parse them
correctly. This is accepted behavior for v1:
- Most C++ projects use `.hpp` for C++ headers
- Mixed C/C++ repos often have separate header conventions
- Heuristic grammar switching is a v2 concern

### 10. Preprocessing Policy

**Source-truth semantics (same as C extractor).**

- Parse source text as-is with tree-sitter-cpp
- Do NOT expand macros
- Do NOT evaluate `#ifdef` / `#ifndef` / `#if`
- Code inside `#ifdef` blocks is parsed if syntactically valid C++
- `#include` guards do not affect extraction

## Crate Structure

New crate: `rust/crates/cpp-extractor/`

```
cpp-extractor/
  Cargo.toml
  src/
    lib.rs           # pub use, crate doc
    extractor.rs     # CppExtractor impl
    metrics.rs       # complexity calculation
    linkage.rs       # extern "C" detection
```

## Dependencies

- `tree-sitter` (0.24.x) — parser runtime
- `tree-sitter-cpp` (0.23.x) — C++ grammar
- `repo-graph-indexer` — ExtractorPort trait
- `repo-graph-classification` — ImportBinding, SourceLocation types

No `tree-sitter-c`. C and C++ extractors are independent.

## Symbol Types

| AST Node | Symbol Subtype | Stable Key |
|----------|----------------|------------|
| `function_definition` | FUNCTION | `repo:file#[qual::]name:SYMBOL:FUNCTION` |
| `class_specifier` | CLASS | `repo:file#[qual::]name:SYMBOL:CLASS` |
| `struct_specifier` | STRUCT | `repo:file#[qual::]name:SYMBOL:STRUCT` |
| `enum_specifier` | ENUM | `repo:file#[qual::]name:SYMBOL:ENUM` |
| `namespace_definition` | (no node) | Namespaces affect qualified names only |
| Method definition | METHOD | `repo:file#Class::method:SYMBOL:METHOD` |
| Constructor | CONSTRUCTOR | `repo:file#Class::Class:SYMBOL:CONSTRUCTOR` |
| Destructor | DESTRUCTOR | `repo:file#Class::~Class:SYMBOL:DESTRUCTOR` |
| `type_alias_declaration` | TYPE_ALIAS | `repo:file#[qual::]name:SYMBOL:TYPE_ALIAS` |

Visibility:
- `static` functions → PRIVATE
- `private:` class members → PRIVATE
- `protected:` class members → PROTECTED
- `public:` class members / default → EXPORT

## Edge Types

### IMPORTS (from `#include`)

Same as C extractor:
```cpp
#include <iostream>     // system include, isRelative=false
#include "myheader.hpp" // local include, isRelative=true
```

### CALLS (from call expressions)

```cpp
void example() {
    foo();              // target_key = "foo"
    ns::bar();          // target_key = "ns::bar"
    obj.method();       // target_key = "method"
    ptr->call();        // target_key = "call"
    std::cout << x;     // target_key = "operator<<"
}
```

### IMPLEMENTS (from base class clause)

```cpp
class D : public B { }; // D IMPLEMENTS B
```

Edge metadata:
```json
{
  "access_specifier": "public",
  "is_virtual": false
}
```

## Metadata Schema

### Symbol Metadata (C ABI evidence)

```json
{
  "language_linkage": "c",
  "declared_in_extern_c_block": true
}
```

Only populated when `extern "C"` is detected. Absent fields mean no explicit
linkage specification (default C++ linkage).

### File Metadata

```json
{
  "has_extern_c_declarations": true,
  "extern_c_symbol_count": 5
}
```

Aggregate counts for quick file-level queries.

## Integration

Wire into `repo-index/compose.rs`:

1. Add `CppExtractor` import
2. Route `.cpp`, `.hpp`, `.cc`, `.cxx`, `.hxx` files to `CppExtractor`
3. Remove "C++ explicitly out of scope" comment
4. Keep `.c`, `.h` routing to `CExtractor`

Language detection for file signals:
- C++ files → `language: "cpp"`
- Manifest signals: empty (no Cargo.toml/package.json equivalent for C++)

## Validation Targets

### Unit Test Fixtures

1. **Namespace nesting** — `ns1::ns2::func` qualified names
2. **Class with methods** — inline and out-of-line definitions
3. **Constructors/destructors** — including qualified `Class::Class()`
4. **Multiple inheritance** — multiple IMPLEMENTS edges
5. **Template wrapper** — class inside `template<>` is extracted
6. **`extern "C"` block** — symbols marked with linkage metadata
7. **`extern "C"` single declaration** — function-level linkage
8. **Mixed linkage file** — some `extern "C"`, some C++ linkage
9. **Access specifiers** — private/protected/public visibility
10. **Duplicate names** — `:dupN` disambiguation

### Smoke Repos

1. **C++11 Deep Dives fixtures** — existing repo, known structure
2. **Mixed C/C++ repo** — real `extern "C"` usage patterns
3. **swupdate** — has some C++ files alongside C

### Parity Tests

Compare Rust `cpp-extractor` output against TS `cpp-extractor.ts` on shared
fixtures:
- Same symbols extracted (names, subtypes, qualified names)
- Same edges (IMPORTS, CALLS, IMPLEMENTS)
- Same complexity metrics

## Success Criteria

1. `rmap index` completes on C++ repos
2. Symbols have correct qualified names
3. IMPLEMENTS edges capture inheritance
4. `extern "C"` boundaries are detectable via metadata
5. Complexity metrics match TS extractor
6. No stable-key collisions
7. Parity tests pass

## Implementation Substeps

1. **Crate skeleton** — Cargo.toml, lib.rs, tree-sitter-cpp dependency
2. **Parser setup** — initialize tree-sitter with C++ grammar
3. **FILE node extraction** — one per C++ file
4. **Namespace traversal** — track current namespace for qualified names
5. **Class extraction** — class_specifier, struct_specifier with body
6. **Method extraction** — inside class body and out-of-line definitions
7. **Constructor/destructor** — special handling for qualified names
8. **Inheritance extraction** — base_class_clause → IMPLEMENTS edges
9. **Template traversal** — walk into template_declaration
10. **Include extraction** — preproc_include → IMPORTS edge
11. **Call extraction** — qualified and unqualified calls
12. **Linkage detection** — linkage_specification → metadata
13. **Complexity metrics** — port from C extractor with C++ additions
14. **Stable-key disambiguation** — :dupN for duplicates
15. **Integration** — wire into compose.rs
16. **Parity tests** — compare against TS extractor
17. **Smoke validation** — C++11 Deep Dives, mixed repos

## Size Estimate

This is a multi-session slice:

| Component | Estimate |
|-----------|----------|
| Crate skeleton + parser | ~50 lines |
| Namespace/class/method extraction | ~400 lines |
| Constructor/destructor handling | ~100 lines |
| Inheritance extraction | ~80 lines |
| Linkage detection | ~100 lines |
| Template traversal | ~50 lines |
| Complexity metrics | ~80 lines (port from C) |
| Stable-key handling | ~60 lines |
| Integration | ~40 lines |
| Unit tests | ~400 lines |
| Parity tests | ~150 lines |

**Total: ~1500 lines**

Budget 2-3 working sessions.

## Risk Assessment

### Managed Risks

- **Separate crate** — blast radius contained
- **TS parity target exists** — known-good reference
- **tree-sitter-cpp is stable** — known grammar
- **`extern "C"` detection is cheap** — direct syntax, no heuristics

### Real Risks

- **Qualified name complexity** — nested namespaces, out-of-line definitions
- **Constructor/destructor declarators** — more nesting than functions
- **Header-heavy repos** — declarations without definitions
- **Access specifier tracking** — state across class body traversal
- **Multiple inheritance edge ordering** — determinism requirement

## Future Slices (Explicit Deferral)

### Layer 2: Build-Context Resolution

- compile_commands.json reader (already exists on TS side)
- Per-translation-unit include paths
- Define propagation for conditional compilation
- Header/source ownership heuristics

### Layer 3: Semantic Enrichment

- Clangd/libclang integration
- Receiver-type resolution for method calls
- Overload resolution
- Template instantiation tracking
- ADL resolution

### C/C++ Interop Support Module

After both C and C++ extractors are stable:
- Query surface for C ABI boundaries
- Module-level interop classification
- Wrapper/shim detection heuristics
- Mixed-language dependency graphs
