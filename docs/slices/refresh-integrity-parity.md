# Refresh Integrity Parity

Status: SUPERSEDED
Superseded-by: Artifact Contract Registry (ACR) program
Depends: delta refresh pipeline, snapshot-scoped artifact tables, invalidation planner
Follow-on: ACR slices (`acr-1` through `acr-6`), then `rust-module-parity.md`
Track: Core Infrastructure

## Supersession Notice (2025-05-08)

This slice's tactical copy-forward approach has been superseded by the
**Artifact Contract Registry (ACR)** program. The remaining blockers
(boundary_interaction_links, project_surfaces) require architectural changes
that are better addressed by explicit artifact contracts rather than continued
ad-hoc table-specific fixes.

**New architecture:**
- `docs/architecture/artifact-contract-model.md` — full specification
- `docs/architecture/adr/adr-artifact-contract-registry.md` — decision record
- `docs/slices/acr-1-artifact-contracts-crate.md` through `acr-6-*.md` — execution slices

**What was completed here remains valid:**
- Copy-forward infrastructure for measurements, inferences, boundary surfaces/channels
- Config-file invalidation widening
- Bug fixes (UID collisions)
- Integration tests

**What will be solved under ACR:**
- `boundary_interaction_links` regeneration (ACR-5)
- Query degradation for unsupported families (ACR-6)
- Per-row freshness and provenance tracking (ACR-3, ACR-4)

---

## Implementation Progress

### Completed (2026-05-08)

| Item | Status | Notes |
|------|--------|-------|
| Refresh context wiring | Done | `orchestrator.rs` populates `parent_snapshot_uid`, `unchanged_files` on `IndexResult` |
| `copy_forward_measurements` | Done | `refresh_copy_forward_impl.rs` |
| `copy_forward_inferences` | Done | `refresh_copy_forward_impl.rs` |
| `copy_forward_boundary_surfaces` | Done | `refresh_copy_forward_impl.rs` — generates new UUIDs, preserves FK mapping |
| `copy_forward_contract_schemas` | Done | `refresh_copy_forward_impl.rs` |
| Changed-file filtering for postpass | Done | `compose.rs` filters `changed_files_owned` for boundary/policy extraction |
| Boundary changed-file reinsertion bug | Fixed | `insert_boundary_surfaces_and_channels` generates fresh UUIDs to avoid PK collision |
| Copy-forward diagnostics surfaced | Done | `ArtifactCopyForward` struct in `IndexResult`, CLI shows counts |
| Config-file invalidation widening | Done | `routing::is_config_file()` identifies configs; scanner includes them; `persist_config_file_versions()` tracks in file_versions; `ConfigFileState` passed to `refresh_repo()` for invalidation planning; config files filtered from extraction (no FILE nodes, not counted in `IndexResult.files_total`). Nested configs widen only scoped files. Note: config files DO count toward snapshot table `files_total` (computed from `file_versions`). |

### Not Implemented

| Item | Status | Blocker |
|------|--------|---------|
| `project_surfaces` family copy-forward | Blocked | Rust indexer does not populate project_surfaces — TS-only feature |
| `boundary_interaction_links` regeneration | Blocked | Requires `copy_forward_boundary_contracts()` with dual UID mapping; see §Analysis |
| Boundary + contract parity integration tests | Done | 6 tests in `rust/crates/repo-index/tests/refresh.rs` |
| Full agent parity validation | Open | CLI output normalization for `boundaries`, `contracts`, `surfaces`, `orient`, `check` not tested |

### Bug Fixed: Contract Schema UID Collision (2026-05-08)

**Problem:** Contract schemas used deterministic UIDs based on `{repo_uid}:{file_path}:{content_hash[..8]}`.
On refresh, when a proto file was re-indexed, `INSERT OR IGNORE` silently dropped the insert because
a row with the same `schema_uid` already existed from the parent snapshot (different `snapshot_uid`).

**Symptom:** After refresh, `rmap contracts list` returned 0 schemas even though the orchestrator
reported `schemas_indexed: 2`.

**Fix:** Changed `proto_indexer.rs` to use fresh UUIDs per snapshot (`Uuid::new_v4().to_string()`).
Schema rows are snapshot-scoped; the primary key must be unique per snapshot. Deterministic identity
belongs in stable attributes (`file_path`, `content_hash`), not the row primary key.

**Note:** This fix enables proto re-indexing during refresh. Contract files are currently always
re-indexed (not copied forward). The `copy_forward_contract_schemas()` support code exists but is
not invoked because proto files are handled separately from source files in the refresh path.

### Analysis: `boundary_interaction_links` Regeneration

