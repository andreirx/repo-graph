# CURRENT_SLICE.md

## Current Priority

Gap-closing: strengthen Layer 0–2 facts before expanding Layer 3 framework detection.

## Active Slice

None currently active. Framework detection follow-on slices completed.

## Completed Follow-on Slices

Framework detection follow-on work completed (2026-05-12).

| Slice | Type | Scope | Status |
|-------|------|-------|--------|
| FD-1A-PARITY | Validation | Rust vs TS Express detector comparison | **COMPLETED** |
| FD-SUPPORT-EXT-JSTS | Support | Unified JS/TS extension contract | **IMPLEMENTED** |
| FD-1B-EXT | Feature | React detector extension widening | **IMPLEMENTED** |
| FD-SUPPORT-3 | Support | CLI regression tests for `rmap inferences` | **IMPLEMENTED** |

Slice docs:
- `docs/slices/fd-1a-parity-validation.md`
- `docs/slices/fd-support-ext-jsts.md`
- `docs/slices/fd-1b-ext-react-extension-widening.md`
- `docs/slices/fd-support-3-inferences-cli-regression.md`

## Recently Implemented

**FD-SUPPORT-3: CLI Regression Tests for rmap inferences** — IMPLEMENTED (2026-05-12)

Slice doc: `docs/slices/fd-support-3-inferences-cli-regression.md`

### Summary

CLI-level regression tests for the `rmap inferences list` command.

### Completed

- `rust/crates/rgr/tests/inferences_command.rs` — 6 test cases
- Test cases: usage error, missing DB, repo not found, empty result, kind filter, output structure
- All tests pass

---

**FD-1B: Rust React Detector Parity** — IMPLEMENTED (2026-05-11)

Slice doc: `docs/slices/fd-1b-rust-react-detector-parity.md`

### Summary

AST-based React component and hook detection for TSX/JSX files, persisting Layer 3 inferences.

### Completed

- `react_detector.rs` — AST-based detection via tree-sitter-typescript
- `detect_react_components()` — PascalCase functions returning JSX
- `detect_react_hooks()` — builtin and custom hook usage detection
- `persist_react_inferences()` — compose-phase wiring
- Inferences persist to `inferences` table with kinds `react_component`, `react_hook_usage`
- FD-SUPPORT-2: `rmap inferences list --kind <kind>` CLI command
- E2E integration test (`fd_1b_react_integration.rs` — 5 tests)
- Validation corpus at `test/fixtures/typescript/react-frontend-corpus/`
- 10 components, 14 hooks detected from corpus (exceeds acceptance criteria)
- 10 unit tests pass

### Validation Evidence (EXECUTED)

```
rmap inferences list --kind react_component
→ 10 react_component inferences detected
→ All component styles detected (function, arrow, fc_typed)
→ Negative cases correctly produce no inferences

rmap inferences list --kind react_hook_usage
→ 14 react_hook_usage inferences detected
→ Both builtin and custom hooks detected
```

### Deferred

- Class components (`extends React.Component`)
- Component props extraction
- HOC detection
- TS prototype parity validation (not executed)

---

**FD-1A: Rust Express Detector Parity** — IMPLEMENTED (2026-05-11)

Slice doc: `docs/slices/fd-1a-rust-express-detector-parity.md`

### Summary

AST-based Express route detection for TypeScript/JavaScript files.

### Completed

- `detect_express_routes()` — AST-based detection via tree-sitter
- `route_to_surface_with_resolver()` — conversion with module resolution
- Compose-phase integration (after npm module persistence for FK)
- Path parameter normalization (`:id` → `{id}`)
- Evidence persistence (`evidence_count: 1` for all surfaces)
- Directory-boundary-safe module resolution (fixed)
- E2E integration test (`fd_1a_express_integration.rs` — 5 tests)
- Validation corpus at `test/fixtures/typescript/express-routes/`
- 16 routes detected from corpus (exceeds 5-route acceptance criteria)
- 10 unit tests pass

### Validation Evidence (EXECUTED)

