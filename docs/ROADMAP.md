# Roadmap

Operational product roadmap. Ordered by engineering priority, not aspiration.
See `docs/VISION.md` for long-term direction and horizon model.
See `docs/TECH-DEBT.md` for known limitations and test gaps.

## Current state (as of last commit)

- **841 tests** across 48 test files.
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

### 1. API-boundary extraction (cross-language REST linkage)
First-class modeling of frontend↔backend REST/HTTP relationships
in mixed-language monorepos. NOT a normal call-graph edge — this is
a protocol-boundary edge with its own confidence model.

Architecture:
- Facts are the source of truth. Persist provider/consumer boundary
  facts first; links are derived artifacts.
- Matching should be facts-first, hybrid delivery:
  - raw facts persist regardless
  - intra-repo links may be materialized at index time as a convenience
  - cross-repo matching remains a separate command/path
- Design the core model generically for future mechanisms:
  HTTP, gRPC/RPC, IPC/shared-memory, IOCTLs, queues, events, sockets.

Support module:
- HTTP server-route extractor: `@GetMapping("/api/orders")`,
  Express `app.get("/api/...")`, Lambda API Gateway routes. Emits:
  method, path template, handler symbol, framework evidence.
- HTTP client-request extractor: `fetch("/api/orders")`, axios
  wrappers, RTK Query / TanStack Query patterns. Emits normalized
  facts: method, path template, source file, caller symbol.

Feature implementation:
- Cross-boundary matcher: creates protocol-boundary links from client
  request sites to server route handlers.
- Trust surface: "frontend module X talks to backend controller Y",
  "this route has no client callers in the monorepo", "this client
  endpoint has no matching server route."
- Confidence: heuristic path/method matching vs exact (generated
  client, OpenAPI spec, shared type contract).
- Execution order:
  1. server-route/provider facts
  2. client-request/consumer facts
  3. matcher + persistence + surfaces

Why this is next:
- glamCRM is a Spring Boot + React monorepo. The current model
  indexes both sides but cannot prove REST linkage end-to-end.
- This is the highest-value cross-language feature for real-world
  monorepos.
- It aligns with the product vision: deterministic discovery of
  relationships agents cannot cheaply extract with grep.

### 2. Java/Spring framework detectors
Spring annotations (@RestController, @Service, @Repository, @Autowired),
JAX-RS, servlet/container entrypoints. These support the API-boundary
provider side and improve dead-code/liveness analysis.

### 3. Python extractor
Python is common in data/ML pipelines, build scripts, and analysis tools.
Requires: tree-sitter-python grammar, Python extractor implementing
ExtractorPort, pip/pyproject.toml/setup.py dependency reader, Python
builtins (builtins module, stdlib modules).

### 4. Rust framework detectors
Actix-web, Axum, Rocket, Warp route handlers. Same pattern as Express
detection: post-classification pass, receiver-provenance gated.

### 5. C/C++ extractor
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
