# KEY-NAMESPACE-REPO-RELATIVE-1: repo-relative file-scope identities (Stage D, foundational)

Slice ID: KEY-NAMESPACE-REPO-RELATIVE-1
Status: **IMPLEMENTED + validated (2026-06-02).** Ingest passes a repo-relative `file_path` to the producer
(all keys repo-relative; producer untouched); daemon computes the prefix; warm-cache `SCHEMA_VERSION 3→4`.
Single-partition is byte-stable. Unblocks IMPORTS-XPART-WIRING-1. See Completion.
Depends: `repo-graph-ir` (keys), `repo-graph-scip-ingest` (key construction), `ts-extractor` (producer; key
shape), `repo-graph-warm-cache` (cache invalidation), the daemon preload/refresh (prefix source).
Track: Stage D, **foundational identity invariant**. BLOCKS IMPORTS-XPART-WIRING-1 (and any multi-partition
correctness). NOT a cross-partition slice itself.

## Why (the finding that triggered D0)
```text
ALL canonical keys embed the file path, PARTITION-relative:
  FILE node:   `{repo}:{file_path}:FILE`                         (ts-extractor:160)
  symbol node: `{repo}:{file_path}#{name}:SYMBOL:{subtype}`      (ts-extractor:351 make_stable_key)
  import tgt:   `{repo}:{resolved_path}:FILE`                    (ts-extractor:1407)
`file_path` = the SCIP document path, relative to the partition root (`{root}/{relative_path}`, one
PartitionIr per TS package). So two packages collide: `packages/a/src/main.ts` and `packages/b/src/main.ts`
both key to `{repo}:src/main.ts:FILE`. The flat LiveGraph `defines` map (key -> basis) is then UNSOUND under
collisions (one file overwrites the other), and cross-partition edges have ambiguous endpoints. This is a
foundational identity invariant, not a local detail.
```

## Goal
```text
Every file-scope identity that can cross a partition boundary uses a REPO-RELATIVE path, not a
partition-relative one — collision-free across packages, with NO change to the producer's key SHAPE.
```

## Ratified decision (D0) + design
```text
D0 = do this prerequisite BEFORE IMPORTS-XPART-WIRING-1 (ratified 2026-06-02).
DESIGN (localized; producer UNCHANGED): the INGEST passes a REPO-RELATIVE `file_path` to the producer
(`extractor.extract(source, file_path = <repo-relative>, ...)`). Because every key derives from
`self.file_path` / `repo_uid`, ALL keys (FILE + symbol + import target + resolved_path) become repo-relative
automatically. The producer's key shape is untouched (it just receives a repo-relative path).
  repo-relative file_path = join(partition_prefix, doc.relative_path)
  partition_prefix        = the partition's repo-relative root (e.g. "packages/a"; "" for a repo-root package)
  source is still READ from `{partition_root}/{doc.relative_path}` (decoupled from the key path).
```

## Forced decisions (to ratify at sign-off)

### 1. Key construction
```text
ingest qualifies the SCIP doc path with the partition's repo-relative root:
  partition_prefix "packages/a" + doc "src/main.ts" -> file_path "packages/a/src/main.ts"
  -> FILE key `{repo}:packages/a/src/main.ts:FILE`, symbol `{repo}:packages/a/src/main.ts#X:SYMBOL:..`.
`partition_prefix` source: the daemon has the repo path (`--repo`) AND the partition source root
(`--source-root`); prefix = source_root RELATIVE TO repo root. `ingest_partition` gains a `partition_prefix`
param (or repo_root); the daemon computes + passes it. Single-partition (repo == source_root) -> prefix "".
```

### 2. Affected identities
```text
FILE nodes; AstFileScope nodes; symbol nodes (make_stable_key); import edge target_key + resolved_path;
ANY canonical key embedding the file path. (ImportObservation.source_file in IMPORTS-XPART-WIRING-1 will then
also be repo-relative — consistent.) ALL fixed by passing the repo-relative file_path; no per-key surgery.
```

### 3. Backward compatibility
```text
Cached PartitionIrs hold the OLD partition-relative keys; new code expects repo-relative. Mixing is unsound.
-> warm-cache `SCHEMA_VERSION` BUMP (3 -> 4): old caches -> SchemaMismatch -> re-extract through the existing
manifest gate. NO backward key translation (do not rewrite old keys). The warm cache is disposable.
```

### 4. Path normalization
```text
POSIX `/` separators; resolve/forbid `..` and `.` in the joined repo-relative path (normalize_join, as the
resolver already does); the prefix itself is normalized (no trailing slash, POSIX). Symlink stability:
DEFER (document) — paths are taken as the producer/daemon present them; no symlink canonicalization this slice.
```

### 5. Validation
```text
- a synthetic TWO-partition fixture/test where BOTH partitions have `src/index.ts` -> the two FILE keys
  DIFFER (`{repo}:packages/a/src/index.ts:FILE` vs `.../packages/b/...`); the LiveGraph `defines` map has NO
  overwrite (both present).
