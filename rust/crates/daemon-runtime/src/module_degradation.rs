//! Honest degradation for the module-graph command surface
//! (MODULE-OWNERSHIP-DUPLICATE-1).
//!
//! `load_module_graph_facts` fails when a snapshot violates the single-owner
//! invariant — a file claimed by more than one module. Generation (`repo-index`
//! compose) resolves the known npm-vs-inferred collision (npm wins), so this
//! should not happen; this module is the safety net for any residual collision
//! shape.
//!
//! Rather than surfacing a bare `InternalError` (opaque — it reads as a repo-graph
//! bug), the module commands report the defect + affected files as a **labeled**
//! error the agent can act on: `StateUnavailable` ("the module graph is not in a
//! queryable state") with a reader-facing message naming the offending files and a
//! `data` block for `--json` consumers.
//!
//! Why an error and not a success-with-empty-results envelope: the module graph
//! genuinely cannot be derived (the ownership DTOs carry no ecosystem, so there is
//! no non-arbitrary winner — MODULE-OWNERSHIP-DUPLICATE-1 §3, "not a coin flip";
//! the resolution rule lives at generation, where ecosystem is known). An empty
//! `results` envelope would render as "No modules detected" through the existing
//! CLI — a FALSE known-zero on a repo that has modules, violating the VISION's
//! "unknown is never zero" rule. A labeled error renders honestly in both human
//! and JSON modes without changing the (out-of-scope) CLI renderer.
//!
//! One helper, five callers (the `load_module_graph_facts` sites in
//! `handle_modules_deps` / `_violations` / `_show` / `_list` and the governance
//! `violations` handler); the shared mapping avoids duplicating the reader-facing
//! message across all five and keeps it out of the oversized `dispatch.rs`.

use repo_graph_daemon_transport::{DispatchResult, ErrorCode, ErrorDetail};
use repo_graph_module_queries::ModuleQueryError;

/// How many affected files to name inline in the human-readable message before
/// eliding. The full list always rides the error `data` for `--json` consumers.
const MAX_NAMED_FILES: usize = 8;

/// Map a `load_module_graph_facts` error to a dispatch result.
///
/// - [`ModuleQueryError::DuplicateOwnership`] → a **labeled** `StateUnavailable`
///   error naming the defect + affected files (NOT a bare `InternalError`).
/// - any other error → the prior `InternalError` behavior, unchanged.
pub(crate) fn module_facts_error_result(
    request_id: &str,
    command: &str,
    err: ModuleQueryError,
) -> DispatchResult {
    match err {
        ModuleQueryError::DuplicateOwnership { affected_files } => DispatchResult::error(
            request_id,
            duplicate_ownership_error(command, &affected_files),
        ),
        other => DispatchResult::error(
            request_id,
            ErrorDetail::new(
                ErrorCode::InternalError,
                format!("failed to load module graph facts: {}", other),
            ),
        ),
    }
}

/// Build the reader-facing, labeled error for a duplicate-ownership defect.
fn duplicate_ownership_error(command: &str, affected_file_uids: &[String]) -> ErrorDetail {
    // Reader-facing paths: a file_uid is "{repo_uid}:{rel_path}"; show the rel_path
    // (labels speak the reader's language, not our internal identity scheme).
    let affected_paths: Vec<&str> = affected_file_uids
        .iter()
        .map(|uid| uid.split_once(':').map(|(_, path)| path).unwrap_or(uid))
        .collect();
    let n = affected_paths.len();

    let mut named = affected_paths
        .iter()
        .take(MAX_NAMED_FILES)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    if n > MAX_NAMED_FILES {
        named.push_str(&format!(", … (+{} more)", n - MAX_NAMED_FILES));
    }

    let message = format!(
        "module relationships unavailable: {n} file(s) are claimed by more than one module \
         ({named}); this is a module-detection defect, so module dependency and rollup facts \
         cannot be derived for this snapshot. Re-index the repo; if it persists, report it as a \
         module-detection defect with the affected files."
    );

    ErrorDetail::with_data(
        ErrorCode::StateUnavailable,
        message,
        serde_json::json!({
            "kind": "duplicate_file_ownership",
            "command": command,
            "affected_files": affected_paths,
            "next_action":
                "Re-index the repo. If this persists it is a module-detection defect — \
                 please report it with the affected files listed here.",
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_ownership_maps_to_labeled_state_unavailable_not_internal_error() {
        let result = module_facts_error_result(
            "req-1",
            "modules list",
            ModuleQueryError::DuplicateOwnership {
                affected_files: vec![
                    "repo_abc:extensions/esbuild-common.mts".to_string(),
                    "repo_abc:shared/util.ts".to_string(),
                ],
            },
        );

        match result {
            DispatchResult::Error(resp) => {
                // Labeled — NOT InternalError.
                assert_eq!(resp.error.code, ErrorCode::StateUnavailable.as_str());
                assert_ne!(resp.error.code, ErrorCode::InternalError.as_str());
                // Reader-facing rel_paths in the message, not internal file_uids.
                assert!(
                    resp.error.message.contains("extensions/esbuild-common.mts"),
                    "message: {}",
                    resp.error.message
                );
                assert!(!resp.error.message.contains("repo_abc:"));
                assert!(resp.error.message.contains("2 file(s)"));
                // Structured data for --json consumers carries the full set.
                let data = resp.error.data.expect("error data");
                assert_eq!(data["kind"], "duplicate_file_ownership");
                assert_eq!(data["affected_files"][0], "extensions/esbuild-common.mts");
                assert_eq!(data["affected_files"][1], "shared/util.ts");
            }
            DispatchResult::Success(_) => {
                panic!("expected a labeled StateUnavailable error, got success")
            }
        }
    }

    #[test]
    fn many_affected_files_are_elided_in_message_but_complete_in_data() {
        let files: Vec<String> = (0..12).map(|i| format!("repo_x:pkg/f{i}.mts")).collect();
        let result = module_facts_error_result(
            "req-3",
            "violations",
            ModuleQueryError::DuplicateOwnership {
                affected_files: files,
            },
        );
        match result {
            DispatchResult::Error(resp) => {
                assert!(
                    resp.error.message.contains("+4 more"),
                    "message: {}",
                    resp.error.message
                );
                let data = resp.error.data.expect("error data");
                // The full set (all 12) rides the data, not just the named 8.
                assert_eq!(data["affected_files"].as_array().unwrap().len(), 12);
            }
            DispatchResult::Success(_) => panic!("expected error"),
        }
    }

    #[test]
    fn other_errors_still_map_to_internal_error() {
        let result = module_facts_error_result(
            "req-2",
            "modules list",
            ModuleQueryError::EdgeDerivation("boom".to_string()),
        );

        match result {
            DispatchResult::Error(resp) => {
                assert_eq!(resp.error.code, ErrorCode::InternalError.as_str());
                assert!(
                    resp.error
                        .message
                        .contains("failed to load module graph facts"),
                    "message: {}",
                    resp.error.message
                );
            }
            DispatchResult::Success(_) => {
                panic!("expected InternalError for a non-duplicate load error")
            }
        }
    }
}
