//! Protocol Buffers schema parser using tree-sitter-proto.
//!
//! This crate parses `.proto` files into the domain model defined in
//! `contract-schema`. It is an outer-layer syntax adapter — the stable
//! business-logic types live in `contract-schema`, this crate owns the
//! tree-sitter integration and syntax-tree traversal.
//!
//! ## Design Context
//!
//! Normative contract: `docs/slices/cs-1-protobuf-schema.md`
//!
//! The parser extracts:
//! - File metadata (package, syntax, imports, options)
//! - Messages with fields, nested messages/enums, oneofs
//! - Enums with values
//! - Services with methods (for gRPC)
//! - Source locations (line numbers) for all elements
//!
//! ## Usage
//!
//! ```ignore
//! use repo_graph_proto_parser::parse_proto;
//!
//! let source = r#"
//!     syntax = "proto3";
//!     package api.v1;
//!     message User {
//!         string name = 1;
//!     }
//! "#;
//!
//! let result = parse_proto("api/v1/user.proto", source)?;
//! assert_eq!(result.package, "api.v1");
//! assert_eq!(result.messages.len(), 1);
//! ```
//!
//! ## Design Locks
//!
//! - CS-1.P1: Pure parsing — no I/O, no storage
//! - CS-1.P2: Produces contract-schema domain types only
//! - CS-1.P3: Full proto2 + proto3 syntax support
//! - CS-1.P4: Source anchoring (line numbers) for all elements

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod parser;

pub use parser::{parse_proto, ParseError, ParseResult};

// Re-export domain types for convenience
pub use contract_schema::{
    ProtoEnum, ProtoEnumValue, ProtoField, ProtoFieldLabel, ProtoFile, ProtoMessage, ProtoMethod,
    ProtoOneof, ProtoOption, ProtoService, StreamingPattern,
};
