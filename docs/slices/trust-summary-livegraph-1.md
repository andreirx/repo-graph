# TRUST-SUMMARY-LIVEGRAPH-1: a LiveGraph-native trust-summary producer (the shared coherence prerequisite)

Slice ID: TRUST-SUMMARY-LIVEGRAPH-1
Status: **SPEC-FIRST — specification only. NO implementation, NO code, NO deletion, NO migration, NO default
flip, NO IR extension built.** This document specifies a LiveGraph-native producer that would compute the
values the coherence commands consume from `get_trust_summary` / `assemble_trust_report` (call-resolution
counts, reliability axes, stale/enrichment inputs) from CURRENT-STATE LiveGraph (IR / xref adjacency) INSTEAD
of the SQLite `edges` + `unresolved_edges` tables + the `extraction_diagnostics_json` blob — with a no-loss
cert proving the LiveGraph-derived summary equals the SQLite-derived one. This producer is the SHARED
PREREQUISITE that orient DR-1 / explain DR-E1 named; it is later consumed by the orient/explain/trust
eager-read-elimination fastpaths (NOT this slice).
Track: Stage D / SQLite-raw decommission — Option B, re-scoped (operator-ratified DR-0 → S1) as "producer
program first, per-command fastpaths second." This is the SHARED PREREQUISITE producer.
Baseline: SQLITE-RAW-DECOMMISSION-READINESS-9; ORIENT-SQLITE-FREE-1 (`e10a455`, DR-1); EXPLAIN-SQLITE-FREE-1
(`f3237f9`, DR-E1); TRUST-LIVEGRAPH-1 (the hybrid, `dc55114`) — this slice is the Option-B "full rebase"
that TRUST-LIVEGRAPH-1 §D-TRUST-2 explicitly DEFERRED.

> **HEADLINE FINDING — VERDICT: `NEEDS-EXTENSION` (NOT a clean projection). Read before the design.**
> The orient DR-1 / explain DR-E1 "Option A" framing assumed the producer is "a clean projection" because the
> "resolved/unresolved adjacency [is] already in the IR/xref." **First-hand IR reads REFUTE that assumption for
> the CALL side.** The LiveGraph/IR carries (a) call-edge ADJACENCY (resolved calls → `callers`/`callees`) but
> does NOT carry (b) the RESOLVED-vs-UNRESOLVED DISPOSITION of call targets, NOR (c) the unresolved-edge
> CLASSIFICATION vocabulary, NOR (d) the `extraction_diagnostics_json` aggregate, NOR (e) the enrichment
> metadata. Every non-trivial value the trust summary feeds the coherence cluster — `call_resolution_rate`,
> `unresolved_calls`, `call_graph_reliability`, the classifications, the blast radius, the enrichment state —
> depends on (b)/(c)/(d)/(e). The IR's `IrEdge` is a RESOLVED-ONLY edge (both endpoints are `CanonicalKey`s,
> ir/lib.rs:364-378); unresolved calls are DROPPED at ingest (no `CallObservation` analogue of the existing
> `ImportObservation`, ir/lib.rs:159-185). Therefore the producer has its OWN prerequisite: an IR/LiveGraph
> extension to carry call-resolution disposition + classification (+ a diagnostics aggregate + an enrichment
> analogue). The exact missing state and the build sequence are surfaced as `DECISION_REQUIRED` (§10). The
> crate-home is a genuine architecture-boundary call, also surfaced (§10, exhaustive matrix). The achievable
> partial design (§5–§6) is specified CONDITIONAL on those decisions; §7 states honestly what it does and does
> NOT achieve.

> **This REFINES (does not contradict) orient DR-1 / explain DR-E1.** Both named "A NEW PRODUCER … computes
> call-resolution + reliability axes from the LiveGraph (resolved/unresolved adjacency already in the IR/xref)"
> as the recommended option and sized it as "its own slice." This slice IS that sizing exercise, done
> first-hand — and its result is that the producer is NOT a thin projection but an IR-EXTENSION program. This
> is the same shape of refinement that EXPLAIN-SQLITE-FREE-1 applied to the DR-0 → S3 "explain is
> producer-light" hypothesis (refuted by first-hand reads). It is also exactly what TRUST-LIVEGRAPH-1
> §D-TRUST-2 anticipated when it DEFERRED "Option B (the full rebase): recompute the v1 reliability levels over
> the LiveGraph" — this slice confirms WHY that was deferred and sizes the prerequisite.

> **DECISION RESOLUTION — DR-TS-0-SEQUENCING (ratified by operator, 2026-06-13): S1 (extension-probe-first).**
> Before committing to the IR extension or the producer, run a cheap investigative probe
> (`SCIP-UNRESOLVED-CALL-PROBE-1`) answering MISSING-1 (does scip-typescript emit unresolved call occurrences?)
> + MISSING-2 (does the classifier reach count parity under SCIP semantics vs the SQLite `unresolved_edges`?).
> **GO** -> build the IR extension (DR-TS-1 A) -> the producer (crate home DR-TS-CRATE-HOME C) -> the
> per-command fastpaths. **NO-GO** -> hard evidence to reconsider Option A (DR-TS-0 S3). S2 (build-anyway)
> REJECTED (dead, always-RED code). DR-TS-1 / DR-TS-2 / DR-TS-CRATE-HOME stay OPEN, actionable only if the probe
> is GO. This spec stands as the authoritative producer-gap + extension-sizing map.

> **PROBE OUTCOME — the producer line is CLOSED (2026-06-13).** `SCIP-UNRESOLVED-CALL-PROBE-1` returned **NO-GO**
> (paired empirical evidence: scip-typescript emits NO unresolved-call occurrence; SCIP count 0 != homegrown
> `unresolved_edges` 3; structurally inverted). Operator ratified **DR-TS-0-POST-PROBE -> Option A**: keep the
> homegrown `unresolved_edges` SQLite-LABELLED; do NOT build the SCIP-sourced producer. **DR-TS-1 A is REFUTED;
> DR-TS-2 / DR-TS-CRATE-HOME are MOOT.** This spec is retained as the authoritative extension-sizing map the
> probe closed.

---

## 0. Spec-first note (read first)

This is a SPECIFICATION. It produces exactly one deliverable: this file. NO source path is touched; no IR field
is added; no cert is built; no default is flipped; `edges`/`unresolved_edges`/`extraction_diagnostics_json` are
not read by this slice (only the SHIPPED trust core reads them, which this doc audits). The eventual
implementation (TRUST-SUMMARY-LIVEGRAPH-IMPL-1, a LATER slice) is gated on the §10 decisions being ratified
first — and, per the VERDICT, on at least one NEW IR/LiveGraph extension landing before the producer can serve
a `edges`/`unresolved_edges`-free trust summary on green.

Per the repo split rule (CLAUDE.md: spec before impl; ratify architecture-boundary decisions before building),
this slice's DEFINITION OF DONE is the specification + the surfaced decisions + the explicit
FEASIBLE-vs-NEEDS-EXTENSION verdict — NOT a working producer.

### Revision note (iteration 1 — evidence corrections; VERDICT unchanged)

This revision corrects three evidence defects flagged in review. The `NEEDS-EXTENSION` VERDICT (§9) is UNCHANGED
after the corrections (re-checked field-by-field against the corrected evidence):

1. **The false/misleading grep claim is removed.** The iteration-0 draft wrote `grep "unresolved"
   …/repo-graph-scip-ingest/src → EMPTY ⇒ ingest records no unresolved call`. That empty result is a CASE
   artifact — every relevant token in scip-ingest is capitalized `Unresolved` (`StaticUnresolved` /
   `PackageUnresolved`), so a lowercase case-sensitive grep matches nothing. It is NOT evidence of absence.
   scip-ingest in fact carries rich unresolved-IMPORT handling. The airtight load-bearing fact is the
   IMPORT/CALL ASYMMETRY, now stated with verifiable greps: `ImportObservation` is pervasive in scip-ingest
   (lib.rs:508/901/918/934/1242); `CallObservation` exists NOWHERE (`rg CallObservation` over the IR +
   scip-ingest → no matches, exit 1). [Corrected in the §0 file list, §2b fact 3, §8, §9, §12.]
2. **The consumed contract is expanded** with the consumer CALL-SITES — the trust signals, `stale`, confidence,
   and the EXPLAIN_TRUST fields — at file:line (new §3a′).
3. **The livegraph dependency claim is corrected.** livegraph has FOUR deps (repo-graph-ir,
   repo-graph-trust-model, repo-graph-algorithms, repo-graph-import-resolver — `Cargo.toml` OBSERVED), all
   PURE/low-level — NOT "ir + trust-model ONLY." The crate-home conclusion is UNCHANGED (none of the four is the
   heavy `repo-graph-trust` policy crate; adding it would still be the first policy-dep). [Corrected in §7, §10.]

### Revision note (iteration 2 — evidence precision + cert-contract reconciliation; VERDICT unchanged)

This revision corrects the two defects flagged in the iteration-1 review. The `NEEDS-EXTENSION` VERDICT (§9) is
UNCHANGED — both are precision/contract defects, not scope or verdict changes (re-checked first-hand):

