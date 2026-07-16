# TypeScript Prototype — Retired

> **Retirement record.** The TypeScript CLI (`rgr`) prototype was retired and
> deleted from the working tree by **TS-PROTOTYPE-RETIREMENT-1** (2026-07-16).
> This file replaces the former build/run instructions.

## What it was

The original `repo-graph` implementation: a TypeScript CLI (`rgr`) with a
tree-sitter (web-tree-sitter) extractor, a SQLite storage adapter, and a
cross-language detector substrate. It lived in `src/` (≈46k LOC), `test/`
(≈44k LOC), and a detector golden corpus at the repo root (`parity-fixtures/`),
built/tested with root `package.json` / `pnpm` / `tsconfig` / `biome` / `vitest`
tooling.

## Why it was retired

The product was ported to Rust; the `rmap` / `rmapd` Rust workspace is the sole
implementation. The prototype had been legacy since the port (last meaningful
`src/` commit 2026-04-26) and was kept only for port-fidelity parity checks.
Once the Rust surface reached parity — and `rmap map` (MAP-FROM-INDEX-1, v0.7.0)
superseded the last dependence on it — the ~90k LOC of retired code no longer
served a live purpose while dominating every self-index discovery signal
(package groups, complexity centers, import cycles) and taxing every session.
Git history is the archive; nothing is lost by deletion.

## Where it is archived

The full prototype remains reachable in git history at every commit up to and
including its last release. **Last release containing the TypeScript prototype:
`v0.7.0`.** To inspect or recover it:

```bash
git show v0.7.0:src/main.ts
git checkout v0.7.0 -- src test parity-fixtures   # restore into a working tree
```

(The slice spec — written at v0.4.0 — named v0.4.0 as the anticipated last
containing release; in fact the prototype survived, unused, through v0.5.0–v0.7.0,
so the accurate record is v0.7.0.)

## What was kept (relocated, not archived)

Two compile-time data files were **living Rust-system data** stranded in the
prototype tree. They were relocated **byte-for-byte** into their consuming Rust
crates — they are current product inputs, not part of the archived prototype:

- `src/core/seams/detectors/detectors.toml`
  → `rust/crates/detectors/detectors.toml` (embedded via `include_str!`)
- `src/adapters/storage/sqlite/migrations/001-initial.sql`
  → `rust/crates/storage/src/migrations/001-initial.sql` (embedded via `include_str!`)

`docs/cli/v1-cli.txt` is retained as a historical record of the retired TS CLI
contract.
