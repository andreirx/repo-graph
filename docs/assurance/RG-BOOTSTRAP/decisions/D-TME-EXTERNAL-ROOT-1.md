# D-TME-EXTERNAL-ROOT-1 — operator decision (2026-09-13)

Raised by: Codex gpt-5.6-terra structured review-3 of TRUST-MODULE-EDGES-1-PREP (decision-required on RG-REQ-011-L06).

Question: the selection packet listed `/private/tmp` throwaway roots under `candidateExclusions`; the accepted stage-3 grammar admits only target-relative operational path prefixes, because candidate tracking observes the target tree.

Decision: **Option A.** `candidateExclusions` hold only target-relative prefixes (`.agent-manager/`, `rust/target/`); isolation of external throwaway roots is proven by the explicit check TME-C17 (roots created, used and removed; operator registry sha256 identical before/after), which is the honest representation — an external path is never part of the candidate tree. The selection packet's wording is corrected; the grammar is not amended.

Authority: operator (in-place-manager), local to the packet's own wording. Basis: docs/assurance/RG-BOOTSTRAP/human-authorization.md; agent-manager docs/contracts/target-owned-relay.md (candidate tracking).
