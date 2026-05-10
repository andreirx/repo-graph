# CURRENT_SLICE.md

## Current Priority

Module truth-model unification (`docs/slices/rust-module-parity.md`).

## Active Slice

**Rust Module Parity — Phase 4: MODULE-node fallback deprecation — DONE**

## Branch Intent

Phase 4 complete. MODULE-node fallback deprecated. `module_candidates` is now the
sole source of module topology. Empty `module_candidates` surfaces repos that need
module detection rather than silently synthesizing from generic graph structure.

## Phase 3 Status

**First cut implemented.** Coarse top-level directory heuristic populates `module_candidates` for manifest-less repos. This is orientation-grade inferred topology, not full module-truth completion.

What exists:
- `rust/crates/indexer/src/inferred_modules.rs` — directory heuristic detection
- Compose wiring for detection and persistence
- Identity contract: `inferred:{repo_uid}:{directory_path}`
- Evidence type: `directory_heuristic`
- Confidence: 0.7 (vs 1.0 for declared)
- 14 unit tests

What the heuristic does:
- Scan source file paths
- Group by top-level directory
- Ignore non-source files
- Suppress pure test-only dirs when real source dirs exist
- Create inferred modules for top-level source-bearing dirs
- Fallback to root `.` only if repo is flat

What was validated:
- sqlite: 5 top-level source directories detected (test/ suppressed)
- nginx: 1 module (src with 396 files)
- leveldb: 6 top-level source directories (benchmarks/ excluded)
- linux: 23 top-level source partitions (not "subsystems")

What is weak (addressed in Phase 3.1):
- Very coarse granularity (known limitation, umbrella splitting is Phase 3.2)
- No umbrella-directory splitting (nginx `src` should be `src/core`, `src/http`, etc.)

What was addressed in Phase 3.1:
- Sanity metrics added (6 metrics including unowned_breakdown)
- Refresh parity tests added (4 tests)

## Phase 3.1 Validation (2026-05-09)

### Exclusions validated

**leveldb:**
- Before: 7 modules
- After: 6 modules (`benchmarks/` excluded as BenchmarkOnly)

**linux (stress test):**
- Before: 23 top-level partitions
- After: 21 modules
- `Documentation/` (4 files) excluded as Documentation
- `samples/` (193 files) excluded as ExamplesOrSamples

**sqlite:** 5 modules (test-only suppression working)
**nginx:** 1 module (src with 396 files)

### Refresh parity validated

4 compose-level tests in `rust/crates/repo-index/src/compose.rs`:
- `inferred_modules_detected_on_first_index`
- `inferred_modules_stable_across_refresh` — UID/key/ownership stability
- `inferred_modules_update_on_directory_addition`
- `inferred_modules_disappear_when_directory_emptied`

### Sanity metrics validated

Example output from sqlite (final model with unowned_breakdown):
```json
{
  "largest_module_ownership_pct": 54.9,
  "tiny_module_count": 2,
  "root_fallback_used": false,
  "mixed_language_module_count": 1,
  "has_inferred_modules": true,
  "unowned_breakdown": {
    "excluded_count": 0,
    "suppressed_test_count": 33,
    "true_gap_count": 0,
    "true_gap_pct": 0.0,
    "classified_pct": 100.0
  }
}
```

### Test coverage

21 unit tests in `inferred_modules.rs` covering:
- Basic detection (sqlite, nginx, flat repo)
- Test-only suppression
- All exclusion categories (vendor, build, docs, examples, benchmarks, generated)
- Evidence payload serialization

49 total tests in `repo-graph-repo-index` crate (all passing)

## Phase 3 Hardening Backlog

### Phase 3.1B — Ownership gap analysis — COMPLETE

**Goal:** Understand the unowned-file gap before Phase 4.

**Deliverables:**
- [x] `rmap modules unowned` command — lists unowned files with reason classification
- [x] Analysis of all validation repos
- [x] Root cause identification

**Unowned file analysis:**

| Repo | Unowned | Breakdown |
|------|---------|-----------|
| sqlite | 33 (7.3%) | All in suppressed `test/` directory |
| leveldb | 7 (5.3%) | 4 in `benchmarks/` (excluded), 3 in `issues/` (test files) |
| linux | 211 (0.33%) | 193 in `samples/`, 18 in `Documentation/` (both excluded) |

**Root causes identified:**
1. `suppressed_test_directory` — test-only dirs suppressed when real source dirs exist (intentional)
2. `excluded_directory:*` — files in explicitly excluded directories (intentional)
3. `heuristic_gap:issues` — small directory not recognized as module (3 files in leveldb)

**Conclusion:** The "unowned" metric is not a real gap — it's mostly intentional exclusions and suppressions. True heuristic gaps are minimal (0.3% real gap across repos).

