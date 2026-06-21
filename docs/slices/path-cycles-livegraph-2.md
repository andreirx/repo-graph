# PATH-CYCLES-LIVEGRAPH-2: LiveGraph-backed `cycles` — **BLOCKED / NOT IMPLEMENTABLE under current substrate** (Stage D)

Slice ID: PATH-CYCLES-LIVEGRAPH-2
Status: **BLOCKED — NOT a build slice (2026-06-02).** Grounding probe found a **semantic boundary, not an
implementation gap.** No daemon/CLI code. SQLite remains authoritative for `rmap cycles`. Superseded by the
follow-up chain below.
Depends (for the eventual unblock): IMPORTS-MODULE-INGEST-1, then CYCLES-LIVEGRAPH-1.
Track: Stage D. Does NOT migrate cycles; does NOT decommission `nodes`/`edges`.

## Why this is blocked (the finding)

```text
- `rmap cycles` currently means MODULE-IMPORT cycles: SCC over SQLite `IMPORTS` edges between
  `MODULE`/`FILE` nodes.
- LiveGraph currently carries `CALLS` and `REFERENCES` edges ONLY; it has NO `IMPORTS` edge class and
  NO module graph (no MODULE/FILE-as-module nodes for import topology).
- A LiveGraph SCC over CALLS would answer FUNCTION RECURSION / CALL cycles — NOT `rmap cycles`.
- Therefore `--engine compare` for cycles would be INVALID: it would compare different graph semantics
  (module-import cycles vs call cycles), not two answers to the same question.
- Exact "no cycles" for IMPORT cycles is IMPOSSIBLE from the current LiveGraph substrate (the IMPORTS
  graph is not present to prove absence over).
- SQLite remains AUTHORITATIVE for `rmap cycles`.
- No default migration. No daemon/CLI code in this slice.
- No raw `nodes`/`edges` decommission credit from this slice (cycles still reads SQLite IMPORTS/MODULE).
```

This is the correct result. The probe found a **semantic boundary**, not an implementation gap. Unlike
PATH-LIVEGRAPH-DEFAULT-1 (where the gap was presentation metadata recoverable from the resident IR), here
the required graph **does not exist** in the LiveGraph substrate.

## Grounding evidence (EXECUTED, 2026-06-02)

```text
Q1  SQLite `rmap cycles` semantics + shape:
    storage/queries.rs:942 find_cycles(snapshot, "module") — SCC (Tarjan find_sccs, repo-graph-algorithms)
    over `IMPORTS` edges between `MODULE` nodes ("file" level → `FILE`), SCCs of size > 1.
    daemon dispatch.rs:1180 hardcodes level "module". Output {repo_uid, display_name, snapshot_uid,
    cycles:[{cycle_id, length, nodes:[{node_id, name, file:null}]}], count}. CLI rgr/.../graph.rs:709
    `rmap cycles [--json]` sends {repo} only — no --engine, no scope flag.

Q2  LiveGraph adjacency:
    repo-graph-ir/src/lib.rs:72 — EdgeType is {Calls, References} ONLY; "`Imports` is intentionally
    absent: scip-typescript does not reliably emit import roles (spike M2) … deferred". path() builds
    outgoing adjacency from RESIDENT partitions' SyntaxConfirmedCall (Calls) edges.

Q3  Enough resident outgoing edges for SCC?
    For CALL cycles: yes, over resident Calls edges — but ONLY resident partitions carry outgoing
    adjacency (non-resident keep `defines` + incoming `ref_counts`, not outgoing). For IMPORT cycles
    (what `rmap cycles` computes): NO — no IMPORTS edges, no MODULE nodes.

Q4  Partitions/languages/freshness vs "no cycle" certainty (for any future cycle migration):
    A cycle's closing back-edge can live in ANY partition, so Exact "no cycles" requires the ENTIRE
    graph scope resident + Fresh + TS-only. This is STRONGER than path's frontier-completeness rule —
    one non-resident/stale/non-TS partition invalidates a global "no cycles" claim.

Nuance: FileScopeReference edges (ir:98) "carry module-scope provenance (imports…)" but are symbol-level
`References`, not module→module `IMPORTS`. Reconstructing module-import cycles from them is an INFERENCE
of unproven fidelity — a research question (IMPORTS-MODULE-INGEST-1), not a drop-in.
```

## Options considered (matrix)

```text
A  Defer, record blocker         — RATIFIED. No code. rmap cycles stays SQLite. Honest; avoids a false
                                   import-vs-call equivalence (the cycle-equivalent of a false "no cycles").
B  New call-cycle surface        — Distinct query (call/recursion cycles) behind --engine livegraph,
                                   relabeled, NEVER a migration of rmap cycles. Deferred to the OPTIONAL
                                   CALL-CYCLES-LIVEGRAPH-1 — only if explicitly wanted.
C  Extend ingest (IMPORTS+MODULE) — Large INGEST-CORE change; imports were deferred because scip-typescript
                                   import roles are unreliable. Out of scope; gated behind IMPORTS-MODULE-INGEST-1.
```

## Follow-up chain (the unblock path)

```text
1. IMPORTS-MODULE-INGEST-1  — decide whether SCIP / file-scope references can FAITHFULLY produce
   module-import facts, or whether imports remain tree-sitter/storage-owned. This is the gating decision:
   it determines whether a module-import graph can ever live in LiveGraph honestly.
2. CYCLES-LIVEGRAPH-1       — LiveGraph-backed `rmap cycles` (module-import). ONLY after a module-import
   graph exists in LiveGraph (i.e. after IMPORTS-MODULE-INGEST-1 lands a faithful import graph).
3. CALL-CYCLES-LIVEGRAPH-1  — OPTIONAL, separate. Call/recursion cycles over the LiveGraph call graph.
   Only if explicitly wanted as a NEW query. NEVER presented as a migration of `rmap cycles`.
```

## Impact on raw decommission (SQLITE-RAW-DECOMMISSION-READINESS-1)

`cycles` remains a **default `nodes`/`edges` dependency** (it reads SQLite `IMPORTS` edges + `MODULE`/`FILE`
nodes). The raw graph cannot be retired while `rmap cycles` is SQLite-backed. This slice adds NO
decommission credit; cycles stays on the blocker list until CYCLES-LIVEGRAPH-1 (itself gated on
IMPORTS-MODULE-INGEST-1). Recorded so the readiness audit is not misread as "path + cycles both migrated".

## References
- `docs/slices/path-cycles-livegraph-1.md` (path migration; the `path()` BFS over resident Calls edges)
- `docs/slices/path-livegraph-default-1.md` (path default flip; the completeness/trust labelling pattern)
- `docs/slices/sqlite-raw-decommission-readiness-1.md` (path/cycles as nodes/edges blockers)
- `rust/crates/storage/src/queries.rs:942` (`find_cycles` — IMPORTS/MODULE SCC)
- `rust/crates/repo-graph-ir/src/lib.rs:72` (EdgeType {Calls, References}; Imports deferred)
- `rust/crates/daemon-runtime/src/dispatch.rs:1129` (`handle_cycles`; level hardcoded "module")
