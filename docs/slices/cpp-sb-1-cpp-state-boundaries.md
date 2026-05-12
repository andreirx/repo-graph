# CPP-SB-1: C++ State Boundaries

Status: NOT STARTED (blocked by C-SB-1)
Depends: C-SB-1 (ACTIVE), cpp-extractor (SHIPPED)
Track: C/C++ Systems Maturation

## Goal

Emit `ResolvedCallsite` facts from the C++ extractor for state-boundary-relevant
APIs, enabling the state-boundary substrate to process C++ code.

## Scope (Preliminary)

### In Scope

1. **C++ standard library file streams:**
   - `std::ifstream` constructor → read
   - `std::ofstream` constructor → write
   - `std::fstream` constructor → read_write (with mode flags)

2. **Qualified name matching:**
   - `std::ifstream::open(path)`
   - Constructor calls with string literal path argument

3. **CppAdapter in state-extractor:**
   - Separate from `CAdapter` (different actor)
   - Handles `std::` namespace prefixes
   - Constructor-as-callsite pattern

### Out of Scope

- C-style APIs in C++ files (handled by C-SB-1 bindings)
- `std::filesystem` (C++17, lower priority)
- Boost.Asio, Qt file APIs (third-party)

## Bindings (Preliminary)

```toml
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
symbol_path   = "ofstream"
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
```

## Blocked By

C-SB-1 must ship first to:
1. Validate the direct-name-matching approach works for native code
2. Establish `Language::C` and `Language::Cpp` as separate variants
3. Prove the substrate handles non-import-based languages

## Validation Target

A C++ codebase with `std::fstream` usage. Candidates:
- buildroot (mixed C/C++)
- A dedicated C++ validation corpus

## Notes

This is a placeholder slice. Full execution-grade specification will be
written after C-SB-1 ships and validates the approach.
