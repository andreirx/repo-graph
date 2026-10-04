# D-HSP-L09-SCOPE — HELP-SURFACE-PARITY-1 proves top-level dispatch parity; nested subcommand help is an RG-REQ-012-L09 residual (HELP-SURFACE-PARITY-2)

Raised: 2026-10-04 by the requirements reviewer of HELP-SURFACE-PARITY-1-PREP (cycle 1, review-0, codex gpt-6-sol).

**The problem in plain words.** RG-REQ-012-L09 says help and handlers agree "for every command". The slice's parity test parses the 44 arms of the top-level dispatcher (`rust/crates/rgr/src/main.rs:84`). Source inspection finds dispatched subcommands that `--help` does not list either: `declare waiver|deactivate|supersede` (`commands/declare/mod.rs:58-61,83`), `deps why|drift` (`commands/deps.rs:46-47`), `contracts show|elements|usages` (`commands/contracts.rs:34-36`), `modules show|unowned` (`commands/modules/mod.rs:70,75`), `boundaries show|links` (`commands/boundaries/mod.rs:47,49`), `surfaces show` (`commands/surfaces.rs:48`). A head-only test cannot prove the full leaf.

**Options (reward / risk).**
- A — include the nested subcommands now. Reward: a complete command inventory in one slice. Risk: a wider help and review surface than D-HSP-SCOPE-1 authorized; six more dispatchers to parse.
- B — keep the ratified top-level slice and record nested help as the L09 residual, with the inventory above, in a follow-up HELP-SURFACE-PARITY-2. Reward: the measured top-level defect (nine commands absent, two drift classes) closes without overstating completion. Risk: L09 stays PARTIALLY MET until the follow-up.
- C — keep the full-L09 claim with a top-level test. Not acceptable: a false completeness claim.

**Resolved: 2026-10-04 by the OPERATOR (in-place manager) as B**, the reviewer's recommendation; the human may override. The slice's acceptance boundary, §0, §7 and §9 state the residual; RG-REQ-012-L09's evidence line after the slice reads PARTIALLY MET (top-level), not MET.

**Correction 1 (2026-10-04, operator; after HELP-SURFACE-PARITY-1-PREP cycle 2, author finding F3 / reviewer finding 4).** The residual inventory above is wrong in three places: `boundaries show` and `surfaces show` are ALREADY in `--help` (before capture lines 65 and 63) and are not residuals; `modules boundary` (`commands/modules/mod.rs:74`) is absent from `--help` and was missing from the list. The residual is twelve subcommands: `declare waiver|deactivate|supersede`, `deps why|drift`, `contracts show|elements|usages`, `modules show|boundary|unowned`, `boundaries links`. Option B stands.
