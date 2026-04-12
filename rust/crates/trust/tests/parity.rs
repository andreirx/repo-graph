//! Rust half of the trust parity harness.
//!
//! Walks `trust-parity-fixtures/` at the repo root, runs each
//! fixture's `input.json` through the corresponding Rust trust
//! function, and compares the actual output against the fixture's
//! `expected.json`.
//!
//! This is the Rust side of the R4-H cross-runtime parity check.
//! The TypeScript half runs the same fixtures through the TS trust
//! functions and compares against the same `expected.json`.
//!
//! **No normalization needed.** Trust computation is pure policy
//! with no generated UIDs, no timestamps, no randomness.
//!
//! **Two tiers:**
//!   - `rules__*` fixtures test individual rule functions.
//!   - `report__*` fixtures test the full trust report via
//!     `assemble_trust_report` with a mock `TrustStorageRead`,
//!     exercising the storage-backed assembly layer (JSON parsing,
//!     fetch composition) on the Rust side — the same boundary
//!     the TS side exercises via a mock `StoragePort`.

use std::fs;
use std::path::{Path, PathBuf};

use repo_graph_trust::service::{assemble_trust_report, TrustAssemblyError};
use repo_graph_trust::storage_port::{
	ClassificationCountRow, CountByClassificationInput, PathPrefixModuleCycle,
	QueryUnresolvedEdgesInput, TrustModuleStats, TrustStorageRead,
	TrustUnresolvedEdgeSample,
};
use repo_graph_trust::types::ReliabilityLevel;
use repo_graph_trust::{
	compute_call_graph_reliability, compute_change_impact_reliability,
	compute_dead_code_reliability, compute_import_graph_reliability,
	detect_alias_resolution_suspicion, detect_framework_heavy_suspicion,
	detect_missing_entrypoint_declarations, detect_registry_pattern_suspicion,
};
use serde_json::Value;
use walkdir::WalkDir;

// ── Fixture loading ──────────────────────────────────────────────

fn fixtures_root() -> PathBuf {
	let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
	manifest_dir
		.join("..")
		.join("..")
		.join("..")
		.join("trust-parity-fixtures")
}

struct Fixture {
	name: String,
	dir: PathBuf,
	input: Value,
	expected: Option<Value>,
}

fn discover_fixtures() -> Vec<Fixture> {
	let root = fixtures_root();
	let mut fixtures = Vec::new();
	for entry in WalkDir::new(&root)
		.min_depth(1)
		.max_depth(1)
		.sort_by_file_name()
	{
		let entry = entry.unwrap_or_else(|e| {
			panic!("failed to walk {}: {e}", root.display())
		});
		if !entry.file_type().is_dir() {
			continue;
		}
		let path = entry.path().to_path_buf();
		let name = path
			.file_name()
			.and_then(|n| n.to_str())
			.expect("fixture dir name utf-8")
			.to_string();
		let input_path = path.join("input.json");
		if !input_path.exists() {
			continue;
		}
		let input: Value = serde_json::from_str(
			&fs::read_to_string(&input_path)
				.unwrap_or_else(|e| panic!("read {}: {e}", input_path.display())),
		)
		.unwrap_or_else(|e| panic!("parse input.json for {name}: {e}"));

		let expected_path = path.join("expected.json");
		let expected = if expected_path.exists() {
			Some(
				serde_json::from_str(
					&fs::read_to_string(&expected_path)
						.unwrap_or_else(|e| panic!("read {}: {e}", expected_path.display())),
				)
				.unwrap_or_else(|e| panic!("parse expected.json for {name}: {e}")),
			)
		} else {
			None
		};

		fixtures.push(Fixture {
			name,
			dir: path,
			input,
			expected,
		});
	}
	fixtures
}

// ── Helpers ──────────────────────────────────────────────────────

fn get_field<T: serde::de::DeserializeOwned>(
	input: &Value,
	key: &str,
	fn_name: &str,
) -> Result<T, String> {
	let v = input
		.get(key)
		.ok_or_else(|| format!("{fn_name}: missing '{key}'"))?
		.clone();
	serde_json::from_value(v).map_err(|e| format!("{fn_name}.{key}: {e}"))
}

