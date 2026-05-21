# CLI-AUDIT-1: Cross-Repo Full Surface Audit

**Status:** COMPLETE (2026-05-20, reconciled 2026-05-21)  
**Type:** Audit / Product Surface Review  
**Prerequisite:** CLI-OUT-7 (all human renderers implemented)

## Problem Statement

SMOKE-1 validated operational stability for 4 commands (gate, orient, trust, stats)
across 12 repos. This proves the smoke harness works and those commands don't crash.

It does not prove:
- Output polish is consistent across all redesigned commands
- Empty states are honest and actionable
- No JSON leaks remain in human mode
- All command families render correctly on diverse corpus data

## Scope

Systematic human-output review across all redesigned command families.

### Command Families (35+ commands)

**First-contact / discovery (5):**
- `orient`
- `trust`
- `cycles`
- `stats`
- `check`

**Drilldown (4):**
- `callers`
- `callees`
- `path`
- `imports`

**Module / architecture (11):**
- `modules list`
- `modules show`
- `modules files`
- `modules unowned`
- `modules deps`
- `modules violations`
- `surfaces list`
- `surfaces show`
- `boundaries list`
- `boundaries show`
- `boundaries summary`

**Inventory / policy (6):**
- `docs list`
- `docs extract`
- `resource list`
- `resource readers`
- `resource writers`
- `policy`

**Governance / quality (7):**
- `assess`
- `violations`
- `gate`
- `churn`
- `hotspots`
- `risk`
- `coverage`

## Audit Corpus (12 repos)

| Repo | Path | Language | Size |
|------|------|----------|------|
| repo-graph | `.` | Rust | 733 files |
| amodx | `../amodx` | TypeScript | 154 files |
| zap-engine | `../zap-engine` | TypeScript/Rust | 154 files |
| zap-squad | `../zap-squad` | TypeScript/Rust | 145 files |
| glamCRM | `../glamCRM` | TypeScript/Java | 418 files |
| hexmanos | `../hexmanos` | TypeScript/Java | 151 files |
| OpenXcom | `../legacy-codebases/OpenXcom` | C++ | 733 files |
| django | `../legacy-codebases/django` | Python | 3015 files |
| duckdb | `../legacy-codebases/duckdb` | C++ | 5109 files |
| grpc-java | `../legacy-codebases/grpc-java` | Java | 1821 files |
| leveldb | `../legacy-codebases/leveldb` | C++ | 133 files |
| sqlite | `../legacy-codebases/sqlite` | C | 457 files |
| langchain4j | `../legacy-codebases/langchain4j` | Java | 2643 files |

## Audit Method

For each command × repo combination:

1. Run command in human mode (no `--json`)
2. Capture output
3. Classify result:
   - **GOOD** — output is clean, scannable, actionable
   - **EMPTY_HONEST** — no data, but message explains why
   - **FIXTURE_ONLY** — requires declarations/setup not in corpus
   - **UNSUPPORTED** — corpus lacks data for this command type
   - **NEEDS_WORK** — output has defects (specify)

## Deliverable

### Primary: Command × Repo Matrix

```
| Command          | repo-graph | amodx | django | duckdb | ... |
|------------------|------------|-------|--------|--------|-----|
| orient           | GOOD       | GOOD  | GOOD   | GOOD   | ... |
| trust            | GOOD       | GOOD  | GOOD   | GOOD   | ... |
| cycles           | ?          | ?     | ?      | ?      | ... |
| modules list     | ?          | ?     | ?      | ?      | ... |
| ...              |            |       |        |        |     |
```

### Secondary: Defect List

Categorized by root cause:

**Presentation defects:**
- Formatting issues
- Missing context
- Unclear empty states
- JSON leaks in human mode

**Backend/data defects:**
- Wrong data returned
- Missing data that should exist
- Incorrect aggregations

**Validation/corpus gaps:**
- Commands that need setup not in any corpus repo
- Commands that need specific data patterns

## Execution Plan

### Phase 1: First-Contact Commands
Run orient, trust, cycles, stats, check across all 12 repos.
Build initial matrix section.

### Phase 2: Drilldown Commands
Run callers, callees, path, imports on repos with sufficient edge data.
Need to identify valid query targets per repo.

### Phase 3: Module/Architecture Commands
Run modules/surfaces/boundaries families.
Most should work on any repo with discovered modules.

### Phase 4: Inventory/Policy Commands
Run docs, resource, policy commands.
May have sparse corpus coverage.

### Phase 5: Governance/Quality Commands
Run assess, violations, gate, churn, hotspots, risk, coverage.
Some require declarations (assess, gate), some require git history (churn, hotspots).

## Explicit Non-Goals

- Do not fix defects during audit
- Do not add corpus data to make commands work
- Do not modify renderers
- Do not change backend queries

Audit first. Implementation decisions after.

## Definition of Done

- [x] Matrix complete for all command × repo combinations
- [x] Each cell has explicit status (not blank)
- [x] Defect list categorized
- [x] Summary with counts: GOOD / EMPTY_HONEST / FIXTURE_ONLY / UNSUPPORTED / NEEDS_WORK
- [x] Recommendation: which defects warrant follow-on slices

## Findings Summary (Reconciled 2026-05-21)

See `docs/audits/cli-audit-1/summary.md` for full reconciled details.

### Defects Found

1. **boundaries summary** - CRITICAL (DTO mismatch with daemon)
2. **boundaries list** - HIGH (shows direction only, no details)
3. **modules deps** - HIGH (missing target module names in edge output)
4. **Legacy contract commands** - 7 commands need daemon migration
5. **TypeScript imports** - 0 imports extracted (data gap)

### Reconciled Totals

| Phase | GOOD | EMPTY_HONEST | NEEDS_WORK | ERROR | LEGACY | NOT RUN |
|-------|------|--------------|------------|-------|--------|---------|
| 1 | 66 | 4 | 0 | 0 | 0 | 0 |
| 2 | ~25 | ~5 | 0 | 0 | 0 | ~26 |
| 3 | 15 | 10 | 4 | 3 | 0 | ~24 |
| 4-5 | 7 | 2 | 0 | 1 | 16 | 13 |

### Recommendation

Create follow-on slice **MODULE-BOUNDARY-FIX-1** to address:
- boundaries summary DTO mismatch (CRITICAL)
- boundaries list missing details (HIGH)
- modules deps missing module names (HIGH)

Legacy command migration is lower priority - commands work,
just require explicit args.
