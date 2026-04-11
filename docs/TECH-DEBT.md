# Technical Debt and Known Limitations

## Extraction — TypeScript

- Call graph resolution: 33% on self-index with import-binding-assisted resolution
  (up from ~15% syntax-only). Strong on class-heavy architectures, weak on
  SDK-heavy/functional patterns. Compiler enrichment (`rgr enrich`) resolves
  ~81% of remaining unknown receiver types via TypeChecker.
- **Imported free-function call resolution:** bare-identifier calls that match an
  import binding are resolved using the binding's source module. Disambiguates
  when the same function name exists in multiple files.
- **Aliased imports not yet resolved:** `import { foo as bar }` — the binding
  stores the local name (`bar`) but not the original name (`foo`). The resolver
  looks for `bar` in the target file, which finds nothing. Requires storing the
  original name in ImportBinding.
- **Namespace imports not resolved:** `import * as ns from "./m"; ns.foo()` —
  the callee is `ns.foo`, which includes the namespace alias. Not handled.
- Inherited method resolution: 11 remaining cases on FRAKTAG (diminishing returns).
- External SDK types (node_modules): not indexed.
- Destructured bindings, reassignment: not tracked.

## Extraction — Rust

- Rust extractor indexes: structs, enums, traits, impl methods, functions,
  constants, statics, type aliases. `use` declarations produce IMPORTS edges +
  import bindings. Method/function calls produce CALLS edges.
- `#[cfg(...)]` conditional duplicates deduplicated (first emission wins).
- Compiler enrichment via rust-analyzer LSP resolves ~85% of unknown receiver types.
- **Crate-internal module heuristic is not infallible:** `use renderer::Camera`
  classified as `internal_candidate` via `RUST_CRATE_INTERNAL_MODULE_HEURISTIC`.
  A mistyped or undeclared external crate with a lowercase name would be
  misclassified as internal. This is a documented limitation of the heuristic,
  not a defect in the model.
- **No Rust framework detectors yet:** Actix-web, Axum, Rocket, Warp routes
  are unmodeled. Same gap as pre-Express TS had.

## Extraction — Java

- Java extractor (tree-sitter-java) indexes: classes, interfaces, enums,
  methods, constructors, fields, annotation types. Overloads disambiguated
  by parameter type signatures in stable keys.
- Gradle dependency reader parses build.gradle / build.gradle.kts.
- **Gradle-to-Java-package namespace gap:** Maven group IDs (e.g.
  `org.springframework.boot`) do not directly correspond to Java package
  paths (e.g. `org.springframework.web.bind.annotation`). The classifier
  uses a 2-segment prefix heuristic (group `org.springframework.boot` →
  also matches `org.springframework.*` imports) which catches most
  transitive framework packages but can over-classify unrelated packages
  under the same vendor root. This is a fundamental limitation of
  matching build coordinates against source imports without JAR manifest
  or transitive dependency resolution. Documented as approximate.
- **Spring framework detectors (first slice shipped):** `@Component`, `@Service`,
  `@Repository`, `@Configuration`, `@RestController`, `@Controller`, `@Bean`
  detected via regex line scanning with comment-line filtering. Emitted as
  `spring_container_managed` inferences. Suppresses class-level dead-code false
  positives. Known gaps:
  - Methods inside container-managed classes still show dead (handler methods have
    no Java-caller inbound edges; Spring dispatcher invokes them at runtime).
  - Plain classes instantiated only by `@Bean` factories remain dead until the `new`
    call is resolved by enrichment or bean-return-type analysis.
  - `@Autowired` / constructor injection not modeled (DI edges, not liveness).
  - Custom stereotype annotations not detected.
  - JAX-RS, servlet/container entrypoints not yet modeled.
- **Java semantic enrichment operational but fragile:** jdtls (Eclipse JDT
  Language Server) adapter exists at `src/adapters/enrichment/java-receiver-resolver.ts`
  and has produced results on spring-petclinic and glamCRM. However, reliability is
  below TS (~81%) and Rust (~85%) enrichment. Known issues:
  - Cold-start penalty: jdtls Gradle import is slow (minutes on large projects).
    The current `rgr enrich` model (start process → query → stop) amplifies this.
  - Workspace caching helps on repeated runs but does not eliminate first-run cost.
  - Server readiness detection (language/status ServiceReady) is correct but
    does not guarantee all project symbols are indexed.
  Viable improvements: pre-warmed jdtls daemon, javac-based type resolution
  for simpler cases, or persistent background server.

## Extraction — Python

