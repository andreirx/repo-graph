# FD-1A: Rust Express Detector Parity

Status: **SHIPPED** (2026-05-12)
Depends: FD-SUPPORT-1 (storage write path) — IMPLEMENTED
Unblocks: FD-1B (React detector)

## Shipped Summary

AST-based Express route detection using tree-sitter-typescript. Integrated into compose.rs orchestration (after npm module persistence for FK constraint).

**Parity validated (2026-05-12).** See `fd-1a-parity-report.md` for comparison results.

Core route detection matches TS prototype (15 of 17 routes shared). Documented differences:
- Rust includes USE middleware mounts (TS excludes)
- Rust skips template literals with interpolation (TS strips and keeps partial path)

Both differences favor precision over recall, aligning with project mission.

### Artifacts

- `rust/crates/repo-index/src/express_detector.rs` — detection module
- `test/fixtures/typescript/express-routes/` — validation corpus
- `rust/crates/repo-index/tests/fd_1a_express_integration.rs` — E2E integration test

### Validation Results (EXECUTED)

- 16 routes detected from corpus (exceeds 5-route acceptance criteria)
- Evidence persisted (`evidence_count: 1` for all surfaces)
- Path parameters normalized (`:id` → `{id}`)
- Dynamic paths with interpolation correctly skipped
- Non-Express receivers correctly ignored
- Module resolution via directory-boundary-safe longest-prefix match

## Goal

Detect Express route registrations in TypeScript/JavaScript files and persist them as `http_provider` surfaces via the FD-SUPPORT-1 storage write path.

Parity target: match the TS prototype (`express-route-extractor.ts`) in route count and basic accuracy, using AST-backed detection instead of regex.

## Certainty Layer

**Layer 3 (Orientation Hints)**

Express detection is heuristic. Pattern matching framework conventions produces evidence-backed hints, not deterministic guarantees. Detection confidence varies by evidence quality.

## Problem Analysis

### What Exists

| Component | Location | Capability |
|-----------|----------|------------|
| Express edge classifier | `classification/framework_boundary.rs` | Reclassifies unresolved edges as `FrameworkBoundaryCandidate` |
| Storage write path | `storage/crud/project_surfaces.rs` | `insert_project_surface()`, `insert_project_surface_evidence()` |
| CLI query | `rmap surfaces list/show` | Queries `project_surfaces` table |
| TS prototype | `express-route-extractor.ts` | Regex-based, emits `BoundaryProviderFact` |

### What's Missing

The existing `detect_framework_boundary()` returns `ClassifierVerdict` which contains only:
- `classification: FrameworkBoundaryCandidate`
- `basis_code: ExpressRouteRegistration | ExpressMiddlewareRegistration`

It does NOT contain:
- HTTP method (GET, POST, etc.)
- Route path (`"/api/users"`)
- Handler symbol attribution
- Source file and line
- Framework-specific metadata

**This slice must extract those fields from the source to produce complete surface facts.**

## Scope

### In Scope

1. **Route extraction from TS/JS files** — detect `app.get()`, `router.post()`, etc.
2. **Path literal extraction** — extract string literal route paths
3. **HTTP method classification** — GET, POST, PUT, DELETE, PATCH, ALL, USE
4. **Surface persistence** — write to `project_surfaces` via FD-SUPPORT-1
5. **Evidence persistence** — write detection evidence to `project_surface_evidence`
6. **Orchestration wiring** — integrate into indexing pipeline

### Out of Scope

- React/frontend detection (FD-1B)
- Dynamic route paths (template strings with runtime variables)
- Router mount composition (`app.use('/api', router)` prefix propagation)
- Middleware chain analysis
- Route parameter extraction (`:id` → `{id}` normalization is in scope, semantic analysis is not)

## Architecture Decision: Detection Input

### Option A: Post-Classification Pass on Unresolved Edges

Run after the classifier has marked edges as `FrameworkBoundaryCandidate`. Extract route details from the edge's `metadata_json` which contains the raw callee expression.

```
index_path
  → extract files
  → resolve edges
  → classify unresolved edges (marks Express edges)
  → express_surface_pass(unresolved_edges with ExpressRouteRegistration basis)
    → parse metadata_json for method/path
    → emit BoundaryProviderFact
  → persist to project_surfaces
```

