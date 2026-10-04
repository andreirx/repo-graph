# D-DFSR-FALLBACK-NOTE-1 — after an `auto` EPERM fallback to stdio on the configured global root, the global-service probe keeps today's pass/fail; the note rule is one predicate — forced stdio or `RMAP_STATE_ROOT` set

Raised: 2026-10-05 by the requirements reviewer of DOCTOR-FALLBACK-STATE-ROOT-1-PREP (cycle 2, DFSR-D02).

**The problem in plain words.** The slice said the service probe becomes a passing note for forced stdio or a configured non-global root, and §2.1(4) also applied the note TONE to `StateRootMode::SandboxLocal`. `SandboxLocal` can arise from an `auto` connection that hits EPERM and falls back to a stdio subprocess with NO `RMAP_STATE_ROOT` set (`daemon_client/mod.rs:151-204`). In that state the probe would stay a counted pass/fail (the predicate "forced stdio or `RMAP_STATE_ROOT` set" is false) while its rendered line said `[note]` — an unhealthy verdict with no visible failure line. RG-REQ-011-L12 revision 6 speaks only of forced stdio and a configured non-global root; it does not settle the fallback case.

**Options (reward / risk).**
- A — retain today's behaviour after an `auto` EPERM fallback: the service probe is a counted pass/fail with the new label and no detail; ONE predicate — the transport is forced stdio, or `RMAP_STATE_ROOT` is set — controls both `passed` and the tone, and `SandboxLocal` reached through a fallback is NOT in it. Reward: no hidden failure; verdict and display always agree; exactly what L12 revision 6 binds. Risk: a fallback run still judges the global service although it used stdio — recorded as a limitation and a follow-up (DFSR-FALLBACK-2).
- B — a passing note after a fallback too. Reward: the verdict concerns the daemon the client used. Risk: the connected client's active transport must reach the platform probe and the tone step — a new data flow and a new truthful detail; a revised L12. Beyond RC-4.
- C — defer the item. Risk: the isolated `doctor` stays misleading meanwhile.

**Resolved: 2026-10-05 by the OPERATOR (in-place manager) as A**, the reviewer's recommendation; the human may override. The slice's tone step keys on the same predicate as the probe's `passed`, never on `SandboxLocal`; DFSR-C01 tests the fallback state (`Auto` + `SandboxLocal`, no override) as a counted pass/fail with `Ok`/`FAIL` tone.
