# IMPORT-RESOLUTION-RUST-1 — a `use other_crate::…` resolves to the file that defines it

Status: SPECIFIED (2026-09-06) · Track: audit round five, group A, first of three (human-ratified
2026-09-06: "rust, then java, then c"). CODE slice, indexer resolver + repo-index compose
boundary + cargo manifest reader + modules-list/cycles zero-state. Maturity: MATURE (modules,
cycles, stats, trust, map, imports, path, explain all consume resolved IMPORTS edges).

## 1. Problem (ROOT-CAUSED — `docs/audits/2026-09-06-root-causes-v0.17.0.md` §A; seam pinned)

Outward surface: `rmap modules list` / `cycles` / `stats` on any Rust workspace — including
repo-graph's own 59 crates — print "No cross-module dependencies detected. hint: all imports
are intra-module", while `trust` on the same snapshot counts 2,821 internal references. The
self-model of this product has no crate graph. Never worked (June's smoke already 0;
MODULE-EDGES-1 was verified on C++ includes only).

Cause, exactly: the Rust extractor emits a non-relative `use` as an IMPORTS edge whose
`target_key` is the use-path minus its last segment, verbatim (`use repo_graph_storage::crud::
foo::Bar` → `repo_graph_storage::crud::foo`; `rust-extractor/src/extractor.rs:259-267,315-372`).
`resolve_import_target` (`indexer/src/resolver.rs:376-418`) has four stages — exact key, the
`file_resolution` map (identity + extensionless + index.ts/__init__.py, :799-836), the C/C++
include map, a repo-prefix fallback skipped when the key contains `:` (:404) — and none maps a
crate name to a directory. The declared-module catalog that could (`CargoModule{package_name,
crate_root}`, `indexer/src/cargo_manifest.rs:67-80`) is parsed IN MEMORY by `compose.rs`
(:676-707) BEFORE `index_repo` (:3401) but is not passed across: only `file_inputs`,
`contract_file_inputs` and `IndexOptions` (`indexer/src/types.rs:497-527`) cross that boundary;
`module_candidates` are persisted only AFTER the snapshot is READY (:3599). Every cross-crate
import therefore lands in `unresolved_edges` as `imports_file_not_found`, which is (i) what
trust's first-party attribution reads and (ii) what makes trust's Import-graph LOW.

## 2. Contract

1. **The catalog crosses the boundary as a DTO.** `IndexOptions` gains an ADDITIVE field
   carrying the declared modules as raw data: `Vec<DeclaredModule{ ecosystem, name,
   canonical_root }>` — no manifest structs, no storage rows (boundary rule). `compose.rs`
   fills it from the modules it already parsed; `ResolverIndex` holds it. One additive field;
   the twelfth-precedent additive-port pattern.
2. **A Rust resolution stage, pure.** In `resolve_import_target`, after the existing stages
   and BEFORE the repo-prefix fallback: split the key on `::`; if the first segment, with
   `_`→`-` canonicalisation, names a declared Cargo package, generate candidate FILE stable
   keys under `<crate_root>/src/` in this order — `<segs>.rs`, `<segs>/mod.rs`, then shorten
   by one segment and repeat, ending at `src/lib.rs` (a `[[bin]]`-only crate ends at
   `src/main.rs`) — and take the FIRST candidate present in the existing `file_resolution`
   identity map. The stage is a pure function `(key, catalog, file set) → Option<file key>`
   with no I/O (core purity; it is also the shape a future reverse map derives from). The
   crate's own `crate::` paths are unchanged (they already resolve).
3. **One canonicalisation — of the `_`→`-` EQUALITY family.** The `_`→`-` canonicalisation
   used for name equality exists three times (`storage/src/trust_impl.rs:128-130`,
   `module-queries/src/deps/reconcile.rs:258`, `classification/src/unresolved_classifier.rs:
   466`) plus the new stage; the slice defines it ONCE in a crate all can import without a new
   dependency edge (chosen: `classification::cargo_name`) and replaces those copies.
   AMENDED 2026-09-06 (cycle-1 finding, name-vs-semantics): `repo-index/src/config.rs:384,417`
   is NOT a copy — it performs the OPPOSITE transform (`-`→`_`) to build the underscore-form
   `PackageDependencySet` that `has_package_dependency` compares against; folding it would
   invert its output. It stays as is; the two families are documented at the shared function.
