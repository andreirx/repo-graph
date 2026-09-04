# CPP-SPAN-FIDELITY-1 — a C++ class is named by its identifier and ends where it ends

Status: SPECIFIED (2026-09-04) · Track: v0.16.0 audit queue #3 (human-ratified). CODE
slice, cpp-extractor, diagnose-then-fix. Maturity: MATURE (every C++ fact consumer
inherits spans and names: find, --text annotation, explain, modules, cycles, map).

## 1. Problem (VERIFIED — two independent audits + operator DB query)

Two defects, one extractor, both field-visible:
- **Macro-decorated class names erased.** `class DLL_LINKAGE CGHeroInstance` is indexed
  as a class named `DLL_LINKAGE` (vcmi `HeroClass.h:18` → `DLL_LINKAGE|CLASS|18-18`);
  770/770 vcmi macro-decorated definitions are unfindable under their real name;
  leveldb `class LEVELDB_EXPORT DB` still lands as `SYMBOL:FUNCTION`. FIND-KIND-MISLABEL-1
  fixed the ROUTING (the file now reaches the cpp extractor) and thereby exposed this
  PARSE defect: a findable wrong label became an unfindable right one.
- **Spans mis-extended, nested definitions swallowed.** leveldb `util/env_posix.cc`:
  `class Limiter` stored as lines 73–806 (true end ≈ 120); the four `Posix*File` classes
  and every method between 130 and 800 have NO node at all. Consequence: `find --text`
  attributes `fsync` at :411 to `[class leveldb::Limiter]` (156/177 C++ hits
  mis-attributed corpus-wide); `WriteOptions` (a struct) renders `[function …]`.

Suspected common mechanism (UNVERIFIED — diagnosis is step 1): tree-sitter-cpp
error-recovery on `class <MACRO> <Name>` and on anonymous-namespace / macro-heavy bodies
produces ERROR nodes whose extents the extractor accepts as class spans, while the real
identifier and nested declarations sit inside the ERROR subtree and are skipped.

## 2. Contract

1. **Diagnose FIRST on the real corpus**: dump the tree-sitter parse for `HeroClass.h:18`
   and `env_posix.cc:73–140`; state which nodes/kinds the extractor consumes and where the
   name/extent go wrong. Evidence in the build report before any fix.
2. **Name fidelity**: for `class|struct [MACRO...] Name [final] [: bases]`, the class name
   is the LAST identifier before the base-clause/body, never a preceding
   attribute/export macro. Macro tokens are recorded (additive metadata) but never become
   the name. Kind: `class`/`struct`/`enum` from the keyword; a struct never renders
   `function`.
3. **Span fidelity**: a class span ends at its body's closing brace; when the parser
   yields an ERROR-wrapped extent, the extractor DOES NOT accept the ERROR node's range —
   it either recovers the true body extent from the balanced-brace region or emits the
   declaration with NO span (visible absence per honesty rules) rather than a swallowing
   span. Nested definitions inside anonymous namespaces / macro-decorated bodies are
   extracted as siblings under their true enclosing scope.
4. **Stable-key transition**: names change → keys change for the affected definitions;
   ONE reindex transition (FIND-KIND-MISLABEL-1 precedent); churn measured and reported
   per corpus repo (vcmi/leveldb/openxcom).
5. **Downstream movement measured (deep-vertical)**: vcmi `find CGHeroInstance` hits the
   class; leveldb `find DB` → SYMBOL:CLASS; `find --text fsync` on leveldb annotates
   `[function leveldb::PosixWritableFile::SyncFd]`; env_posix.cc node count before/after
   (expect ~13 → dozens); C++ --text mis-attribution rate before/after on the audit set.
   Non-C/C++ repos byte-stable.

## 3. Stop conditions

Frozen: storage schema (values/rows change, shape does not), exit codes, other
extractors. Tree-sitter grammar version bump → DECISION_REQUIRED. If true-extent recovery
requires a second parse pass or a preprocessor, STOP + DECISION_REQUIRED with the
smallest-mechanism options (the "no span" honest fallback is always available for the
ERROR case). STANDING HONESTY RULES. Unmet DoD → STOP + DECISION_REQUIRED. Never touch
the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Reproducing fixtures FIRST: (a) `class EXPORT_MACRO Foo {}` → name Foo, kind class,
  correct span; (b) `struct API Bar : Base {}` → Bar/struct; (c) anonymous namespace with
  three classes and methods → all extracted, spans tight; (d) a genuinely unparseable
  region → declaration without span, nothing swallowed.
- Live proof (isolated state root, registry sha unchanged): the §2.5 measurements on
  vcmi/leveldb/openxcom, before/after, verbatim; key-churn counts.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

Macro-decorated C++ types carry their real names and kinds; no span swallows sibling
definitions; ERROR extents never become spans; --text annotation on C++ is correct on the
audit set; churn reported; gates green.

CORPUS PATHS: vcmi at ../legacy-codebases/vcmi; leveldb at ../legacy-codebases/leveldb;
openxcom at ../legacy-codebases/openxcom; repo-graph is THIS repo.
