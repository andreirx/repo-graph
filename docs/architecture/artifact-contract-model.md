# Artifact Contract Model

Status: AUTHORITATIVE
Created: 2025-05-08
Governs: All artifact families, refresh behavior, query degradation, provenance tracking

This document defines the canonical artifact ontology for repo-graph. It is authoritative over slice docs, TECH-DEBT entries, and inline comments. The code-level artifact registry must implement this model.

## Core Principle

The unit of architecture is the **artifact family**, not the database table.

A family may map to one table, multiple tables, a read model with no table, or an inferred surface built from lower facts. Tables are storage details. Families are semantic units.

## Truth Classes

Every artifact family belongs to exactly one truth class.

### Class 1: Extracted Facts (Layer 0-1)

Directly source-owned, deterministic, file/parse-unit anchored.

Properties:
- Derived from source file content through deterministic extraction
- One source file owns the fact (or a small, enumerable set of files)
- If the source file is unchanged, the extracted fact is unchanged
- No dependency on other artifact families

Examples:
- `FileVersions` — file path + content hash
- `Nodes` — AST-derived symbols and file markers
- `Edges` — AST-derived relationships (calls, imports, contains)
- `Measurements` — file-local deterministic metrics (complexity, line counts)
- `BoundaryInteractionSurfaces` — IPC/network call sites detected in one file
- `BoundaryChannelDetails` — channel metadata anchored to one surface
- `ContractSchemas` — proto/schema file parse output
- `ContractElements` — schema-internal elements (messages, fields, services)

### Class 2: Deterministic Relationships (Layer 2)

Computed from current lower-layer facts. Not source-owned by one file.

Properties:
- Derived by joining or correlating Layer 0-1 facts
- Deterministic given current snapshot's Layer 0-1 state
- Cannot be copied forward without FK remapping or recomputation
- Must be recomputed or remapped when upstream facts change

Examples:
- `BoundaryContracts` — surface-to-contract-element mapping
- `BoundaryInteractionLinks` — gRPC service-to-implementation links
- Module-to-file ownership mappings (when deterministic)

### Class 3: Projections / Read Models (Layer 2-3)

Summaries and presentation-oriented aggregations.

Properties:
- Computed from lower layers for query convenience
- May be persisted or computed on read
- Regenerate from current lower layers; do not copy forward
- Absence is not source absence; it is query/model absence

Examples:
- Trust summaries
- Module summaries
- Boundary summaries
- Orient aggregator outputs

### Class 4: Hints / Inferences (Layer 3)

Evidence-backed but partial, heuristic, degradable.

Properties:
- Based on patterns, heuristics, or incomplete evidence
- Carry explicit confidence levels
- May be wrong; must be distinguishable from extracted facts
- Degrade explicitly when prerequisites are missing
- Never present as Layer 0 truth

Examples:
- gRPC implementation/client hints
- Framework detection hints
- Dead-code candidates
- Migration risk overlays
- Inferred module boundaries

### Class 5: Governance Overlays (Layer 4)

Human-authored policy and declarations.

Properties:
- Not derived from source code
- Snapshot-independent or overlay-scoped
- Never erased by source refresh
- May reference Layer 0 anchors but are not computed from them

Examples:
- `RequirementDeclarations` — quality gate rules
- `Waivers` — temporary exemptions
- `PolicyFacts` — extracted governance annotations
- Manual assessments

## Refresh Policies

How an artifact family behaves during incremental refresh.

### `ReextractChangedInputs`

Applies to: Extracted Facts (Layer 0-1)

Behavior:
- Changed/config-widened source files: re-extract
- Unchanged source files: copy forward with new snapshot-scoped row IDs
- Deleted source files: do not copy forward

### `RecomputeFromCurrentSnapshot`

Applies to: Deterministic Relationships, Projections

Behavior:
- Recompute entirely from current snapshot's lower-layer facts
- Never copy forward rows from parent snapshot
- Run after Layer 0-1 refresh is complete

### `CopyForwardWithFkRemap`

Applies to: Child artifacts with FK to parent artifact (e.g., channel details → surface)

Behavior:
- Copy forward only if parent artifact was copied forward
- Remap FK to new parent row ID
- Requires old→new ID mapping from parent copy-forward

### `MarkImpactedDeferRecompute`

Applies to: Expensive hints/inferences

Behavior:
- Do not eagerly recompute
- Mark rows as `impacted` when upstream Layer 0 changes
- Recompute on demand or in background

### `NeverCopyForward`

Applies to: Projections computed on read

Behavior:
- Always regenerate
- No persistence or transient persistence only

### `SnapshotIndependent`

Applies to: Governance Overlays

