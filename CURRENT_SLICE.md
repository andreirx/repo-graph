# CURRENT_SLICE.md

## Current Priority

Gap-closing: strengthen Layer 0–2 facts before expanding Layer 3 framework detection.

## Active Slice

**PY-EXT-2: Python Extractor Depth** — PLANNED

Slice doc: `docs/slices/py-ext-2-python-extractor-depth.md`

## Execution Queue

| Slice | Scope | Layer | Status |
|-------|-------|-------|--------|
| **PY-EXT-2** | Python extractor depth | L0–1 | **NEXT** |
| SB-7A | State boundaries support substrate | L2 | PLANNED |
| SB-7C | Python state boundaries | L2 | PLANNED |
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

## Recently Completed

**Module Truth-Model Unification (rust-module-parity)** — SHIPPED 2026-05-10

- Phase 4 complete: MODULE-node fallback deprecated
- `module_candidates` is now the sole source of module topology
- Umbrella splitting, build-file evidence, dominant language shipped
- See `docs/shipped/slices/rust-module-parity.md` for full history

**Artifact Contract Registry (ACR)** — SHIPPED

- ACR-1 through ACR-6 all complete
- Per-row freshness and provenance tracking
- Refresh pipeline consumes registry
