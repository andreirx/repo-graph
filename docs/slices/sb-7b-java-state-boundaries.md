# SB-7B: Java State Boundaries

Status: PLANNED
Depends: `sb-7a-state-boundaries-support-substrate.md` (REQUIRED)
Follow-on: None

## Goal

Implement state-boundary extraction for Java using the SB-7A substrate. Detect Java standard IO, filesystem usage, and JDBC database touchpoints, emitting READS/WRITES edges to resource nodes.

## Certainty Layer

**Layer 2 (Derived Architecture)**

Interprets raw Java AST nodes and translates them into explicit resource boundary edges based on deterministic binding patterns.

### Degradation Policy

When a callsite matches a known API but arguments are unresolvable:
- Emit edge with `resource_key = "unknown:{api_name}"`
- Set `confidence = 0.5`
- Log to extraction diagnostics: "unresolved argument in {file}:{line}"

When a callsite is to an unrecognized API:
- Do not emit edge
- Do not log (too noisy)

## Scope

### In Scope

**Filesystem APIs (first cut):**
- `java.io.FileInputStream.<init>(String)`
- `java.io.FileOutputStream.<init>(String)`
- `java.io.FileReader.<init>(String)`
- `java.io.FileWriter.<init>(String)`
- `java.nio.file.Files.readAllBytes(Path)`
- `java.nio.file.Files.write(Path, byte[])`
- `java.nio.file.Files.readAllLines(Path)`

**JDBC APIs (first cut):**
- `java.sql.DriverManager.getConnection(String)`
- `java.sql.Connection.prepareStatement(String)`
- `java.sql.Statement.executeQuery(String)`
- `java.sql.Statement.executeUpdate(String)`

**Call Provenance:**
- Track call location (file, line, column)
- Track containing method/class
- Do NOT track constructors or static initializers for first cut

**False-Negative Policy:**
- Accept false negatives on:
  - Wrapped/abstracted IO (e.g., custom FileUtils)
  - Dynamic path construction (e.g., `new File(basePath + name)`)
  - Connection pools (HikariCP, etc.)
- Document known gaps in extraction diagnostics summary

### Out of Scope

- ORM (Hibernate/JPA entity operations)
- Spring Data repositories
- Spring HTTP controllers
- Constructor/static initializer call tracking
- Indirect calls through abstraction layers

## Crate Layout

```
rust/crates/state-extractor/src/languages/
├── mod.rs                    # Adapter registry (updated)
├── typescript.rs             # Existing TS adapter
└── java.rs                   # NEW: JavaStateAdapter

rust/crates/state-bindings/
└── bindings.toml             # Extended with Java entries
```

## Prerequisites

- SB-7A shipped: `LanguageStateAdapter` trait exists
- `java-extractor` emits `METHOD_CALL` nodes with:
  - `callee_qualified_name`
  - `arguments` array with resolvable literals
- `compose.rs` integration point available

## Validation Corpus

Repository: `legacy-codebases/java-jdbc-sample/`

Must contain:
- Direct JDBC connection (`DriverManager.getConnection`)
- Prepared statement usage
- File read/write via `java.io` or `java.nio.file`
- At least 10 files, 1000+ LOC

Fallback: Any Spring Boot starter with embedded H2 and file upload.

## Validation Commands

```bash
# 1. Build
cd rust && cargo build -p repo-graph-state-extractor

# 2. Unit tests
cargo test -p repo-graph-state-extractor java

# 3. Index validation corpus (product surface)
rmap index legacy-codebases/java-jdbc-sample ./test-artifacts/sb-7b.db

# 4. Primary validation: list state boundaries
rmap boundaries list ./test-artifacts/sb-7b.db java-jdbc-sample --kind state_boundary

# 5. Semantic check: verify specific JDBC resource
rmap boundaries list ./test-artifacts/sb-7b.db java-jdbc-sample --kind state_boundary \
  | jq '.results[] | select(.resource_key | contains("jdbc:"))'
# Must return at least one DB_RESOURCE

# 6. Semantic check: verify specific file path
rmap boundaries list ./test-artifacts/sb-7b.db java-jdbc-sample --kind state_boundary \
  | jq '.results[] | select(.resource_kind == "FS_PATH")'
# Must return at least one FS_PATH

# 7. Edge traversal validation
rmap callers ./test-artifacts/sb-7b.db java-jdbc-sample "db:jdbc:h2:mem:testdb" --edge-types READS,WRITES
```

## Acceptance Criteria

1. `JavaStateAdapter` implements `LanguageStateAdapter` trait
2. `bindings.toml` contains ≥7 Java IO entries, ≥4 JDBC entries
3. `rmap boundaries list --kind state_boundary` returns results (not empty)
4. **Semantic example:** `DriverManager.getConnection("jdbc:h2:mem:testdb")` → DB_RESOURCE with `resource_key` containing `jdbc:h2`
5. **Semantic example:** `new FileInputStream("/config/app.properties")` → FS_PATH with READS edge
6. **Negative example:** `connection.close()` → no new resource node (not a state boundary)
7. Extraction diagnostics: unresolved arguments logged, not silently dropped
8. `cargo test -p repo-graph-state-extractor` — all pass

## Definition of Parity

"Parity" for this slice means:
- **API coverage:** Listed APIs are detected when called with literal arguments
- **Resource key accuracy:** Literal paths/URLs extracted correctly
- **Edge direction:** READS for input streams, WRITES for output streams

NOT required:
- Detection of wrapped/abstracted IO
- Resolution of variable-based paths
- ORM operation mapping

## Alternatives Considered

### A. Include ORM detection
Rejected: Hibernate/JPA has complex entity-to-table mapping. Needs separate slice with schema inference.

### B. Track constructor calls
Rejected: Adds complexity for marginal value. Method calls are sufficient for most state boundary analysis.

### C. Support connection pools
Rejected: Pool configuration is typically external (JNDI, Spring config). Not extractable from source alone.
