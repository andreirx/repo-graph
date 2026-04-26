# Documentation-Derived Semantic Facts

Design document for the **secondary** semantic hints layer in repo-graph.

## Architectural Position

**This layer is auxiliary, not primary.**

The primary documentation surface in repo-graph is the documentation
inventory: discovering doc files, classifying them, and surfacing
relevant file paths to agents. Agents then read the actual docs.

This semantic facts layer extracts **derived hints** from documentation
for ranking, filtering, and quick lookups. It is useful but lossy.
It does not replace the documentation itself.

**Correct mental model:**
- Primary: docs inventory → agent reads docs → full context preserved
- Secondary: semantic facts → hints for ranking/filtering → lossy summaries

**Anti-pattern this design avoids:**
Treating extracted semantic facts as THE documentation layer. That
would lose nuance, qualifiers, rationale, and exceptions that exist
in the actual documentation text.

## Problem Statement

Repo-graph extracts structural facts from code (symbols, edges, modules, measurements). But codebases also contain semantic intelligence in documentation that affects agent reasoning:

- Migration/rewrite/replacement relationships
- Alternative implementation relationships
- Environment surfaces (dev/stage/prod)
- Deprecation notices and migration paths
- Operational constraints not visible in code structure

This intelligence is invisible to the current graph. An agent cannot know from structure alone that "module A is being replaced by module B" or "service X runs only in production".

**However:** the solution is NOT to reduce docs into a narrow ontology and pretend that is the documentation layer. The solution is:

1. **Primary:** Surface relevant doc file paths; let agents read them
2. **Secondary:** Extract semantic hints for ranking and filtering

## Design Principle

**This is a secondary hints layer, not the documentation product.**

- It does not produce nodes/edges in the core graph
- It does not affect call/import resolution
- It produces queryable hints with explicit provenance
- Hints are advisory, not authoritative
- Confidence levels distinguish explicit statements from inferences
- **The actual docs remain the canonical source of documentation truth**

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

Primary documentation surface (docs inventory):
- `rmap docs list` surfaces doc file paths, kinds, generated/authored flags
- `rmap orient` includes `documentation.relevant_files[]` as primary section
- `rmap explain` includes doc file references and excerpts

Secondary hints (semantic facts):
- `rmap orient` may include `documentation_hints.semantic_facts[]` as optional
- `rmap check` could warn about deprecated code usage via hints
- Semantic facts are ranking/filtering aids, not the main documentation output

**The agent reads the actual docs.** Semantic facts help locate and rank them.

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

- **This layer does not replace documentation inventory as the primary surface**
- This layer does not replace or modify the core graph
- This layer does not affect structural queries (callers, callees, etc.)
- This layer is advisory - agents decide how to weight the evidence
- This layer does not attempt NLP beyond pattern matching
- This layer does not store full documentation text (v1)
- This layer does not support historical replay without filesystem (v1)
- This layer does not auto-run during indexing (v1)
- **Semantic facts are NOT the canonical documentation representation**
- **Agents should read actual docs, not rely solely on extracted facts**

## Two Distinct Surfaces (architectural lock)

This section prevents future implementers from conflating the two
documentation surfaces. They have different purposes and different
rules. Do not "improve" one using the rules of the other.

### Documentation Inventory Surface

**Purpose:** Discovery and surfacing of doc files for agent reading.

**Characteristics:**
- Primary surface — docs are first-class data
- Lightweight classification only (path/name patterns)
- `generated` flag is an **advisory workflow hint**, not semantic truth
- Path-based convention is sufficient (e.g., `MAP.md` => generated)
- No content adjudication required for inventory
- Docs are meant to be read by the agent, not pre-filtered

**`generated` flag semantics:**
- Advisory provenance hint from workflow/path convention
- NOT a hard trust downgrade for ranking
- Mild tie-breaker only, not dominant ranking factor
- Agent determines document value by reading content

**Correct ranking rule:**
- Rank by structural relevance first (path match > root doc)
- Prefer authored vs generated only as a mild tie-breaker
- A generated `MAP.md` for `src/core/service` may be more useful
  than a repo-root authored README for that specific focus

### Semantic Fact Extraction Surface

**Purpose:** Secondary derived hints for ranking/filtering.

**Characteristics:**
- Secondary surface — hints are lossy summaries
- May inspect content (frontmatter, keyword patterns)
- May use content signals for confidence adjustment
- Still not canonical documentation truth
- Confidence affects hint trust, not document existence

**Content analysis here is valid** because semantic facts are derived
hints, not inventory classification. The distinction:
- Inventory: "these docs exist, here are workflow hints"
- Semantic facts: "these hints were extracted from doc content"

### Why This Lock Exists

Without this explicit split, implementers oscillate between:
- "improve inventory by adding content-authoritative detection"
- "simplify semantic facts to match inventory's lightweight model"

Both are wrong. The surfaces serve different purposes and should
remain distinct. Inventory is for discovery; semantic facts are for
derived hints. Do not merge their concerns.

## Implementation Constraints (binding)

These constraints prevent drift back to treating semantic facts as primary.

1. **`rmap docs list` must NOT derive from semantic_facts.**
   Documentation inventory is the source. Repos with docs but no
   extracted facts must still show their docs.

2. **Shared DTO:** `DocInventoryEntry` with path, kind, generated,
   optional content_hash. Used by docs list, orient, and future
   persisted inventory.

3. **Orient surfaces docs with relevance reasons:**
   ```json
   {"path": "...", "kind": "readme", "generated": false, "reason": "module_path_match"}
   ```

4. **Structural relevance first, provenance second.**
   Ranking: path match (200) >> root doc (100) >> config (80).
   Authored vs generated is a mild tie-breaker (-5), not a filter.
   A generated MAP.md for the focus path outranks a root README.

5. **Relevance rules are simple and structural in v1.**
   No text search, embeddings, or fuzzy matching.

6. **Pure relevance selector function.**
   `select_relevant_docs(inventory, focus) -> ranked_docs`
   Do not inline in CLI handlers.

7. **Real-repo validation required:** glamCRM, amodx, hexmanos.
   Key proof: repos with no semantic_facts still show their docs.

8. **CLI decision required before implementation:**
   - Option A: Keep `rmap docs` as extract, add `rmap docs list`
   - Option B: Rename to `rmap docs extract`, make `docs` a command family

See `docs/ROADMAP.md` §2 for full constraint list.

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
