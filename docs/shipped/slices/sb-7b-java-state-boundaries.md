# SB-7B: Java State Boundaries

Status: SHIPPED (2026-05-11)
Depends:
  - `sb-7a-state-boundaries-support-substrate.md` (SHIPPED)
  - `je-1-java-resolved-callsites.md` (IMPLEMENTED)
Follow-on: SB-7B-EXT (JDBC statements, NIO Path APIs)

## Goal

Implement state-boundary extraction for Java using the SB-7A substrate. First cut is **minimal viable**: detect only APIs whose resource identity is directly representable from a single callsite with a literal string argument.

## Prerequisite

**JE-1 IMPLEMENTED (2026-05-11).**

JE-1 extended the Java extractor to emit `ResolvedCallsite` facts with:
- `resolved_module` from import binding resolution (e.g., `"java.sql"`)
- `resolved_symbol` as `ReceiverClass.methodName` (e.g., `"DriverManager.getConnection"`)
- `arg0_payload` as `StringLiteral` when first argument is a string literal
- Pre-filtering to `java.sql` module

SB-7B consumes these facts.

## Certainty Layer

**Layer 2 (Derived Architecture)**

Interprets raw Java callsite facts and translates them into explicit resource boundary edges based on deterministic binding patterns.

## Scope

### In Scope (First Cut)

**JDBC — Direct Connection Only:**
- `java.sql.DriverManager.getConnection(String url)`
- `java.sql.DriverManager.getConnection(String url, String user, String password)`

Resource key: `db:jdbc:{encoded_url}` (first argument extracted, colons URL-encoded)
Edge types: READS + WRITES (binding direction is `read_write`)

**Why only this:** Direct string argument, no provenance chain, no object tracking required.

### Explicitly Deferred (NOT First Cut)

**JDBC Statement APIs — Require Connection Provenance:**
- `Connection.prepareStatement(String sql)` — requires tracking which connection object
- `Statement.executeQuery(String sql)` — requires statement->connection chain
- `Statement.executeUpdate(String sql)` — requires statement->connection chain
- `PreparedStatement.execute*()` — requires statement object tracking

**Reason:** These APIs require tracking object identity and provenance chains. The callsite alone does not contain sufficient context to determine the database resource.

**Java IO Constructors — Require Constructor Callsite Support:**
- `new FileInputStream(String path)`
- `new FileOutputStream(String path)`
- `new FileReader(String path)`
- `new FileWriter(String path)`

**Reason:** These are constructor calls, not method calls. First cut only proceeds if Java extractor emits constructor callsites. Verify before adding to scope.

**NIO Path APIs — Require Path Provenance:**
- `Files.readAllBytes(Path)`
- `Files.write(Path, ...)`
- `Files.readAllLines(Path)`
- `Files.newInputStream(Path)`
- `Files.newOutputStream(Path)`

**Reason:** These take `Path` objects, not strings. Determining the path requires tracking `Paths.get()` or `Path.of()` calls and their arguments. Not directly representable from the callsite alone.

**Connection Pools and DataSources:**
- `DataSource.getConnection()`
- HikariCP, C3P0, etc.

**Reason:** Connection URL is typically in external configuration, not source code.

### Out of Scope (Future Slices)

- ORM (Hibernate/JPA entity operations)
- Spring Data repositories
- Spring HTTP controllers
- Indirect calls through abstraction layers

## Degradation Policy

**When argument is not a literal string:**
- Do NOT emit edge
- Silent skip (no diagnostic logged)

**When API is not recognized:**
- Do NOT emit edge
- Silent skip

**Rationale:** First cut accepts false negatives for non-literal arguments. This is a documented limitation, not a silent failure.

## Crate Layout

```
rust/crates/state-extractor/src/languages/
+-- mod.rs                    # Adapter registry (updated)
+-- typescript.rs             # Existing TS adapter
+-- python.rs                 # Existing Python adapter
+-- java.rs                   # JavaAdapter

rust/crates/state-bindings/
+-- bindings.toml             # Extended with Java entries
```

## Validation Corpus

**Fixture:** `test/fixtures/java/jdbc-callsites/`

Contains:
- `DriverManager.getConnection(String)` calls with literal URLs
- Dynamic argument cases (should NOT emit)

## Validation Commands

```bash
# 1. Build
cd rust && cargo build -p repo-graph-state-extractor

# 2. Unit tests
cargo test -p repo-graph-state-extractor java

# 3. Java extractor tests (includes JE-1 callsite tests)
cargo test -p repo-graph-java-extractor

# 4. E2E integration tests
cargo test -p repo-graph-repo-index --test sb_7b_java_integration

# 5. Index validation corpus
rmap index test/fixtures/java/jdbc-callsites ./test-artifacts/sb-7b.db

# 6. Primary validation: list state boundaries
rmap resource list ./test-artifacts/sb-7b.db jdbc-callsites

# 7. Semantic check: verify JDBC resource detected
rmap resource list ./test-artifacts/sb-7b.db jdbc-callsites --format json
# Must return DB_RESOURCE nodes with:
#   - name = "jdbc:h2:mem:testdb" (decoded for display)
#   - stable_key contains "jdbc%3Ah2%3Amem%3Atestdb" (encoded for identity)
```

## Acceptance Criteria

