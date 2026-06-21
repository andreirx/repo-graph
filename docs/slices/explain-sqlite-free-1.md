# EXPLAIN-SQLITE-FREE-1: eliminate `rmap explain`'s eager SQLite `nodes`/`edges` base read (Stage D)

Slice ID: EXPLAIN-SQLITE-FREE-1
Status: **SPEC-FIRST — specification only. NO implementation, NO code, NO deletion, NO migration, NO default
flip.** This document specifies how `rmap explain` would ELIMINATE its unconditional/eager SQLite base read,
serving from current-state LiveGraph when a cert is GREEN and reading SQLite ONLY on fallback — mirroring the
imports/cycles/stats cert-fastpath posture, built ON TOP of the shipped explain `CoherenceEnvelope`
(EXPLAIN-LIVEGRAPH-IMPL `82b6557`, which ALREADY genuinely serves 5 green leaf VALUES from the LiveGraph).
Track: Stage D / SQLite-raw decommission — Option B (eliminate the coherence commands' eager SQLite base
reads). LEAD command: explain (per ORIENT-SQLITE-FREE-1 DR-0 → S3; orient DEFERRED, producer-gated).
Baseline: SQLITE-RAW-DECOMMISSION-READINESS-9; EXPLAIN-LIVEGRAPH-1 (`cb8a311`) + IMPL (`82b6557`);
ORIENT-SQLITE-FREE-1 (`e10a455`, the precedent + producer-gap taxonomy this mirrors).

