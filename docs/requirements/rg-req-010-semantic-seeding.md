<!-- requirements-assurance-v1
{
  "formatVersion": 1,
  "kind": "requirement",
  "requirementId": "RG-REQ-010",
  "sources": [
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "semantic-seeding-ratified-2026-08-24" },
    { "kind": "document-section", "path": "docs/slices/find-facts-1.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/seed-chunk-3.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/seed-chunk-3.md", "fragment": "3-stop-conditions" }
  ],
  "lowLevelRequirements": [
    { "id": "RG-REQ-010-L01", "parentId": "RG-REQ-010" },
    { "id": "RG-REQ-010-L02", "parentId": "RG-REQ-010" },
    { "id": "RG-REQ-010-L03", "parentId": "RG-REQ-010" },
    { "id": "RG-REQ-010-L04", "parentId": "RG-REQ-010" },
    { "id": "RG-REQ-010-L05", "parentId": "RG-REQ-010" },
    { "id": "RG-REQ-010-L06", "parentId": "RG-REQ-010" },
    { "id": "RG-REQ-010-L07", "parentId": "RG-REQ-010" },
    { "id": "RG-REQ-010-L08", "parentId": "RG-REQ-010" },
    { "id": "RG-REQ-010-L09", "parentId": "RG-REQ-010" },
    { "id": "RG-REQ-010-L10", "parentId": "RG-REQ-010" },
    { "id": "RG-REQ-010-L11", "parentId": "RG-REQ-010" }
  ]
}
-->
# RG-REQ-010 — Semantic seeds are labeled guesses beneath the facts, never the answer

Status: DRAFT authored by the in-place manager 2026-09-12. Independent requirements review: PERFORMED 2026-09-12 by Codex gpt-5.6-terra (standalone, read-only) — verdict REFINE; its findings are applied in this revision (record: reviews/2026-09-12-requirements-review-0-terra.txt). Human approval: PENDING.
Maturity: PROTOTYPE (requirements record).

## Source and intent

