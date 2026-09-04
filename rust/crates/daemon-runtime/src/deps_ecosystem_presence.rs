//! DEPS-ATTRIB-2 §2.4 (ruling DR-JAVA-NOREADER = Option 2) — the truth of every materially-present,
//! reader-bearing dependency ecosystem OTHER than the one the default `deps list` view renders, so a
//! materially-present ecosystem (glamCRM's Java half) is NEVER silently absent from the default view.
//!
//! Crate-private module extracted from `deps_headline.rs` (classification) and `reader_context.rs`
//! (`secondary_material_ecosystems`) under the 500-line structural guardrail (DEPS-ATTRIB-2 review-1
//! item 5 — a pre-ratified, guardrail-driven extraction, NOT a new public boundary). Unifying both
//! §2.4 pieces here keeps the "secondary ecosystem" responsibility in ONE place instead of split
//! across two >500-line files. AXIS: a FIXED set of ecosystem-truth states with growing operations
//! (classify, render, JSON) → sum type + exhaustive match. CALLERS: `dispatch::handle_deps_list`
//! (selects the secondary ecosystems and classifies each), `deps_headline::build_deps_list_response`
//! (serializes). REJECTED SIMPLER: leaving it inline in the two god-files — forbidden by the guardrail.

use repo_graph_module_queries::{ComposeDependenciesResult, ProvenanceRead};

use crate::reader_context::{
    language_deps_ecosystem, language_display_name, MATERIAL_LANGUAGE_SHARE_NUM,
};

/// The truth of one materially-present, reader-bearing ecosystem in the DEFAULT `deps list` view,
/// OTHER than the rendered dominant ecosystem. Its purpose: a materially-present ecosystem (glamCRM's
/// Java half) is NEVER silently absent from the default view — even though the view renders only the
/// single dominant ecosystem's tables, every other material ecosystem's truth is stated in one line.
pub(crate) struct EcosystemPresence {
    /// The ecosystem key (`java`, `python`, …) as `deps list --ecosystem <this>` accepts.
    pub ecosystem: String,
    /// Its truth on this index.
    pub state: EcosystemPresenceState,
}

/// The mutually-exclusive truth states of a secondary ecosystem (a sum type, not a bool + nullable
/// counts — each variant carries only the data valid in that state). Chosen so the default view can
/// never present a failed read as an absence, nor an absence as a zero-dep attribution.
#[derive(Debug)]
pub(crate) enum EcosystemPresenceState {
    /// ≥1 manifest of this ecosystem PARSED: declared deps were read and attributed to modules. The
    /// counts are the ecosystem's OWN compose result (so `deps list --ecosystem <e>` renders
    /// consistent numbers). `declared_dependencies == 0` is honest measured-empty (a parsed build
    /// script with no deps), NOT an absence.
    Attributed {
        declared_dependencies: usize,
        manifests: usize,
    },
    /// A manifest was PRESENT but could not be read/parsed, OR the provenance/compose read failed —
    /// the truth is UNKNOWN, carried with its specific reason. Never a fabricated count (STANDING
    /// HONESTY RULE #1).
    Unavailable { reason: String },
    /// This ecosystem's language is materially present, but ZERO manifests were parsed for it — a
    /// COMPUTED-TRUE absence (no build script found on this index), stated, never silently dropped.
    NoManifestParsed { source_files: usize },
}

