# Hotspot Presentation Filtering — Design Slice

View-policy controls for `rmap hotspots` to produce cleaner AI-agent working sets.

## Problem Statement

The hotspot command surfaces high-churn, high-complexity files for triage.
On real codebases, the head of the list is often polluted by:

- **Vendored libraries** — `mongoose/mongoose.c`, `vendor/lodash.js`
- **Test baselines** — `tests/baselines/reference/*.js` (generated output)
- **Generated artifacts** — `generated/parser.c`, `dist/*.js`

These files have high scores but are not actionable work items. An AI agent
must mentally filter them every time, which defeats the purpose of a triage
surface.

## Scope

**In scope:**
- Query-time filtering via CLI flags
- `--exclude-tests` (use persisted `is_test` metadata)
- `--exclude-vendored` (path-based heuristic)
- Output indication that filtering was applied
- Raw mode remains default (no silent exclusion)

**Out of scope (explicitly deferred):**
- `--exclude-generated` (no reliable generated-file metadata yet)
- Persisted vendored/generated flags in the index
- Default exclusion profiles
- Config-file-based exclusion rules

## Design Decisions

### 1. View-Policy, Not Data Deletion (Pinned)

Filtering is applied at query/output time only.

- Stored measurements unchanged
- Index contains all files
- Hotspot scores computed for all files
- Filtering happens AFTER scoring, BEFORE output

This preserves the raw truth and makes filtering reversible.

### 2. Explicit Opt-In Flags (Pinned)

No silent default exclusion. User must request filtering.

```bash
# Raw (default) — all files
rmap hotspots <db_path> <repo_uid>

# Exclude test files
rmap hotspots <db_path> <repo_uid> --exclude-tests

# Exclude vendored paths
rmap hotspots <db_path> <repo_uid> --exclude-vendored

# Both
rmap hotspots <db_path> <repo_uid> --exclude-tests --exclude-vendored
```

### 3. Test Exclusion Uses Persisted Metadata (Pinned)

The index already persists `is_test: bool` on each file (computed during
scanning via path heuristics: `test/`, `tests/`, `__tests__/`, `.test.`,
`.spec.`, etc.).

`--exclude-tests` filters files where `is_test = true`.

This is real metadata, not runtime guessing.

### 4. Vendored Exclusion Uses Exact Path-Segment Matching (Pinned)

No persisted `is_vendored` flag exists. `--exclude-vendored` applies
exact path-segment matching at query time.

Excluded directory segments (exact match only):
- `vendor`
- `vendors`
- `third_party`
- `third-party`
- `external`
- `deps`
- `node_modules` (already excluded from index, but defensive)

Matching rule:
1. Normalize path separators to `/`
2. Split into segments
3. Match if ANY segment exactly equals a vendored name

This is **segment-based, not substring-based**. Examples:

| Path | Excluded? | Reason |
|------|-----------|--------|
| `vendor/lodash/index.js` | Yes | segment `vendor` matches |
| `src/mydeps/file.c` | No | `mydeps` != `deps` |
| `lib/external/foo.c` | Yes | segment `external` matches |
| `dependencies/bar.c` | No | `dependencies` != `deps` |

These patterns are documented and deterministic. They are NOT configurable
in v1 (config-based rules are deferred).

### 5. Raw Mode Preserved (Pinned)

Default output (no flags) returns ALL hotspots. Filtered mode is an
alternate lens, not a replacement truth.

An AI agent can:
- Run raw mode to see everything
- Run filtered mode for a cleaner action list
- Compare the two to understand what was excluded

### 6. Honest Counts and Filtering Indication (Pinned)

When filtering is applied:

```json
{
  "command": "graph hotspots",
  "repo": "...",
  "snapshot": "...",
  "results": [...],
  "count": 42,
  "filtering": {
    "exclude_tests": true,
    "exclude_vendored": true,
    "excluded_count": 15,
    "excluded_tests_count": 6,
    "excluded_vendored_count": 11
  },
  "stale": false
}
```

- `count` reflects filtered results (rows returned)
- `filtering` object indicates which filters were applied
- `excluded_count` is unique rows removed (union of all active filters)
- `excluded_tests_count` is rows matching test filter
- `excluded_vendored_count` is rows matching vendored filter

Per-filter counts are category counts. If a file matches both filters
(e.g., `vendor/test/foo.js` with `is_test=true`), it increments both
per-filter counts but only contributes once to `excluded_count`.

