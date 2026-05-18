# Rust CLI (`rmap`) Contract

The Rust CLI is the primary binary for agent-facing commands. This document
specifies intentional contract differences from the original TS CLI (`rgr`).

These are **design decisions**, not gaps or debt.

## Invocation Pattern

### REG-1 Commands (daemon-native)

Most query commands now use CWD-based resolution. The daemon resolves the repo
from the current working directory:

```bash
rmap orient                    # resolves repo from cwd
rmap check                     # resolves repo from cwd
rmap explain <target>          # resolves repo from cwd
rmap callers <symbol>          # resolves repo from cwd
rmap callees <symbol>          # resolves repo from cwd
rmap path <from> <to>          # resolves repo from cwd
rmap imports <file>            # resolves repo from cwd
rmap cycles                    # resolves repo from cwd
rmap stats                     # resolves repo from cwd
rmap trust                     # resolves repo from cwd
rmap gate                      # resolves repo from cwd
rmap modules list              # resolves repo from cwd
rmap modules show <module>     # resolves repo from cwd
rmap boundaries list           # resolves repo from cwd
rmap surfaces list             # resolves repo from cwd
rmap contracts list            # resolves repo from cwd
rmap docs list                 # resolves repo from cwd
rmap deps list                 # resolves repo from cwd
rmap inferences list           # resolves repo from cwd
```

Indexing commands:
```bash
rmap index [repo_path]         # daemon allocates db_path, defaults to cwd
rmap refresh                   # resolves repo from cwd
```

### Legacy Commands (still require positional args)

These commands have not yet migrated to REG-1 and still require explicit paths:

```bash
rmap assess <db_path> <repo_uid>
rmap enrich <db_path> <repo_uid>
rmap policy <db_path> <repo_uid> [options]
rmap modules boundary <db_path> <repo_uid> [options]
rmap violations <db_path> <repo_uid>
rmap quality/* <db_path> <repo_uid> [options]
rmap declare/* <db_path> <repo_uid> [options]
```

These are primarily write operations or governance commands that will be
migrated in future slices.

## Command-Specific Contracts

### `index` / `refresh` — Contract Indexing Summary

When contract files (e.g., `.proto`) are present, the indexing output includes
a contract summary line:

```
indexed 14043 files, 257045 nodes, 210503 edges (1019158 unresolved) → snapshot_uid
  contracts: 81 schemas, 4973 elements
```

**Failure reporting:**

If contract files fail to parse:
```
  contracts: 79 schemas, 4500 elements (2 failed)
    FAILED: path/to/bad.proto: parse error at line 5
    FAILED: other.proto: unexpected token
```

If storage fails during contract indexing:
```
  contracts: 81 schemas, 4973 elements (storage error: connection failed)
```

If both parse failures and storage error occur:
```
  contracts: 79 schemas, 4500 elements (2 failed, storage error: connection failed)
    FAILED: path/to/bad.proto: parse error at line 5
    FAILED: other.proto: unexpected token
```

**No contract line** is printed when:
- No contract files exist in the repo
- Contract indexing produced zero schemas

This surfaces `ContractIndexResult` (schemas_indexed, elements_indexed,
parse_failures, storage_error) at the exact point the system knows the truth,
without requiring post-hoc database queries.

### `index` / `refresh` — Generated Code Mapping Summary (CS-2A)

When contract schemas are indexed and Java generated code exists, the indexing
output includes a mapping summary line after the contract summary:

```
indexed 14043 files, 257045 nodes, 210503 edges (1019158 unresolved) → snapshot_uid
  contracts: 81 schemas, 4973 elements
  mappings: 156 persisted (142 high-confidence)
```

**Failure reporting:**

If element query fails (e.g., missing contract_elements table):
```
  mappings: 0 persisted (0 high-confidence) (element query failed)
    element query: no such table: contract_elements
```

If Java symbol query fails:
```
  mappings: 0 persisted (0 high-confidence) (symbol query failed)
    symbol query: query timeout
```

If storage fails during mapping persistence:
```
  mappings: 5 persisted (3 high-confidence) (storage failed)
    storage: disk full
```

If multiple errors occur:
```
  mappings: 0 persisted (0 high-confidence) (symbol query failed, storage failed)
    symbol query: timeout
    storage: connection lost
```

**No mappings line** is printed when:
- No contract schemas were indexed
- No Java symbols exist in the repo
- Zero mappings were produced and no errors occurred

This surfaces `GeneratedCodeMappingResult` (mappings_persisted, high_confidence_count,
element_query_error, symbol_query_error, storage_error) for explicit degradation
reporting rather than silent failure.

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

### `boundaries` — Boundary Interaction Discovery

