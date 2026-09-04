# End-to-End Usefulness Protocol

**Maturity: PROTOTYPE** (the discipline is ratified; the reference set + rubric will sharpen with use)

## When

After completing a **major branch or track of work** (a multi-slice arc), **before merge** — in
addition to the per-slice [End-of-Slice Procedure](end-of-slice-procedure.md). The per-slice
procedure proves each change is *correct* and the software *runs*; this protocol proves the
**product still delivers useful, honest orientation** to a consuming agent.

## Why

`cargo test`, clippy, and a "does it run" smoke prove **execution**, not **value**. repo-graph is a
tool *for AI agents*; it must be judged by what it **delivers to a consuming agent on a real,
KNOWN codebase** — does the output help the agent look in the right places, is it true, is it
honest about what it does not know. That is the VISION's actual test (Orientation over Perfection),
and it **cannot be unit-tested**: it requires an agent (or human) who already knows the reference
codebase to read the output and judge it against ground truth.

This protocol exists because a green test suite and a passing smoke can coexist with output that is
thin, misleading, or mis-layered. (Discovery example: a full smoke ran green while `orient` led with
tool-confidence instead of structure, `modules` reported low-reliability graph-orphans as flat "dead"
counts, and `stats By fan-in` showed all-zeros on C — each one would steer an agent wrong, none
caught by "does it run".)

## What to run

1. **Index a curated set of WELL-KNOWN reference repos** — architecture publicly known, diverse
   languages, so the evaluator can judge output against real knowledge. Suggested set:
   - `nginx` (C, event-driven web server)
   - `sqlite` (C, embedded database)
   - `django` (Python, web framework)
   - `duckdb` (C++, analytical database)
   - `kafka` (Java, distributed log)
   - a representative project in the **primary target language** (TypeScript) so the
     primary-substrate path is exercised, not only the degraded ones.
2. **Run the FULL command surface at FULL output** on each. The capture harness is
   `scripts/smoke-validation-repos.sh` configured with the comprehensive command set and
   `orient --full` (no `--budget small`). It writes per-command output to `smoke-runs/<ts>/`.
   Exercise every repo-wide command, not a historical subset — the surface grows; the test must
   grow with it.
3. **Evaluate** each command's output against KNOWN ground truth, per the rubric below. The
   evaluation is a JUDGMENT by an agent/human who knows the repo — not an assertion a script can make.
4. **Run the track-level evaluation gate** (below) — two-agent (evaluator + reviewer take) over
   VISION alignment, current-architecture fit, and net tech-debt balance.
5. **Record** a usefulness report (per-command rubric + the track-level gate) and **file a product
   follow-up for every gap**.

## The rubric (score each command × repo)

| Question | A "pass" looks like |
|---|---|
| **TRUE?** | Output matches what the repo actually is — real structure, real complexity centers, real deps. |
| **USEFUL?** | Helps the agent *look in the right places / open the right files / ask the right questions* (the VISION) — orientation, not meta-about-the-tool. |
| **DIRECTIONALLY CORRECT?** | Steers toward the real structure; never toward a false path (an agent acting on it is not misled). |
| **HONESTLY LAYERED?** | Extracted fact ≠ inference ≠ hint ≠ governance; no Layer-2/3 rendered as Layer-0; degradation/unknowns labeled (Fact-Certainty Model). |
| **DENSE?** | A budgeted command spends its tokens on the **most load-bearing facts**, not a truncated list of thin meta. Budget should trade DEPTH, not strip information. |
| **COMPLETE-ENOUGH?** | Name what a consuming agent would still need that is absent (the gap, explicitly). |

A command can be honest (passes layering) yet useless (fails usefulness/density) — score every row.

## Grader ground-truth discipline (added 2026-09-04)

A grader's "fabrication" verdict is itself a claim that must survive falsification.
Bitten: the v0.16.0 matrix grader declared vscode's `.env` resource row invented after a
grep that excluded `.mts`/`.cts`/`.mjs` and script directories; four literal
`fs.*('.env')` sites existed (`extensions/copilot/script/setup/*.mts`). The product was
right; the audit was wrong; a relay builder caught it by refusing to suppress evidenced
rows. Rules: (1) a negative claim ("no X exists") must state the exact search (tool,
pattern, include/exclude globs) so it can be re-run; (2) ground-truth greps include every
source extension the product indexes for that language (TS: ts/tsx/mts/cts/js/mjs/cjs)
and do not exclude `script/`, `tools/`, config roots; (3) any grader claim that a slice
will act on is re-verified by the operator with the cheapest independent check BEFORE it
enters a spec — a spec built on a false negative wastes a relay cycle and, worse, orders
the removal of true facts.

