# Validation Protocol

## Evidence Labels

Every validation claim must be labeled:

| Label | Meaning |
|-------|---------|
| `EXECUTED` | Command was run, output observed directly |
| `OBSERVED` | Artifact inspected directly |
| `INFERRED` | Concluded from context, not directly verified |
| `NOT RUN` | Skipped, with reason stated |

**Never present inferred results as executed evidence.**
**Never reconstruct expected logs from memory.**
**If a command timed out or failed, report that fact directly.**

## Validation Report Format

For every validation step, report:

```
Command: <exact command>
Working directory: <path>
Database path: <path>
Exit code: <code>
Output (excerpt):
<output>
Artifact path: <if applicable>
Label: EXECUTED | OBSERVED | INFERRED | NOT RUN
Interpretation: <what this means>
```

## Tool Hierarchy

1. **Use `rmap` first** for system orientation and user-facing validation.
2. **Raw SQL only after CLI inspection**, and only for storage diagnostics.
3. **Never validate user-facing behavior exclusively through SQL.**

If SQL is used before CLI, explain why CLI was insufficient.

## Standard Validation Sequence

```bash
# 1. Fresh index
rmap index ./repo-graph ./test-artifacts/repo-graph.db

# 2. Trust check
rmap trust ./test-artifacts/repo-graph.db repo-graph

# 3. System check
rmap check ./test-artifacts/repo-graph.db repo-graph

# 4. If testing refresh
rmap refresh ./repo-graph ./test-artifacts/repo-graph.db
# Then re-run trust and check
```

## Database Paths

Use `./test-artifacts/` for validation databases.

Do not create databases:
- in repo root
- in temp directories without explicit cleanup
- in nested crate folders
- in random locations

See `docs/testing/rmap-test-protocol.md` for canonical locations.

## Validation Repos

**Internal:**
- `repo-graph`
- `../amodx`
- `../glamCRM`
- `../hexmanos`

**External:**
- `../legacy-codebases/spring-petclinic`
- `sqlite`
- `nginx`
- `swupdate`
- `linux`

## Build and Test Commands

```bash
# Build
cd rust && cargo build

# Test specific crate
cargo test -p <crate>

# Test all
cargo test --workspace
```

## Conventions

- `rmap` outputs JSON only
- Relative paths in DB, never absolute
- TEXT UIDs everywhere, no auto-increment integers

## Stop Conditions

Stop validation and report if:

- Command times out
- Unexpected error
- Output contradicts expected state
- Artifact missing
- Trust downgrade unexplained
