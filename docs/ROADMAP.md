# Roadmap

Operational product roadmap. Ordered by engineering priority, not aspiration.
See `docs/VISION.md` for long-term direction and horizon model.
See `docs/TECH-DEBT.md` for known limitations and test gaps.

## Current state (as of last commit)

- **1341 tests** across 71 test files.
- **Languages:** TypeScript/JavaScript + Rust + Java + Python + C/C++. Five-extractor indexer.
- **Enrichment:** TS (~81%), Rust (~85%), Java (operational but fragile). All three wired.
- **Classifier version:** 6.
  - v4: language-aware imports
  - v5: Rust crate-internal module heuristic
  - v6: hyphen-normalized Cargo deps + heuristic basis-code distinction
- **Boundary interaction model:** HTTP slice shipped (Spring + Express providers, TS consumers).
  - glamCRM: 97 providers, 85 consumers, 80 links (82.5% provider, 94.1% consumer match rate)
  - fraktag: 47 providers, 42 consumers, 41 links (70.2% provider, 97.6% consumer match rate)
- **Spring framework detectors:** container-managed bean liveness via `@Component`, `@Service`,
  `@Repository`, `@Configuration`, `@RestController`, `@Controller`, `@Bean`. Suppresses 93
  false dead-code reports on glamCRM. Also fixes Lambda entrypoint suppression in `findDeadNodes`.
- **Imported free-function call resolution:** TS/JS import-binding-assisted call
  resolution. repo-graph call_resolution_rate improved from ~15% to 33%.
- **Repo set:** amodx, fraktag, glamCRM, repo-graph, mempalace, glam-scrapers,
  unelte, swupdate, buildroot, C++11 Deep Dives.
  - TS-only, TS+Rust, TS+Java, Python, C/C++, mixed multi-language.
  - C/C++ validated on swupdate (208 files, 3422 nodes) and buildroot (645 files, 5249 nodes).
  - Linux kernel registered but exceeds current indexer memory model (63k C files).
- **Framework detection:** Express routes, Lambda handlers, Spring beans, pytest tests/fixtures.
  `findDeadNodes` consults all framework-liveness inferences.
- **CLI boundary model:** HTTP + cli_command mechanisms. Commander providers, package.json
  script consumers, shell script consumers, Makefile recipe consumers. Binary-prefix matching.
- **Key architectural decisions made:**
  - Nearest-owning per-file context for deps, tsconfig, Cargo, Gradle, pyproject.toml.
  - Enrichment is a separate `rgr enrich` pass, not inline with indexing.
  - Edge promotion (D2c) is opt-in via `--promote`, 8-gate safety filter.
  - Boundary facts stored separately from core edges table (derived links discardable).
  - Large-file guard: files > 1MB skipped during indexing (operational containment).
  - Imported free-function calls resolved via import bindings (no compiler needed).

## Shipped

### Multi-language graph engine
- TypeScript/JavaScript extractor (tree-sitter, syntax-only)
- Rust extractor (tree-sitter-rust)
- Java extractor (tree-sitter-java)
- Multi-extractor indexer: routes files by extension
- Language-aware manifest isolation (.rs → Cargo.toml, .java → build.gradle, .ts → package.json)
- Java overload disambiguation via parameter type signatures in stable keys
- Gradle dependency reader (Groovy + Kotlin DSL) with 2-segment prefix heuristic

### Unresolved-edge classification (classifier v6)
- 4-bucket vocabulary: external_library_candidate, internal_candidate,
  framework_boundary_candidate, unknown
- Per-file signals: import bindings, same-file symbols (subtype-aware),
  package dependencies (nearest-owning), tsconfig aliases (nearest-owning
  with extends)
- Runtime builtins: ES/Node/browser globals, Node stdlib, Rust std/core/alloc
- Language-aware import classification: relative, package dep, runtime stdlib,
  project alias, Rust crate-internal heuristic, unknown
- Hyphen-normalized Cargo dep matching (my-crate ↔ my_crate)
- Basis codes: per-rule audit trail on every classified edge, with distinct
  heuristic vs definite basis for Rust internal imports

