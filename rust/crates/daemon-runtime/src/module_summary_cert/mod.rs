//! EC-M2-LEAF-SERVE-1: the MODULE_SUMMARY structural-count NO-LOSS certificate — the DR-2/DR-E3
//! `module_stats`-pattern IDENTITY-RECONCILIATION cert — plus the serve helpers the
//! `OrientServeDecorator` uses to compute `compute_{repo,path,file}_summary` from the LiveGraph on
//! GREEN. A sibling cert module like `focus_resolution_cert` / `callgraph_cert` /
//! `orient_lg_decisions::complexity_cert`.
//!
//! ## What the cert proves (build-time, SQLite read ONCE per fingerprint — the drilldown invariant)
//!
//! GREEN iff the LiveGraph per-file structural inventory ([`LiveGraph::structural_file_inventory`])
//! reconciles with the SQLite one at THREE granularities:
//!
//! 1. **Per-file** — path presence (a tracked SQLite file must have a resident FILE node), the
//!    ALL-visibility AST symbol count, and the language (the LiveGraph derives it from the path via
//!    the SAME `repo_graph_indexer::routing::detect_language` the scanner stored — one mapping, two
//!    engines, drift turns the cert RED instead of silently diverging).
//! 2. **Per-module** — the dirname rollup of (file_count, symbol_count) on BOTH sides — the ratified
//!    "LG counts == SQLite counts per module" identity reconciliation (EC-1 §5.2 M-2; the RISK-E
//!    module-identity divergence is answered here, not assumed away).
//! 3. **Repo totals** — the EXACT `compute_repo_summary` output (file/symbol/languages) equals the
//!    LiveGraph-derived totals. This is the serve-equivalence proof for the repo-focus value: it
//!    also covers the SQLite asymmetry where `compute_repo_summary` counts SYMBOL rows with no file
//!    join (the per-file rollup cannot see those; the totals compare can).
//!
//! ANY divergence ⇒ RED ⇒ the decorator keeps serving from SQLite (byte-identical, labelled — no
//! silent drift). KNOWN structural bound (surfaced, not papered over): the scanner tracks
//! config/contract files (`package.json`, `tsconfig.json`, `Cargo.toml`, `.proto` — routing.rs
//! `is_config_file`/`is_contract_extension`) into `files`/`file_versions` with NULL language; the
//! LiveGraph substrate carries only extracted TS source, so repos containing tracked non-source
//! files reconcile RED by construction. The cert makes that bound HONEST (SQLite serve, provenance
//! `{sqlite}`) rather than invisible.
//!
//! ## Serve helpers
//!
//! [`repo_summary_from_inventory`] / [`path_summary_from_inventory`] / [`file_summary_from_inventory`]
//! compute the exact `AgentRepoSummary` shapes from the inventory. The path scope replicates the
//! SQLite filter `path LIKE '{prefix}/%' OR path = {prefix}` FAITHFULLY via
//! [`sql_like_prefix_match`] — including SQLite `LIKE`'s ASCII case-insensitivity and `%`/`_`
//! wildcards (a `_` in a directory name is a wildcard in the shipped SQLite serve; byte-identity
//! requires replicating that, defect and all — see the fn doc).

use repo_graph_agent::AgentRepoSummary;
use repo_graph_livegraph::{StructuralFileRow, StructuralInventoryAnswer};
use repo_graph_trust_model::AnswerClass;

use crate::state::RepoState;

