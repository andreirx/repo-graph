# NC-1: LLVM Coverage Import

Status: PLANNED
Depends: TC-1 (toolchain inventory), coverage infrastructure (EXISTS)
Unblocks: Native risk scoring, dead-surface confidence, test-targeted navigation
Track: Toolchain-Aware Evidence Import
Layer: 2 (derived architecture — imported evidence)

## Goal

Import `llvm-cov export` JSON output as coverage measurements, enabling native
C/C++/Rust codebases to have the same coverage-driven orientation surfaces as
JavaScript/TypeScript repos with Istanbul/c8.

**Rationale:** Coverage is the highest-value imported evidence for orientation:
- Upgrades dead-surface confidence
- Enables risk scoring
- Feeds hotspot interpretation
- Supports test-targeted navigation

llvm-cov is already present on this machine via Xcode toolchain. This is a real
asset, not a hypothetical one.

## Problem Analysis

### Current State

Coverage infrastructure exists:
- `rust/crates/coverage/` — coverage parsing and storage
- `rmap coverage <db> <repo> <report>` — CLI command
- Istanbul/c8 JSON format supported
- Measurements table populated with line/function coverage

Gap:
- Only Istanbul JSON format supported
- No llvm-cov export format parser
- No native (C/C++/Rust) coverage import path

### llvm-cov Export Format

`llvm-cov export` produces JSON with this structure:

```json
{
  "data": [{
    "files": [{
      "filename": "/path/to/file.c",
      "segments": [[line, col, count, hasCount, isRegionEntry, isGapRegion], ...],
      "summary": {
        "lines": {"count": 100, "covered": 85, "percent": 85.0},
        "functions": {"count": 10, "covered": 8, "percent": 80.0},
        "regions": {"count": 50, "covered": 40, "percent": 80.0}
      }
    }],
    "functions": [{
      "name": "function_name",
      "count": 5,
      "regions": [[startLine, startCol, endLine, endCol, count, fileId, ...], ...]
    }],
    "totals": { ... }
  }],
  "type": "llvm.coverage.json.export",
  "version": "2.0.1"
}
```

Key differences from Istanbul:
- Segments instead of statement/branch maps
- Region-based coverage model
- Absolute paths (need normalization)
- Function-level counts directly available

### Coverage Workflow for Native Code

1. **Compile with coverage instrumentation:**
   ```bash
   clang -fprofile-instr-generate -fcoverage-mapping -o prog prog.c
   ```

2. **Run program to generate raw profile:**
   ```bash
   ./prog  # generates default.profraw
   ```

3. **Merge profiles:**
   ```bash
   llvm-profdata merge -sparse default.profraw -o prog.profdata
   ```

4. **Export coverage:**
   ```bash
   llvm-cov export ./prog -instr-profile=prog.profdata > coverage.json
   ```

5. **Import into repo-graph:**
   ```bash
   rmap coverage ./repo.db repo-uid coverage.json --format llvm-cov
   ```

## Scope

### In Scope

1. **llvm-cov export JSON parser**
   - Parse version 2.0.x format
   - Extract file-level summaries (lines covered/total)
   - Extract function-level counts
   - Normalize absolute paths to repo-relative

2. **Coverage import adapter**
   - Convert llvm-cov model to repo-graph coverage model
   - Persist to measurements table
   - Record tool provenance (llvm-cov version)

3. **CLI extension**
   - `rmap coverage <db> <repo> <report> --format llvm-cov`
   - Auto-detect format if possible (check `type` field)

4. **Path normalization**
   - llvm-cov uses absolute paths
   - Must normalize to repo-relative for matching

### Out of Scope (Later)

- Invoking llvm-cov directly (user runs export, imports result)
- gcov format (separate slice)
- Rust grcov format (separate slice, though often llvm-cov compatible)
- Branch coverage (segments model is complex, start with line/function)

## Design

### Parser Module

```rust
// rust/crates/coverage/src/llvm_cov.rs

pub struct LlvmCovReport {
    pub version: String,
    pub files: Vec<LlvmCovFile>,
    pub functions: Vec<LlvmCovFunction>,
}

pub struct LlvmCovFile {
    pub filename: String,  // Absolute path from llvm-cov
    pub lines_total: u32,
    pub lines_covered: u32,
    pub functions_total: u32,
    pub functions_covered: u32,
}

pub struct LlvmCovFunction {
    pub name: String,
    pub filename: String,
    pub start_line: u32,
    pub execution_count: u64,
}

pub fn parse_llvm_cov_export(json: &str) -> Result<LlvmCovReport, CoverageError> {
    let raw: serde_json::Value = serde_json::from_str(json)?;
    
    // Validate format
    let typ = raw.get("type").and_then(|v| v.as_str());
    if typ != Some("llvm.coverage.json.export") {
        return Err(CoverageError::UnsupportedFormat);
    }
    
    // Parse data array
    let data = raw.get("data").and_then(|v| v.as_array())
        .ok_or(CoverageError::MalformedReport)?;
    
    // ... extraction logic
}
```

