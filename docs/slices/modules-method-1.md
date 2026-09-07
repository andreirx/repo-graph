# MODULES-METHOD-1 — the modules surface says how it found its modules and where the authors say it better

Status: SPECIFIED (2026-09-07) · Track: human ruling 2026-09-07 on the v0.17.0 audit's modules grades
("this is likely the best DOCUMENTED part of any repo … stop digging into that surface and just
describe the method used AND recommend in plain text to read the repo docs or tree it for better AI
orientation JUST IN THIS CASE"; and, same day: notices and recommendations on the output surfaces only —
no repo-side memo layer, no dropping them). CODE slice, rgr presentation (modules list, orient module
section) + the daemon fields that carry the method and the doc paths. Maturity: MATURE.

## 1. Problem (from the audit; the method is known, the boundary problem is not algorithmic)

Outward surface: `rmap modules list` and orient's module section are where an agent orients. The
v0.17.0 audit graded `modules list` HIT C across every class: module boundaries are a human
convention (module vs sub-module vs directory group) that rmap infers from manifests and directory
prefixes, while the repo's own README / ARCHITECTURE / docs state them directly and an agent reading the
tree often guesses them right. rmap's rows today do not say WHICH method produced them on THIS repo,
nor where the authoritative description lives — an agent cannot tell "59 crates from Cargo.toml"
(trustworthy boundaries) from "8 inferred top-level directories" (a guess) from the rendering alone,
and gets no pointer to the docs that would settle it.

Ruling: NO further algorithmic module-discovery work (CYCLES-POPULATION-1 closes as "the population
is stated"); module EDGE resolution stays (edges are facts once boundaries are given).

## 2. Contract

1. **The method, per repo, from facts already stored.** `modules list` and orient's module section
   open with one line naming the method(s) that produced the modules ON THIS REPO, with counts:
   `Modules: 59 declared in Cargo.toml (workspace members)` · `67 Gradle projects from settings.gradle
   (41 relocated via projectDir)` · `5 npm workspaces from package.json` · `8 inferred from top-level
   directories (Maven manifests present but not parsed on this build)` · mixed repos list each family.
   The data: `module_candidates.module_kind` / `module_candidate_evidence.source_type` + the existing
   diagnostics (unhandled projectDir count, Maven presence). Nothing is inferred anew.
2. **The recommendation, per repo, from the docs rmap already classified.** One line: `For module
   boundaries as the authors describe them, read: README.md, docs/ARCHITECTURE.md, docs/design/ — or
   the tree.` The paths are the repo's actual top orientation docs from `docs list`'s facts (README at
   the root, files named ARCHITECTURE/DESIGN/OVERVIEW/CONTRIBUTING, the `docs/` and `design/`
   directories), at most 4, ranked root-first; when the repo has NONE of these, the line says `No
   README or architecture doc found — the tree is the best orientation` — never a generic sentence
   that reads the same on every repo (the map-not-territory rule).
3. **Honest weight.** When every module is inferred (no manifest family), the method line carries
   `boundaries are a guess from directory names` and the recommendation is rendered FIRST; when all
   are declared, the recommendation still renders (declared ≠ described) but after the rows.
4. **Wire: additive** — `modules_method` (family → count, + diagnostics already present) and
   `orientation_docs` (paths) on the modules-list and orient responses; JSON consumers get the same.
5. **Movement measured**: repo-graph, kafka, grpc-java, hadoop, django, leveldb, FRAKTAG, vcmi
   `modules list` + `orient --budget large` before/after, verbatim; the two lines differ across all
   eight (no two identical) — that difference is the acceptance test.

## 3. Stop conditions

Frozen: wire protocol (additive), storage schema, exit codes, the module discovery itself (NO new
inference — if a repo's method cannot be named from stored facts, say `method not recorded on this
index` and STOP + DECISION_REQUIRED with the missing fact). The line NEVER repeats information the
rows already carry (no per-module restatement). STANDING HONESTY RULES. Unmet DoD → STOP +
DECISION_REQUIRED. Never touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

Unit FIRST: method line for each manifest family + mixed + all-inferred; recommendation with 0 / 1 /
4+ docs (cap and ranking); no-docs sentence; the "reads the same on every repo" property tested as an
inequality across fixtures. Live proof on a COPY of the retained root served with auto passes off (no
reindex needed): the §2.5 eight repos. Gates recorded FIRST; chunked cargo; witness; dogfood-isolated.

## 5. Definition of done

On every repo, `modules list` and orient's module section state which method produced the modules
and where the authors describe them better, from stored facts, repo-specifically; no new discovery;
gates green.

CORPUS PATHS: kafka, grpc-java, hadoop, django, leveldb, vcmi at ../legacy-codebases/<name>; FRAKTAG
under ../FRAKTAG; repo-graph is THIS repo.
