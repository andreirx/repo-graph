# D-HSP-COVERAGE-CONST — the `coverage` handler's usage string becomes a shared constant; `coverage_cmd.rs` is the seventh candidate path of HELP-SURFACE-PARITY-1

Raised: 2026-10-04 by the requirements reviewer of HELP-SURFACE-PARITY-1-PREP (cycle 1, review-0, codex gpt-6-sol).

**The problem in plain words.** The slice says `--help` and the `risk` hint must print the `coverage` handler's own usage line, not a copy. The handler (`rust/crates/rgr/src/commands/quality/coverage_cmd.rs:41-42`) prints an inline literal `usage: rmap coverage <report> [--json]`; there is no constant to share. The slice's six candidate paths (D-HSP-SCOPE-1) do not include that file, so the "not a copy" rule could not be met.

**Options (reward / risk).**
- A — add `coverage_cmd.rs` as a seventh candidate path and introduce one `pub(crate)` usage constant used by the handler, the help text and the `risk` hint. Reward: one source supplies all three consumers; drift becomes impossible, not merely detected. Risk: amends D-HSP-SCOPE-1's path set; the handler's printed usage must stay byte-identical (checked against the before capture `before/coverage-usage.stderr.txt`).
- B — keep six paths and compare the help and hint strings with the handler's actual usage output in a process-level test. Reward: no handler edit. Risk: the strings stay copies; HSP-C04's "not copies" rule has to be weakened.
- C — defer coverage parity. Reward: nothing to change. Risk: RISK-CURSOR-1 (a printed cursor that does not run as printed, RG-REQ-012-L03) stays open.

**Resolved: 2026-10-04 by the OPERATOR (in-place manager) as A**, the reviewer's recommendation; the human may override. Same rule for `declare boundary`: its two usage literals (`boundary.rs:65`, `:74`) become one `pub(crate)` constant, output byte-identical.

**Correction 1 (2026-10-04, operator; after HELP-SURFACE-PARITY-1-PREP cycle 2).** Option A stands in substance — one constant per usage text, no copies — but the constants live in `cli/usage.rs`, not in the handlers: the handler modules are private and `cli`/`presentation` must not import `commands` (D-HSP-CONST-DIRECTION, option A). The handlers import the constants from `crate::cli` and print them unchanged; the `risk` command passes the coverage usage text to its renderer. The seventh path `coverage_cmd.rs` stays; `cli/mod.rs` and `commands/quality/risk.rs` join the allocation.
