# SMOKE-1: Validation Harness Cleanup

**Status:** PLANNED  
**Type:** Infrastructure / Tooling  
**Priority:** Queued after CURSOR-1  
**Prerequisite:** None  

## Problem Statement

The smoke scripts (`smoke-rmap.sh`, `smoke-validation-repos.sh`) were expedient hacks
updated for REG-1 daemon-based CLI but have structural weaknesses that prevent them
from being trusted as a product-grade validation surface.

Current defects:

1. **Weak multi-command model** — cannot express per-command arguments
2. **Execution vs. domain verdict conflation** — any non-zero exit marked as failure
3. **Incorrect metadata field names** — `repo_uid` contains `repo_name`
4. **Build-environment coupling** — `cargo run --release` on every invocation

These issues were documented in `docs/TECH-DEBT.md` on 2026-05-18 after the Tarjan SCC
fix validation exposed the harness limitations.

## Non-Goals

- Full test framework replacement (pytest, bats, etc.)
- CI integration (separate slice)
- Release-smoke binary harness (separate slice)
- Timeout handling (deferred — macOS portability concern)

## Target Contract

### A. Structured Command Specification

Replace positional command args with explicit `--cmd` flag:

```bash
./scripts/smoke-rmap.sh task repo \
  --cmd "orient --budget small" \
  --cmd "check" \
  --cmd "trust" \
  --cmd "explain src/main.cpp --json"
```

**Implementation constraint:** No `eval`. Commands must be parsed into argv arrays
using a defined grammar (IFS word splitting on space, with quote handling via
`read -a` or explicit tokenizer). Injection risk and quoting ambiguity are
unacceptable in a validation harness.

### B. Execution Status vs. Semantic Outcome Separation

Metadata must distinguish transport-level success from command semantics:

```json
{
  "commands": {
    "check": {
      "transport_status": "ok",
      "exit_code": 1,
      "outcome_kind": "pass_fail",
      "semantic_verdict": "fail",
      "seconds": 2
    },
    "orient": {
      "transport_status": "ok",
      "exit_code": 0,
      "outcome_kind": "informational",
      "semantic_verdict": null,
      "seconds": 1
    },
    "trust": {
      "transport_status": "ok",
      "exit_code": 0,
      "outcome_kind": "informational",
      "semantic_verdict": null,
      "seconds": 1
    },
    "explain": {
      "transport_status": "daemon_error",
      "exit_code": 2,
      "outcome_kind": "unknown",
      "semantic_verdict": null,
      "seconds": 0
    }
  }
}
```

**Transport status values:**
- `ok` — command executed, exit code captured
- `daemon_error` — daemon unavailable or returned error (exit 2)
- `usage_error` — invalid arguments (exit 1 with usage pattern in stderr)

**Outcome kind values:**
- `informational` — command produces data, no pass/fail semantics (orient, trust, stats, cycles)
- `pass_fail` — command has explicit verdict (check, gate)
- `error` — command failed to execute properly
- `unknown` — could not determine

**Semantic verdict:** Only populated when `outcome_kind=pass_fail`. Null otherwise.

**Timeout handling:** Deferred. macOS does not ship with `timeout` utility.
GNU `gtimeout` would require Homebrew dependency. For now, rely on daemon's
internal timeouts and treat hangs as manual abort scenarios.

### C. Correct Metadata Field Names

Current (wrong):
```json
{
  "repo_uid": "OpenXcom"
}
```

Correct:
```json
{
  "repo_name": "OpenXcom",
  "repo_path": "/path/to/OpenXcom",
  "repo_uid": "repo_01krwycwpzvtw2re84m4pjxsxb"
}
```

**Acquisition path:** The `repo_uid` must be obtained via machine-readable daemon
interface, not by scraping human output.

After `rmap index`, the script must call:
```bash
rmap repo info --json
```

And parse the `repo_uid` field from the JSON response. This command exists and
returns structured data regardless of CLI-OUT-1 presentation mode.

If `repo info --json` is unavailable, the script must fail explicitly rather
than fabricate a UID from the directory name.

### D. Output File Typing

Already fixed (2026-05-18): `.txt` for human output, `.json` only for JSON content.

No further changes needed.

### E. Build/Run Separation (Deferred)

For dev-smoke, `cargo run --release` is acceptable.

For release-smoke (future slice), build once and run explicit binaries.
This slice does not address release-smoke.

