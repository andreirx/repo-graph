# repo-graph

Deterministic code-graph indexing and trust analysis for real repositories.

`repo-graph` indexes source code into a stable graph, records unresolved edges explicitly, classifies why they are unresolved, and reports how much of the graph can be trusted for downstream analysis. It is built for legacy and mixed-language codebases where "best effort" graph output is not enough.

This repository is the implementation of the engineering-intelligence substrate described in [`docs/VISION.md`](/Users/apple/Documents/APLICATII%20BIJUTERIE/repo-graph/docs/VISION.md).

## What This Tool Does

At index time, `repo-graph`:

1. Parses source files into `FILE` and `SYMBOL` nodes.
2. Emits graph edges such as `IMPORTS`, `CALLS`, `INSTANTIATES`, and `IMPLEMENTS`.
3. Preserves unresolved references instead of dropping them.
4. Resolves what can be resolved against the indexed snapshot.
5. Classifies remaining unresolved edges into semantic buckets:
   - `external_library_candidate`
   - `internal_candidate`
   - `framework_boundary_candidate`
   - `unknown`
6. Computes trust and reliability surfaces over the resulting snapshot.

After indexing, `repo-graph` can:

- query callers, callees, paths, cycles, dead nodes, hotspots, and versions
- explain unresolved edges with category and basis codes
- attach human declarations such as boundaries, entrypoints, and requirements
- evaluate verification obligations and CI gate outcomes
- enrich unresolved receiver types with language-specific semantic engines
- promote a narrow, audited subset of enriched unresolved calls into inferred resolved edges

## Why It Exists

Most static graph tools flatten uncertainty into one of two wrong outcomes:

- pretend unresolved edges do not matter
- mark everything as equally untrustworthy

`repo-graph` does neither.

It treats unresolved structure as first-class data. That allows:

- explicit trust reporting instead of hidden confidence
- targeted improvement work instead of vague "coverage"
- stable JSON surfaces for agents and automation
- support for declaration overlays without corrupting computed truth

## Product Direction

Repo-graph is not a chat layer and not a generic code indexer.

Its role is to be a deterministic system of record for engineering intelligence:

- structural code facts
- architectural intent
- requirements and constraints
- verification obligations
- evidence and waivers
- versioned process state

That boundary matters. The graph is only one layer. The long-term product is the queryable substrate that links intent, code, evidence, and policy without erasing uncertainty.

## Core Model

The system separates three things that many tools collapse together:

1. **Resolved graph facts**
   - nodes and edges that are known inside the indexed snapshot

2. **Unresolved observations**
   - references the extractor saw but could not map to a concrete graph target

3. **Interpretation layers**
   - classification
   - blast-radius
   - framework facts
   - compiler enrichment
   - declarations and policy overlays

That separation is the basis for the trust surface.

## Language Adapters

This repository contains language-specific adapters behind a shared extractor contract.

Implemented adapters:

- TypeScript / JavaScript
- Rust
- Java
- Python
- C / C++

### TypeScript / JavaScript adapter

Implemented:
- `web-tree-sitter` extractor
- nearest-owning `package.json` dependency context
- nearest-owning `tsconfig.json` alias context with relative `extends`
- runtime builtins and Node stdlib classification
- TypeScript compiler side-channel for receiver-type enrichment
- Express route provider extraction (boundary facts)
- HTTP client consumer extraction (axios/fetch, with file-local string resolution)
- Commander CLI command surface extraction (boundary facts)
- package.json script consumer extraction (CLI boundary facts)

### Rust adapter

Implemented:
- `tree-sitter-rust` extractor
- `Cargo.toml` dependency reader
- Rust stdlib/runtime module classification (`std`, `core`, `alloc`)
- language-aware manifest isolation in mixed TS/Rust repos
- rust-analyzer side-channel for receiver-type enrichment

### Java adapter

Implemented:
- `tree-sitter-java` extractor
- Gradle dependency reader (Groovy + Kotlin DSL)
- overload disambiguation via parameter type signatures in stable keys
- language-aware manifest isolation in mixed TS/Java repos
- Spring MVC route provider extraction (AST-backed, boundary facts)
- Spring container-managed bean detection (`@Component`, `@Service`, `@Repository`, `@Configuration`, `@RestController`, `@Bean`)
- jdtls (Eclipse JDT Language Server) side-channel for receiver-type enrichment (operational, fragile)

### Python adapter

Implemented:
- `tree-sitter-python` extractor
- `pyproject.toml` / `requirements.txt` dependency reader
- Python builtins and stdlib module classification
- language-aware manifest isolation in mixed repos
- Pytest test/fixture detection (dead-code suppression)

### C / C++ adapter

