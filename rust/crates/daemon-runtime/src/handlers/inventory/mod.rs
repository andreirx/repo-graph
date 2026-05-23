//! Inventory handler family for daemon requests.
//!
//! LEGACY-CONTRACT-MIGRATION-1D: Migrated from legacy CLI contract.
//!
//! This family handles policy fact queries:
//! - `policy` — query STATUS_MAPPING, BEHAVIORAL_MARKER, RETURN_FATE facts

mod policy;

#[cfg(test)]
mod tests;

pub use policy::handle_policy;
