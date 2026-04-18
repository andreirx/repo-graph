//! Integration-style tests for the binding-table loader /
//! validator. Complements the inline smoke tests in
//! `src/table.rs` with the full rejection-path coverage.

use repo_graph_state_bindings::{
	Basis, BindingTable, Direction, Language, ResourceKind, TableError,
};

/// Valid single-entry TOML used as the base for negative tests.
/// Each negative case perturbs one field.
fn probe_toml_with_notes(notes: &str) -> String {
	format!(
		r#"
[[binding]]
language      = "typescript"
module        = "@aws-sdk/client-s3"
symbol_path   = "PutObjectCommand"
resource_kind = "blob"
driver        = "s3"
direction     = "write"
basis         = "sdk_call"
notes         = "{}"
"#,
		notes
	)
}

// ── Positive paths ────────────────────────────────────────────────

#[test]
fn parses_minimal_synthetic_entry() {
	let toml = r#"
[[binding]]
language      = "typescript"
module        = "fs"
symbol_path   = "readFile"
resource_kind = "fs"
driver        = "node-fs"
direction     = "read"
basis         = "stdlib_api"
"#;
	let table = BindingTable::load_str(toml).expect("valid minimal entry must parse");
	assert_eq!(table.len(), 1);
	let e = &table.entries()[0];
	assert_eq!(e.language, Language::Typescript);
	assert_eq!(e.module, "fs");
	assert_eq!(e.symbol_path, "readFile");
	assert_eq!(e.resource_kind, ResourceKind::Fs);
	assert_eq!(e.driver, "node-fs");
	assert_eq!(e.direction, Direction::Read);
	assert_eq!(e.basis, Basis::StdlibApi);
	assert!(e.notes.is_none());
}

#[test]
fn parses_read_write_direction() {
	// `read_write` is explicit (contract §3.2); must round-trip.
	let toml = r#"
[[binding]]
language      = "typescript"
module        = "pg"
symbol_path   = "Client.query"
resource_kind = "db"
driver        = "postgres"
direction     = "read_write"
basis         = "sdk_call"
"#;
	let table = BindingTable::load_str(toml).unwrap();
	assert_eq!(table.entries()[0].direction, Direction::ReadWrite);
	assert_eq!(
		table.entries()[0].binding_key(),
		"typescript:pg:Client.query:read_write"
	);
}

#[test]
fn parses_empty_table() {
	// Zero `[[binding]]` entries is a legal degenerate case; the
	// loader returns an empty table, not an error.
	let table = BindingTable::load_str("").unwrap();
	assert!(table.is_empty());
}

#[test]
fn parses_every_language_variant() {
	let toml = r#"
[[binding]]
language      = "typescript"
module        = "a"
symbol_path   = "f"
resource_kind = "fs"
driver        = "x"
direction     = "read"
basis         = "stdlib_api"

[[binding]]
language      = "python"
module        = "a"
symbol_path   = "f"
resource_kind = "fs"
driver        = "x"
direction     = "read"
basis         = "stdlib_api"

[[binding]]
language      = "rust"
module        = "a"
symbol_path   = "f"
resource_kind = "fs"
driver        = "x"
direction     = "read"
basis         = "stdlib_api"

[[binding]]
language      = "java"
module        = "a"
symbol_path   = "f"
resource_kind = "fs"
driver        = "x"
direction     = "read"
basis         = "stdlib_api"

[[binding]]
language      = "cpp"
module        = "a"
symbol_path   = "f"
resource_kind = "fs"
driver        = "x"
direction     = "read"
basis         = "stdlib_api"
"#;
	let table = BindingTable::load_str(toml).unwrap();
	assert_eq!(table.len(), 5);
	let langs: Vec<Language> = table.entries().iter().map(|e| e.language).collect();
	assert_eq!(
		langs,
		vec![
			Language::Typescript,
			Language::Python,
			Language::Rust,
			Language::Java,
			Language::Cpp,
		]
	);
}

// ── Rejection paths ───────────────────────────────────────────────

#[test]
fn rejects_empty_module() {
	let toml = r#"
[[binding]]
language      = "typescript"
module        = ""
symbol_path   = "readFile"
resource_kind = "fs"
driver        = "node-fs"
direction     = "read"
basis         = "stdlib_api"
"#;
	let err = BindingTable::load_str(toml).unwrap_err();
	assert!(matches!(
		err,
		TableError::EmptyField { index: 0, field: "module" }
	));
}

