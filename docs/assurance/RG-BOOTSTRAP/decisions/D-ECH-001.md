# D-ECH-001 — May the failed two-independent-index checks ECH-C09 and ECH-C10 be replaced by same-index cross-binary comparisons?

Raised: 2026-09-19 by the EXPLAIN-CYCLES-HONEST-1 implementation review (review-EXPLAIN-CYCLES-HONEST-1-2, codex gpt-5.6-terra), result decision-required; blocking reason: "Changing a mandatory acceptance oracle is a ratification-class evidence decision, not a reviewer substitution."
Resolved: 2026-09-19 by the in-place manager (operator), option A, under docs/MANAGER.md § Oracle corrections (human decision 2026-09-14): a correction that changes only a check's command/expected text is recorded as OC-n and carried as the next INPUT.

## Problem

ECH-C09 compared `cycles`/`orient` text between two INDEPENDENTLY BUILT indexes (the base binary on its own fresh index, the candidate on another). A cycle's rendered walk chooses one ring among equally valid 2-cycles, and that choice depends on the index's node ordering, which is not reproducible across builds: on vcmi four SCCs rendered different rings (`client/renderSDL -> client/render/hdEdition -> client/renderSDL` vs `client/gui -> client/renderSDL -> client/gui`; `launcher/settingsView -> launcher/modManager -> launcher/settingsView` vs `launcher -> launcher/modManager -> launcher`; …). The oracle therefore measured index non-determinism, not the renderer. ECH-C10 likewise compared explain across two indexes and did not strip the per-index `Repo: repo_<uid>` line. Both premises were the manager's; Q1 had already recorded the walk-choice lesson (CYCLES-WALK-DETERMINISM-1) and Q1's own cycles check compared member SETS for that reason. The builder ran everything in the foreground, reported the two literal failures, and proved both obligations by the better oracle (base binary and candidate served against the SAME index: byte-identical).

## Options (as the reviewer put them)

- A — ratify the same-index replacement (reward: measures candidate-vs-base rendering with the binary as the only variable; risk: changes the admitted method after a failure and leaves cross-index non-determinism outside this slice).
- B — retain the current checks (reward: no post-hoc substitution; risk: blocks until the environmental premise is repaired — it cannot be, the premise is false).
- C — accept the failed checks as a limitation (not acceptable: failed verification presented as acceptance).

## Resolution — A, as OC-1/OC-2 with re-review

ECH-C09 and ECH-C10 corrected to same-index cross-binary comparisons (and the `Repo:` line stripped), ledger docs/assurance/EXPLAIN-CYCLES-HONEST-1/oracle-corrections.md, carried as EXPLAIN-CYCLES-HONEST-1-INPUT-2 with an independent document review; the implementation is re-admitted and the checks re-run. The cross-index walk-choice non-determinism is NEW EVIDENCE for the open follow-up CYCLES-WALK-DETERMINISM-1 (an RG-REQ-001-L09 gap: the same source yields different rendered rings on two builds) and is recorded there, not fixed here.
