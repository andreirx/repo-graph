//! repo-graph-state-bindings — state-boundary policy substrate.
//!
//! This crate owns the data contract, validation policy, and
//! matcher primitives for the state-boundary slice of the
//! repo-graph product. It has zero workspace dependencies: under
//! the dependency rule, it is a policy crate that both the
//! future `state-extractor` crate and future tooling can
//! consume without pulling in indexer or storage concerns.
//!
//! Normative contract:
//! `docs/architecture/state-boundary-contract.txt`.
//!
//! Module map:
//!
//! - `basis` — basis tag enum (`stdlib_api`, `sdk_call`).
//! - `table` — binding-table schema, TOML loader, validator,
//!   validated `BindingTable` wrapper.
//! - `stable_key` — validated newtypes (`RepoUid`, `Driver`,
//!   `Provider`, `LogicalName`, `FsPathOrLogical`) and infallible
//!   stable-key builders for four resource shapes
//!   (db / fs / cache / blob).
//! - `matcher` — Form-A matcher primitives (`ImportView`,
//!   `CalleePath`, `MatchResult`, `match_form_a`).
//!
//! Slice-1 scope (SB-1): binding-table loader, schema validator,
//! stable-key builders, basis tag enum, Form-A matcher. Unit
//! tests only. No extractor integration and no edge emission.
//!
//! Out of scope for slice 1:
//!
//! - config/env seam (CONFIG_KEY node kind)
//! - queue / event boundaries
//! - SQL-string parsing
//! - ORM / repository abstractions
//! - compiler / LSP enrichment (Form B)
//! - text-pattern matching (Form A' / text_pattern basis)
//!
//! Design locks (recorded in
//! `docs/milestones/rmap-state-boundaries-v1.md` SB-1 section):
//!
//! - SB-1.1: `toml = "0.8"`
//! - SB-1.2: zero workspace dependencies
//! - SB-1.3: minimal probe entry in `bindings.toml`
//! - SB-1.4: function-form matcher
//! - SB-1.5: independent view structs (no classification dep)
//! - SB-1.6: validated newtypes, infallible builders

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod basis;
pub mod matcher;
pub mod stable_key;
pub mod table;

// Re-export the most commonly used types at the crate root for
// convenience. Consumers that need the full surface go through the
// module namespaces.
pub use basis::Basis;
pub use matcher::{match_form_a, CalleePath, ImportView, MatchResult};
pub use stable_key::{
	build_blob, build_cache_state, build_db_resource, build_fs_path, Driver,
	FsPathOrLogical, LogicalName, Provider, RepoUid, StableKey, ValidationError,
	NODE_KIND_BLOB, NODE_KIND_DB_RESOURCE, NODE_KIND_FS_PATH, NODE_KIND_STATE,
};
pub use table::{BindingEntry, BindingTable, Direction, Language, ResourceKind, TableError};
