# Spike: `rmap orient --focus` on repo-graph itself

**Date:** 2026-04-17
**Slice context:** Rust-44 validation. Path-area and file focus
shipped; spike validates the new focus resolution, prefix scoping,
and signal emission rules before starting Rust-45 (symbol focus).

## Method

1. Built `rmap` release binary with Rust-44 changes.
2. Fresh index:
   `rmap index <repo> /tmp/rgspike2.db`
   Result: `495 files, 3630 nodes, 2091 edges (4556 unresolved)`.
3. Ran five focus scenarios:
   - Repo-level (baseline): `rmap orient /tmp/rgspike2.db repo-graph --budget large`
   - Path-area `src/core`: 60 files, 1544 symbols
   - Path-area `src/adapters/storage`: 18 files, 152 symbols
   - Path-area `src/core/seams`: 13 files, 234 symbols (cycle participant)
   - File `src/core/trust/service.ts`: 1 file, 8 symbols

## Findings

### S44-1 — FIXED. Path-scoped cycle filter used short names instead of qualified paths

**Severity:** P2 (correctness bug, caught and fixed during this spike).

**Problem:** `find_cycles_involving_path` in the storage adapter
compared `CycleNode.name` (short display name like `seams`) against
the focus path prefix (`src/core/seams`). Short names almost never
match full path prefixes, so path-scoped `IMPORT_CYCLES` was
silently empty on every focus.

**Fix:** Changed the adapter to look up `qualified_name` (full path
like `src/core/seams`) for each cycle node. The prefix check now
matches correctly. Path-scoped cycle evidence also uses
qualified_names instead of short names, which is more informative.

**Verification after fix:**
- `src/core` focus: 2 cycles (`src/core/seams ↔ src/core/seams/detectors`,
  `src/core/classification ↔ src/core/ports`)
- `src/core/seams` focus: 1 cycle (`src/core/seams ↔ src/core/seams/detectors`)
- `src/adapters/storage` focus: 0 cycles (correct — no cycles involve
  storage modules)
- Repo-level: unchanged (5 cycles with short names, repo-level
  aggregator is a separate code path)

### S44-2 — Informational. Repo-level cycles still use short names

The repo-level `find_module_cycles` path maps `CycleNode.name`
(short names) into `AgentCycle.modules`. Path-scoped cycles now
use qualified_names. This divergence is not a bug — the repo-level
aggregator and the path-scoped aggregator go through different
code paths. But it means an agent reading repo-level cycle evidence
sees `seams → detectors` while a path-scoped response for the same
repo shows `src/core/seams → src/core/seams/detectors`.

**Action:** Optional follow-up: change repo-level cycle aggregator
to also use qualified_names. Not a correctness issue; output quality
improvement. Deferred.

### S44-3 — Informational. Module-name focus strings resolve as no_match

Typing `--focus seams` resolves as `no_match` because `seams` is
a module short name, not a repo-relative path. The user must type
`--focus src/core/seams` for path resolution or
`--focus repo-graph:src/core/seams:MODULE` for stable-key resolution.

This is correct behavior per the contract precedence rules. But it
is a UX gap when following up on cycle evidence from the repo-level
response, which shows short names. An agent would need to cross-
reference cycle module names to paths before re-issuing orient.

**Action:** Either (a) change repo-level cycle evidence to use full
paths (same as S44-2), or (b) add module-name resolution as a
future step in the focus resolver. Both deferred.

### S44-4 — Confirmed correct. File focus suppression rules

`src/core/trust/service.ts` file focus:
- Emits: TRUST_NO_ENRICHMENT, MODULE_SUMMARY (1 file, 8 symbols),
  SNAPSHOT_INFO.
- Suppressed: BOUNDARY_VIOLATIONS, IMPORT_CYCLES, gate signals.
- Limits: 3 (no GATE_NOT_CONFIGURED — file focus does not
  participate in gate).

All correct per the signal emission matrix.

### S44-5 — Confirmed correct. MODULE_SUMMARY counts are correctly scoped

| Focus | Files | Symbols |
|---|---|---|
| Repo | 495 | 3104 |
| `src/core` | 60 | 1544 |
| `src/adapters/storage` | 18 | 152 |
| `src/core/seams` | 13 | 234 |
| `src/core/trust/service.ts` | 1 | 8 |

All counts are internally consistent with each other.

### S44-6 — Confirmed correct. Focus resolution

| Input | Resolved | Kind | Has key? | Has path? |
|---|---|---|---|---|
| (none) | yes | repo | no | no |
| `src/core` | yes | module | yes (MODULE node) | yes |
| `src/adapters/storage` | yes | module | yes | yes |
| `src/core/trust/service.ts` | yes | file | yes (FILE node) | yes |
| `src/core/seams` | yes | module | yes | yes |
| `seams` | no | — | — | — |
| `nonexistent/path` | no | — | — | — |

All correct per the contract precedence rules.

### S44-7 — Confirmed correct. DEAD_CODE_UNRELIABLE fires at all scopes

The trust reliability axis is repo-wide LOW
(`missing_entrypoint_declarations`), so `DEAD_CODE_UNRELIABLE`
correctly fires at every focus level. The dead-code aggregator's
reliability gate is scope-independent.

### S44-8 — Confirmed correct. No boundary violations at any scope

No boundary declarations exist in this DB. `BOUNDARY_VIOLATIONS`
is correctly absent at all module-focus levels. Would need a
declared boundary to exercise the path-scoped boundary aggregator.

## Summary

| Finding | Status |
|---|---|
| S44-1 Cycle filter used short names | **FIXED** (qualified_name lookup added to adapter) |
| S44-2 Repo-level cycles use short names | Informational, deferred |
| S44-3 Module-name focus → no_match | Informational, correct per contract |
| S44-4 File focus suppression | Confirmed correct |
| S44-5 MODULE_SUMMARY scoping | Confirmed correct |
| S44-6 Focus resolution | Confirmed correct |
| S44-7 DEAD_CODE_UNRELIABLE at all scopes | Confirmed correct |
| S44-8 No boundary violations | Confirmed correct (no declarations in DB) |

## Conclusion

Rust-44 is output-correct after the S44-1 fix. The spike exercised:
- Path-area focus at three depths (top-level module, intermediate,
  leaf with cycle)
- File focus with signal suppression
- No-match resolution for short names and nonexistent paths
- Focus resolution with stable keys (via the FILE path → key path)

The remaining informational findings (S44-2, S44-3) are
output-quality improvements, not correctness issues.

**Rust-45 can proceed.**
