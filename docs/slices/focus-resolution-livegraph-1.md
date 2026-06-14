# FOCUS-RESOLUTION-LIVEGRAPH-1: a LiveGraph-native focus-resolution producer (the PREREQ-1 enabler)

Slice ID: FOCUS-RESOLUTION-LIVEGRAPH-1
Status: **SPEC-FIRST — specification only. NO implementation, NO code, NO deletion, NO migration, NO default
flip, NO IR extension built.** This document specifies a LiveGraph-native producer that would resolve a FOCUS
STRING (path / stable-key / symbol-name / symbol-context) to an IR symbol/file/module identity from
CURRENT-STATE LiveGraph (IR + the FILE inventory) INSTEAD of the SQLite `nodes` reads the four `resolve_*`
functions perform today — with a no-loss cert proving the LiveGraph resolution equals the SQLite resolution.
This producer is the FEASIBLE second producer gap COHERENCE-LEAF-SERVE-1 §5 named; it is later consumed by the
COHERENCE-LEAF-SERVE impl (focused orient + every explain) to make their eager `nodes` read eliminable on
green (NOT this slice).
Track: Stage D / SQLite-raw decommission — Option B continuation, operator-ratified (DR-CLS-FOCUS → B,
COHERENCE-LEAF-SERVE-1 §10). "Producer program first, per-command fastpath second" — the same discipline the
trust-core arc used (TRUST-SUMMARY-LIVEGRAPH-1).
Baseline: COHERENCE-LEAF-SERVE-1 (`741670f`, §5 the focus-resolution gap + the two sub-gaps; §10 DR-CLS-FOCUS →
Option B); ORIENT-SQLITE-FREE-1 (`e10a455`, DR-4 — focus resolution No-LG-producer, BLOCKING for focused
orient); EXPLAIN-SQLITE-FREE-1 (`f3237f9`, DR-E2 — focus resolution No-LG-producer, BLOCKING, unconditional);
TRUST-SUMMARY-LIVEGRAPH-1 (`94fc506`) — the producer-feasibility-spec + crate-home-matrix precedent this
document mirrors.

