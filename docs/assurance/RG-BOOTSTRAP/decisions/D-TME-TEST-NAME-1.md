# D-TME-TEST-NAME-1 — operator decision (2026-09-13)

Raised by: Codex gpt-5.6-terra implementation review-1 of TRUST-MODULE-EDGES-1 under baseline INPUT-4 (decision-required on RG-REQ-002-L02 / RG-REQ-009-L02).

Question: the slice rewrites the render test `suspicious_modules_state_basis_and_point_at_stats` so that it now asserts the reader-frame basis sentence is present and that `fan_in = fan_out = 0`, `directory node` and the `` `stats` `` pointer are ABSENT — the opposite of what its name states. The candidate kept the name because TME-C03's mandatory command selects the test by exact name (the builder's own comment: "The test name is retained so the assurance check selects it"). Options: A rename the test and amend C03's filter under a new baseline; B keep the false name.

Decision: **Option A.** A name that states the opposite of the behaviour is a defect, not cosmetics: it misleads the next reader, human or agent. The test is renamed `suspicious_modules_basis_in_reader_frame_no_internal_wording` (what it asserts: the Suspicious Modules basis is stated in the reader's frame and no internal pipeline wording leaks). TME-C03's command filter and `expected` name the new identity; the retained-name comment is removed. No behaviour changes; the candidate is otherwise unchanged.

Lesson (manager authoring): when a slice rewrites a test's behaviour, the oracle must name the NEW test identity — binding the old name into a mandatory command forces the builder to choose between a false name and a failing oracle. Recorded in agent-manager CLAUDE.md / TD-023.

Authority: operator (in-place-manager), local to a test identifier and the slice's own validation text. The human may override. Basis: docs/assurance/RG-BOOTSTRAP/human-authorization.md.
