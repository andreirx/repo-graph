# Runtime/Build Model v1 Closeout

Validation report for runtime/build v1 completion.

**Date:** 2026-04-24  
**Revision:** 1

## Shipped Capabilities

### Phase 0: Identity Infrastructure

| Component | Status | Notes |
|-----------|--------|-------|
| Migration 018 (stable_surface_key) | SHIPPED | Table rebuild, new columns, UNIQUE constraint |
| `surface-identity.ts` | SHIPPED | Canonical JSON, source-specific ID, stable key, UID generation |
| `DetectedSurface` interface | SHIPPED | sourceType + source-specific fields required |
| `MissingIdentityFieldError` | SHIPPED | Fails closed on missing identity fields |

### Phase 0: Package Ecosystem Detectors

| Detector | Source Type | Identity Fields | surfaceKind | Maturity |
|----------|-------------|-----------------|-------------|----------|
| package.json bin | `package_json_bin` | binName + entrypointPath | cli | MATURE |
| package.json main/exports | `package_json_main` | entrypointPath | library | MATURE |
| package.json framework deps | `framework_dependency` | frameworkName | web_app, backend_service | MATURE |
| package.json script-only | `package_json_scripts` | rootPath (fallback) | library, backend_service | MATURE |
| Cargo.toml [[bin]] | `cargo_bin_target` | binName + entrypointPath | cli | MATURE |
| Cargo.toml [lib] | `cargo_lib_target` | entrypointPath | library | MATURE |
| pyproject.toml scripts | `pyproject_scripts` | scriptName | cli | MATURE |
| pyproject.toml entry_points | `pyproject_entry_points` | scriptName or entrypointPath | cli, library | MATURE |

### Phase 1A: Container Detectors

| Detector | Source Type | Identity Fields | surfaceKind | Maturity |
|----------|-------------|-----------------|-------------|----------|
| Dockerfile | `dockerfile` | dockerfilePath | backend_service, cli | MATURE |
| docker-compose | `docker_compose` | serviceName | backend_service | MATURE |

### Phase 1B: Script-Only Fallback

| Detector | Source Type | Identity Fields | surfaceKind | Maturity |
|----------|-------------|-----------------|-------------|----------|
| Script classification | `package_json_scripts` | rootPath (fallback) | library, backend_service | MATURE |

Script classification rules:
- `start`, `serve`, `dev` → backend_service (0.60)
- `build`, `test`, `lint` only → library (0.50-0.55)
- Fallback for packages with operational scripts but no bin/main/exports/framework

### Phase 1C: Makefile v1

| Detector | Source Type | Identity Fields | surfaceKind | Maturity |
|----------|-------------|-----------------|-------------|----------|
| Makefile targets | `makefile_target` | makefilePath + targetName | cli, library | MATURE |

Target classification rules:
- `run`, `serve`, `start`, `dev` → cli (0.65)
- `test`, `check`, `lint`, `format` → cli (0.65)
- `install`, `deploy`, `migrate` → cli (0.65)
- `all`, `build`, `default` → library (0.55)
- `lib`, `static`, `shared`, `archive` → library (0.55)
- `clean`, `distclean`, `mostlyclean` → skip
- Other explicit targets → skip (design decision: avoid surface pollution)

Skipped patterns (diagnostics emitted):
- Dynamic targets: `$(VAR):`
- Pattern rules: `%.o:`
- Suffix rules: `.c.o:`
- Include directives: `include *.mk`
- Target-specific variable assignments: `target: VAR += value`

### Rust CLI Parity

| Command | Status | Notes |
|---------|--------|-------|
| `rmap surfaces list` | SHIPPED | JSON output, filters: --kind, --runtime, --source, --module |
| `rmap surfaces show` | SHIPPED | Evidence, module linkage, ambiguity handling |

## Consistency Validation

### Identity Model

| Check | Result |
|-------|--------|
| Every detector sets sourceType | PASS |
| Strict identity sources have required fields | PASS |
| MissingIdentityFieldError thrown on missing fields | PASS |
| No source-specific identity collapse cases | PASS |
| stableSurfaceKey deterministic across reindex | PASS (tested) |
| projectSurfaceUid snapshot-scoped | PASS |
| Evidence payloads include identity fields | PASS |

### Source Type Coverage

All implemented sourceType values have corresponding detectors:
- `package_json_bin` ✓
- `package_json_main` ✓
- `package_json_scripts` ✓
- `cargo_bin_target` ✓
- `cargo_lib_target` ✓
- `pyproject_scripts` ✓
- `pyproject_entry_points` ✓
- `framework_dependency` ✓
- `dockerfile` ✓
- `docker_compose` ✓
- `makefile_target` ✓

Reserved but NOT implemented (enum values exist, no detector):
- `cmake_target` — deferred (requires CMake File API)
- `pnpm_workspace`, `npm_workspaces`, etc. — module discovery enrichment, not surface
- `terraform_root`, `pulumi_yaml`, `helm_chart` — deferred (separate deployment slice)
- `compile_commands`, `tsconfig_outdir` — reserved for future use

## Test Coverage

