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

A call shall bind as `static` only when exactly one candidate matches by name after declaration/definition filtering AND the receiver is `self`/`this`, a resolvable module alias, a class, a receiver with a declared type, or absent (a bare call), or when receiver-type evidence selects one; an enclosing-type preference may be applied ONLY to receiverless calls and explicit `this`/self calls — never to a call with an indirect receiver (L01); an ambiguous pool with no such evidence stays unresolved with a named category. Narrowing what is ambiguous is permitted only by adding evidence, never by widening what counts as evidence. A binding on a bare name alone with an UNTYPED receiver (a Python or TypeScript `obj.method()` whose `obj` is neither `self`/`this`, a resolvable module alias, a class, nor a receiver with a declared type) is INFERRED, never `static` (RG-REQ-002-L11): recorded with its candidate pool, rendered as inferred on `callers`/`callees`/`explain`, excluded from trust's resolved share and from `dead` (amended 2026-09-23; RC-2).

**Verification criterion:** `indexer/src/resolver.rs` (`ambiguous_name_stays_unresolved`, `two_definitions_stay_ambiguous`, `one_definition_plus_n_forward_decls_resolves_to_the_definition`, `method_call_prototype_plus_definition_resolves_to_definition`); to be added by PYTHON-RECEIVER-BINDING-1: a resolver test that a unique-name call on an untyped receiver is persisted `inferred` with its candidate pool and never `static`; renderer tests that `callers`/`callees`/`explain` present it only in the inferred remainder; a `dead` and a trust-numerator exclusion test; field: django `callers ListMixin.extend` renders 0 certain callers and "271 inferred (name-only) — investigate".

**Evidence (v0.18.0):** PARTIALLY MET — unique-name and decl/def halves hold; the receiver-type half does not exist (the extractor stores the receiver as metadata only) and the enclosing-class preference substitutes a guess (L01).

### RG-REQ-005-L03 — An inherited `self.method()` binds through the class hierarchy or stays honestly unresolved

