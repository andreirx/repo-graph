# D-ETS-C10 — May the mandatory check ETS-C10 be corrected from `--budget small` (a tier `explain` does not accept) to `--budget large`?

Raised: 2026-09-20 by the EXPLAIN-TYPE-SECTIONS-1 implementation review (review-EXPLAIN-TYPE-SECTIONS-1-0 under INPUT-2, codex gpt-5.6-terra), result decision-required; blocking reason: "The mandatory check is non-passing and its correction changes an approved allocation, not local implementation detail."
Resolved: 2026-09-20 by the in-place manager (operator), option 1, under docs/MANAGER.md § Oracle corrections (human decision 2026-09-14): the change alters only a check's command literal (no allocation set, check-id set, requirement text or candidate path); recorded as OC-1 in docs/assurance/EXPLAIN-TYPE-SECTIONS-1/oracle-corrections.md and carried as INPUT-3 because an admitted item needs a re-baseline to carry corrected bytes (rule 5, TD-022). The human may override.

## Problem

ETS-C10 (vcmi `explain CGHeroInstance`, budget invariance of the two new signals' `count`) invoked `rmap explain CGHeroInstance --budget small --json`. `explain`'s budget flag accepts `medium|large` (or `--full`); `small` is an `orient` tier (parser: rust/crates/rgr/src/commands/orient.rs — `:159` orient accepts small|medium|large, `:498` explain rejects it with `error: invalid --budget value: small (expected medium|large)`). The command exited 1 on BOTH the before and the candidate binary; the product was correct, the oracle was not (the manager did not verify that literal token against the CLI before the document review — a packet defect).

## Options (as the reviewer put them)

1. Amend ETS-C10 to use the supported `--budget large` — reward: the same count-invariance oracle over the whole explain budget surface (medium = default, large, --full); risk: an allocation-text correction with its revalidation; no product change. RECOMMENDED by the reviewer.
2. Add `small` as an explain CLI alias — reward: the literal command passes; risk: widens a frozen CLI contract outside the slice without a ratified need (RG-REQ-012 surface).
3. Accept the failed mandatory check — reward: no work; risk: an internally inconsistent acceptance boundary and a completion with a failed required gate. Not acceptable.

## Resolution — option 1, as OC-1 with re-review

ETS-C10's tiers become medium (the default invocation) / `--budget large` / `--full`; the JSON `count` of EXPLAIN_MEMBERS and EXPLAIN_REFERENCED_BY must be identical across the three and `items` monotonic, equal to `count` under `--full`. The corrected allocation is INPUT-3 (document item PREP-3, independent review); the preserved candidate is re-applied at the second admission together with the fix for the review's other finding, F-ETS-01 (the renderers defaulted a malformed `count` to 0 and did not render the unreadable line for a non-array `items` — a code defect, corrected in the candidate, not in the oracle).
