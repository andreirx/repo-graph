# PF-1: STATUS_MAPPING Extraction

Status: COMPLETE
Scope: STATUS_MAPPING only. No other policy-fact families.

## Goal

Extract status/error translation functions from C code and persist them
as queryable policy facts. Prove the policy-facts architecture on a
single, high-value extraction family before expanding.

## Non-Goals

- BRANCH_OUTCOME, RETURN_FATE, BEHAVIORAL_MARKER, DEFAULT_PROVENANCE (PF-2+)
- Languages other than C (future slices)
- Cross-file policy flow tracing
- Integration with orient/check/explain
- TypeScript `rgr` parity

## Support Module

Location: `rust/crates/policy-facts/`

### Crate structure

```
rust/crates/policy-facts/
  Cargo.toml
  src/
    lib.rs
    types.rs              # StatusMapping DTO only
    extractors/
      mod.rs
      status_mapping.rs   # C extractor
    storage_port.rs       # PolicyFactsStorageRead trait
```

### StatusMapping DTO

```rust
/// A function that translates one status/error code type to another.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusMapping {
    /// Stable key of the function performing the translation.
    pub symbol_key: String,
    
    /// Function name (for display).
    pub function_name: String,
    
    /// Source file path (repo-relative).
    pub file_path: String,
    
    /// Line number where the function starts.
    pub line_start: u32,
    
    /// Line number where the function ends.
    pub line_end: u32,
    
    /// Name of the input type (e.g., "channel_op_res_t").
    pub source_type: String,
    
    /// Name of the output type (e.g., "server_op_res_t").
    pub target_type: String,
    
    /// Individual case mappings.
    pub mappings: Vec<CaseMapping>,
    
    /// Default/fallback output when no case matches.
    /// None if no default arm or if default is unreachable.
    pub default_output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseMapping {
    /// Input enum variant(s) for this case arm.
    /// Multiple values when case arms fall through.
    pub inputs: Vec<String>,
    
    /// Output enum variant returned by this arm.
    pub output: String,
}
```

### Extractor interface

```rust
/// Extract STATUS_MAPPING facts from a C file's AST.
pub fn extract_status_mappings(
    tree: &tree_sitter::Tree,
    source: &[u8],
    file_path: &str,
    repo_uid: &str,
) -> Vec<StatusMapping>;
```

### Extractor rules (C only)

A function is a STATUS_MAPPING candidate if:

1. **Return type is status/result-like:** Name ends with `_t`, `_e`, `_res`,
   `_result`, `_status`, `_code`, `_error`, or `_ret`. Or name contains
   `Result`, `Error`, `Status`, `Code`.

2. **Body contains a switch statement** on any qualifying parameter.

3. **Switch discriminant is any parameter** (directly, via pointer dereference
   like `*http_response_code`, or via simple cast). Not limited to first parameter.

4. **Source discriminant type:** May be either:
   - Enum-like identifier type (same naming patterns as target)
   - Numeric scalar type used as a code domain (`int`, `long`, `unsigned`, etc.)
   
   This allows numeric-code-to-status mappings like `channel_map_http_code(long)`.

5. **At least one explicit case-to-return mapping:** The switch must have at least
   one case arm that directly returns a value. A fallback return after the switch
   alone is not sufficient (this rejects command dispatchers with side effects).

6. **Case arm outputs:** Each case arm's return value is captured as normalized
   source text (identifier or expression). No semantic reduction is performed.

7. **Preprocessor handling:** Case labels inside `#if`/`#ifdef`/`#ifndef` blocks
   within the switch body are extracted correctly. Preprocessor conditionals do
   not break case-group boundaries.

8. **Naming heuristic (optional boost):** Function name contains `map`,
   `convert`, `translate`, `to_`, or `_to_`.

Extraction procedure:

1. Find all `function_definition` nodes (including inside preprocessor blocks).
2. Check return type against status/result-like pattern.
3. Find switch statement in function body.
4. Verify switch discriminant is any qualifying parameter.
5. Walk switch body including preprocessor-wrapped nodes. For each case arm:
   - Collect case labels (handle fallthrough: consecutive labels before a return)
   - Extract return expression as source text
6. Extract default arm if present (inside switch or as fallback after switch).
7. Reject if no explicit case-to-return mappings found.
8. Infer source_type from matched parameter type.
9. Infer target_type from return type.

### Storage port

```rust
pub trait PolicyFactsStorageWrite {
    fn insert_status_mappings(
        &self,
        snapshot_uid: &str,
        mappings: &[StatusMapping],
    ) -> Result<usize, PolicyFactsStorageError>;
}

pub trait PolicyFactsStorageRead {
    fn query_status_mappings(
        &self,
        snapshot_uid: &str,
        file_filter: Option<&str>,
    ) -> Result<Vec<StatusMapping>, PolicyFactsStorageError>;
}
```

## Storage Schema

Migration 021 adds one table. Minimal schema for STATUS_MAPPING only.

```sql
-- Migration: 021-status-mappings

CREATE TABLE status_mappings (
    uid TEXT PRIMARY KEY,
    snapshot_uid TEXT NOT NULL REFERENCES snapshots(uid) ON DELETE CASCADE,
    symbol_key TEXT NOT NULL,
    function_name TEXT NOT NULL,
    file_path TEXT NOT NULL,
    line_start INTEGER NOT NULL,
    line_end INTEGER NOT NULL,
    source_type TEXT NOT NULL,
    target_type TEXT NOT NULL,
    mappings_json TEXT NOT NULL,  -- JSON array of CaseMapping
    default_output TEXT,          -- NULL if no default
    
    UNIQUE(snapshot_uid, symbol_key)
);

CREATE INDEX status_mappings_by_snapshot ON status_mappings(snapshot_uid);
CREATE INDEX status_mappings_by_file ON status_mappings(file_path);
```

Note: Dedicated table, not generic `policy_facts` table. If PF-2+ needs
a different shape, we add another table. Premature generalization avoided.

## CLI Contract

### Command

```
rmap policy <db_path> <repo_uid> [--kind STATUS_MAPPING] [--file <path>]
```

For PF-1, `--kind STATUS_MAPPING` is the only valid kind and is the default.
The flag exists to establish the surface for future kinds.

### Output (JSON)

```json
{
  "repo": "swupdate",
  "snapshot": "swupdate/2026-04-29T09:13:36.741Z/4c647d8c",
  "kind": "STATUS_MAPPING",
  "facts": [
    {
      "symbol_key": "swupdate:corelib/server_utils.c#map_channel_retcode:SYMBOL:FUNCTION",
      "function_name": "map_channel_retcode",
      "file_path": "corelib/server_utils.c",
      "line_start": 72,
      "line_end": 98,
      "source_type": "channel_op_res_t",
      "target_type": "server_op_res_t",
      "mappings": [
        {
          "inputs": ["CHANNEL_ENONET", "CHANNEL_EAGAIN", "CHANNEL_ESSLCERT", "CHANNEL_ESSLCONNECT", "CHANNEL_REQUEST_PENDING"],
          "output": "SERVER_EAGAIN"
        },
        {
          "inputs": ["CHANNEL_EACCES"],
          "output": "SERVER_EACCES"
        },
        {
          "inputs": ["CHANNEL_OK", "CHANNEL_EREDIRECT"],
          "output": "SERVER_OK"
        }
      ],
      "default_output": "SERVER_EERR"
    }
  ],
  "count": 1
}
```

Note: Full swupdate output contains 5 facts. Above shows one representative fact.
Actual functions extracted: `map_channel_retcode`, `channel_map_curl_error`,
`channel_map_http_code`, `map_http_retcode`, `handle_feedback`.

### Exit codes

- 0: success
- 1: no facts found (not an error, just empty)
- 2: invalid arguments or DB error

## Validation Corpus

