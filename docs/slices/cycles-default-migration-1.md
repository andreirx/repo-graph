# CYCLES-DEFAULT-MIGRATION-1: can `rmap cycles` default become LiveGraph-first?

Slice ID: CYCLES-DEFAULT-MIGRATION-1
Status: **DEFERRED / NOT READY (ratified 2026-06-04). NOT a build slice.** The decision is recorded below
(see **Deferral**). The `rmap cycles` default STAYS SQLite. The prerequisite is CYCLES-COMPLETENESS-CERT-1.
Original spec (the analysis that drove the deferral) preserved below.

## Deferral (ratified 2026-06-04)
```text
DECISION: DEFER the cycles default migration. Do NOT build A+P2. The `rmap cycles` default REMAINS SQLite.
REASONS (ratified):
- P1 is UNSAFE: Exact+Fresh+TS-only still missed the repo-graph non-TS module cycle (READINESS-1, repo-graph).
- P2 is SAFE but NOT a migration: it reads SQLite on EVERY default call and changes only `backend_used` --
  no decommission, no perf, byte-identical answer when equivalent.
- P3 is the REAL migration predicate, but needs whole-repo language/partition COMPLETENESS CERTIFICATION
  that does not exist yet.
- A default migration that does NOT remove the SQLite dependency is not worth the added complexity.
- A default migration that drops non-TS / unloaded cycles is FORBIDDEN.
PREREQUISITE (recorded, not yet prioritized): CYCLES-COMPLETENESS-CERT-1 (docs/slices/cycles-completeness-cert-1.md)
  -- certify LiveGraph whole-cycle-graph coverage WITHOUT consulting SQLite every query. Only after it can
  `rmap cycles` default become LiveGraph-first without compare-every-call.
KEEP (unchanged): `--engine livegraph --kind module-import`; `--engine compare --kind module-import`; the
  readiness harness (scripts/measure-module-cycle-readiness.sh); the SQLite default.
```

DECIDE whether the `rmap cycles` DEFAULT
(SQLite MODULE) can become LiveGraph-first with a labelled SQLite fallback — WITHOUT ever silently dropping a
SQLite module cycle. Spec only; NO implementation. NO raw decommission, NO deletion, NO SQLite removal, NO
package resolver.
Depends: MODULE-CYCLES-CLI-1 (the routes + compare), MODULE-CYCLES-COMPARE-CLASSIFY-1 (the classifier),
MODULE-CYCLES-DEFAULT-READINESS-1 (the YELLOW measurement), QUERY-MIGRATION-CLI-1 (the `Engine::Auto`
pattern for callers/callees/path). Track: Stage D.

## Goal
```text
The READINESS-1 measurement is YELLOW: every real TS repo's LiveGraph module cycles are EXACT vs SQLite, the
only divergence is non-TS (Rust) cycles in a mixed repo, Unknown=0/Extra=0. Decide if the default can serve
LiveGraph when it is PROVABLY COMPLETE for the repo, falling back to a LABELLED SQLite answer otherwise --
the labeled-degradation candidate the verdict named. The HARD constraint: no default call may lose a SQLite
cycle silently.
```

## Grounding (EXECUTED 2026-06-03)
```text
Engine::Auto (callers/callees/path; livegraph_feed.rs auto_outcome): serve LiveGraph iff Fresh AND
  class==Exact AND ts_only; else a LABELLED SQLite fallback (FallbackReason::LiveGraph{Unavailable,Stale,
  Partial,UnsupportedLanguage}). This is PER-SYMBOL: the loaded partitions answer for the queried symbol.
CYCLES IS WHOLE-GRAPH, and the same predicate is PROVEN INSUFFICIENT: repo-graph's LiveGraph module cycles
  are Exact+Fresh+ts_only (the loaded TS src/ partitions ARE exact) YET INCOMPLETE -- SQLite has 6, the
  LiveGraph has 5; the missing 1 is a RUST module cycle that was NEVER a LiveGraph partition, so it is ABSENT
  (not in `missing_partitions`), and `Exact` does NOT see it (READINESS-1, repo-graph row).
=> a whole-graph cycle answer cannot be certified COMPLETE from the LiveGraph's self-reported Exact+Fresh:
   the LiveGraph is blind to (a) non-TS files and (b) TS partitions it has not loaded (F2 no-enumeration).
The COMPARE is the only thing that currently DETECTS this (it diffs vs SQLite find_cycles).
```

