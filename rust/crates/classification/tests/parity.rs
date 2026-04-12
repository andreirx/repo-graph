//! Rust half of the classification parity harness.
//!
//! Walks `classification-parity-fixtures/` at the repo root, runs
//! each fixture's `input.json` through the corresponding Rust
//! classification function, and compares the actual output against
//! the fixture's `expected.json`.
//!
//! This is the Rust side of the R3-G cross-runtime parity check.
//! The TypeScript half runs the same fixtures through the TS
//! classification functions and compares against the same
//! `expected.json`.
//!
//! **No normalization needed.** Classification is pure policy with
//! no generated UIDs, no timestamps, no randomness. For the same
//! input, both runtimes must produce byte-equal JSON output.

use std::fs;
use std::path::{Path, PathBuf};

use repo_graph_classification::types::*;
use repo_graph_classification::*;
use serde_json::Value;
use walkdir::WalkDir;

// ── Fixture loading ──────────────────────────────────────────────

fn fixtures_root() -> PathBuf {
	let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
	manifest_dir
		.join("..")
		.join("..")
		.join("..")
		.join("classification-parity-fixtures")
}

struct Fixture {
	name: String,
	input: Value,
	expected: Value,
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
		let path = entry.path();
		let name = path
			.file_name()
			.and_then(|n| n.to_str())
			.expect("fixture dir name utf-8")
			.to_string();
		let input_path = path.join("input.json");
		let expected_path = path.join("expected.json");
		if !input_path.exists() || !expected_path.exists() {
			continue;
		}
		let input: Value = serde_json::from_str(
			&fs::read_to_string(&input_path)
				.unwrap_or_else(|e| panic!("read {}: {e}", input_path.display())),
		)
		.unwrap_or_else(|e| panic!("parse input.json for {name}: {e}"));
		let expected: Value = serde_json::from_str(
			&fs::read_to_string(&expected_path)
				.unwrap_or_else(|e| panic!("read {}: {e}", expected_path.display())),
		)
		.unwrap_or_else(|e| panic!("parse expected.json for {name}: {e}"));
		fixtures.push(Fixture {
			name,
			input,
			expected,
		});
	}
	fixtures
}

// ── Dispatch ─────────────────────────────────────────────────────

fn run_fixture(fixture: &Fixture) -> Result<(), String> {
	let fn_name = fixture
		.input
		.get("fn")
		.and_then(|v| v.as_str())
		.ok_or_else(|| "input.json missing 'fn' field".to_string())?;

	let actual = match fn_name {
		"classify_unresolved_edge" => dispatch_classify(&fixture.input)?,
		"derive_blast_radius" => dispatch_blast_radius(&fixture.input)?,
		"compute_matcher_key" => dispatch_compute_matcher_key(&fixture.input)?,
		"match_boundary_facts" => dispatch_match_boundary_facts(&fixture.input)?,
		"detect_framework_boundary" => dispatch_framework_boundary(&fixture.input)?,
		"detect_lambda_entrypoints" => dispatch_lambda_entrypoints(&fixture.input)?,
		other => return Err(format!("unknown fn: {other}")),
	};

	if actual == fixture.expected {
		Ok(())
	} else {
		let actual_pretty = serde_json::to_string_pretty(&actual).unwrap_or_default();
		let expected_pretty =
			serde_json::to_string_pretty(&fixture.expected).unwrap_or_default();

		if std::env::var("RGR_CLASSIFICATION_PARITY_EMIT_ACTUAL").is_ok() {
			eprintln!("=== ACTUAL for fixture '{}' ===", fixture.name);
			eprintln!("{}", actual_pretty);
			eprintln!("=== END ACTUAL ===");
		}

		Err(format!(
			"mismatch\n--- expected ---\n{expected_pretty}\n\n--- actual ---\n{actual_pretty}"
		))
	}
}

// ── Per-function dispatchers ─────────────────────────────────────

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

