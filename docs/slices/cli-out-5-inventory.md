# CLI-OUT-5: Inventory/Policy Output

**Status:** COMPLETE (2026-05-20)  
**Type:** Product Surface / Implementation  
**Prerequisite:** CLI-OUT-4

## Problem Statement

Inventory and policy commands currently dump raw JSON. Users need scannable
human output for documentation discovery, resource inventory, and policy
introspection queries.

## Scope

All read-side inventory and policy commands. Write commands excluded.

### In Scope (6 commands)

**Documentation inventory:**
- `rmap docs list`
- `rmap docs extract`

**Resource inventory:**
- `rmap resource list`
- `rmap resource readers <resource>`
- `rmap resource writers <resource>`

**Policy introspection:**
- `rmap policy`

### Excluded

- Any write commands
- Any commands not listed above

## Implementation Groups

Commands grouped by output role and change axis. Implement in order.

### Group 1: Documentation Inventory

**Commands:** `docs list`, `docs extract`

**Why first:** Smallest coherent family. Same actor question: "what documentation
exists and what was extracted from it?" Establishes inventory-style rendering.

**Response shapes observed:**

`docs list`:
```json
{
  "command": "docs list",
  "repo": "repo_...",
  "repo_path": "...",
  "entries": [
    { "path": "README.md", "kind": "readme", "generated": false, "content_hash": "..." }
  ],
  "count": 1,
  "counts_by_kind": { "readme": 1 },
  "generated_count": 0
}
```

`docs extract`:
```json
{
  "command": "docs extract",
  "repo": "repo_...",
  "repo_path": "...",
  "files_scanned": 1,
  "files_by_kind": { "readme": 1 },
  "facts_extracted": 0,
  "facts_inserted": 0,
  "facts_deleted": 0,
  "counts_by_kind": {},
  "generated_docs_count": 0,
  "warnings": []
}
```

**Presentation module:** `presentation/docs.rs` (single file, two functions)

Both commands share documentation vocabulary but have different payloads:
- `list` is inventory (row output)
- `extract` is operation summary (counts + warnings)

Different enough to warrant separate render functions, similar enough to share a file.

### Group 2: Resource Inventory

**Commands:** `resource list`, `resource readers`, `resource writers`

**Why second:** Same resource-centered vocabulary. Readers/writers share response shape.

**Response shapes observed:**

`resource list`:
```json
{
  "command": "resource list",
  "repo": "repo_...",
  "snapshot": "...",
  "results": [
    {
      "stable_key": "...:fs:opllog.opl:FS_PATH",
      "name": "opllog.opl",
      "kind": "FS_PATH",
      "subtype": "FILE_PATH",
      "readers": 0,
      "writers": 1
    }
  ],
  "count": 1,
  "total_reads": 0,
  "total_writes": 1
}
```

`resource readers` / `resource writers`:
```json
{
  "command": "resource readers|writers",
  "repo": "repo_...",
  "snapshot": "...",
  "target": "...",
  "results": [
    {
      "stable_key": "...",
      "name": "OPLCreate",
      "qualified_name": "OPLCreate",
      "kind": "SYMBOL",
      "subtype": "FUNCTION",
      "file": "src/Engine/Adlib/fmopl.cpp",
      "line": 1213,
      "column": 0,
      "edge_type": "WRITES",
      "resolution": "static"
    }
  ],
  "count": 1
}
```

**Presentation module:** `presentation/resources.rs`

- `list` renders resource catalog with reader/writer counts
- `readers`/`writers` share identical response shape, single renderer with direction parameter

### Group 3: Policy Introspection

**Commands:** `policy`

**Why last:** Different semantic class. Not inventory - closer to introspection/definition.
Already has internal DTOs (StatusMappingOutput, BehavioralMarkerOutput, ReturnFateOutput).

**Response shapes observed:**

STATUS_MAPPING / BEHAVIORAL_MARKER:
```json
{
  "repo": "repo_...",
  "snapshot": "...",
  "kind": "STATUS_MAPPING",
  "facts": [...],
  "count": 0
}
```

RETURN_FATE:
```json
{
  "repo": "repo_...",
  "snapshot": "...",
  "kind": "RETURN_FATE",
  "facts": [
    {
      "callee_name": "Binary",
      "caller_key": "...",
      "caller_name": "YAML",
      "file_path": "...",
      "line": 24,
      "column": 13,
      "fate": "IGNORED",
      "evidence": { "type": "ignored", "explicit_void_cast": false }
    }
  ],
  "count": N,
  "summary": { "by_fate": { "IGNORED": N, "STORED": N, ... } }
}
```

