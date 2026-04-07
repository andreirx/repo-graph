# Roadmap

Operational product roadmap. Ordered by engineering priority, not aspiration.
See `docs/VISION.md` for long-term direction and horizon model.
See `docs/TECH-DEBT.md` for known limitations and test gaps.

## Current state (as of last commit)

- **1155 tests** across 60 test files.
- **Languages:** TypeScript/JavaScript + Rust + Java. Three-extractor indexer.
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
- **6-repo smoke set:** amodx, fraktag, glamCRM, zap-engine, zap-squad, repo-graph.
  - 2 TS-only, 2 TS+Rust, 1 TS+Java, 1 TS-only (self-index).
  - Benchmark pack: langchain4j (2641 Java files), spring-petclinic (47 Java files)
- **Non-unknown classification rates (before enrichment):**
  - TS-only repos: 53-69%
  - TS+Rust repos: 39-46% (lower due to large Rust unknown-CALLS mass)
- **Enrichment:** TS ~81%, Rust ~85% receiver-type resolution via `rgr enrich`.
- **Call-graph reliability:** all repos LOW (thresholds 50%/85%, fraktag closest at 47%).
- **Framework detection:** Express routes (50 on fraktag), Lambda handlers (93 on amodx, 20 on glamCRM).
  Spring bean liveness (93 suppressions on glamCRM).
- **Key architectural decisions made:**
  - Nearest-owning per-file context for deps, tsconfig, AND Cargo enrichment.
  - Enrichment is a separate `rgr enrich` pass, not inline with indexing.
  - Edge promotion (D2c) is opt-in via `--promote`, 8-gate safety filter.
  - Blast-radius is query-time derived, not persisted.
  - Framework-boundary detection is a post-classification pass.
  - Rust crate-internal module classification uses a distinct heuristic basis code.
  - Boundary facts stored separately from core edges table (derived links discardable).
  - `findDeadNodes` now consults framework-liveness inferences (Lambda + Spring).

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

### 1. Python extractor (syntax-first)
Python is common in data/ML pipelines, build scripts, and analysis tools.

First slice — same proven delivery pattern as TS, Rust, Java:
- tree-sitter-python grammar, Python extractor implementing ExtractorPort
- pip/pyproject.toml/setup.py/requirements.txt dependency reader
- Python builtins (builtins module, stdlib modules)
- language-aware manifest isolation in mixed repos

Do NOT drag into the first slice:
- Python semantic enrichment (pyright/mypy) — that is a separate later step
- Python framework detectors (Django, Flask, FastAPI) — follow after syntax base
- Boundary provider/consumer extraction — follow after framework detectors

### 2. Java semantic enrichment operationalization
jdtls is operational but fragile. The remaining issues are not polish:
- Cold-start/workspace reliability: jdtls Gradle import takes minutes on
  large projects. The start→query→stop model amplifies this.
- Protocol/client completeness: server→client request handling (workspace/
  configuration, client/registerCapability) is implemented but edge cases
  may remain on unfamiliar project structures.
- Operational determinism on large repos: readiness detection (ServiceReady)
  does not guarantee all project symbols are indexed. Results may vary
  between runs on the same project.

Viable improvements:
- Pre-warmed jdtls daemon (persistent background server)
- javac-based type resolution for simpler cases (no LSP overhead)
- Persistent workspace directory with validated warm-start path

### 3. Rust framework detectors
Actix-web, Axum, Rocket, Warp route handlers. Same pattern as Express
detection: post-classification pass, receiver-provenance gated. Also
enables Rust HTTP boundary provider extraction for the boundary model.

### 4. CLI boundary expansion
The `cli_command` mechanism has a first closed loop (Commander providers +
package.json script consumers + binary-prefix matching). Remaining
consumer adapters to broaden coverage:
- Shell scripts (`.sh`, `.bash`)
- Makefiles (`make` target → tool invocation)
- CI configs (`.github/workflows/*.yml`, `.gitlab-ci.yml`)
- Dockerfiles (`ENTRYPOINT`, `CMD`)
- systemd unit files (`ExecStart=`)

Also:
- Provider adapters for other CLI frameworks (yargs, clap, argparse)
- Cross-file Commander composition (commands registered via function params)
- Binary-identity verification for prefix matching (package.json bin field)

### 5. C/C++ extractor
Highest business value (Linux BSP, embedded applications). Highest
implementation cost. Requires compile_commands.json as boundary.
Do not start until Rust, Java, and Python have validated the
multi-language architecture. Needs: Clang-backed symbol extraction,
translation-unit-aware resolution, header/source ownership policy,
macro/preprocessor caveat model.

### 6. Storage/state boundary model
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
