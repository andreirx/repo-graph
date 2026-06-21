# SQLITE-RAW-DECOMMISSION-READINESS-10: end-of-arc re-baseline — the SCIP unresolved-call boundary (Stage D)

Slice ID: SQLITE-RAW-DECOMMISSION-READINESS-10
Status: **AUDIT RE-BASELINE — evidence map. No code, no table deletion, no migration, no decommission, no
default flip.** A re-baseline of the `nodes`/`edges`/`unresolved_edges` retirement readiness after the Option-B
producer investigation closed (2026-06-13). Where READINESS-9 left the A-vs-B sequencing OPEN and recommended
Option B (eliminate the coherence eager reads) as the incremental path to the eventual global drop, the
investigation that followed PROVED Option B is bounded by a substrate boundary: the SCIP/LiveGraph current-state
substrate does NOT carry the unresolved-call disposition the trust contributor needs, so a FULL
`edges`/`unresolved_edges` decommission for that contributor is IMPOSSIBLE — not pending, impossible. This doc
re-baselines the gate against that finding. Baseline: READINESS-9. Track: Stage D. Gates any future
SQLITE-RAW-DECOMMISSION-1.

> NUMBERING NOTE (recorded, decide-and-record): the readiness sequence on disk is `-1..-7`, then `-9`, now
> `-10`; there is NO `-8`. [OBSERVED, first-hand THIS audit: `ls docs/slices/ | grep readiness` →
> `sqlite-raw-decommission-readiness-{1,2,3,4,5,6,7,9}.md` only.] READINESS-9 §"NUMBERING NOTE" recorded the
> `7 → 9` gap as intentional (operator-authored selection); this doc is `-10` per the operator-authored
> PRIORITY-DOCS-RECONCILE-4 packet, continuing the sequence by +1 from `-9`. No artifact is missing.

## Evidence basis (this audit)
```text
Docs-only re-baseline. LABELS (repo Evidence Law, agent_docs/validation.md):
  OBSERVED  = inspected first-hand THIS audit — a read I performed this turn (git log/show; the four committed
              docs; READINESS-9; ROADMAP/CURRENT_SLICE; VISION). Includes direct quotation of a committed
              artifact and direct arithmetic on observed counts.
  INFERRED  = my classification/judgment over those OBSERVED facts (the re-scope, the gate partition, the
              next-priority recommendation). "INFERRED over OBSERVED" where a conclusion sits on observed facts
              but is not raw arithmetic.
  EXECUTED  = a command I ran this turn with output observed.
  NOT RUN   = skipped, with reason.
This audit produced exactly its docs-only deliverables (this file + the ROADMAP/CURRENT_SLICE reconciliation)
and NOTHING else.

git: HEAD=`7d4b3bb` (SCIP-UNRESOLVED-CALL-PROBE-1 + close the trust-producer line) [OBSERVED, first-hand:
`git log --oneline -8`]. Working tree CLEAN before this slice's edits [OBSERVED: `git status` → "nothing to
commit, working tree clean"] — the four-commit arc is fully committed; nothing dirty was relabeled.

The four committed arc docs + READINESS-9 are read FIRST-HAND this turn; their commit metadata is captured via
`git show --stat`/the commit messages (OBSERVED). A docs-only re-baseline does NOT start the daemon and does
NOT run index/refresh or dev-install (state-mutating / out of scope; scripts/** is FILES_OUT_OF_SCOPE) — the
SAME stance READINESS-9 and the four arc specs took. Every claim below carries OBSERVED (commit/doc content) or
INFERRED (re-scope/recommendation).
```

