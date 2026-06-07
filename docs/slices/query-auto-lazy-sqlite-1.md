# QUERY-AUTO-LAZY-SQLITE-1: lazy SQLite fallback for the callers/callees/path default (Auto)

Slice ID: QUERY-AUTO-LAZY-SQLITE-1
Status: **IMPLEMENTED + LIVE-VALIDATED (2026-06-07).** The default Auto path of callers/callees/path no longer
reads SQLite when LiveGraph serves (live: amodx served -> backend=livegraph; OpenXcom non-TS -> sqlite
fallback). Output byte-preserving; --engine sqlite/compare unchanged. Commits df75c0e (spec) -> 65e099d (impl),
UNPUSHED. See **Completion**. Refactor the DEFAULT
`Auto` path of `callers` / `callees` / `path` from an EAGER SQLite fallback read (fetched every call, before
the LiveGraph decision) to a LAZY one (fetched ONLY when LiveGraph cannot serve). OUTPUT-PRESERVING: byte-
identical served + fallback answers; the only change is that a LiveGraph-served default call no longer reads
`nodes`/`edges`. NO new trust model, NO safety trade-off (these are precondition-only today), NO imports /
cycles / stats / orient / explain / check change, NO raw decommission.
Depends: QUERY-MIGRATION-CLI-1 (the `Auto` + `FallbackReason` + `*_engine_response` this refactors).
SQLITE-RAW-DECOMMISSION-READINESS-5 (which flagged the eager read as the highest-leverage decommission step).
Track: Stage D, QUERY-MIGRATION-1 (decommission path).

## Why now (priority path)
```text
READINESS-5: ALL 10 default commands read nodes/edges EAGERLY. The 4 migrated ones (callers/callees/path/
imports) serve LiveGraph but STILL read SQLite every call. For callers/callees/path the eager read is
INCIDENTAL (they are precondition-only -- the served answer never uses the SQLite result), so making it LAZY is
a low-risk code restructure that removes THREE commands' served-path SQLite reads at once -- the highest
decommission leverage available (imports needs a harder safety-model change; deferred to FASTPATH-1).
```

## Grounding (EXECUTED 2026-06-07 — confirm the eager read + where SQLite is actually used)
```text
EAGER READ (dispatch.rs): handle_callers fetches `storage.find_direct_callers` at :954 BEFORE the engine parse
  (:969) and passes the Vec into callers_engine_response (:976). handle_callees: `find_direct_callees` eager,
  same shape. handle_path: builds the full `sqlite_value` (find_shortest_path) eagerly, then path_engine_
  response (:1626+). So SQLite is read on EVERY default call.
WHERE THE EAGER RESULT IS ACTUALLY USED (livegraph_feed.rs callers_engine_response :432):
  Engine::Sqlite (:441)   -> uses it (the answer)                         -> NEEDED.
  Engine::Auto (:442)     -> Ok(keys) -> serves LiveGraph, IGNORES it ; Err(reason) -> uses it (fallback).
  Engine::LiveGraph(:450) -> Some -> serves LiveGraph, IGNORES it ; None -> uses it (fallback).
  Engine::Compare (:463)  -> uses it (the SQLite answer + the compare report)  -> NEEDED.
  => the SERVED LiveGraph path (Auto-Ok / LiveGraph-Some) NEVER uses sqlite_callers -- the eager read is WASTED.
COMPARE SIDECAR: write_compare_sidecar is ONLY in the Engine::Compare arm (:470). Auto / LiveGraph compute NO
  sidecar -> no hidden eager SQLite there.
auto_outcome (the fallback conditions): None -> LiveGraphUnavailable ; !Fresh -> LiveGraphStale ; !Exact ->
  LiveGraphPartial ; !ts_only -> LiveGraphUnsupportedLanguage ; else Ok (serve). path_auto_outcome adds
  LiveGraphDisplayMetadataUnavailable (a node lacks file:line). A no-path EXACT is Ok (serveable -> skip SQLite).
PATH NUANCE: path_engine_response extracts `sqlite_found` / `sqlite_names` from sqlite_value EAGERLY at the top
  (:750-766) but USES them ONLY in the Compare arm (:824-829) -> the extraction must MOVE INTO the Compare arm.
SQLITE-ERROR HANDLING: the eager read handles StorageError in the dispatch (returns an error). Lazy -> the error
  can only occur when the closure is actually called (Sqlite/Compare/fallback) -> it must propagate from the
  engine_response (a Result return).
```

## Forced decisions — every cell filled (ratify at sign-off)

