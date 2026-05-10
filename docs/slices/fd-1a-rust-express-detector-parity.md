# FD-1A: Rust Express Detector Parity

Status: PLANNED
Depends: None (uses existing ts-extractor output)
Follow-on: Surface translation may be wired in boundary-interaction crate

## Goal

Port the Express route detector from the legacy TypeScript codebase to Rust. Identify Express route registrations (`app.get`, `app.post`, `router.put`) and emit framework evidence. The boundary layer consumes this evidence to produce HTTP provider surfaces.

## Certainty Layer

**Layer 3 (Orientation Hints)**

Express detection is heuristic in a dynamic language. Pattern matching framework conventions produces evidence-backed hints, not deterministic guarantees.

### Degradation Policy

When a potential route is detected but path is unresolvable:
- Emit evidence with `path = "unknown"`
- Set `confidence = 0.5`
- Include in extraction diagnostics

When variable is named `app` or `router` but not confirmed Express:
- Emit evidence with `confidence = 0.3`
- Tag as `unconfirmed_framework`

## Scope

### In Scope

**Detector Output Contract:**

The detector emits `FrameworkEvidence` records:
```rust
struct FrameworkEvidence {
    kind: "express_route",
    method: "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "ALL" | "USE",
    path: String,           // "/api/users" or "unknown"
    handler_location: Location,
    receiver_name: String,  // "app", "router", etc.
    confidence: f64,        // 0.3 (unconfirmed) to 1.0 (confirmed)
}
```

**Detection Patterns:**
- `app.get('/path', handler)` — confidence 0.8
- `router.post('/path', handler)` — confidence 0.8
- `express.Router().get('/path', handler)` — confidence 1.0
- `app.use('/prefix', router)` — confidence 0.7 (middleware mount)

**Surface Translation (wiring only):**
- This slice wires detector output to boundary-interaction
- Boundary-interaction creates `project_surface` with:
  - `surface_kind = "http_provider"`
  - `method`, `path` from evidence
- Full surface semantics owned by boundary-interaction crate

### Out of Scope

- React/Frontend detection (FD-1B)
- Dynamic route construction (`eval`, template strings with variables)
- Express middleware analysis (beyond mount points)
- Route parameter extraction (`:id` patterns — future slice)

## Crate Layout

```
rust/crates/detectors/
├── src/
│   ├── lib.rs                    # Detector registry
│   ├── express/
│   │   ├── mod.rs                # Express detector entry
│   │   ├── patterns.rs           # Route pattern matching
│   │   └── evidence.rs           # FrameworkEvidence struct
│   └── traits.rs                 # FrameworkDetector trait
└── tests/
    ├── express_routes.rs         # Route detection tests
    └── fixtures/
        ├── simple_express.js
        ├── router_mounts.js
        └── nested_routers.js

rust/crates/boundary-interaction/
└── src/
    └── framework_surfaces.rs     # Evidence → surface translation
```

## Prerequisites

- `ts-extractor` emits `CALL` nodes with:
  - `callee_property` (method name: get, post, etc.)
  - `callee_object` (receiver: app, router)
  - `arguments` array with string literals
- Boundary-interaction crate can create `project_surface` records

## Validation Corpus

Repository: `test/fixtures/typescript/express-api-corpus/`

Must contain:
- `app.get()`, `app.post()` basic routes
- `express.Router()` usage
- `app.use('/prefix', router)` mount
- At least 10 routes across 3+ files

## Validation Commands

```bash
# 1. Build
cd rust && cargo build -p repo-graph-detectors

# 2. Unit tests
cargo test -p repo-graph-detectors express

# 3. Index validation corpus (product surface)
rmap index test/fixtures/typescript/express-api-corpus ./test-artifacts/fd-1a.db

# 4. Primary validation: list HTTP provider surfaces
rmap surfaces list ./test-artifacts/fd-1a.db express-api-corpus --kind http_provider

# 5. Semantic check: verify specific route exists
rmap surfaces list ./test-artifacts/fd-1a.db express-api-corpus --kind http_provider \
  | jq '.results[] | select(.path == "/api/users" and .method == "GET")'
# Must return exactly one surface

# 6. Semantic check: verify POST route
rmap surfaces list ./test-artifacts/fd-1a.db express-api-corpus --kind http_provider \
  | jq '.results[] | select(.method == "POST")'
# Must return at least one

# 7. Count comparison
rmap surfaces list ./test-artifacts/fd-1a.db express-api-corpus --kind http_provider | jq '.count'
# Compare with legacy TS detector; must be within ±10%
```

## Acceptance Criteria

**Evidence Emission (detector responsibility):**
1. `ExpressDetector` implements `FrameworkDetector` trait
2. Detection patterns: `app.get`, `app.post`, `router.*` recognized
3. `FrameworkEvidence` emitted with method, path, confidence

**Surface Translation (boundary-interaction responsibility):**
4. Boundary-interaction creates `http_provider` surfaces from evidence
5. `rmap surfaces list --kind http_provider` returns results (not empty)

**Semantic Examples:**
6. `app.get('/api/users', handler)` → surface with `method: "GET"`, `path: "/api/users"`
7. `router.post('/login', authHandler)` → surface with `method: "POST"`, `path: "/login"`
8. `app.use('/v1', apiRouter)` → evidence emitted (mount point), surface creation TBD

**Negative Example:**
9. `app.listen(3000)` → no surface (not a route)

**Parity:**
10. Route count within ±10% of legacy TS detector
11. `cargo test -p repo-graph-detectors` — all pass

## Definition of Parity

"Parity" for this slice means:
- **Route count:** Within ±10% of legacy TS detector
- **Method accuracy:** GET/POST/PUT/DELETE correctly classified
- **Path extraction:** Literal paths match exactly

NOT required:
- Exact confidence values
- Route parameter extraction (`:id`)
- Middleware chain analysis

## Alternatives Considered

### A. Combine detector and surface creation
Rejected: Violates layer separation. Detector emits evidence; boundary layer creates surfaces.

### B. Create dedicated framework-detectors crate
Acceptable alternative. Current plan uses `detectors` crate. Can split later if needed.

### C. Detect route parameters
Deferred: Adds complexity. Path string extraction is sufficient for orientation.
