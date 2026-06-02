# IMPORTS-XPART-ENUMERATION-1: multi-partition daemon loading for the cross-partition overlay (Stage D)

Slice ID: IMPORTS-XPART-ENUMERATION-1
Status: **IMPLEMENTED + LIVE-VALIDATED (2026-06-02).** The cross-partition overlay is reachable in the
live daemon: a two-package fixture refreshed via repeated `--source-root` yields `cross_partition=true` in
`rmap cycles --engine livegraph --kind file-import` (JSON + CLI). See **Completion**. Ratified: D1=A (explicit repeated
`--source-root`), D2=B (best-effort per partition), D3=A (sequential), D4=A (repeated `--source-root` on the
existing `livegraph-refresh`), D5=B (per-partition + aggregate), D6=A (structured scope emitted + rendered;
JSON carries structured fields, human render may stringify from them), Fixture=b (committed two-package +
live). Constraints recorded under each decision. No workspace discovery, no SQLite-indexer coupling, no
module aggregation, no raw decommission, no default migration this slice.
Depends: IMPORTS-XPART-WIRING-1 (`deb3e2b` — the in-memory overlay + `ImportCycleScope` flag set; this
slice makes that overlay reachable in the live daemon AND discharges its recorded scope-emission debt),
KEY-NAMESPACE-REPO-RELATIVE-1 (`b72b075` — repo-relative prefixes per partition), the daemon
`livegraph_refresh` path (`run_refresh`), `repo-graph-warm-cache` v5 (per-partition cache).
Track: Stage D, **daemon/runtime loading orchestration**. NO module aggregation. NO raw decommission. NO
`rmap cycles` default migration (SQLite stays default). NO daemon concurrency refactor.

## Goal
```text
Make the ALREADY-BUILT cross-partition import overlay (IMPORTS-XPART-WIRING-1) reachable in LIVE daemon
validation: load TWO+ TS partitions of one repo into the SAME repo LiveGraph so the overlay resolves a
cross-partition import edge and `file_import_cycles()` reports it. The overlay MECHANISM already works
headlessly; this slice supplies the daemon ORCHESTRATION (multiple partitions, one repo, repo-relative
prefixes, per-partition reporting) + the honest scope EMISSION the prior slice deferred.
```

## Grounding (EXECUTED 2026-06-02)
```text
Q1 livegraph_refresh request shape (dispatch.rs:1059-1085 + livegraph_refresh.rs:run_refresh):
   params = { repo, partition (default "default"), project_dir/source_root }. `run_refresh(repo_state,
   repo_uid, partition_id, project_dir)`: assert tsconfig.json under project_dir; compute producer-free
   source_inputs_hash; discover producer (RMAP_SCIP_TYPESCRIPT | PATH); try warm cache (v5) keyed by
   {repo_uid, partition_id, source_inputs_hash}; else run producer -> decode -> ingest_partition(...,
   partition_prefix) -> feed_partition SWAP into repo_state.livegraph (one LiveGraph per repo). Result JSON
   is PER-PARTITION: {status, refreshed, warmed_from_cache, producer_unavailable, partition, nodes, edges,
   value_facts, epoch, source_inputs_hash}. Structured failure model already exists (RefreshFailure: 7
   classes incl. ProducerUnavailable/Timeout/IngestFailed/UnsupportedPartition).
Q2 Package-root discovery: PARSERS EXIST but are NOT wired to livegraph refresh.
   - indexer/src/package_json.rs: parse_package_json (workspaces array + {packages:[]} object) +
     parse_pnpm_workspace (pnpm-workspace.yaml). lerna.json NOT supported.
   - These feed the SQLite indexer (repo-index/compose.rs glob expansion), not the daemon LiveGraph path.
   - The livegraph refresh path has NO enumeration today: project_dir == repo root, partition "default",
     prefix "" (livegraph_refresh.rs:320-322 explicitly defers to THIS slice).
Q3 SCIP producer per partition: `scip-typescript index --cwd <dir> --output <tmp/rmap-scip-refresh-{pid}.scip>
   --no-progress-bar`, 120s timeout, std::process::Command (no shell), stderr captured. Run ONCE per
   run_refresh today. Daemon is single-threaded + !Send (RefCell state) -> producer runs INLINE on the
   request thread; PARALLEL producers would require a concurrency refactor (DAEMON-ASYNC-REFRESH-1
   territory). The producer IS available on the dev machine (a live refresh hit the run_producer path).
Q4 Warm-cache per partition (v5): build_cache_key(repo_uid, partition_id, source_inputs_hash); read/write
   per partition_id (try_read_partition_cache / best_effort_write_partition_cache) + value-facts sidecar.
   Each partition caches INDEPENDENTLY; v5 already in effect (source_file present). No cross-partition cache
   (F1 — overlay is never persisted; it is rebuilt in-memory on each feed).
Q5 Refresh result reporting today: single per-partition object. Multi-partition needs an envelope (per
   partition + aggregate); the RefreshFailure model already supplies per-partition failure classes.
Q6 Single-partition behaviour: preserved by making enumeration ADDITIVE — the existing `livegraph-refresh
   --repo [--partition]` single path stays byte-stable (one root, prefix "", partition "default"); the
   multi path is a new surface that LOOPS the same run_refresh.
```

