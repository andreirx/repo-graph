//! Trust reporting — deterministic detection rules + reliability
//! formulas.
//!
//! Mirror of `src/core/trust/rules.ts`. Every function is PURE.
//! No storage access, no I/O. Inputs are plain data; outputs are
//! flags and reasons.
//!
//! Reliability level semantics:
//!   HIGH   — the command's claimed behavior is reliable on this repo
//!   MEDIUM — reliable enough for orientation but not decision-critical
//!   LOW    — do not act on this command's output without manual verification

use crate::types::{
    DowngradeTrigger, ExtractionDiagnostics, ReliabilityAxisScore, ReliabilityLevel,
};

// ── Downgrade detection ──────────────────────────────────────────

/// Next.js app-router detection (HONESTY-GATE-2 family 2).
///
/// A framework claim requires STRUCTURAL evidence, never a directory/file NAME
/// alone (STANDING HONESTY RULE 2). Next.js is claimed only when BOTH hold:
///   1. an app-router convention file exists (page/layout/route nested under an
///      `app/` directory) — the "which router" signal; AND
///   2. a `next.config.{js,ts,mjs,cjs}` file is present in the repo — the
///      structural signal that this IS a Next.js project (the "config file"
///      evidence named by the slice §2.2).
///
/// The `app/` directory name matched (1) on real non-Next.js repos (a React
/// Router 7 app under `src/app/` in hadoop), fabricating the claim.
///
/// Structural basis = app-router convention AND `next.config.*`. This is a
/// DELIBERATELY CONSERVATIVE basis, not a completeness claim: a `next.config.*`
/// file is OPTIONAL in Next.js (a project can run on framework defaults with no
/// config), so requiring it TRADES RECALL FOR HONESTY — a genuine Next.js app
/// that ships no config is not detected here (accepted recall loss), but the
/// claim we DO emit always carries structural evidence rather than a directory
/// name. Absence of config → no claim (honest abstention over fabrication).
/// Widening this basis to other Next.js signals (a `next` dependency in the
/// manifest, a framework import) is detector-recall widening — OUT OF SCOPE for
/// this slice (STANDING SCOPE), a named follow-up.
///
/// Returns `Some(basis_path)` (the config file proving the claim, rendered WITH
/// the claim) when detected, else `None`. Absent basis → no claim (and no
/// reliability downgrade derived from it).
///
/// Diverges from the TS mirror `detectNextjsConventions` (`rules.ts:36`), which
/// returned a bare bool from names alone — the exact fabrication this removes.
pub(crate) fn detect_nextjs_conventions(file_paths: &[String]) -> Option<String> {
    let has_convention = file_paths.iter().any(|path| {
        has_nextjs_file(path, "page", &["tsx", "jsx"])
            || has_nextjs_file(path, "layout", &["tsx", "jsx"])
            || has_nextjs_file(path, "route", &["ts", "js"])
    });
    if !has_convention {
        return None;
    }
    // Structural evidence gate: a Next.js config file must be present.
    file_paths.iter().find(|p| is_nextjs_config(p)).cloned()
}

/// Whether `path`'s basename is a Next.js config file (`next.config.{js,ts,mjs,cjs}`).
/// Basename match, any directory — a Next.js sub-app (e.g. `renderer/next.config.ts`)
/// is valid structural evidence for the whole indexed repo.
fn is_nextjs_config(path: &str) -> bool {
    let base = path.rsplit('/').next().unwrap_or(path);
    matches!(
        base,
        "next.config.js" | "next.config.ts" | "next.config.mjs" | "next.config.cjs"
    )
}

