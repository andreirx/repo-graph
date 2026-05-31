# SCIP-RUST-SPIKE-1: Rust SCIP Substrate / Self-Host Risk Spike

Slice ID: SCIP-RUST-SPIKE-1
Status: PARTIAL — Run 1 done; verdict GO WITH CAVEATS (per-crate works, whole-workspace
panics, duplicate-symbol bug, 31.6s/crate, 72% locals). See
`docs/audits/scip-rust-spike-1/findings.md`.
Decides: whether SCIP-first is viable for Rust and for self-hosting repo-graph
(gates Rust in `docs/architecture/adr/adr-extraction-substrate-scip-first.md`)
Track: Extraction Substrate Pivot — multi-language risk
Order: SECOND of the two risk spikes (after SCIP-CLANG-SPIKE-1)

## Purpose

Determine whether `rust-analyzer --> SCIP` produces facts of sufficient
completeness to serve as the primary L0/L1 producer for Rust, validated on
repo-graph's own Cargo workspace (self-host — a codebase we know intimately, with
immediate dogfooding leverage). `rust-analyzer`'s SCIP export is a secondary
output of an LSP engine; its completeness is the open question.

Throwaway measurement, not production code.

## Target

`repo-graph/rust` — the Cargo workspace (`daemon-runtime`, `storage`, and sibling
crates). Self-host: repo-graph indexing its own Rust, where we can hand-verify
ground truth.

## Prerequisites

- Rust toolchain (`cargo`) and `rust-analyzer` with the `scip` subcommand
  (`rust-analyzer scip <path>`). Availability is itself C1-style evidence.

## Measures (exact)

### R1 — Setup feasibility
- Is `rust-analyzer` present with a working `scip` subcommand? Record version and
  invocation. Friction relative to TS (`scip-typescript`) noted.

### R2 — Index success and coverage
- Does `rust-analyzer scip` index the workspace without error? Documents /
  occurrences / definitions / references; document count vs `.rs` file count
  (coverage across all crates).

### R3 — Resolution quality and completeness (the open risk)
- Cross-crate resolution within the workspace; external crate (crates.io) symbol
  resolution; macro-expansion handling. Compare against `rmap`'s current
  repo-graph index (already in the registry): resolution rate, edges added/missed.
- Specifically probe known `rust-analyzer` SCIP-export gaps (macro-generated
  items, trait dispatch) — quantify, do not assume.

### R4 — Symbol stability (mirror TS M4)
- Local vs global ratio; determinism across identical re-index; whether the
  canonical-stable-key mapping conclusion holds for Rust symbol formats.

### R5 — Timing and size
- Whole-workspace and per-crate index wall-clock; on-disk index size.
  Per-crate = candidate partition; record per-crate rerun cost.

### R6 — Cross-partition by crate
- Confirm cross-crate references resolve via global SCIP symbols (the Rust analog
  of the validated TS `api -> engine` cross-partition edge).

## Definition of Done

- Report at `docs/audits/scip-rust-spike-1/` with R1-R6, Evidence-Law labels.
- A **go / no-go / narrow** verdict for the Rust substrate and for self-hosting,
  with explicit note on `rust-analyzer` SCIP-export completeness gaps.

## Risks

- `rust-analyzer` SCIP export completeness (macros, trait dispatch, generated items).
- Workspace size / analysis time.
- Version-coupling: SCIP completeness varies by `rust-analyzer` version — pin and record.

## Dependencies
- `cargo`, `rust-analyzer`. Reader: reuse the Node + `protobufjs` SCIP reader from
  SCIP-TS-PARITY-SPIKE-1 (`/tmp/scip-spike/`).
