# CPP-DECLARATORS-1 — a C/C++ function is named by its declarator's identifier; a forward declaration is not a definition

Status: SPECIFIED (2026-09-06) · Track: audit round five, group C (human-ratified 2026-09-06:
"we have root causes, we find a way to address them"). CODE slice, c-extractor + cpp-extractor +
find ranking + resolver + seed classify. Maturity: MATURE (every C/C++ fact consumer inherits
names and identity: orient complexity, find, explain/callers, IMPLEMENTS/INSTANTIATES edges,
seeds).

## 1. Problem (ROOT-CAUSED — `docs/audits/2026-09-06-root-causes-v0.17.0.md` §C, code-cited)

Two defects, one extractor family, both field-visible on v0.17.0:

- **D3 — macro-wrapped function names shipped as the macro token.** hadoop's #3 complexity
  center is `uriparser/UriRecompose.c:87 — URI_FUNC (cx 102)`; the source is
  `static URI_INLINE int URI_FUNC(ToStringEngine)(URI_CHAR * dest, …)`. 25 rows across
  hadoop (12× `URI_FUNC`), poco (9× `PRIV`, 3× `PREFIX`), duckdb (1×
  `ZSTD_ALLOW_POINTER_OVERFLOW_ATTR`); the whole uriparser/pcre2/expat corpora (90 + 32 + 28
  definitions) are stored under the macro name and collide into `:dupN` keys.
  Cause (verified against the real files with the pinned tree-sitter 0.23.4): both
  `extract_function_name`s (`c-extractor/src/extractor.rs:449-472`,
  `cpp-extractor/src/extractor.rs:1684-1730`) unwrap nested `function_declarator`s and return
  the outer identifier. Two syntactic shapes:
  - Shape 1 `M(name)(args)`: tree-sitter parses `URI_FUNC(ToStringEngine)` as an INNER
    `function_declarator` whose single "parameter" is a bare `type_identifier` — the real
    name. The loop unwraps both and lands on `URI_FUNC`.
  - Shape 2 attribute macro(s) before the name (C++, zstd):
    `function_declarator(declarator: identifier "ZSTD_ALLOW_POINTER_OVERFLOW_ATTR",
    ERROR(identifier "U32", identifier "ZSTD_insertBtAndGetAllMatches"), parameters)` — the
    real name is the last identifier of an ERROR child before `parameters`; the code never
    inspects the ERROR.
  CPP-SPAN-FIDELITY-1 fixed the TYPE path only (`walk_top_level` :487-493 → `extract_type`
  → `type_name_and_macros`); functions fall through to the untouched walk. Never worked
  (blame: the original extractors e0fa892 / b96635a).
- **D4 — forward declarations are unmarked and outrank definitions; they also DROP edges.**
  `find CGHeroInstance` (vcmi) shows 7 of 8 visible `[CLASS]` rows that are
  `class CGHeroInstance;` (70 repo-wide); the definition `lib/mapObjects/CGHeroInstance.h:55`
  is off-screen (rows tie through evidence rank, `path ASC` decides). Cause: `extract_type`
  (:682-721) pushes a bodiless `class_specifier` unconditionally (spec CPP-SPAN-FIDELITY-1
  §2.3 keeps them visible — correct) with NOTHING marking decl vs definition; `nodes` has no
  decl column; `is_decl` exists only on `seed_vectors` and only for callables
  (`classify.rs:719-726`). Bigger than find's sort: `resolve_symbol`
  (`storage/src/queries.rs:653-691`) sees 71 identical qualified names → AmbiguousSymbol
  (`explain CGHeroInstance` prints all 71), and `pick_unambiguous`
  (`indexer/src/resolver.rs:610-634`) returns None when >1 candidate survives, so EVERY
  `: public CGHeroInstance` IMPLEMENTS/INSTANTIATES edge is dropped as unresolved.

## 2. Contract

1. **Function name fidelity (both extractors).** In `extract_function_name`:
   (a) a `function_declarator` whose own `declarator` is a `function_declarator` with exactly
   one `parameter_declaration` carrying a bare `type_identifier` and no declarator → the name
   is that type_identifier; the outer identifier is recorded under the additive
   `metadata_json.macro_tokens` (the key the class path already uses, `:1133-1150`);
   (b) after the unwrap, an `ERROR` child of the `function_declarator` immediately preceding
   `parameters` → the name is its LAST identifier; earlier identifiers → `macro_tokens`.
   Both are unambiguous: a function returning a function is invalid C/C++, and
   function-pointer returns always go through `parenthesized_declarator`
   (negative fixture `int (*getHandler(int x))(double)` → `getHandler`, unchanged).
   `signature` derives from the corrected name automatically.
2. **Forward declaration is a stored fact.** On the `BodyProbe::ForwardDecl` / `None` paths
   of `type_span_and_body_close`, the emitted type node carries additive
   `metadata_json {"forward_decl": true}`. Definitions carry nothing new. Schema shape
   unchanged.