// ── Dispatch ─────────────────────────────────────────────────────

fn run_fixture(fixture: &Fixture) -> Result<Value, String> {
	let fn_name = fixture
		.input
		.get("fn")
		.and_then(|v| v.as_str())
		.ok_or_else(|| "input.json missing 'fn' field".to_string())?;

	match fn_name {
		"detect_framework_heavy_suspicion" => dispatch_detect_framework_heavy(&fixture.input),
		"detect_alias_resolution_suspicion" => dispatch_detect_alias_resolution(&fixture.input),
		"detect_missing_entrypoint_declarations" => dispatch_detect_missing_entrypoints(&fixture.input),
		"detect_registry_pattern_suspicion" => dispatch_detect_registry_pattern(&fixture.input),
		"compute_call_graph_reliability" => dispatch_compute_call_graph(&fixture.input),
		"compute_import_graph_reliability" => dispatch_compute_import_graph(&fixture.input),
		"compute_dead_code_reliability" => dispatch_compute_dead_code(&fixture.input),
		"compute_change_impact_reliability" => dispatch_compute_change_impact(&fixture.input),
		"compute_trust_report" => dispatch_assemble_trust_report(&fixture.input),
		other => Err(format!("unknown fn: {other}")),
	}
}

// ── Tier A: Rule dispatchers ─────────────────────────────────────

fn dispatch_detect_framework_heavy(input: &Value) -> Result<Value, String> {
	let file_paths: Vec<String> = get_field(input, "filePaths", "detect_framework_heavy")?;
	let result = detect_framework_heavy_suspicion(&file_paths);
	serde_json::to_value(&result).map_err(|e| e.to_string())
}

fn dispatch_detect_alias_resolution(input: &Value) -> Result<Value, String> {
	let count: usize = get_field(input, "suspiciousModuleCount", "detect_alias_resolution")?;
	let result = detect_alias_resolution_suspicion(count);
	serde_json::to_value(&result).map_err(|e| e.to_string())
}

fn dispatch_detect_missing_entrypoints(input: &Value) -> Result<Value, String> {
	let count: usize = get_field(input, "activeEntrypointCount", "detect_missing_entrypoints")?;
	let result = detect_missing_entrypoint_declarations(count);
	serde_json::to_value(&result).map_err(|e| e.to_string())
}

fn dispatch_detect_registry_pattern(input: &Value) -> Result<Value, String> {
	let by_ancestor: Vec<(String, usize)> =
		get_field(input, "pathPrefixCyclesByAncestor", "detect_registry_pattern")?;
	let total: usize = get_field(input, "pathPrefixCyclesTotal", "detect_registry_pattern")?;
	let result = detect_registry_pattern_suspicion(&by_ancestor, total);
	serde_json::to_value(&result).map_err(|e| e.to_string())
}

fn dispatch_compute_call_graph(input: &Value) -> Result<Value, String> {
	let resolved: u64 = get_field(input, "resolvedCalls", "compute_call_graph")?;
	let unresolved: u64 =
		get_field(input, "unresolvedCallsInternalLike", "compute_call_graph")?;
	let result = compute_call_graph_reliability(resolved, unresolved);
	serde_json::to_value(&result).map_err(|e| e.to_string())
}

fn dispatch_compute_import_graph(input: &Value) -> Result<Value, String> {
	let alias: bool = get_field(input, "aliasResolutionSuspicion", "compute_import_graph")?;
	let registry: bool =
		get_field(input, "registryPatternSuspicion", "compute_import_graph")?;
	let unresolved: u64 =
		get_field(input, "unresolvedImportsCount", "compute_import_graph")?;
	let result = compute_import_graph_reliability(alias, registry, unresolved);
	serde_json::to_value(&result).map_err(|e| e.to_string())
}

