# EMBED-SEED-1: semantic seeding — a local-embedding candidate generator for task-to-anchor orientation — SPEC

Slice: EMBED-SEED-1
Status: **SPEC — RATIFIED 2026-08-25 (human; all-converged packet, D-ES-1..11).** IMPL authorized (EMBED-SEED-IMPL-1). The spec itself changed no production code, no `Cargo.toml`, no schema (SQL)
(packet STOP_CONDITIONS). This doc is the whole deliverable. It runs under **decision-review**:
the `## DECISIONS` section is the ratification surface; an IMPL slice follows only after the
operator ratifies. (The packet cites `docs/slices/decision-review-mode-1.md` for the marker
convention; that file does not exist in the tree at spec time — INFERRED that the intended
convention is the DECISION_REQUIRED-matrix + terminal DECISIONS section used by
`docs/slices/module-model-1.md` §7/§12 and `docs/slices/engine-consolidation-1.md` §6/§8, which
this doc follows.)

> ## REWORK BANNER — iteration 7 (2026-08-25): integrate into existing seams, delete the verb
>
> **The human ratified the MECHANICS and REJECTED the separate verb** (VISION amended, commit
> `a3a90ce`, `docs/VISION.md:149-160` — the binding bound now). Ratified: corpus (§3), state-root
> sidecar + pins (§4), background recompute with its own coordinator (§5), **D-ES-4 = option (a)
> local endpoint**. Rejected: the separate `rmap seed` verb — semantic candidates MUST surface as a
> **labeled fallback tier inside the EXISTING resolution seams** whose contract already returns a
> `candidates` array or a no-match. This rework:
> 1. **DELETES the `rmap seed` verb.** D-ES-1 → **SUPERSEDED-BY-HUMAN** (recorded, §DECISIONS, not
>    silently removed). §8 is rewritten as the **integration contract** (§8.0–§8.4): the tier fires
>    **only after every deterministic tier yields zero matches** (never reordering/diluting the
>    deterministic tiers; **ambiguous-with-exact-matches does NOT fire it**), enumerated per seam
>    against code.
> 2. **New decision cell D-ES-10** (seam enumeration + fire condition + integrated envelope) takes
>    over D-ES-1's ratification role; **D-ES-7's output-shape cells** are updated to the integrated
>    envelope. All other decisions (D-ES-2,3,5,6,8,9 + ratified D-ES-4) carry over UNCHANGED and are
>    **not re-litigated** (TD-015: the decision-review rerun challenges ONLY the changed/new cells).
> 3. **§9 doctor / §8.3 degradation** reference the seam integration, not the verb; §2 invariants
>    I1/I4 are refined for the integrated (no-match-only, additive-labeled) contract.
> 4. **The candidate no longer inlines its neighbourhood.** Amended VISION (as re-worded by OPERATOR
>    RULING 5, `docs/VISION.md:159-164`): each candidate carries *score + provenance + module/path*
>    directly and **names the deterministic follow-up SEQUENCE** — `explain <candidate>` for imports +
>    symbols, then one further `explain` on a listed symbol for callers (no single command yields
>    file-level callers) — so the old §8.2 inline caller-fold is **SUPERSEDED** (smaller design; the
>    neighbourhood is what the named sequence yields). §10/§11/§12 updated to match.
>
> Everything below the banner that a `⟨SUPERSEDED …⟩` marker does not touch is the ratified,
> unchanged mechanics. Change log for this rework: **§22**.
>
> ---

Revision: **iteration 11** — closes `.agent-manager/slices/EMBED-SEED-1/review-10.json` (two internal-contract
reconciliations, no scope change): (1) §12's `find` degraded-state acceptance criterion now requires the
**always-present `candidates: []` + labeled `summary`** (was: `candidates` key *omitted* — which contradicted
the ratified §8B.2/§8B.3/D-ES-11 contract); (2) §8B.1/§8B.2/§8B.3 reworded to state precisely that `find`
shares the §8 **substrate and degradation *causes/state taxonomy*** while its **DTO/rendering is intentionally
distinct** (own DTO, always-present `[]` + `summary`) — the earlier "identical/shared verbatim" and "only
cap/header differs" wording is corrected. Change log: **§27**.
Prior revision: **iteration 10** — **HUMAN DIRECTIVE 2 + SCOPE EXPANSION** (2026-08-25, VISION amended
`8adabca`, binding): the semantic substrate now serves **three** ratified uses, not just the §8
fallback tier. This iteration is **purely additive** (the §8 fallback contract is unchanged): it adds
**§8B — the `rmap find "<concept>"` affirmative concept-search verb** (use (ii), human-named, sharing
the ENTIRE §8 substrate — only the verb + envelope + one CLI/daemon/witness line are new), records
**D-ES-11** (the `find` contract) and D-ES-1's **second** human supersession (the human overrode its
rejection of a search-named verb — honesty mitigation is the *output*), adds the **§3.3 SQL/DDL
corpus-coverage limitation** (spike addendum), folds `find` into the **IMPL-1 milestone** (VISION:
uses (i)+(ii) ship together), and adds **use (iii)** — cross-module concern hints — as a **named
follow-on milestone `EMBED-CONCERN-1`** (§11, rendering-surface decisions deferred to its own slice).
Per TD-015 the decision-review rerun challenges **only** D-ES-11 + the use-(iii) milestone cut. Full
change log: **§25**. This iteration-10 pass additionally **closes `review-9.json`** (two code-truth
reconciliations, no scope change): the `find` verb now specifies its **own** response DTO carrying the
§8.2 candidate *fields* (not `FocusCandidate`) with an **always-present `candidates: []`** (§8B.2/§8B.3,
D-ES-11), and the §11 concern-hint milestone no longer claims the §7.2 query→file ranking is reused —
concern discovery has no query and ranks clusters, so its K-means/span/cohesion is spike evidence with
its ranking contract deferred to `EMBED-CONCERN-1`. Change log: **§26**.
Prior revision **iteration 9** — closed `.agent-manager/slices/EMBED-SEED-1/review-8.json`: one blocking
code-truth fix — the follow-up **second hop keys by NAME, not stable key** (hop 1's serialized
`ExplainSymbolItem` exposes `{ name, subtype, line_start }` only, no `stable_key`), so `explain
<symbol-name>` returns callers only when the name resolves uniquely, else the existing ambiguity
result; recorded as an accepted existing-surface limitation, not an `explain <file>` extension. Full
change log: **§24**. Iteration-8 change log at **§23**.
Prior revision **iteration 8** — closed `.agent-manager/slices/EMBED-SEED-1/review-7.json` (escalation) +
**OPERATOR RULING 5** (VISION amended `bf555ee`): the candidate names the deterministic follow-up
**sequence** (`explain <file>` → imports+symbols; a further `explain <symbol-name>` → callers), and the
four code-truth mismatches (empty `candidates` omitted; `limits` is `Vec<Limit>`; Group-B not-found is
`InvalidRequest`; §11 reconciled with ratified D-ES-4=(a)) are fixed to match code. History below is
retained verbatim.

