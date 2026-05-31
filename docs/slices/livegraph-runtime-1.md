# LIVEGRAPH-RUNTIME-1: In-Memory Runtime Substrate (Stage C, slice 2)

Slice ID: LIVEGRAPH-RUNTIME-1
Status: **BUILT (2026-05-31) — crate `repo-graph-livegraph`, 8 tests green (the 7 D5 `callers`
cases + atomic swap).** D1 accept+swap (no indexers); D2 explicit load/unload (eviction policy
deferred); D3 per-partition epoch + `XrefEpoch` + contributing-epochs on answers; D4
`repo-graph-livegraph` (deps `repo-graph-ir` + `repo-graph-trust-model` only); D5 `callers` only via
the trust vocabulary. The build triggered TWO ratified `repo-graph-trust-model` amendments (residency
`missing_partitions`; `Partial` justified by non-`Fresh` freshness). Headless; no CLI / query /
persistence.
Depends: TRUST-MODEL-REBASE-1 (`repo-graph-trust-model`), INGEST-CORE-1 (`PartitionIr`),
XPART-PROVE-1A/1B (residency + answer-class model), REFRESH-PROBE-1 (epoch / two-speed contract),
STAGE-C-ENTRY-DECISION.
Track: Extraction Substrate Pivot — **Stage C, slice 2** (after TRUST-MODEL-REBASE-1, before
QUERY-MIGRATION-1).

## Purpose / the one risk this slice retires

Build the **in-memory LiveGraph runtime substrate** that holds partitions, manages **residency +
per-partition epoch**, keeps the **always-resident global xref**, and produces **trust-labelled
answers** (`AnswerEnvelope` via `repo-graph-trust-model`) — the substrate QUERY-MIGRATION-1 will
later sit on. The risk: can the runtime hold this state and label answers correctly (residency →
`AnswerClass`, epoch/refresh → `FreshnessState`, identity → degradation) **without** query
migration or persistence?

## Framing (ratified — what this slice is NOT)

```text
Use repo-graph-trust-model as the trust vocabulary.
No query migration yet.
No warm-cache persistence yet.
Runtime is in-memory only.
Partition residency + epoch state + answer-class degradation are first-class.
```

It does **not** touch the `callers`/`callees`/`path` CLI behavior — query migration comes AFTER the
runtime substrate exists. This slice builds the substrate + the trust integration, exposed only
through a **headless Test API** (Clean-Architecture: tests drive the runtime directly, not via CLI).

## What it IS / IS NOT

**IS:** an in-memory `LiveGraph` holding `PartitionIr`s by partition id; residency state; a
per-partition epoch with the REFRESH-B last-good/atomic-swap contract; the always-resident global
xref (symbol → defining partition, symbol → per-partition reference counts); and a headless API
that answers a cross-partition lookup returning an `AnswerEnvelope` built through
`repo-graph-trust-model`.

**IS NOT:** the CLI/query surface (QUERY-MIGRATION-1); a warm cache / disk format (later);
an indexer orchestrator (it is *fed* `PartitionIr`; it does not run scip-typescript / rust-analyzer);
value-level joins (VALUE-JOIN-1); an eviction-policy engine.

## Central design (grounded in Stage B)

- **Partition store** — `PartitionIr` (INGEST-CORE-1) keyed by partition id, in memory only.
- **Residency** — a partition is `Resident` or `NonResident`. Default behavior (XPART ratified):
  answer from resident partitions + the always-resident xref, degrading to `Partial`/`Unavailable`
  when a referenced partition is non-resident. **No auto-load by default**; explicit load/unload is
  available; the *eviction policy* is deferred (D2).
- **Epoch + atomic swap** — each partition carries an epoch. Refresh produces a NEW `PartitionIr`
  for that partition; the runtime serves the **last-good epoch** while a refresh is in flight,
  **atomically swaps** to the new epoch on success, and **keeps last-good on failure**
  (`FreshnessState::RefreshFailed`). The runtime **accepts** new partition data and swaps — it does
  NOT run indexers (D1). The two-speed / coalescing orchestration (REFRESH-PROBE-1) is upstream.
- **Always-resident global xref** — built from partition defs/refs; answers partition-level
  questions (where defined, which partitions reference X, counts) without loading; symbol/function
  detail requires residency. (XPART-PROVE-1A.)
