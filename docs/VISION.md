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

### Three Version Classes

Repo-graph handles three distinct classes of version information.
They must not be mixed or conflated.

**Provenance versions** (internal to repo-graph):
What toolchain produced this snapshot. Schema compat, extraction
semantics, measurement formulas, stable key format. These answer
reproducibility: "can I compare these two snapshots?"

**Extracted domain versions** (from the codebase itself):
What the system claims to be. Package versions from manifests,
API versions from route prefixes or OpenAPI specs, database
migration versions from migration files, event/schema versions
from topic names or schema registries, git tags, lockfile
dependency versions. These answer compatibility and intent:
"which consumers depend on API v1?" "which release shipped
contract version X?"

**Declared versions** (human-authored):
User-supplied version facts where extraction is impossible or
incomplete. Requirement versions, contract versions, process
versions, exception/waiver versions. These answer governance:
"which process version governed this decision?"

Each extracted or declared version should carry:
- source (extracted | declared | inferred)
- evidence link (which file, line, or artifact)
- confidence (1.0 for extracted/declared, < 1.0 for inferred)

The bridge from "graph of code" to "versioned engineering substrate"
is extracted domain versions. Without them, repo-graph knows what
the code does structurally but not what the system claims to be
or what compatibility promises it makes.

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

## Quality Measurement As A First-Class Concern

Code quality is not one number. It is a health vector with trend:

- structure: modularity metrics (instability, abstractness, coupling, cycles)
- complexity: AST-derived metrics (cyclomatic, cognitive, nesting, size)
- coverage: test coverage weighted by risk (complexity, churn, boundary exposure)
- churn: change frequency and hotspot pressure
- policy: violation count against declared thresholds

These are four distinct kinds of truth and must be stored separately:

- Evidence: raw artifacts from external tools (coverage JSON, git log)
- Measurements: deterministic facts from graph, AST, or imported evidence
- Policies: human-declared thresholds and quality gates
- Assessments: derived judgments combining measurements with policies

The strongest product move is not a composite score. It is a per-module
health vector that trends across snapshots and feeds into pre-merge
verification. See docs/architecture/measurement-model.txt for the
full design.

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
- quality measurement (structure, complexity, coverage, churn)
- impact/effect analysis
- required test obligations
- pre-merge structural verification
- structured evidence summaries
- registry and plugin liveness edges
- extracted domain version intelligence

Registry-driven architectures (CMS block renderers, plugin systems,
extension registries, render maps) wire liveness through mechanisms
invisible to import/call analysis. Until those are modeled as edges,
dead code detection overstates confidence on these codebases.

High-value edge types for registry liveness:
- REGISTERED_BY: plugin/extension registered into a composition root
- RENDERS_BLOCK: block-type render map binding component to type key
- PROVIDES_EXTENSION: extension export registered into a registry
- DISPATCHED_BY: string-key or config-driven component dispatch

These are framework-extractor concerns — same architectural layer as
Express ROUTES_TO or NestJS DI bindings. They should be implemented
as framework extractors, not as generic resolution improvements.

Until registry edges are extracted, `graph dead` results on
registry-heavy codebases should be interpreted as "graph orphans"
(no inbound evidence in the modeled graph), not "safe to delete."
The tiered dead-code model (definitely_unreferenced / graph_orphaned /
suppressed_by_declaration) addresses the presentation side.

Extracted domain version intelligence bridges the gap from "graph of
code" to "versioned engineering substrate." Version sources to extract:

- Package/app versions from manifests (package.json, pyproject.toml,
  Cargo.toml, pom.xml)
- API versions from route prefixes, OpenAPI specs, protobuf package
  versions, GraphQL schema conventions
- Database migration versions from migration filenames, ORM metadata
- Event/schema versions from topic names, schema registries,
  AsyncAPI/JSON Schema docs
- Git tags and release branches
- Lockfile dependency versions for third-party compatibility context

Extracted versions are stored as measurements with source file
provenance embedded in value_json. For external artifacts (coverage
reports, git metrics), evidence_links connect artifacts to the
measurements they support. They enable queries like:
- which snapshots correspond to app version 2.4.x
- which modules still depend on API v1
- which migrations introduced schema version 37
- which release first shipped contract version X

### Horizon 3: Versioned Engineering Substrate

Expand to full lifecycle intelligence:

- versioned requirements and contracts
- versioned artifacts/evidence/process
- database/version compatibility reasoning
- cross-repo fleet operations and migration planning
- declared versions for governance objects where extraction is impossible

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
