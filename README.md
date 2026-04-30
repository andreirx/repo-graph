# repo-graph

Deterministic code-graph discovery substrate for AI agents.

`repo-graph` indexes source code into a queryable graph and surfaces what an agent needs before changing code: module structure, boundaries, seams, dependencies, documentation inventory, trust levels, and quality signals. It is built for real repositories where unresolved structure, mixed languages, and architectural drift must remain visible.

## Product center

**Discovery is the primary goal. Enforcement is secondary.**

Repo-graph exists to model the relationships that determine how legacy systems can be understood and changed safely:
- modules and ownership
- boundaries and seams
- state and resource touchpoints
- runtime/build surfaces
- quality and risk signals
- documentation inventory as orientation evidence

Repo-graph is not a documentation authoring system. It is the deterministic discovery substrate that lets an agent:
1. orient with `rmap`
2. inspect existing docs
3. write or repair docs in the target repo if needed
4. implement with docs plus graph facts
5. re-index and re-check after changes

## Primary CLI: `rmap`

The Rust CLI (`rmap`) is the primary binary. Commands use explicit positional arguments:

```bash
rmap <command> <db_path> <repo_uid> [options]
```

### Discovery workflow

```bash
# Orient before changing code
rmap orient ./repo.db my-repo --focus "src/core/auth"

# Deep-dive on an unfamiliar file
rmap explain ./repo.db my-repo "src/core/auth/session.ts"

# Check structural and quality impact after changes
rmap check ./repo.db my-repo

# Surface documentation inventory
rmap docs list ./repo.db my-repo
```

### Structural queries

```bash
rmap callers ./repo.db my-repo "AuthService.validate"
rmap callees ./repo.db my-repo "AuthService.validate"
rmap imports ./repo.db my-repo "src/core/auth/session.ts"
rmap trust ./repo.db my-repo
```

### Quality discovery

```bash
rmap churn ./repo.db my-repo --since "2 weeks ago"
rmap hotspots ./repo.db my-repo
rmap risk ./repo.db my-repo
```

### Indexing

```bash
# Full index
rmap index ./path/to/repo ./repo.db

# Incremental refresh
rmap refresh ./path/to/repo ./repo.db
```

## Current shipped language support

Operational in `rmap`:
- TypeScript / JavaScript
- Rust
- Java
- Python
- C
- C++

Current strategic mobile/client track:
- Objective-C
- Objective-C++
- Swift
- Kotlin
- Dart

Those mobile/client languages are roadmap items, not shipped Rust-primary capability.

## What `rmap` surfaces today

At index time, repo-graph:
1. parses source files into `FILE` and `SYMBOL` nodes
2. emits structural edges such as `IMPORTS`, `CALLS`, `INSTANTIATES`, `IMPLEMENTS`
3. preserves unresolved references instead of dropping them
4. classifies unresolved edges into semantic buckets
5. computes trust and quality-related snapshot signals
6. inventories documentation files as first-class orientation evidence

Shipped structural and discovery areas include:
- modules and module ownership
- HTTP / CLI boundary surfaces
- state-boundary extraction
- C++ extraction with `extern "C"` ABI-boundary evidence
- local IPC boundary extraction in indexing/storage for C (Slice 1A)
- quality measurements: complexity, cognitive complexity, nesting, parameter count, function length, coverage, churn, hotspots, risk

Important current limitation:
- local IPC boundary facts are indexed and stored, but not yet exposed through a finished public `rmap boundaries ...` CLI surface

## Trust model

The trust system exists because an indexed graph is not automatically trustworthy.

`rmap trust` reports:
- unresolved edge classification counts
- call-graph reliability tier
- downgrade triggers and caveats
- enrichment coverage

Unknowns are surfaced, not erased.

## Quality and governance

Quality-policy declarations, assessments, and gate evaluation exist for teams that need hard enforcement, but they are not the product center.

```bash
rmap declare quality-policy ./repo.db my-repo QP-001 \
  --policy-kind absolute_max \
  --measurement cyclomatic_complexity \
  --threshold 15 \
  --severity fail

rmap assess ./repo.db my-repo
rmap gate ./repo.db my-repo
```

See `/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/docs/cli/rmap-contracts.md` for the governance CLI contract.

## Legacy CLI: `rgr`

The TypeScript CLI (`rgr`) remains for TS-side parity checks and features not yet ported to Rust.

```bash
rgr repo add ./path/to/repo --name my-repo
rgr repo index my-repo
rgr enrich my-repo
```

## Current non-goals and cautions

- Public dead-code claims are withdrawn until coverage-backed evidence is integrated. Do not treat old dead-code expectations as current product behavior.
- SQLite is the current persistence mechanism, not the conceptual end-state center.
- The long-term runtime direction is a daemon that coordinates many-reader/few-writer shared access for multiple AI agents.

## Installation

Requirements:
- Node.js 20+
- Rust toolchain
- `pnpm`

```bash
# TypeScript side
pnpm install
pnpm rebuild better-sqlite3
pnpm build

# Rust side
cd rust && cargo build --release
```

For semantic enrichment, ensure language tooling is available where applicable:
- Rust: `rust-analyzer`
- Java: `jdtls`

## Architecture summary

Two CLI binaries share SQLite storage today:
- `rmap` (Rust) — primary, agent-facing product surface
- `rgr` (TypeScript) — legacy / parity / not-yet-ported surfaces

Core rules:
- discovery surfaces are primary
- unresolved structure is first-class data
- storage is an adapter, not the business-logic center
- documentation inventory is primary orientation evidence
- trust and degradation are explicit

The long-term architecture direction is a long-lived daemon with an in-memory current-state graph. SQLite remains the transitional persistence/query adapter until that runtime is built.

## Documentation map

| Topic | Location |
|---|---|
| Vision and priorities | `/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/docs/VISION.md` |
| Roadmap | `/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/docs/ROADMAP.md` |
| Known limitations / debt | `/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/docs/TECH-DEBT.md` |
| Rust CLI contract | `/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/docs/cli/rmap-contracts.md` |
| TS CLI reference | `/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/docs/cli/v1-cli.txt` |
| Database schema | `/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/docs/architecture/schema.txt` |
| Test protocol | `/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/docs/testing/rmap-test-protocol.md` |

## Development

```bash
# Build
pnpm build
cd rust && cargo build

# Test
pnpm test
pnpm run test:rust
pnpm run test:all

# Lint
pnpm lint
```

## License

MIT
