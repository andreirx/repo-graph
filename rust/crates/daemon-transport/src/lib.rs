//! NDJSON transport adapter for the rmap daemon.
//!
//! This crate provides the transport layer for daemon communication using
//! newline-delimited JSON. Two transport modes are supported:
//!
//! - **Socket mode** (default): Unix domain socket for resident daemon
//! - **Stdio mode** (`--stdio`): stdin/stdout for testing and debugging
//!
//! # Architecture
//!
//! ```text
//! [Unix socket / stdin] → [NDJSON parser] → [Dispatcher] → [NDJSON serializer] → [socket / stdout]
//! ```
//!
//! The transport layer handles:
//! - Reading NDJSON lines from input
//! - Parsing request envelopes
//! - Routing to a dispatcher
//! - Serializing responses
//! - Writing NDJSON lines to output
//!
//! The dispatcher is pluggable: MockDispatcher for testing,
//! ServiceDispatcher for real application services.
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
//! ## Socket mode (resident daemon)
//!
//! ```no_run
//! use repo_graph_daemon_transport::{run_socket_transport, SocketConfig, MockDispatcher};
//! use std::path::PathBuf;
//!
//! let config = SocketConfig::new(PathBuf::from("/tmp/daemon.sock"));
//! let dispatcher = MockDispatcher::new();
//! run_socket_transport(&config, &dispatcher).expect("transport error");
//! ```
//!
//! ## Stdio mode (debug/test)
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
mod socket;
mod stdio;

pub use dispatch::{
    DispatchResult, Dispatcher, EmitError, MockDispatcher, NoOpEmitter, ProgressEmitter,
};
pub use envelope::{
    ErrorCode, ErrorDetail, ErrorResponse, ProgressDetail, ProgressResponse, Request,
    SuccessResponse,
};
pub use error::TransportError;
pub use socket::{
    bind_socket, cleanup_socket, run_socket, run_socket_transport, BindResult, SocketConfig,
};
pub use stdio::{run_stdio, run_transport};