1. **The false "ONLY `unresolved` is `DegradationReason::UnresolvedAlias`" claim is replaced.** A first-hand
   `rg "unresolved" rust/crates/repo-graph-livegraph/src/lib.rs` (EXECUTED this revision) shows MULTIPLE
   unresolved surfaces, so "only `UnresolvedAlias`" was wrong. The corrected, precise statement: the LiveGraph
   has TWO distinct "unresolved" surfaces and NEITHER is the extractor unresolved-CALL disposition the trust
   summary needs — (1) `UnresolvedAlias` (lib.rs:132/623-655/698/718/874) is a CALLER/CALLEE CROSS-PARTITION
   RESIDENCY degradation over ALREADY-RESOLVED `IrEdge`s (a resolved callee whose defining partition is not
   resident), and (2) the IMPORT-observation completeness family (`observation_classes` / `live_import_view`,
   lib.rs:1602/1626/1675; `has_unresolved_after_overlay` / `_package` / `_dynamic` / `alias_unresolved` +
   `StaticUnresolved`, lib.rs:3141-3199) is IMPORT-side disposition. The load-bearing IMPORT/CALL ASYMMETRY is
   PRESERVED: rich unresolved-IMPORT state, no unresolved-CALL disposition. [Corrected in §0 file list, §2b
   fact 3, §12.]
2. **The §6b / DR-TS-2 contract contradiction is reconciled.** §6b now states explicitly that the FULL
   field-by-field no-loss cert is THIS slice's required contract (the only one under which a GREEN verdict makes
   the trust read fully `edges`/`unresolved_edges`-free), and that the SCOPED cert (DR-TS-2 option A) is a
   DIFFERENT, narrower contract that REDEFINES "no-loss," leaves the unmatchable fields SQLite-labelled (so does
   NOT by itself satisfy full SQLite-free parity), and remains BLOCKED pending DR-TS-2 ratification — not the
   same green-path cert. [Corrected in §6b; DR-TS-2 option A back-references it.]

### Evidence labels (repo Evidence Law; agent_docs/validation.md)

- **OBSERVED** = inspected first-hand THIS slice (a file read I performed this turn, cited file:line).
- **INFERRED** = my classification/judgment over OBSERVED facts (the feasibility verdicts, the producer design,
  the cert design, the verdict, the next-step recommendation).
- **EXECUTED** = a command I ran this turn with output observed.
- **NOT RUN** = skipped, with reason.

### Evidence basis (this audit)

```text
git: HEAD chain — ORIENT-SQLITE-FREE-1 = `e10a455` (DR-1), EXPLAIN-SQLITE-FREE-1 = `f3237f9` (DR-E1); the
coherence chain `6ed17b8..dc55114` (contract + amendment + 4 specs + 4 impls) sits below them; the trust hybrid
IMPL = `dc55114`. [OBSERVED via the packet + the precedent specs' evidence logs.]

Daemon: command set present (`rmap 0.2.1`, `which rmap` → /Users/apple/.local/bin/rmap) but NO `status`
subcommand and NO running daemon socket assumed; no index/refresh/dev-install run (state-mutating + out of scope
for a spec). [EXECUTED this turn: `rmap --version` → "rmap 0.2.1"; `rmap status` → "unknown command".] All
claims about the trust read surface + the IR feasibility are grounded in FIRST-HAND SOURCE reads of the trust
core, the storage adapter, the agent consumer, the IR crate, and the LiveGraph crate — the stronger evidence
basis for a claim about code structure than a live capture. Every OBSERVED claim carries file:line.

Files read first-hand THIS slice:
- rust/crates/trust/src/types.rs            (the full TrustReport / TrustSummary / TrustReliability / *Row DTOs)
- rust/crates/trust/src/service.rs          (compute_trust_report:210 — the 8 phases; assemble_trust_report:591
                                              — the 8 storage reads; compute_blast_radius_and_enrichment:468)
- rust/crates/trust/src/rules.rs            (the 4 downgrade detectors :77/:98/:132/:149 + the 4 reliability
                                              formulas :171/:204/:238/:275 + sum_unresolved_calls:314)
- rust/crates/trust/src/storage_port.rs     (the TrustStorageRead trait — the 8 reads; the classification DTOs
                                              re-exported from repo-graph-classification :33-35)
- rust/crates/storage/src/trust_impl.rs     (the SQLite edge/unresolved reads — :116/:149/:214/:265/:344/:353)
- rust/crates/agent/src/aggregators/trust.rs(aggregate:34 — the 3 signals + the returned AgentTrustSummary)
- rust/crates/agent/src/storage_port.rs     (AgentTrustSummary:317 + EnrichmentState:282 — the CONSUMED contract)
- rust/crates/storage/src/agent_impl.rs     (get_trust_summary:276 — how AgentTrustSummary projects the report)
- rust/crates/repo-graph-ir/src/lib.rs      (IrEdge:363, EdgeType:76, EdgeBasis:88, ImportObservation:163,
                                              ImportResolution:131, PartitionIr:382, incoming/outgoing:412-419,
                                              SymbolAttributes:320 — the LOAD-BEARING feasibility surface)
- rust/crates/repo-graph-scip-ingest/src/lib.rs (ImportObservation + ImportResolution::StaticUnresolved/
                                              PackageUnresolved PRESENT — lib.rs:508/901/918/934/1242; NO
                                              `CallObservation` — `rg CallObservation` over IR+ingest = no match)
- rust/crates/repo-graph-livegraph/src/lib.rs (callers:469, callees:586, module_import_cycles:1317,
                                              module_stats:1376 + ModuleStatRow:2041, live_import_view:1675;
                                              TWO distinct "unresolved" surfaces, NEITHER the extractor
                                              unresolved-CALL disposition: (1) DegradationReason::UnresolvedAlias
                                              (lib.rs:132/623-655/698/718/874) — caller/callee CROSS-PARTITION
                                              RESIDENCY over already-resolved IrEdges; (2) the IMPORT-observation
                                              completeness family (observation_classes/live_import_view,
                                              lib.rs:1602/1626/1675/3141-3199))
- rust/crates/repo-graph-coherence/src/lib.rs (the CoherenceEnvelope wrapper + its deps — crate-home candidate)
- rust/crates/daemon-runtime/src/livegraph_feed.rs (ImportNoLossCert:1591, import_cert_fingerprint:1612 — the
                                              cert pattern to mirror) + state.rs:205-227 (the cert RwLock slots)
- docs/slices/orient-sqlite-free-1.md (DR-1), explain-sqlite-free-1.md (DR-E1), trust-livegraph-1.md (the
  hybrid + the §D-TRUST-2 anti-Option-B guard), stats-livegraph-1.md (the cert-fastpath + IR-extension split
  precedent), coherence-layer-1.md (the CoherenceEnvelope contract); agent_docs/architecture.md (layer stack +
  build order + dep rule).
```

---

## 1. Why now (priority path)

OBSERVED [ORIENT-SQLITE-FREE-1 §8 DR-1; EXPLAIN-SQLITE-FREE-1 §8 DR-E1 + DR-0 → S1]: the operator ratified
(2026-06-13) the re-scoping of Option B as **"a single shared `TRUST-SUMMARY-LIVEGRAPH-1` producer FIRST, the
per-command fastpaths second."** orient DR-1 and explain DR-E1 are the SAME decision by the SAME source: the
trust aggregator reads `edges` + `unresolved_edges` UNCONDITIONALLY in every focus/pipeline (trust_impl.rs:116/
149/214/265/344/353), feeds BOTH the always-emitted trust signals AND the envelope confidence, and has NO
LiveGraph producer. Building this one producer is the only path to a GREEN composite cert for orient AND explain
AND a true current-state half for trust. This slice SPECS that producer.

OBSERVED [agent/src/aggregators/trust.rs:34-82; storage/src/agent_impl.rs:276-362; trust/src/service.rs:591-705]:
`rmap trust` and the orient/explain/check coherence envelopes ALL flow through ONE source — `get_trust_summary`
→ `assemble_trust_report` → `compute_trust_report` — which is 100% SQLite + Authority today, with ZERO LiveGraph
contribution. That single source is the shared blocker.

VISION alignment: VISION "Operational Architecture" — current repo state in memory is primary truth; SQLite is
the transition mechanism. The trust summary is the LAST coherence input with no current-state path. VISION "Fact
Certainty Model" — Layer 1 trust must be honest about what is current-state fact vs an outgoing-extractor
snapshot artifact; this producer (if feasible) would let the cluster serve a current-state trust summary, and
(if NOT feasible without extension) the honest answer is to say so and keep the labelled SQLite source.

---

## 2. THE LOAD-BEARING QUESTION, answered first (the IR feasibility) — OBSERVED, decisive

> **Q: Does the LiveGraph current state carry enough to compute the trust summary WITHOUT reading
> `edges`/`unresolved_edges`? Specifically, does the IR track, per current state, (a) call-edge adjacency, AND
> (b) the RESOLVED vs UNRESOLVED disposition of each call target?**

### 2a. (a) Call-edge adjacency — **YES** [OBSERVED]

