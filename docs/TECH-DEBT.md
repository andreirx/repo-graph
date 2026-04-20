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

## Rust CLI (`rmap`) — known divergences and deferred items

Frozen at milestone `rmap-structural-v1` (Rust-7B through Rust-20).
See `docs/milestones/rmap-structural-v1.md` for the full milestone
document. This section tracks what is not yet ported and what
intentionally diverges.

### Deferred TS CLI features (not yet ported to Rust)

- **`--edge-types` filter: narrow type set.** TS accepts all 18
  canonical edge types for callers/callees. Rust (Rust-17) accepts
  only CALLS and INSTANTIATES. Widening to all 18 is a one-line
  change in `VALID_EDGE_TYPES` but deferred until needed.
- **`--min-lines` filter for dead.** TS supports `--min-lines N`.
  Rust omits this filter.
- **`graph imports --depth`.** TS supports `--depth N` for transitive
  imports. Rust (Rust-18) is one-hop only.
- **`graph imports` module input.** TS accepts both file paths and
  module/symbol names (falls back to `resolveSymbolKey`). Rust
  (Rust-18) accepts file paths only (constructs `{repo}:{path}:FILE`
  stable key). Module input is a future extension.
- **`graph path` parameter surface.** TS supports `--max-depth N`
  (default 8) and `--edge-types CALLS,IMPORTS`. Rust (Rust-19)
  hardcodes both defaults. Also, TS accepts FILE/MODULE stable
  keys via `resolveSymbolKey`; Rust is symbol-only at both endpoints.
- **`graph metrics`.** Not ported.
- **`graph versions`.** Not ported.
- **Table output format.** Rust is JSON-only. No human-readable
  tables. No `--json` flag (always JSON).

### Accepted divergences (intentional, not debt)

- **Rust `callers`/`callees` envelope includes `target` field.**
  The TS CLI does not include the resolved target in the callers/
  callees JSON output (it uses a different formatting path via
  `outputNodeResults`). The Rust side adds `target` as a
  convenience field. This is a superset, not a contract break.
- **Rust `dead` envelope includes `kind_filter` field.** TS does
  not emit this. The Rust side adds it for transparency. Superset.
- **`trust` command does not use QueryResult envelope.** Both TS
  and Rust emit a trust-specific report shape. No drift.
- **Rust `gate` waiver overlay: PASS obligations are not waivable (Rust-25).**
  TS unconditionally sets `effective_verdict = WAIVED` when any
  matching active waiver exists, regardless of `computed_verdict`.
  This means a PASS obligation with a waiver shows as WAIVED in TS,
  inflating `waived` counts and hiding the distinction between
  obligations that needed an exception and obligations that passed
  on merit. Rust applies waivers only to non-PASS computed verdicts.
  A PASS obligation stays PASS with `waiver_basis = null`, even if
  a matching waiver exists. This is the corrected policy model:
  `effective_verdict` represents the verdict after policy
  transformation, and no transformation occurs for PASS.
  The TS prototype should be aligned to match if/when TS is still
  actively maintained.

- **Rust declaration UIDs are deterministic, TS uses random UUIDs (Rust-32).**
  TS `declare` commands generate `uuidv4()` — a random UUID per
  insert. Repeated declare runs create duplicate rows with different
  UIDs. Rust uses UUID v5 (SHA-1 namespace hash) derived from the
  semantic identity tuple `(kind, identity_key)` where identity_key
  is constructed from typed policy fields:
    - boundary: `{repo}:{module_path}:{forbids}`
    - requirement: `{repo}:{req_id}:{version}`
    - waiver: `{repo}:{req_id}:{requirement_version}:{obligation_id}`
  This makes `insert_declaration` idempotent at the policy level:
  the same logical policy object always produces the same UID, and
  INSERT OR IGNORE prevents duplicates. Cosmetic changes to
  `value_json` (reason text, obligation wording) do NOT alter the
  UID — they are not policy identity. This is a deliberate
  correction to the TS prototype's append-only-by-accident pattern.

### Known contract gaps

- **`index` and `refresh` have no JSON output.** They print
  progress to stderr only. TS `rgr repo index` also uses stderr
  for progress, so this is aligned.
- **No `--level file` flag for cycles.** TS supports `--level
  file` to detect file-level cycles. Rust hardcodes module-level.
