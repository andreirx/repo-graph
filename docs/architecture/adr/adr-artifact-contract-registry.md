# ADR: Artifact Contract Registry

Status: ACCEPTED
Date: 2025-05-08
Deciders: Product owner
Supersedes: Ad-hoc refresh behavior, prose-only layer documentation

## Context

The repo-graph product extracts facts from codebases and surfaces them to AI agents for orientation. The system persists multiple artifact families with different semantic properties:

- Extracted facts (AST nodes, edges, measurements)
- Deterministic relationships (boundary-to-contract links)
- Hints and inferences (dead code candidates, framework detection)
- Governance overlays (quality gates, waivers)

The refresh pipeline (incremental re-indexing) was treating all artifacts similarly, leading to:

1. **Semantic confusion**: Deterministic relationships were being treated like extracted facts
2. **FK integrity bugs**: Copied-forward rows pointed to parent snapshot UIDs instead of current snapshot
3. **Undocumented degradation**: Missing artifacts were indistinguishable from unsupported features
4. **Layer drift**: The fact certainty model existed only in prose, not in code

The `boundary_interaction_links` refresh bug exposed this architectural gap: the system had no explicit model of artifact truth classes, refresh policies, or provenance requirements.

## Decision

We will implement an **Artifact Contract Registry** as the canonical, code-level authority for artifact semantics.

### Key Decisions

#### 1. Option A: Full Registry Now

We choose to classify ALL artifact families upfront rather than incrementally.

Rationale:
- This is core product architecture, not a local fix
- Incomplete classification invites continued drift
- The modeling cost is bounded; the families are known

#### 2. Dedicated Crate

The registry lives in a dedicated crate: `rust/crates/artifact-contracts`

Rationale:
- Storage is an adapter; artifact semantics are core policy
- Multiple crates will consume the registry (indexer, storage, agent, CLI)
- A vague "core" crate is premature; this concern is sharply bounded

#### 3. Per-Row Freshness Tracking

Freshness state (current/impacted/stale/unknown) is tracked per artifact row, not per family.

Rationale:
- Precise impact propagation when Layer 0 changes
- Agents can filter by freshness at query time
- Coarse family-level tracking would hide partial staleness

#### 4. Per-Row Provenance to Layer 0

Every non-Layer-0 artifact row must record its provenance: which Layer 0 stable keys it depends on.

Rationale:
- Enables precise impact propagation
- Makes the system auditable
- Prevents opaque hints with no evidence lineage

#### 5. Upper Layers May Be Marked Impacted

Not all upper-layer artifacts must be eagerly recomputed when Layer 0 changes. Some may be marked `impacted` and recomputed later.

Rationale:
- Some derivations are expensive
- Impacted data is still useful orientation
- Honest freshness state is better than forced recomputation

#### 6. Registry Is Authoritative Over Prose

The code-level registry is authoritative for operational behavior. Architecture docs describe intent; the registry implements it.

Rationale:
- Code cannot drift silently like docs
- Tests enforce registry correctness
- Disagreement between docs and registry must be explicitly resolved

## Consequences

### Positive

- Refresh behavior becomes policy-driven, not ad-hoc
- New artifact families must declare their contracts explicitly
- Query surfaces can report degradation based on contract, not guesswork
- Impact propagation becomes precise and auditable
- Layer confusion is prevented by type system

### Negative

- Initial classification effort is non-trivial
- Schema changes required for freshness/provenance columns
- Existing rows need migration (mark as `unknown` or backfill)
- Some families may be classified provisionally until semantics stabilize

### Neutral

- Refresh pipeline requires refactoring to consume registry
- Agent surfaces require changes to consume freshness state
- Documentation must be aligned to registry (not compete with it)

## Alternatives Considered

### Option B: Minimal Registry, Expand Later

Register only families needed for current bugs, expand incrementally.

Rejected because:
- This is core architecture, not a bug fix
- Incomplete classification guarantees continued drift
- The families are already known; deferral adds no value

### Per-Family Freshness (Not Per-Row)

Track freshness at family level, not row level.

Rejected because:
- Hides partial staleness
- Prevents precise impact propagation
- Would require re-visiting when precision is needed

### Registry in Storage Crate

Put the registry in `repo-graph-storage`.

Rejected because:
- Storage is an adapter
- Artifact contracts are core policy
- Dependency rule violation (policy depending on adapter)

## Implementation

See execution slices:
- `acr-1-artifact-contracts-crate.md`
- `acr-2-refresh-policy-integration.md`
- `acr-3-provenance-and-freshness-schema.md`
- `acr-4-impact-propagation.md`
- `acr-5-boundary-contract-proof-case.md`
- `acr-6-query-degradation-and-freshness.md`

## References

- `docs/architecture/artifact-contract-model.md` — full model specification
- `docs/VISION.md` — product fact-certainty doctrine
- `CLAUDE.md` — layer model (to be updated)