## Verdict (headline)
```text
`nodes`/`edges`/`unresolved_edges` STILL NOT ready to retire — AND, NEW since READINESS-9, the goal is now
re-bounded: a FULL retirement is PROVEN partially IMPOSSIBLE, not merely pending.

THE LOAD-BEARING RE-BASELINE FINDING (VISION-level, INFERRED over OBSERVED — the four arc commits):
  The Option-B continuation READINESS-9 recommended depended on a SHARED LiveGraph trust-summary producer
  (orient DR-1 = explain DR-E1). That producer was specced (TRUST-SUMMARY-LIVEGRAPH-1, `94fc506`) as
  NEEDS-EXTENSION, and its load-bearing prerequisite was probed (SCIP-UNRESOLVED-CALL-PROBE-1, `7d4b3bb`) and
  returned NO-GO. The probe proved, with paired first-hand evidence, that scip-typescript emits NO occurrence
  for an unresolved call target (the unresolved call is ABSENT from the SCIP graph), so there is NO current-
  state SCIP/LiveGraph source from which to COUNT unresolved calls, and the recoverable signal (0) cannot reach
  the homegrown `unresolved_edges` count (3) on the same corpus — the notions are structurally INVERTED, not a
  reconcilable classification gap. Operator ratified Option A: keep the homegrown `unresolved_edges`
  (+ `extraction_diagnostics_json`) as the trust summary's unresolved-call input, served SQLite-LABELLED.

CONSEQUENCE: the unresolved-call TRUST FIELDS (unresolved_calls, call_resolution_rate, the reliability axes
derived from them, classifications, categories, the unknown-call blast radius, enrichment) have NO current-
state substrate replacement. The `edges`/`unresolved_edges` deletion gate for those fields is therefore RED BY
DESIGN — no amount of TS-side or non-TS work flips it, because the fact itself is not in the substrate. This is
a substrate BOUNDARY, not a backlog item.

PRECISION (do NOT overstate): this does NOT refute SCIP as the L0/L1 substrate. SCIP resolves MORE than the
homegrown extractor — that is the point of the pivot. The bounded claim is narrow: SCIP cannot SOURCE a
parity-with-`unresolved_edges` unresolved-call COUNT for the trust summary. resolved-call adjacency, cycles,
imports, module counts ARE LG-derivable; the unresolved-call DISPOSITION is the one fact SCIP drops.

The served state is UNCHANGED by this arc (it was a spec/probe arc, no impl): 6/10 defaults keep a SQLite-free
served path on green (callers/callees/path/imports/cycles/stats); orient/check/explain/trust still read SQLite
eagerly via the base use case (the coherence hybrid). The coherence hybrid is now the TERMINAL posture for the
trust contributor — not a way-station to an eager-read-free trust path. [INFERRED over the OBSERVED arc.]
```