`PartitionIr` carries `edges: Vec<IrEdge>` (ir/lib.rs:387); `IrEdge { src, dst, edge_type, basis, provenance,
import }` (ir/lib.rs:363-378); `EdgeType::Calls` (ir/lib.rs:79); `PartitionIr::incoming`/`outgoing` (ir/lib.rs:
412-419, "the basis for `callers`"). The LiveGraph surfaces this as `callers` (livegraph lib.rs:469) and
`callees` (lib.rs:586). **Resolved CALL adjacency is present and already migrated.**

### 2b. (b) Resolved-vs-UNRESOLVED disposition of each call target — **NO** [OBSERVED — the decisive gap]

Three first-hand facts establish the gap:

1. **`IrEdge` is RESOLVED-ONLY by construction.** `src: CanonicalKey` AND `dst: CanonicalKey` (ir/lib.rs:366/
   368) — an edge EXISTS only when BOTH endpoints resolved to a canonical node identity. A call whose target the
   extractor saw syntactically but could NOT bind to a definition (exactly what `unresolved_edges` records) has
   no `dst` key, so it CANNOT be an `IrEdge`. [OBSERVED ir/lib.rs:363-378]
2. **There is NO unresolved-CALL observation.** The IR carries `import_observations: Vec<ImportObservation>`
   (ir/lib.rs:392) — completeness evidence for imports that did NOT become edges (`ImportResolution::
   StaticUnresolved` / `PackageExternal` / `DynamicUnsupported`, ir/lib.rs:131-143). This is an IMPORT-side
   disposition record. **There is NO `CallObservation` analogue** — nothing records an unresolved call-site or
   its disposition. [OBSERVED ir/lib.rs:159-185, 380-393 — the only observation vector is `import_observations`]
3. **The SCIP-ingest records unresolved IMPORTS but NO unresolved CALL — a grep-provable asymmetry.** [OBSERVED:
   `rg "ImportObservation" rust/crates/repo-graph-scip-ingest/src` → many matches (lib.rs:20/25/508/896/901/934/
   1242/1511/1512); `rg "ImportResolution::StaticUnresolved|PackageUnresolved" …/scip-ingest/src` → lib.rs:918/
   1242; `rg "CallObservation" rust/crates/repo-graph-ir/src rust/crates/repo-graph-scip-ingest/src` → NO
   matches (exit 1).] scip-ingest builds durable `IrImportObservation`s carrying
   `ImportResolution::StaticUnresolved` / `PackageUnresolved` (lib.rs:901/918/934/1242) — an IMPORT-side
   disposition record — but for CALLS it either resolves the call into an `IrEdge` or DROPS it: there is no
   `CallObservation` and no "unresolved-but-recorded" call intermediate. (A case-sensitive `grep -rn
   "unresolved" …/scip-ingest/src/` returns empty, but ONLY because every token is capitalized `Unresolved`;
   that empty result is a CASE artifact, NOT evidence of absence — the IMPORT disposition is present; the CALL
   disposition is what is absent.) On the LiveGraph side there are TWO distinct
   "unresolved" surfaces, and BOTH are import/residency state — NEITHER is the extractor CALL disposition:
   (1) `DegradationReason::UnresolvedAlias` (livegraph lib.rs:132/311/483/596/655/698/718/874) is a
   CALLER/CALLEE CROSS-PARTITION RESIDENCY degradation — `callees` raises it (lib.rs:643-655) when an
   ALREADY-RESOLVED `IrEdge`'s callee `dst` has "no known defining partition" resident (a residency gap on a
   RESOLVED target, NOT an unresolved call-target); (2) the IMPORT-observation completeness family —
   `observation_classes` / `live_import_view` (lib.rs:1602/1626/1675) with `has_unresolved_after_overlay` /
   `has_unresolved_package` / `has_dynamic_unresolved` / `has_alias_unresolved` + `StaticUnresolved`
   (lib.rs:3141/3153/3177/3182/3199) — is IMPORT-side disposition derived from `import_observations` (the
   IMPORT analogue the IR has and CALLS lack). NEITHER surface records the resolved/unresolved disposition of a
   CALL target (the `unresolved_edges` analogue). [OBSERVED livegraph lib.rs:132/623-655/698/718/874 +
   1602/1626/1675/3141-3199 — first-hand this revision; corrects the iteration-1 "only `UnresolvedAlias`" claim.]

### 2c. Three FURTHER gaps the trust summary needs beyond (b) — OBSERVED

Even granting a future (b), the trust summary consumes THREE more things the LiveGraph/IR does not carry:

- **(c) The unresolved-edge CLASSIFICATION vocabulary.** `unresolved_calls_external`, `classifications[]`, and
  the blast-radius/enrichment all key on `UnresolvedEdgeClassification` (ExternalLibraryCandidate / Unknown /
  InternalCandidate), `UnresolvedEdgeCategory` (the 4 CALLS-family categories + imports/instantiates/implements),
  and `UnresolvedEdgeBasisCode` — a `repo-graph-classification` vocabulary applied at SQLite-extraction time and
  stored on each `unresolved_edges` row (trust storage_port.rs:33-35; trust_impl.rs:149/214). The SCIP-ingest
  path NEVER runs this classifier; the IR has no classification field on any call. [OBSERVED]
- **(d) The `extraction_diagnostics_json` aggregate.** `edges_total`, `unresolved_total`, the
  `unresolved_breakdown` (per-category counts), and `unresolved_calls` (`sum_unresolved_calls`, service.rs:247-
  251/314) ALL come from the diagnostics BLOB — a precomputed snapshot artifact produced by the OUTGOING
  extractor and read via `get_snapshot_extraction_diagnostics` (service.rs:600). The LiveGraph/SCIP pipeline has
  NO analogue; it does not produce this blob. [OBSERVED service.rs:599-614, 415-429; trust_impl.rs:90-107]
- **(e) The enrichment metadata.** `enrichment_status` / `enrichment_state` / `enrichment_eligible` /
  `enrichment_enriched` derive from `metadata_json` enrichment markers (`receiverType` / `typeDisplayName` /
  `isExternalType`) on `calls_obj_method_needs_type_info` unresolved edges — a TYPE-INFERENCE enrichment-phase
  artifact (service.rs:507-533). The SCIP/LiveGraph pipeline has NO enrichment phase. This is the DEEPEST gap.
  [OBSERVED service.rs:468-577]

### 2d. The asymmetry that proves the point (INFERRED over OBSERVED)

The IR designers DID build a disposition record for IMPORTS (`ImportObservation` with `StaticUnresolved` /
`PackageExternal` / `DynamicUnsupported` + `external_node_modules`, ir/lib.rs:159-185) — so unresolved IMPORTS
are recoverable from the IR. They did NOT build the CALL analogue. The trust summary's HEAVIEST inputs are on
the CALL side (`call_resolution_rate`, `call_graph_reliability`, the CALLS-family classifications, the unknown
CALLS blast radius, the obj-method enrichment). The exact disposition the trust summary needs is the exact one
the IR omits. [OBSERVED the import/call asymmetry; INFERRED its consequence for trust.]

### 2e. ANSWER

**NO.** The LiveGraph has adjacency but NOT the resolved/unresolved CALL disposition (b), NOR the classification
(c), NOR the diagnostics aggregate (d), NOR the enrichment inputs (e). Per the packet's branch: *"If NO … the
producer has its OWN prerequisite (an IR/LiveGraph extension to carry that state). STOP and emit
DECISION_REQUIRED naming the exact missing state. VERDICT: NEEDS-EXTENSION."* The STOP is taken (§10).

---

## 3. The CONSUMED trust-summary contract (the producer's OUTPUT contract) — OBSERVED, first-hand

The producer must reproduce what consumers read. There are TWO consumer surfaces.

### 3a. `AgentTrustSummary` — what orient / explain / check consume (the decisive blocker)

[OBSERVED agent/src/storage_port.rs:317-326; built by storage/src/agent_impl.rs:276-362 via `assemble_trust_report`
then projected; consumed by agent/src/aggregators/trust.rs:34-82.] This narrow projection is the orient DR-1 /
explain DR-E1 blocker. Every field + its ultimate SQLite source + LiveGraph feasibility:

| `AgentTrustSummary` field | Built from (report field) | Ultimate SQLite source | LiveGraph feasibility |
|---|---|---|---|
| `resolved_calls` | `summary.resolved_calls` | `count_edges_by_type(CALLS)` (`edges`) — trust_impl.rs:116 | **LG-derivable** — count IR `EdgeType::Calls` edges (adjacency, §2a). |
| `unresolved_calls` | `summary.unresolved_calls` | `sum_unresolved_calls(diagnostics)` — the BLOB (d) | **NEEDS-EXTENSION** (d). |
| `call_resolution_rate` | `summary.call_resolution_rate` | `resolved / (resolved + internal_like)`; `internal_like = unresolved_calls(d) − external(c)` | **NEEDS-EXTENSION** (b/c/d). The field `TRUST_LOW_RESOLUTION` keys on (trust.rs:47). |
| `call_graph_reliability` | `summary.reliability.call_graph` | `compute_call_graph_reliability(resolved, internal_like)` — rules.rs:204 | **NEEDS-EXTENSION** (depends on internal_like). |
| `dead_code_reliability` | `summary.reliability.dead_code` | `compute_dead_code_reliability(missing_entrypoints, registry, framework, call_graph_level)` — rules.rs:238 | **NEEDS-EXTENSION** (inherits call_graph; the DEAD_CODE-gate authority, storage_port.rs:300-307). |
| `enrichment_state` | `enrichment_status` + `enrichment_eligible_count` | `unresolved_edges.metadata_json` enrichment markers (e) | **NEEDS-EXTENSION** (e). `TRUST_NO_ENRICHMENT` keys on it (trust.rs:67). |
| `enrichment_eligible` / `enrichment_enriched` | `enrichment_status` | same (e) | **NEEDS-EXTENSION** (e). |

