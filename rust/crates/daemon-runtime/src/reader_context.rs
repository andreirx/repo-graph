//! Pure reader-context label logic for HONEST-DEGRADATION-IMPL-2 (D2 + D5).
//!
//! These helpers are extracted verbatim from `dispatch.rs` (which had grown to ~8300 lines, tripping the
//! architecture rule "do not append new responsibilities to oversized files"; `agent_docs/architecture.md`
//! — Prohibited Patterns). They CONSUME existing values — the repo's extracted languages, the
//! already-counted external imports, the trust reliability posture, and the daemon's configured resolvers
//! (passed in) — and turn them into honest reader-context text. They compute NO new posture and run NO
//! resolver; behavior is unchanged from the inlined form.
//!
//! `configured_resolver_languages` (the single resolver-config source `handle_enrich` registers from)
//! deliberately STAYS in `dispatch`; the D5 next-action line takes the configured set as a parameter, and
//! the unit tests import that source via `crate::dispatch::configured_resolver_languages`.

use enrichment::EnrichmentLanguage;

/// Map an indexer `files.language` token (`indexer::routing::detect_language`'s output, e.g.
/// `"typescript"`, `"tsx"`, `"c"`) to the enrichment language whose resolver covers it, or `None` if no
/// resolver exists for it on ANY build (C / C++ / Python / …). The resolver-FAMILY grouping is deferred
/// to the enrichment crate's own [`EnrichmentLanguage::from_extension`] (the single source for
/// `ts|tsx|js|jsx → TypeScript` etc.) by mapping each language token to a representative file extension —
/// so the family definition is never duplicated here, only the indexer's word-token spelling is named.
fn token_enrichment_language(token: &str) -> Option<EnrichmentLanguage> {
    let representative_ext = match token {
        // Word-tokens whose spelling differs from any file extension:
        "typescript" => "ts",
        "javascript" => "js",
        "rust" => "rs",
        // `tsx`/`jsx`/`java` are already extension-shaped tokens `from_extension` recognizes directly;
        // any other token (`c`/`cpp`/`python`/…) yields `None` from `from_extension` (no resolver).
        other => other,
    };
    EnrichmentLanguage::from_extension(representative_ext)
}

/// Reader-facing display name for an indexer language token (`None` for a token with no stable display
/// name, so a raw internal token is never shown to the reader). Used by the D2 deps note and the D5
/// no-resolution-path line.
fn language_display_name(token: &str) -> Option<&'static str> {
    Some(match token {
        "c" => "C",
        "cpp" => "C++",
        "python" => "Python",
        "java" => "Java",
        "rust" => "Rust",
        "typescript" | "tsx" => "TypeScript",
        "javascript" | "jsx" => "JavaScript",
        "go" => "Go",
        "kotlin" => "Kotlin",
        "scala" => "Scala",
        "ruby" => "Ruby",
        "swift" => "Swift",
        "objc" => "Objective-C",
        _ => return None,
    })
}

/// Join the display names of `languages` deterministically (sorted, de-duplicated, `/`-separated),
/// skipping tokens with no display name. Empty when none are nameable.
fn display_language_names(languages: &[String]) -> String {
    let mut names: Vec<&'static str> = languages
        .iter()
        .filter_map(|l| language_display_name(l))
        .collect();
    names.sort_unstable();
    names.dedup();
    names.join("/")
}

/// Map one indexer language token to the dependency-manifest ecosystem whose reader owns it,
/// or `None` for a language with no manifest reader on this build (C / C++ / Go / …).
///
/// npm = the TypeScript/JavaScript family (`package.json`); cargo = Rust (`Cargo.toml`);
/// python = `pyproject.toml`; java = Gradle build scripts. This is a MANIFEST-ecosystem
/// mapping, deliberately INDEPENDENT of [`token_enrichment_language`]'s resolver mapping —
/// they share token sets only by coincidence (a Python *enrichment* resolver, if added, must
/// not imply anything about the Python *manifest* reader). Keep them separate.
fn language_deps_ecosystem(token: &str) -> Option<&'static str> {
    Some(match token {
        "typescript" | "tsx" | "javascript" | "jsx" => "npm",
        "rust" => "cargo",
        "python" => "python",
        "java" => "java",
        _ => return None,
    })
}

