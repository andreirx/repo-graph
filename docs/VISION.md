# Repo-Graph Vision

> Rewritten 2026-07-02 (fresh-eyes review at v0.4.0). This document is the
> yardstick for every scope decision. Speculative directions that previously
> lived here now live in `docs/FUTURE-ITERATIONS.md` — parked, **not
> authorized**. Current status and priorities: `docs/ROADMAP.md`.

## The Core

**A deterministic, honest, instantly-queryable map of a codebase's current
structure, so an AI agent looks in the right places instead of re-deriving the
repo from scratch.**

Three commitments are load-bearing. Every feature must serve at least one:

1. **Deterministic extraction.** Facts computed from source, reproducible,
   never model output. This is the differentiator against "just let the agent
   grep": the same snapshot always yields the same answer, and the answer has
   provenance.
2. **Honesty about certainty.** Every fact is labeled with what it is —
   extracted fact, bounded inference, or evidence-backed hint. Unknown is
   never rendered as zero. Degradation is stated, in the reader's language.
   An unlabeled 42%-resolved call graph is *worse* than no call graph,
   because agents act on it. Honesty is what makes the map machine-consumable.
3. **Current-state, in milliseconds.** A long-lived daemon holds the current
   repo state; orientation is cheaper than file archaeology. Git owns
   history; repo-graph owns *now*. This is the economic claim — token and
   wall-clock reduction — that justifies the tool existing.

**The acid test for any feature, slice, or roadmap item: does it improve the
agent's first sixty seconds in an unfamiliar repo, or its next edit?**
If it doesn't, it belongs in `docs/FUTURE-ITERATIONS.md` or nowhere.

## Discovery Over Enforcement

**Discovery is the primary product goal. Enforcement is secondary.**

Discovery means: what exists now, how it is structured, where the current risk
concentrations are, where to look first. Quality *trends* — "what got worse
since baseline", snapshot-to-snapshot quality diffs, risk ranking over time —
are **not** a discovery priority (ratified 2026-07-11): the operator derives
those by other means and will not rely on repo-graph for them. Parked in
`docs/FUTURE-ITERATIONS.md`.

Enforcement means: policy declarations, gate verdicts, waiver semantics, audit
trails, CI blocking. The enforcement machinery exists and works. It is not the
core value proposition, and it is **frozen**: maintained, not extended. No new
governance surface ships without a ratified promotion from
`docs/FUTURE-ITERATIONS.md`.

**Product priority order:**
1. Structural discovery (modules, boundaries, seams, dependencies)
2. Current-state quality signals (complexity, size, hotspots) — orientation
   aids, not a trend system
3. Enforcement (gate pass/fail, policy compliance) — useful, not primary

**Implication for CLI output:** a discovery-first CLI answers "what should the
agent notice?", not "should CI block?". If output collapses meaning into
pass/fail verdicts, it has failed as a discovery surface even when the
enforcement logic is correct.

## Orientation, Not Oracle

The purpose of repo-graph is to help an AI agent **look in the right places,
open the right files, and ask the right questions** — not to do the analytical
work for the agent.

Repo-graph is an orientation substrate, not an exhaustive answer engine. It
narrows the search space and highlights what matters; the agent reads the
actual source and makes the engineering decisions. If repo-graph can surface
more precise information (callers, consumers, exact call sites), it will —
but the primary contract is orientation.

This matters because:
- exhaustive static analysis is expensive and often impossible without
  runtime information
- agents are good at reading and reasoning when pointed to the right place
- over-promising completeness destroys trust when edge cases are missed
- orientation scales; oracle-style completeness does not

## Primary Use Case

**Fast AI agent orientation on the current state of a codebase.** An agent
working on a repo needs to immediately understand:

- what modules exist and what they own
- where the boundaries and seams are
- how modules relate to each other
- what runtime/build environment each module runs under

This is high-value, slow-changing architectural truth that agents cannot
efficiently reconstruct from raw file reads. The product makes it queryable in
milliseconds, not minutes.

**The discovery-first agent loop:**

1. Agent asks `orient` before changing code; repo-graph returns architectural
   context plus current quality signals.
2. Agent checks the documentation inventory and reads the relevant docs;
   missing or stale docs are repaired *in the target repo* (repo-graph finds
   what exists and what drifted; the agent writes the docs).
3. Agent changes code using repo-graph facts plus compact docs for
   orientation.
4. Agent asks `check` to validate the structural state it is leaving behind.
5. Agent decides whether to proceed based on visible facts.

