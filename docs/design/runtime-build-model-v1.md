# Runtime / Build Environment Model v1

Design slice for extending the existing project surface infrastructure with
Dockerfile, docker-compose, Makefile detection, and richer package.json
script extraction.

**Status:** PARTIAL IMPLEMENTATION.
- Phase 0 (identity infrastructure): SHIPPED. Migration 018, identity
  helpers, DetectedSurface identity fields.
- Phase 1A (container detectors): SHIPPED. Dockerfile and docker-compose
  detectors implemented and wired.
- Phase 1B (script-only fallback): SHIPPED. Package.json script-only
  fallback for packages with operational scripts but no bin/main/exports.
- Phase 1C (Makefile v1): SHIPPED. Conservative Makefile target detection,
  one surface per explicit target, identity via `makefilePath:targetName`.
- Rust CLI parity: SHIPPED. `rmap surfaces list/show` with filters,
  module enrichment, evidence count, ambiguity handling.
- **v1 COMPLETE.** See `docs/validation/runtime-build-v1-closeout.md`.
- **Deferred:** CMake File API (requires execution boundary design),
  workspace membership (belongs in module discovery enrichment, not surfaces),
  infra roots (separate deployment-surface slice).

**Revision:** 13 (2026-04-24)

### Corrections in Rev 13

1. **v1 closeout complete:** Runtime/build v1 is a coherent stopping point.
   Closeout validation report at `docs/validation/runtime-build-v1-closeout.md`.

2. **Workspace membership reclassified:** Workspace manifests (pnpm-workspace.yaml,
   npm workspaces, lerna.json) belong in module discovery enrichment, not runtime/build
   surfaces. Q3 answer revised to reflect this architectural decision.

3. **Deferred items explicit:** CMake File API requires execution boundary design.
   Infra roots (Terraform, Pulumi, Helm) are a separate deployment-surface slice.
   Neither is v1 scope.

### Corrections in Rev 12

1. **Makefile identity fix:** `makefile_target` sourceSpecificId must include
   `targetName`, not just `makefilePath`. Multiple targets from one Makefile
   must produce distinct surfaces. Identity: `makefilePath:targetName`.

2. **CMake explicitly deferred:** CMake parsing is complex (generator expressions,
   imported targets, toolchain context). Known-good solution is CMake File API
   over generated build metadata. Do not parse CMakeLists.txt as if declarative.

3. **Makefile v1 implementation shipped:** Conservative detector with explicit
   target parsing only. Unknown targets skipped (design decision: avoid surface
   pollution). Diagnostics returned but not persisted in v1.

4. **DetectedSurface interface extension:** Added `targetName?: string` for
   Makefile target identity. Also added to `SourceSpecificIdInput`.

5. **Indexer wiring:** `isMakefileName()` recognizes `Makefile`, `makefile`,
   `GNUmakefile`. `*.mk` files excluded (typically included, not standalone).

### Corrections in Rev 11

1. **Script-only fallback shipped:** Implemented and tested `detectScriptOnlyFallback()`
   in surface-detectors.ts. Packages with scripts but no bin/main/exports/framework
   deps now produce a low-confidence surface.

2. **Test fixtures added:** `script-only-package/` and `script-only-service/` fixtures
   for integration testing.

3. **Phase 1 items 7-10 marked SHIPPED:** Updated status for script-only fallback,
   Dockerfile detector, docker-compose adapter, and docker-compose detector.

### Corrections in Rev 10

1. **Explicit sourceType mapping table:** Added table showing exact `sourceType` and
   identity field for each existing detector output (package_json_bin, cargo_bin_target,
   etc.).

2. **Fixture repair policy:** Existing test fixtures that construct `DetectedSurface`
   must be updated explicitly with `sourceType` and source-specific fields. Do NOT
   make identity optional everywhere to avoid fixture repair.

3. **Model integrity:** New detected surfaces must have explicit identity evidence.
   The repair should be explicit, not watered down by compatibility escapes.

### Corrections in Rev 9

1. **Architectural constraint:** Stable key computation belongs in core, not storage.
   Storage persists and enforces; core computes. Added explicit guidance.

2. **Pure support module first:** Added `src/core/runtime/surface-identity.ts` as
   Step 1, containing all identity computation functions.

3. **DetectedSurface before ProjectSurface:** `sourceType` required on DetectedSurface;
   nullable fields only on persisted ProjectSurface for legacy compatibility.

4. **Legacy row handling tightened:** Do not invent best-effort `source_specific_id`
   values. Set NULL for legacy rows.

5. **Testing requirements expanded:** Added dedicated migration tests with real child
   rows, migration failure test, identity helper tests, surface discovery tests.

6. **Acceptance criteria explicit:** Five specific criteria for Phase 0 completion.

### Corrections in Rev 8

1. **try/finally for FK restoration:** Migration now wraps FK-disabled section in
   `try/finally` to guarantee `PRAGMA foreign_keys = ON` is restored even if
   rebuild transaction or integrity check throws.

2. **Migration record after integrity check:** Moved `schema_migrations` insert
   outside the rebuild transaction, after `foreign_key_check` passes. If rebuild
   succeeds but integrity fails, migration is not recorded (can be diagnosed and
   retried).

3. **Updated invariants:** Added explicit guarantees about failure recovery:
   - Transaction failure → FK restored, migration not recorded
   - Integrity check failure → FK restored, migration not recorded
   - Success → FK restored, migration recorded

### Corrections in Rev 7

1. **FK-safe rebuild mechanics:** Added Foreign Key Safety section. Codebase uses
   `PRAGMA foreign_keys = ON`; dropping `project_surfaces` would cascade-delete
   all child rows. Migration now disables FK enforcement, rebuilds with preserved
   primary keys, runs `PRAGMA foreign_key_check`, then re-enables.

2. **TypeScript migration implementation:** Replaced raw SQL with TypeScript
   implementation showing the exact FK disable/enable sequence and integrity check.

3. **Corrected NULL rationale:** SQLite permits multiple NULLs in UNIQUE constraints.
   The rebuild is needed because the old identity model is wrong, not because of
   NULL handling. Updated explanation.

### Corrections in Rev 6

1. **Table rebuild required:** Migration 013's unique constraint
   `(snapshot_uid, module_candidate_uid, surface_kind, entrypoint_path)` uses
   the wrong identity model. SQLite cannot drop constraints; table must be rebuilt.
   Full migration provided with CREATE TABLE, INSERT, DROP, RENAME sequence.

2. **New columns for source identity:** Added `source_type` and `source_specific_id`
   columns to `project_surfaces`. These are required to compute stable keys and
   were missing from the existing schema.

3. **Legacy backfill strategy:** Existing rows lack fields needed for stable key
   computation. Two options documented; Option A (NULL stable key for legacy,
   reindex required) recommended. No false claims about backfill from existing
   fields.

4. **New unique constraint:** `UNIQUE (snapshot_uid, stable_surface_key)` replaces
   the old constraint. This is the correct identity model that prevents collapse.

### Corrections in Rev 5

1. **Migration required:** Added migration 018 for `stable_surface_key` column
   in `project_surfaces` table. Previous revision claimed no migration needed
   while adding a new persisted field.

2. **Recursive canonical JSON:** Fixed `canonicalJsonStringify` to recursively
   sort keys at all nesting levels. Previous naive implementation only sorted
   top-level keys, failing on nested Compose/Docker payloads.

3. **Duplicate suppression by stable key:** Changed from `module_candidate_uid`
   to `stable_surface_key` for duplicate detection. Ensures consistency with
   the stable key identity model.

4. **Implementation order restructured:** Added Phase 0 (infrastructure) before
   Phase 1 (detectors). Migration and canonical JSON helper must exist before
   any detector can compute stable keys.

### Corrections in Rev 4