In Python, `self.m()`/`cls.m()` inside class `C` shall resolve by walking `C` and its superclass closure breadth-first; the first depth with exactly one METHOD named `m` wins; a same-depth collision (a real MRO ambiguity) stays unresolved with its own basis code (`self_call_ambiguous_mro`, mapped by `agent/src/attribution.rs` to the reader's own-code class; the row keeps the category `calls_obj_method_needs_type_info`) and its candidates persisted as evidence. Superclass facts come from the class's stored metadata; no other language's binding changes. (Wording ratified 2026-09-21 by the human — D-PSB-002, option B: the earlier text said "its own category", an axis with no reader mapping; the verification criterion below had always named the basis-code test.)

**Verification criterion:** resolver fixtures (`Base.run` / `Sub(Base)` calling `self.run()` → one CALLS edge to `Base.run`; two same-depth ancestors both defining `run` → an `unresolved_edges` row with basis `self_call_ambiguous_mro` whose `metadata_json.mroCandidates` lists both candidates as persisted evidence); `agent/src/attribution.rs::every_basis_code_maps_to_its_expected_reader_class` covers `self_call_ambiguous_mro`; field: `explain BaseHandler.get_response` on django lists `WSGIHandler.__call__` (call site wsgi.py:124; the rendered row anchors the caller's declaration, :120) and `ClientHandler.__call__` (client.py:186; row anchor :169), not `ASGIHandler`, not exception.py's closure calls; leveldb and FRAKTAG edge counts byte-stable.

**Evidence (v0.18.0):** NOT MET — RC-2 (never worked): every ground-truth call site is an `unresolved_edges` row (`calls_obj_method_needs_type_info`); 26,461 of django's 91,514 unresolved calls are `self.`/`cls.` calls; simulated gain ~3,600 edges with 385 MRO collisions correctly declined. Queue Q9.

### RG-REQ-005-L04 — `explain <Type>` describes the type

When the focus resolves to a type (class, struct, interface, enum, trait), `explain` shall render its members (from the store's containment, anchored by line), its base and derived types (from inheritance edges anchored on the type node), and the files that reference it, in every language — a type is never called, so `Callers (0) / Callees (0)` is not an answer and shall say "a type is not called; see members / referenced by" when rendered. A bare type name whose candidates are exactly one type and its constructors shall resolve to the type in every language — membership is proved by the constructor's qualified-name container equalling the type's qualified name, with `::` (C++) or `.` (Java, whose constructors are `<pkg>.<Type>.<init>`) as the separator — and shall list every constructor as a stable-key cursor that runs as printed (`N constructor(s) also match: explain '<stable_key>', …`; D-JSAH-CURSOR-1).

Revision 2, 2026-10-05 (operator; D-JSAH-SCOPE-1 A, D-JSAH-CURSOR-1 A): the type/constructor collapse is named for every language and its cursors are stable keys.

**Verification criterion:** `explain CGHeroInstance` on vcmi → Members ≥ 135 anchored at `CGHeroInstance.h:<line>`, base classes = the 7 declared (indexed ones resolved), derived classes, Referenced by = 178 files grouped by module; `explain BaseHandler` (django) and `explain ConversationManager` (FRAKTAG, by NAME not path) list methods; explain output-contract tests updated; vcmi IMPLEMENTS shape histogram loses `SOURCE→CLASS`.

**Evidence (v0.18.0):** NOT MET — RC-3 (never worked, language-wide): members (135 nodes) and referencing files (178 include edges) are already in the store (render-only); C++ base-clause edges are anchored on the FILE node (`cpp-extractor/src/extractor.rs:1288`) and dropped on macro-decorated declarations (451 `class DLL_LINKAGE` in vcmi/lib). Queue Q6, ordered render → anchor → macro recovery.

**Evidence (v0.20.0, 2026-10-05, revision 2 clause):** NOT MET on Java — `explain KafkaProducer` (kafka, HEAD release binary) answers `Target: KafkaProducer (unresolved: ambiguous)` with the CLASS key and six CONSTRUCTOR keys (`…#KafkaProducer:SYMBOL:CONSTRUCTOR` … `:dup6`), because `agent/src/explain/mod.rs classify_type_constructor_collision` proves membership with `rsplit_once("::")` and the Java constructor qualified name is `org.apache.kafka.clients.producer.KafkaProducer.<init>`; by key the class renders (Members (73), Referenced by (90 files)). On C++ the collapse resolves (`explain Slice` on leveldb) but prints the qualified-name cursor `explain 'leveldb::Slice::Slice'`, which is itself ambiguous when the class has several constructors (RG-REQ-012-L03).

**Evidence (v0.20.0 + JAVA-SYMBOL-AMBIGUITY-HINT-1 66341d63, 2026-10-05):** revision 2 clause MET in every language — kafka `explain KafkaProducer --budget large` resolves the CLASS (`Target: KafkaProducer`, `Members (73)`, `Referenced by (90 files)`) and prints `6 constructors also match:` with six DISTINCT stable-key cursors (`…#KafkaProducer:SYMBOL:CONSTRUCTOR` … `:dup6`), each running to its own line (298, 315, 327, 342, 347, 487); leveldb `explain Slice --budget large` keeps its type answer (`Members (14)`, `Referenced by (20 files)`) and its four constructor cursors are now distinct and runnable (slice.h:33/36/39/42) where before the same `explain 'leveldb::Slice::Slice'` was printed four times and was itself ambiguous. Mechanism: `agent/src/explain/mod.rs::classify_type_constructor_collision` proves membership by the qualified-name container with `::` or `.` and sets `ConstructorHint.cursor` to the constructor's stable key; `presentation/explain.rs::render_related_cursors` quotes with `shell_quote_arg`. The v0.18.0 line's remaining items (base/derived types) are not re-read by this slice. Residual: leveldb's default constructor `Slice()` (slice.h:30) is absent from the store (C++ extractor gap, frozen here).

### RG-REQ-005-L05 — One symbol identity: what `find` prints, `explain`/`callers`/`callees` accept

Every row `find` prints shall resolve unchanged by stable key, qualified name or qualified suffix (`<sep><query>`, `<sep>` in `::`/`.`); exactly one hit resolves; more than one lists each candidate by the stable-key cursor `find` prints — the same key the command accepts — with its file:line and signature, ordered as the resolver returns them, and the hint names only a form that distinguishes the candidates (never "use qualified name" when the candidates share one, as Java and C++ overloads do; the hint names the cursor); none is not-found with the searched classes named. The suffix tier runs only after the exact tiers miss.

Revision 2, 2026-10-05 (operator; D-JSAH-SCOPE-1 A, from the 2026-10-03 audit's JAVA-SYMBOL-AMBIGUITY-HINT-1 observation, reproduced on the v0.20.0 kafka store): the "more than one" clause names the cursor form, the anchor and the hint rule; the resolution tiers are unchanged.

**Verification criterion:** `storage/src/queries.rs` (`resolve_symbol_accepts_qualified_suffix`, `…suffix_ambiguous_lists_candidates`, `…suffix_prefers_definition_over_declaration`, `…bare_short_name_not_suffix_widened`); `agent/tests/explain_symbol.rs::explain_resolves_qualified_suffix_through_shared_resolver`. Revision 2: `daemon-runtime/src/dispatch_ambiguous_matches.rs` tests (an overloaded `:dup2` key yields a match carrying the stable key, the stored qualified name, `SYMBOL:METHOD`, file, line and signature — never a mis-split name); `rgr/src/commands/ambiguous_matches.rs` tests (the human listing prints one shell-quoted `rmap <command> '<stable_key>'` per candidate with `<file>:<line>` and the signature, and no "use qualified name" text); `rgr/tests/daemon_dispatch.rs::callers_ambiguous_symbol_returns_structured_error` extended to assert `stable_key` in each match.

**Evidence (v0.18.0):** OBSERVED MET (f5cfe1e). Preservation: `resolve_symbol` is delegated to SQLite by the orient decorator; `resolve_symbol_name` stays name-only for orient and the LiveGraph parity certificate — do not unify.

**Evidence (v0.20.0, 2026-10-05, revision 2 clause):** NOT MET — on kafka (HEAD release binary, isolated copy of the v0.20.0 store) `callers KafkaProducer.send` lists `1. KafkaProducer.send  SYMBOL:METHOD …` and `2. KafkaProducer.send:SYMBOL  METHOD:dup2 …` (the `:dup2` key mis-split by `daemon-runtime/src/dispatch.rs:228 parse_ambiguous_matches`, `rsplitn(3, ':')`) with `hint: use qualified name for exact match`, although both overloads share the qualified name `org.apache.kafka.clients.producer.KafkaProducer.send` (`KafkaProducer.java:942` and `:1061`); the stable keys resolve (`callers '<key>:dup2'` → exit 0, line 1061). Never worked (4fb521d8). The resolution tiers (revision 1) remain MET.

**Evidence (v0.20.0 + JAVA-SYMBOL-AMBIGUITY-HINT-1 66341d63, 2026-10-05):** MET — on kafka (isolated copy of the v0.20.0 store, candidate binary) `callers KafkaProducer.send` still answers `error: symbol 'KafkaProducer.send' is ambiguous` (exit 2, honest) and now lists `Candidates (2) — each matches 'KafkaProducer.send'; pick one by its stable key:` with `1. clients/…/KafkaProducer.java:942  org.apache.kafka.clients.producer.KafkaProducer.send  SYMBOL:METHOD  (ProducerRecord<K, V> record)` → `rmap callers '…#KafkaProducer.send:SYMBOL:METHOD'` and `2. …:1061  …  SYMBOL:METHOD  (ProducerRecord<K, V> record, Callback callback)` → `rmap callers '…:dup2'`, then `hint: run one of the cursors above; a stable key names at most one symbol`; both printed cursors run as printed and answer `File: …KafkaProducer.java:942` / `:1061` (JSAH-C03 steps 1–2); the `Matches:` rows with the mis-split `METHOD:dup2` kind and the `use qualified name` hint are gone. Mechanism: `storage/src/candidate_rows.rs::symbol_row_by_stable_key` (one read by key carrying `signature`; `line_start` 0 → None), `daemon-runtime/src/dispatch_ambiguous_matches.rs` (one payload builder for the four `AmbiguousSymbol` sites; per-candidate `lookup` = `found` | `missing` | `read-failed` with reason), `rgr/src/commands/ambiguous_matches.rs::render_ambiguous_matches(cursor: Option<(&str, &str)>, data)` (`shell_quote_arg`-quoted cursors; no cursor line and an incomplete-listing hint when the command cannot be formed; anchors through `presentation::anchor`). Tests: rgr lib 1488 → 1497, daemon-runtime 892 → 895, storage 761 → 764, `explain_symbol` 24 → 25. Assurance: baselines INPUT-1 (d1fb5465), INPUT-2 (cbbbf43d), INPUT-3 (04b6e944); decisions D-JSAH-SCOPE-1, D-JSAH-CURSOR-1 (+Corr 1–2), D-JSAH-PLACEMENT-1 (+Corr 1–2), D-JSAH-STORAGE-READ-1, D-JSAH-READ-STATE-1 (+Corr 1), D-JSAH-INCOMPLETE-CURSOR-1 (+Corr 1–2); implementation review accepted at admission 3 cycle 1 (codex gpt-6-sol; builder claude-opus-5-5); records `docs/assurance/JAVA-SYMBOL-AMBIGUITY-HINT-1/`.

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

Each unresolved call shall carry a named category and basis; where a caller list (`callers`, `explain`'s Callers) renders a zero, the count of unresolved calls that name this symbol's short name shall be stated with its categories ("0 resolved callers; 2 unresolved calls name `Recover` (ambiguous: 2)"), and where a callee list (`callees`, `explain`'s Callees) renders a zero, the count of unresolved calls the symbol itself makes shall be stated with its categories, so a zero never reads as absence.

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