/// Check if a path matches the pattern `(^|/)app/.../<filename>.<ext>$`
/// where `...` contains at least one `/` (i.e., at least one level
/// of directory nesting under `app/`).
fn has_nextjs_file(path: &str, filename: &str, extensions: &[&str]) -> bool {
    let after_app = if let Some(suffix) = path.strip_prefix("app/") {
        Some(suffix)
    } else {
        path.find("/app/").map(|pos| &path[pos + 5..])
    };
    let after_app = match after_app {
        Some(s) => s,
        None => return false,
    };
    for ext in extensions {
        let suffix = format!("/{filename}.{ext}");
        if after_app.ends_with(&suffix) {
            return true;
        }
    }
    false
}

/// React-heavy UI surface detection. First-slice proxy:
/// (tsx + jsx) file ratio >= 0.20.
///
/// Mirror of `detectReactHeavy` from `rules.ts:62`.
pub(crate) fn detect_react_heavy(file_paths: &[String]) -> bool {
    if file_paths.is_empty() {
        return false;
    }
    let react_count = file_paths
        .iter()
        .filter(|p| p.ends_with(".tsx") || p.ends_with(".jsx"))
        .count();
    let ratio = react_count as f64 / file_paths.len() as f64;
    ratio >= 0.2
}

/// Framework-heavy suspicion: Next.js conventions OR React-heavy
/// ratio.
///
/// Mirror of `detectFrameworkHeavySuspicion` from `rules.ts:75`.
pub fn detect_framework_heavy_suspicion(file_paths: &[String]) -> DowngradeTrigger {
    let mut reasons = Vec::new();
    if let Some(config_basis) = detect_nextjs_conventions(file_paths) {
        // Render the basis WITH the claim (STANDING HONESTY RULE 2): the config
        // file that proves this is Next.js, not the directory name that merely
        // resembles it.
        reasons.push(format!(
            "nextjs_app_router_detected (basis: {} present)",
            config_basis
        ));
    }
    if detect_react_heavy(file_paths) {
        reasons.push("react_heavy_tsx_ratio".to_string());
    }
    DowngradeTrigger {
        triggered: !reasons.is_empty(),
        reasons,
    }
}

/// Registry-pattern suspicion. Triggers when any ancestor has >= 3
/// parent-child cycles, OR total cycles >= 5.
///
/// Mirror of `detectRegistryPatternSuspicion` from `rules.ts:96`.
///
/// Takes a slice of `(ancestor_key, count)` pairs and a total.
/// Both are produced by `group_path_prefix_cycles_by_ancestor`.
pub fn detect_registry_pattern_suspicion(
    cycles_by_ancestor: &[(String, usize)],
    total_cycles: usize,
) -> DowngradeTrigger {
    let mut reasons = Vec::new();
    let mut max_count = 0usize;
    let mut top_ancestor: Option<&str> = None;
    for (ancestor, count) in cycles_by_ancestor {
        if *count > max_count {
            max_count = *count;
            top_ancestor = Some(ancestor);
        }
    }
    if max_count >= 3 {
        if let Some(anc) = top_ancestor {
            reasons.push(format!(
                "ancestor_with_{}_parent_child_cycles:{}",
                max_count, anc
            ));
        }
    }
    if total_cycles >= 5 {
        reasons.push(format!("total_parent_child_cycles={}", total_cycles));
    }
    DowngradeTrigger {
        triggered: !reasons.is_empty(),
        reasons,
    }
}

/// Missing entrypoint declarations: active count === 0.
///
/// Mirror of `detectMissingEntrypointDeclarations` from
/// `rules.ts:125`.
pub fn detect_missing_entrypoint_declarations(active_entrypoint_count: usize) -> DowngradeTrigger {
    if active_entrypoint_count == 0 {
        DowngradeTrigger {
            triggered: true,
            reasons: vec!["active_entrypoint_count=0".to_string()],
        }
    } else {
        DowngradeTrigger {
            triggered: false,
            reasons: vec![],
        }
    }
}

