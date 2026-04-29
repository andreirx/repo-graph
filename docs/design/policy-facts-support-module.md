# Policy-Facts Support Module Design

Status: DESIGN PHASE
Created: 2026-04-28

## Problem Statement

repo-graph surfaces structural relationships (calls, imports, implements)
but does not surface cross-layer policy propagation. When an agent asks
"how does a TFTP download failure propagate through swupdate?", the graph
shows function calls but not:

- Status code translation functions (channel_op_res_t to server_op_res_t)
- Retry/restart behavior (retry loops with backoff, resume-from offsets)
- Return-fate patterns (result ignored, propagated, transformed)
- Default value provenance (where config defaults originate)

This information exists in the code but requires reading function bodies
to understand, which defeats the purpose of a structural discovery tool.

### Evidence from swupdate exploration

Discovery workflow on swupdate/corelib revealed:

1. `map_channel_retcode()` in server_utils.c is a status translation function
   - Takes `channel_op_res_t`, returns `server_op_res_t`
   - This is the cross-layer policy seam between network and server logic

2. `channel_get_file()` in channel_curl.c contains retry behavior
   - Retry loop with `retry_sleep` delay
   - Resume-from offset via `CURLOPT_RESUME_FROM_LARGE`
   - This is behavioral policy, not just structure

3. Handler callbacks consume results differently
   - Some check return codes
   - Some ignore them (fire-and-forget)
   - This is return-fate, invisible in CALLS edges

The graph shows `map_channel_retcode` is called, but not that it is a
STATUS_MAPPING function or that its output determines server behavior.

### Gap vs. rgistr policy hints

rgistr MAP generation now includes Policy Signals (file-level) and Policy
Seams (folder-level) sections. These are advisory LLM-generated hints that
reveal the architectural need but do NOT provide:

- Deterministic extraction (same input produces same output)
- Source-anchor provenance (line numbers, AST node references)
- Queryability (not in SQLite, not structured for programmatic access)
- Cross-file tracing (policy flow across module boundaries)
- Contract-level reproducibility

Policy hints are discovery compression for agent orientation.
Policy facts are deterministic extraction for agent reasoning.

## Extraction Families

Five categories of policy-relevant facts to extract:

### 1. STATUS_MAPPING

Functions that translate one status/error code to another.

**Pattern signals:**
- Switch/match on enum input, returns enum output
- Input and output types are both status/error/result types
- Each case maps one code to another (possibly many-to-one)

**Extraction:**
- Source type (e.g., `channel_op_res_t`)
- Target type (e.g., `server_op_res_t`)
- Mapping table (input code -> output code)
- Unmapped code handling (default case, pass-through, panic)

**Example (C, from swupdate/corelib/server_utils.c):**
```c
server_op_res_t map_channel_retcode(channel_op_res_t response) {
    switch (response) {
    case CHANNEL_ENONET:
    case CHANNEL_EAGAIN:
    case CHANNEL_ESSLCERT:
    case CHANNEL_ESSLCONNECT:
    case CHANNEL_REQUEST_PENDING:
        return SERVER_EAGAIN;
    case CHANNEL_EACCES:
        return SERVER_EACCES;
    case CHANNEL_ENOENT:
    case CHANNEL_EIO:
    // ...
        return SERVER_EERR;
    case CHANNEL_EBADMSG:
    case CHANNEL_ENOTFOUND:
        return SERVER_EBADMSG;
    case CHANNEL_OK:
    case CHANNEL_EREDIRECT:
        return SERVER_OK;
    }
    return SERVER_EERR;  // default
}
```

Note the many-to-one mapping pattern: multiple network-layer codes
(ENONET, EAGAIN, ESSLCERT, etc.) collapse to a single server-layer
retry signal (SERVER_EAGAIN). This is the policy seam.

### 2. BRANCH_OUTCOME

Switch/match arms that produce specific results or actions.

**Pattern signals:**
- Switch/match on a discriminant
- Each arm produces a distinct outcome (return value, side effect, state change)
- Outcome is a policy decision, not just data transformation

**Extraction:**
- Discriminant expression
- Per-arm: case pattern, outcome kind, outcome value/action
- Default arm handling

This overlaps with STATUS_MAPPING but is broader: it captures any
policy-significant branching, not just status code translation.

### 3. RETURN_FATE

What happens to a function's return value at each call site.

**Pattern signals:**
- Call expression
- Is result assigned to a variable?
- Is result immediately checked (if/switch)?
- Is result passed to another function?
- Is result discarded (statement expression, no assignment)?

**Extraction:**
- Callee symbol
- Call site location
- Fate category: IGNORED, CHECKED, PROPAGATED, TRANSFORMED, STORED

**Example:**
```c
// IGNORED
process_image(img);

// CHECKED
if (process_image(img) != 0) { ... }

// PROPAGATED
return process_image(img);

// TRANSFORMED
result = process_image(img) == 0 ? SUCCESS : FAILURE;

// STORED
status = process_image(img);
```

### 4. BEHAVIORAL_MARKER

