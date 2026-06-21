# SQLITE-RAW-DECOMMISSION-READINESS-7: Transition Audit (delta) (Stage D)

Slice ID: SQLITE-RAW-DECOMMISSION-READINESS-7
Status: **AUDIT DELTA — evidence map. No code, no table deletion, no migration, no decommission.** A SHORT
recompute of READINESS-6 after the CYCLES thread landed: CYCLES-OUTPUT-CONTRACT-1 (canonical qualified+
deterministic `rmap cycles` output on BOTH backends, byte-identity proven) and CYCLES-LIVEGRAPH-DEFAULT-
FASTPATH-1 (the cert-gated LiveGraph default — the depth option READINESS-6 §Q5 recommended). Baseline:
READINESS-6. Track: Stage D. Gates any future SQLITE-RAW-DECOMMISSION-1.

## Evidence basis (this audit)
```text
Docs-only recompute (revision 2). LABELS: OBSERVED = inspected first-hand THIS audit (rmap output, code
file:line, git). INFERRED = my classification/judgment from those OBSERVED facts. OBSERVED-in-artifact = read
from the FASTPATH-1 / OUTPUT-CONTRACT-1 completion records, where it is recorded as EXECUTED by THOSE slices — I
did NOT re-run those live captures here (carried, not re-proven; see the ledger). This audit DID execute
read-only orientation + verification commands (rmap orient/trust/cycles/stats, git, grep) — the full list with
labels is in the "Validation / evidence ledger" below. No source code, no migration, no deletion was produced.
git: HEAD=82da168 (CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1 impl) [OBSERVED]. Working-tree status [OBSERVED]: no
tracked src/** | rust/** | scripts/** changes; the only untracked entries are THIS deliverable doc and the
`.agent-manager/` workflow artifacts — the audit produced exactly its docs-only deliverable and nothing else.
```

## Verdict (headline)
```text
`nodes`/`edges` STILL NOT ready to retire. NEW + MATERIAL this delta: cycles becomes the FIFTH default with a
SQLite-FREE served path (callers/callees/path/imports + NOW cycles). The default `rmap cycles` (module-import)
now serves the LiveGraph module cycles WITHOUT `find_cycles` on a valid GREEN repo no-loss cert; SQLite is read
ONCE per fingerprint to BUILD that cert (mirrors imports FASTPATH-1, reusing the SHARED fingerprint), plus on
the RED/non-TS/non-resident fallback. DISTINCT from every prior migration in this track: cycles is the FIRST
default whose served-path SQLite elimination required a ONE-TIME, USER-VISIBLE OUTPUT MIGRATION (short+Tarjan ->
qualified+canonical, applied to BOTH backends via CYCLES-OUTPUT-CONTRACT-1) to MAKE byte-identity true first;
callers/callees/path/imports were byte-preserving. The retirement is otherwise UNCHANGED: 5 defaults still read
SQLite EAGERLY (stats/orient/check/explain/trust); the cert BUILDs (imports + cycles) + all fallback paths +
non-TS still read nodes/edges; the 31 non-graph tables remain. [INFERRED from the OBSERVED facts below]
```

