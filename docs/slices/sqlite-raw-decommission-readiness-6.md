# SQLITE-RAW-DECOMMISSION-READINESS-6: Transition Audit (delta) (Stage D)

Slice ID: SQLITE-RAW-DECOMMISSION-READINESS-6
Status: **AUDIT DELTA — evidence map. No code, no table deletion, no migration, no decommission.** A SHORT
recompute of READINESS-5 after QUERY-AUTO-LAZY-SQLITE-1 (lazy SQLite fallback for callers/callees/path) and
IMPORTS-LIVEGRAPH-DEFAULT-FASTPATH-1 (the repo-level no-loss cert fastpath for imports). Baseline: READINESS-5.
Track: Stage D. Gates any future SQLITE-RAW-DECOMMISSION-1.

## Verdict (headline)
```text
`nodes`/`edges` STILL NOT ready to retire. NEW + MATERIAL this delta: for the FIRST time, a migrated default
command's COMMON (LiveGraph-served) path reads NO SQLite. FOUR defaults now have a SQLite-FREE served path --
callers/callees/path (lazy: SQLite only on fallback) + imports (a valid GREEN repo cert serves LiveGraph; SQLite
is read ONCE per fingerprint to BUILD the cert, not per served call). This is the FIRST real per-call SQLite
reduction. The retirement is otherwise UNCHANGED: 6 defaults still read SQLite eagerly (cycles/stats/orient/
check/explain/trust); the fallback paths + the imports cert build + non-TS still read nodes/edges; the 31
non-graph tables remain.
```

## Delta since READINESS-5 (what the two slices changed — and did NOT)
```text
+ callers/callees/path LAZY (QUERY-AUTO-LAZY-SQLITE-1): the eager find_direct_callers/callees/find_shortest_path
  became a lazy `sqlite_fetch` closure the *_engine_response calls ONLY on the Sqlite/Compare arms + the Auto/
  LiveGraph FALLBACK. The LiveGraph-served Auto path reads NO SQLite (live: amodx publishAudit 66 callers etc.
  -> backend=livegraph, no read). OUTPUT-PRESERVING.
+ imports FASTPATH (FASTPATH-1, D1=C/D2=T1+S1): a repo-level no-loss CERT (in-memory, lazy-built on first query
  via the repo-wide compare, keyed by the SQLite-free import-cert fingerprint). GREEN cert + precondition met ->
  serve LiveGraph WITHOUT find_imports (live: amodx fastpath, comparison.source=repo_no_loss_certificate; 2nd
  file from the cache). SQLite read ONCE per fingerprint (the cert build) instead of every served call.
~ unchanged: the FALLBACK paths still read SQLite (callers/callees/path non-resident/non-TS; imports non-TS/RED/
  stale -> compare-on-call). The explicit --engine sqlite/compare read SQLite (by design).
- NOT changed: `rmap cycles` / `stats` default SQLite ; orient/check/explain/trust SQLite-only (coherence-
  pending) ; LiveGraph TS-only (non-TS always falls back) ; the imports cert BUILD reads SQLite (once/finger-
  print) ; the 31 non-graph tables.
```

## Default-path SQLite-read audit (UPDATED 2026-06-07 — the READINESS-5 table, re-measured)
```text
COMMAND   DEFAULT   SERVED-PATH READS nodes/edges?   WHEN SQLite IS READ
callers   Auto      NO (lazy)                         only on FALLBACK (non-resident / non-TS / stale / !Exact)
callees   Auto      NO (lazy)                         only on FALLBACK
path      Auto      NO (lazy)                         only on FALLBACK
imports   Auto      NO on the GREEN-cert fastpath     cert BUILD (once/fingerprint) + FALLBACK (non-TS/RED/stale
                                                      -> compare-on-call) + non-TS precondition
cycles    sqlite    YES (eager, every call)           the served answer (no default migration)
stats     sqlite    YES (eager)                       the served answer (no LiveGraph path)
orient    sqlite    YES (eager)                       agent reads storage (coherence-pending)
check     sqlite    YES (eager)                       agent reads storage (coherence-pending)
explain   sqlite    YES (eager)                       agent reads storage (coherence-pending)
trust     sqlite    YES (eager)                       service reads storage (coherence-pending)

=> 4/10 defaults now have a SQLite-FREE COMMON (served) path (callers/callees/path/imports). 6/10 still read
   nodes/edges EAGERLY as the served answer. (READINESS-5: 10/10 eager -> now 4 served-free.)
```

