# SQLITE-RAW-DECOMMISSION-READINESS-9: Transition Audit (post-coherence recompute) (Stage D)

Slice ID: SQLITE-RAW-DECOMMISSION-READINESS-9
Status: **AUDIT RECOMPUTE — evidence map. No code, no table deletion, no migration, no decommission, no
default flip.** A recompute of the `nodes`/`edges` retirement readiness after the two things that landed
since the last readiness audit (READINESS-7, HEAD then `82da168`): (1) STATS-LIVEGRAPH-1 (`28ed216`) — the
6th SQLite-free drilldown default, which READINESS-7 §Q5 recommended; and (2) the ENTIRE COHERENCE LAYER —
the ratified `CoherenceEnvelope` contract + the orient/check/explain/trust specs + their four
implementations. Baseline: READINESS-7. Track: Stage D. Gates any future SQLITE-RAW-DECOMMISSION-1.

> NUMBERING NOTE (recorded, decide-and-record): there is NO `sqlite-raw-decommission-readiness-8.md` on disk
> — the readiness sequence is `-1..-7`, and this document is `-9` per the operator-authored selection packet
> (SLICE_ID = PRIORITY-DOCS-RECONCILE-3; SLICE_DOC = readiness-9). The packet's own validation line
> anticipates the gap ("cat readiness-8 (if present) or readiness-7"). The `-9` name is honored verbatim; the
> `7 -> 9` gap is intentional, not a missing artifact. [OBSERVED: `ls docs/slices/ | grep readiness` →
> `-1..-7` only, this turn.]

## Evidence basis (this audit)
```text
Docs-only recompute. LABELS (repo Evidence Law): OBSERVED = inspected first-hand THIS audit (a read I
performed this turn — git, the dispatch handlers, the four coherence serve modules, the contract/spec docs,
READINESS-7). INFERRED = my classification/judgment over those OBSERVED facts (the readiness verdict, the
retirement-distance recompute, the next-slice recommendation). This audit produced exactly its docs-only
deliverables (this file + the ROADMAP/CURRENT_SLICE reconciliation) and NOTHING else.

git: HEAD=`dc55114` (TRUST-LIVEGRAPH-IMPL — the last commit of the coherence chain) [OBSERVED, first-hand:
`git rev-parse HEAD`]. Working tree CLEAN before this slice's edits [OBSERVED, first-hand: `git status
--short` → empty] — the coherence layer is fully committed; nothing dirty was relabeled.

Daemon ABSENT: `rmap orient` → "daemon connection failed: socket does not exist" [EXECUTED, first-hand].
A docs-only recompute does NOT start the daemon or run index/refresh (state-mutating) and does NOT run
`dev-install-local.sh` (builds release binaries + restarts the daemon — out of scope for a docs reconcile;
the SAME stance READINESS-7 §"NOT RUN" took, and the SAME socket-absent result the coherence SPEC docs
recorded). The served `CoherenceEnvelope` outputs are therefore grounded in FIRST-HAND SOURCE reads of the
shipped handlers + serve modules — the stronger evidence basis for a claim about code structure — NOT in a
live capture. Every OBSERVED claim below carries its file:line so a reviewer can re-verify.
```

## Verdict (headline)
```text
`nodes`/`edges` STILL NOT ready to retire. NEW since READINESS-7: (a) stats became the 6th SQLite-free
drilldown default (STATS-LIVEGRAPH-1, `28ed216`); (b) the COHERENCE LAYER shipped — orient/check/explain/
trust now serve a `CoherenceEnvelope<T>` with honest per-signal provenance/trust/freshness + labelled
SQLite fallback, and explain/trust GENUINELY consult/serve the LiveGraph (explain swaps green structural
leaf VALUES from the LiveGraph; trust adds a current-state LiveGraph posture beside the retained v1).

CRITICAL, AND THE LOAD-BEARING RECOMPUTE FINDING [OBSERVED, first-hand: dispatch.rs handlers below]: the
coherence layer did NOT eliminate the EAGER SQLite read for these four. In every one of the four handlers
the base SQLite use case runs UNCONDITIONALLY before the envelope is assembled —
`repo_graph_agent::orient` (dispatch.rs:2603), `run_check` (:2689), `run_explain` (:2766),
`assemble_trust_report` (:2870), each taking `&repo_state.storage`. The `CoherenceEnvelope` builder then
OVERLAYS labels (orient/check), SWAPS green leaf VALUES from the LiveGraph (explain), or ADDS a LiveGraph
posture BESIDE the v1 report (trust) — ON TOP of the already-computed SQLite result. So the four are an
OUTPUT-CONTRACT + SERVING-MODEL advance and an HONESTY advance, NOT an eager-`nodes`/`edges`-read
elimination.

Consequence: the count of defaults with a SQLite-FREE served path is UNCHANGED at 6/10
(callers/callees/path/imports/cycles/stats). The four coherence defaults (orient/check/explain/trust) moved
from "SQLite-ONLY, no LiveGraph path, unlabelled" (READINESS-7) to "CoherenceEnvelope served + LiveGraph-
derived where green + labelled SQLite fallback/residual" — BUT they still read SQLite (incl. `nodes`/`edges`
for orient/explain/trust) EAGERLY on every call via the base use case. Retirement is otherwise UNCHANGED:
the 6 drilldowns' cert BUILDs + all fallback paths still read `nodes`/`edges`; LiveGraph is TS-only so every
non-TS repo falls back; the 31 non-graph tables remain. [INFERRED from the OBSERVED facts below.]
```

