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

---

## 6. Ratified contract amendment + delivery record (operator, 2026-07-12)

**DELIVERED** (relay build + operator close-out; reviewer escalate resolved by THIS
ratification — the §2 letter was authored before the real root cause was known).

**Real root cause (narrower than §1 assumed):** `version.workspace = true` deserializes as
a TOML table, crashed the strict `Option<String>` field, failed the WHOLE manifest parse,
and suppressed the candidate. Five crates were victims (rgr, daemon-runtime,
graph-algorithms, platform-paths, rmapd) — not six; see the coverage carve-out below.

**§2.1 SUPERSEDED (ratified):** the reader stays a pure single-manifest parser. It
tolerates the inherited-table form (and any non-string version) and emits the candidate —
crate identity (name + location) is a deterministic fact of the member manifest alone
(`name` cannot be inherited) — with version **honestly unresolved** (`None` = not
measured, never fabricated). NO cross-file resolution against `[workspace.package]` at
parser level. **Recorded residual:** caller-level (`compose.rs`) version-literal
resolution + a diagnostic distinguishing valid inheritance from a malformed version value
(both need the root table / diagnostics sink that live at the caller) — pick up only if
version display or manifest-lint fidelity ever matters.

**§5 DoD AMENDED (ratified):** 5/5 reachable crates fold (live isolated proof: orient
headline `rgr, storage, daemon-runtime, agent, repo-index, indexer, … · 257 package
groups`; cargo evidence rows 50 → 55; the six crates' directory fragments gone, 281 → 257
groups). `coverage` is EXCLUDED from this slice's DoD: it was never a victim of the
inheritance crash (explicit `version = "0.1.0"`) — the entire `rust/crates/coverage/`
tree is invisible to the index because the scanner applies the root `.gitignore`'s
root-anchored `/coverage/` pattern UNANCHORED (git itself tracks the crate). That
pre-existing scanner defect is now TECH-DEBT (live, named casualty) with its own
follow-up slice; fixing it here would violate the frozen-walks stop condition.
