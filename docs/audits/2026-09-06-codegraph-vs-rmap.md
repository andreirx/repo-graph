# codegraph 1.6.0 vs rmap 0.17.0 — head-to-head usefulness, 2026-09-06

Method: same metric as `docs/audits/2026-09-03-zg-vs-rmap-find.md` — per capability × repo × tool, HIT /
EVIDENCE / HONESTY / ECONOMY graded A–F, ground truth read from source with the exact search stated. Single
run, single grader. Raw captures: `/private/tmp/cg-cmp/captures/<tool>-<repo>-<capability>.txt` (47 files).

Setup (verified): codegraph built from the clone (`npm ci` 3 s, `npm run build` 7 s, Rust kernel 44 s → 34 MB
`.node`), run via `/private/tmp/cg-cmp/cg` with CODEGRAPH_TELEMETRY=0 DO_NOT_TRACK=1 CODEGRAPH_NO_UPDATE_CHECK=1
(telemetry "disabled"). No global install; `~/.codegraph` never created; no git hooks; no daemon left. Storage
`<repo>/.codegraph/codegraph.db`. rmap: read-only against the retained audit-v0.17.0 root (socket
`daemon.sock`; the retained daemon was down — rmapd started by hand under the same env, pid 5301, killed at the
end). No index/refresh/repo run. repo-graph queried at the real path (snapshot 50d06727, basis 28b2ff6);
codegraph indexed a throwaway worktree at HEAD 1f7ed71.

## (a) Grade matrix — capability × repo × tool (HIT / EVIDENCE / HONESTY / ECONOMY)

| Capability | Repo | rmap | codegraph |
|---|---|---|---|
| 1 Overview | leveldb | **A- / B / A / B** | D / C / B / D |
| 1 Overview | django | B- / B / A / C | D- / C / B / F |
| 1 Overview | petclinic | **B+ / B / A / A** | C- / B- / B / D |
| 1 Overview | FRAKTAG | **A / B+ / A / B** | D / C / B / D |
| 1 Overview | repo-graph | B- / B / **C** / C | D- / C / B / D |
| 2 Symbol+callers | leveldb | C / B- / B / C | **B+ / A- / C / B** |
| 2 Symbol+callers | django | C- / B / B / C | B- / A- / C- / C- |
| 2 Symbol+callers | petclinic | C+ / B+ / B / C- | **A- / A / B / B+** |
| 2 Symbol+callers | FRAKTAG | B- / B / A- / B | **B+ / A- / C+ / B+** |
| 2 Symbol+callers | repo-graph | B- / B+ / B+ / C | B / A- / **D** / C |
| 3 Concept | leveldb | C- / C / A / A- | **A / A / B / C-** |
| 3 Concept | django | D / C / A / A- | **A- / A / B- / D** |
| 3 Concept | petclinic | C+ / C+ / A / B- | **B+ / A- / B- / D** |
| 3 Concept | FRAKTAG | D+ / C / A / B+ | **B- / A- / B- / D** |
| 3 Concept | repo-graph | **A- / B / A / B+** | D / B / C / F |
| 4 Routes | petclinic | **A / A / A / A-** | A / A / B+ / B |
| 4 Routes | FRAKTAG | A / B- / A / B | **A / A / B / B+** |
| 5 Impact | leveldb | C- / B / A / B | **B / C+ / C / C-** |
| 5 Impact | django | C- / B / A / B+ | **B+ / B / C- / D** |
| 5 Impact | petclinic | C- / B / A / B+ | C / B / C / C- |
| 5 Impact | FRAKTAG | D+ / C / A / A- | **B / B+ / C+ / C-** |
| 5 Impact | repo-graph | C / B+ / A / C | C+ / B / C / F |
| 6 Change handling | leveldb scratch | C / B / A / D | **A / A / B+ / A** |
| 7 Honesty | all | — / — / **B+** / — | — / — / **C-** / — |

