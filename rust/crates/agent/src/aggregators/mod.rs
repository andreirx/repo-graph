//! Aggregators — pure functions that turn port-method results
//! into zero or more `Signal` records.
//!
//! Each aggregator module is responsible for one signal code (or
//! a small related family). The aggregator:
//!
//!   1. Calls the port methods it needs.
//!   2. Applies the emission threshold rules (`dead_count >= 1`,
//!      `cycle_count >= 1`, trust thresholds, etc.).
//!   3. Builds the typed evidence struct and hands it to the
//!      per-code named constructor on `Signal`.
//!   4. Returns a `Vec<Signal>` (zero-length when the signal is
//!      not emitted) and a `Vec<Limit>` (for unavailable surfaces
//!      it detected during its own work).
//!
//! Aggregators never truncate, never sort, and never compute
//! ranks. The orient pipeline does those things in a dedicated
//! pass after collecting signals from every aggregator.
//!
//! Errors from port methods propagate out of the aggregator as
//! `AgentStorageError`; the orient pipeline wraps them into
//! `OrientError::Storage`.

pub mod boundary;
pub mod cycles;
pub mod dead_code;
pub mod module_summary;
pub mod snapshot;
pub mod trust;

use crate::dto::limit::Limit;
use crate::dto::signal::Signal;

/// Collected output of one aggregator.
#[derive(Debug, Default)]
pub struct AggregatorOutput {
	pub signals: Vec<Signal>,
	pub limits: Vec<Limit>,
}

impl AggregatorOutput {
	pub fn empty() -> Self {
		Self::default()
	}
}
