# TC-1: Snapshot/Evidence Toolchain Provenance

Status: PLANNED
Depends: None (support slice)
Unblocks: NC-1 provenance, BC-1 provenance, comparability checks
Track: Toolchain-Aware Evidence Import
Layer: 1 (architectural substrate — provenance)

## Goal

Persist minimal provenance metadata for snapshots and imported evidence,
enabling reproducibility defense and cross-snapshot comparability checks.

**This is NOT generic host tool inventory.** An AI agent can probe the host
ad hoc. Repo-graph persists only what matters for snapshot/evidence lineage.

## What Repo-Graph Persists vs. What AI Does Ad Hoc

| Concern | Owner | Why |
|---------|-------|-----|
| "What tools are installed right now?" | AI agent | Ephemeral, changes with PATH/shell/venv |
| "Can this machine run llvm-cov?" | AI agent | Live capability check |
| "What tool produced this coverage?" | Repo-graph | Evidence lineage |
| "Are these two snapshots comparable?" | Repo-graph | Reproducibility |
| "What build context was attached?" | Repo-graph | Extraction fidelity context |

## Scope

### In Scope

1. **Evidence source provenance** (persisted when evidence is imported)
   - Tool name + version that produced the evidence
   - Example: `llvm-cov 17.0.0` when importing coverage
   - Example: `clang-tidy 17.0.0` when importing findings

2. **Build context provenance** (persisted when build context is attached)
   - `compile_commands.json` path and content hash
   - Compiler family if derivable from compile commands
   - Example: "build context from ./build/compile_commands.json, sha256=abc123"

3. **Snapshot extraction provenance** (already partially exists)
   - Extractor versions (repo-graph indexer version)
   - Language-specific toolchain if extraction depended on it
   - Example: Python interpreter path if venv-aware extraction used it

4. **Comparability predicate**
   - Query: "are snapshot A and snapshot B comparable?"
   - Answer based on provenance vector differences
   - Surface incomparability reasons explicitly

### Out of Scope

- Generic host tool inventory
- Scanning PATH for everything installed
- Persisting tools not used by this snapshot/evidence
- Running tools (that's NC-1, BC-1, AF-1)

## Design

### Provenance Model

```rust
/// Provenance attached to imported evidence.
pub struct EvidenceProvenance {
    pub tool: String,           // "llvm-cov", "clang-tidy", "istanbul"
    pub version: Option<String>, // "17.0.0"
    pub source_path: Option<String>, // Path to imported artifact
    pub source_hash: Option<String>, // Content hash for reproducibility
}

/// Provenance attached to build context.
pub struct BuildContextProvenance {
    pub compile_commands_path: Option<String>,
    pub compile_commands_hash: Option<String>,
    pub compiler_family: Option<String>,  // "clang", "gcc" if derivable
}

/// Full provenance vector for a snapshot.
pub struct SnapshotProvenance {
    pub indexer_version: String,
    pub indexed_at: String,
    pub build_context: Option<BuildContextProvenance>,
    pub coverage_source: Option<EvidenceProvenance>,
    pub findings_sources: Vec<EvidenceProvenance>,
}
```

### Comparability Check

```rust
pub fn snapshots_comparable(a: &SnapshotProvenance, b: &SnapshotProvenance) -> ComparabilityResult {
    let mut reasons = Vec::new();
    
    // Different build context = may affect extraction
    if a.build_context != b.build_context {
        reasons.push("build context differs");
    }
    
    // Coverage from different tools = not directly comparable
    if a.coverage_source.as_ref().map(|c| &c.tool) != b.coverage_source.as_ref().map(|c| &c.tool) {
        reasons.push("coverage tool differs");
    }
    
    if reasons.is_empty() {
        ComparabilityResult::Comparable
    } else {
        ComparabilityResult::NotComparable { reasons }
    }
}
```

### Persistence

Extend existing snapshot metadata:

```sql
-- Already exists, extend with provenance fields
ALTER TABLE snapshots ADD COLUMN provenance_json TEXT;
```

The `provenance_json` contains the serialized `SnapshotProvenance`.

### CLI

```bash
# Show provenance for a snapshot
rmap provenance ./repo.db repo-uid

# Compare two snapshots for comparability
rmap provenance compare ./repo.db repo-uid --snapshot-a <uid> --snapshot-b <uid>
```

## Integration Points

### NC-1 (Coverage Import)
When importing llvm-cov export:
- Extract tool version from report metadata or CLI flag
- Persist as `coverage_source` in snapshot provenance

### BC-1 (Build Context Import)
When importing compile_commands.json:
- Hash the file content
- Extract compiler family from commands
- Persist as `build_context` in snapshot provenance

### AF-1 (Findings Import)
When importing analyzer findings:
- Record tool + version per import
- Persist as `findings_sources` in snapshot provenance

## Definition of Done

- [ ] `SnapshotProvenance` model
- [ ] Provenance persistence in snapshots table
- [ ] `rmap provenance` CLI command
- [ ] Comparability predicate
- [ ] Integration hooks for NC-1, BC-1, AF-1 to record provenance
- [ ] Unit tests for comparability logic

## What This Slice Does NOT Do

- Does not scan PATH
- Does not run `--version` on arbitrary tools
- Does not persist "machine inventory"
- Does not duplicate what an AI agent can do live

## Test Plan

1. **Unit tests:**
   - Provenance serialization/deserialization
   - Comparability predicate logic

2. **Integration tests:**
   - Import coverage with provenance
   - Import build context with provenance
   - Query provenance via CLI
   - Verify comparability check works

## Why This Is Correct

An AI agent can ask "is clang installed?" at any time. That's ephemeral.

Repo-graph answers "what produced this evidence?" and "can I compare these
snapshots?" Those are durable facts about the graph, not transient host state.

The boundary is: **repo-graph persists lineage, not inventory.**
