# STATS-HONESTY-1: stop `stats` rendering unknowns as facts — SPEC

Slice: STATS-HONESTY-1
Status: **SPEC** (design; the IMPL is the relay slice that follows)
Track: Product-surface honesty (`docs/ROADMAP.md` → Current priority, P2 "stats false-zero" +
P2 "stats reliability marker"). Coordinates with the queued `RELIABILITY-REFRAME-1` (Resolution &
attribution track) — the scope boundary is **surfaced and recommended in §6 + D2, pending operator
ratification**, so that once ratified the two slices never edit the same reliability computation.
This SPEC carries no operator ratification yet (no §12-style ratification block, unlike
`module-model-1.md`); the recommendations below are advisory until ratified.
Pairs: this slice designs the fix for **TECH-DEBT #5** (stats prints `total_symbols: 0` /
per-group `symbols=0` while `orient` reports a real count on the same snapshot) and
**TECH-DEBT #6** (stats reports Martin fan-in/out + distance-from-main-sequence as bare numbers
with no import-resolution reliability marker) as ONE coherent honesty slice.
Grounded in: the first End-to-End Usefulness Protocol run on spring-petclinic
(`smoke-runs/2026-06-21T15-55-40Z/`), confirmed against the actual code (cited first-hand below).
Model: this doc follows `docs/slices/module-model-1.md` (problem+evidence → root cause confirmed
against code → principle → desired before/after → root-cause design → cross-slice coordination →
decisions-to-surface → per-choice VISION defense → validation → smallest-design + STOP assessment).
Prior art: `docs/slices/module-model-1.md` (MODULE-MODEL-1) already relabelled the `stats` Summary to
`package groups` / `directory groups`; **this slice builds on that renderer** and fixes the two
honesty defects it explicitly left out of scope (module-model-1.md §11: "`total_symbols: 0` … and
stats fan-in/out reliability marker — separate slice"). `docs/slices/stats-livegraph-1.md`
(STATS-LIVEGRAPH-1) is the LiveGraph fastpath/cert this slice must propagate the #5 change through.

---

## 0. Evidence law (how to read the claims below)

- `OBSERVED` — read first-hand from source (cited by path/line) or from the smoke capture artifact.
- `INFERRED` — concluded from cited code, not directly executed.
- The IMPL re-validates everything labelled INFERRED via live isolated `rmap` capture (§9).

All source line numbers are against the working tree at spec time; the IMPL confirms before editing
(lines drift). Every citation below was read first-hand during this authoring unless marked otherwise.

---

## 1. The problem (OBSERVED, real output)

On spring-petclinic (`smoke-runs/2026-06-21T15-55-40Z/`), two commands describe the SAME snapshot
and disagree about whether the code even contains symbols; and on syntax-only C/C++, `stats` presents
coupling ratios computed on a fraction of the real edges with no reliability marker. Two distinct
failures, each forbidden by the VISION's Fact-Certainty layer model ("outer layers must surface
unknowns") and "labels speak the reader's language":

### #5 — false zero (P2)

`orient` (`spring-petclinic-orient---full.txt`) headline:
```
spring-petclinic · 49 files, 290 symbols · ...
```
`stats` (`spring-petclinic-stats.txt`) on the SAME snapshot — Summary + per-row `symbols` are zero
(rendered on the current MODULE-MODEL-1 renderer; the smoke capture predates the
`package groups`/`directory groups` relabel but the zero is identical):
```
Summary
  package groups: 6
  directory groups: 11
  total_files: 47
  total_symbols: 0                              ← FALSE ZERO (orient says 290)
...
By size
  …/owner   files=12  symbols=0                 ← FALSE ZERO per group
  …/vet     files=6   symbols=0
  …
```
A **false zero rendered as a fact** is worse than a missing value: an agent reading
`total_symbols: 0` / `symbols=0` can conclude "empty/dead area" and act on it (skip, delete, deprioritize).

### #6 — overclaim (P2)

On a syntax-only C/C++ repo (no `compile_commands.json`; `#include` largely unresolved — TECH-DEBT
§Extraction—C/C++ → Shared limitations), the IMPORTS graph is highly incomplete, yet `stats` prints
the dependency-derived metrics as bare numbers with no marker:
```
By fan-in
  src/core    fan_in=3  fan_out=1
By fan-out
  src/http    fan_out=5  fan_in=0
By distance from main sequence
  src/foo     D=0.87  I=0.13  A=0.00
```
`orient` on the same incomplete graph DOES surface a caveat ("import-graph reliability LOW") post
ORIENT-DENSITY-IMPL-1; `stats` does not. The same incomplete graph is honest in one command and an
overclaim in another. The VISION's own worked example of an overclaim is exactly this: "raw counts
without coverage or confidence markers are overclaims" (Product Layer Model, Dependency Rule #3).

---

## 2. Root cause — confirmed against code

### 2a. #5 — `symbol_count` counts only `visibility = 'export'`; non-TS extractors never emit `export`

The path that runs on spring-petclinic (Java) and on syntax-only C/C++ is the **SQLite fallback**
(`compute_module_stats`), because the LiveGraph fastpath is TS-only and falls back on any non-TS /
non-resident / non-corresponding repo (OBSERVED: dispatch.rs `handle_stats` :1400-1447 → default
`auto` → `stats_auto_response`; STATS-LIVEGRAPH-1 §Target precondition "non-TS → SQLite fallback").
So the SQLite query is the source of the zero:

- The per-group `symbols=N` column and the Summary `total_symbols` both come from one field,
  `ModuleStatsResult.symbol_count`.
  - OBSERVED: `rgr/src/presentation/stats.rs:129` `total_symbols = Σ m.symbol_count`; `:166`
    renders `symbols={m.symbol_count}` per directory group.
- `symbol_count` is computed as the count of **exported** symbols only:
  - OBSERVED: `storage/src/queries.rs:1181`
    `export_count = SUM(CASE WHEN n.visibility = 'export' THEN 1 ELSE 0 END)` (over `nodes` kind='SYMBOL'),
    rolled up at `:1196` `symbol_count = SUM(COALESCE(fs.export_count, 0))`, selected at `:1239`.
- The `visibility` column is the serde-lowercased `Visibility` enum: `Export → "export"`,
  `Public → "public"`, `Private → "private"`, …
  - OBSERVED: `indexer/src/types.rs:201-209` (`#[serde(rename_all = "lowercase")] enum Visibility { Public, Private, Protected, Internal, Export }`).
- The **Java extractor never emits `Visibility::Export`** — Java symbols are `Public`/`Private`/etc.:
  - OBSERVED: `java-extractor/src/extractor.rs:1079` `visibility: Some(Visibility::Public)`;
    `:1387` `fn extract_visibility`; tests `:1556` (`Some(Visibility::Public)`), `:1639`
    (`Some(Visibility::Private)`). No `Export` arm exists for Java.
  - Contrast — the Rust extractor DOES map `pub*` → `Export` (OBSERVED:
    `rust-extractor/src/extractor.rs:435/468/494/520/677/730/754`; `lib_rs_MAP.md:41` "`pub` /
    `pub(crate)` / `pub(super)` => EXPORT; otherwise PRIVATE"), so Rust repos get a non-zero
    `symbol_count`; Java/C/C++ get `'export'`-match = **0**.

⇒ `visibility = 'export'` matches **zero** Java symbols ⇒ `symbol_count = 0` for every module ⇒
`total_symbols = Σ 0 = 0`. The metric is **not "unpopulated"** (the TECH-DEBT #5 hypothesis); it is a
TS/Rust-centric `'export'` filter that Java (and syntax-only C/C++) symbols never satisfy.

### 2b. #5 — the real count IS cheaply available on the same SQLite path (orient already reads it)

`orient`'s "290 symbols" is the **unfiltered** symbol-node count over the SAME snapshot/DB:

- OBSERVED: `storage/src/agent_impl.rs:243-250` (`compute_repo_summary`):
  `SELECT COUNT(*) FROM nodes WHERE snapshot_uid = ? AND kind = 'SYMBOL'` — **no `visibility`
  filter**. Surfaced via `agent/src/storage_port.rs` (`compute_repo_summary`) →
  `agent/src/aggregators/module_summary.rs` → `rgr/src/presentation/orient_sections.rs:127-131`
  (`", {} symbols"`). [Read-trace via subagent; the COUNT(*) site itself read first-hand.]

⇒ The "real symbol count" the fix needs is one already-implemented `COUNT(*)` query (the same number
orient shows). Per-group all-symbol counts are the SAME `nodes` rows the export filter throws away —
a query-level aggregation change over existing data, **not new extraction**. (STOP-condition
assessment §10.)

### 2c. #6 — the dependency-derived metrics ride the IMPORTS graph; the reliability signal already exists, stats just doesn't read it

- `fan_in`/`fan_out` are counts of distinct MODULE↔MODULE `IMPORTS` edges; `instability =
  fan_out/(fan_in+fan_out)`; `distance = |abstractness + instability − 1|`.
  - OBSERVED: `storage/src/queries.rs:1205-1224` (fan_in/fan_out CTEs over `type='IMPORTS'`),
    `:1278-1279` Martin metrics. So fan-in/out, instability, and distance all **depend on the
    resolved import graph** — exactly the graph that is incomplete on syntax-only C/C++.
- The import-graph reliability signal `orient` surfaces is a **pure, reusable** function with
  THREE distinct non-HIGH reasons (read first-hand, `rust/crates/trust/src/rules.rs:171-199`):
  `compute_import_graph_reliability(alias_resolution_suspicion, registry_pattern_suspicion,
  unresolved_imports_count) → ReliabilityAxisScore { level, reasons: Vec<String> }`. The branches:
  - **LOW** iff `unresolved_imports_count > 0` (reason `"unresolved_imports={N}"`) **or**
    `alias_resolution_suspicion` (reason `"alias_resolution_suspicion"`) — the two may co-occur.
  - **MEDIUM** iff not-LOW and `registry_pattern_suspicion` (reason `"registry_pattern_suspicion"`).
  - **HIGH** otherwise (`reasons` empty).

  These three reasons mean three DIFFERENT things for the dependency metrics (missing edges vs
  mis-targeted edges vs registry-invisible edges), which is **why the #6 caveat must be
  reason-specific, not one fixed sentence** (§7 D3/D4): a single "{N} imports unresolved →
  under-counted" wording would print "0 imports unresolved" and assert under-counting when the cause
  is alias/registry at count 0 — manufacturing a NEW false claim (review-1 #1). The reason strings are
  internal-context (`"unresolved_imports=944"`), and `orient` already converts them **reason-by-
  reason** to prose — OBSERVED first-hand:
  `rust/crates/rgr/src/presentation/orient_reliability.rs:96-139` (`humanize_reason`:
  `unresolved_imports={N}` → "{N} unresolved imports"; `alias_resolution_suspicion` → "alias
  resolution suspected"; `registry_pattern_suspicion` → "registry/factory patterns detected"), gated
  on `level != "HIGH"` at `:32-35`. That reason-by-reason posture is exactly what `stats` must mirror
  (in reader-context wording).
  - It is assembled by the shared trust report (OBSERVED-via-subagent: `trust/src/service.rs:252-272`;
    `daemon-runtime/src/util/trust.rs` `assemble_trust_report`; `daemon-runtime/src/orient_coherence.rs`
    `compute_trust_overlay_for_snapshot`). `handle_stats` does **not** call it today
    (OBSERVED: dispatch.rs `handle_stats:1349-1483` reads only `compute_module_stats` / the fastpath).

⇒ #6 is a **surfacing** gap: the signal exists and is reusable; `stats` simply doesn't consume it.
The fix wires the existing signal into the stats response and renders a reader-context caveat — it
does **not** compute anything new and does **not** touch the reliability computation (that's
RELIABILITY-REFRAME-1's territory — §6).

### Root-cause summary

| Symptom (OBSERVED) | Mechanism (cited) | Fix class |
|---|---|---|
| `total_symbols: 0`, per-group `symbols=0` on Java / non-TS | `symbol_count = SUM(visibility='export')` (queries.rs:1181); Java emits `Public` not `Export` (java-extractor:1079); serde lowercases (types.rs:201) | surface existing data (orient's unfiltered COUNT, agent_impl.rs:245) — **no new extraction** |
| orient says 290, stats says 0 on same snapshot | two different symbol counts: orient unfiltered (agent_impl.rs:245) vs stats export-filtered (queries.rs:1181) | unify stats onto the all-symbols count |
| fan-in/out + distance as bare numbers on incomplete import graph | metrics ride `IMPORTS` edges (queries.rs:1205-1224); `handle_stats` never reads the reliability signal (dispatch.rs:1349); the signal has 3 distinct non-HIGH reasons (rules.rs:176-198) | consume the existing `compute_import_graph_reliability` (trust/rules.rs:171) + render a **reason-specific** caveat (orient_reliability.rs:96-139 posture) |

---

## 3. Principle (what "honest" means here)

- **A measured count is Layer 0/1 extracted fact; an unpopulated/filtered-to-empty one is `unknown`,
  not `0`.** `stats` must never render a number it does not have as `0`. Where the true number IS
  available (it is — orient shows it), `stats` must show the true number, not "not measured" and
  certainly not `0`.
- **Labels describe the reader's repo, not our pipeline.** "290 symbols in your code" is reader-context;
  "0 exported symbols (because our `visibility='export'` filter is TS-centric)" is our pipeline state
  leaking out as a false fact about their code. The #6 caveat says "your import graph is incomplete on
  this index — these coupling numbers are directional", not "our resolution reliability is LOW".
- **Mirror orient's posture, don't invent a new one.** orient shows the data AND a compressed
  reliability caveat (ORIENT-DENSITY-1 §4). `stats` should do the same: keep the (directional) numbers,
  attach the marker. Consistency across the surface is itself the Protocol-Surface bar.
- **Honest layering.** The symbol count and the directory topology are Layer 0/1; the Martin ratios are
  Layer 2 interpretations over an import graph whose completeness is itself a reportable fact. No number
  is rendered above its layer; the dependency metrics carry their completeness caveat.

---

## 4. Desired output — before / after (checkable)

The IMPL is "done" when (A) `stats` on spring-petclinic shows a real symbol count that agrees with
`orient`, never `0`; and (B) `stats` on a syntax-only C/C++ repo carries an import-resolution caveat on
its dependency section. Exact label tokens are decision-dependent (§7 D4); the **invariants** are fixed.

### #5 — before / after

**BEFORE** (OBSERVED):
```
Summary
  …
  total_symbols: 0
By size
  …/owner   files=12  symbols=0
  …/vet     files=6   symbols=0
```
**AFTER** (TARGET; counts checkable against orient's 290):
```
Summary
  …
  total_symbols: 290                  ← real count; equals orient's headline
By size
  …/owner   files=12  symbols=NN      ← real per-group all-symbol count (Σ rows ≈ 290)
  …/vet     files=6   symbols=NN
```
Invariant: **no `0` is rendered for a metric whose true value is non-zero and available**; the Summary
total equals the same all-symbols count `orient` reports for the snapshot (cross-command coherence). A
genuinely empty repo still shows a true `0` — that is not a false zero.

### #6 — before / after

**BEFORE** (OBSERVED): the three dependency sections print bare numbers, no marker.

**AFTER** (TARGET; the caveat is **reason-specific**, composed from the non-HIGH reasons actually
present — §7 D3/D4 — shown only when the import-graph reliability axis ≠ HIGH). On the syntax-only
C/C++ case (LOW because `unresolved_imports_count > 0`):
```
Dependency metrics below reflect only the imports resolved on this index — 944 imports are
unresolved (e.g. external libraries / unresolved #include), so module coupling is under-counted;
treat these as directional.

By fan-in
  src/core    fan_in=3  fan_out=1
By fan-out
  …
By distance from main sequence
  …
```
The clause is selected **per reason**, so the other non-HIGH causes render different, true sentences
and the unresolved-count clause appears ONLY when the count is > 0:
- alias suspicion (LOW, count = 0): "…some import paths use aliases that may resolve to the wrong
  module, so coupling may be misattributed; treat these as directional." — **never** "0 imports unresolved".
- registry/factory wiring (MEDIUM, count = 0): "…this index has registry/factory wiring that doesn't
  appear as imports, so coupling may be under-counted through that indirection; treat these as directional."

Invariant: (i) when the import-graph reliability axis is not HIGH, the fan-in/fan-out/distance sections
carry a reader-context caveat **whose wording matches the actual reason(s)**, mirroring `orient`'s
reason-by-reason posture (`orient_reliability.rs:96-139`); (ii) the "{N} imports unresolved /
under-counted" clause renders **only when `unresolved_imports_count > 0`** — the fix must not, in
correcting one overclaim, manufacture a new false claim ("0 imports unresolved"); (iii) when the axis
is HIGH, no caveat (no noise on a fully-resolved repo).

### Coherence across commands (AFTER)

- `orient` and `stats` report the **same** repo symbol count (290) — they stop contradicting each other.
- `stats` and `orient` carry the **same** import-graph reliability disposition (consume the same
  signal) — honest in both, not one.

---

## 5. Root-cause design (the smallest change that delivers honesty)

This section describes the **recommended** path (D1=A, D2=A, D3=A, D4=(a) in §7); it becomes the design
the IMPL builds **only if the operator ratifies those cells** — until then it is advisory, not binding.
The data for both fixes **already exists** — no new extraction, no new subsystem, no new computation:

### 5.1 #5 — populate the real count (reuse orient's unfiltered count)

1. **Per-group `symbols=N` → all-symbol count.** Change the symbol CTE in `compute_module_stats`
   (queries.rs:1178-1190) to count all `nodes` kind='SYMBOL' per `file_uid` (drop the
   `visibility = 'export'` predicate; keep `kind='SYMBOL'`). `symbol_count` then = every symbol owned
   by the group, the Layer-0/1 fact agents expect from a column literally labelled "symbols".
2. **Summary `total_symbols` → the repo-level all-symbols count.** Source it from the same
   `compute_repo_summary` COUNT(*) `orient` uses (agent_impl.rs:243-250) so `stats` and `orient` show
   the identical number. (Mechanism: carry a `total_symbols` field on the stats response, set from
   `compute_repo_summary`, and have the renderer use it instead of summing rows — OR keep the
   renderer's Σ-rows and assert Σ rows == the repo count. §7 D1 picks; recommendation: the repo-level
   count, for guaranteed orient-coherence.)
3. **Propagate to the LiveGraph fastpath + cert (TS path — internal seam).** The semantic of
   `symbol_count` changes from "exports" to "all symbols", so the LiveGraph side must change in lockstep
   or the byte-parity cert goes RED on every TS repo. `livegraph_module_stats_dto`
   (livegraph_feed.rs:2406) / `lg.module_stats()` must emit the all-symbols count, and
   `compute_stats_compare_data` (livegraph_feed.rs:2447-2472) compares the new field. (This actually
   *simplifies* the LiveGraph side: all-symbols is a plain node count, no `visibility` lookup needed.)
   This is an internal-seam propagation kept coherent as ordinary design (VISION "Clarification —
   optimize for the VISION; nothing is frozen"), not an architecture-boundary change — see §8.

### 5.2 #6 — surface the existing reliability signal (reuse `compute_import_graph_reliability`)

4. **`handle_stats` reads the shared import-graph reliability axis** for the snapshot (the same
   `assemble_trust_report` / `compute_trust_overlay_for_snapshot` path orient uses) and attaches the
   axis itself — `ReliabilityAxisScore { level, reasons }` — to the stats response (one additive field
   carrying the EXISTING boundary type, not a new shape), in `stats_auto_response`
   (livegraph_feed.rs:2614) so BOTH the fastpath and the SQLite fallback carry it. Carrying `level` +
   `reasons` (rather than ad-hoc destructured fields) is what lets the renderer be reason-specific
   without re-deriving anything; the `{N}` for the wording is recovered from the
   `"unresolved_imports={N}"` reason exactly as `orient`'s `humanize_reason` recovers it
   (`orient_reliability.rs:114-120`).
5. **The renderer prints a reason-specific reader-context caveat** above the dependency sections
   (stats.rs, before "By fan-in" :173) when `level != HIGH`, composing one clause per reason present
   (each clause gated on its own trigger, so the unresolved-count clause renders only when `N > 0` and
   "0 imports unresolved" can never appear). This mirrors `orient`'s `render_degradation` gate
   (`level != "HIGH"`, `orient_reliability.rs:32-35`) and its reason-by-reason humanization (`:96-139`),
   but in reader-context dependency-metric wording (§7 D4) — describing their coupling graph, not our
   pipeline. (Reuse note: `humanize_reason` produces `orient`-context prose, not the dependency-metric
   caveat stats needs, so stats composes its own reason→clause map; a shared reason classifier is
   earned only if both renderers later want identical tokens — not introduced preemptively.)

**No new module / crate / registry / adapter / DTO layer / config surface is introduced.** The only new
data shapes are two additive response fields (`total_symbols`, the reliability axis) carried on the
existing stats response; both are justified by a concrete current caller (the renderer) and reuse
existing reads (`compute_repo_summary`, `compute_import_graph_reliability`). Smaller alternative
rejected: see §10.

---

## 6. Coordination with `RELIABILITY-REFRAME-1` (the scope boundary — ratify via D2)

The packet flags that #6 OVERLAPS the queued `RELIABILITY-REFRAME-1` (TECH-DEBT R1: reframe reliability
repo-wide as a reader-context coverage map — stop grading ourselves, exclude out-of-scope refs, compute
an in-scope rate). The boundary is **cleanly separable** because the two slices touch **different code at
a stable seam**:

- **STATS-HONESTY-1 owns SURFACING (the consumer side).** It makes `stats` *read and render* the
  import-graph reliability axis that already exists. It edits `dispatch.rs handle_stats`,
  `livegraph_feed.rs stats_auto_response`, and `stats.rs`. It does **NOT** edit
  `trust/src/rules.rs::compute_import_graph_reliability`, `trust/src/service.rs`, or
  `agent/src/aggregators/trust.rs`.
- **RELIABILITY-REFRAME-1 owns the COMPUTATION reframe (the producer side).** It changes *what the
  signal says* (in-scope rate, exclude external deps, reader-context coverage map) across the whole
  surface. It edits the trust computation. When it lands, `stats` **inherits** the improved signal for
  free because it consumes the same axis — no second edit to stats.

This is the clean seam, so **no STOP** is warranted (the packet's STOP-condition "if #6 cannot be
cleanly separated"): STATS-HONESTY-1 consumes, RELIABILITY-REFRAME-1 produces; they never edit the same
function, eliminating the "two slices editing the same reliability computation incoherently" risk.

Consequence to ratify (D2): today's signal is coarse and internal-context — three reasons
(`unresolved_imports={N}`, `alias_resolution_suspicion`, `registry_pattern_suspicion`) over a
LOW/MEDIUM/HIGH band (`rules.rs:176-198`), with no in-scope coverage rate. So STATS-HONESTY-1 authors
**reason-specific reader-context wording around the available reasons/level NOW** (D4), and the precise
"N% of your in-scope imports resolved" coverage form arrives when RELIABILITY-REFRAME-1 reframes the
underlying numbers. The stats caveat is written so that upgrade is a wording swap, not a re-architecture.

---

## 7. Decisions to surface (operator ratifies; the IMPL does NOT re-decide)

Each is an exhaustive matrix with a defensible recommendation. **None is binding yet.** Every D1–D4
recommendation below is advisory until the operator ratifies it; the IMPL executes only the cells the
operator selects, and only **after** that ratification. No ratification is recorded in this artifact.

DECISION_REQUIRED:
- ID: D1-FIVE-FIX
  QUESTION: How does `stats` stop showing the false zero for `total_symbols` / per-group `symbols`?
  OPTIONS:
  - A. POPULATE the real count from existing data (RECOMMENDED): per-group `symbols` = all `nodes`
    kind='SYMBOL' owned by the group (drop the `visibility='export'` filter, queries.rs:1181); Summary
    `total_symbols` = the repo-level all-symbols COUNT(*) orient uses (agent_impl.rs:245). Consequence:
    stats agrees with orient (290); the cross-language-broken "exported symbols" semantic is dropped;
    the per-row number changes on TS/Rust repos too (exports → all symbols); REQUIRES propagating to the
    LiveGraph stats fastpath + cert (livegraph_feed.rs:2406/2447) so byte-parity holds (the change
    *simplifies* the LiveGraph side — a node count, no visibility). Honest (Layer 0/1), coherent, reuses
    an existing query.
  - B. RENDER "not measured on this index" when the export-count is empty repo-wide: keep
    `symbol_count` = exports; when a language doesn't populate `'export'`, show "symbols: not measured"
    instead of `0`. Consequence: avoids the false zero but **HIDES a number we actually have**
    (all symbols), leaves stats incoherent with orient (orient 290 vs stats "not measured"), and needs
    a heuristic to distinguish "0 exports" from "language emits no export". Weaker per the VISION
    ("populate … if the data exists elsewhere — e.g. the same source orient uses").
  - C. RELABEL the column "exports" and show `exports=0` honestly: truthful label, but `exports=0` on
    Java is still misleading (Java symbols ARE public; the extractor just doesn't tag `export`) and is
    useless cross-language; the useful number (all symbols) — which we have — would still be absent.
    Rejected.
  RECOMMENDED: A (POPULATE). The real count is one already-implemented query; populating it is the most
    honest option AND directly serves cross-command coherence.
  SUB-CHOICE (within A) — `total_symbols` source: (a) repo-level `compute_repo_summary` COUNT(*)
    (RECOMMENDED — guarantees equality with orient's headline; add an additive `total_symbols` response
    field) vs (b) Σ of the per-group rows (no new field, but may differ from orient by symbols in
    unowned files / NULL `file_uid`). Recommend (a); the IMPL asserts Σ rows ≈ repo count and documents
    any gap (Persistence-Completeness: read path + CLI visibility).
  BLOCKING_REASON: Changes the meaning of a discovery-output number (`symbol_count`: exports → all
    symbols) and propagates across the SQLite ↔ LiveGraph cert seam (an internal seam many byte-parity
    tests rely on). Per VISION "nothing is frozen" this is allowed but must be ratified + propagated, not
    silently flipped.

- ID: D2-SIX-SCOPE
  QUESTION: Does STATS-HONESTY-1 own the stats-specific SURFACING of #6 (consuming the existing
    reliability signal), with RELIABILITY-REFRAME-1 owning the repo-wide computation reframe? Or does #6
    move entirely to RELIABILITY-REFRAME-1 (leaving STATS-HONESTY-1 = #5 only)?
  OPTIONS:
  - A. STATS-HONESTY-1 SURFACES, RELIABILITY-REFRAME-1 REFRAMES (RECOMMENDED): clean seam (§6) —
    STATS-HONESTY-1 edits only the stats consumer (dispatch/livegraph_feed/stats.rs); RELIABILITY-
    REFRAME-1 edits only the trust producer. stats inherits the reframe for free. Delivers the stats
    honesty fix NOW without waiting for the bigger reframe; no shared-function edits.
  - B. #6 → RELIABILITY-REFRAME-1 entirely: STATS-HONESTY-1 = #5 only. The stats marker lands together
    with the reframe. Consequence: fewer slices touching reliability, but stats keeps overclaiming until
    the larger slice ships, and the stats marker still has to be written somewhere — just later.
  RECOMMENDED: A. The seam is genuinely clean (consumer vs producer), so the honesty fix need not wait;
    the explicit "STATS-HONESTY-1 does not edit the reliability computation" guard removes the
    incoherent-overlap risk.
  BLOCKING_REASON: Sets which slice edits which files; the wrong call risks two slices editing the same
    reliability computation. Foundational to both slices' scopes.

- ID: D3-SIX-MECHANISM
  QUESTION: When the import graph is incompletely resolved, does `stats` ATTACH a marker to the
    dependency section (keep the numbers) or SUPPRESS the metrics — at what threshold, and scoped to
    which reason(s)?
  OPTIONS:
  - A. ATTACH a **reason-specific** reader-context caveat, keep the (directional) numbers; threshold =
    import-graph reliability axis `level != HIGH` (RECOMMENDED). Mirrors orient's posture exactly:
    orient's `render_degradation` shows the import-graph axis whenever `level != "HIGH"`
    (`orient_reliability.rs:32-35`) and humanizes the reason(s) one-by-one (`:96-139`). The caveat is
    composed per reason present (unresolved-imports / alias-suspicion / registry-suspicion — §7 D4),
    each clause gated on its own trigger, so it is honest for every non-HIGH cause and **can never
    print "0 imports unresolved"**. Reuses the existing band (`trust/rules.rs:171-199`); invents no
    new number.
  - A'. ATTACH, but gate the WHOLE dependency caveat on `unresolved_imports_count > 0` (only the
    import-completeness reason): simplest; the caveat wording is always literally true and tied to the
    metric's primary failure mode (missing import edges). COST: when the axis is non-HIGH purely
    because of alias- or registry-suspicion (count = 0), the stats dependency section shows NO caveat
    even though `orient` WOULD show an import-graph degradation line — re-creating a (milder)
    honest-in-orient-not-in-stats split, the very disease #6 names. Acceptable only if the operator
    judges the alias/registry-at-count-0 case too rare to warn on.
  - B. SUPPRESS distance/instability (and/or fan-in/out) below a resolution threshold: more aggressive;
    removes a directional signal an agent could still use; diverges from orient's show-and-caveat
    posture; needs a separate numeric threshold to invent. Rejected as the primary mechanism.
  RECOMMENDED: A (ATTACH; threshold = level != HIGH; reason-specific wording). Fully honest AND
    coherent with orient on all three reasons; A' is the smaller fallback if the operator wants the
    caveat scoped strictly to unresolved-import completeness.
  BLOCKING_REASON: Determines whether numbers disappear, what gates the marker, AND which non-HIGH
    reasons get surfaced — an output-contract behavior the IMPL must not pick unilaterally. (Both A and
    A' satisfy the hard constraint that no false "0 imports unresolved" can render; the open choice is
    the surfaced-reason scope.)

- ID: D4-WORDING
  QUESTION: Exact reader-context wording for (a) the #6 dependency caveat — one clause PER reason, since
    the axis is non-HIGH for three different reasons (D3) — and (b), only if D1=B, the "not measured" label.
  OPTIONS:
  - (a) #6 caveat — RECOMMENDED: a reader-context clause selected per reason present, joined into one
    sentence under the dependency sections; each clause is gated on its own trigger so it states only
    what is true:
      • `unresolved_imports_count > 0` (LOW) →
        "{N} imports on this index are unresolved (e.g. external libraries / unresolved #include), so
        module coupling below is under-counted; treat these as directional."   ← the ONLY clause that
        names a count, and it renders ONLY when N > 0.
      • `alias_resolution_suspicion` (LOW) →
        "some import paths on this index use aliases that may resolve to the wrong module, so coupling
        below may be misattributed; treat these as directional."
      • `registry_pattern_suspicion` (MEDIUM) →
        "this index has registry/factory wiring that does not appear as imports, so coupling below may
        be under-counted through that indirection; treat these as directional."
      All three are reader-context (their index, their imports, their coupling), honest about the
      DIRECTION of the error (under-counted vs misattributed), and mirror orient's reason-by-reason
      humanization (`orient_reliability.rs:96-139`) in dependency-metric language. When multiple reasons
      co-occur (e.g. unresolved + alias), the clauses are joined; the count clause still appears only at
      N > 0. Upgrade path: when RELIABILITY-REFRAME-1 lands the in-scope coverage rate, the unresolved
      clause swaps "{N} imports … unresolved" → "computed on {P}% of your in-scope imports" — a wording
      swap, not a re-architecture.
      Rejected wording: a single fixed "{N} imports unresolved / under-counted" for ALL non-HIGH causes
      (the prior draft) — it prints "0 imports unresolved" and claims under-counting when the cause is
      alias/registry at count 0, manufacturing a NEW false claim (review-1 #1); and
      "import-graph reliability LOW (22%)" / "unresolved_imports=944" — pipeline-state about us, not
      their code (VISION "labels speak the reader's language").
  - (b) "not measured" label (ONLY if D1=B is chosen against the recommendation): "symbols: not measured
      on this index" (a fact about their index, not "0"). Not needed under the recommended D1=A.
  RECOMMENDED: (a) the per-reason clauses above; (b) moot under D1=A.
  BLOCKING_REASON: Output-word truth is the core deliverable (VISION "labels speak the reader's
    language"); the exact tokens — and the per-reason gating that prevents a false "0 imports
    unresolved" — are operator-ratified, not IMPL-invented.

---

## 8. VISION defense (per choice)

- **"Outer layers must surface unknowns" (Product Layer Model, Dependency Rule #3).** #5 turns a false
  `0` into the true Layer-0/1 extracted count (D1=A); #6 attaches the completeness marker the layer
  model's own worked example demands ("raw counts without coverage or confidence markers are
  overclaims") — D3=A. Crucially, surfacing the unknown must not itself overclaim: the caveat is
  **reason-specific** (D4), so it states the error that is actually present (missing vs misattributed
  vs registry-invisible edges) and **never** asserts "0 imports unresolved / under-counted" for an
  alias/registry cause — a fix that, done with one fixed sentence, would replace one overclaim with
  another (review-1 #1). Neither fix renders a number above its layer.
- **"Labels speak the reader's language, not ours."** D4 phrases every #6 clause about the reader's
  import graph ("imports unresolved on this index", "coupling misattributed", "registry wiring that
  doesn't appear as imports"), never our pipeline ("reliability LOW", "unresolved_imports="). #5's fix
  shows a fact about THEIR code (290 symbols), not our `visibility='export'` filter state.
- **Honest layering / Fact-Certainty.** A measured symbol count is Layer 0/1 (state plainly); an
  export-filtered-to-empty count is `unknown`, never `0`. The Martin ratios are Layer-2 interpretations
  over an import graph whose completeness is a reportable fact — so they carry the caveat.
- **Cross-command coherence (Protocol-Surface Standard, Layer 2).** After this slice, `orient` and
  `stats` agree on the symbol count and on the import-graph reliability disposition — an agent learns
  one consistent truth from the surface, not 290-vs-0.
- **"Nothing is frozen; optimize for the VISION" (D1's seam change).** Changing `symbol_count` from
  exports to all-symbols is a discovery-output improvement; the cost is load-bearing assumptions
  disturbed — here the SQLite↔LiveGraph byte-parity cert, which we propagate to in lockstep (§5.1 step 3). The
  discovery output's consumers are our own shims + agent-instruction docs; we update them as maintenance.
  This is surfaced (D1) before the change, scaled to its blast radius (an internal seam, not a
  governance/gate object) — exactly the VISION's ratified discipline.
- **Smallest design (CLAUDE.md decision criteria).** The only new structure is two additive response
  fields consumed by one concrete caller (the renderer); both reuse existing reads. No abstraction is
  introduced for imagined variation (§10).

---

## 9. Validation plan — evidence the IMPL must PRODUCE (not yet run)

This section is an **obligation list for the IMPL slice**, not a record of executed checks: nothing
here has been run (this is a design doc — per the packet, "NO cargo / no code"). Each item names the
check and the evidence label the IMPL must attach AFTER implementation. Per
`docs/testing/end-of-slice-procedure.md` and the isolated dogfood (never index into the operator's
real registry):

1. **#5 real count, coherent with orient (spring-petclinic, isolated).** `rmap stats` Summary shows
   `total_symbols: 290` (== `rmap orient` headline on the same snapshot), and per-group `symbols=NN`
   are non-zero and sum to ≈ 290. Capture both; assert no `0` where symbols exist.
   → IMPL must produce EXECUTED evidence.
2. **#5 true-zero preserved.** A genuinely empty fixture still shows `total_symbols: 0` (true zero is
   not a false zero). → IMPL must produce EXECUTED evidence on a fixture.
3. **#6 caveat on incomplete import graph (unresolved-imports reason).** `rmap stats` on a syntax-only
   C/C++ repo (no `compile_commands.json`) shows the unresolved-imports clause (D4(a), `{N}` > 0) above
   the fan-in/out/distance sections, and `rmap orient` on the same snapshot shows the matching
   import-graph reliability — honest in both. → IMPL must produce EXECUTED evidence.
4. **#6 reason-specific wording, no false "0 imports unresolved" (review-1 #1 guard).** On a non-HIGH
   case whose cause is alias- or registry-suspicion with `unresolved_imports_count = 0` (a fixture with
   the trust inputs set so), `rmap stats` shows the alias/registry clause (or, under ratified D3=A',
   NO dependency caveat) and **never** the string "0 imports unresolved" nor an under-counting claim.
   → IMPL must produce EXECUTED evidence (this is the specific regression the revision exists to prevent).
5. **#6 no caveat when HIGH.** On a fully-resolved repo (import-graph reliability HIGH) `stats` shows the
   dependency sections with NO caveat (no noise). → IMPL must produce EXECUTED evidence.
6. **LiveGraph parity after the #5 seam change (TS repo).** On a GREEN TS repo, `rmap stats`
   (`--engine compare`) is field-exact and human byte-identical between SQLite and LiveGraph with the new
   all-symbols `symbol_count`; the cert stays GREEN; `--engine sqlite` and the default agree.
   → IMPL must produce EXECUTED evidence.
7. **No reliability-computation edit (scope guard, D2=A).** `git diff` touches only the stats consumer
   (dispatch.rs/livegraph_feed.rs/stats.rs + queries.rs symbol CTE + the response DTO); it does NOT
   touch `trust/src/rules.rs`, `trust/src/service.rs`, or `agent/src/aggregators/trust.rs`.
   → IMPL must produce OBSERVED evidence (`git diff`).
8. **Contracts / gates.** `cargo build/fmt/clippy -D warnings/test` green in `rust/`; the smoke protocol
   (`docs/testing/rmap-test-protocol.md`) + the isolated `./scripts/dogfood-isolated.sh`. Existing stats
   tests asserting export-based `symbol_count` are updated to the all-symbols semantic (enumerate them in
   the IMPL); the JSON additive fields (`total_symbols`, the reliability axis) are stripped/optional in
   human output per the imports/cycles precedent. → IMPL must produce EXECUTED evidence.

---

## 10. Smallest-design statement & STOP-condition assessment

- **Smallest design.** The recommended path (D1=A, D2=A, D3=A, D4=(a)) introduces **no new module,
  crate, registry, adapter, DTO layer, or config surface**. It reuses: orient's unfiltered symbol
  COUNT(*) (`compute_repo_summary`, agent_impl.rs:245), the existing `compute_module_stats` query (one
  predicate dropped), the existing `compute_import_graph_reliability` signal (trust/rules.rs:171), and
  the existing stats response/renderer. The two new additive response fields each have one concrete
  current caller (the renderer) and an axis of reuse (orient's count; orient's reliability axis) — they
  are earned, not speculative. Simpler alternative rejected: "leave `symbol_count` as exports and only
  relabel" (D1=C) — keeps the useful all-symbols number we already have off the surface and stays
  incoherent with orient; "compute a stats-local reliability" — duplicates the trust function and risks
  the exact incoherence D2 guards against. The reason-specific caveat (D4) is earned by **demonstrated**
  variation — the three distinct reason sets `compute_import_graph_reliability` already returns
  (`rules.rs:176-198`), not imagined variation — and introduces no abstraction: it is a `match`/`if`
  over the `reasons` the axis already carries, in the stats renderer, exactly as `orient`'s
  `humanize_reason` already does. A single fixed sentence is rejected not for elegance but because it is
  provably false on two of the three reasons (the false "0 imports unresolved").
- **STOP-condition assessment (packet).**
  - "If fixing #5 requires NEW extraction/computation on the rmap path (not just surfacing existing
    data) → STOP + DECISION_REQUIRED." → **NOT triggered.** The real count is an already-implemented
    `COUNT(*)` query orient reads (agent_impl.rs:245); the per-group recount is a query-level
    aggregation over existing `nodes` rows. No extractor change. (The export→all-symbols *semantic*
    change + its LiveGraph-cert propagation is surfaced as D1, an output/seam decision, not a hidden
    extraction.)
  - "If #6 cannot be cleanly separated from RELIABILITY-REFRAME-1's repo-wide reframe → STOP +
    DECISION_REQUIRED on the scope boundary." → **Cleanly separable** (§6: consumer vs producer, disjoint
    files). No hard stop; the boundary is surfaced as D2 with the recommendation and the explicit
    no-shared-edit guard (validated in §9.6).

---

## 11. Out of scope

- The repo-wide reliability **reframe** (in-scope rate, exclude out-of-scope refs, coverage map) —
  `RELIABILITY-REFRAME-1` (TECH-DEBT R1). STATS-HONESTY-1 only *consumes* the existing signal.
- Making the Java/C/C++ extractors emit `Visibility::Export`, or a per-language public-surface metric —
  not needed (the all-symbols count is the honest, available number); a public-surface metric is a
  future enhancement with its own consumer, not this slice.
- `abstractness`'s own classification partialness on non-TS (subtype/`parent` coverage) — the #6 caveat
  honestly covers the import-graph-dependent metrics (fan-in/out, instability, distance); deeper
  per-language symbol-classification fidelity is a separate measurement-correctness concern.
- The MODULE-MODEL-1 topology/notion work (#3/#4) — shipped/ratified separately; this slice builds on
  its renderer.
- Any production code in THIS slice (design only; a later IMPL executes it).
- `docs/ROADMAP.md` / `docs/TECH-DEBT.md` / `docs/VISION.md` / `CURRENT_SLICE.md` and the other queued
  slices (RELIABILITY-REFRAME-1 etc.) — edits out of scope per the selection packet; coordination is via
  the D2 boundary only.
