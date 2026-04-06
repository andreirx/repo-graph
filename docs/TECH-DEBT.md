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

## Extraction — Languages Not Yet Supported

- **Java:** glamCRM has Java code. Architecture is ready. Requires tree-sitter-java,
  Maven/Gradle reader. Blocked only on implementation effort.
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
