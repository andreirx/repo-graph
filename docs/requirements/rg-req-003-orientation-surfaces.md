<!-- requirements-assurance-v1
{
  "formatVersion": 1,
  "kind": "requirement",
  "requirementId": "RG-REQ-003",
  "sources": [
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "primary-use-case" },
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "orientation-not-oracle" },
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "the-core" },
    { "kind": "document-section", "path": "docs/slices/headline-truth-1.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/coherence-3.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/modules-method-1.md", "fragment": "2-contract" }
  ],
  "lowLevelRequirements": [
    { "id": "RG-REQ-003-L01", "parentId": "RG-REQ-003" },
    { "id": "RG-REQ-003-L02", "parentId": "RG-REQ-003" },
    { "id": "RG-REQ-003-L03", "parentId": "RG-REQ-003" },
    { "id": "RG-REQ-003-L04", "parentId": "RG-REQ-003" },
    { "id": "RG-REQ-003-L05", "parentId": "RG-REQ-003" },
    { "id": "RG-REQ-003-L06", "parentId": "RG-REQ-003" },
    { "id": "RG-REQ-003-L07", "parentId": "RG-REQ-003" },
    { "id": "RG-REQ-003-L08", "parentId": "RG-REQ-003" },
    { "id": "RG-REQ-003-L09", "parentId": "RG-REQ-003" },
    { "id": "RG-REQ-003-L10", "parentId": "RG-REQ-003" }
  ]
}
-->
# RG-REQ-003 — The first sixty seconds: orient, check and explain point the agent at the right places

Status: DRAFT authored by the in-place manager 2026-09-12. Independent requirements review: PERFORMED 2026-09-12 by Codex gpt-5.6-terra (standalone, read-only) — verdict REFINE; its findings are applied in this revision (record: reviews/2026-09-12-requirements-review-0-terra.txt). Human approval: PENDING.
Maturity: PROTOTYPE (requirements record).

## Source and intent

