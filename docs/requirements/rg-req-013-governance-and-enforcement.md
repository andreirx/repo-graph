<!-- requirements-assurance-v1
{
  "formatVersion": 1,
  "kind": "requirement",
  "requirementId": "RG-REQ-013",
  "sources": [
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "discovery-over-enforcement" },
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "agent-operating-model" },
    { "kind": "document-section", "path": "docs/contracts/exit-codes.md", "fragment": "verdict-commands" },
    { "kind": "document-section", "path": "docs/slices/dead-causes-1.md", "fragment": "2-contract" }
  ],
  "lowLevelRequirements": [
    { "id": "RG-REQ-013-L01", "parentId": "RG-REQ-013" },
    { "id": "RG-REQ-013-L02", "parentId": "RG-REQ-013" },
    { "id": "RG-REQ-013-L03", "parentId": "RG-REQ-013" },
    { "id": "RG-REQ-013-L04", "parentId": "RG-REQ-013" },
    { "id": "RG-REQ-013-L05", "parentId": "RG-REQ-013" },
    { "id": "RG-REQ-013-L06", "parentId": "RG-REQ-013" },
    { "id": "RG-REQ-013-L07", "parentId": "RG-REQ-013" }
  ]
}
-->
# RG-REQ-013 — Governance is frozen: maintained, never extended, and never erases a measurement

Status: DRAFT authored by the in-place manager 2026-09-12. Independent requirements review: PERFORMED 2026-09-12 by Codex gpt-5.6-terra (standalone, read-only) — verdict REFINE; its findings are applied in this revision (record: reviews/2026-09-12-requirements-review-0-terra.txt). Human approval: PENDING.
Maturity: PROTOTYPE (requirements record).

## Source and intent