3. **Definition first, everywhere identity is chosen.**
   - find Facts: `find_fact_symbols` selects `metadata_json`; `rank_key`
     (`daemon-runtime/src/find_facts/rank.rs:166-180`) gains a `decl_rank` between the
     evidence rank and `qname_len` — definitions before forward declarations, all else equal;
     forward-decl rows render `(decl)` (the tag seeds already use).
   - seeds: `classify::is_declaration` consults `forward_decl` so a type chunk is `(decl)` and
     demotes under its definition exactly as callables do (SEED-CHUNK-2 mechanism).
   - resolution: `resolve_symbol` step 2/3 and `pick_unambiguous` prefer the unique
     non-forward-decl candidate when exactly ONE exists among same-name candidates;
     >1 non-decl candidates stay ambiguous (honest). This restores the inheritance edges.
4. **Stable-key transition**: names change for the affected functions → ONE reindex
   transition (FIND-KIND-MISLABEL-1 / CPP-SPAN-FIDELITY-1 precedent); churn counted and
   reported per corpus repo.
5. **Downstream movement measured (deep-vertical).** Before/after, verbatim:
   hadoop `orient --budget large` complexity row reads `UriRecompose.c:87 — ToStringEngine
   (cx 102)` and `find ToStringEngine` hits; duckdb `find ZSTD_insertBtAndGetAllMatches`
   hits; poco `find check_escape` / `find prologTok` hit; count of `[A-Z_]{4,}` macro-named
   complexity rows on the hadoop/poco/duckdb orient outputs → 0. vcmi `find CGHeroInstance`
   shows `lib/mapObjects/CGHeroInstance.h:55` FIRST among CLASS rows with the 70 others
   tagged `(decl)`; `explain CGHeroInstance` resolves (no 71-row ambiguity); vcmi
   IMPLEMENTS/INSTANTIATES resolved-edge count before/after (expect a large rise — state
   the number). Non-C/C++ repos byte-stable (repo-graph, FRAKTAG, django orient).

## 3. Stop conditions

Frozen: storage schema SHAPE (metadata_json keys are additive; no new columns), exit codes,
wire protocol, non-C/C++ extractors. Tree-sitter grammar version bump → DECISION_REQUIRED.
If a THIRD declarator shape appears in the corpus that (a)/(b) do not cover, record it with
the parse tree and STOP + DECISION_REQUIRED — do not add a preprocessor or a heuristic
beyond the two proven shapes. If resolver preference needs more than "the unique non-decl
candidate wins", STOP + DECISION_REQUIRED (ambiguity stays honest). STANDING HONESTY RULES.
Unmet DoD → STOP + DECISION_REQUIRED. Never touch the operator's real state root. Do NOT
commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Reproducing fixtures FIRST, verbatim from the corpus, in the inline test modules
  (`c-extractor` :978-1031 `extract_ok`; `cpp-extractor` :2379-2444 `sym`):
  `static URI_INLINE int URI_FUNC(ToStringEngine)(URI_CHAR * dest) {…}` → `ToStringEngine`,
  macro_tokens `["URI_FUNC"]`; `int\nPRIV(check_escape)(int *p)\n{…}`;
  `static int PTRCALL\nPREFIX(prologTok)(int x) {…}`; the four-line zstd header →
  `ZSTD_insertBtAndGetAllMatches` (never `FORCE_INLINE_TEMPLATE` /
  `ZSTD_ALLOW_POINTER_OVERFLOW_ATTR`); negative `int (*getHandler(int x))(double)` →
  `getHandler`; `class Foo;\nclass Foo { int x; };` → two nodes, only the first carries
  `forward_decl`.
- rank.rs rule test `definition_beats_forward_decl`; resolver test: one definition + N
  forward decls → the IMPLEMENTS edge resolves to the definition; two definitions → still
  ambiguous.
- Live proof (isolated state root — ALL rmap calls isolated; registry sha identical
  before/after): the §2.5 measurements on hadoop, vcmi, duckdb, poco; key-churn counts;
  hadoop is the proof corpus for D3 (uriparser is small — do NOT rebuild the whole hadoop
  tree twice if an isolated index of `hadoop-hdfs-project/…/uriparser`-bearing subtree
  suffices; state what was indexed).
- Gates recorded FIRST (`build-progress.md`), then the live proof; chunked cargo gates;
  consolidation witness; dogfood-isolated green.

## 5. Definition of done

Macro-wrapped C/C++ functions carry their real names (both shapes; zero macro-named
complexity rows on the audit set); forward declarations are stored as such, render `(decl)`,
rank below their definition in find and seeds, and no longer make a many-times-declared
class ambiguous or drop its inheritance edges; churn reported; non-C/C++ byte-stable; gates
green.

CORPUS PATHS: hadoop, vcmi, duckdb, poco, leveldb at ../legacy-codebases/<name>; repo-graph
is THIS repo.