## Delta since READINESS-7 (what landed — and what it did NOT change)
```text
+ STATS-LIVEGRAPH-1 (spec `f6046ab` + impl `28ed216`) — the 6th SQLite-FREE drilldown default. stats now
  serves from the LiveGraph via a cert-gated fastpath built on the IR symbol-attributes substrate
  (`116fbb0`), SQLite fallback intact, output byte-preserving. This is exactly the slice READINESS-7 §Q5
  recommended ("QUERY-MIGRATION BREADTH -> STATS-LIVEGRAPH-1, SPEC-FIRST"). It moved stats out of the
  SQLite-eager set: READINESS-7 had 5/10 served-free; stats made it 6/10. [OBSERVED: ROADMAP §Current
  Priority + CURRENT_SLICE banner; docs/slices/stats-livegraph-1.md.]

+ COHERENCE-LAYER-1 (contract `6ed17b8`, amended `5129f44`) — the ratified mixed-source contract for
  orient/check/explain/trust: the NEW generic `CoherenceEnvelope<T> { value, provenance, trust, freshness }`
  wrapper (COHERENCE-ENVELOPE-SHAPE), the hybrid trust disposition (TRUST-DISPOSITION), the MEET fold (D3),
  authority-overlays-never-erases (D5), the per-command source map, and (amendment D8) multi-source LEAF
  provenance (`Provenance.source: BTreeSet<Source>`). DESIGN/SPEC — produced no served code itself.
  [OBSERVED: docs/slices/coherence-layer-1.md header + §"Forced decisions".]

+ The FOUR per-command SPECS (ratified): ORIENT-LIVEGRAPH-1 (`af49ea6`), CHECK-LIVEGRAPH-1 (`ef30083`),
  EXPLAIN-LIVEGRAPH-1 (`cb8a311`), TRUST-LIVEGRAPH-1 (`9c18754`). Each pins the field-level LG-first /
  SQLite-first / Authority / FS posture for its command against the ratified contract. [OBSERVED:
  docs/slices/{orient,check,explain,trust}-livegraph-1.md.]

+ The FOUR IMPLEMENTATIONS:
  - ORIENT-LIVEGRAPH-IMPL + the `repo-graph-coherence` SUPPORT crate (`2fd4478`). [OBSERVED, first-hand:
    `rust/crates/repo-graph-coherence/` exists; daemon-runtime/Cargo.toml:63 depends on it; dispatch.rs
    handle_orient:2631 calls `orient_coherence::build_orient_envelope`.]
  - CHECK-LIVEGRAPH-IMPL (`3e76271`). [OBSERVED, first-hand: dispatch.rs handle_check:2714 calls
    `check_coherence::build_check_envelope`.]
  - EXPLAIN-LIVEGRAPH-IMPL (`82b6557`). [OBSERVED, first-hand: dispatch.rs handle_explain:2795 calls
    `explain_coherence::build_explain_envelope`.]
  - TRUST-LIVEGRAPH-IMPL (`dc55114`, HEAD). [OBSERVED, first-hand: dispatch.rs handle_trust:2911 calls
    `trust_coherence::build_trust_envelope`.]

~ unchanged: callers/callees/path LAZY; imports/cycles/stats GREEN-cert fastpath (SQLite once/fingerprint
  for the cert build + on fallback). The explicit `--engine sqlite|livegraph|compare` routes are unchanged.

- NOT changed by the coherence layer: the EAGER SQLite read of orient/check/explain/trust (the base use
  case runs every call — see the per-command table); LiveGraph TS-only (non-TS always falls back); the 6
  drilldowns' cert BUILDs + fallback paths read `nodes`/`edges`; the 31 non-graph tables. [OBSERVED,
  first-hand: dispatch.rs:2603/2689/2766/2870 — base use case unconditional in every handler.]
```