- **No staleness tracking for `index`/`refresh` commands.** Only
  read-side commands compute the `stale` field.

## Rust agent use-case crate (`repo-graph-agent`)

Current state: Rust-43A. Repo-level `orient` use case with
typed DTOs, `AgentStorageRead` port, ranking, budget
truncation, confidence derivation, AND gate signal coverage
(`GATE_PASS` / `GATE_FAIL` / `GATE_INCOMPLETE`) via dependency
on `repo-graph-gate`. No CLI (Rust-43B), no `check`/`explain`
(later slices), no module/symbol focus (Rust-44/45). See
`docs/architecture/agent-orientation-contract.md` for the
normative surface description.

### Rust-43 F1/F2/F3 fix slice — CLOSED

The repo-graph self-index spike
(`docs/spikes/2026-04-15-orient-on-repo-graph.md`) exposed
three semantic defects in the Rust-43B `orient` contract:

- **F1:** The DEAD_CODE signal reported 86% of symbols as
  dead on a Rust-indexed repo, because the Rust indexer does
  not populate framework-liveness inferences and the
  aggregator had no reliability gate. An agent reading that
  output would prioritize mass deletion over investigation.
- **F2:** TRUST_NO_ENRICHMENT was silently suppressed on
  Rust-indexed repos because `enrichment_applied: bool`
  collapsed "phase never ran" and "phase ran with zero
  eligible edges" into one state.
- **F3:** The DEAD_CODE aggregator had no coupling to trust
  reliability, so the signal ranked at #1 (Medium severity)
  regardless of call-graph quality.

Action taken:

1. New DTOs in the agent crate:
   - `AgentReliabilityLevel { Low, Medium, High }`
   - `AgentReliabilityAxis { level, reasons: Vec<String> }`
   - `EnrichmentState { Ran, NotApplicable, NotRun }`
2. `AgentTrustSummary` widened with `call_graph_reliability`,
   `dead_code_reliability`, and `enrichment_state`. The old
   `enrichment_applied: bool` was removed.
3. New `LimitCode::DeadCodeUnreliable` with a static summary.
   The `Limit` struct gained a `reasons: Vec<String>` field
   (serialized only when non-empty, preserving backward
   compatibility for every other limit).
4. Dead-code aggregator signature gained
   `trust: &AgentTrustSummary`. Emission is gated on
   `trust.dead_code_reliability.level == High`. When the
   level is not High, the aggregator emits
   `DEAD_CODE_UNRELIABLE` carrying the trust layer's reason
   vector verbatim (fallback: the stable string
   `"dead_code_reliability_not_high"` if trust returned
   empty reasons).
5. Trust aggregator rule update: `TRUST_NO_ENRICHMENT`
   fires iff `enrichment_state == NotRun`.
6. Confidence derivation update: `NotRun` degrades
   high→medium on the enrichment axis; `Ran` and
   `NotApplicable` are silent on this axis.
7. Storage adapter `get_trust_summary` projects the trust
   crate's composite reliability axes into the agent DTO
   without re-deriving any thresholds. The trust crate is
   the authority.
8. Tests: 7 new integration tests in
   `rust/crates/agent/tests/orient_repo_dead_code_reliability.rs`
   pin the reliable path, the Low/Medium/Low-reason-fallback
   paths, the JSON shape of `reasons`, and the
   `skip_serializing_if = "Vec::is_empty"` behavior.
   Existing tests updated to construct the new DTO shape.
   Storage-side smoke test asserts the empty-snapshot
   reliability-gate case end-to-end.
9. Spike re-run verified the fix on the same database
   (`/tmp/rgspike.db`). See the spike document for the
   before/after comparison.

Zero workspace regressions: 1128 tests passing, 0 failures.

#### P2 follow-up: enrichment state disambiguation

The initial F1/F2/F3 fix mapped `Option<EnrichmentStatus>`
directly: `None → NotRun`, `Some(_) → Ran` or `NotApplicable`
based on `eligible > 0`. A code-review P2 identified that
`None` from the trust layer covers THREE distinct cases:

1. No eligible `CallsObjMethodNeedsTypeInfo` samples at all.
2. Samples exist but none are in that category.
3. Samples exist AND are in that category, but the
   enrichment phase never ran.

