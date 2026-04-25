# Quality Policy Design: Declarations and Assessments

Date: 2026-04-25
Status: Design
Scope: Policy layer between measurements and gates

## Overview

This document defines the architecture for quality policies — the judgment
layer that sits between raw measurements (Phase A) and operational surfaces
(gates, orient, check).

**Current state:**
- Measurements exist: `function_length`, `cognitive_complexity`, `cyclomatic_complexity`, etc.
- No policy layer exists. Gate methods are hardcoded evaluators, not policy-driven.
- No assessment model exists. Gates compute verdicts directly, not assessments.

**Goal:**
- Policies are declared constraints over measurements
- Assessments are derived results of applying policies to measurements
- Waivers suppress assessment verdicts without erasing computed violations
- orient/check can surface assessments informationally before gates enforce them

## 1. Conceptual Model

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│  Measurements   │────▶│     Policies     │────▶│   Assessments   │
│  (Layer 2 fact) │     │  (Declaration)   │     │  (Derived)      │
└─────────────────┘     └──────────────────┘     └─────────────────┘
                                                         │
                                                         ▼
                                                 ┌─────────────────┐
                                                 │    Waivers      │
                                                 │  (Declaration)  │
                                                 └─────────────────┘
                                                         │
                                                         ▼
                                                 ┌─────────────────┐
                                                 │ Effective Verdict│
                                                 └─────────────────┘
```

**Measurements** are facts. They do not judge.
**Policies** are declarations. They define what "acceptable" means.
**Assessments** are derived. They apply policies to measurements.
**Waivers** are declarations. They suppress verdicts without hiding violations.
**Effective Verdict** is the final state after waiver overlay.

## 2. Policy Declaration Schema

### 2.1 Policy Kinds

All four policy kinds define a threshold rule. The **operator is derived from
policy_kind**, not specified separately. This prevents contradictory declarations.

| Kind | Derived Operator | Evaluation Scope | Semantics |
|------|------------------|------------------|-----------|
| `absolute_max` | `<=` | All matching measurements | Every measurement must satisfy `value <= threshold` |
| `absolute_min` | `>=` | All matching measurements | Every measurement must satisfy `value >= threshold` |
| `no_new` | `<=` | Only newly introduced targets | New targets must satisfy `value <= threshold` |
| `no_worsened` | `<=` | Only existing violators | Existing violators (`value > threshold`) may not increase |

**Threshold is required. Operator is implicit.**

`no_new` and `no_worsened` always use max-direction semantics (`<=`). This
covers the primary use case: preventing new high-complexity code or worsening
existing complexity. If min-direction comparative policies are needed later
(e.g., no new low-coverage files), add `no_new_min` / `no_worsened_min` kinds.

Examples:
- `absolute_max`: `cognitive_complexity <= 25` on all functions
- `no_new`: `cognitive_complexity <= 25` checked only on functions added since baseline
- `no_worsened`: existing functions with `cc > 25` may not increase further

### 2.2 Policy Target Scope

Policies target a scope. The scope determines which measurements are evaluated.

**Scope clauses** are a list of filters. All clauses must match for a
measurement to be in scope. Empty list = repo-wide (all measurements).

| Clause Type | Selector | Semantics |
|-------------|----------|-----------|
| `module` | Path prefix | Measurements in files under this module path |
| `file` | Path exact or glob | Measurements in matching files |
| `symbol_kind` | Node subtype | Measurements on symbols of this kind |

Example compositions:
- `[{type: "module", selector: "src/core"}]` — all functions in src/core
- `[{type: "module", selector: "src/core"}, {type: "symbol_kind", selector: "METHOD"}]` — only methods in src/core
- `[]` — entire repository

### 2.3 Policy Declaration Fields

```typescript
interface ScopeClause {
  type: "module" | "file" | "symbol_kind";
  selector: string;
}

interface QualityPolicyDeclaration {
  // Identity
  policy_id: string;           // Human-readable ID (e.g., "QP-001")
  version: number;             // Monotonic version for supersede
  
  // Target scope (AND of all clauses; empty = repo-wide)
  scope_clauses: ScopeClause[];
  
