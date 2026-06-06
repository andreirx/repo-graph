# CYCLES-DEFAULT-MIGRATION-READINESS-2: re-measure default readiness post import-classification

Slice ID: CYCLES-DEFAULT-MIGRATION-READINESS-2
Status: **MEASURED — YELLOW; awaiting the migration-model ratification (2026-06-06).** The certificate is a
PROVEN-SAFE predicate (0 extra; no Complete-with-missing across xpart/amodx/hexmanos/zap-engine/repo-graph) ->
model A (LiveGraph-iff-Complete else labelled SQLite fallback) is BUILDABLE + safe; but ONLY xpart reaches
Complete, so the benefit is fixture-only today (real repos fall back). See **Findings** + **Verdict**.
Recompute `rmap cycles` default-migration
readiness now that import CLASSIFICATION is complete (every import class precise; the residual is isolated to
`WorkspaceLocalUnedgeable`). Decide the migration MODEL from a fresh readiness histogram. NO default flip in this
spec, NO raw decommission, NO workspace package edge, NO heuristic source target, NO deletion.
Depends: CYCLES-COMPLETENESS-CERT-1 / -AUDIT-1 / -ENUMERATION-1 + the six import slices (the certificate this
measures), the `--engine compare` module-cycle diff (READINESS-1). Supersedes the YELLOW of
`module-cycles-default-readiness-1.md` with the post-classification picture. Track: Stage D.

## Goal
```text
READINESS-1 (2026-06-03) was YELLOW: the LiveGraph module cycles were Exact vs SQLite on the TS repos, but the
whole-graph completeness story was not certifiable. Since then the completeness CERTIFICATE exists and EVERY
import class is classified -- so a repo's IncompleteImportClasses now decomposes into named, honest causes, and
amodx's SOLE remaining blocker is WorkspaceLocalUnedgeable (RED, the src-vs-dist moniker chasm). Re-measure the
readiness histogram + the SQLite-vs-LiveGraph compare across the repo set, and decide whether the DEFAULT can
move to "LiveGraph when the certificate permits, else a LABELLED SQLite fallback" -- or stays SQLite.
```

## Forced decisions (to ratify AFTER the measurement) — every cell filled

### D1 — treatment of `WorkspaceLocalUnedgeable`
```text
A. HARD BLOCKER: a repo whose certificate is Incomplete (incl. WorkspaceLocalUnedgeable) -> the default MUST
   serve the LABELLED SQLite fallback (never a LiveGraph answer that could miss a workspace-edge cycle). The
   conservative, trust-safe reading: an unedgeable workspace import is a POTENTIAL missing module-cycle edge
   the LiveGraph cannot see (exactly the RED 1A finding). [RECOMMENDED]
B. LABELLED DEGRADATION: the LiveGraph default MAY serve a `Partial` answer (omitting any cycle that would only
   exist via a workspace edge) IF the degradation is LOUD + agreed. Risk: a user reads fewer cycles than SQLite.
C. IGNORED for module cycles: treat WorkspaceLocalUnedgeable as non-cycle-relevant. REJECT unless PROVEN that no
   workspace edge ever closes a module cycle the SQLite graph has -- the compare (below) is the only evidence,
   and it cannot prove a global negative.
RECOMMENDATION: A. WorkspaceLocalUnedgeable is a KNOWN-incomplete signal; the certificate already says
permits_livegraph_default=false for it. The default fallback to SQLite preserves completeness.
```

### D2 — the migration model
```text
A. AUTO: serve LiveGraph WHEN the certificate is `CompleteForModuleImportCycles` (permits_livegraph_default);
   ELSE a LABELLED SQLite fallback. Exact-only LiveGraph default. [RECOMMENDED -- the certificate is exactly
   the predicate the deferred CYCLES-DEFAULT-MIGRATION-1 needed.]
B. LiveGraph default with a Partial/degraded user-visible output on Incomplete. Needs VERY strong UI/trust
   surfacing (the answer differs from SQLite); high risk of a silent under-count. [REJECT unless surfaced.]
C. STATUS QUO: SQLite default unchanged. [the safe fallback if A's benefit is judged insufficient.]
RECOMMENDATION: A or C (per the brief). A's value: a repo whose certificate is Complete (pure-workspace-edge-
free, all-loaded, TS-only) serves the LiveGraph WITHOUT consulting SQLite -- the first real decommission step.
C if NO measured repo reaches Complete (then A buys only a relabel today).
```

### D3 — the decision rule (the user's #4)
```text
- If ANY repo has WorkspaceLocalUnedgeable AND the compare shows MISSING SQLite module cycles in the LiveGraph
  -> the default MUST fall back (proof a workspace edge closes a real cycle). 
- If WorkspaceLocalUnedgeable exists but the compare shows NO missing cycles in the MEASURED set -> the
  certificate STILL says Incomplete (conservative); decide whether to (i) keep A's fallback (safe) or (ii)
  accept the LiveGraph answer for THOSE repos -- but DO NOT infer the no-missing result GLOBALLY (the measured
  set is not all repos). 
- `UnknownDivergence` / `UnexpectedExtraInLiveGraph` (LiveGraph cycles SQLite lacks) remain FORBIDDEN -- an
  extra is an over-claim; if observed, the migration is RED regardless.
RECOMMENDATION: as written. The certificate is the per-repo predicate; the compare is the cross-check.
```

