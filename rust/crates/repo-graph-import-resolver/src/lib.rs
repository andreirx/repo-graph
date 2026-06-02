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

use std::collections::HashMap;

use repo_graph_ir::EdgeBasis;

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

    /// The FILE key for a repo-relative path, if present.
    fn get(&self, path: &str) -> Option<&str> {
        self.by_path.get(path).map(String::as_str)
    }
}

/// Extract the repo-relative path from a FILE node key `{repo}:{path}:FILE` (path is everything after the
/// first `:` and before the `:FILE` suffix). `None` if the key is not a `:FILE` key.
fn file_key_path(key: &str) -> Option<&str> {
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
/// directory pops the parent — so an import from `packages/a/src` of `../../b/x` -> `packages/b/x`).
fn normalize_join(dir: &str, spec: &str) -> String {
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

/// Directory of a repo-relative file path (everything before the last `/`; empty if none).
fn dirname(path: &str) -> &str {
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
            if let Some(key) = inv.get(&path) {
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
