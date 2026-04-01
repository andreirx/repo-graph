# Repo-Graph (rgr)

Deterministic code graph tool for analyzing legacy codebases. TypeScript CLI over SQLite. Produces structured JSON for AI agent consumption.

## What this is

A CLI tool (`rgr`) that indexes source code repositories into a queryable graph stored in SQLite. Nodes are symbols, files, modules, endpoints. Edges are typed relationships (CALLS, IMPORTS, IMPLEMENTS, etc.). Every fact carries evidence and resolution (static/dynamic/inferred).

Not a chatbot. Not a vector DB. A deterministic graph query engine.

## Architecture

Clean Architecture. Dependency rule: inward only.

```
src/core/model/     Domain entities (Node, Edge, Snapshot, Declaration). Zero external deps.
src/core/queries/   Use cases (callers, callees, path, dead, cycles). Depend on core model only.
src/core/ports/     Interfaces (StoragePort, ExtractorPort). Implemented by adapters.
src/adapters/       SQLite storage, Tree-sitter extractors, Git adapter. Implement core ports.
src/cli/            Commander-based CLI. Depends on use cases, not adapters directly.
src/main.ts         Composition root. Only file that wires concrete adapters to ports.
```

Nothing in `core/` imports from `adapters/` or `cli/`.

## Key design decisions

Read `docs/architecture/` for full context. Summary:

- **SQLite is the local backend.** Portable schema designed for future Postgres central backend.
- **TEXT UIDs everywhere.** No auto-increment integers as identity. Globally portable.
- **Relative paths.** All file paths in the DB are repo-relative. Never absolute.
- **Three data categories:** Facts (extracted, confidence 1.0), Declarations (human-supplied), Inferences (computed, carry confidence). Stored in separate tables.
- **18 canonical edge types.** Each edge has a `resolution` (static/dynamic/inferred) and `extractor` (name:version).
- **Framework extractors are the differentiation.** Base Tree-sitter extraction is commodity. Express/NestJS/Spring route extraction captures edges invisible to import analysis.

## Commands

```
pnpm run build          Build TypeScript
pnpm run test           Run all tests
pnpm run test:unit      Run unit tests only (test/core)
pnpm run test:int       Run integration tests only (test/adapters)
pnpm run lint           Run Biome linter
pnpm run lint:fix       Auto-fix lint issues
```

## Native dependency note

`better-sqlite3` is a native Node.js addon. If you switch Node.js versions (e.g. via nvm), tests will fail with `NODE_MODULE_VERSION` mismatch. Fix:

```
pnpm rebuild better-sqlite3
```

## Current phase: v1

v1 scope is strictly: index one TypeScript repo, answer structural graph queries.

See `docs/cli/v1-cli.txt` for the exact command surface and acceptance criteria.
See `docs/architecture/schema.txt` for the database schema.
See `docs/architecture/project-structure.txt` for the full folder layout and dependency map.

## Conventions

- Use `pnpm` not npm.
- All CLI output supports `--json`. Default is human-readable tables.
- Tests use fixture codebases in `test/fixtures/` with known-good expected graphs.
- No emojis in code or output.

## Eat Your Own Dogfood For Codebase Intelligence (rgr)

Use `rgr` (Repo-Graph) for structural analysis. If rgr is available on PATH:

```bash
rgr repo add .                           # Register this repo
rgr repo index repo-graph                     # Full index (~400 files, <1s)
rgr graph cycles repo-graph                   # Module-level dependency cycles
rgr graph dead repo-graph --kind SYMBOL       # Unreferenced exported symbols
rgr graph callers repo-graph <symbol>         # Who calls this?
rgr graph callers repo-graph <symbol> --edge-types CALLS,INSTANTIATES  # Who uses this?
rgr graph path repo-graph <from> <to>         # Shortest path between two symbols
```

Use `/investigate-symbol repo-graph <SymbolName>` for a guided investigation workflow. Use `/repo-overview .` for a full structural health check.

rgr uses syntax-only resolution (no type info). Import graphs are accurate. Call graphs are conservative — `this.method()` and `obj.method()` calls may not resolve. When callee results look incomplete, read the source directly.

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

