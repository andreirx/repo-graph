//! DEPS-GRADLE-CATALOG-1A (D-DGC-CONDITIONAL-1; RG-REQ-002-L11) — the `deps list` declared-set
//! marking.
//!
//! Crate-private module (the `deps_coverage.rs` / `deps_ecosystem_presence.rs` convention:
//! `deps_headline.rs` and `dispatch.rs` are over the 500-line structural guardrail and gain no new
//! responsibility). WHAT: one pure function that sums the index-time Gradle marking (the
//! `dependencies` blocks the reader skipped because it cannot attribute them statically) over the
//! view ecosystem's provenance records and attaches two additive envelope keys. CALLER:
//! `dispatch::handle_deps_list`, once, after `build_deps_list_response`. REJECTED SIMPLER: adding
//! the keys inside `build_deps_list_response` — a guardrail-exceeding file and a changed signature.

use repo_graph_module_queries::ProvenanceRead;

/// Attach `declared_undetermined_blocks` (u64) and `declared_undetermined_note` (String) to the
/// `deps list` envelope `response`.
///
/// Over `ProvenanceRead::Tracked`, the count is the sum of `undetermined_blocks.count` over the
/// records whose `ecosystem` is the view's — one record per Gradle build carries its build's
/// marking, so the sum counts distinct blocks — and the note names the `first` of the
/// smallest-path record with a nonzero count; a zero sum is a MEASURED zero (0 and ""). Over
/// `Absent` (predates tracking) and `Unavailable` (unreadable/malformed) NEITHER key is inserted, so
/// a JSON consumer never reads a measured zero where the provenance is unknown (RG-REQ-002-L04);
/// the existing coverage and manifest-context notes already carry that reason. A non-object
/// `response` is returned unchanged.
pub(crate) fn attach_declared_undetermined(
    mut response: serde_json::Value,
    provenance: &ProvenanceRead,
    ecosystem: &str,
) -> serde_json::Value {
    let records = match provenance {
        ProvenanceRead::Tracked(records) => records,
        ProvenanceRead::Absent | ProvenanceRead::Unavailable { .. } => return response,
    };
    let mut total: u64 = 0;
    let mut first: Option<(&str, &str)> = None;
    for r in records.iter().filter(|r| r.ecosystem == ecosystem) {
        let Some(marking) = &r.undetermined_blocks else {
            continue;
        };
        if marking.count == 0 {
            continue;
        }
        total += u64::from(marking.count);
        if first.is_none_or(|(path, _)| r.path.as_str() < path) {
            first = Some((r.path.as_str(), marking.first.as_str()));
        }
    }
    let note = match first {
        Some((_, location)) if total > 0 => {
            let at = if location.is_empty() {
                String::new()
            } else {
                format!(" ({location})")
            };
            format!(
                "declared set may be incomplete: {total} dependency block(s) not statically attributed{at} — investigate"
            )
        }
        _ => String::new(),
    };
    if let Some(obj) = response.as_object_mut() {
        obj.insert(
            "declared_undetermined_blocks".to_string(),
            serde_json::Value::from(total),
        );
        obj.insert(
            "declared_undetermined_note".to_string(),
            serde_json::Value::from(note),
        );
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_module_queries::{ManifestProvenance, UndeterminedBlocks};

    fn rec(path: &str, eco: &str, marking: Option<(u32, &str)>) -> ManifestProvenance {
        ManifestProvenance {
            path: path.to_string(),
            dir: path
                .rsplit_once('/')
                .map(|(d, _)| d)
                .unwrap_or("")
                .to_string(),
            ecosystem: eco.to_string(),
            error: None,
            error_kind: None,
            undetermined_blocks: marking.map(|(count, first)| UndeterminedBlocks {
                count,
                first: first.to_string(),
            }),
        }
    }

    fn base() -> serde_json::Value {
        serde_json::json!({ "ecosystem": "java", "count": 0, "results": [] })
    }

    /// The view ecosystem's records are summed (one record per build carries its build's marking,
    /// so the sum counts distinct blocks); another ecosystem's marking never leaks in; the note
    /// names the `first` of the smallest-path record with a nonzero count.
    #[test]
    fn declared_undetermined_sums_the_view_ecosystems_records_and_names_the_first_by_path() {
        let prov = ProvenanceRead::Tracked(vec![
            rec("x/build.gradle", "java", Some((3, "build.gradle:9"))),
            rec("a/build.gradle", "java", Some((2, "a/build.gradle:4"))),
            rec("web/package.json", "npm", Some((7, "web/package.json:1"))),
            rec("b/build.gradle", "java", None),
        ]);
        let out = attach_declared_undetermined(base(), &prov, "java");
        assert_eq!(out["declared_undetermined_blocks"], serde_json::json!(5));
        assert_eq!(
            out["declared_undetermined_note"],
            serde_json::json!(
                "declared set may be incomplete: 5 dependency block(s) not statically attributed (a/build.gradle:4) — investigate"
            )
        );
        assert_eq!(
            out["ecosystem"],
            serde_json::json!("java"),
            "other keys untouched"
        );
    }

    /// Tracked provenance without any marking is a MEASURED zero: both keys, 0 and "".
    #[test]
    fn declared_undetermined_zero_emits_zero_and_empty_note_keys() {
        let prov = ProvenanceRead::Tracked(vec![rec("build.gradle", "java", None)]);
        let out = attach_declared_undetermined(base(), &prov, "java");
        assert_eq!(out["declared_undetermined_blocks"], serde_json::json!(0));
        assert_eq!(out["declared_undetermined_note"], serde_json::json!(""));
        let empty = attach_declared_undetermined(base(), &ProvenanceRead::Tracked(vec![]), "java");
        assert_eq!(empty["declared_undetermined_blocks"], serde_json::json!(0));
    }

    /// Untracked provenance (predates tracking; unreadable/malformed) is NOT a measured zero: neither
    /// key is emitted, so a JSON consumer never reads 0 where the provenance is unknown
    /// (RG-REQ-002-L04); the existing coverage/manifest notes already carry the reason.
    #[test]
    fn declared_undetermined_untracked_provenance_emits_no_keys() {
        for prov in [
            ProvenanceRead::Absent,
            ProvenanceRead::Unavailable {
                reason: "provenance record malformed: x".to_string(),
            },
        ] {
            let out = attach_declared_undetermined(base(), &prov, "java");
            assert_eq!(out, base(), "no key added over untracked provenance");
            assert!(out.get("declared_undetermined_blocks").is_none());
            assert!(out.get("declared_undetermined_note").is_none());
        }
    }
}
