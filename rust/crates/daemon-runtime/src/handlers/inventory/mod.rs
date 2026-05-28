//! Inventory handler family for daemon requests.
//!
//! LEGACY-CONTRACT-MIGRATION-1D: Migrated from legacy CLI contract.
//! RETENTION-POLICY-1: Retention lifecycle enforcement.
//!
//! This family handles:
//! - `policy` — query STATUS_MAPPING, BEHAVIORAL_MARKER, RETURN_FATE facts
//! - `classify_retention` — retention lifecycle (classify + prune)
//! - `mark_baseline` / `unmark_baseline` — user baseline management

mod baseline;
mod policy;
mod retention;

#[cfg(test)]
mod tests;

pub use baseline::{handle_mark_baseline, handle_unmark_baseline};
pub use policy::handle_policy;
pub use retention::{enforce_retention_lifecycle, handle_classify_retention, LifecycleResult};
