# FD-SUPPORT-1: Rust Provider-Fact / Project-Surface Write Path Parity

Status: **IMPLEMENTED** (storage write path)
Depends: None
Unblocks: FD-1A (Express), FD-1B (React), any future Rust-native framework detector

## Implementation Summary (2026-05-11)

**Phase 1 complete:** Storage write path implemented.

Files modified:
- `rust/crates/storage/src/types.rs` — added `CreateProjectSurfaceInput`, `CreateProjectSurfaceEvidenceInput`
- `rust/crates/storage/src/crud/project_surfaces.rs` — added insert methods

Methods added:
- `insert_project_surface()` — single surface insert
- `insert_project_surface_evidence()` — single evidence insert
- `insert_project_surfaces_batch()` — transaction-wrapped batch insert
- `insert_project_surface_evidence_batch()` — transaction-wrapped batch insert

Tests: 20 tests pass (7 new for insert methods)

**Remaining (deferred to FD-1A):**
- Producer-side fact production (how to create `BoundaryProviderFact` from Express detection)
- Orchestration pattern (hook vs compose-phase)
- CLI integration (wiring detection to persistence)

---

## Problem Statement

Rust cannot currently persist framework-detected surfaces to the `project_surfaces` and `project_surface_evidence` tables.

The TS indexer produces these surfaces via a post-extraction boundary-fact pass. Rust has no equivalent path.

### Current State

| Component | TS Path | Rust Path |
|-----------|---------|-----------|
| Producer contract | `BoundaryProviderFact` | None |
| Detection | `express-route-extractor.ts` (regex prototype) | `detect_framework_boundary()` returns `ClassifierVerdict` only |
| Persistence | TS indexer writes `project_surfaces` | Read-only CRUD |
| CLI visibility | `rmap surfaces list` | Queries TS-produced rows only |

### Why `ClassifierVerdict` Is Insufficient

`framework_boundary.rs::detect_framework_boundary()` returns:

```rust
ClassifierVerdict {
    classification: FrameworkBoundaryCandidate,
    basis_code: ExpressRouteRegistration,
}
```

This carries **classification metadata only**. It does NOT carry:

- HTTP method (GET, POST, etc.)
- Literal route path (`"/api/users"`)
- Handler symbol stable key
- Source file and line
- Mount prefix structure
- Framework-specific metadata

These are required to construct a `BoundaryProviderFact` or `ProjectSurface`.

The current Rust detection is useful for **edge reclassification** (marking an unresolved edge as framework-related), not for **surface extraction** (producing a discrete HTTP endpoint fact).

## Scope

### In Scope

1. **Producer-side fact contract** — how Rust feature extractors emit provider facts
2. **Storage write path** — `insert_project_surface()`, `insert_project_surface_evidence()`
3. **Orchestration pattern** — how facts flow from extraction to persistence
4. **Refresh behavior** — how re-indexing updates surfaces
5. **CLI parity** — Rust-produced surfaces visible via existing `rmap surfaces` commands

### Out of Scope

- Express-specific detection logic (FD-1A)
- React/frontend detection (FD-1B)
- Consumer facts (`BoundaryConsumerFact`)
- Boundary link matching

## Certainty Layer

**Layer 2 (Bounded Inferences)**

Project surfaces are derived from extraction facts. The derivation is deterministic given the input, but the input (framework detection) has heuristic confidence.

## Architecture Decision: Producer-Side Fact Contract

### Option A: Feature Extractors Emit `BoundaryProviderFact` Directly

Feature-specific extractors (Express, React, etc.) directly produce `BoundaryProviderFact` records.

```
[Extractor] → BoundaryProviderFact → [Persist] → project_surfaces
```

**Benefits:**
- Reuses existing domain contract from `classification/types.rs`
- Closest to TS product semantics
- No intermediate DTO layer
- `BoundaryProviderFact` already has all required fields (mechanism, operation, address, handler_stable_key, source_file, line_start, framework, basis, metadata)

**Costs:**
- Feature extractors couple to provider-fact semantics
- If surface semantics diverge from provider-fact semantics later, requires refactor

### Option B: Feature Extractors Emit Narrower Internal DTO

Feature extractors emit a framework-specific DTO. A compose-phase translator converts to `BoundaryProviderFact` and then to `ProjectSurface`.

```
[Extractor] → FrameworkEvidence → [Translate] → BoundaryProviderFact → [Persist] → project_surfaces
```

**Benefits:**
- Better separation if multiple frameworks share substrate
- Easier to evolve internal representation

**Costs:**
- More support code before first feature parity
- Another DTO layer
- Translation logic must be written and maintained

