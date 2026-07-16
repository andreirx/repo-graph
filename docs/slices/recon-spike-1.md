# RECON-SPIKE-1 — classify the SCIP↔pipeline divergence (stop discarding the diff)

Status: SPECIFIED (2026-07-16) · Track: Reconciliation (ENGINE-CONSOLIDATION-1 §8b
prerequisite) · Origin: ratified direction change (human, 2026-07-16): the two call-graph
producers are two WITNESSES of the same truth; their differences are evidence to classify,
not a fight to adjudicate. The divergence has NEVER been measured on a real repo
(`dataflow-hotpath-map.md` residuals: "expected SCIP↔tree-sitter mismatches not yet
classified — NOT RUN").

## 1. Problem

The callgraph certificate (`daemon-runtime/src/callgraph_cert/`) performs an exhaustive
per-symbol multiset comparison of the two graphs on every fingerprint — then reduces the
result to one bit (GREEN serve / RED fallback) and discards the detail. We pay for the
comparison and throw away exactly the data the reconciliation design needs.

## 2. Contract (spike: instrument + run + classify; NO reconciliation logic)

1. **Emit the diff.** Additive instrumentation in the cert path: when the comparison runs,
   optionally capture the per-symbol mismatch detail (symbol key; SCIP-only edges;
   pipeline-only edges; per-edge target keys) to a debug artifact (JSON file under the
   state root or a `--debug`-gated dump — least-new-surface option, builder records the
   choice). Off by default; zero cost when off; the GREEN/RED behavior is UNCHANGED.
2. **Run on a real repo:** repo-graph self-index (isolated), with the SCIP producer
   enabled so LiveGraph partitions exist and the cert actually compares. Capture the full
   diff artifact.
3. **Classify every mismatch** in the build report, by:
   - DIRECTION: SCIP-only vs pipeline-only.
   - CAUSE (deterministic evidence per class, cited): semantic resolution the pipeline's
     heuristics missed (aliases/re-exports/etc.) · compilation-failure or producer-skip
     (files SCIP never saw) · coverage boundary (partition/language SCIP doesn't cover) ·
     identity/key mismatch (same edge, different keys — adoption failure) · other
     (enumerated honestly).
   - MAGNITUDE: counts per class; share of total edges; whether the vaunted "SCIP is
     richer" holds, inverts, or both-ways on this repo.
4. **The deliverable is the CLASSIFIED REPORT** (in the build report + a summary appended
   to this doc §5): the empirical answer to "superset or both-ways, and why" — the input
   RECON-DESIGN-1 is built from. Include the null result honestly if divergence is ~0 on
   repo-graph (small TS surface post-retirement) and state what the monorepo run must
   still answer.

## 3. Stop conditions

- Cert GREEN/RED semantics, serving behavior, and epoch/W-B invariants UNCHANGED —
  instrumentation is additive and gated. No reconciliation/merge logic. No schema changes.
- If the TS surface post-retirement is too small for SCIP partitions to exist at all →
  report that honestly as the finding (the spike then re-runs on the monorepo when field
  data exists) rather than fabricating divergence data.
- Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Cargo gates from `rust/` (fmt / clippy / affected crates + full suite chunked), raw exit
  statuses as they land.
- Named tests: instrumentation off-by-default (no artifact, no behavior change);
  on → artifact schema stable/deterministic ordering; GREEN/RED unchanged either way.
- Isolated live run (/private/tmp + stdio; registry checksum): the diff artifact captured
  + the classification with cited examples per class.

## 5. Findings (appended by the spike)

Status: EXECUTED (2026-07-17; **revised iteration 5** — iteration 4 applied review-3's three closing
corrections [degenerate-null measurements, RED⟺divergence scoping, `field_mismatch` relabel] on top of
iterations 1–3 (§5.9); iteration 5 corrects the `diff.rs` line count to the MEASURED **1323** (the stated
1308 was a reporting undercount) per review-4 — a reporting-only fix, no code/validation/artifact change (§5.10)).
Deliverable met: mergeable off-by-default instrumentation + BOTH an honest null on repo-graph
self-index AND a real, EXHAUSTIVELY classified SCIP↔pipeline divergence measurement on the committed
real-producer INGEST-CORE-1 fixture, with the **genuine byte-for-byte** raw artifact + the COMPLETE live-run
transcript retained + byte-verified (§5.4, run B re-executed this iteration against the fresh release `rmapd`
with the new `callgraph-diff/v3` schema). **Iteration 3 corrects the MAGNITUDE to CANONICAL directed edges**
(the pre-review-2 figures summed the two projections of each edge and double-counted): the honest answer on
this fixture is **D = 9 distinct edges, SCIP-only 7 (77.8%), pipeline-only 0, shared 2, richness 4.5×**. The
decisive superset-or-both-ways answer for the **deployment target** still requires the monorepo run (§5.5).

### 5.0 FINDING #0 — a latent, data-dependent daemon PANIC the comparison path hides (RATIFIED P1)

**The spike's first real payoff: the comparison path itself has a fail-soft gap.** The exhaustive walk
reached symbols the SHIPPED cert never does (it short-circuits at the first divergence) and hit a
**documented latent panic** in `repo-graph-livegraph::finalize_envelope`: a `Partial` answer from a
call-graph-incomplete basis (`AstFileScope` — e.g. the FILE node materialized for a top-level `import`)
with no mapped `DegradationReason` panics the `partial` constructor. Cited: the panic is
`.expect("partial invariant holds")` at **`repo-graph-livegraph/src/lib.rs:338`**; the PRECONDITION is
documented at **`lib.rs:303-306`** ("a call-graph-incomplete defining basis (e.g. `AstFileScope`) … would
panic the `partial` constructor; unreachable with current call-graph fixtures (recorded follow-up)").

**EMPIRICAL EVIDENCE (live run B, §5.4):** the daemon's stderr logged the panic **3×** —
`thread 'main' panicked at crates/repo-graph-livegraph/src/lib.rs:338:10: partial invariant holds:
PartialRequiresReasons` — exactly matching `rollup.livegraph_panic: 3` in the emitted artifact
(`main.ts:FILE` both directions + `shapes.ts:FILE` callers). The daemon did **not** crash (`rmapd`
exited 0 and `callers makeCircle` returned its result) ONLY because this spike's `lg_side` `catch_unwind`
caught each panic per symbol.

**Why it matters for the SHIPPED cert (not just the spike).** The shipped `callgraph_compare_is_exact`
also calls `lg.callers()`/`lg.callees()`. On THIS fixture the shipped verdict escapes the panic by
data-dependent luck: in BTreeSet key order `makeCircle` diverges first, so the short-circuiting verdict
returns RED and `callers` falls back to SQLite BEFORE the walk reaches a panicking `AstFileScope` FILE
symbol. **That escape is data-dependent, not a guarantee** — a repo whose first walked FILE symbol
panics with no earlier divergence would crash the shipped serve path. This is a **P1 fail-soft gap on a
serving/trust invariant**.

**Governance (operator-ratified 2026-07-16):** this spike stays NARROW and is NOT blocked on the fix
(the instrumentation is off-by-default → zero new production exposure to a PRE-EXISTING, data-dependent
panic the spike merely DISCOVERED). This slice's change does **not** alter that exposure — the verdict
path is untouched; only the new exhaustive collector is made panic-safe so it survives on a large repo.
The fix is ratified as the **IMMEDIATE NEXT slice, LIVEGRAPH-PARTIAL-FIX-1** (map the `AstFileScope`
`Partial` basis to a `DegradationReason`, or guard the walk) — a `repo-graph-livegraph` change, OUT OF
SCOPE here. **Consequence for RECON-DESIGN-1 (stated CONDITIONALLY — one fixture is not "always"):** an
exhaustive SCIP↔pipeline reconciliation walk hits this on any repo whose corpus contains an affected
`AstFileScope` FILE symbol (run B's did — 3 panics), and cannot be assumed panic-free on real data until
LIVEGRAPH-PARTIAL-FIX-1 lands; the reconciliation must therefore handle the `livegraph_panic` /
`livegraph_unanswerable` classes as first-class, not assume every symbol yields an answer.

### 5.1 Instrumentation (mergeable; off by default; verdict UNCHANGED)

`daemon-runtime/src/callgraph_cert/diff.rs` (new submodule; **1323 lines incl. tests; 19 `#[test]`s**
[`wc -l`, re-verified iteration 5]; was 1275 in iteration 3 — **+48 (measured)** for the iteration-4
change set: the six report measurement fields (`corpus_size`, `divergent_symbol_count`, `canonical_edges`,
`projection_incidences`, `rollup`, `symbols`) wrapped `Option` — `Some(..)` at the measured `collect`
construction site, `None` (→ serialized `null`) on the precondition-failed `degenerate` path — plus the
rewritten serialized-artifact degenerate test and the module-header/doc-comment wording that scopes the
RED⟺divergence equivalence to the measured path and relabels `field_mismatch`. The earlier "+33" was a
reporting undercount of that SAME change set (count corrected iteration 5, review-4); the `#[test]` count
is UNCHANGED at 19 — that degenerate test was REWRITTEN, not added) +
one gated call in `build_and_store_callgraph_cert` (`mod.rs`, **+9 lines**, untouched this iteration). When
the comparison runs, it optionally captures the per-symbol divergence the one-bit verdict discards.

- **Gate (builder's recorded choice): env var `RMAP_CALLGRAPH_DIFF=<dir>`.** Unset ⇒ one `var_os` lookup
  then return (zero corpus walk, no artifact, GREEN/RED byte-unchanged). Set ⇒ write
  `<dir>/callgraph-diff.json`. Chosen over a `--debug` CLI flag as **least-new-surface**: the cert build
  sits deep in the serve ladder (`callgraph_cert_eligibility` in the callers/callees/orient epoch step),
  not on a command, so a flag would thread through CLI→protocol→dispatch→handler; the env gate mirrors the
  shipped `RMAP_SCIP_TYPESCRIPT` / `RMAP_PERF` convention + the `.rgr/livegraph-compare/` sidecar precedent
  and needs no plumbing. `.rgr/`-style artifacts are already gitignored.
- **Additive + read-only:** reuses the verdict's own `lg_caller_rows`/`lg_callee_rows` builders +
  `find_symbol_callers`/`callees` reads — NO new SQLite surface, NO new dependency edge. The verdict
  authority (`callgraph_compare_is_exact`) is untouched; the report echoes the verdict (`cert_verdict`) as
  a cross-check **scoped to the MEASURED path (`precondition: null`)**: there, RED ⟺ ≥1 divergent symbol,
  by construction. On the degenerate precondition-failed path the verdict is still RED (the fallback) but no
  corpus is walked, so every measurement — `divergent_symbol_count` included — serializes as `null`
  (UNKNOWN, not 0); the equivalence does NOT apply there (review-3 #2 / §5.9 iteration-4 honesty fix).
- **Schema `callgraph-diff/v3` (v1→v2→v3, each an HONESTY correction, not a §3 "schema change").** The
  `vN` string labels ONLY this off-by-default, gitignored, spike-only debug artifact — introduced by THIS
  unmerged slice, referenced by no other crate/command/persisted surface (verified by grep). §3's "No
  schema changes" bars touching existing storage/serving schemas; refining the shape of brand-new
  instrumentation before merge is not that. **v2 (iteration 1):** the MAGNITUDE totals distinguish edges
  summed over MEASURED symbols from a count of UNMEASURED symbols, and every divergent symbol carries
  per-witness `caller_edges`/`callee_edges` that render **`null`** when a side was unanswerable / panicked
  / errored (v1 folded those unknowns in as 0 edges — unknown ≠ zero, per VISION). **v3 (iteration 3,
  review-2):** adds a `canonical_edges` block — the EDGE-LEVEL magnitude by canonical `(caller_key,
  callee_key)` identity — and RENAMES the per-direction `totals` (sub-field `edges`) to
  `projection_incidences` (sub-field `incidences`). *Why:* a directed edge is witnessed TWICE (once in its
  caller's callee-projection, once in its callee's caller-projection), so SUMMING the caller + callee
  directions double-counts every edge whose both endpoints are in the corpus — the review-2 defect. The
  honest name + the canonical block make that un-mistakable. Fields: `witness` mapping (livegraph=scip /
  sqlite=tree-sitter), `fingerprint`, `snapshot_uid`, `cert_verdict`, `precondition` (honest-null marker),
  `corpus_size`, `divergent_symbol_count`, **`canonical_edges`** (`livegraph_total`/`sqlite_total`/
  `scip_only`/`pipeline_only`/`shared`/`union_edges` + a self-describing `note`), `projection_incidences`
  (per-direction `incidences` + `unmeasured_symbols`), `rollup` (per-DIRECTION counts + `livegraph_
  unanswerable` / `livegraph_panic`), and per-symbol `livegraph_only` / `sqlite_only` / `field_mismatch`
  buckets + notes + edge counts. Deterministic (BTreeSet corpus / BTreeMap buckets/edges / sorted reprs).
- **Degenerate (precondition-failed) shape (iteration 4, review-3 #1).** When `precondition` is non-null
  (no corpus was walked — no resident LiveGraph / no resident partitions / a storage error), the SIX
  measurement fields (`corpus_size`, `divergent_symbol_count`, `canonical_edges`, `projection_incidences`,
  `rollup`, `symbols`) all serialize as **`null`** — UNKNOWN, never a phantom measured `0`/`{}`/`[]`. This
  COMPLETES v3's own "unknown ≠ zero" contract (already applied to the per-symbol `caller_edges`/
  `callee_edges` in v2) at the top level, where v3 had incompletely applied it; it is a bug-fix WITHIN v3,
  NOT a new schema version — the MEASURED artifact (run B, §5.4) is byte-UNCHANGED (`Some(x)` serializes
  identically to a bare `x`), so the reviewer-verified v3 evidence and its 7/0/2/9 classification stand
  undisturbed. (Reachability: in production the cert build is itself gated on a resident LiveGraph, so the
  degenerate path is defensive — see §5.9 — and its correct validation is the named serialized-artifact
  test, not a live daemon run.)
- **Tests (19 in `diff.rs`; the `callgraph_cert` module = 27 = 19 + 8 in `tests.rs`; the `cargo test
  callgraph_cert` filter matches 28 — the extra is `orient_serve::…despite_green_callgraph_cert`, whose
  name contains "callgraph"):** pure classify (both directions, multiplicity, field-mismatch,
  equal-multisets-empty); **honesty — unknown ≠ zero** (unanswerable/panicked/sqlite-error each record the
  edge count as `None`, `Some(0)` renders as the number, `None` renders as `null`, `SideProjection` counts
  unmeasured symbols never summing 0); **canonical edges (iteration 3, review-2 #3)** — one edge seen from
  BOTH projections counts ONCE, repeated-edge multiplicity preserved (MAX, not summed to 4×), an edge
  recovered from the sole MEASURED projection while the other is unmeasured (unknown ≠ 0), and the
  `scip_only`/`pipeline_only`/`shared`/`union_edges` class split with the `D = sum` invariant; fixture
  `collect` GREEN⇒0-divergence+1 shared canonical edge & `drop_calls`⇒2-divergent-symbols but exactly **1**
  canonical SCIP-only edge (proving the 2-projection-incidence→1-edge dedup on a real fixture);
  **determinism** — two emissions BYTE-equal, asserting `canonical_edges`/`projection_incidences` present;
  **gate control** — `RMAP_CALLGRAPH_DIFF` toggled off→no artifact, on→artifact in the named dir;
  **enabled-gate, BOTH verdict branches** (iteration 2, review-1 #2) — the STORED verdict is identical AND
  correct with emission on vs off for the faithful-mirror fixture (⇒GREEN) AND the dropped-CALLS-edge
  fixture (⇒RED), and the ON arm emits the artifact in each; **degenerate serialized-artifact ⇒ every
  measurement field renders `null`** (never a phantom `0`/`{}`/`[]`, via the real `emit_report` write path;
  review-3 #1); panic-safety (`lg_side` catches the §5.0 upstream panic).

### 5.2 Live run A — repo-graph self-index: HONEST NULL (producer absent)

EXECUTED, isolated (`/private/tmp` state root + stdio; operator registry confirmed untouched):
`rmap index` (tree-sitter/SQLite) succeeds, `rmap callers` serves from SQLite — but
`rmap dev livegraph-refresh` returns `{"status":"ProducerUnavailable","detail":"scip-typescript not
found (set RMAP_SCIP_TYPESCRIPT or add it to PATH)"}`. No LiveGraph ⇒ the cert never reaches its compare
⇒ **no divergence artifact** (`precondition` would be `no_resident_livegraph`). Two independent causes:

1. **Environmental:** `scip-typescript` is not installed and Node is v22.21.1; the pinned
   `scip-typescript@0.4.0` crashes on Node 22 (PRODUCER-COMPAT-1). The SCIP witness cannot be produced
   on this machine.
2. **Structural:** the TS surface was retired to `tools/rgistr/src` (~7 files) by
   TS-PROTOTYPE-RETIREMENT-1 (`800d78e`); even with a producer the divergence surface would be tiny.

This is the null the slice anticipated. It is NOT the answer for the deployment target (§5.5).

### 5.3 Live run B — INGEST-CORE-1 fixture: REAL, EXHAUSTIVELY classified divergence

To exercise the instrumentation against a REAL SCIP witness without a live producer, the committed
real-producer `synthetic/index.scip` was fed via the daemon `livegraph_preload` method into ONE
`rmapd --stdio` process, in the same NDJSON request stream as an `index` of the same two sources
(tree-sitter/SQLite) and a `callers` query that triggers the cert compare (the in-memory LiveGraph
persists across requests in one process). Fully isolated; the reproduction recipe + transcript + raw
artifact are retained in §5.4. **This is the INGEST-CORE-1 fixture (`main.ts` + `shapes.ts`, 2 files),
NOT repo-graph self-index and NOT the monorepo — see §5.5 caveats.**

Empirical result: `cert_verdict: RED`, `corpus_size: 9`, `divergent_symbol_count: 8`. The 9th
(non-divergent) symbol is `report` — its one caller-less state and its two syntactic calls
(`report→makeCircle`, `report→Circle.describe`) are present on BOTH witnesses, so it is GREEN.

**Every one of the 8 divergent symbols, classified (no sampling):**

| # | Symbol (`repo:…`) | DIRECTION | CAUSE (cited from the fixture + artifact) | MAGNITUDE (lg / sqlite) |
|---|---|---|---|---|
| 1 | `src/main.ts#makeCircle` FUNCTION | callees **SCIP-only** (2); callers agree | **(A) semantic** — `return new Circle(radius)` → SCIP resolves the instantiation to `Circle.constructor` **and** the class `Circle`; the syntax pipeline records no callee for `new` | callee lg 2 / sq 0; caller lg 1 / sq 1 (agree: `report`) |
| 2 | `src/main.ts:FILE` | **UNMEASURABLE** (LiveGraph PANIC, both dirs) | **(E) finding #0** — `finalize_envelope` panic on the `AstFileScope` FILE node (materialized by the top-level `import { Circle }`); `lib.rs:338` | caller lg **null** / sq 0; callee lg **null** / sq 0 (2 of the 3 panics) |
| 3 | `src/shapes.ts#Circle.area` GETTER | callees **SCIP-only** (2, ×2 multiplicity); callers agree | **(A) semantic** — getter body `3.14 * this.radius * this.radius` reads `this.radius` **twice** → SCIP emits 2 ref edges to `Circle.radius`; the pipeline emits no `this.field` read edges. Multiplicity preserved by the multiset classifier | callee lg 2 / sq 0; caller lg 0 / sq 0 |
| 4 | `src/shapes.ts#Circle.constructor` CONSTRUCTOR | callers **SCIP-only** (1); callees **SCIP-only** (1) | **(A) semantic** — callers: `new Circle()` in `makeCircle` → SCIP resolves the ctor caller `makeCircle`; callees: `this.radius = radius` → SCIP ref to `Circle.radius`; the pipeline records neither | caller lg 1 / sq 0; callee lg 1 / sq 0 |
| 5 | `src/shapes.ts#Circle.describe` METHOD | callees **SCIP-only** (1); callers agree | **(A) semantic** — callees: `if (this.radius > 10)` reads `this.radius` → SCIP ref to `Circle.radius`; callers: `report` calls `describe()` (a real cross-file CALL both witnesses see) | callee lg 1 / sq 0; caller lg 1 / sq 1 (agree: `report`) |
| 6 | `src/shapes.ts#Circle.radius` PROPERTY | callers **SCIP-only** (4); callees agree | **(A) semantic (incoming mirror)** — `radius` is read/written by `area` (×2), `constructor` (×1), `describe` (×1) → SCIP models 4 incoming reference edges; the pipeline models no property as an edge endpoint | caller lg **4** / sq 0 — the largest single-symbol divergence; callee lg 0 / sq 0 |
| 7 | `src/shapes.ts#Circle` CLASS | callers **SCIP-only** (2); callees agree | **(A) semantic** — `Circle` is referenced by `makeCircle` (`new Circle()` + return type `: Circle`) and by `main.ts:FILE` (the top-level `import { Circle }` → file-scope reference); SCIP models both incoming, the pipeline models no incoming edge to a class identity | caller lg 2 / sq 0; callee lg 0 / sq 0 |
| 8 | `src/shapes.ts:FILE` | **UNMEASURABLE** (callers PANIC; callees `Partial`) | **(E) finding #0** — callers panic (same invariant, `lib.rs:338`); callees return `AnswerClass::Partial` (`livegraph_class=Partial`) — the call-graph-incomplete FILE basis degrades cleanly one way and panics the other. Asymmetry recorded | caller lg **null** / sq 0; callee lg **null** / sq 0 (1 panic + 1 unanswerable) |

*(The per-symbol lg/sq counts above are PROJECTION counts — a symbol's caller-list and callee-list. They
are correct per-symbol, but a directed edge appears in TWO of them; the aggregate below is by CANONICAL
edge, not a sum of projections.)*

**DIRECTION (aggregate):** every divergent edge is **SCIP-only**; **ZERO pipeline-only**. **ZERO
identity/key mismatch** — CAUSE (D), the SAME logical edge carried under DIFFERENT keys on the two
witnesses (the "adoption failure" of §2) — argued from the **absence of any unmatched SQLite-side edge**:
`canonical_edges.pipeline_only = 0` and `rollup.callers_sqlite_only = callees_sqlite_only = 0`. A genuine
identity/key mismatch NECESSARILY surfaces as a `sqlite_only` (pipeline-only) edge — the pipeline's
divergent key for an edge SCIP also witnesses under its own key — so it would raise `pipeline_only` above 0;
with NO unmatched pipeline-side edge (every pipeline edge is `shared` under the SAME key), none is orphaned
under a divergent key ⇒ zero CAUSE (D). This is CORROBORATED by — not derived from — the directly OBSERVABLE
fact that both witnesses key the SAME symbol as `repo_<uid>:src/path#name:KIND` (visible throughout the raw
§5.4 artifact), confirming XPART-PROVE-1B's "SCIP keys byte-equal to SQLite via repo_uid" (cited source).
**A DISTINCT, separately-zero class — `field_mismatch = 0` (review-3 #3):** the code's `field_mismatch`
bucket is a SAME-KEY, ENRICHMENT-DIVERGENT divergence (same stable key on BOTH witnesses, balanced
multiplicity, but the rendered name/file/module differs) — NOT a key mismatch. Its zero means the shared
edges' endpoints render byte-identically on both sides. `field_mismatch` is NOT the identity/key-mismatch
(D) signal and is no longer cited as such: a key mismatch is an UNMATCHED edge (a nonzero `sqlite_only`),
not a matched-key enrichment delta. The two FILE symbols are NOT a content divergence but a LiveGraph-side
ANSWERABILITY gap (finding #0): 3 caught panics + 1 clean `Partial` → **4 unmeasured projections**.

**MAGNITUDE — CANONICAL directed edges (review-2; the authoritative `canonical_edges` block of the §5.4
artifact).** Merging each witness's caller- and callee-projections by canonical identity `(caller_key,
callee_key)` — multiplicity preserved, an edge seen from BOTH projections counted **once**, an edge whose
one projection panicked recovered from the other (unknown ≠ 0): `livegraph_total = 9`, `sqlite_total = 2`,
`scip_only = 7`, `pipeline_only = 0`, `shared = 2`, `union_edges (D) = 9`. **Per-witness richness: SCIP
9 canonical directed edges vs pipeline 2 → 4.5×.** The **7 SCIP-only** canonical edges enumerated (6
distinct pairs; `area→radius` has multiplicity 2):

1. `makeCircle → Circle.constructor`
2. `makeCircle → Circle`
3. `main.ts:FILE → Circle` — recovered from `Circle`'s caller-projection; `main.ts:FILE`'s own
   callee-projection PANICKED (finding #0), so this edge would be LOST by a callee-only count
4. `Circle.area → Circle.radius` **(×2)** — `this.radius` read twice in the getter body
5. `Circle.constructor → Circle.radius`
6. `Circle.describe → Circle.radius`

The **2 shared** canonical edges (present on both witnesses): `report → makeCircle`, `report →
Circle.describe`. **Superset-or-both-ways on THIS fixture: SCIP is a STRICT SUPERSET** (every pipeline edge
is shared; pipeline-only = 0) — but read honestly: the fixture is purpose-built to showcase SCIP's semantic
edges (`this.field` reads, instantiations, imports) and the tree-sitter side is near-empty (2 real
syntactic calls), so "superset" reflects BOTH SCIP richness AND pipeline sparsity on a 2-file sample. It
does NOT establish a GLOBAL superset (§5.5).

**Correction recorded (review-2): the earlier D=17 double-counted.** Iterations 1–2 computed
`D = livegraph_caller.edges 9 + livegraph_callee.edges 8 = 17` and `scip_only = callers_livegraph_only 7 +
callees_livegraph_only 6 = 13`. Those SUM the two PROJECTIONS of the same graph — e.g.
`makeCircle→Circle.constructor` is counted once under `makeCircle`'s callee-projection AND again under
`Circle.constructor`'s caller-projection — so every edge whose both endpoints are in the corpus was
counted twice. The canonical count (`D=9`, `scip_only=7`, `shared=2`, richness `4.5×`) is the honest
distinct-edge magnitude; the `9`/`8`/`7`/`6` figures survive below ONLY as PROJECTION INCIDENCES.

**MAGNITUDE — share of total edges per class (explicit denominator; unmeasured sides excluded per unknown
≠ zero).** DENOMINATOR **D = 9 distinct canonical directed edges** = SCIP-only 7 + pipeline-only 0 +
shared 2. The **4 unmeasured LiveGraph FILE-symbol projections** (`unmeasured_symbols`: 2 caller + 2
callee, the finding-#0 panics/`Partial`) are UNKNOWN, not edges — **excluded from D** (VISION: unknown ≠
zero).

| Class (DIRECTION / CAUSE) | Canonical edges | Share of D=9 |
|---|---|---|
| **SCIP-only** (`canonical_edges.scip_only`) — CAUSE (A) semantic | **7** | **77.8%** |
| **pipeline-only** (`canonical_edges.pipeline_only`) — CAUSE (B) compile-skip / (C) coverage | **0** | **0.0%** |
| **identity/key mismatch** — CAUSE (D); evidenced by the absence of any unmatched SQLite-side edge (`canonical_edges.pipeline_only = rollup.callers_sqlite_only = callees_sqlite_only = 0`), NOT by `field_mismatch` | **0** | **0.0%** |
| **shared** (`canonical_edges.shared`; present + field-equal on BOTH witnesses) | **2** | **22.2%** |
| *(E) finding #0 — UNMEASURABLE FILE projections* | *n/a (4 unknown projections / 2 symbols)* | *excluded from D* |

*Projection incidences (diagnostic — NOT distinct edges, do NOT sum): `projection_incidences` =
`livegraph_caller 9` / `livegraph_callee 8` / `sqlite_caller 2` / `sqlite_callee 2`;
`rollup.callers_livegraph_only 7` / `callees_livegraph_only 6`.*

*Same-key enrichment divergence (diagnostic — a class SEPARATE from CAUSE (D)): `rollup.field_mismatch = 0`
means that for the 2 shared edges both witnesses render byte-identical name/file/module for the endpoint
keys. This is NOT the identity/key-mismatch count and is not cited as such (review-3 #3).*

**Reading it:** SCIP-only **77.8%** ≫ pipeline-only **0.0%** → **SCIP is a strict superset on this fixture**
(the 2 pipeline edges are a subset of SCIP's 9; 100% of the pipeline's edges are shared, 0% unique). By
CAUSE, **100% of the divergent edges are (A) semantic**; classes (B)/(C)/(D) are **0% on this fixture** —
exactly the classes §5.5 says only the monorepo can populate. Per-witness richness: SCIP 9 canonical
directed edges vs pipeline 2 → **4.5×** (supersedes the projection-summing "4.25×" of iterations 1–2).

### 5.4 Retained evidence — raw artifact + full transcript (byte-verified, independently checkable)

**All four raw run-B files are retained in the slice workspace and byte-verified** (`cmp -s` against the exact
bytes `rmapd` wrote): `.agent-manager/slices/RECON-SPIKE-1/runs/runB/` — `requests.ndjson` (546 B),
`responses.ndjson` (**2628 B, the COMPLETE transcript — 24 lines = 21 index-progress frames + 3 result
lines**), `stderr.log` (1271 B), `callgraph-diff.json` (7090 B). **review-2 #4:** the COMPLETE
`responses.ndjson` (not only its result lines) is now durably retained here, no longer only under ephemeral
`/private/tmp`; the byte-verification passed for all four (each reported `BYTE-EQUAL`).

**Isolation (proven THIS iteration — 2026-07-17, run B re-executed against the fresh release `rmapd` after the
v3 code change):** operator registry `~/Library/Application Support/repo-graph/registry.json` SHA-1 =
`218414423f398e31f5c5a2ce627056146f21ebae`, IDENTICAL before AND after the run (`shasum` twice). The fixture
path is ABSENT from the operator registry (`grep -c recon-spike-1-runB-iter3` = **0**) and PRESENT only in the
isolated sandbox registry `/private/tmp/recon-spike-1-runB-iter3/registry.json` (= **2**). No orphaned process
(`pgrep -fl recon-spike-1-runB-iter3` → none; `rmapd` exited 0 on EOF). The run used `RMAP_STATE_ROOT=
/private/tmp/recon-spike-1-runB-iter3` (SandboxLocal) + stdio; the operator daemon was never contacted.

**Reproduction recipe (single `rmapd --stdio` process; `<repo>` = this repo root; the run used the
committed-fixture absolute scip path). The retained `requests.ndjson`, verbatim except the `<repo>`
placeholder for the scip path:**

```
# src-only copy of the fixture keeps the corpus clean (no node_modules):
#   /private/tmp/recon-spike-1-runB-iter3/fixture/{package.json,tsconfig.json,src/main.ts,src/shapes.ts}
{"id":"1","method":"index","params":{"repo_path":"/private/tmp/recon-spike-1-runB-iter3/fixture"}}
{"id":"2","method":"livegraph_preload","params":{"repo":"/private/tmp/recon-spike-1-runB-iter3/fixture","partition_id":"synthetic","scip":"<repo>/rust/crates/repo-graph-scip-ingest/tests/fixtures/synthetic/index.scip","source_root":"/private/tmp/recon-spike-1-runB-iter3/fixture"}}
{"id":"3","method":"callers","params":{"repo":"/private/tmp/recon-spike-1-runB-iter3/fixture","symbol":"makeCircle"}}
# run (rmapd = rust/target/release/rmapd, built fresh this iteration):
RMAP_STATE_ROOT=/private/tmp/recon-spike-1-runB-iter3 RMAP_CALLGRAPH_DIFF=/private/tmp/recon-spike-1-runB-iter3/diff \
  rmapd --stdio < requests.ndjson > responses.ndjson 2> stderr.log
```

**Transcript — `responses.ndjson` result lines (BYTE-FOR-BYTE; the 21 `{"id":"1","progress":…}` index
streaming frames precede the id:1 result — no cert/divergence data — and ARE present in the retained COMPLETE
file):**

```
{"id":"1","result":{"repo_uid":"repo_01kxpdrf1fvasdf2fs248tg1h9","canonical_path":"/private/tmp/recon-spike-1-runB-iter3/fixture","db_path":"/private/tmp/recon-spike-1-runB-iter3/databases/c9ce32734aef1c09.db","snapshot_uid":"repo_01kxpdrf1fvasdf2fs248tg1h9/2026-07-16T21:36:40.556Z/3ab81a07","files_total":2,"nodes_total":10,"edges_total":6,"edges_unresolved":0,"retention":{"pruned_count":0,"prunable_count":0,"current":1,"parent":0,"baseline_auto":0,"baseline_user":0,"total":1,"auto_pass":"queued"},"enrichment":{"auto_pass":"queued"}}}
{"id":"2","result":{"partition_id":"synthetic","nodes":15,"edges":12,"value_facts":5,"epoch":1}}
{"id":"3","result":{"target":{"stable_key":"repo_01kxpdrf1fvasdf2fs248tg1h9:src/main.ts#makeCircle:SYMBOL:FUNCTION","name":"makeCircle","qualified_name":"makeCircle","kind":"SYMBOL","subtype":"FUNCTION","file":"src/main.ts","line":5,"column":7},"callers":[{"stable_key":"repo_01kxpdrf1fvasdf2fs248tg1h9:src/main.ts#report:SYMBOL:FUNCTION","name":"report","qualified_name":"report","kind":"SYMBOL","subtype":"FUNCTION","file":"src/main.ts","line":9,"column":7,"edge_type":"CALLS","resolution":"static"}],"count":1,"backend_used":"sqlite","fallback_reason":"LiveGraphUnavailable"}}
```

`callers makeCircle` served the caller `report` from `backend_used:"sqlite"` (`fallback_reason:
"LiveGraphUnavailable"`) — the RED cert's SQLite fallback, consistent with `cert_verdict:"RED"`. The cert
compare that produced the fallback ALSO emitted the artifact (the `maybe_emit` call in the same build).

**Transcript — `stderr.log` (BYTE-FOR-BYTE; the caught panic **3×** = `livegraph_panic:3`; `rmapd` exit 0):**

```
warning: --stdio mode is for debug/test only, not production
note: running in sandbox-local mode (state root: /private/tmp/recon-spike-1-runB-iter3)
note: authority writes (baselines, aliases, declarations) are blocked
note: cache operations (index, refresh, queries) are allowed
op index started (repo repo_01kxpdrf1fvasdf2fs248tg1h9)
op index phase scanning (repo repo_01kxpdrf1fvasdf2fs248tg1h9)
op index phase initializing (repo repo_01kxpdrf1fvasdf2fs248tg1h9)
op index phase extracting (repo repo_01kxpdrf1fvasdf2fs248tg1h9)
op index phase resolving (repo repo_01kxpdrf1fvasdf2fs248tg1h9)
op index phase persisting (repo repo_01kxpdrf1fvasdf2fs248tg1h9)
op index completed (repo repo_01kxpdrf1fvasdf2fs248tg1h9, snapshot repo_01kxpdrf1fvasdf2fs248tg1h9/2026-07-16T21:36:40.556Z/3ab81a07)

thread 'main' (65914980) panicked at crates/repo-graph-livegraph/src/lib.rs:338:10:
partial invariant holds: PartialRequiresReasons
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

thread 'main' (65914980) panicked at crates/repo-graph-livegraph/src/lib.rs:338:10:
partial invariant holds: PartialRequiresReasons

thread 'main' (65914980) panicked at crates/repo-graph-livegraph/src/lib.rs:338:10:
partial invariant holds: PartialRequiresReasons
```

**Raw emitted artifact `callgraph-diff.json` (BYTE-FOR-BYTE — the exact 7090-byte pretty-printed file this
run wrote; `schema: callgraph-diff/v3`; FULL keys, no `…`; every symbol key carries the
`repo_01kxpdrf1fvasdf2fs248tg1h9` prefix verbatim; identical bytes to the retained `runs/runB/callgraph-diff.json`):**

```json
{
  "schema": "callgraph-diff/v3",
  "witness": {
    "livegraph": "scip_fed_livegraph",
    "sqlite": "tree_sitter_pipeline_sqlite"
  },
  "fingerprint": "imp|snap:repo_01kxpdrf1fvasdf2fs248tg1h9/2026-07-16T21:36:40.556Z/3ab81a07|pol:6|parts[synthetic@1:f1:ts1:preload:scip-typescript@preload]",
  "snapshot_uid": "repo_01kxpdrf1fvasdf2fs248tg1h9/2026-07-16T21:36:40.556Z/3ab81a07",
  "cert_verdict": "RED",
  "precondition": null,
  "corpus_size": 9,
  "divergent_symbol_count": 8,
  "canonical_edges": {
    "note": "directed edges by canonical identity (caller_key,callee_key), multiplicity preserved, each witness's caller+callee projections merged per identity (an edge seen from both counts once) — NOT projection_incidences summed; union_edges = scip_only + pipeline_only + shared",
    "livegraph_total": 9,
    "sqlite_total": 2,
    "scip_only": 7,
    "pipeline_only": 0,
    "shared": 2,
    "union_edges": 9
  },
  "projection_incidences": {
    "livegraph_caller": {
      "incidences": 9,
      "unmeasured_symbols": 2
    },
    "sqlite_caller": {
      "incidences": 2,
      "unmeasured_symbols": 0
    },
    "livegraph_callee": {
      "incidences": 8,
      "unmeasured_symbols": 2
    },
    "sqlite_callee": {
      "incidences": 2,
      "unmeasured_symbols": 0
    }
  },
  "rollup": {
    "callers_livegraph_only": 7,
    "callers_sqlite_only": 0,
    "callees_livegraph_only": 6,
    "callees_sqlite_only": 0,
    "field_mismatch": 0,
    "livegraph_unanswerable": 4,
    "livegraph_panic": 3
  },
  "symbols": [
    {
      "symbol": "repo_01kxpdrf1fvasdf2fs248tg1h9:src/main.ts#makeCircle:SYMBOL:FUNCTION",
      "caller_edges": {
        "livegraph": 1,
        "sqlite": 1
      },
      "callers": {
        "livegraph_only": [],
        "sqlite_only": [],
        "field_mismatch": []
      },
      "callers_note": null,
      "callee_edges": {
        "livegraph": 2,
        "sqlite": 0
      },
      "callees": {
        "livegraph_only": [
          "repo_01kxpdrf1fvasdf2fs248tg1h9:src/shapes.ts#Circle.constructor:SYMBOL:CONSTRUCTOR",
          "repo_01kxpdrf1fvasdf2fs248tg1h9:src/shapes.ts#Circle:SYMBOL:CLASS"
        ],
        "sqlite_only": [],
        "field_mismatch": []
      },
      "callees_note": null
    },
    {
      "symbol": "repo_01kxpdrf1fvasdf2fs248tg1h9:src/main.ts:FILE",
      "caller_edges": {
        "livegraph": null,
        "sqlite": 0
      },
      "callers": {
        "livegraph_only": [],
        "sqlite_only": [],
        "field_mismatch": []
      },
      "callers_note": "livegraph_panic",
      "callee_edges": {
        "livegraph": null,
        "sqlite": 0
      },
      "callees": {
        "livegraph_only": [],
        "sqlite_only": [],
        "field_mismatch": []
      },
      "callees_note": "livegraph_panic"
    },
    {
      "symbol": "repo_01kxpdrf1fvasdf2fs248tg1h9:src/shapes.ts#Circle.area:SYMBOL:GETTER",
      "caller_edges": {
        "livegraph": 0,
        "sqlite": 0
      },
      "callers": {
        "livegraph_only": [],
        "sqlite_only": [],
        "field_mismatch": []
      },
      "callers_note": null,
      "callee_edges": {
        "livegraph": 2,
        "sqlite": 0
      },
      "callees": {
        "livegraph_only": [
          "repo_01kxpdrf1fvasdf2fs248tg1h9:src/shapes.ts#Circle.radius:SYMBOL:PROPERTY",
          "repo_01kxpdrf1fvasdf2fs248tg1h9:src/shapes.ts#Circle.radius:SYMBOL:PROPERTY"
        ],
        "sqlite_only": [],
        "field_mismatch": []
      },
      "callees_note": null
    },
    {
      "symbol": "repo_01kxpdrf1fvasdf2fs248tg1h9:src/shapes.ts#Circle.constructor:SYMBOL:CONSTRUCTOR",
      "caller_edges": {
        "livegraph": 1,
        "sqlite": 0
      },
      "callers": {
        "livegraph_only": [
          "repo_01kxpdrf1fvasdf2fs248tg1h9:src/main.ts#makeCircle:SYMBOL:FUNCTION"
        ],
        "sqlite_only": [],
        "field_mismatch": []
      },
      "callers_note": null,
      "callee_edges": {
        "livegraph": 1,
        "sqlite": 0
      },
      "callees": {
        "livegraph_only": [
          "repo_01kxpdrf1fvasdf2fs248tg1h9:src/shapes.ts#Circle.radius:SYMBOL:PROPERTY"
        ],
        "sqlite_only": [],
        "field_mismatch": []
      },
      "callees_note": null
    },
    {
      "symbol": "repo_01kxpdrf1fvasdf2fs248tg1h9:src/shapes.ts#Circle.describe:SYMBOL:METHOD",
      "caller_edges": {
        "livegraph": 1,
        "sqlite": 1
      },
      "callers": {
        "livegraph_only": [],
        "sqlite_only": [],
        "field_mismatch": []
      },
      "callers_note": null,
      "callee_edges": {
        "livegraph": 1,
        "sqlite": 0
      },
      "callees": {
        "livegraph_only": [
          "repo_01kxpdrf1fvasdf2fs248tg1h9:src/shapes.ts#Circle.radius:SYMBOL:PROPERTY"
        ],
        "sqlite_only": [],
        "field_mismatch": []
      },
      "callees_note": null
    },
    {
      "symbol": "repo_01kxpdrf1fvasdf2fs248tg1h9:src/shapes.ts#Circle.radius:SYMBOL:PROPERTY",
      "caller_edges": {
        "livegraph": 4,
        "sqlite": 0
      },
      "callers": {
        "livegraph_only": [
          "repo_01kxpdrf1fvasdf2fs248tg1h9:src/shapes.ts#Circle.area:SYMBOL:GETTER",
          "repo_01kxpdrf1fvasdf2fs248tg1h9:src/shapes.ts#Circle.area:SYMBOL:GETTER",
          "repo_01kxpdrf1fvasdf2fs248tg1h9:src/shapes.ts#Circle.constructor:SYMBOL:CONSTRUCTOR",
          "repo_01kxpdrf1fvasdf2fs248tg1h9:src/shapes.ts#Circle.describe:SYMBOL:METHOD"
        ],
        "sqlite_only": [],
        "field_mismatch": []
      },
      "callers_note": null,
      "callee_edges": {
        "livegraph": 0,
        "sqlite": 0
      },
      "callees": {
        "livegraph_only": [],
        "sqlite_only": [],
        "field_mismatch": []
      },
      "callees_note": null
    },
    {
      "symbol": "repo_01kxpdrf1fvasdf2fs248tg1h9:src/shapes.ts#Circle:SYMBOL:CLASS",
      "caller_edges": {
        "livegraph": 2,
        "sqlite": 0
      },
      "callers": {
        "livegraph_only": [
          "repo_01kxpdrf1fvasdf2fs248tg1h9:src/main.ts#makeCircle:SYMBOL:FUNCTION",
          "repo_01kxpdrf1fvasdf2fs248tg1h9:src/main.ts:FILE"
        ],
        "sqlite_only": [],
        "field_mismatch": []
      },
      "callers_note": null,
      "callee_edges": {
        "livegraph": 0,
        "sqlite": 0
      },
      "callees": {
        "livegraph_only": [],
        "sqlite_only": [],
        "field_mismatch": []
      },
      "callees_note": null
    },
    {
      "symbol": "repo_01kxpdrf1fvasdf2fs248tg1h9:src/shapes.ts:FILE",
      "caller_edges": {
        "livegraph": null,
        "sqlite": 0
      },
      "callers": {
        "livegraph_only": [],
        "sqlite_only": [],
        "field_mismatch": []
      },
      "callers_note": "livegraph_panic",
      "callee_edges": {
        "livegraph": null,
        "sqlite": 0
      },
      "callees": {
        "livegraph_only": [],
        "sqlite_only": [],
        "field_mismatch": []
      },
      "callees_note": "livegraph_class=Partial"
    }
  ]
}
```

Internal consistency checks (arithmetic over the artifact above). **Canonical edges (the review-2 magnitude):**
`union_edges 9 = scip_only 7 + pipeline_only 0 + shared 2`; `livegraph_total 9` / `sqlite_total 2` → richness
4.5×. **Projections vs canonical (the review-2 dedup):** the `13` projection incidences
(`callers_livegraph_only 7 + callees_livegraph_only 6`) reduce to `scip_only 7` canonical edges — 6 edges are
double-witnessed (in both a caller- and a callee-projection) and `main.ts:FILE→Circle` is witnessed ONLY via
`Circle`'s caller-projection (its own callee-projection panicked). **Unknown ≠ zero:** `unmeasured_symbols` 2
(caller) + 2 (callee) = `livegraph_unanswerable 4` = 3 `livegraph_panic` + 1 `livegraph_class=Partial`.
**Rollup arithmetic:** `callers_livegraph_only 7` = 1 (constructor) + 4 (radius) + 2 (Circle);
`callees_livegraph_only 6` = 2 (makeCircle) + 2 (area) + 1 (constructor) + 1 (describe).

### 5.5 What only the monorepo run can answer

The superset-or-both-ways answer for the **deployment target** (the 160k-file polyglot monorepo, ROADMAP
NOW §"FIELD VALIDATION") is unresolved here. Run B proves the instrument works and gives a directional hint
(SCIP superset via semantic edges; watch the FILE-node panic class), but on a real repo with a live
producer the monorepo can surface the classes this 2-file fixture CANNOT: **pipeline-only** edges
(compilation-skipped / producer-skipped files; non-TS partitions SCIP does not cover) and real
**identity/key mismatch** at scale. Reproducible procedure: run the daemon with a provisioned
`scip-typescript` (Node-18 wrapper per SCIP-UNRESOLVED-CALL-PROBE-1 §2.1) so `livegraph-refresh` loads
partitions, set `RMAP_CALLGRAPH_DIFF=<dir>`, then issue one `orient`/`callers` per repo to emit
`<dir>/callgraph-diff.json`; classify by the §5.3 axes.

### 5.6 Iteration-1 revisions applied (review-0 REVISE list)

1. **HONESTY (unknown ≠ zero):** error/unanswerable/panic edge counts are now `None` → serialized `null`
   per symbol; `totals` split into `edges` (measured) + `unmeasured_symbols` (count). Visible in the §5.4
   artifact (the 2 FILE symbols) and pinned by named tests (§5.1). Schema bumped v1→v2.
2. **EXHAUSTIVE classification:** all 8 divergent symbols, each with DIRECTION / CAUSE (cited) / MAGNITUDE
   (§5.3 table) — no sampling.
3. **RETAINED evidence:** raw artifact + transcript + reproduction recipe + isolation proof inline (§5.4).
4. **Named-test gaps closed:** gate-control test toggles `RMAP_CALLGRAPH_DIFF` (off⇒no artifact, on⇒named
   dir); determinism test proves byte equality across two runs; enabled-gate test proves the stored
   GREEN/RED verdict is identical on/off (§5.1).
5. **Chunked full workspace suite** (53 crates, 0 failures) + corrected counts (diff.rs 917 lines / 17
   tests; the earlier "~430 / 6" and "702 / 11" figures are superseded). Finding #0 recorded prominently
   (§5.0), ratified as LIVEGRAPH-PARTIAL-FIX-1.

### 5.7 Iteration-2 revisions applied (review-1 REVISE list)

review-1 verdict was **revise** (instrumentation accepted: env-gated, off by default, read-only, unknown≠null
preserved; the reviewer re-ran gate-control / determinism / stored-verdict / unknown-as-null and they passed).
The 4 required changes:

1. **RETAIN the ACTUAL raw evidence, byte-for-byte (review-1 #1).** review-1: §5.4 called the JSON
   "verbatim" but replaced the stable-key prefix with `…`, and the transcript was summarized. FIXED: run B
   was **re-executed this iteration** against the fresh release `rmapd`; §5.4 now inlines the GENUINE
   6610-byte `callgraph-diff.json` with FULL keys (no `…`), the byte-for-byte `responses.ndjson` result
   lines, and the byte-for-byte `stderr.log`. Isolation re-proven (operator registry SHA-1 unchanged;
   fixture absent from it; no orphan). New run ⇒ new ULID `snapshot_uid`
   (`repo_01kxp9nzbxs5ycgycweqgqbrr2/…`); the classification is unchanged (same fixture + SCIP).
2. **TEST both verdict branches (review-1 #2).** review-1: `stored_verdict_unchanged_whether_emission_on_or
   _off` proved only the RED case (`build_fixture(true)`). FIXED: it now drives BOTH the faithful-mirror
   fixture (⇒GREEN) and the dropped-CALLS-edge fixture (⇒RED) through the real serve-ladder store, with
   emission ON and OFF, asserting the STORED verdict is identical AND correct in every arm and that the ON
   arm emits (§5.1, via the `assert_stored_verdict_unchanged` helper).
3. **CORRECT the panic certainty claims (review-1 #3).** review-1: `diff.rs` said the shipped cert "never
   hits" the panic (contradicting §5.0's data-dependent framing), and §5.0 said "any exhaustive walk WILL
   hit this on real data" (over one fixture). FIXED: `diff.rs` (`lg_side` doc + module header + rollup doc)
   now states the shipped cert's exposure is DATA-DEPENDENT (escapes only when a divergence precedes the
   affected `AstFileScope` FILE symbol; a P1 gap until LIVEGRAPH-PARTIAL-FIX-1); §5.0's consequence is now
   conditional ("on any repo whose corpus contains an affected FILE symbol … cannot be assumed panic-free
   until the fix").
4. **REPORT the edge shares (review-1 #4).** review-1: §5.3 had counts + a richness ratio but not the
   contract's "share of total edges" per class. FIXED (in iteration 2): §5.3 added a share table.
   **⚠ SUPERSEDED by iteration 3 / §5.8:** iteration 2's denominator **D = 17** and **SCIP-only 76.5% /
   shared 23.5% / richness 4.25×** SUMMED the caller- and callee-PROJECTIONS of each edge and thereby
   double-counted every edge with both endpoints in the corpus (review-2's blocking finding). The corrected
   CANONICAL-edge magnitude is **D = 9, SCIP-only 7 (77.8%), pipeline-only 0.0%, shared 2 (22.2%), richness
   4.5×** — see §5.3 (rewritten) and §5.8.

Count reconciliation (review-1 #5 carried forward): diff.rs was **952 lines / 17 `#[test]`s** at iteration 2
(was 917 at iteration 1; +35 for the conditional-panic wording + the two-branch enabled-gate test helper).

### 5.8 Iteration-3 revisions applied (review-2 REVISE list)

review-2 verdict was **revise** — the instrumentation itself was ACCEPTED (OBSERVED: env-gated, off by
default, read-only, verdict unchanged; the reviewer re-ran `cargo fmt` + all 26 focused tests incl.
gate-off, deterministic emission, unknown-as-null, and BOTH GREEN/RED verdict-preservation branches, and
confirmed the inlined JSON byte-identical to the retained artifact). Two blocking gaps remained; both fixed:

1. **Edge-level magnitude by CANONICAL directed edge (review-2 #1 + #2 — the blocking gap).** review-2:
   §5.3's `D=17` summed `9 caller rows + 8 callee rows`, two PROJECTIONS of the same edges, so every
   mirrored edge (e.g. `makeCircle→Circle.constructor`, seen once under `makeCircle`'s callees AND once
   under `Circle.constructor`'s callers) was double-counted; `13 SCIP-only`, `76.5%`, `4.25×` were
   projection-incidence metrics, not the contract's share of total directed edges. FIXED in CODE: `diff.rs`
   adds `EdgeViews` (each witness's caller- + callee-projection, keyed by canonical `(caller_key,
   callee_key)` with multiplicity) → `EdgeViews::canonical` MERGES the two per identity (MAX, so a
   both-projections edge counts ONCE; an edge whose one projection panicked is recovered from the other —
   unknown ≠ 0) → `edge_magnitude` classifies `scip_only`/`pipeline_only`/`shared`/`union_edges`. The
   artifact carries a new `canonical_edges` block (schema `v3`); the per-direction `totals` are RENAMED
   `projection_incidences` (`edges`→`incidences`) and re-labeled as diagnostic, never distinct edges.
   §5.3 recomputed: **D = 9, SCIP-only 7 (77.8%), pipeline-only 0 (0.0%), shared 2 (22.2%), richness 4.5×.**
2. **Named test for the dedup (review-2 #3).** `canonical_edges_merge_projections_dedup_multiplicity_and_
   unmeasured_not_zero` proves an edge seen from BOTH projections counts once (7 projection incidences → 4
   canonical edges), repeated-edge multiplicity is preserved (MAX, not summed to 4×), and an edge recovered
   from its sole MEASURED projection is not zeroed by the unmeasured one. `edge_magnitude_classifies_…`
   pins the class split + the `D = sum` invariant. The `drop_calls` fixture test now asserts the single
   dropped edge is **2 projection incidences but 1 canonical `scip_only` edge** (the dedup on real data);
   the faithful-mirror test asserts the mirrored edge is **1 shared** canonical edge.
3. **Retain the COMPLETE transcript, byte-verified (review-2 #4).** review-2: §5.4 inlined only the 3
   result lines; the full `responses.ndjson` (incl. 21 progress frames) lived only under ephemeral
   `/private/tmp`. FIXED: run B was **re-executed** against the fresh release `rmapd` (new v3 code), and all
   four raw files — `requests.ndjson`, the COMPLETE `responses.ndjson` (24 lines), `stderr.log`, and the
   v3 `callgraph-diff.json` — are retained in `.agent-manager/slices/RECON-SPIKE-1/runs/runB/` and
   byte-verified (`cmp -s`, each `BYTE-EQUAL`). §5.4 inlines the v3 artifact byte-for-byte (verified
   identical to the retained file) + the result lines + stderr, and points to the retained complete files.
4. **Count reconciliation (review-2 #5 carried forward).** diff.rs is now **1275 lines / 19 `#[test]`s**
   (was 952 / 17; +323 lines / +2 tests for the canonical-edge machinery, the honesty rename, and the two
   canonical tests). Full chunked workspace suite re-run this iteration: 53 crates, 0 failures (see the
   build report). The superseded iteration-2 figures are marked in §5.7 #4.

### 5.9 Iteration-4 revisions applied (review-3 REVISE list — the closing three)

review-3 verdict was **revise** — the instrumentation was ACCEPTED (OBSERVED: `git status` = the 3 in-scope
entries only; the review-2 canonical-edge correction implemented correctly [7 SCIP-only / 0 pipeline-only /
2 shared / union 9]; `cargo fmt --all -- --check` clean; 28 focused `callgraph_cert` tests pass; the four
retained run-B files byte-identical to the `/private/tmp` captures; the inlined JSON byte-identical to the
7090-byte artifact). Three closing corrections + a re-run, all applied:

1. **Precondition-failed measurement fields → `null`, never zero/empty (review-3 #1 — the blocking honesty
   defect).** review-3: `degenerate()` serialized a precondition failure with `corpus_size: 0`,
   `divergent_symbol_count: 0`, a zeroed `canonical_edges`, a zeroed `projection_incidences`, and an empty
   `symbols` — measured-zero shapes for values NO corpus produced (conflicts with VISION + the honest-null
   requirement). FIXED in CODE: `CallgraphDiffReport`'s SIX measurement fields are now `Option` — `Some(..)`
   on the measured path (byte-identical serialization), `None` → **`null`** on the degenerate path (this
   includes `rollup`, extending the reviewer's enumerated five to their logical closure — `rollup` is the
   same corpus-measurement class). It COMPLETES v3's own "unknown ≠ zero" contract (already applied to the
   per-symbol `caller_edges`/`callee_edges` in v2) at the top level; it is a bug-fix WITHIN v3, NOT a schema
   bump — bumping would force regenerating the reviewer-verified MEASURED artifact (operator: do not disturb
   the 7/0/2/9 data), and that measured shape is byte-UNCHANGED (`Some(x)` == `x`). **Named
   serialized-artifact test:** `degenerate_precondition_serializes_measurements_as_null_never_zero_or_empty`
   drives the real `emit_report` write path and asserts all six fields render JSON `null` (never `0`/`{}`/
   `[]`). Reachability note: the cert build is gated on a resident LiveGraph in production, so the degenerate
   path is defensive — run A produced NO artifact because `maybe_emit` was never reached — hence the
   serialized UNIT test (not a live daemon run) is its correct validation.
2. **RED/divergence equivalence scoped to `precondition == null` (review-3 #2).** review-3: "RED ⟺ ≥1
   divergent symbol" (`diff.rs` module header + §5.1) is FALSE on the degenerate path — it emits RED with
   zero (now `null`) divergent symbols. FIXED: the `diff.rs` module header and §5.1 now scope the cross-check
   to the MEASURED path; on the degenerate path the verdict is RED (the fallback) but `divergent_symbol_count`
   is `null`, so the equivalence does NOT apply — RED-with-`divergent_symbol_count: null` is the honest
   degenerate shape, pinned by the serialized-artifact test above.
3. **`field_mismatch` relabeled; zero identity/key-mismatch argued from the absence of unmatched SQLite-side
   edges (review-3 #3).** review-3: §5 conflated `field_mismatch` (in CODE: SAME stable key, DIVERGENT
   enrichment) with identity/key mismatch (the §2 contract's SAME logical edge under DIFFERENT keys). FIXED
   in §5.3: the DIRECTION aggregate + the magnitude-table row now derive **zero CAUSE (D)** from
   `canonical_edges.pipeline_only = rollup.callers_sqlite_only = callees_sqlite_only = 0` (a real key
   mismatch is an UNMATCHED pipeline-side edge → nonzero `sqlite_only`; none exists → none orphaned under a
   divergent key), CORROBORATED by the cited XPART-PROVE-1B byte-equal-keys source — NOT from `field_mismatch`.
   `field_mismatch = 0` is now stated as a DISTINCT, separately-zero class (same-key enrichment divergence).
   The 7/0/2/9 counts and the canonical methodology are UNCHANGED (operator: reviewer-verified — do not
   disturb); only the LABEL and the cited EVIDENCE changed.
4. **Re-run + report (review-3 #4).** `cargo fmt --all -- --check`, the focused `callgraph_cert` tests,
   `cargo clippy -p repo-graph-daemon-runtime`, the chunked workspace suite, and the applicable isolated
   artifact check were re-run — raw statuses in the build report. diff.rs is now **1323 lines / 19
   `#[test]`s** (was 1275 / 19; **+48 lines measured** — the six measurement fields wrapped `Option` with
   `Some(..)`/`None` across the `collect`+`degenerate` paths, the rewritten serialized-artifact degenerate
   test, and the module-header/doc-comment RED⟺divergence-scoping + `field_mismatch` wording; the earlier
   "+33" undercounted the same change set, corrected iteration 5 per review-4. Test count UNCHANGED — the
   degenerate test was REWRITTEN, not added). No change touched the verdict authority, the serving path, or
   any existing storage/serving schema;
   run B's reviewer-verified measured artifact + transcript in `runs/runB/` are left intact (the code change
   is degenerate-path-only, which run B does not exercise).

### 5.10 Iteration-5 revision applied (review-4 — reporting-only count correction)

review-4 verdict was **revise**, but the instrumentation, the iteration-4 fixes (degenerate `null`
measurements, RED⟺divergence scoping, `field_mismatch` relabel), and the classification data were all
ACCEPTED (OBSERVED, review-4: changes limited to the three in-scope files; degenerate measurements serialize
as `null`; `cargo fmt` + all 28 focused `callgraph_cert` tests pass; the retained artifact reports canonical
edges 7 SCIP-only / 0 pipeline-only / 2 shared / 9 union). ONE reporting defect remained:

1. **`diff.rs` line count corrected to the MEASURED value (review-4 — the sole required change).** §5.1/§5.9
   and `build-4.md:20` stated `diff.rs` = **1308 lines**; the file is actually **1323 lines** (`wc -l`,
   re-verified this iteration), so the iteration-4 delta is **+48 from the iteration-3 baseline 1275**, not
   the stated +33 — a reporting UNDERCOUNT of the SAME iteration-4 change set (the six measurement fields
   wrapped `Option` with `Some(..)`/`None` across `collect`+`degenerate`, the rewritten serialized-artifact
   degenerate test, and the module-header/doc-comment RED⟺divergence-scoping + `field_mismatch` wording).
   FIXED: §5.1, §5.9, and `build-4.md:20` now read **1323 lines / 19 `#[test]`s (+48 measured)**. The
   `#[test]` count is UNCHANGED at 19 (the degenerate test was rewritten, not added).

**No code, artifact, or verdict changed** — this is a documentation-count reconciliation only. Per review-4,
no validation rerun is required when only these reporting corrections are made; the iteration-4 validation
(fmt / clippy / 28 focused + chunked-workspace tests / release build / isolated run B / dogfood, all green
and reviewer-spot-checked) carries forward unchanged. The reviewer-verified 1323-line `diff.rs`, the 7/0/2/9
canonical classification, and run B's retained evidence in `runs/runB/` are untouched.