## Delta since READINESS-6 (what the two CYCLES slices changed — and did NOT)
```text
+ CYCLES-OUTPUT-CONTRACT-1 (3e2e8e0; D1=B/D2=B/D3=A) — OUTPUT MIGRATION, not a fastpath. [OBSERVED: slice doc +
  daemon-runtime/src/cycle_output.rs:63 canonical_module_cycles_json / :120 sqlite_module_cycles_json / :146
  livegraph_module_cycles_json / :53 module_basename]. Canonicalized BOTH backends' module-cycle render to
  QUALIFIED names + deterministic lexicographic order (sort+dedup by qualified_name; cycles sorted by their
  qualified-name vector); added the additive JSON `qualified_name`; node_id stays backend-native. The legacy
  SQLite default human output (short `src` + Tarjan/cycle_id order) CHANGED ONCE -> qualified
  `packages/a/src` + canonical order. This slice does NOT reduce the SQLite read; it UNBLOCKED the fastpath by
  PROVING default == `--engine livegraph --kind module-import` byte-for-byte (xpart + amodx) [OBSERVED-in-artifact].
+ CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1 (82da168, HEAD; D1=A compare-GREEN only) — the cert-gated default flip.
  [OBSERVED: state.rs:212 RepoState.cycles_cert (in-memory RwLock<Option<CycleNoLossCert>>, S1) ;
  livegraph_feed.rs:2199 build_and_store_cycles_cert (GREEN iff module_cycle_compare_data .is_exact() —
  missing=0 AND extra=0) ; :2220 cycles_fastpath_or_sqlite (the PURE ladder) ; :2247 cycles_auto_response ;
  :2270 reuses import_cert_fingerprint — the SHARED SQLite-free fingerprint ; :154/:172
  FallbackReason::LiveGraphCycleDivergence ; dispatch.rs:1328 handle_cycles, :1461 default `auto` ->
  cycles_auto_response ; rgr graph.rs:958 ("auto","")->AutoModule DEFAULT, :960 ("sqlite","")->SqliteModule
  forced escape hatch]. GREEN-cert + precondition (module-cycle answer-class == Exact) -> serve LiveGraph, NO
  find_cycles. OUTPUT-PRESERVING vs the NEW (post-OUTPUT-CONTRACT-1) canonical default.
~ unchanged: callers/callees/path LAZY (QUERY-AUTO-LAZY-SQLITE-1) ; imports GREEN-cert fastpath
  (FASTPATH-1, SQLite once/fingerprint). The explicit `--engine sqlite|livegraph|compare --kind module-import`
  routes are unchanged; the file-import kind is untouched. [OBSERVED: graph.rs:960-972]
- NOT changed: stats default SQLite (NO LiveGraph path) ; orient/check/explain/trust SQLite-only (coherence-
  pending) ; LiveGraph TS-only (non-TS always falls back) ; the imports + cycles cert BUILDs read SQLite
  once/fingerprint ; all fallback paths read nodes/edges ; the 31 non-graph tables. [OBSERVED: dispatch.rs:1243
  handle_stats / :2494 orient / :2616 check / :2678 explain / :2769 trust — none has a livegraph_feed served path]
```

## Default-path SQLite-read audit (UPDATED 2026-06-08 — the READINESS-6 table, re-measured)
```text
COMMAND   DEFAULT   SERVED-PATH READS nodes/edges?   WHEN SQLite IS READ
callers   Auto      NO (lazy)                         only on FALLBACK (non-resident / non-TS / stale / !Exact)
callees   Auto      NO (lazy)                         only on FALLBACK
path      Auto      NO (lazy)                         only on FALLBACK
imports   Auto      NO on the GREEN-cert fastpath     cert BUILD (once/fingerprint) + FALLBACK (non-TS/RED/stale)
cycles    Auto      NO on the GREEN-cert fastpath     cert BUILD (once/fingerprint) + FALLBACK (RED/non-TS/
            (was sqlite in R6)                        non-resident -> find_cycles); NEW THIS DELTA
stats     sqlite    YES (eager, every call)           the served answer (NO LiveGraph path exists)
orient    sqlite    YES (eager)                       agent reads storage (coherence-pending)
check     sqlite    YES (eager)                       agent reads storage (coherence-pending)
explain   sqlite    YES (eager)                       agent reads storage (coherence-pending)
trust     sqlite    YES (eager)                       service reads storage (coherence-pending)

=> 5/10 defaults now have a SQLite-FREE COMMON (served) path (callers/callees/path/imports/cycles). 5/10 still
   read nodes/edges EAGERLY as the served answer. (READINESS-6: 4 served-free / 6 eager -> now 5 / 5.)
   [OBSERVED for the 5 served-free + the SQLite-only 5; the GREEN/RED per-repo split is OBSERVED-in-artifact.]
```