**Phase 4 decision:** The unowned files are understood and classified. The remaining gaps are:
- Intentional exclusions (benchmarks, samples, docs) — not a problem
- Test directory suppression — intentional design choice
- Minor heuristic gaps (leveldb `issues/`) — 3 files total

This is acceptable for Phase 4 if agents can query `modules unowned` to understand residuals.

### Phase 3.1 — Low risk, high value — COMPLETE

**Done:**
- [x] Directory exclusions (vendor/, third_party/, node_modules/, dist/, build/, out/, target/, generated/, docs/, examples/, samples/)
- [x] Richer evidence payload: source_file_count, test_file_count, excluded_directories with reasons
- [x] Surface certainty explicitly in `rmap modules list/files` (module_kind, confidence already present)
- [x] Refresh/parity tests — 4 tests proving identity stability, ownership preservation, and correct update semantics
- [x] Sanity metrics — 6 metrics surfaced in `modules list` output

**Sanity metrics surfaced:**
1. `largest_module_ownership_pct` — % of files in largest module (coarse granularity signal)
2. `tiny_module_count` — modules with < 3 files (over-splitting signal)
3. `root_fallback_used` — flat-repo fallback triggered
4. `mixed_language_module_count` — modules with multiple languages
5. `has_inferred_modules` — whether any inferred modules exist
6. `unowned_breakdown` — ownership classification (see Phase 3.1B model below)

**Optional (polish):**
- [ ] Dominant language in evidence

