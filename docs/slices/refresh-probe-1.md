# REFRESH-PROBE-1: SCIP Refresh Cost & Workflow Shape (Stage B)

Slice ID: REFRESH-PROBE-1
Status: **EXECUTED (2026-05-31) — VERDICT B (two-speed refresh).** Increment 1 (single-partition
chain) + increment 2 (burst, provider→consumer fanout, amodx scale) complete; A/B/C decided and
the workload envelope characterized. Verdict + claim constraints below; full evidence
`docs/audits/refresh-probe-1/findings.md` (local). Next: RUST-INGEST-PROVE-1.
Depends: INGEST-CORE-1 (`ingest_partition`), XPART-PROVE-1A/1B (xref + alias reconciliation, the
`Stale` answer class), XPART-ST3-BOUNDARY-DECISION (degraded classes — partition identity/query
semantics are now bounded).
Track: Extraction Substrate Pivot — Stage B (`docs/architecture/scip-migration-plan.md`).

## Verdict (EXECUTED 2026-05-31)

**Verdict: B — two-speed refresh.**

Direct synchronous SCIP refresh is rejected for the default post-edit path. Whole-partition SCIP
indexing dominates and exceeds the A budget on every measured partition:
- FRAKTAG engine: ~1.9s chain
- amodx plugins: ~3.0s chain

C is not indicated: refresh remains seconds, tooling is stable, and no build destabilization was
observed.

Runtime contract:
- serve last-good SCIP epoch
- apply AST fast delta immediately where available
- mark SCIP-backed answers `Stale` / `PrecisionPending` while refresh runs
- coalesce bursts; never start a redundant partition refresh while one is already in flight
- atomic swap to refreshed SCIP epoch on success
- keep last-good epoch on failure
- never return `Exact`-empty for stale/missing refresh state

### Claim constraints (do NOT overclaim from the measurements)

- **Burst:** the 8.4× waste proves coalescing is **mandatory**. It does NOT set the final debounce
  window — the runtime must tune debounce/coalescing policy later.
- **Fanout:** provider→consumer invalidation is proven **symbol-reference-conditioned** on the
  measured FRAKTAG case. Production invalidation must use the **affected exported-symbol refs** and
  **degrade conservatively** when the affected-symbol set is uncertain.
- **xref/alias recompute negligible (~21ms):** holds for the measured **TypeScript
  package-boundary** case only. Do NOT generalize to all languages or huge workspaces yet.

### Tool-reuse boundary (tech debt)

`refresh-probe` reuses `xpart-probe`'s `export_alias` as a library. This is acceptable ONLY as
**research-tool reuse inside `rust/tools`**. **Production crates must NOT depend on `xpart-probe`.**
If LiveGraph reuses export-surface reconciliation, that logic must first move into a proper
**support crate**. Recorded as tech debt.

## Framing (what this slice is NOT)

```text
REFRESH-PROBE-1 is not warm-cache design.
REFRESH-PROBE-1 is not LiveGraph implementation.
REFRESH-PROBE-1 measures update latency, blocking behavior, and workflow shape for
SCIP-backed partitions.
```

## The one risk this slice retires

> Can compiler-grade SCIP indexing support the post-edit / daemon workflow directly, or must
> repo-graph use a two-speed model?

The deliverable is a **measured decision** (A / B / C below) plus the workload shape the runtime
must support — not a refresh implementation.

## Central reality this probe quantifies (under the hood)

`scip-typescript` is a **whole-program typecheck**: it runs the TS compiler over a package's
`tsconfig` and emits one `.scip` for the **whole partition**. Consequences the probe measures
against, not around:

- **The refresh unit is the partition, not the file.** A no-op reindex, a one-line body edit, and
  a signature edit all pay ~the same whole-package typecheck cost. (Measurements #1 and #2 will be
  close — that closeness is itself a finding.)
- **SCIP has no per-file delta; the AST extractor does.** repo-graph already ships per-file AST
  delta (`ts-extractor` + delta-refresh slice 1). So the A-vs-B axis is precisely: is
  whole-partition SCIP reindex cheap enough to sit in the post-edit path (A), or must the fast
  AST delta carry the post-edit path while SCIP refreshes asynchronously (B)?
- **Cold cost is the honest pessimistic case.** The probe measures a cold per-partition reindex
  (fresh `tsc`, no warm language service). A warm incremental TS service would be a *warm-cache*
  optimization — explicitly out of scope here. If even the cold whole-partition cost is
  acceptable, A is safe; if not, B/C, and a future warm-service slice may revisit.

