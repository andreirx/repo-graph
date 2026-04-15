# Agent Orientation Contract

Status: DESIGN (Rust-41)

## Problem

AI agents using repo-graph currently face:

1. **Command selection burden.** 15+ commands with different argument shapes. The agent must know which command answers its question, compose the right flags, and parse a command-specific response shape.

2. **Raw output volume.** `callers` returns N rows. `dead` returns M rows. `stats` returns per-module metrics. None of these are pre-ranked or synthesized. The agent pays tokens to read, filter, and merge.

3. **Multi-command synthesis.** Understanding "what is src/core and should I touch it" requires: callers + callees + imports + dead + stats + trust + gate + boundary violations. That's 7-8 commands, 7-8 JSON parses, 7-8 shapes to understand.

4. **Low signal density.** Most raw graph output is noise for the agent's actual question. 200 caller rows when the answer is "heavily depended on by adapters and CLI layers."

## Design Principle

One command, one answer, ranked signals, explicit confidence, bounded output.

The agent should be able to orient on a codebase area in 1-3 commands, not 7-8. The output should be directly usable as context for a code decision, not raw material requiring further synthesis.

## Product Decisions (Locked)

1. `rgr` is Rust. `rgr-ts` is legacy TypeScript. No long-lived `rgr-rust` name.

2. Agent commands use repo name. `--db <path>` is an optional escape hatch for non-registered repos, not the normal interface. Normal usage assumes `rgr repo add` was run.

3. JSON is the only output format for agent commands. No `--json` flag. Machine-first.

4. Every response has hard caps per budget tier. Every truncated section includes `truncated: true` and `omitted_count`.

5. Signals are typed machine-stable records with code, rank, category, evidence. A `summary` prose field exists for human convenience, but is not the contract.

6. Follow-up actions are structured objects, not shell command strings.

7. Ambiguous focus resolution is explicit and non-silent.

8. Daemon is deferred. Use-case layer first.

## Commands

### `rgr orient <repo> [focus]`

Primary orientation command. Answers: "What is this area, what matters about it, and what should I be careful about?"

Focus can be:
- Omitted: whole-repo orientation
- A path: `src/core`, `src/adapters/storage`
- A symbol: `StorageConnection`, `evaluateObligation`

### `rgr check <repo>`

Pre-action health check. Answers: "Can I trust this repo state before making changes?"

### `rgr explain <repo> <target>`

Deep dive on a single entity. Answers: "Tell me everything the graph knows about this symbol/file/module."

This is the "I found something in orient, now I need detail" follow-up.

## Focus Resolution

Deterministic precedence, no silent guessing:

1. Exact path match (repo-relative) → FILE or MODULE
2. Exact stable key match → any kind
3. Exact symbol name match (SYMBOL kind, `name` field) → SYMBOL
4. If zero matches: `"focus": { "resolved": false, "input": "...", "reason": "no match" }`
5. If multiple matches at the same precedence level: return a bounded `candidates` array (max 5) instead of guessing

```json
{
  "focus": {
    "input": "serve",
    "resolved": false,
    "reason": "ambiguous",
    "candidates": [
      { "stable_key": "r1:src/core/service.ts:SYMBOL:serve", "file": "src/core/service.ts", "kind": "SYMBOL" },
      { "stable_key": "r1:src/api/handler.ts:SYMBOL:serve", "file": "src/api/handler.ts", "kind": "SYMBOL" }
    ]
  }
}
```

When focus is ambiguous, no signals are emitted. The response is only the candidates list. The agent picks one and re-issues the command.

## Output Schema

All three commands emit the same envelope:

```json
{
  "schema": "rgr.agent.v1",
  "command": "orient",
  "repo": "repo-graph",
  "snapshot": "r1/2024-01-15T.../abc",
  "focus": {
    "input": "src/core",
    "resolved": true,
    "resolved_kind": "module",
    "resolved_key": "repo-graph:src/core:MODULE"
  },
  "confidence": "high",
  "signals": [ ... ],
  "limits": [ ... ],
  "next": [ ... ],
  "truncated": false
}
```