## Audit questions (the operator's recompute — answered)
```text
Q1 — Which defaults now COMMONLY serve WITHOUT SQLite?
  FIVE: callers, callees, path (lazy — the served Auto path reads nothing; SQLite only on fallback), imports
  (GREEN repo cert -> LiveGraph, no find_imports), and NOW cycles (GREEN repo cert -> LiveGraph module cycles,
  no find_cycles). cycles is the NEW entry this delta and the SECOND cert-gated default (after imports). The
  "common" path is the post-cert-build steady state on a GREEN TS repo (xpart, amodx); a non-TS / RED / non-
  resident repo (e.g. repo-graph-self, whose excluded fixture cycle makes its cert RED) falls back. [INFERRED
  from OBSERVED code; per-repo GREEN/RED OBSERVED-in-artifact: FASTPATH-1 completion — xpart count=1, amodx
  count=3 backend=livegraph; repo-graph count=6 backend=sqlite fallback_reason=LiveGraphCycleDivergence.]

Q2 — Which still read SQLite EAGERLY (every call, as the served answer)?
  FIVE: stats, orient, check, explain, trust. None migrated; each reads nodes/edges every call. (cycles LEFT
  this set this delta; READINESS-6 had SIX here.) stats has NO LiveGraph path at all; orient/check/explain/trust
  are the SQLite-only coherence cluster (agent/service reads storage). [OBSERVED: dispatch.rs handlers above;
  no livegraph_feed served path for any of the five.]

Q3 — Which use SQLite ONLY to build a CERT / safety predicate (not per served call)?
  TWO, both via the SAME machinery: imports (build_and_store_import_cert, livegraph_feed.rs:1668) and cycles
  (build_and_store_cycles_cert, :2199). Each runs the repo-wide compare ONCE per fingerprint -> GREEN iff the
  no-loss compare is exact (imports: directional no-loss; cycles: missing=0 AND extra=0) -> stores {verdict,
  fingerprint} on RepoState; subsequent GREEN calls serve LiveGraph with NO storage read. They share ONE
  SQLite-free fingerprint helper (import_cert_fingerprint, :1589, reused at :2270) keyed by partitions +
  snapshot_uid. The cert build reads SQLite; the served call does not. (Separately, the EXPLICIT `--engine
  compare` for callers/callees/path/imports/cycles reads SQLite by design — explicit, not a default.) [OBSERVED]

Q4 — What remains before `nodes`/`edges` can be retired?
  (a) DEFAULT-migrate stats — the LAST drilldown command with NO LiveGraph path; a REAL build (degree/complexity
      over the IR + measurements the IR lacks), not a cert-flip. [OBSERVED: no stats livegraph path]
  (b) COHERENCE LAYER for orient/check/explain/trust (SQLite-only; mixed live+persisted — design-first).
  (c) NON-TS coverage: LiveGraph is TS-only -> every non-TS file/repo FALLS BACK to SQLite. nodes/edges stay
      load-bearing for C/C++/Rust/Java until non-TS LiveGraph exists.
  (d) the FALLBACK paths of the 5 migrated defaults (non-resident / non-TS / stale / RED) read SQLite -> the
      table is needed for safety even where the common path is free.
  (e) the imports + cycles CERT BUILDs read SQLite once/fingerprint -> a FULLY SQLite-free imports/cycles default
      needs the cert source itself to become SQLite-free (a STRUCTURAL no-loss proof, not a SQLite compare) — a
      deeper change for BOTH cert-gated commands now.
  (f) the 31 non-graph tables (out of scope for nodes/edges; the broader decommission).
  nodes/edges become droppable ONLY when EVERY default served path is SQLite-free AND the fallback is removed/
  covered (non-TS + residency) AND the cert builds are SQLite-free — (a)+(b)+(c)+(d)+(e). [INFERRED]

Q5 — Highest-value next slice? GOAL-DEPENDENT — VALIDATED against repo state:
  - CERT-FASTPATH LEVERAGE IS NOW EXHAUSTED for the migrated commands. [INFERRED, OBSERVED-backed] Every command
    that HAD the explicit LiveGraph + compare + cert machinery — imports and cycles — is now flipped; callers/
    callees/path are lazy. There is no 6th migrated command with cert machinery waiting to fastpath-flip. The
    next decommission step is NOT another fastpath.
  - QUERY-MIGRATION BREADTH -> STATS-LIVEGRAPH-1, SPEC-FIRST. stats is the only remaining drilldown default that
    is SQLite-only with NO LiveGraph path — it is a real build (needs the IR degree graph + measurements), so
    spec the data dependency first. This is the highest-value next slice for advancing the served-free count.
  - COHERENCE LAYER (orient/check/explain/trust) -> AFTER stats. Design-first; higher blast radius (mixed
    live+persisted contract; agent/service read path). Do not start before a coherence design slice.
  RECOMMEND the DELTA verdict first (this doc), then STATS-LIVEGRAPH-1 spec-first (breadth), then the coherence
  design slice. Further cert fastpaths are exhausted for the currently migrated commands. [INFERRED]
```

