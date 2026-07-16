# SQLITE-RAW-DECOMMISSION-READINESS-1: Transition Audit (Stage D)

Slice ID: SQLITE-RAW-DECOMMISSION-READINESS-1
Status: **AUDIT — evidence map. No code, no table deletion, no migrations.** Gates any future
`SQLITE-RAW-DECOMMISSION-1`.
Track: Stage D, after WARM-CACHE-PRODUCER-ABSENT-1; BEFORE any raw decommission.

## Purpose

```text
Prove exactly which SQLite raw-graph responsibilities have been replaced by LiveGraph/warm-cache,
which remain authoritative, and which shipped query paths still depend on SQLite.
```

## Method + evidence basis

Two read-only sweeps of the workspace (the `storage` crate schema/migrations; the `rgr` CLI commands;
the `daemon-runtime` dispatch; the LiveGraph engine). Table inventory + command handlers are
**OBSERVED** (file:line cited). Role classifications and replacement judgments are **INFERRED** from
those facts. Nothing was executed; nothing was modified.

## Headline finding (the audit's reason to exist)

```text
The LiveGraph + warm-cache substrate currently replaces ONE thing: TS callers/callees — and only
NON-DEFAULT (the default engine is Sqlite). The LiveGraph holds only the TS-package PartitionIr
(call/reference edges); it does NOT hold file inventory, measurements, boundaries, surfaces, contracts,
modules, declarations, or non-TS languages. Therefore ZERO SQLite tables are safe to drop today.
```

"The new path works for callers/callees" is TRUE and INSUFFICIENT. Raw decommission is blocked.

---

## 1. SQLite responsibility inventory (33 tables, OBSERVED)

Schema: `rust/crates/storage/src/migrations/001-initial.sql` (path updated by
TS-PROTOTYPE-RETIREMENT-1 — the SQL was relocated byte-identical from the retired TS tree
into its consuming crate) + migrations `migration_003..025` under
`rust/crates/storage/src/migrations/`.

### 1a. AUTHORITATIVE (source of truth — NOT reproducible by re-indexing)
| Table | file:line | Note |
|---|---|---|
| `repos` | 001-initial.sql:11 | Repo registry (repo_uid, path, branch). The registry. |
| `declarations` | 001-initial.sql:128 | Operator-entered waivers / requirements / supersessions. Non-reproducible. |

Also authoritative but NOT a raw-graph table: the **write-authority / A1 boundary** is a runtime
**mode** (`StateRootMode` global vs sandbox in `daemon-runtime/src/state.rs`), enforced at the daemon,
not a dedicated table (OBSERVED: no `authority`/`a1` table found). Decommission does not touch it.

### 1b. DERIVED RAW GRAPH (extractor output — reproducible by re-indexing)
The historic "raw graph" the graph commands read:
| Table | file:line | Role |
|---|---|---|
| `nodes` | 001-initial.sql:76 | **THE raw graph nodes** (symbols/files/modules/…), all languages. |
| `edges` | 001-initial.sql:104 | **THE raw graph edges** (CALLS/IMPORTS/READS/WRITES/…), all languages. |
| `files`, `file_versions` | 001-initial.sql:45,59 | File inventory + parse/cache state. |
| `artifacts`, `evidence_links`, `annotations` | :164,180 / migration_006 | Artifacts + provenance + code annotations. |
| `boundary_provider_facts`, `boundary_consumer_facts` | migration_008 | Raw IPC/boundary endpoints. |
| `boundary_interaction_surfaces`, `boundary_channel_details` | migration_024 | IPC interaction surfaces (L1/L2). |
| `contract_schemas`, `contract_elements` | migration_025 | IDL/proto schema AST. |
| `semantic_facts` | migration_020 | Doc-derived semantic facts. |

### 1c. CACHE / DERIVED (computed; reproducible from the raw graph / metrics)
`measurements` (complexity/churn/coverage, migration_003), `unresolved_edges` (007), `boundary_links`
(008), `staged_edges`/`extraction_edges`/`file_signals` (009/012 — extraction staging), `inferences`
(001:146), `module_candidates`(+`_evidence`,+`module_file_ownership`) (011), `project_surfaces`
(+`_evidence`) (013), `surface_config_roots`/`surface_entrypoints` (014), `surface_env_*` (015),
`surface_fs_*` (016), `quality_assessments` (019), `status_mappings`/`behavioral_markers`/
`return_fates` (021/022/023), `generated_code_mappings`/`boundary_contracts`/
`boundary_interaction_links` (025).

