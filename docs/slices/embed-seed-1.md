# EMBED-SEED-1: semantic seeding — a local-embedding candidate generator for task-to-anchor orientation — SPEC

Slice: EMBED-SEED-1
Status: **SPEC — AWAITING RATIFICATION.** No production code, no `Cargo.toml`, no schema (SQL) change
(packet STOP_CONDITIONS). This doc is the whole deliverable. It runs under **decision-review**:
the `## DECISIONS` section is the ratification surface; an IMPL slice follows only after the
operator ratifies. (The packet cites `docs/slices/decision-review-mode-1.md` for the marker
convention; that file does not exist in the tree at spec time — INFERRED that the intended
convention is the DECISION_REQUIRED-matrix + terminal DECISIONS section used by
`docs/slices/module-model-1.md` §7/§12 and `docs/slices/engine-consolidation-1.md` §6/§8, which
this doc follows.)

Revision: **iteration 7** — this revision closes `.agent-manager/slices/EMBED-SEED-1/review-6.json`.
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
map layer. Per VISION § Semantic Seeding it is a **candidate generator**: a separate opt-in verb
returns ≤5 seed candidates, each handed off to the *existing deterministic* orient/explain
surfaces. The embedding never enters `orient`/`map`/`modules`.

---

## 2. Principle (the bounds, restated as invariants the IMPL must not cross)

Ratified in VISION § Semantic Seeding; repeated here as the invariants every decision below is
measured against:

- **I1 — Candidate generator, never answer.** `seed` returns ≤5 candidates + each candidate's
  *deterministic* neighbourhood obtained from the existing orient/explain surfaces. No
  embedding-derived fact appears in `orient`/`map`/`modules`.
- **I2 — Evidence-backed hint, Layer-3, labelled.** Every candidate carries `score`,
  `source: "embedding"`, and `model_id`. Ranking is a fixed formula (cosine + deterministic
  tie-break); no LLM in the loop. The output speaks the reader's language (VISION § Labels), not
  our pipeline state.
- **I3 — Deterministic given its pins.** Every vector is pinned `(model_id, dim, content_sha)`;
  any mismatch is a **hard fail → degrade to "no hints"**, never a silent stale answer. Staleness
  recomputes from the content hash.
- **I4 — Local and optional.** Local model only; no API key, no network egress to a third party.
  Absence of the model, or an empty/stale vector store, degrades to **"no hints"** — never to
  degraded orientation. `orient`/`explain`/every other verb are byte-unchanged whether or not
  seeding exists.

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
  (a 160k-file monorepo at 768-dim ≈ 0.5 GiB — beyond the cap the load is rejected and `seed`
  degrades to "vector store exceeds the seed budget — seeding declined", never a partial read).
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
index/refresh — the exact ENRICH-LIFECYCLE-1 shape.** Not on-demand at first `seed`.

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

### 5.2 Why background-at-refresh over on-demand-at-first-seed

| | background at index/refresh (RECOMMENDED) | on-demand at first `seed` |
|---|---|---|
| First `seed` latency | ~ms (store already warm) — the token/wall-clock win the VISION monetizes | first call pays the whole embed cost (spike: ~72 s cold on glamCRM) synchronously — a "safe read-only" verb that blocks a minute fails the Protocol-Surface promise its name makes |
| Cancellation / write-safety | reuses `EnrichCoordinator` supersede + batch-boundary cancel; an incoming index preempts it | must build its own cancellation; a long synchronous embed on a read path contends with writers |
| Staleness | recompute-on-`content_hash`-change: only changed files re-embed; unchanged copy-forward by `content_hash` match | store can be arbitrarily stale until someone calls `seed`; degrades I3 to "recompute lazily" |
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

## 8. CLI CONTRACT (resolves packet item 4, review-0 items 3–4, review-1 items 1–2; DECISIONS D-ES-1, D-ES-7)

### 8.1 The verb — `rmap seed "<task>" [--repo <path>] [--json]`

Name per the Protocol-Surface Standard (the name must imply the workflow role). `seed` reads as
safe/read-only, sibling to `orient`/`explain` — it plants a starting point, it does not change
state. (Alternatives `find`/`search` overclaim answer-ness; `locate` is acceptable — D-ES-1.)
Dispatch is added to the hand-rolled match, **not** a clap enum (there is none):

