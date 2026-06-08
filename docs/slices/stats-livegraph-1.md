# STATS-LIVEGRAPH-1: SQLite-free served path for the `rmap stats` default

Slice ID: STATS-LIVEGRAPH-1
Status: **IMPLEMENTED + VALIDATED (2026-06-08 — commit `28ed216` STATS-LIVEGRAPH-IMPL-1; spec ratified
`f6046ab`).** The `stats` default now serves from LiveGraph via a cert-gated fastpath built on the IR
symbol-attributes substrate (IR-SYMBOL-ATTRIBUTES-IMPL `116fbb0`), with SQLite as the labelled fallback and
BYTE-PRESERVING output — the 6th SQLite-free migrated default. Adds `stats` auto|sqlite|livegraph|compare
routing, `stats_cert`, LiveGraph `module_stats`, the shared martin-metrics, the fastpath/fallback ladder, and
the compare surface. Validation (EXECUTED, codex-reviewed): cargo build/test/fmt/clippy(-D warnings)/diff
--check pass; `engine=auto -> backend_used=livegraph` (fallback_reason=null); `engine=compare is_exact=true`
(byte-identical to SQLite); SQLite fallback intact; GREEN-cached fastpath skip-SQLite unit-tested. (This
document was the ratified PLAN; the sections below are retained as the design of record. The original
"DESIGN, NOT IMPLEMENTED / DECISION_REQUIRED" header was resolved by the ratified spec `f6046ab`.)

Goal: plan a served path for the default `rmap stats` that does NOT read SQLite `nodes`/`edges` per call, so that
`stats` joins callers/callees/path/imports/cycles as a default with a SQLite-free common path — advancing the
SQLITE-RAW-DECOMMISSION track from 5/10 to 6/10 served-free defaults.
Track: Stage D, QUERY-MIGRATION-1 (decommission path). Depends (precedent, reused): IMPORTS-LIVEGRAPH-DEFAULT-
FASTPATH-1 (the cert-fastpath pattern + the SQLite-free fingerprint), CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1 (the
cert-gated default flip + the directory-aggregated module-import substrate), CYCLES-OUTPUT-CONTRACT-1 (the
output-migration-vs-byte-preserving precedent this spec answers AGAINST).

## Spec-first note (read first)
```text
This is a SPECIFICATION. It produces NO source code, NO table deletion, NO schema/data migration, NO default
flip. It RATIFIES: (1) the output contract (byte-preserving — §Output contract), (2) the data-dependency map
(§Data dependency), (3) the cert/fastpath design (§Target + D1–D5), and (4) the ONE architecture-boundary
decision that gates the build (§D0 + DECISION_REQUIRED). The implementation slice (STATS-LIVEGRAPH-IMPL-1, or a
split: an IR-symbol-attributes prerequisite + the fastpath) executes only AFTER D0 is ratified. Per the repo
evidence law every claim below is labelled OBSERVED (inspected first-hand: code file:line or doc) or INFERRED
(my judgment from those OBSERVED facts). Cert-machinery file:line were read first-hand in this authoring.
```

## Why now (priority path)
```text
READINESS-7 §Q5 [OBSERVED]: cert-fastpath leverage is EXHAUSTED for the already-migrated defaults (imports +
cycles are flipped; callers/callees/path are lazy) — there is no 6th command with cert machinery waiting to
flip. The next decommission step is BREADTH, not another fastpath. stats is the ONLY remaining drilldown default
that is SQLite-only with NO LiveGraph served path [OBSERVED: dispatch.rs handle_stats §Current state]. READINESS-7
§Q4(a) names it "a REAL build (degree/complexity over the IR + measurements the IR lacks), not a cert-flip" and
recommends SPEC-FIRST. This spec is that spec. COHERENCE-LAYER (orient/check/explain/trust) follows, design-first,
AFTER stats (higher blast radius; out of scope here). [INFERRED priority, OBSERVED-backed: ROADMAP §Current
Priority + CURRENT_SLICE banner + readiness-7 §Q5.]
```

