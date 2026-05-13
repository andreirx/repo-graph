# rmap Orientation

## Primary Principle

Use `rmap` to understand the system before changing code.
Use `rmap` to validate changes after editing.

Do not use raw SQL as a shortcut.

## Command Pattern

```bash
rmap <command> <db_path> <repo_uid> [options]
```

## Orientation Commands

### Orient
```bash
rmap orient ./repo-graph.db repo-graph --focus "src/core"
```
Get a high-level view of the codebase or a specific area.

### Trust
```bash
rmap trust ./repo-graph.db repo-graph
```
Check what the system knows vs. what it doesn't. Reports:
- unresolved edge classification counts
- call-graph reliability tier
- downgrade triggers and caveats
- enrichment coverage

### Check
```bash
rmap check ./repo-graph.db repo-graph
```
Structural and quality check after changes.

### Explain
```bash
rmap explain ./repo-graph.db repo-graph "src/core/auth/session.ts"
```
Deep-dive on a specific file.

## Structural Queries

### Callers
```bash
rmap callers ./repo-graph.db repo-graph "AuthService.validate"
```

### Callees
```bash
rmap callees ./repo-graph.db repo-graph "AuthService.validate"
```

### Imports
```bash
rmap imports ./repo-graph.db repo-graph "src/core/auth/session.ts"
```

## Indexing

### Full Index
```bash
rmap index ./path/to/repo ./repo.db
```

### Incremental Refresh
```bash
rmap refresh ./path/to/repo ./repo.db
```

## Boundary Commands

```bash
rmap boundaries list ./repo.db repo-uid
rmap boundaries show ./repo.db repo-uid <boundary-id>
rmap boundaries summary ./repo.db repo-uid
```

## Governance Commands

```bash
rmap declare quality-policy ./repo.db repo-uid QP-001 \
  --policy-kind absolute_max \
  --measurement cyclomatic_complexity \
  --threshold 15 \
  --severity fail

rmap assess ./repo.db repo-uid
rmap gate ./repo.db repo-uid
```

## Output Format

`rmap` outputs JSON (the CLI transport contract).

For human reading in shell (not agent execution):
```bash
rmap trust ./repo-graph.db repo-graph | jq .
```

For agent execution, run command without pipe and inspect JSON output directly. Compound commands trigger permission friction.

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

Before any query:
- Verify correct binary path (build if needed)
- Verify correct db path exists
- Verify correct repo_uid
- Verify snapshot exists and is current (check `rmap trust` output)

#### Step 1: Run Trust

Check reliability tier before trusting graph output.

```bash
rmap trust <db> <repo>
```

(Shell example. For agent execution, run without pipe, inspect JSON directly.)

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
rmap callers <db> <repo> <function_name>
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
