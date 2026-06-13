# ORIENT-SQLITE-FREE-1: eliminate `rmap orient`'s eager SQLite `nodes`/`edges` base read (Stage D)

Slice ID: ORIENT-SQLITE-FREE-1
Status: **SPEC-FIRST — specification only. NO implementation, NO code, NO deletion, NO migration, NO default
flip.** This document specifies how `rmap orient` would ELIMINATE its unconditional/eager SQLite base read,
serving from current-state LiveGraph when a cert is GREEN and reading SQLite ONLY on fallback — mirroring the
imports/cycles/stats cert-fastpath posture, built ON TOP of the shipped orient `CoherenceEnvelope`
(ORIENT-LIVEGRAPH-IMPL `2fd4478`).
Track: Stage D / SQLite-raw decommission — Option B (eliminate the coherence commands' eager SQLite base
reads). First command: orient.
Baseline: SQLITE-RAW-DECOMMISSION-READINESS-9 (`56160bb`); ORIENT-LIVEGRAPH-1 (`af49ea6`) + IMPL (`2fd4478`).

> **HEADLINE FINDING (load-bearing, read before the design): orient's eager `nodes`/`edges` read CANNOT be
> eliminated on green by serving its four ratified LiveGraph-first leaves alone.** First-hand source reads
> (below) establish that orient's base use case reads `nodes`/`edges` through FIVE paths, only TWO of which
> are covered by an existing leaf cert (cycles, callgraph). The other three — **trust-core** (`edges` +
> `unresolved_edges`, UNCONDITIONAL in all four focus pipelines), **MODULE_SUMMARY counts** (`nodes`), and
> **focus resolution** (`nodes`, focused orient) — plus the two conditional reads (BOUNDARY_VIOLATIONS
> `edges`, gate arch_violations `edges`) are each either (a) ratified SQLite-first by ORIENT-LIVEGRAPH-1, or
> (b) have NO LiveGraph producer today. Eliminating them is an architecture-boundary + new-producer program,
> not a single cert-flip. The five blocks are surfaced as `DECISION_REQUIRED` (§8). This mirrors exactly how
> STATS-LIVEGRAPH-1 surfaced its D0 architecture-boundary block and split off a prerequisite
> (IR-SYMBOL-ATTRIBUTES-1) before the fastpath impl. The achievable design (§5–§7) is specified CONDITIONAL on
> those decisions; what it does and does NOT achieve is stated honestly in §6.

> **DECISION RESOLUTION — DR-0-SEQUENCING (ratified by operator, 2026-06-13): S3 (reassign lead).**
> `explain` leads Option B (`EXPLAIN-SQLITE-FREE-1`, spec-first); **orient is DEFERRED.** Rationale: explain
> already SERVES 5 green leaf VALUES from the LiveGraph and carries no unconditional trust-core dependency of
> orient's weight, so it is the likeliest first REAL eager-read elimination without a producer program.
> `ORIENT-SQLITE-FREE-IMPL-1` remains gated on its prerequisites — minimally **DR-1** (a `TRUST-SUMMARY-LIVEGRAPH-1`
> producer: the decisive, unconditional, all-focus blocker, SHARED with the `trust` command) and **DR-2**
> (MODULE_SUMMARY structural-count re-source) — each its own separately-ratified slice. **S2 (orient-leads-anyway)
> was REJECTED** by builder + reviewer: the composite cert is RED by construction while trust-core has no
> LiveGraph producer, so an orient fastpath built now is dead, fallback-only code (§4b). This spec stands as the
> authoritative orient producer-gap map; DR-1..DR-5 (§8) remain OPEN, to be ratified when orient is resumed.

> **PRODUCER UPDATE (2026-06-13): DR-1 (the shared trust-core producer) is REFUTED.** `TRUST-SUMMARY-LIVEGRAPH-1`
> + `SCIP-UNRESOLVED-CALL-PROBE-1` proved SCIP cannot source a parity unresolved-call count (NO-GO); operator
> ratified **Option A** — the trust contributor stays homegrown-`unresolved_edges` SQLite-LABELLED. Consequence
> for orient: its trust leaf can NEVER be `edges`-free on green (gate-1 RED by design for that leaf). A future
> `ORIENT-SQLITE-FREE-IMPL-1`, if pursued, can only serve the LG-DERIVABLE contributors (cycles/callgraph/module
> counts) and must keep the trust contributor SQLite-labelled. DR-2..DR-5 (the non-trust leaves) are unaffected.

---

## 0. Spec-first note (read first)

This is a SPECIFICATION. It produces exactly one deliverable: this file. NO source path is touched; no cert is
built; no default is flipped; `nodes`/`edges` are not read by this slice (only the SHIPPED handlers read them,
which this doc audits). The eventual implementation (ORIENT-SQLITE-FREE-IMPL-1, a LATER slice) is gated on the
§8 decisions being ratified first — and, per §8, on at least one NEW prerequisite producer slice (trust-core
LiveGraph projection) landing before orient can serve a `nodes`/`edges`-free repo-focus answer.

Per the repo split rule (CLAUDE.md: spec before impl; ratify architecture-boundary decisions before building),
this slice's DEFINITION OF DONE is the specification + the surfaced decisions, not a working fastpath.

### Evidence labels (repo Evidence Law)

- **OBSERVED** = inspected first-hand THIS slice (a file read I performed this turn, cited file:line).
- **INFERRED** = my classification/judgment over OBSERVED facts (the serve-vs-fallback verdicts, the cert
  design, the next-step recommendation).
- **EXECUTED** = a command I ran this turn with output observed.
- **NOT RUN** = skipped, with reason.

### Evidence basis (this audit)

