# SQLITE-RAW-DECOMMISSION-READINESS-5: Transition Audit (delta) (Stage D)

Slice ID: SQLITE-RAW-DECOMMISSION-READINESS-5
Status: **AUDIT DELTA — evidence map. No code, no table deletion, no migration, no decommission.** A SHORT
recompute of READINESS-4 after the IMPORTS thread: the explicit LiveGraph import surface (CLI-1), the per-file
+ repo-wide no-loss readiness (DEFAULT-READINESS-1 / REPOWIDE-READINESS-1, GREEN-SAFE over 1303 files), and the
DEFAULT FLIP of `rmap imports <file>` to LiveGraph-first compare-on-call (DEFAULT-1). Baseline: READINESS-4.
Track: Stage D. Gates any future SQLITE-RAW-DECOMMISSION-1.

## Verdict (headline)
```text
`nodes`/`edges` STILL NOT ready to retire. NEW this delta: `rmap imports <file>` is now DEFAULT-migrated to
LiveGraph-first (serves the alias/dynamic edges SQLite missed; proven no-loss). BUT the migration does NOT
reduce the SQLite dependency: D2=B compare-on-call reads SQLite `find_imports` EVERY default call (the no-loss
baseline + the fallback answer). So imports JOINS callers/callees/path as a LiveGraph-FIRST command that STILL
reads nodes/edges eagerly. ZERO default commands serve without reading SQLite; the retirement blockers are
otherwise UNCHANGED (cycles/stats default SQLite; orient/check/explain/trust SQLite; non-TS; the eager
safety-net read; the 31 non-graph tables).
```

## Delta since READINESS-4 (what the imports thread changed — and did NOT)
```text
+ imports DEFAULT MIGRATED: `rmap imports <file>` default = `auto` (Engine LiveGraph-first). Serves the
  LiveGraph edge listing (mapped to the ImportEntry shape, WITH the alias/dynamic extras) IFF precondition met
  (resident+Fresh+TS) AND the per-call directional no-loss passes; else a LABELLED SQLite fallback
  (LiveGraphUnavailable / Stale / UnsupportedLanguage / ImportRegression / ImportUnknown). Human byte-compatible
  (metadata stripped); explicit `--engine sqlite|livegraph|compare` unchanged. LIVE: amodx alias file ->
  livegraph + 2 extras ; OpenXcom .cpp -> sqlite fallback.
+ EVIDENCE: per-file + repo-wide no-loss is GREEN-SAFE -- 0 regression / 0 unknown over 1303 files (xpart /
  amodx / repo-graph / OpenXcom). The per-file gate (imports_compare_sidecar) + the precondition
  (file_partition_status) + the bulk compare (imports --engine compare, no-file=repo-wide) all exist.
~ unchanged: callers/callees/path DEFAULT auto (LiveGraph-first, SQLite EAGER fallback). The imports answer
  QUALITY improved (superset) but the SQLite READ did not go away.
- NOT changed (the decommission blockers): imports compare-on-call READS SQLite every call (no fastpath yet);
  `rmap cycles` / `stats` default SQLite ; orient / check / explain / trust SQLite-only (coherence-pending) ;
  LiveGraph TS-only (non-TS always falls back) ; the eager safety-net read in ALL 4 migrated commands ; the 31
  non-graph tables.
```

## Default-path SQLite-read audit (EXECUTED 2026-06-07 — dispatch.rs + livegraph_feed.rs)
```text
COMMAND   DEFAULT      SERVES (when precondition met)     READS nodes/edges   PATTERN
callers   Auto         LiveGraph (Exact+Fresh+TS) else SQL  YES (EAGER)         LiveGraph-first; SQLite safety-net
callees   Auto         same                                YES (EAGER)         LiveGraph-first; SQLite safety-net
path      Auto         LiveGraph (all-present) else SQL     YES (EAGER)         LiveGraph-first; SQLite safety-net
                                                                                (PATH-LIVEGRAPH-DEFAULT-1)
imports   Auto         LiveGraph (precond + no-loss) else   YES (EAGER)         LiveGraph-first; SQLite is the
                       SQLite                                                   COMPARE-ON-CALL baseline (D2=B)
cycles    sqlite       SQLite (LG/compare are EXPLICIT)     YES (EAGER)         SQLite answer; no default flip
stats     sqlite       SQLite                              YES (EAGER)         SQLite-only (no LiveGraph path)
orient    sqlite/agent SQLite                              YES (EAGER)         SQLite-only (coherence-pending)
check     sqlite/agent SQLite                              YES (EAGER)         SQLite-only (coherence-pending)
explain   sqlite/agent SQLite                              YES (EAGER)         SQLite-only (coherence-pending)
trust     sqlite/svc   SQLite                              YES (EAGER)         SQLite-only

=> 4 commands LiveGraph-FIRST but EAGER SQLite (callers/callees/path/imports); 6 SQLite-as-answer. 10/10 read
   nodes/edges EAGERLY on the default call. The stale dispatch comment "path does NOT auto-migrate" is
   superseded by PATH-LIVEGRAPH-DEFAULT-1 (path IS LiveGraph-first).
```