/// The SCAN-time language mapping, replicated as a pure function for the LiveGraph-side derivation
/// (`files.language` was written by `repo_graph_indexer::routing::detect_language` at index time).
///
/// WHY replicated, not imported (decide-and-record): daemon-runtime deliberately carries NO
/// production dependency on the indexer policy crate (`repo-graph-indexer` is dev-only here —
/// Cargo.toml notes the boundary); importing one pure fn would add a new production dependency
/// edge for a 15-line extension map. TWO mechanical guards make silent drift impossible:
/// (1) this cert's per-file language compare (derived-vs-stored) turns RED on ANY disagreement —
/// the serve falls back to SQLite, never a wrongly-labelled language; (2) the unit test
/// `scan_language_matches_indexer_detect_language` pins this fn to the indexer's mapping through
/// the EXISTING dev-dependency, so a mapping change fails the workspace gate.
fn scan_language(path: &str) -> Option<&'static str> {
    // `get_extension` mirror: everything from the LAST '.' (inclusive); "" when no dot.
    let ext = match path.rfind('.') {
        Some(pos) => &path[pos..],
        None => "",
    };
    match ext {
        ".ts" | ".mts" | ".cts" => Some("typescript"),
        ".tsx" => Some("tsx"),
        ".js" | ".mjs" | ".cjs" => Some("javascript"),
        ".jsx" => Some("jsx"),
        ".java" => Some("java"),
        ".py" => Some("python"),
        ".rs" => Some("rust"),
        ".c" | ".h" => Some("c"),
        ".cpp" | ".cc" | ".cxx" | ".hpp" | ".hxx" => Some("cpp"),
        _ => None,
    }
}

/// The in-memory MODULE_SUMMARY no-loss certificate (mirrors its sibling certs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleSummaryNoLossCert {
    /// `GREEN` = the three-granularity reconciliation is exact / `RED` = any divergence.
    pub verdict: String,
    /// The SQLite-free fingerprint this verdict was computed at (the invalidation key).
    pub fingerprint: String,
}

/// The shared compare data — the single source of the verdict, with named divergences so a RED is
/// diagnosable evidence (the slice stop-condition FINDING protocol), not a bare boolean.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ModuleSummaryCompareData {
    /// True iff every granularity reconciles (the GREEN condition).
    pub is_exact: bool,
    /// SQLite tracked-file paths with no resident FILE node (config/contract files, non-TS source,
    /// or a genuinely missing extraction).
    pub missing_in_livegraph: Vec<String>,
    /// LiveGraph inventory paths SQLite does not track (incl. symbol-attribution anomalies).
    pub extra_in_livegraph: Vec<String>,
    /// Paths present on both sides whose symbol count or language diverge.
    pub file_mismatches: Vec<String>,
    /// Dirname modules whose (file_count, symbol_count) rollup diverges — the per-module
    /// identity-reconciliation half.
    pub module_mismatches: Vec<String>,
    /// A repo-totals divergence vs the exact `compute_repo_summary` output, human-readable.
    pub totals_mismatch: Option<String>,
    /// The LiveGraph inventory answer class (`Exact` required for GREEN).
    pub livegraph_class: String,
}

