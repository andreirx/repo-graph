# ACR-3: Provenance and Freshness Schema

Status: NOT STARTED
Depends: `acr-2-refresh-policy-integration.md`
Follow-on: `acr-4-impact-propagation.md`
Track: Core Infrastructure — Artifact Contract Registry

## Objective

Add per-row freshness state and provenance columns to artifact tables that require them. Define the storage schema for tracking which Layer 0 items each derived artifact depends on.

## Scope

### In Scope

- Schema changes for freshness columns
- Schema changes for provenance columns
- Migration for existing rows
- Storage port extensions for freshness/provenance
- Provenance JSON structure definition

### Out of Scope

- Impact propagation logic (ACR-4)
- Query-layer freshness filtering (ACR-6)
- Populating provenance during extraction (ACR-4)

## Tables Requiring Changes

Based on artifact contracts, these families need per-row freshness/provenance:

### Deterministic Relationships (Layer 2)
- `boundary_contracts`
- `boundary_interaction_links`

### Hints / Inferences (Layer 3)
- `inferences`
- `project_surfaces`
- `project_surface_evidence`
- `surface_entrypoints`
- `surface_config_roots`
- `surface_env_dependencies`
- `surface_env_evidence`
- `surface_fs_mutations`
- `surface_fs_mutation_evidence`
- `module_candidates`

### Tables NOT Requiring Changes

Layer 0-1 extracted facts:
- `file_versions` — freshness is implicit from file hash
- `nodes` — freshness is implicit from source file
- `edges` — freshness is implicit from source file
- `measurements` — freshness is implicit from source file
- `boundary_interaction_surfaces` — freshness is implicit from source file
- `boundary_channel_details` — freshness is implicit from parent surface
- `contract_schemas` — freshness is implicit from proto file
- `contract_elements` — freshness is implicit from parent schema

Layer 4 governance:
- `requirement_declarations` — snapshot-independent
- `waivers` — snapshot-independent

## Schema Changes

### New Columns

For each table requiring freshness tracking:

```sql
ALTER TABLE <table_name> ADD COLUMN freshness_state TEXT NOT NULL DEFAULT 'current';
ALTER TABLE <table_name> ADD COLUMN freshness_updated_at TEXT;
ALTER TABLE <table_name> ADD COLUMN provenance_json TEXT;
```

### Freshness State Values

- `current` — computed from current Layer 0 state
- `impacted` — upstream Layer 0 changed; row may be stale
- `stale` — known to be out of date
- `unknown` — freshness cannot be determined (legacy/migrated rows)

### Provenance JSON Structure

**For Deterministic Relationships:**
```json
{
  "version": 1,
  "depends_on": [
    {
      "family": "BoundaryInteractionSurfaces",
      "stable_key": "repo:file.c:123:5:socket:outbound"
    },
    {
      "family": "ContractElements",
      "stable_key": "repo:proto/service.proto#Greeter.SayHello"
    }
  ]
}
```

**For Hints/Inferences:**
```json
{
  "version": 1,
  "basis": [
    {
      "family": "Nodes",
      "stable_key": "repo:src/server.ts#serve:SYMBOL:FUNCTION"
    },
    {
      "family": "Edges",
      "stable_key": "repo:src/server.ts#serve->listen:CALLS"
    }
  ],
  "extractor": "grpc_impl_detector",
  "extraction_context": {
    "confidence": 0.85,
    "version": "1.0"
  }
}
```

## Migration Strategy

### For Existing Rows

Existing rows in these tables will be migrated with:
- `freshness_state = 'unknown'`
- `freshness_updated_at = NULL`
- `provenance_json = NULL`

This is honest: we don't know the provenance of rows created before tracking was added.

### For New Rows

New rows created after this migration must have:
- `freshness_state` set appropriately
- `provenance_json` populated if the family requires provenance tracking

## Storage Port Extensions

### Write Operations

```rust
/// Insert a derived artifact row with provenance.
pub fn insert_boundary_contract(
    &self,
    contract: &BoundaryContract,
    provenance: &Provenance,
) -> Result<(), StorageError>;

/// Insert an inference row with provenance.
pub fn insert_inference(
    &self,
    inference: &Inference,
    provenance: &Provenance,
) -> Result<(), StorageError>;
```

### Read Operations