## Ratified decisions (2026-06-02) — every cell filled

### D1 — Discovery source (where the partition roots come from)
```text
A. EXPLICIT repeated --source-root list (caller names each partition root).            [RECOMMENDED — MVP]
B. WORKSPACE discovery from package.json/pnpm (REUSE indexer/package_json.rs parsers).  [fast-follow]
C. Reuse the SQLite indexer's package discovery wholesale.                              [rejected]
RECOMMENDATION: A now, B as the immediate follow-up (IMPORTS-XPART-ENUMERATION-2). Grounding REFINES the
lean: B is cheaper than "build later" because the PARSERS already exist (package_json.rs) — but wiring them
to the livegraph path crosses indexer->daemon and pulls in glob expansion + workspace-root semantics, which
is its own slice. A makes the overlay reachable NOW with zero discovery risk and a deterministic fixture. C
is rejected: it couples the daemon LiveGraph path to the SQLite indexing pipeline (a boundary we are pulling
APART, not joining).
TRADE-OFF: A needs the caller to know the roots (fine for a fixture + a follow-up that auto-discovers); B is
the eventual ergonomic path; the cost of doing A first is one throwaway CLI surface that B reuses anyway.
```

### D2 — Refresh semantics (multi-partition failure handling)
```text
A. ALL-OR-NOTHING (one partition fails -> whole refresh fails, nothing swapped).
B. BEST-EFFORT per partition (each swaps independently; failures reported, others proceed).  [RECOMMENDED]
RECOMMENDATION: B. The LiveGraph trust model ALREADY degrades on a non-resident/failed partition
(file_import_cycles -> Partial + missing; CYCLES-LIVEGRAPH-1 D2), so a partial load is HONEST, not silent.
B matches the existing per-partition RefreshFailure model and the F1/overlay rebuild (the overlay simply
forms over whatever partitions succeeded). A throws away good partitions and contradicts the "honest
degradation" mission.
TRADE-OFF: B can leave a repo with a SUBSET of partitions resident -> the cross-partition answer is Partial.
That is correct behaviour (the answer says so), but the aggregate refresh status MUST make the partial-ness
loud (see D5) so a caller never reads "ok" when a partition failed.
```

### D3 — Execution model (producer concurrency)
```text
A. SEQUENTIAL producer runs (loop run_refresh per partition on the request thread).   [RECOMMENDED — FORCED]
B. PARALLEL producer runs.                                                             [rejected this slice]
RECOMMENDATION: A. Grounding makes this a CONSTRAINT, not a preference: the daemon is single-threaded +
!Send (RefCell state), so B requires a concurrency refactor (move producers off the request thread, make
state Send/Sync, or spawn a worker pool) — explicitly out of scope (DAEMON-ASYNC-REFRESH-1). A is the only
option that ships without touching the daemon's threading model.
TRADE-OFF: A serializes N producers (each up to 120s) on the request thread -> a long blocking refresh for
big monorepos. Acceptable for a fixture + small repos; the async/parallel path is a separate, ratified
follow-up. Mitigation: warm cache short-circuits unchanged partitions (no producer run), so steady-state
re-refresh is fast.
```

