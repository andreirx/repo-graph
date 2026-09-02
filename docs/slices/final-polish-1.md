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
