# SQLITE-RAW-DECOMMISSION-1: bounded partial decommission CONTRACT (Stage D, terminal)

Slice ID: SQLITE-RAW-DECOMMISSION-1
Status: **RATIFIED (operator, 2026-06-14; DR-SRD1 → Option A) — CONTRACT / SPEC ONLY (decision-first). No
code, no table drop, no migration, no default flip.** This is the bounded-scope CONTRACT that re-frames the
terminal Stage-D slice. It defines WHAT is retired, WHAT is retained-forever, which deletion gates close vs
are impossible-by-substrate, and the prerequisites the (later) retirement IMPL is gated on. It does NOT
perform the retirement.
Track: Stage D — SQLite-raw decommission. The TERMINAL slice.
Baseline / grounding: `docs/slices/sqlite-raw-decommission-readiness-10.md` (the end-of-arc re-baseline;
supersedes readiness-9) + `-readiness-1.md` §5 (the original gates) + the four arc specs
(orient/explain/producer/probe) + `docs/VISION.md` (Fact-Certainty Model).
HEAD at authoring: `78feb81` (PRIORITY-DOCS-RECONCILE-4 + readiness-10), above the `7d4b3bb` arc.

> **DECISION RESOLUTION — DR-SRD1-BOUNDED-SCOPE (ratified by operator, 2026-06-14): Option A (RATIFY-AND-PARK).**
> This contract is ADOPTED as the governing terminal Stage-D storage posture. **Contract Clause 3 (§6) is now
> binding:** `unresolved_edges` + `extraction_diagnostics_json` are RETAINED + SQLite-LABELLED **FOREVER** for
> the trust contributor; the trust contributor is **permanently two-source**. `nodes`/`edges` are bounded-partial
> (class (d), CLOSABLE-PENDING) — NOT permanent; their retirement IMPL is **PARKED** until PREREQ-1 (the (b)
> leaves served) + PREREQ-2 (the covered subset's (d) fallback/cert handling) land (§9). Gate 1 is recorded
> **permanently-partial** — (c) IMPOSSIBLE-BY-SUBSTRATE, (b)/(d) CLOSABLE-PENDING — and a future audit reporting
> "gate 1 closed" is wrong by construction. The next BUILD (P1 marginal fastpaths and/or P2 non-TS coverage) is
> a SEPARATE, still-OPEN governance call. B (partial-close-now) was REJECTED (VISION-forbidden overclaim); C/D
> not taken. ROADMAP.md + CURRENT_SLICE.md reconciled to this ratification.

---

## 0. What this contract is (and is not)

```text
WHAT IT IS [INFERRED over the OBSERVED readiness-10 re-baseline]:
  SQLITE-RAW-DECOMMISSION-1 was framed (readiness-1 §6) as a GLOBAL retirement of the raw
  `nodes`/`edges`/`unresolved_edges` substrate. The Option-B producer investigation (the four-commit arc
  ORIENT-SQLITE-FREE-1 → EXPLAIN-SQLITE-FREE-1 → TRUST-SUMMARY-LIVEGRAPH-1 → SCIP-UNRESOLVED-CALL-PROBE-1)
  closed NO-GO and PROVED that a FULL retirement is partially IMPOSSIBLE: the trust contributor's
  unresolved-call fields have no current-state SCIP source. That global framing is dead. This document
  RE-SCOPES the slice into a precise BOUNDED partial decommission CONTRACT, ratified BEFORE any deletion.

WHAT IT IS NOT:
  Not an implementation. Not a migration. Not a table drop. Not a default flip. It starts no build. The
  retirement IMPL, the marginal P1 fastpaths (the residual Option B), and the P2 non-TS coverage program
  are SEPARATE, LATER tracks (§Scope boundary). This contract is the decision-first artifact the external
  review asked for: "decision-first … then implement only the bounded retirement."

EVIDENCE LABELS (repo Evidence Law, `agent_docs/validation.md`):
  OBSERVED = a doc/source I read first-hand THIS turn (the readiness docs, the four arc specs, VISION,
             ROADMAP/CURRENT_SLICE). Includes direct quotation of a committed artifact.
  INFERRED = my classification/synthesis over those OBSERVED facts (the contract clauses, the gate
             dispositions, the scope partition).
  EXECUTED = a command I ran this turn with output observed (the validation ledger).
  NOT RUN  = skipped, with reason.
  A contract/spec slice does NOT start the daemon and does NOT run index/refresh/dev-install
  (state-mutating; scripts/** out of scope) — the SAME stance readiness-9/-10 and the four arc specs took.
```

---

## 1. VISION tie — Fact-Certainty Model (the load-bearing justification)

```text
The contract rests on ONE VISION principle [OBSERVED: docs/VISION.md § "Fact Certainty Model";
§ "Product Layer Model" Layer 1; agent_docs/architecture.md rule 6 "explicit degradation"]:

  A fact with NO current-state source is LABELLED, not synthesized.
  "If an answer cannot show versioned provenance, evidence chain, and policy context, it is advisory
   only, not authoritative." — VISION § Product Principle.
  Layer 1 (trust) is extracted fact; it MUST be honest about current-state fact vs an outgoing-extractor
  artifact. `null` = unknown, never empty (architecture.md rule 6).

APPLICATION [INFERRED over the probe NO-GO]:
  The trust contributor's unresolved-call fields are sourced from the homegrown `unresolved_edges` +
  `extraction_diagnostics_json` — an OUTGOING syntax-only-extractor artifact (probe §4.6: the count is
  largely an artifact; ~17% self-index resolution rate SCIP would resolve far higher). SCIP/LiveGraph,
  the current-state substrate, carries NO unresolved-call disposition (probe Q1 = NO). Therefore the
  honest posture is NOT to synthesize a fake current-state number to "match" the old count — it is to
  RETAIN the homegrown source and LABEL it `provenance.source = sqlite`. This is the Fact-Certainty Model
  applied at the storage boundary: where current-state truth does not exist, label the outgoing fact;
  do not manufacture a substrate replacement. The bounded contract is the direct expression of that rule.
```

---

## 2. Scope boundary (what this contract does NOT touch)

```text
CONTRACT-ONLY. In scope: the bounded-decommission CONTRACT (this document). Out of scope, each a SEPARATE
later track [INFERRED, mirroring readiness-10 §"Next priorities" P1–P4]:

  · The retirement IMPL (the actual drop/retire of the (a)∪(b)-covered `nodes`/`edges` reads) — a LATER
    slice, gated on the §6 prerequisites. NOTHING is deletable today.
  · The marginal P1 fastpaths (the residual Option B: orient/explain serve their LG-derivable resolved/
    non-trust leaves on green; keep trust SQLite-labelled) — a SEPARATE opportunistic eager-read-reduction
    track that flips NO deletion gate (readiness-10 §(b)).
  · The P2 non-TS LiveGraph coverage program (closes deletion gate 2 / class (d)) — a multi-slice,
    months-scale strategic track (C GO, Rust GO-with-caveats).
  · The broader 31-non-graph-table decommission (readiness-1 §1) — out of scope for `nodes`/`edges`.
  · ROADMAP.md / CURRENT_SLICE.md reconciliation — done AFTER this contract is ratified, not here.

This contract does not choose among P1–P4 or invent the next BUILD. It defines the bounded SCOPE so that
whichever build the operator picks executes against a precise, honest boundary. [INFERRED.]
```

---

## 3. The read partition this contract is built on (readiness-10 §180; OBSERVED)

```text
Every raw-graph read is partitioned by WHAT CAN HAPPEN TO IT [OBSERVED: readiness-10 §188–227, quoted]:

  (a) ALREADY SQLite-FREE-SERVED — the 6/10 drilldown defaults on green: callers/callees/path (lazy);
      imports/cycles/stats (GREEN-cert fastpath → LiveGraph). They read `nodes`/`edges` ONLY on the cert
      BUILD (once/fingerprint) and on fallback. SERVED-free today.
  (b) LG-DERIVABLE but UNBUILT — the coherence commands' (orient/explain) NON-TRUST, RESOLVED structural
      contributors. Servable on green by a future fastpath; flips NO deletion gate while (c) lives.
  (c) RED-BY-DESIGN-FOREVER — the trust contributor's unresolved-call fields (from `unresolved_edges` +
      `extraction_diagnostics_json`). NO SCIP/LiveGraph source. The substrate boundary the arc proved.
  (d) RED-PENDING-OTHER-WORK — non-TS coverage (gate 2 ceiling); the 6 drilldowns' fallback paths; the
      imports/cycles/stats cert BUILDs; the 31 non-graph tables.

NET [OBSERVED: readiness-10 §222]: (a)∪(b) is the maximum a perfect TS-side program could make SQLite-free.
(c) is a permanent floor on the trust contributor. (d) is the rest. A FULL `nodes`/`edges`/`unresolved_edges`
retirement is unreachable → the goal is re-stated as a BOUNDED partial, (c) explicitly excluded + labelled.
```

---

## 4. CONTRACT ELEMENT 1 — RETIRED SCOPE (the (a)∪(b)-covered reads, on GREEN only)

```text
IN-SCOPE TO RETIRE: the SERVED-PATH `nodes`/`edges` reads covered by (a)∪(b), on GREEN (cert GREEN / answer
Exact) only. Each read names its LiveGraph replacement + its no-loss cert. [OBSERVED grounding cited per row.]
EXPLICITLY NOT RETIRED by this contract: the fallback paths, the cert-BUILD reads, and ALL non-TS reads —
those are class (d), retained until separately closed (§5, §6). Retirement is conditional, never global.
```

| # | Read (default served path) | Class | LiveGraph replacement | No-loss cert / gate | Status today |
|---|---|---|---|---|---|
| 1 | callers / callees served adjacency | (a) | LiveGraph `callers`/`callees` (xref + PartitionIr) | lazy; per-symbol key-set compare; `--engine compare` harness | **already served-free on green** (OBSERVED: readiness-9 §Default-path table) |
| 2 | path (BFS over CALLS/IMPORTS) | (a) | LiveGraph adjacency BFS | lazy | already served-free on green |
| 3 | imports served edges | (a) | `live_import_view` | `build_and_store_import_cert` (once/fingerprint) | already served-free on green |
| 4 | cycles served `nodes`/`edges` (SCC) | (a) | `module_import_cycles` | `build_and_store_cycles_cert` | already served-free on green |
| 5 | stats served `nodes`/`edges` (degree/complexity) | (a) | IR symbol-attributes substrate | cycles/complexity cert machinery (STATS-LIVEGRAPH-1, `28ed216`) | already served-free on green |
| 6 | orient/explain IMPORT_CYCLES (`edges`) | (b) | `module_import_cycles` | cycles no-loss cert **[cert exists]** | **LG-derivable, UNBUILT** (OBSERVED: readiness-10 §194) |
| 7 | orient/explain CALLERS/CALLEES (`edges`) | (b) | `callers`/`callees` | per-symbol no-loss compare **[cert exists]** | LG-derivable, unbuilt |
| 8 | trust/orient `resolved_calls` (`edges` count) | (b) | count IR `EdgeType::Calls` edges | direct count **[LG-derivable now]** | LG-derivable, unbuilt (OBSERVED: trust-summary-livegraph-1 §3a — the ONE LG-derivable field) |
| 9 | orient MODULE_SUMMARY structural counts | (b) | `module_stats` | needs DR-2/DR-E3 re-ratify (SQLite-first today) | LG-derivable-with-divergence (OBSERVED: trust-summary-livegraph-1 §4 — `module_stats` identities differ; not byte-equal without reconciliation) |
| 10 | explain `imports` (`edges`) | (b) | `live_import_view` | import cert | served today on the explain leaf (OBSERVED: explain serves 5 green leaf VALUES, `82b6557`) |

```text
CONTRACT CLAUSE 1 [INFERRED]:
  · Rows 1–5 (class (a)): the on-green SERVED paths are ALREADY SQLite-free. The contract formalizes that
    the bounded decommission inherits this — no new work to retire the (a) served reads; the deletion gate
    for the (a) SERVED path is met (the table drop is still gated on (d), §5/§6).
  · Rows 6–10 (class (b)): IN-SCOPE to retire, but only a future *-SQLITE-FREE-IMPL (P1) actually serves
    them. CRITICAL HONESTY: converting (b) flips NO deletion gate, because (c) survives in the SAME
    orient/explain command — the command still reads `unresolved_edges` every call (readiness-10 §202). So
    (b) retirement is a MARGINAL eager-read reduction, valuable as cleanup, NOT a decommission step.
  · CONDITION on every row: GREEN only. On fallback (non-resident / non-TS / stale / RED) the read returns
    to SQLite — that is class (d), NOT retired here.
```

---

## 5. CONTRACT ELEMENT 2 — RETAINED SCOPE (what stays on SQLite)

```text
RETAINED [OBSERVED: readiness-10 §204–220; readiness-1 §1, §3]:

  (c) THE TRUST UNRESOLVED-CALL FIELDS — `unresolved_edges` table + the `extraction_diagnostics_json`
      aggregate. RETAINED FOREVER, SQLite-labelled (Option A). See ELEMENT 3 — this is the permanent floor.

  (d) RED-PENDING-OTHER-WORK — `nodes`/`edges` retained until these close:
      · NON-TS coverage: LiveGraph is TS-only; every non-TS file/repo falls back to the full SQLite read
        (deletion gate 2, the structural ceiling). [OBSERVED: readiness-9 §Q4(b); readiness-10 §216.]
      · The 6 drilldowns' FALLBACK paths (non-resident / non-TS / stale / RED) read `nodes`/`edges`.
      · The imports / cycles / stats CERT BUILDs read SQLite once per fingerprint.

  THE BROADER NON-GRAPH SET — the 31 non-graph tables (readiness-1 §1: `repos`, `declarations`, `files`,
      `measurements`, the module/surface/boundary/contract/semantic/status families, operational metadata)
      have NO LiveGraph representation and are OUT OF SCOPE for the `nodes`/`edges` decommission entirely.
      A "raw decommission" is really a `nodes`/`edges`-only retirement, never a SQLite retirement
      (readiness-1 §6 note). [OBSERVED.]

CONTRACT CLAUSE 2 [INFERRED — the precise table partition the global framing blurred]:
  · `unresolved_edges` + `extraction_diagnostics_json` → PERMANENT (class (c); Option A; §6). Never dropped.
  · `nodes` / `edges` → CLOSABLE-PENDING (class (d)). NOT permanent, NOT droppable today. Droppable for the
    covered subset only after (b) is served AND (d) [non-TS + fallback + cert-build] is closed for that
    subset. The global drop additionally needs gate 2 (non-TS) for all languages.
  · The 31 non-graph tables → retained for independent reasons (no LiveGraph model); not this slice's target.
  This split is the contract's core correction: "drop nodes/edges/unresolved_edges" was ONE undifferentiated
  goal; it is actually one PERMANENT family (`unresolved_edges` + diagnostics) and one PENDING family
  (`nodes`/`edges`).
```

---

## 6. CONTRACT ELEMENT 3 — PERMANENT-SQLITE FACTS (the (c) floor; the ratifiable boundary)

```text
THE PERMANENT TWO-SOURCE POSTURE — formalized as the TERMINAL Stage-D contract clause.

THE FIELDS [OBSERVED: readiness-10 §206–212; trust-summary-livegraph-1 §3a/§4; the 8-field table]:
  · unresolved_calls / unresolved_calls_external / unresolved_calls_internal_like
  · call_resolution_rate  → and the call_graph + change_impact reliability axes derived from it
  · classifications[] / categories[]  → and unknown_calls_blast_radius
  · enrichment_status / enrichment_state (the DEEPEST gap — the SCIP/LiveGraph pipeline has NO enrichment
    phase; trust-summary-livegraph-1 §3a(e))
  Of the 8 `AgentTrustSummary` fields orient/explain/check consume, exactly ONE (`resolved_calls`) is
  LG-derivable; the other 7 are NEEDS-EXTENSION — and the extension is refuted (probe NO-GO).
  [OBSERVED: trust-summary-livegraph-1 §3a CONSEQUENCE.]

WHY PERMANENT (the substrate boundary) [OBSERVED: probe §3.7 Q1=NO, §4.5 Q2=NO; readiness-10 §143–178]:
  · `IrEdge` is resolved-only by construction; unresolved calls are DROPPED at SCIP ingest — there is no
    `CallObservation` analogue. [trust-summary-livegraph-1 §2b; `94fc506`.]
  · scip-typescript emits NO occurrence for an unresolved call target — "the unresolved call is simply
    ABSENT from the SCIP graph." [probe §3.2/§3.7, OBSERVED decoder output.]
  · Paired same-corpus count: SCIP-recoverable = 0 ≠ homegrown `unresolved_edges` = 3; structurally
    inverted, not a reconcilable classification gap. [probe §4.3/§4.5, OBSERVED.]

THE RATIFIED DECISION [OBSERVED: probe §0 Resolution + §7 DR-TS-0-POST-PROBE → Option A; `7d4b3bb` body]:
  Operator ratified Option A — keep the homegrown `unresolved_edges` (+ `extraction_diagnostics_json`) as
  the trust summary's unresolved-call input, served SQLite-LABELLED (`provenance.source = sqlite`; the
  TRUST-LIVEGRAPH-1 hybrid Half-B shape). The deletion gate for these fields stays RED BY DESIGN.

CONTRACT CLAUSE 3 [INFERRED — elevates the ratified producer-line Option A to the TERMINAL decommission
contract]:
  `unresolved_edges` + `extraction_diagnostics_json` are RETAINED + SQLite-LABELLED FOREVER for the trust
  contributor. The trust contributor is PERMANENTLY TWO-SOURCE: a current-state (LiveGraph) half for the
  LG-derivable fields (`resolved_calls`, module rollups, registry/alias/framework downgrades) BESIDE a
  labelled-outgoing (SQLite) half for the unresolved-call fields. This is the TERMINAL posture, not a
  way-station. No future TS-side or non-TS work flips it (§7). The bounded decommission EXCLUDES these
  fields and their tables by design.

  >> THIS CLAUSE IS THE RATIFIABLE ARCHITECTURE-BOUNDARY DECISION OF THIS CONTRACT. <<
  Option A is ratified at the PRODUCER-LINE level (the probe). Elevating it to a PERMANENT storage-
  architecture contract — declaring `unresolved_edges` + diagnostics a retained-forever family and the
  trust contributor permanently two-source — is foundational, high-blast-radius, and the operator's
  governance call. It is surfaced as DECISION_REQUIRED (§DR-SRD1) rather than silently baked in.
```

---

## 7. CONTRACT ELEMENT 4 — GATE CLOSURE (per-gate disposition + closure criteria)

```text
The 5 deletion gates (readiness-1 §5), re-stated with a precise per-gate disposition. Dispositions:
  CLOSED                  = criteria met today.
  CLOSABLE-PENDING        = criteria reachable; names the work.
  IMPOSSIBLE-BY-SUBSTRATE = unreachable under the current SCIP substrate (the probe is the evidence).
```

| Gate | Statement | Disposition | Grounding |
|---|---|---|---|
| 1 | no default command depends on `nodes`/`edges`/`unresolved_edges` | **SPLIT.** (a) CLOSED for the 6 drilldowns' served path on green; (b) CLOSABLE-PENDING (marginal — flips no gate while (c) lives); (c) **IMPOSSIBLE-BY-SUBSTRATE** (the trust unresolved-call fields); (d) CLOSABLE-PENDING (fallbacks + cert builds). NET: gate 1 can go from all-RED to "RED only for (c)+(d)"; **never fully GREEN**. | OBSERVED: readiness-10 §231–237; probe NO-GO |
| 2 | LiveGraph covers SAME data for ALL languages | **CLOSABLE-PENDING** — TS-only today; non-TS always falls back. The structural ceiling, class (d). Closes only via the P2 non-TS program (months). | OBSERVED: readiness-9 §Q4(b); readiness-10 §238–239 |
| 3 | migration / back-compat story | **SPLIT.** CLOSED for the covered subset's CONTRACT shape (the 6 drilldowns: Auto + labelled fallback + byte-compatible + lazy/cert; the 4 coherence: CoherenceEnvelope + the Option-A hybrid for trust). CLOSABLE-PENDING for the actual `nodes`/`edges` drop (needs the covered-subset (d) story). | OBSERVED: readiness-10 §240–244 |
| 4 | operator reset story (rebuild after deletion) | **CLOSABLE-PENDING** — not reachable yet; the raw graph is today the only multi-language store (the cache is disposable, the raw graph is not). The rebuild-after-deletion path is an IMPL concern, designed when the drop is. | OBSERVED: readiness-1 §5; readiness-10 §245 |
| 5 | per-command parity tests on the new backend | **SPLIT.** CLOSED for (a) (lazy/cert proofs; `--engine compare`). CLOSABLE-PENDING for (b) (the impl's parity tests). For (c): **IMPOSSIBLE-BY-SUBSTRATE** — there is NO parity test to build (0 ≠ 3, structural). The honest "test" is the LABEL (`provenance.source = sqlite`) on the trust unresolved-call leaf, not a no-loss cert. | OBSERVED: readiness-10 §246–250; probe §4.5 |

```text
CLOSURE CRITERIA — what must be TRUE to call each gate closed [INFERRED over readiness-1 §5 + readiness-10]:
  Gate 1: every DEFAULT served path is SQLite-free for `nodes`/`edges` across ALL languages. CLOSABLE only
          for the covered (TS, green) subset; the (c) component is NEVER closable (record it IMPOSSIBLE).
  Gate 2: LiveGraph holds the SAME data for ALL languages (non-TS SCIP ingest matured + resident).
  Gate 3: a labelled-fallback + byte-compatible + operator-reindex migration story for each affected
          command, PLUS the permanent `provenance.source = sqlite` label for the (c) fields.
  Gate 4: a documented rebuild-after-deletion path (operator reset) for the dropped tables.
  Gate 5: a per-command parity proof on the new backend for each RETIRED read (the `--engine compare`
          harness); for (c), the label assertion REPLACES the cert (no parity is achievable).

CONTRACT CLAUSE 4 [INFERRED]: gate 1 is declared PERMANENTLY PARTIAL — its (c) component is recorded
IMPOSSIBLE-BY-SUBSTRATE, not pending. A future audit that reports "gate 1 closed" is WRONG; the correct
terminal state is "gate 1 closed for (a)∪(b)∪(d)-covered subset; (c) impossible-by-substrate, retained +
labelled." No build closes (c).
```

---

## 8. CONTRACT ELEMENT 5 — FUTURE-BOUNDARY (what could move each retained/impossible item)

```text
For each retained / impossible item, the ONLY future work that could change the boundary [INFERRED over the
OBSERVED arc; honest about the floor]:

  (d) NON-TS coverage      → closes gate 2 / class (d). A multi-slice P2 program (C GO, Rust GO-with-caveats).
                             This is the largest strategic unlock; it removes the structural ceiling for
                             `nodes`/`edges`. It does NOT touch (c) — orthogonal (readiness-10 §290–293).
  (d) fallback / cert-build → closable by making the cert SOURCE itself SQLite-free (a structural no-loss
                             proof, not a SQLite compare) AND covering residency/fallback so the drop does
                             not strand the non-resident/stale/RED paths.
  (c) trust unresolved-call → **SUBSTRATE FLOOR.** Under the current SCIP substrate, NOTHING closes it. The
                             only things that COULD are: (i) a new homegrown-equivalent unresolved-call
                             extractor (re-introducing the very artifact the pivot retired), or (ii) a
                             type-inference / enrichment pass over SCIP (a new capability SCIP does not
                             provide). Both are effectively OUT OF SCOPE under the SCIP-first ADR. Honest
                             statement: (c) is permanent for as long as SCIP is the substrate. The S4
                             alternative (a REDEFINED `AST_call_sites − SCIP_resolved` metric) is NOT parity
                             — it is a NEW contract with consumer-threshold migration, not a boundary move
                             for the EXISTING fields. [OBSERVED: probe §7 + DR-TS-0-POST-PROBE.]

CONTRACT CLAUSE 5 [INFERRED]: the contract is honest that (c) is a floor. It does not promise (c) closes
"later"; it records that closing (c) requires LEAVING the SCIP substrate posture for the trust contributor,
which is a different product decision, not a continuation of this slice.
```

---

## 9. CONTRACT ELEMENT 6 — IMPL PREREQUISITES (why nothing is deletable today)

```text
The bounded RETIREMENT IMPL (a LATER slice) is GATED. Stated so the contract is honest that NOTHING ships a
deletion today [INFERRED over readiness-10 §295–301 P3 "needs (b) + (d) closed for the covered subset FIRST"
+ the orient/explain spec PRODUCER blocks]:

  PREREQ-1 (b served): the LG-derivable leaves (§4 rows 6–10) must ACTUALLY be served from the LiveGraph on
    green — the marginal P1 fastpaths. This itself needs, per the arc's open DRs:
      · DR-2 / DR-E3 re-ratify (MODULE_SUMMARY structural counts re-sourced; `module_stats` identity
        divergence reconciled — trust-summary-livegraph-1 §4 RISK).
      · DR-E2 focus-resolution producer (explain's unconditional focus-resolution gap).
    Until served, the orient/explain commands still read `nodes`/`edges` eagerly; the drop would strand them.
  PREREQ-2 (d covered subset): for the covered (TS, green) subset, the FALLBACK paths and the
    imports/cycles/stats CERT BUILDs must be SQLite-free or removed. Dropping `nodes`/`edges` while a
    fallback still reads them BREAKS the non-resident / stale / RED path. The cert source must become a
    structural no-loss proof, not a SQLite compare.
  PREREQ-3 (gates 3+4 for the subset): the migration / operator-reset story for the dropped tables (§7).

CONTRACT CLAUSE 6 [INFERRED]: NOTHING under `nodes`/`edges` is deletable today. Even the bounded drop is
GATED on PREREQ-1 + PREREQ-2 for the covered subset; the GLOBAL drop additionally needs gate 2 (non-TS).
The contract ratifies the GOAL and the BOUNDARY; the IMPL is downstream of these prerequisites.
```

---

## 10. What SHIPPING this contract means

```text
[INFERRED — the explicit statement the packet requires]:

Shipping (ratifying) this contract converts SQLITE-RAW-DECOMMISSION-1 from a permanently-RED GLOBAL goal
("retire `nodes`/`edges`/`unresolved_edges`," now proven impossible) into a BOUNDED, HONEST, shippable goal:

  · RETIRE (gated on §9 prereqs): the (a)∪(b)-covered `nodes`/`edges` served reads on green.
  · RETAIN FOREVER (labelled): `unresolved_edges` + `extraction_diagnostics_json` — the (c) trust
    unresolved-call substrate (the permanent two-source posture).
  · RETAIN-PENDING: `nodes`/`edges` for class (d) (non-TS + fallbacks + cert builds); the 31 non-graph tables.

It RATIFIES four things: (1) the bounded scope; (2) the permanent two-source posture for the trust
contributor (§6, the (c) floor); (3) the per-gate disposition with closure criteria (§7), including gate 1
declared permanently-partial; (4) the impl prerequisites (§9). It DROPS nothing — the impl is a later slice.

The terminal Stage-D slice is thereby a bounded goal that CAN ship, not a stuck global one that never can.
The (c) boundary is recorded in the contract rather than chased through an impossible parity. This is the
honest reconciliation readiness-10 recommended (P3) and the external review asked for (decision-first).
```

---

## 11. DECISION_REQUIRED — the ratifiable architecture-boundary decision

```text
DECISION_REQUIRED:
- ID: DR-SRD1-BOUNDED-SCOPE
  QUESTION: Ratify SQLITE-RAW-DECOMMISSION-1 as THIS bounded partial decommission contract — permanent
    retention + SQLite-labelling of `unresolved_edges` + `extraction_diagnostics_json` (the trust
    contributor's (c) fields) as the TERMINAL Stage-D storage posture, with `nodes`/`edges` retirement
    bounded to the (a)∪(b)-covered subset and PARKED until the §9 impl prerequisites land?
  OPTIONS (exhaustive; every cell filled):
  - Option A (RECOMMENDED) — RATIFY-AND-PARK. Adopt the contract as written: (c) permanent + labelled
    (§6); `nodes`/`edges` bounded-partial, IMPL parked until PREREQ-1 (b served) + PREREQ-2 (d covered
    subset) (§9); gate 1 recorded permanently-partial, (c) IMPOSSIBLE-BY-SUBSTRATE (§7).
      CONSEQUENCE: the terminal slice becomes shippable-bounded; no deletion today; the next BUILD is the
      P1 fastpaths and/or the P2 non-TS program, each a separate slice. Storage-architecture posture:
      `unresolved_edges` + diagnostics declared retained-forever. Aligns with readiness-10 P3 + probe
      Option A + VISION Fact-Certainty. Blast radius: records a PERMANENT two-source posture (irreversible
      in intent; a later product decision could revisit only by leaving the SCIP posture for trust).
  - Option B — RATIFY-AND-PARTIAL-CLOSE-NOW. Adopt the bounded scope BUT declare gate 1 "partially closed"
    immediately for the (a) covered subset, without waiting for PREREQ-1/PREREQ-2.
      CONSEQUENCE: OVERCLAIMS. The (a) served path is free on green, but the `nodes`/`edges` TABLE still
      cannot drop ((d) fallbacks + non-TS read it). Declaring gate 1 "partially closed" before the table is
      droppable risks a FALSE "retirement progressing" trust signal — a Layer-0/Layer-2 confusion VISION
      forbids. NOT RECOMMENDED: cosmetic; nothing is deletable; the served-free state is already recorded
      honestly in §4 without a "gate closing" claim.
  - Option C — DO-NOT-RATIFY-PERMANENCE. Keep (c) as an OPEN item pending a future homegrown-equivalent
    extractor or a type-inference pass; treat `unresolved_edges` retention as temporary-pending, not forever.
      CONSEQUENCE: leaves the terminal slice permanently-RED-and-OPEN (the dead global framing). Contradicts
      the probe NO-GO (no SCIP source) and the ratified Option A. Honest ONLY if the operator intends to FUND
      a homegrown-extractor revival or a SCIP enrichment pass — effectively a NEW track outside the SCIP-first
      ADR. NOT RECOMMENDED unless that funding is intended.
  - Option D (P4) — PIVOT-OFF. Do not ratify a bounded decommission; accept the coherence hybrid as terminal,
    shelve SQLITE-RAW-DECOMMISSION-1 as "bounded; gated," and spend effort on a higher-value track (P2 non-TS,
    the 31-table decommission, warm-cache end-state, or quality discovery).
      CONSEQUENCE: no gate movement; `nodes`/`edges`/`unresolved_edges` stay load-bearing (correct given
      (c)+(d)); the contract is SHELVED rather than ratified-and-parked. Defensible if the operator
      deprioritizes raw-graph retirement entirely; loses the honest bounded-goal framing this contract banks.
  RECOMMENDED: Option A. It is the only option that (i) records the substrate boundary honestly instead of
    chasing an impossible parity, (ii) makes the terminal slice shippable, and (iii) does not mint a false
    gate-closure or trust claim. It matches readiness-10's advisory P3 + the probe's ratified Option A +
    VISION's Fact-Certainty Model.
  BLOCKING_REASON: This is an architecture-boundary + storage-architecture + Layer-1 trust-semantics
    decision. It ratifies a PERMANENT two-source posture (a table family retained forever) and fixes whether
    the terminal Stage-D slice is a bounded-shippable goal or stays a dead global one, and whether gate 1 is
    declared partial-now vs parked. readiness-10 (advisory P3) and the probe (Option A, producer-line) supply
    the evidence, but elevating Option A to the TERMINAL decommission/storage contract — and the
    park-vs-partial-close choice — is the operator's governance call, not settled by them. It blocks any
    SQLITE-RAW-DECOMMISSION-IMPL scoping. Per CLAUDE.md Decision Autonomy ("foundational or irreversible →
    stop") and the packet STOP_CONDITION, it is surfaced here rather than decided unilaterally.
```

---

## 12. Validation / evidence ledger (this slice)

```text
EXECUTED (command run, output observed first-hand THIS turn):
- ls docs/slices/sqlite-raw-decommission-1.md (pre-write) -> "No such file or directory" — confirmed the
  deliverable did not pre-exist; this slice CREATES it.
- git status --short (pre-write) -> empty (clean tree). Confirms a clean baseline; the only change this
  slice introduces is this new contract doc.
- git log --oneline -3 -> HEAD `78feb81` (PRIORITY-DOCS-RECONCILE-4 + readiness-10) ← `7d4b3bb` (probe;
  NO-GO) ← `94fc506` (TRUST-SUMMARY-LIVEGRAPH-1). Confirms this contract sits above the closed arc.

OBSERVED (artifact / doc read first-hand THIS turn — the grounding for every clause):
- docs/slices/sqlite-raw-decommission-readiness-10.md — the (a)–(d) partition (§180–227), the gate
  re-annotation (§229–251), the post-probe served state (§253–271), the P1–P4 matrix (§273–320).
- docs/slices/sqlite-raw-decommission-readiness-1.md — §5 the 5 original gates; §1 the 33-table inventory
  (the 31 non-graph tables); §6 the next-slices framing.
- docs/slices/sqlite-raw-decommission-readiness-9.md — the baseline readiness-10 supersedes (6/10
  served-free; the eager-read finding; the A-vs-B open call; Option B recommended).
- docs/slices/orient-sqlite-free-1.md — orient's 5-path map; trust-core unconditional in all four focuses
  (DR-1, BLOCKING); DR-0 → S3; the PRODUCER UPDATE (DR-1 REFUTED).
- docs/slices/scip-unresolved-call-probe-1.md — §3.7 Q1 = NO (unresolved call ABSENT from SCIP); §4.5
  Q2 = NO (0 ≠ 3, structural); §6 VERDICT NO-GO; §7 + DR-TS-0-POST-PROBE → Option A.
- docs/slices/trust-summary-livegraph-1.md — §3a the 8-field `AgentTrustSummary` table (1 LG-derivable / 7
  NEEDS-EXTENSION); §3a(c)/(e) the classification + enrichment gaps; §4 the broader report-field table.
- docs/VISION.md — § Fact Certainty Model; § Product Layer Model (Layer 1); § Product Principle.
- agent_docs/architecture.md (rule 6 explicit degradation; the layer stack) + agent_docs/validation.md
  (Evidence Law) + docs/documentation.md (slice taxonomy — Status PLANNED).
- docs/ROADMAP.md §Storage (line 136) — the SQLITE-RAW-DECOMMISSION-1 row (NEXT; GATED + PARTIAL BY
  DESIGN). Read for consistency only; NOT edited (out of scope).

NOT RUN (skipped, with reason):
- Build / test (cargo) + ./scripts/dev-install-local.sh — contract/spec slice; no source path touched;
  dev-install restarts the daemon (state-mutating; scripts/** out of scope).
- Live `rmap` capture / daemon start — contract-only; starting the daemon runs index/refresh
  (state-mutating). Every clause is grounded in first-hand reads of the committed arc + readiness-10 (the
  SAME stance readiness-9/-10 and the four arc specs took). No clause depends on a live capture.
```

---

## 13. Guardrails honored

```text
No code. No table deletion. No migration. No decommission. No default flip. No new ratified priority
invented (the next BUILD is left an OPEN governance call; §2). Contract/spec doc only. The readiness docs
and the four arc specs are read-only here (already committed). First-hand reads back every OBSERVED claim.
The (c) boundary is stated precisely (a narrow "SCIP cannot SOURCE a parity unresolved-call count," NOT
"SCIP is inadequate as the substrate") so no false trust/certainty claim is minted. The one genuine open
architecture-boundary decision (ratify the permanent two-source posture + park-vs-partial-close) is
surfaced as DECISION_REQUIRED (§11) with an exhaustive matrix rather than decided unilaterally.
STOP-condition check (packet): readiness-10's partition is CONSISTENT with first-hand state (the four arc
commits + the gate split) — no contradiction; no stop on that ground. The contract does not start the impl
or invent a track — no stop on that ground.
```

## 14. References
- `docs/slices/sqlite-raw-decommission-readiness-10.md` — the end-of-arc re-baseline (PRIMARY grounding)
- `docs/slices/sqlite-raw-decommission-readiness-1.md` §5 — the original 5 deletion gates; §1 the table inventory
- `docs/slices/sqlite-raw-decommission-readiness-9.md` — the superseded baseline (6/10 served-free; Option B recommended)
- `docs/slices/orient-sqlite-free-1.md` (`e10a455`) — orient producer-gap map; DR-1; the PRODUCER UPDATE
- `docs/slices/explain-sqlite-free-1.md` (`f3237f9`) — PRODUCER-GATED; DR-E1 = orient DR-1
- `docs/slices/trust-summary-livegraph-1.md` (`94fc506`) — NEEDS-EXTENSION; the 8-field feasibility table
- `docs/slices/scip-unresolved-call-probe-1.md` (`7d4b3bb`) — NO-GO; the paired empirical evidence; Option A
- `docs/slices/trust-livegraph-1.md` — the shipped hybrid (Half-A posture + Half-B labelled v1) Option A makes terminal
- `docs/VISION.md` § Fact Certainty Model, § Product Layer Model — the layer/honesty grounding
- `docs/ROADMAP.md` (Storage Architecture Track) + `CURRENT_SLICE.md` — reconciled AFTER ratification, not here
