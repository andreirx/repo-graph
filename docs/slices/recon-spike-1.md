# RECON-SPIKE-1 — classify the SCIP↔pipeline divergence (stop discarding the diff)

Status: SPECIFIED (2026-07-16) · Track: Reconciliation (ENGINE-CONSOLIDATION-1 §8b
prerequisite) · Origin: ratified direction change (human, 2026-07-16): the two call-graph
producers are two WITNESSES of the same truth; their differences are evidence to classify,
not a fight to adjudicate. The divergence has NEVER been measured on a real repo
(`dataflow-hotpath-map.md` residuals: "expected SCIP↔tree-sitter mismatches not yet
classified — NOT RUN").

## 1. Problem

The callgraph certificate (`daemon-runtime/src/callgraph_cert/`) performs an exhaustive
per-symbol multiset comparison of the two graphs on every fingerprint — then reduces the
result to one bit (GREEN serve / RED fallback) and discards the detail. We pay for the
comparison and throw away exactly the data the reconciliation design needs.

## 2. Contract (spike: instrument + run + classify; NO reconciliation logic)

1. **Emit the diff.** Additive instrumentation in the cert path: when the comparison runs,
   optionally capture the per-symbol mismatch detail (symbol key; SCIP-only edges;
   pipeline-only edges; per-edge target keys) to a debug artifact (JSON file under the
   state root or a `--debug`-gated dump — least-new-surface option, builder records the
   choice). Off by default; zero cost when off; the GREEN/RED behavior is UNCHANGED.
2. **Run on a real repo:** repo-graph self-index (isolated), with the SCIP producer
   enabled so LiveGraph partitions exist and the cert actually compares. Capture the full
   diff artifact.
3. **Classify every mismatch** in the build report, by:
   - DIRECTION: SCIP-only vs pipeline-only.
   - CAUSE (deterministic evidence per class, cited): semantic resolution the pipeline's
     heuristics missed (aliases/re-exports/etc.) · compilation-failure or producer-skip
     (files SCIP never saw) · coverage boundary (partition/language SCIP doesn't cover) ·
     identity/key mismatch (same edge, different keys — adoption failure) · other
     (enumerated honestly).
   - MAGNITUDE: counts per class; share of total edges; whether the vaunted "SCIP is
     richer" holds, inverts, or both-ways on this repo.
4. **The deliverable is the CLASSIFIED REPORT** (in the build report + a summary appended
   to this doc §5): the empirical answer to "superset or both-ways, and why" — the input
   RECON-DESIGN-1 is built from. Include the null result honestly if divergence is ~0 on
   repo-graph (small TS surface post-retirement) and state what the monorepo run must
   still answer.

## 3. Stop conditions

- Cert GREEN/RED semantics, serving behavior, and epoch/W-B invariants UNCHANGED —
  instrumentation is additive and gated. No reconciliation/merge logic. No schema changes.
- If the TS surface post-retirement is too small for SCIP partitions to exist at all →
  report that honestly as the finding (the spike then re-runs on the monorepo when field
  data exists) rather than fabricating divergence data.
- Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Cargo gates from `rust/` (fmt / clippy / affected crates + full suite chunked), raw exit
  statuses as they land.
- Named tests: instrumentation off-by-default (no artifact, no behavior change);
  on → artifact schema stable/deterministic ordering; GREEN/RED unchanged either way.
- Isolated live run (/private/tmp + stdio; registry checksum): the diff artifact captured
  + the classification with cited examples per class.

## 5. Findings (appended by the spike)

_(to be written)_