### Signal Record

Each signal is a typed, machine-stable record:

```json
{
  "code": "GATE_FAIL",
  "rank": 1,
  "severity": "high",
  "category": "gate",
  "summary": "Gate fails: 2 obligations failed.",
  "evidence": {
    "fail_count": 2,
    "total_count": 5,
    "failing_obligations": ["REQ-001/obl-1", "REQ-002/obl-3"]
  },
  "source": "gate::evaluate_obligations"
}
```

#### Signal Codes (Enumerated)

Governance:
- `GATE_PASS` — category: gate, severity: low. All obligations pass or waived.
  Evidence: `{ pass_count, waived_count, total_count }`
- `GATE_FAIL` — category: gate, severity: high. At least one obligation fails.
  Evidence: `{ fail_count, total_count, failing_obligations: ["{req_id}/{obl_id}", ...] }`
- `GATE_INCOMPLETE` — category: gate, severity: medium. Missing evidence or unsupported methods.
  Evidence: `{ missing_count, unsupported_count, total_count }`
- `BOUNDARY_VIOLATIONS` — category: boundary, severity: high. Imports cross declared boundaries.
  Evidence: `{ violation_count, top_violations: [{ source_module, target_module }, ...] }`

Trust:
- `TRUST_LOW_RESOLUTION` — category: trust, severity: medium. Call graph recall below threshold.
  Evidence: `{ resolution_rate, resolved_count, total_count }`
- `TRUST_STALE_SNAPSHOT` — category: trust, severity: medium. Repo changed since last index.
  Evidence: `{ snapshot_age_seconds, snapshot_uid }`
- `TRUST_NO_ENRICHMENT` — category: trust, severity: low. No compiler enrichment applied.
  Evidence: `{ enrichment_status }`

Structure:
- `IMPORT_CYCLES` — category: structure, severity: medium. Module-level circular dependencies.
  Evidence: `{ cycle_count, cycles: [{ length, modules: [...] }, ...] }`
- `DEAD_CODE` — category: structure, severity: medium. Unreferenced exported symbols.
  Evidence: `{ dead_count, top_dead: [{ symbol, file, line_count }, ...] }`
- `HIGH_COMPLEXITY` — category: structure, severity: medium. Max cyclomatic complexity above threshold.
  Evidence: `{ max_cc, symbol, file }`
- `HIGH_FAN_OUT` — category: structure, severity: low. Module imports many other modules.
  Evidence: `{ fan_out, module }`
- `HIGH_INSTABILITY` — category: structure, severity: low. Module instability metric near 1.0.
  Evidence: `{ instability, module }`
- `CALLERS_SUMMARY` — category: structure, severity: low. Direct callers of a symbol.
  Evidence: `{ count, top_modules: [{ module, count }, ...] }`
- `CALLEES_SUMMARY` — category: structure, severity: low. Direct callees of a symbol.
  Evidence: `{ count, top_modules: [{ module, count }, ...] }`

Informational:
- `MODULE_SUMMARY` — category: informational, severity: low. File count, symbol count, language.
  Evidence: `{ file_count, symbol_count, languages: [...] }`
- `SNAPSHOT_INFO` — category: informational, severity: low. Snapshot age, scope, basis commit.
  Evidence: `{ snapshot_uid, scope, basis_commit, age_seconds }`

Each code has a fixed `category` and a fixed default `severity`. The ranking algorithm uses severity first, then category order (gate > boundary > trust > structure > informational).

### Limit Record

```json
{
  "code": "IMPORTS_ONE_HOP",
  "summary": "Import chain is one-hop only; transitive imports not traced."
}
```

Limit codes are enumerated and stable. The `summary` is convenience.

### Follow-Up Action

```json
{
  "kind": "explain",
  "repo": "repo-graph",
  "target": "REQ-001/obl-1",
  "reason": "Inspect the failing obligation evidence."
}
```

The agent can construct the CLI command from the structured fields. The response never contains a shell string.

