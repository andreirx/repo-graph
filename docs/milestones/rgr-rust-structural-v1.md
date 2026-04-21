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

## Post-v1 additions (RS-MG series)

Module graph support shipped after v1 freeze. Slice IDs match Cargo.toml
headers and inline comments in the codebase (authoritative source).

| Slice | Content |
|-------|---------|
| RS-MG-1 | Module candidate CRUD (storage/crud/module_candidates.rs) |
| RS-MG-2 | Module edge derivation: pure policy (classification/module_edges.rs) + storage queries (storage/crud/module_edges_support.rs) |
| RS-MG-3 | Boundary declaration loading and parsing (storage/crud/declarations.rs, classification/boundary_parser.rs) |
| RS-MG-4 | Boundary violation evaluator — pure policy (classification/boundary_evaluator.rs) |
| RS-MG-5 | `modules deps` command |
| RS-MG-6 | `modules violations` command |
| RS-MG-7 | `modules boundary` declaration command |
| RS-MG-8 | `module_violations` gate method (gate crate + storage adapter) |
| RS-MG-9 | `modules list` command |
| RS-MG-10 | Unified `violations` command (declared + discovered-module) |
| RS-MG-11 | `modules files` command |
| RS-MG-12a | module_rollup.rs — pure rollup aggregator (classification crate) |
| RS-MG-12b | Per-module rollups in `modules list` output |
| RS-MG-13a | weighted_neighbors.rs — weighted neighbor aggregation (classification crate) |
| RS-MG-13b | `modules show` command — module briefing with weighted neighbors and inline violations |

## Post-v1 additions (RS-MS series)

Measurement support. Query-time by default, no automatic persistence.
Git is the authoritative history source. TS implementation is reference, not spec.

| Slice | Content |
|-------|---------|
| RS-MS-0 | Architecture decision: query-time default, explicit anchor opt-in later |
| RS-MS-1 | `repo-graph-git` crate: `get_file_churn(repo_path, window)` |
| RS-MS-2 | `rmap churn` command: query-time per-file git churn for indexed files |
| RS-MS-3a | `hotspot_scorer.rs` in classification crate: pure scorer support module |
| RS-MS-3b | `rmap hotspots` command: churn × complexity (lines_changed * sum_complexity) |

Locked sequence (review before continuing):
- RS-MS-4: `rmap risk` command (if hotspots proves useful)
- Explicit anchor option for gate integration (deferred)

Commands added:
```
rmap modules list <db_path> <repo_uid>
rmap modules show <db_path> <repo_uid> <module>
rmap modules files <db_path> <repo_uid> <module>
rmap modules deps <db_path> <repo_uid> [module] [--outbound|--inbound]
rmap modules violations <db_path> <repo_uid>
rmap modules boundary <db_path> <repo_uid> <source> --forbids <target> [--reason <text>]
rmap violations <db_path> <repo_uid>
```

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
- Governance commands (gate, evidence, declare, etc.): partially ported (violations shipped)
- Measurement commands (churn, hotspots, risk, coverage): in progress (RS-MS series)
- Catalog commands (surfaces, docs): not ported (modules shipped)

## Next phase candidates

After this freeze, the recommended sequence is:
1. Governance command subset (arch violations, gate)
2. Measurement commands (metrics)
3. Parameter widening (all 18 edge types, --depth, --max-depth)
4. Non-CLI infrastructure (enrichment port, new extractors)

No timeline commitment. This freeze is stable.