The gate/policy layer is available for teams that want hard enforcement. It is
not the primary interaction model.

## The Certainty Model (Layers 0–4)

Capabilities form a dependency stack. Inner layers pursue deterministic
extracted truth; outer layers may surface partial, source-anchored hints with
explicit unknowns.

| Layer | Contents | Certainty claim |
|-------|----------|-----------------|
| 0 | File inventory, symbol extraction, structural edges (IMPORTS/CALLS/…), unresolved-edge preservation with classification, stable keys | "This is what we extracted. Deterministic, reproducible." |
| 1 | Callers/callees/imports surfaces, declared modules, doc inventory, trust reporting, quality measurements, change-impact primitives | Extracted fact — agents may rely on it |
| 2 | Inferred/operational modules, module dependency graph, runtime/build surfaces, seam rollups, risk/hotspot/churn overlays | "This is what we inferred. Here is the basis." |
| 3 | Framework detectors, HTTP/gRPC/IPC/broker hints, policy-propagation markers, generated-code mapping | "This is what we noticed. Coverage is partial. Open the files." |
| 4 | Declarations, quality policies, assessments, gate verdicts, waivers | "This is what policy says. Underlying facts are preserved." |

Dependency rules:

1. **Layer N requires Layer N−1.** A framework detector without symbol
   extraction is noise.
2. **Certainty claims must match layer.** Layer 0–1 may claim "this is the
   call graph"; Layer 3 may only claim "these files likely contain IPC usage."
3. **Outer layers must surface unknowns.** Raw counts without coverage or
   confidence markers are overclaims.
4. **Governance never replaces extraction.** A waiver suppresses a gate
   failure; it never deletes the measurement. Computed and effective states
   are both queryable.
5. **Maturity is layer-specific.** "C++ shipped" means Layer 0 extraction
   works — it does not imply Layer 3 C++ framework detection exists. The same
   applies per measurement: a quality signal computed for only some supported
   languages must say so wherever it renders.

Every persisted artifact family has an explicit contract (truth class, refresh
policy, identity, degradation, provenance), enforced in code via the artifact
contract registry. See `docs/architecture/artifact-contract-model.md`.

## Semantic Seeding (ratified 2026-08-24)

An agent arrives with a *task*, not a symbol name; exact focus resolution and grep are the
only bridges today. **Embedding-based seeding is ratified as a Layer-3 capability** under
the certainty model, on the strength of EMBED-SEED-SPIKE-1
(`docs/spikes/2026-08-23-embed-seed-spike-1.md`: local 84 MB model, right neighbourhood in
top-5 on 14/16 real glamCRM tasks vs 8/16 lexical). Bounds that keep it inside this VISION:

- **Candidate generator, never answer — integrated, not a separate verb** (human directive
  2026-08-25, superseding the original separate-verb bound): semantic candidates surface ONLY
  through the EXISTING resolution seams — the places whose contract already returns a
  `candidates` array or a no-match (focus resolution in `orient`/`explain`; symbol lookup in
  `callers`/`callees`/`path`) — as an additive, labeled fallback tier that runs only after
  every deterministic tier produced nothing. Deterministic tiers are never reordered or
  diluted; the map (`orient`'s facts, `map`, `modules`) never contains embedding-derived
  facts. Each semantic candidate carries score + provenance + its module/path, and names the
  deterministic follow-up (`explain <candidate>`) from which the full neighbourhood is
  reachable — module and imports directly; callers one further `explain` away on any symbol
  it lists (amended 2026-08-25: a single command yielding file-level callers does not exist,
  and widening resolved `explain` for everyone to satisfy this wording would break its
  byte-stable deterministic output) — ≤5 candidates.
- **Evidence-backed hint, labeled.** Every seed carries score + provenance
  (`source: embedding`, model id); ranking is a fixed formula — no LLM in the loop.
- **Deterministic given its pins.** Every vector is pinned `(model_id, dim, content_sha)`;
  mismatch is a hard error, staleness recomputes from content hash at index/refresh.
- **Local and optional.** Local model only, no API key, no network; absence degrades to
  "no hints", never to degraded orientation.

**Ratified uses (human, 2026-08-25; amended 2026-08-30).** The semantic substrate serves three
uses, all Layer-3:
(i) the resolution fallback tier above; (ii) **`rmap find "<concept>"`** — where the semantic
seeds are the DEMOTED LOWER TIER (amendment 2026-08-30, FIND-FACTS-1): `find` answers FIRST
from a deterministic lexical match over the fact tables (every hit labeled with its fact class
and the command that renders it), and only below that renders the embedding seeds, named as
ranked guesses ("likely areas for X" with scores + provenance, never a completeness claim);
facts outrank similarity, and the verb answers even when the local model is down (the seed
tier then says unavailable-with-reason); (iii) **cross-module concern hints** — cohesive embedding clusters spanning deployable
modules surfaced on the module/boundary discovery surfaces as labeled seam/concern candidates
(measured: glamCRM's sales-targets/tenant-brand/exchange-rate/auth clusters, spike addendum
2026-08-25). Uses (i)+(ii) ship in the first IMPL; (iii) follows once its rendering surface is
specced.

Scope beyond these bounds (semantic search as an answer surface, cross-repo embeddings,
model management) stays in `docs/FUTURE-ITERATIONS.md` unless separately ratified.

## Honesty Rules

- **Unknown is never zero.** `null`/`unknown` means not measured; `0` means
  measured and absent. Degenerate values that a reader would mistake for
  measurements render as unknown, with the reason.
- **Coverage is part of the fact.** A signal that only covers part of the repo
  (by language, by resolution rate, by evidence source) states its coverage
  where it renders — not in a doc the reader will never open.
- **Degradation is a first-class output.** When the answer basis is weaker
  than usual (fallback store, missing enrichment, stale snapshot), the surface
  says so and says what would improve it — as a concrete next action the
  reader can run.

### Labels speak the reader's language, not ours

Every external label describes the reader's subject — *their* code,
dependencies, architecture — in terms that pertain to *their* problem; never
repo-graph's internal processing state. "Library call (`serde`)", "system
call", "dynamic dispatch" tell the agent what a symbol *is* and where to look.
"Unresolved", "below 50% threshold", "enrichment phase did not run" narrate
*our* pipeline — internal diagnostics: useful to us, noise to the consumer.
Honesty applies in the reader's frame too: "likely a `serde` call (inferred
from the import + manifest)" is honest *and* reader-facing;
"external_library_candidate, basis=specifier_matches_package_dependency" is
honest but internal. The test: does the label describe the reader's world or
ours? Only the reader's ships. (Internal diagnostics stay available on an
explicit debug surface — they don't pollute the product.)

## Operational Architecture

The end-state runtime is a **long-lived daemon** holding the current repo
state in memory. Primary truth: the current in-memory model (file inventory,
symbol graph, module catalog, boundary facts, runtime/build model). Secondary
truth: a persistent disk cache for warm starts, incremental update, and delta
rebuild.

**Git owns history; repo-graph owns current-state structure.** Delta indexing
is a recomputation strategy for current-state truth, not a substitute history
system. "What changed" queries compare current vs a git baseline, not a
retained snapshot timeline. Snapshot retention biases toward latest full +
minimal transient comparison state.

SQLite is the persistence and fallback mechanism, not the conceptual center.
Where SQLite remains the only source for a fact class (e.g. unresolved-call
disposition — RED by design), that is a stated floor, not drift.

## Protocol Surface Standard

Repo-graph is not a binary with commands; it is a machine-readable protocol
for agents. Intent is transmitted through three layers, all required:

1. **Command naming** — names imply workflow role (`orient` = safe starting
   point, `check` = validation before handoff, `gate` = policy with exit
   codes, `callers`/`dead`/`trust` = focused investigation). If an agent must
   guess whether a command is safe, destructive, or policy-carrying, the
   naming failed.
2. **Output contracts** — structured output encodes the policy semantics:
   inherited vs new, comparable vs not, confidence, reasons with provenance.
   The verification question: *can an agent learn the optimization target by
   reading the output alone?* Raw counts without semantic categories fail.
3. **External workflow instructions** — canonical docs (CLAUDE.md, AGENTS.md,
   host integration) tell consumer agents when and why to call each command.
   The tool gives structured evidence; the outer instructions give the loop.

A command that passes technical tests but fails protocol-surface verification
is not shippable.

## Change Doctrine — nothing frozen for its own sake

The agent-facing discovery output is the product: make it more useful, denser,
more honest — and **break from a past shape that wasn't ideal** rather than
ossify it. No contract is sacred; each is subject to one question: **does the
benefit to this vision outweigh the cost of changing it?** Cost is
**load-bearing assumptions disturbed, not lines of code**.

