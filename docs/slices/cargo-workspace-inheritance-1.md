# CARGO-WORKSPACE-INHERITANCE-1 — resolve `[workspace.package]` inheritance in the cargo manifest reader

Status: **SPECIFIED** (authorized by ratification 2026-07-11/12, during MODULE-MODEL-2
review-0 and review-2; option A of the CARGO-WORKSPACE-INHERITANCE DECISION_REQUIRED).
Track: Extraction correctness (Rust first-class).
Origin: MODULE-MODEL-2 self-dogfood — 6 of repo-graph's own 50 crates invisible to the
module model (`docs/slices/module-model-1.md` §13 delivery record; `.agent-manager/slices/
MODULE-MODEL-2/build-0.md` §5 for the builder's root-cause).

## 1. Problem

`crates/indexer/src/cargo_manifest.rs` emits a `module_candidate` (evidence
`source_type='cargo_toml'`) only for crates whose `Cargo.toml` carries explicit package
fields. Crates using **workspace field inheritance** — `version.workspace = true`,
`edition.workspace = true`, … resolved against the root `[workspace.package]` — produce NO
candidate. On repo-graph itself that hides 6 of 50 crates (`rgr`, `daemon-runtime`,
`coverage`, `graph-algorithms`, `platform-paths`, `rmapd`); most modern Rust workspaces
inherit, so on the deployment-target monorepo the gap scales. Downstream, EVERY consumer of
`module_candidates` mis-models those crates: orient/stats show directory fragments instead
of crate groups (MODULE-MODEL-2 degrades honestly, by ratified design), and
`modules`/`trust` omit them entirely.

## 2. Contract

1. The cargo manifest reader resolves workspace inheritance: a manifest with
   `package.<field>.workspace = true` (at minimum for the fields whose absence currently
   suppresses the candidate) yields a `module_candidate` with `canonical_root_path` at the
   crate dir and the same evidence shape as an explicit manifest. Resolution uses the
   enclosing workspace root's `[workspace.package]` table; a missing/garbled root table →
   honest skip (no fabricated candidate), logged in extraction diagnostics.
2. A `[workspace]`-only virtual manifest (no `[package]`) still yields NO crate candidate
   (current correct behavior — regression-tested).
3. Fix ALL consumers uniformly by fixing the ONE producer — no fold-side or per-consumer
   workarounds (rejected during MODULE-MODEL-2 as re-introducing cross-command incoherence).

## 3. Stop conditions

- Touches ONLY the cargo manifest reading path + its tests. No module-identity/key scheme
  changes beyond the new candidates appearing; no fold/renderer changes (MODULE-MODEL-2's
  machinery picks the new facts up as-is).
- Do NOT commit.

## 4. Validation (SYNCHRONOUS; TEST REPORT INLINED)

- Cargo gates green from `rust/` (build / full workspace test / fmt / clippy -D warnings).
- Named tests: inheriting crate → candidate emitted with correct root + evidence; explicit
  crate unchanged; virtual root manifest → no candidate; missing `[workspace.package]`
  field referenced by `.workspace = true` → honest skip + diagnostic.
- Isolated self-dogfood (/private/tmp state root; NEVER the real registry): index
  repo-graph, then `orient` + `modules list` name ALL 50 crates — specifically `rgr`,
  `daemon-runtime`, `coverage`, `graph-algorithms`, `platform-paths`, `rmapd` fold as
  crate groups and the MODULE-MODEL-2 directory-fragment degradation for them disappears;
  transcript inlined. (Deep-vertical rule: the output surface proves the fix.)

## 5. Definition of done

repo-graph's own orient/stats/modules name all 50 crates; the six formerly-degraded crates
render as crate groups; no consumer needed changes; gates + transcript inlined.