**Absence contract:** When no filters are active, the `filtering` field
is **omitted entirely** (not `null`). This indicates "no alternate lens
applied" and avoids null-handling noise in consumers.

### 7. No Generated-File Exclusion Yet (Deferred)

`--exclude-generated` requires reliable generated-file detection.
Options:
- Path heuristics (`dist/`, `build/`, `generated/`)
- Persisted `is_generated` flag from index
- `.gitattributes` or similar markers

None of these are robust enough yet. Deferred to future slice.

## Implementation

### CLI Argument Parsing

Add to `run_hotspots` in `main.rs`:

```rust
// Parse flags
let mut exclude_tests = false;
let mut exclude_vendored = false;
for arg in args {
    if arg == "--exclude-tests" {
        exclude_tests = true;
    } else if arg == "--exclude-vendored" {
        exclude_vendored = true;
    }
}
```

### Filtering Logic

After fetching hotspot results from storage, apply filters:

```rust
const VENDORED_SEGMENTS: &[&str] = &[
    "vendor", "vendors", "third_party", "third-party",
    "external", "deps", "node_modules",
];

/// Exact path-segment matching for vendored detection.
fn is_vendored_path(path: &str) -> bool {
    path.split('/')
        .any(|segment| VENDORED_SEGMENTS.contains(&segment.to_lowercase().as_str()))
}

// Count excluded rows by category
let mut excluded_tests_count = 0;
let mut excluded_vendored_count = 0;
let mut excluded_set = HashSet::new();

let filtered: Vec<_> = results
    .into_iter()
    .filter(|r| {
        let exclude_as_test = exclude_tests && r.is_test;
        let exclude_as_vendored = exclude_vendored && is_vendored_path(&r.file_path);
        
        if exclude_as_test {
            excluded_tests_count += 1;
        }
        if exclude_as_vendored {
            excluded_vendored_count += 1;
        }
        if exclude_as_test || exclude_as_vendored {
            excluded_set.insert(&r.file_path);
            return false;
        }
        true
    })
    .collect();

let excluded_count = excluded_set.len();
```

### Output Envelope

Extend the hotspots output to include filtering metadata when active:

```rust
if exclude_tests || exclude_vendored {
    // Add filtering object to output
}
```

## Validation

### Test Cases

1. **No flags** — returns all hotspots, `filtering` field omitted
2. **`--exclude-tests`** — removes `is_test=true` files, `excluded_tests_count` accurate
3. **`--exclude-vendored`** — removes vendored paths, `excluded_vendored_count` accurate
4. **Both flags** — removes both categories, per-filter counts correct
5. **Overlap handling** — file matching both filters counted in both per-filter counts but only once in `excluded_count`
6. **Count accuracy** — `count` matches filtered length
7. **Segment matching** — `src/mydeps/foo.c` NOT excluded, `deps/foo.c` IS excluded
8. **Case insensitivity** — `Vendor/foo.c` excluded (segment match is case-insensitive)

### Real-Repo Validation

1. **react** — verify test baselines excluded with `--exclude-tests`
2. **swupdate** — verify test files excluded; note that `mongoose/` is NOT
   excluded (not a standard vendored segment — project-specific vendoring
   requires config-based rules, which are deferred)
3. **nginx** — baseline (no standard vendored segments, minimal tests)

## CLI Help Update

```
rmap hotspots <db_path> <repo_uid> [--since <expr>] [--exclude-tests] [--exclude-vendored]
```

## Success Criteria

1. `--exclude-tests` removes test files from hotspot output
2. `--exclude-vendored` removes vendored paths from hotspot output
3. Default (no flags) is unchanged
4. `filtering` metadata indicates what was applied
5. `excluded_count` is accurate
6. AI agent can get a clean triage list with combined flags

## Deferred Items

- **`--exclude-generated`** — needs reliable detection
- **Config-based exclusion rules** — `.rmap/config.toml` with custom patterns
- **Default profiles** — e.g., `--profile clean` as shorthand
- **Persisted vendored/generated flags** — index-time detection

## Risk Assessment

Low risk. Changes are:
- CLI argument parsing (additive)
- Post-query filtering (no storage changes)
- Output envelope extension (backward-compatible)

No changes to:
- Indexing
- Storage schema
- Hotspot computation
- Other commands