Sources: [VISION — Discovery Over Enforcement](../VISION.md#discovery-over-enforcement) ("the enforcement machinery exists and works; it is frozen: maintained, not extended; no new governance surface ships without a ratified promotion from FUTURE-ITERATIONS"); [VISION — Agent Operating Model](../VISION.md#agent-operating-model) (preserve computed truth under overlays; never compare non-comparable snapshots; never erase superseded records); the Certainty Model rule 4; `docs/architecture/gate-contract.txt` (normative) and `measurement-model.txt`; the [exit-code contract](../contracts/exit-codes.md); GOV-ARMED-1; DEAD-CAUSES-1; the 2026-04-27 `dead` disable decision (not re-litigated).

## High-level requirement

The governance surface (`gate`, `assess`, `violations`, `modules violations`, `declare *`, `policy`, `coverage`, and the refused `dead`) shall keep its frozen verdict states and exit semantics, expose computed and effective verdicts on every read, report NOT_COMPARABLE rather than a fabricated delta, preserve superseded records, state unarmed as a configuration fact, and refuse `dead` by policy with snapshot-derived causes — and no new governance surface shall ship without a ratified promotion.

**Scope:** Layer 4 only. Discovery surfaces are not governance and remain free to improve their shape.

**High-level acceptance:** the gate/declare/waiver test suites pass; the armed-positive path is exercised on at least one corpus repo (L06) before this H is reported satisfied.

## Low-level requirements

### RG-REQ-013-L01 — The governance surface set is closed

No new governance command, verdict state, waiver kind or gate mode shall ship without a ratified promotion recorded in VISION and ROADMAP; an allowed change shape is a human render or additive JSON with exit codes untouched (GOV-ARMED-1 is the precedent).

**Verification criterion:** `daemon-runtime/tests/consolidation_witness.rs` (declared dispatch arms vs implemented); review against this L for every packet touching a governance command (no test can prove the promotion rule).

**Evidence (v0.18.0):** OBSERVED MET (no new governance verb since GOV-ARMED-1).

### RG-REQ-013-L02 — Computed and effective verdicts are both queryable on every read surface

`gate`, `assess`, obligations and evidence reads shall expose `computed_verdict` and `effective_verdict`; WAIVED is only ever an effective state; a waiver suppresses the gate failure and never deletes the measurement; expired or deactivated waivers stay queryable.

**Verification criterion:** `rgr/tests/gate_command.rs` (`gate_waiver_suppresses_fail`, `gate_pass_with_waiver_stays_pass`, `gate_waiver_wrong_obligation_id_no_suppression`, `gate_expired_waiver_no_suppression`, `gate_exact_json_contract`); `gate/src/compute.rs` waiver tests; `storage/tests/gate_impl.rs`.

**Evidence (v0.18.0):** OBSERVED MET.

### RG-REQ-013-L03 — Gate reduction is a pure function with fixed exit semantics

`reduce_outcome` shall have no storage access or side effects; default mode: 0 all PASS/WAIVED, 1 any FAIL ("judged and failed"), 2 any MISSING_EVIDENCE/UNSUPPORTED ("could not reach a verdict"); strict collapses 2 into 1; advisory treats both as informational; empty input is a vacuous pass; the process exit equals `gate.exit_code`.

**Verification criterion:** `gate/src/compute.rs` (`strict_mode_promotes_quality_incomplete_to_fail`, `advisory_mode_ignores_quality_incomplete`, `quality_fail_takes_precedence_over_obligation_missing`, `unsupported_method_produces_unsupported_verdict`); `gate_command.rs` (`gate_default_mode_unsupported_is_incomplete`, `gate_strict_mode_unsupported_is_fail`, `gate_advisory_mode_unsupported_is_pass`).

**Evidence (v0.18.0):** OBSERVED MET.

### RG-REQ-013-L04 — NOT_COMPARABLE over fake numbers

A comparative policy without a usable baseline, or a comparison across snapshots with incompatible toolchain provenance, shall return NOT_COMPARABLE (gate incomplete, exit 2) naming the remediation — never a fabricated delta; retention shall never degrade a valid baseline into NOT_COMPARABLE.

**Verification criterion:** `gate/src/compute.rs::quality_assessment_not_comparable_causes_incomplete_exit_2`; `daemon-runtime/tests/snapshot_retention.rs` (`comparative_assess_works_against_a_narrowed_stamp_baseline`, no NOT_COMPARABLE after retention); a CLI-level rendered NOT_COMPARABLE test (to be added — verified below the CLI only).

**Evidence (v0.18.0):** OBSERVED MET below the CLI; UNKNOWN in the field (no baseline-bearing corpus repo).

### RG-REQ-013-L05 — Supersession creates rows; nothing is erased

Superseding a declaration shall insert the new row and mark the old inactive atomically; active queries see the new and not the old; the old row and waivers bound to the prior version remain queryable; inheritance reads only the immediate prior version (no reversion resurrection); the semantic-match tuple `(method, target, threshold, operator)` is frozen.

**Verification criterion:** `storage/src/crud/declarations.rs` supersede tests (success, active-queries-see-new, atomic-on-insert-failure, old-missing/inactive errors); `rgr/tests/declare_supersede_{boundary,requirement,waiver}_command.rs`.

**Evidence (v0.18.0):** OBSERVED MET.

### RG-REQ-013-L06 — Unarmed is a configuration fact, never inferred from zero counts

Each governance command shall render an unarmed repository as one honest line plus the real arming path, derived from configuration presence (`armed`), not from `total == 0`; an armed repository with zero findings renders the evaluated counts; exit codes are unchanged (vacuous pass stays 0). The armed-positive path shall be exercised on at least one corpus repository with a real declaration and a firing violation.

**Verification criterion:** `gate/src/compute.rs` armed tests; `presentation/{gate,assess,modules_violations}.rs::*_armed_unknown`; an ARMED-POSITIVE firing-boundary smoke (to be added — ROADMAP carries it as open; every round-six corpus repo is unarmed, hence HIT F for gate/assess/violations).

**Evidence (v0.18.0):** UNKNOWN — the unarmed path holds; the required armed-positive corpus exercise has never run (the largest verification hole in this H).

### RG-REQ-013-L07 — `dead` is refused by policy with a stated reason, quantified cause and alternatives

`dead` shall decline with its false-positive-rate quantification, per-snapshot causes (framework-liveness inferences per family partitioned for test fixtures, coverage and entrypoint facts), four alternatives and the re-enable condition, on stdout at exit 4; the substrate (`find_dead_nodes`, `assess_dead_confidence`) stays intact with its pinning tests; `orient` emits no dead-code claim while `dead` is refused.

**Verification criterion:** `rgr/tests/dead_command.rs` (disabled, JSON refusal, help names codes; substrate pins `dead_exact_results`, `dead_confidence_reasons_are_stable_vocabulary`); `agent/tests/orient_repo_dead_code_reliability.rs`.

**Evidence (v0.18.0):** OBSERVED MET (HONESTY A+); at risk via RC-1 — fabricated fan-in shields 155 leveldb symbols from the substrate; fixing Q1 raises reported deadness, correctly.

## Preservation obligations named by the ratifying specifications

- `SEMANTIC_MATCH_FIELDS = (method, target, threshold, operator)` — change requires a coordinated migration re-assigning obligation ids.
- WAIVED never computed; both verdicts on every read surface — non-negotiable (gate-contract §2); the reducer pure (§3); no reversion resurrection (§5.3); `created_at DESC` tie-break among active waivers.
- Default-mode gate behaviour constant; new behaviour behind explicit flags; breaking changes versioned and announced (§8).
- Quality-policy waivers explicitly deferred (assessments do not participate in the waiver system).
- Governance exit-code semantics and gate JSON additive-only; unarmed never inferred from `total == 0`.
- Verdict-calculation logic, domain verdict terms and legacy commands are not changed, softened or migrated (CLI-OUT-7 non-goals).
- Promotion path only: operator ratification → VISION amendment → roadmap entry.

## Review and approval

Independent requirements review: PERFORMED (Codex gpt-5.6-terra, REFINE; findings applied).
Human approval: PENDING.
Implementation and verification: evidence lines are the manager's OBSERVED reading; they are not acceptance.
