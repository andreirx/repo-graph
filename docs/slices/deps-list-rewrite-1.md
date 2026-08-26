# DEPS-LIST-REWRITE-1 — dependency facts an agent can trust, in a screen

Status: SPECIFIED (2026-08-26) · Track: Usefulness-audit fix queue #2
(`docs/ROADMAP.md` § Usefulness audit v0.9.0; evidence `smoke-runs/2026-08-25T22-41-37Z`).
CODE slice. Maturity: MATURE surface.

## 1. Problem (measured — the audit's worst single command, F on 5/6 repos)

- **Fragments and builtins reported as packages** (OBSERVED): FRAKTAG lists
  `"Object.values(allNodes)\n .filter(...).sort"` as an `observed_but_undeclared` PACKAGE; the
  buckets are full of `Map`, `Set`, `Promise`, `Math.sqrt` (TS) and `StringBuilder`,
  `IllegalArgumentException`, `java.util` (Java), `AssertionError`, `asyncio` (Python) —
  call-expression text hoisted into the package namespace.
- **Wrong ecosystem** (OBSERVED): django reports `ecosystem: npm`, reads `package.json`
  (biome/grunt/puppeteer), and its REAL `pyproject.toml` deps (`asgiref`, `sqlparse`, `tzdata`)
  never appear. petclinic fabricates `manifest_path: "package.json"` — a file that does not exist
  on a Maven/Gradle repo.
- **False coverage claims** (OBSERVED): glamCRM returns `results: [], count: 0` beside
  `total_external_imports: 4336` with `modules_without_manifest_context: 0` — "full coverage,
  nothing found". amodx covers 9 of 43 `package.json` while claiming the same 0.
- **Contradiction with resolution state** (OBSERVED): `@fraktag/engine` reported
  `declared_but_unobserved` at confidence 1.0 while `server.ts:6` imports it — the alias failure
  `trust` already names as `alias_resolution_suspicion`, restated as certainty.
- **Dumps**: 337–2,008 lines of JSON, ~7 lines/entry, `confidence: 1.0` on every row; leveldb's
  one honest reader-context line is line 632 of 632.

## 2. Contract

1. **The `observed_but_undeclared` bucket accepts ONLY import-specifier strings** — values that
   came from an import/require/use/include declaration. Never call-expression text; anything with
   `(`, whitespace, or a method chain is rejected at the source. Language builtin/stdlib
   name-sets (the existing `runtime_builtins` machinery, currently under-used) classify builtins
   as builtins, never as undeclared packages.
2. **Manifest selection by DOMINANT INDEXED LANGUAGE** (files-table counts): Python-plurality →
   `pyproject.toml` `[project].dependencies` (+ optional `requirements*.txt` as a named
   extension point, not built); Java-plurality → the existing Gradle reader (+ `pom.xml` named
   as extension point if absent); JS/TS → `package.json` (all workspace manifests in scope — the
   9-of-43 amodx gap closes or the shortfall is REPORTED). `manifest_path` is always the file
   actually parsed; if none was parsed: `manifest_scope_available: false` + a loud line — never
   a fabricated default and never `modules_without_manifest_context: 0`.
3. **Unattributed imports are a HEADLINE, not a footer**: when external imports exist that no
   manifest attributes, the FIRST line (human and JSON) states "N external imports unattributed
   (<reason: no manifest reader for X / manifests not matched>)". leveldb's honest
   reader-context line moves from position 632 to position 1.
4. **Resolution-state honesty**: when `trust`'s downgrades (alias_resolution_suspicion,
   workspace-package-as-library) are active for the snapshot, `declared_but_unobserved` renders
   as `declared — imports not resolved on this index` with capped confidence, never certainty.
5. **The default output is a ≤20-line human table**: per manifest — declared+used (count),
   declared-unobserved (count + the honesty label), observed-undeclared (count, specifier-only),
   builtins (count) — with top examples, totals, and the drill-down flag (`--json` keeps the full
   machine form, same truth, same headline). Existing JSON fields stay additive-compatible.

## 3. Stop conditions

Frozen: storage schema, trust computation, witness/union/reconciliation, exit codes. New manifest
READERS beyond the listed set (pom.xml, requirements.txt) are extension points, not scope. If the
specifier-only rule requires touching per-language extractors' emitted data (rather than
filtering at the deps assembly), STOP + DECISION_REQUIRED. Never touch the operator's real state
root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: specifier-only rejection matrix (fragment/chain/whitespace/builtin per language);
  manifest-by-dominant-language; fabricated-path impossible (no manifest → the honest flag);
  headline-on-unattributed; downgrade-capped confidence.
- LIVE isolated proofs: django — ecosystem python, `asgiref`/`sqlparse`/`tzdata` appear,
  builtins bucket ≥ the old undeclared junk, npm junk gone; petclinic — no `package.json` claim,
  Gradle deps attributed, `SpringApplication` not double-counted; glamCRM — no `count: 0` claim
  (either Gradle+npm attribution or the loud unattributed headline); FRAKTAG — zero
  call-expression "packages", `@fraktag/engine` rendered with the resolution-honesty label;
  leveldb — reader-context line FIRST. Before/after line counts in the report (target: default
  human output ≤20 lines/repo).
- Chunked cargo gates; witness green; dogfood green; logged smoke SMOKE_ONLY="django spring-petclinic" green.

## 5. Definition of done

An agent reading `rmap deps list` sees, in one screen, what the repo declares, what it actually
uses, what is unattributed and WHY — with no fabricated files, no builtins-as-packages, no
false-coverage zeros; gates green.