### Phase 3.2 — Medium risk, high value
- [ ] Umbrella-directory splitting (src/*, packages/*, services/*, apps/*, libs/*)
- [ ] Refined root fallback (prefer children over root when root has only launcher files)
- [ ] Build-file evidence integration (CMakeLists.txt, Makefile, Kbuild, meson.build, BUILD.bazel)

### Phase 3.3 — Optional hardening
- [ ] Parent-child module relationships (separate from ownership)
- [ ] Graph-assisted split/merge experiments (behind flag)

## Phase Ordering (LOCKED)

1. **Phase 2** — package.json / pnpm-workspace.yaml — DONE
2. **Phase 2c** — pyproject.toml single-package — DONE
3. **Phase 2b** — Gradle settings.gradle — DONE
4. **Phase 3** — inferred modules first cut — DONE
5. **Phase 3.1 hardening** — quality improvements — DONE
6. **Phase 3.1B** — ownership gap analysis — DONE
7. **Phase 3.2** — umbrella splitting, build-file evidence — NOT STARTED (optional)
8. **Phase 4** — MODULE-node fallback deprecation — DONE (2026-05-10)

**Phase 4 gate (minimum requirements):**
- [x] refresh/parity tests passing (4 tests)
- [x] sanity metrics with proper semantic separation (see below)
- [x] unowned files classified by reason (excluded/suppressed_test/true_gap)
- [x] `rmap modules unowned` for detailed investigation
- [x] true_gap_pct thresholds defined and met (≤3% per repo)
- [x] degradation wording in CLI output (warnings field)

**Sanity metric model (corrected):**
```
unowned_breakdown:
  excluded_count       — files in excluded directories (intentional)
  suppressed_test_count — files in test-only dirs suppressed (intentional)
  true_gap_count       — actual heuristic failures
  true_gap_pct         — true gaps as % of total files (the real gate metric)
  classified_pct       — should always be 100%
```

**Validation results:**

| Repo | excluded | suppressed_test | true_gap | true_gap_pct |
|------|----------|-----------------|----------|--------------|
| sqlite | 0 | 33 | 0 | 0.0% |
| leveldb | 4 | 0 | 3 | 2.26% |
| linux | 211 | 0 | 0 | 0.0% |

## Phase 4 Gate Definition (LOCKED)

This is a **product decision**, not an implementation detail.

### Gate conditions

| Condition | Threshold | Rationale |
|-----------|-----------|-----------|
| `classified_pct` | = 100% | Every unowned file must have a classified reason |
| `true_gap_pct` | ≤ 3% per repo | Orientation-grade tolerance; tighter than this forces heuristic special-casing |
| Degradation wording | Required | Agents must see uncertainty when inferred modules exist |
| Refresh parity | No regressions | Identity/ownership must survive refresh |

### Why 3% threshold (not 1%)

This is an orientation product, not a verified truth product. Trade-off accepted:
- 3% allows practical progress without blocking on small heuristic residues
- Residuals are classified and surfaced, not hidden
- Agents can query `modules unowned` to understand any gaps
- Stricter threshold (1%) would force repo-specific heuristic patches

### Fallback deprecation allowed when

All of the following hold:
1. `classified_pct = 100%` on all validation repos
2. `true_gap_pct <= 3%` on each validation repo
3. Degradation wording present in `modules list` warnings
4. No refresh parity test regressions

## Known Residual: leveldb `issues/` (DOCUMENTED)

### Classification: True gap (not debt, not policy change)

The 3 unowned files are:
- `issues/issue178_test.cc`
- `issues/issue200_test.cc`
- `issues/issue320_test.cc`

### Why they are unowned

1. Directory name `issues` is not in the test directory list
2. Files end in `_test.cc` but the heuristic only checks directory names, not file suffixes for test classification at the module level

### Why this is acceptable

- 3 files out of 133 total = 2.26% (within 3% threshold)
- These are edge-case test files in an unconventional directory
- Adding `issues` to test directory list would be repo-specific patching
- The gap is measured, classified, and surfaced to agents

### Future options (not blocking Phase 4)

- Enhance test detection to check file suffixes (`*_test.cc`) regardless of directory
- This would be a heuristic improvement, not a policy change
- Not required for Phase 4 gate

## Validation Status

| Repo | classified_pct | true_gap_pct | Status |
|------|----------------|--------------|--------|
| sqlite | 100% | 0.0% | PASS |
| leveldb | 100% | 2.26% | PASS |
| linux | 100% | 0.0% | PASS |

**Phase 4 gate: MET**

## Phase 4 Implementation (2026-05-10)

### Changes made

1. **`rust/crates/module-queries/src/context.rs`**
   - Removed fallback code path from `ModuleQueryContext::load`
   - `module_candidates` table is now the sole source of module data
   - `is_fallback` field retained but always set to `false` for backward compatibility
   - Empty `module_candidates` results in empty context (surfaces repos needing detection)

2. **Documentation updates**
   - `docs/slices/rust-module-parity.md` — Status changed from PLANNED to DONE
   - `docs/ROADMAP.md` — Follow-on section updated to reflect completion
   - `CURRENT_SLICE.md` — Phase 4 marked DONE

### Validation results

All 4 validation repos pass Phase 4 gate after fallback removal:

| Repo | classified_pct | true_gap_pct | Modules | Status |
|------|----------------|--------------|---------|--------|
| sqlite | 100% | 0.0% | 5 | PASS |
| leveldb | 100% | 2.26% | 6 | PASS |
| nginx | 100% | 0.0% | 1 | PASS |
| linux | 100% | 0.0% | 21 | PASS |

All 54 unit tests in `module-queries` and `repo-index` crates pass.

### Behavior change

Prior to Phase 4, repos without `module_candidates` would fall back to MODULE nodes
from the generic `nodes` table. This created a bifurcated data model where consumers
couldn't trust module data consistency.

After Phase 4, empty `module_candidates` means the repo needs module detection
configured. This is intentional: it surfaces repos that need attention rather than
silently providing degraded data.

## Sanity Metrics (Final Model)

Trust surface for heuristic topology in `modules list` output:

**Module quality:**
- `largest_module_ownership_pct` — coarse granularity signal
- `tiny_module_count` — over-splitting signal (threshold: 3 files)
- `mixed_language_module_count` — mixed-language modules
- `root_fallback_used` — flat-repo fallback triggered
- `has_inferred_modules` — whether heuristic inference was used

**Ownership classification:**
```
unowned_breakdown:
  excluded_count        — files in excluded directories (intentional)
  suppressed_test_count — files in suppressed test dirs (intentional)
  true_gap_count        — actual heuristic failures
  true_gap_pct          — the real gate metric
  classified_pct        — should always be 100%
```

These are not cosmetic. They are the trust surface for heuristic topology.

## Architecture Decisions (LOCKED)

### Single canonical owner per file
Each file belongs to one module. No overlapping ownership. Simpler queries, cleaner dependency graph, easier trust semantics.

### Build/layout-aware heuristics first
Use directory structure, build files, naming conventions, exclusions. Graph-assisted inference deferred until simple heuristic is well-instrumented.

### Inferred modules are orientation-grade
Do not treat inferred modules as declared truth. Lower confidence, explicit evidence basis, honest degradation reporting.

## Validation Repos

1. `/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/sqlite` — minimal C repo
2. `/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/nginx` — medium C tree
3. `/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb` — C++ before kernel scale
4. `/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/linux` — stress corpus

## Approved DB Path

`./test-artifacts/repo-graph.db`

Do not create databases elsewhere.

## Key References

- `rust/crates/indexer/src/inferred_modules.rs` — Phase 3 heuristic
- `rust/crates/indexer/src/cargo_manifest.rs` — declared module pattern
- `rust/crates/repo-index/src/compose.rs` — wiring location
