# SB-7A: State-Boundaries Support Substrate

Status: SHIPPED
Depends: None (foundational)
Follow-on: `sb-7b-java-state-boundaries.md`, `sb-7c-python-state-boundaries.md`

## Goal

Establish the architectural support substrate required to expand state-boundary extraction beyond TypeScript. This slice defines the generic plugin layout, adapter traits, and DTO contracts necessary for downstream language implementations to hook into `state-extractor`.

## Certainty Layer

**Layer 2 (Derived Architecture)**

This slice provides the foundation for interpreting raw AST nodes and emitting bounded inferences about resource usage. The substrate itself is deterministic infrastructure; the inferences it enables are Layer 2.

### Degradation Policy

When the substrate cannot classify a callsite:
- Emit `ResourceClassification::Unknown` with reason
- Do not silently drop unclassifiable callsites
- Surface unknown count in extraction diagnostics

## Scope

### In Scope

**Responsibility Boundaries (Clean Architecture):**
- **Extractors** own language parsing and callsite resolution → emit `ResolvedCallsite` facts
- **Language Adapters** own conversion from extractor facts to emitter inputs → return `StateBoundaryCallsite` DTOs
- **Emitter/Matcher** owns binding-table matching and edge emission (consumes DTOs)

**Adapter Context:**
```rust
/// File-level context passed to adapters.
pub struct AdapterContext<'a> {
    pub file_uid: &'a str,
    pub file_path: &'a str,
}
```

**Language Adapter Trait:**
```rust
pub trait LanguageStateAdapter: Send + Sync {
    /// Language this adapter handles.
    fn language(&self) -> Language;
    
    /// Convert resolved callsites to state-boundary callsites.
    /// Returns DTOs; does NOT write to emitter (clean boundary).
    fn adapt_callsites(
        &self,
        ctx: &AdapterContext<'_>,
        callsites: &[ResolvedCallsite],
    ) -> Vec<StateBoundaryCallsite>;
}
```

**Why adapter returns DTOs, not writes to emitter:**
- Adapter owns conversion only, not emission side effects
- Emitter remains outside adapter boundary
- Headless testing of adapters without emitter setup
- Python/Java adapters can be validated independently

**Adapter Registry:**
- `AdapterRegistry` struct holding registered adapters by language
- `registry.get(language) -> Option<&dyn LanguageStateAdapter>`
- Default registry with TypeScript adapter pre-registered

**Hook Integration:**
- `StateBoundaryHook` queries file language, gets adapter from registry
- Adapter returns `Vec<StateBoundaryCallsite>`
- Hook feeds DTOs to language-specific emitter (one emitter per language per snapshot)

**Diagnostic Policy (Hybrid):**
- Supported language + missing adapter → diagnostic (configuration fault)
- Unsupported language (SB-7B/SB-7C pending) → silent skip (not a fault)
- Unknown language → silent skip

**Multi-Language Emitter Architecture:**
- One `StateBoundaryEmitter` per `Language` per snapshot
- Ensures `match_form_a(..., language)` receives correct language for binding dispatch
- Drain aggregates all emitters at snapshot close

**Bindings Schema:** Already supports `language` field (no change needed).

**Headless Test API:** `adapter.adapt_callsites(ctx, callsites)` returns DTOs directly.

**TypeScript as Reference Implementation:**
- Refactor `adapt_resolved_callsite` to implement `LanguageStateAdapter` trait
- Existing `emit_from_resolved_callsites` becomes hook-level orchestration
- TS adapter validates the trait contract
- No TS-specific concepts leak into trait/DTO definitions

### Out of Scope

- Implementing actual Java, Python, or C++ adapters (SB-7B, SB-7C)
- Framework detectors (FD-* slices)
- Modifying `repo-graph-indexer` node/edge schema

## Crate Layout

```
rust/crates/state-extractor/
├── src/
│   ├── lib.rs                    # Re-exports, crate doc
│   ├── emit.rs                   # StateBoundaryEmitter (existing)
│   ├── evidence.rs               # StateBoundaryEvidence (existing)
│   ├── adapter.rs                # NEW: LanguageStateAdapter trait + AdapterRegistry
│   └── languages/
│       ├── mod.rs                # Adapter exports
│       └── typescript.rs         # TS adapter (refactor to impl trait)
└── tests/
    ├── adapter_contract.rs       # NEW: Trait contract tests
    └── typescript_parity.rs      # Regression tests (existing behavior)

rust/crates/repo-index/
└── src/
    └── state_boundary_hook.rs    # UPDATE: Use registry for dispatch
```

**Note:** `ResolvedCallsite` lives in `repo-graph-indexer::types` (shared extractor output).
`StateBoundaryCallsite` lives in `state-extractor::emit` (adapter-to-emitter input).
No new DTOs needed; existing types serve the architecture.

## Prerequisites

- `state-extractor` crate exists with `StateBoundaryEmitter` and `languages/typescript.rs`
- `state-bindings` crate exists with binding table and matcher
- `StateBoundaryHook` in `repo-index` wires extraction to state-boundary emission
- `ResolvedCallsite` type in `indexer::types` is the extractor output contract
- Existing TS state-boundary logic works end-to-end (SB-2 through SB-4 shipped)

**Follow-on dependencies for language adapters (SB-7B, SB-7C):**
- Language extractor must emit `ResolvedCallsite` facts with `Arg0Payload` classification
- Binding table must have entries for that language's APIs

## Validation Corpus