| Test File | Test Count | Coverage |
|-----------|------------|----------|
| `surface-detectors.test.ts` | 90 | All detectors unit tested |
| `surface-identity.test.ts` | 50 | Identity computation, edge cases |
| `surface-discovery.test.ts` | 9 | Orchestrator logic |
| `surface-discovery-integration.test.ts` | 25 | Full stack with real fixtures |
| Rust `surfaces*.rs` | 22 | rmap surfaces list/show |

**Total: 196 tests**

### Fixtures

| Fixture | Purpose |
|---------|---------|
| `standalone-cli/` | package.json bin detection |
| `typescript/module-deps/` | package.json with main, deps |
| `dockerfile-app/` | Dockerfile detection |
| `dockerfile-plus-compose/` | Dockerfile + Compose coexistence |
| `compose-multiservice/` | Multi-service Compose |
| `compose-api-workers/` | API + worker Compose pattern |
| `script-only-package/` | Script-only fallback |
| `script-only-service/` | Script-only with service scripts |
| `makefile-basic/` | Makefile target detection |

## Real-Repo Validation

### repo-graph

| Surface | sourceType | Status |
|---------|------------|--------|
| rgr (cli) | package_json_bin | ✓ Detected |
| rmap (cli) | cargo_bin_target | ✓ Detected |
| Multiple fixture surfaces | various | ✓ Detected |

### makefile-basic Fixture

| Target | surfaceKind | Status |
|--------|-------------|--------|
| build | library | ✓ Detected (integration test) |
| test | cli | ✓ Detected (integration test) |
| install | cli | ✓ Detected (integration test) |

Note: Makefile-only directories without a module manifest (package.json, Cargo.toml, etc.)
produce unattached surfaces. These are collected for Layer 2 operational promotion but
not persisted in v1. The integration tests create module candidates explicitly to verify
surface-to-module linkage.

## Known Limitations

### Makefile Detector

1. **No variable expansion.** `$(CC)` in recipes is preserved literally; not resolved.
2. **No conditional evaluation.** `ifdef`/`ifeq` blocks not interpreted.
3. **No include file processing.** `include *.mk` noted in diagnostics but not followed.
4. **No shell execution.** Cannot determine actual recipe behavior.
5. **Diagnostics not persisted.** Returned from detector but not stored in v1.

### Container Detectors

1. **Single Dockerfile per directory.** Multi-Dockerfile patterns (Dockerfile.dev, Dockerfile.prod) produce multiple surfaces but may confuse module linkage.
2. **No ARG expansion.** Build args in FROM not resolved.
3. **Compose env_file not parsed.** Environment dependencies from .env files not extracted.

### Identity Model

1. **Legacy rows have NULL stable keys.** Migration 018 does not backfill; reindex required.
2. **No cross-repo identity.** Stable keys are repo-scoped; same package in different repos has different keys.

### Module Linkage

1. **Unattached surfaces not persisted.** Surfaces without a matching module candidate are collected but not stored.
2. **Layer 2 promotion not implemented.** Operational promotion of unattached surfaces is documented but deferred.

## Deferred Items

### CMake File API

CMake parsing is complex (generator expressions, imported targets, toolchain context).
The known-good approach is CMake File API: run `cmake -B build`, read `.cmake/api/v1/reply/` JSON.

**Why deferred:**
- Introduces execution boundary (running cmake)
- Requires build directory configuration
- Needs architecture decisions: determinism, safety, caching, failure handling

### Workspace Membership

Workspace manifests (pnpm-workspace.yaml, npm workspaces, lerna.json) are NOT runtime surfaces.

**Correct design:**
- Workspace manifests enrich module discovery evidence
- Package-local detectors produce surfaces
- Workspace root is metadata/context, not a surface

This belongs in the module discovery track, not runtime/build.

### Infra Roots

Terraform, Pulumi, Helm are deployment/infra surfaces, not code runtime/build.

**Why separate:**
- Different evidence semantics
- Different identity model (infra module paths, chart names)
- Lower priority for code orientation than build surfaces

### Makefile Diagnostics Persistence

Diagnostics (skipped_dynamic_target, skipped_pattern_rule, etc.) are returned from
`detectMakefileSurfaces()` but not persisted in v1.

**Tech debt:** Add diagnostics table or attach to surface evidence in v2.

## Documentation Alignment

| Document | Status |
|----------|--------|
| `docs/design/runtime-build-model-v1.md` | CURRENT (Rev 13) |
| `docs/ROADMAP.md` | CURRENT |
| `docs/VISION.md` | CURRENT |
| `CLAUDE.md` | CURRENT |

## Conclusion

Runtime/build v1 is a coherent stopping point:

1. **Identity infrastructure complete.** Two-key model, strict identity validation, deterministic hashing.
2. **All Phase 0/1 detectors shipped.** Package ecosystems, containers, Makefile.
3. **Rust CLI parity achieved.** `rmap surfaces list/show` operational.
4. **196 tests passing.** Unit, integration, and Rust contract tests.
5. **Deferred items are genuinely different slices.** CMake requires execution boundary design, workspace is module discovery enrichment, infra is deployment surface.

No further detector implementation needed in this track.