## THE CRUX — the completeness predicate (forced decision D2)
```text
"Serve LiveGraph when complete" needs a COMPLETENESS PREDICATE for a whole-graph answer. Three candidates:

P1 — Exact+Fresh+ts_only ONLY (the callers/callees predicate).   REJECTED: PROVEN to silently drop non-TS
     cycles (repo-graph). Violates the hard constraint. Not an option.
P2 — COMPARE-VERIFIED: run SQLite find_cycles + module_import_cycles + the diff EVERY default call; serve
     LiveGraph iff missing==0 AND extra==0 AND Unknown==0 (the READINESS rules). CORRECT + safe. BUT it runs
     SQLite on every call -> ZERO decommission benefit (SQLite still read every time), and when the two are
     equivalent the served LiveGraph answer is BYTE-IDENTICAL to SQLite -> the migration changes only
     `backend_used`, not the answer or the dependency.
P3 — CHEAP COMPLETENESS PROXY: serve LiveGraph iff Exact+Fresh+ts_only AND (repo is TS-ONLY) AND (the
     LiveGraph FILE-node set covers ALL of SQLite's TS files). Avoids the per-call cycle compare. BUT it
     needs signals NOT BUILT: a language-composition check (SQLite files table / registry) AND whole-repo
     ENUMERATION (knowing the repo's FULL TS partition set, to certify "all loaded" -- the deferred F2).
     Not achievable this slice.
```

## Forced decisions (to ratify at sign-off) — every cell filled

### D1 — the model (A vs B; the user's key decision)
```text
A. AUTO with labelled SQLite fallback on any non-complete answer (the Engine::Auto family).   [RECOMMENDED]
B. LiveGraph default with a DEGRADED/Partial answer (omits non-TS cycles unless --engine sqlite).
RECOMMENDATION: A. B silently (or semi-silently) drops the non-TS cycles from the default human output --
unacceptable unless the degradation is loud + agreed, and the READINESS verdict explicitly forbids silent
drops. A preserves default COMPLETENESS (the SQLite answer is served whenever the LiveGraph is not provably
complete). REJECT B.
```

### D2 — the completeness predicate (THE crux; pick P1/P2/P3)
```text
P1 REJECTED (silent non-TS drop). P3 is BLOCKED (needs language-composition + F2 enumeration, not built).
=> the only CORRECT, BUILDABLE predicate today is P2 (compare-verified).
RECOMMENDATION: P2 -- BUT note its honest cost (below). If P2's "no benefit" is unacceptable, the right move
is DEFER this migration until P3's prerequisites (ENUMERATION + a TS-only/file-coverage certification) exist;
that is a legitimate verdict for this spec.
```

### D3 — run the compare every call, or gate it? (the user's #2)
```text
With P2, the compare (SQLite find_cycles + diff) runs on EVERY default call -- there is NO cheap "clearly
complete" signal for a whole-graph answer (D2: P3 is blocked). So the user's "compare only when not clearly
complete" cannot be honored for cycles yet: clearly-complete CANNOT be determined without the compare (or
P3's missing signals). RECOMMENDATION: accept compare-every-call under P2, OR DEFER. (A precompute/cache is
possible but invalidates on every refresh + every SQLite re-index -> same cost, more complexity.)
```

### D4 — human output compatibility (the user's #3)
```text
If FALLBACK (SQLite served): the human output is the EXISTING SQLite module render -- BYTE-UNCHANGED (no
regression). If LIVEGRAPH served (P2 equivalent): the answer is identical cycles, so render via the SAME
MODULE renderer (render_human) so the default human output is module-cycle-compatible either way; the
`backend_used`/`fallback_reason` ride in JSON only (the human format does not change). [RECOMMENDED]
```