/// Classify one secondary ecosystem's truth from the SHARED provenance read plus THIS ecosystem's own
/// compose OUTCOME. Pure — the storage orchestration (running the per-ecosystem compose) stays in the
/// dispatch arm. `compose` is `Err` when that compose read failed; `source_files` is the material file
/// count of the ecosystem's language (for the computed-absence statement). The state precedence
/// encodes the honesty order (review-4 blocker 1): a read failure — the provenance read, OR a
/// present-but-unparsed manifest of THIS ecosystem (even when a SIBLING manifest parsed) — outranks
/// everything, so a partial attribution never masks a failed sibling read; then a parsed manifest
/// yields real attribution; only a set with no failed and no parsed manifest is a computed absence.
pub(crate) fn classify_ecosystem_presence(
    ecosystem: &str,
    source_files: usize,
    provenance: &ProvenanceRead,
    compose: Result<&ComposeDependenciesResult, String>,
) -> EcosystemPresenceState {
    let records = match provenance {
        ProvenanceRead::Tracked(r) => r,
        ProvenanceRead::Absent => {
            return EcosystemPresenceState::Unavailable {
                reason: "indexed before manifest-provenance tracking".to_string(),
            }
        }
        ProvenanceRead::Unavailable { reason } => {
            return EcosystemPresenceState::Unavailable {
                reason: reason.clone(),
            }
        }
    };
    // review-4 blocker 1: a FAILED read of ANY manifest of this ecosystem outranks a partial
    // attribution. Rendering `Attributed` while a sibling manifest of the SAME ecosystem failed to
    // read would claim COMPLETE truth over a hidden failed read — the mixed-failure mask the reviewer
    // caught. Surface the failure (with its exact reason) BEFORE the parsed-count branches, so the
    // presence of one errored manifest is never silently swallowed by another that parsed.
    if let Some(f) = records
        .iter()
        .find(|r| r.ecosystem == ecosystem && r.error.is_some())
    {
        return EcosystemPresenceState::Unavailable {
            reason: format!(
                "manifest {} present but not parsed: {}",
                f.path,
                f.error.as_deref().unwrap_or("unknown error")
            ),
        };
    }
    // No errored manifest of this ecosystem remains — every matching record parsed cleanly.
    let parsed = records.iter().filter(|r| r.ecosystem == ecosystem).count();
    if parsed == 0 {
        // No manifest of this ecosystem at all → computed-true absence, stated.
        return EcosystemPresenceState::NoManifestParsed { source_files };
    }
    // ≥1 parsed manifest, none failed → real attribution from the ecosystem's own compose.
    match compose {
        Ok(result) => {
            let declared_dependencies: usize = result
                .summaries
                .iter()
                .map(|s| s.declared_and_used_count() + s.declared_but_unobserved_count())
                .sum();
            EcosystemPresenceState::Attributed {
                declared_dependencies,
                manifests: parsed,
            }
        }
        Err(reason) => EcosystemPresenceState::Unavailable {
            reason: format!("dependency read failed: {reason}"),
        },
    }
}

/// The additive JSON for one secondary ecosystem's truth (a `state` tag + only the fields valid in
/// that state). Consumed by the CLI presenter (`rgr::presentation::deps_list`) which renders one
/// honest line per element.
pub(crate) fn ecosystem_presence_json(p: &EcosystemPresence) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    m.insert("ecosystem".to_string(), serde_json::json!(p.ecosystem));
    match &p.state {
        EcosystemPresenceState::Attributed {
            declared_dependencies,
            manifests,
        } => {
            m.insert("state".to_string(), serde_json::json!("attributed"));
            m.insert(
                "declared_dependencies".to_string(),
                serde_json::json!(declared_dependencies),
            );
            m.insert("manifests".to_string(), serde_json::json!(manifests));
        }
        EcosystemPresenceState::Unavailable { reason } => {
            m.insert("state".to_string(), serde_json::json!("unavailable"));
            m.insert("reason".to_string(), serde_json::json!(reason));
        }
        EcosystemPresenceState::NoManifestParsed { source_files } => {
            m.insert("state".to_string(), serde_json::json!("no_manifest_parsed"));
            m.insert("source_files".to_string(), serde_json::json!(source_files));
        }
    }
    serde_json::Value::Object(m)
}