## The Option-B investigation ARC + outcome (OBSERVED — the four commits + their verdicts)
```text
READINESS-9 (`56160bb`) left the A-vs-B sequencing OPEN and recommended Option B (eliminate the four coherence
commands' eager SQLite base reads) as the incremental, per-command, testable continuation — and a prerequisite
for the eventual global `nodes`/`edges` drop. The arc below tested that recommendation to destruction. Each
link REFINED the prior one; the chain terminates in Option A.

[1] ORIENT-SQLITE-FREE-1  `e10a455`  (spec; orient DEFERRED)  [OBSERVED: git show e10a455 + the doc]
    PROVED: orient is the supreme aggregator — its eager `nodes`/`edges` read spans FIVE paths, only TWO
    cert-covered. trust-core (`edges` + `unresolved_edges`; NO LiveGraph producer; UNCONDITIONAL in all four
    focuses) makes the composite ORIENT cert RED BY CONSTRUCTION, so an orient fastpath built now is dead,
    fallback-only code. Surfaced DR-1 (a shared trust-core producer) as the decisive blocker.
    DR-0 ratified by operator → S3 (reassign lead): explain leads Option B; orient deferred. S2 (orient-leads-
    anyway) rejected as dead code.

[2] EXPLAIN-SQLITE-FREE-1  `f3237f9`  (spec; PRODUCER-GATED)  [OBSERVED: git show f3237f9 + the doc]
    PROVED: explain is PRODUCER-GATED, not producer-light — REFUTING the S3 working hypothesis that explain,
    because it already SERVES 5 green leaf VALUES from the LiveGraph (`82b6557`), could be the first REAL
    eager-read elimination WITHOUT a producer program. First-hand reads showed explain carries the SAME
    unconditional trust-core dependency orient does (DR-E1 = orient DR-1), PLUS an unconditional focus-
    resolution gap (no repo-focus exemption) — strictly HARDER than orient. The five served leaves are a
    SUBSET of the answer; trust (unconditional) survives.
    Governance consequence: Option B is UNIFORMLY producer-gated on the one shared trust-core producer.
    DR-0 ratified → S1 (shared-prerequisite-first): build TRUST-SUMMARY-LIVEGRAPH-1 ONCE → unblocks orient +
    explain + corroborates trust. Option B re-scoped "producer-program first, per-command fastpaths second."

[3] TRUST-SUMMARY-LIVEGRAPH-1  `94fc506`  (spec; NEEDS-EXTENSION)  [OBSERVED: git show 94fc506 + the doc]
    PROVED: the shared producer is NOT a clean projection — REFINING the orient DR-1 / explain DR-E1
    assumption that "resolved/unresolved adjacency is already in the IR/xref." First-hand IR reads: `IrEdge`
    is RESOLVED-ONLY by construction (both endpoints CanonicalKeys); unresolved calls are DROPPED at SCIP
    ingest — there is NO `CallObservation` analogue of the existing `ImportObservation` (`rg CallObservation`
    over IR + scip-ingest → no matches). Of the 8 consumed `AgentTrustSummary` fields, exactly ONE
    (`resolved_calls`) is LG-derivable today; the other 7 need an IR/ingest/classifier extension whose
    feasibility hinges on an UNPROVEN SCIP capability (MISSING-1: does scip-typescript emit unresolved calls
    at all?). Crate-home decided (DR-TS-CRATE-HOME → C: a new `repo-graph-trust-livegraph` outer-composition
    crate) but actionable only if the probe is GO.
    DR-TS-0 ratified → S1 (extension-probe-first): run SCIP-UNRESOLVED-CALL-PROBE-1 before committing.

[4] SCIP-UNRESOLVED-CALL-PROBE-1  `7d4b3bb`  (investigative spike; NO-GO — HEAD)  [OBSERVED: git show 7d4b3bb + doc]
    PROVED (paired empirical evidence, the repo's own pinned scip-typescript@0.4.0 + decoder crate, scratch in
    /tmp — no committed artifacts):
      Q1 (MISSING-1) = NO. At an `any`/dynamic call target scip-typescript emits NO occurrence — no symbol,
        no `local`, no role, no marker. The unresolved call is ABSENT from the SCIP graph. (For a bare `any`
        call it emits a RESOLVED reference to the bound parameter — the OPPOSITE disposition.) There is no
        current-state SCIP source from which to COUNT unresolved calls.
      Q2 (MISSING-2) = NO. Paired on ONE corpus: SCIP-recoverable unresolved-call signal = 0; homegrown SQLite
        `unresolved_edges` (CALLS) = 3. `0 ≠ 3`. The divergence is STRUCTURAL (absence/inversion), not a
        classification gap — no classifier pass reconciles a present 3 against a structural 0.
    VERDICT NO-GO. The DR-TS-1 A extension ("a `CallObservation` populated FROM SCIP, yielding a no-loss
    current-state unresolved-call count") is NOT feasible.
    Operator ratified DR-TS-0-POST-PROBE → Option A (accept the honest hybrid): keep homegrown
    `unresolved_edges` (+ diagnostics) SQLite-LABELLED (the TRUST-LIVEGRAPH-1 Half-B shape); the deletion gate
    for those fields stays RED BY DESIGN. Closes the producer line: TRUST-SUMMARY-LIVEGRAPH-1 DR-TS-1 A
    refuted; orient DR-1 / explain DR-E1 (the shared producer) refuted; check/trust Option-B leads moot for
    the trust read. S4 (a redefined SCIP-native metric) NOT taken (no parity; needs a new contract + consumer
    threshold migration).

ARC SHAPE [INFERRED over OBSERVED]:  orient producer-gated  →  explain producer-gated (same source; producer-
light hypothesis refuted)  →  shared producer NEEDS-EXTENSION (clean-projection assumption refuted)  →  the
extension's load-bearing prerequisite NO-GO (SCIP carries no unresolved-call fact)  →  Option A (the honest
hybrid is terminal for the trust contributor). The recommendation READINESS-9 made (Option B as the path to
the global drop) is not WRONG so much as BOUNDED: it cannot reach the global drop because the substrate lacks
the fact. That bound is the load-bearing output of this re-baseline.
```