Revision: **iteration 7 (pre-rework close, now historical)** — this revision closed `.agent-manager/slices/EMBED-SEED-1/review-6.json`.
Its four substantive items were applied **inline** (by the operator, per the packet OPERATOR NOTE):
(#1) the D-ES-9 A2 response contract now correlates vectors by `index` (unique permutation of `0..n`,
cardinality-checked) and rejects non-finite / zero-norm vectors (§ D-ES-9 A2 contract); (#2) the
sidecar body is an exact `bincode` versioned-DTO codec with fixed store/endpoint limits and rejection
behaviour (§4.3, § D-ES-9); (#3) the cross-machine ε guarantee is **removed** — ε=1e-5 is now a
non-guaranteeing near-tie *advisory* only, calibration deferred (§7.3); (#4) the milestone is
deep-vertical under **either** D-ES-4 outcome (option (b) gets its own complete vertical; the core is
never dormant, §11). Iteration 7 (this builder pass) additionally reconciles the doc-internal
statements those inline edits left stale — the §13 reproducibility summary and the §11 deferred-list
bullet + D-ES-4 RECOMMENDED note that still described the superseded "core built regardless"
milestone — and closes review-6 item 5 (D-ES-1 given explicit REWARD/RISK; the 49-member claim given
its `rust/Cargo.toml:19-74` line anchor). On top of review-5's two items (iteration 6), review-4's
two (iteration 5), review-3's three (iteration 4), review-2's five (iteration 3), review-1's five
(iteration 2), and review-0's seven (iteration 1). All change logs are at the end (§15 iteration 1,
§16 iteration 2, §17 iteration 3, §18 iteration 4, §19 iteration 5, §20 iteration 6, §21 iteration 7).

Track: **Semantic seeding** — VISION § Semantic Seeding (ratified 2026-08-24); ROADMAP
§ "Semantic seeding — ratified track". The four VISION bounds
(candidate-generator-never-answer; Layer-3 labelled; pinned `(model_id, dim, content_sha)`
hard-fail; local-only, degrade to "no hints") are **ratified and not re-litigated here** — this
spec resolves the *engineering* against the actual codebase.

Grounded in: `docs/spikes/2026-08-23-embed-seed-spike-1.md` + `tools/embed-seed-spike/spike.py`
(local 84 MB model, right neighbourhood in top-5 on **14/16** real glamCRM tasks vs **8/16**
lexical, fully offline).

Model (doc shape): follows `docs/slices/module-model-1.md` (problem → principle → target output →
root-cause/mechanism design against cited code → per-choice VISION defense → DECISION_REQUIRED
matrix → validation → smallest-design statement). Every claim about existing code cites
`file:line` against the working tree at spec time; the IMPL re-confirms before editing (lines
drift).

---

## 0. Evidence law (how to read the claims below)

- `OBSERVED` — read first-hand from source or a spike artifact, cited by `path:line`.
- `INFERRED` — concluded from cited code/docs, not directly executed.
- All `file:line` anchors are OBSERVED against the working tree at spec time unless marked
  INFERRED. The IMPL re-verifies each before depending on it (a tool's "all callers" is a subset
  until an AST/compiler query confirms — these anchors were found by `grep` + direct read, so they
  are a *verified-by-reading subset*, not a proof of exhaustiveness).

---

## 1. The problem (OBSERVED)

An agent arrives with a **task in natural language** — "where does the backend fetch BNR exchange
rates?" — not a symbol name. Today the only bridges from a task to an anchor are:

- `orient --focus` / `explain`, which resolve **only** an exact repo-relative path, an exact stable
  key, or an exact symbol name — deterministic precedence, no fuzzy match.
  - OBSERVED: `docs/architecture/agent-orientation-contract.md:62-70` (Focus Resolution: exact path
    → stable key → symbol name → else `resolved:false`).
  - OBSERVED (impl): `rust/crates/storage/src/agent_impl.rs:448` `resolve_path_focus`, `:519`
    `resolve_stable_key_focus`, `:938` `resolve_symbol_name`; declared on the port at
    `rust/crates/agent/src/storage_port.rs:632,:643,:721`.
- `grep` — lexical, and the spike measured it: tf-idf over the same file docs puts the right anchor
  in top-5 on only **8/16** tasks (`tools/embed-seed-spike/spike.py:112-119`; results table in the spike doc §Result).

So the agent that does not already know the symbol name must guess grep terms. The spike proved a
**local** embedding model over **only what rmap already indexes** (files + symbols + source spans)
puts the right *neighbourhood* in top-5 on **14/16** tasks — the missing bridge.

**What EMBED-SEED-1 is not.** It is not semantic search as an answer, not a reranker, not a new
map layer. Per VISION § Semantic Seeding (as amended 2026-08-25, `docs/VISION.md:149-160`) it is a
**candidate generator integrated into the existing resolution seams** — **not a separate verb**
(⟨SUPERSEDED: the original separate-`rmap seed`-verb framing is removed by the human directive⟩).
Semantic candidates surface **only** through the places whose contract already returns a
`candidates` array or a no-match — focus resolution in `orient`/`explain`; symbol lookup in
`callers`/`callees`/`path` — as an **additive, labeled fallback tier that runs only after every
deterministic tier produced nothing** (§8). Each candidate carries `score` + provenance + its
**module/path directly** and names the deterministic follow-up **sequence** on the existing
surfaces — `explain <candidate>` yields the file's imports + symbols; **callers are one further
`explain` on any symbol that command lists** (VISION amended 2026-08-25, `docs/VISION.md:159-164`:
a single command yielding file-level callers does not exist, and widening resolved `explain` for
everyone would break its byte-stable output). The embedding never enters `orient`'s
facts/signals, `map`, or `modules`.

---

## 2. Principle (the bounds, restated as invariants the IMPL must not cross)

Ratified in VISION § Semantic Seeding; repeated here as the invariants every decision below is
measured against:

- **I1 — Candidate generator, never answer (integrated).** The semantic tier returns ≤5 candidates
  as an **additive, labeled fallback inside the existing resolution seams** (§8), each candidate
  carrying `score` + provenance + module/path **directly** and **naming the deterministic follow-up
  sequence** that reaches its full `(module, imports, callers)` neighbourhood on the existing
  surfaces: the candidate already carries the owning **module**; `explain <candidate>` yields the
  file's **imports + symbols**; **callers are one further `explain <symbol-name>` on any symbol that
  command lists**, which returns callers only when that name resolves uniquely, else the existing
  ambiguity result — the follow-up keys by **name** (hop-1 output exposes no stable keys), an accepted
  existing-surface limitation, §8.2a (`docs/VISION.md:159-164`, amended 2026-08-25 — no single command
  yields file-level callers, and widening resolved `explain` would break its byte-stable output). The
  neighbourhood is obtained by running that sequence, never inlined. **No embedding-derived fact appears in `orient`'s
  resolved facts/signals, `map`, or `modules`** (`docs/VISION.md:156-158`): embedding candidates live
  **only** in the previously-empty no-match `candidates` list / symbol-not-found error `data`, never
  in a resolved result. (⟨SUPERSEDED: the iteration-≤7 "each candidate's inlined deterministic
  neighbourhood" is replaced by the amended VISION's named follow-up **sequence** — `explain` on the
  file for imports+symbols, then `explain` on a listed symbol for callers.⟩)
- **I2 — Evidence-backed hint, Layer-3, labelled.** Every candidate carries `score`,
  `source: "embedding"`, and `model_id`. Ranking is a fixed formula (cosine + deterministic
  tie-break); no LLM in the loop. The output speaks the reader's language (VISION § Labels), not
  our pipeline state.
- **I3 — Deterministic given its pins.** Every vector is pinned `(model_id, dim, content_sha)`;
  any mismatch is a **hard fail → degrade to "no hints"**, never a silent stale answer. Staleness
  recomputes from the content hash.
- **I4 — Local, optional, deterministic tiers never disturbed.** Local model only; no API key, no
  network egress to a third party. Absence of the model, or an empty/stale vector store, degrades to
  **"no hints"** — never to degraded orientation. **The deterministic resolution tiers are
  byte-unchanged** whether or not seeding exists: every *resolved* result, and every
  *ambiguous-with-exact-matches* result, is identical to today. Only the **previously-empty no-match
  `candidates` list (orient/explain) and the symbol-not-found error `data` (callers/callees/path)**
  gain a labeled additive fallback — and when seeding is unavailable those degrade to **exactly
  today's output plus one labeled line stating the fallback was unavailable and why** (§8.3). (⟨REFINED
  from the pre-rework "byte-unchanged" claim, now that the tier lives inside these seams: the
  *deterministic* tiers are byte-unchanged; the *no-match* branch gains a labeled, degradable addition.⟩)

---

## 3. CORPUS — exactly what is embedded (resolves packet item 1)

### 3.1 The population (one authoritative source, already indexed)

The corpus is drawn from the **current READY snapshot** of the target repo — the same tables the
spike read — with **no new extraction**:

- **Files.** `files` table: `rust/crates/storage/src/migrations/001-initial.sql:45-53` —
  `file_uid`(PK), `repo_uid`, `path`, `language`, `is_test`, `is_generated`, `is_excluded`.
  DTO mirror `rust/crates/storage/src/types.rs:340`.
- **Symbols** (deferred to symbol-level corpus, D-ES-5). `nodes` table: `rust/crates/storage/src/migrations/001-initial.sql:76-95`.
- **Per-item content pin.** `file_versions.content_hash`: `rust/crates/storage/src/migrations/001-initial.sql:59-70` (`content_hash
  TEXT NOT NULL` at :62; PK `(snapshot_uid, file_uid)` at :69). DTO
  `rust/crates/storage/src/types.rs:463-466`; staleness semantics doc `rust/crates/storage/src/types.rs:423`
  ("`content_hash` changes between snapshots, the file is stale"). The hash is
  **SHA-256(content_bytes).hex()[0..16]** — OBSERVED `rust/crates/repo-index/src/scanner.rs:66-74`
  `hash_content`. **This exact function is the one the embedding pass re-runs to close the
  source/snapshot race (§3.5).**

> **Name correction (name-vs-semantics).** VISION § Semantic Seeding writes the pin as
> `content_sha`. There is **no column named `content_sha`** — the real column is
> `file_versions.content_hash` (OBSERVED, `rust/crates/storage/src/migrations/001-initial.sql:62`; the value is a 16-hex SHA-256
> prefix, `rust/crates/repo-index/src/scanner.rs:66-74`). The IMPL uses `file_versions.content_hash` and treats "content_sha"
> as the VISION's generic name for it. Also: `files` has **no `snapshot_uid` column** — snapshot
> scoping lives on `file_versions`/`nodes` (the spike joined `nodes.snapshot_uid`,
> `tools/embed-seed-spike/spike.py:82`). The IMPL keys the per-file content pin off `file_versions` for the READY
> snapshot, not `files`.

### 3.2 What is embedded (the exact serialized document) — file-level for IMPL-1

The spike measured two doc formats; **file-level won** and is the recommended IMPL-1 corpus:

| doc format (spike) | build | hit@5 | source |
|---|---|---|---|
| **(F) file-level** | `path` + first 60 lines, one doc per file | **14/16** | `tools/embed-seed-spike/spike.py:76-79` |
| (S) symbol-level, max-per-file | `path :: subtype qualified_name signature + doc_comment + ≤12 body lines`, rolled up max-cosine per file | 13/16 | `tools/embed-seed-spike/spike.py:81-89` |

**The exact IMPL-1 document (byte-for-byte, matching the spike's (F)):**

```
search_document: {path}\n{body}
```

where — OBSERVED `tools/embed-seed-spike/spike.py:60-66,78-79`:

- `{path}` is the repo-relative `files.path`.
- `{body}` is source lines `1..=min(60, line_count)` joined by `\n` — i.e. `read_lines(path, 1,
  60)` = the first up-to-60 physical lines of the file's current working-tree content
  (`tools/embed-seed-spike/spike.py:60-66` slices `lines[max(a-1,0):b]` = `lines[0:60]`; a file shorter than 60 lines
  contributes all its lines, no padding).
- `search_document:` is the **nomic model's required document-role prefix** (`tools/embed-seed-spike/spike.py:79`); the
  query path uses `search_query:` (`tools/embed-seed-spike/spike.py:146`). Mixing roles degrades the model — the prefix
  is not decorative.
- **Truncation:** the serialized document is truncated to its **first 6 000 characters** before it
  is sent to the model — OBSERVED `tools/embed-seed-spike/spike.py:101` (`texts[i][:6000]`). This is a **character**
  cap (Python `str` slice = Unicode scalars). The IMPL cuts on a `char_indices` boundary (never
  mid-scalar) so the byte length is `≤` the byte length of 6 000 chars. The 6 000-char window is a
  fixed mechanism constant (not a ratification cell); it is the exact spike value that produced the
  14/16 result.

File-level is the smaller corpus (one vector per file, ~598 on glamCRM vs 3 363 symbol vectors),
the best hit@5, and needs only `files` + on-disk file content — no `nodes` read. Symbol-level is a
**named extension point** (D-ES-5), deferred: it wins the one real UI-phrasing miss the spike
recorded (`OfferDetailPage` at symbol-#3 vs file-miss — spike doc §Reading the misses) but costs a
5.6× larger store for +0 hit@5.

### 3.3 Exclusions (resolves "which files are excluded")

The `files` table already carries the exclusion flags; the corpus filter is a `WHERE`, **no new
classification logic**:

- **Tests** — `is_test = 0` (spike did the same: `tools/embed-seed-spike/spike.py:73-75`, plus a `/__tests__/` and `/e2e/`
  path guard). OBSERVED column `rust/crates/storage/src/migrations/001-initial.sql:50`.
- **Generated** — `is_generated = 0`. OBSERVED column `rust/crates/storage/src/migrations/001-initial.sql:51`.
- **Vendored / excluded** — `is_excluded = 0`. OBSERVED column `rust/crates/storage/src/migrations/001-initial.sql:52`. "Vendored" is
  whatever the scanner already flagged excluded (SCANNER-GITIGNORE-1 honesty) — the corpus adds no
  new vendoring heuristic.

So the IMPL-1 corpus population = `SELECT file_uid, path FROM files WHERE repo_uid=? AND is_test=0
AND is_generated=0 AND is_excluded=0`, joined to `file_versions.content_hash` for the READY
snapshot. This is a **contract decision** (D-ES-6) only because "should generated files be
seedable?" is a product call, not because the mechanism is unclear.

> **Recorded corpus-coverage limitation — SQL/DDL files (spike addendum, 2026-08-25).** The concept
> search in the EMBED-CONCERN-SPIKE addendum OBSERVED an honest miss: the query *"database table
> schema definitions"* returned zod schemas, **not** the SQL DDL files —
> *"the SQL DDL files are outside the indexed corpus"* (`docs/spikes/2026-08-23-embed-seed-spike-1.md:58-59`).
> The corpus is exactly whatever `files` already holds (the `WHERE` above), so any file kind the
> upstream scanner does not record in `files` is unreachable to **both** the fallback tier (§8) and
> `rmap find` (§8B). Whether `.sql`/DDL absence is scanner language-coverage or an exclusion flag is
> **not resolved in this SPEC** (INFERRED — not read from code here); it is recorded as a **named
> coverage limitation of the semantic surfaces**, not silently glossed. Widening the corpus to SQL/DDL
> is a **separate, evidence-gated change** to the upstream scan (out of this slice's scope); the
> honest posture until then is that semantic hints cannot point at schema-DDL files.

### 3.4 Artifact-family contract + cross-crate registration (resolves review-0 item 2, review-1 item 3)

**The persisted vector store IS an artifact family and carries a full contract — being schema-free
does not make it contract-free.** The authoritative model
(`docs/architecture/artifact-contract-model.md` "Core Principle": *the unit of architecture is the
artifact family, not the database table … a family may map to … a read model with no table*;
"Implementation Authority": the code registry in `rust/crates/artifact-contracts` implements the
model) requires **every** persisted family to have a contract, whether or not it has a SQL table.
The IMPL therefore registers a new family. The contract, in the model's own vocabulary
(`artifact-contract-model.md` §Truth Classes / §Refresh / §Identity / §Provenance / §Impact /
§Freshness / §Degradation), is:

| Contract field (`rust/crates/artifact-contracts/src/contract.rs:17-53`) | Value for `SeedVectors` | Why |
|---|---|---|
| `family` | `SeedVectors` (new enum variant, `rust/crates/artifact-contracts/src/family.rs:15-123`) | the unit is the family |
| `truth_kind` (type `TruthKind`, `rust/crates/artifact-contracts/src/truth_kind.rs:14`) | **`Inference`** (variant at `rust/crates/artifact-contracts/src/truth_kind.rs:58`; Layer 3, Class 4 "Hints/Inferences") | evidence-backed, heuristic, degradable; **never** Layer 0 |
| `refresh_policy` (type `RefreshPolicy`, `rust/crates/artifact-contracts/src/refresh.rs:10`) | **`MarkImpactedDeferRecompute`** (variant at `rust/crates/artifact-contracts/src/refresh.rs:49`) | an expensive hint; content-hash change marks the file's entry impacted, the background pass (§5) recomputes only changed files |
| `identity_policy` (type `IdentityPolicy`, `rust/crates/artifact-contracts/src/identity.rs:10`) | **`StableLogicalKey`** (variant at `rust/crates/artifact-contracts/src/identity.rs:20`) | per-entry logical key = `file_uid`; the body is pinned/discriminated by `content_hash` |
| `degradation_policy` (type `DegradationPolicy`, `rust/crates/artifact-contracts/src/degradation.rs:10`) | **`MayBeOmittedWithExplicitUnknown`** (variant at `rust/crates/artifact-contracts/src/degradation.rs:26`) | absence / pin-mismatch / staleness → explicit "no hints" state, **never** known-zero (§8.3) |
| `provenance_policy` (type `ProvenancePolicy`, `rust/crates/artifact-contracts/src/provenance.rs:10`) | **`DerivedFromLayer0Items`** (variant at `rust/crates/artifact-contracts/src/provenance.rs:26`) | each vector's basis is exactly one Layer-0 item — the file's `file_versions` row (path + `content_hash`) |
| `impact_policy` (type `ImpactPolicy`, `rust/crates/artifact-contracts/src/impact.rs:10`) | **`MarkImpactedOnRelevantLayer0Change`** (variant at `rust/crates/artifact-contracts/src/impact.rs:30`) | only the changed file's entry is impacted (per-file provenance), not the whole store |
| `freshness_tracking` (type `FreshnessTracking`, `rust/crates/artifact-contracts/src/freshness.rs:10`) | per-entry: `content_hash` == current READY `file_versions.content_hash` ⇒ `FreshnessState::Current` (`rust/crates/artifact-contracts/src/freshness.rs:72`); ≠ ⇒ `FreshnessState::Stale` (`…/freshness.rs:85`; excluded from ranking, counted by doctor §9) | the freshness discriminator is the pin itself |
| `classification_maturity` (type `ClassificationMaturity`, `rust/crates/artifact-contracts/src/maturity.rs:15`) | **`Experimental`** (variant at `rust/crates/artifact-contracts/src/maturity.rs:32`) at IMPL-1 (matures with the track) | honesty about a first cut. **Name-vs-semantics note:** the field's type has only `Stable`/`Provisional`/`Experimental` variants — there is **no** `prototype` variant (iteration 3 wrongly wrote "prototype", which is CLAUDE.md's *project* maturity-ladder word `prototype → mature → production`, not this enum). The assignable value for a first cut is `Experimental`; it corresponds to the project's "prototype" stage. |
| `layer_dependencies` | `[FileVersions]` | the coherence rule at `rust/crates/artifact-contracts/src/registry.rs:501-505` requires `DerivedFromLayer0Items` ⇒ non-empty deps |
| `description` | "Local-embedding seed vectors: per-file dense vectors for task→anchor candidate generation. Non-authoritative sidecar; Layer-3 hint." | — |

**Cross-crate audit to register a table-less family (corrected — review-1 item 3).** Iteration 1
wrongly called this "local to the `artifact-contracts` crate … a one-crate mechanical detail with
no cross-boundary blast radius." **That is false:** `ArtifactFamily` is matched exhaustively in a
second crate, `repo-index`, so adding `SeedVectors` compiler-forces edits **across two crates**, and
one existing test asserts a property a sidecar family cannot satisfy. The complete, OBSERVED touch
set the IMPL must resolve:

| Site | What breaks / must change on adding `SeedVectors` | Compiler-forced? |
|---|---|---|
| `rust/crates/artifact-contracts/src/family.rs:128-157` `table_name()` | exhaustive `match self`, returns a **non-optional** `&'static str` — a sidecar has no table (**the core friction**) | **yes** |
| `rust/crates/artifact-contracts/src/family.rs:159-…` `all()` | the `&[ArtifactFamily]` slice every registry iterator walks (`rust/crates/artifact-contracts/src/registry.rs:53-103`) must include the variant, or the family is silently absent from all coherence checks | no (silent omission) — must add by hand |
| `rust/crates/artifact-contracts/src/registry.rs:16` `get_contract()` | exhaustive `match` → forces the `SeedVectors` contract `const` to exist | **yes** |
| `rust/crates/artifact-contracts/tests/coherence.rs:265-279` `table_names_are_valid` | iterates `all_families()` asserting `table_name()` is **non-empty and space-free** — a table-less family fails this unless the test *and* the signature are amended | test fails at build |
| `rust/crates/repo-index/src/impact_propagation.rs:154-181` `family_to_table()` | a **second** exhaustive `match` (returns `Option`, already `None` for freshness-column-less families) → add `SeedVectors => None` | **yes** |
| `rust/crates/repo-index/src/compose.rs:3755-3822` copy-forward dispatch | matches families with a `_ =>` wildcard (**not** forced), but `SeedVectors` must **not** silently fall into the `NotImplemented` arm — it is exempt (its refresh is the sidecar embed pass, §5, not DB copy-forward) | no — explicit exemption |
| `rust/crates/repo-index/src/refresh_policy.rs:43-77` `COPY_FORWARD_FAMILIES` / `RECOMPUTE_FAMILIES` / `REINDEX_FAMILIES` | opt-in `const` arrays; `SeedVectors` is deliberately in **none** of them (sidecar, own refresh path) — a documented non-membership, not an omission | no — explicit exemption |

**The `table_name()` resolution (now a genuine cross-crate decision, not a local detail).** Because
`rust/crates/artifact-contracts/src/family.rs::table_name()` returns non-optional and `rust/crates/artifact-contracts/tests/coherence.rs:265` asserts it non-empty for
*every* family, a table-less family forces a choice with test blast radius: (i) change
`table_name()` to `Option<&'static str>` (`None` for sidecar families) and update the one caller
(`rust/crates/artifact-contracts/tests/coherence.rs:268`) + the test to skip `None`; or (ii) keep the signature and return a documented
non-table sentinel (e.g. `"<sidecar:seed_vectors>"`) that satisfies the non-empty/space-free test
but is never used as SQL. Option (i) is the honest shape (a sidecar genuinely has no table) and is
**recommended**; it is a small but real edit to the `artifact-contracts` public surface + its test,
so the IMPL records it in its task packet. What is **not** optional is that the family exists,
carries the contract above, and is wired through **both** crates' exhaustive matches with the two
refresh arrays explicitly exempting it.

**Still not a SQL schema change.** All of the above is Rust (enum variant, `const` contract, two
match arms, array/test edits) — **no** migration under `rust/crates/storage/src/migrations/`. The SPEC
STOP_CONDITION "no schema change" is honored; "contract-free" is not an available option under the
artifact model. §11 folds this cross-crate audit into the IMPL-1 vertical.

**This is NOT a SQL schema change.** Registering a family is Rust code in `artifact-contracts`
(an enum variant + a `const` contract); it adds **no** migration under
`rust/crates/storage/src/migrations/`. The SPEC STOP_CONDITION "no schema change" (SQL/DB shape) is honored;
"contract-free" is *not* an available option under the artifact model, so IMPL-1 adds the
registry entry as part of its vertical (§11).

### 3.5 Closing the source/snapshot race (resolves review-0 item 5)

**The hazard.** Indexing computes `file_versions.content_hash` from the source bytes read *during
the scan* (`rust/crates/repo-index/src/scanner.rs:194-205` — `read_to_string` → `hash_content`). The background embed pass
(§5) reads the working tree **later**. A file can change between the scan and the embed; a naive
pass would then store a vector of the *new* bytes under the *old* snapshot `content_hash` — a
Layer-3 hint masquerading as pinned-to-snapshot.

**The rule (IMPL-binding).** For each candidate file the pass:

1. reads the working-tree bytes it will embed;
2. computes `h = hash_content(bytes)` using the **same** function the scanner used
   (`rust/crates/repo-index/src/scanner.rs:66-74`, SHA-256 hex[0..16]);
3. **admits the entry only if `h == file_versions.content_hash`** for that `file_uid` in the
   current READY snapshot. On mismatch the working tree drifted from the snapshot → the file is
   **omitted** (not embedded, not stored); it is picked up on the next index once the snapshot
   catches up.
4. stores that verified `h` as the vector's pin.

Consequently the vector's stored pin **always** equals both the bytes that produced it *and* the
snapshot's recorded `content_hash`. A stale-body-under-fresh-pin state is unrepresentable.

**Conservative-but-correct note.** The embedded document is truncated to 6 000 chars / first 60
lines, but the pin is the **whole-file** `content_hash`. A change *outside* the embedded window
still changes the whole-file hash and therefore re-embeds the file. This recomputes slightly more
than strictly necessary and **never** serves a stale vector as current — the honest direction to
err. The pass is therefore idempotent on an unchanged tree and monotone under change.

---

## 4. STORAGE — where vectors live (resolves packet item 2, review-0 item 1, review-1 item 5; DECISION D-ES-2)

**Recommendation: (a) a per-repo sidecar file under the daemon STATE ROOT, beside the snapshot DBs,
using the warm-cache envelope format.** This is the packet's stated option (a "per-repo sidecar
file in the state root"), corrected from iteration 0 which placed it in the repo-local `.rgr/`
tree. Full matrix in `## DECISIONS` D-ES-2.

### 4.1 The state root and the sidecar path (OBSERVED)

The daemon state root is where per-repo snapshot DBs and the registry already live:

- `state_root_dir()` — `rust/crates/daemon-runtime/src/registry.rs:558-566`: `$RMAP_STATE_ROOT` if
  set, else `platform_data_dir()` → `repo_graph_platform_paths::data_dir()`
  (`rust/crates/daemon-runtime/src/registry.rs:568-573`; macOS `<home>/Library/Application Support/repo-graph/`,
  `rust/crates/platform-paths/src/dirs.rs:22-36`).
- `RepoRegistry::new()` wires `registry_path = state_root.join("registry.json")` and `db_dir =
  state_root.join("databases")` — `rust/crates/daemon-runtime/src/registry.rs:139-161`.
- Per-repo DB path: `allocate_db_path(canonical_path, db_dir)` = `db_dir.join("{hash16}.db")` where
  `hash16 = SHA-256(canonical_path)[0..8 bytes] → 16 hex` — `rust/crates/daemon-runtime/src/registry.rs:542-548`.
- `repo_uid` is a **random ULID** minted once at registration (`rust/crates/daemon-runtime/src/registry.rs:535-537`), **not** a
  path hash. It is therefore *not* deterministically derivable from a repo path without reading the
  registry — so the sidecar keys off the **same path hash the DB filename uses**, not `repo_uid`.

**The sidecar (IMPL-1):**

```
<state_root>/seed-vectors/<hash16>.vec
```

where `<hash16>` is **exactly `allocate_db_path`'s hash** of the repo's canonical path
(`rust/crates/daemon-runtime/src/registry.rs:542-548`). Rationale: it sits as a sibling family to `databases/<hash16>.db`, is
correlated to its snapshot DB by an identical, registry-free derivation, and is **safe-to-delete
non-authoritative app-data co-located with the snapshot DBs it derives from** (not because the state
root is structurally private — see the corrected locality note in §4.2). A dedicated `seed-vectors/`
subdirectory (not inside `databases/`) keeps the DB-orphan classifier
(`rust/crates/daemon-runtime/src/reclaim.rs:424-630`) unambiguous.

**Deletion / forget semantics.** The sidecar is **safe-to-delete**: a missing store degrades to
"no hints" and is rebuilt on the next index. `forget <repo>` and reclaim must delete
`<state_root>/seed-vectors/<hash16>.vec` alongside the repo's `<hash16>.db`/`-wal`/`-shm`
(the existing per-repo teardown, `rust/crates/daemon-runtime/src/reclaim.rs:346-385`). IMPL-1 adds the sidecar to that known-
artifact set so `forget` leaves no orphan and reclaim does not flag it as stray.

### 4.2 Why the state root, not the repo-local `.rgr/` (iteration-0 correction; locality claim corrected — review-1 item 5)

The warm cache lives repo-locally at `<project_dir>/.rgr/warm-cache/…`
(`rust/crates/daemon-runtime/src/livegraph_warm_cache.rs:44-55`). Iteration 0 mirrored that location. **Corrected:** the packet
requires the *state-root* option, and the state root is the consistent home for per-repo derived
app-data:

- **The snapshot DBs already live under the state root**, keyed by the *same* `allocate_db_path`
  hash the sidecar uses. Placing the vector sidecar beside them (a) keeps forget/reclaim uniform
  (one per-repo teardown handles `.db` + `.vec` together, §4.1) and (b) treats the vectors exactly
  as the equally-machine-local snapshot DBs are already treated. This — not any privacy property of
  the directory — is the reason.

- **Locality is NOT structural — corrected honesty (INFERRED claim removed).** Iteration 1 asserted
  the state root is "never synced or shipped." That is **not** established by the code: `RMAP_STATE_ROOT`
  accepts an **arbitrary** path (`rust/crates/daemon-runtime/src/registry.rs:558-566`), so an operator can deliberately locate the
  state root on synced/shared/removable storage. The honest, code-backed claims are narrower:
  - By **default** the state root is a platform app-data dir (`~/Library/Application Support/repo-graph/`
    on macOS, `rust/crates/platform-paths/src/dirs.rs:22-36`) that is not inside any repo working tree — so it does
    **not** travel with a `git`/`rsync` of the repo the way repo-local `.rgr/` does. This is a *default-
    location* fact, **INFERRED** as the common case, **not** an invariant.
  - **Correctness does not depend on where the state root lives.** The sidecar is **safe-to-delete and
    non-authoritative** unconditionally: a copied or synced `.vec` either still validates against its
    pins on the new machine (§4.3 header check) — in which case it is a correct cache — or it fails a
    pin/`schema_version`/`content_hash` check and is **discarded → "no hints"** (§8.3), then rebuilt on
    the next index. There is **no** stale-serving path even if an operator syncs the state root across
    machines. Cross-machine float differences (§7.3) are already **not** claimed as reproducible, so a
    travelled store creates no new determinism hazard — the worst case is a discard-and-rebuild.

  In short: the state root is chosen for **co-location with the snapshot DBs and uniform teardown**,
  and the design is **safe regardless of whether the operator keeps the state root machine-private**.

### 4.3 The sidecar shape (IMPL-1) — warm-cache envelope

Format modeled on the ratified warm-cache envelope (pure crate `repo-graph-warm-cache`):

- **Header (the full validation manifest — resolves review-2 item 4).** Modeled field-for-field on
  `CacheManifest` (`rust/crates/repo-graph-warm-cache/src/lib.rs:154-170`), which carries exactly seven fields —
  **every one is reproduced here so corruption validation is buildable, not implied:**
  - `magic` (`u32`) — file magic; a wrong value ⇒ not our file ⇒ discard.
  - `schema_version` (`u32`) — the sidecar format version, a **crate const** owned by the seed
    support unit (the analogue of warm-cache's `SCHEMA_VERSION` const, `rust/crates/repo-graph-warm-cache/src/lib.rs:60`; field doc at
    `rust/crates/repo-graph-warm-cache/src/lib.rs:158-160`); a bump ⇒ `SchemaMismatch` ⇒ discard.
  - `key` — the pin tuple, the analogue of `CacheKey` (`rust/crates/repo-graph-warm-cache/src/lib.rs:134-148`): `model_id`, `dim`,
    `repo_graph_version`. Any field differs from the current runtime/config ⇒ `KeyMismatch` ⇒
    discard (I3 hard-fail). (Here the **model itself is the producer**, so `model_id` occupies the
    role `ProducerFingerprint` plays for warm cache, `rust/crates/repo-graph-warm-cache/src/lib.rs:118-124` — one fingerprint field, the
    model id, not a name+version pair.)
  - `created_at` (`u64`, unix seconds) — **metadata only, never identity** (exactly as
    `rust/crates/repo-graph-warm-cache/src/lib.rs:163-165`); the seed unit does not read the clock, the caller supplies it.
  - **`content_length` (`u64`)** — the length in bytes of the opaque payload (the serialized body
    below); a stored length ≠ the actual payload length on disk ⇒ truncation/corruption ⇒ discard.
    (Field semantics per `rust/crates/repo-graph-warm-cache/src/lib.rs:166-167`.)
  - **`checksum` (`String`)** — the **hex-encoded SHA-256 of the opaque payload bytes** (exactly
    `rust/crates/repo-graph-warm-cache/src/lib.rs:168-169`, "Hex sha256 of the opaque payload bytes"); recomputed on load and compared, so
    any bit-flip in the body ⇒ discard. This is the same hash family (`hex` SHA-256) the content pin
    uses (§3.1), but it covers the **whole serialized body**, not one file.
- **Body (the opaque payload the `content_length`/`checksum` cover).** A `dim`-pinned vector table:
  for each admitted corpus file — `file_uid`, `path`, `content_hash` (the verified per-file pin,
  §3.5), and the `dim`-length `f32` vector, **L2-normalized** (`v / (‖v‖ + 1e-9)`, `tools/embed-seed-spike/spike.py:104`).
  **Codec (review-6 #2 — exact, so independent implementations cannot produce incompatible
  `.vec` files):** the body is a **`bincode` little-endian encoding of a versioned DTO**
  (`SeedVectorBodyV1 { entries: Vec<SeedVectorEntryV1> }`, each entry
  `{ file_uid: String, path: String, content_hash: String, vector: Vec<f32> }`, fields in exactly
  this declaration order — the same bincode envelope+payload discipline as warm-cache,
  `rust/crates/repo-graph-warm-cache/src/lib.rs:172-188`; any future field change bumps
  `schema_version` and older stores are discarded, never migrated). The body is serialized to bytes
  once; `content_length` = that byte count and `checksum` = SHA-256(those bytes), so integrity is
  byte-exact. **Store limits (rejection, not truncation):** header ≤ **64 KiB**; body ≤ **1 GiB**
  (a 160k-file monorepo at 768-dim ≈ 0.5 GiB — beyond the cap the load is rejected and the semantic
  fallback degrades to "vector store exceeds the seed budget — seeding declined", never a partial read).
- **Validation before use** (the exact order of `validate_manifest`, `rust/crates/repo-graph-warm-cache/src/lib.rs:891-915`): `magic` →
  `schema_version` → `key` (`model_id`/`dim`/`repo_graph_version`) → **`content_length`** (payload
  byte count matches) → **`checksum`** (recomputed hex SHA-256 of the payload equals the stored
  value). Any header/integrity mismatch ⇒ discard the whole store ⇒ "no hints" (I3/I4). Only after
  the manifest validates are body entries read; a per-entry `content_hash` mismatch ⇒ that file is
  stale ⇒ excluded from ranking (and counted in the doctor staleness line, §9).
- **Atomic publication (review-0 item 5).** The pass builds the entire new store in a temp file and
  publishes by atomic rename — `atomic_write` (`rust/crates/repo-graph-warm-cache/src/lib.rs:983`). A cancelled or superseded pass
  (§5.1) **never renames**, so a valid existing store can never be replaced by a partial one; the
  worst case is that the store stays at its previous (older but internally consistent) generation.

New pure logic (corpus build + envelope + cosine ranking, no I/O to the model) lives in a support
unit whose **crate-vs-module boundary is itself a ratification cell (D-ES-8)**; the model-runtime
seam is the `Embedder` port (§10). See §10 for the abstraction ledger.

---

## 5. REFRESH — when vectors are (re)computed (resolves packet item 2b; DECISION D-ES-3)

**Recommendation: recompute-on-content-change as a background, cancellable pass after every
index/refresh — the exact ENRICH-LIFECYCLE-1 shape.** Not on-demand at the first semantic-fallback
query. (⟨rework: "first `seed`" now means "the first no-match query that triggers the fallback tier";
there is no verb — the refresh mechanics are otherwise carried unchanged, D-ES-3.⟩)

### 5.1 The precedent to mirror (OBSERVED)

Auto-enrichment is already a background pass spawned after every successful write op:

- Spawn point: `rust/crates/daemon-runtime/src/dispatch.rs:2623` `spawn_auto_enrich(...)` inside
  `finish_write_with_maintenance` (`:2614`); retention chains after (`:2634`).
- Pass module: `rust/crates/daemon-runtime/src/enrich_pass.rs` — `spawn_auto_enrich`:1024,
  `run_auto_enrich`:1056, supersede/cancel via `EnrichCoordinator`:492 + `generations`:493, opt-out
  gate `auto_enrich_enabled`:138 (env `RMAP_AUTO_ENRICH`), no-toolchain honest skip `:23`.
  Detached-completion, batch-boundary cancellable — the exact properties the VISION wants.

**Cancellation contract for the embed pass.** The pass consults the cancel/generation token at
**batch boundaries** (the 32-doc batch, §8.4). On cancel or supersede it discards its temp file
and returns **without publishing** (§4.3) — a newer index's pass wins and the prior store remains
valid. This reuses the `EnrichCoordinator` generation mechanism (`rust/crates/daemon-runtime/src/enrich_pass.rs:492-493`); it does
**not** invent a second cancellation path.

### 5.2 Why background-at-refresh over on-demand-at-first-fallback

| | background at index/refresh (RECOMMENDED) | on-demand at first fallback query |
|---|---|---|
| First-fallback latency | ~ms (store already warm) — the token/wall-clock win the VISION monetizes | the first no-match query pays the whole embed cost (spike: ~72 s cold on glamCRM) synchronously — a no-match branch on `orient`/`explain` that blocks a minute fails the Protocol-Surface promise those commands make |
| Cancellation / write-safety | reuses `EnrichCoordinator` supersede + batch-boundary cancel; an incoming index preempts it | must build its own cancellation; a long synchronous embed on a read path contends with writers |
| Staleness | recompute-on-`content_hash`-change: only changed files re-embed; unchanged copy-forward by `content_hash` match | store can be arbitrarily stale until a no-match query triggers it; degrades I3 to "recompute lazily" |
| Cost when unused | one background pass per index; opt-out via an env flag like `RMAP_AUTO_ENRICH` | zero until first use — the one genuine advantage |

**Refresh unit:** the changed-file set. `file_versions.content_hash` deltas between the parent and
current snapshot decide which files re-embed; the rest copy their vector forward by `content_hash`
match (the sidecar body is keyed on it). This matches the `MarkImpactedDeferRecompute` /
`ReextractChangedInputs` posture (§3.4).

Opt-out mirrors `auto_enrich_enabled` (`rust/crates/daemon-runtime/src/enrich_pass.rs:138`): env `RMAP_SEED_VECTORS` (default-ON
is a DECISION cell, D-ES-3 — leaning **default ON** to match the ratified auto-enrich posture, but
flagged because embedding needs a local model that may be absent → the pass must **skip honestly,
never error**, when no model is reachable, exactly as enrich skips with no resolver toolchain,
`rust/crates/daemon-runtime/src/enrich_pass.rs:23`).

---

## 6. MODEL RUNTIME — how the local model runs (resolves packet item 3, review-1 item 4; DECISION D-ES-4 — DISTRIBUTION-LEVEL, NOT DECIDED HERE)

This is a **distribution-level** choice (new binary dependency + model distribution vs an external
process the operator runs). Per the packet and CLAUDE.md § Decision Autonomy (blast radius:
foundational/irreversible → stop and ask) it is presented as a DECISION_REQUIRED with risk/reward
and **not decided by the builder**. See `## DECISIONS` D-ES-4 for the full matrix. Summary:

- **(a) OpenAI-compatible local endpoint** the operator configures (LM Studio / Ollama; the exact
  setup the spike used — `tools/embed-seed-spike/spike.py:18` `http://localhost:1234/v1/embeddings`).
  - **Reward:** no heavyweight/model dependency and no model-distribution burden (contrast (b)); proven
    in the spike; trivially swappable model. Manifest footprint is light but not zero — a workspace-member
    line (D-ES-8) and, under the recommended std-library transport, no dependency edge (D-ES-9).
  - **Risk:** requires a running local server the operator starts; "local-only" (I4) holds *only if
    the endpoint is loopback* — the IMPL must **refuse a non-loopback URL** so I4 (no third-party
    egress) is enforced, not assumed (the `NonLoopbackRejected` error variant, §10); a stopped
    server = "no hints" (acceptable per I4); and the HTTP transport itself is an unresolved dependency
    cell (D-ES-9) — the workspace has no HTTP client today.
- **(b) Embedded ONNX runtime inside `rmapd`** with a bundled model.
  - **Reward:** self-contained; no external process; "local" is structural, not configuration.
  - **Risk:** a heavyweight new dependency (ONNX runtime + tokenizer) against the binary-first
    distribution principle (VISION § Distribution); an 84 MB+ model to distribute/version/sign
    (macOS notarization surface, MAC-2); larger attack/maintenance surface — a real concern for the
    safety-critical posture. `Cargo.toml` edits are a STOP_CONDITION for *this* spec, so (b) cannot
    even be prototyped under this slice.

### 6.1 Endpoint configuration inputs & loopback enforcement (D-ES-4 option (a); resolves review-1 item 4)

Iteration 1 both required the operator to *configure* an endpoint **and** claimed (§10) "no config
layer beyond one env opt-out" — a contradiction. Corrected: option (a) is **not** configuration-free;
it needs a small, explicit config surface. Consistent with the house pattern (env vars read via
`std::env::var`, exactly like `RMAP_AUTO_ENRICH` at `rust/crates/daemon-runtime/src/enrich_pass.rs:142` and `RMAP_STATE_ROOT` at
`rust/crates/daemon-runtime/src/registry.rs:559`), IMPL-1 reads **three** endpoint inputs — no config file, no new format:

| Env var | Meaning | Default | On absence |
|---|---|---|---|
| `RMAP_SEED_ENDPOINT` | full URL of the OpenAI-compatible `/v1/embeddings` endpoint | `http://127.0.0.1:1234/v1/embeddings` (the loopback-**literal** form of the spike's LM Studio endpoint `http://localhost:1234/v1/embeddings`, `tools/embed-seed-spike/spike.py:18`; written as `127.0.0.1` so the default passes the literal-IP allowlist below) | use default |
| `RMAP_SEED_MODEL_ID` | the model id the operator asserts the endpoint serves; becomes the store pin `model_id` (§4.3) and the query-time identity check (§7.1) | `text-embedding-nomic-embed-text-v1.5` (the id the spike *requested* of its endpoint, `tools/embed-seed-spike/spike.py:17,101`; **operator-asserted, not endpoint-verified** — see the note below) | use default |
| `RMAP_SEED_DIM` | the embedding dimension; becomes the store pin `dim`; a returned vector whose length ≠ this ⇒ `DimMismatch` (§10) | `768` (nomic-embed-text v1.5) | use default |

> **One model id, used everywhere — and honestly labelled operator-asserted (name-vs-semantics —
> resolves review-2 item 1, review-3 item 1).** The pinned / persisted / configured / reader-facing
> model id is the **single exact string `text-embedding-nomic-embed-text-v1.5`** — the identifier the
> spike *requested* of its OpenAI-compatible endpoint (`tools/embed-seed-spike/spike.py:17`,
> `MODEL = "text-embedding-nomic-embed-text-v1.5"`; sent in the request body at `:101`,
> `json={"model": MODEL, "input": …}`). This is the value written into the store pin `model_id`
> (§4.3), the value the query-time identity check compares (§7.1 `ModelMismatch`), the value in the
> `--json` output (§8.2), and the value the human label prints (§9). There is **no** separate short
> "display label": the underlying Hugging Face model is *named* `nomic-embed-text-v1.5`, but the
> endpoint (LM Studio) is addressed under the `text-embedding-…` id above. Introducing a label↔id
> split would add structure with no caller — rejected; one string is the smaller, honest design.
>
> **Provenance correction (review-3 item 1): this id is OPERATOR-ASSERTED, not endpoint-verified.**
> The spike does **not** prove the endpoint *served* this id: it sends `MODEL` in the request
> (`spike.py:101`) and consumes **only** `data[].embedding` from the response
> (`tools/embed-seed-spike/spike.py:103-105`, `for j, d in zip(idx, r.json()["data"]): … d["embedding"]`) — it never reads
> the response's top-level `model` field. So under option (a) the pin is exactly *the model the
> operator told us the endpoint serves* (`RMAP_SEED_MODEL_ID`), a Layer-4 operator assertion, **not**
> a fact rmapd measured. IMPL-1 narrows that residual where the wire allows it — see §7.1's
> **echoed-model check** — but cannot eliminate it for option (a). Only embedded runtime (b), where
> rmapd *is* the model, makes model identity structural (D-ES-4).

These are the `Embedder`-impl's construction inputs (they configure the one (a) implementation; they
do **not** add a general config subsystem). `RMAP_SEED_VECTORS` (the refresh opt-out, §5.2) is
separate and unchanged.

**Loopback enforcement (structural I4, not assumed — corrected to literal-IP-only + no-proxy,
review-2 item 2).** Iteration 2 admitted the *name* `localhost` to the allowlist and asserted it
"resolves to a loopback literal by contract." That is **not** guaranteed: `localhost` is resolved
through the OS resolver / `/etc/hosts` / NSS, which an operator or an attacker can re-point, and an
HTTP client that honours proxy environment variables (`HTTP_PROXY`/`HTTPS_PROXY`/`ALL_PROXY`) can
send even a `127.0.0.1` request off-host through a proxy. Both holes are closed structurally.
"Local-only" is enforced at `Embedder` construction by an **IP-literal allowlist with no name
resolution, plus a proxy-disabled, direct-connect client**:

1. Parse `RMAP_SEED_ENDPOINT` as a URL. The accepted **scheme set is fixed by the transport ratified
   in D-ES-9**: under the recommended std-library transport (D-ES-9 (a2), no TLS crate) the scheme must
   be `http`; under a TLS-capable HTTP client (D-ES-9 (a1)) `http` or `https` are both accepted. Any
   other scheme (e.g. `file`, `ftp`) ⇒ `NonLoopbackRejected` regardless of D-ES-9. (The spike's real
   endpoint is plain `http://…` loopback, `tools/embed-seed-spike/spike.py:18`, so the recommended
   http-only set costs no demonstrated case — see D-ES-9.)
2. The **host must parse as an IP literal** in the loopback ranges — IPv4 `127.0.0.0/8` (any
   `127.x.x.x`) or IPv6 `::1` (`Ipv4Addr`/`Ipv6Addr::is_loopback()` on the parsed literal). **Any
   host that is not an IP literal is rejected** (`NonLoopbackRejected`, §10) — including the *name*
   `localhost` and every other DNS name, *whether or not it would resolve to loopback*. We do **not**
   resolve the host, because (a) a name that resolves to loopback today can be re-pointed (hosts-file
   edit, resolver hijack, DNS rebinding) and (b) resolving is itself a network act; only a literal-IP
   check is a non-egressing, non-spoofable enforcement of I4. The default endpoint is already written
   in literal form (`http://127.0.0.1:1234/v1/embeddings`), so the out-of-box case passes; an operator
   who prefers `localhost` must write `127.0.0.1` — the rejection error message says exactly that.
3. **The (a) transport is proxy-free and direct-connect.** rmapd's outbound request to the endpoint
   **never consults proxy configuration** — it does **not** honour
   `HTTP_PROXY`/`HTTPS_PROXY`/`ALL_PROXY`/`NO_PROXY` — and connects to the parsed literal IP directly.
   This guarantees the request cannot be diverted off the loopback interface even if the operator's
   environment has a proxy configured. *How* this is achieved is fixed by D-ES-9: under the recommended
   std-library transport (a2) a raw `std::net::TcpStream` connects straight to the parsed loopback IP
   and *inherently* consults no proxy env (there is nothing to disable); under a client crate (a1)
   IMPL-1 MUST construct the client with proxies disabled (the crate's `no_proxy()`-equivalent, named in
   that slice's packet). **OBSERVED:** there is no existing outbound-HTTP client in the workspace to
   model on — the daemon is socket-based (Unix domain sockets) and the spike used Python `requests` —
   verified by a deterministic manifest scan (no `reqwest`/`ureq`/`hyper`/`rustls`/`native-tls` in any
   `rust/**/Cargo.toml`). The transport is therefore genuinely new code and is itself the D-ES-9
   ratification cell (dependency-graph edge vs std-library).
4. A rejected endpoint degrades to the "model unavailable" honest state (§8.3) — seeding is optional,
   orientation is unaffected. The refusal is a fact in doctor (§9), not a silent fallback.

**What this guarantees — and what it does NOT (review-3 item 2).** Literal-IP-only + proxy-disabled
constrains **rmapd's own outbound connection**: its *direct peer* is a loopback-literal IP, reached
un-proxied, so **rmapd itself sends no byte off the loopback interface** and a non-loopback endpoint
is refused before connect. That is the honest, structural guarantee. It does **NOT** prove the
process *listening* on that loopback port is a local embedding model rather than a **local forwarding
proxy** that itself egresses to a third party: a user-space listener on `127.0.0.1:1234` can relay
anywhere, and rmapd cannot see past its own socket. So for option (a), true end-to-end "no
third-party egress" reduces to **trusting the local endpoint's behaviour** — that residual is an
explicit risk of (a) (carried into D-ES-4). What IS structural for (a) is narrower and stated as
such: *rmapd's direct network peer is always a loopback listener.* Only embedded runtime (b), where
there is no listener and no socket, makes local processing structural. Note: whether `https` is
accepted at all is a D-ES-9 outcome — under a TLS-capable client (a1) `https` is allowed *only*
against a loopback IP literal (a locally-terminated TLS endpoint) and never widens the host allowlist;
under the recommended std-library transport (a2) `https` is not offered (no TLS crate), which is not a
loss for the spike's plain-`http` loopback endpoint. **Test impact (review-2 item 2):** the degraded-state tests assert that `localhost`, a
public IP, and a DNS name are each rejected with `NonLoopbackRejected` (not merely "unreachable"),
that `127.0.0.1`/`127.0.0.2`/`[::1]` are accepted, and that the constructed client reports
proxy-disabled even when `HTTP_PROXY` is set in the test environment. (No test can assert the
loopback listener is not a relay — that is the documented residual, not a testable property.)

**Builder note (non-binding, does not decide):** a **reversible seam** makes the (a)-vs-(b) choice a
soft one — IMPL-1 defines one internal `Embedder` port (§10) with a single (a) implementation, so the
endpoint-vs-embedded question is answerable later without re-architecting. That port is one of the
two abstractions this slice earns. Which implementation *ships by default* is the operator's
distribution call, ratified in D-ES-4.

---

## 7. DETERMINISM — pins, tie-breaking, and honest "reproducible" (resolves packet item 6 & review-0 item 6)

### 7.1 Pin enforcement points

- **Load-time (whole store):** header `model_id`, `dim`, `schema_version` validated before any
  vector is read (§4.3, `validate_manifest` `rust/crates/repo-graph-warm-cache/src/lib.rs:891-915`). Mismatch ⇒ discard ⇒ "no hints".
- **Per-item (staleness):** each entry's `content_hash` vs the current `file_versions.content_hash`;
  mismatch ⇒ that file excluded from ranking + counted in the doctor staleness line (§9).
- **Query-time (configured identity):** the query is embedded by the *same* configured `model_id`;
  if the configured model differs from the store's `model_id`, that is a whole-store mismatch ⇒ "no
  hints" (never mix a query vector from model X against document vectors from model Y — enforced by
  the `ModelMismatch` variant, §10). This compares two **operator-asserted** ids (the store's pin was
  written from `RMAP_SEED_MODEL_ID` at embed time; the query's from the same var at query time), so it
  catches a *config change between embed and query*, not endpoint lying.
- **Wire-time (echoed-model check — option (a), review-3 item 1).** The OpenAI `/v1/embeddings`
  response carries a top-level `model` string. When the endpoint returns one, IMPL-1 compares it to
  the pinned `model_id`: **present and ≠ pin ⇒ `ModelMismatch` ⇒ "no hints"** (a hard-fail, never
  rank). This is the *only* point at which model identity is checked against the endpoint rather than
  against operator config, and it is **conditional on the endpoint echoing `model`**. When the field
  is **absent** (an endpoint that omits it), the pin stays **operator-asserted** — IMPL-1 does **not**
  hard-fail on a missing echo (that would break conformant-but-terse endpoints, and the spike never
  established that all endpoints echo `model`), it records "model id operator-asserted (endpoint did
  not echo)" as an honest doctor fact (§9). This is the residual §6.1 names: option (a) can *falsify*
  a wrong id when the endpoint volunteers its own, but cannot *prove* a right one; only embedded
  runtime (b) removes the residual. (INFERRED that LM Studio/Ollama echo `model`: the spike did not
  read the field, so the presence path is stated as a *when-present* rule, not a guaranteed one.)

### 7.2 Tie-breaking (exact)

Ranking is cosine similarity = the dot product of L2-normalized vectors (`tools/embed-seed-spike/spike.py:104,151`). The
**sort key is `(-score, path_ascending)`** — descending score, ties broken by repo-relative path in
byte-lexicographic order — exactly the spike's `sorted(key=lambda x: (-score, path))`
(`tools/embed-seed-spike/spike.py:128-132`). No order jitter (architecture.md Rule 5).

### 7.3 What "reproducible" honestly means — the numeric tolerance (review-0 item 6)

The **vectors are NOT bit-reproducible across machines**: a different BLAS, model-runtime build, or
CPU-vs-GPU produces slightly different floats. The reproducibility claim is therefore split into
two precisely-scoped statements:

**(1) Within one store (the guarantee we make).** Given a fixed `.vec` sidecar and a query embedded
on the same machine/runtime, ranking is a pure function of `(-score_f32, path)` over the stored
`f32` vectors. It is **exactly reproducible** — same store + same query ⇒ byte-identical ≤5, same
order, every time. **No epsilon is needed or used inside a store**: even two genuinely near-equal
scores are broken deterministically by `path`. This is the reproducibility we ship; it is a pure
function of the stored `f32` vectors and does **not** depend on where the sidecar lives (§4.2).

**(2) Across re-embeds / machines: NOT guaranteed — at any margin (review-6 #3).** The spike ran
on ONE machine and measured NO cross-machine float drift, so this spec makes **no positive
stability claim** across independently-produced stores: relative order and top-5 membership may
differ between machines or re-embeds, full stop. What ships instead:

- **ε = 1e-5 is a non-guaranteeing near-tie ADVISORY only:** two candidates whose scores differ by
  ≤ ε are rendered as a near-tie in `--json` (`near_tie: true` on the pair) so a consumer knows the
  order between them carries no information. ε guarantees nothing about candidates farther apart.
- The labels and doctor **never assert cross-machine score equality or membership stability**; the
  output carries `source: "embedding"` + the model pin so no reader mistakes a `score` for a
  reproducible constant. Within one store, ranking is exactly deterministic (§7.3(1)).
- If a bounded cross-machine claim is ever wanted, that is a **separate measured calibration
  slice** (embed the same corpus on ≥2 machines, measure drift, ratify the envelope) — deferred,
  and nothing in IMPL-1 depends on it.

---

## 8. INTEGRATION CONTRACT — the semantic fallback tier inside the existing seams (resolves packet item 4; DECISIONS D-ES-10, D-ES-7)

> ⟨SUPERSEDED: the entire pre-rework §8 "CLI CONTRACT" for a standalone `rmap seed` verb — its verb
> dispatch (`main.rs`/`dispatch.rs` `"seed"` arm), its dedicated envelope, and its inlined
> neighbourhood/caller-fold — is removed by the 2026-08-25 human directive. No new verb, no new
> dispatch arm, no `commands/seed.rs`. The tier is wired into the seams enumerated below.⟩

### 8.0 The integration model (D-ES-10)

The semantic tier is a **fifth, non-deterministic fallback** appended to the existing deterministic
resolution ladder. It obeys three hard rules, all measured against I1/I4 (§2):

- **Fires last, only on total deterministic failure.** It runs **iff every deterministic tier
  produced zero matches** — i.e. the seam's `no-match` branch (Group A) / the
  `SymbolResolveError::NotFound` branch (Group B, surfaced **today** as an `InvalidRequest` error,
  §8.1). It **never**
  reorders, replaces, or dilutes a deterministic tier, and **never fires on ambiguity**: an
  ambiguous result already *has* exact matches, so the deterministic `candidates`/`AmbiguousSymbol`
  path is returned unchanged and the embedding is not consulted.
- **Additive and self-labeling.** It only ever *populates a field that was empty on no-match* — it
  adds nothing to a resolved or ambiguous result. Every semantic candidate carries
  `source: "embedding"` + `model_id` (I2), so no consumer can mistake it for a deterministic match.
- **Degrades to exactly today's output.** No vectors / model down / pins mismatch ⇒ the seam returns
  **byte-identical to today** plus **one labeled line** stating the fallback was unavailable and why
  (§8.3). Seeding is never on the critical path of a deterministic answer.

The query string embedded is **the seam's own resolution input** (the `focus`/`target`/`symbol`
argument the agent already typed) — no new argument, no new verb. It is embedded with the
`search_query:` role prefix (§3.2) and scored by brute-force cosine over the store (§7.2), top-5.
(Honest note: that input is often a symbol/path *guess* rather than a full NL task; the spike's
queries were likewise short phrases (`tools/embed-seed-spike/spike.py:120-146`), so a guess is still
a usable query — but the candidates are labeled Layer-3 hints, never asserted answers.)

### 8.1 The seams, enumerated against code (D-ES-10)

Two seam-groups already return the two contract shapes the VISION names — a `candidates` array
(orient/explain) and a no-match/not-found (all five). Each row gives the **fire point** (the exact
deterministic-zero branch), the **do-NOT-fire point** (the ambiguous branch, left untouched), and
the **carrier** the tier additively populates.

**Group A — `orient` / `explain` focus resolution (structured `Focus` envelope; the tier populates
the previously-empty `candidates`).** Contract: `docs/architecture/agent-orientation-contract.md:62-86`
(precedence: exact path → stable key → symbol name → `resolved:false`/`no_match`; ambiguous ⇒
bounded `candidates` ≤5). DTO: `Focus` + `FocusCandidate` at `rust/crates/agent/src/dto/envelope.rs:55-121,174-210`.

| Seam | Fire the tier at (deterministic-zero) | Do NOT fire (ambiguous — exact matches exist) | Carrier populated |
|---|---|---|---|
| `orient <focus>` | `rust/crates/agent/src/orient/mod.rs:251-256` (`resolve_symbol_name` len `0 =>`), + the defensive no-context `no_match`s `:214`,`:273`; all route through `build_no_match_result` `:369` (`Focus::no_match`, empty `candidates`) | `rust/crates/agent/src/orient/mod.rs:278-287` `build_ambiguous_result` (`:321`; multiple exact SYMBOL matches → `Focus::ambiguous`) | `Focus.candidates` (was `[]` on `no_match`) + a labeled `limits` line |
| `explain <target>` | `rust/crates/agent/src/explain/mod.rs:187` (`0 =>`), + `:154`,`:204`; all route through `build_no_match` `:259` (`Focus::no_match`) | `rust/crates/agent/src/explain/mod.rs:208-223` (`Focus::ambiguous`, `:223`) | `Focus.candidates` + a labeled `limits` line |

**Group B — `callers` / `callees` / `path` symbol lookup (JSON-RPC *error*, NOT a `candidates`
envelope; the tier rides the not-found error's additive `data`).** The symbol resolver is a sum type
`SymbolResolveError { NotFound, Ambiguous(keys), Storage }` (`rust/crates/storage/src/queries.rs:568`;
method `resolve_symbol` `:628`). The daemon turns `NotFound` into an `invalid_request("symbol not
found: …")` error and `Ambiguous` into an `ambiguous_symbol` error carrying `matches` `data`; the CLI
renders both in `handle_daemon_error` (`rust/crates/rgr/src/commands/graph.rs:98-139`: `RepoNotFound`
`:106`, `AmbiguousSymbol` `:109-128`, generic `else` `:129-131`).

| Seam | Fire the tier at (`NotFound`) | Do NOT fire (`Ambiguous`) | Carrier populated |
|---|---|---|---|
| `callers <symbol>` | `rust/crates/daemon-runtime/src/dispatch.rs:1268-1272` (`SymbolResolveError::NotFound → invalid_request("symbol not found")`) | `:1274-1279` (`ambiguous_symbol`) | the not-found `ErrorDetail`'s `data` gains `semantic_candidates` + a labeled hint |
| `callees <symbol>` | `rust/crates/daemon-runtime/src/dispatch.rs:1438` (`NotFound`) | `:1445` (`ambiguous_symbol`) | same |
| `path <from> <to>` | `rust/crates/daemon-runtime/src/dispatch.rs:2479-2483` (`from` NotFound) / `:2502-2506` (`to` NotFound) | `:2485-2490` / `:2508-2513` (`ambiguous_symbol`) | same, on whichever endpoint failed |

**Why Group A and Group B carry the tier differently — and why that is honest, not two contracts.**
Group A's contract *already is* "a `candidates` array on no-match", so the tier simply fills it
(additive fields, below). Group B's contract *already is* "an error on not-found" (no `candidates`
array exists to reuse); reusing it means the semantic candidates ride the **error's `data`** — the
same additive-`data` mechanism `ambiguous_symbol` already uses for its `matches`
(`rust/crates/daemon-runtime/src/dispatch.rs:151` `parse_ambiguous_matches` → `ErrorDetail`). In both
groups the rule is identical: *the deterministic outcome is unchanged; a labeled additive field is
attached.* A Group-B `callers` query asks about a **symbol**, but the file-level store (D-ES-5) can
only answer with **files** — so the Group-B hint is explicitly *"no such symbol; here are files
semantically near your query — open one and re-run"*, never *"here are the callers"*. That weaker
fit is why Group B is a **named, deferred cut** in the milestone (§11 / D-ES-10), with Group A the
smallest deep-vertical.

### 8.2 The integrated candidate envelope (D-ES-7, updated for the seam integration)

Each semantic candidate carries **score + provenance + module/path + a named deterministic
follow-up command** — and **not** an inlined neighbourhood. This is the amended VISION bound
(`docs/VISION.md:159-164`): *"Each semantic candidate carries score + provenance + its module/path,
and names the deterministic follow-up (`explain <candidate>`) from which the full neighbourhood is
reachable — module and imports directly; callers one further `explain` away on any symbol it
lists."* Concretely, against code (verified below): the candidate **directly** carries the owning
`module` (+ `path`); running its `next` = `explain <file-stable-key>` yields the file's **imports +
symbols list** (`explain_file`, `rust/crates/agent/src/explain/mod.rs:543-589`); and the **callers**
are one further `explain <symbol-name>` on any symbol that list names. The hop-1 output exposes
symbol **names only, not stable keys** — `ExplainSymbolItem` carries `{ name, subtype, line_start }`
(`rust/crates/agent/src/dto/signal.rs:783-787`), built with exactly those three fields at
`rust/crates/agent/src/explain/mod.rs:574-581` — so hop 2 keys the follow-up **by name** and resolves
through the existing symbol-name path (`resolve_symbol_name`,
`rust/crates/storage/src/agent_impl.rs:938`; `explain_symbol` → `find_symbol_callers`,
`rust/crates/agent/src/explain/mod.rs:324`). That name resolution is **not guaranteed unique**:
`resolve_symbol_name` returns `Vec<AgentFocusCandidate>` (`:942`, `LIMIT 5`), so hop 2 yields callers
**only when the name resolves to exactly one symbol**; a name shared by several symbols returns the
existing **ambiguity** result (a `candidates` list), identical to `explain <name>` today
(precedence + ambiguous ⇒ candidates: `docs/architecture/agent-orientation-contract.md:62-86`). This
is an **existing-surface limitation** of the follow-up sequence, recorded as-is — the tier does **not**
extend or widen resolved `explain <file>` to close it (I4, byte-stable output inviolate). The full
`(module, imports, callers)` neighbourhood is thus obtained by **running that sequence** (with hop 2's
name-resolution honesty above) — the tier stays a pure candidate generator and inlines no
embedding-adjacent aggregation.

> **Code-truth note (name-vs-semantics — resolves review-7 blocking item).** No single existing
> command yields file-level callers: `explain_file` emits identity + imports + symbols with
> `module_path: None` and **no** callers (`rust/crates/agent/src/explain/mod.rs:527-589`); callers
> exist only per-symbol via `explain_symbol` (`:324`). So the amended VISION's "full neighbourhood"
> is a **two-hop sequence**, not one command; the candidate's own `module` field covers "module
> directly" (the owning module is NOT re-derivable from `explain <file>`). Widening resolved
> `explain <file>` to aggregate callers was explicitly rejected by the operator (it would break the
> byte-stable deterministic `explain` output — I4).
>
> **Code-truth note (hop-2 keying — resolves review-8 blocking item).** Hop 1's serialized symbol
> list exposes **names, not stable keys**: `ExplainSymbolItem` is `{ name, subtype, line_start }`
> (`rust/crates/agent/src/dto/signal.rs:783-787`), constructed with only those fields
> (`rust/crates/agent/src/explain/mod.rs:574-581`). So hop 2 can only be `explain <symbol-name>`, and
> `resolve_symbol_name` (`rust/crates/storage/src/agent_impl.rs:938-970`) returns a `Vec` — a name
> may match several symbols. Hop 2 therefore returns callers **only on a unique name**; otherwise it
> returns the existing deterministic **ambiguity** result. The spec does not claim full deterministic
> caller reachability from a file explain; this is an accepted existing-surface limitation of the
> follow-up sequence, not a defect to be fixed by widening resolved `explain <file>` (I4).

**Group A (`orient`/`explain`) — the tier fills the previously-empty `Focus.candidates`.** The
existing `FocusCandidate` shape (`{ stable_key, file, kind }`,
`rust/crates/agent/src/dto/envelope.rs:54-59`) is **reused with additive optional fields**, populated
**only** for a semantic fallback candidate and `skip_serializing_if` absent otherwise (so a
deterministic *ambiguous* candidate is byte-identical to today).

> **Two code-truth constraints the IMPL must satisfy (named IMPL work, not "unchanged code").**
> (1) `FocusCandidate` currently derives `PartialEq, Eq` (`rust/crates/agent/src/dto/envelope.rs:54`);
> a `score` field is a float, which **cannot** satisfy `Eq`. Adding it forces the IMPL to drop the
> `Eq` (and adjust `PartialEq`) derive on `FocusCandidate`, or carry the score as a non-float (e.g.
> fixed-point) — a small, real edit to the boundary DTO, recorded in D-ES-7/D-ES-10.
> (2) The `limits` entry needs a **new `LimitCode` variant** — the enum is closed and exhaustively
> matched with a fixed `summary()` (`rust/crates/agent/src/dto/limit.rs:35-187`), and no
> semantic-fallback code exists today. IMPL adds `SemanticFallback` (candidates present) and
> `SemanticFallbackUnavailable` (degraded, §8.3) — new variants + `as_str`/`summary` arms; the exact
> count/names are a local mechanism decision bounded by D-ES-7. The reader-facing per-situation text
> rides `reasons: Vec<String>`; `summary` stays the fixed contract string.

```jsonc
{
  "schema": "rgr.agent.v1",
  "command": "orient",                 // or "explain" — the SEAM's command, not a new verb
  "repo": "glamCRM",
  "snapshot": "…",
  "focus": {
    "input": "where does the backend fetch BNR exchange rates?",
    "resolved": false,
    "reason": "no_match",              // UNCHANGED: still a deterministic no-match
    "candidates": [                    // was [] on no_match; now the labeled semantic fallback
      {
        "stable_key": "glamCRM:serverless/.../bnr-service.ts:FILE",
        "file": "serverless/packages/backend/src/services/bnr-service.ts",
        "kind": "FILE",
        // ── additive, semantic-only fields (absent on deterministic ambiguous candidates) ──
        "source": "embedding",
        "model_id": "text-embedding-nomic-embed-text-v1.5",
        "score": 0.71,
        "module": "backend/services",  // owning module — carried DIRECTLY (lightweight locator, aggregate_file)
        "next": { "cmd": "explain", "args": ["<stable_key>"], "cwd": "<repo_root_abs>" }
      }
    ]
  },
  // `limits` is Vec<Limit> (envelope.rs:326) — objects, NOT strings. `summary` is a fixed lookup
  // from `code` (limit.rs:136-186); the per-query detail rides `reasons`. This entry needs a NEW
  // `LimitCode` variant (see the IMPL-work note below).
  "limits": [
    {
      "code": "SEMANTIC_FALLBACK",
      "summary": "No exact match. The candidates below are Layer-3 embedding hints, not resolved facts; open one and re-run.",
      "reasons": [
        "5 candidates (model text-embedding-nomic-embed-text-v1.5); run each candidate's `next` (explain <key>) for its imports + symbols, then explain a listed symbol for callers"
      ]
    }
  ]
}
```

- `candidates` capped at **≤5** (VISION bound). `score` is the cosine; `source`/`model_id` on every
  semantic candidate (I2). Ties → path order (§7.2). `reason` stays `no_match` — the candidates are
  explicitly labeled Layer-3 hints, not a resolution; the `limits` line and each candidate's
  `source:"embedding"` are the discriminators from a deterministic result.
- A candidate whose stored path no longer resolves against the current snapshot (a stale vector) is
  **dropped** from the list (honest — same admission check as the follow-up would apply), via
  `resolve_path_focus` (`rust/crates/storage/src/agent_impl.rs:448`, declared
  `rust/crates/agent/src/storage_port.rs:632`), the same resolver the seams already use.

**The `next` follow-up is a single structured, executable command (not a rendered string).** It uses
the **real** CLI syntax — `explain <stable_key>` (one positional, `rust/crates/rgr/src/commands/orient.rs:440-451`),
or `orient --focus <stable_key>` (`:95-118`) — with an explicit `cwd` = the absolute repo root the
seam resolved, because both `explain` and `orient` resolve the repo from the current working
directory (`orient` `:160-172`, `explain` `:488`), not from an argument. Running it yields the
file's deterministic **imports + symbols** on the existing surface; the **callers** are one further
`explain <symbol-name>` on any symbol that output lists (the follow-up **sequence**, §8.2a
below — `next` names and pre-structures only the first hop; the second hop can carry only a symbol
**name** (hop-1 output exposes no stable keys, `signal.rs:783-787`) and is discovered after hop 1
runs, so it is not pre-structured). Because hop 2 keys by name, it returns callers only when that
name resolves uniquely, else the existing ambiguity result (§8.2 hop-2 code-truth note). The
candidate already carries the owning `module` directly.
Human mode renders `next` as `(cd <repo_root> && rmap explain <key>)`.

**Group B (`callers`/`callees`/`path`) — the tier rides the not-found error's additive `data`.** No
`candidates` array exists on this seam, so the same candidate objects (minus `kind`, which is
FILE by construction) are attached under `data.semantic_candidates`, alongside a labeled
`data.hint`, on the **existing** `symbol not found` error — the error, exit code, and message are
otherwise unchanged:

```jsonc
// error response for: rmap callers fetchBnrRates   (symbol not found)
{
  "error": {
    "code": "InvalidRequest",                       // ACTUAL current code (see note) — deterministic outcome UNCHANGED
    "message": "symbol not found: fetchBnrRates",
    "data": {
      "semantic_candidates": [                        // additive, labeled Layer-3
        { "stable_key": "glamCRM:…/bnr-service.ts:FILE",
          "file": "serverless/.../bnr-service.ts", "score": 0.71,
          "source": "embedding", "model_id": "text-embedding-nomic-embed-text-v1.5",
          "module": "backend/services",
          "next": { "cmd": "explain", "args": ["glamCRM:…/bnr-service.ts:FILE"], "cwd": "<repo_root_abs>" } }
      ],
      "hint": "no such symbol; these files are semantically near your query — open one, then re-run callers on a symbol inside it"
    }
  }
}
```

> **Code-truth note (error class — resolves review-7 observed item 3).** The `symbol not found`
> response uses **`ErrorCode::InvalidRequest`** *today*, not a `SymbolNotFound` code:
> `dispatch.rs:1268-1272` calls `ErrorDetail::invalid_request(…)`
> (`rust/crates/daemon-transport/src/envelope.rs:204-205`), which sets `ErrorCode::InvalidRequest`
> (`:113`), serialized `"InvalidRequest"` (`:164`) — there is **no** `SymbolNotFound` variant in the
> enum (`:108-174`). The semantic tier rides the **existing** error's additive `data` — code,
> `message`, and exit unchanged — so no error-code change is required to ship it. Introducing a
> dedicated `SymbolNotFound` code (arguably the honest class) is a **separate, optional IMPL edit**
> (new `ErrorCode` variant + `as_str` arm, exhaustively matched) named as such — **not** "unchanged
> code" — and is out of scope for the Group-A deep-vertical (Group B is deferred, §11).

The Group-B hint is explicitly *file-level and does not name callers* (§8.1): the file-level store
cannot answer a symbol-granularity query directly. Group B is the **deferred cut** (§11); Group A is
the smallest deep-vertical.

#### 8.2a What the `next` command yields — the deterministic neighbourhood surfaces (reference)

> ⟨SUPERSEDED as an *inline* candidate field; RETAINED as the definition of what a candidate's
> `next` follow-up command deterministically returns.⟩ The pre-rework §8.2 inlined this
> `(module, imports, symbols, callers)` fold into every candidate (including the "deterministic
> file-level caller fold", §8.2 old step 5). The amended VISION replaces the inline object with the
> named `next` command; the fold is **no longer built** (its removal is the main simplification of
> this rework — see §10/§11). The surfaces below are what `rmap explain <stable_key>` /
> `rmap orient --focus <stable_key>` already return today, unchanged by this slice:

The full neighbourhood is a **two-hop deterministic sequence**, all surfaces **today, unchanged by
this slice** — the candidate carries `module` directly, hop 1 (`next`) yields imports + symbols, hop
2 yields callers:

- **`module`** (on the candidate, directly — NOT from `explain <file>`) — owning module,
  `aggregators::module_summary::aggregate_file` (`rust/crates/agent/src/orient/file.rs:72`), computed
  at candidate-render time. `explain_file` itself sets `module_path: None`
  (`rust/crates/agent/src/explain/mod.rs:538`), so the owning module is not re-derivable from hop 1 —
  it must be carried on the candidate.
- **`imports`** (hop 1 = `next` = `explain <file-stable-key>`) — `find_file_imports(snapshot_uid,
  path)` → distinct target files (`rust/crates/agent/src/storage_port.rs:794`), ordered as
  `explain_file` orders them (`rust/crates/agent/src/explain/mod.rs:544-548`,
  `ordering::sort_explain_imports`).
- **`symbols`** (hop 1, same command) — `list_symbols_in_file`
  (`rust/crates/agent/src/storage_port.rs:777`), ordered `line_start` ASC → `name` → `stable_key`
  (`rust/crates/agent/src/ordering.rs:148-155`; `rust/crates/agent/src/explain/mod.rs:570-572`). The
  `stable_key` term is an **internal ordering tiebreak only** — it is **not serialized**: the emitted
  `ExplainSymbolItem` carries `{ name, subtype, line_start }` (`rust/crates/agent/src/dto/signal.rs:783-787`),
  so this list supplies symbol **names** for hop 2, never stable keys.
- **`callers`** (hop 2 = `explain <symbol-name>` on a symbol hop 1 listed) — per-symbol
  `find_symbol_callers` (`rust/crates/agent/src/storage_port.rs:737`; `explain_symbol`'s call at
  `rust/crates/agent/src/explain/mod.rs:324`), ranked by `call_ranking::rank_caller_rows`
  (`rust/crates/agent/src/explain/call_ranking.rs:15-17,56-84`). Hop 2 resolves the name via
  `resolve_symbol_name` (`rust/crates/storage/src/agent_impl.rs:938-970`, returns a `Vec`), so it
  yields callers **only on a unique name**; a non-unique name returns the existing deterministic
  **ambiguity** result instead. There is **no** file-level caller surface — callers are per-symbol
  only, which is exactly why the neighbourhood is a sequence (and why hop 2 inherits name-resolution
  ambiguity as an accepted existing-surface limitation).

> **DELETED by this rework — the inline deterministic file-level caller fold.** The pre-rework §8.2
> step 5 built a *new* file-level aggregation (fan-out over ≤8 in-file symbols → `rank_caller_rows` →
> union → first-occurrence dedup → cap 8 → `symbols_scanned` unknown-vs-zero) inside the pre-rework
> `seed` handler. Under the amended VISION the candidate names the `explain`/`orient` follow-up instead of
> inlining a neighbourhood, so **that fold is not built** — the one piece of genuinely-new domain
> logic the pre-rework design carried is removed. This is the rework's principal simplification
> (§10 abstraction ledger, §11 milestone, §12 validation are updated accordingly).

- **`--json`** rides each seam's existing `--json` idiom unchanged
  (`rust/crates/rgr/src/commands/orient.rs:61` `"--json"`, emit `:201` `serde_json::to_string_pretty`);
  the semantic-only fields are additive on the existing `FocusCandidate` / error-`data` shapes. Human
  mode mirrors each seam's existing density.

### 8.3 Honest empty / degraded states (I4; architecture.md Honest Degradation Rule)

Each is a distinct, reader-facing state — `null`/absent ≠ known-zero, and none narrates our
pipeline. **In every degraded state the seam's deterministic outcome is byte-identical to today**
plus **one labeled `Limit`** stating the fallback was unavailable and why.

> **Code-truth note (empty candidates — resolves review-7 observed item 1).** `Focus.candidates`
> carries `#[serde(skip_serializing_if = "Vec::is_empty")]` (`rust/crates/agent/src/dto/envelope.rs:111-112`),
> so an **empty candidates list is OMITTED from JSON, never emitted as `[]`**. The examples below
> therefore show the `candidates` key **absent** in every zero-candidate state; the signal that the
> fallback fired-but-produced-nothing (or was unavailable) is carried **entirely** by the always-present
> labeled `Limit` — never by an empty array. This matches code with **no serializer change** (the
> smallest design; an always-present `[]` would need a bespoke serialization override, which this
> slice does NOT take). Group A's degraded shape is thus exactly today's no-match
> (`resolved:false, reason:no_match`, `candidates` omitted) + the `Limit`; Group B is the plain
> `symbol not found` error (`InvalidRequest`, §8.2) + `data.hint` (and `data.semantic_candidates`
> omitted when empty, same discipline).

The `Output` column below is written for **Group A** (`limits: Vec<Limit>`, §8.2 — objects with a
code-derived `summary` + per-situation `reasons`, **not** strings); for **Group B** the identical
information rides the not-found error's `data.hint` / one appended message line (§8.2). All degraded
rows use the `SemanticFallbackUnavailable` code (new `LimitCode` variant, IMPL work per §8.2); the
stale-subset and nothing-scored rows use `SemanticFallback` (candidates may still be present). `<…>`
is the per-situation `reasons[0]`:

| Situation | Output (Group A shown; `candidates` OMITTED when empty) | Never |
|---|---|---|
| No vector store yet (never indexed / just built) | `candidates` absent; `limits: [{code:"SEMANTIC_FALLBACK_UNAVAILABLE", summary:<fixed>, reasons:["no seed vectors yet; they build in the background after indexing"]}]` | not an error; not "0 matches" as if measured |
| Model unavailable (endpoint down / no local model) | `candidates` absent; `limits: [{code:"SEMANTIC_FALLBACK_UNAVAILABLE", …, reasons:["no local embedding model reachable; seeding is optional, resolution is unaffected"]}]` | never blocks; never degrades the deterministic tiers |
| Pins mismatch (model/dim/schema changed) | `candidates` absent; `limits: [{code:"SEMANTIC_FALLBACK_UNAVAILABLE", …, reasons:["seed vectors were built with a different model; rebuild on next index"]}]`, store discarded | never rank across a pin mismatch |
| Some files stale (content changed since embed) | ranked over the fresh subset; `limits: [{code:"SEMANTIC_FALLBACK", …, reasons:["N files changed since last embed — not yet re-seeded"]}]` | never silently rank a stale vector as current |
| Query embeds but nothing scores | `candidates` absent; `limits: [{code:"SEMANTIC_FALLBACK", …, reasons:["no candidate scored above zero"]}]` (genuine known-zero) | — |

Each degraded state maps to an `Embedder`/store error variant (§10) or an empty/mismatched store —
no state collapses into another (Honest Degradation Rule, `docs/architecture/artifact-contract-model.md:417-429`).
Crucially, none of these is ever reached on a **resolved** or **ambiguous** seam result — the tier
is consulted only on the deterministic-zero branch (§8.1), so a working deterministic answer never
pays for, waits on, or is altered by seeding.

### 8.4 Budget caps (exact — review-0 item 4)

Every numeric limit is fixed here so the IMPL builds against a contract, not a guess:

| Cap | Value | Where enforced | Ratification-class? |
|---|---|---|---|
| Embed input per document | **6 000 chars** (char-boundary) | corpus build (§3.2) | no — spike mechanism constant (`tools/embed-seed-spike/spike.py:101`) |
| Embed batch size | **32** documents/request | embed pass (`tools/embed-seed-spike/spike.py:98`) | no |
| Corpus admission cap | **50 000 files** (default); above it, embed the first 50 000 by `path` order and emit an honest omission `limit` (MODULE-MODEL-2 D7 bounded-output discipline) | corpus build | no — a tunable safety bound for the 160k-file monorepo target; **INFERRED default**, adjustable |
| Candidate count | **≤ 5** | ranking output (the seam's `candidates` / error `data`) | **yes — VISION bound** |
| Per-candidate `module` locator | 1 `aggregate_file` + 1 `resolve_path_focus` per candidate (≤10 lookups across ≤5) | candidate render (§8.2) | no — bounded by the ≤5 cap |
| Query path | one query embedding of the seam's resolution input + brute-force cosine over the store (spike: "trivially fast" over ~4k vectors) → sort → top-5. No pagination. | the seam's no-match branch (§8.1) | — |

> ⟨SUPERSEDED caps: the per-candidate `imports ≤ 8`, `symbols ≤ 8`, caller-fan-out `≤ 8`, and
> `callers ≤ 8` caps are removed — the candidate no longer inlines a neighbourhood (§8.2), so there
> is nothing to cap. Whatever the `next` follow-up (`explain`/`orient`) inlines is bounded by *those
> commands'* own existing caps, unchanged by this slice.⟩

The corpus-size cap is the only cap that bounds *coverage*; per the no-silent-caps rule it emits a
visible omission `limit` rather than silently truncating. The candidate cap (≤5) bounds
*presentation* and is recorded on the seam's envelope when it truncates.

---

## 8B. `rmap find "<concept>"` — the affirmative concept-search verb (VISION use (ii); DECISION D-ES-11)

**Scope note (HUMAN DIRECTIVE 2, 2026-08-25).** The fallback tier (§8) is not the ceiling. VISION
§ Semantic Seeding now ratifies **three** uses of the same substrate (commit `8adabca`, binding;
measured in the spike addendum): (i) the §8 fallback tier; (ii) **`rmap find`**, this section; (iii)
cross-module concern hints, a named follow-on (§11). This section specs **only (ii)**. It is
**additive** — it does not touch §8, and §8's contract is unchanged.

**`find` is human-named and human-ratified**, and it **supersedes D-ES-1's rejection of a
search-named verb** (D-ES-1 rejected `find`/`search` as reading like an answer engine → I1 risk;
the human overrode that directly, VISION amended `8adabca` — recorded at D-ES-1 STATUS + D-ES-11).
The honesty mitigation is **not a rename** but **the output itself**: `find` never claims
completeness and always labels its results as Layer-3 hints to open, not answers (below). It is the
**one deliberately search-named verb**, honest because its rendered output says *hints, open the files*.

### 8B.1 What `find` is, and how it differs from the §8 fallback tier

| | §8 fallback tier | §8B `rmap find` |
|---|---|---|
| Trigger | **only** after every deterministic tier yields zero matches (§8.1) — never user-invoked directly | **user-invoked affirmatively**: `rmap find "<concept>"` always consults the store |
| Carrier | the existing seam's `Focus.candidates` / error `data` (no new verb) | **a new verb + its own envelope** (the only new surface) |
| Cap | ≤5 candidates (VISION bound) | **≤10** candidates (human-directed bound, HUMAN DIRECTIVE 2, 2026-08-25 — VISION `8adabca` ratifies the verb + honesty posture; the numeric cap is the operator's directive, not VISION prose) |
| Substrate (store §4 · pins §7.1 · corpus §3 · ranking §7.2 · degradation *causes/state taxonomy* §8.3) | — | **shared** with §8 |
| Degradation *output shape* | `Focus.candidates` omitted-when-empty + a `Limit` line (§8.3) | **intentionally distinct**: own DTO — always-present `candidates: []` + a `summary` line (§8B.2/§8B.3) |

`find` reuses the same underlying **substrate**: the same per-repo sidecar store (§4), the same pinned
`(model_id, dim, content_hash)` hard-fail (§7.1), the same corpus (§3, incl. the SQL/DDL
coverage limitation §3.3), the same cosine + path-order tie ranking (§7.2), and the same **degradation
state taxonomy / error causes** (§8.3) — i.e. the *same set of unavailable/pin-mismatch/known-zero
conditions*, detected by the **same** `Embedder`/store error variants. What `find` does **not** share
is the *rendered degradation output*: the §8 tier fills `Focus.candidates` (omitted-when-empty) plus a
`Limit` line, whereas `find` renders its **own** DTO (§8B.2) — an always-present `candidates: []` under
a `summary` header. The genuinely new surface is the verb, its envelope, and that distinct empty/degraded
rendering — there is **no** new store, pin, ranking, or degradation-*detection* logic.

### 8B.2 The envelope (`--json`) — its own DTO carrying the §8.2 candidate FIELDS

`find` is a **new verb with its own top-level response DTO** — it does **NOT** reuse Group A's
`FocusCandidate` (which is pinned to stay byte-compatible with the existing deterministic `Focus`
envelope, so it carries `file: Option<String>` and derives `Eq`,
`rust/crates/agent/src/dto/envelope.rs:54-59`). `find` has no such constraint, so it defines a fresh
struct that carries the **same semantic FIELDS** the fallback tier surfaces — but names its locator
field `path` (a plain `String`, not `FocusCandidate.file: Option<String>`) and its score `f64`
(so the DTO does **not** derive `Eq`). This resolves the review-9 §8B contract gap: "reuse §8.2
verbatim" was wrong — §8.2's Group-A object is `FocusCandidate` (field `file`), while `find`'s
example renders `path`; the honest statement is that `find` shares the fallback tier's *fields and
semantics*, not its *struct*. The exact `find` DTO (IMPL work, D-ES-11):

```rust
// NEW DTOs owned by the `find` handler — NOT FocusCandidate. `candidates` is a plain Vec
// (default serde) → serialized ALWAYS-PRESENT as `[]` when empty; NO `skip_serializing_if`
// (that attribute belongs to Focus.candidates, envelope.rs:110-112, and is NOT inherited here).
struct FindResponse {
    schema: String, command: String, repo: String, snapshot: String,
    query: String,
    summary: String,                 // ALWAYS present — the Layer-3 honesty header (I1/I2)
    candidates: Vec<FindCandidate>,  // plain Vec → `[]` when empty (§8B.3), never omitted
}
struct FindCandidate {
    stable_key: String,
    path: String,                    // repo-relative path — a plain String, NOT `file: Option<String>`
    score: f64,                      // cosine (§7.2); a float → this DTO does NOT derive Eq
    source: String,                  // always "embedding" (I2)
    model_id: String,                // the pin (§7.1)
    module: String,                  // owning module, carried directly (§8.2a)
    next: NextCommand,               // the `explain <stable_key>` first hop (§8.2a)
}
```

The store, pins, corpus, ranking, and the *degradation state taxonomy / error causes* are shared with
the §8 tier (§8B.1); what differs is deliberate and confined to **rendering**: the **≤10 cap**, this
**own DTO** in place of `Focus.candidates`, the **always-present top-level `summary` honesty header**,
and the **always-present `candidates: []`** empty serialization (vs §8's omitted-when-empty). Human
mode leads with the labeled line; `--json` emits this DTO via each seam's existing
`serde_json::to_string_pretty` idiom (`rust/crates/rgr/src/commands/orient.rs:199-203`).

```jsonc
// rmap find "exchange rate conversion" --json
{
  "schema": "rgr.agent.v1",
  "command": "find",
  "repo": "glamCRM",
  "snapshot": "…",
  "query": "exchange rate conversion",
  // Layer-3 header — the honesty mitigation, ALWAYS present, NEVER a completeness claim:
  "summary": "likely areas for \"exchange rate conversion\" (semantic hints — open the files)",
  "candidates": [                              // ≤10, ranked by cosine then path order (§7.2)
    {
      "stable_key": "glamCRM:serverless/.../bnr-service.ts:FILE",
      "path": "serverless/packages/backend/src/services/bnr-service.ts",
      "source": "embedding",
      "model_id": "text-embedding-nomic-embed-text-v1.5",
      "score": 0.71,
      "module": "backend/services",
      "next": { "cmd": "explain", "args": ["glamCRM:serverless/.../bnr-service.ts:FILE"], "cwd": "<repo_root_abs>" }
    }
    // … up to 10
  ]
}
```

- **`summary` is the honesty contract (I1/I2).** It reads *"likely areas for `<concept>` (semantic
  hints — open the files)"* — hints, not an answer; it **never** states or implies completeness
  ("here is everything about X"). Human mode prints this line first; each candidate then shows
  `path` + `score` + `model_id` + its `next` follow-up. This is why a search-named verb does **not**
  violate I1: the surface itself declares its Layer-3 status and points the reader at the files.
- **≤10 candidates** (human-directed bound, HUMAN DIRECTIVE 2 — VISION `8adabca` ratifies the verb +
  honesty; the ≤10 cap is the operator's directive), ranked cosine-desc then **path order** on ties (§7.2,
  reused). `source:"embedding"` + `model_id` on every candidate (I2). The `next` follow-up is the
  same deterministic `explain <stable_key>` → imports+symbols, then `explain <symbol-name>` →
  callers-on-a-unique-name sequence as §8.2a (reused verbatim; no new neighbourhood logic).
- A candidate whose stored path no longer resolves against the current snapshot (a stale vector) is
  **dropped** — the same `resolve_path_focus` admission check the fallback tier uses (§8.2).

### 8B.3 Degraded states — same causes/taxonomy as §8.3, distinct rendering for an affirmative verb

`find` has **nothing deterministic to fall back to** (unlike §8, which sits behind a resolved/no-match
tier), so when the substrate is unavailable it returns **zero candidates + one labeled line stating
why** — never an error, never a fabricated "0 results as if measured":

| Situation | `find` output |
|---|---|
| No vector store yet (never indexed / just built) | `candidates: []`; `summary:"semantic index not built yet — hints will be available after indexing"` |
| Model unavailable (endpoint down / no local model) | `candidates: []`; `summary:"no local embedding model reachable — semantic hints unavailable (find is optional)"` |
| Pins mismatch (model/dim/schema changed) | `candidates: []`; `summary:"semantic index was built with a different model — rebuild on next index"`, store discarded (§7.1) |
| Query embeds but nothing scores | `candidates: []`; `summary:"no area scored above zero for \"<concept>\""` (genuine known-zero) |

Each maps to the **same** `Embedder`/store error variant as §8.3 (same causes — no new degradation
*detection* code); only the **rendering** above is `find`'s own (always-present `[]` + `summary`).

> **Code-truth note (empty-array serialization — resolves review-9 item 1).** `find`'s
> `candidates` is a field on its **own** DTO (§8B.2), so its empty-state serialization is a fresh
> IMPL choice, **not** inherited from `Focus.candidates`. §8.3's *omit-when-empty* behaviour is a
> property of the `#[serde(skip_serializing_if = "Vec::is_empty")]` attribute on **`Focus.candidates`**
> (`rust/crates/agent/src/dto/envelope.rs:110-112`) — it exists only to keep the existing `Focus`
> byte-output unperturbed, and a new DTO does **not** and **need not** inherit it. For `find` the
> smaller, honest choice is a **plain `Vec` with NO `skip_serializing_if`**: `candidates` is
> **always present**, serialized as `[]` when empty (default serde `Vec` behaviour — no bespoke
> serializer override). So an affirmative `find` always returns a well-formed object whose empty
> `[]` **and** whose `summary` line together carry the honest signal; a consumer never has to
> distinguish "key absent" from "empty". This divergence from Group A is intentional and named as
> IMPL work under D-ES-11 (Group A must not perturb `Focus`; `find`, a new surface, is free to be
> always-present).

### 8B.4 The new surface — CLI verb + dispatch arm + witness-manifest line (IMPL work)

Adding `find` is a **new read-only verb** under the **standing Protocol Surface Standard**
pre-ratification (naming is human-fixed; the verb reads as read-only, matching the standard). The
IMPL enumerates exactly these additive touch-points against code — no substrate change:

| Site | What IMPL-1 adds | Anchor (OBSERVED) |
|---|---|---|
| CLI dispatch arm | one arm `"find" => run_find(&args[2..])` in the hand-rolled `match args[1].as_str()` (there is **no** clap enum) + one `use` in the handler import block | `rust/crates/rgr/src/main.rs:84` (match), `:96`/`:104` (`orient`/`explain` arms), imports `:54-58`; re-export `rust/crates/rgr/src/commands/mod.rs:67` |
| CLI handler `run_find` | mirrors `run_orient`'s shape verbatim: parse `--json` → `json_mode`, resolve repo from cwd (`current_dir` → `canonicalize`), build params, `client.request("find", …)`, emit `to_string_pretty` when `--json` else human render | `rust/crates/rgr/src/commands/orient.rs:50` (`run_orient`), `--json` `:61-62`, cwd `:161`/`:169`, request `:197`, emit `:199-203` |
| Daemon dispatch arm | one arm `"find" => self.handle_find(request, emitter)` in the canonical method router + a `handle_find` method (peer of `handle_orient`) | `rust/crates/daemon-runtime/src/dispatch.rs:330` (`match request.method.as_str()`), `"orient"` arm `:365`, `"explain"` `:367` |
| Witness manifest line | **one line** `find = FC1, …` in the completeness registry — a new dispatch arm goes **RED** in the guard test until declared | `rust/crates/daemon-runtime/witness/dispatch_fact_classes.txt:37-38` (`orient`/`explain` lines; 68 arms today `:16`), enforced by `rust/crates/daemon-runtime/tests/consolidation_witness.rs:35-43` |

All four are **mechanical additive edits** the IMPL performs under D-ES-11 + the standing verb-naming
pre-ratification; none touches the store, pins, ranking, corpus, or degradation contracts (those are
§3–§8, unchanged). `find`'s query path is one query embedding + brute-force cosine over the store →
sort → top-10 (§8.4 query-path row, cap 10 instead of 5).

---

## 9. CERTAINTY + HONESTY (resolves packet item 5)

- **Layer-3 labels in the reader's language (VISION § Labels).** On a seam's no-match fallback the
  labeled line reads *"no exact match — likely starting point (semantic match, model
  `text-embedding-nomic-embed-text-v1.5`): open the file and re-run"*, never *"embedding cosine 0.71,
  vector store fresh"* (that is our pipeline state; keep it to `--json` fields + doctor).
  `source: "embedding"` + `model_id` are the machine-readable provenance; the human line names
  *their* code and *why* it surfaced, and states plainly it is a fallback, not a resolution.
- **Doctor** (`rust/crates/rgr/src/commands/doctor/mod.rs:65` `run_doctor`) gains a **"Semantic
  seeding"** block beside the existing `Storage:` (:350) / enrichment (`:274-276`) sections,
  showing three honest facts: **vector store state** (present / absent / building), the **model
  pin** (`model_id`, `dim`) with its **identity provenance** — `endpoint-echoed` (the endpoint
  returned a `model` field that matched, §7.1 wire-time check) vs `operator-asserted` (no echo; the
  pin is `RMAP_SEED_MODEL_ID` on the operator's word, §6.1) — and the **staleness count** (N of M
  files changed since embed). The doctor must never print `operator-asserted` as if it were verified.
  The
  `--json` doctor path (`rust/crates/rgr/src/commands/doctor/mod.rs:88-91`, `serde_json::to_string_pretty`) carries the same. This is
  the visibility half of Persistence-Completeness for a *cache* (no trust/schema impact).
- **Trust / reliability untouched.** Seeding contributes **nothing** to trust scores, call-graph
  reliability, or any Layer-0/1/2 surface (I1). The trust model, the RED-by-design unresolved-call
  floor, and every reliability caveat are byte-identical whether or not seeding exists. A Layer-3
  hint must never move a certainty number.

---

## 10. Smallest-design statement & abstraction ledger (resolves review-0 item 7)

Recommended path (D-ES-10 seam integration, D-ES-2 state-root sidecar, D-ES-3 background-at-refresh,
D-ES-5 file-level, D-ES-6 exclude test/generated/vendored, D-ES-7 lightweight candidate envelope +
named `next`) introduces **no SQL schema change and no new DB table**, and reuses: the
`files`/`file_versions` reads the spike proved; the warm-cache envelope + `atomic_write`; the
ENRICH-LIFECYCLE-1 background/cancel lifecycle; the existing focus-resolution surface (to render each
candidate's `stable_key`/`module` locator and to power the `next` follow-up); each seam's existing
`--json` idiom and `Focus`/`error-data` envelope. New code is one support unit + **two ports**
(`Embedder` + `SeedCorpusRead`, the corpus-read seam — §10 ledger) + one background pass + **the
seam-integration wiring (no new verb, no new dispatch arm)** + one doctor block + the cross-crate
`artifact-contracts`/`repo-index` family registration (§3.4). The **only** genuinely new domain
logic is the vector envelope + cosine ranking — pure and headless-testable. (⟨SUPERSEDED: the
pre-rework "one verb" and the "deterministic file-level caller fold (§8.2 step 5)" as new domain
logic are both **deleted** — the tier integrates into existing seams and names a follow-up command
instead of folding a neighbourhood, so the caller-fold surfaces `find_file_imports` /
`list_symbols_in_file` / `find_symbol_callers` / `call_ranking` are no longer called by seed code;
they run inside the existing `explain`/`orient` the candidate points at.⟩)

**Abstraction ledger** (one line each; a line that cannot be filled ⇒ the abstraction is removed):

- **`repo-graph-seed` support unit** — *what:* pure corpus-build + vector envelope + cosine ranking.
  *Concrete current users:* the seam no-match fallback (the `orient`/`explain` focus-resolution path
  that queries the store) + the background embed pass (two callers).
  *Dispatch axis:* operations-fixed / no variant growth → plain functions, no trait. *Why it exists
  at all:* the architecture's support-module-first build order + a headless test seam (rank/envelope
  unit-tested with a faked `Embedder` **and a faked `SeedCorpusRead`**, no model, no daemon, no DB).
  **This unit is INNER/pure and depends on no storage adapter** — it defines the `SeedCorpusRead`
  port (next entry) and owns the raw corpus-row DTO the adapter fills; it imports only `serde` +
  warm-cache-style primitives. **The crate-vs-module boundary is a ratification cell — D-ES-8** (a
  new crate is a permanent release unit + graph node; a module in an existing pure crate is smaller
  but dilutes that crate's cohesion). *Rejected simpler:* inline in `daemon-runtime` — rejected, it
  would put pure domain logic in the daemon adapter (architecture.md Rule 3 / Prohibited Patterns).
- **`SeedCorpusRead` port + `SeedCorpusEntry` boundary DTO** — *what:* the read seam that hands the
  pure seed logic its corpus catalog without the logic importing the SQLite adapter. The port
  (defined **in the pure seed unit**, policy-side) is `fn seed_corpus(&self, repo_uid: &str) ->
  Result<Vec<SeedCorpusEntry>, SeedCorpusError>`; `SeedCorpusEntry { file_uid, path, content_hash }`
  is a raw owned DTO — three `String`s, **no** `rusqlite::Row`, no storage type, no framework object
  (architecture.md boundary-DTO rule). *Concrete current users:* the background embed pass (calls it
  to enumerate what to embed) + the query path's freshness/pin check (two). *Dispatch axis
  (concrete):* the dependency rule — this is the **earned inversion** that keeps the pure core off
  the outer `repo-graph-storage` adapter. **`repo-graph-storage` adds a dependency on the seed unit
  and implements `SeedCorpusRead` on `StorageConnection`** (the READY-snapshot `SELECT file_uid, path
  FROM files WHERE repo_uid=? AND is_test=0 AND is_generated=0 AND is_excluded=0` joined to
  `file_versions.content_hash`, §3.1/§3.3), converting its rows to `SeedCorpusEntry` before calling
  the pure logic — direction **adapter → policy (outer → inner)**, byte-identical to the ratified
  `AgentStorageRead` pattern (`repo-graph-storage` already does exactly this for `repo-graph-agent`:
  `rust/crates/storage/Cargo.toml:155-161`; the policy crate forbids the reverse edge:
  `rust/crates/agent/Cargo.toml:39-41`). *Rejected simpler:* have `repo-graph-seed` depend on
  `repo-graph-storage`'s DTOs directly — **rejected, it inverts the dependency rule** (the pure inner
  core would import the SQLite adapter, which itself depends on `rusqlite` + `agent` + `indexer`;
  `rust/crates/storage/Cargo.toml:60-161`) and would make the D-ES-8 "module inside `agent`"
  alternative impossible (`agent` is forbidden from depending on `storage`). *Not a new ratification
  cell:* dependency inversion via a policy-owned read port is the pattern the architecture already
  mandates and the workspace already uses (`AgentStorageRead`, `TrustStorageRead`,
  `EnrichmentStoragePort`) — this ledger line is the required abstraction one-liner, not a new
  boundary decision.
- **`Embedder` port** — *what:* the seam between the seed pass and the model runtime. *Concrete
  current users:* the background embed pass + the query path (two). *Dispatch axis (concrete):* the
  D-ES-4 distribution choice — implementations grow (endpoint (a) ships; embedded-ONNX (b) is the
  named, ratification-pending second impl) → interface + polymorphism. This is the one place the
  dependency rule earns an inversion (volatile model mechanism kept out of the pure pass). *Rejected
  simpler:* hard-code the HTTP call in the pass — rejected because D-ES-4 is an *open, ratified-
  near-term* distribution decision, so the variation is real, not imagined.

  **Signature (corrected — review-0 item 7).** The iteration-0 `embed(texts) -> Vec<Vec<f32>>`
  could not represent failure. The port is `Result`-typed with a contextual error sum and a
  **raw-DTO boundary** (only `Vec<Vec<f32>>` raw floats cross — never an HTTP/framework type):

  ```rust
  /// The model-runtime seam. `texts` are already role-prefixed
  /// ("search_document: …" / "search_query: …") by the caller (§3.2).
  pub trait Embedder {
      fn model_id(&self) -> &str;
      fn dim(&self) -> usize;
      fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError>;
  }

  pub enum EmbedError {
      /// Endpoint not reachable (server down / no local model) → "no hints".
      Unreachable      { endpoint: String, detail: String },
      /// Configured endpoint is not loopback → refused (I4 enforcement, D-ES-4).
      NonLoopbackRejected { endpoint: String },
      /// Response body not the expected shape.
      Malformed        { detail: String },
      /// A returned vector's length ≠ the pinned `dim`.
      DimMismatch      { expected: usize, got: usize },
      /// Model identity ≠ the pinned `model_id`. Two sources (§7.1): the endpoint's
      /// echoed response `model` field (when present) ≠ pin — the wire-time check; or the
      /// query-time configured model ≠ the store pin. A missing echo is NOT this error
      /// (the pin stays operator-asserted, §6.1/§9), only a present-and-different one is.
      ModelMismatch    { expected: String, got: String },
  }
  ```

  Each variant maps to a distinct honest degraded state (§8.3): `Unreachable` → "model
  unavailable"; `NonLoopbackRejected` → refused (never egress); `DimMismatch`/`ModelMismatch` →
  whole-store "pins mismatch"; `Malformed` → "model unavailable" (with the detail logged, not shown
  as a fact).

  **The (a) impl's transport is NOT an abstraction — it is a D-ES-9 ratification cell.** The single (a)
  `Embedder` impl issues one POST to the loopback `/v1/embeddings` endpoint. The workspace has **no**
  outbound HTTP client today (OBSERVED, §6.1 point 3), so IMPL-1 must either hand-roll a std-library
  `TcpStream` transport (D-ES-9 (a2), the recommendation — **no new dependency edge**) or take an HTTP
  client dependency (D-ES-9 (a1) — **one new component-graph edge**). Either way the **JSON** response
  is parsed with `serde_json`, already a workspace dependency (OBSERVED: present in 32 crate manifests,
  e.g. `rust/crates/artifact-contracts/Cargo.toml`), so response parsing adds nothing. The transport is a
  concrete impl choice with a dependency-graph blast radius, not a new interface — the seam is the
  `Embedder` port above; the transport lives *inside* the (a) impl behind it.

- **No** vector DTO in the DB, **no** SQL migration, **no** new retention class, and **no config
  subsystem** — the only configuration is **four env vars** read directly via `std::env::var`, the
  house pattern (`rust/crates/daemon-runtime/src/enrich_pass.rs:142`): the refresh opt-out `RMAP_SEED_VECTORS` (§5.2) plus the three
  D-ES-4(a) endpoint inputs `RMAP_SEED_ENDPOINT` / `RMAP_SEED_MODEL_ID` / `RMAP_SEED_DIM` (§6.1).
  These configure the single (a) `Embedder` implementation; they are **not** a general config layer
  and add no file format or schema. (Iteration 1's claim of "no config layer beyond one env opt-out"
  was wrong for option (a), which inherently needs the endpoint/model/dim inputs — corrected here and
  in §6.1, review-1 item 4.) The `artifact-contracts` family entry (§3.4) is **not** an abstraction;
  it is the mandatory contract registration the artifact model requires for any persisted family.

---

## 11. MILESTONES (resolves packet item 7)

Smallest deep-vertical IMPL cut = **capability wired to a visible output surface in one slice**.

**Model runtime is settled: D-ES-4 is RATIFIED as option (a), the OpenAI-compatible local endpoint
(2026-08-25, human).** So this is the **binding IMPL-1 plan** — not a conditional one — and the
embedded-ONNX path (b) is a **superseded alternative** (an unbuilt, separately-ratifiable future
runtime, §11 deferred list), **not** a live branch of this milestone. **Two cells remain open** and
shape only the manifest footprint (not whether IMPL-1 runs): **D-ES-8** (crate-vs-module home for the
pure seed logic) and **D-ES-9** (the option-(a) HTTP transport). The footprint is **light but NOT
zero**, and its exact shape depends on D-ES-8/D-ES-9 — stated honestly here (iterations 1–4 wrongly
claimed the plan touches "no `Cargo.toml`"; corrected). IMPL-1 splits into a runtime-agnostic core
and the ratified (a) `Embedder` implementation:

- **Runtime-agnostic core (built in the SAME slice as the first ratified `Embedder` — never
  dormant; review-6 #4):** the pure seed logic (corpus build, envelope,
  cosine+tie ranking — **no caller fold; deleted this rework**), the cross-crate `SeedVectors` family
  registration (§3.4), the `Embedder` **port** (§10, the seam — not any impl), the background pass,
  the **Group-A seam integration** (the `orient`/`explain` focus-resolution no-match fallback that
  fills `Focus.candidates` with labeled semantic candidates + `next`, §8) on each seam's existing
  `--json`, the doctor block, the degraded states, **and — per VISION `8adabca` (uses (i)+(ii) ship in
  the first IMPL) — the `rmap find` verb (use (ii), §8B)**. **The fallback tier (use (i)) adds no new
  verb; `rmap find` is the one new verb + one CLI arm + one daemon arm + one witness-manifest line
  (§8B.4), reusing the shared substrate — no new store/pin/ranking/degradation code.**
  **Manifest impact — decided by D-ES-8:** if D-ES-8 ratifies the **new crate `repo-graph-seed`**,
  IMPL-1 adds one workspace-member line to `rust/Cargo.toml` (`members = [ … "crates/repo-graph-seed" ]`
  — the workspace currently lists 49 crate members with **no** `seed` entry, OBSERVED at
  `rust/Cargo.toml:19-74` (the `members` array; the 49 `crates/…` entries are lines `:20-68`, plus
  four `tools/…` entries, none named `seed`)) and creates `crates/repo-graph-seed/Cargo.toml` (its own manifest, depending only
  on `serde`/`serde_json` + `repo-graph-warm-cache`-style primitives — **not** on `storage`). The pure
  unit **defines the `SeedCorpusRead` port + `SeedCorpusEntry` DTO** (§10); `repo-graph-storage` gets
  the **new dependency edge** (`repo-graph-storage → repo-graph-seed`) and implements the port on
  `StorageConnection`, so the added manifest line is on the **storage** crate's `[dependencies]`
  (adapter → policy, outer → inner — the direction `rust/crates/storage/Cargo.toml:155-161` already
  uses for `repo-graph-agent`), **not** a `storage` dependency inside the seed crate. If D-ES-8 instead
  ratifies a **support module inside an existing pure crate**, the concrete host is named at
  ratification (D-ES-8's stated candidate is `crates/agent`); because the seed logic reaches its
  corpus **only through the `SeedCorpusRead` port it defines** — never a `storage` import — this
  option is now coherent with `agent`'s ratified no-dependency-on-`storage` boundary
  (`rust/crates/agent/Cargo.toml:39-41`). It adds a module file and, only if the host crate does not
  already pull the needed primitives, at most a dependency line to that crate's manifest — **no new
  workspace member**; `repo-graph-storage` still gains the port-impl edge on whichever crate hosts
  the port. Either way the family-registration edits
  (`artifact-contracts`, `repo-index`) are Rust source, not manifest.
- **The ratified (a) `Embedder` implementation.** IMPL-1 ships the endpoint impl (env config +
    loopback enforcement, §6.1) and is complete as one slice. **Manifest impact — decided by D-ES-9:**
    under the recommended std-library transport (a2) the impl adds **no dependency** (raw
    `std::net::TcpStream` + hand-framed HTTP/1.1, http-loopback only; JSON via the already-present
    `serde_json`); under a client crate (a1) it adds **one** HTTP(/TLS) dependency edge to the home
    crate's manifest. So (a)'s total manifest cost ranges from *one workspace-member line* (a2 + new
    crate) to *a member line plus one dependency edge* (a1 + new crate) — light either way.
  - **Superseded alternative — embedded-ONNX (b).** D-ES-4 ratified (a), so (b) is **not** a branch of
    this IMPL-1; it is a named, unbuilt future runtime. Were it ever separately ratified, it would get
    its **own complete vertical** — a post-ratification distribution slice (spec + its own ratification)
    shipping the runtime-agnostic core above **and** the embedded-ONNX `Embedder` (heavyweight
    dependency, bundled/versioned/notarized model, the larger `Cargo.toml` edits) **in the same arc**,
    so the fallback tier would work the day that capability exists (never a dormant core, review-6 #4).
    The core's design (this §11) would be reused verbatim; only the impl behind the port differs. This
    is recorded for completeness — it is not part of the current binding plan.

> The no-`Cargo.toml` / no-production-code STOP_CONDITIONS bind **this SPEC only** — they forbid the
> *spec* from editing manifests, not the future IMPL. **EMBED-SEED-IMPL-1 operates under its own packet
> and WILL edit `rust/Cargo.toml` to the extent D-ES-8 and D-ES-9 dictate (above).** So neither option
> (a)/(b) nor the crate/module split is foreclosed by this spec; each is chosen at ratification and
> built under its own packet.

- **EMBED-SEED-IMPL-1 (the deep-vertical cut — the binding plan under D-ES-4 option (a)).** The pure
  seed logic in its D-ES-8-ratified home (**new crate `repo-graph-seed`** → one `rust/Cargo.toml`
  member line + its crate manifest; **or** a support module in the ratified host crate → module file
  + at most one dependency line) — corpus build from
  `files`+`file_versions`, file-level docs with the §3.2 exact format + 6 000-char cap, the
  source/snapshot-race hash re-verification §3.5, warm-cache envelope with `atomic_write`,
  cosine+path-tie ranking (**no caller fold**), headless tests
  → the **cross-crate `SeedVectors` family registration** (§3.4: `artifact-contracts` enum/`table_name`/
  `all`/`get_contract` + its coherence test, and `repo-index` `family_to_table` + the two refresh-array
  exemptions) → one `Embedder` port with the (a) endpoint impl incl. the §6.1 env config + loopback
  enforcement + the **D-ES-9-ratified transport** (a2 std-library `TcpStream` with no new dep, or a1
  client dependency) → background embed pass after index/refresh reusing
  `spawn_auto_enrich`'s shape + `EnrichCoordinator` cancel → **the Group-A seam integration** (§8):
  wire the `orient`/`explain` no-match branch (`orient/mod.rs:251-256`, `explain/mod.rs:187`) to
  embed the focus/target string and fill `Focus.candidates` (additive fields on `FocusCandidate`)
  with ≤5 labeled candidates + `module` locator + `next` follow-up + the labeled `SemanticFallback`
  `Limit` (new `LimitCode` variant, §8.2/§8.3) →
  **the `rmap find` verb (use (ii), §8B):** one CLI arm `"find" => run_find` (`main.rs:84`) +
  `run_find` mirroring `run_orient` (`orient.rs:50`) + one daemon arm `"find" => handle_find`
  (`dispatch.rs:330`) + one witness-manifest line (`dispatch_fact_classes.txt`), returning ≤10 labeled
  `source:"embedding"` candidates under the honesty header over the SAME store/ranking (no new
  substrate) →
  doctor "Semantic seeding" block → the honest degraded states.
  **Done when:** on the glamCRM smoke fixture, isolated
  `rmap orient "where does the backend fetch BNR exchange rates?"` (a phrase with **no** deterministic
  match) returns, in its `focus.candidates`, the bnr-service file as a labeled `source:"embedding"`
  candidate with its `module` + `next: explain <key>` in ≤5 (reproducing the spike's 14/16-class
  result on the ratified tasks); running that `next` yields the file's deterministic imports +
  symbols, and one further `explain <symbol-name>` on a listed symbol yields its callers **when that
  name resolves uniquely** (else the existing ambiguity result — hop 2 keys by name, §8.2a);
  **and (use (ii)) isolated `rmap find "exchange rate conversion" --json` returns the bnr-service file
  among ≤10 labeled `source:"embedding"` candidates under the `summary:"likely areas … open the files"`
  header, each with its `module` + `next` (§8B);** and
  every **resolved** or **ambiguous** `orient`/`explain`/`callers`/`callees`/`path`
  result, plus `trust`/`doctor`(existing lines), is byte-unchanged (I1/I4 regression check).

**Explicitly deferred (named extension points, not built):**

- **Symbol-level corpus (D-ES-5)** — the (S) format; wins UI-phrasing misses at 5.6× store size.
- **Group-B seam integration — `callers`/`callees`/`path` (D-ES-10, deferred cut).** The tier is
  **specified** for these seams (§8.1/§8.2: fire on `SymbolResolveError::NotFound`,
  `dispatch.rs:1268/1438/2479/2502`; attach `data.semantic_candidates` + `data.hint` to the existing
  `symbol not found` error) but **not built in IMPL-1**. Reason: the integration surface is the
  daemon error `data` (a different carrier than Group A's `Focus.candidates`), and a file-level store
  answers a *symbol* query only with *files* — a weaker fit that is honest but lower-value. Built as a
  follow-up cut once Group A is proven; the core, ports, and store are reused verbatim.
- **Cross-module concern hints (VISION § Semantic Seeding use (iii)) — NAMED FOLLOW-ON milestone
  `EMBED-CONCERN-1`, NOT built in IMPL-1.** *Direction (not deep-designed here):* cluster the same
  file vectors IMPL-1 already stores (§4) — cosine K-means over the store — and surface the clusters
  that span **≥2 deployable modules** on the existing **module / boundary discovery surfaces** as
  **labeled Layer-3 seam/concern candidates**, complementary to (never overriding) the deterministic
  HTTP boundary links. *Evidence (measured, spike addendum):* K=24 cosine clustering over glamCRM's
  file vectors surfaced its real vertical concerns — sales-targets (frontend↔serverless, cohesion .93),
  tenant-config/brand (.92, 15 files), exchange-rate (.90), auth/Cognito (.89, 21 files) — concerns
  **invisible to import analysis** because the subsystems touch only over HTTP
  (`docs/spikes/2026-08-23-embed-seed-spike-1.md:61-68`, EXECUTED). *Shares only the store and pins,
  NOT the query→file ranking:* concern hints reuse IMPL-1's per-file vector **store** (§4), its
  `(model_id, dim, content_hash)` **pins** (§7.1), and the **degraded states** (§8.3) verbatim — but
  they do **not** reuse the §7.2 ranking. §7.2 ranks *query→file* cosine with a path tie-break; concern
  discovery **has no query** and ranks **clusters**, not files, so the fallback/`find` ranking cannot
  apply. The **K=24 cosine K-means + rank-clusters-by-span/cohesion** shown above is **spike evidence**
  (measured in the addendum, `:61-68`), *not* a specified production contract: the clustering algorithm,
  its `K`, and the cluster-ranking function are **deferred to the `EMBED-CONCERN-1` slice's own
  DECISIONS** — this SPEC neither ratifies them nor claims §7 covers them. The **only new work** is
  therefore that clustering/cluster-ranking pass **plus** the **rendering-surface decision** — which
  module/boundary command renders the hints, in what envelope, under what caps/labels — both deferred
  to `EMBED-CONCERN-1` (this SPEC does not deep-design use (iii) beyond this milestone cut). Ships after
  IMPL-1 proves the store + query→file ranking on the fallback tier and `find`.
- **Inlined neighbourhood in the candidate (the pre-rework caller fold)** — **removed, not deferred**:
  the amended VISION replaces it with the named `next` follow-up **sequence**, so the full
  `(module, imports, callers)` is reached by running `explain <file>` (imports + symbols) then
  `explain <symbol-name>` on a listed symbol (callers on a unique name, else the existing ambiguity
  result — hop 2 keys by name, §8.2a), never folded into the candidate (§8.2/§8.2a).
- **Embedded-ONNX runtime (D-ES-4 (b)) — SUPERSEDED ALTERNATIVE, not a live branch.** D-ES-4 is
  RATIFIED as (a), so (b) is **not** decided by this milestone; it is a named, unbuilt future runtime.
  It is out of *this SPEC's* scope regardless (it needs a heavyweight dependency + bundled model +
  `Cargo.toml`, this SPEC's STOP_CONDITION). Were (b) ever separately ratified, it would ship in its
  own complete post-ratification distribution slice (spec + ratification) that ships the
  runtime-agnostic core (this §11) **and** the embedded-ONNX `Embedder` in the same arc, so the
  fallback tier would work the day that capability exists — never a dormant core behind a degraded
  fallback (review-6 #4). The core's design (this §11) is reused verbatim; only the impl behind the
  `Embedder` port differs. Recorded for completeness — not part of the current binding plan.
- **Cross-repo / model-management / semantic-search-as-answer** — VISION § Semantic Seeding parks
  these in FUTURE-ITERATIONS unless separately ratified.
- **DB-backed vector store (D-ES-2 (b))** — only if a ratified requirement needs snapshot-scoped
  vector storage in the authoritative DB; deferred for the schema/retention cost and the truth-class
  blur of putting a non-cross-machine-reproducible Layer-3 hint (§7.3) in the snapshot DB.

---

## 12. Validation (for the IMPL; this SPEC validates by citation)

- **This SPEC:** every existing-code claim carries a spot-checkable `file:line` (§0 evidence law);
  each distribution/contract/boundary decision is a DECISION_REQUIRED matrix with risk/reward; the
  doc is self-contained (readable without the packet). No code, no `Cargo.toml`, no SQL schema
  (STOP_CONDITIONS honored).
- **The IMPL must produce (EXECUTED):** `cargo build/fmt/clippy/test` green in `rust/`; headless
  unit tests for envelope round-trip, pin-mismatch discard, `content_hash` staleness exclusion, the
  source/snapshot-race re-hash omission (§3.5), atomic-publish-on-cancel (a cancelled pass leaves
  the prior store intact, §4.3/§5.1), and cosine+tie ranking (no model needed — the `Embedder` port
  is faked); **the seam-integration tests: (i) a no-match focus fires the tier and fills
  `Focus.candidates` with `source:"embedding"` candidates + `next`; (ii) a resolved focus and (iii)
  an ambiguous focus are byte-identical to the pre-seed baseline (the tier does NOT fire); (iv) each
  degraded state (no store / model down / pins mismatch) returns the baseline no-match plus one
  labeled `limits` line; (v) `rmap find "<concept>"` returns ≤10 labeled `source:"embedding"`
  candidates under the `summary` honesty header, and (vi) each of find's degraded states returns the
  **always-present `candidates: []`** under the labeled `summary` honesty header (no error, no omitted
  key — matches §8B.2/§8B.3 and D-ES-11); and the `find` dispatch arm is
  declared in `witness/dispatch_fact_classes.txt` (else `consolidation_witness.rs` goes RED)** —
  no caller-fold test (deleted); the `artifact-contracts`
  registry-completeness + policy-coherence tests still pass with the new family
  (`rust/crates/artifact-contracts/src/registry.rs:458-465`); the isolated live dogfood
  (`./scripts/dogfood-isolated.sh`, never the operator's registry) running `rmap orient`/`explain`
  with a no-match phrase **and `rmap find` with a concept phrase** on the fixture and observing the
  labeled semantic candidates; a regression
  capture proving every **resolved**/**ambiguous** `orient`/`explain`/`callers`/`callees`/`path`
  result plus `trust`/`doctor`(existing lines) byte-unchanged; and the degraded states each exercised.

---

## DECISIONS (ratification-class — decision-review + operator ratify; the IMPL does NOT re-decide)

Status: **RATIFIED 2026-08-25 (human).** Each is an exhaustive matrix; RECOMMENDED was the builder's
defensible pick except D-ES-4 (distribution-level — **RATIFIED (a) 2026-08-25**). Three cells carry an
architecture-boundary blast radius and so are ratification-class even though they name a
RECOMMENDED: **D-ES-8** (new crate vs module — a component-graph node), **D-ES-9** (option-(a)
HTTP transport — a dependency-graph edge), and **D-ES-10** (the seam-integration contract — extends a
boundary DTO and changes five commands' no-match branches); the builder recommends but does not bind.
**Rework note (2026-08-25):** **D-ES-1 is SUPERSEDED-BY-HUMAN**; **D-ES-10 is NEW** and **D-ES-7's
output-shape cells are UPDATED**; per TD-015 the decision-review rerun challenges **only** D-ES-1
(supersession), D-ES-7 (update), and D-ES-10 (new) — D-ES-2,3,5,6,8,9 and ratified D-ES-4 carry over
UNCHANGED and are not re-litigated.

**Scope-expansion note (HUMAN DIRECTIVE 2, 2026-08-25, VISION `8adabca`):** the substrate now serves
**three** ratified uses (§8B scope note). This adds **D-ES-11** (the `rmap find` contract — NEW; verb
human-ratified, contract challengeable) and the **use-(iii) cross-module concern-hint milestone cut**
(§11, follow-on `EMBED-CONCERN-1`). Per TD-015 the decision-review rerun for this cycle challenges
**only** these two new/changed items — D-ES-11 and the use-(iii) milestone cut. D-ES-1's STATUS is
extended (second human supersession, recorded, not a re-litigation). **All other cells (D-ES-2..10)
carry over UNCHANGED** and are not re-opened.

DECISION_REQUIRED:
- ID: D-ES-1  *(SUPERSEDED-BY-HUMAN 2026-08-25 — twice; recorded, not silently removed)*
  STATUS: **SUPERSEDED (two human directives).** (1) The first directive (VISION amended, commit
    `a3a90ce`, `docs/VISION.md:149-160`) **rejected a separate *fallback* verb entirely**: semantic
    candidates must integrate into the EXISTING resolution seams (that ratification role is taken over
    by **D-ES-10**). (2) HUMAN DIRECTIVE 2 (VISION amended, commit `8adabca`, binding) then **directly
    named an affirmative verb `rmap find`** for concept search (VISION use (ii)), **overriding this
    cell's rejection of `find`/`search`-named verbs** — the human ruled that the honesty mitigation is
    **the output itself** (`find` labels its results *hints — open the files*, never a completeness
    claim), so a search-named verb no longer violates I1. The `find` contract is **D-ES-11** (new); its
    verb+name are human-ratified, only the contract is decision-review-challengeable. This cell's own
    original question ("what to *name* the seeding verb") is thus doubly moot: neither a fallback verb
    (there is none) nor the affirmative verb (human-named `find`) is a builder naming choice. The
    original question/options are retained below for the audit trail only; they are **not a live
    decision** and the decision-review rerun does NOT re-open them (TD-015).
  QUESTION (obsolete): The verb name for the semantic-seeding candidate generator.
  OPTIONS (obsolete):
  - `seed` (was RECOMMENDED): reads as safe/read-only, "plant a starting point"; sibling to
    orient/explain; matched VISION § Protocol Surface Standard.
  - `locate`: also safe-sounding; slightly implies an exactness the Layer-3 hint does not have.
  - `find` / `search`: reads as an answer engine → would have violated I1. Rejected.
  RECOMMENDED (obsolete): `seed` — moot; no verb exists under the integrated design.
  BLOCKING_REASON: none — superseded; see D-ES-10.

- ID: D-ES-2
  QUESTION: Where do vectors live — a state-root sidecar or snapshot-DB rows?
  OPTIONS:
  - (a) `<state_root>/seed-vectors/<hash16>.vec` sidecar, warm-cache envelope (RECOMMENDED):
    the packet's stated option. REWARD — no SQL schema change (honors STOP_CONDITION);
    non-authoritative/safe-to-delete Layer-3 truth class; retention-independent; co-located with the
    snapshot DBs and keyed by the same `allocate_db_path` hash so forget/reclaim wire in trivially;
    pins map onto `validate_manifest`, so a store is either valid or discarded regardless of where the
    state root lives (§4.2, §7.3). RISK — a second per-repo file kind to keep coherent (bounded —
    warm-cache precedent) and a table-less family that requires a **cross-crate** registration touch
    (`artifact-contracts` + `repo-index`, §3.4), not a one-crate edit.
  - (b) new table under `snapshots`: REWARD — one storage substrate, snapshot-scoped lifecycle for
    free. RISK — **requires a SQL migration (this SPEC's STOP_CONDITION forbids), a retention
    integration, full persistence-completeness**; and it puts a Layer-3 hint whose floats are not
    cross-machine-reproducible (§7.3) into the authoritative snapshot DB, blurring the truth class.
    Rejected for IMPL-1.
  RECOMMENDED: (a).
  BLOCKING_REASON: Data shape + storage location is an architecture-boundary decision (CLAUDE.md
    § Decision Autonomy); (b) would also force a schema change this SPEC is barred from making.
  NOTE: Either option requires an `artifact-contracts` family contract (§3.4) — the family, not the
    table, is the architectural unit. That registration is mandatory, not a decision.

- ID: D-ES-3  *(carried unchanged; "first `seed`" reads as "first no-match fallback query" post-rework)*
  QUESTION: Refresh policy — background recompute-on-change at index/refresh, or on-demand at the
    first no-match fallback query? And the opt-out default.
  OPTIONS:
  - Background at index/refresh, recompute changed files by `content_hash`, default ON, env opt-out
    `RMAP_SEED_VECTORS` (RECOMMENDED): reuses ENRICH-LIFECYCLE-1 (spawn/cancel/detached); the first
    fallback query is fast; honest skip when no model (like enrich's no-resolver skip). REWARD — fast
    read, proven lifecycle. RISK — one background pass per index even if the fallback is never hit
    (opt-out mitigates).
  - On-demand at first fallback query: REWARD — zero cost until used. RISK — the first no-match query
    blocks ~minute (spike cold cost) on a read path; bespoke cancellation; unbounded staleness. Rejected.
  RECOMMENDED: Background at index/refresh, default ON, `RMAP_SEED_VECTORS` opt-out.
  BLOCKING_REASON: Refresh behavior is a persistence-completeness + daemon-lifecycle decision; the
    default-ON posture commits background compute on every index (blast radius beyond one command).

- ID: D-ES-4  *(DISTRIBUTION-LEVEL)* — **RATIFIED 2026-08-25 (human): option (a), the OpenAI-compatible local endpoint.** The embedded-ONNX path (b) stays a named, unbuilt alternative requiring its own future ratification.
  QUESTION: How does the local model run — an operator-configured OpenAI-compatible local endpoint,
    or an embedded ONNX runtime bundled in `rmapd`?
  OPTIONS:
  - (a) OpenAI-compatible local endpoint (LM Studio / Ollama, as the spike):
    REWARD — no heavyweight/model dependency and no model-distribution/signing burden (contrast (b));
    proven in the spike; model swappable by config. Its manifest footprint is **light but not zero** —
    one workspace-member line if D-ES-8 ratifies a new crate, plus at most one HTTP-client dependency
    if D-ES-9 ratifies a client crate over the recommended std-library transport (§11 states the exact
    range). RISK —
    (i) needs a running local server the operator starts, plus a small config surface (three env vars
    `RMAP_SEED_ENDPOINT` / `RMAP_SEED_MODEL_ID` / `RMAP_SEED_DIM`, §6.1 — no config file);
    (ii) **rmapd's loopback enforcement is partial, not end-to-end (review-3 item 2).** The IMPL
    enforces an **IP-literal allowlist (no name resolution) plus a proxy-disabled direct-connect
    client** (accept only `127.0.0.0/8` / `::1` IP literals; reject the *name* `localhost` and every
    DNS name; do not inherit `HTTP_PROXY`/`ALL_PROXY`; `NonLoopbackRejected` otherwise, §6.1/§10).
    This guarantees **rmapd's direct peer is always a loopback listener** and rmapd sends no byte
    off-host — but it **cannot** prove that listener is a local model and not a **local forwarding
    proxy** that egresses. End-to-end "no third-party egress" therefore rests on **trusting the local
    endpoint's behaviour** — a residual (a) cannot remove;
    (iii) **model identity is operator-asserted** (§6.1/§7.1): the pin is what the operator says the
    endpoint serves; the endpoint is only checked when it *echoes* a `model` field. Server down =
    "no hints". Only option (b) makes both local-processing and model-identity structural;
    (iv) **the HTTP transport is itself an unresolved dependency cell (D-ES-9).** The workspace has **no**
    outbound HTTP client today (OBSERVED, §6.1 point 3), so (a) either hand-rolls a std-library loopback
    transport (recommended, no new edge) or takes an HTTP-client dependency — a decision with its own
    dependency-graph blast radius, ratified separately in **D-ES-9**.
  - (b) Embedded ONNX runtime + bundled model in `rmapd`:
    REWARD — self-contained; **removes both option-(a) residuals structurally** — "local" processing
    is structural not configured (no listener, no socket, so no forwarding-proxy question, review-3
    item 2) and model identity is structural (rmapd *is* the model, so no operator-asserted pin,
    §7.1); no external process to manage. RISK — heavyweight new dependency (ONNX runtime + tokenizer)
    against binary-first distribution (VISION § Distribution); 84 MB+ model to
    distribute/version/notarize (MAC-2 surface); larger maintenance/attack surface (weighs against
    the safety-critical posture); and it needs `Cargo.toml` edits **this SPEC** is barred from making
    — so under (b) the impl ships in a **separate post-ratification distribution slice** (§11), not in
    this spec's scope. (This SPEC's STOP_CONDITION binds this SPEC, not a future slice; (b) is **not**
    foreclosed.)
  RECOMMENDED: **None — operator's distribution call.** Builder note: IMPL-1's runtime-agnostic core
    defines one internal `Embedder` port (§10, §11) so the choice is a **reversible seam** — the same
    core + port design serves either impl. Under (a) the endpoint impl and the core ship together and
    complete IMPL-1 as one slice; under (b) this option-(a) IMPL-1 does **not** run and the core ships
    **inside** the (b) distribution slice alongside the embedded impl (§11, review-6 #4), so the
    fallback tier is deep-vertical under either outcome — never a dormant core. Neither option is
    preselected; the builder does not bind the default.
  BLOCKING_REASON: New heavyweight binary dependency + model distribution/signing is a foundational,
    hard-to-reverse distribution decision (VISION § Distribution; CLAUDE.md § Decision Autonomy →
    stop and ask).

- ID: D-ES-5
  QUESTION: Corpus granularity for IMPL-1 — file-level only, symbol-level too, or both?
  OPTIONS:
  - File-level only (RECOMMENDED): best hit@5 (14/16), smallest store (~1 vec/file), needs only
    `files` + on-disk content. Symbol-level is a named extension point (§11). REWARD — smallest,
    best-measured. RISK — loses the one UI-phrasing miss symbol-level caught (acceptable — deferred).
  - Both (file + symbol max-per-file): REWARD — +1 recovered miss class. RISK — 5.6× store, +0
    hit@5, more `nodes` read + rollup logic for marginal gain. Deferred, not rejected.
  RECOMMENDED: File-level only for IMPL-1; symbol-level deferred (§11).
  BLOCKING_REASON: Corpus shape sets the store size, the refresh unit, and the acceptance numbers —
    a contract the IMPL builds against.

- ID: D-ES-6
  QUESTION: Corpus exclusions — which files are seedable?
  OPTIONS:
  - Exclude `is_test=1`, `is_generated=1`, `is_excluded=1` (RECOMMENDED): reuses existing scanner
    flags (`rust/crates/storage/src/migrations/001-initial.sql:50-52`), no new heuristic; matches the spike's test/e2e exclusion.
    REWARD — honest reuse of SCANNER-GITIGNORE-1 truth. RISK — a repo that *wants* to seed into a
    generated client won't — acceptable, revisit on evidence.
  - Include generated (exclude only tests): REWARD — seeds generated API clients. RISK — pollutes
    candidates with machine-authored files; against orientation value. Rejected default.
  RECOMMENDED: Exclude test + generated + excluded; "vendored" = whatever `is_excluded` already
    marks (no new vendoring logic).
  BLOCKING_REASON: A product decision about what the agent should be pointed at; cheap to unwind but
    sets the corpus contract.

- ID: D-ES-7  *(output-shape cells UPDATED 2026-08-25 for the seam integration; follow-up re-worded to the ratified SEQUENCE — OPERATOR RULING 5, `docs/VISION.md:159-164`)*
  QUESTION: The per-candidate output shape. (The amended VISION, `docs/VISION.md:159-164`, fixes the
    shape as a **bound**: each candidate carries *score + provenance + module/path* directly, and
    names the deterministic follow-up *sequence* — `explain <candidate>` for imports + symbols, then
    one further `explain` on a listed symbol for callers (no single command yields file-level
    callers, §8.2 code-truth note). What remains recordable is the concrete field set and the reuse of
    each seam's existing carrier.)
  OPTIONS:
  - Lightweight candidate + named `next` follow-up **sequence** (RECOMMENDED — the amended-VISION shape):
    each candidate = `{ stable_key, file/path, score, source:"embedding", model_id, module,
    next:{cmd,args,cwd} }`, additive on the existing `FocusCandidate` (Group A) / error-`data`
    (Group B); `module` from `module_summary::aggregate_file`, `stable_key` from `resolve_path_focus`,
    `next` = the first hop `explain <key>` / `orient --focus <key>` (real syntax + explicit `cwd`,
    §8.2). Hop 1 (`next`) yields imports + symbols; **callers are hop 2** — one further
    `explain <symbol-name>` on a symbol hop 1 lists (hop-1 output exposes names, not stable keys,
    `signal.rs:783-787`; hop 2 returns callers only on a unique name, else the existing ambiguity
    result, §8.2a). REWARD — matches the amended VISION exactly;
    **no new domain logic** (no caller fold); ≤10 lookups across ≤5 candidates; every field is either
    stored or a single existing call; reuses each seam's carrier so the deterministic path is
    byte-unchanged. RISK — (i) the neighbourhood costs a follow-up **sequence** (two hops, the amended
    VISION's explicit choice; the candidate stays a pure generator); (ii) two **code-truth IMPL edits**
    the additive-field claim entails (§8.2 note): `FocusCandidate` must drop its `Eq`/`PartialEq` derive
    (envelope.rs:54) to carry a float `score`, and a new `LimitCode` variant set (`SemanticFallback` /
    `SemanticFallbackUnavailable`) must be added (limit.rs:35-187) for the labeled `Limit` — named IMPL
    work, not "unchanged code".
  - ⟨SUPERSEDED — Inline module + ≤8 imports + ≤8 symbols + ≤8 aggregated callers (the pre-rework
    caller fold, §8.2 old step 5)⟩: the amended VISION replaced inlining with the named command, so
    this is **removed, not chosen** — it built genuinely-new file-level aggregation logic the
    integrated design does not need.
  RECOMMENDED: Lightweight candidate (score + provenance + module/path) + named `next` follow-up,
    reusing each seam's existing carrier (`Focus.candidates` for Group A; error `data` for Group B).
  BLOCKING_REASON: Sets the agent-consumed candidate contract inside the existing seams. (The field
    *set* is the amended-VISION bound; what is recorded here is the concrete shape + carrier reuse.
    Tightly coupled to D-ES-10, which fixes when/where the tier fires.)

- ID: D-ES-8  *(component-graph boundary — new release unit)*
  QUESTION: Does the pure seed logic (corpus build + envelope + cosine ranking) live in a new crate
    `repo-graph-seed`, or as a support module inside an existing pure crate?
  CORPUS-READ BOUNDARY (fixed regardless of this cell — not itself a ratification choice, review-5
    item 1): the seed logic is INNER/pure and MUST NOT depend on `repo-graph-storage` (the outer
    SQLite adapter, which itself depends on `rusqlite` + `repo-graph-agent` + `repo-graph-indexer`:
    `rust/crates/storage/Cargo.toml:60-161`). It therefore **defines a policy-side `SeedCorpusRead`
    port + a raw `SeedCorpusEntry { file_uid, path, content_hash }` DTO** (§10 ledger); the storage
    adapter adds a dependency on the seed unit and implements the port (adapter → policy, outer →
    inner — identical to `AgentStorageRead`, `rust/crates/storage/Cargo.toml:155-161`). This is the
    established dependency-inversion pattern, not a new boundary decision, and it holds under BOTH
    options below.
  OPTIONS:
  - New crate `repo-graph-seed` (RECOMMENDED): mirrors the ratified `repo-graph-warm-cache` split
    (pure envelope crate + `daemon-runtime` wiring). REWARD — clean cohesion, a headless test seam
    isolated from the daemon, dependency rule respected. RISK — a permanent new release unit +
    component-graph node + versioning surface; the graph must stay a DAG (it does: `repo-graph-seed`
    depends only on `serde`/`serde_json` + `repo-graph-warm-cache`-style primitives and **defines**
    `SeedCorpusRead`; the only new edge is `repo-graph-storage → repo-graph-seed`, and nothing depends
    back into `daemon-runtime`).
  - Support module inside an existing pure crate (e.g. `agent`): REWARD — no new graph node; **now
    coherent** because the seed logic reaches its corpus only through the `SeedCorpusRead` port it
    defines and never imports `storage`, so it does not violate `agent`'s ratified
    no-dependency-on-`storage` boundary (`rust/crates/agent/Cargo.toml:39-41`). RISK — dilutes that
    crate's cohesion with an unrelated capability; the seed logic's test seam and its `Embedder`
    dependency land in a crate that has neither.
  RECOMMENDED: New crate `repo-graph-seed`, on the warm-cache precedent.
  BLOCKING_REASON: A new crate boundary / component-graph edge is an architecture-boundary decision
    (CLAUDE.md § Decision Autonomy → stop and ask); it is a permanent release unit, not a local
    detail. (The `SeedCorpusRead` inversion above is NOT part of what is being ratified here — it is
    the mandated pattern; only the crate-vs-module home is the open cell.)

- ID: D-ES-9  *(dependency-graph boundary — option-(a) HTTP transport; builder RECOMMENDS, operator ratifies)*
  QUESTION: Option (a) calls an OpenAI-compatible `/v1/embeddings` endpoint over HTTP, but the workspace
    has **no** outbound HTTP/TLS client today (**OBSERVED:** a deterministic scan of every
    `rust/**/Cargo.toml` finds no `reqwest`/`ureq`/`hyper`/`rustls`/`native-tls`; the daemon is
    Unix-socket based and the spike used Python `requests`). How does the (a) `Embedder` impl issue the
    request — adopt a Rust HTTP-client dependency, or hand-roll a std-library loopback transport?
  OPTIONS:
  - (a1) Adopt a minimal Rust HTTP client (e.g. `ureq`; `reqwest` is heavier):
    REWARD — robust, well-tested HTTP/1.1 (handles `Content-Length` **and** chunked responses, timeouts,
    keep-alive) and, with a TLS feature, supports the `https`-to-loopback scheme (§6.1); almost no
    hand-written wire code. RISK — a **new dependency edge** in the component graph (architecture-boundary
    per CLAUDE.md § Decision Autonomy) — a permanent supply-chain + attack surface that weighs against the
    safety-critical / binary-first posture (VISION § Distribution); a TLS feature drags in a transitive
    TLS stack to serve locally-terminated TLS, a case no current evidence demands.
  - (a2) Hand-roll a std-library transport (RECOMMENDED): one POST to the loopback IP over
    `std::net::TcpStream`, the HTTP/1.1 request framed by hand, request/response JSON via `serde_json`
    (already a workspace dependency — OBSERVED in 32 crate manifests, so **no** new dep). REWARD —
    **zero new dependency edge** (smallest supply-chain surface, VISION-aligned); *inherently*
    proxy-free/direct-connect (§6.1 point 3 has nothing to disable); the spike's endpoint is plain
    `http://localhost:1234` loopback (`tools/embed-seed-spike/spike.py:18`), so http-only costs no
    demonstrated case. RISK — a hand-framed HTTP/1.1 reader accepts only a **bounded response shape**;
    it is NOT a general HTTP client. **Evidence honesty (review-5 item 2):** the spike proves nothing
    about wire framing — it used Python `requests` (`tools/embed-seed-spike/spike.py:13,101-104`:
    `requests.post(...).json()`), which transparently handles `Content-Length` **and** chunked
    transfer-encoding, TLS, and redirects, so the endpoint's actual framing was never OBSERVED. That
    LM Studio / Ollama emit `Content-Length`-delimited JSON for a small embeddings response is
    **INFERRED** (typical for a fixed-size JSON body), **not demonstrated**. a2 therefore does not
    *assume* the framing — it **validates the response against an explicit accepted-input contract and
    degrades honestly on any mismatch** (contract below). If a real endpoint returns chunked / TLS /
    non-200, a2 yields "model unavailable" (never a wrong answer, never a crash), which is the
    demonstrated variation that flips ratification to (a1).
  A2 ACCEPTED-RESPONSE CONTRACT (the exact bounded shape a2's reader parses; everything else →
    honest degradation, never a panic, never a partial-body parse — I2 candidate-never-answers +
    architecture.md Honest Degradation Rule):
    - **Transport/scheme:** plain `http` over `TcpStream` to a loopback IP literal (§6.1 points 1–2).
      A `https`/TLS endpoint is **out of a2's contract** → `NonLoopbackRejected` is not it; TLS bytes
      fail to parse as HTTP/1.1 → `EmbedError::Malformed` → "model unavailable".
    - **Status line:** HTTP/1.1 (or 1.0) `200 OK` only. Any non-200 (4xx/5xx, incl. model-not-loaded)
      → `Malformed { detail: "HTTP <code>" }` → "model unavailable".
    - **Framing:** a `Content-Length` header with a non-negative integer value; the body is read as
      **exactly** that many bytes. **`Transfer-Encoding: chunked` is explicitly unsupported** →
      detected by header and rejected as `Malformed { detail: "chunked unsupported" }` (a2 does NOT
      attempt de-chunking — reading a chunked body as Content-Length would corrupt the JSON). A
      missing/duplicate/non-integer `Content-Length` → `Malformed`.
    - **Bounds (fixed mechanism constants, DoS/hang guards):** connect timeout and read timeout via
      `TcpStream::connect_timeout` + `set_read_timeout` (**timeout → `Unreachable`**, "model
      unavailable"). **Exact limits (review-6 #2 — fixed HERE so independent implementations are
      compatible; each violation ⇒ `Malformed`, never an unbounded read or a silent truncation):**
      connect timeout **2 s**; read timeout **30 s** per request; header section ≤ **64 KiB** before
      the blank line; response body ≤ **32 MiB** (a full 32-document × 768-dim JSON batch is ≈ 0.3
      MiB — the cap is two orders above need and still bounded).
    - **Body:** parsed with `serde_json` into the OpenAI-shaped
      `{ data: [ { index, embedding: [f32…] } ] }`; any parse failure or shape mismatch ⇒ `Malformed`.
      **Response correlation (review-6 #1 — positional `zip` is a spike shortcut, not a production
      contract):** `data.len()` MUST equal the request's input count, every entry MUST carry an
      integer `index`, and the indices MUST form a unique permutation of `0..n` — vectors are
      correlated to inputs **by `index`**, never by array position. Missing/duplicate/out-of-range
      indices, or a cardinality mismatch ⇒ `Malformed` (the whole batch is discarded — a partial
      batch is never stored, so a vector can never bind to the wrong `content_hash` pin).
      **Vector sanity:** any non-finite component (NaN/±Inf) or a zero-norm vector ⇒ `Malformed`
      (a zero-norm vector would make cosine undefined and ranking unstable). Vector length ≠ pinned
      `dim` ⇒ `DimMismatch`; present-and-different top-level `model` ⇒ `ModelMismatch` (§7.1). All
      map to §8.3 degraded states, and each case (cardinality, index permutation, non-finite,
      zero-norm, dim, model) is a named IMPL validation test.
    Net: a2 accepts a narrow, explicitly-bounded loopback-`http` + `Content-Length` + 200 + OpenAI-JSON
    response and treats **every** deviation (chunked, TLS, non-200, oversize, timeout, malformed) as
    "model unavailable" — the candidate generator declines rather than guesses. (a1)'s value is exactly
    that it removes these limits by using a real HTTP client.
  PROBE (EXECUTED 2026-08-25, the tiebreaker both agents asked for): `curl -v` against the
    operator's real LM Studio `/v1/embeddings` → `HTTP/1.1 200`, `Content-Length: 23333`,
    no `Transfer-Encoding: chunked`, plain `http` — the exact subset a2 accepts. a2's framing
    assumption is now MEASURED for the demonstrated endpoint; (a1) remains the named fallback
    the day a chunked/TLS endpoint is a real requirement.
  RECOMMENDED: **(a2) std-library transport** — the smallest design that satisfies the demonstrated need
    (plain-`http` loopback, proven in the spike) with **no** dependency-graph edge, given that a2 states
    its compatibility limits explicitly and degrades honestly outside them. (a1) is the named,
    ratification-ready fallback the moment a TLS or chunked-encoding endpoint is a real requirement.
    Builder RECOMMENDS but does not bind: adopting a client dependency crosses an architecture boundary.
  BLOCKING_REASON: Adding an outbound HTTP/TLS client dependency is a new component-graph edge and a
    distribution/supply-chain posture change (CLAUDE.md § Decision Autonomy → stop and ask; VISION §
    Distribution). It also fixes the §6.1 scheme set (http-only vs http+https) AND a2's accepted-response
    contract above (which real endpoints it can and cannot talk to), so it must be ratified before the
    (a) impl is built. Applies **only under D-ES-4 option (a)**; moot under (b).

- ID: D-ES-10  *(NEW 2026-08-25 — takes over D-ES-1's ratification role; seam enumeration + fire condition + integrated envelope)*
  QUESTION: Now that the human directive requires integration into the EXISTING resolution seams
    (no verb, `docs/VISION.md:149-160`): **which seams carry the semantic fallback tier, exactly when
    does it fire, and in which existing carrier?**
  CONSTRAINTS (from the human directive — these are bounds, not open sub-choices):
  - The tier fires **only after every deterministic tier yields zero matches** (Group A's no-match
    branch / Group B's `SymbolResolveError::NotFound` branch, surfaced today as `InvalidRequest`,
    §8.2) — it **never reorders or dilutes** the deterministic tiers, and
    **ambiguous-with-exact-matches does NOT fire it** (ambiguity already has exact matches).
  - Additive + labeled: each candidate carries `source:"embedding"` + `model_id`; caps ≤5; degraded
    (no vectors / model down / pins mismatch) ⇒ the seam behaves **exactly as today** plus one
    labeled line stating the fallback was unavailable and why.
  OPTIONS:
  - (RECOMMENDED) Wire the tier into **both** enumerated seam-groups (§8.1), building **Group A now**
    and **Group B as a deferred cut** (§11):
    - **Group A — `orient`/`explain` focus resolution.** Fire at the deterministic-zero no-match:
      `orient` `rust/crates/agent/src/orient/mod.rs:251-256` (+ defensive `:214`,`:273`, via
      `build_no_match_result` `:369`); `explain` `rust/crates/agent/src/explain/mod.rs:187` (+ `:154`,
      `:204`, via `build_no_match` `:259`). Do NOT fire on ambiguous (`orient` `:278-287`
      `build_ambiguous_result`; `explain` `:208-223`). Carrier: the previously-empty
      `Focus.candidates` (`rust/crates/agent/src/dto/envelope.rs:55-121,174-210`), additive
      semantic-only fields (`skip_serializing_if` absent on deterministic candidates) + a labeled
      `limits` line. REWARD — the seam's contract *already is* a `candidates` array; one integration
      point (the shared `Focus`/`FocusCandidate` DTO) serves both verbs; deterministic path
      byte-unchanged. RISK — extends a boundary DTO (`FocusCandidate`) with optional fields (directed
      by the human; additive, so no consumer breaks).
    - **Group B — `callers`/`callees`/`path` symbol lookup (deferred).** Fire on
      `SymbolResolveError::NotFound` only (`rust/crates/daemon-runtime/src/dispatch.rs:1268`,`:1438`,
      `:2479`,`:2502`; sum type `rust/crates/storage/src/queries.rs:568`); do NOT fire on
      `Ambiguous` (`:1274`,`:1445`,`:2485`,`:2508`). Carrier: the existing `symbol not found`
      error's additive `data.semantic_candidates` + `data.hint` (the same `ErrorDetail.data`
      mechanism `ambiguous_symbol` uses, `dispatch.rs:151`). REWARD — reuses the not-found error as
      the no-match contract. RISK — a file-level store answers a *symbol* query only with *files*
      (weaker fit) and the carrier differs from Group A → **deferred to a follow-up cut**, specified
      but not built in IMPL-1.
  - Group A only, never Group B: REWARD — smallest surface. RISK — leaves the VISION-named
    `callers`/`callees`/`path` no-match seams without the tier permanently; rejected — specify all,
    defer the build.
  - Fire the tier on ambiguity too (populate alongside exact matches): REWARD — more hints. RISK —
    **violates the human directive** (dilutes a deterministic tier that already has exact matches);
    rejected.
  RECOMMENDED: Wire both seam-groups per §8.1; **build Group A in IMPL-1** (the smallest
    deep-vertical — one `Focus`/`FocusCandidate` integration point across `orient`+`explain`),
    **defer Group B** (§11) as a specified follow-up cut.
  BLOCKING_REASON: This is the load-bearing contract cell of the reworked slice — it fixes the public
    behavior of five existing commands' no-match branches and extends a boundary DTO
    (`FocusCandidate`). It replaces D-ES-1's ratification role and must be ratified before IMPL.

- ID: D-ES-11  *(NEW 2026-08-25 — HUMAN DIRECTIVE 2; the `rmap find` affirmative concept-search verb, VISION use (ii))*
  STATUS: **The verb and its name are HUMAN-RATIFIED** (VISION amended, commit `8adabca`, binding);
    the human directly named `find` and overrode D-ES-1's rejection of a search-named verb. What is
    **decision-review-challengeable here** is the *contract*: the envelope shape, the ≤10 cap, the
    honesty mitigation, the degraded states, and the additive dispatch/witness surface (§8B). The verb
    **shares the §8 substrate** (store §4, pins §7.1, corpus §3, ranking §7.2) and the §8 degradation
    **causes/state taxonomy** (§8.3) — this cell adds **no** new store/pin/ranking/degradation-*detection*
    decision; only the degraded *rendering* is `find`'s own (§8B.2/§8B.3).
  QUESTION: What is the `rmap find "<concept>"` output contract, and how does its honesty hold under
    I1 (candidate-generator-never-answer) given that it is a deliberately search-named verb?
  OPTIONS:
  - (RECOMMENDED) `find` defines its **own** response DTO carrying the §8.2 candidate FIELDS —
    `stable_key, path (a plain String, not FocusCandidate.file), score (f64, no Eq),
    source:"embedding", model_id, module, next` (the exact struct in §8B.2) with an
    **always-present `candidates: []`** (plain `Vec`, no `skip_serializing_if` — §8B.3) — cap **≤10**
    (human-directed bound, HUMAN DIRECTIVE 2), and lead with a
    Layer-3 honesty header `summary:"likely areas for \"<concept>\" (semantic hints — open the files)"`
    that **never** claims completeness; degraded states share §8.3's causes/taxonomy but render `find`'s
    own always-present `[]` + `summary` (§8B.3); the only new surface is the
    verb + one CLI arm + one daemon arm + one witness-manifest line (§8B.4). REWARD — the honesty
    mitigation is **the output itself** (I1 held by the surface declaring its own Layer-3 status +
    pointing at files, per the human directive), zero new substrate, smallest possible new surface for
    a human-ratified verb, and full reuse of the ranking/store/pins/degradation the fallback tier
    already proves. RISK — a search-named verb is inherently more prone to being *read* as an answer
    engine than `orient`/`explain`; mitigated (not removed) by the mandatory header + per-candidate
    provenance + `next` follow-up — the residual is that a caller can still ignore the label (the same
    residual every Layer-3 surface carries).
  - Richer/answer-shaped envelope (inline snippets, ranked "best match", a single top answer): REWARD —
    denser first-screen. RISK — **violates I1** (presents a Layer-3 hint as an answer), and builds new
    inline aggregation the substrate does not have; rejected.
  - Different cap (≤5 like the fallback tier, or unbounded): REWARD — symmetry with §8 / more hits.
    RISK — ≤5 under-serves an affirmative "show me the areas" query (the human bound is use-(ii)'s ≤10);
    unbounded violates the no-silent-caps + bounded-output discipline. Rejected — ≤10 is the human-directed bound.
  RECOMMENDED: `find`'s own DTO carrying the §8.2 candidate FIELDS (`path` locator, `f64` score,
    always-present `candidates: []`, §8B.2/§8B.3) + mandatory Layer-3 header + ≤10 cap + §8.3-shaped
    degraded states; add the verb via the one CLI arm / one daemon arm / one witness-manifest line
    enumerated in §8B.4, under the standing Protocol Surface Standard verb-naming pre-ratification.
  BLOCKING_REASON: The verb existence/name is already human-ratified, but its **output contract** sets
    a new public surface (a new verb + its envelope) and is the I1-honesty-bearing decision — so it is
    recorded here for the decision-review rerun (which challenges **only** the new/changed cells:
    D-ES-11 + the use-(iii) milestone cut, §11). The substrate cells (D-ES-2..10) are **not**
    re-litigated (TD-015). Use (iii)'s rendering-surface decisions are **not** taken here — they are a
    named follow-on milestone `EMBED-CONCERN-1` (§11) with its own DECISIONS.

---

## 13. Stop-condition assessment (packet)

- **No production code / no `Cargo.toml` / no SQL schema change** — honored: this is the spec doc
  only; the recommended storage (D-ES-2 (a)) is chosen precisely to avoid a SQL migration, and the
  `SeedVectors` family registration (§3.4) is Rust code across `artifact-contracts` + `repo-index`
  (enum/match/const/test edits), **not** a migration. **Honest note (review-4):** the future
  EMBED-SEED-IMPL-1 *does* edit `rust/Cargo.toml` (a workspace-member line if D-ES-8 picks the new
  crate; a dependency edge if D-ES-9 picks the (a1) client), and (b) needs heavier manifest edits still
  — but every one of those is a **future IMPL under its own packet**, not this SPEC; this SPEC edits no
  manifest. The earlier "only (b) needs `Cargo.toml`" framing was wrong and is corrected in §11 / D-ES-4
  / D-ES-9.
- **Existing-code claims verified by reading** — all `file:line` anchors are OBSERVED (§0); the
  claims the packet/VISION named but that diverge from code are corrected in-line (`content_sha` →
  `file_versions.content_hash`; `files` has no `snapshot_uid`; the neighbourhood is yielded by the
  candidate's named `explain`/`orient` follow-up, not inlined (§8.2); the two seam-groups' fire
  points and carriers are enumerated against code in §8.1/D-ES-10; the `ArtifactFamily` match is
  **not** single-crate — a second exhaustive match lives in `repo-index`, §3.4; the state root is
  **not** structurally "never synced" — `RMAP_STATE_ROOT` is arbitrary, §4.2).
- **No false trust/certainty claim** — §9: seeding is Layer-3, contributes nothing to trust or
  reliability; §7.3 makes only the honest claim — ranking is **exactly reproducible within one
  store** and **not guaranteed at any margin across machines / re-embeds** (ε=1e-5 is a
  non-guaranteeing near-tie *advisory*, not a stability guarantee; a measured cross-machine envelope
  is a deferred calibration slice) — rather than overclaiming cross-machine determinism; §4.2
  downgrades the state-root privacy claim to a default-location (INFERRED) fact and proves safety is
  location-independent.
- **Out of scope (untouched):** `rust/`, `docs/VISION.md`, `docs/ROADMAP.md`, the agent-manager
  repo (packet FILES_OUT_OF_SCOPE).

---

## 14. Definition of done (this SPEC)

`docs/slices/embed-seed-1.md` exists, buildable, self-contained, with real `file:line` anchors and
the `## DECISIONS` section (D-ES-1 **superseded** (twice), D-ES-2,3,5,6,8,9 carried, D-ES-4
**ratified**, D-ES-7 **updated**, D-ES-10 **new**, **D-ES-11 new** (the `rmap find` contract, §8B) —
each live cell with risk/reward), plus the **use-(iii) cross-module concern-hint follow-on milestone**
(`EMBED-CONCERN-1`, §11). review-impl approves → decision-review reruns **only the changed/new cells**
(this cycle: D-ES-11 + the use-(iii) milestone cut; prior cycle: D-ES-1/7/10 — TD-015) → produces the
ratification packet → **halt at awaiting-ratification for the human.** The IMPL slice
(EMBED-SEED-IMPL-1, §11) ships uses (i) the §8 fallback tier **and** (ii) the §8B `rmap find` verb, and
runs only after D-ES-7 + D-ES-10 + D-ES-11 (and the carried D-ES-2,3,5,6,8,9) are ratified.

---

## 15. Change log — iteration 1 (resolves review-0.json)

Each item cites the review-0 correction it closes:

1. **Storage moved to the STATE ROOT (review-0 item 1).** §4 now recommends
   `<state_root>/seed-vectors/<hash16>.vec` (keyed by `allocate_db_path`'s hash, `rust/crates/daemon-runtime/src/registry.rs:542-548`),
   the packet's stated option — not the repo-local `.rgr/`. Path derivation, deletion/forget
   semantics, and the machine-locality argument are rewritten (§4.1–4.2).
2. **Artifact-family contract added (review-0 item 2).** §3.4 defines the `SeedVectors` family's
   truth class, identity, provenance, refresh/impact, freshness, degradation, and the mandatory
   `artifact-contracts` registry entry (schema-free but contract-bearing), plus the `table_name()`
   friction and its bounded IMPL resolution.
3. **Neighbourhood composition corrected against real surfaces (review-0 item 3).** §8.2 replaces
   the false "orient_file gives imports+callers" with the actual functions: `find_file_imports`
   (`rust/crates/agent/src/explain/mod.rs:544`) + `list_symbols_in_file` (`rust/crates/agent/src/explain/mod.rs:567`) for a file candidate;
   callers are symbol-granularity (`find_symbol_callers`, `rust/crates/agent/src/explain/mod.rs:324`) and referred via
   `next`. D-ES-7 updated.
4. **Exact bytes/spans/caps (review-0 item 4).** §3.2 pins the document format, the `1..=min(60,
   line_count)` span, and the 6 000-char input cap; §8.4 tabulates every numeric cap (corpus 50k,
   batch 32, candidates ≤5, imports ≤8, symbols ≤8) and which are ratification-class.
5. **Source/snapshot race closed (review-0 item 5).** §3.5 requires the pass to re-hash the exact
   embedded bytes with `hash_content` (`rust/crates/repo-index/src/scanner.rs:66-74`) and admit only on match with the READY
   snapshot; §4.3/§5.1 specify atomic `atomic_write` publication and no-publish-on-cancel.
6. **Float tolerance + tie ordering (review-0 item 6).** §7.3 defines within-store exact
   `(-score_f32, path)` reproducibility and a cross-store ε=1e-5 (INFERRED) envelope with explicit
   membership/order semantics.
7. **Abstractions repaired (review-0 item 7).** The `repo-graph-seed` crate boundary is now a
   ratification cell (D-ES-8); the `Embedder` port is `Result`-typed with a contextual error sum
   (`Unreachable`/`NonLoopbackRejected`/`Malformed`/`DimMismatch`/`ModelMismatch`) and a raw-DTO
   boundary (§10).

---

## 16. Change log — iteration 2 (resolves review-1.json)

Each item cites the review-1 correction it closes; every new/changed claim is OBSERVED against the
working tree at spec time (anchors below).

1. **Inline deterministic callers restored (review-1 item 1).** `docs/VISION.md:152-154` ratifies
   the seed neighbourhood as `(module, imports, callers)` — a higher-priority bound than the
   builder's iteration-1 caller *referral*. §8.2 now inlines callers as a **deterministic file-level
   fold** over `find_symbol_callers` (`rust/crates/agent/src/storage_port.rs:737/461-467`) across the ≤8 in-file symbols,
   ranked by the same `call_ranking::rank_caller_rows` total order `explain_symbol` uses
   (`rust/crates/agent/src/explain/call_ranking.rs:15-17,56-84`; call site `rust/crates/agent/src/explain/mod.rs:324`), union + first-occurrence
   dedup by `stable_key`, capped ≤8, with explicit unknown-vs-zero semantics via `symbols_scanned`
   (`applicable:false` when the file has no SYMBOL nodes). §8.4 adds the fan-out (≤8) and caller
   (≤8) caps; D-ES-7 flips its recommendation to inline-aggregated-callers and reframes the *set* as
   a VISION bound, not a decidable option; §10/§11 updated; the `next` referral is retained as an
   *addition* for the untruncated/per-symbol set, not a substitute.
2. **`next` commands made valid and self-contained (review-1 item 2).** The iteration-1 strings
   `explain glamCRM <key>` / `orient glamCRM <key>` are invalid: `run_explain_cmd` takes exactly one
   positional and no repo arg (`rust/crates/rgr/src/commands/orient.rs:440-451`), `orient` takes no positionals (focus is
   `--focus <val>`, `rust/crates/rgr/src/commands/orient.rs:95-125`), and both resolve the repo from cwd (`rust/crates/rgr/src/commands/orient.rs:160-172`;
   `explain` at `:488`).
   §8.2 now emits `next` as **structured `{cmd, args, cwd}`** with the real syntax (`explain <key>`;
   `orient --focus <key>`) and an explicit `cwd` = the resolved repo root, so a `seed --repo` result
   is followable regardless of the shell's cwd.
3. **Artifact-contract integration scope corrected to cross-crate (review-1 item 3).** §3.4 replaces
   the false "local to `artifact-contracts` … one-crate mechanical detail" with the OBSERVED touch
   set: `family.rs` enum/`table_name()`(`:128-157`)/`all()`, `rust/crates/artifact-contracts/src/registry.rs:16` `get_contract()`,
   `rust/crates/artifact-contracts/tests/coherence.rs:265-279` `table_names_are_valid`, **and** `repo-index`'s second exhaustive
   match `rust/crates/repo-index/src/impact_propagation.rs:154-181` `family_to_table()` plus the `rust/crates/repo-index/src/compose.rs:3755-3822`
   copy-forward wildcard and `rust/crates/repo-index/src/refresh_policy.rs:43-77` arrays that must explicitly **exempt** the
   sidecar family. The `table_name()` `Option`-vs-sentinel resolution is now surfaced as a real
   cross-crate decision with test blast radius, not a local detail; §11 folds the audit into IMPL-1.
4. **Endpoint configuration contradiction resolved (review-1 item 4).** New §6.1 specifies the
   D-ES-4(a) config inputs — env vars `RMAP_SEED_ENDPOINT` / `RMAP_SEED_MODEL_ID` / `RMAP_SEED_DIM`
   (house pattern, `rust/crates/daemon-runtime/src/enrich_pass.rs:142`) — and the loopback enforcement policy: a **host-literal
   allowlist with no DNS resolution** (`127.0.0.0/8`/`::1`/`localhost`; scheme `http`/`https` only;
   `NonLoopbackRejected` otherwise) that makes I4 structural. §10 corrects the "no config layer
   beyond one env opt-out" claim to "four env vars, no config subsystem"; D-ES-4(a) risk cell updated.
5. **State-root certainty language corrected (review-1 item 5).** §4.1/§4.2 remove the unproven
   "never synced or shipped" assertion: `RMAP_STATE_ROOT` accepts an arbitrary path
   (`rust/crates/daemon-runtime/src/registry.rs:558-566`). The claim is downgraded to a **default-location (INFERRED)** fact
   (platform data dir, `rust/crates/platform-paths/src/dirs.rs:22-36`, not inside any repo tree), and safety is
   shown to be **location-independent** — a synced/copied sidecar either validates against its pins
   or is discarded → "no hints" (§4.3/§8.3), with no stale-serving path. §7.3 and D-ES-2 de-coupled
   from the machine-locality argument.

---

## 17. Change log — iteration 3 (resolves review-2.json)

Each item cites the review-2 correction it closes; every new/changed claim is OBSERVED against the
working tree at spec time (anchors verified by `grep -n`).

1. **Model-id contradiction fixed (review-2 item 1).** The pinned / persisted / configured /
   reader-facing model id is now the **single exact string `text-embedding-nomic-embed-text-v1.5`**
   — the id the spike's endpoint served (`tools/embed-seed-spike/spike.py:17`, `MODEL = "text-embedding-nomic-embed-text-v1.5"`).
   §6.1's `RMAP_SEED_MODEL_ID` default, the §8.2 `--json` examples, and the §9 human label all now
   carry that one string; the added note in §6.1 explains the underlying HF model is *named*
   `nomic-embed-text-v1.5` but the pin must equal what the endpoint returns (else the
   `(model_id, dim, content_sha)` hard-fail, I3, misfires). No label↔id split was introduced —
   rejected as structure with no caller; one string is the smaller, honest design.
2. **Local-only enforcement made structural — literal-IP-only + no-proxy (review-2 item 2).** §6.1's
   allowlist iteration 2 admitted the *name* `localhost` "by contract"; that is not enforceable (the
   OS resolver / `/etc/hosts` can be re-pointed, and a proxy-honouring client can divert even a
   `127.0.0.1` request off-host). Corrected to: **accept only IP literals** in `127.0.0.0/8` / `::1`
   (parsed, `is_loopback()`), **reject every name including `localhost`** (no DNS), **and construct
   the (a) HTTP client proxy-free / direct-connect** (no `HTTP_PROXY`/`ALL_PROXY` inheritance). The
   default endpoint is already the literal `http://127.0.0.1:1234/…` so the out-of-box case passes.
   Degraded-state tests updated to assert `localhost`/public-IP/DNS-name → `NonLoopbackRejected`,
   IP literals accepted, and proxy-disabled even with `HTTP_PROXY` set. D-ES-4(a) risk cell updated.
   (The no-proxy client construction is INFERRED — there is no existing outbound-HTTP client in the
   tree to model on; the exact builder call is named in the IMPL task packet.)
3. **Caller-fold ordering corrected to the real total order (review-2 item 3).** The in-file symbol
   order is **`line_start` ASC, then `name`, then `stable_key`** — verified at
   `rust/crates/agent/src/ordering.rs:148-155` (`sort_explain_symbols`:
   `line_key(line_start).then(name).then(stable_key)`) and `rust/crates/agent/src/explain/mod.rs:570-572`. §8.2 step 4, the
   step-5 fan-out set, and the step-5 union/dedup iteration order now all name `stable_key` as the
   third, unique tiebreak, so the "total order / no jitter" claim is complete. The
   `ordering::sort_explain_symbols` follow-on citation now carries its own `rust/crates/agent/src/ordering.rs:148-155`
   anchor.
4. **Sidecar envelope header completed (review-2 item 4).** §4.3 iteration 2 listed only
   `magic`/`schema_version`/pins but its validation checked `content_length` and `checksum`. The
   header now reproduces **all seven `CacheManifest` fields field-for-field**
   (`rust/crates/repo-graph-warm-cache/src/lib.rs:154-170`): `magic`, `schema_version` (const `rust/crates/repo-graph-warm-cache/src/lib.rs:60`),
   `key` (`model_id`/`dim`/`repo_graph_version`), `created_at` (metadata only), **`content_length`
   (`u64`, payload byte count, `rust/crates/repo-graph-warm-cache/src/lib.rs:166-167`)**, and **`checksum` (hex SHA-256 of the payload
   bytes, `rust/crates/repo-graph-warm-cache/src/lib.rs:168-169`)** — with the payload defined as the opaque body those two fields cover,
   so corruption validation is buildable.
5. **All existing-code anchors expanded to full repo-relative `file:line` (review-2 item 5).** Every
   abbreviated anchor (`registry.rs`, `lib.rs`, `explain/mod.rs`, `orient.rs`, `scanner.rs`,
   `family.rs`, `spike.py`, the `<crate>/src/…` short forms, and the `ordering::…`/`tests/…`
   follow-ons) was rewritten to a full `rust/crates/…` / `tools/…` / `docs/…` path spot-checkable
   from the repo root. `registry.rs` was disambiguated per-occurrence between
   `rust/crates/daemon-runtime/src/registry.rs` (state root) and
   `rust/crates/artifact-contracts/src/registry.rs` (contract registry). §0's "anchors are against
   the working tree" claim is now literally true for every anchor.

---

## 18. Change log — iteration 4 (resolves review-3.json)

Each item cites the review-3 correction it closes; every new/changed claim is OBSERVED against the
working tree at spec time.

1. **Model-identity provenance repaired — the pin is OPERATOR-ASSERTED, and a wire-time
   echoed-model check is specified (review-3 item 1).** Iteration 3 wrongly wrote that
   `text-embedding-nomic-embed-text-v1.5` was "the id the endpoint **served**." OBSERVED: the spike
   only *requests* that id (`tools/embed-seed-spike/spike.py:17,101`) and consumes **only**
   `data[].embedding` from the response (`tools/embed-seed-spike/spike.py:103-105`) — it never reads
   the response's `model` field, so nothing proves the endpoint served it. Corrected:
   - §6.1 `RMAP_SEED_MODEL_ID` row + the "one model id" note now label the pin **operator-asserted,
     not endpoint-verified** (a Layer-4 operator assertion under option (a)).
   - §7.1 adds a **wire-time echoed-model check**: when the endpoint returns a top-level `model`
     field and it ≠ the pin ⇒ `ModelMismatch` ⇒ "no hints" (hard-fail); when the field is **absent**,
     the pin stays operator-asserted (IMPL does **not** hard-fail on a missing echo — the spike never
     established that all endpoints echo `model`; INFERRED). The query-time check is reframed as
     comparing two operator-asserted ids (catches config change between embed and query).
   - §10 `ModelMismatch` doc comment and §9 doctor block updated: doctor shows the pin's **identity
     provenance** (`endpoint-echoed` vs `operator-asserted`) and must never print `operator-asserted`
     as verified. The affected pin, error, validation, and label wording are all threaded.
   (The prior "endpoint served" phrasing survives verbatim in the §17 iteration-3 log as the historical
   record of what iteration 3 claimed — this entry is its correction.)

2. **Local-only certainty corrected to the actual guarantee; D-ES-4 kept genuinely undecided; the
   IMPL plan made conditional (review-3 item 2).** OBSERVED: literal-IP + proxy-disabled transport
   constrains rmapd's **direct peer** to a loopback listener, but cannot prove that listener is a
   local model rather than a local forwarding proxy. Corrected:
   - §6.1 and the D-ES-4(a) risk cell no longer claim structural end-to-end "no third-party egress."
     They state the honest, narrower guarantee — *rmapd's direct network peer is always a loopback
     listener; rmapd sends no byte off-host* — and name **"trusting the local endpoint's behaviour"**
     as an explicit residual risk of option (a). Only embedded runtime (b) makes local processing
     structural (added to (b)'s REWARD, alongside structural model identity).
   - §11 preamble now splits IMPL-1 into a **runtime-agnostic core** (built regardless, no
     `Cargo.toml`) and the **`Embedder` impl chosen by D-ES-4**: (a) ⇒ the endpoint impl completes
     IMPL-1; (b) ⇒ a **separate post-ratification distribution slice** supplies the embedded impl and
     its plan is not binding until specced. The deferred-list "Embedded-ONNX" item and the D-ES-4(b)
     risk cell are reworded so (b) is **not foreclosed** — this SPEC's no-`Cargo.toml` STOP_CONDITION
     binds **this SPEC only**, not a future slice. D-ES-4's RECOMMENDED stays **None**.

3. **Artifact-contract table rows given full `file:line` anchors, and a name-vs-semantics defect fixed
   (review-3 item 3).** OBSERVED: the §3.4 contract table cited bare filenames (`truth_kind.rs`,
   `refresh.rs`, …). Each row now carries the enum-type anchor **and** the exact variant line:
   `truth_kind.rs:14`/`:58` (`Inference`), `refresh.rs:10`/`:49` (`MarkImpactedDeferRecompute`),
   `identity.rs:10`/`:20` (`StableLogicalKey`), `degradation.rs:10`/`:26`
   (`MayBeOmittedWithExplicitUnknown`), `provenance.rs:10`/`:26` (`DerivedFromLayer0Items`),
   `impact.rs:10`/`:30` (`MarkImpactedOnRelevantLayer0Change`), `freshness.rs:10` +
   `FreshnessState::Current`/`Stale` (`:72`/`:85`), `maturity.rs:15`/`:32`. In doing so, a
   **name-vs-semantics defect** surfaced: iteration 3 assigned `classification_maturity = prototype`,
   but `ClassificationMaturity` has **no `prototype` variant** — only `Stable`/`Provisional`/
   `Experimental` (`rust/crates/artifact-contracts/src/maturity.rs:20,26,32`). "prototype" is
   CLAUDE.md's *project* maturity-ladder word, not this enum's. Corrected to the assignable value
   **`Experimental`** (nearest to a first cut), with the mismatch noted in the row.

---

## 19. Change log — iteration 5 (resolves review-4.json)

Each item cites the review-4 correction it closes. Both are architecture-honesty fixes; no VISION
bound, no recommended-storage/corpus/CLI decision changed.

1. **§11 reconciled with the D-ES-8 crate boundary; the false "no `Cargo.toml`" claims removed
   (review-4 item 1).** OBSERVED: `rust/Cargo.toml` lists **49** crate members with no `seed` entry, so
   a ratified new crate `repo-graph-seed` (D-ES-8) necessarily adds a workspace-member line **and** a
   crate manifest — the iteration-1..4 claim that the runtime-agnostic core / option (a) touches "no
   `Cargo.toml`" was self-contradictory against D-ES-8. §11 is now **conditional on D-ES-4, D-ES-8, AND
   D-ES-9** and states the exact manifest footprint per branch: new crate ⇒ one member line + a crate
   manifest; support-module-in-existing-crate (D-ES-8's named candidate `crates/agent`) ⇒ a module file
   + at most one dependency line, no new member. The `> STOP_CONDITIONS bind this SPEC only` note now
   says plainly that **EMBED-SEED-IMPL-1 WILL edit `rust/Cargo.toml`** to the extent D-ES-8/D-ES-9
   dictate. §13's "only (b) needs `Cargo.toml`" line and D-ES-4(a)'s "nothing to `Cargo.toml`" REWARD
   are corrected to match.

2. **Ratification-class decision added for the option-(a) HTTP transport (review-4 item 2).** OBSERVED:
   a deterministic scan of every `rust/**/Cargo.toml` finds **no** outbound HTTP/TLS client
   (`reqwest`/`ureq`/`hyper`/`rustls`/`native-tls`) — the daemon is Unix-socket based and the spike used
   Python `requests`; and `serde_json` **is** already a workspace dep (32 manifests). So the specified
   `https`-plus-no-dependency endpoint could not be built as written. New **D-ES-9** (dependency-graph
   boundary) now presents the exhaustive matrix: **(a1)** adopt a Rust HTTP client (new dependency edge;
   supports `https`/chunked) vs **(a2, RECOMMENDED)** a std-library `TcpStream` loopback transport (no
   new edge; `http`-only; JSON via the existing `serde_json`) — the spike's endpoint is plain-`http`
   loopback, so http-only costs no demonstrated case. §6.1 point 1 (scheme set), point 3 (proxy-free
   transport), and the `https` note are now **conditional on D-ES-9** instead of asserting `https`
   unconditionally; §10's abstraction ledger records the transport as a D-ES-9 impl choice (not a new
   abstraction); §11 and D-ES-4(a) cross-reference it. Builder RECOMMENDS (a2) but does not bind —
   adopting a client dependency crosses an architecture boundary (CLAUDE.md § Decision Autonomy). The
   DECISIONS set is now **D-ES-1..9** (§14 DoD updated).

---

## 20. Change log — iteration 6 (resolves review-5.json)

Each item cites the review-5 correction it closes. Both are architecture-honesty fixes; no VISION
bound, no recommended-storage/corpus/CLI/refresh decision changed, and the DECISIONS set stays
**D-ES-1..9** (no new ratification cell added — the corpus-read inversion is the pattern the
architecture already mandates, not a new boundary choice).

1. **D-ES-8's dependency direction repaired — the pure seed unit no longer depends on the `storage`
   adapter (review-5 item 1).** OBSERVED: `repo-graph-storage` is the outer SQLite adapter and
   depends on `rusqlite` + `repo-graph-agent` + `repo-graph-indexer` + others
   (`rust/crates/storage/Cargo.toml:60-161`); `repo-graph-agent` (policy) **explicitly forbids**
   depending on `storage` (`rust/crates/agent/Cargo.toml:39-41`). So the iteration-4/5 text that had
   `repo-graph-seed` (and the "module inside `agent`" alternative) "depend on `storage` DTOs"
   inverted the dependency rule and made the `agent`-module option incoherent. Fix, mirroring the
   ratified `AgentStorageRead` pattern (`rust/crates/storage/Cargo.toml:155-161`): the pure seed unit
   **defines a policy-side `SeedCorpusRead` port + a raw `SeedCorpusEntry { file_uid, path,
   content_hash }` boundary DTO**; `repo-graph-storage` gains the dependency edge and implements the
   port on `StorageConnection` (READY-snapshot SELECT + `file_versions` join, §3.1/§3.3), converting
   rows before calling the pure logic — direction adapter → policy (outer → inner). Updated: §10
   abstraction ledger (the `repo-graph-seed` line now states INNER/pure + no storage dep; a **new
   ledger entry** documents the `SeedCorpusRead` port/DTO with concrete users, the earned-inversion
   axis, and the rejected simpler alternative), §10's "new code" enumeration (two ports now), §11
   milestone manifest text (the added manifest line is on `storage`'s `[dependencies]`, not a
   `storage` dep inside the seed crate; the `agent`-module option is now coherent via the port), and
   D-ES-8 (a fixed "CORPUS-READ BOUNDARY" note + both options corrected + BLOCKING_REASON clarifies
   the inversion is not part of what is ratified). No new dependency on `storage` sits inside any pure
   crate; the DAG is preserved (`repo-graph-storage → repo-graph-seed`, no reverse edge).

2. **D-ES-9's a2 std-library transport made evidence-honest with an explicit accepted-response
   contract (review-5 item 2).** OBSERVED: the spike used Python `requests`
   (`tools/embed-seed-spike/spike.py:13,101-104`), which transparently handles `Content-Length` **and**
   chunked / TLS / redirects — so it establishes **nothing** about the endpoint's wire framing. The
   prior D-ES-9 (a2) presented "LM Studio / Ollama return a `Content-Length`-delimited JSON body" as a
   mitigating fact and treated chunked as mere future variation — unsupported and unlabeled. Fix: that
   framing is now labeled **INFERRED, not demonstrated**, and a2 is respecified to **not assume**
   framing but **validate against an explicit bounded accepted-response contract and degrade honestly
   on any deviation**: plain-`http` loopback only; HTTP `200` only; `Content-Length` framing with
   `Transfer-Encoding: chunked` **explicitly rejected** (no de-chunking attempt); connect/read
   timeouts; header- and body-size caps; `serde_json` parse into the OpenAI shape — every deviation
   (chunked, TLS, non-200, oversize, timeout, malformed) maps to an existing `EmbedError` variant
   (§10) → "model unavailable", never a wrong answer or a partial-body parse (I2 candidate-never-
   answers + Honest Degradation Rule). The BLOCKING_REASON now notes ratification also fixes a2's
   compatibility envelope, and (a1)'s value is framed as removing exactly these limits. Also corrected
   a stale citation: the spike endpoint is `http://localhost:1234` (`spike.py:18`), not `127.0.0.1`.

---

## 21. Change log — iteration 7 (closes review-6.json)

Review-6's four **substantive** items were fixed **inline by the operator** before this pass (per the
packet OPERATOR NOTE); this pass **verified** those four against `review-6.json`, then fixed the
doc-internal statements the inline edits left stale and closed review-6 item 5. No VISION bound and no
RECOMMENDED storage/corpus/refresh/CLI decision changed; the DECISIONS set stays **D-ES-1..9**.

**Verified inline (operator) — review-6 #1–#4 (OBSERVED against the current doc):**

1. **#1 endpoint response correlation + vector sanity.** The D-ES-9 A2 accepted-response contract now
   requires `data.len()` == input count, an integer `index` on every entry forming a **unique
   permutation of `0..n`** (vectors correlated **by `index`, never array position**), and **rejects
   non-finite (NaN/±Inf) and zero-norm vectors** as `Malformed`; the request/response body shape gained
   the `index` field; each case is a named IMPL validation test. **Verified present** (D-ES-9 A2 "Body"
   / "Response correlation" / "Vector sanity"). No change needed.
2. **#2 exact codec + fixed limits.** §4.3 now defines the body as a **`bincode` little-endian
   versioned DTO** (`SeedVectorBodyV1 { entries: Vec<SeedVectorEntryV1{ file_uid, path, content_hash,
   vector }> }`, fixed field order; `schema_version` bump ⇒ discard, never migrate) with store limits
   header ≤ 64 KiB / body ≤ 1 GiB (reject, not truncate); D-ES-9 A2 fixes connect 2 s / read 30 s /
   header ≤ 64 KiB / body ≤ 32 MiB. **Verified present**; no residual "generous constant"/"e.g. ≤64
   KiB" wording remains (grep-checked). No change needed.
3. **#3 cross-machine ε guarantee removed.** §7.3(2) now makes **no positive stability claim at any
   margin** across machines/re-embeds; ε=1e-5 is a **non-guaranteeing near-tie advisory** (`near_tie`
   flag) only; a bounded envelope is a **deferred calibration slice**. **Verified present in §7.3.**
4. **#4 deep-vertical under either D-ES-4 outcome.** §11 now builds the runtime-agnostic core **in the
   same slice as the first ratified `Embedder`** (never dormant); under (a) core+endpoint impl ship as
   one slice; under (b) THIS IMPL-1 does **not** run and (b) is its **own complete vertical** shipping
   core + embedded impl together. **Verified present in §11 body.**

**Fixed this pass (genuine misses the inline edits left, + review-6 item 5):**

5. **Reconciled the two stale live spots the #4 fix left behind.** The §11 **deferred-list** bullet
   ("supplies the impl behind the already-built `Embedder` port; the … core is unchanged") and the
   **D-ES-4 RECOMMENDED note** ("the port is built regardless; under (b) a follow-on slice supplies the
   embedded impl") still described the superseded "core built regardless" milestone — which
   re-introduces the exact dormant-core-under-(b) failure #4 removed. Both are rewritten to match the
   corrected §11 body (under (b), option-(a) IMPL-1 does not run; the (b) vertical ships core + impl
   together). The §18 iteration-4 change log keeps the old wording as the historical record of what
   iteration 4 claimed (change logs are not restated).
6. **Reconciled §13's stale reproducibility summary with the corrected §7.3 (#3).** §13 still summarized
   ε as a "bounded reproducibility claim (… cross-store ε=1e-5, INFERRED)"; rewritten to "within-store
   exact; not guaranteed at any margin across machines; ε is a non-guaranteeing near-tie advisory".
7. **Closed review-6 item 5 (citation + decision-format audit).**
   - **D-ES-1 risk/reward.** D-ES-1 alone lacked the explicit `REWARD —`/`RISK —` wording the §DECISIONS
     preamble promises and its eight siblings carry; each option now states REWARD and RISK.
   - **49-member anchor.** The "49 crate members, no `seed`" claim now carries the line anchor
     `rust/Cargo.toml:19-74` (the `members` array; the 49 `crates/…` entries are `:20-68`). **Verified
     by reading:** `rust/Cargo.toml` lists exactly 49 `crates/…` members (lines 20-68) + four `tools/…`
     members, none named `seed`.
   - **`serde_json` in 32 crate manifests** — **verified** (`grep -rl serde_json rust --include=Cargo.toml`
     restricted to `crates/` = 32; the workspace root manifest has none); the claim already cites a
     spot-checkable `e.g. rust/crates/artifact-contracts/Cargo.toml` anchor (§10, D-ES-9 A2), so no
     change was needed for this sub-item.

---

## 22. Change log — iteration 7 REWORK (human directive 2026-08-25: integrate into existing seams, delete the verb)

The human ratified the MECHANICS (corpus, sidecar+pins, background recompute, **D-ES-4=(a)**) and
**rejected the separate verb**; VISION was amended (commit `a3a90ce`, `docs/VISION.md:149-160`).
This rework keeps the diff minimal, marks superseded text rather than deleting healthy sections, and
re-litigates only the changed/new cells (TD-015). Each item is OBSERVED against the working tree at
rework time.

1. **`rmap seed` verb DELETED; §8 rewritten as the INTEGRATION CONTRACT.** The standalone verb (its
   `main.rs`/`dispatch.rs` `"seed"` arm and `commands/seed.rs`) is removed. New §8.0 states the
   integration model (fire last, only on total deterministic failure; additive+self-labeling;
   degrade to today+one line). New §8.1 **enumerates the seams against code**: Group A =
   `orient`/`explain` focus resolution (fire at the no-match branches `orient/mod.rs:251-256`,
   `explain/mod.rs:187`, via `build_no_match_result`:369 / `build_no_match`:259; do NOT fire on
   ambiguous `orient/mod.rs:278-287`, `explain/mod.rs:208-223`; carrier = `Focus.candidates`,
   `dto/envelope.rs:55-121,174-210`); Group B = `callers`/`callees`/`path` symbol lookup (fire on
   `SymbolResolveError::NotFound`, `queries.rs:568`; `dispatch.rs:1268`/`:1438`/`:2479`/`:2502`; do
   NOT fire on `Ambiguous` `:1274`/`:1445`/`:2485`/`:2508`; carrier = the not-found error's `data`,
   rendered by `graph.rs:98-139`).
2. **Candidate envelope simplified to the amended-VISION shape (§8.2).** Each candidate now carries
   `score + source:"embedding" + model_id + module + path` and a single structured `next` follow-up
   (`explain <key>` / `orient --focus <key>` + `cwd`) — the full `(module, imports, callers)`
   neighbourhood is what `next` yields, **not inlined**. Additive optional fields on the existing
   `FocusCandidate` (Group A) / error `data` (Group B).
3. **The inline deterministic file-level caller fold (pre-rework §8.2 step 5) is DELETED** — the one
   piece of genuinely-new domain logic the pre-rework design carried. §8.2a retains the surfaces it
   used only as "what the `next` command yields". §8.4 drops the imports/symbols/caller caps; §10
   drops the fold from "new domain logic" (leaving only the vector envelope + cosine ranking); §11
   and §12 drop it from the milestone and its test.
4. **D-ES-1 → SUPERSEDED-BY-HUMAN** (recorded with its obsolete question/options for the audit
   trail, not silently removed). **D-ES-10 NEW** (seam enumeration + fire condition + carrier; takes
   over D-ES-1's ratification role). **D-ES-7 output-shape cells UPDATED** to the lightweight
   candidate + named `next`. D-ES-2,3,5,6,8,9 and ratified D-ES-4 **carried unchanged**.
5. **§2 invariants refined for honesty.** I1: no embedding fact in resolved facts/signals/`map`/
   `modules`; candidates live only in the no-match `candidates`/error `data`. I4: the *deterministic*
   tiers (resolved + ambiguous) are byte-unchanged; only the no-match branch gains a labeled,
   degradable addition (the pre-rework blanket "orient/explain byte-unchanged" would now be false and
   is corrected). §1's "what it is not" paragraph updated verb→integration.
6. **§8.3 degraded states** reframed per-seam: the deterministic outcome is byte-identical to today
   plus one labeled line (Group A `limits`; Group B error `data`/message). **§9 doctor** wording
   moved off the verb onto the seam fallback; the doctor block itself (store state / model pin /
   staleness) is unchanged.
7. **§11 milestone** = build Group A (one `Focus`/`FocusCandidate` integration point across
   `orient`+`explain`) as the smallest deep-vertical; **Group B specified but deferred** (different
   carrier + file-for-symbol weaker fit). DoD rewritten to an isolated no-match
   `rmap orient "<phrase>"` returning labeled semantic candidates, with all resolved/ambiguous
   results byte-unchanged.

---

## 23. Change log — iteration 8 (closes review-7.json + OPERATOR RULING 5, 2026-08-25)

Review-7 escalated on one **blocking conflict** (the candidate's `next` claimed a single command
yielding file-level `(module, imports, callers)`, which no existing surface provides) + four OBSERVED
code-truth mismatches. OPERATOR RULING 5 (VISION amended, commit `bf555ee`, `docs/VISION.md:159-164`)
ratified the reviewer's RECOMMENDED option and directed the four code-truth fixes. This iteration
applies exactly those; diff kept minimal, healthy sections untouched. Each item OBSERVED against the
working tree at fix time.

1. **Blocking conflict — follow-up is the deterministic SEQUENCE, not one command.** The binding
   VISION wording (`:159-164`) is now: candidate carries module/path directly; `explain <candidate>`
   yields imports + symbols; **callers are one further `explain` on a listed symbol**. Verified
   against code: `explain_file` emits identity + imports + symbols with `module_path: None` and **no**
   callers (`rust/crates/agent/src/explain/mod.rs:527-589`); callers are per-symbol only via
   `explain_symbol` (`:324`). Reworded everywhere the "single command yields the full neighbourhood"
   claim appeared: §1, §2 I1, §8.2 (intro + new code-truth note), §8.2 prose, §8.2a (now a two-hop
   sequence with the `module_path: None` fact at `:538`), §11 (milestone + DoD + deferred bullet),
   D-ES-7, the top rework banner item #4. Widening resolved `explain <file>` for callers was
   explicitly rejected by the operator (byte-stable output is inviolate) and is recorded as such.
2. **Observed #1 — empty `candidates` is OMITTED from JSON, not `[]`.** `Focus.candidates` carries
   `#[serde(skip_serializing_if = "Vec::is_empty")]` (`rust/crates/agent/src/dto/envelope.rs:111-112`).
   §8.3 prose + table rewritten: every zero-candidate state shows the `candidates` key **absent**; the
   fired-but-empty / unavailable signal is carried **entirely** by the always-present labeled `Limit`.
   Chosen resolution (recorded): **reflect code truth, no serializer change** (an always-present `[]`
   would need a bespoke override — the smallest design declines it).
3. **Observed #2 — `limits` is `Vec<Limit>` objects, not strings.** `Focus.limits: Vec<Limit>`
   (`rust/crates/agent/src/dto/envelope.rs:326`); `Limit = {code: LimitCode, summary (code-derived,
   fixed), reasons: Vec<String>, degradation}` (`rust/crates/agent/src/dto/limit.rs:330-337`), and
   `summary` is a fixed lookup from `code` (`:136-186`). §8.2 example + §8.3 table converted to `Limit`
   objects; the reader-facing per-situation detail moved into `reasons`. Because the closed `LimitCode`
   enum (`:35-187`) has **no** semantic-fallback variant, adding `SemanticFallback` /
   `SemanticFallbackUnavailable` is named as explicit IMPL work (§8.2 note, D-ES-7 RISK, §11).
4. **Observed #3 — Group-B not-found is `InvalidRequest` today, not `SymbolNotFound`.**
   `dispatch.rs:1268-1272` → `ErrorDetail::invalid_request` → `ErrorCode::InvalidRequest`
   (`rust/crates/daemon-transport/src/envelope.rs:113,:164,:204-205`); no `SymbolNotFound` variant
   exists (`:108-174`). §8.2 Group-B example fixed to `"InvalidRequest"`; the "unchanged deterministic
   outcome" claim clarified (the tier rides the existing error's additive `data`, no code change);
   introducing a dedicated `SymbolNotFound` code named as separate optional IMPL work, not
   "unchanged code".
5. **Observed #4 — §11 reconciled with D-ES-4's ratified (a).** The "conditional / preselects none /
   no runtime" framing is corrected: D-ES-4 is **RATIFIED (a)**, so §11 is the **binding** IMPL-1 plan
   and only D-ES-8/D-ES-9 remain open (footprint-only). The option-(b) "if the operator ratifies (b)"
   branch is demoted to a **superseded-alternative note** (recorded for completeness, not a live
   branch), in both the `Embedder`-implementation bullet and the deferred list.
6. **Additional code-truth constraint surfaced (not in review-7, found while verifying #1–#4).**
   `FocusCandidate` derives `PartialEq, Eq` (`rust/crates/agent/src/dto/envelope.rs:54`); a float
   `score` cannot satisfy `Eq`, so the "additive optional fields" claim entails dropping that derive
   (or carrying score as non-float). Recorded as named IMPL work in §8.2 and D-ES-7 RISK — honest
   about the real edit the boundary-DTO extension requires.

Per TD-015 the decision-review rerun still challenges only the changed/new cells: **D-ES-7** (updated
follow-up-sequence wording + the two code-truth IMPL edits) and **D-ES-10** (unchanged in substance).
D-ES-1 stays SUPERSEDED-BY-HUMAN; D-ES-2,3,5,6,8,9 and ratified D-ES-4 carry over unchanged.

---

## 24. Change log — iteration 9 (closes review-8.json, 2026-08-25)

Review-8 was `revise` on **one blocking code-truth mismatch** (no new decisions, no scope change):
§8.2/§8.2a specified the follow-up second hop as `explain <symbol-stable-key>`, implying hop 1
(`explain <file>`) supplies symbol **stable keys**. It does not — `explain_file` emits
`ExplainSymbolItem { name, subtype, line_start }` (`rust/crates/agent/src/dto/signal.rs:783-787`;
built at `rust/crates/agent/src/explain/mod.rs:574-581`), with **no** `stable_key` field. The doc
therefore overclaimed deterministic caller reachability from a file explain. All fixes are wording
only; healthy sections untouched. Each fact OBSERVED against the tree at fix time.

1. **Hop-2 keys by NAME, not stable key.** Corrected every `explain <symbol-stable-key>` /
   "symbol keys for hop 2" claim to `explain <symbol-name>` in §8.2 (intro prose + new hop-2
   code-truth note), §8.2 `next`-follow-up prose, §8.2a (`symbols` bullet: `stable_key` is an
   internal ordering tiebreak, **not serialized**; `callers` bullet: hop 2 = `explain <symbol-name>`),
   §2 I1, §11 (milestone "Done when" + deferred bullet), and D-ES-7 option cell.
2. **Honest follow-up contract stated.** Hop 2 resolves the name via `resolve_symbol_name`
   (`rust/crates/storage/src/agent_impl.rs:938-970`), which returns a `Vec` (`LIMIT 5`): a name may
   match several symbols. So hop 2 returns callers **only when the name resolves uniquely**; otherwise
   it returns the existing deterministic **ambiguity** result (contract precedence + ambiguous ⇒
   candidates, `docs/architecture/agent-orientation-contract.md:62-86`). The doc no longer implies
   full deterministic caller reachability from a file explain.
3. **Recorded as an existing-surface limitation, not an implicit `explain <file>` extension.** The
   name-only second hop and its ambiguity are marked an accepted limitation of the follow-up sequence;
   widening resolved `explain <file>` remains explicitly rejected (I4, OPERATOR RULING 5 — byte-stable
   output inviolate). No new IMPL work is introduced by this iteration; the two code-truth IMPL edits
   named in iteration 8 (drop `FocusCandidate` `Eq`; new `LimitCode` variants) are unchanged.

No decision cells changed substance (D-ES-7's wording is refined to match code, not re-litigated); the
decision-review scope from iteration 8 is unchanged.

---

## 25. Change log — iteration 10 (HUMAN DIRECTIVE 2 + SCOPE EXPANSION, 2026-08-25)

Iteration 9 was **approved** (`review-9.json`). This iteration is a **human-directed scope
expansion**, not a review fix: VISION § Semantic Seeding now ratifies **three** uses of the semantic
substrate (commit `8adabca`, binding; measured in the spike addendum). The change is **purely
additive** — the §8 fallback-tier contract, all substrate cells (D-ES-2..10), and every ratified
mechanic are **unchanged**. Each edit below cites the packet item it closes; facts OBSERVED against
the tree at edit time.

1. **§8B added — the `rmap find "<concept>"` affirmative concept-search verb (use (ii)).** A new
   section specs the human-named verb: it **shares the ENTIRE §8 substrate** (store §4, pins §7.1,
   corpus §3, cosine+path-tie ranking §7.2, degraded states §8.3) and carries the §8.2 candidate
   **fields** in its **own** response DTO (§8B.2 — `path` locator, `f64` score, always-present
   `candidates: []`; not `FocusCandidate`); the **only** new surface is the verb + its envelope. Contract: ≤10 candidates (human-directed
   bound, HUMAN DIRECTIVE 2 — VISION `8adabca` ratifies the verb + honesty, not the numeric cap), a
   mandatory Layer-3 honesty header `summary:"likely areas for \"<concept>\"
   (semantic hints — open the files)"` that **never** claims completeness, the same `next`
   `explain`-sequence follow-up, and §8.3-identical degraded states phrased for an affirmative verb.
   §8B.4 enumerates the new surface against code as IMPL work under the standing Protocol Surface
   Standard verb-naming pre-ratification: one CLI arm in the hand-rolled `match`
   (`rust/crates/rgr/src/main.rs:84`, `orient`/`explain` arms `:96`/`:104`, imports `:54-58`), a
   `run_find` mirroring `run_orient` (`rust/crates/rgr/src/commands/orient.rs:50`), one daemon arm
   (`rust/crates/daemon-runtime/src/dispatch.rs:330`, `"orient"` `:365`), and **one witness-manifest
   line** in `rust/crates/daemon-runtime/witness/dispatch_fact_classes.txt` (enforced by
   `rust/crates/daemon-runtime/tests/consolidation_witness.rs:35-43` — a new arm goes RED until
   declared). All OBSERVED.
2. **D-ES-11 recorded (NEW) — the `find` contract.** The verb + name are **human-ratified**; only the
   output contract (envelope, ≤10 cap, honesty header, degraded states, additive surface) is
   decision-review-challengeable, with I1-honesty as the load-bearing point.
3. **D-ES-1 given its SECOND human supersession.** The first directive rejected a *fallback* verb
   (→ D-ES-10); HUMAN DIRECTIVE 2 then **directly named `find`**, overriding this cell's original
   rejection of `find`/`search`-named verbs — the human ruled the honesty mitigation is the *output*
   itself. STATUS updated; obsolete options retained for audit (not re-litigated).
4. **§3.3 corpus-coverage limitation recorded — SQL/DDL files.** Per the spike addendum
   (`docs/spikes/2026-08-23-embed-seed-spike-1.md:58-59`, OBSERVED), the "database table schema
   definitions" query missed the SQL DDL files — they are outside the indexed corpus. Recorded as a
   named coverage limitation of **both** semantic surfaces (the corpus is exactly what `files` holds);
   the exact cause (scanner coverage vs. exclusion flag) is marked **INFERRED** and its fix scoped out.
5. **`find` folded into the IMPL-1 milestone (§11); use (iii) added as a follow-on.** Per VISION
   `8adabca` "uses (i)+(ii) ship in the first IMPL", the runtime-agnostic-core + EMBED-SEED-IMPL-1
   milestone now build the `find` verb alongside the fallback tier (the stale "No new verb, no new
   dispatch arm" line is corrected — it held for the fallback tier only). **Use (iii)** —
   cross-module concern hints — is added as a **named follow-on milestone `EMBED-CONCERN-1`** with its
   direction + measured evidence (spike addendum clusters `:61-68`); its **rendering-surface
   decisions are explicitly deferred** to that slice's own DECISIONS, not taken here.
6. **§12 validation, §14 DoD, DECISIONS preamble, and the revision header updated** to the
   three-use scope. Per TD-015 the decision-review rerun for this cycle challenges **only** D-ES-11 and
   the use-(iii) milestone cut; D-ES-2..10 and ratified D-ES-4 carry over unchanged.

---

## 26. Change log — iteration 10 (closes review-9.json, 2026-08-25)

Two code-truth reconciliations of the iteration-10 additions (§8B `find`, §11 use-(iii)). **No scope
change**: the §8 fallback contract, all substrate cells (D-ES-2..10), and every ratified mechanic are
untouched; this pass only makes the two new/changed cells match code. Both items OBSERVED against the
tree at edit time.

1. **`find`'s candidate/empty-array contract made exact — its OWN DTO, not a `FocusCandidate` reuse
   (review-9 item 1).** §8B.2 previously said `find` "reuses the §8.2 candidate shape verbatim" while
   its example rendered `path`; but §8.2's Group-A object is `FocusCandidate`
   (`{ stable_key, file: Option<String>, kind }`, `rust/crates/agent/src/dto/envelope.rs:54-59`,
   deriving `Eq`), which uses `file`, not `path`. And §8B.3 claimed `find`'s new top-level `candidates`
   inherits §8.3's omit-when-empty behaviour — but that behaviour is the
   `#[serde(skip_serializing_if = "Vec::is_empty")]` attribute on **`Focus.candidates`**
   (`…/envelope.rs:110-112`), which a **new** `find` DTO does not inherit. Fixed by specifying **one
   exact `find` DTO** (§8B.2): a fresh `FindResponse { …, query, summary, candidates: Vec<FindCandidate> }`
   with `FindCandidate { stable_key, path: String, score: f64, source, model_id, module, next }` — it
   carries the §8.2 candidate *fields/semantics* but as its **own** struct, uses `path` (a plain
   `String`, not `file: Option<String>`), a `f64` score (so it does **not** derive `Eq`), and an
   **always-present `candidates: []`** (plain `Vec`, **no** `skip_serializing_if` — the smaller, honest
   design for a fresh surface; named as IMPL work under D-ES-11). §8B.3's degraded rows and closing note,
   and D-ES-11's OPTIONS/RECOMMENDED, are updated to the always-present-`[]` shape; §25 item 1's wording
   corrected to match. The Group-A tier is unchanged: it still reuses `FocusCandidate` with the
   omit-when-empty attribute precisely because it must not perturb the existing `Focus` byte-output.

2. **Concern-hint milestone no longer claims §7.2 ranking is reused (review-9 item 2).** §11's use-(iii)
   bullet said concern hints share "store, pins, ranking, and degraded states … verbatim (§4/§7/§8.3)"
   while also describing K=24 cosine K-means ranked by span/cohesion — incompatible: §7.2 ranks
   *query→file* cosine with a path tie-break, whereas concern discovery **has no query** and ranks
   **clusters**, not files. Fixed: the bullet now states concern hints reuse only the **store** (§4),
   **pins** (§7.1), and **degraded states** (§8.3) verbatim — **not** the §7.2 ranking — and marks the
   K-means/K/span/cohesion as **spike evidence** (`docs/spikes/2026-08-23-embed-seed-spike-1.md:61-68`,
   EXECUTED), with the clustering + cluster-ranking **contract deferred to `EMBED-CONCERN-1`'s own
   DECISIONS**. The requested direction + measured evidence are kept; the incorrect reuse claim is
   removed.

---

## 27. Change log — iteration 11 (closes review-10.json, 2026-08-25)

Two **internal-contract reconciliations** — both entirely within the iteration-10 `find` scope; **no
scope change**, no new decisions, no code-truth claim added (the anchors were already OBSERVED). The
issue was an internal *self-contradiction*: §12's acceptance test asked for the opposite of the ratified
§8B DTO contract, and §8B's "shared verbatim" wording over-claimed against its own deliberately-distinct
rendering. Both fixed by aligning to §8B.2/§8B.3/D-ES-11 (already the intended contract).

1. **§12 `find` degraded-state acceptance criterion corrected (review-10 item 1).** §12 item (vi)
   previously required each degraded `find` state to return "the honesty header with the `candidates`
   key **omitted**" — the exact opposite of the ratified §8B.2/§8B.3/D-ES-11 contract, which specifies an
   **always-present `candidates: []`** (plain `Vec`, no `skip_serializing_if`). A test written to the old
   criterion would have validated the wrong public JSON. Item (vi) now requires the **always-present
   `candidates: []` under the labeled `summary` honesty header (no error, no omitted key)**, matching
   §8B.2/§8B.3 and D-ES-11.

2. **§8B.1/§8B.2/§8B.3 "shared verbatim" degradation wording corrected (review-10 item 2).** The prose
   called `find`'s degradation "identical / shared verbatim" with §8 and said the only differences were
   "cap/header" — but `find`'s degraded output is deliberately a **different DTO/rendering**: its own
   `FindResponse` with a `summary` carrier and an always-present `[]`, versus §8's `Focus.candidates`
   (omitted-when-empty) plus a `Limit` line. Reworded to state precisely that `find` shares the §8
   **substrate** — store (§4), pins (§7.1), corpus (§3), ranking (§7.2) — and the **degradation
   *causes / state taxonomy*** (§8.3): the *same* unavailable/pin-mismatch/known-zero conditions detected
   by the *same* `Embedder`/store error variants, i.e. no new degradation-*detection* logic. What `find`
   does **not** share is the *rendered* degradation output — that is intentionally its own (§8B.2/§8B.3).
   Edits: the §8B.1 substrate table split into a "Substrate … shared" row + a "Degradation *output shape*
   … intentionally distinct" row and the following paragraph rewritten; §8B.2's "shared verbatim" sentence
   rewritten to confine the differences to rendering; §8B.3's header changed from "identical to §8.3" to
   "same causes/taxonomy as §8.3, distinct rendering"; §8B.3's closing line tightened to "same causes — no
   new degradation *detection* code; only the rendering is `find`'s own".

