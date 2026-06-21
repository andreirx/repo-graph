# SQLITE-RAW-DECOMMISSION-READINESS-2: Transition Audit (recompute) (Stage D)

Slice ID: SQLITE-RAW-DECOMMISSION-READINESS-2
Status: **AUDIT — evidence map. No code, no table deletion, no migration.** Recomputes
SQLITE-RAW-DECOMMISSION-READINESS-1 after the QUERY-MIGRATION / PATH / CYCLES / IMPORTS slices.
Track: Stage D. Gates any future `SQLITE-RAW-DECOMMISSION-1`.

## Purpose
```text
Recompute what still blocks `nodes`/`edges` retirement after: callers/callees default auto, path default
auto, the file-import cycle surface, module cycles still SQLite, and imports entering the IR only as
captured resolved-relative FILE imports. Re-separate RAW-GRAPH (nodes/edges) retirement from SQLite
(whole-DB) retirement.
```

## Verdict (headline)
```text
`nodes`/`edges` are NOT ready to retire. Progress is real but does not reduce the raw-graph dependency to
zero: callers/callees/path now DEFAULT to LiveGraph, but their SQLite FALLBACK still reads nodes/edges, and
five commands (cycles-module, imports, stats, orient/explain/check) are SQLite-only by default. The
file-import cycle surface is a NEW, different-semantics query; it does not migrate `rmap cycles`. "Migrated
to auto" is NOT "freed of nodes/edges": a default flip moved the PREFERRED engine, not the DEPENDENCY.
```

## Delta since READINESS-1 (what the migrations changed — and did not)
```text
+ callers/callees: default Sqlite -> AUTO (QUERY-MIGRATION-CLI-1). LiveGraph when Exact+Fresh+TS-only.
+ path:            default Sqlite -> AUTO (PATH-LIVEGRAPH-DEFAULT-1). LiveGraph when Exact+Fresh+complete+
                   display-metadata. (READINESS-1 listed path as Sqlite-default; that is now stale.)
+ cycles:          gained an EXPLICIT `--engine livegraph --kind file-import` FILE-import surface
                   (CYCLES-LIVEGRAPH-1/-CLI-1). DIFFERENT graph; NOT a migration of the SQLite MODULE default.
+ imports (data):  module-import facts now ENTER PartitionIr as captured resolved-relative FILE imports
                   (IMPORTS-MODULE-INGEST-1). BUT the `rmap imports` COMMAND still reads SQLite edges.
~ unchanged:       imports/stats/orient/explain/check are SQLite-only. nodes/edges hold ALL languages;
                   LiveGraph holds TS-package PartitionIr only. The 31 non-graph tables have no LiveGraph
                   representation (READINESS-1 §1; still true).
- NOT changed:     the SQLite FALLBACK in the auto commands STILL reads nodes/edges (the dependency the
                   audit cares about is the FALLBACK, not the default preference).
```

## Audit questions (answered, OBSERVED via dispatch + storage + livegraph_feed)

### Q1 — which DEFAULT shipped commands still read SQLite `nodes`/`edges`?
```text
ALL graph commands, on at least one default path:
  callers / callees / path -> via the SQLite FALLBACK of `auto` (read whenever LiveGraph is not
                              Exact+Fresh+complete+TS — i.e. unavailable / partial / stale / non-TS / no
                              display-metadata, and for every repo never refreshed into the LiveGraph).
  cycles (module)          -> ALWAYS (default engine sqlite, kind module): find_cycles IMPORTS/MODULE SCC.
  imports                  -> ALWAYS (find_imports: edges type=IMPORTS + nodes).
  stats                    -> ALWAYS (compute_module_stats: nodes + edges OWNS/IMPORTS + measurements).
  orient / explain / check -> ALWAYS (agent reads nodes/edges; check also reads `declarations`).
Non-graph commands also read nodes/edges (trust, gate, docs, contracts, inferences, deps, surfaces,
boundaries, modules) — they are NOT migration targets and pin nodes/edges independently.
```

### Q2 — which commands have LiveGraph equivalents with default auto?
```text
callers, callees, path  -> default AUTO (LiveGraph-first, labelled SQLite fallback). SAME question.
cycles                  -> has a LiveGraph path but it is EXPLICIT (--engine livegraph --kind file-import),
                           NOT auto, and a DIFFERENT question (file-import vs module-import).
No other command has any LiveGraph path.
```

### Q3 — which commands are DIFFERENT semantics (cannot count toward migration)?
```text
- file-import cycles (LiveGraph) vs module-import cycles (`rmap cycles`): DIFFERENT graphs. The new surface
  adds capability; it does NOT retire the SQLite module-cycle dependency. (PATH-CYCLES-LIVEGRAPH-2.)
- `rmap imports` (SQLite FILE->FILE IMPORTS edges, all forms) vs the IR's captured imports (resolved-
  relative, intra-partition, TS only): NOT equivalent. The IR import graph is a STRICT SUBSET; it cannot
  back `rmap imports` without the completeness gap closed.
```

### Q4 — which tables remain authoritative for non-graph product features?
```text
Unchanged from READINESS-1 §1/§3: `repos`, `declarations` (authoritative); `snapshots`,
`schema_migrations`, `module_discovery_diagnostics` (operational); and the boundary / surface / contract /
measurement / module / semantic / status / behavioral / return-fate families (derived but NO LiveGraph
representation). These pin SQLite regardless of nodes/edges. nodes/edges themselves remain the ALL-LANGUAGE
raw graph that powers the graph commands AND the non-graph commands above.
```