  // Measurement binding
  measurement_kind: string;    // e.g., "cognitive_complexity"
  
  // Threshold rule
  policy_kind: "absolute_max" | "absolute_min" | "no_new" | "no_worsened";
  threshold: number;           // Required, never null
  // NOTE: operator is derived from policy_kind, not stored separately
  //   absolute_max, no_new, no_worsened → "<="
  //   absolute_min → ">="
  
  // Severity
  severity: "fail" | "advisory";  // fail = gate-blocking, advisory = informational
  
  // Metadata
  description: string | null;
  created_at: string;          // ISO 8601
  created_by: string | null;
}
```

### 2.4 Policy Identity

Policy identity is the tuple: `(repo_uid, policy_id, version)`.

The `policy_id` is a human-assigned stable identifier (like requirement `req_id`).
Version enables supersede semantics without breaking waiver references.

**UID derivation:** Deterministic UUID v5 from `(repo_uid, policy_id, version)`.

### 2.5 Measurement Kind Validation

Policies can only bind to known measurement kinds. At declaration time:
- Validate `measurement_kind` against supported set
- Reject unknown kinds with explicit error

Supported kinds (Phase A):
- `cyclomatic_complexity`
- `cognitive_complexity`
- `function_length`
- `parameter_count`
- `max_nesting_depth`
- `line_coverage` (imported)

## 3. Assessment Computation

### 3.1 Assessment Definition

An assessment is the result of evaluating one policy against one snapshot's
measurements. For comparative policies (`no_new`, `no_worsened`), the baseline
snapshot is part of the assessment identity.

```typescript
interface QualityAssessment {
  // Identity
  // For absolute policies: {snapshot_uid}-qpa-{policy_uid}
  // For comparative policies: {snapshot_uid}-qpa-{policy_uid}-vs-{baseline_snapshot_uid}
  assessment_uid: string;
  snapshot_uid: string;
  policy_uid: string;
  baseline_snapshot_uid: string | null;  // Part of identity for comparative
  
  // Result
  computed_verdict: "PASS" | "FAIL" | "NOT_APPLICABLE" | "NOT_COMPARABLE";
  
  // Evidence
  measurements_evaluated: number;
  violations: AssessmentViolation[];  // Empty if PASS
  
  // Comparative-specific counts
  new_violations: number | null;       // For no_new
  worsened_violations: number | null;  // For no_worsened
}

