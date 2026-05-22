# LEGACY-CONTRACT-MIGRATION-1: Daemon Migration for Legacy Commands

**Status:** CURRENT  
**Type:** Refactor / Contract Alignment  
**Prerequisite:** REG-1 complete (daemon registry infrastructure exists)  
**Discovered:** CLI-AUDIT-1 (2026-05-20)

## Problem Statement

Seven CLI commands still use the legacy direct-storage contract requiring explicit `<db_path> <repo_uid>` arguments. All other read-side commands have migrated to the REG-1 daemon contract (auto-discovery from cwd, no leaked storage concepts).

**Legacy commands:**
| Command | Category | Current Contract | Write Path |
|---------|----------|------------------|------------|
| assess | Governance | `rmap assess <db_path> <repo_uid>` | YES (persists assessments) |
| violations | Governance | `rmap violations <db_path> <repo_uid>` | no |
| churn | Quality | `rmap churn <db_path> <repo_uid>` | no |
| hotspots | Quality | `rmap hotspots <db_path> <repo_uid>` | no |
| risk | Quality | `rmap risk <db_path> <repo_uid>` | no |
| coverage | Quality | `rmap coverage <db_path> <repo_uid> <report>` | YES (imports report) |
| policy | Inventory | `rmap policy <db_path> <repo_uid>` | no |

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

## Sub-Slice Structure

Migration proceeds in four sub-slices to minimize blast radius and isolate write-path commands.

### LEGACY-CONTRACT-MIGRATION-1A: Shared CLI Support

**Scope:** CLI-side shared support for REG-1-style commands. No command migration yet.

**Deliverables:**
- `rust/crates/rgr/src/daemon_command.rs` (new module)
- Repo resolution from cwd (`resolve_repo_from_cwd()`)
- Daemon availability handling
- Request execution wrapper with timeout
- Repo-not-found / runtime error mapping with hints
- JSON passthrough vs DTO rendering switch
- Exit code mapping (0=success, 1=usage, 2=runtime)

**Explicitly out of scope:**
- Daemon handlers (added in 1B-1D)
- Command migrations (added in 1B-1D)
- Daemon-side shared logic (if needed, separate module)

**Validation:**
- Unit tests for repo resolution and error classification
- First real consumer proof deferred to 1B (support module implemented and unit-tested; no existing command refactored yet)

### LEGACY-CONTRACT-MIGRATION-1B: Quality Family

**Commands:** `churn`, `hotspots`, `risk`, `coverage`

**Why first:**
- Strongest shared mechanics (`--since` time-window pattern)
- Mostly read/query path
- Exercises the support module before write-sensitive commands

**Special notes:**
- `coverage` is operationally different: performs report ingestion (write) then query/report
- `coverage` keeps `<report>` as positional argument

**Daemon handlers:**
- `churn` → method `"churn"`
- `hotspots` → method `"hotspots"`
- `risk` → method `"risk"`
- `coverage` → method `"coverage"` (params include `report_path`)

**Validation per command:**
1. Daemon handler unit test
2. CLI command test (usage error, success, filters)
3. Repo-not-found path
4. Daemon-unavailable path
5. `--json` output parity with legacy
6. Human output parity with legacy
7. Real corpus run (classifier-repo fixture minimum)

**Additional for coverage:**
- Write-path validation: report import succeeds
- Invalid report path error handling

### LEGACY-CONTRACT-MIGRATION-1C: Governance Family

**Commands:** `assess`, `violations`

**Why second:**
- `assess` writes (persists assessment results)
- Governance semantics more sensitive
- Better isolated after query-family path proven

**Special notes:**
- `assess` runs `QualityPolicyRunner::assess_snapshot()` which writes to storage
- `assess` has `--baseline` optional parameter
- `violations` is read-only despite governance category

**Daemon handlers:**
- `assess` → method `"assess"` (params include optional `baseline_snapshot_uid`)
- `violations` → method `"violations"`

**Validation per command:**
1. Daemon handler unit test
2. CLI command test
3. Repo-not-found path
4. Daemon-unavailable path
5. `--json` output parity
6. Human output parity
7. Real corpus run

**Additional for assess:**
- Write-path validation: assessments persisted correctly
- Baseline parameter handling
- Re-run produces same results (idempotent write)

### LEGACY-CONTRACT-MIGRATION-1D: Inventory Family

**Commands:** `policy`

**Why last:**
- One command, smallest blast radius
- Filter-heavy (--kind, --file, --callee, --fate)
- Easy to isolate if policy fact query shape needs special handling

**Daemon handler:**
- `policy` → method `"policy"` (params include filters)

**Validation:**
1. Daemon handler unit test
2. CLI command test (all filter combinations)
3. Repo-not-found path
4. Daemon-unavailable path
5. `--json` output parity
6. Human output parity
7. Real corpus run (needs repo with policy facts)

## Daemon-Side Architecture

If daemon handlers share logic, isolate it:

```
daemon-runtime/src/
  dispatch.rs          # match arms route to handlers
  handlers/            # NEW: handler implementations (if extracted)
    quality.rs         # churn, hotspots, risk, coverage
    governance.rs      # assess, violations
    inventory.rs       # policy
  util/                # shared daemon-side helpers
```

Do NOT duplicate shared query logic across inline handlers in dispatch.rs.

## Definition of Done (Full Slice)

- [x] 1A: Shared CLI support module complete and tested (2026-05-22)
- [x] 1B: Quality family migrated (churn, hotspots, risk, coverage) (2026-05-22)
- [x] 1C: Governance family migrated (assess, violations) (2026-05-22)
- [ ] 1D: Inventory family migrated (policy)
- [ ] All 7 commands work with `rmap <cmd>` from registered repo cwd
- [ ] Human output identical to legacy contract
- [ ] JSON output identical to legacy contract
- [ ] No db_path/repo_uid in user-facing contract
- [ ] Write-path validation for assess and coverage
- [ ] Smoke validation on corpus repos

## Files in Scope

**CLI (rgr crate):**
- `rust/crates/rgr/src/daemon_command.rs` — NEW: shared support
- `rust/crates/rgr/src/commands/assess.rs`
- `rust/crates/rgr/src/commands/modules/violations.rs`
- `rust/crates/rgr/src/commands/quality/churn.rs`
- `rust/crates/rgr/src/commands/quality/hotspots.rs`
- `rust/crates/rgr/src/commands/quality/risk.rs`
- `rust/crates/rgr/src/commands/quality/coverage_cmd.rs`
- `rust/crates/rgr/src/commands/policy.rs`

**Daemon (daemon-runtime crate):**
- `rust/crates/daemon-runtime/src/dispatch.rs` — add 7 handlers
- `rust/crates/daemon-runtime/src/handlers/` — optional extraction

**Tests:**
- `rust/crates/rgr/tests/assess_command.rs`
- `rust/crates/rgr/tests/violations_command.rs`
- `rust/crates/rgr/tests/churn_command.rs`
- `rust/crates/rgr/tests/hotspots_command.rs`
- `rust/crates/rgr/tests/risk_command.rs`
- `rust/crates/rgr/tests/cli_out_6_quality.rs` (coverage)

## Risk Assessment

**Low risk:**
- Queries already exist and are validated
- Renderers already exist and are validated
- Pattern is well-established (REG-1 commands as reference)
- No storage schema changes

**Medium risk (mitigated by sub-slicing):**
- `assess` write path needs daemon coordination
- `coverage` report import needs file path handling across daemon boundary

**Testing approach:**
- Reuse existing test fixtures
- Compare output before/after migration
- Explicit write-path validation for assess/coverage
