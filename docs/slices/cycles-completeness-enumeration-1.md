# CYCLES-COMPLETENESS-ENUMERATION-1: an accurate expected-partition set + a load plan (no audit mutation)

Slice ID: CYCLES-COMPLETENESS-ENUMERATION-1
Status: **SPEC — awaiting ratification (2026-06-04). NOT started.** Refine the EXPECTED TS partition set so the
module-cycle certificate can advance past `IncompleteMissingPartitions` on real repos, and provide a LOAD PLAN
that feeds the existing refresh WITHOUT the audit ever mutating. Partition-enumeration/baseline-precision
slice; NOT a default migration. NO default flip, NO decommission, NO package resolver, NO daemon self-refresh
inside the audit.
Depends: CYCLES-COMPLETENESS-AUDIT-1 (the read-only audit + `discover_tsconfig_dirs`), IMPORTS-XPART-
ENUMERATION-1 (`run_refresh_multi`, `derive_partition_target`). Track: Stage D, audit/provider precision.

## Goal
```text
The audit works, but read-only over-discovery pins real repos at `IncompleteMissingPartitions`: every
tsconfig (incl. nested TEST FIXTURES) becomes an expected partition, and nothing is loaded. Make the EXPECTED
set accurate (exclude test corpora; keep real partitions) so that, AFTER a SEPARATE load step, the certificate
reaches its true axis (UnsupportedLanguage / ImportClasses / Complete). The audit STAYS READ-ONLY; a separate
listing/refresh produces + loads the set.
```

## Grounding (EXECUTED 2026-06-04) — refutes the package-root lean
```text
repo-graph discovered 10 tsconfig dirs. package.json co-location (the obvious "package root" signal) does NOT
discriminate fixtures from real partitions:
  PKG + REAL:     . (root, whole-repo tsconfig), tools/rgistr
  PKG + FIXTURE:  test/fixtures/typescript/classifier-repo, .../rust-7a-fixture,
                  test/fixtures/typescript/monorepo-packages/packages/ui|api, rust/.../tests/fixtures/synthetic
  NO-PKG FIXTURE: test/fixtures/typescript/receiver-types
  NO-PKG REAL(!): rust/.../tests/fixtures/xpart-monorepo/packages/a|b  <- the Complete-proof packages have NO
                  package.json; a "require package.json" rule would EXCLUDE exactly what we need.
Root package.json has NO `workspaces` field -> repo-graph is a RUST workspace, no npm/pnpm/lerna manifest to
enumerate package roots from. => "package/workspace roots only" (the expected lean) is UNWORKABLE for this set.
The ONLY clean discriminator: PATH. Every over-discovered culprit lives under a `fixtures` path segment;
the real ones (`default`, `tools/rgistr`) do not.
```

## THE TRUST ASYMMETRY (must be on the table before ratifying)
```text
Over-INCLUDE a fixture as expected -> the cert needs it loaded -> IncompleteMissingPartitions = SAFE (just
  over-conservative; the current behaviour).
Wrongly EXCLUDE a REAL partition from expected -> the cert no longer requires it -> it can reach `Complete`
  while a real partition (maybe with a cycle) is unloaded = FALSE COMPLETE = a false trust claim.
=> Exclusion is the DANGEROUS direction. The policy MUST be conservative + policy-backed + REPORTED, and must
   ERR TOWARD INCLUSION when unsure. This is the load-bearing decision of the slice.
```

## Forced decisions (to ratify at sign-off) — every cell filled

### D1 — discovery rule
```text
A. all tsconfig recursively (current).                     SAFE but over-discovers -> stuck at Missing.
B. package/workspace roots only.                           REFUTED by grounding (no manifest; package.json
                                                           non-discriminating; excludes the xpart packages).
C. explicit manifest/config.                               No manifest exists; premature.
D. all tsconfig + POLICY-BACKED fixture-path EXCLUSION.    [RECOMMENDED] keep A's discovery, subtract dirs under
                                                           a known test-corpus segment.
RECOMMENDATION: D = A ∩ (not a fixture path). Exclude a discovered dir iff a repo-relative path SEGMENT is one
of a CLOSED, policy-backed set: {`fixtures`, `__fixtures__`, `__tests__`, `__mocks__`, `testdata`}. This
catches all 10->8 repo-graph fixtures (each under a `fixtures` segment) while keeping `default` + `tools/rgistr`,
and (auditing xpart AS its own repo) keeps `packages/a|b` (no fixture segment relative to the fixture root).
NOT bare `test`/`tests` segments (too broad -> could drop real `src/test` code -> false Complete).
```