fn dispatch_classify(input: &Value) -> Result<Value, String> {
	let edge: ClassifierEdgeInput = get_field(input, "edge", "classify_unresolved_edge")?;
	let category: UnresolvedEdgeCategory =
		get_field(input, "category", "classify_unresolved_edge")?;
	let snapshot_signals: SnapshotSignals =
		get_field(input, "snapshotSignals", "classify_unresolved_edge")?;
	let file_signals: FileSignals =
		get_field(input, "fileSignals", "classify_unresolved_edge")?;
	let verdict = classify_unresolved_edge(&edge, category, &snapshot_signals, &file_signals);
	serde_json::to_value(&verdict).map_err(|e| e.to_string())
}

fn dispatch_blast_radius(input: &Value) -> Result<Value, String> {
	let category: UnresolvedEdgeCategory =
		get_field(input, "category", "derive_blast_radius")?;
	let basis_code: UnresolvedEdgeBasisCode =
		get_field(input, "basisCode", "derive_blast_radius")?;
	let visibility: Option<String> =
		get_field(input, "sourceNodeVisibility", "derive_blast_radius")?;
	let result = derive_blast_radius(category, basis_code, visibility.as_deref());
	serde_json::to_value(&result).map_err(|e| e.to_string())
}

fn dispatch_compute_matcher_key(input: &Value) -> Result<Value, String> {
	let mechanism: BoundaryMechanism =
		get_field(input, "mechanism", "compute_matcher_key")?;
	let address: String = get_field(input, "address", "compute_matcher_key")?;
	let metadata: serde_json::Map<String, Value> =
		get_field(input, "metadata", "compute_matcher_key")?;
	let result = compute_matcher_key(mechanism, &address, &metadata);
	Ok(serde_json::to_value(&result).unwrap())
}

fn dispatch_match_boundary_facts(input: &Value) -> Result<Value, String> {
	let providers: Vec<MatchableProviderFact> =
		get_field(input, "providers", "match_boundary_facts")?;
	let consumers: Vec<MatchableConsumerFact> =
		get_field(input, "consumers", "match_boundary_facts")?;
	let result = match_boundary_facts(&providers, &consumers);
	serde_json::to_value(&result).map_err(|e| e.to_string())
}

fn dispatch_framework_boundary(input: &Value) -> Result<Value, String> {
	let target_key: String =
		get_field(input, "targetKey", "detect_framework_boundary")?;
	let category: UnresolvedEdgeCategory =
		get_field(input, "category", "detect_framework_boundary")?;
	let import_bindings: Vec<ImportBinding> =
		get_field(input, "importBindings", "detect_framework_boundary")?;
	let result = detect_framework_boundary(&target_key, category, &import_bindings);
	Ok(serde_json::to_value(&result).unwrap())
}

fn dispatch_lambda_entrypoints(input: &Value) -> Result<Value, String> {
	let import_bindings: Vec<ImportBinding> =
		get_field(input, "importBindings", "detect_lambda_entrypoints")?;
	let exported_symbols: Vec<ExportedSymbol> =
		get_field(input, "exportedSymbols", "detect_lambda_entrypoints")?;
	let result = detect_lambda_entrypoints(&import_bindings, &exported_symbols);
	serde_json::to_value(&result).map_err(|e| e.to_string())
}

// ── Entry point ──────────────────────────────────────────────────

#[test]
fn parity_against_shared_classification_fixture_corpus() {
	let fixtures = discover_fixtures();
	assert!(
		!fixtures.is_empty(),
		"no fixtures found in classification-parity-fixtures/"
	);

	let mut failures: Vec<String> = Vec::new();
	for fixture in &fixtures {
		match run_fixture(fixture) {
			Ok(()) => {}
			Err(e) => failures.push(format!("\n── fixture: {} ──\n{}", fixture.name, e)),
		}
	}

	if !failures.is_empty() {
		panic!(
			"\n{} of {} fixtures failed:\n{}\n\nTip: set RGR_CLASSIFICATION_PARITY_EMIT_ACTUAL=1 to print the actual JSON.",
			failures.len(),
			fixtures.len(),
			failures.join("\n")
		);
	}
}
