# Repo-Graph Vision

## Core Direction

Repo-graph is a deterministic engineering intelligence system, not a chat layer and not a generic code indexer.

The long-term direction is to become the portable, queryable substrate for:

- structural code facts
- architectural intent
- requirements and constraints
- versioned contracts
- versioned artifacts and evidence
- cross-repo topology and dependency intelligence

The central bet is simple: generic "code understanding" will be commoditized, but high-assurance, traceable, versioned engineering intelligence across repositories will remain valuable and under-served.

## Strategic Position

Repo-graph should not race platform vendors on generic local indexing features.
It should own the layer that vendors are structurally less likely to prioritize:

- portability across tools, agents, and model vendors
- deterministic evidence trails
- policy and governance enforcement
- cross-repo and fleet-level intelligence
- auditable traceability from intent to runtime evidence

This keeps repo-graph useful even if major AI platforms provide built-in code indexing.

## Vision Statement

Build the system of record for software engineering decisions and evidence.
Every meaningful engineering object is versioned, linked, and queryable:

- requirements
- contracts
- modules
- schemas
- artifacts
- tests
- process states
- approvals
- runtime observations

The result is not "documentation for authorities." The result is a living operational intelligence graph that engineers and agents use every day.

## High-Assurance Engineering, Made Cheap

Historically, avionics/medical-device style rigor is expensive because humans maintain traceability manually.
AI plus deterministic extraction changes that cost curve.

Repo-graph should make high-assurance practices cheap enough for normal software teams:

- full traceability from requirement to code to test to deployment evidence
- explicit invariants and safety constraints
- strong versioning of all system boundaries
- reproducible decision and verification records
- auditability without creating dead paperwork

Rigor becomes operational leverage, not compliance theater.

## Versioning-First Model

Versioning is not only for source files. It is first-class across the entire engineering lifecycle.

### What Must Be Versioned

- Requirements versions (`REQ-v`)
- Contract versions (`API-v`, `EVENT-v`, `SCHEMA-v`)
- Module versions (`MODULE-v`)
- Artifact versions (`ART-v`) for generated outputs and evidence bundles
- Database schema versions (`DB-v`, migration lineage)
- Process versions (`PROC-v`) for workflows, policies, and gates
- Test evidence versions (`EVIDENCE-v`) linked to snapshots and obligations
- Decision and exception versions (`ADR-v`, approved waivers)

### Why This Matters

Without versioning across these layers, teams can answer "what changed in code?" but not:

- which requirement version this release satisfies
- which contract version each consumer relies on
- which database version guarantees compatibility
- which evidence version justifies the merge/deploy decision
- which process/policy version governed the decision at that time

Repo-graph should answer those deterministically.

## Traceability As A Product Primitive

Traceability should be queryable, not narrative.

Each change should produce a machine-navigable chain:

`Requirement -> Capability -> Module/Contract -> Change -> Test Obligation -> Evidence -> Release -> Runtime Observation -> Learning/Policy Update`

This supports:

- faster impact analysis
- safer migration sequencing
- stronger rollback confidence
- targeted audits and incident forensics
- better agent decision quality

## Queryable Requirements And Constraints

Requirements should be represented as structured, versioned objects with explicit fields:

- objective
- constraints
- non-goals
- acceptance criteria
- safety/privacy/compliance requirements
- verification obligations
- rollout conditions

Repo-graph should map each requirement version to impacted modules/contracts and to specific evidence that proves satisfaction.

## Contract-Centric System Intelligence

As generation gets cheaper, durable value moves from implementation internals to boundaries.
Repo-graph should treat contracts as first-class assets:

- API contracts and compatibility windows
- event contracts and payload evolution
- database ownership and schema contracts
- module dependency contracts and forbidden edges
- operational contracts (SLO/SLA, failure-mode handling)

Contract drift detection becomes a core safety mechanism.

## Database Versioning And State Reality

Code can be regenerated quickly. State cannot.
Database evolution remains one of the highest-risk domains.

Repo-graph should maintain:

- schema version lineage
- migration dependency graph
- cross-repo table ownership and access classification
- compatibility impact per migration
- evidence that migration safety checks passed

This turns database changes from "tribal-risk operations" into governed, queryable workflows.

## Artifact And Evidence Intelligence

Artifacts (test runs, contract checks, replay traces, coverage reports, migration checks, benchmark outputs) should be:

- versioned
- content-addressed
- linked to snapshots and obligations
- linked to requirement/contract/module versions

Evidence is not an attachment; it is part of the graph.

## Process As Versioned, Executable Intelligence

Process should also be versioned and queryable:

- review policies
- risk tiers
- merge gates
- release gates
- approval roles
- exception pathways

Repo-graph should capture which process version governed each change and whether all required gates were satisfied.
This preserves accountability while enabling rapid iteration.

## Cross-Repo Intelligence As The Durable Moat

Single-repo indexing is table stakes.
Durable value is at the fleet layer:

- inter-service dependency topology
- API and event contract drift across repos
- shared database ownership conflicts
- deployment coupling and blast radius
- migration order under dependency constraints

This is where large organizations fail today and where deterministic graph intelligence provides compounding advantage.

## Horizon Roadmap

### Horizon 1: Deterministic Trust

Prove correctness and evidence quality:

- high-precision graph extraction
- explicit unresolved-edge diagnostics
- snapshot lineage and reproducibility
- stable declaration workflows

### Horizon 2: Governance And Verification

Make repo-graph part of day-to-day change safety:

- architecture violation detection
- impact/effect analysis
- required test obligations
- pre-merge structural verification
- structured evidence summaries

### Horizon 3: Versioned Engineering Substrate

Expand to full lifecycle intelligence:

- versioned requirements and contracts
- versioned artifacts/evidence/process
- database/version compatibility reasoning
- cross-repo fleet operations and migration planning

## Product Principle

Keep deterministic facts, declarations, and inferences separate.
Use AI for synthesis and automation, but ground decisions in explicit, inspectable evidence.

If an answer cannot show versioned provenance, evidence chain, and policy context, it is advisory only, not authoritative.

## End State

Repo-graph becomes the engineering memory and verification substrate of an organization:

- portable across platforms and model vendors
- trusted for high-stakes engineering decisions
- useful in daily development, not only audits
- capable of bringing high-assurance discipline to mainstream software at low operational cost

That is the long-term direction: not more generated code, but higher-fidelity control over system reality.
