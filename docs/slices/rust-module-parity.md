# Rust Module Parity

Status: PLANNED
Depends: `refresh-integrity-parity.md` (refresh must preserve module data first)
Track: Architectural Unification

## Problem Statement

The TS indexer path and Rust indexer path produce structurally different module
representations:

- **TS path:** populates `module_candidates`, `module_candidate_evidence`,
  `module_file_ownership` with rich manifest-backed facts and module kind
  classification (declared/operational/inferred)
- **Rust path:** creates `MODULE` nodes in `nodes` table only, with no kind
  classification and no evidence provenance

The agent layer compensates by falling back to MODULE nodes when `module_candidates`
is empty. Read surfaces normalize via adapter-side compensation logic.

This is backwards. Module truth belongs in the indexing/domain path, not in
adapter-layer workarounds.

## Objective

Unify module representation so both indexer paths produce the same data model.
The Rust indexer should populate the same tables as the TS indexer for modules:
- `module_candidates`
- `module_candidate_evidence`
- `module_file_ownership`

After this slice, the fallback path (MODULE nodes) becomes a legacy-only path,
not the primary Rust path.

## Non-Goals

- Refresh copy-forward (see `refresh-integrity-parity.md`)
- Config-aware invalidation (see `refresh-integrity-parity.md`)
- Measurement parity across refresh (see `refresh-integrity-parity.md`)
- New module detection heuristics beyond existing signals
- Cross-repo module linking

## Architectural Decision

Two implementation options exist:

### Option 1 (Recommended): Rust populates canonical tables

Rust indexer populates the same tables as TS:
- `module_candidates` with `module_kind` = declared/operational/inferred
- `module_candidate_evidence` with manifest source paths
- `module_file_ownership` with file-to-module assignments

**Pro:** Single source of truth. All read surfaces work identically.
**Con:** Duplicated extraction logic between TS and Rust (temporary until TS path deprecated).

### Option 2: Formalize MODULE-node path as permanent

Declare MODULE nodes as the Rust-path canonical representation. Upgrade all
read surfaces to treat MODULE nodes with equivalent fidelity.

**Pro:** No extraction duplication.
**Con:** Permanently bifurcated data model. Every new module read surface must
handle both paths. Complexity pushed downstream to every consumer.

**Decision:** Option 1. The temporary extraction duplication is preferable to
permanent data model bifurcation.

## Current State

| Aspect | TS Path | Rust Path |
|--------|---------|-----------|
| Module candidates | `module_candidates` table | None |
| Module evidence | `module_candidate_evidence` table | None |
| File ownership | `module_file_ownership` table | None |
| Fallback data | N/A | `MODULE` nodes in `nodes` table |
| Module kind | declared/operational/inferred | Inferred only (implicit) |
| Agent consumption | Primary path | Fallback via `get_module_summary()` |

## Target State

| Aspect | TS Path | Rust Path |
|--------|---------|-----------|
| Module candidates | `module_candidates` table | `module_candidates` table |
| Module evidence | `module_candidate_evidence` table | `module_candidate_evidence` table |
| File ownership | `module_file_ownership` table | `module_file_ownership` table |
| Fallback data | N/A | N/A (deprecated) |
| Module kind | declared/operational/inferred | declared/operational/inferred |
| Agent consumption | Primary path | Primary path |

## Implementation Phases

### Phase 1: Cargo.toml Module Extraction

1. Parse `Cargo.toml` for crate metadata (name, version, path)
2. Parse `[workspace]` for workspace member patterns
3. Create `module_candidates` with `module_kind` = 'declared'
4. Create `module_candidate_evidence` with `source_path` = Cargo.toml path
5. Compute initial file ownership from crate root paths

### Phase 2: package.json Module Extraction

1. Parse `package.json` for package metadata (name, version, main/exports)
2. Parse workspace patterns (`workspaces` array, `pnpm-workspace.yaml`)
3. Create `module_candidates` with `module_kind` = 'declared'
4. Create `module_candidate_evidence` with manifest source paths
5. Compute file ownership from package root paths

### Phase 3: Inferred Module Fallback

1. For directories without manifest-backed modules, infer from structure
2. Create `module_candidates` with `module_kind` = 'inferred'
3. Create `module_candidate_evidence` with basis = 'directory_heuristic'
4. Compute file ownership from directory containment

### Phase 4: Fallback Deprecation

1. Mark MODULE-node fallback path as deprecated in agent layer
2. Add warning when fallback is triggered on post-parity snapshots
3. Update `get_module_summary()` to prefer `module_candidates` unconditionally
4. Document migration path for any external consumers of MODULE nodes

## Acceptance Criteria

- `module_candidates` non-empty on Rust-indexed snapshots
- `module_kind` populated honestly (declared from Cargo.toml, inferred from directory)
- `rmap modules list` returns real module data, not fallback
- `rmap modules files <module>` returns owned files
- `rmap modules show <module>` returns evidence
- Module counts match between TS-indexed and Rust-indexed paths on same repo
- `MODULE_DATA_UNAVAILABLE` never emitted when `module_candidates` is populated

## Test Matrix

| Scenario | Expected |
|----------|----------|
| Rust index on Cargo workspace | `module_candidates` populated with declared modules |
| Rust index on npm workspace | `module_candidates` populated with declared modules |
| Rust index on monorepo (no manifest) | `module_candidates` populated with inferred modules |
| Same repo, TS vs Rust index | `module_candidates` counts match |
| `rmap modules list` post-parity | Returns data from `module_candidates`, not MODULE nodes |

## Validation Repos

- `repo-graph` — Rust workspace with manifest-backed crates
- `amodx` — TS monorepo with pnpm workspaces
- `glamCRM` — TS/Java mixed with package.json workspaces
- `swupdate` — C codebase (inferred modules only)

## References

- `rust/crates/storage/src/agent_impl.rs` — current fallback in `get_module_summary()`
- `rust/crates/storage/src/crud/module_candidates.rs` — existing TS-path implementation
- `docs/slices/refresh-integrity-parity.md` — prerequisite slice
- `docs/TECH-DEBT.md` — module parity gap documented