## Current state — how `stats` depends on SQLite today (OBSERVED, first-hand)
```text
DISPATCH [OBSERVED: rust/crates/daemon-runtime/src/dispatch.rs:1243-1324 handle_stats]:
  handle_stats resolves the repo (REG-1), acquires a read lock, gets the latest snapshot, then calls
  repo_state.storage.compute_module_stats(&snapshot.snapshot_uid) (:1288-1290) — the SOLE data fetch — and
  returns {repo_uid, snapshot_uid, display_name, stats, count} (:1314-1323). NO LiveGraph branch, NO cert, NO
  engine routing, NO fastpath ladder. SQLite is read EAGERLY on EVERY call. (Contrast cycles/imports, which have
  an `auto` arm + cert.)

THE QUERY [OBSERVED: rust/crates/storage/src/queries.rs:1120-1268 compute_module_stats]:
  One SQL statement over a SINGLE snapshot, 6 CTEs + a main SELECT, reading TWO tables — `nodes` and `edges`:
    - module_files (:1127-1136): edges type='OWNS', join nodes for tgt.file_uid -> module->file map.
    - file_stats   (:1139-1151): nodes kind='SYMBOL', per file_uid:
        export_count   = SUM(visibility = 'export')
        abstract_count = SUM(subtype IN ('INTERFACE','TYPE_ALIAS')                AND parent_node_uid IS NULL)
        type_count     = SUM(subtype IN ('INTERFACE','TYPE_ALIAS','CLASS','ENUM') AND parent_node_uid IS NULL)
    - module_symbol_stats (:1154-1163): roll file_stats up to module (symbol_count = SUM export_count, etc.).
    - fan_in  (:1166-1174): edges type='IMPORTS', source IN MODULE nodes -> COUNT(DISTINCT source) per target.
    - fan_out (:1177-1185): edges type='IMPORTS', target IN MODULE nodes -> COUNT(DISTINCT target) per source.
    - files   (:1188-1193): edges type='OWNS' -> COUNT(*) per module (file_count).
  Main SELECT (:1195-1210): nodes m WHERE m.kind='MODULE' AND file_count>0, m.qualified_name AS path,
    ORDER BY m.qualified_name. Rust-side (:1237-1264) derives the Martin metrics and rounds to 2 dp
    (Math.round(x*100)/100 mirror):
        instability = fan_out / (fan_in + fan_out)          (0 when total=0)
        abstractness = abstract_count / type_count          (0 when type_count=0)
        distance_from_main_sequence = |abstractness + instability - 1.0|
  => DTO ModuleStatsResult { module, fan_in, fan_out, instability, abstractness, distance_from_main_sequence,
     file_count, symbol_count } [OBSERVED: queries.rs DTO; mirrored in rgr presentation ModuleStats].

LIVE CORROBORATION [OBSERVED: readiness-7 ledger `rmap stats` -> modules=306 files=1133 symbols=3699] — stats
  SERVES a real size/symbol/degree build, not a cert.

OBSERVED DISCREPANCY vs the roadmap framing (recorded per evidence law, NOT reconciled away):
  ROADMAP §Current Priority / readiness-7 §Q4(a) say stats needs "the IR degree graph + measurements the IR
  lacks." FIRST-HAND, compute_module_stats reads NO `measurements` table and computes NO complexity/coverage/
  churn. Its inputs are PURELY (a) the MODULE node set + qualified_name, (b) OWNS edges, (c) IMPORTS edges
  between MODULE nodes, (d) per-SYMBOL `visibility` + `subtype` + `parent_node_uid`. The Martin metrics are
  ARITHMETIC over (a)–(d), not stored measurements. => "measurements the IR lacks" is IMPRECISE: the real lack
  is per-symbol VISIBILITY and TOP-LEVEL(parent) classification (Layer-0 EXTRACTION facts), not the measurements
  table. This correction is load-bearing — it changes what the build must source (§Data dependency, §D0).
```

