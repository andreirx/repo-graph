# PF-3: RETURN_FATE Extraction

Status: COMPLETE
Scope: RETURN_FATE only. AST-local fate classification at call sites.

## Goal

Extract what happens to function return values at each call site. The agent
learns whether a result is ignored, checked, propagated, or transformed
without reading the surrounding code.

This answers: "Is this error code being dropped on the floor?" and
"Does this status flow upstream or get swallowed?"

## Non-Goals

- STATUS_MAPPING (PF-1, shipped)
- BEHAVIORAL_MARKER (PF-2)
- BRANCH_OUTCOME, DEFAULT_PROVENANCE (PF-4+)
- Languages other than C
- Whole-program dataflow analysis
- SSA or control-flow graph construction
- Alias resolution (tracking variable reassignments)
- Stored-then-checked-later tracking (requires dataflow)
- Cross-function fate propagation
- Integration with orient/check/explain
- TypeScript `rgr` parity

## Support Module

Location: `rust/crates/policy-facts/`

### Additions to crate structure

```
rust/crates/policy-facts/
  src/
    types.rs              # Add ReturnFate, FateKind
    extractors/
      return_fate.rs      # C extractor
```

### ReturnFate DTO

```rust
/// What happens to a function's return value at a specific call site.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReturnFate {
    /// Stable key of the called function (callee).
    /// None if callee is unresolved (e.g., function pointer).
    pub callee_key: Option<String>,
    
    /// Callee function name as it appears in source.
    pub callee_name: String,
    
    /// Stable key of the calling function (caller).
    pub caller_key: String,
    
    /// Caller function name (for display).
    pub caller_name: String,
    
    /// Source file path (repo-relative).
    pub file_path: String,
    
    /// Line number of the call expression.
    pub line: u32,
    
    /// Column of the call expression start.
    pub column: u32,
    
    /// Classification of what happens to the return value.
    pub fate: FateKind,
    
    /// Additional context depending on fate kind.
    pub evidence: FateEvidence,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FateKind {
    /// Return value discarded. Call is a statement expression.
    Ignored,
    
    /// Return value immediately tested in if/switch/ternary condition.
    Checked,
    
    /// Return value directly returned from caller.
    Propagated,
    
    /// Return value passed to another function call.
    Transformed,
    
    /// Return value assigned to a variable (fate unknown without dataflow).
    Stored,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum FateEvidence {
    #[serde(rename = "ignored")]
    Ignored {
        /// True if the call is cast to void: `(void)func()`.
        explicit_void_cast: bool,
    },
    
    #[serde(rename = "checked")]
    Checked {
        /// The kind of check: "if", "switch", "ternary", "while", "for".
        check_kind: String,
        
        /// The comparison operator if simple binary check.
        /// e.g., "==", "!=", "<", ">=". None if complex expression.
        operator: Option<String>,
        
        /// The compared-to value if simple literal or identifier.
        /// e.g., "0", "NULL", "CHANNEL_OK". None if complex.
        compared_to: Option<String>,
    },
    
    #[serde(rename = "propagated")]
    Propagated {
        /// True if wrapped in a transformation before return.
        /// e.g., `return map_result(func())` vs `return func()`.
        wrapped: bool,
        
        /// Wrapper function name if wrapped.
        wrapper: Option<String>,
    },
    
    #[serde(rename = "transformed")]
    Transformed {
        /// The function receiving this result as an argument.
        receiver_name: String,
        
        /// Which argument position (0-indexed).
        argument_index: u32,
    },
    
    #[serde(rename = "stored")]
    Stored {
        /// Variable name receiving the assignment.
        variable_name: String,
        
        /// True if assignment is inside a condition expression.
        /// e.g., `if ((x = func()) != OK)` → immediately_checked = true.
        /// Simple assignment `x = func();` → immediately_checked = false.
        /// Does NOT track next-statement usage (that requires dataflow).
        immediately_checked: bool,
    },
}
```

### Extractor interface

```rust
/// Extract RETURN_FATE facts from a C file's AST.
/// 
/// Uses a static known-void list to filter out noise (printf, free, etc.).
/// All other callees are assumed non-void and included in output.
/// Unresolved callees (function pointers, vtable calls) are included
/// with callee_key = None.
///
/// callee_key resolution:
/// - Direct calls to same-file functions: resolved to stable key
/// - Direct calls to external functions: None (requires symbol table)
/// - Vtable/pointer calls: None (inherent limitation)
pub fn extract_return_fates(
    tree: &tree_sitter::Tree,
    source: &[u8],
    file_path: &str,
    repo_uid: &str,
) -> Vec<ReturnFate>;
```