CONSEQUENCE [INFERRED]: of the 8 `AgentTrustSummary` fields, exactly ONE (`resolved_calls`) is LG-derivable
today; the other 7 are NEEDS-EXTENSION. The narrow summary orient/explain/check consume is NOT servable from the
current LiveGraph.

### 3a′. The consumer CALL-SITES — stale + confidence + signals + EXPLAIN_TRUST (first-hand, file:line)

The §3a table lists the summary's FIELDS; this subsection pins the CALL-SITES that read them and the parallel
`get_stale_files` read, so the producer's behavioral contract (which consumed value drives which output) is
explicit. [OBSERVED — first-hand reads this turn of the four consumer files.]

| Consumer (file:line) | Reads | Consumed trust-summary fields | Also reads (non-summary) |
|---|---|---|---|
| Trust aggregator `aggregate` (agent/src/aggregators/trust.rs:34-82) | `get_trust_summary` :39; `get_stale_files` :40 | `resolved_calls`+`unresolved_calls` (the `total_calls>0` gate :46); `call_resolution_rate` (TRUST_LOW_RESOLUTION `<0.20` :47-53); `enrichment_state`/`enrichment_eligible`/`enrichment_enriched` (TRUST_NO_ENRICHMENT iff `NotRun` :67-72) | `stale` from `get_stale_files` (TRUST_STALE_SNAPSHOT :56-61). Returns `summary`+`stale` onward (:74-81). |
| `derive_repo_confidence` (agent/src/confidence.rs:43-69) | — (takes `&AgentTrustSummary` + `stale`) | `call_resolution_rate` (Low `<0.20` :46 / Medium `≤0.50` :50); `enrichment_state` (`NotRun`→Medium :63, else High) | `stale` (→Medium :56). Does NOT read the reliability axes. |
| orient envelope | the trust aggregator + `derive_repo_confidence` above | (as the aggregator) — the aggregator doc (trust.rs:12-16) returns `summary`+`stale` "because the orient pipeline also needs them for confidence derivation and to gate the dead-code aggregator" | (as above) |
| explain (agent/src/explain/mod.rs) | `get_trust_summary` :343; `get_stale_files` :441 | via EXPLAIN_TRUST (:421-422 → `build_trust_signal`) + `derive_repo_confidence` :442 | `stale` :441 |
| EXPLAIN_TRUST fields (`build_trust_signal`, agent/src/explain/mod.rs:777-793) | — | `call_resolution_rate` :780; `call_graph_reliability.level` :781-785; `enrichment_state` :787-791. `dead_code_reliability` EXPLICITLY WITHDRAWN (:786 "Surface withdrawn"). | — |
| check (agent/src/check/mod.rs:82-102) | `get_stale_files` :83; `get_trust_summary` :86 | `call_resolution_rate` (via `derive_repo_confidence` :93); `call_graph_reliability.level` (`CheckInput` :99); `enrichment_state` (`CheckInput` :100) | `stale_files.len()` (`CheckInput` :98) |

TWO contract facts this enumeration pins:

1. **`stale` is NOT an `edges`/`unresolved_edges` read** [OBSERVED]. Every consumer derives `stale` from
   `get_stale_files` (a file-staleness read), NOT from the trust summary or the graph tables (trust.rs:40,
   confidence.rs:13-14, explain/mod.rs:441, check/mod.rs:83). So the stale input is ORTHOGONAL to this
   decommission — it neither blocks nor is blocked by the `edges`/`unresolved_edges` rebase, and the producer
   does not need to reproduce it.
2. **The consumed fields that DO derive from `edges`/`unresolved_edges`/diagnostics/enrichment are exactly the
   ones that gate confidence + signals + EXPLAIN_TRUST** [OBSERVED call-sites; INFERRED consequence]:
   `call_resolution_rate` (confidence Low/Medium; TRUST_LOW_RESOLUTION; EXPLAIN_TRUST), `unresolved_calls` (the
   aggregator's `total_calls` gate), `enrichment_state` (confidence; TRUST_NO_ENRICHMENT; EXPLAIN_TRUST; check),
   `call_graph_reliability` (EXPLAIN_TRUST; check). These are precisely §3a's NEEDS-EXTENSION fields (b/c/d/e).
   Only `resolved_calls` (the numerator) and `stale` are non-blocking. CONSEQUENCE: the producer cannot serve a
   current-state confidence / signal / EXPLAIN_TRUST without the §10 extension — the consumed values that drive
   those three outputs are exactly the blocked ones. This is the behavioral form of the §3a verdict.

### 3b. The full `TrustReport` — what `rmap trust` Half B consumes (TRUST-LIVEGRAPH-1) — OBSERVED

[OBSERVED trust/src/types.rs:149-300; computed by service.rs:210-457; the 8 reads at service.rs:599-686.] The
full report is the producer's MAXIMAL output contract (the trust command's Half B + its hybrid). Grouped by
LiveGraph feasibility:

| Report field group | SQLite source (file:line) | LiveGraph feasibility |
|---|---|---|
| `summary.resolved_calls` | `count_edges_by_type(CALLS)` trust_impl.rs:116 | **LG-derivable** (count IR Calls edges). |
| `modules[]` (fan_in/fan_out/file_count/stable_key/qualified_name) + `suspicious_zero_connectivity` | `compute_module_stats` trust_impl.rs:313-383 (`edges` IMPORTS + `module_candidates`) | **LG-derivable-WITH-DIVERGENCE** — `module_stats` (livegraph lib.rs:1376) gives fan_in/fan_out/file_count, but the SQLite model is anchored to `module_candidates` semantic modules + synthesizes `repo_uid:path:MODULE` keys; the LiveGraph uses dirname aggregation → identities differ (RISK-E / TRUST-LIVEGRAPH-1 RISK-T-H). NOT byte-equal without reconciliation. |
| `triggered_downgrades.registry_pattern_suspicion` | `find_path_prefix_module_cycles` trust_impl.rs:248-311 (`edges` IMPORTS) | **LG-derivable** — `module_import_cycles` (livegraph lib.rs:1317) + a path-prefix post-filter. |
| `triggered_downgrades.alias_resolution_suspicion` | derived from `compute_module_stats` suspicious count (rules.rs:149) | **LG-derivable** (from the module_stats above). |
| `triggered_downgrades.framework_heavy_suspicion` | `get_file_paths_by_repo` (file list, NOT edges) rules.rs:77 | **LG-derivable-in-principle** — IR FILE-node paths could supply the list; it is a file-inventory scan, not a graph fact. |
| `triggered_downgrades.missing_entrypoint_declarations` | `count_active_declarations(entrypoint)` (`declarations` = Authority) | **Authority — stays SQLite** (no LiveGraph home; not an `edges`/`unresolved_edges` read; does not block). |
| `summary.unresolved_calls` / `unresolved_calls_internal_like` / `call_resolution_rate` | BLOB (d) + classification (c) | **NEEDS-EXTENSION** (b/c/d). |
| `summary.unresolved_calls_external` | `count_unresolved_edges_by_classification(calls-filter, ExternalLibraryCandidate)` (`unresolved_edges`) trust_impl.rs:149 | **NEEDS-EXTENSION** (c). |
| `summary.reliability.{call_graph, import_graph, change_impact}` | rules.rs:171/204/275 over the above | **NEEDS-EXTENSION** (call_graph via internal_like; import_graph via `unresolved_imports` from the BLOB — partially recoverable from `import_observations`, §2d; change_impact inherits import_graph). |
| `summary.{edges_total, edges_resolved, unresolved_total}` + `diagnostics_version` + `diagnostics_available` | the BLOB (d) | **NEEDS-EXTENSION** (d). |
| `categories[]` | the BLOB `unresolved_breakdown` (d) | **NEEDS-EXTENSION** (d). |
| `classifications[]` | `count_unresolved_edges_by_classification(all)` (`unresolved_edges`) trust_impl.rs:149 | **NEEDS-EXTENSION** (c). |
| `unknown_calls_blast_radius` | `query_unresolved_edges(unknown,100k)` + `derive_blast_radius` (`unresolved_edges`) trust_impl.rs:214 | **NEEDS-EXTENSION** (c, per-row category/basis/visibility). |
| `enrichment_status` | `unresolved_edges.metadata_json` enrichment markers (e) | **NEEDS-EXTENSION** (e). |
| `caveats[]` | derived from the reliability levels (service.rs:155-199) | **NEEDS-EXTENSION** (inherits the reliability inputs). |
| `snapshot_uid` / `basis_commit` / `toolchain` / `display_name` | `get_latest_snapshot` + daemon (operational) | **Not-a-decommission-target** — operational identity; stays SQLite. |

