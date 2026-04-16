# Spike: `rmap orient` on repo-graph itself

**Date:** 2026-04-15
**Slice context:** Candidate C in the post-Rust-43B sequence. Validation pass before Rust-43C rename.
**Status:** Findings only. No code changes in this pass.

## Method

1. Built `rmap` release binary.
2. Indexed this repo:
   `rmap index /Users/apple/Documents/APLICATII\ BIJUTERIE/repo-graph /tmp/rgspike.db`
   Result: `488 files, 3630 nodes, 2091 edges (4556 unresolved)`.
3. Ran:
   `rmap orient /tmp/rgspike.db repo-graph --budget large`
   Captured the JSON at `/tmp/rgspike-orient.json` (exit 0, stderr empty,
   3231 bytes, 4 signals + 3 limits).
4. Cross-checked with `rmap trust /tmp/rgspike.db repo-graph` and
   direct SQLite queries on the `inferences`, `declarations`, and
   `nodes` tables.

## Raw numbers

Trust report:

- `call_resolution_rate = 0.2220` (22.2%)
- `resolved_calls = 885`
- `unresolved_calls = 4247`
- `enrichment_status = None`
- `diagnostics_available = true`

Node kinds in the snapshot:

- `SYMBOL = 3104`
- `FILE = 293`
- `MODULE = 233`

`inferences` table row count: **0**.
`declarations` table active entrypoint count: **0**.

Orient output shape:

- 4 signals: `DEAD_CODE` (rank 1), `IMPORT_CYCLES` (rank 2),
  `MODULE_SUMMARY` (rank 3), `SNAPSHOT_INFO` (rank 4).
- 3 limits: `MODULE_DATA_UNAVAILABLE`, `GATE_NOT_CONFIGURED`,
  `COMPLEXITY_UNAVAILABLE`.
- `confidence = "medium"`.
- `focus.resolved = true`, `resolved_kind = "repo"`.
- `next = []`.
- `truncated = false`.

## Findings

### F1 — **Critical.** DEAD_CODE is 86% of all symbols on a Rust-indexed repo

`dead_count = 2683`, total symbols = `3104`. Dead rate = **86.4%**. No
realistic repo has that much dead code. The cause is structural, not
the repo's fault:

The Rust indexer does **not** populate the `inferences` table.
`storage.find_dead_nodes` excludes nodes via three layers:

1. Incoming reference edges (edges table — works fine).
2. Declared entrypoints (`declarations` table, `kind='entrypoint'`).
3. Framework-liveness inferences (`inferences` table, e.g.
   `framework_entrypoint`, `spring_container_managed`,
   `linux_system_managed`).

On this fixture: `inferences` row count is **0**. `declarations`
active entrypoint count is **0**. So layers 2 and 3 subtract nothing.
The only filter active is "has at least one incoming reference edge".
Combined with a 22% call resolution rate, most symbols appear dead.

Impact on the agent:

- `DEAD_CODE` ranks **#1** at Medium severity in the output.
- `top_dead` lists legitimate production symbols alongside test-file
  functions as "top dead code by size".
- An agent reading the output concludes "there are 2683 symbols to
  delete". That is deeply wrong.

