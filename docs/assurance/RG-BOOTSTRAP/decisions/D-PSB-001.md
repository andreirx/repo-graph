# D-PSB-001 — Does a same-depth Python MRO collision carry its own unresolved CATEGORY or its own BASIS CODE?

Raised: 2026-09-20 by the manager while grounding PYTHON-SELF-BINDING-1 (Q9) on HEAD ed78de2, before the document review; to be challenged by the codex gpt-5.6-terra document reviewer of INPUT-1.
Resolved: 2026-09-20 by the in-place manager (operator) — basis code — under the requirement's own verification criterion; the human may override.

## Problem

RG-REQ-005-L03 says a same-depth collision "stays unresolved with its own category", and its verification criterion names `agent/src/attribution.rs::every_basis_code_maps_to_its_expected_reader_class`. Those two halves point at different columns of the `unresolved_edges` row. The read-only seam investigation of 2026-09-20 found: (1) `attribution.rs` has NO category axis — its `EXPECTED` table (:545-614) and its one exhaustive match `attribution_class` (:140-173) are keyed by `UnresolvedEdgeBasisCode` (17 values); a new category would leave that test green and prove nothing; (2) a new `UnresolvedEdgeCategory` fans out across eleven registries that are not compile-forced: `storage/src/call_resolution_reads.rs:80-85` (`CALLS_CATEGORIES: [_; 4]`), `trust/src/service.rs:187-202` (label table with a raw-string fallback) and `:909-914`, `trust/src/rules.rs:359-365`, `classification/src/unresolved_classifier.rs` category dispatch (:262-278, :301-330, :369-393), `framework_boundary.rs:59`, `enrichment/src/contracts.rs:57-87` (a separate wire enum), `blast_radius.rs`, plus the indexer parity corpus; (3) a new basis code is additive by the vocabulary's own contract (`classification/src/types.rs:442-445`, no version bump, no migration — `basis_code TEXT NOT NULL` without CHECK), has exactly ONE compile-forced site (`attribution_class`), two wildcarded matches whose fallbacks (`LocalLike`, `None`) are correct for a collision, and maps onto the reader class the reader already sees: `your own code (call target not resolved)`; (4) the resolver has no reason channel today (`CategorizedUnresolvedEdge` :251-255 carries category + the edge) — the collision reaches the classifier through the cloned edge's `metadata_json` (`orchestrator.rs:1093`), the same channel the extractor's carrier uses.

## Options

1. **Basis code** (`self_call_ambiguous_mro`, classification `internal_candidate`, category unchanged) — reward: the L's own criterion becomes a real gate; the reader sees the collision in the own-code bucket; one compile-forced site; no category-table churn; evidence (`mroCandidates`) persisted on the row; risk: diverges from the L's word "category"; the first resolver-originated basis (precedent: the Java import reasons became categories).
2. **Category** (`calls_self_method_ambiguous_mro`) — reward: matches the L's word; risk: eleven registries to edit by hand with no compiler help, a per-category trust label to invent, the enrichment wire enum to touch, and the L's named verification criterion still unsatisfiable.
3. **Both** — reward: none beyond 1; risk: the union of the costs.

## Resolution — option 1

The collision keeps `calls_obj_method_needs_type_info` and carries `self_call_ambiguous_mro`; `attribution_class` gains one arm; `EXPECTED` 17 → 18; the classifier reads `mroCandidates` from the edge metadata the resolver appends. If the human rules for option 2, the change is a re-baseline of PYTHON-SELF-BINDING-1 (INPUT-n) with the registry list above as its candidate paths.

Ratified 2026-09-21: the human chose option B of D-PSB-002 (raised by the INPUT-1 document review); RG-REQ-005-L03's wording now names the basis code. This resolution is no longer an operator override of the requirement text.