## Implementation Plan

### Phase 1: Command Model (A)

1. Add `--cmd` flag parsing to `smoke-rmap.sh`
2. Store commands in array as full strings
3. Parse each command string into argv array using `read -a`:
   ```bash
   read -ra CMD_ARGV <<< "$CMD_STRING"
   cargo run ... -- "${CMD_ARGV[@]}"
   ```
4. Reject empty commands, validate basic structure
5. Update usage documentation

### Phase 2: Outcome Model (B)

1. Define outcome_kind classification:
   - `check`, `gate` → `pass_fail`
   - `orient`, `trust`, `stats`, `cycles`, `callers`, `callees`, `explain` → `informational`
   - Exit 2 with daemon error pattern → `error`
2. For `pass_fail` commands, map exit code to verdict:
   - `check`: exit 0 = pass, exit 1 = fail
   - `gate`: exit 0 = pass, exit 1 = fail
3. Update metadata schema
4. Update summary to report by outcome_kind, not raw exit code

### Phase 3: Metadata Correction (C)

1. After `rmap index`, call `rmap repo info --json`
2. Parse JSON to extract `repo_uid`
3. If parsing fails, abort with explicit error (do not fabricate)
4. Store correct field names in metadata

### Phase 4: Validation

1. Run full smoke on OpenXcom with new harness
2. Verify:
   - `orient` has `outcome_kind=informational`, `semantic_verdict=null`
   - `check` has `outcome_kind=pass_fail`, `semantic_verdict=fail`
   - `repo_uid` is actual daemon UID, not directory name
3. Verify command with arguments (`orient --budget small`) executes correctly

## Definition of Done

- [ ] `--cmd` flag accepts per-command argument strings
- [ ] No `eval` in command execution path
- [ ] Metadata includes `transport_status`, `outcome_kind`, `semantic_verdict`
- [ ] `orient` recorded as informational, not pass/fail
- [ ] `check` exit 1 recorded as verdict=fail, not transport failure
- [ ] `repo_uid` obtained from `rmap repo info --json`
- [ ] `repo_name` field contains directory basename
- [ ] Smoke run on OpenXcom produces correct metadata
- [ ] TECH-DEBT.md entry marked as resolved

## Files in Scope

- `scripts/smoke-rmap.sh`
- `scripts/smoke-validation-repos.sh`
- `docs/TECH-DEBT.md` (resolution note)

## Files Out of Scope

- Any Rust code
- CI configuration
- Release pipeline

## Risk Assessment

**Runtime/product-code risk:** Low. Changes confined to shell scripts.

**Validation-integrity risk:** Medium. This harness is part of the validation
evidence pipeline. Bad harness semantics can falsify product confidence.
The Tarjan SCC fix was validated with the current weak harness — the fix is
likely correct, but the evidence quality was compromised by metadata that
conflates execution failure with domain verdicts.

Incorrect harness semantics can:
- Mark working features as broken (false negatives)
- Mark broken features as working (false positives)
- Produce audit trails that misrepresent actual test outcomes

This risk justifies placing SMOKE-1 immediately after CURSOR-1, not in
long-term backlog.

## Roadmap Position

**Queued immediately after CURSOR-1.**

Rationale:
- CURSOR-1 remains current product priority (user-facing integration)
- SMOKE-1 is support infrastructure, not product center
- However, smoke evidence is now operationally important (proved by Tarjan fix)
- Deferring indefinitely keeps validation quality compromised

## Alternatives Considered

### Full test framework (pytest, bats)

Rejected. Overkill for current needs. Shell scripts are sufficient for smoke
validation. A framework would add dependencies and complexity without
proportional benefit.

### JSON command manifest

Considered for command specification. Rejected in favor of `--cmd` flags:
- Requires temp file management
- Adds parsing complexity
- Shell flags are more ergonomic for ad-hoc runs

### `eval` for command execution

Rejected. Creates quoting ambiguity, injection risk, and non-deterministic
semantics. Unacceptable in a validation harness. Use explicit argv parsing.

### `timeout` utility for hang detection

Deferred. Not portable to macOS without Homebrew dependency. Rely on daemon
internal timeouts for now. Can revisit if hang scenarios become common.

### Binary pass/fail verdict for all commands

Rejected. Commands like `orient` and `trust` are informational surfaces
with no pass/fail semantics. Forcing them into pass/fail model would create
semantic lies in metadata.
