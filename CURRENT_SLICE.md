# CURRENT_SLICE.md

## Current Priority

Gap-closing: strengthen Layer 0–2 facts before expanding Layer 3 framework detection.

## Active Slice

None. See Execution Queue for next priority.

## Recently Shipped

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
| SB-7B | Java state boundaries | L2 | PLANNED |
| DEP-1 | Dependency reconciliation surface | L2 | PLANNED |
| FD-1A | Rust Express detector parity | L3 | PLANNED |
| FD-1B | Rust React detector parity | L3 | PLANNED |

## Why This Order

1. **PY-EXT-2** strengthens Layer 0–1 facts (callsite resolution improves all downstream)
2. **SB-7A** creates Layer 2 support substrate consumed by language-specific adapters
3. **SB-7C/7B** use SB-7A substrate for Python and Java state boundaries
4. **DEP-1** is cross-cutting query surface over existing facts
5. **FD-1A/1B** are Layer 3 hints — come after stronger fact/substrate work

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
