# DEPS-CLASSIFIER-1 — a declared dependency is "used" when any of its modules is imported

Status: SPECIFIED (2026-09-06) · Track: audit round five, group B (human-ratified 2026-09-06:
B2 at the cause, not a query-time gate). CODE slice, classification + python-extractor +
ts-extractor + module-queries deps + deps headline. Maturity: MATURE (deps list, trust's
external attribution, unresolved-edge classification counts all inherit).

## 1. Problem (ROOT-CAUSED — `docs/audits/2026-09-06-root-causes-v0.17.0.md` §B, code-cited)

Outward surface: `rmap deps list` answers "which of this repo's declared dependencies does the
code actually use?" On django it answers `no static import: asgiref` — asgiref is imported at
42 sites (`from asgiref.sync import …` ×29, `from asgiref.local import Local` ×11, …). An agent
acting on that row removes a live dependency. tzdata in the same row is genuinely unused, which
hides the false half.

Cause: `import sqlparse` produces the specifier `sqlparse`, which EXACTLY equals the declared
name; `from asgiref.sync import …` produces `asgiref.sync`, which does not. The index-time
classifier (`classification/src/unresolved_classifier.rs:457-494` `resolve_declared_dependency`;
`signals.rs:18-20` `has_package_dependency` is `==`) has no Python head reduction and no npm
subpath reduction (`storybook/test` → `storybook`); its only dotted logic is the Java branch.
Those reductions exist ONLY at query time (`module-queries/src/deps/normalize.rs`), which the
classifier never reaches. Result: every asgiref import/call edge is classified `unknown` — a
FOURTH bucket that `deps list` never reads and NO headline counts (the "419 unattributed"
figure is `external_library_candidate`-only; the "could contain asgiref" premise was false).
Two more layers on the same path: (R2) the Python extractor writes the IMPORTS target_key in
slash form `asgiref/sync` while the binding specifier is dotted `asgiref.sync`
(`python-extractor/src/extractor.rs:522-527`), so an import edge would be rejected as a
"non-import fragment" at query time even if classified external (`compose.rs:557-576`);
(R3–R5) the TS extractor emits an IMPORTS edge only for relative resolved imports
(`ts-extractor/src/extractor.rs:1377-1400`), so npm usage evidence is calls-only, `require()`
never produces a binding (:2252), and type-only imports produce no evidence. (C1/C2/C4) the
negative is emitted from the observed set alone (`reconcile.rs:173-187`) with a hardcoded
basis `no_static_import_found` (`deps_headline.rs:470`) and a fixed caveat naming causes that
do not apply; the false row is a canonized test fixture (`rgr … deps_list.rs:1155-1195`).
Never worked; HONESTY-GATE-1 relabeled rather than attributed (its DoD allowed it).

## 2. Contract

1. **Classification matches the package, not the full specifier.** At index time,
   `resolve_declared_dependency` reduces the specifier to its package head per ecosystem
   before matching: Python `a.b.c` → `a` (and PEP 503 normalisation: `-`/`_`/case);
   npm `@scope/pkg/sub` → `@scope/pkg`, `pkg/sub` → `pkg`; Rust `a::b` → `a` (already);
   Java unchanged. The reduction is ONE shared function used by BOTH the classifier and the
   query-time `normalize.rs` — one source of truth, not two copies (this is the demonstrated
   duplication that earns the shared module).
2. **Import edges carry the specifier the binding carries.** The Python IMPORTS target_key
   and the binding specifier agree (dotted), or the storage query reads
   `metadata_json.specifier` — one representation, stated. Same check for TS: a bare package
   import produces an IMPORTS edge (unresolved, external candidate) so usage evidence is not
   calls-only; `require('x')` with a string literal produces a binding; `import type` produces
   a binding flagged type-only (the JSON flag `is_type_only` survives the storage read —
   `module_edges_support.rs:348-361` currently drops it).
3. **The negative has a computed basis or is not printed.** `deps list` prints a per-package
   status with its basis: `used (N import sites, M call sites)`, `type-only import`,
   `referenced in config only (<file>)` when a config-string reference is extracted, and
   `no static import found` ONLY when the package's normalised head appears in no binding,
   no import edge, and no unknown-classified edge of the module. The fixed caveat text is
   replaced by the per-row basis; the headline's unattributed figure states what it counts.
   The deps_list fixture asserting `no static import: asgiref` is replaced by the true row.