## Per-command coherence characterization (OBSERVED, first-hand — the four handlers + serve modules)
```text
The four are NOT symmetric. "Has a CoherenceEnvelope served path" is TRUE for all four, but the LiveGraph
ROLE ranges from NONE (check) to genuine VALUE serving (explain). In ALL four the base SQLite use case runs
UNCONDITIONALLY first — the eager read is not skipped.

COMMAND  HANDLER (dispatch.rs)              BASE SQLite USE CASE (eager,    COHERENCE ADAPTER /         LiveGraph ROLE
                                            every call)                    OUTPUT
orient   handle_orient:2550                 repo_graph_agent::orient       orient_coherence::          LABELS 4 LG-first leaves
         -> envelope :2631                  (&storage) :2603 — reads       build_orient_envelope       (IMPORT_CYCLES/HIGH_COMPLEXITY/
                                            nodes/edges (find_module_      -> CoherenceEnvelope<        CALLERS_SUMMARY/CALLEES_SUMMARY)
                                            cycles, module_summary)        CoherentOrientResult>       via no-loss-gated DECISIONS;
                                                                           + get_stale_files (SQLite)  leaf VALUE stays SQLite-built.
check    handle_check:2660                  run_check(&storage) :2689 —    check_coherence::           NONE. No LiveGraph read, no cert.
         -> envelope :2714                  reads SQLite (operational +    build_check_envelope        Thin stale-read + MEET freshness
                                            trust-core + declarations)     -> CoherenceEnvelope<        + multi-source verdict provenance
                                                                           CoherentOrientResult>       {sqlite, declaration} (D8).
explain  handle_explain:2730                run_explain(&storage) :2766 —  explain_coherence::         GENUINELY SERVES 5 green leaf
         -> envelope :2795                  reads nodes/edges (identity/   build_explain_envelope      VALUES from the LiveGraph
                                            callers/callees/imports/       -> CoherenceEnvelope<        (IDENTITY/IMPORTS/CYCLES/CALLERS/
                                            cycles)                        CoherentOrientResult>       CALLEES), swaps into the result;
                                                                                                       labelled SQLite fallback per leaf.
trust    handle_trust:2814                  assemble_trust_report          trust_coherence::           Half-A posture leaf SERVED from
         -> envelope :2911                  (&storage) :2870 — reads       build_trust_envelope        the LiveGraph (live_partitions()
                                            snapshots/unresolved_edges/    -> CoherenceEnvelope<        + module_stats()) BESIDE the
                                            edges/declarations/            CoherentTrustReport>        RETAINED v1 (Half B, source=sqlite,
                                            module_candidates              + get_stale_files (SQLite)   byte-identical). Hybrid.

[OBSERVED, first-hand: dispatch.rs:2550-2919; orient_coherence.rs:40-118; check_coherence.rs:25-50 ("NO
LiveGraph read, NO cert ... THIN stale-read + delegate"); explain_coherence.rs:1-64 ("REAL LiveGraph
serving, NOT a re-labelled SQLite result ... swaps the live-served values into the bare result");
trust_coherence.rs:1-108 ("Half A is a PROJECTION of EXISTING LiveGraph runtime state ... NO new producer").]

INFERRED: explain is the proof that the LiveGraph can SERVE a composite command's structural sections; trust
is the proof of the hybrid current-state posture. Both are genuine progress toward an eventual SQLite-free
coherence path. But neither removes the base SQLite read TODAY — that is a LATER slice (see §Q4/§Q5).
```

