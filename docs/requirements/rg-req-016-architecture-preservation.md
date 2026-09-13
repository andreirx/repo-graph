<!-- requirements-assurance-v1
{
  "formatVersion": 1,
  "kind": "requirement",
  "requirementId": "RG-REQ-016",
  "sources": [
    { "kind": "document-section", "path": "agent_docs/architecture.md", "fragment": "mandatory-rules" },
    { "kind": "document-section", "path": "agent_docs/architecture.md", "fragment": "prohibited-patterns" },
    { "kind": "document-section", "path": "agent_docs/architecture.md", "fragment": "build-order" },
    { "kind": "document-section", "path": "agent_docs/architecture.md", "fragment": "persistence-completeness-checklist" },
    { "kind": "document-section", "path": "CLAUDE.md", "fragment": "structural-guardrails" },
    { "kind": "document-section", "path": "CLAUDE.md", "fragment": "decision-autonomy" },
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "agent-operating-model" }
  ],
  "lowLevelRequirements": [
    { "id": "RG-REQ-016-L01", "parentId": "RG-REQ-016" },
    { "id": "RG-REQ-016-L02", "parentId": "RG-REQ-016" },
    { "id": "RG-REQ-016-L03", "parentId": "RG-REQ-016" },
    { "id": "RG-REQ-016-L04", "parentId": "RG-REQ-016" },
    { "id": "RG-REQ-016-L05", "parentId": "RG-REQ-016" },
    { "id": "RG-REQ-016-L06", "parentId": "RG-REQ-016" },
    { "id": "RG-REQ-016-L07", "parentId": "RG-REQ-016" },
    { "id": "RG-REQ-016-L08", "parentId": "RG-REQ-016" },
    { "id": "RG-REQ-016-L09", "parentId": "RG-REQ-016" },
    { "id": "RG-REQ-016-L10", "parentId": "RG-REQ-016" }
  ]
}
-->
# RG-REQ-016 — The accepted architecture is preserved by every change unless a change to it is explicitly authorized

Status: DRAFT authored by the in-place manager 2026-09-13 at the human's direction ("preserving the accepted application architecture is a standing engineering obligation … THAT needs to be a requirement in itself"). Independent requirements review: NOT PERFORMED. Human approval: PENDING.
Maturity: PROTOTYPE (requirements record).

## Source and intent

Sources: the human's 2026-09-13 directive (quoted in the H statement); [Architecture rules — Mandatory Rules](../../agent_docs/architecture.md#mandatory-rules) (dependency rule inward; storage is adapter; daemon is coordination authority); [Prohibited Patterns](../../agent_docs/architecture.md#prohibited-patterns) (no domain logic in CLI/storage; do not append responsibilities to oversized files); [Build Order](../../agent_docs/architecture.md#build-order); [Persistence Completeness Checklist](../../agent_docs/architecture.md#persistence-completeness-checklist); [CLAUDE.md — Structural Guardrails](../../CLAUDE.md#structural-guardrails) (`main.rs` wiring only; no new responsibilities in files over 500 lines; refactor before expanding mixed-responsibility files); [CLAUDE.md — Decision Autonomy](../../CLAUDE.md#decision-autonomy) (architecture boundary, dependency edge, data shape crossing a boundary → STOP and ask); [VISION — Agent Operating Model](../VISION.md#agent-operating-model) (support module → storage → feature → tests → docs; do not collapse steps); agent-manager `SYSTEM.txt` §5 (earned abstraction; dependencies point inward; no new component cycles; no dependencies without authorization). The 2026-09-13 structure reading (`docs/audits/2026-09-13-structure-reading.md`) is the evidence baseline.

## High-level requirement

