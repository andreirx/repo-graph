# ENGINE-CONSOLIDATION-1 — One current-state read engine (SPEC)

Status: SPEC SLICE — analysis + spec only, NO code changes (2026-07-02)
Track: Focus / consolidation · Origin: fresh-eyes v0.4.0 review

## 1. Problem — two graph engines, every feature pays twice

The daemon serves current-state answers from **two coexisting engines**: the
SQLite pipeline (indexer/storage/repo-index, ~40k LOC in `storage` alone) and
the in-memory **LiveGraph** stack (`repo-graph-ir`, `scip-ingest`,
`trust-model`, `livegraph`, `warm-cache`, `coherence` + two feed adapters,
~13k LOC). `daemon-runtime` depends on both (28 sibling crates);
`livegraph_feed.rs` alone is ~4.7k lines of adapter glue; the W-B epoch work
had to define coherence witnesses *across* the pair (RequestEpoch fingerprint,
fail-soft to pinned SQLite). Per-surface migrations exist as individual
slices (ORIENT-LIVEGRAPH, STATS-LIVEGRAPH, CYCLES-LIVEGRAPH, …) and the
substrate-decommission arc established the permanent floor (**SCIP carries no
unresolved-call disposition → trust unresolved-call fields are RED by design;
full SQLite decommission is impossible** — ratified, Option A bounded
partial). What does NOT exist is a **named end-state**: which engine owns
which read path when the migration is *done*, and what "done" means. Until
that is written down, every new daemon feature pays double integration cost
and the migration has no finish line.

## 2. Deliverable — a ratifiable end-state spec, written into this doc

The builder (analysis only — no code changes) extends this document with:

**§3 Read-path ownership inventory (evidence, not opinion).** For every
daemon request handler (the ~36 in `daemon-runtime/src/dispatch.rs`): which
store(s) it reads today (LiveGraph, SQLite, both — cite the mixed-read
enumeration in `docs/slices/daemon-w-b-epoch-1.md` §7.3), and which fact
classes it needs (resolved graph, unresolved disposition, measurements,
declarations, history/snapshots).

**§4 Fact-class → engine assignment (the proposal).** For each fact class, a
proposed permanent owner, honoring the known floors: unresolved-call
disposition is SQLite-only (RED by design); declarations/governance and
snapshot-scoped history are persistence-shaped; the current-state resolved
graph is LiveGraph-shaped per the VISION's operational architecture. Name
what "both" costs where it must remain (coherence witnesses, epoch binding).

**§5 End-state definition + milestones.** A checkable definition of
"consolidated" (e.g. "no handler reads both stores for the same fact class;
SQLite reads happen only for its owned fact classes or as labeled fail-soft
fallback"), and a milestone sequence from today's state to it, each milestone
independently shippable and smoke-gateable. Include the retirement (or
explicit permanence) of `livegraph_feed.rs`-style double-integration glue.

**§6 Decisions for ratification.** Each ratification-class choice marked
DECISION_REQUIRED with alternatives and trade-offs stated against the VISION
(the three commitments + change-cost doctrine). Expected decisions include at
least: the fact-class ownership table; whether any mixed-read handler remains
permanently mixed; what happens to per-surface `*-LIVEGRAPH-*` slice plans
that the end-state supersedes.

## 2b. Operator direction (2026-07-04) — candidate end-state to evaluate seriously

The operator's proposed split, to be weighed as a primary candidate in §4:

- **SQLite keeps the STRUCTURE skeleton:** modules, files, functions with
  signatures, file→module ownership, and per-function AGGREGATES (fan-in/
  fan-out counts, complexity value) — the slow-changing, small,
  orient/stats/hotspots-serving layer.
- **LiveGraph owns function INTERNALS:** body-level call sites and edge
  lists (what callers/callees/path walk) — the fast-changing, blob-heavy,
  per-file-rebuildable layer, persisted only via the warm cache.
- **Snapshot degrades to a provenance stamp on the current state** (identity
  for comparability/toolchain/epochs), not retained copies —
  SNAPSHOT-RETENTION-1 already enforces current + delta-base only.

Named collision the spec MUST resolve, not skirt: the RED floor (unresolved-
call disposition is SQLite-only, ratified) is a body-level fact class. Either
disposition rows stay behind as a compact SQLite exception (size the cost),
or re-opening the floor is proposed as an explicit DECISION_REQUIRED with the
persistence story (files-are-system-of-record applies to the warm cache too).
Also size the win: estimate DB footprint and per-refresh write volume under
this split for a kernel-scale repo vs today.

## 3. Stop conditions

- NO code, schema, or contract changes in this slice — spec only.
- If the inventory contradicts a ratified prior decision (e.g. the RED floor
  or a W-B epoch invariant), surface the contradiction as DECISION_REQUIRED;
  do not silently reinterpret it.
- Do not propose retiring the SQLite floor — it is ratified as permanent.

## 4. Validation

- The four sections above exist in this doc, the §3 inventory covers every
  dispatch handler (count stated and reconciled against `dispatch.rs`), and
  every proposal in §4/§5 cites §3 evidence.
- No working-tree changes outside this file.

## 5. Definition of done

This doc contains an evidence-backed read-path inventory, a fact-class
ownership proposal honoring the ratified floors, a checkable end-state
definition with shippable milestones, and an explicit DECISION_REQUIRED list
— ready for decision-review and human ratification. (IMPL slices follow only
after ratification.)
