# CURRENT_SLICE.md

## Current Priority

Gap-closing: strengthen Layer 0–2 facts before expanding Layer 3 framework detection.

## Active Slice

None currently. Next in queue: SB-7B (Java state boundaries) — needs slice doc rewrite first.

## Recently Shipped

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
| SB-7B | Java state boundaries | L2 | PLANNED (needs rewrite) |
| FD-1A | Rust Express detector parity | L3 | PLANNED |
| FD-1B | Rust React detector parity | L3 | PLANNED |

## Why This Order

1. **PY-EXT-2** strengthens Layer 0–1 facts (callsite resolution improves all downstream)
2. **SB-7A** creates Layer 2 support substrate consumed by language-specific adapters
3. **SB-7C** uses SB-7A substrate for Python state boundaries
4. **DEP-1** promoted ahead of SB-7B: cross-cutting query surface over existing facts, no extractor surgery required, immediate value across JS/TS and Rust repos
5. **SB-7B** demoted: current slice doc has internal contradictions (constructor scope, JDBC statement provenance, NIO Path provenance), needs rewrite before implementation
6. **FD-1A/1B** are Layer 3 hints — come after stronger fact/substrate work

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
