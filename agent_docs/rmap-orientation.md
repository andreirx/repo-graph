# rmap Orientation

## Primary Principle

Use `rmap` to understand the system before changing code.
Use `rmap` to validate changes after editing.

Do not use raw SQL as a shortcut.

## Command Pattern

The daemon owns repo state. Read/query commands resolve the repo from the current
working directory — there is no `<db_path>` / `<repo_uid>` to supply (REG-1):

```bash
rmap <command> [options]
```

Run `rmap` from inside the repo you indexed (`rmap index .`). A few legacy
write/governance commands still take explicit `<db_path> <repo_uid>` positionals;
those are called out where they appear below.

## Orientation Commands

### Orient
```bash
rmap orient --focus "src/core"
```
Get a high-level view of the codebase or a specific area.

### Trust
```bash
rmap trust
```
Check what the system knows vs. what it doesn't. Reports:
- unresolved edge classification counts
- call-graph reliability tier
- downgrade triggers and caveats
- enrichment coverage

### Check
```bash
rmap check
```
Structural and quality check after changes.

### Explain
```bash
rmap explain "src/core/auth/session.ts"
```
Deep-dive on a specific file.

## Structural Queries

### Callers
```bash
rmap callers "AuthService.validate"
```

### Callees
```bash
rmap callees "AuthService.validate"
```

### Imports
```bash
rmap imports "src/core/auth/session.ts"
```

## Indexing

### Full Index
```bash
rmap index .
```
Index the current directory; the daemon allocates storage. Pass an explicit
`rmap index <repo_path>` to index a different directory.

### Incremental Refresh
```bash
rmap refresh
```

## Boundary Commands

```bash
rmap boundaries list
rmap boundaries show <surface_uid>
rmap boundaries summary
```

## Governance Commands

`assess` and `gate` resolve the repo from cwd (REG-1):

```bash
rmap assess [--baseline <snapshot>]
rmap gate
```

`declare *` has **not** migrated to REG-1: the handlers still require explicit
`<db_path> <repo_uid>` positionals. (The top-level `rmap --help` summary prints a
cwd-style `declare` form, but the handler rejects it and prints the positional
usage below — the handler is the shipped contract. See `docs/cli/rmap-contracts.md`.)

```bash
rmap declare quality-policy <db_path> <repo_uid> QP-001 \
  --policy-kind absolute_max \
  --measurement cyclomatic_complexity \
  --threshold 15 \
  --severity fail
```

## Output Format

`rmap` defaults to **human-readable** plain text. Pass `--json` for the full
machine envelope (CLI-OUT-1):

```bash
rmap trust            # human-readable (default)
rmap trust --json     # full machine envelope
```

For human inspection of the JSON in a shell, pipe to `jq`:
```bash
rmap trust --json | jq .
```

For agent execution, run the command and inspect the output directly (default
human text, or `--json` when you need to parse fields). Compound commands trigger
permission friction.

## Completeness Protocol

### Request Types

Before path analysis, state which type of request:

| Type | Description | Completeness Standard |
|------|-------------|----------------------|
| One-path trace | Follow single call chain | Traced, not verified complete |
| Entrypoint inventory | Control-flow roots for a subsystem | Enumerate from source within evidence limits |
| Path inventory | Indexed static paths through a subsystem | Entrypoints verified + downstream traced within current evidence |
| Probable cost centers | Structurally central functions | Structural only, not runtime |
| Runtime hot path | Measured execution-frequency hotspots | Requires profiling data |

Never conflate structural centrality with runtime cost.

### Required Workflow

#### Step 0: Verify Target

Before any query (REG-1 resolves the repo from cwd — there is no `db_path` /
`repo_uid` to supply):
- Run from inside the indexed repo; confirm it is registered (`rmap repo info`)
- Verify the daemon is reachable — a query that errors with `repo not indexed`
  means index it first (`rmap index .`)
- Verify the snapshot exists and is current (check `rmap trust` output)

#### Step 1: Run Trust

Check reliability tier before trusting graph output.

```bash
rmap trust
```

(For agent execution, read the default human output, or add `--json` and inspect
the envelope directly.)

#### Step 2: Interpret Reliability

| Tier | Meaning |
|------|---------|
| HIGH | Graph output authoritative within indexed static model. Does not prove runtime behavior or guard against query mistakes. |
| MEDIUM | Graph output useful but cross-check recommended. |
| LOW | Graph output is orientation only. Completeness requires source verification. |

