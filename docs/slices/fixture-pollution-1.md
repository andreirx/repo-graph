# FIXTURE-POLLUTION-1 — test-only surfaces stop posing as production architecture

> AMENDED (ruling fixture-test-scope = Option 1, 2026-08-31): the demotion applies to ALL
> structurally test-only surfaces on every repo (neutral "test-only" wording) — there is no
> provenance fact distinguishing rmap's fixtures from user test code, and inventing one is
> unearned. glamCRM's test-mock surfaces demoting below its real routes is intentional.

Status: SPECIFIED (2026-08-31) · Track: Usefulness audit v0.11.0 fix queue, item #3 (post-
refutation numbering). CODE slice, presentation/classification layer. Maturity: MATURE.

## 1. Problem (measured — audit run 2026-08-30T20-44-57Z, repo-graph self-index)

The v0.9.0 self-pollution fix (SELF-POLLUTION-1) cleaned orient/check (sidecar exclusion with
the visible "+N tool/OS files ignored" line) but the STRUCTURE surfaces still read the tool's
own test corpus as architecture:
- boundaries-list/summary: 29 of 39 boundary groups (35 rows) are `rust/crates/repo-index/
  tests/fixtures/**` amqp/kafka/semaphore props, unlabeled — the 4 real boundary groups
  (rgistr LLM adapters) drown.
- cycles: Cycle 1 is the `xpart-monorepo` fixture, presented like a production cycle.
- docs-list: 60 foreign `*_MAP.md` sidecars under `smoke-runs/**` listed as kind
  `architecture` (exclusion is keyed to THIS repo's generation records, not the pattern).

## 2. Contract

1. **Basis: the stored `is_test` fact and path-containment under test-owned subtrees — never
   bare name matching.** A fixture row is one whose files are `is_test` OR live under a
   directory whose files are wholly test-owned (the conservative aggregation CONTRADICTION-
   SWEEP-1 §2.3 defined: any production file ⇒ production). If the existing facts cannot
   distinguish a fixture subtree on some surface, that surface renders the honest mixed state
   — do NOT invent a name heuristic (`tests/`, `fixtures/` as strings are NOT evidence).
2. **Label + demote, don't hide.** Structure surfaces group fixture/test-only rows under an
   explicit trailing section: `test-fixture surfaces (N groups — excluded from the headline
   counts): …` with the same visible-self-exclusion style orient uses. Headline counts and
   the leading rows are production-only; JSON gains an additive `is_test_only` per row (no
   row removed from JSON).
3. **cycles**: test-only cycle labeling was DEFERRED to CYCLE-FACTS-2 (needs is_test in the
   LiveGraph IR). Here, only the SQLite-served route labels (it has the fact); the LiveGraph
   route renders unchanged — record the asymmetry honestly in the output ("test-composition
   not evaluated on this serving path") rather than pretending uniformity. If that honest
   asymmetry is deemed worse than nothing, STOP + DECISION_REQUIRED with the trade-off.
4. **docs-list**: exclude `*_MAP.md` sidecars by their GENERATED-CONTENT MARKER (the map
   generator stamps its output — verify the marker exists; if it does not, that is a FINDING:
   propose stamping in a follow-up, and here fall back to generation-records + the honest
   "may include foreign generated maps" caveat). Never exclude by filename alone.
5. Exit codes and existing JSON fields unchanged; additions additive.

## 3. Stop conditions

Frozen: storage schema, module ownership computation, LiveGraph/witness/certificates, trust,
exit codes. STANDING HONESTY RULES (no name-based classification; unknown-with-reason on
fallible rendered reads). New public APIs beyond additive DTO fields → DECISION_REQUIRED.
Unmet DoD → STOP + DECISION_REQUIRED. Never touch the operator's real state root. Do NOT
commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: fixture-row determination from is_test aggregation (mixed dir ⇒ production);
  demoted-section rendering with headline exclusion counts; docs-list marker-based exclusion
  (or the fallback caveat); additive JSON.
- Live proof (isolated state root, registry sha unchanged): repo-graph self-index —
  boundaries headline counts the 4 real groups with the fixture section labeled below;
  cycles' fixture cycle labeled on the SQLite route; docs-list free of smoke-run sidecars
  (or honestly caveated). glamCRM spot-check: test-mock surfaces (if any classify test-only)
  demote below production routes — an INTENTIONAL change, captured before/after. Captures in the report.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

An agent orienting on ANY repo reads production boundaries first; test-only content is
labeled and demoted (neutral wording), never silently hidden and never name-classified;
UNKNOWN test-composition is NEVER demoted — it stays in the main listing with an explicit
unknown-with-reason marker (hiding possibly-real architecture is worse than showing a
fixture); docs-list stops importing
foreign generated maps as architecture; asymmetries are stated; gates green.
