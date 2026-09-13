# Human authorization — repo-graph requirements catalog and the first requirements-grounded slice

Date: 2026-09-13. Recorded by: the in-place manager. Mode: MANUAL BOOTSTRAP (agent-manager stages 0–3 accepted; operator-recorded approval per D-AUTH).

## Source

The human instructed on 2026-09-12: "in this new paradigm YOU ARE THE MANAGER. first thing before running any relay with any slice — we need to bridge the gap that we have — WRITE THE REQUIREMENTS FOR REPO-GRAPH — a new folder docs/requirements, and one file per high level requirement, containing all the related low level requirements inside"; and "the point is to ground the proposed slices for addressing the problems and regressions on to the requirements and let's see if we can prevent other regressions".

On 2026-09-13 the human instructed: "remove that latest requirement [RG-REQ-016] and we will refactor on a case by case basis going forward. NOW — on the proposed fixes slices — let's do one of them and see how the new agent-manager setup holds".

## What this authorizes

- The requirements catalog at `docs/requirements/` (15 H / 152 L, revision after the 2026-09-12 independent review by Codex gpt-5.6-terra, verdict REFINE, findings applied; RG-REQ-016 retired) serves as the input baseline for ONE requirements-grounded slice run under agent-manager's accepted assurance stages. Approval of this revision's content by the human is operator-recorded under this instruction; it is not a human line-by-line ratification of every L. The open decisions named in `docs/requirements/README.md` (RG-REQ-008-L01, RG-REQ-006-L11, RG-REQ-011-L11, RG-REQ-009-L01, RG-REQ-010-L10/L11) are PROPOSED and are not needed by the selected slice.
- The selected slice is TRUST-MODULE-EDGES-1 (queue Q2): bounded (one SQL statement, its tests, one seam test), CRITICAL (a regression on ten repositories), and independent of every open decision. The manager chose it over Q1 for the first trial because its regression watch is small enough to observe end to end; Q1 follows.
- Builder and reviewer: Codex CLI gpt-5.6-sol (builder) and Codex CLI gpt-5.6-terra (reviewer), high effort — the combination under which agent-manager's stages 1–3 were exercised and accepted; recorded as same-provider, separate-invocation independence. This departs from the 2026-09-08 two-vendor assignment (reviewer claude-opus-4-8) for this trial only, so that the first run tests the accepted setup rather than an untested adapter pairing; the manager states this in the report.

## Limits

No release, deployment, install, global default change, or migration of other targets. No commit by any relay role; the operator commits deliverables after review and operator checks. Stage-4 acceptance and recovery are not delivered; final acceptance remains a manager-recorded bootstrap act.