```text
git: HEAD=`56160bb` (PRIORITY-DOCS-RECONCILE-3 + readiness-9) [OBSERVED: git log --oneline -15]. The coherence
chain `6ed17b8..dc55114` (contract + amendment + 4 specs + 4 impls) sits below it; ORIENT-LIVEGRAPH-IMPL =
`2fd4478`.

Daemon NOT started; no index/refresh/dev-install run (state-mutating + out of scope for a spec). All claims
about orient's read surface are grounded in FIRST-HAND SOURCE reads of the shipped agent use case, the
aggregators, the storage adapter, and the LiveGraph crate — the stronger evidence basis for a claim about code
structure than a live capture. Every OBSERVED claim carries file:line.

Files read first-hand THIS slice:
- rust/crates/agent/src/orient/mod.rs (focus dispatch + resolution)
- rust/crates/agent/src/orient/{repo,symbol,path,file}.rs (the four focus pipelines)
- rust/crates/agent/src/aggregators/{snapshot,trust,cycles,boundary,boundary_links,dead_code,module_summary,
  gate,complexity}.rs (the nine aggregators)
- rust/crates/storage/src/agent_impl.rs (compute_repo_summary:215, get_trust_summary:276, get_module_summary:
  1297, get_boundary_links_freshness:1351, resolve_path_focus:366, resolve_stable_key_focus:437,
  resolve_symbol_name:800, get_symbol_context:834, find_symbol_callers:883, find_symbol_callees:935)
- rust/crates/storage/src/trust_impl.rs (the trust-core edge reads — :116/:149/:265/:344/:353)
- rust/crates/daemon-runtime/src/dispatch.rs:2550-2656 (handle_orient: base read :2603, envelope build :2631)
- rust/crates/daemon-runtime/src/orient_coherence.rs (the shipped overlay adapter)
- rust/crates/repo-graph-livegraph/src/lib.rs (callers:469, callees:586, value_facts:688,
  module_import_cycles:1317, module_stats:1376 + symbol_count:2051)
- docs/slices/orient-livegraph-1.md §2 (the ratified per-signal source map)
- docs/slices/stats-livegraph-1.md (the cert-fastpath precedent + its D0 architecture-boundary split)
- docs/slices/sqlite-raw-decommission-readiness-9.md (the Option-B driver)
```

---

## 1. Why now (priority path)

OBSERVED [readiness-9 §Recommendation]: the ratified Stage-D order is
`COHERENCE-LAYER-1 ✓ → SQLITE-RAW-DECOMMISSION-READINESS-9 ✓ (gate RED) → SQLITE-RAW-DECOMMISSION-1 (terminal;
GATED)`. readiness-9 recomputed the deletion gate as RED — all five gates FAIL — and recommended **Option B
(eliminate the coherence eager reads)** as the incremental next build, with **orient first**. This slice is the
SPEC for that first step.

OBSERVED [readiness-9 §Per-command table, dispatch.rs:2603]: the coherence layer (ORIENT-LIVEGRAPH-IMPL) did
NOT remove orient's eager SQLite read. `handle_orient` runs the base use case `repo_graph_agent::orient(
&repo_state.storage, …)` UNCONDITIONALLY at dispatch.rs:2603, THEN `orient_coherence::build_orient_envelope`
(:2631) OVERLAYS per-leaf provenance/trust/freshness labels on top of the already-SQLite-built result. So
orient's served path reads `nodes`/`edges` on EVERY call; the four LG-first leaves are LABELS, not value
sources (orient_coherence.rs:118 `to_coherent(result, &decisions, …)` — `result` is the SQLite-built
`OrientResult`; `decisions` only carry leaf LABELS). This slice targets that eager read.

VISION alignment: `orient` is the FIRST command an agent calls (VISION "discovery-first agent loop" step 1).
Removing its eager `nodes`/`edges` read is a precondition for retiring the `nodes`/`edges` substrate (the
Stage-D terminal slice) AND for the in-memory-graph end state (VISION "Operational Architecture": current repo
state in memory is primary truth, SQLite is the transition mechanism).

---

## 2. What `rmap orient` reads today — the eager base read (OBSERVED, first-hand)

`handle_orient` (dispatch.rs:2550) calls `repo_graph_agent::orient` (dispatch.rs:2603), which dispatches on
focus (mod.rs:64-68):

- `focus = None` → `orient_repo` (repo.rs:58) — NO focus resolution.
- `focus = Some(s)` → `orient_focused` (mod.rs:76) — resolves `s` to a FILE/MODULE/SYMBOL node, then routes to
  `orient_file` / `orient_path` / `orient_symbol`.

### 2a. Focus resolution reads (focused orient ONLY; `focus=None`/repo skips this) — OBSERVED

| Read | Storage method | Table | `nodes`/`edges`? | file:line |
|------|----------------|-------|------------------|-----------|
| path-area focus | `resolve_path_focus` | `nodes` | **YES (nodes)** | mod.rs:101 → agent_impl.rs:366 (`FROM nodes n`) |
| stable-key focus | `resolve_stable_key_focus` | `nodes` | **YES (nodes)** | mod.rs:139 → agent_impl.rs:437 |
| symbol-name focus | `resolve_symbol_name` | `nodes` | **YES (nodes)** | mod.rs:191 → agent_impl.rs:800 |
| symbol context | `get_symbol_context` | `nodes` | **YES (nodes)** | mod.rs:142/202 → agent_impl.rs:834 |

### 2b. Aggregator reads (the focus pipelines) — OBSERVED

The repo pipeline (repo.rs:81-143) runs nine aggregators in fixed order; the focused pipelines (symbol/path/
file) run a focus-scoped subset. Reads, with `nodes`/`edges` dependence:

| # | Aggregator | Storage read(s) | Table(s) | `nodes`/`edges`? | Focuses that run it |
|---|------------|-----------------|----------|------------------|---------------------|
| A | snapshot::aggregate (snapshot.rs:13) | — (pure; uses already-fetched `AgentSnapshot`) | — | NO | all |
| B | get_repo / get_latest_snapshot (mod.rs:84/91, repo.rs:65/72) | `repos`, `snapshots` | NO | all |
| C | trust::aggregate (trust.rs:39-40) | `get_trust_summary` → `assemble_trust_report`; `get_stale_files` | **`edges` + `unresolved_edges`** (trust_impl.rs:116/149/265/344/353); `file_versions` | **YES (edges)** for trust-core; NO for stale | **all (UNCONDITIONAL)** |
| D | cycles::aggregate / aggregate_path (cycles.rs:21/58); symbol `find_cycles_involving_module` (symbol.rs:116) | `find_module_cycles` / `find_cycles_involving_path` / `find_cycles_involving_module` | `edges` (IMPORTS) | **YES (edges)** | repo, path; symbol (if owning module) |
| E | boundary::aggregate / aggregate_path (boundary.rs:50/70); symbol exact (symbol.rs:108) | `get_active_boundary_declarations` / `find_boundary_declarations_in_path`; `find_imports_between_paths` | `declarations` (Authority); `edges` (IMPORTS) | **YES (edges)** — only if ≥1 declaration | repo, path, symbol (if owning module) |
| F | boundary_links::aggregate (boundary_links.rs:25) | `get_boundary_links_freshness` | `boundary_interaction_links` (agent_impl.rs:1351) | NO | repo only |
| G | dead_code::aggregate/_path/_file (dead_code.rs:42/58/72) | — (surface WITHDRAWN; returns `empty()` unconditionally) | — | NO | repo, path, file (no-op) |
| H | module_summary::aggregate/_path/_file (module_summary.rs:46/97/127) | `compute_repo_summary` / `compute_path_summary` / `compute_file_summary`; `get_module_summary` | **`nodes`** (symbol_count, agent_impl.rs:241) + `file_versions`/`files`; `module_candidates` (agent_impl.rs:1297) | **YES (nodes)** for counts; NO for module_candidates | repo (full), path/file (counts only) |
| I | gate::aggregate/_path (gate.rs:47/99); symbol exact (symbol.rs:124) | `get_active_requirements`; `assemble_from_requirements` | `declarations` (Authority); arch_violations method → `edges`; coverage/complexity/hotspot → `measurements`/`inferences` | **YES (edges)** — only if an arch_violations obligation exists | repo, path, symbol (if owning module) |
| J | complexity::aggregate (complexity.rs:23, repo.rs:138 gated on `has_complexity_measurements`) | `count_high_complexity_symbols`; `query_high_complexity_symbols` | `measurements` | NO (measurements, NOT nodes/edges) | repo only |
| K | callers/callees (symbol.rs:89/96) | `find_symbol_callers` / `find_symbol_callees` | `edges` (CALLS) (agent_impl.rs:883/935) | **YES (edges)** | symbol only |
| L | documentation (repo.rs:161 / *.rs build_documentation_section) | `get_doc_inventory` | **FS** (doc-facts live discovery; repo.rs:208 comment) | NO (FS, not SQLite) | all |

### 2c. The `nodes`/`edges` reads, by focus (the decommission target) — INFERRED from §2a/§2b

```text
REPO focus  (focus=None):  C(trust edges, uncond) · D(cycles edges, uncond) · H(module_summary nodes, uncond)
                           · E(boundary edges, IF declarations) · I(gate edges, IF arch obligation)
