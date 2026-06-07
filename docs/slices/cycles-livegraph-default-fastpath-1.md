# CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1: cert-gated LiveGraph default for `rmap cycles`

Slice ID: CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1
Status: **RATIFIED (D1=A compare-GREEN only [missing=0 && extra=0 && unknown=0]; D2–D5 as written — 2026-06-07).
BUILD IN PROGRESS.** `Complete` is NOT the gate (stricter than no-loss; would block amodx); UnknownDivergence /
unresolved compare -> non-GREEN -> SQLite. Flip the DEFAULT
`rmap cycles` (MODULE-import, SQLite `find_cycles` every call) to a cert-gated LiveGraph fastpath: serve the
LiveGraph MODULE cycles WITHOUT reading SQLite when a GREEN repo no-loss certificate is valid for the current
fingerprint; ELSE the SQLite fallback (NO loss). Mirrors IMPORTS-LIVEGRAPH-DEFAULT-FASTPATH-1, reusing the
EXISTING module-cycle compare + certificate machinery. NO raw decommission, NO SQLite deletion, NO non-TS
support, NO resolver changes, NO module-identity change.
Depends: MODULE-CYCLES-CLI-1 (the explicit `--engine livegraph|compare --kind module-import`), CYCLES-
COMPLETENESS-CERT-1 (`evaluate_module_cycle_completeness` + `certificate_inputs_fingerprint`), MODULE-CYCLES-
COMPARE-CLASSIFY-1 (the compare missing/extra), IMPORTS-LIVEGRAPH-DEFAULT-FASTPATH-1 (the cert-fastpath pattern
+ the SQLite-free fingerprint). Track: Stage D, QUERY-MIGRATION-1 (decommission path).

## Why now (priority path)
```text
READINESS-6: 4/10 defaults now have a SQLite-free served path; cycles is the highest-leverage REMAINING
decommission because it ALREADY has the explicit LiveGraph module cycles + the compare + the completeness
certificate -- a cert-gated default flip MIRRORS the imports fastpath, reusing that machinery (no new compare
logic). GREEN/EXACT real repos (amodx) serve LiveGraph; repo-graph (excluded-fixture / non-TS) + OpenXcom fall
back (READINESS-2 evidence).
```

## Grounding (EXECUTED 2026-06-07)
```text
DEFAULT cycles (dispatch.rs handle_cycles): engine="sqlite" (no flag) -> find_cycles(snapshot,"module") EVERY
  call -> {repo_uid, display_name, snapshot_uid, cycles, count}. The explicit --engine livegraph|compare --kind
  module-import already exist (MODULE-CYCLES-CLI-1).
COMPARE verdict (module_cycle_compare_response): { sqlite_count, livegraph_count, missing_in_livegraph: Vec,
  extra_in_livegraph: Vec, ... }. NO-LOSS = missing.is_empty() AND extra.is_empty(). READINESS-2: xpart EXACT,
  amodx EXACT (missing=0), hexmanos/zap-engine EXACT, repo-graph missing=1 (the EXCLUDED fixture a<->b cycle),
  OpenXcom non-TS (LiveGraph empty -> all missing). extra=0 EVERYWHERE.
LiveGraph ANSWER to serve: module_import_cycles_response (the LiveGraph MODULE cycles, backend=livegraph).
CERT MACHINERY EXISTS: evaluate_module_cycle_completeness (the Complete predicate, SQLite-free EXCEPT its
  baseline reads SQLite languages) ; certificate_inputs_fingerprint (the full invalidation key) ; the imports
  SQLite-FREE fingerprint (partitions + snapshot_uid + policy) -- REUSABLE for cycles.
CONTRADICTION (the user's rule 1): "Complete AND compare-GREEN" -- but amodx is INCOMPLETE (workspace-local) yet
  compare-EXACT. Requiring Complete makes amodx FALL BACK, contradicting the expected "amodx fastpath". =>
  compare-GREEN is the operative no-loss predicate (D1).
```

## Forced decisions — every cell filled (ratify at sign-off)

