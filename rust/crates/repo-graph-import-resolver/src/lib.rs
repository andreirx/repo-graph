#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! Pure cross-partition TS import resolver (IMPORTS-XPART-RESOLUTION-1).
//!
//! Given a global FILE inventory (repo-relative path -> FILE node key) and import candidates (the
//! importing file's FILE key + the raw specifier), resolve RELATIVE specifiers to a target FILE via
//! extension/index rules — deterministically, with NO filesystem / producer / daemon access.
//!
//! Scope (D2): relative + extension/index ONLY. Non-relative (package) specifiers are `PackageExternal`
//! (out of scope; needs package/tsconfig resolution). Ambiguity (more than one candidate FILE matches) is
//! reported as `Ambiguous` and NEVER silently picked.
//!
//! Output is in-memory edge CANDIDATES with `EdgeBasis::AstImportFileInventoryResolved`. The caller (a
//! later wiring slice) inserts them into the LiveGraph IN-MEMORY; they are NEVER persisted in a
//! per-partition IR / warm cache (per-partition cache coherence, F1).

use std::collections::{BTreeSet, HashMap};

use repo_graph_ir::{EdgeBasis, TsconfigAliasConfig};

/// IMPORTS-PACKAGE-RESOLUTION-1: the class of a NON-RELATIVE (bare) TS import for module-cycle completeness.
/// Refines the single ingest `PackageExternal` bucket using POSITIVE package.json metadata. PURE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageImportClass {
    /// The specifier's package name matches a LOADED workspace package. Cycle-relevant, but this slice does
    /// NOT form the module edge (the producer gives no target file; package entries point to unindexed
    /// `dist/`) -> it BLOCKS completeness, labelled. The edge is IMPORTS-WORKSPACE-PACKAGE-EDGE-1.
    WorkspaceLocalUnedgeable,
    /// POSITIVE external evidence: a `node:`/builtin specifier, OR a DECLARED dependency. An external package
    /// cannot be in a REPO-LOCAL module cycle -> BENIGN (does not block). Never inferred from absence.
    ExternalPackageNonLocal,
    /// Neither workspace-local nor a declared external (e.g. a tsconfig path alias `@/lib`) -> a genuine
    /// unknown -> BLOCKS (honest).
    PackageUnresolved,
}

/// Node.js builtin module names (the bare forms; the `node:` prefix is handled separately). Closed set --
/// a bare specifier matching one cannot be a repo-local cycle source.
const NODE_BUILTINS: &[&str] = &[
    "assert",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "crypto",
    "dgram",
    "dns",
    "events",
    "fs",
    "http",
    "http2",
    "https",
    "net",
    "os",
    "path",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "stream",
    "string_decoder",
    "timers",
    "tls",
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "worker_threads",
    "zlib",
];

/// The PACKAGE NAME of a bare import specifier: `@scope/pkg/sub` -> `@scope/pkg`; `pkg/sub` -> `pkg`;
/// `pkg` -> `pkg`. (A `node:` specifier is handled by the caller before this.)
pub fn package_name_of(specifier: &str) -> String {
    let mut segs = specifier.split('/');
    let first = segs.next().unwrap_or(specifier);
    if first.starts_with('@') {
        match segs.next() {
            Some(second) => format!("{first}/{second}"),
            None => first.to_string(),
        }
    } else {
        first.to_string()
    }
}

/// Classify a NON-RELATIVE import specifier (IMPORTS-PACKAGE-RESOLUTION-1 D1 + IMPORTS-PACKAGE-EXTERNAL-
/// EVIDENCE-1). PURE -- given the loaded `workspace_packages`, the source partition's
/// `declared_dependencies`, and `external_node_modules` (ingest-captured: the package resolves to a REAL
/// node_modules/@types install, NOT a workspace symlink). Precedence (the trust hinge): `node:`/builtin ->
/// external; WORKSPACE MAP -> WorkspaceLocalUnedgeable (BEFORE node_modules, so a workspace package symlinked
/// into node_modules stays local); declared dep OR external_node_modules -> external; else -> PackageUnresolved.
/// CONSERVATIVE: external requires POSITIVE evidence; an unknown bare specifier (not declared, not in
/// node_modules) BLOCKS; never inferred from absence in the workspace map.
pub fn classify_package_import(
    specifier: &str,
    workspace_packages: &BTreeSet<String>,
    declared_dependencies: &BTreeSet<String>,
    external_node_modules: bool,
) -> PackageImportClass {
    if specifier.starts_with("node:") {
        return PackageImportClass::ExternalPackageNonLocal;
    }
    let pkg = package_name_of(specifier);
    if NODE_BUILTINS.contains(&pkg.as_str()) {
        return PackageImportClass::ExternalPackageNonLocal;
    }
    // WORKSPACE precedes node_modules (a workspace package symlinked into node_modules is NOT external).
    if workspace_packages.contains(&pkg) {
        return PackageImportClass::WorkspaceLocalUnedgeable;
    }
    if declared_dependencies.contains(&pkg) || external_node_modules {
        return PackageImportClass::ExternalPackageNonLocal;
    }
    PackageImportClass::PackageUnresolved
}

