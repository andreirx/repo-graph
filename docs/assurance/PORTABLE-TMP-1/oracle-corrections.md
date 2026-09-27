# PORTABLE-TMP-1 — oracle corrections ledger

Append-only (agent-manager docs/MANAGER.md § Oracle corrections).

## OC-1 (2026-09-27, PORTABLE-TMP-1-PREP-2) — PT-C07 asserted a doctor line the fallback path cannot print (→ INPUT-2)

- **Raised by:** implementation review-0 of INPUT-1 (`.agent-manager/slices/PORTABLE-TMP-1/review-0.json`), decision D-PT-C07-ORACLE. **Resolved:** option A, by the operator (in-place-manager), recorded in the PREP-2 packet. The human may override.
- **Check:** PT-C07 (RG-REQ-011-L06, RG-REQ-011-L08, P-PT-01, P-PT-04).
- **Old:**
  - `command`: `… && grep -qF "note: running in sandbox-local mode (state root: $R/state)" "$R/doctor.err" && grep -qF "state_root: override ($R/state)" "$R/doctor.out" && grep -qF 'authority_policy: …' "$R/doctor.out" && …`
  - `inputs`, list "Asserted": `- stdout `state_root: override (/private/tmp/PORTABLE-TMP-1-cliproof/state)` (platform/mod.rs:295-298).`
- **New:**
  - `command`: the one conjunct `grep -qF "state_root: override ($R/state)" "$R/doctor.out" && ` is removed. Nothing else changes. It keeps the stderr fallback note, the absent injection note, the sandbox-local note naming the isolated root, stdout `authority_policy: baselines, aliases, declarations: blocked (sandbox mode)`, `test -d "$R/state/databases"`, the per-uid existence test and the registry digest.
  - `inputs`: the bullet is removed. This sentence is added after the list: "Not asserted (OC-1, docs/assurance/PORTABLE-TMP-1/oracle-corrections.md): the stdout line `state_root: override (…)`, which doctor cannot print in this scenario because `granular_socket_probes()` returns on `ConnectFailed` (platform/mod.rs:223-232) before its `state_root` probe (:284-309), at HEAD and in the candidate; doctor naming the state root under the fallback is the follow-up DOCTOR-FALLBACK-STATE-ROOT-1 (§8)."
  - `expected`: unchanged. It claims only that "doctor reports authority writes blocked", which the failed run printed (log line 22), and names no `state_root` line.
- **Evidence (OBSERVED):**
  - `rust/crates/rgr/src/platform/mod.rs` at HEAD 044e0ebe, :223-232: `SocketConnectResult::ConnectFailed { error, code } => { … probes.push(ProbeResult::fail("socket_connect", msg)); probes.push(ProbeResult::fail("socket_ping", "skipped (connect failed)")); return probes; }`. Probe 5 (`state_root`) is at :284-309, after the return. A mode-000 socket fails `connect` with EACCES, so it takes this arm.
  - The candidate (`.agent-manager/slices/PORTABLE-TMP-1/candidate-admission-1.patch`) has one hunk in that file, `@@ -289,7 +289,13 @@`, which is Probe 5's fallback string. The `ConnectFailed` arm is unchanged, so the line is unreachable in the candidate too.
  - Failed-run log `.agent-manager/slices/PORTABLE-TMP-1/logs/pt-c07-run-20260927T134549Z.txt`:
    - line 20: `  [FAIL] socket_connect: failed: Permission denied (os error 13) (errno 13)`
    - line 21: `  [FAIL] socket_ping: skipped (connect failed)`
    - line 22: `  [ok] authority_policy: baselines, aliases, declarations: blocked (sandbox mode)`
    - `grep state_root` over the log returns no match (rc 1).
  - The review's reading: "`granular_socket_probes()` returns on `ConnectFailed` before it can emit the required `state_root: override (…)` line. This return exists at HEAD. The failed-run log has no `state_root` line."