```
rmap surfaces list --kind http_provider
→ 16 http_provider surfaces detected
→ evidence_count: 1 for all surfaces
→ All routes linked to npm module (FK resolved)
→ Dynamic paths correctly skipped
→ Non-Express files correctly ignored
```

### Deferred

- Handler symbol attribution (FD-1A-4)
- TS prototype parity validation (not executed)

---

**FD-SUPPORT-1: Provider-Fact / Project-Surface Write Path** — IMPLEMENTED (2026-05-11)

Slice doc: `docs/slices/fd-support-1-rust-provider-surface-write-path.md`

### Summary

Storage write path for Rust-produced framework surfaces.

### Completed

- `CreateProjectSurfaceInput` and `CreateProjectSurfaceEvidenceInput` types
- `insert_project_surface()` and `insert_project_surface_evidence()` methods
- Batch insert methods with transaction wrapping
- 7 new tests (20 total for project_surfaces CRUD)
- Round-trip validation: insert → query → fields match

## Recently Shipped

**SB-7B: Java State Boundaries** — SHIPPED (2026-05-11)

Slice doc: `docs/shipped/slices/sb-7b-java-state-boundaries.md`

### Summary

Java adapter for state-boundary extraction using SB-7A substrate.
Scope: `DriverManager.getConnection(String)` only.

### Completed

- `JavaAdapter` implementing `LanguageStateAdapter`
- Java JDBC binding in `bindings.toml` (direction = read_write)
- JDBC URL colon encoding (`jdbc:h2:...` -> `jdbc%3Ah2%3A...`) for stable-key
- URL decoding at display layer (`name` shows decoded, `stable_key` stays encoded)
- Hook promotion (Java classified as Supported)
- End-to-end validation: 2 DB resources detected via `rmap resource list`
- Automated E2E integration test (`sb_7b_java_integration.rs`)

### Deferred (requires substrate extension)

- JDBC statements (need connection->statement provenance)
- NIO Path APIs (need path provenance from `Paths.get()`)
- Java IO constructors (need constructor callsite support)

---

## Recently Implemented (Support Slices)

**JE-1: Java Resolved Callsites** — IMPLEMENTED (2026-05-11)

Slice doc: `docs/slices/je-1-java-resolved-callsites.md`

### Summary

Extended Java extractor to emit `ResolvedCallsite` facts for static method calls with imported receivers and string literal arg0.

### Scope

- Static method calls (e.g., `DriverManager.getConnection("...")`)
- Import binding resolution (receiver → module specifier)
- String literal arg0 extraction
- Pre-filtering to `java.sql` module (state-boundary-relevant)

### Validation

- 7 new unit tests for ResolvedCallsite emission
- 36 total Java extractor tests pass
- Validation corpus: `test/fixtures/java/jdbc-callsites/`

### Unblocks

SB-7B narrow first-cut (`DriverManager.getConnection(String)` only) — can now consume these facts via adapter + bindings. Broader Java state boundaries require additional substrate work.

---

**DEP-1: Dependency Reconciliation Surface** — SHIPPED (2026-05-11)

Slice doc: `docs/shipped/slices/dep-1-dependency-reconciliation-surface.md`

### Summary

Dependency reconciliation surface for joining declared dependencies (from manifests) with observed external references (from imports) to produce module-level dependency summaries.

### Key Fix (2026-05-11)

Resolved upstream signal pollution: callee identifiers (e.g., `useState`, `React.createElement`) are now resolved to their import specifiers (e.g., `react`) using `file_signals.import_bindings_json`.

### Validation

- `deps list` shows `react` as `declared_and_used` with `import_count: 2`
- `deps why react` finds both `useState` and `React.createElement` usages
- `deps drift` correctly identifies `react-dom` as unused
- 42 tests pass (12 CLI + 28 module-queries + 2 doc)

**SB-7C: Python State Boundaries** — SHIPPED (2026-05-11)

Slice doc: `docs/shipped/slices/sb-7c-python-state-boundaries.md`

### Summary