/// Global FILE inventory: repo-relative path -> FILE node key.
#[derive(Debug, Clone, Default)]
pub struct FileInventory {
    by_path: HashMap<String, String>,
}

impl FileInventory {
    /// Build from FILE node keys (`{repo}:{repo-relative-path}:FILE`). Non-`:FILE` keys are ignored.
    pub fn from_file_keys<I: IntoIterator<Item = String>>(keys: I) -> Self {
        let mut by_path = HashMap::new();
        for k in keys {
            if let Some(path) = file_key_path(&k) {
                by_path.insert(path.to_string(), k.clone());
            }
        }
        FileInventory { by_path }
    }

    /// The FILE key for a repo-relative path, if present. PUBLIC so a caller (the LiveGraph overlay,
    /// IMPORTS-XPART-WIRING-1) can resolve the IMPORTING file's actual FILE key from the SAME inventory
    /// rather than reconstructing `{repo}:{path}:FILE` by hand — one source of truth, and `None` (skip)
    /// when the importing file is not resident.
    pub fn file_key_for(&self, path: &str) -> Option<&str> {
        self.by_path.get(path).map(String::as_str)
    }
}

/// Extract the repo-relative path from a FILE node key `{repo}:{path}:FILE` (path is everything after the
/// first `:` and before the `:FILE` suffix). `None` if the key is not a `:FILE` key.
///
/// PUBLIC (MODULE-AGGREGATION-1): the proven, colon-safe key parser. `repo_uid` is `repo_<ulid>` (no
/// colon), so the FIRST `:` is the repo/path boundary and any colon WITHIN the path is preserved. Callers
/// that need a FILE key's repo-relative path (e.g. directory/module aggregation) MUST reuse this rather
/// than re-slice.
pub fn file_key_path(key: &str) -> Option<&str> {
    let inner = key.strip_suffix(":FILE")?;
    inner.find(':').map(|i| &inner[i + 1..])
}

/// An import to resolve: the importing file's FILE node key + the raw specifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportCandidate {
    /// The importing file's FILE node key (e.g. `repo:packages/a/src/main.ts:FILE`).
    pub source_file_key: String,
    /// The raw module specifier as written (e.g. `../../b/src/foo`).
    pub raw_specifier: String,
}

/// Why an import candidate did not resolve to exactly one FILE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnresolvedReason {
    /// No inventory FILE matched any extension/index candidate.
    NotFound,
    /// MORE THAN ONE inventory FILE matched (e.g. both `foo.ts` and `foo.tsx`). Never silently picked.
    Ambiguous,
    /// A non-relative (package / bare) specifier — out of scope (needs package/tsconfig resolution).
    PackageExternal,
    /// The source file key was malformed (not a `:FILE` key) — the importer's directory is unknown.
    BadSourceKey,
}

/// A resolved cross-partition import edge candidate. The caller stamps these into the LiveGraph IN-MEMORY
/// (never a persisted PartitionIr / warm cache).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedImportEdgeCandidate {
    /// Importing file's FILE node key.
    pub src_file_key: String,
    /// Resolved target file's FILE node key.
    pub dst_file_key: String,
    /// Always [`EdgeBasis::AstImportFileInventoryResolved`].
    pub basis: EdgeBasis,
    /// The raw specifier that resolved.
    pub raw_specifier: String,
    /// The matched repo-relative target path.
    pub resolved_repo_path: String,
}

/// An unresolved candidate + the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedImport {
    /// The candidate that did not resolve.
    pub candidate: ImportCandidate,
    /// Why.
    pub reason: UnresolvedReason,
}