1. **Two-key identity model:** Split into `project_surface_uid` (snapshot-scoped,
   changes on reindex) and `stable_surface_key` (snapshot-independent, stable
   across reindex). Fixes contradiction where reindex test expected identical
   UIDs but UID formula included snapshot_uid.

2. **Evidence hash size:** Increased from 64 bits (16 hex) to 128 bits (32 hex).
   Matches repo convention and provides adequate collision resistance at scale.

3. **Canonical JSON encoding:** Evidence and stable key generation now use
   canonical JSON with sorted keys. Ensures identical payloads produce identical
   hashes regardless of object key insertion order.

4. **Package bin alias identity:** Added `bin_name` to package.json bin identity.
   Multiple bins pointing to same script path now produce separate surfaces.
   Added test case 13 for bin alias separation.

5. **Reindex stability test corrected:** Now compares `stable_surface_key` values
   across snapshots, not `project_surface_uid` (which differs by design).

### Corrections in Rev 3

1. **Surface identity tuple:** Expanded identity to include `source_type` and
   `source_specific_id` (service name, dockerfile path, etc.). Prevents collapse
   of multiple docker-compose services under one module.

2. **UID preimage encoding:** Changed from colon-joined strings to JSON-array
   encoding. Handles paths containing colons correctly.

3. **source_specific_id fallback chain:** Explicit precedence varies by source
   type. Switch dispatch instead of generic fallback chain.

4. **Anti-collapse tests:** Added explicit test cases for multi-service Compose
   and Dockerfile+Compose at same root.

5. **Fixed section numbering:** Renumbered duplicate "### 3." headings.

6. **New fixture:** Added `dockerfile-plus-compose/` to validate two-surface
   behavior when both Dockerfile and Compose exist at same root.

### Corrections in Rev 2

1. **Script-only packages:** Added fallback surface generation rule. Packages
   with scripts but no bin/main/exports/framework deps now produce a
   low-confidence (0.50-0.55) library or backend_service surface.

2. **Dockerfile runtime semantics:** Split container runtime from base image
   runtime. `runtimeKind: "container"` on surface; `baseRuntimeKind` in
   evidence payload.

3. **Workspace detector boundary:** Removed `resolvedPaths` from core DTO.
   Adapter resolves glob patterns; core receives typed manifest with both
   patterns and resolved member paths.

4. **docker-compose YAML parsing:** Replaced line-based extraction with
   real YAML parser at adapter boundary. Core receives typed DTO.

5. **UID/idempotency rules:** Added deterministic UID generation formulas
   for surfaces and evidence. Reindex produces identical rows.

6. **Conflict/merge rules:** Added explicit rules for Dockerfile + Compose
   same-context scenarios and duplicate surface suppression.

## Prior Art — What Already Exists

### Core Model (TS)

Location: `src/core/runtime/`

```
ProjectSurface         — operational characterization of a module
  projectSurfaceUid    — TEXT primary key
  moduleCandidateUid   — FK to module_candidates
  surfaceKind          — cli | web_app | backend_service | worker | library | infra_root | test_harness
  buildSystem          — typescript_tsc | typescript_bundler | cargo | gradle | maven | python_setuptools | python_poetry | make | cmake | bazel | unknown
  runtimeKind          — node | deno | bun | browser | rust_native | jvm | python | native_c_cpp | container | lambda | unknown
  entrypointPath       — repo-relative path to primary entrypoint file
  confidence           — 0.0-1.0

ProjectSurfaceEvidence — per-surface evidence items
  sourceType           — package_json_bin | package_json_scripts | dockerfile | etc.
  evidenceKind         — binary_entrypoint | script_command | deploy_config | etc.
  sourcePath           — path to the manifest file
  payloadJson          — structured evidence data
```

### Seams (TS)

Location: `src/core/seams/`

```
EnvDependency          — env var consumed by a surface
  envName              — variable name (e.g., "DATABASE_URL")
  accessKind           — required | optional | unknown
  accessPattern        — process_env_dot | os_environ | std_env_var | etc.

FsMutation             — filesystem mutation by a surface
  targetPath           — path being mutated
  mutationKind         — write_file | delete_path | create_dir | etc.
  mutationPattern      — fs_write_file | py_open_write | rust_fs_write | etc.
```

### Detectors (TS)

Location: `src/core/runtime/surface-detectors.ts`

| Detector | Signals | Implemented |
|----------|---------|-------------|
| package.json | bin field, main/exports, framework deps | YES |
| Cargo.toml | [[bin]], [lib], src/main.rs, src/lib.rs | YES |
| pyproject.toml | [project.scripts], [tool.poetry.scripts] | YES |
| Dockerfile | FROM, CMD, ENTRYPOINT | NO |
| docker-compose.yaml | services, build, image | NO |
| Makefile | target rules, .PHONY | NO |

### Storage (TS)

Location: `src/adapters/storage/sqlite/migrations/`

```
migration 013: project_surfaces, project_surface_evidence
migration 015: surface_env_dependencies, surface_env_evidence
migration 016: surface_fs_mutations, surface_fs_mutation_evidence
```

All tables FK to `module_candidates` via `project_surface_uid` chain.

### CLI (TS)

```
rgr surfaces list <repo>            — surface catalog
rgr surfaces show <repo> <surface>  — full detail
rgr surfaces evidence <repo> <surface>  — evidence items
```

---

## v1 Scope

### In Scope (SHIPPED)

1. **Dockerfile detection** — `cli` or `backend_service` surface with `container` runtime
2. **docker-compose.yaml detection** — multi-service orchestration, per-service surfaces
3. **Makefile detection** — `make` build system, target extraction
4. **Package.json scripts extraction** — named scripts as evidence, not just build-system inference

### Explicitly Deferred

- **Workspace membership** — pnpm-workspace.yaml, lerna.json, npm workspaces belong
  in **module discovery enrichment**, not runtime/build surfaces. Workspace manifests
  describe module topology, not runtime characteristics. See v1 closeout and Q3.
- **CMakeLists.txt detection:** CMake is a language with functions, variables,
  generator expressions, imported targets, and toolchain context. Parsing it
  as declarative data produces false confidence. Known-good solution is the
  **CMake File API** which reads generated build metadata after cmake configure.
  Do not regex CMakeLists.txt as if it were a manifest.
- Gradle application plugin detection (requires Groovy/Kotlin DSL parsing)
- Maven pom.xml plugin detection (requires XML parsing with POM inheritance)
- Daemon detection (requires runtime analysis or explicit annotations)
- Kubernetes manifests (too distant from code-level surface)
- Terraform/Pulumi/Helm (existing enum value, but detector deferred)
- Lambda handler pattern detection (exists as RuntimeKind, detector logic deferred)

---

## Core Model Extensions

### 1. Script-Only Packages — Surface Generation Policy

**Problem:** Many real packages have `scripts` (build, test, start, dev) without
`bin`, `main`, `exports`, or framework dependency signals. Under the original
design, no `ProjectSurface` is produced, so script evidence has no parent row.

**Resolution:** Script-only packages produce a low-confidence surface.

Detection order for package.json:
1. `bin` field → `surfaceKind: "cli"`, confidence 0.95
2. `main`/`exports` field → `surfaceKind: "library"`, confidence 0.85
3. Framework dependency → `surfaceKind` per framework, confidence 0.70-0.75
4. **Scripts only (fallback):**
   - If `scripts.start` exists → `surfaceKind: "backend_service"`, confidence 0.55
   - Else if `scripts.build` or `scripts.test` exists → `surfaceKind: "library"`, confidence 0.50
   - Else → no surface (package is truly empty)

This ensures every package with operational scripts has a surface to attach
script evidence to. The low confidence (0.50-0.55) signals that the surface
kind is inferred from scripts alone, not from explicit entrypoint declarations.

### 2. Surface Kind Mapping Table

