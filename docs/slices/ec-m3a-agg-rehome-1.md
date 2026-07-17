# EC-M3A-AGG-REHOME-1 — g2/g3 aggregate re-home: dead-liveness + map's sketch (EC-1 milestone M-3a)

Status: SPECIFIED (2026-07-17) · IMPLEMENTED (builder, 2026-07-17) · REVISED (builder revision 1,
2026-07-17 — review-0 items 1–4 addressed; §6 delivery record; awaiting review) · Track:
Consolidation milestones (EC-1 §5.2 M-3a, revision-3 scope)
Depends: M-3b (done, ae6e7f8 — the D-EC-7 producer pattern: supplied-from-resolver-output counts,
migration-NULL fallback, transactional promotion deltas, byte-compare harness). HARD ordering:
M-3a ≺ M-6's first drop (after a per-language row drop, liveness silently flips false-dead and the
dep sketch silently thins — §3.4-10).

## 1. Problem

Exactly two read-time mechanisms still consume `edges` CALLS rows outside the FC2a adjacency
walkers: (i) dead-liveness membership — `find_dead_nodes`' `NOT IN` over CALLS∪7 relation types
[queries.rs:1031-1060], served by the modules_list + modules_show dead rollups
[dispatch.rs:7862/:7595]; (ii) map's dep sketch — `map_resolved_dep_edges_in_path`, IMPORTS+CALLS
collapsed to DISTINCT file pairs [queries.rs:2615-2640]. Both break silently under M-6.

## 2. Contract (EC-1 §5.2 M-3a row, as ratified)

1. **WRITE (g2, per-symbol):** the D-EC-7 producer persists per-symbol incoming-CALLS degree,
   computed from the FULL resolver output stream UPSTREAM of storage materialization (the M-3b
   pattern — never recomputed from `edges`), at fresh index AND delta refresh (copy-forward), AND
   coherent under enrichment promotion (extend the `apply_promotion` transactional delta). The same
   family serves §2b's per-function fan-in/fan-out skeleton columns — no additional consumer needed.
2. **WRITE (g3, per-file-pair; D-EC-7-A-i as ratified/recommended):** persist the file-pair
   resolved-dependency family (DISTINCT file pairs — dedup collapses call multiplicity), same
   full-stream/refresh/promotion coherence obligations. Provenance-labeled per the ratified interim
   rule (pipeline accounting, EXPLICITLY TEMPORARY until reconciliation ships).
3. **READ (g2):** modules_list/modules_show dead rollups swap `find_dead_nodes`' CALLS-membership
   input for the persisted degree: liveness = persisted CALLS-degree > 0 OR a row of the 7 retained
   FC2b relation types (that 7-type NOT-IN remains a legitimate owner-read — do NOT migrate it).
4. **READ (g3):** map's sketch swaps its CALLS share to the persisted file-pair family; the IMPORTS
   share stays an owner-read.
5. **stats migrates NOTHING** (verified module-granularity FC2b/FC1 owner-reads, D-EC-5-A); the
   module-rows LG leaf unchanged; trust core untouched (that was M-3b).
6. **Honesty:** pre-migration snapshots (no persisted families) fall back to the labeled live
   row-derived path — never fabricated zeros, never silent partial answers. The M-1 witness stays
   green; manifest/fact-class edits explicit + reviewed.

## 3. Stop conditions

Frozen areas: W-B epoch/coordinator invariants, activity-registry semantics, enrich_pass
semantics, postpass/extractor walks. A parity failure (persisted vs live-derived) is a FINDING —
evidence + DECISION_REQUIRED, not papered over. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

Parity window fresh AND refresh AND post-promotion (persisted g2 degrees / g3 pairs == live
row-derived values while CALLS rows exist — self-validating); a discriminating
non-materialized-row test per family (the M-3b pattern); byte-compare modules_list /
modules_show / map before/after (extend `scripts/byte-compare-five-surfaces.sh` or a sibling
harness); pre-migration fallback tests; chunked cargo gates (standing pattern); witness 15/15;
isolated dogfood.

## 5. Definition of done

No read-time CALLS-row consumer remains outside the FC2a adjacency walkers: dead-liveness and
map's sketch serve from persisted, provenance-labeled, full-stream families on fresh, refreshed,
and promoted snapshots; parity green; the three surfaces byte-identical; fallback labeled;
witness + gates green. The §3.4-10 census re-verifies EMPTY for re-homed mechanisms.

## 6. DELIVERY RECORD (builder, 2026-07-17; REVISED revision 1 — review-0 items 1–4)

