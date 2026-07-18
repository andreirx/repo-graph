# RECON-DESIGN-1 — the reconciliation layer: one graph from two witnesses (SPEC)

Status: SPEC SLICE — analysis + design only, NO code changes (2026-07-17)
Track: Reconciliation (ENGINE-CONSOLIDATION-1 §8b, ratified direction: reconciliation over
adjudication) · Builder: claude-fable-5 (architecture judgment)
Inputs: RECON-SPIKE-1 §5 (the classified fixture divergence: 9 canonical edges, 7
SCIP-only — ALL semantic reference kinds the pipeline never models [new-expression ctor
calls, this.field reads, property/class incoming refs], 0 pipeline-only, 0 identity
mismatch, adoption byte-equal *(re-explained iteration 2: byte-equal because ADOPTED —
§3.5/R-RAT-3)*) · LIVEGRAPH-PARTIAL-FIX-1 (exhaustive walks are now
panic-free) · the EC-1 §4 fact-class table + §8c interim rule.

## 1. The design question

Two witnesses observe the code: the syntax pipeline (all languages, works on
non-compiling code, produces the RED-floor disposition) and SCIP semantic ingestion
(covered languages only, compiler-verified, richer reference kinds). The ratified
direction: reconcile — a union graph with per-edge witness provenance, aggregates over
the reconciled graph, divergence itself a surfaced fact. This spec makes that concrete
and ratifiable.

## 2. Deliverable — written INTO this doc by the builder

**§3 The witness model.** Per-edge provenance representation: witnessed-by-both (the
highest-confidence class — two independent derivations *[wording AMENDED twice, original
contract text retained here: by the WITNESS-INDEPENDENCE ratification, 2026-07-17 —
corroborated by distinct RESOLUTION paths sharing syntax confirmation — and one layer
deeper by the IDENTITY-ADOPTION ratification (iteration 2): identity is ADOPTED from the
pipeline, not independently derived — §3.0a/§3.5/§7.0]*),
SCIP-only ("semantic resolution,
compiler-verified" basis), pipeline-only ("syntactic extraction — works where compilation
fails" basis). Handle: multiplicity; reference-KIND asymmetry (the spike proved SCIP
models kinds the pipeline has NO vocabulary for — ctor-via-new, field reads, property/
class incoming refs: are these NEW edge kinds in the union, or a SCIP-only enrichment
tier? decide with honesty rationale); identity (adoption is byte-equal per the spike
*[iteration 2: byte-equal BY CONSTRUCTION — matched SCIP definitions take the pipeline's
key; §3.5]* — state the assumption + the guard if it breaks).

