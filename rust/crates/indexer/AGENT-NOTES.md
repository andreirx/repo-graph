# indexer — agent notes

- The import ladder runs the stages in order: stable key, extensionless and index file, per-TU include, repo-prefix fallback, Rust crate root, Java suffix, C/C++ include suffix or basename, then for TS the `tsconfig` paths stage, then the workspace-source stage. (src/resolver.rs `resolve_import_ladder`, as of 1510da10; source: TSA-1)
- A stage that binds with a recorded basis returns a variant with `metadata_json`: `IncludeBound`, `WorkspaceSourceEntryBound`, `TsconfigPathsBound`. The carrier gets `basis` and `candidates`. (src/resolver.rs:1029, :1242-1249; source: CIB-1, TSA-1)
- `INDEXER_VERSION` changes by one minor when an index-time fact changes. TOOLCHAIN-STALENESS-1 then schedules the full re-index. (src/orchestrator.rs:80; source: D-REFRESH-STALE-1)
- The refresh planner knows config files by two literal filename tables that must agree: `routing::is_config_file` and `invalidation::RECOGNIZED_CONFIGS`. Since TSA-1 both accept the `tsconfig*.json` and `jsconfig*.json` family. (src/routing.rs, src/invalidation.rs; source: D-TSA-REFRESH-SCOPE-1)
- A Gradle module uid comes from the project directory, not from the Gradle path. Two nested root projects with Gradle path `:` do not collide. (src/settings_gradle.rs:126, :205-228, :405; source: DAP-1)
