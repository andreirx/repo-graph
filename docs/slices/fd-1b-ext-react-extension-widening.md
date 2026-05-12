# FD-1B-EXT: React Detector Extension Widening

Status: IMPLEMENTED (2026-05-12)
Type: Feature
Depends: FD-SUPPORT-EXT-JSTS (IMPLEMENTED)
Blocks: None

## Implementation Summary

Widened React detector hook detection gate from `.tsx`/`.jsx` only to the full JS/TS extension family. Component detection remains narrow (TSX/JSX only).

### Artifacts

- `rust/crates/repo-index/src/react_detector.rs` — widened hook gate, updated grammar selection
- `test/fixtures/typescript/react-frontend-corpus/HooksInTsFile.ts` — validation corpus file

### Validation Results (EXECUTED)

- Hook count increased from 14 to 17 after widening
- `HooksInTsFile.ts` (plain `.ts` file) produces 3 hook inferences:
  - `useState` in `useCounter`
  - `useEffect` in `useCounter`
  - `useEffect` in `useLogger`
- No false component detection in `.ts` file (expected: none)
- Grammar selection correct: TS grammar for `.ts`, TSX grammar for `.tsx`/`.jsx`

---

## Goal

Widen React detector file gate from `.tsx`/`.jsx` only to the full JS/TS extension family, enabling **hook detection** in:

- `.ts`, `.js`, `.mts`, `.cts`, `.mjs`, `.cjs`

**Important limitation:** This slice enables hook detection only for non-TSX/JSX extensions. Component detection requires JSX syntax, which the TS grammar (used for `.ts`/`.js`/etc.) does not parse. Files in these extensions that actually contain JSX would need JSX pragma detection and grammar switching (out of scope).

In practice, this covers:
- Custom hook definitions in `.ts` files (common pattern)
- Hook usage in non-JSX utility files
- Hook-only React modules

It does NOT cover:
- Components in `.ts`/`.js` files (requires JSX pragma support, deferred)

## Why This Slice Exists

FD-1B is marked IMPLEMENTED but with documented limitations:

> Extension coverage is TSX/JSX only. Current implementation only processes `.tsx` and `.jsx` files.

This is documented in TECH-DEBT.md as a known limitation with a fix path:

> Fix path: Unify JS/TS-family extension contract across routing + extractor + detector, then widen FD-1B gate.

FD-SUPPORT-EXT-JSTS provides the unified contract. This slice consumes it to widen the React detector.

## Problem Analysis

### Why TSX/JSX Only Initially

The first-cut implementation used extension as a proxy for "file contains JSX":

```rust
// react_detector.rs current gate
if !file.rel_path.ends_with(".tsx") && !file.rel_path.ends_with(".jsx") {
    return vec![];
}
```

This is a conservative gate that avoids false positives from non-JSX files.

### Actual Detection Logic

The detector's internal logic already handles files with or without JSX:

1. **React import gate:** File must import from `react` (applies regardless of extension)
2. **Component detection:** Requires JSX return (`has_jsx: true`) for function/arrow styles
3. **Hook detection:** Does not require JSX, just `use*` call patterns

So the extension gate is overly conservative. A file importing `react` with hook usage but no JSX would be correctly detected IF it passed the extension gate.

### Difficulty Analysis

| Scope | Difficulty | Risk |
|-------|------------|------|
| `.ts`/`.js` files with React import | Low | Grammar handles these |
| `.mts`/`.cts`/`.mjs`/`.cjs` | Low | Same grammar, different extensions |
| Files with JSX pragma (no extension hint) | Medium | Need pragma detection |
| Arbitrary files with JSX syntax | High | Grammar mismatch risk |

This slice targets **Low difficulty** scope only. JSX pragma detection is deferred.

## Scope

### In Scope

1. **Update extension gate:**
   Replace hardcoded extension check with `is_jsts_extension()` from FD-SUPPORT-EXT-JSTS.

2. **Validate grammar selection:**
   Ensure files are parsed with correct grammar (TS vs TSX) per `grammar_for_extension()`.

3. **Update corpus:**
   Add test files demonstrating:
   - `.ts` file with React hooks (no JSX)
   - `.js` file with React hooks (no JSX)
   - `.mts` file with React import (if tooling supports)

4. **Preserve detection accuracy:**
   - Component detection still requires `has_jsx: true`
   - Hook detection still requires React import
   - No false positives from non-React files

### Out of Scope

- JSX pragma detection (`/** @jsx */` or `/** @jsxImportSource */`)
- Class components
- Props analysis
- Grammar switching for `.ts`/`.js` files that actually contain JSX (rare, non-standard)

## Implementation

### Current Gate (react_detector.rs)

