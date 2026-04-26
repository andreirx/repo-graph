# Documentation-Derived Semantic Facts

Design document for the semantic evidence layer in repo-graph.

## Problem Statement

Repo-graph extracts structural facts from code (symbols, edges, modules, measurements). But codebases also contain semantic intelligence in documentation that affects agent reasoning:

- Migration/rewrite/replacement relationships
- Alternative implementation relationships
- Environment surfaces (dev/stage/prod)
- Deprecation notices and migration paths
- Operational constraints not visible in code structure

This intelligence is invisible to the current graph. An agent cannot know from structure alone that "module A is being replaced by module B" or "service X runs only in production".

## Design Principle

**This is a semantic evidence layer, not graph extraction.**

- It does not produce nodes/edges in the core graph
- It does not affect call/import resolution
- It produces queryable facts with explicit provenance
- Facts are advisory, not authoritative
- Confidence levels distinguish explicit statements from inferences

**V1 is discovery-oriented, not archival.**

- Read documentation/config files live from disk at extraction time
- Persist only extracted semantic facts, not full doc text
- Source of truth is the live repo files, not stored blobs
- Optimized for current-state discovery, not historical replay

## Scope

### In scope (v1) — current-state discovery

1. **Live doc reading** - read docs from disk at extraction time
   - README.md, ARCHITECTURE.md, CONTRIBUTING.md
   - Module-level docs (package README, Cargo.toml description)
   - Config files (docker-compose.yml, .env.*)
   - Generated MAP.md files (with lower confidence)

2. **Relationship extraction** - explicit statements
   - `replacement_for`: "X replaces Y"
   - `alternative_to`: "X and Y are alternatives"
   - `deprecated_by`: "X is deprecated in favor of Y"
   - `migration_path`: "migrate from X to Y"

3. **Environment detection** - config/deployment context
   - Environment surfaces from docker-compose, .env files, deploy configs
   - Per-environment feature flags or configuration differences

4. **Minimal provenance** - facts only, no blobs
   - Source file, line range, short excerpt
   - Content hash for staleness detection
   - Extraction method and confidence
   - Generated vs authored flag

### Out of scope (v1)

- Full doc text persistence (read live, store facts only)
- Historical replay / re-extraction from stored docs
- Javadoc/Doxygen XML ingestion
- Natural language understanding (beyond pattern matching)
- Cross-repo relationship detection
- Inline doc comment extraction (deferred)

### Deferred to v2 (only if needed)

- Raw doc text persistence for archival
- Re-extraction without filesystem access
- Snapshot-based doc versioning

## Data Model

### Semantic Fact

```
semantic_facts table
--------------------
fact_uid: TEXT PRIMARY KEY
repo_uid: TEXT NOT NULL FK

fact_kind: TEXT NOT NULL  -- 'replacement_for', 'alternative_to', 'deprecated_by', 
                          -- 'migration_path', 'environment_surface', 'operational_constraint'

-- Subject reference (typed for query-time resolution)
subject_ref: TEXT NOT NULL      -- the reference value (path, key, name, or text)
subject_ref_kind: TEXT NOT NULL -- 'module', 'symbol', 'file', 'environment', 'text'

-- Object reference (typed, nullable for some fact kinds)
object_ref: TEXT                -- the reference value (nullable)
object_ref_kind: TEXT           -- 'module', 'symbol', 'file', 'environment', 'text' (nullable)

-- Provenance (minimal, not full doc text)
source_file: TEXT NOT NULL      -- relative path to doc/config file
source_line_start: INTEGER
source_line_end: INTEGER
source_text_excerpt: TEXT       -- short excerpt of matched text (not full doc)
content_hash: TEXT NOT NULL     -- hash of source file at extraction time

-- Quality
extraction_method: TEXT NOT NULL -- 'explicit_marker', 'keyword_pattern', 'config_parse'
confidence: REAL NOT NULL        -- 0.0-1.0
generated: INTEGER NOT NULL      -- 1 if from generated doc (MAP.md), 0 if authored
doc_kind: TEXT NOT NULL          -- 'readme', 'architecture', 'config', 'inline_comment', 'map'

-- Timestamps
extracted_at: TEXT NOT NULL      -- ISO 8601
```

**Key design choices:**

1. **No snapshot_uid.** Docs are read live from disk, not from indexed snapshots.
   The command extracts current-state semantics from the working tree. Pretending
   snapshot-tight coupling exists when it does not would be dishonest.

2. **No full doc text storage.** Only extracted facts and minimal provenance are
   persisted. `content_hash` enables staleness detection without storing blobs.

3. **Repo-scoped identity.** `repo_uid` is the primary anchor. Facts represent
   current-state discovery for a repository, not historical snapshots.

### Update Semantics

Each `rmap docs` run **replaces all semantic_facts for that repo_uid**.

- Not append-forever
- Not versioned per run
- Not preserving old facts

This is current-state discovery, not history. The command answers: "what semantic
facts exist in the documentation right now?" Old facts are discarded on re-extraction.

### Reference Kinds

| Kind | Meaning | Resolution |
|------|---------|------------|
| `module` | Module path (e.g. `src/core`, `packages/api`) | Match against module_candidates.root_path |
| `symbol` | Symbol stable key or qualified name | Match against nodes.stable_key or qualified_name |
| `file` | File path relative to repo root | Match against file_versions.file_path |
| `environment` | Environment name (e.g. `production`, `dev`) | No graph resolution, used for filtering |
| `text` | Free-form text (e.g. constraint description) | No resolution, stored as-is |

The ref_kind fields enable typed resolution at query time without eager binding during extraction.