### D4 — CLI / API surface (how the multi-partition refresh is invoked)
```text
A. EXTEND dev `livegraph-refresh` with REPEATED --source-root (each -> one partition).   [RECOMMENDED]
B. NEW dev command `livegraph-refresh-all`.
C. Config file enumerating roots.
RECOMMENDATION: A. A repeated `--source-root <repo-relative-path>` flag (0..N) on the existing hidden dev
command: zero roots / one root == today's single-partition behaviour (byte-stable); N roots loops
run_refresh. Reuses the existing command, transport method, and per-partition result shape. B duplicates the
command for no semantic gain; C front-loads a config format before we know the discovery model (D1=A defers
that). partition_id = the repo-relative source root (e.g. "packages/a"); partition_prefix = the same
(repo-relative), via the existing repo_relative_prefix(repo_path, source_root). The single-partition default
("default", prefix "") is preserved when no --source-root is given.
TRADE-OFF: A overloads one command with single+multi modes; the alternative (B) is a cleaner name but a
second code path. A is chosen for reuse + single-path maintenance.
```

### D5 — Multi-partition refresh result shape (reporting)
```text
A. Per-partition array ONLY (caller aggregates).
B. Per-partition array + AGGREGATE summary {total, succeeded, failed, partial} + overall status.  [RECOMMENDED]
RECOMMENDATION: B. Envelope: { repo_uid, partitions: [ <existing per-partition result | failure> ... ],
aggregate: { total, succeeded, failed, degraded }, status: "AllRefreshed" | "PartiallyRefreshed" |
"AllFailed" }. The per-partition objects are EXACTLY today's run_refresh JSON (+ a failure variant carrying
RefreshFailure.code/detail). The aggregate makes D2's best-effort partial-ness LOUD (never a bare "ok" over
a hidden failure). Single-partition (one root) still returns the same per-partition object at
partitions[0]; the aggregate is additive.
TRADE-OFF: B is a richer response the CLI must render; worth it to avoid a false-success trust gap.
```

### D6 — Honest scope EMISSION (discharges the IMPORTS-XPART-WIRING-1 debt) — REQUIRED by acceptance #4
```text
The daemon cycles handler HARD-CODES the scope string (livegraph_feed.rs:934
"CapturedResolvedRelativeIntraPartition") and the CLI renders that string (graph.rs:830). Once this slice
loads multiple partitions, a daemon-served answer CAN contain overlay edges, so the hard-coded string would
UNDER-REPORT (a false trust claim). Acceptance #4 requires `cross_partition=true` to surface live ->
A. Daemon emits the STRUCTURED env.scope (the D5 flag set { captured_resolved_relative, intra_partition,
   cross_partition, xpart_edge_count }); CLI renders the flags.                          [RECOMMENDED — REQUIRED]
B. Keep a string but recompute it from the flags.                                        [rejected]
RECOMMENDATION: A. The headless answer already carries the D5 `ImportCycleScope`; the daemon must serialize
THAT (not a literal) and the CLI must render it (e.g. "scope: resolved-relative FILE imports [intra +
cross-partition(N)]"). B re-stringifies and loses the per-field precision D5 was chosen for. This is the
ONE intentional CLI change in this slice, and it only discharges debt the prior slice deferred.
TRADE-OFF: A changes the cycles JSON shape (scope: string -> object) + the human line; the SQLite cycles
path is untouched. Recorded as a deliberate, acceptance-driven CLI change (the prior "no CLI" guardrail was
scoped to IMPORTS-XPART-WIRING-1, not here).
```

