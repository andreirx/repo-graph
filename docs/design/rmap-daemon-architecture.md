# rmap Daemon Architecture

Status: IMPLEMENTATION IN PROGRESS (D5b complete)
Created: 2026-04-30
Updated: 2026-05-07
Maturity target: PROTOTYPE -> MATURE

## Implementation Status

### Completed Slices

**D1: Core Policy Module** — DONE
- `rust/crates/daemon-policy/src/state.rs` — Pure state machine
- `rust/crates/daemon-policy/src/coordinator.rs` — RepoCoordinator with FIFO writer queue
- 45 unit tests passing

**D2: Stdio Adapter** — DONE
- `rust/crates/daemon-transport/` — NDJSON transport over stdin/stdout
- Dispatcher trait for pluggable routing
- 23 unit tests passing

**D3: Application Service Bridge** — DONE
- `rust/crates/rgr/src/daemon/` — ServiceDispatcher wiring real services
- Methods: ping, echo, load_repo, unload_repo, list_repos, callers, callees, imports

**D4: Write Operations** — DONE
- `index` method with DB-level write coordination
- `refresh` method with DB + repo coordination
- Proper composite identity (db_path + repo_uid) at API boundary
- 11 state unit tests + 19 integration tests passing

**D5a: Agent Services** — DONE
- `orient` method with optional focus and budget params
- `check` method for pre-action trust/safety check
- `explain` method with target and optional budget
- All return `OrientResult` DTO (rgr.agent.v1 schema)

**D5b: Progress Streaming** — DONE
- `ProgressEmitter` trait in daemon-transport for request-scoped progress emission
- Transport creates emitter bound to request ID and output writer
- `index` and `refresh` emit progress events (scanning, extracting, persisting phases)
- Progress events are separate NDJSON lines before final response
- Event ordering contract: progress events first, final response last
- **Abort checkpoint seam:** emit is fallible, transport failure aborts operation
  - `ProgressCallback` returns `ControlFlow<()>` — Break aborts at checkpoint
  - `EmitError` signals transport channel failure
  - `ComposeError::Aborted` returned when progress emission fails
  - `ProgressDeliveryFailed` error code surfaced to client
- 35 total tests (33 daemon-transport + 2 integration abort tests)

### Key Implementation Details

**Identity Model:**
All repo-scoped methods require composite identity:
- `db_path`: canonical path to the database file
- `repo_uid`: repo identifier within that database

This prevents the identity collision bug where two databases with the same repo_uid would alias.

**Coordination Model:**
Two coordination levels:
1. **Database-scoped** (`DatabaseState`): Single-writer for any DB mutation
2. **Repo-scoped** (`RepoCoordinator`): Reader/writer semantics for loaded repos

**API Contract:**
```json
// Load a repo
{"id":"1","method":"load_repo","params":{"db_path":"/path/to/db","repo_uid":"myrepo"}}

// Query callers (requires db_path + repo_uid)
{"id":"2","method":"callers","params":{"db_path":"/path/to/db","repo_uid":"myrepo","symbol":"foo"}}

// Index (acquires DB write lock)
{"id":"3","method":"index","params":{"repo_path":"/path/to/source","db_path":"/path/to/db"}}

// Refresh (acquires DB lock then repo lock)
{"id":"4","method":"refresh","params":{"db_path":"/path/to/db","repo_uid":"myrepo"}}

// Orient (optional focus and budget)
{"id":"5","method":"orient","params":{"db_path":"/path/to/db","repo_uid":"myrepo"}}
{"id":"6","method":"orient","params":{"db_path":"/path/to/db","repo_uid":"myrepo","focus":"src/core","budget":"large"}}

// Check (pre-action safety check)
{"id":"7","method":"check","params":{"db_path":"/path/to/db","repo_uid":"myrepo"}}

// Explain (deep dive on target)
{"id":"8","method":"explain","params":{"db_path":"/path/to/db","repo_uid":"myrepo","target":"src/main.ts"}}
```

### Remaining Work (D5c/D6)

- Cancellation support (D5c) — can reuse D5b abort checkpoint seam
- Concurrent write coordination tests
- End-to-end validation on real repos (D6)

### Abort Checkpoint Placement (D5b)

Current checkpoints in `repo-index::compose`:
- `scanning` start (0/1)
- `scanning` end (1/1)
- `extracting` start (0/1)
- `extracting` end (1/1)
- `persisting` steps (0-5 of 5)

Abort granularity: operation terminates at the next checkpoint after transport failure.
Partial state risk: if abort occurs during `persisting` phase, some persist steps may
have committed. The orchestrator uses a single transaction for the core index, but
post-processing steps (metrics, liveness, policy facts) are separate commits.