- Python extractor (tree-sitter-python): functions, classes, methods, constructors,
  variables, imports, calls, complexity metrics. Syntax-only.
- Dependency reader: pyproject.toml + requirements.txt. PEP 508 parsing.
- **Package-name-to-import-specifier gap:** `pyyaml` → `import yaml`,
  `beautifulsoup4` → `import bs4`. Exact name matches work; mismatches
  remain unclassified. Curated alias map not yet implemented.
- Pytest detector: test_* functions, Test* classes, @pytest.fixture.
  Non-decorated conftest.py functions not detected.
- **Shadowed definitions:** Two-pass extraction emits only the last
  same-name `def`/`class` at each scope level (module root, class body).
  Earlier shadowed definitions are silently suppressed. No diagnostic
  channel exists to report them. Future: emit extractor diagnostics
  for shadowed definitions so downstream tools can flag dead redefinitions.
- **No Python semantic enrichment** (pyright/mypy) yet.
- **No Python framework detectors** (Django, Flask, FastAPI) yet.

## Extraction — C/C++

- C/C++ extractor (tree-sitter-c + tree-sitter-cpp): functions, structs, classes,
  typedefs, enums, namespaces, methods, constructors, #include, CALLS, complexity.
- **Syntax-only, no compile_commands.json integration.** Header search paths
  are not resolved — `#include "util.h"` does not resolve to a FILE node
  unless the path matches the repo-relative filename exactly.
- **No macro expansion or preprocessor evaluation.** Code inside `#ifdef` blocks
  is extracted from all branches (first-wins dedup prevents collisions).
- **No template instantiation tracking.** Templates are parsed syntactically but
  instantiation-specific edges are not emitted.
- **No overload resolution.** Multiple same-named functions produce ambiguous
  CALLS edges, same as other languages.
- **STL calls:** Qualified calls (std::sort, std::find) are extracted and
  recognized as stdlib. Receiver-method calls (v.push_back) are extracted
  with raw receiver text — no type resolution.
- **Large-file guard:** Files > 1MB are skipped. This is operational containment
  for generated register headers (Linux AMD GPU headers: 200k+ lines). Not a
  semantic correctness feature.
- **Large-repo streaming/batched pipeline:**
  - Bulk `.all()` eliminated: `queryResolverNodes`, `queryStagedEdges`, `queryAllNodes`
    no longer called on the indexing hot path.
  - Resolver index built from row-at-a-time DB iterator.
  - Staged edges resolved in cursor-based batches (default 10K, configurable).
  - Detector/boundary passes use per-file `querySymbolsByFile`.
  - Dead Phase 1 in-memory maps removed (resolverByStableKey, resolverByName,
    resolverNodeToFile, nodeUidToFileUid).
  - Classification loads file signals per-batch from DB (migration 010:
    packageDependenciesJson + tsconfigAliasesJson added to file_signals).
    Same-file symbol sets rebuilt from persisted nodes via querySymbolsByFile.
    No snapshot-wide fileSignalsCache on the classification path.
  - `fileSignalsCache` retained only for Lambda entrypoint detection — stores
    import bindings only (empty stubs for other fields). TS/JS files only.
  - Multi-batch seam tests: edgeBatchSize=1 and edgeBatchSize=3 verified to
    produce identical results to default batch size.
  - **Linux-scale status:** not yet validated. Previous run hit V8 heap OOM at
    3.6 GB during bulk `.all()` materialization, which is now eliminated. Rerun
    required to discover the next blocker, if any.
- **Delta indexing (slice 1):**
  - Invalidation planner, copy-forward storage, durable extraction edges shipped.
  - `refreshRepo` uses delta path: scan → hash → plan → copy forward → extract
    only invalidated files → resolve all edges → postpasses → finalize.
  - Trust metadata persisted in extraction diagnostics (`delta` block with
    per-category file counts and per-artifact-kind copy counts).
  - **Config-file tracking gap:** manifest/config files (package.json, tsconfig.json,
    Cargo.toml, etc.) are not tracked by the file scanner. The invalidation planner
    only compares hashes for scanner-returned source files. Config changes are
    invisible to the invalidation plan. Config-widening logic exists in the planner
    but never fires because config files are never in the current-file hash set.
    Fix: include config files in the hash comparison or add a separate config-change
    detection pass using parent snapshot's file_versions.
  - **Postpasses are conservative:** run on all files, not just invalidated scope.
    Erodes refresh win for detector-heavy repos.
  - **File-local fact reuse not implemented:** inferences, boundary facts not
    copied forward for unchanged files.
  - **Large-repo delta refresh not validated:** verified on repo-graph (235 files),
    not on Linux or other large repos.