### Trust reporting
- `rgr trust <repo>`: reliability axes, downgrade triggers, category/classification
  counts, blast-radius breakdown, enrichment status
- `rgr trust unresolved-samples <repo>`: per-edge samples with source file, line,
  classification, basis, blast-radius, receiver type (when enriched)
- Variant A call-graph reweighting: externals excluded from denominator

### Framework detection
- Express route/middleware registration (edge-level, receiver-provenance gated)
- Lambda exported handler entrypoints (node-level, inference-based)
- Spring container-managed bean detection: `@Component`, `@Service`, `@Repository`,
  `@Configuration`, `@RestController`, `@Controller`, `@Bean` factory methods.
  Emitted as inferences (kind: `spring_container_managed`). Suppresses false
  dead-code reports via `findDeadNodes` inference exclusion.
- `findDeadNodes` now consults framework-liveness inferences (both `framework_entrypoint`
  and `spring_container_managed`), not just entrypoint declarations. This also fixes
  the pre-existing Lambda entrypoint gap where detected entrypoints were persisted but
  not consumed by dead-code analysis.

### Boundary interaction model (HTTP slice — mature)
- Generic boundary-fact architecture: provider facts, consumer facts, derived links.
- Separate storage: `boundary_provider_facts`, `boundary_consumer_facts` (source of
  truth), `boundary_links` (derived artifact, discardable).
- NOT in the core `edges` table. Boundary links are protocol-level inferred facts,
  not language-level extraction edges.
- Mechanism-keyed matcher: `BoundaryMatchStrategy` interface, `HttpBoundaryMatchStrategy`
  first implementation. Strategy pattern for future gRPC, IOCTL, CLI mechanisms.
  Link candidates carry stable persisted fact UIDs, not object references.
- Structured segment normalization: Spring `{id}`, Express `:id`, consumer `{param}`
  all normalize to `{_}` for matching. Raw paths preserved in facts.
- HTTP provider extractors:
  - Spring: AST-backed via tree-sitter-java (MATURE). Handles multiline annotations,
    `value=`/`path=` attributes, `method=RequestMethod.X`. Known double-parse
    inefficiency (Java extractor does not expose parse tree to route extractor).
  - Express: regex-based (PROTOTYPE). Receiver provenance gated to `app`, `router`,
    `server`. Express import gate prevents false positives. Consumes FileLocalStringResolver
    for constant-backed route paths.
- HTTP consumer extractor: `axios.get/post/put/delete/patch`, `fetch()` (PROTOTYPE,
  regex). Supports bare identifier URL arguments resolved via binding table.
- FileLocalStringResolver: reusable support module for file-local constant string
  propagation. Resolves `const`, template literals, binary `+`, chained bindings,
  env-var prefix stripping. Used by both consumer and Express provider extractors.
- Indexer wiring: boundary extraction runs during indexing, facts persisted, intra-repo
  links materialized as convenience.
- CLI surface: `rgr boundary summary|providers|consumers|links|unmatched`.
- Validated on glamCRM: 97 providers, 85 consumers, 80 links, 82.5%/94.1% match rates.
- Validated on fraktag: 47 providers, 42 consumers, 41 links, 70.2%/97.6% match rates.

### CLI boundary model (first slice)
- Mechanism: `cli_command` — second boundary mechanism after HTTP.
- Commander.js provider extractor: command registrations → provider facts. Handles
  nested command composition, positional args, options, descriptions. Import gate +
  receiver-variable tracking for parent-child command path composition.
- package.json script consumer extractor: direct tool invocations → consumer facts.
  Shell chain splitting (`&&`, `||`, `;`), npx/pnpm-exec/yarn-exec unwrapping,
  script-indirection filtering (npm run/pnpm run skipped), shell-builtin filtering.
- `CliBoundaryMatchStrategy`: exact command-path matching + guarded binary-prefix
  heuristic (strips leading binary from 3+ token consumer paths). Guard prevents
  false positives where 2-word external tool invocations (vite build, cargo test)
  would match single-word internal commands.
