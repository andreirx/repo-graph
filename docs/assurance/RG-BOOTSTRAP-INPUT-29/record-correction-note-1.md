# Record correction note 1 — RG-BOOTSTRAP-INPUT-29 (2026-10-05, operator)

Kind: record-correction-note (narrative; not a parsed record). Corrects the subject TEXT of two published records without editing them:

- `docs/assurance/RG-BOOTSTRAP-INPUT-29/requirements-review.json` (sha256:195b2dd1bb95efb3ea3ebe2a92b66b3dc45627d37ce5fd6c94b1a62283136880) — field `report`;
- `docs/assurance/RG-BOOTSTRAP-INPUT-29/baseline-approval.json` (sha256:3940d24d5b1f1c895afc5ac9d3fe30d1f0a80399a5991bd47093542b6c062589) — field `rationale`;
- and the commit message of befac6f1 (RG-BOOTSTRAP-INPUT-29), which is not rewritten.

**Defect.** In all three the phrase that names the second changed pin reads "the new  subsection" / "the  subsection" with a missing word. The intended text is "the new `doctor` subsection" of `docs/cli/rmap-contracts.md` (the subsection "`doctor` — transport, state root and the daemon it judges (DOCTOR-FALLBACK-STATE-ROOT-1)", added at 44f357bb and corrected at 80feaaf4 and 47bca4d9). Cause: the operator's accept script carried the word in backticks inside a double-quoted shell string; the shell substituted the output of a non-existent command `doctor` (empty) before the text reached the records tool. The reviewer's verdict (pass 3, session 01a109b5-7030-7770-9f51-b6d8a1d96620, RESULT: accepted, FINDINGS: none), the manifest digest (sha256:6c0f83d250c463943b6e79bbefd490d2ccb8633e7fa93493296e8afae854d29f), the changed-pin digests and the approval decision are unaffected; only the operator-authored prose lost one word.

**Correct reading of the subject.** Bootstrap re-pin INPUT-28 → INPUT-29: two pins changed — `docs/requirements/rg-req-011-daemon-and-store-lifecycle.md` at 674f9f47 (RG-REQ-011-L12's post-ship evidence line: MET on macOS after DOCTOR-FALLBACK-STATE-ROOT-1 44f357bb; Linux residual DFSR-LINUX-1; requirement text, criterion and status line unchanged) and `docs/cli/rmap-contracts.md` at 44f357bb (the new `doctor` subsection) with wording corrections 80feaaf4 (pass-1 findings F-01, F-02, F-03) and 47bca4d9 (pass-2 findings F-04, F-05). `docs/requirements/README.md` also changed at 674f9f47 but is not a bootstrap pin.

Operator lesson (recorded in agent-manager memory): shell strings that carry Markdown backticks must be single-quoted or passed through a heredoc.
