# repo-graph requirements catalog

Status: DRAFT requirements set authored by the in-place manager on 2026-09-12 at the human's direction ("write the requirements for repo-graph — a new folder docs/requirements, one file per high-level requirement, containing all the related low-level requirements inside"). Independent requirements review: PERFORMED in three standalone passes by Codex gpt-5.6-terra (read-only): 2026-09-12 REFINE (`reviews/2026-09-12-requirements-review-0-terra.txt`), 2026-09-13 REFINE (`reviews/2026-09-13-requirements-review-1-terra.txt`), 2026-09-13 ACCEPT (`reviews/2026-09-13-requirements-review-2-terra.txt`); every finding applied. Baseline: `baselines/RG-BOOTSTRAP-INPUT-1.json`, operator-approved under `docs/assurance/RG-BOOTSTRAP/human-authorization.md` for one slice (TRUST-MODULE-EDGES-1); human line-by-line approval of the catalog revision remains PENDING, and the PROPOSED Ls are open decisions. Maturity: PROTOTYPE (requirements record).

Record grammar: `agent-manager/docs/contracts/requirements-assurance-v1.md` (stage-1 Markdown + JSON metadata). Every file below starts with the `requirements-assurance-v1` metadata block; H IDs are `RG-REQ-NNN`, L IDs `RG-REQ-NNN-LNN`. IDs are stable identity, never status; retired IDs are not reused.

## Why this catalog exists

Fourteen slices shipped between v0.17.0 and v0.18.0; one of them regressed the C++ call graph on every C++ repository while its own review read the regression as a win, and a trust regression from May surfaced on ten repositories only in the sixth audit. The defect shape is "whack-a-mole": a slice satisfies its stated requirement and breaks an invariant nobody had written down. This catalog writes the invariants down — what the product must DO (requirements) and what every change must PRESERVE (the preservation obligations each H lists) — so that a slice can name what it implements, what it preserves, and how each is verified, before a builder touches code.

## Catalog

| High-level ID | Required outcome | L entries | v0.18.0 reading |
|---|---|---|---|
| [RG-REQ-001](rg-req-001-deterministic-extraction-and-identity.md) | Extraction is deterministic, identity is stable, nothing extracted is silently lost | 10 | 6 met · 2 partial · 1 unmet · 1 unknown |
| [RG-REQ-002](rg-req-002-honesty-about-certainty.md) | Every answer is honest about what it knows; surfaces reading one snapshot agree | 10 | 7 met · 3 unmet |
| [RG-REQ-003](rg-req-003-orientation-surfaces.md) | The first sixty seconds: orient, check, explain point the agent at the right places | 10 | 8 met · 1 partial · 1 unmet |
| [RG-REQ-004](rg-req-004-module-model-and-cycles.md) | Modules found by a stated method, related by resolved imports, agreeing across every surface | 11 | 9 met · 2 unmet |
| [RG-REQ-005](rg-req-005-symbol-relationship-discovery.md) | A symbol's relationships are discovered, never invented | 10 | 4 met · 3 partial · 3 unmet |
| [RG-REQ-006](rg-req-006-import-resolution-and-dependencies.md) | Imports resolve to the defining file; a dependency is "used" only on evidence | 12 | 7 met · 5 unmet |
| [RG-REQ-007](rg-req-007-boundaries-surfaces-resources-inferences.md) | Boundaries, surfaces, resources and inferences are evidence tracks, labeled as such | 11 | 10 met · 1 unmet |
| [RG-REQ-008](rg-req-008-documentation-inventory.md) | Documentation found where the authors put it, classified by decidable rules, never authored | 9 | 5 met · 2 partial · 2 unmet |
| [RG-REQ-009](rg-req-009-quality-signals-trust-reliability.md) | Quality signals, trust and reliability tell the agent where the risk is now, with their basis | 11 | 8 met · 3 unmet |
| [RG-REQ-010](rg-req-010-semantic-seeding.md) | Semantic seeds are labeled guesses beneath the facts, never the answer | 11 | 8 met · 3 unmet |
| [RG-REQ-011](rg-req-011-daemon-and-store-lifecycle.md) | The daemon serves in milliseconds, never hangs, never serves a broken store, never touches foreign state | 11 | 8 met · 2 partial · 1 unknown |
| [RG-REQ-012](rg-req-012-cli-protocol-surface.md) | rmap is a machine-readable protocol: names, exit codes, cursors, budgets, JSON | 9 | 7 met · 1 unmet · 1 unknown |
| [RG-REQ-013](rg-req-013-governance-and-enforcement.md) | Governance is frozen: maintained, never extended, never erases a measurement | 7 | 6 met · 1 unknown |
| [RG-REQ-014](rg-req-014-distribution-and-host-integration.md) | Installing, running and integrating rmap is binary-first, reversible, honest about platforms | 9 | 8 met · 1 unknown |
| [RG-REQ-015](rg-req-015-indexing-maps-contracts-enrichment.md) | Indexing, maps, contracts and enrichment are explicit operations with stated outcomes | 11 | 9 met · 2 partial |
| RG-REQ-016 — RETIRED 2026-09-13 | (was: architecture preservation) — withdrawn by the human: agents already default to preserving existing structure, too well; the failure mode is not refactoring when necessary. Refactors are decided case by case. ID reserved, never reused. | 0 | retired |