## Out of scope (hard guardrails)
```text
- Auto-discovery from package.json/pnpm workspaces (D1=B) — a fast-follow (IMPORTS-XPART-ENUMERATION-2),
  REUSING the existing parsers.
- Parallel / async producer execution (D3=B) — DAEMON-ASYNC-REFRESH-1.
- Module aggregation; `rmap cycles` default migration (SQLite stays default); raw SQLite decommission.
- Package/tsconfig-alias resolution (IMPORTS-PACKAGE-RESOLUTION-1) — the resolver stays relative+ext/index.
- Persisted cross-partition overlay (F1 — it is rebuilt in-memory; NEVER cached per partition).
- No daemon threading-model change (state stays single-threaded + !Send).
```

## Required acceptance (EXECUTED later)
```text
1. TWO partitions of one repo load into ONE repo LiveGraph (two slots, distinct partition_ids) via the
   multi-source-root refresh; single-partition refresh stays byte-stable.
2. Keys are REPO-RELATIVE and non-colliding across the two partitions (KEY-NAMESPACE: same-named files in
   packages/a and packages/b yield distinct FILE keys; no `defines` overwrite).
3. The cross-partition overlay contains >= 1 resolved FILE->FILE edge (a relative import in pkg-a resolves
   to a FILE node in pkg-b) — visible via the live answer.
4. `file_import_cycles()` scope reports `cross_partition=true` (and `xpart_edge_count>=1`) when an overlay
   edge participates — surfaced through the daemon + CLI (D6 structured scope), not just headless.
5. Unloading one partition degrades the answer (Partial + missing); the overlay rebuilds without that
   partition's edges (no stale edge).
6. No persisted overlay: a partition's warm cache (v5) round-trips WITHOUT any AstImportFileInventoryResolved
   edge; the overlay exists only in memory.
7. Best-effort (D2): a failing partition is reported (per-partition failure + aggregate "PartiallyRefreshed")
   while the others load; the cross-partition answer is honestly Partial.
8. Full gate (workspace test, clippy -D warnings, fmt) + LIVE: `dev-install-local.sh`, refresh a committed
   two-package fixture with repeated --source-root, then `rmap cycles --engine livegraph --kind file-import`
   shows the cross-partition cycle with cross_partition=true.
```

## Validation fixture (D-fixture, to ratify)
```text
Options: (a) hand-built two-partition unit test only; (b) COMMITTED two-package TS fixture + live producer
run; (c) a local real repo.
RECOMMENDATION: (b). Grounding shows the producer IS available on the dev machine, so a committed
`tests/fixtures/xpart-monorepo/{packages/a,packages/b}` (each a tsconfig'd TS package; a imports b and b
imports a for the cycle) can be refreshed LIVE end-to-end — exactly the slice's goal (reachability in live
validation). Keep the headless 2-partition LiveGraph tests (already in IMPORTS-XPART-WIRING-1) as the
deterministic core; the committed fixture adds the live path. If the producer is later absent in CI, the
live step degrades to NOT RUN + documented, headless still gates.
```

## Build contract (PROPOSED — gated on ratification)
```text
1. orchestration (daemon-runtime): a multi-partition refresh that LOOPS run_refresh over the given
   repo-relative source roots (sequential, D3), each with partition_id = repo-relative root + prefix via
   repo_relative_prefix; collect per-partition results + aggregate (D5). Single-partition path unchanged.
2. surface (rgr + dispatch): repeated --source-root on dev `livegraph-refresh` (D4); transport threads the
   list; response renders per-partition + aggregate.
3. scope emission (D6): daemon cycles handler emits structured env.scope; CLI renders the flag set. SQLite
   path untouched.
4. fixture + validation: committed two-package fixture; headless tests stay; live cycles shows
   cross_partition=true.
5. docs: completion + evidence + the ENUMERATION-2 (workspace auto-discovery) follow-up.
```

## Completion (implemented 2026-06-02, EXECUTED)