Cases 1 and 2 are `NotApplicable` semantically — no work for
enrichment to do — but the pre-P2 adapter reported all three
as `NotRun`, which would emit a spurious `TRUST_NO_ENRICHMENT`
signal and degrade confidence on the enrichment axis for any
repo with nothing to enrich.

Fix:

- Added a `#[serde(skip, default)] pub enrichment_eligible_count: u64`
  field to `trust::types::TrustReport`. The field is internal
  to the Rust trust crate and never serializes, so the TS
  parity contract and existing fixtures are unchanged
  (verified by running the trust parity harness).
- `compute_blast_radius_and_enrichment` now returns a
  3-tuple with the eligible count as its third element, and
  `compute_trust_report` populates the new field from that
  value. When `all_classification_counts.is_empty()` the
  count is 0 (short-circuit path, no compute run).
- The agent storage adapter reads the counter to disambiguate:
  `None && count == 0 → NotApplicable`, `None && count > 0 →
  NotRun`, `Some(_) → Ran`.

Tests added:

- 2 new trust unit tests pinning `enrichment_eligible_count`
  at 0 for "no samples at all" and "samples present but none
  are CallsObjMethodNeedsTypeInfo".
- 1 updated trust unit test pinning the counter at 1 in the
  "phase did not run" case.
- 1 new storage integration test
  (`empty_snapshot_maps_to_enrichment_state_not_applicable`)
  that exercises the adapter through `StorageConnection` and
  verifies the empty-snapshot mapping is `NotApplicable` (not
  the pre-P2 `NotRun`).
- The spike re-run on `/tmp/rgspike.db` produced identical
  output to the post-F1/F2/F3 output, confirming that the
  repo-graph self-index is legitimately in the `NotRun` state
  (eligible samples exist, phase did not run). The P2 fix
  did not alter this; it removed the false-positive case for
  repos with nothing to enrich.

Post-P2 workspace: 1131 tests passing, 0 failures (+3
regression pins).

### Deferred items (explicit)

- **Output-quality cleanup (F4/F5/F6).** Deferred to a
  separate slice. Items: (F4) test-file filter on `top_dead`
  when DEAD_CODE does fire, (F5) polyglot-blind
  `MODULE_SUMMARY.languages` on non-TS indexed repos,
  (F6) ranking tie-break between DEAD_CODE and
  IMPORT_CYCLES in the Structure/Medium tier. None of these
  block further slices but all would improve output quality.
  The F2 fix also left a minor wording tweak on
  `TRUST_NO_ENRICHMENT`'s summary — still says "No compiler
  enrichment applied" when it should say "Enrichment phase
  did not run" under the new NotRun semantics.
- **Module, path, and symbol focus.** `orient(focus = Some(_))`
  currently returns `OrientError::FocusNotImplementedYet`. This
  is deliberately an error variant, not a silent degraded
  response, so a caller that requests a focus area learns
  immediately that their request is not honored. Module focus
  ships in Rust-44, symbol focus in Rust-45.
- **`check` is repo-level only.** Scoped check (file/path/symbol
  focus) is not implemented. Only whole-repo check is available.
- **`check` does not expose individual condition exit codes.**
  The CLI returns only the aggregate verdict exit code (0/1/2).
  Individual condition pass/fail/incomplete status is available
  in the JSON output only.
- **`check` CLI uses `<db_path> <repo_uid>`.** Same temporary
  positional shape as orient, pending repo registry.
- **`explain` use case.** Only `orient` and `check` are
  implemented. The DTO envelope is shared; the `explain`
  aggregator pipeline is not yet written.
- **Binary renamed; repo registry deferred.** Rust-43A
  relocated gate. Rust-43B added the `orient` CLI command.
  Rust-43C renamed the binary from `rgr-rust` to `rmap`
  (test harnesses, docs, CLAUDE.md all updated).
  Repo registry (`rmap repo add` equivalent) is still
  deferred.
- **`orient` positional shape diverges from the contract.**
  The agent orientation contract specifies
  `rmap orient <repo_name>` with an implicit repo registry
  and `--db <path>` as an escape hatch. `rmap orient`
  currently ships with `<db_path> <repo_uid>` because
  `rmap` has no repo registry yet — no equivalent of
  `rmap repo add`. Until a registry slice lands, every
  `rmap` command including `orient` takes the
  `<db_path> <repo_uid>` pair. Repo-name invocation and the
  `--db` escape hatch will land together in the registry
  slice.
