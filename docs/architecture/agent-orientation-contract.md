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

1. **Two CLI binaries.** `rmap` is the Rust CLI — the primary binary for agent-facing commands and all new product surface. `rgr` is the TypeScript CLI — the original implementation, kept for TS-side parity checks and features not yet ported to Rust. `rgr` is NOT being renamed to `rgr-ts`. The two names are the permanent product names, not a migration waypoint.

2. Agent commands use repo name. `--db <path>` is an optional escape hatch for non-registered repos, not the normal interface. Normal usage assumes `rmap repo add` was run (repo registry not yet implemented — see TECH-DEBT.md).

3. JSON is the only output format for agent commands. No `--json` flag. Machine-first.

4. Every response has hard caps per budget tier. Every truncated section includes `truncated: true` and `omitted_count`.

5. Signals are typed machine-stable records with code, rank, category, evidence. A `summary` prose field exists for human convenience, but is not the contract.

6. Follow-up actions are structured objects, not shell command strings.

7. Ambiguous focus resolution is explicit and non-silent.

8. Daemon is deferred. Use-case layer first.

## Commands

### `rmap orient <repo> [focus]`

Primary orientation command. Answers: "What is this area, what matters about it, and what should I be careful about?"

Focus can be:
- Omitted: whole-repo orientation
- A path: `src/core`, `src/adapters/storage`
- A symbol: `StorageConnection`, `evaluateObligation`

### `rmap check <repo>`

Pre-action health check. Answers: "Can I trust this repo state before making changes?"

### `rmap explain <repo> <target>`

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
- `TRUST_NO_ENRICHMENT` — category: trust, severity: low. Enrichment phase did not run
  on a snapshot with eligible work (`EnrichmentState::NotRun`). Does NOT fire when the
  phase ran but resolved nothing (`Ran`), or when no eligible samples exist
  (`NotApplicable`). Evidence: `{ enrichment_eligible, enrichment_enriched }`

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

## CLI Naming (Settled)

**This is not a migration plan. This is the permanent naming
decision.**

| Binary | Language | Role | Status |
|---|---|---|---|
| `rmap` | Rust | Primary. All new product surface ships here. Agent-facing commands (`orient`, `check`, `explain`), structural graph queries, governance commands. | Active, shipping. |
| `rgr` | TypeScript | Original implementation. Used for TS-side parity checks and features not yet ported to Rust. | Maintained, not renamed. |

There is no `rgr-ts` rename planned. `rgr` stays `rgr`. The
two binaries have distinct names so there is no ambiguity
about which one an agent or script is invoking. Future daemon
transport targets the `rmap` binary name.

History: the Rust binary was originally named `rgr-rust`
(Rust-7B through Rust-43B) with a plan to rename to `rgr` and
push the TS CLI to `rgr-ts`. That plan was abandoned in
Rust-43C in favor of the permanent `rmap` name, which has a
stronger product mnemonic ("repo map") and eliminates the
TS-side rename entirely.

## Implementation Sequence

1. `Rust-41`: This design doc (done).
2. `Rust-42`: `orient` use-case module — pure Rust aggregation returning typed DTO. No CLI. Tested with deterministic fixtures. **(done — repo-level focus only; module/symbol focus deferred to Rust-44/45.)**
3. `Rust-43A`: gate.rs relocation — move gate policy out of the `rgr` binary crate into a new `repo-graph-gate` library crate. Add gate signal coverage (`GATE_PASS` / `GATE_FAIL` / `GATE_INCOMPLETE`) to the orient pipeline. Replace unconditional `GATE_UNAVAILABLE` limit with `GATE_NOT_CONFIGURED`. **(done — see `docs/TECH-DEBT.md` "Gate.rs relocation — CLOSED in Rust-43A" for details.)**
4. `Rust-43B`: `rmap orient` CLI command — parse args, call use case, serialize, exit code. **(done — shipped with the `<db_path> <repo_uid>` positional shape; see "Rust-43B implementation notes" below. Repo-name invocation is deferred to a registry slice because the Rust CLI has no repo registry yet.)**
5. `Rust-43 F1/F2/F3 fix slice`: semantic correctness fix for the spike-discovered defects in the orient contract (DEAD_CODE gated on trust reliability, `EnrichmentState` three-state model, typed reliability axes in `AgentTrustSummary`). **(done — see "Rust-43 F1/F2/F3 fix slice implementation notes" below and `docs/spikes/2026-04-15-orient-on-repo-graph.md` for the before/after verification.)**
6. `Rust-43C`: Binary rename: `rgr-rust` → `rmap`. **(done — binary, test harnesses, docs, and all references updated.)**
6. `Rust-44`: `check` use-case + CLI. Module/path focus for `orient`.
7. `Rust-45`: `explain` use-case + CLI. Symbol focus for `orient`.
8. Evaluate: does the agent surface cover 80% of agent needs?
9. Daemon: host the same use-case functions over a socket.

