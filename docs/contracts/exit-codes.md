# `rmap` process exit-code contract

Status: **NORMATIVE** · Ratified by EXIT-CODES-1 (2026-09-08)

This is the single contract for interpreting an `rmap` process exit status.
The JSON body remains the authoritative discriminator when a verdict command
shares a numeric code with a usage or runtime failure.

## Non-verdict commands

| Code | Meaning | Caller action |
|---:|---|---|
| 0 | Success, including an empty or vacuous result. | Consume the output. An empty answer is not an error. |
| 1 | Usage error: invalid arguments, flags, or command form. | Correct the invocation. |
| 2 | Runtime error: for example daemon unavailable, repository not indexed, timeout, malformed response, or storage failure. | Read stderr, restore the runtime precondition, and retry if appropriate. |
| 3 | Still running: the client stopped waiting, but the daemon reports that the asynchronous operation continues. | Follow progress with `rmap doctor`; do not treat this as completion or failure. |
| 4 | Refused by policy: the command understood the request and deliberately declined, with a reason and alternatives. | Consume the stdout verdict. Do not retry unchanged as though the tool had failed. |

`rmap dead` is currently the code-4 command. Its human refusal is written to
stdout. With `--json`, `status: "refused"` and `code: 4` identify the same
verdict.

## Verdict commands

Verdict commands retain their established CI-facing codes. Their codes 1 and 2
therefore cannot be interpreted without the command name and, where available,
the structured `status` or verdict field.

| Command | 0 | 1 | 2 | Ambiguity that callers must handle |
|---|---|---|---|---|
| `rmap doctor` | Healthy | Unhealthy | Not currently emitted as a normal doctor verdict | Invalid arguments also exit 1. Inspect the human/JSON health result to distinguish an unhealthy verdict from usage failure. |
| `rmap gate` | Pass, including no configured obligations (vacuous pass) | Fail | Incomplete | Pre-verdict usage errors also exit 1; runtime failures also exit 2. Inspect `gate.outcome` and `gate.exit_code` in JSON. |
| `rmap check` | Pass | Fail | Incomplete | Runtime failures and a missing/malformed verdict also exit 2. Inspect the `CHECK_*` verdict signal in JSON. |
| `rmap modules violations` | No violations | Violations found | Runtime error | Invalid arguments also exit 1. Inspect `count` in JSON. |
| `rmap hook …` | Success | Non-fatal warning | Fatal error | Invalid hook invocations also exit 1, and internal/runtime failures may exit 2. Inspect JSON `status` when the host transport supplies it. |

The top-level `rmap violations` command is not the `modules violations` verdict
surface; it follows the non-verdict family unless its own structured contract
states otherwise.

## Implementation names

All process-code constants live in
`rust/crates/rgr/src/daemon_command.rs`. General paths use
`EXIT_SUCCESS`, `EXIT_USAGE_ERROR`, `EXIT_RUNTIME_ERROR`,
`EXIT_STILL_RUNNING`, or `EXIT_REFUSED_BY_POLICY`. Verdict paths use the
meaning-specific aliases from the same table (`EXIT_CHECK_*`, `EXIT_GATE_*`,
`EXIT_HOOK_*`, `EXIT_HEALTHY` / `EXIT_UNHEALTHY`, and
`EXIT_NO_VIOLATIONS` / `EXIT_VIOLATIONS_FOUND`).

`rust/crates/rgr/tests/exit_code_contract.rs` enumerates the Rust exit sites and
rejects a numeric literal, a computed value, or `ExitCode::SUCCESS` at a process
conversion boundary. Adding an exit site therefore requires selecting a name
whose semantics match the branch.

## Stream contract

- Successful results and completed verdicts, including policy refusals, use stdout.
- Usage and runtime diagnostics use stderr.
- Exit 3 uses stderr for the progress/lifecycle note because no result has completed.
- `--json` does not change the process code.

