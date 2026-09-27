# DOCS-UNREADABLE-DECODE-1 — oracle corrections ledger

Append-only (agent-manager docs/MANAGER.md § Oracle corrections).

## OC-1 (2026-09-27) — isolated rmap invocations lacked `RMAP_AUTO_REINDEX=off` (→ INPUT-2)

- **Old:** `RMAP_AUTO_ENRICH=off`. **New:** `RMAP_AUTO_ENRICH=off RMAP_AUTO_REINDEX=off` (2 sites). **Evidence:** TOOLCHAIN-STALENESS-1 (`23d12a9b`) re-indexes a store whose stamp differs on the first request (D-STALE-SIGNAL-1). **Also:** before-roots re-captured 2026-09-27 with the release binary of HEAD fbd53845. **Approver:** in-place-manager. **Carried as:** INPUT-2.
- **Slice document digests:** before sha256:90e6ea6f85d6c98cb87193df5e85a836c66b2ccaa1a335499828474296afa506; after sha256:51f907570943b398ac0d5d9996365f6427081e9703ab7c7a80634448a113e023.

## OC-2 (2026-09-27) — moved literals found by the INPUT-2 verification (PREP-5; → INPUT-2)

- **Old → New:** §5 step 0 "`git diff --stat v0.19.0 HEAD -- rust` is empty" → recorded, NOT empty at fbd53845 (36 files, none under doc-facts nor the rgr docs files); DU-C04 environment and §7 before-root provenance "indexed 2026-09-22T23:40Z … v0.19.0-labelled binary" → re-captured 2026-09-27T01:19Z/01:20Z with `rust/target/release/rmap` (self-reports `rmap 0.19.0`; building source per the manager's record, not attested); §1 capture `/private/tmp/DOCS-UNREADABLE-DECODE-1-poco-before/docs-before.txt` (lost) → the PREP-5 reproduction; dispatch.rs 5678/5700-5713/5711/5808 → 5693/5715-5728/5726/5823 (§2.1, §2.3, DU-C04 inputs). Added rule: every check command is a bash command (§5, DU-C04 environment). No check command changed. **Evidence:** EXECUTED on copies of both before-roots — installed comparator and HEAD release build: poco `docs list`/`--json` exit 2 `missing field `content_hash``; leveldb identical; under zsh every DU-C04 `run` exits 1 `unknown command` (unsplit `$4`). **Approver:** pending the delta review. **Carried as:** INPUT-2.
- **Slice document digests:** before sha256:51f907570943b398ac0d5d9996365f6427081e9703ab7c7a80634448a113e023; after sha256:32b82e01ccef05f1a1d65bc42b7a5b161dec663daa80a697f379174eeed69f77.

## OC-3 (2026-09-27) — DU-C04 served the manager's before-roots in place (review F-RG-REQ-011-L06; → INPUT-2)

- **Old:** every comparator/candidate `run` passed `$R-poco-before` / `$R-leveldb-before` as `RMAP_STATE_ROOT`. **New:** each binary serves its own copy `$R-copy-<repo>-<cmp|cand>` (registry `db_path` redirected and asserted); the originals are fingerprinted before and asserted unchanged; the EXIT trap removes the copies; DU-C05 asserts none remain; DU-C04 also carries RG-REQ-011-L06 and P-DU-03. **Evidence:** packet stop condition "the manager's before-roots are read-only (copies only)"; CLAUDE.md "a retained state root is not read-only under a serving daemon"; mechanics dry run EXECUTED under bash (fails, as it must before a candidate exists, at `('exit', '/tmp/du-poco-after.txt', 2, …)` after the fingerprint assertion passed; copies removed; originals unchanged). **Approver:** pending the delta review. **Carried as:** INPUT-2.
- **Slice document digests:** before sha256:32b82e01ccef05f1a1d65bc42b7a5b161dec663daa80a697f379174eeed69f77; after sha256:5eae7ea5bde00539bc6ea08c35802fcbb12cce23f4ff266b8f992cc11ead4f78.

## OC-4 (2026-09-27) — DU-C02 compared a locale-sorted list with a C-collation literal (→ INPUT-3)

- **Old:** `git diff HEAD --name-only --relative -- crates/rgr | sort | tr ...`. **New:** `... | LC_ALL=C sort | tr ...`.
- **Evidence:** implementation review of the INPUT-2 admission, cycle 1 (codex gpt-6-sol), F-DU-C02-1 / D-DU-C02-COLLATION: the verbatim command exited 1 under `en_US` collation on the correct two-file rgr diff and 0 with `LC_ALL=C`; the failed run is preserved in `.agent-manager/slices/DOCS-UNREADABLE-DECODE-1/admission-1/`.
- **Approver:** in-place-manager. **Carried as:** INPUT-3.
- **Slice document digests:** before sha256:5eae7ea5bde00539bc6ea08c35802fcbb12cce23f4ff266b8f992cc11ead4f78; after sha256:abfa134934b4be48d7449209f876e3627b6ef0eca11d4af6a2214e402316cfe0.
