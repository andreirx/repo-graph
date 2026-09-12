# PYTHON-SELF-BINDING-1 — `self.method()` binds through the class hierarchy or stays honestly unresolved

Status: SPECIFIED (2026-09-12) · Track: audit round six, Q9 (MEDIUM; never worked). CODE slice: `python-extractor/src/extractor.rs`, `indexer/src/resolver.rs` (one gated stage), `storage/src/indexer_impl.rs` (populate superclasses from metadata already read). Requires reindex of Python repos. Builder: Codex gpt-5.6-sol; reviewer: Codex gpt-5.6-terra.

## 0. Requirements allocation

**Implements:** RG-REQ-005-L03 (inherited `self.method()` binds via the class hierarchy or stays unresolved), RG-REQ-005-L02 (binding only on evidence), RG-REQ-001-L03 (unresolved preserved with a named category).

**Changes (simulated in RC-2):** django gains ~3,600 CALLS edges (2,544 own-class + 1,093 unique-ancestor), +6% resolved; 385 MRO collisions stay unresolved; `explain BaseHandler.get_response` Callers 0 → 2 (`WSGIHandler.__call__`, `ClientHandler.__call__`); trust's calls-resolved % RISES on Python repos (reported); `dead`'s substrate sees more fan-in.

**Preserves (§3):** every non-Python extractor byte-stable (leveldb, FRAKTAG edge counts), the TS `this.` rewrite, RG-REQ-005-L01 (the C++ invariant — the shared resolver's gated block untouched), RG-REQ-006-L01/L02/L04, RG-REQ-005-L07/L09, `exception.py`'s closure calls stay unresolved (correct), `ASGIHandler` not a caller (`get_response_async`).

## 1. Problem (ROOT-CAUSED — RC-2)

`python-extractor/src/extractor.rs:1094-1105` emits `self.get_response` as a string target; `indexer/src/resolver.rs:888-935`'s only receiver-aware branch is spelled `this.` (:922); the bare-name fallback sees 8 nodes named `get_response` and declines; `categorize_unresolved_call:1252` files it as `calls_obj_method_needs_type_info`. No inheritance edge exists for Python, but `extract_class` stores `metadata_json.superclass` raw text on every class node (`WSGIHandler → "base.BaseHandler"`). Never worked (4c9071e).

## 2. Contract

1. Extractor: when the call key is `self.<m>`/`cls.<m>` and `ctx.current_class == Some(C)`, keep the key and add `metadata_json {"selfCall": true, "enclosingClass": C}` (do NOT rewrite to `C.m` — the resolver has no qualified-name lookup; the TS precedent buys nothing).
2. Resolver index: `nodes_by_qualified_name` and `superclasses: Vec<String>` on `ResolverNode`, parsed from the metadata `query_resolver_nodes` already reads (split on `,`, drop keyword args/subscripts, map the head through the file's import bindings else take the last dotted segment).
3. One stage before the dotted fallback (:888), firing ONLY on `selfCall`: BFS the ancestor closure from `enclosingClass` (self first); at each depth collect `nodes_by_qualified_name["<Ancestor>.<m>"]` with subtype METHOD; the first depth with exactly one hit binds; >1 at a depth (real MRO collision) → unresolved with a new additive category `calls_self_method_ambiguous_mro` (mapped in `attribution.rs`); no hit → unchanged category.
4. Outward proof on django (isolated index, stdio): `explain BaseHandler.get_response` Callers (2) = `WSGIHandler.__call__` (wsgi.py:124) + `ClientHandler.__call__` (client.py:186); `ASGIHandler` absent; exception.py:43/56 absent; resolved/unresolved CALLS before/after; trust % before/after.

## 3. Regression watch

| Preserved L | What would regress | Proof |
|---|---|---|
| RG-REQ-005-L01/L02 | the C++ gated block and the bare-name rule | `resolver.rs` C++ tests (as rewritten by Q1) green; `ambiguous_name_stays_unresolved` |
| RG-REQ-006-L01/L02/L04 | other resolver stages | their families green |
| TS `this.` | unchanged rewrite at `ts-extractor:1311-1317` | ts tests green; FRAKTAG edge counts identical |
| Non-Python extractors | byte-stable | leveldb + FRAKTAG CALLS/unresolved counts identical (isolated indexes) |
| RG-REQ-001-L03, RG-REQ-005-L09 | classification + reader mapping | `every_basis_code_maps_to_its_expected_reader_class` includes the new category |
| RG-REQ-001-L09 | determinism | `indexer/tests/parity.rs` green (fixture corpus may gain edges — update the fixture expectations and state it) |
| RG-REQ-011-L06 | isolation | throwaway roots; registry sha unchanged |

## 4. Stop conditions

Frozen: the shared resolver's hot path for other languages (gate on `selfCall` metadata only), schema (no column), wire (additive), exit codes. If ancestor resolution needs import-binding resolution beyond the file's own bindings (cross-package bases), stay unresolved and record the remainder — no guessing. STANDING HONESTY RULES. Do NOT commit.

## 5. Validation (ORDERED)

1. Failing tests FIRST: `Base.run` / `Sub(Base)` `self.run()` → one edge to `Base.run`; two same-depth ancestors defining `run` → unresolved with the new category; `cls.m()` variant; a `self.attr()` instance-attribute call stays unresolved.
2. `cargo test -p repo-graph-python-extractor`, `-p repo-graph-indexer`, `-p repo-graph-storage --lib indexer_impl`, `-p repo-graph-agent`.
3. Live proof: isolated django index (stdio; django indexes within the window); before-binary via worktree on a second isolated root for the counts; byte-stability on isolated leveldb and FRAKTAG.
4. `build-N.md`.

## 6. Definition of done

§2.4 holds; §3 green; gates green.

CORPUS PATHS: django, leveldb at ../legacy-codebases/<name>; FRAKTAG at ../FRAKTAG.
