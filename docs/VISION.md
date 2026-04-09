# Repo-Graph Vision

## Core Direction

Repo-graph is a deterministic engineering intelligence system, not a chat layer and not a generic code indexer.

The primary use case is **fast AI agent orientation on the current state of a codebase.** An agent working on a repo needs to immediately understand:

- what modules exist and what they own
- where the boundaries and seams are
- how modules and projects relate to each other
- what runtime/build environment each module runs under
- what changed since the last known state

This is the high-value, slow-changing architectural truth that agents cannot efficiently reconstruct from raw file reads. The product exists to make this truth queryable in milliseconds, not minutes.

The long-term direction is to become the portable, queryable substrate for:

- structural code facts
- architectural intent (modules, boundaries, seams, runtimes)
- requirements and constraints
- versioned contracts
- versioned artifacts and evidence
- cross-repo topology and dependency intelligence

The central bet is simple: generic "code understanding" will be commoditized, but high-assurance, traceable, versioned engineering intelligence across repositories will remain valuable and under-served.

## Operational Architecture

The end-state runtime is a **long-lived daemon** holding the current repo state in memory. Not SQLite-first. Not snapshot-history-first.

**Primary truth:** current repo state in memory.
- current file inventory
- current symbol graph (nodes + resolved/unresolved edges)
- current module catalog and file-to-module ownership
- current boundary/provider/consumer facts
- current runtime/build environment model
- current config/build context

**Secondary truth:** persistent disk cache.
- warm start after daemon restart or crash recovery
- content hashes and file fingerprints for incremental update
- raw extraction facts for delta rebuild
- module/boundary/runtime cache

**Not the primary concern:**
- indefinite snapshot history (git provides history)
- many retained snapshots (latest full + latest delta is sufficient)
- trend databases or archaeology (useful later, not core)
- WAL-heavy retained state

The current SQLite design is the transition mechanism. It is useful now for persistence and query infrastructure. It is not the conceptual center of the end-state architecture. The daemon's in-memory model will become the conceptual center.

## Value Frontier

The highest-value, slowest-changing substrate is not the raw code graph. It is the **architectural understanding layer:**

**1. Modules** — declared, operational, and inferred module boundaries. File ownership. Inter-module relationships. This is the primary orientation layer for agents.

**2. Boundaries and seams** — API providers/consumers (HTTP, gRPC, CLI). State boundaries (database, filesystem, cache). Event/queue boundaries. Framework entrypoints. Service/plugin seams. External system touchpoints.

**3. Runtime/build environment** — how each module runs, what config defines it, what build system owns it, what deployment surface it belongs to, what compile context applies.

These three layers change slowly relative to code. They are expensive for an agent to reconstruct from scratch. They are cheap to maintain incrementally. And they are what an agent needs first when orienting in a codebase.

The extraction layer (symbols, imports, calls) is the necessary foundation. But the value is in the architectural interpretation built on top of it.

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

## Agent Operating Model

This section is the operational contract for implementation agents working on repo-graph. It is not aspirational. It tells an agent how to decide what is correct **right now**.

### Current Product Truth

| Area | Shipped | Partial | Future |
|------|---------|---------|--------|
| Structural graph | IMPORTS, CALLS extraction; callers/callees/path/dead/cycles | Call graph recall on SDK/functional patterns | Framework extractors (Express, NestJS) |
| Measurements | Graph stats, AST metrics, coverage, churn, hotspots, risk | Coverage/churn snapshot-tight filtering | Custom measurement kinds |
| Declarations | module, boundary, entrypoint, invariant, requirement, obligation, waiver | `supersedes_uid` lineage exposure | Policy/gate-mode versioning |
| Verdicts | 5-state effective model (computed + effective + WAIVED) | — | Persisted verification records |
| Gate | `rgr gate` with 3 modes, 3 exit codes, waiver overlay | — | CI integration examples |
| Versioning | Toolchain provenance, obligation_id identity, version-scoped waivers | Extracted domain versions (package.json only) | Contract/API/migration versions |
| Trust reporting | v1-validation-report.txt (prose) | — | Machine-queryable trust boundaries |
| Dead-code confidence | Graph orphan detection | Registry/plugin liveness not modeled | REGISTERED_BY, RENDERS_BLOCK edges |
| Cross-repo | — | — | Fleet supergraph, drift detection |