## Audit questions (the user's recompute — answered)
```text
Q1 — Which defaults can now serve WITHOUT SQLite on the common LiveGraph path?
  callers, callees, path (lazy -- the served Auto path reads nothing; SQLite only on fallback) and imports (a
  VALID GREEN repo cert serves LiveGraph with no find_imports). FOUR. This is the FIRST audit where any migrated
  default's common path is SQLite-free.

Q2 — Which still need SQLite EAGERLY (every call, as the served answer)?
  cycles, stats, orient, check, explain, trust. SIX. None migrated; each reads nodes/edges every call.

Q3 — Which need SQLite as a CERT / COMPARE BUILD input?
  imports: the repo cert is BUILT by the repo-wide compare (bulk all_imports) -- ONE SQLite read per fingerprint,
  amortized across all of that repo's fastpath queries. (The explicit `--engine compare` for callers/callees/
  path/imports/cycles also reads SQLite, but those are EXPLICIT, not the default.)

Q4 — What remains before `nodes`/`edges` can be dropped?
  (a) DEFAULT-migrate cycles + stats (still SQLite-served every call). cycles has the explicit LiveGraph/compare;
      stats has NO LiveGraph path yet.
  (b) COHERENCE LAYER for orient/check/explain/trust (SQLite-only; mixed live+persisted -- needs design).
  (c) NON-TS coverage: LiveGraph is TS-only -> every non-TS file/repo FALLS BACK to SQLite. nodes/edges stay
      load-bearing for C/C++/Rust/Java until non-TS LiveGraph exists.
  (d) the FALLBACK paths of the 4 migrated defaults (non-resident / non-TS / stale / RED) read SQLite -> the
      table is needed for safety even where the common path is free.
  (e) the imports CERT BUILD reads SQLite once/fingerprint -> a FULLY SQLite-free imports default needs the cert
      source to become SQLite-free (a STRUCTURAL no-loss proof, not a SQLite compare) -- a deeper change.
  (f) the 31 non-graph tables (out of scope for nodes/edges; the broader decommission).
  nodes/edges become droppable ONLY when EVERY default served path is SQLite-free AND the fallback is removed/
  covered (non-TS + residency) AND the cert build is SQLite-free -- (a)+(b)+(c)+(d)+(e).

Q5 — Highest-value next slice? GOAL-DEPENDENT:
  - QUERY-MIGRATION BREADTH -> STATS-LIVEGRAPH-1 (the last drilldown SQLite-only default with NO LiveGraph path;
    a real build). Spec-first.
  - DECOMMISSION DEPTH -> CYCLES default fastpath/migration (cycles ALREADY has the explicit LiveGraph + compare
    + the completeness certificate -> a cert-gated default flip mirrors imports FASTPATH-1; reuses the existing
    module-cycle machinery). Higher decommission leverage (removes another eager-SQLite default).
  - COHERENCE LAYER (orient/check/explain/trust) -> LATER (design-first; higher blast radius).
  RECOMMEND the DELTA verdict first (this doc), then choose: STATS-LIVEGRAPH-1 (breadth) OR a CYCLES cert-gated
  default flip (depth -- reuses the module-cycle cert, symmetric to imports). Coherence after a design slice.
```

## Deletion gates (READINESS-1 §5; current status)
```text
1 no default command depends on nodes/edges      -> FAILS, but ADVANCED: 4/10 defaults have a SQLite-FREE served
                                                    path (was 0/10). 6/10 eager + all fallbacks + the cert build.
2 LiveGraph covers SAME data for ALL languages   -> FAILS. TS-only; non-TS always falls back.
3 migration / back-compat story                  -> the 4 migrated defaults have it (Auto + labelled fallback +
                                                    byte-compatible human + lazy/cert); cycles/stats/coherence none.
4 operator reset story                           -> not reachable.
5 per-command parity tests on the new backend    -> callers/callees/path: lazy + panicking-closure proof. imports:
                                                    GREEN-SAFE no-loss (0 regression/1303 files) + the cert. cycles:
                                                    fixture-exact only. stats/orient/check/explain/trust: none.
```

## Remaining blockers (the user's recompute, confirmed)
```text
- cycles + stats default still SQLite (every call)                         -> unmigrated.
- orient / check / explain / trust                                        -> SQLite-only (coherence-pending).
- non-TS languages                                                        -> LiveGraph TS-only (always fallback).
- the migrated defaults' FALLBACK paths                                   -> still read nodes/edges.
- the imports cert BUILD                                                  -> SQLite once/fingerprint (not free).
- the 31 non-graph tables                                                 -> the broader decommission.
```

## Guardrails honored
```text
No code. No table deletion. No migration. No decommission. No default flip. Audit-delta doc only. The
QUERY-AUTO-LAZY-SQLITE-1 + FASTPATH-1 changes are RECORDED, not re-done; nodes/edges untouched.
```

## References
- `docs/slices/sqlite-raw-decommission-readiness-5.md` (baseline) + `-4.md` / `-3.md` / `-2.md` / `-1.md`
- `docs/slices/query-auto-lazy-sqlite-1.md` (callers/callees/path served path -> no SQLite)
- `docs/slices/imports-livegraph-default-fastpath-1.md` (imports GREEN-cert fastpath -> SQLite once/fingerprint)
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (`*_engine_response` lazy closures ; `imports_fastpath_or_compare` / `build_and_store_import_cert`)