- **No clangd/libclang enrichment** for receiver-type resolution yet.

## Coverage / Churn Import

- File filtering uses repo-level file inventory (getFilesByRepo), not
  snapshot-scoped FILE nodes. Adequate for single-snapshot model, needs
  tightening for multi-snapshot.

## Test Coverage Gaps

### General
- `supersedes_uid` linkage in declaration supersession: implemented but not
  verified because `declare list --json` does not expose the field.
- `inheritObligationIds()` matcher is tested as a pure helper but not yet
  exercised by any live supersession path.

### Rust-Specific
- **pnpm test is not giving a clean signal in the current workspace** due to
  recurring `better-sqlite3` NODE_MODULE_VERSION drift. The issue is
  environmental (Node.js version changes between invocations), not code-related.
  Fix: `pnpm rebuild better-sqlite3`.

### TypeScript-Specific
- `package-name extends` in tsconfig: `extends: "@tsconfig/node18"` not
  resolved (requires node_modules lookup). Near-zero impact on current repo set.

## Dead Code Trust Boundary

- `graph dead` answers "no known inbound graph edges AND no framework-liveness
  inference," not "semantically unreachable."
- Three suppression layers: (1) inbound edges, (2) entrypoint declarations,
  (3) framework-liveness inferences (`framework_entrypoint`, `spring_container_managed`).
- Registry/plugin-driven architectures produce false positives.
- Spring bean detection suppresses class-level false positives but not method-level
  (handler methods, injected fields inside beans remain dead in the graph).
- See v1-validation-report.txt for the full extraction capability boundary.

## Classifier Limitations

- **Classifier version 6** is the current persisted format. Rows from earlier
  versions are distinguishable by `classifier_version` column on `unresolved_edges`.
- The crate-internal module heuristic (`RUST_CRATE_INTERNAL_MODULE_HEURISTIC`)
  labels likely-internal Rust imports but cannot prove they are internal without
  crate module-tree awareness.
- Blast-radius HIGH is always 0 (entrypoint-path detection deferred).
- No systematic accuracy spot-check of classifier verdicts has been performed
  since the v4 spot-check (100% precision on 96 sampled edges). Rust-specific
  precision has not been formally measured.

## Boundary Interaction Model

### HTTP provider extractor (Spring)
- **MATURE — AST-backed** via tree-sitter-java. Handles multiline annotations,
  `value=`/`path=` attributes, `method=RequestMethod.X`, marker annotations
  (no parens). Validated on glamCRM (97 routes, identical output to regex version).
- **Known inefficiency:** each Java file is parsed twice during indexing (once by
  the Java extractor, once by the Spring route extractor) because the Java extractor
  does not expose its parse tree. Deferred until measurable cost.
- **Not supported:** array-valued paths `@GetMapping({"/a", "/b"})`, custom composed
  annotations, Spring WebFlux functional endpoints.

### HTTP provider extractor (Express)
- **PROTOTYPE — line-based regex.** Receiver provenance gated to `{app, router, server}`.
  Express import gate prevents false positives on non-Express files.
- Consumes `FileLocalStringResolver` for constant-backed route paths.
- Validated on fraktag (47 routes).
- **Not supported:** aliased receivers (`const api = express.Router(); api.get(...)`),
  `app.route("/x").get().post()` chaining, middleware-only registrations (`app.use`),
  mounted router prefixes (`app.use("/api", router)`).

### HTTP consumer extractor (TS/JS)
- **PROTOTYPE — line-based regex** with file-local string resolution via
  `FileLocalStringResolver`. Resolves base-URL constants including bare identifiers.
  glamCRM: 85 consumers, 94.1% match rate. fraktag: 42 consumers, 97.6% match rate.
- **FileLocalStringResolver scope (v1):**
  - Same-file only. Does not follow imports.
  - Top-level `const` and `export const` declarations. No `let`, `var`, destructuring.
  - String literals, template literals, binary `+` concatenation.
  - References to previously-resolved same-file constants (chained bindings).
  - Environment variable placeholders (`import.meta.env.*`, `process.env.*`)
    treated as opaque prefix, stripped from resolved value.
  - No function calls, no object property access, no computed expressions.
  - Bounded recursion (max 10 steps) for cycle safety.
- **Remaining consumer gaps (glamCRM 5 of 85):**
  - Genuine path mismatches between frontend and backend (e.g. `with-docx` vs
    `with-doc`, `POST /estimates` vs `POST /estimates/create`). These are
    application-level facts, not extractor failures.