- Validated on repo-graph: 80 CLI providers, 15 CLI consumers, 9 CLI links.
- Known limitations: cross-file Commander composition (commands registered via function
  params from different modules), aliased receivers (const api = express.Router()),
  binary-prefix matching requires 3+ tokens.

### Python extractor (syntax-plus-dependency-context baseline)
- tree-sitter-python extractor: functions, classes, methods, constructors,
  variables, imports, calls, cyclomatic complexity, nesting depth.
- Python dependency reader: pyproject.toml ([project].dependencies +
  optional-dependencies) and requirements.txt. PEP 508 parsing.
- Python runtime builtins: 60+ identifiers + 80+ stdlib modules.
- Language-aware manifest isolation: .py → pyproject.toml/requirements.txt.
- Classifier integration: stdlib imports classified as external via runtime
  builtins, pip deps classified via specifier_matches_package_dependency,
  relative imports (.utils) classified as internal.
- Pytest detector: test_* functions, Test* classes, @pytest.fixture.
  Emitted as pytest_test/pytest_fixture inferences. Suppresses test code
  from dead-code reports.
- Grammar build infrastructure: scripts/build-grammars.mjs for reproducible
  WASM grammar builds from npm packages.
- Validated on: mempalace (30 files, 396 nodes), glam-scrapers, unelte,
  swupdate (18 Python files + shell scripts).
- Known limitation: Python package names do not map 1:1 to import specifiers
  (pyyaml → import yaml, beautifulsoup4 → import bs4). Exact name matches
  work; mismatches remain unclassified.

### CLI boundary expansion (shell scripts)
- Shell script consumer extractor: .sh/.bash files → cli_command consumer facts.
  Line-based conservative extraction with heredoc detection, continuation
  line skipping, control flow skipping, pipe exclusion, env-prefix stripping.
- Shared cli-invocation-parser support module: chain splitting, wrapper
  unwrapping, builtin filtering, command parsing. Reused by both package.json
  and shell script consumers.
- Validated on: swupdate (66 shell consumers from CI/build scripts),
  glamCRM (15 shell consumers from deploy/setup scripts).

### Makefile CLI consumer extraction
- Makefile recipe-line consumer extractor: tab-detection, @/-/+ prefix stripping,
  $(Q) quiet-prefix handling, $(VAR) expansion skipping, quoted-$(VAR) filtering,
  Make directive filtering. Reuses shared cli-invocation-parser.
- Validated on: buildroot (3410 Makefiles → 1551 CLI consumers), swupdate.

### Imported free-function call resolution (TS/JS)
- Import-binding-assisted call resolution for bare-identifier CALLS edges.
- When global name lookup is ambiguous (multiple same-name functions across files),
  import bindings disambiguate by narrowing to the imported source file.
- repo-graph call_resolution_rate: ~15% → 33%. Resolved calls: ~300 → 1004.
- No compiler enrichment needed — uses existing import bindings from extractors.
- Known limitation: aliased imports (`import { foo as bar }`) not yet resolved.

### C/C++ extractor (syntax-only first slice)
- tree-sitter-c + tree-sitter-cpp grammars (WASM, built via build-grammars.mjs).
- Extracts: functions, structs, classes, typedefs, enums, namespaces, methods,
  constructors, #include directives, CALLS edges, cyclomatic complexity.
- Dual grammar: .c/.h use C grammar, .cpp/.hpp/.cc/.cxx use C++ grammar.
- Preprocessor guard recursion: symbols inside #ifndef/#ifdef blocks are extracted.
- Stable-key dedup: typedef+struct and #ifdef branch collisions handled.
- Quoted #include classified as internal (rawPath metadata with "./" prefix).
- STL qualified calls (std::sort, std::make_unique) extracted and pinned.
- Large-file guard: files > 1MB skipped (operational containment for generated headers).
- Validated on: swupdate (208 files, 3422 nodes), buildroot (645 files, 5249 nodes),
  C++11 Deep Dives (165 files, 829 nodes).