Repository: `test/fixtures/typescript/state-boundaries-corpus/`

**Scope: FS-only reference implementation.**

The corpus validates the adapter substrate using FS (filesystem) bindings only.
DB/Cache/Blob validation is deferred to language-specific slices (SB-7B, SB-7C)
where those patterns will be exercised through non-TypeScript extractors.

Must contain:
- FS read (`readFile` from `fs` / `node:fs`)
- FS write (`writeFile` from `fs` / `node:fs`)
- FS promises API (`node:fs/promises`)
- Non-FS negative case (no state-boundary edges emitted)
- URI-shaped path (`file:///...` → `normalized_url` evidence)
- Windows path (`C:\...` → `normalized_path` evidence)

## Validation Commands (CLI-only)

```bash
# 1. Build
cd rust && cargo build -p repo-graph-state-extractor -p rmap

# 2. Unit tests
cargo test -p repo-graph-state-extractor
cargo test -p repo-graph-storage list_resources  # SB-7A storage tests
cargo test -p repo-graph-rgr --test resource_command  # SB-7A CLI tests

# 3. Index corpus and verify counts
rmap index test/fixtures/typescript/state-boundaries-corpus /tmp/sb-parity.db

# 4. Product surface validation (CLI-only, no SQL)
rmap resource list /tmp/sb-parity.db state-boundaries-corpus
# Expected: count=10, total_reads=7, total_writes=3

# 5. Spot-check specific resource
rmap resource readers /tmp/sb-parity.db state-boundaries-corpus \
  "state-boundaries-corpus:fs:/etc/app.yaml:FS_PATH"
# Expected: count=1, source=loadConfig
```

**Note:** The before/after tuple diff method requires a pre-refactor baseline.
Since SB-7A is a refactor of existing functionality, the checked-in corpus
baseline (`PARITY_BASELINE.md`) serves as the acceptance reference. Future
refactors diff against this baseline.

## Acceptance Criteria

### Substrate (trait + registry)

1. `LanguageStateAdapter` trait compiles with `language()` and `adapt_callsites()` returning `Vec<StateBoundaryCallsite>`
2. `AdapterContext` struct provides `file_uid` and `file_path`
3. `AdapterRegistry` holds adapters by `Language`, provides `get(language)` lookup
4. `TypeScriptAdapter` implements trait, registered in default registry

### Integration

5. `StateBoundaryHook` gets adapter from registry, calls `adapt_callsites()`, feeds DTOs to language-specific emitter
6. One emitter per language per snapshot (multi-language correctness)
7. Hybrid diagnostic policy: supported + missing → diagnostic; unsupported → silent

### Product Surface (CLI)

8. `rmap resource list` command exists and returns resource nodes with read/write counts
9. `rmap resource list /tmp/test.db repo` returns JSON with `count`, `total_reads`, `total_writes`
10. `rmap resource list --kind FS_PATH` filters by resource kind

### Regression (Parity)

11. **Edge counts on corpus:** `total_reads=7`, `total_writes=3` via `rmap resource list`
12. **Semantic:** `/etc/app.yaml` resource has 1 reader (loadConfig)
13. `cargo test -p repo-graph-state-extractor` — all existing tests pass (33 tests)
14. `cargo test -p repo-graph-repo-index --test state_boundary_integration` — 6 tests pass

### Tests

15. `cargo test -p repo-graph-storage list_resources` — 4 storage tests pass
16. `cargo test -p repo-graph-rgr --test resource_command` — 5 CLI tests pass
17. `TypeScriptAdapter::adapt_callsites()` can be called without emitter setup (headless)

## Definition of Parity

Parity for this refactor slice means exact equality on:

| Aspect | Validation Method |
|--------|-------------------|
| Edge count | `rmap resource list` returns `total_reads=7`, `total_writes=3` |
| Resource nodes | 10 FS_PATH nodes in corpus |
| Evidence validity | Spot-check: `file:///` path → `normalized_url`, Windows path → `normalized_path` |

**Baseline:** `test/fixtures/typescript/state-boundaries-corpus/PARITY_BASELINE.md`

### Parity Model Disclosure

Pre-refactor before/after comparison was **not captured** for SB-7A. The checked-in
CLI baseline (`PARITY_BASELINE.md`) is the **canonical acceptance reference** from
this point forward.

This baseline establishes a forward parity contract:
- Future refactors must preserve this product-visible surface
- Validation is CLI-first (`rmap resource list`), not SQL queries
- The baseline does not prove historical non-regression against pre-SB-7A code

This approach is acceptable because:
1. The major architectural issue (multi-language emitter bug) was identified and fixed
2. The substrate is now well-tested (46 state-extractor + 6 integration + 9 CLI/storage tests)
3. The remaining value of literal historical diff is low given no suspected behavior drift

## Alternatives Considered

### A. Adapter trait takes `ExtractedNode`, does resolution
Rejected: Resolution belongs in extractors, not adapters. Would duplicate logic, make adapters into mini-extractors, increase churn for no product gain. `ResolvedCallsite` already exists as the extractor output contract.

### B. No adapter trait, just language-specific modules
Rejected: Would duplicate binding-matcher logic across languages. Trait enforces consistent contract and enables registry-based dispatch.

### C. JSON schema instead of TOML
Rejected: TOML is already used for bindings, switching adds migration burden.

### D. Inline language adapters in compose.rs
Rejected: Violates separation of substrate and feature. Adapters belong in state-extractor.
