//! Public detector pipeline — eager-loaded production graph plus
//! the drop-in replacement for the TS `detectEnvAccesses` /
//! `detectFsMutations` public API.
//!
//! This module is the parity boundary. Callers get the same
//! output shape and the same byte-level facts as the TS pipeline
//! for every fixture in `parity-fixtures/`.
//!
//! Pipeline composition (mirror of the TS wrappers):
//!
//!   1. Map file path extension → `DetectorLanguage` (or return
//!      empty if the extension is not recognized)
//!   2. Mask comments via `comment_masker::mask_comments_for_file`
//!   3. Dispatch to the generic walker with the production graph
//!
//! Production graph loading:
//!
//! The detector graph source is the shared `detectors.toml` file
//! at the TS repository root
//! (`src/core/seams/detectors/detectors.toml`). The Rust crate
//! embeds the file bytes at compile time via `include_str!`, then
//! parses and validates them lazily on first use via `LazyLock`.
//! This mirrors the TS "eager at module import" semantics closely
//! enough: the first call to either detector function triggers
//! the load and panics if the TOML is malformed.
//!
//! The `include_str!` path crosses from the Rust crate into the
//! TS source tree. This is the intentional cross-runtime contract
//! surface — both runtimes consume the SAME bytes from the SAME
//! file. Any change to `detectors.toml` immediately affects both
//! runtimes without risk of stale copies.

use std::sync::LazyLock;

use crate::comment_masker::mask_comments_for_file;
use crate::hooks::create_default_hook_registry;
use crate::loader::load_detector_graph;
use crate::types::{
	DetectedEnvDependency, DetectedFsMutation, DetectorLanguage, HookRegistry,
	LoadedDetectorGraph,
};
use crate::walker::{walk_env_detectors, walk_fs_detectors};

// ── Production graph source ───────────────────────────────────────

/// The raw TOML bytes of the production detector graph, embedded
/// at compile time from the shared file at the repo root.
///
/// The path is resolved at compile time relative to this source
/// file. If the TS side moves or renames `detectors.toml`, the
/// Rust crate fails to build — which is the intended fail-fast
/// signal for cross-runtime contract drift.
const DETECTORS_TOML: &str = include_str!(
	"../../../../src/core/seams/detectors/detectors.toml"
);

// ── Eager-loaded singleton ────────────────────────────────────────

/// The production detector pipeline: a loaded graph plus a hook
/// registry, both populated from the embedded `detectors.toml` and
/// the default hook registry respectively.
pub struct ProductionPipeline {
	pub graph: LoadedDetectorGraph,
	pub registry: HookRegistry,
}

/// Lazy-initialized singleton holding the production pipeline.
///
/// The first dereference triggers:
///   1. `create_default_hook_registry()` to build the hook set
///   2. `load_detector_graph` to parse and validate the embedded
///      TOML against that registry
///
/// Any validation failure panics. This is intentional: a malformed
/// `detectors.toml` is a build-time / test-time bug, not a
/// runtime-recoverable error.
///
/// `LazyLock` is stable since Rust 1.80 and is the canonical way
/// to express "run this initialization exactly once, thread-safe,
/// on first access".
static PRODUCTION_PIPELINE: LazyLock<ProductionPipeline> = LazyLock::new(|| {
	let registry = create_default_hook_registry();
	let graph = load_detector_graph(DETECTORS_TOML, &registry)
		.expect("production detectors.toml should parse and validate against the default hook registry");
	ProductionPipeline { graph, registry }
});

/// Access the production pipeline singleton.
///
/// Exposed for tests and advanced callers that need the graph or
/// registry directly. Normal callers should use
/// `detect_env_accesses` / `detect_fs_mutations` instead.
pub fn production_pipeline() -> &'static ProductionPipeline {
	&PRODUCTION_PIPELINE
}

// ── Public detector API ───────────────────────────────────────────

/// Detect environment variable accesses in a source file.
///
/// Rust mirror of the TS `detectEnvAccesses(content, filePath)`
/// public function. Behavior:
///
///   1. Determine `DetectorLanguage` from `file_path` extension.
///      Unknown extensions return an empty Vec (no detection).
///   2. Mask comments via `mask_comments_for_file` (preserves
///      newlines and string literals, blanks comment content).
///   3. Dispatch to `walk_env_detectors` with the production graph
///      and registry.
///
/// Byte-identical to the TS public function against every fixture
/// in `parity-fixtures/`.
pub fn detect_env_accesses(
	content: &str,
	file_path: &str,
) -> Vec<DetectedEnvDependency> {
	let language = match language_from_extension(file_path) {
		Some(l) => l,
		None => return Vec::new(),
	};
	let masked = mask_comments_for_file(file_path, content);
	let pipeline = production_pipeline();
	walk_env_detectors(
		&pipeline.graph,
		&pipeline.registry,
		language,
		&masked,
		file_path,
	)
}