- **Not yet supported:**
  - Imported constants (`import { BASE_URL } from "./config"`)
  - Wrapper functions (`function apiGet(path) { return axios.get(BASE + path) }`)
  - Object property URLs (`config.baseUrl`)
  - Multi-line URL arguments
  - RTK Query / TanStack Query patterns

### Boundary links (derived)
- Stored in separate `boundary_links` table, NOT in core `edges` table.
- Materialized at index time for intra-repo convenience. Discardable.
- No cross-repo matching yet (architecture supports it, no implementation).

### Dead-code suppression via framework inferences
- `findDeadNodes` excludes nodes with `framework_entrypoint` (Lambda),
  `spring_container_managed` (Spring bean), `pytest_test`, `pytest_fixture`.
- **Remaining gap:** methods inside container-managed classes (e.g. `@GetMapping`
  handler methods, injected fields) still show dead because they have no
  Java-caller inbound edges. Spring's HTTP dispatcher invokes them at runtime.

## Large-Repo Scalability

- **All-in-memory indexer architecture:** `fileContents` Map holds all source
  text, `allNodes` and `allUnresolvedEdges` arrays accumulate all extracted
  data before persistence. This works up to ~1000 files but OOMs on
  Linux-scale repos (63k C files).
- **Large-file guard (1MB):** operational containment for generated headers
  (AMD GPU register masks: 200k+ lines). Does not fix the aggregate memory
  problem from tens of thousands of normal-sized files.
- **Required fix:** batch persistence + bounded resolution windows. Remove
  `fileContents`, persist nodes/edges incrementally during extraction, then
  resolve in chunks. This is an architecture redesign, not a parameter tune.

## Operational Dependency Seam Slice (deferred items)

Items surfaced during the env + fs operational dependency seam slice that
were intentionally not addressed in that slice. Tracked here so they do
not drift. Each item names the in-slice context in which it was discovered.

### Pin Node version via .nvmrc and tighten engines (P2)
- `package.json` declares `engines.node: ">=20"`, which permits any of
  v20, v22, v24. `better-sqlite3` is a native addon and its compiled
  binary is keyed to the Node ABI version. Switching Node versions
  silently invalidates the binary, producing recurring `NODE_MODULE_VERSION`
  mismatches that cost minutes per encounter and create false-baseline
  test claims when the rebuild ran against a different Node than the
  developer's interactive shell.
- **Fix:** add `.nvmrc` (or `.tool-versions`) pinning a specific Node
  version, and tighten `engines.node` to a narrow range (e.g. `">=22 <23"`)
  so an accidental v24 invocation fails fast.
- **Severity:** P2. Affects every contributor with multiple Node versions
  installed (homebrew + nvm + asdf are common). Discovered during the
  seam slice's Step 0 floor stabilization.

### CLAUDE.md rebuild note must clarify Node-version scope (P3)
- The current note `pnpm rebuild better-sqlite3` does not state that the
  rebuild is only valid for the Node version active when the rebuild ran.
  Switching Node afterwards silently re-breaks the binary.
- **Fix:** two-line addition to CLAUDE.md noting that the rebuild is
  Node-version-scoped and must be re-run after `nvm use` / version
  switches.
- **Severity:** P3. Documentation. Cheap.

### Evaluate node:sqlite as a substrate replacement (P2)
- Node ≥22.5 ships a built-in `node:sqlite` module. Migrating off
  `better-sqlite3` would eliminate the entire native-addon ABI failure
  class. This is the durable structural fix that the .nvmrc pin only
  postpones.
- **Not an automatic win.** Capability parity must be evaluated
  before adopting:
  - synchronous API shape (better-sqlite3 is sync, node:sqlite is sync)
  - prepared statement behavior and lifecycle
  - transaction ergonomics
  - WAL mode behavior
  - performance on the sizes this codebase actually exercises
- **Severity:** P2. Substrate decision, not a maintenance tweak. Should
  be a separate evaluation slice. May be subsumed by the eventual Rust
  daemon move (which uses `rusqlite` and bypasses the issue entirely).

### Live jdtls test self-skip is incomplete (P2) — RESOLVED in D-1