SYMBOL focus:              focus-resolution C2/C4(nodes) · trust(edges, uncond) · K(callers/callees edges, uncond)
                           · D-MC(cycles edges, IF module) · E-MC(boundary edges, IF decl) · I-MC(gate edges, IF arch)
PATH focus:                focus-resolution C1/C2(nodes) · trust(edges, uncond) · D(cycles edges, uncond)
                           · H(path-summary nodes, uncond) · E(boundary edges, IF decl) · I(gate edges, IF arch)
FILE focus:                focus-resolution C1/C2(nodes) · trust(edges, uncond) · H(file-summary nodes, uncond)
                           (file focus emits NO cycles/complexity/callers — §1c of orient-livegraph-1)
```

REFINEMENT OF READINESS-9 [OBSERVED, not a contradiction]: readiness-9 §Default-path table summarized orient's
base `nodes`/`edges` read as "find_module_cycles + module_summary" (the repo-focus headline). First-hand this
is a REPRESENTATIVE subset, not the full set: **trust-core (`edges`+`unresolved_edges`) is an additional
unconditional `edges` read in every focus**, and boundary/gate/focus-resolution/callgraph add more. readiness-9
remains correct (orient reads `nodes`/`edges` eagerly; the eager read survives coherence); this slice EXTENDS
its enumeration to the precision a serve-vs-fallback design requires. STOP-condition check (packet): orient's
base use case + cert machinery were located and are CONSISTENT with readiness-9 — no contradiction; no stop on
that ground.

---

## 3. Per-source serve-vs-fallback classification (the field-level boundary)

The question the packet asks per source: **can it be served from current-state LiveGraph when GREEN, or must
it stay SQLite (fallback only)?** Verdicts are keyed to the ratified ORIENT-LIVEGRAPH-1 §2 source map
(posture), the LiveGraph surfaces that exist (lib.rs), and whether a NEW producer would be required.

Legend: **LG-servable-now** = a LiveGraph surface + a no-loss cert exist (the four ratified leaves).
**LG-derivable-but-ratified-SQLite** = the LiveGraph CAN compute it, but ORIENT-LIVEGRAPH-1 deliberately kept
it SQLite-first (re-sourcing crosses a ratified decision). **No-LG-producer** = nothing in the LiveGraph
produces it; a new producer/extraction would be required. **Not-a-decommission-target** = the read is SQLite
but does NOT touch `nodes`/`edges` (Authority/Tier-B/operational/FS), so it does not block the `nodes`/`edges`
retirement even if it stays.

| Source (§2 ref) | `nodes`/`edges`? | ORIENT-LIVEGRAPH-1 §2 posture | LiveGraph surface | Serve-on-green verdict |
|-----------------|------------------|-------------------------------|-------------------|------------------------|
| `IMPORT_CYCLES` (D) | YES edges | **LG-first** | `module_import_cycles` (lib.rs:1317) + cycles no-loss cert | **LG-servable-now** — cert exists; serve on green, SQLite `find_module_cycles*` on fallback. |
| `CALLERS_SUMMARY`/`CALLEES_SUMMARY` (K) | YES edges | **LG-first** | `callers`/`callees` (lib.rs:469/586) + per-symbol no-loss compare | **LG-servable-now** — cert exists (symbol focus). |
| `HIGH_COMPLEXITY` (J) | NO (measurements) | **LG-first** (cyclomatic) | `value_facts` CyclomaticComplexity (lib.rs:688) + complexity cert | **LG-servable-now** — cert exists. NOTE: its SQLite read is `measurements`, NOT `nodes`/`edges`; serving it from LG is honest but does NOT itself reduce the `nodes`/`edges` surface. |
| **trust-core** (C) — `TRUST_*` + `confidence` + `trust_briefing` | **YES edges + unresolved_edges** | **SQLite-first** ("hybrid trust rebase is TRUST-LIVEGRAPH-1, not here") | partial — TRUST-LIVEGRAPH-1 adds a posture BESIDE v1; it does NOT replace the edge-derived call-resolution/reliability core | **No-LG-producer** for the VALUES orient consumes (resolved/unresolved counts, reliability axes, stale). UNCONDITIONAL in all four focuses. → **DR-1 (BLOCKING).** |
| **MODULE_SUMMARY counts** (H) — file/symbol/languages | **YES nodes** | **SQLite-first** (D-ORIENT-2 `module_candidates` anchor + RISK-E identity divergence) | `module_stats` (lib.rs:1376; symbol_count:2051) PROVES the count is LG-derivable (stats-livegraph) | **LG-derivable-but-ratified-SQLite.** Re-sourcing crosses ORIENT-LIVEGRAPH-1. → **DR-2 (BLOCKING).** |
| **focus resolution** (C2a) | **YES nodes** | — (not in §2; resolution layer) | none — focus→node resolution was never migrated to the LiveGraph | **No-LG-producer.** Affects every FOCUSED orient (repo/`None` is exempt). → **DR-4 (BLOCKING for focused orient).** |
| `BOUNDARY_VIOLATIONS` structural half (E) | YES edges (cond.) | **Authority + SQLite-first** ("structural import-edge half … kept SQLite-first per contract") | imports surface is migrated (could answer "imports between A and B") | **LG-derivable-but-ratified-SQLite.** Conditional (declarations exist). → **DR-3 (BLOCKING for full elimination).** |
| `GATE_*` arch_violations structural half (I) | YES edges (cond.) | **Authority** ("declarations have no LiveGraph home by construction") | imports surface (for the arch_violations edge check only) | **LG-derivable-but-ratified-SQLite** for the structural half; the obligation/waiver evaluation is Authority and STAYS SQLite. Conditional (arch obligation exists). → **DR-5 (low-priority).** |
| `BOUNDARY_LINKS_SUMMARY` (F) | NO (`boundary_interaction_links`) | **SQLite-first** ("No LiveGraph producer for `boundary_interaction_links`") | none | **No-LG-producer**, but **Not-a-decommission-target** (Tier-B/L2, not `nodes`/`edges`) → STAYS SQLite, does not block. |
| `MODULE_SUMMARY` discovered-module count (H) | NO (`module_candidates`) | **SQLite-first** (D-ORIENT-2) | none | **No-LG-producer**, but **Not-a-decommission-target** (Tier-B) → STAYS SQLite, does not block. |
| `GATE_*` obligation/waiver eval (I) | NO (`declarations`/`measurements`/`inferences`) | **Authority** | none (by construction) | **Not-a-decommission-target** → STAYS SQLite, does not block. |
| Authority declarations — boundary (E) | NO (`declarations`) | **Authority** | none (by construction) | **Not-a-decommission-target** → STAYS SQLite, does not block. |
| `SNAPSHOT_INFO` + `get_stale_files` (A/B/C) | NO (`snapshots`/`repos`/`file_versions`) | **SQLite-first** (operational identity) | none | **Not-a-decommission-target** → STAYS SQLite, does not block. (`get_stale_files` is ALSO read by the SHIPPED overlay, orient_coherence.rs:59, to set leaf freshness — preserved.) |
| documentation (L) | NO (**FS**) | **FS** | none | **Not-a-decommission-target** (filesystem, not SQLite at all) → STAYS FS. |
| dead_code (G) | NO (withdrawn) | — | — | No read on the served path (returns `empty()`). |

### 3a. Distilling the verdict (INFERRED)

```text
The `nodes`/`edges` reads in orient's base use case fall into THREE classes:

CLASS 1 — LG-servable NOW (an existing leaf cert covers it):
  · cycles (edges)           — IMPORT_CYCLES leaf, cycles no-loss cert
  · callers/callees (edges)  — CALLERS/CALLEES_SUMMARY leaves, per-symbol no-loss compare
  (complexity is LG-servable now too, but reads `measurements`, not `nodes`/`edges`.)

CLASS 2 — LG-derivable but RATIFIED SQLite-first (re-sourcing crosses ORIENT-LIVEGRAPH-1):
  · MODULE_SUMMARY counts (nodes)         — DR-2  [UNCONDITIONAL on repo/path/file focus]
  · BOUNDARY_VIOLATIONS edges (edges)     — DR-3  [conditional: declarations exist]
  · gate arch_violations edges (edges)    — DR-5  [conditional: arch obligation exists]

CLASS 3 — NO LiveGraph producer today (a new producer/extraction is required):
  · trust-core (edges + unresolved_edges) — DR-1  [UNCONDITIONAL, all four focuses]  ← the decisive blocker
  · focus resolution (nodes)              — DR-4  [every FOCUSED orient; repo/None exempt]

Everything ELSE orient reads (Authority declarations, module_candidates, boundary_interaction_links,
snapshots/repos, measurements-for-complexity-fallback, docs-FS) is NOT a `nodes`/`edges` read, so it does NOT
block the `nodes`/`edges` retirement even though it keeps orient touching SQLite/FS on every call.

