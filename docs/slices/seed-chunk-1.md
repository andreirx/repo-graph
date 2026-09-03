# SEED-CHUNK-1 — seeds from symbol chunks, always on

Status: SPECIFIED (2026-09-03) · Track: zg-derived queue #4 — human-ratified Option 1
(2026-09-03: full swap). CODE slice, large. Maturity: MATURE.

## 1. Problem (MEASURED — docs/audits/2026-09-03-seed-chunk-spike-1.md)

find's semantic seeds are file-granularity nomic embeddings through lmstudio: dilution
loses the right answers inside their own files (db_impl.cc absent from its own top-10
twice), the tier silently vanishes when lmstudio is down, and scores cluster flat. The
spike measured the cure: per-symbol chunks + potion-code-16M-v2 static embeddings
(model2vec-rs, in-process) → every failing query absent→top-3-or-better, 3/4 wins vs
nomic at equal granularity, 18k chunks < 1 min offline; the stored is_test fact compounds
(obsolete-files → rank 1).

## 2. Contract

1. **Chunks**: one per SYMBOL node from the snapshot's nodes table — text = qualified_name
   + doc_comment (when stored) + span source (capped ~60 lines/6KB), the spike's recipe.
   Files/nodes without spans contribute no chunk (no invention).
2. **Engine**: `model2vec-rs` crate in-process with `minishlab/potion-code-16M-v2`
   (f32). Model resolution: local cache first (under the app state dir), else the crate's
   HF fetch once, cached; checksum recorded. Model UNAVAILABLE (offline, no cache) →
   seeds tier renders honestly absent WITH REASON ("embedding model not cached and not
   fetchable") — never a crash, never silent absence.
3. **Storage**: vectors per snapshot keyed to node identity, ADDITIVE migration; refresh
   copy-forward reuses vectors of unchanged file versions (no recompute); embedding-model
   identity stamped with the vectors — a model change invalidates (rebuild semantics, zg
   precedent); snapshots without vectors → seeds absent with reason (pre-migration).
4. **Serving swap**: find's seed tier ranks chunk vectors by cosine; the existing
   similarity-floor honesty stays ("no seeds above the floor (best: X)"). Output per
   seed: `path:line` + qualified symbol name (FIND-EVIDENCE-1 anchor discipline) + score
   + model name (existing honesty). Seeds stay BELOW the facts wall, labeled guesses.
5. **is_test partition** (the moat): production-classified chunks rank above
   test-classified in the rendered list, test seeds labeled — FIXTURE-POLLUTION
   semantics; unknown is_test never demoted, never invisible.
6. **Retirement**: the lmstudio embedding path is REMOVED from the find seed tier (the
   ratified swap — no dual engines, no fallback ladder). Any OTHER lmstudio consumer
   (enrichment etc.) is untouched — verify and state what else uses it before deleting
   shared plumbing.
7. Pipeline placement: embedding runs in the existing background pass family
   (index/enrich lifecycle), never blocking foreground; the busy/foreground-lock
   semantics (FOREGROUND-LOCK-1) apply unchanged.

## 3. Stop conditions

Frozen: the facts tier and its wall, exit codes, existing storage tables (additive
migration only), FOREGROUND-LOCK semantics. STANDING HONESTY RULES (absent tier states
its reason; floors stated; unknown visible). New dependency: `model2vec-rs` + its
transitive needs ONLY — anything else → DECISION_REQUIRED. If the crate cannot run
in-process on this toolchain (build failure, platform issue), STOP + DECISION_REQUIRED
with options (do not shell out to a CLI silently). Unmet DoD → STOP + DECISION_REQUIRED.
Never touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: chunk assembly (with/without doc_comment/span); model-absent reason path;
  model-identity invalidation; copy-forward reuse; is_test partition rendering; floor
  honesty.
- Live proof (isolated state root, registry sha unchanged): re-run the FOUR spike queries
  through the product — leveldb "crash recovery" and "stale obsolete files" (with the
  now-real IS-TEST-CPP fact — expect ≈ the spike's demoted ranks), repo-graph "retention"
  and "import cycles"; report the rank table vs the spike baseline. THEN: stop lmstudio
  (or point at a dead port) and show seeds still serve. Byte-stable elsewhere.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

Seeds are chunk-granular, anchored, is_test-partitioned, engine-local, and always-on
(honest reason when the model is genuinely unreachable); the spike's measured ranks are
reproduced through the product; lmstudio is out of the seed path with other consumers
untouched; gates green.

CORPUS PATHS: leveldb at ../legacy-codebases/leveldb; repo-graph is THIS repo; FRAKTAG at
../FRAKTAG; glamCRM at ../glamCRM.