## Default-path SQLite-read audit (the READINESS-7 table, re-measured at HEAD `dc55114`)
```text
COMMAND   DEFAULT   SERVED-PATH READS nodes/edges?     WHEN SQLite IS READ
callers   Auto      NO (lazy)                          only on FALLBACK (non-resident / non-TS / stale / !Exact)
callees   Auto      NO (lazy)                          only on FALLBACK
path      Auto      NO (lazy)                          only on FALLBACK
imports   Auto      NO on the GREEN-cert fastpath      cert BUILD (once/fingerprint) + FALLBACK (non-TS/RED/stale)
cycles    Auto      NO on the GREEN-cert fastpath      cert BUILD (once/fingerprint) + FALLBACK (RED/non-TS/non-resident)
stats     Auto      NO on the GREEN-cert fastpath      cert BUILD (once/fingerprint) + FALLBACK; NEW since R7 (the 6th)
            (was sqlite-eager in R7)
orient    coherence YES — EAGER base read every call   base repo_graph_agent::orient (nodes/edges via cycles +
                                                       module_summary) + get_stale_files; LG LABELS 4 leaves on green
check     coherence YES — EAGER base read every call   base run_check (operational + trust-core + declarations);
                                                       NO LiveGraph read
explain   coherence YES — EAGER base read every call   base run_explain (nodes/edges, heavy); LG SERVES 5 leaf
                                                       VALUES on green (base read already happened)
trust     coherence YES — EAGER base read every call   base assemble_trust_report (edges/unresolved_edges/nodes via
                                                       module stats); LG posture ADDED beside v1

=> 6/10 defaults have a SQLite-FREE served path (callers/callees/path/imports/cycles/stats) — UNCHANGED count
   vs the coherence layer; stats is the only addition since R7 (was 5/10 -> 6/10). 4/10
   (orient/check/explain/trust) now carry a CoherenceEnvelope served path WITH genuine LiveGraph serving/
   labeling on green, but STILL read SQLite (incl. nodes/edges for orient/explain/trust) EAGERLY via the
   base use case. The coherence layer did NOT change the served-FREE count; it upgraded the OUTPUT HONESTY +
   LiveGraph serving of the remaining 4. [OBSERVED for the 10 handlers + serve modules, first-hand.]
```

## Audit questions (the operator's recompute — answered)
```text
Q1 — Which defaults now COMMONLY serve WITHOUT SQLite?
  SIX, unchanged in count by the coherence layer: callers, callees, path (lazy), imports, cycles, stats
  (GREEN-cert fastpath -> LiveGraph). stats is the only NEW entry since READINESS-7. The four coherence
  defaults are NOT in this set: they have a LiveGraph-serving/labeling path but still read SQLite eagerly.
  [OBSERVED, first-hand: dispatch.rs drilldown handlers (<=1701) vs the four coherence handlers (2550-2919).]

Q2 — Which still read SQLite EAGERLY (every call, as part of the served answer)?
  FOUR: orient, check, explain, trust — the coherence cluster. Each runs its base SQLite use case
  unconditionally (dispatch.rs:2603/2689/2766/2870) before assembling the CoherenceEnvelope. orient/explain/
  trust read `nodes`/`edges` in that base pass; check reads SQLite operational + trust-core + declarations
  (its raw nodes/edges dependence is indirect, via the edge-derived trust-core reliability). READINESS-7 had
  FIVE here (stats + these four); stats LEFT this set via STATS-LIVEGRAPH-1, the four REMAIN. [OBSERVED,
  first-hand.] What CHANGED for the four is the OUTPUT (now a labelled CoherenceEnvelope) and the LiveGraph
  involvement (explain serves values; trust serves a posture; orient labels) — NOT the eager read.

Q3 — Which use SQLite ONLY to build a CERT / safety predicate (not per served call)?
  TWO drilldowns, unchanged: imports (build_and_store_import_cert) and cycles (build_and_store_cycles_cert),
  each once per fingerprint (stats reuses the same cert machinery). NEW NUANCE from the coherence layer:
  orient's 4 LG-first leaf LABELS and explain's 5 green leaf SERVES are gated by the SAME no-loss certs (the
  cycles/complexity certs + the per-symbol callers/callees no-loss key-set compare) [OBSERVED, first-hand:
  orient_coherence.rs:79-114 maps cycles/complexity/callers/callees outcomes; explain_coherence.rs:48 reuses
  build_and_store_import_cert]. But because the BASE use case already read SQLite eagerly, the cert here gates
  the LABEL/VALUE-SOURCE, it does NOT (yet) let the command SKIP the SQLite read the way the drilldowns do.

Q4 — What remains before `nodes`/`edges` can be retired?
  (a) The FOUR coherence defaults' EAGER base reads. The coherence layer gave them a LiveGraph-serving/
      labeling path + honest output, but the base use case still reads SQLite (nodes/edges for orient/
      explain/trust) every call. Making them SQLite-FREE-on-green (the drilldown posture) is a FURTHER slice:
      the base use cases are COMPOSITE multi-source aggregators, not single structural queries, so this is a
      real build per command, not a cert-flip. explain (already serving 5 leaf values) and trust (already
      serving the posture) are the closest; orient labels-only; check has no LiveGraph leaf at all.
  (b) NON-TS coverage: LiveGraph is TS-only -> every non-TS file/repo FALLS BACK to SQLite. `nodes`/`edges`
      stay load-bearing for C/C++/Rust/Java regardless of any TS-side migration. This is the STRUCTURAL
      ceiling (deletion gate 2) — even a perfect TS migration cannot retire `nodes`/`edges` globally while
      non-TS repos exist. [OBSERVED-in-artifact: READINESS-7 §Q4(c); CURRENT_SLICE RUST-INGEST-PROVE-1
      GO-with-caveats, C/Rust SCIP ingest partial.]
  (c) The 6 drilldowns' FALLBACK paths (non-resident / non-TS / stale / RED) read `nodes`/`edges`.
  (d) The imports + cycles + stats CERT BUILDs read SQLite once/fingerprint; a fully SQLite-free default
      needs the cert source itself to become SQLite-free (a STRUCTURAL no-loss proof, not a SQLite compare).
  (e) The 31 non-graph tables (the broader decommission; out of scope for `nodes`/`edges` specifically).
  `nodes`/`edges` become droppable ONLY when EVERY default served path is SQLite-free AND the fallback is
  removed/covered (non-TS + residency) AND the cert builds are SQLite-free — (a)+(b)+(c)+(d). [INFERRED]

Q5 — Highest-value next slice? GOAL-DEPENDENT — VALIDATED against repo state. See §Recommendation.
```

