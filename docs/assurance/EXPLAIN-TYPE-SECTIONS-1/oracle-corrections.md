# EXPLAIN-TYPE-SECTIONS-1 — oracle corrections (docs/MANAGER.md § Oracle corrections)

| id | date | check | old → new | evidence | approver | slice digest before → after |
|---|---|---|---|---|---|---|
| OC-1 | 2026-09-20 | ETS-C10 | `rmap explain CGHeroInstance --budget small --json` (a tier explain does not accept: usage `--budget medium|large`, `--full`) → `--budget large --json`; the budget-invariance assertion compares medium (default) / large / --full | first admission cycle 1: the command exited 1 with `error: invalid --budget value: small (expected medium|large)` on BOTH binaries; every other check green | in-place-manager (operator) | 7f1e232e46294ede3e5cf8374306b7ecd8fe2167e5906191acdf0534f38cb181 → 91b6e6c0b75b8be68885545e020b1f040f2bb8df8f97e9ef99001bd382abab66 |

Text-only: no allocation set, check-id set, requirement text or candidate path changes. Carried as INPUT-3 because the runtime needs a re-baseline to carry corrected bytes (MANAGER.md rule 5, TD-022).