Every change to repo-graph shall work within the accepted architecture — its crate responsibilities, dependency directions, layer rules, boundary contracts and state ownership — unless a change to that architecture has been explicitly authorized. A task that appears to require an architectural change shall stop its dependent implementation and present the problem, the proposed change, its system-wide impact, its risks and the smallest alternative within the current architecture, distinguishing genuine necessity from design preference. Local implementation details that preserve those contracts need no approval. Slices carry the constraints relevant to the code they touch; reviewers inspect for unauthorized structural change. Mechanical checks establish record integrity; human judgment, routed by the manager, establishes architectural correctness.

**Scope:** the Rust workspace (`rust/crates`), the daemon/CLI/storage boundaries, the state-root classes, and the process by which slices and reviews handle structure. Not a target layout or layer count — the accepted architecture is whatever the tracked architecture documents and the current crate graph say it is, until changed by an authorized decision.

**High-level acceptance:** the mechanical checks in L01, L04, L08 run in the gate; every slice packet carries §0 constraints (L06); a reviewer record names the structural check (L07); the observed deviations in the evidence baseline are either authorized, fixed, or recorded as debt with an owner.

## Low-level requirements

### RG-REQ-016-L01 — Dependencies point inward; the crate graph is a DAG and core never imports adapters or the CLI

Core crates (domain policy: `agent`, `classification`, `trust`, `trust-model`, `coherence`, `gate`, `quality-policy`, `indexer`'s resolver policy, `repo-graph-seed` ranking, `doc-facts` rules, `module-queries` composition rules) shall not depend on adapter or delivery crates (`storage`, `daemon-runtime`, `daemon-transport`, `rgr`, `rmapd`); adapters depend inward on the ports core defines; `rgr`/`rmapd` are the composition roots and may depend on everything. The workspace crate graph shall have no cycles. A new crate, a new inter-crate dependency edge, or a type crossing a crate boundary is an architectural change (L05).

**Verification criterion:** a check over `cargo metadata --no-deps` asserting (a) acyclic, (b) no edge from a listed core crate to a listed adapter/delivery crate — run in the gate; the list of core/adapter crates is recorded in `agent_docs/architecture.md` (to be added — today the rule is prose only); `rmap cycles` on repo-graph shows no runtime module cycle in `rust/crates`.

**Evidence (v0.18.0):** PARTIALLY MET — the graph is acyclic (54 crates; `rmap cycles` finds only a type-only cycle in `tools/rgistr`); one inward-rule violation observed: `quality-policy` (policy) imports `repo_graph_storage::types::{QualityPolicyKind, QualityPolicyPayload, ScopeClause…}` — a core crate depending on the storage adapter for its domain types (`quality-policy/src/lib.rs:79`, `assess.rs:34`). `module-queries` → `storage` is a query/read-model crate over the adapter (acceptable as application logic; stated, not a violation). No mechanical check exists.

### RG-REQ-016-L02 — Domain logic lives in core; storage adapters and CLI handlers carry no business rules

A rule that defines a valid domain state or outcome (what counts as resolved, suspicious, used, generated, a cycle, a verdict) shall live in a core crate with explicit inputs and results, not inside a SQL statement, a storage `*_impl.rs`, a `dispatch.rs` handler or a presentation renderer; adapters translate and persist, handlers route and compose, renderers render.

**Verification criterion:** review against this L for every packet touching `storage/src/*_impl.rs`, `storage/src/queries.rs`, `daemon-runtime/src/dispatch.rs`, `rgr/src/commands/*`; the structure reading's named instances (RC-5's suspicion math in `trust_impl.rs` SQL; the `dependencies` vendored predicate living in a daemon-runtime handler module) are the current debt list.

**Evidence (v0.18.0):** PARTIALLY MET — two named instances where a domain predicate sits in an adapter (`compute_module_stats` SQL decides connectivity; `is_vendored_path` lives in `daemon-runtime/src/handlers/quality/support.rs`, which the core complexity aggregator cannot call — RC-9); Q2 and Q7 move them inward as part of their fixes.

### RG-REQ-016-L03 — The daemon is the coordination authority; clients do not bypass it for storage