#### Step 3: Enumerate Entrypoints from Source

Do not rely solely on graph for entrypoint discovery.

**Distinguish three categories:**

| Category | Definition | Examples |
|----------|------------|----------|
| Entrypoint roots | Externally or framework-invoked control-flow roots | CLI handlers, HTTP route handlers, scheduler callbacks, `main`, framework lifecycle hooks |
| Candidate surfaces | Externally visible but not proven roots | Public methods, exported functions — require caller verification |
| Shared core functions | Central internals with multiple callers | Not entrypoints; trace callers to find actual roots |

**Search for entrypoint roots first:**
- CLI: command handlers, subcommand registration
- HTTP: route handlers, middleware entry points
- Scheduler: cron callbacks, event handlers
- Framework: lifecycle hooks, plugin entry points
- Binary: `main`, `__main__`

**Then verify candidate surfaces:**
- Public/exported functions may be internal implementation
- Query callers or inspect usage before treating as entrypoint
- Many `pub fn` or `export` are not control-flow roots

**Use subsystem-specific naming conventions:**
- Orchestration: `*_repo`, `*_pipeline`, `run_*`, `process_*`
- Refresh/incremental: `refresh_*`, `incremental_*`, `update_*`
- CLI handlers: `cmd_*`, `handle_*`, command registration
- HTTP: route handlers, middleware

#### Step 4: Trace Downstream from Each Entrypoint

Separate concerns:
- **Entrypoint completeness**: Did I find all entry points?
- **Path completeness**: Did I trace all downstream paths from each?

#### Step 5: Query Callers for Shared Core Functions

```bash
rmap callers <function_name>
```

#### Step 6: Handle Zero Results

Zero results from graph queries can mean:
- No callers/callees exist (true negative)
- Symbol spelling or qualification wrong (query error)
- Wrong repo/database target (target error)
- Indexing stale or incomplete (data error)
- Extraction gap for this language/pattern (tool limitation)

**Rule**: If zero results contradict source expectations, verify symbol identity and target selection before concluding absence. This applies at ALL trust tiers, not just LOW.

Under LOW reliability, treat empty output as **suspicious by default**.

#### Step 7: Cross-Check with Source Search

Use language-appropriate source search (grep, ripgrep, IDE search, LSP):
- Sibling-name search for related functions
- Call-site search for the function name
- Definition search for exports/declarations

The policy is source verification, not a specific tool.

#### Step 8: Separate Paths Explicitly

- Main path
- Alternate/fallback path
- Refresh/incremental path
- Post-processing/subpipeline path
- Side pipelines

#### Step 9: Report with Evidence Labels

Path analysis reports must use both certainty labels AND evidence labels.

**Certainty labels** (what was established):

| Label | Meaning |
|-------|---------|
| TRACED_ONE_PATH | Followed single call chain, completeness not verified |
| ENTRYPOINTS_ENUMERATED | Found entry points via source search within evidence limits |
| NO_ADDITIONAL_PATHS_FOUND | Cross-checked graph + source, no missed paths found within current evidence |
| UNRESOLVED_ZONES | Explicitly list areas not covered |
| RUNTIME_NOT_MEASURED | Structural analysis only, no profiling data |

**Evidence labels** (how it was established):

| Label | Meaning |
|-------|---------|
| EXECUTED | Command run, output observed |
| OBSERVED | Artifact inspected directly |
| INFERRED | Concluded from context |
| NOT RUN | Skipped, state why |

Example report structure:
```
Certainty: NO_ADDITIONAL_PATHS_FOUND
Evidence: EXECUTED (rmap callers), EXECUTED (grep sibling search)
Unresolved: proto subpipeline internals not traced
```

### Trust-Fallback Rules

| Graph Reliability | Behavior |
|-------------------|----------|
| HIGH | Graph authoritative within static model; still verify zero-result anomalies |
| MEDIUM | Graph + spot-check source |
| LOW | Graph for orientation only; source search required for completeness |

Empty graph output is **not proof of absence** at any trust tier if it contradicts source expectations.

## When to Use Raw SQL

Only after `rmap` inspection, and only for:
- storage-layer debugging
- schema verification
- artifact existence checks that CLI doesn't expose

Always explain why CLI was insufficient.
