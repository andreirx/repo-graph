//! XPART-PROVE-1 probe library surface (research/probe, NOT production).
//!
//! Exposes the export-surface reconciliation engine so other Stage-B probes (REFRESH-PROBE-1's
//! `refresh-probe`) reuse the SAME alias logic for timing instead of forking it. The binary
//! (`main.rs`) consumes this same module. See docs/slices/xpart-prove-1b.md.
//!
//! BOUNDARY (tech debt, ratified REFRESH-PROBE-1): this lib is research-tool reuse INSIDE
//! `rust/tools` only. Production crates MUST NOT depend on `xpart-probe`. If LiveGraph reuses
//! export-surface reconciliation, that logic must first move into a proper support crate. See
//! docs/slices/refresh-probe-1.md (Tool-reuse boundary).

pub mod export_alias;