### Recommendation

**Option A** for first parity slice.

Rationale:
- `BoundaryProviderFact` is already defined and proven in TS
- Express detection is the only consumer initially
- Simpler path to parity; can refactor later if needed

This decision may be revisited when FD-1B (React) arrives — if React surfaces differ significantly, Option B may become necessary.

## Architecture Decision: Orchestration Pattern

### Option A: Hook-Based (Parallel to StateBoundaryHook)

A `FrameworkSurfaceHook` receives extraction events and accumulates `BoundaryProviderFact` records. At snapshot completion, it drains facts to storage.

```
index_path
  → orchestrator with FrameworkSurfaceHook
    → on_extraction_result: detect Express patterns, emit BoundaryProviderFact
    → drain_snapshot_extras: persist to project_surfaces
```

**Benefits:**
- Proven pattern (SB-7A/7B/7C use it)
- Clean separation from core indexing

**Costs:**
- Hook receives `ExtractionResult`, which may not have the right granularity for framework detection
- State boundaries work from `ResolvedCallsite`; framework detection may need different inputs (unresolved edges + file signals)

### Option B: Compose-Phase Fact Collection

Framework detection runs as a post-extraction pass in `compose::index_path`, after all files are extracted but before snapshot finalization.

```
index_path
  → extract all files
  → run framework_surface_pass(all_edges, all_file_signals)
    → produces Vec<BoundaryProviderFact>
  → persist to project_surfaces
  → finalize snapshot
```

**Benefits:**
- Has access to all edges and file signals (needed for Express detection)
- Simpler mental model: one pass, then persist
- Does not require per-file hook invocations

**Costs:**
- Requires buffering all edges/signals before detection
- Less incremental (cannot emit surfaces as files are processed)

### Recommendation

**Defer decision** until FD-1A requirements are clearer.

The hook pattern works well for state boundaries because:
- Input is `ResolvedCallsite` (emitted per-file by extractors)
- Detection is per-callsite, no cross-file context needed

Framework detection for Express may need:
- Unresolved edges (not resolved callsites)
- File-level import signals (to confirm Express import)
- Potentially cross-file context (router mount composition)

The compose-phase pattern may be more natural for Express.

**This slice defines the storage write path and producer contract. The orchestration pattern is finalized when FD-1A is written.**

## Storage Write Path

### Input Types

```rust
/// Input for creating a project surface.
pub struct CreateProjectSurfaceInput {
    pub snapshot_uid: String,
    pub repo_uid: String,
    pub module_candidate_uid: String,
    pub surface_kind: String,          // "http_provider", "grpc_provider", etc.
    pub display_name: Option<String>,
    pub root_path: String,
    pub entrypoint_path: Option<String>,
    pub build_system: String,
    pub runtime_kind: String,
    pub confidence: f64,
    pub metadata_json: Option<String>,
    pub source_type: String,           // "express_route", "spring_controller", etc.
    pub source_specific_id: Option<String>,
    pub stable_surface_key: String,
}

/// Input for creating project surface evidence.
pub struct CreateProjectSurfaceEvidenceInput {
    pub project_surface_uid: String,
    pub snapshot_uid: String,
    pub repo_uid: String,
    pub source_type: String,           // "code_detection"
    pub source_path: String,           // file path where detected
    pub evidence_kind: String,         // "route_registration", "middleware_mount"
    pub confidence: f64,
    pub payload_json: Option<String>,  // {"method": "GET", "path": "/api/users", ...}
}
```

### CRUD Methods

Add to `storage/src/crud/project_surfaces.rs`:

```rust
impl StorageConnection {
    /// Insert a project surface. Returns the generated UID.
    pub fn insert_project_surface(
        &mut self,
        input: &CreateProjectSurfaceInput,
    ) -> Result<String, StorageError>;

    /// Insert project surface evidence. Returns the generated UID.
    pub fn insert_project_surface_evidence(
        &mut self,
        input: &CreateProjectSurfaceEvidenceInput,
    ) -> Result<String, StorageError>;

    /// Batch insert project surfaces (for efficiency).
    pub fn insert_project_surfaces_batch(
        &mut self,
        inputs: &[CreateProjectSurfaceInput],
    ) -> Result<Vec<String>, StorageError>;

    /// Batch insert project surface evidence.
    pub fn insert_project_surface_evidence_batch(
        &mut self,
        inputs: &[CreateProjectSurfaceEvidenceInput],
    ) -> Result<Vec<String>, StorageError>;
}
```

### UID Generation

