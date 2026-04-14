# Repo-Graph (rgr)

Deterministic code graph tool for analyzing codebases. Multi-language (TypeScript, Rust, Java, Python, C/C++). CLI over SQLite. Produces structured JSON for AI agent consumption.

## What this is

A CLI tool (`rgr`) that indexes source code repositories into a queryable graph stored in SQLite. Nodes are symbols, files, modules, endpoints. Edges are typed relationships (CALLS, IMPORTS, IMPLEMENTS, etc.). Every fact carries evidence and resolution (static/dynamic/inferred).

Not a chatbot. Not a vector DB. A deterministic graph query engine.

## Architecture

Clean Architecture. Dependency rule: inward only.

```
src/core/model/          Domain entities (Node, Edge, Snapshot, Declaration). Zero external deps.
src/core/ports/          Interfaces (StoragePort, ExtractorPort, IndexerPort). Implemented by adapters.
src/core/classification/ Unresolved-edge classifier, blast-radius derivation, framework-boundary detection, boundary matcher (HTTP + CLI).
src/core/trust/          Trust reporting: reliability rules, service orchestrator.
src/adapters/extractors/ Tree-sitter extractors: typescript/, rust/, java/, python/, cpp/.
src/adapters/enrichment/ Post-index semantic enrichment: TS TypeChecker, Rust rust-analyzer LSP.
src/adapters/config/     Config readers: tsconfig-reader, cargo-reader, gradle-reader, python-deps-reader.
src/adapters/storage/    SQLite storage (migrations 001-008).
src/adapters/indexer/    Multi-language indexer: file routing, edge resolution, classification.
src/cli/                 Commander-based CLI. Depends on ports, not adapters directly.
src/main.ts              Composition root. Wires all extractors and adapters.
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
pnpm run build                Build TypeScript
pnpm run test                 Build + run all default TS tests (includes CLI integration)
pnpm run test:int             Run TS integration tests only (test/adapters)
pnpm run test:cli             Build + run CLI integration tests only (test/cli)
pnpm run test:rust                   Run ALL Rust workspace crates (detectors + storage + classification + trust + indexer + ts-extractor + repo-index + rgr)
pnpm run test:parity                 Run the detector cross-runtime parity harness (TS half + Rust half)
pnpm run test:storage:parity         Run the storage cross-runtime parity harness (TS half + Rust half)
pnpm run test:classification:parity  Run the classification cross-runtime parity harness (TS half + Rust half)
pnpm run test:trust:parity           Run the trust cross-runtime parity harness (TS half + Rust half)
pnpm run test:indexer:parity         Run the indexer cross-runtime parity harness (TS half + Rust half)
pnpm run test:ts-extractor:parity   Run the ts-extractor cross-runtime parity harness (TS half + Rust half)
pnpm run test:interop                Run the cross-runtime DB interop test (Rust writes SQLite, TS reads)
pnpm run test:all                    Composite acceptance: full default TS suite + full Rust workspace
pnpm run test:live            Run live external-tool integration tests (test/live, opt-in)
pnpm run lint                 Run Biome linter
pnpm run lint:fix             Auto-fix lint issues
```

The Rust scripts (`test:rust`, `test:parity`, `test:storage:parity`,
`test:classification:parity`, `test:trust:parity`,
`test:indexer:parity`, `test:ts-extractor:parity`, `test:interop`,
`test:all`) shell out to `cargo` and require a Rust toolchain. The
pinned channel lives in `rust/rust-toolchain.toml`. If `cargo` is not
on PATH the scripts fail fast with an actionable error pointing at
rustup. TS-only contributors who never touch the Rust crates can
ignore these scripts; `pnpm test` remains unchanged and does not
require cargo.

`test:rust` runs `cargo test --workspace` which executes every Rust
crate's unit tests + integration tests, including all six parity test
files (`rust/crates/detectors/tests/parity.rs`,
`rust/crates/storage/tests/parity.rs`,
`rust/crates/classification/tests/parity.rs`,
`rust/crates/trust/tests/parity.rs`,
`rust/crates/indexer/tests/parity.rs`, and
`rust/crates/ts-extractor/tests/parity.rs`). The dedicated
`test:parity`, `test:storage:parity`, `test:classification:parity`,
`test:trust:parity`, `test:indexer:parity`, and
`test:ts-extractor:parity` scripts are for targeted cross-runtime
verification: each runs BOTH the TS half (via vitest against the
corresponding shared fixture corpus) AND the Rust half (via the
qualified `cargo test -p <crate> --test parity` invocation). They
share the same cargo-guard prefix.