The extractor:
1. Builds a function registry from same-file function definitions
2. Finds all `call_expression` nodes
3. Skips calls where callee name is in the known-void list
4. Classifies remaining calls by AST context
5. Resolves callee_key from registry (None if external or vtable)
6. Emits ReturnFate for each non-void call

Note: Cross-file callee_key resolution requires symbol table access,
deferred to PF-3b.

### Extractor rules (C only)

#### Fate classification

For each `call_expression` node:

1. **Find enclosing context:** Walk up the AST to find the immediate
   parent statement or expression context.

2. **Classify by context:**

   Classification uses innermost-context-first precedence. Walk up from
   the call expression and classify at the first matching context.

   | Parent Node Type | Fate | Notes |
   |-----------------|------|-------|
   | `assignment_expression` | STORED | Assigned to variable (check immediately_checked) |
   | `init_declarator` | STORED | Initialized variable |
   | `expression_statement` (call is entire statement) | IGNORED | Fire-and-forget |
   | `cast_expression` to `void` | IGNORED | Explicit suppression |
   | `argument_list` (call is argument to another call) | TRANSFORMED | Passed to receiver |
   | `return_statement` (call is return value) | PROPAGATED | Direct propagation |
   | `return_statement` with wrapper call | PROPAGATED (wrapped) | Transformed propagation |
   | `if_statement` condition (no assignment) | CHECKED | Direct test |
   | `switch_statement` condition (no assignment) | CHECKED | Direct test |
   | `conditional_expression` condition (no assignment) | CHECKED | Ternary test |
   | `while_statement` condition (no assignment) | CHECKED | Loop condition |
   | `for_statement` condition (no assignment) | CHECKED | Loop condition |
   | `binary_expression` with comparison (no assignment) | CHECKED | Inline comparison |

   **Precedence rule:** Assignment contexts take priority over condition contexts.
   `if ((result = func()) != OK)` matches `assignment_expression` FIRST, so
   the fate is STORED (with immediately_checked=true), NOT CHECKED.

3. **Extract evidence:** Based on fate kind, extract relevant details
   (operator, compared-to value, wrapper name, variable name, etc.).

4. **Skip void calls:** If callee is known to return void, skip the call site.
   Use the `callee_has_return` callback or a static void-function list.

#### Known void functions (C stdlib)

Do not emit RETURN_FATE for calls to:
- `free`, `abort`, `exit`, `_Exit`, `quick_exit`
- `va_start`, `va_end`, `va_copy`
- `setbuf`, `setvbuf` (when result unused is normal)
- `perror`, `puts`, `putchar`, `putc`, `fputc`, `fputs` (commonly ignored)
- `printf`, `fprintf`, `sprintf`, `snprintf` family (commonly ignored)
- `memcpy`, `memmove`, `memset` (return value is input, commonly ignored)
- `strcpy`, `strncpy`, `strcat`, `strncat` (return value is input)

This list is for noise reduction, not correctness. These functions have
returns, but ignoring them is idiomatic C. PF-3 focuses on status/result
returns that indicate success/failure.

### Storage port

```rust
pub trait PolicyFactsStorageWrite {
    // ... existing ...
    
    fn insert_return_fates(
        &self,
        snapshot_uid: &str,
        fates: &[ReturnFate],
    ) -> Result<usize, PolicyFactsStorageError>;
}

pub trait PolicyFactsStorageRead {
    // ... existing ...
    
    fn query_return_fates(
        &self,
        snapshot_uid: &str,
        file_filter: Option<&str>,
        callee_filter: Option<&str>,
        fate_filter: Option<FateKind>,
    ) -> Result<Vec<ReturnFate>, PolicyFactsStorageError>;
}
```

## Storage Schema

Migration 023 adds one table.