- `rust/crates/rgr/src/main.rs:84` `match args[1].as_str()` — add `"seed" => run_seed(&args[2..]),`
  beside `"orient"` (:96) / `"explain"` (:104); import the handler at `rust/crates/rgr/src/main.rs:53-56`; new module
  `rust/crates/rgr/src/commands/seed.rs`. Unknown-verb fallthrough is `rust/crates/rgr/src/main.rs:145`.
- Daemon side: `rust/crates/daemon-runtime/src/dispatch.rs:365` `"orient" => self.handle_orient(...)`
  — add `"seed" => self.handle_seed(...)` (string-keyed request `command`, same as every verb).

### 8.2 Output — ≤5 candidates, each with its deterministic neighbourhood

The JSON is the product. Shape (additive to the `rgr.agent.v1` envelope family,
`rust/crates/agent/src/dto/envelope.rs`):

```json
{
  "schema": "rgr.agent.v1",
  "command": "seed",
  "repo": "glamCRM",
  "snapshot": "…",
  "task": "where does the backend fetch BNR exchange rates?",
  "source": "embedding",
  "model_id": "text-embedding-nomic-embed-text-v1.5",
  "candidates": [
    {
      "stable_key": "glamCRM:serverless/.../bnr-service.ts:FILE",
      "path": "serverless/packages/backend/src/services/bnr-service.ts",
      "score": 0.71,
      "source": "embedding",
      "model_id": "text-embedding-nomic-embed-text-v1.5",
      "neighbourhood": {
        "module": "…",       // owning module (module_summary aggregate_file)
        "imports": [ … ],    // ≤8 target files (find_file_imports, sorted)
        "symbols": [ … ],    // ≤8 in-file symbols (list_symbols_in_file, line order)
        "callers": {         // ≤8 aggregated callers over the file's symbols (see step 5)
          "items": [ … ],           // ranked, deduped caller symbols
          "count": 12,              // distinct callers found before the ≤8 cap
          "symbols_scanned": 8,     // in-file SYMBOL nodes fanned out over
          "applicable": true        // false ⇒ file has no SYMBOL nodes → callers not measurable
        }
      },
      "next": [
        { "cmd": "explain", "args": ["<stable_key>"],            "cwd": "<repo_root_abs>" },
        { "cmd": "orient",  "args": ["--focus", "<stable_key>"], "cwd": "<repo_root_abs>" }
      ]
    }
  ],
  "limits": [ … ]
}
```

- `candidates` capped at **≤5** (VISION bound). `score` is the cosine; `source`/`model_id` on every
  candidate (I2). Ties → path order (§7.2).