D5c cancellation will use the same checkpoint seam with an explicit cancel flag.

## 1. Problem Statement

`rmap` is moving toward a long-lived daemon because repeated one-shot CLI execution is the wrong runtime shape for the product vision.

Current one-shot execution repeats volatile outer-layer work on every command:
- process startup
- CLI argument parsing
- SQLite open + migration checks
- extractor and parser initialization
- repeated loading of facts that are stable across adjacent queries
- repeated recomputation of query context that a long-lived process could retain

This is operationally expensive, but the deeper issue is architectural.
A daemon must NOT be implemented as "CLI logic behind a socket". If that happens,
command handlers become the real application layer and every new transport re-embeds
CLI-specific behavior.

There is also a concurrency requirement that is not optional:
- multiple AI agents must be able to query and refresh the same repo state
- reads will be much more frequent than writes
- agents must not stomp over each other at the SQLite boundary
- the daemon must become the synchronization authority for shared repo-graph databases

If clients continue opening the same `.db` file independently for mixed read/write
traffic, the daemon has failed its primary operational purpose.

The recent `main.rs` refactor materially improved this situation. Command-family
modules now expose where the real seams are and where CLI-local orchestration still
needs to be extracted.

## 2. Architectural Position

The daemon is an **outer adapter**, not the product center.

The product center remains:
- support modules with deterministic domain logic
- transport-neutral application services
- explicit request/response DTOs
- typed errors

The daemon is only a delivery mechanism for these services.

This aligns with existing project rules:
- dependency rule: inward only
- support module first
- storage is adapter
- deterministic output
- explicit degradation (`null` = unknown, empty = known-zero)

It also aligns with `docs/VISION.md`:
- primary truth: current repo state in memory
- secondary truth: persistent disk cache
- git owns history
- repo-graph owns current-state structured truth

It must also align with the product's real core business logic:
- model relationships in legacy code that determine how change can be made safely
- keep those relationship models language-neutral
- let extractors for C, C++, Rust, Python, Java, TypeScript, and later Go/Scala/Kotlin
  feed the same relationship substrate instead of creating language-specific product silos

## 3. Non-Goals

This daemon design explicitly does **not** do the following:

- wrap existing CLI handlers and call that architecture complete
- make HTTP the product center
- turn retained snapshots into a historical warehouse
- move domain logic into transport routing or socket handlers
- conflate daemon-local caches with canonical truth
- invent daemon-only semantics for `orient`, `check`, `explain`, `modules`, `policy`, or other surfaces
- permit daemon-backed clients to keep mutating the same SQLite file out-of-band

## 4. Core Business Logic Center

The daemon should revolve around the stable relationship model, not around command names
and not around extractor implementation details.

The most valuable long-lived substrate is the language-neutral model of legacy-code
change relationships, especially those motivated by *Working Effectively with Legacy Code*:
- boundaries
- seams
- enabling points
- sensing surfaces
- separation barriers
- effect paths
- return-fate and status-mapping policy flow
- state/resource touchpoints
- module dependency pressure
- testability constraints

Extractors are evidence adapters for these relationships.
The daemon's job is to serve this relationship substrate safely to many agents at once.

## 5. What the `main.rs` Refactor Already Bought

The refactor changed the composition shape of the outermost Rust layer.

Before:
- `rust/crates/rgr/src/main.rs` mixed dispatch, parsing, orchestration, and output shaping
- command behavior was hard to separate from process entry behavior
- a daemon path would likely have duplicated command behavior behind another outer surface

Now:
- command families live under `rust/crates/rgr/src/commands/`
- shared CLI concerns live under `rust/crates/rgr/src/cli/`
- some reusable orchestration is already moving into support crates such as:
  - `rust/crates/agent/`
  - `rust/crates/module-queries/`
  - `rust/crates/policy-facts/`
  - `rust/crates/trust/`
  - `rust/crates/gate/`
  - `rust/crates/repo-index/`

This is not daemonization, but it is enabling work. It exposed which command families
are already thin adapters and which ones still hide application orchestration.

## 6. Current-State Read of the Rust Architecture

### Already daemon-friendly

These areas already resemble transport-neutral use cases or support modules:
- `rust/crates/agent/` for `orient`, `check`, `explain`
- `rust/crates/gate/` for gate assembly
- `rust/crates/trust/` for trust evaluation
- `rust/crates/module-queries/` for preloaded module fact orchestration
- `rust/crates/policy-facts/` for deterministic policy extraction
- `rust/crates/repo-index/` for indexing/refresh orchestration

### Still too CLI-shaped