### Agent Priorities

1. **Correctness over surface expansion.** Do not ship a feature whose verdict you cannot defend.
2. **Preserve computed truth under policy overlays.** Waivers, entrypoint declarations, and any future suppression layer must NEVER erase the underlying computed fact. Every read surface exposes both.
3. **Do not compare non-comparable snapshots.** Check toolchain provenance before computing deltas. Report NOT_COMPARABLE rather than fake numbers.
4. **Do not erase superseded records.** Supersession creates new rows; it does not overwrite. Audit depends on this.
5. **Document every divergence and temporary debt.** Every assumption, shortcut, and known limitation must be recorded in the appropriate doc at the time it is made.

### Canonical Concepts

| Concept | One-line meaning | Source of truth | Stability |
|---------|------------------|-----------------|-----------|
| snapshot | Point-in-time indexed state of a repo | `snapshots` table | stable |
| declaration | Human-supplied fact or policy object | `declarations` table | stable |
| requirement | Versioned engineering intent with obligations | `declarations` kind=requirement | stable |
| obligation | Verification check inside a requirement, with stable `obligation_id` | `verification[]` in requirement value | stable |
| waiver | Version-scoped exception suppressing gate failure | `declarations` kind=waiver | stable |
| measurement | Deterministic value computed from graph/AST/imports | `measurements` table | stable |
| inference | Derived assessment with confidence | `inferences` table | stable |
| computed_verdict | Raw obligation evaluation result (4-state) | `resolveEffectiveVerdict()` output | frozen |
| effective_verdict | Computed verdict with waiver overlay (5-state) | `resolveEffectiveVerdict()` output | frozen |
| gate outcome | Policy reduction of verdicts to pass/fail/incomplete + exit code | `reduceToGateOutcome()` | frozen |

Stability: **frozen** = breaking change requires contract version bump; **stable** = additive changes only; **evolving** = contract under iteration.

Normative contracts:
- `docs/architecture/gate-contract.txt` — verdicts, waivers, gate, obligation identity
- `docs/architecture/versioning-model.txt` — toolchain provenance
- `docs/architecture/measurement-model.txt` — four-layer truth model

### Trust Boundaries

| Category | What it contains | Confidence |
|----------|------------------|------------|
| Deterministic | IMPORTS edges, structural node identity, measurements from AST/graph | high — replay-reproducible |
| Inferred | Hotspot scores, risk scores, dead-code candidates | medium — formula-dependent |
| Unresolved | Ambiguous calls (multiple candidates), dynamic dispatch, SDK methods | tracked but not persisted |
| Snapshot-scoped | Nodes, edges, per-function metrics | correct within snapshot |
| Repo-scoped, not snapshot-tight | Coverage import, churn import (filtered against repo-level file inventory) | known limitation — see measurement-model.txt |
| Framework-opaque | Registry/plugin/extension-driven call paths | not modeled — use `declare entrypoint` to suppress false positives |

An agent MUST surface uncertainty when consumers will act on a result. Do not present inferred or snapshot-scoped data without its category label.

### Operational Sequence

When adding a capability, follow this order. Do not collapse steps.

1. **Support module.** Primitives, pure helpers, types, validators. No storage, no CLI.
2. **Storage / model changes.** Schema migration if needed. Port additions. Adapter implementation.
3. **Feature using support module.** Composes primitives. Does not re-implement them.
4. **Tests.** Unit tests for pure helpers. Integration tests for storage. End-to-end CLI tests.
5. **Contract docs.** Update `v1-cli.txt` for CLI-visible changes. Update normative docs if the contract surface changed. Update architecture summary docs (schema, project-structure, measurement-model, versioning-model) with links back to normative docs.
6. **Limitations and debt.** Record every assumption, every divergence from the proposal, every deferred cleanup. Do not leave these uncommitted.

### Decision Rules

