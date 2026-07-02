# TS-PROTOTYPE-RETIREMENT-1 — Bury the TypeScript prototype

Status: SPECIFIED (2026-07-02) · Track: Focus / consolidation
Origin: fresh-eyes v0.4.0 review (2026-07-02), operator review

## 1. Problem — 90k LOC of retired code pollutes every discovery signal

The TypeScript prototype (`src/` ≈ 46k LOC, `test/` ≈ 44k LOC, plus
`parity-fixtures/` at repo root) has been legacy since the Rust port
(`docs/ts-prototype.md`: "remains for parity checks and features not yet
ported"; last meaningful commit to `src/` 2026-04-26). The v0.4.0
self-dogfood showed the cost concretely: **every** complexity center, every
top hotspot, and 4 of 6 module cycles reported by `rmap orient` on repo-graph
itself point into the dead TS tree; the package-group list carries ~60+
parity/TS-fixture noise groups. The tool orients agents to a graveyard, and
the repo pays the reading/searching tax on every session.

Git history is the archive: the full prototype remains reachable at every
commit ≤ v0.4.0 (last release containing it). Nothing is lost by deletion.

## 2. Contract

**Verify-then-delete.** The slice is an inventory + mechanical removal, with
an explicit halt if anything load-bearing is found.

**Phase 1 — inventory (read-only).** Enumerate every reference from the
LIVING system to the candidate trees:
- `rust/` workspace (code, tests, build scripts) referencing `src/`, `test/`,
  `parity-fixtures/`, `dist/`, or npm/pnpm tooling. Pay explicit attention to
  the Rust **parity tests** (the release scripts run
  `cargo test -- --skip parity`): determine what they compare against. If
  they consume TS-prototype outputs or `parity-fixtures/`, they are
  port-fidelity scaffolding whose purpose is complete — mark them for
  retirement WITH the prototype, and say so in the build report.
- `scripts/` (e.g. `update-parity-fixtures.py`, `fd-1a-parity-ts-harness.ts`,
  `validate-xpart-fixture.sh`), CI configs, `package.json`/`pnpm` manifests,
  `tsconfig*`, docs (`CLAUDE.md`, `AGENTS.md`, `README`, `docs/**`) and the
  smoke/dogfood harnesses.

**Phase 2 — removal.** Delete: `src/`, `test/`, root `parity-fixtures/`,
TS-only build/test tooling (package manifests, tsconfigs, lockfiles, TS-only
scripts), and the Rust-side parity tests identified in Phase 1 as
prototype-coupled. Update `docs/ts-prototype.md` into a short retirement
record (what it was, why retired, "archived in git history; last release
containing it: v0.4.0"). Update every doc reference found in Phase 1. If
`cargo test -- --skip parity` filters become vacuous, update the release
scripts' comments/flags accordingly (behavior-preserving).

**Out of scope:** `tools/rgistr/` (separate assessment — it had recent churn
and may be live tooling); anything under `rust/` beyond parity-test
retirement and reference updates; the smoke-validation repo inventory.

## 3. Stop conditions

- Any LIVING-system dependency on the deleted trees that is not obviously
  retirement-safe (i.e. anything beyond parity scaffolding and doc
  references) → STOP + DECISION_REQUIRED with the dependency named.
- Any "feature not yet ported" (per `docs/ts-prototype.md`'s claim) that
  turns out to be *referenced by current docs/roadmap as pending port* →
  list them in the build report (do not silently discard claimed pending
  features); if any is load-bearing → DECISION_REQUIRED.
- Do NOT touch `tools/rgistr/`. Do NOT rewrite git history.

## 4. Validation (end-of-slice, synchronous; TEST REPORT)

- `cargo build`, `cargo test`, `cargo fmt --check`, `cargo clippy -- -D warnings`
  green (from `rust/`) — with parity filters updated if tests were retired.
- `./scripts/dogfood-isolated.sh` green.
- `./scripts/cut_release_patch.sh --dry-run` equivalent check if available,
  or manual verification that release-script test invocations still pass.
- **De-noise proof:** isolated self-index — `orient` package groups no longer
  include `src/`, `test/`, or `parity-fixtures/` entries; complexity
  centers/hotspots no longer point into the deleted trees.
- `git status` clean apart from intended deletions; the diff contains no
  `rust/` behavior changes beyond parity-test retirement + reference updates.

## 5. Definition of done

The working tree contains no TypeScript prototype (`src/`, `test/`,
`parity-fixtures/` gone, TS tooling gone, docs updated, retirement record in
place), the Rust workspace builds and tests green, and a self-index orients
to the Rust product code only. Cargo gates green (EXECUTED + reported) +
de-noise proof + dogfood green.

## 6. Notes

- Expected follow-on benefit: METRIC-LANG-COVERAGE-1's coverage caveat and
  rankings become meaningful on self-index (TS noise gone), and repo-wide
  signals (cycles, churn, groups) reflect the product.
- The deletion is large in lines but low in load-bearing assumptions (the
  VISION change-cost doctrine): nothing in the live Rust system should
  depend on it — Phase 1 exists to prove that before anything is removed.