Several command families still mix application orchestration with CLI-local concerns:
- graph query surfaces
- module discovery surfaces
- resource/state-boundary surfaces
- governance write flows under `declare`
- some output DTO assembly performed directly in command modules

### Main gap

The missing layer is an explicit **application service layer** between:
- support modules / storage ports
- transport adapters (CLI now, daemon next)

Without that layer, a daemon would still pull logic from command modules and would
remain CLI-shaped internally.

## 7. Required Layering

The daemon-ready architecture needs four layers.

### 7.1 Support modules

Purpose:
- deterministic logic
- no transport knowledge
- no socket/session concerns
- no CLI rendering concerns

Examples:
- `agent`
- `gate`
- `trust`
- `module-queries`
- `policy-facts`
- `repo-index`
- extractor crates
- classifier crates

### 7.2 Application services

Purpose:
- orchestrate one use case
- depend only on support modules and ports
- define request/response DTOs
- define typed errors
- remain callable from CLI, daemon, tests, and future batch workers

Representative service families:
- `IndexRepo`
- `RefreshRepo`
- `QueryCallers`
- `QueryCallees`
- `QueryImports`
- `FindPath`
- `ListModules`
- `ShowModule`
- `ListModuleFiles`
- `EvaluateModuleViolations`
- `ListSurfaces`
- `ShowSurface`
- `RunPolicyFactsQuery`
- `RunOrient`
- `RunCheck`
- `RunExplain`
- `DeclareBoundary`
- `DeclareRequirement`
- `DeclareWaiver`
- `DeactivateDeclaration`

### 7.3 Transport adapters

Initial outer adapters:
- CLI adapter (`rmap` one-shot commands)
- daemon stdio adapter (`rmap daemon`)

Possible later adapters:
- background worker / batch adapter
- test harness adapter

### 7.4 Runtime/session management

Purpose:
- manage long-lived repo state
- own caches, locks, cancellation, refresh swaps, and lifecycle
- stay outside support logic

This runtime layer belongs to the daemon adapter boundary, not to core support modules.

## 8. Authored Knowledge Model: Documents First

This system already treats documentation as a first-class surface. That needs to become
stronger, not weaker.

High-level rule:
- if a human or agent discovers architectural knowledge outside automatic extraction,
  the canonical authored form should be a documentation item
- the daemon/database may index, inventory, anchor, and project that documentation
- the daemon/database should NOT become the only opaque store for human-authored
  architectural knowledge

Implications:
- rationale, migration notes, seam notes, ownership notes, replacement plans, and
  hand-discovered relationship explanations should prefer document paths plus anchors
  over opaque JSON blobs with no first-class reading surface
- derived tables remain useful for query acceleration and filtering, but they are
  projections of authored knowledge, not the authored knowledge itself
- governance/policy objects may still need structured storage for deterministic
  enforcement, but discovery-oriented authored knowledge should bias toward documents

This is especially important for multi-agent use. A document is inspectable,
reviewable, versioned by git, and understandable outside repo-graph. An opaque
DB row is not.

## 9. Proposed Daemon Shape

```mermaid
flowchart TD
    Agent["AI agent / test harness"] --> STDIO["stdin/stdout NDJSON transport"]
    STDIO --> Router["daemon request router"]
    Router --> Coordinators["per-repo coordinators"]
    Router --> Services["application services"]
    Coordinators --> Services
    Services --> Support["support crates / use cases"]
    Support --> Ports["storage and repo ports"]
    Coordinators --> DB["repo DB handle (owned by coordinator)"]
    DB --> SQLite["SQLite adapter (transitional)"]
```

Interpretation:
- `rmap daemon` is a mode of `rmap`, not a separate binary
- agents connect via stdin/stdout with NDJSON framing
- the daemon router does not contain domain logic
- each repo has its own coordinator thread with owned DB handle
- application services call support modules
- the coordinator enforces single-writer / multiple-reader semantics
- SQLite remains an adapter during the transition period

## 10. Daemon Runtime Components

### 10.1 Request router

Responsibilities:
- decode NDJSON requests from stdin
- validate envelope shape (id, method, params)
- route to the correct application service via repo coordinator
- attach transport-scoped values such as wall clock time, cancellation handle, and progress sink
- serialize typed success/error DTOs to stdout

Must not:
- execute business rules
- assemble graph facts directly
- read SQLite directly except through service-owned ports/adapters

### 10.2 Repo session manager

Responsibilities:
- maintain per-repo runtime state
- load or warm state on demand
- coordinate read/query access versus refresh/index work
- pin a consistent snapshot view for each request
- swap refreshed state atomically
- evict idle sessions when policy requires it