CONSEQUENCE [INFERRED]: the LG-derivable subset is `{resolved_calls, modules[] (with divergence), registry/alias
downgrades, framework downgrade}`. The reliability axes, the resolution rate, the classifications, the
categories, the blast radius, the enrichment — the SUBSTANCE of the trust summary — are all NEEDS-EXTENSION.

---

## 4. Field-by-field LiveGraph/IR feasibility analysis (the core) — INFERRED over §2/§3 OBSERVED facts

Three feasibility classes, mirroring the orient/explain CLASS 1/2/3 taxonomy but keyed to the trust summary:

```text
CLASS T1 — LG-DERIVABLE NOW (a LiveGraph surface exists; a no-loss compare is buildable):
  · resolved_calls                       — count IR EdgeType::Calls edges (callers/callees adjacency, lib.rs:469/586)
  · registry_pattern_suspicion           — module_import_cycles (lib.rs:1317) + path-prefix post-filter
  · framework_heavy_suspicion            — IR FILE-node path list (file-inventory scan, not a graph fact)

CLASS T2 — LG-DERIVABLE WITH RECONCILIATION (a surface exists but identities/semantics diverge → cert may be RED
            by construction without reconciliation work):
  · modules[] + fan_in/fan_out/file_count + suspicious_zero_connectivity + alias_resolution_suspicion
      — module_stats (lib.rs:1376) exists, but the SQLite model is module_candidates-anchored with synthesized
        `repo_uid:path:MODULE` keys vs the LiveGraph dirname model (RISK-E / RISK-T-H). The stats slice proved
        the COUNT is no-loss; the trust FRAMING (stable_key identity, suspicious-zero-connectivity, trust_notes)
        is NOT proven no-loss and likely diverges.

CLASS T3 — NEEDS-EXTENSION (no IR/LiveGraph state exists; a new producer/extraction is required):
  · unresolved_calls, unresolved_calls_external, unresolved_calls_internal_like, call_resolution_rate
      — needs (b) unresolved-call observations + (c) classification + (d) the diagnostics aggregate
  · call_graph_reliability, change_impact_reliability, (import_graph_reliability — partially via import_observations)
      — computed FROM the above; inputs not reproducible ⇒ axes not reproducible
  · classifications[], categories[]                      — needs (c) + (d)
  · unknown_calls_blast_radius                           — needs (c) per-row category/basis/visibility
  · enrichment_status / enrichment_state / counts        — needs (e) the enrichment-phase metadata (DEEPEST gap)
  · edges_total / edges_resolved / unresolved_total      — needs (d) the diagnostics aggregate
```

The exact missing IR/LiveGraph state, named precisely (the §10 DR-TS-1 payload):

```text
MISSING-1  An unresolved-CALL observation in the IR — a `CallObservation` analogue of the existing
           `ImportObservation` (ir/lib.rs:163): a record of a call-site whose target did not resolve to a
           CanonicalKey, carrying enough to classify it. REQUIRES the SCIP-ingest to EMIT such observations —
           an OPEN PROBE (does scip-typescript surface unresolved call occurrences at all? — a CJOIN-PROVE-style
           probe, not assumable). Without MISSING-1 there is no denominator for call resolution.
MISSING-2  The classification of each MISSING-1 observation into the UnresolvedEdgeClassification /
           UnresolvedEdgeCategory / UnresolvedEdgeBasisCode vocabulary — a classifier pass over the LiveGraph's
           observations. Semantics question (RISK-T-D): "unresolved edge" means something DIFFERENT under SCIP
           (SCIP resolves cross-file/cross-package that the homegrown extractor left unresolved), so the SAME
           classifier may produce DIFFERENT counts → a no-loss cert may be RED by construction (§6c).
MISSING-3  A diagnostics-aggregate analogue (edges_total / unresolved_total / unresolved_breakdown) — derivable
           from MISSING-1 + MISSING-2 IF they exist, but it is a NEW aggregate the LiveGraph does not compute.
MISSING-4  An enrichment-phase analogue (type-inference markers on obj-method unresolved calls) — the DEEPEST
           gap; there is no enrichment phase in the SCIP/LiveGraph pipeline. May be UNACHIEVABLE without a new
           type-inference pass, or must be conceded (enrichment_state = a degraded/Unknown posture on green).
```

---

## 5. The LiveGraph-native computation design (for the DERIVABLE subset; honest about the gaps) — INFERRED

For the CLASS T1 (and, with reconciliation, T2) subset, the producer is a projection over existing LiveGraph
surfaces. For CLASS T3 it is BLOCKED on the §10 extension. The design, conditional on §10:

```text
TrustSummaryLiveGraphProducer (a NEW support module; crate home = DR-TS-CRATE-HOME §10):
  INPUT:  the resident LiveGraph (IR partitions) + the Authority reads that stay SQLite (entrypoint count).
  OUTPUT: a value SHAPED like AgentTrustSummary (and, maximally, like TrustReport) so the no-loss compare is
          field-aligned with the SQLite producer.

  CLASS T1 (servable now):
    resolved_calls          := count of IR edges with edge_type == Calls across resident partitions.
    registry_pattern        := detect_registry_pattern_suspicion over module_import_cycles path-prefix groups
                               (REUSE rules.rs:98 — do NOT re-derive the thresholds; §DR-TS-CRATE-HOME governs
                               how the producer reaches rules.rs without a dep inversion).
    framework_heavy         := detect_framework_heavy_suspicion over the IR FILE-node path list (REUSE rules.rs:77).

  CLASS T2 (servable with reconciliation; cert likely RED until reconciled):
    modules[]               := project module_stats (lib.rs:1376) into ModuleTrustRow, reconciling the
                               module-identity model (module_candidates-anchored stable_key vs dirname) — a
                               reconciliation this slice does NOT design (RISK-E; its own work). suspicious +
                               alias_resolution := the rules.rs:149 detector over the reconciled rows.

  CLASS T3 (BLOCKED — needs §10):
    unresolved_calls / external / internal_like / call_resolution_rate
                            := REQUIRES MISSING-1 + MISSING-2 + MISSING-3. Until then, UNAVAILABLE on green.
    reliability axes        := REUSE rules.rs:171/204/275 over the (blocked) inputs — the formulas are reusable;
                               the INPUTS are not yet producible.
    classifications/categories/blast_radius
                            := REQUIRES MISSING-1 + MISSING-2 (+ derive_blast_radius, reusable).
    enrichment_*            := REQUIRES MISSING-4.

  REUSE-NOT-REINVENT: the producer MUST call the EXISTING rules.rs formulas + derive_blast_radius (one source of
    truth for trust thresholds — a trust fact must not have two divergent threshold implementations). The
    producer supplies LiveGraph-sourced INPUTS to the same pure functions. This is the DIP boundary the
    crate-home decision (§10) governs.
```

---

## 6. The no-loss cert design (mirroring imports/cycles/stats) — INFERRED + the structural limits

### 6a. Why mirror the drilldown certs

OBSERVED [livegraph_feed.rs:1591-1637; state.rs:205-227]: the shipped pattern is a per-answer in-memory
`*NoLossCert { verdict, fingerprint }` on `RepoState` (S1, rebuilt on restart), keyed by the SQLite-free
`import_cert_fingerprint` (partition epoch/fresh/ts/source_inputs_hash/producer_fingerprint ⊕ snapshot_uid ⊕
policy version). GREEN at the current fingerprint ⇒ serve LiveGraph WITHOUT SQLite; else build-once / fallback.

### 6b. The TRUST-SUMMARY no-loss cert (the design)

```text
TrustSummaryNoLossCert { verdict: GREEN|RED, fingerprint }  — on RepoState, RwLock<Option<…>>, S1, keyed by the
  SHARED import_cert_fingerprint (NO new invalidation key — reuse the existing one + the trust policy version).
  Built once per fingerprint; the SQLite trust read survives ONLY (i) to BUILD the cert and (ii) on fallback.

GREEN (the FULL no-loss target — the contract this slice's GOAL names; see the FULL-vs-SCOPED note below) iff,
  at the current fingerprint, the LiveGraph-assembled trust summary is no-loss-EQUAL to the SQLite
  `assemble_trust_report` summary, FIELD BY FIELD (resolved_calls, unresolved_calls, call_resolution_rate, the
  reliability axes, classifications, categories, blast radius, enrichment, modules) — a single divergent field
  ⇒ RED ⇒ SQLite fallback. AND precondition: every contributing partition resident + Fresh + TS-primary
  (non-TS ⇒ precondition unmet). It is the AND-fold the coherence root already uses (MEET discipline).

It is the gate the orient/explain/trust fastpaths AND-fold into their composite cert (orient §4b contributor
(5); explain §4b contributor (6)). This cert is the missing contributor that makes those composites GREEN-able.
```

**FULL vs SCOPED — two DIFFERENT cert contracts; this slice's target is the FULL one** [INFERRED,
contract-precision — resolves the §6b ↔ DR-TS-2 relationship the iteration-1 review flagged]:

- The cert specified ABOVE is the FULL no-loss cert: GREEN requires EVERY summary field to be value-equal. This
  is the contract this slice's GOAL names — "a no-loss cert proving the LiveGraph-derived summary EQUALS the
  SQLite-derived one" — and the ONLY contract under which a GREEN verdict makes the trust read fully
  `edges`/`unresolved_edges`-FREE, because on green NO field is served from SQLite.
