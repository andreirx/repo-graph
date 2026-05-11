# DEP-1: Dependency Reconciliation Surface

Status: SHIPPED (2026-05-11)

## Implementation Notes (Phase A)

**Implemented:**
- Storage queries: `get_external_imports_for_snapshot`, `get_external_imports_with_locations`, `get_package_dependencies_for_snapshot`, `get_external_import_bindings_for_snapshot`
- Import binding resolution: callee identifiers (e.g., `useState`) resolved to import specifiers (e.g., `react`) via `resolve.rs`
- Core reconciliation: specifier normalization (npm/Cargo), declared+observed+builtin join
- Module identity: uses `canonical_root_path` (user-facing), not internal UIDs
- Ecosystem filtering: `--ecosystem npm|cargo` filters modules by `module_key` prefix
- CLI: `deps list`, `deps why` (with sample import locations), `deps drift`
- All commands support `--format json` (JSON is default and only format)
- CLI integration tests: 12 tests in `rust/crates/rgr/tests/deps_command.rs`
- Unit tests: 28 in module-queries (normalize, reconcile, resolve)

**Deferred to DEP-1B or later:**
- `dependency_class` (prod/dev/peer/optional): needs extended manifest JSON parsing
- Workspace hoisting: deps attributed only to manifest-owning module; root→child inheritance not implemented
- Empty-module surfacing: modules with no imports AND no declared deps silently excluded
- Python/Java: manifest context not attached in compose.rs (silently excluded, no diagnostic)
- Degradation diagnostics: `unowned_file`, `module_scope_inferred`, `manifest_scope_unavailable` not emitted

Depends:
  - Module truth surface exists (rust-module-parity or equivalent)
  - Unresolved import classification stable (ExternalLibraryCandidate basis codes)
  - Manifest-derived dependency sets queryable from file signals
Track: Dependencies
Follow-on: DEP-2 (explicit PACKAGE nodes), DEP-3 (transitive/lockfile analysis)

## Goal

Surface a reconciled dependency view at module/workspace level by joining:
- Declared dependencies (from manifests)
- Observed external references (from source imports)
- Runtime builtins (from ecosystem-specific lists)

This is the 80/20 move: no new storage, reconciliation query over existing facts plus CLI surface.

## Language Scope (Phase A)

**Supported:**
- JavaScript/TypeScript (package.json manifest context attached)
- Rust (Cargo.toml manifest context attached)

**Deferred:**
- Python — manifest dependency context not yet attached on Rust compose path
- Java — manifest dependency context not yet attached on Rust compose path
- C/C++ — no manifest-based dependency model

See `rust/crates/repo-index/src/compose.rs` — current match arm only handles Cargo and package.json.

## Strategic Value

An agent can read `package.json`. That is not the hard part.

The hard parts are:
- Monorepo/workspace scoping
- Declared vs actually used
- Builtin vs third-party vs unknown
- Package subpath normalization (`react/jsx-runtime` → `react`)
- Shared dependency topology across modules
- Spotting undeclared or suspicious imports
- Answering "why does this module depend on X?"

This is tedious, cross-cutting, and easy for agents to get subtly wrong.

## Certainty Layer

**Layer 0–1 (Deterministic Facts):**
- Manifest declaration exists
- Raw import/specifier exists
- Builtin list match exists

**Layer 2 (Bounded Inference):**
- Specifier normalized to package
- Module uses dependency
- Import maps to declared dependency

**Layer 3–4 (Future):**
- Framework dependency clusters
- Undeclared/unused policy warnings

This slice implements Layer 0–2 only.

## Scope

### In Scope

**Reconciled Dependency Summary per Module:**
- `declared_and_used` — in manifest AND observed in source
- `declared_but_unobserved` — in manifest, no source references found
- `observed_but_undeclared` — source references, not in manifest
- `runtime_builtins_used` — builtin/runtime modules (fs, path, std::*)
- `unknown_external_like` — external-looking specifiers, classification unclear

**Normalization Rules (Phase A):**

npm/TS/JS:
- `react` → `react`
- `react/jsx-runtime` → `react`
- `@tanstack/react-query` → `@tanstack/react-query`
- `lodash/get` → `lodash`

Rust:
- `reqwest::Client` → `reqwest`
- `tokio::spawn` → `tokio`

**CLI Surfaces:**

1. `rmap deps list <db> <repo> [--module <name>]`
2. `rmap deps why <db> <repo> <package>`
3. `rmap deps drift <db> <repo>`

**Dependency Class (where manifest provides it):**
- prod / dev / peer / optional

### Out of Scope

- `FILE_DEPENDS_ON_PACKAGE` edges
- `SYMBOL_DEPENDS_ON_PACKAGE` edges
- Explicit `PACKAGE` nodes in storage (DEP-2)
- Transitive dependency graph from lockfiles (DEP-3)
- Python normalization (until manifest context attached)
- Java normalization (until manifest context attached)

