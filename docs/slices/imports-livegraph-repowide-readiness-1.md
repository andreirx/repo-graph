# IMPORTS-LIVEGRAPH-REPOWIDE-READINESS-1: repo-wide directional no-loss imports compare

Slice ID: IMPORTS-LIVEGRAPH-REPOWIDE-READINESS-1
Status: **IMPLEMENTED + MEASURED (2026-06-07). VERDICT: GREEN-SAFE** — zero regression + zero unknown over 1303
files across xpart/amodx/repo-graph/OpenXcom; every non-TS file fallback-by-precondition (OpenXcom YELLOW =
pure-non-TS control). D6=A. Commits 40df5e4 (spec) -> 94164d5 (impl); reports under
`docs/audits/imports-repowide-readiness-1/`. UNPUSHED. See **Completion**.
Measurement/readiness
ONLY. Run the per-file DIRECTIONAL no-loss compare (READINESS-1) across ALL import-bearing files in selected
repos, and emit an aggregate report + verdict. NO default flip, NO decommission, NO resolver changes, NO CLI
default change. The deliverable is the repo-wide readiness REPORT + verdict; the default flip is a SEPARATE
slice (IMPORTS-LIVEGRAPH-DEFAULT-1) gated on this.
Depends: IMPORTS-LIVEGRAPH-DEFAULT-READINESS-1 (the per-file `imports --engine compare` + `imports_compare_
sidecar` this generalizes; the D3 precondition; the directional no-loss criterion). Track: Stage D,
QUERY-MIGRATION-1.

## Why now (priority path)
```text
The per-file compare is GREEN-SAFE on the sampled D4 files (zero regression). The default migration still needs
BROADER COVERAGE -- the open work is COVERAGE, not a known regression. The repo-wide compare was explicitly
deferred from READINESS-1 D1. This slice closes that coverage gap with an aggregate measurement, then the
default flip becomes a coverage-backed decision.
```

## Grounding (EXECUTED 2026-06-07)
```text
PER-FILE GATE (reuse): imports_compare_sidecar already computes the directional verdict per file
  (FallbackPreconditionUnmet / Regression / NoLossLivegraphSuperset / NoLossEquivalent) from {sqlite resolved-
  local targets, LiveGraph edge targets, the D3 precondition}. The repo-wide harness calls this PER FILE over a
  bulk-enumerated file set + AGGREGATES.
SQLite enumeration: NO bulk "import-bearing source files" query exists (only per-file `find_imports` + the
  prefix-bounded `find_imports_between_paths`). The harness needs a NEW read-only bulk query: all IMPORTS edges
  for a snapshot as (source_file, target_file, kind, subtype, resolution) -- the SQLite import-bearing file set
  + their classified targets in ONE query.
LiveGraph enumeration: `live_import_view(None)` already returns ALL edges ({src_file, dst_file}) + observations
  ({source_file, class, blocking}) repo-wide -> group by source file = the LiveGraph TS import-bearing file set.
PRECONDITION: `file_partition_status(file)` gives resident/Fresh/TS per file; a BULK file->partition map (built
  once from the resident IRs) avoids O(files x nodes) re-scans (an implementation detail, decide-and-record).
NON-TS: a non-TS repo (OpenXcom) / a mixed repo's non-TS files have NO resident TS partition -> precondition
  UNMET -> FallbackPreconditionUnmet (the language gate; never a silent loss). Confirmed per-file in READINESS-1.
```

## Forced decisions — every cell filled (ratify at sign-off)

### D1 — Repo set
```text
xpart-monorepo (TS fixture; the GREEN baseline -- every file must be NoLoss).
amodx (real TS monorepo; alias / workspace-local / external / asset / dynamic at scale).
repo-graph (MIXED: Rust-primary + any TS -> the non-TS files MUST be FallbackPreconditionUnmet, the TS files
  compared) -- the mixed-language control.
OpenXcom (pure NON-TS C/C++; every file MUST be FallbackPreconditionUnmet -- the pure language-gate control).
RECOMMENDATION: as written. Each repo loads via `livegraph-refresh --all-discovered` (the non-TS repos load NO
  TS partition -> their LiveGraph is empty -> every file falls back, which is the POINT).
```