### D2 — baseline producer (who computes/loads the set; NOT the audit)
```text
1. audit computes expected set only.                       Already true (the report's expected_partition_ids).
2. separate `livegraph-refresh --all-discovered`.          [RECOMMENDED] the MUTATION lives in refresh.
3. generated manifest checked into `.rgr`.                 Durable; premature (no cache ratified).
4. manual explicit roots.                                  The status quo (--source-root); the user's escape hatch.
RECOMMENDATION: (1)+(2). The AUDIT keeps reporting the refined expected set (read-only). A NEW mode
`rmap dev livegraph-refresh --all-discovered` runs the SHARED refined discovery -> run_refresh_multi over the
non-fixture roots. The mutation is in REFRESH (its job), never the audit. The refined discovery is a SHARED
function used by BOTH (audit = expected; refresh = load plan) so they cannot drift.
```

### D3 — fixture/test handling
```text
A. nested test fixtures included as real partitions.       The current over-discovery.
B. excluded by default.                                    [RECOMMENDED]
C. require explicit opt-in to include.
RECOMMENDATION: B -- exclude test fixtures from REPO-LEVEL completeness by the D1 path policy. KEY: the
exclusion is RELATIVE to the audited repo root, so when the repo ITSELF is a fixture (auditing xpart-monorepo
directly), its `packages/a|b` are NOT under a fixture segment -> INCLUDED. "Exclude fixtures unless the repo is
a fixture root" falls out of relative-path matching for free. An explicit `--include-fixtures` opt-in (C) is a
cheap escape hatch for "I really do want to certify the test corpus".
```

### D4 — validation (EXECUTED later)
```text
- xpart fixture (own repo): expected {packages/a, packages/b} (fixture segment not present relative to root);
  load both -> CompleteForModuleImportCycles (unchanged from AUDIT-1).
- repo-graph: refined expected EXCLUDES the 8 nested fixtures -> {default, tools/rgistr} (or as measured);
  `--all-discovered` loads them -> audit advances PAST MissingPartitions -> IncompleteUnsupportedLanguage
  (Rust present) [or ImportClasses if a pure-TS subset]. repo-graph no longer over-discovers fixtures unless
  `--include-fixtures`.
- a real pure-TS repo (amodx): refined expected = its real packages; `--all-discovered` loads them -> past
  Missing -> IncompleteImportClasses (package imports) OR Complete -- the import-class axis, finally LIVE.
- the audit performs NO refresh in any case (read-only invariant); the report lists what was EXCLUDED.
- full gate.
```

## Out of scope (hard guardrails)
```text
NO default flip, NO decommission, NO package resolver, NO durable manifest/cache (D2-3 deferred), NO daemon
self-refresh INSIDE the audit (the read-only invariant). The exclusion policy is a CLOSED segment set -- no
heuristic "project root" guessing (rejected in AUDIT-1).
```

## Key invariant (RATIFIED by the brief)
```text
The audit remains READ-ONLY. Enumeration/listing MAY feed refresh; the audit NEVER triggers refresh. The
refined discovery is SHARED between the (read-only) audit and the (mutating) refresh so expected == load-plan.
```

## Build contract (PROPOSED — gated on ratification)
```text
1. a SHARED refined discovery: `discover_partition_roots(repo_root) -> {included: Vec, excluded: Vec<(dir,
   reason)>}` = all-tsconfig walk (reuse AUDIT-1's) MINUS the D1 fixture-segment set. Pure + unit-tested.
2. audit: expected_partition_ids uses `included`; the report adds `excluded_fixture_partitions` (transparency).
3. `rmap dev livegraph-refresh --all-discovered`: shared discovery -> run_refresh_multi(included roots). Mutation
   in refresh only.
4. (optional) `--include-fixtures` opt-in on both, for fixture-root certification.
5. docs: completion + the refined repo-graph expected set + a LIVE run showing repo-graph/amodx advancing past
   MissingPartitions after `--all-discovered`. Explicit "still not a migration".
```

## Follow-up
```text
- the daemon RUNTIME wiring (cache the BaselineInput) + CYCLES-DEFAULT-MIGRATION-1 (un-deferred).
- IMPORTS-PACKAGE-RESOLUTION-1: once repos advance to ImportClasses, the lever to reach Complete.
- CYCLES-COMPLETENESS-LANGUAGE-PRECISION-1: the A' import-bearing non-TS refinement, if A over-blocks.
```

## References
- `rust/crates/daemon-runtime/src/cycle_completeness_audit.rs` (`discover_tsconfig_dirs` to refine + share)
- `rust/crates/daemon-runtime/src/livegraph_refresh.rs` (`run_refresh_multi`, `derive_partition_target`)
- `docs/slices/cycles-completeness-audit-1.md` (the read-only audit + the over-discovery evidence)
- `docs/slices/imports-xpart-enumeration-1.md` (the explicit --source-root precedent)
