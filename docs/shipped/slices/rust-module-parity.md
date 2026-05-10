# Rust Module Parity

Status: DONE (2026-05-10)
Depends: `refresh-integrity-parity.md` (refresh must preserve module data first)
Track: Architectural Unification

## Problem Statement (Historical)

The TS indexer path and Rust indexer path produced structurally different module
representations:

- **TS path:** populated `module_candidates`, `module_candidate_evidence`,
  `module_file_ownership` with rich manifest-backed facts and module kind
  classification (declared/operational/inferred)
- **Rust path:** created `MODULE` nodes in `nodes` table only, with no kind
  classification and no evidence provenance

The agent layer compensated by falling back to MODULE nodes when `module_candidates`
was empty. Read surfaces normalized via adapter-side compensation logic.

This was backwards. Module truth belongs in the indexing/domain path, not in
adapter-layer workarounds.

## Resolution

Module representation is now unified. Both indexer paths populate the same data model:
- `module_candidates` — module catalog with kind classification
- `module_candidate_evidence` — provenance backing
- `module_file_ownership` — file-to-module assignments

The MODULE-node fallback has been deprecated (Phase 4, 2026-05-10). Empty
`module_candidates` now surfaces repos that need module detection configured
rather than silently falling back to degraded data.

## Architectural Decision

**Decision:** Option 1 — Rust populates canonical tables.

The temporary extraction duplication was preferable to permanent data model
bifurcation. All read surfaces now work identically regardless of indexer path.

## Current State (Post-Parity)

| Aspect | All Paths |
|--------|-----------|
| Module candidates | `module_candidates` table |
| Module evidence | `module_candidate_evidence` table |
| File ownership | `module_file_ownership` table |
| Fallback data | Deprecated (returns empty, not MODULE nodes) |
| Module kind | declared/inferred |
| Agent consumption | Primary path only |

## Implementation Phases (All Complete)

### Phase 1: Cargo.toml Module Extraction — DONE

Rust crate workspaces detected via `Cargo.toml` / `[workspace]` parsing.
Creates declared modules with manifest-backed evidence.

**Implementation:** `rust/crates/indexer/src/cargo_manifest.rs`

### Phase 2: package.json / pyproject.toml / settings.gradle — DONE

NPM/pnpm workspaces, Python single-package projects, and Gradle multi-project
builds all produce declared modules.

**Implementation:**
- `rust/crates/indexer/src/package_json.rs`
- `rust/crates/indexer/src/pyproject.rs`
- `rust/crates/indexer/src/settings_gradle.rs`

### Phase 3: Inferred Module Heuristic — DONE

Top-level directory heuristic for manifest-less repos. Creates inferred modules
(confidence 0.7) with `directory_heuristic` evidence basis.

Features:
- Test-only directory suppression
- Exclusion categories (vendor, build, docs, examples, benchmarks, generated)
- Sanity metrics (largest_module_ownership_pct, tiny_module_count, etc.)
- Unowned file classification (excluded/suppressed_test/true_gap)

**Implementation:** `rust/crates/indexer/src/inferred_modules.rs`

### Phase 4: Fallback Deprecation — DONE (2026-05-10)

MODULE-node fallback removed from:
- `rust/crates/module-queries/src/context.rs` — `ModuleQueryContext::load()`
- `rust/crates/storage/src/agent_impl.rs` — `get_module_summary()`

Empty `module_candidates` now returns empty/None rather than falling back to
MODULE nodes. This surfaces repos that need module detection configured.

## Acceptance Criteria (All Met)

- [x] `module_candidates` non-empty on indexed snapshots (declared or inferred)
- [x] `module_kind` populated honestly (declared from manifests, inferred from heuristic)
- [x] `rmap modules list` returns real module data from `module_candidates`
- [x] `rmap modules files <module>` returns owned files
- [x] `rmap modules show <module>` returns evidence
- [x] No fallback to MODULE nodes after Phase 4

## Validation

Phase 4 gate conditions met on all validation repos:

| Repo | classified_pct | true_gap_pct | Status |
|------|----------------|--------------|--------|
| sqlite | 100% | 0.0% | PASS |
| leveldb | 100% | 2.26% | PASS |
| nginx | 100% | 0.0% | PASS |
| linux | 100% | 0.0% | PASS |

## Test Coverage

- `rust/crates/module-queries/src/context.rs` — unit tests for context helpers
- `rust/crates/storage/tests/agent_impl.rs` — storage-level module summary tests
- `rust/crates/rgr/tests/modules_list_command.rs` — CLI integration tests
- `rust/crates/repo-index/src/compose.rs` — refresh parity tests (4 tests)
- `rust/crates/indexer/src/inferred_modules.rs` — heuristic unit tests (21 tests)

## References

- `rust/crates/storage/src/agent_impl.rs` — fallback removed from `get_module_summary()`
- `rust/crates/module-queries/src/context.rs` — fallback removed from `load()`
- `rust/crates/storage/src/crud/module_candidates.rs` — canonical storage implementation
- `CURRENT_SLICE.md` — Phase 4 implementation details