- the existing SINGLE-partition synthetic stays stable (prefix "" -> keys UNCHANGED), OR gains the expected
  prefix if relocated under a subdir; pick the synthetic so prefix "" keeps it byte-stable.
- callers/callees/path behavior UNCHANGED for a single partition (same keys, prefix "").
```

### 6. Trust
```text
Old partition-relative caches are DISCARDED (schema bump), never mixed with repo-relative keys. No false
identity carried across the namespace change.
```

## Out of scope (hard guardrails)
```text
No cross-partition resolution (that resumes in IMPORTS-XPART-WIRING-1). No daemon multi-partition
enumeration. No CLI/cycles/default migration. No producer key-SHAPE change (only the path VALUE is
repo-relative). No symlink canonicalization (deferred). No raw decommission.
```

## Acceptance (EXECUTED later)
```text
1. ingest builds repo-relative keys via `partition_prefix` (daemon-computed); producer unchanged.
2. two-partition test: same-named files -> distinct keys; `defines` no overwrite; cross-partition keys
   unambiguous.
3. single-partition synthetic: prefix "" -> keys byte-stable; callers/callees/path/cycles unchanged.
4. warm-cache SCHEMA_VERSION 3 -> 4; old caches re-extract; round-trip preserves repo-relative keys.
5. full workspace test green; clippy + fmt clean.
```

## Commit structure (proposed)
```text
1. ingest + daemon: `ingest_partition` gains `partition_prefix`; ast_facts_for_source receives repo-relative
   file_path; daemon preload/refresh computes the prefix (source_root relative to repo). + tests (incl. a
   two-partition unit/ingest test). Producer UNTOUCHED.
2. cache: warm-cache SCHEMA_VERSION 3 -> 4 (discard old) + a note; round-trip test.
3. docs: status/evidence.
```

## Completion (implemented + validated 2026-06-02, EXECUTED)

Commit `b72b075` (ingest + daemon + cache v4 + tests) + this doc.

- **repo-graph-scip-ingest**: `repo_relative_file_path(prefix, doc)` (POSIX-normalized; `..` rejected);
  `ingest_partition` gains `partition_prefix`; pass 1 computes the repo-relative `key_path` per doc and
  passes it to the producer (`ast_facts_for_source`) + `build_partition_nodes` (synthesized fallback key +
  `range.file`); the source is still READ from the partition-relative on-disk path; pass 3 import source
  uses `key_path`. The producer (`ts-extractor`) is UNTOUCHED — keys become repo-relative because every key
  derives from the `file_path` it receives.
- **daemon-runtime**: `livegraph_feed::repo_relative_prefix(repo_path, source_root)`; the dev-preload
  handler computes the prefix; `preload_into`/`preload_partition` thread it; `livegraph_refresh` passes `""`
  (default partition == repo root; F2).
- **repo-graph-warm-cache**: `SCHEMA_VERSION 3 → 4` (key VALUES changed; old partition-relative caches →
  SchemaMismatch → re-extract; NO key translation).

```text
Tests (EXECUTED): repo-graph-scip-ingest 11 unit (incl. repo_relative_path_rejects_dotdot_and_normalizes;
  two_partitions_repo_relative_keys_are_distinct -> the SAME source under packages/a + packages/b yields
  DISJOINT keys, no defines overwrite) + 10 harness; the single-partition synthetic stays byte-stable
  (prefix "" -> keys unchanged, callers/callees/path/cycles unaffected). Full cargo test --workspace green;
  clippy --workspace -D warnings + fmt clean.
```

### Stop conditions — outcomes
```text
- non-FILE symbol keys derive from the SAME file_path (make_stable_key:351) -> consistent; NOT triggered.
- no existing query assumes partition-relative key DISPLAY -> the `range.file` display is now repo-relative
  too (= key_path; single-partition unchanged); keys are internal node_ids, never displayed -> NOT triggered.
- cache schema bump contained to warm-cache const + daemon re-extract -> NO ripple; NOT triggered.
```

### Deferred (recorded)
```text
- SYMLINK CANONICALIZATION: paths are taken as the producer/daemon present them; no symlink resolution.
  A repo reachable via two symlinked roots could key the same file twice. Deferred (document) — out of scope.
- Multi-partition daemon ENUMERATION (compute per-package prefixes for `livegraph_refresh`):
  IMPORTS-XPART-ENUMERATION-1.
```

## Follow-up
```text
- RESUME IMPORTS-XPART-WIRING-1 (now correct on real multi-partition repos): source_file (repo-relative) +
  the LiveGraph cross-partition overlay.
```

## References
- `docs/slices/imports-xpart-wiring-1.md` (the BLOCKED consumer that needs this)
- `rust/crates/ts-extractor/src/extractor.rs:160` (FILE key), `:351` (`make_stable_key`), `:1407` (import target)
- `rust/crates/repo-graph-scip-ingest/src/lib.rs` (`ingest_partition`; `ast_facts_for_source`; reads `{root}/{relative_path}`)
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (`preload_into` source_root; the daemon has `--repo` + `--source-root`)
- `rust/crates/repo-graph-warm-cache/src/lib.rs` (`SCHEMA_VERSION`)