- Linux kernel: 63k C files exceeds current all-in-memory indexer architecture.
  Requires batch-persistence redesign (see Next).

### Compiler enrichment
- `rgr enrich <repo>`: post-index receiver-type resolution
- TypeScript: via `ts.Program` / `TypeChecker` (~81% enrichment rate)
- Rust: via rust-analyzer LSP subprocess (~85% enrichment rate)
- Java: via Eclipse JDT Language Server (~76% enrichment rate on glamCRM)
- Per-project-context routing: one language server per nearest build manifest
- Safe-subset edge promotion (`--promote`): 8-gate filter, inferred resolution

### Blast-radius axis
- Query-time derived: receiver origin + enclosing scope significance
- Scoped to unknown CALLS (low = private scope, medium = exported scope)

## Next (in priority order)

### 1. Large-repo indexing architecture
The current indexer reads all files into memory, accumulates all nodes
and edges, then persists. This works for repos up to ~1000 files but
fails on Linux-scale repos (63k C files).

Required changes:
- Remove `fileContents` all-in-memory map
- Persist nodes/edges in batches during extraction, not after
- Bounded in-memory resolution windows (chunked resolution)
- Optional skip policy for generated/preprocessed files (current 1MB
  guard is containment, not the fix)

This is the primary architectural blocker for Linux, large BSP repos,
and monorepos with thousands of source files.

### 2. C/C++ maturation
The syntax-only first slice is shipped. Next maturation steps:
- `compile_commands.json` integration for include path resolution
- Header/source ownership: which .h belongs to which translation unit
- Clangd/libclang enrichment for receiver-type resolution (same
  architectural pattern as TS TypeChecker / Rust rust-analyzer)
- #include resolution against actual file paths (not just specifiers)

System/framework detectors:
- Linux kernel module patterns (module_init, platform_driver)
- Driver registration patterns
- RTOS task/thread registration (FreeRTOS, Zephyr)
- IOCTL/shared-memory boundary extraction

### 3. Dead-code confidence stratification
Stop presenting every zero-caller symbol as equally dead. Add evidence
quality to dead-symbol reports.

Labels:
- `dead_high_confidence` — zero callers, zero imports, no unresolved pressure
- `dead_low_confidence` — zero resolved callers, but imported in files with
  high unresolved CALLS mass
- `imported_but_call_unresolved` — symbol is imported elsewhere but the
  call resolution did not connect

### 4. CLI boundary expansion (remaining)
Shell and Makefile consumer extraction shipped. Remaining adapters:
- CI configs (`.github/workflows/*.yml`, `.gitlab-ci.yml`)
- Dockerfiles (`ENTRYPOINT`, `CMD`) — deferred further

Also:
- Provider adapters for other CLI frameworks (yargs, clap, argparse)
- Cross-file Commander composition (commands registered via function params)
- Binary-identity verification for prefix matching (package.json bin field)
- Barrel-cycle normalization: separate export-only/barrel cycles from
  logic cycles in cycle reporting

### 5. Trust overlay on structural queries
Surface evidence quality on query output. Examples:
- "0 callers, low confidence: module has 4042 unresolved CALLS"
- "dead symbol candidate, low confidence: imported in 2 files but call
  target unresolved"
Turns raw graph limitations into explicit epistemic status.

### 6. Rust framework detectors
Actix-web, Axum, Rocket, Warp route handlers. Same pattern as Express
detection: post-classification pass, receiver-provenance gated. Also
enables Rust HTTP boundary provider extraction for the boundary model.

### 7. Java semantic enrichment operationalization
jdtls is operational but fragile. The remaining issues are not polish:
- Cold-start/workspace reliability
- Protocol/client completeness
- Operational determinism on large repos

