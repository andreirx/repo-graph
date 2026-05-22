# ORIENT-BUG-1: Module Count Mismatch

**Status:** COMPLETE (2026-05-21)  
**Type:** Bug / Domain Model Correction  
**Priority:** Closed  
**Discovered:** CLI-OUT-2A audit (2026-05-18)

**Scope completed:**
- Module count alignment: orient/trust now consume same `module_candidates` source
- Refresh performance regression (RMAPD-PERF-2): batched copy-forward queries (6+ min → ~100s)
- Bug fix: `:FILE` key extraction off-by-one (-5 → -4)

**Out of scope (separate slice):**
Client timeout failure (EAGAIN / os error 35) traced to daemon-client transport layer.
Not a module-model or refresh-algorithm bug. See `RMAP-IO-1`.

## Problem Statement

`orient` reports dramatically wrong module counts compared to `trust`.

| Repo | orient Shows | trust Shows | Delta |
|------|-------------|-------------|-------|
| repo-graph | 1 | 281 | -99.6% |
| OpenXcom | 2 | 19 | -89% |
| DuckDB | 17 | 240+ | -93% |
| Django | 2 | 100+ | -98% |
| Buildroot | 5 | ~20 | -75% |
| grpc-java | 42 | 42+ | OK? |

## Root Cause — IDENTIFIED (2026-05-21)

Two competing definitions of "module" in the system:

| Source | Populated by | Used by |
|--------|--------------|---------|
| `module_candidates` table | Manifest parsers + inferred module detection | orient (via `get_module_summary()`) |
| `nodes` table (kind=MODULE) | Indexer package detection | trust (via `compute_module_stats()`) |

**The discrepancy:**
- `module_candidates` is sparsely populated (only manifest-based + directory heuristics)
- `nodes` MODULE entries are created for every package the indexer detects
- These two populations are not synchronized

**On repo-graph:**
- orient (`module_candidates`) → 1 declared module
- trust (`nodes` MODULE) → 281 modules

## Evidence

```bash
# orient MODULE_SUMMARY signal (with --budget large to avoid truncation)
rmap orient --budget large --json | jq '.signals[] | select(.code == "MODULE_SUMMARY") | .evidence'
{
  "discovered_module_count": 1,
  "module_kinds": { "declared": 1, "operational": 0, "inferred": 0 }
}

# trust module count
rmap trust --json | jq '.modules | length'
281
```

## Why This Is Not a Renderer Issue

This is a **domain model inconsistency**, not a presentation defect.

The system has two unreconciled sources of truth for "module":
1. Layer 0: indexed MODULE nodes in `nodes` table
2. Layer 1/2: `module_candidates` projection

User-facing commands should not arbitrarily choose between them.

## Implemented Fix: Option C (Hybrid)

The fix required three coordinated changes:

### 1. Deep Manifest Discovery

**Problem:** Cargo extraction only looked at root `Cargo.toml`. Repos with Rust code in subdirectories (e.g., `rust/Cargo.toml`) had their crates missed.

**Fix:** Scan ALL `Cargo.toml` files anywhere in tree. For each workspace root found, expand members relative to that root's directory. Collect standalone crates not discovered via workspace. Same pattern applied to npm.

### 2. Ecosystem-Scoped Coverage

**Problem:** A root npm `package.json` at "." would suppress all inferred detection, even for Rust files that npm doesn't semantically own.

**Fix:** Coverage is now ecosystem-scoped:
- Cargo modules only cover `.rs` files
- npm modules only cover `.js/.ts/.jsx/.tsx/.mjs/.cjs` files
- Python modules only cover `.py/.pyi` files
- Gradle modules only cover `.java/.kt/.scala` files

A `ModuleEcosystem` enum and `DeclaredRoot` struct track each declared module's ecosystem.

### 3. Language-Filtered File Ownership

**Problem:** Cargo modules were claiming ownership of ALL files under their root, causing duplicate ownership with npm modules in mixed-language repos.

**Fix:** Each ecosystem's persist function filters to only its language:
- `persist_cargo_modules`: only `.rs` files
- `persist_npm_modules`: only JS/TS files (was already filtered)
- `persist_pyproject_modules`: only Python files (was already filtered)
- `persist_gradle_modules`: only JVM files (was already filtered)
- `persist_inferred_modules`: only files NOT covered by any declared module

### 4. Trust Query Rewrite

**Problem:** Trust's `compute_module_stats` started from MODULE nodes in `nodes` table (directory-based), not from `module_candidates` (semantic model).

**Fix:** Rewrote query to start from `module_candidates`:
- Uses `module_file_ownership` for file counts (not OWNS edges)
- LEFT JOINs to MODULE nodes for fan_in/fan_out where available
- Synthesizes stable_key from repo_uid + canonical_root_path

