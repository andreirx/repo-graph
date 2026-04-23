# C Include Resolution v1.1 — Design Slice

Repair slice to restore module topology on embedded-style C repos like `swupdate`.

## Problem Statement

C v1 same-directory include resolution works on `sqlite` (53% call resolution,
MEDIUM trust) but degrades on `swupdate` (38% call resolution, LOW trust,
15 zero-connectivity modules).

Root cause: cross-directory includes. Files in `core/`, `handlers/`,
`suricatta/` include headers from `include/` but resolution fails because
v1 only searches the source file's own directory.

## Scope

**In scope:**
- Widen include resolution to project include roots
- Deterministic resolution policy
- Ambiguity reporting (not guessing)
- Revalidate swupdate post-repair

**Out of scope (explicitly deferred):**
- `compile_commands.json` integration
- Build-system parsing
- Macro expansion
- Full preprocessor evaluation
- Compiler-semantic include search emulation

## Include Forms to Handle

| Form | Example | v1.0 | v1.1 Target |
|------|---------|------|-------------|
| Same-directory | `#include "foo.h"` from `src/bar.c` → `src/foo.h` | Resolved | Resolved |
| Sibling directory | `#include "util.h"` from `handlers/x.c` → `core/util.h` | Unresolved | Unresolved (no magic) |
| Shared include tree | `#include "util.h"` from `core/x.c` → `include/util.h` | Unresolved | **Resolved** |
| Project-root-relative | `#include "include/util.h"` | Unresolved | **Resolved** |
| Generated include dirs | `#include "autoconf.h"` → `generated/autoconf.h` | Unresolved | Configurable |
| Ambiguous basename | `#include "config.h"` with multiple `config.h` in tree | N/A | **Ambiguous** |
| System headers | `#include <stdio.h>` | Unresolved | Unresolved |

## Resolution Sources (v1.1)

Ordered by priority:

1. **Source file directory** (v1.0 behavior, unchanged)
2. **Configured include roots** (new, explicit)
   - User-declared, explicit paths relative to repo root
   - Source: see "Configured Roots Source" below
3. **Conventional include roots** (new, heuristic)
   - `include/`
   - `inc/`
   - `src/include/`
   - Repo-root anchored only (no recursive discovery)

**Order rationale:** Explicit repo knowledge outranks convention. A heuristic
path must not steal resolution from a deliberately configured include root.

NOT in v1.1:
- `compile_commands.json` parsing
- Build-system include path extraction
- Recursive directory search
- "Find any directory named include" discovery

## Ambiguity Policy

**Rule: ambiguity-preserving, not guessy.**

Ambiguity is defined narrowly:
- Only when exact normalized lookup yields more than one indexed candidate
- Never from fuzzy basename search
- Never from suffix guessing

What this means:
- `#include "foo.h"` can become ambiguous only if multiple allowed roots each
  contain an exact `foo.h`
- `#include "sub/foo.h"` must resolve by exact `sub/foo.h` path, not by
  basename `foo.h` alone

If multiple indexed headers match the exact include specifier:
- Do NOT silently pick one
- Report as AMBIGUOUS in unresolved edges
- Include candidate paths in metadata for debugging
- Trust report surfaces ambiguity count

Example:
```
#include "config.h"
```
If both `include/config.h` and `inc/config.h` exist (exact `config.h` in
multiple roots):
- Edge is UNRESOLVED
- Classification: `imports_ambiguous_match`
- Metadata: `candidates: ["include/config.h", "inc/config.h"]`

NOT ambiguous:
- `#include "sub/config.h"` when only `include/sub/config.h` exists
- That is a single exact match, resolved normally

## Architecture

### Support Module

New module: `rust/crates/indexer/src/include_resolver.rs`

Pure policy, no side effects.

```rust
pub struct IncludeResolution {
    pub status: ResolutionStatus,
    pub target_stable_key: Option<String>,
    pub candidates: Vec<String>,  // for ambiguous case
}

pub enum ResolutionStatus {
    Resolved,
    Unresolved,
    Ambiguous,
}

pub struct IncludeResolverConfig {
    /// User-declared roots (from CLI, config file, or programmatic)
    /// Checked BEFORE conventional roots
    pub configured_roots: Vec<String>,
    /// Repo-root anchored conventional roots
    /// Only: ["include", "inc", "src/include"]
    pub conventional_roots: Vec<&'static str>,
}

impl IncludeResolver {
    pub fn resolve(
        &self,
        source_file_path: &str,      // .c, .h, .cpp, .hpp, .hxx
        include_specifier: &str,      // exact specifier from #include
        indexed_files: &HashSet<String>,
        repo_uid: &str,
    ) -> IncludeResolution;
}
```

Note: `source_file_path` accepts header files as includers (`.h`, `.hpp`,
`.hxx`), not just source files. The policy is symmetric.

### Resolution Algorithm

```
1. If system include (<...>): return Unresolved
2. Try same-directory: source_dir + specifier (exact normalized path)
   - If exact match in indexed_files: return Resolved
3. For each configured root (in declaration order):
   - Try root + specifier (exact normalized path)
   - Collect all matches
4. For each conventional root (include, inc, src/include):
   - Try root + specifier (exact normalized path)
   - Collect all matches
5. If exactly one match across all roots: return Resolved
6. If multiple matches: return Ambiguous with candidates
7. If no matches: return Unresolved
```

