# PATH-CYCLES-LIVEGRAPH-1: LiveGraph-backed `path` (cycles deferred) (Stage D)

Slice ID: PATH-CYCLES-LIVEGRAPH-1
Status: **DESIGN — D1–D5 ratified (2026-06-01). Implementation NOT started.**
Depends: QUERY-MIGRATION-CLI-1 (the `--engine` auto/compare pattern; `callers_engine_response`),
SQLITE-RAW-DECOMMISSION-READINESS-1 (path/cycles keep `nodes`/`edges` alive), the trust model
(`AnswerEnvelope`; never exact-empty).
Track: Stage D. Does NOT decommission SQLite; does NOT flip the `path` default.

## Framing

```text
callers/callees default migration is done (QUERY-MIGRATION-CLI-1).
path/cycles still keep the SQLite nodes/edges tables alive (readiness audit §2).
To retire the raw graph (nodes/edges), path/cycles must EITHER migrate to LiveGraph OR be explicitly
scoped out. This slice migrates `path` (behind a flag) and DEFERS `cycles` with an explicit reason.
```

## Purpose
```text
Add LiveGraph-backed `path` query support (headless + behind --engine), with honest completeness
semantics; explicitly define why `cycles` remains SQLite-backed for now.
```

## Ratified decisions (D1–D5, 2026-06-01)

### D1 — surfaces: **path only** (cycles deferred)
`path` is targeted traversal; `cycles` requires whole-graph SCC + stronger completeness semantics
(D5). Migrate `path` first; defer `cycles`.

### D2 — algorithm: **BFS over RESIDENT LiveGraph edges**
Shortest path `from → to` via BFS over the edges of RESIDENT partitions' `PartitionIr`s (the only
place outgoing adjacency exists — the xref summary keeps `defines` + incoming `ref_counts`, NOT full
outgoing adjacency, so it is NOT sufficient for traversal). Do NOT synthesize edges from the xref.

### D3 — completeness rule (the trust-critical part; avoid false "no path")
```text
PATH FOUND  -> the path is real. Class Exact iff every partition ON the path is resident + Fresh;
               else Stale/Partial (path served, freshness degraded).
NO PATH     -> may claim Exact "no path" ONLY if the ENTIRE reachable region from `from` was resident
               + Fresh (the BFS never hit a frontier node it could not expand). If the BFS reached any
               node whose defining partition is NON-resident (outgoing edges not loaded) or stale ->
               Partial / Unavailable, NEVER Exact-empty. A non-resident partition may hold the missing
               edge; we must not assert "no path" we cannot prove.
```
This is the hard invariant: a partial graph must not produce a confident "no path". (Single-partition
fixtures are fully resident, so they are Exact; multi-partition correctness rides on this rule.)

### D4 — default CLI behavior: **SQLite stays the `path` default this slice**
```text
rmap path (no flag)        -> SQLite (unchanged; the proven path)
rmap path --engine livegraph -> the new LiveGraph BFS (explicit, trust-labelled)
rmap path --engine compare   -> SQLite answer + a LiveGraph compare report (sidecar)
```
Do NOT flip the `path` default to `auto` in this slice — the completeness semantics (D3) must be proven
across multi-partition repos first (a future PATH-DEFAULT-MIGRATION slice, mirroring QUERY-MIGRATION-CLI-1).

### D5 — cycle semantics: **deferred**
`cycles` needs whole-graph SCC; over a partial resident graph it would mislabel. If/when added: SCC over
the RESIDENT graph only, labelled `Partial` if ANY contributing partition is non-resident/stale. Out of
scope here; recorded as PATH-CYCLES-LIVEGRAPH-2 (cycles).

## Acceptance (path-first, EXECUTED)
```text
1. a headless LiveGraph `path(from, to)` API exists, returning a trust-labelled AnswerEnvelope
2. rmap path <from> <to> --engine livegraph works on the synthetic fixture (finds the known path)
3. a no-path result is Unavailable/Partial UNLESS graph completeness is known (no exact-empty on a
   partial graph) — unit-tested with a non-resident partition on the frontier
4. SQLite default (rmap path, no flag) is unchanged
5. compare mode (--engine compare) classifies path mismatches (SQLite vs LiveGraph) into a sidecar
```

## Out of scope (hard guardrails)
```text
No nodes/edges decommission (gated on path/cycles disposition being explicit — this slice makes path's
explicit). No `path` default flip. No cycles. No multi-language (TS partitions only, as callers/callees).
No new SQLite path semantics.
```

## Implementation notes (grounding; confirm during build)
```text
- repo-graph-livegraph: add `path(from, to, ...) -> AnswerEnvelope<PathAnswer>` (headless, Test-API
  surface). BFS over resident slots' `ir.edges`; track whether any frontier node was non-resident/stale
  to drive D3. PathAnswer = the node-key sequence (or none) + contributing_epochs/languages.
- daemon: handle_path engine branch (Engine::Sqlite default; Engine::LiveGraph/Compare like
  callers/callees). The LiveGraph path is behind the flag; auto is NOT used for path this slice.
- rgr: `rmap path` gains `--engine sqlite|livegraph|compare` (default sqlite); --json may surface
  backend_used (consistent with QUERY-MIGRATION-CLI-1); human format unchanged.
- compare: reuse the .rgr/livegraph-compare sidecar pattern; classify path mismatch (different path /
  one finds a path the other doesn't / length differs).
```

## Commit structure (proposed)
```text
1. support: LiveGraph path() API + the D3 completeness logic + unit tests (repo-graph-livegraph)
2. impl:    daemon handle_path engine branch + rgr --engine for path + compare sidecar; SQLite default
```

## References
- `docs/slices/query-migration-cli-1.md` (the `--engine` auto/compare pattern; backend_used metadata)
- `docs/slices/sqlite-raw-decommission-readiness-1.md` (path/cycles as nodes/edges blockers; §2/§6)
- `rust/crates/repo-graph-livegraph/src/lib.rs` (callers/callees BFS-adjacent structures; `Slot.ir.edges`)
- `rust/crates/daemon-runtime/src/dispatch.rs` (handle_path :1212), `rgr/src/commands/graph.rs` (path)