Query extracted boundary interaction surfaces from C source files. Surfaces
are populated automatically during `rmap index` / `rmap refresh`.

**Scope (Slice 1A):** Local IPC mechanisms only. This slice covers:
- Unix domain sockets (AF_UNIX)
- Named pipes / FIFOs (mkfifo)
- Anonymous pipes (pipe, pipe2)
- POSIX shared memory (shm_open, mmap MAP_SHARED)
- POSIX message queues (mq_open)

**Explicit exclusions (Slice 1A):**
- TCP/UDP sockets (Slice 1B — requires scope heuristics)
- Serial/CAN (Slice 2 — inter_device scope)
- MQTT/ZeroMQ/D-Bus (Slice 3 — library wrappers)
- I2C/SPI/USB (Slice 4 — low-level device protocols)

**Commands:**

```
rmap boundaries list <db_path> <repo_uid> [filters...]
rmap boundaries show <db_path> <repo_uid> <surface_uid>
rmap boundaries summary <db_path> <repo_uid>
```

**List filters:**

| Filter | Values | Description |
|--------|--------|-------------|
| `--kind` | unix_socket, named_pipe, anonymous_pipe, shared_memory, message_queue | Channel mechanism type |
| `--scope` | inter_process, inter_device, unknown | Boundary crossing scope |
| `--direction` | provider, consumer, bidirectional | Role in the interaction |
| `--family` | socket, pipe, shared_memory, message_queue | Protocol family |
| `--file` | path | Exact source file match |
| `--file-prefix` | prefix | Source file path prefix |
| `--symbol` | key | Enclosing symbol stable key (exact match) |

**Output (list):**

```json
{
  "command": "boundaries list",
  "repo": "swupdate",
  "snapshot": "swupdate/2026-04-30T...",
  "results": [
    {
      "surfaceUid": "bi:swupdate:core/network_utils.c:58:13:unix_socket:bidirectional",
      "sourceFile": "core/network_utils.c",
      "lineStart": 58,
      "lineEnd": 58,
      "channelKind": "unix_socket",
      "boundaryScope": "inter_process",
      "direction": "bidirectional",
      "protocolFamily": "socket",
      "protocol": "unix",
      "interactionPattern": "stream",
      "symbolStableKey": "swupdate:core/network_utils.c#listener_create:SYMBOL:FUNCTION",
      "confidence": 0.95,
      "basis": "api_call",
      "channelCount": 1
    }
  ],
  "count": 1,
  "stale": false
}
```

**Output (show):**

```json
{
  "surfaceUid": "bi:swupdate:core/network_utils.c:58:13:unix_socket:bidirectional",
  "snapshotUid": "swupdate/2026-04-30T.../a86b5e96",
  "repoUid": "swupdate",
  "boundaryScope": "inter_process",
  "channelKind": "unix_socket",
  "direction": "bidirectional",
  "protocol": "unix",
  "protocolFamily": "socket",
  "interactionPattern": "stream",
  "endpointLocality": "same_host_named",
  "symbolStableKey": "swupdate:core/network_utils.c#listener_create:SYMBOL:FUNCTION",
  "sourceFile": "core/network_utils.c",
  "lineStart": 58,
  "lineEnd": 58,
  "colStart": 13,
  "colEnd": 38,
  "extractor": "c-ipc:0.1.0",
  "basis": "api_call",
  "confidence": 0.95,
  "evidenceJson": "{ ... binding table match evidence ... }",
  "channels": [
    {
      "channelUid": "ch:bi:swupdate:...:6210e6e330d84b58",
      "channelKind": "unix_socket",
      "channelIdentity": "core/network_utils.c:58"
    }
  ]
}
```

**Output (summary):**

```json
{
  "command": "boundaries summary",
  "repo": "swupdate",
  "snapshot": "swupdate/2026-04-30T...",
  "summary": {
    "totalSurfaces": 14,
    "totalChannels": 14,
    "byChannelKind": [
      { "channelKind": "unix_socket", "count": 5 },
      { "channelKind": "anonymous_pipe", "count": 6 }
    ],
    "byBoundaryScope": [
      { "boundaryScope": "inter_process", "count": 14 }
    ],
    "byDirection": [
      { "direction": "bidirectional", "count": 13 },
      { "direction": "provider", "count": 1 }
    ],
    "byProtocolFamily": [
      { "protocolFamily": "socket", "count": 5 },
      { "protocolFamily": "pipe", "count": 7 }
    ],
    "byBasis": [
      { "basis": "api_call", "count": 14 }
    ],
    "filesWithBoundaries": [
      "core/network_utils.c",
      "core/notifier.c",
      "ipc/network_ipc.c"
    ]
  }
}
```