Key: steps 3-4 collect candidates, resolution/ambiguity decided at step 5-7.
Configured roots are checked before conventional roots, but all candidates
are collected before deciding ambiguity.

### Integration

Wire into `orchestrator.rs`:
- Build `IncludeResolver` with conventional roots
- Pass to edge resolution phase
- Update `per_file_include_resolution` to use new resolver

### Trust Surface

Update `rmap trust` to report:
- `unresolved_imports_ambiguous` count
- Per-category breakdown includes `imports_ambiguous_match`

## Locked Decisions

### 1. Resolution Order: Same-dir → Configured → Conventional

Explicit repo knowledge outranks heuristic convention. A configured root
must win over a conventional root.

### 2. No Sibling Directory Magic

`#include "util.h"` from `handlers/x.c` does NOT search `core/util.h`.

That would require build-system knowledge or expensive recursive search.
v1.1 only adds explicit include roots.

### 3. Conventional Roots Are Repo-Root Anchored Only

Only these exact paths from repo root:
- `include/`
- `inc/`
- `src/include/`

No recursive "find any directory named include" discovery. Recursive
convention search becomes magic and will over-match on large repos.

### 4. Ambiguity on Multiple Exact Candidates

If both `include/foo.h` and `inc/foo.h` exist, and source includes `"foo.h"`:
- This is AMBIGUOUS
- Not resolved to first match
- Not resolved by basename fuzzy matching

### 5. Headers Are First-Class Includers

`.h`, `.hpp`, `.hxx` files participate in resolution as source locations,
not just as include targets.

Reason: C v1 already extracts `#include` from headers. The policy must be
symmetric or the header-to-header resolution hole remains.

### 6. Configured Roots Source (Pinned)

Configured include roots come from exactly one of:
- **Index config object** — `IndexOptions.c_include_roots: Vec<String>`
- **CLI option** — `rmap index --include-root <path>` (repeatable)
- **Repo config file** — `.rmap/config.toml` with `[c] include_roots = [...]` **(DEFERRED)**

Priority if multiple sources present:
1. CLI option (overrides all)
2. Repo config file **(not yet implemented)**
3. Index config object (programmatic default)

**Implementation status (v1.1 shipped):**
- CLI option: implemented (`--include-root`)
- Index config object: implemented (`IndexOptions.c_include_roots`)
- Repo config file: DEFERRED — no `.rmap/config.toml` infrastructure exists

The repo-config path requires new config discovery and parsing infrastructure
that does not exist in the Rust codebase. This is deferred to a future slice.

### 7. Path Normalization

All paths normalized before comparison:
- No trailing slashes
- No `./` prefixes
- Forward slashes only

### 8. No Suffix Guessing

`#include "foo"` does NOT try `foo.h`, `foo.hpp`, etc.
Specifier must match exactly.

### 9. Exact Normalized Path Matching Only

`#include "sub/foo.h"` resolves by exact `sub/foo.h` under allowed roots.
Never by basename `foo.h` alone.

## Validation Fixtures

Before swupdate revalidation:

1. **Same-directory baseline** — existing behavior unchanged
2. **Include root resolution** — `core/x.c` includes `"util.h"` → `include/util.h`
3. **Header-to-header resolution** — `include/a.h` includes `"b.h"` → `include/b.h`
4. **Project-root-relative** — `"include/util.h"` resolves from any location
5. **Ambiguous basename** — two `config.h` in different roots → Ambiguous
6. **Configured roots priority** — configured root wins over conventional
7. **Configured roots source** — CLI option overrides config file
8. **No sibling magic** — `handlers/x.c` includes `"core/y.h"` stays unresolved
9. **No suffix guessing** — `"foo"` does not match `foo.h`
10. **Exact subpath** — `"sub/foo.h"` matches only `root/sub/foo.h`, not `root/foo.h`

## Success Criteria

1. swupdate zero-connectivity modules reduce materially (from 15)
2. swupdate module topology becomes usable (`rmap stats` shows real fan_in/fan_out)
3. `rmap trust` import_graph reliability improves from LOW
4. No false positives (ambiguous cases reported, not guessed)
5. sqlite behavior unchanged (regression test)
6. Header-to-header includes resolve (symmetric policy proof)

## Implementation Substeps

1. **Support module skeleton** — `include_resolver.rs`, types, trait
2. **Same-directory passthrough** — delegate v1.0 behavior
3. **Conventional root search** — `include/`, `inc/`, `src/include/`
4. **Ambiguity detection** — multiple matches → Ambiguous
5. **Integration** — wire into orchestrator
6. **Trust surface update** — ambiguous category in trust report
7. **Validation fixtures** — unit tests per fixture list
8. **swupdate revalidation** — measure improvement
9. **sqlite regression** — verify no degradation

## Future Slices (Explicit Deferral)

**v1.2: compile_commands.json**
- Parse compile_commands.json if present
- Extract `-I` include paths per file
- Use as authoritative resolution source

**v1.3: Build-system detection**
- Detect CMake, Meson, Make include patterns
- Extract include roots from build files

These are NOT in v1.1. They add complexity and external dependencies.
v1.1 should prove the architecture with deterministic conventional roots first.