Dimension averages (22 cells): rmap HIT C+, EVIDENCE B-, HONESTY A-, ECONOMY B-; codegraph HIT B-, EVIDENCE B+,
HONESTY C, ECONOMY D+. The zg pattern repeats and sharpens: codegraph finds and shows (verbatim source,
caller/callee trail, routes with lines); rmap orients at module level, says what it does not know, and costs
3–8× fewer bytes. codegraph indexed all five languages natively.

## (b) Per-capability verdicts (quoted outputs; ground-truth searches in the captures)

### 1. Overview — rmap 5–0
rmap has a module model; codegraph has no overview verb (`explore` is keyword retrieval; its `context` says
"⚠️ Low-confidence match … Do not assume the list above is comprehensive" on all five repos). rmap leveldb:
"133 files, 2112 symbols · 8 package groups … Module edges: db → include (89), db → util (48) … 1 import cycle
(db -> table -> db) … calls 27% resolved (LOW) … no semantic-resolution path exists for C++". rmap FRAKTAG:
5 modules, 93 HTTP surfaces (47/46), type-only cycle verdict, and "modules are likely connected via HTTP route
match (heuristic, 36 links)". rmap's one defect: repo-graph `modules list` "No cross-module dependencies … all
imports are intra-module" — FALSE (the D1 class; fixed by IMPORT-RESOLUTION-RUST-1 in flight: 0 → 129).