interface AssessmentViolation {
  target_stable_key: string;
  measurement_value: number;
  threshold: number;             // Always present (all policies have thresholds)
  baseline_value: number | null; // Present for no_worsened violations
}
```

**Assessment identity rule:**
- `absolute_max` / `absolute_min`: identity is `(snapshot_uid, policy_uid)`
- `no_new` / `no_worsened`: identity is `(snapshot_uid, policy_uid, baseline_snapshot_uid)`

This ensures different baseline comparisons produce distinct persisted assessments.

### 3.2 Verdict States

| Verdict | Meaning |
|---------|---------|
| `PASS` | All evaluated measurements satisfy threshold |
| `FAIL` | One or more measurements violate threshold |
| `NOT_APPLICABLE` | No measurements match policy scope |
| `NOT_COMPARABLE` | Baseline required but unavailable or incompatible |

### 3.3 Evaluation Rules

**absolute_max:**
1. Query measurements matching policy scope
2. For each measurement, check `value <= threshold`
3. Collect violations (measurements where check fails)
4. Verdict = PASS if zero violations, FAIL otherwise
5. If zero measurements match scope → NOT_APPLICABLE

**absolute_min:**
1. Query measurements matching policy scope
2. For each measurement, check `value >= threshold`
3. Collect violations
4. Verdict = PASS if zero violations, FAIL otherwise
5. If zero measurements match scope → NOT_APPLICABLE

**no_new:**
1. Require explicit baseline snapshot
2. Query measurements in both snapshots matching policy scope
3. Identify targets in current but not in baseline (new targets)
4. For each new target, check against threshold rule
5. Collect violations from new targets only
6. Existing targets are NOT evaluated (they may violate threshold without failing)
7. Verdict = PASS if zero new-target violations, FAIL otherwise
8. If no baseline provided → NOT_COMPARABLE

**no_worsened:**
1. Require explicit baseline snapshot
2. Query measurements in both snapshots matching policy scope
3. Identify targets that violate threshold in baseline (existing violators)
4. For each existing violator, compare current value to baseline value
5. If current value is worse (further from threshold), collect violation
6. New targets and non-violating existing targets are NOT evaluated
7. Verdict = PASS if no existing violator worsened, FAIL otherwise
8. If no baseline provided → NOT_COMPARABLE

### 3.4 Missing Measurements

A symbol with no measurement for the bound kind is NOT a violation.
It is simply excluded from evaluation.

Rationale: Missing measurement means "not computed" (unsupported language,
extraction failure). Absence is not evidence of violation.

### 3.5 Unsupported Languages

Phase A measurements are TS/JS only. Java/Rust/Python/C functions have no
`function_length` or `cognitive_complexity` measurements.

Policy evaluation:
- Only TS/JS symbols will have measurements
- Other languages produce zero violations (not evaluated)
- `measurements_evaluated` count reflects actual coverage

This is explicit degradation, not a bug. The policy applies to what exists.
Cross-language rollout expands coverage without changing policy semantics.

## 4. Waiver Compatibility

Existing waiver model (from requirement/obligation):
- Waiver targets `(req_id, requirement_version, obligation_id)`
- Waiver suppresses effective verdict without erasing computed verdict

Quality policy waivers:
- Waiver targets `(policy_id, policy_version, target_stable_key)`
- Per-symbol granularity: waive specific violations, not entire policy
- Waiver suppresses that symbol's contribution to verdict

### 4.1 Waiver Declaration Fields (extension)

```typescript
interface QualityPolicyWaiver {
  // Identity (extends standard waiver)
  policy_id: string;
  policy_version: number;
  target_stable_key: string;   // The violating symbol
  
  // Standard waiver fields
  reason: string;
  expires_at: string | null;
  created_by: string | null;
  rationale_category: string | null;
  policy_basis: string | null;
}
```

### 4.2 Effective Verdict

After waiver overlay:
1. Compute assessment with all violations
2. Filter violations against active waivers
3. If remaining violations = 0 → effective verdict = PASS (waived)
4. If remaining violations > 0 → effective verdict = FAIL
5. Report both computed_verdict and effective_verdict separately

## 5. Persistence

### 5.1 Tables

**quality_policies** (declarations table with kind='quality_policy'):
- Reuse existing declarations table schema
- `value_json` contains policy-specific fields

**quality_assessments** (new table):
```sql
CREATE TABLE quality_assessments (
  assessment_uid       TEXT PRIMARY KEY,
  snapshot_uid         TEXT NOT NULL REFERENCES snapshots(snapshot_uid),
  policy_uid           TEXT NOT NULL,  -- References declarations
  computed_verdict     TEXT NOT NULL,  -- PASS|FAIL|NOT_APPLICABLE|NOT_COMPARABLE
  measurements_evaluated INTEGER NOT NULL,
  violations_json      TEXT NOT NULL,  -- JSON array of AssessmentViolation
  baseline_snapshot_uid TEXT,
  new_violations       INTEGER,
  worsened_violations  INTEGER,
  created_at           TEXT NOT NULL
);
```

**quality_policy_waivers** (declarations table with kind='quality_policy_waiver'):
- Reuse existing declarations table
- `value_json` includes `target_stable_key` for per-symbol targeting

### 5.2 Assessment Identity

Assessment UID is deterministic, with baseline included for comparative policies:

**Absolute policies (`absolute_max`, `absolute_min`):**
```
assessment_uid = {snapshot_uid}-qpa-{policy_uid}
```

**Comparative policies (`no_new`, `no_worsened`):**
```
assessment_uid = {snapshot_uid}-qpa-{policy_uid}-vs-{baseline_snapshot_uid}
```

This ensures:
- Same policy + same snapshot = same assessment UID (absolute)
- Same policy + same snapshot + same baseline = same assessment UID (comparative)
- Different baselines = different assessments (no overwrite)
- Re-evaluation produces same UID (idempotent)

## 6. CLI Surface

### 6.1 Policy Declaration Commands

```
rmap declare quality-policy <db_path> <repo_uid> <policy_id>
    --version <n>
    --measurement <kind>
    --policy-kind <absolute_max|absolute_min|no_new|no_worsened>
    --threshold <n>                           # Required
    [--scope-clause <type>:<selector>]...     # Repeatable; omit for repo-wide
    [--severity <fail|advisory>]              # Default: fail
    [--description <text>]