- **`--focus` CLI grammar is locked but runtime is
  deferred.** The `rmap orient --focus <string>` flag
  parses and validates, then exits 2 with a
  `FocusNotImplementedYet` diagnostic. Rust-44 (module/path)
  and Rust-45 (symbol) will implement the runtime without
  changing the flag grammar.
- **`COMPLEXITY_UNAVAILABLE`.** Cyclomatic complexity is not
  produced by the Rust indexer; the agent pipeline emits the
  limit code unconditionally. Signal `HIGH_COMPLEXITY` is
  declared in the enumeration but never constructed.
- **`MODULE_DATA_UNAVAILABLE`.** Module discovery (Layer-1
  catalog) is TS-only. The Rust `MODULE_SUMMARY` signal falls
  back to raw snapshot totals (files, symbols, distinct file
  languages). The limit is emitted unconditionally so the
  agent can distinguish this fallback from a richer catalog.
- **Next-action emission.** The repo-level `next` list is
  always empty. Structured `NextAction` records become
  meaningful under module/symbol focus.
- **Staleness wording discipline.** `TRUST_STALE_SNAPSHOT`
  uses `get_stale_files` as its data source and describes the
  storage-internal condition only (`"Snapshot has N stale
  files recorded in storage."`). It does NOT claim that the
  repository has changed since the last index — that would
  require a filesystem or git comparison the use-case layer
  intentionally does not perform. If a future slice adds a
  `current_commit: Option<&str>` parameter, the wording and
  the signal code should both be revisited so the distinction
  between "parse-staleness" and "git-staleness" stays explicit.
- **`now` is a required parameter on `orient()`.** The
  orient entry point takes `now: &str` as its final argument
  and threads it through to the gate aggregator, which passes
  it to `GateStorageRead::find_waivers` for lexicographic ISO
  8601 expiry comparison. The agent crate is deliberately
  clock-free: the function signature forces callers (CLI,
  daemon, tests) to supply an explicit wall-clock value.
  A previous draft used a constant `AGENT_NOW_SENTINEL`
  (`"9999-12-31T23:59:59Z"`) which silently mis-evaluated
  finite-expiry waivers — a far-future sentinel makes
  `expires_at > now` false for every realistic expiry, so
  every finite waiver appears already expired. The sentinel
  was removed and replaced with this explicit parameter.
  Regression tests in
  `rust/crates/agent/tests/orient_repo_gate.rs`
  (`finite_waiver_applies_before_expiry_at_orient_level`,
  `finite_waiver_does_not_apply_after_expiry_at_orient_level`,
  `perpetual_waiver_applies_regardless_of_now`) pin the
  correct semantics.

### Gate.rs relocation — CLOSED in Rust-43A

Prior state (Rust-42): `gate.rs` lived inside the `rgr` binary
crate. The agent crate had no way to call it, so orient
emitted `GATE_UNAVAILABLE` as a limit in every response.

Action taken (Rust-43A):

1. Created `rust/crates/gate/` as a new policy crate
   `repo-graph-gate`. Two-layer design:
   - `compute(input: GateInput) -> GateReport` — pure, no
     I/O, no storage, no clock.
   - `assemble(storage: &impl GateStorageRead, ...) -> GateReport`
     and `assemble_from_requirements(...)` — thin
     orchestration around storage reads, delegates to
     `compute`.
2. Defined `GateStorageRead` port (agent-style concrete error,
   mirroring `AgentStorageRead`). Implemented on
   `StorageConnection` in `rust/crates/storage/src/gate_impl.rs`.
3. Defined gate-owned DTOs (`GateRequirement`,
   `GateObligation`, `GateWaiver`, `GateMeasurement`,
   `GateInference`, `GateBoundaryDeclaration`,
   `GateImportEdge`). Gate does NOT import
   `repo_graph_storage::queries::*`.
4. `rgr/src/main.rs::run_gate` updated to call
   `repo_graph_gate::assemble`. Deleted `rgr/src/gate.rs`.
   Stderr wording preserved verbatim via a
   `format_gate_error` helper in the CLI (the gate crate
   itself has CLI-agnostic error types).
5. Added `repo-graph-agent` dependency on
   `repo-graph-gate`. `orient_repo`'s trait bound widened to
   `S: AgentStorageRead + GateStorageRead`. New
   `aggregators::gate` aggregator emits `GATE_PASS`,
   `GATE_FAIL`, `GATE_INCOMPLETE` from `GateReport.outcome`
   (always `GateMode::Default`).
