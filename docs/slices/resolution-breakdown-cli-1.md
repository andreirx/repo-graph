# RESOLUTION-BREAKDOWN-CLI-1 — expose per-language / per-module call-resolution as a CLI surface

Status: SPECIFIED (2026-07-20) · Track: CLI protocol completeness. First of the operator's
CLI→JS→HTTP sequence (ratified 2026-07-20). Maturity target: MATURE.

## 1. Problem (measured, first-person)

During the glamCRM verification the operator needed the per-language call-resolution split
(java 10.6% / typescript 14.2% / jsx 24.2% / …). **No CLI command exposes it** — it was obtained
by hand-querying the snapshot SQLite (`edges` vs `unresolved_edges` joined through `nodes` to
`files.language`). The VISION states repo-graph "is a machine-readable protocol for agents";
a basic breakdown that requires raw-DB spelunking is a protocol gap — the agent (human or model)
cannot get it through the documented surface. `trust`/`check`/`orient` render only the AGGREGATE
reliability figure; the per-language and per-module decomposition that makes it actionable is not
surfaced anywhere.

The per-language *measurement* machinery already exists (`classification/src/measurement_coverage.rs`
— "per-language function/measured counts, the data-driven honesty verdict"), and the reliability
view (`agent/src/reliability.rs`, `CallReliabilityView`) already computes the aggregate. This slice
SURFACES a decomposition the engine already has the data for; it does not invent a new metric.

## 2. Contract

1. **A CLI command that renders the call-resolution breakdown the operator hand-queried**, at two
   granularities:
   - **per language**: resolved CALLS / unresolved / % resolved, per `files.language`
     (java, typescript, javascript, jsx, python, rust, c, cpp, …), test files separable.
   - **per module**: the same split per module (the 4 glam modules; the 94 package groups where
     that granularity is meaningful) so an agent can see WHICH module's calls are unresolved.
2. **Reuse, do not recompute.** The numerator/denominator MUST be the same populations
   `CallReliabilityView` / `measurement_coverage` already use (resolved = `edges` CALLS;
   unresolved = `unresolved_edges`; the in-scope-vs-external split already computed by
   RELIABILITY-REFRAME-1's shared view). If those views don't expose the per-language grouping,
   add the grouping READ beside them — do NOT fork a second reliability definition (that would
   reintroduce the multi-definition drift RELIABILITY-REFRAME-1 closed). One shared source.
3. **Command shape — decide-and-record, least-new-surface:** prefer extending an existing command
   over a new top-level verb IF one fits (candidate: `rmap check --full` already renders the
   aggregate reliability block — a `--by-language` / `--by-module` breakdown under it is the
   smallest surface; OR a `rmap reliability [--by-language|--by-module]` subcommand if the
   maintainer judges the breakdown is its own concern). The builder picks and records the choice
   with its one-line rationale. Whatever the shape: **structured (JSON) output AND human output**,
   both — this is a protocol surface, agents consume the JSON.
4. **Honesty (non-negotiable, VISION):** every figure carries its basis and the same caveats the
   aggregate does (pre-enrichment LOW, in-scope-vs-external, unclassified conservative band).
   A language/module with zero measured calls renders as UNKNOWN, never a fabricated 0% or 100%.
   The pre-enrichment state is labeled exactly as `check`/`trust` label it today (one shared caveat
   vocabulary — no new phrasing).
5. **NOT in scope:** changing any resolution number, the reliability computation, or the enrichment
   path. This is a READ/RENDER surface over existing facts. No storage schema change (the data is
   already in `edges`/`unresolved_edges`/`nodes`/`files`).

## 3. Stop conditions

Frozen: `CallReliabilityView` / `measurement_coverage` definitions (consume, don't fork), the
reliability caveat vocabulary, storage schema, witness/union/reconciliation surfaces, trust ratio.
If surfacing the breakdown reveals the shared reliability view CANNOT be grouped without duplicating
its logic, that is a FINDING (DECISION_REQUIRED — how to add the grouping without a second
definition), not a silently-forked second computation. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- **Reproduce the operator's numbers through the CLI:** on glamCRM under an ISOLATED state root
  (/private/tmp — NEVER the operator registry ~/Library/Application Support/repo-graph; sha256 the
  real registry before/after), the new command's per-language output MUST match the hand-query it
  replaces (java ≈10.6% / typescript ≈14.2% / jsx ≈24.2% pre-enrichment, modulo the exact
  in-scope-vs-external framing the shared view applies). The build report SHOWS the command output.
- JSON schema: the structured output is valid, documented, and an agent can parse the per-language
  and per-module arrays.
- Aggregate consistency: the per-language figures reconcile to the SAME aggregate `check`/`trust`
  already report (sum of the parts == the whole the aggregate view uses — a named test).
- Zero-measured degradation test (a language present with no measured calls → UNKNOWN, not 0/100).
- Byte-parity: existing `check`/`trust`/`orient` output unchanged (this ADDS a surface).
- Chunked cargo gates (standing pattern); consolidation witness 15/15; SMOKE_ONLY logged run.

## 5. Definition of done

`rmap` exposes the per-language AND per-module call-resolution breakdown (JSON + human) from the
shared reliability/coverage machinery; running it on glamCRM reproduces the operator's hand-queried
split without touching SQLite; figures reconcile to the aggregate; unknowns stay unknown; existing
surfaces byte-identical; gates green. The DB-spelunking gap that motivated this slice is closed.
