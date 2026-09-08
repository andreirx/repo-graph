# EXIT-CODES-1 — one exit-code contract; a refusal is not an error

Status: SPECIFIED (2026-09-06) · Track: audit round five, group G3 (human-ratified 2026-09-06:
"ok for standardized exit codes"; supersedes the 2026-04-27 freeze of `dead`'s exit 2 —
recorded here as the decision record). CODE slice, rgr commands + one contract document.
Maturity: MATURE (every agent that shells out to rmap reads `$?`).

## 1. Problem (ROOT-CAUSED — `docs/audits/2026-09-06-root-causes-v0.17.0.md` §F "dead exit")

Outward surface: an agent running `rmap dead` under `set -e` sees exit 2 and the prefix
`error:` — the code the product's only definition (`rgr/src/daemon_command.rs:38-52`:
0 success / 1 usage / 2 runtime error "daemon unavailable, repo not found, timeout" /
3 still running) reserves for infrastructure failure — for a deliberate, correct policy
refusal (`commands/dead.rs:49,70,98`; frozen 2026-04-27). The agent cannot distinguish "the
tool refused, here is why and what to do instead" from "the tool is broken", so the product's
best refusal is machine-illegible. No exit-code contract is documented anywhere (no
`docs/contracts/`; README silent; only `doctor --help` and a TECH-DEBT note mention codes).
Second instance, cause UNDETERMINED: the smoke recorded django `orient --full` exiting 2 on
a non-error path (`django-meta.json`); the builder root-causes it first (§2.1).

## 2. Contract

1. **Root-cause the django `orient --full` exit 2 FIRST** (read the smoke's stderr capture;
   reproduce isolated; name the branch). If it is a real runtime error, fix or file it; if it
   is a refusal/limit path, it moves to the refusal code below.
2. **The contract, documented once.** `docs/contracts/exit-codes.md` (new; the target's
   first contract document — placed where the operator practice says contracts live) states:
   `0` success (including vacuous/empty results — `gate` vacuous pass stays 0) · `1` usage
   error · `2` runtime error (daemon unavailable, repo not found, timeout, storage failure) ·
   `3` still running (async) · `4` **refused by policy** — the command ran, understood the
   request, and declined with a stated reason and alternatives (`dead` today; any future
   disabled-by-decision command). Constants live in ONE place (`daemon_command.rs`); every
   command's exit path uses them; `--help` for `dead` and the README name the codes.
3. **Refusals print to stdout as a verdict, not stderr as an error.** `dead`'s refusal text
   (the FP-rate quantification, per-snapshot causes, four alternatives, re-enable condition —
   the audit's best output) is unchanged in content, loses the `error:` prefix, and goes to
   stdout; `--json` carries `{"status":"refused","code":4,…}` additively.
4. **Machine-verifiable.** A test enumerates every `ExitCode::from(...)` / `process::exit`
   site in rgr and asserts it uses a named constant; the smoke harness's per-repo meta
   records the code and the summary lists any command exiting 2 for later triage.

## 6. Amendment after cycle 1 (2026-09-08; the Codex builder's finding, reviewer-verified)

§2.2's single five-meaning table contradicts §3: four existing VERDICT commands already use 1/2 as
documented verdicts, not errors — `doctor` 1 = unhealthy (`doctor/mod.rs:136`), `modules violations`
1 = violations found (`violations.rs:93-96`), `hook` 1 = warning / 2 = error (`hook/mod.rs:31-33`),
`gate` fail 1 / incomplete 2 / pass 0 (`gate/compute.rs:623-644`) — and stamping `EXIT_USAGE_ERROR` on
them would be a false name. RULING (mapped outward: an agent reading `$?` must never confuse "the tool
broke" with "the tool answered no"): the contract has TWO FAMILIES, both documented in
`docs/contracts/exit-codes.md`. (a) Non-verdict commands: 0 success (incl. vacuous/empty) · 1 usage
error · 2 runtime error · 3 still running · 4 refused by policy. (b) Verdict commands (`doctor`, `gate`,
`check`, `modules violations`, `hook`): 0 = the positive verdict, 1/2 = the documented negative verdicts
listed per command in the contract (unchanged — §3 froze them); a RUNTIME error inside a verdict command
also exits 2 today — the contract states that ambiguity honestly per command rather than hiding it, and
`--json` carries `status` so automation can disambiguate. Constants are named by MEANING at each site
(`EXIT_UNHEALTHY`, `EXIT_GATE_FAIL`, `EXIT_GATE_INCOMPLETE`, `EXIT_VIOLATIONS_FOUND`, `EXIT_REFUSED_BY_
POLICY`, …) from ONE table, and the enumeration test asserts every exit site references a named constant
— never a bare integer, never a name that lies. `gate.rs:36-40`'s "3: gate fail" doc is corrected to the
producer's truth (1 fail / 2 incomplete). `dead` → 4 with its verdict on stdout, as §2.

## 3. Stop conditions

Frozen: wire protocol (the JSON `status` field is additive), the meaning of 0/1/2/3 for every
existing command (no command changes its code except `dead` 2→4 and whatever §2.1 proves
mis-coded — each such change listed in the build report with its before/after). If §2.1's
cause needs a storage/daemon change, STOP + DECISION_REQUIRED. STANDING HONESTY RULES.
Unmet DoD → STOP + DECISION_REQUIRED. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

Unit FIRST: `run_dead` output has no `error:` prefix, exit is `EXIT_REFUSED_BY_POLICY`; the
constant-usage enumeration test; `gate` vacuous pass still 0. Live (isolated): `rmap dead;
echo $?` → 4 with the verdict on stdout; django `orient --full` reproduced and explained;
`rmap dead --json` carries the status. Gates recorded FIRST; chunked cargo; witness;
dogfood-isolated.

## 5. Definition of done

The contract document exists and is the only definition; `dead` exits 4 with its verdict on
stdout; the django exit-2 path is explained or fixed; every exit site uses a named constant;
gates green.

CORPUS PATHS: django at ../legacy-codebases/django; repo-graph is THIS repo.
