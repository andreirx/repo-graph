//! Gate aggregator.
//!
//! Calls `repo_graph_gate::assemble_from_requirements` through a
//! `GateStorageRead` handle and projects the resulting
//! `GateReport` into at most one agent signal:
//!
//!   - `GATE_NOT_CONFIGURED` limit when no active requirements
//!     exist. Zero signals.
//!   - `GATE_FAIL` signal when `outcome == "fail"`.
//!   - `GATE_INCOMPLETE` signal when `outcome == "incomplete"`.
//!   - `GATE_PASS` signal when `outcome == "pass"` AND at least
//!     one obligation was evaluated. A trivially-empty gate
//!     (no obligations) does NOT emit GATE_PASS — that state
//!     is covered by the limit above.
//!
//! ── Mode ─────────────────────────────────────────────────────
//!
//! The agent orient pipeline always calls gate in
//! `GateMode::Default`. Orient is a "what matters" synthesis
//! surface, not a CI gate; strict/advisory modes are for the
//! `rgr-rust gate` CLI command where the caller has explicit
//! intent about how to treat missing evidence.
//!
//! ── Storage boundary ─────────────────────────────────────────
//!
//! This aggregator is the only place in `repo-graph-agent` that
//! takes a `&dyn GateStorageRead` bound. `orient_repo` requires
//! `S: AgentStorageRead + GateStorageRead` so one concrete
//! adapter (e.g. `StorageConnection` in production, or a test
//! fake) satisfies both ports with a single handle.

use super::AggregatorOutput;
use crate::dto::limit::{Limit, LimitCode};
use crate::dto::signal::{
	GateFailEvidence, GateIncompleteEvidence, GatePassEvidence, Signal,
};
use crate::errors::{AgentStorageError, OrientError};
use repo_graph_gate::{
	assemble_from_requirements, GateError, GateMode, GateReport,
	GateStorageRead,
};

pub fn aggregate<S: GateStorageRead + ?Sized>(
	storage: &S,
	repo_uid: &str,
	snapshot_uid: &str,
	now: &str,
) -> Result<AggregatorOutput, OrientError> {
	// Fetch requirements once so we can short-circuit on empty.
	let requirements = storage
		.get_active_requirements(repo_uid)
		.map_err(|e| {
			OrientError::Storage(AgentStorageError::new(
				"get_active_requirements",
				e.message,
			))
		})?;

	if requirements.is_empty() {
		return Ok(AggregatorOutput {
			signals: Vec::new(),
			limits: vec![Limit::from_code(LimitCode::GateNotConfigured)],
		});
	}

	let report = match assemble_from_requirements(
		storage,
		repo_uid,
		snapshot_uid,
		GateMode::Default,
		now,
		requirements,
	) {
		Ok(r) => r,
		Err(e) => return Err(map_gate_error(e)),
	};

	let signals = project_report(&report);

	Ok(AggregatorOutput { signals, limits: Vec::new() })
}

fn project_report(report: &GateReport) -> Vec<Signal> {
	// A gate with zero obligations reduces to outcome="pass"
	// with total_count=0 in every mode. The caller should
	// already have short-circuited on empty requirements, but
	// if a requirement existed with zero obligations (which
	// the storage layer skips), the assemble path returns an
	// empty report. Treat that exactly the same as "not
	// configured" — emit no signal. The `GATE_NOT_CONFIGURED`
	// limit is added by the caller, not here.
	if report.outcome.counts.total == 0 {
		return Vec::new();
	}

	let counts = &report.outcome.counts;

	match report.outcome.outcome.as_str() {
		"fail" => {
			let failing_obligations: Vec<String> = report
				.obligations
				.iter()
				.filter(|o| {
					matches!(
						o.effective_verdict,
						repo_graph_gate::EffectiveVerdict::FAIL
					)
				})
				.map(|o| format!("{}/{}", o.req_id, o.obligation_id))
				.collect();

			vec![Signal::gate_fail(GateFailEvidence {
				fail_count: counts.fail as u64,
				total_count: counts.total as u64,
				failing_obligations,
			})]
		}
		"incomplete" => vec![Signal::gate_incomplete(GateIncompleteEvidence {
			missing_count: counts.missing_evidence as u64,
			unsupported_count: counts.unsupported as u64,
			total_count: counts.total as u64,
		})],
		"pass" => vec![Signal::gate_pass(GatePassEvidence {
			pass_count: counts.pass as u64,
			waived_count: counts.waived as u64,
			total_count: counts.total as u64,
		})],
		// Unknown outcome string — defensive. Do not panic, do
		// not guess. Emit no signal and let the caller notice
		// through the limit/signal count mismatch.
		_ => Vec::new(),
	}
}

fn map_gate_error(e: GateError) -> OrientError {
	match e {
		GateError::Storage(inner) => OrientError::Storage(
			AgentStorageError::new(inner.operation, inner.message),
		),
		GateError::MalformedEvidence { operation, reason } => {
			OrientError::Storage(AgentStorageError::new(operation, reason))
		}
	}
}
