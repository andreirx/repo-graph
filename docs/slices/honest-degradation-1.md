# HONEST-DEGRADATION-1: one honest-degradation contract across every relationship surface — SPEC

Slice: HONEST-DEGRADATION-1
Status: **SPEC** (design; awaiting-ratification → the IMPL is the relay slice that follows). No
operator ratification is recorded in this artifact; every recommendation below is **advisory**
until ratified (no §-ratification block, following `stats-honesty-1.md`'s pattern, unlike
`module-model-1.md` §12).
Track: Product-surface honesty (`docs/ROADMAP.md` → Current priority). **PROMOTED ahead of capability
(ENRICH-LIFECYCLE-1)** by the second End-to-End Usefulness Protocol run's **two-agent gate**
(analyst + Codex, 2026-06-29). Codex verdict: the honesty lapses are **trust-contract violations,
not polish** — fix the honest-degradation contract across every surface before adding more
relationship capability.
Subsumes: **`docs/slices/stats-honesty-1.md` (STATS-HONESTY-1)** — that stats-specific SPEC (D1–D4
pending) becomes **surface #1** of this cross-surface contract. Its decisions are folded in here by
reference (this doc's D1/D4), not duplicated and not contradicted (§6). One genuine **correction** to
its root-cause framing is recorded (§6.3).
Pairs / consolidates: **TECH-DEBT § Checkpoint Smoke (2026-06-29) C1–C4** — C1 (`stats` resolution-
zeros), C2 (`deps` wrong-ecosystem), C3 (`orient` certainty collapse), C4 (cross-surface symbol-count
mismatch) — as ONE honest-degradation contract. (C5 orient-budget bimodality → separate orient-density
slice; C6 `enrich` ergonomics → ENRICH-LIFECYCLE-1; C7 smoke-harness → tooling. All out of scope here.)
Grounded in: the nginx (C) smoke capture `smoke-runs/2026-06-29T12-42-18Z/`, confirmed first-hand
against the actual code (cited by path:line below).
Model: this doc follows `docs/slices/stats-honesty-1.md` and `docs/slices/module-model-1.md`
(problem+evidence → positive model → root cause confirmed against code → contract/principle → shared
mechanism + earned-abstraction ledger → cross-slice reconciliation → decisions-as-matrices → per-choice
VISION defense → validation obligations → smallest-design + STOP assessment).

---

## 0. Evidence law (how to read the claims below)

- `OBSERVED` — read first-hand from a smoke capture artifact (cited by file) or from source (cited by
  path:line) during this authoring.
- `INFERRED` — concluded from cited code, not directly executed; or stated by the selection packet /
  TECH-DEBT and corroborated by code but not present in the capture set.
- `NOT RUN` — no code is run in this slice (SPEC only). The IMPL re-validates every INFERRED claim via
  live isolated `rmap` capture (§9).

Source line numbers are against the working tree at spec time; the IMPL confirms before editing (lines
drift). Completeness of any "no other caller" / "all surfaces" claim below is bounded by the
deterministic reads cited — it is a verified subset, not an embeddings guess; where a claim rests on a
single grep or a sub-agent read-trace it is marked.

---

## 1. The contract (what this slice ratifies)

> **The honest-degradation contract.** Any surface that emits a **relationship-derived fact** — module
> coupling (fan-in/out, instability, distance-from-main-sequence), dependency/ecosystem results, call or
> dead-code claims, change-impact, certainty/zero counts derived from the resolved graph — MUST render
> the **reliability/trust posture inline**, in the reader's language. Unresolved or unsupported data is
> labeled **unknown / low-confidence**, **never** rendered as **known-zero or exact fact**. Honesty is
> **uniform** across the surface: a posture that `trust`/`check` already state must not be silently
> dropped (`stats`, `deps`) or collapsed (`orient` footer) on another surface describing the same snapshot.

Three precise clauses (each maps to surfaces audited in §3):

- **(a) Relationship-derived facts carry their reliability posture inline.** A coupling number, a
  dependency result, a call/dead claim derived from the resolved graph is a **Layer-2 interpretation
  over a graph whose completeness is itself a reportable fact** (VISION Product Layer Model; Layer Rules
  #2/#3 — *never describe Layer-3/2 as Layer-0; outer layers must surface unknowns*). It must show the
  posture beside the number, reason-specific, the way `trust`/`check` already do. *(C1 stats, C2 deps.)*

- **(b) `null` = unknown, never `0`/`1.00`; and a label means the same thing on every surface.** A metric
  whose value is a pure artifact of resolution failure (a `fan_in=0` produced by an unresolved import
  graph; a `0/0` instability) is **unknown**, not **known-zero** (`agent_docs/architecture.md` Mandatory
  Rule #6 — *"`null` = unknown, empty = known-zero. Never conflate."*). And no two surfaces may use the
  **same word** for **different quantities** without a self-label (VISION Protocol-Surface Standard,
  Layer 2 — *an agent must learn one consistent truth from the output*). *(C1 stats, C4 symbol-count.)*

- **(c) Freshness/serving-status is not semantic certainty.** A snapshot-freshness or answer-serving
  signal (`fresh`, answer-class `exact`) must not be **rendered as, or labeled as, global confidence**.
  The VISION's *Three Version Classes* separate **provenance/freshness** ("can I compare these
  snapshots?") from the **reliability of the answer's content**; collapsing them under a word like
  "Certainty" mislabels one as the other. *(C3 orient footer.)*

This is the VISION's own *"labels speak the reader's language, not ours"* and *Fact-Certainty Model*
made **uniform**: today `trust` and `check` honor it; `stats`, `deps`, and `orient`'s footer do not.

---

## 2. The positive model — what `trust` and `check` already do right (OBSERVED)

The contract is not invented here; two surfaces already satisfy it on the **same nginx snapshot**. The
others must match them.

`trust` (`nginx-trust.txt`) — every relationship axis carries level + reason + a safety caveat, inline:
```
Reliability  (sqlite, snapshot-scoped extraction, Fresh)
  - Call-graph: LOW (42% call resolution, below 50% threshold)
  - Import-graph: LOW (alias resolution suspected; 1090 unresolved imports)
  - Change-impact: LOW (alias resolution suspected)
Suspicious Modules (zero connectivity)  (sqlite, snapshot-scoped extraction, Fresh)
  - src/core … src/event … src/http … src/mail … src/os … src/stream     ← 6 modules
Caveats
  - Call-graph reliability is LOW on this repo. Do not use callers/callees for safety-critical
    decisions without verification.
  - Import-graph reliability is LOW. Module fan-in/fan-out and change-impact propagation may
    undercount relationships.
```
`check` (`nginx-check---full.txt`) — reliability is a **first-class, verdict-affecting condition**, not a
footnote:
```
Verdict: FAIL@Fresh
Failing conditions
  - CALL_GRAPH_RELIABILITY: Call graph reliability is LOW.
```

**The positive-model discipline (what the other surfaces must adopt):** reliability is *inline*,
*reason-bearing*, *reader-context*, and *safety-caveated*; an unresolved import graph is stated as such,
not silently folded into a `0`. Note the cruel irony the gate caught: `trust` **explicitly names the 6
zero-connectivity modules and warns "fan-in/fan-out … may undercount"** — and on the very same snapshot,
`stats` prints those modules' fan-in/fan-out as a clean `0` with no caveat (§3, C1). The signal exists;
two surfaces honor it; three do not.

---

## 3. Surface audit — the four lapses (OBSERVED evidence + code root cause)

All four describe the **same nginx snapshot** as the §2 captures. Root causes were read first-hand
during this authoring.

### C1 — `stats` renders resolution-derived zeros as measured architectural fact (P1; Codex: "the clearest VISION violation")

**OBSERVED (`nginx-stats.txt`):** every package group shows `fan_in=0 fan_out=0`, and the Martin
"distance from main sequence" section shows `A=1.00 D=0.00 I=0.00` for nearly every module (e.g.
`src/core D=0.00 I=0.00 A=1.00`), with **no caveat** — while `trust` on the same snapshot reports
import-graph reliability **LOW** and flags **6 zero-connectivity modules**. Resolution failure is
presented as **measured architectural absence**.
```
By fan-in
  src/core  fan_in=0  fan_out=0          ← all 15 groups identical
By distance from main sequence
  src/core  D=0.00  I=0.00  A=1.00       ← "maximally abstract & stable" — false for nginx C core
  src/event/quic/bpf  D=1.00  I=0.00  A=0.00
```

**Root cause (confirmed against code):**
1. The metrics ride the resolved IMPORTS graph. `fan_in`/`fan_out` are distinct MODULE↔MODULE `IMPORTS`
   edge counts; `instability = fan_out/(fan_in+fan_out)`; `distance = |A + I − 1|`
   (`storage/src/queries.rs`, the fan-in/out CTEs + Martin metrics — same query family stats-honesty-1
   cites at `:1205-1224`/`:1278-1279`). On nginx the import graph is **highly unresolved** (`trust`:
   *1090 IMPORTS file-not-found*; `deps`: *56 external imports* unattributed — C `#include` is
   syntax-only, no `compile_commands.json` per TECH-DEBT §Extraction—C/C++), so every cross-module edge
   is missing → `fan_in=fan_out=0` for all groups.
2. `handle_stats` **never reads the reliability posture.** [OBSERVED via sub-agent read-trace of
   `daemon-runtime/src/dispatch.rs handle_stats` and `livegraph_feed.rs` `serve_stats_fastpath`
   :2927-2935: the stats response JSON carries no trust/reliability field.] `trust` knows the import
   graph is LOW; `stats` is built on a path that doesn't consult it.
3. Two distinct degeneracies hide in the "distance" line: (i) `instability = 0/0` when
   `fan_in+fan_out=0` — **mathematically undefined**, rendered as `I=0.00`; (ii) `A=1.00` is the
   **abstractness** metric degenerating on a non-OO language (C has no class/interface abstract-type
   denominator) — a *classification* artifact distinct from the import graph. The reader sees
   `D=0.00` ("on the main sequence") synthesized from two artifacts.

This extends TECH-DEBT #6 (stats reliability marker) and is the nginx face of it: where spring-petclinic
showed a *partial* coupling graph (directional numbers), nginx shows a *degenerate* one (all-zero,
undefined) — a sharper instance of the same overclaim.

### C2 — `deps list` labels a C repo `ecosystem: npm`, implying an npm graph was evaluated (P2)

**OBSERVED (`nginx-deps-list.txt`):**
```json
{ "command":"deps list", "results":[], "count":0,
  "ecosystem":"npm", "total_external_imports":56, "modules_without_manifest_context":0 }
```
nginx is C. `ecosystem:"npm"` + `count:0` reads as **"the npm dependency graph was evaluated and is
empty"** — false on two counts (there is no npm graph; the 56 real external includes are unattributed).

**Root cause (confirmed against code):**
- `ecosystem` is a **hardcoded default, not detected.** [OBSERVED `dispatch.rs handle_deps_list`
  :4956-4958: `ecosystem = get_optional_string_param("ecosystem").unwrap_or("npm")`.] It is **not**
  derived from the repo's language or any detected manifest; absent a caller override it is always
  `"npm"`. Runtime-builtin selection then branches only `"cargo" | _ => npm` (:4990-4993) — there is no
  C/C++ arm and **no C manifest reader exists** (render side of TECH-DEBT R3).
- `total_external_imports:56` is **real, already-counted data**: `external_imports.len()` over all
  external-import edges for the snapshot [OBSERVED `module-queries/src/deps/compose.rs:78-79`]. It is the
  raw count of nginx's external `#include`s — honest if surfaced as "observed, unattributed," dishonest
  when sitting under an `ecosystem:"npm"` that implies package resolution happened.

### C3 — `orient` footer `Certainty: class exact, freshness fresh` collapses freshness with semantic certainty (P2; found by the Codex pass)

**OBSERVED (`nginx-orient---full.txt`):** the very same `orient --full` answer contains, above the
footer, every signal that contradicts "certainty":
```
ln 3:   nginx · 397 files, 3977 symbols · 15 package groups … · 6 inferred modules
ln 6:   Reliability: call-graph 42% resolved (LOW) — verify call/dead claims against source.
ln 10:  - No current-state LiveGraph was available … LiveGraph-first signals fell back to the SQLite primary.
ln 381: Degradation
ln 382:   - Call-graph reliability is LOW (42% call resolution, below 50% threshold)
ln 383:   - Import-graph reliability is LOW (alias resolution suspected; 1090 unresolved imports)
ln 384:   - Change-impact reliability is LOW (alias resolution suspected)
ln 389: Certainty
ln 390:   - class exact, freshness fresh                       ← reads as GLOBAL factual certainty
```
The answer carries **inferred** modules (the "6 inferred modules" headline; inference confidence 0.7 —
the stored Layer-2 inferred-module confidence, per packet [INFERRED; the capture shows "inferred", not
the numeral]), **LOW** call/import/change reliability, and a **LiveGraph-unavailable / fell-back-to-
SQLite** note — yet the footer reads "exact / fresh."

**Root cause (confirmed against code):**
- The footer is rendered from the `CoherenceEnvelope<T>` wrapper: [OBSERVED `rgr/src/presentation/orient.rs`
  `render_orient_envelope` :130-166: `class = format!("{:?}", env.trust.class).to_lowercase()`,
  `freshness = format!("{:?}", env.freshness)…`.] `env.trust.class` is an **`AnswerClass`** and
  `env.freshness` a **`FreshnessState`** (`repo-graph-coherence/src/lib.rs` `TrustPosture` :221-234,
  `CoherenceEnvelope` :466-475), computed by the MEET fold over the **snapshot-served leaves**
  (:319-421).
- **`AnswerClass::Exact` means "this leaf was served exactly from the snapshot," NOT "the call graph is
  exact."** The per-axis reliability (call/import LOW) rides **separately** on `trust_briefing.reliability`
  (orient.rs :268-286), computed independently of the footer. So the value is *literally true about
  snapshot serving + freshness* but is **mislabeled** by the header word "Certainty" into a claim about
  the answer's content — a **name-vs-semantics defect** (the identifier `Exact` encodes a serving fact;
  rendered under "Certainty" it reads as semantic exactness; clause (c)).

### C4 — the same snapshot yields three different "symbol"/"node" numbers, two of them both labeled "symbols" (P2)

**OBSERVED:** `orient` headline **"3977 symbols / 397 files"** (`nginx-orient---full.txt:3`); `stats`
**"total_symbols: 1816 / total_files: 396"** (`nginx-stats.txt:8-9`). `index` reportedly **"4393 nodes /
396 files"** [INFERRED — per packet/TECH-DEBT C4; no `index` capture exists in this smoke set;
corroborated by code below]. Two commands print the **identical word "symbols"** for **3977 ≠ 1816**;
the reader cannot tell which to trust.

**Root cause (confirmed against code) — three genuinely different quantities, none self-labeled:**
| Surface | Number | Computation (cited) | What it actually counts |
|---|---|---|---|
| `orient` | 3977 symbols | `SELECT COUNT(*) FROM nodes WHERE kind='SYMBOL'` (`storage/src/agent_impl.rs:267-274`) | **all** SYMBOL nodes (unfiltered) |
| `stats` | 1816 symbols | `SUM(CASE WHEN n.visibility='export' …)` per module (`storage/src/queries.rs:1280-1304`) | **export-visibility** symbols, module-owned only |
| `index` | 4393 nodes | `nodes_total = COUNT(*) FROM nodes` all kinds (`storage/src/crud/snapshots.rs:168-178`) | **every** node kind (SYMBOL + FILE + MODULE …) |
| files 397 vs 396 | `orient` `COUNT(DISTINCT file_uid) FROM file_versions` (all files) vs `stats` files **owned by a MODULE** (`queries.rs` OWNS-edge filter `files.cnt>0`) | all-indexed vs module-owned |

The `stats` 1816 is the **export filter** stats-honesty-1 diagnosed (TECH-DEBT #5) — but see §6.3: on C
it is a **silent undercount**, not a zero.

---

## 4. The shared mechanism — thread the EXISTING posture into the silent surfaces

The honesty signal already exists as a **reusable snapshot-level value**; the three lapsing surfaces
simply don't consume it. The fix is **consumption + reader-context rendering**, not a new computation
and not a new boundary.

### 4.1 The existing posture (verified)

- **Per-axis score:** `ReliabilityAxisScore { level: ReliabilityLevel /*HIGH|MEDIUM|LOW*/, reasons:
  Vec<String> }` — crate `repo-graph-trust` (`trust/src/types.rs:74-87`), computed by
  `compute_import_graph_reliability` / `compute_call_graph_reliability` / `…_change_impact` /
  `…_dead_code` (`trust/src/rules.rs:171-300`), publicly re-exported (`trust/src/lib.rs:160-165`).
- **Assembled once per snapshot:** `TrustReliability { import_graph, call_graph, change_impact,
  dead_code }` inside `TrustReport`, projected to `TrustOverlaySummary { reliability, degradation_flags,
  caveats }` (`trust/src/types.rs:135-143`, `trust/src/overlay.rs:27-46`). It is **not recomputed per
  surface.**
- **The reusable entry point:** `compute_trust_overlay_for_snapshot(&storage, repo_uid, &snapshot,
  "CALLS+IMPORTS") -> TrustOverlaySummary` (`daemon-runtime/src/util/trust.rs:11-35`). **`orient` already
  calls it** via `compute_trust_briefing` (`daemon-runtime/src/orient_coherence.rs:226-240`), gated on
  `has_degradation() || !caveats.is_empty()`. `handle_trust` builds the full report
  (`dispatch.rs:3549+`).

### 4.2 Feasibility — no new cross-crate boundary (STOP-condition resolution)

[OBSERVED `daemon-runtime/Cargo.toml:44`: `repo-graph-trust = { path = "../trust" }`.] `handle_stats`,
`handle_deps_list`, `handle_orient`, and `handle_trust` **all live in `daemon-runtime`**, which **already
depends on `repo-graph-trust`**. Threading the existing `TrustOverlaySummary` into the stats and deps
responses, and into orient's footer, requires **zero new crate dependency edges** — it reuses the same
edge `handle_trust`/`handle_orient` already use. **The packet's first STOP condition is NOT triggered**
(no new boundary; the shared posture already exists as a reusable value).

### 4.3 Earned-abstraction ledger

> **Abstraction (pre-existing, NOT introduced here):** the snapshot reliability posture —
> `TrustOverlaySummary` (reliability axes + caveats) from `compute_trust_overlay_for_snapshot`
> (`daemon-runtime/src/util/trust.rs`; crate `repo-graph-trust`).
> **Concrete current callers:** `handle_trust` (full report), `handle_orient` (degraded-state briefing).
> **Callers this slice adds:** `handle_stats` (C1 dependency-section caveat + unknown-coupling),
> `handle_deps_list` (C2 ecosystem/attribution honesty), and orient's posture-footer (C3 — it already
> holds the briefing; it renders it in the footer instead of only the separate `trust_briefing` JSON).
> **Named axis of variation:** per-surface **rendering** of ONE shared snapshot posture.
> **Rejected simpler alternative:** per-surface ad-hoc reliability re-derivation (each command computing
> its own caveat from raw counts) — rejected because it **reproduces the exact cross-surface incoherence
> this slice exists to remove** (honest in `trust`, silent in `stats`/`deps`), and would risk two
> surfaces disagreeing about the same snapshot's reliability.
> **New structure introduced:** none — no crate, module, adapter, DTO layer, registry, or config surface;
> only additive response fields carrying the **existing** `TrustOverlaySummary` type, each with one
> concrete current renderer-caller.

### 4.4 No underlying number changes (second STOP-condition resolution)

Every proposed fix changes **rendering/labeling** or **surfaces already-counted data** — none recomputes
resolution:
- C1: attach the existing posture + render undefined coupling as `unknown` (a divide-by-zero/`null`
  guard over **existing** edge counts).
- C2: relabel `ecosystem` from a hardcoded default to the detected state + surface the **already-counted**
  56 external imports honestly.
- C3: re-compose/relabel **values orient already computes** (the briefing + freshness).
- C4: count **existing** `nodes` rows differently (drop the export filter — the same already-stored rows)
  + label the all-kinds node count. No extractor, enrichment, or resolver runs.
**The packet's second STOP condition is NOT triggered.**

---

## 5. Cross-surface uniformity — one posture, four renderings

Because the posture is one shared value (§4), the four renderings stay coherent by construction:

| Surface | Today (OBSERVED) | After (target) | Posture source |
|---|---|---|---|
| `trust`/`check` | inline level+reason+caveat (the model) | unchanged (the model) | `TrustOverlaySummary` (already) |
| `stats` | `fan_in=0`, `A=1.00`, no caveat | reason-specific caveat; undefined coupling → `unknown` not `0`/`1.00` (D1) | consumes same `TrustOverlaySummary` |
| `deps` | `ecosystem:"npm", count:0` on C | detected/none ecosystem + "56 external includes observed, unattributed (no C manifest reader)" (D2) | language/manifest detection + raw count |
| `orient` footer | `Certainty: exact/fresh` | posture block: freshness + answer-class scoped to serving; reliability/module-status/LiveGraph-status legible together (D3) | the briefing orient already holds |
| symbol counts | 3977 / 1816 / 4393, two say "symbols" | one canonical "symbols" (stats == orient); "nodes" self-labeled all-kinds (D4) | query unification |
| LOW next-action | (absent) | toolchain-aware honest line: enrich where a resolver exists; "no resolution path for C" where not (D5) | resolver-availability by language |

---

## 6. Reconciliation with STATS-HONESTY-1 (this slice subsumes it as surface #1)

### 6.1 Scope fold — no duplication, no contradiction
- **C1 = stats-honesty-1 #6** (fan-in/out reliability marker) on nginx. This doc's **D1** *is*
  stats-honesty-1 **D3** (attach vs suppress), **extended** for the all-zero degeneracy nginx exposes
  (§6.2). stats-honesty-1 **D2** (the SURFACING-vs-REFRAME seam with `RELIABILITY-REFRAME-1`) is adopted
  unchanged: HONEST-DEGRADATION-1 owns the **consumer** side (render the existing axis); it does **not**
  edit `trust/src/rules.rs` / `trust/src/service.rs` / `agent/src/aggregators/trust.rs`.
- **C4 ⊃ stats-honesty-1 #5** (the export filter). This doc's **D4** *includes* stats-honesty-1 **D1**
  (drop the export filter so stats' symbol count == orient's) and its sub-choice (source `total_symbols`
  from the repo-level `compute_repo_summary` COUNT for guaranteed orient-coherence), and **adds** the
  cross-command "symbols" vs "nodes" labeling that C4 newly surfaces.
- **D4 wording** reuses stats-honesty-1 **D4(a)** reason-specific clauses verbatim where applicable.
- stats-honesty-1 §11 *Out of scope* (abstractness's non-OO classification partialness) is preserved
  here as a **named deferred item** (§11), not silently pulled in.

### 6.2 Extension — the all-zero degeneracy nginx exposes (not present in spring-petclinic)
stats-honesty-1 D3 faced a **partial** coupling graph (spring-petclinic `fan_in=3/fan_out=1` —
directional numbers; "attach a caveat, keep the numbers" is right there). nginx shows a **degenerate**
graph (`fan_in=fan_out=0` for *every* group; `instability=0/0` undefined). "Attach only" would leave
`fan_in=0` standing — which is precisely **unknown rendered as known-zero** (clause (b),
`agent_docs/architecture.md` #6). So D1 unifies both cases: **always attach** the reason-specific caveat
(level ≠ HIGH), **and additionally** render a coupling metric as `unknown` when its value is a pure
artifact of non-resolution (the `fan_in+fan_out=0` ⇒ undefined-instability/-distance case). On
spring-petclinic the numbers are non-degenerate → they stay (annotated, exactly as stats-honesty-1
intended); on nginx they are degenerate → `unknown` (annotated). This **extends, does not contradict**,
stats-honesty-1.

### 6.3 Correction to stats-honesty-1's root-cause framing (recorded honestly)
stats-honesty-1 §2a states the `visibility='export'` filter yields **0 on all non-TS** ("Java/C/C++ get
`'export'`-match = 0"). **OBSERVED contradiction:** nginx (C) `stats total_symbols: 1816` is **non-zero**.
**Verified mechanism:** the C extractor maps **non-`static` functions → `Visibility::Export`**, `static →
Private` (`c-extractor/src/extractor.rs:408-412`). So on C the export filter is a **silent undercount**
(externally-linked functions only — 1816, ~46% of orient's 3977), **not a zero**. The "→0 on all non-TS"
claim holds for **Java** (extractor emits `Public`, never `Export`) but **not for C**. Same root cause
(the TS-centric `export` notion of "public surface"), **two symptoms**: false-**zero** on Java,
silent-**undercount** on C. The recommended fix is unchanged — **drop the filter, count all SYMBOL nodes**
(D4) — which resolves **both** symptoms and unifies stats onto orient's count. (This correction touches
only the root-cause *description*; it does not alter stats-honesty-1's recommended cell.)

---

## 7. Decisions to surface — operator ratifies (the IMPL does NOT re-decide)

Each is an exhaustive matrix with a defensible recommended cell + blocking reason. **None is binding
yet.** The IMPL executes only the cells the operator selects, and only after ratification.

DECISION_REQUIRED:
- ID: D1-STATS-LOW-RESOLUTION
  QUESTION: When the import-graph posture is non-HIGH, how does `stats` render the dependency-derived
    metrics (fan-in/out, instability, distance) — annotate, suppress, or both — and at what trigger?
  OPTIONS:
  - A. ANNOTATE only: attach a reason-specific reader-context caveat above the dependency sections when
    the import-graph axis `level != HIGH` (the stats-honesty-1 D3=A posture), keep the numbers.
    Consequence: honest for a *partial* graph (directional numbers), but on nginx leaves `fan_in=0` /
    `A=1.00` standing under the caveat — an `unknown` still rendered as `known-zero` (clause (b)
    violated for the degenerate case).
  - B. SUPPRESS only: omit fan-in/out + distance entirely below a resolution threshold. Consequence:
    removes a directional signal an agent could use on partial graphs; needs a new numeric threshold;
    diverges from the positive model's show-and-caveat posture.
  - C. **BOTH (RECOMMENDED):** (i) **always** attach the reason-specific caveat when `level != HIGH`
    (subsumes A / stats-honesty-1 D3); **and** (ii) render a metric as **`unknown` / `—`** (not `0` or
    `1.00`) when its value is a pure artifact of non-resolution — concretely, when `fan_in+fan_out=0`
    the module's instability is `0/0` (mathematically undefined) and its distance is not computable, so
    show `unknown`. On a non-degenerate (partial) graph the numbers stay, annotated (the spring-petclinic
    behavior). Consequence: satisfies clause (a) *and* clause (b) on both the partial and the degenerate
    case; the `unknown` guard is a small divide-by-zero correctness fix that is arguably always-correct
    (even on a HIGH repo, a no-import leaf has undefined instability).
  RECOMMENDED: C (BOTH). The all-zero nginx case proves annotate-only (A) still prints `unknown` as `0`;
    suppress-only (B) over-hides partial signal. BOTH is the only cell honest on both graph shapes.
  SUB-CHOICE (within C) — the `A=1.00` abstractness degeneracy on non-OO languages is a **distinct**
    (classification, not import-graph) artifact that stats-honesty-1 §11 scoped OUT. Cells: (a) keep that
    scope-out — the import-graph caveat covers fan-in/out + instability + distance's `I` term, and the
    whole "distance" line carries the caveat so a reader won't act on `D`; the `A` non-OO degeneracy is a
    **named deferred** measurement-fidelity item (RECOMMENDED — smallest, matches the subsumed spec); vs
    (b) fold an abstractness-applicability marker into this slice (larger; pulls a second root cause in).
  BLOCKING_REASON: Determines whether numbers disappear, what gates the marker, and whether a `0` can
    stand for `unknown` — an output-contract behavior the IMPL must not pick unilaterally; and it sets
    whether the abstractness non-OO issue is in or out of this slice's scope.

- ID: D2-DEPS-ECOSYSTEM
  QUESTION: How does `deps list` stop mislabeling a C repo `ecosystem:"npm"`, and does it surface the 56
    observed-but-unattributed external includes honestly?
  OPTIONS:
  - A. **DETECT + HONEST-SURFACE (RECOMMENDED):** derive `ecosystem` from the detected manifest/language
    instead of the hardcoded `"npm"` default; when no dependency-manifest reader exists for the repo's
    language, render `ecosystem: "none-detected"` (or the language token, e.g. `"c"`) **plus** a
    reader-context note: *"no dependency-manifest reader for C on this build; 56 external includes
    observed, not attributed to packages."* Consequence: kills the false "npm graph evaluated" implication
    and turns the already-counted 56 into a usable reader-context fact. The C **manifest reader itself is
    a separate capability slice** (changes underlying attribution — out of scope here; this is the LABEL +
    honest surfacing only).
  - B. NULL the field: `ecosystem: null` when undetected. Consequence: honest (no false "npm"), but loses
    the *why* and does not surface the 56 — weaker reader-context than A.
  - C. SUPPRESS `deps list` on unsupported languages. Consequence: too aggressive — hides the real 56
    external includes an agent could follow; diverges from discovery-over-suppression.
  RECOMMENDED: A. Removes the wrong-ecosystem overclaim and honestly surfaces observed data, without
    building the C reader (which would change underlying numbers → packet STOP).
  BLOCKING_REASON: Sets the output contract for an unsupported-ecosystem repo and the exact reader-context
    wording (VISION "labels speak the reader's language"); also draws the line between this honesty fix
    and the separate C-manifest-reader capability slice.

- ID: D3-ORIENT-CERTAINTY
  QUESTION: How granular is the `orient` certainty decomposition — a one-line relabel, or a small posture
    block — so that freshness/answer-class is no longer read as global semantic certainty?
  OPTIONS:
  - A. **POSTURE BLOCK (RECOMMENDED):** replace the bare `Certainty\n class exact, freshness fresh` footer
    with a small, self-labeling posture block that scopes "exact/fresh" to **snapshot serving + freshness**
    and co-locates the answer's epistemic posture already present elsewhere in the output: relationship-
    reliability (call/import/change LOW), module-model status (declared vs inferred + confidence),
    LiveGraph-status (unavailable → SQLite). Consequence: nothing reads as global certainty; the reader
    gets one coherent posture. Largely a **re-composition** — orient *already* computes/renders Degradation
    (:381-384), Limits/LiveGraph (:8-10), and the inferred-module headline; this co-locates + relabels,
    minimal new computation.
  - B. ONE-LINE RELABEL: rename the footer so it no longer claims global certainty — e.g. `Snapshot:
    served exact, freshness fresh` (or `Serving: answer-class exact (from snapshot); freshness fresh`) —
    but leave reliability/module/LiveGraph in their existing separate sections. Consequence: smaller; fixes
    the mislabel (clause (c)) but relies on the reader to connect scattered sections.
  - C. DROP the word: remove "Certainty"; fold freshness into "Other signals". Consequence: minimal; loses
    the (useful) freshness signal's prominence.
  RECOMMENDED: A (posture block) for full coherence; B is the smaller fallback if the operator wants the
    minimum change that removes the mislabel. Either A or B MUST also address the **name-vs-semantics**
    defect: `AnswerClass::Exact` is a *serving* fact — its rendered label must say "served exact / from
    snapshot," not a bare "exact" under "Certainty."
  BLOCKING_REASON: Changes the primary surface's footer contract and a label an agent reads as global
    confidence (clause (c)); the granularity (one line vs block) and the relabel of `Exact` are
    output-contract decisions the IMPL must not pick unilaterally.

- ID: D4-SYMBOL-COUNT
  QUESTION: Reconcile the three counts to one canonical number, precisely label each count's meaning, or
    both?
  OPTIONS:
  - A. **RECONCILE + LABEL (RECOMMENDED, composite):** (i) adopt stats-honesty-1 **D1=A** — drop the
    `visibility='export'` filter so `stats` "symbols" == `orient`'s unfiltered all-SYMBOL count (nginx:
    both 3977), sourcing the Summary total from the repo-level `compute_repo_summary` COUNT(*)
    (`agent_impl.rs:267-274`) for guaranteed orient-coherence (stats-honesty-1 D1 sub-choice (a)); **and**
    (ii) self-label the `index` count as **nodes (all kinds)** — a named superset of "symbols" (SYMBOL +
    FILE + MODULE) — so 4393 is never read as a "symbols" number. Net: ONE canonical "symbols" (3977,
    agreed by orient+stats); "nodes" a clearly distinct, labeled quantity. Resolves the Java false-zero
    AND the C undercount (§6.3) in one move. (Also surfaces the files 397-vs-396 gap: orient = all files,
    stats = module-owned — self-label or reconcile per the IMPL; recommend labeling stats' as "files in
    modules".)
  - B. LABEL only (don't change stats' number): keep stats at 1816 but relabel it "exported/public
    symbols", orient "symbols (all)", index "nodes (all kinds)". Consequence: honest via disambiguation,
    but stats still shows a per-language-incoherent "public surface" (1816 on C non-static funcs; 0 on
    Java) the reader rarely wants, and the word "symbols" still disagrees across surfaces.
  - C. RECONCILE only (no labels): make stats == orient (3977), leave `index` 4393 unexplained.
    Consequence: orient/stats agree, but 3977-vs-4393 still puzzles a reader who sees both.
  RECOMMENDED: A. One canonical "symbols" number across orient+stats AND a self-labeled "nodes" — the only
    cell that both reconciles and disambiguates, and it folds in stats-honesty-1 D1 cleanly.
  BLOCKING_REASON: Changes the meaning of a discovery-output number (`stats.symbol_count`: exports → all)
    and propagates across the SQLite↔LiveGraph stats byte-parity cert (the internal seam stats-honesty-1
    D1's BLOCKING_REASON already names) — allowed per VISION "nothing is frozen" but must be ratified +
    propagated in lockstep, not silently flipped.

- ID: D5-LOW-NEXT-ACTION
  QUESTION: When relationship reliability is LOW, what honest next-action / limitation line do the
    posture-bearing surfaces render (the STATEMENT only — NOT auto-enrich)?
  OPTIONS:
  - A. **TOOLCHAIN-AWARE HONEST LINE (RECOMMENDED):** append a reader-context line keyed on whether an
    enrichment **resolver exists for the repo's language**. If one exists (Rust / TypeScript / Java —
    `enrichment` subsystem, TECH-DEBT §Enrichment): *"relationship facts are low-confidence on this index;
    run enrichment to resolve more"* (a STATEMENT — auto-run is ENRICH-LIFECYCLE-1, out of scope). If none
    exists (C / C++ / Python — no resolver on this build): *"no semantic-resolution path exists for C on
    this build; these relationship facts remain low-confidence"* — honest about the dead-end, no false
    promise. Consequence: the agent learns whether a remedy exists, and is **not** sent to try `enrich` on
    C where it would fail (TECH-DEBT C6: enrich supports rust|typescript|java only).
  - B. GENERIC LINE: one fixed language-agnostic *"relationship facts are low-confidence; verify against
    source."* Consequence: honest but silent on whether a remedy exists — an agent may waste a turn trying
    enrich on C.
  - C. NO next-action line (status quo): the caveat states the problem, offers no path.
  RECOMMENDED: A. The toolchain-aware line is the honest STATEMENT the packet asks for; it must NOT trigger
    auto-enrich and must NOT over-promise on C. Resolver-availability-by-language is a known fact, rendered
    wherever the shared LOW posture is shown (so it stays uniform across surfaces).
  BLOCKING_REASON: Sets reader-facing guidance that could either correctly route or falsely promise a
    remedy (a false-trust risk on C); the wording + the enrich-mention scope are operator-ratified, and the
    boundary against auto-enrich (ENRICH-LIFECYCLE-1) must be explicit.

---

## 8. VISION defense (per choice)

- **Fact-Certainty Model / Layer Rules #2–#3 (`docs/VISION.md`; `agent_docs/architecture.md`).** Every
  cell turns a Layer-2 interpretation rendered as Layer-0 fact back into an honestly-layered signal: C1
  attaches the completeness posture the layer model's own worked example demands (*"raw counts without
  coverage or confidence markers are overclaims"*) and stops rendering an unresolved-graph `0` as
  measured absence (D1=C); C4 renders the true extracted count instead of a filtered-to-undercount one
  (D4=A).
- **`agent_docs/architecture.md` Mandatory Rule #6 ("`null` = unknown, empty = known-zero. Never
  conflate.").** D1=C(ii) is this rule applied literally to coupling metrics: a `fan_in=0` from an
  unresolved import graph is `unknown`, rendered as `unknown`.
- **"Labels speak the reader's language, not ours."** D2 phrases deps about the reader's repo ("56
  external includes observed, no C manifest reader"), not our pipeline ("npm"); D5 states the reader's
  remedy ("run enrichment" / "no path for C"), not "enrichment phase did not run"; D1's caveat reuses
  stats-honesty-1 D4(a)'s reader-context clauses, never "reliability LOW / unresolved_imports=".
- **Three Version Classes — freshness ≠ confidence (`docs/VISION.md` Versioning-First).** D3 separates
  the **provenance/freshness** signal ("served exact from a fresh snapshot") from the **answer's content
  reliability** (call/import LOW), refusing to collapse them under one word "Certainty" — and fixes the
  `AnswerClass::Exact` name-vs-semantics mislabel.
- **Protocol-Surface Standard, Layer 2 ("can an agent learn one truth from the output?").** D4 gives the
  agent ONE canonical "symbols" number across orient+stats and a self-labeled "nodes"; after this slice
  the surface stops answering "how many symbols?" with 3977-vs-1816.
- **"Nothing is frozen; optimize for the VISION" (the seam changes in D1/D4).** Changing `stats.symbol_count`
  (exports → all) and threading a posture field touch the SQLite↔LiveGraph stats byte-parity cert — a
  load-bearing internal seam — so they are **surfaced before the change** (D1/D4 BLOCKING_REASONs) and
  propagated in lockstep, scaled to blast radius (an internal seam, not a governance/gate object), exactly
  the VISION's ratified discipline.
- **Smallest design (CLAUDE.md decision criteria).** §4.3: no new abstraction — the posture already
  exists; this slice adds concrete renderer-callers to it. §10 states the rejected larger alternatives.

---

## 9. Validation plan — evidence the IMPL must PRODUCE (NOT RUN here)

This is an **obligation list for the IMPL**, not a record of executed checks (SPEC only — no code runs in
this slice). Each names the check + the evidence label the IMPL attaches AFTER implementation. Per
`docs/testing/end-of-slice-procedure.md` + the isolated dogfood (never index into the operator's real
registry):

1. **C1 — stats posture + no false zero (nginx, isolated).** `rmap stats` on a low-resolution C repo
   shows the reason-specific import-graph caveat above the dependency sections, and degenerate coupling
   (`fan_in+fan_out=0`) renders `unknown`/`—`, never `0`/`A=1.00`-as-fact; `rmap trust` on the same
   snapshot shows the matching import-graph LOW — honest in BOTH. → IMPL must produce EXECUTED evidence.
2. **C1 — partial graph keeps directional numbers (spring-petclinic).** A partially-resolved repo still
   shows its non-degenerate fan-in/out numbers, annotated (the stats-honesty-1 behavior preserved). →
   IMPL must produce EXECUTED evidence.
3. **C1 — no caveat when HIGH.** A fully-resolved repo shows the dependency sections with NO caveat (no
   noise). → IMPL must produce EXECUTED evidence.
4. **C2 — deps ecosystem honesty (nginx).** `rmap deps list` no longer prints `ecosystem:"npm"` on C;
   it shows the detected/none ecosystem + the reader-context "56 external includes observed, unattributed"
   line; the 56 count is unchanged (no resolver ran). → IMPL must produce EXECUTED evidence.
5. **C3 — orient posture, no global-certainty mislabel (nginx).** `rmap orient --full` no longer renders a
   bare `Certainty: exact/fresh`; freshness/answer-class is scoped to snapshot serving and the
   reliability/module-status/LiveGraph posture is legible together; `AnswerClass::Exact` renders as a
   serving fact, not bare "exact". → IMPL must produce EXECUTED evidence.
6. **C4 — one canonical symbol count (nginx).** `rmap stats` "symbols" == `rmap orient` "symbols" on the
   same snapshot (both the all-SYMBOL count); the `index` count is self-labeled "nodes (all kinds)";
   Java false-zero AND C undercount both resolved (re-check on spring-petclinic: stats == orient, non-zero).
   → IMPL must produce EXECUTED evidence.
7. **C4 — LiveGraph stats parity after the symbol-count seam change (TS repo).** On a GREEN TS repo `rmap
   stats --engine compare` is field-exact / byte-identical SQLite vs LiveGraph with the new all-symbols
   count; the cert stays GREEN. → IMPL must produce EXECUTED evidence.
8. **D5 — toolchain-aware next-action line.** On a LOW-reliability repo WITH a resolver (Rust/TS/Java) the
   line suggests enrichment (statement only — no auto-run); on C it states "no resolution path for C," and
   `rmap enrich` is NOT invoked by any of these surfaces. → IMPL must produce EXECUTED evidence.
9. **Scope guard (D2 of stats-honesty-1, adopted).** `git diff` touches only the consumer surfaces
   (dispatch stats/deps/orient renderers, the stats symbol CTE, response DTOs); it does NOT edit
   `trust/src/rules.rs`, `trust/src/service.rs`, or `agent/src/aggregators/trust.rs` (the
   RELIABILITY-REFRAME-1 producer territory). → IMPL must produce OBSERVED evidence (`git diff`).
10. **Contracts / gates.** `cargo build/fmt/clippy -D warnings/test` green in `rust/`; the smoke protocol
    (`docs/testing/rmap-test-protocol.md`) + `./scripts/dogfood-isolated.sh`. Existing stats tests asserting
    export-based `symbol_count` updated to all-symbols (enumerate in the IMPL); additive JSON fields
    (posture, `total_symbols`) optional/stripped in human output per the imports/cycles precedent. → IMPL must produce EXECUTED evidence.

---

## 10. Smallest-design statement & STOP-condition assessment

- **Smallest design.** The recommended path (D1=C, D2=A, D3=A, D4=A, D5=A) introduces **no new module,
  crate, registry, adapter, DTO layer, or config surface** (§4.3). It reuses: the existing
  `TrustOverlaySummary` / `compute_trust_overlay_for_snapshot` (`daemon-runtime/util/trust.rs`) that
  orient/trust already consume; orient's unfiltered symbol COUNT(*) (`agent_impl.rs:267-274`); the existing
  `compute_module_stats` query (one predicate dropped); the existing stats/deps/orient responses +
  renderers; and language→resolver-availability that the `enrichment` subsystem already knows. The only
  new data are **additive response fields carrying existing types**, each with one concrete current
  renderer-caller — earned, not speculative. **Rejected larger alternatives:** a per-surface
  reliability re-derivation (§4.3 — reproduces the incoherence); a stats-local degeneracy heuristic
  beyond the `0/0` guard (D1-B/threshold — invents a number); building the C-manifest reader to "fix"
  deps (D2 — changes underlying attribution, packet STOP, separate capability slice); making the
  Java/C extractors emit a uniform `export` (unneeded — the all-symbols count is the honest, available
  number).
- **STOP-condition assessment (packet).**
  - *"If making a surface render the posture requires a NEW cross-crate boundary beyond consuming the
    existing trust/reliability value → STOP."* → **NOT triggered** (§4.2). `daemon-runtime` already
    depends on `repo-graph-trust` (`Cargo.toml:44`); all four handlers live in `daemon-runtime`;
    `compute_trust_overlay_for_snapshot` is the existing reusable value orient already calls. The posture
    exists; this slice adds consumers, no boundary.
  - *"If any proposed fix would change the UNDERLYING resolution numbers (enrichment) rather than only
    their HONEST RENDERING → STOP."* → **NOT triggered** (§4.4). Every fix changes rendering/labeling or
    surfaces already-counted data (the 56 external imports, the existing `nodes` rows); no extractor,
    enrichment, or resolver runs. D4's export-filter drop counts **existing** rows differently — surfacing
    existing data, not new resolution (the same basis stats-honesty-1 established for #5).

---

## 11. Out of scope

- The repo-wide reliability **reframe** (in-scope rate, exclude out-of-scope refs, coverage map) —
  `RELIABILITY-REFRAME-1` (TECH-DEBT R1). This slice only **consumes** the existing posture; when the
  reframe lands, all four surfaces inherit it for free (shared value, §4.1).
- A **C/C++ dependency-manifest reader** and any **C/C++/Python enrichment resolver** — separate capability
  slices (ENRICH-LIFECYCLE-1 / a C-manifest slice). They change underlying attribution/resolution numbers
  (packet STOP). D2/D5 fix only the LABEL + honest surfacing on this build.
- **Auto-running enrichment** — ENRICH-LIFECYCLE-1 (R4). D5 emits the honest STATEMENT only; it must not
  invoke `enrich`.
- The **abstractness (`A`) non-OO classification** degeneracy as a measurement-fidelity fix — named
  deferred (D1 sub-choice (a); preserves stats-honesty-1 §11's scope-out). This slice ensures the
  *distance* line carries the caveat; it does not re-derive abstractness for non-OO languages.
- **C5** orient-budget bimodality (separate orient-density slice); **C6** enrich ergonomics
  (ENRICH-LIFECYCLE-1); **C7** smoke-harness exit-code conflation (tooling). Out of this slice.
- **MODULE-MODEL-1** topology/notion work (#3/#4) — ratified separately; this slice builds on its
  renderer (the `package groups` / `directory groups` relabel already shipped, OBSERVED `nginx-stats.txt`).
- Any **production code** in THIS slice (design only; a later IMPL executes the ratified cells).
- `docs/ROADMAP.md` / `docs/TECH-DEBT.md` / `docs/VISION.md` / `CURRENT_SLICE.md` and the other queued
  slices — edits out of scope per the selection packet; the operator records ratification + roadmap moves.

---

## 12. Ratification (operator — 2026-06-30)

Ratified after the relay's **decision-review phase** (DECISION-REVIEW-MODE-1): Codex challenged the
§7 slate, the builder (Claude) rebutted with first-hand source reads — **5 converged, 0 contested**.
The review CORRECTED two of the spec's own §10 recommendations before this gate, both toward a smaller /
more honest design.

**Binding slate (overrides §10 where noted):**
- **D1 = C** — annotate the dependency metrics when import-graph ≠ HIGH **and** convert degenerate
  zero-degree results (`fan_in+fan_out=0`) to an explicit `unknown`. **Ratified rider:** the honest
  `unknown` MUST reach **JSON consumers**, not only human text (architecture Rule #6 at the DTO layer) —
  never a bare `0`.
- **D2 = A** — derive `ecosystem` from the detected manifest/language; with no reader for the language,
  render `none-detected`/the language token + the honest "N external includes observed, not attributed"
  note. The C manifest reader is a separate capability slice.
- **D3 = B — CORRECTED from the spec's D3=A.** A posture *block* (A) is unearned duplication — orient
  already renders reliability / module-status / LiveGraph posture in its existing sections
  (`orient_reliability.rs`, `orient_sections.rs`). Ratified: a **one-line relabel** of the single footer
  site (`render_orient_envelope`, `orient.rs:130-166`) to serving/provenance semantics, faithful to
  `AnswerClass::Exact`'s real doc-comment meaning ("required-basis complete + fresh",
  `repo-graph-trust-model/src/lib.rs:28-37`), NOT a re-render. A block is a named, deferred promotion
  gated on demonstrated usability evidence.
- **D4 = A** — drop the `visibility='export'` filter so `stats` "symbols" == orient's all-SYMBOL count
  (3977), sourced from the repo-level `compute_repo_summary` COUNT(*); label `index`'s 4393 as
  **"nodes (all kinds)"**. Resolves the cross-surface count mismatch + the Java false-zero (#5) + the C
  undercount in one move.
- **D5 = A, key TIGHTENED to configured-resolver availability — CORRECTED from a flat language list.**
  "Rust/TypeScript/Java → run enrichment" is a FALSE PROMISE on a build without JDTLS (Java enrichment is
  JDTLS-gated; a blind Java suggestion errors). Ratified: Rust/TS (built-in) → suggest-enrich; Java →
  suggest only if JDTLS configured, else "configure JDTLS"; C/C++/Python → "no semantic-resolution path
  on this build." Keyed on the daemon's actual `available_languages`.

The earned-abstraction discipline held: the shared posture is the **existing** `compute_trust_overlay_
for_snapshot` / `TrustOverlaySummary` (orient/trust already consume it) — no new module/crate/DTO-layer
(§10); this slice adds CONSUMERS + additive response fields.

**IMPL decomposition (decide-and-record; multi-crate ⇒ split to converge, per the mega-slice lesson):**
- **HONEST-DEGRADATION-IMPL-1 = D1 + D4** — the stats overhaul (count reconciliation + LOW-resolution
  posture / degenerate-unknown; subsumes stats-honesty-1). Stats-cohesive + top-severity.
- **HONEST-DEGRADATION-IMPL-2 = D2 + D3 + D5** — deps ecosystem label + orient footer relabel + the
  cross-surface toolchain-aware next-action line.

Full audit trail (challenge / rebuttal / ratification packet) in the slice's `.agent-manager/` working dir.