6. Replaced `LimitCode::GateUnavailable` with
   `LimitCode::GateNotConfigured`. The new limit fires only
   when the repo has no active requirements.
7. Gate-crate dependency direction: `agent → gate`,
   `storage → gate`, `rgr → gate`. NO reverse edge. Gate has
   no knowledge of agent orientation.

Behavioral preservation guarantees:

- Default / strict / advisory mode semantics are byte-
  identical to pre-relocation `gate.rs`. Exit code tests in
  `rust/crates/rgr/tests/gate_command.rs` pass unchanged.
- PASS-not-waivable divergence (Rust-25) preserved. Tests
  `pass_obligation_stays_pass_even_with_matching_waiver` in
  `compute.rs` and `gate_pass_with_waiver_stays_pass` in
  `gate_command.rs` both pass.
- Malformed measurement/inference error strings preserved
  verbatim: `"malformed coverage measurement for {}: ..."`,
  `"coverage measurement for {} missing numeric \"value\" field"`,
  and the equivalents for complexity and hotspot. These are
  emitted from the new `assemble.rs` pre-parse helpers.

Future gate work:

- Storage integration tests for `GateStorageRead` on
  `StorageConnection` currently reuse the `gate_command.rs`
  CLI suite (which exercises the full pipeline). A
  dedicated `rust/crates/storage/tests/gate_impl.rs` that
  isolates the adapter layer was deliberately not added in
  this slice to keep the diff focused; adding one is a
  small follow-up.

### Agent storage port narrowness

The `AgentStorageRead` trait (defined in
`rust/crates/agent/src/storage_port.rs`) is deliberately
narrow: one method per orient data need, with agent-owned
DTOs and a storage-agnostic `AgentStorageError` type. The
trait will grow when `check`/`explain` ship. Each addition
must stay narrow — no generic escape-hatch methods, no
passing through of `StorageConnection`, no leaking of
`rusqlite::Error` or `StorageError`.

The `get_trust_summary` method is the one place in the trait
that sits on top of a second policy crate (`repo-graph-trust`).
The storage adapter calls `trust::assemble_trust_report`
internally and projects the result into the agent-owned
`AgentTrustSummary` DTO. If the trust surface gains new
fields that orient wants to read, extend `AgentTrustSummary`
— do NOT expose `TrustReport` through the port.

### Signal evidence serialization

`SignalEvidence` is a produce-only tagged enum with a hand-
written `Serialize` impl that forwards to the inner variant
struct with no discriminator tag. The `Signal` parent carries
the `code` field which serves as the discriminator in the
JSON output. Adding deserialization — for instance to consume
signals on the daemon's client side — would require
redesigning the discriminator, which is intentionally out of
scope today. If/when that need arises, the options are:
container-tagged serde (add a `kind` field inside
`evidence`), internally-tagged serde, or a dedicated
deserializer that routes by parent `code`.

### Agent signal code coverage

The enumeration in `SignalCode` is complete — every code the
agent contract mentions has a variant — but only the codes
the repo-level pipeline actually constructs have evidence
variants and named constructors. The unused codes
(`HighComplexity`, `HighFanOut`, `HighInstability`,
`CallersSummary`, `CalleesSummary`, and the gate variants)
are reserved for later slices. When they are wired up, each
addition must:

1. Add its evidence struct in `signal.rs`.
2. Add a variant to `SignalEvidence`.
3. Extend the manual `Serialize` match arm.
4. Add a named constructor to `Signal`.
5. Add a unit test asserting the code ↔ category ↔ severity
   descriptor and the evidence variant match.

## State-Boundary Extraction (slice 1)

Normative contract: `docs/architecture/state-boundary-contract.txt`.
Milestone plan: `docs/milestones/rmap-state-boundaries-v1.md`.

### Rust-only posture — TS parity gap

State-boundary extraction ships Rust-first. The TypeScript extractor
path (`src/adapters/extractors/*`) does NOT emit READS / WRITES
edges with target kind DB_RESOURCE, FS_PATH, STATE, or BLOB. This
is deliberate and documented here rather than papered over with a
stub. CONFIG_KEY is not a state-boundary target in this slice on
either runtime (see state-boundary-contract.txt §4.3, §5.6).

Implications:

- Databases indexed with `rgr repo index` (TS) will not contain
  state-boundary edges; databases indexed with `rmap index` (Rust)
  will.
- Cross-runtime interop (`pnpm run test:interop`) verifies that the
  TS storage adapter tolerates Rust-written databases containing
  the three new `nodes.kind` values. It does NOT verify that TS
  emits equivalent facts.
- Any future product surface that needs state-boundary edges on
  TS-indexed repos must either (a) re-index with `rmap`, or (b)
  port the extractor logic. The canonical direction is (a) for
  now; (b) is a planned future slice only if TS-indexed repos
  must carry these facts.
- Parity harness (`test/fixtures/parity-*`) has no state-boundary
  fixture. A Rust-only parity harness entry (Rust-half only) is
  acceptable; a TS-half stub is NOT acceptable (would misrepresent
  feature availability).

### Deferred items within the state-boundary program

Not in slice 1. Each is its own planned future slice.

- **Queue / event boundaries.** `EMITS`, `CONSUMES`, and the
  `QUEUE` node kind. Kafka, SQS, SNS, RabbitMQ, Redis pub/sub.
- **Config / env seam graph emission.** `CONFIG_KEY` node
  emission, explicit config→resource wiring edges, cross-language
  CONFIG_KEY identity normalization. Slice 1 carries config/env
  provenance in edge evidence only (via `logical_name_source =
  "env_key"`) and does NOT emit CONFIG_KEY nodes. See
  state-boundary-contract.txt §5.6.

### TS-side `importedName` population deferred (SB-3-pre)

`ImportBinding.importedName` (TS interface in
`src/core/ports/extractor.ts`; Rust struct in
`rust/crates/classification/src/types.rs`) stores the original
exported symbol name for named imports:

- `import { readFile } from "fs"` → `importedName = "readFile"`.
- `import { readFile as rf } from "fs"` → `importedName =
  "readFile"`, `identifier = "rf"`.
- `import fs from "fs"` (default) → `importedName = null`.
- `import * as fs from "fs"` (namespace) → `importedName = null`.

**Rust populated.** `rust/crates/ts-extractor/src/extractor.rs`
populates this correctly for all four patterns. Covered by unit
tests.

**TS NOT populated.** All five TS extractors
(`src/adapters/extractors/{typescript,python,rust,java,cpp}/`)
pass `importedName: null` unconditionally. This is a deliberate
Fork-1 posture: TS-side state-boundary emission is deferred, so
no current TS consumer needs the resolved original-symbol name.

**Parity impact.** Cross-runtime parity harness
(`test/ts-extractor-parity/ts-extractor-parity.test.ts:88-100`)
projects `ImportBinding` to a fixed field set that does NOT
include `importedName`. The Rust serde attribute
`#[serde(default, skip_serializing_if = "Option::is_none")]`
additionally keeps absent values invisible on the wire for
`None` cases. Net effect: no parity harness impact, no
serialization drift.

**Follow-on slice.** When TS-side state-boundary emission is
prioritized (or any other TS consumer needs resolved original-
symbol names), ship a dedicated slice that ports the Rust
alias-resolution logic to each TS extractor. Until then, TS
consumers of `ImportBinding` must treat `importedName` as
potentially null.

### Refresh stale-orphan resource nodes (SB-4-pre Fix B)

`rmap refresh` (incremental re-index) copies resource nodes
(`file_uid IS NULL`, kinds `DB_RESOURCE` / `FS_PATH` / `BLOB` /
`STATE`) from the parent snapshot unconditionally. If a resource
was referenced ONLY by a file that was subsequently changed or
deleted, the resource node persists as a stale orphan in the
refresh snapshot. It is not reachable from any symbol via
READS/WRITES edges (the old edges belonged to the changed file
and are NOT copy-forwarded), but it occupies the graph as an
unreferenced node.

Workaround: run `rmap index` (full re-index) to produce a clean
graph without stale orphans.

Root cause: copy-forward cannot determine which resource nodes
are still referenced without re-running state-boundary extraction
on ALL files, which defeats the performance benefit of delta
refresh. Correct fix requires either:
- persisting resolved_callsites for delta reuse, or
- a post-copy GC pass that prunes unreferenced resource nodes

