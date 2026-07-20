# MODULE-OWNERSHIP-DUPLICATE-1 — dual detector claims must resolve, not crash

Status: SPECIFIED (2026-07-20) · Track: Field bugs (scale-ladder baseline run
smoke-runs/2026-07-20T03-16-37Z; first vscode contact).

## 1. Problem (measured)

On vscode (11,878 files), `modules list`, `modules violations`, and `violations` all fail:
`InternalError: failed to load module graph facts: edge derivation failed: file
extensions/esbuild-common.mts has duplicate ownership: ["inferred-mod-…", "npm-mod-…"]`.
vscode's `extensions/` is an npm workspace whose shared build scripts are claimed by BOTH
the npm-package detector and the inferred-directory detector; the module-edge derivation
(`classification/src/module_edges.rs:92`, `module_rollup.rs:112`) treats double ownership
as a hard error, killing the entire module command family on any repo with the collision.
The declared-vs-inferred exclusion exists (`repo-index/src/compose.rs` ORIENT-BUG-1
"declared roots") but does not cover this npm-vs-inferred shape.

## 2. Contract

1. **Deterministic resolution at the source:** when a file is claimed by an npm-package
   module AND an inferred module, the npm-package claim WINS (the more specific, declared
   evidence — same principle as ORIENT-BUG-1's declared-roots exclusion). Prefer fixing
   at candidate/ownership-generation time (compose) so the invariant stays true
   downstream, over relaxing the downstream check. Investigate WHY the existing
   declared-roots exclusion misses this shape and close that gap — do not add a second
   parallel dedup if extending the existing mechanism suffices.
2. **The downstream check stays** as the invariant guard (it caught this) — but its
   failure mode must degrade honestly: if double ownership ever recurs, the module
   commands report the defect + affected files as a labeled degradation, not a bare
   InternalError killing the surface. (Both: fix generation AND soften the crash.)
3. **No silent reassignment beyond the stated rule** — the resolution rule (npm >
   inferred) is recorded in output-visible module provenance where module provenance is
   already shown; ownership changes for files NOT in collision are forbidden (byte-parity
   everywhere else).

## 3. Stop conditions

Frozen: witness/union/reconciliation surfaces, storage write schema (unless a compose fix
requires none — expected), trust. If the correct resolution rule turns out to be
ambiguous on the evidence (npm-vs-inferred is not clearly more specific in some real
shape), that is a FINDING (DECISION_REQUIRED), not a coin flip. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Repro fixture: a committed test reproducing the vscode shape (npm workspace package +
  shared build script claimed by an inferred module) — RED before, GREEN after.
- Live proof on the ACTUAL vscode clone (../legacy-codebases/vscode, isolated state
  root): `modules list`, `modules violations`, `violations` all succeed; the previously
  crashing files appear with npm ownership.
- Byte-parity on non-colliding repos: modules-family outputs identical on ≥2 repos from
  the retained baseline run (e.g. mempalace, OpenXcom) vs smoke-runs/2026-07-20T03-16-37Z.
- Degradation path test: forced double ownership downstream → labeled degradation, not
  InternalError.
- Chunked cargo gates (standing pattern); consolidation witness 15/15; smoke
  SMOKE_ONLY="vscode mempalace" logged run green.

## 5. Definition of done

The vscode module command family works; the collision class is resolved at generation
with the recorded rule; the guard degrades honestly if it ever fires again; byte-parity
holds everywhere else; gates green.
