# Roadmap

Operational product roadmap. Ordered by engineering priority, not aspiration.
See `docs/VISION.md` for long-term direction and horizon model.
See `docs/TECH-DEBT.md` for known limitations and test gaps.

## Current state (as of last commit)

- **785 tests** across 43 test files. All passing.
- **Languages:** TypeScript/JavaScript + Rust. Multi-extractor indexer.
- **Classifier version:** 4 (language-aware imports).
- **6-repo smoke set:** amodx, fraktag, glamCRM, zap-engine, zap-squad, repo-graph.
  - 3 TS-only, 2 TS+Rust, 1 TS-only (self-index).
  - Latest assessment: `smoke-runs/2026-04-06T18-45-00Z/ASSESSMENT-from-agent.md`
- **Weighted non-unknown classification rate:** ~64% (before enrichment).
- **Enrichment:** TS ~81%, Rust ~85% receiver-type resolution.
- **Call-graph reliability:** all repos LOW (thresholds 50%/85%, fraktag closest at 47%).
- **Framework detection:** Express routes (50 on fraktag), Lambda handlers (93 on amodx, 20 on glamCRM).
- **Key architectural decisions made:**
  - Nearest-owning per-file context for deps and tsconfig (not repo-root, not union).
  - Enrichment is a separate `rgr enrich` pass, not inline with indexing.
  - Edge promotion (D2c) is opt-in via `--promote`, 8-gate safety filter.
  - Blast-radius is query-time derived, not persisted.
  - Framework-boundary detection is a post-classification pass, not embedded in the generic classifier.

## Shipped

### Multi-language graph engine
- TypeScript/JavaScript extractor (tree-sitter, syntax-only)
- Rust extractor (tree-sitter-rust)
- Multi-extractor indexer: routes files by extension
- Language-aware manifest isolation (.rs → Cargo.toml, .ts → package.json)

### Unresolved-edge classification (classifier v4)
- 4-bucket vocabulary: external_library_candidate, internal_candidate,
  framework_boundary_candidate, unknown
- Per-file signals: import bindings, same-file symbols (subtype-aware),
  package dependencies (nearest-owning), tsconfig aliases (nearest-owning
  with extends)
- Runtime builtins: ES/Node/browser globals, Node stdlib, Rust std/core/alloc
- Language-aware import classification: relative, package dep, runtime stdlib,
  project alias, unknown
- Basis codes: per-rule audit trail on every classified edge

### Trust reporting
- `rgr trust <repo>`: reliability axes, downgrade triggers, category/classification
  counts, blast-radius breakdown, enrichment status
- `rgr trust unresolved-samples <repo>`: per-edge samples with source file, line,
  classification, basis, blast-radius, receiver type (when enriched)
- Variant A call-graph reweighting: externals excluded from denominator

### Framework detection
- Express route/middleware registration (edge-level, receiver-provenance gated)
- Lambda exported handler entrypoints (node-level, inference-based)

### Compiler enrichment
- `rgr enrich <repo>`: post-index receiver-type resolution
- TypeScript: via `ts.Program` / `TypeChecker` (~81% enrichment rate)
- Rust: via rust-analyzer LSP subprocess (~85% enrichment rate)
- Safe-subset edge promotion (`--promote`): 8-gate filter, inferred resolution

### Blast-radius axis
- Query-time derived: receiver origin + enclosing scope significance
- Scoped to unknown CALLS (low = private scope, medium = exported scope)

## Next (in priority order)

### 1. Live Rust enrichment integration test
Pin the end-to-end path: rust-analyzer startup → project warm-up →
hover requests → metadata persistence. Requires real Cargo fixture +
rust-analyzer on PATH. This is the next commit.

### 2. Nearest-Cargo-context enrichment routing
`rgr enrich` currently starts one rust-analyzer per repo root.
Nested-crate or multi-workspace repos need per-Cargo.toml enrichment
boundaries. Same nearest-owning pattern as dependency resolution.

### 3. Rust internal module path resolution
`use renderer::Camera` (without `crate::/super::/self::` prefix)
currently falls to unknown. Needs crate-root-relative module path
awareness to classify as internal_candidate.

### 4. Java extractor
Third language. glamCRM has Java code. Architecture is proven for
multi-language. Requires: tree-sitter-java grammar, Java extractor
implementing ExtractorPort, Maven/Gradle dependency reader.

### 5. Rust framework detectors
Actix-web, Axum, Rocket, Warp route handlers. Same pattern as Express
detection: post-classification pass, receiver-provenance gated.

### 6. C/C++ extractor
Highest business value (Linux BSP, embedded applications). Highest
implementation cost. Requires compile_commands.json as boundary.
Do not start until Rust and Java have validated the multi-language
architecture. Needs: Clang-backed symbol extraction,
translation-unit-aware resolution, header/source ownership policy,
macro/preprocessor caveat model.

## Deferred

### Entrypoint declarations (operational)
`rgr declare entrypoint` on the 6 smoke repos would lift dead-code
reliability out of LOW. Not a code change — an adoption step. Best
done when a team starts using rgr operationally on a repo.

### Trust-score reweighting (policy)
Call-graph reliability thresholds (50%/85%) have not been changed
since the denominator was corrected. May need recalibration after
enrichment and framework detection have stabilized.

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
