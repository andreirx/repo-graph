# XPART-FIXTURE-STANDALONE-1: the cross-partition fixture validates as its OWN repo

Slice ID: XPART-FIXTURE-STANDALONE-1
Status: **IMPLEMENTED + LIVE-VALIDATED (2026-06-02).** Harness-only, ZERO code change. `rmap index
<fixture>` registers the two-package fixture as its own repo; live validation now reports the fixture's
repo_uid/display_name, not the enclosing repo-graph. See **Completion**.
Reason: validation HYGIENE (O1 from IMPORTS-XPART-ENUMERATION-1) — the committed fixture lives UNDER
repo-graph, so before registration it resolved to the enclosing repo-graph registration (cosmetic
repo_uid/display_name impurity; keys/overlay were always correct). Fixing it prevents future evidence
ambiguity. Track: Stage D, fixture/test-harness ONLY.

## Scope (hard guardrails)
```text
Fixture / test-harness ONLY. No graph/runtime/trust changes. No CLI behaviour changes. No module
aggregation. No raw decommission. PRODUCTION KEY CONSTRUCTION UNCHANGED (the keys were already correct;
this slice only changes WHICH repo identity the live validation runs under).
```

## Grounding (EXECUTED 2026-06-02) — why no code change is needed
```text
repo identity is a REGISTRY, not a git walk (registry.rs):
- rmap index <path> registers the CANONICAL path AS-IS (no `.git` walk; no `.git` required). (index.rs;
  scanner.rs "no .git requirement")
- repo_uid = a random ULID minted on first registration of a path; idempotent per path
  (registry.rs generate_repo_uid / RegistryEntry::new).
- resolve(path) = EXACT match, else the LONGEST registered ANCESTOR prefix (registry.rs resolve()).
- display_name = the registered ALIAS (else the uid).
=> O1's cause: the fixture was NEVER registered, so the only ancestor was repo-graph -> it resolved to
   repo-graph. REGISTERING the fixture (`rmap index <fixture>`) makes <fixture> a longer prefix than
   /repo-graph, so it WINS resolution -> the fixture gets its own identity. NO runtime change; NO fixture
   MOVE needed (location does not affect identity — registration does).
```

## Ratified approach
```text
Add a validation HARNESS (scripts/validate-xpart-fixture.sh) that:
  1. `rmap index <fixture> --alias xpart-monorepo`  (register the fixture as its own repo; readable name)
  2. `rmap dev livegraph-refresh --repo <fixture> --source-root <fixture>/packages/a
     --source-root <fixture>/packages/b`            (the producer path + repeated --source-root, absolute)
  3. `rmap cycles --engine livegraph --kind file-import --json` from the fixture cwd, and ASSERT:
       display_name == "xpart-monorepo" (NOT "repo-graph"); cross_partition == true; xpart_edge_count == 2;
       the a/main + b/foo FILE nodes present.
  4. the human render says "FILE import cycle" and never "module".
The committed fixture STAYS where it is (under repo-graph-scip-ingest/tests/fixtures); the harness gives it
a distinct identity at validation time. Re-runnable (index/refresh are idempotent per path).
```

## Requirements outcomes
```text
1. fixture treated as its own repo by rmap                 DONE (rmap index registers it; longest-prefix wins).
2. live repo_uid/display_name belong to the fixture        DONE (repo_01kt4r0z..., display_name xpart-monorepo).
3. cross-partition cycle preserved (a/main, b/foo, edge=2, cross_partition=true)  DONE (live).
4. producer path + repeated --source-root validation kept  DONE (the harness uses both).
5. update docs that referenced the old fixture path/behaviour  DONE (this doc + ENUMERATION-1 O1 resolved).
6. production key construction unchanged                   DONE (no code touched).
```

## Acceptance (EXECUTED 2026-06-02, live)
```text
- rmap index <fixture> --alias xpart-monorepo -> repo_01kt4r0zm67mjb3a5hvzm409a5 (a NEW identity, distinct
  from repo-graph's repo_01ks2...), snapshot minted.                                              PASS
- rmap dev livegraph-refresh --repo <fixture> --source-root <fixture>/packages/a
  --source-root <fixture>/packages/b -> AllRefreshed (both partitions).                           PASS
- rmap cycles --engine livegraph --kind file-import --json -> repo_uid repo_01kt4r0z...,
  display_name "xpart-monorepo" (NOT repo-graph); node keys carry the fixture uid; cross_partition
  true; xpart_edge_count 2; cycle a/main <-> b/foo.                                                PASS
- human render: "Cycles: xpart-monorepo" / "1 FILE import cycle found" / "Cycle 1 (2 files):".     PASS
```

## Out of scope
```text
No fixture MOVE (registration, not location, decides identity). No runtime/registry/resolution change. No
auto-registration of the fixture (the harness indexes it explicitly). No change to the RENDER (FILE
vocabulary already landed in CYCLES-FILE-IMPORT-RENDER-1). The repo-graph self-index would still see the
fixture's TS files (the fixture is committed in-repo) — out of scope + harmless (separate identity).
```

## References
- `rust/crates/daemon-runtime/src/registry.rs` (`resolve` longest-prefix; `generate_repo_uid`; `register`)
- `rust/crates/rgr/src/commands/index.rs` (`rmap index` — registers the path as-is)
- `scripts/validate-xpart-fixture.sh` (the harness this slice adds)
- `docs/slices/imports-xpart-enumeration-1.md` (O1 — the impurity this resolves)
- `rust/crates/repo-graph-scip-ingest/tests/fixtures/xpart-monorepo/` (the fixture)