```rust
/// Get provenance for a derived artifact row.
pub fn get_provenance(
    &self,
    family: ArtifactFamily,
    row_uid: &str,
) -> Result<Option<Provenance>, StorageError>;

/// Get rows by freshness state.
pub fn get_rows_by_freshness(
    &self,
    family: ArtifactFamily,
    snapshot_uid: &str,
    state: FreshnessState,
) -> Result<Vec<RowRef>, StorageError>;
```

### Update Operations

```rust
/// Mark rows as impacted.
pub fn mark_rows_impacted(
    &self,
    family: ArtifactFamily,
    row_uids: &[String],
) -> Result<u64, StorageError>;

/// Mark rows matching provenance as impacted.
pub fn mark_impacted_by_provenance(
    &self,
    snapshot_uid: &str,
    family: ArtifactFamily,
    changed_stable_keys: &[String],
) -> Result<u64, StorageError>;
```

## Provenance Types

```rust
// In artifact-contracts crate or storage crate

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub version: u32,
    pub depends_on: Vec<ProvenanceAnchor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extractor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extraction_context: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceAnchor {
    pub family: String,  // ArtifactFamily as string
    pub stable_key: String,
}

impl Provenance {
    pub fn from_layer0_items(anchors: Vec<ProvenanceAnchor>) -> Self {
        Self {
            version: 1,
            depends_on: anchors,
            extractor: None,
            extraction_context: None,
        }
    }
    
    pub fn with_extractor(mut self, extractor: &str) -> Self {
        self.extractor = Some(extractor.to_string());
        self
    }
}
```

## Migration Script

```sql
-- Migration: Add freshness and provenance columns

-- boundary_contracts (if table exists)
ALTER TABLE boundary_contracts ADD COLUMN freshness_state TEXT NOT NULL DEFAULT 'unknown';
ALTER TABLE boundary_contracts ADD COLUMN freshness_updated_at TEXT;
ALTER TABLE boundary_contracts ADD COLUMN provenance_json TEXT;

-- boundary_interaction_links
ALTER TABLE boundary_interaction_links ADD COLUMN freshness_state TEXT NOT NULL DEFAULT 'unknown';
ALTER TABLE boundary_interaction_links ADD COLUMN freshness_updated_at TEXT;
ALTER TABLE boundary_interaction_links ADD COLUMN provenance_json TEXT;

-- inferences
ALTER TABLE inferences ADD COLUMN freshness_state TEXT NOT NULL DEFAULT 'unknown';
ALTER TABLE inferences ADD COLUMN freshness_updated_at TEXT;
-- inferences already has basis_json which serves as provenance

-- project_surfaces
ALTER TABLE project_surfaces ADD COLUMN freshness_state TEXT NOT NULL DEFAULT 'unknown';
ALTER TABLE project_surfaces ADD COLUMN freshness_updated_at TEXT;
ALTER TABLE project_surfaces ADD COLUMN provenance_json TEXT;

-- ... similar for other tables

-- Create indexes for freshness queries
CREATE INDEX IF NOT EXISTS idx_boundary_contracts_freshness 
    ON boundary_contracts(snapshot_uid, freshness_state);
CREATE INDEX IF NOT EXISTS idx_boundary_interaction_links_freshness 
    ON boundary_interaction_links(snapshot_uid, freshness_state);
CREATE INDEX IF NOT EXISTS idx_inferences_freshness 
    ON inferences(snapshot_uid, freshness_state);
```

## Definition of Done

- [ ] Schema migration defined and tested
- [ ] Freshness columns added to all required tables
- [ ] Provenance columns added to all required tables
- [ ] Provenance types defined in code
- [ ] Storage port extensions implemented
- [ ] Existing rows migrated with `freshness_state = 'unknown'`
- [ ] Indexes created for freshness queries
- [ ] Migration tested on real database

## Validation Commands

```bash
cd /Users/apple/Documents/APLICATII\ BIJUTERIE/repo-graph/rust
cargo build -p repo-graph-storage
cargo test -p repo-graph-storage

# Test migration on a real database
rmap index /path/to/test/repo
# Check schema
sqlite3 ~/.local/share/repo-graph/repo-graph.db ".schema boundary_interaction_links"
```

## Notes

- `inferences` table already has `basis_json` which serves a similar purpose to `provenance_json`. Decide whether to unify or keep separate.
- Indexes on freshness columns are important for query performance
- Migration should be idempotent (can run multiple times safely)
- Consider adding CHECK constraints for freshness_state values