| Source | surfaceKind | buildSystem | runtimeKind |
|--------|-------------|-------------|-------------|
| Dockerfile | backend_service or cli | unknown | container |
| docker-compose service | backend_service | unknown | container |
| Makefile with .PHONY | library or cli | make | native_c_cpp or unknown |
| package.json (bin) | cli | inferred | node |
| package.json (main/exports) | library | inferred | node |
| package.json (framework dep) | per framework | inferred | node or browser |
| package.json (scripts only) | backend_service or library | inferred | node |

**Dockerfile runtime semantics:** The `runtimeKind` field is always `"container"`
for Dockerfile-sourced surfaces. The base image runtime (node, python, rust, etc.)
is stored in evidence payload as `baseRuntimeKind`, NOT in the surface's
`runtimeKind` field. This separation ensures:
- `rgr surfaces list --runtime container` returns all containerized surfaces
- The underlying language/runtime is queryable via evidence payload
- No contradiction between "runs in container" and "written in Node"

### 3. New Evidence Source Types

Extend `SurfaceEvidenceSourceType`:

```typescript
| "dockerfile"           // Dockerfile detection
| "docker_compose"       // docker-compose.yaml detection
| "makefile_target"      // Makefile rule detection
| "package_json_scripts" // Existing, but expand payload
| "pnpm_workspace"       // pnpm-workspace.yaml
| "npm_workspaces"       // package.json workspaces field
| "lerna_json"           // lerna.json packages field
```

### 4. New Evidence Kinds

Extend `SurfaceEvidenceKind`:

```typescript
| "container_config"     // Dockerfile/docker-compose signals
| "build_target"         // Makefile .PHONY target
| "workspace_member"     // Workspace membership evidence
```

### 5. Package.json Scripts — Evidence Shape

Scripts are NOT new surfaces. They are evidence items attached to the
existing surface (library, cli, backend_service).

```typescript
interface PackageJsonScriptEvidence {
  scriptName: string;      // "build", "test", "start", etc.
  scriptCommand: string;   // the actual command line
  inferredPurpose: ScriptPurpose | null;
}

type ScriptPurpose =
  | "build"       // compile, bundle, transpile
  | "test"        // test runner invocation
  | "start"       // runtime entry
  | "lint"        // linting/formatting
  | "dev"         // development server
  | "deploy"      // deployment script
  | "unknown";
```

Evidence payload in `project_surface_evidence.payload_json`:

```json
{
  "scriptName": "build",
  "scriptCommand": "tsc && vite build",
  "inferredPurpose": "build"
}
```

---

## Detector Specifications

### Dockerfile Detector

**Input:** Dockerfile content as string, repo-relative path.

**Pure function signature:**

```typescript
function detectDockerfileSurfaces(
  content: string,
  manifestRelPath: string,
): DetectedSurface[]
```

**Detection rules:**

1. `runtimeKind` is ALWAYS `"container"`. Base image runtime goes in evidence.

2. Parse FROM instructions to infer `baseRuntimeKind` (evidence field only):
   - `FROM node:*` → baseRuntimeKind: "node"
   - `FROM python:*` → baseRuntimeKind: "python"
   - `FROM rust:*` → baseRuntimeKind: "rust_native"
   - `FROM golang:*` → baseRuntimeKind: "unknown" (Go compiles to native)
   - `FROM alpine`, `FROM ubuntu`, `FROM scratch` → baseRuntimeKind: "unknown"

3. Parse CMD/ENTRYPOINT to infer surfaceKind:
   - `CMD ["node", ...]` → "backend_service" (default for long-running)
   - `CMD ["python", "-m", "flask", ...]` → "backend_service"
   - `ENTRYPOINT` with no CMD → "cli" (one-shot container)
   - No CMD/ENTRYPOINT → "backend_service" (assumes base image has default)

4. Multi-stage builds: only the final stage determines surface and baseRuntimeKind.

**Confidence rules:**

| Signal | Confidence |
|--------|------------|
| Explicit CMD/ENTRYPOINT | 0.90 |
| FROM with known runtime base | 0.85 |
| Dockerfile exists, no CMD | 0.60 |

**Evidence payload:**

```json
{
  "baseImage": "node:18-alpine",
  "baseRuntimeKind": "node",
  "entrypoint": ["node", "dist/server.js"],
  "exposedPorts": [3000],
  "stages": 2
}
```

**Important:** The surface's `runtimeKind` is `"container"`. The evidence
payload's `baseRuntimeKind` captures what language/runtime runs inside the
container. These are separate concerns:
- `runtimeKind: "container"` answers "how is this deployed?"
- `baseRuntimeKind: "node"` answers "what language runs inside?"

### docker-compose.yaml Detector

**Architecture:** The detector is split into two layers:

1. **Adapter layer** — Parses YAML using a real YAML library (e.g., `yaml` npm
   package or `serde_yaml` in Rust). Produces a typed DTO.
2. **Core layer** — Pure function that receives the typed DTO, produces
   `DetectedSurface[]`. No YAML parsing, no filesystem access.

This separation is mandatory. Line-based YAML extraction is not product-grade:
docker-compose files use nested structures, anchors, list vs map syntax for
`depends_on`, profiles, extension fields, and quoted values. A real parser
handles these; regex does not.

**Adapter input:** docker-compose.yaml content as string.

**Adapter output (DTO passed to core):**

```typescript
interface DockerComposeManifest {
  manifestRelPath: string;
  services: DockerComposeService[];
}

interface DockerComposeService {
  name: string;
  build: DockerComposeBuild | null;
  image: string | null;
  ports: string[];
  dependsOn: string[];
  environment: Record<string, string>;
}

interface DockerComposeBuild {
  context: string;           // "./backend" or "."
  dockerfile: string | null; // "Dockerfile.prod" or null for default
}
```

**Core pure function signature:**

```typescript
function detectDockerComposeSurfaces(
  manifest: DockerComposeManifest,
): DetectedSurface[]
```

**Detection rules:**

1. Each service in `manifest.services` produces one `DetectedSurface`:
   - `surfaceKind: "backend_service"` (default)
   - `runtimeKind: "container"`
   - `displayName: service.name`

2. If `service.build` exists:
   - `rootPath` is resolved from `build.context` relative to manifest directory
   - Evidence records build context and dockerfile path

3. If `service.image` exists (no build):
   - Surface is still created (external image dependency)
   - Evidence records image name
   - Confidence is lower (0.75 vs 0.90)

**Confidence rules:**

| Signal | Confidence |
|--------|------------|
| Service with build context | 0.90 |
| Service with image only | 0.75 |

**Evidence payload per service:**

```json
{
  "serviceName": "api",
  "buildContext": "./backend",
  "dockerfile": null,
  "image": null,
  "ports": ["3000:3000"],
  "dependsOn": ["db", "redis"]
}
```

### Makefile Detector v1

**Status:** SHIPPED.

**Scope constraint:** Makefile-only in this slice. CMake deferred.

**Input:** Makefile content as string, repo-relative path.

**Pure function signature:**

```typescript
function detectMakefileSurfaces(
  content: string,
  manifestRelPath: string,
): DetectedSurface[]
```

#### v1 Parsing Constraints

Conservative target extraction only. No shell execution, no variable expansion,
no recursive make traversal.

**What v1 parses:**

1. **Explicit target rules:** Lines matching `target: deps` where `target` is a
   literal name (no `$(VAR)` or `${VAR}` expansion).

2. **`.PHONY` declarations:** Lines matching `.PHONY: target1 target2 ...`

3. **Comment lines:** Lines starting with `#` are ignored.