The gRPC link detection (GR-3A) joins:
- `boundary_interaction_surfaces` (copied with new UIDs via `copy_forward_boundary_surfaces`)
- `boundary_contracts` (NOT copied — links surface_uid to contract_element_uid)
- `contract_elements` (RE-INDEXED every refresh with fresh UIDs)

**Problem:** `boundary_contracts` uses two per-snapshot UIDs as foreign keys:
- `surface_uid` → points to parent snapshot's surface UID (but surfaces get new UIDs during copy-forward)
- `contract_element_uid` → points to parent snapshot's element UID (but elements get new UIDs during re-indexing)

Neither UID is stable across refresh. The dual UID mapping approach (Option 1 below) is not viable
because contract elements are re-indexed (not copied forward), so we have no old→new element UID mapping.

**Fix options:**
1. ~~**Copy-forward boundary_contracts with dual mapping**~~ — NOT VIABLE. Requires element UID
   mapping, but contracts are re-indexed (not copied forward), so no mapping exists.
2. **Re-run GR chain after copy-forward** (RECOMMENDED) — Move gRPC detection chain
   (GR-1A/GR-2A/GR-3A) from `orchestrator::refresh_repo` to `compose.rs`, running AFTER
   copy-forward completes. This regenerates `boundary_contracts` from scratch using the
   new surface and element UIDs.

**Scope:** Requires changes to:
- Move GR-1A/GR-2A/GR-3A invocation from `orchestrator.rs` to `compose.rs`
- Run detection AFTER `copy_forward_derived_artifacts()` returns
- Detection must use the new snapshot's surface and element UIDs

### Analysis: `project_surfaces` Family

**FK dependency chain:**
```
project_surfaces.module_candidate_uid (NOT NULL)
    → module_candidates.module_candidate_uid
        → module_candidate_evidence.source_path (determinant files)
```

**Blocker:** `project_surfaces` has a NOT NULL FK to `module_candidates`. The Rust
indexer does not populate `module_candidates`. Without module_candidates rows in
the current snapshot, project_surfaces cannot exist (FK constraint).

**Options:**
1. Implement `copy_forward_module_candidates()` — only helps if parent snapshot
   has module_candidates (i.e., was indexed by TS prototype)
2. Implement Rust-side module_candidates population — full module discovery parity
3. Both

For repos indexed only by Rust CLI, option 1 doesn't help because there's nothing
to copy forward. The practical fix is option 2 (rust-module-parity.md).

### Bug Fixed During Implementation

**Boundary insertion PK collision** — Fresh extraction used deterministic `surface_uid`
(`bi:repo:file:line:col:kind:direction`). On refresh, changed file's new boundary had
same UID as parent snapshot row. `INSERT OR IGNORE` silently dropped it.

Fix: `insert_boundary_surfaces_and_channels()` generates fresh UUIDs per snapshot,
maintains old→new mapping for channel FKs. Matches copy-forward behavior.

## Problem Statement

The product promise is **current-state discovery**. The `rmap refresh` command
should preserve "what exists now" across incremental re-indexing. Currently it
does not.

Observed regressions after `rmap refresh`:

1. **Contract schemas/elements** — dropped to zero
2. **Boundary interaction surfaces** — dropped to zero
3. **Project surfaces** — not preserved
4. **Measurements** — not copied forward for unchanged files
5. **Inferences** — file-local facts not copied forward
6. **Config changes** — not detected (invalidation logic exists but never fires)

This is a product-center problem, not a polish problem. If `rmap` cannot
preserve derived truth across `refresh`, then adding more mechanism families
increases breadth while weakening trust.

## Objective

Make `rmap refresh` preserve all derived artifacts for unchanged files.
A refresh with no relevant changes must produce semantically identical
query results to the parent snapshot (after normalizing snapshot-specific
fields like UIDs and timestamps).

## Non-Goals

- Module truth-model unification (separate slice: `rust-module-parity.md`)
- New mechanism-family detection
- TCP/UDP role completion
- New state-boundary language coverage
- New policy-fact extraction families
- Rust indexer populating `module_candidates` table

## Artifact Policy

Copy-forward vs regenerate decisions for each artifact family:

