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
- **Quality discovery substrate:** measurement storage, AST-derived
  cyclomatic complexity, cognitive complexity, nesting depth, parameter count,
  function length, coverage, churn, hotspots, risk. Quality-policy declarations
  and policy-backed assessments shipped as governance substrate
  (`rmap declare quality-policy`, `rmap assess`). Missing discovery layer:
  snapshot-to-snapshot quality diff, quality delta surfacing in `orient`/`check`,
  risk prioritization combining complexity with churn/coverage/boundary violations.
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
  Framework detectors suppress false positives in internal dead-code substrate; they are
  NOT sufficient for public dead-code claims (see Dead-code surface withdrawal below).
- **Dead-code surface withdrawal:** All public dead-code vocabulary removed from `rmap`.
  `rmap dead` disabled, `DEAD_CODE` signal removed from orient, `DEAD_CODE_RELIABILITY`
  removed from check. Internal substrate preserved. Reintroduction blocked on measured
  execution evidence (line or function coverage). See `docs/TECH-DEBT.md` for policy.
- **Native Python extraction in `rmap`:** Rust-side tree-sitter-python extractor with
  Python import resolution through shared file-resolution model. Extracts functions,
  classes, methods, imports, calls. TS-side broader scope (constructors, variables,
  complexity metrics) not yet ported.
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