```sql
-- Migration: 023-return-fates

CREATE TABLE return_fates (
    uid TEXT PRIMARY KEY,
    snapshot_uid TEXT NOT NULL REFERENCES snapshots(uid) ON DELETE CASCADE,
    callee_key TEXT,              -- NULL if unresolved
    callee_name TEXT NOT NULL,
    caller_key TEXT NOT NULL,
    caller_name TEXT NOT NULL,
    file_path TEXT NOT NULL,
    line INTEGER NOT NULL,
    col INTEGER NOT NULL,
    fate TEXT NOT NULL,           -- IGNORED, CHECKED, PROPAGATED, TRANSFORMED, STORED
    evidence_json TEXT NOT NULL,
    
    UNIQUE(snapshot_uid, caller_key, line, col)
);

CREATE INDEX return_fates_by_snapshot ON return_fates(snapshot_uid);
CREATE INDEX return_fates_by_file ON return_fates(file_path);
CREATE INDEX return_fates_by_callee ON return_fates(callee_name);
CREATE INDEX return_fates_by_fate ON return_fates(fate);
```

Note: Unique constraint is per call site (caller + line + col), not per
callee. Multiple calls on the same line are distinguished by column.

## CLI Contract

### Command

```
rmap policy <db_path> <repo_uid> --kind RETURN_FATE [--file <path>] [--callee <name>] [--fate <IGNORED|CHECKED|...>]
```

Extends `rmap policy` with RETURN_FATE kind and fate-specific filters.

### Output (JSON)

```json
{
  "repo": "swupdate",
  "snapshot": "swupdate/2026-04-29T...",
  "kind": "RETURN_FATE",
  "facts": [
    {
      "callee_key": "swupdate:corelib/channel_curl.c#channel_map_curl_error:SYMBOL:FUNCTION",
      "callee_name": "channel_map_curl_error",
      "caller_key": "swupdate:corelib/channel_curl.c#channel_get_file:SYMBOL:FUNCTION",
      "caller_name": "channel_get_file",
      "file_path": "corelib/channel_curl.c",
      "line": 1456,
      "column": 12,
      "fate": "STORED",
      "evidence": {
        "type": "stored",
        "variable_name": "result",
        "immediately_checked": false
      }
    },
    {
      "callee_key": null,
      "callee_name": "map_channel_retcode",
      "caller_key": "swupdate:suricatta/server_hawkbit.c#server_get_device_info:SYMBOL:FUNCTION",
      "caller_name": "server_get_device_info",
      "file_path": "suricatta/server_hawkbit.c",
      "line": 577,
      "column": 6,
      "fate": "STORED",
      "evidence": {
        "type": "stored",
        "variable_name": "result",
        "immediately_checked": true
      }
    },
    {
      "callee_key": null,
      "callee_name": "install_update",
      "caller_key": "swupdate:suricatta/suricatta.c#start_suricatta:SYMBOL:FUNCTION",
      "caller_name": "start_suricatta",
      "file_path": "suricatta/suricatta.c",
      "line": 351,
      "column": 4,
      "fate": "IGNORED",
      "evidence": {
        "type": "ignored",
        "explicit_void_cast": false
      }
    }
  ],
  "count": 3,
  "summary": {
    "by_fate": {
      "CHECKED": 69,
      "IGNORED": 126,
      "PROPAGATED": 7,
      "STORED": 79,
      "TRANSFORMED": 15
    }
  }
}
```

Note: `callee_key` is resolved for same-file functions (e.g., `channel_map_curl_error`
called from `channel_get_file` in the same file). Cross-file calls show `null`
(e.g., `map_channel_retcode` is defined in `server_utils.c`, called from
`server_hawkbit.c`).

### Exit codes

- 0: success
- 1: no facts found (not an error, just empty)
- 2: invalid arguments or DB error

## Validation Corpus

### swupdate targets

Primary validation targets:

| File | Line | Caller | Callee | Expected Fate | Notes |
|------|------|--------|--------|---------------|-------|
| suricatta/suricatta.c | 351 | start_suricatta | server->install_update | IGNORED | Fire-and-forget in polling loop |
| suricatta/server_hawkbit.c | 342 | (caller TBD) | map_channel_retcode | STORED | `result = map_channel_retcode(...)` |
| suricatta/server_hawkbit.c | 577 | (caller TBD) | map_channel_retcode | STORED | `if ((result = map_channel_retcode(...)) != SERVER_OK)` with immediately_checked=true |
| suricatta/server_hawkbit.c | 632 | (caller TBD) | map_channel_retcode | STORED | Same compound-assignment-in-condition pattern |
| corelib/channel_curl.c | 1455 | channel_get_file | curl_easy_perform | STORED | `curlrc = curl_easy_perform(...)` |
| corelib/channel_curl.c | 1456 | channel_get_file | channel_map_curl_error | STORED | `result = channel_map_curl_error(curlrc)` |

