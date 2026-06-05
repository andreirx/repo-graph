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

use repo_graph_ir::EdgeBasis;

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

/// Classify a NON-RELATIVE import specifier (IMPORTS-PACKAGE-RESOLUTION-1 D1, ratified A). PURE -- given the
/// loaded `workspace_packages` (package.json `name`s) + the source partition's `declared_dependencies`.
/// Precedence: `node:`/builtin -> external; workspace map -> WorkspaceLocalUnedgeable (a workspace package is
/// LOCAL even when also a declared dep, via the workspace protocol); declared dep -> external; else ->
/// PackageUnresolved. CONSERVATIVE: external requires POSITIVE evidence; an unknown bare specifier BLOCKS.
pub fn classify_package_import(
    specifier: &str,
    workspace_packages: &BTreeSet<String>,
    declared_dependencies: &BTreeSet<String>,
) -> PackageImportClass {
    if specifier.starts_with("node:") {
        return PackageImportClass::ExternalPackageNonLocal;
    }
    let pkg = package_name_of(specifier);
    if NODE_BUILTINS.contains(&pkg.as_str()) {
        return PackageImportClass::ExternalPackageNonLocal;
    }
    if workspace_packages.contains(&pkg) {
        return PackageImportClass::WorkspaceLocalUnedgeable;
    }
    if declared_dependencies.contains(&pkg) {
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

/// The DETERMINISTIC candidate paths tried for a normalized target base `T` (extensions THEN index). The
/// resolver collects ALL inventory matches across this set; >1 match -> `Ambiguous`.
fn candidate_paths(base: &str) -> [String; 7] {
    [
        format!("{base}.ts"),
        format!("{base}.tsx"),
        format!("{base}.d.ts"),
        format!("{base}.mts"),
        format!("{base}.cts"),
        format!("{base}/index.ts"),
        format!("{base}/index.tsx"),
    ]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
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
    fn workspace_local_takes_precedence_over_declared_dep() {
        // @amodx/shared is BOTH a workspace package AND declared (workspace protocol) -> LOCAL, not external.
        let ws = set(&["@amodx/shared", "@amodx/effects"]);
        let deps = set(&["@amodx/shared", "react"]);
        assert_eq!(
            classify_package_import("@amodx/shared", &ws, &deps),
            PackageImportClass::WorkspaceLocalUnedgeable
        );
    }

    #[test]
    fn declared_external_is_benign() {
        let ws = set(&["@amodx/shared"]);
        let deps = set(&["react", "@tiptap/react"]);
        assert_eq!(
            classify_package_import("react", &ws, &deps),
            PackageImportClass::ExternalPackageNonLocal
        );
        assert_eq!(
            classify_package_import("@tiptap/react/menus", &ws, &deps),
            PackageImportClass::ExternalPackageNonLocal
        );
    }

    #[test]
    fn node_builtins_are_external_with_or_without_prefix() {
        let empty = BTreeSet::new();
        assert_eq!(
            classify_package_import("node:fs", &empty, &empty),
            PackageImportClass::ExternalPackageNonLocal
        );
        assert_eq!(
            classify_package_import("path", &empty, &empty),
            PackageImportClass::ExternalPackageNonLocal
        );
    }

    #[test]
    fn unknown_bare_specifier_blocks() {
        // a tsconfig path alias (@/lib) is NEITHER workspace nor declared -> PackageUnresolved (blocks);
        // NEVER inferred external from absence in the workspace map (the trust hinge).
        let ws = set(&["@amodx/shared"]);
        let deps = set(&["react"]);
        assert_eq!(
            classify_package_import("@/lib/api", &ws, &deps),
            PackageImportClass::PackageUnresolved
        );
        assert_eq!(
            classify_package_import("some-undeclared-pkg", &ws, &deps),
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
