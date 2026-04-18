//! Stable-key builder contract tests. Pins every shape from
//! contract §5.1 byte-for-byte.

use repo_graph_state_bindings::{
	build_blob, build_cache_state, build_db_resource, build_fs_path, Driver,
	FsPathOrLogical, LogicalName, Provider, RepoUid, ValidationError,
};

// ── Shape pins (contract §5.1 + §5.2 examples) ────────────────────

#[test]
fn db_resource_shape() {
	let repo = RepoUid::new("myservice").unwrap();
	let drv = Driver::new("postgres").unwrap();
	let name = LogicalName::new("DATABASE_URL").unwrap();
	let key = build_db_resource(&repo, &drv, &name);
	assert_eq!(key.as_str(), "myservice:db:postgres:DATABASE_URL:DB_RESOURCE");
}

#[test]
fn db_resource_mysql_readreplica() {
	let repo = RepoUid::new("myservice").unwrap();
	let drv = Driver::new("mysql2").unwrap();
	let name = LogicalName::new("readreplica").unwrap();
	let key = build_db_resource(&repo, &drv, &name);
	assert_eq!(key.as_str(), "myservice:db:mysql2:readreplica:DB_RESOURCE");
}

#[test]
fn fs_path_literal() {
	let repo = RepoUid::new("myservice").unwrap();
	let p = FsPathOrLogical::new("/etc/app/settings.yaml").unwrap();
	let key = build_fs_path(&repo, &p);
	assert_eq!(key.as_str(), "myservice:fs:/etc/app/settings.yaml:FS_PATH");
}

#[test]
fn fs_path_logical() {
	let repo = RepoUid::new("myservice").unwrap();
	let p = FsPathOrLogical::new("CACHE_DIR").unwrap();
	let key = build_fs_path(&repo, &p);
	assert_eq!(key.as_str(), "myservice:fs:CACHE_DIR:FS_PATH");
}

#[test]
fn cache_state_shape() {
	let repo = RepoUid::new("myservice").unwrap();
	let drv = Driver::new("redis").unwrap();
	let name = LogicalName::new("REDIS_URL").unwrap();
	let key = build_cache_state(&repo, &drv, &name);
	assert_eq!(key.as_str(), "myservice:cache:redis:REDIS_URL:STATE");
}

#[test]
fn blob_shape() {
	let repo = RepoUid::new("myservice").unwrap();
	let prov = Provider::new("s3").unwrap();
	let name = LogicalName::new("artifacts-bucket").unwrap();
	let key = build_blob(&repo, &prov, &name);
	assert_eq!(key.as_str(), "myservice:blob:s3:artifacts-bucket:BLOB");
}

// ── Newtype validation — per-newtype exhaustive coverage ──────────

#[test]
fn repo_uid_happy_paths() {
	assert!(RepoUid::new("a").is_ok());
	assert!(RepoUid::new("my-service").is_ok());
	assert!(RepoUid::new("my_service-123").is_ok());
	assert!(RepoUid::new("billing-service").is_ok());
}

#[test]
fn repo_uid_rejections() {
	assert_eq!(
		RepoUid::new("").unwrap_err(),
		ValidationError::Empty { field: "repo_uid" }
	);
	assert_eq!(
		RepoUid::new("  ").unwrap_err(),
		ValidationError::Empty { field: "repo_uid" }
	);
	assert_eq!(
		RepoUid::new("a:b").unwrap_err(),
		ValidationError::ContainsColon { field: "repo_uid" }
	);
	assert_eq!(
		RepoUid::new("a\tb").unwrap_err(),
		ValidationError::ContainsControl { field: "repo_uid" }
	);
}

#[test]
fn driver_rejections() {
	assert_eq!(
		Driver::new("").unwrap_err(),
		ValidationError::Empty { field: "driver" }
	);
	assert_eq!(
		Driver::new("pg:extra").unwrap_err(),
		ValidationError::ContainsColon { field: "driver" }
	);
}

#[test]
fn provider_rejections() {
	assert_eq!(
		Provider::new("").unwrap_err(),
		ValidationError::Empty { field: "provider" }
	);
	assert_eq!(
		Provider::new("s3:extra").unwrap_err(),
		ValidationError::ContainsColon { field: "provider" }
	);
}

