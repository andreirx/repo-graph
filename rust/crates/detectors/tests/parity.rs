//! Rust parity test harness for the cross-runtime detector contract.
//!
//! Walks the shared `parity-fixtures/` corpus at the repo root,
//! runs each fixture's `input.<ext>` through the Rust detector
//! pipeline (`detect_env_accesses` + `detect_fs_mutations`), and
//! compares the output against the fixture's `expected.json`.
//!
//! This is the Rust half of the cross-runtime parity check. The
//! TypeScript half (R1-I) runs the same fixtures through the TS
//! pipeline and compares against the same `expected.json`. Both
//! runtimes consume the SAME bytes from the SAME files — both the
//! fixture corpus and the shared `detectors.toml` (embedded into
//! the Rust crate via `include_str!`). If one runtime passes and
//! the other fails on the same fixture, the detector contract has
//! drifted and the slice has broken parity.
//!
//! ── Contract notes ────────────────────────────────────────────
//!
//!   - Output shape: `{ env: [...], fs: [...] }` — both keys
//!     always present, arrays always present, nulls explicit.
//!     Enforced by the `ParityOutput` struct below: both fields
//!     are non-optional `Vec<T>`, so serde emits empty arrays
//!     rather than absent keys.
//!
//!   - Field order inside each record: Rust serde preserves struct
//!     field declaration order when serializing. `DetectedEnvDependency`
//!     and `DetectedFsMutation` declare their fields in the locked
//!     `expected.json` order (see `src/types.rs` docstrings). The
//!     parity harness uses structural comparison via
//!     `serde_json::Value`, so field order does not affect the pass/
//!     fail verdict, but the declaration order is maintained as a
//!     parity guardrail regardless.
//!
//!   - `destinationPath` normalization: NOT needed here. The Rust
//!     `DetectedFsMutation.destination_path: Option<String>` always
//!     serializes to either a string value or JSON `null` — never
//!     absent. This is the contract target, so no harness-side
//!     fill-in is required. The TS harness (R1-I) does fill in
//!     `destinationPath: null` because the TS interface declares
//!     the field optional and TS `JSON.stringify` omits undefined
//!     fields. The asymmetry is intentional: R1-J is the reference
//!     shape, R1-I normalizes to it.
//!
//!   - `filePath` in expected.json is the fixture-local filename
//!     (e.g., `input.ts`). The harness passes the same basename to
//!     the detector so the reported filePath matches.
//!
//! ── Test shape ────────────────────────────────────────────────
//!
//! A single `#[test]` function iterates the fixture corpus and
//! collects per-fixture failures, panicking at the end with the
//! full failure list. This is coarser than the TS harness (which
//! emits one vitest `it(...)` per fixture), but Rust integration
//! tests do not support data-driven per-fixture test naming
//! without external crates like `test_case` / `rstest`. The slice
//! constraint is "no new external crates", so the single-test-
//! with-collected-failures shape is the chosen compromise. Each
//! failure entry is tagged with the fixture name plus a pretty-
//! printed diff so acceptance diagnostics are still isolable to a
//! single dimension.

use std::fs;
use std::path::{Path, PathBuf};

use repo_graph_detectors::types::{DetectedEnvDependency, DetectedFsMutation};
use repo_graph_detectors::{detect_env_accesses, detect_fs_mutations};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

// ── Parity boundary output shape ───────────────────────────────────

/// The parity boundary output shape. Matches the locked
/// `expected.json` contract and the TS `ParityOutput` interface in
/// `test/parity/parity.test.ts`.
///
/// Both fields are non-optional `Vec<T>`, so serde always emits
/// both keys with at least an empty array. This directly enforces
/// the "both keys always present, arrays always present" rule of
/// the locked contract.
///
/// Field declaration order is `env` then `fs`, matching the locked
/// expected.json top-level key order.
#[derive(Debug, Serialize, Deserialize)]
struct ParityOutput {
	env: Vec<DetectedEnvDependency>,
	fs: Vec<DetectedFsMutation>,
}

// ── Fixture discovery ─────────────────────────────────────────────

/// Locate the repo-root `parity-fixtures/` directory from this
/// crate's Cargo manifest directory.
///
/// The crate lives at `rust/crates/detectors/`, so three `..`
/// hops reach the repo root. `CARGO_MANIFEST_DIR` is injected by
/// cargo at compile time and is always an absolute path to the
/// crate directory — resolution is environment-independent and
/// does not depend on the current working directory at test time.
fn fixtures_root() -> PathBuf {
	let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
	manifest_dir
		.join("..")
		.join("..")
		.join("..")
		.join("parity-fixtures")
}

/// One loaded fixture: identity, input bytes, and parsed expected.
struct Fixture {
	/// Fixture directory name (e.g., `record__ts__process_env_dot`).
	/// Used for failure reporting.
	name: String,
	/// Fixture-local basename of the input file (e.g., `input.ts`).
	/// Passed to the detector so the emitted `filePath` matches
	/// the locked `expected.json` value.
	input_file_name: String,
	/// Raw source bytes of the input file.
	input_content: String,
	/// Parsed `expected.json` as a structural JSON value. Parsed
	/// once per fixture, compared structurally against the actual
	/// pipeline output on each test run.
	expected: serde_json::Value,
}