Behavior:
- Not affected by source refresh
- Persist independently of snapshots
- May reference snapshot-scoped artifacts by stable key

## Identity Policies

How row identity works for each family.

### `StableLogicalKey`

Row has a stable logical identity (e.g., `repo:file:symbol`) separate from row UID.
Row UID is snapshot-scoped. Logical key is stable across snapshots.

Applies to: Nodes, Edges, Measurements, Surfaces, Schemas, Elements

### `SnapshotRowId`

Identity is snapshot-row-id only. No stable key.
Used for computed relationships where identity is derived from current inputs.

Applies to: BoundaryContracts, BoundaryInteractionLinks

### `DerivedFromCurrentSnapshot`

Identity derived from referenced rows in current snapshot.
FKs point to current snapshot's rows, not parent snapshot.

Applies to: Child artifacts with FK relationships

### `SnapshotIndependent`

Identity independent of snapshots. Row persists across all snapshots.

Applies to: Repos, RequirementDeclarations, Waivers

## Degradation Policies

How missing/incomplete data should be reported to consumers.

### `MustBePresent`

Absence is an error if source files exist.
Query surfaces should fail or warn loudly.

Applies to: Core extracted facts (Nodes, Edges, FileVersions)

### `MayBeOmittedWithExplicitUnknown`

May be absent. Query surfaces report explicit "unknown" state.
Consumers must not interpret absence as "known zero."

Applies to: Boundaries, Contracts, Measurements, Inferences

### `MustTriggerWarning`

Absence triggers warning in diagnostics.
System is functional but degraded.

Applies to: Optional enrichment families

### `MustTriggerRebuildRecommendation`

Absence or staleness triggers recommendation to rebuild.

Applies to: Stale or corrupted artifacts

### `UnsupportedOnEmbodiment`

Known unsupported on current indexer embodiment.
Not a bug; a capability gap.

Applies to: ModuleCandidates on Rust-only path, ProjectSurfaces on Rust-only path

## Provenance Policies

How artifact provenance is tracked.

### `DirectFromSourceFile`

Provenance is the source file itself (file path + content hash).

Applies to: All Layer 0-1 extracted facts

### `DerivedFromLayer0Items`

Provenance is specific Layer 0 stable keys.
Must be recorded per-row.

Applies to: Deterministic relationships, hints/inferences

### `DerivedFromArtifactFamilies`

Provenance is other artifact families (transitive).
Used for multi-layer derivations.

Applies to: Complex projections

### `HumanAuthored`

No automated provenance. Human-authored content.

Applies to: Governance overlays

## Impact Policies

What happens when upstream Layer 0 changes.

### `RecomputeOnRelevantLayer0Change`

Recompute immediately when relevant Layer 0 items change.

Applies to: Cheap deterministic relationships

### `MarkImpactedOnRelevantLayer0Change`

Mark rows as `impacted` when their provenance anchors change.
Defer recomputation.

Applies to: Expensive hints, inferences, relationships

### `MarkImpactedOnAnyLayer0Change`

Mark rows as `impacted` on any Layer 0 change in the snapshot.
Coarse-grained impact for families that depend on global state.

Applies to: Global summaries, trust aggregates

### `UnaffectedByLayer0Refresh`

Not affected by Layer 0 refresh.

Applies to: Governance overlays

## Per-Row Freshness Model

Every artifact row that is not Layer 0 has a freshness state.

### Freshness States

- `current` — row is computed from current Layer 0 state
- `impacted` — upstream Layer 0 changed; row may be stale but is still useful
- `stale` — row is known to be out of date; use with caution
- `unknown` — freshness cannot be determined (e.g., legacy rows)

### Freshness Tracking

Per-row freshness is tracked via:
- `freshness_state` column (TEXT)
- `freshness_updated_at` column (TEXT, ISO 8601)

Layer 0 families do not need explicit freshness columns. Their freshness is implicit from source file hash matching.

### Query Filtering

Read surfaces can filter by freshness:
- `CurrentOnly` — only rows with `freshness_state = 'current'`
- `CurrentAndImpacted` — rows that are current or impacted (default for agent surfaces)
- `All` — include stale and unknown

## Per-Row Provenance Model

Every artifact row that is not Layer 0 tracks its provenance.

### Provenance Structure

Provenance is stored as JSON, structured per truth class:

**Deterministic Relationships:**
```json
{
  "depends_on": [
    {"family": "BoundaryInteractionSurfaces", "stable_key": "..."},
    {"family": "ContractElements", "stable_key": "..."}
  ]
}
```

**Hints/Inferences:**
```json
{
  "basis": [
    {"family": "Nodes", "stable_key": "..."},
    {"family": "Edges", "stable_key": "..."}
  ],
  "extractor": "grpc_impl_detector",
  "confidence": 0.85
}
```