## THE LOAD-BEARING FINDING (VISION-level) — stated precisely
```text
CLAIM (INFERRED over OBSERVED — probe §3/§4/§6 + the Option-A ratification):
  The SCIP/LiveGraph current-state substrate does NOT carry unresolved-call DISPOSITION. Therefore the trust
  summary's unresolved-call fields cannot be served from current state, and a FULL `edges`/`unresolved_edges`
  decommission FOR THE TRUST CONTRIBUTOR is IMPOSSIBLE. The deletion gate for those fields is RED BY DESIGN
  (a substrate boundary), not RED-pending-work (a backlog item).

GROUNDING (OBSERVED, first-hand THIS audit, quoted from the committed arc):
  · `IrEdge` is resolved-only; unresolved calls are dropped at SCIP ingest — no `CallObservation` analogue.
    [trust-summary-livegraph-1.md §2b; commit 94fc506 body.]
  · scip-typescript emits NO occurrence for an unresolved call target — "the unresolved call is simply ABSENT
    from the SCIP graph." [scip-unresolved-call-probe-1.md §0/§3.2/§3.7, OBSERVED decoder output.]
  · Paired same-corpus count: SCIP-recoverable 0 ≠ homegrown `unresolved_edges` 3; structurally inverted.
    [probe §4.3/§4.5, OBSERVED.]
  · Operator-ratified Option A: "keep homegrown `unresolved_edges` SQLite-LABELLED … the deletion gate for
    those fields stays RED by design." [probe §0 Resolution + §7 DR-TS-0-POST-PROBE; commit 7d4b3bb body.]

WHAT THE CLAIM IS NOT (the precision the packet and the probe both demand):
  · NOT "SCIP is inadequate as the L0/L1 substrate." SCIP RESOLVES MORE than the homegrown extractor (at scale
    ≥35% of the homegrown "unresolved" set is exactly what SCIP resolves — probe §4.6). The pivot stands.
  · NOT "edges/unresolved_edges can never be retired for anything." resolved-call adjacency, cycles, imports,
    and module counts ARE LG-derivable (the 6 drilldowns already serve them on green; TRUST-SUMMARY-LIVEGRAPH-1
    §4 CLASS T1). Only the unresolved-call DISPOSITION is doomed.
  · NOT a claim about non-TS producers specifically. The probe is scip-typescript-scoped; non-TS producers
    (scip-clang, rust-analyzer) were not probed for unresolved-call emission. But non-TS coverage is ORTHOGONAL
    to this finding — it does not supply the missing TS unresolved-call fact, and the trust contributor's
    `unresolved_edges` read is the same shape across languages.

HONESTY CARRY-FORWARD (OBSERVED — probe §4.6 honesty note): the homegrown `unresolved_edges` count is largely a
syntax-only-extractor artifact (e.g. a 17% self-index resolution rate that SCIP would resolve far higher).
"No-loss PARITY with `unresolved_edges`" was therefore never a parity with ground truth — it was parity with an
artifact slated for retirement. This further weakens any future attempt to FORCE the trust unresolved-call
fields onto a current-state source by matching the old number; the honest path (Option A) labels the outgoing
artifact as such rather than reproducing its blindness.
```