**Minimal first cut:**
1. `JavaAdapter` implements `LanguageStateAdapter` trait
2. `bindings.toml` contains `DriverManager.getConnection` entry with `direction = read_write`
3. `rmap resource list` returns JDBC resources for corpus
4. **Semantic:** `DriverManager.getConnection("jdbc:h2:mem:testdb")` -> DB_RESOURCE with:
   - `name` decoded for display (`jdbc:h2:mem:testdb`)
   - `stable_key` encoded for identity (`jdbc%3Ah2%3Amem%3Atestdb`)
5. `cargo test -p repo-graph-state-extractor java` — all pass
6. `cargo test -p repo-graph-repo-index --test sb_7b_java_integration` — all pass

**Negative (not required):**
- `prepareStatement` calls -> no resource (deferred)
- `Files.readAllBytes()` calls -> no resource (deferred)
- Variable-based URLs -> no resource (silent skip)

## Definition of Parity

"Parity" for this slice means:
- **API coverage:** `DriverManager.getConnection(String)` detected with literal argument
- **Resource key accuracy:** JDBC URL extracted and URL-encoded
- **Edge direction:** READS + WRITES (connection can both read and write)

NOT required:
- Statement/query detection
- File IO detection (unless constructor callsites verified)
- Variable resolution
- Connection pool detection

## Expansion Path (SB-7B-EXT)

After first cut ships, subsequent slice can add:

1. **Java IO constructors** — if constructor callsite support added
2. **NIO Path APIs** — if path provenance tracking added
3. **JDBC statements** — if connection->statement provenance tracking added

Each expansion requires substrate capability verification before scoping.

## Alternatives Considered

### A. Include JDBC statements in first cut
Rejected: Requires object provenance tracking not available in current substrate.

### B. Include NIO Files.* APIs in first cut
Rejected: Path argument requires provenance chain from `Paths.get()`. Not directly representable.

### C. Include java.io constructors unconditionally
Rejected: Depends on whether Java extractor emits constructor callsites. Verify first, then decide.

### D. Emit edges with "unknown" resource for unresolved arguments
Rejected: Creates noise. Accept false negatives instead for first cut.

### E. Log diagnostics for unresolved JDBC URLs
Rejected: Too noisy. Silent skip preferred for first cut.

## Implementation Notes (2026-05-11)

### Files Created/Modified

**New:**
- `rust/crates/state-extractor/src/languages/java.rs` — JavaAdapter
- `test/fixtures/java/jdbc-callsites/` — validation corpus

**Modified:**
- `rust/crates/state-extractor/src/languages/mod.rs` — exports JavaAdapter
- `rust/crates/state-extractor/src/adapter.rs` — registers JavaAdapter in default_registry
- `rust/crates/state-bindings/bindings.toml` — Java JDBC binding
- `rust/crates/repo-index/src/state_boundary_hook.rs` — promoted Java to Supported language

### JDBC URL Encoding

JDBC URLs contain colons (e.g., `jdbc:h2:mem:testdb`, `jdbc:postgresql://localhost:5432/db`). The `LogicalName` newtype forbids colons because stable-key segments use `:` as delimiter.

**Implementation:** URL-encode colons before creating LogicalName.
- `jdbc:h2:mem:testdb` -> `jdbc%3Ah2%3Amem%3Atestdb`
- Stable key: `<repo>:db:jdbc:jdbc%3Ah2%3Amem%3Atestdb:DB_RESOURCE`

### URL Encoding Product Contract (RESOLVED)

**Decision:** Option A — Keep encoded stable-key, decode for display.

**Implementation:**
- `stable_key` remains URL-encoded (e.g., `jdbc%3Ah2%3Amem%3Atestdb`)
- `name` field is decoded in `list_resources()` query layer (e.g., `jdbc:h2:mem:testdb`)
- Decoding happens in `storage/src/queries.rs`, keeping CLI thin
- Added `decode_url_percent_encoding()` helper with unit tests

**Rationale:**
- Stable keys are identity artifacts; delimiter-safe encoding is required
- User-facing display should not expose transport encoding
- Generic `LogicalName` contract unchanged

**Files modified:**
- `rust/crates/storage/src/queries.rs` — decode for DB_RESOURCE in `list_resources()`

### Edge Direction

Binding direction is `read_write` (JDBC connections can both read and write). The emitter produces two edges per callsite: one READS, one WRITES.

### Test Coverage

- `cargo test -p repo-graph-state-extractor languages::java` — 4 tests
- `cargo test -p repo-graph-java-extractor` — 36 tests (includes JE-1 callsite tests)
- `cargo test -p repo-graph-storage decode_url` — 4 tests (URL decoding)
- `cargo test -p repo-graph-storage list_resources_decodes` — 1 test (DB_RESOURCE display decoding)
- `cargo test -p repo-graph-repo-index --test sb_7b_java_integration` — 5 tests (E2E)

### End-to-End Integration Test

**File:** `rust/crates/repo-index/tests/sb_7b_java_integration.rs`

Tests validate:
1. Literal JDBC URL produces DB_RESOURCE node
2. `stable_key` is URL-encoded, `name` is decoded for display
3. `read_write` direction produces both READS and WRITES edges
4. Dynamic URL arguments produce no state-boundary facts
5. Multiple literal URLs produce multiple DB_RESOURCE nodes
