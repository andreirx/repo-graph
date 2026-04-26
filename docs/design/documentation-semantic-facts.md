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

## Scope

### In scope (v1)

1. **Documentation detection** - find and parse doc files
   - README.md, ARCHITECTURE.md, CONTRIBUTING.md
   - Module-level docs (package README, Cargo.toml description)
   - Inline doc comments (optional, lower priority)

2. **Relationship extraction** - explicit statements
   - `replacement_for`: "X replaces Y"
   - `alternative_to`: "X and Y are alternatives"
   - `deprecated_by`: "X is deprecated in favor of Y"
   - `migration_path`: "migrate from X to Y"

3. **Environment detection** - config/deployment context
   - Environment surfaces from docker-compose, .env files, deploy configs
   - Per-environment feature flags or configuration differences

4. **Provenance tracking**
   - Source file, line range
   - Extraction method (pattern match, keyword, explicit marker)
   - Confidence level
   - Extraction date

### Out of scope (v1)

- Javadoc/Doxygen XML ingestion (future enhancement)
- Natural language understanding (beyond pattern matching)
- Cross-repo relationship detection
- Automated staleness detection against code changes

## Data Model

### Semantic Fact

```
semantic_facts table
--------------------
fact_uid: TEXT PRIMARY KEY
snapshot_uid: TEXT FK
repo_uid: TEXT FK

fact_kind: TEXT  -- 'replacement_for', 'alternative_to', 'deprecated_by', 
                 -- 'migration_path', 'environment_surface', 'operational_constraint'

-- Subject reference (typed for query-time resolution)
subject_ref: TEXT           -- the reference value (path, key, name, or text)
subject_ref_kind: TEXT      -- 'module', 'symbol', 'file', 'environment', 'text'

-- Object reference (typed, nullable for some fact kinds)
object_ref: TEXT            -- the reference value (nullable)
object_ref_kind: TEXT       -- 'module', 'symbol', 'file', 'environment', 'text' (nullable)

-- Evidence
source_file: TEXT      -- relative path to doc file
source_line_start: INTEGER
source_line_end: INTEGER
source_text: TEXT      -- the actual text that was matched

-- Quality
extraction_method: TEXT  -- 'explicit_marker', 'keyword_pattern', 'config_parse'
confidence: REAL         -- 0.0-1.0
generated: BOOLEAN       -- true if from generated doc (MAP.md), false if authored

-- Timestamps
extracted_at: TEXT       -- ISO 8601
```

### Reference Kinds

| Kind | Meaning | Resolution |
|------|---------|------------|
| `module` | Module path (e.g. `src/core`, `packages/api`) | Match against module_candidates.root_path |
| `symbol` | Symbol stable key or qualified name | Match against nodes.stable_key or qualified_name |
| `file` | File path relative to repo root | Match against file_versions.file_path |
| `environment` | Environment name (e.g. `production`, `dev`) | No graph resolution, used for filtering |
| `text` | Free-form text (e.g. constraint description) | No resolution, stored as-is |

The ref_kind fields enable typed resolution at query time without eager binding during extraction.

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
- Indexes on (snapshot_uid, fact_kind), (repo_uid, subject_ref, subject_ref_kind)

### CLI

- `rmap docs <db> <repo> [path]` - extract and persist semantic facts
- Output: count of facts extracted by kind

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

## Open Questions

1. Should semantic facts be per-snapshot or per-repo?
   - Per-snapshot: facts can change with doc changes
   - Per-repo: facts are more stable, less churn
   - **Recommendation**: per-snapshot, like all other extracted facts

2. Should we resolve subject_ref/object_ref to actual graph entities?
   - Pro: enables graph traversal from semantic facts
   - Con: adds coupling, may not always resolve
   - **Recommendation**: store as text, resolve at query time (lazy)

3. Should `rmap docs` run automatically during `rmap index`?
   - Pro: always fresh
   - Con: adds indexing time, may not be wanted
   - **Recommendation**: separate command initially, optional auto-run later