/// Alias-resolution suspicion: suspicious module count >= 3.
///
/// Mirror of `detectAliasResolutionSuspicion` from `rules.ts:141`.
pub fn detect_alias_resolution_suspicion(suspicious_module_count: usize) -> DowngradeTrigger {
    if suspicious_module_count >= 3 {
        DowngradeTrigger {
            triggered: true,
            reasons: vec![format!(
                "suspicious_zero_connectivity_modules={}",
                suspicious_module_count
            )],
        }
    } else {
        DowngradeTrigger {
            triggered: false,
            reasons: vec![],
        }
    }
}

// ── Reliability formulas ─────────────────────────────────────────

/// Import graph reliability.
///
/// Mirror of `computeImportGraphReliability` from `rules.ts:161`.
pub fn compute_import_graph_reliability(
    alias_resolution_suspicion: bool,
    registry_pattern_suspicion: bool,
    unresolved_imports_count: u64,
) -> ReliabilityAxisScore {
    let mut reasons = Vec::new();
    if alias_resolution_suspicion {
        reasons.push("alias_resolution_suspicion".to_string());
    }
    if unresolved_imports_count > 0 {
        reasons.push(format!("unresolved_imports={}", unresolved_imports_count));
    }
    if !reasons.is_empty() {
        return ReliabilityAxisScore {
            level: ReliabilityLevel::LOW,
            reasons,
        };
    }
    if registry_pattern_suspicion {
        return ReliabilityAxisScore {
            level: ReliabilityLevel::MEDIUM,
            reasons: vec!["registry_pattern_suspicion".to_string()],
        };
    }
    ReliabilityAxisScore {
        level: ReliabilityLevel::HIGH,
        reasons: vec![],
    }
}

/// Call graph reliability (Variant A reweighting).
///
/// Mirror of `computeCallGraphReliability` from `rules.ts:200`.
pub fn compute_call_graph_reliability(
    resolved_calls: u64,
    unresolved_calls_internal_like: u64,
) -> ReliabilityAxisScore {
    let total = resolved_calls + unresolved_calls_internal_like;
    if total == 0 {
        return ReliabilityAxisScore {
            level: ReliabilityLevel::HIGH,
            reasons: vec![],
        };
    }
    let rate = resolved_calls as f64 / total as f64;
    let rate_pct = format!("{:.1}", rate * 100.0);
    if rate < 0.5 {
        ReliabilityAxisScore {
            level: ReliabilityLevel::LOW,
            reasons: vec![format!("call_resolution_rate={}%_below_50%", rate_pct)],
        }
    } else if rate < 0.85 {
        ReliabilityAxisScore {
            level: ReliabilityLevel::MEDIUM,
            reasons: vec![format!("call_resolution_rate={}%_below_85%", rate_pct)],
        }
    } else {
        ReliabilityAxisScore {
            level: ReliabilityLevel::HIGH,
            reasons: vec![],
        }
    }
}

/// Dead-code reliability.
///
/// Mirror of `computeDeadCodeReliability` from `rules.ts:231`.
pub fn compute_dead_code_reliability(
    missing_entrypoint_declarations: bool,
    registry_pattern_suspicion: bool,
    framework_heavy_suspicion: bool,
    call_graph_level: ReliabilityLevel,
) -> ReliabilityAxisScore {
    let mut reasons = Vec::new();
    if missing_entrypoint_declarations {
        reasons.push("missing_entrypoint_declarations".to_string());
    }
    if registry_pattern_suspicion {
        reasons.push("registry_pattern_suspicion".to_string());
    }
    if framework_heavy_suspicion {
        reasons.push("framework_heavy_suspicion".to_string());
    }
    if !reasons.is_empty() {
        return ReliabilityAxisScore {
            level: ReliabilityLevel::LOW,
            reasons,
        };
    }
    if call_graph_level == ReliabilityLevel::LOW {
        return ReliabilityAxisScore {
            level: ReliabilityLevel::MEDIUM,
            reasons: vec!["call_graph_reliability_low".to_string()],
        };
    }
    ReliabilityAxisScore {
        level: ReliabilityLevel::HIGH,
        reasons: vec![],
    }
}

