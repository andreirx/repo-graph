//! LSP transport layer (re-exported from lsp-subprocess).
//!
//! This module re-exports the shared LSP subprocess transport machinery.
//! The implementation lives in the `lsp-subprocess` crate.

pub use lsp_subprocess::{
    IdGenerator, LspResponse, ReaderHandle, TransportError, write_notification, write_request,
};