### swupdate targets

Five STATUS_MAPPING functions detected in swupdate:

| File | Function | Source Type | Target Type | Notes |
|------|----------|-------------|-------------|-------|
| corelib/server_utils.c | map_channel_retcode | channel_op_res_t | server_op_res_t | cross-domain translation |
| corelib/channel_curl.c | channel_map_curl_error | CURLcode | channel_op_res_t | cross-domain, has #ifdef cases |
| corelib/channel_curl.c | channel_map_http_code | long | channel_op_res_t | numeric-to-enum via pointer deref |
| suricatta/server_general.c | map_http_retcode | channel_op_res_t | server_op_res_t | cross-domain translation |
| suricatta/server_hawkbit.c | handle_feedback | server_op_res_t | server_op_res_t | same-domain normalization |

**Note on same-domain handlers:** `handle_feedback` maps server_op_res_t to
server_op_res_t (same domain). It is included because it has explicit case-to-return
mappings and matches all extraction rules. Same-domain status normalizers are in
scope for PF-1. Future slices may refine whether to filter these at the policy layer.

### Required assertions

1. **map_channel_retcode detected:**
   - source_type = "channel_op_res_t"
   - target_type = "server_op_res_t"
   - mappings includes: `["CHANNEL_ENONET", ...] -> "SERVER_EAGAIN"`
   - default_output = "SERVER_EERR"

2. **channel_map_http_code detected:**
   - source_type = "long" (from `*http_response_code` pointer dereference)
   - target_type = "channel_op_res_t"
   - Maps HTTP codes (200, 401, 404, etc.) to channel results

3. **channel_map_curl_error detected:**
   - source_type = "CURLcode"
   - target_type = "channel_op_res_t"
   - **Preprocessor handling verified:**
     - `CURLE_SSL_ENGINE_INITFAILED` → `CHANNEL_EINIT` (not merged with ENONET)
     - `CURLE_COULDNT_RESOLVE_HOST` → `CHANNEL_ENONET` (correct group)
     - `CURLE_PEER_FAILED_VERIFICATION` → `CHANNEL_ESSLCERT` (not merged with ESSLCONNECT)
     - default_output = "CHANNEL_EINIT"

4. **Precision check:**
   - No false positives on swupdate/corelib (command dispatchers like
     `channel_post_method` are not detected)

5. **Fallthrough handling:**
   - map_channel_retcode case arms with fallthrough (CHANNEL_ENONET,
     CHANNEL_EAGAIN, etc. all mapping to SERVER_EAGAIN) correctly
     captured as single CaseMapping with multiple inputs.

### Test structure

```
rust/crates/policy-facts/tests/
  swupdate_validation.rs   # Integration test on swupdate corpus with semantic checks
```

Unit tests are in `rust/crates/policy-facts/src/extractors/status_mapping.rs`.

## Implementation Sequence

1. **Crate scaffold:** Create `rust/crates/policy-facts/` with types only.

2. **Extractor unit tests:** Write tests for synthetic C fragments first.
   Pin expected behavior before implementation.

3. **Extractor implementation:** `extract_status_mappings` function.

4. **Storage migration:** Add `status_mappings` table.

5. **Storage adapter:** Implement `PolicyFactsStorageWrite` and
   `PolicyFactsStorageRead` on `StorageConnection`.

6. **Indexer integration:** Call extractor during C file extraction,
   persist results.

7. **CLI command:** `rmap policy` with STATUS_MAPPING output.

8. **swupdate validation:** Run on swupdate, verify all five targets
   detected with correct mappings and semantic accuracy.

## Acceptance Criteria

