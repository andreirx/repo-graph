//! Deterministic MAP.md renderer (MAP-FROM-INDEX-1).
//!
//! Turns the flat extracted facts from the daemon `map` handler into MAP.md
//! files — one per directory (including ancestor/rollup directories) + one per
//! mapped file — with NO model call anywhere in the path (VISION commitment #1).
//! This is a PURE function (`render_maps: &MapFacts -> Vec<RenderedMapFile>`):
//! the command layer writes the results to disk under the resolved repo root.
//! Purity is deliberate — every named test in the slice (byte-determinism,
//! marker + provenance, unmapped honesty, stable ordering under permuted input,
//! golden fixture) exercises this function directly, without a daemon or DB.
//!
//! ## What it emits (extracted facts only — no inferred prose)
//!
//! Unlike rgistr's LLM output (`# Purpose`, `# Likely Change Reasons`,
//! `# Legacy Analysis` — model synthesis), this renders ONLY Layer-0/1 facts:
//! per file the symbols with signatures, its imports (resolved intra-repo target
//! where known, external/unresolved specifier otherwise), and coverage-honest
//! complexity; per directory the file inventory, package identity, an index of
//! child directories, an outbound dependency sketch (resolved import + call
//! edges), and — never silently omitted — the files the index could not map,
//! each with the reason. "What the index does not know, the map does not say."
//!
//! ## Determinism
//!
//! The renderer re-imposes a TOTAL order on everything it touches (files by path,
//! symbols by line then name then kind then signature — every rendered field,
//! imports/deps by target, directories by path, the emitted file set by output
//! path). So identical facts always produce byte-identical output, regardless of
//! the order the daemon happened to return rows in — the stable-ordering-under-
//! permuted-input guarantee.
//!
//! ## Placement + marker (mirrors rgistr's shipped conventions)
//!
//! Per-directory map: `<dir>/MAP.md` — emitted for every directory that
//! transitively contains a file within the requested scope, INCLUDING ancestor
//! directories whose files live only in descendants (rgistr's
//! `getFoldersForGeneration` rolls a folder up when a descendant has code). Those
//! parent-only maps carry a child-directory index so an agent walking the tree is
//! never handed a dead end. Per-file map: `<dir>/<base>_<ext>_MAP.md` (only the
//! FINAL extension dot becomes `_`; internal dots preserved — Node
//! `path.extname`/`basename` semantics, matching rgistr's `fileMapFilename`).
//! Every file opens with the ratified marker comment (no YAML frontmatter —
//! rgistr stripped that 2026-07-10), carrying the snapshot provenance the old
//! header lacked.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

/// One rendered map file: where it goes (repo-root-relative) + its full byte
/// contents (already newline-terminated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedMapFile {
    pub rel_path: String,
    pub contents: String,
}

// ── Deserialized daemon facts (the `map` method response) ────────────────────

/// The flat fact payload returned by the daemon `map` handler.
///
/// **Fail-closed contract (Architecture Rule 6: `null`=unknown, empty=known-zero
/// — never conflate).** Every top-level field is REQUIRED: it mirrors the daemon
/// `map` response, which emits every key unconditionally. There is deliberately
/// NO `#[serde(default)]` on these fields, so a partial or incompatible daemon
/// response (a key genuinely absent) FAILS to deserialize rather than silently
/// manufacturing an empty collection that would render as a confident zero
/// ("0 files", "0 symbols") — a false Layer-0 claim. Absent ≠ known-empty: the
/// daemon states empty by sending an empty array; the CLI never invents it. Only
/// a present-but-empty collection is a legitimate known-zero. (`#[derive(Default)]`
/// is retained purely for test ergonomics — `MapFacts { ..Default::default() }`
/// — and is independent of the serde contract.)
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MapFacts {
    pub repo: String,
    pub repo_name: String,
    /// Absolute repo root (from the daemon), where the CLI writes the files.
    /// Not rendered into map content — only used by the command write path.
    pub repo_root: String,
    pub snapshot: String,
    pub path: String,
    pub files: Vec<FileFact>,
    pub symbols: Vec<SymbolFact>,
    /// Resolved intra-repo dependency edges (IMPORTS + CALLS), each tagged with
    /// its edge type — hence `dependency_edges`, not `imports`: the collection is
    /// NOT imports-only. The renderer takes the IMPORTS subset for a file's import
    /// list and the full import+call set for the directory dependency sketch.
    pub dependency_edges: Vec<DepEdgeFact>,
    /// Unresolved IMPORTS (external packages / missing paths): source file → the
    /// specifier the source wrote. Feeds a file's "external / unresolved" imports.
    pub unresolved_imports: Vec<UnresolvedImportFact>,
    pub complexity: Vec<ComplexityFact>,
    pub manifest_roots: Vec<ManifestRootFact>,
    /// The always-present per-language measurement-coverage block; its caveat
    /// (partial coverage) or reason (coverage unavailable) is surfaced so
    /// complexity numbers — and their absence — are never read as complete.
    pub measurement_coverage: serde_json::Value,
    /// RECON-M-R3a (g3u): the daemon's union-sketch block — compiler-witnessed CALL file
    /// pairs the syntax sketch lacks (`dependency_call_pairs_added`) + the recorded
    /// `pair_delta`, coverage-labeled. Absent outside W-BOTH with a current measured
    /// ledger (R-0: the sketch is then exactly today's).
    #[serde(default)]
    pub witnesses: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileFact {
    pub path: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub parse_status: String,
    /// Raw `file_versions.extractor`: the skip-cause discriminator — `Some(
    /// "skipped:oversized")` = size cap, `None` on a skipped file = no extractor.
    #[serde(default)]
    pub extractor: Option<String>,
    #[serde(default)]
    pub is_test: bool,
    #[serde(default)]
    pub is_generated: bool,
    #[serde(default)]
    pub symbol_count: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SymbolFact {
    pub file: String,
    pub name: String,
    #[serde(default)]
    pub qualified_name: Option<String>,
    #[serde(default)]
    pub subtype: Option<String>,
    #[serde(default)]
    pub line_start: Option<u64>,
    #[serde(default)]
    pub signature: Option<String>,
}

/// A resolved intra-repo dependency edge: source file → target file, tagged with
/// its edge type (`IMPORTS` or `CALLS`).
#[derive(Debug, Clone, Deserialize)]
pub struct DepEdgeFact {
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub edge_type: String,
}

/// An unresolved IMPORTS edge: source file → the import specifier the source
/// wrote (external package or missing path — no resolved file target).
#[derive(Debug, Clone, Deserialize)]
pub struct UnresolvedImportFact {
    pub source: String,
    pub specifier: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ComplexityFact {
    pub file: String,
    pub sum_complexity: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestRootFact {
    pub path: String,
    pub kind: String,
}

// ── Marker + provenance ──────────────────────────────────────────────────────

/// The ratified generated-file marker, carrying snapshot provenance. Mirrors
/// rgistr's HTML-comment style (no frontmatter) and adds the snapshot short-uid.
pub fn marker(snapshot: &str) -> String {
    format!(
        "<!-- generated by rmap map from snapshot {}; do not hand-edit -->",
        short_uid(snapshot)
    )
}

/// A stable, deterministic provenance token identifying the snapshot the map
/// was rendered from. Snapshot uids are composite —
/// `<repo_uid>/<timestamp>/<content-hash>` — so a leading prefix is the *repo*
/// uid, shared by every snapshot of the repo and useless as snapshot
/// provenance. We take the trailing `/`-segment (the content hash, which ties
/// the map to the exact indexed content: identical content re-indexed → same
/// token → same maps) capped at 12 chars; a uid with no `/` falls back to its
/// 12-char prefix. Empty snapshot → `unknown` (never a fabricated id).
fn short_uid(snapshot: &str) -> String {
    let tail = snapshot.rsplit('/').next().unwrap_or(snapshot);
    let s: String = tail.chars().take(12).collect();
    if s.is_empty() {
        "unknown".to_string()
    } else {
        s
    }
}

// ── Path helpers (mirror rgistr's Node path semantics) ───────────────────────

fn dir_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..i],
        None => "",
    }
}

fn base_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[i + 1..],
        None => path,
    }
}