Proposed per-repo record:
- repo identity and root path
- DB path / cache path
- current READY snapshot UID
- session state
- warmed query context handles
- in-memory projections/caches
- lock state
- DB access policy and handle set
- active request count
- last access time

### 10.3 DB access coordinator

This component is mandatory for the multi-agent goal.

Responsibilities:
- own the database handles used by daemon-backed clients
- present readers-writer semantics above SQLite
- serialize write-intent operations per repo
- keep read-heavy traffic from blocking each other unnecessarily
- prevent out-of-process agent commands from competing as ad hoc writers

Recommended initial shape:
- one writer connection per repo session
- one or more read-only/read-mostly connections for query traffic
- daemon-managed write queue per repo
- explicit publish points for refresh/index operations

Product rule:
- in daemon-backed mode, agents talk to the daemon
- they do not open the SQLite file directly for normal operations

SQLite already gives file-level locking and WAL behavior, but that is not the product
concurrency model. The product model is daemon-mediated coordination with explicit
readers-writer rules.

### 10.4 Progress broker

Responsibilities:
- receive structured progress events from long-running operations
- stream them to the requesting client
- preserve deterministic event ordering per request

Initial scope:
- index
- refresh
- docs extract
- other long-running extraction or recomputation tasks

### 10.5 Cancellation registry

Responsibilities:
- map request IDs to cancellation tokens
- allow client disconnect or explicit cancel to stop long-running work
- ensure cancelled work does not partially publish new session state

### 10.6 Maintenance lane

Responsibilities:
- session eviction
- stale cache cleanup
- warm-start metadata cleanup
- future `rmap clean` support

## 11. Repo Session State Model

A daemon needs explicit lifecycle states. Without them, refresh/query behavior becomes implicit and fragile.

Proposed states:
- `UNLOADED` — known repo, no active in-memory session
- `LOADING` — warming session from storage/disk cache
- `READY` — current state queryable
- `REFRESHING` — recomputing next state while current READY state remains queryable
- `FAILED` — last load/refresh failed; prior READY state may or may not exist
- `EVICTED` — state intentionally discarded

State rules:
- read queries never observe a half-built refresh
- refresh builds a candidate state off to the side
- publish is atomic: candidate replaces current READY only after successful completion
- failed refresh does not poison the previously published READY state
- cancellation during refresh discards the candidate state

## 12. Concurrency Model

The roadmap already points to three daemon lanes:
- query
- index
- maintenance

That direction is correct.

But it needs one more explicit statement:

**The daemon is the multi-agent arbitration layer for a repo database.**

If ten agents issue reads and one agent issues a refresh, the daemon decides how
that interaction proceeds. The agents do not coordinate by racing raw SQLite access.

### Query lane

Characteristics:
- many concurrent requests allowed
- operates on pinned READY state
- no mutation of published repo state
- should use daemon-owned read handles, not the writer handle when avoidable

### Index lane

Characteristics:
- at most one write/refresh operation per repo
- builds candidate state without invalidating active readers
- publishes by atomic swap
- owns all repo-mutating DB traffic in daemon-backed mode

### Maintenance lane

Characteristics:
- low priority
- never blocks correctness-critical reads unless explicitly required

### Locking rules

Per repo:
- one writer max
- multiple readers
- readers pin a snapshot/session generation
- published generation changes only on successful swap
- write requests queue behind the repo writer
- document/index refresh publish only at explicit swap boundaries

Cross repo:
- no global repo write lock unless a shared global resource truly requires it
- unrelated repos must not serialize each other

Practical consequence:
- many AI agents can read concurrently
- one AI agent can refresh or author a policy/document projection at a time per repo
- no client-side stomping is possible as long as clients use the daemon surface

## 13. Transport Contract

### Initial transport

The initial transport is **NDJSON over stdin/stdout**.

This is appropriate for the first daemon slice because it gives:
- universal IPC for AI agent tooling (no socket path negotiation)
- explicit request/response envelopes with message framing
- easy multiplexing of progress and cancellation on the same channel
- clear separation from CLI rendering
- easy to test, easy to pipe, easy to wrap
- no platform-specific IPC concerns (works identically on Unix/Windows)

### Deferred transports

Deferred, not rejected:
- Unix socket (for multi-client scenarios)
- Windows named pipe equivalent
- HTTP bridge for remote/fleet scenarios

These should remain adapters over the same application services. The application service layer is transport-neutral.

## 14. Service Contract Rules

Every daemon-facing application service should expose:
- input DTO
- output DTO
- typed error enum
- deterministic ordering rules
- explicit degradation rules

### Example rules

