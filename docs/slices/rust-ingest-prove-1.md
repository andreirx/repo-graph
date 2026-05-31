# RUST-INGEST-PROVE-1: Rust SCIP Ingestion Support Boundary (Stage B)

Slice ID: RUST-INGEST-PROVE-1
Status: **EXECUTED (2026-05-31) — VERDICT GO WITH CAVEATS.** Per-crate Rust SCIP ingestion is
supportable as a degraded language; whole-workspace panics (unsupported); refresh =
**B-very-slow-async** (~29–32s/crate, never post-edit synchronous); identity ~94–96%
SCIP-synthesized fallback (no value-level AST join); maturity **PARTIAL/BETA**; no TS/C parity.
Verdict + written support boundary below; evidence `docs/audits/rust-ingest-prove-1/findings.md`.
**Stage B complete.**
Depends: INGEST-CORE-1 (`ingest_partition`), SCIP-RUST-SPIKE-1 (`docs/audits/scip-rust-spike-1`,
prior per-crate evidence), REFRESH-PROBE-1 (the B two-speed model + D2 thresholds Rust is measured
against).
Track: Extraction Substrate Pivot — Stage B (`docs/architecture/scip-migration-plan.md`). The
**last open Stage B risk** (ST1, ST3, refresh-model now bounded).

## Verdict (EXECUTED 2026-05-31)

**GO WITH CAVEATS.** Rust enters Stage C **only** as a **per-crate, async, SCIP-backed,
degraded-support (PARTIAL/BETA)** language. Whole-workspace export is **unsupported** (rust-analyzer
panics at ~32.5s on repo-graph). **No TS/C parity** — ~94–96% SCIP-synthesized fallback identity
(no value-level AST join), per-crate refresh ~29–32s p95. Cross-crate resolution works (other-crate
/ stdlib / external all resolve). N=3, stable, 0 panics.

**Refresh (D4 ratified) = B-very-slow-async, NOT C explicit-refresh.** Rust SCIP refresh is **never
synchronous and never post-edit blocking.** It runs as a **background per-crate refresh** with the
**last-good epoch served meanwhile**; answers depending on Rust SCIP state remain
**`Stale` / `PrecisionPending`** until the crate refresh completes. **Explicit manual refresh is an
operator fallback, not the default contract.**

### Rust support boundary (recorded)
- **indexing unit:** per crate only
- **whole workspace:** unsupported
- **refresh:** B-very-slow-async
- **identity basis:** SCIP-synthesized fallback dominant (~94–96%)
- **duplicate policy:** deterministic dedup + aliases/provenance
- **def-not-in-document:** tolerated only if bounded/surfaced and not corrupting public symbols or
  call-graph endpoints
- **no TS/C parity claim**

### Residuals (before any PRODUCTION maturity)
1. **public duplicate audit** — confirm no duplicate is a conflicting public definition.
2. **def-not-in-document impact audit** — classify whether any hits exported symbols or call-graph
   endpoints.

### Stage B closure
**Stage B probes complete. Strategic risks are bounded, not erased. Stage C may begin with scoped
support contracts.**

## Purpose / the one risk this slice retires

> Is Rust SCIP ingestion supportable enough for the LiveGraph path, and **under what boundary**?

This slice is **not** "prove Rust is perfect." It is **"define the Rust support boundary
honestly"** — the indexing unit, the known failure modes, and the degraded-support contract under
which Rust may enter Stage C.

## Known evidence (SCIP-RUST-SPIKE-1)

- rust-analyzer SCIP **whole-workspace** export **panicked** on repo-graph.
- **Per-crate** export succeeded for `storage`: **88 docs, ~52k occurrences**, cross-crate /
  stdlib / external refs resolved.
- Caveats: **duplicate symbols**; **definition-not-in-document** errors; **~31.6s per crate**;
  **~72% local symbols**.
- Existing capture reusable: `/tmp/scip-spike/rust-storage.scip` (+ `.globalsyms.txt`).

## Central reality this probe must characterize (under the hood)