/// Per-file map filename from a basename, byte-identical to rgistr's
/// `fileMapFilename` (Node `path.extname`/`path.basename`): the final extension
/// dot becomes `_`; a name with no extension — or a dotfile whose only dot is
/// leading — yields the `<base>__MAP.md` double underscore. Internal dots are
/// preserved (`vitest.config.ts` -> `vitest.config_ts_MAP.md`).
fn file_map_filename(base: &str) -> String {
    match base.rfind('.').filter(|&i| i > 0) {
        Some(i) => format!("{}_{}_MAP.md", &base[..i], &base[i + 1..]),
        None => format!("{}__MAP.md", base),
    }
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", dir, name)
    }
}

// ── Fact classification (reader-frame, honest) ───────────────────────────────

/// A file is MAPPED (gets a per-file map) only when the index successfully
/// parsed it. `parse_status` is the raw `file_versions` value; `parsed` is the
/// success state (`ParseStatus::Parsed`, serialized lowercase). Everything else
/// is unmapped and listed with a reason — never silently omitted.
fn is_mapped(parse_status: &str) -> bool {
    parse_status == "parsed"
}

/// Reader-frame reason for an unmapped file, derived from the raw parse status
/// AND the raw `extractor` skip-cause (so a size-capped file, a no-extractor
/// file, and a parse failure read distinctly). An unrecognized status/cause is
/// surfaced verbatim rather than hidden.
fn unmapped_reason(parse_status: &str, extractor: Option<&str>) -> String {
    match parse_status {
        "skipped" => match extractor {
            Some("skipped:oversized") => "skipped — exceeds the size cap".to_string(),
            None => "skipped — no extractor for this language".to_string(),
            // Any other extractor tag on a skipped file: surface it verbatim.
            Some(other) => format!("skipped — {}", other),
        },
        "failed" => "parse failed".to_string(),
        "stale" => "stale — changed since last successful parse".to_string(),
        "" => "not parsed (status unknown)".to_string(),
        other => format!("not parsed (status: {})", other),
    }
}

/// Reader-frame kind for a symbol subtype (e.g. `FUNCTION` -> `function`).
fn kind_label(subtype: &Option<String>) -> String {
    match subtype {
        Some(s) if !s.is_empty() => s.to_lowercase(),
        _ => "symbol".to_string(),
    }
}

/// The single reader-frame coverage line from the always-present daemon
/// `measurement_coverage` block: the caveat when a significant language is
/// unmeasured, the reason when coverage could not be read, `None` only when
/// coverage is complete (nothing to say). Mirrors the shipped
/// `MeasurementCoverageBlock::caveat_line` semantics — so complexity numbers,
/// and their conspicuous absence, are never read as complete/zero.
fn coverage_note(mc: &serde_json::Value) -> Option<String> {
    let field = match mc.get("status").and_then(|s| s.as_str()) {
        Some("unavailable") => "reason",
        // "available" (or an older/absent status): the partial-coverage caveat.
        _ => "caveat",
    };
    mc.get(field)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

// ── Directory-set derivation (mirror rgistr's folder rollup) ─────────────────

/// Whether `dir` lies within the requested scope (`scope == ""` is whole-repo).
fn within_scope(dir: &str, scope: &str) -> bool {
    scope.is_empty() || dir == scope || dir.starts_with(&format!("{}/", scope))
}

/// The set of directories to emit a `MAP.md` for: every directory that
/// (transitively) contains a file within `scope`, INCLUDING ancestor directories
/// whose files live only in descendants (rgistr's `getFoldersForGeneration`).
/// Derived by walking each file's directory up to the scope root (inclusive), so
/// a selected parent that contains only child directories still gets an index
/// map. Deterministic (BTreeSet, sorted).
fn dirs_in_scope(files: &[FileFact], scope: &str) -> BTreeSet<String> {
    let scope = scope.trim_matches('/');
    let mut dirs = BTreeSet::new();
    for f in files {
        let mut d = dir_of(&f.path).to_string();
        loop {
            if within_scope(&d, scope) {
                dirs.insert(d.clone());
            }
            // Stop at the scope root (or repo root when whole-repo). The
            // within_scope guard above means a malformed row can never insert a
            // directory above scope even if the walk overshoots.
            if d == scope || d.is_empty() {
                break;
            }
            d = dir_of(&d).to_string();
        }
    }
    dirs
}

/// The immediate child directories of `dir` among the generated set (sorted, via
/// the BTreeSet's ordered iteration).
fn child_dirs<'a>(dir: &str, all_dirs: &'a BTreeSet<String>) -> Vec<&'a str> {
    all_dirs
        .iter()
        .filter(|d| d.as_str() != dir && dir_of(d) == dir)
        .map(|d| d.as_str())
        .collect()
}

// ── Renderer ─────────────────────────────────────────────────────────────────