## Measurement protocol (EXECUTED — see Findings)
```text
Repo set: xpart-monorepo (control), amodx, hexmanos, zap-engine, repo-graph.
Per repo: `livegraph-refresh --all-discovered` (fresh ingest) THEN
  (a) `cycle-completeness-audit` -> the certificate state + the per-class flags (the histogram), and
  (b) `cycles --engine compare --kind module-import` -> SQLite count, LiveGraph count, matched, missing-in-
      LiveGraph (+ divergence classes), extra-in-LiveGraph.
Capture per repo: certificate; {has_workspace_local_unedgeable, has_unresolved_after_overlay,
  has_unresolved_package, has_alias_unresolved, has_dynamic_unresolved, has_external_nonlocal_benign,
  has_asset_nonrelevant_benign}; SQLite/LiveGraph cycle counts; matched/missing/extra. Evidence-labelled
  EXECUTED/NOT RUN per repo (no inferred rows).
```

## Findings (EXECUTED 2026-06-06)
```text
REPO        CERTIFICATE                    BLOCKING FLAGS                  SQLITE/LG/MATCHED/MISSING/EXTRA
xpart       CompleteForModuleImportCycles  --                             1 / 1 / 1 / 0 / 0   EXACT
amodx       IncompleteImportClasses        workspace_local_unedgeable     3 / 3 / 3 / 0 / 0   EXACT
hexmanos    IncompleteUnsupportedLanguage  unresolved_package*            1 / 1 / 1 / 0 / 0   EXACT
zap-engine  IncompleteUnsupportedLanguage  alias_U,dynamic_U,unres_rel*   1 / 1 / 1 / 0 / 0   EXACT
repo-graph  IncompleteUnsupportedLanguage  -- (language precedes)         6 / 5 / 5 / 1 / 0   MISSING=1
  *hexmanos/zap-engine: language precedence fires (non-TS present) BEFORE the import-class flags; the flags are
   shown for the histogram but the cert reason is UnsupportedLanguage.
  repo-graph MISSING=1 -> `MissingDueToUnloadedOrNonTsPartition`: the cycle is the nested xpart FIXTURE's
   packages/a/src <-> packages/b/src -- correctly EXCLUDED from repo-graph's partitions (ENUMERATION-1 fixture
   policy), but SQLite indexes it -> the compare flags it. The cert (Incomplete) correctly forces fallback.

THREE decisive facts:
1. EXTRA = 0 in EVERY repo. The FORBIDDEN over-claim (a LiveGraph cycle SQLite lacks) NEVER occurs.
2. The CERTIFICATE IS A SAFE PREDICATE: Complete -> EXACT (xpart). Incomplete -> EXACT (amodx/hexmanos/
   zap-engine, CONSERVATIVE) OR missing>0 (repo-graph). NEVER Complete-with-missing -> serving LiveGraph only on
   Complete can NEVER lose a SQLite cycle.
3. amodx (the WorkspaceLocalUnedgeable case) is EXACT (missing=0): the unedgeable workspace imports do NOT close
   any SQLite module cycle the LiveGraph misses IN THE MEASURED SET. So WorkspaceLocalUnedgeable is CONSERVATIVE
   here (the cert blocks; the LiveGraph happens to be exact) -- safe, not proven-necessary, NOT inferable globally.
```

## Verdict — YELLOW (safe predicate proven; benefit still fixture-only)
```text
GREEN for SAFETY: the certificate is a PROVEN-SAFE migration predicate -- model A (serve LiveGraph iff Complete,
  else labelled SQLite fallback) loses NO cycle in the measured set (0 extra; no Complete-with-missing). This is
  the predicate READINESS-1 LACKED -> the deferred CYCLES-DEFAULT-MIGRATION-1 is now BUILDABLE + safe.
YELLOW for BENEFIT: ONLY xpart (the fixture) reaches Complete -> model A serves LiveGraph for xpart ALONE; every
  real repo falls back to SQLite (amodx: workspace-local RED; hexmanos/zap-engine/repo-graph: non-TS). So model A
  today is a SAFE RELABEL (`backend_used`) with NO decommission win for real repos -- SQLite is still read every
  real-repo call. The decommission value awaits a real repo reaching Complete: the RED workspace-edge (amodx) or
  non-TS support (the others).
D3 application: repo-graph has missing>0 AND its cert is Incomplete -> fallback REQUIRED (satisfied by model A).
  amodx/hexmanos/zap-engine have missing=0 but Incomplete -> model A conservatively falls back (safe); we do NOT
  infer the no-missing result globally. EXTRA=0 -> the forbidden case is absent. -> model A is RATIFIABLE + SAFE.
RECOMMENDATION: ratify D1=A (workspace-local -> fallback) + D2=A (AUTO with labelled SQLite fallback) as the safe
  model, and BUILD it as the un-deferred CYCLES-DEFAULT-MIGRATION (the certificate is the proven predicate) IF
  the served-by metadata + the xpart-Complete foundation have value; ELSE D2=C (status quo) until a real repo
  reaches Complete. Either is defensible; the measurement makes A SAFE, not yet HIGH-VALUE.
```

## Out of scope (hard guardrails)
```text
NO default flip (this spec only DECIDES the model + measures), NO raw decommission, NO workspace package edge
(RED until decl-maps / unified-index / a ratified exact target mechanism), NO heuristic source target, NO
deletion. The build of the chosen model is a SEPARATE slice (the un-deferred CYCLES-DEFAULT-MIGRATION).
```

## References
- `docs/slices/module-cycles-default-readiness-1.md` (the READINESS-1 YELLOW this supersedes)
- `docs/slices/cycles-default-migration-1.md` (the DEFERRED migration; the certificate is its prerequisite)
- `docs/slices/cycles-completeness-cert-1.md` (the certificate / `permits_livegraph_default`)
- `docs/slices/imports-asset-and-literal-ext-1.md` (the import-classification completion; amodx's sole blocker)
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (`module_cycle_compare_response` -- the compare)