- **Trust integration** — the runtime knows residency + epoch + identity bases, so it computes the
  `CompletenessInput` and builds the answer through `repo-graph-trust-model`: `classify_answer` →
  `(AnswerClass, QueryCompleteness)`; epoch/refresh state → `FreshnessState`; non-resident /
  unreconciled → `DegradationReason`. Answers are `AnswerEnvelope<T>`; **never an exact-empty for
  missing/stale state** (null ≠ empty, enforced by the constructors).

## The headless Test API (the deliverable surface)

A minimal, headless `LiveGraph` API exercised by tests (NOT the CLI). At least:
- `load_partition(id, PartitionIr)` / `unload_partition(id)` — residency control.
- `swap_partition(id, new_ir)` — atomic epoch swap (success path); `mark_refresh_failed(id)` —
  keep last-good (`RefreshFailed`).
- `callers(target, QueryGranularity) -> AnswerEnvelope<CallersAnswer>` — the trust-labelled
  cross-partition lookup (the XPART `callers` shape, now routed through `repo-graph-trust-model`).
  Demonstration that the runtime labels answers correctly; NOT wired to any CLI command.

## Ratified decisions (2026-05-31) — built as below

**D1 — refresh injection model.** (a) the runtime **accepts** a new `PartitionIr` and atomically
swaps (it never runs scip-typescript / rust-analyzer — orchestration is upstream / REFRESH's
concern); (b) the runtime owns refresh orchestration (shells out, coalesces, two-speed). *Lean:*
(a) — keeps the runtime in-memory and free of subprocess/indexer coupling; the producer is injected.

**D2 — residency / eviction.** Residency state + explicit `load`/`unload` are in scope; the default
is partial-with-degradation, no auto-load (XPART). Is an **eviction policy** (when to auto-evict
under memory pressure) in scope here or deferred? *Lean:* deferred — model residency state now,
defer the policy (it needs a memory model this slice does not build).

**D3 — epoch granularity.** (a) **per-partition** epoch (partitions refresh independently; matches
REFRESH per-partition + two-speed); (b) single global epoch. *Lean:* (a). The global xref tracks
the set of per-partition epochs it was built from (staleness detection, XPART `Stale`).

**D4 — crate placement.** A new crate `repo-graph-livegraph`, depending on `repo-graph-ir`
(`PartitionIr`) + `repo-graph-trust-model` (vocabulary) ONLY — **not** `repo-graph-scip-ingest` or
`scip` (the runtime is *fed* already-ingested IR; ingestion is upstream). *Lean:* new crate,
those two deps only. Confirm the name.

**D5 — answer surface scope.** Implement `callers` only this slice (the XPART must), routed through
`repo-graph-trust-model`; `path`/`callees` deferred to QUERY-MIGRATION-1. *Lean:* `callers` only.

## Out of scope (hard guardrails)

```text
No query migration (no callers/callees/path CLI behavior).
No warm-cache / persistence / disk format.
No indexer orchestration (no scip-typescript / rust-analyzer subprocess).
No value-level joins (VALUE-JOIN-1).
No eviction-policy engine.
```

## Definition of done

- `repo-graph-livegraph` (in-memory) holds partitions + residency + per-partition epoch + the
  global xref; deps limited to `repo-graph-ir` + `repo-graph-trust-model` (structurally verified).
- The headless API answers `callers` as an `AnswerEnvelope<...>` built through
  `repo-graph-trust-model`, with the residency/epoch cases producing the correct `AnswerClass` /
  `FreshnessState` / `DegradationReason` — no exact-empty for missing/stale state.
- Epoch contract tested: serve last-good during refresh; atomic swap on success; keep last-good on
  failure (`RefreshFailed`).
- Residency cases tested: both-resident `Exact`; non-resident referenced partition → `Partial`
  (missing listed) or `Unavailable`; never silent-empty.
- Headless tests only; no CLI wiring; no persistence.

## Exit criterion

The runtime substrate exists in memory, labels every answer through `repo-graph-trust-model`
(residency → class, epoch → freshness, identity → degradation), and honors the epoch contract —
all via a headless Test API. QUERY-MIGRATION-1 can then route the real query surfaces onto it; it
does not redefine the trust vocabulary or the runtime.

## References
- `docs/architecture/stage-c-entry-decision.md` (the Stage C order + vocabulary)
- `docs/slices/trust-model-rebase-1.md` (`repo-graph-trust-model`; the answer/freshness/identity types)
- `docs/slices/xpart-prove-1.md` (residency + answer-class model + global xref)
- `docs/slices/refresh-probe-1.md` (epoch / last-good / atomic-swap contract)
- `docs/slices/ingest-core-1.md` (`PartitionIr` the runtime holds)
