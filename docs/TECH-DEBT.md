# Technical Debt and Known Limitations

## Extraction — TypeScript

- Call graph resolution: 37% on self-index (syntax-only). Strong on class-heavy
  architectures, weak on SDK-heavy/functional patterns. Compiler enrichment
  (`rgr enrich`) resolves ~81% of remaining unknown receiver types via TypeChecker.
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
- **No Java framework detectors yet:** Spring annotations, JAX-RS,
  servlet/container entrypoints are unmodeled.
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

## Extraction — Languages Not Yet Supported

- **Python:** Common in data/ML pipelines and build tooling. Requires tree-sitter-python,
  pip/pyproject.toml reader.
- **C/C++:** Highest business value (Linux BSP, embedded). Requires compile_commands.json
  boundary + Clang-backed extraction. Do not start until simpler languages validate
  the architecture.

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

- `graph dead` answers "no known inbound graph edges," not "semantically unreachable."
- Registry/plugin-driven architectures produce false positives.
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
- **PROTOTYPE — regex-based**, not AST-backed. Sufficient for glamCRM validation.
  Fragile on multi-line annotations, `value=` attribute syntax, `produces`/`consumes`
  constraints. Should be rebuilt on tree-sitter-java AST.

### HTTP consumer extractor (TS/JS)
- **PROTOTYPE — line-based regex** with file-local string resolution via
  `FileLocalStringResolver`. Resolves base-URL constants (glamCRM 93.9% consumer
  match rate, up from 80.5% before resolver).
- **FileLocalStringResolver scope (v1):**
  - Same-file only. Does not follow imports.
  - Top-level `const` declarations only. No `let`, `var`, destructuring.
  - String literals, template literals, binary `+` concatenation.
  - References to previously-resolved same-file constants (chained bindings).
  - Environment variable placeholders (`import.meta.env.*`, `process.env.*`)
    treated as opaque prefix, stripped from resolved value.
  - No function calls, no object property access, no computed expressions.
  - Bounded recursion (max 10 steps) for cycle safety.
- **Remaining consumer gaps (glamCRM 5 of 82):**
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
- No Express route provider extractor yet (TS-only repos have no provider facts).