- input parsing belongs at adapter boundary or in request DTO validation, not deep inside support logic
- services return structured empty results when the answer is known-zero
- services return `null` fields only when the value is unknown or unavailable
- services do not print
- services do not exit the process
- services do not depend on transport-specific types (NDJSON envelope, etc.)

## 15. Error Taxonomy

The daemon cannot reuse CLI exit codes as its primary error model.
It needs typed machine-readable errors.

Minimum daemon error classes:
- `InvalidRequest`
- `UnknownMethod`
- `RepoNotFound`
- `SnapshotNotFound`
- `StateUnavailable`
- `RefreshInProgress` or `WriteConflict`
- `Cancelled`
- `Timeout`
- `UnsupportedFeature`
- `StorageFailure`
- `InternalInvariantViolation`
- `OutOfBandDbAccessDetected` or equivalent policy error if daemon-managed exclusivity is violated

CLI mapping remains an adapter concern:
- daemon error DTO -> CLI stderr + exit code
- daemon error DTO -> test assertion surface

## 16. Current-State Truth and Cache Strategy

This is the most important daemon rule.

### Primary truth

The daemon's current in-memory repo state is the primary operational truth.

That state should eventually hold or project:
- file inventory
- symbol graph
- resolved and unresolved edges
- module catalog
- state-boundary facts
- boundary/provider/consumer facts
- runtime/build surfaces
- documentation inventory
- quality/trust summaries needed for hot queries

### Secondary truth

Persistent disk state exists for:
- warm start
- crash recovery
- incremental refresh support
- derived-fact reuse where contract-safe

### Transitional reality

Today SQLite remains necessary and valuable.

Near-term daemonization should therefore use a **transitional model**:
- SQLite remains the persistent adapter and query substrate
- the daemon keeps SQLite connections warm
- the daemon adds preloaded in-memory projections only where they materially reduce repeated work
- do not attempt a big-bang removal of SQLite from the daemon path
- the daemon owns read/write coordination for the DB in daemon-backed mode

This keeps the architecture honest:
- conceptual center moves toward in-memory current state
- implementation migrates in slices
- correctness stays ahead of latency work

## 17. What Should Be Cached First

Cache only what has clear reuse value and stable invalidation rules.

Good first caches:
- repo/session metadata
- prepared storage handles/statements
- latest READY snapshot identity
- module query context already modeled in `module-queries`
- documentation inventory for the current session
- trust/quality summary projections reused across adjacent discovery commands

Do not cache first:
- ad hoc command-local formatted JSON
- historical result blobs as product history
- semantically ambiguous partial aggregates with unclear invalidation

## 18. CLI Relationship to the Daemon

The CLI should become a thin client, not disappear.

CLI responsibilities after daemonization:
- parse user command line
- build request DTO
- connect/start daemon if needed
- send request
- render returned DTO to JSON/stdout
- render progress to stderr
- map typed errors to exit codes
- optionally fall back to direct in-process execution while migration is incomplete

Important consequence:
- the daemon should not know about clap-style help text or command usage text
- the CLI should not own domain orchestration once a service is extracted
- the CLI should not bypass daemon coordination for ordinary shared-db work once
  daemon-backed mode is active

## 19. Migration Path

This should be built incrementally.

### Phase 0 — current enabling state

Already underway:
- `main.rs` decomposition
- command-family extraction
- support crate extraction
- transport-neutral `agent` use-case crate
- reusable support crates for gate, trust, policy facts, module queries, repo index

### Phase 1 — extract application services

Target:
- move command orchestration into transport-neutral services
- define request/response DTOs and typed errors per service family

Priority order:
1. `orient` / `check` / `explain` path continues through `agent`
2. graph query services
3. module services
4. policy-facts service
5. governance write services

### Phase 2 — daemon runtime skeleton

Target:
- stdin/stdout NDJSON transport
- request router
- per-repo coordinator threads
- DB access coordination (single writer, multiple readers)
- cancellation registry
- progress streaming
- per-repo locks

At this phase, the daemon may still use SQLite heavily.
That is acceptable.

### Phase 3 — daemon-backed CLI for thin surfaces

Target:
- wire selected stable services through daemon first
- keep direct execution fallback while coverage is incomplete
- establish daemon as the preferred shared-db access path for multi-agent operation

Good first daemon-backed surfaces:
- `orient`
- `check`
- `explain`
- `callers`
- `callees`
- `imports`
- `modules list`
- `modules show`

### Phase 4 — warm-state acceleration for heavy surfaces

Target:
- add preloaded projections for repeated expensive queries
- atomic refresh swap
- snapshot pinning for concurrent readers