### Rust-43 F1/F2/F3 fix slice implementation notes (as shipped)

- **Trust as authority.** The trust crate already computes
  `reliability.dead_code` as a composite axis that combines
  call-graph reliability, missing entrypoint declarations,
  registry pattern suspicion, and framework-heavy suspicion.
  The agent crate does NOT re-derive these rules. The
  dead-code aggregator reads
  `trust.dead_code_reliability.level == High` as a
  single-site gate.
- **New agent DTOs:**
  - `AgentReliabilityLevel { Low, Medium, High }`
  - `AgentReliabilityAxis { level, reasons: Vec<String> }`
  - `EnrichmentState { Ran, NotApplicable, NotRun }`
- **`AgentTrustSummary` widening:** `call_graph_reliability`,
  `dead_code_reliability`, `enrichment_state`. The old
  `enrichment_applied: bool` was removed (it collapsed
  "phase never ran" and "phase ran with nothing to do" into
  a single boolean, which was the F2 bug).
- **`Limit` widening:** new optional `reasons: Vec<String>`
  field, serialized only when non-empty. Pre-existing
  limits continue to serialize without the field.
- **New limit code:** `LimitCode::DeadCodeUnreliable`. Fires
  when `trust.dead_code_reliability.level != High`. Carries
  the trust layer's reason vector verbatim. Falls back to
  a stable `"dead_code_reliability_not_high"` string when
  trust reports a non-High level with an empty reason vector
  (defensive).
- **Dead-code aggregator gate:** signature gained
  `trust: &AgentTrustSummary`. If the level is not High,
  emit the limit and no signal. Otherwise emit DEAD_CODE as
  before. The agent crate's `orient_repo` passes the trust
  summary by reference; the borrow checker accepts this
  because `AggregatorOutput` merges happen via partial move
  on `trust_result.output`.
- **`TRUST_NO_ENRICHMENT` rule update:** fires iff
  `enrichment_state == NotRun`. Previously the rule was
  `!applied && eligible > 0`, which was unreachable in the
  new storage mapping (`eligible > 0 → Ran`).
- **Confidence rule update:** `NotRun` degrades high→medium
  on the enrichment axis. `Ran` and `NotApplicable` are
  silent on this axis. `stale` still degrades
  independently.
- **Storage adapter mapping:**
  - `Option<EnrichmentStatus>` + `enrichment_eligible_count`
    (the latter is the spike-follow-up P2 disambiguator —
    a `#[serde(skip)]` field on `TrustReport` that exposes
    the `CallsObjMethodNeedsTypeInfo` sample count even
    when `enrichment_status == None`):
    - `Some(es)` → `Ran` (phase executed on at least one
      eligible sample; `enriched` count may be zero)
    - `None` && `enrichment_eligible_count == 0` →
      `NotApplicable` (no eligible samples at all)
    - `None` && `enrichment_eligible_count > 0` → `NotRun`
      (eligible samples existed but the phase never ran)
    - The `Some(es) where es.eligible == 0` branch is
      defensive and currently unreachable in trust code —
      kept in the match for forward compatibility
  - **Why both inputs are required.** `Option<EnrichmentStatus>`
    alone collapses "no eligible samples" and "phase did
    not run on eligible samples" into a single `None`. The
    P2 review identified this as a false-positive source
    for `TRUST_NO_ENRICHMENT` on snapshots with nothing to
    enrich. Future CLI/daemon/adapter work MUST keep the
    counter in the mapping; reverting to the
    `Option`-only shape reintroduces the bug.
  - `reliability.call_graph` and `reliability.dead_code`
    project directly via `trust::ReliabilityLevel` →
    `AgentReliabilityLevel` enum mapping. Reason strings
    pass through unchanged.