`rmap` (the CLI) shall reach repository state through the daemon (socket, or the daemon as a stdio subprocess); direct `StorageConnection` use from the CLI is limited to the documented offline paths (`hook --db <path>` for host integrations without a daemon) and is listed as such.

**Verification criterion:** an enumeration test of `repo_graph_storage::StorageConnection` uses in `rgr/src` against an allow-list with a stated reason per entry (to be added — none exists); today's uses: `cli/context.rs`, `cli/snapshot_hint.rs`, `cli/envelope.rs`, `commands/hook/session_start.rs`.

**Evidence (v0.18.0):** UNKNOWN — four direct-use sites exist in the CLI; whether each is a documented offline path or a bypass has not been classified.

### RG-REQ-016-L04 — Oversized files do not grow new responsibilities; a leaf module's size has a recorded reason

No slice shall append a new responsibility to a source file over 500 lines (net of inline tests); a change that must touch such a file either stays within its existing responsibility or first extracts the responsibility it needs (a local refactor, no approval needed when no boundary moves). A leaf module (one responsibility, few dependents) whose net size exceeds 1,000 lines shall have its reason recorded in the structure reading (e.g. a language's tree-sitter walk: one responsibility, grammar coverage) — absence of a reason is a finding. Routers and composers (`dispatch.rs`, `compose.rs`, `orchestrator.rs`) are expected to be large but shall not hold handler bodies that are themselves domain or application logic (L02).

**Verification criterion:** a gate check listing every file over 500 net lines whose line count GREW in the candidate diff, for the reviewer to match against the slice's stated responsibility (to be added — the guardrail is prose only today); the structure reading's leaf table with reasons.

**Evidence (v0.18.0):** NOT MET — 222 of 669 source files exceed 500 lines, 81 exceed 1,000, 20 exceed 2,000; `dispatch.rs` is 10,113 net lines and the 90-day hotspot by an order of magnitude (churn 7,744 lines × complexity 1,162); the guardrail has never been checked mechanically.

### RG-REQ-016-L05 — An architectural change stops the dependent implementation and is presented, not made

When a task appears to need a new crate or module boundary, a new dependency edge, a changed data shape across a boundary, a moved responsibility between layers, a new state-root class or store, or a contract change on a documented boundary, the builder shall STOP the dependent implementation and present: the problem; the proposed change; its system-wide impact (every consumer); its risks; the smallest alternative within the current architecture; and whether the need is genuine necessity or design preference. Local implementation details that preserve the contracts proceed without approval.

**Verification criterion:** the slice packet's §4 Stop conditions carry this rule verbatim; a DECISION_REQUIRED record exists for every structural change shipped; review rejects a candidate whose diff contains an unauthorized structural change (L07).

**Evidence (v0.18.0):** OBSERVED practised in the relay (DECISION_REQUIRED is the existing mechanism; CPP-DECLARATORS-1, SYMBOL-IDENTITY-1 ruling B, DAEMON-RESIDUALS-2C are records); not yet stated as an obligation in every packet.

### RG-REQ-016-L06 — Slices carry the architectural constraints relevant to the code they touch

Every slice packet shall state: the crates and files it may touch; the boundaries it crosses (ports used, data shapes carried); the state it owns or reads; the constraints from this H that apply (dependency direction, oversized-file rule, daemon authority, state-root classes); and the preservation obligations from the catalog it must hold (the Regression watch).

**Verification criterion:** the nine queue packets' §0/§3/§4 sections; a packet missing the constraints section is not admitted (manager check today; a mechanical check when the assurance runtime's slice contract lands).

**Evidence (v0.18.0):** OBSERVED MET for the nine queue packets (each names files, frozen items and the watch); older packets vary.

### RG-REQ-016-L07 — Reviewers inspect for unauthorized structural change