### Path Normalization

```rust
pub fn normalize_llvm_cov_path(
    absolute_path: &str,
    repo_root: &Path,
) -> Option<String> {
    let path = Path::new(absolute_path);
    
    // Try to strip repo root prefix
    if let Ok(relative) = path.strip_prefix(repo_root) {
        return Some(relative.to_string_lossy().to_string());
    }
    
    // Fallback: check if path contains repo directory name
    // (handles cases where repo_root is symlinked differently)
    // ...
    
    None  // Cannot normalize — file not in repo
}
```

### Conversion to Measurements

```rust
pub fn llvm_cov_to_measurements(
    report: &LlvmCovReport,
    repo_root: &Path,
    snapshot_uid: &str,
) -> Vec<Measurement> {
    let mut measurements = Vec::new();
    
    for file in &report.files {
        if let Some(rel_path) = normalize_llvm_cov_path(&file.filename, repo_root) {
            // Line coverage measurement
            measurements.push(Measurement {
                snapshot_uid: snapshot_uid.to_string(),
                measurement_kind: "line_coverage".to_string(),
                scope_kind: "file".to_string(),
                scope_key: rel_path.clone(),
                value_num: Some(file.lines_covered as f64 / file.lines_total as f64),
                value_json: Some(json!({
                    "lines_covered": file.lines_covered,
                    "lines_total": file.lines_total,
                })),
                source_tool: Some("llvm-cov".to_string()),
                source_version: Some(report.version.clone()),
            });
        }
    }
    
    measurements
}
```

## CLI

```bash
# Import llvm-cov export JSON
rmap coverage ./repo.db repo-uid coverage.json --format llvm-cov

# Auto-detect format (checks type field)
rmap coverage ./repo.db repo-uid coverage.json

# With explicit repo root for path normalization
rmap coverage ./repo.db repo-uid coverage.json --format llvm-cov --repo-root /path/to/repo
```

## Persistence

Uses existing measurements table:
- `measurement_kind = "line_coverage"` or `"function_coverage"`
- `scope_kind = "file"` or `"function"`
- `scope_key = <repo-relative-path>` or `<function-stable-key>`
- `source_tool = "llvm-cov"`
- `source_version = "17.0.0"` (from report or detected)

## Definition of Done

- [ ] llvm-cov export JSON parser in `rust/crates/coverage/src/llvm_cov.rs`
- [ ] Path normalization for absolute → repo-relative
- [ ] Conversion to measurements model
- [ ] `--format llvm-cov` CLI flag
- [ ] Format auto-detection
- [ ] Tool provenance recorded in measurements
- [ ] Unit tests with sample llvm-cov JSON
- [ ] Integration test: generate coverage for a C file, import, verify measurements

## Test Plan

1. **Unit tests:**
   - Parse sample llvm-cov export JSON
   - Path normalization edge cases
   - Conversion to measurements

2. **Integration test on this machine:**
   ```bash
   # Compile test file with coverage
   clang -fprofile-instr-generate -fcoverage-mapping -o test_prog test_prog.c
   
   # Run to generate profile
   ./test_prog
   
   # Merge and export
   xcrun llvm-profdata merge -sparse default.profraw -o test.profdata
   xcrun llvm-cov export ./test_prog -instr-profile=test.profdata > coverage.json
   
   # Import
   rmap coverage ./test.db test-repo coverage.json --format llvm-cov
   
   # Verify
   rmap check ./test.db test-repo  # should show coverage data
   ```

3. **Real repo test:**
   - Run on a real C/C++ project with existing coverage
   - Verify measurements match llvm-cov report summary

## Dependencies

- `rust/crates/coverage/` — existing coverage infrastructure
- `serde_json` — already in use
- No new external crates needed

## Risks

- llvm-cov export format may vary between versions
- Absolute path normalization is fragile across symlinks/mounts
- Large repos may have large coverage reports

Mitigation:
- Support version 2.0.x initially, extend as needed
- Path normalization falls back gracefully (skips unmatchable files)
- Stream parsing if memory becomes an issue (unlikely for JSON)