- **Spike validation.** Re-ran
  `rmap orient /tmp/rgspike.db repo-graph --budget large`
  against the same database used in the spike. Before:
  DEAD_CODE ranked #1 with 2683 dead symbols. After:
  DEAD_CODE suppressed, `DEAD_CODE_UNRELIABLE` limit fires
  with the trust reason `missing_entrypoint_declarations`,
  IMPORT_CYCLES takes rank #1, TRUST_NO_ENRICHMENT now
  visible at rank #2. The output is dramatically more
  honest and more actionable. See
  `docs/spikes/2026-04-15-orient-on-repo-graph.md` for the
  full before/after diff.
- **Test totals:** 7 new integration tests in
  `rust/crates/agent/tests/orient_repo_dead_code_reliability.rs`.
  Existing confidence tests rewritten around the new
  three-state enum. Storage smoke test asserts the
  empty-snapshot reliability gate. Full workspace: 1128
  tests passing, 0 failures.

### Rust-43B implementation notes (as shipped)

- New CLI command: `rmap orient <db_path> <repo_uid>
  [--budget small|medium|large] [--focus <string>]`.
  Implemented in `rust/crates/rgr/src/main.rs::run_orient`.
- **Positional shape divergence from the contract.** The
  contract's target command is `rmap orient <repo_name>` with
  `--db <path>` as an optional escape hatch for non-registered
  repos. Rust has no repo registry yet. Until a registry slice
  lands, `orient` takes the same `<db_path> <repo_uid>`
  positional pair as every other Rust CLI command. This
  divergence is temporary and tracked in `docs/TECH-DEBT.md`.
- **Budget parsing is strict.** Default `small`. Accepted
  values `small | medium | large`. No aliases. No
  case-insensitive matching. Unknown value / missing value /
  repeated flag all exit 1 (usage error).
- **`--focus` accepts any string and exits 2.** The parser
  recognizes the flag and propagates `OrientError::FocusNotImplementedYet`
  as a runtime error. Exit 2 is correct — the command is
  syntactically valid, only the runtime capability is not yet
  implemented. Rust-44/45 will implement the use case without
  touching the CLI grammar.
- **Clock discipline.** The CLI reads the system clock at the
  outermost boundary via `utc_now_iso8601()` (the existing
  helper in `main.rs`) and passes the ISO 8601 string into
  `repo_graph_agent::orient` as `now`. The agent crate remains
  clock-free. Do not invent another clock helper.
- **No `--json` flag.** Output is always JSON, pretty-printed.
- **Exit codes**: 0 success, 1 usage error, 2 runtime error
  (missing DB, missing repo, missing snapshot,
  focus-not-implemented, storage failure, serialization
  failure).
- **Binary name.** Originally shipped as `rgr-rust`. Renamed
  to `rmap` in Rust-43C.
- **No repo registry.** `rmap` still has no
  `repo add` / `repo list` equivalent. Users invoke orient
  with the same `<db_path> <repo_uid>` pair they use for
  `cycles`, `dead`, `gate`, etc.
- **Test coverage.** 16 integration tests in
  `rust/crates/rgr/tests/orient_command.rs`. One end-to-end
  smoke test indexes a tiny TS fixture via
  `repo_graph_repo_index::compose::index_path` and asserts
  the output JSON carries `schema = "rgr.agent.v1"`,
  `command = "orient"`, repo-level focus, the always-emitted
  `SNAPSHOT_INFO` and `MODULE_SUMMARY` signals, and the
  `GATE_NOT_CONFIGURED` limit (because the fixture has no
  active requirements). Full workspace: 1118 tests, 0
  failures.

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
5. Binary naming settled in Rust-43C: Rust CLI is `rmap`, TS CLI stays `rgr`. No further rename planned. See "CLI Naming (Settled)" above.