#[test]
fn rejects_empty_symbol_path() {
	let toml = r#"
[[binding]]
language      = "typescript"
module        = "fs"
symbol_path   = ""
resource_kind = "fs"
driver        = "node-fs"
direction     = "read"
basis         = "stdlib_api"
"#;
	let err = BindingTable::load_str(toml).unwrap_err();
	assert!(matches!(
		err,
		TableError::EmptyField {
			index: 0,
			field: "symbol_path"
		}
	));
}

#[test]
fn rejects_empty_driver() {
	let toml = r#"
[[binding]]
language      = "typescript"
module        = "fs"
symbol_path   = "readFile"
resource_kind = "fs"
driver        = ""
direction     = "read"
basis         = "stdlib_api"
"#;
	let err = BindingTable::load_str(toml).unwrap_err();
	assert!(matches!(
		err,
		TableError::EmptyField { index: 0, field: "driver" }
	));
}

#[test]
fn rejects_duplicate_entry() {
	let toml = r#"
[[binding]]
language      = "typescript"
module        = "fs"
symbol_path   = "readFile"
resource_kind = "fs"
driver        = "node-fs"
direction     = "read"
basis         = "stdlib_api"

[[binding]]
language      = "typescript"
module        = "fs"
symbol_path   = "readFile"
resource_kind = "fs"
driver        = "different-driver"
direction     = "write"
basis         = "stdlib_api"
"#;
	let err = BindingTable::load_str(toml).unwrap_err();
	match err {
		TableError::Duplicate {
			language,
			module,
			symbol_path,
			first,
			second,
		} => {
			assert_eq!(language, Language::Typescript);
			assert_eq!(module, "fs");
			assert_eq!(symbol_path, "readFile");
			assert_eq!(first, 0);
			assert_eq!(second, 1);
		}
		other => panic!("expected Duplicate, got {:?}", other),
	}
}

#[test]
fn duplicate_across_languages_is_not_a_duplicate() {
	// Same (module, symbol_path) under two different languages
	// is legal — TS `fs.readFile` and Python `fs.readFile` (if
	// such a Python module existed) are distinct bindings.
	let toml = r#"
[[binding]]
language      = "typescript"
module        = "fs"
symbol_path   = "readFile"
resource_kind = "fs"
driver        = "node-fs"
direction     = "read"
basis         = "stdlib_api"

[[binding]]
language      = "python"
module        = "fs"
symbol_path   = "readFile"
resource_kind = "fs"
driver        = "node-fs"
direction     = "read"
basis         = "stdlib_api"
"#;
	let table = BindingTable::load_str(toml).unwrap();
	assert_eq!(table.len(), 2);
}

#[test]
fn rejects_notes_over_200_chars() {
	let notes = "x".repeat(201);
	let toml = probe_toml_with_notes(&notes);
	let err = BindingTable::load_str(&toml).unwrap_err();
	match err {
		TableError::NotesTooLong { index, actual, max } => {
			assert_eq!(index, 0);
			assert_eq!(actual, 201);
			assert_eq!(max, 200);
		}
		other => panic!("expected NotesTooLong, got {:?}", other),
	}
}

#[test]
fn accepts_notes_at_exactly_200_chars() {
	let notes = "x".repeat(200);
	let toml = probe_toml_with_notes(&notes);
	BindingTable::load_str(&toml).expect("200-char notes is exactly at the limit");
}

#[test]
fn rejects_resource_kind_config() {
	// Contract §9.2 reserves `config` for a future slice. Slice 1
	// MUST reject it.
	let toml = r#"
[[binding]]
language      = "typescript"
module        = "process"
symbol_path   = "env.DATABASE_URL"
resource_kind = "config"
driver        = "env"
direction     = "read"
basis         = "stdlib_api"
"#;
	// `config` is not a variant of `ResourceKind`, so serde
	// rejects it at the TOML deserialization layer as an unknown
	// variant. The loader surfaces that as a Parse error.
	let err = BindingTable::load_str(toml).unwrap_err();
	assert!(matches!(err, TableError::Parse(_)));
}

#[test]
fn rejects_unknown_resource_kind() {
	let toml = r#"
[[binding]]
language      = "typescript"
module        = "a"
symbol_path   = "b"
resource_kind = "queue"
driver        = "x"
direction     = "read"
basis         = "stdlib_api"
"#;
	let err = BindingTable::load_str(toml).unwrap_err();
	assert!(matches!(err, TableError::Parse(_)));
}

