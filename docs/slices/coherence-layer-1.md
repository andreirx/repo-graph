# COHERENCE-LAYER-1: the mixed-source contract for orient / check / explain / trust

Slice ID: COHERENCE-LAYER-1
Status: **DESIGN / SPEC-FIRST — NOT IMPLEMENTED. CONTRACT RATIFIED (operator sign-off 2026-06-08).** This
document RATIFIES a contract; it produces NO source code, NO table deletion, NO schema/data migration, NO
default flip. The two load-bearing boundary decisions are now RATIFIED (see §Ratified decisions):
COHERENCE-ENVELOPE-SHAPE = **new `CoherenceEnvelope<T>` wrapper**; TRUST-DISPOSITION = **hybrid labelled
model**. No open DECISION_REQUIRED remains. The per-command builds (ORIENT-LIVEGRAPH-1 → CHECK → EXPLAIN →
TRUST) are later slices that execute against THIS ratified contract.

Goal: define how `orient` / `check` / `explain` / `trust` can serve from current-state **LiveGraph** facts
combined with durable **SQLite/persisted authority** WITHOUT lying — honest degradation, no false
completeness. These four are the remaining SQLite-eager defaults after the graph-drilldown migration
(callers/callees/path/imports/cycles/stats = 6/10 served SQLite-free). They are NOT more drilldown
migrations: they are **composite, multi-source aggregators** (orient/check/explain) and an
**extraction-reliability meta-report** (trust). The contract must be specified BEFORE any per-command
implementation, because the seam they cross — current-state LiveGraph × snapshot-scoped SQLite cache ×
user-authored authority — is exactly where a false trust/freshness/completeness claim can be minted.

Track: Stage D, SQLITE-RAW-DECOMMISSION path (design-first leg).
Depends (precedent, reused — NOT re-derived here):
- IMPORTS / CYCLES / STATS-LIVEGRAPH-DEFAULT-FASTPATH-1 — the **cert-gated fastpath** + the SQLite-free
  fingerprint + the byte-preserving additive-metadata (`backend_used`/`fallback_reason`) pattern.
- TRUST-MODEL-REBASE-1 — the `repo-graph-trust-model` crate: the `AnswerEnvelope<T>` answer-vocabulary
  (the per-answer trust/freshness/identity/degradation axes) this contract projects into the four commands.
- QUERY-MIGRATION-1 / VALUE-JOIN-1 / LIVEGRAPH-RUNTIME-1 — what the LiveGraph actually serves today
  (callers/callees/imports/cycles/stats/value_facts), each already as an `AnswerEnvelope`.
- STORAGE-ARCH-1 (`agent_docs/storage-architecture-v2.md`) — the four-tier model (Authority / Operational /
  Derived Cache / Live Working Graph) that decides what may and may not move off SQLite.

## Spec-first note (read first)
```text
This is a SPECIFICATION. Per the repo evidence law, every claim is labelled OBSERVED or INFERRED.
  OBSERVED = inspected first-hand in this authoring — either (a) a read I performed this turn (handler
    offsets in dispatch.rs; the AnswerEnvelope axis vocabulary in repo-graph-trust-model/src/lib.rs; the
    LiveGraph dispatch-wiring boundary), or (b) a read-only investigation subagent that cited the exact
    file:line (the per-table data-source enumeration, the storage-tier mapping, the v1-trust computation).
    Every OBSERVED claim carries its file:line so a reviewer can re-verify.
  INFERRED = my design judgment over those OBSERVED facts (the contract, the source map boundaries, the
    fallback rules, the slice sequence).
Spine claims I PERSONALLY verified first-hand this turn are marked [OBSERVED, first-hand].
NO live `rmap` orientation was run: the daemon socket is absent (`rmap orient` →
"socket does not exist"), and a spec-only slice does not start the daemon or run the index/trust/check
sequence (that mutates state). Orientation was grounded in first-hand source + doc reads instead — the
stronger evidence basis for a contract about code structure. [EXECUTED: `rmap orient` → connection error.]
```

## Why now (priority path)
```text
[OBSERVED: docs/ROADMAP.md §Current Priority + CURRENT_SLICE.md STATUS banner + docs/slices/
stats-livegraph-1.md §"Out of scope".] STATS-LIVEGRAPH-1 shipped (`28ed216`) as the 6th SQLite-free
default. The cert-fastpath + breadth leverage is now EXHAUSTED for the drilldown defaults
(callers/callees/path are lazy; imports/cycles/stats are flipped). stats-livegraph-1.md explicitly
defers the next step to here: "COHERENCE-LAYER (orient/check/explain/trust) follows, design-first, AFTER
stats (higher blast radius; out of scope here)."

[OBSERVED, first-hand: dispatch.rs — handle_orient:2550, handle_check:2672, handle_explain:2734,
handle_trust:2825; LiveGraph is wired into dispatch ONLY for callers/callees/imports/stats/cycles/
livegraph_preload/refresh (all at lines ≤1480). The four coherence handler bodies (2550–2920) contain
NO LiveGraph branch.] => orient/check/explain/trust are the LAST four SQLite-eager defaults and the
LAST class with no served LiveGraph path. They are the precondition for SQLITE-RAW-DECOMMISSION-1: the
raw `nodes`/`edges` substrate cannot be decommissioned while these four read it eagerly on every call.

Higher blast radius than the drilldown defaults because (INFERRED, OBSERVED-backed): (1) each blends
MANY sub-fact sources, not one answer; (2) some of those sources are Layer-4 AUTHORITY (declarations)
that can NEVER move to a rebuildable cache; (3) `trust` is a meta-report whose entire input set
(unresolved-edge classification + the extraction-diagnostics blob) is conceptually an artifact of the
OUTGOING homegrown extractor, which SCIP-first changes. A naive "flip to LiveGraph" here would mint
false-completeness — the exact failure the Fact Certainty Model forbids. Hence: contract first.
```

---

## The eight questions (the contract core)

Each answer is evidence-labelled. The per-command tables in the next section give the field-level detail;
this section is the load-bearing summary the implementation slices must obey.

### Q1 — Which facts come from LiveGraph (current-state)?
```text
The Layer 0–1 STRUCTURAL facts the LiveGraph already serves as an `AnswerEnvelope<T>` [OBSERVED:
repo-graph-livegraph/src/lib.rs — callers:443, callees:560, value_facts:662, path:816,
file_import_cycles:1144, module_import_cycles:1264, module_stats:1323; imports via
live_import_view:1574 (non-envelope read-model)]:

  - callers / callees adjacency (the always-resident global xref + resident-partition detail).
  - file & module import structure; module-import CYCLES (directory-aggregated).
  - module STATS — degree (fan_in/fan_out/file_count) AND, post IR-SYMBOL-ATTRIBUTES-IMPL (`116fbb0`),
    the symbol-classification half (visibility/top-level/subtype) [OBSERVED: stats-livegraph-1.md
    §"DECISION_REQUIRED — RESOLVED": IrNode gained IrVisibility + SymbolAttributes].
  - per-symbol value facts = CYCLOMATIC COMPLEXITY ONLY [OBSERVED: livegraph lib.rs:154-190 —
    ValueFactKind::CyclomaticComplexity is the only kind].

In COMMAND terms these back: explain's IDENTITY / CALLERS / CALLEES / IMPORTS / cycle sections; orient's
cycle-topology signal, its complexity signal, and module structural degree; check's structural inputs.
Identity is the SCIP-derived CanonicalKey [OBSERVED: livegraph lib.rs xref keyed by CanonicalKey].
These are exactly the cert-gated-fastpath candidates the drilldown migration already proved.
[INFERRED mapping from the OBSERVED LiveGraph surface to the OBSERVED command data-sources in §Source map.]
```

