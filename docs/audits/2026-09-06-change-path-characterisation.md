# Change-path characterisation — how a diff reaches the representations today (2026-09-06)

Read-only investigation at a551df8, prompted by the human's next-horizon question: how would a git
diff map to changes in the in-memory representation and in SQLite. This is EVIDENCE, not a design.
All claims cite code; "UNDETERMINED" where stated.

## Two unrelated "refresh" mechanisms
- `rmap refresh` (SQLite): dispatch.rs:3281-3609 → compose.rs:3773-3899 → orchestrator::refresh_repo.
  Per-file delta at the EXTRACTION layer only; resolution and every aggregate re-run whole-snapshot.
  Never touches the LiveGraph.
- `livegraph_refresh` (in-memory, TS packages only): whole-partition scip-typescript re-index + atomic
  slot swap (livegraph_refresh.rs:170-178, 222-363; livegraph/src/lib.rs:401-501). No per-file anything.
  REFRESH-PROBE-1 measured only this one.

## 1. `rmap refresh`
- Change detection = CONTENT HASH vs the parent snapshot's file_versions (orchestrator.rs:1645-1695;
  invalidation.rs:116-241: Unchanged|Changed|New|Deleted|ConfigWidened; a changed recognised config
  widens its directory scope, root configs widen globally :179-198). Not git, not mtime. Parent =
  latest READY snapshot (get_latest_snapshot), NOT the parent_snapshot_uid link.
- Extraction is delta: only files_to_extract go to run_pipeline (:1883-1921); unchanged files are
  ROW-COPIED forward first (§2). files_to_delete is computed and never consumed outside tests; a
  deleted file's repo-scoped `files` row persists (no DELETE FROM files anywhere).
- Resolution is NOT delta: the resolver re-runs over ALL extraction edges of the new snapshot, copied +
  fresh (:852-1085; comment :917-919 "batches cover ALL extraction edges"). edges/unresolved_edges are
  written fresh every snapshot — never copied.
- Recomputed from scratch every refresh: MODULE nodes (:829-847), module edges (:1087-1099 "rebuilt
  every snapshot"), call degrees / file pairs / resolved-call aggregate (:936-938, :1122-1131), module
  ownership for all files (compose.rs ~:4346-4360), Rust/C++ is_test reclass over the whole snapshot
  (~:4185-4215), HTTP boundary surfaces (~:4313-4316), contract schemas (:1947-1957), enrichment CALLS
  promotion and seeds afterwards over the whole snapshot.
- Copied forward for unchanged files: nodes, extraction_edges, file_signals, file_versions
  (indexer_impl.rs:719-1153), measurements, inferences, C/TS boundary surfaces
  (refresh_copy_forward_impl.rs:42-462). Docs (semantic_facts) are repo-scoped, on-demand, outside
  refresh.
- Impact propagation (compose.rs:4059-4140 + impact_propagation.rs:95-150): loads ALL nodes of the new
  snapshot, string-matches stable_key against changed paths, marks provenance-referencing rows in the
  freshness-tracked families (inferences, module_candidates, project_surfaces, surface_*,
  boundary_contracts, boundary_interaction_links). nodes/edges/measurements have no freshness columns.

## 2. Snapshot model — FULL ROW COPIES, never deltas
copy_forward_unchanged_files (indexer_impl.rs:719-1153): nodes SELECT…INSERT with NEW node_uid, same
stable_key, parent_node_uid remapped (:802-846); resource nodes; extraction_edges with target_key copied
verbatim (:959-1039); file_signals; file_versions. Measurements/inferences copied via a SUBSTR path match
on target_stable_key (34 s on Django). Seed vectors copied by (stable_key, file content_hash).
parent_snapshot_uid: written at orchestrator.rs:1768; read only by seed_impl.rs:362-369 (prior vectors),
retention/prune.rs:215-218 (null the link), compose.rs:3933 (drive copy-forward). No query reconstructs
anything through the chain.
Snapshot-scoped tables (every CREATE TABLE with snapshot_uid): snapshots, file_versions, nodes, edges,
declarations, inferences, artifacts, evidence_links, measurements, annotations, unresolved_edges,
boundary_provider_facts, boundary_consumer_facts, boundary_links, staged_edges, file_signals,
module_candidates, module_candidate_evidence, module_file_ownership, extraction_edges, project_surfaces
(+_new), project_surface_evidence, surface_config_roots, surface_entrypoints, surface_env_dependencies,
surface_env_evidence, surface_fs_mutations, surface_fs_mutation_evidence, module_discovery_diagnostics,
quality_assessments, status_mappings, behavioral_markers, return_fates, boundary_interaction_surfaces,
boundary_channel_details, contract_schemas, generated_code_mappings, boundary_interaction_links,
symbol_call_degrees, resolved_call_file_pairs, seed_vectors. Repo-scoped: repos, files,
schema_migrations, semantic_facts; child tables via parent uid: contract_elements, boundary_contracts.
(This list IS the "snapshot closure" DR-1 option A must enumerate — derivable from the schema.)

## 3. Stable keys — path-based identity, no rename tracking
file_uid = "{repo}:{rel_path}"; FILE "{repo}:{path}:FILE"; MODULE "{repo}:{dir}:MODULE"; SYMBOL
"{repo}:{file}#{name_or_qualified}:SYMBOL:{SUBTYPE}" (+ :dupN). A rename/move ⇒ New + Deleted at every
layer: new keys for every symbol, seeds re-embed under the new key, cursors change, measurements/
inferences don't copy. No rename tracking anywhere in the index path (git/src/churn.rs uses
--no-renames; basis.rs keeps only the post-rename path). Consumers that depend on keys surviving:
measurement/inference copy-forward, impact propagation, seed carry-forward, user cursors, LiveGraph
xref maps and cert fingerprints, resolver maps. idx_nodes_repo_key(repo_uid, stable_key) exists
cross-snapshot; no reader uses it for cross-snapshot identity (UNDETERMINED beyond the searches run).

