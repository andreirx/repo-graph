# LIVEGRAPH-INTEGRATION-1B: Flag-Gated Shipped Query Serving from Preloaded LiveGraph (Stage D)

Slice ID: LIVEGRAPH-INTEGRATION-1B
Status: **DESIGN RATIFIED (phase-1; D2a=(a) pilot pre-load; S1–S3 ratified). HIGH blast radius (shipped
`rmapd` + `rmap`). Grounding the RUST transport/CLI surface before code — preload + flag must NOT
depend on possibly-stale TypeScript tooling.**
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

## Shipped-surface sub-decisions (S1–S3) — RATIFIED

**S1 — pilot pre-load: a dev-only DAEMON TRANSPORT METHOD, NOT a public CLI command, NOT `rgr`.** The
TypeScript-side tooling may be stale vs the SQLite schema, so the preload must NOT depend on it.
- Add a dev-only daemon method `livegraph_preload` (params `{repo, partition_id, scip, source_root}`),
  invoked by a Rust integration/dev harness over daemon transport. (Acceptable alternative: a hidden
  Rust-side `rmap dev livegraph-preload` ONLY if Rust `rmap` owns the transport path cleanly — never
  the TS layer.)
- The daemon may DECODE/INGEST a supplied existing `index.scip` and feed the resulting `PartitionIr` +
  `ValueFact`s into LiveGraph. The daemon must NOT run scip-typescript, NOT do package discovery, NOT
  do refresh orchestration. STOP CONDITION: if preload needs daemon-side SCIP generation → move to 1C.
- **Pilot repo: the committed `synthetic/` fixture first** (SQLite/tree-sitter indexes it as the
  oracle; the LiveGraph path preloads its committed real `index.scip`; CI/dev-repeatable; no FRAKTAG
  dependency). FRAKTAG only as optional manual evidence.

**S2 — serving flag: explicit engine selector `--engine sqlite|livegraph|compare`, default `sqlite`.**
- `sqlite` — current behavior, byte-compatible default. `livegraph` — use LiveGraph if present, else
  fallback (D4). `compare` — run both, return the normal SQLite output, emit the classified report.
- `livegraph` is NEVER the default in 1B. (If the current CLI style resists `--engine`, use an
  equivalent hidden/dev flag but keep the three modes — Rust-side only.)

**S3 — comparison: `--engine compare` is the primary surface.** stdout stays the SQLite-compatible
answer; the classified report goes to a structured sidecar `.rgr/livegraph-compare/<timestamp>.json`
(NOT logs-only). Buckets: the 6 classes.

**Fallback guardrail:** `--engine livegraph` miss (no preloaded partition) → fall back to `sqlite`,
record `PartitionUnavailable`, do NOT fail the user-facing query. Only a future explicit strict mode
may fail.

## Build order (ratified)

```text
1. Transport enum / request DTO: engine mode (sqlite|livegraph|compare) + the livegraph_preload method.
2. Daemon state: RepoState += optional LiveGraph.
3. Hidden preload: daemon method livegraph_preload (decode supplied .scip → feed_partition).
4. callers LiveGraph path behind the engine selector (+ fallback).
5. callees LiveGraph path behind the engine selector (+ fallback).
6. compare-mode classified report (sidecar).
7. Validation against the committed synthetic fixture (live rmap sqlite vs livegraph/compare).
```

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
