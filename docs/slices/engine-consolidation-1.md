# ENGINE-CONSOLIDATION-1 — One current-state read engine (SPEC)

Status: SPEC SLICE — analysis + spec only, NO code changes (2026-07-02).
**DELIVERABLE LANDED (builder, 2026-07-16): §3 inventory + §4 ownership
proposal + §5 end-state/milestones + §6 DECISION_REQUIRED — awaiting
decision-review + operator ratification. No code changed.**
**REVISION 1 (builder, 2026-07-16, addressing review-0):** the FC2a-agg
distinction added and applied consistently (review item 1 — `check`/trust/
orient/explain/stats consume the resolved-call COUNT via one shared trust
core [trust/service.rs:875], never FC2a content); M-3 split into M-3a/M-3b
with the trust-core migration fully specified (item 2); D-EC-3 rewritten as
a four-category exhaustive handler classification (item 3); D-EC-6 rewritten
as genuine terminal alternatives (item 4); D-EC-8/C-8 rewritten to preserve
the ratified W-B pinned-reader invariant (item 5); FC2a fallback semantics
split into three regimes (item 6).
**REVISION 2 (builder, 2026-07-16, addressing review-1):** C-3's owner-read
predicate corrected (item 1 — the predicate is zero per-call SQLite
**FC2a/CALLS-row** reads; the FC1 `nodes` `resolve_symbol` read is a
permanent owner-read, consistent with §3.3-A/D-EC-3); the snapshot
copy-count + write-volume model redone against the code and the ratified
retention contract (item 2): steady state retains up to TWO full graph-row
families (current + delta-base — SNAPSHOT-RETENTION-1's keep-set) plus
`baseline_user` marks; refresh peak is up to THREE families until the
queued background retention pass runs and pinned readers drain
(**supersedes revision 1's "≈2×" refresh-peak claim**; itself REFINED by
REVISION 3 below into a two-regime bound — the flat "up to THREE" is
only the nominal single-refresh case); per-refresh SQLite
writes under today's representation are ≈ one full snapshot's graph-family
bytes — copy-forward + full re-resolution [OBSERVED in code] — so the
"KB–MB/refresh" claim is RETRACTED and the storage-representation change
that would enable it is now an explicit D-EC-8 axis (REP-1/REP-2), with
M-6 savings separated from M-7/REP-2 throughout §4.5.
**REVISION 3 (builder, 2026-07-16, addressing review-2):** (1) the raw
extraction stream is a NAMED pipeline-input class, **FC0**
(`extraction_edges` — §3.2 FC0 note, §4.2 row): body-granularity,
pre-resolution, SQLite-owned FOREVER as floor/pipeline input, never
served — resolving the §4.2-R2 ("SQLite plays NO FC2a role") vs §4.5
("CALLS extraction rows remain") collision: R2 now claims no SERVED,
RESOLVED FC2a role, and §2b's internals headline is qualified
accordingly (§4.3-4; ratified via D-EC-1's table + D-EC-2's new FC0
cells). (2) The refresh-peak bound is a TWO-REGIME statement (§4.5
model, C-8, D-EC-8): nominal single-refresh peak = THREE full families
under a NAMED operational precondition (prunable backlog drained at
refresh start); back-to-back refreshes under a starved pass stack
3+K — schema-UNBOUNDED, drained by the first idle-window pass
(supersedes revision 2's flat "up to THREE" transient worst case).
(3) M-3a is re-scoped to the VERIFIED sub-snapshot FC2a-derived
consumers — per-symbol dead-liveness in modules_list/modules_show and
map's file-pair dep sketch, both DISCOVERED this revision (§3.4-10) —
and stats is explicitly OUT (its read-time degree is module-granularity
FC2b/FC1 owner-reads; revision 1's "stats fact-class split … FC2b
derivations replaced" wording retracted); D-EC-7 widened to the three
FC2a-agg granularities.
**REVISION 4 (builder, 2026-07-16, addressing review-3):** (1) the FC0
kernel-scale byte range is RECALCULATED with an explicit three-index
multiplier — revision 3's "≈1.0–2.2 GB/copy" paired the payload-only
product (4.8M × 200–350 B ≈ 0.96–1.68 GB, BEFORE indexes) with an
including-indexes label. Corrected model (§4.5): per-index-B-tree share
derived from the `edges` ×1.8–2.5 anchor (6 B-trees there: 5 named
composites + TEXT-PK autoindex) → `extraction_edges`' 4 B-trees
(3 named [OBSERVED: migration_012.rs:53-55] + autoindex) ≈ ×1.5–2.0
all-in → **≈1.4–3.4 GB/copy** [INFERRED, UNMEASURED], propagated
through §3.2, §4.3-1/-4, §4.5, D-EC-2-A. Knock-on restatement: FC3
(≈1.3–2.4) and FC0 now have OVERLAPPING ranges, so "the floor is the
largest single residual family" is retracted in favour of the
floor-bound PAIR (together ≈2.7–5.8 GB/copy; individual ordering
UNMEASURED). (2) The §3.4-9 grep transcript is corrected to the
reproducible commands (review-3 item 2): the 9-file result is
RUST-SOURCE-ONLY (`--include='*.rs'`); the unrestricted full-tree grep
returns 15 files — both re-run this revision, all 15 hits classified
(6 non-source: 4 rgistr-generated `*_rs_MAP.md` docs, 1 test fixture
JSON, 1 Cargo.toml comment). The zero-dispatch-arm-reader conclusion is
UNCHANGED — now verified over the full tree, not only Rust source.
Track: Focus / consolidation · Origin: fresh-eyes v0.4.0 review ·
Selection: operator-ratified queue item 2 (2026-07-16)

## 1. Problem — two graph engines, every feature pays twice

The daemon serves current-state answers from **two coexisting engines**: the
SQLite pipeline (indexer/storage/repo-index, ~40k LOC in `storage` alone) and
the in-memory **LiveGraph** stack (`repo-graph-ir`, `scip-ingest`,
`trust-model`, `livegraph`, `warm-cache`, `coherence` + two feed adapters,
~13k LOC). `daemon-runtime` depends on both (28 sibling crates);
`livegraph_feed.rs` alone is ~4.7k lines of adapter glue; the W-B epoch work
had to define coherence witnesses *across* the pair (RequestEpoch fingerprint,
fail-soft to pinned SQLite). Per-surface migrations exist as individual
slices (ORIENT-LIVEGRAPH, STATS-LIVEGRAPH, CYCLES-LIVEGRAPH, …) and the
substrate-decommission arc established the permanent floor (**SCIP carries no
unresolved-call disposition → trust unresolved-call fields are RED by design;
full SQLite decommission is impossible** — ratified, Option A bounded
partial). What does NOT exist is a **named end-state**: which engine owns
which read path when the migration is *done*, and what "done" means. Until
that is written down, every new daemon feature pays double integration cost
and the migration has no finish line.

## 2. Deliverable — a ratifiable end-state spec, written into this doc

The builder (analysis only — no code changes) extends this document with:

**§3 Read-path ownership inventory (evidence, not opinion).** For every
daemon request handler (the ~36 in `daemon-runtime/src/dispatch.rs`): which
store(s) it reads today (LiveGraph, SQLite, both — cite the mixed-read
enumeration in `docs/slices/daemon-w-b-epoch-1.md` §7.3), and which fact
classes it needs (resolved graph, unresolved disposition, measurements,
declarations, history/snapshots).

**§4 Fact-class → engine assignment (the proposal).** For each fact class, a
proposed permanent owner, honoring the known floors: unresolved-call
disposition is SQLite-only (RED by design); declarations/governance and
snapshot-scoped history are persistence-shaped; the current-state resolved
graph is LiveGraph-shaped per the VISION's operational architecture. Name
what "both" costs where it must remain (coherence witnesses, epoch binding).

**§5 End-state definition + milestones.** A checkable definition of
"consolidated" (e.g. "no handler reads both stores for the same fact class;
SQLite reads happen only for its owned fact classes or as labeled fail-soft
fallback"), and a milestone sequence from today's state to it, each milestone
independently shippable and smoke-gateable. Include the retirement (or
explicit permanence) of `livegraph_feed.rs`-style double-integration glue.

**§6 Decisions for ratification.** Each ratification-class choice marked
DECISION_REQUIRED with alternatives and trade-offs stated against the VISION
(the three commitments + change-cost doctrine). Expected decisions include at
least: the fact-class ownership table; whether any mixed-read handler remains
permanently mixed; what happens to per-surface `*-LIVEGRAPH-*` slice plans
that the end-state supersedes.

## 2b. Operator direction (2026-07-04) — candidate end-state to evaluate seriously

The operator's proposed split, to be weighed as a primary candidate in §4:

- **SQLite keeps the STRUCTURE skeleton:** modules, files, functions with
  signatures, file→module ownership, and per-function AGGREGATES (fan-in/
  fan-out counts, complexity value) — the slow-changing, small,
  orient/stats/hotspots-serving layer.
- **LiveGraph owns function INTERNALS:** body-level call sites and edge
  lists (what callers/callees/path walk) — the fast-changing, blob-heavy,
  per-file-rebuildable layer, persisted only via the warm cache.
- **Snapshot degrades to a provenance stamp on the current state** (identity
  for comparability/toolchain/epochs), not retained copies —
  SNAPSHOT-RETENTION-1 already enforces current + delta-base only.

Named collision the spec MUST resolve, not skirt: the RED floor (unresolved-
call disposition is SQLite-only, ratified) is a body-level fact class. Either
disposition rows stay behind as a compact SQLite exception (size the cost),
or re-opening the floor is proposed as an explicit DECISION_REQUIRED with the
persistence story (files-are-system-of-record applies to the warm cache too).
Also size the win: estimate DB footprint and per-refresh write volume under
this split for a kernel-scale repo vs today.

---

# DELIVERABLE (builder analysis, 2026-07-16) — §3–§6 per §2

> Evidence law (`agent_docs/validation.md`): **EXECUTED** = command run this
> slice, output observed. **OBSERVED** = artifact/code read first-hand this
> slice. **INFERRED** = synthesis over OBSERVED facts, basis stated.
> **NOT RUN** = skipped, with reason. Tree at authoring: HEAD `2e69226`
> (TS-PROTOTYPE-RETIREMENT-1 delivery record, above `800d78e` — the retire
> commit; the inventory below is of the CURRENT tree) [EXECUTED: `git log`].
> Handler traces were produced by four scoped read-only tracing passes over
> `rust/crates/{daemon-runtime,storage,agent,trust,gate,...}` and the docs
> corpus; every load-bearing claim (dispatch arm count, mixed-read set,
> `path` default, orient/check serving shape, schema row shapes, floor
> contract) was re-verified first-hand and is cited to file:line. Table-level
> traces for the long tail are labelled OBSERVED-BY-TRACE (subset-honest:
> traced through named storage entry points, not exhaustively re-read line
> by line).

## 3. Read-path ownership inventory (evidence, not opinion)

### 3.1 Handler count — stated and reconciled

**The dispatcher surface is ONE match statement**:
`rust/crates/daemon-runtime/src/dispatch.rs:282-413` (`ServiceDispatcher::
dispatch`). `daemon-transport/src/dispatch.rs` defines only the `Dispatcher`
trait — no second handler set [OBSERVED]. Top-level method arms: **66**
[EXECUTED: `sed -n '282,413p' … | grep -c '^\s*"[a-z_]*" =>'` → 66].

Reconciliation against the two prior enumerations:

- **This doc's §2 said "~36"** (authored 2026-07-02). The real count then was
  already higher; today it is 66. The delta is not new invention: §2's figure
  undercounted the long tail (contracts/boundaries/surfaces/modules/deps
  families are 19 arms alone) and predates `map` (MAP-FROM-INDEX-1, v0.7.0).
  The "~36" is hereby corrected to the reconciled 66. [OBSERVED]
- **`daemon-w-b-epoch-1.md` §7.3** (2026-06-29, AUTHORITATIVE for the flip)
  classified **50** arms: 10 MIXED-READ + 35 SQLITE-ONLY + 5 WRITERS. The 16
  arms it does not name: `ping`/`echo` (test stubs, no store) plus 14 arms
  (`check`, `map`, `churn`, `hotspots`, `risk`, `coverage`, `assess`,
  `violations`, `policy`, `classify_retention`, `mark_baseline`,
  `unmark_baseline`, `perf`, `storage_health`). §7.3's *verdict* ("the
  mixed-read set is CLOSED at ten") survives this gap because its method —
  trace every reader of the LiveGraph FIELD crate-wide, not grep dispatch.rs
  — implicitly covers all arms: none of the 14 reaches a LiveGraph-reading
  module. Re-verified on TODAY's tree [EXECUTED: `grep -rn livegraph
  rust/crates/daemon-runtime/src/handlers/` → zero hits; `grep -rl
  "repo_state\.livegraph|\.livegraph\.read()|state\.livegraph"
  daemon-runtime/src/` → the known serving/cert/coherence modules only;
  `check_coherence.rs` NOT among them]. **The mixed-read set today is the
  same ten** — `map` (the one raw-graph-reading arm added since) reads
  SQLite only.

**Two different "ten"s — do not conflate** (a live naming trap [INFERRED
over OBSERVED docs]): the *migration ten* of the readiness docs (callers,
callees, path, imports, cycles, stats, orient, **check**, explain, trust —
the tracked read-command surface; "6/10 served-free") is NOT the *mixed-read
ten* of W-B §7.3 (same minus check, plus **cycle_completeness_audit**).
`check` reads SQLite only (`dispatch.rs:3861-3993`: `run_check_cancellable(
&storage,…)`; envelope adds MEET-freshness labels from `get_stale_files` —
"check has NO LiveGraph leaf" [OBSERVED first-hand]); the audit reads both
stores by design (it is a comparator). Both sets have cardinality 10 by
coincidence.

### 3.2 Fact-class taxonomy (the vocabulary §4 assigns owners to)

Grounded in the 33-table inventory (`sqlite-raw-decommission-readiness-1.md`
§1 [OBSERVED]), the packet's five classes, and §2b's skeleton/internals
split. Layer references: `agent_docs/architecture.md` Product Layer Stack.

| FC | Fact class | Today's store(s) | Layer |
|---|---|---|---|
| **FC0** | **Extraction stream (pipeline input)** — durable PRE-resolution reference rows: source symbol uid, RAW `target_key` expression (not a resolved endpoint), edge kind, body-level source location, extractor, metadata (`extraction_edges`) — body-granularity but NEVER served; consumed only by index/refresh-time passes (named in revision 3 — see the FC0 note below) | SQLite ONLY (persistent, snapshot-scoped, CASCADE, 3 indexes; copy-forwarded on delta) [OBSERVED: migration_012.rs:37-55] | 0 (pipeline input — not a read-path class) |
| **FC1** | **Structure skeleton** — file inventory (`files`, `file_versions`), declaration symbols with signatures (`nodes`: kind/name/qualified_name/signature/doc_comment/positions), module catalog + file→module ownership (`module_candidates`+`_evidence`, `module_file_ownership`), manifest roots | SQLite; LiveGraph holds a *projection* (IrNode identity + attributes) for its own serving | 0–2 |
| **FC2a** | **Function internals / resolved call graph — CONTENT** — symbol-level CALLS adjacency, per-edge rows, body-level call sites (what callers/callees/path WALK) | SQLite `edges` (type CALLS); LiveGraph `IrEdge::Calls` + xref (TS-covered partitions only); warm cache `.rgr/warm-cache/*.cache` | 0–1 |
| **FC2a-agg** | **FC2a-DERIVED read products at coarser-than-edge granularity** — a distinct consumption mode from FC2a content: consumers need the derived value, never the rows. THREE granularities (revision 3 completes the census): **(g1)** snapshot-level resolved-call count (trust's `resolved_calls`); **(g2)** per-symbol incoming-reference liveness/degree — §2b's per-function fan-in/fan-out, TODAY consumed as the CALLS share of dead-liveness; **(g3)** per-file-pair resolved dependency rows — the CALLS share of map's dep sketch | No persisted row today — all three are READ-time mechanisms over `edges` CALLS rows: g1 = `COUNT` (`trust/service.rs:875` `count_edges_by_type(snapshot_uid, "CALLS")`), consumed by trust/check/orient/explain/stats through ONE shared trust core (§3.4-8); g2 = `NOT IN` membership over CALLS∪7 relation types in `find_dead_nodes` [OBSERVED: queries.rs:1031-1060], served by the modules_list/modules_show dead rollups [OBSERVED: dispatch.rs:7862/:7595]; g3 = `type IN ('IMPORTS','CALLS')` collapsed to DISTINCT file pairs in `map_resolved_dep_edges_in_path` [OBSERVED: queries.rs:2615-2640] (§3.4-10) | 1 (derived, deterministic) |
| **FC2b** | **Structure relations** — file-level IMPORTS edges, resource READS/WRITES edges (what imports/cycles/violations/resources walk), PLUS — named in revision 3 so the M-6 boundary is exact — every other relation-typed `edges` row: IMPLEMENTS / INSTANTIATES / ROUTES_TO / REGISTERED_BY / TESTED_BY / COVERS [OBSERVED: the liveness query's type list, queries.rs:1058] and the Phase-4 module rows (OWNS, MODULE→MODULE IMPORTS). **FC2b = all `edges` rows EXCEPT symbol-level CALLS** | SQLite `edges`; LiveGraph import observations (`live_import_view`, TS only) | 0–1 |
| **FC3** | **Unresolved-reference disposition** — per-site unresolved calls/imports + classification/category/basis (`unresolved_edges`), `extraction_diagnostics_json` aggregate | SQLite ONLY — **the ratified RED floor** (`sqlite-raw-decommission-1.md` Clause 3) | 1 |
| **FC4** | **Measurements / quality signals** — complexity, churn-derived, coverage (`measurements`), `quality_assessments` | SQLite authoritative; LiveGraph value-facts sidecar = complexity-only serving cache (TS) | 1–2 |
| **FC5** | **Declarations / governance** — waivers, requirements, boundary declarations (`declarations`), policy facts (`status_mappings`, `behavioral_markers`, `return_fates`) | SQLite; AUTHORITATIVE, non-reproducible | 4 (policy facts 1) |
| **FC6** | **Derived architecture & hints** — surfaces, boundaries, contracts, `semantic_facts`, `inferences`, `generated_code_mappings` | SQLite; no LiveGraph model | 2–3 |
| **FC7** | **Docs inventory** — the documents themselves + extracted semantic facts | Filesystem PRIMARY (`doc_facts::discover_doc_inventory` walks the repo) + SQLite `semantic_facts` projection | 1 |
| **FC8** | **History / provenance / operational** — snapshot state+retention+toolchain (`snapshots`), repo registry (`repos`, `registry.json`), activity/enrichment runtime state, git history (churn) | SQLite + registry.json + in-memory + **git** (git owns history — VISION) | op |

**Why FC2a-agg is a named class and FC2b-derived aggregates are not**
(decide-and-record; review-0 item 1 asked for the explicit call): the trust
core ALSO derives module fan-in/fan-out and module cycles at read time from
FC2b IMPORTS rows (`trust_impl.rs:523-587` `compute_module_stats`;
`:458-521` `find_path_prefix_module_cycles`). Those stay UNNAMED because no
ownership question arises: FC2b rows stay SQLite-owned (D-EC-5-A), so
read-time derivation over them is a permanent owner-read. FC2a-agg needs a
name because its SOURCE rows are scheduled to drop per-language (M-6) —
after a drop, a read-time COUNT over the remaining rows would silently
undercount. The classification decision: **resolved-call counts are
persisted FC4-shaped aggregates at the end-state** (owner-written at
index/refresh, D-EC-7), not FC2a content; today's COUNT-over-rows is the
read-time MECHANISM that M-3b retires. [OBSERVED: service.rs:858-917;
trust_impl.rs:458-587; agent_impl.rs:320-344]

**Why FC0 is a named class (revision 3; review-2 item 1).**
`extraction_edges` is body-granularity (per-call-site rows with source
locations) yet neither FC2a content (its `target_key` is a RAW
pre-resolution expression, not a resolved endpoint — callers/callees/path
never walk it) nor FC3 alone (it is the INPUT the resolution loop splits
into resolved FC2a/FC2b output AND FC3 disposition [OBSERVED:
resolver.rs:135-197 `resolve_edges` — every row lands as `ResolvedEdge`
or `CategorizedUnresolvedEdge`; orchestrator.rs:804-972]). Classification
decision: **a separately named PIPELINE-INPUT class — not a read-path
class** (the reviewer's third alternative). Its contract, per field:

- **Terminal owner:** SQLite, permanent, ALL languages — the input that
  keeps the FC3 floor classifiable and every FC2a-agg granularity
  language-complete under delta refresh (resolution re-runs over ALL FC0
  rows of the new snapshot, copied-forward + fresh). Ratified through
  the D-EC-1 table row + D-EC-2's FC0 cells (A retains; D prices the
  rejected narrowing) — never by re-opening the RED floor.
- **Read/write purpose:** WRITTEN by extraction (changed files) + delta
  copy-forward (unchanged files) [OBSERVED: indexer_impl.rs:654-1018];
  READ only by index/refresh-time passes — the resolver batch
  [orchestrator.rs:804-972] and the GR-1A gRPC hint detectors, which
  read IMPLEMENTS/CALLS-typed FC0 rows during the pipeline
  [OBSERVED: refresh_dispatch.rs:134; grpc_impl_hint_impl.rs:76/:215/
  :518]. ZERO dispatch arms read its content [EXECUTED: full-tree grep,
  every hit classified — §3.4-9]; `perf` counts rows content-free
  (tier map [OBSERVED: metrics.rs:123 — Tier B, layer 0-1]).
- **Refresh behavior:** copy-forward + fresh insert + whole-snapshot
  re-read by the resolver each refresh; no DELETE path except CASCADE
  on snapshot prune [EXECUTED: grep, revision 2].
- **Duplication cost:** after M-6-for-L, body-granularity call-site
  facts for L exist TWICE, permanently — FC0 raw rows in SQLite
  (≈1.4–3.4 GB/copy at kernel scale — §4.5 revision-4 recalculation)
  and FC2a resolved adjacency
  in LiveGraph/warm cache. Priced as floor rent: every alternative is
  foreclosed by a ratified contract (D-EC-2 cells C/D).
- **Effect on "LiveGraph owns body-level call sites":** the honest form
  is "LiveGraph owns the SERVED, RESOLVED body-level call graph for
  covered languages" — SQLite permanently retains the RAW body-level
  extraction stream. §2b's internals headline is qualified, not
  falsified (§4.3-4).

### 3.3 The inventory — every arm, stores read TODAY, fact classes needed

Grouping key: **A** mixed-read (both stores, epoch-pinned) · **B**
SQLite-only readers that touch raw-graph tables (`nodes`/`edges`/
`unresolved_edges`) · **C** SQLite readers, non-graph tables ·
**D** registry/in-memory/no store · **E** writers. 10+16+20+7+13 = **66** ✓.

**A. Mixed-read — the W-B ten (epoch witnesses per §7.3)** [OBSERVED
first-hand: orient `dispatch.rs:3680-3738`, path Auto arm
`livegraph_feed.rs:940-951`; remainder OBSERVED-BY-TRACE, spot-verified]

| Handler | Default serving path TODAY | Eager SQLite reads (every call) | SQLite tables (eager ∪ fallback) | Fact classes |
|---|---|---|---|---|
| callers | callgraph-cert-gated LG fastpath; **lazy** SQLite fallback (`callers_engine_response`) | `resolve_symbol` → `nodes` (eager); `find_direct_callers` only on fallback | nodes, files; fallback + edges | FC1 (resolve), FC2a |
| callees | same shape | same | same | FC1, FC2a |
| path | **PINNED-SQLITE default** — Auto serves SQLite; NO LG fastpath (no CALLS∪IMPORTS cert; W-B-EPOCH-IMPL-2A, ratified D-CC refinement) [OBSERVED: `livegraph_feed.rs:943-951`]. LG BFS only behind `--engine livegraph/compare` | 2× `resolve_symbol` + `find_shortest_path` (recursive CTE) | nodes, edges, files | FC1, FC2a, FC2b (BFS walks CALLS∪IMPORTS) |
| imports | import-cert-gated LG fastpath (`imports_auto_response`) + labelled fallback | `node_exists` → `nodes` (eager); listing lazy | nodes; fallback + edges, files | FC1, FC2b |
| cycles | cycles-cert-gated LG module-cycle fastpath + labelled fallback | none eager beyond snapshot resolve | fallback: edges, nodes, files | FC2b, FC1 |
| stats | module-rows leaf cert-gated LG; **summary layer eager SQLite every call** (`compute_repo_summary` [FC1 only: file_versions/nodes/files — agent_impl.rs:243-302], trust overlay = the full trust core §3.4-8, manifest roots) | nodes, files, file_versions, edges, unresolved_edges, module_candidates | + declarations, measurements | FC1, FC2a-agg, FC2b (via trust core), FC3, FC4, FC5 |
| orient | eager SQLite base + LG (b)-leaves via `OrientServeDecorator`+`CoherenceEnvelope`; **SYMBOL-focus on GREEN is `nodes`-free** (COHERENCE-LEAF-SERVE, `ccaad68`/`765583b`) [OBSERVED: `dispatch.rs:3680-3738`] | MODULE_SUMMARY + trust + cycle values always SQLite | repos, nodes, files, file_versions, edges, unresolved_edges, declarations, module_candidates(+evidence), module_file_ownership, measurements, file_signals, snapshots | FC1, FC2a (bounded SYMBOL-focus caller leaves, fallback), FC2a-agg (trust leaf), FC2b, FC3, FC4, FC5, FC8 |
| explain | same decorator + envelope; SYMBOL-focus `nodes`-free on GREEN (`9e6077c`), **never edges-free** (trust + cycles stay SQLite) | trust contributor + cycles | ≈ orient set | FC1, FC2a (relevance-ranked caller leaves, fallback), FC2a-agg (trust contributor [explain/mod.rs:855]), FC2b, FC3, FC4, FC5 |
| trust | eager SQLite v1 report (Half-B) + LG posture leaf (Half-A) in envelope — **the ratified permanent two-source hybrid** | `assemble_trust_report` every call (incl. the `resolved_calls` COUNT scan [service.rs:875] + module stats/cycles over IMPORTS [trust_impl.rs:458-587]) | nodes, edges, unresolved_edges, declarations, module_candidates, module_file_ownership, module_files, file_stats, file_signals, snapshots | FC1, FC2a-agg, FC2b (module stats/cycles), **FC3**, FC5, FC8 |
| cycle_completeness_audit | COMPARATOR over both stores by design; `AuditEpoch` identity witness (NOT GREEN-eligibility — §7.3) | both every call | edges, nodes, files, module_candidates, module_file_ownership | FC2b, FC1 |

**B. SQLite-only readers touching raw-graph tables (16)** — the finding
that reshapes §4: raw-graph readership is FAR broader than the migration
ten [OBSERVED-BY-TRACE through named storage entry points]

| Handler | Raw-graph reads | Other tables | Fact classes |
|---|---|---|---|
| map | **nodes + edges + unresolved_edges** (`map_symbols/`, `map_resolved_dep_edges_`, `map_unresolved_imports_in_path`) — the one NEW raw-graph consumer since §7.3 (v0.7.0). Revision-3 correction: the dep sketch reads `type IN ('IMPORTS','CALLS')` collapsed to file pairs [OBSERVED: queries.rs:2615-2640] — a CALLS-row read the earlier rows omitted (§3.4-10) | files, file_versions, measurements, module_candidates(+evidence), snapshots | FC1, FC2b, **FC2a-agg g3** (dep-sketch CALLS share; mechanism M-3a re-homes), FC3, FC4 |
| gate | nodes + edges (`find_imports_between_paths`, module-violation eval) | declarations, measurements, inferences, quality_assessments, module_candidates, module_file_ownership, files, snapshots | FC5 (primary), FC2b, FC4, FC6, FC1 |
| check | via trust core (§3.4-8): the `resolved_calls` COUNT over CALLS rows [service.rs:875 → check/mod.rs:134] + module stats/cycles over IMPORTS + unresolved counts; `get_stale_files` (files/file_versions); requirements | declarations, module_candidates, module_file_ownership, file_signals, snapshots | FC1, FC2a-agg (never FC2a content), FC2b (module stats/cycles), **FC3**, FC5, FC8 |
| violations | nodes + edges (`load_module_graph_facts` → `get_resolved_imports_for_snapshot`, IMPORTS-typed) | declarations, module_candidates, module_file_ownership, files, snapshots | FC2b, FC1, FC5 |
| resource_list / resource_readers / resource_writers | nodes + edges (READS/WRITES-typed; `list_resources`, `find_resource_readers/writers`) | files, snapshots | **FC2b**, FC1 |
| modules_deps / modules_violations / modules_list | nodes + edges (IMPORTS-typed via `load_module_graph_facts`; **list also `find_dead_nodes` — revision 3: that is a liveness `NOT IN` over CALLS∪7 relation types, i.e. a CALLS-row read** [OBSERVED: dispatch.rs:7862 → queries.rs:1031-1060; §3.4-10]) | module_candidates, module_file_ownership, files, declarations, inferences, file_versions, snapshots | FC2b, FC1, **FC2a-agg g2** (list's dead rollup; mechanism M-3a re-homes), FC5, FC6 |
| modules_show | nodes + edges + **unresolved_edges** (trust overlay `compute_trust_overlay_for_snapshot`; **revision 3: also `find_dead_nodes` for its dead rollup — the same CALLS∪7-type liveness read as modules_list** [OBSERVED: dispatch.rs:7595; §3.4-10]) | + module_candidate_evidence, file_signals | FC1, FC2b, **FC2a-agg g2**, **FC3**, FC5, FC6 |
| deps_list / deps_why / deps_drift | nodes + **unresolved_edges** (external-import attribution: `get_external_imports_*`) — NO `edges` | module_candidates, module_file_ownership, files, file_signals, file_versions, snapshots | **FC3** (attribution consumer), FC1 |
| hotspots / risk | nodes (complexity/coverage joins via `query_complexity_by_file`) | files, measurements, snapshots | FC4, FC1, FC8 (git churn) |

**C. SQLite readers, non-graph tables only (20)** [OBSERVED-BY-TRACE]

| Handlers | Tables | Fact classes |
|---|---|---|
| contracts_list / contracts_show / contracts_elements / contracts_usages | contract_schemas, contract_elements, generated_code_mappings, snapshots | FC6 |
| boundaries_list / boundaries_show / boundaries_summary / boundaries_links | boundary_interaction_surfaces, boundary_channel_details, boundary_contracts, boundary_interaction_links, contract_elements, snapshots | FC6 |
| surfaces_list / surfaces_show | project_surfaces(+evidence), module_candidates, snapshots | FC6, FC1 |
| inferences_list | inferences, snapshots | FC6 |
| policy | one of status_mappings / behavioral_markers / return_fates, snapshots | FC5 |
| modules_files | module_candidates, module_file_ownership, files, snapshots | FC1 |
| modules_unowned | file_versions, module_file_ownership, module_candidates, snapshots (never nodes/edges) | FC1 |
| docs_list | repos + FILESYSTEM walk (`discover_doc_inventory`) | FC7 |
| churn | files, snapshots + **git history** | FC8, FC1 |
| perf | `sqlite_master` + COUNT(*) over all tables (count-only, no content), dbstat, snapshots | FC8 |
| storage_health | snapshots (+diagnostics), retention stats | FC8 |
| repo_info | snapshots via `collect_snapshot_facts` | FC8 |
| load_repo | repos (open-validation only) | FC8 |

**D. Registry / in-memory / no store (7):** ping, echo (none);
daemon_info (registry.json + RSS + activity state); unload_repo,
list_loaded_repos (in-memory map); resolve_repo, list_repos
(registry.json). Fact class FC8. [OBSERVED-BY-TRACE]

**E. Writers (13)** — serialize under the coordinator; reads noted:
index, refresh, enrich (the pipeline: write **FC0**-FC4+FC6, FC8;
index/refresh also RE-READ FC0 — the resolver batch + the GR-1A hint
detectors, §3.2 FC0 note);
livegraph_preload, livegraph_refresh (LiveGraph swap + warm-cache write;
read SQLite registry/snapshots for identity); coverage (writes
measurements; reads files) ; assess (writes quality_assessments; reads
measurements+policies); docs_extract (writes semantic_facts; reads FS);
classify_retention, mark_baseline, unmark_baseline (write
snapshots.retention_class; prune+VACUUM); repo_alias, repo_remove
(write registry.json). [OBSERVED-BY-TRACE; retention trio + coverage +
assess confirmed writers — they take write locks]

### 3.4 What the inventory forces (findings)

1. **Raw-graph readership is 26/66 arms, not "the graph commands".**
   10 mixed + 16 SQLite-only arms read `nodes`/`edges`/`unresolved_edges`
   content. Any owner assignment for FC2 that strands SQLite readers
   strands gate, map, resources, modules, deps, violations, check — none
   of which has (or should need) LiveGraph plumbing. [OBSERVED §3.3]
2. **`edges` is two fact classes wearing one table.** Symbol-level CALLS
   adjacency (FC2a — what callers/callees walk; blob-heavy, fast-churn)
   vs file-level IMPORTS + resource READS/WRITES rows (FC2b — what
   violations/resources/map/cycles walk; small, slow-churn,
   structure-shaped). Every B-group `edges` reader reads FC2b CONTENT;
   none WALKS FC2a adjacency. **(Revision-3 correction: iterations 0–2
   implied the B group's only FC2a dependency was `check`'s count —
   WRONG by evidence: modules_list/modules_show's dead-liveness
   membership and map's dep sketch READ CALLS rows without walking
   them — the complete census is finding 10.)** `check` consumes the
   FC2a-DERIVED snapshot count through the shared trust core
   (`count_edges_by_type(CALLS)` [OBSERVED: service.rs:875 →
   check/mod.rs:134]). That consumption mode is what §3.2 names FC2a-agg
   (granularity g1), and M-3b re-homes it. §2b's "LiveGraph owns edge
   lists" is exactly
   right for FC2a content and wrong-by-evidence for FC2b's consumers.
   [INFERRED over §3.3; edge types OBSERVED in the tracing:
   `e.type='IMPORTS'` at `queries.rs:2748`, `module_edges_support.rs:41`;
   READS/WRITES in resource queries]
3. **`unresolved_edges` has TWO consumer families**: the trust
   contributor (the ratified (c) floor) AND the deps/attribution family
   (`deps_*`, ATTRIBUTION-1's named external deps) + map's unresolved
   imports. The floor is not a trust-only appendix; it is a served
   discovery surface. Any floor re-shaping touches deps/map too.
   [OBSERVED §3.3 rows]
4. **`path`'s default is pinned SQLite today** — the readiness-9/-10 /
   CURRENT_SLICE "6/10 served-free (callers/callees/path lazy)" ledger is
   STALE on path: the served-free-on-green count is **5** of the
   migration ten, plus orient/explain SYMBOL-focus bounded leaves. This
   was a RATIFIED refinement (W-B-EPOCH-IMPL-2A / D-CC: no CALLS∪IMPORTS
   no-loss cert → an LG BFS could be as-of a different epoch than its
   stamp), not drift — but the ledgers were never corrected.
   [OBSERVED: `livegraph_feed.rs:940-951`]
5. **Comment-vs-behavior defect (surfaced, not fixed — code out of
   scope):** `dispatch.rs:2342-2345` still says "PATH-LIVEGRAPH-DEFAULT-1
   migrated path's Auto to LiveGraph-first; the prior comment was STALE"
   — the authoritative Auto arm it delegates to serves SQLite. The
   comment now misleads exactly the way it once corrected.
   [OBSERVED both sites first-hand]
6. **`stats` is the widest mixed handler** — its module-rows leaf is
   LG-served on green while its summary layer eagerly reads six fact
   classes from SQLite every call (FC1, FC2a-agg, FC2b, FC3, FC4, FC5).
   Consolidation for stats is a fact-class split, not an engine flip.
   [OBSERVED-BY-TRACE §3.3-A]
7. **The double-integration tax, measured:** daemon-runtime depends on
   **35** `[dependencies]` crates (28 internal path deps) [EXECUTED:
   Cargo.toml grep]; the LiveGraph adapter/witness layer inside
   daemon-runtime (livegraph_feed.rs 4,726 + certs + coherence + serve
   decorators + their tests) is **~15.7k LOC** [EXECUTED: `find … wc -l`
   = 15,739]; the LiveGraph stack crates total ~12.2k LOC; `storage` is
   44,237 LOC [EXECUTED]. Every mixed surface pays: RequestEpoch capture
   + fingerprint validation + a per-surface no-loss cert + a fallback
   arm + (during migration) a compare/readiness harness.
8. **One trust core feeds five surfaces.** trust, check, orient, explain
   and stats consume `resolved_calls` (FC2a-agg) + the FC2b-derived
   module stats/cycles + the FC3 classification counts through ONE
   assembly: `assemble_trust_report[_cancellable]` — the agent port
   `get_trust_summary` merely projects its output [OBSERVED first-hand:
   service.rs:858-917; agent_impl.rs:320-344; consumers
   check/mod.rs:134, explain/mod.rs:855, aggregators/trust.rs:55]. Two
   consequences: (i) until M-3b lands, all five surfaces carry an eager
   read-time COUNT scan over CALLS rows on every call; (ii) the
   FC2a-agg re-home is ONE producer change + ONE read swap in that
   core — not five per-surface migrations.
9. **`extraction_edges` (FC0) has ZERO dispatch-arm readers.** Its
   readers are all index/refresh-time: the resolver batch
   [orchestrator.rs:804-972 → resolver.rs:135-197], the GR-1A gRPC
   hint detectors [refresh_dispatch.rs:134;
   grpc_impl_hint_impl.rs:76/:215/:518], and the delta copy-forward
   [indexer_impl.rs:654-1018]; `perf` counts rows content-free.
   [EXECUTED: full-tree grep for `extraction_edges`, all 15 hits
   classified — revision-4 ledger; 9 Rust-source + 6 non-source, none
   executable] This is the basis for classifying it a PIPELINE-INPUT
   class rather than a read-path class (§3.2 FC0 note) — resolving
   review-2 item 1 without touching the floor.
10. **The complete read-time CALLS-row consumer census (revision 3 —
   the M-6 blast list).** Beyond the FC2a adjacency walkers
   (callers/callees fallback, orient/explain SYMBOL-focus caller
   leaves, path's default BFS), exactly THREE read-time mechanisms
   consume `edges` CALLS rows: **(i)** the trust core's snapshot COUNT
   [service.rs:875-877] — five surfaces (finding 8; g1 → M-3b);
   **(ii)** dead-liveness membership (`find_dead_nodes`, `NOT IN` over
   CALLS∪7 relation types [queries.rs:1031-1060]) served by the
   modules_list + modules_show dead rollups [dispatch.rs:7862/:7595 —
   the rollup is served output, `DeadNodeFact` per module]; the
   withdrawn dead-code aggregators return empty WITHOUT querying, so
   orient/explain do NOT consume this [OBSERVED:
   aggregators/dead_code.rs:42-80 — `Ok(AggregatorOutput::empty())`
   unconditionally; explain/mod.rs:843 "surface withdrawn"; the `dead`
   CLI is disabled, no dispatch arm] (g2 → M-3a); **(iii)** map's dep
   sketch (`map_resolved_dep_edges_in_path`, IMPORTS+CALLS collapsed
   to DISTINCT file pairs [queries.rs:2615-2640]) (g3 → M-3a).
   CONSEQUENCE for M-6: a per-language CALLS-row drop silently flips
   (ii)'s liveness answers — a symbol whose only incoming references
   are calls from language-L files reads as falsely "dead" — and
   silently thins (iii)'s file-dependency sketch. Both MUST be re-homed
   onto persisted FC2a-agg families or explicitly degrade-ratified
   BEFORE M-6's first drop (M-3a ≺ M-6; D-EC-7 g2/g3). [EXECUTED:
   grep for CALLS-typed reads across storage/src, every non-test hit
   classified]

## 4. Fact-class → engine assignment (the proposal)

### 4.1 The floors this assignment is built under (ratified; not re-opened)

- **The RED floor** [OBSERVED: `sqlite-raw-decommission-1.md` Clause 3,
  ratified 2026-06-14]: `unresolved_edges` + `extraction_diagnostics_json`
  are retained + SQLite-labelled FOREVER; the trust contributor is
  permanently two-source. Nothing below proposes retiring it.
- **Gate 2 ceiling** [OBSERVED: readiness-10]: LiveGraph is TS-only;
  every non-TS file/repo falls back to SQLite. Any "LiveGraph owns X"
  claim is language-conditional until per-language SCIP ingest lands.
- **W-B epoch invariant** [OBSERVED: `daemon-w-b-epoch-1.md` §3/§10]:
  wherever one request reads both stores, it pins `(snapshot_uid,
  fingerprint)` once; eviction degrades to the pinned SQLite snapshot,
  never a mixed epoch.
- **Warm cache is non-authoritative** [OBSERVED:
  `partitioned-warm-cache-arch-1.md`; `livegraph_warm_cache.rs`]:
  disposable, validated-before-load, best-effort — never a system of
  record.

### 4.2 The ownership table (proposal — D-EC-1 ratifies)

"Owner" = the store a READ surface must consult for that class; "serving
cache" = may serve on GREEN with a witness, never authoritative.

| FC | Class | Proposed permanent owner | LiveGraph role | Basis (§3 evidence) |
|---|---|---|---|---|
| FC0 | Extraction stream (pipeline input) | **SQLite FOREVER, all languages** — floor/pipeline input, never served (§3.2 FC0 note; D-EC-2 cells A/D) | none (LiveGraph ingests SCIP producer output, not FC0) | zero dispatch-arm readers (§3.4-9); its retention is what keeps FC3 classification and every FC2a-agg granularity language-complete after per-language M-6 drops — dropping it per-language would break delta re-resolution (D-EC-2-D, rejected) |
| FC1 | Structure skeleton | **SQLite** (§2b) | projection for its own serving (IrNode identity/attributes); orient SYMBOL-focus leaves may serve on GREEN cert | read by 40+ arms incl. every B/C-group handler; slow-churn, small; the floor's FK anchor (`unresolved_edges.source_node_uid` → `nodes`) [OBSERVED: migration_007] |
| FC2a | Function internals — symbol-level CALLS content | **Three regimes, per language** (review-0 item 6 — ownership and fallback stated separately): **(R1) covered L, migration-time (pre-M-6-for-L):** LiveGraph owns on GREEN; the retained SQLite CALLS rows are the labelled, epoch-pinned fail-soft fallback. **(R2) covered L, terminal (post-M-6-for-L):** LiveGraph + warm cache is the ONLY FC2a store for L — the ladder is warm-cache load → producer rebuild → if the producer is unavailable, a labelled degraded answer (Partial/Unavailable + the concrete remediation, e.g. "re-run index"; never silent-empty, never zero-for-unknown). SQLite plays NO **SERVED, RESOLVED** FC2a role for L — no resolved CALLS rows exist for L; the RAW body-level extraction stream for L (FC0, §3.2 note) is retained permanently as pipeline input and is not servable adjacency (revision-3 precision — this is the review-2 item-1 collision, resolved). **(R3) non-covered L:** SQLite `edges` CALLS rows are the OWNER (not a fallback), permanently until SCIP ingest for L lands (gate 2) | THE owner on covered GREEN (R1/R2) | FC2a CONTENT walkers: callers/callees/path + orient/explain's bounded SYMBOL-focus caller leaves (§3.3-A); snapshot COUNTS are FC2a-agg (next row), NOT content; kernel-scale churn + size concentrate here (§4.5) |
| FC2a-agg | FC2a-derived read products, three granularities (§3.2): g1 snapshot totals (trust's `resolved_calls`), g2 per-symbol degree/liveness (§2b's fan-in/fan-out), g3 per-file-pair dep rows (map's sketch) | **SQLite, persisted at index/refresh time as FC4-shaped owner-written rows (D-EC-7; g1 built at M-3b, g2/g3 at M-3a)**. Today: all three computed at READ time over CALLS rows — COUNT [service.rs:875], liveness NOT-IN [queries.rs:1031-1060], file-pair collapse [queries.rs:2615-2640] — the mechanisms M-3a/M-3b retire. Persisted values are computed from the FULL resolution stream (all languages — FC0 input, D-EC-2), so they stay correct after per-language row drops | none — an LG-side derivation would be residency- and language-bounded (an overclaim under partial residency); the persisted full-stream values are deterministic | g1: five consumers through ONE core (§3.4-8); g2: modules_list/modules_show dead rollups, g3: map's sketch (§3.4-10); pairs with the FC3 unresolved counts in the resolution rate — the numerator stays single-store beside its denominator |
| FC2b | Structure relations — file-level IMPORTS, resource READS/WRITES | **SQLite** (recommended — D-EC-5 offers the alternative) | `live_import_view` remains a GREEN serving cache for imports/cycles surfaces | 10 SQLite-only arms read FC2b (gate, violations, modules_deps/violations/show/list, resources×3, map) — moving FC2b out would force LiveGraph plumbing into structure handlers that today need none (§3.4-1/2) |
| FC3 | Unresolved-reference disposition | **SQLite FOREVER** (the ratified floor) | none possible (SCIP carries no unresolved-call disposition — probe NO-GO) | Clause 3 [OBSERVED]; dual consumers: trust + deps/attribution/map (§3.4-3) |
| FC4 | Measurements / quality | **SQLite** | value-facts sidecar = complexity-only serving cache (TS), epoch-bound | written by index postpass + `coverage`; read by stats/hotspots/risk/orient/gate/assess (§3.3); no LG model for churn/coverage |
| FC5 | Declarations / governance | **SQLite** | none | authoritative, non-reproducible (readiness-1 §1a); governance surfaces frozen (VISION) |
| FC6 | Derived architecture & hints | **SQLite** | none | no LiveGraph model (readiness-1: "the other 31 tables"); out of consolidation scope |
| FC7 | Docs inventory | **Filesystem primary + SQLite `semantic_facts` projection** | none | architecture rules 4/7 (docs are primary; DB projections secondary) [OBSERVED] |
| FC8 | History / provenance / operational | **SQLite `snapshots` degraded to a provenance stamp** (§2b leg 3; D-EC-8) + `registry.json` + **git for history** | none | SNAPSHOT-RETENTION-1 already enforces current + delta-base (≤2); churn reads git directly (§3.3-C) — "git owns history" holds |

**Consequence for the "consolidated" meaning:** mixed handlers do not
disappear — the Cat-1 composers (D-EC-3: callers/callees/imports/cycles/
orient/explain/trust/stats) legitimately compose fact classes with
different owner stores under the RequestEpoch, while check stays a
single-store multi-class reader (Cat-4 — its classes share one owner).
Consolidation abolishes *within-class* double-reads, not *cross-class*
composition. §5 makes that checkable.

### 4.3 §2b evaluated seriously (the primary candidate)

**What the evidence supports as-is** [basis: §3.3/§3.4]:

- *SQLite keeps the STRUCTURE skeleton* — confirmed by consumer weight:
  all 43 B/C/D-group read arms consume skeleton/structure/metadata-shaped
  classes (FC1/FC2b/FC3-attribution/FC4–FC8), none FC2a adjacency;
  the skeleton is small, slow-moving, and is the FK anchor the permanent
  floor references. LiveGraph cannot replace it for 31 of 33 tables
  (no model), and should not (persistence-shaped, multi-language today).
- *LiveGraph owns function INTERNALS* — confirmed for FC2a: only the
  mixed ten consume symbol-level CALLS adjacency; callers/callees are
  already cert-gated LG-first; the warm cache already persists exactly
  this layer (PartitionIr bincode + value-facts sidecar, disposable,
  `.rgr/warm-cache/`) [OBSERVED: `repo-graph-warm-cache/src/lib.rs` DTOs
  — no `metadata_json`/`signature`/`doc_comment` blobs].
- *Snapshot degrades to a provenance stamp* — SNAPSHOT-RETENTION-1
  shipped the retention half (steady state ≤2 snapshots; "blob-narrowing
  is ENGINE-CONSOLIDATION-1 territory" [OBSERVED: its §3]); the stamp is
  the identity half (toolchain provenance, comparability, epoch binding)
  and is what the W-B RequestEpoch already pins.

**Where §2b must be refined, with evidence** — three collisions:

1. **The RED-floor collision (named in §2b; resolved here, not
   skirted).** Unresolved-reference disposition is a body-level fact
   class that CANNOT move (ratified floor) and — §4.5 — is not small: on
   unresolved-heavy repos it is among the LARGEST residual graph
   families (kernel: 2.78M unresolved vs 2.05M resolved edges
   [OBSERVED: `adr-extraction-substrate-scip-first.md:95`]; the
   4.8M-row `extraction_edges` resolution-input family — FC0 since
   revision 3, §3.2 note — also floor-serving — is comparable-to-LARGER
   in bytes: ≈1.4–3.4 vs the floor's ≈1.3–2.4 GB/copy, overlapping
   INFERRED ranges — §4.5 revision-4 recalculation). "Compact SQLite
   exception" is honest only per-repo: compact in CHURN and schema,
   not universally in bytes. Resolution → D-EC-2 matrix (recommended:
   keep the floor exactly as ratified — per-site rows + diagnostics in
   SQLite, labelled; the write path already exists; deps/attribution
   keeps its store). Re-homing disposition rows into the warm cache is
   listed there ONLY as a matrix cell with its blast radius (it would
   AMEND ratified Clause 3's table-level retention, put a served fact
   class in a deletable best-effort cache — "files-are-system-of-record"
   applies to the warm cache too [§2b], and strand deps/map's SQL joins)
   — NOT recommended, NOT a floor retirement proposal.
2. **The FC2b collision (§2b's "edge lists" is one word for two
   classes).** File-level IMPORTS + resource READS/WRITES rows are
   structure-shaped facts consumed by ten SQLite-only structure arms
   (§3.4-2). Moving them with FC2a would convert gate/violations/
   modules/resources/map into LiveGraph consumers — MORE double
   integration, the opposite of this slice's goal — or require derived
   SQLite projections that duplicate what `edges` rows already are.
   Recommended: FC2b stays SQLite (owner), LiveGraph keeps its existing
   import-observation serving cache for imports/cycles GREEN paths.
   D-EC-5 carries the alternative.
3. **The language ceiling (gate 2).** For non-SCIP-covered languages
   SQLite `edges` remains the ONLY FC2a store — on the deployment
   target (160k-file polyglot monorepo) and on kernel-scale C, §2b's
   internals move is INOPERATIVE until per-language ingest lands.
   The end-state is therefore **per-language**: FC2a ownership flips
   language-by-language as SCIP ingest + residency mature (TS first —
   already true on green), never globally. A global "LiveGraph owns
   internals" claim today would be a false trust claim; §5's definition
   is written per-language to avoid minting it.
4. **The body-granularity duplication the headline hides (revision 3;
   review-2 item 1).** Post-M-6-for-L, body-level call-site facts for L
   exist in TWO representations, permanently: FC0 raw rows in SQLite —
   the pipeline input the floor's classification and delta
   re-resolution require (≈1.4–3.4 GB/copy at kernel scale — §4.5
   revision-4 recalculation) —
   and FC2a resolved adjacency in LiveGraph + warm cache. §2b's
   "LiveGraph owns function internals" is therefore honest ONLY as
   "owns the SERVED, RESOLVED representation": the raw body-level
   stream never leaves SQLite. Priced as permanent floor rent; the
   alternatives are each foreclosed by a ratified contract (D-EC-2-C:
   the warm cache may not hold a system-of-record family; D-EC-2-D:
   dropping FC0 CALLS rows per covered language breaks delta
   re-resolution and the floor's language-completeness). Not a
   decision — a priced consequence; the ownership cell itself is
   ratified via D-EC-1/D-EC-2.

**Write-path consequence (named, since §2b implies it):** FC2a-derived
AGGREGATES — all three granularities (g1 snapshot count, five composers
§3.4-8; g2 per-symbol degree/liveness; g3 file-pair dep rows, §3.4-10) —
stay in SQLite while their SOURCE (FC2a content) moves engine — so they
must be computed at INDEX/refresh time from the resolution stream (FC0
input, all languages) and written as FC1/FC4 rows, rather than derived
at read time from SQLite `edges`. **(Revision-3 correction of the
sentence that misled M-3a, review-2 item 3: revisions 1–2 said "stats'
fallback computes degree from `edges` at read time" in this FC2a
paragraph — VERIFIED WRONG as an FC2a example: stats' read-time degree
is MODULE-granularity, from MODULE→MODULE IMPORTS + OWNS rows
[OBSERVED: queries.rs:1382-1471 module-rows fallback;
trust_impl.rs:523-587 trust core] — FC2b/FC1-derived owner-reads that
STAY under D-EC-5-A. stats has NO read-time FC2a-derived aggregate of
its own; its FC2a-agg intake is g1 via the trust core. The real
read-time FC2a-derived mechanisms are the §3.4-10 census: g1 COUNT
[service.rs:875] → M-3b; g2 dead-liveness in modules_list/show → M-3a;
g3 map's sketch → M-3a.)** This is D-EC-7 (a write-path data-shape
change — surfaced, not decided here) executed by M-3a/M-3b; it is a
HARD prerequisite of M-6 (after a per-language row drop, the read-time
COUNT silently undercounts, the liveness membership silently flips
false-dead, and the sketch silently thins — §3.2 note, §3.4-10).
FC2b-derived module stats/cycles need NO such move: their source rows
stay owner-resident under D-EC-5-A.

### 4.4 What "both" costs where it must remain (the permanent tax, priced)

Wherever one request composes SQLite-owned classes with LG-served leaves
(orient, explain, trust, stats, callers/callees/imports/cycles on their
eager+leaf shapes, the audit):

- **RequestEpoch** capture once per request + fingerprint validation per
  LG leaf + per-leaf fail-soft to the pinned snapshot (EV-A) [OBSERVED:
  W-B §3, §6]. This stays FOREVER on every mixed handler — it is the
  price of honesty under concurrency, ratified.
- **Per-surface no-loss certs** (import, cycles, stats, callgraph,
  focus-resolution, bounded-orient) — each a GREEN witness that the LG
  answer is byte-loss-free vs SQLite. Certs remain wherever a serving
  cache fronts a SQLite-owned class (FC2b/FC1 leaves) — but certs whose
  surface's owner flips to LiveGraph (FC2a on covered languages) become
  MIGRATION scaffolding: after the per-language edges retirement (§5
  M-6) there is no SQLite FC2a row set to compare against, and the cert
  is retired with the compare harness (D-EC-6 covers path's).
- **The audit comparator** (`AuditEpoch` identity witness) exists only
  while two stores answer the same question — per-surface, it retires
  with the migration it audits.
- **Measured tax today** [§3.4-7]: ~15.7k LOC adapter/witness layer +
  4.7k of it `livegraph_feed.rs` alone + 28 sibling-crate deps. The
  end-state does not zero this; it caps it at: ONE epoch mechanism +
  one cert per remaining serving-cache surface + zero per-surface
  compare/readiness harnesses.

### 4.5 Sizing — DB footprint & per-refresh write volume, kernel-scale vs today

**Measured anchors** [OBSERVED, cited]:

| Anchor | Value | Source |
|---|---|---|
| repo-graph self-index DB | 755 MB (ADR) / 1.4 GB (perf-baselines) | `adr-extraction-substrate-scip-first.md:97`; `docs/perf-baselines/README.md` |
| hadoop monorepo DB | 9.5 GB | perf-baselines; `cli-out-2a/hadoop.md` |
| 160k-file monorepo, ONE snapshot | 4 GB | `daemon-visibility-1.md:60-114` |
| 87k-file index | peak 5.8 GB; 3 orphaned snapshots = 11 GB | `daemon-crash-recovery-1.md:25-28` |
| Linux kernel edges | **2.05M resolved + 2.78M unresolved**; ~77 min/index | ADR `:95`; `daemon-concurrency-1.md:60` |
| Java monorepo nodes | 257K nodes / 13,962 files (~18/file) | `large-repo-validation.md:1086` |
| SCIP producer cost | 1.9–3.0 s/TS partition; 29–32 s/Rust crate; xref ~21 ms | `dataflow-hotpath-map.md` §5 |

**Row-shape basis** [OBSERVED: `001-initial.sql:76-127`, `migration_007`]:
every graph row is snapshot-scoped (snapshot_uid TEXT per row) and
TEXT-uid-keyed. Deletion lifecycle is NOT uniform: `nodes`/`edges` cascade
on snapshot delete, but `unresolved_edges` — the ratified permanent RED-floor
table — has NO `ON DELETE CASCADE` (`migration_007.rs:14-36`) and requires
manual deletion (`retention/prune.rs:184-211` lists it explicitly); any
milestone touching snapshot lifecycle must carry that manual step.
`edges` carries FIVE composite TEXT indexes,
`nodes`/`unresolved_edges` four each — index bytes plausibly ≥ data bytes;
`nodes` additionally carries `signature`/`doc_comment`/`metadata_json`
blobs (which §2b KEEPS — signatures are skeleton).

**Kernel-scale estimate** [INFERRED — assumptions stated; no kernel DB was
measured in-tree, and PartitionIr byte size is UNMEASURED (dataflow map
§10, NOT RUN); ranges deliberately wide]:

- Assume 26–64 B/TEXT uid, 200–350 B/edge row payload, +5 indexes ≈
  ×1.8–2.5 all-in → `edges` ≈ 2.05M rows ≈ **0.9–1.8 GB**/snapshot copy.
- `unresolved_edges` ≈ 2.78M rows × similar shape + classification
  columns + 4 indexes ≈ **1.3–2.4 GB**/copy — **the floor plus its FC0
  input family (next block) together dominate the §2b residual on
  unresolved-heavy repos** (kernel C; consistent with the observed
  4 GB@160k-file single snapshot; revision 4 — which of the two is
  individually larger is UNMEASURED, their INFERRED ranges overlap:
  "the floor dominates" alone is retracted).
- Skeleton (`nodes` ~18/file → kernel ~1.2–1.5M rows incl. signatures)
  ≈ **0.5–0.9 GB**; measurements/modules/families sub-GB.

**Family OMITTED by iteration-1's per-copy estimate (review-1 correction;
named FC0 in revision 3 — §3.2 note; bytes RECALCULATED in revision 4 per
review-3 item 1):**
`extraction_edges` — the persistent, snapshot-scoped resolution-INPUT
family (CASCADE on snapshot delete; THREE named indexes — one
single-column + two 2-column, TEXT-keyed — plus the TEXT-PK autoindex;
in the schema since 2026-04-11, so the DB-file anchors above already
include it) [OBSERVED: migration_012.rs:37-55, indexes :53-55; EXECUTED:
`git log --follow` on the migration]. Its row count ≈ resolved +
unresolved by construction — the resolution loop lands every extraction
edge in exactly one output family [OBSERVED: orchestrator.rs:876-967] —
so kernel ≈ 4.8M rows. Bytes, arithmetic explicit (revision 4: the
revision-3 figure "≈1.0–2.2 GB/copy" was the payload-only product
carrying an including-indexes label — RETRACTED): payload 4.8M ×
200–350 B ≈ **0.96–1.68 GB** BEFORE indexes; per-index-B-tree share
derived from the `edges` anchor above (×1.8–2.5 all-in over 6 B-trees —
5 named composites [OBSERVED: 001-initial.sql:120-124] + TEXT-PK
autoindex — ⇒ ≈0.13–0.25 per B-tree); `extraction_edges` materializes
4 B-trees (3 named + autoindex) ⇒ ≈ **×1.5–2.0 all-in** → ≈
**1.4–3.4 GB/copy** (1.47–3.36 rounded OUTWARD) [INFERRED, UNMEASURED —
and the payload assumption leans LOW here: `target_key` holds raw
pre-resolution EXPRESSIONS, multi-line snippets observed in the
self-index corpus, so the true upper end can exceed this]. M-6 does
NOT retire it: it feeds the floor (unresolved-call classification requires
resolution attempted over every call site), so CALLS *extraction* rows
keep being written and copied forward even for covered languages.

**What a delta refresh WRITES today** [OBSERVED first-hand — the code
basis review-1 required under any write-volume claim]:

- **Copy-forward for UNCHANGED files** into the new `snapshot_uid`:
  `nodes` (every column, incl. the `signature`/`doc_comment`/
  `metadata_json` blobs), null-file resource nodes, `extraction_edges`,
  `file_signals`, `file_versions` [OBSERVED: indexer_impl.rs:654-1018
  (DeltaCopyPort steps 1–4); invoked at orchestrator.rs:1541-1548].
- **Full-snapshot re-resolution**: the resolver batches over ALL
  `extraction_edges` of the NEW snapshot — copied-forward AND fresh —
  and re-writes the complete `edges` (resolved) and `unresolved_edges`
  (classified) families under the new uid; module edges are re-created
  on top [OBSERVED: orchestrator.rs:804-972 —
  `query_extraction_edges_batch(snap_uid)` →
  `insert_resolved_edges` / `insert_unresolved_edges`; Phase 4].
- **Consequence:** per-refresh SQLite write volume is ≈ **one full
  snapshot's graph-family bytes regardless of delta size** — GBs of
  row+index writes per refresh at kernel scale under today's
  representation. Iteration-1's "KB–MB/refresh under §2b" is
  **RETRACTED**: M-6 removes only the resolved-CALLS share of `edges`
  from that stream; the `nodes`/`extraction_edges`/`file_signals`/
  `file_versions` copy-forward and the `unresolved_edges` + FC2b
  re-writes all remain. The KB–MB regime is reachable ONLY under a
  changed-row-only / metadata-only-delta-base storage representation —
  a NEW cross-boundary data shape, now an explicit D-EC-8 axis (REP-2),
  never an implicit assumption.

**Copy-count & write-volume model** (worst cases, per review-1; "family
bytes" = one full copy of all snapshot-scoped graph families at the
given scale):

| Measure | Today (REP-1) | After M-6 for covered languages (REP-1) | REP-2 (ONLY if separately ratified — D-EC-8) |
|---|---|---|---|
| Steady-state retained copies | **1** after a full index (no parent role); **2** in delta steady state — current + the delta-base parent, the ratified keep-set — **plus 1 full copy per `baseline_user` mark** (today's row-retaining behavior; opt-in under D-EC-8-D) [OBSERVED: retention/classify.rs:51-77; snapshot-retention-1.md §2.2 + its §0 1-snapshot transcript] | same copy COUNT; each copy smaller by the resolved-CALLS share of `edges` (+ its index share) | ~1 full family + delta overlays + stamps |
| Refresh peak (transient) — TWO REGIMES (revision-3 restatement per review-2 item 2; supersedes revision 2's flat "up to THREE" worst case, which itself superseded iteration-1's "≈2×") | **Nominal single-refresh peak = THREE full families** — N−1 (retained parent) + N (current, the copy-forward source) + N+1 (being written) — plus baselines, until the pass prunes and pinned readers drain (prune is an exclusive writer) [OBSERVED: daemon-w-b-epoch-1.md §2c :229-238]. **Named precondition (operational, NOT schema-enforced): the prunable backlog was EMPTY when the refresh began** — i.e. the previous refresh's pass ran to completion. **Backlog regime = 3 + K, schema-UNBOUNDED:** the pass is queued background and yields to ANY writer — bounded requeue ≤60 × 1 s, then it defers entirely to the NEXT successful refresh's pass [OBSERVED: retention_pass.rs — `REQUEUE_MAX_ATTEMPTS`/`REQUEUE_BACKOFF`, `try_retention_attempt` two gates, `run_auto_retention` defer path]. NOTHING gates refresh ADMISSION on a drained backlog, so back-to-back refreshes that keep winning the write lock stack K unpruned (prunable-classified) prior generations. Drain guarantee: the FIRST pass that finds the repo idle removes the WHOLE backlog to the keep-set in one pass [OBSERVED: `run_retention_pass` — classify → `prune_prunable_snapshots` (all prunable) → orphan reclaim; retention/classify.rs:51-77] | same two regimes, smaller copies | base + overlays + the in-flight delta |
| Per-refresh SQLite writes | ≈ full family bytes (copy-forward + full re-resolution, above) | ≈ full family bytes **minus the resolved-CALLS share** for covered languages; per-changed-partition warm-cache blob rewrite added (outside SQLite) | O(changed files) — the only regime where "KB–MB/refresh" is honest |

**What §2b buys, honestly stated** (M-6 under TODAY's representation —
the only levers this consolidation itself pulls; REP-2 is deliberately
NOT counted):

| Lever | Effect | Condition |
|---|---|---|
| FC2a resolved rows leave SQLite (per language) | − the CALLS share of `edges` rows + its 5-index share, per retained copy AND per refresh write. UPPER BOUND: whole-`edges` ≈ 0.9–1.8 GB/copy at kernel scale; the CALLS share within it is UNMEASURED (the resolved family also carries IMPORTS/module/resource rows) [INFERRED]. Replaced by per-partition bincode (no TEXT-uid PKs, no per-row snapshot column, no SQL indexes, no `metadata_json`) — plausibly 3–6× smaller for the same facts [INFERRED from DTO shape; UNMEASURED]. FC0 CALLS rows STAY (floor input, above) | per-language SCIP coverage (gate 2); TS available NOW, C/C++/Rust after P2; M-3b ≺ M-6 AND M-3a ≺ M-6 (the §3.4-10 g2/g3 consumers re-homed or degrade-ratified first) |
| Snapshot identity → stamp | comparability/toolchain/epoch identity concentrates on the stamp the RequestEpoch pins; baseline marks stop retaining full graph rows BY DEFAULT (D-EC-8 A/D; B/D's per-mark opt-in priced at mark time). The retained-copy COUNT does not change — it is owned by the ratified retention keep-set (≤2 + baselines), not by this lever | D-EC-8 baseline axis ratified |
| Per-refresh write volume | under REP-1 (recommended now): stays ≈ full family bytes minus the CALLS share — only FC2a churn moves to partition-scoped warm-cache blobs; under REP-2: O(changed files) — requires the D-EC-8 REP-2 ratification (delta-indexing-track territory), not this consolidation | REFRESH-PROBE-1's two-speed model unchanged; producer time still dominates (1.9–3.0 s/TS partition) |
| What §2b does NOT shrink | the FC3 floor (1.3–2.4 GB/copy at kernel scale, permanent), FC0 `extraction_edges` (1.4–3.4 GB/copy — revision-4 recalculation, floor input — the §4.3-4 duplication; the floor-bound pair together ≈2.7–5.8 GB/copy), FC1 skeleton (0.5–0.9 GB), FC4–FC6 families — and no copy-COUNT mechanics | — |

FC2a-agg persistence (M-3a/M-3b/D-EC-7) adds: a snapshot-level scalar
family (g1 — trivial); a per-symbol degree family bounded by
function-node count (g2 — as measurement-shaped rows ~0.1–0.2 GB/copy at
kernel scale (~1.2–1.5M rows); as a packed column on `nodes`, far less);
and, under D-EC-7-A-i only, a per-file-pair resolved-dependency family
(g3) bounded by DISTINCT file pairs — dedup collapses call multiplicity,
so well below the per-edge row count [INFERRED; unmeasured]. An order
below every family above either way; exact shapes are M-3a/M-3b
implementation details [INFERRED from row counts; iteration-1's "KB–MB"
understated the kernel case and is corrected].

**Net [INFERRED, against the model above]:** under REP-1, §2b's M-6
shrinks each retained copy and each refresh write by the resolved-CALLS
share (upper-bounded by whole-`edges` ≈ 0.9–1.8 GB/copy at kernel scale)
on covered languages, and moves FC2a churn to partition-scoped
warm-cache blobs. It does NOT change copy counts, does NOT touch the
floor or `extraction_edges`, and does NOT make refresh writes KB-scale.
On non-covered languages nothing changes until P2 — and on kernel-scale
C (non-covered today) the M-6 saving is ZERO until C coverage lands,
with the dominant residuals being the ratified floor + the
resolution-input family. The honest headline stays: "SQLite sheds the
fast-churn resolved-CALLS family where SCIP covers it" — the DB does
not get small, and refresh writes get proportionally lighter, not
cheap.

## 5. End-state definition + milestones

### 5.1 "Consolidated" — the checkable definition

The migration is DONE when all of C-1…C-8 hold. Each is a predicate an
audit (or CI witness, C-6) can evaluate against the tree — no vibes.

- **C-1 Ownership is ratified and recorded.** The §4.2 table (as amended
  by ratification) is recorded in `docs/architecture/` and mirrored into
  the artifact-contract registry docs (`artifact-contract-model.md`
  Canonical Family Inventory gains an "owner engine" column). One place
  answers "who owns fact class X".
- **C-2 No within-class double-read.** No handler reads the SAME fact
  class from both stores in one request EXCEPT (i) the labelled,
  epoch-pinned fail-soft fallback (RED cert / non-resident / non-covered
  language / no snapshot), and (ii) explicitly-ratified comparators
  (`cycle_completeness_audit`, `--engine compare` where a migration is
  still live). Cross-CLASS composition (orient reading FC1+FC3 from
  SQLite beside FC2a from LG) is sanctioned, epoch-pinned, and labelled
  per leaf — that is the terminal shape, not a defect (§4.2).
- **C-3 FC2a default-serves from its owner.** For every SCIP-covered
  language, callers/callees (and path iff D-EC-6 lands an LG-serve
  variant [A or D]) default-serve FC2a from the LiveGraph on GREEN with
  **zero per-call SQLite FC2a reads** — no `edges` CALLS-row content is
  read on the GREEN serve path; SQLite FC2a reads happen only per the
  §4.2 regime the language is in (R1 labelled fallback; R3 owner-read).
  The eager FC1 `nodes` symbol-resolution read (`resolve_symbol`
  [OBSERVED: dispatch.rs:1187 handle_callers / :1293 handle_callees])
  is an OWNER-read of a SQLite-owned class and REMAINS — C-3 does not
  demand `nodes`-freeness. (Review-1 correction: iteration-1's "zero
  per-call `nodes`/`edges` content reads" contradicted §3.3-A and
  D-EC-3 Cat-1, which classify callers/callees as permanent FC1+FC2a
  cross-store composers; that phrasing is retracted.)
- **C-4 Eager SQLite reads are owner-reads only.** orient/explain/trust/
  stats/check's unconditional reads touch only SQLite-OWNED classes
  (FC1, FC2a-agg once M-3a/M-3b persist it, FC2b, FC3, FC4, FC5, FC8).
  On GREEN, no handler eagerly reads FC2a CONTENT — today's violators,
  all FOUR named (revision 3 completes the census, §3.4-10; revision 2's
  "both named" was an undercount): (i) path's default BFS walk (resolved
  by D-EC-6/M-4); (ii) the trust core's COUNT scan over CALLS rows on
  every trust/check/orient/explain/stats call [service.rs:875; §3.4-8]
  (g1 → M-3b); (iii) the dead-liveness membership on every modules_list/
  modules_show call [dispatch.rs:7862/:7595; queries.rs:1031-1060]
  (g2 → M-3a); (iv) map's dep-sketch CALLS read [queries.rs:2615-2640]
  (g3 → M-3a, per the D-EC-7 sub-choice). In (ii)–(iv) the fact consumed
  is already owner-shaped (FC2a-agg g1/g2/g3) — the read-time MECHANISM
  over rows is what M-3a/M-3b replace with persisted aggregates.
  FC2b-derived read-time aggregation (module fan-in/out, module cycles
  [trust_impl.rs:458-587]) is an owner-read and stays.
- **C-5 Every fallback serve is labelled.** `provenance.source = sqlite`
  (or the surface's equivalent) on every fail-soft leaf — no silent
  engine switch (already the shipped contract; kept as a predicate so it
  cannot regress).
- **C-6 The reader-set witness is enforced.** The §7.3 method is a
  script: the set of modules reading the LiveGraph field equals the
  sanctioned list, and any new dispatch arm declares its fact classes
  (a one-line manifest the script checks). New features pay ONE
  integration by construction. (CLAUDE.md: enforce by script where
  possible.)
- **C-7 Warm cache stays non-authoritative.** Deleting `.rgr/warm-cache`
  never loses a fact class PERMANENTLY, in every §4.2 regime: R1 and R3,
  SQLite rows serve (labelled fallback / owner-read); R2 (post-M-6
  covered languages), the producer rebuilds FC2a deterministically from
  source — the source tree is the system of record — and AVAILABILITY is
  honestly degraded until the rebuild: labelled, with the concrete
  remediation, never silent-empty (§4.2 R2 ladder). The floor and the
  skeleton never depend on the cache. Consequence for M-6: a language
  may drop rows ONLY when its producer story satisfies deletion gates
  3/4 (migration + operator reset [readiness-1 §5]) — TS's producer is
  dev-only pinned today [livegraph-integration-1c], a named blocker.
- **C-8 Retained copies = the ratified retention keep-set; identity
  lives on the stamp.** Steady state: graph-family rows exist for
  exactly the ratified keep-set — the CURRENT state, plus the
  delta-base parent while the last successful write was a delta refresh
  with a valid-epoch parent (≤2 full families — SNAPSHOT-RETENTION-1
  §2.2 [OBSERVED: retention/classify.rs:51-77]), plus explicitly
  user-marked baselines per the ratified D-EC-8 cell. During a refresh,
  the ratified W-B rule is PRESERVED, not amended: publishing N+1 never
  deletes N's rows; old rows stay readable by pinned uid; prune is an
  exclusive writer that never runs under a pinned reader [OBSERVED:
  daemon-w-b-epoch-1.md §2c :229-238]. Transient regimes (revision-3
  restatement per review-2 item 2 — supersedes revision 2's flat "up to
  THREE" transient worst case, which superseded "≈2×"): the NOMINAL
  single-refresh peak is THREE full families (N−1 retained parent + N
  copy-source + N+1 being written) + baseline marks, under the NAMED
  precondition that the prunable backlog was empty at refresh start;
  under back-to-back refreshes with a starved (always-yielding) pass
  the stack is 3+K — schema-UNBOUNDED — until the first idle-window
  pass drains the whole backlog to the keep-set and pinned readers
  drain (§4.5 two-regime model). **C-8's checkable predicate therefore
  binds the QUIESCENT state** (no refresh in flight, no pass queued):
  retained graph-row families = the ratified keep-set exactly. A
  pending backlog is a liveness obligation of the retention pass — NOT
  a C-8 violation while a pass is queued, and NOT presentable as a
  hard three-family bound. (If the operator wants three as a hard
  bound, that is a small daemon behavior change — refresh admission
  drains/awaits the backlog first — a code change deliberately NOT
  proposed by this spec; it would be its own decide-and-record inside
  M-7's build if wanted.) Comparability/toolchain/epoch identity live
  on the stamp the RequestEpoch pins; baseline marks default to
  stamp+measurement retention (D-EC-8 A/D). No retained snapshot
  TIMELINE in SQLite (git owns history). Per-refresh WRITE volume is
  deliberately NOT a C-8 predicate — it is a representation property
  (D-EC-8 REP axis; §4.5).

**Explicitly NOT in the definition:** retiring SQLite (impossible —
floors FC3/FC5/FC6 + non-covered languages); a single write pipeline
(the homegrown extractor keeps writing FC1/FC2b/FC3/FC4/FC6 — SCIP
carries no disposition, no measurements, no boundaries); zeroing the
epoch/cert machinery (§4.4 — the cross-class tax is permanent and
priced).

### 5.2 Milestones — each independently shippable + smoke-gateable

Ordering constraints only where real; every milestone leaves the tree
releasable. Gates name existing harnesses (smoke scripts,
`dogfood-isolated.sh`, `--engine compare`, no-loss certs, workspace
tests).

| M | Content | Ships when / gate | Depends on |
|---|---|---|---|
| **M-0** | Ratify §6 decisions; record the ownership table (C-1); correct the stale ledgers this inventory exposed (CURRENT_SLICE "6/10" → 5/10 + path posture; the two-tens disambiguation); banner the superseded per-surface plans (D-EC-4) | docs-only merge; decision-review sign-off | operator ratification |
| **M-1** | The reader-set + fact-class witness script (C-6), wired into CI/smoke | script red/green on today's tree (must PASS with the sanctioned list = §3.3-A modules + the two LG writers) | M-0 (list is the ratified one) |
| **M-2** | Finish the (b)-leaves: orient/explain MODULE_SUMMARY + cycle VALUES LG-serve on GREEN (the deferred P1 remainder: DR-2/DR-E3 `module_stats` identity reconciliation; CYCLES-B) — Cat-2(ii) cache serves over SQLite-owned classes, cert-witnessed. Explicitly NOT here: a `resolved_calls` LG-serve — that leaf's terminal source is the M-3b persisted aggregate (an LG count is residency-bounded, §4.2 FC2a-agg row); the parked `*-SQLITE-FREE-1` aspiration to LG-serve it is superseded | per-leaf no-loss certs GREEN + byte-compare on the smoke fixtures + `dogfood-isolated.sh` | M-0; supersedes the parked `*-SQLITE-FREE-1` spec-first plans (D-EC-4) |
| **M-3a** | **Sub-snapshot FC2a-agg re-home (re-scoped in revision 3 per review-2 item 3; the prior "stats fact-class split … FC2a-agg/FC2b derivations replaced" wording contradicted §3.2/§4.3/D-EC-7's FC2b-stays-read-time rule and named no exact aggregate — RETRACTED).** Exactly the two §3.4-10 consumers migrate, granularity named: **(g2, per-symbol)** modules_list/modules_show dead rollups swap `find_dead_nodes`' CALLS-membership input [queries.rs:1031-1060; dispatch.rs:7862/:7595] for the D-EC-7 persisted per-symbol incoming-CALLS degree (liveness = persisted CALLS-degree > 0 OR a row of the 7 retained FC2b relation types — that 7-type NOT-IN remains an owner-read); **(g3, per-file-pair)** map's dep sketch: under D-EC-7-A-i, `map_resolved_dep_edges_in_path`'s CALLS share [queries.rs:2615-2640] swaps to the persisted file-pair family; under A-ii, the labelled IMPORTS-only degrade is ratified instead. The same D-EC-7 producer writes §2b's per-function fan-in/fan-out skeleton columns (g2 family — no additional consumer required). **stats migrates NOTHING here** [VERIFIED, review-2 item 3: its read-time degree is MODULE-granularity from MODULE→MODULE IMPORTS + OWNS rows, queries.rs:1382-1471 — FC2b/FC1 owner-reads that stay (D-EC-5-A); its FC2a-agg intake is g1 via the trust core → M-3b]; the module-rows LG leaf unchanged | parity window while CALLS rows still exist: persisted g2 degree / g3 pairs MUST equal the live row-derived values (self-validating, same pattern as M-3b); byte-compare modules_list/modules_show/map on fixtures; fresh index AND refresh both validated (Persistence Completeness) | D-EC-7 ratified (incl. the A-i/A-ii g3 sub-choice); M-0. **HARD ordering: M-3a ≺ M-6's first drop** — after a per-language row drop, liveness silently flips false-dead and the sketch silently thins (§3.4-10) |
| **M-3b** | **FC2a-agg re-home — the trust-core migration** (C-4's last eager FC2a scan; review-0 item 2): **(i) WRITE:** index + refresh persist the snapshot-level resolved-call count (+ the per-function degree family, per D-EC-7) computed from the FULL extraction stream, including delta-refresh copy-forward — the full Persistence Completeness checklist (write path / read path / refresh behavior / trust impact / CLI visibility / validation); **(ii) READ:** the ONE shared trust core swaps `count_edges_by_type(CALLS)` [service.rs:875] for the persisted aggregate — trust, check, orient, explain, stats all inherit through `assemble_trust_report`/`get_trust_summary` [agent_impl.rs:326-344], zero per-surface work (§3.4-8); the FC2b-derived module stats/cycles stay read-time OWNER reads (D-EC-5-A — no migration); **(iii) trust posture unchanged:** the two-source hybrid (Half-A/Half-B) is untouched — only the v1 report's `resolved_calls` INPUT changes source | parity window: while CALLS rows still exist, the persisted count MUST equal the live COUNT (self-validating); byte-compare trust/check/orient/explain/stats on fixtures; **fresh index AND refresh both validated** | D-EC-7 ratified; M-0. **HARD ordering: M-3b ≺ M-6's first drop** — after a per-language row drop, the read-time COUNT silently undercounts; the persisted full-stream aggregate is the only honest source |
| **M-4** | Execute the ratified D-EC-6 posture for path: under C (recommended), land the LG serve — A's cert-flip or D's degrade contract, the pick recorded when built — BEFORE M-6's first language drop; under B, record the permanence and STRIKE M-6 (gate 1 forecloses, see D-EC-6-B). The §3.4-5 comment defect is fixed in every variant | A: compare harness on fixtures + smoke; D: degrade-path tests + smoke; B: docs-only | D-EC-6 |
| **M-5** | PREREQ-2 for the covered subset: cert-BUILD sources become structural proofs (not SQLite compares); fallback paths audited so a per-language FC2a row drop cannot strand non-resident/stale/RED serving | cert tests + fallback-path tests + smoke | M-2, M-3a, M-3b |
| **M-6** | **Per-language FC2a retirement, TS first** (repeatable template): stop writing symbol-level CALLS rows into `edges` for language L; retain FC2b rows; aggregates already owner-written (M-3a/M-3b). Rows for L drop only when the 5 deletion gates hold for L (readiness-1 §5, per-language reading) — incl. gates 3/4's producer story: the producer must be distributable/provisionable for L, not dev-only (C-7; TS blocker named there) | the five gates evaluated per-language **PLUS the §3.4-10 census re-verified EMPTY for L: no read-time CALLS-row consumer left un-re-homed (M-3a/M-3b landed) or un-degrade-ratified — the dead-liveness false-positive and sketch-thinning failure modes are the named regression risks**; `--engine compare` retired for L's surfaces after the drop; dogfood + smoke on an L fixture | M-4 (per D-EC-6-C ordering), M-5 + gate-2-for-L (TS: now; others: P2 program); M-3a/M-3b bind transitively via M-5 and are ALSO named directly (hard orderings in their rows) |
| **M-7** | Snapshot → provenance stamp (D-EC-8, baseline axis, under REP-1): blob-narrowing + baseline-stamp semantics (the part SNAPSHOT-RETENTION-1 §3 explicitly deferred here); `classify_retention`/baseline handlers keep working against the stamp. The W-B transient window + exclusive reader-drain prune AND the ratified keep-set copy-COUNT are PRESERVED unchanged (C-8) — M-7 changes what a baseline MARK retains + blob width, never the keep-set count or the refresh-window invariant. **REP-2 (changed-row-only / metadata-only delta base) is explicitly OUT** — it is the D-EC-8 REP axis, delta-indexing-track territory (ROADMAP queue 4), its own future ratification | retention tests + refresh smoke (fresh index AND refresh — Persistence Completeness) | D-EC-8; do not fold the queue-4 delta-indexing track in here |

Milestones deliberately NOT claimed: non-TS SCIP ingest (that is the P2
program — ROADMAP "Parallel strategic bet", months-scale, its own track;
M-6 consumes its output per language); DAEMON-CONCURRENCY-1 (queue 3 —
shares the coordinator seam, no ordering conflict: the epoch invariant is
already flip-safe [W-B §7.3 verdict]).

### 5.3 Fate of the `livegraph_feed.rs`-style glue (the §2 ask)

Today: 4,726-line multi-surface adapter + ~11k more of certs/coherence/
serve/tests [EXECUTED counts, §3.4-7]. End-state, per family:

- **RequestEpoch + coordinator seam — PERMANENT** (the cross-class tax,
  §4.4). Stays small and single-mechanism; never per-surface.
- **Per-surface `*_engine_response` fallback arms — PERMANENT but
  SHRINKING**: one labelled fallback arm per FC2a surface, live in §4.2
  regimes R1 (labelled fallback) and R3 (owner-read for non-covered
  languages). Each M-6 language retirement moves L to R2: the SQLite
  arm has no row set left for L and is replaced by the R2
  producer-rebuild/degrade ladder — the arm's code is deleted per
  surface when its last R1/R3 language leaves. The `Engine::parse` dev
  escape hatch (`--engine sqlite/livegraph`) stays as a diagnostic
  surface.
- **`*_compare_response` + `*_readiness_response` harnesses —
  MIGRATION SCAFFOLDING**: retired per surface at its M-6 drop (a
  comparator with one store left is dead code); the audit handler
  retires with the last compared surface (D-EC-3 records it).
- **No-loss certs**: retired per-surface where the owner flips
  (post-M-6 there is nothing to be no-loss AGAINST); retained where
  LiveGraph remains a serving CACHE over a SQLite-owned class
  (FC2b imports/cycles GREEN paths, orient bounded leaves).
- **Structural split (mechanical, no behavior change):** the file
  violates the 500-line guardrail by 9×; decompose along the lines
  above (epoch/engine core; per-surface serve+fallback; harnesses) —
  scheduled WITH M-2/M-3a/M-3b edits (refactor-before-expand rule), not as
  its own slice. The `repo-graph-livegraph-feed` CRATE (92 LOC, the
  ingest→LG adapter) is a different artifact than this daemon FILE
  despite the shared name — rename consideration recorded in D-EC-4's
  paper pass (name-vs-semantics).

## 6. Decisions for ratification (DECISION_REQUIRED)

Convention per CLAUDE.md: exhaustive matrices, every cell filled;
trade-offs stated against the VISION's three commitments (deterministic
extraction / honesty about certainty / current-state-in-milliseconds) and
the change-cost doctrine (cost = load-bearing assumptions disturbed).

DECISION_REQUIRED:
- ID: D-EC-1
  QUESTION: Ratify the §4.2 fact-class ownership table (with FC2 split
    into FC2a content / FC2a-agg / FC2b) as the named end-state?
  OPTIONS:
  - A (RECOMMENDED) — §4.2 as written: SQLite owns FC0 (pipeline
    input, never served — revision 3)/FC1/FC2a-agg (as persisted
    owner-written aggregates, all three granularities, D-EC-7)/FC2b/
    FC3/FC4/FC5/FC6/FC8-stamp; LiveGraph owns FC2a content per covered
    language (§4.2 three-regime statement); FC7 filesystem-primary.
    Consequence:
    every §5 milestone becomes executable; the mixed handlers become
    sanctioned cross-class composers (D-EC-3 categories);
    determinism untouched (owners are stores, extraction unchanged);
    honesty strengthened (per-leaf source labels already shipped);
    milliseconds served by LG on the hot adjacency class. Cost: the
    FC2a/FC2a-agg/FC2b split is a NEW distinction to keep documented —
    earned by ten SQLite-only FC2b consumers (§3.4-2) and five
    FC2a-agg consumers through one shared core (§3.4-8).
  - B — §2b literal: ALL `edges` (CALLS+IMPORTS+READS/WRITES) to
    LiveGraph. Consequence: gate/violations/modules/resources/map/deps
    become LiveGraph consumers or need new derived SQLite projections —
    either MORE double-integration glue (contradicts this slice's
    purpose) or a projection layer duplicating today's rows; non-TS
    languages strand entirely on these STRUCTURE surfaces (a
    current-state discovery regression, worst on the polyglot
    deployment target). Honesty cost: structure answers become
    residency-dependent.
  - C — Status quo + ledger only (name today's state, change no
    ownership). Consequence: zero build cost; the double-integration tax
    (~15.7k LOC + every new feature ×2) persists unbounded; "no finish
    line" — the exact problem §1 names — remains.
  RECOMMENDED: A.
  BLOCKING_REASON: every §5 milestone and every other decision below
    executes against this table; it is THE architecture-boundary call of
    the slice (data-shape ownership across a store boundary).

- ID: D-EC-2
  QUESTION: How does the §2b split coexist with the ratified RED floor
    (unresolved-reference disposition, SQLite-only forever) — AND with
    the floor's INPUT CHAIN, the FC0 extraction stream that keeps
    disposition classifiable under delta refresh (§3.2 FC0 note;
    widened in revision 3 per review-2 item 1 — FC0's terminal
    ownership is ratified HERE plus the D-EC-1 table row, without
    re-opening the floor)?
  OPTIONS:
  - A (RECOMMENDED) — Floor unchanged, named as the sanctioned body-level
    exception: per-site `unresolved_edges` rows + `extraction_
    diagnostics_json` stay SQLite exactly per Clause 3, serving trust
    (labelled) AND deps/attribution/map (§3.4-3). **FC0 explicitly
    included: `extraction_edges` retained for ALL languages — its CALLS
    rows keep being written and copied forward for covered languages
    even post-M-6 (pipeline input, never served FC2a; the §4.3-4
    body-granularity duplication is the priced consequence).** Cost
    honestly sized (§4.5, revision-4 recalculation): on unresolved-heavy
    repos the floor-bound PAIR is the largest residual — FC3 kernel ≈
    1.3–2.4 GB/copy, its FC0 input family ≈ 1.4–3.4 GB/copy, together ≈
    2.7–5.8 GB/copy (overlapping INFERRED ranges; which of the two is
    individually larger is UNMEASURED — FC0 has ~1.7× the rows but
    fewer index B-trees) — accepted as the price of the only honest
    disposition source. No contract disturbed.
  - B — Narrow the floor's SHAPE: keep aggregates + classifications +
    bounded samples in SQLite, move per-site rows elsewhere/drop them.
    Consequence: AMENDS ratified Clause 3 (table-level retention
    "FOREVER") — a ratified-contract change, operator-only; breaks
    deps_why/map per-site attribution (§3.3-B reads locations);
    shrinks the floor's footprint at the cost of per-site provenance —
    the honesty commitment bleeds (a disposition you cannot point at a
    site is a weaker Layer-1 fact). Listed because §2b demands the
    persistence story be stated, NOT recommended.
  - C — Re-home per-site rows into the warm cache. Consequence: puts a
    permanent, non-reproducible-from-SCIP fact class into a disposable,
    best-effort, validated-or-discarded cache [OBSERVED: warm-cache
    contract] — violates files-are-system-of-record (§2b's own caveat)
    and Clause 3 both; a deleted cache would LOSE dispositions until
    full re-index. Rejected on contract + honesty grounds; per the
    slice stop condition this is NOT proposed — the cell exists so the
    matrix is exhaustive.
  - D — Narrow FC0 per language: stop writing/copy-forwarding CALLS
    `extraction_edges` rows for covered languages post-M-6 (the "why
    keep raw call sites twice?" objection, answered). Consequence:
    delta refresh could no longer re-resolve unchanged covered-language
    files' call sites — resolution batches over the NEW snapshot's FC0
    rows, copied + fresh [OBSERVED: orchestrator.rs:804-972] — so the
    floor's disposition for covered languages goes stale/incomplete on
    every delta refresh (Clause 3 violated in EFFECT though not in
    letter: the table survives, its content rots) and every FC2a-agg
    granularity (M-3a/M-3b) loses its language-complete source; the
    only repair is a full re-index per refresh. REJECTED; the cell
    exists so FC0 retention is an explicit ratified choice, not an
    omission (review-2 item 1).
  RECOMMENDED: A.
  BLOCKING_REASON: §2b names this collision as the one the spec must
    resolve explicitly; M-6/M-7 scoping depends on which rows exist in
    SQLite at the end-state — and FC0's terminal ownership (this
    matrix + the D-EC-1 row) bounds the §4.3-4 duplication every
    covered language pays.

- ID: D-EC-3
  QUESTION: Which handlers are PERMANENTLY mixed, in which SHAPE, and is
    that classification ratified as terminal? (Rewritten for review-0
    item 3: iteration-0 called callers/callees/imports/cycles
    "single-class serves" — WRONG by §3.3-A: callers/callees compose
    SQLite FC1 resolution with LG-served FC2a; imports/cycles compose
    FC1 with FC2b. That wording is retracted; the four categories below
    replace it.)
  OPTIONS:
  - A (RECOMMENDED) — Ratify the four-category terminal classification,
    exhaustive over the 66-arm surface (§3.3 is the evidence):
    **Cat-1 — permanent cross-store, cross-class composers** (the
    RequestEpoch pin is their permanent price, §4.4): callers, callees
    (SQLite FC1 symbol resolution + FC2a serve); imports, cycles
    (SQLite FC1 + an FC2b answer whose LG leaf is a same-class CACHE);
    orient, explain (SQLite FC1/FC3/FC4/FC5(/FC8) + LG bounded leaves +
    FC2a-agg via the trust core); trust (the ratified two-source
    hybrid); stats (FC1/FC4 summary + LG module-rows leaf + trust
    overlay). Eight today; **path joins Cat-1 iff D-EC-6 lands an
    LG-serve variant (A or D); under D-EC-6-B it stays Cat-4.**
    **Cat-2 — same-class serve shapes** (leaf-level mechanics INSIDE
    Cat-1 handlers — a per-leaf property, deliberately NOT a second
    handler list): per leaf, either (i) LG-as-OWNER serve with the
    §4.2 regime ladder behind it (FC2a on covered languages: R1
    labelled SQLite fallback pre-M-6, R2 producer/degrade ladder
    after), or (ii) LG-as-CACHE serve over a SQLite-OWNED class with
    owner fallback (FC2b for imports/cycles; FC1 for focus-resolution /
    bounded-orient leaves) — permanent, cert-witnessed (§4.4).
    **Cat-3 — temporary comparators** (each retires with the migration
    it audits): `cycle_completeness_audit` (the dispatch arm retires
    with the last surface it compares); `--engine compare` +
    `*_compare_response`/`*_readiness_response` harnesses (retired per
    surface at its M-6 drop). The explicit `--engine sqlite|livegraph`
    escape stays as a diagnostic surface, not a comparator.
    **Cat-4 — single-store multi-class readers** (SQLite-only; NO epoch
    machinery — one snapshot pin covers all their classes): check is
    the named exemplar (FC1 + FC2a-agg + FC2b-derived + FC3 + FC5 +
    FC8, all through one store, §3.3-B); with it gate, map, violations,
    modules_show, deps_list/why/drift, the remaining B/C-group arms
    (§3.3-B/C), the D-group, and the writers. Multi-class within ONE
    store is unremarkable and terminal — consolidation never asks them
    to change STORES. (Revision-3 precision: M-3a does change the
    SQLite rows three of them read — modules_list/modules_show swap
    the CALLS-liveness input, map swaps or degrade-labels its sketch's
    CALLS share, §3.4-10 — but each stays a single-store Cat-4 reader:
    the persisted FC2a-agg families are SQLite rows.)
    RECONCILIATION (the review-0 ask): C-2 sanctions Cat-1/Cat-2
    (labelled fallback + certs are its named exceptions) and exempts
    Cat-3 while a migration is live; C-6's sanctioned reader list =
    Cat-1 handlers (+ Cat-3 while alive) + the two LG writers;
    RequestEpoch permanence applies to exactly Cat-1 — the W-B
    mixed-read ten minus the audit (Cat-3) minus path-until-D-EC-6;
    D-EC-6 outcomes map: A/D → path Cat-1; B → path Cat-4 with CALLS
    rows retained forever (M-6 foreclosed); C → path Cat-4 now, Cat-1
    at the M-6 gate.
  - B — Force single-store handlers everywhere. Consequence: requires
    synthesizing FC3 in LiveGraph (probe-refuted, VISION-forbidden
    overclaim) or amputating trust/disposition leaves from orient/
    explain/stats (a discovery + honesty regression), and inlining FC1
    resolution into the LiveGraph for callers/callees (a projection it
    holds only per covered language — non-TS callers would strand).
    Not viable without violating a ratified floor.
  - C — Leave "mixed" undefined. Consequence: C-2 uncheckable; the
    finish line dissolves again.
  RECOMMENDED: A.
  BLOCKING_REASON: C-2/C-6 (the checkable definition + the CI witness)
    need the ratified categories; W-B's epoch contract stays
    load-bearing for exactly Cat-1; M-6's fallback-arm deletions assume
    Cat-2's regime ladder.

- ID: D-EC-4
  QUESTION: What happens to the superseded per-surface `*-LIVEGRAPH-*` /
    `*-SQLITE-FREE-*` plans?
  OPTIONS:
  - A (RECOMMENDED) — A follow-up docs-only pass (M-0) banners the
    still-SPEC-FIRST plans (`orient-sqlite-free-1.md`,
    `explain-sqlite-free-1.md`, `trust-summary-livegraph-1.md`
    [OBSERVED: Status SPEC-FIRST, unimplemented remainder], the P1
    "marginal fastpath" framing of readiness-10, CYCLES-B) as SUPERSEDED
    → their live content becomes M-2/M-3b of THIS end-state (revision-3
    precision: M-3a's re-scoped content — dead-liveness + map's sketch,
    §3.4-10 — never appeared in those parked specs) (the
    `resolved_calls` leaf goes to the M-3b persisted aggregate, NOT an
    LG serve — see M-2); delivered
    records (cycles/imports/stats/coherence arcs) are left untouched
    (history, not plans). Also fixes the two name-vs-semantics items:
    the `dispatch.rs:2342` stale comment (with M-4) and the
    livegraph-feed file/crate name collision (rename PROPOSED there,
    boundary-touching, separately ratified). Consequence: one
    authoritative plan; no agent follows a dead spec.
  - B — Leave the old specs as-is. Consequence: two competing
    finish-line documents; the next builder must re-derive which is
    live — the documented failure mode this slice exists to end.
  RECOMMENDED: A.
  BLOCKING_REASON: those docs are out of THIS slice's file scope
    (spec-doc-only edit); the supersession needs an explicit operator
    go so the paper pass can touch them.

- ID: D-EC-5
  QUESTION: FC2b (file-level IMPORTS, resource READS/WRITES) — owner?
  OPTIONS:
  - A (RECOMMENDED) — SQLite owns FC2b; LiveGraph keeps its existing
    import-observation serving cache (imports/cycles GREEN fastpaths
    unchanged, cert-witnessed). Consequence: the ten SQLite-only
    structure consumers (§3.4-2) keep their single-store reads; zero
    new glue; non-TS parity on structure surfaces preserved.
  - B — LiveGraph owns FC2b too (per covered language), SQLite keeps a
    fallback + non-covered rows. Consequence: conceptually uniform
    ("all edges one owner") but adds LG plumbing to up to ten structure
    handlers or a projection layer; per-language conditionality bleeds
    into gate/violations/modules answers — governance surfaces (highest
    change-cost class per VISION) start depending on residency.
  RECOMMENDED: A.
  BLOCKING_REASON: decides whether M-6's per-language retirement drops
    CALLS rows only (A) or all edge rows (B) — different deletion-gate
    evaluations and different blast radii.

- ID: D-EC-6
  QUESTION: `path` terminal posture (today: ratified pinned-SQLite
    default, LG BFS only behind explicit `--engine` [§3.4-4])?
    (Rewritten for review-0 item 4: iteration-0's option B called
    pinned-SQLite "terminal" while admitting it stops working after
    M-6, and recommended a staged choice absent from the matrix. Below,
    A/B/D are genuine END STATES; C is the staged choice, present as
    its own option and labelled as such.)
  OPTIONS:
  - A — Build the CALLS∪IMPORTS parity cert NOW; flip path's Auto
    default to LG on GREEN for covered languages (labelled SQLite
    fallback while R1 rows exist; the R2 ladder after M-6). Terminal
    shape: path is a Cat-1 composer, included in C-3; compatible with
    M-6. Cost: the union cert W-B-EPOCH-IMPL-2A explicitly declined is
    built and maintained until M-5 converts cert-BUILDs to structural
    proofs; milliseconds improved on deep BFS. Determinism/honesty
    unchanged (cert-witnessed serve, labelled fallback).
  - B — Retain SQLite CALLS rows FOREVER — all languages, INCLUDING
    covered ones — so the pinned-SQLite default is truly terminal.
    Priced honestly: deletion gate 1 ("no shipped command depends on
    it by default" [readiness-1 §5]) permanently RE-FAILS for `edges`
    CALLS → **M-6 never runs for any language**; the §2b FC2a
    footprint win is forfeited system-wide (the CALLS share of `edges`,
    upper-bounded by whole-`edges` ≈ 0.9–1.8 GB/copy at kernel scale,
    §4.5) and the write path double-writes CALLS (rows
    + warm cache) forever — all to keep ONE default command's BFS on
    SQLite. Zero new machinery; path stays coherent-by-pin.
  - C (RECOMMENDED) — Staged, with the stage bound explicit: ratify
    pinned-SQLite as the CURRENT posture (re-affirming the W-B D-CC
    refinement), and make a landed LG serve for path — A's cert-flip
    or D's degrade contract, picked at M-4 build time — a NAMED GATE
    of M-6's first covered-language drop (M-4 ≺ M-6(L1), bound in the
    milestone table). Not itself a terminal state: it SCHEDULES the
    A-vs-D pick for the moment it becomes load-bearing. If no LG serve
    ever lands, M-6 never fires and B's cost is being paid — this
    option makes that visible at the M-6 gate instead of silently.
  - D — Contract change at the drop: post-M-6(L), path serves covered
    language L LG-only when resident + structurally proven (M-5); when
    not, it returns an honestly-degraded answer — labelled
    Partial/Unavailable with the concrete remediation, never
    silent-empty (VISION honesty rules; XPART answer-class precedent).
    No parity cert is ever built: pre-drop path stays pinned-SQLite;
    post-drop there is no SQLite row set to be parity-checked against.
    Cheapest machinery; cost: path's covered-language answers become
    residency-conditional — honesty machinery replaces the
    coherent-by-pin guarantee. Compatible with M-6.
  RECOMMENDED: C (the A-vs-D pick is recorded inside M-4 when built).
  BLOCKING_REASON: determines whether M-6's deletion gate 1 can EVER
    close (B forecloses it; A/D open it; C schedules the pick at the
    gate); path is a default command reading `edges`, so C-3 and the
    M-4 ≺ M-6 ordering bind to this decision.

- ID: D-EC-7
  QUESTION: FC2a-derived aggregates — THREE granularities (§3.2,
    revision 3): **g1** the snapshot-level resolved-call count all five
    composers consume (§3.4-8); **g2** per-symbol incoming-reference
    degree (§2b's per-function fan-in/fan-out; TODAY consumed as the
    CALLS share of dead-liveness by modules_list/modules_show,
    §3.4-10); **g3** per-file-pair resolved dependency rows (map's dep
    sketch, §3.4-10) — written at index/refresh time as owner facts?
    (Widened from two granularities in revision 3; the prior option-A
    consumer claim "stats/hotspots/orient degree reads (M-3a)" was
    UNVERIFIED and is RETRACTED — none of the three derives per-function
    degree from CALLS rows at read time [VERIFIED: stats is
    module-granularity FC2b/FC1, queries.rs:1382-1471; hotspots/risk
    join complexity measurements, not degree]. The verified g2/g3
    consumers replace it.)
  OPTIONS:
  - A (RECOMMENDED) — Indexer/refresh computes the ratified
    granularities from the FULL resolution stream — which post-M-6
    still covers ALL languages, because FC0 keeps being written and
    re-resolved (D-EC-2-A) — and writes them as FC1/FC4 rows (a new
    small column family or measurement kind; exact shapes are M-3a/M-3b
    implementation details); read surfaces stop deriving them from
    `edges` at read time — the g2 dead-liveness membership + g3 sketch
    (M-3a) and the trust core's g1 `resolved_calls` COUNT
    [service.rs:875] (M-3b). FC2b-derived module stats/cycles are
    explicitly OUT: their source rows stay owner-resident (D-EC-5-A),
    read-time derivation stays legitimate.
    **g3 SUB-CHOICE (decided inside A): A-i (RECOMMENDED)** — persist
    the file-pair resolved-dependency family (bounded by DISTINCT file
    pairs; dedup collapses call multiplicity — well below per-edge rows
    [INFERRED, unmeasured]); map's sketch stays language-uniform and
    deterministic. **A-ii** — no g3 family: post-M-6, map's sketch
    degrades to IMPORTS-only for covered languages, labelled per the
    VISION honesty rules (coverage stated where it renders). Cheaper
    schema; costs call-coupling discovery signal and makes the sketch's
    meaning language-conditional — weighed against commitment 3
    (milliseconds is untouched either way) and commitment 2 (A-ii needs
    honesty machinery where A-i needs none).
    Consequence of A overall: a write-path data-shape change (schema
    addition + refresh/copy-forward handling — full Persistence
    Completeness checklist applies); unlocks C-4, M-3a/M-3b, M-6;
    deterministic (same extraction, same numbers, language-complete —
    unaffected by per-language row drops).
  - B — Derive aggregates from the LiveGraph at read time. Consequence:
    all three granularities become residency- and language-conditional
    (unknown on non-covered languages → honesty machinery for a
    formerly-plain fact; a partial-residency count/liveness/pair-set is
    WRONG, not just degraded — false-dead is a trust failure);
    orient/stats/trust/check/modules/map gain an LG dependency for
    skeleton-class facts — ownership bleed into SQLite-only Cat-4 arms.
  - C — Keep deriving from SQLite `edges`. Consequence: blocks M-6
    forever (edges rows must stay for the COUNTs, the liveness
    membership, and the sketch to stay correct) — §2b's win evaporates;
    the trust core's per-call scan and the per-call liveness NOT-IN
    stay.
  RECOMMENDED: A, with A-i on the g3 sub-choice.
  BLOCKING_REASON: schema + write-path change crossing the storage
    boundary; M-3a/M-3b/M-6 are unbuildable without the pick, and the
    A-i/A-ii sub-choice decides whether map's discovery output keeps
    call-coupling or ratifies a labelled degrade (a discovery-output
    change under the change doctrine).

- ID: D-EC-8
  QUESTION: Snapshot → provenance stamp — exact semantics? TWO axes are
    decided here: the BASELINE axis (options A–D — what a mark retains)
    and the REPRESENTATION axis (REP-1/REP-2 — what a refresh writes),
    added per review-1 so no sizing can assume a representation change
    implicitly.
    INVARIANTS PRESERVED BY EVERY OPTION (review-0 item 5; review-1
    item 2): (i) the ratified W-B rule — refresh publishes N+1 WITHOUT
    deleting N's rows; a request pinned to N reads N's rows by uid;
    prune is an exclusive writer that never runs under a pinned reader
    [OBSERVED: daemon-w-b-epoch-1.md §2c :229-238]; (ii) the ratified
    retention keep-set (SNAPSHOT-RETENTION-1 §2.2): steady state
    retains up to TWO full graph-row families — current + the
    delta-base parent — plus `baseline_user` marks. Iteration-1's
    reading ("no RETAINED second copy in STEADY STATE", refresh peak
    "≈2×") is RETRACTED as contradicting that contract: the honest
    copy-count facts (revision-3 two-regime statement, review-2
    item 2) are 1–2 retained full families + baselines in QUIESCENT
    steady state; a NOMINAL single-refresh peak of THREE transient
    families under the named precondition (prunable backlog empty at
    refresh start); and 3+K — schema-UNBOUNDED — under back-to-back
    refreshes with a starved pass, drained to the keep-set by the
    first idle-window pass (§4.5 two-regime model; revision 2's flat
    "up to THREE" transient worst case is superseded).
    What §2b's stamp leg actually changes is (a) where
    IDENTITY lives (the stamp the RequestEpoch pins), (b) what a
    BASELINE mark retains (A–D below), and — only if separately
    ratified — (c) the storage REPRESENTATION (REP axis below), which
    is what per-refresh write volume actually hinges on. Literal
    single-copy storage (in-place mutation under readers, or
    mid-request eviction) would amend the W-B contract and is NOT
    offered on either axis.
  OPTIONS:
  - A — Stamp semantics: `snapshots` keeps ONE row family as the
    identity/provenance stamp of the current state (toolchain,
    comparability, epoch anchor; what RequestEpoch pins); graph-family
    rows exist for current + the delta-base while the delta path needs
    it (SNAPSHOT-RETENTION-1 steady state ≤2) + the transient refresh
    window above; `mark_baseline` marks STAMPS — comparability
    metadata + the (small) FC4 measurement rows retained per mark,
    graph-family rows NOT retained. Consequence: C-8;
    `classify_retention`/`mark_baseline` keep their surface (they
    already operate on retention_class); graph-row baseline
    comparisons degrade honestly to NOT_COMPARABLE (VISION rule 3 —
    never fake numbers) while measurement-level comparison keeps
    working; TODAY'S row-retaining `baseline_user` capability is
    REMOVED — a governance-adjacent capability change, named as such.
  - B — Keep full-row baselines (today's `baseline_user` behavior):
    user-marked snapshots retain graph-family rows. Consequence:
    row-level baseline diffs keep full power; multi-GB pinned copies
    return per mark (87k-file evidence: 3 retained snapshots = 11 GB
    [§4.5]); retention honesty must surface the cost per mark; the
    steady-state "current + delta-base only" claim gains a
    user-controlled exception.
  - C — Defer entirely to the delta-indexing track (ROADMAP queue 4).
    Consequence: C-8 unresolved; §2b's third leg unnamed — the finish
    line stays partial; M-7 unscopeable.
  - D (RECOMMENDED) — A's stamp semantics as the DEFAULT + B's row
    retention behind an EXPLICIT per-mark opt-in (`mark_baseline`
    gains a stated row-retention flag; the GB cost is surfaced at mark
    time). Consequence: the existing operator capability is preserved
    instead of silently deleted, but stops being the default; one flag
    of new surface; retention reporting shows per-mark cost either
    way. (Iteration-0 recommended this hybrid without listing it — it
    is now a first-class option per review-0 item 5.)
  REPRESENTATION AXIS (REP — orthogonal to A–D; added per review-1 so
    the write-volume story is an explicit choice, not an assumption):
  - REP-1 (RECOMMENDED NOW) — keep today's snapshot-scoped row-copy
    representation: every graph row carries `snapshot_uid`; delta
    refresh = copy-forward of unchanged-file rows (`nodes`,
    `extraction_edges`, `file_signals`, `file_versions` [OBSERVED:
    indexer_impl.rs:654-1018]) + full-snapshot re-resolution re-writing
    `edges` + `unresolved_edges` [OBSERVED: orchestrator.rs:804-972].
    Consequence: per-refresh SQLite writes stay ≈ one full snapshot's
    graph-family bytes (minus the dropped resolved-CALLS share after
    M-6, per language — §4.5 model); ZERO contract disturbance — every
    snapshot_uid-scoped reader query, the W-B pin mechanics, CASCADE
    deletion, and retention/prune all keep their current shape.
  - REP-2 — changed-row-only writes / metadata-only delta base:
    unchanged rows are NOT copied; current-state reads resolve through
    base + delta overlays (or rows become snapshot-unscoped with
    validity ranges). Consequence: per-refresh writes drop to
    O(changed files) — the KB–MB regime iteration-1 wrongly attributed
    to M-6 alone — and steady-state footprint tends to ~1 full family
    + overlays. Cost (why it is NOT folded in here): a NEW
    cross-boundary data shape — every snapshot_uid-scoped reader query
    (40+ arms, §3.3), the copy-forward port, CASCADE prune, and the
    W-B pinned-reader mechanics all change; the pinned-reader
    invariant must be re-established over overlay reads, which touches
    the ratified W-B contract — its own ratification, in
    delta-indexing-track territory (ROADMAP queue 4). No §4.5 number
    in this spec assumes it.
  RECOMMENDED: D on the baseline axis + REP-1 on the representation
    axis. REP-2 is deliberately deferred: it becomes its own
    DECISION_REQUIRED against the W-B contract if/when the queue-4
    delta-indexing track is chosen.
  BLOCKING_REASON: touches retention lifecycle, delta-refresh base
    semantics, and what `assess --baseline`/gate comparisons may claim —
    snapshot-spanning comparability rules must be decided before
    implementation (Agent Operating Model: comparability rules first).
    The REP axis additionally bounds every §4.5 write-volume claim —
    leaving it implicit is exactly how iteration-1's "KB–MB" error
    arose (review-1).

---

## Builder evidence ledger (this deliverable)

```text
EXECUTED (command run this slice, output observed):
- sed -n '282,413p' rust/crates/daemon-runtime/src/dispatch.rs | grep -c '^\s*"[a-z_]*" =>'  → 66
- grep -rn "livegraph" rust/crates/daemon-runtime/src/handlers/ → zero hits
- grep -rl "repo_state\.livegraph|\.livegraph\.read()|state\.livegraph" daemon-runtime/src/ →
  serving/cert/coherence modules only (no handlers/, no check_coherence.rs)
- wc -l livegraph_feed.rs → 4726; adapter/witness layer find|wc → 15,739; storage crate → 44,237;
  LiveGraph-stack crates → 12,247
- daemon-runtime Cargo.toml [dependencies] path-dep count → 28 (35 total [dependencies])
- git log → HEAD 2e69226 above 800d78e (TS prototype retired); git status → clean before this edit
- rmap orient → "error: daemon response timed out after 300s" (operator daemon busy/unavailable;
  per end-of-slice rules the operator registry was NOT poked further — orientation from docs +
  first-hand code reads)
OBSERVED (read first-hand this slice): dispatch.rs dispatch match + handle_orient/handle_check/
  path comment sites; livegraph_feed.rs:918-975 (path Auto arm); 001-initial.sql nodes/edges;
  migration_007 (unresolved_edges); VISION; ROADMAP; CURRENT_SLICE; daemon-w-b-epoch-1 §3/§7.3/§8;
  sqlite-raw-decommission-1 (full); readiness-10 §180-321; readiness-1 §1-§6;
  artifact-contract-model §Canonical Family Inventory; agent_docs/architecture.md;
  snapshot-retention-1 (ratified model); statuses of orient/explain-sqlite-free-1 +
  trust-summary-livegraph-1 (SPEC-FIRST) + coherence-leaf-serve-1 (SHIPPED).
OBSERVED-BY-TRACE (scoped read-only tracing passes over storage/agent/trust/gate/doc-facts/
  module-queries + handlers; entry points named in §3.3; load-bearing rows re-verified first-hand):
  the B/C/D/E group tables; mixed-handler table sets; warm-cache DTO shapes + disk layout;
  measured-size quotes (file:line cited in §4.5).
INFERRED (labelled inline): the FC taxonomy; the kernel-scale byte ranges (§4.5, assumptions
  stated; kernel DB never measured in-tree; PartitionIr bytes UNMEASURED/NOT RUN per dataflow map).
NOT RUN: cargo build/test (docs-only change — no code path touched); daemon start/index
  (state-mutating; spec slice); kernel-scale DB measurement (no kernel DB in tree).

REVISION 1 (2026-07-16, review-0 items 1-6) — evidence verified first-hand this revision:
OBSERVED: trust/src/service.rs:858-917 (resolved_calls = count_edges_by_type(snapshot_uid,
  "CALLS") at :875-877, inside assemble_trust_report's fetch block);
  storage/src/trust_impl.rs:458-521 (find_path_prefix_module_cycles — edges type='IMPORTS'
  between MODULE nodes) + :523-587 (compute_module_stats — fan_in/fan_out from IMPORTS edges);
  storage/src/agent_impl.rs:243-302 (compute_repo_summary — file_versions/nodes/files ONLY, no
  edges) + :320-344 (get_trust_summary delegates to assemble_trust_report_cancellable — the ONE
  shared core; projects resolved_calls at :344);
  consumers: agent/src/check/mod.rs:134 (check), agent/src/explain/mod.rs:855 (explain),
  agent/src/aggregators/trust.rs:55 (orient), agent/src/check/evaluate.rs:91;
  daemon-w-b-epoch-1.md §2c :229-238 (refresh never deletes prior-snapshot rows; prune is an
  exclusive writer — the pinned-reader invariant D-EC-8 now preserves explicitly);
  sqlite-raw-decommission-readiness-1.md §5 :143-158 (the five deletion gates, gate texts);
  snapshot-retention-1.md (steady state ≤2 = current + delta-base; baseline_user kept).
EXECUTED: grep count_edges_by_type across rust/crates → definition (storage trust_impl:176),
  the ONE production call site (service.rs:876), tests/mocks only otherwise — basis for the
  "one core, five consumers" claim (deterministic grep, full-tree).

REVISION 2 (2026-07-16, review-1 items 1-2) — evidence verified first-hand this revision:
OBSERVED: daemon-runtime/src/dispatch.rs:1187 (handle_callers) + :1293 (handle_callees) —
  each eagerly calls storage.resolve_symbol(epoch.snapshot_uid(), symbol): the permanent FC1
  `nodes` owner-read C-3 now retains (path's pair at :2295/:2318, matching §3.3-A);
  storage/src/indexer_impl.rs:654-1018 (DeltaCopyPort::copy_forward_unchanged_files, steps
  1/1b/2/3/4) — unchanged-file `nodes` (ALL columns incl. signature/doc_comment/metadata_json),
  null-file resource nodes, `extraction_edges`, `file_signals`, `file_versions` are copied into
  the NEW snapshot_uid;
  indexer/src/orchestrator.rs:1500-1548 (refresh creates the child snapshot then invokes
  copy-forward) + :804-972 (the resolution loop batches over ALL extraction_edges of the NEW
  snapshot — copied + fresh — and inserts the full `edges`/`unresolved_edges` families;
  Phase 4 re-creates module edges) — basis for "per-refresh writes ≈ one full snapshot's
  graph-family bytes";
  storage/src/migrations/migration_012.rs:37-55 (`extraction_edges` DDL: persistent,
  snapshot-scoped, CASCADE, 3 indexes; no DELETE path anywhere in storage/src — grep);
  storage/src/retention/classify.rs:51-77 + :116-171 (keep-set: current = latest valid-epoch
  READY; parent = current's parent, kept as delta-base mechanics; everything else prunable;
  baseline_user immune) — the ratified ≤2 + baselines steady state;
  daemon-runtime/src/retention_pass.rs:1-20 (the pass is spawned BACKGROUND after successful
  index/refresh, contention-yielding) — basis for the transient 3-family window being an
  operational bound.
EXECUTED: grep "DELETE FROM extraction_edges|delete_extraction" storage/src → zero hits;
  git log --follow migration_012.rs → in tree since 2026-04-11 (b9cc7d5 lineage) — the §4.5
  DB-file anchors postdate it, so they already include the family.

REVISION 3 (2026-07-16, review-2 items 1-3) — evidence verified first-hand this revision:
OBSERVED (item 1, FC0): migrations/migration_012.rs:37-55 (extraction_edges DDL — RAW target_key
  TEXT, resolution, extractor, line/col, metadata; snapshot-scoped CASCADE, 3 indexes);
  indexer/src/resolver.rs:100-197 (resolve_edges: every ExtractedEdge → ResolvedEdge OR
  CategorizedUnresolvedEdge — the FC2a/FC2b vs FC3 split point);
  storage/src/grpc_impl_hint_impl.rs:60-105 (+ :215/:518 — GR-1A queries read
  IMPLEMENTS/CALLS-typed extraction_edges) with indexer/src/refresh_dispatch.rs:134
  (run_grpc_impl_hint_detection invoked INSIDE index/refresh — pipeline-time, not dispatch);
  storage/src/metrics.rs:108-125 (extraction_edges tier map: Tier B "rebuildable", layer 0-1).
EXECUTED (item 1) [TRANSCRIPT CORRECTED IN REVISION 4 — review-3 item 2: the command as
  recorded here does not reproduce the stated result — unrestricted it returns 15 files.
  The command matching the 9-file result is
  `grep -rln --include='*.rs' extraction_edges rust/crates` → 9 files, i.e. RUST-SOURCE-ONLY,
  not full-tree; the revision-3 "deterministic full-tree grep" label was an overclaim for this
  transcript line. Full-tree = 15, all classified in the REVISION 4 block below.]
  The nine Rust-source hits, classification unchanged: writers (indexer_impl copy-forward,
  grpc_impl_hint_port_impl INSERTs incl. test fixtures), pipeline readers (orchestrator batch,
  grpc_impl_hint_impl SELECTs), the indexer storage-port trait (storage_port.rs — pipeline
  method signatures), migrations (migration_012 + mod), metrics tier map, one doc comment
  (storage/lib.rs:49) — ZERO dispatch-arm readers (§3.4-9 basis; classified by reading each
  hit's context).
OBSERVED (item 2, retention regimes): daemon-runtime/src/retention_pass.rs IN FULL —
  spawn_auto_retention (background thread after successful index/refresh; enrich-chained or
  direct), try_retention_attempt (two gates: activity registry + non-blocking DB write lock;
  yields whenever another op writes), run_auto_retention (REQUEUE_MAX_ATTEMPTS=60 ×
  REQUEUE_BACKOFF=1s, then terminal "deferred … the next index/refresh retries"),
  run_retention_pass (classify → prune ALL prunable READY + orphaned non-READY in ONE pass →
  threshold-gated VACUUM) — basis for: nothing gates refresh admission on a drained backlog;
  writers always win; one idle pass drains the whole backlog. Named tests read:
  steady_state_keeps_current_and_parent_prunes_older, retention_yields_under_contention.
OBSERVED (item 3, the census): storage/src/queries.rs:1031-1060 (find_dead_nodes — NOT-IN over
  edges type IN (IMPORTS, CALLS, IMPLEMENTS, INSTANTIATES, ROUTES_TO, REGISTERED_BY, TESTED_BY,
  COVERS)) + agent_impl.rs:556-610/:613-666 (path/file variants, same exclusion);
  daemon-runtime/src/dispatch.rs:7595 (modules_show) + :7862 (modules_list) — both call
  find_dead_nodes(…, Some("SYMBOL")) unconditionally and SERVE the rollup (DeadNodeFact);
  agent/src/aggregators/dead_code.rs:42-80 — aggregate/aggregate_file/aggregate_path return
  Ok(AggregatorOutput::empty()) UNCONDITIONALLY, storage parameter unused ("surface withdrawn")
  → orient/explain do NOT execute the dead query despite stale module-doc comments claiming
  they do (orient/path.rs:5, orient/file.rs:6 — name-vs-behavior, noted);
  rgr/src/commands/dead.rs:45-78 (the dead CLI is disabled — no dispatch arm);
  orient/symbol.rs:104 (dead_code signal removed); explain/mod.rs:843 (surface withdrawn);
  storage/src/queries.rs:2615-2640 (map_resolved_dep_edges_in_path — type IN (IMPORTS,CALLS)
  collapsed to DISTINCT file pairs, "per-directory dependency sketch (both types)");
  storage/src/queries.rs:1380-1471 (module-rows stats query: module fan_in/fan_out CTEs over
  IMPORTS edges between MODULE nodes + OWNS/nodes rollups — MODULE granularity, no CALLS).
EXECUTED (item 3): grep "'CALLS'|\"CALLS\"" storage/src → every non-test hit classified:
  count_edges_by_type (trust g1), find_dead_nodes family (g2), find_shortest_path
  CALLS∪IMPORTS (path BFS), map_resolved_dep_edges_in_path (g3), find_direct_callers/callees
  (FC2a walkers), grpc_impl_hint (FC0 pipeline) — the §3.4-10 census is grep-complete over
  storage/src (deterministic; agent-crate SQL confirmed via agent_impl sites above).
  grep -rn "fan_in|fan_out" rust/crates → module-granularity sites only (trust_impl, queries
  module-stats, livegraph module rows, presentation) — NO per-function CALLS-degree read
  surface exists today (basis for retracting D-EC-7's "stats/hotspots/orient degree reads").
NOT RUN (this revision): cargo build/test, smoke, dogfood (docs-only change — no code path
  touched); rmap orientation (operator daemon not poked for a docs-only revision — iteration-0
  precedent: 300s timeout); kernel-scale DB measurement (no kernel DB in tree).

REVISION 4 (2026-07-16, review-3 items 1-2) — evidence verified first-hand this revision:
EXECUTED (item 2 — both grep commands re-run this revision, output observed):
  grep -rln extraction_edges rust/crates → 15 files (the unrestricted full-tree command);
  grep -rln --include='*.rs' extraction_edges rust/crates → 9 files (the Rust-source-only
  restriction — the command the revision-3 entry's "9 files" actually corresponds to; that
  entry is corrected in place above). All 15 classified: the 9 Rust-source hits per the
  corrected revision-3 entry, plus 6 non-source hits — NONE executable, none a reader:
  4 rgistr-generated per-file map docs (`*_rs_MAP.md` beside their sources:
  grpc_impl_hint_impl, grpc_impl_hint_port_impl, indexer_impl, migration_012 — header
  `generated_by: rgistr` [OBSERVED]); 1 enrichment test fixture
  (enrichment/src/testdata/ey1b_selfindex_corpus.json — self-index corpus rows whose
  target_key/stable_key TEXT mentions the table); 1 crate-manifest scope comment
  (storage/Cargo.toml:28). ZERO dispatch-arm readers — the §3.4-9 conclusion UNCHANGED,
  now verified over the FULL tree.
OBSERVED (item 1, sizing basis): 001-initial.sql:104-124 (edges DDL: TEXT PK + FIVE named
  composite indexes — the table the ×1.8–2.5 all-in anchor describes);
  migration_012.rs:37-55 re-read (extraction_edges: TEXT PK + THREE named indexes —
  idx_…_snapshot single-column, idx_…_source_file + idx_…_cursor 2-column, all TEXT-keyed);
  ey1b_selfindex_corpus.json:2199 (a target_key holding a multi-line raw expression — basis
  for "the 200–350 B payload assumption leans LOW for FC0").
INFERRED (item 1, the corrected range — assumptions stated in §4.5): per-index-B-tree share
  (×1.8–2.5 − 1)/6 ≈ 0.13–0.25 from the edges anchor; 4 B-trees ⇒ ×1.5–2.0 all-in;
  0.96–1.68 GB payload ⇒ 1.47–3.36 ⇒ stated ≈1.4–3.4 GB/copy, rounded OUTWARD (uncertainty
  bounds widen, never narrow). Knock-on: FC3-vs-FC0 individual ordering UNMEASURED
  (overlapping ranges) — "the floor dominates" restated as the floor-bound pair throughout.
NOT RUN (this revision): cargo build/test, smoke, dogfood (docs-only change — no code path
  touched); rmap orientation (operator daemon not poked for a docs-only revision — iteration-0
  precedent stands); kernel-scale DB measurement (no kernel DB in tree — the corrected FC0
  range stays INFERRED/UNMEASURED, deliberately wide).
```

Decide-and-record (local, one line each): deliverable sections numbered
§3–§6 per §2's own naming; the pre-existing administrative sections
(stop conditions / validation / definition of done — §3/§4/§5 at
selection time, as the relay packet cites them) are renumbered §7/§8/§9
below, text UNCHANGED. Fact-class IDs FC1–FC8 and decision IDs
D-EC-1…D-EC-8 follow existing house conventions.

Revision-1 decide-and-record: the derived-count class is named `FC2a-agg`
(suffix, not a new FC number — it is a consumption mode of FC2a, not a new
extraction family); M-3 split as `M-3a`/`M-3b` to keep M-4…M-7 IDs stable
across review rounds; FC2b-derived module aggregates deliberately NOT
named (no ownership question arises — §3.2 note states the asymmetry).

Revision-2 decide-and-record: the D-EC-8 representation cells are named
`REP-1`/`REP-2` (NOT `R-1`/`R-2` — §4.2's FC2a language regimes already
use R1/R2/R3; reusing the letter would mint a name collision); the REP
axis lives INSIDE D-EC-8 per review-1's explicit placement ("add that
choice explicitly to D-EC-8") rather than as a new D-EC-9, marked
orthogonal to A–D so the matrix stays exhaustive without falsely
presenting it as mutually exclusive with the baseline options;
superseded revision-1 claims are retracted IN PLACE with the correction
named (never silently rewritten), per the do-not-erase-superseded-records
doctrine.

Revision-3 decide-and-record: the pipeline-input class is named **FC0**
(not FC9 — it is upstream INPUT to the served classes, not a ninth
served class; 0 matches its Layer-0 shape and its exclusion from the
read-path ownership question; no existing FC0 to collide with); the
FC2a-agg granularities are named **g1/g2/g3** (suffix within the
existing FC2a-agg class — naming them FC2a-agg-1… would suggest three
classes where there is one class with three granularities); FC0's
ownership is ratified through the EXISTING D-EC-1 table + widened
D-EC-2 (no new top-level D-EC minted: every FC0 alternative is
foreclosed by a ratified contract, so there is no live top-level
choice — the cells exist for exhaustiveness); the g3 map sub-choice
(A-i/A-ii) lives INSIDE D-EC-7 (it is a granularity of the same
write-path decision, mirroring the review-1 REP-axis placement
precedent); the discovered stale module-doc comments
(orient/path.rs:5, orient/file.rs:6 — claim dead queries the withdrawn
aggregators no longer make) are SURFACED here as name-vs-behavior
defects and folded into D-EC-4-A's existing paper-pass scope (code out
of scope this slice); the refresh-peak restatement PRESERVES both
ratified contracts (SNAPSHOT-RETENTION-1 keep-set, W-B window) — it
corrects only this spec's own bound claim, so it is decide-and-record,
not DECISION_REQUIRED.

Revision-4 decide-and-record: the index multiplier is derived
per-index-B-TREE (counting the TEXT-PK autoindex both tables share)
rather than per-named-index — it counts what SQLite actually
materializes and avoids under-attributing the autoindex to the
3-index table; INFERRED range endpoints are rounded OUTWARD (1.47→1.4,
3.36→3.4) — uncertainty bounds widen, never narrow (honesty rule:
degenerate precision reads as measurement). Both are estimate-model
choices inside this spec's own sizing, no boundary touched. The
`edges` (0.9–1.8) and `unresolved_edges` (1.3–2.4) ranges are
deliberately NOT re-derived: review-3 flagged only FC0's label
inconsistency, and both carry the all-in factor correctly; churning
unflagged figures would manufacture diff noise.

---

## 7. Stop conditions *(§3 at selection time — text unchanged)*

- NO code, schema, or contract changes in this slice — spec only.
- If the inventory contradicts a ratified prior decision (e.g. the RED floor
  or a W-B epoch invariant), surface the contradiction as DECISION_REQUIRED;
  do not silently reinterpret it.
- Do not propose retiring the SQLite floor — it is ratified as permanent.

## 8. Validation *(§4 at selection time — text unchanged)*

- The four sections above exist in this doc, the §3 inventory covers every
  dispatch handler (count stated and reconciled against `dispatch.rs`), and
  every proposal in §4/§5 cites §3 evidence.
- No working-tree changes outside this file.

## 9. Definition of done *(§5 at selection time — text unchanged)*

This doc contains an evidence-backed read-path inventory, a fact-class
ownership proposal honoring the ratified floors, a checkable end-state
definition with shippable milestones, and an explicit DECISION_REQUIRED list
— ready for decision-review and human ratification. (IMPL slices follow only
after ratification.)

---

## 8. RATIFICATION (operator + human, 2026-07-16)

**D-EC-2, D-EC-3, D-EC-4, D-EC-5, D-EC-6, D-EC-8 — RATIFIED AS WRITTEN** (all converged
in decision-review; see `ratification-packet.md` in the slice workspace for the challenge/
rebuttal audit trail). The named end-state stands: **SQLite owns the structure skeleton +
pipeline plumbing + the RED floor; LiveGraph owns function internals for covered
languages; mixed handlers are sanctioned cross-class composers; superseded per-surface
migration plans are retired; snapshots are provenance stamps.**

**D-EC-1 / D-EC-7 (the contested pair) — SUPERSEDED by a ratified DIRECTION CHANGE:
reconciliation over adjudication** (human decision, 2026-07-16). The dispute assumed one
producer must own the derived aggregates. The ratified frame instead: the two producers
are two *witnesses* of the same truth whose differences are themselves evidence —
understand and classify the divergence, then USE both:

- **(a) Divergence-classification spike (FIRST, evidence before design):** the callgraph
  certificate already computes an exhaustive per-symbol comparison and discards the
  detail. Instrument it to EMIT the diff; run on real repos (repo-graph self-index; the
  160k-file monorepo when field data exists); CLASSIFY every mismatch by direction
  (SCIP-only vs pipeline-only) and cause (semantic resolution vs compilation-failure vs
  coverage gap). Expected asymmetry: SCIP wins where the compiler succeeds; the pipeline
  wins where compilation fails or coverage ends — both-ways divergence, never yet
  measured (`dataflow-hotpath-map.md` residuals).
- **(b) Reconciliation layer (designed FROM the spike's data):** a union graph with
  per-edge witness provenance — agreement = highest-confidence (two independent
  witnesses); SCIP-only = "semantic resolution (compiler-verified)" basis; pipeline-only
  = "syntactic extraction (works where compilation fails)" basis. Aggregates compute over
  the RECONCILED graph with coverage labels; the divergence rate becomes a surfaced
  per-repo fact (the promotion-funnel pattern). Pipeline-unresolved/SCIP-resolved sites
  land as labeled Layer-2 facts — never silently inflating the trust ratio (whose
  denominator remains the ratified pipeline-only floor).
- **(c) INTERIM RULE (until reconciliation ships):** FC2a-agg persists pipeline-derived
  values (one coherent accounting, matching the trust denominator) with explicit
  provenance labels, per the builder's amended D-EC-1/D-EC-7 cell — ratified as
  EXPLICITLY TEMPORARY, not terminal. The §5 milestones (M-1..M-6) proceed under this
  rule and do not wait for reconciliation; M-6's divergence gate stands.

Follow-on slices: RECON-SPIKE-1 (a), RECON-DESIGN-1 (b, spec slice), then IMPL milestones
per its ratification.