## Measurement protocol

Per scenario the probe times the full chain and reports each stage separately:

```text
T_scip    scip-typescript index run (the compiler-grade cost; dominant term)
T_decode  decode .scip
T_ingest  ingest_partition (INGEST-CORE-1)
T_xref    global xref rebuild (XPART-PROVE-1)
T_alias   export-surface alias reconciliation (XPART-PROVE-1B; cross-partition scenarios)
T_answer  answer-class recompute for a fixed callers() target
```

Invoke the `scip-typescript` **binary directly** (not `npx`) so Node/npm resolution overhead does
not inflate `T_scip`; separate process-spawn from index-compute if the tool surfaces it.

**Mutation model (edits applied to the real partition).** FRAKTAG is a git work tree with clean
tracked sources. For an edit scenario: verify the target file is tracked-and-clean (guard; abort
if dirty so no user work is clobbered), apply the edit, measure, then `git checkout -- <file>` to
revert (file-scoped — the repo's untracked clutter is untouched). See D1.

### Minimum measurements

1. **Single-partition no-op reindex** — same files, no edits → fixed overhead (lower bound).
2. **Single-file edit inside one partition** — three edit classes, measured separately:
   trivial body edit · public signature edit · export-surface edit.
3. **Cross-partition impact** — edit engine **public API**, then measure the **consumer
   invalidation requirement**: reindex engine, reindex api, diff api's cross-partition references
   (engine→api) before/after to determine whether api *must* reindex. Body-private edits should
   show no consumer fanout; public/export edits should.
4. **Burst workflow** — 5–10 edits in sequence → does refresh need to **debounce/coalesce**?
   (N whole-partition reindexes vs 1 coalesced reindex after N edits.)
5. **Blocking model** — when a query arrives during refresh: serve stale? block? return `Stale`?
   The probe measures the refresh **window**; the contract choice is informed by it (D4).
6. **Scale comparison** — FRAKTAG small partition (mandatory); **amodx** larger TS workspace
   (present at `/Users/apple/Documents/APLICATII BIJUTERIE/amodx` — include, confirm TS-indexable);
   repo-graph Rust per-crate **only if trivial to measure** — do not pull Rust into the critical
   path otherwise.

## Outputs

```text
per-partition reindex time        (T_scip, by partition + scale)
xref rebuild time                 (T_xref)
alias reconciliation time         (T_alias)
answer-class recompute time       (T_answer)
file-change fanout                (which partitions must reindex for a body edit)
public API edit fanout            (which partitions must reindex for a public/export edit)
burst edit shape                  (per-edit cost × N vs coalesced; debounce recommendation)
recommended refresh model         (A / B / C, with the measured basis)
```

## Candidate outcomes

```text
A. Direct SCIP refresh
   - per-partition reindex acceptable
   - daemon marks partition refreshing, then swaps epoch

B. Two-speed refresh
   - AST fast delta immediately (existing ts-extractor delta path)
   - SCIP refresh async
   - queries during the gap return Stale / PrecisionPending

C. Explicit refresh only
   - SCIP too expensive for post-edit
   - user/agent triggers refresh
   - daemon reports stale facts until then
```

B reuses an **existing** fast path (AST delta already ships), so it is not new-capability risk —
it is a routing decision. Expected outcome **B**, but the probe is written so **A wins if the data
supports it** (whole-partition reindex cheap + bounded public-API fanout).

## Decision criteria (what the numbers must show)

- **A viable** iff cold whole-partition `T_scip` (+ chain) is small enough to run synchronously in
  the post-edit hook without the agent noticing **and** public-API fanout is bounded (few
  consumers reindex). Threshold band is a product judgment — see D2.
- **B required** iff `T_scip` is seconds-scale (typical for `tsc` on non-trivial packages) so
  blocking the post-edit path is unacceptable, but async SCIP behind AST delta is fine.
- **C** iff `T_scip` is large or public-API fanout cascades badly enough that even async
  per-edit refresh is wasteful → explicit trigger only.

## Ratified decisions (2026-05-31)

**Decision rule — this slice decides the runtime refresh CONTRACT, not just timings.** If B wins,
the runtime MUST expose a **two-speed state**:
- fast local AST state may be newer,
- SCIP graph state may lag,
- **query answers must carry freshness/precision state.**

**D1 — mutation model: in-place edit + file-scoped `git restore`.** Measures the REAL installed
workspace (`node_modules`, symlinks, `tsconfig`, package references, generated `dist`,
package-manager behavior); a sandbox copy would measure a synthetic environment unless the whole
workspace state were cloned. Safety protocol per edit:
```text
precondition: no TRACKED changes in the target repo AND target file clean
edit exactly one known file
restore exactly that file  (git checkout -- <file>)
postcondition: no TRACKED changes AND target file clean
abort if dirty before or after
```
*Documented divergence:* FRAKTAG carries pre-existing **untracked** files (`.idea/`, experimental
dirs). Literal "git status clean" would falsely abort. The guard is therefore scoped to **tracked**
state (`git status --porcelain --untracked-files=no` empty) plus the specific target file; the
probe never creates or touches untracked files and asserts the untracked set is unchanged across
edit/restore. Safety intent preserved (never clobber user work; clean revert) without aborting on
pre-existing clutter.

**D2 — latency threshold (set BEFORE measurement; report p50/p95/min/max, NEVER mean alone):**
```text
A Direct SCIP refresh:    p50 <= 1.0s AND p95 <= 1.5s for the changed-partition refresh chain
B Two-speed refresh:      p50 > 1.0s OR p95 > 1.5s, but async refresh remains tolerable
C Explicit refresh only:  p95 > 10s on normal product partitions, OR refresh routinely
                          destabilizes build/tooling
```
Classify by partition size: a small partition may be direct-acceptable while a large partition is
likely two-speed. Each scenario runs **N ≥ 10 repetitions** for stable percentiles.

**D3 — scale scope:** (1) FRAKTAG mandatory; (2) **amodx mandatory IF it indexes without dependency
repair** — attempt it; if it needs `npm install` / fixes, document and skip (do NOT repair);
(3) Rust **excluded** (zero-friction only; Rust has its own probe, RUST-INGEST-PROVE-1). Do not let
Rust re-enter through this slice.

**D4 — blocking-model contract (ratified):**
```text
serve last-good epoch + Stale marker while refresh runs
atomic swap to the new epoch after a successful refresh
a failed refresh keeps the last-good epoch and reports RefreshFailed / Stale
never return exact-empty for stale/missing refresh state
```
Hard rejections: no blocking by default; no query against a half-refreshed graph; no mutation of
authoritative state until a full partition refresh succeeds. The probe **demonstrates** this — hold
the last-good epoch during a timed refresh, a query returns `Stale` (non-empty last-good), atomic
epoch swap on success; a simulated failure keeps last-good + `RefreshFailed`.

## Feasibility (measured today, OBSERVED)

- **amodx present** at `/Users/apple/Documents/APLICATII BIJUTERIE/amodx` → larger-scale TS
  comparison available (confirm it is `tsconfig`-indexable at build).
- **FRAKTAG is a git work tree**, tracked sources clean → in-place edit + file-scoped revert is
  viable (D1a). Untracked clutter (`.idea/`, experimental dirs) does not interfere.
- `scip-typescript` toolchain already used by SCIP-TS-PARITY-SPIKE-1 / XPART probes.

## The probe (NOT built yet)

`rust/tools/refresh-probe` (research/probe, `publish = false`). Reuses `ingest_partition`
(INGEST-CORE-1) and the XPART-PROVE-1 xref/alias logic (shared thin path or duplicated for
timing). It drives `scip-typescript`, applies/reverts edits per D1, and emits the Outputs table +
the recommended model. No production runtime, no shared-crate extraction required for the probe.

## Out of scope (hard rule)

```text
No SQLite pruning/storage work enters this slice.
No warm-cache format decision enters this slice.
No runtime implementation enters this slice.
```

Also: NO LiveGraph residency/eviction; NO query-surface migration; NO warm/incremental TS language
service (that is a warm-cache optimization); NO Rust in the critical path unless trivial.

## Exit criterion

The probe passes when it produces the Outputs table across FRAKTAG (+ amodx) with separated stage
timings and fanout, and a **recommended refresh model (A/B/C) justified by the measured numbers**
against the D2 threshold — not asserted. A documented retreat: if measurement is too noisy to
separate stages, report `T_scip` end-to-end + fanout alone and still pick A/B/C.

## References
- `docs/architecture/scip-migration-plan.md` (Stage B — REFRESH-PROBE-1)
- `docs/slices/xpart-prove-1b.md` (xref/alias, `Stale` class, answer-class precision rule)
- `docs/slices/xpart-st3-boundary-decision.md` (`null`=unknown, never empty)
- `agent_docs/architecture.md` (degradation primitive)
