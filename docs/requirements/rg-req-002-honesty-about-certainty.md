<!-- requirements-assurance-v1
{
  "formatVersion": 1,
  "kind": "requirement",
  "requirementId": "RG-REQ-002",
  "sources": [
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "the-core" },
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "honesty-rules" },
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "labels-speak-the-readers-language-not-ours" },
    { "kind": "document-section", "path": "docs/slices/honesty-gate-2.md", "fragment": "5-definition-of-done" },
    { "kind": "document-section", "path": "docs/slices/coherence-3.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/headline-truth-1.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/find-facts-1.md", "fragment": "2-contract" }
  ],
  "lowLevelRequirements": [
    { "id": "RG-REQ-002-L01", "parentId": "RG-REQ-002" },
    { "id": "RG-REQ-002-L02", "parentId": "RG-REQ-002" },
    { "id": "RG-REQ-002-L03", "parentId": "RG-REQ-002" },
    { "id": "RG-REQ-002-L04", "parentId": "RG-REQ-002" },
    { "id": "RG-REQ-002-L05", "parentId": "RG-REQ-002" },
    { "id": "RG-REQ-002-L06", "parentId": "RG-REQ-002" },
    { "id": "RG-REQ-002-L07", "parentId": "RG-REQ-002" },
    { "id": "RG-REQ-002-L08", "parentId": "RG-REQ-002" },
    { "id": "RG-REQ-002-L09", "parentId": "RG-REQ-002" },
    { "id": "RG-REQ-002-L10", "parentId": "RG-REQ-002" }
  ]
}
-->
# RG-REQ-002 — Every answer is honest about what it knows, and surfaces reading one snapshot agree

Status: DRAFT authored by the in-place manager 2026-09-12. Independent requirements review: PERFORMED 2026-09-12 by Codex gpt-5.6-terra (standalone, read-only) — verdict REFINE; its findings are applied in this revision (record: reviews/2026-09-12-requirements-review-0-terra.txt). Human approval: PENDING.
Maturity: PROTOTYPE (requirements record).

## Source and intent