## Track-level evaluation — the gate (two-agent)

The per-command rubric scores individual outputs. On top of it, the track **as a whole** must pass a
track-level evaluation before it is "done" / merge-ready — and this evaluation is **two-agent**
(analogous to the relay's `decision-review`, though lighter — no mandatory rebuttal round): an
**evaluator** agent produces the analysis, and the **reviewer model gives an independent take that
challenges it** (the evaluator MAY rebut). Disagreement is surfaced to the human, not averaged away.

Four questions — all answered, with evidence:

1. **VISION alignment.** Does the track's net effect serve the stated VISION (`docs/VISION.md`) —
   orientation an agent can trust, honest layering, the right knowledge at the right layer? Cite the
   VISION sections; name any output that contradicts them.
2. **Current-architecture fit.** Do the outputs respect and advance the CURRENT architecture
   (`agent_docs/architecture.md` + `docs/architecture/*` + the VISION's Product Layer Model — the
   layer model, the daemon model, the boundaries)? Surface any drift or architecture violation the
   track introduced or revealed.
3. **Net tech-debt balance.** Across the track, is debt **net-removed**? List both sides explicitly —
   debt RESOLVED (entries closed in `docs/TECH-DEBT.md`) vs debt INTRODUCED (new entries, deferred
   work, shortcuts). **Score by SEVERITY, not count** (TECH-DEBT uses P1/P2/P3): introducing a debt
   of severity ≥ the highest resolved this track means the track is **net-debt-positive and does NOT
   pass** — e.g. a new P1 requires a P1 resolved (or an explicit human waiver, below). (Cost is
   load-bearing assumptions disturbed, not lines of code.)
4. **Two-agent verdict.** The evaluator's analysis of (1)–(3) plus the reviewer model's independent
   challenge of it. Converged → pass; contested → the human adjudicates (the evaluator may have
   missed a violation, or the reviewer may be wrong — show both arguments).

This is a **GATE with one explicit escape**: a major track is not merge-ready until the track-level
evaluation **passes** — OR a failing/contested result is **explicitly waived by the human** (each gap
filed in `docs/TECH-DEBT.md` and the waiver recorded). "Passes" and "human-waived" are the only two
ways forward; an unaddressed failure is NOT a pass. It is the track analogue of the relay's
per-decision review (see agent-manager `CLAUDE.md` → decision-review).

## Output

- A usefulness report — e.g. `docs/testing/usefulness-eval-<track>.md` — table of command × repo ×
  rubric, with the concrete evidence (quote the output, compare to the known truth).
- The **track-level gate** result: VISION alignment, current-architecture fit, and the **net
  tech-debt balance** (debt resolved vs introduced, both sides named) — plus the **two-agent verdict**
  (evaluator analysis + reviewer challenge; converged or contested).
- One product follow-up per gap (overclaim, mis-layer, thin/non-dense output, missing signal,
  architecture drift, net-debt-positive).

## First run (arch/scip-substrate-pivot)

The first application of this protocol surfaced nine findings — consolidated in
[`../TECH-DEBT.md`](../TECH-DEBT.md) § *Pre-Merge Hardening + E2E Usefulness
Findings*. Two came straight from the rubric on spring-petclinic: `orient` reports
`1 module` where `stats` reports `11` (TRUE / DIRECTIONALLY-CORRECT fail — a
structurally wrong model on the primary surface), and `stats total_symbols: 0` vs
`orient`'s `290` (HONESTLY-LAYERED fail — an unpopulated metric rendered as a fact).
Both coexisted with a green `cargo test` and a passing smoke — which is exactly why
this protocol exists.

## Relationship to the other procedures

- **Per slice** → [`end-of-slice-procedure.md`](end-of-slice-procedure.md): Test → Install/deploy →
  Cleanup. Mechanical correctness of one change.
- **Per track (this)** → does the product, as a whole, still deliver **useful + honest** orientation
  to an agent on a codebase it does not already know.

Honest **and** dense. Truth without usefulness is a footnote; usefulness without truth misleads.
