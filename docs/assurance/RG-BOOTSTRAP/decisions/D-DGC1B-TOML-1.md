# D-DGC1B-TOML-1 — `repo-index` reads `gradle/libs.versions.toml` through the workspace's existing `toml` crate (a direct dependency on an already-built crate), not a hand-written TOML subset

Raised: 2026-10-03 by the manager while packeting DEPS-GRADLE-CATALOG-1B.

**The problem in plain words.** grpc-java declares 79 aliases in `gradle/libs.versions.toml` (`[libraries]`: 74 `alias = "group:artifact:version"` strings and 5 `alias = { module = "group:artifact", version.ref = "x" }` inline tables). `repo-index` has no TOML parser; `toml = "0.8"` is a direct dependency of `indexer` and `rgr`, and `repo-index` already depends on `indexer`, so the crate is in its transitive graph today (`cargo tree -p repo-graph-repo-index -e normal` lists it).

**Options (reward / risk).**
- A — add `toml = "0.8"` to `repo-index/Cargo.toml` and read the catalog as a TOML document: `[libraries]` values that are strings or tables with `module`; everything else (`[bundles]`, `[plugins]`, a `version` without `module`) is not a library alias. Reward: the real grammar (quoted keys, comments, inline tables, multi-line) with no second parser to maintain; no new crate enters the workspace graph or the build. Risk: one more direct edge to an external crate on a crate that had none (`Cargo.lock` gains the dependency line).
- B — a hand-written line grammar for the two forms. Reward: no Cargo edit. Risk: a second TOML grammar in the repository (the one in `indexer` already exists), fragile on legal TOML the two forms do not cover, and a reader that cannot say "malformed" honestly.
- C — read no TOML catalogs (kafka only). Risk: grpc-java's 70 `libraries.` references stay unread; `core` stays `used 1`.

Resolved: 2026-10-03 by the OPERATOR (in-place manager) as **A** — an edge to an external crate the workspace already builds, inside an allocated crate; overridable by the human. A catalog that fails to parse is a FAILED read for that build's alias resolution: every alias reference of the build is counted as unresolved with the parse error named (D-DGC1B-ALIAS-MARKING-1), never a partial table.