1. **Per-crate, slow — but the refresh class is EVIDENCE-GATED.** rust-analyzer SCIP is per-crate;
   the prior spike saw ~31.6s/crate. **That 31.6s/crate is a prior RISK SIGNAL, not a verdict.**
   REFRESH-PROBE-1's thresholds were measured for **TypeScript under `scip-typescript`** — a
   different producer. RUST-INGEST-PROVE-1 measures Rust with **N≥3 per crate** before assigning a
   refresh class. Reuse the *concept* (B-slow-async / C-explicit / unsupported), NOT TS's numeric
   conclusion.
2. **No value-level AST join for Rust (degraded identity basis, not automatically a failure).**
   INGEST-CORE-1's `(file,range)` AST join is **TS-specific** (`ts-extractor`); repo-graph
   currently has **no value-level AST join path for Rust**, so Rust nodes fall to
   `IdentitySource::ScipSynthesizedFallback`. **Expected fallback-identity rate is high and MUST be
   measured.** This is a **degraded identity basis versus TS, not automatically a failure** — Rust
   identity is SCIP-substrate-derived rather than value-level reconciled.
3. **High local-symbol ratio (~72%).** Most Rust symbols are local → not cross-partition
   addressable. Cross-crate addressability is limited to the ~28% global symbols. Bounds what
   LiveGraph cross-partition Rust queries can answer (echoes the XPART degraded-class discipline).
4. **rust-analyzer emits duplicates + def-not-in-document.** These are upstream behaviors we
   consume and bound — we do NOT fix rust-analyzer.

## Measurements required

1. **repo-graph selected crate re-run, N ≥ 3** (`storage`): `T_scip` (rust-analyzer scip),
   decode, ingest (`ingest_partition`), **duplicate-symbol count**, **definition-not-in-document
   count**, **local/global symbol ratio**, and the **fallback-identity rate** (expected ~100%).
2. **Multiple crates if cheap:** `storage`, `indexer`, and a CLI/daemon crate (`rgr` or `rmapd`).
   Same metrics; classify cost spread.
3. **Cross-crate reference evidence:** resolved refs to **another repo-graph crate**, **stdlib**
   refs, **external crate** refs (do they resolve / what symbol scheme).
4. **Whole-workspace retry: run ONCE only.** If it panics, record as **unsupported**; do not
   chase (no upstream debugging).
5. **Can Rust enter LiveGraph Stage C?** yes as per-crate async degraded support, or no (keep
   TS/C primary) — answered from the numbers + the support contract.

## Ratified decisions (2026-05-31)

> **Rust refresh classification is evidence-gated inside RUST-INGEST-PROVE-1. Prior 31.6s/crate is
> a risk signal, not a verdict.** Reuse the *concept* of the REFRESH-PROBE-1 classes, never the TS
> numeric thresholds as language-independent law.

**D1 — supported indexing unit.** Per crate. Whole-workspace export is a **diagnostic only**. If
whole-workspace panics again, record **unsupported** and stop chasing.

**D2 — duplicate-symbol handling.** Deterministic dedup + aliases/provenance. **Hard-fail only if**
duplicates cannot be canonicalized deterministically **or** produce conflicting definitions for
exported/public symbols.

**D3 — definition-not-in-document errors.** Tolerate **only if** bounded, counted,
provenance-surfaced, and **not** corrupting exported/public symbols or call-graph endpoints.
Otherwise **fail the Rust support boundary**.

**D4 — refresh model.** **Evidence-gated.** Measure **N≥3** per selected crate, then classify:
- **B-slow async** if stable but slow and safe for background refresh,
- **C explicit-refresh** if p95 > 10s or disruptive,
- **unsupported** if the tool panics / errors unpredictably.

Do NOT reuse TS thresholds blindly as language-independent law — reuse the concept, not the number.

**Measured outcome (N=3): B-very-slow-async.** ~29–32s p95 across storage/indexer/rgr, stable, 0
panics → safe for **background** per-crate refresh, never synchronous / never post-edit blocking.
Last-good epoch served meanwhile; `Stale`/`PrecisionPending` until the crate refresh completes;
explicit manual refresh = operator fallback only. **Not C** (stable + background-safe).

