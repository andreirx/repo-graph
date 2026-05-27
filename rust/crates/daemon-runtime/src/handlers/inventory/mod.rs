//! Inventory handler family for daemon requests.
//!
//! LEGACY-CONTRACT-MIGRATION-1D: Migrated from legacy CLI contract.
//!
//! This family handles:
//! - `policy` — query STATUS_MAPPING, BEHAVIORAL_MARKER, RETURN_FATE facts
//! - `classify_retention` — CACHE-SEMANTICS-1 retention classification
//! - `mark_baseline` / `unmark_baseline` — CACHE-SEMANTICS-1 user baseline management

mod baseline;
mod policy;
mod retention;

#[cfg(test)]
mod tests;

pub use baseline::{handle_mark_baseline, handle_unmark_baseline};
pub use policy::handle_policy;
pub use retention::handle_classify_retention;
