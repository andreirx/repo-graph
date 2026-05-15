# repo-graph

Deterministic code-graph discovery substrate for AI agents.

`repo-graph` indexes source code into a queryable graph and surfaces what an agent needs before changing code: module structure, boundaries, seams, dependencies, documentation inventory, trust levels, and quality signals. It is built for real repositories where unresolved structure, mixed languages, and architectural drift must remain visible.

## Product center

**Discovery is the primary goal. Enforcement is secondary.**

### Orientation, not oracle

Repo-graph helps an AI agent **look in the right places, open the right files, and ask the right questions**. It does not replace agent cognition or guarantee exhaustive answers.

The product narrows the search space and highlights what matters. The agent reads the actual files and makes the final engineering decisions. If repo-graph can surface more precise information (exact callers, boundary consumers, call sites), it will — but the primary contract is orientation, not completeness.

### Product layers

Repo-graph capabilities form a dependency tree. Later layers require earlier layers. Each layer has a distinct certainty contract.

**Layer 0 — Extraction substrate.** The non-negotiable core. File inventory, language routing, symbol extraction, structural edges (IMPORTS, CALLS, INSTANTIATES, IMPLEMENTS), unresolved edge preservation, manifest inputs, stable keys. Certainty: extracted fact, deterministic, reproducible.

**Layer 1 — Architectural substrate.** What agents need first for orientation. Callers/callees/imports queries, declared modules, documentation inventory, trust reporting, quality measurements, change-impact primitives. Certainty: extracted fact, deterministic.

**Layer 2 — Derived architecture.** Combines fact with bounded inference. Inferred modules, module dependency graph, runtime/build surfaces, seam rollups, risk/hotspot/churn overlays. Certainty: interpretation with explicit basis — useful but one step removed from raw extraction.

**Layer 3 — Orientation hints.** Useful but not product center. Framework detectors, HTTP surfaces, gRPC hints, IPC/socket/signal detection, message-broker detection, policy propagation markers. Certainty: evidence-backed hints with explicit unknowns and confidence limits.

**Layer 4 — Governance overlay.** Constrains action, never redefines lower-layer truth. Declarations, quality policies, assessments, gate verdicts, waivers. Certainty: policy overlays fact — computed truth is always preserved alongside effective (waiver-adjusted) truth.

**Doctrine:** Inner layers (0–1) pursue deterministic extracted truth. Outer layers (2–3) surface partial, source-anchored hints with explicit unknowns. Governance (4) overlays fact but never erases it.

### What repo-graph models

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

The Rust CLI (`rmap`) is the primary binary. Query commands use explicit positional arguments:

```bash
rmap <command> <db_path> <repo_uid> [options]
```

Index/refresh commands differ (daemon required):
```bash
rmap index   <repo_path> <db_path>
rmap refresh <db_path> <repo_uid>
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
# Full index (daemon required)
rmap index ./path/to/repo ./repo.db

# Incremental refresh (daemon required)
rmap refresh ./repo.db my-repo
```

### Daemon Mode

The daemon (`rmapd`) is installed as a system service by the installer. To run manually:

```bash
# Start the daemon (normally auto-started by launchd/systemd)
rmapd
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

Capabilities are organized by product layer. Inner layers are extracted facts; outer layers are derived or partial.

### Layer 0-1: Extraction substrate (shipped)

At index time, repo-graph:
1. parses source files into `FILE` and `SYMBOL` nodes
2. emits structural edges: `IMPORTS`, `CALLS`, `INSTANTIATES`, `IMPLEMENTS`
3. preserves unresolved references instead of dropping them
4. classifies unresolved edges into semantic buckets
5. computes quality measurements: complexity, cognitive complexity, nesting, parameter count, function length
6. inventories documentation files as first-class orientation evidence
7. extracts C++ symbols with `extern "C"` ABI-boundary linkage detection
8. extracts local IPC boundary facts for C (indexed and stored)

### Layer 2: Derived architecture (partial)

Schema support exists but Rust indexer population is incomplete:
- module inference (`module_candidates`) — TS path populates, Rust path emits compatibility MODULE nodes only
- HTTP/CLI boundary surfaces (`project_surfaces`) — partial population
- state-boundary extraction (resource nodes + READS/WRITES) — schema exists, not populated
- contract schemas (`contract_schemas`) — schema exists, not populated

### Layer 3: Orientation hints (mixed maturity)

Specialized evidence tracks with varying implementation state:
- local IPC boundary surfaces — shipped (BI-1A), public CLI via `rmap boundaries list/show/summary/links`
- HTTP boundary model — documented as mature
- policy facts (RETURN_FATE, STATUS_MAPPING) — implemented
- gRPC contract-based linking — shipped (GR-3A), links provider/consumer surfaces by shared proto service
- framework detectors — partial, some TS-only
- broader IPC link inference — roadmap

### Current Rust-primary visibility gaps

- Some boundary contract linking remains incomplete
- Enrichment passes not yet ported to Rust

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

## Current non-goals and cautions

- Public dead-code claims are withdrawn until coverage-backed evidence is integrated. Do not treat old dead-code expectations as current product behavior.
- SQLite is the current persistence mechanism, not the conceptual end-state center.
- The runtime architecture now relies on a shipped, long-lived daemon that coordinates many-reader/few-writer shared access for multiple AI agents, removing CLI bootstrap latency.

## Installation

### Binary install (recommended)

```bash
curl -fsSL https://github.com/andreirx/repo-graph/releases/latest/download/install.sh | bash
```

This installs `rmap`, `rmapd`, and `rgistr` to `~/.local/bin` and configures the daemon service.

### Build from source

Requirements:
- Rust toolchain

```bash
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

The architecture now includes a shipped, long-lived daemon providing an in-memory current-state coordination layer. SQLite remains the persistence and query adapter underlying this runtime.

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
cd rust

# Build
cargo build

# Test
cargo test

# Lint
cargo clippy
```

## Legacy TypeScript CLI (Deprecated)

The TypeScript codebase (`rgr` CLI) is deprecated and being phased out as features achieve parity in Rust. 

If you still need to run the TS codebase for legacy features:
Requirements: Node.js 20+, `pnpm`
```bash
pnpm install
pnpm rebuild better-sqlite3
pnpm build
pnpm test
```

## License

MIT