## Re-scoped Stage-D — the gate/read partition (INFERRED over OBSERVED)
```text
READINESS-9 stated the goal as a global `nodes`/`edges` retirement gated RED by five FAILing deletion gates,
with the implicit promise that Option B + Option A together could eventually flip them. This re-baseline
partitions every raw-graph read by what CAN actually happen to it, exposing that one class can NEVER flip:

(a) ALREADY SQLite-FREE-SERVED (unchanged by this arc)
    The 6/10 drilldown defaults on green: callers/callees/path (lazy), imports/cycles/stats (GREEN-cert
    fastpath → LiveGraph). These read `nodes`/`edges` ONLY on the cert BUILD (once/fingerprint) and on fallback.
    STATUS: served-free today; this arc changed nothing here. [OBSERVED: READINESS-9 §Default-path table.]

(b) LG-DERIVABLE but UNBUILT (a marginal partial fastpath is possible; it flips NO deletion gate)
    The coherence commands' (orient/explain) NON-TRUST, RESOLVED structural contributors:
      · IMPORT_CYCLES (edges)            — module_import_cycles + cycles no-loss cert  [cert exists]
      · CALLERS/CALLEES (edges)          — callers/callees + per-symbol no-loss compare [cert exists]
      · resolved_calls (edges count)     — count IR Calls edges                         [LG-derivable now]
      · MODULE_SUMMARY structural counts — module_stats (DR-2/DR-E3; re-ratify SQLite-first)
      · imports (explain, edges)         — live_import_view + import cert               [served today]
    A future *-SQLITE-FREE-IMPL could serve these from the LiveGraph on green. BUT per the arc's closure
    pointers (orient/explain spec PRODUCER UPDATE blocks): such an impl "can only serve the LG-DERIVABLE
    contributors and must keep the trust contributor SQLite-labelled." Because (c) survives in the SAME
    command, the command still reads `unresolved_edges` every call → deletion gate 1 stays RED for
    orient/explain regardless. So (b) is a MARGINAL eager-read reduction that flips NO gate. [INFERRED.]

(c) RED-BY-DESIGN-FOREVER (no SCIP/LiveGraph source; the substrate boundary this arc proved)
    The trust contributor's unresolved-call fields, read from `unresolved_edges` + `extraction_diagnostics_json`:
      · unresolved_calls / unresolved_calls_external / unresolved_calls_internal_like
      · call_resolution_rate  → and the call_graph / change_impact reliability axes derived from it
      · classifications[] / categories[]  → and unknown_calls_blast_radius
      · enrichment_status / enrichment_state (the DEEPEST gap — no SCIP enrichment phase)
    These feed EXPLAIN_TRUST, the TRUST signals, the envelope confidence, and check's CheckInput. They have NO
    current-state substrate. RED by design. Served SQLite-LABELLED (Option A). This is the NEW class READINESS-9
    did not have — it had assumed all of gate 1 was eventually closable. [OBSERVED arc; INFERRED class.]

(d) RED-PENDING-OTHER-WORK (closable in principle; not by this arc, and gated on large programs)
      · NON-TS LiveGraph coverage — the structural ceiling (deletion gate 2). LiveGraph is TS-only; every
        non-TS file/repo falls back to the full SQLite read. Months of work (C GO, Rust GO-with-caveats).
      · The 6 drilldowns' FALLBACK paths (non-resident / non-TS / stale / RED) read `nodes`/`edges`.
      · The imports/cycles/stats CERT BUILDs read SQLite once/fingerprint.
      · The 31 non-graph tables (the broader decommission; out of scope for `nodes`/`edges` specifically).
    [OBSERVED: READINESS-9 §Q4(b)–(e), §Remaining blockers — carried forward unchanged.]

NET [INFERRED]: the union (a)∪(b) is the maximum a perfect TS-side Option-B program could make SQLite-free.
(c) is a permanent floor on the trust contributor. (d) is the rest of the global drop. A FULL `nodes`/`edges`/
`unresolved_edges` retirement is therefore unreachable for the trust contributor at ANY effort — the goal must
be re-stated as a BOUNDED/partial decommission, with the trust unresolved-call fields explicitly excluded and
kept SQLite-labelled (the honest hybrid).
```

## Deletion gates (READINESS-1 §5; re-annotated with the (a)–(d) partition)
```text
1 no default command depends on nodes/edges/unresolved_edges
    -> FAILS, and now PARTITIONED:
       (a) satisfied for the 6 drilldowns on green;
       (b) achievable-but-MARGINAL for the coherence resolved/non-trust leaves (flips no gate while (c) lives);
       (c) NEVER satisfiable for the trust contributor's unresolved-call fields — RED BY DESIGN (NEW);
       (d) pending for fallbacks + cert builds.
    NET: gate 1 can at best go from "all-RED" to "RED only for (c)+(d)"; it can NEVER go fully GREEN. [INFERRED]
2 LiveGraph covers SAME data for ALL languages
    -> FAILS. TS-only; non-TS always falls back. UNCHANGED. Class (d), the structural ceiling.
3 migration / back-compat story
    -> the 6 drilldowns: Auto + labelled fallback + byte-compatible + lazy/cert. The 4 coherence: the
       CoherenceEnvelope output contract + (NEW) the Option-A hybrid for trust (Half-A current-state posture +
       Half-B labelled outgoing-extractor diagnostics). Trust's unresolved-call half is now a RATIFIED permanent
       hybrid, not a transition state. [OBSERVED: TRUST-LIVEGRAPH-1 Half-B + Option A.]
4 operator reset story
    -> not reachable. UNCHANGED.
5 per-command parity tests on the new backend
    -> drilldowns: lazy/cert proofs. coherence: served/round-trip tests. For (c): there is NO parity test to
       build — the probe proved parity is unachievable (0 ≠ 3, structural). The honest "test" is the LABEL
       (provenance.source = sqlite) on the trust unresolved-call leaf, not a no-loss cert. [INFERRED over OBSERVED.]
```

