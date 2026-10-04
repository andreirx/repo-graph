# repo-graph-import-resolver — agent notes

- `normalize_join` drops a `..` that has no parent left. It does not fail. A caller must reject a path that escapes the repository root before it uses the result. (src/lib.rs:276-295, as of 1510da10; source: TSA-1 admission 1, OC-4)
- `candidate_paths` appends the TS extensions and `/index.<ext>`. It never tries the given path itself. (src/lib.rs:246-267; source: D-TSA-EXACT-CANDIDATE-1)
- `resolve_tsconfig_alias` serves only the LiveGraph route. The indexer uses `tsconfig_alias_hits`, which follows TypeScript's longest-prefix selection. The two differ on a tie. (src/lib.rs:425; source: TSA-1, ALIAS-SELECTION-PARITY-1)
- This crate depends on `repo-graph-ir` only. The indexer depends on this crate since TSA-1. (Cargo.toml; source: D-TSA-RESOLVER-EDGE-1)