### Q2 — Which facts remain persisted / SQLite authority?
```text
Two distinct kinds — do not conflate them:

(2a) Tier-A1 AUTHORITY — PERMANENTLY SQLite, never rebuildable, never in LiveGraph [OBSERVED, first-hand:
storage-architecture-v2.md:37 Tier A1 + Invariant 3 :229-231 "User-authored facts (declarations) never
reside in Tier B or C"]:
  - `declarations` (ONE table, keyed by `kind` ∈ {boundary, requirement, waiver, obligation, entrypoint,
    quality-policy}) [OBSERVED: 001-initial.sql:128; kinds confirmed in crud/declarations.rs +
    trust/src/service.rs:641]. Everything DERIVED from declarations is therefore SQLite-authoritative:
    boundary VIOLATIONS (declared forbidden IMPORTS), the GATE outcome (requirement/obligation/waiver
    evaluation), entrypoint-gated dead-code suppression, quality-policy ASSESS.
  The LiveGraph is a rebuildable-from-source current-state cache (Tier C); authority is user intent.
  An authority fact has no LiveGraph home by construction.

(2b) Tier-B DERIVED-CACHE facts whose PRODUCER is not (yet) SCIP-ingest, so the LiveGraph does not hold
them today [OBSERVED: storage-architecture-v2.md:82-140 Tier B list; migration file:line below]:
  - unresolved-edge CLASSIFICATION + the snapshot `extraction_diagnostics_json` blob — the entire input
    of `trust` [OBSERVED: unresolved_edges @ migration_007.rs:14; extraction_diagnostics_json on
    snapshots @ migration_005; read by trust_impl.rs:90/137/196].
  - MEASUREMENTS beyond complexity: coverage, churn, hotspot, risk [OBSERVED: measurements @
    migration_003.rs:14; orient reads them via query_high_complexity_symbols @ agent_impl.rs:1190].
  - INFERENCES: hotspot scores, framework liveness, Spring beans [OBSERVED: inferences @
    001-initial.sql:146].
  - BOUNDARY facts / surfaces / channels / contracts [OBSERVED: boundary_* @ migration_008/024/025;
    boundary_interaction_links read by orient @ agent_impl.rs:1351].
  - MODULE_CANDIDATES (manifest-declared module discovery — distinct from the LiveGraph's dirname
    aggregation) [OBSERVED: module_candidates @ migration_011.rs:15; orient module summary @
    agent_impl.rs:1297].
  Rebuildable in principle, but NOT by the current SCIP-ingest producer → SQLite-first until a producer
  exists (see Q5 and the slice sequence). This is the honest "rebuildable ≠ rebuilt" distinction.

(2c) FILESYSTEM live-scan (orient's documentation inventory) is neither SQLite nor LiveGraph — it is its
  own current-state source [OBSERVED: get_doc_inventory @ agent_impl.rs:1146 reads repos.root_path then
  live-scans via repo_graph_doc_facts::discover_doc_inventory]. It is already current-state; it stays.
```

### Q3 — Which outputs must carry freshness / degradation labels?
```text
RULE: ANY output that MIXES a LiveGraph current-state fact with a SQLite snapshot-scoped fact MUST carry
per-signal freshness + degradation, because the two sources have INDEPENDENT epochs. [OBSERVED, first-hand:
the cert fingerprint exists precisely to detect this — it digests resident-partition epochs AND the
SQLite snapshot_uid; livegraph_feed.rs import_cert_fingerprint reused across imports/cycles/stats.] A
LiveGraph partition can be Fresh while the SQLite snapshot is a stale index, or vice-versa.

Therefore, mandatory labels:
  - orient: per-signal (cycle-topology / complexity / boundary / module / gate / docs each carry their own
    source + freshness; the envelope confidence is the MEET — Q7).
  - explain: per-section (CALLERS/CALLEES/IMPORTS may be LiveGraph-Fresh while a boundary or
    declaration section is SQLite-snapshot-scoped).
  - check: the verdict carries the freshness of its WEAKEST contributing input (a PASS over a Stale
    trust-summary is a Stale PASS, never a Fresh PASS).
  - trust: must itself state which axes are LiveGraph-current vs snapshot-scoped extraction artifacts.
This is the Fact Certainty Model made operational: "Never describe Layers 2–4 as if they were Layer 0
truth. Never collapse unknown, inferred, and extracted into the same certainty class." [CLAUDE.md §Fact
Certainty Model; architecture.md §"Explicit degradation: null = unknown, empty = known-zero".]
```

### Q4 — Which outputs can be LiveGraph-first?
```text
The pure STRUCTURAL Layer 0–1 sections, via the EXISTING cert-gated fastpath (serve LiveGraph when the
answer-class is Exact AND a GREEN no-loss cert holds at the current fingerprint; else SQLite fallback,
byte-preserving) [OBSERVED: the imports/cycles/stats fastpath ladder, livegraph_feed.rs]:
  - explain: IDENTITY, CALLERS, CALLEES, IMPORTS, cycle sections — these are direct reuses of the
    already-migrated callers/callees/imports/cycles answers.
  - orient: cycle-topology signal; complexity signal (value_facts); module structural degree.
"LiveGraph-first" is precise: it means cert-gated fastpath with SQLite as the labelled fallback, NOT
"LiveGraph-only". The SQLite read survives to BUILD the cert (once per fingerprint) and to SERVE the
fallback — identical to imports/cycles/stats. [INFERRED, OBSERVED-backed by the three prior fastpaths.]
```

### Q5 — Which outputs must stay SQLite-first?
```text
  - Everything reading `declarations` (Layer 4 authority): boundary VIOLATIONS, GATE, entrypoint-gated
    dead-code suppression, quality ASSESS. SQLite is the source of truth; the LiveGraph has no authority
    surface and must not invent one. [OBSERVED: orient/check/explain read declarations via
    get_active_boundary_declarations @ agent_impl.rs:180 + GateStorageRead; storage-arch Invariant 3.]
  - `trust`'s diagnostics CORE: unresolved-edge classification + the extraction-diagnostics blob + the
    entrypoint count. The LiveGraph holds NONE of these [OBSERVED: livegraph crate has no unresolved-edge
    store and no diagnostics blob; trust_impl.rs:90/137/196 read snapshots/unresolved_edges/declarations].
    This CORE stays SQLite-first; per the RATIFIED hybrid (D2) trust ALSO reports a LiveGraph-derived
    current-state posture BESIDE it, each line source+freshness-labelled — neither presented as the other.
  - MEASUREMENTS beyond complexity (coverage/churn/hotspot/risk), INFERENCES, BOUNDARY facts/surfaces/
    contracts, MODULE_CANDIDATES — no SCIP-ingest producer today → SQLite-first (Q2b).
The boundary between Q4 and Q5 is drawn FIELD-BY-FIELD per command in §Source map, not command-by-command:
orient and explain are each PARTLY LiveGraph-first and PARTLY SQLite-first within one envelope. That
intra-command seam is the whole reason this contract exists.
```

