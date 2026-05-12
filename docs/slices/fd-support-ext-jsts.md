# FD-SUPPORT-EXT-JSTS: Unified JS/TS Extension Contract

Status: IMPLEMENTED (2026-05-12)
Type: Support
Depends: None
Unblocks: FD-1B-EXT (React detector extension widening)

## Implementation Summary

Unified JS/TS extension contract created in `jsts_extensions.rs`. All routing, extractor, and detector components migrated to use shared utilities.

### Artifacts

- `rust/crates/indexer/src/jsts_extensions.rs` — unified extension contract (31 tests)
- `rust/crates/indexer/src/routing.rs` — migrated to extended family (62 tests)
- `rust/crates/ts-extractor/src/extractor.rs` — migrated grammar selection
- `rust/crates/repo-index/src/express_detector.rs` — migrated file filter
- `rust/crates/repo-index/src/react_detector.rs` — migrated file filter (narrow gate preserved)

### Validation Results (EXECUTED)

- 31 jsts_extensions unit tests pass
- 62 routing unit tests pass (including 8 new extended family tests)
- 191 ts-extractor tests pass
- 10 express_detector tests pass
- 10 react_detector tests pass
- 5 FD-1A integration tests pass
- 5 FD-1B integration tests pass

---

## Goal

Unify the JS/TS-family extension contract across routing, extractor, and detector layers so that all components agree on which extensions constitute the JS/TS family and how to handle each.

## Why This Slice Exists

Multiple components independently define JS/TS extension handling:

| Component | Location | Current Extensions |
|-----------|----------|-------------------|
| Routing | `rust/crates/indexer/src/routing.rs` | `.ts`, `.tsx`, `.js`, `.jsx` |
| TS Extractor | `rust/crates/ts-extractor/src/extractor.rs` | Same + grammar selection |
| Express Detector | `rust/crates/repo-index/src/express_detector.rs` | Same (hardcoded check) |
| React Detector | `rust/crates/repo-index/src/react_detector.rs` | `.tsx`, `.jsx` only |

Missing from all:
- `.mts` (ES Module TypeScript)
- `.cts` (CommonJS TypeScript)
- `.mjs` (ES Module JavaScript)
- `.cjs` (CommonJS JavaScript)

Without a unified contract, widening React detector coverage (FD-1B-EXT) would require ad-hoc patches in multiple places, risking inconsistency.

## Scope

### In Scope

1. **Define canonical JS/TS extension family:**
   ```
   Core:     .ts, .tsx, .js, .jsx
   Extended: .mts, .cts, .mjs, .cjs
   ```

2. **Define grammar selection policy:**
   - TSX grammar: `.tsx`, `.jsx` (contains JSX syntax support)
   - TS grammar: `.ts`, `.mts`, `.cts`, `.js`, `.mjs`, `.cjs`

3. **Create shared extension utilities:**
   - `is_jsts_extension(ext: &str) -> bool`
   - `is_jsts_jsx_extension(ext: &str) -> bool` (for TSX grammar)
   - `jsts_grammar_for_extension(ext: &str) -> Grammar`

4. **Migrate consumers to shared utilities:**
   - `routing.rs`: `is_source_extension`, `detect_language`, `language_to_extensions`
   - `ts-extractor/extractor.rs`: `language_for_file`
   - `express_detector.rs`: file filter
   - `react_detector.rs`: file filter (enables FD-1B-EXT)

5. **Update tests:**
   - Add tests for `.mts`, `.cts`, `.mjs`, `.cjs` handling
   - Ensure existing tests pass

### Out of Scope