The signal is currently produced without any annotation that the
reliability of dead-code detection is a function of the call
resolution rate. CLAUDE.md already documents this as a known
limitation of `graph dead` ("treat results as graph orphans, not
deletion candidates") — the agent surface does not propagate that
caveat.

**Priority:** P1. This makes repo-level orient actively misleading
on the one repo we control end-to-end.

### F2 — **High.** `TRUST_NO_ENRICHMENT` is silently suppressed on Rust-indexed repos

The trust aggregator rule:

```
if !summary.enrichment_applied && summary.enrichment_eligible > 0 {
    emit TRUST_NO_ENRICHMENT
}
```

On this fixture, `enrichment_status = None` in the TrustReport.
`storage/src/agent_impl.rs::get_trust_summary` maps that to:

```rust
(eligible = 0, enriched = 0, applied = false)
```

So `eligible == 0` → signal is suppressed.

But semantically, "enrichment_status is None" means "no enrichment
phase ran at all," which is very different from "there were no
eligible edges". The Rust indexer does not run a compiler enrichment
stage, so for every Rust-indexed repo, enrichment is silently reported
as "not applicable" when it is actually "never attempted."

Impact on the confidence derivation:

```
if !trust.enrichment_applied && trust.enrichment_eligible > 0 {
    return Confidence::Medium;
}
```

Same masking. A Rust-indexed repo with high call resolution rate would
land in Confidence::High without ever being penalized for the absent
enrichment phase. We avoided this on the current fixture only because
the resolution rate (22%) already pushed us into Medium.

**Priority:** P2. The `AgentTrustSummary` DTO needs to distinguish
three states:

- `Applied` (enrichment ran, some edges resolved)
- `NotApplicable` (enrichment ran, 0 eligible edges)
- `NotRun` (no enrichment phase, state unknown)

### F3 — **High.** DEAD_CODE ranking does not account for resolution quality

Independent of F1, there is a ranking problem: `DEAD_CODE` rank 1 is
handed to the agent at Medium severity with no coupling to the trust
layer. At call resolution rate < ~40-50%, dead-code results are
unreliable. The agent has no hint.

The contract says signals are ranked by (severity desc, category asc,
code asc). There is no mechanism for a signal to demote itself when a
related trust signal is weak.

Options:

- Cross-aggregator coupling: DEAD_CODE reads the trust summary and
  demotes to Low severity (or suppresses entirely) when resolution
  rate is below a threshold.
- New limit code: `DEAD_CODE_UNRELIABLE_LOW_RESOLUTION` emitted
  alongside the signal when resolution is low. Preserves the signal
  but warns the consumer.
- Do not fire DEAD_CODE at all below a threshold; replace with a
  limit.

**Priority:** P2. Coupling or annotation.

### F4 — **Medium.** `top_dead` surfaces test-file functions

Top 3 entries in `top_dead` by line count:

1. `dispatchOp` in `test/storage-parity/storage-parity.test.ts` (189)
2. `RepoIndexer.refreshRepo` in `src/adapters/indexer/repo-indexer.ts` (187)
3. `INITIAL_MIGRATION` in `src/adapters/storage/sqlite/migrations/001-initial.ts` (177)

Entry 1 is a test utility. An agent would not act on it as dead code.
Entries 2 and 3 are production symbols that are legitimately live
(migration runner and refresh handler) but lack entrypoint
declarations — same root cause as F1.

A smaller fix independent of F1: the DEAD_CODE aggregator could
exclude nodes in files where `files.is_test = 1`. That column already
exists in storage; the port just does not surface it.

**Priority:** P3. Does not fix F1 but strips the worst noise at the
top of the evidence list.

### F5 — **Medium.** `MODULE_SUMMARY.languages` is polyglot-blind

`languages: ["jsx", "tsx", "typescript"]`. The repo has a Rust tree
at `rust/crates/*` with ~40 `.rs` files across 10 crates, none of
which appear in the output. The Rust CLI uses the TypeScript
extractor only (documented limitation). An agent reading
`MODULE_SUMMARY` concludes "this is a pure TypeScript project".

`MODULE_DATA_UNAVAILABLE` limit covers the module-catalog gap but
does not cover the "files exist on disk in a language this indexer
does not understand" gap.

**Priority:** P3. Options:

- New limit `LANGUAGE_COVERAGE_PARTIAL` that fires whenever the
  indexer's language set is known to be narrower than the file tree.
- Scan the file tree on the indexer side and record detected
  extensions in `file_versions.language` even for unparsed files.
- Document it in the existing `MODULE_DATA_UNAVAILABLE` summary.

### F6 — **Low.** Tie-break between DEAD_CODE and IMPORT_CYCLES is alphabetical

Both are (Structure, Medium). Current sort breaks ties on code string
ascending, so "DEAD_CODE" ranks before "IMPORT_CYCLES" unconditionally.
Deterministic but arbitrary. There is no product reason DEAD_CODE
should always outrank IMPORT_CYCLES.

**Priority:** P3. Small. A fixed sub-ordering by code (e.g. cycles
before dead code within the Structure/Medium tier) would be a
one-line change in `ranking.rs`.

### F7 — **Informational.** Cycles include suspicious-looking module names

```
seams → detectors
typescript → src
typescript → src → indexer
```

`seams → detectors` is plausibly real — seams and detectors are
neighboring concerns in the extraction pipeline. `typescript → src`
and `typescript → src → indexer` are suspect: "typescript" looks
like a language identifier, not a logical module path. If the
extractor is producing MODULE nodes named after language identifiers,
that is an upstream issue unrelated to orient.

Not actionable in orient. Worth investigating in the extractor.
**Priority:** informational.

### F8 — **Known deferral.** `next = []`

Repo-level `next` list is always empty per the Rust-43B contract.
Rust-44+ module focus is when structured follow-up actions become
meaningful. Not a bug.

## Summary table

| Finding | Priority | Action shape |
|---|---|---|
| F1 DEAD_CODE at 86% on Rust-indexed repo | **P1** | Limit + threshold + or index-side inferences |
| F2 Enrichment state masked as NotApplicable | **P2** | Widen `AgentTrustSummary` enrichment state |
| F3 DEAD_CODE ranking ignores resolution rate | **P2** | Couple with trust, or add limit |
| F4 top_dead includes test-file functions | P3 | Port widening for `is_test` filter |
| F5 MODULE_SUMMARY languages polyglot-blind | P3 | New limit or indexer-side scan |
| F6 Alphabetical tie-break within Medium/Structure | P3 | One-line sort change |
| F7 "typescript" as a module name in cycles | info | Investigate extractor |
| F8 `next` always empty | n/a | Rust-44+ scope |

## Recommendation

F1 is the blocker. It makes repo-level orient actively misleading on
the canonical self-index. Module focus (slice B) would surface the
same noise at a tighter scope, which would be worse — a single module
with 30 symbols showing "25 dead" is even more misleading than the
aggregate. **Do not ship module focus before F1 is addressed.**

F2 and F3 interact with F1: the root cause for all three is that the
Rust indexer does not participate in the trust/inference/enrichment
pipeline the same way the TS indexer does, and orient treats "absent
signal" as "no concern". Fixing them together is cheaper than fixing
them separately.

F4, F5, F6 are small independent improvements, not blockers.

F7 is extractor-side; out of scope for orient work.

## Proposed follow-up order

1. **Fix slice:** F1 + F2 + F3 together. New limit code
   `DEAD_CODE_UNRELIABLE` OR suppression rule + widened enrichment
   state. Details need a decision pass before code.
2. **Small cleanup slice:** F4, F5, F6 bundled. Low risk.
3. **Then** Rust-43C (binary rename) as originally planned.
4. **Then** Rust-44 (module focus).

Inserting the fix slice between C (this spike) and A (rename) is
consistent with "do not expand the user-visible surface on top of a
known-misleading output shape". The rename itself is mechanical; it
does not get harder if it waits.

## Artifacts

- Captured orient JSON: `/tmp/rgspike-orient.json` (temp, not
  checked in).
- Temp DB: `/tmp/rgspike.db` (temp, not checked in).
- Trust report subset used for cross-checks recorded above.

## Not covered in this spike

- Module-focus orient (not yet implemented).
- `rgr enrich` path (only affects TS indexer).
- TS-indexed snapshot comparison. The same DB seeded via the TS side
  would exercise the inferences/declarations layers and likely produce
  very different DEAD_CODE numbers. Worth doing if a future spike has
  capacity.

## Post-fix verification

After the F1/F2/F3 fix slice landed (new
`DEAD_CODE_UNRELIABLE` limit, three-state `EnrichmentState`
enum, trust-authoritative reliability gate in the dead-code
aggregator), the same spike was re-run against the same
`/tmp/rgspike.db`:

```
./target/release/rmap orient /tmp/rgspike.db repo-graph --budget large
```

### Observed output (post-fix)

4 signals, 4 limits. Rank order:

1. `IMPORT_CYCLES` (Medium, Structure) — 5 module-level cycles
2. `TRUST_NO_ENRICHMENT` (Low, Trust) — enrichment phase never ran
3. `MODULE_SUMMARY` (Low, Informational)
4. `SNAPSHOT_INFO` (Low, Informational)

Limits:

1. `DEAD_CODE_UNRELIABLE` — reasons: `["missing_entrypoint_declarations"]`
2. `MODULE_DATA_UNAVAILABLE`
3. `GATE_NOT_CONFIGURED`
4. `COMPLEXITY_UNAVAILABLE`

Confidence: `medium` (unchanged, 22.2% call resolution rate
lands in the Medium band independent of enrichment).

### Findings status after fix

| Finding | Status |
|---|---|
| **F1** DEAD_CODE at 86% on Rust-indexed repo | **FIXED.** Signal suppressed, `DEAD_CODE_UNRELIABLE` limit fires with trust reason `missing_entrypoint_declarations` passed through verbatim. |
| **F2** TRUST_NO_ENRICHMENT silently suppressed | **FIXED.** `EnrichmentState::NotRun` is now distinct from `NotApplicable`. Signal fires at rank 2 on this fixture. |
| **F3** DEAD_CODE ranking ignored resolution quality | **FIXED.** The reliability gate in the aggregator reads `trust.reliability.dead_code.level` as the authoritative verdict. The agent crate does not re-derive threshold logic. |
| F4 `top_dead` surfaces test-file functions | Still open. Deferred to the F4/F5/F6 cleanup slice. Not a correctness issue now because `top_dead` is never emitted on this fixture. |
| F5 `MODULE_SUMMARY.languages` polyglot-blind | Still open. Deferred. |
| F6 Alphabetical tie-break Medium/Structure | Still open. Deferred. On this fixture the tie-break no longer affects output because DEAD_CODE is gone and IMPORT_CYCLES stands alone in its tier. |
| F7 `"typescript"` in cycle module names | Still informational. Extractor-side. |
| F8 `next = []` | Still deferred to Rust-44+. |

### Minor post-fix note for output-quality slice

`TRUST_NO_ENRICHMENT` now fires on `EnrichmentState::NotRun`.
Its current summary text reads:

> "No compiler enrichment applied; call graph resolution
> relies on syntax-only extraction."

"No compiler enrichment applied" is slightly imprecise under
the new semantics: the signal now specifically marks
"phase never ran" (not "phase ran but resolved nothing"). The
wording could be tightened to "Enrichment phase did not run;
call graph resolution relies on syntax-only extraction". This
is a wording tweak, not a correctness issue — tracked as
part of the F4/F5/F6 output-quality cleanup.

### Comparison table

| Dimension | Before fix | After fix |
|---|---|---|
| Signals | DEAD_CODE(2683), IMPORT_CYCLES(5), MODULE_SUMMARY, SNAPSHOT_INFO | IMPORT_CYCLES(5), TRUST_NO_ENRICHMENT, MODULE_SUMMARY, SNAPSHOT_INFO |
| Rank 1 signal | DEAD_CODE (misleading) | IMPORT_CYCLES (actionable) |
| Limits | 3 | 4 (new DEAD_CODE_UNRELIABLE carrying trust reasons) |
| Confidence | medium | medium |
| Agent conclusion from rank-1 signal | "delete 2683 symbols" (wrong) | "investigate 5 import cycles" (correct) |

The fix preserves all outputs that were right and removes
the one output that was wrong. No pre-existing test regressed
except the storage-side smoke test, whose assertion was
updated to encode the new (and more meaningful) reliability-
gate behavior.

### P2 follow-up: enrichment state disambiguation

A code-review P2 after the initial F1/F2/F3 fix identified
that `Option<EnrichmentStatus> == None` from the trust layer
could mean "no eligible samples" (NotApplicable) just as
easily as "phase did not run on eligible samples" (NotRun).
The initial fix mapped all `None` to `NotRun`, which would
have produced a false positive on any repo with nothing to
enrich.

Resolution (spike re-verified after the fix): added a
`#[serde(skip, default)] pub enrichment_eligible_count: u64`
field to `trust::types::TrustReport`. The counter is
populated by `compute_blast_radius_and_enrichment` and
exposed to the agent adapter. Adapter mapping now reads:

- `enrichment_status.is_some() → Ran`
- `enrichment_status.is_none() && count == 0 → NotApplicable`
- `enrichment_status.is_none() && count > 0 → NotRun`

The field never serializes so the trust crate's JSON shape
and cross-runtime parity with TS are unchanged. The trust
parity harness passes unaltered (verified).

Spike output after the P2 fix: **identical** to the
post-F1/F2/F3 output above. This confirms that the
repo-graph self-index is legitimately in the `NotRun` state
(eligible samples exist, enrichment phase never ran on
them). The P2 fix did NOT alter the repo-graph fixture's
classification — it eliminated a separate false-positive
path the original fixture never exercised. Repos with zero
eligible samples (some minimal fixtures, some empty
snapshots) would have been incorrectly flagged; those now
map to `NotApplicable` as intended.

Regression tests:

- `repo_graph_trust::service::tests::enrichment_eligible_count_is_zero_when_no_samples_at_all`
- `repo_graph_trust::service::tests::enrichment_eligible_count_is_zero_when_samples_are_not_calls_obj_method`
- `repo_graph_trust::service::tests::enrichment_null_when_no_enrichment_markers` (extended to also assert the counter)
- `repo_graph_storage::tests::agent_impl::empty_snapshot_maps_to_enrichment_state_not_applicable`

Post-P2 workspace: 1131 tests passing, 0 failures.