Sources: [VISION — Semantic Seeding (ratified 2026-08-24)](../VISION.md#semantic-seeding-ratified-2026-08-24) — four bounds (candidate generator never answer; evidence-backed hint, labeled; deterministic given pins; local and optional) and the ratified uses (resolution fallback, `find`'s demoted tier, cross-module concern hints); the ratified contracts of EMBED-SEED-1, FIND-FACTS-1, FIND-EVIDENCE-1, SEED-CHUNK-1/2/3; EMBED-SEED-SPIKE-1; the v0.17.0 root cause §D11 and the filed SEED-DOCUMENT-1.

## High-level requirement

When an agent arrives with a task rather than a symbol name, `rmap find "<concept>"` and the resolution fallback of `explain`/`callers`/`callees`/`path` shall offer embedding-ranked candidates — each with score, provenance and the runnable deterministic follow-up — strictly beneath every deterministic fact, partitioned and tiered so that test code, declarations and one-line fields never outrank the code that does the work, and shall degrade to "no hints" (never to degraded orientation) when the local model is absent.

**Scope:** the seed corpus, document composition, ranking tiers, rendering of the seed tier, its pins and floors. The facts tier of `find` is RG-REQ-005/RG-REQ-012.

**High-level acceptance:** all L entries hold on the FRAKTAG persistence query, the leveldb `Recover` decl/impl proof, the retention and obsolete control queries of SEED-CHUNK-2, and the repo-graph self-queries.

## Low-level requirements

### RG-REQ-010-L01 — Seeds are a candidate generator beneath every deterministic tier, never in the map

The facts tier shall render first and always answer; embedding candidates appear only in the previously empty `candidates`/no-match positions and in `find`'s lower tier, at most 5 on the resolution fallback; `orient`'s facts, `map` and `modules` never contain embedding-derived facts; `--exact` never consults the model.

**Verification criterion:** `rgr/src/commands/find/tests.rs` (`facts_render_above_seeds_with_class_and_command_labels`, `exact_mode_omits_seed_section_entirely`, `fact_hit_with_only_subfloor_seeds_does_not_claim_nothing_matched`); `rgr/src/presentation/explain.rs::semantic_no_match_renders_labeled_candidates_in_human_mode`.

**Evidence (v0.18.0):** OBSERVED MET.

### RG-REQ-010-L02 — Every seed carries score, provenance, anchor and the runnable follow-up

Each seed row shall render its similarity score, `source: embedding` with the model id (hoisted once to the heading; repeated per row only when a row's model differs), `path:line` and qualified name, module, and an `explain <candidate>` cursor that runs as printed from the repository root; every emitted cursor is non-mutating.

**Verification criterion:** `rgr/src/commands/find/seed_render_tests.rs` (`chunk_seed_renders_path_line_anchor_qualified_name_and_test_label`, `non_embedding_seed_source_is_counted_never_relabeled`, `in_root_composable_seed_row_drops_cursor_and_shows_kind`, `out_of_root_seed_cursor_keeps_full_cd_form`).

**Evidence (v0.18.0):** OBSERVED MET.

### RG-REQ-010-L03 — Seeds are deterministic given their pins

Every vector shall be pinned `(model_id, dim, content_sha)`; a model mismatch is a hard fail to "no hints"; per-item staleness excludes and recomputes that item; ranking within a store is the pure function `(-score, path)`. No cross-machine score equality or top-5 membership stability is claimed.

**Verification criterion:** `daemon-runtime/src/seed/local_engine.rs` model-id assertion; `repo-graph-seed/src/rank.rs::within_block_orders_by_score_then_path`; `daemon-runtime/src/find_facts/rank.rs::total_order_is_deterministic_regardless_of_input_order`; `repo-graph-seed/src/hash.rs`.

**Evidence (v0.18.0):** OBSERVED MET within a store.

### RG-REQ-010-L04 — Local and optional: absence degrades to "no hints", never to degraded orientation

`find` shall answer from facts when the model is unavailable and the seed tier shall say unavailable with its reason; "unavailable" without a reason and a missing availability field are malformed states, never defaults; pre-migration stores self-heal by a scheduled re-seed rather than going silently seedless; embedding never blocks the foreground.

**Verification criterion:** `find/tests.rs::endpoint_down_renders_facts_and_seed_unavailable_with_reason`; `seed_render_tests.rs` (`seeds_available_missing_is_malformed_never_defaulted`, `unavailable_without_reason_is_malformed`, `seed_below_floor_with_missing_score_is_still_surfaced_not_swallowed`).

**Evidence (v0.18.0):** OBSERVED MET. The standing honesty rule (no `unwrap_or`/`.ok()` on a seed read) is encoded as tests.

### RG-REQ-010-L05 — The floor and the model are frozen constants with a recorded calibration

`SEED_SIMILARITY_FLOOR = 0.30` (presentation-side) and the model `minishlab/potion-code-16M-v2` (256-dim, L2-normalised, in-process) shall not change without human ratification; all-below-floor renders an honest abstain naming the best score; scores are never tuned to a query — a tier that cannot meet a target stops with the measured alternative.

**Verification criterion:** constants at `rgr/src/commands/find/seed_render.rs` and `daemon-runtime/src/seed/local_engine.rs`; `seed_render_tests.rs` (`all_seeds_below_floor_abstains_with_best_score`, `seeds_above_floor_render_and_sub_floor_are_dropped_without_abstain`).

**Evidence (v0.18.0):** OBSERVED MET.

### RG-REQ-010-L06 — Test chunks partition below production chunks on structural evidence

A chunk is test-partitioned by its file's `is_test` or a structural per-symbol marker (Rust `#[test]`/`#[cfg(test)]` on the item or an enclosing module; TS/JS enclosing `describe(`/`it(`/`test(`; Python only with a framework marker); names are never the basis; an unknown partition ranks with production (never demoted) and renders an explicit unknown marker.

**Verification criterion:** `repo-graph-seed/src/rank.rs` (`production_ranks_above_test_even_when_test_scores_higher`, `a_test_partition_field_stays_within_the_test_partition`); `classify_tests.rs`; `seed_render_tests.rs` partition-header and unknown-marker tests; `find_facts/rank.rs::rule_a_unknown_is_test_ranks_with_non_test_never_demoted`.

**Evidence (v0.18.0):** OBSERVED MET (SEED-CHUNK-2).

### RG-REQ-010-L07 — A declaration never outranks an implementation of the same qualified name

Within a partition, a bodiless chunk ranks below every body-bearing chunk of the same qualified name and renders `(decl)`; the partition is applied first (a production decl may sit above a test impl); a decl with no impl keeps its own score and still appears.

**Verification criterion:** `rank.rs` (`impl_outranks_its_own_decl_even_when_decl_scores_higher`, `decl_ranks_below_every_impl_of_its_name_even_a_lower_scoring_one`, `production_decl_outranks_a_test_impl_of_the_same_name`, `a_decl_with_no_matching_impl_keeps_its_own_score_and_is_labeled`); field: leveldb `db_impl.cc:292` above `db_impl.h:113`.

**Evidence (v0.18.0):** OBSERVED MET.

### RG-REQ-010-L08 — A one-line field never outranks the code that does the work

A PROPERTY/FIELD chunk, or a ≤1-line span with no doc comment, ranks in a FIELD tier below every body-bearing chunk of its partition and renders `[field]`; fields stay in the corpus (`find "sessionId field"` still hits them); the decl tier and the field tier are distinct rules.

**Verification criterion:** `rank.rs` (`field_ranks_below_body_bearing_even_when_it_scores_higher`, `field_below_every_body_bearing_chunk_of_its_partition_not_just_the_best`, `field_boundary_is_not_a_near_tie`); field: FRAKTAG persistence query renders 0 `[field]` rows in the top 10 and `ConversationManager` in the top 3.

**Evidence (v0.18.0):** OBSERVED MET as restated (SC3-FIELD-DOD = C).

### RG-REQ-010-L09 — Printed order is monotone in printed score, or the inversion carries its label

A reader shall never see a higher score below a lower one without a rendered tier label (`(decl)`, `[field]`, `[test]`) on the demoted row explaining the inversion; the remedy is label completeness, never a score change.

**Verification criterion:** a render test asserting every score inversion in the seed block carries a label (to be added — none exists); field: leveldb `find` recovery probe (the 0.45 `(decl)` row printed sixth is labeled).

**Evidence (v0.18.0):** NOT MET — D-N7 (carried minor): the inversion exists and the label is not always present.

### RG-REQ-010-L10 — A method chunk's document carries enough beyond its name for a task query to meet it

STATUS: RATIFIED 2026-09-14 (human, option A); SEED-DOCUMENT-1 is scheduled after the round-six queue (a re-embedding of every repo is the stated cost). The embedded document of a method chunk shall carry, besides its qualified name, doc comment and leading body lines, its enclosing type's doc and a bounded identifier summary, so that a natural-language task query can reach the method that does the work; measured on the FRAKTAG persistence query with the floor and model unchanged and the SEED-CHUNK-2 control proofs byte-stable.

**Verification criterion:** `repo-graph-seed/src/document.rs` composition tests (pin the current form; to be extended); the recorded rank/score table (createSession 0.27, logTurn 0.15, TreeStore 0.20, ContentStore 0.20 vs floor 0.30) as the before; the same query as the after.

**Evidence (v0.18.0):** NOT MET — SEED-DOCUMENT-1 (its own root-caused slice; ratified position: after the round-six queue).

### RG-REQ-010-L11 — PROPOSED: cross-module concern hints, labeled Layer 3, beneath the deterministic facts

STATUS: PROPOSED, decision pending — the human asked (2026-09-14) why it matters and whether other means find the same thing; the manager's answer is in the ROADMAP decisions block of that date.

Cohesive embedding clusters spanning deployable modules shall be surfaced on the module/boundary discovery surfaces as labeled seam/concern candidates (score, provenance, member modules), beneath the deterministic facts and never in the map. STATUS: PROPOSED — VISION ratifies the use (iii) but its rendering surface is not yet specified; no obligation until the surface is ratified.

**Verification criterion:** to be defined with the surface (the glamCRM sales-targets/tenant-brand/exchange-rate/auth clusters of the spike addendum are the acceptance corpus).

**Evidence (v0.18.0):** NOT MET / PROPOSED — no surface exists.

## Preservation obligations named by the ratifying specifications

- `SEED_SIMILARITY_FLOOR = 0.30` and its calibration basis; the model and its dimension; the sidecar format and pins; the ranking formula (cosine + `(-score, path)`); floors and tiers are rank/presentation-side, never formula changes.
- The facts tier and its wall; the is_test partition; the FORGET-vs-SEED invariants; `find`'s match semantics; exit codes; additive migrations with SeedCoordinator self-heal.
- FOREGROUND-LOCK semantics (embedding never blocks the foreground); new dependencies limited to `model2vec-rs`; no LLM in the ranking loop; no cross-machine stability claim; lmstudio retired from the seed path only.

## Review and approval

Independent requirements review: PERFORMED (Codex gpt-5.6-terra, REFINE; findings applied).
Human approval: PENDING.
Implementation and verification: evidence lines are the manager's OBSERVED reading; they are not acceptance.