**Benefits:**
- Reuses existing classification infrastructure
- Only processes edges already identified as Express-related
- Does not require re-parsing source files

**Costs:**
- `metadata_json` may not contain sufficient detail (depends on extractor)
- Tightly coupled to classifier output shape

### Option B: Direct AST Pass on Express-Importing Files

Run a dedicated Express extractor on files that import `express`. Parse AST to find route registrations directly.

```
index_path
  → extract files
  → identify files with express import (from import_bindings)
  → express_route_extractor(file_source, symbols)
    → parse AST for app.get/post/etc patterns
    → emit BoundaryProviderFact per route
  → persist to project_surfaces
```

**Benefits:**
- Full AST access, can extract any detail
- Independent of classifier/edge infrastructure
- Mirrors TS prototype approach (but with AST instead of regex)

**Costs:**
- Requires file source access during detection pass
- May re-parse files already parsed by extractor
- Parallel infrastructure to edge-based detection

### Recommendation

**Option B** (direct AST pass).

Rationale:
- The TS prototype already demonstrates this pattern works
- Edge `metadata_json` is not guaranteed to contain route paths
- AST access is necessary for reliable path extraction
- This approach is independent of the edge/classifier infrastructure

## Architecture Decision: Orchestration Pattern

### Option A: Compose-Phase Integration

Add an `express_surface_pass()` function called from `compose::index_path` after extraction completes but before snapshot finalization.

```rust
// In compose::index_path
let express_surfaces = express_surface_pass(&extraction_results, &file_signals)?;
storage.insert_project_surfaces_batch(&express_surfaces)?;
```

**Benefits:**
- Simple integration point
- Access to all extraction results at once
- No new trait/hook infrastructure

**Costs:**
- Compose becomes aware of Express-specific logic
- Less extensible if many framework detectors are added

### Option B: Framework Detector Registry

Create a `FrameworkDetector` trait and registry. Each detector (Express, React, Spring) implements the trait. Compose iterates the registry.

```rust
trait FrameworkDetector {
    fn detect(&self, ctx: &DetectionContext) -> Vec<CreateProjectSurfaceInput>;
}

// In compose::index_path
for detector in framework_detectors.iter() {
    let surfaces = detector.detect(&ctx)?;
    storage.insert_project_surfaces_batch(&surfaces)?;
}
```

**Benefits:**
- Extensible for future detectors
- Clean separation of concerns

**Costs:**
- More infrastructure for first detector
- May be premature abstraction

### Recommendation

**Option A** (compose-phase integration) for FD-1A.

Rationale:
- Single detector does not justify registry infrastructure
- Can refactor to registry pattern when FD-1B (React) arrives
- Keeps first implementation simple and focused

## Detection Algorithm

### Input

- Files with `express` or `@types/express` in import bindings
- File source text (for AST parsing)
- Extracted symbols (for handler attribution)

### Pattern Matching

```
Receiver: app | router | server (conventional Express variable names)
Method: get | post | put | delete | patch | options | head | all | use
Arguments: (path_literal, ...handlers)
```

### Route Registration Detection

```rust
fn detect_express_routes(
    source: &str,
    file_path: &str,
    import_bindings: &[ImportBinding],
    symbols: &[ExtractedSymbol],
) -> Vec<ExpressRouteDetection> {
    // 1. Check for express import
    if !has_express_import(import_bindings) {
        return vec![];
    }
    
    // 2. Parse AST (tree-sitter-typescript)
    let tree = parse_typescript(source)?;
    
    // 3. Find call expressions matching pattern
    let mut routes = vec![];
    for call in find_call_expressions(&tree) {
        if let Some(route) = match_express_route(call, source) {
            routes.push(route);
        }
    }
    
    // 4. Attribute handlers to enclosing symbols
    for route in &mut routes {
        route.handler_stable_key = find_enclosing_symbol(route.line, symbols);
    }
    
    routes
}
```

### Output: ExpressRouteDetection

```rust
struct ExpressRouteDetection {
    http_method: String,      // "GET", "POST", etc.
    path: String,             // "/api/users"
    receiver: String,         // "app", "router"
    line_start: i64,
    handler_stable_key: Option<String>,
    confidence: f64,
}
```