### D1 — Default predicate (the contradiction — force)
```text
A. COMPARE-GREEN ONLY [LEAN]: the cert verdict = GREEN iff the cycles compare has missing_in_livegraph EMPTY
   AND extra_in_livegraph EMPTY (no SQLite cycle lost, no over-claimed extra). Complete is NOT required.
   -> xpart GREEN, amodx GREEN (EXACT despite Incomplete), repo-graph RED (missing=1), OpenXcom RED (non-TS).
   Mirrors imports FASTPATH-1 (the no-loss compare). MATCHES the user's expected.
B. COMPLETE AND COMPARE-GREEN: requires the module-cycle cert Complete TOO. -> amodx (Incomplete, workspace-
   local) FALLS BACK -> CONTRADICTS the expected (amodx fastpath). REJECT.
C. COMPLETE OR COMPARE-GREEN: Complete (a SQLite-free* shortcut) OR compare-GREEN. The Complete branch adds
   NOTHING the compare-GREEN doesn't already fire on (xpart is compare-GREEN too), and Complete's baseline reads
   SQLite languages anyway (*not actually SQLite-free to build). Redundant complexity.
RECOMMENDATION: A. The compare-GREEN (missing=0 AND extra=0) is the operative no-loss predicate; it fires for
  EXACTLY the expected repos and mirrors the imports fastpath. The user's "Complete AND" contradicts the amodx
  expected, so Complete is NOT a gate. (Fresh/resident is the precondition -- a non-resident/stale partition is
  not GREEN; the LiveGraph answer carries the answer-class, and a non-Exact LiveGraph -> fallback.)
```

### D2 — Cert source + storage (mirror imports)
```text
A repo-level CYCLES no-loss cert {verdict: GREEN/RED, fingerprint} on RepoState (in-memory RwLock, S1; rebuilt
on restart), BUILT by the EXISTING module_cycle_compare_response (missing/extra -> GREEN). Keyed by the SQLite-
FREE fingerprint REUSED from imports FASTPATH-1 (partitions {epoch/fresh/ts/hash/producer} + snapshot_uid +
policy version) -- it captures BOTH the LiveGraph cycles (partitions) AND the SQLite cycles (snapshot/index
epoch). A fingerprint mismatch invalidates + rebuilds. The fingerprint helper is SHARED (rename to a neutral
`livegraph_no_loss_fingerprint`, or keep + reuse).
RECOMMENDATION: as written. Reuse the imports fingerprint + the module-cycle compare -- no new compare logic, no
  new invalidation key.
```

### D3 — Runtime behavior (the fallback ladder)
```text
The DEFAULT cycles (no --engine, MODULE kind) becomes:
  1. precondition UNMET (no resident TS partition / a non-resident or stale contributing partition / non-TS) ->
     SQLite fallback (find_cycles, labelled). [repo-graph non-TS partition, OpenXcom]
  2. precondition met AND a VALID GREEN cert -> FASTPATH: serve the LiveGraph MODULE cycles, NO find_cycles
     (backend_used=livegraph). [xpart, amodx]
  3. precondition met AND (cert RED / stale / missing / build-failed) -> SQLite fallback (find_cycles). The cert
     is LAZILY built on the first eligible default call (T1) via the compare; build reads SQLite ONCE per
     fingerprint. [repo-graph missing=1 -> RED -> SQLite]
NO behavior loss: a RED/stale/missing cert always serves the SQLite cycles (the proven answer). repo-graph is
the EXPECTED fallback (its excluded fixture cycle makes the compare RED).
RECOMMENDATION: as written. Identical shape to imports FASTPATH-1 (precondition -> GREEN cert ? LiveGraph : SQLite).
```