## Audit questions (the user's recompute — answered)
```text
Q1 — Which default commands STILL read SQLite nodes/edges?
  ALL TEN: callers, callees, path, imports, cycles, stats, orient, check, explain, trust. Every default call
  hits nodes/edges (the audit table). NONE has eliminated the read.

Q2 — Which defaults read SQLite ONLY as a safety net (the answer comes from LiveGraph when possible)?
  callers, callees, path, imports. These SERVE LiveGraph when the precondition holds (imports: + no-loss), but
  the SQLite read is EAGER -- fetched every call as the fallback (callers/callees/path) or the compare-on-call
  baseline (imports). The READ is not avoided; only the ANSWER is LiveGraph. The other 6 read SQLite AS the
  served answer (not a safety net).

Q3 — Which defaults can serve LiveGraph WITHOUT reading SQLite?
  NONE in the default path. Only the EXPLICIT `--engine livegraph` surfaces (callers/callees/path/imports +
  cycles --kind) answer without SQLite. The DEFAULT always reads nodes/edges (eager).

Q4 — What remains for raw `nodes`/`edges` retirement?
  (a) FASTPATH the 4 migrated commands: make the eager SQLite read LAZY (only in the fallback arm) so a SERVED
      LiveGraph answer does not touch SQLite. For callers/callees/path this is a code restructure (precondition-
      only already trusts LiveGraph; the eager fetch is incidental). For imports it is a SAFETY-MODEL change
      (D2 B compare-on-call -> A precondition-only / C certificate-cache) -- IMPORTS-LIVEGRAPH-DEFAULT-FASTPATH-1.
  (b) DEFAULT-migrate cycles + stats (still SQLite default; cycles has the explicit LG/compare; stats has no
      LiveGraph path yet).
  (c) COHERENCE LAYER for orient / check / explain / trust (SQLite-only; mixed live+persisted -- needs design).
  (d) NON-TS coverage: LiveGraph is TS-only; every non-TS file/repo falls back -> nodes/edges stay load-bearing
      for C/C++/Rust/Java until non-TS LiveGraph exists.
  (e) the 31 non-graph tables (out of scope for nodes/edges, but the broader decommission).
  nodes/edges become retirable ONLY when NO default reads them for a served answer AND the fallback path is
  itself removed or covered (a + b + c + d).

Q5 — Next highest-value slice? GOAL-DEPENDENT (the user's criterion):
  - REDUCE SQLite dependency NOW (the decommission goal) -> a FASTPATH. Highest leverage: callers/callees/path
    share the SAME incidental eager read -> a UNIFIED auto-fastpath (eager -> lazy) removes THREE commands'
    served-path SQLite reads at low effort. imports needs the safety-model change (FASTPATH-1) -- do it too, but
    it is the harder one (it gives up the per-call no-loss net).
  - CONTINUE QUERY-MIGRATION breadth -> `stats` (a SQLite-only default -> LiveGraph; no LiveGraph path exists
    yet, so it is a real build) before coherence.
  - orient/check/explain/trust -> ONLY after the COHERENCE-LAYER design (mixed live+persisted contract).
  RECOMMEND the DELTA verdict first (this doc), then choose: (i) imports FASTPATH-1 if imports-specific
  decommission is the goal ; (ii) a callers/callees/path lazy-fastpath if broad served-path SQLite reduction is
  the goal (higher leverage) ; (iii) stats migration for QUERY-MIGRATION breadth ; (iv) coherence-layer design.
```

## Deletion gates (READINESS-1 §5; current status)
```text
1 no default command depends on nodes/edges      -> FAILS. 10/10 defaults read nodes/edges EAGERLY (4
                                                    LiveGraph-first-but-eager + 6 SQLite-answer).
2 LiveGraph covers SAME data for ALL languages   -> FAILS. TS-only; non-TS always falls back. (imports proven
                                                    no-loss for TS, but TS only.)
3 migration / back-compat story                  -> PARTIAL for the 4 migrated commands (Auto + labelled
                                                    fallback + byte-compatible human); none for cycles/stats/
                                                    coherence; none for nodes/edges as a whole.
4 operator reset story                           -> not reachable.
5 per-command parity tests on the new backend    -> callers/callees/path: Auto + compare evidence. imports:
                                                    GREEN-SAFE no-loss (0 regression/1303 files) -- the
                                                    STRONGEST parity evidence in the codebase. cycles: fixture-
                                                    exact only. stats/orient/check/explain/trust: none.
```

## Remaining blockers (the user's recompute, confirmed)
```text
- imports compare-on-call reads SQLite EVERY call                          -> no fastpath (the imports nuance).
- callers/callees/path eager SQLite safety-net read                        -> still reads nodes/edges.
- default `rmap cycles` + `stats` still SQLite                             -> unflipped.
- orient / check / explain / trust                                        -> SQLite-only (coherence-pending).
- non-TS languages                                                        -> LiveGraph TS-only.
- the 31 non-graph tables                                                 -> the broader decommission.
```

## Guardrails honored
```text
No code. No table deletion. No migration. No decommission. No default flip. Audit-delta doc only. The imports
DEFAULT migration (DEFAULT-1) is RECORDED, not re-done; nodes/edges untouched.
```

## References
- `docs/slices/sqlite-raw-decommission-readiness-4.md` (baseline) + `-3.md` / `-2.md` / `-1.md` (inventory + gates)
- `docs/slices/imports-livegraph-default-1.md` (the imports DEFAULT flip — compare-on-call, reads SQLite every call)
- `docs/slices/imports-livegraph-repowide-readiness-1.md` (GREEN-SAFE no-loss evidence, 1303 files)
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (`imports_auto_response` / `auto_outcome` / `FallbackReason` — the eager-read default paths)