/// The dirname module of a repo-relative path; `""` = the repo root bucket. The SAME rule on both
/// sides of the compare — module identity is reconciled by construction of the shared fold.
fn dirname_module(path: &str) -> &str {
    path.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

/// Compute the compare data: SQLite per-file rows + the exact `compute_repo_summary` totals vs the
/// LiveGraph structural inventory. Reads SQLite once + the LiveGraph once (one read guard).
pub(crate) fn module_summary_compare_data(
    repo_state: &RepoState,
    snapshot_uid: &str,
) -> Result<ModuleSummaryCompareData, String> {
    use std::collections::BTreeMap;

    // D-S = S-A: one fresh per-operation connection for the cert-build reads.
    let conn = repo_state
        .storage()
        .map_err(|e| format!("failed to open storage connection: {e}"))?;
    let sqlite_rows = conn
        .file_structural_rows(snapshot_uid)
        .map_err(|e| format!("file_structural_rows: {e}"))?;
    let summary = repo_graph_agent::AgentStorageRead::compute_repo_summary(&conn, snapshot_uid)
        .map_err(|e| format!("compute_repo_summary: {e}"))?;

    let (inventory, livegraph_class) = {
        let guard = repo_state.livegraph.read();
        match guard.as_ref() {
            Some(lg) => {
                let env = lg.structural_file_inventory();
                let class = format!("{:?}", env.class());
                match (env.class(), env.data()) {
                    (AnswerClass::Exact, Some(d)) => (Some(d.clone()), class),
                    _ => (None, class),
                }
            }
            None => (None, "Unavailable".to_string()),
        }
    };
    let Some(inventory) = inventory else {
        // Non-Exact / no LiveGraph: not a divergence FINDING — a residency/coverage precondition
        // miss. RED with the class recorded; no per-file diagnostics to name.
        return Ok(ModuleSummaryCompareData {
            is_exact: false,
            livegraph_class,
            ..Default::default()
        });
    };

    let mut data = ModuleSummaryCompareData {
        livegraph_class,
        ..Default::default()
    };

    // ── 1. Per-file reconciliation ────────────────────────────────────────────────────────────
    let lg_map: BTreeMap<&str, &StructuralFileRow> = inventory
        .files
        .iter()
        .map(|r| (r.path.as_str(), r))
        .collect();
    for sq in &sqlite_rows {
        match lg_map.get(sq.path.as_str()) {
            None => data.missing_in_livegraph.push(sq.path.clone()),
            Some(lg) if !lg.has_file_node => data.missing_in_livegraph.push(sq.path.clone()),
            Some(lg) => {
                let derived_language = scan_language(&sq.path).map(str::to_string);
                if lg.ast_symbol_count != sq.symbol_count || derived_language != sq.language {
                    data.file_mismatches.push(sq.path.clone());
                }
            }
        }
    }
    {
        use std::collections::BTreeSet;
        let sq_paths: BTreeSet<&str> = sqlite_rows.iter().map(|r| r.path.as_str()).collect();
        for row in &inventory.files {
            if !sq_paths.contains(row.path.as_str()) {
                data.extra_in_livegraph.push(row.path.clone());
            }
        }
    }

    // ── 2. Per-module (dirname) rollup — the ratified identity reconciliation ────────────────
    let mut sq_modules: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
    for sq in &sqlite_rows {
        let e = sq_modules.entry(dirname_module(&sq.path)).or_default();
        e.0 += 1;
        e.1 += sq.symbol_count;
    }
    let mut lg_modules: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
    for row in &inventory.files {
        let e = lg_modules.entry(dirname_module(&row.path)).or_default();
        if row.has_file_node {
            e.0 += 1;
        }
        e.1 += row.ast_symbol_count;
    }
    for (module, sq) in &sq_modules {
        match lg_modules.get(module) {
            Some(lg) if lg == sq => {}
            _ => data.module_mismatches.push(module.to_string()),
        }
    }
    for module in lg_modules.keys() {
        if !sq_modules.contains_key(module) {
            data.module_mismatches.push(module.to_string());
        }
    }

    // ── 3. Repo totals vs the EXACT compute_repo_summary output (the serve-equivalence proof) ─
    let (lg_file_count, lg_symbol_count, lg_languages) =
        summary_parts(inventory.files.iter(), inventory.unattributed_symbols);
    if lg_file_count != summary.file_count
        || lg_symbol_count != summary.symbol_count
        || lg_languages != summary.languages
    {
        data.totals_mismatch = Some(format!(
            "livegraph files={} symbols={} languages={:?} vs sqlite files={} symbols={} languages={:?}",
            lg_file_count,
            lg_symbol_count,
            lg_languages,
            summary.file_count,
            summary.symbol_count,
            summary.languages
        ));
    }

    data.is_exact = data.missing_in_livegraph.is_empty()
        && data.extra_in_livegraph.is_empty()
        && data.file_mismatches.is_empty()
        && data.module_mismatches.is_empty()
        && data.totals_mismatch.is_none();
    Ok(data)
}

/// The MODULE_SUMMARY leaf label when the decorator ACTUALLY served the counts from the LiveGraph
/// this request (module-summary cert GREEN at the captured witness fingerprint — review-0 #1:
/// independent of the bounded fold — ∧ epoch still resident post-use-case). The axes mirror the
/// sibling cert-served leaves: the cert requires the inventory answer `Exact` over an
/// all-resident + Fresh + TS-primary graph, so the projected posture is exactly that.
pub(crate) fn module_summary_served_outcome() -> crate::orient_lg_decisions::OrientLgOutcome {
    use repo_graph_trust_model::{FreshnessState, LanguageSupport, QueryCompleteness};
    crate::orient_lg_decisions::OrientLgOutcome::Livegraph {
        class: AnswerClass::Exact,
        completeness: QueryCompleteness::Complete,
        freshness: FreshnessState::Fresh,
        degradation_reasons: Vec::new(),
        contributing_languages: std::collections::BTreeSet::from([
            LanguageSupport::TypeScriptPrimary,
        ]),
    }
}

/// Build + store the cert keyed by `fingerprint`; `Some(is_green)`, or `None` on no fingerprint /
/// a storage error (the caller then serves SQLite — mirrors `build_and_store_cycles_cert`).
pub(crate) fn build_and_store_module_summary_cert(
    repo_state: &RepoState,
    snapshot_uid: &str,
    fingerprint: Option<String>,
) -> Option<bool> {
    let fingerprint = fingerprint?;
    let data = module_summary_compare_data(repo_state, snapshot_uid).ok()?;
    let is_green = data.is_exact;
    let verdict = if is_green { "GREEN" } else { "RED" }.to_string();
    *repo_state.module_summary_cert.write() = Some(ModuleSummaryNoLossCert {
        verdict,
        fingerprint,
    });
    Some(is_green)
}

// ── Serve helpers (the decorator's GREEN value computation, beside the cert that proves them) ────

/// Fold `(file_count, symbol_count, languages)` from inventory rows: files = rows WITH a FILE node;
/// symbols = ALL attributed counts (+ the unattributed remainder — repo scope only, mirroring the
/// SQLite `compute_repo_summary` no-file-join symbol count); languages = DISTINCT derived language
/// of FILE-node rows, ascending (the SQLite `DISTINCT f.language … ORDER BY ASC` mirror).
fn summary_parts<'a>(
    rows: impl Iterator<Item = &'a StructuralFileRow>,
    unattributed_symbols: u64,
) -> (u64, u64, Vec<String>) {
    use std::collections::BTreeSet;
    let mut file_count = 0u64;
    let mut symbol_count = unattributed_symbols;
    let mut languages: BTreeSet<String> = BTreeSet::new();
    for row in rows {
        symbol_count += row.ast_symbol_count;
        if row.has_file_node {
            file_count += 1;
            if let Some(lang) = scan_language(&row.path) {
                languages.insert(lang.to_string());
            }
        }
    }
    (file_count, symbol_count, languages.into_iter().collect())
}

