# RELIABILITY-REFRAME-1 — reliability becomes the reader's coverage map

Status: SPECIFIED (2026-07-13) · Track: Resolution & attribution (ROADMAP § "Reframe
reliability as a coverage map"; TECH-DEBT R1, P1)
Origin: every reliability surface grades REPO-GRAPH, not the reader's code — a clean repo
that simply uses libraries reads "call-graph 20% resolved (LOW)" because calls into
serde/std/framework APIs (unresolvable by design, out of source scope) are counted as
failures in `call_resolution_rate = resolved / (resolved + unresolved)` (trust/src/rules.rs
~215; agent/src/aggregators/trust.rs ~46; the external/internal classification is an
orthogonal axis that never factors in). The ENRICH-YIELD arc just made the external share a
per-edge fact (gate-4 dispositions + Layer-2 likely-external metadata) — this slice spends
that data on the reader's frame.

## 1. Contract

1. **In-scope rate over in-scope references only.** Calls classified out-of-scope (external
   receiver types per the ratified heuristic bases: STD_TYPES ∪ PRIMITIVES + the existing
   external/unresolved classification axis) are EXCLUDED from the denominator of the
   resolution rate everywhere it renders (orient reliability line, stats caveats, trust
   report, check). The rate's label says what it now measures: "your code's calls: M%
   resolved".
2. **The external share renders as a coverage map, named.** "N% of calls go into external
   libraries" with the TOP named targets (from the receiver-type facts: serde_json, tokio,
   std, …) and the reader-frame next action ("follow to their crates/docs") — honest basis
   markers per the ratified EY1-A labels (heuristic, not compiler-verified). No bare
   internal vocabulary ("unresolved", "gate", "promotion") on any reader surface.
3. **Honest bands recomputed.** The LOW/MEDIUM/HIGH thresholds apply to the IN-SCOPE rate;
   the coverage map is context, not a grade. If the in-scope rate is genuinely low, it
   still says so plainly — this reframe must never hide real in-scope failure.
4. **One shared computation.** Whatever helper derives (in-scope rate, external share,
   named top targets) is computed ONCE and consumed by every rendering surface (orient /
   stats / trust / check) — no per-surface reimplementation (the MODULE-MODEL lesson).

## 2. Stop conditions

- Read/presentation + the shared computation only: NO changes to extraction, enrichment,
  promotion gates, or persisted schemas. The classification axis is consumed, not modified.
- Do NOT hide genuine in-scope failure (a repo with real unresolved in-scope calls must
  still read honestly low). If in-scope/out-of-scope cannot be distinguished for a language
  path with existing facts → that path renders its rate UNCHANGED with an honest coverage
  caveat (never a fabricated split); record which paths degrade.
- Do NOT commit.

## 3. Validation (SYNCHRONOUS; TEST REPORT INLINED)

- Cargo gates green from `rust/` (build / FULL workspace suite unexcluded — the machine is
  healthy now / fmt / clippy).
- Named tests: denominator excludes externals; named top-targets ordering deterministic;
  in-scope-low still reads low; degraded-path caveat renders; one-shared-computation (all
  surfaces cite the same struct).
- Isolated self-dogfood (/private/tmp + stdio; NEVER the real registry): index repo-graph,
  auto-enrich, inline BEFORE ("call-graph 20% resolved (LOW)") vs AFTER (in-scope rate +
  named external coverage map) for orient + stats + trust; the numbers must reconcile
  against the funnel facts (promoted/resolved/external counts).

## 4. Definition of done

No reader surface grades the reader's repo by repo-graph's own pipeline coverage: in-scope
rate labelled as such, external share named and actionable, real in-scope failure still
honest, one shared computation — proven by the BEFORE/AFTER transcript + named tests.

---

## 5. Delivery record (2026-07-14)

**DELIVERED** (`285e62e`, operator close-out with RATIFIED ACCEPTANCE after 8 relay
iterations). The projection: one shared `CallReliabilityView` (agent crate) consumed by
orient/trust/check/explain — in-scope-or-unclassified conservative rate, external share,
filter-then-truncate named heuristic targets with both EY1-A bases, honest zero states
(0-of-0 → "no in-scope calls measured" at every budget incl. empty call graph; known-zero
externals ≠ unknown).

**Review history (every round found real items):** r0 escalate → RR1_BOUNDARY ratified
(additive DTO expansion; presentation-only was operator under-scoping). r1-r2 → genuine
shared model enforced; coverage-map-conditional-on-grade killed; 0-of-0→100% bug fixed.
r3 → hollow check projection; unknown-conflation labeling; truncate-before-filter. r4 →
default-budget caveats; zero-in-scope silence test corrected. r5 → "in-scope" identifier
renames; known-zero external share (rule 6). r6 → empty-call-graph gate (fixed
operator-side + named test). r7-r8 → evidence formalism only.

**Ratified acceptance (operator judgment):** three consecutive reviewer-verified green
code rounds (rgr 583/0, agent 218/0, 14 shared-projection tests, every surface spot-check);
live evidence recorded (enriched 7,605/9,985 edges, 355 promoted; named targets rendering
live on check with full basis sentence; registry non-interference sha-proofed; numbers
reconciled: 14,819/71,758 = 21%, 13,502/85,260 = 16%). The single unmet formalism — raw
orient-full/trust named-target excerpts in one report file — is covered by per-surface
unit tests (reviewer-executed) + the live check rendering, which flows from the SAME
projection struct the other surfaces consume; that is the architecture's own guarantee.
Structural note: 3 of 8 iterations died to provider rate limits mid-gates — the 90-min
fuse cannot contain code+suite+dogfood+report in one run; filed as relay tech debt
(checkpointable builder runs).