### Q6 — What is the shared answer-envelope equivalent (the AnswerEnvelope/cert analogue)?
```text
TWO envelopes already exist and are REUSED, not replaced:
  (i) The per-FACT carrier: `AnswerEnvelope<T>` from repo-graph-trust-model [OBSERVED, first-hand:
      lib.rs:349-365 — fields class, freshness, completeness, data:Option<T>, degradation_reasons,
      missing_partitions, provenance, contributing_languages; smart constructors exact/
      exact_precision_pending/partial/unavailable/stale enforce the 6 invariants]. Every LiveGraph answer
      is already one of these.
  (ii) The per-COMMAND carrier: `OrientResult` [OBSERVED: agent/src/dto/envelope.rs:300-339 — schema,
      command, repo, snapshot, focus, confidence, documentation, signals[], limits[], next[], truncated]
      — ALREADY SHARED by orient, check, AND explain [OBSERVED: check/mod.rs:128-152 and explain reuse
      OrientResult]. trust returns the separate full `TrustReport` [OBSERVED: trust/src/types.rs:241-300].

THE COHERENCE ANSWER-ENVELOPE (RATIFIED) = a NEW generic wrapper
`CoherenceEnvelope<T> { value, provenance, trust, freshness }` (COHERENCE-ENVELOPE-SHAPE, operator
sign-off 2026-06-08), applied COMPOSITIONALLY at two granularities so per-signal certainty is preserved
(the operator's stated reason for the wrapper over an additive field: clean separation + preserved
per-signal certainty):
  - LEAF (per Signal / per explain section): `CoherenceEnvelope<Signal>` — `Signal` is the EXISTING shared
    DTO [OBSERVED, first-hand: agent/src/dto/signal.rs:947-959], NOT a new type. (The iteration-1 draft's
    `SignalData` was fictional — no such type exists; `grep SignalData rust/` is empty. Resolved here.)
    `value` = the `Signal` record; its evidence payload is NOT widened with provenance (that is the contrast
    vs rejected Option A). The leaf's `provenance`/`trust`/`freshness` ride in the wrapper's SIBLING fields
    and describe THAT signal's source. This is where per-signal certainty lives (D4). The one inner field
    that overlaps — `Signal.freshness: Option<FreshnessInfo>` — is the RISK-G reconciliation target, NOT a
    second source of truth: the OUTER envelope freshness is authoritative.
  - ROOT (per command): `CoherenceEnvelope<CoherentOrientResult>`. `value` is NOT the bare `OrientResult`;
    it is a NEW `CoherentOrientResult` command-container DTO = `OrientResult` with its `signals` slot
    re-typed `Vec<Signal>` → `Vec<CoherenceEnvelope<Signal>>` (the leaves), every other field copied
    verbatim (D7). So the per-fact value payload (each `Signal` evidence) stays pristine, but the COMMAND
    CONTAINER shape DOES change at the signals slot — stated precisely, NOT as "OrientResult unchanged".
    Root `trust`/`freshness` = the MEET fold of the leaves (Q7-2); root `provenance` = the set of
    contributing sources. The container `confidence` [OBSERVED: envelope.rs:313] is derived FROM the root
    MEET — never higher than the weakest contributor.
  - `trust` and `freshness` PROJECT the AnswerEnvelope axes verbatim (§envelope spec); `provenance` is the
    NEW source axis (livegraph | sqlite | filesystem | declaration) plus the `ProvenanceBasis` /
    `missing_partitions` / `fallback_reason` detail.
This REUSES the AnswerEnvelope axis vocabulary and the cert-fallback ladder and adds ONE new generic
carrier type `CoherenceEnvelope<T>` (plus the thin `CoherentOrientResult` root container — `OrientResult`
with its `signals` slot re-typed to leaf envelopes, D7). The operator ACCEPTED the larger boundary change /
output-contract churn (orient/check/explain all return the wrapper, superseding the bare shared
`OrientResult` as the top-level return) in exchange for clean separation. The full struct, the MEET fold, and the invariant-preservation proof are in §"The shared
coherence answer-envelope (specification)". [RATIFIED; the rest of this contract — Q1–Q5, Q7, Q8, the
source map, the fallback ladder, the slice sequence — does not depend on the realization details.]
```

### Q7 — How does current-state LiveGraph combine with the durable authority tables?
```text
THE COMBINE MODEL (INFERRED, grounded in the OBSERVED handler assembly + the AnswerEnvelope invariants):

1. ASSEMBLE-BY-SOURCE. Each command handler fetches each sub-fact from its AUTHORITATIVE source:
   structural facts from the LiveGraph (cert-gated, SQLite fallback); authority facts from `declarations`
   (always SQLite); Tier-B derived-cache facts from SQLite (until a producer exists); docs from the
   filesystem. The handler is an ASSEMBLER, not a single-query.

2. MEET FOR FRESHNESS/CONFIDENCE. The envelope's overall freshness/confidence is the MEET of its
   contributing sub-facts. A command blending a Fresh LiveGraph fact with a Stale SQLite snapshot reports
   the AFFECTED signal — and the envelope confidence — as Stale/Degraded, never Fresh-overall. The cert
   fingerprint (resident-partition epochs + snapshot_uid) is the JOIN KEY that detects epoch divergence
   [OBSERVED, first-hand: the shared fingerprint already spans both sides]. The AnswerEnvelope smart
   constructors already forbid the illegal combinations (Exact requires Fresh+Complete; PrecisionPending
   cannot be Exact without a non-SCIP basis; Stale is not Fresh) — the coherence layer must PRESERVE those
   invariants when it folds many envelopes into one [OBSERVED, first-hand: lib.rs invariants in §header
   design rules].

3. AUTHORITY OVERLAYS, NEVER ERASES. A Layer-4 declaration OVERLAYS a structural fact but never deletes
   it. The existing "computed + effective" rule (VISION §Agent Priorities #2; an entrypoint declaration
   SUPPRESSES a dead-code signal but the graph-orphan fact is preserved; a waiver SUPPRESSES a gate
   failure but the computed verdict is preserved) MUST hold across the new LiveGraph/SQLite seam: when the
   structural fact comes from the LiveGraph and the suppression comes from SQLite `declarations`, BOTH the
   computed (LiveGraph) and effective (SQLite-overlaid) views remain queryable. The seam must not become
   an excuse to drop the computed fact.

4. DISJOINT, NOT RECONCILED. The LiveGraph and SQLite contributions to a coherence answer are DISJOINT
   fact sets joined by identity, NOT two computations of the SAME fact that must agree. (Contrast the
   cert-fastpath, which IS a same-fact compare.) orient's cycle topology (LiveGraph) and orient's boundary
   violations (SQLite authority) are different facts; there is no cert between them — only independent
   provenance + freshness labels. The cert applies WITHIN a LiveGraph-first signal (LiveGraph-vs-SQLite
   for THAT structural fact), not ACROSS the disjoint authority facts.
```

### Q8 — What counts as SAFE FALLBACK vs FALSE COMPLETENESS?
```text
SAFE FALLBACK (REQUIRED behaviour): when the LiveGraph is partial / stale / unavailable for a
LiveGraph-first signal, the command either
  (a) serves the SQLite-computed equivalent, the leaf carrying provenance.source=sqlite +
      provenance.fallback_reason (the imports/cycles/stats ladder: RED/stale/missing/precondition-unmet →
      SQLite, never silent loss [OBSERVED: livegraph_feed.rs FallbackReason enum + serve_*_sqlite]); OR
  (b) marks the signal Partial / Stale / Unavailable with an explicit DegradationReason — never drops it,
      never presents it as complete. "Unavailable is not empty" (carries a reason); "null = unknown,
      never empty" (an unaddressable target is unknown, not known-zero) [OBSERVED, first-hand: lib.rs
      invariants 3 + 5; architecture.md Rule 6].
The SQLite answer is the PROVEN PRIMARY; the LiveGraph is the accelerant.

FALSE COMPLETENESS (FORBIDDEN — each is a specific, testable failure):
  F1. Serving a LiveGraph structural section as Exact when a contributing partition is non-resident or
      non-TS (must be Partial+missing_partitions or Partial+language). [Guard: the Exact PRECONDITION =
      answer_class==Exact, already enforced by the fastpath.]
  F2. Reporting orient/check confidence HIGH when the trust-summary input is snapshot-stale or a
      contributing LiveGraph partition is PrecisionPending (MEET rule, Q7-2).
  F3. Emitting an empty callers / dead-code / boundary list as "known-zero" when the true status is
      "unknown" (residency gap, missing producer, or stale index). [Guard: Unavailable≠empty, Q8b.]
  F4. Folding a SCIP-dependent refresh-pending answer into Exact (invariant 6). [OBSERVED, first-hand:
      lib.rs is_scip_backed + the NotScipDependent proof token.]
  F5. trust reporting reliability HIGH when its inputs are stale, OR presenting the v1 SQLite
      extraction-reliability report as if it described the CURRENT-state LiveGraph (it describes the
      OUTGOING extractor's unresolved edges). [The trust-specific false-completeness; the RATIFIED hybrid
      (D2) forbids it by labelling every axis source+freshness — see §Ratified decisions.]
  F6. Letting an authority overlay (waiver/entrypoint) ERASE the computed structural fact instead of
      overlaying it (Q7-3) — a false-completeness about what the graph actually contains.
```

