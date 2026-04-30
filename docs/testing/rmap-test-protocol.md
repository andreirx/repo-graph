# rmap Test Protocol

Canonical protocol for smoke tests, validation runs, and verification passes.
Applies to all agent-driven and manual CLI testing of `rmap`.

## DB Location Rules

### Forbidden locations

- No `.db` files in repo root
- No `.db` files in `rust/`
- No ad hoc filenames anywhere in the repo tree

### Canonical scratch root

All test databases live under:

```
/private/tmp/repo-graph-tests/
```

### Naming conventions

Ephemeral smoke DBs (default):
```
/private/tmp/repo-graph-tests/<task>/<repo>.db
```

Retained debug DBs (explicit only):
```
/private/tmp/repo-graph-tests/_retained/<date>-<task>-<repo>.db
```

Examples:
```
/private/tmp/repo-graph-tests/slice-12-modules/repo-graph.db
/private/tmp/repo-graph-tests/pf-2-swupdate/swupdate.db
/private/tmp/repo-graph-tests/modules-repo-graph/repo-graph.db
/private/tmp/repo-graph-tests/_retained/2026-04-30-deadcode-debug-repo-graph.db
```

Task naming patterns:
- `slice-<N>-<feature>` — slice verification
- `pf-<N>-<repo>` — production fix verification
- `<family>-<repo>` — command family smoke test
- `quality-<repo>` — quality/policy verification

## Lifecycle Rules

### Default behavior

1. Create DB directory: `mkdir -p /private/tmp/repo-graph-tests/<task>/`
2. Index or refresh into that directory
3. Run verification commands
4. Delete directory after successful verification

### Retention

Only retain DBs when:
- Diagnosis of a failure requires post-mortem inspection
- Explicit user request for retained artifact

Retained DBs must:
- Use the `_retained/` subdirectory
- Include date in filename
- Be explicitly named in the verification report

## Command Templates

### Setup

```bash
# Create task directory
mkdir -p /private/tmp/repo-graph-tests/<task>/
```

### Index (new DB)

```bash
cargo run --manifest-path "/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/Cargo.toml" \
  -p repo-graph-rgr -- \
  index "/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph" \
  /private/tmp/repo-graph-tests/<task>/repo-graph.db
```

### Refresh (existing DB)

```bash
cargo run --manifest-path "/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/Cargo.toml" \
  -p repo-graph-rgr -- \
  refresh "/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph" \
  /private/tmp/repo-graph-tests/<task>/repo-graph.db
```

### Smoke command

```bash
cargo run --manifest-path "/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/Cargo.toml" \
  -p repo-graph-rgr -- \
  <command> /private/tmp/repo-graph-tests/<task>/repo-graph.db repo-graph [args...]
```

### Unit/integration tests

```bash
cd "/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust"
cargo test -p <crate> -- <filter>
```

### Cleanup

```bash
rm -rf /private/tmp/repo-graph-tests/<task>/
```

## Verification Report Contract

Every verification report must include:

1. **DB path used** — exact path, no guessing
2. **Creation status** — newly created or reused
3. **Disposal status** — deleted or retained
4. **Retention reason** — if retained, why

Example report block:

```
Verification DB: /private/tmp/repo-graph-tests/slice-12-modules/repo-graph.db
Status: newly created
Disposal: deleted after successful verification
```

Or for retained:

```
Verification DB: /private/tmp/repo-graph-tests/_retained/2026-04-30-modules-debug-repo-graph.db
Status: newly created
Disposal: RETAINED for post-mortem (modules deps returned empty)
```

## Forbidden Patterns

Agents must never:

- Guess DB locations with `ls *.db` or `find`
- Use paths not declared before running
- Leave DBs in repo root or `rust/`
- Omit DB path from verification reports
- Retain without explicit reason

## Validation Repos

Standard validation targets (paths relative to repo-graph parent):

| Repo | Path | DB name |
|------|------|---------|
| repo-graph | `.` | `repo-graph.db` |
| amodx | `../amodx` | `amodx.db` |
| glamCRM | `../glamCRM` | `glamcrm.db` |
| hexmanos | `../hexmanos` | `hexmanos.db` |
| spring-petclinic | `../legacy-codebases/spring-petclinic` | `spring-petclinic.db` |
| swupdate | external | `swupdate.db` |

Full path example for swupdate validation:
```
/private/tmp/repo-graph-tests/pf-2-swupdate/swupdate.db
```

## Run Logging

All verification runs must be logged to the `smoke-runs/` folder at repo root.

### Structure

```
smoke-runs/
  <ISO-8601 timestamp>/
    00-meta.json              run identity + context
    <command outputs...>      captured JSON per command
    92-tool-latency.json      per-command timing
```

### Logging requirements

1. **Create timestamped directory** at run start:
   ```bash
   RUN_DIR="smoke-runs/$(date -u +%Y-%m-%dT%H-%M-%SZ)"
   mkdir -p "$RUN_DIR"
   ```

2. **Capture 00-meta.json** with:
   - `task` — slice/task identifier
   - `db_path` — exact DB path used
   - `repo_uid` — target repo
   - `baseline_shape_version` — current: 3
   - `timestamp` — ISO-8601 UTC

3. **Capture command outputs** as JSON files per command run.

4. **Record timings** in `92-tool-latency.json`.

### When to log

- All slice verification runs
- All production fix validation runs
- Smoke tests on validation repos

Unit tests (cargo test) do not require smoke-runs logging — they use
ephemeral in-memory fixtures.

### Reference

See `smoke-runs/README.md` for baseline shape version history and
capture set documentation.