**Presentation module:** `presentation/policy.rs`

Three policy kinds need separate render logic:
- STATUS_MAPPING: function name -> status code mappings
- BEHAVIORAL_MARKER: function -> behavioral pattern markers
- RETURN_FATE: call site -> fate classification (most complex)

**Contract Exception:** `policy` command does NOT use REG-1 contract. It requires
explicit `db_path` and `repo_uid` arguments. This makes it a legacy/outlier surface:

- Groups 1-2 (`docs *`, `resource *`) follow modern human-default/`--json` daemon pattern
- Group 3 (`policy`) is handled last, explicitly as an exception
- No migration to daemon contract planned for this slice

This asymmetry must remain visible in review packets.

## Structural Assessment

**Command file sizes (updated after implementation):**
- `commands/docs.rs` — 241 lines
- `commands/resource.rs` — 298 lines
- `commands/policy.rs` — 264 lines

**Presentation module sizes:**
- `presentation/docs.rs` — 412 lines (under guardrail)
- `presentation/resources.rs` — 512 lines (exceeds 500-line trigger)

**500-line guardrail note for resources.rs:**

`presentation/resources.rs` exceeds the 500-line guardrail trigger (512 lines total,
226 lines code + 286 lines tests). Kept as single file because:

- `resource list` and `resource readers/writers` are one coherent resource-inventory family
- Same actor: "what resources exist and who accesses them?"
- Same vocabulary: resource, readers, writers, stable_key
- Same object viewed at two granularities (catalog vs accessor detail)
- `readers` and `writers` share identical response shape, use single parameterized renderer