## Deletion gates (READINESS-1 §5; current status — ALL still FAIL, two ADVANCED)
```text
1 no default command depends on nodes/edges      -> FAILS, but ADVANCED: 5/10 defaults have a SQLite-FREE served
                                                    path (was 4/10). 5/10 eager + all fallbacks + 2 cert builds.
2 LiveGraph covers SAME data for ALL languages   -> FAILS. TS-only; non-TS always falls back.
3 migration / back-compat story                  -> the 5 migrated defaults have it (Auto + labelled fallback +
                                                    byte-compatible human + lazy/cert). NEW NUANCE: cycles
                                                    required a ONE-TIME, ratified OUTPUT MIGRATION (CYCLES-
                                                    OUTPUT-CONTRACT-1) to reach byte-identity — the first in this
                                                    track to change the human default; stats/coherence: none.
4 operator reset story                           -> not reachable.
5 per-command parity tests on the new backend    -> callers/callees/path: lazy + panicking-closure proof.
                                                    imports: GREEN-SAFE no-loss (0 regression/1303 files) + cert.
                                                    cycles ADVANCED: the module-cycle compare + the canonical
                                                    adapter-parity test + the panicking-SQLite-closure GREEN
                                                    proof (livegraph_feed.rs:2326+). stats/orient/check/explain/
                                                    trust: none. [OBSERVED]
```

## Remaining blockers (the operator's recompute, confirmed)
```text
- stats default still SQLite (every call; NO LiveGraph path)               -> unmigrated; a real build.
- orient / check / explain / trust                                        -> SQLite-only (coherence-pending).
- non-TS languages                                                        -> LiveGraph TS-only (always fallback).
- the 5 migrated defaults' FALLBACK paths                                 -> still read nodes/edges.
- the imports + cycles cert BUILDs                                        -> SQLite once/fingerprint (not free).
- the 31 non-graph tables                                                 -> the broader decommission.
```