/// The resolution report.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportResolutionReport {
    /// Candidates resolved to exactly one FILE.
    pub resolved: Vec<ResolvedImportEdgeCandidate>,
    /// Candidates that did not resolve (with reason).
    pub unresolved: Vec<UnresolvedImport>,
}

/// IMPORTS-ASSET-AND-LITERAL-EXT-1: known NON-CODE asset extensions (CLOSED allowlist -- styles/images/fonts).
/// A relative import ending in one is non-cycle-relevant (benign), NEVER a graph edge. `.json` is DATA, NOT
/// here. Unknown extensions are NEVER assets.
const ASSET_EXTENSIONS: &[&str] = &[
    "css", "scss", "sass", "less", "styl", // styles
    "svg", "png", "jpg", "jpeg", "gif", "webp", "avif", "ico", "bmp", // images
    "woff", "woff2", "ttf", "eot", "otf", // fonts
];

/// TS SOURCE file extensions for the literal-source-extension exact match (`.d.ts` ends with `.ts`).
const SOURCE_EXTENSIONS: &[&str] = &[".ts", ".tsx", ".mts", ".cts"];

/// IMPORTS-ASSET-AND-LITERAL-EXT-1: is `specifier` a known NON-CODE ASSET import (by the extension of its LAST
/// path segment)? CLOSED allowlist; an unknown extension is NEVER an asset. PURE.
pub fn is_asset_specifier(specifier: &str) -> bool {
    let last_segment = specifier.rsplit('/').next().unwrap_or(specifier);
    last_segment
        .rsplit_once('.')
        .is_some_and(|(_, ext)| ASSET_EXTENSIONS.contains(&ext))
}

/// True if the normalized base already ends in a TS source extension (`./App.tsx` -> base `.../App.tsx`).
fn base_ends_with_source_extension(base: &str) -> bool {
    SOURCE_EXTENSIONS.iter().any(|ext| base.ends_with(ext))
}

/// The DETERMINISTIC candidate paths tried for a normalized target base `T` (extensions THEN index, PLUS the
/// IMPORTS-RELATIVE-RESOLUTION-COMPLETE-1 ESM output->source substitutions). The resolver collects ALL
/// inventory matches across this set; >1 match -> `Ambiguous` (no silent extension preference -- order here is
/// irrelevant because the caller counts matches, it does NOT take the first).
fn candidate_paths(base: &str) -> Vec<String> {
    let mut out = vec![
        format!("{base}.ts"),
        format!("{base}.tsx"),
        format!("{base}.d.ts"),
        format!("{base}.mts"),
        format!("{base}.cts"),
        format!("{base}/index.ts"),
        format!("{base}/index.tsx"),
    ];
    // TS moduleResolution: a relative import that writes the OUTPUT extension resolves to its SOURCE file
    // (`./x.js` -> `x.ts`/`x.tsx`). Stem = base minus the JS-family extension. Both substitution candidates
    // for `.js` join the set -> if BOTH `x.ts` and `x.tsx` exist the caller reports Ambiguous (never picks).
    if let Some(stem) = base.strip_suffix(".js") {
        out.push(format!("{stem}.ts"));
        out.push(format!("{stem}.tsx"));
    } else if let Some(stem) = base.strip_suffix(".jsx") {
        out.push(format!("{stem}.tsx"));
    } else if let Some(stem) = base.strip_suffix(".mjs") {
        out.push(format!("{stem}.mts"));
    } else if let Some(stem) = base.strip_suffix(".cjs") {
        out.push(format!("{stem}.cts"));
    }
    out
}

/// Join a repo-relative directory with a relative specifier, resolving `.`/`..` (a `..` that escapes the
/// directory pops the parent — so an import from `packages/a/src` of `../../b/x` -> `packages/b/x`). PUBLIC
/// (MODULE-CYCLES-COMPARE-CLASSIFY-1): the classifier reuses this to normalize a `StaticUnresolved` import's
/// target path from its source MODULE directory (no new path math).
pub fn normalize_join(dir: &str, spec: &str) -> String {
    let mut parts: Vec<&str> = if dir.is_empty() {
        Vec::new()
    } else {
        dir.split('/').collect()
    };
    for seg in spec.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// Directory of a repo-relative file path (everything before the last `/`; empty if none). PUBLIC
/// (MODULE-AGGREGATION-1): matches the SQLite cycle path's `get_module_path` (dirname) — empty result means
/// the file is at the repo root and has NO module.
pub fn dirname(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..i],
        None => "",
    }
}