The six parity surfaces (detector parity in `parity-fixtures/`,
storage parity in `storage-parity-fixtures/`, classification parity
in `classification-parity-fixtures/`, trust parity in
`trust-parity-fixtures/`, indexer parity in
`indexer-parity-fixtures/`, ts-extractor parity in
`ts-extractor-parity-fixtures/`) are deliberately independent scripts.
A detector-code change runs `test:parity` for fast feedback without
paying the storage, classification, trust, indexer, or extractor harness
cost; a storage-code change runs `test:storage:parity`; a
classification-code change runs `test:classification:parity`; a
trust-code change runs `test:trust:parity`; an indexer-code change
runs `test:indexer:parity`. `test:all` transitively covers all six
(via `test` + `test:rust`) for contributors who want comprehensive
coverage in one command.

`pnpm run test:interop` is a separate cross-runtime acceptance surface
that builds the Rust `rgr-rust` binary, indexes a checked-in fixture
repo into a temp SQLite file, then opens that file from the TS storage
adapter and verifies real read-side operations succeed. It proves that
Rust-written databases are consumable by the existing TS product layer.

`pnpm run test:live` is a peer surface that runs the live external-tool
integration tests under `test/live/` (currently jdtls and rust-analyzer
hover-resolver tests). These are EXCLUDED from `pnpm run test` and
`pnpm run test:all` because their pass/fail depends on external language
servers being installed AND in a healthy workspace state. They self-skip
gracefully when the required tool is missing from PATH, but they cannot
detect tool-state corruption (jdtls workspace drift, rust-analyzer
indexing race) and therefore are not admissible as a deterministic
acceptance baseline. Run them explicitly when investigating enrichment
behavior; do not rely on them for slice closure evidence.

## Rust CLI (`rgr-rust`)

Milestone: `rgr-rust-structural-v1`. See `docs/milestones/rgr-rust-structural-v1.md`
for full contracts, slice inventory, and deferred items.

The Rust binary `rgr-rust` (crate `repo-graph-rgr`) is the Rust-side
CLI. It produces JSON-only output on stdout and uses stderr for errors.
Exit codes: 0 success, 1 usage error, 2 runtime error.

Commands (structural: Rust-7B through Rust-20, governance: Rust-22+):

```
rgr-rust index      <repo_path> <db_path>              # Full index to SQLite
rgr-rust refresh    <repo_path> <db_path>              # Delta refresh
rgr-rust trust      <db_path> <repo_uid>               # Trust report
rgr-rust callers    <db_path> <repo_uid> <symbol> [--edge-types <types>]  # Direct callers (one hop)
rgr-rust callees    <db_path> <repo_uid> <symbol> [--edge-types <types>]  # Direct callees (one hop)
rgr-rust path       <db_path> <repo_uid> <from> <to>   # Shortest path (CALLS+IMPORTS, depth 8)
rgr-rust imports    <db_path> <repo_uid> <file_path>   # File import chain (one hop, IMPORTS)
rgr-rust violations <db_path> <repo_uid>               # Boundary violation check (IMPORTS)
rgr-rust gate       <db_path> <repo_uid> [--strict | --advisory]  # CI gate (arch_violations, waivers, 3 modes)
rgr-rust dead       <db_path> <repo_uid> [kind]        # Unreferenced nodes
rgr-rust cycles     <db_path> <repo_uid>               # Module-level IMPORTS cycles
rgr-rust stats      <db_path> <repo_uid>               # Module structural metrics
```

Read-side commands (callers, callees, path, imports, violations, dead,
cycles, stats) emit a TS-compatible QueryResult JSON envelope:

```json
{
  "command": "graph <cmd>",
  "repo": "<repo name>",
  "snapshot": "<snapshot_uid>",
  "snapshot_scope": "full" | "incremental",
  "basis_commit": "<hash>" | null,
  "results": [...],
  "count": N,
  "stale": true | false
}
```

Symbol resolution (callers, callees) uses exact match only:
stable_key, then qualified_name, then name. SYMBOL kind only.

Known Rust CLI divergences from TS CLI:
- `--edge-types` on callers/callees accepts CALLS and INSTANTIATES only (TS accepts all 18 edge types)
- No `--min-lines` filter on dead
- No `--json` flag (always JSON, no table format)
- No `graph metrics` command yet
- `path` is symbol-only endpoints, CALLS+IMPORTS fixed, max-depth 8 fixed (no `--edge-types` or `--max-depth`)
- `imports` is one-hop only (no `--depth`), file paths only (no module/symbol fallback)
- `trust` command envelope does not use the QueryResult wrapper
  (trust has its own report shape, matching TS)
- `index` and `refresh` use stderr for progress, no JSON output
- `gate` waiver overlay: PASS obligations are not waivable (Rust-25
  deliberate correction — TS unconditionally marks WAIVED, Rust only
  suppresses non-PASS verdicts; see TECH-DEBT.md)

