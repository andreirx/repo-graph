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

## Script Wrappers (MANDATORY)

**All smoke validation and verification runs must use the provided smoke scripts
unless the script is incapable of expressing the run.** Manual command execution
is exception-only and must reproduce the same `smoke-runs/` artifacts.

The scripts enforce correct manifest paths, package names, DB locations, and
`smoke-runs/` logging. Bypassing them forfeits the audit trail.

### Unit/Integration Tests

```bash
# All tests for repo-graph-rgr
./scripts/test-rgr-crate.sh

# Specific test file
./scripts/test-rgr-crate.sh --test modules_list_command

# Filter within tests
./scripts/test-rgr-crate.sh -- envelope_contract
```

### Integration Tests (by name)

```bash
# Run specific integration test files
./scripts/test-rgr-integration.sh modules_list_command
./scripts/test-rgr-integration.sh envelope_contract check_command
```

### Single Repo Smoke

```bash
# Smoke a single command on a repo
./scripts/smoke-rmap.sh <task> <repo-path> <command> [args...]

# Examples
./scripts/smoke-rmap.sh slice-12 . trust
./scripts/smoke-rmap.sh pf-2 ../legacy-codebases/swupdate policy --kind BEHAVIORAL_MARKER
./scripts/smoke-rmap.sh modules-test . modules list
```

### Validation Repos Smoke

```bash
# Run on all validation repos with default commands (trust, modules, check)
./scripts/smoke-validation-repos.sh <task>

# Run specific commands on all validation repos
./scripts/smoke-validation-repos.sh <task> trust orient check

# Examples
./scripts/smoke-validation-repos.sh slice-12
./scripts/smoke-validation-repos.sh quality-gate trust check orient
```

## Manual Command Templates (FALLBACK ONLY)

**Manual commands are fallback only.** Use only when the smoke scripts cannot
express the required run.

**CRITICAL:** If the smoke script fails or cannot express the run, **fix the
script first**. The script is shared infrastructure — a broken script affects
all future validations. Do not work around script failures by manually creating
artifacts.

If manual execution is truly necessary (e.g., the script cannot be fixed in
the current context), the run does not count as slice validation. Document the
limitation and defer the validation until the script is fixed.

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
- **Report smoke validation as complete if the run did not produce `smoke-runs/` artifacts**
- **Promote slice maturity (IMPLEMENTED → SHIPPED) without citing a `smoke-runs/<timestamp>/` path**
- **Bypass smoke scripts for ad hoc shell execution when scripts can express the run**
- **Manually create `00-meta.json` or `92-tool-latency.json` after command execution**
- **Work around script failures instead of fixing the script**
- **Use artifacts that lack the `generator` provenance field**
- **Edit script-generated artifacts to "fix" validation failures**

## Validation Repos

Hybrid inventory model: internal repos are explicitly listed, legacy repos
are discovered dynamically from a bucket directory.

### Internal repos (explicit)

| Repo | Path | DB name |
|------|------|---------|
| repo-graph | `.` | `repo-graph.db` |
| amodx | `../amodx` | `amodx.db` |
| glamCRM | `../glamCRM` | `glamCRM.db` |
| hexmanos | `../hexmanos` | `hexmanos.db` |
| zap-engine | `../zap-engine` | `zap-engine.db` |

### Legacy repos (discovered)

Bucket path: `../legacy-codebases/`

Discovery rules:
- directories only
- hidden entries skipped (`.git`, `.cache`, etc.)
- sorted lexicographically

To add a legacy repo: drop it into `../legacy-codebases/`. No script edit needed.

Current typical contents: `spring-petclinic`, `swupdate`, `sqlite`, `nginx`, `linux`, etc.

Full path example for swupdate validation:
```
/private/tmp/repo-graph-tests/pf-2-swupdate/swupdate.db
```

### Running validation smoke

Use the `smoke-validation-repos.sh` script:

```bash
# Default commands (trust, modules, check) on all repos (internal + discovered legacy)
./scripts/smoke-validation-repos.sh slice-12

# Specific commands
./scripts/smoke-validation-repos.sh quality-gate trust orient check
```

The script indexes each repo if needed, runs the specified commands,
and reports pass/fail summary.

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

### Maturity Promotion Requirement

Slice promotion from IMPLEMENTED to SHIPPED requires citing a `smoke-runs/<timestamp>/`
path in:
- The slice doc's "Smoke Validation" section, OR
- The ROADMAP status note

Chat summaries or ad hoc command output alone cannot satisfy the maturity bar.
The `smoke-runs/` artifact is the audit hook.

### Reference

See `smoke-runs/README.md` for baseline shape version history and
capture set documentation.

## Artifact Integrity Requirements

### Script-Generated Provenance (MANDATORY)

All `smoke-runs/` artifacts used for slice promotion MUST be generated by the
smoke scripts (`smoke-rmap.sh`, `smoke-validation-repos.sh`). Hand-crafted
artifacts do not satisfy the protocol.

The `00-meta.json` file MUST contain these provenance fields:

| Field | Description |
|-------|-------------|
| `generator` | Script name (e.g., `smoke-rmap.sh`) |
| `generator_version` | Script version number |
| `command_argv` | Exact command-line array as executed |
| `started_at` | ISO-8601 UTC timestamp when run started |
| `finished_at` | ISO-8601 UTC timestamp when run completed |
| `status` | `success`, `no_results`, or `error` |

If any of these fields are missing, the artifact is invalid.

### Anti-Backfill Rules

**FORBIDDEN:**

1. **Do not manually create `00-meta.json` after command execution.**
   If the script did not emit it, the run is invalid.

2. **Do not manually create `92-tool-latency.json` after command execution.**
   Timing data must be captured by the script during execution.

3. **Do not edit script-generated artifacts to "fix" them.**
   If the artifact is wrong, the script is wrong. Fix the script and re-run.

4. **Do not copy command output into `smoke-runs/` after manual execution.**
   The artifact must be generated in the same execution context as the script.

### Invalid Run Handling

If a smoke script fails to emit required artifacts:

1. **Do not work around.** The script failure is a blocking issue.
2. **Fix the script.** Script infrastructure takes priority over slice completion.
3. **Re-run after fix.** Only script-generated artifacts count.
4. **Delete invalid run directories.** Partial artifacts pollute the audit trail.

A run is invalid if:
- `00-meta.json` is missing
- `00-meta.json` lacks `generator` field
- Command output JSON is missing
- `92-tool-latency.json` is missing

### Script Self-Validation

The `smoke-rmap.sh` script (v2+) validates its own output before exit:

- Checks all required files exist
- Verifies `generator` field is present
- Exits nonzero (code 3) if validation fails

If the script exits 0, the artifacts are guaranteed complete. If it exits 3,
artifact validation failed — diagnose and fix the script.

### Promotion Checklist

Before marking a slice SHIPPED:

1. Smoke script ran to completion (exit 0 or 1, not 2 or 3)
2. `smoke-runs/<timestamp>/00-meta.json` exists
3. `00-meta.json` contains `generator` field
4. `00-meta.json` contains `command_argv` field with correct command order
5. Command output JSON exists
6. `92-tool-latency.json` exists
7. Slice doc or ROADMAP cites the `smoke-runs/<timestamp>/` path

If any item fails, the slice is not SHIPPED.