**15 H requirements, 152 individually identified L requirements** (RG-REQ-016 retired, ID reserved). The "v0.18.0 reading" column is computed from each L's leading evidence label (a mixed line such as "OBSERVED MET as honesty; NOT MET as capability" counts by its first label — read the L); it is the manager's OBSERVED reading of the round-six audit and its root-cause record; it is not verification evidence and not acceptance. Each L carries a verification criterion naming the existing test, probe or protocol — or states NO EXISTING VERIFICATION and what to build.

## How the catalog binds the queue — the grounded slice packets

Each queued slice now has a specification in `docs/slices/` whose §0 allocates requirement IDs three ways — **Implements** (the Ls it makes true), **Changes** (the Ls whose evidence or headline numbers move, pre-authorised so a falling trust percentage or a rising dead-code count is the expected outcome, not a regression), **Preserves** (the Ls that must stay true) — and whose §3 **Regression watch** lists, per preserved L, what would regress and the test that proves it did not (existing, write-first, or a corpus count). The preserved set is derived two ways: mechanically (every L whose verification cites a test on a file the slice touches) and from the root cause's blast radius (every surface consuming the changed data path). A preserved row without a proof blocks the definition of done; a preserved row whose proof fails is a STOP. The reviewer checks §3 row by row.

| Queue | Slice specification | Implements | Changes (pre-authorised) |
|---|---|---|---|
| Q1 | [call-binding-receiver-1](../slices/call-binding-receiver-1.md) | RG-REQ-005-L01/L02, 002-L01, 009-L03, 001-L03 | trust calls-resolved % falls; `dead` substrate rises; callers rows change |
| Q2 | [trust-module-edges-1](../slices/trust-module-edges-1.md) | RG-REQ-009-L02, 004-L01, 002-L02, 009-L04 | suspicious-modules section and false downgrade reason disappear; levels stay LOW while unresolved > 0 |
| Q3 | [cpp-include-roots-1](../slices/cpp-include-roots-1.md) | RG-REQ-006-L03, 004-L09, 002-L06 | poco/duckdb/OpenXcom edge counts rise; store grows (cost reported); L11 suffix fallback RATIFIED 2026-09-14, its own slice |
| Q4 | [explain-cycles-honest-1](../slices/explain-cycles-honest-1.md) | RG-REQ-003-L01, 002-L01, 004-L07 | explain's cycle block becomes the unordered form |
| Q5 | [deps-ecosystem-partition-1](../slices/deps-ecosystem-partition-1.md) | RG-REQ-006-L08/L09/L10, 002-L06 | django npm undeclared 102 → 0; gstreamer header names readers and parsed manifests |
| Q6 | [explain-type-sections-1](../slices/explain-type-sections-1.md) (inc 1 of 3) | RG-REQ-005-L04 (members, referenced-by), 005-L09, 002-L07 | every `explain <Type>` gains sections; bases deferred to inc 2 |
| Q7 | [complexity-scope-1](../slices/complexity-scope-1.md) | RG-REQ-009-L01, 001-L08, 003-L02, 002-L02 | complexity top-N moves; seed corpus narrows on repos with generated files; reindex |
| Q8 | [docs-discovery-1](../slices/docs-discovery-1.md) — RG-REQ-008-L01 ratified 2026-09-14 | RG-REQ-008-L01/L02/L06/L07, 003-L09 | hadoop 23 → ~545; django readme 4 → 6; buildroot 4 → ~77 |
| Q9 | [python-self-binding-1](../slices/python-self-binding-1.md) | RG-REQ-005-L03/L02, 001-L03 | django +~3,600 edges; trust % rises on Python repos; reindex |

## How the catalog binds the queue

RATIFIED ORDER (human 2026-09-14, option A): Q1 → Q3 → Q4 → Q5 → Q6 → Q7 → Q8 → Q9 → ALIAS-SUSPICION-1 (the false "alias resolution suspected" reason on repo-graph/kafka/FRAKTAG, RC-11) → SEED-DOCUMENT-1 (RG-REQ-010-L10). Q2 shipped df08725.

