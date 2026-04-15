# Roadmap

Operational product roadmap. Ordered by engineering priority, not aspiration.
See `docs/VISION.md` for long-term direction and horizon model.
See `docs/TECH-DEBT.md` for known limitations and test gaps.

## Current state (as of last commit)

- **1464 tests** across 78 test files.
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
- **Streaming/batched indexing pipeline:** Linux kernel indexes successfully.
  - Resolver index built from row-at-a-time DB iterator (no bulk `.all()`).
  - Staged edges resolved in cursor-based batches (10K default).
  - Classification loads file signals per-batch from DB (migration 010:
    packageDependenciesJson + tsconfigAliasesJson in file_signals table).
    Same-file symbol sets rebuilt from persisted nodes. No snapshot-wide
    fileSignalsCache on the classification hot path.
  - Detector/boundary passes use per-file `querySymbolsByFile`.
  - Dead Phase 1 in-memory maps eliminated.
  - Multi-batch seam tests verify count/breakdown parity at batch sizes 1 and 3.
- **Python two-pass extraction:** last-definition-wins semantics for same-scope function
  and class redefinitions. Shadowed definitions suppressed (no node, no edges, no metrics).
- **Repo set:** amodx, fraktag, glamCRM, repo-graph, mempalace, glam-scrapers,
  unelte, swupdate, buildroot, C++11 Deep Dives, **Linux kernel**.
  - TS-only, TS+Rust, TS+Java, Python, C/C++, mixed multi-language.
  - C/C++ validated on swupdate (208 files, 3422 nodes) and buildroot (645 files, 5249 nodes).
  - Linux kernel: 63,701 files, 1,045,482 nodes, 2,045,964 resolved edges,
    2,775,402 unresolved edges. Indexed in 77 min. Syntax-only (no compile_commands.json
    in this run). High unresolved rate expected without build-system context.
- **Framework detection:** Express routes, Lambda handlers, Spring beans, pytest tests/fixtures,
  Linux kernel system patterns (module_init, platform_driver, GCC constructor/destructor).
  `findDeadNodes` consults all framework-liveness inferences.
- **CLI boundary model:** HTTP + cli_command mechanisms. Commander providers, package.json
  script consumers, shell script consumers, Makefile recipe consumers. Binary-prefix matching.
- **Module discovery:** Declared modules detected from manifests/workspaces
  (package.json, pnpm-workspace.yaml, Cargo.toml, settings.gradle, pyproject.toml).
  Module candidates, evidence, and file ownership persisted in dedicated tables
  (migration 011). CLI surface: `rgr modules list|evidence|files`.
  Additive — coexists with directory MODULE nodes.
- **Key architectural decisions made:**
  - Nearest-owning per-file context for deps, tsconfig, Cargo, Gradle, pyproject.toml.
  - Enrichment is a separate `rgr enrich` pass, not inline with indexing.
  - Edge promotion (D2c) is opt-in via `--promote`, 8-gate safety filter.
  - Boundary facts stored separately from core edges table (derived links discardable).
  - Large-file guard: files > 1MB skipped during indexing (operational containment).
  - Imported free-function calls resolved via import bindings (no compiler needed).
  - Sharded indexing (shard-local extraction + global integration pass) is the target
    architecture for Linux-scale repos beyond the current streaming pipeline.

## Shipped

### Rust structural CLI (`rgr-rust-structural-v1`)
- 10 commands: index, refresh, trust, callers, callees, path, imports, dead, cycles, stats
- JSON-only output with TS-compatible QueryResult envelopes
- Exact symbol resolution (SYMBOL-only, 3-step)
- Edge-type filters on callers/callees (CALLS, INSTANTIATES)
- Shortest-path BFS (CALLS+IMPORTS, depth 8)
- Module-level cycle detection, structural metrics, dead-code analysis
- Cross-runtime interop proven (19 tests: Rust writes, TS reads + formats)
- Contract tests, envelope tests, per-command deterministic test matrices
- Milestone doc: `docs/milestones/rgr-rust-structural-v1.md`
- Built across Rust-7B through Rust-20 (14 slices)