/// Render the full set of MAP.md files (per-directory + per-file) from the
/// extracted facts. Pure and deterministic. The returned vector is sorted by
/// output path.
pub fn render_maps(facts: &MapFacts) -> Vec<RenderedMapFile> {
    let mark = marker(&facts.snapshot);
    let cov_note = coverage_note(&facts.measurement_coverage);

    // ── Lookups (all built into ordered structures) ──────────────────────
    let complexity_by_file: BTreeMap<&str, u64> = facts
        .complexity
        .iter()
        .map(|c| (c.file.as_str(), c.sum_complexity))
        .collect();

    // Symbols grouped by file, each group in TOTAL order over every rendered
    // field so two rows tied on (line, name) can never permute the bytes.
    let mut symbols_by_file: BTreeMap<&str, Vec<&SymbolFact>> = BTreeMap::new();
    for s in &facts.symbols {
        symbols_by_file.entry(s.file.as_str()).or_default().push(s);
    }
    for group in symbols_by_file.values_mut() {
        group.sort_by(|a, b| {
            a.line_start
                .cmp(&b.line_start)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.subtype.cmp(&b.subtype))
                .then_with(|| a.signature.cmp(&b.signature))
        });
    }

    // Resolved dependency targets per source file: the IMPORTS subset (per-file
    // import list) and the full IMPORTS+CALLS set (directory dependency sketch).
    let mut imports_by_source: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut dep_targets_by_source: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for e in &facts.dependency_edges {
        dep_targets_by_source
            .entry(e.source.as_str())
            .or_default()
            .insert(e.target.as_str());
        if e.edge_type == "IMPORTS" {
            imports_by_source
                .entry(e.source.as_str())
                .or_default()
                .insert(e.target.as_str());
        }
    }

    // Unresolved import specifiers per source file (external / missing paths).
    let mut unresolved_by_source: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for u in &facts.unresolved_imports {
        unresolved_by_source
            .entry(u.source.as_str())
            .or_default()
            .insert(u.specifier.as_str());
    }

    // Manifest roots longest-path-first for nearest-ancestor package lookup.
    let mut roots: Vec<&ManifestRootFact> = facts.manifest_roots.iter().collect();
    roots.sort_by(|a, b| {
        b.path
            .len()
            .cmp(&a.path.len())
            .then_with(|| a.path.cmp(&b.path))
    });

    // Files grouped by their containing directory (DIRECT files only), sorted.
    let mut files_by_dir: BTreeMap<&str, Vec<&FileFact>> = BTreeMap::new();
    for f in &facts.files {
        files_by_dir.entry(dir_of(&f.path)).or_default().push(f);
    }
    for group in files_by_dir.values_mut() {
        group.sort_by(|a, b| a.path.cmp(&b.path));
    }

    // Every directory to map — including parent-only ancestors within scope.
    let all_dirs = dirs_in_scope(&facts.files, &facts.path);

    let mut out: Vec<RenderedMapFile> = Vec::new();

    // ── Per-directory maps ───────────────────────────────────────────────
    for dir in &all_dirs {
        let dir_files: &[&FileFact] = files_by_dir
            .get(dir.as_str())
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let children = child_dirs(dir, &all_dirs);
        let contents = render_dir_map(
            dir,
            dir_files,
            &children,
            facts,
            &mark,
            &complexity_by_file,
            &dep_targets_by_source,
            &roots,
            cov_note.as_deref(),
        );
        out.push(RenderedMapFile {
            rel_path: join(dir, "MAP.md"),
            contents,
        });
    }

    // ── Per-file maps (mapped files only) ────────────────────────────────
    for f in &facts.files {
        if !is_mapped(&f.parse_status) {
            continue;
        }
        let contents = render_file_map(
            f,
            &mark,
            symbols_by_file.get(f.path.as_str()).map(|v| v.as_slice()),
            complexity_by_file.get(f.path.as_str()).copied(),
            imports_by_source.get(f.path.as_str()),
            unresolved_by_source.get(f.path.as_str()),
            cov_note.as_deref(),
        );
        let rel_path = join(dir_of(&f.path), &file_map_filename(base_of(&f.path)));
        out.push(RenderedMapFile { rel_path, contents });
    }

    out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    out
}