```

**Operator is derived from policy_kind** (no `--operator` flag):
- `absolute_max`, `no_new`, `no_worsened` → `value <= threshold`
- `absolute_min` → `value >= threshold`

**Scope clause syntax:** `--scope-clause module:src/core --scope-clause symbol_kind:METHOD`

Multiple clauses are ANDed. Omit all `--scope-clause` flags for repo-wide policy.

**Examples:**
```bash
# Repo-wide cognitive complexity limit (value <= 25)
rmap declare quality-policy db repo QP-001 \
  --version 1 \
  --measurement cognitive_complexity \
  --policy-kind absolute_max \
  --threshold 25

# No new violations in src/core methods only
rmap declare quality-policy db repo QP-002 \
  --version 1 \
  --measurement cognitive_complexity \
  --policy-kind no_new \
  --threshold 25 \
  --scope-clause module:src/core \
  --scope-clause symbol_kind:METHOD
```

### 6.2 Assessment Commands

```
rmap assess <db_path> <repo_uid>
    [--policy <policy_id>]    # Filter to specific policy
    [--baseline <snapshot>]   # Required for no_new/no_worsened policies
```

Output: JSON array of assessments with verdicts and violations.

For comparative policies without `--baseline`, returns `NOT_COMPARABLE`.

### 6.3 Waiver Commands

```
rmap declare quality-waiver <db_path> <repo_uid>
    --policy-id <id>
    --policy-version <n>
    --target <stable_key>
    --reason <text>
    [--expires-at <iso>]
    [--created-by <author>]