/// Change-impact reliability.
///
/// Mirror of `computeChangeImpactReliability` from `rules.ts:282`.
pub fn compute_change_impact_reliability(
    alias_resolution_suspicion: bool,
    registry_pattern_suspicion: bool,
    import_graph_level: ReliabilityLevel,
) -> ReliabilityAxisScore {
    let mut reasons = Vec::new();
    if alias_resolution_suspicion {
        reasons.push("alias_resolution_suspicion".to_string());
    }
    if registry_pattern_suspicion {
        reasons.push("registry_pattern_suspicion".to_string());
    }
    if !reasons.is_empty() {
        return ReliabilityAxisScore {
            level: ReliabilityLevel::LOW,
            reasons,
        };
    }
    match import_graph_level {
        ReliabilityLevel::HIGH => ReliabilityAxisScore {
            level: ReliabilityLevel::HIGH,
            reasons: vec![],
        },
        ReliabilityLevel::MEDIUM => ReliabilityAxisScore {
            level: ReliabilityLevel::MEDIUM,
            reasons: vec!["import_graph_reliability_medium".to_string()],
        },
        ReliabilityLevel::LOW => ReliabilityAxisScore {
            level: ReliabilityLevel::LOW,
            reasons: vec!["import_graph_reliability_low".to_string()],
        },
    }
}

// ── Diagnostic aggregation helpers ───────────────────────────────

/// Sum unresolved counts for CALLS-family categories.
///
/// Mirror of `sumUnresolvedCalls` from `rules.ts:317`.
pub(crate) fn sum_unresolved_calls(diagnostics: &ExtractionDiagnostics) -> u64 {
    let calls_categories = [
        "calls_this_wildcard_method_needs_type_info",
        "calls_this_method_needs_class_context",
        "calls_obj_method_needs_type_info",
        "calls_function_ambiguous_or_missing",
    ];
    calls_categories
        .iter()
        .map(|cat| {
            diagnostics
                .unresolved_breakdown
                .get(*cat)
                .copied()
                .unwrap_or(0)
        })
        .sum()
}

/// Sum unresolved counts for IMPORTS-family categories.
///
/// Mirror of `sumUnresolvedImports` from `rules.ts:330`.
pub(crate) fn sum_unresolved_imports(diagnostics: &ExtractionDiagnostics) -> u64 {
    diagnostics
        .unresolved_breakdown
        .get("imports_file_not_found")
        .copied()
        .unwrap_or(0)
}

/// Count modules matching the suspicious-zero-connectivity pattern.
///
/// Mirror of `countSuspiciousZeroConnectivityModules` from
/// `rules.ts:346`.
pub(crate) fn count_suspicious_zero_connectivity_modules(
    modules: &[ModuleForSuspicionCheck],
) -> usize {
    modules
        .iter()
        .filter(|m| {
            m.fan_in == 0
                && m.fan_out == 0
                && m.file_count >= 2
                && m.qualified_name != "."
                && !m.qualified_name.is_empty()
        })
        .count()
}

/// Input shape for `count_suspicious_zero_connectivity_modules`.
pub(crate) struct ModuleForSuspicionCheck {
    pub qualified_name: String,
    pub fan_in: u64,
    pub fan_out: u64,
    pub file_count: u64,
}

/// Group path-prefix cycles by ancestor stable_key and return
/// counts as a sorted Vec of (ancestor_key, count) pairs.
///
/// Mirror of `groupPathPrefixCyclesByAncestor` from `rules.ts:372`.
///
/// Returns `Vec<(String, usize)>` instead of the TS
/// `Record<string, number>` — per the no-HashMap API rule.
/// The Vec is sorted by key for determinism.
pub(crate) fn group_path_prefix_cycles_by_ancestor(
    cycles: &[PathPrefixCycleInput],
) -> Vec<(String, usize)> {
    use std::collections::BTreeMap;
    let mut by_ancestor: BTreeMap<&str, usize> = BTreeMap::new();
    for c in cycles {
        *by_ancestor.entry(&c.ancestor_stable_key).or_insert(0) += 1;
    }
    by_ancestor
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
}