## Data dependency — what stats needs vs what the IR / LiveGraph has (OBSERVED, first-hand)
```text
The cert-fastpath model (imports/cycles) requires the LiveGraph to compute the FULL answer so a compare can gate
GREEN. So the question is: can today's IR/LiveGraph produce all 8 ModuleStatsResult fields? Per-field audit:

WHAT THE IR/LiveGraph HAS [OBSERVED]:
  - PartitionIr { nodes: Vec<IrNode>, edges: Vec<IrEdge>, import_observations } [repo-graph-ir/src/lib.rs:331-341].
  - IrEdge carries EdgeType::Imports — a FILE->FILE AST-derived module-import edge [lib.rs:77-83, :310-326].
  - Modules are DIRECTORY-AGGREGATED (module = dirname(file)); the LiveGraph already aggregates the FILE import
    graph to module identities for cycles [repo-graph-livegraph/src/lib.rs:1773-1803 ModuleImportCycle*; the
    `module_aggregated` scope marker]. Cross-partition import resolution exists (AstImportFileInventoryResolved /
    tsconfig-path / dynamic) [lib.rs:106-123].
  - value_facts: a per-symbol channel, ValueFactKind = CyclomaticComplexity ONLY [livegraph lib.rs:154-190,
    :661 value_facts(); used :2004,:2280].

WHAT IrNode CARRIES [OBSERVED: repo-graph-ir/src/lib.rs:292-308]:
  IrNode { key, subtype: String (e.g. "FUNCTION","CLASS"), name, range, partition_id, identity_source,
  provenance }.  *** NO `visibility` field.  NO `parent`/top-level field. ***

PER-FIELD VERDICT [OBSERVED data + INFERRED mapping]:
  | stats field      | SQLite source (OBSERVED)                              | computable on IR/LiveGraph today? |
  |------------------|-------------------------------------------------------|-----------------------------------|
  | fan_in           | IMPORTS edges between MODULE nodes                     | YES — dir-aggregate the FILE       |
  | fan_out          | IMPORTS edges between MODULE nodes                     |       Imports graph (same substrate|
  |                  |                                                       |       as cycles), count distinct.  |
  | file_count       | OWNS edge count per module                            | YES* — dirname/file-inventory      |
  |                  |                                                       |       aggregation (see RISK-2).    |
  | module set+path  | nodes kind='MODULE', qualified_name                   | YES* — dir identities (see RISK-1).|
  | symbol_count     | SYMBOL visibility='export'                            | NO  — IrNode has no visibility.    |
  | abstract_count   | SYMBOL subtype∈{INTERFACE,TYPE_ALIAS} ∧ parent NULL   | NO  — no parent; subtype PARTIAL.  |
  | type_count       | SYMBOL subtype∈{INTERFACE,TYPE_ALIAS,CLASS,ENUM} ∧ …  | NO  — no parent; subtype PARTIAL.  |
  | instab/abstr/dist| Rust arithmetic over the above                        | derived once inputs exist.         |

  => SPLIT VERDICT [INFERRED, OBSERVED-backed]: the STRUCTURAL/degree half (fan_in, fan_out, file_count, module
     set) is COMPUTABLE from existing IR data — NOT a data gap, just NEW computation (the same directory
     aggregation cycles already does, with degree instead of SCC). The SYMBOL-CLASSIFICATION half (symbol_count,
     abstract_count, type_count -> abstractness, distance) is STRUCTURALLY NOT computable: the IR carries no
     symbol `visibility` and no top-level/`parent` attribute, and value_facts is complexity-only. This is the
     FIRST decommission migration that hits a hard Layer-0 EXTRACTION-SUBSTRATE gap — imports/cycles reused
     existing edges and added NO node attribute. That gap is the D0 architecture decision.
```

## Output contract — BYTE-PRESERVING (the required answer, with evidence)
```text
QUESTION (DEFINITION_OF_DONE): does stats require a one-time USER-VISIBLE OUTPUT MIGRATION (like CYCLES-OUTPUT-
CONTRACT-1) to reach byte-identity, or is it BYTE-PRESERVING (like callers/callees/path/imports)?

ANSWER: BYTE-PRESERVING. No output migration is required. [INFERRED from the OBSERVED renderer + query below.]

EVIDENCE:
  1. The renderer is ALREADY CANONICAL [OBSERVED: rgr/src/presentation/stats.rs:80-161 render_human]. Each
     section CLONES the stats vec and RE-SORTS it deterministically before printing:
       - "By size"  (:106-118): sort DESC by (file_count, then symbol_count).
       - "By fan-in" (:122-130): sort DESC by fan_in.
       - "By fan-out"(:133-142): sort DESC by fan_out.
       - "By distance from main sequence" (:146-158): sort DESC by distance_from_main_sequence.
     The rendered TEXT depends ONLY on the per-module VALUES, never on the daemon's row order — UNLESS values
     tie (see the tie-break condition below). Summary (:91-97) is order-independent (len + sums).
  2. The daemon ALREADY emits qualified module identities [OBSERVED: queries.rs:1196 `m.qualified_name AS path`,
     :1210 `ORDER BY m.qualified_name`]. There is NO short-vs-qualified name divergence to fix.

WHY THIS DIFFERS FROM CYCLES [OBSERVED: cycles-livegraph-default-fastpath-1.md "BLOCKED" §]: cycles needed a
  migration because the SQLite default emitted SHORT names ("src") in Tarjan/cycle_id order while LiveGraph
  emitted QUALIFIED names ("packages/a/src") in its own order — the BYTES differed for the SAME set, so CYCLES-
  OUTPUT-CONTRACT-1 had to canonicalize FIRST. stats has NEITHER problem: SQLite already emits qualified_name,
  and the renderer already canonicalizes via deterministic per-section re-sort. stats is the IMPORTS class
  (byte-identical served data), not the CYCLES class.

BYTE-IDENTITY CONDITION (the one nuance a reviewer must see) [OBSERVED: Rust `sort_by` is a STABLE sort]:
  The section sorts are single-key (or two-key for "By size"); WITHIN a tie group the order is the INPUT order =
  the daemon's qualified_name ascending. Therefore byte-identity requires the LiveGraph path to hand the renderer
  (i) IDENTICAL per-module values (with the SAME 2-dp rounding, queries.rs:1250-1253) AND (ii) the modules in
  qualified_name-ASCENDING order, so stable-sort ties resolve identically. Both are satisfiable by construction
  (the fastpath sorts its module vec by qualified_name before returning). The renderer itself is UNTOUCHED.
  => No CLI/renderer change, no human-output migration, no JSON contract break. backend_used / fallback_reason
     are additive JSON-only fields, stripped in human (the imports/cycles precedent).
```