### 1d. OPERATIONAL METADATA
`snapshots` (001:24 — extraction state, retention, cache epoch), `schema_migrations` (001:193),
`module_discovery_diagnostics` (017).

**Cross-cut:** of these 33 tables, exactly **two** (`nodes`, `edges`) have ANY LiveGraph counterpart,
and only for the TS-package subset. The other 31 have no LiveGraph/warm-cache representation at all.

---

## 2. Query path inventory (shipped commands, OBSERVED)

Default engine for callers/callees = **`Engine::Sqlite`** (`daemon-runtime/src/livegraph_feed.rs:82`).
`--engine livegraph|compare` is opt-in; `livegraph` silently falls back to SQLite when the partition is
unavailable (`livegraph_feed.rs:268`).

| Command | Handler | Current backend | LiveGraph replacement? | Trust-labelled? | Default path | Fallback req'd? | Decommission blocker? |
|---|---|---|---|---|---|---|---|
| callers | graph.rs:299 / dispatch:767 | daemon→SQLite; opt-in livegraph engine | **Partial** (TS only, opt-in) | partial (AnswerClass in compare/livegraph) | **SQLite** | yes (livegraph→sqlite) | **YES** — default reads nodes/edges |
| callees | graph.rs:407 / dispatch:862 | daemon→SQLite; opt-in livegraph | **Partial** (TS only, opt-in) | partial | **SQLite** | yes | **YES** — default reads nodes/edges |
| path | graph.rs:515 / dispatch:1212 | daemon→SQLite (BFS CALLS+IMPORTS) | **No** | No | SQLite | n/a | **YES** — reads edges |
| cycles | graph.rs:680 / dispatch:1124 | daemon→SQLite (SCC) | **No** | No | SQLite | n/a | **YES** — reads nodes/edges |
| dead | dead.rs:45 | **DISABLED** (false-positive rate) | No | n/a | n/a | n/a | No (disabled) — but raw-graph-shaped if revived |
| imports | graph.rs:599 / dispatch:957 | daemon→SQLite (FILE edges) | **No** | No | SQLite | n/a | **YES** — reads edges |
| stats | graph.rs:754 / dispatch:1039 | daemon→SQLite (module degree/complexity) | **No** | No | SQLite | n/a | **YES** — reads nodes/edges + measurements |
| orient | orient.rs:43 / dispatch:2146 | daemon→agent→SQLite | **No** (agent reads SQLite) | trust overlay when degraded | SQLite | n/a | **YES** — agent reads raw graph |
| explain | orient.rs:340 / dispatch:2330 | daemon→agent→SQLite | **No** | trust overlay | SQLite | n/a | **YES** |
| check | orient.rs:222 / dispatch:2268 | daemon→agent→SQLite (waiver expiry) | **No** | signals + display_name | SQLite | n/a | **YES** (also reads `declarations`) |
| modules (list) | modules/list.rs:37 / dispatch:5719 | daemon→SQLite + classification | **Partial** (module_candidates) | violations_warning advisory | SQLite | n/a | **YES** — module/surface tables |
| surfaces (list) | surfaces.rs:61 / dispatch:4130 | daemon→SQLite (project_surfaces) | **No** | No | SQLite | n/a | **YES** — surface tables |

**Every shipped graph command's DEFAULT path is SQLite.** Only callers/callees even have a LiveGraph
option, and it is opt-in + TS-only + falls back to SQLite.

---

## 3. Table-level decommission candidates

| Classification | Tables | Rationale |
|---|---|---|
| **KEEP authoritative** | `repos`, `declarations` | Source of truth; not reproducible. (Plus the A1 write-mode, not a table.) |
| **KEEP operational** | `snapshots`, `schema_migrations`, `module_discovery_diagnostics` | Schema/indexing bookkeeping. |
| **KEEP until query migrated** | `nodes`, `edges` | The only DROP-*eligible* raw graph, BUT read by the DEFAULT path of callers/callees/path/cycles/imports/stats/orient/explain/check. Not droppable until ALL migrate AND the LiveGraph covers all languages (today: TS only). |
| **KEEP until query migrated** | `files`, `file_versions`, `measurements`, all module/surface/boundary/contract/semantic/status/behavioral/return-fate tables | Derived, but have **NO LiveGraph representation** — they power surfaces/modules/boundary/quality/orient. The warm-cache PartitionIr does not hold them. |
| **DROP derived raw graph (now)** | — none — | No raw-graph table has a complete, default replacement today. |
| **UNKNOWN** | `staged_edges`, `extraction_edges`, `file_signals`, `inferences`, `unresolved_edges` | Transient/diagnostic; likely droppable independently of LiveGraph but need their own readers audited before removal (out of scope for THIS audit). |

