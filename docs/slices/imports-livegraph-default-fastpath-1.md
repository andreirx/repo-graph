# IMPORTS-LIVEGRAPH-DEFAULT-FASTPATH-1: skip the per-call SQLite read for the imports default

Slice ID: IMPORTS-LIVEGRAPH-DEFAULT-FASTPATH-1
Status: **IMPLEMENTED + LIVE-VALIDATED (2026-06-07). D1=C, D2=T1+S1.** The imports default now serves LiveGraph
WITHOUT per-call SQLite when a GREEN repo cert is valid (live: amodx 1st call builds + fastpaths, 2nd file
served from cache; OpenXcom non-TS -> sqlite; re-refresh invalidates + rebuilds). SQLite read ONCE per
fingerprint, not every call. Commits 02e2a50 (spec) -> 10e7602 (impl), UNPUSHED. See **Completion**. Replace
the imports
default (`Auto`) COMPARE-ON-CALL (which reads SQLite `find_imports` EVERY served call) with a SAFE FASTPATH that
serves LiveGraph WITHOUT the per-call SQLite read when a no-loss SIGNAL is valid -- ELSE the existing
compare-on-call / SQLite fallback (NO behavior loss). NO raw decommission, NO SQLite deletion, NO resolver
changes, NO cycles/stats/orient/explain/check change.
Depends: IMPORTS-LIVEGRAPH-DEFAULT-1 (the compare-on-call this optimizes), -REPOWIDE-READINESS-1 (GREEN-SAFE
evidence + the repo-wide compare that can BUILD a cert), CYCLES-COMPLETENESS-CERT-1 (`certificate_inputs_
fingerprint` -- the invalidation key). Track: Stage D, QUERY-MIGRATION-1 (decommission path).

## Why now (priority path)
```text
QUERY-AUTO-LAZY-SQLITE-1 removed the eager SQLite read for callers/callees/path. imports is the LAST
already-migrated default that STILL reads SQLite every served call -- by DESIGN (D2=B compare-on-call: the
per-call no-loss check). This slice removes that read when a no-loss signal is valid. It is RISKIER than the
lazy refactor (it trades the per-call guarantee for a cached/certified one), so spec-first + conservative.
```

## Grounding (EXECUTED 2026-06-07)
```text
COMPARE-ON-CALL (imports_auto_response, livegraph_feed.rs ~1490): reads `storage.find_imports` EVERY call (the
  no-loss baseline + the fallback answer), then live_import_view + file_partition_status (the precondition),
  then imports_auto_body decides served-livegraph (precondition met AND no missing AND no unknown) else
  labelled SQLite fallback. backend_used / fallback_reason emitted (JSON-only, stripped in human).
PRECONDITION available: file_partition_status(file) -> resident + Fresh + TS-primary (no SQLite).
INVALIDATION KEY EXISTS: certificate_inputs_fingerprint(snapshot, baseline) -- a deterministic digest over
  EVERY partition {epoch, fresh, ts, source_inputs_hash, producer_fingerprint} + the baseline {expected set,
  non-TS flag, repo_index_epoch, language_support_version, import_completeness_policy_version}. It covers BOTH
  sides of the no-loss claim: LiveGraph (partition epoch/hash/producer) AND SQLite (repo_index_epoch from the
  snapshot). ANY import-relevant change -> a different fingerprint. REUSABLE as-is.
NO VERDICT CACHE: the module-cycle cert is RECOMPUTED per audit call (not cached); the warm cache holds
  partition IR, not query verdicts; there is NO per-file or repo-level IMPORT no-loss cert/cache. -> B and C
  are NEW machinery (the SUPPORT module); the fingerprint is the only existing piece.
READINESS EVIDENCE (REPOWIDE-1): GREEN-SAFE -- 0 regression / 0 unknown over 1303 files; non-TS fallback by
  precondition. STRUCTURAL basis: SQLite TS imports are resolved-relative FILE/static ONLY (the homegrown
  ts-core extractor's limit) -> LiveGraph edges are a superset -> a regression would require the resolver to
  MISS a relative import SQLite resolved (handled by the overlay). The 0/1303 confirms the structural guarantee.
```

