# PATH-LIVEGRAPH-DEFAULT-1: `rmap path` Default → auto (LiveGraph with SQLite fallback) (Stage D)

> **SUPERSEDED (2026-07-16, ENGINE-CONSOLIDATION-1 §8 / D-EC-4 ratified):** this
> per-surface migration plan is retired. The consolidation end-state + milestone sequence
> in `docs/slices/engine-consolidation-1.md` (as amended by `recon-design-1.md` §8)
> replaces it. Kept for the historical record.


Slice ID: PATH-LIVEGRAPH-DEFAULT-1
Status: **IMPLEMENTED + live-validated (2026-06-02).** `path` default flipped to `auto`; serves LiveGraph
when Exact+Fresh+TS-only AND every rendered node has display metadata, else labelled SQLite fallback. A
ratified mid-build addition (display-metadata gate + `node_location` support API) closed a human-format
regression. See Completion.
Depends: PATH-CYCLES-LIVEGRAPH-1 (LiveGraph `path()` + the D3 completeness class), QUERY-MIGRATION-CLI-1
(the `auto` default + `backend_used`/`fallback_reason` pattern for callers/callees).
Track: Stage D. Flips the `path` DEFAULT (shipped behavior). Does NOT touch cycles; does NOT decommission
`nodes`/`edges`.

## Framing

```text
PATH-CYCLES-LIVEGRAPH-1 proved `path` behind --engine livegraph, but SQLite stayed the DEFAULT, so
nodes/edges remain a default dependency for path. This slice finishes the callers/callees migration
pattern for path: auto default, LiveGraph when complete, else labelled SQLite fallback.
```

## Purpose
```text
Make `rmap path` default to `auto`: use LiveGraph when Exact/Fresh/complete, otherwise fall back to
SQLite with labelled metadata.
```

## Ratified decisions (D1–D5, 2026-06-02)

### D1 — default = `auto`
Same policy as callers/callees (QUERY-MIGRATION-CLI-1). `rmap path` (no flag) → `auto`.

### D2 — serve threshold
Serve LiveGraph ONLY if `AnswerClass::Exact` AND `FreshnessState::Fresh` (+ TS-only, mirroring
callers/callees D4). `Partial` / `Stale` / `RefreshFailed` / `Unavailable` → SQLite fallback.

### D3 — no-path handling (the highest-risk, path-specific rule)
```text
Exact no-path may be served from LiveGraph ONLY when traversal completeness is proven.
Partial no-path MUST fall back to SQLite.
```
**Already encoded by `path()`'s class** (PATH-CYCLES-LIVEGRAPH-1 D3): `path()` returns `Exact` for a
proven-complete result (found OR no-path) and `Partial` for an incomplete traversal (a non-resident /
stale frontier). So the D2 rule "serve iff Exact+Fresh" handles D3 correctly with NO extra path-specific
branch: an Exact no-path (proven complete) is served from LiveGraph; a Partial no-path falls back to
SQLite. This is the load-bearing reuse — the trust class IS the completeness signal.

### D4 — metadata
Same as callers/callees: JSON `backend_used` + `fallback_reason`; human FORMAT unchanged (content may
differ only when the backend differs, explainable by metadata). No new trust metadata in human output.

### D5 — compare
Keep `--engine compare` + the path compare sidecar (PATH-CYCLES-LIVEGRAPH-1). `--engine sqlite` /
`--engine livegraph` still force their engine.

## Acceptance (EXECUTED)
```text
1. default `rmap path report makeCircle` uses LiveGraph on the synthetic fixture; human format preserved
2. `--json` shows backend_used=livegraph (when served from LiveGraph)
3. a nonresident/stale -> Partial path falls back to SQLite with a fallback_reason
4. `--engine sqlite` forces SQLite
5. `--engine livegraph` still works
6. `--engine compare` still writes the sidecar
7. no cycles migration
```

## Out of scope (hard guardrails)
```text
No cycles (PATH-CYCLES-LIVEGRAPH-2). No nodes/edges decommission. No change to path() traversal /
completeness semantics. No multi-language (TS partitions only).
```

## Implementation notes (grounding; confirm during build)
```text
- livegraph_feed: extend `livegraph_path` to also report TS-only (the path answer's contributing
  languages), then add an `Engine::Auto` branch to `path_engine_response`: serve LiveGraph iff Exact +
  Fresh + TS-only (reuse the auto_outcome shape — freshness-before-class so Stale reports LiveGraphStale);
  else SQLite with backend_used="sqlite" + the fallback_reason. (Today path's Auto maps to Sqlite; this
  slice makes Auto actually decide.)
- rgr `run_path`: remove the `auto -> sqlite` override so `path` defaults to `auto`; keep --engine
  sqlite|livegraph|compare explicit. Strip metadata in the human render (already done in
  PATH-CYCLES-LIVEGRAPH-1).
- A nonresident/stale Partial-fallback live test needs a multi-partition or unloaded/stale partition on
  the path; the single-partition synthetic is always Exact -> use unit coverage for the Partial->SQLite
  fallback if a live Partial path cannot be staged on the fixture (state the limitation).
```