**RESOLUTION (D-1, live-enrichment test gating cleanup):** Closed via fix
option 3 from the original entry. Both live integration tests
(`java-enrichment-integration.test.ts`, `rust-enrichment-integration.test.ts`)
were moved from `test/adapters/enrichment/` to `test/live/`. The default
vitest config (`vitest.config.ts`) now excludes `test/live/**` via
`configDefaults.exclude` extension. A new `pnpm run test:live` script runs
the live tests explicitly as opt-in observability via a dedicated
`vitest.live.config.ts` that inherits shared test options from the default
config and includes only `test/live/**/*.test.ts`. After D-1, `pnpm run test`
and `pnpm run test:all` no longer transit `test/live/**` and therefore can
no longer be made false-negative by jdtls workspace drift, rust-analyzer
indexing race, or any other live external-tool state. The Rust-1
acceptance report's "non-admissible surfaces" caveat about live-tool
contamination is historically resolved.

**SCOPE LIMIT — what D-1 does NOT fix.** D-1 closed the live-tool
contamination only. The default `pnpm test` surface still has a separate,
unrelated instability source: the `pnpm test is not giving a clean signal`
debt about `better-sqlite3` NODE_MODULE_VERSION drift listed under
`Test-Scope Debt > Rust-Specific` earlier in this file. That debt is
environmental (Node.js version changes between invocations) and is
independent of D-1's scope. Acceptance language for any future slice
that uses `pnpm run test` or `pnpm run test:all` as evidence should
phrase those surfaces as "green in this shell with the current Node ABI
aligned" rather than as universally deterministic, until the
NODE_MODULE_VERSION drift debt is also closed.

**Original problem statement (preserved for audit trail):**

- `test/adapters/enrichment/java-enrichment-integration.test.ts` gates
  itself on `which jdtls`. If jdtls is on PATH, the tests run; if not,
  they self-skip. The gate does NOT detect the case where jdtls is on
  PATH but its workspace state is corrupted, the JDK toolchain is
  incompatible, or the Gradle import fails to complete. In those cases
  hover requests return `null` and assertions like `expect(receiverType).toBe("HashMap")`
  fail with no obvious connection to environment state.
- **Effect:** `pnpm run test` produces false-negative baselines whenever
  jdtls workspace state drifts. Affects every contributor with jdtls
  installed.
- **Fix options (option 3 was selected and implemented in D-1):**
  1. Add a sentinel hover precheck that verifies jdtls + JDK + fixture
     import are actually working before running the receiver-type
     assertions. Skip the live tests if the precheck fails.
  2. Gate the live test behind an explicit `RGR_LIVE_INTEGRATION=1`
     env var so it never runs by default.
  3. Move all live integration tests into a separate `pnpm run test:live`
     script and exclude them from the default `pnpm run test`.
- **Severity:** P2. Discovered during the seam slice's Step 6
  re-baseline. Caused divergence between the AI's environment (test
  passed) and the user's shell (test failed). Out of scope for the
  seam slice itself.

### String-literal-embedded env access false positives (P3)
- The comment masker landed in the seam slice (`src/core/seams/comment-masker.ts`)
  preserves string literal contents by design — fs detectors rely on
  literal first-argument paths inside strings (e.g.
  `fs.writeFile("real_path")`). As a side effect, env-access patterns
  embedded in string literals like `"http://example.com/path/process.env.X"`
  are still matched by the env detector regex.
- **Severity:** P3. Lower than the comment-derived false positives that
  the masker DID fix. String-embedded env patterns are rare in real
  production code and were not visible in repo-graph dogfood after the
  masker landed.
- **Possible fix paths (any of):**
  - Tighten env regex to require code-context anchors (e.g., assignment
    operator, declaration, or call argument boundary).
  - Add a second pre-pass that masks string literal contents
    specifically for the env detector (the fs detector cannot use this
    because it depends on string literal contents).
  - Move env detection from regex to a real lexer so context is
    available.
- Discovered during seam slice Step 6 dogfood after C-1 (comment
  masking) shipped.

### Detector externalization to shared YAML/TOML tables (P2)
- Env, fs, and other seam detectors are currently regex tables defined
  inline in TypeScript. The architectural decision recorded earlier in
  this conversation (R-Q3) is to externalize detector rules to shared
  YAML or TOML tables consumed by both the TypeScript implementation
  and the eventual Rust daemon implementation. This avoids regex drift
  between two language runtimes during the prototype-then-port window.
- **Scope:** detector tables become data; detector wiring becomes a
  YAML loader plus a generic regex-table walker. Comment masking
  remains language-specific code.
- **Severity:** P2. Substrate refactor, scheduled after the operational
  dependency seam feature surface ships and after detector contracts
  stabilize from real-repo exposure. Not P1 because the current
  hardcoded tables work and the parity problem only matters once Rust
  porting begins.
- **Critical sequencing:** must NOT be folded into a feature slice.
  This is its own substrate move. Recorded as the "next substrate
  slice" in the conversation roadmap.