## Forced decisions — every cell filled (ratify at sign-off)

### D1 — Fastpath model (THE decision — force)
```text
A. PRECONDITION-ONLY: serve LiveGraph on precondition (resident+Fresh+TS) alone, trusting the MEASURED +
   STRUCTURAL no-loss. Zero per-call SQLite. RISK: a resolver regression on a file NOT in the measurement is
   not re-checked. [REJECT for now -- the user's explicit "do not jump to A without accepting measurement-only
   risk".]
B. PER-FILE NO-LOSS CACHE: the FIRST `imports <file>` runs compare-on-call (reads SQLite once), CACHES
   {file -> (no_loss: bool, fingerprint)}; subsequent calls with the SAME fingerprint serve LiveGraph WITHOUT
   SQLite; a fingerprint change invalidates. Fine-grained; first-call-per-file still reads SQLite.
C. REPO-LEVEL NO-LOSS CERTIFICATE [LEAN]: a repo cert {verdict: GREEN/RED, fingerprint} built by the EXISTING
   repo-wide compare (imports --engine compare, no-file -- it already computes 0-regression/0-unknown). If the
   cert is GREEN for the CURRENT fingerprint -> serve LiveGraph for ANY precondition-met TS file WITHOUT SQLite.
   RED / stale / missing -> compare-on-call. ONE SQLite amortization per repo-fingerprint, not per call.
D. KEEP COMPARE-ON-CALL: no fastpath. [the safe status quo if B/C are judged not worth the machinery.]
RECOMMENDATION: C. GREEN means EVERY TS file is no-loss -> a repo-level cert safely covers all of them; it
   reuses the repo-wide compare (no new compare logic) + the existing fingerprint; and RED/missing falls back to
   the proven compare-on-call (NO behavior loss). B is the finer-grained alternative if per-file invalidation is
   preferred over repo-level. NOT A (no measurement-only trust without explicit risk acceptance).
```

### D2 — Cert build trigger + storage (the new-machinery sub-decision — force)
```text
TRIGGER (when is the cert built?):
  T1. LAZY: the first default `imports <file>` for a repo at a new fingerprint runs the repo-wide compare,
      caches the cert, then serves. First-query latency = one repo-wide compare; subsequent = fastpath. [LEAN]
  T2. PROACTIVE: build at livegraph-refresh (the cert is ready before any query). Couples the fastpath to
      refresh; more refresh work. 
  T3. EXPLICIT: only `imports --engine compare` (repo-wide) builds/refreshes the cert; the default reads it,
      else compare-on-call. Cleanest separation; the user must run the compare once.
STORAGE:
  S1. IN-MEMORY per repo (RwLock<Option<{verdict, fingerprint}>> on RepoState, like the livegraph). Lost on
      daemon restart (rebuilt lazily). [LEAN -- simplest; the cert is cheap to rebuild.]
  S2. PERSISTED (a cache table / file). Survives restart; adds a storage surface + invalidation discipline.
RECOMMENDATION: T1 + S1 (lazy, in-memory). Simplest; the cert rebuilds cheaply on a fingerprint change or
   restart; no persisted surface. (T3 is the clean alternative if lazy first-query latency is unwanted.)
```

### D3 — Invalidation
```text
The cert carries the `certificate_inputs_fingerprint` it was built at. EVERY default call recomputes the CURRENT
fingerprint (cheap -- it is a digest over the resident partition snapshot + baseline) and compares: MATCH ->
the cert is valid ; MISMATCH (a re-index / refresh / swap / policy bump) -> the cert is STALE -> rebuild (T1)
or fall back to compare-on-call. NEVER serve a fastpath answer against a stale cert.
RECOMMENDATION: as written. Reuse certificate_inputs_fingerprint (it already covers partition epoch/hash/
producer + repo_index_epoch + policy version -- the user's full invalidation list).
```

