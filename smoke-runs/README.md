# smoke-runs/

Field-calibration logs from running `rgr` against real repositories.

This folder is gitignored. Runs are stored locally and persist across sessions
as a personal baseline for cross-run comparison.

## Purpose

Each run captures a point-in-time JSON snapshot of every rgr read surface
against each target repo. These outputs serve three purposes:

1. **Defect discovery.** Comparing outputs across runs surfaces regressions
   in extraction, traversal, or measurement.
2. **Trust calibration.** Before extraction trust reporting exists as a
   first-class surface, raw outputs let a human calibrate confidence per
   codebase shape.
3. **Benchmarking baseline.** Future token-consumption benchmarks (see
   VISION.md Phase 8) will need both agent-discovery and rgr-assisted
   baselines anchored to a specific repo commit and toolchain version.

## Structure

```
smoke-runs/
  <ISO-8601 timestamp>/
    00-meta.json                   run identity + basis_commit per repo
    00-add-<repo>.txt              repo registration log
    01-<repo>-repo-index.txt       indexing summary
    02-<repo>-repo-status.json     snapshot + toolchain provenance
    03-<repo>-graph-stats.json     module structural metrics
    04-<repo>-graph-metrics-top50.json
    05-<repo>-graph-metrics-module.json
    06-<repo>-graph-cycles.json
    07-<repo>-graph-dead.json
    08-<repo>-graph-versions.json
    09-<repo>-graph-churn-365d.json
    10-<repo>-graph-hotspots.json
    11-<repo>-graph-risk.json
    12-<repo>-change-impact-against-snapshot.json
    13-<repo>-evidence.json
    14-<repo>-gate.json
```

## Cross-run comparison

Two runs are **structurally comparable** iff their `00-meta.json` files
show the same `rgr_version` AND the same `toolchain` per repo.

Two runs are **content-comparable** iff also `basis_commit` matches per
repo. If the basis_commit differs but toolchain matches, diffs reflect
code changes, not tool changes.

Fast diff workflow:

```bash
# Compare two runs, single file
diff smoke-runs/<run-A>/03-amodx-graph-stats.json \
     smoke-runs/<run-B>/03-amodx-graph-stats.json

# All files for one repo
diff -r smoke-runs/<run-A> smoke-runs/<run-B> | grep amodx
```

## Reproduction

Runs are produced by a manual sequence (see the commit message that
introduces this folder). Future work may add a `scripts/smoke-run.sh`
harness to automate this. Until then, the canonical sequence is:

1. Build rgr (`pnpm run build`) and rebuild sqlite
   (`pnpm rebuild better-sqlite3`) if needed.
2. Create a temp DB and set `RGR_DB_PATH`.
3. Register each target repo via `rgr repo add`.
4. Index each target repo.
5. Invoke each read surface with `--json`, redirecting to the
   matching numbered file in the timestamped subfolder.
6. Write `00-meta.json` with the run identity block.

No declarations (requirements, obligations, waivers, boundaries) are
created during a smoke run. These runs capture the BASELINE state of
each read surface before any governance overlay. To test gate/evidence
with active governance, set up declarations in a separate DB or use
the test fixtures.