### D1 — The lazy mechanism
```text
A. LAZY CLOSURE [LEAN]: pass a `sqlite_fetch: impl FnOnce() -> Result<Vec<CallerResult>, StorageError>` (path:
   a closure building the sqlite_value) into `*_engine_response`, which returns `Result<Value, StorageError>`.
   The Sqlite + Compare arms call it (always); the Auto + LiveGraph arms call it ONLY in the fallback (Err/None)
   branch. The dispatch maps the Result -> DispatchResult (Ok -> success, Err -> InternalError). The served
   LiveGraph path never invokes the closure -> no SQLite read. Pure, local, output-preserving.
B. RESTRUCTURE IN DISPATCH: move the LiveGraph decision into the dispatch; fetch SQLite only in the dispatch's
   fallback branch; keep `*_engine_response` eager-input for Sqlite/Compare. More dispatch churn; duplicates the
   Auto decision across the 3 handlers (the engine_response already centralizes it).
C. EAGER-BUT-CONDITIONAL: keep the eager fetch but guard it behind a "will I need SQLite?" precheck (run the
   LiveGraph decision first, then fetch SQLite only if it failed). == A's effect but without the closure -- two
   LiveGraph evaluations or a re-ordering that re-introduces the coupling. REJECT (A is cleaner).
RECOMMENDATION: A. The closure makes the laziness STRUCTURAL (the served arm cannot read SQLite) and directly
   testable (D3). `*_engine_response` already owns the Auto/LiveGraph/Compare decision -- the closure just defers
   the input it already gates.
```

### D2 — Testability (extract the pure decision)
```text
A. EXTRACT a pure decision per command -- e.g. `callers_auto(lg_outcome: Result<Vec<String>, FallbackReason>,
   sqlite_fetch: impl FnOnce() -> Result<Vec<CallerResult>, StorageError>) -> Result<(answer, backend, reason)>`
   -- so a UNIT TEST can pass `lg_outcome = Ok(keys)` + a PANICKING closure and assert it is NEVER called
   (proves the served path skips SQLite WITHOUT a RepoState/LiveGraph). The RepoState-bound part (compute
   lg_outcome via livegraph_callers_auto) stays in the engine_response. [LEAN]
B. NO extraction; test only live (instrument the SQLite call count). Heavier, less precise, no structural proof.
RECOMMENDATION: A. The closure-not-called unit test is the crisp proof of the user's #4 ("SQLite query function
   is not called on the LiveGraph-served path").
```

### D3 — Validation (the user's #4)
```text
- UNIT: a PANICKING `sqlite_fetch` closure + an Ok LiveGraph outcome -> the decision serves LiveGraph and NEVER
  calls the closure (no panic). An Err/None outcome -> the closure IS called and its result served + labelled.
- LIVE: xpart/amodx TS callers/callees/path served by LiveGraph -> backend_used=livegraph and (instrument or a
  debug counter) ZERO `find_direct_callers`/`find_direct_callees`/`find_shortest_path` calls on that path.
- LIVE: a FORCED fallback (non-TS symbol / non-resident partition) -> backend_used=sqlite, fallback_reason set,
  output BYTE-IDENTICAL to today.
- LIVE: a path NO-PATH EXACT (LiveGraph proves no route, all-resident) -> served by LiveGraph, SKIPS SQLite.
RECOMMENDATION: as written. The unit test is the gate; the live checks confirm end-to-end + byte-parity.
```

### D4 — Scope + parity (the user's #5 risk)
```text
SCOPE: callers, callees, path ONLY. imports (compare-on-call, the harder safety-model change) UNTOUCHED;
  cycles / stats / orient / explain / check / trust UNTOUCHED.
PARITY (byte-identical, OUTPUT-PRESERVING):
  - `--engine sqlite` -> the closure is called (== the eager fetch) -> byte-IDENTICAL. UNCHANGED.
  - `--engine compare` -> the closure is called -> byte-IDENTICAL (the compare still reads SQLite + writes the
    sidecar). UNCHANGED. (path: the sqlite_found/sqlite_names extraction moves INTO the Compare arm -- same
    inputs, same report.)
  - Auto served (LiveGraph) -> byte-IDENTICAL to today's served answer (only the wasted read is removed).
  - Auto fallback -> byte-IDENTICAL to today's fallback (same SQLite answer + same fallback_reason).
  - the EXPLICIT `--engine livegraph` arm ALSO becomes lazy (skips SQLite on Some) -- a free bonus, output
    unchanged. STALE COMMENTS corrected (e.g. dispatch.rs:1626 "path does NOT auto-migrate" -- superseded by
    PATH-LIVEGRAPH-DEFAULT-1).
RECOMMENDATION: as written.
```

## Build contract (PROPOSED — gated on D1–D4 ratification)
```text
1. livegraph_feed: change callers_engine_response / callees_engine_response / path_engine_response to take a
   `sqlite_fetch` closure (FnOnce -> Result) instead of the eager Vec/Value, and return Result<Value,
   StorageError>. Call the closure only in Sqlite/Compare (always) + Auto/LiveGraph (fallback). Extract the pure
   per-command auto-decision (D2). path: move sqlite_found/sqlite_names into the Compare arm.
2. dispatch: handle_callers/callees/path -- remove the eager find_*; pass a closure that does the find_* lazily;
   map the Result -> DispatchResult. Correct the stale comments.
3. UNIT tests (D3): the panicking-closure-not-called proof per command + the fallback-calls-closure proof.
4. live + gate + completion doc (D3 live checks).
Stop if: making the read lazy changes ANY byte of the served OR fallback output (it must not -- output-
  preserving). Stop if a SQLite error on a served path is observed (the served path must not touch SQLite).
```