Every implementation review shall record, as a named check, whether the candidate diff introduces a new crate, a new inter-crate dependency, a type crossing a boundary it did not cross before, a responsibility moved between layers, a new store or state-root class, or growth of an oversized file outside the slice's stated responsibility — and shall reject a candidate that does so without a DECISION_REQUIRED record.

**Verification criterion:** the reviewer role prompt (`agent-manager/prompts/roles/reviewer-target.md`) names the check; each `review-<n>.json` carries its result; the L01/L04 gate outputs are inputs to the review.

**Evidence (v0.18.0):** PARTIALLY MET — reviewers check scope and earned abstraction by prompt; the structural checklist is not yet explicit in the role prompt or the review record.

### RG-REQ-016-L08 — No dormant or orphan structure: a crate nobody depends on is wired, retired, or recorded

Every workspace crate shall have at least one dependent or be a binary/tool; a crate with no dependents is either wired into a delivered surface, retired, or recorded in `docs/TECH-DEBT.md` with its disposition and owner. The same holds for a port method with no caller and a feature flag with no reader.

**Verification criterion:** the `cargo metadata` check from L01 also lists crates with fan-in 0 that are not binaries; `docs/TECH-DEBT.md` carries an entry for each.

**Evidence (v0.18.0):** NOT MET — `repo-graph-detectors` (3,338 source + 4,176 test lines; "the sole implementation" of the seam-detector substrate since TS-PROTOTYPE-RETIREMENT-1) has no dependent in the workspace: dormant capability with no recorded disposition.

### RG-REQ-016-L09 — Abstractions are earned and recorded

A new module, interface, adapter, config layer, DTO family or port method shall be introduced only with two or more concrete current callers, a ratified near-term variation, a documented boundary, a necessary inversion, or a test seam unobtainable more simply — and shall be recorded in one line (what, current users, the growth axis, the simpler alternative rejected). Ports stay narrow: a port method with one caller is a candidate for removal, not precedent.

**Verification criterion:** the one-line record in the build report for every abstraction introduced; review rejects an unrecorded one; the structure reading tracks port width (`agent/src/storage_port.rs`: 43 methods; `indexer/src/storage_port.rs`: 49).

**Evidence (v0.18.0):** OBSERVED practised in packets; port width unreviewed — the two storage ports are fat interfaces whose growth has never been challenged.

### RG-REQ-016-L10 — Persisted features are complete across the architecture before they are claimed

A change introducing or modifying a persisted artifact is incomplete until schema, write path, read/query path, refresh/copy-forward/invalidation behaviour, trust/maturity impact, CLI visibility and validation on both fresh index and refresh exist — the support module is built first, then storage, then the feature, then tests, then docs; steps are not collapsed.

**Verification criterion:** the persistence checklist in the slice's DoD; refresh-path tests for every new artifact family (`artifact-contracts/tests/completeness.rs` tripwire for new families).

**Evidence (v0.18.0):** OBSERVED MET as practice (the artifact-contract registry tripwire); the refresh half is where past defects hid (copy-forward — the single most complex function in the product, `copy_forward_unchanged_files` cx 93).

## Preservation obligations named by the ratifying specifications

- `agent_docs/architecture.md` Mandatory Rules 1–8 and Prohibited Patterns; CLAUDE.md Structural Guardrails; the Decision Autonomy boundary list.
- The state-root classes A1/A2/B (STATE-ROOT-SEPARATION-1) and the daemon-as-authority rule (RG-REQ-011).
- Documented boundaries: the `AgentStorageRead` / indexer storage ports; the daemon transport envelope; the artifact-contract registry.
- SYSTEM.txt §5 (agent-manager) — earned abstraction, inward dependencies, no component cycles, no unauthorized dependencies — is the general form of this H and is not restated here.

## Review and approval

Independent requirements review: NOT PERFORMED.
Human approval: PENDING.
Implementation and verification: evidence lines are the manager's OBSERVED reading from the 2026-09-13 structure reading; they are not acceptance.
