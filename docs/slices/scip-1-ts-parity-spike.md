# SCIP-1: TypeScript SCIP Parity & Operational Spike

Slice ID: SCIP-TS-PARITY-SPIKE-1
Status: PARTIAL — Run 1 complete (engine + api on FRAKTAG); see
`docs/audits/scip-ts-parity-spike-1/findings.md`. Verdict GO for TS. Remaining:
precise CALLS parity (M1), M3 correlation, M4b edit-churn, secondary indexers
(scip-clang, rust-analyzer).
Decides: the deferred decisions in `docs/architecture/adr/adr-extraction-substrate-scip-first.md`
Track: Extraction Substrate Pivot

## Purpose

Produce **evidence** to settle the operational decisions the ADR deferred, and
to de-risk its riskiest assumptions, before any IR or cache design is committed.
This is a throwaway measurement harness, not production code.

The ADR cannot responsibly choose a refresh model, partition granularity, cache
format, or trust grading without measured facts. This spike produces those facts.

## Scope

### In scope
- Read-only measurement on real repositories.
- A throwaway harness that runs SCIP indexers and derives a comparison call graph.

### Out of scope
- Production ingestion code, the canonical IR, the warm-cache implementation,
  command migration. Those follow the spike and are informed by it.
- Any schema change or daemon change.

## Targets

**Primary (ingestion-design risk):** one real, buildable TypeScript repo with
`node_modules` installed and a valid `tsconfig.json`. Candidate corpus:
`glamCRM`, `fraktag`, or `amodx`. Indexer: `scip-typescript` (most mature).

**Secondary (indexer-maturity risk), lighter assessment:**
- `scip-clang` on one corpus C repo (`swupdate` or `nginx`) with
  `compile_commands.json` — the entire C/Linux ambition depends on this indexer.
- `rust-analyzer --> SCIP` on repo-graph's own Rust crates (self-host path).

## Measures (exact)

### M1 — Parity / resolution
- Run `scip-typescript`; derive a call graph from occurrences + enclosing ranges.
- For N >= 50 sampled symbols (mix of exported, file-local, and methods),
  compare derived callers/callees against current `rmap` output.
- Record: resolution rate (resolved call sites / total call sites); edges added
  vs current; edges missed vs current; false edges. Hand-verify >= 20 edges
  against source for ground truth.
- **Pass signal:** resolution materially exceeds the current ~20-33%; no
  systematic class of false edges.

### M2 — Call-vs-reference derivation
- For sampled reference occurrences, measure how often a CALL can be
  distinguished from a non-call reference using SCIP data alone (roles +
  enclosing range) vs requiring AST inspection.
- Record: % distinguishable from SCIP alone; characterize cases needing AST.

### M3 — AST <-> SCIP correlation reliability
- Take one value-layer fact already produced by the existing extractor
  (cyclomatic complexity per function, or one boundary site type).
- Attempt to key each fact to a SCIP symbol by source range.
- Record: correlation success rate; off-by-one / range-mismatch rate; failure
  characterization.

### M4 — Symbol stability across re-index (load-bearing risk)
- Run `scip-typescript` twice: (a) no source change; (b) after a trivial edit to
  one file.
- Diff symbol IDs. Record: global-symbol stability; `local <n>` churn rate;
  whether synthesized stable keys (path + qualified name + signature) are
  necessary and sufficient to survive re-index.
- **This decides whether the canonical-stable-key mapping is mandatory.**

### M5 — Operational timing and size (decides refresh model + partition size)
- Cold full-index wall-clock for the repo.
- Per-`tsconfig`-project / per-package re-index wall-clock (the partition-rerun
  cost).
- Single-file-edit re-index wall-clock; does `scip-typescript` support cheap
  incremental re-index at all?
- On-disk SCIP index size; estimated parsed in-memory size.
- Behavior with missing `node_modules` / broken `tsconfig` (failure mode and
  degradation quality).

### M6 — Consumption mode: batch SCIP vs live LSP
- Given existing `rgr enrich` LSP integration and SE-1's client design, compare
  batch-SCIP latency against an equivalent tsserver/LSP live-query latency for a
  small incremental change.
- Informs whether the fast incremental path should use live LSP rather than
  batch SCIP.

## Definition of Done

- A measurement report (`docs/audits/scip-ts-parity-spike-1/`) presenting M1-M6
  results, each labeled per the Evidence Law (EXECUTED / OBSERVED / INFERRED /
  NOT RUN).
- An explicit, evidence-grounded recommendation for each deferred ADR decision:
  refresh model, partition granularity, cache-format direction, trust grading.
- A go / no-go / revise verdict on SCIP-first for TypeScript.
- A short separate note on `scip-clang` and `rust-analyzer --> SCIP` maturity
  (secondary targets).

## Risks

- `scip-typescript` version/behavior variance — pin and record the version.
- Harness mistaken for production — it is explicitly throwaway; ingestion is
  designed in SCIP-INGEST-IR-1, informed by but not reusing this code.

## Dependencies

- `node` + `scip-typescript` installed (TC-1 toolchain inventory relevant).
- A buildable TS repo with `node_modules` present.
- Secondary: `clang` + `scip-clang` + `compile_commands.json` (BC-1); Rust
  toolchain + `rust-analyzer`.

## Validation Commands (to be populated during execution)

- `scip-typescript index ...` (EXECUTED — record version + wall-clock)
- harness diff vs `rmap callers/callees` (EXECUTED — record M1 counts)
- All claims labeled per `agent_docs/validation.md`.
