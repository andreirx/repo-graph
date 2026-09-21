<!-- requirements-assurance-v1
{
  "formatVersion": 1,
  "kind": "requirement",
  "requirementId": "RG-REQ-005",
  "sources": [
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "orientation-not-oracle" },
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "strategic-position" },
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "honesty-rules" },
    { "kind": "document-section", "path": "docs/slices/symbol-identity-1.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/cpp-declarators-1.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/anchors-everywhere-1.md", "fragment": "2-contract" }
  ],
  "lowLevelRequirements": [
    { "id": "RG-REQ-005-L01", "parentId": "RG-REQ-005" },
    { "id": "RG-REQ-005-L02", "parentId": "RG-REQ-005" },
    { "id": "RG-REQ-005-L03", "parentId": "RG-REQ-005" },
    { "id": "RG-REQ-005-L04", "parentId": "RG-REQ-005" },
    { "id": "RG-REQ-005-L05", "parentId": "RG-REQ-005" },
    { "id": "RG-REQ-005-L06", "parentId": "RG-REQ-005" },
    { "id": "RG-REQ-005-L07", "parentId": "RG-REQ-005" },
    { "id": "RG-REQ-005-L08", "parentId": "RG-REQ-005" },
    { "id": "RG-REQ-005-L09", "parentId": "RG-REQ-005" },
    { "id": "RG-REQ-005-L10", "parentId": "RG-REQ-005" }
  ]
}
-->
# RG-REQ-005 — A symbol's relationships are discovered, never invented: who calls it, what it calls, what it is

Status: DRAFT authored by the in-place manager 2026-09-12. Independent requirements review: PERFORMED 2026-09-12 by Codex gpt-5.6-terra (standalone, read-only) — verdict REFINE; its findings are applied in this revision (record: reviews/2026-09-12-requirements-review-0-terra.txt). Human approval: PENDING.
Maturity: PROTOTYPE (requirements record).

## Source and intent

Sources: [VISION — Orientation, Not Oracle](../VISION.md#orientation-not-oracle) ("if repo-graph can surface more precise information — callers, consumers, exact call sites — it will"); [VISION — Strategic Position](../VISION.md#strategic-position) ("a `callers` query replaces a multi-file search"); [VISION — Honesty Rules](../VISION.md#honesty-rules); the Certainty Model's Layer 0–1 claim ("this IS the call graph" may be claimed only for extracted facts); the ratified contracts of SYMBOL-IDENTITY-1 (ruling EXPLAIN-RESOLVER-ROUTING = B), CPP-DECLARATORS-1, ANCHORS-EVERYWHERE-1, EXPLAIN-*; the v0.18.0 root causes RC-1 (regression), RC-2, RC-3 and the Codex adjudication's invariant correction. The codegraph comparison (2026-09-06) established that symbol/impact questions are where rmap is weakest.

## High-level requirement

An agent asking `rmap find`, `explain <symbol>`, `callers`, `callees` or `path` shall receive the symbol's relationships as they exist in the source — every listed edge backed by binding evidence, every unresolved reference counted rather than guessed, every identity the product prints accepted back as input, and a type described by its members and uses — so that acting on the answer never leads the agent to a call that does not exist or away from one that does.

**Scope:** symbol resolution, call binding, the relationship renderers. Import resolution and dependency classification are RG-REQ-006; the seed tier beneath `find` is RG-REQ-010.

**High-level acceptance:** all L entries hold on the fixture parity corpora and on the audit's symbol probes (leveldb `DBImpl::Recover`, django `BaseHandler.get_response`, vcmi `CGHeroInstance`, FRAKTAG `ConversationManager`, spring-petclinic `processCreationForm`).

## Low-level requirements

### RG-REQ-005-L01 — An indirect receiver never binds to the enclosing class without receiver-type evidence

A CALLS edge whose call site carries a receiver other than `this`/`self` (or their explicit equivalents) shall not resolve to the caller's own enclosing type unless receiver-type evidence supports it (the receiver's declared field or local type, or an inheritance walk from it); an explicit `this->m()` legitimately may. Without such evidence the call stays unresolved and counted (L02, L09).