Split not required unless change axes diverge (e.g., if list gains aggregation features
that don't apply to readers/writers).

## Proposed Human Output Formats

### docs list

```
Documentation

1 document

By kind:
  readme  1

  README.md  readme

hint: run 'rmap docs extract' to extract semantic facts from documentation.
```

### docs extract (operation summary)

```
Documentation Extraction

1 file scanned

By kind:
  readme  1

Extraction results:
  0 facts extracted
  0 facts inserted
  0 facts deleted
  0 generated docs

No warnings.
```

### resource list

```
Resources

1 resource

Totals:
  0 reads
  1 writes

By kind:
  FS_PATH  1

  opllog.opl  FS_PATH  0 readers  1 writers

hint: use 'rmap resource readers <key>' or 'rmap resource writers <key>' for details.
```

### resource readers / writers

```
Writers for: opllog.opl

1 writer

  OPLCreate  src/Engine/Adlib/fmopl.cpp:1213  FUNCTION  static
```

### policy (RETURN_FATE example)

```
Policy Facts: RETURN_FATE

6 facts

By fate:
  IGNORED     2
  PROPAGATED  3
  STORED      1

  deps/include/yaml-cpp/binary.h:24   Binary -> YAML   IGNORED
  deps/include/yaml-cpp/binary.h:26   Binary -> YAML   STORED
  deps/include/yaml-cpp/binary.h:31   owned  -> YAML   PROPAGATED
  ...
```

## Output Contract (preserved from CLI-OUT-4)

1. **No clipping** — Full output, caller can pipe to `head`
2. **No arbitrary top-N** — Don't sample or truncate
3. **Deterministic ordering** — Sort by primary key, alphabetical tie-breakers
4. **`--json` preserved** — Machine mode outputs raw daemon response
5. **Hints guide action** — When results are empty or unexpected, suggest next steps

## Definition of Done

### Group 1: Documentation Inventory — COMPLETE (2026-05-20)

**Files:**
- [x] `presentation/docs.rs` — list + extract DTOs + renderers (412 lines)
- [x] `commands/docs.rs` — `--json` flag + human mode (241 lines)

**Functionality:**
- [x] `docs list` human renderer + `--json` flag
- [x] `docs extract` human renderer + `--json` flag
- [x] Deterministic ordering (by path for list)
- [x] Full output, no truncation

**Proof surfaces:**
- [x] Unit tests: 17 (9 list + 8 extract) in presentation/docs.rs
- [x] CLI integration tests: 10 in cli_out_5_inventory.rs (opt-in)
  - 8 empty/zero-fact path tests
  - 2 positive-path extraction tests (marker-based)

**Corpus validation:**
- OpenXcom (1 doc), django (4 docs) — zero-fact path
- Fixture with `<!-- rg:replaces -->` marker — positive extraction path

### Group 2: Resource Inventory — COMPLETE (2026-05-20)

**Files:**
- [x] `presentation/resources.rs` — list + readers/writers DTOs + renderers (512 lines incl tests, 226 lines code)
- [x] `commands/resource.rs` — `--json` flag + human mode (298 lines)

**Functionality:**
- [x] `resource list` human renderer + `--json` flag (sort by kind, name)
- [x] `resource readers` human renderer + `--json` flag (sort by file, line)
- [x] `resource writers` human renderer + `--json` flag (shared renderer with direction param)
- [x] Full output, no truncation

**Proof surfaces:**
- [x] Unit tests: 18 (10 list + 8 readers/writers) in presentation/resources.rs
- [x] Daemon dispatch tests: 4 (pre-existing in daemon_dispatch.rs)
- [x] CLI integration tests: 5 in cli_out_5_inventory.rs (opt-in)

**Corpus validation:**
- OpenXcom (1 resource, 1 writer) — positive path for list + writers
- django (0 resources) — zero-resource path
- OpenXcom (0 readers for opllog.opl) — empty readers path

**Evidence note:** `resource readers` positive path (nonzero readers) not corpus-validated.
Current corpus lacks a resource with detected readers. Renderer logic for populated
readers list is unit-test validated only (fixture data). Same evidence class as
CLI-OUT-4 Groups 4-5 populated cases.

### Group 3: Policy Introspection — COMPLETE (2026-05-20)

**Legacy contract exception:** `policy` does NOT use REG-1 daemon contract.
Requires explicit `db_path` and `repo_uid` arguments. This is preserved, not migrated.

**Files:**
- [x] `presentation/policy.rs` — DTOs + renderers for all three kinds (599 lines incl tests, 303 code)
- [x] `commands/policy.rs` — `--json` flag + human mode (403 lines)

**Functionality:**
- [x] `policy --kind STATUS_MAPPING` human renderer + `--json` flag
- [x] `policy --kind BEHAVIORAL_MARKER` human renderer + `--json` flag
- [x] `policy --kind RETURN_FATE` human renderer + `--json` flag
- [x] Deterministic ordering (by file, line for all)
- [x] Full output, no truncation
- [x] Kind-specific rendering (not generic row model)

**Proof surfaces:**
- [x] Unit tests: 12 (4 STATUS_MAPPING + 3 BEHAVIORAL_MARKER + 5 RETURN_FATE)
- [x] CLI integration tests: 3 (usage error, invalid db, unknown kind)

**Corpus validation:**
- RETURN_FATE: OpenXcom (1233 facts) — positive path corpus-validated
- STATUS_MAPPING: OpenXcom (0 facts) — empty path corpus-validated, populated fixture-validated
- BEHAVIORAL_MARKER: OpenXcom (0 facts) — empty path corpus-validated, populated fixture-validated

**Evidence note:** STATUS_MAPPING and BEHAVIORAL_MARKER positive paths (nonzero facts) not
corpus-validated. Current corpus lacks C codebases with status translation functions or
retry/resume patterns. Renderer logic for populated output is unit-test validated only.

### Validation
- [x] Group 1 corpus validation: positive and empty paths
- [x] Group 2 corpus validation: positive list/writers, empty readers
- [x] Group 3 corpus validation: positive RETURN_FATE, empty STATUS_MAPPING/BEHAVIORAL_MARKER

## Files in Scope

### Presentation (new files)

**Group 1:**
- `presentation/docs.rs` — list + extract renderers

**Group 2:**
- `presentation/resources.rs` — list + readers/writers renderers

**Group 3:**
- `presentation/policy.rs` — STATUS_MAPPING + BEHAVIORAL_MARKER + RETURN_FATE renderers

### Commands (updates)

- `commands/docs.rs` (179 lines) — add --json + human mode
- `commands/resource.rs` (249 lines) — add --json + human mode
- `commands/policy.rs` (264 lines) — add --json + human mode

## Explicit Non-Goals

- Do not change daemon response structure
- Do not add new query capabilities
- Do not migrate `policy` to daemon contract (preserves explicit db_path/repo_uid)
- Do not add colors/styling (future slice)