/// The materially-present, reader-bearing dependency ecosystems of this repo OTHER than `dominant` —
/// each as `(ecosystem, source_files)`, where `source_files` is the summed material file count of the
/// languages mapping to it. This is what lets the DEFAULT `deps list` view state every material
/// ecosystem's truth (glamCRM's Java half) instead of silently rendering only the dominant one.
///
/// Materiality is applied to the ECOSYSTEM TOTAL, not to each language independently (DEPS-ATTRIB-2
/// review-1 item 3): source-file counts are first aggregated BY ecosystem (typescript + javascript
/// collapse to one `npm`), THEN the ≥[`MATERIAL_LANGUAGE_SHARE_NUM`]%-of-code-files gate is applied to
/// each ecosystem's total. A repo that is Java 88% / TS 6% / JS 6% keeps its materially-present 12%
/// npm ecosystem — filtering TS and JS independently would drop both (each < 10%) and silently lose
/// npm. Reuses the SAME gate the D5 next-action uses (one materiality definition, never a re-derived
/// threshold) and [`language_deps_ecosystem`] for the language→ecosystem map (reader-less languages
/// have no ecosystem and are excluded — the dominant reader-context note already speaks to a
/// reader-less repo). Count-DESC order preserved. `language_counts` MUST arrive count-DESC (as
/// `query_file_count_by_language` returns).
pub(crate) fn secondary_material_ecosystems(
    language_counts: &[(String, u64)],
    dominant: &str,
) -> Vec<(String, usize)> {
    // Denominator: total CODE files (every language with a display name — config-file tokens like
    // `json` never dilute or contribute to an ecosystem's share).
    let total_code: u64 = language_counts
        .iter()
        .filter(|(l, _)| language_display_name(l).is_some())
        .map(|(_, n)| *n)
        .sum();
    if total_code == 0 {
        return Vec::new();
    }
    // Phase 1 — aggregate code-file counts BY ecosystem (before any gate), preserving first-seen
    // (count-DESC) order. The dominant ecosystem is the primary rendered view, never a "secondary".
    let mut order: Vec<&'static str> = Vec::new();
    let mut source_files: std::collections::HashMap<&'static str, u64> =
        std::collections::HashMap::new();
    for (lang, n) in language_counts {
        if language_display_name(lang).is_none() {
            continue; // config-file token — never counts toward an ecosystem
        }
        let Some(eco) = language_deps_ecosystem(lang) else {
            continue; // reader-less language — not a dependency ecosystem
        };
        if eco == dominant {
            continue;
        }
        if !source_files.contains_key(eco) {
            order.push(eco);
        }
        *source_files.entry(eco).or_default() += *n;
    }
    // Phase 2 — keep only ecosystems whose AGGREGATED share clears the ≥10% materiality gate.
    order
        .into_iter()
        .filter(|eco| source_files[eco] * MATERIAL_LANGUAGE_SHARE_NUM >= total_code)
        .map(|eco| (eco.to_string(), source_files[eco] as usize))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_module_queries::{
        DependencyCategory, DependencyEntry, ManifestContext, ManifestProvenance,
        ModuleDependencySummary,
    };

    fn prov_rec(path: &str, dir: &str, eco: &str) -> ManifestProvenance {
        ManifestProvenance {
            path: path.to_string(),
            dir: dir.to_string(),
            ecosystem: eco.to_string(),
            error: None,
        }
    }

    fn prov_rec_failed(path: &str, dir: &str, eco: &str, reason: &str) -> ManifestProvenance {
        ManifestProvenance {
            path: path.to_string(),
            dir: dir.to_string(),
            ecosystem: eco.to_string(),
            error: Some(reason.to_string()),
        }
    }

    fn summary(module: &str, ctx: ManifestContext, declared: &[&str]) -> ModuleDependencySummary {
        let entries = declared
            .iter()
            .map(|p| DependencyEntry {
                package: p.to_string(),
                category: DependencyCategory::DeclaredButUnobserved,
                import_count: 0,
                dependency_class: None,
                confidence: 1.0,
                raw_specifiers: vec![],
            })
            .collect();
        ModuleDependencySummary {
            module: module.to_string(),
            manifest_context: ctx,
            manifest_scope_available: true,
            entries,
            rejected_non_specifier: 0,
            declared_manifest_paths: vec![],
        }
    }

    fn result_of(summaries: Vec<ModuleDependencySummary>) -> ComposeDependenciesResult {
        ComposeDependenciesResult {
            summaries,
            total_external_imports: 3,
        }
    }

    // ── §2.4 classification ──

    #[test]
    fn ecosystem_presence_attributed_reports_real_declared_dep_count() {
        // A parsed Gradle manifest + a compose that attributed deps → Attributed with the real count
        // (glamCRM's Java half: the audit's "zero mention of Java" is replaced by the truth).
        let records = vec![prov_rec("backend/build.gradle", "backend", "java")];
        let compose = result_of(vec![summary(
            "backend",
            ManifestContext::Parsed {
                path: "backend/build.gradle".into(),
            },
            &["org.spring:core", "com.google:guava"],
        )]);
        let state = classify_ecosystem_presence(
            "java",
            267,
            &ProvenanceRead::Tracked(records),
            Ok(&compose),
        );
        match state {
            EcosystemPresenceState::Attributed {
                declared_dependencies,
                manifests,
            } => {
                assert_eq!(declared_dependencies, 2);
                assert_eq!(manifests, 1);
            }
            other => panic!("expected Attributed, got a different state: {other:?}"),
        }
    }

    #[test]
    fn ecosystem_presence_present_but_unparsed_is_unavailable_with_reason() {
        // A build.gradle present but UNREADABLE → unknown-with-reason, never a false absence.
        let records = vec![prov_rec_failed(
            "backend/build.gradle",
            "backend",
            "java",
            "permission denied",
        )];
        let state = classify_ecosystem_presence(
            "java",
            267,
            &ProvenanceRead::Tracked(records),
            Err("unused".to_string()),
        );
        match state {
            EcosystemPresenceState::Unavailable { reason } => {
                assert!(reason.contains("backend/build.gradle"), "{reason}");
                assert!(reason.contains("permission denied"), "{reason}");
            }
            other => panic!("expected Unavailable, got: {other:?}"),
        }
    }

    #[test]
    fn ecosystem_presence_mixed_parsed_and_failed_manifest_is_unavailable_not_attributed() {
        // review-4 blocker 1: TWO Gradle manifests — one PARSED, one UNREADABLE. The pre-fix code saw
        // `parsed >= 1` and rendered `Attributed`, silently swallowing the failed sibling read. The
        // failure must OUTRANK the partial attribution → Unavailable, carrying the failed manifest's
        // exact reason (never a complete-attribution claim over a hidden failed read).
        let records = vec![
            prov_rec("app/build.gradle", "app", "java"),
            prov_rec_failed("lib/build.gradle", "lib", "java", "permission denied"),
        ];
        let compose = result_of(vec![summary(
            "app",
            ManifestContext::Parsed {
                path: "app/build.gradle".into(),
            },
            &["org.spring:core"],
        )]);
        let state = classify_ecosystem_presence(
            "java",
            300,
            &ProvenanceRead::Tracked(records),
            Ok(&compose),
        );
        match state {
            EcosystemPresenceState::Unavailable { reason } => {
                assert!(reason.contains("lib/build.gradle"), "{reason}");
                assert!(reason.contains("permission denied"), "{reason}");
            }
            other => {
                panic!("expected Unavailable (failed sibling outranks Attributed), got: {other:?}")
            }
        }
    }

    #[test]
    fn ecosystem_presence_material_source_no_manifest_is_computed_absence() {
        // Java source materially present, but NO java manifest parsed → computed-true absence, stated.
        let records = vec![prov_rec("frontend/web/package.json", "frontend/web", "npm")];
        let state = classify_ecosystem_presence(
            "java",
            120,
            &ProvenanceRead::Tracked(records),
            Err("no java compose".to_string()),
        );
        assert!(matches!(
            state,
            EcosystemPresenceState::NoManifestParsed { source_files: 120 }
        ));
    }

    #[test]
    fn ecosystem_presence_untracked_provenance_is_unavailable_not_silent() {
        // Old snapshot / unreadable provenance → coverage unknown, never a silent absence.
        assert!(matches!(
            classify_ecosystem_presence("java", 9, &ProvenanceRead::Absent, Err("x".into())),
            EcosystemPresenceState::Unavailable { .. }
        ));
        assert!(matches!(
            classify_ecosystem_presence(
                "java",
                9,
                &ProvenanceRead::Unavailable {
                    reason: "disk".into()
                },
                Err("x".into())
            ),
            EcosystemPresenceState::Unavailable { .. }
        ));
    }

    #[test]
    fn ecosystem_presence_compose_read_failure_is_unavailable_not_fabricated() {
        // A parsed manifest exists but the ecosystem's compose read FAILED → unknown-with-reason,
        // never a fabricated attributed-0 (STANDING HONESTY RULE #1).
        let records = vec![prov_rec("backend/build.gradle", "backend", "java")];
        let state = classify_ecosystem_presence(
            "java",
            267,
            &ProvenanceRead::Tracked(records),
            Err("storage read failed".to_string()),
        );
        match state {
            EcosystemPresenceState::Unavailable { reason } => {
                assert!(reason.contains("storage read failed"), "{reason}")
            }
            other => panic!("expected Unavailable, got: {other:?}"),
        }
    }

    #[test]
    fn presence_json_carries_only_the_valid_fields_per_state() {
        let attributed = ecosystem_presence_json(&EcosystemPresence {
            ecosystem: "java".into(),
            state: EcosystemPresenceState::Attributed {
                declared_dependencies: 18,
                manifests: 1,
            },
        });
        assert_eq!(attributed["state"], "attributed");
        assert_eq!(attributed["declared_dependencies"], 18);
        assert!(attributed.get("reason").is_none());

        let unavailable = ecosystem_presence_json(&EcosystemPresence {
            ecosystem: "java".into(),
            state: EcosystemPresenceState::Unavailable {
                reason: "boom".into(),
            },
        });
        assert_eq!(unavailable["state"], "unavailable");
        assert_eq!(unavailable["reason"], "boom");
        assert!(unavailable.get("declared_dependencies").is_none());
    }

    // ── §2.4 secondary material ecosystems (materiality on the ecosystem TOTAL) ──

    #[test]
    fn secondary_material_ecosystems_names_glamcrm_java_not_dominant_npm() {
        // glamCRM shape: TS/JS dominant (npm), Java a material ~40% half. The DEFAULT npm view must
        // surface Java as a secondary ecosystem (with its source-file count), never silently drop it.
        let counts = vec![
            ("typescript".to_string(), 400u64),
            ("java".to_string(), 267u64),
            ("json".to_string(), 90u64), // config token — never dilutes the code-file share
        ];
        let out = secondary_material_ecosystems(&counts, "npm");
        assert_eq!(out, vec![("java".to_string(), 267)]);
    }

    #[test]
    fn secondary_material_ecosystems_excludes_below_gate_and_reader_less() {
        // django shape: Python dominant, ~3.7% JS (below the ≥10% gate) → npm NOT surfaced. A C half
        // (reader-less) is never an ecosystem. Only material, reader-bearing, non-dominant survives.
        let counts = vec![
            ("python".to_string(), 2904u64),
            ("c".to_string(), 500u64), // reader-less, even though material → not an ecosystem
            ("javascript".to_string(), 111u64), // ~3.7% → below gate
        ];
        let out = secondary_material_ecosystems(&counts, "python");
        assert!(out.is_empty(), "unexpected secondary ecosystems: {out:?}");
    }

    #[test]
    fn secondary_material_ecosystems_collapses_ts_js_into_one_npm() {
        // typescript + javascript both map to npm → one entry, source_files summed; dominant java
        // excluded.
        let counts = vec![
            ("java".to_string(), 500u64),
            ("typescript".to_string(), 300u64),
            ("javascript".to_string(), 250u64),
        ];
        let out = secondary_material_ecosystems(&counts, "java");
        assert_eq!(out, vec![("npm".to_string(), 550)]);
    }

    #[test]
    fn secondary_material_ecosystems_gate_applies_to_ecosystem_total_not_each_language() {
        // DEPS-ATTRIB-2 review-1 item 3 (the regression): Java 88% / TS 6% / JS 6%. Each of TS and JS
        // is individually BELOW the ≥10% gate, but npm's AGGREGATED share (12%) clears it. Filtering
        // per-language would silently lose the materially-present 12% npm ecosystem — it must NOT.
        let counts = vec![
            ("java".to_string(), 88u64),
            ("typescript".to_string(), 6u64),
            ("javascript".to_string(), 6u64),
        ];
        let out = secondary_material_ecosystems(&counts, "java");
        assert_eq!(
            out,
            vec![("npm".to_string(), 12)],
            "npm's 12% aggregate must survive even though TS and JS are each < 10%"
        );
    }
}
