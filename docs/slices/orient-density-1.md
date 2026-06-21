# ORIENT-DENSITY-1: budget = information density, not token-count — SPEC

Slice: ORIENT-DENSITY-1
Status: **SPEC** (design; the IMPL is the relay slice that follows)
Track: Product. `orient` delivers DENSE load-bearing orientation; budget trades DEPTH, not COUNT.
Grounded in: the End-to-End Usefulness Protocol evaluation of `orient` on nginx/repo-graph (real output).

## 1. The problem (OBSERVED, real output)

A budget is a DENSITY contract: "N tokens → the N most load-bearing facts." `orient` inverts it. At
`--budget small` it truncates a list of signals that are themselves META (tool-confidence, counts,
reliability tiers) — so small budget = FEWER tokens AND THINNER information. The useful orientation
(where the complexity is, the structure) is ABSENT from `orient` — it lives in `hotspots`/`stats`/
`modules`.

Real `orient --budget small` on nginx:
> Confidence: medium · 342 symbols exceed complexity 20 · 397 files, 6 modules · call-graph LOW ·
> import-graph LOW · change-impact LOW · certainty/limits

The token budget is spent on the tool reporting about ITSELF + unnamed counts. It does not tell the
agent: what the modules are, WHERE the complexity is, where to start.

## 2. The principle (RATIFIED by operator)

- **BUDGET CONTROLS DEPTH, NOT INFORMATION.** At any budget, `orient` leads with the MOST
  load-bearing orientation. Small = the densest headline set; larger = more depth (more items, more
  detail). Budget NEVER strips information down to thin meta.
- `orient` **SYNTHESIZES** the signals that already work (the data is computed — `orient` must
  PRIORITIZE + NAME it densely, not report counts).
- This is density **on top of** the honesty the truth-audit established — never at its expense.

## 3. The load-bearing facts `orient` should lead with (priority order)

Each is DENSE (names things) and sourced from data orient already has or can read:
1. **STRUCTURE** — the module map: top modules by size, NAMED ("core, http, event, stream, mail").
   The agent learns the shape. Source: module/stats data.
2. **COMPLEXITY CENTERS** — the top complex FILES/SYMBOLS, NAMED — not "342 exceed threshold" but
   "http/upstream, http/core, http/2". Source: the high-complexity query orient already runs (it has
   the rows; it currently emits only the count).
3. **CYCLES** — the import cycles, named (orient already does this). Keep.
4. **DOCS / ENTRY** — docs (already present) + entry points if available.
5. **RELIABILITY** — ONE compressed caveat line ("call-graph 42% resolved — LOW; verify call/dead
   claims against source"), not three separate degradation lines. The certainty/provenance footer
   stays (honest) but compressed at small budget.

## 4. Target output (the dense `orient`)

Small budget, nginx:
```
nginx · 397 files · modules: core, http, event, stream, mail, os
Complexity centers: http/upstream, http/core, http/2, proxy + grpc modules
3 import cycles in http. Docs: README, CONTRIBUTING.
Reliability: call-graph 42% resolved (LOW — verify call/dead claims against source).
[--full for the full breakdown]
```
Every line load-bearing, points the agent somewhere. Large/`--full` = the full module list + more
complexity centers + per-axis reliability + the full certainty/provenance block.

## 5. Budget → depth mapping

- **small**: the headline dense set (structure summary + top ~3 complexity centers + cycles + 1
  reliability caveat + docs).
- **medium**: + more complexity centers + the fuller module breakdown.
- **large / --full**: the complete detail (all hotspots, full module list, per-axis reliability, full
  certainty/provenance).
Budget trades DEPTH within each signal and how many dense signals — NOT "fewer thin meta items."

## 6. Constraints (the bar)

- **HONEST (Fact-Certainty):** complexity centers + structure are EXTRACTED facts (Layer 0–1); the
  reliability caveat is honest; NO overclaiming. The truth-audit's honesty is PRESERVED — this is
  density on top of it, not a relaxation of it. (Do not re-introduce a flat "dead" claim, etc.)
- **REUSE the computed signals** — the data exists; orient prioritizes/synthesizes it. If a needed
  datum (e.g. the per-module size breakdown) is not already read by orient, the builder MAY add the
  read; if it crosses an architectural boundary, surface it.
- **DO NOT regress the JSON envelope** (`CoherenceEnvelope`). The HUMAN output is what changes
  (denser); the JSON carries the same/extended signals. If the JSON shape must change, surface it
  (DECISION_REQUIRED).
- The drilldown commands (`hotspots`/`stats`/`modules`) stay; `orient` synthesizes a dense headline
  and points to them for depth.

## 7. Decisions to surface

- The exact load-bearing set + priority, if ambiguous.
- The budget thresholds (how much depth per budget tier).
- Whether orient needs a new read (per-module breakdown) — surface if it crosses a boundary.
- The JSON shape, if it must change to carry the dense signals.

## 8. Validation (for the IMPL)

- The dense `orient` on a well-known repo (nginx) matches §4 (load-bearing, named, dense), evaluated
  per `docs/testing/end-to-end-usefulness-protocol.md`.
- Honest — no overclaim; the truth-audit's honesty preserved.
- Budget trades depth (small = dense headline; large/`--full` = full), proven.
- cargo green; the JSON contract not regressed (or the change surfaced + ratified).