#[allow(clippy::too_many_arguments)]
fn render_dir_map(
    dir: &str,
    dir_files: &[&FileFact],
    children: &[&str],
    facts: &MapFacts,
    mark: &str,
    complexity_by_file: &BTreeMap<&str, u64>,
    dep_targets_by_source: &BTreeMap<&str, BTreeSet<&str>>,
    roots: &[&ManifestRootFact],
    cov_note: Option<&str>,
) -> String {
    let mut s = String::new();
    s.push_str(mark);
    s.push('\n');
    let title = if dir.is_empty() { "(repo root)" } else { dir };
    s.push_str(&format!("# {}/\n\n", title));

    if !facts.repo_name.is_empty() {
        s.push_str(&format!("Repository: {}\n", facts.repo_name));
    }
    if let Some(root) = package_for(dir, roots) {
        let name = if root.path.is_empty() {
            "(repo root)".to_string()
        } else {
            root.path.clone()
        };
        s.push_str(&format!("Package: {} ({})\n", name, root.kind));
    }
    s.push('\n');

    // ── Inventory (every direct file, mapped or not) ─────────────────────
    s.push_str(&format!("## Files ({})\n", dir_files.len()));
    for f in dir_files {
        let base = base_of(&f.path);
        let mut flags: Vec<String> = Vec::new();
        if let Some(lang) = &f.language {
            if !lang.is_empty() {
                flags.push(lang.clone());
            }
        }
        if f.is_test {
            flags.push("test".to_string());
        }
        if f.is_generated {
            flags.push("generated".to_string());
        }
        let flag_str = if flags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", flags.join(", "))
        };
        let mut facts_bits: Vec<String> = Vec::new();
        if is_mapped(&f.parse_status) {
            facts_bits.push(format!("{} symbols", f.symbol_count));
            if let Some(c) = complexity_by_file.get(f.path.as_str()) {
                facts_bits.push(format!("complexity {}", c));
            }
        } else {
            facts_bits.push(unmapped_reason(&f.parse_status, f.extractor.as_deref()));
        }
        s.push_str(&format!(
            "- {} — {}{}\n",
            base,
            facts_bits.join(", "),
            flag_str
        ));
    }
    s.push('\n');

    // ── Child-directory index (so parent/rollup maps are not dead ends) ──
    if !children.is_empty() {
        s.push_str(&format!("## Subdirectories ({})\n", children.len()));
        for c in children {
            s.push_str(&format!("- {}/\n", base_of(c)));
        }
        s.push('\n');
    }

    // ── Outbound dependency sketch (resolved import + call target files) ─
    let mut dep_targets: BTreeSet<&str> = BTreeSet::new();
    for f in dir_files {
        if let Some(targets) = dep_targets_by_source.get(f.path.as_str()) {
            for t in targets {
                dep_targets.insert(t);
            }
        }
    }
    // RECON-M-R3a (g3u, §5.3.4): fold in the compiler-witnessed CALL pairs the syntax
    // sketch lacks (`semantic`/`new_pair` only — the union never loses a pipeline pair).
    // Targets reachable ONLY through a witness pair are labeled inline; absent block →
    // exactly today's sketch (R-0). Review-1 item 2 + the §5.3.0 labeling rule, via the
    // ONE shared gate (`union_coverage_phrase`, review-2 item 1): the additions fold ONLY
    // when the block carries `accounting: "union"` + a derivable coverage basis (a union
    // value never renders unlabeled), and the basis RENDERS beside them — coverage is part
    // of the fact, stated where the fact renders.
    let mut witness_only: BTreeSet<&str> = BTreeSet::new();
    let mut witness_coverage: Option<String> = None;
    if let Some(w) = facts.witnesses.as_ref() {
        if let (Some(coverage), Some(pairs)) = (
            crate::presentation::witnesses::union_coverage_phrase(w),
            w.get("dependency_call_pairs_added")
                .and_then(|v| v.as_array()),
        ) {
            let dir_paths: BTreeSet<&str> = dir_files.iter().map(|f| f.path.as_str()).collect();
            for p in pairs {
                let (Some(src), Some(dst)) = (
                    p.get("source").and_then(|v| v.as_str()),
                    p.get("target").and_then(|v| v.as_str()),
                ) else {
                    continue;
                };
                if dir_paths.contains(src) && !dep_targets.contains(dst) {
                    witness_only.insert(dst);
                }
            }
            if !witness_only.is_empty() {
                witness_coverage = Some(coverage);
            }
        }
    }
    if !dep_targets.is_empty() || !witness_only.is_empty() {
        s.push_str(&format!(
            "## Dependencies ({})\n",
            dep_targets.len() + witness_only.len()
        ));
        s.push_str(
            "Resolved intra-repo dependency targets (import + call edges) of this directory's \
             files. External / unresolved imports appear in each file's map.\n",
        );
        // The §5.3.0 human frame for the union-witnessed additions, coverage beside the fact.
        if let Some(coverage) = &witness_coverage {
            s.push_str(&format!(
                "Compiler-witnessed additions below are reconciled — combined analyses \
                 (coverage: {coverage}).\n"
            ));
        }
        let merged: BTreeSet<&str> = dep_targets.union(&witness_only).copied().collect();
        for t in &merged {
            if witness_only.contains(t) {
                s.push_str(&format!(
                    "- {} (compiler-witnessed call — reconciled, syntax+compiler)\n",
                    t
                ));
            } else {
                s.push_str(&format!("- {}\n", t));
            }
        }
        s.push('\n');
    }

    // ── Unmapped files (never silently omitted) ──────────────────────────
    let unmapped: Vec<&&FileFact> = dir_files
        .iter()
        .filter(|f| !is_mapped(&f.parse_status))
        .collect();
    if !unmapped.is_empty() {
        s.push_str(&format!("## Unmapped files ({})\n", unmapped.len()));
        s.push_str("Present in the index but not parsed for symbols; listed so the map never hides a file.\n");
        for f in &unmapped {
            s.push_str(&format!(
                "- {} — {}\n",
                base_of(&f.path),
                unmapped_reason(&f.parse_status, f.extractor.as_deref())
            ));
        }
        s.push('\n');
    }

    // ── Coverage honesty (per-language complexity caveat / unavailable) ──
    if let Some(note) = cov_note {
        s.push_str("## Complexity coverage\n");
        s.push_str(note);
        s.push('\n');
    }

    trim_trailing_blank_lines(&mut s);
    s
}

#[allow(clippy::too_many_arguments)]
fn render_file_map(
    f: &FileFact,
    mark: &str,
    symbols: Option<&[&SymbolFact]>,
    complexity: Option<u64>,
    imports: Option<&BTreeSet<&str>>,
    unresolved: Option<&BTreeSet<&str>>,
    cov_note: Option<&str>,
) -> String {
    let mut s = String::new();
    s.push_str(mark);
    s.push('\n');
    s.push_str(&format!("# {}\n\n", f.path));

    if let Some(lang) = &f.language {
        if !lang.is_empty() {
            s.push_str(&format!("Language: {}\n", lang));
        }
    }
    if let Some(c) = complexity {
        s.push_str(&format!(
            "Complexity: {} (cyclomatic, summed over symbols)\n",
            c
        ));
    }
    s.push('\n');

    // ── Symbols with signatures ──────────────────────────────────────────
    let syms = symbols.unwrap_or(&[]);
    s.push_str(&format!("## Symbols ({})\n", syms.len()));
    for sym in syms {
        let kind = kind_label(&sym.subtype);
        let loc = match sym.line_start {
            Some(l) => format!(", L{}", l),
            None => String::new(),
        };
        let sig = match &sym.signature {
            Some(sig) if !sig.is_empty() => format!(" — `{}`", sig.trim()),
            _ => String::new(),
        };
        s.push_str(&format!("- {} ({}{}){}\n", sym.name, kind, loc, sig));
    }
    s.push('\n');

    // ── Imports (resolved intra-repo targets + external/unresolved) ──────
    let resolved: Vec<&str> = imports
        .map(|set| set.iter().copied().collect())
        .unwrap_or_default();
    let external: Vec<&str> = unresolved
        .map(|set| set.iter().copied().collect())
        .unwrap_or_default();
    if !resolved.is_empty() || !external.is_empty() {
        s.push_str(&format!(
            "## Imports ({})\n",
            resolved.len() + external.len()
        ));
        if !resolved.is_empty() {
            s.push_str(&format!("Resolved intra-repo ({}):\n", resolved.len()));
            for t in &resolved {
                s.push_str(&format!("- {}\n", t));
            }
        }
        if !external.is_empty() {
            s.push_str(&format!("External / unresolved ({}):\n", external.len()));
            for t in &external {
                s.push_str(&format!("- {}\n", t));
            }
        }
        s.push('\n');
    }

    // ── Coverage honesty (per-file context: why complexity may be absent) ─
    if let Some(note) = cov_note {
        s.push_str("## Complexity coverage\n");
        s.push_str(note);
        s.push('\n');
    }

    trim_trailing_blank_lines(&mut s);
    s
}

/// Nearest manifest-root ancestor of `dir` (longest matching root path), giving
/// the directory's owning crate/package. `roots` must be longest-path-first.
fn package_for<'a>(dir: &str, roots: &[&'a ManifestRootFact]) -> Option<&'a ManifestRootFact> {
    roots
        .iter()
        .copied()
        .find(|r| r.path.is_empty() || dir == r.path || dir.starts_with(&format!("{}/", r.path)))
}