/// Resolve import candidates against the global FILE inventory (D2: relative + extension/index ONLY).
/// Multiple matches -> `Ambiguous` (never silently picked). Non-relative -> `PackageExternal`. Pure +
/// deterministic.
pub fn resolve_imports(
    inv: &FileInventory,
    candidates: Vec<ImportCandidate>,
) -> ImportResolutionReport {
    let mut report = ImportResolutionReport::default();
    for cand in candidates {
        if !cand.raw_specifier.starts_with('.') {
            report.unresolved.push(UnresolvedImport {
                candidate: cand,
                reason: UnresolvedReason::PackageExternal,
            });
            continue;
        }
        let src_path = match file_key_path(&cand.source_file_key) {
            Some(p) => p.to_string(),
            None => {
                report.unresolved.push(UnresolvedImport {
                    candidate: cand,
                    reason: UnresolvedReason::BadSourceKey,
                });
                continue;
            }
        };
        let base = normalize_join(dirname(&src_path), &cand.raw_specifier);
        // IMPORTS-ASSET-AND-LITERAL-EXT-1: a literal SOURCE-extension import (`./App.tsx`) resolves to the
        // EXACT FILE node if present -- EXCLUSIVE (do NOT append candidates / risk Ambiguity). Fall through to
        // the normal candidate matching only when no exact FILE node exists.
        if base_ends_with_source_extension(&base) {
            if let Some(key) = inv.file_key_for(&base) {
                report.resolved.push(ResolvedImportEdgeCandidate {
                    src_file_key: cand.source_file_key,
                    dst_file_key: key.to_string(),
                    basis: EdgeBasis::AstImportFileInventoryResolved,
                    raw_specifier: cand.raw_specifier,
                    resolved_repo_path: base,
                });
                continue;
            }
        }
        // Collect ALL inventory matches (extension/index); >1 -> Ambiguous (no silent priority).
        let mut matches: Vec<(String, String)> = Vec::new();
        for path in candidate_paths(&base) {
            if let Some(key) = inv.file_key_for(&path) {
                matches.push((path, key.to_string()));
            }
        }
        match matches.len() {
            0 => report.unresolved.push(UnresolvedImport {
                candidate: cand,
                reason: UnresolvedReason::NotFound,
            }),
            1 => {
                let (path, dst_key) = matches.into_iter().next().unwrap();
                report.resolved.push(ResolvedImportEdgeCandidate {
                    src_file_key: cand.source_file_key,
                    dst_file_key: dst_key,
                    basis: EdgeBasis::AstImportFileInventoryResolved,
                    raw_specifier: cand.raw_specifier,
                    resolved_repo_path: path,
                });
            }
            _ => report.unresolved.push(UnresolvedImport {
                candidate: cand,
                reason: UnresolvedReason::Ambiguous,
            }),
        }
    }
    report
}

/// IMPORTS-TSCONFIG-PATHS-1: the resolution of a NON-RELATIVE specifier against a partition's tsconfig
/// `paths`. PURE. `NotAnAlias` means no pattern matched (the caller then does the package classification).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AliasResolution {
    /// No `paths` pattern matched -> NOT an alias (caller falls through to package classification).
    NotAnAlias,
    /// Exactly one inventory FILE matched -> the resolved FILE key (-> a real FILE->FILE edge).
    Resolved(String),
    /// A pattern matched but NO inventory FILE resolved (extension/index) -> BLOCKS (honest).
    Unresolved,
    /// More than one DISTINCT FILE matched -> AMBIGUOUS; surfaced, NEVER silently picked -> BLOCKS.
    Ambiguous,
}

/// Match `specifier` against ONE tsconfig `paths` pattern; return the `*` capture if it matches. Patterns
/// have at most one `*` (TS semantics). An exact pattern (no `*`) matches the whole specifier (capture `""`).
fn match_alias_pattern(pattern: &str, specifier: &str) -> Option<String> {
    match pattern.split_once('*') {
        Some((prefix, suffix)) => {
            if specifier.len() >= prefix.len() + suffix.len()
                && specifier.starts_with(prefix)
                && specifier.ends_with(suffix)
            {
                Some(specifier[prefix.len()..specifier.len() - suffix.len()].to_string())
            } else {
                None
            }
        }
        None => (pattern == specifier).then(String::new),
    }
}