Sources: [VISION — Primary Use Case](../VISION.md#primary-use-case) (what modules exist and what they own; where the boundaries are; how modules relate; what runtime/build environment each runs under — the discovery-first loop `orient` → docs → change → `check`); [VISION — Orientation, Not Oracle](../VISION.md#orientation-not-oracle); [VISION — The Core](../VISION.md#the-core) (the acid test: does it improve the agent's first sixty seconds); the governing clause of MODULE-MODEL-1 D5 and the Change Doctrine — the agent-facing output is useful information, the product, not a frozen API; the ratified contracts of ORIENT-DENSITY-1, ORIENT-SEGMENT-2, ORIENT-FACT-COHERENCE-1, ORIENT-CYCLES-DISAGREE-1, COHERENCE-2/3, HEADLINE-TRUTH-1, CHECK-SIGNAL-1, MODULES-METHOD-1, ECONOMY-2. The v0.18.0 root cause RC-4 and RC-9.

## High-level requirement

An agent running `rmap orient` on an unfamiliar repository shall receive, within one budgeted answer, the repository's modules with the method that found them, its boundaries and surfaces, its module relationships and cycles, its runtime/build environment, its documentation entry points and its current quality signals — each number reconciling with `stats`, `check` and the drill-down commands on the same snapshot — and `rmap check` shall tell the agent, relative to this build's ceiling, whether the structural state it is leaving behind is sound.

**Scope:** `orient`, `check`, `stats`, `explain <file|path>`'s orientation sections. Modules and cycles content is RG-REQ-004; docs inventory RG-REQ-008; quality signals RG-REQ-009; budgets and markers RG-REQ-012-L04.

**High-level acceptance:** all L entries hold on the smoke corpus; the audit's HIT dimension for `orient small/medium/large` grades B or better on every repo class.

## Low-level requirements

### RG-REQ-003-L01 — `explain` draws no cycle arrow without a verified walk

The Import-cycles block of `explain` shall render `->` only between members a stored walk verifies; with no walk it renders `Cycle N (K modules): members (unordered): …` with zero arrows — the same rule `cycles` and `orient` already apply.

**Verification criterion:** a `render_cycles` test with `walk: null` asserting no arrow and with a valid walk asserting the ring (to be added — none exists); field: vcmi `explain CGHeroInstance` and django `explain BaseHandler.get_response` contain no `->` in the block; `rmap cycles` on repo-graph byte-identical.

**Evidence (v0.18.0):** NOT MET — RC-4 (never worked; the honest form was built twice for `cycles` and `orient` and never reached `explain_sections.rs`). Queue Q4.

### RG-REQ-003-L02 — The complexity headline names the symbol and ranks the repository's own code

The headline form is `<file> — <symbol> (cx N)`, top-N symbols without file dedup, `+N more above threshold — rmap hotspots`; generated, vendored and test symbols are excluded and counted (RG-REQ-009-L01).

**Verification criterion:** `orient_density_tests.rs::complexity_headline_names_symbols_not_file_rollup`; `orient_tests.rs::small_complexity_centers_are_named`; the exclusion tests of RG-REQ-009-L01.

**Evidence (v0.18.0):** OBSERVED MET for the symbol name; NOT MET for scope (RC-9).

### RG-REQ-003-L03 — One file universe, stated once, reconciling across `orient`, `stats` and `check`

`orient` prints `N files indexed (M source; K config/contract/unreadable, tracked only)` with N = M + K; `check` states the same N for the same snapshot; `stats`' total names its exclusion with R = indexed − Σ groups computed, not asserted; a fourth basis that does not reconcile is recorded with its residual and escalated, never fitted.

**Verification criterion:** `orient_tests.rs::header_renders_indexed_source_tracked_only_split`; `agent/tests/check_repo.rs` (`orient_and_check_state_the_same_indexed_file_total`, `orient_module_summary_reconciles_indexed_source_and_tracked_only`); `stats.rs::render_human_warns_when_grouped_exceeds_indexed`.

**Evidence (v0.18.0):** OBSERVED MET (grpc 1917 = 1627 + 290).

### RG-REQ-003-L04 — `check`'s verdict is ceiling-relative and names the ceiling

When every materially present language has no resolver path, `CALL_GRAPH_RELIABILITY` and `ENRICHMENT_STATE` render as passing stated limitations carrying "Call-graph resolution has reached this build's ceiling for <langs> (no resolver exists) — N% resolved is the deterministic-extraction figure; verify call/dead claims against source"; the reliability figures are unchanged; JSON carries additive `ceiling: true`; verdict → exit-code mapping is frozen.

**Verification criterion:** `agent/src/check/reduce.rs` ceiling tests; `check/mod.rs::ceiling_marker_present_only_when_set_and_serde_skipped_otherwise`; a human-render assertion of the ceiling sentence in `presentation/check.rs` (to be added — none exists); field: nginx passes at 43% with the ceiling sentence, FRAKTAG fails at 28%.

**Evidence (v0.18.0):** OBSERVED MET (keep-and-imitate item).

### RG-REQ-003-L05 — `orient`'s cycle line is `cycles`' walk or the unordered form, from one derivation

One derivation (`agent::cycle_walk`) feeds `orient` and `cycles`; a ring renders only from a verified walk via the shared formatter; otherwise `largest: N modules — rmap cycles`; a one-element or non-string walk renders "cycle walk unreadable on this snapshot — run `rmap cycles`", never a self-ring or partial ring.

**Verification criterion:** `orient_tests.rs` (`cycle_anchor_full_chain_for_small_cycle`, `cycle_anchor_unordered_form_when_no_walk`, `cycle_anchor_one_element_walk_is_not_a_fabricated_self_ring`, `cross_surface_cycle_walk_agrees_ordered`, `cross_surface_cycle_walk_agrees_unordered_without_edges`).

**Evidence (v0.18.0):** OBSERVED MET (COHERENCE-3). Residual: leveldb orient rendered a 4-member cycle as a 2-member path in round six — to be checked against this L.

### RG-REQ-003-L06 — The type-only cycle verdict survives a test-only partition and matches `cycles`

`orient` reads the SCC state from the first production cycle before top-N truncation and renders the label independently of the anchor gate; `(N test)` means "of which", `(+N test-only excluded)` means "in addition", distinguished by one legend line; `orient` and `cycles` print the same verdict.

**Verification criterion:** `orient_tests.rs` (`coh2_type_only_verdict_survives_test_only_partition`, `coh2_type_only_verdict_read_from_field_when_production_cycle_truncated_out`, `type_only_verdict_rendered_identically_by_cycles_and_orient`).

**Evidence (v0.18.0):** OBSERVED MET (COHERENCE-2).

### RG-REQ-003-L07 — Budgets change depth and length, never facts

Every number `orient` prints derives from the same snapshot reads at `small | medium | large | --full`; small is the dense headline set (structure · named complexity centers · cycles · docs · one reliability caveat); large/`--full` is the complete detail; markers never deny an elision (RG-REQ-012-L04).

**Verification criterion:** `orient_density_tests.rs` (`rendered_numbers_are_budget_invariant_length_is_monotonic`, `budget_ladder_is_progressive_small_medium_large`, `honesty_posture_present_at_every_tier`); `orient_tests.rs::small_is_dense_not_thin_meta`.

**Evidence (v0.18.0):** OBSERVED MET for invariance; `orient --full` is the weakest ECONOMY cell (carried).

### RG-REQ-003-L08 — Surfaces order providers → consumers → fixtures, and no "0 project surfaces"

`orient`'s and `surfaces list`'s surface lines order providers, consumers, then a `test fixtures (excluded from counts)` section; the headline equals the rows above the section; a zero "project surfaces" line is omitted beside HTTP routes or names what it counts.

**Verification criterion:** `surfaces.rs::list_render_zero_project_surfaces_line_omitted_when_http_present`; an ordering test for providers → consumers → fixtures (to be added — HEADLINE-TRUTH-1 names it; not locatable); orient section-order test (to be added).

**Evidence (v0.18.0):** OBSERVED MET in the field (corpus grep empty); order untested.

### RG-REQ-003-L09 — The modules method line and the docs recommendation render on `orient`

`orient`'s module section opens with the method line (`N declared in Cargo.toml (workspace members) · M inferred from top-level directories`) and carries the repo-specific documentation recommendation (`For module boundaries as the authors describe them, read: README.md, docs/ — or the tree`) derived from the docs inventory; an all-inferred repo adds "boundaries are a guess from directory names"; the line never reads the same on every repo. There is no further module discovery (human ruling 2026-09-07).

**Verification criterion:** `orient_tests.rs::orient_method_*`; `modules_list_tests.rs` (`method_line_renders_*`, `orientation_recommendation_renders`, `inequality_across_fixtures`); orient-side assertions on the recommendation (to be added — wired at `orient_sections.rs:661`, asserted only in the modules tests).

**Evidence (v0.18.0):** OBSERVED MET (MODULES-METHOD-1); blind to root `README.rst`/`.txt` (RG-REQ-008).

### RG-REQ-003-L10 — Runtime and build environment per module appears in orientation

`orient` shall state, per module or package group, the build system that owns it (Cargo workspace member, Gradle project, npm workspace, pyproject, CMake/meson where read) and, where persisted, the toolchain provenance of its evidence — so the agent knows how each module is built and run before editing.

**Verification criterion:** the method line already names manifest families per module (L09); a per-module build-owner and runtime line with its test (to be added once TC-1 ships — RG-REQ-007-L11).

**Evidence (v0.18.0):** PARTIALLY MET — manifest family per module is stated; runtime/toolchain provenance NOT MET (TC-1 planned).

## Preservation obligations named by the ratifying specifications

- The agent-facing output shape is the product, not a frozen API (MODULE-MODEL-1 D5; Change Doctrine) — only the governance surface's verdict states and exit codes are frozen.
- HEADLINE-TRUTH-1 §3: wire additive, exit codes, schema shape, the FOURTH-BASIS STOP.
- CHECK-SIGNAL-1 §3: verdict → exit-code mapping; reliability figures (only the condition's classification may change); `check` is MATURE and CI-facing.
- ORIENT-SEGMENT-2 §3: schema, stats computation (orient consumes it), trust, LiveGraph/witness/union, the orient JSON envelope beyond additive fields; leveldb's orient is the gold standard.
- COHERENCE-2/3 §3: cycle computation and exclusion semantics, the CYCLES-B certificate; ORIENT-FACT-COHERENCE-1: the serving routes and the no-loss certificate; the "budget-dependent facts" report was a temporal race, not routing — do not re-litigate.
- ECONOMY-2 §3: facts, ranking, seeds computation; an elision line always states N and where.
- `docs/architecture/agent-orientation-contract.md` is stale as a source except its INDEX-BASIS section and focus-resolution precedence.

## Review and approval

Independent requirements review: PERFORMED (Codex gpt-5.6-terra, REFINE; findings applied).
Human approval: PENDING.
Implementation and verification: evidence lines are the manager's OBSERVED reading; they are not acceptance.