> **HEADLINE FINDING (load-bearing, read before the design): explain's eager `nodes`/`edges` read CANNOT be
> eliminated on green by serving its five LiveGraph-served leaves alone. explain is PRODUCER-GATED — it hits
> the SAME orient-style producer gap, by the SAME source.** First-hand source reads (below) establish that
> explain's base use case (`run_explain`) reads `nodes`/`edges` through paths the five served leaves do NOT
> cover, and TWO of those are UNCONDITIONAL with NO LiveGraph producer today:
> 1. **trust-core** — `get_trust_summary` → `assemble_trust_report` reads `edges` + `unresolved_edges`
>    (trust_impl.rs:116/149/214/265/344/353), called UNCONDITIONALLY in ALL THREE pipelines (explain_symbol
>    mod.rs:343, explain_file mod.rs:521, explain_path mod.rs:644). Its values feed BOTH the always-emitted
>    `EXPLAIN_TRUST` signal (build_trust_signal, mod.rs:422/544/710) AND the envelope `confidence`
>    (`derive_repo_confidence`, mod.rs:442/564/730). It is NOT one of the five served leaves. This is the
>    **SAME** blocker as orient DR-1 — the trust SUMMARY has **no LiveGraph producer at all**.
> 2. **focus resolution** — every explain resolves its target via `resolve_path_focus` (mod.rs:86, `nodes`),
>    then conditionally `resolve_stable_key_focus` / `resolve_symbol_name` / `get_symbol_context` (mod.rs:115/
>    159/117·165, all `nodes`). explain has **no repo-focus exemption** (orient's `focus=None` path skips
>    resolution; explain ALWAYS requires a `target`, dispatch.rs:2738) → the `nodes` read is UNCONDITIONAL.
>    There is no LiveGraph focus-resolution surface. This is orient DR-4, but **unconditional** for explain.
>
> The five LG-first leaves (identity anchor, callers, callees, imports, cycles) ARE genuinely served from the
> LiveGraph today (`82b6557` — a real advance over orient's label-only overlay). But they are a **strict
> SUBSET** of explain's answer: trust (unconditional), focus resolution (unconditional), the file/path
> listings + structural counts (ratified SQLite-first), and the boundary/gate structural edges (conditional)
> all survive on green. Eliminating explain's eager read is therefore an architecture-boundary + new-producer
> program — NOT a single cert-flip. The gaps are surfaced as `DECISION_REQUIRED` (§8). This mirrors exactly
> the orient finding (ORIENT-SQLITE-FREE-1) and STATS-LIVEGRAPH-1's D0 architecture-boundary split. The
> achievable design (§4–§5) is specified CONDITIONAL on those decisions; §6 states honestly what it does and
> does NOT achieve.

> **VERDICT: explain is PRODUCER-GATED (not PRODUCER-LIGHT).** It needs a NEW producer before any green
> fastpath can be `nodes`/`edges`-free — minimally **DR-E1** (a `TRUST-SUMMARY-LIVEGRAPH-1` producer, SHARED
> with orient DR-1 and the `trust` command). This **REFUTES** the DR-0 → S3 working hypothesis that explain,
> because it already serves 5 leaf values, is "the likeliest first REAL eager-read elimination WITHOUT a
> producer program." First-hand evidence shows explain carries the SAME unconditional trust-core dependency
> orient does (§2/§3), PLUS an unconditional focus-resolution gap orient only had conditionally. The
> serving-advance (`82b6557`) is necessary but **not sufficient**. The governance consequence: **Option B is
> uniformly producer-gated on the trust-core producer** — that producer is the shared prerequisite for the
> whole coherence cluster (orient, explain, and trust's hybrid). Decisive verdict in §11; the producer-gap
> map is §8 (DR-0 + DR-E1..DR-E5, all OPEN).

> **DECISION RESOLUTION — DR-0-SEQUENCING (ratified by operator, 2026-06-13): S1 (shared-prerequisite-first).**
> Build the single shared `TRUST-SUMMARY-LIVEGRAPH-1` producer (DR-E1 = orient DR-1) FIRST — it is the
> unavoidable prerequisite for the entire coherence cluster (orient + explain + trust); building it once unblocks
> all three. **Option B is hereby re-scoped as a producer program first, per-command fastpaths second.** The
> per-command-lead hypothesis (the earlier DR-0 → S3, "explain leads because producer-light") is REFUTED and
> CLOSED. S2 (explain-leads-anyway) REJECTED (dead, always-RED code). S3 (pivot to Option A) NOT taken now — the
> trust producer is foundational under Option A too, so it does not foreclose it. `EXPLAIN-SQLITE-FREE-IMPL-1` +
> `ORIENT-SQLITE-FREE-IMPL-1` stay gated on `TRUST-SUMMARY-LIVEGRAPH-1` (+ their focus-resolution producers,
> DR-E2/DR-4). This spec stands as the authoritative explain producer-gap map; DR-E1..DR-E5 (§8) remain OPEN
> until the producer and the per-command impls are taken up.

> **PRODUCER UPDATE (2026-06-13): DR-E1 (the shared trust-core producer) is REFUTED.** `TRUST-SUMMARY-LIVEGRAPH-1`
> + `SCIP-UNRESOLVED-CALL-PROBE-1` proved SCIP cannot source a parity unresolved-call count (NO-GO); operator
> ratified **Option A** — the trust contributor stays homegrown-`unresolved_edges` SQLite-LABELLED. Consequence
> for explain: its trust leaf can NEVER be `edges`-free on green (gate-1 RED by design for that leaf). A future
> `EXPLAIN-SQLITE-FREE-IMPL-1`, if pursued, can only serve the LG-DERIVABLE leaves (the 5 already served + any
> other non-trust derivable fields) and must keep the trust leaf + focus-resolution (DR-E2) SQLite-sourced.

---

## 0. Spec-first note (read first)

This is a SPECIFICATION. It produces exactly one deliverable: this file. NO source path is touched; no cert is
built; no default is flipped; `nodes`/`edges` are not read by this slice (only the SHIPPED handler reads them,
which this doc audits). The eventual implementation (EXPLAIN-SQLITE-FREE-IMPL-1, a LATER slice) is gated on the
§8 decisions being ratified first — and, per §8, on at least one NEW prerequisite producer slice (the
trust-core LiveGraph projection, DR-E1) landing before explain can serve a `nodes`/`edges`-free answer on green.

Per the repo split rule (CLAUDE.md: spec before impl; ratify architecture-boundary decisions before building),
this slice's DEFINITION OF DONE is the specification + the surfaced decisions + the explicit
PRODUCER-LIGHT-vs-PRODUCER-GATED verdict — NOT a working fastpath.

### Evidence labels (repo Evidence Law)

- **OBSERVED** = inspected first-hand THIS slice (a file read I performed this turn, cited file:line).
- **INFERRED** = my classification/judgment over OBSERVED facts (the serve-vs-fallback verdicts, the cert
  design, the verdict, the next-step recommendation).
- **EXECUTED** = a command I ran this turn with output observed.
- **NOT RUN** = skipped, with reason.

### Evidence basis (this audit)

```text
git: HEAD=`e10a455` (ORIENT-SQLITE-FREE-1 spec; orient deferred → DR-0 S3 reassigns the Option-B lead to
explain) [OBSERVED: git log --oneline -15]. The coherence chain `6ed17b8..dc55114` sits below it;
EXPLAIN-LIVEGRAPH-IMPL = `82b6557`; readiness-9 + PRIORITY-DOCS-RECONCILE-3 = `56160bb`. Working tree CLEAN at
audit time [OBSERVED: git status --short → empty]. The explain LiveGraph-serving modules
(explain_lg_serve.rs / explain_lg_identity.rs / explain_coherence.rs) are COMMITTED in `82b6557`, NOT
uncommitted working-tree work [OBSERVED: git show --stat 82b6557].

Daemon NOT started; no index/refresh/dev-install run (state-mutating + out of scope for a spec). All claims
about explain's read surface are grounded in FIRST-HAND SOURCE reads of the shipped agent use case, the
storage adapter, the trust core, the daemon handler, the explain coherence/serve modules, and the LiveGraph
crate — the stronger evidence basis for a claim about code structure than a live capture. Every OBSERVED claim
carries file:line.

Files read first-hand THIS slice:
- rust/crates/agent/src/explain/mod.rs (run_explain:55; explain_symbol:263; explain_file:470; explain_path:592;
  build_trust_signal:777; build_gate_signal:796 — the full base use case)
- rust/crates/daemon-runtime/src/dispatch.rs:2730-2808 (handle_explain: eager base read :2766, envelope build
  :2795, target required :2738)
- rust/crates/daemon-runtime/src/explain_coherence.rs (build_explain_envelope:65 — serves the 5 leaves AFTER
  the base read; explain_imports_outcome:246)
- rust/crates/daemon-runtime/src/explain_lg_identity.rs (serve_identity:45 — anchor name/subtype only, takes
  the SQLite-resolved key)
- rust/crates/storage/src/agent_impl.rs (find_imports_between_paths:197, get_trust_summary:276,
  resolve_path_focus:366, resolve_stable_key_focus:437, compute_path_summary:587, compute_file_summary:647,
  find_cycles_involving_path:734, resolve_symbol_name:800, get_symbol_context:834, find_symbol_callers:883,
  find_symbol_callees:935, find_cycles_involving_module:987, list_symbols_in_file:1037, list_files_in_path:1070,
  find_file_imports:1113)
- rust/crates/storage/src/trust_impl.rs (assemble_trust_report import :36; edge reads :116/:265/:344/:353;
  unresolved_edges :149/:214)
- rust/crates/repo-graph-livegraph/src/lib.rs (callers:469, callees:586, value_facts:688, node_display:1051,
  module_import_cycles:1317, module_stats:1376, live_partitions:1571, live_import_view:1675,
  file_partition_status:1759 — the surface inventory; NO trust-summary / focus-resolver / per-file
  symbol-lister)
- docs/slices/explain-livegraph-1.md §2 (the ratified per-signal source map; EXPLAIN_TRUST = SQLite trust-core
  in all 3 pipelines) + §1a/§1c
- docs/slices/orient-sqlite-free-1.md (the precedent: the CLASS 1/2/3 taxonomy, the composite-cert design, the
  trust-core DR-1, DR-0 → S3)
- docs/slices/sqlite-raw-decommission-readiness-9.md (the Option-B driver; "eager read not skipped"; "real
  build per command, not a cert-flip")
- docs/slices/stats-livegraph-1.md (the cert-fastpath precedent + the D0 architecture-boundary split model)
```

NOTE ON LINE NUMBERS: explain-livegraph-1.md (`cb8a311`, pre-impl) cites the PRE-`82b6557` mod.rs offsets
(get_trust_summary :333/:511/:634; build_trust_signal :412/:534/:700). `82b6557` added the `pub mod coherent;`
re-export block (+10 lines, dispatch diff `mod.rs | 10 +`), shifting them. This spec cites the CURRENT
post-impl offsets I read first-hand THIS slice (get_trust_summary :343/:521/:644; build_trust_signal
:422/:544/:710). Same code, shifted; both are reconcilable at the cited symbol.

---

## 1. Why now (priority path)

OBSERVED [readiness-9 §Recommendation lines 276-291]: the ratified Stage-D order is
`COHERENCE-LAYER-1 ✓ → SQLITE-RAW-DECOMMISSION-READINESS-9 ✓ (gate RED) → SQLITE-RAW-DECOMMISSION-1 (terminal;
GATED)`. readiness-9 recomputed the deletion gate as RED — all five gates FAIL — and recommended **Option B
(eliminate the coherence eager reads)** as the incremental next build. ORIENT-SQLITE-FREE-1 (the first Option-B
spec) found orient PRODUCER-GATED on trust-core and, in **DR-0 → S3 (operator-ratified 2026-06-13)**, REASSIGNED
the Option-B LEAD to `explain` on the hypothesis that explain — already serving 5 leaf VALUES from the LiveGraph
— is the likeliest first REAL eager-read elimination WITHOUT a producer program. This slice is the SPEC that
tests that hypothesis rigorously. Its result (§11): the hypothesis is **REFUTED**; explain is producer-gated by
the same trust-core source.

OBSERVED [readiness-9 lines 110-111, 124-125, 159-167; dispatch.rs:2766/2795]: the coherence layer
(EXPLAIN-LIVEGRAPH-IMPL) did NOT remove explain's eager SQLite read. `handle_explain` runs the base use case
`repo_graph_agent::run_explain(&repo_state.storage, …)` UNCONDITIONALLY at dispatch.rs:2766, THEN
`explain_coherence::build_explain_envelope` (:2795) SWAPS the five green leaf VALUES (served from the LiveGraph)
into the already-SQLite-built `OrientResult` and labels per-leaf provenance. So explain's served path reads
`nodes`/`edges` on EVERY call; the five LG-first leaves are genuinely LiveGraph-sourced VALUES, but they are
swapped in AFTER the full base read already ran. readiness-9 line 110: "In ALL four the base SQLite use case
runs UNCONDITIONALLY first — the eager read is not skipped." This slice targets that eager read.

VISION alignment: `explain` is the agent's deep-dive command (the structural detail pipeline). Removing its
eager `nodes`/`edges` read is a precondition for retiring the `nodes`/`edges` substrate (the Stage-D terminal
slice) AND for the in-memory-graph end state (VISION "Operational Architecture": current repo state in memory is
primary truth; SQLite is the transition mechanism).

---

## 2. What `rmap explain` reads today — the eager base read (OBSERVED, first-hand)

`handle_explain` (dispatch.rs:2730) requires a `target` param (:2738 — explain has NO repo/`None` mode), then
calls `repo_graph_agent::run_explain` (dispatch.rs:2766), which:

### 2a. Pre-dispatch reads — common to EVERY explain call (OBSERVED, mod.rs:55-214)

| # | Read | Storage method | Table | `nodes`/`edges`? | file:line |
|---|------|----------------|-------|------------------|-----------|
| P1 | repo identity | `get_repo` | `repos` | NO | mod.rs:69 |
| P2 | snapshot | `get_latest_snapshot` | `snapshots` | NO | mod.rs:77 |
| P3 | **focus resolution (path-area)** | `resolve_path_focus` | `nodes` | **YES (nodes)** | mod.rs:86 → agent_impl.rs:366 (`FROM nodes` :378/:393/:406/:421) |
| P4 | focus resolution (stable-key) | `resolve_stable_key_focus` | `nodes` (+`edges`) | **YES (nodes)** | mod.rs:115 → agent_impl.rs:437 (`FROM nodes` :444/:489/:545; `FROM edges` :495/:551) |
| P5 | focus resolution (symbol-name) | `resolve_symbol_name` | `nodes` | **YES (nodes)** | mod.rs:159 → agent_impl.rs:800 (`FROM nodes` :809) |
| P6 | symbol context | `get_symbol_context` | `nodes` (+`edges` OWNS) | **YES (nodes)** | mod.rs:117/165 → agent_impl.rs:834 (`FROM nodes` :845; `JOIN edges` :850) |

P3 (`resolve_path_focus`) runs on EVERY explain (mod.rs:86, before any branch). P4–P6 run on the
stable-key / symbol-name resolution branches. So focus resolution is an **UNCONDITIONAL `nodes` read** on every
explain call, including the ambiguous/no-match paths (mod.rs:129/161/180-210/232, which emit no signals but have
already read `nodes`). This is the structural difference from orient, whose `focus=None` repo pipeline skips
resolution entirely.

### 2b. The three section pipelines (OBSERVED) — the five served leaves vs the rest

`run_explain` routes to exactly one of `explain_symbol` / `explain_file` / `explain_path`. Each emits a
focus-specific subset of the 11 signal codes. The table marks each read's table, `nodes`/`edges` dependence,
and whether EXPLAIN-LIVEGRAPH-IMPL (`82b6557`) genuinely SERVES that signal's VALUE from the LiveGraph today
(the "LG-served leaf" column — OBSERVED from explain_coherence.rs:107-179 + explain-livegraph-1.md §2).

| Pipeline / signal | Storage read | Table | `nodes`/`edges`? | LG-served leaf today? | file:line |
|-------------------|--------------|-------|------------------|-----------------------|-----------|
| **explain_symbol** (mod.rs:263) | | | | | |
| EXPLAIN_IDENTITY (anchor) | `get_symbol_context` (P6, pre-fetched) | `nodes` | YES (nodes) | **YES (anchor name/subtype via `node_display`)** — multi-source `{livegraph, sqlite}` | mod.rs:279; serve_identity explain_lg_identity.rs:45 (`node_display` :77) |
| EXPLAIN_CALLERS | `find_symbol_callers` | `edges` (CALLS) | **YES (edges)** | **YES (`callers` lib.rs:469)** | mod.rs:296 → agent_impl.rs:883 (`FROM edges` :896) |
| EXPLAIN_CALLEES | `find_symbol_callees` | `edges` (CALLS) | **YES (edges)** | **YES (`callees` lib.rs:586)** | mod.rs:320 → agent_impl.rs:935 (`FROM edges` :948) |
| EXPLAIN_CYCLES (if module) | `find_cycles_involving_module` | `edges` (IMPORTS) | **YES (edges)** | **YES (`module_import_cycles` lib.rs:1317)** | mod.rs:346 → agent_impl.rs:987 |
| EXPLAIN_BOUNDARY (if module + decls) | `get_active_boundary_declarations` + `find_imports_between_paths` | `declarations` (Auth) + `edges` | **YES (edges)** — only if ≥1 decl | NO (SQLite-first per contract) | mod.rs:369/378 → agent_impl.rs:197 |
| EXPLAIN_GATE (if module) | `get_active_requirements` + `assemble_from_requirements` | `declarations` (Auth); `edges` (arch); `measurements`/`inferences` | **YES (edges)** — only if arch obligation | NO (Authority) | mod.rs:409 (build_gate_signal:796) |
| **EXPLAIN_TRUST** | `get_trust_summary` | **`edges` + `unresolved_edges`** | **YES (edges)** | **NO** | mod.rs:343 (read) / :422 (emit) → trust_impl.rs:116/149/265/344 |
| confidence (envelope) | `derive_repo_confidence(&trust, stale)` | (uses the trust summary above) | YES (via trust) | NO | mod.rs:442; confidence.rs |
| stale (freshness) | `get_stale_files` | `file_versions` | NO | — | mod.rs:441 |
| **explain_file** (mod.rs:470) | | | | | |
| EXPLAIN_IDENTITY (file) | `compute_file_summary` | **`nodes`** (symbol_count) + files | **YES (nodes)** | NO (SQLite listings/summary, D-EXPLAIN-LISTINGS) | mod.rs:486 → agent_impl.rs:647 (`FROM nodes` :668) |
| EXPLAIN_IMPORTS | `find_file_imports` | `edges` (IMPORTS) | **YES (edges)** | **YES (`live_import_view` lib.rs:1675)** | mod.rs:502 → agent_impl.rs:1113 (`FROM edges` :1122) |
| EXPLAIN_SYMBOLS | `list_symbols_in_file` | **`nodes`** | **YES (nodes)** | NO (SQLite-first; counts LG-derivable, deferred) | mod.rs:522 → agent_impl.rs:1037 (`FROM nodes` :1046) |
| **EXPLAIN_TRUST** | `get_trust_summary` | **`edges` + `unresolved_edges`** | **YES (edges)** | **NO** | mod.rs:521 (read) / :544 (emit) |
| confidence | `derive_repo_confidence` | (trust summary) | YES (via trust) | NO | mod.rs:564 |
| stale | `get_stale_files` | `file_versions` | NO | — | mod.rs:563 |
| **explain_path** (mod.rs:592) | | | | | |
| EXPLAIN_IDENTITY (path) | `compute_path_summary` | **`nodes`** (symbol_count) + files | **YES (nodes)** | NO (SQLite summary, D-EXPLAIN-LISTINGS) | mod.rs:607 → agent_impl.rs:587 (`FROM nodes` :609) |
| EXPLAIN_FILES | `list_files_in_path` | **`nodes`** (per-file symbol_count subquery) + files | **YES (nodes)** | NO (SQLite-first listings) | mod.rs:623 → agent_impl.rs:1070 (`FROM nodes n2` :1081) |
| EXPLAIN_CYCLES | `find_cycles_involving_path` | `edges` (IMPORTS) | **YES (edges)** | **YES (`module_import_cycles` lib.rs:1317)** | mod.rs:645 → agent_impl.rs:734 |
| EXPLAIN_BOUNDARY (if decls) | `find_boundary_declarations_in_path` + `find_imports_between_paths` | `declarations` (Auth) + `edges` | **YES (edges)** — only if ≥1 decl | NO (SQLite-first) | mod.rs:665/670 |
| EXPLAIN_GATE | build_gate_signal | `declarations` (Auth); `edges` (arch); `measurements`/`inferences` | **YES (edges)** — only if arch obligation | NO (Authority) | mod.rs:698 |
| **EXPLAIN_TRUST** | `get_trust_summary` | **`edges` + `unresolved_edges`** | **YES (edges)** | **NO** | mod.rs:644 (read) / :710 (emit) |
| confidence | `derive_repo_confidence` | (trust summary) | YES (via trust) | NO | mod.rs:730 |
| stale | `get_stale_files` | `file_versions` | NO | — | mod.rs:729 |

EXPLAIN_MEASUREMENTS is emitted by no pipeline today (`measurement_items` is always empty — the Rust indexer
produces no measurements; mod.rs:428-435/550-557/716-723). It is not a read.

### 2c. The `nodes`/`edges` reads, by focus (the decommission target) — INFERRED from §2a/§2b

```text
EVERY focus:   P3 focus-resolution (nodes, UNCONDITIONAL) · EXPLAIN_TRUST (edges+unresolved_edges, UNCONDITIONAL)
SYMBOL focus:  + P4/P5/P6 (nodes) · CALLERS (edges) · CALLEES (edges) · IDENTITY anchor (nodes via context)
               · CYCLES (edges, IF owning module) · BOUNDARY (edges, IF decl) · GATE (edges, IF arch obligation)
FILE focus:    + IDENTITY/compute_file_summary (nodes) · IMPORTS (edges) · SYMBOLS/list_symbols_in_file (nodes)
PATH focus:    + IDENTITY/compute_path_summary (nodes) · FILES/list_files_in_path (nodes) · CYCLES (edges)
               · BOUNDARY (edges, IF decl) · GATE (edges, IF arch obligation)
```

CONSISTENCY WITH READINESS-9 [OBSERVED, not a contradiction]: readiness-9 line 124-125 summarized explain's base
read as "reads nodes/edges (identity/…)" while "GENUINELY SERVES 5 green leaf VALUES from the LiveGraph." This
slice EXTENDS that to the per-source precision a serve-vs-fallback design requires, and CONFIRMS the headline:
the served leaves are a SUBSET; trust-core (edges) + focus-resolution (nodes) are unconditional reads OUTSIDE
that subset. STOP-condition check (packet): explain's base use case + cert machinery were located and are
CONSISTENT with readiness-9 AND with the orient findings (same trust-core gap) — no contradiction; no stop on
that ground.

---

## 3. Per-source serve-vs-fallback classification (the field-level boundary)

The question the packet asks per source: **can it be served from current-state LiveGraph when GREEN, or must it
stay SQLite (fallback only)?** Verdicts are keyed to the ratified EXPLAIN-LIVEGRAPH-1 §2 source map (posture),
the LiveGraph surfaces that exist (lib.rs), and whether a NEW producer would be required.

Legend (same taxonomy as orient-sqlite-free-1 §3): **LG-servable-now** = a LiveGraph surface + a no-loss cert
exist AND explain ALREADY serves it (`82b6557`). **LG-derivable-but-ratified-SQLite** = the LiveGraph CAN
compute it, but EXPLAIN-LIVEGRAPH-1 deliberately kept it SQLite-first. **No-LG-producer** = nothing in the
LiveGraph produces it; a new producer/extraction would be required. **Not-a-decommission-target** = the read is
SQLite but does NOT touch `nodes`/`edges` (Authority/Tier-B/operational/FS), so it does not block the
`nodes`/`edges` retirement even if it stays.

| Source (§2 ref) | `nodes`/`edges`? | EXPLAIN-LIVEGRAPH-1 §2 posture | LiveGraph surface | Serve-on-green verdict |
|-----------------|------------------|--------------------------------|-------------------|------------------------|
| EXPLAIN_CALLERS (sym) | YES edges | **LG-first** (served) | `callers` (lib.rs:469) + per-symbol no-loss key compare | **LG-servable-now** — served today; serve on green, `find_symbol_callers` on fallback. |
| EXPLAIN_CALLEES (sym) | YES edges | **LG-first** (served) | `callees` (lib.rs:586) + per-symbol no-loss compare | **LG-servable-now** — served today (residency asymmetry → labelled SQLite fallback common). |
| EXPLAIN_IMPORTS (file) | YES edges | **LG-first** (served) | `live_import_view` (lib.rs:1675) + import no-loss cert | **LG-servable-now** — served today via `explain_imports_outcome` (explain_coherence.rs:246). |
| EXPLAIN_CYCLES (sym mod / path) | YES edges | **LG-first** (served) | `module_import_cycles` (lib.rs:1317) + module-cycle field-exact cert | **LG-servable-now** — served today. |
| EXPLAIN_IDENTITY **anchor** (sym) — name/subtype | YES nodes (via context) | **LG-first anchor** (D8 `{livegraph, sqlite}`, served) | `node_display` (lib.rs:1051) | **LG-servable-now for the ANCHOR ONLY.** The anchor name/subtype is served; BUT it consumes the SQLite-resolved key (P6) and the coordinate fields (path/line_start/module_path/language) STAY SQLite (serve_identity explain_lg_identity.rs:84-91). So serving identity does **NOT** remove the `nodes` read — it sits downstream of focus resolution and keeps the coordinate read. |
| **EXPLAIN_TRUST** (all 3) + **confidence** | **YES edges + unresolved_edges** | **SQLite trust-core (v1)** — explain-livegraph-1 §2 row + §1a tables (mod.rs:343/521/644) | partial — TRUST-LIVEGRAPH-1 adds a current-state POSTURE beside v1; it does NOT replace the edge-derived call-resolution/reliability core | **No-LG-producer** for the VALUES explain consumes (call_resolution_rate, call_graph_reliability, enrichment_state — build_trust_signal mod.rs:777-793; + confidence). UNCONDITIONAL in all three pipelines. → **DR-E1 (BLOCKING) — SAME as orient DR-1.** |
| **focus resolution** (P3–P6) | **YES nodes** | — (resolution layer; not a §2 signal) | none — target→node resolution was never migrated to the LiveGraph | **No-LG-producer.** UNCONDITIONAL (explain always resolves a target; no repo-focus exemption). → **DR-E2 (BLOCKING, unconditional).** |
| EXPLAIN_SYMBOLS (file) / EXPLAIN_FILES (path) listings | **YES nodes** | **SQLite-first** (listing-coherence; structural counts LG-derivable but deferred — D-EXPLAIN-LISTINGS) | `module_stats` (lib.rs:1376) proves the COUNT is LG-derivable; no per-item symbol/file LISTING surface exists | **LG-derivable-but-ratified-SQLite** for the COUNTS; **No-LG-producer** for the per-item LISTINGS (line_start/subtype/path/is_test have no LiveGraph home). → **DR-E3 (BLOCKING for file/path focus).** |
| EXPLAIN_IDENTITY counts (file/path) — symbol_count/file_count | **YES nodes** | **SQLite-first** (count-anchor; mirror orient D-ORIENT-2) | `module_stats` (lib.rs:1376) PROVES LG-derivable | **LG-derivable-but-ratified-SQLite.** Re-sourcing crosses EXPLAIN-LIVEGRAPH-1. → **DR-E3 (BLOCKING for file/path focus).** |
| EXPLAIN_BOUNDARY structural half (sym mod / path) | YES edges (cond.) | **Authority + SQLite-first** (structural import-edge half kept SQLite-first per contract) | imports surface is migrated (could answer "imports between A and B") | **LG-derivable-but-ratified-SQLite.** Conditional (declarations exist). → **DR-E4 (BLOCKING for full elimination).** |
| EXPLAIN_GATE arch_violations structural half | YES edges (cond.) | **Authority** (declarations have no LiveGraph home) | imports surface (for the arch_violations edge check only) | **LG-derivable-but-ratified-SQLite** for the structural half; the obligation/waiver eval is Authority and STAYS SQLite. Conditional (arch obligation exists). → **DR-E5 (low-priority).** |
| EXPLAIN_IDENTITY anchor INPUT key | (the key from P4/P5/P6) | — | none | **No-LG-producer** — the anchor serving REUSES the SQLite-resolved key (DR-E2). Folds into DR-E2. |
| Authority declarations — boundary/gate (E/I) | NO (`declarations`) | **Authority** | none (by construction) | **Not-a-decommission-target** → STAYS SQLite, does not block. |
| EXPLAIN_GATE obligation/waiver eval | NO (`declarations`/`measurements`/`inferences`) | **Authority** | none | **Not-a-decommission-target** → STAYS SQLite, does not block. |
| repo/snapshot identity (P1/P2) + `get_stale_files` | NO (`repos`/`snapshots`/`file_versions`) | **SQLite-first** (operational identity) | none | **Not-a-decommission-target** → STAYS SQLite. (`get_stale_files` is ALSO read by the SHIPPED overlay, explain_coherence.rs:79, for leaf freshness — preserved.) |

### 3a. Distilling the verdict (INFERRED)

```text
explain's `nodes`/`edges` reads fall into THREE classes (same shape as orient):

CLASS 1 — LG-servable NOW (an existing leaf cert covers it AND explain already serves it, `82b6557`):
  · callers (edges)          — EXPLAIN_CALLERS leaf, per-symbol no-loss key compare
  · callees (edges)          — EXPLAIN_CALLEES leaf, per-symbol no-loss key compare
  · imports (edges)          — EXPLAIN_IMPORTS leaf, import no-loss cert
  · cycles (edges)           — EXPLAIN_CYCLES leaf, module-cycle field-exact cert
  · identity ANCHOR (nodes)  — EXPLAIN_IDENTITY name/subtype via node_display
    (BUT the identity anchor still consumes the SQLite-resolved key + keeps SQLite coordinate fields — it does
     NOT by itself remove a `nodes` read; see CLASS 3 focus-resolution.)

CLASS 2 — LG-derivable but RATIFIED SQLite-first (re-sourcing crosses EXPLAIN-LIVEGRAPH-1):
  · EXPLAIN_SYMBOLS / EXPLAIN_FILES structural COUNTS (nodes)   — DR-E3 [file/path focus]
  · identity symbol_count/file_count (nodes)                    — DR-E3 [file/path focus]
  · EXPLAIN_BOUNDARY structural import-edge half (edges)        — DR-E4 [conditional: declarations]
  · EXPLAIN_GATE arch_violations edges (edges)                  — DR-E5 [conditional: arch obligation]
  (the per-item LISTINGS — line_start/subtype/path/is_test — are CLASS 3: no LiveGraph home at all.)

CLASS 3 — NO LiveGraph producer today (a new producer/extraction is required):
  · trust-core (edges + unresolved_edges)  — DR-E1  [UNCONDITIONAL, all three pipelines; SAME as orient DR-1]
  · focus resolution (nodes)               — DR-E2  [UNCONDITIONAL, every explain; no repo-focus exemption]
  · file/path per-item listings (nodes)    — part of DR-E3 [file/path focus]

Everything ELSE explain reads (Authority declarations, gate obligation/waiver eval, repos/snapshots,
get_stale_files) is NOT a `nodes`/`edges` read, so it does NOT block the `nodes`/`edges` retirement even though
it keeps explain touching SQLite on every call.

CONSEQUENCE: serving explain's five LG-first leaves on green (which `82b6557` already does, ON TOP of the eager
read) removes ONLY the Class-1 edge reads from the answer's VALUE SOURCE — but the Class-2/Class-3 reads survive,
including TWO UNCONDITIONAL Class-3 reads (trust-core edges in every pipeline; focus-resolution nodes on every
call). Therefore explain does NOT become `nodes`/`edges`-free on green by the leaf-serving alone. Full
elimination requires DR-E1 + DR-E2 + DR-E3 resolved (DR-E4/DR-E5 are conditional). This is the headline finding,
now proven per-source — and it is the SAME structural result as orient.
```

---

## 4. The cert / fingerprint gate for explain's full answer (INFERRED, mirroring the drilldowns + orient §4)

### 4a. Why explain cannot reuse a single drilldown cert verbatim

OBSERVED [stats-livegraph-1 §Target; orient-sqlite-free-1 §4a]: a drilldown cert (imports/cycles/stats) gates a
SINGLE structural answer — one no-loss compare of one LiveGraph payload against the SQLite payload, GREEN ⇒ skip
the one SQLite read. explain's answer is a UNION of independently-sourced sections (five LG-first leaves +
Class-2 structural + Class-3 trust/focus/listings + Authority + operational + freshness). There is no single
payload to compare. explain is a COMPOSITE, exactly like orient.

### 4b. The composite EXPLAIN no-loss cert (the design, conditional on §8)

```text
EXPLAIN cert {verdict: GREEN|RED, fingerprint}  — on RepoState, in-memory RwLock<Option<ExplainNoLossCert>>,
  S1 (rebuilt on restart), mirroring import_cert / cycles_cert / stats_cert. Keyed by the SHARED SQLite-free
  fingerprint (import_cert_fingerprint: partition epoch/hash set ⊕ classifier/policy version) — NO new
  invalidation key, REUSING import_cert_fingerprint (explain_coherence.rs:262 already uses it). Lazily built
  once per fingerprint; the SQLite read survives ONLY (i) to BUILD the cert and (ii) on fallback (the drilldown
  invariant).

GREEN  iff  ALL of the following contributing no-loss verdicts are GREEN at the current fingerprint, FOR THE
  FOCUS THIS CALL EMITS (symbol / file / path each fold a different subset):
  (1) per-symbol callers/callees no-loss compare (symbol focus)        [exists — orient_callers/callees_outcome]
  (2) import no-loss cert (file focus)                                 [exists — explain_imports_outcome :246]
  (3) module-cycle field-exact cert (symbol-module / path focus)       [exists — orient_cycles_outcome]
  (4) identity anchor cert (symbol focus)                              [exists — serve_identity ladder]
  (5) file/path structural-count no-loss cert + per-item listing cert  [needs DR-E3]
  (6) trust-core no-loss cert (ALL focuses)                            [needs DR-E1 — the decisive one]
  (7) focus-resolution no-loss cert (ALL focuses)                      [needs DR-E2]
  (8) BOUNDARY import-edge no-loss cert (only if declarations)         [needs DR-E4]
  (9) gate arch_violations import-edge no-loss cert (only if arch obl) [needs DR-E5]
AND precondition: every contributing partition resident + Fresh + TS-primary (non-TS ⇒ precondition unmet).

It is an AND-fold: the WEAKEST contributor decides (the MEET discipline the coherence root already uses,
explain_to_coherent). A RED or missing contributor ⇒ EXPLAIN cert RED ⇒ SQLite fallback (the full run_explain).
```

CRITICAL HONESTY [INFERRED]: contributors (5)(6)(7)(8)(9) do NOT exist today. (6) trust-core has **no LiveGraph
producer at all** (DR-E1) and runs UNCONDITIONALLY in every pipeline; (7) focus-resolution has no LiveGraph
producer and runs on every call. Without BOTH, the AND-fold can NEVER be GREEN. So under the CURRENT
architecture the composite EXPLAIN cert is **RED by construction** for every repo, and explain always falls
back. The cert design is therefore **specified but inert until DR-E1 (minimum) + DR-E2 + DR-E3 land**. This is
the SAME shape as orient-sqlite-free-1 §4b and stats-livegraph-1 (cert designed; a prerequisite producer had to
land first — IR-SYMBOL-ATTRIBUTES-1 for stats; the trust-core producer here).

---

## 5. Serve-then-fallback control flow (the target; honest about §8 gating)

The flow REPLACES the current "always run the full base SQLite use case, then swap in the five live leaves"
(dispatch.rs:2766→2795) with the drilldown serve-then-fallback ladder. It is built INSIDE
`build_explain_envelope`'s call site so the shipped `CoherenceEnvelope` provenance/freshness labels and the
five live-served leaf builders are reused verbatim — no new output contract.

```text
handle_explain:
  1. PRECONDITION CHECK (SQLite-free): the target's partition(s) resident + Fresh + TS-primary?
       NO  → FALLBACK: run repo_graph_agent::run_explain(&storage, target, …) (today's eager base read), wrap
             in the CoherenceEnvelope with every leaf provenance.source=sqlite, fallback_reason ∈
             {UnsupportedLanguage, Partial, Stale} (the SHIPPED FallbackReason mapping, explain_coherence.rs).
       YES → step 2.
       NOTE: the precondition itself needs a LiveGraph TARGET→PARTITION map without a SQLite focus read — i.e.
             DR-E2. Until DR-E2, the precondition cannot be evaluated `nodes`-free, so step 1 cannot be reached
             without already reading `nodes`. This is why DR-E2 is BLOCKING even for the precondition.
  2. EXPLAIN cert lookup at current fingerprint:
       missing/stale → LAZY BUILD (reads SQLite ONCE per fingerprint via the per-contributor compares), re-read.
       RED  → FALLBACK (as step 1, fallback_reason = the failing contributor's reason: CallgraphDivergence /
              ImportDivergence / CycleDivergence / <trust / listings / boundary divergence>).
       GREEN → step 3.
  3. FASTPATH (GREEN, no eager `nodes`/`edges` read): assemble the OrientResult from
       · LiveGraph: identity anchor (node_display), callers/callees, imports (live_import_view),
         cycles (module_import_cycles),
         file/path structural counts + listings (the NEW listing producer)   [needs DR-E3],
         trust-core summary (the NEW LiveGraph trust producer)               [needs DR-E1],
         focus resolution (the NEW LiveGraph focus-resolution path)          [needs DR-E2],
         BOUNDARY / gate arch edges via the imports surface                  [needs DR-E4/DR-E5, only if decl/arch],
       · SQLite (NON-`nodes`/`edges`, retained — these never blocked the decommission):
         Authority declarations (boundary/gate obligation+waiver eval), repos/snapshots (identity),
         get_stale_files (leaf freshness, explain_coherence.rs:79),
       Wrap in the CoherenceEnvelope with the served leaves provenance.source=livegraph + the cert's
       freshness/completeness; the retained SQLite sections labelled source=sqlite/authority (UNCHANGED from the
       shipped overlay). The root MEET + trust_briefing are computed exactly as today (explain_coherence.rs:189-191).
```

The fastpath (step 3) skips `resolve_*_focus` / `get_symbol_context`, `find_symbol_callers/callees`,
`find_file_imports`, `find_cycles_involving_*`, `compute_*_summary`, `list_symbols_in_file`,
`list_files_in_path`, `find_imports_between_paths`, AND the trust-core `edges` reads — i.e. it skips EVERY
Class-1/2/3 `nodes`/`edges` read. It retains the non-graph SQLite reads. THAT is precisely "no eager
`nodes`/`edges` read on green," and ONLY that.

---

## 6. What this does and does NOT achieve (honesty — per readiness-9 discipline)

```text
DOES (when DR-E1 + DR-E2 + DR-E3 are ratified + the trust-core + focus-resolution producers land):
  + Removes explain's eager `nodes`/`edges` read on the GREEN served path (the Class-1/2/3 reads), converting
    explain from "swap five live leaves onto an eager SQLite read" to "serve-then-fallback" — the proven
    drilldown posture. This BUILDS ON the real serving `82b6557` already shipped (the five leaf builders are reused).
  + Reuses the shipped CoherenceEnvelope output contract verbatim (provenance/trust/freshness labels, MEET root,
    trust_briefing): no new wire shape, no human-output break (the served values are no-loss-equal to the SQLite
    values by cert).
  + Keeps the labelled SQLite fallback for not-green / non-TS / non-resident / stale (honest degradation).

DOES NOT (the boundaries readiness-9 demands be stated):
  - Does NOT make explain SQLite-FREE. Authority declarations, gate obligation/waiver eval, repos/snapshots, and
    get_stale_files remain read on the served path. NONE are `nodes`/`edges`, so they do not block the
    `nodes`/`edges` retirement — but "explain no longer touches SQLite" is FALSE and must not be claimed.
  - Does NOT remove the non-TS fallback. LiveGraph is TS-only; every non-TS repo/file/symbol falls back to the
    full SQLite base read. `nodes`/`edges` stay load-bearing for C/C++/Rust/Java (deletion gate 2, the
    structural ceiling).
  - Does NOT remove the cert-BUILD SQLite read (once per fingerprint) — the drilldown invariant survives.
  - Is INERT until at least DR-E1 + DR-E2 land: the composite cert is RED by construction while trust-core has
    no LiveGraph producer AND focus resolution has no LiveGraph path (§4b). So this slice's IMPL cannot ship a
    working green fastpath without those prerequisites.
  - Does NOT, by itself, retire `nodes`/`edges`. The other defaults' fallbacks, the imports/cycles/stats cert
    builds, and the 31 non-graph tables remain (readiness-9 gates 2–5).
```

---

## 7. Validation plan (for the eventual IMPL; mirrors the drilldown proofs + orient §7)

NOT RUN here (spec-first; no code). The IMPL slice (EXPLAIN-SQLITE-FREE-IMPL-1) must produce, mirroring
stats-livegraph-1 §validation and the shipped explain_coherence_served_tests.rs:

```text
PARITY (green compare):  rmap explain --engine compare on a TS pilot where the EXPLAIN cert is GREEN →
  is_exact=true: the LiveGraph-assembled OrientResult is BYTE-equal (post-canonicalization) to the SQLite base
  result, leaf-by-leaf (identity, callers, callees, imports, cycles, symbols/files listings, trust summary,
  confidence). A single divergent field ⇒ RED ⇒ fallback (no silent mismatch). [EXECUTED proof required.]
NO-EAGER-READ PROOF:     a unit/integration test that, on a GREEN cert + precondition met, asserts the served
  path performs ZERO `nodes`/`edges` reads — e.g. a storage spy / panicking-closure on resolve_*_focus /
  get_symbol_context / find_symbol_callers / find_symbol_callees / find_file_imports / find_cycles_involving_* /
  compute_*_summary / list_symbols_in_file / list_files_in_path / find_imports_between_paths / the trust-core
  edge reads, mirroring the callers/callees lazy proof (readiness-9 gate 5). This is the load-bearing test: it
  is the operational definition of "eager read eliminated."
FALLBACK CORRECTNESS:    non-TS repo → fallback (UnsupportedLanguage); non-resident partition → fallback
  (Partial); stale index → fallback (Stale); cert RED (any contributor diverges) → fallback (the named
  divergence reason). Each labelled in the CoherenceEnvelope provenance. Default `--engine auto` unchanged for
  the human renderer (byte-compatible).
CERT-BUILD-ONCE:         the cert is built once per fingerprint (SQLite read on build only), reused across calls,
  invalidated on fingerprint change/restart (mirror import_cert / cycles_cert / stats_cert).
SCOPE GUARD:             orient (deferred) / check / trust are NOT touched (other Option-B slices); only
  explain's handler + a new explain serve path change.
```

EXECUTED this slice:
- `git log --oneline -15` → HEAD `e10a455`; coherence chain `6ed17b8..dc55114`; EXPLAIN-LIVEGRAPH-IMPL `82b6557`.
  [OBSERVED — confirms the baseline this spec builds on.]
- `git status --short` → empty (clean tree); `git show --stat 82b6557` → the explain LG-serve modules are
  COMMITTED, not working-tree. [OBSERVED — the five served leaves are shipped, not pending.]
- `grep`/`Read`/`sed` over the explain use case (mod.rs), the storage adapter (agent_impl.rs), the trust core
  (trust_impl.rs), the daemon handler (dispatch.rs), the explain coherence/serve/identity modules, and the
  LiveGraph lib.rs — every §2/§3 OBSERVED claim re-verifiable at the cited file:line.
NOT RUN: cargo build/test, dev-install, live `rmap explain` capture — spec-first; no source path touched; daemon
start runs index/refresh (state-mutating, out of scope). [Same posture orient took; no daemon available.]

---

## 8. Forced decisions — `DECISION_REQUIRED` (architecture-boundary + new-producer blocks)

Per CLAUDE.md Decision Autonomy: a re-sourcing that contradicts a ratified decision (EXPLAIN-LIVEGRAPH-1's
SQLite-first postures), a new dependency edge (a LiveGraph trust producer; a LiveGraph focus resolver), or a new
data shape crossing a boundary is a **stop-and-ask, presented as an exhaustive matrix**. The packet's
STOP_CONDITION is explicit: "If any base source cannot be LiveGraph-served without a new producer, STOP and emit
DECISION_REQUIRED." Multiple sources qualify; the meta-sequencing is DR-0.

```text
DECISION_REQUIRED:
- ID: DR-0-SEQUENCING
  QUESTION: explain is a COMPOSITE; its eager `nodes`/`edges` read cannot be eliminated by one cert-flip, AND it
            is producer-gated by the SAME trust-core source as orient (DR-E1) PLUS an unconditional
            focus-resolution gap (DR-E2). The DR-0 → S3 hypothesis (explain is the producer-LIGHT lead because
            it already serves 5 leaves) is REFUTED. What is the build sequence for Option B now?
  OPTIONS:
  - S1 SHARED-PREREQUISITE-FIRST (recommended): ratify DR-E1, land a SINGLE shared trust-core LiveGraph producer
    (TRUST-SUMMARY-LIVEGRAPH-1) that serves BOTH orient and explain (and corroborates trust's hybrid), THEN
    sequence the per-command eager-read eliminations (explain + orient) behind it, EACH additionally needing its
    own focus-resolution + listings producers (DR-E2/DR-E3). Mirrors stats → IR-SYMBOL-ATTRIBUTES-1.
    Consequence: Option B is correctly re-scoped as "producer program first, fastpaths second"; the trust
    producer is built ONCE for the whole cluster. No command ships a dead always-RED fastpath.
  - S2 EXPLAIN-LEADS-ANYWAY: build the explain fastpath now with the cert RED by construction (trust-core +
    focus-resolution never GREEN) → explain ALWAYS falls back → ZERO decommission win, dead code. Rejected
    (no value; breaks the cert — the SAME reason orient's S2 was rejected by builder + reviewer).
  - S3 RECONSIDER-OPTION-A: because BOTH orient and explain (and, by construction, check/trust) are producer-gated
    on the trust-core producer, Option B is UNIFORMLY producer-gated. Re-weigh Option A (non-TS LiveGraph
    coverage, the larger strategic unlock per readiness-9) vs the Option-B producer program in the A-vs-B
    sequencing call readiness-9 left open. Consequence: the next build may be the trust producer (serves both A
    and B futures) OR an Option-A coverage slice — a governance call above this spec.
  RECOMMENDED: S1. The trust-core producer is the shared, unavoidable prerequisite for the entire coherence
    cluster; building it once unblocks explain AND orient AND strengthens trust. It is the only sequence that
    makes any command's "no eager `nodes`/`edges` read on green" claim TRUE, and it matches the proven stats
    precedent (prerequisite producer, then fastpath).
  BLOCKING_REASON: building the explain fastpath before the trust-core + focus-resolution producers ship
    produces an always-RED cert (dead fallback-only code, §4b). The sequence — and the A-vs-B re-weigh — must be
    chosen before any IMPL. This DR also records the REFUTATION of the DR-0 → S3 producer-light hypothesis,
    which is itself a governance-relevant finding.

- ID: DR-E1-TRUST-CORE-PRODUCER  [SHARED with ORIENT-SQLITE-FREE-1 DR-1]
  QUESTION: explain's trust aggregator reads `edges` + `unresolved_edges` UNCONDITIONALLY in all three pipelines
            (get_trust_summary → assemble_trust_report; trust_impl.rs:116/149/214/265/344/353) and feeds BOTH
            EXPLAIN_TRUST (build_trust_signal, mod.rs:422/544/710) and the envelope confidence
            (derive_repo_confidence, mod.rs:442/564/730). It is ratified SQLite trust-core (explain-livegraph-1
            §2 + §1a). TRUST-LIVEGRAPH-1 added a current-state POSTURE beside the v1 report; it did NOT replace
            the edge-derived call-resolution/reliability core. How is the trust SUMMARY explain consumes
            (call_resolution_rate, call_graph_reliability, enrichment_state) served without reading `edges`?
  OPTIONS:
  - A NEW SHARED PRODUCER (recommended): a TRUST-SUMMARY-LIVEGRAPH producer that computes call-resolution +
    reliability axes from the LiveGraph (resolved/unresolved adjacency already in the IR/xref) instead of
    `edges`/`unresolved_edges`, with a no-loss cert vs the SQLite trust summary. SHARED with orient DR-1 — ONE
    slice serves both commands. New producer + new dependency edge. The only path to a GREEN composite cert.
  - B PERMANENT-SQLITE: accept trust-core as a permanent SQLite `edges` read on explain's served path.
    Consequence: explain NEVER becomes `nodes`/`edges`-free (gate 1 stays FAIL for explain); the whole slice's
    value evaporates. Rejected as a path to decommission; acceptable only if the operator deprioritizes explain.
  - C DROP-TRUST-ON-GREEN: omit EXPLAIN_TRUST + the trust-derived confidence when serving from LiveGraph.
    Consequence: a DIFFERENT (degraded) answer on green vs fallback; violates overlay-never-erases and the
    confidence contract. Rejected (false completeness / certainty mislabel).
  RECOMMENDED: A. It is the only option that removes the read AND preserves the answer, AND it is shared work
    (one producer unblocks explain + orient + trust's hybrid).
  BLOCKING_REASON: trust runs in every explain pipeline unconditionally; until A lands, the composite EXPLAIN
    cert is RED by construction (§4b) and no explain focus can be `nodes`/`edges`-free on green. This is the
    decisive block — IDENTICAL to orient's decisive block, by the SAME source.

- ID: DR-E2-FOCUS-RESOLUTION  [orient DR-4, but UNCONDITIONAL for explain]
  QUESTION: every explain resolves its target to a node via resolve_path_focus (mod.rs:86, UNCONDITIONAL) and
            conditionally resolve_stable_key_focus / resolve_symbol_name / get_symbol_context — each reads
            `nodes` (agent_impl.rs:366/437/800/834). explain has NO repo/`None` focus (dispatch.rs:2738 requires
            a target), so unlike orient (where DR-4 was focused-only) the `nodes` read is UNCONDITIONAL. Even the
            served identity anchor reuses the SQLite-resolved key (serve_identity explain_lg_identity.rs:76). No
            LiveGraph focus-resolution path exists. How is explain made `nodes`-free on green?
  OPTIONS:
  - A LG-FOCUS-RESOLUTION (recommended — REQUIRED, no scope-cut escape): a LiveGraph target→IR-symbol/file/module
    resolver (new surface) + cert, so the precondition check and the identity anchor can locate the node without
    a SQLite `nodes` read. New producer/data-shape across a boundary. Unlike orient, explain has no repo-focus
    fallback to scope to, so this is UNAVOIDABLE for ANY green elimination.
  - B PERMANENT-SQLITE: leave focus resolution on `nodes`. Consequence: an UNCONDITIONAL `nodes` read survives on
    every explain call → explain NEVER `nodes`-free even with DR-E1 solved. Rejected for the goal.
  RECOMMENDED: A. There is no S-curve scope cut for explain here (no repo-focus mode); the resolver is required.
  BLOCKING_REASON: the focus-resolution read is UNCONDITIONAL on every explain; without A the precondition (§5
    step 1) cannot even be evaluated `nodes`-free. This is a SECOND unavoidable producer, distinct from DR-E1 —
    and it makes explain STRICTLY harder than orient's repo-focus first increment.

- ID: DR-E3-LISTINGS-AND-COUNTS
  QUESTION: file/path focus emit EXPLAIN_SYMBOLS / EXPLAIN_FILES listings + EXPLAIN_IDENTITY structural counts
            from `nodes` (list_symbols_in_file agent_impl.rs:1037; list_files_in_path :1070; compute_file_summary
            :647; compute_path_summary :587), ratified SQLite-first (D-EXPLAIN-LISTINGS). `module_stats`
            (lib.rs:1376) proves the COUNTS are LG-derivable, but no per-item LISTING surface exists. Re-source?
  OPTIONS:
  - A RE-SOURCE-COUNTS + NEW-LISTING-PRODUCER (recommended for file/path coverage): serve structural counts from
    `module_stats` (cert-gated, reusing the stats no-loss compare) AND add a LiveGraph per-file/per-symbol
    LISTING producer (name/subtype/line_start/is_test have no LiveGraph home today) + cert. Removes the `nodes`
    listing read on green. Crosses the ratified SQLite-first posture → must be re-ratified.
  - B KEEP-SQLITE-FIRST: leave listings/counts on `nodes`. Consequence: an UNCONDITIONAL `nodes` read survives on
    file/path focus → explain never `nodes`-free for those focuses even with DR-E1/DR-E2 solved (symbol focus
    could be, file/path could not). Acceptable only if the first IMPL scopes to SYMBOL focus.
  RECOMMENDED: A for full coverage; B is a defensible interim IF the first IMPL targets SYMBOL focus only (which
    still needs DR-E1 + DR-E2). The listing producer is genuinely new work (the snapshot-scoped per-item fields
    have no IR home — mirror the orient RISK-E coordinate gap).
  BLOCKING_REASON: the listing/count read is UNCONDITIONAL on file/path focus; B defeats file/path elimination.
    Re-ratifying a ratified decision + adding a listing producer is an architecture-boundary call.

- ID: DR-E4-BOUNDARY-VIOLATION-EDGES  [mirror orient DR-3]
  QUESTION: EXPLAIN_BOUNDARY reads `edges` via find_imports_between_paths (mod.rs:378/670), ratified SQLite-first.
            Route the import-edge check through the migrated LiveGraph imports surface?
  OPTIONS:
  - A ROUTE-VIA-IMPORTS-LG (recommended for full elimination): answer "imports between path A and B" from the
    LiveGraph imports surface, cert-gated; re-ratify off SQLite-first. Conditional (only when ≥1 boundary
    declaration). The Authority declaration STAYS SQLite (it is not `nodes`/`edges`).
  - B KEEP-SQLITE-FIRST: leave it on `edges`. Consequence: on repos WITH boundary declarations an `edges` read
    survives on green; repos without declarations are unaffected. Acceptable as a short-term scope cut.
  RECOMMENDED: A for completeness; B is a defensible interim if the first IMPL targets declaration-free repos.
  BLOCKING_REASON: a `nodes`/`edges` read survives on green for declaration-bearing repos under B; choosing A
    re-ratifies a ratified posture (architecture-boundary).

- ID: DR-E5-GATE-ARCH-EDGES  [mirror orient DR-5]
  QUESTION: gate's arch_violations obligation method reads `edges` (conditional on an arch_violations obligation
            existing). The obligation/waiver EVALUATION is Authority (no LiveGraph home). Route only the
            structural edge check through the LiveGraph imports surface?
  OPTIONS:
  - A ROUTE-ARCH-EDGE-VIA-IMPORTS-LG: cert-gate the arch_violations edge check via the LiveGraph imports surface;
    keep obligation/waiver eval SQLite (Authority). Removes the conditional `edges` read on green.
  - B KEEP-SQLITE (recommended interim): leave it; the read fires only when an arch_violations obligation exists
    (rare in the corpus). Lowest priority; revisit after DR-E1..DR-E4.
  RECOMMENDED: B short-term (conditional/rare), A for completeness.
  BLOCKING_REASON: low — a conditional `edges` read on green for arch-obligation repos only. Not on the critical
    path, but must be acknowledged for a TRUE `nodes`/`edges`-free claim.
```

---

## 9. Scope boundary

```text
IN SCOPE (this spec): explain ONLY. The source enumeration (§2), the serve-vs-fallback classification (§3), the
  composite cert design (§4), the serve-then-fallback flow (§5), the honesty section (§6), the validation plan
  (§7), the architecture-boundary decisions (§8), and the explicit PRODUCER-LIGHT-vs-PRODUCER-GATED VERDICT (§11).
OUT OF SCOPE: any code, table deletion, migration, or default flip (spec-first). orient (deferred) / check /
  trust (other Option-B slices). The explain IMPLEMENTATION (EXPLAIN-SQLITE-FREE-IMPL-1, gated on §8). The
  trust-core LiveGraph producer (DR-E1 → its own SHARED prerequisite slice). The focus-resolution + listing
  producers (DR-E2/DR-E3 → their own slices). Non-TS LiveGraph coverage (Option A; readiness-9). The 31
  non-graph tables and the other defaults' fallbacks (the broader decommission). ROADMAP.md / CURRENT_SLICE.md
  edits (read-only here).
```

---

## 10. References

- `docs/slices/orient-sqlite-free-1.md` — THE precedent: the CLASS 1/2/3 taxonomy, the composite-cert design,
  the trust-core blocker (DR-1, SHARED here as DR-E1), and DR-0 → S3 (which assigned this slice the lead).
- `docs/slices/sqlite-raw-decommission-readiness-9.md` — the Option-B driver; the eager-read-not-skipped finding
  ("real build per command, not a cert-flip"); the gate-RED recompute; the A-vs-B open call.
- `docs/slices/explain-livegraph-1.md` §2 — the ratified per-signal source map (LG-first / SQLite-first /
  Authority postures this slice builds on and, for DR-E3/DR-E4, proposes re-ratifying); §1a/§1c (EXPLAIN_TRUST
  = SQLite trust-core in all three pipelines).
- `docs/slices/stats-livegraph-1.md` — the cert-fastpath precedent + its D0 architecture-boundary split (spec →
  prerequisite producer → fastpath impl); the model this spec follows.
- `docs/slices/coherence-layer-1.md` — the ratified `CoherenceEnvelope<T>` contract (the output shape reused
  verbatim).
- `rust/crates/agent/src/explain/mod.rs` — the base use case (`run_explain`:55; the three pipelines; the
  unconditional trust read; build_trust_signal:777) — the §2 enumeration.
- `rust/crates/storage/src/agent_impl.rs` + `trust_impl.rs` — the storage reads (§2/§3 `nodes`/`edges`
  classification; the trust-core edge reads).
- `rust/crates/daemon-runtime/src/dispatch.rs:2730-2808` + `explain_coherence.rs` + `explain_lg_serve.rs` +
  `explain_lg_identity.rs` — handle_explain (eager read :2766) + the shipped REAL serving this slice converts to
  serve-then-fallback.
- `rust/crates/repo-graph-livegraph/src/lib.rs` — the LiveGraph surfaces (node_display / callers / callees /
  live_import_view / module_import_cycles / module_stats) the fastpath would consume, and the ABSENCE of a
  trust-summary / focus-resolver / per-file symbol-listing surface (the DR-E1/DR-E2/DR-E3 gaps).

---

## 11. VERDICT (the key output)

**explain is PRODUCER-GATED, not PRODUCER-LIGHT.** [INFERRED over OBSERVED §2/§3.]

Evidence-anchored statement of the verdict:

- explain's eager `nodes`/`edges` read CANNOT be eliminated on green by serving its five LiveGraph leaves alone
  (those leaves are genuinely served today, `82b6557`, but are a strict SUBSET of the answer).
- TWO UNCONDITIONAL base sources have NO LiveGraph producer today:
  1. **trust-core** — `get_trust_summary` → `edges` + `unresolved_edges`, in EVERY pipeline (mod.rs:343/521/644;
     trust_impl.rs:116/149/265/344), feeding EXPLAIN_TRUST + confidence. This is the **SAME source** that gates
     orient (DR-1). [OBSERVED.]
  2. **focus resolution** — `resolve_*_focus` / `get_symbol_context` → `nodes`, on EVERY explain (mod.rs:86 +
     115/159/117; agent_impl.rs:366/437/800/834), with no repo-focus exemption. [OBSERVED.]
- A new producer (minimally DR-E1, the trust-core LiveGraph projection) is REQUIRED before any green explain
  fastpath can be `nodes`/`edges`-free. DR-E2 (focus-resolution) is a second unavoidable producer; DR-E3 gates
  file/path focus. [INFERRED.]

**This REFUTES the DR-0 → S3 working hypothesis** that explain — because it already serves 5 leaf values — is
the likeliest first REAL eager-read elimination WITHOUT a producer program. The serving advance (`82b6557`) is
necessary but not sufficient: explain carries the SAME unconditional trust-core dependency orient does, PLUS an
unconditional focus-resolution gap that orient only had conditionally. explain is, if anything, STRICTLY harder
than orient's repo-focus first increment (orient could scope to `focus=None` and skip focus resolution; explain
has no such mode).

**Governance consequence (drives the next step):** Option B is **uniformly producer-gated on the trust-core
producer**. The same `TRUST-SUMMARY-LIVEGRAPH-1` producer (DR-E1 = orient DR-1) is the shared, unavoidable
prerequisite for the whole coherence cluster (orient, explain, and trust's hybrid corroboration). The next
governance step is the DR-0 sequencing call: **S1 (build the shared trust-core producer first, then the
per-command fastpaths)** is recommended; the A-vs-B re-weigh (readiness-9's open call) is reopened by the fact
that BOTH leading Option-B commands are now proven producer-gated by the same source. This spec stands as the
authoritative explain producer-gap map; DR-0 + DR-E1..DR-E5 (§8) remain OPEN, to be ratified before any
EXPLAIN-SQLITE-FREE-IMPL-1.