4. **Line continuations:** `\` at end of line joins with next line.

**What v1 skips (with diagnostic):**

1. **Variable-derived targets:** `$(TARGET): deps` or `${NAME}: deps`
   - Emit diagnostic: `skipped_dynamic_target`

2. **Pattern rules:** `%.o: %.c` or `%: %.in`
   - Emit diagnostic: `skipped_pattern_rule`

3. **Special targets:** `.DEFAULT`, `.SUFFIXES`, `.PRECIOUS`, etc.
   - These are control targets, not build surfaces

4. **Included Makefiles:** `include foo.mk` or `-include optional.mk`
   - v1 does not resolve includes
   - Emit diagnostic: `skipped_include_directive`

5. **Conditional blocks:** `ifeq`, `ifdef`, `ifndef`, `else`, `endif`
   - v1 does not evaluate conditionals
   - Targets inside conditionals are extracted if they are explicit

#### Surface Generation Rules

**Critical identity rule:** Each detected target produces ONE surface. Multiple
targets from the same Makefile produce multiple surfaces with distinct
`stableSurfaceKey` values. The identity tuple is `(makefilePath, targetName)`.

**Surface kind classification:**

| Target Name Pattern | surfaceKind | Confidence | Classification Reason |
|---------------------|-------------|------------|----------------------|
| `run`, `serve`, `start`, `dev` | `cli` | 0.65 | Runnable service target |
| `test`, `check`, `lint`, `format` | `cli` | 0.65 | Test/quality tool |
| `install`, `deploy`, `migrate` | `cli` | 0.65 | Deployment action |
| `all`, `build`, `default` | `library` | 0.55 | Primary build target |
| `lib`, `static`, `shared`, `archive` | `library` | 0.55 | Library build target |
| `clean`, `distclean`, `mostlyclean` | (skip) | — | Not a build surface |
| Other explicit targets | (skip) | — | Unknown target, avoid surface pollution |

**Confidence modifiers:**

| Condition | Modifier |
|-----------|----------|
| Target is `.PHONY` | +0.10 |
| Target has no dependencies | -0.10 |

**Skipped targets (no surface produced):**

- `clean`, `distclean`, `mostlyclean`, `realclean` — cleanup, not build
- Targets starting with `.` (special targets)
- Variable-derived targets
- Pattern rules

#### runtimeKind Inference

| Signal in Makefile | runtimeKind | Confidence |
|--------------------|-------------|------------|
| References `.c`, `.cpp`, `.h`, `.cc` | `native_c_cpp` | 0.70 |
| References `.go` files | `unknown` | — (Go has its own build) |
| References `.rs` files | `unknown` | — (defer to Cargo.toml) |
| `CC =`, `CXX =`, `gcc`, `g++`, `clang` | `native_c_cpp` | 0.75 |
| No language signals | `unknown` | — |

**buildSystem:** Always `"make"` for Makefile-sourced surfaces.

#### Identity Fields

```typescript
interface DetectedSurface {
  // ... standard fields ...
  sourceType: "makefile_target";
  makefilePath: string;   // Required: repo-relative path to Makefile
  targetName: string;     // Required: target name (e.g., "build", "test")
}
```

**sourceSpecificId computation:**

```typescript
case "makefile_target":
  if (!detected.targetName) {
    throw new Error("makefile_target requires targetName");
  }
  return `${detected.makefilePath}:${detected.targetName}`;
```

#### Evidence Schema

One evidence item per detected target:

```typescript
interface MakefileTargetEvidence {
  sourceType: "makefile_target";
  sourcePath: string;              // Makefile path
  evidenceKind: "build_target";
  confidence: number;
  payload: {
    makefilePath: string;
    targetName: string;
    dependencies: string[];        // Literal deps from rule
    isPhony: boolean;
    classificationReason: string;  // "runnable_service" | "build_target" | "unknown"
    skippedDynamic: false;         // Always false for detected targets
  };
}
```

#### Diagnostic Output

For unsupported patterns, emit diagnostic items (not errors):

```typescript
interface MakefileDetectionDiagnostic {
  makefilePath: string;
  kind: "skipped_dynamic_target" | "skipped_pattern_rule" | "skipped_include_directive";
  line: number;
  rawLine: string;
  reason: string;
}
```

Diagnostics are collected but do not prevent surface detection. They inform
callers that the Makefile contains constructs v1 cannot fully model.

#### Technical Debt (v1)

Document these limitations explicitly:

1. **No variable expansion:** Targets like `$(BINARIES): ...` are skipped.
2. **No included Makefile resolution:** `include common.mk` is noted but not followed.
3. **No shell semantic analysis:** Recipe commands are not parsed.
4. **No recursive make traversal:** `$(MAKE) -C subdir` is not followed.
5. **No conditional evaluation:** `ifeq`/`ifdef` blocks are not resolved.
6. **No CMake support:** CMake requires CMake File API, not source parsing.

#### Tests Required

**Unit tests:**

1. Explicit target `build: deps` → one `library` surface
2. `.PHONY: test` → one `cli` surface with `isPhony: true`
3. Multiple targets in same Makefile → distinct `stableSurfaceKey` values
4. Variable-derived target `$(FOO): bar` → no surface, diagnostic emitted
5. Pattern rule `%.o: %.c` → no surface, diagnostic emitted
6. `clean` target → no surface (skipped)
7. No explicit targets → no surfaces

**Integration tests:**

1. Fixture `makefile-basic/` with `Makefile` containing `build`, `test`, `install`
   → three distinct surfaces
2. Existing package.json/Dockerfile/compose tests unchanged (regression)

### Workspace Detector

> **DEFERRED (v1 closeout).** Workspace membership has been reclassified as
> module discovery enrichment, not a runtime/build surface. The design below
> was drafted before this reclassification and is preserved for reference only.
> See `docs/validation/runtime-build-v1-closeout.md` and Q3 in the Appendix.
>
> **Correct architecture:** Workspace manifests should enrich **module candidate
> evidence** (Layer 1 discovery), not produce or attach to surfaces. The workspace
> root is metadata/context on module candidates, not a surface property.

~~**Architecture:** Split into adapter and core layers (same as docker-compose).~~

~~**Sources:**~~
- ~~`pnpm-workspace.yaml` — packages field~~
- ~~`package.json` — workspaces field (npm/yarn)~~
- ~~`lerna.json` — packages field~~

This section is obsolete. Workspace detection belongs in the module discovery
track (`src/core/modules/`), not runtime/build surfaces (`src/core/runtime/`).

---

## UID Generation and Idempotency

### Two-Key Model: UID vs Stable Key

Surfaces use a two-key identity model:

| Key | Scope | Purpose |
|-----|-------|---------|
| `project_surface_uid` | Snapshot-scoped | Primary key, includes `snapshot_uid`. Changes on each reindex. |
| `stable_surface_key` | Snapshot-independent | Cross-snapshot identity. Stable across reindex of same commit. |

**Rationale:** Full indexing creates a new `snapshot_uid` (often timestamp-bearing).
Using `snapshot_uid` in the primary key means reindexing the same commit produces
different `project_surface_uid` values by design. The `stable_surface_key` enables
cross-snapshot comparison and reindex stability verification.

### Surface Identity Tuple (for stable_surface_key)

Each surface has a **source-specific identity** that distinguishes it from
other surfaces under the same module. The identity tuple varies by source:

| Source | Identity Fields |
|--------|-----------------|
| package.json bin | `bin_name` + `entrypoint_path` |
| package.json main/exports | `entrypoint_path` (the main entry) |
| package.json framework | `root_path` + `framework_name` |
| package.json scripts-only | `root_path` (fallback surface) |
| Dockerfile | `source_path` (path to the Dockerfile) |
| docker-compose service | `source_path` + `service_name` |
| Makefile target | `source_path` + `target_name` |
| Cargo.toml bin | `bin_name` + `entrypoint_path` |
| Cargo.toml lib | `root_path` (library surface) |

**Package bin aliases:** Multiple bin names pointing to the same script path
produce **separate surfaces** (one per bin name). The `bin_name` is part of
the identity, not just evidence. This models the operational reality: each
command name is a distinct entrypoint.

**Rationale:** Container surfaces commonly have no `entrypoint_path`. Multiple
docker-compose services under the same module (e.g., `api`, `worker`, `redis`)
must remain distinct. Using `service_name` in the identity prevents collapse.

### stable_surface_key Generation

`stable_surface_key` is generated deterministically using **JSON-array encoding**
with sorted keys (canonical JSON). It excludes `snapshot_uid`:

```typescript
const identityArray = [
  repo_uid,
  module_canonical_root_path,   // stable across snapshots
  surface_kind,
  source_type,                  // "dockerfile", "docker_compose", "package_json_bin", etc.
  source_specific_id,           // see table above
];
const preimage = canonicalJsonStringify(identityArray);  // sorted keys
const key = sha256(preimage).slice(0, 32);               // 128 bits, 32 hex chars
```

### project_surface_uid Generation

`project_surface_uid` is snapshot-scoped. It includes the stable key plus snapshot:

```typescript
const identityArray = [
  snapshot_uid,
  stable_surface_key,
];
const preimage = canonicalJsonStringify(identityArray);
const uid = uuidv5(preimage, SURFACE_NAMESPACE);
```

### source_specific_id Computation

```typescript
function computeSourceSpecificId(detected: DetectedSurface): string {
  // Source-specific identity varies by detector
  switch (detected.sourceType) {
    case "package_json_bin":
    case "cargo_bin_target":
      // Bin aliases: include name to prevent collapse
      return `${detected.binName}:${detected.entrypointPath}`;
    
    case "docker_compose":
      return detected.serviceName;
    
    case "dockerfile":
      return detected.dockerfilePath ?? detected.rootPath;
    
    case "makefile_target":
      // Target name is part of identity to prevent collapse
      if (!detected.targetName) {
        throw new Error("makefile_target requires targetName");
      }
      return `${detected.makefilePath}:${detected.targetName}`;
    
    case "framework_dependency":
      return `framework:${detected.frameworkName}`;
    
    default:
      // Fallback chain for other sources
      if (detected.entrypointPath) return detected.entrypointPath;
      return detected.rootPath;
  }
}
```

**Required interface extension:** The existing `DetectedSurface` interface
(in `src/core/runtime/surface-detectors.ts`) must be extended with optional
identity fields:

```typescript
interface DetectedSurface {
  // ... existing fields ...
  sourceType: SurfaceEvidenceSourceType;  // required for identity dispatch
  binName?: string;            // package.json/Cargo bin name
  serviceName?: string;        // docker-compose service name
  dockerfilePath?: string;     // path to Dockerfile
  makefilePath?: string;       // path to Makefile
  targetName?: string;         // Makefile target name (required for makefile_target)
  frameworkName?: string;      // framework name
}
```

### Canonical JSON Encoding

All identity preimages use **recursive canonical JSON**. The naive
`JSON.stringify(obj, Object.keys(obj).sort())` only sorts top-level keys
and fails on nested objects. Use a proper recursive canonicalizer:

```typescript
// Recursive canonical JSON: sorted keys at all levels, deterministic
function canonicalJsonStringify(value: unknown): string {
  if (value === null || typeof value !== 'object') {
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) {
    return '[' + value.map(canonicalJsonStringify).join(',') + ']';
  }
  const keys = Object.keys(value as Record<string, unknown>).sort();
  const pairs = keys.map(k => 
    JSON.stringify(k) + ':' + canonicalJsonStringify((value as Record<string, unknown>)[k])
  );
  return '{' + pairs.join(',') + '}';
}
```

Alternatively, use `json-stable-stringify` (npm) or equivalent library.
The implementation must handle nested Compose/Docker payloads correctly.

### Evidence UID Generation

`project_surface_evidence_uid` uses canonical JSON encoding with a 128-bit hash:

```typescript
const payloadHash = sha256(canonicalJsonStringify(payload)).slice(0, 32);  // 128 bits