**D5 — maturity.** Default expected maturity **PARTIAL / BETA**. Promotion requires measured
stability, bounded duplicate/error behavior, and clear degradation semantics. **No PRODUCTION
claim in this slice.**

## Acceptance — Rust passes ONLY with a written support boundary

**Pass condition (explicit):** Rust may enter Stage C **only** as a language with a **written
support boundary**. The boundary MUST state:
- **indexing unit** (per-crate; whole-workspace is diagnostic-only),
- **refresh class** (evidence-gated per D4: B-slow async / C-explicit / unsupported — measured,
  not pre-decided),
- **duplicate policy** (deterministic dedup + aliases; the hard-fail conditions),
- **error tolerance** (definition-not-in-document bounded/surfaced; the fail conditions),
- **identity basis** (SCIP-synthesized fallback; measured rate; degraded vs TS value-level),
- **degraded query semantics** (high-local-ratio cross-partition limits; degraded answer-classes
  consistent with the XPART contract).

No written boundary → no GO. Known rust-analyzer failure modes (whole-workspace panic, duplicates,
def-not-in-document) are recorded as part of the boundary, not hidden.

## Hard guardrails

```text
no fixing rust-analyzer
no building a Rust compiler frontend
no production runtime wiring
no warm-cache design
no broad refactor of SCIP ingest core
```

Also: whole-workspace gets exactly ONE retry; per-crate only otherwise; reuse existing captures
where valid; the probe MEASURES and BOUNDS, it does not harden ingest for Rust.

## Expected verdict path

**GO WITH CAVEATS:** Rust enters Stage C **only** as a **per-crate, async, SCIP-backed,
degraded-support** language. **Whole-workspace export unsupported** until upstream behavior
changes. **Refresh is not post-edit synchronous.** Rust must **not overclaim parity with TS/C**
(no value-level AST identity; high local ratio; per-crate slow refresh). The alternative honest
outcome is **disabled-pending-upstream** if duplicates/def-not-in-document prove unbounded or hit
the call graph.

## Feasibility (measured today, OBSERVED)

- rust-analyzer at `~/.cargo/bin/rust-analyzer` (binary-direct; toolchain `1.93.0` matches the
  workspace `rust-version`). Invocation: `rust-analyzer scip <crate-dir>` → `index.scip`;
  whole-workspace = run at the workspace root (the panicking case).
- Targets are **repo-graph's own crates** (self-hosting): `storage`, `indexer`, `rgr`, `rmapd`,
  `daemon-runtime`, …
- Prior evidence: `docs/audits/scip-rust-spike-1` (local). Existing capture
  `/tmp/scip-spike/rust-storage.scip` reusable for the ingest measurement.

## The probe (NOT built yet)

`rust/tools/rust-ingest-probe` (research/probe, `publish = false`). Reuses `ingest_partition`
(INGEST-CORE-1) to ingest Rust `.scip`; drives `rust-analyzer scip` per crate (binary-direct,
timed); counts duplicates, definition-not-in-document, local/global ratio, fallback-identity rate;
attempts the whole-workspace export ONCE. Emits the metrics + the proposed support contract. No
production runtime, no ingest-core changes.

## Exit criterion

Passes when it produces, per measured crate: the metric table (T_scip, decode, ingest, dup count,
def-not-in-document count, local/global ratio, fallback rate), cross-crate reference evidence, the
whole-workspace one-shot result, AND a **written Rust support contract** with a maturity
classification (D5) justified by the numbers — not asserted. Documented retreat: if per-crate is
too unstable to bound, classify Rust **disabled-pending-upstream** and keep TS/C primary.

## References
- `docs/architecture/scip-migration-plan.md` (Stage B — RUST-INGEST-PROVE-1)
- `docs/audits/scip-rust-spike-1/` (prior per-crate Rust evidence, local)
- `docs/slices/refresh-probe-1.md` (B model + D2 thresholds Rust is measured against)
- `docs/slices/ingest-core-1.md` (`ingest_partition`; the TS-specific AST join Rust lacks)
- `docs/slices/xpart-prove-1b.md` + `xpart-st3-boundary-decision.md` (degraded answer-classes)
