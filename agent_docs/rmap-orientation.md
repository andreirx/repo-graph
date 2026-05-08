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

`rmap` currently outputs JSON (the CLI transport contract). Parse with `jq` for human reading:

```bash
rmap trust ./repo-graph.db repo-graph | jq .
```

## When to Use Raw SQL

Only after `rmap` inspection, and only for:
- storage-layer debugging
- schema verification
- artifact existence checks that CLI doesn't expose

Always explain why CLI was insufficient.
