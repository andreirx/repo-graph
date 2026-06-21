# QUERY-MIGRATION-1: Route Query Surfaces onto the LiveGraph Runtime (Stage C, slice 3)

Slice ID: QUERY-MIGRATION-1
Status: **BUILT (2026-05-31) — headless query SEMANTICS on the Test API, NOT shipped-CLI rewiring.**
`repo-graph-livegraph` now serves `callers` + `callees` through the trust vocabulary; `AnswerEnvelope`
carries the D1 contributing-language set. 17 livegraph tests + 22 trust-model tests green. Maturity
**PROTOTYPE** (not PRODUCTION until a shipped surface consumes it).
Depends: LIVEGRAPH-RUNTIME-1 (`repo-graph-livegraph`), TRUST-MODEL-REBASE-1 (`repo-graph-trust-model`),
STAGE-C-ENTRY-DECISION.
Track: Extraction Substrate Pivot — **Stage C, slice 3** (after LIVEGRAPH-RUNTIME-1, before
VALUE-JOIN-1).

## Purpose / the one risk this slice retired

Route the query surfaces onto the LiveGraph runtime emitting the **trust vocabulary**
(`AnswerEnvelope`) **without losing trust fidelity** — specifically, a query answer spanning
**TS + C/C++ + Rust** partitions must report ALL contributing language maturities, never one
arbitrary `LanguageSupport`. Retired: the LIVEGRAPH-RUNTIME-1 last-wins language collapse, and the
absence of a symmetric `callees`.

## Scope rule (ratified)

```text
QUERY-MIGRATION-1 = migrate query SEMANTICS onto the LiveGraph headless Test API.
NOT shipped CLI migration. "migration" does not imply rewiring rmap callers/... here.
```

## Ratified decisions (2026-05-31) — built as below

**D1 — language-support metadata shape: ON `AnswerEnvelope` (Option 1).** `AnswerEnvelope` carries
`contributing_languages: BTreeSet<LanguageSupport>` (a 3rd `repo-graph-trust-model` amendment).
Rationale: `LanguageSupport` is part of the trust label, not presentation metadata — if it lived only
in a query DTO wrapper, lower layers could emit trust-labelled answers that silently dropped the
language-maturity axis. Constructor rules:
- `Exact` / `Partial` / `Stale` → `contributing_languages` non-empty (`TrustError::MissingContributingLanguages`).
- `Unavailable` → MAY be empty (target unknown before any partition is identified; may also carry
  languages when the partition is known but the xref is absent).
- `BTreeSet` (deduped, ordered) — deterministic output, never a single collapsed value.

**D2 — surfaces: `callers` + `callees`. `path` NOT included.** `callees` is the symmetric
single-hop counterpart (outgoing edges); `path` needs traversal-completeness rules and is deferred.

**D3 — CLI wiring: headless query API only.** No shipped CLI/daemon changes. Tests drive the runtime
directly (Clean-Architecture Test API). Shipped `rmap` migration is a later integration slice.

**D4 — crate placement: extend `repo-graph-livegraph`.** No new `repo-graph-query` crate (premature
until `path`/multi-hop exists).

## What was built

- **D1 amendment (`repo-graph-trust-model`):** `AnswerEnvelope.contributing_languages`,
  `TrustError::MissingContributingLanguages`, `Ord` on `LanguageSupport` (for `BTreeSet`); every
  smart constructor threads + validates the set. 22 trust tests (incl.
  `exact_answer_requires_nonempty_languages`, `unavailable_unknown_target_may_have_empty_languages`).
- **Language collapse removed (`repo-graph-livegraph`):** `fold_contributing(ids)` returns
  `(worst freshness, contributing epochs, contributing-language UNION)`. `callers` and `callees` both
  build the union; the prior `language = s.language` last-wins assignment is gone.
- **`callees(target)` added:** finds the target's defining partition; if absent → `Unavailable`; if
  non-resident → `Partial` + `missing_partitions=[def_part]` (outgoing adjacency is not in the xref);
  if resident → reads target's outgoing edges, resolves each callee to its defining partition via the
  retained `defines` summary (callee partitions need NOT be resident), and unions languages. A callee
  with no known defining partition → `DegradationReason::UnresolvedAlias` + `Partial`.
