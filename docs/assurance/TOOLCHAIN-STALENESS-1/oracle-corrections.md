# TOOLCHAIN-STALENESS-1 — oracle corrections ledger

Append-only (agent-manager docs/MANAGER.md § Oracle corrections).

## OC-1 (2026-09-26) — TS-C06's added-line cap on `rgr/src/commands/orient.rs` was 4; the described change takes 5 (→ INPUT-2)

- **Check:** TS-C06. **Old text:** `test "$(git -C .. diff HEAD --numstat -- rust/crates/rgr/src/commands/orient.rs | cut -f1)" -le 4`. **New text:** `… -le 5`.
- **Evidence:** implementation cycle 0 of the first admission (claude-opus-5-5) — the change the allocation describes (two import renames, two render calls moved to the `_at` entry points with `repo_path`) takes 5 added lines because rustfmt splits the orient call (its macro arguments are 62 characters against rustfmt's 60-character call-width default); the only 4-line shapes (an import alias or a wildcard import) make the code less clear to satisfy a count. Every other check passed. Recorded by the builder in `.agent-manager/slices/TOOLCHAIN-STALENESS-1/build-0-admission-1.md` (DECISION_REQUIRED D-TS-C06-ORIENT-CAP).
- **Approver:** in-place-manager (operator).
- **Carried as:** INPUT-2 (the runtime cannot yet chain a corrected allocation digest — MANAGER.md § Oracle corrections point 5, TD-022); delta review accepted at TOOLCHAIN-STALENESS-1-PREP-4 cycle 0. The slice document's `baselinePath` and §9 changed with it.
- **Slice document digests:** before (9a84f07f) sha256:35229be663bcd5585d3ada30d120d2883527cf5339b52daf4ceaf382aaed9e17; after (c574c8a6) sha256:3595717fb558a4338e69b293cc390823d7fc57c4a99aee92615adb2b46956e89.
