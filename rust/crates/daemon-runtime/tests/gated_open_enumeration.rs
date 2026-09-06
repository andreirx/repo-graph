//! DAEMON-RESIDUALS-2C cycle-5 ruling: the rebuild-sentinel refusal lives behind ONE gated store-open
//! primitive that every open funnels through, so the next accidental bypass fails CI, not review.
//!
//! Three cycles each found a NEW per-caller bypass (`load_repo` → `reconcile` → `enrich_pass`) because
//! the sentinel check was applied per caller. This test ENUMERATES the direct-open call sites — the
//! same `rg StorageConnection::open_existing` the operator ran — over daemon-runtime/src and rgr/src,
//! and asserts the ONLY direct opens are the two gated primitives in `state.rs`
//! (`open_existing_gated` and `open_existing_with_busy_retry`). `repo rebuild` (the remedy) reindexes
//! via the `compose::open` CREATE path, so it has zero `open_existing` and needs no allow-listing here.
//!
//! Deliberately a source-text enumeration (not a type-level seam): the bypass vector is a human writing
//! `StorageConnection::open_existing(...)` in a new code path, exactly as the three removed bypasses
//! were written. Catching that literal is the point.

use std::path::{Path, PathBuf};

/// The needle the operator grepped for — a direct call to the NO-CREATE storage constructor.
const DIRECT_OPEN: &str = "StorageConnection::open_existing(";

/// The one file allowed to contain direct opens: the two gated primitives live here.
const ALLOWED_FILE: &str = "state.rs";

/// How many direct opens `state.rs` is expected to hold — one per gated primitive
/// (`open_existing_gated`, `open_existing_with_busy_retry`). A third would mean a new open primitive
/// slipped in un-reviewed; update this only alongside a deliberate new primitive.
const EXPECTED_IN_STATE_RS: usize = 2;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Recursively collect every `*.rs` file under `root`.
fn rs_files(root: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// A direct-open occurrence that is NOT inside a line comment (`//`, `///`, `//!`). We intentionally
/// key on the fully-qualified `StorageConnection::open_existing(` form: that is exactly how a bypass
/// gets written (and how the three removed bypasses were written). Test code that aliases the type
/// does not match — production bypasses do not alias.
fn is_real_call(line: &str) -> bool {
    match line.find(DIRECT_OPEN) {
        None => false,
        Some(idx) => {
            let before = &line[..idx];
            !before.contains("//")
        }
    }
}

#[test]
fn every_direct_store_open_routes_through_the_one_gated_helper() {
    let daemon_src = manifest_dir().join("src");
    let rgr_src = manifest_dir().join("../rgr/src");

    let mut files = Vec::new();
    rs_files(&daemon_src, &mut files);
    rs_files(&rgr_src, &mut files);
    assert!(
        !files.is_empty(),
        "found no .rs files to scan under {daemon_src:?} / {rgr_src:?}"
    );

    let mut offending: Vec<String> = Vec::new();
    let mut in_state_rs = 0usize;

    for file in &files {
        let text = std::fs::read_to_string(file).expect("read source file");
        let is_state_rs = file.file_name().and_then(|n| n.to_str()) == Some(ALLOWED_FILE);
        for (i, line) in text.lines().enumerate() {
            if !is_real_call(line) {
                continue;
            }
            if is_state_rs {
                in_state_rs += 1;
            } else {
                offending.push(format!("{}:{}: {}", file.display(), i + 1, line.trim()));
            }
        }
    }

    assert!(
        offending.is_empty(),
        "direct `StorageConnection::open_existing(` outside the gated helper in `state.rs` — every \
         store open must route through `state::open_existing_gated` (or the serving primitive \
         `open_existing_with_busy_retry`); `repo rebuild` is the only bypass and uses the create path. \
         Offending call sites:\n{}",
        offending.join("\n")
    );

    assert_eq!(
        in_state_rs, EXPECTED_IN_STATE_RS,
        "expected exactly {EXPECTED_IN_STATE_RS} direct opens in state.rs (the two gated primitives); \
         found {in_state_rs}. A new one means a new open primitive was added un-reviewed."
    );
}