- DR-TS-2 (§10) surfaces a DIFFERENT, NARROWER contract: a SCOPED cert (DR-TS-2 option A) that compares ONLY the
  matchable fields and serves the unmatchable ones (the diagnostics-blob counts, module identity, enrichment;
  §6c) from SQLite + LABELLED. That is NOT this cert with some fields tolerated — it REDEFINES "no-loss" to
  "no-loss over a declared matchable SUBSET." Its consequences differ in kind: (i) the trust read STAYS a
  partial `edges`/`unresolved_edges` touch for the labelled fields, so it does NOT by itself satisfy full
  SQLite-free parity for the trust contributor; (ii) the fastpaths could AND-fold only a SUBSET verdict and must
  surface the labelled-SQLite fields honestly (Half-B-style).
- THEREFORE the scoped cert is NOT the same green-path cert as §6b and is NOT adopted by this spec. It is a
  governance ALTERNATIVE that changes the cert's GREEN definition and what the fastpaths may claim, and it
  remains BLOCKED pending DR-TS-2 ratification. Until DR-TS-2 is decided, the cert contract this slice specifies
  is the FULL one above; the scoped variant is a labelled fork, not a drop-in.

### 6c. The STRUCTURAL limits (the honesty the cert must encode) — INFERRED, load-bearing

```text
INERT-UNTIL-EXTENSION: §4 CLASS T3 contributors do NOT exist. Until MISSING-1..4 land, the field-by-field
  compare can never be all-GREEN (the T3 fields have no LiveGraph value to compare), so the cert is RED by
  construction and every trust read falls back. Same shape as stats-livegraph (cert designed; IR-SYMBOL-
  ATTRIBUTES-1 had to land first) — but the trust extension is LARGER (4 missing pieces, one a probe, one
  possibly unachievable).
RED-EVEN-AFTER-EXTENSION (the deeper point): for THREE field groups, byte/value equality may be UNACHIEVABLE
  even WITH the extension, because the two producers MEASURE DIFFERENT THINGS:
    (i)  the diagnostics-blob counts (edges_total/unresolved_total) are the OUTGOING extractor's tallies; a
         LiveGraph recomputation over SCIP resolution yields DIFFERENT numbers (RISK-T-D — SCIP resolves what
         the homegrown extractor left unresolved). A no-loss cert on these is RED by construction.
    (ii) modules[] stable_key identity diverges (module_candidates vs dirname; RISK-E / RISK-T-H).
    (iii)enrichment is an outgoing-extractor type-inference artifact with no SCIP analogue (MISSING-4).
  So the cert design must DISTINGUISH "RED because not-yet-built" from "RED because the fact is not the same
  fact" — and the latter is a governance question (do we redefine the trust summary's current-state meaning, or
  keep these fields permanently SQLite-labelled?). This is surfaced in DR-TS-1 / DR-TS-2.
```

---

## 7. What this does and does NOT achieve (honesty — per readiness-9 discipline) — INFERRED

```text
DOES (when DR-TS-1..3 are ratified + the IR extension lands):
  + Gives the coherence cluster a current-state trust summary with NO `edges`/`unresolved_edges` read on the
    GREEN path — the shared contributor orient §4b(5) / explain §4b(6) need to make their composite certs
    GREEN-able. ONE producer unblocks orient + explain + strengthens trust's Half A.
  + REUSES the existing trust rules.rs formulas + derive_blast_radius (one threshold source of truth) and the
    shipped *NoLossCert pattern (no new invalidation key).

DOES NOT (the boundaries the evidence demands be stated):
  - Does NOT exist as a clean projection. It is GATED on an IR/LiveGraph EXTENSION (the VERDICT). It cannot ship
    a working green producer without MISSING-1..4 (or a ratified concession on the unachievable fields).
  - Is TS-only. LiveGraph is TS-only — its FOUR deps (repo-graph-ir + repo-graph-trust-model +
    repo-graph-algorithms + repo-graph-import-resolver; `Cargo.toml` OBSERVED) are all PURE/low-level; NONE is
    the heavy `repo-graph-trust` policy crate. Non-TS
    repos (C/C++/Rust/Java) fall back to the full SQLite trust read ALWAYS. `edges`/`unresolved_edges` stay
    load-bearing for them (deletion gate 2, the structural ceiling).
  - Does NOT by itself retire `edges`/`unresolved_edges`/`extraction_diagnostics_json`. The fastpaths that
    consume it, the non-TS fallback, the cert-BUILD read, and the 31 non-graph tables remain (readiness-9 gates).
  - Does NOT make `rmap trust` SQLite-free. The TRUST-LIVEGRAPH-1 hybrid Half B (the labelled outgoing-extractor
    diagnostics) is RETAINED verbatim; the Authority entrypoint read stays SQLite. This producer feeds a GREEN
    current-state SUMMARY; it does not delete the residual diagnostic report.
  - For the diagnostics-blob counts, module identity, and enrichment, value-parity may be UNACHIEVABLE (§6c) —
    a separate governance call from "build the extension."
```

---

## 8. Validation plan (for the eventual IMPL; mirrors the drilldown proofs) — NOT RUN (spec-first)

```text
PARITY (green compare):  on a TS pilot where the cert is GREEN → the LiveGraph-assembled trust summary is
  value-equal to the SQLite `assemble_trust_report` summary, field by field (resolved_calls, unresolved_calls,
  call_resolution_rate, the reliability axes, classifications, categories, blast radius, enrichment, modules). A
  single divergent field ⇒ RED ⇒ fallback (no silent mismatch). [EXECUTED proof required at IMPL.]
NO-`edges`/`unresolved_edges`-READ PROOF: a storage spy / panicking closure on count_edges_by_type /
  count_unresolved_edges_by_classification / query_unresolved_edges / find_path_prefix_module_cycles /
  compute_module_stats / get_snapshot_extraction_diagnostics — assert ZERO calls on a GREEN-cert served path.
  This is the operational definition of "the trust summary no longer reads the raw graph."
FALLBACK CORRECTNESS:    non-TS → fallback (UnsupportedLanguage); non-resident → fallback (Partial); stale →
  fallback (Stale); cert RED (any field diverges) → fallback (named divergence). Each labelled in provenance.
CERT-BUILD-ONCE:         built once per fingerprint (SQLite read on build only), reused, invalidated on
  fingerprint change / restart (mirror import_cert/cycles_cert/stats_cert).
EXTENSION PROBES (prerequisite, BEFORE the producer):  (1) does scip-typescript emit unresolved call occurrences
  at all? (MISSING-1 feasibility — a CJOIN-PROVE-style probe). (2) does the classifier produce parity counts over
  SCIP observations, or does "unresolved" diverge? (MISSING-2 / RISK-T-D). (3) is an enrichment analogue
  feasible, or is enrichment conceded? (MISSING-4).
SCOPE GUARD:             this slice is the producer + cert ONLY. orient / explain / trust fastpath CONSUMPTION
  are LATER slices; this one does not touch their handlers.
```

EXECUTED this slice:
- `rmap --version` → "rmap 0.2.1"; `rmap status` → "unknown command" (no live daemon probe taken). [OBSERVED]
- `rg "ImportObservation" rust/crates/repo-graph-scip-ingest/src` → many matches (lib.rs:20/25/508/896/901/934/
  1242/1511/1512); `rg "ImportResolution::StaticUnresolved|PackageUnresolved" …/scip-ingest/src` → lib.rs:918/
  1242 — the unresolved-IMPORT disposition IS recorded. [OBSERVED]
- `rg "CallObservation" rust/crates/repo-graph-ir/src rust/crates/repo-graph-scip-ingest/src` → NO matches
  (exit 1) — there is no unresolved-CALL observation type. [OBSERVED — load-bearing for §2b fact 3.]
- CORRECTION (vs the iteration-0 draft): a case-sensitive `grep -rn "unresolved" …/scip-ingest/src/` returns
  empty, but that is a CASE artifact (all tokens are capitalized `Unresolved`); it is NOT evidence that ingest
  records no unresolved state. The accurate, verifiable fact is the IMPORT/CALL asymmetry above.
- `Read` over trust types/service/rules/storage_port, trust_impl, the agent aggregator + storage_port +
  agent_impl get_trust_summary, the IR lib.rs, the LiveGraph lib.rs surfaces, the coherence crate, and
  livegraph_feed cert pattern — every §2/§3 OBSERVED claim re-verifiable at the cited file:line.
NOT RUN: cargo build/test, dev-install, live `rmap trust` capture — spec-first; no source path touched; daemon
  start runs index/refresh (state-mutating, out of scope). Same posture orient/explain took.

---

## 9. VERDICT (stated explicitly, evidence-first)