CONSEQUENCE: serving orient's four LG-first leaves on green removes ONLY the Class-1 reads. The Class-2 and
Class-3 reads — including TWO UNCONDITIONAL ones (trust-core in every focus; MODULE_SUMMARY counts in
repo/path/file) — survive. Therefore orient does NOT become `nodes`/`edges`-free on green by the leaf-serving
alone. Full elimination requires DR-1..DR-4 resolved (DR-5 is conditional/low-priority). This is the headline
finding, now proven per-source.
```

---

## 4. The cert / fingerprint gate for orient's full answer (INFERRED, mirroring the drilldowns)

The packet asks for "the cert/fingerprint gate for orient's full answer (no-loss compare, mirroring the
drilldown cert-fastpaths) that decides serve-vs-fallback." The honest design:

### 4a. Why orient cannot reuse a single drilldown cert verbatim

OBSERVED [stats-livegraph-1 §Target, D1-D3]: a drilldown cert (imports/cycles/stats) gates a SINGLE structural
answer — one no-loss compare of one LiveGraph payload against the SQLite payload, GREEN ⇒ skip the one SQLite
read. orient's answer is a UNION of independently-sourced sections (four LG-first leaves + Class-2 structural +
Class-3 trust/focus + Authority + Tier-B + FS). There is no single payload to compare.

### 4b. The composite ORIENT no-loss cert (the design, conditional on §8)

```text
ORIENT cert {verdict: GREEN|RED, fingerprint}  — on RepoState, in-memory RwLock<Option<OrientNoLossCert>>,
  S1 (rebuilt on restart), mirroring import_cert/cycles_cert/stats_cert [OBSERVED: state.rs pattern referenced
  by stats-livegraph-1 D2]. Keyed by the SHARED SQLite-free fingerprint (import_cert_fingerprint: partition
  epoch/hash set ⊕ classifier/policy version) — NO new invalidation key. Lazily built once per fingerprint;
  the SQLite read survives ONLY (i) to BUILD the cert and (ii) on fallback (the drilldown invariant).

GREEN  iff  ALL of the following contributing no-loss verdicts are GREEN at the current fingerprint:
  (1) cycles no-loss cert        — reuse build_and_store_cycles_cert         [exists]
  (2) per-symbol callers/callees no-loss compare (symbol focus only)         [exists]
  (3) complexity no-loss cert    — reuse the complexity cert                 [exists]
  (4) MODULE_SUMMARY structural-count no-loss cert  — reuse the stats module_stats compare  [needs DR-2]
  (5) trust-core no-loss cert    — NEW: LiveGraph-native trust summary == SQLite trust summary  [needs DR-1]
  (6) BOUNDARY_VIOLATIONS import-edge no-loss cert (only if declarations)    [needs DR-3]
  (7) gate arch_violations import-edge no-loss cert (only if arch obligation)[needs DR-5]
AND precondition: every contributing partition resident + Fresh + TS-primary (non-TS ⇒ precondition unmet).