const identityArray = [
  project_surface_uid,
  source_type,
  source_path,
  payloadHash,
];
const preimage = canonicalJsonStringify(identityArray);
const uid = uuidv5(preimage, EVIDENCE_NAMESPACE);
```

**Payload stability:** The recursive canonical JSON encoding ensures identical
payloads produce identical hashes regardless of object key insertion order at
any nesting level. The 128-bit (32 hex char) hash matches the pattern used
elsewhere in the repo and provides sufficient collision resistance at scale.

### Idempotency Guarantees

1. **Stable key stability:** Same code produces same `stable_surface_key` across
   any number of reindexes.
2. **UID variation:** `project_surface_uid` changes per snapshot (by design).
3. **Incremental refresh:** Only updates surfaces for changed files.
4. **Evidence dedup:** Same payload + source produces same evidence UID within
   a snapshot.
5. **No service collapse:** Multiple docker-compose services under one module
   produce distinct stable keys and distinct UIDs.

---

## Detector Precedence and Merging

When multiple manifest files describe the same directory:

1. **Specificity wins:** Cargo.toml > Makefile for Rust projects.
2. **Explicit over implicit:** package.json bin field > framework-dep inference.
3. **No automatic merging:** Each detector produces independent surfaces.
   The orchestrator (`discoverSurfaces`) links to modules by root path.

### Multiple Surfaces Per Module

A single module can have multiple surfaces. This is intentional:

| Module root | Surfaces | Explanation |
|-------------|----------|-------------|
| `./` | cli (package.json bin), container (Dockerfile) | Dev runs via node, prod runs in Docker |
| `./` | library (package.json main), cli (package.json bin) | Package is both importable and executable |
| `./backend` | backend_service (package.json), container (docker-compose) | Express app, also a Compose service |

### Dockerfile + docker-compose Conflict

When both `Dockerfile` and `docker-compose.yaml` exist at the same root:

**Case 1: docker-compose service builds from local Dockerfile**

```yaml
services:
  api:
    build: .        # points to Dockerfile in same directory
```

- Dockerfile detector produces: surface A (container, backend_service)
- docker-compose detector produces: surface B (container, backend_service, displayName: "api")

**Resolution:** Both surfaces are stored. They represent different operational
views (standalone container vs orchestrated service). The docker-compose
evidence includes `buildContext: "."` linking to the Dockerfile context.

**Case 2: docker-compose service uses external image**

```yaml
services:
  redis:
    image: redis:7
