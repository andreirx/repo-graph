# Structure reading — repo-graph at 2524493 (v0.18.0 + catalog), 2026-09-13

Manager reading at the human's request: "look at the repo-graph current structure and architecture and using rmap check any bloated modules — a command router may be bloated, but any leaf module that is overly complex needs a good explanation." Evidence: `rmap refresh` of repo-graph on the production daemon (snapshot 2026-09-13T06:12Z; 1,000 source files, 17,626 symbols), then `rmap orient --full`, `stats`, `cycles`, `modules list`, `hotspots --exclude-tests`, `trust`; cross-checked with `cargo metadata` (crate graph) and `wc -l` net of inline `#[cfg(test)]` blocks. Evidence labels: EXECUTED (rmap/cargo/wc) unless stated.

> Note 2026-09-13: the requirement RG-REQ-016 drafted from this reading was RETIRED the same day by the human (agents already over-preserve structure; refactoring is decided case by case). The reading stands as evidence; its refactor candidates are not obligations.

## 1. Shape of the workspace

54 workspace crates (58 Cargo.toml members incl. tools/probes); `rmap stats`: 71 package groups, 170 directory groups, 17,626 symbols. Source: 669 `.rs` files under `crates/*/src` (tests excluded).

| crate | src LOC | files | avg/file | role |
|---|---|---|---|---|
| daemon-runtime | 74,741 | 128 | 584 | application layer + router (dispatch) |
| rgr | 73,584 | 167 | 440 | CLI / presentation (composition root) |
| storage | 55,535 | 106 | 524 | adapter (SQLite) |
| agent | 22,222 | 52 | 427 | core (orient/explain/check policy, DTOs, ports) |
| indexer | 20,703 | 27 | 767 | core pipeline + resolver |
| repo-index | 18,280 | 24 | 762 | index composer (application) |
| classification | 9,544 | 20 | 477 | core |
| ts-extractor | 9,147 | 10 | 915 | leaf (language) |
| repo-graph-livegraph | 6,495 | 7 | 928 | leaf (in-memory graph) |
| trust | 5,667 | 8 | 708 | core |
| cpp-extractor | 4,352 | 5 | 870 | leaf (language) |
| c-extractor | 4,029 | 4 | 1,007 | leaf (language) |
| python-extractor | 2,854 | 3 | 951 | leaf (language) |
| java-extractor | 2,680 | 4 | 670 | leaf (language) |
| repo-graph-scip-ingest | 1,657 | 1 | 1,657 | leaf, single file |
| repo-graph-warm-cache | 1,437 | 1 | 1,437 | leaf, single file |
| repo-graph-coherence | 1,003 | 1 | 1,003 | leaf, single file |

**Guardrail status (CLAUDE.md: "do not append new responsibilities to files over 500 lines"):** 222 of 669 source files exceed 500 lines; 81 exceed 1,000; 20 exceed 2,000. The guardrail has never been checked mechanically.

## 2. Crate dependency graph (cargo metadata, EXECUTED)

- Acyclic. `rmap cycles`: one type-only cycle, in `tools/rgistr` (TS), plus one test-fixture cycle — none in `rust/crates`.
- Composition roots: `rgr` depends on 24 crates (incl. `storage`, `indexer`, `repo-index`); `daemon-runtime` on 29; `rmapd` on `daemon-runtime`. Expected.
- Adapters depend inward: `storage` → `agent` (45 file-level imports), `storage` → `indexer` (44) — the port implementations. Correct direction.
- **Inward-rule violation:** `quality-policy` (a policy crate) imports `repo_graph_storage::types::{QualityPolicyKind, QualityPolicyPayload, ScopeClause, ScopeClauseKind, QualityPolicySeverity}` (`lib.rs:79`, `assess.rs:34`). Domain types owned by the storage adapter, consumed by core. Smallest fix within the architecture: move those types to `quality-policy` (or a small core types crate) and have storage depend on them — a dependency-edge change, therefore an authorized decision (RG-REQ-016-L05), not a drive-by.
- `module-queries` → `storage` (uses `StorageConnection` directly in `violations.rs`): a read-model/query crate sitting on the adapter. Application-layer placement; acceptable, recorded.
- **Orphan crate:** `repo-graph-detectors` (fan-in 0; 12 files; 3,338 src + 4,176 test lines; its header says it is "the sole implementation" of the seam-detector substrate after TS-PROTOTYPE-RETIREMENT-1). Nothing in the workspace depends on it — dormant capability (RG-REQ-016-L08). Disposition needed: wire, retire, or record.
- **Daemon-authority check:** the CLI uses `StorageConnection` directly in `rgr/src/cli/{context,snapshot_hint,envelope}.rs` and `commands/hook/session_start.rs`. `hook --db` is a documented offline path; the three `cli/*` sites need classification (RG-REQ-016-L03).
- Fat ports: `agent/src/storage_port.rs` 43 methods / 29 types, 1,224 lines; `indexer/src/storage_port.rs` 49 methods / 46 types, 1,150 lines. Never challenged (RG-REQ-016-L09).

