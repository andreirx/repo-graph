# SEED-CHUNK-3 — a one-line field never outranks the code that does the work

Status: SPECIFIED (2026-09-06) · Track: audit round five, group E (human-ratified 2026-09-06;
follow-up named in ROADMAP after SEED-CHUNK-2). CODE slice, repo-graph-seed (document,
classify, rank, pass) + ts-extractor doc comments. Maturity: MATURE (find's seed tier).

## 1. Problem (ROOT-CAUSED — `docs/audits/2026-09-06-root-causes-v0.17.0.md` §E)

Outward surface: `rmap find "<concept>"`. On FRAKTAG, "where are conversations persisted to
disk" returns 6/10 rows that are `ConversationSession.id / startedAt / updatedAt` property
declarations (duplicated across an exported interface and a local copy in the UI package) and
ZERO write-path symbols — `createSession` (ConversationManager.ts:71), `logTurn` (:133),
`TreeStore`, `ContentStore` are absent. A `updatedAt: string;` line outranks the method that
persists the data.

Four stacked causes:
1. A TS interface property is a symbol chunk (`ts-extractor/src/extractor.rs:941-943,
   994-1027`), and the extractor hard-codes `doc_comment: None` for properties (:1023) — the
   only counter-signal is discarded.
2. PRIMARY: the embedded document is name-dominated. `build_chunk_document`
   (`repo-graph-seed/src/document.rs:25-56`) = qualified_name + doc + first 60 span lines.
   For `updatedAt` that is the entire text `ConversationSession.updatedAt\nupdatedAt:
   string;` — ~90% qualified name — while `createSession` is name + doc + 60 code lines with
   no lexical "persist/disk". The static mean-pooled model (model2vec) lets "conversation /
   session" dominate a 2-line document and averages it away over 60 lines of code.
3. SEED-CHUNK-2's demotion cannot reach properties: `is_callable_subtype`
   (`classify.rs:719-724`) covers FUNCTION/METHOD/CONSTRUCTOR/GETTER/SETTER only, and the
   demotion (`rank.rs:89-135`) sinks a decl only below an impl with the SAME qualified
   name — a property has no implementation counterpart.
4. The duplication is real source duplication (two `interface ConversationSession`).

Design gap since SEED-CHUNK-1's per-symbol chunks; not a regression.

## 2. Contract

1. **Field tier.** A chunk whose subtype is PROPERTY/FIELD (any language), or whose span is
   ≤ 1 line with no doc comment, ranks in a FIELD tier below every body-bearing chunk of its
   partition (same mechanism as decl demotion, keyed on the partition only, never on the
   qualified name). Properties stay IN the corpus — `find "sessionId field"` must still hit
   them; the tier applies within the ranked list, not at admission.
2. **The document carries the counter-signal.** TS property extraction keeps the property's
   doc comment (JSDoc / leading line comment) instead of `None`; `build_chunk_document`
   states in its header comment what proportion of a one-line chunk's document is its name,
   and the rank test below proves the tier compensates.
3. **Stored, not recomputed.** `seed_vectors` gains what the rank needs (subtype, span line
   count) via an additive migration in the SEED-CHUNK-2 style (pass.rs already holds
   `chunk.subtype` and `line_start/line_end`); pre-migration stores self-heal via the
   SeedCoordinator latch as migration 034 did. One re-seed transition, reported.
4. **Rendering states the tier.** Field-tier rows render `[field]` beside the existing
   `(decl)` / `[test]` labels so the reader sees why a high-similarity property sits low.
5. **Movement measured (deep-vertical).** The sc2-persist query on the retained FRAKTAG root:
   `createSession` / `logTurn` (or `TreeStore` / `ContentStore`) in the top 10, ≤ 2 `[field]`
   rows; the SEED-CHUNK-2 proofs (leveldb Recover decl-below-impl, retention 10/10, obsolete)
   byte-stable; ≥ 3 further concept queries across FRAKTAG / repo-graph / leveldb before/after
   with the verdict per query (helped / neutral / hurt) — a tier that hurts any of them STOPS.

6. **Acceptance restated after measurement (operator ruling `SC3-FIELD-DOD` = C, 2026-09-08).** The
   field tier was built and MEASURED: FRAKTAG's persistence query went from 7/10 one-line property rows
   to 0/10; the decl tier was restored (a build-0 defect field-tiered one-line declarations — found by
   measuring, fixed); the write-path methods' ranks improved (createSession 26 → 13, logTurn 150 → 79,
   TreeStore 85 → 46, ContentStore 81 → 43) but their SIMILARITY SCORES (0.27, 0.15, 0.20, 0.20) sit
   below the frozen 0.30 floor, so no tier can place them in the top 10. Diagnosis (one): DOCUMENT
   COMPOSITION — a method's chunk document is its qualified name + doc + first 60 lines of body, and
   `createSession`/`logTurn`'s bodies carry none of the query's words ("persist", "disk"); the tier
   is not the remaining cause. The literal top-10 names were the operator's PROXY for "helped"; the
   measured outward outcome is: the answer's top rows are body-bearing code, `ConversationManager`
   (the owning class, whose methods ARE createSession/logTurn) sits at rank 2, and `explain
   ConversationManager` (SYMBOL-IDENTITY-1) lists them — the agent is pointed at the right place, not
   misled. §2.5/§5 acceptance is therefore: 0 `[field]` rows in the FRAKTAG top 10; the owning
   persistence class in the top 3; the SEED-CHUNK-2 proofs byte-stable; the three control queries not
   hurt; the exact ranks of the four write-path symbols recorded before/after. The design cause is
   FILED, not papered over: SEED-DOCUMENT-1 (a method chunk's document carries its enclosing type's
   doc and a bounded identifier summary so a query's words can meet it; measured on this same query,
   with the floor unchanged) — its own root-caused slice.

## 3. Stop conditions

Frozen: `SEED_SIMILARITY_FLOOR = 0.30` and its calibration, the embedding model
(`minishlab/potion-code-16M-v2`), the is_test partition, the FORGET-vs-SEED invariants, wire
protocol. If the field tier needs a similarity re-weighting (a score change rather than a
tier) to meet §2.5, STOP + DECISION_REQUIRED with the measured alternative — never tune
scores to a query. STANDING HONESTY RULES. Unmet DoD → STOP + DECISION_REQUIRED. Never touch
the operator's real state root; every rmap call isolated. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit FIRST: `rank.rs` — a 0.47 PROPERTY chunk sorts below a 0.39 METHOD chunk in the same
  partition; a PROPERTY in the test partition still sorts within the test partition; a
  one-line documented constant with a doc comment is NOT field-tiered by the span rule alone
  (state the rule you chose). Extractor: a TS property with a JSDoc keeps it.
- Live proof (isolated FRAKTAG re-seed on the retained audit root or a fresh isolated index —
  FRAKTAG is small; NEVER re-index repo-graph twice for this): the §2.5 measurements verbatim.
- Gates recorded FIRST; chunked cargo; witness; dogfood-isolated.

## 5. Definition of done

Write-path methods outrank one-line properties on the FRAKTAG persistence query without
hurting the SEED-CHUNK-2 proofs or the three control queries; `[field]` renders; the tier
inputs are stored and self-heal; gates green.

CORPUS PATHS: FRAKTAG under ../FRAKTAG; leveldb at ../legacy-codebases/leveldb; repo-graph is
THIS repo.