/// DEPS-LIST-REWRITE-1 (§2.2): select the dependency `ecosystem` by the repo's DOMINANT indexed
/// language (files-table plurality), superseding the old "any TS/JS file present → npm" rule that
/// mislabelled Python-plurality django as npm and hoisted a fabricated `package.json` onto Maven
/// petclinic. `language_counts` MUST arrive sorted by count DESC (as
/// `query_file_count_by_language` returns). The plurality is taken over real *code* languages only
/// (those with a [`language_display_name`]) so config-file tokens (json/yaml) never win; the
/// dominant code language's manifest ecosystem is returned, or `"none-detected"` when the dominant
/// language has no manifest reader (C / C++ / Go / …) — which is honest, and triggers the
/// unattributed headline rather than a wrong-ecosystem guess.
pub(crate) fn dominant_deps_ecosystem(language_counts: &[(String, u64)]) -> &'static str {
    for (lang, _n) in language_counts {
        if language_display_name(lang).is_some() {
            return language_deps_ecosystem(lang).unwrap_or("none-detected");
        }
    }
    "none-detected"
}

/// HONEST-DEGRADATION-IMPL-2 (D2): the reader-context note for a repo with no dependency-manifest reader.
/// Names the language(s) and surfaces the EXISTING external-import count honestly — observed, not
/// attributed (no resolver ran; attribution numbers unchanged). `external_imports` is the already-counted
/// `total_external_imports`.
pub(crate) fn deps_reader_context_note(languages: &[String], external_imports: usize) -> String {
    let names = display_language_names(languages);
    let lang = if names.is_empty() {
        "this language".to_string()
    } else {
        names
    };
    format!(
        "no dependency-manifest reader for {lang} on this build; {external_imports} external includes \
         observed, not attributed to packages"
    )
}

/// HONEST-DEGRADATION-IMPL-2 (D5): is any of the three resolution-driven relationship axes (call-graph /
/// import-graph / change-impact) LOW? `dead_code` is excluded (a derived confidence axis, not a
/// resolution axis). Matches the ratified "when relationship reliability is LOW" trigger — LOW, not
/// merely non-HIGH (a MEDIUM axis emits no next-action).
pub(crate) fn relationship_reliability_is_low(
    reliability: &repo_graph_trust::types::TrustReliability,
) -> bool {
    use repo_graph_trust::types::ReliabilityLevel::LOW;
    reliability.call_graph.level == LOW
        || reliability.import_graph.level == LOW
        || reliability.change_impact.level == LOW
}

/// HONEST-DEGRADATION-IMPL-2 (D5): the honest next-action line for a LOW-relationship-reliability repo,
/// keyed on the repo's language(s) × the daemon's CONFIGURED resolvers. `configured` MUST come from
/// [`configured_resolver_languages`] / [`configured_resolver_languages_from_env`] (the SAME source
/// `handle_enrich` registers from) — passed in so this stays pure + unit-testable across the jdtls matrix.
/// `None` when no relationship axis is LOW (no noise on a resolved repo), or when no honest statement
/// applies (an unknown-only language set — no false promise either way).
///
/// One line, by priority:
/// 1. some present language maps to a CONFIGURED resolver (Rust / the TS-JS family always; Java iff JDTLS)
///    → suggest enrichment (a STATEMENT only — auto-run is ENRICH-LIFECYCLE-1). `rmap enrich` auto-selects
///    the resolvable languages, so "resolve more" is honestly partial, never a per-language promise.
/// 2. Java is present but its resolver is NOT configured (no JDTLS) → name the JDTLS requirement instead
///    of a blind enrich suggestion (a `languages:["java"]` enrich would error without JDTLS).
/// 3. no resolver exists for any present language (C / C++ / Python / …) → state the dead-end, naming the
///    language(s); never a false promise.
///
/// `configured_resolver_languages` / `configured_resolver_languages_from_env` are referenced by their
/// `crate::dispatch::…` path (they stay in `dispatch`); the doc-links above resolve through `dispatch`'s
/// `pub(crate) use` re-export of this module.
pub(crate) fn relationship_next_action_line(
    reliability: &repo_graph_trust::types::TrustReliability,
    repo_languages: &[String],
    configured: &[EnrichmentLanguage],
) -> Option<String> {
    if !relationship_reliability_is_low(reliability) {
        return None;
    }

    let enrichable_now = repo_languages
        .iter()
        .any(|t| match token_enrichment_language(t) {
            Some(lang) => configured.contains(&lang),
            None => false,
        });
    if enrichable_now {
        return Some(
            "relationship facts are low-confidence on this index; run `rmap enrich` to resolve more"
                .to_string(),
        );
    }

    let java_present = repo_languages
        .iter()
        .any(|t| token_enrichment_language(t) == Some(EnrichmentLanguage::Java));
    if java_present {
        // Java has a resolver, but it is JDTLS-gated and not configured on this build.
        return Some(
            "semantic enrichment for Java requires JDTLS (`jdtls_path` / `JDTLS_PATH`) configured; \
             until then these relationship facts remain low-confidence"
                .to_string(),
        );
    }

    // No resolver exists for any present language (C / C++ / Python / …).
    let names = display_language_names(repo_languages);
    if names.is_empty() {
        return None;
    }
    Some(format!(
        "no semantic-resolution path exists for {names} on this build; these relationship facts remain \
         low-confidence"
    ))
}