### Doc Kinds

| Kind | Source |
|------|--------|
| `readme` | README.md, README |
| `architecture` | ARCHITECTURE.md, CONTRIBUTING.md, design docs |
| `config` | docker-compose.yml, .env.*, deploy configs |
| `inline_comment` | JSDoc, Javadoc, rustdoc (future) |
| `map` | Generated MAP.md files (lower confidence) |

### Fact Kinds

| Kind | Subject | Object | Example |
|------|---------|--------|---------|
| `replacement_for` | new module/symbol | old module/symbol | "auth-v2 replaces auth" |
| `alternative_to` | module A | module B | "java-backend and ts-serverless are alternatives" |
| `deprecated_by` | deprecated entity | replacement entity | "OldClient deprecated in favor of NewClient" |
| `migration_path` | from entity | to entity | "migrate from REST to GraphQL" |
| `environment_surface` | service/surface | environment name | "api runs in production" |
| `operational_constraint` | symbol/module | constraint text | "must not be called in hot path" |

### Confidence Levels

| Level | Range | Meaning |
|-------|-------|---------|
| High | 0.9-1.0 | Explicit marker or structured config |
| Medium | 0.6-0.8 | Strong keyword pattern match |
| Low | 0.3-0.5 | Weak inference or heuristic |

## Extraction Methods

### 1. Explicit Markers (highest confidence)

Look for structured markers in docs:

```markdown
<!-- rg:replaces auth-v1 -->
<!-- rg:deprecated-by NewService -->
<!-- rg:alternative-to java-backend -->
```

Or YAML frontmatter:

```yaml
---
replaces: auth-v1
deprecated: true
deprecated_by: NewService
---
```

### 2. Keyword Pattern Matching (medium confidence)

Regex patterns for common phrasings:

- "replaces X" / "replacement for X" / "supersedes X"
- "deprecated in favor of X" / "use X instead"
- "alternative to X" / "X or Y can be used"
- "only runs in production/staging/dev"
- "do not call from X" / "must not be used in Y"

### 3. Config Parsing (high confidence for env detection)

Parse structured configs:

- docker-compose.yml: service names, environment variables
- .env.production, .env.development: environment-specific config
- deploy/*.yaml: deployment targets

## Integration Points

### Storage

- New table: `semantic_facts` (migration 020, after quality_assessments)
- Indexes on (repo_uid, fact_kind), (repo_uid, subject_ref, subject_ref_kind)
- No snapshot_uid — this is repo-scoped current-state extraction
- Each run deletes existing facts for repo_uid before inserting new ones

### CLI

- `rmap docs <db> <repo> [path]` - extract and persist semantic facts
- Separate command, NOT part of `rmap index`
- Reads docs/config live from disk
- Output: count of facts extracted by kind, grouped by doc_kind

### Extraction flow

```
rmap docs <db> <repo>
  1. Delete existing semantic_facts for repo_uid
  2. Find doc files (README, ARCHITECTURE, docker-compose, .env.*, MAP.md)
  3. Read each file from disk (live from working tree)
  4. Compute content_hash per file
  5. Run extractors (explicit markers, keyword patterns, config parsers)
  6. Insert new facts with minimal provenance
  7. Report summary (facts by kind, by doc_kind)
```

This is idempotent: running twice produces the same result (assuming docs unchanged).

### Query integration (future)

- `rmap orient` could surface relevant semantic facts for focused module
- `rmap check` could warn about deprecated code usage
- `rmap explain` could include semantic context

## Ingestion of Generated Docs

When repo-graph detects a MAP.md file:

1. Parse frontmatter to detect `generated_by: rgistr`
2. Set `generated: true` on any facts extracted
3. Use lower base confidence for generated docs

This keeps authored documentation distinguished from synthesized documentation.

## Implementation Order

1. **Migration 020**: `semantic_facts` table with ref_kind columns
2. **Doc detector**: find README, ARCHITECTURE, etc.
3. **Explicit marker extractor**: parse `<!-- rg:* -->` and frontmatter
4. **Keyword pattern extractor**: regex-based relationship detection
5. **Config parser**: docker-compose, .env environment detection
6. **CLI command**: `rmap docs`
7. **Generated doc detection**: MAP.md frontmatter handling (additive, lower confidence)

## Non-Goals

- This layer does not replace or modify the core graph
- This layer does not affect structural queries (callers, callees, etc.)
- This layer is advisory - agents decide how to weight the evidence
- This layer does not attempt NLP beyond pattern matching
- This layer does not store full documentation text (v1)
- This layer does not support historical replay without filesystem (v1)
- This layer does not auto-run during indexing (v1)

## Decisions Made

1. **Repo-scoped, not snapshot-scoped**
   - No snapshot_uid in the data model
   - repo_uid is the sole identity anchor
   - Facts represent current-state discovery from working tree
   - No false impression of snapshot-tight coupling

2. **Replace semantics on each run**
   - `rmap docs` deletes existing facts for repo_uid before inserting
   - Not append-forever, not versioned per run
   - Current-state discovery, not history

3. **Lazy resolution with typed references**
   - Store subject_ref/object_ref as typed text (with ref_kind)
   - Resolve to graph entities at query time
   - No eager binding during extraction

4. **Separate command, not auto-run**
   - `rmap docs` is a distinct command
   - Not part of `rmap index`
   - Keeps extraction cost and failure modes isolated

5. **Live read, minimal persistence**
   - Read docs from disk at extraction time
   - Do NOT store full doc text
   - Persist only facts + provenance (excerpt, hash, line range)
   - Optimized for discovery, not archival
