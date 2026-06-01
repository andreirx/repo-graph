# Dataflow & Hot-Path Map (DATAFLOW-HOTPATH-MAP-1)

Status: **Architecture evidence — current as of 1B (2026-06-01).** No code; no cache-format decision;
no daemon-SCIP implementation. This map MUST guide LIVEGRAPH-INTEGRATION-1C and
PARTITIONED-WARM-CACHE-ARCH-1. Evidence labels: EXECUTED (measured), OBSERVED (artifact inspected),
INFERRED (concluded), NOT RUN (deferred measurement).

Charter: `docs/slices/dataflow-hotpath-map-1.md`.

## Why this exists

1C is where the daemon starts owning the real production flow
(`source → SCIP producer → scip-ingest → PartitionIr → value facts → LiveGraph → rmap query`). Warm
cache then decides what to serialize. Both must be guided by a concrete map of data shapes,
authorities, rebuildability, epochs, hot paths, and copy points — not discovered accidentally inside
implementation (the SQLite "serialize whatever exists" failure mode).

## 1. Pipeline diagram

```text
                         ┌─────────────── authoritative, durable ───────────────┐
                         │ source files (ground truth)   user A1 authority/waivers│
                         └───────┬───────────────────────────────┬──────────────┘
                                 │                               (durable store, NOT derived)
        ┌────────────────────────┼────────────────────────┐
        ▼ (per TS package / Rust crate = a PARTITION)      ▼
   scip-typescript / rust-analyzer            ts-extractor (AST, stable-key + metrics)
        │  index.scip (protobuf)                    │  AstNodeLite (stable_key, range), metrics
        ▼                                           ▼
   decode_index ─────────────────► scip-ingest::ingest_partition  ◄──────────── (joins AST ⨝ SCIP)
                                          │  IngestOutcome { ir: PartitionIr, complexity, counts, ... }
                                          ▼
                                    PartitionIr (nodes + edges + partition meta)   [repo-graph-ir]
                                          │                    └─ complexity map (canonical key → u32)
                                          ▼ (D6 separate channel)
                        ┌─────────────────┴───────────────────┐
                        ▼                                      ▼
                 LiveGraph.load_partition            ValueFact[] → LiveGraph.load_value_facts
                 (Slot: ir, epoch, status,           (epoch-stamped, D7)
                  defines, ref_counts, language)
                        │  always-resident xref summary (defines + ref_counts)
                        ▼
                 callers / callees / value_facts ──► classify_answer + smart constructors
                        │
                        ▼
                 AnswerEnvelope<T> { class, freshness, completeness, data,
                                     degradation_reasons, missing_partitions,
                                     provenance, contributing_languages }   [repo-graph-trust-model]
                        │
                        ▼
                 rmap callers/callees --engine livegraph|compare   (1B; default = sqlite)
                        │
                        ▼ (DEFERRED) PARTITIONED-WARM-CACHE: serialize the EXPENSIVE artifact only

  OLD PATH (oracle, rebuildable derived cache — to be decommissioned LAST):
  source → ts-extractor → indexer → SQLite nodes/edges/snapshots → resolve_symbol → find_direct_callers
```

## 2. Data-shape table (per boundary)

