# XPART-PROVE-1: Cross-Partition Traversal Semantics (Stage B, ST3)

Slice ID: XPART-PROVE-1
Status: **EXECUTED (2026-05-31), split.**
- **XPART-PROVE-1A (answer-class semantics) — PASS.** Six `callers` cases over the
  source-aligned `api-src.scip` capture each returned a typed answer class
  (Exact / Partial / Unavailable / Stale) with an explicit reason; no silent-empty path.
- **XPART-PROVE-1B (dist↔src export-symbol reconciliation) — REQUIRED / OPEN.** The
  published-interface `api-dist.scip` capture shows raw SCIP equality misses 95/95 real
  `api → engine` references; an export-surface reconciliation layer is still owed.

Answer-class defaults ratified (xref-exact where sufficient, else partial-with-explicit-
degradation; load-on-demand opt-in only; `callers` only; xref carries per-partition counts).
**ST3 is NOT retired by 1A alone.**
Depends: INGEST-CORE-1 (the in-memory `PartitionIr` + canonical identity exist),
SCIP-TS-PARITY-SPIKE-1 (engine.scip captured + reused; the spike's api.scip is discarded — see Target)
Track: Extraction Substrate Pivot — Stage B (`docs/architecture/scip-migration-plan.md`)
Addresses (does NOT retire by 1A): **ST3 — cross-partition traversal completeness.** The risk
is *silent incompleteness*: an answer that looks complete but is not, because a relevant
partition was not resident. 1A retires the **answer-class half** (honest, machine-readable
degradation). **ST3 stays open until 1B closes the dist↔src cross-partition identity half.**

## Ratified (2026-05-31)

- **Default behavior:** **(c) partial-with-explicit-degradation**, plus **(a) xref-exact where
  the global xref is sufficient**. **(b) load-on-demand is an explicit opt-in only**;
  **(d) forced eager load rejected.** No auto-load by default. Both invariants kept: never a
  silent incomplete answer; never unbounded memory/load.
- **Granularity model (class + granularity metadata):** an answer carries an `AnswerClass`
  (`Exact` / `Partial` / `Unavailable` / `Stale`) AND a granularity:
  - `Exact` + `PartitionSummary` — exact per-partition reference **counts** + defining
    partition, from the always-resident xref, WITHOUT loading any referenced partition.
  - `Partial` + `CallerDetail` — caller **identities** only for resident referencing
    partitions; non-resident referencing partitions are listed as missing (their counts still
    exact from xref). A summary-exact answer must NOT be presented as full caller-identity truth.
- **Query surface:** `callers` ONLY this slice. `path` deferred to a later
  query-migration/runtime probe (this slice locks answer-class semantics, not traversal richness).
- **Xref granularity:** per-symbol **defining partition** + per-partition **reference counts**
  (which partitions reference X, and how many each). Enables `Exact`/`PartitionSummary` without
  loading; does not equal full caller identity.

### Required probe cases — `callers` of an engine symbol referenced by api
1. **Both loaded** → `Exact` / `CallerDetail`: per-partition counts + caller identities complete.
2. **api loaded, engine unloaded (xref resident)** → per-partition **count** is exact
   (`PartitionSummary`); caller *identities* for engine's own callers are missing → overall
   `Partial` (engine listed missing); api caller identities present. **Not empty.**
3. **engine loaded, api unloaded (xref resident)** → engine caller identities present; api
   caller identities missing → `Partial` (api listed missing); counts still exact. **Not empty.**
4. **xref absent / stale** → `Unavailable` (no xref) or `Stale` (epoch mismatch). **Not empty.**

No code path may return a silent empty / unmarked result.

## First-class finding (2026-05-31): cross-partition symbol divergence (src vs dist)

XPART-PROVE-1 surfaced, *before* measuring answer classes, that **cross-partition identity has
two legitimate symbol surfaces**:
- the **provider** partition indexed from **source** —
  `scip-typescript npm @fraktag/engine 0.1.0 src/index.ts/Fraktag#` (this is engine.scip /
  INGEST-CORE-1);
- the **consumer** partition resolving the dependency through the **published interface** —
  `scip-typescript npm @fraktag/engine 0.1.0 dist/index.d.ts/Fraktag#` (api `import`s
  `@fraktag/engine`, whose package `types` = `dist/index.d.ts`).

Same exported entity, **different SCIP symbols.** Therefore **the global xref CANNOT be based on
raw SCIP symbol equality alone.** (The originally-captured `api.scip` is worse still — indexed
with engine unlinked, so the import was unresolved/local; it is neither view and is discarded.)

### The slice now spans TWO linked risks (1A retires the first; 1B still owes the second)
1. **Answer-class semantics** — Exact / Partial / Unavailable / Stale; no silent empty.
   **Retired by 1A.**
2. **Cross-partition symbol reconciliation** — raw SCIP equality works ONLY in source-path mode;
   published-interface mode REQUIRES an **export-surface reconciliation layer** aliasing a
   published declaration symbol (`dist/index.d.ts/Fraktag#`) to the source definition symbol
   (`src/index.ts/Fraktag#`) via the package export map. The cross-partition key is then NOT a
   raw SCIP string but `(package, exported-name, {source-def-symbol, published-decl-symbol},
   partition)`. **Open — XPART-PROVE-1B.**

### Split (adopted)
- **XPART-PROVE-1A** — answer-class semantics under the **source-path** api capture (raw symbols
  already aligned; proves the answer-class machinery works). This probe (`xpart-probe`).
- **XPART-PROVE-1B** — **dist↔src export-symbol reconciliation** (published-interface capture;
  proves raw equality insufficient and builds the alias layer). Follow-on.

**ST3 is NOT retired by 1A alone.** Source-path success proves the answer-class contract; it does
NOT retire the published-interface identity-reconciliation half.

### Two captures (both kept — evidence for both risks)
- **`api-dist.scip` (published-interface)** — engine resolved via `dist/index.d.ts`; evidence
  that raw symbol equality fails cross-partition. Drives 1B.
- **`api-src.scip` (source-path)** — tsconfig `paths @fraktag/engine → ../engine/src/index.ts`;
  symbols match engine.scip; the controlled comparison for 1A's answer-class proof.

## The ST3 risk this slice addresses (1A retires the answer-class half; ST3 not retired)

Whether cross-partition queries (`callers`, `path`) can be answered **without silent
incompleteness** when not all referenced partitions are resident. The deliverable is not
"can we traverse across partitions" — it is a **written answer-class contract**: every query
result falls into a *declared* class carrying an explicit, machine-readable completeness
marker. **Forbidden: returning empty (or a clean-looking partial) when relevant unloaded
partitions may contain answers.** 1A proves this contract on a source-aligned capture; it does
**not** retire ST3, because the dist↔src cross-partition identity problem (1B) remains open.

**Throwaway measurement, not production code.** NO runtime, NO generalized LiveGraph residency
manager, NO eviction, NO persistence, NO query migration. This is a two-partition in-memory
model + an always-resident global cross-reference index + the answer-class contract, and a
probe that exercises it. If it grows past that, it has failed its purpose.

## Target (two partitions; TWO api captures with distinct roles)

The FRAKTAG TS workspace — two real partitions with a known `api → engine` cross-partition edge:
- **`@fraktag/engine`** — `engine.scip` (52 docs, 3703 defs); the INGEST-CORE-1 partition,
  indexed from source.
- **`@fraktag/api`** — the consumer. The original `api.scip` is **discarded** (indexed with
  engine unlinked → the import resolved local/unresolved; it is neither a source nor a published
  view). Two purpose-built captures replace it:
  - **`api-src.scip`** (source-path: tsconfig `paths @fraktag/engine → ../engine/src/index.ts`)
    — api references resolve to engine **src** symbols matching `engine.scip` **for the named
    surface** (measured 78/95; the other 17 are anonymous inline type-literal members whose
    `typeLiteralNN` numbering is compilation-unit-relative and diverges even here — see
    XPART-PROVE-1B). The controlled capture for the **1A** answer-class proof; the named target
    is source-aligned.
  - **`api-dist.scip`** (published interface: engine resolved via its package `types`
    `dist/index.d.ts`) — api references carry `dist/index.d.ts/...` symbols `engine.scip` never
    defines. The real consumer view; the **1B** dist↔src divergence proof.
- **Known edge `api → engine`** — ~95 `api` references into `engine`. The probe re-measures it
  exactly: under `api-src.scip` the **named** surface is source-aligned (measured 78/95; a
  pickable cross-partition target exists), while 17 anonymous type-literal members diverge even
  here; under `api-dist.scip` all 95 are DIVERGENT (0 source-aligned → 1B required).

First build step (the leveldb lesson): re-confirm each capture is valid (non-externalized)
before measuring; do **not** reuse the discarded `api.scip`.

## The answer-class contract (the core deliverable — SPECIFIED, not discovered)

Every cross-partition query result MUST be exactly one declared class, per query surface:

1. **Exact** — every partition the answer depends on is either resident OR fully covered by
   the always-resident global xref summary. The answer is complete.
2. **Partial** — answer carries the resident-partition facts AND an explicit, machine-readable
   list of the missing/unloaded partitions whose contents could change the answer.
3. **Unavailable** — the target symbol or a required partition is not loaded and not indexed
   (no xref entry); the query cannot be answered, stated explicitly.
4. **Stale** — a partition / xref epoch mismatch: the xref references a partition version that
   is no longer the resident one; the answer is flagged stale.

**Forbidden:** silently returning empty (or an unmarked partial) when relevant unloaded
partitions may exist. A `Partial`/`Unavailable`/`Stale` result must be *distinguishable by a
consumer* from an `Exact` empty result.

## The probe

Minimal in-memory model (reuses `repo-graph-ir` + `repo-graph-scip-ingest`):
- Two `PartitionIr`s: `engine`, `api` (ingested via `ingest_partition`).
- An **always-resident global xref index** (small, partition-level):
  - `canonical-key → defining partition`
  - `canonical-key → set of referencing partitions`
  built from both partitions' defs + edges. It can answer *partition-level* questions
  (where is X defined; which partitions reference X) WITHOUT loading the referenced partition;
  *symbol/function-level* answers (the actual caller functions) require partition residency.

Run at least:
1. `callers(engine_symbol)` evaluated from the `api` context — the cross-partition callers.
2. `path(api_entry → engine_symbol)` — if feasible on this fixture (api is one file).
3. **The unloaded-partition case (the real test):** evict/omit `engine`, keep `api` + the
   global xref, re-run (1). The result MUST degrade to a **declared class** (`Partial` listing
   `engine`, or `Unavailable`) with an explicit marker — **never silent empty**.

## The key decision to force (do NOT bury in implementation)

**Default cross-partition query behavior when a referenced partition is NOT resident** — pick
the default (surfaces may override, but the default must be chosen, not discovered):

- **(a) Exact-via-global-xref-only:** answer partition-level queries exactly from the always-
  resident xref; symbol/function-level queries that need a non-resident partition return
  `Unavailable`/`Partial`. No loading. Bounded memory; narrowest exactness.
- **(b) Load-on-demand:** load referenced partitions to answer exactly, then (optionally) evict.
  Exact; pays load latency + transient memory.
- **(c) Partial-with-explicit-degradation:** answer from resident partitions + xref, always
  emitting the `Partial` marker + missing-partition list. Honest, bounded, never blocks.
- **(d) Forced-eager-load:** always load all referenced partitions. Exact; maximum memory,
  defeats the partitioning memory benefit.

Trade-off axis: exactness vs memory/latency vs honesty. **Lean: (c) as the default**, with
(a) used for the queries the xref covers exactly, and (b) available as an explicit opt-in —
because (c) never silently misleads and never forces unbounded load, while (a) keeps the common
partition-level questions exact for free. But this is your call.

## Measures / Go-no-go

- Global xref index **size and build cost** bounded and measured (it is always resident).
- Each query returns the **correct answer class**; `Exact` results match hand-checked truth
  for the api→engine edge (N≥20 cross-partition references).
- The unloaded-`engine` case returns a **declared degradation marker**, verified
  machine-readable — never a silent empty/partial.
- **Retreat (ST3):** if stitching is too complex/incomplete to preserve trust → fall back to
  load-all-partitions-of-a-repo-per-query (sacrifice the memory benefit, keep correctness), or
  residency-scoped answers with explicit degradation. Taken explicitly, documented.

## Exit criterion (all must hold)

The probe passes only if it can: (1) produce a cross-partition answer; (2) detect when answer
completeness depends on an unloaded partition; (3) report that condition **explicitly**
(a declared non-`Exact` class); (4) have **no silent incomplete-answer path** — every
non-exact result is a typed, marked class.

## Out of scope (hard guardrails)

NO LiveGraph residency manager / eviction policy / memory ceiling; NO persistence / warm
cache; NO query migration of the real CLI surfaces; NO trust-model work; NO >2 partitions.
Two partitions, one always-resident xref index, the four-class contract, one probe.

## Sign-off decisions (RATIFIED 2026-05-31)

1. **Default answer behavior** — **(c) partial-with-explicit-degradation + (a) xref-exact where
   the always-resident xref is sufficient**; (b) load-on-demand is opt-in only; (d) forced
   eager load rejected.
2. **Query surfaces this pass** — **`callers` only.** `path` deferred to a later
   query-migration/runtime probe.
3. **Xref granularity** — partition-level **plus per-partition reference counts**, so the
   `callers` *count* is exact from the always-resident xref without loading a partition.

## References
- `docs/architecture/scip-migration-plan.md` (Stage B — XPART-PROVE-1, the answer-class contract)
- `docs/slices/ingest-core-1.md` (the `PartitionIr` + `ingest_partition` this reuses)
- `docs/audits/scip-ts-parity-spike-1/findings.md` (api.scip + engine.scip evidence)
