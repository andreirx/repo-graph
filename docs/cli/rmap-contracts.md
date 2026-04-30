# Rust CLI (`rmap`) Contract

The Rust CLI is the primary binary for agent-facing commands. This document
specifies intentional contract differences from the original TS CLI (`rgr`).

These are **design decisions**, not gaps or debt.

## Invocation Pattern

All `rmap` commands use `<db_path> <repo_uid>` positional arguments:

```
rmap <command> <db_path> <repo_uid> [options]
```

This differs from `rgr` which uses a repo registry (`rgr <command> <repo_name>`).
The `<db_path> <repo_uid>` pattern keeps `rmap` consistent and self-contained
until a registry slice ships.

## Command-Specific Contracts

### `gate` — Waiver Overlay

PASS obligations are **not waivable**.

- TS (`rgr`): unconditionally marks WAIVED on any waiver match
- Rust (`rmap`): only suppresses non-PASS verdicts

Rationale: allowing PASS-waiver conflated "no evidence" with "suppressed."
A PASS verdict means the obligation is satisfied; there is nothing to waive.
(Rust-25 deliberate correction.)

### `gate` — Quality-Policy Assessment Integration

Gate consumes pre-computed quality-policy assessments and reduces them into
the gate outcome. Quality assessments are reported separately from obligations
because they have different verdict semantics.

**Report shape:**

```json
{
  "obligations": [ /* requirement-based evaluations */ ],
  "quality_assessments": [
    {
      "policy_id": "QP-001",
      "policy_version": 1,
      "policy_kind": "no_new",
      "severity": "fail",
      "assessment_state": "present",
      "computed_verdict": "PASS",
      "is_comparative": true,
      "violations_count": 0
    }
  ],
  "outcome": {
    "outcome": "pass",
    "exit_code": 0,
    "mode": "default",
    "counts": { /* obligation counts */ },
    "quality_counts": {
      "total": 1,
      "pass": 1,
      "fail": 0,
      "advisory_fail": 0,
      "missing": 0,
      "not_comparable": 0,
      "not_applicable": 0
    }
  }
}
```

**Verdict semantics:**

| Assessment State | Verdict | Severity | Exit Code |
|-----------------|---------|----------|-----------|
| Missing | N/A | Any | 2 (incomplete) |
| Present | NOT_COMPARABLE | Any | 2 (incomplete) |
| Present | FAIL | `fail` | 1 (fail) |
| Present | FAIL | `advisory` | 0 (reported only) |
| Present | PASS | Any | 0 (pass) |
| Present | NOT_APPLICABLE | Any | 0 (pass) |

**Key semantics:**

1. **Missing assessment = incomplete.** Active quality-policy without computed
   assessment is treated as missing required evidence.

2. **NOT_COMPARABLE = incomplete.** Comparative policies (`no_new`,
   `no_worsened`) without a baseline snapshot return NOT_COMPARABLE. This
   blocks gate until a baseline is established.

3. **Severity determines blocking.** `severity: fail` assessments with FAIL
   verdict block gate. `severity: advisory` assessments are reported but do
   not affect exit code.

4. **No waiver overlay.** Quality-policy waivers are explicitly deferred.
   Quality assessments do not participate in the waiver system.

**Mode interaction:**

- `default`: Missing/NOT_COMPARABLE = exit 2; FAIL(blocking) = exit 1
- `strict`: Missing/NOT_COMPARABLE/FAIL(blocking) = exit 1
- `advisory`: Missing/NOT_COMPARABLE ignored; FAIL(blocking) = exit 1

### `violations` — Output Shape

`results` is an object with explicit sections:

```json
{
  "results": {
    "declared_boundary_violations": [...],
    "discovered_module_violations": [...]
  },
  "stale_declarations": [...],
  "declared_count": N,
  "discovered_count": N
}
```

TS uses a flat array. The object shape is more explicit for agent consumption.

### `modules list` — Degradation Contract

Envelope includes `rollups_degraded` (boolean) and `warnings` (string array).

When policy parsing fails:
- Exit 0 (not exit 2)
- `rollups_degraded: true`
- Warning message in `warnings[]`
- `violation_count: null` on each module
- Other rollup fields remain populated

Deliberate: orientation surfaces must survive policy corruption.

### `modules show` — Briefing Shape

This is a briefing, not a list. Envelope differs from QueryResult:

```json
{
  "module": { /* identity */ },
  "rollups": { /* counts */ },
  "outbound_dependencies": [ /* weighted */ ],
  "inbound_dependencies": [ /* weighted */ ],
  "violations": [ /* source-side only */ ]
}
```

No `results`/`count` wrapper.

### `modules violations` — Canonical Command

TS CLI does not have an equivalent command (`rgr arch violations` uses a
different selector domain).

Output includes `diagnostics` object reporting derivation counts so callers
can detect degraded graphs where ownership gaps suppress violations.

### Measurement Commands (`churn`, `hotspots`, `risk`)

Query-time computation, not persistence-first.