| Boundary | Type (owning crate) | Key fields | Notes |
|---|---|---|---|
| SCIP index | `scip::types::Index` (scip 0.7.1) | documents, symbols, occurrences | protobuf; decode via `decode_index(bytes)` |
| AST facts | `AstNodeLite` / `AstFacts` (scip-ingest) | `stable_key`, range, metrics (`cyclomatic_complexity`) | from ts-extractor over source |
| Ingest result | `IngestOutcome` (scip-ingest) | `ir: PartitionIr`, `complexity: BTreeMap<key,u32>`, `edges_report`, `node_counts`, `ts_call_sites`, `missing_source` | the use-case surface; complexity is `pub` |
| Canonical identity | `CanonicalKey(String)` (repo-graph-ir) | adopts the ts-extractor `stable_key` byte-equal (AstAdopted) | **same key the SQLite path uses** (1B OBSERVED) |
| Node | `IrNode` (repo-graph-ir) | `key`, `subtype`, `name`, `range: Option<SourceRange>`, `partition_id`, `identity_source`, `provenance` | `identity_source ∈ {AstAdopted, ScipSynthesizedFallback, AstFileScope}` |
| Edge | `IrEdge` (repo-graph-ir) | `src`, `dst`, `edge_type ∈ {Calls, References}`, `basis: EdgeBasis`, `provenance` | strict CALLS = `SyntaxConfirmedCall` only |
| Partition | `PartitionIr { partition, nodes, edges }` (repo-graph-ir) | `Partition { id, kind, root, indexer, indexer_version, build_inputs_hash }` | one buildable unit = one index |
| Value fact | `ValueFact` (repo-graph-livegraph) | `subject: ValueSubject(Symbol\|RawAnchor)`, `kind`, `value`, `basis`, `source_range`, `provenance` | D6 side channel; D1 = complexity only |
| Runtime slot | `Slot` (repo-graph-livegraph, private) | `epoch`, `status`, `ir: Option<PartitionIr>`, `language`, `defines: HashMap<String,IdentityBasis>`, `ref_counts: HashMap<String,usize>`, `value_facts`, `value_facts_epoch` | xref summary = `defines` + `ref_counts`, retained on unload |
| Answer | `AnswerEnvelope<T>` (repo-graph-trust-model) | `class`, `freshness`, `completeness`, `data: Option<T>`, `degradation_reasons`, `missing_partitions`, `provenance`, `contributing_languages: BTreeSet<LanguageSupport>` | smart constructors; illegal states unrepresentable |
| Answer payload | `CallersAnswer` / `CalleesAnswer` / `ValueFactsAnswer` | per-partition counts, `(partition, key)` identities, `contributing_epochs` | |
| OLD query DTO | `ResolvedSymbol`, `CallerResult`, `CalleeResult` (storage) | `stable_key`, name, file, line, edge_type, resolution | the shipped JSON `{target, callers\|callees, count}` |

## 3. Authority / rebuildability table

| Fact | Authoritative source | Rebuildable? | Durable need? |
|---|---|---|---|
| source files | the repo (ground truth) | n/a | versioned externally (git) |
| SCIP index (`index.scip`) | scip-typescript / rust-analyzer over source + build config | YES (re-index) — **expensive (the hot path)** | only as a cache artifact |
| AST facts / metrics | ts-extractor over source | YES (re-extract) | no |
| `PartitionIr` (nodes/edges/identity) | derived (AST ⨝ SCIP) | YES — but reproducing needs the SCIP index (expensive) | **warm-cache candidate** |
| complexity / value facts | derived (ts-extractor metrics) | YES | no |
| xref summary (`defines`, `ref_counts`) | derived from `PartitionIr` | YES — **cheap (~21ms, REFRESH EXECUTED)** | NO — rebuild from IR on load |
| aliases / reconciliation (XPART) | derived (`.d.ts.map` + descriptor) | YES | no |
| answer cache (`AnswerEnvelope`) | derived per query | YES — recompute per query | NO |
| SQLite raw graph (`nodes`/`edges`) | derived (tree-sitter) | YES | NO — disposable (ADR Tier B) → RAW-DECOMMISSION |
| **user A1 authority / waivers / declarations** | the USER (governance) | NO — not derivable | **YES — the only durably-owned facts** |
| provenance / manifests (`build_inputs_hash`, indexer versions) | producers | partially (re-stamp) | recommended durable (cache validity) |

Rule (anti-SQLite-mistake): **only `PartitionIr` is worth persisting** (it embeds the expensive
SCIP-derived facts); everything downstream (xref, value-fact placement, answers) is cheap to rebuild
from it. User authority lives in a SEPARATE durable store, never in the rebuildable cache.

## 4. Epoch / coherence table

| Epoch / version | Scope | Bumped when | Binds / detects |
|---|---|---|---|
| `PartitionEpoch` | per partition | `swap_partition` (refresh accepted) | the IR generation a query was answered from |
| `XrefEpoch` | global xref | any partition swap | xref staleness across partitions |
| `value_facts_epoch` | per partition (D7) | `load_value_facts` (stamps current partition epoch) | **swap-without-reload → value facts `Stale`** (proven) |
| `contributing_epochs` | per answer | recorded at answer time | which partition epochs an `AnswerEnvelope` was built from |
| SQLite `snapshot_uid` (+ status BUILDING→READY→STALE, `parent_snapshot_uid`) | whole repo | index / refresh | the old path's single-snapshot coherence; readers see latest READY |

Refresh contract (REFRESH-PROBE-1, EXECUTED): serve **last-good epoch** during a refresh; **atomic
swap** on success; **keep last-good** on failure (`RefreshFailed`); `callers`/`callees` are
SCIP-dependent → a pending SCIP refresh is `Partial` + `PrecisionPending`, never `Exact`.

## 5. Hot-path timing table