## Deletion gates (READINESS-1 §5; current status — ALL still FAIL)
```text
1 no default command depends on nodes/edges      -> FAILS. 6/10 have a SQLite-free served path (on green);
                                                    4/10 (coherence) still read eagerly; all fallbacks + 3
                                                    cert builds read nodes/edges. NOT advanced by coherence
                                                    in the eager-read sense (the four still read eagerly);
                                                    advanced only by stats joining the served-free set.
2 LiveGraph covers SAME data for ALL languages   -> FAILS. TS-only; non-TS always falls back. UNCHANGED.
3 migration / back-compat story                  -> the 6 drilldowns: Auto + labelled fallback + byte-
                                                    compatible human + lazy/cert. The 4 coherence: a NEW
                                                    CoherenceEnvelope output contract (NOT byte-preserving —
                                                    by design; the wrapper adds honest provenance/freshness
                                                    labels per the ratified contract). NEW since R7.
4 operator reset story                           -> not reachable. UNCHANGED.
5 per-command parity tests on the new backend    -> drilldowns: lazy/cert proofs (callers/callees/path
                                                    panicking-closure; imports/cycles/stats no-loss + cert).
                                                    coherence: NEW served/round-trip tests
                                                    (explain_coherence_served_tests.rs,
                                                    explain_coherence_tests.rs, and #[cfg(test)] mods in
                                                    check_coherence.rs / trust_coherence.rs) [OBSERVED,
                                                    first-hand: those files exist]. ADVANCED for the four.
```

## Remaining blockers (recompute, confirmed)
```text
- orient / check / explain / trust EAGER base reads      -> still read SQLite every call (coherence overlays
                                                            on top; does not skip the read). NEW framing.
- non-TS languages                                       -> LiveGraph TS-only (always fallback). The ceiling.
- the 6 drilldowns' FALLBACK paths                       -> still read nodes/edges.
- the imports + cycles + stats cert BUILDs               -> SQLite once/fingerprint (not free).
- the 31 non-graph tables                                -> the broader decommission.
```

