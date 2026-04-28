//! Dead-code aggregator — **surface withdrawn**.
//!
//! The `rmap dead` command surface is withdrawn. This module
//! preserves internal substrate for future coverage-backed or
//! heuristic reintroduction, but emits **no signals or limits**
//! to user-facing output.
//!
//! See `docs/TECH-DEBT.md` for reintroduction conditions.
//!
//! ── Preserved substrate ──────────────────────────────────────────
//!
//! The following internal capabilities are retained:
//!
//!   - Storage queries: `find_dead_nodes`, `find_dead_nodes_in_file`,
//!     `find_dead_nodes_in_path` remain callable.
//!   - Trust computation: `reliability.dead_code` axis is computed
//!     internally (not serialized to user-facing JSON).
//!   - Sorting logic: `sort_top_by_size` preserved for future use.
//!
//! ── Why withdrawn ────────────────────────────────────────────────
//!
//! Dead-code detection without coverage data produces high
//! false-positive rates on framework-heavy codebases. The spike
//! `docs/spikes/2026-04-15-orient-on-repo-graph.md` found 86% of
//! symbols flagged as dead on repo-graph self-index — all noise.
//!
//! Reintroduction requires:
//!   1. Coverage integration (test coverage proves liveness), OR
//!   2. Entrypoint declaration coverage (explicit main/handler markers)
//!
//! Until then, the surface remains withdrawn.

use super::AggregatorOutput;
use crate::errors::AgentStorageError;
use crate::storage_port::{AgentDeadNode, AgentStorageRead, AgentTrustSummary};

/// Repo-level dead-code aggregator.
///
/// **Surface withdrawn.** Returns empty output unconditionally.
/// Internal substrate preserved for future reintroduction.
#[allow(unused_variables)]
pub fn aggregate<S: AgentStorageRead + ?Sized>(
	storage: &S,
	snapshot_uid: &str,
	repo_uid: &str,
	trust: &AgentTrustSummary,
) -> Result<AggregatorOutput, AgentStorageError> {
	// Surface withdrawn. No signals, no limits.
	// Internal dead-node queries and trust computation remain
	// available but are not projected to user-facing output.
	Ok(AggregatorOutput::empty())
}

/// File-scoped dead-code aggregator.
///
/// **Surface withdrawn.** Returns empty output unconditionally.
#[allow(unused_variables)]
pub fn aggregate_file<S: AgentStorageRead + ?Sized>(
	storage: &S,
	snapshot_uid: &str,
	repo_uid: &str,
	file_path: &str,
	trust: &AgentTrustSummary,
) -> Result<AggregatorOutput, AgentStorageError> {
	Ok(AggregatorOutput::empty())
}

/// Path-scoped dead-code aggregator.
///
/// **Surface withdrawn.** Returns empty output unconditionally.
#[allow(unused_variables)]
pub fn aggregate_path<S: AgentStorageRead + ?Sized>(
	storage: &S,
	snapshot_uid: &str,
	repo_uid: &str,
	path_prefix: &str,
	trust: &AgentTrustSummary,
) -> Result<AggregatorOutput, AgentStorageError> {
	Ok(AggregatorOutput::empty())
}

/// Stable sort that orders dead nodes by descending `line_count`
/// with `None` line counts pushed to the tail.
///
/// **Preserved substrate.** Not currently used (surface withdrawn),
/// but retained for future reintroduction. Tests verify correctness.
///
/// Sort key: `(size_bucket_desc, symbol_asc, file_asc)` where
/// `size_bucket_desc` maps `Some(n)` to `(0, MAX-n)` and `None`
/// to `(1, 0)` so None always follows any known size.
#[allow(dead_code)]
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
			is_test: false,
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
