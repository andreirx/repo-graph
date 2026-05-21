# MODULE-BOUNDARY-FIX-1: Module/Boundary Command DTO Fixes

**Status:** COMPLETE (2026-05-21)  
**Type:** Bug Fix / DTO Alignment  
**Prerequisite:** CLI-AUDIT-1 (defects discovered)

## Problem Statement

CLI-AUDIT-1 discovered three DTO mismatches between the CLI presentation layer and daemon responses. These cause commands to fail or render with empty fields.

## Root Cause Analysis

### Defect 1: boundaries summary (CRITICAL)

**Symptom:** `failed to parse boundaries summary response: invalid type: string "...", expected struct FileWithBoundaries`

**Root cause:** Two DTO shape mismatches:

1. **CategoryCount fields**: Daemon sends category-specific field names, CLI expects generic `category`.
   
   Daemon sends:
   ```json
   {"channelKind": "amqp_queue", "count": 6}
   {"boundaryScope": "inter_process", "count": 31}
   ```
   
   CLI DTO expects:
   ```rust
   struct CategoryCount { category: String, count: u64 }
   ```

2. **filesWithBoundaries**: Daemon sends array of strings, CLI expects objects.
   
   Daemon sends:
   ```json
   "filesWithBoundaries": ["path/to/file.ts", "another/file.c"]
   ```
   
   CLI DTO expects:
   ```rust
   struct FileWithBoundaries { file_path: String, boundary_count: u64 }
   ```

### Defect 2: boundaries list (HIGH)

**Symptom:** Output shows `bidirectional - ` with all other fields empty.

**Root cause:** Field naming convention mismatch. Daemon uses camelCase, DTO expects snake_case but has no `#[serde(rename)]` annotations.

| Daemon field | CLI DTO field | Status |
|--------------|---------------|--------|
| `channelKind` | `channel_kind` | MISSING rename |
| `boundaryScope` | `boundary_scope` | MISSING rename |
| `protocolFamily` | `protocol_family` | MISSING rename |
| `sourceFile` | `file_path` | MISSING rename |
| `surfaceUid` | `surface_uid` | MISSING rename |
| `symbolStableKey` | `symbol_key` | MISSING rename |
| `direction` | `direction` | OK (same name) |
| `confidence` | `confidence` | OK (same name) |
| `basis` | `basis` | OK (same name) |

### Defect 3: modules deps (HIGH)

**Symptom:** Output shows `-> (89 imports from 34 files)` without source/target module names.

**Root cause:** Field naming mismatch in both edge and diagnostics DTOs.

**ModuleDependencyEdge:**
| Daemon field | CLI DTO field | Status |
|--------------|---------------|--------|
| `source` | `source_module` | MISSING rename |
| `target` | `target_module` | MISSING rename |
| `import_count` | `import_count` | OK |
| `source_file_count` | `source_file_count` | OK |

**ImportDiagnostics:**
| Daemon field | CLI DTO field | Status |
|--------------|---------------|--------|
| `imports_total` | `total_import_edges` | MISSING rename |
| `imports_cross_module` | `cross_module_edges` | MISSING rename |
| `imports_intra_module` | `intra_module_edges` | MISSING rename |
| `imports_source_unowned` | `from_unowned_edges` | MISSING rename |

## Design Decision

**Fix CLI DTOs, not daemon.**

Rationale:
- Daemon contracts should remain stable (other clients may depend on them)
- CLI presentation layer should adapt to domain contract
- Adding `#[serde(rename)]` is non-breaking for daemon

## Implementation Plan

### Fix 1: boundaries_summary.rs

1. Create category-specific structs with proper field names:
   ```rust
   struct ChannelKindCount { #[serde(rename = "channelKind")] kind: String, count: u64 }
   struct BoundaryScopeCount { #[serde(rename = "boundaryScope")] scope: String, count: u64 }
   // etc.
   ```

2. Change `filesWithBoundaries` to `Vec<String>` (daemon sends paths only, no counts).

3. Update renderer to work with new DTO structure.

### Fix 2: boundaries_list.rs

Add `#[serde(rename = "...")]` annotations to all mismatched fields in `BoundaryListEntry`.

### Fix 3: modules_deps.rs

1. Add `#[serde(rename = "...")]` to `ModuleDependencyEdge` fields.
2. Add `#[serde(rename = "...")]` to `ImportDiagnostics` fields.

## Files in Scope

- `rust/crates/rgr/src/presentation/boundaries_summary.rs`
- `rust/crates/rgr/src/presentation/boundaries_list.rs`
- `rust/crates/rgr/src/presentation/modules_deps.rs`

## Files Explicitly Out of Scope

- Daemon handlers (rmapd)
- Command layer (already correct)
- Other presentation modules

## Validation Commands

```bash
# After each fix, rebuild and test
./scripts/dev-install-local.sh

# Defect 1: boundaries summary should render
cd /path/to/repo-graph && rmap boundaries summary

# Defect 2: boundaries list should show full details
cd /path/to/repo-graph && rmap boundaries list

# Defect 3: modules deps should show target module names
cd /path/to/leveldb && rmap modules deps db
```

## Definition of Done

- [x] boundaries summary parses without error and renders all categories
- [x] boundaries list shows channel_kind, direction, scope, protocol, identity
- [x] modules deps shows both source and target module names
- [x] All existing unit tests pass
- [x] Smoke test on real corpus (repo-graph, leveldb)

## Validation Evidence (2026-05-21)

### boundaries summary (repo-graph)
```
Boundaries Summary

72 surfaces
70 channels

By channel kind:
  14  inter_core_channel
  14  shared_memory
  10  semaphore
  ...

Files with boundaries:
  test/fixtures/amqp-basic/consumer.ts
  test/fixtures/grpc-java-minimal/src/main/java/...
  ...
```

### boundaries list (grpc-java)
```
Boundaries

116 boundaries

  grpc_channel  consumer  unknown  rpc  alts/src/test/java/io/grpc/alts/HandshakerServiceChannelTest.java
  grpc_channel  consumer  unknown  rpc  authz/src/test/java/io/grpc/authz/AuthorizationEnd2EndTest.java
  ...
```

### modules deps (leveldb)
```
Module Dependencies

Queried: all directions
Module: db

6 dependency edges

  db -> include  (89 imports from 34 files)
  db -> port  (16 imports from 11 files)
  db -> table  (5 imports from 2 files)
  db -> util  (48 imports from 26 files)
  helpers -> db  (1 imports from 1 files)
  table -> db  (3 imports from 1 files)
```

## Technical Debt

None introduced. This fix aligns CLI DTOs with daemon contract.