Note: The `if ((result = func()) != OK)` pattern is STORED with
immediately_checked=true, NOT CHECKED. The call is assigned to a variable;
the check is on the variable, not the call.

Secondary validation (verify classification accuracy):

| Pattern | Expected Fate | immediately_checked | Notes |
|---------|---------------|---------------------|-------|
| `func();` | IGNORED | n/a | Standalone statement |
| `(void)func();` | IGNORED | n/a | Explicit void cast |
| `if (func() == OK)` | CHECKED | n/a | Direct condition |
| `return func();` | PROPAGATED | n/a | Direct return |
| `return map(func());` | PROPAGATED (wrapped) | n/a | Wrapped return |
| `other(func())` | TRANSFORMED | n/a | Argument to another call |
| `x = func();` | STORED | false | Simple assignment |
| `x = func(); if (x)` | STORED | false | Next-stmt check NOT detected (dataflow) |
| `if ((x = func()) != OK)` | STORED | true | Compound assignment in condition |

### Required assertions

1. **server->install_update() IGNORED detected (line 351):**
   - Caller = start_suricatta
   - explicit_void_cast = false (raw ignore)
   - callee_key = None (method call through vtable pointer)

2. **map_channel_retcode STORED detected (line 342):**
   - Simple assignment to `result` variable
   - immediately_checked = false (or true if next statement is condition)

3. **map_channel_retcode STORED detected (line 577):**
   - Compound assignment in condition `if ((result = ...) != SERVER_OK)`
   - immediately_checked = true

4. **Precision on void functions:**
   - printf/free/memcpy calls do NOT produce IGNORED facts
   - Only non-void callees tracked

5. **Method/vtable calls included:**
   - `server->install_update()` produces a fact with callee_key = None
   - callee_name = "install_update"

### Test structure

```
rust/crates/policy-facts/tests/
  return_fate_validation.rs   # Integration test on swupdate corpus
```

Unit tests in `rust/crates/policy-facts/src/extractors/return_fate.rs`.

## Implementation Sequence

1. **DTO addition:** Add `ReturnFate`, `FateKind`, `FateEvidence` to types.rs.

2. **Void function list:** Define static list of commonly-ignored C functions.

3. **Extractor unit tests:** Write tests for each fate classification pattern
   using synthetic C fragments.

4. **Fate classifier:** Implement parent-context walking and classification.

5. **Evidence extraction:** Extract operator, compared-to, wrapper, variable
   name for each fate kind.

6. **Storage migration:** Add `return_fates` table.

7. **Storage adapter:** Implement write/read traits.

8. **Indexer integration:** Call extractor during C file extraction postpass.
   Integrate with symbol table for callee_key resolution.

9. **CLI extension:** Extend `rmap policy` to accept `--kind RETURN_FATE`
   with `--callee` and `--fate` filters.

10. **swupdate validation:** Verify all target patterns detected correctly.

## Acceptance Criteria

- [x] `rmap policy swupdate.db swupdate --kind RETURN_FATE` returns facts
- [x] server->install_update() in start_suricatta classified as IGNORED
- [x] map_channel_retcode at line 577 classified as STORED with immediately_checked=true
- [x] map_channel_retcode at line 632 classified as STORED with immediately_checked=true
- [x] curl_easy_perform in channel_get_file classified as STORED
- [x] Summary by_fate counts in output (deterministic via BTreeMap)
- [x] --callee filter works (show fates for specific callee)
- [x] --fate IGNORED filter works (show only ignored results)
- [x] No IGNORED facts for printf/free/memcpy calls
- [x] Compound-assignment-in-condition classified as STORED, not CHECKED
- [x] Unit tests cover all FateKind variants and evidence extraction
- [x] Storage migration 023 and round-trip verified
- [x] callee_key resolved for same-file functions (None for external/vtable calls)
- [N/A] map_channel_retcode at line 342: function contains STRINGIFY macro that
  breaks tree-sitter parsing (see Known Limitations)

## Known Limitations

### STRINGIFY and similar macros break parsing

Functions containing `STRINGIFY(...)` macros with embedded JSON-like
content (braces, colons) cause tree-sitter to misparse the function body.
Calls inside such functions are not extracted.

Example: `server_send_cancel_reply` in `suricatta/server_hawkbit.c` contains:
```c
static const char* const json_hawkbit_cancelation_feedback = STRINGIFY(
{
    "id": %d,
    "time": "%s",
    ...
}
);
```