/// Collapse trailing blank lines to exactly one terminating newline, so every
/// emitted file ends with a single `\n` (byte-stable; mirrors rgistr's
/// trailing-newline normalization).
fn trim_trailing_blank_lines(s: &mut String) {
    while s.ends_with('\n') {
        s.pop();
    }
    s.push('\n');
}

// ── Command-facing summary ───────────────────────────────────────────────────

/// A human summary of a render (for the command layer's non-JSON output).
pub fn render_summary(facts: &MapFacts, rendered: &[RenderedMapFile]) -> String {
    let dir_maps = rendered
        .iter()
        .filter(|r| r.rel_path == "MAP.md" || r.rel_path.ends_with("/MAP.md"))
        .count();
    let file_maps = rendered.len() - dir_maps;
    let unmapped = facts
        .files
        .iter()
        .filter(|f| !is_mapped(&f.parse_status))
        .count();
    let scope = if facts.path.is_empty() {
        "(whole repo)".to_string()
    } else {
        facts.path.clone()
    };
    format!(
        "map: {} directory maps, {} file maps from snapshot {} [{}]\n\
         {} file(s) listed as unmapped (not parsed).\n",
        dir_maps,
        file_maps,
        short_uid(&facts.snapshot),
        scope,
        unmapped,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A representative fixture: two directories, mapped + unmapped files (one
    /// oversized, one no-extractor), symbols including a (line,name) tie,
    /// resolved import AND call edges, unresolved imports, complexity, a manifest
    /// root, and a coverage caveat.
    fn fixture() -> MapFacts {
        MapFacts {
            repo: "r1".to_string(),
            repo_name: "demo".to_string(),
            repo_root: "/tmp/demo".to_string(),
            snapshot: "snap-abcdef012345-tail".to_string(),
            path: "src".to_string(),
            files: vec![
                FileFact {
                    path: "src/a.rs".to_string(),
                    language: Some("rust".to_string()),
                    parse_status: "parsed".to_string(),
                    extractor: Some("rust-extractor".to_string()),
                    is_test: false,
                    is_generated: false,
                    symbol_count: 4,
                },
                FileFact {
                    path: "src/b.rs".to_string(),
                    language: Some("rust".to_string()),
                    parse_status: "parsed".to_string(),
                    extractor: Some("rust-extractor".to_string()),
                    is_test: false,
                    is_generated: false,
                    symbol_count: 1,
                },
                FileFact {
                    path: "src/big.rs".to_string(),
                    language: Some("rust".to_string()),
                    parse_status: "skipped".to_string(),
                    extractor: Some("skipped:oversized".to_string()),
                    is_test: false,
                    is_generated: false,
                    symbol_count: 0,
                },
                FileFact {
                    path: "src/data.bin".to_string(),
                    language: None,
                    parse_status: "skipped".to_string(),
                    extractor: None,
                    is_test: false,
                    is_generated: false,
                    symbol_count: 0,
                },
                FileFact {
                    path: "src/util/helper.rs".to_string(),
                    language: Some("rust".to_string()),
                    parse_status: "parsed".to_string(),
                    extractor: Some("rust-extractor".to_string()),
                    is_test: false,
                    is_generated: false,
                    symbol_count: 1,
                },
            ],
            symbols: vec![
                SymbolFact {
                    file: "src/a.rs".to_string(),
                    name: "beta".to_string(),
                    qualified_name: None,
                    subtype: Some("FUNCTION".to_string()),
                    line_start: Some(20),
                    signature: Some("fn beta(x: u8) -> u8".to_string()),
                },
                SymbolFact {
                    file: "src/a.rs".to_string(),
                    name: "Alpha".to_string(),
                    qualified_name: None,
                    subtype: Some("STRUCT".to_string()),
                    line_start: Some(5),
                    signature: None,
                },
                // A (line,name) TIE: same line 10, same name `twin`, distinct
                // subtype + signature. Total order must sort by subtype then
                // signature so permuting the input can never reorder these two.
                SymbolFact {
                    file: "src/a.rs".to_string(),
                    name: "twin".to_string(),
                    qualified_name: None,
                    subtype: Some("STRUCT".to_string()),
                    line_start: Some(10),
                    signature: None,
                },
                SymbolFact {
                    file: "src/a.rs".to_string(),
                    name: "twin".to_string(),
                    qualified_name: None,
                    subtype: Some("FUNCTION".to_string()),
                    line_start: Some(10),
                    signature: Some("fn twin() -> A".to_string()),
                },
                SymbolFact {
                    file: "src/b.rs".to_string(),
                    name: "go".to_string(),
                    qualified_name: None,
                    subtype: Some("FUNCTION".to_string()),
                    line_start: Some(1),
                    signature: Some("fn go()".to_string()),
                },
                SymbolFact {
                    file: "src/util/helper.rs".to_string(),
                    name: "help".to_string(),
                    qualified_name: None,
                    subtype: Some("FUNCTION".to_string()),
                    line_start: Some(3),
                    signature: Some("fn help()".to_string()),
                },
            ],
            dependency_edges: vec![
                DepEdgeFact {
                    source: "src/a.rs".to_string(),
                    target: "src/b.rs".to_string(),
                    edge_type: "IMPORTS".to_string(),
                },
                DepEdgeFact {
                    source: "src/a.rs".to_string(),
                    target: "src/util/helper.rs".to_string(),
                    edge_type: "IMPORTS".to_string(),
                },
                DepEdgeFact {
                    source: "src/a.rs".to_string(),
                    target: "src/b.rs".to_string(),
                    edge_type: "CALLS".to_string(),
                },
                // b.rs -> a.rs is a CALL only (no import) — the directory sketch
                // must surface it even though IMPORTS-only would miss it.
                DepEdgeFact {
                    source: "src/b.rs".to_string(),
                    target: "src/a.rs".to_string(),
                    edge_type: "CALLS".to_string(),
                },
            ],
            unresolved_imports: vec![
                UnresolvedImportFact {
                    source: "src/a.rs".to_string(),
                    specifier: "std::collections".to_string(),
                },
                UnresolvedImportFact {
                    source: "src/a.rs".to_string(),
                    specifier: "serde".to_string(),
                },
            ],
            complexity: vec![ComplexityFact {
                file: "src/a.rs".to_string(),
                sum_complexity: 12,
            }],
            manifest_roots: vec![ManifestRootFact {
                path: "src".to_string(),
                kind: "rust crate".to_string(),
            }],
            measurement_coverage: serde_json::json!({
                "status": "available",
                "caveat": "Complexity measured for Rust only."
            }),
            witnesses: None,
        }
    }

    fn dir_map<'a>(rendered: &'a [RenderedMapFile], rel: &str) -> &'a str {
        &rendered
            .iter()
            .find(|r| r.rel_path == rel)
            .unwrap_or_else(|| panic!("{} not rendered", rel))
            .contents
    }

    #[test]
    fn short_uid_uses_snapshot_content_hash_not_repo_prefix() {
        // Real snapshot uid: `<repo_uid>/<timestamp>/<content-hash>`. The token
        // must be the distinguishing trailing hash, not the shared repo prefix.
        assert_eq!(
            short_uid("repo_01kxkw2dw124p8wkpmaqzx0amy/2026-07-15T21:49:03.823Z/9564665e"),
            "9564665e"
        );
        // Two snapshots of the SAME repo must get distinct tokens.
        let a = short_uid("repo_R/2026-01-01T00:00:00Z/aaaaaaaa");
        let b = short_uid("repo_R/2026-01-02T00:00:00Z/bbbbbbbb");
        assert_ne!(a, b, "same-repo snapshots need distinct provenance tokens");
        // No-slash uid → 12-char prefix; empty → unknown (never fabricated).
        assert_eq!(short_uid("snap-abcdef012345-tail"), "snap-abcdef0");
        assert_eq!(short_uid(""), "unknown");
    }

    #[test]
    fn file_map_filename_mirrors_rgistr_node_semantics() {
        assert_eq!(file_map_filename("generator.ts"), "generator_ts_MAP.md");
        assert_eq!(file_map_filename("map.rs"), "map_rs_MAP.md");
        // internal dots preserved; only the final extension dot becomes '_'.
        assert_eq!(
            file_map_filename("vitest.config.ts"),
            "vitest.config_ts_MAP.md"
        );
        assert_eq!(
            file_map_filename("copy-runtime-assets.mjs"),
            "copy-runtime-assets_mjs_MAP.md"
        );
        // no extension / leading-dot dotfile -> double underscore (Node parity).
        assert_eq!(file_map_filename("Makefile"), "Makefile__MAP.md");
        assert_eq!(file_map_filename(".gitignore"), ".gitignore__MAP.md");
    }

    #[test]
    fn unmapped_reason_distinguishes_oversized_from_no_extractor() {
        // The SAME parse_status `skipped` reads distinctly by its skip-cause.
        assert_eq!(
            unmapped_reason("skipped", Some("skipped:oversized")),
            "skipped — exceeds the size cap"
        );
        assert_eq!(
            unmapped_reason("skipped", None),
            "skipped — no extractor for this language"
        );
        // An unexpected extractor tag on a skipped file is surfaced verbatim.
        assert_eq!(unmapped_reason("skipped", Some("weird")), "skipped — weird");
        assert_eq!(
            unmapped_reason("failed", Some("rust-extractor")),
            "parse failed"
        );
        // An unknown status is surfaced, never hidden.
        assert_eq!(
            unmapped_reason("mystery", None),
            "not parsed (status: mystery)"
        );
    }

    #[test]
    fn every_rendered_file_carries_marker_with_snapshot_provenance() {
        let rendered = render_maps(&fixture());
        assert!(!rendered.is_empty());
        for r in &rendered {
            assert!(
                r.contents.starts_with(
                    "<!-- generated by rmap map from snapshot snap-abcdef0; do not hand-edit -->\n"
                ),
                "missing/incorrect marker in {}:\n{}",
                r.rel_path,
                r.contents
            );
            assert!(
                r.contents.ends_with('\n') && !r.contents.ends_with("\n\n"),
                "file must end with exactly one newline: {}",
                r.rel_path
            );
        }
    }

    #[test]
    fn render_is_byte_identical_across_two_renders() {
        let facts = fixture();
        assert_eq!(render_maps(&facts), render_maps(&facts));
    }

    #[test]
    fn render_is_stable_under_permuted_input() {
        let facts = fixture();
        let baseline = render_maps(&facts);

        // Permute EVERY input vector; a deterministic renderer must not care —
        // including the (line,name)-tied `twin` symbols, whose order must be
        // pinned by the subtype/signature tie-breakers.
        let mut permuted = facts.clone();
        permuted.files.reverse();
        permuted.symbols.reverse();
        permuted.dependency_edges.reverse();
        permuted.unresolved_imports.reverse();
        permuted.complexity.reverse();
        permuted.manifest_roots.reverse();

        assert_eq!(
            baseline,
            render_maps(&permuted),
            "output must be independent of input row order"
        );
    }

    #[test]
    fn symbol_ordering_is_total_over_tied_name_and_line() {
        // The two `twin` rows tie on (line 10, name). The tie-break is subtype
        // then signature: FUNCTION (< STRUCT) first. Deterministic regardless of
        // input order.
        let a_map = {
            let rendered = render_maps(&fixture());
            dir_map(&rendered, "src/a_rs_MAP.md").to_string()
        };
        let func_twin = a_map.find("- twin (function, L10)").expect("function twin");
        let struct_twin = a_map.find("- twin (struct, L10)").expect("struct twin");
        assert!(
            func_twin < struct_twin,
            "FUNCTION twin must sort before STRUCT twin:\n{}",
            a_map
        );
    }

    #[test]
    fn unmapped_files_listed_with_distinct_reasons_never_omitted() {
        let rendered = render_maps(&fixture());
        let src = dir_map(&rendered, "src/MAP.md");
        assert!(src.contains("## Unmapped files (2)"));
        // Oversized vs no-extractor read distinctly (reviewer-1 fidelity).
        assert!(
            src.contains("big.rs — skipped — exceeds the size cap"),
            "oversized reason:\n{}",
            src
        );
        assert!(
            src.contains("data.bin — skipped — no extractor for this language"),
            "no-extractor reason:\n{}",
            src
        );
        // Neither skipped file gets a per-file map.
        assert!(!rendered.iter().any(|r| r.rel_path.contains("big")));
        assert!(!rendered.iter().any(|r| r.rel_path.contains("data")));
    }

    #[test]
    fn file_imports_list_resolved_and_external() {
        let rendered = render_maps(&fixture());
        let a = dir_map(&rendered, "src/a_rs_MAP.md");
        // Resolved intra-repo = IMPORTS edges only (NOT the CALLS target); the
        // external/unresolved specifiers are listed, not dropped.
        assert!(
            a.contains(
                "## Imports (4)\n\
                 Resolved intra-repo (2):\n- src/b.rs\n- src/util/helper.rs\n\
                 External / unresolved (2):\n- serde\n- std::collections\n"
            ),
            "imports block:\n{}",
            a
        );
    }

    #[test]
    fn symbols_render_signature_kind_line_and_complexity() {
        let rendered = render_maps(&fixture());
        let a = dir_map(&rendered, "src/a_rs_MAP.md");
        // Ordered by line: Alpha (L5) before twin (L10) before beta (L20).
        assert!(a.find("Alpha").unwrap() < a.find("beta").unwrap());
        assert!(a.contains("- Alpha (struct, L5)\n"));
        assert!(a.contains("- beta (function, L20) — `fn beta(x: u8) -> u8`\n"));
        assert!(a.contains("Language: rust\n"));
        assert!(a.contains("Complexity: 12 (cyclomatic, summed over symbols)\n"));
    }

    #[test]
    fn directory_sketch_includes_call_edges_and_labels_resolved() {
        let rendered = render_maps(&fixture());
        let src = dir_map(&rendered, "src/MAP.md");
        assert!(src.contains("# src/\n"));
        assert!(src.contains("Repository: demo\n"));
        assert!(src.contains("Package: src (rust crate)\n"));
        assert!(src.contains("## Files (4)"));
        assert!(src.contains("- a.rs — 4 symbols, complexity 12 [rust]\n"));
        // The child directory is indexed.
        assert!(src.contains("## Subdirectories (1)\n- util/\n"));
        // src/a.rs is a dependency ONLY via b.rs's CALL edge — proof the sketch
        // uses the import+call basis, not imports alone. The label states the
        // resolved-only scope.
        assert!(
            src.contains("## Dependencies (3)")
                && src.contains("Resolved intra-repo dependency targets (import + call edges)"),
            "dependency sketch:\n{}",
            src
        );
        assert!(src.contains("- src/a.rs\n"));
        assert!(src.contains("- src/util/helper.rs\n"));
        assert!(src.contains("## Complexity coverage\nComplexity measured for Rust only.\n"));
        assert!(rendered.iter().any(|r| r.rel_path == "src/util/MAP.md"));
    }

    /// Review-1 items 2+3: a NONZERO g3u overlay through FINAL document rendering. The
    /// witness-only pair folds in labeled AND the section carries the coverage basis
    /// (§5.3.0: a union value never renders without accounting + coverage; VISION: coverage
    /// renders with the fact); a pair whose target the pipeline sketch already reaches is
    /// subtracted (renders plain, never double-counted, never re-labeled).
    #[test]
    fn witness_pairs_render_labeled_with_coverage_and_subtract_known_targets() {
        let mut facts = fixture();
        facts.witnesses = Some(serde_json::json!({
            "accounting": "union",
            "coverage": {"languages": ["TypeScript"], "partitions": ["app", "lib"], "fingerprint": "fp9"},
            "pair_delta": 2,
            "dependency_call_pairs_added": [
                // Witness-only: target NOT in the pipeline sketch → labeled + counted.
                {"source": "src/a.rs", "target": "src/net/client.rs"},
                // Target already in the pipeline sketch → subtracted at render.
                {"source": "src/a.rs", "target": "src/b.rs"},
            ],
        }));
        let rendered = render_maps(&facts);
        let src = dir_map(&rendered, "src/MAP.md");
        // 3 pipeline targets + exactly 1 witness-only addition (the b.rs pair subtracted).
        assert!(src.contains("## Dependencies (4)"), "{src}");
        assert!(
            src.contains(
                "Compiler-witnessed additions below are reconciled — combined analyses \
                 (coverage: TypeScript (2 partitions))."
            ),
            "the coverage basis must render beside the witness additions: {src}"
        );
        assert!(
            src.contains(
                "- src/net/client.rs (compiler-witnessed call — reconciled, syntax+compiler)\n"
            ),
            "{src}"
        );
        assert!(
            src.contains("- src/b.rs\n") && !src.contains("- src/b.rs (compiler-witnessed"),
            "a pipeline-reachable target renders plain — subtraction holds: {src}"
        );
    }

    /// Review-1 item 2 (the labeling gate): a witness block whose coverage basis is missing
    /// or malformed must fold NOTHING — a union value never renders unlabeled (§5.3.0), and
    /// the sketch stays exactly the pipeline sketch.
    #[test]
    fn witness_pairs_without_coverage_basis_never_fold() {
        let mut facts = fixture();
        facts.witnesses = Some(serde_json::json!({
            "accounting": "union",
            // coverage ABSENT (malformed/additive payload)
            "pair_delta": 1,
            "dependency_call_pairs_added": [
                {"source": "src/a.rs", "target": "src/net/client.rs"},
            ],
        }));
        let rendered = render_maps(&facts);
        let src = dir_map(&rendered, "src/MAP.md");
        assert!(src.contains("## Dependencies (3)"), "{src}");
        assert!(!src.contains("compiler-witnessed"), "{src}");
        assert!(!src.contains("src/net/client.rs"), "{src}");
    }

    /// Review-2 item 1 (the gate's OTHER half): a witness block with a valid coverage basis
    /// but no `accounting: "union"` marker folds NOTHING either — both §5.3.0 labels are
    /// required, through the one shared gate.
    #[test]
    fn witness_pairs_without_the_accounting_marker_never_fold() {
        let mut facts = fixture();
        facts.witnesses = Some(serde_json::json!({
            // accounting ABSENT; coverage well-formed
            "coverage": {"languages": ["TypeScript"], "partitions": ["app"], "fingerprint": "fp9"},
            "pair_delta": 1,
            "dependency_call_pairs_added": [
                {"source": "src/a.rs", "target": "src/net/client.rs"},
            ],
        }));
        let rendered = render_maps(&facts);
        let src = dir_map(&rendered, "src/MAP.md");
        assert!(src.contains("## Dependencies (3)"), "{src}");
        assert!(!src.contains("compiler-witnessed"), "{src}");
        assert!(!src.contains("src/net/client.rs"), "{src}");
    }

    #[test]
    fn parent_only_directory_gets_index_map_with_subdirectories() {
        // A selected parent whose code lives ONLY in a descendant directory must
        // still get an index map (reviewer-3 / rgistr's getFoldersForGeneration).
        let facts = MapFacts {
            repo_name: "demo".to_string(),
            snapshot: "snapPARENT".to_string(),
            path: "a".to_string(),
            files: vec![FileFact {
                path: "a/b/c/leaf.rs".to_string(),
                language: Some("rust".to_string()),
                parse_status: "parsed".to_string(),
                extractor: Some("rust-extractor".to_string()),
                is_test: false,
                is_generated: false,
                symbol_count: 0,
            }],
            ..Default::default()
        };
        let rendered = render_maps(&facts);
        // All three directories get maps: a (parent-only), a/b (parent-only), a/b/c.
        for rel in ["a/MAP.md", "a/b/MAP.md", "a/b/c/MAP.md"] {
            assert!(
                rendered.iter().any(|r| r.rel_path == rel),
                "missing dir map {}: {:?}",
                rel,
                rendered.iter().map(|r| &r.rel_path).collect::<Vec<_>>()
            );
        }
        // The parent-only `a` map has zero direct files but indexes its child.
        let a = dir_map(&rendered, "a/MAP.md");
        assert!(a.contains("## Files (0)"), "parent-only files:\n{}", a);
        assert!(
            a.contains("## Subdirectories (1)\n- b/\n"),
            "child index:\n{}",
            a
        );
    }

    #[test]
    fn coverage_unavailable_reason_is_rendered_and_reaches_unmeasured_files() {
        // Coverage could not be read: the explicit reason must be rendered (not
        // dropped), AND it must reach a per-file map whose complexity is absent.
        let facts = MapFacts {
            repo_name: "demo".to_string(),
            snapshot: "snapCOV".to_string(),
            path: "m".to_string(),
            files: vec![FileFact {
                path: "m/x.py".to_string(),
                language: Some("python".to_string()),
                parse_status: "parsed".to_string(),
                extractor: Some("python-extractor".to_string()),
                is_test: false,
                is_generated: false,
                symbol_count: 1,
            }],
            symbols: vec![SymbolFact {
                file: "m/x.py".to_string(),
                name: "f".to_string(),
                qualified_name: None,
                subtype: Some("FUNCTION".to_string()),
                line_start: Some(1),
                signature: Some("def f()".to_string()),
            }],
            measurement_coverage: serde_json::json!({
                "status": "unavailable",
                "reason": "complexity measurement coverage could not be read for this snapshot"
            }),
            ..Default::default()
        };
        let rendered = render_maps(&facts);
        let note = "complexity measurement coverage could not be read for this snapshot";
        let dir = dir_map(&rendered, "m/MAP.md");
        assert!(
            dir.contains("## Complexity coverage\n") && dir.contains(note),
            "dir coverage note (unavailable reason) dropped:\n{}",
            dir
        );
        // The python file has no complexity fact; its map still carries the
        // coverage context so the absence is explained, never read as zero.
        let file = dir_map(&rendered, "m/x_py_MAP.md");
        assert!(
            !file.contains("Complexity:")
                && file.contains("## Complexity coverage\n")
                && file.contains(note),
            "per-file coverage context missing:\n{}",
            file
        );
    }

    #[test]
    fn golden_directory_map_exact_bytes() {
        // A minimal fixture pinned byte-for-byte: the honesty contract (marker,
        // inventory, unmapped, no trailing blank) is locked against drift.
        let facts = MapFacts {
            repo: "r1".to_string(),
            repo_name: "demo".to_string(),
            snapshot: "snapXYZ".to_string(),
            path: "lib".to_string(),
            files: vec![
                FileFact {
                    path: "lib/one.rs".to_string(),
                    language: Some("rust".to_string()),
                    parse_status: "parsed".to_string(),
                    extractor: Some("rust-extractor".to_string()),
                    is_test: false,
                    is_generated: false,
                    symbol_count: 1,
                },
                FileFact {
                    path: "lib/skip.bin".to_string(),
                    language: None,
                    parse_status: "skipped".to_string(),
                    extractor: None,
                    is_test: false,
                    is_generated: false,
                    symbol_count: 0,
                },
            ],
            symbols: vec![SymbolFact {
                file: "lib/one.rs".to_string(),
                name: "run".to_string(),
                qualified_name: None,
                subtype: Some("FUNCTION".to_string()),
                line_start: Some(1),
                signature: Some("fn run()".to_string()),
            }],
            ..Default::default()
        };
        let rendered = render_maps(&facts);
        let dir = dir_map(&rendered, "lib/MAP.md");
        // one.rs has no complexity fact, so its inventory line carries only the
        // symbol count; skip.bin (no extractor) is unmapped and appears in both
        // the inventory and the dedicated honesty section; no manifest root → no
        // Package line; no edges → no Dependencies; no children → no
        // Subdirectories; null coverage → no coverage section. Exactly one
        // terminating newline.
        let expected = "\
<!-- generated by rmap map from snapshot snapXYZ; do not hand-edit -->
# lib/

Repository: demo

## Files (2)
- one.rs — 1 symbols [rust]
- skip.bin — skipped — no extractor for this language

## Unmapped files (1)
Present in the index but not parsed for symbols; listed so the map never hides a file.
- skip.bin — skipped — no extractor for this language
";
        assert_eq!(dir, expected, "golden dir map drift:\n{}", dir);
    }

    #[test]
    fn partial_daemon_payload_fails_closed_never_empty_facts() {
        // Architecture Rule 6 (`null`=unknown, empty=known-zero — never conflate)
        // + VISION honest degradation. A `map` response MISSING a required fact
        // collection must NOT deserialize into an empty Vec that renders as a
        // confident zero ("0 files", "0 symbols") — that would present "the daemon
        // didn't say" as "measured and absent". Absence fails closed at the DTO
        // boundary; only a PRESENT-but-empty collection is a legitimate known-zero.
        let complete = serde_json::json!({
            "repo": "r",
            "repo_name": "demo",
            "repo_root": "/tmp/demo",
            "snapshot": "repo_x/2026-07-15T00:00:00Z/deadbeef",
            "path": "src",
            "files": [],
            "symbols": [],
            "dependency_edges": [],
            "unresolved_imports": [],
            "complexity": [],
            "manifest_roots": [],
            "measurement_coverage": { "status": "available" }
        });
        // Baseline: the full contract (all keys present, collections empty)
        // deserializes — an honest known-zero the daemon explicitly stated.
        assert!(
            serde_json::from_value::<MapFacts>(complete.clone()).is_ok(),
            "a complete payload with present-but-empty collections must deserialize"
        );

        // Drop each required fact field in turn: every omission must fail closed
        // (Err), never silently become an empty/zero claim.
        for key in [
            "files",
            "symbols",
            "dependency_edges",
            "unresolved_imports",
            "complexity",
            "manifest_roots",
            "measurement_coverage",
        ] {
            let mut partial = complete.clone();
            partial
                .as_object_mut()
                .expect("object")
                .remove(key)
                .expect("key present before removal");
            let parsed = serde_json::from_value::<MapFacts>(partial);
            assert!(
                parsed.is_err(),
                "missing `{}` must fail closed at the DTO boundary, not default to \
                 an empty/zero fact claim (Rule 6: absent != known-empty)",
                key
            );
        }
    }
}
