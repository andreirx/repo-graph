<!-- requirements-assurance-v1
{
  "formatVersion": 1,
  "kind": "requirement",
  "requirementId": "RG-REQ-012",
  "sources": [
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "protocol-surface-standard" },
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "agent-operating-model" },
    { "kind": "document-section", "path": "docs/contracts/exit-codes.md", "fragment": "non-verdict-commands" },
    { "kind": "document-section", "path": "docs/contracts/exit-codes.md", "fragment": "verdict-commands" },
    { "kind": "document-section", "path": "docs/contracts/exit-codes.md", "fragment": "stream-contract" },
    { "kind": "document-section", "path": "docs/slices/economy-2.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/cursor-roundtrip-1.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/anchors-everywhere-1.md", "fragment": "2-contract" }
  ],
  "lowLevelRequirements": [
    { "id": "RG-REQ-012-L01", "parentId": "RG-REQ-012" },
    { "id": "RG-REQ-012-L02", "parentId": "RG-REQ-012" },
    { "id": "RG-REQ-012-L03", "parentId": "RG-REQ-012" },
    { "id": "RG-REQ-012-L04", "parentId": "RG-REQ-012" },
    { "id": "RG-REQ-012-L05", "parentId": "RG-REQ-012" },
    { "id": "RG-REQ-012-L06", "parentId": "RG-REQ-012" },
    { "id": "RG-REQ-012-L07", "parentId": "RG-REQ-012" },
    { "id": "RG-REQ-012-L08", "parentId": "RG-REQ-012" },
    { "id": "RG-REQ-012-L09", "parentId": "RG-REQ-012" }
  ]
}
-->
# RG-REQ-012 — rmap is a machine-readable protocol: names, exit codes, cursors, budgets and JSON an agent can rely on

Status: DRAFT authored by the in-place manager 2026-09-12. Independent requirements review: PERFORMED 2026-09-12 by Codex gpt-5.6-terra (standalone, read-only) — verdict REFINE; its findings are applied in this revision (record: reviews/2026-09-12-requirements-review-0-terra.txt). Human approval: PENDING.
Maturity: PROTOTYPE (requirements record).

## Source and intent