### Budget

```
--budget small   (default)  max 5 signals, max 3 limits, max 3 next
--budget medium             max 15 signals, max 5 limits, max 5 next
--budget large              max 50 signals, max 20 limits, max 10 next
```

All tiers have hard caps. When data is truncated:

```json
{
  "signals": [ ... ],
  "signals_truncated": true,
  "signals_omitted_count": 12,
  "truncated": true
}
```

The top-level `truncated` is true if any section was truncated. Per-section `_truncated` and `_omitted_count` fields appear only when truncation occurred.

### Confidence

Derived from trust report data:

- `high`: call resolution rate > 50%, snapshot is not stale, focus area has low unresolved pressure.
- `medium`: call resolution rate 20-50%, or snapshot is stale, or focus area has significant unresolved edges.
- `low`: call resolution rate < 20%, or no enrichment, or focus area is predominantly unresolved.

## Signal Ranking

Signals are ordered by actionability within their severity tier.

Ranking rules:
1. `high` severity: gate failures, boundary violations with active gate obligations.
2. `medium` severity: trust warnings, import cycles, dead code above size threshold.
3. `low` severity: informational stats, module membership, snapshot metadata.

Within the same severity, category order breaks ties: gate > boundary > trust > structure > informational.

## What Is Forbidden

These waste agent tokens and must not appear in orient/check/explain output:

1. **Raw node arrays.** Never return a list of 200 callers. Summarize in evidence: `{ "count": 47, "top_modules": [{"module": "src/adapters", "count": 23}, ...] }`.
2. **Per-edge detail.** Never return individual import edges. Summarize.
3. **Full file paths in signal summaries.** Use module-relative paths or module names. Full paths in evidence only.
4. **Duplicate schema boilerplate.** One envelope per response.
5. **Unresolved edge lists.** Summarize as a `TRUST_LOW_RESOLUTION` signal.
6. **Empty sections.** Omit signal codes that have no data. Do not emit zero-count signals.

## What orient Aggregates (By Focus Kind)

### Focus: repo (no focus argument)

- Gate outcome (if requirements exist) → `GATE_PASS` / `GATE_FAIL` / `GATE_INCOMPLETE`
- Module-level cycle count → `IMPORT_CYCLES` if > 0
- Trust summary → `TRUST_LOW_RESOLUTION` / `TRUST_STALE_SNAPSHOT` / `TRUST_NO_ENRICHMENT`
- Boundary violations count → `BOUNDARY_VIOLATIONS` if > 0
- Dead-code summary → `DEAD_CODE` with count + top 3 by size
- Module/file counts → `MODULE_SUMMARY`
- Snapshot info → `SNAPSHOT_INFO`

### Focus: module/path

Same as repo, but scoped to the focus module:
- Boundary violations involving this module only
- Gate obligations targeting this module only
- Dead symbols in this module only
- Cycles involving this module only
- Module fan-in/fan-out and instability

### Focus: symbol

- Symbol identity (kind, subtype, file, line)
- Direct callers (count + top 3 by module) → evidence in `CALLERS_SUMMARY` signal
- Direct callees (count + top 3 by module) → evidence in `CALLEES_SUMMARY` signal
- Dead status → `DEAD_CODE` if unreferenced
- Complexity (if measured) → `HIGH_COMPLEXITY` if above threshold
- Module ownership → included in focus resolution
- Boundary participation → `BOUNDARY_VIOLATIONS` if this symbol's module has violations

## What check Aggregates

- Snapshot UID, age, scope → `SNAPSHOT_INFO`
- Stale status → `TRUST_STALE_SNAPSHOT` if repo changed
- Trust summary → `TRUST_LOW_RESOLUTION` / `TRUST_NO_ENRICHMENT`
- Gate outcome → `GATE_PASS` / `GATE_FAIL` / `GATE_INCOMPLETE`
- Cycle count → `IMPORT_CYCLES` if > 0
- Dead symbol count → `DEAD_CODE` (count only, no details)
- Boundary violation count → `BOUNDARY_VIOLATIONS` (count only)