Tree-sitter sees the inner `{` and `}` as compound statements, breaking
the parse of the enclosing function. This affects ~4 functions in
server_hawkbit.c.

Workaround: None at tree-sitter level. Would require preprocessing source
before parsing, which is out of scope for PF-3.

### callee_key resolution is same-file only

`callee_key` is resolved for direct calls to functions defined in the
same file. Cross-file references require access to the symbol table
during extraction, which is deferred to PF-3b.

- Same-file function call: `callee_key` = resolved stable key
- Cross-file function call: `callee_key` = None
- Vtable/pointer call: `callee_key` = None (inherent limitation)

## What PF-3 Does NOT Include

- Dataflow analysis (tracking variable through multiple statements)
- SSA construction
- Alias resolution
- Cross-function fate propagation
- Interprocedural analysis
- STORED-then-checked-later beyond immediate next statement
- Macro-expanded call tracking
- Function pointer call fate (callee_key is NULL)
- Cross-file callee_key resolution (requires symbol table)
- Languages other than C

## Locked Scope Decisions

These are binding decisions for PF-3. Not open questions.

### AST-local only

Fate classification uses only the immediate AST context of the call
expression. No dataflow, no variable tracking beyond the assignment
statement.

Rationale: Dataflow is a different complexity tier. PF-3 proves the
value of fate classification with local analysis. If valuable, dataflow
can be a later expansion (PF-3b or PF-4).

### STORED is a terminal fate

When a result is assigned to a variable, the fate is STORED. We do NOT
attempt to track what happens to that variable in subsequent statements.

The `immediately_checked` field is true ONLY when the assignment is
inside a condition expression: `if ((x = func()) != OK)`. This is
AST-local (the assignment node is a child of the condition node).

Cross-statement tracking (`x = func(); if (x)`) is NOT supported.
That requires dataflow analysis, which is out of scope for PF-3.

Rationale: STORED is an honest answer: "we saw the assignment, further
fate unknown." The compound-assignment-in-condition case is a bonus
that costs nothing extra because it's visible in the same AST walk.

### Void function list is noise reduction

The void function list (printf, free, memcpy, etc.) is for noise
reduction. These are not errors to ignore; they are idiomatic patterns.
The list can be expanded based on false-positive feedback.

Rationale: Reporting every ignored memcpy return value adds noise, not
signal. The focus is on status/result returns where IGNORED indicates
potential error-handling gaps.

### Unresolved callees are included with NULL key

Calls through function pointers or unresolved names are included in
output with `callee_key: null`. The callee_name is the syntactic name.

Rationale: Unresolved calls may still have interesting fates (e.g.,
callback results ignored). Excluding them loses signal. NULL key makes
unresolved status explicit.

### immediately_checked is single-statement only

The `immediately_checked` heuristic in STORED evidence looks at exactly
one context:

1. **Compound assignment in condition:** `if ((x = func()) != OK)` sets
   immediately_checked = true because the assignment and check are in
   the same AST node.

2. **Next statement is condition on same variable:** NOT supported in PF-3.
   This would require tracking the variable across statements, which is
   dataflow.

Rationale: Compound-assignment-in-condition is AST-local (the assignment
expression is the condition's child). Tracking "next statement uses x"
requires dataflow. PF-3 stays AST-local.

### Macro invocations are treated syntactically

PF-2 analyzes macro invocations as-if they are function calls. If the
AST contains `CHECK(channel_get_file(...))` where CHECK is a macro, the
extractor sees a call to `CHECK` with `channel_get_file(...)` as argument.

The inner call `channel_get_file(...)` is classified based on its AST
context (argument to CHECK → TRANSFORMED). The outer "call" to CHECK
may also be classified if it appears to be a statement.

Known limitation: If CHECK expands to `if ((result = ...) != OK)`, we
do not see that expansion. We see the syntactic macro call. This may
misclassify the fate of the inner call.

Rationale: tree-sitter operates on source text, not preprocessed text.
Preprocessing would require build system integration. PF-3 accepts
this limitation.

### Comparison values are stored as raw source text

CHECKED evidence stores `compared_to` as raw source text from the AST.
No normalization is performed:
- `0` stays `0`
- `NULL` stays `NULL`
- `CHANNEL_OK` stays `CHANNEL_OK`

Rationale: Raw text preserves evidence. The agent or consumer can
normalize if needed. Normalization in the extractor risks losing
project-specific constants.