### D2 — File selection
```text
The compared file set per repo = (SQLite import-bearing source files) UNION (LiveGraph import-bearing source
  files). SQLite import-bearing = a source of >=1 IMPORTS edge (the new bulk query). LiveGraph import-bearing =
  a source_file in `live_import_view(None)` edges OR observations. NO sample cap by default (every file). A cap
  is allowed ONLY if explicitly LABELLED + justified in the report (e.g. a pathological repo) -- a silent cap is
  FORBIDDEN (it would read as full coverage). 
RECOMMENDATION: as written. The UNION (not just SQLite) ensures a LiveGraph file with imports SQLite lacks is
  still counted (it cannot be a regression, but it is coverage). Report the per-repo file_total + the two
  source counts so the union is auditable.
```

### D3 — Metrics (the user's list + the `unknown` definition)
```text
Per repo: files_total ; files_precondition_met ; files_fallback_required (precondition unmet) ; files_regression
  (precondition met AND >=1 SQLite resolved-local target missing from LiveGraph) ; missing_in_livegraph_total
  (sum over files) ; extra_livegraph_edges_total ; blocking_observation_total + by class ; unknown_total.
UNKNOWN (unclassified) = a SQLite IMPORTS row the harness cannot confidently bucket: kind=FILE AND a non-empty
  target AND resolution != 'static' AND subtype != 'EXTERNAL' (a FILE-target import that is neither cleanly
  resolved-local NOR cleanly external/unresolved). unknown>0 means a SQLite import whose treatment is
  ambiguous -> it could HIDE a loss -> blocks GREEN (D4). Each unknown is listed (file + target + resolution).
RECOMMENDATION: as written. The unknown bucket is the safety net against silent misclassification.
```

### D4 — Verdict (the user's rule)
```text
GREEN  iff: files_regression == 0 AND unknown_total == 0 AND every unsupported-language / non-TS file is
  FallbackPreconditionUnmet (NOT a silent loss) AND coverage is the full union (no unlabelled cap).
YELLOW iff: files_regression == 0 AND unknown_total == 0 but coverage is incomplete (a labelled cap) OR the set
  is FALLBACK-HEAVY (a large share precondition-unmet -> LiveGraph serves few real files; safe but low value).
RED    iff: ANY TS file (precondition met) has a SQLite resolved-local import MISSING from LiveGraph
  (files_regression > 0) -- a real loss. OR unknown_total > 0 (ambiguous imports that could hide a loss).
RECOMMENDATION: as written. RED is the hard stop (a measured regression); GREEN requires zero regression + zero
  unknown + honest fallback for every non-TS file.
```

### D5 — Output
```text
A measurement HARNESS + REPORT only. NO CLI default change (the default `imports <file>` stays SQLite, frozen).
The report is JSON (the authoritative aggregate) + a human summary; the exact sidecar/report PATHS are recorded
in the completion (the acceptance requires it). No persisted artifact in the repo DB (a measurement, not state).
RECOMMENDATION: as written.
```

### D6 — The repo-wide harness MECHANISM (surfaced; determines the build)
```text
A. `imports --engine compare` with NO file -> the repo-wide aggregate report (the optional-file pattern,
   mirroring `imports --engine livegraph` no-file=repo-wide). `imports --engine compare <file>` stays the
   per-file response (READINESS-1). A new daemon `imports_readiness_response` (no file) does the bulk enumerate
   + per-file diff + aggregate. [LEAN -- consistent surface; reuses the per-file directional logic; one route.]
B. A separate command (e.g. `imports-readiness`). Clean name but a new top-level surface for a measurement.
C. A measurement SCRIPT calling `imports --engine compare <file>` per file (N daemon round-trips). No daemon
   code but O(N) round-trips + throwaway; the in-process bulk route is faster + reusable.
RECOMMENDATION: A. The no-file repo-wide mode. BUILD: (1) a storage bulk-import query; (2) the daemon route
   (bulk SQLite + `live_import_view(None)` grouped by source + a bulk file->partition map + per-file diff +
   aggregate); (3) the CLI no-file branch + an aggregate renderer. Reuses the directional verdict from
   READINESS-1 (no new compare logic). NO default flip.
```