---

## Per-command source map (the field-level boundary)

Legend: **LG-first** = LiveGraph-first via cert-gated fastpath, SQLite fallback (Q4). **SQLite-first** =
SQLite is source of truth (Q5). **FS** = filesystem live-scan. **Authority** = Tier-A1 `declarations`,
permanent SQLite, overlays-never-erases. Layer = Fact Certainty Model layer. All data-source file:line are
OBSERVED (subagent-cited unless marked first-hand).

### orient  [handler dispatch.rs:2550 — first-hand; today 100% SQLite, LiveGraph=NONE — first-hand]
| Signal / section | Today's source (OBSERVED file:line) | Layer | Target posture |
|---|---|---|---|
| repo / snapshot identity | repos @ agent_impl.rs:101; snapshots @ :111 | A2 | SQLite-first (operational) |
| cycle-topology signal | find_module_cycles (edges+nodes) @ agent_impl.rs:143 | 1 | **LG-first** (module_import_cycles) |
| complexity signal | has_complexity_measurements @ :1250; query_high_complexity_symbols @ :1190 (measurements) | 1 | **LG-first** for cyclomatic (value_facts); SQLite-first for coverage/churn/risk |
| module summary / count | get_module_summary (module_candidates) @ :1297 | 1 | SQLite-first (manifest-declared; ORIENT-BUG-1 anchored counts here — keep) |
| boundary violations | get_active_boundary_declarations @ :180 + find_imports_between_paths @ :197 | 4 | **Authority** (declarations) — SQLite-first, overlay |
| boundary links freshness | get_boundary_links_freshness (boundary_interaction_links) @ :1351 | 2-3 | SQLite-first (no LG producer) |
| dead-code | find_dead_nodes (nodes/files/edges/declarations/inferences) @ :158 | 1+4 | computed half LG-derivable; suppression = Authority+inferences SQLite. **See RISK-D** |
| gate readiness | GateStorageRead (declarations/requirements) | 4 | **Authority** — SQLite-first |
| trust overlay (if degraded) | compute_trust_overlay_for_snapshot @ dispatch.rs:2637 | 1 | SQLite-first (trust core; see trust row) |
| documentation | get_doc_inventory (repos.root_path → FS scan) @ :1146 | 1 | **FS** current-state (keep) |

### check  [handler dispatch.rs:2672 — first-hand; today 100% SQLite, LiveGraph=NONE — first-hand]
| Signal | Today's source (OBSERVED file:line) | Layer | Target posture |
|---|---|---|---|
| repo / snapshot | get_repo @ check/mod.rs:53; get_latest_snapshot @ :60 | A2 | SQLite-first |
| stale-files input | get_stale_files (files/file_versions) @ :81 | A2 | SQLite-first (operational freshness) |
| trust summary (reliability/enrichment/confidence) | get_trust_summary @ :84 | 1 | SQLite-first (trust core) |
| gate outcome (verdict driver) | gather_gate_outcome (GateStorageRead) @ :87 | 4 | **Authority** — SQLite-first |
| verdict (PASS/FAIL/INCOMPLETE) | pure reducer @ check/reduce.rs | — | derived; carries MEET freshness of the above |

Note: check is a thin 3-phase reducer over the SAME ports orient uses; it gains no NEW LiveGraph source of
its own. Its coherence work is the MEET-freshness verdict label, not a fastpath.

### explain  [handler dispatch.rs:2734 — first-hand; today 100% SQLite, LiveGraph=NONE — first-hand]
| Section | Today's source (OBSERVED file:line) | Layer | Target posture |
|---|---|---|---|
| identity / symbol context | get_symbol_context (nodes/files/edges OWNS) @ agent_impl.rs:834 | 0-1 | **LG-first** |
| CALLERS | find_symbol_callers @ :883 | 1 | **LG-first** (direct reuse of migrated `callers`) |
| CALLEES | find_symbol_callees @ :935 | 1 | **LG-first** (direct reuse of migrated `callees`) |
| IMPORTS | find_file_imports @ :1113 | 1 | **LG-first** (reuse migrated `imports`) |
| file/path summaries, listings | compute_file_summary @ :647; list_symbols_in_file @ :1037 | 1 | **LG-first** where structural; else SQLite |
| cycles section | find_cycles_involving_{module,path} @ :987/:734 | 1 | **LG-first** (module_import_cycles) |
| boundary section | get_active_boundary_declarations @ :359; find_boundary_declarations_in_path @ :705 | 2-4 | **Authority**+SQLite-first |
| trust + stale | get_trust_summary @ :333; get_stale_files @ :431 | 1 | SQLite-first |

explain is the HEAVIEST (most sections) but also the most DIRECTLY reusable: its structural sections are
the migrated drilldown answers re-projected. Do it AFTER orient proves the provenance-tag pattern.

### trust  [handler dispatch.rs:2825 — first-hand; today 100% SQLite, LiveGraph=NONE — first-hand]
| Output axis | Today's source (OBSERVED file:line) | Layer | Target posture |
|---|---|---|---|
| extraction diagnostics (edges_total, unresolved_total) | get_snapshot_extraction_diagnostics (snapshots.extraction_diagnostics_json) @ trust_impl.rs:90 | 1 | **SQLite-first** (outgoing-extractor artifact) |
| resolution rates / call-graph reliability | count_edges_by_type + count_unresolved_edges_by_classification @ :109/:137 | 1 | **SQLite-first** |
| unresolved categories / classifications / blast radius | query_unresolved_edges (unresolved_edges) @ :196 | 1 | **SQLite-first** |
| downgrade triggers (framework/registry/entrypoint/alias) | rules.rs + count_active_declarations(entrypoint) @ :123 | 1-4 | **SQLite-first** (+ Authority entrypoint count) |
| per-module trust rows | compute_module_stats (nodes/edges/module_candidates) @ :313 | 1 | SQLite-first; module degree is LG-derivable but the trust framing is the v1 model |

trust is the OUTLIER: 100% SQLite, and its inputs are conceptually tied to the OUTGOING homegrown
extractor's unresolved-edge model. Under SCIP-first the meaning of "unresolved edge" changes (the producer
is compiler-grade). TRUST-MODEL-REBASE-1 already built the per-ANSWER `AnswerEnvelope` trust vocabulary as
the INCOMING replacement — a different object (per-answer posture, not a repo-wide reliability report).
RATIFIED (TRUST-DISPOSITION, 2026-06-08) = **hybrid labelled model**: `rmap trust` reports the current
per-answer posture (LiveGraph-derived) PLUS the residual SQLite extraction diagnostics, each line carrying
an explicit source + freshness label. Each axis above becomes a `CoherenceEnvelope` leaf (source =
livegraph for the current-state posture, source = sqlite for the outgoing-extractor diagnostics). TRUST is
the only command whose CHANGE is conceptual, not a re-projection; TRUST-LIVEGRAPH-1 builds this hybrid.

---