Both are deferred to a dedicated delta-aware state-boundary
slice. The stale-orphan behavior is pinned by a test
(`refresh_mixed_unchanged_and_changed_files` in
`repo-index/tests/state_boundary_integration.rs`) so that an
unintentional fix or regression is detected.

### `rmap dead` excludes resource kinds — TS divergence (SB-5)

`rmap dead` (Rust `find_dead_nodes` in `storage/src/queries.rs`)
excludes resource node kinds (FS_PATH, DB_RESOURCE, BLOB, STATE+CACHE)
from the dead-node result set. This is correct behavior: resource
nodes have no inbound static edges by construction (they are targets
of READS/WRITES edges, not sources), so they would appear as mass
false positives without this exclusion.

**TS divergence:** The TypeScript `findDeadNodes` implementation
(`src/adapters/storage/sqlite/sqlite-storage.ts`) does NOT yet have
this exclusion. This creates a cross-runtime query behavior split:

- `rmap dead amodx` excludes resource nodes.
- `rgr graph dead amodx` includes resource nodes (if any exist).

Impact: TS CLI users may see resource nodes in dead results on
databases that contain state-boundary facts (i.e., Rust-indexed DBs
opened via TS CLI). This is a documentation/awareness gap, not a
data-loss bug.

**Fix path:** Either (a) port the exclusion to the TS `findDeadNodes`
query, or (b) accept the divergence and document it in CLI help text.
Option (a) is preferred for query-surface consistency.

### SB-3 binding-table coverage is FS-only

Slice SB-3 ships the state-extractor TS integration plus a
binding table restricted to Node.js filesystem stdlib APIs. The
milestone-listed SDK/DB/Cache modules are NOT in the SB-3 binding
table. Reasons and scope:

**Not in SB-3** (deferred to SB-3-next and beyond):

- AWS SDK S3 (`@aws-sdk/client-s3`): the identifying payload is
  `{Bucket: "..."}` inside a `new PutObjectCommand(...)` argument
  — an object-property pattern. SB-3's arg-0 classifier only
  handles string literals and `process.env.NAME` member reads.
  Object-property extraction is SB-3-next scope.
- Database drivers (`pg`, `mysql2`, `better-sqlite3`, `sqlite3`):
  the resource identity is at client CONSTRUCTION (e.g. `new
  Client({connectionString: ...})`), not at the `.query()`
  call. SB-3 does not track constructor arguments. Constructor
  tracking is SB-3-next-next scope.
- Redis clients (`redis`, `ioredis`): same pattern as DB drivers
  — `createClient({url: ...})` or `new Redis({host, port})` at
  construction. Deferred to constructor-tracking slice.
- FS metadata-only ops (`readdir`, `stat`, `access`, `realpath`,
  etc.): out of slice-1 scope (content touchpoints only).
  Deferred pending a consumer need.
- `fs.open` / `fs.openSync` / `fs.promises.open`: direction is
  flag-dependent (`r`/`w`/`a`/...). Without flag parsing a
  single `direction` would misrepresent the call. Deferred.

**Shipped in SB-3** binding-table coverage:

- `fs`, `node:fs`: `readFile`, `readFileSync`, `writeFile`,
  `writeFileSync`, `appendFile`, `appendFileSync`,
  `createReadStream`, `createWriteStream` (8 symbols × 2
  specifiers = 16 entries).
- `fs/promises`, `node:fs/promises`: `readFile`, `writeFile`,
  `appendFile` (3 symbols × 2 specifiers = 6 entries).

Total: 22 binding entries. Exact module matching; module-
specifier normalization (e.g. `node:fs` → `fs`) is a separate
substrate decision and is NOT part of SB-3.

### `ResolvedCallsite` population is Rust-only (SB-3-pre)

`ExtractionResult.resolvedCallsites` / `resolved_callsites` is
structurally present on BOTH runtimes (TS `src/core/ports/extractor.ts`
and Rust `rust/crates/indexer/src/types.rs` at the
`ExtractionResult` boundary). Z-a (see SB-3-pre locks) kept the
extractor-port contract unified across runtimes.

POPULATION is Rust-only:

- Rust `ts-extractor` populates `resolved_callsites` for call
  expressions that match Form-A resolution and slice-1 arg-0
  patterns.
- Every TS extractor
  (`src/adapters/extractors/{typescript,python,rust,java,cpp}/`)
  returns `resolvedCallsites: []` under the Fork-1 posture.