**Exit codes:**

- 0: success (surfaces found)
- 1: no surfaces found (not an error, just empty)
- 2: runtime error (invalid args, DB error, missing repo/snapshot)

**Architecture notes:**

- Two-level model: surfaces (Level 1) + channel details (Level 2)
- Surfaces capture the architectural relationship (what/where)
- Channel details capture mechanism-specific addressing
- `endpointLocality` distinct from `boundaryScope` — locality is what can
  be determined from the code, scope is the architectural classification
- Shared memory surfaces require dual projection (boundary + state) per
  the design doc, but state projection is not yet implemented

**Design doc:** `docs/design/boundary-interaction-ipc-device.md`

### `contracts` — Contract Schema Discovery

Query extracted contract schemas (protobuf files) and their elements. Schemas
are populated automatically during `rmap index` / `rmap refresh`.

**Scope (CS-1):** Protobuf schema extraction only. This slice covers:
- Proto2 and Proto3 syntax parsing
- Package/namespace extraction
- Message, enum, service, method elements
- Nested message/enum handling
- Import statement tracking (resolution deferred)
- Option extraction (java_package, go_package, etc.)
- Line/column source anchoring

**CS-2A scope (Java generated code mapping):**
- Maps checked-in Java generated protobuf artifacts to schema elements
- Top-level elements only (messages, enums, services)
- Explicit confidence tiers with basis tracking
- Java proto and gRPC file detection

**Explicit exclusions (CS-2A):**
- Field/method-level mapping (future)
- Kotlin/Swift/other language mapping (future)
- Build-output inference (checks repo files only)
- Import resolution across files (future slice)
- gRPC-specific detection (GR-1, GR-2, GR-3)
- Other IDL formats (OpenAPI, GraphQL, Thrift — future)

**Commands:**

```
rmap contracts list <db_path> <repo_uid> [--kind protobuf]
rmap contracts show <db_path> <repo_uid> <file_path>
rmap contracts elements <db_path> <repo_uid> [--kind message|enum|service|method|field] [--file <path>]
rmap contracts usages <db_path> <repo_uid> [--element <element_uid>] [--min-confidence <0.0-1.0>]
```

**List filters:**

| Filter | Values | Description |
|--------|--------|-------------|
| `--kind` | protobuf | Contract schema type (only protobuf supported) |

**Elements filters:**

| Filter | Values | Description |
|--------|--------|-------------|
| `--kind` | message, enum, service, method, field | Element type filter |
| `--file` | path | Exact source file match |

**Output (list):**

```json
{
  "command": "contracts list",
  "repo": "myrepo",
  "snapshot": "myrepo/2026-05-01T.../a86b5e96",
  "snapshot_scope": "full",
  "basis_commit": null,
  "results": [
    {
      "schema_uid": "proto-myrepo:api/v1/user.proto:820819b3",
      "file_path": "api/v1/user.proto",
      "schema_kind": "protobuf",
      "package_name": "api.v1",
      "syntax_version": "proto3",
      "parsed_at": "2026-05-01T10:30:00Z"
    }
  ],
  "count": 1,
  "stale": false
}
```

**Output (show):**

```json
{
  "command": "contracts show",
  "repo": "myrepo",
  "snapshot": "myrepo/2026-05-01T.../a86b5e96",
  "snapshot_scope": "full",
  "basis_commit": null,
  "results": {
    "schema_uid": "proto-myrepo:api/v1/user.proto:820819b3",
    "file_path": "api/v1/user.proto",
    "schema_kind": "protobuf",
    "package_name": "api.v1",
    "syntax_version": "proto3",
    "content_hash": "820819b3...",
    "extractor": "proto-parser:0.1.0",
    "parsed_at": "2026-05-01T10:30:00Z",
    "elements": [
      {
        "element_uid": "elem-proto-myrepo:...:User",
        "element_kind": "message",
        "name": "User",
        "full_name": "api.v1.User",
        "parent_element_uid": null,
        "line_start": 10,
        "line_end": 25,
        "metadata": {
          "fields_count": 5
        }
      },
      {
        "element_uid": "elem-proto-myrepo:...:User.id",
        "element_kind": "field",
        "name": "id",
        "full_name": "api.v1.User.id",
        "parent_element_uid": "elem-proto-myrepo:...:User",
        "line_start": 12,
        "line_end": 12,
        "metadata": {
          "number": 1,
          "label": "optional",
          "type_name": "string",
          "type_kind": "scalar"
        }
      },
      {
        "element_uid": "elem-proto-myrepo:...:UserService",
        "element_kind": "service",
        "name": "UserService",
        "full_name": "api.v1.UserService",
        "parent_element_uid": null,
        "line_start": 30,
        "line_end": 45,
        "metadata": {
          "methods_count": 2
        }
      },
      {
        "element_uid": "elem-proto-myrepo:...:GetUser",
        "element_kind": "method",
        "name": "GetUser",
        "full_name": "api.v1.UserService.GetUser",
        "parent_element_uid": "elem-proto-myrepo:...:UserService",
        "line_start": 32,
        "line_end": 35,
        "metadata": {
          "input_type": "api.v1.GetUserRequest",
          "output_type": "api.v1.GetUserResponse",
          "client_streaming": false,
          "server_streaming": false
        }
      }
    ]
  },
  "count": 1,
  "stale": false
}
```