### Rust governance CLI (post-v1)
- `violations`: boundary violation check via declared forbidden IMPORTS
  (Rust-22). Reads boundary declarations, queries cross-module
  IMPORTS edges, reports violations.
- `gate`: CI gate with obligation evaluation (Rust-24). Narrow first
  gate: arch_violations method only, default mode only.
  Exit codes: 0 pass, 1 fail, 2 incomplete. TS-compatible gate
  report shape with toolchain, computed/effective verdicts.
- `gate`: Rust-25 adds active waiver resolution. Exact 3-tuple
  match (req_id, requirement_version, obligation_id). Expiry via
  lexicographic ISO 8601 comparison. waiver_basis audit trail in
  output. Deliberate divergence from TS: PASS obligations are not
  waivable (corrected policy model, see TECH-DEBT.md).
- `gate`: Rust-26 adds strict/advisory modes via `--strict` and
  `--advisory` flags (mutually exclusive). Default mode unchanged.
  Strict: MISSING_EVIDENCE/UNSUPPORTED treated as fail (exit 1).
  Advisory: MISSING_EVIDENCE/UNSUPPORTED informational (exit 0).
  WAIVED non-failing in all three modes. Mirrors TS flag interface.
- `gate`: Rust-28 adds `coverage_threshold` method. Reads
  `line_coverage` measurements from the measurements table (Rust-27
  plumbing). Filters by target path prefix, computes average, compares
  against threshold with operator (default `>=`). Evidence includes
  avg_coverage, threshold, operator, files_measured.
- `gate`: Rust-29 adds `complexity_threshold` method. Reads
  `cyclomatic_complexity` measurements, finds max across matching
  functions (prefix filter), compares against threshold with operator
  (default `<=`). Evidence: max_complexity, threshold, operator,
  functions_measured. Strict parsing on malformed measurement JSON.
  Uses `starts_with` for prefix matching (TS uses `includes` which
  is arguably a bug).
- `gate`: Rust-31 adds `hotspot_threshold` method. Reads
  `hotspot_score` inferences (Rust-30 plumbing), finds max
  `normalized_score`. Target optional (whole-repo if omitted).
  Default operator `<=`. Evidence: max_hotspot_score, threshold.
  Strict parsing on malformed inference JSON. This completes the
  TS gate method set: all four methods are now ported to Rust.
- `declare`: Rust-32 adds declaration write substrate in storage
  crate. `insert_declaration` with deterministic UUID v5 UIDs
  (idempotent, INSERT OR IGNORE). `deactivate_declaration` for
  soft-delete. Supports boundary, requirement, waiver kinds.
  Deliberate divergence from TS random UIDs (see TECH-DEBT.md).
- `declare boundary`: Rust-33 adds the first governance write
  CLI command. `rgr-rust declare boundary <db> <repo> <module>
  --forbids <target> [--reason <text>]`. Uses Rust-32 storage
  substrate with semantic identity key. Idempotent (reason text
  does not affect UID). JSON output with declaration_uid, kind,
  target, forbids, inserted. Proven end-to-end: declared boundary
  is visible to `violations` command.
- `declare requirement`: Rust-34. Single-obligation requirement
  per command. Required: --version, --obligation-id, --method,
  --obligation. Optional: --target, --threshold, --operator.
  Operator validated against `>=, >, <=, <, ==`. Threshold
  validated as number. Identity: `(repo, req_id, version)` only —
  obligation text/method/target do not affect UID. Idempotent.
  Proven end-to-end: declared requirement visible to `gate`.
- `declare waiver`: Rust-35. Required: --requirement-version,
  --obligation-id, --reason. Optional: --expires-at, --created-by,
  --rationale-category, --policy-basis. Identity:
  `(repo, req_id, requirement_version, obligation_id)` — reason
  and optional fields do not affect UID. Idempotent. Proven
  end-to-end: declare boundary + declare requirement + gate FAIL
  + declare waiver + gate PASS (WAIVED with waiver_basis). Expired
  waiver correctly ignored by gate.
  This completes the governance write surface: boundary,
  requirement, and waiver can all be authored from Rust CLI.
