# SCANNER-GITIGNORE-1 — full gitignore semantics in the scanner (no silently dropped source)

Status: SPECIFIED (2026-07-13) · Track: Extraction correctness / honesty core
Origin: TECH-DEBT § "Scanner applies root .gitignore patterns unanchored — LIVE" (found
during CARGO-WORKSPACE-INHERITANCE-1): the root `.gitignore`'s ROOT-ANCHORED `/coverage/`
pattern silently drops the entire tracked crate `rust/crates/coverage/` from the index —
zero FILE nodes, no caveat, no diagnostic. Any repo root-ignoring `/build/`, `/dist/`,
`/coverage/`, `/target/` while having same-named nested SOURCE dirs is affected — common in
monorepos, including the deployment target.

## 1. Problem

The scanner's hand-rolled root-only `.gitignore` loader (walkdir + load_root_gitignore)
treats root patterns as unanchored directory-name matches: `/coverage/` (git semantics:
only the root-level `coverage/`) matches ANY `coverage` directory anywhere. Git tracks the
files; the map claims they don't exist. "This is what exists" — the tool's core claim — is
silently false.

## 2. Contract

1. Replace the hand-rolled loader with `ignore::WalkBuilder` (the `ignore` crate): full
   gitignore semantics — anchoring, negation (`!keep`), nested `.gitignore` files,
   `.git/info/exclude`. The scanner's OUTPUT contract (which files are yielded to the
   indexer, ordering requirements if any consumers depend on it) otherwise unchanged.
2. Parity guard: on a repo where the old and new scanners agree (no anchored/nested/negation
   patterns), the yielded file set is IDENTICAL (fixture-proven).
3. The fix applies to every scan entry point that uses the defective loader (inventory them;
   cite each) — no surface keeps the old semantics.

## 3. Stop conditions

- Scanner/file-inventory scope ONLY: no extractor, postpass, enrichment, retention, or
  module-model changes. The frozen areas stay frozen.
- If any consumer depends on the CURRENT (wrong) exclusion behavior in a load-bearing way
  (e.g. perf guards against target/ scanning), handle via correct root-anchored patterns —
  the defaults must still exclude what git excludes, nothing more. If genuinely blocked →
  STOP + DECISION_REQUIRED.
- Do NOT commit.

## 4. Validation (SYNCHRONOUS; TEST REPORT INLINED)

- Cargo gates green from `rust/` (build / workspace test with the documented environmental
  exclusion / fmt / clippy).
- Named fixtures: (a) root-anchored `/coverage/` + nested `rust/crates/coverage/` SOURCE
  dir → nested dir INDEXED, root-level dir excluded (the live casualty); (b) negation
  pattern; (c) nested `.gitignore`; (d) parity fixture (old == new where semantics agree).
- Isolated self-dogfood (/private/tmp + stdio; NEVER the real registry): index repo-graph —
  `rust/crates/coverage/` files appear in the inventory, the `coverage` crate emits its
  module candidate and FOLDS as a package group (closing the CARGO-WORKSPACE-INHERITANCE-1
  carve-out: all 50 crates now fold); orient/stats transcript inlined. THE OUTPUT SURFACE
  IS THE PROOF (deep-vertical rule).

## 5. Definition of done

git-tracked source can no longer be silently dropped by ignore-pattern collisions; the
coverage crate is visible end-to-end (inventory → candidate → package group); parity holds
where the old behavior was correct; gates + transcript inlined.