```

Per-symbol waivers only. No policy-level bulk waivers in v1.

## 7. Read Surfaces (orient/check)

### 7.1 Informational Integration

**orient** budget allocations (existing):
- Add `quality_assessments` section when policies exist
- Show top N violations by severity
- Show waiver coverage
- Informational only — no pass/fail in orient

**check** output (existing):
- Add `quality_summary` section
- Count of policies evaluated
- Count of violations (pre-waiver)
- Count of waived violations
- Effective status: "clean" / "N violations" / "N waived"
- Informational only — check does not exit non-zero on quality

### 7.2 Gate Integration (deferred)

Gate integration is NOT part of this design slice.

Future gate integration:
- `rmap gate` evaluates quality policies alongside requirement obligations
- Quality policy FAIL contributes to gate exit code
- Severity determines impact: `fail` → exit 1, `advisory` → exit 0
- This requires policy + assessment implementation first

## 8. Validation Plan

### 8.1 Unit Tests

**Policy declaration:**
- Valid policy persists
- Invalid measurement kind rejected
- Invalid policy_kind rejected
- Missing threshold rejected
- Invalid scope clause type rejected
- Scope clause selector validation (non-empty)

**Assessment computation:**
- absolute_max: single violation, multiple violations, no violations, no measurements
- absolute_min: same matrix
- no_new: new target violates threshold, new target passes, existing target (ignored), no baseline
- no_worsened: existing violator worsens, existing violator improves (ignored), non-violator (ignored), no baseline
- Scope filtering: single module clause, single symbol_kind clause, composed clauses (AND)
- Comparative identity: same policy + different baselines = different assessment UIDs

**Waiver application:**
- Waiver suppresses specific violation
- Expired waiver not applied
- Non-matching waiver not applied

### 8.2 Integration Tests

**Fixture:**
- 3 functions with varying cognitive complexity (5, 15, 30)
- Policy: cognitive_complexity <= 20
- Expected: 1 violation (cc=30)
- Waiver for cc=30 function
- Expected after waiver: effective PASS

### 8.3 Real-Repo Validation

Run on repo-graph:
- Declare policy: cognitive_complexity <= 50
- Run assess
- Verify violations match `rmap metrics --kind cognitive_complexity --sort value`
- Document threshold selection rationale

## 9. Technical Debt

| Item | Description | Severity |
|------|-------------|----------|
| No NPath yet | Deferred until policy design proves need | P3 |
| TS/JS only | Cross-language measurements not computed | P2 |
| No gate integration | Policies informational only | P2 |
| No baseline auto-selection | Must specify baseline explicitly | P3 |
| No policy inheritance | Can't declare repo-wide defaults with module overrides | P3 |

## 10. Non-Goals (explicit exclusions)

- NPath complexity measurement
- Gate enforcement (separate slice)
- Cross-language measurement rollout
- Policy inheritance/cascade
- Automatic baseline selection (git parent)
- Trend analysis across multiple snapshots

## 11. Resolved Design Decisions

These were open questions resolved during design review.

### D1: Policies live in declarations table

**Decision:** Reuse declarations table with `kind='quality_policy'`.

Rationale: Quality policies are declarations. Consistent with boundary/requirement/waiver
patterns. Existing CRUD, deactivation, supersede infrastructure applies.

### D2: Assessments are persisted

**Decision:** Persist assessments, not compute at query time.

Rationale: Auditable, stable across queries, supports historical comparison.
Re-computation on policy supersede is explicit (new policy version = new assessment).

### D3: Baseline selection is explicit only

**Decision:** Caller must provide `--baseline <snapshot_uid>` for comparative policies.

Rationale: Explicit is always correct. Auto-selection (git parent) is a convenience
that can come later. Initial implementation returns `NOT_COMPARABLE` when baseline
is required but not provided.

### D4: Baseline is part of assessment identity for comparative policies

**Decision:** Assessment UID includes baseline for `no_new` and `no_worsened`.

Rationale: Different baseline comparisons are logically distinct assessments.
Without baseline in identity, comparing against two different baselines would
overwrite each other in storage.

### D5: Operator is derived from policy_kind

**Decision:** `threshold` is required. `operator` is derived from `policy_kind`,
not specified separately.

| policy_kind | Derived operator |
|-------------|------------------|
| `absolute_max` | `<=` |
| `absolute_min` | `>=` |
| `no_new` | `<=` |
| `no_worsened` | `<=` |

Rationale: Prevents contradictory declarations (e.g., `absolute_max` with `>`).
Comparative kinds use max-direction only in v1. Min-direction comparative policies
(`no_new_min`, `no_worsened_min`) can be added later if needed.

### D6: Scope uses clause-based composition

**Decision:** `scope_clauses: [{type, selector}, ...]` instead of single scope.

Rationale: The design requires composition (e.g., module + symbol_kind). Single
scope_type/scope_selector cannot express this. Clause-based model supports all
intended use cases and future growth. Empty clause list = repo-wide.

### D7: Per-symbol waivers only

**Decision:** v1 supports only per-symbol waivers, not policy-level bulk waivers.

Rationale: Per-symbol waivers preserve diagnostic fidelity and force explicit
debt acknowledgment. Policy-level waivers are operationally convenient but
dangerously broad and erase too much signal. If bulk suppression is needed
later, add a separate scoped waiver type deliberately.

## 12. Implementation Sequence

1. **Schema extension** — Add `kind='quality_policy'` and `kind='quality_policy_waiver'` to declarations value_json contracts. Add `quality_assessments` table (migration 013 or later).

2. **Policy declaration command** — `rmap declare quality-policy`. Storage + CLI.

3. **Assessment computation** — Pure function: `(policies, measurements) → assessments`. No persistence yet.

4. **Assessment persistence** — Store computed assessments.

5. **`rmap assess` command** — Query and display assessments.

6. **Waiver declaration command** — `rmap declare quality-waiver`.

7. **Waiver overlay** — Effective verdict computation.

8. **orient/check integration** — Informational surfaces.

9. **Gate integration** — Separate slice, after validation.