- Git is the authoritative history source
- `repo-graph-git` crate wraps git CLI
- TS implementation is reference, not spec
- Explicit anchoring for gate integration is future opt-in, not automatic

### `dead` — DISABLED

**Status: Deliberately disabled as of 2026-04-27.**

The `dead` command is removed from the CLI surface because current signal
quality produces 85-95% false positive rates on real-world codebases.

**Root causes:**
- Missing framework detectors (Spring, React, Axum, FastAPI)
- Missing entrypoint declarations
- No coverage-backed evidence

**Underlying substrate preserved:**
- `storage::find_dead_nodes()` still works
- `trust::assess_dead_confidence()` still works
- Tests pinning current behavior remain

**Reintroduction plan:**

This surface will be split into TWO separate commands:

1. **`rmap orphans`** — Structural graph orphans with no deadness claim.
   Pure graph heuristic. "Not currently referenced in the graph we built."
   Useful for orientation, not deletion decisions.

2. **`rmap dead`** — Coverage-backed + framework-liveness-backed detection.
   Much stronger evidence required. "Unexecuted under measured scenarios
   AND structurally weakly connected."

**Criteria for reintroduction:**
- Framework entrypoint detection mature (Spring, React, Axum, FastAPI), OR
- Coverage import surface operational, OR
- Entrypoint declaration workflow established

See `docs/TECH-DEBT.md` for full rationale

### `policy` — Policy-Facts Discovery

Query extracted policy-facts from C source files. Facts are populated
automatically during `rmap index` / `rmap refresh`.

**Usage:**

```
rmap policy <db_path> <repo_uid> [--kind STATUS_MAPPING|BEHAVIORAL_MARKER|RETURN_FATE] [--file <path>] [--callee <name>] [--fate <kind>]
```

**Supported kinds:**

- `STATUS_MAPPING` (default): Status/error code translation functions.
  Functions that switch on an input status and return an output status.
  PF-1 scope. See `docs/shipped/policy-facts/pf-1-status-mapping.md`.

- `BEHAVIORAL_MARKER`: Behavioral patterns in control flow.
  - `RETRY_LOOP`: loops with sleep/delay calls (retry with backoff)
  - `RESUME_OFFSET`: curl `CURLOPT_RESUME_FROM*` patterns
  PF-2 scope. See `docs/shipped/policy-facts/pf-2-behavioral-marker.md`.

- `RETURN_FATE`: What happens to function return values at each call site.
  - `IGNORED`: return value discarded
  - `CHECKED`: return value tested in condition
  - `PROPAGATED`: return value returned from caller
  - `TRANSFORMED`: return value passed to another function
  - `STORED`: return value assigned to variable
  PF-3 scope. See `docs/shipped/policy-facts/pf-3-return-fate.md`.

**Filters (RETURN_FATE only):**

- `--callee <name>`: filter by callee function name
- `--fate <kind>`: filter by fate kind (IGNORED, CHECKED, PROPAGATED, TRANSFORMED, STORED)

**Output (STATUS_MAPPING):**

```json
{
  "repo": "swupdate",
  "snapshot": "swupdate/2026-04-29T...",
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
        { "inputs": ["CHANNEL_ENONET", "CHANNEL_EAGAIN"], "output": "SERVER_EAGAIN" },
        { "inputs": ["CHANNEL_OK"], "output": "SERVER_OK" }
      ],
      "default_output": "SERVER_EERR"
    }
  ],
  "count": 1
}
```

**Output (BEHAVIORAL_MARKER):**

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
      "line_start": 1359,
      "line_end": 1364,
      "kind": "RETRY_LOOP",
      "evidence": {
        "type": "retry_loop",
        "loop_kind": "for",
        "sleep_call": "sleep",
        "delay_ms": 1000,
        "max_attempts": 4,
        "break_condition": "file_handle > 0"
      }
    },
    {
      "symbol_key": "swupdate:corelib/channel_curl.c#channel_get_file:SYMBOL:FUNCTION",
      "function_name": "channel_get_file",
      "file_path": "corelib/channel_curl.c",
      "line_start": 1437,
      "line_end": 1439,
      "kind": "RESUME_OFFSET",
      "evidence": {
        "type": "resume_offset",
        "api_call": "curl_easy_setopt",
        "option_name": "CURLOPT_RESUME_FROM_LARGE",
        "offset_source": "total_bytes_downloaded"
      }
    }
  ],
  "count": 2
}
```

**Output (RETURN_FATE):**

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
  "count": 2,
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

Note: `callee_key` is resolved for same-file direct calls. Cross-file and vtable
calls show `null`. The `summary.by_fate` counts are sorted alphabetically
(deterministic output).

**Exit codes:**

- 0: success (facts found)
- 1: no facts found (not an error, just empty)
- 2: runtime error (invalid args, DB error, missing repo/snapshot)

**Scope:**

- C files only (.c, .h)
- C++ is explicitly out of scope for PF-1/PF-2
- Languages other than C deferred

## JSON-Only Output

`rmap` always produces JSON on stdout. There is no `--json` flag because
JSON is the default and only format.

Human-readable table format is not planned. Agents are the primary consumer.
