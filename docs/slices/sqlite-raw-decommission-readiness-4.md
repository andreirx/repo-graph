# SQLITE-RAW-DECOMMISSION-READINESS-4: Transition Audit (delta) (Stage D)

Slice ID: SQLITE-RAW-DECOMMISSION-READINESS-4
Status: **AUDIT DELTA — evidence map. No code, no table deletion, no migration.** A SHORT recompute of
READINESS-3 after the MODULE-cycle thread: MODULE-AGGREGATION-1 (derive MODULE cycles from the captured FILE
graph), MODULE-CYCLES-CLI-1 (explicit `--engine livegraph|compare --kind module-import`), and
MODULE-CYCLES-COMPARE-CLASSIFY-1 (classify missing divergences). Baseline: READINESS-3.
Track: Stage D. Gates any future SQLITE-RAW-DECOMMISSION-1 AND any `rmap cycles` default migration.

## Verdict (headline)
```text
`nodes`/`edges` STILL NOT ready to retire. NEW this delta: the `rmap cycles` MODULE-default migration is now
MECHANICALLY POSSIBLE (the LiveGraph derives MODULE cycles, the CLI exposes + compares them, missing
divergences are classified) but NOT READY -- real-repo equivalence is unproven (only the fixture is EXACT)
and the FILE-graph completeness gap is open. The broader nodes/edges retirement is UNCHANGED: the cycles
thread advanced, the other commands / fallback / non-TS / non-graph tables did not.
```

## Delta since READINESS-3 (what the 3 module-cycle slices changed — and did NOT)
```text
+ MODULE aggregation EXISTS: LiveGraph::module_import_cycles() aggregates the captured FILE import graph
  (resident AstImport UNION the overlay) to MODULE granularity by dirname identity (== the SQLite `rmap
  cycles` identity), skip-self + dedup + Tarjan. FIXTURE-EXACT vs SQLite (unit + live harness).
+ EXPLICIT module-cycle CLI: `--engine livegraph --kind module-import` (MODULE vocabulary, aggregation=
  dirname) + `--engine compare --kind module-import` (SQLite PRIMARY + structural compare + sidecar). The
  `rmap cycles` DEFAULT is byte-UNCHANGED (`--engine sqlite --kind module-import` == default, D6).
+ DIVERGENCE CLASSIFICATION: missing SQLite module cycles are classed from LiveGraph EVIDENCE
  (MissingDueToPackageExternal / DynamicImport / StaticUnresolved / UnloadedOrNonTsPartition /
  ModuleIdentityMismatch), else UnknownDivergence; extras are UnexpectedExtraInLiveGraph. LIVE: a partial
  load classed the missing a<->b cycle MissingDueToUnloadedOrNonTsPartition.
~ unchanged since READINESS-3: callers/callees/path DEFAULT auto (SQLite fallback intact); FILE-import
  surface; completeness = relative + ext/index ONLY.
- NOT changed: the `rmap cycles` DEFAULT is SQLite MODULE. imports / stats / orient / explain / check
  SQLite-only. nodes/edges all-language; LiveGraph TS-only. The 31 non-graph tables. No real-repo module
  parity established (only the synthetic fixture).
```