## Measurement protocol (PROPOSED — gated on ratification)
```text
Per D1 repo: `livegraph-refresh --all-discovered` THEN `imports --engine compare --json` (no file) -> the
aggregate report. Capture the D3 metrics + the per-file regression list (must be empty) + the unknown list
(must be empty for GREEN) + the fallback share. Record the report path. Aggregate the per-repo verdicts ->
the slice verdict (GREEN / YELLOW / RED).
```

## Acceptance (to verify post-build, EXECUTED)
```text
1. xpart: ALL files GREEN (NoLossEquivalent/Superset; zero regression, zero fallback -- a pure TS fixture).
2. amodx: repo-wide GREEN-safe OR YELLOW with ZERO regressions (the alias/dynamic files are Superset; the
   workspace-local files report blocking observations but lose no SQLite import).
3. repo-graph (mixed) + OpenXcom (non-TS): every non-TS file is FallbackPreconditionUnmet (the language gate),
   NEVER a silent loss / Regression.
4. unknown_total == 0 across the set (or every unknown listed + explained).
5. The exact sidecar/report paths recorded. Default `imports <file>` unchanged.
Gate: cargo test --workspace ; clippy --workspace --all-targets -- -D warnings ; cargo fmt --all -- --check.
```

## Out of scope (hard guardrails)
```text
NO default flip (this MEASURES; the flip is IMPORTS-LIVEGRAPH-DEFAULT-1) ; NO decommission ; NO SQLite deletion ;
NO resolver changes ; NO CLI default change ; NO new compare LOGIC (reuse READINESS-1's directional verdict) ;
NO silent sample cap.
```

## Build contract (PROPOSED — gated on D1–D6 ratification)
```text
1. storage: a read-only bulk query -- all IMPORTS edges for a snapshot as (source_file, target_file, kind,
   subtype, resolution). Unit-tested.
2. livegraph (if needed): a bulk file->partition-status map (or reuse file_partition_status per file). Group
   `live_import_view(None)` by source file.
3. daemon: `imports_readiness_response(repo_state, repo_uid, snapshot_uid)` -- bulk SQLite + bulk LiveGraph +
   per-file directional diff (reuse) + aggregate metrics + the D4 verdict. The `imports --engine compare`
   no-file route calls it.
4. cli: `imports --engine compare` (no file) -> the aggregate report (JSON + human summary).
5. MEASURE across D1 ; record the report paths + the verdict.
6. live + gate + completion doc.
Stop if: ANY repo shows files_regression > 0 (a real TS loss) -> RED, surface before any flip discussion. Stop
if unknown_total > 0 (ambiguous imports) -> classify before a verdict.
```

## After this slice
```text
If GREEN / YELLOW-safe (zero regression): IMPORTS-LIVEGRAPH-DEFAULT-1 becomes considerable -- the default
`imports <file>` serves LiveGraph WHEN the precondition is met AND the per-file gate passes, ELSE a LABELLED
SQLite fallback (precondition-unmet OR a regression). That flip is a SEPARATE ratified slice.
```

## Completion (IMPLEMENTED + MEASURED 2026-06-07, EXECUTED) — verdict GREEN-SAFE

Commits: `40df5e4` (spec) -> `94164d5` (impl: storage `all_imports` + livegraph `resident_file_statuses` +
daemon `directional_status`/`aggregate_readiness`/`imports_readiness_response` + the `compare` no-file route +
the CLI `ImportsReadinessReport`). UNPUSHED. Report artifacts: `docs/audits/imports-repowide-readiness-1/{xpart,amodx,repo-graph,openxcom}.json`
(the full per-repo aggregates -- LOCAL, the `docs/audits/` dir is gitignored; the **Measurement** metrics table
below is the committed record, and regressions/unknowns are EMPTY so the JSON adds only the per-file detail).
Reproduce: `rmap imports --engine compare --json` from each repo (after `livegraph-refresh --all-discovered`).

### Gate (EXECUTED 2026-06-07)
```text
cargo test --workspace -> no failures. clippy --workspace --all-targets -- -D warnings -> clean. fmt --check ->
clean. Unit tests: storage all_imports (path JOINs + IMPORTS-only); livegraph resident_file_statuses;
aggregate_readiness (GREEN / RED-regression / YELLOW-fallback / RED-unknown); the CLI report renderer.
```

