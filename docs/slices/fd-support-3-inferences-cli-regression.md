# FD-SUPPORT-3: CLI Regression Coverage for rmap inferences

Status: IMPLEMENTED (2026-05-12)
Type: Support / Validation Hardening
Depends: `rmap inferences list` command (implemented as embedded support work in FD-1B)
Blocks: None

## Implementation Summary

Created `rust/crates/rgr/tests/inferences_command.rs` with 6 test cases covering CLI argument parsing, output structure, error handling, and filtering.

### Artifacts

- `rust/crates/rgr/tests/inferences_command.rs` — 280+ lines, 6 tests

### Test Cases Implemented

1. `inferences_list_usage_error` — wrong args, exit 1, usage in stderr
2. `inferences_list_missing_db` — nonexistent DB path, exit 2
3. `inferences_list_repo_not_found` — invalid repo reference, exit 2
4. `inferences_list_empty_result` — empty result is success (exit 0)
5. `inferences_list_with_kind_filter` — `--kind` filter works correctly
6. `inferences_list_output_structure` — JSON envelope and result fields validated

### Validation Results (EXECUTED)

```
cargo test -p repo-graph-rgr --test inferences_command
running 6 tests
test inferences_list_usage_error ... ok
test inferences_list_missing_db ... ok
test inferences_list_repo_not_found ... ok
test inferences_list_with_kind_filter ... ok
test inferences_list_output_structure ... ok
test inferences_list_empty_result ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

---

## Goal

Add dedicated CLI-level regression tests for the `rmap inferences list` command, following the established pattern used by `surfaces` command tests.

## Why This Slice Exists

The `rmap inferences list` command was implemented as part of FD-1B (embedded support work, sometimes referenced as "FD-SUPPORT-2" in docs). It is validated by:

- Manual execution (EXECUTED evidence in slice doc)
- E2E integration tests (`fd_1b_react_integration.rs`)

However, it lacks **dedicated CLI regression tests** like those in `rust/crates/rgr/tests/surfaces_command.rs`. This is documented in TECH-DEBT.md:

> No CLI regression tests for `rmap inferences list`. The new CLI command is validated by manual execution and E2E tests, but has no dedicated CLI-level regression tests.

CLI commands are product surface. They should have regression coverage that:

1. Tests the CLI argument parsing
2. Tests output format (JSON structure)
3. Tests error handling (missing DB, invalid repo, etc.)
4. Documents expected behavior for future changes

## Scope

### In Scope

1. **Create test file:** `rust/crates/rgr/tests/inferences_command.rs`

2. **Test cases:**
   - `list_inferences_returns_json` — basic output format
   - `list_inferences_with_kind_filter` — `--kind react_component`
   - `list_inferences_empty_for_unknown_repo` — graceful handling
   - `list_inferences_empty_for_no_matches` — kind with no results
   - `list_inferences_output_structure` — verify JSON fields present

3. **Test infrastructure:**
   - Reuse existing test helpers from `surfaces_command.rs` pattern
   - May need to create test fixtures or use existing React corpus

### Out of Scope

- New CLI functionality (that's FD-SUPPORT-2, already done)
- Query performance testing
- Integration with other commands

## Existing Pattern: surfaces_command.rs

Check the existing surfaces command test file for patterns to follow.

```rust
// Expected pattern (from surfaces_command.rs)
#[test]
fn surfaces_list_returns_json() {
    // Setup: index test corpus
    // Execute: run `rmap surfaces list` via CLI
    // Assert: output parses as JSON with expected structure
}
```

## Test Cases

### 1. list_inferences_returns_json

```rust
#[test]
fn list_inferences_returns_json() {
    // Setup: index react-frontend-corpus
    let db_path = setup_test_db_with_react_corpus();
    
    // Execute
    let output = run_rmap(&["inferences", "list", &db_path, "react-frontend-corpus"]);
    
    // Assert
    assert!(output.status.success());
    let json: Value = serde_json::from_str(&output.stdout).unwrap();
    assert!(json.get("command").is_some());
    assert!(json.get("count").is_some());
    assert!(json.get("results").is_some());
}
```

### 2. list_inferences_with_kind_filter

```rust
#[test]
fn list_inferences_with_kind_filter() {
    let db_path = setup_test_db_with_react_corpus();
    
    let output = run_rmap(&[
        "inferences", "list", &db_path, "react-frontend-corpus",
        "--kind", "react_component"
    ]);
    
    assert!(output.status.success());
    let json: Value = serde_json::from_str(&output.stdout).unwrap();
    
    // All results should have kind = react_component
    let results = json["results"].as_array().unwrap();
    for result in results {
        assert_eq!(result["kind"], "react_component");
    }
}
```

### 3. list_inferences_empty_for_unknown_repo

```rust
#[test]
fn list_inferences_empty_for_unknown_repo() {
    let db_path = setup_test_db_with_react_corpus();
    
    let output = run_rmap(&[
        "inferences", "list", &db_path, "nonexistent-repo"
    ]);
    
    // Should succeed with empty results, not error
    assert!(output.status.success());
    let json: Value = serde_json::from_str(&output.stdout).unwrap();
    assert_eq!(json["count"], 0);
    assert!(json["results"].as_array().unwrap().is_empty());
}
```

### 4. list_inferences_output_structure

```rust
#[test]
fn list_inferences_output_structure() {
    let db_path = setup_test_db_with_react_corpus();
    
    let output = run_rmap(&[
        "inferences", "list", &db_path, "react-frontend-corpus",
        "--kind", "react_component"
    ]);
    
    let json: Value = serde_json::from_str(&output.stdout).unwrap();
    let results = json["results"].as_array().unwrap();
    
    // At least one result exists (corpus has components)
    assert!(!results.is_empty());
    
    // Check first result has expected fields
    let first = &results[0];
    assert!(first.get("inference_uid").is_some());
    assert!(first.get("target_stable_key").is_some());
    assert!(first.get("kind").is_some());
    assert!(first.get("value").is_some());
    assert!(first.get("confidence").is_some());
}
```

## Test Infrastructure

### Option A: Inline Corpus Setup

Each test indexes the React corpus and creates a temp DB.

**Pros:**
- Self-contained tests
- No shared state

**Cons:**
- Slower (repeated indexing)
- May need to manage temp files

### Option B: Shared Test Fixture DB

Pre-index corpus once, share read-only DB across tests.

**Pros:**
- Faster test execution
- Consistent test data

**Cons:**
- Test isolation requires care
- Setup complexity

### Recommendation

**Option A** for initial implementation. Tests are small enough that indexing overhead is acceptable. Can optimize later if test suite grows.

## Validation Commands

```bash
# 1. Build tests
cd rust && cargo build -p rgr --tests

# 2. Run new test file
cargo test -p rgr inferences_command

# 3. Run all rgr tests to ensure no regressions
cargo test -p rgr
```

## Acceptance Criteria

1. `rust/crates/rgr/tests/inferences_command.rs` exists
2. At least 4 test cases implemented
3. Tests use consistent helpers (DRY with surfaces pattern)
4. All tests pass
5. Tests document expected CLI behavior

## Definition of Done

- Test file created
- All test cases implemented
- Tests pass in CI (cargo test -p rgr)
- No regressions in existing tests

## Estimated Effort

Small slice. Primarily test writing following established patterns.

- Study surfaces_command.rs pattern: 30 minutes
- Implement test infrastructure: 1 hour
- Write test cases: 1-2 hours
- Validation: 30 minutes

Total: ~0.5 day