## Post-probe served state (build on READINESS-9; the count is unchanged)
```text
This was a SPEC + PROBE arc — no implementation, no impl commit. So the served state is byte-for-byte what
READINESS-9 recorded:

  6/10 defaults have a SQLite-FREE served path on green: callers, callees, path (lazy); imports, cycles, stats
    (GREEN-cert fastpath → LiveGraph). [OBSERVED: READINESS-9 §Q1, UNCHANGED.]
  4/10 (orient, check, explain, trust) carry a CoherenceEnvelope served path but still read SQLite EAGERLY via
    the base use case every call (orient/explain/trust read `nodes`/`edges`; check reads operational +
    trust-core + declarations). [OBSERVED: READINESS-9 §Per-command table, UNCHANGED.]

What the arc CHANGED is not the served state but the OUTLOOK on it:
  · BEFORE (READINESS-9): the four coherence eager reads were "the next incremental target" (Option B), expected
    to convert to serve-then-fallback and eventually go SQLite-free-on-green.
  · AFTER (this re-baseline): the trust portion of those eager reads is now KNOWN to be permanent (class (c)).
    The coherence hybrid (CoherenceEnvelope + Option-A labelled trust) is the TERMINAL posture for the trust
    contributor, not a way-station. Only the (b) resolved/non-trust leaves remain convertible, and converting
    them flips no deletion gate. [INFERRED over OBSERVED.]
```

## Next priorities — recommendation matrix (INFERRED, OBSERVED-backed; NOT ratified)
```text
This readiness doc is ADVISORY (the operator chooses; the packet forbids inventing the next track here). The
A-vs-B sequencing READINESS-9 left OPEN is now RESOLVED FOR B: Option B is bounded by the SCIP boundary — it can
no longer be "the incremental path to the global drop," because the shared producer it required is refuted and
the trust unresolved-call fields are RED by design. B survives only as a marginal partial that flips no gate.
The remaining governance question is what to do INSTEAD. Candidates, every cell filled:

  OPTION                         WHAT IT BUYS                      WHAT IT DOES NOT DO                  GATE EFFECT
  ---------------------------------------------------------------------------------------------------------------
  P1 MARGINAL PARTIAL FASTPATHS  Converts orient/explain (b)       Does NOT touch the trust (c) read;  Flips NO gate
     (the residual Option B:      resolved/non-trust leaves to      command still reads unresolved_     (gate 1 stays
     orient/explain serve the     serve-then-fallback on green;     edges every call; not SQLite-free;  RED via (c)).
     LG-derivable leaves, keep    reduces SOME eager edge reads;    requires DR-2/DR-E3 re-ratify +     Marginal.
     trust SQLite-labelled)       per-command testable.             focus-resolution producer (DR-E2).
  ---------------------------------------------------------------------------------------------------------------
  P2 OPTION A — NON-TS LiveGraph  Attacks gate 2 (the structural    Does NOT fix the unresolved-call    Closes gate 2
     coverage (READINESS-9's      ceiling); the ONLY blocker no     gap (c) — orthogonal; even with     class (d);
     larger strategic unlock)     TS-side work removes; largest     full non-TS coverage the trust      gate 1 (c)
                                   strategic value; aligns with the  unresolved-call fields stay RED     UNCHANGED.
                                   multi-language product center.    by design. Multi-slice, months.
  ---------------------------------------------------------------------------------------------------------------
  P3 BOUNDED PARTIAL DECOMMISSION  Banks the achievable retirement   An architecture-boundary decision   Partially
     (re-scope SQLITE-RAW-         honestly: drop/retire only where   (a permanent two-source posture     closes gate 1
     DECOMMISSION-1 to drop only   (a)∪(b) cover, keep unresolved_    for the trust contributor); needs   for the
     the (a)/(b)-covered reads;    edges + diagnostics permanently    (b) + (d) closed for the covered    covered
     unresolved_edges + diagnostics for the trust contributor;       subset FIRST; high blast radius.    subset;
     RETAINED + labelled forever)  makes the terminal slice shippable                                     (c) excluded
                                   as a bounded goal, not a stuck one.                                    by design.
  ---------------------------------------------------------------------------------------------------------------
  P4 PIVOT OFF THE DECOMMISSION    Accepts the coherence hybrid as    Leaves nodes/edges/unresolved_      No gate
     (treat the trust hybrid as    terminal and spends effort on a    edges load-bearing (correct, given  movement on
     terminal; move to another     higher-value track — non-TS, the   (c)+(d)); SQLITE-RAW-DECOMMISSION-1 the
     Stage-D / Horizon track)      31-table decommission, warm-cache  parks as "bounded; gated."          decommission.
                                   end-state, or quality discovery.

RECOMMENDED (INFERRED, advisory): **P3 (re-scope the terminal slice to a BOUNDED partial decommission) as the
honest reconciliation of the goal, with P2 (non-TS coverage) as the higher STRATEGIC unlock to sequence
alongside or after it.** Rationale: P3 makes SQLITE-RAW-DECOMMISSION-1 a SHIPPABLE bounded slice instead of a
permanently-RED global one, and it records the (c) boundary in the contract rather than chasing an impossible
parity. P1 is real but marginal (flips no gate) — defensible only as opportunistic eager-read reduction, not as
a decommission step. P2 is the largest strategic value but does not change the (c) floor. P4 is the honest
"stop digging" option if the operator deprioritizes the raw-graph retirement entirely.

THIS IS A RECOMMENDATION, NOT A RATIFIED PICK. Choosing among P1–P4 (and the exact scope of any bounded
SQLITE-RAW-DECOMMISSION-1) is a governance call for the operator. Per the packet STOP_CONDITION, this doc does
NOT invent the next track; ROADMAP/CURRENT_SLICE are reconciled to record the CLOSED producer line + the
BOUNDED goal, and leave the next build as an OPEN governance call. [INFERRED over the OBSERVED gate partition.]
```

