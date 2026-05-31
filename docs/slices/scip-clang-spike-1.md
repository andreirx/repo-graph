# SCIP-CLANG-SPIKE-1: C/C++ SCIP Substrate Risk Spike

Slice ID: SCIP-CLANG-SPIKE-1
Status: PARTIAL — Run 1 done on leveldb; verdict GO (scip-clang real: 39 TUs -> 90
docs, 16,643 occ, 0 errors, 0.3s, compiler-grade). Operational nuances: build-env
setup + correct project rooting. See `docs/audits/scip-clang-spike-1/findings.md`.
Decides: whether SCIP-first is real for C/C++ (gates the C/Linux ambition in
`docs/architecture/adr/adr-extraction-substrate-scip-first.md`)
Track: Extraction Substrate Pivot — multi-language risk
Order: FIRST of the two risk spikes (highest risk retired first, per decision)

## Purpose

Determine whether `scip-clang` can produce compiler-grade C/C++ facts on a real
repository with a `compile_commands.json`, or whether the C/C++ story needs
narrowing. `scip-clang` is the least mature of the major SCIP indexers and the
entire C/Linux ambition rests on it. This spike is a go/no-go on that ambition.

This is throwaway measurement, not production code.

## Targets

**Primary (clean small cmake C/C++):** `legacy-codebases/leveldb` (133 files,
C++, CMake) — `compile_commands.json` generated via
`cmake -DCMAKE_EXPORT_COMPILE_COMMANDS=ON`. Smallest clean cmake target.

**Secondary (scale + config):** one larger / config-heavy C repo
(`duckdb` CMake, or `nginx`/`sqlite`/`swupdate` via `bear -- make`) — only if the
primary passes. The Linux kernel is the ultimate target but explicitly NOT in this
spike (kernel config + build is too heavy for a first go/no-go).

## Prerequisites (themselves spike evidence — the build-environment dependency)

- `scip-clang` binary (prebuilt release from `sourcegraph/scip-clang`; no `go` on system).
- A C/C++ toolchain (`clang`) and a way to produce `compile_commands.json`
  (`cmake` export, or `bear`).
- The target repo configured/build-prepared enough to emit `compile_commands.json`.

Whether these can be satisfied at acceptable friction IS a measured result (C1).

## Measures (exact)

### C1 — Setup feasibility / build-environment friction
- Can `scip-clang` be obtained and run on this platform? Record how.
- Can `compile_commands.json` be produced for the target, and at what friction
  (toolchains required, configure/build steps, failures)? This quantifies the
  build-environment dependency the ADR flagged as the cost of SCIP-first for C.

### C2 — Index success and coverage
- Does `scip-clang` index the translation units without crashing?
- Documents / occurrences / definitions / references produced; document count vs
  source-file count (coverage). Record crashes/skips per TU.

### C3 — Resolution quality (the compiler-grade payoff)
- System/library header resolution; cross-TU symbol resolution; include-path- and
  define-dependent resolution (the `#ifdef` payoff that syntax-only cannot do).
- Compare against `rmap`'s current C/C++ extraction on the same repo (leveldb is
  already in the `rmap` registry): resolution rate, edges added/missed.

### C4 — Symbol stability (mirror TS M4)
- Local vs global symbol ratio; determinism across identical re-index; whether the
  canonical-stable-key mapping conclusion from TS holds for C/C++.

### C5 — Timing and size
- Per-TU / whole-repo index wall-clock; on-disk index size. Per-partition rerun
  cost where partition = translation unit / TU cluster.

### C6 — Config reality
- Confirm `scip-clang` reflects the single configuration in `compile_commands.json`
  (single-config-correct), and that covering N configs = N indexing runs /
  partitions. Sets honest expectations for the kernel.

## Definition of Done

- Report at `docs/audits/scip-clang-spike-1/` with C1-C6, Evidence-Law labels.
- A **go / no-go / narrow** verdict for the C/C++ substrate, with explicit
  statement of build-environment friction.
- If no-go or narrow: a recommended scope reduction (e.g., C/C++ supported only
  where `compile_commands.json` is available; kernel deferred).

## Risks

- `scip-clang` maturity — may crash or under-resolve on real code.
- `compile_commands.json` generation friction (no cmake/bear, build failures).
- Large TUs / generated headers (the 1MB-guard concern from current extraction).

## Dependencies
- `scip-clang` (download), `clang`, `cmake` or `bear`. Reader: reuse the Node +
  `protobufjs` SCIP reader from SCIP-TS-PARITY-SPIKE-1 (`/tmp/scip-spike/`).