## Audit questions (answered)
```text
Q1 — What new deletion gate, if any, is now CLOSER?
  Gate 5 (per-command parity tests on the new backend) ADVANCED for cycles: READINESS-3 had "no module
  parity mechanism"; now the parity MECHANISM exists (module_import_cycles + compare_module_cycles + the
  classifier + compare-module-cycles.sh) AND is FIXTURE-EXACT. But it is NOT proven on real repos. A NEW
  sub-gate is now explicit: the `rmap cycles` MODULE-MIGRATION EQUIVALENCE gate (Q2). Gates 1-4 (and the
  broader nodes/edges retirement) are UNCHANGED — no other gate moved.

Q2 — What evidence is STILL MISSING before `rmap cycles` default could migrate?
  (a) A REAL-REPO compare histogram across a repo SET (not just the fixture): the divergence-class counts.
  (b) UnknownDivergence == 0 (every divergence explained — Q4).
  (c) UnexpectedExtraInLiveGraph == 0 across the set (no overclaim — Q5).
  (d) The remaining MISSING classes either CLOSED (package/path-alias/dynamic resolution) OR the migrated
      default HONESTLY LABELS the gap (Partial/caveat), never silently dropping cycles.
  Today only the synthetic fixture is EXACT; (a)-(d) are unestablished.

Q3 — Which divergence classes are ACCEPTABLE for default migration?
  For a PARITY (equivalent) flip: NONE silently — every missing cycle is a SQLite cycle the migrated default
  would drop, i.e. a false "complete" claim. They must be CLOSED.
  For a LABELED-DEGRADATION flip: the MISSING-due-to-{PackageExternal, DynamicImport, StaticUnresolved,
  UnloadedOrNonTsPartition} classes MAY remain IF the migrated default reports the answer as INCOMPLETE
  (e.g. "Partial: N module cycles need package/dynamic resolution") — never a silent drop. ModuleIdentity-
  Mismatch is NOT acceptable either way (it means the identity rule diverged; fix the rule first).

Q4 — Is UnknownDivergence ALLOWED? NO. Unknown means the compare cannot EXPLAIN the divergence -> there is
  no basis to certify the migrated default's completeness. Unknown MUST be 0 before any flip.

Q5 — Is UnexpectedExtraInLiveGraph ALLOWED? NO. An extra LiveGraph cycle is one SQLite does NOT report -> an
  OVERCLAIM / derivation BUG (the LiveGraph module cycles must be a SUBSET of SQLite's). Any extra blocks a
  flip and must be fixed.

Q6 — Next highest-value slice? GOAL-DEPENDENT (the user's criterion):
  - Flip `rmap cycles` default SOON  -> MODULE-CYCLES-DEFAULT-READINESS-1: run the compare across a REAL repo
    SET, establish the divergence histogram, and check Q2(b)/(c) (Unknown=0, no extras). Evidence, not a flip.
  - Reduce divergence CAUSES        -> IMPORTS-PACKAGE-RESOLUTION-1: resolve package names / tsconfig path
    aliases -> shrinks MissingDueToPackageExternal (the dominant real-repo cause) toward 0, closing the gap.
  Recommend MODULE-CYCLES-DEFAULT-READINESS-1 FIRST (it MEASURES the gap cheaply and tells you whether
  IMPORTS-PACKAGE-RESOLUTION-1 is even necessary, or whether labeled-degradation already suffices).
```

## Deletion gates (READINESS-1 §5; current status)
```text
1 no default command depends on nodes/edges      -> FAILS (auto-fallback + 5 SQLite-only; `rmap cycles`
                                                    MODULE default unchanged).
2 LiveGraph covers SAME data for ALL languages   -> FAILS (TS-only; completeness gaps; no measurements/
                                                    boundaries). MODULE cycles now derivable (progress) but
                                                    only fixture-exact + TS.
3 migration / back-compat story                  -> not reachable for nodes/edges; for `rmap cycles` ONLY,
                                                    the compare/classifier are the FIRST evidence rung.
4 operator reset story                           -> not reachable.
5 per-command parity tests on the new backend    -> cycles: MECHANISM + FIXTURE-EXACT (advanced); REAL-REPO
                                                    parity + Unknown=0/no-extra PENDING. Other commands: none.
```

## Remaining blockers (the user's recompute, confirmed)
```text
- default `rmap cycles` still SQLite                                       -> unflipped (gated on Q2).
- package / path-alias / dynamic import completeness gaps                  -> the MISSING-class causes.
- real-repo compare histogram NOT established                             -> only the fixture is EXACT.
- imports / stats / orient / explain / check                             -> SQLite-only.
- non-TS languages                                                       -> LiveGraph TS-only.
- SQLite FALLBACK for callers/callees/path auto                          -> still reads nodes/edges.
```

## Guardrails honored
```text
No code. No table deletion. No migration. No default flip. Audit-delta doc only. FILE-import vs MODULE-import
preserved; the module-cycle surface stays EXPLICIT; nodes/edges untouched.
```

## References
- `docs/slices/sqlite-raw-decommission-readiness-3.md` (baseline) + `-2.md` / `-1.md` (inventory + gates)
- `docs/slices/module-aggregation-1.md` (derive MODULE cycles; fixture-exact)
- `docs/slices/module-cycles-cli-1.md` (explicit module-cycle CLI + compare + sidecar)
- `docs/slices/module-cycles-compare-classify-1.md` (divergence classification; the class vocabulary)
- `rust/crates/repo-graph-livegraph/src/module_cycle_compare.rs` (compare + classifier — the parity evidence engine)
