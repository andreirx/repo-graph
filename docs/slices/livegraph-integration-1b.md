# LIVEGRAPH-INTEGRATION-1B: Flag-Gated Shipped Query Serving from Preloaded LiveGraph (Stage D)

Slice ID: LIVEGRAPH-INTEGRATION-1B
Status: **DESIGN RATIFIED (phase-1; D2a=(a) pilot pre-load). HIGH blast radius (shipped `rmapd` +
`rmap`). TASK PACKET REQUIRED before code; CODE BLOCKED on the shipped-surface sub-decisions (S1–S3).**
Depends: LIVEGRAPH-INTEGRATION-1A (`repo-graph-livegraph-feed::feed_partition`), VALUE-JOIN-1 /
QUERY-MIGRATION-1.
Track: Stage D integration, sub-slice **1B** (after 1A; before 1C).

## Scope wording (ratified)

```text
1B does NOT prove daemon SCIP indexing.
1B proves the shipped query surface can route through LiveGraph WHEN LiveGraph state is supplied
(pre-loaded), behind a flag, with SQLite fallback — and compares it against the current backend.
```

NOT in scope: warm cache, SQLite raw decommission, `path`/`cycles`, C++/Rust value facts, new trust
vocabulary, large refactors, **in-daemon SCIP indexing (→ LIVEGRAPH-INTEGRATION-1C)**.

## Grounding (verified — current shipped path)

- **Serve:** `daemon-runtime/src/dispatch.rs:693 handle_callers` / `:779 handle_callees` →
  `storage::resolve_symbol` → `storage/src/queries.rs:489 find_direct_callers` / `:553
  find_direct_callees` (SQL over `edges`/`nodes`). Output DTO `ResolvedSymbol` + `Vec<CallerResult>`
  (`storage/src/queries.rs:51/64`), JSON `{target, callers|callees, count}`.
- **Index:** `dispatch.rs:1254 handle_index` → tree-sitter pipeline → SQLite snapshots. **Refresh:**
  `dispatch.rs:1465 handle_refresh`. **State:** `state.rs:186 RepoState { storage, coordinator }`.
- **NO partition concept, NO SCIP in the daemon** — whole-repo, one SQLite DB + single snapshot/repo.

## THE DISCOVERY (why 1B ≠ "feed_partition after handle_index")

```text
Daemon indexes via TREE-SITTER → SQLite. LiveGraph needs scip-ingest PartitionIr (SCIP). Different
producers. Serving rmap from LiveGraph requires SCIP data to EXIST in the daemon first, and the daemon
has no SCIP-producing path. Building one is ingestion orchestration → its own slice (1C).
```

## Ratified decisions

**D2a — SCIP `PartitionIr` into the daemon: (a) pilot pre-load.** A pre-load step feeds a known TS
repo's SCIP partitions (via the 1A `feed_partition` path) into the daemon's LiveGraph; the daemon does
NOT run scip-typescript. **Full in-daemon SCIP indexing/refresh = LIVEGRAPH-INTEGRATION-1C (deferred).**

**D1 — transition mode: side-by-side compare FIRST, then primary-with-fallback.** No one-step cutover.

**D3 — CLI output compatibility:** preserve current `rmap` output by default; trust metadata
(`AnswerClass` / `FreshnessState` / `contributing_languages`) ONLY behind an explicit flag / structured
mode. No default output breakage.

**D4 — fallback:** LiveGraph miss → fall back to the old SQLite path during transition + record the
discrepancy internally. No hard user-facing failure during transition unless an explicit
LiveGraph-only mode is requested.

**D5 — validation:** the current `rmap` SQLite backend (running daemon built from `main`) is the
**oracle**. Compare LiveGraph vs it; classify mismatches (never generic failure). Mismatch classes:

```text
MissingInLiveGraph | ExtraInLiveGraph | IdentityMismatch | EdgeBasisMismatch
PartitionUnavailable | TrustClassMismatch
```

## Naming (ratified)

```text
LIVEGRAPH-INTEGRATION-1B: flag-gated shipped query serving from preloaded LiveGraph
LIVEGRAPH-INTEGRATION-1C: daemon SCIP indexing and refresh orchestration
```

## Shipped-surface sub-decisions (S1–S3) — confirm in the TASK PACKET before code

These touch shipped daemon/CLI behavior; the task packet surfaces them for sign-off (the build does
NOT start until they are confirmed):

- **S1 — pilot pre-load trigger:** (a) hidden `rgr`/`rmap` dev command that ingests + `feed_partition`s
  a repo into the running daemon's LiveGraph · (b) daemon-start config/env pointing at a pre-ingested
  partition set · (c) test-only harness. *Lean (a)* (a real daemon path is needed so `rmap` can serve
  from it; (c) cannot satisfy "rmap uses the LiveGraph path").
- **S2 — serving flag surface:** the engine selector threaded CLI → daemon-transport request →
  dispatch (e.g. `rmap callers --engine livegraph` / `--livegraph`). *Confirm name + that it is opt-in,
  default unchanged.*
- **S3 — comparison surface:** where the classified mismatch report lives (a `rmap`/`rgr` diagnostic
  subcommand vs a dev harness vs structured logs). *Confirm.*

## Acceptance (ratified — phase-1)

```text
- daemon can hold a LiveGraph instance (RepoState += Option<Arc<LiveGraph>>)
- pilot pre-load feeds SCIP PartitionIr + ValueFacts into the daemon LiveGraph
- rmap callers can execute the LiveGraph path behind a flag for ≥1 preloaded TS repo
- rmap callees can execute the LiveGraph path behind a flag for ≥1 preloaded TS repo
- default rmap output remains unchanged
- fallback to the SQLite path works on a LiveGraph miss
- side-by-side comparison report classifies mismatches (the 6 classes above)
- no daemon SCIP indexing pipeline | no warm cache | no SQLite decommission
```

## Out of scope (hard guardrails)

```text
No warm cache. No SQLite raw decommission. No path/cycles. No C++/Rust value facts. No new trust
vocabulary. No large refactors. No in-daemon SCIP indexing (D2a(b) → 1C). No default-output change.
```

## Definition of done

- `RepoState` can hold an optional `LiveGraph`; a pilot pre-load populates it for ≥1 real TS repo;
  `rmap callers`/`callees` serve from LiveGraph behind an opt-in flag with SQLite fallback; default
  output unchanged; the live-`rmap` comparison emits a class-bucketed mismatch report.
- Task packet stated + S1–S3 confirmed; clippy/fmt clean; workspace builds; no default behavior change.

## Exit criterion

The shipped query surface can serve `callers`/`callees` from preloaded LiveGraph state behind a flag,
falling back to SQLite, with a classified comparison against the current backend — proving the routing
without an in-daemon SCIP pipeline. 1C adds in-daemon SCIP indexing + refresh; primary-with-fallback
default follows once parity is measured.

## References
- `docs/slices/livegraph-integration-1a.md` (`feed_partition` seam)
- `docs/slices/query-migration-1.md` (headless callers/callees + trust metadata)
- `daemon-runtime/src/dispatch.rs` (handlers), `state.rs` (`RepoState`), `storage/src/queries.rs` (DTOs)