## The shared coherence answer-envelope (specification)
```text
NAME: `CoherenceEnvelope<T>` — a NEW generic wrapper type (COHERENCE-ENVELOPE-SHAPE RATIFIED 2026-06-08).
NOT an additive field on the shared Signal/OrientResult DTO; NOT an envelope-level-only overlay. The
per-FACT value payload stays pristine — the `Signal` evidence and the reused structural answers are NOT
widened with provenance; the coherence metadata rides in the wrapper's sibling fields. PRECISION (the
iteration-1 contradiction, now resolved): "pristine" is scoped to the per-fact PAYLOAD, NOT to the command
CONTAINER. The root command container shape DOES change — it is a distinct `CoherentOrientResult` whose
`signals` slot holds leaf envelopes (D7 below) — so this spec does NOT claim "OrientResult is unchanged".
Operator-chosen shape: accept the larger output-contract churn for clean separation + preserved per-signal
certainty.

THE TYPE (proposed realization; the HOME crate — extend repo-graph-trust-model vs a new
repo-graph-coherence crate — is a small boundary call DEFERRED to COHERENCE-ENVELOPE-1, not re-opened
here) [OBSERVED axes, first-hand: repo-graph-trust-model/src/lib.rs:26-208; AnswerEnvelope<T> :349-365]:

  CoherenceEnvelope<T> {
    value:      T,             // the answer payload; leaf T = Signal (evidence un-widened),
                               //                     root  T = CoherentOrientResult (see D7)
    provenance: Provenance,    // WHERE value came from (the NEW source axis)
    trust:      TrustPosture,  // certainty axes, projected from AnswerEnvelope
    freshness:  FreshnessState // epoch axis, the trust-model enum verbatim
  }
  Provenance {
    source:             Source,                // livegraph | sqlite | filesystem | declaration
    basis:              Vec<ProvenanceBasis>,  // reuse AnswerEnvelope.provenance (alias/reconciliation)
    missing_partitions: Vec<String>,           // reuse AnswerEnvelope.missing_partitions (residency gap)
    fallback_reason:    Option<FallbackReason> // set when source flipped LiveGraph→SQLite (cert ladder)
  }
  TrustPosture {                               // projects AnswerEnvelope's certainty axes verbatim
    class:                  AnswerClass,           // Exact | Partial | Unavailable | Stale
    completeness:           QueryCompleteness,     // Complete | Degraded | Unknown
    degradation_reasons:    Vec<DegradationReason>,   // the 10-variant enum (lib.rs:112-135)
    contributing_languages: BTreeSet<LanguageSupport> // union, never collapsed (lib.rs:143)
  }
  freshness: FreshnessState                    // Fresh | Stale | PrecisionPending | RefreshFailed | Unavailable

COMPOSITIONAL APPLICATION (two granularities; this is HOW per-signal certainty is preserved under one
  wrapper type) [OBSERVED Signal DTO: agent/src/dto/signal.rs:947-959, pub(crate), built only via named
  constructors; OrientResult: envelope.rs:300-339, `signals: Vec<Signal>` :320]:
  - LEAF = CoherenceEnvelope<Signal>, one per Signal / explain section. `Signal` is the EXISTING shared DTO,
    NOT a new `SignalData` — that name was fictional (grep-empty); the leaf wraps the real `Signal`.
    Constructed by delegating to (or mirroring) the AnswerEnvelope smart constructors so the six invariants
    hold AT THE LEAF. The inner `Signal` evidence is pristine; the leaf's provenance/trust/freshness live in
    the wrapper siblings (the inner `Signal.freshness: Option<FreshnessInfo>` is the RISK-G reconciliation
    target — the outer envelope freshness is authoritative).
  - ROOT = CoherenceEnvelope<CoherentOrientResult>. The root `value` is a NEW command-container DTO defined
    HERE (D7), NOT the bare `OrientResult`:

      CoherentOrientResult {                      // = OrientResult, with exactly ONE slot re-typed; SHARED
        schema, command, repo, display_name,      //   by orient/check/explain exactly as OrientResult is
        snapshot, focus, confidence,              //   today [OBSERVED reuse: check/mod.rs:128;
        documentation, limits[], limits_*,        //   explain/mod.rs:180/228/436/558]
        next[], next_*, signals_truncated,        //   ── all fields copied VERBATIM from OrientResult …
        signals_omitted_count, truncated,
        signals: Vec<CoherenceEnvelope<Signal>>   //   ── … EXCEPT this slot: Vec<Signal> → leaf envelopes
      }

    So the command CONTAINER shape changes (its `signals` slot now holds leaf envelopes) while each `Signal`
    payload stays pristine — both stated, no longer claimed mutually (the iteration-1 contradiction). Root
    trust/freshness = the MEET fold of the leaves (Q7-2). Root provenance.source = the SET of contributing
    sources. The CoherentOrientResult.confidence [OBSERVED: envelope.rs:313; Confidence{High,Medium,Low}
    :216] is DERIVED from the root MEET — never exceeds the weakest contributor.

ENVELOPE-LEVEL limits[] [OBSERVED base OrientResult: envelope.rs:300-339]: gains provenance-derived codes
  (LIVEGRAPH_PARTIAL, SQLITE_SNAPSHOT_STALE, AUTHORITY_OVERLAY_APPLIED, PRECISION_PENDING,
  PRODUCER_UNAVAILABLE) so degradation is machine-discoverable, not only inside the per-leaf trust.

OUTPUT-CONTRACT IMPACT (the ACCEPTED churn): the JSON wire shape changes — the wrapper is the new top
  level for orient/check/explain and a distinct `CoherentOrientResult` (OrientResult re-shaped at the
  signals slot, D7) becomes its `value`. The byte-preserving precedent applies to the VALUE PAYLOAD of the
  reused structural signals (callers/callees/imports/cycles are byte-identical to the migrated drilldown
  answers [OBSERVED strip of backend_used/fallback_reason: rgr/src/commands/graph.rs :496/:607/:708/:878])
  — NOT to the command container, whose `signals` slot is re-typed and which BY DESIGN gains the honest
  freshness/provenance labels (Q3). Byte-identity of the whole command output is explicitly NOT a
  goal here; honest labelling is. Human render surfaces the labels; JSON consumers read the structured
  wrapper. (This is the precise difference vs the rejected additive-field option, which optimized for
  byte-identity at the cost of widening the shared value DTO.)

INVARIANT-PRESERVATION (the coherence layer MUST NOT weaken the AnswerEnvelope guarantees when folding
  many leaves into one root) [OBSERVED, first-hand: lib.rs §header design rules + smart constructors
  :367-425]:
    I1 Exact requires Complete+Fresh.   I2 Partial must be justified (reason | missing_partition |
    non-Fresh).   I3 Unavailable carries a reason (≠ empty).   I4 Stale is not Fresh.   I5 null≠empty.
    I6 PrecisionPending cannot be Exact without a non-SCIP basis.   + non-empty contributing_languages
    for Exact/Partial/Stale.
  The MEET fold is monotone: it can only LOWER class/freshness/completeness, never raise — so no fold can
  manufacture an Exact from non-Exact leaves. This is the formal anti-false-completeness guarantee.

EXISTING-FRESHNESS RECONCILIATION (OBSERVED, first-hand): the Signal DTO ALREADY carries
  `freshness: Option<FreshnessInfo>` — a coarse Current/Impacted/Unknown from `artifact_contracts`, NOT
  the trust-model FreshnessState [signal.rs:89-98, :958]. The leaf CoherenceEnvelope's richer
  freshness+trust SUPERSEDES this coarse field; COHERENCE-ENVELOPE-1 MUST define the mapping (FreshnessInfo
  → FreshnessState) and decide whether FreshnessInfo is retired or kept as a render-only projection.
  Recorded as a realization point implied by adopting the wrapper, NOT a new boundary decision. See RISK-G.

trust's envelope (HYBRID, RATIFIED): the full `TrustReport` [OBSERVED: trust/src/types.rs:241-300] is
  retained, and EACH reported axis is wrapped as a CoherenceEnvelope leaf using the SAME projection above —
  source=livegraph for the current-state per-answer posture, source=sqlite for the residual outgoing-
  extractor diagnostics — each with its own freshness label. No axis is presented as current-state unless
  its leaf carries source=livegraph + freshness=Fresh.
```

