//! repo-graph-boundary-interaction — boundary interaction policy substrate.
//!
//! This crate owns the data contract, validation policy, and matcher
//! primitives for the IPC and inter-device boundary interaction slice
//! of the repo-graph product. It has zero workspace dependencies: under
//! the dependency rule, it is a policy crate that both the
//! `boundary-interaction-extractor` crate and future tooling can
//! consume without pulling in indexer or storage concerns.
//!
//! Normative contract:
//! `docs/design/boundary-interaction-ipc-device.md`.
//!
//! ## Module map
//!
//! - `types` — core domain types: `BoundaryScope`, `EndpointLocality`,
//!   `InteractionPattern`, `ChannelKind`, `Direction`, `InteractionBasis`.
//! - `surface` — `BoundaryInteractionSurface` (Level 1 architectural fact).
//! - `channel` — `ChannelDetail` (Level 2 mechanism-specific fact).
//! - `table` — binding-table schema, TOML loader, validator.
//! - `query` — read-side DTOs and `BoundaryInteractionReadPort` trait.
//!
//! ## Slice scope (BI-1A: Local IPC)
//!
//! First implementation slice covers local IPC mechanisms with
//! unambiguous `inter_process` scope:
//!
//! - Unix domain sockets (AF_UNIX)
//! - Named pipes / FIFOs (mkfifo)
//! - Anonymous pipes (pipe, pipe2)
//! - POSIX shared memory (shm_open, mmap MAP_SHARED)
//! - POSIX message queues (mq_open, mq_send, mq_receive)
//!
//! See design doc section 7 (Slice 1A) and section 18 (locked decision).
//!
//! ## Explicit exclusions (Slice 1A)
//!
//! - Generic TCP/UDP sockets (Slice 1B — requires scope heuristics)
//! - Serial/CAN (Slice 2 — inter_device scope)
//! - MQTT/ZeroMQ/D-Bus (Slice 3 — library wrappers)
//! - I2C/SPI/USB (Slice 4 — low-level device protocols)
//!
//! ## Design locks
//!
//! - BI-1.1: Zero workspace dependencies
//! - BI-1.2: Two-level model (surface + channel detail)
//! - BI-1.3: `in_process` scope reserved, not emitted in v1
//! - BI-1.4: Shared memory requires dual projection (boundary + state)
//! - BI-1.5: Protocol-focused mechanism naming (not `ipc_` prefixed)
//! - BI-1.6: `endpoint_locality` distinct from `boundary_scope`

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod channel;
pub mod query;
pub mod surface;
pub mod table;
pub mod types;

// Re-export the most commonly used types at the crate root.
pub use channel::ChannelDetail;
pub use query::{
    BasisCount, BoundaryContractView, BoundaryInteractionChannelView, BoundaryInteractionDetail,
    BoundaryInteractionFilter, BoundaryInteractionListItem, BoundaryInteractionReadError,
    BoundaryInteractionReadPort, BoundaryInteractionSummary, DirectionCount, FamilyCount,
    KindCount, ScopeCount,
};
pub use surface::BoundaryInteractionSurface;
pub use types::{
    BoundaryScope, ChannelKind, Direction, EndpointLocality, InteractionBasis, InteractionPattern,
    ProtocolFamily, TransportClass,
};
