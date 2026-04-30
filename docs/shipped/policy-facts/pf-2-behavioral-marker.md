# PF-2: BEHAVIORAL_MARKER Extraction

Status: COMPLETE
Scope: BEHAVIORAL_MARKER only. Scoped to RETRY_LOOP and RESUME_OFFSET markers.

## Goal

Extract behavioral markers from C code that indicate policy-relevant control
flow patterns: retry loops with sleep/delay, resume-from-offset handling.
These are local behavior semantics visible from AST structure alone.

The agent learns: "This function has retry logic" or "This function uses
resume-from offsets" without reading the function body.

## Non-Goals

- STATUS_MAPPING (PF-1, shipped)
- RETURN_FATE (PF-3)
- BRANCH_OUTCOME, DEFAULT_PROVENANCE (PF-4+)
- Languages other than C
- Cross-file behavior correlation
- Dataflow analysis or SSA
- Timeout detection (requires timer correlation, deferred)
- Poll-loop detection without sleep (ambiguous, deferred)
- Cache-replay detection (requires state tracking, deferred)
- Integration with orient/check/explain
- TypeScript `rgr` parity

## Support Module

Location: `rust/crates/policy-facts/`

### Additions to crate structure

```
rust/crates/policy-facts/
  src/
    types.rs              # Add BehavioralMarker, MarkerKind
    extractors/
      behavioral_marker.rs   # C extractor
```

### BehavioralMarker DTO

```rust
/// A behavioral marker indicating policy-relevant control flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehavioralMarker {
    /// Stable key of the enclosing function.
    pub symbol_key: String,
    
    /// Function name (for display).
    pub function_name: String,
    
    /// Source file path (repo-relative).
    pub file_path: String,
    
    /// Line number where the marker pattern starts.
    pub line_start: u32,
    
    /// Line number where the marker pattern ends.
    pub line_end: u32,
    
    /// Kind of behavioral marker.
    pub kind: MarkerKind,
    
    /// Kind-specific evidence.
    pub evidence: MarkerEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MarkerKind {
    RetryLoop,
    ResumeOffset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MarkerEvidence {
    #[serde(rename = "retry_loop")]
    RetryLoop {
        /// Loop construct kind: "while", "for", "do_while".
        loop_kind: String,
        
        /// Sleep/delay call detected in loop body.
        /// Function name (e.g., "sleep", "usleep", "nanosleep").
        sleep_call: Option<String>,
        
        /// Retry delay in milliseconds if statically determinable.
        /// None if dynamic or not determinable.
        delay_ms: Option<u64>,
        
        /// Max attempts if statically determinable from loop condition.
        /// None if unbounded or not determinable.
        max_attempts: Option<u32>,
        
        /// Break/return condition expression as source text.
        /// None if not a simple success-check pattern.
        break_condition: Option<String>,
    },
    
    #[serde(rename = "resume_offset")]
    ResumeOffset {
        /// The API call that sets the resume offset.
        /// e.g., "curl_easy_setopt" with CURLOPT_RESUME_FROM_LARGE
        api_call: String,
        
        /// The option/flag name if applicable.
        /// e.g., "CURLOPT_RESUME_FROM_LARGE"
        option_name: Option<String>,
        
        /// Variable holding the offset if identifiable.
        offset_source: Option<String>,
    },
}
```

### Extractor interface

```rust
/// Extract BEHAVIORAL_MARKER facts from a C file's AST.
pub fn extract_behavioral_markers(
    tree: &tree_sitter::Tree,
    source: &[u8],
    file_path: &str,
    repo_uid: &str,
) -> Vec<BehavioralMarker>;
```

### Extractor rules (C only)

#### RETRY_LOOP detection

A loop is a RETRY_LOOP candidate if:

1. **Loop construct:** `while`, `for`, or `do-while` statement.

2. **Sleep call in body:** Loop body contains a call to any of:
   - `sleep`, `usleep`, `nanosleep`, `Sleep` (Windows)
   - `poll` with timeout > 0 (acts as sleep)
   
   Custom delay functions (`*delay*`, `*wait*`) are NOT matched in PF-2.
   See Locked Scope Decisions.