| Stage | Cost | Evidence |
|---|---|---|
| **SCIP indexing (scip-typescript)** | **~1.9–3.0 s / TS partition** | EXECUTED (REFRESH-PROBE-1: FRAKTAG engine ~1.9s, amodx plugins ~3.0s) — **DOMINANT** |
| **SCIP indexing (rust-analyzer)** | **~29–32 s / crate p95** | EXECUTED (RUST-INGEST-PROVE-1) — dominant; whole-workspace UNSUPPORTED |
| cross-partition xref / alias recompute | **~21 ms** | EXECUTED (REFRESH-PROBE-1) — negligible vs indexer |
| scip-ingest IR construction (decode + AST ⨝ SCIP + build) | sub-second on the synthetic (15 nodes) | OBSERVED (1B preload) — NOT RUN at scale |
| `decode_index` (protobuf) | ∝ index size | INFERRED — NOT RUN at scale |
| LiveGraph lookup (callers/callees/value_facts) | in-memory, sub-ms | OBSERVED (1B live) |
| SQLite query (`find_direct_callers`) | ms | OBSERVED (shipped path) |
| dist-rebuild + provider + consumer reindex (public-API edit cascade) | ~3.5 s | EXECUTED (REFRESH-PROBE-1) |

Conclusion: the pipeline is **indexer-bound**, not repo-graph-bound. The SCIP producer is the only
multi-second stage; everything repo-graph owns (ingest, xref, value facts, query) is ms or sub-ms.
This is the single most important fact for 1C and warm-cache.

## 6. Copy / allocation risk table

| Site | What is copied | Magnitude | Mitigation (deferred) |
|---|---|---|---|
| SCIP decode | protobuf → `Index` | ∝ index | unavoidable; bounded by index size |
| IR construction | `Vec<IrNode>`, `Vec<IrEdge>`, `CanonicalKey(String)` per node/edge | ∝ nodes+edges | string keys dominate |
| xref summary build | `HashMap<String,_>` `defines` + `ref_counts` — **String key clones** | ∝ nodes+edges | **key interning** (Arc<str>/symbol table) |
| value-fact conversion | complexity map → `Vec<ValueFact>` (clones key + provenance + range per fact) | ∝ functions | interning + Arc on provenance |
| answer payload | `CallersAnswer`/`CalleeAnswer` `(String,String)` clones; `AnswerEnvelope` | ∝ result size | borrow where possible |
| 1B serving render | `caller_results_from_keys` clones keys into `CallerResult` | ∝ result size | acceptable (opt-in mode) |
| repo_uid-prefixed keys | the same `repo_uid:file#sym:KIND` String reproduced in defines, ref_counts, value facts, answers | **multiplied across structures** | **the top interning target** |

Top risk: the canonical key `String` (e.g. `repo_01k…:src/main.ts#report:SYMBOL:FUNCTION`, OBSERVED
1B) is cloned in `defines`, `ref_counts`, value facts, and every answer. An interned key (Arc<str> or
a per-partition symbol id) would cut the dominant allocation — **a warm-cache + runtime design input,
not a 1B concern.**

## 7. Old SQLite path vs new LiveGraph path

| Dimension | OLD (SQLite, oracle) | NEW (LiveGraph) |
|---|---|---|
| Producer | ts-extractor (tree-sitter) | scip-typescript → scip-ingest (SCIP) |
| Identity | `stable_key` (ts-extractor) | `CanonicalKey` = **adopts the same `stable_key` byte-equal** (AstAdopted) |
| Storage | persistent SQLite, whole-repo single snapshot | in-memory, per-partition, NOT persisted (warm-cache later) |
| Partitioning | none (whole repo) | per TS package / Rust crate |
| Query | SQL `find_direct_callers/callees` | `callers`/`callees`/`value_facts` over xref + resident IR |
| Trust labels | none | `AnswerClass`/`FreshnessState`/`contributing_languages`/degradation |
| Coherence | snapshot status | per-partition epoch + atomic swap + value-fact epoch binding |
| 1B comparison (synthetic) | 10 nodes / 6 edges; callers(makeCircle)={report}, callees(report)={makeCircle, describe} | 15 nodes / 11 edges (FILE + fallback richer); **same caller/callee keys → 0 mismatches, class Exact** (EXECUTED) |

Key 1B finding (EXECUTED): with the **same `repo_uid`**, the SCIP-adopted keys are byte-equal to the
SQLite `stable_key`s, so the two paths AGREE on the synthetic fixture (the compare sidecars were
`Exact`, all buckets empty). The SCIP graph is *richer* (materialized FILE nodes, labeled fallbacks)
— the extra nodes are not call-graph callers/callees, so they do not produce caller/callee mismatches
here. Multi-partition / fallback-heavy real repos are expected to differ (NOT RUN).