### D4 — Fallback / rollout (NO behavior loss)
```text
The default `imports <file>` decision becomes:
  1. precondition UNMET (non-TS / non-resident / stale partition) -> SQLite fallback (UNCHANGED, labelled).
  2. precondition met AND a VALID GREEN cert -> FASTPATH: serve LiveGraph, NO SQLite read (backend_used=
     livegraph, fallback_reason=null). [the win]
  3. precondition met AND (cert RED / stale / missing) -> COMPARE-ON-CALL (the current D2=B path: read SQLite,
     verify no-loss, serve LiveGraph or fall back). [no behavior loss -- the proven path]
`--engine compare <file>` (per-file) + `--engine compare` (repo-wide, which BUILDS the cert) + `--engine
sqlite` + `--engine livegraph` UNCHANGED. The fastpath is an OPTIMIZATION layered over compare-on-call, not a
replacement -- a RED/stale/missing cert always degrades to the verified path.
RECOMMENDATION: as written. The fastpath only ever SKIPS SQLite when a GREEN cert proves it safe; otherwise the
existing compare-on-call runs -> output identical, just sometimes faster + SQLite-free.
```

### D5 — Scope
```text
TS-primary files only (the precondition gates). LiveGraph-served ONLY when the file's partition is resident +
Fresh + TS-primary (file_partition_status precondition met). Non-TS / non-resident -> SQLite fallback
(unchanged). The cert is per-REPO (C); B's cache is per-FILE. NO change to imports/cycles/stats/etc beyond the
imports default decision.
RECOMMENDATION: as written.
```

## Build contract (PROPOSED — gated on D1–D5 ratification; SUPPORT + IMPLEMENTATION)
```text
SUPPORT (the cert machinery):
  1. an in-memory repo IMPORT NO-LOSS CERT {verdict: GreenNoLoss | RedRegression | RedUnknown, fingerprint}
     on RepoState (RwLock), built from the EXISTING repo-wide compare (aggregate_readiness's verdict + the
     fingerprint). PURE verdict derivation unit-tested.
  2. build trigger (T1 lazy / T3 explicit per ratification) + the fingerprint check (D3).
IMPLEMENTATION (the fastpath):
  3. imports_auto_response: the D4 decision -- precondition -> (valid GREEN cert ? serve LiveGraph no-SQLite :
     compare-on-call) ; non-TS -> SQLite. Unit-tested: GREEN-cert -> no find_imports (panicking-closure style) ;
     stale/RED/missing -> compare-on-call ; non-TS -> SQLite.
  4. live: a TS file served via the cert reads NO SQLite (backend=livegraph) ; a fingerprint bump rebuilds ;
     a RED repo (forced) uses compare-on-call ; non-TS falls back. Gate + completion.
Stop if: a GREEN cert would serve a file the compare-on-call would have FALLEN BACK on (the cert must be a
  SUBSET-safe predicate -- GREEN repo-wide implies every TS file no-loss, so this cannot happen; assert it).
Stop if: the fingerprint does NOT change on an input the no-loss depends on (would serve a stale fastpath).
```

## Out of scope (hard guardrails)
```text
NO precondition-only (D1=A) without explicit risk acceptance ; NO raw decommission (SQLite still read to BUILD
the cert + on fallback) ; NO SQLite deletion ; NO resolver changes ; NO cycles/stats/orient/explain/check change ;
NO change to the explicit engines ; NO behavior loss (RED/stale/missing -> compare-on-call).
```

## Completion (IMPLEMENTED + LIVE-VALIDATED 2026-06-07, EXECUTED)

Commits: `02e2a50` (spec) -> `10e7602` (impl: live_partitions + RepoState.import_cert + ImportNoLossCert +
import_cert_fingerprint + the fastpath ladder + build_and_store_import_cert + imports_auto_response refactor).
UNPUSHED.

