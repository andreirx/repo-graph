# STORAGE-ARCH-1: Three-Tier Storage Architecture

## Status

SPECIFICATION — not yet implemented.

## Problem Statement

The current storage model treats all persisted data uniformly:
- User-authored declarations share the same persistence semantics as extracted graph facts
- Low-layer deterministic facts (nodes, edges) occupy SQLite alongside durable policy
- No explicit retention policy distinguishes ephemeral cache from authoritative state
- Daemon restart incurs either full reindex cost or stale data risk

This conflation creates three problems:

1. **Semantic confusion**: Extracted facts are treated as durable truth rather than rebuildable cache
2. **Performance ceiling**: Hot-path graph queries traverse SQLite even for current-snapshot data
3. **Retention bloat**: Old snapshots accumulate without explicit pruning policy

## Target Architecture

Three distinct storage tiers with different persistence semantics:

### Tier A: Durable Authority Store

**Purpose**: Non-derivable, user-authored, policy-bearing state that cannot be reconstructed from source code.

**Contents**:
| Table | Description |
|-------|-------------|
| `repos` | Repository registry |
| `declarations` | Boundaries, requirements, waivers, quality policies |
| `schema_migrations` | Migration tracking |
| `snapshots` (metadata only) | Snapshot manifest, status, timestamps |

**Properties**:
- Authoritative — loss requires manual reconstruction
- Migration-stable — schema changes require careful versioning
- Small volume — grows with policy complexity, not repo size
- SQLite is appropriate backing store

**Retention**: Indefinite (until user deletes)

### Tier B: Derived Snapshot Cache

**Purpose**: Rebuildable extracted/inferred state needed for warm restart, incremental refresh, and historical comparison.

**Contents**:
| Table | Layer | Description |
|-------|-------|-------------|
| `nodes` | 0-1 | Extracted symbols, files, modules |
| `edges` | 0-1 | Extracted relationships |
| `file_versions` | 0-1 | File content hashes, parse status |
| `files` | 0-1 | File metadata |
| `measurements` | 0-1 | Coverage, complexity measurements |
| `inferences` | 2-3 | Liveness, ownership, framework hints |
| `annotations` | 2 | Extracted doc annotations |
| `unresolved_edges` | 1 | Unresolved import references |
| `extraction_edges` | 1 | Staging for incremental extraction |
| `staged_edges` | 1 | Copy-forward staging |
| `file_signals` | 1 | Per-file extraction signals |
| `module_candidates` | 2-3 | Discovered module boundaries |
| `module_candidate_evidence` | 2-3 | Module discovery evidence |
| `module_file_ownership` | 2-3 | File-to-module mapping |
| `module_discovery_diagnostics` | 2-3 | Module inference metadata |
| `project_surfaces` | 3 | Framework detection results |
| `project_surface_evidence` | 3 | Framework detection evidence |
| `surface_entrypoints` | 3 | Detected entrypoints |
| `surface_config_roots` | 3 | Config file roots |
| `surface_env_dependencies` | 3 | Environment dependencies |
| `surface_env_evidence` | 3 | Env dependency evidence |
| `surface_fs_mutations` | 3 | Filesystem mutation surfaces |
| `surface_fs_mutation_evidence` | 3 | FS mutation evidence |
| `boundary_provider_facts` | 2 | Boundary provider signals |
| `boundary_consumer_facts` | 2 | Boundary consumer signals |
| `boundary_links` | 2 | Boundary relationships |
| `boundary_interaction_surfaces` | 2 | Interaction surface catalog |
| `boundary_channel_details` | 2 | Channel implementation details |
| `boundary_contracts` | 2 | Contract specifications |
| `boundary_interaction_links` | 2 | Contract-to-surface links |
| `contract_schemas` | 2 | Schema definitions |
| `contract_elements` | 2 | Schema element catalog |
| `generated_code_mappings` | 2 | Code generation mappings |
| `artifacts` | varies | Snapshot-scoped artifacts |
| `evidence_links` | varies | Evidence linkage |
| `quality_assessments` | 3 | Computed quality verdicts |
| `semantic_facts` | 2 | Policy fact extraction |
| `status_mappings` | 2 | Status code mappings |
| `behavioral_markers` | 2 | Behavioral signal markers |
| `return_fates` | 2 | Return value fate tracking |

**Properties**:
- Rebuildable — can be regenerated from source via extraction
- Retention-limited — keep current + parent + explicit baseline only
- Versioned — tagged with extractor/schema version
- Safe to invalidate — loss triggers reindex, not data loss

**Retention Policy**:
- Current snapshot: always retain
- Parent snapshot: retain for incremental refresh
- Baseline snapshot: retain if explicitly marked for comparison
- All others: eligible for pruning

### Tier C: Live Working Graph

**Purpose**: Fast hot-path query execution for current snapshot state.

**Contents**:
- Current snapshot node adjacency maps (in-memory)
- Edge traversal indexes (in-memory)
- Symbol/resource/module lookup tables (in-memory)
- Path/cycle memoization structures (in-memory)

