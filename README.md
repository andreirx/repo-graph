# repo-graph

Deterministic code-graph discovery for AI agents.

`repo-graph` indexes source code into a queryable graph and surfaces what an agent needs to understand before changing code: module structure, boundaries, dependencies, documentation, trust levels, and quality signals. It is built for real repositories where "best effort" graph output is not enough.

## What This Tool Does

**Discovery, not enforcement.** The primary goal is helping agents understand codebases, not blocking CI pipelines.

At index time:
1. Parses source files into `FILE` and `SYMBOL` nodes
2. Emits graph edges: `IMPORTS`, `CALLS`, `INSTANTIATES`, `IMPLEMENTS`
3. Preserves unresolved references instead of dropping them
4. Classifies unresolved edges into semantic buckets
5. Computes trust and reliability surfaces over the snapshot

After indexing, agents can:
- **Orient** before changing code (`rmap orient`)
- **Explain** unfamiliar areas with bounded deep-dives (`rmap explain`)
- **Check** structural impact after changes (`rmap check`)
- Query callers, callees, paths, cycles, dead nodes
- Surface documentation inventory for focused areas
- See quality measurements and risk signals
- Understand trust levels and graph reliability

## Primary CLI: `rmap`

The Rust CLI (`rmap`) is the primary binary. All commands use `<db_path> <repo_uid>` positional arguments:

```bash
rmap <command> <db_path> <repo_uid> [options]
```

### Discovery Commands

```bash
# Before changing code: understand the area
rmap orient ./repo.db my-repo --focus "src/core/auth"

# Deep-dive on unfamiliar code
rmap explain ./repo.db my-repo "src/core/auth/session.ts"

# After changing code: see structural impact
rmap check ./repo.db my-repo

# Documentation for focused area
rmap docs list ./repo.db my-repo
```

### Structural Queries

```bash
# Graph traversal
rmap callers ./repo.db my-repo "AuthService.validate"
rmap callees ./repo.db my-repo "AuthService.validate"
rmap path ./repo.db my-repo "src/api/handler.ts" "src/db/connection.ts"

# Dead code and cycles
rmap dead ./repo.db my-repo SYMBOL
rmap cycles ./repo.db my-repo

# Trust and reliability
rmap trust ./repo.db my-repo
```

### Quality Discovery

```bash
# Measurements
rmap churn ./repo.db my-repo --since "2 weeks ago"
rmap hotspots ./repo.db my-repo
rmap risk ./repo.db my-repo
```

### Indexing

```bash
# Index a repository
rmap index ./path/to/repo ./repo.db

# Re-index after changes
rmap refresh ./path/to/repo ./repo.db
```

## Languages

Implemented extractors:
- TypeScript / JavaScript
- Rust
- Java
- Python
- C / C++

All use tree-sitter for syntax extraction. Semantic enrichment available for TS (TypeScript compiler), Rust (rust-analyzer), and Java (jdtls).

## Trust Model

The trust system exists because an indexed graph is not automatically trustworthy.

`rmap trust` reports:
- Unresolved edge classification counts
- Call-graph reliability tier (HIGH, MEDIUM, LOW)
- Downgrade triggers and caveats
- Enrichment coverage

Unresolved edges are classified into:
- `external_library_candidate` — likely points outside repo
- `internal_candidate` — likely points to repo-internal code
- `framework_boundary_candidate` — matches framework registration pattern
- `unknown` — no defensible signal

Unknowns are surfaced, not erased.

## Governance Substrate (Secondary)

Quality-policy declarations and gate evaluation exist for teams that need CI blocking and formal compliance workflows. This is available infrastructure, not the primary product surface.

```bash
# Declare policies
rmap declare quality-policy ./repo.db my-repo QP-001 \
  --policy-kind absolute_max --measurement cyclomatic_complexity \
  --threshold 15 --severity fail

# Evaluate policies
rmap assess ./repo.db my-repo

# CI gate (when needed)
rmap gate ./repo.db my-repo
```

See `docs/cli/rmap-contracts.md` for full governance CLI contract.

## Legacy CLI: `rgr`

The TypeScript CLI (`rgr`) remains for features not yet ported to Rust. It uses a repo registry pattern:

```bash
rgr repo add ./path/to/repo --name my-repo
rgr repo index my-repo
rgr enrich my-repo
```

## Installation

Requirements:
- Node.js 20+ (for TS CLI and grammars)
- Rust toolchain (for `rmap`)
- `pnpm`

```bash
# TypeScript CLI
pnpm install
pnpm rebuild better-sqlite3
pnpm build

# Rust CLI
cd rust && cargo build --release
```

For semantic enrichment, ensure language servers are on PATH:
- Rust: `rust-analyzer` (`rustup component add rust-analyzer`)
- Java: `jdtls`

## Architecture

Two CLI binaries with shared SQLite storage:
- `rmap` (Rust) — primary, agent-facing commands
- `rgr` (TypeScript) — enrichment, boundary extraction, legacy features

Storage is SQLite. Each command takes an explicit `<db_path>` argument.

Core architectural rules:
- Discovery surfaces are primary
- Computed facts and human declarations are separate
- Unresolved structure is first-class data
- Trust reporting is explicit, not hidden

## Documentation

| Topic | Location |
|-------|----------|
| Vision and priorities | `docs/VISION.md` |
| Roadmap | `docs/ROADMAP.md` |
| Known limitations | `docs/TECH-DEBT.md` |
| Rust CLI contract | `docs/cli/rmap-contracts.md` |
| TS CLI reference | `docs/cli/v1-cli.txt` |
| Database schema | `docs/architecture/schema.txt` |

## Development

```bash
# Build
pnpm build                    # TypeScript
cd rust && cargo build        # Rust

# Test
pnpm test                     # TS tests
pnpm run test:rust            # Rust tests
pnpm run test:all             # Full acceptance

# Lint
pnpm lint
```

## License

MIT
