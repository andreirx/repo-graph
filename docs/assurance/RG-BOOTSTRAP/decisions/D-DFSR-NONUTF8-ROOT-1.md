# D-DFSR-NONUTF8-ROOT-1 — `doctor` reads `RMAP_STATE_ROOT` with the client's own predicate; a value that is not UTF-8 is reported as a failure that names the fact, never as an override

Raised: 2026-10-05 by the requirements reviewer of DOCTOR-FALLBACK-STATE-ROOT-1-PREP (cycle 6).

**The problem in plain words.** The slice said the early `state_root` probe reads `std::env::var_os("RMAP_STATE_ROOT")` and displays a non-UTF-8 value lossily as an override. The client decides its state root with `std::env::var("RMAP_STATE_ROOT").is_ok()` (`daemon_client/mod.rs:154`, `stdio_transport.rs:179-180`): a value that is not UTF-8 is `Err(NotUnicode)` and is treated as UNSET — the client uses the global root (or a sandbox-local root after an `auto` fallback). The probe would then claim an override the client does not use: a false state-root claim (RG-REQ-011-L12, RG-REQ-002-L02).

**Options (reward / risk).**
- A — change the client to honour a non-UTF-8 value. Risk: the client is a frozen path in this slice; wider scope.
- B — report the configured value and mark the active root unknown until a connection establishes it. Reward: no false claim. Risk: revises L12's early-output rule; the agent still cannot act on "unknown".
- C — defer. Risk: the isolated `doctor` defect stays.
- D — read with the client's predicate (`std::env::var`), so the probe and the client always agree: `Ok(path)` → `state_root: override (<path>)`; `Err(NotPresent)` → `state_root: global`; `Err(NotUnicode)` → a FAILED probe `state_root: RMAP_STATE_ROOT is set but is not valid UTF-8; the client ignores it and uses the global root` (counted — a misconfiguration the operator must see). Reward: one predicate shared with the client, so no divergence is possible; the malformed case is visible, not hidden. Risk: L12 gains one output form (revision 9) and one test.

**Resolved: 2026-10-05 by the OPERATOR (in-place manager) as D**, the reviewer's option B's goal (no false claim) reached without an "unknown" that the agent cannot act on and without changing L12's early-output rule; the human may override. RG-REQ-011-L12 revision 9 binds the three forms; DFSR-C01 tests all three.

**Correction 1 (2026-10-05, operator; bootstrap re-pin review INPUT-28 pass 1).** Option D's failed-probe text said the client "uses the global root"; the code only establishes that the client IGNORES a non-UTF-8 value (`daemon_client/mod.rs:154`, `stdio_transport.rs:179-180`) — under an `auto` fallback it prepares a sandbox-local root (`daemon_client/mod.rs:192-204`). The text reads `RMAP_STATE_ROOT is set but is not valid UTF-8; the client ignores it (global root, or a sandbox-local root after an auto fallback)`. The override precondition in L12 reads "present and read as valid UTF-8"; L06 is qualified the same way.