## What explain Aggregates

Everything orient returns for the target's focus kind, plus:
- Full caller list (up to budget limit, grouped by module)
- Full callee list (up to budget limit, grouped by module)
- Import chain (one-hop)
- If file: all symbols in the file with dead status
- If module: all files with test/generated flags
- Coverage and complexity measurements (if available)

Explain uses `medium` budget minimums regardless of `--budget` flag (it is inherently a detail command).

## Existing Rust Surfaces That Feed This

| Agent command | Existing Rust method |
|---|---|
| Gate status | `gate::evaluate_obligations` + `gate::reduce_to_gate_outcome` |
| Boundary violations | `storage.get_active_boundary_declarations` + `storage.find_imports_between_paths` |
| Trust | `trust` crate (already ported) |
| Callers/callees | `storage.find_direct_callers` / `storage.find_direct_callees` |
| Dead code | `storage.find_dead_nodes` |
| Cycles | `storage.find_cycles` |
| Stats | `storage.compute_module_stats` |
| Imports | `storage.find_imports` |
| Snapshot/staleness | `storage.get_latest_snapshot` |
| Measurements | `storage.query_measurements_by_kind` |
| Inferences | `storage.query_inferences_by_kind` |

All of these exist in the Rust storage or rgr crates already.

## Missing Surfaces (Explicitly Omitted from v1)

- Module discovery data (Layer 1 exists in TS storage, not yet queryable from Rust)
- Module dependency edges (not yet derived)
- Boundary provider/consumer facts (TS-only)
- Env/fs operational seams (TS-only)
- Coverage/churn/hotspot import (TS-only; Rust can read but not produce)

These appear as limit codes in the output:
- `MODULE_DATA_UNAVAILABLE`
- `BOUNDARY_PROVIDERS_UNAVAILABLE`
- `SEAMS_UNAVAILABLE`

## How This Runs Inside a Daemon Later

The use-case layer is a set of pure functions. Shipped
shape (post Rust-43A, post P2 fix):

```rust
pub fn orient<S>(
    storage: &S,
    repo_uid: &str,
    focus: Option<&str>,
    budget: Budget,
    now: &str,
) -> Result<OrientResult, OrientError>
where
    S: AgentStorageRead + GateStorageRead + ?Sized;

// Not yet implemented — shape locked for Rust-44+:
pub fn check<S>(
    storage: &S,
    repo_uid: &str,
    now: &str,
) -> Result<CheckResult, CheckError>
where
    S: AgentStorageRead + GateStorageRead + ?Sized;

pub fn explain<S>(
    storage: &S,
    repo_uid: &str,
    target: &str,
    budget: Budget,
    now: &str,
) -> Result<ExplainResult, ExplainError>
where
    S: AgentStorageRead + GateStorageRead + ?Sized;
```

Three contract invariants every caller must honor:

1. **Storage is a policy port, not a concrete adapter.** The
   generic bound `AgentStorageRead + GateStorageRead` is the
   single handle requirement. `StorageConnection` (SQLite) in
   production, a test fake in integration tests, a future
   daemon-side proxy on a socket — all satisfy the bound.
   Callers MUST NOT name `StorageConnection` in these
   signatures; the concrete adapter is wired at the outermost
   composition root only.

2. **`now: &str` is mandatory and clock-explicit.** The agent
   crate is clock-free. Every entry point takes an ISO 8601
   timestamp supplied by the caller. Used internally for
   waiver expiry comparison in the gate aggregator. Passing a
   far-future or far-past sentinel silently distorts waiver
   semantics — the P2 review in Rust-43A identified this
   exact mistake and the fix removed the sentinel. Daemon
   transport MUST read wall-clock time at the socket boundary
   and pass it in; MUST NOT invent a default. CLI reads the
   wall clock in `main.rs` via `utc_now_iso8601()` and passes
   the result in.

