# Future Iterations — Parked Directions

> Moved out of `docs/VISION.md` on 2026-07-02 (fresh-eyes review at v0.4.0).
> These are deliberate somedays: coherent, possibly valuable, **not
> authorized**. Nothing in this file justifies a slice, a crate, a schema, a
> migration, or a CLI surface. The promotion path is: operator ratification →
> VISION amendment → ROADMAP entry. The reason they are parked: each is an
> *enforcement/governance/fleet* product, while the proven core is a
> *discovery* product for agents — and the E2E usefulness gates keep showing
> that the discovery surface is where the product wins or loses trust.

## The System-of-Record End State

The most expansive framing: repo-graph becomes the system of record for
software engineering decisions and evidence. Every meaningful engineering
object is versioned, linked, and queryable — requirements, contracts, modules,
schemas, artifacts, tests, process states, approvals, runtime observations.
The result is not "documentation for authorities" but a living operational
intelligence graph engineers and agents use daily: the engineering memory and
verification substrate of an organization, portable across platforms and
model vendors, trusted for high-stakes decisions, capable of bringing
high-assurance discipline to mainstream software at low operational cost.

## High-Assurance Engineering, Made Cheap

Historically, avionics/medical-device rigor is expensive because humans
maintain traceability manually. AI plus deterministic extraction changes that
cost curve. Repo-graph could make high-assurance practices cheap enough for
normal software teams: full traceability from requirement to code to test to
deployment evidence; explicit invariants and safety constraints; strong
versioning of all system boundaries; reproducible decision and verification
records; auditability without dead paperwork. Rigor becomes operational
leverage, not compliance theater.

## Process As Entropy Containment

Safety-critical process was historically a human straightjacket compensating
for human weakness at exhaustive, repetitive detail work. LLMs invert that
profile: strong at expansion and clerical repetition, weak at systems
judgment and long-horizon correctness, prone to probabilistic drift. That
changes the role of process: high-assurance process becomes a deterministic
containment vessel for stochastic agents —

1. intent captured as structured requirements, constraints, and non-goals
2. verification obligations generated before implementation
3. tests and checks as executable boundary conditions
4. implementation judged against locked requirements and verified obligations
5. evidence linked back to the requirement, contract, and process version
   that demanded it

The system externalizes and enforces the parts humans are bad at sustaining,
while keeping architecture, trade-offs, and exceptions in human hands. This
would position repo-graph as an engineering control system for stochastic
agents, not merely a repository index.

## Versioning-First Model (Beyond Provenance)

What is shipped and stays in scope: **toolchain provenance** (snapshot
comparability — the NOT_COMPARABLE discipline) and **extracted package
versions** from manifests.

What is parked: versioning as a first-class model across the entire
lifecycle — requirements versions (`REQ-v`), contract versions (`API-v`,
`EVENT-v`, `SCHEMA-v`), module versions, artifact/evidence versions,
database schema lineage, process versions (`PROC-v`), decision/exception
versions (`ADR-v`). Three distinct version classes that must not be
conflated:

- **Provenance versions** (internal): what toolchain produced this snapshot —
  answers reproducibility. *(shipped)*
- **Extracted domain versions** (from the codebase): package versions, API
  versions from route prefixes/OpenAPI, migration versions, event/schema
  versions, git tags, lockfile dependencies — answers compatibility and
  intent ("which consumers depend on API v1?"). *(manifest versions shipped;
  the rest parked)*
- **Declared versions** (human-authored): requirement/contract/process
  versions where extraction is impossible — answers governance. *(parked)*

Each extracted or declared version would carry source
(extracted|declared|inferred), an evidence link, and confidence. Extracted
domain versions are the bridge from "graph of code" to "versioned engineering
substrate" — enabling queries like "which snapshots correspond to app version
2.4.x" or "which release first shipped contract version X".

## Traceability As A Product Primitive

Traceability as queryable, not narrative. Each change produces a
machine-navigable chain:

`Requirement → Capability → Module/Contract → Change → Test Obligation →
Evidence → Release → Runtime Observation → Learning/Policy Update`

Supports impact analysis, migration sequencing, rollback confidence, targeted
audits, incident forensics, and better agent decision quality.

## Queryable Requirements And Constraints

