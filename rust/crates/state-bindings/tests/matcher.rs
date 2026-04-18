//! Form-A matcher integration tests. Complements the inline smoke
//! tests in `src/matcher.rs` with cross-cutting cases.

use repo_graph_state_bindings::{
	match_form_a, BindingTable, CalleePath, ImportView, Language,
};

// Small two-entry synthetic table used to exercise match vs.
// no-match paths with deterministic inputs. Using a synthetic
// table instead of the probe keeps this file robust to future
// changes in `bindings.toml` scope.
fn synthetic_table() -> BindingTable {
	BindingTable::load_str(
		r#"
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
module        = "@aws-sdk/client-s3"
symbol_path   = "PutObjectCommand"
resource_kind = "blob"
driver        = "s3"
direction     = "write"
basis         = "sdk_call"
"#,
	)
	.expect("synthetic table must parse")
}

#[test]
fn matches_stdlib_entry_when_import_and_callee_align() {
	let table = synthetic_table();
	let imports = vec![ImportView {
		module_path: "fs".to_string(),
		imported_symbol: "readFile".to_string(),
		import_alias: None,
	}];
	let callee = CalleePath {
		resolved_module: Some("fs".to_string()),
		resolved_symbol: "readFile".to_string(),
	};
	let m = match_form_a(&imports, &callee, &table, Language::Typescript)
		.expect("must match fs.readFile");
	assert_eq!(m.binding.driver, "node-fs");
	assert_eq!(m.binding.module, "fs");
	assert_eq!(m.binding_key, "typescript:fs:readFile:read");
}

#[test]
fn matches_sdk_entry_when_both_conditions_hold() {
	let table = synthetic_table();
	let imports = vec![ImportView {
		module_path: "@aws-sdk/client-s3".to_string(),
		imported_symbol: "PutObjectCommand".to_string(),
		import_alias: None,
	}];
	let callee = CalleePath {
		resolved_module: Some("@aws-sdk/client-s3".to_string()),
		resolved_symbol: "PutObjectCommand".to_string(),
	};
	let m = match_form_a(&imports, &callee, &table, Language::Typescript)
		.expect("must match PutObjectCommand");
	assert_eq!(m.binding.driver, "s3");
	assert_eq!(
		m.binding_key,
		"typescript:@aws-sdk/client-s3:PutObjectCommand:write"
	);
}

#[test]
fn no_match_when_import_absent() {
	let table = synthetic_table();
	let imports: Vec<ImportView> = vec![];
	let callee = CalleePath {
		resolved_module: Some("fs".to_string()),
		resolved_symbol: "readFile".to_string(),
	};
	assert!(match_form_a(&imports, &callee, &table, Language::Typescript).is_none());
}

#[test]
fn no_match_when_callee_module_differs() {
	let table = synthetic_table();
	let imports = vec![ImportView {
		module_path: "some-other-module".to_string(),
		imported_symbol: "readFile".to_string(),
		import_alias: None,
	}];
	let callee = CalleePath {
		resolved_module: Some("some-other-module".to_string()),
		resolved_symbol: "readFile".to_string(),
	};
	assert!(match_form_a(&imports, &callee, &table, Language::Typescript).is_none());
}

#[test]
fn no_match_when_symbol_differs() {
	let table = synthetic_table();
	let imports = vec![ImportView {
		module_path: "fs".to_string(),
		imported_symbol: "writeFile".to_string(),
		import_alias: None,
	}];
	let callee = CalleePath {
		resolved_module: Some("fs".to_string()),
		resolved_symbol: "writeFile".to_string(),
	};
	// Table only has `readFile`, not `writeFile`.
	assert!(match_form_a(&imports, &callee, &table, Language::Typescript).is_none());
}

#[test]
fn no_match_across_languages() {
	// Same module + symbol, but the bindings are TS-only. A
	// Python source file must NOT match a TypeScript binding.
	let table = synthetic_table();
	let imports = vec![ImportView {
		module_path: "fs".to_string(),
		imported_symbol: "readFile".to_string(),
		import_alias: None,
	}];
	let callee = CalleePath {
		resolved_module: Some("fs".to_string()),
		resolved_symbol: "readFile".to_string(),
	};
	assert!(match_form_a(&imports, &callee, &table, Language::Python).is_none());
}

#[test]
fn no_match_when_callee_not_resolved_to_module() {
	let table = synthetic_table();
	let imports = vec![ImportView {
		module_path: "fs".to_string(),
		imported_symbol: "readFile".to_string(),
		import_alias: None,
	}];
	let callee = CalleePath {
		resolved_module: None,
		resolved_symbol: "readFile".to_string(),
	};
	assert!(match_form_a(&imports, &callee, &table, Language::Typescript).is_none());
}

#[test]
fn match_survives_extra_unrelated_imports() {
	let table = synthetic_table();
	let imports = vec![
		ImportView {
			module_path: "lodash".to_string(),
			imported_symbol: "map".to_string(),
			import_alias: None,
		},
		ImportView {
			module_path: "fs".to_string(),
			imported_symbol: "readFile".to_string(),
			import_alias: None,
		},
		ImportView {
			module_path: "path".to_string(),
			imported_symbol: "join".to_string(),
			import_alias: None,
		},
	];
	let callee = CalleePath {
		resolved_module: Some("fs".to_string()),
		resolved_symbol: "readFile".to_string(),
	};
	let m = match_form_a(&imports, &callee, &table, Language::Typescript)
		.expect("must match despite unrelated imports");
	assert_eq!(m.binding.module, "fs");
}

#[test]
fn aliased_import_is_handled_upstream() {
	// The matcher does not walk aliases. The extractor is
	// responsible for resolving `import { readFile as rf }` and
	// producing a CalleePath with the ORIGINAL symbol name.
	// This test documents that contract: if the extractor
	// surfaces the alias as the resolved symbol, no match occurs.
	let table = synthetic_table();
	let imports = vec![ImportView {
		module_path: "fs".to_string(),
		imported_symbol: "readFile".to_string(),
		import_alias: Some("rf".to_string()),
	}];
	// Wrong: extractor handed us the local alias as the
	// resolved_symbol. Contract says the extractor must walk the
	// alias first. So this must NOT match.
	let callee = CalleePath {
		resolved_module: Some("fs".to_string()),
		resolved_symbol: "rf".to_string(),
	};
	assert!(
		match_form_a(&imports, &callee, &table, Language::Typescript).is_none(),
		"matcher does not walk aliases; extractor is responsible"
	);

	// Correct: extractor resolved through the alias. Matches.
	let callee = CalleePath {
		resolved_module: Some("fs".to_string()),
		resolved_symbol: "readFile".to_string(),
	};
	assert!(match_form_a(&imports, &callee, &table, Language::Typescript).is_some());
}