### Conversion to Surface

```rust
fn route_to_surface(
    route: &ExpressRouteDetection,
    file_path: &str,
    snapshot_uid: &str,
    repo_uid: &str,
    module_candidate_uid: &str,
) -> CreateProjectSurfaceInput {
    CreateProjectSurfaceInput {
        snapshot_uid: snapshot_uid.to_string(),
        repo_uid: repo_uid.to_string(),
        module_candidate_uid: module_candidate_uid.to_string(),
        surface_kind: "http_provider".to_string(),
        display_name: Some(format!("{} {}", route.http_method, route.path)),
        root_path: extract_module_root(file_path),
        entrypoint_path: Some(file_path.to_string()),
        build_system: "npm".to_string(),
        runtime_kind: "node".to_string(),
        confidence: route.confidence,
        metadata_json: Some(serde_json::json!({
            "framework": "express",
            "httpMethod": route.http_method,
            "receiver": route.receiver,
        }).to_string()),
        source_type: "express_route".to_string(),
        source_specific_id: None,
        stable_surface_key: format!(
            "surface:express_route:{}:{}",
            route.http_method,
            normalize_path(&route.path)
        ),
    }
}
```

## Crate Location

New module in existing crate (not a new crate):

```
rust/crates/repo-index/src/
├── compose.rs              # existing, add express_surface_pass call
├── express_detector.rs     # NEW: Express route detection
└── ...
```

Rationale: `repo-index` already owns the compose/indexing pipeline. Adding Express detection here keeps the wiring simple. Can extract to separate crate later if needed.

## Validation Corpus

Create test fixtures at `test/fixtures/typescript/express-routes/`:

```
express-routes/
├── basic-app.ts           # app.get, app.post basic patterns
├── router-usage.ts        # express.Router() patterns
├── multiple-routes.ts     # multiple routes in one file
├── dynamic-path.ts        # negative: template string paths (should not detect)
├── non-express-app.ts     # negative: app.get that's not Express
└── package.json           # declares express dependency
```

## Validation Commands

```bash
# 1. Build
cd rust && cargo build -p repo-graph-repo-index

# 2. Unit tests
cargo test -p repo-graph-repo-index express

# 3. Index validation corpus
rmap index test/fixtures/typescript/express-routes ./test-artifacts/fd-1a.db

# 4. List detected surfaces
rmap surfaces list ./test-artifacts/fd-1a.db express-routes --kind http_provider

# 5. Verify route count (compare with TS prototype)
# Expected: at least 5 routes from corpus

# 6. Verify specific route exists
rmap surfaces list ./test-artifacts/fd-1a.db express-routes --kind http_provider \
  | grep "GET /api/users"
# Must find at least one match
```

## Acceptance Criteria

1. `express_detector.rs` module exists in `repo-index`
2. `detect_express_routes()` function extracts routes from TS/JS source
3. `express_surface_pass()` integrates into `compose::index_path`
4. Routes persist to `project_surfaces` with `source_type = "express_route"`
5. Evidence persists to `project_surface_evidence`
6. `rmap surfaces list --kind http_provider` returns Express routes
7. Validation corpus produces at least 5 detected routes
8. Negative cases (dynamic paths, non-Express) produce no false positives
9. Route count within +/-20% of TS prototype on same corpus

## Definition of Done

- Detection functional (criteria 1-6)
- Validation corpus exists (criterion 7)
- Negative cases verified (criterion 8)
- Parity validation documented (criterion 9)
- No Express-specific logic leaks into core indexing (compose only calls pass function)

## Resolved Questions

1. **Module candidate resolution:** SOLVED — Uses directory-boundary-safe longest-prefix match against npm module `package_root`. Files not covered by any module are skipped.

2. **Confidence scoring:** SOLVED — Paths starting with `/` get 0.9, others get 0.7. Dynamic paths skipped entirely.

3. **tree-sitter availability:** SOLVED — Already available via `tree-sitter-typescript` crate.

## Deferred

- Handler symbol attribution (FD-1A-4) — link routes to enclosing function symbols
- Router mount composition (FD-1A-2)
- Middleware analysis (FD-1A-3)
- Express 5.x patterns if they differ
