//! repo-graph-boundary-interaction-extractor — extraction adapter layer.
//!
//! This crate bridges the policy surface of `repo-graph-boundary-interaction`
//! (types, binding table, enums) with the extraction pipeline. It provides:
//!
//! - Emitter struct for accumulating boundary interaction facts
//! - Evidence struct for extraction provenance
//! - Callsite detection helpers for API pattern matching
//! - State-boundary dual projection logic for shared memory
//!
//! It is an adapter, not a policy crate; its place in the architecture is
//! analogous to `repo-graph-state-extractor` for the state-boundary slice.
//!
//! ## Slice scope (BI-1A: Local IPC)
//!
//! First implementation slice covers C-language detection of:
//!
//! - Unix domain sockets (socket AF_UNIX, bind, connect, listen, accept)
//! - Named pipes (mkfifo, mknod with S_IFIFO)
//! - Anonymous pipes (pipe, pipe2)
//! - POSIX shared memory (shm_open, mmap MAP_SHARED)
//! - POSIX message queues (mq_open, mq_send, mq_receive)
//!
//! ## Integration path
//!
//! The C extractor will:
//! 1. Detect function calls matching the binding table
//! 2. Create `BoundaryCallsite` inputs with call details
//! 3. Pass to `BoundaryInteractionEmitter::emit()`
//! 4. Receive `EmittedFacts` with surface and channel detail
//! 5. For shared memory, also emit state-boundary edges (dual projection)
//!
//! ## Design locks
//!
//! - BI-E-1: Stateful emitter struct (like StateBoundaryEmitter)
//! - BI-E-2: Evidence struct with version constant
//! - BI-E-3: Dual projection helper for shared memory

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod emit;
pub mod evidence;
pub mod socket_lineage;

// Re-export the main surface at crate root
pub use emit::{BoundaryCallsite, BoundaryInteractionEmitter, EmitError, EmittedFacts};
pub use evidence::{BoundaryInteractionEvidence, BOUNDARY_INTERACTION_EVIDENCE_VERSION};
pub use socket_lineage::{FdRegistry, RoleEvidence, SocketLineage, TrackedChannelKind};