### D5 — JSON metadata (the user's #4)
```text
Add to the default cycles JSON (additive; never removes the existing fields): backend_used ("livegraph" |
"sqlite"), fallback_reason (null when livegraph served; else the reason -- a NEW reason
LiveGraphIncompleteVsSqlite for the missing-cycle case), livegraph_compare summary (matched/missing/extra/
unknown counts) when the compare ran. The DEFAULT cycles answer's `cycles`/`count` stay the served backend's.
[RECOMMENDED]
```

### D6 — strict overrides unchanged (the user's #5)
```text
`--engine sqlite` (+ optional --kind module-import) -> SQLite MODULE, byte-unchanged. `--engine livegraph
--kind module-import` -> the explicit LiveGraph answer (current). `--engine compare --kind module-import` ->
the current compare. Only the DEFAULT (no --engine) gains the Auto behaviour. [RATIFIED by the brief]
```

## The honest tension (must be on the table before ratifying)
```text
Cycles is a WHOLE-GRAPH question; the LiveGraph cannot self-certify completeness (non-TS blindness + F2). So
the only correct near-term default is P2 (compare-verified), which:
  - RUNS SQLite on every default call -> does NOT advance nodes/edges decommission for cycles (the opposite
    of the migration thread's purpose), and
  - serves a BYTE-IDENTICAL answer when equivalent -> the user-visible change is ONLY `backend_used`.
So CYCLES-DEFAULT-MIGRATION via P2 is a LABELLING change, not a capability or a decommission step. The
genuinely valuable migration (P3: serve LiveGraph WITHOUT touching SQLite) is BLOCKED on enumeration +
completeness certification. This spec's most likely correct outcome is: ship P2 as a safe "served-by"
relabel IF the metadata has value, ELSE DEFER until P3 is buildable. << Decide this explicitly. >>
```

## Acceptance (EXECUTED later, IF a buildable model is ratified)
```text
1. xpart / amodx / hexmanos / zap-engine: default serves LiveGraph (P2: compare equivalent) OR a
   byte-compatible module-cycle output; backend_used=livegraph, fallback_reason=null.
2. repo-graph: default FALLS BACK to SQLite (the non-TS missing cycle makes the compare non-empty),
   fallback_reason=LiveGraphIncompleteVsSqlite; the human output is the full SQLite module cycles.
3. NO default call loses a SQLite cycle silently (the hard constraint) -- verified on all of the above.
4. `--engine sqlite|livegraph|compare` paths unchanged; full gate (workspace test, clippy, fmt).
```

## Out of scope (hard guardrails)
```text
NO raw decommission, NO deletion, NO SQLite removal, NO package resolver, NO enumeration (F2 stays a
prerequisite for P3). The default answer's COMPLETENESS is never reduced (fallback preserves it).
```

## Recommendation (for the ratification call)
```text
Given the honest tension: EITHER (i) ratify A+P2 as a labelling-only migration (the default serves LiveGraph
when proven-equivalent, else labelled SQLite -- safe, but no decommission/perf win), OR (ii) DEFER the
default migration and instead pursue the prerequisites (IMPORTS-XPART-ENUMERATION-1 to load whole repos + a
TS-only/file-coverage completeness certification) that would make P3 -- the migration that actually frees
SQLite -- buildable. I lean (ii) DEFER: P2 spends real complexity for a `backend_used` relabel; the READINESS
verdict is YELLOW precisely because the whole-graph completeness story is not yet certifiable without SQLite.
```

## Follow-up
```text
- IMPORTS-XPART-ENUMERATION-1 + a completeness-certification slice : the P3 prerequisites (the real migration).
- (only if A+P2 ratified) the implementation: extend the cycles default route with auto_outcome-for-cycles
  gated on the compare.
```

## References
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (`auto_outcome`; the Engine::Auto pattern; `module_cycle_compare_response`)
- `rust/crates/daemon-runtime/src/dispatch.rs` (`handle_cycles` routing; the default SQLite path)
- `docs/slices/module-cycles-default-readiness-1.md` (the YELLOW verdict + the repo-graph non-TS evidence)
- `docs/slices/query-migration-cli-1.md` (the callers/callees Auto precedent — and why cycles differs)
- `docs/slices/sqlite-raw-decommission-readiness-4.md` (Q3/Q4/Q5: which divergences are migration-acceptable)
