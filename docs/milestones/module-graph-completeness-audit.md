# Module Graph Completeness Audit

Audit of `rgr modules deps` and `rmap modules violations` production-readiness
following Layer 3 (inferred modules) completion.

## Audit Date

2026-04-23

## Surfaces Inspected

1. `rgr modules deps <repo> [module]` — TS CLI
2. `rmap modules violations <db_path> <repo_uid>` — Rust CLI
3. Module rollups (`queryModuleCandidateRollups`)
4. Storage/query paths for module dependency edges
5. Trust/diagnostic output for degraded module topology

## Data Flow

```
files
  → module_file_ownership (longest-prefix assignment)
    → resolved IMPORTS edges (nodes with file_uid)
      → deriveModuleDependencyEdges (pure core)
        → cross-module edges (aggregated by import_count, source_file_count)
          → CLI output / violations evaluation
```

Key layers:
- **Storage**: `queryResolvedImportsWithFiles`, `queryModuleFileOwnership`, `queryModuleCandidates`
- **Adapter**: `getModuleDependencyGraph` (query-time derivation, no persistence)
- **Core**: `deriveModuleDependencyEdges` (pure function, no side effects)
- **CLI**: JSON envelope formatting with diagnostics

## Core Questions — Answers

### 1. Are module dependency edges derived from file ownership correctly?

**YES.** The derivation is pure and well-tested:
- `deriveModuleDependencyEdges` in `src/core/modules/module-dependency-edges.ts`
- Takes IMPORTS edges, node-file mapping, file-module ownership
- Emits cross-module edges only (same-module excluded)
- Aggregates by (source_module, target_module) with importCount and sourceFileCount
- 15 unit tests cover basic, aggregation, missing file/module, deterministic ordering

### 2. Do inferred modules participate exactly like declared modules where they should?

**YES.** No special-casing by moduleKind in:
- File ownership assignment (longest-prefix, kind-agnostic)
- Dependency edge derivation (uses moduleCandidateUid only)
- Boundary evaluation (uses canonicalRootPath only)

The `moduleKind` field is preserved in output for display but does not affect
policy or derivation logic.

### 3. Are declared/operational/inferred kinds preserved in output?

**YES.** CLI output includes:
- `source_module_kind` / `target_module_kind` on each edge
- `moduleKind` in modules list

Evidence verified in `rgr modules deps module-deps-ts` output.

### 4. Do missing ownership cases produce diagnostics instead of silent omission?

**YES.** Diagnostics are comprehensive:
```json
{
  "diagnostics": {
    "imports_edges_total": 609,
    "imports_source_no_file": 0,
    "imports_target_no_file": 0,
    "imports_source_no_module": 0,
    "imports_target_no_module": 0,
    "imports_intra_module": 609,
    "imports_cross_module": 0
  }
}
```

All degradation categories are tracked:
- `imports_source_no_file` — source node has no file_uid
- `imports_target_no_file` — target node has no file_uid
- `imports_source_no_module` — source file has no module owner
- `imports_target_no_module` — target file has no module owner

### 5. Are intra-module imports excluded and cross-module imports counted correctly?

**YES.** Explicitly separated:
- `imports_intra_module` — same module, excluded from edges
- `imports_cross_module` — different modules, contributes to edges

Unit test: `excludes intra-module imports` in `module-dependency-edges.test.ts`

### 6. Are violations based on explicit boundary declarations only, not inferred assumptions?

**YES.** Violations require explicit policy:
1. User runs `rgr modules boundary <source> --forbids <target>` (TS)
   or `rmap declare boundary ...` (Rust)
2. Declaration stored with `selectorDomain: "discovered_module"`
3. Evaluator loads active declarations, parses, evaluates against edges

No implicit violations. A cross-module edge is not a violation unless a
boundary declaration forbids it.

### 7. Does the CLI distinguish "no violations" from "cannot evaluate because graph is degraded"?

**YES.**

- Rust `rmap modules violations`: exit 0 on success, exit 2 on load failure
- Stale declarations (module no longer exists) are reported separately
- Diagnostics object included in output envelope:
  - `imports_edges_total`, `imports_cross_module`, `imports_intra_module`
  - `imports_source_no_module`, `imports_target_no_module` — detect missing ownership
  - `imports_source_no_file`, `imports_target_no_file` — always 0 in Rust (storage pre-filters)

Callers can detect degraded graphs by checking `imports_source_no_module > 0`
or `imports_target_no_module > 0`.

### 8. Are results deterministic across runs?

**YES.**

- Violations: sorted by (source_path, target_path, declaration_uid) — **deterministic**
- Stale declarations: sorted by declaration_uid — **deterministic**
- Edges: sorted by (importCount DESC, sourceModuleUid ASC, targetModuleUid ASC) — **deterministic**

Both TS and Rust implementations use full sort keys with tie-breakers.

### 9. Do commands work on repos with only B1 directory modules?

**YES.** Tested on `directory-module-b1` fixture (B1 only, no Kbuild):
- `rgr modules list` shows `directory_structure` as sole evidence source
- `rgr modules deps` works (all intra-module in this fixture)

No special handling needed — B1 modules are just `moduleKind: "inferred"`.

### 10. Do commands work on repos with A1+B1 overlapping evidence?