**Verification criterion:** unit fixture — two classes `A` and `B` each defining `run`, where `B` holds a member `a_` of type `A*` and `B::run` calls `a_->run()`: `callers A::run` = `B::run`, `callees B::run` = `A::run`, no self-loop (an indirect receiver typed as the OTHER class — the RC-1 shape); corpus assertion on leveldb, OpenXcom, vcmi, poco, duckdb, gstreamer: `SELECT count(*) FROM edges WHERE type='CALLS' AND source_node_uid=target_node_uid AND metadata_json LIKE '%receiver%'` = 0; field: `callers leveldb::DBImpl::Recover` = `leveldb::DB::Open` (db_impl.cc:1503) and not itself; `callees leveldb::DBImpl::Recover` includes `VersionSet::Recover` and excludes itself. The existing tests `indexer/src/resolver.rs::enclosing_class_preference_resolves_ambiguous_method_by_caller_container` (a fictional `DBImpl::Open` caller) and `::enclosing_class_preference_leaves_outside_caller_unresolved` encode the defect and shall be rewritten.

**Evidence (v0.18.0):** NOT MET — RC-1, a REGRESSION introduced by CPP-DECLARATORS-1 (5c3ec2d): self-loops 0 → 155 leveldb / 497 OpenXcom / 836 vcmi / 1,098 poco / 2,522 duckdb / 321 gstreamer; codegraph 0. Queue Q1. Consequences to pre-authorise: trust's calls-resolved % falls; `dead` reports more candidates; C stays byte-stable (no receiver emitted).

### RG-REQ-005-L02 — A call binds to a target only on evidence

A call shall bind when exactly one candidate matches by name after declaration/definition filtering, or when receiver-type evidence selects one; an enclosing-type preference may be applied ONLY to receiverless calls and explicit `this`/self calls — never to a call with an indirect receiver (L01); an ambiguous pool with no such evidence stays unresolved with a named category. Narrowing what is ambiguous is permitted only by adding evidence, never by widening what counts as evidence.

**Verification criterion:** `indexer/src/resolver.rs` (`ambiguous_name_stays_unresolved`, `two_definitions_stay_ambiguous`, `one_definition_plus_n_forward_decls_resolves_to_the_definition`, `method_call_prototype_plus_definition_resolves_to_definition`).

**Evidence (v0.18.0):** PARTIALLY MET — unique-name and decl/def halves hold; the receiver-type half does not exist (the extractor stores the receiver as metadata only) and the enclosing-class preference substitutes a guess (L01).

### RG-REQ-005-L03 — An inherited `self.method()` binds through the class hierarchy or stays honestly unresolved