## Target — the SQLite-free served path (mirrors the cycles/imports fastpath)
```text
The default `rmap stats` becomes a cert-gated LiveGraph fastpath, EXACTLY the shape of cycles_auto_response
[OBSERVED: livegraph_feed.rs:2247] and the imports fastpath:
  1. The handler dispatches default -> a `stats_auto_response` (new `auto` arm), analogous to handle_cycles ->
     cycles_auto_response. An explicit escape hatch (`--engine sqlite`) forces the SQLite arm UNCHANGED.
  2. PRECONDITION (SQLite-free): the LiveGraph module-stats answer-class is `Exact` (all contributing partitions
     resident + Fresh + TS-primary). Else -> SQLite fallback, labelled (non-TS / non-resident / stale).
  3. CERT GATE: a repo-level STATS no-loss cert {verdict: GREEN/RED, fingerprint} on RepoState, GREEN iff a
     repo-wide STATS COMPARE (LiveGraph-computed stats == SQLite compute_module_stats, per-module field-exact)
     has zero divergence. GREEN + precondition -> serve LiveGraph stats WITHOUT compute_module_stats. RED /
     stale / missing / build-failed -> SQLite fallback (the proven answer, NO loss).
  4. The cert is keyed by the SHARED SQLite-free fingerprint [OBSERVED: import_cert_fingerprint,
     livegraph_feed.rs reused at :2270 for cycles] (partitions {epoch/fresh/ts/hash/producer} + snapshot_uid +
     policy version). A fingerprint mismatch invalidates + lazily rebuilds. In-memory, rebuilt on restart (S1).
The SQLite read survives ONLY (i) to BUILD the cert (once per fingerprint) and (ii) on the fallback — IDENTICAL
to imports/cycles. This is NOT a raw decommission; `nodes`/`edges` stay load-bearing.
```

## Forced decisions — every cell filled (ratify at sign-off)

### D0 — Sourcing the symbol-classification half (THE architecture-boundary decision — BLOCKING; see DECISION_REQUIRED)
```text
The cert model needs the LiveGraph to compute symbol_count/abstract_count/type_count, which need per-symbol
`visibility` (export) + top-level(`parent`) + the {INTERFACE,TYPE_ALIAS,CLASS,ENUM} subtypes. IrNode carries
NONE of visibility/parent today [OBSERVED: repo-graph-ir/src/lib.rs:292-308]. How is that data sourced? This is
a NEW data shape crossing the scip-ingest -> repo-graph-ir -> repo-graph-livegraph boundary — an architecture
decision precedent does NOT cover (imports/cycles added no node attribute). Options, consequences exhaustive:

  A. EXTEND IrNode (Layer-0) with `visibility` + `is_top_level` (and confirm subtype coverage), populated by
     repo-graph-scip-ingest from the ts-extractor/SCIP. LiveGraph computes FULL stats; cert-gate as cycles.
     CONSEQUENCE: clean architectural home (structural facts belong on the IR node); touches repo-graph-ir +
     repo-graph-scip-ingest + repo-graph-livegraph + livegraph-feed; REQUIRES a spike confirming scip-typescript/
     the ts-extractor reliably emit export-visibility + nesting + the 4 subtypes (parity-or-fallback). Biggest
     blast radius; unblocks byte-identical full-stats fastpath. The warm-cache projection (PARTITIONED-WARM-
     CACHE-ARCH-1, persists PartitionIr) must carry the new fields — a cache-format bump.
  B. STRUCTURAL VALUE-FACT CHANNEL: carry is_export + subtype + is_top_level as NEW per-symbol facts on the
     value-facts channel (VALUE-JOIN-1 D6, the extensible separate channel) instead of on IrNode. CONSEQUENCE:
     smaller IR footprint; but SEMANTICALLY STRETCHES "value facts = MEASUREMENTS (complexity)" to hold
     structural attributes — a contract smell; AND it still needs the ingest/extractor to SOURCE visibility +
     parent (the hard part is unchanged). Lower architectural coherence than A.
  C. PARTIAL-STATS LiveGraph: serve only the structural columns (fan_in/fan_out/file_count) from LiveGraph; keep
     symbol_count/abstractness/distance SQLite-sourced. CONSEQUENCE: the served answer is a HYBRID that STILL
     reads SQLite every call -> near-ZERO decommission win; breaks the single-answer cert; the renderer needs all
     8 fields. REJECT (defeats the slice goal).
  D. SEQUENCE AS A PREREQUISITE: this spec ratifies the output contract + the degree-graph computation + the
     cert design; the symbol-attributes source (A or B) becomes its OWN prerequisite slice (IR-SYMBOL-
     ATTRIBUTES-1, extraction-substrate, with its own parity/fallback validation), and STATS-LIVEGRAPH-IMPL-1
     depends on it. CONSEQUENCE: honest sequencing for a Layer-0 change too big to bundle with the fastpath;
     keeps each slice single-actor (extraction vs query-serving). This is a SEQUENCING choice ORTHOGONAL to A/B
     (you still pick a source); it answers "one slice or two?".

RECOMMENDATION [INFERRED]: A as the data home (structural attributes belong on the IR node, not the measurement
  channel) + D as the sequencing (a Layer-0 extraction change with a producer-capability spike is its own slice,
  ratified and validated independently before the query-serving fastpath consumes it). i.e. STATS-LIVEGRAPH-1
  (this spec, ratified) -> IR-SYMBOL-ATTRIBUTES-1 (prerequisite: extend IrNode + ingest + warm-cache; spike
  scip-typescript export/nesting/subtype coverage; PARTIAL/fallback where the producer can't) -> STATS-LIVEGRAPH-
  IMPL-1 (the cert/fastpath, byte-preserving). This is a genuine architecture-boundary + new-dependency-edge
  decision -> NOT mine to invent. See DECISION_REQUIRED: STATS-IR-ATTRS.
```

