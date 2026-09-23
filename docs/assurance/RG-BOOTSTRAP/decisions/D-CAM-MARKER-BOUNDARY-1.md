# D-CAM-MARKER-BOUNDARY-1 — May the undetermined-identity marker cross the storage-port and agent→renderer boundaries as additive DTO fields?

Raised: 2026-09-23 by the first document review of CPP-ATTRIBUTE-MACRO-1A (codex gpt-5.6-terra, D-CAM-MEMBER-MARKER-BOUNDARY-1): `AgentMemberEntry` (agent/src/storage_port.rs) and `ExplainMemberItem` (agent/src/dto/signal.rs) carry no identity fields; D-CERTAINTY-MARK-1 authorizes additive node metadata but does not name the boundary data shape.
Resolved: 2026-09-23 by the OPERATOR — the human's certainty rule (RG-REQ-002-L11) requires the marked fact to be rendered on the agent-facing surface, which is only possible if the fact crosses the two boundaries; the additive-field shape is the ratified precedent (`forward_decl` — EXPLAIN-TYPE-SECTIONS-1, CPP-DECLARATORS-1). The human may override at closeout.

## Options
- **A (ratified)** — additive raw DTO fields `identity` ("undetermined"), `identity_candidates` (two distinct strings) and `identity_basis` (the recovery shape), decoded all-or-nothing by storage (a present-but-malformed carrier marks the entry unreadable), serialized by serde only when present (a determined member is byte-identical), rendered by `explain`. Reward: L11's outcome — the agent sees the candidates and the reason; the same pattern as `forward_decl`. Risk: two documented boundary shapes gain optional fields; every construction site (five struct literals) and the all-or-nothing contract must be kept by consumers.
- **B** — keep the marker opaque storage metadata outside the DTO contracts. Reward: no boundary change. Risk: `explain` cannot render the candidates, or the storage representation leaks into the renderer.
- **C** — persist the node metadata only; defer the `explain` vertical. Reward: smallest change. Risk: a dormant capability — the agent is not shown what to investigate, so L11 is not implemented.

## Resolution
A. Carried by CPP-ATTRIBUTE-MACRO-1A-INPUT-1 (`requiredDecisionIds`).