| Artifact Family | Policy | Rationale |
|-----------------|--------|-----------|
| `measurements` | copy-forward | file-local deterministic extraction |
| `inferences` | copy-forward | file-local derived fact with stable basis |
| `contract_schemas` | copy-forward | source-file anchored, proto file unchanged |
| `contract_elements` | copy-forward | schema-anchored, parent schema unchanged |
| `boundary_interaction_surfaces` | copy-forward | source-file anchored extraction |
| `boundary_channel_details` | copy-forward | surface-anchored, parent surface unchanged |
| `boundary_interaction_links` | regenerate | cross-surface derived relation, depends on full surface set |
| `project_surfaces` | copy-forward | source-file anchored detection |
| `project_surface_evidence` | copy-forward | surface-anchored evidence |
| `surface_entrypoints` | copy-forward | surface-anchored |
| `surface_config_roots` | copy-forward | surface-anchored |
| `surface_env_dependencies` | copy-forward | surface-anchored |
| `surface_env_evidence` | copy-forward | surface-anchored |
| `surface_fs_mutations` | copy-forward | surface-anchored |
| `surface_fs_mutation_evidence` | copy-forward | surface-anchored |

**Key distinction:**
- **Copy-forward:** artifact is deterministic function of unchanged source file(s)
- **Regenerate:** artifact depends on cross-file or cross-artifact relationships

## Config Invalidation Widening Rules

Config file changes must widen the invalidation scope appropriately:

| Config File | Affects | Widening Scope |
|-------------|---------|----------------|
| `Cargo.toml` | Rust dependency signals, crate module evidence | All `.rs` files in nearest crate; workspace members if `[workspace]` changed |
| `package.json` | TS/JS dependency signals, runtime surfaces | All `.ts/.js/.tsx/.jsx` files in nearest package |
| `tsconfig.json` / `jsconfig.json` | Alias resolution, enrichment ownership | All files under tsconfig subtree |
| `pnpm-workspace.yaml` | Workspace member discovery | All packages matching changed globs |
| `compile_commands.json` | C/C++ include/define context | All TUs listed in changed entries, or full C/C++ scope if global change |
| `pyproject.toml` | Python dependency signals, module evidence | All `.py` files in nearest Python project |
| `requirements.txt` | Python dependency signals | All `.py` files in nearest Python project |
| `build.gradle` / `build.gradle.kts` | Java dependency signals | All `.java` files in nearest Gradle project |
| `settings.gradle` / `settings.gradle.kts` | Multi-project structure | All subprojects if `include` statements changed |

**Current debt:** Config-widening logic exists in the invalidation planner but never
fires because config files are not in the file scanner hash set. See
`docs/TECH-DEBT.md` §Config-file tracking gap.

## Task Set A: Refresh Copy-Forward

### A1. Contract Refresh Parity

**Documented debt:** `docs/TECH-DEBT.md` §Contract schemas not copied forward

Tables:
- `contract_schemas`
- `contract_elements`

Implementation:
1. Identify proto files unchanged between parent and current snapshot
2. Copy `contract_schemas` rows where `file_path` is in unchanged set
3. Copy `contract_elements` rows where parent `schema_uid` was copied

Acceptance:
- `rmap contracts list` returns same count after refresh with no proto changes
- `rmap contracts elements` returns same results after refresh
- `rmap contracts usages` returns same results after refresh

### A2. Boundary Refresh Parity

Tables:
- `boundary_interaction_surfaces`
- `boundary_channel_details`
- `boundary_interaction_links` (regenerate after copy-forward)

Implementation:
1. Identify source files unchanged between parent and current snapshot
2. Copy `boundary_interaction_surfaces` rows where `source_file` is in unchanged set
3. Copy `boundary_channel_details` rows where parent `surface_uid` was copied
4. Regenerate `boundary_interaction_links` from full surface set

Acceptance:
- `rmap boundaries list` returns same surface count after refresh
- `rmap boundaries show` returns same details for unchanged files
- `rmap boundaries summary` returns consistent totals

### A3. Surface Refresh Parity

Tables:
- `project_surfaces`
- `project_surface_evidence`
- `surface_entrypoints`
- `surface_config_roots`
- `surface_env_dependencies`
- `surface_env_evidence`
- `surface_fs_mutations`
- `surface_fs_mutation_evidence`

Implementation:
1. Identify determinant inputs for each surface:
   - `project_surface_evidence.source_path` — evidence source files
   - `surface_config_roots.config_path` — config anchor files
   - Parent `module_candidate` evidence files
2. Copy `project_surfaces` only when all determinant source/config inputs
   for the surface are unchanged; otherwise regenerate for the affected root
3. Copy child evidence tables where parent surface was copied

Acceptance:
- `rmap surfaces list` returns same count after refresh
- `rmap surfaces show` returns same details for unchanged surfaces

### A4. Config-Aware Invalidation

Implementation options:
1. Include config files in the file scanner hash set
2. Separate config-change detection pass against parent snapshot `file_versions`