It is an AND-fold: the WEAKEST contributor decides (the MEET discipline the coherence root already uses,
orient-livegraph-1 D-ORIENT-4). A RED or missing contributor ⇒ ORIENT cert RED ⇒ SQLite fallback.
```

CRITICAL HONESTY [INFERRED]: contributors (4)(5)(6)(7) do NOT exist today. (5) trust-core has **no LiveGraph
producer at all** (DR-1) — without it the AND-fold can NEVER be GREEN, because trust runs unconditionally in
every focus. So under the CURRENT architecture the composite ORIENT cert is RED by construction for every repo,
and orient always falls back. The cert design is therefore **specified but inert until DR-1 (minimum) +
DR-2/DR-3 land**. This is the same shape as stats-livegraph-1: the cert was designed, but its
symbol-classification contributor did not exist, so a prerequisite slice (IR-SYMBOL-ATTRIBUTES-1) had to land
first.

---

## 5. Serve-then-fallback control flow (the target; honest about §8 gating)

The flow REPLACES the current "always run base SQLite use case, then overlay labels" (dispatch.rs:2603→2631)
with the drilldown serve-then-fallback ladder. It is built INSIDE `build_orient_envelope`'s call site so the
shipped `CoherenceEnvelope` provenance/freshness labels are reused verbatim — no new output contract.

```text
handle_orient:
  1. PRECONDITION CHECK (SQLite-free): partitions resident + Fresh + TS-primary?
       NO  → FALLBACK: run repo_graph_agent::orient(&storage,…) (today's eager base read), wrap in the
             CoherenceEnvelope with every leaf provenance.source=sqlite, fallback_reason ∈ {UnsupportedLanguage,
             Partial, Stale} (the SHIPPED FallbackReason → CoherenceFallbackReason mapping, orient_coherence.rs:160).
       YES → step 2.
  2. ORIENT cert lookup at current fingerprint:
       missing/stale → LAZY BUILD (reads SQLite ONCE per fingerprint via the per-contributor compares), then re-read.
       RED  → FALLBACK (as step 1, fallback_reason = the failing contributor's reason: CycleDivergence /
              CallgraphDivergence / StatsDivergence / ComplexityDivergence / <trust/​boundary divergence>).
       GREEN → step 3.
  3. FASTPATH (GREEN, no eager `nodes`/`edges` read): assemble OrientResult from
       · LiveGraph: cycles (module_import_cycles), callers/callees, complexity (value_facts),
         MODULE_SUMMARY structural counts (module_stats)            [needs DR-2],
         trust-core summary (the NEW LiveGraph trust producer)      [needs DR-1],
         focus resolution (the NEW LiveGraph focus-resolution path) [needs DR-4, focused orient only],
         BOUNDARY_VIOLATIONS / gate arch edges via the imports surface [needs DR-3/DR-5, only if declarations/arch],
       · SQLite (NON-`nodes`/`edges`, retained — these never blocked the decommission):
         Authority declarations (boundary/gate obligation+waiver eval), module_candidates (discovered-module
         count), boundary_interaction_links (links freshness), snapshots/repos (snapshot identity),
         get_stale_files (leaf freshness, orient_coherence.rs:59),
       · FS: docs inventory (doc-facts).
     Wrap in the CoherenceEnvelope with the served leaves provenance.source=livegraph + the cert's
     freshness/completeness; the retained SQLite/FS sections labelled source=sqlite/authority/fs (UNCHANGED
     from the shipped overlay). The root MEET is computed exactly as today (D-ORIENT-4).
```

The fastpath (step 3) skips `find_module_cycles*`, `find_symbol_callers/callees`, `compute_*_summary`,
`find_imports_between_paths` (boundary/gate), the trust-core `edges` reads, AND the focus-resolution `nodes`
reads — i.e. it skips EVERY Class-1/2/3 `nodes`/`edges` read. It retains the non-graph SQLite + FS reads. THAT
is precisely "no eager `nodes`/`edges` read on green," and ONLY that.

---

## 6. What this does and does NOT achieve (honesty — per readiness-9 discipline)

```text
DOES (when DR-1..DR-4 are ratified + the trust-core producer prerequisite lands):
  + Removes orient's eager `nodes`/`edges` read on the GREEN repo-focus served path (the Class-1/2/3 reads),
    converting orient from "overlay on an eager SQLite read" to "serve-then-fallback" — the proven drilldown posture.
  + Reuses the shipped CoherenceEnvelope output contract verbatim (provenance/trust/freshness labels, MEET root):
    no new wire shape, no human-output break (the served values are no-loss-equal to the SQLite values by cert).
  + Keeps the labelled SQLite fallback for not-green / non-TS / non-resident / stale (honest degradation).

DOES NOT (the boundaries readiness-9 demands be stated):
  - Does NOT make orient SQLite-FREE. Authority declarations, module_candidates, boundary_interaction_links,
    snapshots/repos, the complexity `measurements` fallback, and docs (FS) remain read on the served path.
    NONE are `nodes`/`edges`, so they do not block the `nodes`/`edges` retirement — but "orient no longer
    touches SQLite" is FALSE and must not be claimed.
  - Does NOT remove the non-TS fallback. LiveGraph is TS-only; every non-TS repo/file falls back to the full
    SQLite base read. `nodes`/`edges` stay load-bearing for C/C++/Rust/Java (deletion gate 2, the structural ceiling).
  - Does NOT remove the cert-BUILD SQLite read (once per fingerprint) — the drilldown invariant survives.
  - Does NOT cover FOCUSED orient until DR-4 (focus resolution). A first impl scoped to REPO focus (`focus=None`)
    leaves symbol/path/file orient reading `nodes` for focus resolution.
  - Does NOT, by itself, retire `nodes`/`edges`. The other defaults' fallbacks, the imports/cycles/stats cert
    builds, and the 31 non-graph tables remain (readiness-9 gates 2–5).
  - Is INERT until at least DR-1 lands: the composite cert is RED by construction while trust-core has no
    LiveGraph producer (§4b). So this slice's IMPL cannot ship a working green fastpath without a prerequisite.
```

---

## 7. Validation plan (for the eventual IMPL; mirrors the drilldown proofs)

NOT RUN here (spec-first; no code). The IMPL slice (ORIENT-SQLITE-FREE-IMPL-1) must produce, mirroring
stats-livegraph-1 §validation:

```text
PARITY (green compare):  rmap orient --engine compare on a TS pilot where the ORIENT cert is GREEN →
  is_exact=true: the LiveGraph-assembled OrientResult is BYTE-equal (post-canonicalization) to the SQLite base
  result, leaf-by-leaf (cycles, callers/callees, complexity, module_summary counts, trust summary). A single
  divergent field ⇒ RED ⇒ fallback (no silent mismatch). [EXECUTED proof required.]
NO-EAGER-READ PROOF:     a unit/integration test that, on a GREEN cert + precondition met, asserts the served
  path performs ZERO `nodes`/`edges` reads — e.g. a storage spy/panicking-closure on find_module_cycles /
  find_symbol_callers / find_symbol_callees / compute_*_summary / find_imports_between_paths / the trust-core
  edge reads / resolve_*_focus, mirroring the callers/callees lazy proof (readiness-9 gate 5). This is the
  load-bearing test: it is the operational definition of "eager read eliminated."
FALLBACK CORRECTNESS:    non-TS repo → fallback (UnsupportedLanguage); non-resident partition → fallback
  (Partial); stale index → fallback (Stale); cert RED (any contributor diverges) → fallback (the named
  divergence reason). Each labelled in the CoherenceEnvelope provenance. Default `--engine auto` unchanged for
  the human renderer (byte-compatible).
CERT-BUILD-ONCE:         the cert is built once per fingerprint (SQLite read on build only), reused across calls,
  invalidated on fingerprint change/restart (mirror import_cert/cycles_cert/stats_cert).
SCOPE GUARD:             check/explain/trust are NOT touched (later Option-B slices); only orient's handler +
  a new orient serve module change.
```

EXECUTED this slice:
- `git log --oneline -15` → HEAD `56160bb`; coherence chain `6ed17b8..dc55114`; ORIENT-LIVEGRAPH-IMPL `2fd4478`.
  [OBSERVED — confirms the baseline this spec builds on.]
- `grep`/`Read` over the agent use case, aggregators, storage adapter, trust_impl, dispatch.rs, livegraph
  lib.rs — every §2/§3 OBSERVED claim re-verifiable at the cited file:line.
NOT RUN: cargo build/test, dev-install, live `rmap orient` capture — spec-first; no source path touched; daemon
start runs index/refresh (state-mutating, out of scope).

---

## 8. Forced decisions — `DECISION_REQUIRED` (architecture-boundary + new-producer blocks)

Per CLAUDE.md Decision Autonomy: a re-sourcing that contradicts a ratified decision (ORIENT-LIVEGRAPH-1's
SQLite-first postures), a new dependency edge (a LiveGraph trust producer), or a new data shape crossing a
boundary (LiveGraph focus resolution) is a **stop-and-ask, presented as an exhaustive matrix**. The packet's
STOP_CONDITION is explicit: "If a base source cannot be LiveGraph-served without a new producer, STOP and emit
DECISION_REQUIRED." Five sources qualify; the meta-sequencing is DR-0.

```text
DECISION_REQUIRED:
- ID: DR-0-SEQUENCING
  QUESTION: orient is a COMPOSITE; its eager `nodes`/`edges` read cannot be eliminated by one cert-flip.
            What is the build sequence for ORIENT-SQLITE-FREE, given DR-1 (trust-core) is an UNCONDITIONAL,
            all-focus, no-producer-today blocker?
  OPTIONS:
  - S1 PREREQUISITE-FIRST (recommended): ratify DR-1 + DR-2, land a trust-core LiveGraph producer slice
    (TRUST-SUMMARY-LIVEGRAPH-1) + re-source MODULE_SUMMARY counts (DR-2) FIRST, THEN ship
    ORIENT-SQLITE-FREE-IMPL-1 scoped to REPO focus (avoids DR-4). Mirrors stats→IR-SYMBOL-ATTRIBUTES-1.
    Consequence: orient repo-focus becomes `nodes`/`edges`-free on green; focused orient deferred to DR-4.
  - S2 ORIENT-LEADS-ANYWAY: build the orient fastpath now with the cert RED by construction (trust-core never
    GREEN) → orient ALWAYS falls back → ZERO decommission win, dead code. Rejected (no value; breaks the cert).
  - S3 REASSIGN-LEAD: let `explain` lead Option B instead of orient — explain already SERVES 5 leaf VALUES
    from the LiveGraph (readiness-9), is more purely structural, and its composite has no unconditional
    trust-core-in-the-answer dependency of the same weight. orient follows once the trust producer exists.
    Consequence: Option B still advances; orient is not first. Contradicts the packet's "orient first" framing —
    a governance call.
  RECOMMENDED: S1. It is the only sequence that makes orient's claim ("no eager `nodes`/`edges` read on green")
    TRUE for any focus, and it matches the proven stats precedent (prerequisite producer, then fastpath).
  BLOCKING_REASON: building the orient fastpath before the trust-core producer ships produces an
    always-RED cert (dead fallback-only code, §4b). The sequence must be chosen before any IMPL.

- ID: DR-1-TRUST-CORE-PRODUCER
  QUESTION: orient's trust aggregator reads `edges` + `unresolved_edges` UNCONDITIONALLY in all four focuses
            (get_trust_summary → assemble_trust_report; trust_impl.rs:116/149/265/344/353) and is ratified
            SQLite-first (orient-livegraph-1 §2). TRUST-LIVEGRAPH-1 added a posture BESIDE the v1 report, it did
            NOT replace the edge-derived call-resolution/reliability core. How is the trust SUMMARY orient
            consumes (resolved/unresolved counts, reliability axes, stale) served without reading `edges`?
  OPTIONS:
  - A NEW PRODUCER (recommended): a TRUST-SUMMARY-LIVEGRAPH producer that computes call-resolution +
    reliability axes from the LiveGraph (resolved/unresolved adjacency already in the IR/xref) instead of
    `edges`/`unresolved_edges`, with a no-loss cert vs the SQLite trust summary. New producer + new dependency
    edge. The only path to a GREEN composite cert. Sized as its own slice.
  - B PERMANENT-SQLITE: accept trust-core as a permanent SQLite `edges` read on orient's served path.
    Consequence: orient NEVER becomes `nodes`/`edges`-free (gate 1 stays FAIL for orient); the whole slice's
    value evaporates. Rejected as a path to decommission; acceptable only if the operator deprioritizes orient.
  - C DROP-TRUST-ON-GREEN: omit TRUST_* signals + the trust-derived confidence when serving from LiveGraph.
    Consequence: a DIFFERENT (degraded) answer on green vs fallback; violates overlay-never-erases (D-ORIENT-5)
    and the confidence contract (D-ORIENT-4). Rejected (false completeness / certainty mislabel).
  RECOMMENDED: A. It is the only option that removes the read AND preserves the answer.
  BLOCKING_REASON: trust runs in every focus unconditionally; until A lands, the composite ORIENT cert is RED
    by construction (§4b) and no orient focus can be `nodes`/`edges`-free on green. This is the decisive block.

- ID: DR-2-MODULE-SUMMARY-COUNTS
  QUESTION: MODULE_SUMMARY's file/symbol/language counts read `nodes` (compute_repo_summary, agent_impl.rs:241)
            and are ratified SQLite-first (D-ORIENT-2 module_candidates anchor + RISK-E identity divergence),
            even though `module_stats` (lib.rs:1376) proves the count is LG-derivable. Re-source to LiveGraph?
  OPTIONS:
  - A RE-SOURCE-STRUCTURAL-COUNTS (recommended): serve file/symbol/language counts from `module_stats`
    (cert-gated, reusing the stats no-loss compare); KEEP discovered_module_count anchored to module_candidates
    (Tier-B SQLite — not a `nodes`/`edges` read, so it stays harmlessly). Removes the `nodes` count read on green.
    Crosses the ratified SQLite-first posture → must be re-ratified by the operator.
  - B KEEP-SQLITE-FIRST: leave MODULE_SUMMARY counts on `nodes`. Consequence: an UNCONDITIONAL `nodes` read
    survives on repo/path/file focus → orient never `nodes`-free even with DR-1 solved. Rejected for the goal.
  RECOMMENDED: A. The stats slice already proved the LG count is no-loss; the anchor stays SQLite (Tier-B).
  BLOCKING_REASON: the count read is UNCONDITIONAL on repo/path/file focus; B defeats the slice. Re-ratifying a
    ratified decision is an architecture-boundary call the operator must make.

- ID: DR-3-BOUNDARY-VIOLATION-EDGES
  QUESTION: BOUNDARY_VIOLATIONS reads `edges` via find_imports_between_paths (boundary.rs:70), ratified
            SQLite-first. Route the import-edge check through the migrated LiveGraph imports surface?
  OPTIONS:
  - A ROUTE-VIA-IMPORTS-LG (recommended for full elimination): answer "imports between path A and B" from the
    LiveGraph imports surface, cert-gated; re-ratify off SQLite-first. Conditional (only when ≥1 boundary
    declaration). The Authority declaration STAYS SQLite (it is not `nodes`/`edges`).
  - B KEEP-SQLITE-FIRST: leave it on `edges`. Consequence: on repos WITH boundary declarations an `edges` read
    survives on green; repos without declarations are unaffected. Acceptable as a short-term scope cut.
  RECOMMENDED: A for completeness; B is a defensible interim if the first IMPL targets declaration-free repos.
  BLOCKING_REASON: a `nodes`/`edges` read survives on green for declaration-bearing repos under B; choosing A
    re-ratifies a ratified posture (architecture-boundary).

- ID: DR-4-FOCUS-RESOLUTION
  QUESTION: focused orient resolves the focus string to a node via resolve_path_focus / resolve_stable_key_focus
            / resolve_symbol_name / get_symbol_context — each reads `nodes` (agent_impl.rs:366/437/800/834).
            No LiveGraph focus-resolution path exists. How is focused orient made `nodes`-free on green?
  OPTIONS:
  - A LG-FOCUS-RESOLUTION (recommended for focused coverage): a LiveGraph focus→IR-symbol/file/module resolver
    (new surface) + cert. New producer/data-shape across a boundary.
  - B SCOPE-TO-REPO-FOCUS-FIRST: ship ORIENT-SQLITE-FREE-IMPL-1 for `focus=None` (repo) only, which never calls
    focus resolution (mod.rs:65); defer focused orient to a follow-up. Consequence: symbol/path/file orient keep
    a `nodes` read on green until A lands.
  RECOMMENDED: B for the first IMPL (smallest honest increment), A as the follow-up.
  BLOCKING_REASON: focused orient cannot be `nodes`-free without A; the scope (repo-only vs all-focus) must be
    fixed before IMPL.

- ID: DR-5-GATE-ARCH-EDGES
  QUESTION: gate's arch_violations obligation method reads `edges` (conditional on an arch_violations obligation
            existing). The obligation/waiver EVALUATION is Authority (no LiveGraph home). Route only the
            structural edge check through the LiveGraph imports surface?
  OPTIONS:
  - A ROUTE-ARCH-EDGE-VIA-IMPORTS-LG: cert-gate the arch_violations edge check via the LiveGraph imports
    surface; keep obligation/waiver eval SQLite (Authority). Removes the conditional `edges` read on green.
  - B KEEP-SQLITE (recommended interim): leave it; the read fires only when an arch_violations obligation
    exists (rare in the corpus). Lowest priority; revisit after DR-1..DR-4.
  RECOMMENDED: B short-term (conditional/rare), A for completeness.
  BLOCKING_REASON: low — a conditional `edges` read on green for arch-obligation repos only. Not on the
    critical path, but must be acknowledged for a TRUE `nodes`/`edges`-free claim.