The round-six fix queue maps onto currently unmet Ls: Q1 CALL-BINDING-RECEIVER-1 → RG-REQ-005-L01/L02, RG-REQ-009-L03, RG-REQ-002-L01 · Q2 TRUST-MODULE-EDGES-1 → RG-REQ-009-L02, RG-REQ-004-L01 · Q3 CPP-INCLUDE-ROOTS-1 → RG-REQ-006-L03 · Q4 EXPLAIN-CYCLES-HONEST-1 → RG-REQ-003-L01 · Q5 DEPS-ECOSYSTEM-PARTITION-1 → RG-REQ-006-L08/L09 · Q6 EXPLAIN-TYPE-SECTIONS-1 → RG-REQ-005-L04 · Q7 COMPLEXITY-SCOPE-1 → RG-REQ-009-L01, RG-REQ-001-L08 · Q8 DOCS-DISCOVERY-1 → RG-REQ-008-L01/L02/L06/L07 · Q9 PYTHON-SELF-BINDING-1 → RG-REQ-005-L03. A slice packet shall reference the L IDs it implements and the preservation obligations it must hold, and its reviewer shall check both.

## Gaps the review named and the decisions it routes to the human

- **Missing coverage (closed in this revision):** RG-REQ-015 now owns `rmap index/refresh` as an operation, `map`, `contracts`, enrichment and `perf`, stating what is PLANNED (CS-2, BC-1, PERF-OBS-1B) rather than shipped; the `imports <file>` CLI outcome is RG-REQ-006-L12 (NOT MET: the per-file listing drops unresolved rows). RG-REQ-015 has not yet had its own independent review.
- **Decisions for the human (each L names the problem and the options):** RG-REQ-011-L11 — the warm-`orient` latency bound (no defined benchmark: ratify a measured machine/corpus/percentile bound, publish measurements only, or self-calibrate per machine); RG-REQ-008-L01 — the docs discovery scope unfreezes DOCS-LIST-2's discovery freeze (ratify the stem/extension/`src/site` rule, keep it proposed pending a corpus probe, or retain the old scope); RG-REQ-006-L11 — the unique-suffix include fallback as Q3's second increment (roots only, or roots plus suffix); a repository-declared generated exclusion (`.gitattributes linguist-generated`) — a separate proposal explicitly OUTSIDE RG-REQ-009-L01 as written — and RG-REQ-010-L10 SEED-DOCUMENT-1 / RG-REQ-010-L11 concern hints: ratify as new obligations or keep in the proposed backlog.
- **Structure reading (2026-09-13):** `docs/audits/2026-09-13-structure-reading.md` (rmap on itself) records where size has a reason and where it does not; the architecture-preservation requirement drafted from it was RETIRED the same day (human: agents over-preserve structure; refactor case by case). Its two findings stay as candidates: `quality-policy` → `storage` domain types; the orphan `repo-graph-detectors` crate.
- **Builder-facing consequences for Q1/Q2 (from the review):** Q1's regression fixture must type the indirect receiver as the OTHER class; enclosing-type preference is receiverless/explicit-`this` only; unresolved rows are preserved and the resolution-rate decrease is reported. Q2 implements the trust↔modules shared derivation only (RG-REQ-004-L01) and must not claim RG-REQ-004-L11 (stats/grpc identity) or an Import-graph level change.

## Record conventions

- An H file owns its statement, scope, sources, Ls and the preservation obligations its ratifying specifications name. Source links name origins (a VISION section, a ratified slice's contract heading); they are not proof the refinement is ratified.
- An L states conditions, observable behaviour, limits, a verification criterion and the current evidence reading, labeled OBSERVED MET / PARTIALLY MET / NOT MET / UNKNOWN. Requirement content, approval of a revision, implementation progress and verification outcome are separate concepts; a status word cannot manufacture acceptance.
- Open decisions are named inside the L that needs them (RG-REQ-011-L11's latency bound; RG-REQ-008-L01's scope rule authored against an audit rather than a ratified spec).
- Preservation lists reference enduring obligations; slices reference them by H/L ID rather than copying text.
- Do not renumber IDs on wording changes; retire rather than reuse.

## Read next

- `docs/VISION.md` — product intent (the source of every H).
- `docs/ROADMAP.md` — ordering and the pending queue; `docs/audits/2026-09-08-root-causes-v0.18.0.md` — the code-level causes the unmet Ls cite.
- `agent-manager/docs/PROCESS.md`, `docs/MANAGER.md` — how requirements, slices, baselines and acceptance relate.