**VERDICT: `NEEDS-EXTENSION`.** The producer is NOT a clean projection. [Evidence: §2b/§2c OBSERVED — the IR's
`IrEdge` is resolved-only (ir/lib.rs:364-378); there is no unresolved-CALL observation (only `import_observations`,
ir/lib.rs:392); SCIP-ingest records unresolved IMPORTS (`ImportObservation`) but has NO unresolved-CALL observation
(`rg CallObservation` → no matches, exit 1); the LiveGraph has no classification /
diagnostics-aggregate / enrichment surface.] Of the 8 `AgentTrustSummary` fields the coherence cluster consumes,
exactly ONE (`resolved_calls`) is LG-derivable today; the other 7 require an IR/LiveGraph extension (§3a). The
producer therefore has its OWN prerequisite: an extension carrying (1) unresolved-call observations, (2) their
classification, (3) a diagnostics aggregate, (4) an enrichment analogue — of which (1) needs a probe (does SCIP
emit unresolved calls?), (2) faces a semantics divergence (RISK-T-D), and (4) may be unachievable without a new
type-inference pass (§4 MISSING-1..4). For three field groups, value-parity may be unachievable even after the
extension because the LiveGraph and the outgoing extractor measure different things (§6c). The exact missing
state + the build sequence + the crate home are surfaced as `DECISION_REQUIRED` (§10).

This is the FEASIBLE-vs-NEEDS-EXTENSION branch the packet's STOP_CONDITION targets; the STOP is taken.

---

## 10. Forced decisions — `DECISION_REQUIRED` (architecture-boundary + new-extension blocks)