```rust
pub fn detect_react_components(
    files: &[FileInput],
) -> Vec<ReactComponentDetection> {
    // ...
    for file in files {
        // Current: narrow gate
        if !file.rel_path.ends_with(".tsx") && !file.rel_path.ends_with(".jsx") {
            continue;
        }
        // ...
    }
}
```

### After This Slice

```rust
use repo_graph_indexer::jsts_extensions::{is_jsts_extension, grammar_for_extension, JsTsGrammar};

pub fn detect_react_components(
    files: &[FileInput],
) -> Vec<ReactComponentDetection> {
    // ...
    for file in files {
        let ext = get_extension(&file.rel_path);
        
        // Widened gate: any JS/TS family file
        if !is_jsts_extension(ext) {
            continue;
        }
        
        // Select grammar based on extension
        let grammar = match grammar_for_extension(ext) {
            Some(JsTsGrammar::Tsx) => tsx_language.clone(),
            Some(JsTsGrammar::TypeScript) => ts_language.clone(),
            None => continue, // Should not happen given is_jsts_extension check
        };
        
        // Parse with selected grammar
        parser.set_language(&grammar).ok()?;
        // ...
    }
}
```

### Hook Detection (No Change Needed)

Hook detection does not depend on JSX, so widening the extension gate immediately enables detection in all JS/TS files that import React.

### Component Detection (Automatic Filtering)

Component detection already requires `has_jsx: true` for function/arrow styles:

```rust
// Existing logic in detect_react_components
if is_pascal_case(name) && has_jsx {
    // Emit component inference
}
```

A `.ts` file without JSX syntax will have `has_jsx: false` and correctly produce no component inferences. No additional filtering needed.

## Validation Corpus Extension

Add to `test/fixtures/typescript/react-frontend-corpus/`:

### HooksInTsFile.ts (NEW)

```typescript
import { useState, useEffect } from 'react';

// This is a .ts file with React hooks but no JSX
// Should detect hook usage but NOT component

export function useCounter(initial: number) {
    const [count, setCount] = useState(initial);
    
    useEffect(() => {
        console.log('Count changed:', count);
    }, [count]);
    
    return { count, setCount };
}

export function useLogger(prefix: string) {
    useEffect(() => {
        console.log(prefix, 'mounted');
        return () => console.log(prefix, 'unmounted');
    }, [prefix]);
}
```

### Expected Detection

- `react_hook_usage` for `useState` in `useCounter`
- `react_hook_usage` for `useEffect` in `useCounter` (x2, one per usage)
- `react_hook_usage` for `useEffect` in `useLogger`
- NO `react_component` inferences (no JSX return)

## Validation Commands

```bash
# 1. Build
cd rust && cargo build -p repo-graph-repo-index

# 2. Unit tests
cargo test -p repo-graph-repo-index react_detector

# 3. Index extended corpus
rmap index test/fixtures/typescript/react-frontend-corpus ./test-artifacts/fd-1b-ext.db

# 4. Verify hook detection in .ts file
rmap inferences list ./test-artifacts/fd-1b-ext.db react-frontend-corpus --kind react_hook_usage \
  | grep "HooksInTsFile.ts"
# Expected: hook usages found

# 5. Verify no false component detection in .ts file
rmap inferences list ./test-artifacts/fd-1b-ext.db react-frontend-corpus --kind react_component \
  | grep "HooksInTsFile.ts"
# Expected: no matches (hooks-only file should not produce component inferences)
```

## Acceptance Criteria

1. Extension gate uses `is_jsts_extension()` from FD-SUPPORT-EXT-JSTS
2. Grammar selection uses `grammar_for_extension()` from FD-SUPPORT-EXT-JSTS
3. `.ts` files with React hooks are detected
4. `.js` files with React hooks are detected
5. `.ts`/`.js` files without JSX do not produce false component inferences
6. Existing `.tsx`/`.jsx` detection unchanged
7. All existing tests pass
8. New corpus files added and validated

## Definition of Done

- Extension gate widened using shared utilities
- Grammar selection correct per extension
- Corpus extended with `.ts` hook-only file
- All validation commands pass
- No regressions in existing tests
- Slice doc updated to IMPLEMENTED

## Deferred

- JSX pragma detection (requires AST scanning for pragma comments)
- Class components
- Props analysis
- `.mts`/`.cts`/`.mjs`/`.cjs` corpus files (covered by extension handling, no unique behavior)

## Estimated Effort

Small slice. Primarily gate change and corpus extension.

- Gate update: 1 hour
- Grammar selection update: 1 hour
- Corpus extension: 1 hour
- Validation: 1-2 hours

Total: ~0.5 day (assuming FD-SUPPORT-EXT-JSTS is complete)