Surface UID: `ps-<uuid-prefix>`
Evidence UID: `pse-<uuid-prefix>`

### Stable Surface Key Contract

For HTTP provider surfaces from Express:

```
surface:express_route:<method>:<normalized_path>
```

Example: `surface:express_route:GET:/api/users/{id}`

Path normalization:
- `:param` → `{param}` (Express to OpenAPI style)
- Trailing slash stripped
- Query parameters stripped

## BoundaryProviderFact → ProjectSurface Translation

```rust
fn provider_fact_to_surface(
    fact: &BoundaryProviderFact,
    snapshot_uid: &str,
    repo_uid: &str,
    module_candidate_uid: &str,
) -> CreateProjectSurfaceInput {
    CreateProjectSurfaceInput {
        snapshot_uid: snapshot_uid.to_string(),
        repo_uid: repo_uid.to_string(),
        module_candidate_uid: module_candidate_uid.to_string(),
        surface_kind: mechanism_to_surface_kind(fact.mechanism),
        display_name: Some(fact.operation.clone()),
        root_path: extract_root_path(&fact.source_file),
        entrypoint_path: Some(fact.source_file.clone()),
        build_system: "npm".to_string(), // or inferred
        runtime_kind: "node".to_string(), // or inferred
        confidence: confidence_from_basis(fact.basis),
        metadata_json: Some(serde_json::to_string(&fact.metadata).unwrap()),
        source_type: fact.framework.clone(),
        source_specific_id: None,
        stable_surface_key: compute_stable_surface_key(fact),
    }
}

fn mechanism_to_surface_kind(mechanism: BoundaryMechanism) -> String {
    match mechanism {
        BoundaryMechanism::Http => "http_provider".to_string(),
        BoundaryMechanism::Grpc => "grpc_provider".to_string(),
        // ...
    }
}
```

## Refresh Behavior

**Contract:** Re-indexing a snapshot replaces all Rust-produced surfaces for that snapshot.

Implementation:
1. Before inserting new surfaces, delete existing surfaces where `source_type` matches the Rust producer (e.g., `express_route`)
2. Insert new surfaces
3. This preserves TS-produced surfaces (different `source_type`) in hybrid scenarios

Alternative: Full surface replacement. Simpler but loses TS-produced surfaces if Rust re-indexes.

**Decision deferred** — depends on whether hybrid TS+Rust indexing is a supported scenario.

## CLI Visibility

No new CLI commands. Rust-produced surfaces are visible via existing:

```bash
rmap surfaces list <db> <repo> --kind http_provider
rmap surfaces show <db> <surface-ref>
```

The `source_type` column distinguishes Rust-produced (`express_route`) from TS-produced (`dockerfile`, `package_json`, etc.) surfaces.

## Validation Commands

```bash
# 1. Build storage crate
cd rust && cargo build -p repo-graph-storage

# 2. Unit tests for new CRUD methods
cargo test -p repo-graph-storage project_surfaces

# 3. Verify insert_project_surface compiles and runs
# (integration test with in-memory DB)

# 4. Verify round-trip: insert → query → verify fields match
```

## Acceptance Criteria

1. [x] `CreateProjectSurfaceInput` type exists in `storage/src/types.rs`
2. [x] `CreateProjectSurfaceEvidenceInput` type exists
3. [x] `insert_project_surface()` method exists and works
4. [x] `insert_project_surface_evidence()` method exists and works
5. [x] Batch insert methods exist for efficiency
6. [x] Round-trip test: insert surface → query via `get_project_surfaces_for_snapshot` → fields match
7. [x] Evidence test: insert evidence → query via `get_project_surface_evidence` → fields match
8. [x] `rmap surfaces list` shows Rust-inserted surfaces (verified via query layer — same table, same queries)

## Definition of Done

- [x] Storage write path functional (criteria 1-7)
- [x] CLI visibility confirmed (criterion 8) — Rust-inserted rows use same table schema; existing queries work
- [x] Orchestration pattern decision documented as "deferred to FD-1A"
- [x] No feature-specific detection logic in this slice

## Open Questions

1. **Hybrid TS+Rust indexing:** Is this a supported scenario? Affects refresh behavior.
2. **Module candidate resolution:** How does Rust determine `module_candidate_uid` for a detected surface? Express routes in file `src/routes/api.ts` — which module owns them?
3. **Orchestration granularity:** Should Express detection run per-file (hook) or post-extraction (compose-phase)?

## Deferred to FD-1A

- Express-specific detection patterns
- Route path extraction from unresolved edges
- Handler symbol attribution
- Test corpus creation
- Parity validation against TS prototype