## Out of scope (hard guardrails)
```text
NO imports change (FASTPATH-1 is separate) ; NO cycles/stats/orient/explain/check/trust change ; NO raw
decommission ; NO SQLite deletion ; NO new fallback conditions / trust model ; NO output change (byte-
preserving) ; NO change to `--engine sqlite` / `--engine compare`.
```

## Completion (IMPLEMENTED + LIVE-VALIDATED 2026-06-07, EXECUTED)

Commits: `df75c0e` (spec) -> `65e099d` (impl: the 3 *_engine_response lazy refactors + the 3 pure
*_auto_or_sqlite decisions + the dispatch closures + the stale-comment fix). UNPUSHED.

### Gate (EXECUTED 2026-06-07)
```text
cargo test --workspace -> no failures (98 daemon tests). clippy --workspace --all-targets -- -D warnings ->
clean. fmt --check -> clean. Unit: callers_auto_or_sqlite (served = PANICKING closure NEVER called; fallback +
sqlite = closure runs, sentinel appears) + path_auto_or_sqlite (no-path-EXACT served skips; fallback calls).
```

### Live validation (EXECUTED 2026-06-07)
```text
SERVED (LiveGraph, TS) -> backend_used=livegraph (the eager SQLite read is SKIPPED): amodx callers
  deriveTenantFromOrigin/detectGpuTier(5)/verifyTenantFromOrigin(12)/loadMediaMap(6)/publishAudit(66) ; callees
  detectGpuTier(12). All fallback_reason=null.
FALLBACK (non-TS) -> backend_used=sqlite: OpenXcom callers `Action` -> fallback_reason=LiveGraphUnavailable,
  count=7 (the C++ callers preserved). The empty LiveGraph correctly forces the LAZY SQLite read.
HUMAN unchanged: amodx callers detectGpuTier -> the existing "Callers of <s>\nFile: ...\n\nN callers found\n..."
  format, NO backend/fallback leak.
OVERRIDES: `--engine sqlite` -> backend_used=sqlite (the escape hatch reads SQLite) ; `--engine compare` ->
  backend_used=sqlite + livegraph_compare present (Compare STILL reads SQLite + writes the sidecar). UNCHANGED.
```

### Acceptance (the ratified list) — PASS
```text
1. SQLite query function NOT called on the LiveGraph-served path -- the panicking-closure unit test (structural
   proof) + live backend_used=livegraph.                                                                 PASS.
2. Forced fallback (non-TS) calls SQLite + preserves output (OpenXcom 7 callers, byte-identical).         PASS.
3. path no-path EXACT skips SQLite (unit test: served Some((false, [])) with a panicking closure).        PASS.
4. Compare calls SQLite (live: --engine compare -> backend=sqlite + the compare report). UNIT: covered
   structurally (the Compare arm's first line is sqlite_fetch()?) -- see Divergences.                     PASS.
5. callers/callees/path ONLY; imports/cycles/stats/orient/explain/check/trust untouched; --engine sqlite/
   compare unchanged; human unchanged; the stale path comment corrected.                                  PASS.
```

### Divergences / notes (recorded)
```text
- Test 4 (Compare-calls-SQLite) is NOT a pure unit test: the Compare arm needs a RepoState (the LiveGraph
  compare report), and RepoState construction needs a real db path (RepoKey::new) -- disproportionate. MITIGATED:
  the Compare arm calls sqlite_fetch()? UNCONDITIONALLY as its first line (structural), and the LIVE check
  (--engine compare -> backend=sqlite + livegraph_compare) confirms it. The pure tests cover served-skip /
  fallback-call / sqlite-call / no-path-EXACT.
- error handling (constraint #2): *_engine_response returns Result<Value, StorageError>; the dispatch maps it
  to DispatchResult ONLY when the closure runs. The eager-SQLite-error on a served path is intentionally gone
  (the served path never reads SQLite) -- the point of the slice.
- LIVE-VALIDATION DAEMON STATE: the manual release rmapd was restarted (pkill -x rmapd, exact; new binary +
  producer env) and amodx/repo-graph refreshed. Reported before acting. Re-run ./scripts/dev-install-local.sh
  to restore the launchd-managed daemon.
```

## References
- `rust/crates/daemon-runtime/src/dispatch.rs` (handle_callers :952 / handle_callees / handle_path — the eager reads to make lazy; the stale path comment :1626)
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (`callers_engine_response` :432 / `callees_engine_response` :501 / `path_engine_response` :740 / `auto_outcome` / `FallbackReason` — the arms + the served-vs-fallback usage)
- `docs/slices/sqlite-raw-decommission-readiness-5.md` (the audit that flagged the eager read)
- `docs/slices/query-migration-cli-1.md` (the Auto + fallback semantics this preserves)