## Safe-fallback contract (per command, explicit degradation)
```text
ORIENT / EXPLAIN (multi-signal): each LG-first leaf degrades INDEPENDENTLY. precondition-unmet or RED
  cert → that leaf's `provenance.source` flips to sqlite with `provenance.fallback_reason` set (the cert
  ladder). A non-resident/non-TS/stale partition → that leaf is Partial/Stale with a degradation_reason;
  OTHER leaves are unaffected. Authority + SQLite-first leaves always carry source=sqlite|declaration. The
  root envelope trust/freshness = MEET, so one degraded leaf lowers overall confidence but never blanks
  the answer. NEVER drop a leaf; NEVER mark a degraded leaf Exact (F1–F4).
CHECK (verdict): the verdict inherits the MEET freshness of (trust-summary, gate, stale-files). A PASS
  computed over a Stale or PrecisionPending input is reported PASS@Stale / PASS@PrecisionPending, exit
  code unchanged but the freshness label explicit. INCOMPLETE remains the honest verdict when a required
  input is Unavailable. NEVER report PASS@Fresh over a non-Fresh input (F2).
TRUST (hybrid, RATIFIED): each axis is a CoherenceEnvelope leaf. The current-state posture leaves carry
  source=livegraph + their own freshness; the residual extraction-diagnostics leaves carry source=sqlite
  and are LABELLED as describing the OUTGOING extractor's snapshot-scoped unresolved-edge model — NEVER
  claimed as current-state LiveGraph resolution (F5). If a contributing snapshot is stale, that leaf is
  Stale-labelled, not silently served as current. The hybrid is the safe-fallback: no axis overstates its
  source or epoch.
SHARED LADDER (mirror the proven fastpath) [OBSERVED: livegraph_feed.rs ladder + panicking-SQLite-closure
  tests prove GREEN skips SQLite]: precondition met + GREEN cert → LiveGraph (SQLite-free for that signal);
  precondition unmet OR cert RED/stale/missing/build-failed → SQLite fallback, labelled. The cert is
  per-LG-first-signal, lazily built per fingerprint; the fingerprint invalidates on any index/refresh/
  partition/policy change.
```

---

## Forced decisions — every cell filled (ratify at sign-off)

### D1 — Envelope realization = new CoherenceEnvelope<T> wrapper (RATIFIED 2026-06-08)
```text
COHERENCE-ENVELOPE-SHAPE is RATIFIED: a NEW generic `CoherenceEnvelope<T> { value, provenance, trust,
freshness }` wrapper, applied compositionally (leaf `CoherenceEnvelope<Signal>` per signal + root
`CoherenceEnvelope<CoherentOrientResult>` per command) — NOT an additive field on the shared
Signal/OrientResult DTO, NOT an envelope-level-only overlay. The operator accepted the larger boundary
change / output-contract churn across orient/check/explain in exchange for clean separation (the per-fact
value payload — each `Signal` evidence — stays un-widened; the command container is re-shaped only at the
signals slot, D7) and preserved per-signal certainty. Full spec in §"The shared coherence answer-envelope".
The exhaustive option matrix that led here is preserved in §Ratified decisions.
```

### D2 — trust disposition = hybrid labelled model (RATIFIED 2026-06-08)
```text
TRUST-DISPOSITION is RATIFIED: `rmap trust` becomes the HYBRID labelled model — it reports the current
per-answer posture (LiveGraph-derived) PLUS the residual SQLite extraction diagnostics, with explicit
source/freshness labels per line. Not (a) freeze-v1, not (b) full LiveGraph rebase. TRUST-LIVEGRAPH-1 is
specified against this. The exhaustive option matrix is preserved in §Ratified decisions.
```

### D3 — Combine semantics = MEET (DECIDED, recorded)
```text
The envelope confidence/freshness fold is the MEET (greatest-lower-bound) over contributing signals
(Q7-2). DECIDED, not asked: it is the only fold consistent with the AnswerEnvelope invariants (monotone,
cannot raise class) and with "never collapse unknown/inferred/extracted". Recorded here per Decision
Autonomy (local mechanism implied by a ratified invariant).
```

### D4 — Per-signal (not per-envelope) provenance granularity (DECIDED, recorded)
```text
Provenance/freshness/trust is labelled PER SIGNAL (the LEAF CoherenceEnvelope), not once per envelope.
DECIDED, not asked: a coarse envelope-level label would collapse a Fresh LiveGraph cycle fact and a Stale
SQLite boundary fact into one class — exactly the Fact Certainty Model violation (architecture.md Rule
"null=unknown, empty=known-zero"; CLAUDE.md "Never collapse … into the same certainty class"). This is WHY
the ratified wrapper is applied compositionally (leaf + root): the root MEET fold never erases a leaf's
label. Per-signal granularity is implied by a ratified invariant and is exactly the "preserved per-signal
certainty" the operator required of the wrapper choice (D1).
```

### D5 — Authority overlay preserves computed fact (DECIDED, recorded)
```text
Across the LiveGraph/SQLite seam, declarations OVERLAY but never ERASE the computed structural fact; both
computed and effective views stay queryable (Q7-3). DECIDED, not asked: this is VISION §Agent Priorities #2
("Preserve computed truth under policy overlays … NEVER erase") applied to the new seam. Recorded.
```

### D6 — Scope (DECIDED, recorded)
```text
This slice ratifies the CONTRACT only. NO command is migrated here. NO non-TS LiveGraph support (non-TS →
SQLite fallback, the fastpath posture). NO change to declarations/gate/measurements/boundary producers. NO
raw `nodes`/`edges` decommission (SQLite builds certs + serves fallbacks + remains authority for Q2b/Q5).
```

### D7 — Root value DTO realization = distinct `CoherentOrientResult` (DECIDED, recorded — iteration 2)
```text
The ratified wrapper's root `value: T` is instantiated as a NEW command-container DTO `CoherentOrientResult`
= `OrientResult` [OBSERVED: envelope.rs:300-339] with its `signals` slot re-typed `Vec<Signal>` →
`Vec<CoherenceEnvelope<Signal>>`, ALL other fields verbatim; the LEAF type is `CoherenceEnvelope<Signal>`
(the real shared `Signal` DTO at signal.rs:947-959 — the iteration-1 `SignalData` was fictional, grep-empty,
and is removed).

DECIDED, not asked: the iteration-1 review (review-1.json) flagged an internal data-shape contradiction —
the draft said `value = OrientResult` (pristine) WHILE also making `OrientResult.signals[]` leaf envelopes;
those cannot both hold. The review offered TWO acceptable realizations of the ALREADY-RATIFIED wrapper:
  (α) define a distinct coherent command value DTO explicitly, or
  (β) define that root and leaf envelopes are serialized SEPARATELY (parallel structures).
Chose α. Rationale: α nests each leaf exactly where its signal lives (ONE self-describing tree, leaf
provenance co-located with the signal it labels). β creates two position-/id-joined arrays
(`OrientResult.signals` + a sibling `Vec<CoherenceEnvelope<Signal>>`) that must stay aligned — a drift
hazard that contradicts this doc's own "DISJOINT, NOT RECONCILED / no parallel structures that must agree"
posture (Q7-4) and the Fact-Certainty discipline.

NOT a re-escalation: the ratified wrapper SHAPE `{ value, provenance, trust, freshness }` is unchanged; D7
only pins how `T` is instantiated at the root — a local realization the ratified decision + the reviewer's
two offered options already imply. Recorded per CLAUDE.md §Decision Autonomy ("choices a ratified decision
already imply → decide and record"). Resolution of the iteration-1 contradiction, stated precisely: the
`Signal` evidence payload is PRISTINE (un-widened); the COMMAND CONTAINER shape DOES change at the signals
slot (a distinct `CoherentOrientResult`). Both are now asserted together, not as mutually exclusive claims.
The existing bare `OrientResult` is left intact for any legacy/non-coherent path; the coherent commands
return `CoherenceEnvelope<CoherentOrientResult>`.
```

---