### 8. Storage/state boundary model
A component interacts with persisted or semi-persisted state through
a storage mechanism. This is a STATE boundary, distinct from the
interaction boundaries (HTTP, CLI) already modeled. Agents can grep
for individual file reads or SQL queries; they cannot cheaply maintain
normalized resource identities, read/write directionality, confidence,
cross-language storage coupling, or historic repeatability across
many queries. The indexer can.

Mechanisms (new `BoundaryMechanism` variants):
- `filesystem` — file read/write/append/delete
- `database_sql` — SQL table read/write/migrate (SQLite, Postgres, MySQL)
- `database_nosql` — document/collection access (MongoDB, DynamoDB)
- `cache` — key/value read/write (Redis, Memcached)
- `object_store` — bucket/object read/write (S3, GCS, Azure Blob)

Roles (richer than HTTP/CLI provider/consumer):
- resource definition / schema owner
- reader
- writer
- migrator

Provider/resource facts:
- file path or path pattern
- DB table / collection / key prefix / bucket prefix
- schema/migration identifier
- ownership module
- declared resource kind

Consumer interaction facts:
- operation: read / write / append / delete / migrate / enumerate
- resource address: normalized path/table/key
- confidence: literal path > resolved constant > pattern > dynamic

Why a separate mechanism family, not an extension of HTTP/CLI:
- HTTP/CLI are interaction boundaries (request/response, command/exit)
- storage is a state boundary (read/write to persisted data)
- the semantics are different: directionality (read vs write),
  schema coupling, migration lineage, orphaned resource detection
- flattening them would lose the operational distinction

Best first slice:
- Filesystem literal-path reads/writes in TS/JS
  (`readFile`, `writeFile`, `open`, `createReadStream`, etc.)
- Only literal or file-local-resolved paths (reuse FileLocalStringResolver)
- Classify operation: read/write/append/delete
- Higher-value but harder: SQL table access from literal query strings

What this unlocks:
- which modules read vs write which resources
- which storage resources are shared across components
- which writes have no known readers
- which reads depend on external state no in-repo writer maintains
- which changes affect persistence contracts or operational data flow
- which repos/components couple through filesystem, DB, cache, or
  object-store boundaries

Design constraint: do not start this before the current boundary model
(HTTP + CLI) is mature. The fact table schema supports it already
(mechanism is extensible). The matcher strategy pattern supports it.
The CLI query surfaces support it. The new work is extraction adapters
and a resource-centric (not call-centric) matching model.

## Deferred

### Entrypoint declarations (operational)
`rgr declare entrypoint` on the 6 smoke repos would lift dead-code
reliability out of LOW. Not a code change — an adoption step. Best
done when a team starts using rgr operationally on a repo.

### Trust-score reweighting (policy)
Call-graph reliability thresholds (50%/85%) have not been changed
since the denominator was corrected. May need recalibration after
enrichment and framework detection have stabilized across languages.

### D2c expansion: this.x.method() with field-type binding
The `not_simple_receiver_method` skip bucket (301 edges) is the
largest remaining promotable frontier. Requires class field type
resolution in the promotion gate.

### tsconfig package-name extends
`extends: "@tsconfig/node18"` requires node_modules lookup.
Near-zero impact on current repo set.

### Full TS semantic extraction (Path C broadening)
Replace tree-sitter for TS with Program/TypeChecker for all symbol
extraction. Largest architectural investment. Only justified after
the current syntax-first + classifier + enrichment model has been
pushed to its natural ceiling.

### Python semantic enrichment
Equivalent of TS TypeChecker / Rust rust-analyzer / Java jdtls for Python.
pyright/mypy for type inference. Follows the same enrichment adapter pattern.

### Go extractor
Useful, but lower priority than Python and C/C++ given the current repo set.
Add only after the primary language stack is mature:
TypeScript/JavaScript + Rust + Python + C/C++.

Why later:
- Current demand is exploratory rather than product-critical.
- It should not displace Python breadth or the C/C++ systems track.
- When added, it will reuse the shared scaffolding:
  multi-extractor indexer, manifest routing, trust/reporting, and the
  boundary-interaction model. Semantic enrichment would likely use `gopls`.
