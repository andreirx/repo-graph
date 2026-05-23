# TS-IMPORT-RESOLUTION-1: TypeScript Aliased and Namespace Import Resolution

**Status:** COMPLETE (2026-05-23)  
**Type:** Enhancement / Accuracy Improvement  
**Impact:** Call graph resolution (currently 33% on self-index)  
**Discovered:** Extraction parity work, documented in TECH-DEBT.md

## Problem Statement

Two TypeScript import patterns are not resolved, causing caller→callee edges to remain unresolved:

### Gap 1: Aliased Named Imports

```typescript
import { readFile as rf } from "fs";
rf("path");  // ← callee "rf" not resolved to fs.readFile
```

**Root cause:** The extractor correctly populates `imported_name` on `ImportBinding`:
- `import { readFile as rf }` → `identifier = "rf"`, `imported_name = Some("readFile")`

But the resolver does NOT use `imported_name` when looking up symbols in the target module. At line 396 in `resolver.rs`:
```rust
if let Some(candidates) = nodes_by_name.get(target_key) {  // BUG: uses "rf" not "readFile"
```

This is a resolver bug, not an extractor gap.

### Gap 2: Namespace Imports

```typescript
import * as fs from "fs";
fs.readFile("path");  // ← callee "fs.readFile" not resolved
```

**Root cause:** Two issues:
1. **Missing import-kind distinction:** Namespace imports (`import * as X`) and default imports (`import X`) both serialize as `imported_name = None`. The resolver cannot distinguish them.
2. **No member resolution logic:** When callee is `X.member`, the resolver extracts `member` but doesn't scope the lookup to the namespace-imported module.

**Critical semantic difference:**
- `import * as fs from "fs"` — `fs` IS the module namespace object
- `import fs from "fs"` — `fs` is the module's default export (could be any value)

These are NOT interchangeable. Collapsing them creates false positives.

### Gap 3: Default Import Member Access (out of scope)

```typescript
import fs from "fs";
fs.readFile("path");  // ← default export member access
```

This requires modeling the default export's structure. Out of scope for this slice — kept conservative/honest.

## Current State

**ImportBinding struct** (`classification/src/types.rs`):
```rust
pub struct ImportBinding {
    pub identifier: String,      // Local name ("rf" or "fs")
    pub specifier: String,       // Module specifier ("fs")
    pub is_relative: bool,
    pub location: Option<SourceLocation>,
    pub is_type_only: bool,
    pub imported_name: Option<String>,  // Original name for named imports
    // MISSING: explicit import kind (Named/Default/Namespace)
}
```

**Extractor behavior** (`ts-extractor/src/extractor.rs`):
- Named import: `{ X }` → `identifier = "X"`, `imported_name = Some("X")`
- Named import with alias: `{ X as Y }` → `identifier = "Y"`, `imported_name = Some("X")`
- Default import: `import X` → `identifier = "X"`, `imported_name = None`
- Namespace import: `import * as X` → `identifier = "X"`, `imported_name = None`
- Tests confirm all four patterns extract correctly

**Resolver behavior** (`indexer/src/resolver.rs`):
- Matches callee identifier against `import_bindings[].identifier`
- Uses `specifier` to find target module
- Does NOT use `imported_name` for symbol lookup (Gap 1)
- Cannot distinguish default vs namespace imports (Gap 2 blocker)

## Implementation Plan

### Phase 1: Aliased Named Import Resolution (SAFE NOW)

**Resolver fix only.** The extractor is already correct.

In `resolver.rs` line 396, change:
```rust
// Before:
if let Some(candidates) = nodes_by_name.get(target_key) {

// After:
let lookup_name = binding.imported_name.as_deref().unwrap_or(target_key);
if let Some(candidates) = nodes_by_name.get(lookup_name) {
```

**Validation:**
- Unit test: aliased import resolves to original symbol
- Self-index resolution rate measurement before/after

### Phase 2: Import Kind Modeling

Add explicit import-kind to `ImportBinding` before namespace resolution.