### Gate (EXECUTED 2026-06-07)
```text
cargo test --workspace -> no failures. clippy --workspace --all-targets -- -D warnings -> clean. fmt --check ->
clean. Unit (imports_fastpath_ladder): GREEN cert -> panicking find_imports NEVER called ; RED -> compare-on-
call ; stale/missing -> build then (green->fastpath / red->compare) ; build-failure(None) -> compare-on-call ;
non-TS -> sqlite.
```

### Live validation (EXECUTED 2026-06-07)
```text
FASTPATH (GREEN cert): amodx MediaPicker.tsx (1st call) -> builds the cert (GREEN), serves backend=livegraph,
  count=2 (the alias edges), comparison.source="repo_no_loss_certificate" (NO per-call find_imports). A SECOND
  file admin/src/main.tsx -> backend=livegraph, comparison.source=cert (served from the CACHED cert, no SQLite).
FALLBACK (non-TS): OpenXcom src/main.cpp -> backend=sqlite, fallback_reason=LiveGraphUnavailable, count=6
  (precondition unmet -> SQLite, the 6 C++ imports preserved).
COMPARE unchanged: amodx MediaPicker --engine compare -> comparison.status=NoLossLivegraphSuperset (per-file).
HUMAN unchanged: the default human render shows the 2 edges in the existing format, NO backend/comparison leak.
INVALIDATION: re-refresh amodx (partition epochs change -> the fingerprint changes) -> the next default query
  REBUILDS the cert (StaleOrMissing) and still serves the fastpath (GREEN) -- the cert correctly invalidated +
  rebuilt, no break.
```

### Acceptance (the ratified list) — PASS
```text
1. xpart/amodx fastpath serves LiveGraph WITHOUT SQLite after cert GREEN (comparison.source=cert; the 2nd file
   served from the cache).                                                                                PASS.
2. OpenXcom / non-TS falls back to SQLite (LiveGraphUnavailable, imports preserved).                      PASS.
3. compare mode still works (--engine compare -> the per-file directional compare).                       PASS.
4. No default loss (human byte-identical; the served `imports` array == compare-on-call's LiveGraph answer). PASS.
5. Invalidation: a fingerprint change rebuilds the cert (live re-refresh + the stale unit test).          PASS.
```

### Divergences / notes (recorded)
```text
- The cert build reuses imports_readiness_response (the repo-wide compare) -- it needs NO repo_root (the verdict
  is from the precond map + bulk all_imports, not the module-cycle baseline) -> imports_auto_response keeps its
  4-arg signature; handle_imports unchanged.
- The fastpath `imports` array is byte-identical to compare-on-call's served-LiveGraph answer; only the JSON
  `comparison` differs ({source: repo_no_loss_certificate} vs the per-call {sqlite_resolved_local, ...}) -- both
  JSON-only, stripped in human. NO import is lost.
- DECOMMISSION WIN: SQLite is read ONCE per fingerprint (to build the cert), not every served call. NOT a raw
  decommission (the cert build + the non-TS/RED fallback still read SQLite); the table stays.
- imports_auto_response (RepoState-bound) is not RepoState-unit-tested (siblings likewise) -- MITIGATED: the
  PURE imports_fastpath_or_compare (6 branches) is unit-tested + the end-to-end is live-validated.
- LIVE-VALIDATION DAEMON STATE: the manual release rmapd was restarted (pkill -x rmapd, exact; new binary +
  producer env) and xpart/amodx refreshed. Reported before acting. Re-run ./scripts/dev-install-local.sh to
  restore the launchd-managed daemon.
```

## References
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (`imports_auto_response` / `imports_auto_body` — the compare-on-call to fastpath; `aggregate_readiness` — the repo-wide verdict that BUILDS the cert)
- `rust/crates/repo-graph-livegraph/src/module_cycle_cert.rs` (`certificate_inputs_fingerprint` — the reusable invalidation key)
- `docs/slices/imports-livegraph-repowide-readiness-1.md` (the GREEN-SAFE evidence + the repo-wide compare)
- `docs/slices/imports-livegraph-default-1.md` (the compare-on-call this optimizes)
