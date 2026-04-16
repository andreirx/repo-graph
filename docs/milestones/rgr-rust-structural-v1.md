# rmap structural CLI v1

Milestone freeze for the Rust structural analysis CLI surface.
Accumulated through Rust-7B to Rust-20 (14 slices).

## What shipped

A standalone Rust binary (`rmap`, crate `repo-graph-rgr`) that
indexes TypeScript/JavaScript codebases into SQLite and provides
10 structural graph query commands. JSON-only output. Deterministic.
Cross-runtime interop proven: databases written by Rust are consumable
by the existing TS product layer.

## Commands

Canonical command inventory. This is the single source of truth;
`CLAUDE.md` mirrors this list.

```
rmap index   <repo_path> <db_path>                               # Full index to SQLite
rmap refresh <repo_path> <db_path>                               # Delta refresh (incremental)
rmap trust   <db_path> <repo_uid>                                # Extraction trust report
rmap callers <db_path> <repo_uid> <symbol> [--edge-types <t>]    # Direct callers (one hop)
rmap callees <db_path> <repo_uid> <symbol> [--edge-types <t>]    # Direct callees (one hop)
rmap path    <db_path> <repo_uid> <from> <to>                    # Shortest path (BFS)
rmap imports <db_path> <repo_uid> <file_path>                    # File import chain (one hop)
rmap dead    <db_path> <repo_uid> [kind]                         # Unreferenced nodes
rmap cycles  <db_path> <repo_uid>                                # Module-level IMPORTS cycles
rmap stats   <db_path> <repo_uid>                                # Module structural metrics
```

## Contracts

### Exit codes
- 0: success (JSON on stdout)
- 1: usage error (stderr only)
- 2: runtime/storage/snapshot failure (stderr only)

### JSON envelope
Read-side commands (callers, callees, path, imports, dead, cycles,
stats) emit a TS-compatible QueryResult envelope:

```json
{
  "command": "graph <cmd>",
  "repo": "<repo name>",
  "snapshot": "<snapshot_uid>",
  "snapshot_scope": "full" | "incremental",
  "basis_commit": "<hash>" | null,
  "results": [...],
  "count": N,
  "stale": true | false
}
```

### Symbol resolution
Callers, callees, and path use exact SYMBOL-only resolution:
stable_key, then qualified_name, then name. No fuzzy matching.
SYMBOL kind only (FILE/MODULE rejected).

### Edge-type filter
Callers and callees accept `--edge-types CALLS,INSTANTIATES`.
Default (no flag): CALLS only. Unknown tokens: usage error.

### Path defaults
CALLS + IMPORTS, max depth 8. Both hardcoded (no flags).

### Imports resolution
File path only (constructs `{repo}:{path}:FILE` stable key).
No module/symbol fallback.

### Dead node exclusions
Three layers (mirrors TS exactly):
1. Incoming reference edges (8 edge types)
2. Declared entrypoints (declarations table)
3. Framework-liveness inferences (5 inference kinds)

### Cycles algorithm
Simple cycle enumeration via recursive CTE on MODULE-level
IMPORTS edges. Canonicalized by lexicographic UID rotation.
Matches TS `findCycles` semantics.

### Stats metrics
Per-module: fan_in, fan_out, instability, abstractness,
distance_from_main_sequence, file_count, symbol_count.
Rounded to 2 decimal places. Modules with file_count=0 excluded.

## Slice inventory

| Slice | Content |
|-------|---------|
| Rust-7B | CLI binary + interop test |
| Rust-8 | trust command |
| Rust-9 | refresh command |
| Rust-10 | callers command + resolve_symbol |
| Rust-11 | callees command |
| Rust-12 | dead command |
| Rust-13 | cycles command |
| Rust-14 | stats command |
| Rust-15 | QueryResult envelope standardization |
| Rust-16 | Consolidation: docs, contract tests, interop expansion |
| Rust-17 | --edge-types filter for callers/callees |
| Rust-18 | imports command |
| Rust-19 | path command |
| Rust-20 | Interop hardening: cross-runtime proof for all surfaces |

## Test coverage

- 38 Rust workspace test suites, 0 failures
- 19 cross-runtime interop tests (TS reading Rust-written DBs)
- 5 Rust-side envelope contract tests
- 10 edge-type filter tests
- Per-command test matrices: 5-10 tests each covering usage errors,
  missing DB, repo not found, symbol not found, exact results,
  envelope shape

## Deferred (not shipped in v1)

See `docs/TECH-DEBT.md` section "Rust CLI" for the full list.
Summary:

- `--edge-types` accepts only CALLS and INSTANTIATES (not all 18)
- `--min-lines` filter on dead: not ported
- `--depth` on imports: not ported (one-hop only)
- `--max-depth` and `--edge-types` on path: not ported (hardcoded)
- `graph metrics` command: not ported
- `graph versions` command: not ported
- Table output format: not ported (JSON only)
- Module/symbol fallback on imports: not ported (file paths only)
- Governance commands (gate, evidence, violations, declare, etc.): not ported
- Measurement commands (churn, hotspots, risk, coverage): not ported
- Catalog commands (modules, surfaces, docs, boundary): not ported

## Next phase candidates

After this freeze, the recommended sequence is:
1. Governance command subset (arch violations, gate)
2. Measurement commands (metrics)
3. Parameter widening (all 18 edge types, --depth, --max-depth)
4. Non-CLI infrastructure (enrichment port, new extractors)

No timeline commitment. This freeze is stable.
