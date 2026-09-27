# D-PRB-UNION-ROUTE-1 — An inferred-inclusive callers/callees request is not a union answer

Raised: 2026-09-27 by the document review of PYTHON-RECEIVER-BINDING-1-PREP-8 (INPUT-3, cycle 1; reviewer codex gpt-6-sol; decision D-PRB-UNION-COUNT), on OC-7's proof of implementation review-0 finding F-1.

**The problem.** With `RMAP_RECON_UNION=1` and `--engine auto`, callers/callees can be served by the union (`dispatch.rs:1300`: `union_serving_enabled() && engine == Auto`). The union response's `witness_counts` is defined one-to-one over the answer's whole row multiset, with `count = rows.len()` (`docs/slices/recon-design-1.md` §5.2). This slice's `--include-inferred` adds inferred rows to the answer. Those rows have no witness. Appending them to a union answer either breaks that one-to-one account or needs a new response shape. Review-0 F-1 showed the candidate also counts them twice on that route.

Resolved: 2026-09-27 by the OPERATOR (in-place-manager): option D below. The human may override.

## Options
- **A (the reviewer's recommendation): a separate inferred tier on union responses.** Reward: the union contract is untouched. Risk: a new response field on the union route only. The same request would then carry inferred rows in `callers` on the SQLite/LiveGraph route and in another field on the union route, so there are two shapes for one question.
- **B: redefine the union counts over the combined list.** Risk: changes a ratified JSON meaning (§5.2) and its consumers; needs version/compatibility treatment.
- **C: defer inferred inclusion on union serving.** Risk: the ratified `--include-inferred` behaviour is incomplete on that route.
- **D (ratified): route an inferred-inclusive request around the union.** The union arm is taken only when `include_inferred` is false. An inferred-inclusive request with the flag on is served by the existing `Auto` route (LiveGraph when its certificate is green, else SQLite), which already carries inferred rows in the one list with `count` equal to the rows listed; `backend_used` names the backend that served it. Reward: no response shape and no ratified contract changes; `--include-inferred` works on every route; the union's witness accounting stays one-to-one because union answers never hold inferred rows. Risk: with the flag on, adding `--include-inferred` also changes the serving backend (stated by `backend_used`), so the certain rows of that answer are the `Auto` route's, not the union's. Union serving is flag-gated and non-default.

## Consequences for the slice
- The routing predicate that selects the union arm includes `!include_inferred`, and a test proves it through the code the dispatch arms execute (both callers and callees), not by re-assembling it.
- Every comparison or witness calculation (the union's and the `Compare` engine's `compare_keys`) receives only the certain rows; inferred rows are added once, after, to the served answer (review-0 F-1's required correction).