Control-flow patterns that indicate policy behavior.

**Retry loops:**
- Loop with condition based on result/status
- Sleep/delay inside loop body
- Break on success or max attempts

**Timeout handling:**
- Timer setup before operation
- Check for timeout condition after operation
- Timeout-specific error path

**Resume/checkpoint:**
- State saved before operation
- State restored on failure
- Resume from saved state

**Extraction:**
- Pattern type (RETRY_LOOP, TIMEOUT, RESUME, etc.)
- Associated function call(s)
- Policy parameters (retry count, sleep duration, timeout value)

### 5. DEFAULT_PROVENANCE

Where default/fallback values originate and how they propagate.

**Pattern signals:**
- Variable initialized with literal or config-read value
- Conditional assignment (if not set, use default)
- Config file parsing with fallback values

**Extraction:**
- Variable/field name
- Default value (literal or expression)
- Override source (config file key, environment variable, parameter)
- Propagation path (where is this default used?)

## Architecture

### Support module structure

```
rust/crates/policy-facts/
  src/
    lib.rs
    types.rs          # PolicyFact enum, extraction families
    extractors/
      mod.rs
      status_mapping.rs
      branch_outcome.rs
      return_fate.rs
      behavioral_marker.rs
      default_provenance.rs
    matchers/
      mod.rs
      c.rs            # C-specific pattern matchers
      cpp.rs          # C++ matchers (extends C)
      rust.rs         # Rust matchers (future)
      java.rs         # Java matchers (future)
      typescript.rs   # TS matchers (future)
```

### Dependency direction

```
policy-facts (use case) -> storage-port (interface)
storage (adapter) -> policy-facts (implements port)
rmap CLI -> policy-facts (calls use case)
```

This is a Rust-first capability. The `rmap` (Rust) binary is the
composition root. TypeScript `rgr` parity is not planned unless a
concrete consumer need emerges.

Policy-facts is a pure support module. It defines:
- `PolicyFact` enum with all extraction families
- Per-family extractor functions (AST -> Vec<PolicyFact>)
- Language-specific pattern matchers
- Storage port for persistence

It does NOT:
- Own storage schema
- Know about CLI rendering
- Depend on specific parser implementation (takes AST, not source text)

### Storage schema extension

New tables:

```sql
CREATE TABLE policy_facts (
  uid TEXT PRIMARY KEY,
  snapshot_uid TEXT NOT NULL REFERENCES snapshots(uid) ON DELETE CASCADE,
  source_file TEXT NOT NULL,      -- file stable key
  source_symbol TEXT,             -- symbol stable key (if inside a function)
  kind TEXT NOT NULL,             -- STATUS_MAPPING, BRANCH_OUTCOME, etc.
  subkind TEXT,                   -- RETRY_LOOP, TIMEOUT, etc. for BEHAVIORAL_MARKER
  line_start INTEGER NOT NULL,
  line_end INTEGER NOT NULL,
  evidence_json TEXT NOT NULL     -- per-kind structured evidence
);

CREATE INDEX policy_facts_by_snapshot ON policy_facts(snapshot_uid);
CREATE INDEX policy_facts_by_file ON policy_facts(source_file);
CREATE INDEX policy_facts_by_kind ON policy_facts(kind);
```

The `evidence_json` column contains kind-specific structured data:

**STATUS_MAPPING:**
```json
{
  "source_type": "channel_op_res_t",
  "target_type": "server_op_res_t",
  "mappings": [
    {"input": "CHANNEL_OK", "output": "SERVER_OK"},
    {"input": "CHANNEL_EINIT", "output": "SERVER_EERR"}
  ],
  "default_output": "SERVER_EERR"
}
```

**RETURN_FATE:**
```json
{
  "callee": "process_image",
  "callee_stable_key": "swupdate:core/stream_interface.c#process_image:SYMBOL:function",
  "fate": "IGNORED"
}
```

**BEHAVIORAL_MARKER:**
```json
{
  "pattern": "RETRY_LOOP",
  "loop_kind": "while",
  "break_condition": "result == CHANNEL_OK",
  "retry_delay_ms": 30000,
  "max_attempts": null
}
```

### Integration with existing extraction

Policy-fact extraction runs as a post-extraction pass, similar to
framework detection and boundary extraction. It uses the already-parsed
AST (tree-sitter nodes) and the extracted symbols.

Sequence:
1. File extraction (symbols, edges, metrics) — existing
2. Policy-fact extraction (AST patterns) — new
3. Framework detection — existing
4. Boundary extraction — existing

Policy-fact extraction does NOT require enrichment. It operates on
syntax patterns, not resolved types.

## C-First Proving Ground

swupdate is the validation corpus. Target extraction points:

### Priority 1: STATUS_MAPPING

- `corelib/server_utils.c::map_channel_retcode` — channel-to-server translation
- `corelib/channel_curl.c::channel_map_http_code` — HTTP code to channel code
- `corelib/channel_curl.c::channel_map_curl_error` — curl error to channel code
- Handler result mapping functions (scattered across handlers/)