**Shape (decide-and-record; D-EC-7-A left storage shape open):** two per-snapshot family
tables (migration 031, mirroring 030's producer pattern) + two nullable `snapshots`
presence markers: `symbol_call_degrees` (g2 — `call_fan_in` serves dead-liveness;
`call_fan_out` is §2b's other per-function skeleton column, same producer per the ratified
M-3a row, no read consumer yet) and `resolved_call_file_pairs` (g3, D-EC-7-A-i —
`call_edge_count` multiplicity persisted so promotion can maintain the DISTINCT pair set
by lawful delta arithmetic; readers filter `> 0`). Markers
`symbol_call_degree_provenance` / `call_file_pair_provenance`: NULL = never persisted
(pre-migration → labeled live-derived fallback), non-NULL = measured (zero rows = measured
zero), stamped `'pipeline'` (the ratified interim-rule accounting, EXPLICITLY TEMPORARY
until recon-design-1). `CHECK (… >= 0)` on every degree/count: a delta that would go
negative aborts the whole promotion transaction — fabricated data is unrepresentable.

**Write census:** all family writers live in `storage/src/crud/call_aggregates.rs` (the
M-3b census discipline): the two pipeline persisters (called at `run_pipeline` Phase-5,
fresh AND delta refresh, values TALLIED FROM THE RESOLVER'S OUTPUT STREAM upstream of
materialization — never derived from `edges`) and the two promotion adjusters (invoked
INSIDE `apply_promotion`'s single transaction, marker-gated never-seed). Orchestrator
tally: per-symbol fan-in/fan-out + cross-file pairs (self-pairs and file-less endpoints
excluded, mirroring the live join it replaces), BTreeMaps for deterministic row order.

**Read swaps:** `find_dead_nodes` [queries.rs] — g2 marker present ⇒ liveness = persisted
`call_fan_in > 0` OR the retained 7-type FC2b NOT-IN (stays an owner-read per the ratified
row); marker NULL ⇒ the pre-M-3a CALLS∪7 live membership (labeled fallback).
`map_resolved_dep_edges_in_path` [queries.rs] — g3 marker present ⇒ IMPORTS owner-read
UNION persisted pairs (UNION ALL exact: type strings cannot collide; outer ORDER BY
reproduces the fallback's total order); marker NULL ⇒ the pre-M-3a combined live query
verbatim. stats/trust/LG leaves untouched (§2.5).

**Review-0 dispositions (revision 1):**

1. **g3-discriminating byte-compare fixture (item 1):** the fixture's CALLS-only pair is
   now SCRIPT-context TS (`src/main.ts` calls `helper()` defined in `src/util.ts`; neither
   file has import/export — the legacy global-scope shape; the call resolves cross-file by
   the resolver's unambiguous bare-name lookup). EXECUTED storage proof: 0 IMPORTS edges
   target `src/util.ts`; the (main.ts → util.ts) pair exists ONLY as CALLS (multiplicity
   2). The non-vacuity guard now runs on BOTH out-A (baseline generates the facts) and
   out-B (candidate's persisted families serve them) and asserts `- src/util.ts` occurs
   EXACTLY ONCE in rendered map output — the dependency-sketch line; no per-file
   `## Imports` list can resupply it, so a broken pair swap drops it to 0 and fails the
   guard before any diff. A module-context pair (extra.ts →IMPORTS+CALLS→ format.ts)
   retains union-branch + per-file-imports + ordering coverage; an intra-file call
   (lonely → helper) pins self-pair exclusion. g2 guard: dead_symbol_count exactly 3
   (lonely/main/extra; helper/fmt alive via CALLS fan-in only).
2. **Complete binding validation (item 2):** EXECUTED this revision — parity
   fresh+refresh (repo-index/tests/call_aggregate_parity.rs), post-promotion + fallback +
   swap-identity + discriminating-source suite (storage/tests/call_aggregate_families.rs
   8/8), promotion unit suite 8/8 (incl. family-delta rollback and never-seeds), the two
   non-materialized-row orchestrator tests, three-surface byte-compare 12/12 (A-vs-C RAW
   6/6 on a real pre-migration DB — migration 031 applied, markers NULL, live fallback
   byte-identical; A-vs-B normalized 6/6 with state-B PROVEN serving from stamped
   families — markers `pipeline|pipeline`, 5 degree rows, 2 pairs), chunked gates
   3772+395+1038 = 5205/0, witness 15/15, isolated dogfood green. Full transcript: relay
   TEST REPORT. Baseline for the byte-compare: HEAD `37b8cd3` built in a git worktree
   (`/private/tmp/rg-m3a-baseline`, kept for reviewer re-runs; `git worktree remove` at
   cleanup). NOTE: iteration-0's leftover baseline binary was PRE-M-3B (29 migrations —
   wrong baseline); it was discarded and rebuilt from HEAD, verified by behavioral probe
   (30 migrations, no family tables, g1 aggregate present).
3. **§3.4-10 census re-verified EMPTY for the re-homed mechanisms (item 3):**
   deterministic grep for `'CALLS'` over `storage/src`, every non-test hit classified
   (call sites of ambiguous functions enumerated by full-tree grep, each production site
   read): (a) the two re-homed mechanisms now read CALLS rows ONLY in their marker-gated
   pre-migration fallback branches [queries.rs:1079, :2711] — the persisted-branch
   `'CALLS'` literal at queries.rs:2696 is a constant SELECT string, not a row read;
   (b) sanctioned FC2a adjacency walkers unchanged: callers/callees leaves
   [agent_impl.rs:1045/:1097], path's default BFS [queries.rs:1703]; (c) FC0
   `extraction_edges` detector reads (not `edges`) [grpc_impl_hint_impl.rs:219/:522];
   (d) `apply_promotion`'s endpoint SELECT [enrichment_impl.rs:596] — write-path delta
   accounting inside the transaction, not a serving read; (e) cfg(test) fixtures.
   RESIDUAL (pre-existing, unchanged, named): `find_dead_nodes_in_path`/`_in_file`
   [agent_impl.rs:579/:635] keep the 8-type NOT-IN, but their ONLY consumers are the
   WITHDRAWN dead-code aggregators, verified returning `AggregatorOutput::empty()`
   unconditionally without touching storage [OBSERVED: aggregators/dead_code.rs] — no
   served read; they fall under the withdrawn surface's reintroduction conditions
   (TECH-DEBT), not under M-3a's re-home obligation (M-6 note: if the surface is ever
   reintroduced, those two queries must swap to the g2 family the same way).
4. **Witness/manifests (items 3+4):** ZERO manifest edits — a reviewed NO-OP, recorded
   here explicitly: no dispatch arm added/removed (66 reconciled), no LiveGraph reader
   change (the swap is SQLite→SQLite), and the three touched arms' declarations already
   carry `FC2a-agg` (map:41, modules_list:50, modules_show:51 — declared from EC-1 §3.3
   revision 3, which classified these consumptions as FC2a-agg g2/g3 BEFORE the re-home;
   M-3a moved the MECHANISM, not the fact class). EXECUTED: witness 15/15 on this working
   tree, including `reader_set_matches_sanctioned_list_on_head` and
   `every_dispatch_arm_is_declared_in_manifest`. Stale "all 30 migrations" doc corrected
   to 31 [migrations/mod.rs; also the "002 through 021" range in the same comment — both
   predated-stale].

**Abstraction record (one line each — users / axis / rejected simpler alternative):**
- `SymbolCallDegree` + `ResolvedCallFilePair` (indexer/storage_port.rs): boundary DTOs
  for the port crossing. Users: orchestrator tally (producer), storage adapter
  (consumer), orchestrator mock (tests). Axis: the M-3b-established supplied-stream
  contract — values cross the boundary as data, never as "recompute instructions".
  Rejected: passing raw BTreeMaps (ties the port to the tally's internal shape).
- `crud/call_aggregates.rs` (module): the write census file. Users: the two pipeline
  persisters (indexer_impl), the two promotion adjusters (enrichment_impl), family_marker
  (both read paths). Axis: auditability — every writer of the two families in ONE file
  (the M-3b census discipline). Rejected: inlining writers at call sites (scatters the
  parity-audit surface across three files).
- `CallAggregateFamily` (enum, pub(crate)): marker-column selector shared by
  `family_marker`'s three call sites (two read paths + promotion gate). Axis: none
  claimed — it exists to keep the marker-validation rule (non-NULL, non-empty) in one
  place. Rejected: two near-identical marker functions (duplicated validation rule).
- Byte-compare normalize() rule 7 (`<kind>-mod-<16hex>` → `<MODUID>`): module-candidate
  uids are SHA256 over the per-run random repo ULID [OBSERVED: package_json.rs:222-235,
  inferred_modules.rs:842-853] — rule-4 volatility in hashed form, rendered only by the
  modules JSON surfaces (M-3b's five surfaces never rendered it, hence new here).

**Known gaps / residuals (named, honest):** (1) `call_fan_out` has no read consumer yet —
written per the ratified M-3a row ("the same family serves §2b's fan-in/fan-out skeleton
columns"), consumed when §2b's per-function surface lands; the parity suite asserts it
anyway. (2) The g3 provenance label shares the interim `'pipeline'` accounting —
EXPLICITLY TEMPORARY until recon-design-1 (same supersession as g1). (3) M-6 forward
obligation (extends M-3b's): when storage-layer CALLS filtering lands, the promotion
insert path must keep its family deltas stream-side, same revisit as
`insert_resolved_edges`. (4) The agent_impl withdrawn-surface residual in census item 3.
(5) No automated end-to-end drive of `run_promotion` → `apply_promotion` through a live
resolver toolchain (adapter-level + served-surface coverage only — the M-3b gap,
unchanged).