### D1 — Cert predicate (no-loss verdict)
```text
GREEN iff the repo-wide STATS COMPARE is EXACT: for EVERY module in the SQLite answer, the LiveGraph answer has
the SAME module identity (qualified_name) AND field-equal (fan_in, fan_out, file_count, symbol_count, and the
2-dp-rounded instability/abstractness/distance) — missing=0 AND extra=0 AND no field mismatch. Mirrors the cycles
predicate `comparison.is_exact()` [OBSERVED: livegraph_feed.rs:2206], extended from a set-compare to a
field-compare (stats rows carry numbers, not just identities).
RATIONALE [INFERRED]: stats is a richer payload than a cycle set, so no-loss must include field equality, not
  just module-set equality. Any divergence (a module only SQLite has; a count off by one; a rounding edge) ->
  RED -> SQLite fallback. Conservative: a repo whose dirname-aggregation does not correspond to its SQLite MODULE
  nodes (RISK-1) is RED -> safely falls back, never serves a wrong stat.
ALTERNATIVE (rejected): set-only compare (ignore field values) — would serve mismatched counts on a GREEN; unsafe.
```

### D2 — Cert source + storage (mirror imports/cycles)
```text
A repo-level STATS no-loss cert {verdict, fingerprint} on RepoState [in-memory RwLock<Option<StatsNoLossCert>>,
S1 — the EXACT pattern of import_cert/cycles_cert, OBSERVED: state.rs:205,:212]. Built by a new
build_and_store_stats_cert (the stats compare -> is_exact -> verdict), keyed by the SHARED import_cert_fingerprint
[OBSERVED reused at livegraph_feed.rs:2270]. Lazily built on the first eligible default call (T1); rebuilt on a
fingerprint change or restart. No persisted surface, no new invalidation key.
RATIONALE [INFERRED]: the fingerprint already covers BOTH sides of no-loss — LiveGraph (partition epoch/hash/
  producer) AND SQLite (snapshot_uid -> repo_index_epoch). Identical to how cycles reuses it. T1+S1 is the
  ratified imports/cycles default.
```

### D3 — Runtime behaviour (the fallback ladder)
```text
default `stats` (engine `auto`):
  1. precondition UNMET (non-TS / non-resident / stale contributing partition; or — until D0 lands — the symbol-
     classification half is unavailable) -> SQLite fallback (compute_module_stats, labelled).
  2. precondition met AND a VALID GREEN cert -> FASTPATH: serve LiveGraph stats, NO compute_module_stats
     (backend_used=livegraph).
  3. precondition met AND (cert RED / stale / missing / build-failed) -> SQLite fallback. The cert is LAZILY
     built on the first eligible call (reads SQLite ONCE per fingerprint via the compare).
NO behaviour loss: RED/stale/missing/precondition-unmet ALWAYS serves the SQLite answer. Pure ladder shape ==
cycles_fastpath_or_sqlite [OBSERVED: livegraph_feed.rs:2220-2241], unit-testable with a panicking SQLite closure
proving the GREEN path skips compute_module_stats.
```

