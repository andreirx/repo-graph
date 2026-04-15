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

The use-case layer is a set of pure functions:

```rust
fn orient(storage: &StorageConnection, repo_uid: &str, focus: Option<&str>, budget: Budget) -> OrientResult
fn check(storage: &StorageConnection, repo_uid: &str) -> CheckResult
fn explain(storage: &StorageConnection, repo_uid: &str, target: &str, budget: Budget) -> ExplainResult
```

CLI calls these and serializes to JSON.
Daemon later calls the same functions over a socket.
No daemon-specific product semantics. The daemon is a delivery mechanism.

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
2. `Rust-42`: `orient` use-case module — pure Rust aggregation returning typed DTO. No CLI. Tested with deterministic fixtures.
3. `Rust-43`: `rgr orient` CLI command — parse args, call use case, serialize, exit code.
4. `Rust-44`: `check` use-case + CLI.
5. `Rust-45`: `explain` use-case + CLI.
6. Binary rename: `rgr-rust` → `rgr`, `rgr` (TS) → `rgr-ts`.
7. Evaluate: does the agent surface cover 80% of agent needs?
8. Daemon: host the same use-case functions over a socket.

## Resolved Questions

1. `orient` with no focus defaults to repo-level summary. No `--repo` flag needed.
2. `explain` supports module-level targets (modules, files, and symbols).
3. `next` section uses structured actions `{ kind, repo, target, reason }`, not shell strings.
4. Three commands is the initial set. A fourth can be added if evaluation reveals a gap.
5. Binary rename happens with the agent surface ship (Rust-43), not later.