## Proposed follow-up slice sequence (with dependencies)
```text
The build order obeys architecture.md §Build Order ("support module → storage → feature → tests → docs").
Both boundary decisions are RATIFIED (D1, D2), so the dependent builds are unblocked.

  COHERENCE-LAYER-1  (this; design-first contract) — RATIFIED
        │  COHERENCE-ENVELOPE-SHAPE = wrapper; TRUST-DISPOSITION = hybrid. Both signed off 2026-06-08.
        ▼
  COHERENCE-ENVELOPE-1  (SUPPORT module, pure, off-target unit-tested)
        │   the `CoherenceEnvelope<T>` wrapper + the `CoherentOrientResult` root container (D7) +
        │   Provenance/TrustPosture projections + the MEET fold + the SAFE-FALLBACK ladder as pure domain
        │   logic; PLUS the FreshnessInfo→FreshnessState reconciliation (RISK-G). Home: extend
        │   repo-graph-trust-model OR a new repo-graph-coherence crate — a small boundary call decided AT
        │   that slice. Depends: COHERENCE-LAYER-1.
        ▼
  ORIENT-LIVEGRAPH-1   (FIRST feature build — highest-traffic entrypoint, most overlap with migrated
        │   commands). Cert-gate orient's LG-first leaves (cycle topology, cyclomatic complexity, module
        │   degree); declarations/measurements/boundary/module_candidates/docs stay as mapped; return
        │   CoherenceEnvelope<CoherentOrientResult> with per-leaf provenance + root MEET (D7). Depends:
        │   COHERENCE-ENVELOPE-1.
        ▼
  CHECK-LIVEGRAPH-1    (verdict carries MEET freshness; gate stays Authority). Small; reuses orient's
        │   provenance-aware trust summary. Depends: ORIENT-LIVEGRAPH-1.
        ▼
  EXPLAIN-LIVEGRAPH-1  (heaviest; CALLERS/CALLEES/IMPORTS/cycles sections become LG-first leaves by direct
        │   reuse of the migrated drilldown answers; boundary/declarations stay SQLite). Depends:
        │   ORIENT-LIVEGRAPH-1 (wrapper proven) — explain after the pattern is de-risked on orient.
        ▼
  TRUST-LIVEGRAPH-1    (HYBRID per D2, no longer decision-gated). Wrap each axis as a CoherenceEnvelope
        │   leaf: source=livegraph current-state posture + source=sqlite residual diagnostics, each
        │   freshness-labelled. Depends: EXPLAIN-LIVEGRAPH-1 + COHERENCE-ENVELOPE-1.
        ▼
  COHERENCE-READINESS-RECOMPUTE-1   (re-run the SQLITE-RAW-DECOMMISSION readiness audit with all four
            coherence-migrated; recount SQLite-free defaults; enumerate the residual eager SQLite reads
            (Q2b producers, Q5 authority + the hybrid trust's RETAINED diagnostics tables) that still
            block SQLITE-RAW-DECOMMISSION-1). Depends: all four.

Critical-path note (INFERRED): trust is still the LONG POLE — the hybrid is a larger output contract and
its source-split logic is unique — but it is NO LONGER decision-gated (D2 ratified). The other three are
mechanical re-projections of proven fastpaths onto the wrapper. Sequence puts the de-risking (orient)
first and the heaviest/most-novel case (trust) last. NOTE: the hybrid trust RETAINS the SQLite
unresolved-edge / diagnostics tables (it still reports them, labelled), so it does NOT by itself unblock
their decommission — COHERENCE-READINESS-RECOMPUTE-1 must record them as still load-bearing.
```

## Risks (OBSERVED-grounded; each implementation slice must address)
```text
RISK-A — EPOCH SKEW MINTING FALSE FRESHNESS. A LiveGraph partition Fresh while the SQLite snapshot is a
  stale index (or vice-versa) → a blended answer could read Fresh-overall. MITIGATION: the MEET fold (D3)
  + the fingerprint join key (spans both epochs). The fold is monotone — cannot raise to Fresh.
RISK-B — AUTHORITY/STRUCTURE SEAM ERASING COMPUTED FACT. With the structural fact from LiveGraph and the
  suppression from SQLite declarations, a careless impl could drop the computed fact when overlaying.
  MITIGATION: D5 — both views queryable; preserve computed + effective (VISION §Agent Priorities #2).
RISK-C — TRUST CONCEPTUAL DRIFT. Presenting the v1 SQLite extraction-reliability report as if it described
  current-state LiveGraph resolution (F5). The unresolved-edge model is an OUTGOING-extractor artifact.
  MITIGATION: TRUST-DISPOSITION decision + explicit source labelling; do not flip trust without it.
RISK-D — DEAD-CODE SURFACE TENSION (OBSERVED DISCREPANCY, recorded not reconciled). VISION §"Dead-code
  surface withdrawal" states the DEAD_CODE signal was REMOVED from orient and `rmap dead` disabled; yet
  the orient pipeline still CALLS find_dead_nodes [OBSERVED: agent_impl.rs:158 invoked at orient repo.rs:
  114]. Before ORIENT-LIVEGRAPH-1 touches a dead-code signal it MUST first confirm whether that path
  still surfaces output or is dormant internal substrate. Do NOT migrate a withdrawn surface. [Flagged
  per evidence law; resolution belongs to ORIENT-LIVEGRAPH-1, not this contract.]
RISK-E — MODULE-IDENTITY CORRESPONDENCE (inherited from stats-livegraph-1.md RISK-1). orient/explain
  module facts: SQLite enumerates manifest MODULE nodes / module_candidates; the LiveGraph aggregates by
  dirname. The two module identity sets may differ. MITIGATION: the per-signal cert (field-exact) gates
  any LG-first module signal → RED → SQLite fallback where they diverge; module_candidates COUNT stays
  SQLite (ORIENT-BUG-1 anchored it there).
RISK-F — ENVELOPE SHAPE CHURN (ACCEPTED, bounded). The ratified `CoherenceEnvelope<T>` wrapper changes the
  JSON wire shape of orient/check/explain (the wrapper becomes the new top level; a distinct
  `CoherentOrientResult` — OrientResult with its signals slot re-typed, D7 — becomes its `value`). This is
  the operator-accepted cost of clean separation (D1). MITIGATION: land the wrapper ONCE
  in COHERENCE-ENVELOPE-1 (pure support module, off-target unit-tested) before any command build, so all
  four commands serialize ONE settled shape; keep the reused structural VALUE payloads byte-identical to
  the migrated drilldown answers (only the surrounding envelope gains labels); update the CLI renderer and
  any JSON-contract fixtures in lockstep with COHERENCE-ENVELOPE-1 (schema-version bump if the contract
  tests pin the top-level shape). The churn is front-loaded and one-time, not per-command.
RISK-G — EXISTING PER-SIGNAL FRESHNESS RECONCILIATION (OBSERVED, first-hand). The Signal DTO ALREADY
  carries `freshness: Option<FreshnessInfo>` (Current/Impacted/Unknown from `artifact_contracts`)
  [signal.rs:89-98, :958] — a DIFFERENT vocabulary from the trust-model FreshnessState the wrapper
  projects. Two freshness representations on one signal would be a Fact-Certainty hazard (which is
  authoritative?). MITIGATION: COHERENCE-ENVELOPE-1 defines a SINGLE mapping (FreshnessInfo →
  FreshnessState) and either retires FreshnessInfo OR keeps it strictly as a render-only projection of the
  leaf envelope's freshness — never an independent source of truth. Resolution belongs to
  COHERENCE-ENVELOPE-1 (realization detail implied by the ratified wrapper, not a new boundary decision).
```

## Out of scope (hard guardrails)
```text
NO source code, NO table deletion, NO schema/data migration, NO default flip (this is the CONTRACT). NO
per-command implementation (ORIENT/CHECK/EXPLAIN/TRUST-LIVEGRAPH are later slices). NO new producer for
measurements/boundary/inferences/unresolved-edges. NO change to declarations / gate / authority semantics.
NO raw `nodes`/`edges` decommission. NO non-TS LiveGraph support. NO edit to docs/ROADMAP.md or
CURRENT_SLICE.md (already reconciled; read-only here). NO live daemon run / index / refresh.
```

## Ratified decisions (operator sign-off 2026-06-08)

Both load-bearing boundary decisions are RATIFIED. The iteration-0 review (review-0.json) escalated them as
DECISION_REQUIRED; the operator resolved both on 2026-06-08. The exhaustive option matrices are preserved
below for the decision audit trail (CLAUDE.md §Decision Autonomy: surface a boundary decision as an
exhaustive matrix, every cell filled). NO open DECISION_REQUIRED remains.