## 8. Implications for LIVEGRAPH-INTEGRATION-1C

1. **The daemon must run the SCIP producer (the only multi-second stage) ASYNC / background.** Block
   nothing on it. Use the REFRESH two-speed contract: serve last-good epoch + (optionally) an AST
   fast-delta, label `Stale`/`PrecisionPending`, atomic-swap on completion, keep last-good on failure.
2. **Partition discovery is real work** the dev-preload sidestepped: enumerate TS packages (Rust
   crates) → one `ingest_partition` per package → `feed_partition`. This is where 1C's complexity
   lives, NOT in the ingest/xref/feed (all ms).
3. **repo_uid must thread through** so SCIP-adopted keys stay byte-equal to the SQLite oracle (proven
   necessary in 1B). 1C must pass the repo's real `repo_uid` and a real `build_inputs_hash` (the 1B
   preload used `"preload"` placeholders — fine for a flag-gated demo, NOT for production coherence).
4. **xref/value-fact recompute is free (~21ms)** — 1C need not cache them; rebuild on each swap.
5. **Refresh = per-partition, coalesce bursts** (REFRESH 8.4× burst waste); invalidate only
   referencing consumers on a public-API edit (~3.5s cascade). Rust = per-crate, background-only.
6. **Keep the SQLite path as the `--engine sqlite` default + oracle** through 1C; do not decommission.

## 9. Implications for PARTITIONED-WARM-CACHE-ARCH-1

1. **Serialize `PartitionIr` ONLY** (it embeds the expensive SCIP-derived facts), keyed by
   `(partition_id, build_inputs_hash)`. Rebuild xref (`defines`/`ref_counts`) and value-fact
   placement from the loaded IR — they are ms-cheap. Do NOT serialize the xref, the answer cache, or
   "whatever exists" (the SQLite mistake).
2. **The cache is rebuildable derived state, not authority.** It MUST be invalidated by
   `build_inputs_hash` mismatch and is always reconstructable by re-indexing. User A1 authority /
   waivers live in a SEPARATE durable store and are NEVER in the warm cache.
3. **Value facts are a separate channel + epoch-bound (D7).** Either co-serialize them with the IR for
   the same epoch, or rebuild them from the cached IR — never let cached value facts attach to a newer
   IR epoch (the D7 invariant).
4. **Interning is a format input:** an interned/symbol-id key representation would shrink both memory
   and the serialized cache (the canonical-key String is the dominant allocation, §6).
5. **Format candidates (NO decision here):** rkyv / Cap'n Proto / embedded KV — chosen by
   PARTITIONED-WARM-CACHE-ARCH-1 against the IR shape + the interning decision, AFTER the runtime is
   credible (the warm cache exists only to skip the multi-second re-index; everything else is fast).

## 10. Open risks / deferred measurements

```text
- Real MULTI-PARTITION timing + key alignment: 1A/1B are single synthetic partition. (NOT RUN; XPART-1 residual)
- scip-ingest IR-construction + decode_index cost at SCALE (large index.scip). (NOT RUN)
- Canonical-key String allocation cost / interning benefit — unmeasured. (NOT RUN)
- build_inputs_hash population + reliability as the warm-cache key (1B used a placeholder). (OBSERVED gap)
- Rust value facts: absent (ScipSynthesized-dominant); value-join for Rust deferred. (RUST-INGEST)
- finalize_envelope precondition: a call-graph-incomplete defining basis (AstFileScope) without other
  degradation is not mapped to a DegradationReason. (recorded; unreachable with current fixtures)
- Fallback-heavy / cross-partition real repos: expected SCIP↔tree-sitter mismatches not yet classified. (NOT RUN)
- SQLite vs LiveGraph identity divergence for NON-adopted (ScipSynthesized) symbols. (INFERRED; NOT RUN)
```

## References
- `docs/slices/ingest-core-1.md` (`PartitionIr` / `IngestOutcome` / adoption byte-equality)
- `docs/slices/refresh-probe-1.md` (two-speed; the ~1.9–3.0s indexer + ~21ms xref evidence)
- `docs/slices/rust-ingest-prove-1.md` (per-crate ~29–32s; fallback-identity)
- `docs/slices/{xpart-prove-1,cjoin-prove-2}.md` (cross-partition + C/C++ join)
- `docs/slices/{trust-model-rebase-1,livegraph-runtime-1,query-migration-1,value-join-1}.md` (vocabulary + runtime + value facts)
- `docs/slices/livegraph-integration-1{a,b}.md` (the real-data feed + the live SQLite↔LiveGraph comparison)
- `docs/architecture/scip-migration-plan.md` (Stage D order)