## Recommendation — highest-value next slice/track (INFERRED, OBSERVED-backed)
```text
TRACK-LEVEL (unambiguous from the ratified Stage-D order): the genuine next priority is the SQLite-raw
`nodes`/`edges` retirement -> **SQLITE-RAW-DECOMMISSION-1** (the terminal Stage-D slice). In the current
post-coherence Stage-D order THIS audit is the gate immediately preceding it:
`COHERENCE-LAYER-1 ✓ -> SQLITE-RAW-DECOMMISSION-READINESS-9 ✓ (this doc; gate RED) -> SQLITE-RAW-DECOMMISSION-1
(next; terminal; GATED)`. [OBSERVED, first-hand THIS audit: CURRENT_SLICE.md "Stage D order (updated
2026-06-12)" line marks "COHERENCE-LAYER-1 ✓", then "SQLITE-RAW-DECOMMISSION-READINESS-9 ✓ (post-coherence
recompute; gate RED)", then "SQLITE-RAW-DECOMMISSION-1 (next — terminal; GATED..."; ROADMAP.md Storage-track
SQLITE-RAW-DECOMMISSION-1 row = "NEXT (Stage D, terminal; GATED — readiness-9 gate RED...)". An earlier draft
of this line tagged COHERENCE-LAYER-1 itself as the next slice; that was pre-coherence and is now FALSE —
coherence is ✓ and this readiness doc sits between coherence and the decommission. Corrected here, not
silently.]

The gate is RED (all five deletion gates FAIL), so SQLITE-RAW-DECOMMISSION-1 cannot proceed AS A GLOBAL
`nodes`/`edges` drop yet. The highest-value next BUILD is a GOVERNANCE FORK — surfaced here as a
recommendation with trade-offs (this readiness doc is advisory; the operator chooses):

  OPTION A — NON-TS LiveGraph coverage first (attack deletion gate 2, the structural ceiling).
    PRO: it is the ONLY blocker that no amount of TS-side work can remove; until it lands, `nodes`/`edges`
      cannot be globally retired for C/C++/Rust/Java. Largest strategic unlock.
    CON: largest scope; depends on non-TS SCIP ingest maturing (C GO, Rust GO-with-caveats per CURRENT_SLICE)
      — months of work, not a single slice.
  OPTION B — Eliminate the coherence eager base reads (make orient/check/explain/trust SQLite-free-on-green,
      the drilldown posture; attack deletion gate 1 for the 4).
    PRO: explain already serves 5 leaf values and trust already serves the posture from the LiveGraph, so the
      groundwork exists; converts the four from "overlay on an eager SQLite read" to "serve-then-fallback".
      Per-command, incremental, testable — matches the proven drilldown pattern.
    CON: the base use cases are COMPOSITE multi-source aggregators (Authority declarations + Tier-B derived
      cache + FS + structural), so a real per-command build, and it does NOT remove the non-TS / fallback /
      cert-build reads — `nodes`/`edges` still cannot drop after it. Necessary but not sufficient.
  OPTION C — SCOPED decommission: a TS-ONLY-repo `nodes`/`edges` retirement (drop only where the LiveGraph
      fully covers), leaving non-TS on SQLite.
    PRO: banks the TS-side win without waiting for non-TS coverage.
    CON: an architecture-boundary decision (two storage postures by language); high blast radius; needs the
      eager reads (Option B) AND the fallback/cert reads closed for the TS subset FIRST. Premature now.

RECOMMENDED: **Option B (eliminate the coherence eager reads), as the next concrete slice(s)**, BECAUSE it is
the incremental, evidence-supported continuation of the migration the coherence layer set up (explain/trust
already serve from the LiveGraph), it is per-command testable, and it is a prerequisite for BOTH Option A's
eventual global drop and Option C's scoped drop. Option A (non-TS coverage) is the higher STRATEGIC unlock
but is a multi-slice track, not the immediate next step. Option C is premature (gates B-prereqs unmet).
This is a recommendation, not a ratified pick — the A-vs-B sequencing is a governance call for the operator.
[INFERRED from the OBSERVED gate status + the per-command coherence characterization.]
```