Port-contract invariant: a consumer reading `ExtractionResult`
sees the same field set regardless of producer runtime. Under
Fork 1, TS-sourced `resolvedCallsites` is uniformly empty; this
is a population gap, not a contract split. When TS-side state-
boundary emission opens, the port already carries the field —
only the TS extractor logic needs to land.

The type is slice-scoped:

- `Arg0Payload` variants `StringLiteral` / `EnvKeyRead` are the
  only slice-1 argument-0 patterns (both runtimes).
- Object-property extraction (e.g., `{Bucket: "x"}`), constructor
  tracking (`new Client({...})`), and argument positions beyond
  0 are intentionally out of scope and will land in their own
  future slices with their own typed additions to `Arg0Payload`
  OR new sibling types.
- The name `Arg0Payload` carries the slice scope in its name
  (not `CallArgumentModel` or `UniversalArgumentShape`) to
  prevent future expansion from silently inheriting a name that
  claims more than it delivers.
- **SQL-string parsing.** Until this lands, DB targets are
  `DB_RESOURCE` (logical-connection granularity). `TABLE` node
  kind remains reserved.
- **ORM / repository / DAO pattern inference.** Prisma, TypeORM,
  JPA/Hibernate, SQLAlchemy, Diesel. Currently out of scope;
  ORM call sites will NOT produce state-boundary edges until a
  dedicated slice.
- **GCP and Azure blob SDKs.** Only AWS S3 SDKs for Node, Java,
  Python, Rust are in the slice-1 binding table.
- **C++ DB, cache, and blob SDKs.** Only `std::filesystem` and
  POSIX `open` / `fopen` ship in C++ for slice 1. Vendor-specific
  C++ libraries are deferred.
- **Type-enrichment-backed matching (Form B).** Receiver-type
  resolution via TS TypeChecker / rust-analyzer / JDT-LS is
  explicitly NOT used for state-boundary matching in slice 1.
  Matcher form A (import-anchored call) only.
- **Dynamic-target emission.** When the target is a parameter of
  unknown provenance or a runtime-composed string, slice 1 emits
  NOTHING (see contract §5.4). A later slice with value tracking
  may fill this gap.
- **Dedicated `rmap state` command.** No resource-node
  enumeration CLI in slice 1. Agents use existing `callees` /
  `callers` with `--edge-types READS,WRITES`.
- **State-boundary reliability axis in trust report.** Current
  trust reliability measures CALLS resolution. State-boundary
  coverage reliability requires a ground-truth corpus; deferred.

### Known gaps discovered during contract design

- **Pre-existing READS / WRITES / EMITS / CONSUMES declared but
  unused.** Before slice 1, these four edge types existed in the
  canonical `EdgeType` enum (both TS `src/core/model/types.ts`
  and Rust `rust/crates/storage/src/types.rs`) and in the CLI
  edge-type filter list, but NO extractor emitted them. Similarly,
  node kinds `STATE`, `TABLE`, `QUEUE`, `CONFIG_KEY` were declared
  but unused. Slice 1 resolves this for READS / WRITES and for
  STATE. CONFIG_KEY / EMITS / CONSUMES / QUEUE remain
  declared-but-unused after slice 1: CONFIG_KEY waits on the
  future config/env seam slice, EMITS / CONSUMES / QUEUE wait on
  the queue-boundary slice. TABLE remains reserved pending SQL
  parsing. This transitional state is not a defect; it documents
  that the canonical vocabulary was designed ahead of
  implementation.

- **Schema column `nodes.kind` and `edges.type` have no CHECK
  constraint or enumerated lookup table on either runtime
  (verified at slice-1 design time).** Adding node kinds or edge
  types therefore requires only enum-value additions in the TS
  and Rust domain models; no SQL migration. This is a schema
  property relied on by this slice and by any future vocabulary
  expansion. If a CHECK constraint is ever introduced, the
  impact on existing and future vocabulary-expansion slices
  must be reassessed.

- **Resource nodes have no inbound static edges by construction.**
  Dead-code reducers MUST exclude resource kinds emitted by
  state-boundary extraction (DB_RESOURCE, FS_PATH, STATE, BLOB)
  to avoid mass false positives. Slice 1 enforces this in the
  `rmap dead` path; future kind-filter additions to dead-code
  must follow the same rule. CONFIG_KEY is outside this slice
  and is handled by the config/env seam slice when it ships.