/// Input shape for `group_path_prefix_cycles_by_ancestor`.
pub(crate) struct PathPrefixCycleInput {
    pub ancestor_stable_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    // ── Framework detection ──────────────────────────────────

    #[test]
    fn nextjs_detects_app_router_page() {
        // Convention file + structural evidence (next.config) → detected, basis returned.
        assert_eq!(
            detect_nextjs_conventions(&[
                "src/app/home/page.tsx".into(),
                "next.config.js".into(),
                "lib/util.ts".into(),
            ]),
            Some("next.config.js".to_string())
        );
    }

    #[test]
    fn nextjs_detects_app_router_layout() {
        assert_eq!(
            detect_nextjs_conventions(&["app/[id]/layout.tsx".into(), "next.config.mjs".into()]),
            Some("next.config.mjs".to_string())
        );
    }

    #[test]
    fn nextjs_detects_app_router_route() {
        assert_eq!(
            detect_nextjs_conventions(&[
                "app/api/users/route.ts".into(),
                "renderer/next.config.ts".into()
            ]),
            Some("renderer/next.config.ts".to_string())
        );
    }

    /// HONESTY-GATE-2 family 2 (reproducing): app-router convention files but NO
    /// `next.config.*` — the real hadoop case (React Router 7 under `src/app/`).
    /// A directory NAME is not a framework: the claim must NOT fire.
    #[test]
    fn nextjs_not_detected_without_config_even_with_convention_files() {
        assert_eq!(
            detect_nextjs_conventions(&[
                "src/app/routes/layout.tsx".into(),
                "src/app/dashboard/page.tsx".into(),
                "vite.config.ts".into(),
                "package.json".into(),
            ]),
            None,
            "app/ convention files without next.config must not detect Next.js"
        );
    }

    #[test]
    fn nextjs_returns_none_with_no_matches() {
        assert_eq!(
            detect_nextjs_conventions(&[
                "src/index.ts".into(),
                "lib/util.ts".into(),
                "components/Button.tsx".into(),
            ]),
            None
        );
    }

    #[test]
    fn nextjs_returns_none_for_app_without_specific_files() {
        assert_eq!(detect_nextjs_conventions(&["app/README.md".into()]), None);
    }

    /// A `next.config.*` present but NO app-router convention file → the
    /// app-router claim still requires (2) the convention (pages-router or a
    /// non-app Next app is not an app-router detection).
    #[test]
    fn nextjs_not_detected_with_config_but_no_convention() {
        assert_eq!(
            detect_nextjs_conventions(&["next.config.js".into(), "src/index.ts".into()]),
            None
        );
    }