3. **Result-dependent control:** At least one of:
   - Break/return on a success condition (comparison to OK/SUCCESS/0)
   - Loop condition references a result/status variable
   - Counter variable with decrement or increment toward limit

4. **Not a simple poll loop:** Must have sleep/delay. Busy-wait loops
   without delay are excluded from PF-2 (ambiguous intent).

Extraction procedure:

1. Find all loop statements (`while_statement`, `for_statement`, `do_statement`).
2. Search loop body for sleep/delay calls.
3. If found, extract:
   - Loop kind
   - Sleep function name
   - Delay value if literal or simple constant
   - Max attempts if loop condition is counter-based
   - Break condition if simple success check
4. Create marker anchored to loop start/end lines.

#### RESUME_OFFSET detection

A statement is a RESUME_OFFSET candidate if:

1. **API pattern:** Call to `curl_easy_setopt` with:
   - Second argument is `CURLOPT_RESUME_FROM` or `CURLOPT_RESUME_FROM_LARGE`

PF-2 detects ONLY the curl pattern. Generic offset patterns (lseek, fseek,
field assignments) are NOT in scope. See Locked Scope Decisions.

Extraction procedure:

1. Find all `call_expression` nodes where callee is `curl_easy_setopt`.
2. Check second argument for `CURLOPT_RESUME_FROM` or `CURLOPT_RESUME_FROM_LARGE`.
3. Extract option name and third argument (offset source) if identifiable.
4. Create marker anchored to the call statement.

### Storage port

```rust
pub trait PolicyFactsStorageWrite {
    // ... existing ...
    
    fn insert_behavioral_markers(
        &self,
        snapshot_uid: &str,
        markers: &[BehavioralMarker],
    ) -> Result<usize, PolicyFactsStorageError>;
}

pub trait PolicyFactsStorageRead {
    // ... existing ...
    
    fn query_behavioral_markers(
        &self,
        snapshot_uid: &str,
        file_filter: Option<&str>,
        kind_filter: Option<MarkerKind>,
    ) -> Result<Vec<BehavioralMarker>, PolicyFactsStorageError>;
}
```

## Storage Schema

Migration 022 adds one table.

```sql
-- Migration: 022-behavioral-markers

CREATE TABLE behavioral_markers (
    uid TEXT PRIMARY KEY,
    snapshot_uid TEXT NOT NULL REFERENCES snapshots(uid) ON DELETE CASCADE,
    symbol_key TEXT NOT NULL,
    function_name TEXT NOT NULL,
    file_path TEXT NOT NULL,
    line_start INTEGER NOT NULL,
    line_end INTEGER NOT NULL,
    kind TEXT NOT NULL,           -- 'RETRY_LOOP' or 'RESUME_OFFSET'
    evidence_json TEXT NOT NULL,  -- JSON of MarkerEvidence
    
    UNIQUE(snapshot_uid, symbol_key, kind, line_start)
);

CREATE INDEX behavioral_markers_by_snapshot ON behavioral_markers(snapshot_uid);
CREATE INDEX behavioral_markers_by_file ON behavioral_markers(file_path);
CREATE INDEX behavioral_markers_by_kind ON behavioral_markers(kind);
```

Note: A function can have multiple markers (e.g., both RETRY_LOOP and
RESUME_OFFSET in `channel_get_file`). The unique constraint includes
`line_start` to allow multiple markers per function.

## CLI Contract

### Command

```
rmap policy <db_path> <repo_uid> --kind BEHAVIORAL_MARKER [--file <path>]
```

Extends the existing `rmap policy` command with a new kind.

### Output (JSON)