### Priority 2: BEHAVIORAL_MARKER

- `corelib/channel_curl.c::channel_get_file` — retry loop with resume
- `corelib/downloader.c` — retry and timeout handling
- Handler recovery patterns

### Priority 3: RETURN_FATE

- Handler callback return value handling
- IPC message send result handling
- Library call result patterns

### Deferred (Priority 4-5):

- BRANCH_OUTCOME: broader than status mapping, requires scope control
- DEFAULT_PROVENANCE: requires config-file correlation

## Implementation Plan

### Slice PF-1: STATUS_MAPPING extraction (C)

Scope:
- Pattern matcher for C switch-on-enum functions
- Heuristic: input and output types both end in `_t` or `_e` or `Result` or `Error`
- Extract mapping table from case arms
- Persist to policy_facts table

Validation:
- 3 known status-mapping functions in swupdate/corelib
- Precision: no false positives on swupdate
- Recall: all 3 functions detected

### Slice PF-2: BEHAVIORAL_MARKER extraction (C)

Scope:
- Pattern matcher for while/for loops with:
  - Sleep/usleep/nanosleep call in body
  - Break/return on success condition
  - Counter or result-based loop condition
- Extract retry parameters (delay, max attempts if detectable)

Validation:
- channel_get_file retry loop detected
- downloader retry logic detected
- Precision: no false positives on retry-like patterns

### Slice PF-3: RETURN_FATE extraction (C)

Scope:
- For each call expression, determine fate of result:
  - Statement expression (no assignment) -> IGNORED
  - Immediate if/switch check -> CHECKED
  - Direct return -> PROPAGATED
  - Assignment to variable -> STORED
  - Passed to another function -> TRANSFORMED
- Focus on calls to functions with non-void return type

Validation:
- Spot-check 20 call sites in swupdate handlers
- Verify IGNORED detection for fire-and-forget patterns

### Slice PF-4: CLI surface

- `rmap policy <db> <repo>` — list all policy facts
- `rmap policy <db> <repo> --kind STATUS_MAPPING` — filter by kind
- `rmap policy <db> <repo> --file <path>` — filter by file
- `rmap policy <db> <repo> --symbol <name>` — filter by symbol

JSON output with stable keys and evidence.

### Slice PF-5: Cross-layer policy edges

Materialize cross-layer edges in the graph:
- STATUS_MAPPING function produces a `TRANSLATES_STATUS` edge
  from source type node to target type node
- RETURN_FATE IGNORED produces a `RESULT_DISCARDED` marker
- BEHAVIORAL_MARKER RETRY_LOOP produces a `HAS_RETRY_BEHAVIOR` marker

These are inference-level facts, not extraction-level edges.

## Deferred Items

### Language expansion beyond C

- C++: inherits C patterns, adds class method patterns
- Rust: Result/Option patterns, `?` operator propagation
- Java: exception translation, checked vs unchecked
- TypeScript: Promise/async error handling

Each language requires its own pattern matchers. C-first proves the model.

### Cross-file policy flow

Given a status code, trace its propagation across files:
1. Where is it produced?
2. What translations does it pass through?
3. Where is it ultimately handled or discarded?

This requires graph traversal over policy facts + call graph.

### Policy contract validation

Detect policy violations:
- Status codes that are translated then lost (IGNORED fate after TRANSLATES)
- Retry loops without exponential backoff (flat retry)
- Missing default case in status mapping

This is enforcement, not discovery. Deferred per vision doc.

### Integration with orient/check/explain

- `orient` should surface top policy facts for focused scope
- `check` should include policy-related conditions (no-ignored-error-codes)
- `explain` should show policy facts affecting a symbol

These are feature integrations after the support module is proven.

## Open Questions

### Q1: Granularity of RETURN_FATE

Should we track fate per call site (fine-grained) or per caller function
(coarse-grained)? Fine-grained is more precise but produces more data.

Tentative answer: Fine-grained (per call site). Storage is cheap.

### Q2: Enum type heuristics for C

C enums are stringly-typed at the AST level. How do we detect that a
function is a status-mapping function vs. a generic enum converter?

Tentative answer: Naming heuristics (contains `map`, `convert`, `translate`,
or input/output types contain `result`, `error`, `status`, `code`, `ret`).
Also: switch-on-param-1, return-enum pattern.

### Q3: Behavioral marker scope

BEHAVIORAL_MARKER is broad. Should we scope PF-2 to RETRY_LOOP only and
defer TIMEOUT, RESUME, CHECKPOINT to later slices?

Tentative answer: Yes. RETRY_LOOP is the highest-value pattern for
understanding failure recovery. Scope creep is the enemy of shipping.

### Q4: Interaction with existing inferences

Policy facts are a new fact category. Should they be stored in the
existing `inferences` table with a new inference kind, or in a dedicated
`policy_facts` table?

Tentative answer: Dedicated table. Policy facts have structured evidence
that doesn't fit the inference model. They are closer to extracted facts
than to derived inferences.
