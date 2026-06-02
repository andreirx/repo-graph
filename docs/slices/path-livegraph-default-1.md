# PATH-LIVEGRAPH-DEFAULT-1: `rmap path` Default → auto (LiveGraph with SQLite fallback) (Stage D)

Slice ID: PATH-LIVEGRAPH-DEFAULT-1
Status: **DESIGN — D1–D5 ratified (2026-06-02). Implementation NOT started.**
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

## After this
```text
PATH-CYCLES-LIVEGRAPH-2 (cycles).
```

## References
- `docs/slices/path-cycles-livegraph-1.md` (LiveGraph `path()`; the D3 completeness class; path_engine_response)
- `docs/slices/query-migration-cli-1.md` (the `auto` default + backend_used/fallback_reason for callers/callees)
- `docs/slices/sqlite-raw-decommission-readiness-1.md` (path as a default nodes/edges dependency)
