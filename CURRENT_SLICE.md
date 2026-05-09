# CURRENT_SLICE.md

## Current Priority

Module truth-model unification (`docs/slices/rust-module-parity.md`).

## Active Slice

**Rust Module Parity — Phase 3: Inferred modules**

## Branch Intent

Provide module structure for repos without manifest files.
Directory-based heuristics detect module boundaries from filesystem layout.

## Definition of Done (Phase 3)

- [ ] Detect module boundaries from directory structure heuristics
- [ ] Create `module_candidates` with `module_kind` = 'inferred'
- [ ] Create `module_candidate_evidence` with `source_type` = 'directory_heuristic'
- [ ] Compute file ownership from inferred module root paths
- [ ] `rmap modules list` shows inferred modules on manifest-less repos
- [ ] `rmap modules files <module>` returns owned files for inferred modules
- [ ] Validated on local C repos (manifest-less)

## Identity Contract (LOCKED)

**Module key format:** `inferred:{repo_uid}:{directory_path}`

Examples:
- `inferred:linux:drivers/net`
- `inferred:sqlite:.`

Path-anchored identity. Same rule as declared modules.

## Evidence Structure

- `source_type` = "directory_heuristic"
- `source_path` = directory path that triggered inference
- `evidence_kind` = "directory_structure"
- `payload_json` contains:
  - `heuristic`: which heuristic matched (e.g., "src_directory", "top_level_source")
  - `directory_path`: the inferred module root
  - `file_count`: number of source files in scope

## Inference Heuristics (to define)

Candidate heuristics for module boundary detection:
- `src/` or `lib/` directory presence
- Top-level directories containing source files
- Directories with high file count relative to siblings
- Language-specific patterns (e.g., `include/` for C/C++)

Heuristic selection and priority TBD during implementation.

## Scope Constraints (LOCKED)

**In scope:**
- Directory-based module inference
- File ownership for inferred modules
- Lower confidence than declared modules

**Not in scope (deferred):**
- Import graph analysis for module detection
- Historical/git-based module detection
- Cross-repo module inference

## Phase Ordering (LOCKED)

1. **Phase 2** — package.json / pnpm-workspace.yaml — DONE
2. **Phase 2c** — pyproject.toml single-package — DONE
3. **Phase 2b** — Gradle settings.gradle — DONE
4. **Phase 3** — inferred modules — ACTIVE
5. **Phase 4** — MODULE-node fallback deprecation

## Implementation Approach

Same pattern as declared modules:
- Inference module in `indexer` crate (policy owns heuristics)
- Reuse existing storage port (generic module/evidence input types)
- Compose layer wiring in `repo-index` crate
- Lower confidence score (e.g., 0.7) than declared modules (1.0)

## Validation Repos

- Local C repos without manifests (identify available repos)
- Any manifest-less codebase in `../legacy-codebases/`

## Approved DB Path

`./test-artifacts/repo-graph.db`

Do not create databases elsewhere.

## Key References

- `rust/crates/indexer/src/cargo_manifest.rs` — declared module pattern
- `rust/crates/indexer/src/settings_gradle.rs` — Phase 2b pattern
- `rust/crates/repo-index/src/compose.rs` — wiring location