/// The repo-focus `compute_repo_summary` value from the inventory (totals — cert granularity 3).
pub(crate) fn repo_summary_from_inventory(inv: &StructuralInventoryAnswer) -> AgentRepoSummary {
    let (file_count, symbol_count, languages) =
        summary_parts(inv.files.iter(), inv.unattributed_symbols);
    AgentRepoSummary {
        file_count,
        symbol_count,
        languages,
    }
}

/// The path-focus `compute_path_summary` value: rows matched by the FAITHFUL SQL-LIKE prefix filter.
/// No unattributed remainder (the SQLite path summary joins symbols through files — a symbol with no
/// file join is invisible to it, and to this).
pub(crate) fn path_summary_from_inventory(
    inv: &StructuralInventoryAnswer,
    path_prefix: &str,
) -> AgentRepoSummary {
    let (file_count, symbol_count, languages) = summary_parts(
        inv.files
            .iter()
            .filter(|r| sql_like_prefix_match(path_prefix, &r.path)),
        0,
    );
    AgentRepoSummary {
        file_count,
        symbol_count,
        languages,
    }
}

/// The file-focus `compute_file_summary` value: the exact-path row (SQL `=` is BINARY — exact,
/// case-sensitive string equality, unlike the LIKE arm).
pub(crate) fn file_summary_from_inventory(
    inv: &StructuralInventoryAnswer,
    file_path: &str,
) -> AgentRepoSummary {
    let (file_count, symbol_count, languages) =
        summary_parts(inv.files.iter().filter(|r| r.path == file_path), 0);
    AgentRepoSummary {
        file_count,
        symbol_count,
        languages,
    }
}