```json
{
  "repo": "swupdate",
  "snapshot": "swupdate/2026-04-29T...",
  "kind": "BEHAVIORAL_MARKER",
  "facts": [
    {
      "symbol_key": "swupdate:corelib/channel_curl.c#channel_get_file:SYMBOL:FUNCTION",
      "function_name": "channel_get_file",
      "file_path": "corelib/channel_curl.c",
      "line_start": 892,
      "line_end": 945,
      "kind": "RETRY_LOOP",
      "evidence": {
        "type": "retry_loop",
        "loop_kind": "do_while",
        "sleep_call": "sleep",
        "delay_ms": 30000,
        "max_attempts": null,
        "break_condition": "result == CHANNEL_OK"
      }
    },
    {
      "symbol_key": "swupdate:corelib/channel_curl.c#channel_get_file:SYMBOL:FUNCTION",
      "function_name": "channel_get_file",
      "file_path": "corelib/channel_curl.c",
      "line_start": 876,
      "line_end": 876,
      "kind": "RESUME_OFFSET",
      "evidence": {
        "type": "resume_offset",
        "api_call": "curl_easy_setopt",
        "option_name": "CURLOPT_RESUME_FROM_LARGE",
        "offset_source": "channel->offs"
      }
    }
  ],
  "count": 2
}
```

### Exit codes

- 0: success
- 1: no facts found (not an error, just empty)
- 2: invalid arguments or DB error

## Validation Corpus

### swupdate targets

Primary validation targets in swupdate:

| File | Function | Line | Expected Marker | Notes |
|------|----------|------|-----------------|-------|
| corelib/channel_curl.c | channel_get_file | 1359-1364 | RETRY_LOOP | IPC start retry: `for (int retries = 3; ...)` with `sleep(1)` |
| corelib/channel_curl.c | channel_get_file | 1415-1490 | RETRY_LOOP | Download retry: `do { ... } while (result != CHANNEL_OK)` with `sleep(retry_sleep)` |
| corelib/channel_curl.c | channel_get_file | 1437-1439 | RESUME_OFFSET | `curl_easy_setopt(..., CURLOPT_RESUME_FROM_LARGE, ...)` |

Note: `channel_get_file` contains TWO distinct RETRY_LOOP patterns (IPC retry
and download retry) plus one RESUME_OFFSET pattern. PF-2 should produce 3
markers for this function.

Secondary validation (verify no false positives):

| File | Function | Expected | Notes |
|------|----------|----------|-------|
| corelib/channel_curl.c | channel_curl_init | None | Setup function, no retry/resume patterns. |
| corelib/downloader.c | download_from_url | None | Wrapper around channel->get_file(), no own retry. |
| suricatta/suricatta.c | suricatta_wait | None | Timed wait helper, not a retry loop. |

### Required assertions

1. **channel_get_file IPC RETRY_LOOP detected (lines 1359-1364):**
   - loop_kind = "for"
   - sleep_call = "sleep"
   - delay_ms = 1000 (literal `sleep(1)`)
   - max_attempts = 4 (literal `retries = 3` with `>= 0` condition)

2. **channel_get_file download RETRY_LOOP detected (lines 1415-1490):**
   - loop_kind = "do_while"
   - sleep_call = "sleep"
   - delay_ms = None (dynamic `channel_data->retry_sleep`)
   - break_condition contains "result != CHANNEL_OK" or similar

3. **channel_get_file RESUME_OFFSET detected (line 1437-1439):**
   - api_call = "curl_easy_setopt"
   - option_name = "CURLOPT_RESUME_FROM_LARGE"
   - offset_source = "total_bytes_downloaded"

4. **Precision check:**
   - channel_curl_init does NOT produce markers
   - download_from_url does NOT produce markers
   - suricatta_wait does NOT produce markers

5. **Multiple markers per function:**
   - channel_get_file produces 3 separate marker records (2 RETRY_LOOP + 1 RESUME_OFFSET)

### Test structure

```
rust/crates/policy-facts/tests/
  behavioral_marker_validation.rs   # Integration test on swupdate corpus
```

Unit tests in `rust/crates/policy-facts/src/extractors/behavioral_marker.rs`.

## Implementation Sequence

1. **DTO addition:** Add `BehavioralMarker`, `MarkerKind`, `MarkerEvidence` to types.rs.

2. **Extractor unit tests:** Write tests for synthetic C fragments with retry
   loops and curl resume patterns.

3. **RETRY_LOOP extractor:** Implement loop detection with sleep-call search.

4. **RESUME_OFFSET extractor:** Implement curl_easy_setopt pattern detection.