Likely beneficiaries:
- module violations
- policy-facts queries
- trust/quality aggregation surfaces

### Phase 5 — delta-refresh-first daemon behavior

Target:
- integrate invalidation planner
- minimize recomputation scope
- maintain explicit provenance for reused vs recomputed facts

## 20. Test Strategy

Daemon work must preserve off-target testability.

### Service-layer tests

Required:
- headless tests against transport-neutral application services
- fake or test storage ports where practical
- deterministic DTO assertions

### Transport tests

Required:
- NDJSON envelope parsing and validation
- error serialization tests
- cancellation tests
- progress streaming tests
- stdin/stdout round-trip tests
- graceful shutdown on EOF

### Runtime/session tests

Required:
- concurrent read during refresh
- refresh failure keeps prior READY state
- cancellation discards candidate state
- per-repo write lock enforcement
- session eviction safety
- multi-agent read storm against one repo while a queued write waits
- queued writes execute in order without interleaving state publication
- daemon-backed clients do not stomp over each other at the DB boundary

### Smoke tests

Required:
- daemon-backed `rmap orient/check/explain` on validation repos from the canonical test protocol
- compare daemon DTOs against direct in-process DTOs for parity

## 21. Resolved Design Decisions

These decisions have been confirmed and should guide implementation.

### 22.1 Storage topology: one private DB per repo

**Decision:** Option B — one SQLite database per repo under a daemon-owned private state root.

**Rationale:**
- Aligns with daemon's per-repo session model (10.2, 10.3)
- Unrelated repos do not serialize each other at SQLite level
- Per-repo writer/read handle ownership is straightforward
- Cleanup/eviction is simple (delete directory)
- Smoke/test mode isolation is much cleaner
- Migration failure is isolated to one repo
- Enables future per-repo storage replacement

**What this means:**
- Daemon maintains a separate registry for repo metadata and session control
- Each repo gets its own SQLite file under the state root
- Cross-repo queries require orchestration above storage (acceptable trade-off)

### 22.2 DBs become private in daemon-backed mode

**Decision:** Normal CLI stops taking raw DB paths once daemon-backed mode is active.

**Rationale:**
- Prevents two competing control planes
- Daemon owns: where DBs live, when created, whether retained, repo load state
- User-facing CLI moves toward repo identity, not DB path

**CLI contract change:**
- Before: `rmap <command> <db_path> <repo_uid> ...`
- After (daemon mode): `rmap <command> <repo_uid> ...` or `rmap <command> --repo <path>`

The daemon resolves repo identity to its private DB location.

### 22.3 Test mode via state root injection

**Decision:** Test mode uses environment variable to inject alternative state root.

**Shape:**
```
RMAP_STATE_ROOT=/private/tmp/repo-graph-daemon-tests/<test-run>/
```

**Normal mode state root:**
```
~/.rmapd/
├── repos/<repo_uid>/repo.db
├── registry.db          # repo metadata, session state
├── rmapd.pid
├── logs/
└── cache/
```

**Test mode state root:**
```
/private/tmp/repo-graph-daemon-tests/<test-run>/
├── repos/<repo_uid>/repo.db
├── registry.db
└── ...
```

Note: No socket file needed. The daemon communicates via stdin/stdout.

Implementation is dependency injection of the state-root resolver. No forked logic.

### 22.4 Daemon v1 surface priority

**Decision:** Thin surfaces first, complexity later.

**v1 daemon-backed surfaces:**
1. `orient`
2. `check`
3. `explain`
4. `callers`
5. `callees`
6. `imports`

**Deferred:**
- Full module query family
- Policy write surfaces
- Cross-repo orchestration
- In-memory graph replacement

## 22. Open Design Decisions (Remaining)

These decisions remain open and should not be settled accidentally inside implementation.

### 22.1 Application service packaging

Option A: one new crate for daemon-facing services
- pros: one obvious seam, simple adapter dependency graph
- cons: risk of becoming a second orchestration blob

Option B: one service module per surface family, inside existing support crates where cohesion is high
- pros: strong locality, less artificial layering
- cons: risk of inconsistent request/response conventions

Constraint:
- whichever option is chosen, transport DTOs and error conventions must be uniform
- command modules must not remain the de facto service layer

### 22.2 Extent of in-memory projection in the first daemon slice

Option A: warm SQLite + minimal session metadata only
- pros: lower implementation risk
- cons: smaller latency and reuse gains

Option B: selective projections for module/trust/docs/orient-heavy paths
- pros: meaningful reuse where repeated cost is real
- cons: harder invalidation and refresh rules

Constraint:
- start with caches whose invalidation boundaries are already understood