- **If a policy overlay is added, preserve underlying computed fact.** Never erase. Every read surface exposes both computed and effective views.
- **If identity changes, define the versioning contract.** What tuple constitutes identity? What changes invalidate inheritance? What is the migration path?
- **If a new CLI surface changes JSON, update normative contract docs.** Breaking JSON changes require updates to `gate-contract.txt` or equivalent normative specs.
- **If a command is CI-facing, define exit code semantics explicitly.** Distinguish "judged and failed" from "could not reach a verdict."
- **If a feature spans snapshot history, state comparability rules first.** Toolchain mismatch → NOT_COMPARABLE, not silent drift.

### Near-Term Roadmap For Agents

Ordered by current product priority. The architectural understanding
layer is the primary value frontier.

1. **Module discovery expansion (Layer 2 + 3).** Why now: Layer 1
   (manifest-declared) is shipped. Operational modules (entrypoints,
   deploy surfaces) and inferred modules (graph clustering) are the
   next step toward complete module truth. Agents need this for
   orientation in multi-surface repos (backend/frontend/admin/infra).

2. **Boundary/seam expansion.** Why now: HTTP and CLI boundaries are
   shipped. State boundaries (database, filesystem, cache) are the
   next high-value seam class. Agents need to know which modules
   read/write which resources.

3. **Runtime/build environment model.** Why now: without this, an agent
   knows what a module contains but not how it runs. Compile context,
   deployment target, and runtime configuration are critical for
   correct reasoning about changes.

4. **Delta indexing completion.** Why now: 77-minute full index on Linux
   is not operationally viable. Delta refresh slice 1 is shipped.
   Config-file tracking, scoped postpasses, and large-repo validation
   remain.

5. **Long-lived daemon with in-memory graph.** Why now: eliminates
   cold-start overhead, enables concurrent queries, provides the
   correct runtime model for fast agent orientation.

6. **Registry/framework liveness edges.** Why now: without these,
   dead-code detection on plugin-driven codebases is unsafe. Largest
   known soundness gap in the extraction layer.

## Current State And Next Moves

**Just shipped:**
- Streaming/batched indexing pipeline — Linux kernel indexes (63K files,
  1M+ nodes, 2M+ edges, 77 min). V8 heap OOM eliminated.
- Module discovery Layer 1 — declared modules from manifests/workspaces.
  9 detectors, evidence persistence, file ownership, CLI surface.
- Delta refresh slice 1 — content-hash-based invalidation, copy-forward
  for unchanged files, trust metadata in diagnostics. Verified on repo-graph.
- Python two-pass last-definition-wins extraction.
- Durable extraction edges (migration 012) for delta reuse.
- `runIndex` decomposed into reusable phase helpers.

**Known open items (not blocking current frontier):**
- Config-file tracking gap in delta invalidation
- Conservative postpasses in delta refresh
- Large-repo delta refresh not yet validated
- `supersedes_uid` lineage not exposed in `declare list --json` output

**Current product priority:**
Module/boundary/runtime substrate — the architectural understanding
layer that makes agents effective. See Horizon 1 above.

**What is NOT next:**
- Fleet / cross-repo features (Horizon 3)
- Snapshot history / trend optimization
- Token consumption benchmarking

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

## Process As Entropy Containment

Safety-critical process was historically a human straightjacket.
It existed to compensate for human weakness at exhaustive, repetitive,
pedantic detail work. Humans provide system judgment well, but do not
scale to maintaining full traceability matrices, verification
obligations, and change records by hand.

LLMs invert that profile:

- strong at expansion, clerical repetition, and detail grinding
- weak at systems judgment, boundary design, and long-horizon correctness
- prone to probabilistic drift when asked to generate unconstrained code

This changes the role of process.

Repo-graph should treat high-assurance process as a deterministic
containment vessel for stochastic agents. The value is not "generate
compliance paperwork." The value is forcing code generation through
machine-checkable choke points that reduce the probability space:

1. intent is captured as structured requirements, constraints, and
   non-goals
2. verification obligations are generated before implementation
3. tests and checks become executable boundary conditions
4. implementation is judged against locked requirements and verified
   obligations
5. evidence is linked back to the specific requirement, contract, and
   process version that demanded it

The system does not replace engineering judgment.
It externalizes and enforces the parts humans are bad at sustaining,
while keeping high-level architecture, trade-offs, and exception
decisions in human hands.

This is the real opportunity:

- apply avionics/medical-style rigor to ordinary software projects
- make strong process economically viable outside regulated industries
- reduce quality dependence on human fatigue and review stamina
- constrain AI generation with explicit structure instead of post-hoc
  code reading