### D4 — Output compatibility (byte-preserving)
```text
Human default stays byte-identical (§Output contract): the fastpath maps LiveGraph module stats into the SQLite-
compatible {repo_uid, display_name, snapshot_uid, stats[8 fields], count} shape, qualified_name-ascending, with
2-dp rounding identical to queries.rs:1250-1253; the renderer (stats.rs) is UNTOUCHED. JSON: + backend_used
("livegraph"|"sqlite") + fallback_reason (null on the fastpath) — additive, JSON-only, stripped in human (the
imports/cycles precedent). The explicit `--engine sqlite` route forces compute_module_stats unchanged (no
backend_used) — the cycles auto/sqlite split [OBSERVED: cycles-livegraph-default-fastpath-1.md "DIVERGENCE FROM
PLAN"] is reused so the daemon can distinguish DEFAULT from forced SQLite.
NOTE: unlike cycles, stats today has NO explicit `--engine livegraph|compare` surface [OBSERVED: handle_stats has
  no engine routing]. The implementation may either add an explicit stats compare/livegraph surface FIRST (the
  cycles ordering: CLI -> compare -> readiness -> default), or build the auto/sqlite split + the internal compare
  directly. RECOMMEND mirroring cycles: a thin `--engine compare|livegraph|sqlite` for stats lands first
  (provides the compare that BUILDS the cert + an operator escape hatch), then the default flip. [INFERRED]
```

### D5 — Scope + validation
```text
SCOPE: the default `rmap stats` (module stats) served path ONLY. NO non-TS support (non-TS -> SQLite fallback).
  NO resolver / module-identity change. NO measurements/value-facts semantics beyond what D0 ratifies. NO change
  to any other command.
VALIDATION (the eventual IMPL would prove — see §Validation plan): GREEN TS repos (e.g. xpart, amodx) serve
  LiveGraph stats == SQLite stats (field-exact, human byte-identical); RED / non-corresponding / non-TS repos
  fall back; a fingerprint bump rebuilds; the panicking-SQLite-closure proves the GREEN path is compute-free.
```

## Safety predicate / cert (how correctness is gated before the default flips)
```text
The default NEVER serves a LiveGraph stat the SQLite compare would not have produced. The gate is the GREEN cert
(D1): a repo-wide field-exact stats compare. Until a GREEN cert exists for the current fingerprint, the default
serves SQLite (lazy-build reads SQLite once; RED -> SQLite). The cert is the COMPARE verdict, so a GREEN cert
cannot serve a divergent answer (assert: GREEN => is_exact). The fingerprint (shared, SQLite-free) guarantees any
index/refresh/partition/policy change invalidates the cert -> no stale fastpath. This is the IMPORTS/CYCLES no-
loss contract, extended to field equality for the richer stats payload. [INFERRED from the OBSERVED cycles cert.]
```

## Risks (OBSERVED-grounded; the implementation must address each)
```text
RISK-1 — MODULE-IDENTITY CORRESPONDENCE [the central correctness risk]. SQLite stats enumerates `nodes`
  kind='MODULE' with qualified_name [OBSERVED: queries.rs:1196,:1208]; LiveGraph modules are dirname(file)
  directory aggregations [OBSERVED: livegraph lib.rs:1773-1791]. These sets/identities may DIFFER (manifest/
  declared MODULE nodes vs pure dirname). MITIGATION: the field-exact cert (D1) catches ANY divergence -> RED ->
  SQLite fallback. GREEN repos are byte-safe; non-corresponding repos fall back (EXPECTED, not a regression —
  the cycles/imports posture). The IMPL must NOT assume correspondence; it must PROVE it per-repo via the cert.
RISK-2 — OWNS-vs-dirname file_count [OBSERVED: queries.rs OWNS edges vs IR has no OWNS]. file_count from dirname/
  file-inventory aggregation may not equal the SQLite OWNS count if OWNS encodes non-dirname ownership.
  MITIGATION: same cert gate (field-exact includes file_count). Surfaces as RED where they disagree.
RISK-3 — D0 EXTRACTION GAP [OBSERVED: IrNode has no visibility/parent]. Without D0, the symbol half is
  uncomputable -> stats cannot cert-migrate at all. This is the BLOCKING dependency, not a residual.
RISK-4 — SUBTYPE COVERAGE [INFERRED, needs spike under D0]. IrNode.subtype is a free string from extraction
  [OBSERVED: lib.rs:295-297 "FUNCTION","CLASS"]; whether scip-typescript ingest emits INTERFACE/TYPE_ALIAS/ENUM
  with the SAME spelling the SQLite path uses is UNVERIFIED. The D0 prerequisite slice must spike this; any gap
  -> those modules RED -> fallback (no wrong answer, but a smaller GREEN set).
RISK-5 — ROUNDING/FLOAT EQUALITY [OBSERVED: queries.rs:1250-1253 (x*100).round()/100]. The compare must apply
  the IDENTICAL rounding before field equality, or float jitter forces spurious RED. MITIGATION: compute the
  Martin metrics in ONE shared helper used by both backends (the cycles "extract module_cycle_compare_data,
  both derive from it" pattern [OBSERVED: cycles-livegraph-default-fastpath-1.md "Completion"]).
```