### 22.3 How far to push document-first authored knowledge in the first daemon slice

Option A: keep current declaration tables for governance, add document anchoring gradually
- pros: lower migration pressure, smaller initial slice
- cons: discovery knowledge remains split between docs and opaque rows

Option B: introduce document-backed authored relationship items as the primary human surface
- pros: matches docs-first product direction and git-native reviewability
- cons: requires contract work for anchors, parsing, and projection rules

Constraint:
- discovery-oriented human knowledge should not end up trapped only in SQLite rows

### 22.4 Cross-platform transport

**Decision:** NDJSON over stdin/stdout is the initial transport.

**Rationale:**
- stdin/stdout works identically on Unix and Windows
- no socket path negotiation, no platform-specific IPC code
- universal for AI agent tooling
- socket transport can be added later as an adapter over the same service layer

**Deferred:**
- Unix socket (for multi-client scenarios where stdin/stdout is insufficient)
- Windows named pipes
- HTTP bridge

Constraint:
- application service contracts must not care which IPC transport is used

## 23. Assumptions and Divergences

Assumptions used in this document:
- `rmap` remains the primary Rust product surface
- SQLite remains the transitional persistence/query adapter during early daemon work
- the `agent` crate is the reference pattern for transport-neutral use-case extraction
- daemonization begins only after discovery surfaces are stable enough to avoid freezing the wrong contracts
- daemon-backed multi-agent use is a primary operational goal, not an incidental optimization
- documentation is the preferred authored surface for hand-discovered architectural knowledge

Intentional divergences from a naive daemon plan:
- not "run existing commands behind a socket"
- not "replace SQLite first"
- not "invent daemon-only response shapes"
- not "make history retention the daemon's central job"
- not "let many clients mutate the same DB file directly and trust SQLite alone to sort it out"

## 24. Bottom Line

The daemon should be built as a long-lived outer adapter over transport-neutral
application services and deterministic support crates, with explicit repo session
management, snapshot pinning, daemon-owned DB coordination, readers-writer semantics
for multi-agent use, per-repo write serialization, progress/cancellation, a
transitional SQLite-backed persistence strategy, and a document-first authored
knowledge model for human-discovered architectural facts.

The recent `main.rs` refactor matters because it made this architecture feasible. It did not produce the daemon. It exposed the seams required to build one correctly.

## 25. Implementation Slices (D1–D6)

The daemon is not a separate binary. `rmap` gains a daemon mode: `rmap daemon`.

### Transport: NDJSON over stdin/stdout

The initial transport is NDJSON (newline-delimited JSON) over stdin/stdout, not Unix sockets.

Rationale:
- stdin/stdout is the most universal IPC for AI agent tooling
- no socket path negotiation, no platform-specific IPC
- easy to test, easy to pipe, easy to wrap
- progress and cancellation use the same channel with message framing
- socket transport can be added later as an adapter over the same service layer

Request/response envelope:
```json
{"id":"req-1","method":"orient","params":{"repo":"myrepo","focus":"src/core"}}
{"id":"req-1","result":{...}}
```

Progress streaming:
```json
{"id":"req-1","progress":{"phase":"extracting","current":42,"total":100}}
```

Error response:
```json
{"id":"req-1","error":{"code":"RepoNotFound","message":"repo 'foo' not registered"}}
```

### Daemon runtime shape

One process managing multiple repos:
- per-repo coordinator thread
- single writer, multiple readers per repo
- coordinator owns DB handle for its repo
- coordinator serializes write operations
- concurrent reads proceed without blocking each other

```
┌──────────────────────────────────────────────────────────────┐
│                       rmap daemon                            │
│                                                              │
│  ┌─────────────┐   ┌─────────────┐   ┌─────────────┐        │
│  │ repo-A      │   │ repo-B      │   │ repo-C      │        │
│  │ coordinator │   │ coordinator │   │ coordinator │  ...   │
│  │             │   │             │   │             │        │
│  │ writer lock │   │ writer lock │   │ writer lock │        │
│  │ readers: N  │   │ readers: N  │   │ readers: N  │        │
│  │ DB handle   │   │ DB handle   │   │ DB handle   │        │
│  └─────────────┘   └─────────────┘   └─────────────┘        │
│                                                              │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │                    stdio adapter                        │ │
│  │  stdin → request router → coordinator → response → stdout│ │
│  └─────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────┘
```

### Slice D1: Core Policy Module

**Goal:** Define the concurrency policy as a testable support module with no transport knowledge.