- **Same class, same pass.** The rule: no part of the slice claims doctor prints the state root during the EACCES fallback. §6's report item "the doctor `state_root: override (…)` and `authority_policy` lines" becomes "the doctor `authority_policy` line". The acceptanceBoundary's "doctor renders it" refers to the authority block, which doctor prints, so it is unchanged. `grep` of the slice for `state_root: override` after the edit finds only the two records of the removal: PT-C07's `inputs` sentence and the §9 entry.
- **Probes (EXECUTED, scratch `/private/tmp/PORTABLE-TMP-1-PREP-2-probe`, removed):**
  - `bash -n` on the corrected command: ok.
  - The assertion chain was run on the real failed-run output: log lines 3-36 as stdout and 37-51 as stderr, with the proof root rebased to the scratch root.
  - The new chain passes (rc 0), and the old chain fails (rc 1).
  - The new chain fails (rc 1) on each of: the fallback note removed; the `authority_policy` line changed; an injection note added; `databases/` absent; the sandbox-local note naming another root.
- **Limitation:** the `state_root` probe's rendering is no longer covered by PT-C07. Doctor naming the state root under the fallback is the follow-up DOCTOR-FALLBACK-STATE-ROOT-1 (slice §8; review-0 option C), outside this allocation.
- **Also resolved in the same review (no ledger text change):** D-PTMP-ROADMAP → A. The fixed per-OS roots of D-PTMP-ROOT-1 govern; the ROADMAP line was corrected by the operator (044e0ebe).
- **Author:** requirements author of PORTABLE-TMP-1-PREP-2 (claude-opus-5-5). **Approver:** in-place-manager (D-PT-C07-ORACLE → A). **Carried as:** INPUT-2.
- **Slice document digests:**
  - at HEAD, as INPUT-1 pinned it: sha256:c3380462d6cd4ffb272640dfac28bd3dc49d8d6c496e66691438ddc276ac0ef2;
  - before OC-1, with the manager's `baselinePath` re-pin: sha256:2c9613d8a8fb32ed67d8449b89baa511a1d9d07d3e887d4a4a03bf8ef73f2955;
  - after OC-1, with the Status line, §6, §8 and §9 edited in the same pass: sha256:8207a7707bb215301702bfe494bc68161fe7ff13f14ea4e231b41503c21c80d1.

  The INPUT-2 manifest's `allocation` digest is re-pinned to the after value.

## OC-1 revision 2 (2026-09-27, PREP-2 cycle 2) — the packet's diff allowance includes §6 (→ INPUT-2)

- **Raised by:** document review-0 of PREP-2 (`.agent-manager/slices/PORTABLE-TMP-1-PREP-2/review-0.json`), decision D-PTMP-PREP2-SCOPE. The PREP-2 VERIFY line limited `git diff 52dd7b72 -- docs/slices/portable-tmp-1.md` to baselinePath, PT-C07, Status, §9 and follow-ups. OC-1 also corrected §6's definition of done, which required quoting the unreachable `state_root` line. The reviewer found the correction necessary and could not pass the scope check against that list.
- **Resolved:** option A, by the operator (in-place-manager), recorded in the PREP-2 packet's CYCLE 2 section: "The VERIFY line's diff allowance was the manager's error. It omitted §6 …". The allowance is now baselinePath, PT-C07, §6's definition-of-done sentence, Status, §9 and follow-ups.
- **Text change:** none. OC-1 above already records the §6 edit ("Same class, same pass"). The slice document, the INPUT-2 manifest and their digests are unchanged by this revision. The slice stays at sha256:8207a7707bb215301702bfe494bc68161fe7ff13f14ea4e231b41503c21c80d1, which is the manifest's `allocation` pin.
- **Re-run of VERIFY under the amended allowance (EXECUTED 2026-09-27):**
  - `git diff -U0 52dd7b72 -- docs/slices/portable-tmp-1.md` has hunks only at 6 (baselinePath), 201 and 204 (PT-C07 `command`, `inputs`), 261 (Status), 444 (§6 definition-of-done sentence), 459 (§8 follow-ups) and 493 (§9).
  - The allocation validator reports `ALLOCATION VALID: 9 checks`.
  - All 14 INPUT-2 pins match the tree.
- **Approver:** in-place-manager (D-PTMP-PREP2-SCOPE → A). **Carried as:** INPUT-2.