## Scope Resolution Rules

How module/workspace scoping works for dependency attribution:

**Module ownership (implemented):**
- Only declared modules participate
- Inferred modules: silently excluded from output
- Files with no module owner: silently excluded (no diagnostic emitted)

**Manifest scope attribution (implemented):**
- Child package manifest declarations: attributed to that child module only
- Root workspace declarations: attributed to root module only

**Not implemented (deferred to DEP-1B):**
- Workspace hoisting: root deps are NOT visible to child modules
- `unowned_file` diagnostics
- `module_scope_inferred` flag

**Scope disagreement (implemented):**
- If file belongs to module A but imports package declared only in module B's manifest → `observed_but_undeclared` for module A
- Cross-module dependency inference is NOT done (would require DEP-2 graph edges)

**Example:**
```
monorepo/
  package.json          # declares "lodash"
  packages/
    frontend/
      package.json      # declares "react"
      src/app.tsx       # imports "react", "lodash"
```
Result for `frontend` module:
- `declared_and_used`: `react`
- `observed_but_undeclared`: `lodash` (hoisting NOT implemented)

## Degradation Policy

**Implemented:**
- Specifier normalization ambiguity: classified as `unknown_external_like` with `confidence < 0.8`
- Import classification failure: inherits `UnresolvedEdgeCategory` from indexer

**Not implemented (silent exclusion instead):**
- Python/Java modules: silently excluded from output (no `manifest_scope_unavailable` diagnostic)
- Inferred modules: silently excluded (no `module_scope_inferred` flag)
- Unowned files: silently excluded (no `unowned_file` diagnostic)

## Crate Layout

**Decision required:** Where does reconciliation logic live?

**Architectural constraint:** `rgr` is a delivery mechanism (CLI). Reconciliation is product policy. Policy must not live in the CLI command layer.

### Option A — Dedicated `deps-reconcile` crate

```
rust/crates/deps-reconcile/
├── src/
│   ├── lib.rs              # Public API
│   ├── normalize.rs        # Specifier → package normalization
│   ├── reconcile.rs        # Join declared + observed
│   ├── builtins/
│   │   ├── mod.rs
│   │   ├── node.rs         # Node.js builtins
│   │   └── rust.rs         # Rust std:: modules
│   └── summary.rs          # ModuleDependencySummary type
└── tests/
    ├── normalize_npm.rs
    ├── normalize_cargo.rs
    └── reconcile.rs
```

Benefits:
- Clean policy boundary
- Reusable by CLI, daemon, future APIs
- Testable headlessly
- Good growth path for DEP-2/DEP-3

Costs:
- Another crate boundary
- More upfront structure

### Option B — Existing query/policy layer

Location: `rust/crates/module-queries/` or adjacent support crate

`rgr` remains thin presentation only — calls into query layer, formats output.

Benefits:
- Less structural overhead
- Stays out of CLI layer
- Good if surface remains modest

Costs:
- Risk of mixing too much into query crate
- Harder extraction if it grows fast

**No recommendation locked.** Decision to be made at implementation time based on expected scope.

## CLI Contract

### `rmap deps list`

Primary answer: module-level dependency summary.

```
$ rmap deps list ./db myrepo --module frontend

Module: frontend
Manifest: packages/frontend/package.json

Declared and Used (3):
  react          prod    12 imports
  react-dom      prod     3 imports
  axios          prod     5 imports

Declared but Unobserved (1):
  lodash         prod     0 imports

Observed but Undeclared (1):
  some-internal-pkg       2 imports   [confidence: 0.6]

Runtime Builtins (2):
  fs             4 imports
  path           2 imports
```

### `rmap deps why`

Primary answer: module/workspace dependency relationship.
File-level imports are **supporting evidence only**, not the primary surface.

```
$ rmap deps why ./db myrepo axios

Package: axios
Ecosystem: npm

Used by modules:
  frontend       5 imports    declared: yes (prod)
  backend        3 imports    declared: yes (prod)

Sample imports (frontend):
  src/api/client.ts:3    import axios from 'axios'
  src/api/client.ts:45   axios.get(...)
```

### `rmap deps drift`

Anomaly report for governance.

```
$ rmap deps drift ./db myrepo

Observed but Undeclared:
  frontend    some-pkg         2 imports
  backend     debug            1 import    [likely: devDependency missing]

Declared but Unobserved:
  frontend    moment           0 imports   [candidate for removal]

Unknown External-like:
  backend     @internal/utils  3 imports   [unresolved]
```

## Prerequisites

