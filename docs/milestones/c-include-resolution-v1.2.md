# C Include Resolution v1.2 — Design Slice

Repair slice to restore module topology on angle-bracket-include C repos like `nginx`.

## Problem Statement

v1.1 solved quoted-include layouts (`swupdate`) but nginx uses angle-bracket
includes for local headers:

```c
#include <ngx_config.h>   // local header, not system
#include <ngx_core.h>     // local header, not system
```

v1.1 explicitly skips angle-bracket includes as system headers. nginx topology
is completely collapsed: 13 modules with zero connectivity.

## Root Cause

The C extractor conflates two distinct concepts:

1. **Delimiter syntax** — quoted (`"foo.h"`) vs angle-bracket (`<foo.h>`)
2. **Semantic origin** — local (indexed in this repo) vs system/external

These are NOT the same thing:
- `"stdio.h"` is quoted but system
- `<ngx_core.h>` is angle-bracket but local

Current behavior: angle-bracket → skip as system. This is wrong.

Correct behavior: try to resolve both forms against local headers; only
classify as system/external if resolution fails.

## Scope

**In scope:**
- Apply same resolution policy to angle-bracket includes
- Distinguish resolved-local from unresolved-system by outcome, not syntax
- Maintain same-dir as quote-only (angle-bracket implies search paths)
- Preserve ambiguity detection unchanged
- Revalidate nginx post-repair

**Out of scope (explicitly deferred):**
- `compile_commands.json` integration
- Build-system search path parsing
- System header path scanning (`/usr/include`, etc.)
- Macro expansion

## Design Decisions

### 1. Resolution Policy for Both Forms (Pinned)

Both quoted and angle-bracket includes go through:

1. Configured roots (CLI `--include-root`, programmatic `c_include_roots`)
2. Conventional roots (`include/`, `inc/`, `src/include/`)
3. Exact normalized path matching
4. Ambiguity detection (multiple matches → `ImportsAmbiguousMatch`)

Same-dir resolution remains **quote-only**. Rationale: angle-bracket syntax
conventionally implies "not in my directory, search elsewhere." A file using
`<foo.h>` expects build flags to provide the path, not source-dir locality.

### 2. Semantic Origin by Outcome (Pinned)

After resolution:
- **Resolved** → local header (indexed file matched)
- **Unresolved** → system/external (no indexed file matched)

Do NOT guess based on:
- Header name patterns (`stdio.h` "looks like" system)
- Path prefixes (`/usr/include` detection)
- Heuristics

If `<stdio.h>` happens to match an indexed local file (someone vendored libc
headers), it resolves as local. If `<ngx_core.h>` does not match any indexed
file, it stays unresolved. The resolution attempt is the arbiter.

### 3. Metadata Preservation (Pinned)

The extractor already emits `is_system: bool` based on delimiter syntax.
This metadata is preserved for diagnostics but does NOT control resolution.

Post-v1.2 interpretation:
- `is_system: true` + resolved → angle-bracket local include
- `is_system: true` + unresolved → likely true system header
- `is_system: false` + resolved → quoted local include
- `is_system: false` + unresolved → quoted include, file not found

Trust reports can use this to distinguish "angle-bracket that resolved" from
"quoted that resolved" for diagnostic purposes.

### 4. Ambiguity Rules Unchanged (Pinned)

If `<config.h>` matches multiple indexed headers under allowed roots:
- Category: `ImportsAmbiguousMatch`
- No guessing which one
- Trust surface reports it distinctly

Same rules as v1.1 for quoted includes.

### 5. Same-Dir Remains Quote-Only (Pinned)

Resolution order for **quoted** includes:
1. Same directory as source file
2. Configured roots
3. Conventional roots

Resolution order for **angle-bracket** includes:
1. Configured roots (skip same-dir)
2. Conventional roots

Rationale: `#include "foo.h"` conventionally means "look here first."
`#include <foo.h>` conventionally means "don't look here, use search paths."

This is a deliberate asymmetry that matches C convention.

## Implementation Changes

### C Extractor

No changes needed. The extractor already emits `is_system` based on delimiter.
Resolution is handled by the indexer.

### Include Resolver (`include_resolver.rs`)

Current: `resolve()` takes `is_system: bool` and skips resolution if true.

Change: `resolve()` always attempts resolution regardless of `is_system`.
The `is_system` flag is metadata, not a skip condition.

```rust
// BEFORE (v1.1)
pub fn resolve(&self, source_path: &str, specifier: &str, is_system: bool) -> IncludeResolution {
    if is_system {
        return IncludeResolution::unresolved(); // SKIP
    }
    // ... resolution logic
}

// AFTER (v1.2)
pub fn resolve(&self, source_path: &str, specifier: &str, is_system: bool) -> IncludeResolution {
    // Always attempt resolution. is_system controls same-dir behavior only.
    let skip_same_dir = is_system;
    // ... resolution logic with skip_same_dir flag
}
```

### Resolver (`resolver.rs`)

No changes needed. The resolver calls `include_resolver.resolve()` and
handles the result. The change is internal to the include resolver.

### Trust Reporting

Optional enhancement: trust report could show breakdown of:
- Angle-bracket resolved (local)
- Angle-bracket unresolved (system)
- Quoted resolved (local)
- Quoted unresolved (not found)

This is diagnostic enrichment, not required for v1.2 MVP.

## Validation Fixtures

Before nginx revalidation:

1. **Quoted baseline unchanged** — existing v1.1 tests pass
2. **Angle-bracket local resolution** — `<ngx_core.h>` from `src/http/x.c` → `src/core/ngx_core.h`
3. **Angle-bracket system unchanged** — `<stdio.h>` remains unresolved
4. **Same-dir quote-only** — `"foo.h"` checks same-dir, `<foo.h>` does not
5. **Ambiguity detection** — multiple matches still categorized correctly

## Success Criteria

nginx revalidation shows:
- Zero-connectivity modules: < 5 (was 13)
- `src/core` module fan_in > 0 (was 0)
- Hotspots unchanged (already work)
- callers/callees unchanged (already work)
- Trust report: `alias_resolution_suspicion` NOT triggered

If nginx topology is restored, v1.2 is validated for angle-bracket C layouts.

## Test Plan

1. Unit tests in `include_resolver.rs`:
   - `angle_bracket_resolves_via_conventional_root`
   - `angle_bracket_skips_same_dir`
   - `angle_bracket_unresolved_when_no_match`
   - `angle_bracket_ambiguous_when_multiple_matches`

2. Integration test:
   - nginx-style fixture with angle-bracket local includes
   - Verify module connectivity post-indexing

3. Regression test:
   - swupdate fixture unchanged (quoted includes still work)
   - sqlite fixture unchanged

4. Revalidation:
   - Re-index nginx
   - Run `rmap trust`, `rmap stats`
   - Verify topology restoration

## Deferred Items

- **Repo-config source** (`.rmap/config.toml`) — same as v1.1, infrastructure not built
- **System header path scanning** — would require platform-specific `/usr/include` detection
- **compile_commands.json** — build-system integration, larger scope
- **Trust diagnostic breakdown** — angle-bracket vs quoted resolved counts

## Risk Assessment

Low risk. The change is localized to `include_resolver.rs` and does not
affect:
- Extraction (C extractor unchanged)
- Edge creation (still creates IMPORTS edges)
- Resolution dispatch (resolver.rs unchanged)
- Other edge types (CALLS, etc.)

The only behavioral change: angle-bracket includes now attempt resolution
instead of being skipped.
