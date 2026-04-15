//! Dead-code aggregator.
//!
//! Calls `AgentStorageRead::find_dead_nodes` with SYMBOL filter
//! and emits `DEAD_CODE` when `dead_count >= DEAD_CODE_EMIT_THRESHOLD`.
//!
//! Threshold policy (Rust-42): emit on `>= 1`. The constant is
//! at the top of the module so future slices can tune it as a
//! single-site change. Do not invent a silent larger threshold
//! — signal emission is a product statement, not a heuristic.
//!
//! ── top_dead ordering ────────────────────────────────────────
//!
//! The contract says the `top_dead` slice in evidence is "top 3
//! by size". The underlying storage port orders rows by
//! `n.name ASC` (alphabetical), NOT by line count, so a naive
//! `.take(N)` would surface alphabetically-first symbols and
//! omit genuinely large dead code.
//!
//! This aggregator sorts the full dead-node list before slicing:
//!
//!   1. `line_count` DESCENDING (largest first). Rows with
//!      `None` line_count (missing line_end) sort LAST, so they
//!      never displace a row with a known size.
//!   2. `symbol` ascending — deterministic tiebreaker when two
//!      symbols have the same line count.
//!   3. `file` ascending — final tiebreaker across identically-
//!      named symbols in different files.
//!
//! `dead_count` reflects the FULL list length regardless of
//! sorting or truncation — the slice is output compression,
//! not detection filtering.

use super::AggregatorOutput;
use crate::dto::signal::{DeadCodeEvidence, DeadSymbolEvidence, Signal};
use crate::errors::AgentStorageError;
use crate::storage_port::{AgentDeadNode, AgentStorageRead};

/// Minimum `dead_count` required to emit the signal.
pub const DEAD_CODE_EMIT_THRESHOLD: usize = 1;

/// Number of dead symbols surfaced in the evidence `top_dead`
/// list. The full count is always in `dead_count`; this cap
/// is an output-compression constant, not a detection threshold.
const DEAD_CODE_TOP_N: usize = 3;

pub fn aggregate<S: AgentStorageRead + ?Sized>(
	storage: &S,
	snapshot_uid: &str,
	repo_uid: &str,
) -> Result<AggregatorOutput, AgentStorageError> {
	let mut dead = storage.find_dead_nodes(snapshot_uid, repo_uid, Some("SYMBOL"))?;

	if dead.len() < DEAD_CODE_EMIT_THRESHOLD {
		return Ok(AggregatorOutput::empty());
	}

	let dead_count = dead.len() as u64;

	sort_top_by_size(&mut dead);
	let top_dead: Vec<DeadSymbolEvidence> = dead
		.into_iter()
		.take(DEAD_CODE_TOP_N)
		.map(|d| DeadSymbolEvidence {
			symbol: d.symbol,
			file: d.file,
			line_count: d.line_count,
		})
		.collect();

	let evidence = DeadCodeEvidence { dead_count, top_dead };

	Ok(AggregatorOutput {
		signals: vec![Signal::dead_code(evidence)],
		limits: Vec::new(),
	})
}

/// Stable sort that orders dead nodes by descending `line_count`
/// with `None` line counts pushed to the tail. Used to compute
/// the `top_dead` evidence slice; does not mutate `dead_count`.
///
/// Sort key: `(size_bucket_desc, symbol_asc, file_asc)` where
/// `size_bucket_desc` is a synthetic ordering that maps `Some(n)`
/// to `(0, n)` (reversed) and `None` to `(1, 0)` so None always
/// follows any known size.
fn sort_top_by_size(nodes: &mut [AgentDeadNode]) {
	nodes.sort_by(|a, b| {
		let key_a: (u8, u64) = match a.line_count {
			Some(n) => (0, u64::MAX - n),
			None => (1, 0),
		};
		let key_b: (u8, u64) = match b.line_count {
			Some(n) => (0, u64::MAX - n),
			None => (1, 0),
		};
		key_a
			.cmp(&key_b)
			.then_with(|| a.symbol.cmp(&b.symbol))
			.then_with(|| a.file.cmp(&b.file))
	});
}

#[cfg(test)]
mod tests {
	use super::*;

	fn node(symbol: &str, file: Option<&str>, line_count: Option<u64>) -> AgentDeadNode {
		AgentDeadNode {
			stable_key: format!("r1:{}:SYMBOL:{}", file.unwrap_or("?"), symbol),
			symbol: symbol.to_string(),
			kind: "SYMBOL".to_string(),
			file: file.map(|s| s.to_string()),
			line_count,
		}
	}

	#[test]
	fn sort_puts_largest_line_count_first() {
		let mut nodes = vec![
			node("aa", Some("a.rs"), Some(5)),
			node("bb", Some("b.rs"), Some(50)),
			node("cc", Some("c.rs"), Some(20)),
		];
		sort_top_by_size(&mut nodes);
		assert_eq!(nodes[0].symbol, "bb");
		assert_eq!(nodes[1].symbol, "cc");
		assert_eq!(nodes[2].symbol, "aa");
	}

	#[test]
	fn sort_pushes_none_line_counts_to_the_tail() {
		let mut nodes = vec![
			node("aa", Some("a.rs"), None),
			node("bb", Some("b.rs"), Some(1)),
			node("cc", Some("c.rs"), None),
		];
		sort_top_by_size(&mut nodes);
		assert_eq!(nodes[0].symbol, "bb", "Some(1) beats every None");
		assert_eq!(nodes[1].symbol, "aa", "None: symbol-asc tiebreak");
		assert_eq!(nodes[2].symbol, "cc");
	}

	#[test]
	fn sort_tiebreaks_equal_line_counts_by_symbol_then_file() {
		let mut nodes = vec![
			node("zz", Some("a.rs"), Some(10)),
			node("aa", Some("z.rs"), Some(10)),
			node("aa", Some("a.rs"), Some(10)),
		];
		sort_top_by_size(&mut nodes);
		assert_eq!(nodes[0].symbol, "aa");
		assert_eq!(nodes[0].file.as_deref(), Some("a.rs"));
		assert_eq!(nodes[1].symbol, "aa");
		assert_eq!(nodes[1].file.as_deref(), Some("z.rs"));
		assert_eq!(nodes[2].symbol, "zz");
	}
}