**Preferred design:** Enum, not boolean.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImportKind {
    Named,      // import { X } or import { X as Y }
    Default,    // import X from "m"
    Namespace,  // import * as X from "m"
}
```

Extend `ImportBinding`:
```rust
pub struct ImportBinding {
    pub identifier: String,
    pub specifier: String,
    pub is_relative: bool,
    pub location: Option<SourceLocation>,
    pub is_type_only: bool,
    pub imported_name: Option<String>,
    pub kind: ImportKind,  // NEW
}
```

**Extractor changes:**
- `collect_local_identifiers` returns `(identifier, imported_name, kind)` tuples
- Namespace imports: `kind = Namespace`
- Default imports: `kind = Default`
- Named imports: `kind = Named`

### Phase 3: Namespace Import Resolution

**After Phase 2 is complete.**

Resolver changes:
```rust
match binding.kind {
    ImportKind::Named => {
        // Use imported_name for symbol lookup (Phase 1 fix)
        let symbol = binding.imported_name.as_deref().unwrap_or(&binding.identifier);
        lookup_symbol_in_module(symbol, resolved_file_uid)
    }
    ImportKind::Namespace => {
        // Callee is "ns.member" — extract member and lookup in target module
        if let Some(member) = callee.strip_prefix(&format!("{}.", binding.identifier)) {
            lookup_symbol_in_module(member, resolved_file_uid)
        }
    }
    ImportKind::Default => {
        // Conservative: default export structure not modeled
        // Could resolve if member matches an export, but risks false positives
        None
    }
}
```

### Phase 4 (Deferred): Default Import Member Resolution

Requires modeling default export structure. Not in scope.

## Definition of Done

### Phase 1 (Aliased Named Imports)
- [x] `imported_name` populated by extractor (already done)
- [x] Resolver uses `imported_name` for symbol lookup (2026-05-23)
- [x] Unit test: aliased import resolution (2026-05-23)
- [x] Self-index resolution rate: 18.8% → 20.0% (+1.2pp) (2026-05-23)
      Note: Modest gain expected — repo-graph uses few aliased imports.
      Larger gains expected in alias-heavy codebases.

### Phase 2 (Import Kind Modeling)
- [x] `ImportKind` enum added to classification crate (2026-05-23)
- [x] `ImportBinding.kind` field added (2026-05-23)
- [x] ts-extractor populates `kind` correctly (2026-05-23)
- [x] All extractors updated (rust, python, java, c, cpp) (2026-05-23)
- [x] Unit tests updated (222 classification tests pass) (2026-05-23)
- [x] Cross-runtime parity notes updated in TECH-DEBT.md (2026-05-23)

### Phase 3 (Namespace Import Resolution)
- [x] Resolver branches by import kind (2026-05-23)
- [x] Namespace member resolution implemented (2026-05-23)
- [x] Unit test: namespace import resolution (2026-05-23)
- [x] Self-index resolution rate: no change (19.9% → 19.9%) (2026-05-23)
      Note: repo-graph uses few internal namespace imports.
      External module namespace calls (node_modules) not indexed.
- [x] Default imports handled conservatively (2026-05-23)

## Files in Scope

**Phase 1:**
- `rust/crates/indexer/src/resolver.rs` — symbol lookup fix

**Phase 2:**
- `rust/crates/classification/src/types.rs` — add `ImportKind` enum
- `rust/crates/ts-extractor/src/extractor.rs` — populate `kind` field

**Phase 3:**
- `rust/crates/indexer/src/resolver.rs` — branch by import kind

## Risk Assessment

**Phase 1 — Low risk:**
- Pure resolver bug fix
- Extractor already correct
- No schema changes
- Additive change to lookup logic

**Phase 2 — Medium risk:**
- Schema change to ImportBinding
- Requires cross-runtime parity consideration
- TS extractors will emit `null` until ported (acceptable)

**Phase 3 — Medium risk:**
- New resolution logic
- Must not create false positives
- Default import handling must be conservative

## Notes on Parity

The TS extractors (`src/adapters/extractors/typescript/`) currently pass `importedName: null` unconditionally (Fork-1 posture). The new `kind` field will also be null from TS extractors until explicitly ported. The Rust serde attributes handle absent values gracefully.

Parity harness (`test/ts-extractor-parity/`) projects ImportBinding to a fixed field set that excludes new fields. No parity harness impact.
