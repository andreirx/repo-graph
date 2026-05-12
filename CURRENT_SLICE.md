# CURRENT_SLICE.md

## Current Priority

C/C++ systems maturation: extend state-boundary extraction to C codebases.

## Recently Shipped

**C-SB-1: C State Boundaries** — SHIPPED (2026-05-12)

Slice doc: `docs/shipped/slices/c-sb-1-c-state-boundaries.md`

### Summary

C state-boundary extraction via `ResolvedCallsite` facts for `fopen`, `open`,
and `sqlite3_open*` calls with mode/flag parsing for direction classification.

### Completed

- C extractor ResolvedCallsite emission (85 unit tests pass)
- Mode parsing: `fopen("x", "r")` → `fopen_read`, etc.
- Flag parsing: `open("x", O_RDONLY)` → `open_read`
- CAdapter in state-extractor (6 unit tests pass)
- 8 C bindings in bindings.toml
- Hook promotion (C classified as Supported)
- E2E integration test (`c_sb_1_integration.rs` — 10 tests)
- Refresh-path coverage (3 tests: unchanged preservation, mixed files, dedup)

### Validation Evidence (EXECUTED)

```
# E2E integration tests (including refresh)
cargo test -p repo-graph-repo-index --test c_sb_1_integration
→ 10 tests pass
  - indexing: fopen read/write/read_write, open O_RDONLY, sqlite3_open
  - negative: dynamic path, printf
  - refresh: unchanged preservation, mixed changed/unchanged, dedup

# Test corpus validation
rmap resource list ./test-artifacts/c-sb-1.db state-boundaries-corpus
→ 9 resources (7 FS, 2 DB)
→ Directions correctly classified (read/write/read_write)

# Smoke validation on swupdate
rmap resource list ./test-artifacts/swupdate.db swupdate
→ 5 FS resources detected
→ /dev/null, /dev/urandom, /proc/cmdline, etc.
```

### Deferred

- C++ state boundaries (separate CPP-SB-1 slice)
- fread/fwrite (need file handle provenance)
- Macro-wrapped calls

## Next Priority

CPP-SB-1: C++ state boundaries (not started)

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

## Other Recently Shipped

**FD-1A: Rust Express Detector Parity** — SHIPPED (2026-05-12)
Slice doc: `docs/shipped/slices/fd-1a-rust-express-detector-parity.md`

**SB-7B: Java State Boundaries** — SHIPPED (2026-05-11)
Slice doc: `docs/shipped/slices/sb-7b-java-state-boundaries.md`

**DEP-1: Dependency Reconciliation Surface** — SHIPPED (2026-05-11)
Slice doc: `docs/shipped/slices/dep-1-dependency-reconciliation-surface.md`

**SB-7C: Python State Boundaries** — SHIPPED (2026-05-11)
Slice doc: `docs/shipped/slices/sb-7c-python-state-boundaries.md`

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

## Execution Queue

| Slice | Scope | Layer | Status |
|-------|-------|-------|--------|
| **C-SB-1** | C state boundaries ([slice](docs/shipped/slices/c-sb-1-c-state-boundaries.md)) | L2 | **SHIPPED** |
| CPP-SB-1 | C++ state boundaries ([slice](docs/slices/cpp-sb-1-cpp-state-boundaries.md)) | L2 | NOT STARTED |
| **PY-EXT-2** | Python extractor depth | L0–1 | IMPLEMENTED (functional) |
| **PY-EXT-2-PERF** | Python extractor performance validation | L0–1 | DEFERRED |
| **SB-7A** | State boundaries support substrate | L2 | **SHIPPED** |
| **SB-7C** | Python state boundaries | L2 | **SHIPPED** |
| **DEP-1** | Dependency reconciliation surface | L2 | **SHIPPED** |
| **JE-1** | Java resolved callsites | L0–1 | **IMPLEMENTED** |
| **SB-7B** | Java state boundaries | L2 | **SHIPPED** |
| FD-SUPPORT-1 | Provider-fact / project-surface write path | L2–3 | **IMPLEMENTED** |
| FD-SUPPORT-2 | Inference query surface | L3 | **IMPLEMENTED** |
| FD-1A | Rust Express detector parity | L3 | **SHIPPED** |
| FD-1B | Rust React detector parity | L3 | **IMPLEMENTED** |

### Toolchain-Aware Evidence Import Track (NEW)

| Slice | Scope | Layer | Status |
|-------|-------|-------|--------|
| NC-1 | LLVM coverage import ([slice](docs/slices/nc-1-llvm-cov-import.md)) | L2 | PLANNED |
| BC-1 | Build context import ([slice](docs/slices/bc-1-compile-commands-import.md)) | L1 | PLANNED |
| TC-1 | Snapshot/evidence provenance ([slice](docs/slices/tc-1-toolchain-inventory.md)) | L1 | PLANNED |
| AF-1 | Analyzer findings import ([slice](docs/slices/af-1-analyzer-findings-import.md)) | L3 | PLANNED |
| SE-1 | Clangd semantic enrichment ([slice](docs/slices/se-1-clangd-enrichment.md)) | L2 | PLANNED (LOW) |

**Priority order:** NC-1 > BC-1 > TC-1 > AF-1 > SE-1

**Rationale:** Coverage import (NC-1) provides immediate value for risk/liveness.
Build context (BC-1) unlocks native semantic paths. TC-1 is narrowed to snapshot/evidence
provenance only (not generic host inventory—AI agents do that live). Findings import
(AF-1) adds risk signal as artifact import. Clangd enrichment (SE-1) is expensive/volatile.

**Boundary:** Repo-graph persists evidence lineage, not host tool inventory. An AI agent
can probe "what's installed?" ad hoc. Repo-graph answers "what produced this evidence?"
and "are these snapshots comparable?"

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