/// Detect filesystem mutation occurrences in a source file.
///
/// Rust mirror of the TS `detectFsMutations(content, filePath)`
/// public function. Composition is identical to
/// `detect_env_accesses`: extension → language, comment mask,
/// walker dispatch.
pub fn detect_fs_mutations(
	content: &str,
	file_path: &str,
) -> Vec<DetectedFsMutation> {
	let language = match language_from_extension(file_path) {
		Some(l) => l,
		None => return Vec::new(),
	};
	let masked = mask_comments_for_file(file_path, content);
	let pipeline = production_pipeline();
	walk_fs_detectors(
		&pipeline.graph,
		&pipeline.registry,
		language,
		&masked,
		file_path,
	)
}

// ── Language dispatch ─────────────────────────────────────────────

/// Map a file extension to its `DetectorLanguage` bucket.
///
/// Mirror of the TS `languageFromExtension` helper in
/// `env-detectors.ts` / `fs-mutation-detectors.ts`:
///
///   - `.ts` / `.tsx` / `.js` / `.jsx` → `Ts` (the JS/TS family)
///   - `.py`                           → `Py`
///   - `.rs`                           → `Rs`
///   - `.java`                         → `Java`
///   - `.c` / `.h` / `.cpp` / `.hpp` / `.cc` / `.cxx` → `C`
///   - anything else                   → `None` (no detection)
///
/// Extension comparison is case-sensitive, matching the legacy
/// behavior exactly. Files with no extension or unknown extensions
/// return `None` and the walker is never called.
fn language_from_extension(file_path: &str) -> Option<DetectorLanguage> {
	let dot = file_path.rfind('.')?;
	let ext = &file_path[dot..];
	match ext {
		".ts" | ".tsx" | ".js" | ".jsx" => Some(DetectorLanguage::Ts),
		".py" => Some(DetectorLanguage::Py),
		".rs" => Some(DetectorLanguage::Rs),
		".java" => Some(DetectorLanguage::Java),
		".c" | ".h" | ".cpp" | ".hpp" | ".cc" | ".cxx" => Some(DetectorLanguage::C),
		_ => None,
	}
}

#[cfg(test)]
mod extension_tests {
	use super::*;

	#[test]
	fn ts_family_extensions_map_to_ts() {
		assert_eq!(language_from_extension("src/a.ts"), Some(DetectorLanguage::Ts));
		assert_eq!(language_from_extension("src/a.tsx"), Some(DetectorLanguage::Ts));
		assert_eq!(language_from_extension("src/a.js"), Some(DetectorLanguage::Ts));
		assert_eq!(language_from_extension("src/a.jsx"), Some(DetectorLanguage::Ts));
	}

	#[test]
	fn python_extension_maps_to_py() {
		assert_eq!(language_from_extension("src/a.py"), Some(DetectorLanguage::Py));
	}

	#[test]
	fn rust_extension_maps_to_rs() {
		assert_eq!(language_from_extension("src/a.rs"), Some(DetectorLanguage::Rs));
	}

	#[test]
	fn java_extension_maps_to_java() {
		assert_eq!(
			language_from_extension("src/A.java"),
			Some(DetectorLanguage::Java)
		);
	}

	#[test]
	fn c_cpp_extensions_map_to_c() {
		for ext in [".c", ".h", ".cpp", ".hpp", ".cc", ".cxx"] {
			assert_eq!(
				language_from_extension(&format!("src/a{ext}")),
				Some(DetectorLanguage::C),
				"failed for extension {ext}"
			);
		}
	}

	#[test]
	fn unknown_extension_returns_none() {
		assert_eq!(language_from_extension("src/a.rb"), None);
		assert_eq!(language_from_extension("src/a.go"), None);
		assert_eq!(language_from_extension("src/a.txt"), None);
	}

	#[test]
	fn no_extension_returns_none() {
		assert_eq!(language_from_extension("Makefile"), None);
		assert_eq!(language_from_extension("src/README"), None);
	}

	#[test]
	fn uppercase_extensions_are_not_recognized() {
		// Case-sensitive by design, matching TS.
		assert_eq!(language_from_extension("src/a.TS"), None);
		assert_eq!(language_from_extension("src/a.PY"), None);
	}

	#[test]
	fn extension_is_last_dot_segment() {
		assert_eq!(language_from_extension("a.b.c.ts"), Some(DetectorLanguage::Ts));
		assert_eq!(language_from_extension("src/foo.bar.rs"), Some(DetectorLanguage::Rs));
	}
}