### Q5 — is `nodes`/`edges` decommission still blocked by…?
```text
rmap cycles (module-import)   -> BLOCKS (SQLite-only default; the file-import surface is a different graph).
imports                       -> BLOCKS (SQLite-only command; the IR captured imports are a subset, and are
                                 consumed only by file_import_cycles, not by `rmap imports`).
stats                         -> BLOCKS (SQLite-only; nodes/edges + measurements).
orient                        -> BLOCKS (SQLite-only; agent reads raw graph).
explain                       -> BLOCKS (SQLite-only; agent).
check                         -> BLOCKS (SQLite-only; agent; also `declarations`).
any CLI default path still SQLite -> BLOCKS: the auto-fallback (callers/callees/path) reads nodes/edges
                                 whenever LiveGraph is not complete/fresh/TS, AND for every un-refreshed repo.
PLUS (not in the original list, but real): multi-language (LiveGraph is TS-only; nodes/edges are all-
languages) and the non-graph commands (trust/gate/docs/contracts/deps/surfaces/boundaries/modules).
=> Every item BLOCKS. nodes/edges retirement is gated on ALL of them.
```

### Q6 — next highest-value migration slice?
```text
GOAL-DEPENDENT (record the criterion; the user chooses the next slice):

Thread A — "exact module cycles" (close the cycles semantics gap, toward rmap-cycles parity):
  IMPORTS-EXTRACT-COMPLETENESS-1  (widen the producer: unresolved/package/dynamic/re-export imports)
  -> IMPORTS-XPART-RESOLUTION-1    (cross-partition + index/extension resolution)
  -> MODULE-AGGREGATION-1          (FILE import graph -> MODULE identity / MODULE->MODULE edges)
  -> CYCLES-LIVEGRAPH-1 (module)   (then `rmap cycles` could migrate with honest completeness)

Thread B — "broader command migration" (toward retiring nodes/edges):
  IMPORTS-LIVEGRAPH-1   (`rmap imports` over the IR import graph — closest, edge listing; but BLOCKED on
                         the same completeness gap as Thread A — a subset graph cannot back `rmap imports`).
  STATS-LIVEGRAPH-1     (module degree/complexity over the IR — needs measurements, which the IR lacks).
  ORIENT-EXPLAIN-TRUST-1 (route the agent through the trust substrate — large; reads many tables).

Highest LEVERAGE: IMPORTS-EXTRACT-COMPLETENESS-1 — it gates BOTH threads (module cycles AND any honest
`rmap imports`/stats migration). Until the captured import graph is complete enough, neither module cycles
nor imports/stats can migrate without lying about completeness. Recommend it next IF the goal is cycles or
imports/stats; choose ORIENT-EXPLAIN-TRUST-1 only if the goal is the orient/explain/check thread (a
separate, larger, agent-side effort that does not depend on imports).
```

## Raw-graph retirement vs SQLite retirement (re-separated)
```text
RAW GRAPH (`nodes`/`edges`) retirement: the ONLY decommission candidate. Blocked by Q5 (all items).
  Even fully migrating the graph commands does NOT free nodes/edges while the non-graph commands and the
  multi-language requirement read them. A realistic "nodes/edges retirement" needs: all graph commands on
  LiveGraph with NO SQLite fallback (eliminated, not defaulted-away), all languages covered, AND the
  non-graph readers re-homed or accepted as permanent SQLite.
SQLite (whole-DB) retirement: NOT a goal and likely NEVER. 31 of 33 tables have no LiveGraph representation
  (authoritative/operational/boundary/surface/contract/measurement). "Decommission" remains, at most, a
  `nodes`/`edges`-ONLY retirement — and even that is far off.
```

## Deletion gates (unchanged from READINESS-1 §5; current status)
```text
1 no default command depends on nodes/edges (all languages)  -> FAILS (auto-fallback + 5 SQLite-only cmds)
2 LiveGraph covers the SAME data for ALL languages           -> FAILS (TS-only; topology only; no imports-
                                                                completeness, no measurements/boundaries)
3 migration/back-compat story                                -> not reachable
4 operator reset story (raw graph is the only multi-lang store)-> not reachable
5 per-command parity tests on the new backend                -> partial (callers/callees/path have
                                                                --engine compare; cycles file-import is a
                                                                DIFFERENT graph, no parity to module cycles)
```

## Guardrails honored
```text
No code. No table deletion. No migration. Audit doc only.
```

## References
- `docs/slices/sqlite-raw-decommission-readiness-1.md` (the baseline 33-table inventory + gates)
- `docs/slices/query-migration-cli-1.md`, `path-livegraph-default-1.md` (callers/callees/path default auto)
- `docs/slices/cycles-livegraph-1.md`, `cycles-livegraph-cli-1.md`, `path-cycles-livegraph-2.md` (file-import surface; the module/file-import boundary)
- `docs/slices/imports-module-ingest-1.md` (captured imports in the IR; the completeness limits)
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (Engine::Auto arms :411/:480/:750; file_import_cycles_response)
- `rust/crates/daemon-runtime/src/dispatch.rs` (handle_imports/stats/orient/explain/check — SQLite-only)
- `rust/crates/storage/src/queries.rs` (find_imports, compute_module_stats, find_cycles — nodes/edges reads)