#[test]
fn rejects_unknown_language() {
	let toml = r#"
[[binding]]
language      = "golang"
module        = "a"
symbol_path   = "b"
resource_kind = "fs"
driver        = "x"
direction     = "read"
basis         = "stdlib_api"
"#;
	let err = BindingTable::load_str(toml).unwrap_err();
	assert!(matches!(err, TableError::Parse(_)));
}

#[test]
fn rejects_unknown_direction() {
	let toml = r#"
[[binding]]
language      = "typescript"
module        = "a"
symbol_path   = "b"
resource_kind = "fs"
driver        = "x"
direction     = "delete"
basis         = "stdlib_api"
"#;
	let err = BindingTable::load_str(toml).unwrap_err();
	assert!(matches!(err, TableError::Parse(_)));
}

#[test]
fn rejects_malformed_toml() {
	let err = BindingTable::load_str("this is not valid toml { ]]]").unwrap_err();
	assert!(matches!(err, TableError::Parse(_)));
}

// ── Whitespace rejection (P3-a) ───────────────────────────────────
//
// Authored configuration must be byte-clean. Surrounding whitespace
// on `module`, `symbol_path`, or `driver` is an authoring bug that
// would break Form-A matching downstream and cause surprising
// duplicate collisions. The loader rejects the whole file.

#[test]
fn rejects_module_with_surrounding_whitespace() {
	let leading = r#"
[[binding]]
language      = "typescript"
module        = " fs"
symbol_path   = "readFile"
resource_kind = "fs"
driver        = "node-fs"
direction     = "read"
basis         = "stdlib_api"
"#;
	assert!(matches!(
		BindingTable::load_str(leading).unwrap_err(),
		TableError::SurroundingWhitespace { index: 0, field: "module" }
	));

	let trailing = r#"
[[binding]]
language      = "typescript"
module        = "fs "
symbol_path   = "readFile"
resource_kind = "fs"
driver        = "node-fs"
direction     = "read"
basis         = "stdlib_api"
"#;
	assert!(matches!(
		BindingTable::load_str(trailing).unwrap_err(),
		TableError::SurroundingWhitespace { index: 0, field: "module" }
	));
}

#[test]
fn rejects_symbol_path_with_surrounding_whitespace() {
	let leading = r#"
[[binding]]
language      = "typescript"
module        = "fs"
symbol_path   = " readFile"
resource_kind = "fs"
driver        = "node-fs"
direction     = "read"
basis         = "stdlib_api"
"#;
	assert!(matches!(
		BindingTable::load_str(leading).unwrap_err(),
		TableError::SurroundingWhitespace {
			index: 0,
			field: "symbol_path"
		}
	));

	let trailing = r#"
[[binding]]
language      = "typescript"
module        = "fs"
symbol_path   = "readFile "
resource_kind = "fs"
driver        = "node-fs"
direction     = "read"
basis         = "stdlib_api"
"#;
	assert!(matches!(
		BindingTable::load_str(trailing).unwrap_err(),
		TableError::SurroundingWhitespace {
			index: 0,
			field: "symbol_path"
		}
	));
}

#[test]
fn rejects_driver_with_surrounding_whitespace() {
	let leading = r#"
[[binding]]
language      = "typescript"
module        = "fs"
symbol_path   = "readFile"
resource_kind = "fs"
driver        = " node-fs"
direction     = "read"
basis         = "stdlib_api"
"#;
	assert!(matches!(
		BindingTable::load_str(leading).unwrap_err(),
		TableError::SurroundingWhitespace { index: 0, field: "driver" }
	));

	let trailing = r#"
[[binding]]
language      = "typescript"
module        = "fs"
symbol_path   = "readFile"
resource_kind = "fs"
driver        = "node-fs "
direction     = "read"
basis         = "stdlib_api"
"#;
	assert!(matches!(
		BindingTable::load_str(trailing).unwrap_err(),
		TableError::SurroundingWhitespace { index: 0, field: "driver" }
	));
}

#[test]
fn whitespace_only_field_is_reported_as_empty_not_whitespace() {
	// Priority order: a pure-whitespace field is the author's
	// intent of emptiness; the clearer diagnostic is EmptyField,
	// not SurroundingWhitespace.
	let toml = r#"
[[binding]]
language      = "typescript"
module        = "   "
symbol_path   = "readFile"
resource_kind = "fs"
driver        = "node-fs"
direction     = "read"
basis         = "stdlib_api"
"#;
	assert!(matches!(
		BindingTable::load_str(toml).unwrap_err(),
		TableError::EmptyField { index: 0, field: "module" }
	));
}
