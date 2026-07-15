# ATTRIBUTION-1 — name where unresolved calls go, in the reader's terms

Status: SPECIFIED (2026-07-15) · Track: Resolution & attribution (ROADMAP § "Attribute the
unresolved set"; TECH-DEBT R2, P1)
Origin: the unresolved set is CATEGORIZED internally (basis codes:
`external_library_candidate`, `internal_candidate`, …) but never ATTRIBUTED for the reader.
RELIABILITY-REFRAME-1 shipped the aggregate coverage map ("16% external — Vec, str,
String"); this slice completes the story per-reference: "library call → serde_json (from
your Cargo.toml serde_json 1.x)" instead of a raw category code. The trust surface still
leaks the internal vocabulary (`Unresolved Breakdown`, `external_library_candidate`,
`internal_candidate` — RR1 review-4 OBSERVED); that leak dies here.

## 1. Contract

1. **Reader-frame attribution labels.** Every surface that renders unresolved-reference
   categories replaces internal codes with the reader's frame: "library call → <named dep>"
   / "standard library (std::…)" / "system/native call" / "dynamic dispatch" / "unknown —
   couldn't attribute". Basis markers follow the ratified EY1-A honesty style (heuristic vs
   manifest-anchored, never "compiler-verified" unless it is).
2. **Provenance where facts exist.** For library calls: the manifest dependency it maps to
   (name + version as recorded by the existing manifest readers — Cargo/package.json;
   Java/Gradle stays out per R3, rendered as the honest degraded path). NO new
   provenance computation — join what storage already carries; where the join has no fact,
   the label degrades honestly ("library call (dependency not identified)").
3. **The trust Unresolved Breakdown reframes** into these labels (raw codes move to the
   debug/structured surface only); orient/check inherit whatever compact form they already
   render via the shared projection — extend `CallReliabilityView`'s named-target model
   with the attribution class ONLY if it needs no new wire fields beyond RR1's ratified
   additive expansion (else STOP + DECISION_REQUIRED).
4. **One shared attribution mapping** (code → reader label + basis) in ONE module consumed
   by every renderer — the RR1/MODULE-MODEL lesson, applied from the start.

## 2. Stop conditions

- Read/presentation + one shared mapping + existing-fact joins ONLY. No extractor,
  enrichment, promotion, scanner, or schema changes; no new provenance computation
  (no include-path resolution — that is future C/C++ work).
- Languages without the needed facts render the honest degraded label; record which.
- Do NOT commit.

## 3. Validation (SYNCHRONOUS; INCREMENTAL REPORT — write build-N.md AS evidence lands:
gates section per gate, transcripts as captured; a killed run must leave a resumable report)

- Cargo gates from `rust/` (build / full UNEXCLUDED workspace suite / fmt / clippy), each
  with raw exit status, WRITTEN INTO THE REPORT IMMEDIATELY on completion.
- Named tests: mapping covers every existing basis code (exhaustive match — a new code
  fails compilation, not silently "unknown"); provenance join renders name+version when
  the manifest fact exists and degrades honestly when not; no internal vocabulary on any
  reader surface (grep-proof test).
- Isolated live dogfood (/private/tmp + stdio; NEVER the real registry; registry checksum
  before/after): index + enrich repo-graph, capture trust's reframed breakdown +
  orient/check unchanged-or-extended lines RAW into the report as captured. stats: N/A
  expected — verify and record.

## 4. Definition of done

No reader surface shows an internal category code; unresolved calls are named in the
reader's world with honest provenance where facts exist and honest degradation where not;
one shared mapping; incremental report complete with raw evidence.

---

## 5. Delivery record (2026-07-15)

**DELIVERED** (`adfd0cf`, operator close-out with ratified acceptance after 4 build
iterations + 5 reviews). Live on repo-graph: "Unresolved references — where they go" with
named base dependencies (serde 187, workspace crates, vitest 681 post-join, …), honest
unidentified/unattributed tails, and the basis + provenance disclosure lines; internal
codes grep-proofed off every reader surface. Ratifications: ATTR1-MAPPING-BOUNDARY
(agent → classification inward edge; one compile-fail-exhaustive mapping),
ATTR1-ORIENT-CHECK (trust-only; RR1's coverage map is the compact surface),
ATTR1-PARITY-BASELINE (fixture repaired). Review value per round: coarse-classification
mapping rejected → basis codes; import-path names rejected → declared-dependency reduction
reusing the classifier's logic; false provenance sentence caught after the join improved.
Final residuals (3 stale doc comments + the report stub after another fuse kill) closed
operator-side; the reviewer had already EXECUTED the substantive proofs (e2e
storage-to-render test, smoke artifacts with complete captures). TECH-DEBT R2 closed.
Next: MAP-FROM-INDEX-1, then v0.7.0.