- **Shared `finalize_envelope`:** `callers` + `callees` apply the SAME rules in one place — a
  cross-partition lookup is SCIP-dependent so `PrecisionPending` → `Partial` (trust invariant 6, no
  `NotScipDependent` proof); a non-resident contributing partition → `Partial`; never exact-empty for
  a missing/stale state.

## callers/callees residency asymmetry (ratified, documented)

```text
callers can answer partition-summary from the INCOMING xref while referencing partitions are non-resident.
callees requires the target's defining partition RESIDENT because OUTGOING adjacency is not retained in the xref.
Summary-level callees is deferred until a measured memory model decides whether to retain outgoing adjacency.
Ratified: do NOT add outgoing adjacency in this slice (it would turn the always-resident xref into a second graph copy).
```

## Tests (EXECUTED)

- D1 / language: `callers_contributing_languages_union`, `callees_contributing_languages_union`,
  `mixed_language_answer_has_all_languages` (TS+C+++Rust → set of 3), `no_last_wins_language_collapse`,
  `exact_answer_requires_nonempty_languages`, `unavailable_unknown_target_may_have_empty_languages`.
- `callees` core: `callees_all_resolved_resident_is_exact`,
  `callees_target_partition_nonresident_is_partial_missing`, `callees_unknown_target_is_unavailable`,
  `callees_unresolved_callee_is_partial`, `callees_stale_callee_partition_is_stale`.
- All 8 prior LIVEGRAPH `callers` cases still green. **17 livegraph + 22 trust = green; clippy
  `-D warnings` clean; fmt clean.**

## Tech debt / divergences (recorded)

1. **`CompletenessInput.language` is a single field and `classify_answer` does not consume it.**
   `classify_cross_partition` passes the least-mature contributing language as a conservative
   placeholder; the query-visible language set travels on the envelope. When the policy becomes
   language-aware it should read the full set. (No user-visible collapse — the placeholder is internal.)
2. **`finalize_envelope` precondition:** a `Partial` classification must carry a reason, a missing
   partition, or a non-`Fresh` freshness. A call-graph-incomplete defining basis (e.g. `AstFileScope`)
   with none of those is not mapped to a `DegradationReason` and would panic the `partial` constructor
   — unreachable with current call-graph fixtures; harden when file-scope targets are queried.
3. **`PartitionId` still `String`** across the trust/runtime boundary (typed id is a recorded follow-up).
4. **Divergence from the original migration-plan bullet:** the plan's QUERY-MIGRATION-1 also called for
   strict-CALLS-default / graded-REFERENCES surfacing and a SQLite fallback-during-transition. This
   slice scoped to headless `callers`/`callees` + language metadata (D2/D3); those items remain for the
   shipped-CLI integration slice.

## Out of scope (held)

```text
No warm-cache / persistence / disk format. No value-level joins (VALUE-JOIN-1).
No new trust vocabulary beyond the D1 language set. No indexer orchestration.
No shipped CLI / daemon rewiring. No path / cycles. No outgoing-adjacency xref.
```

## Exit criterion (met)

`callers` + `callees` run on the LiveGraph runtime and emit the trust vocabulary with honest
multi-language metadata (the contributing-language UNION); the last-wins collapse is gone; no
warm-cache; no new trust semantics beyond D1; headless only. VALUE-JOIN-1 can add value-level
cross-partition identity; it does not redefine the query surfaces or the vocabulary.

## References
- `docs/architecture/stage-c-entry-decision.md` (`LanguageSupport` as a query-visible trust axis)
- `docs/slices/livegraph-runtime-1.md` (the runtime + the language-collapse defect this fixed)
- `docs/slices/trust-model-rebase-1.md` (`AnswerEnvelope`; the vocabulary)
- `docs/slices/xpart-prove-1.md` (cross-partition answer-class model the surfaces honor)