## 3. Where the complexity is (rmap orient --full, EXECUTED)

Top complexity centers (cyclomatic): `storage/src/indexer_impl.rs — copy_forward_unchanged_files (cx 93)`; `storage/tests/parity.rs — dispatch_op (cx 92)` (a TEST file ranked #2 — RC-9 live on our own repo); `indexer/src/orchestrator.rs — run_pipeline (cx 76)`; `storage/src/migrations/mod.rs — run_migrations (cx 76)` (a linear migration list — benign); `daemon-runtime/src/callgraph_cert/ledger.rs — build_witness_ledger (cx 74)`; `daemon-runtime/src/dispatch.rs — dispatch (cx 72)`, `handle_enrich (60)`, `handle_modules_list (41)`; `storage/src/boundary_interaction_read_impl.rs — get_boundary_interaction_detail (70)`; `refresh_copy_forward_impl.rs — copy_forward_boundary_surfaces (62)`, `copy_forward_contract_schemas (51)`; `repo-index/src/compose.rs — refresh_into_storage_with_progress (61)`, `index_into_storage_with_progress (51)`; `c-extractor/src/boundary_detector.rs — try_extract_boundary_call (60)`; five `rgr/src/commands/declare/*` handlers at cx 40–57 (argument parsing — governance, frozen); `rgr/src/main.rs — main (51)`.

Hotspots, 90 days, tests excluded (score = lines changed × Σ complexity): `dispatch.rs` 8,998,528 (churn 7,744, cx 1,162) — seven times the next; `livegraph_feed.rs` 1.28M; `compose.rs` 1.15M; `queries.rs` 0.99M; `cpp-extractor/extractor.rs` 0.84M; `ts-extractor/extractor.rs` 0.43M; `indexer/resolver.rs` 0.37M; `reclaim.rs`, `enrich_pass.rs`, `orchestrator.rs`, `doctor/daemon_info.rs`, `enrichment_impl.rs`, `presentation/map.rs`, `trust_impl.rs`, `repo-index/config.rs`.

## 4. The big files, by role (net of inline tests; fns = functions, pub = public)

| file | net lines | fns | pub | types | role and reading |
|---|---|---|---|---|---|
| daemon-runtime/src/dispatch.rs | 10,113 | 88 | 8 | 1 | ROUTER — expected large, BUT it holds the handler bodies (`handle_enrich` cx 60, `handle_modules_list` cx 41, `handle_index` cx 35, `handle_cycles` cx 31 …): routing plus application logic in one file, and the #1 hotspot by 7×. A router that routes is fine; this one also composes. Local extraction of handler bodies into `handlers/*` (the crate already has a `handlers/` tree) preserves every boundary — no authorization needed, but it is a slice, not a drive-by. |
| repo-index/src/compose.rs | 4,614 | 57 | 13 | 18 | COMPOSER of the index pipeline — large by nature; `refresh_into_storage_with_progress` (cx 61) and `index_into_storage_with_progress` (cx 51) duplicate structure (the five `is_generated: false` construction sites are here and in `orchestrator.rs`). |
| daemon-runtime/src/livegraph_feed.rs | 3,554 | 78 | 36 | 20 | application/adapter for the in-memory graph — 78 small functions; size without depth (no fn over 150 lines). Not bloated in the harmful sense; cohesion question only. |
| storage/src/queries.rs | 3,231 | 55 | 44 | 33 | ADAPTER god-file: 44 public read functions for unrelated surfaces (callers, dead, complexity, module stats, coupling). Not a leaf; bloat by accretion. `compute_module_stats` here is a THIRD module population (RG-REQ-004-L11). |
| ts-extractor/src/extractor.rs | 2,523 | 56 | 1 | 2 | LEAF (TypeScript). One public fn; 56 private walkers. Explanation: one language's tree-sitter walk — every construct family (imports/exports/calls/classes/JSX/require) is a function; the size is grammar coverage, not mixed responsibility. Acceptable leaf size; the risk is a long file hiding unconsumed facts (the C++ sibling hid RC-1's receiver). |
| cpp-extractor/src/extractor.rs | 2,425 | 67 | 1 | 5 | LEAF (C++). Same explanation; contains a 271-line builtin table (`cpp_runtime_builtins`) that is data, not code — a candidate for a data file, a local refactor. This is where the v0.18.0 regression lived. |
| repo-graph-livegraph/src/lib.rs | 2,368 | 55 | 35 | 26 | LEAF library with 26 types and 35 public fns in one file while the crate has 6 other files — a module-layout problem (everything in `lib.rs`), not a responsibility problem. Local split. |
| indexer/src/orchestrator.rs | 2,151 | 14 | 2 | 3 | PIPELINE — 14 functions averaging 150 lines; `run_pipeline` cx 76 and `refresh_repo` cx 34 are the two long spines; `is_generated: false` hard-coded at five sites is the symptom. Needs explanation → given: fresh-index and refresh pipelines are written twice in parallel. |
| storage/src/agent_impl.rs, indexer_impl.rs, trust_impl.rs, enrichment_impl.rs, boundary_interaction_read_impl.rs | 835–1,887 | — | — | — | ADAPTER port implementations — one file per port; large because the ports are fat (43/49 methods). The width of the port is the cause. |
| python-extractor 1,529 · java-extractor 1,520 · c-extractor 1,053 (+ boundary_detector 866) · rust-extractor 1,013 | — | 30–40 | 1 | 2–3 | LEAVES (languages) — same explanation as TS/C++; proportionate to grammar size. |
| indexer/src/resolver.rs | 1,434 | 29 | 10 | 8 | CORE policy — the shared binding path for every language (Q1/Q9 touch it). `resolve_call_target` 180 lines. Size is policy accretion per language; acceptable, but every language-specific branch added here widens a shared hot path (RG-REQ-005-L01's gate rule exists for this reason). |
| daemon-runtime/src/callgraph_cert/ledger.rs | 1,416 | 18 | 16 | 23 | LEAF (LiveGraph witness ledger) — `build_witness_ledger` cx 74: one function carrying the certificate's case analysis. Explanation needed — a single exhaustive classification of edge witness states is inherently branchy; a sum-type dispatch would make the exhaustiveness visible. Candidate, not a defect. |
| agent/src/storage_port.rs · indexer/src/storage_port.rs | 1,224 · 1,150 | 43 · 49 | — | 29 · 46 | PORTS — fat interfaces (RG-REQ-016-L09). |
| agent/src/dto/signal.rs | 1,081 | 18 | 12 | 50 | DTO catalogue — 50 types and three cx-31 `match`es (`as_str`, `tier_priority`, `descriptor`) over the same enum: variants fixed, operations growing — the correct dispatch axis; the three matches are the price, made visible by exhaustiveness. Fine. |
| repo-graph-scip-ingest/src/lib.rs 1,269 · repo-graph-warm-cache/src/lib.rs 1,057 · repo-graph-coherence 1,003 | — | — | — | — | SINGLE-FILE CRATES — a crate boundary was earned (each is a distinct mechanism) but no internal module layout followed; local layout debt. |
| rgr/src/commands/doctor/daemon_info.rs | 967 | 16 | 1 | 1 | LEAF renderer of `doctor`'s daemon section — 16 probes in one file; fine. |
| rgr/src/presentation/map.rs | 950 | 21 | 4 | 8 | LEAF renderer — fine. |
| trust/src/service.rs | 988 | 8 | 5 | 3 | CORE — 8 functions averaging 120 lines; `compute_trust_report_cancellable` 357 lines: the trust assembly spine; long but single-purpose. |

## 5. Reading

1. **Router bloat is real and mixed.** `dispatch.rs` is not only a router: 88 functions, most of them handler bodies with application logic (cx 31–60), 10k lines, and the dominant 90-day hotspot. Every slice since May has touched it. The smallest architecture-preserving move is mechanical: handler bodies to `daemon-runtime/src/handlers/<family>.rs` (the directory already exists), `dispatch` keeps the match. No boundary moves; it is a local refactor but a sizeable one — a slice, scheduled, not folded into Q1.
2. **Leaf complexity has an explanation in every case but two.** The language extractors are large because grammars are large — one responsibility each, one public function; the ledger is branchy because a certificate is a case analysis. The two without a satisfying explanation: `repo-graph-livegraph/src/lib.rs` (a 7-file crate with everything in `lib.rs`) and the three single-file crates — layout debt, not design debt.
3. **The adapter is where responsibility has leaked.** `queries.rs` (44 public reads), the `*_impl.rs` files (one per fat port), a connectivity rule in SQL (RC-5), a vendored predicate in a handler module (RC-9). These are the places the catalog's unmet Ls live, and RG-REQ-016-L02 names the rule.
4. **Two architecture-level findings need decisions:** `quality-policy` → `storage` types (inward-rule violation; fix changes a dependency edge), and `repo-graph-detectors` with no dependent (dormant crate; wire/retire/record).
5. **The guardrail was never enforced.** 222 files over 500 lines; a mechanical "grew past 500 in this diff" check in the gate (RG-REQ-016-L04) turns the rule from prose into review input.

## 6. What this reading does NOT claim

No function-level claims beyond rmap's cyclomatic numbers and `wc`; the "fns/pub/types" columns are regex counts (approximate). No claim that any file is wrong — only where size has a reason and where it does not. Refactors proposed here are candidates for slices under RG-REQ-016, each needing its own Regression watch; none is authorized by this document.