#[cfg(test)]
mod honest_degradation_tests {
    //! HONEST-DEGRADATION-IMPL-2 (D2 + D5) pure-helper unit tests. The full branch matrix (incl.
    //! Java-WITH-JDTLS, which the surface tests cannot exercise without a process-global env race) is
    //! covered here deterministically by INJECTING the configured-resolver set; the daemon-runtime
    //! SURFACE proofs (real index → `deps list` / `stats`) live in `tests/honest_degradation_impl2.rs`.
    use super::*;
    use crate::dispatch::configured_resolver_languages;
    use repo_graph_trust::types::{ReliabilityAxisScore, ReliabilityLevel, TrustReliability};

    fn axis(level: ReliabilityLevel) -> ReliabilityAxisScore {
        ReliabilityAxisScore {
            level,
            reasons: vec![],
        }
    }
    fn high() -> TrustReliability {
        TrustReliability {
            import_graph: axis(ReliabilityLevel::HIGH),
            call_graph: axis(ReliabilityLevel::HIGH),
            dead_code: axis(ReliabilityLevel::HIGH),
            change_impact: axis(ReliabilityLevel::HIGH),
        }
    }
    fn call_low() -> TrustReliability {
        let mut r = high();
        r.call_graph = axis(ReliabilityLevel::LOW);
        r
    }
    fn langs(ts: &[&str]) -> Vec<String> {
        ts.iter().map(|s| s.to_string()).collect()
    }
    /// The built-in-only configured set (no JDTLS), as `configured_resolver_languages(None)` returns.
    fn builtin_only() -> Vec<EnrichmentLanguage> {
        configured_resolver_languages(None)
    }
    /// The configured set WITH a JDTLS path present.
    fn with_jdtls() -> Vec<EnrichmentLanguage> {
        configured_resolver_languages(Some("/opt/jdtls"))
    }

    // ── the shared source ───────────────────────────────────────
    #[test]
    fn configured_source_is_builtin_plus_jdtls_gated_java() {
        assert_eq!(
            configured_resolver_languages(None),
            vec![EnrichmentLanguage::Rust, EnrichmentLanguage::TypeScript]
        );
        assert_eq!(
            configured_resolver_languages(Some("/p")),
            vec![
                EnrichmentLanguage::Rust,
                EnrichmentLanguage::TypeScript,
                EnrichmentLanguage::Java
            ]
        );
        // `Some("")` counts as configured (faithful to handle_enrich's `if let Some(path)`).
        assert!(configured_resolver_languages(Some("")).contains(&EnrichmentLanguage::Java));
    }

    // ── token → enrichment language (defers family to from_extension) ──
    #[test]
    fn token_map_covers_word_tokens_and_react_family() {
        assert_eq!(
            token_enrichment_language("typescript"),
            Some(EnrichmentLanguage::TypeScript)
        );
        assert_eq!(
            token_enrichment_language("javascript"),
            Some(EnrichmentLanguage::TypeScript)
        );
        // React `.tsx`/`.jsx` carry distinct tokens yet ARE the TS/JS family (one resolver).
        assert_eq!(
            token_enrichment_language("tsx"),
            Some(EnrichmentLanguage::TypeScript)
        );
        assert_eq!(
            token_enrichment_language("jsx"),
            Some(EnrichmentLanguage::TypeScript)
        );
        assert_eq!(
            token_enrichment_language("rust"),
            Some(EnrichmentLanguage::Rust)
        );
        assert_eq!(
            token_enrichment_language("java"),
            Some(EnrichmentLanguage::Java)
        );
        assert_eq!(token_enrichment_language("c"), None);
        assert_eq!(token_enrichment_language("cpp"), None);
        assert_eq!(token_enrichment_language("python"), None);
    }

    // ── §2.2 dominant-language ecosystem selection ──────────────
    fn counts(pairs: &[(&str, u64)]) -> Vec<(String, u64)> {
        pairs.iter().map(|(l, n)| (l.to_string(), *n)).collect()
    }