## Commit structure (proposed)
```text
1. daemon Auto path decision + CLI default flip (combined, like QUERY-MIGRATION-CLI-1, so the flip never
   leaves a non-building/inconsistent step) + live validation.
```

## Ratified mid-build addition (2026-06-02) — display-metadata gate (DIVERGENCE from the spec notes)

The spec assumed PATH-CYCLES-LIVEGRAPH-1's `livegraph_path_result` preserved the human format. It did
NOT for the **human/default** case: it emitted `symbol = full stable key` and `file:""`/`line:0`
(acceptable when LiveGraph was opt-in + JSON-focused behind `--engine livegraph`, but a visible
regression once `auto` is the human default — `Path: synthetic:...#report:FUNCTION:...` / `report  :0`).
Surfaced as a STOP-and-ask. Ratified **C** with an added invariant:

```text
1. node_id stays the full stable key; symbol = key_name(key).
2. file/line populated from the resident IrNode.range via a NEW read-only support API
   LiveGraph::node_location(&CanonicalKey) -> Option<SourceRange>. PathAnswer is NOT grown.
3. Engine::Auto serves LiveGraph ONLY if Exact + Fresh + TS-only AND every rendered path node has
   display metadata {file,line}; else SQLite fallback with FallbackReason::LiveGraphDisplayMetadataUnavailable.
   Never render `:0` on the default path. A no-path serves with no nodes to render (gate vacuous).
4. --engine livegraph MAY still serve degraded/missing metadata (explicit); default auto MUST NOT.
5. Display metadata is presentation only — it does NOT become trust/completeness semantics; it gates
   DEFAULT serving because default human-output compatibility is part of the shipped-surface contract.
```

Line-base (spec rule 4): `SourceRange.start_line` is 1-based (IR convention, from `ast.line_start`).
**Verified live**: LiveGraph lines (`9,5`) equal SQLite lines (`9,5`) for `report`/`makeCircle` on the
synthetic — no conversion needed. (Had they differed, conversion would be applied at the presentation
boundary in `livegraph_path_result`.)

Bonus: `livegraph_path_result` now fills real `file:line` for `--engine livegraph` too (was `:0`).

## Completion (implemented + live-validated 2026-06-02, EXECUTED)

Support: `LiveGraph::node_location` (read-only IR lookup) + 4 unit tests (repo-graph-livegraph 35→39).
Daemon: `FallbackReason::LiveGraphDisplayMetadataUnavailable`; `PathNodeDisplay{key, location}`;
`livegraph_path` resolves per-node locations under the read guard; `path_auto_outcome` gates on
Exact+Fresh+TS-only+all-metadata-present; `Engine::Auto` arm decides (was → SQLite); `livegraph_path_result`
emits `symbol=key_name`, real `file:line` (+6 path_auto unit tests: serves-exact-fresh-ts, partial, stale,
unsupported-language, **missing-display-metadata**, unavailable). CLI: `run_path` default `auto` (removed
the `auto→sqlite` override); usage advertises `auto|sqlite|livegraph|compare`; human render strips JSON-only
metadata (unchanged).

```text
Gating (EXECUTED): repo-graph-livegraph 39, repo-graph-daemon-runtime 78, repo-graph-rgr 444 (+suites);
  cargo clippy --workspace --all-targets -D warnings clean; cargo fmt --all --check clean.
Live (synthetic fixture, daemon v0.2.1; after livegraph-preload synthetic = 15 nodes/11 edges):
  rmap path report makeCircle               -> backend_used=livegraph; human BYTE-IDENTICAL to SQLite;
                                               report src/main.ts:9 -CALLS-> makeCircle src/main.ts:5   [#1]
  rmap path report makeCircle --json        -> backend_used=livegraph, fallback_reason=null, lines 9,5  [#2]
  rmap path report makeCircle --engine sqlite --json    -> backend_used=sqlite                          [#4]
  rmap path report makeCircle --engine livegraph --json -> backend_used=livegraph, Exact/Fresh, lines 9,5 [#5]
  rmap path report makeCircle --engine compare          -> sidecar written, buckets=[]                  [#6]
  rmap path makeCircle report               -> "No path found." human; --json backend_used=livegraph,
                                               found=false (D3: Exact no-path SERVED from LiveGraph)
  Partial/Stale/missing-metadata fallback   -> unit-tested (live single-partition is always Exact)      [#3]
```

All 7 acceptance criteria PASS; no cycles touched; no nodes/edges decommission.

## After this
```text
PATH-CYCLES-LIVEGRAPH-2 (cycles).
```

## References
- `docs/slices/path-cycles-livegraph-1.md` (LiveGraph `path()`; the D3 completeness class; path_engine_response)
- `docs/slices/query-migration-cli-1.md` (the `auto` default + backend_used/fallback_reason for callers/callees)
- `docs/slices/sqlite-raw-decommission-readiness-1.md` (path as a default nodes/edges dependency)
