# Rust CLI (`rmap`) Contract

The Rust CLI is the primary binary for agent-facing commands. This document
specifies intentional contract differences from the original TS CLI (`rgr`).

These are **design decisions**, not gaps or debt.

## Invocation Pattern

All `rmap` commands use `<db_path> <repo_uid>` positional arguments:

```
rmap <command> <db_path> <repo_uid> [options]
```

This differs from `rgr` which uses a repo registry (`rgr <command> <repo_name>`).
The `<db_path> <repo_uid>` pattern keeps `rmap` consistent and self-contained
until a registry slice ships.

## Command-Specific Contracts

### `gate` — Waiver Overlay

PASS obligations are **not waivable**.

- TS (`rgr`): unconditionally marks WAIVED on any waiver match
- Rust (`rmap`): only suppresses non-PASS verdicts

Rationale: allowing PASS-waiver conflated "no evidence" with "suppressed."
A PASS verdict means the obligation is satisfied; there is nothing to waive.
(Rust-25 deliberate correction.)

### `gate` — Quality-Policy Assessment Integration

Gate consumes pre-computed quality-policy assessments and reduces them into
the gate outcome. Quality assessments are reported separately from obligations
because they have different verdict semantics.

**Report shape:**

```json
{
  "obligations": [ /* requirement-based evaluations */ ],
  "quality_assessments": [
    {
      "policy_id": "QP-001",
      "policy_version": 1,
      "policy_kind": "no_new",
      "severity": "fail",
      "assessment_state": "present",
      "computed_verdict": "PASS",
      "is_comparative": true,
      "violations_count": 0
    }
  ],
  "outcome": {
    "outcome": "pass",
    "exit_code": 0,
    "mode": "default",
    "counts": { /* obligation counts */ },
    "quality_counts": {
      "total": 1,
      "pass": 1,
      "fail": 0,
      "advisory_fail": 0,
      "missing": 0,
      "not_comparable": 0,
      "not_applicable": 0
    }
  }
}
```

**Verdict semantics:**

| Assessment State | Verdict | Severity | Exit Code |
|-----------------|---------|----------|-----------|
| Missing | N/A | Any | 2 (incomplete) |
| Present | NOT_COMPARABLE | Any | 2 (incomplete) |
| Present | FAIL | `fail` | 1 (fail) |
| Present | FAIL | `advisory` | 0 (reported only) |
| Present | PASS | Any | 0 (pass) |
| Present | NOT_APPLICABLE | Any | 0 (pass) |

**Key semantics:**

1. **Missing assessment = incomplete.** Active quality-policy without computed
   assessment is treated as missing required evidence.

2. **NOT_COMPARABLE = incomplete.** Comparative policies (`no_new`,
   `no_worsened`) without a baseline snapshot return NOT_COMPARABLE. This
   blocks gate until a baseline is established.

3. **Severity determines blocking.** `severity: fail` assessments with FAIL
   verdict block gate. `severity: advisory` assessments are reported but do
   not affect exit code.

4. **No waiver overlay.** Quality-policy waivers are explicitly deferred.
   Quality assessments do not participate in the waiver system.

**Mode interaction:**

- `default`: Missing/NOT_COMPARABLE = exit 2; FAIL(blocking) = exit 1
- `strict`: Missing/NOT_COMPARABLE/FAIL(blocking) = exit 1
- `advisory`: Missing/NOT_COMPARABLE ignored; FAIL(blocking) = exit 1

### `violations` — Output Shape

`results` is an object with explicit sections:

```json
{
  "results": {
    "declared_boundary_violations": [...],
    "discovered_module_violations": [...]
  },
  "stale_declarations": [...],
  "declared_count": N,
  "discovered_count": N
}
```

TS uses a flat array. The object shape is more explicit for agent consumption.

### `modules list` — Degradation Contract

Envelope includes `rollups_degraded` (boolean) and `warnings` (string array).

When policy parsing fails:
- Exit 0 (not exit 2)
- `rollups_degraded: true`
- Warning message in `warnings[]`
- `violation_count: null` on each module
- Other rollup fields remain populated

Deliberate: orientation surfaces must survive policy corruption.

### `modules show` — Briefing Shape

This is a briefing, not a list. Envelope differs from QueryResult:

```json
{
  "module": { /* identity */ },
  "rollups": { /* counts */ },
  "outbound_dependencies": [ /* weighted */ ],
  "inbound_dependencies": [ /* weighted */ ],
  "violations": [ /* source-side only */ ]
}
```

No `results`/`count` wrapper.

### `modules violations` — Canonical Command

TS CLI does not have an equivalent command (`rgr arch violations` uses a
different selector domain).

Output includes `diagnostics` object reporting derivation counts so callers
can detect degraded graphs where ownership gaps suppress violations.

### Measurement Commands (`churn`, `hotspots`, `risk`)

Query-time computation, not persistence-first.

- Git is the authoritative history source
- `repo-graph-git` crate wraps git CLI
- TS implementation is reference, not spec
- Explicit anchoring for gate integration is future opt-in, not automatic

### `dead` — Per-Result Dead-Confidence

Every dead-code result carries explicit confidence tiering:

```json
{
  "results": [
    {
      "stable_key": "r1:src/utils.ts:SYMBOL:helper",
      "symbol": "helper",
      "kind": "SYMBOL",
      "file": "src/utils.ts",
      "is_test": false,
      "trust": {
        "dead_confidence": "HIGH",
        "reasons": []
      }
    },
    {
      "stable_key": "r1:src/server.ts:SYMBOL:unused",
      "symbol": "unused",
      "kind": "SYMBOL",
      "file": "src/server.ts",
      "is_test": false,
      "trust": {
        "dead_confidence": "LOW",
        "reasons": ["framework_opaque", "registry_pattern_suspicion"]
      }
    }
  ],
  "trust": {
    "summary_scope": "repo_snapshot",
    "graph_basis": "CALLS+IMPORTS",
    "reliability": { ... }
  }
}
```

**Two separate layers:**

1. **Top-level trust summary** — repo/snapshot-level context (reliability axes,
   degradation flags, caveats). Always present when trust can be assembled.

2. **Per-result `trust.dead_confidence`** — candidate-specific confidence tier.
   Every result carries this; no Option A hiding for dead-code because it is
   destructive-action-adjacent.

**Confidence tiers (v1):**

- `HIGH` — structurally dead with no significant counter-signal
- `MEDIUM` — orphaned but repo has some unresolved pressure
- `LOW` — known opacity pattern or strong liveness uncertainty

**Stable reason vocabulary:**

- `framework_opaque` — framework-heavy suspicion triggered
- `registry_pattern_suspicion` — registry/plugin/DI patterns detected
- `missing_entrypoint_declarations` — no entrypoint declarations declared
- `unresolved_call_pressure` — call-graph reliability degraded
- `unresolved_import_pressure` — import-graph reliability degraded
- `trust_unavailable` — trust assembly failed; confidence cannot be assessed

**Important:** This is evidence-tiering under current repo trust, NOT proof
of symbol-local liveness. LOW confidence means "don't trust this deadness
claim strongly" due to repo-level opacity, not "we traced this symbol and
found it's alive."

## JSON-Only Output

`rmap` always produces JSON on stdout. There is no `--json` flag because
JSON is the default and only format.

Human-readable table format is not planned. Agents are the primary consumer.