### Measurement (EXECUTED 2026-06-07) — `rmap imports --engine compare` (no file), per the D1 set
```text
REPO        VERDICT  files_total  precond_met  fallback  REGRESSION  unknown  missing  extra  fallback_share
xpart       GREEN    2            2            0         0           0        0        0      0%
amodx       GREEN    377          371          6         0           0        0        414    1.6%
repo-graph  GREEN    324          165          159       0           0        0        7      49.1% (mixed: Rust falls back)
OpenXcom    YELLOW   600          0            600       0           0        0        0      100% (pure non-TS; serves none)
TOTAL       --       1303         538          765       0           0        0        421    --

amodx: blocking_observation_by_class = {WorkspaceLocalUnedgeable: 69}. All four: NOT RUN = none (all available).

THREE decisive facts:
1. files_regression == 0 AND unknown_total == 0 across ALL FOUR repos (1303 files). The directional no-loss gate
   loses NO SQLite resolved-local import for ANY TS file, and no SQLite import is ambiguous.
2. Every NON-TS file falls back by PRECONDITION, never a silent loss: repo-graph's 159 Rust files + OpenXcom's
   600 C++ files are FallbackPreconditionUnmet (precond_met counts only the TS files), NEVER Regression.
3. LiveGraph is a SUPERSET for the served TS files: 421 extra edges total (amodx 414, repo-graph 7) -- imports
   the homegrown SQLite extractor missed; for amodx LiveGraph even covers MORE files (371) than SQLite (205).
```

### Verdict — GREEN-SAFE (the slice); OpenXcom YELLOW is the expected pure-non-TS control
```text
GREEN-SAFE: the per-file directional no-loss gate is PROVEN SAFE at repo scale -- zero regression + zero unknown
  over 1303 files spanning a TS fixture (xpart), a real TS monorepo (amodx), a MIXED Rust+TS repo (repo-graph),
  and a pure C++ repo (OpenXcom). Per-repo: xpart/amodx/repo-graph GREEN; OpenXcom YELLOW (100% fallback, serves
  no file -- SAFE, the language gate, the expected pure-non-TS outcome; reported explicitly, not hidden).
NOTE: repo-graph fallback_share = 49.1% (just under the 50% fallback-heavy threshold) -> GREEN; the 165 TS files
  (default + tools/rgistr) are served with no loss, the 159 Rust files fall back. This is the honest mixed-repo
  picture (fallback recorded explicitly).
=> the predicate IMPORTS-LIVEGRAPH-DEFAULT-1 needs (precondition met AND no-loss, else labelled SQLite fallback)
  is BUILDABLE + coverage-backed safe. The flip is a SEPARATE ratified slice.
```

### Divergences / notes (recorded)
```text
- fallback-heavy threshold = 50% of files (documented; the share is ALWAYS reported so a fallback-heavy repo is
  never hidden -- D5). A repo just under (repo-graph 49.1%) is GREEN; OpenXcom (100%) is YELLOW.
- The bulk SQLite source path uses sn.file_uid -> files.path (FILE nodes carry file_uid); rows with no source
  file are dropped (defensive). Validated live: amodx 205 / repo-graph 297 / OpenXcom 600 SQLite import-bearing
  files enumerated (non-empty -> the JOIN resolves).
- imports_readiness_response (RepoState-bound) is not RepoState-unit-tested (no harness; siblings likewise) --
  MITIGATED: the PURE aggregate_readiness + the bulk query are unit-tested, and the end-to-end is live-measured.
- LIVE-VALIDATION DAEMON STATE: the manual release rmapd was restarted (pkill -x rmapd, exact; new binary +
  producer env) and xpart/amodx/repo-graph/OpenXcom refreshed. Reported before acting. Re-run
  ./scripts/dev-install-local.sh to restore the launchd-managed daemon.
```

## References
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (`imports_compare_sidecar` / `imports_compare_response` — the per-file gate this generalizes)
- `rust/crates/repo-graph-livegraph/src/lib.rs` (`live_import_view` / `file_partition_status` — the LiveGraph enumeration + precondition)
- `rust/crates/storage/src/queries.rs:1256` (`find_imports` — the per-file SQLite path; the bulk query is new)
- `docs/slices/imports-livegraph-default-readiness-1.md` (the per-file readiness this broadens)