### 2. Symbol + callers — codegraph 3, rmap 1, tie 1
Ground truth: leveldb DBImpl::Recover def db_impl.cc:292, ONE caller (DB::Open at :1511); django
BaseHandler.get_response base.py:138, callers wsgi.py:124, test/client.py:186, exception.py:43/56; petclinic
OwnerController :49, referenced by OwnerControllerTests:58; FRAKTAG logTurn :133, callers index.ts:1915/1990;
repo-graph ServiceDispatcher dispatch.rs:129, .dispatch( callers socket.rs:261, stdio.rs:163, transport
dispatch.rs:243….
rmap `find` is excellent and typed, but the HAND-OFF from find to explain/callers is broken three ways:
`explain DBImpl::Recover` → "Target: DBImpl::Recover (unresolved: no_match) / Confidence: high" (a string find
just resolved); `callers leveldb::DBImpl::Recover` → "ambiguous … 1. db_impl.cc 2. db_impl.h" (decl vs def);
`callers OwnerController.processCreationForm` → "symbol not found" while its own hint list contains the FQN.
Only the full stable key works, and then "0 callers found" (truth 1); callees list Status::ok ×5 and miss
VersionSet::Recover/NewDB/RecoverLogFile. `callers BaseHandler.get_response` → 0 (truth ≥3); callees
log_response, set_urlconf — exactly right, nothing invented. `callers ServiceDispatcher.dispatch` → 1
(a test; "CALLS inferred" — honest), trait-routed production callers missed.
codegraph `node DBImpl::Recover` prints the body + "Calls → CreateDir (helpers/memenv/memenv.cc:331) …
GetChildren (db/db_test.cc:90) … Called by ← Open (db/db_impl.cc:1503)". Caller right (1/1); callee bindings
WRONG: env_ is an abstract Env*; codegraph bound the in-memory TEST env and a db_test.cc MOCK and printed them as
fact. Same class on django (`append` → gis/geos/mutable_list.py:188 for a list append; ClientHandler.__call__
attributed to FSFilesHandler.get_response) and FRAKTAG (getTree → KnowledgeBase.ts:208, receiver is a TreeStore).
Where names are unique codegraph is precise and complete (logTurn callees 5/5; petclinic "callers
processCreationForm (1): route POST /owners/new :77" — the route as caller). repo-graph: "callers dispatch (20)"
= 20 tests; the 38 real callers appear only with --limit 500; "(20)" printed as if total (60 real).

### 3. Concept search — codegraph 4, rmap 1
leveldb crash recovery: codegraph explore lands DBImpl::Recover / VersionSet::Recover / NewDB with bodies;
rmap ranks leveldb_repair_db / RepairDB first, Recover at 0.30 (the wrong mechanism first, as in the zg round).
django "responses converted to bytes": codegraph → http/response.py (make_bytes :310, serialize :391); rmap →
ASGIHandler.send_response, GZipMiddleware … response.py never appears. petclinic "where is an owner saved":
codegraph dumps OwnerController/PetController/Owner (the four .save lines are in there, unpointed); rmap right
class, processCreationForm rank 9, no save. FRAKTAG persistence: neither reached JsonStorage (writeFile :69);
codegraph got ConversationManager, rmap UI props (the D11 class). repo-graph "old snapshots cleaned up": rmap
retention/prune.rs delete_snapshots_cascade (0.57) FIRST; codegraph keyword hits on "clean"/"snapshot",
prune/retention never appear. `find --text "<NL sentence>"` → 0 matches on all five, labelled honestly.

### 4. Routes — tie on HIT; codegraph better anchors, rmap better honesty
Both 17/17 petclinic and 47/47 FRAKTAG. rmap flags 4 test consumers and dynamic URLs; rmap FRAKTAG providers
have NO line anchors (consumers do); codegraph "route DELETE /api/conversations/:sessionId server.ts:241" ×47.

### 5. Impact — codegraph 3, rmap 0, tie 2 (codegraph honesty weak)
rmap has no file-level "who depends on this": map/explain <file> are OUTBOUND; only `modules deps --inbound`
answers, at module level. risk/assess honest everywhere ("history is shallow", "not armed"). codegraph `impact
BaseHandler — 88 affected` lists the true dependents (ASGIHandler, WSGIHandler, ClientHandler, get_wsgi_
application…). Defects: `impact DBImpl` mis-groups db_iter.cc symbols under db_impl.cc; `affected base.py` → 27
test files incl. a QUnit JS file with no imports; `affected OwnerController.java` → "No test files affected"
though OwnerControllerTests references it; `affected dispatch.rs` → 167 test files (≈ every test); `impact
findConfig` merged a different same-named function; explore "what depends on <file>" is keyword-driven.

### 6. Change handling — codegraph A, rmap C
codegraph on a leveldb scratch copy: init 0.98 s; 3-line edit → `sync` "1 changed file — 89 nodes in 191 ms"
(0.50 s wall); callees show the new function immediately; no-op sync 0.23 s. rmap: `refresh` has no --help;
documented no-change cost ~84–102 s on django (not run); no watcher; the drift line on every answer is the
honest half.

### 7. Honesty
rmap wrong/over-claims: `explain <find-resolved string>` → "unresolved: no_match / Confidence: high" (the wrong
knob); the intra-module hint on the Cargo workspace; bare "0 callers found" for symbols with real callers (the
LOW banner lives in explain, not on callers). Limitation statements verbatim: no C++ resolver; JDTLS required;
"conservative: 6358 of 8750 calls unclassified"; "ranked guesses, not facts"; "<dynamic …>"; "showing 8 of 29";
shallow history; not armed. Term counts: LOW 74, inferred 30, Confidence 15, unresolved 14, conservative 10.
codegraph wrong/over-claims: fabricated bindings printed as facts (test mock, GIS mixin, wrong getTree); the
silent --limit 20 rendered as the total; affected false positives/negatives; impact mis-grouping; caller
reported at the callee's definition line not the call site (rmap same). Limitations: context's low-confidence
warning; node "Structural outline only"; "+N more" (77×); explore never qualifies its retrieval; NO per-edge
confidence in any output.

## (c) Index / update cost

| Repo | cg files | cg init wall | cg nodes/edges | cg db | rmap files/symbols | rmap db | rmap index (approx.) |
|---|---|---|---|---|---|---|---|
| leveldb | 134 | 2.15 s | 2,911 / 9,810 | 7.6 MB | 133 / 2,112 | 30 MB | ~0.6 s |
| django | 2,987 | 7.83 s | 61,197 / 199,088 | 162 MB | 3,019 / 78,214 | 704 MB | ~14 s |
| petclinic | 72 | 0.50 s | 857 / 1,250 | 2.4 MB | 50 / 290 | 6 MB | ~0.1 s |
| FRAKTAG | 89 | 0.81 s | 1,216 / 3,387 | 4.6 MB | 92 / 1,291 | 14 MB | ~0.4 s |
| repo-graph | 982 | 4.38 s | 23,038 / 78,885 | 112 MB | 1,053 / 17,100 | 304 MB | ~7 s |
| one-file edit (leveldb) | — | sync 0.50 s (191 ms work); no-op 0.23 s | | | refresh no-change ≈ 84–102 s (django, documented) | | |

rmap keeps snapshot history + seed vectors + measurements (django 704 MB vs 162 MB). Query latency: codegraph
0.16–0.53 s per CLI call (Node start-up); rmap 0.03–1.3 s via daemon, django orient --budget large 3.8 s.

## (d) Imitate / keep / verdict

IMITATE (ranked): (1) a route/surface as a CALLER ("callers processCreationForm (1): route POST /owners/new
:77") — rmap has the fact in find's http-surface rows but callers says not found; (2) ONE verb that resolves
find's own printed row (accept the qualified name; dedupe decl/def, prefer the definition) — today explain says
no_match/Confidence: high for it; (3) verbatim, line-numbered, BOUNDED source for the resolved symbol on
explain (what won every concept question on doc-rich code); (4) file-level "what depends on this" + symbol-level
impact with the LOW banner; (5) second-scale incremental update; (6) line anchors on every provider row (TS).
KEEP: orientation with a module model and edges; honest floors on every answer; no fabricated edges; typed rows
with certainty classes; drift/freshness disclosure; economy (171 KB vs 483 KB for the same questions).
VERDICT: codegraph is the better retriever and reader and the worse witness — it binds virtual/duck-typed calls
to whichever same-named definition it finds (a test mock, a GIS mixin) and prints a 20-row default as a total;
an agent trusting a codegraph row will sometimes edit the wrong file with full confidence. rmap is the better
orienter and witness, invented no edge in this run — but its find → explain/callers hand-off is broken for C++
and Java qualified names, its call graph under-reports, it has no symbol impact or file dependents, TS providers
lack lines, and refresh costs ~100 s where codegraph's costs 0.2 s. Route-as-caller and `impact` are the two
codegraph verbs agents will feel most.

## (e) Cleanup — verified
`.codegraph/` removed from all four corpus repos (git status: only pre-existing untracked items); repo-graph
never written; worktree removed; audit rmapd pid 5301 killed (only the operator's real 29112 remains; 0 socket
holders); registry sha256 4762fd94…d64c identical before/after; no ~/.codegraph, no global install; build
artifacts inside the clone only (codegraph-kernel/target 335 MB); /private/tmp leftovers: /private/tmp/cg-cmp.

---

# CodeGraph (b9ca4b7) — structural investigation (2026-09-06, read-only)

Root: legacy-codebases/codegraph. TS engine src/ 109,357 LOC; Rust kernel ONE crate 24,626 LOC (per-language
walkers 600–2,000 LOC; napi cdylib, NOT standalone — parse+extract only); Svelte viewer 24,108 LOC; 236 test
files / 3,774 cases; kernel 21 #[test]s; no snapshot tests.

## 1. MODULE MAP
CLI (commander) / MCP (stdio proxy per host → one detached daemon per project root over a Unix socket; shared
CodeGraph + watcher + SQLite writer; idle 300 s; read tools on a 16-worker query pool) / UI server → the
`CodeGraph` facade (src/index.ts:142, the ONLY public surface) → extraction (tree-sitter wasm | kernel),
resolution (imports, name-match, frameworks, ~45 synth passes), graph (traversal, type-hierarchy, dead-code,
flow), context, search, sync (FileWatcher, git hooks) → db (node:sqlite, WAL+FTS5) → kernel (optional napi FFI,
synchronous in-process; ABI + kind-table check at load; per-file `defer:` fallback to wasm; kill switch
CODEGRAPH_KERNEL=0). Boundary DTO: (path, content, language) in; five Buffers out (meta 36 B, nodes 96 B rows,
edges 44 B, refs 40 B, UTF-8 arena; kinds cross as indexes into shared tables "append, never reorder").
Distribution: bundled Node 24 + node:sqlite, zero native addons except the kernel.

## 2. GRAPH MODEL
23 node kinds (file…route, component, union); 13 edge kinds (contains, calls, imports, exports, extends,
implements, references, type_of, returns, instantiates, overrides[never emitted], decorates, navigates).
Provenance 'tree-sitter'|'scip'[no producer]|'heuristic' (every synthesized edge). IDENTITY:
id = kind:sha256(path:kind:name:LINE)[..32] — any line shift changes every id below it; a rename changes every id
in the file; compensated at the EDGE level (§4). Edge identity UNIQUE(source,target,kind,line,col), INSERT OR
IGNORE. Cross-file resolution = tree-sitter + heuristics ONLY (no LSP/SCIP/type-checker; receiver types by regex
over source lines). Ladder: knownNames prefilter → function_ref → JVM FQN import → framework resolvers (≥0.9
short-circuit) → import resolver (TS/JS/ArkTS/Svelte/Vue/Astro/Py/Go/Java+Kotlin/PHP/C+C++) → name-matcher
(filePath → qualified → call-chain → method → exact → fuzzy) → chained-call deferral; batched 5,000 refs on a
read-only worker pool. Every edge carries metadata.confidence/resolvedBy/refName/refKind. ~45 whole-graph
synthesizer passes (callbacks, EventEmitter, React/Vue/Svelte/Flutter/ArkUI, C fn-ptr, Go iface, gRPC, RN,
Fabric, Expo, MyBatis, Gin, thunk/RTK/Pinia, Celery, Spring, MediatR, Sidekiq, Laravel, Nix…). Framework routes:
per-framework extract() mints `route` nodes (Express regex over app|router.METHOD; Next.js app/pages files;
Expo Router; TanStack; Vue Router; React Router; SvelteKit) and resolve() emits `navigates`; computed
destinations left unresolved. iOS/RN bridging: Swift↔ObjC selector name math (confidence 0.6);
RCT_EXPORT_* / @ReactMethod / TurboModuleRegistry maps (0.95 with receiver evidence, 0.6 bare); Fabric suffix
probing. Type hierarchy / dead code (17 named subtractions incl. unresolvedName, ambiguousName) / branch guards
(re-parse at query time; '' on drift) are QUERY-TIME, read-only, no invented edges.

## 3. STORAGE
SQLite via node:sqlite, WAL, FTS5 — ONE file <projectRoot>/.codegraph/codegraph.db (+ daemon.pid/.sock,
ui/trails); home: ~/.codegraph/telemetry.json, ~/.codegraph/daemons/. Tables: nodes (id PK, kind, name,
qualified_name, file_path, language, lines/cols, docstring, signature, visibility, flags, decorators JSON,
return_type), edges (AUTOINCREMENT, source/target FK CASCADE, kind, metadata JSON, line, col, provenance), files
(path PK, content_hash, language, size, modified_at, indexed_at, node_count, errors, generated), unresolved_refs
(from_node FK CASCADE, name, kind, line, col, candidates JSON, status pending|failed, name_tail), nodes_fts
(external-content FTS5 + triggers), name_segment_vocab, project_metadata. Persisted: nodes, edges (resolution +
synthesized), files w/ sha256+size+mtime, unresolved refs. Recomputed at query time: type hierarchy, dead code,
branch guards, boundaries, flow, explore ranking; nothing loaded into memory on open (1,000-node LRU). Bulk index:
journal MEMORY, synchronous OFF, FTS triggers/indexes dropped; full re-index UNLINKS the db file. No snapshot
history — one mutable db.

## 4. CHANGE TRACKING (the key section)
Content-hash reconcile against `files`, driven by (a) fs.watch (FSEvents on macOS; inotify per-dir capped
50,000) with adaptive debounce (≤2 pending → 300 ms else 2 s) and a SCOPED FAST PATH when ≤500 exact events
(directory removal / codegraph.json / .gitignore edits / >500 events force a full scan-diff); (b) `codegraph
sync` full scan-diff with (size, mtime) prefilter then sha256 ("filesystem-based, never git"; blind spot:
same-size same-mtime edit); (c) git hooks post-commit/merge/checkout → `sync &` (WSL2). git status is used ONLY
for the status report. Watcher degrades permanently after 5 lock retries / 5 sync failures and says so.
UPDATE PATH (storeExtractionResult, src/extraction/index.ts:2367-2540): skip if hash unchanged → SNAPSHOT
incoming cross-file edges (target in this file, source elsewhere) with the target's (name, kind) — because ids
embed line numbers → deleteFile (cascade) → insert new nodes/edges/refs in one txn → RE-ATTACH snapshotted edges
by (kind\0name) → new id; vanished targets RESURRECTED as their original unresolved ref from metadata.refName
(pending); unstamped edges drop silently ("silent beats wrong") → resolve refs from changed files only → RETRY
previously-failed refs whose name_tail matches a newly defined name (≤500/name) → REBIND (CG-33): definitionDelta
= symmetric difference of file\0name pairs per file → delete resolution edges targeting those names from
UNCHANGED files and re-open them as pending refs (skip heuristic/unstamped, skip names with >500 edges) → orphan
sweep → conformance/this.member passes. RENAME: none (--no-renames); a move = delete + add; incoming edges come
back by name if unique, else tie-break (file_path, start_line). REVERSE-DEPENDENCY INDEX exists (idx_edges_
target_kind + getDependentFilePaths) but is used for REPORTING (affected, blast radius, dependents), NOT to
re-extract dependents — dependents are never re-parsed; only their edges into the changed file are re-attached.
Convergence MEASURED: docs/benchmarks/index-drift-cg33.md — 4.3% divergent edges on a long-lived synced index
before the fix; replay after: 16 commits → 0 residual, 80 commits → 361 residual (generic names above the
500-per-name ceiling, deliberately left); `status` deliberately reports NO drift metric ("an honest estimate is
not available"). "For every PR, know what to test" = labelled "coming" (marketing). What exists: `codegraph
affected` — BFS over whole-FILE dependents to depth 5, stopping at test files (regex), piped from `git diff
--name-only`. NO code maps diff hunks (line ranges) to symbols; "flows affected / business logic compromised"
have no implementation.

## 5. ALGORITHMS
Search: FTS5 prefix + bm25 weights (name 20, qualified 5, docstring 1, signature 2) → LIKE ladder → bounded
Levenshtein; re-score with kind/path/name bonuses; NO embeddings. Prose→symbol via name_segment_vocab (camelCase
segments; co-occurrence tier then rarity tier). Explore ("surgical context"): seeds (pinned paths ∪ multi-channel
relevance ∪ named-symbol seeds ∪ glue callers/callees) → Random-Walk-with-Restart over an undirected per-request
adjacency (α 0.25, 25 iterations) → per-file score tiers → order pinned → named → corroborated → RWR mass → term
hits. BUDGET IS CHARACTERS (no tokenizer): tiers by file count 13K/18K/24K chars, hard ceiling min(1.5×, 25,000)
under the host's inline cap; per-file floor 700, max share 70%, reservation invariant tested; render modes whole /
clusters / focused / skeleton; session dedup by content fingerprint with back-references. Flow: BFS over
calls+navigates, 7 hops / 1,500 visits (directed: bidirectional 12 hops). Impact: DFS over incoming edges excl.
contains, depth 2 (tool) / 3 (API), per definition of an ambiguous name. Cycles: Tarjan over file-dependency
pairs (min confidence 0.6), capped 40×12, VIEWER ONLY; modules = directory-prefix grouping. Every hop is SQL
(no in-memory graph); resolver holds LRU maps (5,000).

## 6. ACCESS PATTERNS
8 MCP tools, all readOnly; ONLY codegraph_explore listed by default (measured: description wording does not move
tool choice; collapse to one sufficient tool). explore(query) → verbatim line-numbered source grouped by file +
Flow + Blast radius + Relationships + dynamic boundaries, 13–24K chars; node(symbol|file) body 12,000 chars /
file 38,000; search/callers/callees/impact 15,000 chars; files; status. CLI: init, index, sync, status, query,
explore, context, node, files, callers, callees, impact, affected, daemon, ui, serve --mcp, … Expected conditions
(not indexed, not found) are SUCCESS-SHAPED text, never isError ("one or two isError responses early in a
session and the agent stops calling codegraph entirely"). Explore header: "verbatim, current on-disk source …
byte-for-byte identical to what the Read tool returns … Treat each block as a Read you have already performed".

## 7. MEASUREMENT & HONESTY
"Measured cross-file coverage" (README:903-932) = share of symbol-bearing files with ≥1 resolved cross-file
dependent, one OSS repo per language (leveldb 94.8%, ripgrep 86.7%, …) — UNDETERMINED how computed (no script
exists; asserted in prose). A/B agent benchmarks documented incl. a volunteered negative (80% more residual
context). Uncertainty in output: heuristic edges labelled inline ("dynamic: callback via …"); "Dynamic boundaries
(the static path ends at runtime dispatch)" with ≤4 candidates; confidence bucketed at 0.6 only in the boundary
report/viewer — NOT on explore's normal edges; per-file staleness banners; drift-aware node() refuses to slice a
shifted file; status warns on interrupted runs; MCP instructions self-declare "best-effort name matching".
Does NOT: per-query unresolved counts, per-edge confidence on ordinary output, a coverage figure for the queried
repo, a drift metric (by decision), refusal when coverage is low (explore always renders something; an
anti-abandonment guard restores content). README "Full support" labels exceed what the resolver does for
C#/Ruby/Swift/Dart/Scala (name-matching only).

## 8. WHAT RMAP COULD LEARN (cited)
1. Edge survival across re-extraction: snapshot incoming edges by (kind,name) before deleting a re-indexed file,
   re-attach, resurrect vanished targets as pending refs from the refName stamp, rebind name-keyed edges in
   UNCHANGED files by definitionDelta — with a REPLAY BENCHMARK proving convergence (index-drift-cg33.md).
   rmap: new snapshot + prune. Outward: "I edited foo.rs, is `deps bar` still right?" answered ~0.3 s after save.
2. Per-file staleness in every response (pendingFiles firstSeen/lastSeen/indexing; false-positive "stale"
   preferred). rmap: snapshot-level age only.
3. One dense verbatim payload with a Read-parity contract and a tiered CHARACTER budget (floor/ceiling/reservation
   invariant) + cross-call dedup with back-references; measured A/B feedback metrics.
4. Never isError for expected conditions — refusal SHAPE decides whether the agent keeps calling.
5. Dynamic boundaries as a first-class verdict (dispatch site, form, key, ≤4 candidates), computed once, rendered
   by both explore and the viewer.
6. Single derivation per fact shared across surfaces ("two derivations eventually disagree"; derivations live in
   graph/, never in tool handlers) — the structural form of COHERENCE-2/3.
7. Adapt to the agent: one default tool with sufficient output; validate on the floor model.
8. Kernel as optional accelerator with a wire contract + byte-parity gate methodology.
What codegraph lacks that rmap has: trust/honesty labels on ordinary edges, per-query unresolved accounting,
refusals under low coverage, repo-level coverage at query time, snapshot history, drift metric.

## Language support (from code)
Extension map: ts/tsx/arkts/js/jsx/python/go/rust/java/c/cpp(+metal,cu)/csharp/razor/php/yaml/twig/ruby/swift/
kotlin/dart/liquid/svelte/vue/astro/r/pascal/scala/lua/luau/objc/solidity/cfml/nix/xml/cobol/vbnet/erlang/
properties/terraform (+ Play routes, Shopify templates, Erlang .app.src). 31 wasm grammar keys; custom TS
extractors for svelte/vue/astro/liquid/razor/MyBatis/cfml/dfm; yaml/twig/properties file-level only. Kernel: 20
languages, all default-routed.