Acceptance:
- Changing `Cargo.toml` invalidates dependent `.rs` files
- Changing `package.json` invalidates dependent `.ts/.js` files
- Changing `tsconfig.json` invalidates files under its scope
- Config file hash changes appear in invalidation plan diagnostics
- Widening scope matches the table above

## Task Set C: Quality Measurement Parity

### Current State

- Complexity measurements exist in `measurements` table after full index
- `orient` correctly emits `HIGH_COMPLEXITY` when measurements exist
- **Gap:** After refresh, measurements for unchanged files are not preserved
- **Gap:** `orient` may emit `COMPLEXITY_UNAVAILABLE` where parent had data

### Implementation

1. Copy `measurements` rows where `file_path` (via node lookup) is in unchanged set
2. Verify `orient` reads measurements from current snapshot correctly
3. Verify `check` quality conditions evaluate against current snapshot

### Acceptance

- `rmap orient` on refresh snapshot emits same `HIGH_COMPLEXITY` as parent
- `COMPLEXITY_UNAVAILABLE` not emitted when parent had measurements
- Measurement counts in refresh diagnostics show copy-forward totals

## Implementation Strategy

### Phase 1: Copy-Forward Infrastructure

1. Add `copy_forward_unchanged_artifacts(parent_uid, current_uid, unchanged_files)`
   to refresh pipeline in `rust/crates/repo-index/src/compose.rs`
2. Implement file-scoped copy logic per artifact family
3. Add diagnostics for copy-forward counts in extraction metadata
4. Wire into existing `refreshRepo` / delta path

### Phase 2: Per-Artifact Copy-Forward

Order by dependency (parent artifacts before child):
1. `measurements` (no dependencies)
2. `inferences` (no dependencies)
3. `contract_schemas` (no dependencies)
4. `contract_elements` (depends on schemas)
5. `boundary_interaction_surfaces` (no dependencies)
6. `boundary_channel_details` (depends on surfaces)
7. `project_surfaces` (no dependencies)
8. Surface evidence tables (depend on surfaces)

### Phase 3: Link Regeneration

After copy-forward:
1. Regenerate `boundary_interaction_links` from full surface set
2. Any other cross-artifact derived relations

### Phase 4: Config Tracking

1. Identify config files from manifest patterns per language
2. Include in hash comparison or implement separate detection pass
3. Wire widening logic to fire on config changes
4. Add widening diagnostics to refresh output

## Test Matrix

| Scenario | Expected Outcome |
|----------|------------------|
| Refresh, no changes | All artifact counts identical (normalized) |
| Refresh, unrelated `.ts` edit | Contract/boundary/surface artifacts for other files preserved |
| Refresh, proto file edit | Contract artifacts regenerated for that file only |
| Refresh, C file edit | Boundary artifacts regenerated for that file only |
| Refresh, `Cargo.toml` edit | Dependent `.rs` files invalidated and re-extracted |
| Refresh, `package.json` edit | Dependent `.ts/.js` files invalidated |
| Refresh, `tsconfig.json` edit | Files under tsconfig scope invalidated |
| Full index vs refresh on unchanged repo | Semantically identical query results |

**Normalization for comparison:**
- Ignore `snapshot_uid` differences
- Ignore timestamps (`created_at`, `indexed_at`)
- Ignore refresh-specific diagnostics counters
- Compare artifact counts and content, not envelope metadata

## Validation Repos

- `repo-graph` — mixed TS/Rust, has contracts, boundaries, surfaces, measurements
- `swupdate` — C, has boundary interactions
- `glamCRM` — TS/Java, has HTTP boundaries
- `amodx` — TS monorepo, has surfaces

## Success Criteria

1. On unchanged repo, normalized outputs of `rmap contracts`, `rmap boundaries`,
   `rmap surfaces`, `rmap orient`, `rmap check` are semantically identical
   before and after refresh.

2. On unrelated source edit, unaffected artifact families remain queryable
   with unchanged counts and content.

3. On config-file edit, invalidation widens according to the declared scope
   in the Config Invalidation Widening Rules table.

4. Copy-forward diagnostics report per-artifact-family counts in refresh
   metadata (e.g., "copied 47 boundary surfaces, 12 contract schemas").

5. At least one real-repo validation (repo-graph or glamCRM) proves refresh
   preserves contracts, boundaries, surfaces, and agent signals.

## References

- `docs/TECH-DEBT.md` §Config-file tracking gap
- `docs/TECH-DEBT.md` §Contract schemas not copied forward
- `docs/TECH-DEBT.md` §Delta indexing (slice 1)
- `rust/crates/repo-index/src/compose.rs` — refresh pipeline entry
- `rust/crates/repo-index/src/lib.rs` — delta/refresh orchestration
