# RG-REQUIREMENTS-BOOTSTRAP-1 — admission allocation for the first requirements-document item on repo-graph

Status: OPERATOR-PUBLISHED admission allocation (2026-09-13), MANUAL BOOTSTRAP under the human's direction. Maturity: PROTOTYPE (assurance record).

## Purpose

repo-graph has no accepted requirements baseline yet. This document is the role-`allocation` dependency of the operator-published v1 baseline `docs/requirements/baselines/RG-BOOTSTRAP-INPUT-1.json`. It authorizes exactly one requirements-document work item:

- author the stage-3 implementation allocation (`requirements-assurance-implementation-v1` metadata block) at the head of `docs/slices/trust-module-edges-1.md` (the SLICE_DOC), binding TRUST-MODULE-EDGES-1's Implements / Changes / Preserves sets, preservation IDs, acceptance boundary, candidate paths, exclusions, post-review record paths and mandatory checks to the catalog's H/L IDs; and
- author the v2 candidate manifest `docs/requirements/baselines/TRUST-MODULE-EDGES-1-INPUT-1.json` (the REVIEW_BASELINE) whose role-`allocation` dependency is that SLICE_DOC, for independent structured review.

## What it does not authorize

No code change in `rust/`; no edit to any `docs/requirements/rg-req-*.md` file, to `docs/VISION.md`, to `CLAUDE.md`, or to this document; no new requirement ID (a needed refinement is a review finding, not an edit); no commit; no implementation dispatch. The implementation of TRUST-MODULE-EDGES-1 runs only after the v2 review is accepted and the operator records approval.

## Source of authority

`docs/assurance/RG-BOOTSTRAP/human-authorization.md`.

## Amendment 1 (2026-09-18) — successor document items for the ratified queue

Under `docs/assurance/RG-BOOTSTRAP/human-authorization.md` Amendment 1 (the human's 2026-09-14 ratification of the queue order), this allocation also authorizes, one at a time and in the ratified order recorded in `docs/requirements/README.md`, the requirements-document item of each remaining queued slice: authoring that slice's stage-3 allocation block at the head of its SLICE_DOC (`docs/slices/<slice>.md`) and its v2 candidate manifest `docs/requirements/baselines/<WORK-ITEM>-INPUT-n.json` for independent structured review. Everything under "What it does not authorize" stands unchanged for every such item; the amendment is an operator edit under new human authority, not an edit by any relay role.
