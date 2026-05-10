# SB-7A: State-Boundaries Support Substrate

Status: PLANNED
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

- **Language Adapter Trait:** Define `LanguageStateAdapter` trait with methods:
  - `language_id() -> &'static str`
  - `resolve_callsite(node: &ExtractedNode) -> Option<ResolvedCallsite>`
  - `supported_binding_prefixes() -> Vec<&'static str>`
- **Plugin Registration:** `register_adapter(adapter: Box<dyn LanguageStateAdapter>)` in `StateExtractor`
- **Generic Intermediate Representation:**
  - `ResolvedCallsite { callee_path: String, arguments: Vec<ArgumentValue>, source_location: Location }`
  - `ArgumentValue { kind: Literal | Variable | Unknown, value: Option<String> }`
- **Resource Classification Output:**
  - `ResourceClassification { resource_kind: FS_PATH | DB_RESOURCE | NETWORK_STREAM | Unknown, stable_key: String, confidence: f64, evidence: String }`
- **Bindings Schema Extension:** Add `[language]` table to `bindings.toml`:
  ```toml
  [[bindings]]
  language = "typescript"
  callee_pattern = "fs.readFileSync"
  resource_kind = "FS_PATH"
  argument_index = 0
  ```
- **Headless Test API:** `StateExtractor::test_classify(language: &str, callee: &str, args: &[&str]) -> ResourceClassification`

**TypeScript as First Adopter:**
- Migrate existing TS state-boundary logic to implement `LanguageStateAdapter`
- TS adapter is a validation of the substrate, not the design driver
- No TS-specific concepts leak into the canonical trait/DTO definitions

### Out of Scope

- Implementing actual Java, Python, or C++ adapters (SB-7B, SB-7C)
- Framework detectors (FD-* slices)
- Modifying `repo-graph-indexer` node/edge schema

## Crate Layout

```
rust/crates/state-extractor/
├── src/
│   ├── lib.rs                    # StateExtractor, register_adapter()
│   ├── adapter.rs                # LanguageStateAdapter trait
│   ├── callsite.rs               # ResolvedCallsite, ArgumentValue
│   ├── classification.rs         # ResourceClassification, ResourceKind
│   ├── bindings/
│   │   ├── mod.rs                # Bindings loader, matcher
│   │   ├── schema.rs             # TOML deserialization
│   │   └── matcher.rs            # Pattern matching logic
│   └── languages/
│       ├── mod.rs                # Adapter registry
│       └── typescript.rs         # TS adapter (migrated)
└── tests/
    ├── adapter_contract.rs       # Trait contract tests
    └── typescript_parity.rs      # Regression tests
```

## Prerequisites

- `java-extractor`, `python-extractor`, `cpp-extractor` exist and emit call expressions
- `compose.rs` has integration point for post-extraction state analysis
- Existing TS state-boundary logic is locatable and understood

## Validation Corpus

Repository: `test/fixtures/typescript/state-boundaries-corpus/`

Must contain:
- `fs.readFileSync` / `fs.writeFileSync` calls
- `pg` / `mysql2` database connections
- Mixed TS/JS files

## Validation Commands

```bash
# 1. Build
cd rust && cargo build -p repo-graph-state-extractor

# 2. Unit tests
cargo test -p repo-graph-state-extractor

# 3. Index validation corpus (product surface)
rmap index test/fixtures/typescript/state-boundaries-corpus ./test-artifacts/sb-7a.db

# 4. Primary validation: product surface query
rmap boundaries list ./test-artifacts/sb-7a.db state-boundaries-corpus --kind state_boundary

# 5. Verify specific resource exists (semantic check)
rmap boundaries list ./test-artifacts/sb-7a.db state-boundaries-corpus --kind state_boundary \
  | jq '.results[] | select(.resource_key | contains("config.json"))'
# Must return exactly one READS edge

# 6. Secondary diagnostic: edge count comparison (before/after refactor)
# Only if regression suspected:
sqlite3 ./test-artifacts/sb-7a.db "SELECT COUNT(*) FROM edges WHERE kind IN ('READS','WRITES')"
```

## Acceptance Criteria

1. `LanguageStateAdapter` trait compiles with documented methods
2. `TypeScriptAdapter` implements trait, passes all existing tests
3. `bindings.toml` schema accepts `language` field, existing entries default to `"typescript"`
4. Headless test API `test_classify()` works for TS bindings
5. `rmap boundaries list --kind state_boundary` returns results (not empty)
6. **Semantic example:** `fs.readFileSync('config.json')` in corpus → READS edge with `resource_key` containing `config.json`
7. Unknown callsites surfaced in extraction diagnostics (not silently dropped)
8. READS/WRITES edge count: before == after on validation corpus
9. `cargo test -p repo-graph-state-extractor` — all pass

## Definition of Parity

"Parity" for this slice means:
- **Exact edge count:** Same number of READS/WRITES edges on validation corpus
- **Exact resource keys:** Same `stable_key` values for resources
- **No new unknowns:** If TS adapter previously classified a callsite, it still classifies it

## Alternatives Considered

### A. No adapter trait, just language-specific modules
Rejected: Would duplicate binding-matcher logic across languages. Trait enforces consistent contract.

### B. JSON schema instead of TOML
Rejected: TOML is already used for bindings, switching adds migration burden.

### C. Inline language adapters in compose.rs
Rejected: Violates separation of substrate and feature. Adapters belong in state-extractor.
