# D-TME-VALIDATION-STALE-1 — operator decision (2026-09-13)

Raised by: Codex gpt-5.6-terra implementation review-0 of TRUST-MODULE-EDGES-1 under baseline INPUT-3 (decision-required).

Question: TME-C08B's mandatory command greps `calls resolved` and TME-C14B's greps `"total_files"`; neither token exists in the product's current output (`rmap trust` prints "your code's calls N% resolved (…)"; `stats --json` has `indexed_file_count`, no `total_files`). The stage-3 builder proved both obligations by byte-identity of the real line / the whole JSON and reported `passed`; the reviewer correctly refused to close a mandatory check on substituted evidence. Options: A amend the oracles and re-approve; B change product output or JSON shape to satisfy the stale greps; C refuse the fix.

Decision: **Option A.** The tokens were wrong when authored (a manager authoring error never executable at document review); the intent — resolution figures and the indexed-file count are byte-stable before/after — is unchanged. C08B now greps `calls [0-9]+% resolved`; C14B greps `indexed_file_count`. In the same amendment, TME-C03's `expected` and §2 item 3 drop the "no alias reason" wording superseded by D-TME-MOVEMENT-1 (reviewer finding F-TME-001, PREP-3 cycle 2). B is rejected: it would alter a user-visible surface to serve a check (RG-REQ-002 honesty; the map is not the territory). C is rejected: the fix is independently reproduced (seam test on two fixtures; 0 fan mismatches on five store copies).

Consequence: re-baseline as TRUST-MODULE-EDGES-1-INPUT-4 (this record added as a governance dependency and required decision); the implementation is re-admitted and its evidence regenerated under INPUT-4. The builder's cycle-0 candidate is unchanged.

Authority: operator (in-place-manager), local to the slice's own validation text; no ratified behaviour or requirement changes. The human may override. Basis: docs/assurance/RG-BOOTSTRAP/human-authorization.md; D-TME-MOVEMENT-1. Runtime gap recorded as agent-manager TD-022.
