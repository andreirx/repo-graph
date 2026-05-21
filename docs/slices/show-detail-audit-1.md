# SHOW-DETAIL-AUDIT-1: Unexercised Detail Command Audit

**Status:** COMPLETE (2026-05-21)  
**Type:** Audit / Exercise / Fix  
**Prerequisite:** CLI-AUDIT-1 complete, MODULE-BOUNDARY-FIX-1 complete

## Problem Statement

CLI-AUDIT-1 identified four detail commands that were not fully exercised:
- `rmap surfaces show <surface_ref>`
- `rmap boundaries show <surface_uid>`
- `rmap resource readers <resource_key>`
- `rmap resource writers <resource_key>`

These require valid refs/keys obtained from other commands. Without exercising them on real corpus data, the CLI output track remains incomplete.

## Scope

**In scope:**
- Discovery workflow for valid refs/keys
- Real execution on corpus repos
- Human output review for actionability
- JSON output verification
- Defect classification and targeted fixes

**Out of scope:**
- Legacy contract migration (separate track)
- TypeScript extractor fixes (separate track)
- New renderer implementations (already exist)

## Commands Under Test

| Command | Requires | Source Command |
|---------|----------|----------------|
| surfaces show | surface_ref | surfaces list |
| boundaries show | surface_uid | boundaries list |
| resource readers | resource_key | resource list |
| resource writers | resource_key | resource list |

## Discovery Workflow

### Step 1: Identify repos with populated data

From CLI-AUDIT-1 corpus:
- **Surfaces**: repo-graph (72 surfaces)
- **Boundaries**: repo-graph (72 boundaries), grpc-java (116 boundaries)
- **Resources**: repo-graph (has resources), need to verify others

### Step 2: Extract valid refs/keys

```bash
# Get project_surface_uid from surfaces list (--json for parsing)
rmap surfaces list --json | jq '.results[0].project_surface_uid'

# Get surfaceUid from boundaries list (--json for parsing)
rmap boundaries list --json | jq '.results[0].surfaceUid'

# Get stable_key from resource list (--json for parsing)
rmap resource list --json | jq '.results[1].stable_key'
```

### Step 3: Execute detail commands with discovered values

```bash
rmap surfaces show <discovered_ref>
rmap surfaces show <discovered_ref> --json

rmap boundaries show <discovered_uid>
rmap boundaries show <discovered_uid> --json

rmap resource readers <discovered_key>
rmap resource readers <discovered_key> --json

rmap resource writers <discovered_key>
rmap resource writers <discovered_key> --json
```

## Failure Classification

| Category | Meaning | Action |
|----------|---------|--------|
| DISCOVERY_GAP | Can't obtain valid ref/key from source command | Fix source command or document limitation |
| RENDERER_DEFECT | Command executes but output not actionable | Fix renderer |
| DTO_MISMATCH | Parse error on daemon response | Fix CLI DTO (like MODULE-BOUNDARY-FIX-1) |
| BACKEND_ABSENT | No data exists for this command type | Document as corpus limitation |
| GOOD | Command works, output actionable | Mark validated |

## Definition of Done

For each of the 4 commands:
- [x] Valid discovery workflow documented
- [x] At least one real successful execution observed
- [x] Empty/not-found path observed (boundaries show before fix)
- [x] Human output reviewed for actionability
- [x] `--json` path verified
- [x] Audit artifact updated
- [x] Any defects fixed (boundaries show DTO mismatch)

## Validation Matrix

| Command | Repo | Discovery | Human | JSON | Status |
|---------|------|-----------|-------|------|--------|
| surfaces show | repo-graph | ps-2625f0eb | GOOD | GOOD | VALIDATED |
| boundaries show | repo-graph | 2f256aae-... | GOOD (after fix) | GOOD | VALIDATED |
| boundaries show | grpc-java | grpc-client-1af3... | GOOD | not captured | HUMAN_ONLY |
| resource readers | repo-graph | db:sqlite3:app.db | GOOD | GOOD | VALIDATED |
| resource writers | repo-graph | db:sqlite3:app.db | GOOD | GOOD | VALIDATED |

## Execution Plan — COMPLETE

1. [x] Run discovery workflow on repo-graph
2. [x] Execute all 4 commands with discovered values
3. [x] Review outputs, classify results
4. [x] Fixed boundaries show DTO mismatch (detail vs boundary, camelCase renames)
5. [x] Re-validated after fix
6. [x] Validated on grpc-java for boundaries
7. [x] Update audit artifacts
8. [x] Close slice

## Defects Found and Fixed

### boundaries show DTO_MISMATCH — FIXED

**Symptom:** Human output showed "(not found)" but JSON returned full data.

**Root cause:** Two DTO mismatches:
1. Response field named `detail` but DTO expected `boundary`
2. BoundaryDetail fields used snake_case but daemon sends camelCase

**Fix:** Added `#[serde(rename = "detail")]` to response struct and camelCase rename annotations to BoundaryDetail fields.

### Test fix (unrelated pre-existing issue)

`declare_boundary_visible_to_violations` test was calling `violations` without `--json` but expecting JSON output. Added `--json` flag.