### D4 — Output compatibility
```text
HUMAN default stays MODULE-CYCLE-compatible: the fastpath maps the LiveGraph MODULE cycles into the SQLite-
  compatible {repo_uid, display_name, snapshot_uid, cycles, count} shape so the cycles renderer is byte-
  unchanged. backend_used / fallback_reason are JSON-ONLY, STRIPPED in human (the QUERY-MIGRATION-CLI-1 +
  imports precedent). The RICH livegraph trust envelope (answer_class/freshness/scope) stays on the EXPLICIT
  `--engine livegraph`, NOT the default.
JSON: + backend_used ("livegraph"|"sqlite") + fallback_reason (null on the fastpath). The default {cycles,count}
  contract is preserved; the extra fields are additive.
EXPLICIT `--engine sqlite|livegraph|compare --kind module-import` UNCHANGED. FILE-import kind UNCHANGED.
RECOMMENDATION: as written. The fastpath maps LiveGraph cycles -> the SQLite cycle shape (like imports edges ->
  ImportEntry); the human is byte-compatible; the metadata is JSON-only.
```

### D5 — Scope + validation
```text
SCOPE: the MODULE-import cycles DEFAULT only. NO file-import default change. NO non-TS support (non-TS -> SQLite
  fallback). NO resolver / module-identity change. explicit engines unchanged.
VALIDATION (post-build, EXECUTED): xpart/amodx default -> LiveGraph fastpath (backend=livegraph, NO find_cycles,
  cycles == the SQLite cycles); repo-graph default -> SQLite fallback (RED cert, missing=1); OpenXcom/non-TS ->
  SQLite fallback; NO default LOSES a SQLite cycle (the GREEN cert proves it; a RED -> SQLite); the human render
  is byte-unchanged; `--engine compare --kind module-import` route unchanged; a fingerprint bump rebuilds.
```

## Build contract (PROPOSED — gated on D1–D5 ratification; SUPPORT + IMPLEMENTATION)
```text
SUPPORT:
  1. RepoState.cycles_cert (in-memory RwLock<Option<{verdict, fingerprint}>>, S1).
  2. build_and_store_cycles_cert: run module_cycle_compare -> GREEN iff missing.is_empty() && extra.is_empty();
     store {verdict, fingerprint} keyed by the SHARED SQLite-free fingerprint. PURE verdict derivation tested.
  3. share the imports SQLite-free fingerprint (rename to `livegraph_no_loss_fingerprint` or reuse in place).
IMPLEMENTATION:
  4. a PURE cycles_fastpath_or_sqlite ladder (precondition -> GREEN cert ? serve LiveGraph module cycles (mapped
     to the SQLite cycle shape, backend=livegraph) : SQLite find_cycles). Unit-tested: GREEN -> panicking
     find_cycles NEVER called ; RED/stale/build-fail -> SQLite ; non-TS/non-resident -> SQLite.
  5. handle_cycles: the DEFAULT (no --engine, module kind) -> the ladder ; explicit engines unchanged ; the CLI
     strips backend_used/fallback_reason for the human (the precedent).
  6. live: xpart/amodx fastpath ; repo-graph/OpenXcom fallback ; human byte-compatible ; compare unchanged ;
     fingerprint-bump rebuild. Gate + completion.
Stop if: a GREEN cert would serve a file/repo the compare would have flagged missing (the cert IS the compare
  verdict, so this cannot happen; assert it). Stop if the LiveGraph cycle -> SQLite-shape mapping changes the
  human bytes for the SAME cycle set.
```

## Out of scope (hard guardrails)
```text
NO raw decommission (SQLite read to BUILD the cert + on fallback) ; NO SQLite deletion ; NO non-TS support ; NO
resolver / module-identity change ; NO file-import default change ; NO change to the explicit engines ; NO new
cycle classes. repo-graph/non-TS FALLBACK is EXPECTED, not a regression.
```

## References
- `rust/crates/daemon-runtime/src/dispatch.rs` (handle_cycles — the SQLite default `find_cycles` + the engine routing)
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (`module_cycle_compare_response` — missing/extra ; `module_import_cycles_response` — the LiveGraph answer ; `imports_fastpath_or_compare` / the fingerprint — the pattern to mirror)
- `rust/crates/repo-graph-livegraph/src/module_cycle_cert.rs` (`evaluate_module_cycle_completeness` / `certificate_inputs_fingerprint`)
- `docs/slices/imports-livegraph-default-fastpath-1.md` (the cert-fastpath pattern) + `docs/slices/cycles-default-migration-readiness-2.md` (the amodx-EXACT / repo-graph-missing evidence)