## Validation / evidence ledger (this audit — revision 2)
```text
EXECUTED (command run, output observed first-hand THIS audit):
- git log -1            -> HEAD=82da168 "CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1 (impl, D1=A): cert-gated LiveGraph
                          cycles default (UNBLOCKED post OUTPUT-CONTRACT-1)". Confirms the baseline commit.
- git status --short    -> only `?? docs/slices/sqlite-raw-decommission-readiness-7.md` + `?? .agent-manager/`.
                          No tracked source / migration / deletion. Backs the docs-only guardrail + Evidence basis.
- rmap orient --budget small -> Repo: repo-graph; Confidence: low; call resolution 19% (11189/69206); enrichment
                          phase did NOT run (syntax-only extraction). High-level orientation (rmap 0.2.1, daemon).
- rmap trust            -> Call-graph / Import-graph / Change-impact ALL reliability=LOW; 4873 unresolved imports;
                          "Suspicious Modules (zero connectivity)" lists `rust` + `rust/crates/*` (the non-TS
                          source). First-hand CORROBORATION of Q4(c) / blocker "LiveGraph TS-only -> non-TS falls
                          back" — the Rust crates carry no resolved call graph.
- rmap cycles           -> "6 module-level cycles found", rendered as QUALIFIED canonical names (e.g.
                          `src/core/classification -> src/core/ports`; xpart fixture `packages/a/src ->
                          packages/b/src`). First-hand CORROBORATION of: (i) CYCLES-OUTPUT-CONTRACT-1 canonical
                          qualified+ordered output is live on the default; (ii) repo-graph-self count=6 — the
                          RED-cert SQLite-fallback figure the Verdict/Q1 cite (the excluded xpart fixture cycle
                          keeps this repo's cert RED, so its default `cycles` falls back to SQLite).
- rmap stats            -> serves Module Stats (modules=306, files=1133, symbols=3699). First-hand CORROBORATION
                          that stats SERVES a real size/symbol build — Q4(a) "a real build, not a cert-flip".
- grep -n (read-only)   -> every file:line this doc cites verified at the stated line THIS audit:
                          state.rs:212 cycles_cert; livegraph_feed.rs build_and_store_cycles_cert:2199 /
                          cycles_fastpath_or_sqlite:2220 / cycles_auto_response:2247 / import_cert_fingerprint
                          def:1589 reused:2270 / LiveGraphCycleDivergence:154+:172 / panicking-SQLite-closure
                          GREEN tests :2350,:2391,:2405; cycle_output.rs module_basename:53 /
                          canonical_module_cycles_json:63 / sqlite_module_cycles_json:120 /
                          livegraph_module_cycles_json:146; dispatch.rs handle_stats:1243 / handle_cycles:1328 /
                          default auto->cycles_auto_response:1461 / handle_orient:2494 / handle_check:2616 /
                          handle_explain:2678 / handle_trust:2769; rgr graph.rs AutoModule default("auto","")
                          :958 / SqliteModule escape hatch("sqlite","") :960.

OBSERVED (artifact inspected directly, not executed):
- docs/slices/sqlite-raw-decommission-readiness-6.md (baseline) + -5..-1 (precedent structure this doc mirrors).
- docs/slices/cycles-output-contract-1.md, cycles-livegraph-default-fastpath-1.md,
  imports-livegraph-default-fastpath-1.md (the landed slices this delta recomputes, incl. their recorded
  EXECUTED live captures — the source of every OBSERVED-in-artifact claim).

NOT RUN (skipped, with reason):
- Build / test (cargo, ./scripts/dev-install-local.sh): NOT RUN — packet states "docs-only; no build/test
  required". No code path was touched, so no build is owed.
- Re-execution of the FASTPATH-1 / OUTPUT-CONTRACT-1 LIVE captures (xpart count=1 + amodx count=3 LiveGraph
  fastpath; repo-graph SQLite fallback; gate green): NOT RUN — recorded as EXECUTED in those slices' completion
  artifacts; re-running needs their multi-repo fastpath fixtures and a clean-cert state, out of scope for a
  docs-only recompute. They are carried as OBSERVED-in-artifact, NOT presented as first-hand EXECUTED here. The
  repo-graph-self side (count=6 -> RED-cert SQLite fallback) WAS re-confirmed first-hand via `rmap cycles`.

DISCREPANCY (recorded per evidence law, NOT presented as reconciled):
- `rmap orient` reports "3 import cycles (largest: 2 modules)" while `rmap cycles` enumerates 6 (largest = 3
  modules: `src -> ... -> src`). [OBSERVED both]. INFERRED cause: orient emits a budget-limited / curated cycle
  SIGNAL ("Some signals omitted due to budget"), computed/filtered differently from the full canonical `rmap
  cycles` enumeration (which includes the test-fixture + tools cycles). NOT investigated — orient's signal
  pipeline is OUT OF SCOPE for this SQLite-decommission recompute and bears on NO audit claim: the load-bearing
  fact (the `cycles` DEFAULT serves canonical qualified output; repo-graph-self=6 -> SQLite fallback) is
  first-hand confirmed by `rmap cycles` above.
```

## Guardrails honored
```text
No code. No table deletion. No migration. No decommission. No default flip. Audit-delta doc only. The
CYCLES-OUTPUT-CONTRACT-1 + CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1 changes are RECORDED, not re-done; nodes/edges
untouched. Read-only orientation (rmap orient/trust/cycles/stats; git log/status; grep over the cited source)
used to back OBSERVED claims first-hand — see the Validation / evidence ledger above.
```

## References
- `docs/slices/sqlite-raw-decommission-readiness-6.md` (baseline) + `-5.md` / `-4.md` / `-3.md` / `-2.md` / `-1.md`
- `docs/slices/cycles-output-contract-1.md` (D1=B canonical qualified+deterministic output; byte-identity proven — the UNBLOCKER)
- `docs/slices/cycles-livegraph-default-fastpath-1.md` (the cert-gated cycles default — the 5th SQLite-free served default)
- `docs/slices/imports-livegraph-default-fastpath-1.md` (the cert-fastpath pattern + the SHARED SQLite-free fingerprint cycles reuses)
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (`cycles_auto_response` :2247 / `build_and_store_cycles_cert` :2199 / `cycles_fastpath_or_sqlite` :2220 / `import_cert_fingerprint` :1589 shared)
- `rust/crates/daemon-runtime/src/cycle_output.rs` (`canonical_module_cycles_json` — both backends' canonical render)
- `rust/crates/daemon-runtime/src/dispatch.rs` (`handle_cycles` :1328 default `auto`; `handle_stats` :1243 / orient :2494 / check :2616 / explain :2678 / trust :2769 — the SQLite-only eager defaults)