Per CLAUDE.md Decision Autonomy: a new IR field / data shape crossing a boundary (the extension), a new
dependency edge (the producer's crate home + how it reaches `rules.rs`), and a re-sourcing that contradicts a
ratified decision (TRUST-LIVEGRAPH-1's deferral of Option B) are each a **stop-and-ask, presented as an
exhaustive matrix**. The packet's STOP_CONDITION is explicit for the NEEDS-EXTENSION branch.

```text
DECISION_REQUIRED:
- ID: DR-TS-0-SEQUENCING
  QUESTION: TRUST-SUMMARY-LIVEGRAPH-1 is NEEDS-EXTENSION (§9), not the clean projection orient DR-1 / explain
            DR-E1 assumed. The producer cannot be built before an IR/LiveGraph extension (MISSING-1..4) lands.
            What is the build sequence now that the shared prerequisite is itself prerequisite-gated?
  OPTIONS:
  - S1 EXTENSION-PROBE-FIRST (recommended): land an IR-EXTENSION PROBE slice (a CJOIN-PROVE-style spike) that
    answers MISSING-1 (does scip-typescript emit unresolved call occurrences?) + MISSING-2 (does the classifier
    produce parity counts, or does "unresolved" diverge under SCIP?) BEFORE committing to the producer. THEN, if
    the probe is GO, build the IR extension, THEN the producer + cert, THEN the per-command fastpaths.
    Consequence: honest, evidence-gated; mirrors Stage B's probe-before-build discipline; defers the producer
    until its feasibility is PROVEN rather than assumed. Cost: another probe slice before any decommission win.
  - S2 BUILD-PRODUCER-ANYWAY: build the producer + cert now over the DERIVABLE subset (CLASS T1/T2) with the
    CLASS T3 fields UNAVAILABLE-on-green. Consequence: the cert is RED by construction (T3 fields have no value
    to compare), the producer ALWAYS falls back → ZERO decommission win, dead code. Rejected (same reason
    orient S2 / explain S2 were rejected — a dead always-RED fastpath).
  - S3 RECONSIDER-OPTION-A (the readiness-9 governance fork): because the shared prerequisite (this producer) is
    itself NEEDS-EXTENSION — a multi-part IR program, one part possibly unachievable — re-weigh Option A (non-TS
    LiveGraph coverage, the larger strategic unlock per readiness-9) against the Option-B extension program. The
    trust extension may be MORE expensive than the per-command fastpaths it unblocks; the operator may prefer
    Option A or a different next build. Consequence: Option B may be paused; this spec stands as the
    authoritative producer-gap + extension-sizing map. A governance call above this slice.
  - S4 PERMANENT-HYBRID (de-scope the producer): accept that the trust SUMMARY stays SQLite-sourced permanently
    (the TRUST-LIVEGRAPH-1 hybrid Half B is the answer; Half A is the only current-state posture). Consequence:
    orient/explain NEVER become `edges`/`unresolved_edges`-free on green for the trust contributor (their gate-1
    stays FAIL for the trust read); the whole Option-B value for the coherence cluster evaporates. Acceptable
    ONLY if the operator deprioritizes the `edges`/`unresolved_edges` decommission for the coherence commands.
  RECOMMENDED: S1. It is the only sequence that does not build on an unproven assumption (MISSING-1 is a probe,
    not a fact) and matches the proven Stage-B/stats discipline (probe/prerequisite before build). It also
    surfaces the S3 re-weigh honestly without foreclosing it.
  BLOCKING_REASON: the producer's feasibility rests on whether SCIP emits unresolved calls at all (unproven) and
    whether the classifier produces parity (a semantics divergence risk). Committing to the producer — or to
    Option B over Option A — before that is known risks a large dead-end. The sequence + the A-vs-B re-weigh
    must be chosen before any IMPL.

- ID: DR-TS-1-MISSING-IR-STATE  [the exact missing state — the NEEDS-EXTENSION payload]
  QUESTION: the trust summary needs (b) unresolved-call disposition, (c) its classification, (d) a diagnostics
            aggregate, (e) enrichment metadata — NONE in the IR/LiveGraph (§2b/§2c). How is that state carried?
  OPTIONS:
  - A EXTEND-THE-IR (recommended IF S1 probe is GO): add a `CallObservation` to `PartitionIr` (the analogue of
    the existing `ImportObservation`, ir/lib.rs:163) recording unresolved call-sites + their disposition, fed by
    a SCIP-ingest extension (MISSING-1), classified by a pass reusing `repo-graph-classification` (MISSING-2),
    aggregated into a diagnostics analogue (MISSING-3). enrichment (MISSING-4) is a SEPARATE later extension or
    a conceded-degraded field. Consequence: the IR becomes the single current-state source of call-resolution
    truth; the producer is then a projection. Cost: a multi-part IR + ingest + classifier program; the deepest
    Stage-D extraction work. New IR data shape crossing a boundary → architecture-boundary, operator-ratified.
  - B CLASSIFIER-OVER-SCIP-ONLY (no IR field): run the classifier directly over SCIP occurrences at ingest,
    emitting classification counts WITHOUT a durable IR `CallObservation`. Consequence: lighter IR, but the
    classification is recomputed each ingest and not inspectable as IR state; the warm-cache (PartitionIr-only)
    would not carry it → recomputed on every load. Weaker than A for the warm-cache end state.
  - C REDEFINE-THE-SUMMARY (current-state semantics): define a NEW current-state trust summary whose
    "unresolved" means the SCIP/LiveGraph disposition (cross-partition `UnresolvedAlias` + unbound occurrences),
    NOT the homegrown extractor's `unresolved_edges` model — and accept that it is a DIFFERENT (not no-loss)
    number, labelled as current-state. Consequence: NO no-loss cert against the SQLite summary (they measure
    different things, §6c); the fastpaths could not AND-fold a no-loss verdict; orient/explain would serve a
    DIFFERENT trust number on green vs fallback → violates overlay-never-erases + the confidence contract.
    Rejected as a no-loss path; viable only as an explicitly-labelled Half-A enrichment of TRUST-LIVEGRAPH-1.
  - D PERMANENT-SQLITE (no extension): keep (b)/(c)/(d)/(e) on SQLite forever. Consequence: = DR-TS-0 S4.
  RECOMMENDED: A, gated on the S1 probe being GO; else S3/S4. A is the only option that yields a no-loss
    current-state summary AND a durable warm-cache-carried fact.
  BLOCKING_REASON: this is a NEW IR data shape + a new ingest/classifier path (architecture boundary), and its
    feasibility depends on an unproven SCIP capability (MISSING-1). It cannot be decided unilaterally.

- ID: DR-TS-2-UNACHIEVABLE-PARITY-FIELDS  [the cert may be RED even after the extension]
  QUESTION: even with DR-TS-1 A, three field groups may not achieve value-parity with the SQLite summary —
            (i) the diagnostics-blob counts (edges_total/unresolved_total: SCIP resolves differently, RISK-T-D),
            (ii) modules[] stable_key identity (module_candidates vs dirname, RISK-E/RISK-T-H), (iii) enrichment
            (no SCIP analogue, MISSING-4). How does the no-loss cert treat fields that CANNOT match?
  OPTIONS:
  - A SCOPE-THE-CERT-TO-MATCHABLE-FIELDS (recommended): the no-loss cert compares ONLY the fields that CAN be
    value-equal (resolved_calls, the call-resolution rate + reliability axes once MISSING-1..3 land, the
    classifications/categories/blast-radius once MISSING-2 lands); the unmatchable fields (i)/(ii)/(iii) are
    served from SQLite + LABELLED (Half-B-style) and EXCLUDED from the no-loss verdict. Consequence: the cert
    can go GREEN on the matchable subset; the labelled-SQLite fields keep the trust read partially SQLite (still
    an `edges`/`unresolved_edges` touch for those fields, unless they too are extended). This REDEFINES the §6b
    FULL no-loss contract into a SCOPED one (see §6b "FULL vs SCOPED") — a DIFFERENT cert, not the §6b cert with
    fields tolerated. Honest, incremental.
  - B REQUIRE-FULL-PARITY: the cert is GREEN only if EVERY field matches. Consequence: given (i)/(ii)/(iii) the
    cert is RED forever → the producer never serves → no decommission win. Rejected (defeats the slice).
  - C RECONCILE-EVERYTHING: do the module-identity reconciliation (RISK-E), redefine the diagnostics counts as
    LiveGraph-recomputed (accepting a number change), and build an enrichment analogue. Consequence: maximal,
    but it is a large multi-slice program and the diagnostics-count redefinition is itself a Half-A semantics
    change (overlaps DR-TS-1 C). Defer to a later slice; not the first increment.
  RECOMMENDED: A. It lets the matchable substance (the resolution rate + reliability — the fields orient/explain
    actually gate on) go GREEN while honestly keeping the unmatchable fields SQLite-labelled.
  BLOCKING_REASON: deciding which fields the no-loss cert covers vs labels-as-SQLite is an architecture-boundary
    call about what "no-loss" MEANS for trust; it changes the cert's GREEN definition and what the fastpaths can
    claim. Must be ratified before the cert is built.

- ID: DR-TS-CRATE-HOME  [where the producer lives + how it reaches the trust rules — dependency edge]
  QUESTION: the producer reads the LiveGraph/IR AND must apply the EXISTING trust rules (rules.rs formulas +
            derive_blast_radius, in `repo-graph-trust` → `repo-graph-classification`). Where does it live, and
            how does it reach the rules WITHOUT inverting the dependency direction (livegraph's FOUR deps are
            all pure/low-level — repo-graph-ir, repo-graph-trust-model, repo-graph-algorithms,
            repo-graph-import-resolver (`Cargo.toml` OBSERVED) — and NONE is the heavy `repo-graph-trust`; the
            trust crate does NOT depend on livegraph)?
  OPTIONS (exhaustive — every cell filled):
  - A EXTEND repo-graph-livegraph (add a `trust_summary()` answer like `module_stats()`):
      reaches-rules: would need `repo-graph-trust` + `repo-graph-classification` added to livegraph's deps —
        WIDENS a crate whose four current deps are ALL pure/low-level (repo-graph-ir, repo-graph-trust-model,
        repo-graph-algorithms, repo-graph-import-resolver; `Cargo.toml` OBSERVED, and the crate doc lists
        exactly these four). dep-direction: livegraph → trust (NEW edge; trust is heavier, pulls
        classification). DRY: rules reused (good). VERDICT: REJECTED — adds the FIRST policy-crate dep to a
        crate whose invariant is pure/low-level deps only.
  - B EXTEND repo-graph-trust (define a `TrustLiveGraphRead` port the LiveGraph implements — the DIP mirror of
      the existing `TrustStorageRead`):
      reaches-rules: the rules stay in trust; trust defines the port, the LiveGraph (or an adapter) implements it
        → adapter depends on policy (Clean Architecture-correct). dep-direction: livegraph → trust (the port) —
        same widening as A but PORT-INVERTED (cleaner). DRY: rules reused. VERDICT: VIABLE; the most
        Clean-Architecture-faithful, but still adds a livegraph→trust edge.
  - C NEW CRATE repo-graph-trust-livegraph (depends on BOTH repo-graph-livegraph AND repo-graph-trust — the
      outer composition, mirroring `repo-graph-livegraph-feed` which depends on scip-ingest + livegraph):
      reaches-rules: imports rules.rs from trust directly; reads the IR/LiveGraph from livegraph. dep-direction:
        the NEW crate → {livegraph, trust} (outer adapter; inverts nothing — both are inner to it). DRY: rules
        reused. livegraph + trust UNTOUCHED. VERDICT: RECOMMENDED — the proven `*-feed` precedent; keeps both
        inner crates minimal; the producer is an outer composition exactly like the existing feed adapter.
  - D repo-graph-coherence (the existing coherence support crate):
      reaches-rules: coherence is PURE wrapper algebra (no I/O, no LiveGraph reads, no SQLite — its own doc); a
        producer that READS the LiveGraph + applies rules would VIOLATE that purity. dep-direction: would add
        livegraph + trust to coherence. VERDICT: REJECTED — coherence is the envelope shape, not a producer.
  - E the daemon livegraph_feed.rs (where the other certs' BUILD lives):
      reaches-rules: the daemon already deps both; but the daemon is WIRING ("main.rs is wiring only";
        "domain logic never lives in CLI/daemon" — architecture.md). The trust-summary COMPUTATION is domain
        logic. dep-direction: fine (daemon is outermost) but LAYER-WRONG. VERDICT: REJECTED for the COMPUTATION;
        the cert BUILD/STORE (the RwLock plumbing) DOES belong here, mirroring build_and_store_import_cert.
  RECOMMENDED: C (new `repo-graph-trust-livegraph` crate, the `*-feed`-style outer composition) for the
    producer; the cert build/store plumbing in the daemon `livegraph_feed.rs` (mirroring the import/cycles/stats
    certs). This keeps repo-graph-livegraph + repo-graph-trust minimal and untouched, reuses rules.rs (one
    threshold source of truth), and matches the proven feed-adapter precedent.
  BLOCKING_REASON: this introduces a NEW crate boundary + a NEW dependency edge (the producer → livegraph +
    trust) and decides how a trust FACT reaches the trust RULES without a dep inversion. That is squarely an
    architecture-boundary decision (CLAUDE.md), and it mirrors the COHERENCE_HOME / RP-T3 call — operator-ratified.
```

---

## 11. Scope boundary

```text
IN SCOPE (this spec): the trust-summary producer + its no-loss cert ONLY. The load-bearing IR feasibility
  analysis (§2), the consumed contract (§3), the field-by-field feasibility (§4), the partial computation design
  (§5), the cert design + its structural limits (§6), the honesty section (§7), the validation plan (§8), the
  VERDICT (§9), and the four architecture-boundary / new-extension decisions (§10).
OUT OF SCOPE: any code, IR field, table deletion, migration, schema change, or default flip (spec-first). The IR
  EXTENSION itself (DR-TS-1 → its own prerequisite probe + slice). The orient / explain / trust fastpath
  CONSUMPTION of this producer (LATER slices — orient §5, explain §5). The TRUST-LIVEGRAPH-1 hybrid Half A/Half B
  (shipped, `dc55114` — this producer would feed a future Option-B current-state summary BESIDE it, not replace
  it). Non-TS LiveGraph coverage (Option A; readiness-9). The 31 non-graph tables + the other defaults'
  fallbacks (the broader decommission). ROADMAP.md / CURRENT_SLICE.md edits (read-only here).
```

---

## 12. References

- `docs/slices/orient-sqlite-free-1.md` §8 DR-1 — the trust-core producer this slice IS (the orient blocker).
- `docs/slices/explain-sqlite-free-1.md` §8 DR-E1 + DR-0 → S1 — the SHARED-prerequisite framing this slice fulfils.
- `docs/slices/trust-livegraph-1.md` §D-TRUST-2 — the anti-Option-B guard; this slice IS the deferred Option B,
  and confirms WHY it was deferred (the IR gap).
- `docs/slices/stats-livegraph-1.md` — the cert-fastpath + IR-extension split precedent (spec → IR-SYMBOL-
  ATTRIBUTES-1 prerequisite → fastpath); the model this slice follows (here the prerequisite is larger).
- `docs/slices/coherence-layer-1.md` — the `CoherenceEnvelope<T>` contract the consuming fastpaths reuse.
- `rust/crates/trust/src/{types,service,rules,storage_port}.rs` — the trust summary's full computation + contract.
- `rust/crates/storage/src/trust_impl.rs` — the SQLite `edges`/`unresolved_edges` reads this producer replaces.
- `rust/crates/agent/src/aggregators/trust.rs` + `storage_port.rs` (`AgentTrustSummary`) + `storage/src/agent_impl.rs`
  (`get_trust_summary`) — the CONSUMED contract (orient/explain/check).
- `rust/crates/repo-graph-ir/src/lib.rs` — the feasibility surface (`IrEdge` resolved-only; `ImportObservation`
  with no `CallObservation` analogue; `SymbolAttributes`).
- `rust/crates/repo-graph-scip-ingest/src/lib.rs` — records unresolved IMPORTS (`ImportObservation`) but has NO
  unresolved-CALL observation (no `CallObservation`; the §2b grep-provable asymmetry, corrected this revision).
- `rust/crates/repo-graph-livegraph/src/lib.rs` — the derivable surfaces (`callers`/`callees`/`module_import_cycles`/
  `module_stats`); its TWO "unresolved" surfaces are `UnresolvedAlias` (cross-partition residency over RESOLVED
  edges) + the IMPORT-observation completeness family (`observation_classes`/`live_import_view`) — NEITHER the
  unresolved-CALL disposition.
- `rust/crates/repo-graph-coherence/src/lib.rs` + `rust/crates/daemon-runtime/src/livegraph_feed.rs` (cert pattern)
  + `state.rs` (cert RwLock slots) — the crate-home candidates + the cert plumbing to mirror.
- `agent_docs/architecture.md` — the dependency rule + build order + layer stack grounding §10's crate-home matrix.