### COHERENCE-ENVELOPE-SHAPE — RATIFIED = Option B (new `CoherenceEnvelope<T>` wrapper)

QUESTION: How is per-signal provenance/freshness/degradation realized in the shared orient/check/explain
output contract — given OrientResult is the SHARED DTO of all three [OBSERVED: envelope.rs:300-339; module
doc "check and explain re-use the same envelope" envelope.rs:1-5]? A data shape crossing the daemon→CLI
boundary touching a shared DTO → an architecture-boundary decision (CLAUDE.md Decision Autonomy).

| Option | Per-signal certainty | Value DTO | Boundary change | Byte-identity | Reuse by trust hybrid | Verdict |
|---|---|---|---|---|---|---|
| A — additive field on shared Signal DTO | preserved (field per Signal) | WIDENED (provenance added) | small (additive) | preserved (JSON-only, stripped in render) | n/a (no new type) | not chosen |
| B — new `CoherenceEnvelope<T>` wrapper | preserved (leaf `CoherenceEnvelope<Signal>`) | payload un-widened; container re-shaped at signals slot (D7) | larger (new top-level type; root `value`=`CoherentOrientResult`) | NOT a goal (envelope gains labels by design) | one generic carrier, reused by hybrid trust | **RATIFIED** |
| C — envelope-level overlay only | COLLAPSED (one label/command) | pristine | small | preserved | n/a | REJECTED (violates D4 / Fact Certainty Model) |

RATIFIED: **Option B.** The operator accepted the larger boundary change / output-contract churn in
exchange for clean separation (the per-fact value PAYLOAD stays un-widened — each `Signal` evidence is not
touched; the command container is re-shaped only at its signals slot, D7) and preserved per-signal
certainty. Spec'd against the wrapper throughout (§Q6, §"The shared coherence answer-envelope", §source
map, §slice sequence). Override of the iteration-0 recommendation (which was A): A optimized for
byte-identity by widening the shared value DTO; B isolates ALL coherence metadata in the wrapper SIBLING
fields and keeps each `Signal` payload un-widened — the operator weighted clean separation + per-signal
certainty above byte-identity (not a goal for commands whose entire purpose is honest per-signal labelling,
Q3). REALIZATION (D7, iteration 2): the wrapper's root `value` is a distinct `CoherentOrientResult`
(OrientResult with its `signals` slot re-typed to leaf envelopes), NOT the bare OrientResult — resolving
the iteration-1 "pristine value vs leaf-wrapped signals" contradiction.

### TRUST-DISPOSITION — RATIFIED = Option C (hybrid labelled model)

QUESTION: What does `rmap trust` MEAN under SCIP-first, and where does its data live? Today it is 100%
SQLite and its inputs (unresolved-edge classification + extraction_diagnostics_json + entrypoint count)
are artifacts of the OUTGOING homegrown extractor [OBSERVED: trust_impl.rs:90/137/196]. SCIP is
compiler-grade, so "unresolved edge" changes meaning; TRUST-MODEL-REBASE-1 already built the per-ANSWER
AnswerEnvelope as the incoming trust vocabulary — a different object from a repo-wide reliability report.

| Option | Describes current-state? | New producer | Unblocks raw-table decommission | Output size | Transition honesty | Verdict |
|---|---|---|---|---|---|---|
| A — keep v1 SQLite trust indefinitely | NO (outgoing extractor only) | none | NO (inputs stay load-bearing) | unchanged | honest ONLY if labelled outgoing | not chosen |
| B — rebase onto LiveGraph current-state reliability | YES | YES (repo-wide roll-up; substantial) | YES eventually | similar | honest but front-loads a large build | not chosen (revisit later) |
| C — hybrid labelled (per-answer posture + residual SQLite diagnostics, each source+freshness-labelled) | PARTIAL, explicitly | none (reuses AnswerEnvelope; retains diagnostics) | NO yet (diagnostics retained, labelled) | LARGER | maximal (no axis overstates source/epoch) | **RATIFIED** |

RATIFIED: **Option C.** `rmap trust` reports the current per-answer posture (LiveGraph-derived) PLUS the
residual SQLite extraction diagnostics, with explicit source/freshness labels per line. Satisfies F5
without freezing trust (A) or front-loading a large LiveGraph reliability build (B); revisit toward B once
a current-state reliability producer exists. TRUST-LIVEGRAPH-1 builds this. CONSEQUENCE ACCEPTED: larger
output contract; the unresolved-edge / diagnostics tables are RETAINED (reported, labelled) and are NOT
decommissioned by this work — recorded for COHERENCE-READINESS-RECOMPUTE-1.

## References
```text
GOVERNANCE / MODEL:
- docs/VISION.md §Fact Certainty Model / §Product Layer Model / §Agent Priorities (#2 preserve computed
  truth) / §"Dead-code surface withdrawal".
- agent_docs/architecture.md §Product Layer Stack (Layer 0–4) / Rule 6 "null=unknown, empty=known-zero".
- CLAUDE.md §Fact Certainty Model / §Decision Autonomy.
- agent_docs/storage-architecture-v2.md (Tier A1/A2/B/C; Invariant 3 authority never in Tier B/C; the
  table→tier mapping :37-158, :82-140).

COMMAND IMPLEMENTATIONS TODAY (all SQLite-only; LiveGraph=NONE) [OBSERVED, handler offsets first-hand]:
- rust/crates/daemon-runtime/src/dispatch.rs — handle_orient:2550, handle_check:2672, handle_explain:2734,
  handle_trust:2825; LiveGraph wired only for callers/callees/imports/stats/cycles/preload/refresh (≤1480).
- rust/crates/agent/src/{orient,check,explain}/* + storage_port.rs (AgentStorageRead).
- rust/crates/storage/src/agent_impl.rs (concrete SQL: repos/snapshots/nodes/edges/declarations/inferences/
  unresolved_edges/measurements/module_candidates/boundary_interaction_links + FS doc scan).
- rust/crates/trust/src/service.rs (assemble_trust_report) + storage/src/trust_impl.rs (the v1 SQLite trust).
- rust/crates/agent/src/dto/envelope.rs:300-339 (OrientResult); rust/crates/trust/src/types.rs:241-300
  (TrustReport).

ANSWER-ENVELOPE VOCABULARY [OBSERVED, first-hand]:
- rust/crates/repo-graph-trust-model/src/lib.rs — AnswerClass:26, Granularity:41, FreshnessState:53,
  IdentityBasis:74 (+is_scip_backed:94), DegradationReason:112 (10 variants), LanguageSupport:143,
  ProvenanceBasis:157, QueryCompleteness:180, QueryGranularity:195, classify_answer:256,
  AnswerEnvelope<T>:349 + smart constructors exact/exact_precision_pending/partial/unavailable/stale +
  the 6 invariants (header design rules + tests).

LIVEGRAPH SURFACE (each returns AnswerEnvelope<T>) [OBSERVED]:
- rust/crates/repo-graph-livegraph/src/lib.rs — callers:443, callees:560, value_facts:662 (complexity
  only :154-190), path:816, file_import_cycles:1144, module_import_cycles:1264, module_stats:1323;
  live_import_view:1574 (imports, non-envelope).

CERT-FASTPATH PRECEDENT [OBSERVED]:
- rust/crates/daemon-runtime/src/livegraph_feed.rs — *NoLossCert + *CertState + import_cert_fingerprint
  (shared) + the pure fastpath ladders + FallbackReason (11 variants) + serve_*_sqlite; rgr/src/commands/
  graph.rs strip of backend_used/fallback_reason (:496/:607/:708/:878).
- docs/slices/stats-livegraph-1.md (the immediate precedent; deferred this coherence layer), plus
  imports-/cycles-livegraph-default-fastpath-1.md, query-auto-lazy-sqlite-1.md, trust-model-rebase-1.md,
  query-migration-1.md, value-join-1.md, sqlite-raw-decommission-readiness-7.md.
```