## Validation / evidence ledger (this audit)
```text
EXECUTED (command run, output observed first-hand THIS audit):
- git rev-parse HEAD  -> `dc55114...` (TRUST-LIVEGRAPH-IMPL). Confirms the coherence chain is at HEAD.
- git status --short  -> empty. Confirms a CLEAN tree before this slice's docs edits (the docs-only
                        guardrail starting point; the coherence layer is fully committed).
- git log --oneline -15 -> the coherence chain `6ed17b8..dc55114` (contract + amendment + 4 specs + 4 impls)
                        sits above the last reconcile `7e7c416`, which itself sits above STATS-LIVEGRAPH-IMPL
                        `28ed216`. Confirms the delta this audit recomputes.
- rmap orient --budget small -> "daemon connection failed: socket does not exist". Recorded as the
                        transport-absent path; grounding shifted to first-hand source reads (below).

OBSERVED (artifact / source inspected first-hand THIS audit):
- rust/crates/daemon-runtime/src/dispatch.rs:2550-2919 — the four coherence handlers; each runs its base
  SQLite use case UNCONDITIONALLY (orient:2603, check:2689, explain:2766, trust:2870) then calls its
  CoherenceEnvelope builder (orient:2631, check:2714, explain:2795, trust:2911). The load-bearing eager-read
  evidence.
- rust/crates/daemon-runtime/src/orient_coherence.rs:40-118 — orient LABELS its 4 LG-first leaves via
  no-loss-gated decisions over the already-SQLite-built result; reads get_stale_files.
- rust/crates/daemon-runtime/src/check_coherence.rs:25-50 — "NO LiveGraph read, NO cert ... THIN stale-read
  + delegate"; multi-source verdict provenance.
- rust/crates/daemon-runtime/src/explain_coherence.rs:1-120 — "REAL LiveGraph serving, NOT a re-labelled
  SQLite result"; SWAPS green leaf values into the bare result; labelled SQLite fallback per leaf.
- rust/crates/daemon-runtime/src/trust_coherence.rs:1-108 — Half-A posture leaf PROJECTED from
  live_partitions() + module_stats() BESIDE the retained byte-identical v1 (Half B, source=sqlite).
- rust/crates/repo-graph-coherence/ exists; daemon-runtime/Cargo.toml:63 depends on it; the per-command
  serve modules + coherence test files (explain_coherence_served_tests.rs etc.) exist.
- docs/slices/coherence-layer-1.md (ratified contract + D8 amendment); orient/check/explain/trust-livegraph-1.md
  (the four ratified specs); stats-livegraph-1.md (the 6th drilldown); sqlite-raw-decommission-readiness-7.md
  (the baseline this recomputes).

NOT RUN (skipped, with reason):
- Build / test (cargo) and ./scripts/dev-install-local.sh: NOT RUN — docs-only slice; no source path touched,
  so no build is owed; dev-install restarts the daemon (out of scope; scripts/** is FILES_OUT_OF_SCOPE).
- Live `rmap orient/check/explain/trust` capture of the served CoherenceEnvelope JSON: NOT RUN — daemon
  socket absent; starting it runs index/refresh (state-mutating). Grounded in first-hand source reads of the
  shipped handlers + serve modules instead (the SAME stance the coherence SPEC docs took under the same
  socket-absent condition). The structural claims do not depend on a live capture.
```

## Guardrails honored
```text
No code. No table deletion. No migration. No decommission. No default flip. Audit-recompute doc only. The
STATS-LIVEGRAPH-1 + coherence-layer changes are RECORDED, not re-done; `nodes`/`edges` untouched. The
coherence slice docs are read-only here (already committed). First-hand source reads back every OBSERVED
claim; the eager-base-read finding is stated precisely so no false "nodes/edges now retirable" claim is
minted.
```

## References
- `docs/slices/sqlite-raw-decommission-readiness-7.md` (baseline) + `-6.md` … `-1.md` (precedent structure)
- `docs/slices/coherence-layer-1.md` (ratified `CoherenceEnvelope` contract + D8 amendment — the layer this recomputes)
- `docs/slices/orient-livegraph-1.md` / `check-livegraph-1.md` / `explain-livegraph-1.md` / `trust-livegraph-1.md` (the four ratified per-command specs)
- `docs/slices/stats-livegraph-1.md` (STATS-LIVEGRAPH-1 — the 6th SQLite-free drilldown default, landed since READINESS-7)
- `rust/crates/repo-graph-coherence/` (the support crate realizing `CoherenceEnvelope<T>`, `2fd4478`)
- `rust/crates/daemon-runtime/src/dispatch.rs` (`handle_orient`:2550 / `handle_check`:2660 / `handle_explain`:2730 / `handle_trust`:2814 — base use case unconditional, then the envelope builder)
- `rust/crates/daemon-runtime/src/{orient,check,explain,trust}_coherence.rs` (the four IMPURE serve adapters)
