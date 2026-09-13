# D-TME-CHANGE-CLASSIFICATION-1 — operator decision (2026-09-13)

Raised by: Codex gpt-5.6-terra structured review-3 of TRUST-MODULE-EDGES-1-PREP (decision-required on RG-REQ-009-L04).

Question: the selection packet asked for RG-REQ-009-L04 in both `implements` and `changes`; the accepted stage-3 allocation grammar requires the three sets to be pairwise disjoint, and `changes` means a change to a requirement STATEMENT authorized by the baseline, not a movement of rendered evidence.

Decision: **Option A.** RG-REQ-009-L04 stays in `implements`; `changes` stays empty; the selection packet's wording is corrected — the pre-authorised movement of the trust downgrade-reason lines is a behaviour-change RECORD in the slice document's §0, carried by L04's implementation, not a requirement change. The accepted grammar is not amended.

Authority: operator (in-place-manager), local to the packet's own wording; no requirement text, boundary or grammar changes. Basis: docs/assurance/RG-BOOTSTRAP/human-authorization.md; agent-manager docs/slices/assurance-3-evidence-linked-review.md (allocation grammar).