## 4. LiveGraph
LiveGraph{slots: partition_id → Slot{epoch, status, ir: PartitionIr (flat Vec<IrNode>/Vec<IrEdge>,
linear incoming/outgoing), defines, ref_counts, …}, xref_epoch, xpart_overlay} (livegraph/src/lib.rs:
107-120, 283-292). Partitions = TS packages only. Built from scip-typescript output (whole program), not
SQLite. livegraph_refresh replaces the WHOLE partition under the write lock held only for the swap;
xref_epoch bumps; overlay rebuilt over all resident partitions. No per-file mutation API. W-B epochs:
one RequestEpoch{snapshot, fingerprint} per request; every LiveGraph serve validates the fingerprint or
falls back to SQLite@N. Per-file invalidation: only invalidation.rs (extraction plan); git changed_files
feeds only the drift DISPLAY.

## 5. Reverse dependencies — none usable for re-resolution
edges has reverse indexes (snapshot_uid, target_node_uid) but keyed by per-snapshot node_uid that changes
on every copy. extraction_edges/unresolved_edges forward-only; target_key unindexed. Resolver maps all
forward. LiveGraph ref_counts = incoming COUNT per key, no source list. "Given changed target file T,
find every extraction edge whose target_key resolves to T" does not exist — answered today by re-resolving
everything.

## 6. Seeds — the one incremental-shaped mechanism
Unit of publish = the whole seed_vectors row set of one snapshot (DELETE by snapshot then insert all,
seed_impl.rs:282-330); the incremental part is the EMBED: per chunk reuse when (stable_key, FILE
content_hash) matches the parent's (pass.rs:132-137, 172-224) — one edited line re-embeds every chunk of
that file; a rename re-embeds all. FORGET-vs-SEED / generation supersede at seed_pass.rs:44-75, 175-234.

## 7. Cost shape today
No measurement of `rmap refresh` vs full index exists in docs. Evidence: Django NO-CHANGE refresh
(3015 files, 121K measurements) ≈ 84-102 s (deep-dives/refresh-path-algorithms.md:182-201; RMAPD-PERF-2):
scan 4 s; core refresh 62-72 s (row copy-forward + full re-resolution, 0 files extracted); measurement
copy 34 s; vs full index of repo-graph ~40 s. A no-op refresh costs the order of a full index.
LiveGraph (REFRESH-PROBE-1): FRAKTAG engine no-op 1.955 s vs one-file edit 1.925 s — "the refresh unit
is the partition, not the file"; T_scip dominant; verdict B (two-speed) ratified but the AST fast-delta
path is not wired.

## What a one-file diff would invalidate today, per representation
| Representation | Scoped by | Recomputed on refresh today | Needed for a per-file delta |
|---|---|---|---|
| files catalog | repo | upsert all; deletes never removed | delete handling; rename tracking |
| file_versions | snapshot | changed rewritten; unchanged ROW-COPIED | in-place update |
| nodes | snapshot; key embeds path | changed re-extracted; unchanged ROW-COPIED w/ new uid; MODULE nodes rebuilt | stable identity across snapshots; rename tracking |
| extraction_edges | snapshot, fwd by source file | changed re-extracted; unchanged ROW-COPIED | — |
| edges / unresolved_edges | snapshot | ALL re-resolved | reverse map target_key → dependent sources; rename tracking |
| module edges, call degrees, pairs | snapshot | ALL | aggregate recompute after re-resolution |
| module_candidates / ownership | snapshot | ALL | recompute keyed on config changes |
| measurements, inferences | snapshot; path in key | changed recomputed; unchanged copied via SUBSTR | indexed path column or no-copy |
| HTTP boundary, is_test reclass, contracts | snapshot | ALL | per-file + cross-file handling |
| enrichment CALLS | snapshot | ALL (background) | reverse map + rename tracking |
| seed_vectors | snapshot | corpus rebuilt; vectors reused per (key, file hash) | per-chunk hash; rename tracking |
| semantic_facts (docs) | repo | none (on demand) | n/a |
| LiveGraph IR/xref/overlay | memory, per TS package | untouched by refresh; whole-partition swap | per-file IR mutation; per-file SCIP producer (none) |
| LiveGraph ↔ SQLite coherence | request epoch | any change REDs every cert | epoch semantics admitting a per-file bump |

Carry-forward facts: (1) refresh knows the changed-file set by hash; only extraction and row copy honour
it. (2) Snapshots are full row copies — a one-line Django edit copies ~160K rows and re-resolves 81K
edges. (3) Identity is path-based; a move is delete+create at every layer incl. seeds and cursors.
(4) No reverse dependency map exists. (5) The LiveGraph is a separate TS-only whole-partition store.
