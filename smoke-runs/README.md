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
3. **Tool-latency baseline.** `92-tool-latency.json` captures wall-clock
   per command. This is INFRASTRUCTURE latency — one input to Phase 8
   token-consumption benchmarking, NOT the full Phase 8 baseline.
   Phase 8 additionally requires task definitions, prompt/profile
   versions, input/output token counts, tool-call counts, correctness
   against expected artifacts, and human verification burden. See
   VISION.md Phase 8 for the full benchmark spec.

## Baseline shape versions

Runs carry a `baseline_shape_version` field in `00-meta.json`. When the
capture set changes materially (files added, dropped, or reshaped),
the version bumps. Cross-run comparison at the same shape version is
straightforward; comparison across shape versions must account for
the shape delta.

### Shape version 1 (initial)

First baseline capture. Included raw `07-<repo>-graph-dead.json` full
dumps (~812KB on AMODX), redundant `02-<repo>-repo-status.json`
(overlapping with `00-meta.json`), vacuous `13-evidence.json` /
`14-gate.json` when no declarations exist, and empty
`12-change-impact-*.json` on clean working trees. Preserved as
evidence; see the first run folder for details.

### Shape version 2 (current)

Trimmed capture set. Key changes:
- Dropped `00-add-*.txt`, `02-*-repo-status.json`, `12-*`, `13-*`, `14-*`
  (redundant, vacuous, or low-signal).
- `07-*-graph-dead-summary.json` replaces the full dead-code dump.
  Contains `counts_by_kind`, `total`, and `top_n_by_kind` (up to 50
  per kind). Rationale: on registry-heavy codebases the full dump is
  ~500KB of mostly false positives (see trust report for context).
- Added `15-<repo>-trust.json` — full trust report with reliability
  axes, downgrade triggers, per-module trust rows, and caveats.
- Added `92-timings.json` — per-command wall-clock (ms) + exit codes.
  Baseline for VISION.md Phase 8 token-consumption benchmarking.

```
smoke-runs/
  <ISO-8601 timestamp>/
    00-meta.json                              run identity + toolchain + baseline_shape_version
    01-<repo>-repo-index.txt                  indexing summary + unresolved breakdown
    03-<repo>-graph-stats.json                module structural metrics
    04-<repo>-graph-metrics-top50.json        top complex functions
    05-<repo>-graph-metrics-module.json       per-module complexity aggregates
    06-<repo>-graph-cycles.json               dependency cycles
    07-<repo>-graph-dead-summary.json         dead-code counts + top-50 samples by kind
    08-<repo>-graph-versions.json             package manifests
    09-<repo>-graph-churn-365d.json           per-file git churn
    10-<repo>-graph-hotspots.json             churn x complexity ranking
    11-<repo>-graph-risk.json                 under-tested hotspots
    15-<repo>-trust.json                      extraction trust report
    92-timings.json                           per-command wall-clock + exit codes
```

Churn/hotspots/risk caveat (addressed in shape v3): these commands run
`git log` against the CURRENT working-tree HEAD, not the snapshot's
`basis_commit`. In shape v2 they drift with wall-clock git history
across runs even at a fixed snapshot. Shape v3 freezes the churn
window to an absolute ISO timestamp.

### Shape version 3 (next, contract-only updates)

Applies to all future runs. Supersedes v2 mechanically, with a
larger comparability discipline.

File-level changes:
- `92-timings.json` → `92-tool-latency.json` (renamed for scope clarity)
- NEW: `93-commands.json` — manifest of exact CLI invocations used to
  produce each captured file. Full reproducibility without relying
  on commit messages or external notes.

Capture discipline changes:
- Churn window frozen to absolute ISO timestamp instead of relative
  `--since 365.days.ago`. At run start, compute `NOW - 365 days`
  as an ISO string, pass to `--since`, and record as
  `churn_window.since` in `00-meta.json`. This removes wall-clock
  drift from hotspots/risk/churn.
- `00-meta.json` adds:
  - `baseline_shape_version: 3`
  - `churn_window: { since: "<ISO>", mode: "absolute_iso" }`
  - `normalization: { volatile_fields: [...] }` — declarative list
    of fields that must be normalized before mechanical diffing

## Normalization contract (cross-cutting, applies to all shape versions)

Mechanical cross-run diffing requires normalizing identity/volatility
fields first. These fields carry run identity, not regression signal,
and will differ between any two runs even with identical inputs.

Normalization is **per-field, per-file**. Blind textual replacement is
forbidden because: (a) cross-repo baselines may contain multiple
`repo_uid` strings where one must stay and another must normalize;
(b) some content strings may coincidentally contain identity values;
(c) the repo_uid inside a `stable_key` carries structural meaning
that should be marked, not erased.

**Volatile fields (always normalize before diffing):**

- `snapshot_uid` / `snapshot` — run identity per snapshot
- `generated_at` / `extracted_at` / `created_at` — run timestamps
- `run_id` — directory timestamp
- `repo_uid` prefix inside `stable_key` values — replace with the
  fixed placeholder `<repo_uid>` while PRESERVING the rest of the
  key structure (e.g. `<repo_uid>:src/core:MODULE`, not `:src/core:MODULE`)

**Non-volatile fields (NEVER normalize in same-code comparisons):**

- `basis_commit` — content anchor, not run identity. Two runs with
  different `basis_commit` are legitimately different content; diffs
  should surface that.
- `toolchain` — provenance signal. Differences between runs are
  meaningful (they indicate tool drift).
- `diagnostics_version` — provenance signal.
- All measurement values, counts, rates, reliability levels.

