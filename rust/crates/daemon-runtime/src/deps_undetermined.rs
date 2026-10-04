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
//!
//! DEPS-GRADLE-CATALOG-1B (D-DGC1B-ALIAS-MARKING-1): the same function attaches the second marking
//! of the same records — the version-catalog alias references the index could not bind to a group —
//! as two more additive keys (the one call site is unchanged).

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
///
/// DEPS-GRADLE-CATALOG-1B: by the same rules, `declared_unresolved_alias_refs` (u64) sums
/// `unresolved_alias_refs.count` over the view ecosystem's records, and
/// `declared_unresolved_alias_note` (String) reads `declared set may be incomplete: N alias
/// reference(s) could not be resolved (<first>) — investigate`, `<first>` being the record's
/// `first` (the site and the binding test that failed for it) of the smallest-path record with a
/// nonzero count; 0 and "" when none.
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
    let mut alias_total: u64 = 0;
    let mut alias_first: Option<(&str, &str)> = None;
    for r in records.iter().filter(|r| r.ecosystem == ecosystem) {
        let Some(aliases) = &r.unresolved_alias_refs else {
            continue;
        };
        if aliases.count == 0 {
            continue;
        }
        alias_total += u64::from(aliases.count);
        if alias_first.is_none_or(|(path, _)| r.path.as_str() < path) {
            alias_first = Some((r.path.as_str(), aliases.first.as_str()));
        }
    }
    let alias_note = match alias_first {
        Some((_, site)) if alias_total > 0 => {
            let at = if site.is_empty() {
                String::new()
            } else {
                format!(" ({site})")
            };
            format!(
                "declared set may be incomplete: {alias_total} alias reference(s) could not be resolved{at} — investigate"
            )
        }
        _ => String::new(),
    };
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
        obj.insert(
            "declared_unresolved_alias_refs".to_string(),
            serde_json::Value::from(alias_total),
        );
        obj.insert(
            "declared_unresolved_alias_note".to_string(),
            serde_json::Value::from(alias_note),
        );
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_module_queries::{ManifestProvenance, UndeterminedBlocks, UnresolvedAliasRefs};

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
            unresolved_alias_refs: None,
        }
    }

    fn rec_aliases(path: &str, eco: &str, count: u32, first: &str) -> ManifestProvenance {
        ManifestProvenance {
            unresolved_alias_refs: Some(UnresolvedAliasRefs {
                count,
                first: first.to_string(),
            }),
            ..rec(path, eco, None)
        }
    }

    /// DEPS-GRADLE-CATALOG-1B (D-DGC1B-ALIAS-MARKING-1): the unbound alias references of the view
    /// ecosystem's builds are summed (one record per build carries its build's count, so the sum
    /// counts each reference once); another ecosystem's never leaks in; the note names the `first`
    /// of the smallest-path record with a nonzero count, in the decision record's corrected line;
    /// zero is a measured zero; untracked provenance emits neither key; the undetermined-blocks keys
    /// are unaffected.
    #[test]
    fn declared_unresolved_alias_refs_sums_the_view_ecosystems_records_and_names_the_first_by_path()
    {
        let prov = ProvenanceRead::Tracked(vec![
            rec_aliases(
                "x/build.gradle",
                "java",
                3,
                "x/build.gradle:9 libs.x: no catalog entry",
            ),
            rec_aliases(
                "a/build.gradle",
                "java",
                2,
                "a/build.gradle:4 libs.slf: catalog gradle/libs.versions.toml could not be read (TOML parse error at line 2, column 16)",
            ),
            rec_aliases("web/package.json", "npm", 7, "web/package.json:1 libs.y: no catalog entry"),
            rec("b/build.gradle", "java", Some((1, "b/build.gradle:2"))),
        ]);
        let out = attach_declared_undetermined(base(), &prov, "java");
        assert_eq!(out["declared_unresolved_alias_refs"], serde_json::json!(5));
        assert_eq!(
            out["declared_unresolved_alias_note"],
            serde_json::json!(
                "declared set may be incomplete: 5 alias reference(s) could not be resolved (a/build.gradle:4 libs.slf: catalog gradle/libs.versions.toml could not be read (TOML parse error at line 2, column 16)) — investigate"
            )
        );
        assert_eq!(out["declared_undetermined_blocks"], serde_json::json!(1));

        let zero = attach_declared_undetermined(
            base(),
            &ProvenanceRead::Tracked(vec![rec("build.gradle", "java", None)]),
            "java",
        );
        assert_eq!(zero["declared_unresolved_alias_refs"], serde_json::json!(0));
        assert_eq!(
            zero["declared_unresolved_alias_note"],
            serde_json::json!("")
        );

        for untracked in [
            ProvenanceRead::Absent,
            ProvenanceRead::Unavailable {
                reason: "provenance record malformed: x".to_string(),
            },
        ] {
            let out = attach_declared_undetermined(base(), &untracked, "java");
            assert!(out.get("declared_unresolved_alias_refs").is_none());
            assert!(out.get("declared_unresolved_alias_note").is_none());
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