fn dispatch_compute_dead_code(input: &Value) -> Result<Value, String> {
	let missing_ep: bool =
		get_field(input, "missingEntrypointDeclarations", "compute_dead_code")?;
	let registry: bool =
		get_field(input, "registryPatternSuspicion", "compute_dead_code")?;
	let framework: bool =
		get_field(input, "frameworkHeavySuspicion", "compute_dead_code")?;
	let call_level: ReliabilityLevel =
		get_field(input, "callGraphLevel", "compute_dead_code")?;
	let result = compute_dead_code_reliability(missing_ep, registry, framework, call_level);
	serde_json::to_value(&result).map_err(|e| e.to_string())
}

fn dispatch_compute_change_impact(input: &Value) -> Result<Value, String> {
	let alias: bool =
		get_field(input, "aliasResolutionSuspicion", "compute_change_impact")?;
	let registry: bool =
		get_field(input, "registryPatternSuspicion", "compute_change_impact")?;
	let import_level: ReliabilityLevel =
		get_field(input, "importGraphLevel", "compute_change_impact")?;
	let result = compute_change_impact_reliability(alias, registry, import_level);
	serde_json::to_value(&result).map_err(|e| e.to_string())
}

// ── Tier B: Report dispatcher via assembly layer ─────────────────
//
// Uses `assemble_trust_report` with a mock `TrustStorageRead` so
// the Rust parity test exercises the same storage-backed assembly
// boundary the TS side exercises via mock `StoragePort`. This
// means JSON parsing of diagnostics/toolchain, fetch composition,
// and the full computation pipeline are all covered.

struct FixtureMockStorage {
	diagnostics_json: Option<String>,
	file_paths: Vec<String>,
	module_stats: Vec<TrustModuleStats>,
	path_prefix_cycles: Vec<PathPrefixModuleCycle>,
	active_entrypoint_count: usize,
	resolved_calls: u64,
	calls_classification_counts: Vec<ClassificationCountRow>,
	all_classification_counts: Vec<ClassificationCountRow>,
	unknown_calls_samples: Vec<TrustUnresolvedEdgeSample>,
}

impl TrustStorageRead for FixtureMockStorage {
	type Error = String;

	fn get_snapshot_extraction_diagnostics(
		&self,
		_snapshot_uid: &str,
	) -> Result<Option<String>, String> {
		Ok(self.diagnostics_json.clone())
	}

	fn count_edges_by_type(&self, _snapshot_uid: &str, _edge_type: &str) -> Result<u64, String> {
		Ok(self.resolved_calls)
	}

	fn count_active_declarations(&self, _repo_uid: &str, _kind: &str) -> Result<usize, String> {
		Ok(self.active_entrypoint_count)
	}

	fn count_unresolved_edges_by_classification(
		&self,
		input: &CountByClassificationInput,
	) -> Result<Vec<ClassificationCountRow>, String> {
		if input.filter_categories.is_empty() {
			Ok(self.all_classification_counts.clone())
		} else {
			Ok(self.calls_classification_counts.clone())
		}
	}

	fn query_unresolved_edges(
		&self,
		_input: &QueryUnresolvedEdgesInput,
	) -> Result<Vec<TrustUnresolvedEdgeSample>, String> {
		Ok(self.unknown_calls_samples.clone())
	}

	fn find_path_prefix_module_cycles(
		&self,
		_snapshot_uid: &str,
	) -> Result<Vec<PathPrefixModuleCycle>, String> {
		Ok(self.path_prefix_cycles.clone())
	}

	fn compute_module_stats(
		&self,
		_snapshot_uid: &str,
	) -> Result<Vec<TrustModuleStats>, String> {
		Ok(self.module_stats.clone())
	}

	fn get_file_paths_by_repo(&self, _repo_uid: &str) -> Result<Vec<String>, String> {
		Ok(self.file_paths.clone())
	}
}