- [x] `rmap policy swupdate.db swupdate` returns 5 STATUS_MAPPING facts
- [x] All required swupdate targets detected:
      - map_channel_retcode (enum-to-enum) - DETECTED
      - channel_map_curl_error (enum-to-enum with #ifdef cases) - DETECTED
      - channel_map_http_code (long -> channel_op_res_t via pointer deref) - DETECTED
      - map_http_retcode (enum-to-enum) - DETECTED
      - handle_feedback (same-domain normalization) - DETECTED
- [x] map_channel_retcode mappings match actual swupdate source:
      - CHANNEL_ENONET -> SERVER_EAGAIN (with fallthrough siblings)
      - default_output = SERVER_EERR
- [x] channel_map_curl_error preprocessor handling correct:
      - SSL init errors → CHANNEL_EINIT (not merged with ENONET)
      - Network errors → CHANNEL_ENONET (correct boundary)
      - CURLE_PEER_FAILED_VERIFICATION → CHANNEL_ESSLCERT
- [x] Fallthrough case arms correctly grouped (multiple inputs per CaseMapping)
- [x] No false positives on swupdate/corelib (channel_post_method, disk_ioctl excluded)
- [x] Unit tests cover: status-like type heuristics, switch detection,
      fallthrough handling, default arm extraction, expression-return preservation,
      preprocessor-guarded cases, command dispatcher rejection
- [x] Storage migration 021 and round-trip
- [x] Indexer integration — temporary postpass re-parse (C files only)
- [x] CLI command `rmap policy`

### Implementation Notes

**Scope extension during implementation:** The original rule "top-level switch
on first parameter" was extended to support:
- Switch on any qualifying parameter (not just first)
- Pointer dereference discriminants (`*http_response_code`)
- Preprocessor-wrapped case labels inside switch bodies

**False positive reduction:** Functions with zero explicit case-to-return
mappings are rejected. This excludes command dispatchers that use switches
for side effects with a trailing `return result;`.

**Temporary indexer integration:** PF-1 uses a postpass re-parse approach
in `repo-graph-repo-index::compose.rs`. This duplicates tree-sitter parsing
already done by the C extractor. The target architecture is extraction-time
integration; see `docs/TECH-DEBT.md` entry "PF-1 temporary re-parse postpass".

**Language scope:** PF-1 extracts from C files only (.c, .h). C++ files are
explicitly excluded to avoid false positives from different language semantics.

## What PF-1 Does NOT Include

- Generic `policy_facts` table (we use dedicated `status_mappings`)
- Other policy fact kinds (PF-2: BEHAVIORAL_MARKER, PF-3: RETURN_FATE)
- C++ extensions
- Type-resolved source/target types (we use AST text only)
- Nested switch extraction (top-level only, per locked scope decisions)
- Semantic reduction of return expressions (stored as source text)
- Cross-layer edge materialization (PF-5)
- Integration with orient/check/explain

## Locked Scope Decisions

These are not open questions. They are binding decisions for PF-1.

### Numeric discriminants are included

PF-1 covers mappings from numeric code domains to status/result types.
`channel_map_http_code(long -> channel_op_res_t)` is in scope.

Rationale: This function is part of the real swupdate translation ladder.
Excluding it would weaken the proving-ground result and miss an important
real-world pattern.

### Nested switches are excluded

PF-1 only extracts top-level switch mappings. Nested-switch cases are out of scope.

Rationale: Keeps the extractor deterministic and local. Avoids premature
control-flow complexity. Nested structures can be a later expansion if a
calibration repo proves need.

### Same-domain handlers are included

Functions that map a status type to itself (e.g., `server_op_res_t -> server_op_res_t`)
are included if they have explicit case-to-return mappings.

Rationale: These are still status translation seams, even if not crossing type
boundaries. They normalize error propagation and are part of the policy surface.
Future slices may refine whether to filter these at consumption time.

### Expression returns are preserved as text

Case-arm outputs may be identifiers or arbitrary return expressions.
PF-1 stores the extracted expression text without semantic reduction.

Example: `return (bar_t)(BAZ | QUX);` is stored as `"(bar_t)(BAZ | QUX)"`.

Rationale: Preserves evidence, avoids dropping real mappings, keeps
extractor simple. Consumer can decide later whether expression is reducible.
