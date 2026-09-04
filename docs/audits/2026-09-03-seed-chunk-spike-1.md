# SEED-CHUNK-SPIKE-1 — chunk-granularity seeds bake-off (2026-09-03)

Question (human-ratified spike): does symbol-chunk embedding fix the measured seed
dilution, and does a static code model (potion-code-16M-v2 via model2vec-rs, zg's stack)
hold against nomic-via-lmstudio at equal granularity?

Method: chunks = per-SYMBOL spans from rmap's own nodes table (v0.15.0 audit root DB;
qualified_name + doc_comment header + ≤60 body lines), leveldb 1,527 + repo-graph 16,566
chunks; both models embed identical texts; 4 concept queries with audit-established ground
truth; harness /private/tmp/seedspike/.

## Rank of first ground-truth hit (top-10 shown to agent; absent = not in top-10)

| Query                    | product today (file-nomic) | nomic×chunks | potion×chunks | potion×chunks×is_test |
|--------------------------|---------------------------|--------------|---------------|----------------------|
| leveldb crash recovery   | absent                    | 29           | 49            | 10 (simulated fact)  |
| leveldb obsolete files   | absent (wrong layer)      | 13           | **2**         | **1** (simulated)    |
| repo-graph retention     | bare paths                | 10           | **3**         | —                    |
| repo-graph import cycles | absent                    | 6            | **3**         | —                    |

## Findings

1. **Chunk granularity is the fix** — every absent→ranked transition comes from it,
   model-independent. The dilution class the graders measured (db_impl.cc lost to its own
   test file) is a granularity artifact.
2. **The static model holds: potion wins 3 of 4 raw head-to-heads** (2v13, 3v10, 3v6;
   loses recovery 49v29), with visibly better score discrimination (0.30–0.70 spread vs
   nomic's 0.71–0.79 flat — the same flatness the zg audit flagged in our current seeds).
   Cost: 18k chunks in <1 min offline vs ~20 min through lmstudio; ~30MB model asset; no
   external process.
3. **Facts × seeds compounds, and only we can do it**: is_test demotion took
   obsolete-files to rank 1 and recovery from 49 to 10. Test-pollution of concept queries
   is model-independent (both models ranked RecoveryTest 1–4) — the cure is our stored
   fact, not a better embedder.
4. **Fact gap surfaced: `is_test=0` for ALL leveldb files** — C++ test classification does
   not exist (Rust cfg(test) and TS bases do). The leveldb demotion rows above use a
   SIMULATED fact (gtest structural markers: TEST/TEST_F macro or gtest include) — the
   exact basis an IS-TEST-CPP-1 would ratify. Labeled simulation, not product behavior.

## Proposed follow-ups (awaiting ratification)

- SEED-CHUNK-1: per-symbol chunk embeddings via model2vec-rs + potion-code-16M-v2
  replacing lmstudio file-level seeds; is_test-partitioned seed rendering (production
  above test, matching FIXTURE-POLLUTION semantics); vectors stored per snapshot;
  model-swap invalidates vectors (rebuild semantics, zg precedent).
- IS-TEST-CPP-1: gtest structural basis for C++ is_test (measured consumer: seed
  demotion; also heals FIXTURE-POLLUTION and module test counts on C++ repos).

Caveats: 4 queries, 2 repos; is_test arm simulated on leveldb; potion-code is
code-tuned while nomic is general (fair — that IS the choice being made); single run.

## Addendum (2026-09-04) — potion-code-16M-v2 floor calibration (SEED-CHUNK-1)

Durable record of the mandated Option-C calibration (operator ruling SEEDCHUNK-FLOOR,
2026-09-04) that set the seed similarity floor. Measured in the isolated rig
(`RMAP_STATE_ROOT=/private/tmp/seedchunk-cal`, model cached under the state root; the
operator registry sha256 was identical before and after). model2vec-rs ran in-process;
`model.safetensors` = 32 MB. This addendum is the operator-committed copy of the table
that the code doc-comment (`rgr::commands::find::seed_render::SEED_SIMILARITY_FLOOR`)
and build report cite.

### No-home band — top score per deliberately-absent concept (≥5 per corpus)

| corpus | absent concept | top | note |
|---|---|---|---|
| leveldb | audio waveform synthesis | 0.185 | noise |
| leveldb | blockchain consensus protocol | 0.217 | noise |
| leveldb | http request routing middleware | 0.288 | noise |
| leveldb | GPU shader compilation | 0.312 | noise |
| leveldb | react component hooks state | **0.338** | `Version::RecordReadSample` — pure noise |
| FRAKTAG | MIDI audio synthesis | 0.206 | noise |
| FRAKTAG | genome sequence alignment | 0.243 | noise |
| FRAKTAG | blockchain consensus protocol | 0.300 | noise |
| FRAKTAG | http request routing middleware | 0.356 | noise |
| FRAKTAG | GPU shader compilation | 0.491 | `gpu_lock` — REAL (MLX); has a home, EXCLUDE |
| glamCRM | blockchain consensus protocol | 0.305 | noise |
| glamCRM | GPU shader compilation | 0.331 | noise |
| glamCRM | LSM tree compaction | 0.376 | noise |
| glamCRM | kernel process scheduler | 0.460 | `NotificationScheduler` — partial real |
| glamCRM | genome sequence alignment | 0.494 | `Etapa.sequence` — 1-word collision |
| repo-graph | genome sequence alignment | 0.458 | lexical |
| repo-graph | MIDI audio synthesis | **0.589** | `FileArtifact.synthesisMode` — 1-word collision on "synthesis" |

No-home band (excluding FRAKTAG-GPU which has a real home): **0.185 – 0.589**.

### Spike true hits (raw ranking, this model)

| query | corpus | ground truth | rank | score | top hit |
|---|---|---|---|---|---|
| crash recovery | leveldb | `DBImpl::Recover` | 4 | 0.322 | `VersionSet::Recover` 0.44 |
| stale obsolete files | leveldb | `RemoveObsoleteFiles` | 2 | 0.360 | `MaybeAddFile` 0.36 |
| retention | repo-graph | `RetentionClass.as_str` | 3 | 0.688 | `record_retention_report` 0.78 |
| import cycles | repo-graph | `module_import_cycles` | 1 | 0.708 | 0.71 |

### Finding — the bands OVERLAP (Option C's premise is falsified for potion)

The no-home band (ceiling 0.589, a single-word lexical collision) exceeds EVERY leveldb
true hit (≤0.44). Within leveldb ALONE a genuine no-home concept ("react component hooks
state" → 0.338) outscores the real ground-truth `DBImpl::Recover` (0.322): inverted. So
NO fixed global floor both renders leveldb's true hits (needs ≤0.32) AND abstains on the
no-home band (needs >0.59).

### Ruling — `SEED_SIMILARITY_FLOOR = 0.30`, meaning demoted (SEEDCHUNK-FLOOR-2)

0.30 is RATIFIED as a NOISE-TAIL cutoff, NOT a certainty threshold: it reproduces both
leveldb ground truths (0.322 / 0.360 clear it) and abstains only on the lowest no-home
tail (0.18–0.29); it cannot separate the lexical-collision no-home tail (0.46–0.59) from
truth. The old 0.60 was nomic geometry — carrying it to potion's different, corpus-
relative geometry was itself an unfounded certainty claim. The Layer-3 honesty is carried
by the "ranked guesses, not facts" header, the facts wall, and the production/test
partition — the floor only trims the hopeless tail; the rendered abstain says "no
candidates above the minimum similarity 0.30" (never wording implying calibrated
certainty).