## Native dependency note

`better-sqlite3` is a native Node.js addon. The compiled binary is keyed to the **active Node ABI** at rebuild time, not to the version it was first installed with.

This repo pins Node `v22.21.1` via `.nvmrc` and `.tool-versions`. `package.json` declares `engines.node: ">=22 <23"` as the safety envelope (NODE_MODULE_VERSION 127 across all v22.x). Run `nvm use` (or your version manager equivalent) before any install or rebuild to align with the pinned version.

If you switch Node versions afterward (e.g. `nvm use 24`), the binary becomes invalid and produces `NODE_MODULE_VERSION` mismatch errors. Fix:

```
pnpm rebuild better-sqlite3
```

The rebuild only fixes the binary for whichever Node is active when you run it. Switching Node again requires another rebuild. **If the binary was built under a different Node, run `nvm use && pnpm rebuild better-sqlite3` before running tests or invoking the CLI** — otherwise the failure surfaces inside test output or `rgr` invocation, not at install time.

## Reference docs

See `docs/ROADMAP.md` for the product roadmap (what is shipped, what is next, what is deferred).
See `docs/TECH-DEBT.md` for known limitations and test-scope debt.
See `docs/cli/v1-cli.txt` for the CLI command surface.
See `docs/architecture/schema.txt` for the database schema.
See `docs/architecture/project-structure.txt` for the folder layout.
See `docs/architecture/measurement-model.txt` for the four-layer truth model.
See `docs/architecture/versioning-model.txt` for toolchain provenance.
See `docs/architecture/gate-contract.txt` for the normative gate/waiver/verdict contract.
See `docs/architecture/annotations-contract.txt` for the normative provisional-annotations contract.
See `docs/milestones/rgr-rust-structural-v1.md` for the Rust CLI structural milestone (commands, contracts, deferred items).

## Conventions

- Use `pnpm` not npm.
- All CLI output supports `--json`. Default is human-readable tables.
- Tests use fixture codebases in `test/fixtures/` with known-good expected graphs.
- No emojis in code or output.

## IMPORTANT: Use rgr For Every Structural Change

**MANDATORY.** Before modifying any file in this codebase, use `rgr` to understand what you are touching. After completing work, re-index and verify.

This is not optional. Every session that skipped rgr produced avoidable mistakes: missed callers, broken stable-key contracts, wrong file counts, undetected regressions. Every session that used rgr caught issues earlier and produced cleaner code.

**Before making changes:**
- `rgr graph callers repo-graph <symbol>` — who depends on what you're changing?
- `rgr graph callees repo-graph <symbol>` — what does it call?
- `rgr graph imports repo-graph <file>` — what does this file import?
- `rgr graph stats repo-graph` — module stability/abstraction before touching it
- `rgr trust repo-graph` — current extraction quality baseline

**After making changes:**
- `rgr repo refresh repo-graph` — re-index to pick up new code
- `rgr boundary summary repo-graph` — verify boundary facts if boundary code changed
- `rgr graph dead repo-graph --kind SYMBOL` — check for new dead symbols

**In every response, include a brief rgr usage report:**
- What queries were run
- What was learned from them
- What the alternative would have been (grep, reading files, guessing)
- Whether rgr saved time or caught something grep would have missed

Known rgr limitations on this codebase:
- Type-only imports (`import type { X }`) are invisible to the import graph
- `obj.method()` calls with unresolved receiver types show as unresolved
- Import-binding-assisted resolution helps but aliased imports are not yet resolved
- Framework-liveness inferences suppress some dead-code false positives but not all

If rgr is not on PATH, register it: `ln -sf "$(pwd)/dist/cli/index.js" /usr/local/bin/rgr`

Commands reference:

```bash
# Repository management
rgr repo add .                           # Register this repo
rgr repo index repo-graph                # Full index (<1s)
rgr repo status repo-graph               # Current snapshot, toolchain provenance
rgr repo refresh repo-graph              # Delta refresh (reuses unchanged files)
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
rgr graph obligations repo-graph         # Evaluate obligations (five-state: PASS/FAIL/MISSING/UNSUPPORTED/WAIVED)
rgr evidence repo-graph                  # Verification status report (computed + effective verdicts, waiver basis)
rgr gate repo-graph                      # CI gate: five-state verdicts, three modes, structured exit code
rgr gate repo-graph --strict             # Strict mode: exit 1 on FAIL/MISSING/UNSUPPORTED
rgr gate repo-graph --advisory           # Advisory mode: exit 1 only on FAIL
rgr trend repo-graph                     # Snapshot-to-snapshot health delta (latest vs parent)
rgr change impact repo-graph             # Modules affected by changes (reverse IMPORTS only)
rgr change impact repo-graph --staged    # Diff staged changes instead of working tree vs basis
rgr change impact repo-graph --since main --max-depth 3
rgr trust repo-graph                     # Extraction trust report: reliability levels + downgrade flags
rgr docs repo-graph .                    # Repo-level provisional annotations (README, package description)
rgr docs repo-graph src/core             # Module-level annotations via path match

# Boundary interactions (HTTP + CLI command)
rgr boundary summary repo-graph          # Aggregate counts: providers, consumers, links, match rates
rgr boundary providers repo-graph        # List boundary provider facts
rgr boundary consumers repo-graph        # List boundary consumer facts
rgr boundary links repo-graph            # List matched provider-consumer links (derived)
rgr boundary unmatched repo-graph        # Providers/consumers with no matched link
rgr boundary providers repo-graph --mechanism http --limit 20
rgr boundary providers repo-graph --mechanism cli_command
rgr boundary unmatched repo-graph --side consumers

# Quality measurements
rgr graph stats repo-graph               # Module structural metrics (fan-in/out, instability)
rgr graph metrics repo-graph --limit 10  # Top complex functions (cyclomatic complexity)
rgr graph metrics repo-graph --module    # Per-module complexity aggregates
rgr graph versions repo-graph            # Extracted domain versions (package.json)
rgr graph churn repo-graph --since 90.days.ago  # Per-file git churn
rgr graph hotspots repo-graph            # Churn x complexity ranking
rgr graph coverage repo-graph ./coverage/coverage-final.json  # Import Istanbul/c8 coverage
rgr graph risk repo-graph                # Under-tested hotspots (hotspot x coverage gap)

# Discovered modules (manifest/workspace-detected)
rgr modules list repo-graph              # Module catalog with rollups (files, symbols, evidence)
rgr modules show repo-graph <module>     # Full detail: identity, surfaces, files, topology, env+fs rollup
rgr modules evidence repo-graph <module> # Evidence items for a module
rgr modules files repo-graph <module>    # Files owned by a module

# Project surfaces (operational characterization)
rgr surfaces list repo-graph             # Surface catalog: kind, build, runtime, entrypoints, configs
rgr surfaces show repo-graph <surface>   # Full detail: module, build, runtime, config roots, entrypoints, evidence, env deps, fs mutations
rgr surfaces evidence repo-graph <surface> # Evidence items for a surface

# Declarations
rgr declare module repo-graph src/core --purpose "domain model" --maturity MATURE
rgr declare boundary repo-graph src/core --forbids src/adapters
rgr declare entrypoint repo-graph <symbol> --type public_export
rgr declare invariant repo-graph <symbol> "must not fail silently"
rgr declare requirement repo-graph src/core --req-id REQ-001 --objective "..."
rgr declare obligation repo-graph REQ-001 --obligation "..." --method arch_violations --target src/core
rgr declare waiver repo-graph REQ-001 --obligation-id <id> --requirement-version 2 --reason "..."
rgr declare list repo-graph --kind requirement
rgr declare list repo-graph --kind waiver
rgr declare remove repo-graph <uid>
```

Slash commands (`.claude/commands/`):
- `/investigate-symbol <repo> <SymbolName>` — callers, callees, dead-code, cycles
- `/repo-overview <path>` — index + structural health overview
- `/assess-health <repo>` — full quality health report (structure, complexity, hotspots, risk)
- `/verify-requirements <repo>` — evaluate requirement obligations (PASS/FAIL/MISSING)

Agent skill docs (`docs/skills/`):
- `investigate-symbol.txt` — detailed investigation workflow
- `assess-code-health.txt` — full health report workflow with all measurement steps
- `verify-requirements.txt` — requirement obligation verification workflow

rgr uses syntax-only extraction (tree-sitter) with optional compiler enrichment (`rgr enrich`). Import graphs are accurate. Call graphs resolve well on class-heavy architectures; weaker on SDK-heavy or functional patterns. Compiler enrichment resolves ~80-85% of unknown receiver types (TypeScript via TypeChecker, Rust via rust-analyzer). When callee results look incomplete, read the source directly or run `rgr enrich`.

Trust boundaries for `graph dead`:
- On clean-architecture codebases with explicit call/import graphs: high confidence.
- On registry/plugin-driven architectures (CMS renderers, extension registries, render maps, string-key dispatch): treat results as "graph orphans," not deletion candidates. These codebases wire liveness through channels the graph does not yet model.
- Spring container-managed classes (`@Component`, `@Service`, `@Repository`, `@Configuration`, `@RestController`, `@Bean`) are automatically suppressed from dead-code results via framework-liveness inferences.
- Lambda exported handler functions are suppressed via `framework_entrypoint` inferences.
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