4. **Every ecosystem present is named.** When a repo declares manifests in more than one
   ecosystem and only the dominant is rendered, one line names the others with their manifest
   count and the flag to see them (`npm: 1 manifest — rmap deps list --ecosystem npm`); the
   two-root-manifests canonical-path collision (`compose.rs:140/212`) is resolved so
   `--ecosystem npm` on django renders its 6 devDeps.
5. **Reindex transition and count movement measured.** Classification changes → the
   `unknown` bucket shrinks and `external_library_candidate` grows; trust's external
   attribution and "N unresolved imports" move. Before/after, verbatim, on django, storybook,
   zvec-grep, hadoop (Maven still unparsed — its honest refusal must be byte-stable): the
   deps table, the classification histogram (`SELECT classification, COUNT(*) FROM
   unresolved_edges GROUP BY 1`), trust's external/first-party lines.

## 3. Stop conditions

Frozen: storage schema SHAPE (values move; no new columns beyond what §2.2 needs — if the
slash/dot fix needs a column, STOP + DECISION_REQUIRED), exit codes, wire protocol, Java/Rust
classification behaviour (byte-stable on repo-graph, kafka trust lines). If §2.2's TS import
edge changes `check`/trust verdicts on TS repos beyond the deps surface, report the movement
and STOP for ratification before widening. STANDING HONESTY RULES. Unmet DoD → STOP +
DECISION_REQUIRED. Never touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Failing tests FIRST: `unresolved_classifier`: `edge_with_meta("asgiref/sync",
  {"specifier":"asgiref.sync"})` with deps `["asgiref"]` → `ExternalLibraryCandidate`
  (today `Unknown`); npm `storybook/test` with `["storybook"]` → external; `@scope/pkg/sub`;
  PEP 503 `Django-Extensions` vs `django_extensions`. `compose`: bindings
  `[(f,"sync_to_async","asgiref.sync")]`, declared `["asgiref"]` → `DeclaredAndUsed` with
  basis. Storage: the type-only flag round-trips.
- Live proof (isolated state root; registry sha identical before/after): the §2.5
  measurements on django, storybook, zvec-grep, hadoop; ground-truth greps for five storybook
  rows that flip (state each search with all extensions ts/tsx/mts/cts/js/mjs/cjs).
- Gates recorded FIRST in `build-progress.md`; chunked cargo; witness; dogfood-isolated.

## 5. Definition of done

`deps list` never prints "no static import found" for a package whose module is imported;
asgiref reads `used`; each row's status carries a computed basis; every present ecosystem is
named; the shared head-reduction is the only one in the tree; counts moved and reported;
Java/Rust/hadoop byte-stable; gates green.

## 6. Sequencing (operator ruling DEPS-CLASSIFIER-1-SEQUENCING, 2026-09-07)

One relay run cannot carry eight substrate edit sites, four corpus reindexes and a spec-mandated
STOP trigger. Executed as TWO increments, each deep-vertical (user-visible on its own):
- **Increment 1 (DEPS-CLASSIFIER-1)**: §2.1 the ONE shared head reduction (classification crate;
  `normalize.rs` delegates) + the classifier using it; §2.2's Python half (import edge specifier
  agrees with the binding — dotted — or the storage query reads `metadata_json.specifier`); §2.3
  the computed per-row basis and the fixture replacement; §2.4's "also present, below
  materiality" ecosystem line. Outward: django's `deps list` reads `asgiref — used (29 import
  sites, 136 call sites)`; the caveat becomes a basis. Proof: ONE isolated django index (after)
  vs a copy of the retained root (before); kafka + repo-graph byte-stability from copies.
- **Increment 2 (DEPS-CLASSIFIER-1B)**: §2.2's TS half — a bare-package `import` emits an IMPORTS
  edge (unresolved external candidate), `require('x')` literal → binding, `import type` →
  binding with `is_type_only` surviving the storage read — plus the two-root-manifests
  canonical-path collision. The §3 STOP trigger (trust/`check` movement on TS repos) lives HERE
  and is measured on storybook + FRAKTAG before any widening.

CORPUS PATHS: django, storybook, zvec-grep, hadoop at ../legacy-codebases/<name>; repo-graph
is THIS repo.