3. **Error surface is typed.** Each entry point returns its
   own typed error (`OrientError`; future `CheckError`,
   `ExplainError`). The daemon is expected to serialize these
   to a stable wire format; the CLI prints them to stderr.
   Neither layer invents error strings; the types are the
   contract.

CLI calls these and serializes to JSON. Daemon later calls the
same functions over a socket with the same signatures. No
daemon-specific product semantics. The daemon is a delivery
mechanism.

## CLI Rename Plan

### Immediate (with agent surface ship)
- Rust binary name changes from `rgr-rust` to `rgr` in `Cargo.toml` `[[bin]]`.
- `package.json` bin entry for TS CLI changes from `rgr` to `rgr-ts`.
- CLAUDE.md updated: `rgr` refers to Rust, `rgr-ts` refers to TS.
- Existing structural commands (`callers`, `callees`, etc.) remain under `rgr` as power-user escape hatches.

### Deferred
- Symlink or wrapper script for backward compatibility during transition.
- TS parity test scripts updated to use `rgr-ts`.
- Documentation sweep to replace `rgr-rust` references.

## Implementation Sequence

1. `Rust-41`: This design doc (done).
2. `Rust-42`: `orient` use-case module — pure Rust aggregation returning typed DTO. No CLI. Tested with deterministic fixtures. **(done — repo-level focus only; module/symbol focus deferred to Rust-44/45.)**
3. `Rust-43A`: gate.rs relocation — move gate policy out of the `rgr` binary crate into a new `repo-graph-gate` library crate. Add gate signal coverage (`GATE_PASS` / `GATE_FAIL` / `GATE_INCOMPLETE`) to the orient pipeline. Replace unconditional `GATE_UNAVAILABLE` limit with `GATE_NOT_CONFIGURED`. **(done — see `docs/TECH-DEBT.md` "Gate.rs relocation — CLOSED in Rust-43A" for details.)**
4. `Rust-43B`: `rgr orient` CLI command — parse args, call use case, serialize, exit code.
5. `Rust-43C`: Binary rename: `rgr-rust` → `rgr`, `rgr` (TS) → `rgr-ts`. May or may not ship with Rust-43B depending on diff size.
6. `Rust-44`: `check` use-case + CLI. Module/path focus for `orient`.
7. `Rust-45`: `explain` use-case + CLI. Symbol focus for `orient`.
8. Evaluate: does the agent surface cover 80% of agent needs?
9. Daemon: host the same use-case functions over a socket.

### Rust-43A implementation notes (as shipped)

- New crate: `repo-graph-gate` at `rust/crates/gate/`. Policy
  layer. Depends on `serde` and `serde_json` only — no
  dependency on `repo-graph-storage`, `repo-graph-agent`, or
  any adapter crate.
- Two-layer shape:
    * `compute(input: GateInput) -> GateReport` — pure.
    * `assemble(storage: &impl GateStorageRead, repo_uid,
      snapshot_uid, mode, now) -> Result<GateReport, GateError>`
      and `assemble_from_requirements(...)` for callers (like
      the agent aggregator) that have already fetched
      requirements.
- Port: `GateStorageRead` with agent-style concrete
  `GateStorageError`. Implemented on `StorageConnection` in
  `rust/crates/storage/src/gate_impl.rs`.
- Dependency edges: `agent → gate`, `storage → gate`,
  `rgr → gate`. No reverse edges.
- `rgr/src/main.rs::run_gate` now delegates the whole
  pipeline to `repo_graph_gate::assemble` and formats
  `GateError` via a `format_gate_error` helper so the
  pre-relocation stderr wording is preserved. Every existing
  `gate_command.rs` test passes unchanged.