Python adapter for state-boundary extraction using SB-7A substrate.
Scope: `open(path, mode)`, `sqlite3.connect()`, `psycopg2.connect()`.

### Completed

- Phase 1: `CallArgPayload` rename + `arg1_payload` addition to `ResolvedCallsite`
- Phase 2: Python extractor `ResolvedCallsite` emission (builtin normalization, mode classification)
- Phase 3: Python bindings in `bindings.toml` (open_read/write/read_write, sqlite3, psycopg2)
- Phase 4: `PythonAdapter` implementation with mode-to-symbol normalization
- Phase 5: Test corpus + hook fix (`classify_language` promoted Python to Supported)
- End-to-end validation: 11 resources, 12 reads, 13 writes detected via `rmap resource list`

### Deferred (requires substrate extension)

- `pathlib.Path.*` methods (resource on receiver, needs `receiver_payload`)
- `mysql.connector.connect(**kwargs)` (needs keyword arg payload)
- `cursor.execute()` (needs cursor→connection provenance)

## Execution Queue

| Slice | Scope | Layer | Status |
|-------|-------|-------|--------|
| **PY-EXT-2** | Python extractor depth | L0–1 | IMPLEMENTED (functional) |
| **PY-EXT-2-PERF** | Python extractor performance validation | L0–1 | DEFERRED |
| **SB-7A** | State boundaries support substrate | L2 | **SHIPPED** |
| **SB-7C** | Python state boundaries | L2 | **SHIPPED** |
| **DEP-1** | Dependency reconciliation surface | L2 | **SHIPPED** |
| **JE-1** | Java resolved callsites | L0–1 | **IMPLEMENTED** |
| **SB-7B** | Java state boundaries | L2 | **SHIPPED** |
| FD-SUPPORT-1 | Provider-fact / project-surface write path | L2–3 | **IMPLEMENTED** |
| FD-SUPPORT-2 | Inference query surface | L3 | **IMPLEMENTED** |
| FD-1A | Rust Express detector parity | L3 | **IMPLEMENTED** |
| FD-1B | Rust React detector parity | L3 | **IMPLEMENTED** |

## Why This Order

1. **PY-EXT-2** strengthens Layer 0–1 facts (callsite resolution improves all downstream)
2. **SB-7A** creates Layer 2 support substrate consumed by language-specific adapters
3. **SB-7C** uses SB-7A substrate for Python state boundaries
4. **DEP-1** promoted: cross-cutting query surface over existing facts, immediate value across JS/TS and Rust repos
5. **JE-1** implemented: extends Java extractor to emit `ResolvedCallsite` facts
6. **SB-7B** shipped: consumes JE-1 facts via adapter + bindings
7. **FD-SUPPORT-1** implemented — Rust write path for `project_surfaces` now exists
8. **FD-1A** implemented — AST-based Express detection with evidence persistence and E2E tests
9. **FD-SUPPORT-2** implemented — `rmap inferences list` CLI command for inference query
10. **FD-1B** implemented — AST-based React component/hook detection with inference persistence

## Previously Completed

**SB-7A: State Boundaries Support Substrate** — SHIPPED 2026-05-11

- `LanguageStateAdapter` trait and `AdapterRegistry` for multi-language dispatch
- TypeScript adapter as reference implementation
- Multi-language emitter architecture (one emitter per language per snapshot)
- Hybrid diagnostic policy (supported + missing = diagnostic; unsupported = silent)
- `rmap resource list` CLI for parity validation
- Canonical forward parity baseline established
- See `docs/shipped/slices/sb-7a-state-boundaries-support-substrate.md`

**Module Truth-Model Unification (rust-module-parity)** — SHIPPED 2026-05-10

- Phase 4 complete: MODULE-node fallback deprecated
- `module_candidates` is now the sole source of module topology
- Umbrella splitting, build-file evidence, dominant language shipped
- See `docs/shipped/slices/rust-module-parity.md` for full history

**Artifact Contract Registry (ACR)** — SHIPPED

- ACR-1 through ACR-6 all complete
- Per-row freshness and provenance tracking
- Refresh pipeline consumes registry