## Validation / evidence ledger (this audit)
```text
EXECUTED (command run, output observed first-hand THIS audit):
- git log --oneline -8  -> the four-commit arc at HEAD: `7d4b3bb` (probe; NO-GO) ← `94fc506`
  (TRUST-SUMMARY-LIVEGRAPH-1; NEEDS-EXTENSION) ← `f3237f9` (EXPLAIN-SQLITE-FREE-1; PRODUCER-GATED) ← `e10a455`
  (ORIENT-SQLITE-FREE-1; deferred) ← `56160bb` (PRIORITY-DOCS-RECONCILE-3 + readiness-9). Confirms the arc this
  doc re-baselines sits above readiness-9.
- git status  -> "nothing to commit, working tree clean" (before this slice's docs edits). Confirms the arc is
  fully committed; nothing dirty was relabeled.
- git show --stat e10a455 / f3237f9 / 94fc506 / 7d4b3bb  -> the FIRST THREE each add exactly ONE NEW slice doc
  (e10a455: orient-sqlite-free-1.md, +521; f3237f9: explain-sqlite-free-1.md, +684; 94fc506:
  trust-summary-livegraph-1.md, +800; 1 file changed each). The FOURTH (`7d4b3bb`, HEAD) touches FOUR files: it
  ADDS scip-unresolved-call-probe-1.md (+573) AND appends +7 lines EACH to the three prior arc docs
  (orient-sqlite-free-1.md, explain-sqlite-free-1.md, trust-summary-livegraph-1.md) — the producer-line CLOSURE
  pointers (commit body: "Closure pointers added to all three gated specs"; this doc inspects those PRODUCER
  UPDATE / PROBE OUTCOME blocks in the OBSERVED list below). `7d4b3bb` totals: 4 files changed, 594 insertions(+).
  The commit bodies carry the verdicts quoted in §"The Option-B investigation ARC".
- git diff --stat  -> lists the TWO tracked modified governance docs ONLY: `CURRENT_SLICE.md` and
  `docs/ROADMAP.md` (the reconciliation this slice makes). It does NOT list
  `docs/slices/sqlite-raw-decommission-readiness-10.md`, because that file is UNTRACKED and plain `git diff`
  compares tracked working-tree-vs-index only — it never lists untracked files. HONESTY NOTE (packet-expectation
  correction): the packet annotates this command "should show readiness-10 added + ROADMAP.md + CURRENT_SLICE.md
  modified"; that does NOT hold for an untracked NEW file under plain `git diff`. The OBSERVED evidence that
  readiness-10 was added is `git status --short` (`?? docs/slices/sqlite-raw-decommission-readiness-10.md`) plus
  `ls docs/slices/sqlite-raw-decommission-readiness-10.md` (the packet's first validation command — path
  returned). The index was NOT mutated (no `git add` / `git add -N`); the working tree is left in its natural
  uncommitted state for review.

OBSERVED (artifact / doc inspected first-hand THIS audit):
- docs/slices/orient-sqlite-free-1.md — orient producer-gap map; DR-1 trust-core blocker; DR-0 → S3; the
  PRODUCER UPDATE block (DR-1 REFUTED; trust leaf can never be edges-free on green).
- docs/slices/explain-sqlite-free-1.md — PRODUCER-GATED verdict (§11); DR-E1 = orient DR-1; the PRODUCER UPDATE
  block (DR-E1 REFUTED).
- docs/slices/trust-summary-livegraph-1.md — NEEDS-EXTENSION verdict (§9); IrEdge resolved-only + no
  CallObservation (§2b); the 8-field feasibility table (1 LG-derivable / 7 NEEDS-EXTENSION, §3a); the PROBE
  OUTCOME block (producer line CLOSED → Option A).
- docs/slices/scip-unresolved-call-probe-1.md — NO-GO headline (§0); Q1 decoder evidence (§3.2, the unresolved
  call ABSENT); Q2 paired counts 0 ≠ 3 (§4.3/§4.5); DR-TS-0-POST-PROBE → Option A (§7).
- docs/slices/sqlite-raw-decommission-readiness-9.md — the baseline this re-baselines (the 6/10 served-free; the
  five RED deletion gates; the A-vs-B open call).
- docs/VISION.md — the Fact-Certainty Model (Layer 1 trust must be honest about current-state fact vs an
  outgoing-extractor artifact) the Option-A decision rests on; the Operational Architecture (SQLite is the
  transition mechanism).
- docs/ROADMAP.md (Storage track) + CURRENT_SLICE.md — the governance docs reconciled by this slice.

NOT RUN (skipped, with reason):
- Build / test (cargo) and ./scripts/dev-install-local.sh: NOT RUN — docs-only slice; no source path touched;
  dev-install restarts the daemon (out of scope; scripts/** is FILES_OUT_OF_SCOPE).
- Live `rmap` capture / daemon start: NOT RUN — docs-only re-baseline; starting the daemon runs index/refresh
  (state-mutating). Grounded in first-hand reads of the committed arc instead (the SAME stance READINESS-9 and
  the four arc docs took). No structural claim here depends on a live capture.
- Re-running the probe: NOT RUN — the probe is committed (`7d4b3bb`) with its reproduction commands (§9); this
  re-baseline consumes its ratified verdict, it does not re-derive it.
```