Commits: `3e8dde5` (spec) + `faa5c13` (1/5 orchestration) + `90063c1` (2/5 dispatch+CLI) + `821c01a`
(3/5 structured scope, D6) + `6452f92` (4/5 filename-safe partition ids) + this doc (5/5).

### What landed
```text
daemon-runtime/livegraph_refresh.rs : run_refresh gains partition_prefix (threaded to ingest_partition);
  derive_partition_target (repo_path, source_root) -> (project_dir abs, partition_id, prefix repo-relative;
  prefix "" -> "default"); run_refresh_multi: sequential (D3) best-effort (D2) loop over source roots,
  feeding all into ONE repo LiveGraph (distinct partition_ids -> overlay rebuilds), returning per-partition
  + aggregate {total,succeeded,failed,degraded} + status (D5).
daemon-runtime/dispatch.rs          : handle_livegraph_refresh routes a non-empty `source_roots` array to
  run_refresh_multi; absent/empty preserves the single-partition path (D4).
rgr/commands/graph.rs               : dev livegraph-refresh repeated --source-root; cycles render reads the
  STRUCTURED scope (D6) and stringifies the human line ("[intra-partition + cross-partition(N)]").
daemon-runtime/livegraph_feed.rs    : file_import_cycles_response emits the structured D5 scope object
  (scope_json/default_scope_json), replacing the hard-coded string (discharges the WIRING-1 debt).
daemon-runtime/livegraph_warm_cache.rs : filename_safe_partition_id (path-sep -> "_") for the .cache/.vf
  filenames; run_refresh sanitizes the temp .scip name (the live-validation ENOENT fix).
fixtures/xpart-monorepo             : packages/a (imports ../../b/src/foo) + packages/b (imports
  ../../a/src/main) -> the cross-partition file-import cycle.
```

### Live validation (EXECUTED 2026-06-02, dev-install + real scip-typescript producer)
```text
dev-install-local.sh -> daemon healthy. `rmap dev livegraph-refresh --repo <fixture> --source-root
  packages/a --source-root packages/b`:
  - FIRST run (pre-fix) surfaced a real bug: partition_id "packages/a" (slash) broke the temp .scip path
    -> scip-typescript ENOENT -> per-partition ProducerFailed + aggregate AllFailed. The best-effort +
    aggregate machinery behaved CORRECTLY under failure (no crash, partial-ness loud) -> D2/D5 validated
    live BEFORE the fix. Fixed in 6452f92 (filename-safe ids).
  - SECOND run (post-fix): both partitions Refreshed (3 nodes, 0 edges each; distinct source_inputs_hash);
    aggregate {total:2, succeeded:2, failed:0}, status AllRefreshed. 0 intra edges => the real producer did
    NOT pull B's file into A's partition; the relative cross-package import is partition-unresolved (the
    overlay precondition) -- the producer-determinism question RESOLVED in our favour, not papered over.
`rmap cycles --engine livegraph --kind file-import` (both resident):
  scope = { captured_resolved_relative:true, intra_partition:false, cross_partition:true, xpart_edge_count:2 },
  class=Exact, freshness=Fresh, missing=[]; cycle packages/a/src/main.ts <-> packages/b/src/foo.ts built
  ENTIRELY from the overlay; keys repo-relative + distinct + non-colliding. CLI human line:
  "[cross-partition(2)]".
```

### Acceptance outcomes
```text
1. two partitions in one LiveGraph                       PASS (both Refreshed, both resident).
2. repo-relative keys distinct / non-colliding           PASS (packages/a/... vs packages/b/...).
3. overlay contains >= 1 cross-partition edge            PASS (xpart_edge_count == 2, live).
4. file_import_cycles scope cross_partition=true (live)  PASS (daemon JSON + CLI render, D6).
5. unload one partition degrades                          HEADLESS (no daemon single-partition UNLOAD CLI;
   covered by IMPORTS-XPART-WIRING-1 unload_rebuilds_overlay_without_the_edge_and_degrades).
6. no persisted overlay                                   PASS (structural: PartitionIr has no overlay field).
7. best-effort partial failure is loud                   PASS (the pre-fix ProducerFailed run -> AllFailed).
8. full gate + live                                       PASS (see Validation evidence).
```