**Composition of the neighbourhood (D-ES-7) — module + imports + callers, the ratified VISION set
(review-1 item 1; corrected against the real surfaces, review-0 item 3).** `docs/VISION.md:152-154`
ratifies the seed neighbourhood as **`(module, imports, callers)`** — a higher-priority bound than
any builder simplicity preference (Decision Hierarchy #1). Iteration 1 wrongly *deferred* callers to
a `next`-referral; this revision **inlines callers deterministically**. Iteration 0's separate error
(claiming `orient_file` yields imports+callers) is also corrected: `orient_file`
(`rust/crates/agent/src/orient/file.rs:37-92`) aggregates only snapshot, trust, dead-code and
module-summary — imports live on the *file explain* path, callers on the *symbol* explain path. The
executable composition uses exactly the existing functions:

1. Resolve each candidate path → focus via `resolve_path_focus`
   (`rust/crates/storage/src/agent_impl.rs:448`, declared `rust/crates/agent/src/storage_port.rs:632`), the same resolver orient uses
   (`rust/crates/agent/src/orient/mod.rs:156`). A file that no longer resolves is dropped from the candidate list
   (honest: the vector is stale relative to the snapshot).
2. **`module`** — the owning module, from the same `aggregators::module_summary::aggregate_file`
   that `orient_file` calls (`rust/crates/agent/src/orient/file.rs:72`). Deterministic, single value.
3. **`imports`** — `find_file_imports(snapshot_uid, path)` → distinct target files
   (`rust/crates/agent/src/storage_port.rs:794`), ordered `target_file` ascending exactly as `explain_file` orders them
   (`rust/crates/agent/src/explain/mod.rs:544-548`, `ordering::sort_explain_imports`), **capped at 8** (§8.4).
4. **`symbols`** — `list_symbols_in_file(snapshot_uid, path)` → in-file SYMBOL nodes
   (`rust/crates/agent/src/storage_port.rs:777`), ordered **`line_start` ASC, then `name`, then `stable_key`** exactly as
   `explain_file` orders them (`rust/crates/agent/src/explain/mod.rs:570-572`; the sort is `ordering::sort_explain_symbols`,
   `rust/crates/agent/src/ordering.rs:148-155`, whose comparator is
   `line_key(line_start).then(name).then(stable_key)` — a **total** order, `stable_key` being the
   unique final tiebreak). **Capped at 8** (§8.4). Each item carries `name`, `subtype`, `line_start`
   (the `ExplainSymbolItem` shape, `rust/crates/agent/src/explain/mod.rs:574-581`).
5. **`callers`** — a **deterministic file-level aggregation** of the existing symbol-caller surface,
   not a new fact. Callers are symbol-granularity (`find_symbol_callers(snapshot_uid,
   symbol_stable_key)` → `Vec<AgentCallerRow>`, `rust/crates/agent/src/storage_port.rs:737/461-467`; the exact call
   `explain_symbol` makes at `rust/crates/agent/src/explain/mod.rs:324`). A file has many symbols, so the file-level set is
   defined by a **fixed fold** with no order jitter:
   - **Fan-out set.** The **same ≤8 in-file symbols** from step 4 (`line_start`, then `name`, then
     `stable_key` order — the total order of step 4). Fanning out over the already-capped,
     already-ordered symbol list bounds cost to ≤8 `find_symbol_callers` lookups per candidate (≤40
     across the ≤5 candidates) and makes the fold input deterministic.
   - **Per-symbol rank.** Each symbol's caller rows are ranked by the *same* total order
     `explain_symbol` uses — `call_ranking::rank_caller_rows` (concentration DESC, `module_path` ASC,
     `name` ASC, `stable_key` ASC; `rust/crates/agent/src/explain/call_ranking.rs:15-17,56-84`). This is a total order over
     distinct callers, so the ranked list is jitter-free.
   - **Union + dedup.** Iterate the symbols in their (`line_start`, `name`, `stable_key`) order; within each symbol,
     append callers in ranked order; **the first occurrence of a caller `stable_key` fixes its
     position**, later duplicates (a caller that calls several in-file symbols) are skipped. The
     result is a stable, total order over distinct callers. **Cap at ≤8** (§8.4); `count` reports the
     distinct total before the cap so truncation is visible (no-silent-caps).
   - **Empty vs not-applicable (Honesty Rule — unknown ≠ zero).** `symbols_scanned = 0` (the file has
     no SYMBOL nodes — e.g. a config/asset/markup file) ⇒ `applicable: false`, `items: []`: callers
     are **not measurable** for this anchor, distinct from a measured zero. `symbols_scanned > 0` with
     no callers ⇒ `applicable: true`, `count: 0`, `items: []`: a **measured** "0 callers", which
     `explain_symbol` itself treats as meaningful positive information (`rust/crates/agent/src/explain/mod.rs:322-323`).

Each caller `item` carries `stable_key`, `name`, `module` (the `ExplainCallerItem` fields,
`rust/crates/agent/src/explain/mod.rs:334-338`). This inlines the file-native `(module, imports, callers)` VISION set in a
single round-trip with **zero new storage surface** — every field is a fold over functions that
already exist. The `next` referral (below) remains, as an *addition* for the agent that wants
per-symbol callees or the full untruncated caller set, not as a substitute for the inlined callers.

**The `next` commands are structured and executable (review-1 item 2).** Iteration 1 emitted the
strings `"explain glamCRM <stable_key>"` / `"orient glamCRM <stable_key>"`; both are **invalid
against the real CLI**: `run_explain_cmd` accepts exactly **one** positional target and no repo
argument (`rust/crates/rgr/src/commands/orient.rs:440-451`), `orient` accepts **no positionals at all** — focus
is `--focus <value>` (`rust/crates/rgr/src/commands/orient.rs:95-118`, the `other =>` arm errors on any positional at `:124-125`)
— and **both resolve the repo from the current working directory**, not from an argument (`orient`
at `rust/crates/rgr/src/commands/orient.rs:160-172`, `explain` at `:488`, via `std::env::current_dir().canonicalize()`). A
two-positional `explain glamCRM
<key>` errors; a positional `orient glamCRM <key>` errors; and neither can target a `seed --repo`
repo that differs from the shell's cwd. `next` is therefore emitted as **structured commands** —
`{cmd, args, cwd}` — with the **actual** syntax (`explain <key>`; `orient --focus <key>`) and an
explicit **`cwd` = the absolute repo root that `seed` resolved** (so the follow-up resolves the same
repo the seed candidate came from, regardless of the shell's cwd). The structured shape is honest
(no rendered string that would fail if pasted) and self-contained (the agent has everything to run
it). Human mode renders it as, e.g., `(cd <repo_root> && rmap explain <key>)`.

- **Human mode** mirrors orient's density; **`--json`** via the shipped idiom
  (`rust/crates/rgr/src/commands/orient.rs:61` `"--json"`, emit `:201`
  `serde_json::to_string_pretty`). The candidate DTO reuses `FocusCandidate`
  (`rust/crates/agent/src/dto/envelope.rs:55`: `stable_key`, `file`, `kind`) as the shape precedent.

### 8.3 Honest empty / degraded states (I4; architecture.md Honest Degradation Rule)

Each is a distinct, reader-facing state — `null`/absent ≠ known-zero, and none narrates our
pipeline:

| Situation | Output | Never |
|---|---|---|
| No vector store yet (never indexed / just built) | `candidates: []`, `limits: ["no seed vectors yet — embeddings build in the background after indexing"]` | not an error; not "0 matches" as if measured |
| Model unavailable (endpoint down / no local model) | `candidates: []`, `limits: ["semantic seeding unavailable — no local embedding model reachable; seeding is optional, orientation is unaffected"]` | never blocks; never degrades `orient` |
| Pins mismatch (model/dim/schema changed) | `candidates: []`, `limits: ["seed vectors were built with a different model — rebuild on next index"]`, store discarded | never rank across a pin mismatch |
| Some files stale (content changed since embed) | ranked over the fresh subset; `limits: ["N files changed since last embed — not yet re-seeded"]` | never silently rank a stale vector as current |
| Query embeds but nothing scores | `candidates: []` (genuine known-zero) | — |

Each degraded state maps to an `Embedder`/store error variant (§10) or an empty/mismatched store —
no state collapses into another (Honest Degradation Rule, `docs/architecture/artifact-contract-model.md:417-429`).

### 8.4 Budget caps (exact — review-0 item 4)

Every numeric limit is fixed here so the IMPL builds against a contract, not a guess:

| Cap | Value | Where enforced | Ratification-class? |
|---|---|---|---|
| Embed input per document | **6 000 chars** (char-boundary) | corpus build (§3.2) | no — spike mechanism constant (`tools/embed-seed-spike/spike.py:101`) |
| Embed batch size | **32** documents/request | embed pass (`tools/embed-seed-spike/spike.py:98`) | no |
| Corpus admission cap | **50 000 files** (default); above it, embed the first 50 000 by `path` order and emit an honest omission `limit` (MODULE-MODEL-2 D7 bounded-output discipline) | corpus build | no — a tunable safety bound for the 160k-file monorepo target; **INFERRED default**, adjustable |
| Candidate count | **≤ 5** | ranking output | **yes — VISION bound** |
| `imports` per candidate | **≤ 8** | neighbourhood build (§8.2) | no — local presentation cap |
| `symbols` per candidate | **≤ 8** | neighbourhood build (§8.2) | no — local presentation cap |
| Caller fan-out symbols per candidate | **≤ 8** (the same capped `symbols` set) | neighbourhood build (§8.2 step 5) | no — bounds `find_symbol_callers` calls to ≤8/candidate |
| `callers` per candidate (after union+dedup) | **≤ 8** (`count` reports pre-cap distinct total) | neighbourhood build (§8.2 step 5) | no — local presentation cap |
| Query path | one query embedding + brute-force cosine over the store (spike: "trivially fast" over ~4k vectors) → sort → top-5. No pagination. | `seed` handler | — |

The corpus-size cap is the only cap that bounds *coverage*; per the no-silent-caps rule it emits a
visible omission `limit` rather than silently truncating. All other caps bound *presentation* and
are recorded in the output envelope when they truncate.

---

## 9. CERTAINTY + HONESTY (resolves packet item 5)

- **Layer-3 labels in the reader's language (VISION § Labels).** The seed result reads *"likely
  starting point (semantic match, model `text-embedding-nomic-embed-text-v1.5`) — open the file"*, never
  *"embedding cosine 0.71, vector store fresh"* (that is our pipeline state; keep it to `--json`
  fields + doctor). `source: "embedding"` + `model_id` are the machine-readable provenance; the
  human line names *their* code and *why* it surfaced, not our processing.
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

Recommended path (D-ES-1 `seed`, D-ES-2 state-root sidecar, D-ES-3 background-at-refresh, D-ES-5
file-level, D-ES-6 exclude test/generated/vendored, D-ES-7 inline module+imports+symbols+aggregated
callers) introduces **no SQL schema change and no new DB table**, and reuses: the
`files`/`file_versions` reads the spike proved; the warm-cache envelope + `atomic_write`; the
ENRICH-LIFECYCLE-1 background/cancel lifecycle; the existing focus-resolution + `find_file_imports` /
`list_symbols_in_file` / `find_symbol_callers` + `call_ranking::rank_caller_rows` neighbourhood
surfaces; the `--json` idiom. New code is one support unit + **two ports** (`Embedder` +
`SeedCorpusRead`, the corpus-read seam — §10 ledger) + one background pass + one verb + one doctor
block + the cross-crate `artifact-contracts`/`repo-index` family registration (§3.4). The only
genuinely new domain logic is (i) the vector envelope + cosine ranking and (ii) the deterministic
file-level caller fold (§8.2 step 5) — both pure and headless-testable.

**Abstraction ledger** (one line each; a line that cannot be filled ⇒ the abstraction is removed):

- **`repo-graph-seed` support unit** — *what:* pure corpus-build + vector envelope + cosine ranking.
  *Concrete current users:* the `seed` verb handler + the background embed pass (two callers).
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

**This plan is CONDITIONAL on three ratification cells and preselects none of them (review-4 items
1–2):** D-ES-4 (model runtime), D-ES-8 (crate-vs-module home for the pure seed logic), and D-ES-9
(option-(a) HTTP transport). The milestone is binding **only under D-ES-4 option (a)**. Its manifest
footprint is **light but NOT zero**, and its exact shape depends on D-ES-8 and D-ES-9 — stated
honestly here (iterations 1–4 wrongly claimed the plan touches "no `Cargo.toml`"; corrected). IMPL-1
splits into a runtime-agnostic core and the `Embedder` implementation:

- **Runtime-agnostic core (built in the SAME slice as the first ratified `Embedder` — never
  dormant; review-6 #4):** the pure seed logic (corpus build, envelope,
  cosine+tie ranking, caller fold), the cross-crate `SeedVectors` family registration (§3.4), the
  `Embedder` **port** (§10, the seam — not any impl), the background pass, the `rmap seed` verb +
  neighbourhood + `--json`, the doctor block, and the five degraded states.
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
- **The `Embedder` implementation (chosen by D-ES-4):**
  - **If the operator ratifies (a):** IMPL-1 ships the endpoint impl (env config + loopback enforcement,
    §6.1) and is complete as one slice. **Manifest impact — decided by D-ES-9:** under the recommended
    std-library transport (a2) the impl adds **no dependency** (raw `std::net::TcpStream` + hand-framed
    HTTP/1.1, http-loopback only; JSON via the already-present `serde_json`); under a client crate (a1)
    it adds **one** HTTP(/TLS) dependency edge to the home crate's manifest. So (a)'s total manifest cost
    ranges from *one workspace-member line* (a2 + new crate) to *a member line plus one dependency edge*
    (a1 + new crate) — light either way, and categorically unlike (b)'s heavyweight ONNX + model
    distribution burden.
  - **If the operator ratifies (b):** THIS IMPL-1 does **not** run (review-6 #4 — shipping the
    core with no working `Embedder` would be dormant infrastructure behind a permanently degraded
    verb, violating the deep-vertical rule). Instead the (b) path gets its **own complete vertical**:
    a post-ratification distribution slice (spec + its own ratification) that ships the
    runtime-agnostic core above **and** the embedded-ONNX `Embedder` (heavyweight dependency,
    bundled/versioned/notarized model, the larger `Cargo.toml` edits) **in the same arc**, so the
    verb works the day the capability exists. The core's design (this §11) is reused verbatim there;
    only the impl behind the port differs.

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
  cosine+path-tie ranking, **the deterministic file-level caller fold §8.2 step 5**, headless tests
  → the **cross-crate `SeedVectors` family registration** (§3.4: `artifact-contracts` enum/`table_name`/
  `all`/`get_contract` + its coherence test, and `repo-index` `family_to_table` + the two refresh-array
  exemptions) → one `Embedder` port with the (a) endpoint impl incl. the §6.1 env config + loopback
  enforcement + the **D-ES-9-ratified transport** (a2 std-library `TcpStream` with no new dep, or a1
  client dependency) → background embed pass after index/refresh reusing
  `spawn_auto_enrich`'s shape + `EnrichCoordinator` cancel → `rmap seed "<task>"` verb returning ≤5
  candidates with the inlined `(module, imports, symbols, callers)` neighbourhood (§8.2) + structured
  `next` + `--json` → doctor "Semantic seeding" block → the five honest empty/degraded states.
  **Done when:** on the glamCRM smoke fixture, isolated `rmap seed "where does the backend fetch BNR
  exchange rates?"` returns the bnr-service/exchange-rates neighbourhood (module + imports + inlined
  callers) in ≤5 (reproducing the spike's 14/16-class result on the ratified tasks), and
  `orient`/`explain`/`trust`/`doctor`(existing lines) are byte-unchanged (I1/I4 regression check).

**Explicitly deferred (named extension points, not built):**

- **Symbol-level corpus (D-ES-5)** — the (S) format; wins UI-phrasing misses at 5.6× store size.
- **Untruncated / per-symbol callers & callees in the neighbourhood** — the inline caller fold caps
  at ≤8 aggregated callers (§8.2 step 5); the full per-symbol caller/callee set is covered by the
  structured `next` `explain <key>` referral, not inlined. (Note: inline *aggregated* callers are
  **built** in IMPL-1 per the VISION `(module, imports, callers)` bound — only the untruncated tail
  is deferred.)
- **Embedded-ONNX runtime (D-ES-4 (b))** — the alternative `Embedder` impl. **Not deferred by a
  builder preference** — its inclusion is exactly what D-ES-4 decides. Out of *this SPEC's* scope
  because it needs a heavyweight dependency + bundled model + `Cargo.toml` (this SPEC's
  STOP_CONDITION). **If the operator ratifies (b)**, this option-(a) IMPL-1 does **not** run; instead
  the (b) path is its own complete post-ratification distribution slice (spec + ratification) that
  ships the runtime-agnostic core (this §11) **and** the embedded-ONNX `Embedder` in the same arc, so
  the verb works the day the capability exists — never a dormant core behind a degraded verb (review-6
  #4). The core's design (this §11) is reused verbatim; only the impl behind the `Embedder` port
  differs.
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
  is faked); the `artifact-contracts` registry-completeness + policy-coherence tests still pass with
  the new family (`rust/crates/artifact-contracts/src/registry.rs:458-465`); the isolated live dogfood
  (`./scripts/dogfood-isolated.sh`, never the operator's registry) running `rmap seed` on the
  fixture; a regression capture proving `orient`/`explain`/`trust`/`doctor`(existing lines)
  byte-unchanged; and the five degraded states each exercised.

---

## DECISIONS (ratification-class — decision-review + operator ratify; the IMPL does NOT re-decide)

Status: **AWAITING RATIFICATION.** Each is an exhaustive matrix; RECOMMENDED is the builder's
defensible pick except D-ES-4 (distribution-level — deliberately not decided). Two cells carry an
architecture-boundary blast radius and so are ratification-class even though they name a
RECOMMENDED: **D-ES-8** (new crate vs module — a component-graph node) and **D-ES-9** (option-(a)
HTTP transport — a dependency-graph edge); the builder recommends but does not bind either.

DECISION_REQUIRED:
- ID: D-ES-1
  QUESTION: The verb name for the semantic-seeding candidate generator.
  OPTIONS:
  - `seed` (RECOMMENDED): reads as safe/read-only, "plant a starting point"; sibling to
    orient/explain; matches VISION § Protocol Surface Standard. REWARD — the name itself signals a
    safe, read-only candidate generator (I1), so the protocol surface is honest before the agent
    reads a word of output. RISK — slightly less self-evident to a first-time reader than a search
    verb (mitigated: the human line names the workflow role, §9).
  - `locate`: REWARD — also safe-sounding and acceptable. RISK — slightly implies an exactness the
    Layer-3 hint does not have (a near-miss with I2's evidence-backed-hint framing).
  - `find` / `search`: REWARD — most familiar to users. RISK — reads as an answer engine → violates
    I1 (candidate generator, never answer). Rejected.
  RECOMMENDED: `seed`.
  BLOCKING_REASON: The verb name is the primary protocol-surface signal (VISION); it is a public
    CLI contract cell the IMPL cannot pick unilaterally.

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

- ID: D-ES-3
  QUESTION: Refresh policy — background recompute-on-change at index/refresh, or on-demand at first
    `seed`? And the opt-out default.
  OPTIONS:
  - Background at index/refresh, recompute changed files by `content_hash`, default ON, env opt-out
    `RMAP_SEED_VECTORS` (RECOMMENDED): reuses ENRICH-LIFECYCLE-1 (spawn/cancel/detached); first
    `seed` is fast; honest skip when no model (like enrich's no-resolver skip). REWARD — fast read,
    proven lifecycle. RISK — one background pass per index even if `seed` is never used (opt-out
    mitigates).
  - On-demand at first `seed`: REWARD — zero cost until used. RISK — first call blocks ~minute
    (spike cold cost) on a "safe read" verb; bespoke cancellation; unbounded staleness. Rejected.
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
    **inside** the (b) distribution slice alongside the embedded impl (§11, review-6 #4), so the verb
    is deep-vertical under either outcome — never a dormant core. Neither option is preselected; the
    builder does not bind the default.
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

- ID: D-ES-7
  QUESTION: Neighbourhood composition in the `seed` output. (The VISION-ratified set
    `(module, imports, callers)`, `docs/VISION.md:152-154`, is a **bound**, not an open choice; what
    remains decidable is the deterministic *aggregation* of the symbol-granularity caller fact to
    file granularity, and its caps.)
  OPTIONS:
  - Inline module + ≤8 imports + ≤8 symbols + ≤8 **deterministically-aggregated callers**, plus a
    `next` referral for the untruncated/per-symbol set (RECOMMENDED): `seed` reuses
    `resolve_path_focus` + `module_summary::aggregate_file` + `find_file_imports` +
    `list_symbols_in_file` + `find_symbol_callers`; callers are the fixed fold over the ≤8 in-file
    symbols (per-symbol `rank_caller_rows`, union, first-occurrence dedup by `stable_key`, cap 8,
    unknown-vs-zero via `symbols_scanned`) specified in §8.2 step 5. REWARD — satisfies the ratified
    `(module, imports, callers)` set in a single round-trip; every field is a fold over an existing
    function; deterministic (total orders throughout); bounded cost (≤8 caller lookups/candidate).
    RISK — the caller fold is genuine new (small, pure, unit-tested) aggregation logic, and inline
    callers are capped/deduped so an agent needing the full set follows `next`.
  - Referral-only (callers, or all, via `next`): REWARD — thinnest handler. RISK — **contradicts the
    ratified VISION neighbourhood set** (iteration 1's error); costs a round-trip for a bound the
    VISION says must be inline. Rejected.
  - Inline callers with a *larger*/uncapped fan-out (all in-file symbols, higher cap): REWARD —
    more complete callers. RISK — unbounded `find_symbol_callers` fan-out per candidate; against the
    ≤5-candidate/bounded-output discipline. Rejected for IMPL-1; the ≤8 cap + `next` covers the tail.
  RECOMMENDED: Inline module + ≤8 imports + ≤8 symbols + ≤8 aggregated callers (§8.2 step 5) + a
    `next` referral for the untruncated/per-symbol caller & callee set.
  BLOCKING_REASON: Sets the `seed` JSON output contract (the agent-consumed product surface), the
    caller-aggregation semantics, and which existing surfaces the handler composes. (The *set*
    `(module, imports, callers)` is not itself decidable — it is the VISION bound; only the
    aggregation/caps are.)

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
  `file_versions.content_hash`; `files` has no `snapshot_uid`; `orient_file` returns no
  imports/callers — the neighbourhood composition is built from the real surfaces in §8.2; the
  `ArtifactFamily` match is **not** single-crate — a second exhaustive match lives in `repo-index`,
  §3.4; the state root is **not** structurally "never synced" — `RMAP_STATE_ROOT` is arbitrary, §4.2).
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
the `## DECISIONS` section (D-ES-1..9, each risk/reward). review-impl approves → decision-review
produces the ratification packet → **halt at awaiting-ratification for the human.** The IMPL slice
(EMBED-SEED-IMPL-1, §11) runs only after D-ES-1..9 are ratified.

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

