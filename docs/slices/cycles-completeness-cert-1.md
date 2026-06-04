# CYCLES-COMPLETENESS-CERT-1: certify LiveGraph whole-cycle-graph coverage (without SQLite-per-query)

Slice ID: CYCLES-COMPLETENESS-CERT-1
Status: **RECORDED PREREQUISITE (2026-06-04). NOT a build slice; not yet prioritized.** The blocker that
CYCLES-DEFAULT-MIGRATION-1 deferred onto: a way to certify the LiveGraph covers the WHOLE cycle-relevant
graph for a repo WITHOUT consulting SQLite on every query. Spec stub; ratify scope before any build.
Depends: IMPORTS-XPART-ENUMERATION-1 (whole-repo partition discovery, the deferred F2), the trust/freshness
model (epochs), MODULE-AGGREGATION-1 / MODULE-CYCLES-* (the cycle surface this would unblock). Track: Stage D.

## Why (the gap CYCLES-DEFAULT-MIGRATION-1 hit)
```text
A whole-graph cycle answer cannot be served LiveGraph-first because the LiveGraph CANNOT self-certify
completeness: it is blind to (a) non-TS files (repo-graph's Rust module cycle: SQLite has it, the LiveGraph
never had it as a partition, and `Exact` did not see it) and (b) TS partitions it has not loaded (F2). The
only current completeness check is COMPARE-vs-SQLite EVERY call -- which keeps the SQLite dependency, so it
is not a migration. This slice would build a certification that lets the default serve LiveGraph ONLY when
provably complete, WITHOUT the per-query SQLite compare.
```

## Goal
```text
Produce a per-repo CYCLE-COMPLETENESS CERTIFICATE: a cached, epoch-invalidated fact that says "the LiveGraph's
captured module-cycle graph is COMPLETE for this repo" (or names exactly why it is not). When valid, the
`rmap cycles` default may serve LiveGraph WITHOUT consulting SQLite; when invalid/absent, it falls back to a
labelled SQLite answer (never a silent drop).
```

## Must answer (the certification questions — to spec before building)
```text
1. ENUMERATION: are ALL of the repo's cycle-relevant partitions DISCOVERED + loaded? (needs
   IMPORTS-XPART-ENUMERATION-1 -- whole-repo partition discovery, not the current explicit --source-root.)
2. LANGUAGES: are ALL languages with import / module-cycle semantics ACCOUNTED FOR? (the LiveGraph is
   TS-only; a repo with non-TS import graphs -- Rust, Python, ... -- cannot be certified complete for cycles
   unless those languages are either represented or explicitly excluded-from-scope.)
3. TS IMPORT CLASSES: are all TS import classes either RESOLVED (captured edges) or REPRESENTED as DEGRADED
   evidence (the classifier's PackageExternal/Dynamic/StaticUnresolved)? An unrepresented class = a possible
   hidden cycle -> not certifiable.
4. NON-TS HIDDEN CYCLES: can NO non-TS SQLite module cycle exist UNREPRESENTED -- i.e. either the repo is
   provably TS-only (no non-TS source with import semantics), OR any non-TS import graph forces FALLBACK.
   (This is the exact repo-graph failure: a Rust cycle the TS LiveGraph cannot see.)
5. CACHING / INVALIDATION: the certificate is CACHED and INVALIDATED by repo / partition EPOCH (every
   refresh / swap / re-index busts it) -- so the default never serves a stale "complete" claim.
```

## Outcome that unblocks the migration
```text
ONLY after a VALID certificate exists can `rmap cycles` default become LiveGraph-first WITHOUT
compare-every-call: valid certificate -> serve LiveGraph (no SQLite read); invalid/absent -> labelled SQLite
fallback. THAT is the migration that actually frees the SQLite dependency for cycles (vs the deferred P2,
which kept it).
```

## Out of scope (when/if this is built)
```text
No raw decommission, no deletion, no SQLite removal, no package resolver, no default flip (that is the
SUBSEQUENT CYCLES-DEFAULT-MIGRATION-1 build, gated on this certificate). This slice produces the CERTIFICATE
mechanism only.
```

## Keep (unchanged regardless)
```text
`--engine livegraph --kind module-import`; `--engine compare --kind module-import`; the readiness harness
(scripts/measure-module-cycle-readiness.sh); the SQLite `rmap cycles` default.
```

## References
- `docs/slices/cycles-default-migration-1.md` (the DEFERRED migration that needs this; the P1/P2/P3 analysis)
- `docs/slices/imports-xpart-enumeration-1.md` (F2 whole-repo enumeration -- question 1's prerequisite)
- `docs/slices/module-cycles-default-readiness-1.md` (the YELLOW measurement; the repo-graph non-TS evidence)
- `docs/slices/sqlite-raw-decommission-readiness-4.md` (the broader decommission gate this serves)
