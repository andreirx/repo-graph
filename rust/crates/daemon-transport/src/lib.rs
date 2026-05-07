//! NDJSON transport adapter for the rmap daemon.
//!
//! This crate provides the transport layer for daemon communication using
//! newline-delimited JSON over stdin/stdout.
//!
//! # Architecture
//!
//! ```text
//! stdin → [NDJSON parser] → [Dispatcher] → [NDJSON serializer] → stdout
//! ```
//!
//! The transport layer handles:
//! - Reading NDJSON lines from input
//! - Parsing request envelopes
//! - Routing to a dispatcher
//! - Serializing responses
//! - Writing NDJSON lines to output
//!
//! The dispatcher is pluggable: D2 uses a mock dispatcher for testing,
//! D3 will use a real dispatcher that invokes application services.
//!
//! # Protocol
//!
//! ## Request
//! ```json
//! {"id":"req-1","method":"orient","params":{"repo":"myrepo"}}
//! ```
//!
//! ## Success Response
//! ```json
//! {"id":"req-1","result":{...}}
//! ```
//!
//! ## Error Response
//! ```json
//! {"id":"req-1","error":{"code":"UnknownMethod","message":"..."}}
//! ```
//!
//! # Usage
//!
//! ```no_run
//! use repo_graph_daemon_transport::{run_stdio, MockDispatcher};
//!
//! let dispatcher = MockDispatcher::new();
//! run_stdio(&dispatcher).expect("transport error");
//! ```

mod dispatch;
mod envelope;
mod error;
mod stdio;

pub use dispatch::{DispatchResult, Dispatcher, MockDispatcher};
pub use envelope::{
    ErrorCode, ErrorDetail, ErrorResponse, ProgressDetail, ProgressResponse, Request,
    SuccessResponse,
};
pub use error::TransportError;
pub use stdio::{run_stdio, run_transport};