    #[test]
    fn dominant_ecosystem_picks_plurality_code_language() {
        // django: python plurality, minor tooling TS/JS → python, NOT npm.
        assert_eq!(
            dominant_deps_ecosystem(&counts(&[
                ("python", 900),
                ("typescript", 30),
                ("javascript", 12)
            ])),
            "python"
        );
        // petclinic: java plurality → java, NOT a fabricated npm/package.json.
        assert_eq!(
            dominant_deps_ecosystem(&counts(&[("java", 60), ("javascript", 3)])),
            "java"
        );
        // TS-plurality stays npm; pure-React (tsx) must not regress to none-detected.
        assert_eq!(
            dominant_deps_ecosystem(&counts(&[("typescript", 500)])),
            "npm"
        );
        assert_eq!(dominant_deps_ecosystem(&counts(&[("tsx", 500)])), "npm");
        assert_eq!(dominant_deps_ecosystem(&counts(&[("rust", 40)])), "cargo");
        // leveldb: C++ plurality → none-detected (headline fires), even with minor JS present.
        assert_eq!(
            dominant_deps_ecosystem(&counts(&[("cpp", 120), ("javascript", 2)])),
            "none-detected"
        );
        // Config-file tokens never win the plurality (only real code languages do).
        assert_eq!(
            dominant_deps_ecosystem(&counts(&[("json", 9999), ("python", 5)])),
            "python"
        );
        assert_eq!(dominant_deps_ecosystem(&[]), "none-detected");
    }

    #[test]
    fn deps_note_names_language_and_keeps_count() {
        let note = deps_reader_context_note(&langs(&["c"]), 56);
        assert!(
            note.contains("no dependency-manifest reader for C on this build"),
            "{note}"
        );
        assert!(
            note.contains("56 external includes observed, not attributed"),
            "{note}"
        );
        assert!(!note.contains("npm"), "must not mention npm: {note}");
    }

    // ── D5 line selection (configured set injected — deterministic, no env) ──
    #[test]
    fn d5_none_when_reliability_high() {
        assert!(
            relationship_next_action_line(&high(), &langs(&["rust"]), &builtin_only()).is_none()
        );
        assert!(relationship_next_action_line(&high(), &langs(&["c"]), &builtin_only()).is_none());
    }

    #[test]
    fn d5_builtin_languages_suggest_enrich() {
        for l in ["rust", "typescript", "javascript", "tsx", "jsx"] {
            let line =
                relationship_next_action_line(&call_low(), &langs(&[l]), &builtin_only()).unwrap();
            assert!(line.contains("rmap enrich"), "{l}: {line}");
        }
    }

    #[test]
    fn d5_c_repo_states_no_path() {
        let line =
            relationship_next_action_line(&call_low(), &langs(&["c"]), &builtin_only()).unwrap();
        assert!(
            line.contains("no semantic-resolution path exists for C"),
            "{line}"
        );
        assert!(
            !line.contains("rmap enrich"),
            "must not suggest enrich on C: {line}"
        );
    }

    #[test]
    fn d5_cpp_python_named_no_enrich() {
        let line =
            relationship_next_action_line(&call_low(), &langs(&["cpp", "python"]), &builtin_only())
                .unwrap();
        assert!(line.contains("C++/Python"), "{line}");
        assert!(!line.contains("rmap enrich"), "{line}");
    }

    #[test]
    fn d5_java_without_jdtls_says_configure_jdtls() {
        // The false-promise guard: Java with NO configured resolver must NOT suggest a (blind) enrich.
        let line =
            relationship_next_action_line(&call_low(), &langs(&["java"]), &builtin_only()).unwrap();
        assert!(line.contains("requires JDTLS"), "{line}");
        assert!(line.contains("JDTLS_PATH"), "{line}");
        assert!(!line.contains("rmap enrich"), "{line}");
    }

    #[test]
    fn d5_java_with_jdtls_suggests_enrich() {
        // With JDTLS configured (Java in the set), enrich IS the remedy.
        let line =
            relationship_next_action_line(&call_low(), &langs(&["java"]), &with_jdtls()).unwrap();
        assert!(line.contains("rmap enrich"), "{line}");
    }

    #[test]
    fn d5_mixed_builtin_and_no_resolver_prefers_enrich() {
        // Rust + C: enrichment helps the Rust part → suggest it ("resolve more", honestly partial).
        let line =
            relationship_next_action_line(&call_low(), &langs(&["c", "rust"]), &builtin_only())
                .unwrap();
        assert!(line.contains("rmap enrich"), "{line}");
    }

    #[test]
    fn d5_unknown_only_language_emits_nothing() {
        assert!(
            relationship_next_action_line(&call_low(), &langs(&["other"]), &builtin_only())
                .is_none()
        );
        assert!(relationship_next_action_line(&call_low(), &[], &builtin_only()).is_none());
    }

    #[test]
    fn d5_triggers_on_import_or_change_axis_and_not_medium() {
        let mut import_low = high();
        import_low.import_graph = axis(ReliabilityLevel::LOW);
        assert!(
            relationship_next_action_line(&import_low, &langs(&["rust"]), &builtin_only())
                .is_some()
        );
        let mut medium = high();
        medium.call_graph = axis(ReliabilityLevel::MEDIUM);
        assert!(relationship_next_action_line(&medium, &langs(&["c"]), &builtin_only()).is_none());
    }
}
