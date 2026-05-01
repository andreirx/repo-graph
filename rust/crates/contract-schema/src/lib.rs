//! contract-schema — Contract/IDL schema substrate for repo-graph.
//!
//! This crate owns the domain model for schema-backed boundary interactions:
//! Protocol Buffers, gRPC, eRPC, and future IDL systems. It is a pure policy
//! crate with zero workspace dependencies, following the dependency rule.
//!
//! ## Design Context
//!
//! Normative contract: `docs/design/boundary-detection-multitrack.md`
//!
//! Schema-backed RPC systems (Track B) are fundamentally different from raw
//! transport mechanisms (Track A):
//! - Precise contract extraction from IDL files
//! - Cross-language by design (generated code in multiple languages)
//! - Explicit versioning surface
//! - Strong provider/consumer linking through contract identity
//!
//! This crate provides the vocabulary for extracting, storing, and querying
//! these contract-based boundaries.
//!
//! ## Module Map
//!
//! - `contract` — Contract kind classification and common types
//! - `proto` — Protocol Buffers schema model (messages, services, methods)
//! - `mapping` — Generated code provenance mapping
//!
//! ## Slice Scope
//!
//! Foundation work for Track B (Schema-Backed RPC):
//! - CS-1: Protobuf schema extraction
//! - CS-2: Generated code mapping
//! - GR-1/2/3: gRPC server/client/linking
//! - ER-1: eRPC IDL extraction (future)
//!
//! ## Design Locks
//!
//! - CS-1.1: Zero workspace dependencies
//! - CS-1.2: Pure domain model, no I/O
//! - CS-1.3: Serde-compatible for storage/JSON roundtrip
//! - CS-1.4: Full qualification via package namespace

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod contract;
pub mod mapping;
pub mod proto;

// Re-export commonly used types at crate root
pub use contract::{ContractKind, SchemaProvenance};
pub use mapping::GeneratedCodeMapping;
pub use proto::{
    ProtoEnum, ProtoEnumValue, ProtoField, ProtoFieldLabel, ProtoFile, ProtoMessage, ProtoMethod,
    ProtoOneof, ProtoOption, ProtoService, StreamingPattern,
};
