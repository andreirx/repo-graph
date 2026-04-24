# Java Rust Extractor v1

Design document for a production-grade Java extraction path in the Rust pipeline,
enabling `rmap` orientation in Java repositories.

**Status:** Phase A-B implemented, Phase C initial validation complete.

**Revision:** 2 (2026-04-24)

**Maturity:** PROTOTYPE→MATURE boundary (see validation report)

---

## 1. Problem Framing

### What orientation questions Java support must answer

An AI agent using `rmap orient` on a Java repository needs to understand:

1. **What packages/classes/methods exist** — the structural vocabulary of the codebase
2. **What imports exist** — how files connect to each other and to external dependencies
3. **Which methods call which names syntactically** — call graph skeleton even without type resolution
4. **What annotations are present** — metadata that hints at framework wiring (without interpreting it)
5. **What files/packages participate in the module graph** — file-to-module ownership

These questions can be answered with syntax-only extraction. Tree-sitter can provide
accurate structural data without a full Java type solver.

### What it explicitly does NOT answer in v1

The following require deeper semantic analysis and are out of scope:

| Gap | Why deferred |
|-----|--------------|
| **Precise overload resolution** | Requires full type solver to distinguish `foo(int)` from `foo(String)` |
| **Inherited dispatch** | Requires class hierarchy resolution |
| **Framework wiring** | Spring/CDI bean graphs require annotation interpretation + runtime model |
| **Bean graph resolution** | Requires `@Component`/`@Autowired` semantic understanding |
| **Reflection/dynamic loading** | Fundamentally opaque to static analysis |
| **Generated sources** | Unless already present in the indexed tree |
| **Maven/Gradle dependency resolution** | Requires build system model ingestion |

The extractor must not overclaim precision it cannot justify. Unresolved edges must
be explicitly reported, not silently dropped.

---

## 2. Prior Art / Current Architecture Fit

### How existing Rust extractors are structured

| Crate | Languages | Grammar | Pattern |
|-------|-----------|---------|---------|
| `ts-extractor` | typescript, tsx, javascript, jsx | tree-sitter-typescript (compiled) | ExtractorPort trait, sync extract() |
| `c-extractor` | c, cpp | tree-sitter-c, tree-sitter-cpp (compiled) | ExtractorPort trait, sync extract() |

Both crates:
- Implement `ExtractorPort` from `repo-graph-indexer`
- Use compiled tree-sitter grammars (native, not WASM)
- Return `ExtractionResult` containing nodes, edges, metrics, import_bindings
- Produce symbolic `target_key` for edges (resolution happens in indexer)
- Handle parse errors gracefully (partial trees, not failures)
- Provide `runtime_builtins` for classifier to identify stdlib symbols

### Stable abstractions vs volatile implementation details

**Stable (can depend on):**
- `ExtractorPort` trait signature
- `ExtractionResult`, `ExtractedNode`, `ExtractedEdge` DTOs
- `NodeKind`, `EdgeType`, `Resolution` enums
- `ImportBinding` shape
- Error model: `ExtractorError` for adapter failures, partial results for syntax errors

**Volatile (may change):**
- Specific metrics computed (cyclomatic, nesting, etc.)
- State-boundary callsite resolution (Fork 1 specific)
- Import binding representation details

### Where the new crate should live

```
rust/crates/java-extractor/
  Cargo.toml
  src/
    lib.rs           # Crate root, re-exports
    extractor.rs     # JavaExtractor implementation
    metrics.rs       # Complexity computation
    builtins.rs      # Java runtime builtins set
  tests/
    parity.rs        # Cross-runtime parity tests
```

The crate depends on:
- `repo-graph-indexer` (for `ExtractorPort` trait + DTOs)
- `repo-graph-classification` (for `ImportBinding`, `RuntimeBuiltinsSet`)
- `tree-sitter` + `tree-sitter-java`

It does NOT depend on:
- `repo-graph-storage` (extractors do not touch the database)
- `repo-graph-rgr` (CLI is a separate layer)

### Which ports/adapters consume extractor output

The `repo-graph-indexer` crate:
1. Routes files to extractors by language/extension
2. Collects `ExtractionResult` per file
3. Stores nodes via storage port
4. Stages edges for resolution
5. Stores metrics and import bindings

The extractor is an adapter implementing a core port. It has no knowledge of
storage schema, resolution logic, or trust computation.

---

## 3. Support Module Design

### New crate: `rust/crates/java-extractor`

