# DAEMON-RESIDUALS-2 — retention: reproduce, measure, fix what was measured, then prevent

Status: SPECIFIED (2026-09-05) · Track: split from DAEMON-RESIDUALS-1 (human-ratified
(a)+(b) + prevention set; the FK-index premise RETRACTED — indexes exist since
001-initial.sql). CODE slice, diagnose-then-fix. Maturity: MATURE.

## 1. Problem (MEASURED on production, 2026-09-04)

A retention prune on a 4.8 GB / 29-snapshot repo-graph store ran 5h+ with ZERO committed
progress (per-snapshot transactions; cascades already index-seek; 2 MB page cache; the
daemon restarts each time killed it since May). A `repo remove` + `index` rebuild took 40 s
(253 MB, 1 snapshot). The prune's actual mechanism is UNDIAGNOSED.

## 2. Contract

1. **Reproduce on a representative multi-snapshot store** (6–8 isolated re-indexes of a
   mid-size repo, or a snapshot-heavy fixture) and MEASURE per phase: which statement,
   which table, rows/s, page-cache behaviour, WAL growth, FK RESTRICT checks on
   non-cascade children, the `parent_snapshot_uid` update. The diagnosis is a deliverable.
2. **Fix ONLY the measured mechanism** (chunked per-snapshot deletes with the write slot
   released between chunks are ratified regardless; `cache_size` sized to the store for
   maintenance connections is ratified regardless).
3. **(b) Rebuild-instead-of-delete** when the prunable share dominates (copy the kept
   snapshot(s) into a fresh file and swap; no VACUUM needed), exposed as
   `rmap maintenance rebuild`.
4. **Prevention set** (human directive "no future purge takes this long"): snapshot hard
   cap (current + parent) with prune-on-commit at index/refresh commit; a retention time
   budget that aborts at a chunk boundary and flips the repo to the rebuild path; `doctor`
   shows store size, snapshot count, prunable share, last-pass duration + progress with
   an ETA basis; a retention BENCHMARK GATE (N snapshots × M rows under a fixed bound) in
   the test suite.
5. Frozen invariants: single-writer per DB FIFO-fair; readers never see partial writes;
   W-B epoch/coordinator semantics; wire protocol; the 300 s client timeout value.

## 3. Stop conditions

Frozen as §2.5; storage schema additive only; exit codes. STANDING HONESTY RULES (Busy
named; progress stated with basis). If the measured mechanism needs more than chunking +
cache sizing + rebuild, STOP + DECISION_REQUIRED with options. Never touch the operator's
real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Reproduction FIRST with per-phase numbers; unit: chunk boundaries preserve per-snapshot
  atomicity; cap + prune-on-commit; budget → rebuild; rebuild swap atomic; benchmark gate.
- Live proof (isolated state root, registry sha unchanged): the reproduced store prunes
  (or rebuilds) under the bound with a concurrent foreground read loop staying under the
  patience; doctor fields verbatim. Gates first, proofs small; delete every isolated root.

## 5. Definition of done

Mechanism measured and named; the measured fix + rebuild path + prevention set shipped
under the frozen invariants; benchmark gate green; gates green.

## 6. Ratification after cycle 1 (2026-09-06)

**Mechanism MEASURED (cycle 1, harness `storage/src/retention/tests/reproduce.rs`, e957aeb):**
the FK cascade from `DELETE FROM snapshots` → `nodes` performs, per deleted node, child lookups
on `edges.source_node_uid`, `edges.target_node_uid`, `unresolved_edges.source_node_uid`,
`nodes.parent_node_uid` that are full SCANs — every existing index on those tables is composite
with `snapshot_uid` leading and cannot serve a bare child-column predicate (proved
size-independently via `EXPLAIN QUERY PLAN`). Cost O(nodes × child rows) per snapshot.
Toy store (4 snapshots, 8,000 nodes + 16,000 edges each, 68 MB, pruning 2): current 374.5 s;
+512 MB cache 264.9 s; FK-ON pre-delete children (V1) ~200 s then FK error; FK-OFF explicit
deletes (V2) 0.52 s; single-column FK indexes (V3) 0.84 s (+0.29 s one-time build). This
RECONCILES the 2026-09-05 retraction: the indexes exist, and do not serve the cascade. §2.2's
"chunking + cache sizing" pair is measured-insufficient on its own.

**DR-1 — HUMAN RULING: B.** An additive migration adds single-column indexes on the measured
FK child columns (the four above, plus any other snapshot-scoped FK child column the schema
introspection finds); the existing FK-ON cascade prune is kept; SQLite keeps enforcing
integrity. Chunked per-snapshot deletes with the write slot released between chunks and the
maintenance `cache_size` remain ratified alongside. The slice MEASURES the insert-path cost of
the new indexes (index and refresh wall time on the harness corpus and one isolated
repo-graph index, before/after) and the store-size delta, and REPORTS them verbatim — the
human's expectation is "a little"; a material cost is surfaced, not absorbed.

**DR-2 (operator rulings):** (i) the diagnosis harness ASSERTS what it proves — the EXPLAIN
test asserts SCAN on the four bare predicates before the migration and SEARCH after; the
timing harness records the phases the contract names (§2.1) and becomes the seed of the
benchmark gate; (ii) SPLIT: **increment 1** = the B migration + assertion-bearing harness +
chunked/cache-sized prune + retention benchmark gate (storage crate, self-contained);
**increment 2** = rebuild path + `rmap maintenance rebuild` (additive maintenance DTO variant,
never a new wire message) + snapshot hard cap + prune-on-commit + time budget → rebuild +
doctor fields. Increment 1 ships first; §5's DoD is met by both together.

CORPUS PATHS: leveldb at ../legacy-codebases/leveldb; FRAKTAG at ../FRAKTAG; repo-graph is
THIS repo.
