# SE-1: Clangd Semantic Enrichment

Status: PLANNED (LOW PRIORITY)
Depends: TC-1, BC-1 (build context required), C/C++ extraction maturity
Unblocks: High-fidelity C/C++ call graph, receiver type resolution
Track: Toolchain-Aware Evidence Import
Layer: 2 (derived architecture — semantic enrichment)

## Goal

Use clangd (LLVM language server) for semantic enrichment of C/C++ extraction,
resolving receiver types, call targets, and namespace-qualified symbols that
tree-sitter syntax-only extraction cannot determine.

**Rationale:** Tree-sitter extracts syntax. It cannot resolve:
- Which overloaded function is being called
- Virtual method dispatch targets
- Template instantiation targets
- Namespace-qualified symbol resolution
- Include-path-dependent symbol visibility

clangd, backed by libclang, has full semantic understanding. Using it as an
enrichment source (not primary extractor) upgrades call graph fidelity without
replacing the fast, deterministic syntax extraction.

**Why this is last:** Semantic enrichment is expensive, volatile, and requires
build context. The other slices (coverage, build context, findings) provide
high value with lower complexity. Do those first.

## Problem Analysis

### Current State

- C/C++ extraction is tree-sitter syntax-only
- Call edges are syntactic: `foo()` → CALLS edge to `foo` (unqualified)
- No receiver type resolution
- No overload resolution
- No virtual dispatch insight

### What clangd Provides

Via LSP protocol:
- `textDocument/definition` — resolve symbol to definition location
- `textDocument/typeDefinition` — resolve expression to type
- `textDocument/references` — find all references
- `workspace/symbol` — search symbols by name

Via clangd extensions:
- AST dump
- Hover with type information
- Semantic tokens

### Enrichment vs. Primary Extraction

**Primary extraction (tree-sitter):**
- Fast, deterministic
- Works without build context
- Produces syntax-level facts
- Always runs

**Enrichment (clangd):**
- Slow, requires compilation database
- Produces semantic refinements
- Optional, additive
- Upgrades existing facts

The pattern is the same as TypeScript enrichment with tsserver or Rust
enrichment with rust-analyzer: syntax extraction first, semantic enrichment
as an optional refinement pass.

## Scope

### In Scope

1. **clangd process management**
   - Spawn clangd with compile_commands.json
   - LSP initialization
   - Graceful shutdown

2. **Symbol resolution enrichment**
   - For unresolved CALLS edges, query definition
   - Resolve to qualified symbol path
   - Update edge with resolved target

3. **Receiver type enrichment**
   - For method calls with unknown receiver type
   - Query hover/type information
   - Record resolved receiver type

4. **CLI integration**
   - `rmap enrich <db> <repo> --enricher clangd`
   - Requires build context to be imported first

### Out of Scope

- Using clangd as primary extractor
- Real-time enrichment during index
- clangd-based code navigation (that's IDE territory)
- Cross-translation-unit analysis

## Design

### Enrichment Pipeline

```
1. Load snapshot with C/C++ files
2. Load build context (compile_commands.json)
3. Spawn clangd with --compile-commands-dir
4. For each unresolved CALLS edge in C/C++ files:
   a. Open file in clangd
   b. Query textDocument/definition at call site
   c. If resolved, update edge with qualified target
5. Persist enriched edges
6. Shutdown clangd
```

### clangd LSP Client

```rust
pub struct ClangdClient {
    process: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    request_id: AtomicU64,
}

impl ClangdClient {
    pub fn spawn(compile_commands_dir: &Path) -> Result<Self, EnrichmentError> {
        let process = Command::new("clangd")
            .arg("--compile-commands-dir")
            .arg(compile_commands_dir)
            .arg("--log=error")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        
        // ... LSP initialization
    }
    
    pub fn definition(&mut self, file: &Path, line: u32, col: u32) -> Result<Option<Location>, EnrichmentError> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "textDocument/definition",
            "params": {
                "textDocument": {"uri": file_uri(file)},
                "position": {"line": line - 1, "character": col - 1}
            }
        });
        
        self.send_request(&request)?;
        let response = self.read_response()?;
        
        // Parse Location from response
    }
}
```

### Enrichment Logic

```rust
pub fn enrich_cpp_calls_with_clangd(
    storage: &StorageConnection,
    snapshot_uid: &str,
    compile_commands_dir: &Path,
) -> Result<EnrichmentStats, EnrichmentError> {
    let mut client = ClangdClient::spawn(compile_commands_dir)?;
    let mut stats = EnrichmentStats::default();
    
    // Get unresolved C/C++ CALLS edges
    let unresolved_calls = storage.get_unresolved_calls(snapshot_uid, &["c", "cpp"])?;
    
    for call in unresolved_calls {
        stats.attempted += 1;
        
        // Query clangd for definition
        if let Some(location) = client.definition(&call.source_file, call.line, call.column)? {
            // Extract qualified symbol name from definition location
            if let Some(qualified_name) = extract_qualified_name(&location) {
                storage.update_edge_target(
                    &call.edge_uid,
                    &qualified_name,
                    "clangd",
                )?;
                stats.resolved += 1;
            }
        }
    }
    
    client.shutdown()?;
    Ok(stats)
}
```

### CLI

```bash
# Enrich C/C++ extraction with clangd
rmap enrich ./repo.db repo-uid --enricher clangd

# Requires build context
rmap build-context import ./repo.db repo-uid ./build/compile_commands.json
rmap enrich ./repo.db repo-uid --enricher clangd

# Check enrichment status
rmap trust ./repo.db repo-uid
# Shows: clangd enrichment coverage
```

## Prerequisites

1. **Build context imported** — clangd needs compile_commands.json
2. **clangd available** — detected via TC-1 toolchain inventory
3. **C/C++ files indexed** — enrichment refines existing extraction

## Definition of Done

- [ ] ClangdClient LSP implementation
- [ ] textDocument/definition query
- [ ] Edge target resolution and update
- [ ] Enrichment statistics tracking
- [ ] `rmap enrich --enricher clangd` command
- [ ] Trust report shows clangd coverage
- [ ] Unit tests with mock LSP responses
- [ ] Integration test: enrich a real C++ file with clangd

## Test Plan

1. **Unit tests:**
   - LSP message formatting
   - Response parsing
   - Location to qualified name extraction

2. **Integration test:**
   ```bash
   # Index a C++ project
   rmap index ./cpp-project ./test.db
   
   # Import build context
   rmap build-context import ./test.db cpp-project ./cpp-project/build/compile_commands.json
   
   # Enrich
   rmap enrich ./test.db cpp-project --enricher clangd
   
   # Verify
   rmap trust ./test.db cpp-project
   # Should show improved call resolution rate
   ```

## Dependencies

- `lsp-types` crate for LSP protocol types
- clangd binary (detected via TC-1)
- Build context (BC-1)

## Risks

- clangd startup time is significant (~seconds)
- Large projects may have many unresolved edges → slow
- clangd may crash on malformed code
- Different clangd versions have different capabilities

Mitigation:
- Batch queries where possible
- Timeout per-file queries
- Graceful error handling (skip file, continue)
- Version detection via TC-1

## Why This Is Last

1. **Coverage import (NC-1)** provides immediate value without semantic complexity
2. **Build context (BC-1)** is required anyway, and has its own value
3. **Findings import (AF-1)** adds risk signal without compiler integration
4. **Semantic enrichment** is the most complex, most volatile, slowest

Do the cheaper wins first. Come back to clangd when the foundation is solid
and the need for higher call graph fidelity is proven by real usage.