This judgment is surfaced, not made silently, scaled by blast radius: a
low-cost improvement to discovery output is made and recorded in one line; a
change that disturbs load-bearing assumptions (an internal seam, a governance
object — gate verdicts, exit codes, CI-facing specs) is a
**DECISION_REQUIRED**, surfaced *before* the change with the trade-off
explicit. Governance and CI-facing surfaces carry the most assumptions and get
the highest bar and versioned propagation.

## Value Frontier

The highest-value, slowest-changing substrate is the **architectural
understanding layer**:

1. **Modules** — declared, operational, and inferred boundaries; file
   ownership; inter-module relationships. The primary orientation layer.
2. **Boundaries and seams** — API provider/consumer structure, state
   boundaries, event/queue boundaries, service/plugin seams, external
   touchpoints. Mechanism detectors (HTTP, gRPC, CLI, DB, …) are evidence
   tracks feeding the seam model, not Layer 0/1 substrate themselves.
3. **Runtime/build environment** — how each module runs, what config defines
   it, what build system owns it, what deploy surface it belongs to.
4. **Documentation as first-class evidence** — the docs themselves are the
   data; repo-graph finds what exists, what is missing, and what likely
   drifted. Repo-graph is not a documentation authoring system.
5. **Current-state quality signals** — deterministic measurements (complexity,
   size, hotspots, cycles, unresolved-edge pressure) that tell the agent where
   the risk concentrations are *now*, honestly labelled with per-language
   coverage. Snapshot-delta quality diffs / health-vector-with-trend are
   parked (`docs/FUTURE-ITERATIONS.md`, ratified 2026-07-11) — the operator
   does not rely on repo-graph for trends.

The extraction layer is the necessary foundation; the value is the
architectural interpretation on top of it.

## Agent Operating Model

Operational contract for implementation agents working *on* repo-graph:

**Priorities**
1. **Correctness over surface expansion.** Do not ship a feature whose
   verdict you cannot defend.
2. **Preserve computed truth under policy overlays.** Suppression layers
   never erase the underlying fact; every read surface exposes both.
3. **Do not compare non-comparable snapshots.** Check toolchain provenance;
   report NOT_COMPARABLE rather than fake numbers.
4. **Do not erase superseded records.** Supersession creates rows; audit
   depends on it.
5. **Document every divergence and temporary debt** at the time it is made
   (`docs/TECH-DEBT.md`).

**Decision rules**
- New policy overlay → preserve computed fact alongside effective view.
- Identity change → define the versioning contract and migration path first.
- CI-facing command → exit-code semantics explicit; distinguish "judged and
  failed" from "could not reach a verdict".
- Snapshot-spanning feature → comparability rules first.
- Breaking JSON on a governance surface → update the normative contract docs
  (`docs/architecture/gate-contract.txt`, `versioning-model.txt`,
  `measurement-model.txt`).

**Operational sequence** for new capability: support module → storage/model →
feature composing them → tests → contract docs → limitations recorded. Do not
collapse steps.

## Distribution Principles

- The **daemon is a first-class runtime** with lifecycle management, not an
  optional manual tool.
- **Binary-first distribution** — no Rust toolchain required to install.
- **Host integrations are deterministic, reviewable, reversible**: detect,
  show, back up, patch only what was selected, provide rollback. Never
  silently rewrite developer automation.
- **Lifecycle policy lives in `rmap hook` commands**; host-specific files are
  thin shims (Claude Code / Codex: lifecycle hooks; Cursor: MCP + rules —
  hosts are not forced into one model).
- Platform priority: macOS, then Linux; Windows explicitly deferred.

## Strategic Position

Repo-graph does not race platform vendors on generic local indexing. It owns
the layer vendors are structurally less likely to prioritize:

- portability across tools, agents, and model vendors
- deterministic structural discovery with current-state quality signals
- architectural understanding that persists across sessions

Token reduction is the measurable proof: a `callers` query replaces a
multi-file search; impact analysis replaces heuristic file reading. Fewer
tokens, fewer tool calls, more complete answers.

## What This Vision Is Not (Now)

Repo-graph today is **not** a system of record for all engineering decisions,
not a compliance/traceability platform, not a fleet-intelligence layer, and
not a requirements database. Those directions — versioned requirements,
evidence chains, process-as-data, cross-repo topology, high-assurance
tooling — are parked with their full rationale in
`docs/FUTURE-ITERATIONS.md`. They may become real after the core is
undeniable. The promotion path is: operator ratification → this document
amended → roadmap entry. Until then, the acid test above governs.