/// Walk `parity-fixtures/` and load every fixture directly below
/// it. A fixture is a direct child directory containing both an
/// `input.<ext>` file and an `expected.json` file.
///
/// Uses `walkdir` with depth bounds `[1, 1]` so only the direct
/// children of the root are visited — the fixture corpus is flat,
/// so deeper traversal is not required. `sort_by_file_name()`
/// makes the iteration order deterministic across platforms and
/// filesystems, matching the sorted ordering used by the TS
/// harness (`fixtures.sort((a, b) => a.name.localeCompare(b.name))`).
///
/// Discovery errors panic with a descriptive message. A fixture
/// corpus that does not parse is a build-time bug, not a runtime
/// condition.
fn discover_fixtures() -> Vec<Fixture> {
	let root = fixtures_root();
	let mut fixtures = Vec::new();
	for entry in WalkDir::new(&root)
		.min_depth(1)
		.max_depth(1)
		.sort_by_file_name()
		.into_iter()
	{
		let entry = entry.unwrap_or_else(|e| {
			panic!("failed to walk fixtures root {}: {e}", root.display())
		});
		if !entry.file_type().is_dir() {
			continue;
		}
		let path = entry.path();
		let name = path
			.file_name()
			.and_then(|n| n.to_str())
			.unwrap_or_else(|| {
				panic!(
					"fixture directory name is not valid UTF-8: {}",
					path.display()
				)
			})
			.to_string();
		// Find the fixture-local input.<ext> file. Scanning the
		// direct children is sufficient; the fixture layout has
		// exactly one input.<ext> plus expected.json per directory.
		let mut input_file_name: Option<String> = None;
		for sub in fs::read_dir(path).unwrap_or_else(|e| {
			panic!("failed to read fixture dir {name}: {e}")
		}) {
			let sub = sub.unwrap_or_else(|e| {
				panic!("failed to read sub-entry of {name}: {e}")
			});
			let sub_name = sub.file_name();
			let sub_name = sub_name.to_str().unwrap_or_else(|| {
				panic!(
					"fixture file name is not valid UTF-8 in {name}: {:?}",
					sub.file_name()
				)
			});
			if sub_name.starts_with("input.") {
				input_file_name = Some(sub_name.to_string());
				break;
			}
		}
		let input_file_name = input_file_name.unwrap_or_else(|| {
			panic!("fixture {name} has no input.<ext> file")
		});
		let input_content = fs::read_to_string(path.join(&input_file_name))
			.unwrap_or_else(|e| panic!("failed to read input for {name}: {e}"));
		let expected_raw = fs::read_to_string(path.join("expected.json"))
			.unwrap_or_else(|e| panic!("failed to read expected.json for {name}: {e}"));
		let expected: serde_json::Value = serde_json::from_str(&expected_raw)
			.unwrap_or_else(|e| panic!("failed to parse expected.json for {name}: {e}"));
		fixtures.push(Fixture {
			name,
			input_file_name,
			input_content,
			expected,
		});
	}
	fixtures
}

// ── Harness ────────────────────────────────────────────────────────

/// Rust-side parity test: runs every fixture in `parity-fixtures/`
/// through the production Rust detector pipeline and asserts the
/// structural output matches the fixture's `expected.json`.
///
/// Flow:
///   1. Discover fixtures (sorted by directory name).
///   2. Assert the corpus is non-empty (sanity check mirror of the
///      TS harness's "discovered at least one fixture" it-block).
///   3. For each fixture: run both detectors, build a
///      `ParityOutput` struct, serialize to `serde_json::Value`,
///      compare structurally against `expected`.
///   4. Collect per-fixture failures and panic at the end with
///      the full list.
///
/// Failure diagnostics: each failed fixture contributes a block
/// with the fixture name, pretty-printed actual JSON, and pretty-
/// printed expected JSON. Structural diff is not implemented
/// (would require an external crate); reading the two side by
/// side identifies the drift precisely enough for acceptance.
#[test]
fn parity_against_shared_fixture_corpus() {
	let fixtures = discover_fixtures();

	// Mirror of the TS harness "discovered at least one fixture"
	// sanity check. A zero-fixture corpus almost certainly means
	// the fixtures_root() path is wrong (e.g., after a directory
	// reorganization), so fail loudly rather than silently passing.
	assert!(
		!fixtures.is_empty(),
		"parity fixture corpus is empty at {} — expected at least one fixture",
		fixtures_root().display()
	);

	let mut failures: Vec<String> = Vec::new();
	for fixture in &fixtures {
		let env = detect_env_accesses(&fixture.input_content, &fixture.input_file_name);
		let fs_records = detect_fs_mutations(&fixture.input_content, &fixture.input_file_name);
		let actual_struct = ParityOutput {
			env,
			fs: fs_records,
		};
		let actual_json = serde_json::to_value(&actual_struct)
			.expect("ParityOutput should serialize to JSON");
		if actual_json != fixture.expected {
			let actual_pretty = serde_json::to_string_pretty(&actual_json)
				.unwrap_or_else(|_| "<unserializable>".to_string());
			let expected_pretty = serde_json::to_string_pretty(&fixture.expected)
				.unwrap_or_else(|_| "<unserializable>".to_string());
			failures.push(format!(
				"\n── fixture: {} ──\n--- actual ---\n{actual_pretty}\n--- expected ---\n{expected_pretty}",
				fixture.name
			));
		}
	}

	if !failures.is_empty() {
		panic!(
			"{} of {} parity fixtures failed:\n{}",
			failures.len(),
			fixtures.len(),
			failures.join("\n")
		);
	}
}
