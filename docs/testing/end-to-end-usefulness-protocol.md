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
4. **Record** a usefulness report and **file a product follow-up for every gap**.

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

## Output

- A usefulness report — e.g. `docs/testing/usefulness-eval-<track>.md` — table of command × repo ×
  rubric, with the concrete evidence (quote the output, compare to the known truth).
- One product follow-up per gap (overclaim, mis-layer, thin/non-dense output, missing signal).

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
