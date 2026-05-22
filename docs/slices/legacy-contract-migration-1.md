# LEGACY-CONTRACT-MIGRATION-1: Daemon Migration for Legacy Commands

**Status:** QUEUED  
**Type:** Refactor / Contract Alignment  
**Prerequisite:** REG-1 complete (daemon registry infrastructure exists)  
**Discovered:** CLI-AUDIT-1 (2026-05-20)

## Problem Statement

Seven CLI commands still use the legacy direct-storage contract requiring explicit `<db_path> <repo_uid>` arguments. All other read-side commands have migrated to the REG-1 daemon contract (auto-discovery from cwd, no leaked storage concepts).

**Legacy commands:**
| Command | Category | Current Contract |
|---------|----------|------------------|
| assess | Governance | `rmap assess <db_path> <repo_uid>` |
| violations | Governance | `rmap violations <db_path> <repo_uid>` |
| churn | Quality | `rmap churn <db_path> <repo_uid>` |
| hotspots | Quality | `rmap hotspots <db_path> <repo_uid>` |
| risk | Quality | `rmap risk <db_path> <repo_uid>` |
| coverage | Quality | `rmap coverage <db_path> <repo_uid> <report>` |
| policy | Inventory | `rmap policy <db_path> <repo_uid>` |

**Target contract (REG-1 pattern):**
```bash
rmap assess           # auto-discover from cwd
rmap violations       # auto-discover from cwd
rmap churn            # auto-discover from cwd
rmap coverage <report>  # report path is the only required arg
```

## Why This Matters

1. **Adoption friction:** Users must know internal storage paths
2. **Contract inconsistency:** Some commands work from cwd, others don't
3. **Leaky abstraction:** `repo_uid` is daemon-internal identity

## Scope

**In scope:**
- Daemon handler implementation for each command (7 handlers)
- CLI presentation layer migration (7 commands)
- Existing human/JSON renderers preserved (already implemented in CLI-OUT-5/6/7)

**Out of scope:**
- New renderer work (renderers exist)
- Storage schema changes (queries exist, just need daemon routing)
- Write-side commands (separate track)

## Architecture Pattern

Each legacy command follows the same migration pattern:

1. **Daemon handler:** Route request to existing query logic
2. **CLI command:** Replace direct storage call with daemon RPC
3. **Response DTO:** Reuse existing DTO (already validated in CLI-OUT track)

Reference implementation: Any REG-1 command (e.g., `modules list`, `surfaces list`)

## Migration Checklist

For each command:
- [ ] Daemon handler in `daemon-runtime/src/handlers/`
- [ ] Register handler in daemon router
- [ ] CLI command calls daemon via socket
- [ ] Remove db_path/repo_uid positional args
- [ ] Validate human output unchanged
- [ ] Validate JSON output unchanged

## Command-Specific Notes

### assess
- Returns repo assessment with violation counts, risk scores
- Query exists in storage crate

### violations
- Returns policy violations list
- Query exists, used by `gate` (already migrated)

### churn
- Returns file churn metrics with time window
- Query exists in storage crate

### hotspots
- Returns ranked hotspot files
- Query exists in storage crate

### risk
- Returns risk-scored files with metadata
- Query exists in storage crate

### coverage
- Takes report path argument (keep as positional)
- Returns coverage gaps
- Query exists in storage crate

### policy
- Returns policy facts (STATUS_MAPPING, BEHAVIORAL_MARKER, RETURN_FATE)
- Query exists in storage crate

## Definition of Done

- [ ] All 7 commands work with `rmap <cmd>` from registered repo cwd
- [ ] Human output identical to legacy contract
- [ ] JSON output identical to legacy contract
- [ ] No db_path/repo_uid in user-facing contract
- [ ] Smoke validation on corpus repos

## Files in Scope

- `rust/crates/daemon-runtime/src/handlers/` (new handlers)
- `rust/crates/daemon-runtime/src/router.rs` (handler registration)
- `rust/crates/rgr/src/commands/` (CLI command files)
- `rust/crates/rgr/src/presentation/` (no changes expected, renderers exist)

## Risk Assessment

**Low risk:**
- Queries already exist and are validated
- Renderers already exist and are validated
- Pattern is well-established (REG-1 commands as reference)
- No storage schema changes

**Testing approach:**
- Reuse existing test fixtures
- Compare output before/after migration