/// True if `specifier` matches ANY of the partition's tsconfig `paths` patterns (i.e. it IS an alias, even if
/// it does not resolve to a FILE). Lets a caller distinguish an UNRESOLVED alias (blocks as alias) from a
/// non-alias bare specifier (package classification) without re-running the full inventory resolution.
pub fn specifier_matches_any_alias(config: &TsconfigAliasConfig, specifier: &str) -> bool {
    config
        .paths
        .keys()
        .any(|pattern| match_alias_pattern(pattern, specifier).is_some())
}

/// Resolve a NON-RELATIVE `specifier` against the partition's tsconfig alias `config` + the global FILE
/// inventory. PURE. For each matching `paths` target: substitute the `*`, join against `baseUrl` (resolved
/// from the partition's repo-relative prefix), then try the SAME extension/index candidates as a relative
/// import. Collects DISTINCT FILE hits: 0 -> Unresolved, 1 -> Resolved, >1 -> Ambiguous (never picked). No
/// pattern match -> NotAnAlias. Reuses `candidate_paths` -- alias resolution differs from relative ONLY in how
/// the base path is formed (paths/baseUrl vs `dirname(source)`).
pub fn resolve_tsconfig_alias(
    specifier: &str,
    config: &TsconfigAliasConfig,
    inv: &FileInventory,
) -> AliasResolution {
    // baseUrl is relative to the tsconfig dir (= the partition root, repo-relative `partition_prefix`).
    let effective_base = normalize_join(&config.partition_prefix, &config.base_url);
    let mut matched_any = false;
    let mut hits: BTreeSet<String> = BTreeSet::new();
    for (pattern, targets) in &config.paths {
        let Some(capture) = match_alias_pattern(pattern, specifier) else {
            continue;
        };
        matched_any = true;
        for target in targets {
            let substituted = target.replacen('*', &capture, 1);
            let base = normalize_join(&effective_base, &substituted);
            for path in candidate_paths(&base) {
                if let Some(key) = inv.file_key_for(&path) {
                    hits.insert(key.to_string());
                }
            }
        }
    }
    if !matched_any {
        return AliasResolution::NotAnAlias;
    }
    match hits.len() {
        0 => AliasResolution::Unresolved,
        1 => AliasResolution::Resolved(hits.into_iter().next().unwrap()),
        _ => AliasResolution::Ambiguous,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn alias_config(
        prefix: &str,
        base_url: &str,
        pattern: &str,
        target: &str,
    ) -> TsconfigAliasConfig {
        TsconfigAliasConfig {
            base_url: base_url.to_string(),
            paths: [(pattern.to_string(), vec![target.to_string()])]
                .into_iter()
                .collect(),
            partition_prefix: prefix.to_string(),
        }
    }

    #[test]
    fn alias_wildcard_resolves_to_partition_source() {
        // admin: baseUrl=".", paths {"@/*":["./src/*"]}; @/lib/api -> admin/src/lib/api.ts.
        let inv = FileInventory::from_file_keys(["repo:admin/src/lib/api.ts:FILE".to_string()]);
        let cfg = alias_config("admin", ".", "@/*", "./src/*");
        assert_eq!(
            resolve_tsconfig_alias("@/lib/api", &cfg, &inv),
            AliasResolution::Resolved("repo:admin/src/lib/api.ts:FILE".to_string())
        );
    }

    #[test]
    fn alias_index_resolution() {
        let inv = FileInventory::from_file_keys(["repo:admin/src/lib/index.ts:FILE".to_string()]);
        let cfg = alias_config("admin", ".", "@/*", "./src/*");
        assert_eq!(
            resolve_tsconfig_alias("@/lib", &cfg, &inv),
            AliasResolution::Resolved("repo:admin/src/lib/index.ts:FILE".to_string())
        );
    }

    #[test]
    fn alias_no_pattern_match_is_not_an_alias() {
        let inv = FileInventory::from_file_keys(["repo:admin/src/lib/api.ts:FILE".to_string()]);
        let cfg = alias_config("admin", ".", "@/*", "./src/*");
        // react does not match @/* -> NotAnAlias (caller does package classification).
        assert_eq!(
            resolve_tsconfig_alias("react", &cfg, &inv),
            AliasResolution::NotAnAlias
        );
    }

    #[test]
    fn alias_matched_but_no_file_blocks() {
        let inv = FileInventory::from_file_keys(["repo:admin/src/other.ts:FILE".to_string()]);
        let cfg = alias_config("admin", ".", "@/*", "./src/*");
        assert_eq!(
            resolve_tsconfig_alias("@/lib/missing", &cfg, &inv),
            AliasResolution::Unresolved
        );
    }

    #[test]
    fn alias_ambiguous_when_multiple_targets_resolve() {
        // two targets both resolve to distinct files -> Ambiguous (never silently picked).
        let inv = FileInventory::from_file_keys([
            "repo:admin/src/lib/api.ts:FILE".to_string(),
            "repo:admin/generated/lib/api.ts:FILE".to_string(),
        ]);
        let cfg = TsconfigAliasConfig {
            base_url: ".".to_string(),
            paths: [(
                "@/*".to_string(),
                vec!["./src/*".to_string(), "./generated/*".to_string()],
            )]
            .into_iter()
            .collect(),
            partition_prefix: "admin".to_string(),
        };
        assert_eq!(
            resolve_tsconfig_alias("@/lib/api", &cfg, &inv),
            AliasResolution::Ambiguous
        );
    }

    #[test]
    fn package_name_extraction() {
        assert_eq!(package_name_of("react"), "react");
        assert_eq!(package_name_of("react/jsx-runtime"), "react");
        assert_eq!(package_name_of("@amodx/shared"), "@amodx/shared");
        assert_eq!(package_name_of("@tiptap/pm/model"), "@tiptap/pm");
        assert_eq!(package_name_of("@scope"), "@scope"); // malformed -> whole
    }

    #[test]
    fn workspace_local_takes_precedence_over_declared_dep_and_node_modules() {
        // @amodx/shared is workspace + declared + (symlinked) in node_modules -> STILL LOCAL, never external.
        let ws = set(&["@amodx/shared", "@amodx/effects"]);
        let deps = set(&["@amodx/shared", "react"]);
        assert_eq!(
            classify_package_import("@amodx/shared", &ws, &deps, false),
            PackageImportClass::WorkspaceLocalUnedgeable
        );
        // EXTERNAL-EVIDENCE-1 trust hinge: even with external_node_modules=true (a workspace symlink in
        // node_modules), the workspace map wins -> stays WorkspaceLocalUnedgeable, NOT benign.
        assert_eq!(
            classify_package_import("@amodx/shared", &ws, &deps, true),
            PackageImportClass::WorkspaceLocalUnedgeable
        );
    }

    #[test]
    fn declared_external_is_benign() {
        let ws = set(&["@amodx/shared"]);
        let deps = set(&["react", "@tiptap/react"]);
        assert_eq!(
            classify_package_import("react", &ws, &deps, false),
            PackageImportClass::ExternalPackageNonLocal
        );
        assert_eq!(
            classify_package_import("@tiptap/react/menus", &ws, &deps, false),
            PackageImportClass::ExternalPackageNonLocal
        );
    }

    #[test]
    fn node_modules_external_is_benign_even_when_undeclared() {
        // EXTERNAL-EVIDENCE-1: a transitively-pulled / type-only package (NOT directly declared) that resolves
        // to a real node_modules/@types install -> benign.
        let ws = set(&["@amodx/shared"]);
        let deps = set(&["@tiptap/react"]); // @tiptap/core is transitive, NOT declared
        assert_eq!(
            classify_package_import("@tiptap/core", &ws, &deps, true),
            PackageImportClass::ExternalPackageNonLocal
        );
        // without the node_modules evidence -> blocks (unknown).
        assert_eq!(
            classify_package_import("@tiptap/core", &ws, &deps, false),
            PackageImportClass::PackageUnresolved
        );
    }

    #[test]
    fn node_builtins_are_external_with_or_without_prefix() {
        let empty = BTreeSet::new();
        assert_eq!(
            classify_package_import("node:fs", &empty, &empty, false),
            PackageImportClass::ExternalPackageNonLocal
        );
        assert_eq!(
            classify_package_import("path", &empty, &empty, false),
            PackageImportClass::ExternalPackageNonLocal
        );
    }

    #[test]
    fn unknown_bare_specifier_blocks() {
        // NEITHER workspace, declared, nor in node_modules -> PackageUnresolved (blocks); NEVER inferred
        // external from absence in the workspace map (the trust hinge).
        let ws = set(&["@amodx/shared"]);
        let deps = set(&["react"]);
        assert_eq!(
            classify_package_import("@/lib/api", &ws, &deps, false),
            PackageImportClass::PackageUnresolved
        );
        assert_eq!(
            classify_package_import("some-undeclared-pkg", &ws, &deps, false),
            PackageImportClass::PackageUnresolved
        );
    }

    fn inventory() -> FileInventory {
        FileInventory::from_file_keys(
            [
                "repo:packages/a/src/main.ts:FILE",
                "repo:packages/b/src/foo.ts:FILE", // cross-partition target
                "repo:packages/a/src/bar/index.ts:FILE", // index target
                "repo:packages/a/src/widget.tsx:FILE", // .tsx target
                "repo:packages/a/src/dup.ts:FILE", // ambiguity pair
                "repo:packages/a/src/dup.tsx:FILE",
            ]
            .into_iter()
            .map(String::from),
        )
    }

    fn cand(spec: &str) -> ImportCandidate {
        ImportCandidate {
            source_file_key: "repo:packages/a/src/main.ts:FILE".to_string(),
            raw_specifier: spec.to_string(),
        }
    }

    #[test]
    fn cross_partition_relative_resolves_to_ts() {
        let r = resolve_imports(&inventory(), vec![cand("../../b/src/foo")]);
        assert_eq!(r.resolved.len(), 1);
        assert_eq!(r.unresolved.len(), 0);
        let e = &r.resolved[0];
        assert_eq!(e.src_file_key, "repo:packages/a/src/main.ts:FILE");
        assert_eq!(e.dst_file_key, "repo:packages/b/src/foo.ts:FILE");
        assert_eq!(e.resolved_repo_path, "packages/b/src/foo.ts");
        assert_eq!(e.basis, EdgeBasis::AstImportFileInventoryResolved);
    }

    #[test]
    fn relative_resolves_to_index() {
        let r = resolve_imports(&inventory(), vec![cand("./bar")]);
        assert_eq!(r.resolved.len(), 1);
        assert_eq!(
            r.resolved[0].dst_file_key,
            "repo:packages/a/src/bar/index.ts:FILE"
        );
    }

    #[test]
    fn relative_resolves_to_tsx() {
        let r = resolve_imports(&inventory(), vec![cand("./widget")]);
        assert_eq!(r.resolved.len(), 1);
        assert_eq!(
            r.resolved[0].dst_file_key,
            "repo:packages/a/src/widget.tsx:FILE"
        );
    }

    #[test]
    fn unresolved_relative_stays_unresolved() {
        let r = resolve_imports(&inventory(), vec![cand("./missing")]);
        assert_eq!(r.resolved.len(), 0);
        assert_eq!(r.unresolved.len(), 1);
        assert_eq!(r.unresolved[0].reason, UnresolvedReason::NotFound);
    }

    #[test]
    fn non_relative_is_package_external() {
        let r = resolve_imports(&inventory(), vec![cand("react")]);
        assert_eq!(r.resolved.len(), 0);
        assert_eq!(r.unresolved[0].reason, UnresolvedReason::PackageExternal);
    }

    #[test]
    fn js_substitution_resolves_to_ts_source() {
        // IMPORTS-RELATIVE-RESOLUTION-COMPLETE-1: `./x.js` (ESM output) -> the `.ts` SOURCE.
        let r = resolve_imports(&inventory(), vec![cand("../../b/src/foo.js")]);
        assert_eq!(r.resolved.len(), 1, "unresolved: {:?}", r.unresolved);
        assert_eq!(
            r.resolved[0].dst_file_key,
            "repo:packages/b/src/foo.ts:FILE"
        );
    }

    #[test]
    fn js_substitution_resolves_to_tsx_source() {
        // `./widget.js` -> widget.tsx (no widget.ts present).
        let r = resolve_imports(&inventory(), vec![cand("./widget.js")]);
        assert_eq!(r.resolved.len(), 1, "unresolved: {:?}", r.unresolved);
        assert_eq!(
            r.resolved[0].dst_file_key,
            "repo:packages/a/src/widget.tsx:FILE"
        );
    }

    #[test]
    fn js_substitution_ambiguous_when_both_ts_and_tsx() {
        // `./dup.js` with BOTH dup.ts AND dup.tsx -> Ambiguous (no silent .ts-over-.tsx preference).
        let r = resolve_imports(&inventory(), vec![cand("./dup.js")]);
        assert_eq!(r.resolved.len(), 0);
        assert_eq!(r.unresolved[0].reason, UnresolvedReason::Ambiguous);
    }

    #[test]
    fn jsx_mjs_cjs_substitution_families() {
        let inv = FileInventory::from_file_keys(
            [
                "repo:p/comp.tsx:FILE", // .jsx -> .tsx
                "repo:p/mod.mts:FILE",  // .mjs -> .mts
                "repo:p/cfg.cts:FILE",  // .cjs -> .cts
            ]
            .into_iter()
            .map(String::from),
        );
        let c = |spec: &str| ImportCandidate {
            source_file_key: "repo:p/main.ts:FILE".to_string(),
            raw_specifier: spec.to_string(),
        };
        let dst = |spec: &str| {
            resolve_imports(&inv, vec![c(spec)])
                .resolved
                .first()
                .map(|e| e.dst_file_key.clone())
        };
        assert_eq!(dst("./comp.jsx").as_deref(), Some("repo:p/comp.tsx:FILE"));
        assert_eq!(dst("./mod.mjs").as_deref(), Some("repo:p/mod.mts:FILE"));
        assert_eq!(dst("./cfg.cjs").as_deref(), Some("repo:p/cfg.cts:FILE"));
    }

    #[test]
    fn js_substitution_no_source_stays_unresolved() {
        // `./nope.js` with no `nope.ts`/`nope.tsx` -> still unresolved (blocks).
        let r = resolve_imports(&inventory(), vec![cand("./nope.js")]);
        assert_eq!(r.resolved.len(), 0);
        assert_eq!(r.unresolved[0].reason, UnresolvedReason::NotFound);
    }

    #[test]
    fn literal_source_extension_exact_match() {
        // IMPORTS-ASSET-AND-LITERAL-EXT-1: `./widget.tsx` -> the EXACT widget.tsx FILE node (not subject to
        // ambiguity with appended candidates).
        let r = resolve_imports(&inventory(), vec![cand("./widget.tsx")]);
        assert_eq!(r.resolved.len(), 1, "unresolved: {:?}", r.unresolved);
        assert_eq!(
            r.resolved[0].dst_file_key,
            "repo:packages/a/src/widget.tsx:FILE"
        );
    }

    #[test]
    fn literal_source_extension_no_node_falls_through() {
        // `./nope.tsx` with no `nope.tsx` node -> falls through -> candidate_paths -> NotFound.
        let r = resolve_imports(&inventory(), vec![cand("./nope.tsx")]);
        assert_eq!(r.resolved.len(), 0);
        assert_eq!(r.unresolved[0].reason, UnresolvedReason::NotFound);
    }

    #[test]
    fn asset_specifier_allowlist() {
        assert!(is_asset_specifier("./globals.css"));
        assert!(is_asset_specifier("../assets/logo.svg"));
        assert!(is_asset_specifier("./fonts/x.woff2"));
        assert!(!is_asset_specifier("./App.tsx")); // a SOURCE file, not an asset
        assert!(!is_asset_specifier("./lib/db")); // extensionless
        assert!(!is_asset_specifier("./data.json")); // .json is DATA, NOT in the allowlist
        assert!(!is_asset_specifier("./x.weird")); // unknown extension -> never benign
    }

    #[test]
    fn multiple_candidates_are_ambiguous_not_silently_picked() {
        // Both dup.ts and dup.tsx exist -> Ambiguous (never a silent extension-priority pick).
        let r = resolve_imports(&inventory(), vec![cand("./dup")]);
        assert_eq!(r.resolved.len(), 0);
        assert_eq!(r.unresolved[0].reason, UnresolvedReason::Ambiguous);
    }

    #[test]
    fn bad_source_key_reported() {
        let r = resolve_imports(
            &inventory(),
            vec![ImportCandidate {
                source_file_key: "not-a-file-key".to_string(),
                raw_specifier: "./x".to_string(),
            }],
        );
        assert_eq!(r.unresolved[0].reason, UnresolvedReason::BadSourceKey);
    }
}