fn dispatch_assemble_trust_report(input: &Value) -> Result<Value, String> {
	let snapshot_uid: String = get_field(input, "snapshotUid", "compute_trust_report")?;
	let basis_commit: Option<String> = get_field(input, "basisCommit", "compute_trust_report")?;

	// Serialize toolchain and diagnostics BACK to JSON strings,
	// matching what the real storage would return. This forces
	// assemble_trust_report to parse them, exercising the JSON
	// parsing path.
	let toolchain_json: Option<String> = input
		.get("toolchain")
		.and_then(|v| {
			if v.is_null() {
				None
			} else {
				Some(serde_json::to_string(v).unwrap())
			}
		});
	let diagnostics_json: Option<String> = input
		.get("diagnostics")
		.and_then(|v| {
			if v.is_null() {
				None
			} else {
				Some(serde_json::to_string(v).unwrap())
			}
		});

	let mock = FixtureMockStorage {
		diagnostics_json,
		file_paths: get_field(input, "filePaths", "compute_trust_report")?,
		module_stats: get_field(input, "moduleStats", "compute_trust_report")?,
		path_prefix_cycles: get_field(input, "pathPrefixCycles", "compute_trust_report")?,
		active_entrypoint_count: get_field(input, "activeEntrypointCount", "compute_trust_report")?,
		resolved_calls: get_field(input, "resolvedCalls", "compute_trust_report")?,
		calls_classification_counts: get_field(
			input,
			"callsClassificationCounts",
			"compute_trust_report",
		)?,
		all_classification_counts: get_field(
			input,
			"allClassificationCounts",
			"compute_trust_report",
		)?,
		unknown_calls_samples: get_field(
			input,
			"unknownCallsSamples",
			"compute_trust_report",
		)?,
	};

	let report = assemble_trust_report(
		&mock,
		"r1",
		&snapshot_uid,
		basis_commit.as_deref(),
		toolchain_json.as_deref(),
	)
	.map_err(|e: TrustAssemblyError<String>| format!("assembly error: {e}"))?;

	serde_json::to_value(&report).map_err(|e| e.to_string())
}

// ── Entry point ──────────────────────────────────────────────────

#[test]
fn parity_against_shared_trust_fixture_corpus() {
	let fixtures = discover_fixtures();
	assert!(
		!fixtures.is_empty(),
		"no fixtures found in trust-parity-fixtures/"
	);

	let update_mode = std::env::var("RGR_TRUST_PARITY_UPDATE").is_ok();

	let mut failures: Vec<String> = Vec::new();
	let mut generated: Vec<String> = Vec::new();

	for fixture in &fixtures {
		let actual = match run_fixture(fixture) {
			Ok(v) => v,
			Err(e) => {
				failures.push(format!("\n-- fixture: {} --\nerror: {}", fixture.name, e));
				continue;
			}
		};

		match &fixture.expected {
			Some(expected) => {
				if actual != *expected {
					let actual_pretty =
						serde_json::to_string_pretty(&actual).unwrap_or_default();
					let expected_pretty =
						serde_json::to_string_pretty(expected).unwrap_or_default();

					if update_mode {
						let path = fixture.dir.join("expected.json");
						fs::write(&path, format!("{actual_pretty}\n"))
							.unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
						generated.push(fixture.name.clone());
					} else {
						failures.push(format!(
							"\n-- fixture: {} --\nmismatch\n--- expected ---\n{expected_pretty}\n\n--- actual ---\n{actual_pretty}",
							fixture.name
						));
					}
				}
			}
			None => {
				let actual_pretty =
					serde_json::to_string_pretty(&actual).unwrap_or_default();
				let path = fixture.dir.join("expected.json");
				fs::write(&path, format!("{actual_pretty}\n"))
					.unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
				generated.push(fixture.name.clone());
			}
		}
	}

	if !generated.is_empty() {
		eprintln!(
			"\nGenerated expected.json for {} fixture(s): {:?}\nRe-run to verify.",
			generated.len(),
			generated
		);
	}

	if !failures.is_empty() {
		panic!(
			"\n{} of {} fixtures failed:\n{}\n\nTip: set RGR_TRUST_PARITY_UPDATE=1 to overwrite expected.json with actual output.",
			failures.len(),
			fixtures.len(),
			failures.join("\n")
		);
	}

	if !generated.is_empty() {
		panic!(
			"Generated {} expected.json file(s). Re-run to verify they match.",
			generated.len()
		);
	}
}
