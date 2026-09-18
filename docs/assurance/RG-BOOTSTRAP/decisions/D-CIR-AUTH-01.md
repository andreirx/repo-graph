# D-CIR-AUTH-01 — What authorizes CPP-INCLUDE-ROOTS-1 (and the rest of the ratified queue) as requirements-grounded allocations?

Raised: 2026-09-18 by the Q3 document review (review-CPP-INCLUDE-ROOTS-1-PREP-0, codex gpt-5.6-terra), result decision-required, blocking reason: "The current allocation source evidence contradicts the claimed CPP scope; approving the packet would extend a one-slice authorization without a durable authority record."
Resolved: 2026-09-18 by the in-place manager (operator), option A. Authority: the human's 2026-09-14 ratification of the queue order (docs/requirements/README.md "RATIFIED ORDER (human 2026-09-14, option A)", commit c1df38d) — an existing human decision; recording it is bookkeeping that decision implies, not a new decision.

## Problem

The two authority documents pinned by the bootstrap admission baseline — `docs/assurance/RG-BOOTSTRAP/human-authorization.md` and `docs/slices/rg-requirements-bootstrap-1.md` — named exactly one slice, TRUST-MODULE-EDGES-1. The human then ratified the order of the remaining slices on 2026-09-14, but that ratification was recorded only in the requirements README and the roadmap, neither of which the per-slice manifests pinned. Q1 (CALL-BINDING-RECEIVER-1) ran and shipped (437cc16) through that gap unremarked; the Q3 reviewer, whose inputs were exactly the pinned closure, correctly could not find any durable authority for a CPP allocation.

## Options (as the reviewer put them)

- A — publish an explicit authorization / successor admission allocation and pin it (reward: the one-slice bootstrap limit is extended durably rather than silently; risk: a fresh bootstrap baseline and review).
- B — record the runtime selection packet as the authority (reward: keeps the content; risk: authority inferred from generated task text).
- C — defer the allocation (reward: nothing unverified proceeds; risk: RC-10 stays without its packet).

## Resolution — A

Amendment 1 appended to both authority documents (dated, quoting the ratified order and the human directive for the roles), the bootstrap baseline revised to RG-BOOTSTRAP-INPUT-3 (only those two digests change), independently reviewed (standalone codex gpt-5.6-terra, ACCEPT, no findings: docs/assurance/RG-BOOTSTRAP-INPUT-3/requirements-review.json), operator-approved (RG-BOOTSTRAP-BASELINE-APPROVAL-3), commit fcc12e9. Every later per-slice manifest pins docs/requirements/README.md (the ratified order) as a governance dependency. Consequence: each remaining queued slice is authorized one at a time, in order, for a requirements-document item + review + operator approval before its implementation is admitted; nothing else in the limits changes.