- `repo-graph-agent`:
    * `orient_repo` widened to
      `S: AgentStorageRead + GateStorageRead`.
    * New `aggregators::gate` module consumes `GateReport`
      and emits `GATE_PASS` / `GATE_FAIL` / `GATE_INCOMPLETE`
      or the `GATE_NOT_CONFIGURED` limit when no requirements
      exist.
    * `LimitCode::GateUnavailable` removed; replaced by
      `LimitCode::GateNotConfigured`. Summary text updated.
    * `orient` always calls gate in `GateMode::Default`.
      Strict/advisory are CLI gate surface concerns only.
    * Gate signal evidence variants added to
      `SignalEvidence`: `GatePassEvidence`, `GateFailEvidence`,
      `GateIncompleteEvidence`. Named constructors match the
      pattern of other signals.
    * `SourceRef::GateAssemble` added and stringifies as
      `"gate::assemble"` in the JSON output.
- PASS-not-waivable divergence (Rust-25) preserved.
- Malformed measurement / inference error strings preserved
  verbatim via pre-parse helpers in `gate/src/assemble.rs`.
- `orient()` takes `now: &str` as a required final parameter
  (P2 fix, post Rust-43A). The agent crate never touches the
  system clock; callers supply the ISO 8601 timestamp used
  for waiver expiry comparison inside
  `GateStorageRead::find_waivers`. An earlier draft used a
  sentinel constant that silently distorted finite-expiry
  waiver semantics; the sentinel was removed. Regression
  tests in `orient_repo_gate.rs` pin the three expiry paths
  (finite active, finite expired, perpetual).
- Tests: 9 new agent integration tests in
  `rust/crates/agent/tests/orient_repo_gate.rs`. 23 unit
  tests in `repo-graph-gate` (14 compute, 5 assemble, 4 other).
  Full workspace: 1099 tests passing, 0 failures.

### Rust-42 implementation notes (as shipped)

- Crate: `repo-graph-agent` at `rust/crates/agent/`. Pure
  policy layer — no dependency on `repo-graph-storage`,
  `repo-graph-trust`, or any adapter crate.
- Port: `AgentStorageRead` trait defined in
  `rust/crates/agent/src/storage_port.rs`. Implemented on
  `StorageConnection` in `rust/crates/storage/src/agent_impl.rs`.
  Dependency direction is adapter → policy (same shape as
  `TrustStorageRead` / `repo-graph-trust`).
- Focus: repo-level only. Calling `orient(_, _, Some(_), _)`
  returns `OrientError::FocusNotImplementedYet` — explicit
  slice-limitation error, NOT a `focus.resolved = false`
  domain failure. Contract reserves the latter shape for
  ambiguous/no-match focus resolution.
- Signal evidence: typed enum, manual `Serialize` impl, no
  `serde_json::Value` escape hatch, no `#[serde(untagged)]`.
- Per-code named constructors on `Signal` (no public raw
  record constructor); category and severity are looked up
  from `SignalCode::descriptor()` so aggregators cannot
  override them.
- Rust-42 limit codes emitted unconditionally:
  `GATE_UNAVAILABLE`, `MODULE_DATA_UNAVAILABLE`,
  `COMPLEXITY_UNAVAILABLE`.
- `TRUST_STALE_SNAPSHOT` wording is deliberate: describes the
  storage-internal stale parse-state, NOT a filesystem or git
  comparison. See `docs/TECH-DEBT.md` ("Staleness wording
  discipline").
- `DEAD_CODE_EMIT_THRESHOLD = 1`, surfaced as a `const` at the
  top of the `dead_code` aggregator for single-site tuning.
- Tests: 36 unit tests inside `repo-graph-agent`, 44
  integration tests in `rust/crates/agent/tests/` driven by an
  in-memory `FakeAgentStorage` that implements the port, plus
  7 adapter-side integration tests in
  `rust/crates/storage/tests/agent_impl.rs` that exercise the
  real SQLite path (including one end-to-end smoke test that
  runs the full `orient` pipeline through a `StorageConnection`).

## Resolved Questions

1. `orient` with no focus defaults to repo-level summary. No `--repo` flag needed.
2. `explain` supports module-level targets (modules, files, and symbols).
3. `next` section uses structured actions `{ kind, repo, target, reason }`, not shell strings.
4. Three commands is the initial set. A fourth can be added if evaluation reveals a gap.
5. Binary rename happens with the agent surface ship (Rust-43), not later.