4. **Manifest facts the stage needs are parsed; the rest is stated.** The Cargo reader gains
   `[lib] name` (overrides the import name). AMENDED 2026-09-06: bin-only crates are resolved
   by the stage's entrypoint probe (`src/lib.rs`, then `src/main.rs`) — the frozen DTO carries
   no bin path; custom `[[bin]] path` values, `[dependencies] foo = { package = "bar" }` renames
   and `[lib] path` overrides stay unparsed and are STATED as a limitation in the build report
   with the count of manifests in the corpus that use them (repo-graph: 0 / 0 / 0 custom;
   2 default `[[bin]]` covered by the probe), not silently mis-resolved.
5. **The output reflects the resolution (deep-vertical).** `modules list` prints the edge
   count and, on the same line, the unresolved-import count the handler already computes
   (`facts.diagnostics`) — `N cross-module dependencies (M imports unresolved)`; the "all
   imports are intra-module" hint is emitted only when M == 0 (never conditioned on the HTTP
   link count alone). `cycles`' zero branch states `over N modules / E resolved import edges`.
   These are the surfaces where the delivered resolution becomes visible; they are in scope
   because the deep-vertical rule requires it, not as a substitute for the resolution.
6. **Movement measured, before/after, verbatim (isolated).** repo-graph: `modules list` edge
   count (expect dozens across 59 crates), `cycles` (expect the known crate cycles or a true
   zero over E > 0 edges), `stats` module fan-in/out, `trust` "Import-graph" level and
   "internal crate/package → X: N" lines (expect the first-party lines to SHRINK by the
   resolved count — state it), the classification histogram
   (`imports_file_not_found` before/after), `map --dry-run` resolved vs unresolved counts,
   `explain` on a cross-crate type's file imports. zvec-grep (single crate): byte-stable.
   Non-Rust repos (django, leveldb, kafka): byte-stable.

## 3. Stop conditions

Frozen: wire protocol (the `IndexOptions` field is process-internal; any response field is
ADDITIVE), storage schema shape, exit codes, the other three resolver stages' behaviour (the
new stage runs only when stages 1–3 miss and the first segment names a declared package),
non-Rust extractors. If §2.2 needs the catalog at a point where `compose.rs` has not parsed it
(refresh path: `:3785 → :3886`), thread it the same way or STOP. If a Cargo layout in the
corpus defeats the candidate order (a `[lib] path` crate, a `path` dependency outside the
workspace), record it with its count and STOP + DECISION_REQUIRED rather than add heuristics.
STANDING HONESTY RULES. Unmet DoD → STOP + DECISION_REQUIRED. Never touch the operator's
real state root; every rmap call isolated; never stash. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Fixture FIRST: a two-crate workspace under `repo-index/tests/fixtures/rust/workspace/`
  (`a` with a `path` dependency on `b`; `a/src/lib.rs` uses `b::util::helper` and
  `b::Thing`; `b/src/util.rs`, `b/src/lib.rs`). Integration test asserts: both IMPORTS edges
  resolve to `b/src/util.rs` and `b/src/lib.rs`; `imports_file_not_found == 0`; one
  MODULE→MODULE edge a→b; `modules list` renders it; `cycles` renders "over 4 directory
  groups / 1 resolved import edge". AMENDED 2026-09-06 (cycle-4 finding): `cycles` runs over
  the indexer's per-directory MODULE nodes (the population `stats` already calls "directory
  groups"), not over declared crates — the two-crate fixture has four directory groups
  (`a`, `a/src`, `b`, `b/src`). The zero-state names that population by the term the user
  already sees ("directory groups"), never a bare "modules" that `modules list` would read
  as crates. The cross-surface divergence (cycles over directory groups vs modules list over
  declared modules) is recorded as a follow-up, not changed here. Unit tests for the pure stage: nested module → `mod.rs`; shortened
  path → parent file; unknown crate → None; `[lib] name` override; canonicalisation.
- Live proof (isolated; registry sha identical before/after; repo-graph indexed ONCE
  before and ONCE after — the retained audit root may serve as "before"): the §2.6 table.
- Gates recorded FIRST in `build-progress.md`; chunked cargo; witness; dogfood-isolated.

## 5. Definition of done

Cross-crate `use` paths resolve to files by the stated candidate order; repo-graph's crate
graph renders in modules list / cycles / stats with the unresolved count beside it; trust's
first-party lines shrink by the resolved count and the histogram moves accordingly; one
canonicalisation function; limitations stated with counts; non-Rust byte-stable; gates green.

CORPUS PATHS: repo-graph is THIS repo; zvec-grep, django, leveldb, kafka at
../legacy-codebases/<name>.