### Rust structural CLI (`rmap-structural-v1`)
- 10 commands: index, refresh, trust, callers, callees, path, imports, dead, cycles, stats
- JSON-only output with TS-compatible QueryResult envelopes
- Exact symbol resolution (SYMBOL-only, 3-step)
- Edge-type filters on callers/callees (CALLS, INSTANTIATES)
- Shortest-path BFS (CALLS+IMPORTS, depth 8)
- Module-level cycle detection, structural metrics, dead-code analysis
- Cross-runtime interop proven (19 tests: Rust writes, TS reads + formats)
- Contract tests, envelope tests, per-command deterministic test matrices
- Milestone doc: `docs/milestones/rmap-structural-v1.md`
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
  CLI command. `rmap declare boundary <db> <repo> <module>
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
- `declare supersede requirement`: Rust-39. Reads old row,
  validates kind=requirement + active + parseable value_json
  with req_id and version. Inherits repo_uid, req_id, version
  from old row. Builds replacement with new obligation. Proven
  end-to-end: old requirement targeting src/core (PASS) superseded
  to target src/adapters (FAIL) — gate sees replacement only.
- `declare supersede waiver`: Rust-40. Reads old row, validates
  kind=waiver + active + parseable value_json with req_id,
  requirement_version, obligation_id. Inherits identity from old
  row. Builds replacement with new reason and optional fields.
  Proven end-to-end: reason update visible in gate waiver_basis,
  supersede to expired expiry restores gate failure.
  This completes the declaration lifecycle surface: create,
  deactivate, and supersede for all three governance kinds.
- Deferred: multi-obligation requirements, evidence, obligations
- Deferred: measurement commands, table output, full edge-type set

### State-boundary extraction (`rmap-state-boundaries-v1`)
- Fork 1 (TS-only) delivered: TS/TSX/JS/JSX state-boundary edges
- New node kinds: DB_RESOURCE, FS_PATH, BLOB, STATE+CACHE subtype
- Edge types: READS, WRITES (targeting resource nodes)
- Form A matcher: import-anchored calls with literal arguments
- Binding table: TS/JS entries for fs, node:fs, fs/promises,
  node:fs/promises (stdlib FS only). SDK/DB/cache entries deferred
  pending object-property extraction and constructor tracking.
- CLI: `--edge-types READS,WRITES` on callers/callees, resource kinds
  excluded from `rmap dead`, `rmap resource readers/writers` commands
- Contract frozen at `state_boundary_version: 1`
- Milestone doc: `docs/milestones/rmap-state-boundaries-v1.md`
- Built across SB-0 through SB-6 (7 slices, plus SB-3-pre prerequisite)

### Multi-language graph engine
- TypeScript/JavaScript extractor (tree-sitter, syntax-only)
- Rust extractor: TS-side (tree-sitter-rust) for `rgr`, Rust-side (native
  tree-sitter) for `rmap`. Both share extraction scope: structs, enums,
  traits, impl methods, functions, consts, statics, type aliases, use
  imports, call edges, implements edges.
- Python extractor: TS-side (tree-sitter-python) for `rgr`, Rust-side (native
  tree-sitter) for `rmap`. Rust-side scope: functions, classes, methods,
  imports, calls. Python import resolution through shared file-resolution
  model. TS-side has broader scope (constructors, variables, complexity
  metrics) not yet ported to Rust.
- Java extractor (tree-sitter-java)
- Multi-extractor indexer: routes files by extension
- Language-aware manifest isolation (.rs → Cargo.toml, .java → build.gradle, .ts → package.json)
- **Cargo dependency resolution (Slice A):** `rmap index` resolves nearest-
  ancestor Cargo.toml for Rust files. Hyphen-to-underscore normalization.
  Handles dependencies, dev-dependencies, build-dependencies sections.
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

### C include resolution v1.2 (angle-bracket + configured roots)
- **v1.1** (conventional roots): same-dir → configured → conventional
  (`include/`, `inc/`, `src/include/`). Works for quoted-include codebases
  (swupdate, sqlite).
- **v1.2** (angle-bracket): both quoted and angle-bracket includes attempt
  resolution. Same-dir remains quote-only. Angle-bracket proceeds to
  configured/conventional roots.
- CLI: `--include-root <path>` for project-specific include directories.
- Validated on nginx: requires explicit roots (`--include-root src/core`
  etc.) because nginx has no conventional root directories.
- nginx v1.2 results: zero-connectivity modules 13→0, unresolved imports
  1,079→294, `src/core` fan_in 0→13.
- Trust labeling: ambiguous matches labeled `IMPORTS (ambiguous match)`.
- See `docs/milestones/c-include-resolution-v1.2.md` for design.

### Hotspot presentation filtering
- `--exclude-tests`: removes files with `is_test=true` (scanner-persisted
  metadata from path patterns: `.test.`, `.spec.`, `tests/`, `__tests__/`).
- `--exclude-vendored`: removes files under vendored path segments (exact
  segment match: `vendor`, `vendors`, `third_party`, `third-party`,
  `external`, `deps`, `node_modules`).
- View-policy only: filtering applied after scoring, before output.
  Stored measurements unchanged.
- Explicit opt-in: raw mode (no flags) is default, returns all hotspots.
- Output envelope includes `filtering` metadata when flags active (omitted
  when no flags). Per-filter counts: `excluded_tests_count`,
  `excluded_vendored_count`, `excluded_count` (union).
- Validated on swupdate (10 test files excluded, mongoose/ NOT excluded —
  correct, not a standard segment) and nginx (neutral baseline, 0 excluded).
- See `docs/milestones/hotspot-presentation-filtering.md` for design.

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

### Module discovery Layer 2 — operational modules
- Three-tier module kind: declared > operational > inferred.
- Layer 2 promotion: unattached project surfaces promote to operational
  module candidates when surface kind in {cli, backend_service, web_app, worker}
  and max evidence confidence >= 0.70.
- Confidence ceiling: Layer 2 modules capped at 0.85 (Layer 1 reaches 1.0).
- Exact-root collision dedup: surfaces at existing declared roots are not promoted.
- Same-pass re-linkage: after promotion, surfaces re-link to extended candidate set.
- Kind-precedence tiebreak: when declared and operational candidates share root,
  declared wins for file ownership (longest-prefix still primary).
- Pure orchestrator: `promoteUnattachedSurfaces` in `src/core/modules/operational-promotion.ts`.
- `assignFileOwnershipForCandidates` exported for pipeline use.
- PromotionDiagnostics: surfacesEvaluated, surfacesPromoted, surfacesSkippedKind,
  surfacesSkippedConfidence, promotionSkippedExistingRoot.
- 28 unit tests: 23 operational-promotion, 5 kind-precedence ownership.
- Contract: docs/architecture/module-discovery-layers.txt.

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

### Module discovery expansion (Layer 3 partial)
- **Layer 3 A1: Kbuild detector** — obj-y, obj-m, xxx-objs patterns for Linux
  kernel module boundaries. Pure detector, wired to indexer. Produces
  `kbuild` module candidates with evidence.
- **Layer 3 B1: directory detector** — heuristic module boundaries from
  directory structure (src/, lib/, packages/ with files). Pure detector,
  lower confidence than manifest-based. Fills gaps in repos without
  workspace manifests.
- **Module discovery diagnostics** — persisted via migration 017. Tracks
  skipped patterns, parse failures, confidence adjustments. CLI surface
  via `rgr modules diagnostics`.
- **Module CLI provenance visibility** — `source_type` and evidence chain
  exposed in `modules list` and `modules show` output.
- Deferred to future slice: Layer 3 C1 graph clustering, `__init__.py`
  evidence, GNU Makefile/CMake module discovery.

### Module graph — dependency edges and violations
- Cross-module IMPORTS edge derivation from file ownership.
- `rmap modules deps` command with `--outbound` / `--inbound` filters.
- `rmap modules violations` command for discovered-module boundary violations.
- `rmap violations` unified output includes both declared and discovered
  module violations in separate sections.
- Module boundary declarations via `rmap modules boundary` (semantic alias
  for `rmap declare boundary` with module-key target).
- Weighted neighbor computation (`import_count`, `source_file_count`) for
  `modules show` output.
- Violation diagnostics (`imports_edges_total`, `imports_source_no_module`,
  etc.) for degraded graph detection.
- P2/P3/P4 bug closeout: rollup degradation handling, policy parse failure
  recovery, deterministic ordering.

### Runtime/build Phase 0 — identity infrastructure
- Migration 018: `source_type`, `source_specific_id`, `stable_surface_key`
  columns on `project_surfaces` table. FK-safe table rebuild.
- `computeStableKey()` and `computeSourceSpecificId()` pure functions in
  `src/core/runtime/surface-identity.ts`.
- `DetectedSurface` now requires `sourceType` and source-specific identity
  fields. Nullable only on legacy persisted `ProjectSurface` rows.
- Fixture repair: existing detectors updated with explicit identity fields.

### Runtime/build Phase 1A — container detectors
- **Dockerfile detector** — `backend_service` or `cli` surface with
  `container` runtime. Base image runtime in evidence payload as
  `baseRuntimeKind`. CMD/ENTRYPOINT extraction.
- **docker-compose detector** — per-service surfaces from YAML. Service
  name as identity. Build context and image evidence.
- Indexer wiring: `detectDockerfileSurfaces()` and `detectDockerComposeSurfaces()`
  called during surface discovery pass.
- Validated on test fixtures.

### Runtime/build Phase 1B — script-only fallback
- **Package.json script-only fallback** — packages with `scripts` but no
  bin/main/exports/framework deps now produce a low-confidence surface.
- Detection order preserved: bin → main/exports → framework deps → scripts fallback.
- Fallback rules:
  - `scripts.start`/`dev`/`serve`/`server` → `backend_service`, confidence 0.55
  - Only build/test/lint scripts → `library`, confidence 0.50
- Evidence: one item per relevant script with `script_command` kind.
- Metadata includes `fallbackReason: "script_only_package"`.
- Unit tests (7) and integration tests (5) with dedicated fixtures.

### Runtime/build Phase 1C — Makefile v1
- **Makefile target detection** — conservative parsing, one surface per target.
- Identity: `makefilePath:targetName` prevents collapse of multiple targets.
- Recognized: `Makefile`, `makefile`, `GNUmakefile`. `*.mk` excluded.
- Target classification:
  - CLI targets: `run`, `serve`, `test`, `lint`, `install`, `deploy`, etc.
  - Library targets: `all`, `build`, `lib`, `static`, `shared`, etc.
  - Skipped: `clean`, `distclean`, pattern rules, variable-derived targets.
- RuntimeKind: `native_c_cpp` if C/C++ signals, else `node`/`python`/`unknown`.
- Diagnostics returned but not persisted (tech debt).
- Unit tests (20) and integration tests (5) with `makefile-basic/` fixture.

### Quality policy assessment surface (governance substrate)
Available governance/enforcement substrate. Not the primary product surface.
See `docs/VISION.md` §Absolute Priority for discovery-first positioning.

- `rmap declare quality-policy` — policy declarations over measurements.
  Supported policy kinds: `absolute_max`, `absolute_min`, `no_new`,
  `no_worsened`. Supported measurements: `cognitive_complexity`,
  `cyclomatic_complexity`, `function_length`, `max_nesting_depth`,
  `parameter_count`. Policies produce assessments, not mutations.
- `rmap assess` — evaluate declared policies against latest snapshot.
  Produces per-policy verdicts (PASS/FAIL/NOT_APPLICABLE/NOT_COMPARABLE).
  Comparative policies (`no_new`, `no_worsened`) require `--baseline`.
  Assessments persisted to `quality_assessments` table with computed
  verdict, evidence, and optional baseline reference.
- Baseline validation: nonexistent, wrong-repo, and non-ready baselines
  rejected at assessment time (exit 2), not silently degraded.
- Storage: quality policies stored in `declarations` table with
  `kind='quality_policy'` (shared declaration lifecycle with boundary,
  requirement, waiver kinds). `quality_assessments` is the dedicated
  assessment result table with FK to snapshot and policy declaration.

This is the enforcement substrate for teams that need CI blocking and
formal compliance workflows. Discovery (snapshot diff, quality delta
surfacing, risk prioritization) is the primary product direction.

### Rust CLI surfaces parity
- `rmap surfaces list` — surface catalog with filters (`--kind`, `--runtime`,
  `--source`, `--module`). Module enrichment via JOIN. Evidence count.
  Deterministic ordering.
- `rmap surfaces show` — surface detail with module ref, evidence items,
  parsed metadata. Multi-strategy surface ref resolution (UID, UID prefix,
  stable key, stable key prefix, display name). Ambiguity handling.
- Repo-ref resolution: supports UID, name, or root_path.
- 22 CLI regression tests covering filters, ambiguity, legacy NULL fields,
  invalid metadata preservation.

### Runtime/build v1 closeout
- **Status:** COMPLETE. 196 tests passing.
- **Closeout doc:** `docs/validation/runtime-build-v1-closeout.md`
- **Deferred items (explicitly not v1):**
  - CMake File API: requires execution boundary design (running `cmake -B`)
  - Workspace membership: module discovery enrichment, not runtime surface
  - Infra roots (Terraform, Pulumi, Helm): separate deployment-surface slice
  - Makefile diagnostics persistence: tech debt for v2

### Dead-code surface withdrawal (Option D)
- **Status:** COMPLETE. All public dead-code vocabulary removed from `rmap`.
- Public surfaces withdrawn:
  - `rmap dead` command disabled (returns exit 2)
  - `DEAD_CODE` signal removed from orient
  - `DEAD_CODE_UNRELIABLE` limit removed from orient
  - `DEAD_CODE_RELIABILITY` condition removed from check
  - `dead_code_reliability` field removed from explain trust evidence
  - `dead_code` field hidden from trust JSON output
- Internal substrate preserved for future coverage-backed reintroduction
- Framework detectors and entrypoint declarations remain active for liveness
  suppression in the internal substrate
- **Binding policy:** Public dead-code surfaces blocked until measured
  execution evidence (line or function coverage) exists in the Rust product
  path. See `docs/TECH-DEBT.md` §Dead-code surface withdrawal for full policy.

## Next

### Near-term execution sequence

This section reflects dependency-aware execution order, not strategic
importance ranking. Strategic priorities (quality discovery, daemon, delta
indexing) are listed below in their own sections, but this sequence is
what actually gets built next.

**Discovery surfaces precede enforcement refinement.**
The agent needs to see what changed before it needs gate verdicts.

1. **Trust overlay on read surfaces** — DONE
   Inline trust summary in callers, callees, path, dead, modules show,
   orient, explain. Option A: only present when degraded.

2. **Dead-confidence stratification** — DONE
   Every dead-code candidate now carries explicit confidence tier (HIGH,
   MEDIUM, LOW) with reasons. Two layers: top-level repo trust summary
   and per-result dead_confidence. See `docs/cli/rmap-contracts.md` for
   output contract.

3. **Snapshot-to-snapshot quality diff**
   Compare two snapshots and surface what changed: new violations, worsened
   measurements, improved measurements. This is discovery, not enforcement.
   The agent learns "3 functions got more complex" not "gate pass/fail."

4. **Quality delta surfacing in `rmap check` and `rmap orient`**
   `check` should include top complexity/risk deltas before agent hands off.
   `orient` should include current quality-risk hotspots for focused scope.
   Focus-aware discovery: relevant quality signals for the code being touched.

5. **Comparability/identity caveats in diff output**
   When snapshots are non-comparable (metric version mismatch, identity
   drift), surface that explicitly. Honest comparison > forced verdict.

6. **Long-lived daemon** (see #3 below)
   Only after discovery surfaces are stable.

7. **Seam expansion** (event/pubsub, persistence/schema, DI, registry/plugin)
   Architectural extraction, not feature work.

---

### 1. Quality discovery surface (remaining)

**The goal is discovery, not enforcement.** See `docs/VISION.md` §Absolute
Priority. The agent needs to see what changed, what got worse, and where
the risks are. Policy enforcement exists as available substrate.

Quality-policy declarations, assessments, and baseline validation are
shipped (see Shipped section). This is available governance substrate,
not the primary next frontier.

**Trust/correctness surfacing** (execution items 1-2)
- Trust overlay on read surfaces — DONE
- Dead-confidence stratification — DONE

**Quality discovery** (execution items 3-5)
- Snapshot-to-snapshot quality diff: compare two snapshots, surface deltas.
- Quality delta surfacing in `check` and `orient`: what got worse, what
  got better, what is risky in the current scope.
- Comparability/identity caveats: when snapshots are non-comparable,
  surface that explicitly rather than forcing a verdict.

**Agent-facing surface integration**
- `rmap check` should include top complexity/risk deltas before an agent
  hands off a change.
- `rmap orient` should include current quality-risk hotspots for the
  focused module/symbol.
- `rmap risk` should combine complexity + churn + coverage gap + boundary
  violations into prioritized risk assessments.

**Available governance substrate** (shipped, not expanding)
- Quality-policy declarations (`rmap declare quality-policy`)
- Policy-backed assessments (`rmap assess`)
- Comparative policies (`no_new`, `no_worsened` kinds)
- Gate integration for CI blocking (works, not primary)

**Support module: metric expansion** (deferred until needed)
- NPath complexity for combinatorial path explosion detection.
- Measurement version/source identifiers for comparison rejection.

**Why discovery is strategically #1:**
- Agents need to see what changed before deciding what to do.
- "3 functions got more complex" is actionable discovery.
- "gate fail" is not actionable without the underlying discovery.
- Risk prioritization helps agents focus on what matters.

### 2. Documentation inventory (shipped + remaining)
Documentation files are first-class orientation evidence. The docs
themselves are the data — not a narrow ontology of extracted facts.

**Shipped:**
- `rmap docs list` — documentation inventory surface (live discovery)
- `rmap docs extract` — semantic hints extraction (secondary layer)
- `rmap orient` — `documentation.relevant_files[]` section with ranked docs
- `DocInventoryEntry` DTO shared across inventory surfaces
- Generated flag: path-based advisory hint only (MAP.md → generated)
- Relevance tiering: exact match > descendant > ancestor > root doc
- Semantic facts stored but demoted to secondary hints

**Documentation model (binding):**
- **Docs inventory is PRIMARY.** Surface doc file paths; let agents read them.
- **Generated flag is ADVISORY.** Path-convention hint, not semantic truth.
  A generated MAP.md for a module may be more useful than a root README.
- **Semantic facts are SECONDARY.** Useful for ranking/filtering, never
  canonical. Repos with docs but no semantic_facts still show their docs.
- See `docs/design/documentation-semantic-facts.md` §Two Distinct Surfaces.

**Remaining:**
- `rmap explain` integration: doc excerpts and file references
- Persisted inventory (optimization, currently live filesystem discovery)

**Implementation constraints (still binding):**

1. **`rmap docs list` must NOT derive from semantic_facts.**
   It uses documentation inventory (live discovery or future persistence).

2. **In orient, each relevant doc includes a relevance reason:**
   ```json
   {
     "path": "src/core/README.md",
     "kind": "readme",
     "generated": false,
     "reason": "module_path_match"
   }
   ```

3. **Relevance tiering (structural, not fuzzy):**
   - Tier 1 (exact match): doc path matches focus exactly
   - Tier 2 (descendant): doc is under the focused path
   - Tier 3 (ancestor): doc is an ancestor of the focused path
   - Tier 5 (root): repo-root docs as fallback
   - Within tiers: authored > generated (mild tie-breaker only)

4. **Generated flag is path-based only in inventory layer.**
   Content-based frontmatter detection belongs in semantic-fact extraction,
   not inventory classification. Do NOT add content-authoritative detection
   to the inventory surface.

### 3. Long-lived analysis daemon + daemon-backed CLI
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

**Dependency constraint:**
Daemonization hardens the execution boundary. The discovery surfaces
must stabilize first:
- Trust overlay on read surfaces — DONE
- Dead-confidence stratification — DONE
- Snapshot-to-snapshot quality diff — must complete
- Quality delta surfacing in check/orient — must complete

Only then does daemon work begin. This is correctness-before-latency
sequencing.

### 4. Delta indexing support module (infrastructure)
**Architectural principle:** Git owns historical truth. Repo-graph owns
structured current-state truth. Delta indexing is a recomputation
strategy for current-state truth, not a substitute history system.

Full indexing (77 min on Linux) is a bootstrap path, not operational
flow. Delta indexing changes the amount of work, not just the startup
cost. But it is infrastructure optimization, not product capability.

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

**Design constraints:**
- Explicitly ephemeral: optimize current-state recomputation only.
- No archival snapshot accumulation as product history.
- Retention policy biases toward latest snapshot plus minimal transient
  comparison state. A future `rmap clean` command prunes stale snapshots.
- Longitudinal analysis uses git-backed re-extraction on demand, not
  retained graph snapshots.

**Special considerations for C/C++:**
- header changes have wide blast radius
- compile_commands.json changes can invalidate per-TU resolution
- macro-heavy code may require conservative widening

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

### 8. Dead-code confidence stratification — DONE
**See execution sequence item #2 at top of Next section.**

Implemented. Every `rmap dead` result now carries:
- `trust.dead_confidence`: HIGH, MEDIUM, or LOW
- `trust.reasons[]`: stable vocabulary of degradation signals

Top-level repo trust summary retained alongside per-result confidence.
See `docs/cli/rmap-contracts.md` for full output contract.

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

### 10. Trust overlay on structural queries — DONE
**See execution sequence item #1 at top of Next section.**

Inline trust summary in callers, callees, path, dead, modules show,
orient, explain. Option A: only present when degraded. Includes:
- `summary_scope: "repo_snapshot"` for repo-level context
- `graph_basis` (CALLS, IMPORTS, CALLS+IMPORTS)
- `reliability` axes (call_graph, dead_code, import_graph, change_impact)
- `degradation_flags` and `caveats` when non-HIGH

Per-result markers (dead confidence, edge confidence) designed but
reserved for dead-confidence stratification slice.

### 11. Rust framework detectors
Actix-web, Axum, Rocket, Warp route handlers. Same pattern as Express
detection: post-classification pass, receiver-provenance gated. Also
enables Rust HTTP boundary provider extraction for the boundary model.

### 12. Java semantic enrichment operationalization
jdtls is operational but fragile. The remaining issues are not polish:
- Cold-start/workspace reliability
- Protocol/client completeness
- Operational determinism on large repos

### 13. State-boundary expansion (post-slice-1)
Slice 1 shipped (see Shipped section above). Remaining work:

**Language coverage (blocked on extractors):**
- Java state-boundary coverage — blocked on Java Rust-workspace extractor
- Python state-boundary coverage — blocked on Python Rust-workspace extractor
- Rust-language state-boundary coverage — blocked on Rust-language Rust-workspace extractor
- C++ state-boundary coverage — blocked on C++ Rust-workspace extractor

**Feature expansion:**
- Queue/event boundaries: EMITS, CONSUMES, QUEUE node kind (Kafka, SQS, SNS, RabbitMQ)
- Config/env seam: CONFIG_KEY graph emission, explicit config→resource wiring edges
- SQL-string parsing: table-level DB granularity, TABLE node kind
- ORM/repository pattern inference (Prisma, JPA, SQLAlchemy, TypeORM)
- GCP/Azure blob coverage
- Form B matching (type-enriched receiver resolution)
- Dedicated `rmap state` command for resource enumeration
- TS-runtime parity

See `docs/milestones/rmap-state-boundaries-v1.md` §Deferred for the full
SB-next-* inventory.

### 14. Policy-facts support module (design phase)
Cross-layer policy propagation extraction: status translation patterns,
retry/restart behavior, return-fate tracking, default-provenance extraction.

**Gap identified:** rgistr MAP generation now surfaces policy signals as
advisory LLM-generated hints (Policy Signals, Policy Seams). This reveals
the architectural need but does not provide deterministic extraction,
source-anchor provenance, or queryability.

**Support module scope:**
- AST-anchored extraction of status/error translation functions
- Control-flow analysis for retry loops with backoff/resume
- Return-fate tracking (result ignored, propagated, transformed)
- Default-provenance extraction from config parsing patterns
- Cross-layer edge materialization in the graph

**Extraction families (C-first proving ground):**
- `status_mapping`: function transforms one status/error code to another — **SHIPPED (PF-1)**
- `behavioral_marker`: retry loops, resume offsets — **SHIPPED (PF-2)**
- `branch_outcome`: switch/match arm produces specific result/action
- `return_fate`: function result is ignored, propagated, or transformed
- `default_provenance`: where default values originate and propagate

**Design constraints:**
- Pure support module first, then `rmap policy` CLI surface
- Deterministic extraction (same input → same output)
- Source-anchor provenance (line numbers, AST node references)
- Stored in graph for programmatic queries
- C/C++ as proving ground (swupdate codebase)

**Shipped slices:**
- **PF-1 (STATUS_MAPPING):** Status/error code translation functions.
  `rmap policy <db> <repo> --kind STATUS_MAPPING`. Validated on swupdate
  (map_channel_retcode, channel_map_curl_error, channel_map_http_code).
- **PF-2 (BEHAVIORAL_MARKER):** Retry loops and resume offsets.
  `rmap policy <db> <repo> --kind BEHAVIORAL_MARKER`. Detects RETRY_LOOP
  (loops with sleep/delay) and RESUME_OFFSET (curl CURLOPT_RESUME_FROM*).
  Validated on swupdate channel_get_file (2 retry loops + 1 resume offset).

**Next:** PF-3 (RETURN_FATE), PF-4+ (BRANCH_OUTCOME, DEFAULT_PROVENANCE).

See slice docs: `docs/slices/pf-1-status-mapping.md`, `docs/slices/pf-2-behavioral-marker.md`.
Design doc: `docs/design/policy-facts-support-module.md`.

## Deferred

### Dead-code public surface reintroduction (blocked)
**Status:** Blocked on coverage-backed evidence.

Public dead-code surfaces (`rmap dead`, `DEAD_CODE` signal, `DEAD_CODE_RELIABILITY`
check condition) are withdrawn and must NOT be reintroduced from structural graph
heuristics alone.

**Mandatory prerequisite:** Measured execution evidence in the Rust product path.
- Line coverage (lcov, cobertura), OR
- Function/call coverage (llvm-cov function-level)

**What does NOT unblock this:**
- Framework entrypoint detection (Spring, React, Axum, FastAPI)
- Entrypoint declarations (`rmap declare entrypoint`)
- Structural orphan analysis (no inbound edges)
- Trust reliability improvements

Framework detectors and entrypoint declarations are valuable for:
- Orientation (understanding what symbols are framework-managed)
- Liveness suppression (reducing false positives in internal substrate)
- Discovery (what is structurally isolated)

They are NOT sufficient proof of deadness. The distinction:
- **`rmap orphans`** (future): "not currently referenced in the graph we built"
- **`rmap dead`** (blocked): "unexecuted under measured scenarios"

See `docs/TECH-DEBT.md` §Dead-code surface withdrawal for the full policy.

### Halstead metrics
Computable and historically established, but lower priority than cognitive
complexity and NPath for the current agent-control loop. Halstead can be added
later if a concrete consumer needs volume/difficulty vocabulary. Until then,
avoid expanding the metric set just because the formula is available.

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