### Classification Rules

| Source | module_kind | Confidence | File Ownership |
|--------|-------------|------------|----------------|
| Cargo.toml manifest | declared | 1.0 | .rs files only |
| package.json manifest | declared | 1.0 | JS/TS files only |
| pyproject.toml manifest | declared | 1.0 | Python files only |
| settings.gradle manifest | declared | 1.0 | JVM files only |
| Directory heuristic (gap-fill) | inferred | 0.7 | Uncovered files only |

### Shared Read Model

Both `orient` and `trust` now consume `module_candidates`:
- orient: via `get_module_summary()` which queries module_candidates
- trust: via rewritten `compute_module_stats()` which starts from module_candidates

This eliminates the dual-truth about "how many modules."

## Implementation Plan

### Phase 1: Investigation (COMPLETE)
- [x] Identify data sources used by orient vs trust
- [x] Confirm discrepancy is in data, not rendering
- [x] Quantify discrepancy on corpus repos
- [x] Find the skip logic in compose.rs (lines 454-464)

### Phase 2: Design (COMPLETE)
- [x] Choose Option C: hybrid (deep discovery + ecosystem-scoped coverage)
- [x] Define coverage rule: ecosystem-scoped subtree (Cargo covers .rs, npm covers JS/TS, etc.)
- [x] Define three-stage model: deep discovery, ecosystem-scoped coverage, per-ecosystem gap-fill

### Phase 3: Implementation (COMPLETE)
- [x] Deep manifest discovery: find all Cargo.toml anywhere in tree, not just root
- [x] Ecosystem-scoped coverage: ModuleEcosystem enum, DeclaredRoot struct
- [x] Language-filtered file ownership: Cargo only owns .rs files, npm only owns JS/TS, etc.
- [x] Trust query rewrite: use module_candidates as source, file_ownership for counts
- [x] Inferred modules only own files not covered by declared modules

### Phase 4: Validation (COMPLETE)
- [x] Dual-count comparison on corpus repos
- [x] orient and trust module counts align with module_candidates
- [x] Module-kind breakdown accurate (declared vs inferred)
- [x] Verified on repo-graph (41/41), OpenXcom (2/2), Django (2/2)

### Historical Note: Refresh Choking (RESOLVED)
During validation, refresh commands on large repos (Django) experienced hangs with high CPU.
Symptom: daemon at 37% CPU for 7+ minutes, refresh never completing.

**Root cause identified and fixed:**
- RMAPD-PERF-2: Copy-forward queries were O(N) per file. Fixed via batched temp-table approach.
- RMAP-IO-1: Client misclassified socket timeout (EAGAIN) as fatal error. Fixed via Timeout variant.

Django refresh now completes in ~100 seconds.

## Existing Fallback Path

The codebase already has `get_module_nodes_as_candidates()` (storage/src/crud/module_candidates.rs:199-238) which converts MODULE nodes to ModuleCandidate format. This proves the system already recognizes the need to bridge these models.

Current behavior:
- Returns MODULE nodes with `module_kind = "directory"`
- Used as fallback when `module_candidates` is empty
- NOT used as primary population path

## Definition of Done

- [x] Single canonical module count visible to both orient and trust
- [x] Module-kind breakdown (declared/operational/inferred) accurate
- [x] No dual-truth about "how many modules"
- [x] Provenance visible for each module's classification
- [x] Smoke validation on corpus repos (repo-graph, OpenXcom, Django)
- [ ] Refresh behavior not regressed
  - [x] RMAPD-PERF-2: Batched copy-forward queries (6+ min → ~100s)
  - [x] Bug fix: `:FILE` key extraction off-by-one
  - [ ] EAGAIN / os error 35: Transport retry handling (NOT ADDRESSED)

## Files Modified

- `rust/crates/repo-index/src/compose.rs` — deep manifest discovery, ecosystem coverage, language-filtered ownership, timing instrumentation
- `rust/crates/storage/src/trust_impl.rs` — rewritten `compute_module_stats()` to use module_candidates
- `rust/crates/storage/src/refresh_copy_forward_impl.rs` — batched copy-forward queries (RMAPD-PERF-2 fix)

## Files NOT Modified (contrary to original scope)

- `rust/crates/storage/src/queries.rs` — has a separate `compute_module_stats()` that is UNUSED (trust uses trust_impl.rs version via trait)
- `rust/crates/storage/src/crud/module_candidates.rs` — no changes needed
- `rust/crates/indexer/src/` — no changes needed

## Files Out of Scope

- `rust/crates/rgr/src/presentation/` (renderer — not the bug location)
