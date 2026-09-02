# FINAL-POLISH-1 — three small honesty fixes: surfaces dedup, deps self-import, resource claim

Status: SPECIFIED (2026-09-02) · Track: queue tail bundle (each item sentence-to-function
level). CODE slice, small. Maturity: MATURE.

## 1. Problems (measured)

1. **SURFACES-DEDUP**: amodx surfaces-list prints 46 VERBATIM-identical rows ("GET <dynamic —
   …> tools/mcp-server/src/index.ts [consumer]") — boundaries-list already collapses the same
   file to "×46". Also: glamCRM's boundaries-list counts "+2 test-only surfaces" in GROUPS
   while boundaries-summary counts the same set as "10 surfaces" — unit labels disagree.
2. **DEPS-SELF**: django deps renders "undeclared: django" — the package importing itself
   counted as an undeclared external. A package's own name (from its parsed manifest — the
   fact, not the directory name) is first-party, TRUST-FIRSTPARTY-1's cousin in deps.
3. **RESOURCE-CPP-INERT (claim narrowing)**: resource-list's coverage line says "covers C,
   C++ …" while the detector sees only fopen-style calls — std::ofstream/ifstream invisible,
   so file-driven engines read "0 reads". Narrow the CLAIM to the detected mechanism
   ("fopen-family calls"; per-language mechanism naming from the detector registry — the
   line must describe what the detector DOES, not the language it parses). Teaching the
   detector streams is a NAMED FOLLOW-UP (RESOURCE-STREAMS-1, its own slice), not this one.

## 2. Contract

1. surfaces-list collapses identical (method, route, file, role) rows to one row ×N (the
   boundaries pattern; JSON keeps every row); unit labels: both boundary surfaces say
   "surfaces" and "groups" with the same meanings (pick the existing summary vocabulary,
   align list).
2. deps: a used/undeclared candidate equal to a parsed manifest's OWN package name renders
   as first-party self-reference (excluded from undeclared; noted "self" if rendered at
   all) — manifest-name fact only, never directory names.
3. resource coverage line names the MECHANISM per language from the registry; the follow-up
   is recorded in the spec doc.
4. JSON additive; exit codes unchanged.

## 3. Stop conditions

Frozen: detection/attribution computation (render + classification-by-manifest-fact only),
storage schema, exit codes, trust. STANDING HONESTY RULES. New public APIs beyond additive
DTO fields → DECISION_REQUIRED (read-only precedent citable). Unmet DoD → STOP +
DECISION_REQUIRED. Never touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: dedup identity + ×N; unit-label alignment; self-reference classification (manifest
  fact; a directory coincidentally named like a package does NOT trigger it); mechanism
  naming.
- Live proof (isolated state root, registry sha unchanged): amodx surfaces ≤ a screen with
  ×46; django deps free of "undeclared: django"; leveldb/OpenXcom resource line names
  fopen-family. Captures.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

No verbatim row walls; no self-imports as externals; no coverage claim wider than its
mechanism; unit labels agree; RESOURCE-STREAMS-1 recorded; gates green.

## 6. Measurement correction (build-2, 2026-09-02) — §1.3 premise refuted, follow-up re-scoped

§1.3 stated as measured that "std::ofstream/ifstream [are] invisible". **End-to-end measurement
(operator ruling 2026-09-02 answering review-1) refutes that premise.** Two isolated indexes on
this build, each `index` → `resource list`:

- **OpenXcom** (`../legacy-codebases/OpenXcom`) → `resource list`: **1 resource** (`opllog.opl`, a
  write). `SavedGame.cpp:567/696` (`std::ofstream tmp(tmpPath.c_str())` / `sav(savPath.c_str())`)
  do **NOT** appear — the operator's branch (b) trigger.
- **Purpose-built C++ fixture** (three writes) → `resource list`: `std::ofstream out("literal_stream.log")`
  (string-literal path) → **DELIVERED** as an FS_PATH write; `std::ofstream sav(p.c_str())`
  (non-literal) → **DROPPED**; `fopen("literal_fopen.txt","w")` → DELIVERED.

**Conclusion:** `std::fstream` IS counted end-to-end when the path is a **string literal** (so the
"not yet counted" claim was false — neither branch premise held; the coverage sentence follows the
measurement per the operator's overriding rule). The C++ coverage line now reads
`fopen/open/sqlite3 and std::fstream calls`.

**RESOURCE-STREAMS-1 re-scoped** (name now a misnomer — recommend renaming to
`RESOURCE-DYNAMIC-PATH-1`; not renamed here to avoid touching a cross-referenced id): the real,
**measured** gap is that **only string-literal path arguments resolve to resource keys**. A computed
path (`.c_str()`, a variable, any non-literal arg0) is dropped at the extractor's arg0 gate
(`cpp-extractor/src/extractor.rs` `extract_arg0_string_literal`: "Not a string literal → dynamic
path, skip"; the `.open()`/constructor paths share it). This is **cross-detector and cross-language**
(fopen/open/Python `open()`/Node `fs` all require a literal arg0), **not fstream-specific** — hence
it is deliberately not carried in any per-language call-family string. It is why file-driven engines
(OpenXcom) under-report. Locating = done (above); resolving non-literal paths = the follow-up slice.