### Observations recorded (NOT papered over)
```text
O1 NESTED FIXTURE REPO RESOLUTION: the committed fixture lives UNDER the repo-graph repo, so both the
   refresh (--repo <fixture>) and `rmap cycles` (cwd) resolve to the ENCLOSING repo-graph registration
   (repo_uid repo_01ks2..., display_name "repo-graph", an old snapshot_uid). The partitions still load with
   CORRECT repo-relative prefixes ("packages/a"/"packages/b") because repo_relative_prefix used the fixture
   path I passed as --repo; the overlay + keys + cycle are correct. The display_name/snapshot belonging to
   the enclosing repo is cosmetic impurity inherent to a committed-in-repo fixture. A standalone registered
   fixture repo would be cleaner (follow-up).
O2 HUMAN RENDER SAYS "module-level": the NON-empty file-import cycle falls through to the generic
   CyclesResponse::render_human ("1 module-level cycle found" / "(2 modules)" / "rmap modules deps"), which
   CONFLATES FILE-import cycles with MODULE cycles -- the exact distinction the project guards. The scope
   line + JSON are correct; only the generic body wording is wrong. PRE-EXISTING (CYCLES-LIVEGRAPH-CLI-1,
   which special-cased only the EMPTY message), now more visible. Recorded as debt (a file-import-specific
   non-empty renderer); not fixed here to avoid expanding CLI scope beyond D6 -- flagged for ratification.
```

### Validation evidence
```text
EXECUTED: cargo test -p repo-graph-daemon-runtime (orchestration + filename-safe helpers green);
  cargo test --workspace (220 binaries ok, 0 failures); clippy --workspace --all-targets
  -- -D warnings (clean); cargo fmt --all -- --check (clean).
LIVE (EXECUTED): dev-install healthy; two-partition refresh AllRefreshed; cross-partition file-import cycle
  with cross_partition=true (JSON + CLI). Best-effort/aggregate validated live (the pre-fix AllFailed run).
HEADLESS: acceptance #5 (unload degradation) via IMPORTS-XPART-WIRING-1 (no daemon unload CLI).
```

## Follow-up slices
```text
- IMPORTS-XPART-ENUMERATION-2 : auto-discover roots from package.json/pnpm workspaces (REUSE
  indexer/package_json.rs), replacing the explicit --source-root list.
- DAEMON-ASYNC-REFRESH-1 : move producers off the request thread (enables parallel D3=B).
- IMPORTS-PACKAGE-RESOLUTION-1 : tsconfig path aliases + package exports/types.
- CYCLES-FILE-IMPORT-RENDER-1 (from O2) : a FILE-import-specific NON-empty CLI renderer so a file-import
  cycle is never printed as "module-level cycle" / "(N modules)" / "rmap modules deps" (the generic
  CyclesResponse text). Scope line + JSON are already correct; only the human body conflates the two cycle
  families the project otherwise guards.
- XPART-FIXTURE-STANDALONE-1 (from O1) : a standalone registered fixture repo so live validation does not
  resolve to the enclosing repo-graph registration (cosmetic repo_uid/display_name impurity).
```

## References
- `docs/slices/imports-xpart-wiring-1.md` (the overlay + the scope-emission debt this discharges)
- `rust/crates/daemon-runtime/src/livegraph_refresh.rs` (`run_refresh`; producer invocation; per-partition result)
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (`repo_relative_prefix`; preload prefix threading; cycles JSON :934)
- `rust/crates/daemon-runtime/src/dispatch.rs` (`livegraph_refresh` handler; partition default; `handle_cycles`)
- `rust/crates/indexer/src/package_json.rs` (workspace/pnpm parsers — REUSE target for ENUMERATION-2)
- `rust/crates/rgr/src/commands/graph.rs` (dev `livegraph-refresh` / cycles render :830)
- `rust/crates/repo-graph-warm-cache/src/lib.rs` (per-partition v5 cache key)