- `compose.rs` attaches `PackageDependencySet` for JS/TS and Rust (EXISTS)
- Unresolved edge classification emits `ExternalLibraryCandidate` (EXISTS)
- Node.js builtin list (EXISTS in some form, verify coverage)
- Rust std module list (NEEDS verification or creation)
- Module truth surface queryable (EXISTS via module_candidates)

## Validation Corpus

Actual fixtures that exist:

- `test/fixtures/typescript/express-app` — Express backend with npm deps
- `test/fixtures/typescript/module-deps` — explicit dependency relationships
- `test/fixtures/typescript/monorepo-packages` — workspace with multiple packages

## Validation Commands

```bash
# 1. Build
cd rust && cargo build -p rgr

# 2. Unit tests (once implemented)
cargo test deps_reconcile

# 3. Index validation corpus
rmap index test/fixtures/typescript/express-app ./test-artifacts/dep-1.db

# 4. Primary validation: list dependencies
rmap deps list ./test-artifacts/dep-1.db express-app
# Must return non-empty, show declared_and_used including "express"

# 5. Semantic check: express is declared and used
rmap deps list ./test-artifacts/dep-1.db express-app --format json \
  | jq '.modules[0].declared_and_used[] | select(.package == "express")'
# Must return entry with usage_count > 0

# 6. Why query
rmap deps why ./test-artifacts/dep-1.db express-app "express"
# Must show: module name, import locations

# 7. Drift detection
rmap deps drift ./test-artifacts/dep-1.db express-app
# Must run without error; content depends on corpus

# 8. Runtime builtins (if corpus uses fs/path)
rmap deps list ./test-artifacts/dep-1.db express-app --format json \
  | jq '.modules[0].runtime_builtins_used'
# If corpus uses Node builtins, must show them

# 9. Unsupported language degradation
# Python-only repos: silently excluded from output (no diagnostic)
```

## Acceptance Criteria

**Normalization (Layer 1):**
1. `react/jsx-runtime` normalizes to `react`
2. `@tanstack/react-query` stays `@tanstack/react-query`
3. `lodash/get` normalizes to `lodash`
4. `tokio::spawn` normalizes to `tokio` (Rust corpus required)

**Reconciliation (Layer 2):**
5. `rmap deps list` returns non-empty for JS/TS workspace
6. `declared_and_used` includes packages in manifest AND imported
7. `declared_but_unobserved` includes packages in manifest but never imported
8. `observed_but_undeclared` includes imports not in manifest
9. `runtime_builtins_used` includes Node.js stdlib usage

**Import binding resolution:**
10. `useState` resolves to `react` via import binding lookup
11. `React.createElement` resolves to `react` via receiver resolution

**Scoping:**
12. Child package deps attributed to child module, not root

**CLI:**
13. `rmap deps list` works with `--module` filter
14. `rmap deps why <package>` shows module relationship + file evidence
15. `rmap deps drift` reports anomalies
16. All commands support `--format json`

**Negative:**
17. No `FILE_DEPENDS_ON_PACKAGE` edges created
18. No Python/Java reconciliation attempted (deferred)

**Not implemented (deferred):**
- Unowned files: silently excluded (no `unowned_file` diagnostic)
- Python/Java: silently excluded (no `manifest_scope_unavailable` diagnostic)
- Ambiguous specifier `unknown_external_like` classification (returns raw specifier instead)

## Definition of Parity

"Parity" for this slice means:

**Reconciled dependency surface from existing Rust-path facts for JS/TS and Rust.**

NOT:
- Full package-manager resolution
- Transitive dependency graph
- Cross-language parity
- Symbol-level dependency tracking

Parity target is: what can be derived from current `compose.rs` manifest attachment + unresolved import classification.

## Alternatives Considered

### A. Add DEPENDS_ON edges from every file
Rejected: Too noisy, duplicates what agent infers from imports, low value.

### B. Start with explicit PACKAGE nodes
Deferred to DEP-2: Reconciliation query provides value without new storage.

### C. Include Python/Java in Phase A
Rejected: Manifest context not attached on Rust path. Would require lying about coverage.

### D. New crate immediately
Deferred: Start in rgr command, extract if >500 LOC or DEP-2 needs reuse.

## Follow-on Slices

**DEP-2: Explicit Dependency Nodes**
- Add `PACKAGE` and `RUNTIME_MODULE` node kinds
- Add `MODULE_DECLARES_DEPENDENCY` edges (deterministic)
- Add `MODULE_USES_DEPENDENCY` edges (Layer 2)

**DEP-3: Transitive and Lockfile Analysis**
- Parse lockfiles for installed versions
- Build transitive dependency graph
- Version conflict detection

**DEP-1B: Python/Java Manifest Context**
- Extend `compose.rs` to attach dependency sets for Python (pyproject.toml) and Java (build.gradle)
- Then extend DEP-1 reconciliation to those languages