```

- Only docker-compose detector produces a surface (redis service).
- No Dockerfile, so no Dockerfile-sourced surface.
- The service surface has lower confidence (0.75) since it's an external image.

### Duplicate Surface Suppression

Duplicate suppression uses `stable_surface_key` as the canonical identity.
Two surfaces are duplicates if and only if they have the same `stable_surface_key`.

Since `stable_surface_key` is computed from:
- `repo_uid`
- `module_canonical_root_path` (not `module_candidate_uid`)
- `surface_kind`
- `source_type`
- `source_specific_id`

...duplicate detection is stable across snapshots and module candidate regeneration.

**When duplicates occur:** The orchestrator keeps the surface with **higher
confidence** and merges evidence from both detectors onto the surviving surface.

**When duplicates do NOT occur (by design):**

| Scenario | Identity difference | Result |
|----------|---------------------|--------|
| Dockerfile + Compose service at same root | source_type differs | 2 surfaces |
| 3 Compose services under one module | service_name differs | 3 surfaces |
| package.json bin + main | source_specific_id differs | 2 surfaces |
| Makefile + Cargo.toml at same root | source_type differs | 2 surfaces |

This prevents the collapse bug where multiple Compose services would merge
into one survivor.

---

## Storage Shape

### Required Migration: Table Rebuild

The two-key identity model requires **replacing the unique constraint** on
`project_surfaces`. Migration 013 created:

```sql
UNIQUE (snapshot_uid, module_candidate_uid, surface_kind, entrypoint_path)
```

**Why rebuild is required:**

The old constraint uses the wrong identity model. It cannot enforce the new
stable-surface identity and will reject or collapse valid surfaces that share
the same `(module, kind, entrypoint)` tuple but differ in source-specific
identity (e.g., two package.json bins pointing to the same script path, or
bin aliases). The constraint must be replaced with one based on `stable_surface_key`.

Note: SQLite permits multiple NULLs in a UNIQUE constraint, so NULL entrypoints
do not themselves cause violations. The issue is the identity model, not NULL
handling.

SQLite cannot drop constraints with `ALTER TABLE`; the table must be rebuilt.

### Foreign Key Safety

This codebase opens SQLite with `PRAGMA foreign_keys = ON`. Child tables
reference `project_surfaces(project_surface_uid)` with `ON DELETE CASCADE`:
- `project_surface_evidence`
- `surface_env_dependencies` / `surface_env_evidence`
- `surface_fs_mutations` / `surface_fs_mutation_evidence`
- Topology links (migration 014)

Dropping `project_surfaces` while FK enforcement is active will cascade-delete
all child rows. The migration must use FK-safe rebuild mechanics.

**Migration 018: FK-safe rebuild of project_surfaces**

```typescript
// TypeScript migration implementation (not raw SQL)
export function runMigration018(db: Database.Database): void {
  // Step 0: Disable FK enforcement for this connection
  // IMPORTANT: Must be done outside any transaction
  db.pragma('foreign_keys = OFF');

  try {
    // Step 1: Rebuild table in transaction (no migration record yet)
    db.transaction(() => {
      // Create new table with correct schema
      db.exec(`
        CREATE TABLE project_surfaces_new (
          project_surface_uid    TEXT PRIMARY KEY,
          snapshot_uid           TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
          repo_uid               TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
          module_candidate_uid   TEXT NOT NULL REFERENCES module_candidates(module_candidate_uid) ON DELETE CASCADE,
          surface_kind           TEXT NOT NULL,
          display_name           TEXT,
          root_path              TEXT NOT NULL,
          entrypoint_path        TEXT,
          build_system           TEXT NOT NULL,
          runtime_kind           TEXT NOT NULL,
          confidence             REAL NOT NULL,
          metadata_json          TEXT,
          source_type            TEXT,
          source_specific_id     TEXT,
          stable_surface_key     TEXT,
          UNIQUE (snapshot_uid, stable_surface_key)
        )
      `);

      // Copy existing rows (preserving primary keys for FK integrity)
      db.exec(`
        INSERT INTO project_surfaces_new
        SELECT 
          project_surface_uid,
          snapshot_uid,
          repo_uid,
          module_candidate_uid,
          surface_kind,
          display_name,
          root_path,
          entrypoint_path,
          build_system,
          runtime_kind,
          confidence,
          metadata_json,
          NULL,
          COALESCE(entrypoint_path, root_path),
          NULL
        FROM project_surfaces
      `);

      // Drop old table (FK enforcement is OFF, so no cascade)
      db.exec('DROP TABLE project_surfaces');

      // Rename new table
      db.exec('ALTER TABLE project_surfaces_new RENAME TO project_surfaces');

      // Recreate indexes
      db.exec('CREATE INDEX idx_project_surfaces_snapshot ON project_surfaces(snapshot_uid)');
      db.exec('CREATE INDEX idx_project_surfaces_module ON project_surfaces(module_candidate_uid)');
      db.exec('CREATE INDEX idx_project_surfaces_stable_key ON project_surfaces(stable_surface_key)');

      // NOTE: Migration record is NOT inserted here
    })();

    // Step 2: Verify FK integrity BEFORE recording migration
    const violations = db.pragma('foreign_key_check') as unknown[];
    if (violations.length > 0) {
      throw new Error(`FK violations after rebuild: ${JSON.stringify(violations)}`);
    }

    // Step 3: Record migration ONLY after integrity check passes
    db.exec(`
      INSERT OR IGNORE INTO schema_migrations (version, name, applied_at)
      VALUES (18, '018-stable-surface-key', datetime('now'))
    `);

  } finally {
    // Step 4: ALWAYS re-enable FK enforcement, even on failure
    db.pragma('foreign_keys = ON');
  }
}
```

**Critical invariants:**
- Primary keys (`project_surface_uid`) are preserved exactly, so child table
  FKs remain valid.
- FK enforcement is disabled before DROP, preventing cascade deletes.
- `try/finally` guarantees FK enforcement is restored even if migration fails.
- Migration is recorded ONLY after `foreign_key_check` passes.
- If rebuild transaction fails, FK enforcement is restored and migration is
  not recorded (can be retried).
- If integrity check fails, FK enforcement is restored and migration is not
  recorded (database is in a consistent but unfinished state).

### Legacy Backfill Strategy

Existing `project_surfaces` rows lack `source_type`, `bin_name`, `serviceName`,
and other fields needed to compute the full stable key. Options:

**Option A: NULL stable key for legacy rows (RECOMMENDED)**

Leave `stable_surface_key = NULL` for pre-migration rows. These surfaces were
created under the old identity model and cannot participate in cross-snapshot
comparison. On next full reindex, surfaces are recreated with proper stable keys.

```sql
-- No backfill computation; leave NULL
-- Application treats NULL stable_surface_key as "legacy, reindex required"
```

**Pros:**
- Migration is fast and safe
- No incorrect stable keys from incomplete data
- Forces reindex to get proper identity

**Cons:**
- Cross-snapshot queries exclude legacy surfaces until reindex

**Option B: Synthetic legacy key**

Compute a legacy-format stable key using available fields only:

```sql
UPDATE project_surfaces
SET stable_surface_key = 'legacy:' || repo_uid || ':' || 
    COALESCE(root_path, '.') || ':' || surface_kind || ':' || 
    COALESCE(entrypoint_path, '')
