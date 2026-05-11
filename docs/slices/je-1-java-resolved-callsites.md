# JE-1: Java Resolved Callsites

Status: IMPLEMENTED (2026-05-11)
Depends: None (Java extractor exists)
Enables: SB-7B (Java state boundaries)

## Goal

Extend the Java extractor to emit `ResolvedCallsite` facts for method invocations. This is **Layer 0-1 support work** that enables downstream state-boundary extraction.

Previously the Java extractor returned `resolved_callsites: Vec::new()`. This slice adds the fact emission path.

## Certainty Layer

**Layer 0-1 (Deterministic Facts)**

This slice produces raw extracted facts. No inference, no policy interpretation.

## Scope

### In Scope

**ResolvedCallsite emission for static method calls:**
- Static method calls: `DriverManager.getConnection(...)` → emit callsite
- Imported static receiver resolution: `DriverManager` resolved via `import java.sql.DriverManager`
- String literal arg0 extraction: `"jdbc:h2:mem:testdb"` → `CallArgPayload::StringLiteral { value }`
- Pre-filtering to state-boundary-relevant modules (`java.sql`)

**Import binding utilization:**
- Java extractor already emits `ImportBinding` for imports
- This slice uses those bindings to resolve static receivers to their source module

**Emitted callsite structure:**
```rust
ResolvedCallsite {
    enclosing_symbol_node_uid: <method/constructor containing the call>,
    resolved_module: "java.sql",  // from import binding specifier
    resolved_symbol: "DriverManager.getConnection",  // receiver.method
    arg0_payload: CallArgPayload::StringLiteral { value: "jdbc:h2:mem:testdb".to_string() },
    arg1_payload: None,  // Java JDBC doesn't use arg1
    source_location: <source location>,
}
```

### Emission Rules (Silent Skip)

**No `ResolvedCallsite` emitted when:**
- arg0 is not a string literal (variable, method call, concatenation) → silent skip
- arg0 is absent → silent skip
- Receiver is not a simple identifier (chained access like `System.out`) → silent skip
- Receiver has no matching import binding → silent skip
- Resolved module is not in `STATE_BOUNDARY_MODULES` → silent skip

This is the correct behavior per the `CallArgPayload` contract: only `StringLiteral` and `EnvKeyRead` variants exist. There is no `Dynamic` or `Absent` variant.

### Explicitly Deferred

**Instance method calls:**
- `conn.prepareStatement(...)` where `conn` is an instance variable
- Requires receiver type inference (not available in tree-sitter extraction)

**Chained calls:**
- `DriverManager.getConnection(...).prepareStatement(...)`
- Second call has non-resolvable receiver

**Constructor calls (`new Foo(...)`):**
- These emit INSTANTIATES edges, not CALLS
- Different resolution path; defer to JE-2 if needed

**Wildcard imports:**
- `import java.sql.*` does not create ImportBinding (per design doc)
- Receivers from wildcard imports cannot be resolved

## Resolution Strategy

### Static Receiver Resolution

For `DriverManager.getConnection(url)`:

1. Extract receiver text: `"DriverManager"`
2. Verify receiver is a simple identifier (not chained)
3. Look up in `import_bindings` by identifier
4. If found AND module is in `STATE_BOUNDARY_MODULES`: proceed
5. Otherwise: silent skip (no emission)

### Argument Extraction

For the arguments list, extract arg0:

1. Find `arguments` child of `method_invocation`
2. Get first non-punctuation child
3. If `string_literal`: extract text, strip quotes → `Some(StringLiteral { value })`
4. Otherwise: `None` → no callsite emitted

## Implementation

**Modified:** `rust/crates/java-extractor/src/extractor.rs`

All resolution logic is inline in `extractor.rs` (no separate module).

### Key Functions

**`try_resolve_static_callsite_java`** — attempts to build a `ResolvedCallsite`:
- Returns `Some(callsite)` only when all conditions are met
- Returns `None` for silent skip

**`classify_arg0_payload_java`** — extracts string literal from first argument:
- Returns `Some(CallArgPayload::StringLiteral { value })` for string literals
- Returns `None` otherwise

### State-Boundary Module Filter

```rust
const STATE_BOUNDARY_MODULES: &[&str] = &[
    "java.sql", // JDBC (DriverManager, Connection, Statement)
];
```

Only calls to imported receivers from these modules emit `ResolvedCallsite`.

## Validation Corpus

**Created:** `test/fixtures/java/jdbc-callsites/src/main/java/com/example/App.java`

Contains:
- `connectLiteral()` — string literal arg, emits callsite
- `connectWithCredentials()` — string literal arg with multiple params, emits callsite
- `connectVariable(String url)` — dynamic arg, no callsite emitted
- `connectFromConfig()` — method call result arg, no callsite emitted

## Validation Commands

```bash
# 1. Build
cd rust && cargo build -p repo-graph-java-extractor

# 2. Unit tests (primary validation)
cargo test -p repo-graph-java-extractor resolved_callsite
# Must pass all 7 tests

# 3. Full test suite (regression check)
cargo test -p repo-graph-java-extractor
# Must pass all 36 tests
```

**Note:** `resolved_callsites` flow through the extraction hook to the state-boundary emitter, not to storage. SQL-based validation requires SB-7B (which promotes Java from `Unsupported` to `Supported` in the hook).

## Acceptance Criteria

1. `JavaExtractor::extract()` returns non-empty `resolved_callsites` for static method calls with imported receivers AND string literal arg0
2. `resolved_module` matches the import specifier (e.g., `"java.sql"`)
3. `resolved_symbol` is `"ReceiverClass.methodName"` (e.g., `"DriverManager.getConnection"`)
4. `arg0_payload` is `StringLiteral { value }` when first argument is a string literal
5. No `ResolvedCallsite` emitted when arg0 is not a string literal
6. No `ResolvedCallsite` emitted for unresolved receivers
7. No `ResolvedCallsite` emitted for non-state-boundary modules
8. Existing CALLS edge emission unchanged
9. `cargo test -p repo-graph-java-extractor` — all 36 tests pass

## Negative Criteria (NOT in scope)

- Instance method calls on variables → no callsite emitted
- Constructor calls (`new Foo(...)`) → no callsite emitted
- Chained receivers (`a.b.method()`) → no callsite emitted
- Wildcard import receivers → no callsite emitted
- Non-`java.sql` modules → no callsite emitted

## Definition of Done

Java extractor emits `ResolvedCallsite` facts for static method invocations with:
- Imported receiver resolution
- String literal arg0 extraction
- Module pre-filtering to `java.sql`

This unblocks a **narrow** SB-7B (`DriverManager.getConnection(String)` only).

Does NOT include:
- State-boundary adapter
- Bindings.toml entries
- Hook promotion from Unsupported → Supported
- `rmap resource list` validation

Those belong to SB-7B.

## Follow-on

**SB-7B** (Java State Boundaries) — uses these facts to emit DB_RESOURCE nodes via binding table. First-cut scope: `DriverManager.getConnection(String)` only.

**JE-2** (if needed) — constructor callsite emission for `new FileInputStream(path)` patterns.
