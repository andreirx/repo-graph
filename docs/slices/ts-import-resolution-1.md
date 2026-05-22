# TS-IMPORT-RESOLUTION-1: TypeScript Aliased and Namespace Import Resolution

**Status:** QUEUED  
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

**Root cause:** The `imported_name` field on `ImportBinding` exists but is not populated by ts-extractor. The field is designed for this:
- `import { readFile }` → `identifier = "readFile"`, `imported_name = Some("readFile")`
- `import { readFile as rf }` → `identifier = "rf"`, `imported_name = Some("readFile")`

Currently ts-extractor emits `imported_name = None` for all bindings.

### Gap 2: Namespace Imports

```typescript
import * as fs from "fs";
fs.readFile("path");  // ← callee "fs.readFile" not resolved
```

**Root cause:** The callee identifier is `fs.readFile` (includes namespace prefix). The resolver looks for `fs.readFile` as a symbol name in the target module, which doesn't exist. Need to:
1. Detect namespace import pattern (`import * as X`)
2. At call resolution time, strip the namespace prefix from callee
3. Look up the member name (`readFile`) in the target module

## Current State

**ImportBinding struct** (`classification/src/types.rs`):
```rust
pub struct ImportBinding {
    pub identifier: String,      // Local name ("rf" or "fs")
    pub specifier: String,       // Module specifier ("fs")
    pub is_relative: bool,
    pub location: Option<SourceLocation>,
    pub is_type_only: bool,
    pub imported_name: Option<String>,  // ← exists but unpopulated
}
```

**Resolver behavior** (`indexer/src/resolver.rs`):
- Matches callee identifier against `import_bindings[].identifier`
- Uses `specifier` to find target module
- Does not use `imported_name` for final symbol lookup
- Does not handle namespace prefix stripping

## Scope

**In scope:**
- ts-extractor: Populate `imported_name` for aliased named imports
- ts-extractor: Mark namespace imports distinctly (new field or convention)
- Resolver: Use `imported_name` when resolving to target symbol
- Resolver: Strip namespace prefix for namespace import calls

**Out of scope:**
- Re-exported aliases (`export { foo as bar }`) — separate pattern
- Dynamic imports (`import("./x")`) — separate pattern
- Destructured re-bindings (`const { x } = imported`) — data flow analysis

## Implementation Plan

### Phase 1: Aliased Named Import Resolution

1. **ts-extractor change:**
   - Parse import declaration to extract both local name and original name
   - Populate `imported_name` field in `ImportBinding`
   - tree-sitter query: `import_specifier` node has `name` (original) and optional `alias` children

2. **Resolver change:**
   - When looking up symbol in target module, use `imported_name` if present
   - Fall back to `identifier` if `imported_name` is None

### Phase 2: Namespace Import Resolution

1. **ts-extractor change:**
   - Detect `import * as X` pattern
   - Options:
     a. New field `is_namespace: bool` on ImportBinding
     b. Convention: `imported_name = Some("*")` for namespace imports
   - Decision needed: which approach?

2. **Resolver change:**
   - When callee is `X.member` and `X` matches a namespace import:
     - Strip prefix to get `member`
     - Look up `member` in target module

## Tree-Sitter Query Notes

**Named import with alias:**
```
(import_specifier
  name: (identifier) @original
  alias: (identifier)? @local)
```

**Namespace import:**
```
(namespace_import
  (identifier) @namespace_name)
```

## Definition of Done

- [ ] `imported_name` populated for aliased named imports
- [ ] Aliased imports resolve correctly in call graph
- [ ] Namespace imports identified at extraction time
- [ ] Namespace import calls resolve correctly
- [ ] Self-index resolution rate improvement measured
- [ ] No regression on existing resolution

## Files in Scope

- `rust/crates/ts-extractor/src/extractor.rs` (import extraction)
- `rust/crates/indexer/src/resolver.rs` (call resolution)
- `rust/crates/classification/src/types.rs` (ImportBinding — may need new field)

## Validation

**Test corpus:**
- Self-index (repo-graph TypeScript code)
- Existing TypeScript test fixtures

**Metrics:**
- Call graph resolution rate (currently 33%)
- Specific improvement on aliased/namespace patterns

## Risk Assessment

**Low risk:**
- `imported_name` field already exists in contract
- Changes are additive (new data, not schema change)
- Resolver changes are isolated

**Testing approach:**
- Add targeted test fixtures for aliased imports
- Add targeted test fixtures for namespace imports
- Run resolution rate comparison before/after