## Validation plan (how the eventual IMPLEMENTATION would be proven — NOT run here)
```text
SUPPORT (pure, unit-tested off-target):
  - the Martin-metric helper (shared by both backends) — rounding parity vs queries.rs:1250-1253.
  - the stats compare (field-exact) -> verdict; GREEN iff is_exact. Pure, table-driven (missing/extra/field-
    mismatch -> RED; identical -> GREEN).
  - the stats_fastpath_or_sqlite ladder — GREEN -> panicking compute_module_stats NEVER called; RED/stale/
    build-fail/precondition-unmet -> SQLite (mirror the 7 cycles ladder tests).
IMPLEMENTATION (live, EXECUTED on the real corpus, dev-install-local):
  - GREEN TS repo (xpart / amodx): default -> backend=livegraph, stats field-exact vs `--engine sqlite`, human
    byte-identical (diff == empty).
  - RED / non-corresponding repo (repo-graph self, where the fixture partitions / dirname-vs-MODULE divergence
    forces RED): default -> backend=sqlite, fallback_reason set, stats unchanged.
  - non-TS repo (OpenXcom): default -> SQLite fallback.
  - invalidation: refresh -> fingerprint change -> next default rebuilds the cert, still correct.
  - GATE: cargo test --workspace 0 failures; clippy -D warnings clean; fmt clean.
EVIDENCE LABELS: each result EXECUTED/OBSERVED per the evidence law; no INFERRED presented as OBSERVED.
ORIENTATION/VALIDATION FOR THIS SPEC (read-only, already used): rmap orient/trust/stats/cycles, git, grep,
  first-hand reads of dispatch.rs/queries.rs/stats.rs/state.rs/livegraph_feed.rs/repo-graph-ir + repo-graph-
  livegraph. No code/test was owed for a spec-only deliverable.
```

## Out of scope (hard guardrails)
```text
NO source code, NO table deletion, NO schema/data migration, NO default flip (this is the SPEC). NO non-TS
support. NO resolver / module-identity change. NO measurements-table introduction (stats does not read it —
§Current state discrepancy). NO COHERENCE-LAYER (orient/check/explain/trust) work — that is the NEXT, design-
first slice. NO raw decommission of `nodes`/`edges` (SQLite still builds the cert + serves the fallback). NO edit
to ROADMAP.md / CURRENT_SLICE.md (reconciled in PRIORITY-DOCS-RECONCILE-1).
```