Requirements as structured, versioned objects: objective, constraints,
non-goals, acceptance criteria, safety/privacy/compliance requirements,
verification obligations, rollout conditions. Repo-graph maps each
requirement version to impacted modules/contracts and to evidence proving
satisfaction. (A minimal `declare requirement` + obligations + waivers layer
is shipped and frozen; this section is the full product around it.)

## Contract-Centric System Intelligence

As generation gets cheaper, durable value moves from implementation internals
to boundaries. Contracts as first-class assets: API contracts and
compatibility windows, event contracts and payload evolution, database
ownership and schema contracts, module dependency contracts and forbidden
edges, operational contracts (SLO/SLA, failure-mode handling). Contract drift
detection as a core safety mechanism.

## Database Versioning And State Reality

Code can be regenerated quickly; state cannot. Schema version lineage,
migration dependency graph, cross-repo table ownership and access
classification, compatibility impact per migration, evidence that migration
safety checks passed. Turns database changes from tribal-risk operations into
governed, queryable workflows.

## Artifact And Evidence Intelligence

Artifacts (test runs, contract checks, replay traces, coverage reports,
migration checks, benchmarks) versioned, content-addressed, linked to
snapshots and obligations and to requirement/contract/module versions.
Evidence as part of the graph, not an attachment.

## Process As Versioned, Executable Intelligence

Review policies, risk tiers, merge/release gates, approval roles, exception
pathways — versioned and queryable. Which process version governed each
change; whether all required gates were satisfied. Accountability preserved
while iterating rapidly.

## Cross-Repo Intelligence As The Durable Moat

Single-repo indexing is table stakes; durable value is at the fleet layer:
inter-service dependency topology, API/event contract drift across repos,
shared database ownership conflicts, deployment coupling and blast radius,
migration order under dependency constraints. Where large organizations fail
today and where deterministic graph intelligence compounds. (Horizon 3;
explicitly not next.)

## Registry / Framework Liveness Edges

Registry-driven architectures (CMS block renderers, plugin systems, extension
registries, render maps) wire liveness through mechanisms invisible to
import/call analysis. Until modeled as edges, dead-code detection overstates
confidence on those codebases — which is why the public dead-code surface is
withdrawn behind the reliability boundary today. High-value edge types:
`REGISTERED_BY`, `RENDERS_BLOCK`, `PROVIDES_EXTENSION`, `DISPATCHED_BY`.
These are framework-extractor concerns (same layer as Express `ROUTES_TO`),
not generic resolution improvements. This item is closest to promotion: it is
discovery-core, currently sequenced behind coverage-backed liveness on the
roadmap.

## Token-Reduction Benchmarking Program

The token-reduction claim (in VISION) eventually deserves measurement: 
benchmark tasks with defined expected artifacts; comparable protocols (pure
agent discovery vs rmap-assisted); measuring input/output tokens, tool calls,
wall-clock, correctness, completeness, human verification burden; results
stored as evidence artifacts linked to snapshots and toolchain versions.

## Enforcement Progression, Phase 2

Host integration starts informational (orientation injection, post-edit
refresh, stop-time summary — shipped direction). Phase 2 would add selective
enforcement: block raw SQL before CLI, block completion without a validation
transcript, block stale-DB use in critical flows. Parked until field
experience with Phase 1 accumulates.

## Quality Trend Discovery (Snapshot-To-Snapshot Diffs, Risk Ranking Over Time)

Parked 2026-07-11 (operator ratification). Previously the VISION's Value
Frontier item 5 and the ROADMAP's "Quality discovery surface" section:
snapshot-to-snapshot quality diff, "what got worse" delta surfacing in
`orient`/`check`, risk prioritization combining complexity with
churn/coverage/boundary signals, the health-vector-with-trend model, and the
comparability rules (toolchain provenance → `NOT_COMPARABLE`) that gate it.

**Why parked:** the operator derives quality trends by other means and will
not rely on repo-graph for them. Current-state quality signals (complexity,
hotspots, cycles — honestly coverage-labelled) REMAIN in scope as orientation
aids; what is parked is the *trend/delta* system. This also aligns with the
retention ratification ("git has history — I want DISCOVERY"): internal
snapshots exist for refresh mechanics, not as a queryable quality timeline.

Promotion path (as for everything here): operator ratification → VISION
amended → ROADMAP entry.
