# SQLITE-RAW-DECOMMISSION-READINESS-3: Transition Audit (delta) (Stage D)

Slice ID: SQLITE-RAW-DECOMMISSION-READINESS-3
Status: **AUDIT DELTA — evidence map. No code, no table deletion, no migration.** A SHORT recompute of
READINESS-2 after: the cross-partition FILE-import OVERLAY (IMPORTS-XPART-WIRING-1), multi-partition daemon
loading (IMPORTS-XPART-ENUMERATION-1), the FILE-import render fix (CYCLES-FILE-IMPORT-RENDER-1), and the
standalone fixture (XPART-FIXTURE-STANDALONE-1). Baseline: READINESS-2.
Track: Stage D. Gates any future SQLITE-RAW-DECOMMISSION-1.

## Verdict (headline)
```text
`nodes`/`edges` STILL NOT ready to retire. Progress since READINESS-2 is REAL but stays INSIDE the
file-import cycle surface — a DIFFERENT, EXPLICIT graph. It does not migrate `rmap cycles` (MODULE), does
not free any SQLite-only command, and does not reduce the raw-graph dependency. Every READINESS-1/-2
deletion gate still FAILS.
```

## Delta since READINESS-2 (what the 4 slices changed — and did NOT)
```text
+ FILE-import cycles are now LIVE end-to-end through the daemon. Multi-partition refresh (repeated
  --source-root, sequential best-effort) loads 2+ TS partitions into one LiveGraph; the cross-partition
  OVERLAY upgrades StaticUnresolved relative imports into FILE->FILE edges
  (EdgeBasis::AstImportFileInventoryResolved), so file_import_cycles() detects CROSS-partition cycles.
  EXECUTED (standalone fixture): cross_partition=true, xpart_edge_count=2, cycle a/main <-> b/foo.
+ the CAPTURED import graph WIDENED: intra-partition resolved-relative (READINESS-2) -> intra UNION
  cross-partition resolved-relative (this delta). Still relative + extension/index ONLY.
+ the FILE-import human render is now CORRECT (FILE vocabulary, not "module-level"; no "rmap modules deps")
  -- removes a false-label trust defect; adds NO capability.
+ the cross-partition surface validates cleanly as a STANDALONE repo (its own repo_uid/alias) -- validation
  hygiene; no product-surface change.
~ TS callers/callees/path still DEFAULT auto (unchanged since READINESS-2).
- NOT changed: `rmap cycles` default = SQLite MODULE. OBSERVED on the fixture: `rmap cycles` -> "1
  module-level cycle found" / "(2 modules)"; `rmap cycles --engine livegraph --kind file-import` -> "1 FILE
  import cycle found" / "(2 files)". DIFFERENT graphs, the file-import one EXPLICIT (opt-in). imports / stats
  / orient / explain / check still SQLite-only (no slice touched them). The auto fallback still reads
  nodes/edges. nodes/edges still ALL-language; LiveGraph still TS-only. The 31 non-graph tables unchanged.
```

## Why still not ready (gating distinctions, preserved)
```text
1 FILE-import != MODULE-import: the new graph is FILE->FILE; `rmap cycles` answers MODULE->MODULE. NO module
  aggregation exists yet -> the file-import surface CANNOT replace `rmap cycles`.
2 EXPLICIT != default: file-import cycles are opt-in (--engine livegraph --kind file-import); the `rmap
  cycles` default is untouched SQLite.
3 SUBSET != complete: the captured import graph is relative + extension/index ONLY (cross-partition now
  included). Package-name imports, tsconfig path aliases, dynamic imports, re-exports remain OUT of graph ->
  any honest `rmap imports`/cycles migration is still blocked on completeness.
4 graph-only: stats/orient/explain/check + the non-graph families still pin nodes/edges; multi-language
  still pins nodes/edges (LiveGraph TS-only).
```

## Deletion gates (READINESS-1 §5 / READINESS-2; current status — ALL still FAIL)
```text
1 no default command depends on nodes/edges          -> FAILS (auto-fallback + 5 SQLite-only cmds; `rmap
                                                        cycles` MODULE default unchanged).
2 LiveGraph covers the SAME data for ALL languages   -> FAILS (TS-only; FILE-import topology only; no module
                                                        aggregation; completeness gaps; no measurements/
                                                        boundaries).
3 migration / back-compat story                      -> not reachable.
4 operator reset story                               -> not reachable.
5 per-command parity tests on the new backend        -> still partial: file-import is a DIFFERENT graph (no
                                                        parity to module cycles). The parity MODULE-
                                                        AGGREGATION-1 must establish: derived MODULE cycles
                                                        == SQLite `rmap cycles`.
```

## Remaining blockers (the user's recompute, confirmed)
```text
- MODULE aggregation from FILE imports (the bridge to `rmap cycles` parity)        -> NOT built.
- default `rmap cycles` migration (only AFTER aggregation proven equivalent)       -> not begun.
- imports / stats / orient / explain / check                                       -> SQLite-only.
- non-TS languages                                                                 -> LiveGraph TS-only.
- package / path-alias / dynamic / re-export import completeness gaps              -> captured graph is
                                                                                      relative + ext/index only.
```

## Next FEATURE (recorded; NOT this audit) — MODULE-AGGREGATION-1
```text
Highest-leverage next feature: derive a MODULE-level import graph from the FILE->FILE import graph (resident
FILE edges + the cross-partition overlay), define MODULE IDENTITY + aggregation rules, and COMPARE the
derived MODULE cycles against SQLite `rmap cycles` on (a) the standalone fixture and (b) an existing repo
with module cycles. NO SQLite decommission until proven equivalent/compatible with the current `rmap
cycles`. Ready comparison target: the xpart-monorepo fixture already exhibits BOTH a SQLite module cycle
("1 module-level cycle, 2 modules") AND a LiveGraph file-import cycle ("1 FILE import cycle, 2 files") -- a
clean side-by-side for the equivalence check. This audit does NOT start it.
```

## Guardrails honored
```text
No code. No table deletion. No migration. Audit-delta doc only. The FILE-import vs MODULE-import distinction
is preserved; no module aggregation; no nodes/edges retirement.
```

## References
- `docs/slices/sqlite-raw-decommission-readiness-2.md` (baseline) + `-1.md` (33-table inventory + gates)
- `docs/slices/imports-xpart-wiring-1.md`, `imports-xpart-enumeration-1.md` (overlay live + multi-partition)
- `docs/slices/cycles-file-import-render-1.md` (FILE render correctness)
- `docs/slices/xpart-fixture-standalone-1.md` (standalone fixture validation)
- `rust/crates/repo-graph-import-resolver/src/lib.rs` (relative + ext/index ONLY — the completeness boundary)
- `rust/crates/repo-graph-livegraph/src/lib.rs` (`file_import_cycles` + the cross-partition overlay)
