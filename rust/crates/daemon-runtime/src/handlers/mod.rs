//! Handler modules for daemon requests.
//!
//! Each handler family lives in its own module to keep dispatch.rs
//! focused on routing/wiring only.
//!
//! # Architecture
//!
//! ```text
//! dispatch.rs → handlers/quality.rs → application services
//!             → handlers/governance.rs
//!             → handlers/inventory.rs
//! ```
//!
//! Handler functions take the necessary context (DaemonState, Request)
//! and return DispatchResult. They do not know about transport or
//! serialization beyond the JSON request/response contract.

pub mod governance;
pub mod inventory;
pub mod metrics;
pub mod quality;
pub mod support;
