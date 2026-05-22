# Refresh Path: Algorithms and Data Structures Deep Dive

**Created:** 2026-05-21  
**Context:** RMAPD-PERF-2 investigation

## Overview

The refresh path updates an indexed repository incrementally by:
1. Re-scanning the file system
2. Comparing with previous snapshot to identify changed files
3. Re-extracting changed files only
4. Copying forward artifacts from unchanged files
5. Persisting new module/boundary data

## Phase 1: File System Scan (`prepare_repo_inputs`)

**Algorithm:** Recursive directory traversal with filtering  
**Data Structure:** Vec<FileInput>  
**Complexity:** O(F) where F = total files in repo

```
Input: repo_path
Output: PreparedInputs { file_inputs, config_file_inputs, ... }

1. Walk directory tree recursively
2. For each file:
   - Check if excluded by .gitignore / .rmap-ignore
   - Check if test file (path heuristics)
   - Check if generated (path heuristics)
   - Compute content hash (SHA-256)
   - Detect language from extension
3. Collect into Vec<FileInput>
```

**Data:**
- Django: 3015 files scanned
- Time: ~4 seconds

## Phase 2: Invalidation Planning (`orchestrator::refresh_repo`)

**Algorithm:** Hash comparison against previous snapshot  
**Data Structure:** HashMap<String, String> (path → hash)  
**Complexity:** O(F) file comparisons

```
Input: file_inputs, previous_snapshot
Output: changed_files[], unchanged_files[]

1. Load previous snapshot's file hashes from DB
2. For each current file:
   - If not in previous: mark ADDED
   - If hash differs: mark CHANGED  
   - If hash same: mark UNCHANGED
3. For each previous file not in current: mark DELETED
```

**Data:**
- Django: 3015 files, all unchanged in no-change refresh
- Time: included in core_refresh (~62 seconds)

## Phase 3: Extraction (per changed file)

**Algorithm:** Tree-sitter parsing per language  
**Data Structure:** AST → nodes[], edges[]  
**Complexity:** O(C * S) where C = changed files, S = avg symbols per file

```
For each changed file:
1. Route to language extractor (TS, Python, Java, etc.)
2. Parse to AST via tree-sitter
3. Walk AST, emit nodes (functions, classes, etc.)
4. Walk AST, emit edges (calls, imports, etc.)
5. Collect into batch for persistence
```

**Data:**
- Django no-change: 0 files extracted
- Django full index: 3015 files, 81934 nodes, 81045 edges

## Phase 4: Copy-Forward Artifacts

### 4a. Measurements Copy-Forward

**Algorithm (BEFORE FIX - O(F * M)):**
```
For each unchanged_file in 3015 files:
    Execute SQL:
        INSERT INTO measurements
        SELECT ... FROM measurements
        WHERE target_stable_key LIKE 'repo:path#%'
```
- 3015 SQL executions, each ~23ms = 69 seconds

**Algorithm (AFTER FIX - O(M + F)):**
```
1. CREATE TEMP TABLE _unchanged_files_m
2. INSERT all 3015 paths into temp table: O(F)
3. Single SQL:
   INSERT INTO measurements
   SELECT ... FROM measurements
   WHERE SUBSTR(target_stable_key, 32, 
         INSTR(target_stable_key, '#') - 32)
         IN (SELECT path FROM _unchanged_files_m)
```
- 1 SQL execution, full table scan with SUBSTR evaluation
- 121K measurements scanned: ~34 seconds

**Why still O(M):** SQLite cannot index SUBSTR expressions. Every row
in measurements table has SUBSTR evaluated. With 121K rows, this is
121K string operations.

### 4b. Inferences Copy-Forward

Same algorithm as measurements. Django has 0 inferences.

### 4c. Boundary Surfaces Copy-Forward

**Algorithm:** Temp table join (already optimized)
```
1. CREATE TEMP TABLE _unchanged_files
2. INSERT paths
3. Single SQL with JOIN on source_file column
```

**Data:** Django has 0 boundary surfaces.

## Phase 5: Impact Propagation

**Algorithm:** Dependency traversal (skipped when no changes)  
**Data Structure:** provenance_json links

```
If changed_files is empty:
    Skip impact propagation
Else:
    1. Query all nodes for changed files: O(N) where N = total nodes
    2. Filter by stable_key prefix matching: O(N * C) naive
    3. For each impacted node, mark derived artifacts as stale
```

**Data:**
- Django no-change refresh: skipped (0 changed files)

## Phase 6: Postpass Extraction

Re-extracts derived artifacts from changed files:
- Spring liveness inferences
- Policy facts (STATUS_MAPPING, BEHAVIORAL_MARKER, RETURN_FATE)
- Boundary interactions

**Algorithm:** Tree-sitter re-parse (per changed file)  
**Complexity:** O(C) where C = changed files

**Data:**
- Django no-change: ~2 seconds (mostly setup overhead)

## Phase 7: Module Persistence

**Algorithm:** Manifest parsing + file ownership assignment  
**Data Structure:** module_candidates, module_file_ownership tables

```
1. persist_cargo_modules: O(cargo_modules * rust_files)
2. persist_npm_modules: O(npm_modules * js_files)
3. persist_pyproject_modules: O(pyproject_modules * py_files)
4. persist_gradle_modules: O(gradle_modules * jvm_files)
5. persist_inferred_modules: O(inferred * uncovered_files)
```

Each uses longest-prefix-match algorithm:
```
For each file:
    For each module (sorted by path length desc):
        If file.path starts with module.root:
            Assign file to module
            Break
```

**Data:**
- Django: 2 declared modules, ~700ms total

## Timing Summary (Django, 3015 files, 121K measurements)

**Run 1:** After batched query fix, before `:FILE` extraction bug fix.
Measurements: 121015 copied. Total: ~102s.

| Phase | Time | Algorithm | Bottleneck |
|-------|------|-----------|------------|
| Scan | 4s | Dir walk + hash | I/O bound |
| Core refresh | 62-72s | Hash compare + DB setup | DB transactions |
| Copy measurements | 34s | Full table scan | SUBSTR evaluation |
| Copy inferences | <1s | (no data) | - |
| Copy boundaries | <1s | (no data) | - |
| Impact propagation | 0s | (skipped) | - |
| Postpass | 2s | Setup overhead | - |
| Module persist | 0.7s | Prefix match | - |
| **Total** | **~100s** | | |

**Note:** Times vary 84-102s across runs due to I/O variance, cache state, and
whether daemon was freshly started. The order-of-magnitude improvement from
6+ minutes to ~100 seconds is the significant signal.

## Key Observations

1. **Scan is I/O bound:** Cannot optimize without caching
2. **Core refresh is dominated by DB setup:** Creating new snapshot, transaction overhead
3. **Copy-forward is the main CPU bottleneck:** Even batched, 121K SUBSTR evaluations take 34s
4. **Module persistence is O(F * M):** Sorted modules enable early-break but still linear scan per file

## Potential Optimizations

### Implemented (RMAPD-PERF-2)
- Batched temp-table queries for copy-forward: 69s → 34s

### Deferred (RMAPD-PERF-2B)
- Add `anchor_file_path` column with index
- Would enable O(log N) lookups instead of O(N) full scan
- Trade-off: Storage overhead, schema migration complexity

### Not Investigated
- Parallel extraction (multi-threaded tree-sitter parsing)
- Incremental hashing (git-based change detection)
- Lazy artifact copy (copy-on-read instead of copy-on-refresh)
