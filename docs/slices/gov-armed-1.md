# GOV-ARMED-1 — the governance quartet says "not armed" in one line, never a page of zeros

Status: SPECIFIED (2026-08-28) · Track: Usefulness audit v0.9.0 fix queue, item #9. CODE slice,
presentation-layer. Maturity: MATURE surfaces (gate is CI-facing — exit codes frozen).

## 1. Problem (measured — audit §9; re-verified live 2026-08-28 on leveldb)

The four governance surfaces (`gate` / `assess` / `violations` / `modules-violations`) render an
UNARMED repo (nothing configured/declared) as pages of zeros — and worse, as success:
- `gate`: "Outcome: pass" + a 6-row all-zero obligation table. An unarmed gate is
  byte-indistinguishable in spirit from a configured-and-clean one; a CI reader learns nothing.
- `violations`: "No violations detected." with 0 declarations — reads as a clean architectural
  bill of health when NOTHING has ever been checked.
- `assess`: "0 policies evaluated" under a header, dressed as a result.
41 lines of zeros across the quartet to say one true sentence: "this surface is not armed."
Unarmed-vs-clean is precisely a certainty claim, and today it is collapsed.

## 2. Contract

1. **Unarmed → one honest line per command** (plus the arming CTA), no zero tables:
   - gate: `Gate: not armed — no obligations or quality policies configured for this repo.`
     plus outcome line `Outcome: pass (vacuous — nothing was evaluated)`. Exit code UNCHANGED
     (frozen CI semantics); the word "vacuous" carries the honesty.
   - assess: `Assess: not armed — no quality policies configured for this repo.`
   - violations: `Violations: not armed — no boundary declarations exist for this repo;
     nothing has been checked.`
   - modules-violations: same pattern over its input kind.
   Each names the arming path (the real command/config, verified from code — not guessed).
2. **Unarmed ≠ armed-and-empty.** Armed with zero findings renders the evaluated counts
   explicitly: e.g. `N obligations evaluated: all pass`, `N declarations checked: no
   violations`. Zero-count rows inside an ARMED render may stay if they carry information;
   the collapse applies ONLY to the unarmed state.
3. **The unarmed determination is a fact, not an inference from zeros.** Use the payload's
   configuration-presence facts (the daemon already knows — gate's current footer proves it).
   If any command's payload cannot distinguish "no policies configured" from "policies
   configured, zero obligations produced", extend the payload ADDITIVELY (pre-ratified DTO
   field) — never infer unarmed from `total == 0`.
4. **JSON unchanged or additive only** (gate's JSON is CI-facing); human render is the target.
   Contract docs updated in-slice.

## 3. Stop conditions

Frozen: all four exit-code semantics, gate's JSON fields (additive only), storage schema,
trust, LiveGraph/witness. STANDING HONESTY RULES apply (the armed/unarmed read is rendered —
unknown-with-reason on failure, never defaulted to either state). Unmet DoD → STOP +
DECISION_REQUIRED. Never touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit per command: unarmed → the one-liner (assert the zero table is GONE and the CTA names
  the real arming path); armed-and-clean → explicit evaluated counts; armed-with-findings →
  unchanged; determination-read failure → unknown-with-reason.
- Live proof (isolated state root, registry sha unchanged): leveldb (unarmed) before/after
  captures for all four; one ARMED capture (configure a minimal policy/declaration on a
  fixture or validation repo the way the docs say) proving the armed render still carries its
  counts. Exit codes asserted unchanged in both states.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

An unarmed governance surface states its unarmed truth in one line with the arming path; armed
output is explicit about what was evaluated; the two states can never be confused; exit codes
and JSON contracts untouched (or additive); gates green.