Sources: [VISION — Protocol Surface Standard](../VISION.md#protocol-surface-standard) (command naming, output contracts, external workflow instructions — all required; "a command that passes technical tests but fails protocol-surface verification is not shippable"); [VISION — Agent Operating Model](../VISION.md#agent-operating-model) (CI-facing command → exit-code semantics explicit); the Change Doctrine (the agent-facing output is the product, not a frozen API — the one genuine output contract is the governance surface: verdict states + exit codes); the normative [exit-code contract](../contracts/exit-codes.md); the ratified contracts of EXIT-CODES-1, CURSOR-ROUNDTRIP-1, ECONOMY-2, HEADLINE-TRUTH-1, ANCHORS-EVERYWHERE-1, ORIENT-DENSITY-1, ORIENT-SEGMENT-2, CLI-OUT-1..7, `docs/cli/rmap-contracts.md`.

## High-level requirement

An agent shall be able to drive `rmap` without guessing: a command's name states its workflow role; its exit status has one documented meaning; a refusal is a verdict, not an error; every cursor the product prints runs as printed; every budget marker tells the truth about what was elided; and the human and JSON renderers answer the same question — while the rendered shape of discovery output stays free to improve.

**Scope:** the CLI contract layer shared by every command. Content honesty is RG-REQ-002; governance semantics are RG-REQ-013.

**High-level acceptance:** the exit-code enumeration test, the cursor round-trip tests, the budget-invariance tests and the paired human/JSON tests pass; the usefulness audit's ECONOMY dimension grades no surface below C.

## Low-level requirements

### RG-REQ-012-L01 — One documented exit-code contract, two families, named constants at every exit site

`docs/contracts/exit-codes.md` is the sole definition. Non-verdict commands: 0 success (including vacuous), 1 usage error, 2 runtime error, 3 still running, 4 refused by policy. Verdict commands (`doctor`, `gate`, `check`, `modules violations`, `hook`) keep their documented 0/1/2 with JSON `status` as the discriminator; the per-command ambiguity (a runtime error inside a verdict command also exits 2) is stated, not hidden. Every process-exit site uses a meaning-named constant from one table — never a literal, a computed value or `ExitCode::SUCCESS`. A change to any code is a DECISION_REQUIRED with versioned propagation.

**Verification criterion:** `rgr/tests/exit_code_contract.rs` (`exit_code_values_preserve_both_contract_families`, `every_process_exit_site_uses_a_named_constant`); `presentation/check.rs` exit-code tests; `gate_command.rs` mode tests.

**Evidence (v0.18.0):** OBSERVED MET (365d695; `dead` 4 on 29/29; `check --full` 0/1/2 honoured).

### RG-REQ-012-L02 — A refusal is a verdict on stdout, machine-readable in JSON

A policy refusal shall print its reason and alternatives to stdout with no `error:` prefix, exit 4, and carry `status: "refused", code: 4` under `--json`; stderr carries only usage and runtime diagnostics; `--json` never changes the process exit code.

**Verification criterion:** `rgr/tests/dead_command.rs` (`dead_command_is_disabled`, `dead_json_reports_refusal_status_and_code`, `dead_help_names_its_exit_codes`); `rgr/tests/envelope_contract.rs::dead_envelope_contract` (stderr empty).

**Evidence (v0.18.0):** OBSERVED MET.

### RG-REQ-012-L03 — Every printed cursor runs as printed

The short symbol cursor (`<path>#<qualified_name>:SYMBOL:<KIND>`) and every `rmap …` next-command the product prints shall execute from the repository root in the command it names; the cursor resolves at one syntax-gated, storage-free site shared by `explain`/`callers`/`callees`/`path`; a plain name or a canonical cursor is untouched; human output is shell-quoted, JSON carries the raw cursor; every emitted command is non-mutating (`map --dry-run`, never bare `map`).

**Verification criterion:** `daemon-runtime/src/dispatch_explain_alias.rs` tests (`short_symbol_cursor_gets_the_uid_prefix_reattached`, `plain_symbol_name_focus_is_untouched_stays_nodes_free`, `canonical_full_cursor_is_untouched`, `empty_uid_never_fabricates_a_prefix`); `find/fact_hit.rs::raw_cursor_equals_human_short_cursor_with_verb_and_quoting_removed`; `seed.rs::in_root_symbol_key_is_shortened_and_round_trips`.

**Evidence (v0.18.0):** OBSERVED MET (CURSOR-ROUNDTRIP-1, AUDIT5-MINORS-1 F3).

### RG-REQ-012-L04 — Budgets change depth and length, never facts; a marker never denies an elision

Every number `orient` prints shall be identical across `small | medium | large | --full` with monotonic length; `[budget not reached — output complete]` renders only when nothing was elided; when `--full` equals `--budget large` but rows were capped, the line names the elided count and where to get the rest; every elidable list ends `… and N more <kind> — <command>`; `--full` and `--budget` are mutually exclusive flags.

**Verification criterion:** `orient_density_tests.rs::rendered_numbers_are_budget_invariant_length_is_monotonic`; `agent/tests/orient_repo_budget_identity.rs::budgets_change_length_never_numbers`; `orient_seg2_tests.rs` (`full_identical_to_large_with_elided_groups_names_the_elision`, `saturated_line_renders_only_at_full_when_complete`, the `budget_saturated_*` family); `orient_command.rs::orient_full_with_budget_mutually_exclusive`; `map.rs::dry_run_caps_content_and_manifest_and_names_the_remainder_with_the_hint`.

**Evidence (v0.18.0):** OBSERVED MET for invariance and the marker; the budgets themselves are the weakest ECONOMY cells (`orient --full` D/D+; `surfaces list` 335 uncapped lines on glamCRM; `stats` uncapped) — carried.

### RG-REQ-012-L05 — Output economy is a measured target with its exact measure

Where a slice sets an economy target it shall name the numerator, denominator and measurement ("≤ N% of <output> bytes spent on <precisely which bytes>, measured by <how>, excluding <what>"); a redefinition of the measure to meet the code is rejected; the usefulness protocol's ECONOMY dimension is graded per command × repo each release with boilerplate and signal bytes reported separately.

**Verification criterion:** the ECONOMY-2 record (the ≤15%-of-whole-cursor-lines target; "repeated boilerplate only" rejected); `docs/testing/end-to-end-usefulness-protocol.md`; no automated byte-share assertion exists — a probe is to be built before any L cites a byte percentage as met.

**Evidence (v0.18.0):** OBSERVED practised; measured per slice, not automated.

### RG-REQ-012-L06 — Human and JSON renderers answer the same question

Agent-facing query commands shall default to human text and emit the full envelope under `--json`; the human renderer drops only JSON-only routing metadata (`backend_used`, `fallback_reason`, compare sidecars) so the text is backend-independent; a fact visible in one mode is present in the other, except the recorded deliberate exceptions (AUDIT5-MINORS-1 F4: JSON keeps the LiveGraph MEET posture; `doctor`'s snapshot verdict is absent from JSON by contract); JSON fields are additive; no terminal styling.

**Verification criterion:** `rgr/tests/cli_output_mode.rs`, `cli_out_3_drilldown.rs`, `cli_out_4_modules.rs`, `cli_out_6_quality.rs`, `cli_out_7_governance.rs` (paired human/JSON cases — they assert both modes work; a same-facts assertion per command is to be added); `docs_tests.rs::json_filtered_view_preserves_entry_fields_and_recomputes_counts_by_kind`.

**Evidence (v0.18.0):** UNKNOWN — both modes exist and are paired-tested; the required same-facts parity was not probed in round six and no test asserts it.

### RG-REQ-012-L07 — Every row that has a line opens at it; no row invents one

Symbol-level rows render `path:line` from the same store as `path`; an absent or zero line renders nothing; a grouped row carries the SET of lines (capped, `+N more`), never one picked line; summaries stay line-free; `callers`/`callees` anchor the call site (RG-REQ-005-L08).

**Verification criterion:** `presentation/mod.rs` anchor tests; `explain.rs` tier tests; `boundaries_list/tests.rs` line-set tests; `http_boundary.rs::surface_row_anchors_line_and_distinct_lines_do_not_collapse`.

**Evidence (v0.18.0):** OBSERVED MET (ANCHORS-EVERYWHERE-1, F2, F7); callers/callees anchor the declaration line — partial.

### RG-REQ-012-L08 — Zero-states carry denominators and a per-repo gap clause

An empty result shall state what was searched and over what denominator, name the build's detector or reader families, and name only gaps materially present in this repo — never a sentence identical on every repo.

**Verification criterion:** `surfaces.rs::list_render_empty_states_detector_coverage_not_repo_blame`; `resources.rs::list_render_empty_names_coverage_not_the_codebase`; `cycles/tests.rs::zero_state_states_module_and_edge_counts`; `modules_list_tests.rs::list_render_nonzero_reduction_renders_the_reconciled_footnote`; `find/text_render/tests.rs::honest_empty_is_capability_not_repo_absence`.

**Evidence (v0.18.0):** OBSERVED MET broadly (keep-and-imitate items); poco's `modules list` zero-state is faithful to a never-resolved include graph (RG-REQ-006-L03).

### RG-REQ-012-L09 — Command names imply workflow role; help and handlers agree

A command name shall state whether it is safe orientation (`orient`, `find`), validation before hand-off (`check`), focused investigation (`callers`, `trust`, `explain`), maintenance (`repo rebuild`, `maintenance gc` — destructive verbs confirm), or policy-carrying with exit codes (`gate`, `violations`); `--help` text and the handler's accepted arguments agree for every command; a new surface declares its role and, if CI-facing, its propagation.

**Verification criterion:** an automated help-vs-handler consistency check (to be added — none exists); the recorded open instance: `declare *` help shows the cwd form while handlers require `<db_path> <repo_uid>` (`docs/cli/rmap-contracts.md`).

**Evidence (v0.18.0):** NOT MET / UNKNOWN — no automated check; one concrete drift recorded.

## Preservation obligations named by the ratifying specifications

- The meaning of 0/1/2/3 for every existing command; the JSON `status` field additive; constants in one table (`rgr/src/daemon_command.rs`); the stream contract.
- The literal ≤15%-of-bytes-on-whole-cursor-lines measure (redefinition rejected); cursor grammar; seed ranking; storage.
- `modules list` degradation contract: a policy-parse failure exits 0 with `rollups_degraded: true` and `violation_count: null`.
- Output-contract invariants of CLI-OUT-7: no clipping, no arbitrary top-N, deterministic ordering with alphabetical tie-breaks, `--json` preserved; no terminal colour (CLI-OUT-1).
- Governance/CI-facing surfaces carry the highest bar and versioned propagation (Change Doctrine).

## Review and approval

Independent requirements review: PERFORMED (Codex gpt-5.6-terra, REFINE; findings applied).
Human approval: PENDING.
Implementation and verification: evidence lines are the manager's OBSERVED reading; they are not acceptance.