/// FAITHFUL replication of the SQLite path-scope predicate
/// `f.path LIKE '{prefix}/%' OR f.path = {prefix}` (no ESCAPE clause):
///
/// - the `=` arm is BINARY (exact bytes);
/// - the `LIKE` arm is SQLite-default ASCII-case-INSENSITIVE, with `%` = any sequence and `_` = any
///   single CHARACTER — INCLUDING metacharacters inside `prefix` itself, exactly as the shipped SQL
///   behaves today.
///
/// DELIBERATE-DEFECT NOTE (surfaced, not fixed here — name-vs-semantics): a `_` in a real directory
/// name (`my_module`) is a WILDCARD in the shipped SQLite serve (it also matches `my-module/…`), and
/// `Src/…` matches prefix `src`. That is arguably a latent contract defect in `compute_path_summary`,
/// but M-2's binding requirement is BYTE-IDENTITY between the GREEN serve and the SQLite serve, so
/// the LiveGraph serve replicates the behavior bit-for-bit; fixing the predicate is a separate,
/// explicitly-surfaced decision (it would change shipped output on both engines).
pub(crate) fn sql_like_prefix_match(prefix: &str, path: &str) -> bool {
    if path == prefix {
        return true;
    }
    let pattern: Vec<char> = prefix.chars().chain(['/', '%']).collect();
    let text: Vec<char> = path.chars().collect();
    sqlite_like_match(&pattern, &text)
}

/// SQLite-default `LIKE` over UTF-8 CHARACTERS (code points), the unit SQLite's `patternCompare`
/// reads via `Utf8Read`: `%` = any character sequence, `_` = exactly ONE character (review-0 #2:
/// a byte-wise `_` diverged — SQLite proves `'aéb/x.ts' LIKE 'a_b/%'` = 1, one code point 'é');
/// otherwise characters compare ASCII-case-insensitively — SQLite folds ONLY when BOTH code points
/// are < 0x80, which is precisely `char::eq_ignore_ascii_case` (non-ASCII folds to itself ⇒ exact
/// compare). Both inputs are Rust `str` (valid UTF-8), so `chars()` equals SQLite's `Utf8Read`
/// decoding. Iterative two-pointer backtracking (only `%` backtracks). Ground-truth-pinned against
/// the real engine by `tests::like_prefix_matches_real_sqlite_like` through the EXACT shipped
/// `compute_path_summary` query.
fn sqlite_like_match(pattern: &[char], s: &[char]) -> bool {
    fn eq_ci(a: char, b: char) -> bool {
        a.eq_ignore_ascii_case(&b)
    }
    let (mut p, mut i) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None; // (pattern idx of '%', s idx it matched to)
    while i < s.len() {
        if p < pattern.len() && pattern[p] == '%' {
            star = Some((p, i));
            p += 1;
        } else if p < pattern.len() && (pattern[p] == '_' || eq_ci(pattern[p], s[i])) {
            p += 1;
            i += 1;
        } else if let Some((sp, si)) = star {
            p = sp + 1;
            i = si + 1;
            star = Some((sp, si + 1));
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '%' {
        p += 1;
    }
    p == pattern.len()
}

#[cfg(test)]
mod tests;