WHERE stable_surface_key IS NULL;
```

**Pros:**
- All rows have a stable key immediately
- Legacy surfaces can participate in queries

**Cons:**
- Legacy keys may collide (old collapse bug persists for legacy data)
- Legacy keys are not comparable to new-format keys

**Decision:** Option A. Legacy rows get `stable_surface_key = NULL`. Application
code handles NULL as "legacy surface, reindex to get stable key." This is honest
about the data quality and encourages migration via reindex.

### Model Updates Required

Add to `ProjectSurface` interface in `src/core/runtime/project-surface.ts`:

```typescript
interface ProjectSurface {
  // ... existing fields ...
  readonly sourceType: SurfaceEvidenceSourceType | null;  // null for legacy
  readonly sourceSpecificId: string | null;               // null for legacy
  readonly stableSurfaceKey: string | null;               // null for legacy
}
```

### Storage Insert/Query Changes

- **Insert:** Compute `source_type`, `source_specific_id`, and `stable_surface_key`
  before insert. All three are required for new surfaces.
- **Query by stable key:** `WHERE stable_surface_key = ?`
- **Cross-snapshot comparison:** `WHERE stable_surface_key IS NOT NULL GROUP BY stable_surface_key`
- **Legacy detection:** `WHERE stable_surface_key IS NULL`

### Evidence Tables: Extend Existing (No New Tables)

For evidence, extend `SurfaceEvidenceSourceType` and `SurfaceEvidenceKind`
enums in the core model. Persist new evidence types using existing columns:

- `project_surface_evidence.source_type` — new values: "dockerfile", "docker_compose", "makefile_target"
- `project_surface_evidence.evidence_kind` — new values: "container_config", "build_target"
- `project_surface_evidence.payload_json` — detector-specific structured payload

**Pros:**
- No new evidence tables required.
- Query patterns unchanged.
- Evidence is naturally scoped to surfaces.

**Cons:**
- payload_json is schemaless; validation is application-layer.

### Option B: Typed Evidence Tables (DEFERRED)

Create separate tables for each evidence type with explicit columns:

```sql
CREATE TABLE dockerfile_evidence (
  evidence_uid TEXT PRIMARY KEY,
  project_surface_uid TEXT NOT NULL REFERENCES project_surfaces,
  base_image TEXT,
  entrypoint_json TEXT,
  exposed_ports_json TEXT,
  stages INTEGER
);
```

**Pros:**
- Typed queries, schema validation.

**Cons:**
- Migration per evidence type.
- Table explosion as evidence types grow.

### Decision

- **Surface table:** Migration required for `stable_surface_key` column.
- **Evidence tables:** Extend existing table with new enum values (no migration).

Use existing `project_surface_evidence` table with structured `payload_json`.
Type safety is enforced in the core domain layer, not the storage layer.

---

## CLI / Query Surface

### Existing Commands (No Change)

```
rgr surfaces list <repo>
rgr surfaces show <repo> <surface>
rgr surfaces evidence <repo> <surface>
```

### New Query Filters (v1)

```
rgr surfaces list <repo> --runtime container    # Filter by runtime
rgr surfaces list <repo> --build make           # Filter by build system
rgr surfaces list <repo> --kind backend_service # Filter by surface kind
```

### Rust CLI Parity (SHIPPED)

```
rmap surfaces list <db_path> <repo_uid> [--kind <kind>] [--runtime <rt>] [--source <src>] [--module <m>]
rmap surfaces show <db_path> <repo_uid> <surface_ref>
```

Rust CLI surfaces commands are shipped and documented in `docs/milestones/rmap-structural-v1.md`.
Features: filters (kind, runtime, source, module), module enrichment via JOIN, evidence count,
multi-strategy surface ref resolution (UID, UID prefix, stable key, display name), ambiguity handling.
22 CLI regression tests.

---

## Validation Repos

### Primary Validation

| Repo | Expected Surfaces | Key Evidence |
|------|-------------------|--------------|
| repo-graph | 2 (cli: rgr, cli: rmap) | package.json bin, Cargo.toml [[bin]] |
| simple-dockerfile | 1 (backend_service/container) | Dockerfile FROM/CMD, baseRuntimeKind in payload |
| compose-multiservice | 3+ (per docker-compose service) | docker-compose services |
| c-project-make | 1 (library or cli/native_c_cpp) | Makefile .PHONY targets |
| makefile-basic | 3 (build, test, install) | Makefile explicit targets |
| script-only-package | 1 (library, confidence 0.50) | package.json scripts only, no bin/main/exports |
| bin-aliases | 2 (cli: foo, cli: bar) | two bin names pointing to same script |

> **Note:** `pnpm-monorepo` removed. Workspace membership is module discovery
> enrichment, not surface production. See v1 closeout.

### Fixture Creation Required

Create test fixtures in `test/fixtures/`:

1. `simple-dockerfile/` — minimal Dockerfile with FROM, CMD
2. `compose-multiservice/` — docker-compose with 3 services (api, worker, redis) under one module
3. `dockerfile-plus-compose/` — Dockerfile + docker-compose.yaml at same root, Compose builds from local Dockerfile
4. `c-project-make/` — C project with Makefile, .PHONY targets
5. `bin-aliases/` — package.json with `bin: { "foo": "./cli.js", "bar": "./cli.js" }`
6. `script-only-package/` — package.json with scripts.build and scripts.test, no bin/main/exports
7. `makefile-basic/` — Makefile with build/test/install targets (SHIPPED)

### Success Criteria

1. Each detector produces correct `DetectedSurface[]` output.
2. Evidence payloads are parseable and contain expected fields.
3. Confidence values match documented rules.
4. Surfaces link to correct `ModuleCandidate` by root path.
5. `rgr surfaces list` shows all detected surfaces.
6. `rgr surfaces show` includes all evidence items.
7. Script-only packages produce a surface with confidence 0.50-0.55.
8. Dockerfile surfaces have `runtimeKind: "container"` and `baseRuntimeKind` in evidence.
9. Reindexing same commit produces identical `stable_surface_key` values.

### Anti-Collapse Tests (Critical)

These tests verify that the identity tuple prevents incorrect surface merging:

10. **Multi-service Compose under one module:**
    - Fixture: `compose-multiservice/` with services `api`, `worker`, `redis`
    - Expected: 3 distinct surfaces, each with unique `stable_surface_key`
    - Verify: `service_name` in identity prevents collapse

11. **Dockerfile + Compose at same root:**
    - Fixture: `dockerfile-plus-compose/` with Dockerfile and docker-compose service building from it
    - Expected: 2 distinct surfaces (one from Dockerfile detector, one from Compose detector)
    - Verify: `source_type` in identity prevents collapse

12. **Reindex stability:**
    - Run full index twice on same commit (produces two snapshots)
    - Verify: all `stable_surface_key` values are identical across snapshots
    - Verify: `project_surface_uid` values differ (expected, snapshot-scoped)
    - Verify: surface count and structure match

13. **Bin alias separation:**
    - Fixture: package.json with `bin: { "foo": "./cli.js", "bar": "./cli.js" }`
    - Expected: 2 distinct CLI surfaces (foo and bar)
    - Verify: `bin_name` in identity prevents collapse despite same entrypoint

---

## Implementation Order (When Approved)

### Phase 0: Infrastructure (before detectors)

**Architectural constraint:** Stable surface identity is core policy, not storage
adapter policy. Core computes; storage persists and enforces. Do not compute
stable keys in storage insert.

**Step 1: Pure support module**

Create `src/core/runtime/surface-identity.ts`:
- `canonicalJsonStringify(value: unknown): string` — recursive sorted keys
- `computeSourceSpecificId(detected: DetectedSurface): string`
- `computeStableSurfaceKey(input: StableSurfaceKeyInput): string`
- `computeProjectSurfaceUid(snapshotUid: string, stableSurfaceKey: string): string`
- `computeProjectSurfaceEvidenceUid(input: EvidenceUidInput): string`

All pure functions. No storage, no side effects.

**Step 2: Update DetectedSurface before ProjectSurface**

- `sourceType: SurfaceEvidenceSourceType` — required on DetectedSurface
- Source-specific fields (`binName`, `serviceName`, etc.) — sufficient to compute `sourceSpecificId`
- Keep legacy nullable fields only on persisted `ProjectSurface`, not on new `DetectedSurface`

**Step 3: Migration 018**

Rebuild `project_surfaces` table:
- Drop old unique constraint
- Add columns: `source_type`, `source_specific_id`, `stable_surface_key`
- Add new unique constraint `(snapshot_uid, stable_surface_key)`

Legacy row handling:
- Preserve `project_surface_uid` exactly
- `source_type = NULL`
- `source_specific_id = NULL` (do not invent best-effort values)
- `stable_surface_key = NULL`

**Step 4: Core model updates**

- Add `sourceType`, `sourceSpecificId`, `stableSurfaceKey` to `ProjectSurface` interface
- All nullable for legacy compatibility
- New rows: all three must be non-null

**Step 5: Core model enum extensions**

Add new `SurfaceEvidenceSourceType` and `SurfaceEvidenceKind` values.

**Step 6: Update existing detectors (minimal)**

Do the smallest compatibility update needed. Do not change detection behavior
except to populate identity fields required by the new model.

| Existing surface | sourceType | Identity field |
|------------------|------------|----------------|
| package.json bin | `"package_json_bin"` | `binName` |
| package.json main/exports | `"package_json_main"` | — |
| package.json framework dep | `"framework_dependency"` | `frameworkName` |
| Cargo.toml bin | `"cargo_bin_target"` | `binName` |
| Cargo.toml lib | `"cargo_lib_target"` | — |
| pyproject.toml scripts | `"pyproject_scripts"` | `scriptName` |

**Fixture repair policy:**

For old callers/tests that construct `DetectedSurface` manually, update fixtures
explicitly rather than making `sourceType` optional. New detected surfaces must
have explicit identity evidence.

Do NOT water down the model by making identity optional everywhere. The repair
should be explicit:
- Find all test fixtures that construct `DetectedSurface`
- Add `sourceType` and source-specific fields to each
- Verify tests still pass with required fields

This ensures the new identity model is enforced from the start, not gradually
eroded by compatibility escapes.

### Phase 0 Testing Requirements

**Migration tests (dedicated, with real child rows):**
- `project_surface_evidence` rows survive
- `surface_config_roots` rows survive (if table exists)
- `surface_entrypoints` rows survive (if table exists)
- `surface_env_dependencies` + evidence rows survive
- `surface_fs_mutations` + evidence rows survive
- Verify counts and FK links are preserved

**Migration failure test:**
- Force `foreign_key_check` failure or simulate thrown error during rebuild
- Assert `foreign_keys` is ON afterward
- Assert `schema_migrations` does not contain version 18

**Identity helper tests:**
- `canonicalJsonStringify` handles nested objects and arrays
- `computeSourceSpecificId` dispatches correctly by source type
- `computeStableSurfaceKey` produces deterministic output
- `computeProjectSurfaceUid` changes with snapshot, stable with same inputs

**Surface discovery tests:**
- Stable key dedup works in core (before storage)
- `project_surface_uid` is deterministic for same inputs
- Duplicate suppression uses `stableSurfaceKey`

### Phase 0 Acceptance Criteria

1. Full TS test suite passes
2. Focused tests for identity helpers pass
3. Migration tests prove child rows survive
4. Surface-discovery tests prove stable key dedup and deterministic UID
5. No detector behavior added (except minimal sourceType population)

### Phase 1: Detectors

7. **Package.json script-only fallback** — SHIPPED. Low-confidence surface
   generation for packages with operational scripts but no explicit entrypoints.
   Service scripts (start/dev/serve) → backend_service (0.55); build scripts
   (build/test/lint) → library (0.50). Unit and integration tests added.
8. **Dockerfile detector** — SHIPPED. Pure function + unit tests.
9. **docker-compose adapter** — SHIPPED. YAML parsing to typed DTO.
10. **docker-compose core detector** — SHIPPED. Pure function over DTO + unit tests.
11. **Makefile detector v1** — SHIPPED. Pure function, conservative parsing,
    one surface per explicit target. No variable expansion, no CMake. Identity:
    `makefilePath:targetName`. Diagnostics returned but not persisted in v1.
12. **Workspace adapter** — YAML/JSON parsing + glob expansion
13. **Workspace core detector** — pure function over DTO + unit tests
14. **CMake detector** — DEFERRED. Requires CMake File API over generated metadata,
    not source parsing. Do not parse CMakeLists.txt as if declarative.

### Phase 2: Integration

14. **Duplicate surface suppression** — orchestrator merge by stable key
15. **Storage insert/query** — compute stable key on insert, query by stable key
16. **CLI filter flags** — `--runtime`, `--build`, `--kind`
17. **Integration test** — validate fixtures through full pipeline
18. **Documentation** — update CLAUDE.md command reference

---

## Open Questions

### Q1: Should docker-compose services produce separate ModuleCandidates?

**Current answer:** No. Surfaces link to existing module candidates.
A docker-compose.yaml at repo root produces surfaces that link to the
root module candidate. Services are NOT promoted to modules.

**Alternative:** Promote each service to an "operational" module candidate
(like Layer 2 operational promotion for unattached surfaces). This is
complex and deferred.

### Q2: How to handle Makefile-only projects?

**Current answer:** Makefile detector produces a surface with
`buildSystem: "make"`. If no other manifest exists, the module candidate
must already exist (from directory structure or explicit declaration).

The surface links to an existing module. It does not create a module.

### Q3: Should workspace membership be a property of the surface or the module?

**Revised answer (v1 closeout):** Workspace membership belongs in module
discovery enrichment, not runtime/build surfaces.

Workspace manifests (pnpm-workspace.yaml, npm workspaces, lerna.json) describe
module topology, not runtime characteristics. The correct design:
- Workspace manifests enrich **module candidate evidence** (Layer 1 discovery)
- Package-local detectors produce **surfaces** (runtime/build)
- Workspace root is metadata/context attached to module candidates

This question was answered incorrectly in early design iterations. The v1
closeout explicitly reclassifies workspace membership as module discovery
work, not runtime/build surface work. See `docs/validation/runtime-build-v1-closeout.md`.

---

## Technical Debt Notes

### TD-1: Makefile v1 Limitations

The Makefile detector v1 is deliberately conservative:

1. **No variable expansion:** Targets like `$(BINARIES): ...` are skipped.
   Diagnostic emitted: `skipped_dynamic_target`.

2. **No included Makefile resolution:** `include common.mk` is noted but
   not followed. Diagnostic emitted: `skipped_include_directive`.

3. **No shell semantic analysis:** Recipe commands are not parsed. We cannot
   infer what a target actually does from its shell commands.

4. **No recursive make traversal:** `$(MAKE) -C subdir` is not followed.
   Subdirectory Makefiles must be discovered separately.

5. **No conditional evaluation:** `ifeq`/`ifdef` blocks are not resolved.
   Targets inside conditionals are extracted if they are explicit.

6. **No pattern rule semantics:** `%.o: %.c` is skipped. We detect explicit
   targets only.

These limitations are intentional for v1. The alternative—partial variable
expansion with unknown accuracy—produces false confidence. Better to be
conservative and emit diagnostics.

**Future path:** v2 could integrate with make's `--print-data-base` to get
resolved target and dependency information. This requires make execution
(not pure parsing) and is deferred.

### TD-1B: CMake Deferred

CMake is not a build system config file; it is a language with:
- Functions and macros
- Generator expressions (`$<TARGET_FILE:foo>`)
- Imported targets (`find_package` + `target_link_libraries`)
- Toolchain files and platform conditionals
- Multi-configuration generators (Debug/Release)

Parsing `CMakeLists.txt` as if it were declarative data produces false
confidence. The known-good solution is the **CMake File API** which reads
metadata generated after `cmake -B build` (the configure step).

This requires:
1. Running cmake configure (or finding existing build directory)
2. Reading `.cmake/api/v1/reply/` JSON files
3. Extracting target information from generated metadata

This is a fundamentally different approach than source parsing. Deferred
until runtime/build model demonstrates demand.

### TD-2: Dockerfile Multi-Stage Complexity

Multi-stage Dockerfiles can have complex inheritance (`FROM build AS builder`,
`FROM runtime AS final`). The v1 detector uses the final `FROM` instruction
only. Intermediate stages are not modeled.

### TD-3: YAML Parser Dependency

The docker-compose detector requires a YAML parser at the adapter layer.
This adds a runtime dependency:
- TS: `yaml` npm package (or built-in if using Bun)
- Rust: `serde_yaml` crate

This is intentional. Line-based YAML extraction is not product-grade.
The dependency is isolated to the adapter layer; core detection functions
receive typed DTOs and have no YAML dependency.

> **Note:** pnpm-workspace.yaml parsing belongs in module discovery, not
> runtime/build. See v1 closeout.

---

## Appendix: Existing Enum Values

### SurfaceKind (no changes)

```
cli | web_app | backend_service | worker | library | infra_root | test_harness
```

### BuildSystem (no changes)

```
typescript_tsc | typescript_bundler | cargo | gradle | maven |
python_setuptools | python_poetry | make | cmake | bazel | unknown
```

### RuntimeKind (no changes)

```
node | deno | bun | browser | rust_native | jvm | python |
native_c_cpp | container | lambda | unknown
```

### SurfaceEvidenceSourceType (EXTENDED)

Existing:
```
package_json_bin | package_json_scripts | package_json_main |
package_json_deps | cargo_bin_target | cargo_lib_target |
gradle_application_plugin | pyproject_scripts | pyproject_entry_points |
dockerfile | docker_compose | terraform_root | compile_commands |
tsconfig_outdir | framework_dependency
```

Note: `dockerfile` and `docker_compose` already exist in the enum but
have no detector implementation.

### SurfaceEvidenceKind (EXTENDED)

Existing:
```
binary_entrypoint | script_command | main_export | framework_signal |
build_target | deploy_config | infra_config | test_config | compile_config
```

New in v1:
```
container_config | workspace_member
```