**§4 The no-second-witness regime (the DOMINANT case).** For uncovered languages/
partitions (all of ../legacy-codebases: C/C++/Java/Python today) the union degrades to
pipeline-only — design how provenance/confidence renders there WITHOUT noise (a repo with
no SCIP coverage must not scream "single witness" on every edge; coverage is a repo/
partition-level fact, per METRIC-LANG-COVERAGE's pattern). The reconciliation must be
a strict generalization: zero-SCIP repos behave exactly as today.

**§5 Serving + aggregates.** How callers/callees serve from the union (replacing the
GREEN/RED binary: what happens to the cert — does it become the witness-agreement
computation itself?); how FC2a-agg computes over the reconciled graph (superseding the
EC-1 §8c interim rule — state the transition); how the divergence RATE surfaces
(doctor/trust — the promotion-funnel pattern); how pipeline-unresolved/SCIP-resolved
sites land as labeled Layer-2 facts without touching the RED-floor denominator.

**§6 Validation design + milestones.** Independently shippable milestones, each
smoke-gateable; the real-TS-corpus validation gap stated honestly (legacy-codebases has
no substantial TS — the deployment-target monorepo run with RMAP_CALLGRAPH_DIFF is the
scale validation; name what it must confirm before which milestone).

**§7 DECISION_REQUIRED list** — every ratification-class choice with alternatives +
trade-offs against the VISION (certainty model, labels-speak-reader's-language,
change-cost doctrine).

---

# DELIVERABLE (builder analysis, 2026-07-17; **revised iteration 6 — FINAL**) — §3–§7 per §2

> **Iteration 6 (2026-07-17, after review-5 revise):** closes review-5's two contract-level
> gaps. The empirical core stays untouched again — both changes are serving/capture
> contracts; no measured figure moves (the S-excess class whose serving changes is
> measured 0 at amodx, §3.3).
>
> 1. **The CAPTURE contract + the exhaustive regime matrix (extends R-RAT-6).** Review-5
> proved two related gaps, both verified first-hand this iteration: (a) a covered,
> resident, `Fresh` partition whose fingerprint MOVED after request capture satisfied no
> W-ONE reason (`stale` requires ¬Fresh; the other two require ¬resident) — an uncovered
> state; (b) the SHIPPED capture contract returns a fingerprint ONLY for a GREEN
> byte-equality cert [OBSERVED: livegraph_feed.rs:63-72 `Some(fp)` = "a GREEN no-loss
> cert"; callgraph_cert/mod.rs:384 the peek's `verdict == "GREEN"` arm] — and semantic
> enrichment GUARANTEES RED (§5.1), so an un-redefined capture makes W-BOTH
> UNREPRESENTABLE on exactly the repos the reconciliation serves. Resolution (§4.2):
> the regime predicate (partition state: covered ∧ resident ∧ `Fresh`) is split from
> request ACTIVATION (capture: build-then-peek, which at M-R2 becomes LEDGER-validity-
> gated, verdict-independent — the GREEN gate stays byte-exact through M-R1; serve: the
> shipped EV-A pin re-check, unchanged). The state space
> `covered × resident × freshness × producer × pinned-match` is classified EXHAUSTIVELY
> (§4.2 matrix — unrepresentable cells named as such); epoch movement and capture
> failure are ORTHOGONAL TRANSIENT FAIL-SOFT states of one request's activation, never
> coverage regimes and never W-ONE reasons (the regime describes partition state the
> reader can act on; the transient describes one request's race, self-healing at the
> next capture — today it is even unmintable mid-request under the W-A serial
> coordinator [OBSERVED: mod.rs:366-374]). The producer axis is confirmed OUT of the
> eligibility predicate (Fresh resident data corroborates regardless of producer
> presence; producers matter for FUTURE refresh — the W-ONE ladder + the stale
> compound). Propagated: §4.2, §5.1, §5.2, §5.3.6, §5.4 doctor, M-R1/M-R2 gates,
> R-RAT-6's record, D-R1.
> 2. **The row/count invariant preserved + the location-claim correction (extends
> R-RAT-5).** Review-5 proved iteration 5's rowless S-excess broke a shipped boundary
> invariant: the serving contract computes `count` FROM row length [OBSERVED:
> livegraph_feed.rs:542,646 `count = …len()`] and the cert builders emit one row per
> edge instance [OBSERVED: callgraph_cert/mod.rs:118-162]. Resolution (§3.3/§5.2): an
> S-excess instance now MINTS a served row via the SAME S-only row mechanism as
> `new_pair` (`witness: "semantic"`, `multiplicity` sub-class) — `count == rows` holds
> universally, rows and instances are 1:1 in W-BOTH dual-measured answers, and the
> instance model gets MORE faithful (every classified instance is one served row; no
> phantom counts). The rowless rationale's premise is corrected everywhere it leaked:
> P rows do NOT carry call-site locations — `find_direct_callers`/`callees` serve the
> OPPOSITE ENDPOINT SYMBOL's definition location (`n.line_start, n.col_start` of the
> caller/callee node) [OBSERVED first-hand: storage/src/queries.rs:672-685, :736-749],
> so nothing in a row identifies which occurrence it denotes and occurrence attribution
> is meaningless at row level on BOTH sides (the `mixed` summary's honesty is thereby
> strengthened, §3.3/§3.3a);
> the ledger-compare row family carries no location at all [OBSERVED:
> agent_impl.rs:1029-1033]; the `edges` table DOES persist occurrence-site columns
> (`line_start`/`col_start`) that NO serving query reads today [OBSERVED: storage
> types.rs:597-611] — which re-prices the S-2 site-attribution upgrade (P-side sites
> are persisted-but-unserved; only the S side needs an IR extension). Propagated:
> §3.1, §3.2, §3.3, §5.2, §5.4, M-R2 gate, R-RAT-5's record, D-R1.
>
> **Iteration 5 (2026-07-17, after review-4 revise at checkpoint — the closing two, both
> operator-RATIFIED):** the empirical core is reviewer-CONFIRMED by independent
> recomputation (both=494, syntactic=13, unmeasured=24, semantic_only=48, union 579,
> agreement 97.4%; collision intersection ∅ over 280 × 2,530 keys) and is NOT disturbed —
> no measured figure moves in this iteration. Two contract-level gaps closed:
>
> 1. **INSTANCE-LEVEL PROVENANCE (R-RAT-5, §7.0).** Review-4 proved §3.3's pair-level
> prose incoherent with §5.4's closure: a P=2/S=1 pair either hid the disagreement (both
> instances `both`, agreement 100%) or left the excess instance unclassifiable
> (`syntactic` required no S call on the pair), and the closure carried no delta term.
> Ratified resolution: provenance classifies EDGE INSTANCES — per dual-measured pair,
> min(P, S-strict-`Calls`) instances are `both`, each side's excess lands in its own
> class under the mechanical sub-class **`multiplicity`** (`syntactic`: S corroborates
> the pair but attests fewer occurrences; `semantic`: count-only, raises the served MAX,
> mints no row *[as this note originally read — AMENDED iteration 6, review-5: an
> S-excess instance now MINTS a served `semantic` row, preserving `count == rows` —
> see the iteration-6 note and §3.3]*); every instance lands in exactly ONE class and `agreement_pct` is
> instance-exact (§3.1, §3.3, §5.1, §5.4, M-R1). Pair-level served labels may SUMMARIZE
> but never claim unconfirmed occurrences: on a P-EXCESS delta pair the per-row
> `witness` is the summarizing **`mixed`** + exact `occurrences: {confirmed, total}`,
> never `both` (an S-excess pair's rows are all fully corroborated — `both` stands; the
> excess is rowless, visible as count > rows *[AMENDED iteration 6: row-minting, as
> above]*) — `IrEdge` carries no per-occurrence site
> [OBSERVED: repo-graph-ir
> lib.rs:364-378], so count-level attribution is the honest maximum today; site-level
> attribution is the named, evidence-gated upgrade (§3.3, S-2). The arithmetic was
> ALREADY instance-level — `edge_magnitude` accumulates min/excess [OBSERVED:
> diff.rs:605-629] and the iteration-4 recompute classifies instances the same way — so
> the ratification aligns the MODEL with the measurement; re-verified from retained
> artifacts alone [EXECUTED: `runs/amodx/iter5-multiplicity-check.py` — 494/13/24/48/579/
> 97.4% reproduced; excess on corroborated pairs **0 in BOTH directions**; split pairs 0].
> 2. **MUTUALLY-EXCLUSIVE COVERAGE REGIMES (R-RAT-6, §7.0).** Review-4 proved §4.2's
> conditions overlapped: a stale-but-resident partition satisfied both W-BOTH
> ("resident") and W-ONE-AVAILABLE ("… or stale"), and the single "available but not
> loaded" message was false for stale data, imprecise for producer-absent. Ratified
> resolution: the regimes are **W-BOTH / W-ONE / W-NONE** (the operator's ratified
> names), mutually exclusive via the ACTUAL serving eligibility the epoch machinery
> already computes — W-BOTH requires resident ∧ status `Fresh` ∧ pinned-fingerprint
> match *[as this note originally read — SPLIT iteration 6, review-5: the first two
> conjuncts are the partition-state ELIGIBILITY (the regime); the pin is the
> request-scoped ACTIVATION, whose capture contract is redefined and whose failure
> modes are orthogonal transients — see the iteration-6 note and §4.2]*, where the
> fingerprint binds per-partition epoch + freshness bit +
> `source_inputs_hash` + `producer_fingerprint` to the pinned snapshot [OBSERVED:
> livegraph_feed.rs:1727-1752; module_cycle_cert.rs:62-78], matched under one read guard
> (the build-then-peek discipline [OBSERVED: callgraph_cert/mod.rs:375-387]). W-ONE
> renders REASON-SPECIFIC (`stale` / `not_resident` / `producer_unavailable` —
> deterministic ladder, §4.2, incl. the measured stale∧producer-absent compound
> [OBSERVED: livegraph_refresh.rs:562-566]); W-NONE is the no-producer capability
> regime. The freshness exclusion is a witness-honesty invariant, not plumbing: a stale
> S beside a current P would mint FALSE divergence classes describing our refresh lag,
> not the reader's code — today that case is silently absorbed by RED→fallback; the
> union retires that channel (§5.1), so the condition must be explicit. Propagated:
> §4.2/§4.3/§4.4, §5.1 ledger scope + refresh, §5.2, §5.3.0, §5.4 doctor, §6.1 gates,
> D-R1.
>
> **Iteration 4 (2026-07-17, after review-3 revise):** applies review-3's four required
> changes, each verified against the artifacts before writing — with one evidence-driven
> deviation from the reviewer's expected arithmetic, stated openly:
>
> 1. **Kind-ALIGNED corroboration (R3-C1).** `both` now REQUIRES an S strict-`Calls` edge
> on the pair: `derive_edges` emits `References` precisely when the occurrence is NOT
> syntax-confirmed as a call [OBSERVED first-hand this iteration: scip-ingest
> lib.rs:727-740], so a same-pair reference is a different-KIND fact and corroborates no
> call resolution — it stays in the reference tier. Classification thereby inherits the
> SAME kind partition §3.4 ratifies for serving (one corpus rule; the old pair-level
> any-kind match was the artifact's kind-blind canonical block leaking into the model).
> The reviewer's expected figures (`both=493, syntactic=14, 97.2%`) were derived from
> iteration-1/2's DEDUCED composition ("493 via-call + 2 via-reference-kind") — the exact
> multiset recomputation [EXECUTED: `runs/amodx/iter4-recompute.py` over the retained
> artifacts + the surviving isolated-run DB; reconstruction reproduces the artifact's
> 531/36/495 exactly] corrects the deduction itself: **`both` = 494 instances (454
> identities), `syntactic` = 13 (12), `unmeasured` = 24 (14), `semantic_only` = 48 (37);
> union calls 579; agreement 494/507 = 97.4%** of dual-measured instances. Exactly ONE
> pair reclassifies (not two): `admin main.tsx:FILE → loadConfig` — a MODULE-INIT call
> (P: FILE-caller CALLS row; S: FileScopeReference BY DESIGN, never Calls) — surfacing a
> third mechanical `syntactic` sub-class, **`file_scope`** (§3.1), which S structurally
> cannot corroborate regardless of P's correctness. The other "via-reference-kind" pair
> was an accounting artifact: its S `Calls` instance had been set-membership-miscounted
> as scip-only. All canonical figures are now population-labeled as EDGE INSTANCES
> (multiplicity preserved — the diff.rs min/excess accounting), identity counts beside
> them (§3.0b) — review-1's rule, applied one level deeper.
> 2. **Collision claim downgraded AND the guard predicate measured exactly (R3-C2).**
> Iteration-3's spelling sweep is relabeled a HEURISTIC pre-signal — it could not
> evaluate `fallback_keys(S) ∩ keys(P)` (no per-node `identity_source`, no complete P key
> set, byte-equal keys string-indistinguishable); its "exact collisions 0" deduction is
> WITHDRAWN. In its place, the predicate itself is now MEASURED: a per-node
> `identity_source` dump (extended retained harness, re-run over the retained SCIP
> indexes; regenerated `ir-edges.tsv` byte-identical, SHA-verified) × P's COMPLETE
> 2,530-key node set (the surviving isolated-run DB) → **intersection ∅** (280 distinct
> fallback keys; exact segments Term 272/Type 4/Namespace 3/Method 1) — the first true
> guard-predicate measurement, satisfying review-3's "a run containing identity sources
> and the complete P key set" (§3.5). The heuristic is post-hoc VALIDATED (exactly 244
> fallback keys touch the S edge set) and superseded.
> 3. **R-0 ⟂ accounting labels resolved (R3-C3).** The pipeline accounting marker is
> INTERNAL/PERSISTED provenance (family contract), never rendered in W-ONE regimes
> *[iteration 5 rename: the two W-ONE-* regimes are now `W-ONE` / `W-NONE` — read
> "outside W-BOTH"]*; the
> human "syntax analysis (all languages)" frame renders ONLY where the two accountings
> co-render (W-BOTH additive blocks). W-ONE default output stays byte-identical — R-0/R-1
> remain exact (§5.3.0 rewritten; §4.4 cross-referenced).
> 4. **D-R4 retired into R-RAT-2's record (R3-C4)** — the ratified trust-ratio decision
> is not re-opened; the §8c letter-amendment record moves to §7.0; the open list now
> holds only genuinely unresolved decisions (D-R1..D-R3, D-R5..D-R8).
>
> **Iteration 3 (2026-07-17, after review-2 escalate):** applies the fourth operator
> RATIFICATION — **FALLBACK-KEY-INVARIANT option A, the EXPLICIT COLLISION GUARD**
> (recorded as R-RAT-4, §7.0). Iteration 2's "structurally distinct fallback key /
> the `:SYMBOL:` infix cannot byte-collide" claim is AMENDED AWAY — review-2 proved,
> and this iteration re-verified first-hand, that both key families share ONE grammar
> (`{repo_uid}:{path}#{name}:SYMBOL:{segment}` — pipeline: extractor.rs:351-354;
> fallback mint: scip-ingest lib.rs:432; `CanonicalKey::from_existing` is a plain
> newtype wrap, no namespace [ir lib.rs:33-35]): non-collision is CONTINGENT on two
> spelling conventions, not structural. The enforceable replacement: the union carries
> a per-key collision guard — `ScipSynthesizedFallback` identities NEVER merge with
> pipeline keys, byte-equal or not; a byte-collision surfaces as the named divergence
> fact **`identity_collision`** and can never classify `both`/corroborated (§3.5
> guard 2; surfacing §5.4; tests §6.1 M-R1/M-R2; S-3 §6.2). Canonical key shape
> UNCHANGED; the reserved-namespace fix (option B) is the RECORDED escalation path iff
> the guard ever fires; option C (prove + freeze disjoint vocabularies) REJECTED as
> brittle. New pre-signal measured from the RETAINED amodx artifacts [EXECUTED, §3.5]:
> **244 fallback-minted keys** live in the S edge set (`Term` 241 / `Type` 1 /
> `Namespace` 1 / `Method` 1; touching 1 of 542 strict `Calls` and 880 of 12,189
> `References` edges); the two segment vocabularies ALREADY intersect up to case
> (`Method` vs `METHOD`) yet full-key collisions are ZERO — case-folded measured,
> exact deduced from the measured segment disjointness (§3.5) *[AMENDED iteration 4,
> review-3: that sweep is a spelling HEURISTIC and cannot evaluate the guard predicate —
> the deduction is withdrawn; the predicate itself is now measured exactly, ∅ over the
> complete key sets — see the iteration-4 note and §3.5]* — so
> the guarded scenario is one spelling convention away from expressible, and today's
> non-collision is evidenced, not assumed. Plus review-2's secondary correction: §7.0's
> R-RAT-1 record now states the property split exactly as §3.0a does (independently
> derived: callee RESOLUTION; shared: detection / anchoring / identity; S-only by
> design: the reference KINDS — no corroboration possible).
>
> **Iteration 2 (2026-07-17, after review-1 escalate):** applies the third operator
> RATIFICATION — **IDENTITY-ADOPTION HONESTY** (recorded as R-RAT-3, §7.0): identity is
> ADOPTED, not independently derived — matched SCIP definitions take the pipeline's
> `ast.stable_key` (`IdentitySource::AstAdopted` [OBSERVED: scip-ingest lib.rs:396-428]);
> unmatched ones mint a labeled, shape-distinct *["shape-distinct" as this note
> originally read — AMENDED by R-RAT-4, iteration 3: same key grammar; see the
> iteration-3 note above and §3.5]* `ScipSynthesizedFallback` key [OBSERVED:
> lib.rs:429-445]; `symbol_to_key` carries the adopted key into all edge derivation
> [OBSERVED: lib.rs:1174-1182]. Worked through §3–§7: the spike's "zero identity
> mismatch" is re-explained as adoption-by-construction wherever cited (§3.0, §3.5,
> §6.2 S-3); the amodx 100%-key-equality join is re-described as DETERMINISTIC REPLAY
> of the same ingest path (the harness calls the same `ingest_partition`), never
> corroboration; "independent derivations" is dropped from the confidence vocabulary
> entirely — "corroborated" = the compiler's semantic resolution confirmed the
> pipeline-detected, pipeline-identified edge. Plus review-1's arithmetic correction
> (§3.0b/§5.4): one denominator per figure, every population labeled — the headline
> agreement is **495/507 = 97.6% of dual-measured canonical edges** *[iteration 4,
> kind-aligned + exact-join corrected: **494/507 = 97.4%** of dual-measured instances —
> §3.0b]* (never 495/531 =
> 93.2%, which blends the 24 S-unmeasured edges into the denominator), and
> canonical-edge counts are never mixed with projection counts in one figure. The amodx
> findings remain the design's empirical core (§3.0b; artifacts retained in `runs/`).
>
> **Iteration 1 (2026-07-17, after review-0 escalate):** applies the two operator
> RATIFICATIONS — **WITNESS-INDEPENDENCE option A** (the "two independent witnesses"
> vocabulary amends to corroboration-by-distinct-RESOLUTION-paths; §3.0b states exactly
> which properties are independent vs shared; §7 records the EC-1 §8 amendment) and
> **FC2A-UNION option A, dual accounting** (§5.3 rewritten: union-computed discovery
> aggregates + the separately named pipeline-pure trust accounting) — plus the now-BINDING
> DATA UPGRADE: the design below is grounded in a REAL multi-partition divergence
> measurement of `../amodx` (8 SCIP partitions, corpus 2430 symbols) and a mixed-language
> measurement of `../zap-engine` (TS covered / Rust+Python uncovered), both run isolated
> with `RMAP_CALLGRAPH_DIFF` on; artifacts + full classification retained in
> `.agent-manager/slices/RECON-DESIGN-1/runs/` (ANALYSIS.md + raw callgraph-diff.json ×2 +
> transcripts + the IR kind join). Review-0's three named corrections are applied
> (three-regime count §4.2/D-R1; §3.6⇄§4.3 consistency — witness classes exist only where
> BOTH witnesses measured; reference-tier label includes WRITES §5.2).
>
> Evidence law (`agent_docs/validation.md`): **EXECUTED** = command run this slice, output
> observed. **OBSERVED** = artifact/code read first-hand this slice. **INFERRED** =
> synthesis over OBSERVED facts, basis stated. **NOT RUN** = skipped, with reason.
> Tree at authoring: HEAD `103f7c9` (this slice doc's own selection commit), working tree
> clean before this edit [EXECUTED: `git status --porcelain` empty; `git log --oneline -3`].
> `rmap` state inspection: `rmap 0.6.0`; operator registry listing (read-only) confirms
> several legacy-codebases repos are real served repos today [EXECUTED: `rmap repo list`].
> Every load-bearing claim below is cited file:line or slice-doc §; the full ledger is at
> the end of the deliverable.

## 3. The witness model

### 3.0 The two witnesses, named precisely

- **Witness P (pipeline):** tree-sitter syntax extraction → resolution over the FC0
  extraction stream → SQLite `edges` (symbol-level CALLS + the 7 relation types + IMPORTS)
  + `unresolved_edges` disposition. ALL languages; works where compilation fails; the ONLY
  source of the RED-floor disposition (EC-1 §4.1, ratified Clause 3). Its per-edge row
  already carries `type` + `resolution` [OBSERVED: `storage/src/queries.rs:676,740` —
  `e.type AS edge_type, e.resolution`].
- **Witness S (SCIP semantic):** per-language SCIP producer → strict ingest → LiveGraph
  `PartitionIr`. Covered languages only (shipped: `scip-typescript`, dev-pinned —
  `livegraph-integration-1c`); compiler-verified; models reference kinds P never emits
  (spike §5.3); carries NO unresolved-call disposition (SCIP-UNRESOLVED-CALL-PROBE-1
  NO-GO — the floor's basis). Identity is ADOPTED from P (§3.0a/§3.5) — S is a
  resolution + reference-kind witness, not an identity witness.

Identity: ONE namespace, pipeline-owned. P derives `repo_<uid>:path#name:KIND`
(`ast.stable_key`); S does not re-derive it — matched SCIP definitions ADOPT it
(`IdentitySource::AstAdopted` [OBSERVED: scip-ingest lib.rs:396-428]), unmatched ones
mint a LABELED fallback key (`IdentitySource::ScipSynthesizedFallback` [OBSERVED:
lib.rs:429-445]) in the SAME key grammar the pipeline uses — non-collision is
contingent, not structural, so fallback keys NEVER merge with pipeline keys: the
R-RAT-4 collision guard (§3.5 guard 2) — and every S edge endpoint resolves through
the adopted-key map `symbol_to_key` [OBSERVED: lib.rs:1174-1182]. The spike's "zero
identity mismatch" is
therefore adoption-by-construction, NOT cross-derivation corroboration (R-RAT-3 —
iteration 0/1's reading is corrected); the amodx run's 100% key-equality join of all
10,300 SCIP-only divergent edges [EXECUTED] is DETERMINISTIC REPLAY (the harness calls
the same `ingest_partition`) — evidence that ingest keying is deterministic, nothing
more. §3.5 states the adoption contract, its real residual risks, and the guards
(incl. the R-RAT-4 collision guard).

### 3.0a Witness anatomy — what is independently derived vs shared (RATIFIED 2026-07-17)

**RATIFIED (WITNESS-INDEPENDENCE, option A — extended one layer deeper by
IDENTITY-ADOPTION, R-RAT-3, iteration 2):** the witnesses are NOT independent
end-to-end. SCIP ingest REUSES the pipeline's own AST extraction: `ast_facts_for_source`
runs `TsExtractor` and harvests its `Calls`-edge locations as `call_sites` [OBSERVED:
`repo-graph-scip-ingest/src/lib.rs:511-546`], and `derive_edges` classifies a SCIP
occurrence as `Calls` ONLY when `is_call_at` matches one of those sites [OBSERVED:
`lib.rs:690-740`, the check at `:734`]. It likewise adopts the pipeline's IDENTITY
(§3.5). Independence therefore holds for **resolution only** — not detection, and not
identity. Property by property:

| Property | Witness P derivation | Witness S derivation | Independent? |
|---|---|---|---|
| **Call-site detection** ("is this a call expression?") | tree-sitter `TsExtractor` Calls edges | the SAME extractor's call sites, via `ast_facts_for_source` + `is_call_at` [lib.rs:511-546, :734] | **SHARED — one detector.** Corroboration says nothing extra about call-siteness |
| **Caller anchoring** (which symbol contains the site?) | extractor scope containment | innermost enclosing MATERIALIZED node over the same `AstFacts.nodes`, bubble-up closure [lib.rs:715, :690-695] | **Shared AST substrate**, distinct closure rules — attribution can diverge (observed candidate class, §5.4) |
| **Callee resolution** (which symbol does it target?) | syntactic name resolution over the extraction stream | the compiler's semantic occurrence→symbol binding (`occ.symbol` → `symbol_to_key`) [lib.rs:720] | **INDEPENDENT — the load-bearing corroboration.** Exactly where P is weakest (heuristic), measured: 5 of amodx's 13 dual-measured `syntactic` instances (4 of 12 identities) are P misresolutions — the compiler bound those names to external APIs (§3.0b) |
| **Identity** (the canonical key) | pipeline key derivation (`ast.stable_key`) — the ONLY derivation | ADOPTED: matched definitions take `ast.stable_key` verbatim (`AstAdopted` [lib.rs:396-428]); unmatched mint a labeled `ScipSynthesizedFallback` key in the SAME grammar — never merged with P keys (the §3.5 R-RAT-4 collision guard) [lib.rs:429-445]; edges key through `symbol_to_key` [lib.rs:1174-1182] | **SHARED — single-sourced (P), adopted by S.** Agreement where matched is BY CONSTRUCTION, so it corroborates nothing; the real risks are a WRONG match, a fallback-key COLLISION (guarded), and the adoption-miss rate (§3.5) |
| **Reference kinds** (reads / writes / type refs / file-scope refs) | absent — no vocabulary | SCIP occurrences + the strict rule [lib.rs:727-740] | **S-only by design** — no corroboration possible; coverage-labeled Layer-1, never claimed corroborated |
| **Unresolved-call disposition** | pipeline-only (the RED floor) | absent (SCIP-UNRESOLVED-CALL-PROBE-1 NO-GO) | **P-only by design** |

**The honest witness model, final form (R-RAT-3):** the PIPELINE is the PRIMARY
witness — detection + identity, all languages; SCIP contributes INDEPENDENT RESOLUTION
plus additional semantic reference KINDS, layered on adopted identity. "Corroborated"
henceforth means exactly: *the compiler's semantic resolution confirmed the
pipeline-detected, pipeline-identified edge*. (Stated plainly: this lands close to the
operator's ORIGINAL intuition — the pipeline as the skeleton, SCIP as a
semantic-enrichment overlay — which the two ratifications recovered from the code
evidence after iterations 0/1 overclaimed independence.)

Three consequences the design builds on: (1) every reader-facing agreement claim below
says **resolution** corroboration, never "independent analyses/witnesses/derivations" —
the call site is attested ONCE (syntax) and the identity is derived ONCE (pipeline);
(2) because detection is shared, an S-only `Calls` edge is BY CONSTRUCTION a call site P
detected but resolved differently or not at all — which makes the §5.5 unresolved-site
join and the §5.4 contested class precise site-level statements, not fuzzy correlations;
(3) because identity is adopted, key-equality joins measure INGEST DETERMINISM and
adoption coverage, never independent agreement — every such figure below is labeled
accordingly (§3.5). EC-1 §8's "two independent witnesses" wording is amended by these
ratifications — recorded in §7 (R-RAT-1, R-RAT-3); the EC-1 doc itself receives a
one-line pointer amendment at commit time by the operator (out of scope here; silent
drift forbidden).

### 3.0b Evidence — the measured divergence at scale (the DATA-UPGRADE runs, 2026-07-17)

Both runs isolated (`/private/tmp` state roots; operator registry SHA-1 identical
before/after; both target repos' `git status` clean after — read-only proven); producer
`@sourcegraph/scip-typescript@0.4.0` under Node 18 re-provisioned at the launchd-env path;
`rmapd` rebuilt from HEAD (includes LIVEGRAPH-PARTIAL-FIX-1). Full classification +
raw artifacts: `.agent-manager/slices/RECON-DESIGN-1/runs/{ANALYSIS.md,amodx/,zap-engine/}`.

- **amodx (the multi-partition case: 8 SCIP partitions, corpus 2430 symbols)** [EXECUTED]:
  canonical edges LG **10,795** / pipeline **531** (480 identities) / SCIP-only
  **10,300** / pipeline-only **36** (25 identities) / shared **495** (455 identities).
  Population, corrected iteration 4: these are directed canonical edge **INSTANCES** —
  multiplicity preserved, keyed by `(caller_key, callee_key)` identity, the diff.rs
  min/excess accounting [OBSERVED: `edge_magnitude` :605-629] — NOT distinct-identity
  counts (iterations 0–3 mislabeled them "identities"); identity counts now measured and
  given in parentheses [EXECUTED iteration 4: `runs/amodx/iter4-recompute.py`, whose
  reconstruction reproduces the artifact's 531/36/495 exactly from the retained tsv +
  the surviving isolated-run DB]. Joining every divergent edge to its IR `EdgeType` via
  a read-only `ingest_partition` harness — a DETERMINISTIC REPLAY of the same ingest
  path, so its 100% join (0 unjoined) attests keying determinism, not identity agreement
  (R-RAT-3); the regenerated dump is byte-identical (SHA-256-verified, iteration 4).
  **Kind-ALIGNED classification (R3-C1 — the §3.1 rule: `both` requires an S strict-
  `Calls` edge on the pair):** of P's 531 call instances, S measured **507**: `both`
  **494 instances (454 identities)**; `syntactic` **13 (12)**; S could not measure
  **24 (14)** (§3.6). Corroboration rate **494/507 = 97.4% of dual-measured instances**
  — the naive 495/531 = 93.2% blends the 24 unmeasured into the denominator (the §3.6
  rule; iteration 1 quoted the blended figure), and iterations 1–3's "495 = 493 via-call
  + 2 via-reference-kind" was a divergence-list SET-MEMBERSHIP deduction the exact
  multiset join corrects on both numbers: one S-`Calls` instance on a pair also carrying
  `References` excess had been miscounted scip-only, and exactly ONE shared pair — not
  two — lacks an S `Calls` edge. That pair, source-verified: `admin/src/main.tsx:FILE →
  #loadConfig` (line 33, `loadConfig().then(…)` at module scope) — a MODULE-INIT call P
  models as a FILE-caller CALLS row and the strict ingest BY DESIGN never emits as
  `Calls` (file-scope callers → `FileScopeReference` [OBSERVED: lib.rs:727-733]); it
  classifies `syntactic`/`file_scope` (§3.1), its S reference fact staying in the
  reference tier. S adds **48 genuine call instances (37 identities; 8.9% of its
  542-instance strict call graph)**; **kind-blind serving would inflate `callers` ~20×**
  (10,795 LG all-kind vs 531 P call-kind instances). The 13, source-verified by CAUSE
  (instances): **5 P misresolutions** (`localStorage.removeItem` → a project
  `CartContextType.removeItem:PROPERTY`, two callers ×1 each [DB-exact; the run record's
  "×2 on one" was wrong]; `next/cache` `revalidatePath` ×2 / `revalidateTag` ×1
  [route.ts:2] → same-named backend functions — the compiler bound these names to
  EXTERNAL APIs) + **7 genuine cross-package calls** (`@amodx/plugins/admin`
  getExtensions/getPluginList, `@amodx/shared` validateUpload ×2, getCountryPack ×3)
  that per-package SCIP structurally cannot see + **1 module-init call** (the
  `file_scope` pair above — a witness-MODEL asymmetry, not a resolution disagreement);
  by the MECHANICAL topology split (§3.1): **boundary 11 instances (10 identities) /
  file_scope 1 (1) / uncorroborated 1 (1)** — 4 of the 5 misresolution instances cross
  partitions; the uncorroborated one is the same-partition removeItem misresolution —
  SCIP-only cross-partition edges: **ZERO**. Multiplicity deltas between P and S-`Calls`
  on `both` pairs: **0 in BOTH directions** (P-excess 0, S-excess 0, split pairs 0
  [EXECUTED iteration 5: `runs/amodx/iter5-multiplicity-check.py`, retained artifacts
  only] — §3.3). Constructor-targeted edges: **13/13 `References`** —
  `is_call_at` does
  not cover `new` expressions (the §3.4 unknown, answered). `livegraph_panic` **0** (the
  fix held); unanswerable **1,071** projections (943 Partial + 128 Unavailable; FILE 772);
  `field_mismatch` 1 (FILE-node display enrichment); identity suspects **0**. Measurement
  blocks byte-EQUAL across two repeat runs in separate state roots (uid-normalized).
  Whole run (index + 8
  preloads + exhaustive walk + 3 MB artifact): **1.77 s wall**.
- **zap-engine (the mixed-coverage case: TS covered — root/infra/tools partitions;
  Rust + Python uncovered)** [EXECUTED]: canonical LG 2,616 / pipeline 1,722 (1,366
  identities) / SCIP-only 2,479 / pipeline-only **1,585** / shared 137 (instances, as
  above). Exact coverage split [EXECUTED iteration 4, same method — the surviving zap
  DB × the artifact's measurability]: pipeline-only 1,585 instances = **29 dual-measured-
  divergent (27 identities; kind-blind — no kind harness was run for zap) + 1,556
  S-unmeasurable (1,212 identities)**: **98.2% of the naive pipeline-only class on a
  mixed repo is uncovered-language COVERAGE, not divergence** (rs 3,440 + py 116
  `Unavailable` projections; iterations 1–3 quoted "27 of 1,585" across the two units —
  the split is now single-unit and closure-exact, 1,585 = 29 + 1,556). Cross-LANGUAGE
  misresolutions observed (TS `.stop()` → a Rust
  `AnimationComponent.stop`; Web Audio `source.start()` → a TS type's `start` property).
  `livegraph_panic` 0.

### 3.1 Witness classes — vocabulary, certainty, reader labels

Per-edge provenance is the WITNESS SET, interpreted inside a COVERAGE context (§4).
Per the WITNESS-INDEPENDENCE + IDENTITY-ADOPTION ratifications (§3.0a), no class claims
"independent witnesses" or "independent derivations"; the corroborated class claims
exactly what is true — two distinct RESOLUTION paths agree on the same syntax-detected,
pipeline-identified edge:

| Class | Meaning | Certainty (VISION layers) | Reader label (labels speak the reader's language) |
|---|---|---|---|
| `both` | a call INSTANCE whose resolution both paths corroborate: on each dual-measured canonical pair, exactly **min(P, S-strict-`Calls`) instances** classify `both` (instance-level, R-RAT-5 — §3.3), **with call-KIND agreement: an S strict-`Calls` edge on the pair is REQUIRED** (R3-C1; kind-aligned with §3.4's union partition — a same-pair S `References` edge is a different-kind fact and corroborates no call, footnote ²) — the compiler's resolution confirmed the pipeline-detected, pipeline-identified CALL, up to exactly the corroborated occurrence count (identity adopted, §3.5) | Layer 0–1; the highest-confidence class — the RESOLUTION is corroborated (detection is shared and identity single-sourced, §3.0a) | "resolution confirmed two ways: syntax-based and compiler-based analysis agree on the target" ² |
| `semantic` (SCIP-only) | compiler-resolved call INSTANCES the syntax pipeline's resolution lacks. TWO mechanical sub-classes (§3.3): **`new_pair`** — instances on pairs P holds no call edge of; amodx: all 48 — and **`multiplicity`** — S-excess occurrences on a corroborated pair (beyond the min); amodx: 0. EACH instance of either sub-class mints a served S-only row via the same row mechanism (§5.2; iteration 6 — the served `count == rows` invariant holds, review-5) | Layer 0–1 extracted fact (deterministic compiler output), language-coverage-conditional | new_pair: "compiler-resolved (the syntax analysis did not resolve this)"; multiplicity: "the compiler found more occurrences of this call than the syntax analysis resolved (N of M)" |
| `syntactic` (pipeline-only, dual-measured) | syntax-resolved call INSTANCES the compiler measured here and does not corroborate — either the whole PAIR (S holds no `Calls`-kind edge on it: the first three sub-classes) or the EXCESS occurrences beyond the corroborated count (the fourth; §3.3). FOUR mechanically separable sub-classes (the pair-level three all OBSERVED, §3.0b, amodx instances 11/1/1; the occurrence-level fourth measured **0** at amodx [EXECUTED, §3.3] and fixture-tested, M-R1): **boundary** (caller and callee lie in different compiler runs — partition topology decides; S structurally could not corroborate THIS edge regardless of P's correctness), **`file_scope`** (the caller is the file/module node — P models module-init execution as a CALLS row; the strict ingest BY DESIGN never emits `Calls` for a file-scope caller [lib.rs:727-733], so S structurally cannot corroborate it either — a witness-MODEL asymmetry, mechanically classifiable, discovered iteration 4), **uncorroborated** (within one compiler run's scope, callable caller — S measured and holds no such call), and **`multiplicity`** (S corroborates the pair but attests FEWER occurrences — the P-excess instances, R-RAT-5; distinct from `uncorroborated`, whose "did not confirm this resolution" label would misdescribe a corroborated pair). The sub-classes describe S's CORROBORATION STRUCTURE, never P's correctness: the source-verified CAUSES cut across them (amodx's 5 misresolution instances: 1 uncorroborated + 4 boundary; 7 genuine cross-package call instances: all boundary; 1 module-init: file_scope) | Layer 0–1 extracted fact + a surfaced divergence fact about the code | boundary: "resolved by syntax across compiler-run boundaries (the compiler analyzed the two sides separately)"; file_scope: "module-initialization call (runs at import time) — the compiler-based analysis models only calls made from inside functions or methods"; uncorroborated: "the compiler's analysis did not confirm this resolution"; multiplicity: "the compiler confirmed this call, but fewer occurrences than the syntax analysis found (N of M)" ¹ |
| *(no class)* single-witness | the second witness did not measure HERE — a partition outside W-BOTH (W-NONE: no producer exists; W-ONE: covered but not eligible — stale / not resident / producer unavailable, §4.2), an unanswerable projection inside a W-BOTH-eligible one (§3.6), or a transient fail-soft answer (pin moved / capture failed — request-scoped, §4.2, iteration 6) | Layer 0–1 — exactly today's fact, unchanged | NO per-edge label — coverage/eligibility is stated at partition level (reason-specific for W-ONE, §4.2), answerability at answer level (§4.3, §3.6), the transient via `fallback_reason` (§4.2/§5.2) |

¹ Cause wording is now grounded in OBSERVED causes (amodx §3.0b): genuine cross-boundary
calls, pipeline misresolutions, and the module-init model asymmetry (file_scope).
Compile-skip / producer-skip causes remain UNOBSERVED
(both corpora compile cleanly) — S-2 (§6.2) keeps the monorepo obligation to confirm the
label wording covers them before the default flip (never guess a cause we haven't seen).
² Wording implements the ratified vocabulary in the reader's frame — "corroborated" =
the compiler's semantic resolution confirmed the pipeline-detected, pipeline-identified
edge (R-RAT-3). Corroboration is INSTANCE-level on the KIND-PARTITIONED call corpora
(§3.2/§3.3/§3.4 — R-RAT-5): an S strict-`Calls` edge on the pair is NECESSARY and
min(P, S-`Calls`) is the corroborated count, because
`derive_edges` emits `References` precisely when the occurrence is NOT syntax-confirmed
as a call [OBSERVED: lib.rs:727-740] — a same-pair reference attests a same-pair
RELATIONSHIP of a different kind (it lives in the §5.2 reference tier) and confirms no
call resolution. A pair-level summary of a P-excess delta pair renders the `mixed` form
with exact counts, never a bare `both`; an S-excess pair's P rows stay `both` — every P
occurrence is corroborated there — and its excess instances mint `semantic` rows beside
them (§3.3, iteration 6 — the R-RAT-5 never-claim-unconfirmed rule). *(R3-C1 retires iterations 1–3's "via_call / via_reference_kind"
bookkeeping sub-count inside `both`: the exact join showed one of its two members was a
set-membership miscount and the other is the `file_scope` sub-class — §3.0b.)* The
section-level line (§5.2)
carries the one-time "the call sites themselves are detected by the syntax analysis"
clarification — per-row labels never re-state it.

The fourth row is the load-bearing certainty distinction: pipeline-only-dual-measured
(divergence: S looked and does not corroborate) ≠ single-witness (coverage/answerability:
S never looked HERE). Collapsing them mints a false claim in both directions — and the
measured magnitude of the error is large: naively, 24 of amodx's 36 and 1,556 of
zap-engine's 1,585 canonical pipeline-only edge instances would be mislabeled divergence
when they are coverage facts (§3.0b, §3.6, §4.3).

### 3.2 Where the union lives — serve-time composition under the epoch pin; no third store

The union is NOT a third store and NOT a new persisted family. It is a read-path
composition over the two existing witnesses at the pinned
`(ready_snapshot_uid, livegraph_fingerprint)` RequestEpoch — exactly where the cert
already reads both sides coherently today [OBSERVED: `callgraph_cert/mod.rs:224-271`;
W-B §3 invariant]. The repeated-use data is not the union rows (recomputed per query,
same as every serve today) but the per-fingerprint AGREEMENT classification — the
**witness ledger** (§5.1), which inherits the existing cert's exact lifecycle: in-memory,
fingerprint-keyed, non-durable, lazily rebuilt [OBSERVED: `mod.rs:61-65` "NOT durable
(rebuilt on restart)"].

Abstraction accounting (the one new mechanism this design introduces): the witness
ledger — concrete consumers: callers/callees union labels (§5.2), trust `witnesses`
block (§5.4), doctor operational block (§5.4), Layer-2 landing (§5.5); axis of
variation: per-fingerprint witness agreement over the union corpus; simpler alternatives
rejected: (a) per-query two-store compare — O(corpus) per request, unaffordable; (b) the
one-bit cert — discards exactly the data all four consumers need (the spike's §1 problem
statement).

Union definition (per symbol, per direction): multiset merge by canonical edge identity
`(caller_key, callee_key)` of P's rows and S's KIND-FILTERED edges (§3.4); per-witness
multiplicities retained; served multiplicity per identity = MAX across witnesses, never
a sum (the spike's canonical-merge rule, review-2-proven [spike §5.3 MAGNITUDE, §5.8-1;
`diff.rs` `EdgeViews::canonical` is the working precedent]). Classification runs at
INSTANCE level over the same multisets — min corroborated, excess per side (§3.3,
R-RAT-5) — so the served MAX equals exactly the number of classified instances for the
identity (min + one side's excess): every served occurrence-slot has exactly one class,
every occurrence-slot is exactly one served ROW in a W-BOTH dual-measured answer
(§5.2 — the shipped `count == rows` contract preserved, iteration 6),
and the closure (§5.4) partitions what is served. Merging is
IDENTITY-SOURCE-CONDITIONAL, not key-bytes-alone: an S endpoint whose node is
`ScipSynthesizedFallback` never merges with a P key even when byte-equal — the R-RAT-4
collision guard (§3.5 guard 2). And the WITNESS CLASSIFICATION runs on the SAME
kind-partitioned corpora as the merge (R3-C1): a pair classifies `both` only via S's
`Calls`-kind edges (§3.1); same-pair S facts of other kinds belong to the reference
tier and never affect call-graph classes — one corpus rule for classification and
serving, not two.

### 3.3 Multiplicity — provenance is INSTANCE-level (R-RAT-5, iteration 5; served-row rule amended iteration 6)

Per-witness multiplicity is preserved as multisets (the fixture's `area→radius` ×2
[spike §5.3 #3]; live: amodx `POST→revalidatePath` ×2 [DB-exact, edge lines 35/38;
iteration 3's second example, `getStoredConsent→removeItem` "×2", was a run-record
error — the DB holds ×1, §3.0b]).

**The instance rule (replaces iterations 0–4's pair-level prose, which review-4 proved
incoherent with the §5.4 closure):** classification operates on directed canonical edge
INSTANCES, per dual-measured pair, on the kind-partitioned corpora (P CALLS vs S
strict-`Calls`, §3.2). For a pair with P multiplicity `p` and S-`Calls` multiplicity `s`:

- **`min(p, s)` instances classify `both`** — the compiler corroborated exactly that
  many occurrences, no more is claimed;
- **a P excess (`p − s > 0` with `s ≥ 1`) classifies `syntactic`/`multiplicity`** — the
  pair is corroborated, the extra occurrences are not (S measured here and attests
  fewer); `s = 0` is the pair-level case (boundary / file_scope / uncorroborated, §3.1);
- **an S excess (`s − p > 0` with `p ≥ 1`) classifies `semantic`/`multiplicity`** —
  each excess instance MINTS a served row via the SAME S-only row mechanism as
  `new_pair` (§5.2) — *[iteration 6, review-5: replaces iteration 5's "count-only,
  mints NO row", which broke the shipped serving contract `count == rows.len()`
  [OBSERVED: livegraph_feed.rs:542,646] and rode a false premise — "no location to
  serve" assumed P rows carry call-site locations; they do not (§3.3a below)]*;
  `p = 0` is `semantic`/`new_pair` (§3.1);
- so **every instance lands in exactly ONE class**, the §5.4 closure and
  `agreement_pct` are instance-exact, and a P=2/S=1 pair contributes 1 `both` + 1
  `syntactic` — agreement 50% for that pair; the disagreement is IN the rate, never
  absorbed (review-4's incoherence: the old prose classified the whole pair `both` and
  "rolled the delta into the rate" through a closure that had no delta term).

This is NOT a new computation — it is what the shipped instrument and the confirmed
figures ALREADY compute: `edge_magnitude` accumulates min/excess per identity
[OBSERVED: diff.rs:605-629 — `shared += min`, each side's saturating excess], and the
iteration-4 recompute classified instances the same way; the ratification aligns the
MODEL with the measurement, so no measured figure moves. Re-verified from retained
artifacts alone [EXECUTED iteration 5: `runs/amodx/iter5-multiplicity-check.py`]: the
confirmed totals reproduce (494/13/24/48; union 579; 97.4%) and
excess-on-corroborated-pairs is **0 in BOTH directions** (split pairs 0) — the
`multiplicity` sub-classes are measured-empty at amodx scale; the fixture's ×2 is
symmetric (both witnesses hold it), so it exercises multiset preservation, not a delta.
The delta classes are designed against and fixture-tested (M-R1); their live incidence
is measured before the default flip (S-2).

**§3.3a The location fact — what a served row's `line`/`column` actually is (review-5's
correction, verified first-hand iteration 6):** iterations 0–5 claimed P's rows carry
"call-site locations". FALSE. The shipped serving queries return the OPPOSITE ENDPOINT
SYMBOL's DEFINITION location: `find_direct_callers` selects `n.line_start, n.col_start`
where `n` is the CALLER node; `find_direct_callees` the CALLEE node [OBSERVED:
storage/src/queries.rs:672-685, :736-749 — the location join is on the endpoint node,
never on `e`]. The ledger-compare row family (`AgentCallerRow`/`AgentCalleeRow`, the
cert's shape) carries NO location at all [OBSERVED: storage/src/agent_impl.rs:1029-1033,
:1081-1085]. Consequences the design builds on: (1) NOTHING in a served row identifies which
occurrence it denotes — same-pair rows share every endpoint-derived field (identity,
name, definition location; only the per-edge-row `resolution`/`type` columns could in
principle differ), so a served row denotes an occurrence SLOT, never a specific
occurrence, on EITHER witness; (2) the `edges` table PERSISTS per-occurrence site columns
(`line_start`/`col_start`) that no serving query reads today [OBSERVED: storage
types.rs:597-611 — nullable columns; population rate unmeasured here], while `IrEdge`
carries none [OBSERVED: repo-graph-ir lib.rs:364-378] — so the S-2 site-attribution
upgrade needs an IR extension on the S side plus a serving change on the P side, NOT a
new P-side fact class; (3) an S-minted row is not location-poorer than its P siblings
in any call-site sense — no served row carries a call site.

**Served labels on a delta pair — the summary rule (R-RAT-5: pair-level labels may
summarize but never claim unconfirmed occurrences):** the served count is MAX per
identity (§3.2) and equals the row count (§5.2); P's rows serve verbatim. The two delta
directions differ:

- **P-excess (`p > s ≥ 1`):** some of P's rows describe occurrences the compiler did
  not confirm — and WHICH is not determinable today, twice over: the S-side
  multiplicity is a count, not a set of sites (`IrEdge` carries no per-occurrence site
  [OBSERVED: repo-graph-ir lib.rs:364-378]), and nothing in a P row identifies which
  occurrence it denotes anyway (§3.3a — rows denote slots, not occurrences). Per-row `witness: "both"` here would
  claim corroboration for an occurrence the compiler may not have confirmed —
  forbidden. These rows instead carry the summarizing form
  **`witness: "mixed"` + `occurrences: {confirmed: s, total: p}`** (human marker:
  "target confirmed by both analyses — N of M occurrences corroborated").
- **S-excess (`s > p ≥ 1`):** every P row IS fully corroborated (min = p), so those
  rows carry `both` honestly; each of the `s − p` excess compiler-side instances MINTS
  a served row via the same S-only row mechanism as `new_pair` (`witness: "semantic"`,
  `multiplicity` sub-class; §5.2) — *[iteration 6: replaces iteration 5's rowless
  design, which minted `count > rows.len()` against the shipped contract
  `count = rows.len()` [OBSERVED: livegraph_feed.rs:542,646] and against the cert
  builders' one-row-per-instance shape [OBSERVED: callgraph_cert/mod.rs:118-162];
  review-5's arbitration: preserve the invariant]* — never a phantom `both` row, never
  an understated `mixed`, and the answer's composition is readable off the rows
  themselves (p rows `both` + s−p rows `semantic`).

An agent filtering `witness == "both"` therefore gets ONLY fully-corroborated rows, and
rows and classified instances are 1:1 in every W-BOTH dual-measured answer (§5.2).
Identity counts beside instance counts: a delta pair belongs to BOTH its classes'
identity populations — the one overlap instance counts never have (measured today: zero
such pairs, so today's identity counts are non-overlapping). **Named upgrade, NOT built
here:** site-exact attribution — retain per-occurrence sites on `IrEdge` (an INGEST-CORE
IR extension; S's `Calls` sites are P's own detected sites, §3.0a, so alignment is
structurally sound) AND serve the P-side occurrence sites the `edges` table already
persists but no query reads (§3.3a) — evidence-gated on the monorepo's measured delta
incidence (S-2), deliberately not bundled (the measured incidence today is zero).

### 3.4 The reference-KIND asymmetry — kind-partitioned union (the §2 decision)

The empirical asymmetry [spike §5.3]: all 7 SCIP-only canonical edges are semantic
reference kinds — ctor-via-`new`, `this.field` reads/writes (×2 multiplicity preserved),
property/class incoming refs, a file-scope import ref. At scale the asymmetry holds and
gains a small genuine-call component [EXECUTED, §3.0b]: S's edge set splits **542 strict
`Calls` instances (491 identities) vs 12,189 `References` (7,069)**, and beyond P's call
graph S contributes exactly **48 genuine call instances (37 identities)** — real calls
P's resolution missed (e.g.
`Toolbar → cn(...)` inside JSX attribute expressions [Toolbar.tsx:172+], same-file helper
calls like `handler → getMasterKey`). (Iterations 1–3 quoted "49 Calls + 10,251
References" — the kind composition of the artifact's kind-blind scip-only lists, a
set-membership accounting; superseded by the exact kind-aligned join, §3.0b.)

The evidence that decides the shape — **the IR already has the kind vocabulary, and the
kind conflation is a LiveGraph SERVING defect, not a model property**:

- `IrEdge` carries `EdgeType::{Calls, References, Imports}` + a graded `EdgeBasis`
  (`SyntaxConfirmedCall` / `DerivedReference` / `FileScopeReference` / `AstImport`…)
  [OBSERVED: `repo-graph-ir/src/lib.rs:76-124`].
- The ingest derives kinds STRICTLY: `Calls` only when an AST call-expression confirms
  it; a file-scope caller is ALWAYS `References` ("Never infers a call from the mere
  existence of a SCIP reference") [OBSERVED: `repo-graph-scip-ingest/src/lib.rs:690-740`].
- But `LiveGraph::callers`/`callees` traverse `ir.edges` KIND-BLIND (`e.dst == target` /
  `e.src == target`, NO `EdgeType` filter) [OBSERVED: `repo-graph-livegraph/src/lib.rs:
  532-534, 652-653`] — today's LG "callers" answer mixes Calls, References and Imports.
  That is why `Circle.radius` (a PROPERTY) had 4 LG "callers" from field reads [spike
  §5.3 #6]. This conflation is unreachable in production ONLY because the byte-equality
  cert turns any semantic enrichment into RED → SQLite fallback [OBSERVED:
  `mod.rs:11-20`; spike run B `cert_verdict: RED`].

**DECISION (D-R2): kind-partitioned union.**

1. **The union CALL graph** (what callers/callees serve) admits P's CALLS rows ∪ S's
   `EdgeType::Calls` edges (basis `SyntaxConfirmedCall` — the strict call graph the IR
   already defines). A compiler-resolved call the syntax pipeline missed joins `callers`
   as a NEW member, witness-labeled `semantic`. Rationale (reader frame): `callers` must
   keep ONE meaning — "code that calls this" — across all languages. Admitting genuine
   calls regardless of witness keeps the answer complete; admitting non-call references
   would shift WHAT QUESTION the command answers per-repo (a certainty-class collapse on
   the primary surface). Measured stakes [§3.0b]: kind-blind union serving would inflate
   amodx `callers` ~20× (10,795 all-kind vs 531 call-kind instances);
   kind-partitioned, the surface gains exactly the 48 genuine call instances (37
   identities; 8.9% of S's 542-instance strict call graph) and nothing else.
2. **S's `References`-kind edges** (field reads, incoming type refs, file-scope import
   refs) form a DISTINCT, explicitly-named enrichment tier — "compiler-verified
   references" — a separate answer section (§5.2), never inflating caller/callee counts.
   Certainty: Layer-1 extracted fact (deterministic compiler output) with PARTIAL
   LANGUAGE COVERAGE stated where it renders (the METRIC-LANG-COVERAGE §2A data-driven
   pattern) — NOT Layer 2 (no inference is involved; the coverage limit is the honesty
   obligation).
3. **The kind-filter fix in LG traversal is a NAMED IMPL prerequisite** (M-R2): the
   union call projection filters `EdgeType::Calls`; the reference tier reads the
   `References` projection; `Imports` edges keep their existing surfaces and enter
   neither.

So the answer to §2's either/or is BOTH, split by the IR's own deterministic kind: new
union members where SCIP witnesses genuine CALLS; an enrichment tier for the reference
kinds. No new classification logic is invented — the reconciliation INHERITS
INGEST-CORE-1's ratified strict derivation.

The iteration-0 unknown ("does `is_call_at` cover `new` expressions?") is now ANSWERED
at scale [EXECUTED, §3.0b]: **all 13 constructor-targeted amodx IR edges are
`References`, zero `Calls`** — the extractor emits no Calls site for `new`, so
ctor-via-`new` lands in the reference tier under strict inheritance (expectation for the
fixture's 7: all `References`; M-R1's gate still RECORDS the fixture's actual per-kind
split, §6.1). A future `is_call_at` extension to new-expressions would move
instantiations into the union call graph — that is an INGEST-CORE derivation amendment,
named in §6.1 as a possible follow-up, deliberately NOT bundled here (one meaning change
at a time on the primary surface).

A structural consequence of shared detection (§3.0a) worth stating once: an S-only
`Calls` edge is always a call site P DETECTED (is_call_at requires the extractor's own
site) whose RESOLUTION differs — P either left it unresolved (the RED-floor site — the
§5.5 join) or bound it elsewhere (a candidate P misresolution — the §5.4 contested
signal). The `semantic` class is therefore never "a call P couldn't see"; it is "a call
P saw and could not (or wrongly did) resolve" — which is exactly the claim the reader
labels make.

### 3.5 Identity — adopted, not derived twice: the adoption contract + the guards (R-RAT-3, R-RAT-4)

**FACT (corrected iteration 2; replaces iteration 0/1's "byte-equal derivations that
must agree"):** there is ONE identity derivation — the pipeline's `ast.stable_key`.
SCIP ingest ADOPTS it: `find_match` joins each SCIP definition to the pipeline's AST
node and takes `ast.stable_key` verbatim (`IdentitySource::AstAdopted`) [OBSERVED:
scip-ingest lib.rs:396-428]; a definition matching NO pipeline node mints a LABELED
fallback key (`IdentitySource::ScipSynthesizedFallback`) [OBSERVED: lib.rs:429-445];
every S edge
endpoint then resolves through `symbol_to_key`, which carries the adopted-or-fallback
key plus an `is_fallback` bit [OBSERVED: lib.rs:1174-1182]. The union therefore merges
over one pipeline-owned namespace — a JOIN, not a cross-check — and the join is
identity-source-conditional per guard 2 below, never key-bytes-alone.

**FACT (corrected iteration 3 — R-RAT-4; replaces iteration 2's "structurally distinct
shape / the `:SYMBOL:` infix cannot byte-collide"):** the fallback key is NOT
shape-distinct. Both key families share ONE grammar,
`{repo_uid}:{path}#{name}:SYMBOL:{segment}`: the pipeline fills `{segment}` with the
serde SCREAMING_SNAKE spelling of its `NodeSubtype` (`FUNCTION`, `METHOD`, `PACKAGE`…)
[OBSERVED: `ts-extractor/src/extractor.rs:345-364` `make_stable_key`, incl. the
`:dupN` disambiguation suffix at :356-363; `indexer/src/types.rs:133` `#[serde(
rename_all = "SCREAMING_SNAKE_CASE")]`]; the fallback mint fills it with the Rust
`Debug` spelling of the SCIP descriptor suffix (`Method`, `Type`, `Term`, `Package`…)
[OBSERVED: lib.rs:432; `kind_of` at lib.rs:135-138]; and `CanonicalKey::from_existing`
is a plain newtype wrap — no namespace, no guard [OBSERVED: repo-graph-ir
lib.rs:33-35]. Non-collision today is therefore CONTINGENT on two spelling conventions
staying byte-disjoint — conventions held in two crates that do not know about each
other, one of which (the extractor) even carries a latent `Debug`-format branch that
would mint TitleCase pipeline segments [OBSERVED: extractor.rs:346-349, the
`unwrap_or_else` arm].

**The measurements, two grades (corrected iteration 4 — R3-C2):**

- *The iteration-3 spelling sweep — a HEURISTIC pre-signal, not a guard evaluation.*
  It classified **244 keys in the S edge set** as fallback-minted BY SEGMENT SPELLING
  (`Term` 241 / `Type` 1 / `Namespace` 1 / `Method` 1) and found the two segment
  vocabularies ALREADY intersect up to case (`Method` vs `METHOD` — the one fallback
  `Method` key sits in a generated `.d.ts`:
  `renderer/.next/types/cache-life.d.ts#cacheLife:SYMBOL:Method`), with zero case-folded
  full-key near-collisions over its 2,631 keys. What it could NOT do (review-3): evaluate
  the guard predicate `fallback_keys(S) ∩ keys(P)` — it had no per-node
  `identity_source`, no complete P key set, and a byte-equal collision would appear as
  ONE string, indistinguishable without source provenance. Its "exact collisions are
  ZERO by deduction" claim is WITHDRAWN — a heuristic with no observed collision
  indication, nothing stronger.
- *The iteration-4 EXACT guard-predicate measurement* [EXECUTED:
  `runs/amodx/iter4-recompute.py`; per-node `identity_source` dump
  `runs/amodx/ir-nodes.tsv` from the extended retained harness re-run over the retained
  SCIP indexes (regenerated `ir-edges.tsv` byte-identical, SHA-256-verified — the
  deterministic-replay property again); P's COMPLETE key set from the surviving
  isolated-run DB, `runs/amodx/pipeline-node-keys.tsv`]: **`fallback_keys(S) ∩ keys(P)`
  = ∅** — 280 distinct `ScipSynthesizedFallback` keys (from 337 fallback node rows;
  exact segments `Term` 272 / `Type` 4 / `Namespace` 3 / `Method` 1) against 2,530
  pipeline node keys, per partition and overall. This is the run review-3 named
  ("identity sources and the complete P key set") — `identity_collision = 0` is now a
  MEASURED value at 250-file scale, not a deduction. The sweep's edge-set count is
  post-hoc VALIDATED (exactly 244 fallback keys touch the S edge set) and superseded.
  Fallback endpoints touch exactly **1 of 542 strict `Calls` instances**
  (`renderer fbpixel.ts trackFBEvent → …#fbq:SYMBOL:Term` — an ambient `window.fbq`
  declaration no AST definition owns, source-verified) and **880 of 12,189 `References`**
  instances. Node-level adoption context: 3,089 IR node rows = `AstAdopted` 2,366 /
  fallback 337 (10.9%) / `AstFileScope` 386 — fallback CONCENTRATES in generated code
  (renderer, dominated by `.next` output: 214 of its 716 node rows ≈ 30%), feeding
  risk 3 below.

So: the fallback population is REAL, today's non-collision is now exactly measured at
this scale (and still CONTINGENT on spelling conventions, not structural), and the
collision that guard 2 bars remains one spelling convention away from expressible —
which is why
option C of the review-2 arbitration (ratify + prove disjoint vocabularies) was
REJECTED as brittle and the EXPLICIT GUARD ratified instead.

**A new ingest observation the guard's implementation must absorb** [EXECUTED,
iteration 4]: `PartitionIr.nodes` holds DUPLICATE keys with distinct identity sources —
all 386 `AstFileScope` keys also exist as `AstAdopted` nodes (scip-typescript emits a
per-file module symbol whose definition matches the AST FILE node). Both sources are
adoption-compatible, so guard 2 is unaffected; but the per-key discriminant must be
computed over a key→sources SET (never assume key uniqueness), and — measured today:
ZERO keys mix `ScipSynthesizedFallback` with any other source — a fallback-mixed key,
if one ever appears, is treated as COLLIDING (conservative; the guard's never-merge arm
already covers it). M-R1's ledger build inherits this rule (§6.1).

**What the old evidence actually showed (re-explained per R-RAT-3):** the spike's "zero
identity mismatch" is adoption-by-construction — matched definitions CANNOT disagree on
the key. The amodx 100%-key-equality join [EXECUTED, §3.0b] is deterministic replay
(the harness calls the same `ingest_partition`): it proves ingest keying is
deterministic — same inputs, same keys; a real and necessary property — but NOT
corroboration by a second derivation. XPART-PROVE-1B's byte-equal keys are likewise
adoption/reconciliation products; it is cited below only for its repair-path precedent.

**The real residual risks** (what CAN break, now that "derivations disagree" cannot):

1. **Wrong match:** `find_match` joins a SCIP definition to the WRONG pipeline node →
   S's edges ride a real-but-incorrect pipeline identity — with guard 2 enforced, the
   ONE remaining class that could mint
   a false `both`. Narrow by construction (the join requires position + name agreement;
   the CJOIN-PROVE-2 name-correspondence rule — range-only joining forbidden — is the
   governing precedent) and symptom-visible as paired divergence (guard 3).
2. **Fallback-key collision (R-RAT-4):** a minted fallback key byte-equals an existing
   pipeline key — same path + name, segment spellings aligned by any future convention
   change (or the latent `Debug` branch above). Absent a guard, the union would
   SILENTLY merge two DIFFERENT entities' facts and mint a false `both` with no wrong
   match anywhere — and, because byte-equal keys are indistinguishable by string, the
   corruption would be retro-undetectable from served data. Not present today — the
   guard predicate measures **∅** exactly at amodx scale (iteration 4, above) — but
   that absence is CONTINGENT on spelling conventions, which is exactly the kind of
   fact a trust claim must not ride on. Guard 2 makes the invariant
   enforced, not assumed.
3. **Adoption-miss rate:** unmatched definitions take fallback keys, which the union
   NEVER merges with
   P keys (guard 2) — their edges can only land in S-only classes or orphan, never corrupt a `both`.
   Measured bounds, now exact for TS [EXECUTED iteration 4]: amodx node-level fallback
   is **10.9% overall (337/3,089 rows) but locally dominant in GENERATED code**
   (renderer ≈ 30%, `.next` type output) while its CALL-graph incidence stays marginal
   (1/542 `Calls` instances) — so even for TS, adoption coverage is a per-PARTITION
   fact, not a flat "high rate"; Rust ingest is
   the on-record extreme — **~94–96% SCIP-synthesized fallback identity**
   (RUST-INGEST-PROVE-1 [OBSERVED: CURRENT_SLICE.md ledger]) — so adoption coverage is a
   PER-PRODUCER, per-partition fact the posture surfaces must show (guard 3), never
   assume.

**GUARDS:**

1. *No-loss:* P's rows enter the union verbatim, so an adoption MISS degrades to
   S-side redundancy/orphaning — never
   loss of a pipeline fact. (This half of iteration 2's "structural safety" never
   depended on key shape and stands.) With guard 2 enforced, only the wrong-match
   class (risk 1) can corrupt a label — exactly what guards 3–4 watch.
2. *The R-RAT-4 per-key COLLISION GUARD (the ratified invariant — enforceable,
   shape-independent):* merging is conditional on IDENTITY SOURCE, never key bytes
   alone. `both`/corroborated REQUIRES the S endpoint's node be adoption-compatible —
   `AstAdopted`, or `AstFileScope`, whose keys ARE the pipeline's own AST keys
   [OBSERVED: ir lib.rs:53-66; ingest lib.rs:452-465 adopts `n.stable_key`]. A
   `ScipSynthesizedFallback` key NEVER merges with a pipeline key, byte-equal or not.
   At ledger build — where both witnesses are already held under the epoch pin (§5.1)
   — the guard computes `fallback_keys(S) ∩ keys(P)` per partition; every member
   surfaces as the named divergence fact **`identity_collision`** — never `both`,
   never silent. Classification consequence: the P pair (if any) classifies as if S
   held no matching pair; the S pair is WITHHELD from union serving under that key
   (serving it would misattribute S's fact to P's entity) and counted (§5.4 — the
   count on trust's witnesses block, the colliding KEYS on doctor). Implementable
   exactly where classification happens: `identity_source` is per-node in the IR and
   reaches the LiveGraph inside `PartitionIr.nodes` [OBSERVED: ingest lib.rs:433-443].
   Canonical key shape UNCHANGED. **Escalation path, recorded (arbitration option B):**
   iff the guard EVER fires in practice, the structural fix is a reserved fallback-key
   namespace — an identity change, so versioning contract + migration path first (the
   VISION decision rule), its own ratified slice. The guard is what makes deferring
   that safe: a collision becomes a visible, counted, serving-safe event instead of a
   silent false `both`.
3. *Detection (adoption quality):* the `identity_suspect` detector — a `syntactic`-class edge whose
   (caller key, callee NAME) matches a `semantic`-class edge's (caller key, callee name)
   under a DIFFERENT callee key — the symptom signature of a wrong or missed adoption;
   counted, surfaced on the posture block (§5.4), NEVER silently merged. Beside it, the
   ingest's own per-partition `matched`/`fallback` counts [OBSERVED: aggregated at
   lib.rs:1167-1170] surface on doctor's operational block (§5.4), making adoption
   COVERAGE a visible per-producer fact (the Rust bound shows it can be the dominant
   class).
4. *Repair path (future, evidence-gated):* an explicit alias with basis + provenance
   (the XPART-1B `export_alias` D3 precedent — no silent rewrite), its own slice iff the
   monorepo shows a nonzero suspect rate (S-3, §6.2).

Pre-signal at 250-file multi-partition scale [EXECUTED, §3.0b + iteration 4]: the `identity_suspect`
detector computed over the amodx artifact returns **0** (no pipeline-only edge shares
(caller, callee-name) with a SCIP-only edge under a different callee key), and the
guard predicate `fallback_keys(S) ∩ keys(P)` measures **∅ exactly** (280 fallback ×
2,530 pipeline keys — the two-grade measurement record above; iteration-3's
heuristic-sweep deduction withdrawn per review-3) WITH a real 280-key fallback
population present — real
absence-of-symptom measurements for risks 1–3 as they manifest in served edges. The 100%
join is determinism evidence only (above). S-3 remains the deployment-scale
confirmation, restated in adoption + collision terms (§6.2).

### 3.6 Answerability is first-class (the spike finding-#0 consequence — now measured)

A witness side can be UNANSWERABLE per symbol/direction. The panic class is fixed —
`AstFileScope` bases now degrade to a clean `Partial` with the reader-frame
`StructuralNodeNoCallGraphContent` reason [OBSERVED: `livegraph/src/lib.rs:553-560,
687-694`; LIVEGRAPH-PARTIAL-FIX-1 §6: `livegraph_panic: 0`, no `catch_unwind`;
CONFIRMED live at scale: 0 panics over both exhaustive walks, §3.0b] — and the
unanswerable classes are COMMON, not exotic [EXECUTED] (population: symbol×direction
projections, 2 per corpus symbol — a DIFFERENT unit than canonical edges, never mixed
into one figure with them): amodx 1,071 of 4,860 projections unanswerable (943 `Partial`
+ 128 `Unavailable`; FILE symbols dominate at 772); zap-engine 3,693 of 5,172 (dominated
by uncovered-language `Unavailable`: rs 3,440 + py 116).

Model — **witness classes exist only where BOTH witnesses measured the projection**
(this is the §4.3 rule applied at symbol/direction granularity; review-0 consistency
fix): an unanswerable side contributes UNKNOWN, never zero edges (the v2/v3 schema
honesty rule [spike §5.1]; VISION: unknown ≠ zero). Consequences:

- (i) edges the measurable side witnesses still SERVE (facts are never withheld), but
  they carry **NO per-edge witness class** — labeling them `syntactic`/`semantic` would
  claim the second witness looked when it could not answer here (the same false claim
  §4.3 bars at partition level, minted at symbol level);
- (ii) in the LEDGER they are the class "single-witness-measured (second witness
  unanswerable here)" — counted in `unmeasured_edges` (§5.4), EXCLUDED from the divergence-rate
  denominator, SHOWN beside the rate (§5.4), never rendered as per-edge noise. The
  answer-level `witness_counts` reports them as `unmeasured` (§5.2) so the composition
  never hides;
- (iii) an unknown target stays `Unavailable` (null ≠ empty) [OBSERVED:
  `livegraph/src/lib.rs:500-506`].

**A measured defect this rule fixes (found in the prototype this iteration):** the spike
instrument's own `canonical_edges` block classifies a P edge whose LG projections were
all unanswerable as `pipeline_only` — divergence blended with coverage — while its
per-symbol buckets correctly populate only when both sides measured [OBSERVED:
`diff.rs` `edge_magnitude` :605-629 vs `diff_direction` doc :653-657]. Measured
magnitude of the blend (instances; §3.0b): amodx canonical pipeline-only 36, of which
only 12 are truly dual-measured under the kind-blind rule (13 kind-aligned — the
file_scope pair moves in from `shared`); zap-engine 1,585 = 29 dual-measured + 1,556
unmeasured (98.2% coverage noise, single-unit closure). The LEDGER's classes therefore follow the
buckets' rule, not the canonical block's: divergence classes require dual measurement;
everything else is `unmeasured`. (The artifact is an off-by-default debug surface; its
canonical-block note gains this caveat in M-R1 — §3.7-5.)

### 3.7 Defects this analysis surfaced (name-vs-semantics; code untouched this slice)

1. **LG kind-blind traversal** (§3.4) — the substrate defect the union IMPL fixes
   (M-R2).
2. **`callgraph_cert/mod.rs:118/:141` doc-comments** say "one row per incoming/outgoing
   `CALLS` edge"; the traversal they wrap is kind-blind [OBSERVED both sites]. The
   comment misleads exactly where the KIND decision lives — fix with M-R1.
3. **`livegraph_feed.rs:490/:509`** hardcode `edge_type: "CALLS"` on rows built from LG
   keys (the `--engine livegraph` dev path): under kind-blind traversal a References
   edge would render as "CALLS". Unreachable on the default path today (RED → fallback);
   an ACTIVE mislabel under any union serve — M-R2 replaces this row builder.
4. **`livegraph_feed.rs:487-489`** serve `file: Some("")`, `line: Some(0)`,
   `column: Some(0)` as "non-null placeholders" for locations the LG does not carry —
   known-zero standing in for unknown (VISION: unknown is never zero). M-R2 requires
   null/absent locations + presentation accepting them.
5. **`diff.rs` `edge_magnitude` (:605-629) blends coverage into divergence** — an edge
   whose second witness never measured lands in a `*_only` class (§3.6; measured: 24/36
   on amodx, ~98% on zap-engine). A debug-surface-only defect (off by default,
   gitignored artifact), but its `note` field claims more than it delivers — M-R1 adds
   the caveat to the note AND builds the ledger on the dual-measured rule the per-symbol
   buckets already implement.

## 4. The no-second-witness regime (the DOMINANT case)

### 4.1 The dominant case, grounded in two named repos

- **`../legacy-codebases/nginx`** — pure C: 260 `.c` + 135 `.h` tracked sources
  [EXECUTED: git ls-files census]. No SCIP producer shipped for C ⇒ witness S absent
  REPO-WIDE. Additional reality this design must not paper over: C import resolution is
  syntax-only (TECH-DEBT #6 overclaim caveat) — P is not merely the only witness there,
  it is a weaker witness, and the reconciliation adds NOTHING for C until a C producer
  exists (gate-2 ceiling, EC-1 §4.1).
- **`../legacy-codebases/spring-petclinic`** — pure Java: 47 `.java` [EXECUTED: census].
  Same regime: no SCIP witness; Java serves the honest degraded attribution path today
  (ROADMAP R-track).
- Several legacy-codebases repos are indexed in the operator registry NOW (OpenXcom,
  buildroot, django, duckdb, grpc-java, langchain4j, …) [EXECUTED: `rmap repo list`,
  read-only] — the no-second-witness regime is the currently-served reality, not a
  hypothetical.
- **`../zap-engine`** — the MIXED case, now measured [EXECUTED, §3.0b]: TS covered
  (3 partitions preloaded), Rust (1,720 divergent-corpus symbols) + Python (58) uncovered
  — one repo spanning regimes per partition/language. Its measurement is why coverage
  scoping is designed at language×partition granularity, not repo granularity: treated
  repo-wide, ~98% of its pipeline-only class would be coverage noise.

On such repos EVERY edge is single-witness — which is precisely why per-edge
"single witness" labels are forbidden noise: a label appearing on 100% of rows carries
zero information and grades OUR pipeline on THEIR surface (the RELIABILITY-REFRAME
lesson, verbatim [OBSERVED: `agent/src/reliability.rs:27-29` "we never grade repo-graph's
pipeline"]).

### 4.2 Witness-coverage regimes — THREE, mutually exclusive, per language × partition (R-RAT-6, iteration 5; eligibility/activation split + the exhaustive matrix, iteration 6)

Coverage derives from the snapshot + resident partitions and appears/disappears by
itself — the METRIC-LANG-COVERAGE §2A mechanism, no hardcoded language list. The model
is **three partition-level regimes** (review-0 count correction — iteration 0 miscounted
them as four); per-symbol ANSWERABILITY inside W-BOTH (§3.6) is a separate axis at a
different granularity, not a fourth regime. Iterations 0–4's conditions OVERLAPPED
(review-4: a stale-but-resident partition satisfied both W-BOTH's "resident" and
W-ONE-AVAILABLE's "or stale") — the regimes are now defined by the ACTUAL serving
eligibility the epoch machinery already computes, so they are mutually exclusive and
collectively exhaustive by construction:

**The W-BOTH eligibility predicate** (per language×partition — the PARTITION-STATE
half; it decides the REGIME): the language is covered AND the partition is RESIDENT
with status **`Fresh`**. Producer presence is deliberately NOT a conjunct: Fresh
resident S data corroborates regardless of whether its producer is still provisioned —
the producer matters for FUTURE refresh, so it enters only the W-ONE reason ladder and
the `stale` compound's named blocker (the matrix below pins this cell). Eligibility is
evaluated over the same `LivePartition` snapshot the fingerprint binds — per partition
its epoch (bumps on every swap), its freshness bit (`fresh` = `status == Fresh`), its
language flag, its `source_inputs_hash`, and its `producer_fingerprint`, all bound to
the pinned `snapshot_uid` + the completeness-policy version [OBSERVED first-hand:
livegraph_feed.rs:1727-1752 — `{id}@{epoch}:f{fresh}:ts{ts}:{hash}:{producer}` +
`snap:` + `pol:`; `LivePartition`, repo-graph-livegraph/src/module_cycle_cert.rs:62-78]
— so regime and fingerprint cannot disagree at capture.

**The ACTIVATION contract** (the REQUEST-SCOPED half; it decides whether THIS answer
actually serves the union — review-5's gap, closed): W-BOTH serving requires the
request to have CAPTURED a pin and the pin to still MATCH at data read.

- *Capture — the post-cert build-then-peek contract.* The shipped capture returns
  `Some(fp)` ONLY for a GREEN byte-equality cert at exactly the current resident
  fingerprint [OBSERVED first-hand: livegraph_feed.rs:63-72 — "`Some(fp)` = a GREEN
  no-loss cert exists at EXACTLY the resident fingerprint";
  callgraph_cert/mod.rs:375-387, the `verdict == "GREEN"` peek arm at :384]. Semantic
  enrichment GUARANTEES RED (§5.1), so carrying that contract forward unchanged would
  make W-BOTH UNREPRESENTABLE on exactly the repos the reconciliation serves — the
  capture contract is therefore REDEFINED with the ledger: **through M-R1** the capture
  stays GREEN-gated byte-exact (the ledger is built by the same warm step; the derived
  GREEN/RED verdict preserves today's semantics — behavior unchanged, M-R1's gate);
  **at M-R2** the callers/callees capture becomes LEDGER-VALIDITY-gated,
  verdict-independent — `Some(current_fp)` ⟺ a witness ledger exists at exactly the
  current resident fingerprint (warm = lazy ledger build over the then-eligible
  partition set; peek = fingerprint-compute + ledger presence under ONE read guard —
  the build-then-peek atomicity discipline retained verbatim [OBSERVED:
  mod.rs:359-374, the swap-exclusion rationale]); the flip rides M-R2's serving flag —
  the default path's capture stays GREEN-gated byte-exact until the recorded default
  flip (§6.1/§6.2). The GREEN/RED verdict remains
  COMPUTABLE (derived from the ledger, §5.1) for its OTHER consumer — the bounded-orient
  cert's LG-as-byte-substitute serving (`RequestEpoch`'s orient/explain capture path
  [OBSERVED: livegraph_feed.rs:39-41]) keeps its GREEN gate: those leaves substitute
  bytes, they do not serve unions (§5.1 "untouched" list).
- *Serve — the EV-A pin re-check, unchanged.* Each serve site recomputes the resident
  fingerprint under the data read guard and compares it to the captured pin; mismatch
  ⇒ that answer fails soft to the pinned SQLite snapshot [OBSERVED:
  livegraph_feed.rs:427-429 (callers), :455-459 (callees)] — never a cross-epoch mix.

**Transient fail-soft states — orthogonal to the regimes (review-5's arbitration:
option "orthogonal transient", not a fourth W-ONE reason):** two request-scoped states
exist INSIDE a W-BOTH-eligible partition set, and neither is a coverage regime:

1. **Pin moved mid-request** (captured, then the fingerprint changed before the data
   read — any witness movement: snapshot advance, swap, load/unload, `mark_stale`):
   the EV-A fail-soft above serves pipeline rows at the pinned snapshot, with NO
   witness fields, for THIS answer; the NEXT request re-captures at the new
   fingerprint and lands wherever the new state is. Today this state is UNMINTABLE
   mid-request (the W-A serial coordinator excludes swaps during a request [OBSERVED:
   mod.rs:366-374]); it becomes real under the DAEMON-CONCURRENCY-1 relax — designed
   for now, exactly like the shipped EV-A discipline it reuses.
2. **Capture failed** (warm ran, no valid ledger at the current fingerprint): per the
   shipped compare, `None` arises ONLY on a storage error during the build; no-LG and
   empty-partition-set yield a verdict, not an error [OBSERVED first-hand:
   mod.rs:218-241 — `.ok()?` sites vs the `Some(false)` arms]. The request serves
   pipeline (today's exact `None`-fingerprint path); the failure is an OPERATIONAL
   fact about US, not the reader's code — it renders on doctor (ledger absent + the
   build-failure reason, §5.4), never as a per-edge or regime label.

Why they are not regimes: a regime is a partition-state FACT the reader can act on
(refresh / load / provision — the W-ONE next actions); these two are one request's
RACE/FAILURE outcome, self-healing at the next capture, with "retry" as the only
action. Classifying them W-ONE would make the posture describe our request timing as
if it were their coverage state (the §4.3 label-class violation). Rendering: the
existing `fallback_reason` channel carries them — today the mismatch folds into
`LiveGraphUnavailable` [OBSERVED: livegraph_feed.rs:290-292 — the `None` arm], a name
that is FALSE for a resident-and-available graph whose pin moved (name-vs-semantics);
M-R2 names the movement case distinctly (decide-and-record: `LiveGraphEpochMoved`,
additive enum value) and keeps witness fields ABSENT on the failed-soft answer.

**The exhaustive state classification** (review-5's required matrix —
`covered × resident × freshness × producer × pinned-match`; pinned-match ∈ {match,
moved, no-pin} is the request axis and only exists where a pin can exist; every cell
named, unrepresentable cells stated as such):

| covered | resident | freshness | producer | pin state | Classification |
|---|---|---|---|---|---|
| no | no | — | — (none exists) | — (no S component) | **W-NONE** (capability truth on doctor; R-0 output) |
| no | yes | any | any | any | **unrepresentable BY DERIVATION** — coverage is data-driven (METRIC-LANG-COVERAGE §2A): resident S data for a language IS coverage evidence, so ¬covered ∧ resident cannot be stated |
| yes | no | — (nothing to be stale) | provisioned | — | **W-ONE(`not_resident`)** |
| yes | no | — | absent | — | **W-ONE(`producer_unavailable`)** |
| yes | yes | ¬Fresh | provisioned | any (its `f0` bit is IN the fingerprint — invalidation only, never eligibility) | **W-ONE(`stale`)** |
| yes | yes | ¬Fresh | absent | any | **W-ONE(`stale`)** + the named blocker on the next action (the measured warm-cache compound [OBSERVED: livegraph_refresh.rs:562-566]) |
| yes | yes | Fresh | provisioned OR absent | match | **W-BOTH, activated** — union serves (producer presence irrelevant to corroboration of already-Fresh data; an absent producer surfaces on doctor's toolchain truth and would gate the NEXT refresh, not this serve) |
| yes | yes | Fresh | provisioned OR absent | moved | **W-BOTH regime, transient fail-soft 1** — this answer serves pipeline at the pinned snapshot, no witness fields; not a W-ONE reason |
| yes | yes | Fresh | provisioned OR absent | no-pin (capture failed) | **W-BOTH regime, transient fail-soft 2** — pipeline serve; doctor carries the operational fact; not a W-ONE reason |

Any witness movement — snapshot advance, partition swap/load/unload, staleness
marking — changes the fingerprint, so "an ACTIVATED W-BOTH serve at a moved pin" is
unrepresentable (the EV-A re-check); the same fingerprint that keys the ledger (§5.1)
is the activation witness — one mechanism, no second eligibility computation.

| Regime | Condition (mutually exclusive by construction) | Serving | Provenance rendering |
|---|---|---|---|
| **W-BOTH** | covered AND the eligibility predicate holds (resident ∧ `Fresh`) | the union (§5.2) when the request's ACTIVATION holds (pin captured + matching at data read); on a transient fail-soft (pin moved / capture failed — above), pipeline at the pinned snapshot for that answer | per-edge witness class ON DUAL-MEASURED PROJECTIONS ONLY (§3.6) + one section-level agreement line; NO witness fields on a failed-soft answer |
| **W-ONE** | covered AND NOT eligible — exactly one REASON per the deterministic ladder below | pipeline — exactly today, byte-identical rows | NO per-edge labels; posture surfaces render the REASON-SPECIFIC line + its concrete next action (below; HONEST-DEGRADATION next-action pattern) |
| **W-NONE** | no shipped producer for the language (nginx C, petclinic Java; zap-engine's Rust/Python partitions today) | pipeline — exactly today | NO per-edge labels; NO new default-output lines (R-0, §4.4); capability truth lives on doctor's existing toolchain/lifecycle section (DAEMON-VISIBILITY precedent) |

**The W-ONE reason ladder** (deterministic — residency splits the space first, then
producer presence; each actual state maps to exactly one reason):

| Reason | Actual state | Reader-frame posture line + next action |
|---|---|---|
| `stale` | resident, status ≠ `Fresh`: `mark_stale` ("inputs changed; refresh not yet run" [OBSERVED: repo-graph-livegraph/src/lib.rs:398-401]) or a refresh in flight (`begin_refresh` → `PrecisionPending` [lib.rs:394-397]) — the rendering distinguishes pending vs in-flight from the existing `RefreshStatus`, one reason | "compiler-side analysis here is out of date (the source changed after the compiler last ran) — refresh `<partition>` to re-enable corroboration"; when the producer is ALSO unavailable — the measured production compound: warm-cache-restored partitions are marked Stale + `DegradationReason::ProducerUnavailable` [OBSERVED: livegraph_refresh.rs:562-566] — the next action names the blocker: "refresh requires `<producer>`, which is not provisioned". A compound FACT on the next action, never a second regime |
| `not_resident` | no resident partition data; producer provisioned | "compiler analysis for `<partition>` is available but not loaded — load it to enable corroboration" |
| `producer_unavailable` | no resident partition data AND the producer is not provisioned | "no compiler analysis is loaded here, and its producer (`<indexer>`) is not provisioned" |

Reasons are mutually exclusive by construction: staleness is a property OF resident
data (a non-resident partition has nothing to be stale), and producer presence splits
the non-resident half. They are also EXHAUSTIVE over W-ONE (review-5's gap, closed):
W-ONE = covered ∧ ¬(resident ∧ `Fresh`) = ¬resident (two producer-split reasons) ∪
resident ∧ ¬`Fresh` (`stale`) — the previously-uncovered "resident ∧ `Fresh` ∧
pin-moved/no-pin" states are NOT W-ONE at all: they are W-BOTH's transient fail-soft
(the matrix above), so the ladder's domain and the matrix partition the whole space.
Review-4's defect — one message ("available but not loaded") rendered for all three
states, false for stale data — is structurally unmakeable: the message is a function
of the reason.

**Why staleness excludes W-BOTH (a witness-honesty invariant, not plumbing):** witness
classes are statements about the reader's code that presuppose both witnesses describe
the SAME source state. A stale S beside a current P would mint FALSE divergence — every
edit since the partition's ingest would surface as `syntactic`/`semantic` classes
describing OUR refresh lag, not THEIR code (exactly the label-class violation §4.3
bars). Today that case is silently absorbed: stale rows diverge from SQLite, the cert
goes RED, the fallback serves [OBSERVED: mod.rs:11-20]. The union RETIRES that channel
(§5.1), so the freshness condition must be explicit — it replaces the cert's accidental
staleness gate with a named one.

A REPO is a mixture of regimes (zap-engine measured: W-BOTH TS partitions beside
W-NONE Rust/Python) — every §5 aggregate, rate and label scopes per
language×partition, never per repo. (Naming: `W-*` avoids colliding
with EC-1's R1/R2/R3 FC2a language regimes and REP-1/REP-2 representation cells; the
iteration-5 renames W-ONE-AVAILABLE→**`W-ONE`**, W-ONE-UNCOVERED→**`W-NONE`** are the
operator's RATIFIED vocabulary, recorded in R-RAT-6 — the names grade the SECOND
witness: measuring (BOTH) / exists but blocked, reason named (ONE) / nonexistent
(NONE); the pipeline witness is present in all three, so no regime ever means "no
witnesses".)

### 4.3 Uncovered ≠ pipeline-only — the distinction that must never collapse

`syntactic` (pipeline-only, dual-measured) is a DIVERGENCE class: S looked and does not
corroborate — evidence about the READER'S CODE, worth a label and a count.
Single-witness is a COVERAGE/ANSWERABILITY fact: S never looked here — evidence about
TOOLING, stated once at partition level (or, for per-symbol unanswerability, folded into
`unmeasured`, §3.6). Rendering the latter per-edge would either falsely imply the
compiler examined and declined (a false Layer-1 claim) or drown the real divergence
signal — the measured ratio is decisive: on zap-engine the real divergence signal (29
dual-measured pipeline-only instances) would drown ~54:1 under the 1,556 coverage
instances; on amodx ~2:1 under the 24 (§3.0b). By construction they are different fields at
different granularities: witness classes EXIST only where both witnesses measured the
projection — a condition requiring the W-BOTH regime (partition granularity, §4.2), the
request's ACTIVATION (a failed-soft answer carries no witness fields, §4.2), AND
per-symbol dual answerability (§3.6); the coverage regime is a partition-level fact.
(This is the CLAUDE.md certainty-class rule applied to provenance.)

### 4.4 The strict-generalization contract (R-0)

**R-0 (checkable; a gate on every milestone):** on a repo with ZERO covered languages,
every default discovery surface (callers, callees, path, orient, explain, trust, check,
stats, modules, map) produces BYTE-IDENTICAL output to today — human AND `--json`.
Mechanism: all witness machinery activates per covered partition; the
witnesses/divergence blocks render only when ≥1 covered language exists (data-driven
absence, not suppression). Gate fixtures: nginx + spring-petclinic isolated dogfood
byte-compare (§6.1). Formally: the union operator with witness S absent is the IDENTITY
on witness P — a strict generalization, not a parallel mode. Scope note: doctor (an
OPS surface, not a repo-facts discovery surface) MAY gain witness-capability truth in
its existing toolchain section without violating R-0 — recorded in D-R1. Accounting
note (R3-C3): the §5.3 `accounting` markers are INTERNAL/PERSISTED provenance outside
W-BOTH (the W-ONE and W-NONE regimes, §4.2) — a rendered accounting label exists ONLY
inside W-BOTH's additive dual-
accounting blocks (§5.3.0), so R-0's byte-identity claim covers the pipeline figures
unconditionally.

**R-1 (the MIXED-repo corollary; same gate class):** on a repo with covered AND
uncovered partitions (zap-engine is the named fixture), rows/answers whose symbols lie
wholly in uncovered partitions carry NO witness fields and byte-identical row CONTENT;
witness machinery renders only for covered-partition answers, and every aggregate the
witnesses block quotes is scoped per language×partition (§4.2). R-1 differs from R-0
exactly where honesty requires it: the repo-level posture surfaces (trust/doctor) MAY
name the covered subset — coverage is a fact about the repo — but per-edge and
per-answer rendering follows the answer's own partition regime.

## 5. Serving + aggregates

### 5.1 The cert's fate — the byte-equality verdict retires; the comparison becomes the witness ledger

Today [OBSERVED: `callgraph_cert/mod.rs`]: `callgraph_compare_is_exact` walks the union
corpus, SHORT-CIRCUITS at the first divergence (`return Some(false)`, :256/:265) → one
bit. GREEN licenses serving LG rows as byte-substitutes for SQLite's; RED — which
semantic enrichment GUARANTEES (spike run B is RED *because* S is richer) — forces the
SQLite fallback. The cert therefore structurally punishes the win the reconciliation
exists to serve.

Under the union the premise inverts: **no-loss versus today holds BY CONSTRUCTION** (the
union contains P's rows verbatim, §3.2), so byte-equality stops being the serving
license. The comparison machinery — corpus walk, fingerprint keying, epoch coherence,
shared row builders [OBSERVED: `mod.rs:84-162`] — is RETAINED and generalized into the
**witness ledger**: per-fingerprint, in-memory, non-durable (the cert's exact
lifecycle), holding per-canonical-edge INSTANCE witness classes (`both` —
S-strict-`Calls`-corroborated, min per pair, §3.1/R3-C1/R-RAT-5 / `semantic` with its
new_pair/multiplicity sub-classes / `syntactic` with
its boundary/file_scope/uncorroborated/multiplicity sub-classes, §3.1/§3.3 — each delta
pair's exact `(p, s)` multiplicities retained for doctor's enumeration /
field-mismatch), per-side
answerability (§3.6), the `identity_suspect` count (§3.5 guard 3), the R-RAT-4
collision-guard verdicts (`identity_collision` = `fallback_keys(S) ∩ keys(P)`, computed
here because the ledger is exactly where both witnesses' key sets are held under one
pin — §3.5 guard 2; the key→identity-source map is a key→sources SET, tolerating the
measured adoption-compatible duplicates, §3.5), and the divergence summary
(§5.4). Five rules the DATA-UPGRADE measurements and the iteration-5 ratifications make
non-negotiable: **(a) divergence classes require DUAL MEASUREMENT** — a single-measured
edge is `unmeasured`, never `*_only` (§3.6's fix of the prototype's canonical-block
blend); **(b) every ledger rollup is keyed per language×partition** (§4.2 — zap-engine's
98%-coverage-noise lesson), with the union call graph additionally kind-filtered per
§3.4; **(c) classification is KIND-ALIGNED** — `both` requires S's `Calls`-kind edge on
the pair (§3.1/R3-C1 — the exact recomputation showed the kind-blind pair match minted
a false `both` on the file_scope pair and miscounted one corroborating S-`Calls`
instance as scip-only, §3.0b); **(d) classification is INSTANCE-level** — min
corroborated, excess per side into the `multiplicity` sub-classes, every instance in
exactly one class (§3.3/R-RAT-5 — the rule `edge_magnitude` already computes
[diff.rs:605-629]); **(e) the ledger's SCOPE is the W-BOTH-eligible partition set at
its fingerprint** (§4.2/R-RAT-6) — a partition outside eligibility (W-ONE / W-NONE)
contributes NO classification rows and its serving never consults the ledger (a stale
partition classified against a current P would mint false divergence, §4.2); per-symbol
unanswerability INSIDE eligible partitions remains the separate §3.6 axis. The spike's
`diff.rs` collector is the working prototype of exactly this
computation — schema v3's per-symbol buckets already implement rule (a), its min/excess
canonical accounting already implements rule (d), and its
`canonical_edges`/`rollup` are the ledger's classes MINUS the remaining rules [spike
§5.1; §3.7-5]; it graduates from env-gated debug artifact to serving substrate.

**Transition compatibility (M-R1):** the stored GREEN/RED verdict becomes DERIVED from
the ledger — GREEN ⟺ zero divergent symbols ∧ zero unmeasured projections ∧ zero field
mismatch, on the measured path (exactly the equivalence `diff.rs` documents, degenerate
paths excluded per spike §5.9-2) — behavior byte-unchanged on both fixture classes
(faithful-mirror ⇒ GREEN, drop-calls ⇒ RED). After M-R2 flips callers/callees to union
serving, the GREEN/RED gate RETIRES for those surfaces — WHICH INCLUDES THE CAPTURE
SITE (iteration 6, review-5): the `RequestEpoch` fingerprint capture for
callers/callees is today GREEN-gated [OBSERVED: livegraph_feed.rs:63-72;
callgraph_cert/mod.rs:384] and becomes LEDGER-VALIDITY-gated, verdict-independent, at
M-R2 (the §4.2 activation contract — without this redefinition a divergent union could
never receive a fingerprint and W-BOTH would be unrepresentable on enriched repos, the
exact repos this track serves; build-then-peek atomicity retained verbatim). The
derived verdict REMAINS COMPUTABLE for its other consumer — the bounded-orient cert's
byte-substitute serving keeps its GREEN gate (it substitutes bytes, it serves no
union). **Untouched:** every OTHER
no-loss cert (imports, cycles, stats, focus-resolution, bounded-orient) — those witness
LG-as-CACHE over SQLite-OWNED classes (FC2b/FC1), the mechanism EC-1 §4.4 prices as
permanent; this design is FC2a-content-scoped and does not reach them. The W-B epoch
invariant is untouched: the ledger is keyed by the same fingerprint the RequestEpoch
pins; eviction degrades to the pinned SQLite snapshot exactly as ratified.

Cost, honestly: today's build is CHEAP on divergent repos (early short-circuit) and
full-walk only on GREEN ones; the ledger ALWAYS pays the full exhaustive walk. First
real measurement [EXECUTED, §3.0b]: the ENTIRE amodx run — index (250+ files) + 8
preloads + the exhaustive 2,430-symbol walk + the 3 MB artifact write — completes in
**1.77 s wall**; zap-engine (2,586 symbols) in 0.79 s. The walk itself is a fraction of
either. Same lazy per-fingerprint caching as today [OBSERVED: `mod.rs:306-335`]; no NEW
stall class under concurrent dispatch (GREEN repos already pay the full walk per
fingerprint). The 160k-file monorepo remains the honest cost gate → S-1 (§6.2) gates the
default flip; the named lever if it bites is partition-scoped/incremental ledger build
(the per-language×partition keying in rule (b) above is already the natural partition
seam).

### 5.2 callers/callees served from the union

Ladder per §4.2: **W-BOTH (activated)** → union rows: P's rows verbatim (all their
fields — their `line`/`column` being the opposite-endpoint symbol's DEFINITION
location, §3.3a, never a call site) tagged with their witness class; PLUS S-minted
`Calls`-kind rows (`new_pair` instances AND S-excess `multiplicity` instances, §3.3 —
one row per instance) enriched via `symbol_context` exactly like the cert's shared row
builders [OBSERVED: `mod.rs:104-162`]. **W-BOTH, transient fail-soft** (pin moved
mid-request / capture failed, §4.2) → pipeline rows at the pinned snapshot, NO witness
fields, `fallback_reason` named (the existing channel; `LiveGraphEpochMoved` for the
movement case — §4.2). **W-ONE / W-NONE** → today's pipeline serving, unchanged bytes
(a stale partition serves pipeline, never union). Both sides
read under ONE RequestEpoch pin.

Response contract — additive, per the change doctrine (low blast radius on a discovery
surface; JSON additive, human suffixed):

- each row gains `witness: "both" | "semantic" | "syntactic" | "mixed"` — present ONLY
  in W-BOTH AND only when this answer's projection is dual-measured (§3.6); on a
  single-measured projection the field is ABSENT on every row (unknown corroboration is
  not a row property). `mixed` appears ONLY on the rows of a P-EXCESS occurrence-delta
  pair (§3.3 — an S-excess pair's P rows are all fully corroborated and carry `both`;
  its excess instances mint `semantic` rows beside them, iteration 6) and carries
  `occurrences: {confirmed, total}` — `both` is RESERVED for
  fully-corroborated rows, so filtering on it never captures an occurrence the compiler
  did not confirm (the R-RAT-5 never-claim-unconfirmed rule);
- `edge_type` keeps the kind vocabulary (every union CALL row is `CALLS`); `resolution`
  keeps today's values (compat);
- `count` = union multiset size (per-identity MAX, §3.2/§3.3) **= `rows.len()`** — the
  shipped boundary contract computes `count` FROM row length [OBSERVED first-hand:
  livegraph_feed.rs:542,646] and the cert builders emit one row per edge instance
  [OBSERVED: mod.rs:118-162]; the union preserves that invariant EXACTLY (iteration 6,
  review-5 — rows and classified instances are 1:1 in a W-BOTH dual-measured answer:
  P instances serve P rows, S-only instances of both sub-classes serve S-minted rows,
  §3.3). JSON adds
  `witness_counts: {both, semantic_only, syntactic_only, unmeasured}` — population: the
  union call INSTANCES of this answer, which under the 1:1 rule is also its row
  multiset partitioned by class (instance classification per §3.1/§3.3/§3.6) — so
  composition never hides;
  `unmeasured` carries the §3.6 single-measured case (repo-wide on amodx, 1,071 of 4,860
  symbol×direction projections are single-measured: rows on such projections would
  otherwise vanish or, worse, masquerade as divergence);
- S-minted rows (`new_pair` + `multiplicity`, one mechanism) carry `file` from
  `symbol_context` and NULL `line`/`column` — the field's meaning being the endpoint
  symbol's definition location (§3.3a), which the LG does not carry: unknown, never 0
  (retiring defect §3.7-4); a FALLBACK-keyed S row has no SQLite
  context row, so its unknown fields stay null (measured rarity in the call
  projection: 1 of 542 strict `Calls` at amodx scale, §3.5); a key the R-RAT-4 guard
  flags as colliding is WITHHELD entirely, never served under the pipeline's identity
  (§3.5 guard 2);
- human output: a compact per-row marker + ONE section line ("N callers — M resolution
  confirmed by both analyses, K compiler-only; call sites are syntax-detected" — the
  §3.1 ² clarification lives HERE, once) ONLY in W-BOTH on dual-measured answers;
  zero-SCIP output byte-unchanged (R-0).

**The reference tier** (§3.4-2): a separate `references` block (JSON) / section (human)
on callers/callees/explain SYMBOL focus — **"compiler-verified references (reads /
writes / type references)"** — writes included per the spike's own observations
(`this.radius = radius`, spike §5.3 #4/#6; review-0 correction) — budget-disciplined:
a hot symbol carries hundreds of incoming references already at 250-file scale
[EXECUTED: amodx max fan-in 456 (`ui/label.tsx#Label`), top-8 ≥ 268, mean 5.8 —
§3.0b], thousands-plus at monorepo scale; truncate-with-count per orient's
budget-ladder precedent; S-4 sizes the budget. Present only in W-BOTH.

**path:** stays pinned-SQLite — the ratified W-B D-CC / EC-1 D-EC-6-C posture is NOT
disturbed by this design; a union BFS would need coherence machinery path deliberately
does not have. Named here so the omission is a decision, not drift.

### 5.3 FC2a-agg — DUAL ACCOUNTING over the reconciled graph (FC2A-UNION option A, RATIFIED 2026-07-17)

The interim rule [EC-1 §8c, ratified]: persisted FC2a-agg = pipeline-derived, "one
coherent accounting, matching the trust denominator", EXPLICITLY TEMPORARY. **The
ratified transition (FC2A-UNION option A): discovery aggregates compute over the
coverage-labelled UNION; the trust ratio retains a separately NAMED pipeline-pure
accounting — numerator and denominator on ONE graph, the ratified D-EC-7 principle.**
Option B (make the interim rule terminal) was REJECTED by the operator. This section is
the complete persistence/refresh/serving story for BOTH accountings.

**5.3.0 The two accountings, named so they cannot be confused.**

| | **PIPELINE accounting** (the trust accounting) | **UNION accounting** (the reconciled/discovery accounting) |
|---|---|---|
| Granularities | `g1`/`g2`/`g3` exactly per EC-1 §3.2/D-EC-7 | `g1u`/`g2u`/`g3u` — the same three granularities over the kind-partitioned union CALL graph (§3.4), coverage-labelled per language×partition (§4.2) |
| Computed from | the FULL resolution stream, ALL languages, at index/refresh time (D-EC-7-A) | the union at the pinned `(snapshot, livegraph_fingerprint)` witness pair — deterministic GIVEN the pair [EXECUTED: amodx measurement blocks byte-equal across repeat runs, §3.0b] |
| Lives | PERSISTED FC4-shaped rows (M-3a/M-3b, unchanged) | the witness ledger — in-memory, fingerprint-keyed, non-durable (D-R8); NOT persisted (rationale below) |
| Consumes | the trust RATIO (numerator beside its RED-floor denominator); M-3a/M-3b parity windows; every surface in the W-ONE / W-NONE regimes | discovery surfaces in W-BOTH: orientation call totals (g1u), fan-in/out + liveness overlays (g2u), map's sketch pairs (g3u), the §5.4 witnesses block |
| Marked | `pipeline` accounting is a property of the PERSISTED family, carried at the artifact-contract level (the family's contract names its accounting — the existing provenance field of the artifact contract registry; no per-row column, no output change); it is INTERNAL/PERSISTED provenance and is NEVER rendered on W-ONE / W-NONE default output (R-0/R-1 byte-identity governs). The human frame "syntax analysis (all languages)" renders ONLY where the two accountings co-render — inside a W-BOTH additive block, disambiguating the pair | every SERVED union value carries `accounting: "union"` + its coverage basis (`languages`, `partitions`, `fingerprint`); human frame: "reconciled — combined analyses (coverage: …)". Union values exist ONLY inside W-BOTH additive blocks, so this obligation never touches R-0 |

Naming decide-and-record: the EC-1-ratified identifiers `g1/g2/g3` are NOT renamed —
they denote the pipeline family; the union family is the `-u` suffix.

**The labeling rule, scoped so R-0 stays exact (R3-C3 — review-3 change 3):** a UNION
value served anywhere without its `accounting` + coverage basis is a defect; a pipeline
value rendered INSIDE a W-BOTH dual-accounting block without its disambiguating frame
is a defect; and OUTSIDE W-BOTH, pipeline values render exactly as today — unlabeled,
byte-identical (R-0/R-1) — while the persisted family still carries its accounting
property internally. Iterations 1–3's blanket "a value without an `accounting` label is
a defect" contradicted R-0 (it would have relabeled every zero-SCIP surface); the
obligation attaches to the union accounting and to DUAL-rendered contexts, not to every
value everywhere. M-R3a pins both halves: the §5.3.1 byte-invariance test (W-ONE / W-NONE /
ledger-absent output unchanged) AND the labeling test (every W-BOTH block value
labeled).

**5.3.1 The trust RATIO is pipeline-accounting PERMANENTLY** (ratified via FC2A-UNION-A;
recorded, with its §8c letter-amendment consequence, in §7.0 R-RAT-2 — R3-C4 retired the
old D-R4 stub from the open list). `resolved_calls` (g1) and the unresolved
denominator are two halves of ONE witness's accounting: `resolved(P) / (resolved(P) +
unresolved(P))` [g1: `count_edges_by_type(CALLS)`, trust/src/service.rs:875, one core →
five surfaces, per EC-1 §3.4-8; denominator: the RED-floor disposition]. A union
numerator over the pipeline denominator would silently INFLATE the ratio — the false
trust claim §8b bars. §8c's "temporary" resolves as: temporary as the ONLY accounting
(the union accounting now exists beside it), permanent as the RATIO's accounting — the
letter-amendment is recorded in §7 (R-RAT-2). **Named invariance test:**
trust/check/orient/explain/stats output byte-identical with the ledger absent vs
present, EXCEPT the additive, explicitly-labeled union blocks.

**5.3.2 g1u — union call totals for orientation surfaces.** Content: per
language×partition, the union CALL-graph totals + witness-class split
`{union_calls, both, semantic_only_calls, syntactic_only, unmeasured_edges}` (the §5.4
union-call fields — one unit, edge instances) — this IS the ledger's rollup (one
computation, §5.4's
shared projection serves it). Serving: the §5.4 witnesses block (trust posture + doctor
ops); orient/stats in W-BOTH may quote the union total as an ADDITIVE labeled line
beside the pipeline figure ("call graph: 531 syntax-resolved (all languages) ·
reconciled: 579 combined-analyses calls, TS only — of the 507 the compiler could
measure, 494 corroborated, 97.4%" — amodx's real numbers: 531 + 48 semantic-only = 579;
the rate on §5.4's dual-measured instance denominator). Never replaces the
pipeline figure; absent outside W-BOTH (R-0/R-1) — and note this additive line is
exactly the W-BOTH dual-render context where BOTH figures carry their human frames
(§5.3.0).

**5.3.3 g2u — union degree + liveness at read time.** The persisted g2 family stays
pipeline (M-3a UNCHANGED; its family contract carries the `pipeline` accounting marker —
§5.3.0's contract-level marking, no row or output change). In W-BOTH, at read time:
(a) modules_list/modules_show "unref?" rollups subtract ledger-witnessed exceptions — a
symbol whose only incoming edges are compiler-witnessed (union calls OR compiler
references) is NOT unreferenced; serving today's answer there is a KNOWN false positive
— labeled in the rollup line ("N fewer flagged: compiler-verified references found").
REDUCTION-ONLY: it can only remove false positives, never add flags; ledger absent →
exactly today's hedged answer. (b) per-symbol fan-in/fan-out displays (explain SYMBOL,
modules_show) may add the union degree as a labeled second figure where it differs from
the pipeline degree. D-R6 (rewritten) ratifies the concrete mechanism.

**5.3.4 g3u — map's sketch pairs from the union call graph.** In W-BOTH, map's dep
sketch computes its CALLS share over the UNION call graph (⊇ pipeline pairs by
construction — no pair is ever lost; new pairs come only from `semantic`/`new_pair`
instances — a `multiplicity` excess adds no pair, §3.3),
labeled with coverage. Reference-KIND edges stay OUT of the sketch — "uses types from"
is a different relation than the sketch's meaning, and admitting it would change what
map answers per-repo (the same certainty-collapse §3.4 bars on callers). Persisted g3
family (D-EC-7 A-i) stays pipeline. Expected magnitude, stated honestly: amodx's 48
union-only call instances are same-file-dominant or import-covered, so the pair DELTA
is likely
small — M-R3a RECORDS the measured delta; if it is 0 at monorepo scale too, g3u is a
no-op that costs nothing (the code path is the same union projection).

**5.3.5 Persistence of the union accounting: DECLINED now — with the named re-open.**
Union values are deterministic given the witness pair (§3.0b determinism evidence), but
persisting them creates a staleness class the in-memory ledger cannot have: a persisted
union row can misdescribe the CURRENT pair after either witness moves, and serving it
would need fingerprint-match refusal machinery — honesty plumbing for a value that
recomputes in ~2 s at 250-file scale [EXECUTED, §3.0b; monorepo cost is S-1's question].
So: read-time from the ledger, W-BOTH only, zero Persistence Completeness burden
(D-R8-A unchanged). RE-OPEN condition (named): IF the monorepo walk cost (S-1) forces
cross-restart caching, the persisted family is FC4-shaped rows keyed
`(snapshot_uid, livegraph_fingerprint, language, partition)` with read-time fingerprint
verification (serve-or-recompute, never serve-stale) — priced then, under the full
checklist.

**5.3.6 Refresh story, both accountings.** Pipeline: index/refresh-time write from the
full stream incl. delta-refresh copy-forward — M-3b (g1) and M-3a (g2/g3) exactly as
ratified, parity-window-validated. Union: the ledger lazily rebuilds per fingerprint
(today's cert lifecycle [OBSERVED: `mod.rs:306-335`]); any witness movement (snapshot
advance, partition swap/load/unload, producer refresh, staleness marking — `mark_stale`
and `begin_refresh` flip the fingerprint's per-partition freshness bit, §4.2) changes
the fingerprint → the
old ledger is dead, and the next read rebuilds ONLY over the then-eligible partition
set (§5.1 rule (e)): a partition that went stale leaves W-BOTH (→ W-ONE `stale`), its
answers serve pipeline-exact until refresh restores `Fresh`, and it contributes no
classification rows meanwhile; a request IN FLIGHT across the movement takes the §4.2
transient fail-soft (EV-A: pipeline at its pinned snapshot, no witness fields — the
next capture warms the new-fingerprint ledger and self-heals); ledger eviction
degrades to the
pinned SQLite snapshot exactly as the W-B invariant ratifies. No cross-accounting
write coupling exists anywhere: the pipeline family never reads the ledger; the ledger
never writes SQLite.

### 5.4 The divergence rate — surfaced per the promotion-funnel pattern

Precedent: `enrich`'s additive `promotion.funnel` object — no new command, additive
block, per-class counts, reader-frame labels [OBSERVED: `dispatch.rs:3571-3581`].
Landing:

- **trust** (the posture surface): an additive `witnesses` block, per covered
  language×partition (§4.2 scoping — never per repo). DISTINCT POPULATIONS, never mixed
  in one figure (the review-1 correction — each field's unit is part of its contract;
  the two field groups below, plus the separately-labeled reference-tier line):
  - **Union-call fields** (population: directed canonical edge INSTANCES on the
    kind-partitioned CALL corpora — multiplicity preserved, keyed by
    `(caller_key, callee_key)` identity, §3.2; the iteration-4 population correction —
    each field may carry an `identities` sub-count where the two differ):
    `{union_calls, pipeline_calls, dual_measured, agreement_pct, both,
    semantic_only_calls: {new_pair, multiplicity},
    syntactic_only: {boundary, file_scope, uncorroborated, multiplicity},
    unmeasured_edges, identity_suspect, identity_collision}` —
    every count an INSTANCE count classified per the §3.3 instance rule (min
    corroborated, excess per side; every instance in exactly one class — R-RAT-5). The
    former standalone `multiplicity_delta` field is RETIRED: its content now lives
    INSIDE the closure as the two `multiplicity` sub-classes (review-4's incoherence —
    the old field sat beside a closure that had no delta term; the ledger still retains
    each delta pair's exact `(p, s)` for doctor's enumeration, §5.1). Where
    `agreement_pct = both / dual_measured` and
    `dual_measured = both + syntactic_only` — divergence classes DUAL-MEASURED
    only (§3.6), `both` kind-aligned AND occurrence-exact (an S strict-`Calls` edge on
    the pair is necessary; min(p,s) is the corroborated count —
    §3.1/R3-C1/§3.3) — so a P=2/S=1 pair contributes 1 `both` + 1
    `syntactic`/`multiplicity` and depresses the rate (never a hidden 100%);
    `unmeasured_edges` EXCLUDED from the rate's denominator and SHOWN
    beside it (unknown ≠ zero; the spike §5.3 magnitude-table pattern). Closure check
    on amodx [EXECUTED, §3.0b + the iteration-5 retained-artifact re-verification,
    §3.3]: `union_calls` **579** = `both` **494** +
    `semantic_only_calls` **48** (48 new_pair + 0 multiplicity) + `syntactic_only`
    **13** (11 boundary + 1 file_scope
    + 1 uncorroborated + 0 multiplicity) + `unmeasured_edges` **24**;
    `pipeline_calls` 531 = 494 + 13 + 24; `agreement_pct` = 494/507 = **97.4%**
    (identities beside: 454 / 37 / 12 / 14; pipeline 480 — identity populations can
    overlap ONLY on delta pairs, §3.3; measured: zero such pairs, so today's identity
    counts are non-overlapping).
    `identity_collision` counts the S-side canonical pairs the R-RAT-4 guard WITHHELD
    from the union (same instance population; amodx: the guard predicate measured ∅
    exactly, iteration 4 — §3.5); withheld
    pairs are OUTSIDE `union_calls` by definition and shown beside it, so the closure
    equation stays exact and a fired guard is visible in the block itself, never
    absorbed.
    (The REFERENCE TIER is a separately-labeled line with its own population — S's
    `References` projection, amodx 12,189 instances / 7,069 identities — the §5.2
    tier; it is NOT a term of the call closure equation. Adoption `matched`/`fallback`
    counts
    are a THIRD population — SCIP definition NODES per partition — and land on doctor
    only, §3.5 guard 3; the trust block never mixes them in.)
  - **Projection fields** (population: symbol×direction LG projections, §3.6):
    a separately named `projections: {total, unanswerable}` — never summed or ratioed
    against edge fields.

  Reader-frame example (amodx's real numbers [EXECUTED, §3.0b], every figure's
  population labeled): "TypeScript (8 packages): 531 syntax-resolved calls; the compiler
  could measure 507 of them — 494 corroborated (97.4%; occurrence counts agreed on
  every corroborated call), 13 syntax-only (11 across
  compiler-run boundaries, 1 module-initialization call outside the compiler's call
  model, 1 the compiler did not confirm), and 24 more it could not
  measure here (shown, excluded from the rate); beyond the syntax graph: 48
  compiler-resolved calls, and 12,189 compiler-verified references (reads / writes /
  type references); separately, 1,071 of 4,860 symbol-direction lookups had no
  compiler-side
  answer."
- **doctor** (the ops surface): the operational half — ledger present? at which
  fingerprint? when ABSENT, the last capture outcome + its build-failure reason (the
  §4.2 transient-2 fact — a storage error during the walk is OUR operational state,
  so it lands here, never on a repo-facts surface; trust's witnesses block renders
  absent/unknown meanwhile, D-R8-A); unmeasured counts (both populations, each labeled); identity_suspect
  flag + the per-partition adoption `matched`/`fallback` counts (the ingest's own node
  report [OBSERVED: lib.rs:1167-1170]; adoption coverage is a per-producer,
  per-partition fact, §3.5
  guard 3 — the Rust ~94–96%-fallback bound shows it can dominate, and even TS shows a
  ~30%-fallback generated-code partition at amodx [EXECUTED, §3.5]); the R-RAT-4
  collision guard's colliding KEYS per partition (a KEY population, labeled as such —
  count + the keys themselves; the reader-frame line when it fires: "N identities
  collide between the syntax index and the compiler index — shown separately, never
  merged"; amodx: the guard predicate `fallback_keys(S) ∩ keys(P)` measured **∅**
  exactly — 280 fallback × 2,530 pipeline keys, iteration 4, §3.5);
  the occurrence-delta enumeration when either `multiplicity` sub-class is nonzero
  (each delta pair with its exact `(p, s)` — the ledger retains them, §5.1; measured
  today: none, §3.3); per-language×partition REGIME + the W-ONE reason
  (`stale` / `not_resident` / `producer_unavailable`, §4.2 — the reason-specific
  next-action lines render here and on trust's degradation line; the stale∧producer-
  absent compound names its blocker) — extending doctor's existing toolchain/lifecycle
  truth (DAEMON-VISIBILITY precedent), which already carries producer presence.
- **Rendering seam:** the shared-projection discipline — ONE computation feeds every
  surface (the ratified RELIABILITY-REFRAME rule; `CallReliabilityView` is the
  precedent and natural neighbor [OBSERVED: `agent/src/reliability.rs:1-25`]) — no
  per-surface re-derivation.
- **Zero-SCIP repos:** the block is ABSENT entirely (R-0; data-driven absence).

### 5.5 Layer-2 landing — pipeline-unresolved × SCIP-resolved, denominator untouched

For an unresolved SITE (an `unresolved_edges` row: caller, raw target expression,
location — the ratified floor) where the ledger holds a `semantic` edge from the SAME
caller key whose callee NAME corresponds to the site's target expression head: land a
Layer-2 annotation — "this call likely resolves to `X` (the compiler resolved a
same-named call in this function; syntax resolution could not confirm)" — basis +
provenance named (witness S + name correspondence). The join is structurally precise,
not a fuzzy correlation: S's `Calls` edges exist only at P's OWN detected call sites
(§3.0a shared detection), so an S-only call from caller F targeting name N is a site P
saw in F — measured live: amodx's `Toolbar → cn(...)` class, where P detected the JSX
call sites but did not resolve them and the compiler did [§3.0b]. The SAME machinery,
run the other way, is the **contested-resolution signal**: where P RESOLVED a site to
project target A and the ledger holds a `semantic` edge binding a same-named site in
the same caller to project target B, the pair surfaces on the attribution surface as a
labeled contradiction hint ("syntax and compiler resolutions disagree here"). Scope,
stated honestly: this fires only when S's competing binding is a PROJECT symbol — the
measured amodx misresolutions all bound to EXTERNAL APIs (`next/cache`, DOM
`localStorage`), which the strict ingest DROPS at callee resolution [OBSERVED:
lib.rs:720-726 — unresolvable callee ⇒ no edge], so THEY surface only via their
§3.1 divergence class, not as contested pairs. Retaining S's external-binding facts
(which would catch them) is a named possible upgrade, evidence-gated on monorepo
contested-rate — NOT built here (it extends the ingest contract). Layer-2, basis
stated, never a silent rewrite of either witness's fact.

- **Guard:** NAME correspondence REQUIRED; caller-key-only or position-only joins
  FORBIDDEN (the CJOIN-PROVE-2 name-correspondence rule: range-only joining silently
  misattaches). Ambiguous (≥2 candidates) → NOT landed; unknown stays unknown.
- **Non-negotiables (stop conditions §8):** the `unresolved_edges` row is untouched; no
  counter anywhere decrements; the trust ratio is byte-invariant (the §5.3.1 named test
  covers this surface too).
- **Rendering:** the attribution surface ("Unresolved references — where they go",
  ATTRIBUTION-1) + explain SYMBOL focus — as clearly-basised Layer-2 hints ("this is
  what we inferred; here is the basis" — VISION certainty table).
- **Persistence:** deliberately NOT persisted in this design — read-time from the
  ledger, W-BOTH only, residency-conditional like every Layer-2/3 hint. A persisted
  family is priced in D-R8 and deferred until the monorepo proves hit-rate (acid test;
  no Persistence Completeness burden for an unproven family).

## 6. Validation design + milestones

### 6.1 Milestones — each independently shippable + smoke-gateable

| M | Content | Gate |
|---|---|---|
| **M-R1** | The witness ledger: generalize the cert compare into the full-walk ledger (classes §3.1/§3.3/§3.6 — divergence classes DUAL-MEASURED only, KIND-ALIGNED per §5.1 rule (c) [`both` requires an S strict-`Calls` edge on the pair], INSTANCE-LEVEL per §5.1 rule (d) [min corroborated, excess per side into the `multiplicity` sub-classes — R-RAT-5], SCOPED to the W-BOTH-eligible partition set per §5.1 rule (e) [the §4.2 eligibility: covered ∧ resident ∧ `Fresh`; the ledger itself keyed by the fingerprint — R-RAT-6 + the iteration-6 split], rollups keyed per language×partition, union call projection kind-filtered; `identity_suspect` §3.5 guard 3; the R-RAT-4 COLLISION GUARD §3.5 guard 2 — merge requires adoption-compatible identity, `fallback_keys(S) ∩ keys(P)` computed per partition over a key→sources SET [duplicate adoption-compatible keys tolerated, a fallback-mixed key treated as colliding — the §3.5 measured observation], members surface as `identity_collision`); stored GREEN/RED verdict DERIVED from it (behavior byte-unchanged); fix the §3.7-2 doc-comments + the §3.7-5 `diff.rs` note; RECORD the per-kind classification of the fixture's 7 SCIP-only edges (expected all-References per the measured ctor evidence, §3.4) AND the fixture's kind-aligned shared split | ledger reproduces the spike's 7/0/2/9 canonical classification on the committed fixture [spike §5.3] AND the amodx retained-artifact classification, kind-aligned per §3.0b/§5.4's closure check: `both` 494 (454 identities), `syntactic` 13 = 11 boundary + 1 file_scope + 1 uncorroborated + 0 multiplicity, `unmeasured` 24, `semantic_only_calls` 48 = 48 new_pair + 0 multiplicity, union calls 579, `agreement_pct` 494/507 = 97.4% (the multiplicity zeros re-verified from retained artifacts: `runs/amodx/iter5-multiplicity-check.py` — excess 0 both directions, split pairs 0); S kind totals 542 `Calls` / 12,189 `References`; suspects 0; INSTANCE-fixture tests (the measured-empty classes are fixture-proven, §3.3): a hand-built `PartitionIr` pair with P=2/S-`Calls`=1 → 1 `both` + 1 `syntactic`/`multiplicity`, `dual_measured` 2, `agreement_pct` 50%, closure exact; P=1/S-`Calls`=2 → 1 `both` + 1 `semantic`/`multiplicity`, union count 2, closure exact; REGIME tests (R-RAT-6 + the iteration-6 matrix): a resident partition marked stale (`mark_stale`) → W-ONE(`stale`), ledger holds NO classification rows for it, its serving byte-identical pipeline; every REPRESENTABLE cell of the §4.2 state matrix (covered × resident × freshness × producer × pin state) lands in EXACTLY one classification — regimes exclusive AND exhaustive, the two transient states never classify as regimes and never as W-ONE reasons; the three W-ONE reasons map deterministically (stale / not_resident / producer_unavailable — incl. the warm-cache stale∧producer-absent compound, one reason + named blocker); Fresh-resident-producer-absent → W-BOTH-eligible (the producer-out-of-predicate cell); CAPTURE-CONTRACT byte-parity through M-R1 (named test: a divergent fixture still captures NO fingerprint at M-R1 — the GREEN gate is preserved until M-R2 flips it, §5.1); the guard predicate's first LIVE computation must reproduce the iteration-4 exact baseline `identity_collision = ∅` with the 280-key fallback population present [§3.0b/§3.5; `runs/ANALYSIS.md` iteration-4 supersession; `runs/amodx/iter4-recompute.py`]; COLLISION-GUARD test: a hand-constructed `PartitionIr` holding a `ScipSynthesizedFallback` node whose key byte-equals a pipeline key → never `both`, `identity_collision` counted + retained + debug-artifact-visible at M-R1 (**amendment 2026-07-18, operator-recorded from the M-R1 escalate:** trust-block + doctor RENDERING lands with M-R3a's read surfaces per this doc's own §5.4 ownership — the original wording conflicted with M-R1's zero-served-byte definition of done; the guard's substance is unchanged), P rows byte-unchanged (the ingest cannot currently MINT this fixture — that impossibility IS the contingent disjointness §3.5 measures, so the test constructs IR directly at the guard's own layer, the existing livegraph hand-built-`PartitionIr` test pattern); zap-engine mixed-repo scoping (rs/py projections land `unmeasured`/uncovered, never divergence; coverage split reproduces 1,585 = 29 + 1,556); GREEN/RED byte-unchanged on faithful-mirror (GREEN) + drop-calls (RED) + degenerate paths; R-0 byte-parity dogfood on nginx + spring-petclinic; full cargo gates |
| **M-R2** | Union serving for callers/callees in W-BOTH: the CAPTURE-CONTRACT flip (ledger-validity-gated, verdict-independent — §4.2 activation/§5.1; the named movement `fallback_reason` value; the flip RIDES THE SAME FLAG as union serving — the default path's capture stays GREEN-gated byte-exact until the recorded default flip), LG kind filter (§3.4-3), union rows + `witness` (dual-measured only; `mixed` + `occurrences: {confirmed, total}` on P-excess delta pairs; S-excess instances MINT `semantic` rows — §3.3, iteration 6) + `witness_counts` incl. `unmeasured` (1:1 with rows, §5.2), MAX multiplicity **= row count** (the preserved `count == rows.len()` contract, §5.2), null-not-zero locations (§3.7-4; the §3.3a definition-location semantics), presentation accepts unknown; replace the §3.7-3 row builder; ADD a pipeline-only test fixture (a P row absent from S — the committed fixture cannot produce it [spike §5.3: pipeline_only = 0]; the amodx artifacts prove the shape live and inform the fixture: both a boundary case and an uncorroborated case) | union ⊇ P verbatim (named test); R-0 + R-1 byte-parity (nginx/petclinic + zap-engine mixed); count/MAX rule tests + the ROW/COUNT INVARIANT test (`count == rows.len()` across every fixture class — review-5); DIVERGENT-CAPTURE test (a divergent fixture CAPTURES a fingerprint at M-R2 and serves union in W-BOTH — the §4.2 redefinition's named test; its M-R1 twin proves the opposite); EPOCH-MOVED test (fingerprint moved between capture and data read → pipeline bytes at the pinned snapshot, NO witness fields, the named movement `fallback_reason` — transient 1); CAPTURE-FAILED test (ledger build error → pipeline serve, doctor reports absent + reason — transient 2); DELTA-PAIR row tests (M-R1's P=2/S=1 fixture driven through serving: both rows `witness: "mixed"` + `{confirmed: 1, total: 2}`, NEVER `both`; the S-excess P=1/S=2 fixture: `count` 2, TWO served rows — one P row `both` (every P occurrence corroborated) + one S-minted row `semantic`/`multiplicity`, closure and row multiset 1:1); STALE-serving test (M-R1's marked-stale fixture through callers/callees: pipeline bytes, no union fields — W-ONE, §4.2); collision-withheld pairs NEVER serve — M-R1's guard fixture driven through union serving (named test, §3.5 guard 2); W-B epoch tests (pin + eviction unchanged); **ships FLAG-GATED, non-default, until S-1..S-3 (§6.2)** — the default flip is its own recorded step |
| **M-R3a** | Divergence posture + the union accounting's read surfaces: trust `witnesses` block + doctor operational block + orient/stats g1u lines + g2u liveness/degree overlays + g3u sketch pairs (§5.3.2-4, §5.4), all through ONE shared projection | §5.3.1 invariance + accounting-label tests; zero-SCIP absence (R-0) + mixed-repo scoping (R-1) tests; W-ONE REASON-RENDERING tests (three reasons → three distinct posture lines + next actions; stale ≠ "available but not loaded" — review-4's defect pinned; the stale∧producer-absent compound renders its blocker, §4.2); doctor's ledger-ABSENT rendering (last capture outcome + build-failure reason — §4.2 transient 2 / §5.4; trust renders unknown, never a stale number); deterministic ordering; RECORDS the measured g3u pair delta (§5.3.4); smoke |
| **M-R3b** | The reference tier on callers/callees/explain — "reads / writes / type references" — budget-disciplined (§5.2) | tier renders only in W-BOTH; truncation named test (amodx max fan-in 456 is the fixture-scale bound); R-0/R-1; S-4 informs budgets |
| **M-R4** | Layer-2 landing on attribution/explain with the name guard, incl. the contested-resolution signal (§5.5) | denominator-invariance test; ambiguity-refusal test; label wording audited against the labels rule |

Ordering: M-R1 ≺ everything. M-R2 ∥ M-R3a after M-R1. M-3a/M-3b (EC-1) proceed
independently under §5.3 — no new coupling (the pipeline accounting is theirs; the union
accounting never touches their write path). DAEMON-CONCURRENCY-1 (queue 3): no ordering
conflict; the ledger build cost class is noted for its spec (§5.1). No milestone here
introduces a persisted family — Persistence Completeness is N/A by design in this arc
(the deliberate D-R8 choice; §5.3.5 names the re-open). Named possible follow-up, NOT
scheduled: extending `is_call_at` to new-expressions (moves instantiations into the
union call graph — an INGEST-CORE derivation amendment with its own ratification, §3.4).

### 6.2 The scale-validation gap — narrowed by the DATA-UPGRADE runs, honestly restated

The iteration-0 gap ("no real two-witness data exists locally") is CLOSED: the design
above is built FROM a real multi-partition measurement (amodx, 8 partitions, corpus
2,430) and a real mixed-language measurement (zap-engine) [§3.0b; artifacts retained].
What remains is exactly what the operator note predicted: **the deployment-target
monorepo confirms at deployment scale (160k files)** — the spike's shipped
`RMAP_CALLGRAPH_DIFF` instrument, the same §5.5 procedure these runs used. Per gate,
pre-signal vs what the monorepo must still confirm:

- **S-1 (before the M-R2 DEFAULT flip) — walk cost.** Pre-signal: whole-run 1.77 s at
  250-file/8-partition scale, walk a fraction of it [EXECUTED]. Monorepo must confirm
  per-fingerprint affordability at 160k files; else partition-scoped/incremental ledger
  build (the §5.1 rule-(b) seam) becomes a named M-R2 prerequisite.
- **S-2 (before the M-R2 DEFAULT flip) — the pipeline-only class.** Pre-signal: the
  class EXISTS (13 dual-measured instances on amodx, 29 on zap-engine) with THREE
  observed causes —
  boundary (mechanically classifiable), uncorroborated-incl.-misresolution, and the
  module-init model asymmetry (`file_scope`, mechanically classifiable) — now in
  the §3.1 labels. Monorepo must confirm: magnitude at scale; whether
  compile-skip/producer-skip causes (still UNOBSERVED — both corpora compile clean)
  appear — if they do, the `uncorroborated` label wording is re-audited before the
  flip; and the occurrence-delta (`multiplicity`) incidence — measured **0 in both
  directions** at amodx (§3.3): a materially nonzero rate at scale escalates the §3.3
  site-attribution IR extension from named-upgrade to scheduled (until then delta rows
  serve the honest-but-coarser `mixed` summary).
- **S-3 (before the M-R2 DEFAULT flip) — identity soundness: adoption + collision.**
  Pre-signal:
  `identity_suspect` = 0 at amodx scale [EXECUTED] — an absence-of-symptom measurement
  for the wrong-match and adoption-miss classes (§3.5) — and the guard predicate
  `fallback_keys(S) ∩ keys(P)` measured **∅ exactly** over the complete key sets (280
  fallback × 2,530 pipeline keys; iteration-3's heuristic-sweep deduction withdrawn —
  §3.5's two-grade record)
  [EXECUTED, iteration 4]; the 100%-key-equality join is
  deterministic replay, cited for ingest determinism only (R-RAT-3). Monorepo must
  confirm suspects ≈ 0 AND collisions = ∅ AND the adoption `fallback` share pattern —
  measured at amodx as a small overall share (10.9% of node rows) that concentrates in
  GENERATED code (a ~30% partition) while call-graph incidence stays marginal (1/542) —
  remains labeled and marginal on the call graph
  at deployment scale; a nonzero suspect rate → the alias repair slice (§3.5 guard 4)
  precedes union serving; a FIRED collision guard — there or ever after — → the
  reserved-namespace slice (§3.5 guard 2's recorded option-B escalation) precedes the
  default flip.
- **S-4 (before M-R3b) — reference-tier volume.** Pre-signal: max fan-in 456 / top-8 ≥
  268 at 250-file scale [EXECUTED] — budgets are mandatory, not precautionary. Monorepo
  sizes the actual budget ladder.

Until S-1..S-3: M-R1 and M-R3a ship (no serving change; posture is additive); M-R2
exists behind its flag. This mirrors the ROADMAP's own NOW gate ("priorities below are
hypotheses until that run speaks").

## 7. DECISION_REQUIRED list — with the six operator RATIFICATIONS recorded first

Convention per CLAUDE.md: exhaustive matrices; trade-offs against the VISION's three
commitments (deterministic extraction / honesty about certainty /
current-state-in-milliseconds) + the change-cost doctrine.

### 7.0 RATIFIED (operator, 2026-07-17, review-0 + review-1 + review-2 arbitrations; D-R4 absorbed into R-RAT-2 per review-3; review-4's closing two ratified as R-RAT-5 + R-RAT-6) — recorded, not open

- **R-RAT-1 — WITNESS-INDEPENDENCE, option A.** The contradiction: EC-1 §8b's ratified
  wording "agreement = highest-confidence (two independent witnesses)"
  [engine-consolidation-1.md:1679-1683] vs the OBSERVED mechanism — SCIP `Calls`
  classification reuses the pipeline's AST call sites [scip-ingest lib.rs:511-546,
  :690-736], so independence holds for RESOLUTION, not detection. RESOLUTION: the
  vocabulary and ALL reader-facing claims amend to corroboration by distinct RESOLUTION
  paths sharing syntax confirmation (§3.0a states the property split exactly:
  independently derived — callee RESOLUTION, the one load-bearing independence; shared
  — call-site detection, AST anchoring, and, per R-RAT-3's deeper correction, identity;
  S-only BY DESIGN — the reference KINDS, for which no corroboration is possible, so
  they are never claimed corroborated *[iteration 3, review-2's secondary correction:
  this record previously listed reference kinds beside resolution as "independent" —
  §3.0a's table was already right; the record now matches it]*; §3.1 carries the
  reader-frame labels). Option C (retain "independent") REJECTED — evidence-backed
  false certainty. **EC-1 §8 receives a one-line pointer amendment at commit time BY
  THE OPERATOR** (this spec is the amendment's substance; silent drift forbidden, and
  the EC-1 edit is outside this slice's file scope).
- **R-RAT-2 — FC2A-UNION, option A: DUAL ACCOUNTING.** Discovery aggregates (g1u for
  orientation surfaces, g2u fan-in/out + liveness, g3u sketch pairs) compute over the
  coverage-labelled UNION; the trust RATIO retains the separately NAMED pipeline-pure
  accounting (numerator and denominator on one graph — the ratified D-EC-7 principle).
  §5.3 is the complete dual-accounting persistence/refresh/serving story with
  cannot-be-confused naming (`accounting: pipeline|union`; `g*` vs `g*u`). Option B
  (interim rule terminal) REJECTED — abandons the ratified reconciliation intent. This
  re-scopes D-R6 (now the concrete union-read mechanism).
  **Recorded consequences (absorbs the former D-R4 per review-3 change 4 — the
  ratified decision is NOT re-opened):** (i) the trust RATIO's accounting — numerator
  AND denominator — is pipeline-witness PERMANENTLY (§5.3.1); a union numerator over
  the pipeline denominator would inflate the ratio by construction (semantic-only
  calls enter with no denominator counterpart — the false trust claim EC-1 §8b's own
  text bars), and a union-based REPLACEMENT ratio would change the served number's
  meaning per coverage regime and kill cross-repo comparability — both rejected with
  the ratification. (ii) EC-1 §8c's "EXPLICITLY TEMPORARY" therefore resolves as:
  temporary as the ONLY accounting (the union accounting now exists beside it),
  permanent as the RATIO's accounting — a letter-amendment of ratified text, surfaced
  here rather than silently reinterpreted (stop-condition honored). (iii) The §5.3.1
  byte-invariance test pins it: no silent inflation is POSSIBLE. (iv) The EC-1 §8c
  one-line pointer amendment is the OPERATOR'S commit-time action, same vehicle as
  R-RAT-1's §8 amendment (outside this slice's file scope; silent drift forbidden).
- **R-RAT-3 — IDENTITY-ADOPTION HONESTY (iteration 2; extends R-RAT-1 one layer
  deeper).** The contradiction: iterations 0/1 presented canonical identity as
  "independent derivations that must agree", citing the spike's zero identity mismatch
  and the amodx 100%-key-equality join as corroboration. The OBSERVED mechanism:
  matched SCIP definitions ADOPT the pipeline's `ast.stable_key`
  (`IdentitySource::AstAdopted` [scip-ingest lib.rs:396-428]); unmatched ones mint a
  labeled, shape-distinct *["shape-distinct" as ratified then — AMENDED by R-RAT-4
  below: same key grammar]* `ScipSynthesizedFallback` key [lib.rs:429-445]; all S edge
  endpoints key through `symbol_to_key` [lib.rs:1174-1182] — so agreement is BY
  CONSTRUCTION, and the join harness re-runs the same `ingest_partition`, making its
  100% join DETERMINISTIC REPLAY. RESOLUTION (ratified): identity is ADOPTED, not
  independently derived; the witness model's final form — the PIPELINE is the PRIMARY
  witness (detection + identity, all languages); SCIP contributes INDEPENDENT
  RESOLUTION plus additional semantic reference KINDS layered on adopted identity;
  "independent derivations" is dropped from the confidence vocabulary entirely;
  "corroborated" = the compiler's semantic resolution confirmed the pipeline-detected,
  pipeline-identified edge. The spike's zero-identity-mismatch citation is corrected
  wherever it appears (header Inputs, §3.0, §3.5, §6.2 S-3). Convergence noted in
  §3.0a's framing: the corrected model matches the operator's original skeleton +
  semantic-enrichment-overlay intuition.
- **R-RAT-4 — FALLBACK-KEY-INVARIANT, option A: the EXPLICIT COLLISION GUARD
  (iteration 3; extends R-RAT-3 to the fallback branch of adoption).** The
  contradiction: iteration 2 called the fallback key "structurally distinct" and
  claimed the `:SYMBOL:` infix "cannot byte-collide with a pipeline key". The OBSERVED
  mechanism (review-2's proof, re-verified first-hand iteration 3): both key families
  share ONE grammar — pipeline `make_stable_key` emits
  `{repo_uid}:{path}#{name}:SYMBOL:{NodeSubtype as SCREAMING_SNAKE}` [extractor.rs:
  345-364; indexer types.rs:133], the fallback mint emits
  `{repo_uid}:{key_path}#{name}:SYMBOL:{scip suffix as Debug TitleCase}` [scip-ingest
  lib.rs:432; :135-138], and `CanonicalKey::from_existing` adds no namespace or guard
  [repo-graph-ir lib.rs:33-35] — so non-collision is CONTINGENT on two spelling
  conventions in two mutually-unaware crates (measured at amodx: the vocabularies
  already intersect up to case, `Method`/`METHOD`; the guard predicate itself measured
  ∅ exactly at iteration 4 — §3.5's two-grade record; iteration-3's heuristic-sweep
  deduction withdrawn per review-3).
  RESOLUTION (ratified): the "structurally distinct" claim is amended away; the
  enforceable invariant is a PER-KEY COLLISION GUARD in the union/ledger —
  `ScipSynthesizedFallback` identities NEVER merge with pipeline keys (merge is
  identity-source-conditional, §3.2/§3.5 guard 2); a fallback key byte-equal to a
  pipeline key surfaces EXPLICITLY as the named divergence fact `identity_collision`
  (trust witnesses block; colliding keys on doctor — §5.4) and can never be classified
  `both`/corroborated; the affected S pair is withheld from union serving under that
  key. Canonical key shape UNCHANGED — the smallest enforceable trust claim. Option B
  (a reserved fallback-key namespace) is RECORDED as the designed escalation path IFF
  the guard ever fires in practice (an identity change: versioning contract + migration
  path first; its own slice — §3.5 guard 2, §6.2 S-3), not the default. Option C
  (prove + freeze disjoint segment vocabularies) REJECTED as brittle — §3.5's measured
  near-miss is the demonstration. Tests: §6.1 M-R1 (hand-constructed colliding
  `PartitionIr` → never-`both` + surfaced + P byte-unchanged) and M-R2
  (collision-withheld pairs never serve).
- **R-RAT-5 — INSTANCE-LEVEL PROVENANCE (iteration 5; review-4 change 1).** The
  incoherence: iterations 0–4's §3.3 classified a multiplicity-delta pair `both`
  wholesale ("the served count uses MAX; the delta rolls into the rate") while §5.4's
  closure and `agreement_pct = both / (both + syntactic_only)` carried NO delta term —
  under pair-level classes a P=2/S=1 pair either hides the disagreement (both instances
  `both`, agreement 100%) or leaves the excess instance unclassifiable (`syntactic`
  required no S call on the pair). RESOLUTION (ratified): provenance is defined at
  INSTANCE level — per dual-measured pair, min(p, s) instances classify `both` and each
  side's excess lands in its own class under the mechanical sub-class `multiplicity`
  (§3.1/§3.3); every instance lands in exactly one class, so the closure and the rate
  are instance-exact; pair-level labels may SUMMARIZE but never claim unconfirmed
  occurrences — the served-row rule: P-excess delta-pair rows carry `mixed` +
  `occurrences: {confirmed, total}`, never `both`; an S-excess pair's P rows stay
  `both` (every P occurrence corroborated), its excess *[as ratified at iteration 5:
  "rowless and visible as count > rows" — AMENDED iteration 6, review-5: the excess
  MINTS per-instance `semantic` rows via the same S-only mechanism as `new_pair`,
  preserving the shipped serving invariant `count == rows.len()` [OBSERVED:
  livegraph_feed.rs:542,646; one-row-per-instance builders, mod.rs:118-162]; the
  ratification's substance — instance classes, never-claim-unconfirmed — is unchanged
  and STRENGTHENED (every classified instance is one served row); the rowless
  rationale's "no location to serve" premise was itself false — P rows carry the
  opposite-endpoint DEFINITION location, not call sites (§3.3a, queries.rs:672-685/
  :736-749)]*
  (§3.3/§5.2; `IrEdge` carries no
  per-occurrence site [repo-graph-ir lib.rs:364-378], so count-level attribution is the
  honest maximum; site retention on the S side + serving the persisted-but-unread
  P-side `edges` site columns [types.rs:597-611] is the named evidence-gated upgrade,
  S-2). NOT a
  measurement change: `edge_magnitude` already accumulates min/excess per identity
  [diff.rs:605-629] and the confirmed figures were computed that way — the model now
  matches the measurement; re-verified from retained artifacts (excess on corroborated
  pairs 0 in both directions, split pairs 0 — `runs/amodx/iter5-multiplicity-check.py`).
  Worked through §3.1, §3.2, §3.3, §5.1 (rule d), §5.2, §5.4 (sub-class schema; the
  standalone `multiplicity_delta` field retired), M-R1/M-R2 fixtures, S-2.
- **R-RAT-6 — MUTUALLY-EXCLUSIVE COVERAGE REGIMES (iteration 5; review-4 change 2).**
  The incoherence: iterations 0–4's §4.2 conditions OVERLAPPED — a stale-but-resident
  partition satisfied both W-BOTH ("partition resident") and W-ONE-AVAILABLE ("not
  resident / producer absent / stale"), and the single posture message ("semantic
  corroboration available but not loaded") was FALSE for stale data and imprecise for
  producer-absent. RESOLUTION (ratified): the three regimes are **W-BOTH / W-ONE /
  W-NONE** (operator-ratified names; W-ONE-AVAILABLE→W-ONE, W-ONE-UNCOVERED→W-NONE),
  made mutually exclusive by the ACTUAL eligibility predicate the epoch machinery
  computes — W-BOTH ⟺ covered ∧ resident ∧ status `Fresh` ∧ the serve-time
  `import_cert_fingerprint` (binding per-partition epoch + freshness bit +
  `source_inputs_hash` + `producer_fingerprint` to the pinned `snapshot_uid`
  [livegraph_feed.rs:1727-1752; module_cycle_cert.rs:62-78]) equals the pinned
  fingerprint, matched under one read guard (the build-then-peek discipline,
  callgraph_cert/mod.rs:375-387) *[COMPLETED iteration 6, review-5: the fingerprint
  conjunct is the request-scoped ACTIVATION half, split in §4.2 from the
  partition-state eligibility (covered ∧ resident ∧ `Fresh`) that decides the REGIME —
  review-5 proved the conflated form left "Fresh ∧ pin-moved/no-pin" unclassifiable
  and, deeper, that the SHIPPED capture returns a fingerprint only for a GREEN
  byte-equality cert [livegraph_feed.rs:63-72; mod.rs:384], which semantic enrichment
  guarantees never fires — the capture contract is redefined ledger-validity-gated at
  M-R2 (§4.2/§5.1), pin movement and capture failure are ORTHOGONAL TRANSIENT
  fail-soft states (never regimes, never W-ONE reasons), and the full
  covered × resident × freshness × producer × pin-state space is exhaustively
  classified in the §4.2 matrix]*; W-ONE = covered ∧ ¬eligible with EXACTLY one reason
  from the deterministic ladder `stale` / `not_resident` / `producer_unavailable`
  (§4.2 — residency first, then producer presence; the measured warm-cache
  stale∧producer-absent compound [livegraph_refresh.rs:562-566] is one reason + a named
  blocker, never a fourth state); W-NONE = no shipped producer. The freshness exclusion
  is the witness-honesty invariant: a stale S beside a current P would mint FALSE
  divergence describing refresh lag, not the reader's code — today absorbed by
  RED→fallback, which the union retires, so the condition becomes explicit. Worked
  through §4.2/§4.3/§4.4, §5.1 (rule e), §5.2, §5.3.0/§5.3.6, §5.4 doctor, M-R1/M-R2/
  M-R3a gates, D-R1.

DECISION_REQUIRED:
- ID: D-R1
  QUESTION: Ratify the witness vocabulary + rendering contract — the three witness
    classes at INSTANCE granularity with reader labels: `both`'s kind-aligned,
    occurrence-exact definition (an S strict-`Calls` edge on the pair necessary;
    min(p, s) instances corroborated — R3-C1 + R-RAT-5), the `syntactic`
    boundary/file_scope/uncorroborated/multiplicity sub-classes + the `semantic`
    new_pair/multiplicity split (§3.1/§3.3, wording per R-RAT-1 + R-RAT-3), the
    delta-pair row rules (P-excess: the `mixed` summary, never claiming unconfirmed
    occurrences; S-excess: per-instance `semantic` rows via the new_pair mechanism —
    the served `count == rows.len()` contract preserved, §3.3/§5.2, iteration 6 — with
    the §3.3a location fact: served `line`/`column` is the endpoint's DEFINITION
    location, never a call site), the
    dual-measured rule (§3.6), the THREE mutually-exclusive coverage regimes
    W-BOTH / W-ONE / W-NONE via the §4.2 ELIGIBILITY predicate (covered ∧ resident ∧
    `Fresh`) with W-ONE's reason-specific rendering (R-RAT-6; review-0's count
    correction stands — per-symbol answerability is an axis inside W-BOTH, not a fourth
    regime), the §4.2 ACTIVATION contract beside them (iteration 6: capture becomes
    ledger-validity-gated at M-R2 — a divergent union captures a fingerprint; pin
    movement and capture failure are orthogonal TRANSIENT fail-soft states with the
    exhaustive covered × resident × freshness × producer × pin-state matrix — never
    regimes, never W-ONE reasons), the uncovered ≠ pipeline-only distinction (§4.3),
    and R-0 + R-1 (§4.4,
    incl. doctor's ops-surface carve-out)?
  OPTIONS:
  - A (RECOMMENDED) — as written. Consequence: provenance is honest at every
    granularity (per-instance class only where a second witness measured; occurrence
    counts never overclaimed — a row claims `both` only when fully corroborated;
    coverage at partition level, reason-specific when degraded); zero-SCIP repos
    byte-unchanged (the dominant case pays nothing);
    determinism untouched; the labels rule holds (every label describes the reader's
    code or the analyses run on it, never our pipeline state).
  - B — per-edge witness labels EVERYWHERE (incl. single-witness repos). Consequence:
    100%-uniform labels on nginx/petclinic (zero information, pure noise); implies the
    compiler declined where it never looked — a false Layer-1 claim; violates R-0.
  - C — no per-edge classes anywhere; only aggregate agreement rates. Consequence: the
    highest-confidence fact this track creates (THIS edge's resolution is corroborated)
    is computed then hidden — repeats the exact §1-of-the-spike failure (pay for the
    comparison, discard the detail) at the serving layer.
  RECOMMENDED: A.
  BLOCKING_REASON: every §5 surface and every milestone renders this vocabulary; it is
    the certainty-model contract of the track (false-trust-claim class if wrong).

- ID: D-R2
  QUESTION: The reference-KIND asymmetry — new union call-graph members, or a SCIP-only
    enrichment tier (§2's either/or)?
  OPTIONS:
  - A (RECOMMENDED) — KIND-PARTITIONED (§3.4): S's `Calls`-kind edges join
    callers/callees as witness-labeled members; S's `References`-kind edges form the
    separate compiler-verified-references tier; the LG kind-blind traversal is fixed as
    a prerequisite. Consequence: `callers` keeps one meaning across languages
    (honesty); genuine compiler-resolved calls are not withheld (completeness); the
    reference kinds ship as what they are (Layer-1, coverage-labeled); inherits
    INGEST-CORE-1's ratified strict derivation — no new classification logic. MEASURED
    stakes [§3.0b]: gains exactly 48 genuine call instances (37 identities) on amodx
    (8.9% of S's 542-instance
    strict call graph; e.g. JSX-expression calls to `cn`) while keeping the 12,189
    reference-kind instances out of the count.
  - B — everything into callers/callees (today's kind-blind LG shape, served).
    Consequence: a PROPERTY shows field-reads as "callers" (the fixture's
    `Circle.radius` +4); MEASURED at scale: amodx `callers` inflates ~20× (10,795
    all-kind vs 531 call-kind instances); counts shift meaning
    per-repo/per-language; the certainty class of the
    primary surface collapses; the §3.7-3 "CALLS" hardcode becomes an active mislabel.
    Rejected on the labels rule + change doctrine (a meaning change on the primary
    surface with no honesty gain).
  - C — enrichment tier ONLY (S never adds members to callers/callees). Consequence:
    once we KNOW (compiler-resolved, at a call site P itself detected — §3.0a) that
    `Toolbar` calls `cn`, `callers cn` would knowingly under-answer — withholding a
    Layer-1 fact from the surface whose question it answers; safe-looking but
    dishonest-by-omission; measured cost: 48 genuine call instances withheld on amodx
    alone.
  RECOMMENDED: A.
  BLOCKING_REASON: decides what `callers` MEANS under reconciliation — the primary
    discovery surface's answer contract; M-R2 is unbuildable without it.

- ID: D-R3
  QUESTION: The cert's fate — retire the byte-equality GREEN/RED as the callers/callees
    serving license and generalize the comparison into the witness ledger (§5.1)?
  OPTIONS:
  - A (RECOMMENDED) — ledger replaces the verdict; GREEN/RED derived during transition
    (M-R1, byte-compatible), retired for callers/callees at M-R2; other certs
    untouched; no serving GATE remains for the union because no-loss holds by
    construction (union ⊇ P verbatim), with `identity_suspect` as a POSTURE flag, not a
    gate — and the R-RAT-4 collision guard as a PER-KEY merge bar (it withholds
    specific colliding pairs, never gates union serving wholesale; §3.5 guard 2).
    Consequence: the comparison's cost buys the data the track needs;
    honesty machinery (epoch pin, fallback ladder) unchanged; the full-walk cost delta
    is real and S-1-gated.
  - B — keep GREEN/RED beside the ledger as a union-serving gate (serve union only on
    some agreement threshold). Consequence: reintroduces adjudication through the back
    door — a threshold decides which witness "wins" enough to serve; the ratified
    direction (reconcile, not adjudicate) is contradicted; any threshold is arbitrary
    (what agreement % makes a union "safe" when the union never loses facts?).
  - C — keep the cert exactly as-is and bolt the ledger beside it (two full
    comparisons). Consequence: double walk cost for no additional truth; the RED
    verdict keeps suppressing LG serving on exactly the repos where S is richest —
    the spike's measured absurdity becomes permanent.
  RECOMMENDED: A.
  BLOCKING_REASON: the cert is the shipped serving license for the callgraph fastpath;
    changing its role is a serving/trust-invariant change (the §8-of-EC-1 direction
    executed in code) — operator-class.

- *(D-R4 — RETIRED from the open list, iteration 4 / review-3 change 4: its substance
  was RATIFIED by R-RAT-2 (FC2A-UNION-A) and its matrix now lives in R-RAT-2's
  "Recorded consequences" above — a ratified decision is not re-presented as open. The
  ID is retired, not reused; prior reviews' D-R4 references resolve to that record.)*

- ID: D-R5
  QUESTION: The M-6 collision — EC-1 §8c ratified "M-1..M-6 proceed under the interim
    rule"; but M-6-for-a-covered-L deletes the pipeline's resolved-CALLS rows — the
    persisted second witness this design makes load-bearing (union input, ledger input,
    R-0 fallback). What is M-6's fate under reconciliation?
  OPTIONS:
  - A (RECOMMENDED) — M-6-for-covered-L is FORECLOSED WHILE union serving is the
    ratified shape: the rows are a witness stream, so deletion gate 1 ("no shipped
    command depends on it by default") re-fails BY DESIGN. Cost, honestly: the §2b
    CALLS-share footprint win is forfeited for covered languages (upper bound:
    whole-`edges` ≈ 0.9–1.8 GB/copy at kernel scale per EC-1 §4.5 — but kernel is C,
    uncovered; the covered-language share on the deployment target is the TS subset,
    smaller, UNMEASURED). Re-open condition NAMED: monorepo shows pipeline-only(L) ≈ 0
    sustained (the rows are then content-redundant) AND a ratified refresh-time
    agreement design (witness classes computed at index/refresh from the transient
    resolution stream + persisted as a summary) preserves the labels without the rows.
  - B — M-6 proceeds as spec'd (drop rows post-gates). Consequence: for covered L the
    union degenerates to S-only (every edge `semantic`, agreement class dead), the
    divergence RATE — §8b's own ratified surfaced fact — becomes uncomputable for
    exactly the languages it is about, and R-0's fallback thins to the R2 producer
    ladder. Contradicts the direction it executes under.
  - C — design refresh-time agreement NOW so M-6 stays schedulable. Consequence:
    persistence machinery for a need not yet demonstrated (M-6-for-TS is already
    blocked on the producer story — EC-1 C-7 names the dev-only pin); speculative
    double work; it is A's re-open path, built early.
  RECOMMENDED: A.
  BLOCKING_REASON: a direct contradiction between two ratified texts (§8c's "M-1..M-6
    proceed" vs §8b's union-with-witnesses) — must be resolved by the operator, not
    silently narrowed by a builder.

- ID: D-R6 — **re-scoped by R-RAT-2: now ratifies the CONCRETE union-accounting read
    mechanism (§5.3.2-5.3.5), not whether union aggregates exist (that is ratified)**
  QUESTION: Ratify the union accounting's serving shape: g1u/g2u/g3u served READ-TIME
    from the witness ledger in W-BOTH (g2u liveness reduction-only; g3u pairs from the
    union CALL graph only; g1u additive beside pipeline figures), with persistence
    DECLINED now under the §5.3.5 named re-open condition?
  OPTIONS:
  - A (RECOMMENDED) — as §5.3 specifies. Consequence: known false positives (a symbol
    whose only incoming edges are compiler-witnessed) stop being flagged where a second
    witness measured [measured class exists: amodx's 48 semantic-only call instances +
    reference-only-incoming symbols]; every union value carries its accounting +
    coverage basis; ledger absent → exactly today's answers (strict generalization);
    zero new persisted families; impossible to serve a stale union value (the
    staleness class never exists).
  - B — union aggregates land in the PERSISTED family beside the pipeline rows now.
    Consequence: the persisted family becomes witness-pair-keyed — staleness honesty
    machinery (fingerprint-match refusal) owed IMMEDIATELY for values that recompute in
    ~2 s at measured scale; the full Persistence Completeness checklist for an unproven
    cross-restart need; M-3a's parity-window validation gains a second accounting to
    disentangle. This is §5.3.5's re-open path, built before its evidence (S-1).
  - C — no union aggregates at all; witness summaries only (iteration-0's shape).
    Consequence: REJECTED BY RATIFICATION (FC2A-UNION option B analog) — the discovery
    surfaces would keep serving KNOWN-incomplete figures (g2 false-dead, g1 counts
    excluding 48 measured genuine call instances) while holding the correction in
    memory.
  RECOMMENDED: A.
  BLOCKING_REASON: changes served discovery answers' basis (liveness, degree, sketch,
    orientation totals) — a certainty-model call the ratification mandates but whose
    concrete mechanism (read-time, reduction-only, labeled, non-persisted) still needs
    the operator's sign-off; M-3a's validation design needs it fixed.

- ID: D-R7
  QUESTION: The default-flip gate — may M-R2's union serving become the DEFAULT for
    covered languages only after S-1..S-3 (§6.2) are confirmed on the deployment-target
    monorepo run?
  OPTIONS:
  - A (RECOMMENDED) — yes: flag-gated until S-1 (walk cost), S-2 (pipeline-only causes),
    S-3 (identity at scale) pass; the flip is its own recorded step. Consequence: no
    unvalidated serving change on the primary surface; mirrors the ROADMAP NOW gate.
    All three gates now carry REAL pre-signals (§6.2: 1.77 s whole-run; three observed
    pipeline-only causes; suspects 0; guard predicate ∅ exact) — encouraging, but 250
    files is not 160k, and
    compile-skip causes remain unobserved; the flip still waits for the monorepo.
  - B — default immediately at M-R2 on the amodx/zap evidence. Consequence: a
    serving-path behavior change on the primary surface validated at 1/640th of
    deployment scale, with one known cause class (compile-skip/producer-skip) never yet
    observed — a smaller version of the same "green suite, lying surface" failure the
    honesty reckoning documented; the cost saved is only waiting.
  RECOMMENDED: A.
  BLOCKING_REASON: release posture for the primary discovery surface; binds milestone
    sequencing to field evidence.

- ID: D-R8
  QUESTION: Ledger + divergence-summary persistence — in-memory-only (cert lifecycle)
    or a persisted family?
  OPTIONS:
  - A (RECOMMENDED) — in-memory, fingerprint-keyed, non-durable; doctor/trust render
    `null`/absent when not yet computed (unknown, never a stale number). Consequence:
    zero Persistence Completeness burden; impossible to serve a STALE divergence rate
    as current (the false-trust hazard of persistence); cost: the walk recomputes per
    fingerprint/restart — today's cert economics, now MEASURED cheap at 250-file scale
    (1.77 s whole-run, §3.0b), S-1-checked at deployment scale. Covers the union
    accounting's read surfaces too (§5.3.5 — one lifecycle, one re-open condition).
  - B — persist the summary (an FC4-shaped measurement family). Consequence: doctor
    shows divergence with no resident LG; but write path/refresh/copy-forward/trust
    impact all owed (full checklist), and a persisted rate can misdescribe the CURRENT
    pair after either witness moves — staleness honesty machinery needed for a
    diagnostic. Deferred unless the monorepo shows the walk cost forces caching across
    restarts (the same §5.3.5 re-open).
  RECOMMENDED: A.
  BLOCKING_REASON: the only place this design could mint a new persisted family —
    explicitly declining it is itself a ratification-class scope decision.

---

## Builder evidence ledger (this deliverable; iteration-1 items marked ⊕, iteration-2 marked ⊗, iteration-3 marked ⊛, iteration-4 marked ⊙, iteration-5 marked ⊚, iteration-6 marked ⊜)

```text
EXECUTED (command run this slice, output observed):
- git status --porcelain → empty (clean tree before edit); git log --oneline -3 → HEAD 103f7c9
- rmap --version → "rmap 0.6.0"; rmap repo list (read-only) → registry listing incl.
  OpenXcom/buildroot/django/duckdb/grpc-java/langchain4j under legacy-codebases
- ls ../legacy-codebases → 18 repos, no substantial-TS repo present
- nginx census: git ls-files | ext count → 260 c + 135 h (top: c, h — pure C)
- spring-petclinic census: → 47 java (top: java — pure Java)
- grep IrEdge/EdgeType vocabulary (repo-graph-ir); grep callers/callees + edge_type sites
  (repo-graph-livegraph, livegraph_feed.rs); grep funnel (dispatch.rs); grep CallReliabilityView
  (10 files); grep resolution/'"static"' (storage) — each hit read in context
⊕ THE DATA-UPGRADE RUNS (iteration 1; full recipe + raw artifacts:
  .agent-manager/slices/RECON-DESIGN-1/runs/{ANALYSIS.md,amodx/,zap-engine/}):
  - producer re-provisioned: nvm install 18 → Node v18.20.8; npm install
    @sourcegraph/scip-typescript@0.4.0 into /private/tmp/repo-graph-tools/…; wrapper at the
    launchd-env path (launchctl getenv RMAP_SCIP_TYPESCRIPT names it; the prior provisioning
    was wiped from /private/tmp) → scip-typescript-node18 --version = 0.4.0
  - rmapd REBUILT release from HEAD 103f7c9 (cargo build --release -p rmapd, exit 0) — the
    prior binary's timestamp predated the LIVEGRAPH-PARTIAL-FIX-1 commit timestamp (ambiguous)
  - amodx: 8 scip-typescript project indexes, all rc=0 (1–5 s each); isolated rmapd --stdio
    (RMAP_STATE_ROOT=/private/tmp/recon-design-1-amodx, RMAP_CALLGRAPH_DIFF on): index +
    8× livegraph_preload + callers requireRole → 3,022,226 B callgraph-diff/v3 artifact,
    RED, precondition null, corpus 2430, canonical 10,300/36/495; repeat run in a second
    state root: 1.77 s wall, measurement blocks byte-EQUAL (uid-normalized symbols EQUAL)
  - amodx kind join: read-only ingest_partition harness (scratch crate in /private/tmp,
    path-dep on repo-graph-scip-ingest; source retained as runs/amodx/ir-kind-harness.rs)
    → 13,352 IR edges (542 Calls / 12,189 References / 621 Imports); joined 100% of the
    10,300 SCIP-only divergent edges (0 unjoined): 49 Calls + 10,251 References; ctor-
    targeted 13/13 References; per-partition split recorded in ANALYSIS.md
  - amodx pipeline-only source verification (read-only): renderer route.ts:2 imports
    revalidatePath/Tag from "next/cache" (pipeline bound them to backend/src/lib/
    revalidate.ts — misresolution); admin api.ts:40 + renderer CookieConsent =
    localStorage.removeItem (misresolution ×2 callers); BlockEditor.tsx:5 imports
    getExtensions from "@amodx/plugins/admin" + upload.ts:2 validateUpload from
    "@amodx/shared" (genuine cross-package); Toolbar.tsx:172+ cn(...) JSX call sites exist
  - zap-engine: 3 scip indexes rc=0; same isolated procedure → 2,041,015 B artifact,
    corpus 2586, canonical 2,479/1,585/137; bucket-measured pipeline-only 27; unanswerable
    3,693 (rs 3,440 + py 116 Unavailable dominant); run 0.79 s wall
  - isolation proof, all runs: operator registry SHA-1 218414423f398e31f5c5a2ce627056146f21ebae
    IDENTICAL before/after; git status --porcelain in amodx AND zap-engine → 0 entries
  - identity_suspect detector over the amodx artifact → 0; reference fan-in distribution
    → max 456, top-8 ≥ 268, mean 5.8
⊗ retained-artifact re-verification (iteration 2): python3 structural walk of
  runs/amodx/callgraph-diff.json → canonical_edges {LG 10,795 / SQ 531 / scip_only
  10,300 / pipeline_only 36 / shared 495 / union 10,831}, corpus_size 2430,
  rollup {callers_sqlite_only 12, callees_sqlite_only 9, livegraph_unanswerable 1,071,
  livegraph_panic 0, field_mismatch 1} — the §3.0b/§5.4 arithmetic's inputs re-read
  first-hand before correction
⊛ THE COLLISION SWEEP (iteration 3; offline, over RETAINED artifacts only — no new
  isolated runs): python3 over runs/amodx/ir-edges.tsv (13,352 S edges) +
  callgraph-diff.json (corpus symbols + divergence-bucket keys) → 2,631 distinct keys;
  final-segment classification: SCREAMING_SNAKE (pipeline vocabulary) {PROPERTY 600,
  FUNCTION 521, CONSTANT 477, TYPE_ALIAS 154, INTERFACE 109, METHOD 96, CLASS 24,
  CONSTRUCTOR 13, VARIABLE 7} vs TitleCase (SCIP-suffix vocabulary = fallback-minted)
  {Term 241, Type 1, Namespace 1, Method 1}, other segments none; case-folded
  SEGMENT-vocabulary intersection {METHOD}; case-folded FULL-KEY near-collisions 0;
  EXACT collisions 0 — deduced: a byte-collision requires an identical final segment
  and the observed segment vocabularies are byte-disjoint (case separates them)
  [⊙ WITHDRAWN as a measurement claim per review-3: caveat (c) below makes the
  deduction circular — superseded by the ⊙ exact guard-predicate measurement];
  fallback-endpoint incidence per kind: Calls 1/542, References 880/12,189; the three
  non-Term fallback keys enumerated (all generated-file entities, e.g.
  renderer/.next/types/cache-life.d.ts#cacheLife:SYMBOL:Method — whose case-folded
  pipeline twin `…:SYMBOL:METHOD` is exactly the guarded scenario).
  POPULATION CAVEATS, per the evidence law: (a) fallback classification is BY SEGMENT
  VOCABULARY — deduced from the two OBSERVED constructors, because the artifact carries
  no per-node identity_source; (b) the sweep's P-side keys are the corpus + bucket keys
  the artifact retains, NOT P's full node-key set — the M-R1 ledger computes the guard
  over the full key sets both witnesses hold under the pin; (c) [⊙, review-3] a
  byte-identical collision would appear as ONE string — indistinguishable without
  source provenance — so this sweep is a HEURISTIC pre-signal with no observed
  collision indication, never an evaluation of `fallback_keys(S) ∩ keys(P)`.
⊙ THE EXACT RECOMPUTATION (iteration 4; offline + one scratch-harness re-run; NO new
  isolated daemon runs, NO SCIP toolchain re-exercised; script + outputs retained:
  runs/amodx/iter4-recompute.py):
  - INPUTS: retained callgraph-diff.json + ir-edges.tsv; the SURVIVING iteration-1
    isolated state root /private/tmp/recon-design-1-amodx (SQLite opened READ-ONLY
    immutable URI — a retained evidence artifact of the isolated run, NOT the operator
    registry; zap likewise); the retained scratch harness EXTENDED to dump per-node
    identity_source (source updated in runs/amodx/ir-kind-harness.rs) and re-run over
    the retained SCIP indexes → runs/amodx/ir-nodes.tsv (3,089 rows); regenerated
    ir-edges.tsv byte-identical to iteration 1's (SHA-256 f0cf80ff… both) —
    deterministic replay re-verified.
  - RECONSTRUCTION SOUNDNESS: re-implementing diff.rs's EdgeViews-canonical (max of
    projections) + edge_magnitude (per-identity min/excess) accounting [both re-read
    first-hand: diff.rs:551-629] over P (from the DB: 531 CALLS instances / 480
    identities / 2,530 node keys — dumps retained: pipeline-call-edges.tsv,
    pipeline-node-keys.tsv) × S (tsv) × the artifact's per-projection measurability
    (caller_edges/callee_edges livegraph null ⟺ unmeasured; 450/621 — matches
    projection_incidences exactly) reproduces the artifact's canonical
    sqlite_total/pipeline_only/shared = 531/36/495 EXACTLY (the LG-only-side totals
    over-count under the unlisted-symbol measurability default, which cannot affect
    any P-side class — every P projection is measured, sq unmeasured = 0).
  - POPULATION CORRECTION: canonical figures are INSTANCE counts (multiplicity
    preserved), not identities — shared 495 instances = 455 identities; P 531 = 480;
    pipeline_only 36 = 25 (iterations 0–3 labeled these "identities").
  - KIND-ALIGNED CLASSIFICATION (R3-C1): both 494 instances / 454 identities
    (S strict-Calls on pair required); syntactic 13/12 = boundary 11/10 + file_scope
    1/1 + uncorroborated 1/1; unmeasured 24/14 (the old ⊗ INFERRED 36−12=24
    attribution now ENUMERATED exactly); semantic_only 48/37; union calls 579;
    S kind totals: Calls 542 instances/491 identities, References 12,189/7,069;
    agreement 494/507 = 97.4%; closures 531 = 494+13+24 and 542 = 494+48 both exact;
    multiplicity deltas on both-pairs 0. The iteration-1 "542 → 493 both-witnessed"
    was divergence-list set-membership: one S-Calls instance on a pair also carrying
    References excess was miscounted scip-only (hence 49→48 and 493→494).
  - THE RECLASSIFIED PAIR, source-verified: admin/src/main.tsx:FILE → #loadConfig
    (P line 33; `loadConfig().then(…)` at module scope — read first-hand); P CALLS
    with FILE caller: exactly 1 instance in the whole corpus, and its S References
    edge is the FileScopeReference strict rule [derive_edges :727-740 re-read
    first-hand this iteration].
  - MULTIPLICITY CORRECTIONS (DB-exact): POST→revalidatePath ×2 (lines 35/38) +
    revalidateTag ×1; BOTH removeItem callers ×1 (ANALYSIS.md's "×2 on one" was
    wrong — supersession section appended to ANALYSIS.md, original preserved).
  - THE EXACT GUARD PREDICATE: fallback_keys(S) ∩ keys(P) = ∅ — 280 distinct
    ScipSynthesizedFallback keys (337 node rows; segments Term 272/Type 4/Namespace
    3/Method 1) × 2,530 P keys, per partition and overall; the sweep's edge-set count
    validated (exactly 244 fallback keys touch edges); the one fallback Calls endpoint
    identified + source-verified (fbpixel.ts trackFBEvent → window.fbq ambient
    declaration, read first-hand). Node sources: AstAdopted 2,366 / fallback 337 /
    AstFileScope 386; ZERO fallback-mixed keys; all 386 AstFileScope keys duplicated
    as AstAdopted nodes (the §3.5 duplicate-key observation).
  - ZAP COVERAGE SPLIT (exact, from the surviving zap DB + artifact measurability):
    P 1,722 instances / 1,366 identities; pipeline_only 1,585 = 29 dual-measured
    (27 identities) + 1,556 unmeasured (1,212 identities) — closure exact; 98.2%
    coverage share, single-unit (the prior "27 of 1,585" mixed identity- and
    instance-level units).
⊚ THE INSTANCE-LEVEL RE-VERIFICATION (iteration 5; offline, RETAINED artifacts ONLY —
  no DB re-open, no /private/tmp dependency, no toolchain; NEW script retained beside
  the run record, prior evidence untouched): python3 runs/amodx/iter5-multiplicity-check.py
  over pipeline-call-edges.tsv × ir-edges.tsv × callgraph-diff.json measurability →
  reproduces both 494 / syntactic_only 13 / unmeasured 24 / semantic_only 48 / union
  579 / agreement 494/507 = 97.4% (the reviewer-confirmed core, from retained files
  alone); P-excess instances on corroborated pairs 0; S-excess instances on
  corroborated pairs 0; pairs split across classes 0 — the §3.3 instance rule moves NO
  measured figure, both `multiplicity` sub-classes are measured-empty at amodx scale,
  and today's identity counts are non-overlapping (VERDICT: ALL EXPECTED)
OBSERVED (read first-hand this slice):
- docs: recon-spike-1.md §5 FULL (incl. §5.7-5.10); engine-consolidation-1.md FULL (§1-§9 + §8
  ratification); livegraph-partial-fix-1.md FULL; reliability-reframe-1.md; metric-lang-coverage-1.md
  §0-§2; VISION.md; ROADMAP.md; CURRENT_SLICE.md; agent_docs/architecture.md
- code: repo-graph-ir/src/lib.rs:40-129 (IdentitySource incl. AstFileScope; EdgeType; EdgeBasis);
  repo-graph-livegraph/src/lib.rs:440-699 (callers :488-577 kind-blind :532-534; callees :607-699
  kind-blind :652-653; Unavailable null≠empty :500-506; StructuralNodeNoCallGraphContent :553-560,
  :687-694); daemon-runtime/src/callgraph_cert/mod.rs FULL (union corpus + short-circuit :249-268;
  row builders :104-162; build+diff hook :276-294; ladder :306-335; eligibility :375-387; the
  :118/:141 "CALLS" doc-comments); daemon-runtime/src/livegraph_feed.rs:455-529 (hardcoded CALLS
  :490/:509; zero placeholders :487-489); daemon-runtime/src/dispatch.rs:3540-3599 (enrich funnel);
  agent/src/reliability.rs:1-100 (shared projection; conservative denominator);
  repo-graph-scip-ingest/src/lib.rs:655-756 (strict kind derivation; is_call_at :734);
  storage/src/queries.rs edge_type/resolution row sites (:676,:740 via grep read)
⊕ code (iteration 1): repo-graph-scip-ingest/src/lib.rs:500-560 (ast_facts_for_source runs
  TsExtractor, harvests Calls-edge locations as call_sites — the SHARED detector) +
  :1102-1145 (ingest_partition public API; KEY-NAMESPACE repo-relative key derivation);
  daemon-runtime/src/dispatch.rs:870-919 (handle_livegraph_preload; partition_prefix =
  source_root relative to repo — KEY-NAMESPACE-REPO-RELATIVE-1); livegraph_feed.rs:139-157
  (preload_partition is ADDITIVE per partition — get_or_insert + load_partition);
  callgraph_cert/diff.rs:555-660 (EdgeViews/canonical MAX merge; edge_magnitude min/excess
  classification — the §3.6/§3.7-5 coverage-divergence blend; diff_direction's
  both-sides-measured bucket rule :653-657)
⊕ review-0.json (verdict escalate; the two DECISION_REQUIREDs + three corrections) +
  the iteration-1 selection packet's OPERATOR_NOTEs (ratifications + DATA UPGRADE binding)
⊗ code (iteration 2): repo-graph-scip-ingest/src/lib.rs:396-428 (find_match Some arm —
  `ast.stable_key` adopted verbatim, `IdentitySource::AstAdopted`; attributes ride the
  adopted node "the facts are AST/producer facts, NOT SCIP — SCIP only triggers the
  join"), :429-445 (None arm — `ScipSynthesizedFallback` synth key [the record's
  original "shape-distinct" characterization was WRONG — R-RAT-4, ⊛ below];
  attributes None "unknown, not zero"), :1167-1170
  (matched/reconciled/fallback/file_scope node-count aggregation), :1174-1182
  (symbol_to_key stores the adopted-or-fallback key + is_fallback bit)
⊗ review-1.json (escalate: the identity-adoption contradiction + the §5.4 arithmetic
  correction) + the iteration-2 OPERATOR_NOTE (R-RAT-3 ratified + the 3-item revise list)
⊗ CURRENT_SLICE.md ledger: RUST-INGEST-PROVE-1 "identity ~94–96% SCIP-synthesized
  fallback" (the on-record adoption-miss bound, §3.5 risk 3); XPART-PROVE-1B entry
  (dist↔src export-surface reconciliation — its byte-equal keys are
  reconciliation/adoption products, hence dropped from the identity-corroboration chain);
  docs/ROADMAP.md NOW (the field-validation gate §6.2 mirrors)
⊛ code (iteration 3 — the R-RAT-4 evidence, each read first-hand): ts-extractor/src/
  extractor.rs:340-364 (`make_stable_key` — the shared grammar
  `{repo_uid}:{file_path}#{name}:SYMBOL:{subtype_str}` at :351-354; the `:dupN`
  disambiguation :356-363; the latent `unwrap_or_else` Debug-format branch :346-349)
  + :409-429 (`make_symbol_node` calls it) + the test-pinned key shapes
  (:2692 `r1:src/greet.ts#greet:SYMBOL:FUNCTION` et al.); indexer/src/types.rs:133-174
  (`#[serde(rename_all = "SCREAMING_SNAKE_CASE")]` on `NodeSubtype` + the variant
  vocabulary incl. `Method`→`METHOD`, `Package`→`PACKAGE`); repo-graph-scip-ingest/
  src/lib.rs:116-159 (`symbol_kinds` + `descriptors_info`/`kind_of` — the fallback
  segment is `format!("{s:?}")` Debug TitleCase of the SCIP suffix), :390-446 re-read
  (adoption Some arm; the :432 mint), :449-476 (`AstFileScope` arm adopts the
  pipeline's own `n.stable_key` → adoption-COMPATIBLE for guard 2), :1160-1188 re-read
  (`symbol_to_key` + `is_fallback` bit); repo-graph-ir/src/lib.rs:33-40
  (`CanonicalKey::from_existing` = plain newtype wrap — no namespace, no guard) +
  :53-66 (the three `IdentitySource` variants — the guard's discriminant)
⊛ review-2.json (escalate: the shared-grammar proof — extractor.rs:351-354 vs
  lib.rs:432, `from_existing` guardless — + the FALLBACK-KEY-INVARIANT
  DECISION_REQUIRED + the §7.0 independence-wording secondary) + the iteration-3
  OPERATOR_NOTE (R-RAT-4 ratified: option A explicit collision guard; option B the
  recorded escalation path; option C rejected as brittle; "proceed to finalization")
⊙ review-3.json (revise: the four required changes — S-Calls-required corroboration;
  collision-claim downgrade with the "or a run containing identity sources and the
  complete P key set" alternative this iteration took; the R-0/accounting-label
  contradiction; D-R4 not re-opened) — each verified against artifacts before writing;
  the reviewer's expected 493/14/97.2% figures corrected by the ⊙ exact join (they
  inherited iteration-1/2's deduced composition), stated openly in the iteration-4 note
⊚ code (iteration 5 — the R-RAT-6 eligibility + R-RAT-5 attribution mechanisms, each
  read first-hand this iteration): daemon-runtime/src/livegraph_feed.rs:1727-1752
  (`import_cert_fingerprint` — per partition
  `{id}@{epoch}:f{fresh}:ts{ts}:{source_inputs_hash}:{producer_fingerprint}`, bound to
  `snap:{snapshot_uid}` + the policy version) + :59-79 (`RequestEpoch` — pinned
  snapshot + eligibility-witness fingerprint; `None` ⇒ eager SQLite) + :427-429 (the
  serve-time fingerprint re-check precedent); repo-graph-livegraph/src/
  module_cycle_cert.rs:62-78 (`LivePartition` — `fresh` = `status == Fresh`, epoch
  "bumps on every swap", the invalidation-key rationale); repo-graph-livegraph/src/
  lib.rs:394-401 (`begin_refresh` → Refreshing/PrecisionPending; `mark_stale` — "inputs
  changed; refresh not yet run") + :67-71 (RefreshStatus→FreshnessState mapping) +
  :784-785 (epoch-mismatch Stale degradation); daemon-runtime/src/
  livegraph_refresh.rs:545-566 (the production warm-cache path: cache-warmed partition
  marked Stale + `DegradationReason::ProducerUnavailable` — the measured
  stale∧producer-absent compound); callgraph_cert/mod.rs:296-335 (`callgraph_is_green`
  fingerprint match/rebuild ladder; re-read) + :337-357 (`callgraph_cached_green` peek)
  + :375-387 (`callgraph_cert_eligibility` build-then-peek — re-read, the one-guard
  atomicity rationale); repo-graph-ir/src/lib.rs:364-378 (`IrEdge` —
  src/dst/edge_type/basis/provenance/import; NO per-occurrence site — the §3.3
  count-only attribution fact) + :212-239 (`Partition.build_inputs_hash`) + :256-268
  (`Provenance` — no range either); callgraph_cert/diff.rs:551-629 re-read
  (`EdgeViews::canonical` MAX; `edge_magnitude` min/excess — the instance-level
  accounting the §3.3 model now matches)
⊚ review-4.json (revise: the two contract-level inconsistencies — §3.3's pair-level
  prose vs the §5.4 closure [lines 418-421 cited]; §4.2's regime overlap on a
  stale-resident partition + the one-message-for-three-states defect [lines 782-783
  cited]; the empirical core independently recomputed and CONFIRMED) + the iteration-5
  OPERATOR_NOTE (the closing two RATIFIED: instance-level provenance with
  never-claim-unconfirmed summaries; W-BOTH/W-ONE/W-NONE mutual exclusivity via the
  actual epoch/fingerprint eligibility + freshness condition, W-ONE reason-specific;
  "do not disturb" the confirmed core — honored, no measured figure moved)
⊜ code (iteration 6 — the review-5 mechanisms, each read first-hand this iteration):
  livegraph_feed.rs:39-45 + :63-79 (`RequestEpoch` — the capture doc + field:
  "`Some(fp)` = a GREEN no-loss cert exists at EXACTLY the resident fingerprint"; the
  orient-vs-callgraph capture split at :39-41 — the §4.2/§5.1 capture-contract facts),
  :189-268 (the `FallbackReason` vocabulary + `as_str`), :271-305 (`LgAuto` +
  `auto_outcome` — a serve-site `None`, incl. the pin-mismatch return, folds into
  `LiveGraphUnavailable` :290-292 — the §4.2 naming fact), :427-429 + :447-475 (the
  serve-time pin re-checks, callers + callees twins — the EV-A fail-soft), :477-513
  (`caller/callee_results_from_keys` — the §3.7-3/-4 row builder M-R2 replaces),
  :536-550 + :640-654 (`callers_value`/`callees_value` — `count = …len()`, the
  row/count invariant review-5 defends), :1727-1752 re-read (the FULL fingerprint
  composition incl. the `ts` language flag + `pol:` policy version — §4.2 now quotes
  it exactly); callgraph_cert/mod.rs:84-162 re-read (`RowEnrichment` +
  `lg_caller_rows`/`lg_callee_rows` — one row per CALLS edge instance; NO location
  fields on this row family), :164-216 (the multiset compare), :218-271
  (`callgraph_compare_is_exact` — `None` ONLY on a storage error [the `.ok()?` sites];
  no-LG :229 and empty-partition-set :233-235 yield `Some(false)`, not errors — the
  §4.2 transient-2 capture-failure class exactly), :276-294 (`build_and_store` —
  verdict stored + the diff hook), :296-335 + :337-357 + :359-387 re-read (the
  is_green ladder / cached peek / build-then-peek eligibility — the
  `verdict == "GREEN"` arm at :384 and the swap-exclusion atomicity rationale
  :366-374); storage/src/queries.rs:657-718 + :720-779 (`find_direct_callers`/
  `find_direct_callees` FULL — the served `line`/`column` is the OPPOSITE ENDPOINT
  node's `n.line_start, n.col_start` [:674-675, :738-739]; the location join is on
  `nodes`, never on `e` — the §3.3a definition-location fact);
  storage/src/agent_impl.rs:1021-1123 (`find_symbol_callers`/`callees` — the
  cert/ledger row family: stable_key/name/file/module only, NO location at all);
  storage/src/types.rs:590-612 (the `edges` DDL — nullable occurrence-site columns
  `line_start`/`col_start`/`line_end`/`col_end` EXIST and no serving query reads them;
  population rate unmeasured — the §3.3a upgrade-pricing fact)
⊜ review-5.json (revise: the two required changes — (1) the uncovered epoch-mismatch
  state + the GREEN-only capture contract [livegraph_feed.rs:64-72 and
  callgraph_cert/mod.rs:375-387 cited by the reviewer, both re-verified first-hand
  above]; (2) the row/count invariant [`livegraph_feed.rs:542,646` vs the §3.3/§5.2
  rowless S-excess] + the false "P rows carry call-site locations" claim
  [queries.rs:666-711, :729-774 cited, re-verified]; the retained multiplicity script
  re-run by the reviewer reproduced 494/13/24/48, union 579, zero multiplicity deltas
  — the empirical core stays CONFIRMED and untouched)
INFERRED (labelled inline): the union-operator identity claim (R-0 formal statement); the
  expectation that the fixture's 7 SCIP-only edges are all References-kind (basis: the strict
  rule + the MEASURED 13/13 References for ctor-targeted amodx edges — M-R1 still records the
  fixture's actual split); the 36−12=24 decomposition attribution to LG-unmeasured-side edges
  (basis: bucket rule OBSERVED in diff_direction :653-657 + rollup arithmetic 12+9=21
  incidences) [⊙ upgraded: now ENUMERATED exactly — 24 instances / 14 identities, the
  ⊙ block]; trust-core citation service.rs:875 adopted from EC-1 §3.4-8's first-hand ledger
  (that doc OBSERVED; the line not re-read this slice)
⊗ the `both` kind composition 493 + 2 (§3.0b/§5.4): deduction from two EXECUTED
  iteration-1 counts (canonical shared 495; S strict-Calls shared 493 — runs/ANALYSIS.md)
  — a shared pair not covered by an S `Calls` edge is necessarily covered by
  non-`Calls`-kind S edges; the 2 pairs are enumerable from retained ir-edges.tsv, not
  re-enumerated this iteration [⊙ SUPERSEDED: the exact join corrects both counts —
  494 + 1, and the composition concept itself is retired by R3-C1 (the ⊙ block; §3.1 ²)]
⊗ projection-population totals 4,860 (amodx) / 5,172 (zap-engine): corpus 2,430 / 2,586
  symbols × 2 directions (the artifact's per-symbol caller+callee projection model,
  OBSERVED in diff.rs)
NOT RUN (with reason): cargo build/fmt/clippy/test on the WORKSPACE — docs-only change to this
  repo, no repo code path touched (EC-1 docs-only precedent; the rmapd release REBUILD above
  compiled the unchanged workspace for the evidence runs, exit 0 — not a validation of new
  code, none exists); isolated rmap-CLI dogfood / dev-install — no repo binary changed;
  daemon index/orient against operator state — spec slice, isolation mandatory and proven;
  the monorepo divergence run — requires the deployment-target machine (it IS §6.2's
  remaining gap, deliberately: "the monorepo confirms at deployment scale")
⊛ NOT RUN (iteration 3): new isolated index/preload runs — the RETAINED iteration-1
  artifacts sufficed for every iteration-3 claim (the collision sweep is offline); the
  SCIP toolchain was not re-exercised because nothing in the R-RAT-4 mandate required
  it; an exact live `fallback_keys(S) ∩ full keys(P)` intersection needs per-node
  identity_source the artifacts don't carry — it is M-R1's ledger computation + test,
  named in §6.1, not silently skipped [⊙ the intersection has now been computed
  OFFLINE from the extended dump — M-R1 retains the LIVE ledger computation + test]
⊙ NOT RUN (iteration 4): new isolated daemon index/preload runs and the SCIP toolchain
  — the recomputation is offline over retained/surviving artifacts; the one execution
  beyond python/SQL was the scratch harness re-run (cargo run in /private/tmp, path-dep
  compile of the UNCHANGED workspace crates, exit 0 — repo untouched); a zap-engine
  KIND-aligned split — zap never had a kind harness; its §3.0b figures are labeled
  kind-blind, and only its COVERAGE split (which needs no kinds) is quoted as exact;
  the monorepo divergence run — unchanged, §6.2's deliberate remaining gap
⊚ NOT RUN (iteration 5): everything beyond the one offline python check — the two
  ratified changes are contract-level (model/serving/schema), grounded in code re-reads
  + the retained measurements; no new isolated daemon runs (nothing new to measure —
  the delta classes are measured-empty and the regime predicate is a state condition,
  fixture-tested at M-R1, not corpus-dependent), no SCIP toolchain, no scratch harness;
  workspace cargo gates — docs-only change, no repo code path touched (EC-1 precedent,
  unchanged); the monorepo divergence run — unchanged, §6.2's deliberate remaining gap
⊜ NOT RUN (iteration 6): all measurement — NOTHING new to measure: both review-5
  changes are serving/capture CONTRACTS, grounded in the first-hand code reads above;
  the one class whose serving changes (S-excess `semantic`/`multiplicity`) is
  measured 0 at amodx (the ⊚ retained-artifact verification stands unchanged, and the
  reviewer independently re-ran it at review-5 — zero multiplicity deltas), so no
  quoted figure can move; no new isolated daemon runs, no SCIP toolchain, no scratch
  harness; workspace cargo gates — docs-only change, no repo code path touched (EC-1
  precedent, unchanged); the monorepo divergence run — unchanged, §6.2's deliberate
  remaining gap
```

Decide-and-record (local, one line each): deliverable sections numbered §3–§7 per §2's
own naming; the pre-existing administrative sections (stop conditions / validation — §3/§4
at selection time, as the relay packet cites them) are renumbered §8/§9 below, text
UNCHANGED (the EC-1 precedent). Witness classes named `both`/`semantic`/`syntactic`
(reader-meaningful; not vendor names — "scip"/"tree-sitter" are our internals). Coverage
regimes named `W-*` (collision-avoidance with EC-1's R1/R2/R3 and REP-1/REP-2). The
ledger is named "witness ledger", NOT "cert v2" — it records what each witness saw; it
certifies no equality (name matches semantics). ⊕ Union-accounting granularities named
`g1u/g2u/g3u` (the ratified `g1/g2/g3` identifiers stay pipeline — no rename of ratified
vocabulary; §5.3.0). ⊕ `syntactic` sub-classes named `boundary`/`uncorroborated`
(mechanical topology split; reader-frame labels in §3.1) *[⊙ extended: + `file_scope`,
the measured third mechanical member]*. ⊕ Divergence artifacts retained
under `.agent-manager/slices/RECON-DESIGN-1/runs/` (the RECON-SPIKE-1 runs/runB
precedent; `.agent-manager/` is gitignored [EXECUTED: git check-ignore] so the working
tree stays single-file — the operator note's "retain the artifacts" satisfied without a
scope violation). ⊕ The scratch kind-join harness lives OUTSIDE the repo (/private/tmp),
source retained in runs/amodx/ — no repo file added for a one-shot evidence tool. The
§3.7 defects are surfaced here and folded into M-R1/M-R2 scope rather than proposed as
standalone renames (boundary-touching; code out of scope this slice).
⊗ (iteration 2) `both` is defined at canonical-PAIR level — matching what the artifact's
canonical block actually computes — with the kind composition (`via_call` /
`via_reference_kind`) a recorded ledger sub-count, never a served label
(name-matches-semantics: iteration 1's "same site" wording overclaimed the 2
reference-kind-corroborated pairs). ⊗ §5.4 field names carry their unit
(`pipeline_calls`, `unmeasured_edges`, `projections.{total,unanswerable}`) so a
population mix-up is a schema error, not a prose slip. ⊗ "repeat runs" replaces
"independent runs" in determinism claims — "independent" is reserved for the one
ratified independent property (resolution). ⊗ XPART-PROVE-1B is dropped from the
identity-corroboration chain (its byte-equality is an adoption/reconciliation product);
it remains cited as the `export_alias` repair-path precedent. ⊗ `runs/ANALYSIS.md` is
NOT rewritten — it is a retained run record; its "93.2% corroborated" line is superseded
by §3.0b's labeled correction in THIS spec (evidence records are corrected by
supersession, not edited).
⊛ (iteration 3) `identity_collision` named for what it IS — a DETECTED key collision,
a fact, distinct from `identity_suspect` (a symptom-based suspicion); the two never
share a counter (name-matches-semantics). ⊛ Classification precedence recorded: the
collision guard runs BEFORE `both` classification — merging is
identity-source-conditional (`AstAdopted`/`AstFileScope` only), never key-bytes-alone
(§3.2, §3.5 guard 2). ⊛ The M-R1 guard test hand-constructs the colliding
`PartitionIr` at the guard's own layer — the ingest cannot currently mint the fixture,
and that impossibility is itself the contingent disjointness §3.5 measures, not a
reason to skip the test. ⊛ Guards renumbered 1–4 (no-loss / collision / detection /
repair); every cross-reference updated (risk pointers, §5.4 doctor notes, §6.2 S-3).
⊛ The iteration-3 sweep ran OFFLINE over retained artifacts — no new runs minted, no
toolchain re-provisioned; its two population caveats are labeled in the EXECUTED entry.
⊙ (iteration 4) `both` is KIND-ALIGNED (S strict-`Calls` on the pair required, R3-C1) —
classification and serving now share ONE corpus rule (§3.2/§3.4); the ⊗ pair-level
any-kind definition and its `via_call`/`via_reference_kind` sub-count are RETIRED (the
exact join showed the sub-count was partly a set-membership artifact and partly the
`file_scope` sub-class). ⊙ Third `syntactic` sub-class named **`file_scope`** — a
mechanical, model-asymmetry fact (P models module-init as CALLS; the strict ingest
never does), named for its mechanism, not "compiler declined" (which would misdescribe
it). ⊙ Every canonical figure now carries its true population — edge INSTANCES
(multiplicity preserved), identities beside where they differ — extending the ⊗
unit-in-field-name rule to the counts themselves. ⊙ The iteration-3 heuristic sweep is
labeled as such and superseded by the exact guard-predicate measurement (two-grade
record in §3.5; ANALYSIS.md corrected by an APPENDED supersession section — original
run record preserved, per the ⊗ supersession rule). ⊙ The accounting-label obligation
is scoped to union values + dual-render contexts (R3-C3) so R-0 stays exact — recorded
in §5.3.0. ⊙ D-R4 retired to R-RAT-2's record (R3-C4); IDs are stable, never reused.
⊙ The iteration-4 recomputation ran OFFLINE (python + read-only immutable SQLite over
the surviving isolated state roots + one scratch-harness re-run in /private/tmp); no
daemon touched, operator registry untouched, repo working tree single-file throughout.
⊚ (iteration 5) The regime renames are the OPERATOR'S ratified vocabulary
(W-ONE-AVAILABLE → `W-ONE`, W-ONE-UNCOVERED → `W-NONE`; R-RAT-6), not a builder choice
— recorded with their reading (the names grade the SECOND witness: measuring / exists
but blocked, reason named / nonexistent; the pipeline witness is present in all three,
so no regime means "no witnesses"); every doc occurrence swept, historical iteration
notes bracket-amended rather than rewritten. ⊚ The excess-instance sub-class is named
`multiplicity` on BOTH sides (one mechanism, one name); the semantic non-excess
sub-class `new_pair` (instances on pairs the pipeline call graph lacks entirely —
exactly the rows §3.4-1 admits as NEW union members; name matches serving semantics:
new_pair mints rows, multiplicity never does *[as recorded at iteration 5 — AMENDED
iteration 6, review-5: BOTH sub-classes mint rows now, through one mechanism; the
names still match semantics — new_pair = a pair P lacks, multiplicity = excess
occurrences on a pair P holds — the distinction is the PAIR relation, no longer
row-minting]*). ⊚ The standalone `multiplicity_delta`
field is RETIRED, not renamed — its content moved INSIDE the closure as the two
`multiplicity` sub-classes; a retired field name is never reused. ⊚ The delta-pair row
value is named `mixed` (the pair's instances span classes) — NOT "partial", which would
collide with the ratified `AnswerClass::Partial` on a different axis (answer
completeness vs occurrence corroboration). ⊚ `iter5-multiplicity-check.py` is a NEW
script beside `iter4-recompute.py`, not an edit of it (run records are append-only per
the ⊗ supersession rule; it also avoids re-running iter4's tsv-writing steps — retained
evidence is never regenerated in place).
⊜ (iteration 6) The movement fail-soft's `fallback_reason` is named
`LiveGraphEpochMoved` (M-R2, additive enum value) — name-matches-semantics: today the
serve-site `None` folds it into `LiveGraphUnavailable` [livegraph_feed.rs:290-292],
which is FALSE for a resident-and-available graph whose pin moved; the capture-failure
transient keeps `LiveGraphUnavailable` (there the ledger genuinely is not available).
⊜ S-excess served rows REUSE the `new_pair` S-only row mechanism — one row builder,
two sub-classes; the sub-class distinction renders in `witness_counts` + the human
marker, never as a second mechanism (the smallest design preserving `count == rows`).
⊜ `§3.3a` is a bolded ANCHOR inside §3.3 (the location fact), not a new top-level
section — §2's deliverable contract names §3–§7 and gains no sibling.
⊜ The §4.2 matrix lists UNREPRESENTABLE cells explicitly (¬covered ∧ resident —
coverage is data-driven, so the cell cannot be stated) rather than omitting them —
CLAUDE.md's matrix rule: every cell filled; gaps belong at sign-off, not later.
⊜ The producer axis is recorded OUT of the eligibility predicate (Fresh resident data
corroborates; producers gate future refresh) — a mechanism refinement the exhaustive
matrix forced into the open, not a new decision (the W-ONE ladder and stale compound
already carried producer truth).

---

## 8. Stop conditions *(§3 at selection time — text unchanged)*

- NO code/schema changes — this doc is the only modified file.
- The RED floor is untouchable (disposition = pipeline-only forever); the reconciliation
  NEVER silently alters the trust ratio (Layer-2 landing only, labeled).
- Contradictions with ratified decisions (EC-1 §8, W-B epochs, the interim rule) surface
  as DECISION_REQUIRED, never silently reinterpreted.
- Do NOT commit.

## 9. Validation *(§4 at selection time — text unchanged)*

§3-§7 exist in this doc; every proposal cites the spike's classified evidence or
architecture source (path:line); `git status` shows only this file; the no-second-witness
regime is designed against at least two named legacy-codebases repos' realities (read-only
inspection permitted).

---

## 8. RATIFICATION (human, 2026-07-17)

**ALL SEVEN DECISIONS RATIFIED AS RECOMMENDED** (D-R1, D-R2, D-R3, D-R5, D-R6, D-R7,
D-R8 — all converged in decision-review, zero contested; audit trail in the slice
workspace ratification-packet.md). Binding consequences:

- The witness model is final: pipeline = primary witness (detection + identity, all
  languages); SCIP = semantic overlay (independent resolution + reference kinds on
  adopted identity). Three instance-granular witness classes with count-honest labels;
  strict Calls-requires-Calls corroboration; the fallback-key collision guard.
- SCIP-only reference kinds are a labeled enrichment tier, never silent call-graph
  members (D-R2).
- The GREEN/RED byte-equality cert generalizes into the witness ledger (D-R3);
  ledger + divergence summary in-memory per cert lifecycle, funnel-style surfacing (D-R8).
- Union aggregates serve read-time in W-BOTH; persistence declined — the EC-1 §8c
  interim rule stands until the union earns persistence (D-R6).
- **EC-1 AMENDMENTS (explicit, per D-R5 + the witness-wording corrections):** EC-1 §8's
  "two independent witnesses" phrasing is superseded by this spec's ratified vocabulary
  (shared detection, adopted identity, independent resolution); EC-1 milestone M-6's
  pipeline-CALLS-row deletion is SUPERSEDED for covered languages — those rows are the
  load-bearing second witness of this design.
- The default flip to union serving for covered languages is GATED on the
  deployment-target monorepo run confirming S-1..S-3 (D-R7).

IMPL milestones slice from §6 when queued (first: the witness ledger, fable-sized).
Review history: 7 iterations; the loop forced five honesty amendments (shared detection,
adopted identity, fallback-key guard, Calls-requires-Calls, count-vs-site labeling) —
each ratified explicitly, none silently.