```toml
# Cargo.toml
[package]
name = "repo-graph-java-extractor"
version = "0.1.0"
edition = "2021"

[dependencies]
repo-graph-indexer = { path = "../indexer" }
repo-graph-classification = { path = "../classification" }
tree-sitter = "0.24"
tree-sitter-java = "0.23"

[dev-dependencies]
# Parity test harness dependencies
```

### Public API

```rust
pub struct JavaExtractor {
    // Internal: parser, language handle
}

impl ExtractorPort for JavaExtractor {
    fn name(&self) -> &str;
    fn languages(&self) -> &[String];
    fn runtime_builtins(&self) -> &RuntimeBuiltinsSet;
    fn initialize(&mut self) -> Result<(), ExtractorError>;
    fn extract(
        &self,
        source: &str,
        file_path: &str,
        file_uid: &str,
        repo_uid: &str,
        snapshot_uid: &str,
    ) -> Result<ExtractionResult, ExtractorError>;
}
```

### Input DTOs

The extractor receives:
- `source: &str` — UTF-8 Java source text
- `file_path: &str` — repo-relative path (e.g., `src/main/java/com/example/Foo.java`)
- `file_uid: &str` — unique file identifier
- `repo_uid: &str` — repository identifier
- `snapshot_uid: &str` — snapshot being built

No file I/O inside the extractor. The indexer provides source text.

### Output DTOs

`ExtractionResult` containing:
- `nodes: Vec<ExtractedNode>` — FILE node + SYMBOL nodes
- `edges: Vec<ExtractedEdge>` — IMPORTS, CALLS, IMPLEMENTS edges
- `metrics: BTreeMap<String, ExtractedMetrics>` — per-method metrics
- `import_bindings: Vec<ImportBinding>` — import statement records
- `resolved_callsites: Vec<ResolvedCallsite>` — empty for Java v1

### Error Model

```rust
pub struct ExtractorError {
    pub message: String,
}
```

**Failure modes:**
- `initialize()` returns `Err` if grammar loading fails
- `extract()` returns `Err` only for adapter failures (null parser, uninitialized state)
- Syntax errors in source produce partial trees — extraction proceeds and returns `Ok`

### Parse-Failure / Degradation Model

Tree-sitter produces partial trees for syntactically broken source. The extractor:
1. Visits whatever nodes tree-sitter produces
2. Extracts what it can find
3. Returns `Ok(ExtractionResult)` with partial data
4. Does NOT return `Err` for syntax errors

The indexer records parse status based on whether `extract()` returns `Ok` or `Err`,
not based on tree completeness. Trust reporting surfaces unresolved-edge rates.

### Versioning / Provenance Fields

```rust
const EXTRACTOR_NAME: &str = "java-rust:0.1.0";
```

This string appears in:
- `ExtractedEdge.extractor` field (every edge carries extractor provenance)
- Trust reports (which extractors contributed to a snapshot)

Version bumps when extraction logic changes in ways that affect output shape.

---

## 4. Extraction Scope v1

### Must include