In Python, `self.m()`/`cls.m()` inside class `C` shall resolve by walking `C` and its superclass closure breadth-first; the first depth with exactly one METHOD named `m` wins; a same-depth collision (a real MRO ambiguity) stays unresolved with its own basis code (`self_call_ambiguous_mro`, mapped by `agent/src/attribution.rs` to the reader's own-code class; the row keeps the category `calls_obj_method_needs_type_info`) and its candidates persisted as evidence. Superclass facts come from the class's stored metadata; no other language's binding changes. (Wording ratified 2026-09-21 by the human — D-PSB-002, option B: the earlier text said "its own category", an axis with no reader mapping; the verification criterion below had always named the basis-code test.)

**Verification criterion:** resolver fixtures (`Base.run` / `Sub(Base)` calling `self.run()` → one CALLS edge to `Base.run`; two same-depth ancestors both defining `run` → an `unresolved_edges` row with basis `self_call_ambiguous_mro` whose `metadata_json.mroCandidates` lists both candidates as persisted evidence); `agent/src/attribution.rs::every_basis_code_maps_to_its_expected_reader_class` covers `self_call_ambiguous_mro`; field: `explain BaseHandler.get_response` on django lists `WSGIHandler.__call__` (call site wsgi.py:124; the rendered row anchors the caller's declaration, :120) and `ClientHandler.__call__` (client.py:186; row anchor :169), not `ASGIHandler`, not exception.py's closure calls; leveldb and FRAKTAG edge counts byte-stable.

**Evidence (v0.18.0):** NOT MET — RC-2 (never worked): every ground-truth call site is an `unresolved_edges` row (`calls_obj_method_needs_type_info`); 26,461 of django's 91,514 unresolved calls are `self.`/`cls.` calls; simulated gain ~3,600 edges with 385 MRO collisions correctly declined. Queue Q9.

### RG-REQ-005-L04 — `explain <Type>` describes the type

When the focus resolves to a type (class, struct, interface, enum, trait), `explain` shall render its members (from the store's containment, anchored by line), its base and derived types (from inheritance edges anchored on the type node), and the files that reference it, in every language — a type is never called, so `Callers (0) / Callees (0)` is not an answer and shall say "a type is not called; see members / referenced by" when rendered.

**Verification criterion:** `explain CGHeroInstance` on vcmi → Members ≥ 135 anchored at `CGHeroInstance.h:<line>`, base classes = the 7 declared (indexed ones resolved), derived classes, Referenced by = 178 files grouped by module; `explain BaseHandler` (django) and `explain ConversationManager` (FRAKTAG, by NAME not path) list methods; explain output-contract tests updated; vcmi IMPLEMENTS shape histogram loses `SOURCE→CLASS`.

**Evidence (v0.18.0):** NOT MET — RC-3 (never worked, language-wide): members (135 nodes) and referencing files (178 include edges) are already in the store (render-only); C++ base-clause edges are anchored on the FILE node (`cpp-extractor/src/extractor.rs:1288`) and dropped on macro-decorated declarations (451 `class DLL_LINKAGE` in vcmi/lib). Queue Q6, ordered render → anchor → macro recovery.

### RG-REQ-005-L05 — One symbol identity: what `find` prints, `explain`/`callers`/`callees` accept

Every row `find` prints shall resolve unchanged by stable key, qualified name or qualified suffix (`<sep><query>`, `<sep>` in `::`/`.`); exactly one hit resolves; more than one lists candidates with files; none is not-found with the searched classes named. The suffix tier runs only after the exact tiers miss.

**Verification criterion:** `storage/src/queries.rs` (`resolve_symbol_accepts_qualified_suffix`, `…suffix_ambiguous_lists_candidates`, `…suffix_prefers_definition_over_declaration`, `…bare_short_name_not_suffix_widened`); `agent/tests/explain_symbol.rs::explain_resolves_qualified_suffix_through_shared_resolver`.

**Evidence (v0.18.0):** OBSERVED MET (f5cfe1e). Preservation: `resolve_symbol` is delegated to SQLite by the orient decorator; `resolve_symbol_name` stays name-only for orient and the LiveGraph parity certificate — do not unify.

### RG-REQ-005-L06 — A miss is never `Confidence: high`

`explain`'s no-match and ambiguous arms shall derive confidence from the resolution path's state (never above `medium`; `low` when stated so) and the render shall never print `high` beside `no_match`.

**Verification criterion:** `rgr/src/presentation/explain.rs` (`no_high_confidence_beside_no_match`, `render_shows_unresolved_target`, `no_match_without_seed_candidate_is_todays_bare_line`).

**Evidence (v0.18.0):** OBSERVED MET.

### RG-REQ-005-L07 — Forward declarations are a stored fact that renders `(decl)` below its definition

Types and in-class method prototypes shall carry `metadata_json.forward_decl`; a many-times-declared C++ class is not ambiguous and does not lose its inheritance edges; `find` and the seed tier render declarations as `(decl)` ranked below the definition; an unreadable flag is a named error, never defaulted to "definition".

**Verification criterion:** `daemon-runtime/src/find_facts/rank.rs::definition_beats_forward_decl`; `indexer/src/resolver.rs` (`forward_decl_classification_reads_the_stamped_key`, `…malformed_metadata_is_unreadable_not_a_definition`, `lone_declaration_still_resolves_when_no_definition_indexed`, `affinity_cpp_implements_admits_class_and_struct`); field: vcmi `find CGHeroInstance`.

**Evidence (v0.18.0):** OBSERVED MET (CPP-DECLARATORS-1). The IMPLEMENTS 2→852 count is provisional from a class focus (L04).

### RG-REQ-005-L08 — A relationship row anchors the call site from one store

`callers`/`callees` rows shall render `path:line` of the CALL SITE (the edge's line), not the caller's declaration line; every symbol citation on explain/find/orient/boundaries/inferences/resources renders a line from one store; absence renders no line, never 0 or 1.

**Verification criterion:** ANCHORS-EVERYWHERE-1 per-surface tests and the single-source assertion; a callers test asserting the call-site line (to be added — none exists); field: `callers leveldb::DBImpl::Recover` anchors `db_impl.cc:1511`, not `:1503`.

**Evidence (v0.18.0):** PARTIALLY MET — anchors present everywhere (F2/F7 shipped); callers/callees currently anchor the declaration line (RC-1 render note).

### RG-REQ-005-L09 — Unresolved calls are classified, counted, and visible beside a zero

Each unresolved call shall carry a named category and basis; where `callers`/`callees`/`explain` render a zero, the count of unresolved calls that name this symbol's short name shall be stated with its categories ("0 resolved callers; 2 unresolved calls name `Recover` (ambiguous: 2)"), so a zero never reads as absence.

**Verification criterion:** `indexer/src/resolver.rs` categorize tests (`categorize_calls_this_method`, `categorize_calls_obj_method`, `categorize_calls_function`, `java_import_wildcard_is_named_basis`); a render test for the zero-state count (to be added); field: `explain BaseHandler.get_response` before L03 ships reads the unresolved count beside Callers (0).

**Evidence (v0.18.0):** PARTIALLY MET — classification MET at the store; the count reaches import zero-states but not the call surfaces (audit D-N9 / CLAIM-INVARIANT-1).

### RG-REQ-005-L10 — A relationship query pays for itself

A `callers`/`callees`/`explain` answer shall be denser than the multi-file search it replaces: signal bytes (rows with anchors and relationships) dominate boilerplate bytes, measured by the usefulness protocol's ECONOMY dimension per release, with boilerplate and signal reported separately.

**Verification criterion:** `docs/testing/end-to-end-usefulness-protocol.md` ECONOMY grading per command × repo, recorded in `docs/audits/*-per-command-usefulness-*.md`; no unit test (a measured claim).

**Evidence (v0.18.0):** OBSERVED — ECONOMY B- for explain/callers/callees while HIT and HONESTY are D/D+ (the worst cells in the matrix): the economy is real, the content is not yet.

## Preservation obligations named by the ratifying specifications

- Wire protocol additive only; `metadata_json` keys additive, no new columns; exit codes per `docs/contracts/exit-codes.md`.
- `find`'s substring match semantics; the classification of unresolved calls; `ambiguous_name_stays_unresolved` refusal semantics.
- The LiveGraph↔SQLite parity certificate; `resolve_symbol_name` name-only for orient and the cert; "nodes-free on green" explain invariant as amended for the resolution step only (ruling B).
- Tree-sitter grammar version bumps are DECISION_REQUIRED; non-C/C++ extractors byte-stable under any C/C++ fix; C emits no receiver.
- `dead`'s disabled state (RG-REQ-013).

## Review and approval

Independent requirements review: PERFORMED (Codex gpt-5.6-terra, REFINE; findings applied).
Human approval: PENDING.
Implementation and verification: evidence lines are the manager's OBSERVED reading; they are not acceptance.