**Net: zero tables are safe to drop now.** `nodes`/`edges` are the eventual target, gated on §5.

---

## 4. Fallback contract (DECISION — surfaced for ratification)

The question: does the default `rmap` stay SQLite until full command migration, or does LiveGraph
become the default for callers/callees first?

| Option | Default path | Trade-off |
|---|---|---|
| **(A) SQLite stays default until ALL graph commands have a trust-labelled LiveGraph path** | sqlite | one atomic cutover; no mixed-backend window; cost: long road; LiveGraph stays opt-in for a long time |
| **(B) LiveGraph becomes default for callers/callees first; others stay SQLite (mixed)** | livegraph for callers/callees, sqlite for the rest | proves the migration on the two ready commands; cost: a MIXED-backend default (two engines live), and the LiveGraph must be populated (refresh/warm-start) or fall back — a freshness/coverage cliff for repos never refreshed |
| **(C) Per-command migration with a trust-labelled SQLite fallback retained even after LiveGraph default** | per-command | safest correctness; cost: every command carries two code paths + a fallback contract |

Lean: **(C) per-command migration, SQLite retained as a labelled fallback** — but this is a
load-bearing default-backend decision and is **NOT decided here**; it is the gating decision for
QUERY-MIGRATION-CLI-1. The audit's point stands regardless: callers/callees readiness ≠ raw-graph
decommission readiness.

Hard rule (independent of A/B/C): **raw `nodes`/`edges` must not be dropped while ANY default command
path reads them.** Today every default graph command does.

## 5. Deletion safety (the gates a future SQLITE-RAW-DECOMMISSION-1 must clear, per table)

```text
A raw table may be removed ONLY when ALL hold:
1. no shipped command depends on it by DEFAULT (not just opt-in) — verified per command, all languages
2. the LiveGraph/warm-cache (or another store) covers the SAME data for ALL languages, not just TS
3. a migration / backward-compatibility story exists (old DBs: re-index, not silent breakage)
4. an operator reset story exists (how to rebuild after deletion; the cache is disposable, the raw
   graph today is NOT — it is the only multi-language store)
5. tests prove each affected command's behavior on the new backend (trust-labelled, parity-checked
   against the SQLite path — the existing `--engine compare` is the parity harness)
```

Current status against the gates: **gate 1 fails** (every default graph command reads nodes/edges),
**gate 2 fails** (LiveGraph is TS-only and holds graph topology only — no boundaries/surfaces/
contracts/measurements), so gates 3–5 are not yet reachable.

## 6. Next slices (expected; confirms the user's prediction)

```text
QUERY-MIGRATION-CLI-1     — make callers/callees a trust-labelled LiveGraph default (ratify §4 first);
                            keep SQLite as a labelled fallback; use --engine compare as the parity gate.
PATH-CYCLES-LIVEGRAPH-1   — path + cycles over the LiveGraph (BFS / SCC on PartitionIr edges);
                            imports + stats likely fold in here (all are edge/degree traversals).
ORIENT-EXPLAIN-TRUST-1    — orient/explain/check read the raw graph via the agent; route them through
                            the trust-labelled substrate (also: check reads `declarations` — authoritative,
                            stays SQLite).
SQLITE-RAW-DECOMMISSION-1 — ONLY after the above + §5 gates; and even then it drops `nodes`/`edges`
                            ONLY, never the authoritative/operational/boundary/surface/contract tables.
```

Note: `modules`/`surfaces` + the boundary/contract/measurement families are NOT covered by the
LiveGraph at all and may NEVER move — their tables are likely permanent (a "raw decommission" that is
really a `nodes`/`edges`-only retirement, not a SQLite retirement).

## Deliverable / guardrails honored
```text
No code. No table deletion. No migrations. Audit doc only.
```

## References
- `rust/crates/storage/src/migrations/001-initial.sql` (path updated by TS-PROTOTYPE-RETIREMENT-1) + `src/migrations/migration_0NN.rs`
- `rust/crates/rgr/src/commands/{graph,orient,dead,modules,surfaces}.rs`
- `rust/crates/daemon-runtime/src/dispatch.rs` (route table ~219; handlers 767–5719)
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (Engine::Sqlite default :82; fallback :268)
- `docs/slices/warm-cache-*.md`, `docs/slices/livegraph-integration-1*.md`, `docs/slices/query-migration-1.md`