Scope:
- `RepoCoordinator` type with readers-writer semantics
- single writer lock per repo
- concurrent reader permits
- writer queue (FIFO fairness)
- explicit state: `Idle`, `Reading(count)`, `Writing`, `Refreshing`
- typed error: `WriteConflict`, `Timeout`, `Cancelled`

**Not in D1:**
- actual DB access
- transport
- application services

**Test surface:**
- unit tests with fake work simulating read/write contention
- deterministic assertions about lock state transitions
- verify writer queue ordering
- verify readers do not block each other
- verify writer blocks new readers until complete

Crate: `rust/crates/daemon-policy/` (new support module)

### Slice D2: Stdio Adapter

**Goal:** NDJSON transport adapter that can drive the coordinator layer.

Scope:
- read NDJSON lines from stdin
- parse request envelope (id, method, params)
- route to mock dispatcher (D1 coordinator + stub services)
- write NDJSON response to stdout
- handle malformed input gracefully
- support shutdown on stdin EOF

**Not in D2:**
- real application services
- real DB access
- progress streaming (deferred to D4/D5)

**Test surface:**
- integration tests with piped stdin/stdout
- verify request/response round-trip
- verify unknown method error
- verify malformed JSON error
- verify graceful shutdown on EOF

Crate: `rust/crates/daemon-transport/` or inline in `rgr`

### Slice D3: Application Service Bridge

**Goal:** Wire transport layer to application services without shell-out.

Scope:
- direct Rust function invocation for each service
- no `std::process::Command`, no subprocess spawning
- request DTO → service call → response DTO
- typed error mapping to transport error envelope
- service registry / dispatch table

**Not in D3:**
- new services (use existing: orient, check, callers, callees, imports)
- progress streaming

**Test surface:**
- verify orient service invocation through transport layer
- verify error propagation preserves type
- verify no subprocess calls in call path

### Slice D4: Write Operations

**Goal:** Index and refresh operations through the daemon with proper coordination.

Scope:
- `index` method → acquire writer lock → run index → release
- `refresh` method → acquire writer lock → run refresh → release
- progress streaming during long operations
- cancellation support (client disconnect or explicit cancel)
- atomic state publication (refresh does not poison prior READY state)

**Test surface:**
- verify writer lock acquired during index
- verify concurrent index requests queue
- verify progress messages stream correctly
- verify cancellation aborts without publishing partial state
- verify failed refresh preserves prior queryable state

### Slice D5: Read Operations

**Goal:** Query operations through the daemon with concurrent reader support.

Scope:
- `orient`, `check`, `explain`, `callers`, `callees`, `imports` methods
- acquire reader permit (non-exclusive)
- concurrent reads proceed in parallel
- reads block if writer is active (or use snapshot isolation)
- response DTO matches CLI JSON output

**Test surface:**
- verify multiple concurrent reads complete
- verify read during refresh uses prior READY state
- verify DTO parity with direct CLI invocation

### Slice D6: Smoke Validation

**Goal:** End-to-end validation on real repos using the test protocol.

Scope:
- spawn `rmap daemon` in test harness
- run full discovery flow: index → orient → check → explain → callers
- compare daemon JSON output against direct CLI JSON output
- verify on validation repos: `repo-graph`, `amodx`, `spring-petclinic`
- document any delta in `docs/testing/daemon-validation-report.md`

**Test surface:**
- byte-for-byte JSON parity (or documented acceptable deltas)
- timing comparison (daemon warm vs CLI cold)
- multi-agent simulation: 3 concurrent readers + 1 writer

### Slice dependency graph

```
D1 (core policy) ← D2 (stdio adapter)
                        ↓
                   D3 (app bridge) ← D4 (write ops)
                                   ← D5 (read ops)
                                          ↓
                                     D6 (smoke validation)
```

D1 is the foundation. D2 depends on D1. D3 depends on D2. D4 and D5 depend on D3. D6 validates the complete stack.

### Definition of Done per Slice

| Slice | DoD |
|-------|-----|
| D1 | Unit tests pass. Coordinator state machine is deterministic. No transport code. |
| D2 | Integration tests with piped stdio. Request/response round-trip works. Unknown method returns error. |
| D3 | At least one real service (orient) callable through transport. No subprocess. |
| D4 | Index and refresh work through daemon. Progress streams. Cancellation works. |
| D5 | All v1 read surfaces work through daemon. Concurrent reads verified. |
| D6 | Validation report documents parity on at least 3 repos. Multi-agent test passes. |

### What This Achieves

After D6:
- `rmap daemon` is a functional long-running process
- AI agents can connect via stdin/stdout and issue queries
- multiple agents reading the same repo do not block each other
- one agent refreshing does not corrupt reads
- the daemon is the coordination authority for shared repo databases
