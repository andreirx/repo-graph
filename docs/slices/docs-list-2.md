# DOCS-LIST-2 — docs get a budget and a truthful taxonomy; doctor's advice speaks the repo's language

Status: SPECIFIED (2026-09-01) · Track: Usefulness audit v0.11.0 fix queue, tail item (final).
CODE slice, small. Maturity: MATURE surfaces.

## 1. Problem (measured — audit run 2026-08-30T20-44-57Z)

- django `docs list`: 683 unbudgeted lines; 666 of 670 docs collapse to kind `architecture`
  — including ~190 `docs/releases/1.4.x.txt` release notes and `fontawesome/LICENSE.txt`.
  FRAKTAG surfaces 6 vendored `fraktag-env/**/site-packages/**` READMEs of 16 rows.
- doctor still tells a pure-Python repo `npm i -D typescript` (the per-language CTA fix
  never reached doctor's remediation lines).

## 2. Contract

1. **Taxonomy from content/location FACTS, never bare name guessing**: add kinds
   `release-notes` (docs under a release/changelog subtree — the subtree identified by its
   MANIFEST-adjacent location or front-matter/content markers, with the basis stated; if no
   deterministic basis exists for a doc, it KEEPS `architecture` — no guessing),
   `license` (SPDX/license content markers), `vendored` (files under directories the index
   already classifies vendored/third-party — the existing vendor classification fact; if
   none exists for a path, it is not vendored). Existing kinds untouched.
2. **Vendored docs demote** (FIXTURE-POLLUTION-1 pattern): excluded from the headline with
   the explicit "+N vendored docs (excluded)" line; never silently hidden. release-notes
   GROUP to one line per family ("django release notes: 190 files under docs/releases/ —
   --full to list").
3. **Budget** per the house standard: default cap + explicit "(+N more — --full)"; JSON
   stays complete.
4. **doctor's per-language remediation**: the enrichment skip/remediation lines route
   through the SAME per-language capability logic the CTA uses (CONTRADICTION-SWEEP-1's
   material-language gate): a repo with no enrichable language gets the no-path sentence,
   never `npm i -D typescript`; mixed repos name the language each remedy applies to.
5. JSON additive; exit codes unchanged.

## 3. Stop conditions

Frozen: doc extraction/inventory computation (classification labels and rendering change;
discovery does not), storage schema, exit codes, trust. STANDING HONESTY RULES (kind
assignment needs a stated deterministic basis; unknown keeps the old kind). New public APIs
beyond additive DTO fields → DECISION_REQUIRED (registry/read-only accessor precedent chain
applies). Unmet DoD → STOP + DECISION_REQUIRED. Never touch the operator's real state root.
Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: each kind's deterministic basis (+ no-basis keeps architecture); vendored demotion
  line; release-notes grouping; budget remainder; doctor remediation per language mix
  (pure-Python → no npm advice; mixed → per-language).
- Live proof (isolated state root, registry sha unchanged): django docs list ≤ budget with
  grouped release notes and no license-as-architecture; FRAKTAG vendored demoted; django
  doctor free of npm advice. Before/after captures.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

docs-list renders a bounded, truthfully-kinded inventory (vendored demoted, release notes
grouped, licenses named); doctor never prescribes another ecosystem's remedy; every kind
assignment has a stated deterministic basis; gates green.