**YES.** Overlap produces a single module candidate with both sources.
The candidate's confidence is max(A1, B1) = 0.9. Edges derive correctly.

## Validation Matrix

| Repo | Deps Works | Violations Works | Notes |
|------|------------|------------------|-------|
| repo-graph | YES | YES | All TS imports intra-module (root "."), no boundary decl |
| module-deps-ts | YES | YES | Cross-module edge exists, no boundary decl to violate |
| directory-module-b1 | YES | YES | B1 only, no boundary decl |
| kbuild-overlap | YES | YES | A1+B1 overlap, no boundary decl |

**Violations validated E2E:** `modules_violations_command.rs` creates a temp
workspace fixture with cross-package imports, seeds module candidates and file
ownership via SQL, declares boundary via CLI, detects violation. 9 tests pass.
Uses temp repo (not static fixture) for test isolation.

C repos (sqlite, swupdate, nginx) not validated — require re-index with
current schema (migration 017). Out of scope for this audit.

## Defects Found

**None critical.**

## Product Gaps Found

### Gap 1: Violations command lacks derivation diagnostics (P3) — **RESOLVED**

**Resolved:** Added `diagnostics` object to `rmap modules violations` output:
```json
{
  "diagnostics": {
    "imports_edges_total": 123,
    "imports_source_no_file": 0,
    "imports_target_no_file": 0,
    "imports_source_no_module": 4,
    "imports_target_no_module": 9,
    "imports_intra_module": 88,
    "imports_cross_module": 22
  }
}
```

Note: `imports_source_no_file` and `imports_target_no_file` are always 0 in
the Rust CLI because the storage layer pre-filters edges where nodes lack
file_uid. The TS implementation tracks these separately.

Tests added: `modules_violations_diagnostics_healthy` and
`modules_violations_diagnostics_degraded` verify diagnostics in normal
and degraded (missing ownership) cases. 11 tests pass.

### Gap 2: No TS CLI `modules violations` command (P4) — **DOCUMENTED**

The Rust CLI `rmap modules violations` is canonical for discovered-module
boundary violations. The TS CLI does not expose a dedicated command.

**Canonical surfaces:**
- `rmap modules violations` — discovered-module boundary violations (Rust)
- `rgr modules deps` — cross-module dependency inspection (TS)
- `rgr modules boundary` — declare discovered-module boundaries (TS)
- `rgr arch violations` — NOT equivalent; uses directory_module selector domain

**Accepted:** Adding `rgr modules violations` is deferred. The Rust CLI is
authoritative for governance/violation reporting. TS CLI remains the
dependency-inspection and declaration surface.

### Gap 3: No fixture with boundary declaration producing violation (P2) — **RESOLVED**

**Resolved:** Added `rust/crates/rgr/tests/modules_violations_command.rs` with
9 tests covering the full E2E path:
- `build_workspace_db()` creates temp fixture with cross-package imports
- Tests insert module candidates and file ownership
- Boundary declaration via `rmap modules boundary` CLI
- `rmap modules violations` detects and reports violation
- Exit code 1 when violations exist, 0 when none

All 9 tests pass. The fixture uses a relative import between packages
to ensure the IMPORTS edge is created by the indexer.

### Gap 4: TS edge sorting lacks path tie-breaker (P2) — **RESOLVED**

**Resolved:** Added tie-breakers to `deriveModuleDependencyEdges` sort:
```typescript
edges.sort((a, b) => {
  if (b.importCount !== a.importCount) {
    return b.importCount - a.importCount;
  }
  const srcCmp = a.sourceModuleUid.localeCompare(b.sourceModuleUid);
  if (srcCmp !== 0) return srcCmp;
  return a.targetModuleUid.localeCompare(b.targetModuleUid);
});
```

Two tests added: `sorts deterministically with tie-breakers` and
`deterministic ordering with complex graph`. 15 tests pass.

## Decision

**PRODUCTION-READY.** All P2/P3 gaps resolved or documented:
- Module dependency edge derivation
- Cross-module import counting
- Module kind preservation
- Diagnostics for degraded derivation
- Boundary violation detection (end-to-end tested)
- Deterministic edge ordering (both TS and Rust)
- Derivation diagnostics in violations output
- Rust CLI documented as canonical for violation reporting

**Accepted/deferred (not blockers):**
- No TS CLI `modules violations` — Rust is canonical, TS handles deps/declarations

## Repair Backlog

| Priority | Item | Status |
|----------|------|--------|
| P2 | Add module boundary violation E2E test | **DONE** — 11 Rust CLI tests (temp repo + SQL seeding) |
| P2 | Add path tie-breaker to TS edge sorting | **DONE** — 2 new tests |
| P3 | Add derivation diagnostics to `rmap modules violations` | **DONE** — diagnostics object in output |
| P4 | Document Rust CLI as canonical violations surface | **DONE** — documented in audit |

## Test Summary

- **TS module dependency edges:** 15 tests pass (+2 deterministic ordering)
- **TS module boundary evaluator:** 15 tests pass
- **TS module dependency query adapter:** 17 tests pass
- **TS CLI modules deps:** 18 tests pass
- **Rust classification module tests:** 32 tests pass
- **Rust CLI modules violations tests:** 11 tests pass (+2 diagnostics tests)

Total focused tests: **108 pass**.
