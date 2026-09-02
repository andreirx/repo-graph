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
pub(crate) fn token_enrichment_language(token: &str) -> Option<EnrichmentLanguage> {
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

/// ORIENT-SMALL-ENRICH-1 / HONEST-DEGRADATION-IMPL-2 (D5): is a materially-present code-language `token`
/// ENRICHABLE NOW — does THIS build have a CONFIGURED resolver for it? The ONE definition of "enrichable
/// now": the language has a resolver at all ([`token_enrichment_language`] `Some`) AND that resolver is in
/// the daemon's `configured` set (`handle_enrich` registers EXACTLY this set;
/// `dispatch::configured_resolver_languages` is its single source). C / C++ / Python / … have no resolver on
/// ANY build → never enrichable; Java is enrichable ONLY when JDTLS is configured; Rust / TS / JS always.
///
/// Sole consumers: [`relationship_next_action_line`] (the D5 CTA — whose inline `is_enrichable` closure this
/// REPLACES, so "enrichable now" has one home) and [`in_flight_pass_can_apply`]. Axis: none — a shared
/// predicate over the existing resolver-family × configured facts, not a new seam. Rejected simpler: keep the
/// closure inline AND re-spell the same `match` at the in-flight gate — rejected because the slice forbids
/// re-deriving the enrichable definition away from one home.
pub(crate) fn token_is_enrichable_now(token: &str, configured: &[EnrichmentLanguage]) -> bool {
    match token_enrichment_language(token) {
        Some(lang) => configured.contains(&lang),
        None => false,
    }
}

/// Reader-facing display name for an indexer language token (`None` for a token with no stable display
/// name, so a raw internal token is never shown to the reader). Used by the D2 deps note and the D5
/// no-resolution-path line.
///
/// `pub(crate)` so the DEPS-ATTRIB-2 §2.4 `deps_ecosystem_presence` module reuses the SAME code-file
/// token set (one materiality definition, never re-derived) — see `secondary_material_ecosystems`.
pub(crate) fn language_display_name(token: &str) -> Option<&'static str> {
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
///
/// `pub(crate)` so the DEPS-ATTRIB-2 §2.4 `deps_ecosystem_presence` module reuses this exact
/// language→ecosystem map (never a second copy) — see `secondary_material_ecosystems`.
pub(crate) fn language_deps_ecosystem(token: &str) -> Option<&'static str> {
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

/// The minimum file-share a language must hold among a repo's CODE files before the D5 next-action
/// speaks to it — CONTRADICTION-SWEEP-1 §5 (operator ruling CS1-5, OPTION B, 2026-08-28). A language
/// below this is incidental tooling, not part of the repo's semantic surface, so it must NOT drive the
/// remedy either way.
///
/// This gate is applied SYMMETRICALLY (to the enrichable AND the non-enrichable side), which is what
/// makes the ruling's stated django outcome hold: django is 2904 Python / 111 JavaScript files
/// (VERIFIED 2026-08-28) — its JavaScript is ~3.7%, an enrichable family present but incidental, so it
/// must NOT trip an enrich CTA; django reads as Python-only → the honest no-path sentence stands alone
/// with NO CTA. glamCRM's TS/JS is a demonstrable ~90% half (VERIFIED), well above the gate, so its true
/// CTA survives while its ~10% Java draws its own JDTLS truth. Integer comparison `count * 10 >= total`
/// == `share >= 10%`.
///
/// `pub(crate)` so the DEPS-ATTRIB-2 §2.4 `deps_ecosystem_presence` module gates on the SAME
/// threshold (never a re-derived one) — see `secondary_material_ecosystems`.
pub(crate) const MATERIAL_LANGUAGE_SHARE_NUM: u64 = 10;

/// A CODE language materially present in the repo (a token with a reader display name AND a file share
/// at or above [`MATERIAL_LANGUAGE_SHARE_NUM`]%), with its display name. `total_code_files` is the sum
/// of file counts over CODE languages only (config-file tokens like json/yaml never dilute the share).
pub(crate) struct MaterialLanguage {
    pub(crate) token: String,
    pub(crate) display: &'static str,
}

/// The materially-present CODE languages of `language_counts` (count-DESC as
/// `query_file_count_by_language` returns), each ≥ the [`MATERIAL_LANGUAGE_SHARE_NUM`]% code-file gate,
/// in the same count-DESC order. Config-file tokens (no display name) are excluded from BOTH the share
/// denominator and the result.
pub(crate) fn material_code_languages(language_counts: &[(String, u64)]) -> Vec<MaterialLanguage> {
    let total_code: u64 = language_counts
        .iter()
        .filter(|(l, _)| language_display_name(l).is_some())
        .map(|(_, n)| *n)
        .sum();
    if total_code == 0 {
        return Vec::new();
    }
    language_counts
        .iter()
        .filter_map(|(l, n)| {
            let display = language_display_name(l)?;
            (n * MATERIAL_LANGUAGE_SHARE_NUM >= total_code).then_some(MaterialLanguage {
                token: l.clone(),
                display,
            })
        })
        .collect()
}

/// CHECK-SIGNAL-1 (§2.1): the PERMANENT call-graph-resolution ceiling of a repo — `Some(langs)` iff
/// EVERY materially-present code language (the SAME ≥10%-of-code-files gate [`material_code_languages`]
/// the D5 CTA uses, REUSED so the check verdict and the CTA read ONE materiality definition) has NO
/// resolver on ANY build. "No resolver on any build" is exactly [`token_enrichment_language`] returning
/// `None` (C / C++ / Python / Go / …) — a BUILD-INDEPENDENT fact, deliberately NOT gated on the
/// daemon's `configured` set: a language whose resolver merely isn't wired here (Java without JDTLS)
/// is ACTIONABLE (the reader can enable it), NOT a permanent ceiling, so it keeps `check`'s degrading
/// verdict. `language_counts` MUST arrive count-DESC (as `query_file_count_by_language` returns).
///
/// `None` when NOT a permanent ceiling: no materially-present code language at all (unknown-/config-only),
/// or at least one materially-present language HAS a resolver (TS/JS/Rust/Java) → the gap is actionable,
/// so the pre-CHECK-SIGNAL-1 degrading verdict stands. `Some` carries the reader display names as a
/// STRUCTURED list (`["C++"]`, `["Python"]`, `["C", "C++"]`), sorted + deduped for determinism and
/// guaranteed non-empty — NOT a pre-joined prose string. `dispatch::handle_check` maps this Option into
/// the boundary DTO `repo_graph_agent::dto::ceiling_fact::CeilingFact` at the injection site
/// (`Some(langs)` → `Ceiling { languages }`, `None` → `NoCeiling`; a FAILED read of the underlying
/// breakdown becomes `Unknown { reason }` there, never reaching this pure function). This function is
/// consulted ONLY on a successful read, so it models just the two affirmative capability outcomes.
///
/// Consumers: the daemon's `handle_check` (which wraps + injects this into `CheckInput`);
/// [`relationship_next_action_line`] in this module (ORIENT-SMALL-ENRICH-1 §3 — the orient/stats
/// no-resolution CTA CONSUMES this same set instead of re-deriving its own, so the two surfaces render
/// ONE phrasing and cannot drift); and this module's tests. Axis: the resolver-path capability — a
/// demonstrated volatile per-language mechanism
/// (the C/C++/Python-no-resolver reality vs the TS/Rust/Java-resolver reality). Rejected simpler: inline
/// the `all(token_enrichment_language(..).is_none())` check in `handle_check` — rejected because it would
/// re-derive the materiality gate + the resolver-family vocabulary away from their one home here, the
/// exact "never re-derived" the slice forbids.
pub(crate) fn call_graph_ceiling_languages(
    language_counts: &[(String, u64)],
) -> Option<Vec<String>> {
    let material = material_code_languages(language_counts);
    if material.is_empty() {
        return None;
    }
    if material
        .iter()
        .all(|m| token_enrichment_language(&m.token).is_none())
    {
        let mut names: Vec<&'static str> = material.iter().map(|m| m.display).collect();
        names.sort_unstable();
        names.dedup();
        Some(names.into_iter().map(str::to_string).collect())
    } else {
        None
    }
}

/// ORIENT-SMALL-ENRICH-1 (§1a/§2.1): can the in-flight AUTO enrichment pass raise THIS repo's resolution
/// figures? The daemon's `enrichment_in_flight_for_db` is repo-scoped but capability-BLIND —
/// `spawn_auto_enrich` calls `enter_flight` for EVERY repo the instant a pass spawns, BEFORE the
/// per-language skip decision — so on a repo the pass will skip, "in flight" is a true-but-irrelevant
/// daemon fact. Rendering "resolution figures may rise" from it is the false promise this slice removes.
///
/// `Ok(true)` iff ≥1 materially-present code language ([`material_code_languages`] — the ONE materiality
/// gate) is ENRICHABLE NOW ([`token_is_enrichable_now`] — the SAME configured-resolver predicate the D5 CTA
/// uses). This is EXACTLY slice §2.1's "≥1 materially-present enrichable language", and it is deliberately
/// STRICTER than the `NoCeiling` ceiling verdict: that verdict also covers (a) a repo with NO
/// materially-present code language at all (config-only → `material_code_languages` empty → `any` is `false`)
/// and (b) a repo whose only resolver-bearing language is NOT configured on this build (Java without JDTLS).
/// In BOTH the running pass raises nothing, so neither may render the promise. Pass-applicability is a
/// DISTINCT fact from the ceiling verdict, not a re-spelling of it (reviewer review-0).
///
/// A FAILED count read (`Err`) is PRESERVED and handed BACK to the caller, NEVER collapsed to a boolean:
/// the applicability of an in-flight pass CANNOT be classified from a read that did not happen (STANDING
/// HONESTY RULE #1 — a classified fallible read is unknown-WITH-REASON, never swallowed to a sentinel;
/// reviewer review-1 F1). Each surface then surfaces the reason honestly: `handle_orient` /
/// `handle_reliability` return the established structured handler error (they have no unknown-enrichment
/// render channel — the frozen [`EnrichmentState`](repo_graph_agent::EnrichmentState) sum carries no
/// `Unknown`), while `handle_check` maps the SAME read's failure to the rendered `CeilingFact::Unknown`
/// it already computes (CHECK-SIGNAL-1 ratified `ceiling-read-unknown`) and leaves the override off — the
/// reason is surfaced either way, never hidden.
///
/// Sole consumers: `dispatch::handle_orient` + `dispatch::handle_check` + `handlers::reliability` + this
/// module's tests. Axis: none — composes the existing materiality gate × enrichable predicate. Rejected
/// simpler: return a bare `bool` mapping `Err → false` (build-1) — rejected because it CLASSIFIES the
/// applicability from a failed read, hiding the reason (reviewer review-1 F1).
pub(crate) fn in_flight_pass_can_apply(
    counts: Result<Vec<(String, u64)>, String>,
    configured: &[EnrichmentLanguage],
) -> Result<bool, String> {
    let counts = counts?;
    Ok(material_code_languages(&counts)
        .iter()
        .any(|m| token_is_enrichable_now(&m.token, configured)))
}

/// RESOURCE-HONESTY-1 (§2.1/§2.2): the repo's MATERIALLY-present code languages (the SAME
/// ≥10%-of-code-files gate [`material_code_languages`] — REUSED so resource coverage and the
/// call-graph ceiling / deps notes read ONE materiality definition) that this build has NO
/// resource-access detector for. These are the languages whose resources `resource list` cannot
/// see, so the surface names them instead of blaming the repo for the tool's blind spot.
///
/// `covers` is the registry-backed coverage predicate
/// (`repo_graph_repo_index::resource_detection_covers`), injected so this stays pure + testable —
/// the SAME daemon→pure injected-fact shape as `configured` on [`relationship_next_action_line`].
/// `language_counts` MUST arrive count-DESC (as `query_file_count_by_language` returns). Returns
/// sorted + deduped reader display names (empty when every material language is covered, or when no
/// code language is materially present). The caller pairs this with the build-static covered-language
/// list; a FAILED counts read is the caller's `Unknown`-with-reason branch, never a silent empty here.
///
/// Sole consumers: the daemon's `handle_resource_list` and this module's tests. Axis: the
/// resource-detector coverage capability — a demonstrated volatile per-language mechanism (the
/// Rust/Go-no-detector reality vs the TS/Python/Java/C/C++-detector reality). Rejected simpler:
/// inline the filter in `handle_resource_list` — rejected because it would re-derive the materiality
/// gate away from its one home here.
pub(crate) fn resource_uncovered_material_languages(
    language_counts: &[(String, u64)],
    covers: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut names: Vec<&'static str> = material_code_languages(language_counts)
        .iter()
        .filter(|m| !covers(&m.token))
        .map(|m| m.display)
        .collect();
    names.sort_unstable();
    names.dedup();
    names.into_iter().map(str::to_string).collect()
}

/// CYCLE-HONESTY-1 (§2.4, ts-caveat-basis C1 REPO-level, operator ruling 2026-08-28 + review-2): is TS/JS
/// a MATERIALLY-present code language of this repo — the SAME ≥10%-of-code-files gate
/// [`material_code_languages`] applies for the D5 next-action, REUSED (never a re-derived threshold) so the
/// cycles type-only caveat and the enrich CTA speak from ONE materiality definition. `language_counts` MUST
/// arrive count-DESC (as `query_file_count_by_language` returns). "Any TS/JS file present" is NOT enough: a
/// ~3.7% incidental JS (django is 2904 Python / 111 JavaScript, VERIFIED 2026-08-28) is BELOW the gate →
/// `false`, so the caveat does not fire on a Python repo with tooling JS; a TS/JS-dominant repo → `true`.
/// Sole current consumer: the cycles routes' `snapshot_has_material_ts_js` (livegraph_feed) — one function,
/// no axis of variation, the rejected simpler alternative (inline the `matches!` at each call site) would
/// duplicate the TS/JS token vocabulary across five routes.
pub(crate) fn repo_has_material_ts_js(language_counts: &[(String, u64)]) -> bool {
    material_code_languages(language_counts)
        .iter()
        .any(|m| is_ts_js_language_token(&m.token))
}

/// Is an indexer `files.language` token a member of the TypeScript/JavaScript family
/// (`typescript` | `tsx` | `javascript` | `jsx`)? The ONE home of that token vocabulary,
/// so the repo-level cycles caveat gate ([`repo_has_material_ts_js`]) and the per-cycle-
/// membership gate ([`crate::cycle_output::any_cycle_member_is_ts_js`], ZEROSTATE-SCOPE-1
/// §2.3) never re-spell it. `pub(crate)` for the cycle-membership caller.
pub(crate) fn is_ts_js_language_token(token: &str) -> bool {
    matches!(token, "typescript" | "tsx" | "javascript" | "jsx")
}

/// ZEROSTATE-SCOPE-1 (§2.1): the repo's MATERIALLY-present code languages (the SAME
/// ≥10%-of-code-files gate [`material_code_languages`] — REUSED so the surfaces/boundaries
/// roster reads ONE materiality definition, never a re-derived one) that this build has NO
/// HTTP SURFACE detector for, rendered as reader-frame gap names. This is the PER-REPO
/// "no detector for X" clause that replaces the build-static blob: leveldb (C/C++) names
/// its C/C++ truth, a materially-Python repo names "Django URLconf routes", a covered-only
/// repo (Java / TS/JS) names nothing → the caller omits the clause. No repo wears another's
/// sentence.
///
/// `covers` is the registry-adjacent coverage predicate
/// (`repo_graph_repo_index::surface_coverage::http_surface_detection_covers`); `gap_name`
/// is the framework-specific display override
/// (`…::http_surface_named_gap_for` — `Some("Django URLconf routes")` for Python, `None`
/// otherwise). Both injected so this stays pure + testable, the SAME daemon→pure shape as
/// [`resource_uncovered_material_languages`]. `language_counts` MUST arrive count-DESC (as
/// `query_file_count_by_language` returns). Returns sorted + deduped names (empty when every
/// material language is HTTP-surface-covered, or no code language is materially present); a
/// FAILED counts read is the caller's `Unknown`-with-reason branch, never a silent empty here.
///
/// Sole consumers: `surface_coverage_read::surface_coverage_json` and this module's tests.
/// Axis: the HTTP-surface-detector coverage capability (the Python/C/C++-no-detector reality
/// vs the Java/TS-JS-detector reality). Rejected simpler: inline the filter at the read site —
/// rejected because it would re-derive the materiality gate away from its one home here.
pub(crate) fn surface_uncovered_material_gaps(
    language_counts: &[(String, u64)],
    covers: impl Fn(&str) -> bool,
    gap_name: impl Fn(&str) -> Option<&'static str>,
) -> Vec<String> {
    let mut names: Vec<String> = material_code_languages(language_counts)
        .iter()
        .filter(|m| !covers(&m.token))
        .map(|m| gap_name(&m.token).unwrap_or(m.display).to_string())
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

/// HONEST-DEGRADATION-IMPL-2 (D5) + CONTRADICTION-SWEEP-1 §5: the honest next-action line for a
/// LOW-relationship-reliability repo, keyed on the repo's MATERIALLY-PRESENT languages (≥10% of code
/// files — see [`material_code_languages`]) × the daemon's CONFIGURED resolvers. `language_counts` MUST
/// arrive count-DESC (as [`query_file_count_by_language`] returns). `configured` MUST come from
/// [`configured_resolver_languages`] / [`configured_resolver_languages_from_env`] (the SAME source
/// `handle_enrich` registers from) — passed in so this stays pure + unit-testable across the jdtls matrix.
/// `None` when no relationship axis is LOW (no noise on a resolved repo), or when no code language is
/// materially present (an unknown-only / config-only set — no false promise either way).
///
/// Per CS1-5 OPTION B (operator-ratified 2026-08-28) the line is PER-LANGUAGE truthful:
/// 1. ≥1 materially-present language is enrichable now → CTA "run `rmap enrich` — resolves <those
///    languages>", PLUS one clause per materially-present NON-enrichable language stating ITS OWN truth
///    (Java without JDTLS → set `JDTLS_PATH`; C / C++ / Python / … → no resolver exists on any build).
///    This kills django's false CTA WITHOUT killing glamCRM's true one: only the false clauses die.
/// 2. NO materially-present language is enrichable → NO CTA; the no-path (or JDTLS) sentence for the
///    DOMINANT (plurality) code language stands alone.
///
/// `configured_resolver_languages` / `configured_resolver_languages_from_env` are referenced by their
/// `crate::dispatch::…` path (they stay in `dispatch`); the doc-links above resolve through `dispatch`'s
/// `pub(crate) use` re-export of this module.
pub(crate) fn relationship_next_action_line(
    reliability: &repo_graph_trust::types::TrustReliability,
    language_counts: &[(String, u64)],
    configured: &[EnrichmentLanguage],
) -> Option<String> {
    if !relationship_reliability_is_low(reliability) {
        return None;
    }

    let material = material_code_languages(language_counts);
    if material.is_empty() {
        return None; // unknown-only / config-only — no honest statement applies.
    }

    // The ONE home of "enrichable now" (configured resolver present) — shared with the in-flight gate.
    let is_enrichable = |token: &str| token_is_enrichable_now(token, configured);

    // The enrichable languages present now (display names, sorted + deduped so the TS/JS family reads as
    // its two present member names, not a repeated resolver identity).
    let mut enrichable_names: Vec<&'static str> = material
        .iter()
        .filter(|m| is_enrichable(&m.token))
        .map(|m| m.display)
        .collect();
    enrichable_names.sort_unstable();
    enrichable_names.dedup();

    if !enrichable_names.is_empty() {
        // CS1-5 §5.1: the true CTA, plus each materially-present non-enrichable language's own truth.
        let mut line = format!(
            "relationship facts are low-confidence on this index; run `rmap enrich` — resolves {}",
            enrichable_names.join("/")
        );
        for clause in non_enrichable_clauses(&material, &is_enrichable) {
            line.push_str(&clause);
        }
        return Some(line);
    }

    // CS1-5 §5.2: nothing enrichable is materially present → no CTA; speak the no-resolution truth.
    let dominant = &material[0];
    if token_enrichment_language(&dominant.token) == Some(EnrichmentLanguage::Java) {
        // Java has a resolver, but it is JDTLS-gated and not configured on this build.
        return Some(
            "semantic enrichment for Java requires JDTLS (`jdtls_path` / `JDTLS_PATH`) configured; \
             until then these relationship facts remain low-confidence"
                .to_string(),
        );
    }
    // ORIENT-SMALL-ENRICH-1 (§3): name the no-resolution set by CONSUMING the SAME source check/dead
    // read — `call_graph_ceiling_languages` — instead of re-deriving a second display set here (the
    // drift the slice removes: orient/stats said "C" on a `.h`-plurality C/C++ repo while check/dead
    // said "C/C++"). `Some(langs)` IS that gate's own sorted+deduped display-name list — structurally
    // one source, one phrasing, so the two surfaces cannot drift again. This is the reported bug's repo
    // whenever it is a PURE permanent ceiling (every material language no-resolver: leveldb's C/C++,
    // django's Python), which is exactly the case §3 targets. `None` here is only the exotic residue —
    // a resolver-bearing language materially co-present but unconfigured (only Java is possible at §5.2:
    // TS/JS/Rust are always configured and would have taken the enrichable branch above) — so the repo
    // is NOT a pure ceiling; fall back to the dominant no-resolver language's name (the pre-slice
    // baseline), NOT a re-derived multi-language set. Widening that residual mixed case is out of this
    // slice's scope (§3 is the pure-ceiling drift only).
    let named = match call_graph_ceiling_languages(language_counts) {
        Some(langs) => langs.join("/"),
        None => dominant.display.to_string(),
    };
    Some(format!(
        "no semantic-resolution path exists for {named} on this build; these relationship facts remain \
         low-confidence"
    ))
}

/// CONTRADICTION-SWEEP-1 (review-1 #1): the next-action for a repo whose per-language file counts were
/// requested but the READ FAILED. When a relationship axis is LOW the reader is OWED a next-action; a
/// failed language-breakdown read must therefore render an **unknown-with-reason** line, NOT silently
/// omit the remedy (which would misclassify the repo as "nothing to say" — the STANDING HONESTY RULE:
/// a rendered/classified fallible read is unknown-with-reason, never dropped). `None` only when no
/// relationship axis is LOW (a resolved repo genuinely has no next-action).
///
/// This is the ONE site both `stats`/deps (dispatch) and `orient` (orient_coherence) route the
/// counts-read result through, so the two surfaces render the SAME line on success AND the SAME
/// unknown-with-reason on failure — the cross-surface coherence this slice exists to hold.
pub(crate) fn relationship_next_action_line_or_read_error(
    reliability: &repo_graph_trust::types::TrustReliability,
    language_counts: Result<Vec<(String, u64)>, String>,
    configured: &[EnrichmentLanguage],
) -> Option<String> {
    if !relationship_reliability_is_low(reliability) {
        return None; // resolved repo — no next-action, success or failure irrelevant.
    }
    match language_counts {
        Ok(counts) => relationship_next_action_line(reliability, &counts, configured),
        Err(reason) => Some(format!(
            "relationship facts are low-confidence on this index; the per-language remedy is \
             unavailable — could not read the language breakdown ({reason})"
        )),
    }
}

/// CS1-5 §5.1: one reader-frame clause per materially-present NON-enrichable code language, in display
/// order, each stating that language's OWN remedy truth. Empty when every material language is enrichable.
fn non_enrichable_clauses(
    material: &[MaterialLanguage],
    is_enrichable: &impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut langs: Vec<&MaterialLanguage> = material
        .iter()
        .filter(|m| !is_enrichable(&m.token))
        .collect();
    langs.sort_unstable_by_key(|m| m.display);
    langs
        .into_iter()
        .map(|m| match token_enrichment_language(&m.token) {
            // Java: a resolver exists but is JDTLS-gated and unconfigured on this build.
            Some(EnrichmentLanguage::Java) => format!(
                "; {}: set `JDTLS_PATH` to enable its resolver (no semantic resolver is configured for \
                 it on this build)",
                m.display
            ),
            // A resolver exists but is not configured (unreachable with the built-in Rust/TS set, which
            // is always configured — handled honestly rather than silently dropped).
            Some(_) => format!(
                "; {}: its semantic resolver is not configured on this build",
                m.display
            ),
            // No resolver exists for this language on ANY build (C / C++ / Python / Go / …).
            None => format!(
                "; no semantic-resolution path exists for {} on this build",
                m.display
            ),
        })
        .collect()
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
    /// Equal-share language counts (each token 100 files) — every listed language clears the ≥10%
    /// material gate, so a `langs(&["c", "rust"])` reads as "C and Rust both materially present",
    /// preserving the pre-CS1-5 single-/dual-language test intent under the counts signature.
    fn langs(ts: &[&str]) -> Vec<(String, u64)> {
        ts.iter().map(|s| (s.to_string(), 100)).collect()
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

    // ── ORIENT-SMALL-ENRICH-1: "enrichable now" = has a CONFIGURED resolver (one home) ──
    #[test]
    fn token_is_enrichable_now_is_configured_gated() {
        // Built-in resolvers (Rust / TS / JS family) are always enrichable-now.
        assert!(token_is_enrichable_now("typescript", &builtin_only()));
        assert!(token_is_enrichable_now("rust", &builtin_only()));
        assert!(token_is_enrichable_now("jsx", &builtin_only()));
        // No resolver on any build → never enrichable, regardless of the configured set.
        assert!(!token_is_enrichable_now("c", &with_jdtls()));
        assert!(!token_is_enrichable_now("python", &with_jdtls()));
        // Java is enrichable ONLY when JDTLS is configured (the configured-gating that separates
        // pass-applicability from the build-independent ceiling fact).
        assert!(!token_is_enrichable_now("java", &builtin_only()));
        assert!(token_is_enrichable_now("java", &with_jdtls()));
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
        let note = deps_reader_context_note(&["c".to_string()], 56);
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
        // Nothing enrichable is materially present → no CTA; the no-path sentence names the DOMINANT
        // (plurality) language and stands alone (CS1-5 §5.2). C++ leads the count here.
        let line = relationship_next_action_line(
            &call_low(),
            &counts(&[("cpp", 120), ("python", 40)]),
            &builtin_only(),
        )
        .unwrap();
        assert!(
            line.contains("no semantic-resolution path exists for C++"),
            "{line}"
        );
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
    fn d5_mixed_enrichable_and_no_resolver_is_per_language_truthful() {
        // CS1-5 OPTION B (operator-ratified 2026-08-28, supersedes the pre-slice "any-enrichable →
        // resolve more" rule — that behavior change IS the point). Rust + C, both material: the CTA
        // names the enrichable part (Rust) AND states C's own no-path truth in the SAME line. Only the
        // false clause dies; the true CTA survives.
        let line =
            relationship_next_action_line(&call_low(), &langs(&["c", "rust"]), &builtin_only())
                .unwrap();
        assert!(
            line.contains("run `rmap enrich` — resolves Rust"),
            "true CTA names the enrichable language: {line}"
        );
        assert!(
            line.contains("no semantic-resolution path exists for C on this build"),
            "the non-enrichable language states its own truth: {line}"
        );
    }

    // CS1-5 §5.1 (glamCRM shape): a Java-plurality repo whose TS/JS half is materially present renders
    // the TRUE enrich CTA for the TS/JS family AND Java's own JDTLS truth — dominant-gating would have
    // killed the true CTA; B keeps it and adds the honest Java clause.
    #[test]
    fn d5_java_plus_material_ts_js_gets_cta_plus_java_jdtls_clause() {
        let line = relationship_next_action_line(
            &call_low(),
            // java ~10%, ts ~27%, js ~63% — all material (glamCRM's verified rough shape).
            &counts(&[("javascript", 1658), ("typescript", 699), ("java", 273)]),
            &builtin_only(),
        )
        .unwrap();
        assert!(line.contains("run `rmap enrich` — resolves"), "{line}");
        assert!(
            line.contains("JavaScript") && line.contains("TypeScript"),
            "the enrichable family members are both named: {line}"
        );
        assert!(
            line.contains("Java: set `JDTLS_PATH`"),
            "Java draws its own JDTLS truth, not a blind enrich promise: {line}"
        );
    }

    // CS1-5 §5.2 (django shape): a Python-dominant repo whose only enrichable family (JavaScript) is
    // BELOW the ≥10% material gate (django is ~3.7% JS, VERIFIED) reads as Python-only → NO enrich CTA;
    // the honest no-path sentence for Python stands alone. This is the measured false CTA dying.
    #[test]
    fn d5_django_minor_tooling_js_below_gate_gets_no_cta() {
        let line = relationship_next_action_line(
            &call_low(),
            &counts(&[("python", 2904), ("javascript", 111)]),
            &builtin_only(),
        )
        .unwrap();
        assert!(
            !line.contains("rmap enrich"),
            "incidental (<10%) JS must NOT trip an enrich CTA: {line}"
        );
        assert!(
            line.contains("no semantic-resolution path exists for Python on this build"),
            "the dominant language's honest no-path sentence stands alone: {line}"
        );
    }

    // review-1 #1: a language-breakdown READ FAILURE on a LOW axis renders an UNKNOWN-WITH-REASON
    // next-action, carrying the error reason — NEVER a silent omission (which would misclassify the repo
    // as "nothing to say" and hide a remedy the reader is owed). Both `stats`/deps and `orient` route
    // their read result through this ONE wrapper, so they render the SAME failure line.
    #[test]
    fn read_error_on_low_axis_renders_unknown_with_reason() {
        let line = relationship_next_action_line_or_read_error(
            &call_low(),
            Err("db locked".to_string()),
            &builtin_only(),
        )
        .expect("a LOW axis owes the reader a next-action even when the counts read fails");
        assert!(
            line.contains("per-language remedy is unavailable"),
            "must render unknown-with-reason, not omit: {line}"
        );
        assert!(
            line.contains("db locked"),
            "the read-failure reason is preserved: {line}"
        );
        assert!(
            !line.contains("rmap enrich"),
            "a failed read must not fabricate a remedy it could not compute: {line}"
        );
    }

    // review-1 #1: a read failure on a RESOLVED (no LOW axis) repo stays `None` — a healthy repo is owed
    // no next-action, so the failure is irrelevant (and the hot-path guard means the read is never issued).
    #[test]
    fn read_error_on_high_axis_is_none() {
        assert!(relationship_next_action_line_or_read_error(
            &high(),
            Err("db locked".to_string()),
            &builtin_only(),
        )
        .is_none());
    }

    // review-1 #1: on success the wrapper delegates to `relationship_next_action_line` byte-for-byte, so
    // the Ok path is unchanged from the direct helper.
    #[test]
    fn read_ok_delegates_to_line_helper() {
        let counts = langs(&["rust"]);
        assert_eq!(
            relationship_next_action_line_or_read_error(
                &call_low(),
                Ok(counts.clone()),
                &builtin_only()
            ),
            relationship_next_action_line(&call_low(), &counts, &builtin_only())
        );
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

    // ── CHECK-SIGNAL-1 (§2.1): the permanent no-resolver ceiling classification ──

    #[test]
    fn ceiling_when_all_material_languages_have_no_resolver() {
        // leveldb (pure C++), and a C/C++ mix → permanent ceiling, languages named + sorted.
        assert_eq!(
            call_graph_ceiling_languages(&counts(&[("cpp", 120), ("javascript", 2)])),
            Some(vec!["C++".to_string()]),
            "C++-dominant with incidental JS is a ceiling (JS below the material gate)"
        );
        assert_eq!(
            call_graph_ceiling_languages(&counts(&[("c", 80), ("cpp", 60)])),
            Some(vec!["C".to_string(), "C++".to_string()]),
            "C and C++ both material, neither has a resolver → ceiling naming both, sorted"
        );
        // django: Python-dominant, JS ~3.7% (below gate) → Python-only ceiling.
        assert_eq!(
            call_graph_ceiling_languages(&counts(&[("python", 2904), ("javascript", 111)])),
            Some(vec!["Python".to_string()]),
            "Python-dominant with incidental JS is a Python-only ceiling"
        );
    }

    #[test]
    fn not_ceiling_when_any_material_language_has_a_resolver() {
        // A resolver EXISTS for TS/JS/Rust/Java (build-independent) → actionable, never a ceiling.
        assert!(
            call_graph_ceiling_languages(&langs(&["typescript"])).is_none(),
            "TS has a resolver → actionable, not a ceiling"
        );
        assert!(call_graph_ceiling_languages(&langs(&["rust"])).is_none());
        // Java WITHOUT JDTLS: a resolver EXISTS (configurable) → actionable, not a permanent ceiling.
        assert!(
            call_graph_ceiling_languages(&langs(&["java"])).is_none(),
            "Java's resolver is configurable (JDTLS) → not a permanent ceiling"
        );
        // Mixed: materially-present TS/JS half (glamCRM shape) keeps the actionable verdict even with
        // material Java/no-resolver-langs present — the enrichable side governs.
        assert!(
            call_graph_ceiling_languages(&counts(&[
                ("javascript", 1658),
                ("typescript", 699),
                ("java", 273)
            ]))
            .is_none(),
            "a material TS/JS half makes the repo actionable, not a ceiling"
        );
        // Rust + C both material: Rust's resolver exists → actionable (the D5 line still names C's
        // own no-path clause, but check treats the repo as degrading, not ceiling).
        assert!(call_graph_ceiling_languages(&langs(&["c", "rust"])).is_none());
    }

    #[test]
    fn not_ceiling_when_no_material_code_language() {
        // Unknown-/config-only: nothing materially present → no ceiling statement applies.
        assert!(call_graph_ceiling_languages(&langs(&["other"])).is_none());
        assert!(call_graph_ceiling_languages(&[]).is_none());
        // Config-file tokens never establish materiality on their own.
        assert!(call_graph_ceiling_languages(&counts(&[("json", 9999)])).is_none());
    }

    // ── ORIENT-SMALL-ENRICH-1 (§3): the no-path line names the FULL no-resolver set, one source ──

    #[test]
    fn no_path_line_names_full_ceiling_set_not_just_dominant() {
        // A C++ repo whose `.h` headers (→ token "c") outnumber its `.cpp` (→ token "cpp"): the DOMINANT
        // language is C, so the pre-fix line said "…for C". check/dead name the full ceiling set "C/C++"
        // (the CeilingFact.languages the gate hands them). The line must now agree: "C/C++", one phrasing.
        let line = relationship_next_action_line(
            &call_low(),
            &counts(&[("c", 60), ("cpp", 40)]),
            &builtin_only(),
        )
        .expect("a LOW pure-C/C++ repo yields the no-path line");
        assert!(
            line.contains("no semantic-resolution path exists for C/C++ on this build"),
            "the no-path line must name the full C/C++ set (not the single dominant): {line}"
        );
        // Cross-check: the ceiling gate names the SAME set for the SAME counts — proving one source.
        assert_eq!(
            call_graph_ceiling_languages(&counts(&[("c", 60), ("cpp", 40)])),
            Some(vec!["C".to_string(), "C++".to_string()]),
        );
    }

    #[test]
    fn no_path_line_single_language_unchanged() {
        // A single no-resolver language still reads as just that language (no spurious separator).
        let line = relationship_next_action_line(
            &call_low(),
            &counts(&[("python", 500)]),
            &builtin_only(),
        )
        .expect("a LOW Python repo yields the no-path line");
        assert!(
            line.contains("no semantic-resolution path exists for Python on this build"),
            "single-language no-path line is unchanged: {line}"
        );
        assert!(
            !line.contains("Python/"),
            "no trailing separator for a single language: {line}"
        );
    }

    // ── ORIENT-SMALL-ENRICH-1 (§1a): the in-flight applicability gate across the capability cells ──

    #[test]
    fn in_flight_gate_across_capability_cells() {
        // Ceiling repo (C / C++ / Python — no resolver on ANY build): the pass CANNOT raise figures.
        assert_eq!(
            in_flight_pass_can_apply(Ok(counts(&[("c", 80), ("cpp", 60)])), &builtin_only()),
            Ok(false),
            "a permanent C/C++ ceiling must NOT admit the in-flight 'may rise' promise"
        );
        assert_eq!(
            in_flight_pass_can_apply(
                Ok(counts(&[("python", 2904), ("javascript", 111)])),
                &builtin_only()
            ),
            Ok(false),
            "a Python-dominant ceiling (incidental JS below the gate) must NOT admit it"
        );
        // ENRICHABLE-NOW repo (a materially-present CONFIGURED-resolver language): the pass CAN raise.
        assert_eq!(
            in_flight_pass_can_apply(Ok(counts(&[("typescript", 500)])), &builtin_only()),
            Ok(true),
            "a TS repo has a configured resolver → the in-flight posture applies"
        );
        assert_eq!(
            in_flight_pass_can_apply(
                Ok(counts(&[("javascript", 1658), ("typescript", 699), ("java", 273)])),
                &builtin_only()
            ),
            Ok(true),
            "a material TS/JS half (TS resolver configured) makes the repo enrichable → in-flight applies"
        );
        // CELL 4a — config-only / no materially-present code language (reviewer review-0): `NoCeiling`
        // by the CeilingFact source, but NOTHING is enrichable, so the pass raises nothing → NEVER admit.
        assert_eq!(
            in_flight_pass_can_apply(Ok(counts(&[("json", 9999)])), &builtin_only()),
            Ok(false),
            "a config-only repo (no material code language) must NOT admit the promise"
        );
        assert_eq!(
            in_flight_pass_can_apply(Ok(vec![]), &builtin_only()),
            Ok(false),
            "an empty count set must NOT admit the promise"
        );
        // CELL 4b — Java present but JDTLS NOT configured on this build: `NoCeiling` (Java HAS a resolver
        // on some build), but the running pass has no Java resolver wired, so it raises nothing → NEVER
        // admit. WITH JDTLS configured it becomes enrichable-now → admits. This is the configured-gating
        // that distinguishes pass-applicability from the build-independent `NoCeiling`.
        assert_eq!(
            in_flight_pass_can_apply(Ok(counts(&[("java", 500)])), &builtin_only()),
            Ok(false),
            "a Java repo without JDTLS configured must NOT admit the promise (pass has no Java resolver)"
        );
        assert_eq!(
            in_flight_pass_can_apply(Ok(counts(&[("java", 500)])), &with_jdtls()),
            Ok(true),
            "the SAME Java repo WITH JDTLS configured admits the promise (the pass can raise Java figures)"
        );
    }

    /// reviewer review-1 F1: a FAILED count read is PRESERVED — the gate hands the reason BACK (never
    /// collapses it to `Ok(false)`, which would classify "pass does not apply" from a read that never
    /// happened). The three surfaces then surface that reason honestly (orient/reliability error, check via
    /// its `CeilingFact::Unknown` from the same read).
    #[test]
    fn in_flight_gate_preserves_the_read_failure_reason() {
        assert_eq!(
            in_flight_pass_can_apply(Err("db locked".to_string()), &builtin_only()),
            Err("db locked".to_string()),
            "a failed count read must hand back its reason, not collapse to Ok(false)"
        );
    }

    // ── RESOURCE-HONESTY-1 (§2.1/§2.2): materially-present languages with no resource detector ──

    /// The real build's coverage: TS/JS/Python/Java/C/C++ covered, Rust (and everything else) not.
    /// Mirrors `repo_graph_repo_index::resource_detection_covers` without the crate dep in this pure
    /// test — the daemon injects the real predicate at the call site.
    fn covers(token: &str) -> bool {
        matches!(
            token,
            "typescript" | "tsx" | "javascript" | "jsx" | "python" | "java" | "c" | "cpp"
        )
    }

    #[test]
    fn uncovered_material_names_the_detectorless_material_languages() {
        // repo-graph's own shape: Rust-dominant with an incidental covered language. Rust is
        // material AND has no detector → named; the covered language is filtered out.
        assert_eq!(
            resource_uncovered_material_languages(
                &counts(&[("rust", 900), ("typescript", 100)]),
                covers,
            ),
            vec!["Rust".to_string()]
        );
        // Two uncovered material languages (both reader-nameable, neither covered) → both named,
        // sorted.
        assert_eq!(
            resource_uncovered_material_languages(&counts(&[("rust", 80), ("go", 60)]), covers),
            vec!["Go".to_string(), "Rust".to_string()]
        );
    }

    #[test]
    fn uncovered_material_empty_when_all_material_languages_covered() {
        // A pure-TS repo: TS is covered → nothing uncovered (the zero-state then reads as an honest
        // "genuinely none found", not a coverage gap).
        assert!(resource_uncovered_material_languages(&langs(&["typescript"]), covers).is_empty());
        // Incidental (<10%) uncovered language is BELOW the material gate → not named.
        assert!(resource_uncovered_material_languages(
            &counts(&[("python", 950), ("rust", 50)]),
            covers,
        )
        .is_empty());
        // No material code language at all → empty.
        assert!(resource_uncovered_material_languages(&[], covers).is_empty());
    }

    // ── CYCLE-HONESTY-1 (§2.4): the TS/JS caveat's materiality gate reuses the ≥10% code-file rule ──

    #[test]
    fn material_ts_js_requires_ten_percent_not_mere_presence() {
        // django-shape: 2904 Python / 111 JavaScript -> JS ~3.7%, incidental tooling, NOT material.
        let django = vec![
            ("python".to_string(), 2904u64),
            ("javascript".to_string(), 111u64),
        ];
        assert!(
            !repo_has_material_ts_js(&django),
            "incidental (<10%) JS must NOT trip the caveat"
        );
        // A TS-dominant repo (config-file tokens like json never dilute the CODE-file denominator).
        let ts = vec![
            ("typescript".to_string(), 900u64),
            ("json".to_string(), 300u64),
        ];
        assert!(repo_has_material_ts_js(&ts), "TS-dominant repo is material");
        // Exactly 10% of code files clears the gate (`count*10 >= total`).
        let border = vec![("python".to_string(), 90u64), ("tsx".to_string(), 10u64)];
        assert!(repo_has_material_ts_js(&border), "exactly 10% is material");
        // Just under 10% does not.
        let under = vec![("python".to_string(), 91u64), ("tsx".to_string(), 9u64)];
        assert!(!repo_has_material_ts_js(&under), "9% is not material");
        // No TS/JS token at all -> false regardless of share.
        let none = vec![("rust".to_string(), 500u64)];
        assert!(!repo_has_material_ts_js(&none));
        // The full TS/JS family vocabulary is recognized (jsx here dominates).
        let jsx = vec![("jsx".to_string(), 50u64), ("python".to_string(), 50u64)];
        assert!(repo_has_material_ts_js(&jsx));
    }

    // ── ZEROSTATE-SCOPE-1 (§2.1): the per-repo surfaces/boundaries gap clause ──

    /// The gap helper composed with the REAL build-static repo-index accessors — proving the
    /// per-repo sentence: leveldb (C/C++) names C/C++, django (Python) names Django URLconf, a
    /// covered-only repo names nothing, and no code language → nothing. No repo wears another's.
    #[test]
    fn surface_gap_is_per_repo_never_anothers_sentence() {
        let covers = repo_graph_repo_index::surface_coverage::http_surface_detection_covers;
        let gap_name = repo_graph_repo_index::surface_coverage::http_surface_named_gap_for;

        // leveldb: a pure C/C++ repo names ITS OWN languages, not django's URLconf.
        let leveldb = vec![("cpp".to_string(), 800u64), ("c".to_string(), 200u64)];
        assert_eq!(
            surface_uncovered_material_gaps(&leveldb, covers, gap_name),
            vec!["C".to_string(), "C++".to_string()]
        );

        // django: a materially-Python repo keeps the URLconf framework name. Its ~3.7% JS is
        // below the materiality gate and is covered anyway, so it never appears.
        let django = vec![
            ("python".to_string(), 2904u64),
            ("javascript".to_string(), 111u64),
        ];
        assert_eq!(
            surface_uncovered_material_gaps(&django, covers, gap_name),
            vec!["Django URLconf routes".to_string()]
        );

        // A covered-only repo (Java + TS/JS) → every material language covered → no clause.
        let covered = vec![
            ("java".to_string(), 500u64),
            ("typescript".to_string(), 500u64),
        ];
        assert!(surface_uncovered_material_gaps(&covered, covers, gap_name).is_empty());

        // No materially-present CODE language (config-only) → nothing to name.
        let config_only = vec![("json".to_string(), 100u64)];
        assert!(surface_uncovered_material_gaps(&config_only, covers, gap_name).is_empty());

        // A mixed repo names BOTH the covered-language's absence-of-clause AND the uncovered
        // languages: Rust (dominant, uncovered) + a material TS half (covered) → only Rust.
        let mixed = vec![
            ("rust".to_string(), 600u64),
            ("typescript".to_string(), 400u64),
        ];
        assert_eq!(
            surface_uncovered_material_gaps(&mixed, covers, gap_name),
            vec!["Rust".to_string()]
        );
    }
}