Implemented:
- `tree-sitter-c` + `tree-sitter-cpp` dual-grammar extractor
- Functions, structs, classes, typedefs, enums, namespaces, methods, constructors
- `#include` directives (system vs local classification)
- Preprocessor guard recursion (`#ifndef`/`#ifdef` blocks)
- STL qualified calls (`std::sort`, `std::make_unique`)
- C/C++ runtime builtins (C stdlib, POSIX headers, C++ STL)
- Cyclomatic complexity, parameter count, nesting depth
- Large-file guard (files > 1MB skipped)

Not yet implemented:
- `compile_commands.json` integration
- Header search path resolution
- Clangd/libclang semantic enrichment
- Macro expansion, template instantiation

## Architecture

This repository follows a Clean Architecture split.

Top-level structure:

- `src/core`
  - business rules, graph model, trust logic, classification logic, gate/evidence logic
- `src/adapters`
  - concrete mechanisms: extractors, storage, config readers, enrichment adapters, git, importers
- `src/cli`
  - orchestration and presentation
- `src/main.ts`
  - composition root; the only place that wires concrete adapters together

Important architectural rules:

- extractors emit unresolved symbolic edges, never direct database writes
- the indexer resolves and classifies after full-file inventory is known
- declarations and annotations are separate from computed graph truth
- enrichment is a post-index side-channel, not the base extractor
- graph mutation from enrichment is opt-in and restricted to a narrow safe subset

## Storage

Default database location:

```text
~/.rgr/repo-graph.db
```

Override with:

```bash
RGR_DB_PATH=/absolute/path/to/repo-graph.db
```

SQLite is the system of record for:

- repositories
- snapshots
- files
- nodes
- resolved edges
- unresolved edges and their classification
- declarations
- inferences
- measurements
- boundary provider facts (source of truth)
- boundary consumer facts (source of truth)
- boundary links (derived, discardable)

## Installation

Requirements:

- Node.js 20+
- `pnpm`

Base install:

```bash
pnpm install
pnpm rebuild better-sqlite3
pnpm build
```

Optional but required for semantic enrichment:

- TypeScript enrichment: `typescript` package is already a dependency
- Rust enrichment: `rust-analyzer` must be on `PATH`
- Java enrichment: `jdtls` (Eclipse JDT Language Server) must be on `PATH`

To rebuild WASM grammars (required after grammar package updates):

```bash
pnpm rebuild tree-sitter-cli
pnpm run build:grammars
```

Rust analyzer installation example:

```bash
rustup component add rust-analyzer
```

## Quick Start

Register a repository:

```bash
rgr repo add ../some-repo --name some-repo
```

Index it:

```bash
rgr repo index some-repo
```

Inspect trust:

```bash
rgr trust some-repo
rgr trust unresolved-samples some-repo --limit 20
```

Inspect graph structure:

```bash
rgr graph callers some-repo "SomeClass.someMethod"
rgr graph callees some-repo "SomeClass.someMethod"
rgr graph cycles some-repo
rgr graph dead some-repo
```

Inspect boundary interactions:

```bash
rgr boundary summary some-repo
rgr boundary links some-repo
rgr boundary unmatched some-repo
```

Run semantic enrichment:

```bash
rgr enrich some-repo
```

Promote the narrow safe subset of enriched unresolved calls:

```bash
rgr enrich some-repo --promote
```

Declare known architectural facts:

```bash
rgr declare entrypoint some-repo "src/server.ts:start"
rgr declare boundary some-repo "src/app" --forbids "src/infrastructure"
```

Evaluate verification evidence and CI gate state:

```bash
rgr evidence some-repo
rgr gate some-repo
```

## Command Families

The CLI is organized by responsibility:

- `repo`
  - register repositories, index snapshots, inspect repo status
- `graph`
  - structural queries over callers, callees, paths, cycles, dead code, metrics, versions, hotspots
- `trust`
  - reliability report and unresolved-edge samples
- `enrich`
  - semantic receiver-type enrichment and safe promotion
- `declare`
  - human declarations for modules, boundaries, entrypoints, requirements, waivers
- `arch`
  - architecture boundary violation queries
- `change`
  - change-impact analysis
- `evidence`
  - obligation and evidence closure report
- `gate`
  - CI reduction over obligations and waivers
- `trend`
  - snapshot-to-snapshot health comparison
- `boundary`
  - boundary interaction facts and links (HTTP routes, CLI commands, package.json scripts)
- `docs`
  - read-only provisional annotation surface

Use `rgr --help` and `rgr <command> --help` for the exact current surface.

## Trust Model

The trust system exists because an indexed graph is not automatically a trustworthy graph.

`rgr trust` reports:

- unresolved category counts
- classification bucket counts
- call-graph reliability
- downgrade triggers
- unknown-call blast-radius breakdown
- compiler enrichment coverage

Current classifier buckets:

- `external_library_candidate`
  - unresolved edge likely points outside the repo or to runtime facilities
- `internal_candidate`
  - unresolved edge likely points to repo-internal code or files
- `framework_boundary_candidate`
  - unresolved edge matches a known runtime registration/boundary pattern
- `unknown`
  - no defensible supporting signal yet

This is the central distinction in the product: unknowns are surfaced, not erased.

## Declarations And Policy Overlays

Computed truth and human-authored truth are deliberately separate.

Declarations include:

- module metadata
- ownership
- maturity
- forbidden boundaries
- entrypoints
- requirements
- waivers

These declarations do not rewrite the raw graph. They feed read surfaces, evidence evaluation, and policy reduction.

## Enrichment And Promotion

`rgr enrich` is a post-index semantic pass.

Current behavior:

- TypeScript / JavaScript files are enriched through the TypeScript compiler
- Rust files are enriched through rust-analyzer
- Java files are enriched through jdtls (Eclipse JDT Language Server)
- Python and C/C++ enrichment not yet implemented (planned: pyright, clangd)
- enrichment writes receiver-type metadata onto unresolved edges
- failed enrichment is recorded explicitly
- the original unresolved edge remains for auditability

`--promote` is intentionally narrow:

- only compiler-derived enrichment is eligible
- only internal, concrete, unambiguous class-method targets are promotable
- promoted edges are inserted as inferred resolved graph edges
- the original unresolved row is preserved

This is not a generic speculative resolver. It is a constrained graph-improvement path.

## Boundary Interaction Model

`repo-graph` models cross-boundary interactions that the normal call graph cannot capture: HTTP requests, CLI command invocations, and (planned) gRPC, IPC, and other protocol-level links.

Boundary facts are stored separately from the core graph edges table. They are protocol-level inferred observations, not language-level extraction edges.

Two sides:
- **Provider facts** — something exposes an entry surface (Spring route, Express route, Commander command)
- **Consumer facts** — something reaches across the boundary (axios/fetch call, package.json script invocation)

A mechanism-keyed matcher pairs providers and consumers into derived links. These links are discardable convenience artifacts that can be recomputed from raw facts.

Current mechanisms:
- `http` — Spring MVC routes (AST-backed), Express routes, axios/fetch calls
- `cli_command` — Commander.js command registrations, package.json script invocations

Query boundary data:

```bash
rgr boundary summary some-repo
rgr boundary providers some-repo --mechanism http
rgr boundary consumers some-repo --mechanism cli_command
rgr boundary links some-repo
rgr boundary unmatched some-repo
```

## Framework Detection

Framework-liveness inferences suppress false dead-code reports for symbols that are live by container convention rather than by explicit caller edges.

Current detectors:
- **Spring beans**: `@Component`, `@Service`, `@Repository`, `@Configuration`, `@RestController`, `@Controller`, `@Bean` factory methods. Emitted as `spring_container_managed` inferences.
- **Lambda handlers**: exported functions matching Lambda naming conventions in files importing AWS Lambda types. Emitted as `framework_entrypoint` inferences.
- **Express routes**: edge-level classification of `app.get()` / `router.post()` as framework boundary registration.

`graph dead` automatically excludes nodes with framework-liveness inferences.

## Mixed-Language Repositories

The indexer is language-aware.

Current behavior in mixed-language repositories:

- files are routed to extractors by extension:
  - `.ts`/`.tsx`/`.js`/`.jsx` → TypeScript
  - `.rs` → Rust
  - `.java` → Java
  - `.py` → Python
  - `.c`/`.h` → C, `.cpp`/`.hpp`/`.cc`/`.cxx` → C++
- TypeScript files resolve dependency context from nearest `package.json`
- Rust files resolve dependency context from nearest `Cargo.toml`
- Java files resolve dependency context from nearest `build.gradle`
- Python files resolve dependency context from nearest `pyproject.toml` or `requirements.txt`
- C/C++ files: quoted `#include` classified as internal, angle-bracket as system
- runtime builtins from all registered extractors are merged into classifier signals
- boundary fact extraction runs on all applicable files regardless of language

This prevents Node dependency context from contaminating Rust or Java files and vice versa.

## Development

Build:

```bash
pnpm build
```

Run tests:

```bash
pnpm rebuild better-sqlite3
pnpm test
```

Lint:

```bash
pnpm lint
```

Format:

```bash
pnpm format
```

## Smoke Runs

The repository keeps local smoke-run assessment artifacts under [`smoke-runs/README.md`](/Users/apple/Documents/APLICATII%20BIJUTERIE/repo-graph/smoke-runs/README.md).

Those runs exist to:

- compare tool behavior across real repos
- calibrate trust surfaces
- catch regression between semantic versions of the classifier and indexer

They are not part of the shipped runtime. They are field-calibration evidence.

## License

MIT
