# Repo-Graph (rgr)

Deterministic code graph tool for analyzing legacy codebases. TypeScript CLI over SQLite. Produces structured JSON for AI agent consumption.

## What this is

A CLI tool (`rgr`) that indexes source code repositories into a queryable graph stored in SQLite. Nodes are symbols, files, modules, endpoints. Edges are typed relationships (CALLS, IMPORTS, IMPLEMENTS, etc.). Every fact carries evidence and resolution (static/dynamic/inferred).

Not a chatbot. Not a vector DB. A deterministic graph query engine.

## Architecture

Clean Architecture. Dependency rule: inward only.

```
src/core/model/     Domain entities (Node, Edge, Snapshot, Declaration). Zero external deps.
src/core/ports/     Interfaces (StoragePort, ExtractorPort, IndexerPort). Implemented by adapters.
src/adapters/       SQLite storage, Tree-sitter extractors, indexer, manifest extractor.
src/cli/            Commander-based CLI. Depends on ports, not adapters directly.
src/main.ts         Composition root. Only file that wires concrete adapters to ports.
src/version.ts      Canonical version manifest. Single source of truth for all component versions.
```

Nothing in `core/` imports from `adapters/` or `cli/`.

## Key design decisions

Read `docs/architecture/` for full context. Summary:

- **SQLite is the local backend.** Portable schema designed for future Postgres central backend.
- **TEXT UIDs everywhere.** No auto-increment integers as identity. Globally portable.
- **Relative paths.** All file paths in the DB are repo-relative. Never absolute.
- **Four data categories:** Facts (nodes/edges, confidence 1.0), Measurements (deterministic metrics), Declarations (human-supplied), Inferences (computed, carry confidence). Stored in separate tables.
- **18 canonical edge types.** Each edge has a `resolution` (static/dynamic/inferred) and `extractor` (name:version).
- **Framework extractors are the differentiation** (planned v2). Base Tree-sitter extraction is commodity. Express/NestJS/Spring route extraction will capture edges invisible to import analysis.

## Commands

```
pnpm run build          Build TypeScript
pnpm run test           Build + run all tests (includes CLI integration)
pnpm run test:int       Run integration tests only (test/adapters)
pnpm run test:cli       Build + run CLI integration tests only (test/cli)
pnpm run lint           Run Biome linter
pnpm run lint:fix       Auto-fix lint issues
```

## Native dependency note

`better-sqlite3` is a native Node.js addon. If you switch Node.js versions (e.g. via nvm), tests will fail with `NODE_MODULE_VERSION` mismatch. Fix:

```
pnpm rebuild better-sqlite3
```

## Current phase: v1.5

v1.5 scope: index one TypeScript repo, answer structural graph queries,
enforce architecture boundaries, compute quality measurements, extract domain versions.

See `docs/cli/v1-cli.txt` for the exact command surface.
See `docs/architecture/schema.txt` for the database schema.
See `docs/architecture/project-structure.txt` for the folder layout.
See `docs/architecture/measurement-model.txt` for the four-layer truth model.
See `docs/architecture/versioning-model.txt` for toolchain provenance.
See `docs/architecture/v1-validation-report.txt` for extraction capability boundary.

## Conventions

- Use `pnpm` not npm.
- All CLI output supports `--json`. Default is human-readable tables.
- Tests use fixture codebases in `test/fixtures/` with known-good expected graphs.
- No emojis in code or output.

## Eat Your Own Dogfood For Codebase Intelligence (rgr)

Use `rgr` (Repo-Graph) for structural analysis. If rgr is available on PATH:

```bash
# Repository management
rgr repo add .                           # Register this repo
rgr repo index repo-graph                # Full index (<1s)
rgr repo status repo-graph               # Current snapshot, toolchain provenance
rgr repo refresh repo-graph              # Re-index (refresh snapshot)
rgr repo list                            # All registered repos
rgr repo remove repo-graph               # Unregister

# Structural graph queries
rgr graph callers repo-graph <symbol>    # Who calls this?
rgr graph callers repo-graph <symbol> --edge-types CALLS,INSTANTIATES
rgr graph callees repo-graph <symbol>    # What does this call?
rgr graph imports repo-graph <file>      # Import chain
rgr graph path repo-graph <from> <to>    # Shortest path between two symbols
rgr graph dead repo-graph --kind SYMBOL  # Unreferenced exported symbols
rgr graph cycles repo-graph              # Module-level dependency cycles

# Architecture & governance
rgr arch violations repo-graph           # IMPORTS edges crossing forbidden boundaries
rgr graph obligations repo-graph         # Evaluate verification obligations (PASS/FAIL/MISSING)

# Quality measurements
rgr graph stats repo-graph               # Module structural metrics (fan-in/out, instability)
rgr graph metrics repo-graph --limit 10  # Top complex functions (cyclomatic complexity)
rgr graph metrics repo-graph --module    # Per-module complexity aggregates
rgr graph versions repo-graph            # Extracted domain versions (package.json)
rgr graph churn repo-graph --since 90.days.ago  # Per-file git churn
rgr graph hotspots repo-graph            # Churn x complexity ranking
rgr graph coverage repo-graph ./coverage/coverage-final.json  # Import Istanbul/c8 coverage
rgr graph risk repo-graph                # Under-tested hotspots (hotspot x coverage gap)

# Declarations
rgr declare module repo-graph src/core --purpose "domain model" --maturity MATURE
rgr declare boundary repo-graph src/core --forbids src/adapters
rgr declare entrypoint repo-graph <symbol> --type public_export
rgr declare invariant repo-graph <symbol> "must not fail silently"
rgr declare requirement repo-graph src/core --req-id REQ-001 --objective "..."
rgr declare obligation repo-graph REQ-001 --obligation "..." --method arch_violations --target src/core
rgr declare list repo-graph --kind requirement
rgr declare remove repo-graph <uid>
```

Use `/investigate-symbol repo-graph <SymbolName>` for a guided investigation workflow.
Use `/repo-overview .` for a full structural health check.

Agent skills in `docs/skills/`:
- `investigate-symbol.txt` — understand a symbol's callers, callees, and dead-code status
- `assess-code-health.txt` — full structural + quality health report (stats, metrics, hotspots, risk)
- `verify-requirements.txt` — check verification obligations against measurements

rgr uses syntax-only resolution with receiver type binding. Import graphs are accurate. Call graphs resolve well on class-heavy architectures with explicit typing; weaker on SDK-heavy or functional patterns. When callee results look incomplete, read the source directly.

Trust boundaries for `graph dead`:
- On clean-architecture codebases with explicit call/import graphs: high confidence.
- On registry/plugin-driven architectures (CMS renderers, extension registries, render maps, string-key dispatch): treat results as "graph orphans," not deletion candidates. These codebases wire liveness through channels the graph does not yet model.
- Use `declare entrypoint` to suppress known live nodes that appear as false positives.

`arch violations` checks IMPORTS edges only. It does not check CALLS edges (call graph resolution is architecture-sensitive). The import graph is the trustworthy substrate for policy enforcement.

When you find architectural breadcrumbs, comments, or docs in the repo:
- Do not copy them into repo-graph blindly.
- First verify them against current code.
- If the claim is still true and operationally useful, store the distilled fact in rgr as a declaration.
- Prefer short, canonical facts over prose.
- Treat rgr as the system of record for verified architectural facts, with source files/docs serving as evidence.
- If a breadcrumb conflicts with code, trust code and report the drift.

HOWEVER - Only store a fact in rgr if it would help a future agent make a better design/debugging decision without rereading the same code.
Store surprises, constraints, hazards, and architectural intent.
Don’t store summaries of obvious code.

## Known limitations and test-scope debt

Extraction:
- Call graph resolution: 37% on self-index. Strong on class-heavy architectures,
  weak on SDK-heavy/functional patterns. See v1-validation-report.txt.
- Inherited method resolution: 11 remaining cases on FRAKTAG (diminishing returns).
- External SDK types (node_modules): not indexed.
- Destructured bindings, reassignment: not tracked.

Coverage/churn import:
- File filtering uses repo-level file inventory (getFilesByRepo), not snapshot-scoped
  FILE nodes. Adequate for single-snapshot model, needs tightening for multi-snapshot.

Test coverage gaps:
- Non-empty `graph hotspots` and `graph risk` CLI paths: not testable because the
  test fixture is not a git repo (churn returns empty). Requires temp git repo harness.
- `supersedes_uid` linkage in obligation supersession: implemented but not verified
  because `declare list --json` does not expose the field.
- Obligation evaluation for coverage_threshold, complexity_threshold, hotspot_threshold:
  only arch_violations PASS and UNSUPPORTED verdicts tested end-to-end.

Dead code trust boundary:
- `graph dead` answers "no known inbound graph edges," not "semantically unreachable."
- Registry/plugin-driven architectures produce false positives.
- See v1-validation-report.txt for the full extraction capability boundary.