5. **Storage migration:** Add `behavioral_markers` table.

6. **Storage adapter:** Implement write/read traits on `StorageConnection`.

7. **Indexer integration:** Call extractor during C file extraction postpass.

8. **CLI extension:** Extend `rmap policy` to accept `--kind BEHAVIORAL_MARKER`.

9. **swupdate validation:** Run on swupdate, verify all targets detected.

## Acceptance Criteria

- [x] `rmap policy swupdate.db swupdate --kind BEHAVIORAL_MARKER` returns 3+ facts
- [x] channel_get_file produces 2 RETRY_LOOP markers (IPC retry + download retry)
- [x] channel_get_file produces 1 RESUME_OFFSET marker
- [x] IPC RETRY_LOOP: loop_kind="for", sleep_call="sleep", delay_ms=1000, max_attempts=4
- [x] Download RETRY_LOOP: loop_kind="do_while", sleep_call="sleep", delay_ms=None
- [x] RESUME_OFFSET: api_call="curl_easy_setopt", option_name="CURLOPT_RESUME_FROM_LARGE"
- [x] No false positives on channel_curl_init, download_from_url, suricatta_wait
- [x] Unit tests cover: loop detection, sleep-call matching, curl option
      detection, nested loop handling, preprocessor-guarded code
- [x] Storage migration 022 and round-trip verified
- [x] CLI `--kind BEHAVIORAL_MARKER` filter works

## What PF-2 Does NOT Include

- TIMEOUT markers (requires timer setup correlation)
- POLL_LOOP markers without sleep (too ambiguous)
- CACHE_REPLAY markers (requires state tracking)
- BACKOFF detection (exponential vs linear vs fixed)
- Cross-function retry pattern correlation
- Retry parameter extraction from config files
- Languages other than C

## Locked Scope Decisions

These are binding decisions for PF-2. Not open questions.

### Sleep call is required for RETRY_LOOP

A loop without a sleep/delay call is not a RETRY_LOOP in PF-2 scope.
Busy-wait loops and poll loops without delay are excluded.

Rationale: Sleep presence is a strong, unambiguous signal. Busy-wait
detection would require understanding intent, which is not AST-local.

### RESUME_OFFSET is curl-only

PF-2 detects ONLY `curl_easy_setopt` with `CURLOPT_RESUME_FROM` or
`CURLOPT_RESUME_FROM_LARGE`. The following are NOT in scope:

- `lseek` / `fseek` calls (ambiguous: may be normal file positioning)
- Assignments to `*resume*` / `*offset*` fields (requires context)
- Other HTTP library resume APIs (future expansion)

Rationale: The curl pattern is unambiguous and covers the swupdate
proving ground. Generic offset-tracking risks false positives and
requires retry-context correlation that PF-2 does not perform.

### No backoff classification

PF-2 does not classify retry delay strategy (exponential, linear, fixed).
It extracts the delay value if determinable, but does not infer the pattern.

Rationale: Backoff classification requires seeing multiple delay values
or understanding delay-growth expressions. This is beyond AST-local scope.

### Sleep function matching is libc/POSIX only

PF-2 matches only known libc/POSIX sleep functions: `sleep`, `usleep`,
`nanosleep`, `Sleep` (Windows), and `poll` with timeout.

Custom delay functions (`*delay*`, `*wait*` patterns) are NOT matched.

Rationale: libc names are unambiguous. Pattern matching on `*delay*` or
`*wait*` risks false positives (e.g., `wait()` for process, `waitpid()`).
If swupdate or another proving corpus needs custom functions, a later
slice can add a configurable allowlist.

### Retry counter extraction is trivial-only

PF-2 extracts `max_attempts` only when trivially determinable from AST:
- `for (int i = 0; i < 5; i++)` → max_attempts = 5
- `for (int retries = 3; retries >= 0; retries--)` → max_attempts = 4
- `while (count--)` with `int count = 3` in same scope → max_attempts = 3

Dynamic limits, limits from struct fields, or limits requiring dataflow
are NOT extracted. max_attempts = None in those cases.

Rationale: Keeps extractor AST-local. Complex limit extraction is
dataflow territory.