## DECISION_REQUIRED
```text
DECISION_REQUIRED:
- ID: STATS-IR-ATTRS
  QUESTION: How is the per-symbol classification data (visibility=export + top-level/parent + the {INTERFACE,
    TYPE_ALIAS,CLASS,ENUM} subtypes) sourced so the LiveGraph can compute stats' symbol_count/abstract_count/
    type_count, given IrNode carries NO visibility and NO parent today [OBSERVED: repo-graph-ir/src/lib.rs:292-
    308]? And is it one slice or a prerequisite + impl? This is an architecture-boundary + new-dependency-edge
    decision (a new data shape crossing scip-ingest -> repo-graph-ir -> repo-graph-livegraph) that precedent
    (imports/cycles, which added no node attribute) does NOT cover. Per the slice STOP_CONDITION and CLAUDE.md
    Decision Autonomy, I surface it rather than invent it.
  OPTIONS:
  - A (extend IrNode, Layer-0): add visibility + is_top_level to IrNode, populated by scip-ingest from the ts-
    extractor/SCIP. Clean home; biggest blast radius (4 crates + warm-cache format bump); needs a producer-
    capability spike (does scip-typescript emit export-visibility + nesting + the 4 subtypes?).
  - B (structural value-fact channel): carry is_export/subtype/is_top_level on the per-symbol value-facts channel
    instead of IrNode. Smaller IR change; but stretches "value facts = measurements" (contract smell) and still
    needs the ingest to source visibility+parent (the hard part is unchanged).
  - C (partial-stats): serve only fan_in/fan_out/file_count from LiveGraph, keep the symbol half on SQLite.
    REJECT — still reads SQLite every call (near-zero decommission win); breaks the single-answer cert.
  - D (sequencing, orthogonal to A/B): split into a prerequisite IR-SYMBOL-ATTRIBUTES-1 (extraction-substrate,
    own parity/fallback validation) + STATS-LIVEGRAPH-IMPL-1 (the cert/fastpath). vs. one combined slice.
  RECOMMENDED: A (data home) + D (sequencing): ratify this spec -> IR-SYMBOL-ATTRIBUTES-1 (extend IrNode + ingest
    + warm-cache; spike producer coverage; PARTIAL/fallback where unavailable) -> STATS-LIVEGRAPH-IMPL-1 (byte-
    preserving cert fastpath). Keeps each slice single-actor (extraction vs query-serving) and validates the
    Layer-0 change independently before the fastpath consumes it.
  BLOCKING_REASON: the cert-fastpath model requires the LiveGraph to compute the FULL stats answer (to compare
    GREEN). The symbol half is structurally uncomputable on today's IR. Until A or B is chosen (and, under D,
    sequenced), the IMPLEMENTATION cannot proceed without inventing an extraction-substrate change + cross-crate
    dependency edge — exactly the architecture decision this spec must NOT invent. The output contract (byte-
    preserving) and the degree-graph half are settled and do NOT depend on this answer; only the symbol-half
    source does.
```

## DECISION_REQUIRED — RESOLVED

STATS-IR-ATTRS is **RATIFIED** (operator sign-off, 2026-06-08): **Option A + D** as
recommended — extend `IrNode` with Layer-0 structural fields, sequenced through a
prerequisite extraction slice before the stats fastpath.

- Prerequisite spec: `docs/slices/ir-symbol-attributes-1.md` (codex-approved).
- Prerequisite implementation: **IR-SYMBOL-ATTRIBUTES-IMPL**, committed `116fbb0` —
  `IrNode` gains `IrVisibility` + `SymbolAttributes` (visibility, top-level/parent,
  symbol kind); scip-ingest extraction with `None` fallback; warm-cache
  `SCHEMA_VERSION 6 -> 7`. Validated: `cargo build` / `cargo test` green; `rmap index`
  reindexed 1142 files.
- Unblocks **STATS-LIVEGRAPH-IMPL-1** (the byte-preserving cert fastpath): the symbol
  half is now computable on the extended IR.
- Output contract remains **byte-preserving** (no migration), per §Output contract.

## References
- `rust/crates/daemon-runtime/src/dispatch.rs` (`handle_stats` :1243-1324 — the eager SQLite default, no engine routing)
- `rust/crates/storage/src/queries.rs` (`compute_module_stats` :1120-1268 — the `nodes`+`edges` query + Rust-side Martin metrics; DTO `ModuleStatsResult`)
- `rust/crates/rgr/src/presentation/stats.rs` (`render_human` :80-161 — the already-canonical per-section re-sort; the byte-preserving basis)
- `rust/crates/repo-graph-ir/src/lib.rs` (`IrNode` :292-308 — NO visibility/parent; `PartitionIr` :331-341; `EdgeType::Imports` :77-83)
- `rust/crates/repo-graph-livegraph/src/lib.rs` (`ModuleImportCycle*` :1773-1803 — the dirname-aggregated module-import substrate fan_in/fan_out reuses; `ValueFact`/`ValueFactKind` :154-190 — complexity-only)
- `rust/crates/daemon-runtime/src/state.rs` (`import_cert` :205 / `cycles_cert` :212 — the in-memory cert storage pattern to mirror)
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (`build_and_store_cycles_cert` :2199 / `cycles_fastpath_or_sqlite` :2220 / `cycles_auto_response` :2247 / `import_cert_fingerprint` reused :2270 — the cert/fastpath to mirror)
- `docs/slices/imports-livegraph-default-fastpath-1.md` (the byte-preserving cert-fastpath precedent + the shared fingerprint)
- `docs/slices/cycles-livegraph-default-fastpath-1.md` + `docs/slices/cycles-output-contract-1.md` (the cert-gated default flip + the output-migration precedent stats answers AGAINST)
- `docs/slices/sqlite-raw-decommission-readiness-7.md` (§Q4(a)/§Q5 — stats is the last SQLite-only drilldown default; spec-first recommendation)