> **HEADLINE FINDING — VERDICT: `BUILDABLE-FROM-EXISTING-IR` (a derivation, NOT a clean 1:1 projection, and NOT
> an IR extension). Read before the design.**
> Focus resolution is the OPPOSITE branch from TRUST-SUMMARY-LIVEGRAPH-1. There the IR's `IrEdge` is
> resolved-only and the unresolved-call fact is DROPPED at ingest — a SUBSTRATE gap (SCIP emits nothing;
> SCIP-UNRESOLVED-CALL-PROBE-1 NO-GO). Here, **first-hand IR reads CONFIRM every datum the four `resolve_*`
> functions return is already carried by the IR or is a deterministic function of data the IR carries.** The
> resolution KEYS — name (`IrNode.name`), file-by-path (FILE-scope node keys + the existing `FileInventory`),
> stable-key→node (the IR `CanonicalKey` is the SAME namespace as the SQLite `stable_key`) — are present
> directly. The TWO sub-gaps COHERENCE-LEAF-SERVE-1 §5c flagged are BOTH resolvable WITHOUT a new IR field:
> (1) **MODULE-node identity** is a PURE FUNCTION of the file-path set (the SQLite directory-MODULE materializer
> takes ONLY the file list, `orchestrator.rs:1005-1039`; key = `{repo}:{dir}:MODULE`, deterministic), so the
> LiveGraph reproduces it by the same ancestor-walk over its FILE inventory — a DERIVED module-identity model,
> not a missing fact; (2) **`qualified_name`** is EMBEDDED in the `CanonicalKey` — `make_stable_key` builds the
> key FROM `qualified_name` (`extractor.rs:351-353`, `:725`), so it is RECOVERABLE by parsing, not missing.
> Neither sub-gap is a substrate gap; neither requires an IR field. The producer is a NEW LiveGraph read
> surface + a NEW data shape (focus string → identity) crossing the LiveGraph API boundary, gated by a no-loss
> cert against the SQLite resolution, with honest TS-only scope + SQLite fallback. The CRATE HOME and the
> `qualified_name` sourcing are surfaced as `DECISION_REQUIRED` (§11). The STOP_CONDITION ("a sub-gap needs an
> IR field that is NOT a feasible additive extension") is **NOT triggered** — no sub-gap needs an IR field at
> all.

> **This CONFIRMS (does not contradict) COHERENCE-LEAF-SERVE-1 §5c.** §5c classified the gap "FEASIBLE-BUT-
> UNBUILT" with "TWO sub-gaps [that] are NOT clean," calling for "a derived MODULE-node identity model" and
> noting `qualified_name` "is a DISPLAY field, not a resolution KEY." This slice does that sizing first-hand and
> finds BOTH sub-gaps land on the feasible side: the derived module model is a deterministic reproduction of the
> SQLite materializer, and `qualified_name` is recoverable from the key (or, optionally, a clean additive IR
> field). The "NOT clean" caution survives as the no-loss cert's RED-by-construction risks (§7c), not as an
> impossibility.

> **DECISION RESOLUTION (ratified by operator, 2026-06-14):**
> - **DR-FR-CRATE-HOME → Option A (EXTEND `repo-graph-livegraph`).** The resolver (resolve_path / resolve_stable_key
>   / resolve_symbol_name / symbol_context) lives in `repo-graph-livegraph` beside module_stats/callers/node_display
>   (NO new dependency edge); the cert build/store wiring in the daemon `livegraph_feed.rs`; the native-result →
>   agent-DTO mapping in the later COHERENCE-LEAF-SERVE consumer adapter. New-crate / coherence / daemon-computation
>   were rejected.
> - **DR-FR-QNAME-SOURCE → Option A (PARSE-FROM-KEY).** `qualified_name` is parsed from the `#…:SYMBOL:` segment of
>   the `CanonicalKey` (single source of truth, no IR change); the cert guards the fallback-node edge case. The
>   additive IR field (B) was NOT taken.
> **NEXT BUILD: FOCUS-RESOLUTION-LIVEGRAPH-IMPL** — the resolver + no-loss cert (resolver in `repo-graph-livegraph`,
> cert build/store in the daemon). Standalone; the COHERENCE-LEAF-SERVE consumer wiring is a later slice.

---

## 0. Spec-first note (read first)

This is a SPECIFICATION. It produces exactly one deliverable: this file. NO source path is touched; no IR field
is added; no resolver surface is built; no cert is built; no default is flipped; `nodes` is not read by this
slice (only the SHIPPED `resolve_*` core reads it, which this doc audits). The eventual implementation
(FOCUS-RESOLUTION-LIVEGRAPH-IMPL-1, a LATER slice) is gated on the §11 decisions being ratified first. The
COHERENCE-LEAF-SERVE impl (focused orient + explain consumption) is a SEPARATE later slice that depends on this
producer + its cert (out of scope here — §12).

Per the repo split rule (CLAUDE.md: spec before impl; ratify architecture-boundary decisions before building),
this slice's DEFINITION OF DONE is the specification + the surfaced decisions + the explicit
`BUILDABLE-FROM-EXISTING-IR`-vs-`NEEDS-IR-EXTENSION` verdict — NOT a working producer.

### Evidence labels (repo Evidence Law; agent_docs/validation.md)

`OBSERVED` = artifact/source inspected directly, file:line cited. `INFERRED` = concluded from OBSERVED facts.
`EXECUTED` = command run, output seen. `NOT RUN` = skipped, reason stated. Every claim below carries a label.

### Evidence basis (this audit) — OBSERVED, first-hand, this session

- `rust/crates/storage/src/agent_impl.rs` — the four `resolve_*` impls: `resolve_path_focus:366`,
  `resolve_stable_key_focus:437`, `resolve_symbol_name:800`, `get_symbol_context:834` (the consumed contract).
- `rust/crates/agent/src/storage_port.rs` — the OUTPUT DTOs (`AgentFocusKind:336`, `AgentFocusCandidate:349`,
  `AgentPathResolution:368`, `AgentSymbolContext:387`) + the `StoragePort` method signatures (`:516`-`:610`).
- `rust/crates/agent/src/orient/mod.rs:100-219` + `rust/crates/agent/src/explain/mod.rs:86-165` — the CONSUMER
  resolution cascade (path → stable-key → symbol-name) the producer must reproduce.
- `rust/crates/repo-graph-ir/src/lib.rs` — the IR shape: `IrNode:338-360` (`key`/`name`/`subtype`/`range`,
  NO `qualified_name`), `SourceRange:274-286` (`range.file`), `IdentitySource:52-66` (`AstAdopted` /
  `ScipSynthesizedFallback` / `AstFileScope`), `CanonicalKey:27-46`.
- `rust/crates/repo-graph-livegraph/src/lib.rs` — the LiveGraph surface: `callers:469`/`callees:586`/
  `value_facts:688`/`path:842` (all key-string keyed), `node_location:1031`, `node_display:1051`,
  `module_stats:1376` (`dirname(&range.file):1413`), `rebuild_xpart_overlay:1077` (the `FileInventory`
  path→FILE-key map, `:1077-1088`).
- `rust/crates/ts-extractor/src/extractor.rs` — the key/qualified_name producer: `make_stable_key:345-364`
  (`{repo}:{file}#{name}:SYMBOL:{subtype}`, `:351-353`), top-level `qualified_name: Some(name):429`, method
  `qualified_name = format!("{}.{}", parent, name):707` + `make_stable_key(&qualified_name, …):725`, FILE
  `:FILE` key `:160`.
- `rust/crates/indexer/src/orchestrator.rs` — directory-MODULE + OWNS: dir-MODULE materializer `:1005-1039`
  (`stable_key = {repo}:{dir}:MODULE:1024`, `qualified_name = dir:1028`), `file_to_module` population `:830-840`
  (`module_key = {repo}:{mod_path}:MODULE:834`), OWNS edge MODULE→FILE `create_module_edges:1043-1069`.
- `rust/crates/indexer/src/resolver.rs:671-678` — `get_module_path` = dirname (immediate parent).
- COHERENCE-LEAF-SERVE-1 §5 (`docs/slices/coherence-leaf-serve-1.md:221-310`), §10 DR-CLS-FOCUS (`:499-554`);
  ORIENT-SQLITE-FREE-1 DR-4 (`docs/slices/orient-sqlite-free-1.md:202`); EXPLAIN-SQLITE-FREE-1 DR-E2
  (`docs/slices/explain-sqlite-free-1.md:276`); TRUST-SUMMARY-LIVEGRAPH-1 §10 DR-TS-CRATE-HOME
  (`docs/slices/trust-summary-livegraph-1.md:720-763`).
- Daemon: NOT RUN — the local daemon socket refused connection this session (`os error 61`); per the packet
  ("else first-hand reads, every OBSERVED claim file:line'd") all evidence here is first-hand source reads.

---

## 1. Why now (priority path) + the VISION tie

[OBSERVED: ROADMAP §"Current Priority"; CURRENT_SLICE banner; COHERENCE-LEAF-SERVE-1 §10 DR-CLS-FOCUS → B.]

PREREQ-1 of the ratified bounded-decommission contract (`docs/slices/sqlite-raw-decommission-1.md`) is "the (b)
leaves served." COHERENCE-LEAF-SERVE-1 closed PREREQ-1 for orient REPO-focus only and proved the rest
(focused orient + EVERY explain) is GATED on a focus-resolution producer that does not exist: the four
`resolve_*` functions read `nodes` UNCONDITIONALLY and FIRST, BEFORE any leaf or cert precondition can run
(§5c; orient DR-4; explain DR-E2). Operator ratified DR-CLS-FOCUS → Option B: build the producer first. This
slice specs it. Its impl, then the COHERENCE-LEAF-SERVE consumption impl, fully close PREREQ-1.

**VISION tie — Orientation over Perfection + the Fact-Certainty Model.** [OBSERVED: VISION §"Orientation, Not
Oracle"; CLAUDE.md §"Fact Certainty Model".] Focus resolution is a Layer-0/1 EXTRACTED-FACT lookup (file/symbol/
module identity). Moving it from the outgoing SQLite `nodes` substrate to the current-state LiveGraph (the
in-memory primary truth) WITHOUT touching precision is exactly the substrate migration the Stage-D arc pursues:
every resolution served from the LiveGraph is gated by a no-loss cert (the resolution is byte/field-equal to the
SQLite resolution) + a labelled SQLite fallback. The fact-certainty class is PRESERVED (deterministic extracted
fact → deterministic extracted fact); the cert is what keeps a Layer-0 lookup from being silently downgraded.
Persistence-completeness (CLAUDE.md): the served-vs-fallback decision is visible (the `backend_used` label the
drilldowns already carry), the cert is keyed by the shared SQLite-free fingerprint, and the SQLite read survives
ONLY to build the cert + on fallback.

---

## 2. The CONSUMED resolution contract — the producer's OUTPUT contract (OBSERVED, first-hand, file:line)

The producer's job is to return, for the same focus string, the SAME identity the four SQLite `resolve_*`
functions return today. Enumerated first-hand. Each names its INPUTS, its `nodes` READS, and its OUTPUT.

### 2a. `resolve_path_focus(snapshot_uid, path) -> AgentPathResolution` — `agent_impl.rs:366`

INPUT: a repo-relative `path`. READS `nodes` (JOIN `files`) FOUR times [OBSERVED `:376-427`]:
- `has_exact_file` — `COUNT(*) … n.kind='FILE' AND f.path = path` (`:376-386`).
- `has_content_under_prefix` — `COUNT(*) … n.kind='FILE' AND f.path LIKE 'path/%'` (`:391-401`).
- `file_stable_key` — `SELECT n.stable_key … n.kind='FILE' AND f.path = path` (`:404-416`).
- `module_stable_key` — `SELECT stable_key … kind='MODULE' AND qualified_name = path` (`:419-427`). **← sub-gap 1.**

OUTPUT `AgentPathResolution { has_exact_file: bool, file_stable_key: Option<String>, has_content_under_prefix:
bool, module_stable_key: Option<String> }` [OBSERVED `storage_port.rs:368-376`]. Doc note (`:355-366`): the
dispatcher checks `has_exact_file` → file pipeline, else `has_content_under_prefix || module_stable_key` →
path-area pipeline. `module_stable_key` is `Some` only when a MODULE node's `qualified_name` matches the path
exactly.

### 2b. `resolve_stable_key_focus(snapshot_uid, stable_key) -> Option<AgentFocusCandidate>` — `agent_impl.rs:437`

INPUT: a `stable_key`. READS `nodes` (LEFT JOIN `files`) once: `SELECT n.stable_key, n.kind, f.path … WHERE
n.stable_key = ?` (`:442-454`). The `kind` string maps to `AgentFocusKind`: `"FILE"→File`, `"MODULE"→Module`,
else `Symbol` (`:458-462`). OUTPUT `Option<AgentFocusCandidate { stable_key, kind: {File|Module|Symbol}, file:
Option<String> }>` [OBSERVED `storage_port.rs:336-353`]; `None` when no row.

### 2c. `resolve_symbol_name(snapshot_uid, name) -> Vec<AgentFocusCandidate>` — `agent_impl.rs:800`

INPUT: a `name`. READS `nodes` (LEFT JOIN `files`): `SELECT n.stable_key, n.kind, f.path … WHERE n.kind='SYMBOL'
AND n.name = ? ORDER BY n.stable_key ASC LIMIT 5` (`:806-814`). OUTPUT `Vec<AgentFocusCandidate>` (≤5, all
`kind=Symbol`). **NOTE [OBSERVED, load-bearing]: the SQLite resolver matches on `name` ONLY and does NOT read or
disambiguate by `qualified_name`** — same-name ambiguity is surfaced as up-to-5 candidates, each carrying its
`stable_key` (which encodes `file#qualified_name`). So sub-gap 2 (`qualified_name`) does NOT bite name
resolution; it bites only the symbol-context payload (§2d).

### 2d. `get_symbol_context(snapshot_uid, symbol_stable_key) -> Option<AgentSymbolContext>` — `agent_impl.rs:834`

INPUT: a symbol `stable_key`. READS `nodes` (the symbol) LEFT JOIN `files`, LEFT JOIN `nodes file_node`
(`kind='FILE'`, same `file_uid`), LEFT JOIN `edges own` (`type='OWNS'`, `target=file_node`), LEFT JOIN `nodes
mod_n` (the owning MODULE) (`:839-854`). OUTPUT `AgentSymbolContext { file_path, module_path, module_stable_key,
name, qualified_name, subtype, line_start }` [OBSERVED `storage_port.rs:387-395`]. Doc note (`:380-385`): the
owning module is "the SINGLE source of truth for which module this symbol belongs to" — downstream boundary/
gate/cycle code reads it, so `module_path`/`module_stable_key` are load-bearing. Fields needing scrutiny:
`qualified_name` (**sub-gap 2**), `module_path` + `module_stable_key` (**sub-gap 1**, via the OWNS edge).

### 2e. The consumer cascade the producer must reproduce — `orient/mod.rs:100-219`, `explain/mod.rs:86-165`

[OBSERVED first-hand.] Both consumers resolve a focus string in this PRECEDENCE order:

```text
1. resolve_path_focus(focus):
     has_exact_file               -> FILE pipeline   (uses file_stable_key)
     has_content_under_prefix
       || module_stable_key.is_some() -> PATH-area pipeline (uses module_stable_key)
2. else resolve_stable_key_focus(focus):
     Some(Symbol) -> get_symbol_context -> SYMBOL pipeline
     Some(File)   -> FILE pipeline
     Some(Module) -> PATH pipeline (path extracted from candidate/key)
     None         -> step 3
3. resolve_symbol_name(focus):
     0 candidates -> no_match
     1 candidate  -> get_symbol_context -> SYMBOL pipeline
     >1           -> ambiguous (candidate list)
```

The producer must reproduce this cascade and its branch outputs EXACTLY (the no-loss cert, §7, gates on it).
`explain/mod.rs` requires a target (no repo/None focus, `explain/mod.rs` target REQUIRED) → its `nodes` read is
UNCONDITIONAL; orient REPO-focus (`focus=None`) skips the cascade entirely (the §1 asymmetry).

---

## 3. THE LOAD-BEARING QUESTION — is focus resolution servable from the current-state LiveGraph?

### 3a. The question (the packet's load-bearing question, verified first-hand — not assumed)

```text
Is the producer BUILDABLE FROM THE EXISTING IR, or do the two sub-gaps (MODULE-node identity, qualified_name)
need an IR EXTENSION — and if so, is the extension a feasible additive one (the data exists in the producer,
just not carried in the IR) or a substrate gap (the data exists nowhere current-state, like the unresolved-call
data SCIP drops)?
```

### 3b. First-hand evidence (OBSERVED, first-hand)

```text
[E1] The LiveGraph is keyed EXCLUSIVELY by CanonicalKey; it has NO focus-string resolver surface today.
     [OBSERVED: repo-graph-livegraph/src/lib.rs.] callers(target:&str):469 / callees:586 / value_facts:688 /
     path:842 match `target` against key-indexed maps (the string must ALREADY be a CanonicalKey);
     node(&CanonicalKey) (ir/lib.rs:407), node_location(&CanonicalKey):1031, node_display(&CanonicalKey):1051
     all take a key. There is NO name->key, NO path->node, NO stable_key->node PUBLIC surface. (Confirms
     COHERENCE-LEAF-SERVE-1 §5b E1.) => a NEW resolver surface must be BUILT.

[E2] The resolution KEYS are all present in the IR.
     - name: IrNode.name (ir/lib.rs:347) -- direct source for resolve_symbol_name's `name` match.
     - file-by-path: every FILE-scope node (IdentitySource::AstFileScope, ir/lib.rs:60-66) carries its path in
       the CanonicalKey ({repo}:{file}:FILE, extractor.rs:160). The LiveGraph ALREADY builds a path->FILE-key
       map: FileInventory::from_file_keys over AstFileScope node keys (lib.rs:1077-1088, in rebuild_xpart_
       overlay). So file-by-path + content-under-prefix are derivable over an EXISTING structure.
     - stable_key->node: the IR CanonicalKey IS the SAME namespace as the SQLite stable_key. [OBSERVED:
       extractor.rs:160 {repo}:{file}:FILE, :351-353 {repo}:{file}#{name}:SYMBOL:{subtype}; ir/lib.rs:17-20
       "value-level reuse of the existing ts-extractor symbol stable-key string."] A user/prior-resolved key is
       a valid LiveGraph lookup key (LIVEGRAPH-INTEGRATION-1B proved SCIP keys byte-equal to SQLite).

[E3] qualified_name is EMBEDDED in the CanonicalKey -- the IR does NOT need a new field. [OBSERVED, decisive.]
     make_stable_key(name, subtype) = format!("{}:{}#{}:SYMBOL:{}", repo, file, name, subtype) (extractor.rs:
     351-353). For a method the `name` arg passed is qualified_name (`Parent.method`, extractor.rs:707 ->
     make_stable_key(&qualified_name,…):725); for a top-level symbol qualified_name == name (extractor.rs:429).
     So in BOTH cases the segment between `#` and `:SYMBOL:` in the key IS qualified_name. => qualified_name is
     RECOVERABLE by parsing the key. (ir/lib.rs has NO `qualified_name` field -- confirmed; but it does not need
     one.)

[E4] MODULE-node identity is a PURE FUNCTION of the file-path set -- derivable, not a missing fact. [OBSERVED.]
     The SQLite directory-MODULE materializer takes ONLY `files` as input (orchestrator.rs:1005 `for f in
     files`), walks EVERY ancestor directory of every file (:1006-1015), and emits one MODULE node per dir:
     stable_key = format!("{}:{}:MODULE", repo, dir) (:1024), qualified_name = dir (:1028), subtype=Directory
     (:1026). The `nodes` kind='MODULE' set resolve_path_focus matches IS exactly this directory set (declared
     modules live in the separate module_candidates table -- "additive, coexists," VISION §Current state). The
     IR carries every FILE path; the LiveGraph reproduces the dir-MODULE set by the SAME ancestor-walk and
     synthesizes the SAME deterministic key. The repo prefix is parseable from any FILE key.

[E5] symbol->module attribution is dirname(file) -- derivable. [OBSERVED.]
     file_to_module maps a FILE's path via get_module_path(path) (orchestrator.rs:832-834); get_module_path =
     file_path[..last_slash] = the immediate-parent dir (resolver.rs:671-678). The OWNS edge links that
     immediate-parent MODULE to the FILE (create_module_edges:1053-1069). So get_symbol_context's module_path =
     dirname(symbol's file) and module_stable_key = {repo}:{dirname}:MODULE -- EXACTLY the derivation
     module_stats already runs (dirname(&range.file), lib.rs:1413). No OWNS edge needed in the IR.
```

### 3c. The contrast that fixes the verdict (INFERRED over §3b OBSERVED)

```text
TRUST-SUMMARY-LIVEGRAPH-1 was NEEDS-EXTENSION because the unresolved-call disposition is DROPPED at ingest
(IrEdge is resolved-only, ir/lib.rs:364-378) and SCIP emits no unresolved-call occurrence (probe NO-GO) -- the
fact exists NOWHERE current-state. FOCUS RESOLUTION is the opposite: every datum is present (E2/E3) or a
deterministic function of present data (E4/E5). There is NO dropped fact, NO substrate gap. The work is a NEW
READ SURFACE over existing IR data + two DERIVATIONS (the dir-MODULE model; the qualified_name key-parse) --
not an IR extension. => BUILDABLE-FROM-EXISTING-IR.
```

---

## 4. Per-resolution-kind feasibility (the four functions, each with its LiveGraph source) — INFERRED over §3 OBSERVED

| Function | Output field | LiveGraph/IR source | Sub-gap | Feasibility |
|----------|--------------|---------------------|---------|-------------|
| `resolve_path_focus` | `has_exact_file` | FILE-scope key exists for `path` (FileInventory, lib.rs:1077) | — | **BUILDABLE** |
| | `file_stable_key` | that FILE-scope `CanonicalKey` ({repo}:{path}:FILE) | — | **BUILDABLE** |
| | `has_content_under_prefix` | any FILE-scope path starts with `path/` | — | **BUILDABLE** |
| | `module_stable_key` | `Some({repo}:{path}:MODULE)` iff `path` is an ancestor dir of some resident FILE (E4 walk) | **1** | **BUILDABLE (derived model)** |
| `resolve_stable_key_focus` | match + `kind` File | FILE-scope node (`AstFileScope`) with key == focus | — | **BUILDABLE** |
| | match + `kind` Symbol | SYMBOL node (`AstAdopted`) with key == focus | — | **BUILDABLE** |
| | match + `kind` Module | focus key parses as `…:MODULE` AND names an ancestor dir of a resident FILE (E4) | **1** | **BUILDABLE (derived model)** |
| | `file` | the matched node's `range.file` (or the key's file segment) | — | **BUILDABLE** |
| `resolve_symbol_name` | ≤5 candidates by `name` | `ir.nodes.filter(n.name == focus && AstAdopted)`, sorted by key, take 5 | — | **BUILDABLE** (matches on name; §2c) |
| | candidate `stable_key`/`file` | `n.key` / `n.range.file` | — | **BUILDABLE** |
| `get_symbol_context` | `name` / `subtype` / `line_start` / `file_path` | `IrNode.name` / `subtype` / `range.start_line` / `range.file` | — | **BUILDABLE** |
| | `qualified_name` | parse `#{qualified_name}:SYMBOL:` from `CanonicalKey` (E3) | **2** | **BUILDABLE (key-parse)** |
| | `module_path` | `dirname(range.file)` (E5) | **1** | **BUILDABLE (derived model)** |
| | `module_stable_key` | `{repo}:{dirname(range.file)}:MODULE` (E5) | **1** | **BUILDABLE (derived model)** |

Every cell is BUILDABLE. The only non-trivial cells are the two sub-gaps (§5).

---

## 5. The two sub-gaps, resolved (the §5c "NOT clean" pair) — INFERRED over §3b/§4

### 5a. Sub-gap 1 — MODULE-node identity (resolve_path_focus.module_stable_key; get_symbol_context module fields)

```text
THE GAP (E4 / COHERENCE-LEAF-SERVE-1 §5c i): the IR has NO MODULE node. AstFileScope is "source-file scope, not
a module-architecture entity" (ir/lib.rs:60-66). resolve_path_focus returns a real MODULE node's stable_key.

THE RESOLUTION (no IR field): the SQLite kind='MODULE' set is the DIRECTORY-module set, and that set is a PURE
DETERMINISTIC FUNCTION of the file-path inventory (orchestrator.rs:1005-1039, input = `files` only). The
producer derives a "DerivedModuleIdentity" model over the LiveGraph FILE inventory:
  - module exists at `dir` iff some resident FILE path has `dir` as an ancestor directory (same ancestor-walk as
    orchestrator.rs:1006-1015).
  - module_stable_key(dir) = format!("{}:{}:MODULE", repo, dir)  -- byte-identical to the SQLite key (:1024).
  - the symbol->module map = dirname(symbol's file) (E5), matching the OWNS-edge derivation 1:1.
This is exactly the "derived MODULE-node identity model" §5c called for. It needs NO new IR/extraction; it is a
read + a walk over data already resident.

PARITY RISK (the cert encodes it, §7c): no-loss holds ONLY if the LiveGraph's resident FILE-scope set == the
SQLite FILE-node set for the partition. The stats slice already proved module_stats file_count no-loss vs SQLite
(COHERENCE-LEAF-SERVE-1 §3 row 5, stats cert [EXISTS]) -- strong evidence the FILE inventory is complete for
resident TS partitions. The cert RE-PROVES it for the resolution.
```

### 5b. Sub-gap 2 — `qualified_name` (get_symbol_context.qualified_name)

```text
THE GAP (E3 / COHERENCE-LEAF-SERVE-1 §5c ii): the IR has no `qualified_name` field. It is a DISPLAY/payload
field (NOT a resolution key -- §2c), biting only the get_symbol_context payload.

THE RESOLUTION (no IR field, RECOMMENDED): qualified_name is EMBEDDED in the CanonicalKey. make_stable_key
builds the key FROM qualified_name (extractor.rs:351-353 with the `name` arg = qualified_name for methods, :725;
== name for top-level, :429). Parse it back: qualified_name = the substring of the key between the first `#`
after the `:FILE-path:` segment and the `:SYMBOL:` marker. Robust for AST-adopted nodes (the TS path that
dominates).

OPTIONAL CLEANER ALTERNATIVE (a FEASIBLE additive IR field, surfaced as DR-FR-QNAME-SOURCE §11): qualified_name
is a real producer fact -- the ts-extractor computes it (extractor.rs:707/784/961) and the SCIP-ingest could
carry it onto IrNode as `qualified_name: Option<String>` (a small additive field, mirroring how
IR-SYMBOL-ATTRIBUTES-1 added `SymbolAttributes`). This is NOT required (the key-parse works) and DOES touch the
IR data shape + ingest + warm-cache, so it is surfaced as a decision, NOT taken here. RECOMMENDATION: parse from
key (no boundary change).

EDGE CASE (the cert encodes it): ScipSynthesizedFallback keys are NOT built by make_stable_key, so the parse may
not hold for them. Those are counted/surfaced fallback nodes (ir/lib.rs:421-428); the no-loss cert catches any
divergence and forces fallback. For TS, AST-adopted dominates.
```

### 5c. STOP_CONDITION check (explicit)

```text
STOP_CONDITION-1: "If a sub-gap needs an IR field that is NOT a feasible additive extension (a substrate gap),
STOP and emit DECISION_REQUIRED." => NOT TRIGGERED. Neither sub-gap needs an IR field at all (both derivable
from existing IR -- §5a/§5b). The only IR-field OPTION (qualified_name as an additive field) is FEASIBLE and
ADDITIVE (the producer has the fact), and is the non-recommended alternative -- not a substrate gap. The
contrast with the trust producer is total: there the missing fact had no source anywhere current-state; here
there is no missing fact.
```

---

## 6. The LiveGraph-native resolver DESIGN (for the IMPL; conditional on §11) — INFERRED

### 6a. The surfaces — four LiveGraph methods mirroring the four SQLite functions 1:1

```text
FocusResolver (a NEW read surface on LiveGraph; crate home = DR-FR-CRATE-HOME §11). Each method is a pure read
over resident IR, returning a LiveGraph-NATIVE result type wrapped in AnswerEnvelope<T> (the trust vocabulary
callers/callees/module_stats already use), so the answer carries completeness/freshness/contributing-languages.
The methods mirror the SQLite cascade (§2e) exactly:

  resolve_path(&self, path)        -> AnswerEnvelope<PathResolutionAnswer>
       PathResolutionAnswer { has_exact_file, file_key: Option<CanonicalKey>, has_content_under_prefix,
                              module_key: Option<String> }
       - has_exact_file / file_key: FileInventory lookup of `path` (lib.rs:1077 pattern).
       - has_content_under_prefix: any resident FILE path starts with `{path}/`.
       - module_key: DerivedModuleIdentity (§5a) -- Some({repo}:{path}:MODULE) iff `path` is an ancestor dir of
         a resident FILE.

  resolve_stable_key(&self, key)   -> AnswerEnvelope<Option<FocusCandidate>>
       FocusCandidate { key: CanonicalKey, kind: {File|Module|Symbol}, file: Option<String> }
       - File: AstFileScope node with key == focus. Symbol: AstAdopted node with key == focus.
       - Module: focus parses as `{repo}:{dir}:MODULE` AND `dir` is an ancestor of a resident FILE (§5a).

  resolve_symbol_name(&self, name) -> AnswerEnvelope<Vec<FocusCandidate>>
       - ir.nodes filtered by (name == focus && AstAdopted), sorted by key.as_str(), take 5 (mirrors the SQLite
         ORDER BY stable_key ASC LIMIT 5, agent_impl.rs:812-813). All kind=Symbol.

  symbol_context(&self, key)       -> AnswerEnvelope<Option<SymbolContext>>
       SymbolContext { file_path, module_path, module_key, name, qualified_name, subtype, line_start }
       - name/subtype/line_start/file_path: IrNode fields (§4).
       - qualified_name: key-parse (§5b). module_path/module_key: dirname(file) derivation (§5a/E5).

DEPENDENCY-DIRECTION NOTE (architecture, load-bearing): the LiveGraph result types are LiveGraph-native (NOT the
agent crate's AgentPathResolution/AgentFocusCandidate/AgentSymbolContext). repo-graph-livegraph must NOT depend
on the agent crate (that would invert the dep direction). The agent-port DTOs are produced by a MAPPING in the
CONSUMER/daemon adapter -- the SAME boundary where callers/callees' CallersAnswer is mapped to the agent DTO
today. This mapping is part of the COHERENCE-LEAF-SERVE consumption impl (a later slice, §12), not this producer.
```

### 6b. Ambiguity + miss handling (parity with SQLite)

```text
- Same-name ambiguity: resolve_symbol_name returns the ≤5-candidate vector exactly as SQLite (§2c). Downstream
  the consumer renders the candidate list (orient/mod.rs >1 branch). The producer does NOT disambiguate by
  qualified_name -- neither does SQLite. PARITY by construction.
- Miss (no match): resolve_path returns all-false/None; resolve_stable_key/symbol_context return None;
  resolve_symbol_name returns empty. The cascade then yields no_match -- byte-identical to SQLite.
- Safety rule (LiveGraph vocabulary, OBSERVED CURRENT_SLICE: "null=unknown, never empty"): when a partition is
  non-resident / non-Fresh / non-TS, the AnswerEnvelope is Partial/Unavailable and the consumer MUST fall back
  to the SQLite resolver -- a non-resident target is UNKNOWN, never resolved-as-miss. The no-loss cert (green)
  is the precondition that lets the consumer trust the LiveGraph resolution as exhaustive.
```

---

## 7. The no-loss cert (LiveGraph resolution == SQLite resolution) — INFERRED, mirroring the drilldown certs

### 7a. Why a cert, and what the COHERENCE-LEAF-SERVE fastpath AND-folds

```text
The COHERENCE-LEAF-SERVE bounded composite cert (COHERENCE-LEAF-SERVE-1 §6) AND-folds ONLY (b)-leaf no-loss
certs. Focused orient + explain resolve their focus BEFORE any leaf runs (§2e; orient DR-4 / explain DR-E2), so
the FOCUS-RESOLUTION no-loss cert is a NEW contributor the bounded composite cert must AND-fold AHEAD of the
leaf certs: the consumer may serve a LiveGraph-resolved identity (and skip the eager `nodes` read) ONLY when the
focus-resolution cert is GREEN at the current fingerprint. RED -> SQLite resolution (the existing path).
```

### 7b. The cert shape (mirror import_cert / cycles_cert / stats_cert)

```text
FOCUS-RESOLUTION no-loss cert {verdict: GREEN | RED, fingerprint} -- on RepoState, in-memory
RwLock<Option<...>>, S1 (rebuilt on restart), mirroring the drilldown certs [OBSERVED (doc): COHERENCE-LEAF-
SERVE-1 §6; stats-livegraph-1 cert-fastpath pattern]. Keyed by the SHARED SQLite-free fingerprint
`certificate_inputs_fingerprint` (partition {epoch/fresh/ts/hash/producer} (+) snapshot_uid (+) policy version)
-- NO new invalidation key. Lazily built once per fingerprint; the SQLite read survives ONLY (i) to BUILD the
cert and (ii) on fallback (the drilldown invariant). Cert BUILD/STORE plumbing lives in the daemon
livegraph_feed.rs beside build_and_store_{import,cycles,stats}_cert (the cert is daemon wiring; the COMPUTATION
is the producer -- DR-FR-CRATE-HOME).

GREEN iff, for a CORPUS of focus strings drawn from the resident snapshot (see §7d), the LiveGraph resolution
result is FIELD-EQUAL to the SQLite resolution result for EVERY focus, across all four functions AND the cascade
outcome (§2e). This is the FULL no-loss cert the slice GOAL names ("LiveGraph resolution equals the SQLite
resolution, same identity for the same focus string").
```

### 7c. The STRUCTURAL limits the cert must encode (honesty) — INFERRED, load-bearing

```text
[L1] FILE-inventory completeness (sub-gap 1): if the resident AstFileScope set != the SQLite FILE-node set, the
     derived module set + path resolution diverge. The cert's path/module comparisons RE-PROVE the stats-cert
     completeness result for resolution; a mismatch -> RED -> fallback. (Not assumed from the stats cert; re-
     checked here.)
[L2] Fallback-node keys (sub-gap 2): ScipSynthesizedFallback keys are not make_stable_key-shaped, so the
     qualified_name parse may diverge from SQLite. The cert's symbol_context comparison catches it -> RED.
[L3] MODULE model = DIRECTORY modules only. resolve_path_focus matches kind='MODULE' (directory modules,
     orchestrator.rs:1024). Declared-module candidates (module_candidates table) are NOT kind='MODULE' nodes and
     are NOT in scope -- parity holds because the SQLite query also only sees directory MODULE nodes. The cert
     compares against the SAME directory-MODULE set.
[L4] Non-TS partitions: the LiveGraph is TS-only (§8). A non-TS focus -> Partial -> fallback; the cert never
     claims green for a non-TS partition (the completeness envelope guards it).
```

### 7d. The corpus (what "for a corpus of focus strings" means)

```text
The cert cannot enumerate the infinite focus-string space; it proves parity over a FINITE corpus derived from
the resident snapshot, exhaustive over the resolvable identity set: every resident FILE path (exact + a prefix
sample), every directory in the ancestor-walk (module focuses), every resident SYMBOL stable_key + name, plus a
negative sample (known-miss strings). This makes the cert a FULL identity-set parity proof (every resolvable
identity is checked), not a spot check. Any focus that resolves to a resident identity is covered; a focus that
SQLite resolves via a non-resident node forces RED via L1.
```

---

## 8. Honesty — TS-only; what it DOES and does NOT achieve (per readiness-9 discipline) — INFERRED

```text
DOES: a green-path focus resolution with NO `nodes` read for resident + Fresh + TS partitions -- the producer +
cert that UNBLOCK the COHERENCE-LEAF-SERVE consumption (focused orient + explain) to make their eager `nodes`
read ELIMINABLE on green. This is the FEASIBLE second producer that DR-CLS-FOCUS → B ratified; combined with the
COHERENCE-LEAF-SERVE impl it closes PREREQ-1 for focused orient + explain (the §1 asymmetry's gated half).

DOES NOT:
- It does NOT itself eliminate any read. This slice is the PRODUCER + cert; the eager-read elimination is the
  COHERENCE-LEAF-SERVE consumption impl (§12). On its own this changes nothing the consumer sees.
- TS-ONLY (the LiveGraph is TS-only). Non-TS focused orient/explain STILL fall back to SQLite resolution -- the
  honest degradation. This does NOT regress them; it adds a green TS fastpath beside them.
- It does NOT flip any deletion gate by itself. PREREQ-1 closure (this + consumption) makes the
  bounded-decommission contract mechanically-ready for the coherence subset; it does NOT drop nodes/edges
  (PREREQ-2 + the retirement impl, out of scope).
- It does NOT touch the (c) trust unresolved-call boundary (FIXED, Option A) -- orthogonal.
```

---

## 9. VERDICT (stated explicitly, evidence-first)

```text
VERDICT: BUILDABLE-FROM-EXISTING-IR.

The producer is a NEW LiveGraph READ SURFACE over EXISTING resident IR data + TWO DERIVATIONS, gated by a
no-loss cert -- NOT an IR extension, NOT a substrate gap.
  - Resolution keys present directly: name (IrNode.name), file-by-path (FileInventory over AstFileScope keys),
    stable_key->node (shared CanonicalKey namespace). [E2, OBSERVED]
  - Sub-gap 1 (MODULE-node identity): BUILDABLE via a DERIVED model -- the directory-MODULE set is a pure
    deterministic function of the file-path inventory the IR already carries (orchestrator.rs:1005-1039 input =
    files only); key {repo}:{dir}:MODULE is reproducible byte-exact; symbol->module = dirname(file). [E4/E5,
    OBSERVED] No IR field.
  - Sub-gap 2 (qualified_name): BUILDABLE via key-parse -- qualified_name is embedded in the CanonicalKey
    because make_stable_key builds the key FROM it (extractor.rs:351-353/707/725). [E3, OBSERVED] No IR field.
    (Optional cleaner additive IR field is FEASIBLE but not required; DR-FR-QNAME-SOURCE.)

Therefore NEEDS-IR-EXTENSION is REJECTED: no sub-gap needs an IR field; the only field-option is feasible +
additive + non-recommended. STOP_CONDITION-1 NOT triggered. The remaining work is buildable from the existing
IR, conditional on the §11 architecture-boundary decisions (crate home; the cert is the value-equivalence
proof). This is the opposite branch from TRUST-SUMMARY-LIVEGRAPH-1 (NEEDS-EXTENSION, a dropped-at-ingest
substrate gap) -- here nothing is dropped.
```

---

## 10. Validation plan (for the eventual IMPL; SPEC'D here, NOT RUN)

```text
[V1] Resolution-parity green-compare (the no-loss proof): on a TS corpus (repo-graph self-index + amodx +
     glamCRM, agent_docs/validation.md repos), for the §7d focus corpus, assert the LiveGraph resolution is
     FIELD-EQUAL to the SQLite resolution across all four functions AND the cascade outcome (§2e). RED on any
     divergence. This is the cert's own proof, run off-target (headless Test API, the QUERY-MIGRATION-1
     pattern -- no daemon, no CLI).
[V2] No-`nodes`-read proof (storage spy): with the cert GREEN, exercise the (eventual) consumer fastpath under a
     StoragePort spy that records every `nodes` SELECT; assert ZERO `nodes` reads on the resolution path (the
     read survives ONLY to build the cert + on fallback). Mirrors the drilldown no-read proofs. [Belongs to the
     consumption impl; specified here as the producer's acceptance target.]
[V3] Ambiguity/miss correctness: same-name -> ≤5 candidate parity (count + keys + order); known-miss -> no_match
     parity; non-resident target -> Partial -> fallback (never resolved-as-miss, the null=unknown rule).
[V4] Fallback correctness: non-TS partition, non-Fresh partition, cert-RED fingerprint -> SQLite resolution
     served, labelled (backend_used). Assert byte-identical to today's output.
[V5] Cert invalidation: a refresh that changes the partition fingerprint invalidates the cert (lazy rebuild);
     stale-cert never serves. Mirrors import/cycles/stats cert invalidation tests.

SCOPE of validation: the resolver + its no-loss cert ONLY. The COHERENCE-LEAF-SERVE consumption (V2 wiring,
focused orient + explain) is a LATER slice (§12). V1/V3/V5 are this producer's; V2/V4 are stated as its
acceptance contract for the consumer.
```

---

## 11. Forced decisions — `DECISION_REQUIRED` (architecture-boundary + the optional IR field)

The packet mandates surfacing the crate home + any IR addition as `DECISION_REQUIRED` with exhaustive matrices
(every cell filled). DR-FR-CRATE-HOME is the BLOCKING architecture-boundary call (a new public data shape +
where the producer lives); DR-FR-QNAME-SOURCE is NON-BLOCKING (the recommended path crosses no boundary).

```text
DECISION_REQUIRED:
- ID: DR-FR-CRATE-HOME  [where the focus resolver lives + how its result reaches the agent-port DTOs without a
    dep inversion -- a new public data shape crossing the LiveGraph API boundary]
  QUESTION: the resolver reads ONLY the IR/LiveGraph and applies NO policy (unlike the trust producer). Where
    does it live, and how is its native result mapped to the agent-port DTOs (AgentPathResolution /
    AgentFocusCandidate / AgentSymbolContext) without repo-graph-livegraph depending on the agent crate?
  OPTIONS (exhaustive -- every cell filled):
  - A EXTEND repo-graph-livegraph (add resolve_path/resolve_stable_key/resolve_symbol_name/symbol_context, like
      module_stats/node_display):
      new-dep: NONE. The resolver reads IrNode.name/range/key -- all in repo-graph-ir, ALREADY a livegraph dep
        (the four livegraph deps are repo-graph-ir, repo-graph-trust-model, repo-graph-algorithms,
        repo-graph-import-resolver -- OBSERVED trust-summary-livegraph-1.md:724-725). dep-direction: unchanged.
        precedent: callers:469 / module_stats:1376 / node_display:1051 are EXACTLY this shape (read IR ->
        AnswerEnvelope) and already live here. mapping: native result types in livegraph; the agent-port DTO
        mapping happens in the consumer/daemon adapter (where CallersAnswer is mapped today). VERDICT:
        RECOMMENDED -- adds NO new dependency edge; mirrors the established read-surface precedent; the
        boundary stakes are LOWER than DR-TS-CRATE-HOME (which would have added a heavy policy dep).
  - B NEW CRATE repo-graph-focus-resolver (depends on repo-graph-livegraph + repo-graph-ir):
      new-dep: a new crate + edge. dep-direction: fine (outer of livegraph). precedent: the *-feed pattern --
        but that pattern exists to COMPOSE TWO inner crates (scip-ingest + livegraph); here there is only ONE
        (livegraph/ir), so the composition rationale is absent. VERDICT: REJECTED -- over-structured; introduces
        a crate boundary with no second dependency to justify it (unlike DR-TS-CRATE-HOME C, which composed
        livegraph + the heavy trust crate).
  - C repo-graph-coherence (the existing coherence support crate):
      new-dep: livegraph + ir added to coherence. dep-direction: coherence is PURE wrapper algebra (no LiveGraph
        reads -- its own doc); a producer that READS the LiveGraph violates that purity. VERDICT: REJECTED --
        coherence is the envelope shape, not a producer (mirrors trust-summary DR-TS-CRATE-HOME D).
  - D the daemon livegraph_feed.rs (where the certs' BUILD lives):
      new-dep: none (daemon deps both). dep-direction: fine (outermost) but LAYER-WRONG -- the resolution is
        DOMAIN logic ("main.rs is wiring only"; "domain logic never lives in CLI/daemon" -- architecture.md).
        VERDICT: REJECTED for the COMPUTATION; the cert BUILD/STORE (RwLock plumbing) DOES belong here, beside
        build_and_store_{import,cycles,stats}_cert.
  RECOMMENDED: A (extend repo-graph-livegraph) for the resolver; the cert build/store plumbing in the daemon
    livegraph_feed.rs (mirroring the import/cycles/stats certs); the native-result -> agent-DTO mapping in the
    consumer adapter (the COHERENCE-LEAF-SERVE impl). This adds NO new dependency edge and matches the
    module_stats/callers/node_display precedent.
  BLOCKING_REASON: this introduces a NEW public data shape (focus string -> identity) on the LiveGraph API
    boundary. Per CLAUDE.md ("a new module/crate boundary, dependency edge, or data shape crossing a boundary"
    -> stop and ask) and the packet (surface the crate home as DECISION_REQUIRED), it is surfaced for
    ratification rather than decided unilaterally -- even though, unlike DR-TS-CRATE-HOME, it adds no dependency
    edge and is the cleaner-cut of the two. It blocks the FOCUS-RESOLUTION-LIVEGRAPH-IMPL-1 crate placement.

- ID: DR-FR-QNAME-SOURCE  [how get_symbol_context's qualified_name is sourced -- NON-BLOCKING]
  QUESTION: serve qualified_name by PARSING the CanonicalKey (no IR change) or by CARRYING it as an additive
    IrNode field (cleaner, but touches the IR data shape + SCIP-ingest + warm-cache)?
  OPTIONS (exhaustive -- every cell filled):
  - A PARSE-FROM-KEY (RECOMMENDED): qualified_name = the `#…:SYMBOL:` segment of the key (E3). new-state: NONE.
      boundary: none crossed. risk: fallback-node keys (ScipSynthesizedFallback) are not make_stable_key-shaped
        -> the cert's symbol_context compare catches divergence -> RED -> fallback (§5b/§7c L2). VERDICT:
        RECOMMENDED -- zero boundary change; the fact is already in the key.
  - B ADDITIVE IR FIELD `qualified_name: Option<String>` on IrNode: the ts-extractor HAS the fact
      (extractor.rs:707/784/961); the SCIP-ingest would carry it (mirroring IR-SYMBOL-ATTRIBUTES-1's additive
      SymbolAttributes). new-state: one additive Option field + ingest population + warm-cache mirror DTO.
      boundary: crosses the IR data shape (repo-graph-ir) + ingest + warm-cache. risk: none beyond the additive
      change; it is a FEASIBLE additive extension (not a substrate gap). VERDICT: VIABLE -- cleaner (no string
      parsing), but strictly more work touching three crates for a field recoverable from the key.
  RECOMMENDED: A (parse-from-key). It crosses no boundary and the cert guards the fallback-node edge case. B is
    recorded as the clean alternative if a reviewer prefers an explicit field over key-parsing.
  BLOCKING_REASON: NON-BLOCKING. The recommended option (A) needs no decision to proceed (no boundary crossed).
    Surfaced ONLY because option B would cross the IR boundary, and the packet asks any IR addition be surfaced.
    Work continues on A absent a directive to take B.
```

---

## 12. Scope boundary (what this spec does NOT touch)

```text
IN SCOPE (this spec): the focus-resolution producer DESIGN (the four LiveGraph surfaces) + its no-loss cert
DESIGN + the per-kind feasibility + the two sub-gaps resolved + the BUILDABLE-FROM-EXISTING-IR verdict + the
surfaced decisions. SPEC ONLY.

OUT OF SCOPE (explicit, mirrors the packet FILES_OUT_OF_SCOPE):
- ANY code / src/** / rust/** / scripts/** -- spec-first.
- The COHERENCE-LEAF-SERVE consumption impl (focused orient + explain calling the resolver; the native-result ->
  agent-DTO mapping; the V2 no-read wiring; the bounded composite cert AND-fold of this cert) -- a LATER slice.
- PREREQ-2, the retirement impl, P2 (non-TS coverage), P1 (the marginal fastpaths).
- The (c) trust unresolved-call boundary (FIXED, Option A) -- orthogonal.
- ROADMAP.md / CURRENT_SLICE.md edits.
- Building the optional qualified_name IR field (DR-FR-QNAME-SOURCE B) -- not taken.
```

---

## 13. Validation / evidence ledger (this slice)

```text
[OBSERVED] The four resolve_* impls + their nodes reads + outputs -- agent_impl.rs:366/437/800/834 (§2).
[OBSERVED] The output DTOs -- storage_port.rs:336/349/368/387; the StoragePort signatures :516-610.
[OBSERVED] The consumer cascade -- orient/mod.rs:100-219; explain/mod.rs:86-165 (§2e).
[OBSERVED] LiveGraph keyed by CanonicalKey, no resolver surface -- lib.rs:469/586/688/842/1031/1051 (E1).
[OBSERVED] The FileInventory path->FILE-key map exists -- lib.rs:1077-1088 (E2).
[OBSERVED] qualified_name embedded in the key (make_stable_key builds key FROM it) -- extractor.rs:351-353,
           top-level :429, method :707+:725 (E3).
[OBSERVED] dir-MODULE = pure function of file paths; key {repo}:{dir}:MODULE -- orchestrator.rs:1005-1039 (E4).
[OBSERVED] symbol->module = dirname(file); OWNS = immediate-parent MODULE -- orchestrator.rs:830-840/1043-1069;
           resolver.rs:671-678 (E5).
[OBSERVED] IR carries name/subtype/range, NO qualified_name field, NO MODULE node -- ir/lib.rs:338-360, 60-66.
[OBSERVED] COHERENCE-LEAF-SERVE-1 §5 + §10 DR-CLS-FOCUS -> B -- coherence-leaf-serve-1.md:221-310, 499-554.
[OBSERVED] orient DR-4 / explain DR-E2 (focus resolution No-LG-producer) -- orient-sqlite-free-1.md:202;
           explain-sqlite-free-1.md:276.
[OBSERVED] crate-home matrix precedent -- trust-summary-livegraph-1.md:720-763.
[INFERRED] Per-kind feasibility (§4); the two sub-gaps resolved (§5); the resolver + cert design (§6/§7); the
           BUILDABLE-FROM-EXISTING-IR verdict (§9) -- all INFERRED over the OBSERVED facts above.
[NOT RUN]  rmap orient -- the local daemon socket refused connection this session (os error 61). No-daemon
           posture recorded; per the packet, all evidence is first-hand source reads (file:line'd above).
[NOT RUN]  V1-V5 (§10) -- impl-phase validation; this is a spec.
```

---

## 14. Guardrails honored

```text
- Spec-first: exactly one deliverable (this file). No src/rust/scripts touched; no code; no IR field added; no
  cert built; no default flipped (CLAUDE.md spec-before-impl; packet FILES_OUT_OF_SCOPE).
- Architecture-boundary decisions surfaced, not decided unilaterally: DR-FR-CRATE-HOME (BLOCKING, exhaustive
  matrix) + DR-FR-QNAME-SOURCE (non-blocking, exhaustive matrix) -- CLAUDE.md Decision Autonomy ("data shape
  crossing a boundary -> stop and ask"); the packet STOP_CONDITIONS.
- STOP_CONDITION-1 (a sub-gap needs a non-feasible IR field) explicitly checked + NOT triggered (§5c).
- STOP_CONDITION (consumed contract cannot be located / contradicts COHERENCE-LEAF-SERVE-1): located first-hand
  (§2), CONSISTENT with COHERENCE-LEAF-SERVE-1 §5 (§3c). No contradiction.
- Evidence Law: every claim labelled OBSERVED / INFERRED / NOT RUN, file:line'd (agent_docs/validation.md).
- Fact-Certainty Model: the producer preserves the Layer-0/1 extracted-fact class; the no-loss cert prevents a
  silent downgrade; honest TS-only scope + labelled SQLite fallback (CLAUDE.md; VISION layer model).
- Reuse over reinvention: mirrors the EXISTING module_stats/node_display read-surface precedent + the EXISTING
  import/cycles/stats cert-fastpath plumbing (no new invalidation key, shared fingerprint).
```

---

## 15. References

- `docs/slices/coherence-leaf-serve-1.md` (`741670f`) — §5 the focus-resolution gap + the two sub-gaps; §10
  DR-CLS-FOCUS → Option B (the ratified decision this slice specs); §6 the bounded composite cert this cert
  AND-folds into.
- `docs/slices/orient-sqlite-free-1.md` (`e10a455`) — DR-4 (focus resolution No-LG-producer; BLOCKING for
  focused orient; repo/None exempt).
- `docs/slices/explain-sqlite-free-1.md` (`f3237f9`) — DR-E2 (focus resolution No-LG-producer; BLOCKING,
  unconditional — explain always resolves a target).
- `docs/slices/trust-summary-livegraph-1.md` (`94fc506`) — the producer-feasibility-spec precedent (the
  NEEDS-EXTENSION sibling) + the DR-TS-CRATE-HOME exhaustive-matrix precedent this doc mirrors.
- `docs/slices/sqlite-raw-decommission-1.md` — the ratified bounded contract; PREREQ-1 = "the (b) leaves served"
  (this producer + the COHERENCE-LEAF-SERVE consumption close it for focused orient + explain).
- `rust/crates/storage/src/agent_impl.rs:366/437/800/834` — the four consumed `resolve_*` functions.
- `rust/crates/agent/src/storage_port.rs:336/349/368/387` — the output DTO contract.
- `rust/crates/agent/src/{orient,explain}/mod.rs` — the consumer cascade.
- `rust/crates/repo-graph-ir/src/lib.rs` + `rust/crates/repo-graph-livegraph/src/lib.rs` — the IR shape (no
  qualified_name field, no MODULE node) + the LiveGraph surfaces (CanonicalKey-keyed; FileInventory map).
- `rust/crates/ts-extractor/src/extractor.rs` + `rust/crates/indexer/src/{orchestrator,resolver}.rs` — the
  key/qualified_name producer + the directory-MODULE/OWNS derivation (the parity sources for the two sub-gaps).
- `agent_docs/architecture.md`, `agent_docs/validation.md` — the layer model + the evidence protocol.