### Impact Propagation

When Layer 0 items change during refresh:
1. Identify changed Layer 0 stable keys
2. Query all upper-layer rows with provenance referencing those keys
3. Update `freshness_state` to `impacted`
4. Record `freshness_updated_at`

This is a precise, row-level operation, not a family-level flag.

## Canonical Family Inventory

### Extracted Facts (Layer 0-1)

| Family | Truth Kind | Refresh Policy | Identity Policy | Degradation Policy |
|--------|------------|----------------|-----------------|-------------------|
| FileVersions | ExtractedFact | ReextractChangedInputs | StableLogicalKey | MustBePresent |
| Nodes | ExtractedFact | ReextractChangedInputs | StableLogicalKey | MustBePresent |
| Edges | ExtractedFact | ReextractChangedInputs | StableLogicalKey | MustBePresent |
| Measurements | ExtractedFact | ReextractChangedInputs | StableLogicalKey | MayBeOmittedWithExplicitUnknown |
| BoundaryInteractionSurfaces | ExtractedFact | ReextractChangedInputs | StableLogicalKey | MayBeOmittedWithExplicitUnknown |
| BoundaryChannelDetails | ExtractedFact | CopyForwardWithFkRemap | DerivedFromCurrentSnapshot | MayBeOmittedWithExplicitUnknown |
| ContractSchemas | ExtractedFact | ReextractChangedInputs | StableLogicalKey | MayBeOmittedWithExplicitUnknown |
| ContractElements | ExtractedFact | ReextractChangedInputs | DerivedFromCurrentSnapshot | MayBeOmittedWithExplicitUnknown |

### Deterministic Relationships (Layer 2)

| Family | Truth Kind | Refresh Policy | Identity Policy | Degradation Policy |
|--------|------------|----------------|-----------------|-------------------|
| BoundaryContracts | DeterministicRelationship | RecomputeFromCurrentSnapshot | SnapshotRowId | MayBeOmittedWithExplicitUnknown |
| BoundaryInteractionLinks | DeterministicRelationship | RecomputeFromCurrentSnapshot | SnapshotRowId | MayBeOmittedWithExplicitUnknown |

### Hints / Inferences (Layer 3)

| Family | Truth Kind | Refresh Policy | Identity Policy | Degradation Policy |
|--------|------------|----------------|-----------------|-------------------|
| Inferences | Inference | MarkImpactedDeferRecompute | StableLogicalKey | MayBeOmittedWithExplicitUnknown |
| ModuleCandidates | Inference | RecomputeFromCurrentSnapshot | StableLogicalKey | UnsupportedOnEmbodiment |
| ProjectSurfaces | Inference | RecomputeFromCurrentSnapshot | StableLogicalKey | UnsupportedOnEmbodiment |
| ProjectSurfaceEvidence | Inference | CopyForwardWithFkRemap | DerivedFromCurrentSnapshot | UnsupportedOnEmbodiment |

### Governance Overlays (Layer 4)

| Family | Truth Kind | Refresh Policy | Identity Policy | Degradation Policy |
|--------|------------|----------------|-----------------|-------------------|
| RequirementDeclarations | GovernanceOverlay | SnapshotIndependent | SnapshotIndependent | MayBeOmittedWithExplicitUnknown |
| Waivers | GovernanceOverlay | SnapshotIndependent | SnapshotIndependent | MayBeOmittedWithExplicitUnknown |
| PolicyFacts | ExtractedFact | ReextractChangedInputs | StableLogicalKey | MayBeOmittedWithExplicitUnknown |

## Implementation Authority

The code-level artifact registry in `rust/crates/artifact-contracts` is the operational authority for this model.

This document describes the model.
The registry implements it.
Tests enforce it.

If this document and the registry disagree, the registry is authoritative for current behavior, but this document governs correctness. Disagreement must be resolved by updating one or the other.

## Layer 0 Authoritative Rule

Layer 0-1 extracted facts are the ground truth.

All upper layers are derived from, computed from, or anchored to Layer 0.

No upper-layer artifact may exist without traceable provenance to Layer 0 anchors.

No upper-layer artifact may be presented as extracted fact.

No query surface may conflate extracted facts with hints or inferences.

## Honest Degradation Rule

The system must never:
- Present missing data as known-zero
- Present impacted data as current
- Present unsupported features as absent data
- Present inference as extracted fact

The system must always:
- Distinguish current from impacted from stale from unknown
- Report unsupported embodiment capabilities explicitly
- Attach confidence to non-extracted data
- Trace upper-layer artifacts to Layer 0 provenance