- JSX pragma detection (`.ts` files with `/** @jsx */` pragma)
- Babel/esbuild transpiler configuration parsing
- Actually widening React detector to use extended family (that's FD-1B-EXT)

## Architecture Decision: Utility Location

### Option A: In `routing.rs`

Add utilities to existing `rust/crates/indexer/src/routing.rs`.

**Pros:**
- No new files
- Routing already owns extension logic

**Cons:**
- `routing.rs` becomes the dependency for extractors and detectors
- Slight layering concern (indexer depended on by repo-index)

### Option B: New `jsts_extensions.rs` in indexer

Create dedicated module in `rust/crates/indexer/src/jsts_extensions.rs`.

**Pros:**
- Clear separation
- Single source of truth

**Cons:**
- Still in indexer crate

### Option C: New utility crate

Create `rust/crates/lang-extensions/` or similar.

**Pros:**
- Clean dependency graph
- Reusable across any crate

**Cons:**
- Overhead for simple utilities

### Recommendation

**Option B** (new module in indexer crate).

The indexer crate is already depended on by repo-index where the detectors live. A dedicated module makes the contract explicit without adding crate overhead.

## Contract Definition

```rust
// rust/crates/indexer/src/jsts_extensions.rs

/// Canonical JS/TS extension family.
pub const JSTS_EXTENSIONS: &[&str] = &[
    ".ts", ".tsx", ".js", ".jsx",
    ".mts", ".cts", ".mjs", ".cjs",
];

/// Extensions that require TSX grammar (JSX syntax support).
pub const JSTS_JSX_EXTENSIONS: &[&str] = &[".tsx", ".jsx"];

/// Check whether an extension is in the JS/TS family.
pub fn is_jsts_extension(ext: &str) -> bool {
    JSTS_EXTENSIONS.contains(&ext)
}

/// Check whether an extension requires TSX grammar.
pub fn is_jsts_jsx_extension(ext: &str) -> bool {
    JSTS_JSX_EXTENSIONS.contains(&ext)
}

/// Grammar selection for JS/TS files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsTsGrammar {
    TypeScript,  // .ts, .mts, .cts, .js, .mjs, .cjs
    Tsx,         // .tsx, .jsx
}

pub fn grammar_for_extension(ext: &str) -> Option<JsTsGrammar> {
    match ext {
        ".tsx" | ".jsx" => Some(JsTsGrammar::Tsx),
        ".ts" | ".mts" | ".cts" | ".js" | ".mjs" | ".cjs" => Some(JsTsGrammar::TypeScript),
        _ => None,
    }
}
```

## Migration Plan

### Phase 1: Add utilities (non-breaking)

1. Create `rust/crates/indexer/src/jsts_extensions.rs`
2. Add constants and functions
3. Add unit tests
4. Export from `indexer` crate

### Phase 2: Migrate routing.rs

1. Import from `jsts_extensions`
2. Update `is_source_extension` to use `is_jsts_extension` for JS/TS family
3. Update `detect_language` to handle `.mts`, `.cts`, `.mjs`, `.cjs`
4. Update `language_to_extensions` mappings
5. Run existing tests

### Phase 3: Migrate ts-extractor

1. Import from `jsts_extensions`
2. Update `language_for_file` to use `grammar_for_extension`
3. Update `LANGUAGES` constant
4. Run existing tests

### Phase 4: Migrate detectors

1. Express: use `is_jsts_extension` instead of hardcoded check
2. React: keep narrow gate for now (FD-1B-EXT will widen)
3. Run integration tests

## Validation Commands

```bash
# 1. Build
cd rust && cargo build -p repo-graph-indexer

# 2. Unit tests for new module
cargo test -p repo-graph-indexer jsts_extensions

# 3. Verify routing tests still pass
cargo test -p repo-graph-indexer routing

# 4. Verify ts-extractor tests still pass
cargo test -p repo-graph-ts-extractor

# 5. Verify detector tests still pass
cargo test -p repo-graph-repo-index express_detector
cargo test -p repo-graph-repo-index react_detector
```

## Acceptance Criteria

1. `jsts_extensions.rs` module exists with constants and utilities
2. All existing routing tests pass
3. All existing extractor tests pass
4. All existing detector tests pass
5. New tests cover `.mts`, `.cts`, `.mjs`, `.cjs` cases
6. No hardcoded extension lists remain in migrated consumers

## Definition of Done

- Utilities implemented and tested
- Routing migrated
- TS extractor migrated
- Express detector migrated
- React detector prepared (narrow gate preserved, uses utility)
- No regressions in existing tests

## Follow-on

**FD-1B-EXT** can then widen React detector by simply expanding which extensions pass the gate, using the shared utilities.

## Estimated Effort

Small-medium slice. Primarily refactoring with some new code.

- Phase 1 (utilities): 2 hours
- Phase 2 (routing migration): 2 hours
- Phase 3 (extractor migration): 1 hour
- Phase 4 (detector migration): 1 hour
- Testing and validation: 2 hours

Total: ~1 day