`basis_commit` and `toolchain` should ONLY be normalized when
explicitly comparing SHAPE across different toolchains / commits —
a different comparison class from same-code regression.

### Per-file normalization map

| File pattern | Normalize | Preserve | Sorted? |
|--------------|-----------|----------|---------|
| `00-meta.json` | `snapshot_uid`, `generated_at`, `run_id` | `toolchain`, `basis_commit`, `files`/`nodes`/`edges`, `diagnostics_version`, `churn_window` (v3+) | - |
| `01-*-repo-index.txt` | `snapshot_uid` line, `duration_ms` | `files`, `nodes`, `edges`, `unresolved`, breakdown counts | - |
| `03-*-graph-stats.json` | `snapshot`, repo_uid in module keys | `module`, `fan_in`, `fan_out`, `instability`, `abstractness`, `distance_from_main_sequence`, `file_count`, `symbol_count` | alphabetical by `module` |
| `04-*-graph-metrics-top50.json` | `snapshot` | `symbol`, `file`, `line`, `cyclomatic_complexity`, `parameter_count`, `max_nesting_depth` | CC descending |
| `05-*-graph-metrics-module.json` | `snapshot`, repo_uid in module keys | `module`, `function_count`, avg/max CC, avg/max nesting | max CC descending (documented default) |
| `06-*-graph-cycles.json` | `snapshot`, `node_id` (UUIDs) | cycle count, node `name` values | cycle_id ascending (stable) |
| `07-*-graph-dead-summary.json` | `snapshot` | `counts_by_kind`, `total`, `top_n_by_kind` entries | kind alphabetical within `top_n_by_kind` |
| `08-*-graph-versions.json` | `snapshot` | `kind`, `value`, `source_file` | alphabetical by kind |
| `09-*-graph-churn-365d.json` | `snapshot` | `file`, `commit_count`, `lines_changed` | `lines_changed` descending |
| `10-*-graph-hotspots.json` | `snapshot` | `file`, `normalized_score`, `raw_score`, `churn_lines`, `change_frequency`, `sum_complexity` | `normalized_score` descending |
| `11-*-graph-risk.json` | `snapshot` | `file`, `risk_score`, `hotspot_score`, `line_coverage`, `churn_lines`, `sum_complexity` | `risk_score` descending |
| `15-*-trust.json` | `snapshot_uid`, repo_uid in module keys + ancestor reason strings | `basis_commit`, `toolchain`, `diagnostics_version`, `reliability`, `triggered_downgrades`, `categories`, `modules`, `caveats` | categories: unresolved desc; modules: by path |
| `92-tool-latency.json` (v3+) | every `ms` value, `run_id` | `label`, `exit` codes, count of entries | run order (not sorted) |
| `93-commands.json` (v3+) | — (stable across runs) | all fields | file manifest order |

### Ordering contract

rgr output surfaces are expected to emit results in DETERMINISTIC
order (alphabetical, score-descending, etc.). The per-file
normalization map names the expected sort key per file.

Ordering drift is itself regression signal. A cross-run diff that
shows the same rows in different order should be investigated — the
query's sort stability has broken, not just the data.

The only exception is `92-tool-latency.json`: entries are in run
execution order, which is intentional.

### Normalization helper (per-field jq, not blind gsub)

```bash
# Example: normalize 03-graph-stats.json
REPO_UID=$(jq -r '.repos.amodx.snapshot_uid | split(":")[0]' run/00-meta.json)
jq --arg uid "$REPO_UID" '
  .snapshot = "<snapshot_uid>"
  | .results |= map(
      .module |= sub("^" + $uid + ":"; "<repo_uid>:")
    )
' run/03-amodx-graph-stats.json
```

This keeps the structural shape of every stable_key visible while
replacing only the per-run UUID prefix. Blind `walk` + `gsub` at
the document level is forbidden per the per-field contract.

Agents comparing runs programmatically should apply the same
normalization rule set. A future `scripts/smoke-normalize.sh`
harness would encode the full map.

## Cross-run comparison

### Formal comparability levels

The five levels below form a hierarchy. Each stronger level REQUIRES
all weaker levels below it. A cross-run comparison should declare
explicitly which level it is operating at, because the conclusions
that can be drawn differ.

**1. shape-comparable**
DEFINITION: `baseline_shape_version` in `00-meta.json` matches between
runs. Capture set is identical.
WITHOUT: capture-set delta must be reconciled first.

**2. toolchain-comparable**
DEFINITION: shape-comparable AND `rgr_version` matches AND
`toolchain` (extraction_semantics, stable_key_format,
extractor_versions, indexer_version, measurement_semantics) matches
per repo.
WITHOUT: tool drift pollutes every diff. Any content difference
might be tool behavior, not code behavior.

**3. content-comparable**
DEFINITION: toolchain-comparable AND `basis_commit` matches per repo.
WITHOUT: code delta pollutes every diff. Any difference might be
code drift, not tool drift.

**4. churn-comparable** (v3+)
DEFINITION: content-comparable AND `churn_window.since` matches per
run (absolute ISO timestamp, not relative `--since 365.days.ago`).
Only required for comparing churn/hotspots/risk outputs.
WITHOUT: those three files will drift across runs even at identical
basis_commit, because the churn window advances with wall-clock time.

**5. mechanically diff-comparable**
DEFINITION: content-comparable AND churn-comparable (if comparing
churn-dependent files) AND both runs normalized per the
per-file normalization contract.
WITHOUT: cross-run `diff` output is noise. Volatile identity fields
(snapshot_uid, repo_uid prefixes, generated_at) differ on every run
regardless of content, and unsorted or reordered outputs produce
false positives.

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
