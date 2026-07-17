# EC-M3A-AGG-REHOME-1 — g2/g3 aggregate re-home: dead-liveness + map's sketch (EC-1 milestone M-3a)

Status: SPECIFIED (2026-07-17) · Track: Consolidation milestones (EC-1 §5.2 M-3a, revision-3 scope)
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