    #[test]
    fn react_heavy_true_at_20_percent() {
        let files: Vec<String> = vec![
            "a.tsx", "b.tsx", "c.ts", "d.ts", "e.ts", "f.ts", "g.ts", "h.ts", "i.ts", "j.ts",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        assert!(detect_react_heavy(&files)); // 2/10 = 20%
    }

    #[test]
    fn react_heavy_counts_jsx_too() {
        let files: Vec<String> = vec![
            "a.jsx", "b.jsx", "c.ts", "d.ts", "e.ts", "f.ts", "g.ts", "h.ts", "i.ts", "j.ts",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        assert!(detect_react_heavy(&files));
    }

    #[test]
    fn react_heavy_counts_tsx_and_jsx_together() {
        // 1 tsx + 1 jsx = 2 react out of 10 = 20%
        let files: Vec<String> = vec![
            "a.tsx", "b.jsx", "c.ts", "d.ts", "e.ts", "f.ts", "g.ts", "h.ts", "i.ts", "j.ts",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        assert!(detect_react_heavy(&files));
    }

    #[test]
    fn react_heavy_false_below_threshold() {
        let files: Vec<String> = vec![
            "a.tsx", "c.ts", "d.ts", "e.ts", "f.ts", "g.ts", "h.ts", "i.ts", "j.ts", "k.ts",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        assert!(!detect_react_heavy(&files)); // 1/10 = 10%
    }

    #[test]
    fn react_heavy_false_for_empty() {
        assert!(!detect_react_heavy(&[]));
    }

    #[test]
    fn framework_heavy_triggered_by_nextjs() {
        // Structural evidence present → detected, and the reason renders the basis.
        let result = detect_framework_heavy_suspicion(&[
            "app/home/page.tsx".into(),
            "next.config.js".into(),
        ]);
        assert!(result.triggered);
        assert!(
            result
                .reasons
                .iter()
                .any(|r| r.starts_with("nextjs_app_router_detected (basis: next.config.js")),
            "reason must render the config basis: {:?}",
            result.reasons
        );
    }

    /// HONESTY-GATE-2 family 2: the fabrication case — app-router convention
    /// files but no `next.config.*` → nextjs is NOT among the reasons.
    #[test]
    fn framework_heavy_not_triggered_by_nextjs_without_config() {
        let result = detect_framework_heavy_suspicion(&[
            "src/app/routes/layout.tsx".into(),
            "vite.config.ts".into(),
        ]);
        assert!(
            !result
                .reasons
                .iter()
                .any(|r| r.contains("nextjs_app_router_detected")),
            "must not claim Next.js from the app/ directory name: {:?}",
            result.reasons
        );
    }

    #[test]
    fn framework_heavy_triggered_by_react_heavy_alone() {
        // 4 tsx out of 10 = 40% → react-heavy but no nextjs
        let files: Vec<String> = vec![
            "a.tsx", "b.tsx", "c.tsx", "d.tsx", "e.ts", "f.ts", "g.ts", "h.ts", "i.ts", "j.ts",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        let result = detect_framework_heavy_suspicion(&files);
        assert!(result.triggered);
        assert!(result
            .reasons
            .contains(&"react_heavy_tsx_ratio".to_string()));
        assert!(!result
            .reasons
            .contains(&"nextjs_app_router_detected".to_string()));
    }

    #[test]
    fn framework_heavy_not_triggered_for_plain_backend() {
        let result =
            detect_framework_heavy_suspicion(&["src/index.ts".into(), "src/utils.ts".into()]);
        assert!(!result.triggered);
    }

    // ── Registry pattern ─────────────────────────────────────

    #[test]
    fn registry_triggered_by_ancestor_with_3_cycles() {
        let result = detect_registry_pattern_suspicion(&[("src:MODULE".into(), 3)], 3);
        assert!(result.triggered);
    }

    #[test]
    fn registry_triggered_by_total_5_cycles() {
        let result = detect_registry_pattern_suspicion(
            &[
                ("a:MODULE".into(), 2),
                ("b:MODULE".into(), 2),
                ("c:MODULE".into(), 1),
            ],
            5,
        );
        assert!(result.triggered);
    }

    #[test]
    fn registry_not_triggered_for_empty_input() {
        let result = detect_registry_pattern_suspicion(&[], 0);
        assert!(!result.triggered);
    }

    #[test]
    fn registry_not_triggered_for_small_counts() {
        let result = detect_registry_pattern_suspicion(&[("a:MODULE".into(), 1)], 1);
        assert!(!result.triggered);
    }

    // ── Missing entrypoints ──────────────────────────────────

    #[test]
    fn missing_entrypoints_triggered_at_zero() {
        assert!(detect_missing_entrypoint_declarations(0).triggered);
    }

    #[test]
    fn missing_entrypoints_not_triggered_above_zero() {
        assert!(!detect_missing_entrypoint_declarations(1).triggered);
    }

    // ── Alias resolution ─────────────────────────────────────

    #[test]
    fn alias_suspicion_triggered_at_3() {
        assert!(detect_alias_resolution_suspicion(3).triggered);
    }

    #[test]
    fn alias_suspicion_not_triggered_below_3() {
        assert!(!detect_alias_resolution_suspicion(2).triggered);
    }

    // ── Import graph reliability ──────────────────────────────

    #[test]
    fn import_low_with_alias_suspicion() {
        let r = compute_import_graph_reliability(true, false, 0);
        assert_eq!(r.level, ReliabilityLevel::LOW);
    }

    #[test]
    fn import_low_with_unresolved_imports() {
        let r = compute_import_graph_reliability(false, false, 5);
        assert_eq!(r.level, ReliabilityLevel::LOW);
    }

    #[test]
    fn import_medium_with_registry_pattern_only() {
        let r = compute_import_graph_reliability(false, true, 0);
        assert_eq!(r.level, ReliabilityLevel::MEDIUM);
    }

    #[test]
    fn import_high_when_clean() {
        let r = compute_import_graph_reliability(false, false, 0);
        assert_eq!(r.level, ReliabilityLevel::HIGH);
    }

    // ── Call graph reliability ────────────────────────────────

    #[test]
    fn call_low_below_50() {
        let r = compute_call_graph_reliability(40, 61); // 40/101 ≈ 39.6%
        assert_eq!(r.level, ReliabilityLevel::LOW);
    }

    #[test]
    fn call_medium_between_50_and_85() {
        let r = compute_call_graph_reliability(70, 30); // 70%
        assert_eq!(r.level, ReliabilityLevel::MEDIUM);
    }

    #[test]
    fn call_high_above_85() {
        let r = compute_call_graph_reliability(90, 10); // 90%
        assert_eq!(r.level, ReliabilityLevel::HIGH);
    }

    #[test]
    fn call_high_when_no_calls() {
        let r = compute_call_graph_reliability(0, 0);
        assert_eq!(r.level, ReliabilityLevel::HIGH);
    }

    // ── Dead code reliability ────────────────────────────────

    #[test]
    fn dead_low_if_missing_entrypoints() {
        let r = compute_dead_code_reliability(true, false, false, ReliabilityLevel::HIGH);
        assert_eq!(r.level, ReliabilityLevel::LOW);
    }

    #[test]
    fn dead_low_if_registry_pattern_suspicion() {
        let r = compute_dead_code_reliability(false, true, false, ReliabilityLevel::HIGH);
        assert_eq!(r.level, ReliabilityLevel::LOW);
        assert!(r
            .reasons
            .contains(&"registry_pattern_suspicion".to_string()));
    }

    #[test]
    fn dead_low_if_framework_heavy_suspicion() {
        let r = compute_dead_code_reliability(false, false, true, ReliabilityLevel::HIGH);
        assert_eq!(r.level, ReliabilityLevel::LOW);
        assert!(r.reasons.contains(&"framework_heavy_suspicion".to_string()));
    }

    #[test]
    fn dead_medium_if_call_graph_low() {
        let r = compute_dead_code_reliability(false, false, false, ReliabilityLevel::LOW);
        assert_eq!(r.level, ReliabilityLevel::MEDIUM);
    }

    #[test]
    fn dead_high_otherwise() {
        let r = compute_dead_code_reliability(false, false, false, ReliabilityLevel::HIGH);
        assert_eq!(r.level, ReliabilityLevel::HIGH);
    }

    // ── Change impact reliability ────────────────────────────

    #[test]
    fn change_low_if_alias_suspicion() {
        let r = compute_change_impact_reliability(true, false, ReliabilityLevel::HIGH);
        assert_eq!(r.level, ReliabilityLevel::LOW);
    }

    #[test]
    fn change_low_if_registry_pattern_suspicion() {
        let r = compute_change_impact_reliability(false, true, ReliabilityLevel::HIGH);
        assert_eq!(r.level, ReliabilityLevel::LOW);
        assert!(r
            .reasons
            .contains(&"registry_pattern_suspicion".to_string()));
    }

    #[test]
    fn change_inherits_high_from_import_graph() {
        let r = compute_change_impact_reliability(false, false, ReliabilityLevel::HIGH);
        assert_eq!(r.level, ReliabilityLevel::HIGH);
    }

    #[test]
    fn change_inherits_medium_from_import_graph() {
        let r = compute_change_impact_reliability(false, false, ReliabilityLevel::MEDIUM);
        assert_eq!(r.level, ReliabilityLevel::MEDIUM);
    }

    #[test]
    fn change_inherits_low_from_import_graph() {
        let r = compute_change_impact_reliability(false, false, ReliabilityLevel::LOW);
        assert_eq!(r.level, ReliabilityLevel::LOW);
    }

    // ── Diagnostic helpers ───────────────────────────────────

    #[test]
    fn sum_unresolved_calls_sums_calls_family() {
        let mut breakdown = BTreeMap::new();
        breakdown.insert("calls_obj_method_needs_type_info".into(), 10);
        breakdown.insert("calls_function_ambiguous_or_missing".into(), 5);
        breakdown.insert("imports_file_not_found".into(), 3);
        let diag = ExtractionDiagnostics {
            diagnostics_version: 1,
            edges_total: 100,
            unresolved_total: 18,
            unresolved_breakdown: breakdown,
        };
        assert_eq!(sum_unresolved_calls(&diag), 15);
    }

    #[test]
    fn sum_unresolved_calls_zero_for_missing_categories() {
        let diag = ExtractionDiagnostics {
            diagnostics_version: 1,
            edges_total: 0,
            unresolved_total: 0,
            unresolved_breakdown: BTreeMap::new(),
        };
        assert_eq!(sum_unresolved_calls(&diag), 0);
    }

    #[test]
    fn sum_unresolved_imports_picks_imports_family() {
        let mut breakdown = BTreeMap::new();
        breakdown.insert("imports_file_not_found".into(), 7);
        breakdown.insert("calls_obj_method_needs_type_info".into(), 10);
        let diag = ExtractionDiagnostics {
            diagnostics_version: 1,
            edges_total: 100,
            unresolved_total: 17,
            unresolved_breakdown: breakdown,
        };
        assert_eq!(sum_unresolved_imports(&diag), 7);
    }

    #[test]
    fn count_suspicious_modules_matches_all_criteria() {
        let modules = vec![
            ModuleForSuspicionCheck {
                qualified_name: "src/api".into(),
                fan_in: 0,
                fan_out: 0,
                file_count: 5,
            },
            ModuleForSuspicionCheck {
                qualified_name: "src/core".into(),
                fan_in: 3,
                fan_out: 2,
                file_count: 10,
            },
            ModuleForSuspicionCheck {
                qualified_name: ".".into(), // repo root — excluded
                fan_in: 0,
                fan_out: 0,
                file_count: 50,
            },
        ];
        assert_eq!(count_suspicious_zero_connectivity_modules(&modules), 1);
    }

    #[test]
    fn group_path_prefix_cycles_groups_by_ancestor() {
        let cycles = vec![
            PathPrefixCycleInput {
                ancestor_stable_key: "a:MODULE".into(),
            },
            PathPrefixCycleInput {
                ancestor_stable_key: "a:MODULE".into(),
            },
            PathPrefixCycleInput {
                ancestor_stable_key: "b:MODULE".into(),
            },
        ];
        let grouped = group_path_prefix_cycles_by_ancestor(&cycles);
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[0], ("a:MODULE".to_string(), 2));
        assert_eq!(grouped[1], ("b:MODULE".to_string(), 1));
    }

    #[test]
    fn group_path_prefix_cycles_empty_for_empty_input() {
        let grouped = group_path_prefix_cycles_by_ancestor(&[]);
        assert!(grouped.is_empty());
    }
}