#[test]
fn logical_name_rejections() {
	assert_eq!(
		LogicalName::new("").unwrap_err(),
		ValidationError::Empty {
			field: "logical_name"
		}
	);
	assert_eq!(
		LogicalName::new("a:b").unwrap_err(),
		ValidationError::ContainsColon {
			field: "logical_name"
		}
	);
	// Unicode whitespace is NOT control — pass through (trimmed
	// normally by `trim()`).
	let n = LogicalName::new(" spaced-name ").unwrap();
	assert_eq!(n.as_str(), "spaced-name");
}

#[test]
fn fs_path_or_logical_rejections() {
	assert_eq!(
		FsPathOrLogical::new("").unwrap_err(),
		ValidationError::Empty {
			field: "fs_path_or_logical"
		}
	);
	// Interior control char: trim does not touch it, validation
	// rejects it.
	assert_eq!(
		FsPathOrLogical::new("/etc/path\nwith/newline").unwrap_err(),
		ValidationError::ContainsControl {
			field: "fs_path_or_logical"
		}
	);
	// Whitespace-only input is empty after trim.
	assert_eq!(
		FsPathOrLogical::new("   ").unwrap_err(),
		ValidationError::Empty {
			field: "fs_path_or_logical"
		}
	);
	// Note: surrounding whitespace (leading/trailing spaces or
	// tabs) is silently trimmed at the newtype layer. The
	// binding-table loader is the layer that rejects authoring
	// mistakes of that kind on `module` / `symbol_path` /
	// `driver` (see `TableError::SurroundingWhitespace`).
	// `FsPathOrLogical` has no authored-value path in slice 1.
}

#[test]
fn fs_path_allows_windows_drive_letter() {
	// Contract §5.1 note: FS stable-key parsing is prefix/suffix
	// bounded, not naive colon-split. The path payload
	// legitimately contains colons on Windows.
	let p = FsPathOrLogical::new("C:\\Windows\\path").expect("Windows absolute paths are legal");
	assert_eq!(p.as_str(), "C:\\Windows\\path");

	let p = FsPathOrLogical::new("D:\\repo\\data.db").expect("secondary drive paths are legal");
	assert_eq!(p.as_str(), "D:\\repo\\data.db");
}

#[test]
fn fs_path_allows_uri_style_references() {
	// URI-like references may also appear as fs targets in some
	// bindings (e.g. `file:///etc/config`). Colons in the scheme
	// and authority are legal.
	let p = FsPathOrLogical::new("file:///etc/config").expect("file URIs are legal");
	assert_eq!(p.as_str(), "file:///etc/config");
}

#[test]
fn fs_path_stable_key_round_trip_with_windows_path() {
	use repo_graph_state_bindings::{build_fs_path, RepoUid};
	// End-to-end: a Windows path travels through the builder and
	// the resulting stable key preserves the payload verbatim
	// between the fixed `fs:` prefix and the fixed `:FS_PATH`
	// suffix.
	let repo = RepoUid::new("myservice").unwrap();
	let p = FsPathOrLogical::new("C:\\Windows\\path").unwrap();
	let key = build_fs_path(&repo, &p);
	assert_eq!(key.as_str(), "myservice:fs:C:\\Windows\\path:FS_PATH");

	// Prefix and suffix structural check: the stable key starts
	// with `<repo_uid>:fs:` and ends with `:FS_PATH`. A parser
	// that consumes the stable key must use these bounds, not
	// naive global colon-split.
	assert!(key.as_str().starts_with("myservice:fs:"));
	assert!(key.as_str().ends_with(":FS_PATH"));
}

// ── Display / interop ─────────────────────────────────────────────

#[test]
fn stable_key_display_matches_as_str() {
	let repo = RepoUid::new("r").unwrap();
	let drv = Driver::new("d").unwrap();
	let name = LogicalName::new("n").unwrap();
	let key = build_db_resource(&repo, &drv, &name);
	assert_eq!(format!("{}", key), key.as_str());
}

#[test]
fn stable_key_into_string_round_trip() {
	let repo = RepoUid::new("r").unwrap();
	let drv = Driver::new("d").unwrap();
	let name = LogicalName::new("n").unwrap();
	let key = build_db_resource(&repo, &drv, &name);
	let expected = key.as_str().to_string();
	assert_eq!(key.into_string(), expected);
}
