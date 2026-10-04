# D-HSP-PERF-USAGE — the `perf` help line shows the accepted form `rmap perf [--json]`, not the handler's printed `[OPTIONS]`; HSP-C04 and the contract say so for `perf`

Raised: 2026-10-04 by the implementation reviewer of HELP-SURFACE-PARITY-1 (admission 1, cycle 1, codex gpt-6-sol).

**The problem in plain words.** The slice specifies the `perf` help line as `rmap perf [--json]` — the form the handler accepts (`rust/crates/rgr/src/commands/perf.rs:110-125` parses `--json`). The same slice's HSP-C04 (2) says every new help line not backed by a constant equals the handler's PRINTED usage; the `perf` handler prints `Usage: rmap perf [OPTIONS]` (`perf.rs:180-189`). The literal and the oracle contradict each other for this one command; the candidate cannot satisfy both.

**Options (reward / risk).**
- A — keep `rmap perf [--json]` and make HSP-C04 and the contract say "the accepted form" for `perf`. Reward: the agent sees an actionable flag the handler accepts (D-AGENT-USEFULNESS-FRAME-1: the reader phrase is the predicate at the code's site — the parser accepts `--json`); nothing in the line is unsupported. Risk: the help line is not a verbatim copy of the handler's printed summary — stated plainly in HSP-C04 and the contract.
- B — print `[OPTIONS]`, amending the slice's specified line. Reward: verbatim match with the handler's print. Risk: less useful — the agent must run `rmap perf --help` to learn the one option; changes the approved line.
- C — no change. Not acceptable: HSP-C04's equality claim stays false.

**Resolved: 2026-10-04 by the OPERATOR (in-place manager) as A**, the reviewer's recommendation, implied by D-AGENT-USEFULNESS-FRAME-1; the human may override. HSP-C04 (2) and the contract's parity subsection state the `perf` exception by name; every other non-constant line still equals the handler's printed usage.