- `declare deactivate`: Rust-36. Soft-delete by UID. Idempotent:
  nonexistent/already-deactivated UID returns `deactivated: false`,
  exit 0. Proven end-to-end: deactivated boundary removes
  violations, deactivated requirement removes gate obligations,
  deactivated waiver restores gate failure.
- `declare supersede substrate`: Rust-37. Storage-level
  `get_declaration_by_uid` and `supersede_declaration`. Atomic
  transaction: verify active → insert new (fresh UUID v4) with
  `supersedes_uid` → deactivate old. Old missing or inactive
  returns `SupersedeError`. 8 new tests including active-query
  visibility and double-supersede rejection.
- `declare supersede boundary`: Rust-38. Reads old row via
  `get_declaration_by_uid`, validates kind=boundary + active +
  parseable MODULE target key. Inherits repo_uid and module_path
  from old row. Builds replacement with new --forbids and optional
  --reason. Calls `supersede_declaration` (Rust-37 substrate).
  Proven end-to-end: old boundary produced violations, superseded
  boundary to non-imported module eliminates violations.
- Deferred: declare supersede requirement/waiver,
  multi-obligation requirements, evidence, obligations
- Deferred: measurement commands, table output, full edge-type set

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
- Linux kernel: 63k C files. Staged architecture removed primary memory
  bottleneck; runs ~18 min before runtime crash. Sharded indexing architecture
  is the target solution (see Next #1).

### Streaming/batched indexing pipeline (Linux-scale validated)
- Per-file persistence: read → extract → insertNodes → insertStagedEdges →
  insertFileSignals → discard source. Source text no longer retained across files.
- Staging tables: `staged_edges` and `file_signals` (migrations 009-010), both
  CASCADE-scoped to snapshot. Migration 010 added `package_dependencies_json`
  and `tsconfig_aliases_json` to `file_signals`.
- Resolver index built from row-at-a-time DB iterator (`queryResolverNodesIter`).
  No bulk `.all()` materialization. Peak memory = Maps only.
- Staged edges resolved in cursor-based batches (`queryStagedEdgesBatch`, default
  10K, configurable via `IndexOptions.edgeBatchSize`). Per batch: read → resolve →
  classify → persist → discard.
- Classification loads file signals per-batch from DB. Same-file symbol sets
  rebuilt from persisted nodes via `querySymbolsByFile`. No snapshot-wide
  `fileSignalsCache` on the classification hot path.
- `queryAllNodes` eliminated from all indexer call sites. Module-edge creation
  uses pre-built Maps from resolver iterator. Detector/boundary passes use
  per-file `querySymbolsByFile`.
- Dead Phase 1 in-memory maps removed: `resolverByStableKey`, `resolverByName`,
  `resolverNodeToFile`, `nodeUidToFileUid`.
- tree.delete() in try/finally across all 5 extractors (WASM heap hygiene).
- Parser reset every 5000 parses (C/C++ extractor).
- Read failures registered as FAILED file versions (not silently dropped).
- Oversized files registered as SKIPPED with isExcluded=true.
- Multi-batch seam tests: edgeBatchSize=1 and edgeBatchSize=3 verified to
  produce identical counts/breakdown vs default.
- **Validated on Linux kernel:** 63,701 files, 1,045,482 nodes, 2,045,964
  edges, 77 min. Exit code 0, clean stderr.

### Python two-pass last-definition-wins extraction
- Pre-scan determines winning (last) definition for each function/class name
  at every scope level (module root, class body).
- Shadowed definitions fully suppressed: no node, no edges, no metrics emitted.
- Matches Python runtime semantics (last same-name def at a scope shadows earlier).
- Handles decorated definitions (unwraps decorated_definition to inner def/class).
- No diagnostic channel for reporting shadowed definitions yet (TECH-DEBT).

### compile_commands.json reader
- Per-translation-unit include path and define extraction.
- Relative directory resolution against compile_commands.json location.
- Used by indexer for per-TU include resolution in C/C++ import targets.

### Linux system detector
- module_init/module_exit macro patterns.
- platform_driver registration.
- GCC constructor/destructor attributes.
- register_handler patterns.
- Emitted as `linux_system_managed` inferences. Suppresses false dead-code
  reports for kernel-managed symbols.

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

### Module discovery support module (Layer 1 — declared)
- Core model: ModuleCandidate, ModuleCandidateEvidence, ModuleFileOwnership.
  Identity anchored by repo-relative root path, not package name.
  moduleKey format: `{repoUid}:{rootPath}:DISCOVERED_MODULE`.
- Pure detectors: package.json workspaces, pnpm-workspace.yaml, Cargo.toml
  (workspace + crate), settings.gradle, pyproject.toml ([project] + [tool.poetry]).
- Pure orchestrator: dedup by root path, evidence merging, longest-prefix
  file ownership assignment, confidence propagation.
- Discovery port: narrow interface, ManifestScanner adapter.
- Migration 011: module_candidates, module_candidate_evidence, module_file_ownership.
  All CASCADE on snapshot/repo deletion.
- Source-specific evidence attribution: pnpm vs package.json patterns expanded
  separately. Evidence attributed only to the source that matched.
- Cargo workspace root with [workspace] + [package] emits crate root.
- Additive: existing directory MODULE nodes unchanged.
- 57 tests: 33 detector unit, 16 orchestrator unit, 4 scanner attribution,
  8 integration (monorepo workspace + mixed-lang + rollups + owned files).
- Validated on repo-graph: 9 candidates, 10 evidence items, 228 ownership rows.

### Module graph and file-to-module ownership (feature slice 1)
- `rgr modules list <repo>` — catalog with rollups (file count, symbol count,
  test files, evidence count, languages, has-directory-module).
- `rgr modules evidence <repo> <module>` — evidence items per module.
- `rgr modules files <repo> <module>` — owned files with language, test flag,
  assignment kind, confidence. Targeted SQL query per module, not whole-snapshot.
- `queryModuleCandidateRollups` — single SQL with LEFT JOINs.
- Deterministic languages output (GROUP_CONCAT with ORDER BY).
- All commands support --json.
- No directory MODULE replacement. No module dependency edges. No arch violations.

### Operational dependency seams (env + fs feature surface)
- Cross-language detectors (TS/JS, Python, Rust, Java, C/C++) for environment
  variable accesses and filesystem mutations. Pure functions, line-based regex.
- Identity + evidence persistence pattern:
  - env identity: `(snapshot, surface, env_name)`, evidence per source occurrence
  - fs identity: `(snapshot, surface, target_path, mutation_kind)`, evidence
    per source occurrence including dynamic-path occurrences with null FK
- Pure linkage cores: `linkEnvDependencies`, `linkFsMutations`. Multi-surface
  linkage when files are shared across surfaces. Identity dedup per surface.
- Pure cross-surface aggregator: `aggregateEnvAcrossSurfaces`,
  `aggregateFsAcrossSurfaces`, `summarizeFsDynamicEvidence` in
  `src/core/seams/module-seam-rollup.ts`. Aggregation rules:
  - env accessKind: required-if-any > optional-if-any > unknown
  - env defaultValue: first non-null in deterministic surface order
  - env hasConflictingDefaults: true if ≥2 distinct non-null defaults
  - fs union by `(target_path, mutation_kind)` with destinationPaths union
  - fs dynamic summary aggregated across surfaces (totalCount, distinctFileCount, byKind)
- Storage migrations 015 (env) + 016 (fs). Targeted per-surface query methods
  for both env and fs evidence (`querySurfaceEnvEvidenceBySurface`,
  `querySurfaceFsMutationEvidenceBySurface`).
- Feature surface in CLI:
  - `rgr surfaces show <surface>` exposes direct env + fs sections
    (envDependencies, fsMutations.literal, fsMutations.dynamic) in both
    `--json` and human output.
  - `rgr modules show <module>` exposes module-level cross-surface rollup
    (rollup.envDependencies, rollup.fsMutations) plus per-surface direct
    env + fs blocks. Human output uses a compact `Seams: env=N fs=M +K dyn`
    breadcrumb per surface; `surfaces show` is the drill-down surface for
    full per-surface tables.
- Positional comment masker (`src/core/seams/comment-masker.ts`) pre-pass
  for both env and fs detectors. Masks line, block, and JSDoc comments
  while preserving newlines (line-number stability) and string literal
  contents (fs detectors depend on literal first-arg paths). C-style and
  Python language families.
- Test files excluded from seam detection at the indexer seam-pass entry
  point. The `isTestFile` heuristic recognizes `__tests__`, `.test.`,
  `.spec.`, `/test/`, `/tests/`, plus top-level `test/`, `tests/`, and
  `__tests__/` paths (the prior heuristic missed root-level conventions).
- See `docs/cli/v1-cli.txt` for the full JSON contract and worked examples.
- See `docs/TECH-DEBT.md` for deferred items (string-literal-embedded env
  false positives, detector externalization, jdtls live-test gating, Node
  version pinning, node:sqlite evaluation).

## Next (in priority order)

### 1. Delta indexing support module
Full indexing (77 min on Linux) is a bootstrap path, not operational
flow. Delta indexing changes the amount of work, not just the startup
cost.

**Support module: delta invalidation planner**
- determine what changed (git diff, working-tree diff)
- determine what must be invalidated (reverse edge traversal)
- determine what can be reused from the parent snapshot
- classify invalidation scope:
  - file-local only (body change, no interface change)
  - outbound dependency change (import/include changes)
  - public surface change (exported symbols, signatures)
  - config/manifest change (package.json, Cargo.toml, tsconfig, compile_commands)
  - structural/module change (file moved, module root changed)
- output: affected files, affected modules, invalidation plan, reuse plan

**Feature: parent-snapshot incremental indexing**
- parent snapshot is baseline
- copy/reuse unchanged truth logically
- delete stale rows only for affected scope
- re-extract only affected files
- recompute only affected derived artifacts
- carry forward unchanged nodes, edges, file signals, module candidates
- explicit trust metadata: what was reused vs recomputed vs widened

**Why this is #1:**
- 77-minute full index is not operationally viable for iterative work
- module discovery (just shipped) provides better invalidation scoping
  (file-to-module ownership, manifest change detection)
- daemon only improves startup/flow; delta indexing reduces total work
- C/C++ header fan-out and compile context changes need explicit
  invalidation semantics, not just file-level diff

**Special considerations for C/C++:**
- header changes have wide blast radius
- compile_commands.json changes can invalidate per-TU resolution
- macro-heavy code may require conservative widening

### 2. Module discovery expansion (Layer 2 + Layer 3)
Extend the shipped Layer 1 (declared/manifest) module discovery with:
- Layer 2: operational modules (entrypoints, deploy surfaces)
- Layer 3: inferred modules (graph clustering, structural patterns)
- Kbuild/obj-y parsing for Linux kernel module boundaries
- `__init__.py` as weaker evidence (not declared-tier)

### 3. Module graph — dependency edges and violations
User-facing implementation on top of discovered modules:
- module dependency edges derived from discovered module ownership
- cross-module violation checks
- per-module rollups for dead code, boundaries, blast radius
- `rgr arch violations <repo>` augmented with discovered-module policy

### 4. Long-lived analysis daemon + daemon-backed CLI
Two-part item: support module (warmed runtime) + feature (CLI client).

Support: warmed runtime that eliminates repeated CLI bootstrap cost
(WASM grammar load, extractor initialization, SQLite open, migration
checks). Provides warm parser pool, prepared SQLite statements,
request routing (JSON-RPC over Unix socket), three concurrency lanes
(query/index/maintenance), per-repo write locks, snapshot pinning,
progress streaming, cancellation tokens.

Feature: CLI becomes a thin client (connect, send request, render
response). Auto-start daemon on first command if absent. Progress
rendering via stderr. Fallback to direct execution if daemon unavailable.

### 5. Sharded indexing architecture
The streaming pipeline handles Linux-scale repos in a single pass
(77 min, exit 0). Sharding is the next scaling tier for:
- reducing peak memory further (bounded by shard, not snapshot)
- enabling parallel shard extraction
- natural fit for build-aware C/C++ partitioning

Architecture: shard-local extraction + global integration pass.
Same pattern as Clang tooling, LSIF pipelines, large code intelligence.

**Shard partition keys** (language-dependent):
- C/C++: translation-unit shards (compile_commands.json), subsystem
  directories, Kbuild/object groups
- JS/TS: package/workspace roots
- Java: Gradle subproject or Maven module
- Python: package roots
- Monorepos: workspace manifest entries

**Invariants:**
- Global stable keys regardless of shard
- Per-TU compile context attached to source file
- Cross-shard unresolved edges are expected, not failures
- All shards in same logical snapshot
- Module/subsystem aggregation happens last

### 6. C/C++ semantic maturation
The syntax-only first slice + compile_commands.json reader + Linux
system detector are shipped. Next maturation steps:
- Header/source ownership: which .h belongs to which translation unit
- Clangd/libclang enrichment for receiver-type resolution (same
  architectural pattern as TS TypeChecker / Rust rust-analyzer)
- #include resolution against actual file paths (not just specifiers)

Remaining system/framework detectors:
- RTOS task/thread registration (FreeRTOS, Zephyr)
- IOCTL/shared-memory boundary extraction
- Driver registration patterns beyond platform_driver

### 7. CLI progress rendering
The indexer emits progress events via callback. The CLI layer should
render them to stderr. stdout remains reserved for final `--json`
output. Not implemented yet — indexing runs silently until completion.

Options:
- `--progress` flag rendering to stderr (human-readable)
- `--progress=jsonl` for machine-readable progress stream
- daemon progress streaming (once daemon exists)

### 8. Dead-code confidence stratification
Stop presenting every zero-caller symbol as equally dead. Add evidence
quality to dead-symbol reports.

Labels:
- `dead_high_confidence` — zero callers, zero imports, no unresolved pressure
- `dead_low_confidence` — zero resolved callers, but imported in files with
  high unresolved CALLS mass
- `imported_but_call_unresolved` — symbol is imported elsewhere but the
  call resolution did not connect

### 9. CLI boundary expansion (remaining)
Shell and Makefile consumer extraction shipped. Remaining adapters:
- CI configs (`.github/workflows/*.yml`, `.gitlab-ci.yml`)
- Dockerfiles (`ENTRYPOINT`, `CMD`) — deferred further

Also:
- Provider adapters for other CLI frameworks (yargs, clap, argparse)
- Cross-file Commander composition (commands registered via function params)
- Binary-identity verification for prefix matching (package.json bin field)
- Barrel-cycle normalization: separate export-only/barrel cycles from
  logic cycles in cycle reporting

### 10. Trust overlay on structural queries
Surface evidence quality on query output. Examples:
- "0 callers, low confidence: module has 4042 unresolved CALLS"
- "dead symbol candidate, low confidence: imported in 2 files but call
  target unresolved"
Turns raw graph limitations into explicit epistemic status.

### 11. Rust framework detectors
Actix-web, Axum, Rocket, Warp route handlers. Same pattern as Express
detection: post-classification pass, receiver-provenance gated. Also
enables Rust HTTP boundary provider extraction for the boundary model.

### 12. Java semantic enrichment operationalization
jdtls is operational but fragile. The remaining issues are not polish:
- Cold-start/workspace reliability
- Protocol/client completeness
- Operational determinism on large repos

### 13. Storage/state boundary model
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
