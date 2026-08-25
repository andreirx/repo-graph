# EMBED-SEED-SPIKE-1 — can local embeddings seed orientation? (2026-08-23)

**Question.** An agent arrives with a *task* ("where does the backend fetch BNR exchange
rates?"), not a symbol name. `orient --focus` resolves only exact path / stable key / symbol name
(agent-orientation-contract § Focus Resolution), so today the anchor comes from grep. Can a
**local** embedding model, over **only what rmap already indexes** (files + symbols + source
spans), pick the right anchor neighbourhood — and does it beat lexical matching? Idea imported
from FRAKTAG (vectors as candidate generator → deterministic neighbourhood expansion; never the
answer, never inside the map).

**Setup (EXECUTED).** Corpus = rmap SQLite index of glamCRM from the retained smoke state root
(`smoke-runs/2026-08-23T18-38-37Z`): 598 non-test files, 3,363 symbols (CLASS/INTERFACE/METHOD/
FUNCTION/TYPE_ALIAS/ENUM/CONSTRUCTOR). Docs: (F) `path + first 60 lines`; (S) `path :: kind
qualified_name signature + doc_comment + ≤12 body lines` (FRAKTAG's metadata-prefixed chunk),
rolled up max-per-file. Model: LM Studio `text-embedding-nomic-embed-text-v1.5` (768-d, 84 MB,
local, OpenAI-compatible `/v1/embeddings`; `search_document:` / `search_query:` prefixes).
Baseline (L): tf-idf over the same file docs. 16 natural-language tasks with expert-chosen anchor
files (some tasks accept several). Script + raw results: `tools/embed-seed-spike/`.

**Result (OBSERVED).**

| method | hit@1 | hit@5 | hit@10 |
|---|---|---|---|
| (F) embeddings, file-level | 4/16 | **14/16** | 15/16 |
| (S) embeddings, symbol-level, max per file | 4/16 | 13/16 | 15/16 |
| (L) lexical tf-idf baseline | 2/16 | 8/16 | 9/16 |

Cost: 72 s to embed the whole corpus once on an M1 (cache keyed by sha256(model + doc text) →
incremental re-embed on change is per-file); query embedding ~ms; brute-force cosine over 4k
vectors is trivially fast. No network, no API key.

**Reading the misses honestly.** hit@1 is low mainly because glamCRM has two frontends
(`frontend/web`, `frontend/workspace`) and a dual backend (Java + TS serverless) — the #1 hit was
usually the *sibling* of the chosen anchor (e.g. `workspace/.../CognitoAuthAdapter.ts` vs
`web/.../CognitoAuthAdapter.ts`; `pdf-estimate-service.ts` for "where is the estimate PDF
generated", which is arguably the better anchor). Type-definition files (`shared/src/types/*.ts`)
often rank first for a domain term — a useful hub, and the deterministic graph (who imports this
type) gives the handler/service one hop away. Real miss: "where does the UI let a user edit an
existing offer" (F) — UI-page phrasing; symbol-level found `OfferDetailPage` at #3.

**Conclusion (INFERRED from the above).** Embedding seeding finds the right *neighbourhood* in
the top-5 on 14/16 tasks vs 8/16 for lexical — enough signal to justify a SPEC slice
(`EMBED-SEED-1`) under the certainty model as a **Layer-3 evidence-backed hint**: a separate
opt-in verb (e.g. `rmap seed "<task>"`) returning ≤5 candidates with score + provenance, each
rendered with its deterministic neighbourhood (module, imports, callers) — never inside
`orient`'s map, never an answer. Must-haves from FRAKTAG's failure modes: pin
`(model_id, dim, content_sha)` per vector and hard-fail on mismatch; recompute on content change;
fixed-formula ranking (no LLM reranker); local model only, absence degrades to "no hints", never
"no orientation". This is a VISION-level promotion (embeddings appear nowhere in VISION/ROADMAP
today) → needs operator ratification before the spec slice runs.


## Addendum — EMBED-CONCERN-SPIKE (2026-08-25): concept search + cross-module concern discovery

**Concept search (EXECUTED, same corpus/model):** "user login and authentication" → `auth.ts`,
`AccountCredentials.java`, `AuthContext.tsx` (all three subsystems, top-5); "email or notification
sending" → both email services (TS + Java); "role based permissions" → `user-role.ts` + `roles.ts`.
Honest miss: "database table schema definitions" → zod schemas (the SQL DDL files are outside the
indexed corpus — corpus-inclusion note for the spec).

**Concern discovery (EXECUTED):** K-means (K=24, cosine) over the file vectors; clusters spanning
≥2 deployable modules ranked by span, then intra-cluster cohesion. The top cross-module clusters
ARE glamCRM's real vertical concerns — invisible to import analysis (subsystems touch only over
HTTP): sales-targets (frontend↔serverless, cohesion .93), tenant-config/brand (.92, 15 files),
exchange-rate (.90), auth/Cognito (.89, 21 files across frontend+serverless). Conclusion
(INFERRED from the above): semantic cohesion is a usable Layer-3 instrument for
concern/seam-candidate discovery, complementary to the HTTP boundary links; ratified into scope
2026-08-25 (VISION § Semantic Seeding, use (iii)).