```

---

## 9. Scope boundary

```text
IN SCOPE (this spec): orient ONLY. The source enumeration (§2), the serve-vs-fallback classification (§3), the
  composite cert design (§4), the serve-then-fallback flow (§5), the honesty section (§6), the validation plan
  (§7), and the five+one architecture-boundary decisions (§8).
OUT OF SCOPE: any code, table deletion, migration, or default flip (spec-first). check / explain / trust (later
  Option-B slices). The orient IMPLEMENTATION (ORIENT-SQLITE-FREE-IMPL-1, gated on §8). The trust-core LiveGraph
  producer (DR-1 → its own prerequisite slice). Non-TS LiveGraph coverage (Option A; readiness-9). The 31
  non-graph tables and the other defaults' fallbacks (the broader decommission). ROADMAP.md / CURRENT_SLICE.md
  edits (read-only here).
```

---

## 10. References

- `docs/slices/sqlite-raw-decommission-readiness-9.md` — the Option-B driver; the eager-read finding; the gate-RED recompute.
- `docs/slices/orient-livegraph-1.md` §2 — the ratified per-signal source map (LG-first / SQLite-first / Authority / FS postures this slice builds on and, for DR-2/DR-3, proposes re-ratifying).
- `docs/slices/stats-livegraph-1.md` — the cert-fastpath precedent + its D0 architecture-boundary split (spec → prerequisite IR-SYMBOL-ATTRIBUTES-1 → fastpath impl); the model this spec follows.
- `docs/slices/coherence-layer-1.md` — the ratified `CoherenceEnvelope<T>` contract (the output shape reused verbatim).
- `rust/crates/agent/src/orient/{mod,repo,symbol,path,file}.rs` + `aggregators/*.rs` — the base use case (§2 enumeration).
- `rust/crates/storage/src/agent_impl.rs` + `trust_impl.rs` — the storage reads (§2/§3 `nodes`/`edges` classification).
- `rust/crates/daemon-runtime/src/dispatch.rs:2550-2656` + `orient_coherence.rs` — handle_orient (eager read :2603) + the shipped overlay this slice converts to serve-then-fallback.
- `rust/crates/repo-graph-livegraph/src/lib.rs` — the LiveGraph surfaces (callers/callees/value_facts/module_import_cycles/module_stats) the fastpath would consume.