**Properties**:
- In-memory only — not persisted
- Rebuilt on startup from Tier B cache (or fresh index)
- Not authoritative — derived from persisted snapshot
- Command-optimized data structures

**Retention**: Session lifetime (daemon process)

## Command-to-Tier Mapping

Commands are classified by their primary data source:

### Tier C Candidates (Live Working Graph)

These commands operate primarily on current-snapshot graph structure:

| Command | Reason |
|---------|--------|
| `callers` | Graph traversal, current snapshot |
| `callees` | Graph traversal, current snapshot |
| `path` | Path finding, current snapshot |
| `cycles` | SCC analysis, current snapshot |
| `imports` | Direct edge lookup, current snapshot |
| `dead` | Reachability analysis, current snapshot |
| `deps` | Module dependency traversal, current snapshot |

### Tier B Required (Derived Snapshot Cache)

These commands need persisted derived data or history:

| Command | Reason |
|---------|--------|
| `churn` | Git history correlation |
| `hotspots` | Complexity + churn join |
| `risk` | Coverage + hotspot join |
| `coverage` | Measurement data |
| `orient` | Multi-source summary |
| `trust` | Aggregated statistics |
| `stats` | Count aggregations |
| `surfaces` | Framework detection results |
| `boundaries` | Boundary inference + declarations |
| `modules` | Module candidate data |
| `docs` | Annotation extraction |
| `resource` | Resource node queries |
| `contracts` | Contract schema data |
| `explain` | Evidence chain traversal |

### Tier A + Tier B Required (Authority + Derived Cache Join)

These commands require both authoritative policy AND derived snapshot data:

| Command | Authority Source | Derived Source | Reason |
|---------|-----------------|----------------|--------|
| `gate` | declarations, requirements | nodes, edges, measurements | Evaluate policy against current code state |
| `assess` | quality policies | measurements, inferences | Compute verdicts from policy + extracted data |
| `violations` | boundary declarations | edges, module ownership | Check boundaries against current imports |

### Tier A Only (Durable Authority Store)

These commands read or write authoritative policy without derived data:

| Command | Reason |
|---------|--------|
| `policy` | Policy CRUD |
| `declare` | Declaration management |
| `repo` | Registry management |

## Migration Invariants

### Invariant 1: Authority Survives Cache Loss

If all Tier B data is deleted, Tier A data must be intact. Reindexing must restore Tier B without affecting Tier A.

### Invariant 2: Live Graph Rebuilds from Cache

Tier C must be reconstructible from Tier B without accessing source files. Warm restart loads from cache; cold restart requires reindex.

### Invariant 3: No Cross-Tier Authority Leakage

User-authored facts (declarations) never reside in Tier B or C. Extracted facts never reside in Tier A.

### Invariant 4: Retention Bounds are Explicit

No snapshot persists in Tier B without explicit retention classification (current/parent/baseline/prunable).

### Invariant 5: Schema Versioning is Per-Tier

Tier A and Tier B may evolve on different migration tracks. Tier B schema changes may allow cache invalidation without migration.

## Implementation Phases

### Phase 0: Specification (this document)
- Define tier boundaries
- Classify existing tables
- Map commands to tiers
- State invariants

### Phase 1: Observability (PERF-OBS-1)
- Instrument table sizes, row counts
- Measure command latency by tier
- Profile memory usage
- Baseline before changes

### Phase 2: Semantic Separation (CACHE-SEMANTICS-1)
- Mark Tier B tables as cache in docs/code
- Define invalidation/versioning rules
- Implement retention policy hooks
- No storage-engine changes yet

### Phase 3: Live Graph Introduction (LIVE-GRAPH-1, LIVE-GRAPH-2)
- Build in-memory graph model
- Load from Tier B on startup
- Migrate graph-traversal commands
- Validate parity with SQLite path

### Phase 4: Cache Optimization (future)
- Evaluate Tier B backing store options
- Consider physical separation (separate DB file)
- Implement aggressive pruning
- Only if metrics justify

## Decision Log

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Tier B backing store | SQLite initially | Reduce migration risk; separate semantics before swapping engines |
| Retention policy | current + parent + baseline | Supports refresh, comparison, and pruning |
| First command migration | callers/callees/path | Graph-native, current-snapshot, high latency sensitivity |
| Physical DB split | Deferred | Logical separation first; measure before splitting |

## Open Questions

1. Should `files` table be Tier A (registry) or Tier B (cache)? Currently classified as Tier B because file list is derivable from source tree, but `file_uid` is used as FK across tiers.

2. Should baseline snapshot selection be explicit (user-declared) or implicit (most recent ready snapshot before current)?

3. What is the warm-start time budget for Tier C graph loading from Tier B? Acceptable latency before daemon reports ready?

## References

- `docs/TECH-DEBT.md` — performance issues that motivated this redesign
- `agent_docs/architecture.md` — fact certainty layer model
- User discussion on storage economics (2026-05-22)