Sources: [VISION — The Core](../VISION.md#the-core) commitment 2 ("an unlabeled 42%-resolved call graph is worse than no call graph, because agents act on it"); [VISION — Honesty Rules](../VISION.md#honesty-rules) (unknown is never zero; coverage is part of the fact; degradation is a first-class output); [VISION — Labels speak the reader's language](../VISION.md#labels-speak-the-readers-language-not-ours); the Certainty Model's dependency rules 2–5; the ratified invariant of HONESTY-GATE-1/2 ("no row is emitted whose evidence the printed caveat excludes"), CONTRADICTION-SWEEP-1, COHERENCE-2/3, HEADLINE-TRUTH-1, FIND-FACTS-1, HONEST-DEGRADATION-1, METRIC-LANG-COVERAGE-1, ZEROSTATE-SCOPE-1; the STANDING HONESTY RULES of every packet since 2026-09-04; the audit rounds five and six (fabrication classes; coherence under composition). The human's ratified stance: the map is not the territory — directionally correct answers to high-level questions about a repo; a defect is a defect only when a user is misled or unhelped.

## High-level requirement

Every surface of `rmap` shall state what it extracted, what it inferred and what it does not know — never rendering an unmeasured value as zero, never asserting a relationship the store does not hold, never contradicting another surface about the same snapshot, and always stating coverage and degradation where the answer renders, in the reader's language — so that an agent acting on any answer is never misled by it.

**Scope:** cross-cutting; binds every other H. Specific surfaces carry their own Ls; this H owns the rules they share and the enforcement of those rules in code.

**High-level acceptance:** the audit's honesty dimension grades no surface below B on any repo class; the fabrication corpus assertions (L01) hold; the coherence seams (L02) exist for every pair of surfaces that render the same quantity.

## Low-level requirements

### RG-REQ-002-L01 — No surface asserts a relationship the store does not hold

A rendered edge or row (CALLS, IMPORTS, IMPLEMENTS, cycle arrow, resource access, framework detection, map edge, module edge) shall exist only if a stored, evidence-backed fact backs it; a binding without evidence is a fabrication even when it happens to be right; an indirect receiver never binds to the enclosing class without receiver-type evidence; a cycle arrow is drawn only over a verified edge.

**Verification criterion:** corpus assertion — zero receiver-bearing CALLS self-loops on every C++ store; `explain`'s Import-cycles block contains no `->` without a walk; `cycles/walk.rs` unordered-fallback tests; HONESTY-GATE-2 resource/framework tests; a binding-invariant unit test in the resolver (to be added).

**Evidence (v0.18.0):** NOT MET — RC-1 (call self-loops, regression), RC-4 (explain cycle arrows); MET for resources, trust framework claims, map. Queue Q1, Q4.

### RG-REQ-002-L02 — Surfaces reading one snapshot agree

Where two surfaces render the same quantity from one snapshot, they shall read one computation — or one shall state its differing basis inline — and a seam test shall make disagreement unrepresentable: trust vs modules (module connectivity), callers vs surfaces (a route at the same line), a table vs its own edge list (identifier space), printed scores vs printed order, exclusion predicates (generated/vendored/test) applied by one surface and not another, the file universe across orient/stats/check.

**Verification criterion:** the existing seams — `orient_tests.rs::cross_surface_cycle_*`, `cycle_composition.rs::headline_partition_equals_renderer_partition_seam`, `modules_list_tests.rs::modules_list_rendered_file_totals_sum_to_the_check_indexed_basis`, `presentation/mod.rs::surfaces_footer_summary_line_and_boundaries_rows_agree`; seams to add for trust↔modules fan counts, find score↔order labels, the complexity/docs/seeds exclusion predicates.

**Evidence (v0.18.0):** NOT MET — the dominant round-six class (RC-5, RC-9, D-N7, D-N9, D-N10); MET for the cycle and file-universe axes.

### RG-REQ-002-L03 — Every measured count states its universe

Each rendered file or symbol total shall name its inclusion rule in one clause (`1917 files indexed (1627 source; 290 config/contract/unreadable, tracked only)`); totals of the same basis agree across surfaces; a fourth basis that does not reconcile is recorded with its residual and escalated, never fitted to the number.

**Verification criterion:** `orient_tests.rs::header_renders_indexed_source_tracked_only_split`; `check_repo.rs::orient_and_check_state_the_same_indexed_file_total`; `modules_list_tests.rs` footer tests; `stats.rs::render_human_warns_when_grouped_exceeds_indexed`.

**Evidence (v0.18.0):** OBSERVED MET (HEADLINE-TRUTH-1).

### RG-REQ-002-L04 — Unknown is never rendered as zero, and a failed read renders unknown-with-reason — enforced in code

A value that is not measured shall render `unknown` (JSON `null`) with its reason; a fallible read on a rendering path shall never be swallowed into 0, empty or a default (`unwrap_or(0)`, `.ok()`, `unwrap_or_default`, `.ok().flatten()`); the unknown reaches JSON consumers, not only human text. The rule shall be enforced by an automated check over rendering and aggregation code, not by review alone.

**Verification criterion:** the renderer tests (`anchor_omits_zero_sentinel_line`, `coverage_read_failure_renders_unknown_with_reason_not_silent`, `render_resolution_zero_in_scope_is_unknown_not_fabricated_full`, `complexity_malformed_evidence_renders_named_unavailable_not_cx_zero`, `has_runtime_edges_missing_counts_fails_to_decode_not_defaults_to_zero`, …); a lint or source-level test forbidding the swallowing forms on `presentation/`, `aggregators/` and DTO-decoding paths (to be added — none exists).

**Evidence (v0.18.0):** NOT MET — the renderers hold individually, but the required automated enforcement does not exist; six of seven opus-4-6 builder cycles introduced a swallowing form and only review caught them.

### RG-REQ-002-L05 — Every multi-class hit carries its certainty layer; no Layer 2–4 row is tagged extracted

`find` and any surface mixing fact classes shall tag each group `extracted | inferred | hint | governance` from its source table's layer; a missing, mistagged or unknown class is malformed and withholds the verdict rather than rendering untagged; semantic seeds are labeled as ranked guesses beneath the facts.

**Verification criterion:** `find/facts_render_tests.rs` (`symbol_hit_renders_with_class_and_certainty_label`, `group_missing_certainty_is_malformed_never_untagged`, `group_mistagged_certainty_is_malformed_never_actionable`, `omitted_class_is_surfaced_as_missing_never_silently_dropped`); `daemon-runtime/tests/find_facts_class_honesty_seam.rs`.

**Evidence (v0.18.0):** OBSERVED MET for `find`; `find --text` UNKNOWN (not probed).

### RG-REQ-002-L06 — Coverage is stated where the signal renders, derived from the snapshot

A signal that covers only part of the repository (by language, resolution rate or evidence source) shall state its coverage at the render site, derived from snapshot facts — never a hardcoded language list — and the caveat disappears by itself when coverage completes; a zero-state names the build's detector or reader coverage for this repo's materially present languages, never blaming the codebase and never wearing a sentence that reads the same on every repo.

**Verification criterion:** `hotspots.rs` coverage-caveat tests; `resources.rs::list_render_empty_names_coverage_not_the_codebase`; `surfaces.rs::list_render_empty_states_detector_coverage_not_repo_blame`; `daemon-runtime/tests/measurement_coverage_surface.rs`; `deps_list.rs::maven_capability_limit_names_the_gap_and_suppresses_downgraded`.

**Evidence (v0.18.0):** OBSERVED MET for complexity/hotspots/resources/surfaces/boundaries; NOT MET for `deps` (RC-6, RC-7) and complexity scope (RC-9).

### RG-REQ-002-L07 — Degradation is first-class with a concrete, applicable next action

A weaker-than-usual basis (fallback store, missing or in-flight enrichment, stale snapshot, unparsed manifests, retention not yet run) shall be stated at the render site with a next action that exists for this language and build; a remedy with no implementation is never offered; a capability that exists but has not yet run states its standing rather than disappearing.

**Verification criterion:** `deps_list.rs::workspace_coverage_shortfall_splits_no_source_from_unparsed`; `http_boundary.rs::render_surfaces_degraded_is_unknown`; `agent/tests/orient_repo_confidence.rs`; `daemon-runtime/tests/honest_degradation_impl2.rs`; doctor's retention standing (RG-REQ-011-L10).

**Evidence (v0.18.0):** OBSERVED MET in wording; NOT MET as a deep vertical for retention observability (dormant on both roots).

### RG-REQ-002-L08 — Labels describe the reader's subject, not repo-graph's pipeline

Product surfaces shall carry no internal-diagnostic label (serving-engine posture, threshold wording, enrichment phase narration, basis codes, hex store filenames); internal diagnostics live only on explicit debug surfaces or `--json` fields documented as routing metadata. The human/JSON asymmetry where JSON keeps an internal posture the human render drops (AUDIT5-MINORS-1 F4) is a deliberate exception, recorded.

**Verification criterion:** `trust_tests.rs::render_drops_the_livegraph_posture_section`; `deps_list_secondary.rs::unknown_tag_from_a_newer_daemon_names_the_ecosystem_only`; the cli-out-1 `orient_human_hides_internal_fields` pattern; Busy messages identify the repo, not only the store file (RG-REQ-011-L02).

**Evidence (v0.18.0):** OBSERVED MET (28/28 → 0 posture lines); residual: Busy messages leak the hex store path.

### RG-REQ-002-L09 — A policy overlay never erases the measurement

A waiver, suppression or effective view shall never delete the computed fact; every read surface exposes both computed and effective state (detailed in RG-REQ-013).

**Verification criterion:** `gate/src/compute.rs::waiver_suppresses_fail_verdict`, `::pass_obligation_stays_pass_even_with_matching_waiver`; `rgr/tests/gate_command.rs::gate_waiver_suppresses_fail`.

**Evidence (v0.18.0):** OBSERVED MET.

### RG-REQ-002-L10 — Honesty holds under change: a headline may fall

When a fix removes fabricated inputs, the affected headline numbers (resolution %, dead-code count, edge counts) shall move to their honest value and the release record shall state the movement and its cause; no slice may hold a headline steady by a compensating redefinition, and no metric may be redefined to meet the code.

**Verification criterion:** the ship record of every slice touching a headline states before/after with attribution (operator practice since ECONOMY-2's metric-redefinition incident); reviewers reject a redefinition (the ≤15% cursor-bytes case is the precedent).

**Evidence (v0.18.0):** OBSERVED practised (HEADLINE-TRUTH-1, SEED-CHUNK-3 restated acceptance); binds Q1 (trust % falls, dead rises).

## Preservation obligations named by the ratifying specifications

- Manifest-parser and detector output shapes additive only; storage schema additive; `is_test` on inferences via join, not column.
- Tarjan/SCC semantics — which cycles exist; trust/stats computation — a factual disagreement is a FINDING + DECISION_REQUIRED, not a silent recompute.
- Detection itself and detector recall are separate slices from what is said.
- LiveGraph/witness and CYCLES-B certificates; `compute_trust_overlay_for_snapshot` as the one posture source.
- `(N test)` means SUBSET product-wide; no hardcoded language list in coverage logic; Maven parser out of scope — name the absence.
- Exit codes (RG-REQ-012).

## Review and approval

Independent requirements review: PERFORMED (Codex gpt-5.6-terra, REFINE; findings applied).
Human approval: PENDING.
Implementation and verification: evidence lines are the manager's OBSERVED reading; they are not acceptance.