## Guardrails honored
```text
No code. No table deletion. No migration. No decommission. No default flip. No new ratified priority invented
(P1–P4 are advisory; the next build is left an OPEN governance call). Audit-re-baseline doc only. The four arc
docs are read-only here (already committed) and were not re-edited. First-hand reads back every OBSERVED claim;
the RED-by-design boundary is stated precisely (a narrow "SCIP cannot SOURCE a parity unresolved-call count,"
NOT "SCIP is inadequate") so no false trust/certainty claim is minted. STOP-condition check (packet): the
commit chain + served state are CONSISTENT with the RED-by-design premise (probe Q1/Q2 + Option A) — no
contradiction; no stop on that ground.
```

## References
- `docs/slices/sqlite-raw-decommission-readiness-9.md` (baseline) + `-1..-7` (precedent structure)
- `docs/slices/orient-sqlite-free-1.md` (`e10a455`) — orient producer-gap map; DR-1; DR-0 → S3
- `docs/slices/explain-sqlite-free-1.md` (`f3237f9`) — PRODUCER-GATED; DR-E1 = orient DR-1; DR-0 → S1
- `docs/slices/trust-summary-livegraph-1.md` (`94fc506`) — NEEDS-EXTENSION; the IR/SCIP feasibility analysis; DR-TS-0 → S1
- `docs/slices/scip-unresolved-call-probe-1.md` (`7d4b3bb`, HEAD) — NO-GO; the paired empirical evidence; DR-TS-0-POST-PROBE → Option A
- `docs/slices/trust-livegraph-1.md` — the shipped hybrid (Half-A posture + Half-B labelled v1) that Option A makes terminal for the trust contributor
- `docs/VISION.md` § Fact Certainty Model, § Operational Architecture — the layer/honesty grounding for the RED-by-design call
- `docs/ROADMAP.md` (Storage Architecture Track) + `CURRENT_SLICE.md` — reconciled by this slice
