# Technical Debt and Known Limitations

## Extraction — TypeScript

- Call graph resolution: 37% on self-index (syntax-only). Strong on class-heavy
  architectures, weak on SDK-heavy/functional patterns. Compiler enrichment
  (`rgr enrich`) resolves ~81% of remaining unknown receiver types via TypeChecker.
- Inherited method resolution: 11 remaining cases on FRAKTAG (diminishing returns).
- External SDK types (node_modules): not indexed.
- Destructured bindings, reassignment: not tracked.

## Extraction — Rust

- Rust extractor (tree-sitter-rust) indexes .rs files: structs, enums, traits,
  impl methods, functions, constants, statics, type aliases.
- Cargo.toml dependency reader feeds per-crate deps into the classifier.
- Rust stdlib module specifiers (std::*, core::*, alloc::*) recognized as runtime.
- Compiler enrichment via rust-analyzer LSP resolves ~85% of unknown receiver types.
- `#[cfg(...)]` conditional duplicates deduplicated (first emission wins).
- Language-aware manifest isolation: .rs files use Cargo.toml, .ts/.js use package.json.
- **Rust internal module path resolution incomplete:** `use renderer::Camera`
  (without `crate::/super::/self::` prefix) falls to `unknown`. Needs
  crate-root-relative module path awareness.
- **Nearest-Cargo-context routing missing:** `rgr enrich` sends all .rs sites
  to one repo-root rust-analyzer instance. Nested-crate or multi-workspace repos
  need per-Cargo.toml enrichment boundaries.

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
- **Live Rust enrichment integration test (next slice):**
  `resolveRustReceiverTypes()` and the Rust branch in `rgr enrich` have NO
  automated coverage. The hover parser and validator are unit-tested (34 tests),
  but the end-to-end path — rust-analyzer subprocess startup, project warm-up,
  live hover requests, failure handling, and metadata persistence — is unpinned.
  Requires a real Cargo project fixture + rust-analyzer on PATH.

### TypeScript-Specific
- `package-name extends` in tsconfig: `extends: "@tsconfig/node18"` not
  resolved (requires node_modules lookup). Near-zero impact on current repo set.

## Dead Code Trust Boundary

- `graph dead` answers "no known inbound graph edges," not "semantically unreachable."
- Registry/plugin-driven architectures produce false positives.
- See v1-validation-report.txt for the full extraction capability boundary.