**Output (elements):**

```json
{
  "command": "contracts elements",
  "repo": "myrepo",
  "snapshot": "myrepo/2026-05-01T.../a86b5e96",
  "snapshot_scope": "full",
  "basis_commit": null,
  "results": [
    {
      "element_uid": "elem-proto-myrepo:...:User",
      "schema_uid": "proto-myrepo:api/v1/user.proto:820819b3",
      "file_path": "api/v1/user.proto",
      "element_kind": "message",
      "name": "User",
      "full_name": "api.v1.User",
      "line_start": 10
    }
  ],
  "count": 1,
  "filter_kind": "message",
  "stale": false
}
```

**Usages filters (CS-2A):**

| Filter | Values | Description |
|--------|--------|-------------|
| `--element` | element_uid | Filter by specific schema element |
| `--min-confidence` | 0.0-1.0 | Minimum confidence threshold (default: 0.0) |

**Output (usages):**

```json
{
  "command": "contracts usages",
  "repo": "hadoop",
  "snapshot": "hadoop/2026-05-01T.../a86b5e96",
  "snapshot_scope": "full",
  "basis_commit": null,
  "results": [
    {
      "mapping_uid": "map-1234...",
      "schema_element_uid": "elem-proto-hadoop:...:RequestHeaderProto",
      "element_name": null,
      "element_full_name": null,
      "generated_symbol_key": "hadoop:proto2-generated/.../ProtobufRpcEngineProtos.java:RequestHeaderProto:CLASS",
      "language": "java",
      "generated_file": "proto2-generated/org/apache/hadoop/ipc/protobuf/ProtobufRpcEngineProtos.java",
      "mapping_basis": "exact_option_match",
      "confidence": 0.95,
      "evidence": {
        "proto_package": "hadoop.common",
        "java_package_option": "org.apache.hadoop.ipc.protobuf",
        "java_outer_classname_option": "ProtobufRpcEngineProtos",
        "java_package_actual": "org.apache.hadoop.ipc.protobuf",
        "java_outer_class_actual": "ProtobufRpcEngineProtos",
        "schema_element_name": "RequestHeaderProto",
        "java_class_name": "RequestHeaderProto"
      }
    }
  ],
  "count": 1,
  "filter_element": null,
  "filter_min_confidence": null,
  "stale": false
}
```

**Mapping basis confidence tiers:**

| Basis | Confidence | Description |
|-------|------------|-------------|
| `exact_option_match` | 0.95 | java_package + java_outer_classname match proto options |
| `option_package_match` | 0.90 | java_package matches, classname follows filename convention |
| `filename_convention` | 0.85 | Generated file pattern + symbol name match |
| `symbol_normalized_match` | 0.75 | Symbol name normalizes to schema element |
| `weak_wrapper_match` | 0.50 | Partial match via outer class wrapper |

**Element metadata by kind:**

| Kind | Metadata fields |
|------|-----------------|
| message | `fields_count`, `oneofs`, `reserved_numbers`, `reserved_names` |
| field | `number`, `label`, `type_name`, `type_kind`, `default_value` |
| enum | `values_count` |
| service | `methods_count` |
| method | `input_type`, `output_type`, `client_streaming`, `server_streaming` |

**Exit codes:**

- 0: success (schemas/elements found)
- 1: usage error
- 2: runtime error (DB error, missing repo/snapshot, schema not found)

**Architecture notes:**

- Dual-pipeline architecture: contract files are indexed in parallel with
  source files under the same snapshot lifecycle
- Contract files are tracked in the file catalog (`tracked_files`,
  `file_versions`) alongside source files
- `files_total` includes both source and contract files
- Parse failures are surfaced via `ContractIndexResult.storage_error` and
  reflected in `file_versions.parse_status`

**Design doc:** `docs/slices/cs-1-protobuf-schema.md`

## JSON-Only Output

`rmap` always produces JSON on stdout. There is no `--json` flag because
JSON is the default and only format.

Human-readable table format is not planned. Agents are the primary consumer.
