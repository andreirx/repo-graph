# Roadmap

Operational product roadmap. Ordered by engineering priority, not aspiration.
See `docs/VISION.md` for long-term direction and horizon model.
See `docs/TECH-DEBT.md` for known limitations and test gaps.

## Current state (as of last commit)

- **972 tests** across 54 test files.
- **Languages:** TypeScript/JavaScript + Rust + Java. Three-extractor indexer.
- **Enrichment:** TS (~81%), Rust (~85%), Java (~76%). All three operational.
- **Classifier version:** 6.
  - v4: language-aware imports
  - v5: Rust crate-internal module heuristic
  - v6: hyphen-normalized Cargo deps + heuristic basis-code distinction
- **6-repo smoke set:** amodx, fraktag, glamCRM, zap-engine, zap-squad, repo-graph.
  - 2 TS-only, 2 TS+Rust, 1 TS+Java, 1 TS-only (self-index).
  - Latest assessment: `smoke-runs/2026-04-07T08-20-00Z/ASSESSMENT-from-agent.md`
  - Benchmark pack: langchain4j (2641 Java files), spring-petclinic (47 Java files)
- **Non-unknown classification rates (before enrichment):**
  - TS-only repos: 53-69%
  - TS+Rust repos: 39-46% (lower due to large Rust unknown-CALLS mass)
- **Enrichment:** TS ~81%, Rust ~85% receiver-type resolution via `rgr enrich`.
- **Call-graph reliability:** all repos LOW (thresholds 50%/85%, fraktag closest at 47%).
- **Framework detection:** Express routes (50 on fraktag), Lambda handlers (93 on amodx, 20 on glamCRM).
- **Key architectural decisions made:**
  - Nearest-owning per-file context for deps, tsconfig, AND Cargo enrichment.
  - Enrichment is a separate `rgr enrich` pass, not inline with indexing.
  - Edge promotion (D2c) is opt-in via `--promote`, 8-gate safety filter.
  - Blast-radius is query-time derived, not persisted.
  - Framework-boundary detection is a post-classification pass.
  - Rust crate-internal module classification uses a distinct heuristic basis code.

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

### Boundary interaction model (HTTP slice)
- Generic boundary-fact architecture: provider facts, consumer facts, derived links.
- Separate storage: `boundary_provider_facts`, `boundary_consumer_facts` (source of
  truth), `boundary_links` (derived artifact, discardable).
- NOT in the core `edges` table. Boundary links are protocol-level inferred facts,
  not language-level extraction edges.
- Mechanism-keyed matcher: `BoundaryMatchStrategy` interface, `HttpBoundaryMatchStrategy`
  first implementation. Strategy pattern for future gRPC, IOCTL, CLI mechanisms.
- Structured segment normalization: Spring `{id}`, Express `:id`, consumer `{param}`
  all normalize to `{_}` for matching. Raw paths preserved in facts.
- HTTP provider extractor: Spring `@GetMapping`/`@PostMapping`/etc. (PROTOTYPE, regex).
- HTTP consumer extractor: `axios.get/post/put/delete/patch`, `fetch()` (PROTOTYPE, regex).
- Indexer wiring: boundary extraction runs during indexing, facts persisted, intra-repo
  links materialized as convenience.
- CLI surface: `rgr boundary summary|providers|consumers|links|unmatched`.
- Validated on glamCRM: 97 providers, 82 consumers, 66 links, 68% provider match rate.
- Known limitation: consumer extractor misses calls where API prefix is baked into a
  base URL constant (e.g. `const BASE = VITE_API_URL + "/api/v2/users"`). Fix path:
  AST-backed extraction with constant resolution.

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

### 1. Java/Spring framework detectors
Spring annotations (@RestController, @Service, @Repository, @Autowired),
JAX-RS, servlet/container entrypoints. These support the API-boundary
provider side and improve dead-code/liveness analysis.

### 2. Boundary extractor maturation
Two sub-phases. The fact schema, matcher, storage, and CLI surfaces
are stable. The extractors are behind the `BoundaryProviderFact[]` /
`BoundaryConsumerFact[]` interface — changes are adapter-local.

#### 2A. TS consumer extractor maturation (highest ROI)
The glamCRM gap is not a parser problem — it is a value-resolution
problem. `const BASE_URL = VITE_API_URL + "/api/v2/sales-targets"`
followed by `axios.get(\`${BASE_URL}/salesperson/${id}\`)` requires
extractor-local binding analysis, not only a different parser.

Target defects:
- Base-URL constant resolution: trace `const BASE = ...` definitions,
  evaluate string concatenation, combine with template literal paths.
  Requires a small constant propagator on top of the AST.
- Simple wrapper recognition: `function apiGet(path) { return axios.get(BASE + path) }`
  — follow one level of function indirection.
- Path concatenation normalization: `BASE + "/foo"` vs template literal.

AST gets the structure to find the bindings. The constant propagator
is the actual fix.

#### 2B. Provider extractor maturation
- Spring: move from regex to tree-sitter-java AST for annotation extraction.
  More robust on multi-line annotations, `value=` vs positional, `produces`/
  `consumes` attributes.
- Express route provider: new extractor for `app.get("/api/...")`,
  `router.post(...)` patterns. Enables boundary modeling on TS-only repos
  (fraktag, amodx). Same boundary-fact model, new adapter.

### 3. CLI boundary modeling
The CLI is a real interaction boundary. Users, scripts, cron, systemd,
CI, Makefiles, and Docker entrypoints interact through it. That is not
a normal language call edge — it is a boundary interaction.

Mechanism: `cli_command` (new `BoundaryMechanism` variant).

Provider facts (command surface):
- Extracted from Commander/yargs/clap/argparse command registrations
- Operation: `rgr boundary links`, `fraktag serve`, `ip link set`
- Address: command path (program + subcommand chain)
- Metadata: flags, positional args, exit behavior

Consumer facts (invocations):
- Extracted from shell scripts, Makefiles, CI configs (.github/workflows),
  systemd unit files, Dockerfiles, package.json scripts
- Operation: the invocation command string
- Confidence: exact invocation > wrapper > interpolated

Matcher: `CliBoundaryMatchStrategy`
- Command name + subcommand path matching
- Normalized flag surface matching (future)

Why this matters:
- Many repos (Linux tools, build tools, compilers, FRAKTAG, repo-graph
  itself) have the CLI as the primary entry surface
- Enables: unused command detection, blast radius of flag changes,
  script-to-command coupling, operational usage paths
- Distinct from `declare boundary` (architecture rule) — this is a
  system interaction fact
- Reuses the full boundary-fact architecture (facts, links, matcher,
  CLI query surface) with a new mechanism

### 4. Python extractor
Python is common in data/ML pipelines, build scripts, and analysis tools.
Requires: tree-sitter-python grammar, Python extractor implementing
ExtractorPort, pip/pyproject.toml/setup.py dependency reader, Python
builtins (builtins module, stdlib modules).

### 5. Rust framework detectors
Actix-web, Axum, Rocket, Warp route handlers. Same pattern as Express
detection: post-classification pass, receiver-provenance gated.

### 6. C/C++ extractor
Highest business value (Linux BSP, embedded applications). Highest
implementation cost. Requires compile_commands.json as boundary.
Do not start until Rust, Java, and Python have validated the
multi-language architecture. Needs: Clang-backed symbol extraction,
translation-unit-aware resolution, header/source ownership policy,
macro/preprocessor caveat model.

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