Repo-graph should therefore be designed as an engineering control
system for stochastic agents, not merely as a repository index.

## Deterministic Discovery As Token Reduction

A secondary but measurable value of repo-graph is reducing the token
cost of engineering cognition for AI agents.

Without a deterministic graph, an agent answering "who calls this
function?" must read source files, follow imports, scan for references,
and reason about the results. That is expensive in input tokens,
output tokens, tool calls, and wall-clock time. The answer quality
depends on the agent's search strategy and context window.

With repo-graph, the same question is a single `rgr graph callers`
call that returns a deterministic, pre-indexed result. The agent
consumes fewer tokens, makes fewer tool calls, and gets a more
complete answer.

This is not a side metric. It is proof that the product reduces the
cost of controlled engineering cognition.

The token-reduction value compounds across task complexity:
- caller/callee discovery: one query vs multi-file search
- impact analysis: graph traversal vs heuristic file reading
- hotspot triage: pre-computed ranking vs ad-hoc churn analysis
- architecture violation check: one command vs manual import tracing
- requirement/evidence closure: structured query vs scattered doc reading

A clean separation of concerns enables this reduction:

- rgr owns identity and structure: what symbol, which module, what
  relationships, what measurements.
- The agent owns live source location: which line right now, exact
  current byte span, current file content.
- rgr provides snapshot-scoped coordinates (file path, line range,
  file content hash) as lightweight traceability aids.
- The agent resolves final live positions on demand from the working
  tree using standard tools (rg, awk, tree-sitter, LSP).

This means rgr does not need a live line database, continuously
updated coordinates, or branch-sensitive mutable location tracking.
The indexed snapshot stays internally consistent. Workspace divergence
(branch switch, revert, uncommitted edits) is a normal condition
surfaced as staleness, not a data integrity problem.

To validate this value, the product should eventually support:
1. Benchmark tasks with defined expected artifacts
2. Comparable protocols: pure agent discovery vs rgr-assisted discovery
3. Measurement: input tokens, output tokens, tool calls, wall-clock time,
   correctness, completeness, human verification burden
4. Results stored as first-class evidence artifacts linked to repo
   snapshots and toolchain versions

This positions repo-graph not just as "better answers" but as a
lower-cost, higher-fidelity control plane for agent-driven engineering.

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

### Horizon 1: Architectural Understanding (current frontier)

Make the codebase structurally legible to agents:

- multi-language extraction (shipped: TS, Rust, Java, Python, C/C++)
- module discovery from manifests/workspaces (shipped: Layer 1 declared)
- module discovery from operational surfaces (Layer 2: entrypoints, deploy targets)
- boundary/seam discovery (shipped: HTTP, CLI; next: state boundaries)
- runtime/build environment model per module
- incremental update from diffs (shipped: delta refresh slice 1)
- long-lived daemon with in-memory current-state graph
- fast agent orientation queries

### Horizon 2: Governance And Verification

Make repo-graph part of day-to-day change safety:

- architecture violation detection
- quality measurement (structure, complexity, coverage, churn)
- structured requirement and verification-obligation objects
- impact/effect analysis
- required test obligations
- pre-merge structural verification
- machine-checkable quality and release gates
- waiver/exception recording tied to policy versions
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
- requirement-to-evidence closure tracking
- agent execution records linked to gates, evidence, and approvals
- reusable process profiles for different assurance levels
- database/version compatibility reasoning
- cross-repo fleet operations and migration planning
- declared versions for governance objects where extraction is impossible

## Product Principle

Keep deterministic facts, declarations, and inferences separate.
Use AI for synthesis and mechanical expansion, but ground decisions in
explicit, inspectable evidence and human-owned architectural judgment.

If an answer cannot show versioned provenance, evidence chain, and policy context, it is advisory only, not authoritative.

## End State

Repo-graph becomes the engineering memory and verification substrate of an organization:

- portable across platforms and model vendors
- trusted for high-stakes engineering decisions
- useful in daily development, not only audits
- capable of bringing high-assurance discipline to mainstream software at low operational cost

That is the long-term direction: not more generated code, but higher-fidelity control over system reality.