| Element | NodeKind | Notes |
|---------|----------|-------|
| Package declaration | — | Stored as qualified_name prefix |
| Import statements | — | Produce IMPORTS edges + ImportBinding |
| Classes | SYMBOL (subtype: CLASS) | Top-level and nested |
| Interfaces | SYMBOL (subtype: INTERFACE) | |
| Enums | SYMBOL (subtype: ENUM) | |
| Records | SYMBOL (subtype: CLASS) | Java 14+ records treated as class |
| Annotation types | SYMBOL (subtype: INTERFACE) | `@interface` declarations |
| Methods | SYMBOL (subtype: METHOD) | Instance and static |
| Constructors | SYMBOL (subtype: CONSTRUCTOR) | Separate subtype (see Decision #5) |
| Fields | SYMBOL (subtype: PROPERTY) | If cheap and structurally useful (see Decision #2) |
| Annotations | — | Raw names in metadata_json (see Section 7) |
| Method invocations | CALLS edges | Syntactic only |
| File node | FILE | One per file |

### Language tagging

All extracted files tagged as `language: "java"` in FILE node metadata.

---

## 5. Identity and Stability

### Stable symbol identity strategy

Symbols follow the existing extractor stable key format:
```
{repo_uid}:{file_path}#{name}:SYMBOL:{SUBTYPE}
```

Examples:
- FILE: `myrepo:src/main/java/com/example/Foo.java:FILE`
- Class: `myrepo:src/main/java/com/example/Foo.java#Foo:SYMBOL:CLASS`
- Interface: `myrepo:src/main/java/com/example/UserService.java#UserService:SYMBOL:INTERFACE`
- Method: `myrepo:src/main/java/com/example/Foo.java#doWork:SYMBOL:METHOD`
- Constructor: `myrepo:src/main/java/com/example/Foo.java#Foo:SYMBOL:CONSTRUCTOR`
- Field: `myrepo:src/main/java/com/example/Foo.java#name:SYMBOL:PROPERTY`

This matches the TS/C extractor contract exactly.

### Overloaded methods: one node per declaration

**Problem:** Java allows overloaded methods with same name but different signatures.
Without type resolution, we cannot distinguish `foo(int)` from `foo(String)` at
call sites.

**Decision:** Keep one SYMBOL node per declaration, not collapsed. Use the existing
`:dupN` disambiguation suffix for multiple declarations with the same base key.

**Representation:**
```
# First declaration of foo
myrepo:Foo.java#foo:SYMBOL:METHOD

# Second declaration of foo (overload)
myrepo:Foo.java#foo:SYMBOL:METHOD:dup2

# Third declaration of foo (overload)
myrepo:Foo.java#foo:SYMBOL:METHOD:dup3
```

**Why keep separate nodes:**
- Each declaration has its own source location, signature, annotations
- Each can be a caller/callee attachment point
- Metrics are per-declaration
- Matches existing TS/C extractor duplicate handling
- Ambiguity lives at call resolution, not symbol extraction

**Signature metadata:** Each method node includes `signature` field with
parameter types as written in source (e.g., `"(int, String)"`). This is
syntactic only but helps agents distinguish overloads.

### Snapshot-scoped UID vs stable symbol key

| Field | Scope | Purpose |
|-------|-------|---------|
| `node_uid` | Snapshot | Primary key, includes snapshot_uid prefix |
| `stable_key` | Cross-snapshot | Identity for comparison, format above |

Same model as TS/C extractors.

### Ambiguity representation

Ambiguity is represented at different layers:

1. **Overloaded methods:** Separate nodes with `:dupN` suffix; call resolution
   cannot distinguish which overload is called without type information
2. **Unresolved calls:** Symbolic edges that fail resolution remain in
   `unresolved_edges` table with classifier verdict
3. **Wildcard imports:** `import java.util.*` emits an IMPORTS edge with
   `target_key = "java.util.*"`. This edge will NOT resolve (the current
   resolver is file-oriented; there are no package nodes in v1). The edge
   goes to `unresolved_edges` with category `ImportsFileNotFound` (the
   existing category for imports that cannot be matched to a file). No
   ImportBinding is emitted (wildcard imports do not introduce a specific
   local binding). This is an explicit v1 limitation; package-level import
   resolution would require package nodes which are out of scope.

Trust reporting exposes unresolved-edge rates per file and per snapshot.

---

## 6. Call Extraction Policy

### Syntactic method call capture

The extractor captures:
- `method_invocation` nodes from tree-sitter
- `object_creation_expression` nodes (constructor calls)

### Call classification

| Pattern | Category | Example | Resolvability |
|---------|----------|---------|---------------|
| `receiver.method()` | receiver-qualified | `foo.bar()` | Unlikely (needs type of `foo`) |
| `method()` | unqualified local | `helper()` | Unlikely (could be local, inherited, or static import) |
| `new ClassName()` | constructor | `new Foo()` | Possible (class name known) |
| `ClassName.method()` | static-looking | `Math.abs()` | Possible (class name known) |

### Edge creation policy

All syntactic calls produce `ExtractedEdge` with symbolic `target_key`:

| Category | Edge type | Target key format |
|----------|-----------|-------------------|
| Constructor call | `EdgeType::Instantiates` | `{ClassName}` |
| Static-looking call | `EdgeType::Calls` | `{ClassName}.{method}` |
| Receiver-qualified | `EdgeType::Calls` | `?.{method}` (receiver unknown) |
| Unqualified local | `EdgeType::Calls` | `{method}` |

**Constructor calls use INSTANTIATES edge type.** This is consistent with how
the TS extractor handles `new ClassName()` expressions. The `EdgeType::Instantiates`
edge targets the class name; resolution matches against CLASS nodes, not
CONSTRUCTOR nodes. Constructor nodes exist as separate symbols for metrics and
annotation attachment, but instantiation edges link to the class.

**Constructor node naming:** Constructor nodes use the class name (e.g., `Foo`)
matching Java source syntax. The stable key is `#Foo:SYMBOL:CONSTRUCTOR`.
Multiple constructors (overloads) get `:dupN` suffixes.

All edges emitted with `Resolution::Static` (the edge exists in source).
The `target_key` is symbolic, not a resolved node UID.

### Staged edge resolution flow

Following the existing indexer model:

1. **Extractor emits** `ExtractedEdge` with symbolic `target_key`
2. **Indexer stages** edges in `staged_edges` table
3. **Resolution phase** attempts to match `target_key` to actual nodes
4. **Matched edges** become resolved edges in `edges` table
5. **Unmatched edges** go through classifier, stored in `unresolved_edges` with verdict

The extractor does not decide resolution status. It emits symbolic references;
the indexer pipeline handles resolution.

### Trust reporting contract

Trust reports expose:
- `unresolved_edges_count` per file
- `total_edges_count` per file
- Edge resolution rate breakdown by type (CALLS, IMPORTS, etc.)

Unresolved edges carry classifier verdicts explaining why resolution failed
(e.g., "receiver type unknown", "no matching symbol in scope").

---

## 7. Annotation Handling

### Raw annotation capture

Annotations are captured as metadata on the annotated symbol:

```json
{
  "annotations": [
    { "name": "Override" },
    { "name": "Autowired", "arguments": null },
    { "name": "RequestMapping", "arguments": { "value": "/api" } }
  ]
}
```

### No framework interpretation

The extractor does NOT:
- Interpret `@Component` as a bean definition
- Resolve `@Autowired` injection targets
- Understand `@RequestMapping` as an HTTP endpoint

This is deliberate. Annotation semantics are framework-specific and belong in
a separate Spring detector layer.

### Enabling future Spring detectors

By capturing raw annotation names and arguments:
1. Spring detectors can query nodes with specific annotations
2. Detectors can be developed independently of extractor changes
3. Extractor remains framework-agnostic

The data model supports:
```sql
SELECT * FROM nodes
WHERE metadata_json LIKE '%"name":"Component"%'
```

This is crude but sufficient for detector prototyping. Dedicated annotation
indexes are future work.

---

## 8. Storage / Integration Plan

### Mapping extractor output to existing storage

| Extractor output | Storage table | Notes |
|------------------|---------------|-------|
| `ExtractedNode` (FILE) | `nodes` | kind = FILE |
| `ExtractedNode` (SYMBOL) | `nodes` | kind = SYMBOL, various subtypes |
| `ExtractedEdge` (IMPORTS) | `staged_edges` → `edges` | After resolution |
| `ExtractedEdge` (CALLS) | `staged_edges` → `edges` | After resolution |
| `ExtractedEdge` (IMPLEMENTS) | `staged_edges` → `edges` | After resolution |
| `ExtractedMetrics` | `measurements` | Per-function metrics |
| `ImportBinding` | `import_bindings` | For classifier use |

### Schema changes required

**None required.** The existing schema supports Java extraction:
- `NodeKind::SYMBOL` with appropriate subtypes
- `EdgeType::IMPORTS`, `EdgeType::CALLS`, `EdgeType::IMPLEMENTS` exist
- `language` field on FILE nodes
- `metadata_json` for annotation storage

### Unresolved calls/imports representation

Unresolved edges follow the same pattern as other extractors:
1. Extractor emits `ExtractedEdge` with symbolic `target_key`
2. Edges inserted into `staged_edges` table
3. Indexer resolution phase attempts matching `target_key` to actual nodes
4. Matched edges: inserted into `edges` table with resolved `target_node_uid`
5. Unmatched edges: passed through classifier, stored in `unresolved_edges`
   table with classifier verdict (e.g., "receiver type unknown")
6. Trust reporting counts unresolved edges by type and verdict

### Trust/degradation reporting changes

Add Java to trust report language breakdown:
- Files indexed by language
- Unresolved edge rates by language
- Extractor provenance (java-rust:0.1.0)

No new trust dimensions required.

---

## 9. Validation Plan

### Primary validation repo: spring-petclinic

Spring PetClinic is a canonical small-to-medium Java/Spring project:
- ~50 Java files
- Standard package structure
- Annotations (Spring, JPA)
- Multiple service/controller/repository classes

**Why this repo:**
- Well-known, stable
- Exercises typical Java patterns
- Small enough for rapid iteration
- Complex enough to validate real extraction

### Validation criteria

| Criterion | Pass condition |
|-----------|----------------|
| Parse success rate | 100% of .java files parse without ExtractorError |
| FILE node count | Equals .java file count |
| Symbol count sanity | Rough parity with TS extractor on same repo |
| Import extraction | All import statements produce IMPORTS edges |
| Method extraction | Methods in source appear as SYMBOL nodes |
| Call extraction | Method invocations produce CALLS edges |
| Unresolved rate reporting | Trust output includes Java-specific rates |
| Annotation capture | @Component, @Service, etc. visible in metadata |

### rmap orientation usefulness

After extraction, verify:
```bash
rmap orient $db $repo
```
- Returns meaningful summary for a Java repo
- Lists packages/classes/methods
- Shows import graph topology
- Does not crash or return empty results

### Secondary validation

If primary validation passes, validate on a larger repo from fixtures or
local validation set. Candidate: glamCRM (Java/Spring, larger scale).

---

## 10. Technical Debt

### Explicit limitations in v1

| Limitation | Impact | Future path |
|------------|--------|-------------|
| **No type solver** | Cannot resolve receiver types for method calls | jdtls enrichment pass |
| **No overload call resolution** | Calls to overloaded methods cannot distinguish which overload | Type-aware call binding |
| **No inheritance dispatch** | Cannot trace calls through class hierarchy | Full type graph |
| **No annotation semantics** | @Autowired doesn't create edges | Spring detector layer |
| **No Maven/Gradle model** | Cannot resolve external dependencies | Build manifest parsing |
| **No generated sources** | Generated code invisible unless in tree | Build integration |
| **No cyclomatic complexity v1** | Metrics map may be empty initially | Port TS metrics logic |

**Clarification:** Overloaded method *declarations* are NOT collapsed. Each
declaration gets its own SYMBOL node with `:dupN` disambiguation. The limitation
is at *call resolution*: we cannot determine which overload a call targets.

### Documentation of precision boundaries

The extractor MUST document in its output:
- Extractor version in edge provenance
- Unresolved edge counts in trust report
- Signature metadata on method nodes (for human disambiguation)

Agents reading `rmap trust` output should see explicit Java precision indicators.

---

## 11. Implementation Plan

### Phase A: Support module crate

**Scope:**
- Create `rust/crates/java-extractor` crate
- Implement `JavaExtractor` struct
- Implement `ExtractorPort` trait
- Parse Java files with tree-sitter-java
- Extract FILE, SYMBOL nodes
- Extract IMPORTS, CALLS edges
- Capture annotations as metadata
- Unit tests for extraction logic

**Deliverables:**
- Working crate that compiles
- Can extract a single Java file
- Returns valid `ExtractionResult`

### Phase B: Repo indexing integration

**Scope:**
- Wire `JavaExtractor` into `repo-graph-indexer` router
- Add Java to language-extension mapping
- Run full indexing on spring-petclinic
- Verify nodes/edges stored correctly

**Deliverables:**
- `rmap index` works on Java repos
- Nodes visible in database
- Edges (resolved + unresolved) stored

### Phase C: Query/trust validation

**Scope:**
- Verify `rmap orient` produces useful output for Java
- Verify `rmap trust` reports Java-specific rates
- Verify `rmap callers/callees` work on Java symbols
- Document validation results

**Deliverables:**
- Validation report for spring-petclinic
- Trust output shows Java extraction quality
- Orientation commands work

### Phase D: Spring semantic layer (FUTURE)

**Not implemented in v1.** Design notes only:
- Spring detector consuming annotation metadata
- Bean graph construction from @Component/@Autowired
- HTTP boundary detection from @RequestMapping
- Separate crate or module, not in java-extractor

---

## 12. Decision Points

The following decisions must be made explicitly during implementation, not
silently chosen:

### Decision #1: tree-sitter-java vs other parser

**Recommendation:** Use tree-sitter-java.

**Rationale:**
- Consistent with TS, C, Rust extractors
- Grammar available and maintained
- Native compilation, no WASM overhead
- Partial tree support for syntax errors
- Codebase already uses tree-sitter everywhere

**Alternative considered:** JavaParser (Java library). Rejected because:
- Would require JNI or subprocess
- Different error model
- Inconsistent with existing architecture

### Decision #2: Field extraction in v1

**Options:**
- A: Extract fields as SYMBOL nodes (cheap, adds structural completeness)
- B: Skip fields (focus on methods/classes only)

**Recommendation:** Option A. Fields are cheap to extract and useful for
understanding data shape. Spring @Value fields especially relevant.

### Decision #3: Unresolved syntactic calls storage

**Options:**
- A: Emit edges with symbolic target_key; let indexer pipeline handle
     resolution; unresolved edges go to `unresolved_edges` table with classifier verdict
- B: Drop after trust accounting (count but don't persist)

**Recommendation:** Option A. Matches existing TS/C extractor behavior.
The indexer pipeline handles resolution; extractors emit symbolic edges.
Unresolved edges are valuable for orientation even without resolution.

### Decision #4: Overloaded method keying

**Options:**
- A: Keep separate SYMBOL nodes per declaration with `:dupN` disambiguation
- B: Collapse overloads to single node with `overload_count` metadata
- C: Include parameter types in key (requires type resolution)

**Recommendation:** Option A. Matches existing TS/C extractor duplicate handling.
Each declaration keeps its source location, signature, annotations, and metrics.
Ambiguity lives at call resolution, not symbol extraction. Java syntax exposes
parameter lists, so `signature` metadata can include syntactic parameter types
even without full type resolution.

### Decision #5: Constructor symbol kind

**Options:**
- A: Separate `NodeSubtype::CONSTRUCTOR`
- B: Normalize to `NodeSubtype::METHOD` with name `<init>`

**Recommendation:** Option A. TS extractor already uses CONSTRUCTOR subtype.
Constructors have distinct semantics (no return type, special invocation).
Consistent with existing model.

---

## Definition of Done

This design slice is complete when:

- [x] Design document created: `docs/design/java-rust-extractor-v1.md`
- [x] **Phase A: Support module crate** (`rust/crates/java-extractor`)
  - [x] `JavaExtractor` struct implementing `ExtractorPort`
  - [x] Tree-sitter-java grammar integration
  - [x] FILE, SYMBOL nodes (class, interface, enum, record, method, constructor, field)
  - [x] CALLS, INSTANTIATES, IMPLEMENTS edges (emitted; resolution limited by type info availability)
  - [x] IMPORTS edges (emitted but structurally unresolved — see note below)
  - [x] Annotation capture in metadata_json
  - [x] Cyclomatic complexity metrics
  - [x] Nested type hierarchy preservation (parent_node_uid + stable key chain)
  - [x] Member identity scoped to enclosing type (no false collisions)
  - [x] Constructor stable keys use type chain directly (no name duplication)
  - [x] Member qualified_name populated (package.Type.member)
  - [x] Receiver provenance preserved in CALLS edges (this.save, repo.find)
  - [x] 27 unit tests passing
  
  **IMPORTS edge limitation:** Java IMPORTS edges emit package-qualified target
  keys (`java.util.List`, `com.example.Foo`). The current Rust indexer import
  resolver is file-path oriented and cannot resolve these to nodes. All Java
  IMPORTS edges will be stored in `unresolved_edges` with category
  `ImportsFileNotFound`. The edges exist and are useful for orientation (showing
  what a file depends on), but they do not resolve to nodes. This is a
  documented v1 limitation; package-level resolution requires package nodes
  which are out of scope.
- [x] **Phase B: Repo indexing integration**
  - [x] Added `repo-graph-java-extractor` dependency to `repo-index`
  - [x] Added Java to `detect_language()` in `routing.rs`
  - [x] Wired `JavaExtractor` in `index_into_storage` and `refresh_into_storage`
  - [x] Integration test: `index_java_extracts_file_and_symbol_nodes`
  - [x] Integration test: `index_java_persists_metrics`
  - [x] Refresh test: `refresh_java_copies_unchanged_and_reextracts_changed`
- [x] **Phase C: Initial validation on spring-petclinic**
  - [x] CLI-first validation performed (no raw SQL)
  - [x] 47 files indexed, 359 nodes, 332 resolved edges
  - [x] callers/callees work on stable keys
  - [x] trust report includes java-core:0.1.0 provenance
  - [x] Validation report: `docs/validation/java-rust-extractor-v1-spring-petclinic.md`
  - Known degradation: 427 unresolved IMPORTS (by design)
  - Known degradation: 652 unresolved obj.method CALLS (needs type info)
  - Known degradation: module connectivity zero (no resolved IMPORTS)
- [ ] **No Spring detector yet**
- [ ] **No jdtls path yet**
- [ ] **No Gradle/Maven parsing yet**
