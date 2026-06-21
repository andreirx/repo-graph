//! XPART-PROVE-1B export-surface reconciliation (research / probe — NOT production).
//!
//! Reconciles a consumer-side *published* declaration SCIP symbol
//! (`@fraktag/engine 0.1.0 dist/`index.d.ts`/Fraktag#`) to the provider-side *source* SCIP
//! identity (`… src/index.ts/Fraktag#`) so the cross-partition xref stops depending on raw SCIP
//! symbol equality. See `docs/slices/xpart-prove-1b.md`.
//!
//! Ratified scope (D1/D2/D3): provider-source anchor → `CanonicalKey`; bases
//! `DeclarationMapExact` + strict `NameExactUnique` + explicit `Ambiguous`/`Unresolved`; alias
//! carries provenance, never a silent rewrite. **No VLQ / source-map token mapping** — deferred
//! hardening for descriptor-divergent declarations (FRAKTAG's measured descriptors are identical
//! dist↔src, so only the file component differs; token decode would be unexercised here).

use scip::symbol::{format_symbol, parse_symbol};
use scip::types::{descriptor, Descriptor, Index, Symbol};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

/// Reconciliation basis — the ratified closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basis {
    /// `.d.ts.map` `sources[]` file-correspondence + descriptor-exact reconstruction, confirmed
    /// to exist uniquely among provider defs. The strongest basis.
    DeclarationMapExact,
    /// Exactly one provider def shares the full code-descriptor sequence and package version.
    /// Strict fallback when the declaration-map reconstruction does not resolve.
    NameExactUnique,
    /// Multiple candidates on either side — NOT attached.
    Ambiguous,
    /// No map / descriptor absent among provider defs — NOT attached.
    Unresolved,
}

impl Basis {
    /// Stable label for output.
    pub fn label(self) -> &'static str {
        match self {
            Basis::DeclarationMapExact => "DeclarationMapExact",
            Basis::NameExactUnique => "NameExactUnique",
            Basis::Ambiguous => "Ambiguous",
            Basis::Unresolved => "Unresolved",
        }
    }

    /// Whether this basis attaches an alias. Only the two confirmed bases attach; `Ambiguous`
    /// and `Unresolved` never do (strict default: uncertainty stays unattached).
    pub fn attaches(self) -> bool {
        matches!(self, Basis::DeclarationMapExact | Basis::NameExactUnique)
    }
}

/// One reconciliation outcome with full provenance (D3: never a bare published→canonical map).
#[derive(Debug, Clone)]
pub struct AliasRecord {
    /// The consumer-side published declaration symbol.
    pub published_symbol: String,
    /// The reconstructed provider source symbol (only when attached).
    pub provider_source_symbol: Option<String>,
    /// The repo-graph `CanonicalKey` the provider source symbol resolves to (D1), when known.
    pub canonical_key: Option<String>,
    /// Which basis fired.
    pub basis: Basis,
    /// Package name (provenance).
    pub package_name: String,
    /// Package version (provenance).
    pub package_version: String,
    /// Declaration file the published symbol lives in (e.g. `dist/index.d.ts`).
    pub declaration_file: Option<String>,
    /// The `.d.ts.map` consulted, if any.
    pub declaration_map: Option<String>,
    /// The source file resolved from the declaration map's `sources[]`.
    pub source_file: Option<String>,
    /// Human-readable reason (machine-checkable class is `basis`).
    pub reason: String,
}

/// The always-resident export alias index: every reconciliation outcome + the attach-only map.
pub struct ExportAliasIndex {
    /// One record per unique published symbol (every input classified — no silent miss).
    pub records: Vec<AliasRecord>,
    /// published_symbol → provider_source_symbol, attached records only.
    alias_map: HashMap<String, String>,
    /// published_symbol → basis, for occurrence-level tallying.
    basis_by_symbol: HashMap<String, Basis>,
}

impl ExportAliasIndex {
    /// The attach-only alias map used to rewrite consumer references to provider identity.
    pub fn alias_map(&self) -> &HashMap<String, String> {
        &self.alias_map
    }

    /// Basis for a published symbol (None if it was not an input — should not happen for
    /// symbols drawn from the same reference set).
    pub fn basis_of(&self, published_symbol: &str) -> Option<Basis> {
        self.basis_by_symbol.get(published_symbol).copied()
    }
}

/// Provider (engine) definition index: existence set + code-sequence map + canonical resolver.
pub struct EngineDefIndex {
    /// Normalized full provider def symbols (for `DeclarationMapExact` existence).
    full: HashSet<String>,
    /// Normalized code-descriptor sequence → provider def symbols (for `NameExactUnique`).
    by_code: HashMap<String, BTreeSet<String>>,
    /// Normalized provider source symbol → repo-graph `CanonicalKey` (D1).
    canonical: HashMap<String, String>,
}

impl EngineDefIndex {
    /// Build from the provider SCIP index (definitions only) plus the
    /// `scip_symbol_id → CanonicalKey` map taken from the provider IR node provenance.
    pub fn build(engine_index: &Index, symbol_to_canonical: &HashMap<String, String>) -> Self {
        let mut full = HashSet::new();
        let mut by_code: HashMap<String, BTreeSet<String>> = HashMap::new();
        for doc in &engine_index.documents {
            for occ in &doc.occurrences {
                // Definitions only (role bit 0x1); skip locals.
                if occ.symbol_roles & 0x1 == 0 || scip::symbol::is_local_symbol(&occ.symbol) {
                    continue;
                }
                if let Ok(sym) = parse_symbol(&occ.symbol) {
                    let norm = format_symbol(sym.clone());
                    full.insert(norm.clone());
                    let (_file, code) = split_file_and_code(&sym);
                    if !code.is_empty() {
                        by_code.entry(code_key(&code)).or_default().insert(norm);
                    }
                }
            }
        }
        let canonical = symbol_to_canonical
            .iter()
            .filter_map(|(s, k)| parse_symbol(s).ok().map(|p| (format_symbol(p), k.clone())))
            .collect();
        Self {
            full,
            by_code,
            canonical,
        }
    }

    fn contains(&self, normalized_symbol: &str) -> bool {
        self.full.contains(normalized_symbol)
    }

    fn canonical_of(&self, normalized_symbol: &str) -> Option<String> {
        self.canonical.get(normalized_symbol).cloned()
    }
}

/// Reconcile each unique published symbol against the provider defs, in strict basis order.
/// `engine_root` is the provider package root that holds `dist/**/*.d.ts.map`.
pub fn reconcile(
    published_symbols: &[String],
    engine: &EngineDefIndex,
    engine_root: &str,
) -> ExportAliasIndex {
    let mut records = Vec::new();
    let mut alias_map = HashMap::new();
    let mut basis_by_symbol = HashMap::new();

    for published in published_symbols {
        let record = reconcile_one(published, engine, engine_root);
        basis_by_symbol.insert(published.clone(), record.basis);
        if record.basis.attaches() {
            if let Some(src) = &record.provider_source_symbol {
                alias_map.insert(published.clone(), src.clone());
            }
        }
        records.push(record);
    }

    ExportAliasIndex {
        records,
        alias_map,
        basis_by_symbol,
    }
}

fn reconcile_one(published: &str, engine: &EngineDefIndex, engine_root: &str) -> AliasRecord {
    let parsed = match parse_symbol(published) {
        Ok(p) => p,
        Err(_) => return unresolved(published, "", "", None, "unparseable published symbol"),
    };
    let (pkg_name, pkg_version) = package_of(&parsed);
    let (file, code) = split_file_and_code(&parsed);

    // ── Basis 1: DeclarationMapExact ───────────────────────────────
    if !file.is_empty() {
        let decl_file = path_join(&file);
        if let Some((map_path, sources)) = read_decl_map_sources(engine_root, &decl_file) {
            let mut hits: BTreeSet<String> = BTreeSet::new();
            let mut hit_source: Option<String> = None;
            for source in &sources {
                let candidate = reconstruct(&parsed, source, &code);
                if engine.contains(&candidate) {
                    hits.insert(candidate.clone());
                    hit_source = Some(source.clone());
                }
            }
            // Attach only if the provider source symbol exists UNIQUELY (acceptance point 2).
            if hits.len() == 1 {
                let src = hits.into_iter().next().unwrap();
                let canonical = engine.canonical_of(&src);
                return AliasRecord {
                    published_symbol: published.to_string(),
                    provider_source_symbol: Some(src),
                    canonical_key: canonical,
                    basis: Basis::DeclarationMapExact,
                    package_name: pkg_name,
                    package_version: pkg_version,
                    declaration_file: Some(decl_file),
                    declaration_map: Some(map_path),
                    source_file: hit_source,
                    reason: "declaration-map sources[] + descriptor-exact, unique provider def"
                        .to_string(),
                };
            } else if hits.len() > 1 {
                return AliasRecord {
                    published_symbol: published.to_string(),
                    provider_source_symbol: None,
                    canonical_key: None,
                    basis: Basis::Ambiguous,
                    package_name: pkg_name,
                    package_version: pkg_version,
                    declaration_file: Some(decl_file),
                    declaration_map: Some(map_path),
                    source_file: None,
                    reason: format!(
                        "{} declaration-map sources resolve to distinct provider defs",
                        hits.len()
                    ),
                };
            }
            // map present but no descriptor-exact hit → fall through to Basis 3.
        }
    }

    // ── Basis 3: NameExactUnique (strict) ──────────────────────────
    if !code.is_empty() {
        let key = code_key(&code);
        if let Some(candidates) = engine.by_code.get(&key) {
            // Same package version is part of the strict predicate.
            let version_matched: Vec<&String> = candidates
                .iter()
                .filter(|c| {
                    parse_symbol(c)
                        .ok()
                        .map(|p| package_of(&p).1 == pkg_version)
                        .unwrap_or(false)
                })
                .collect();
            if version_matched.len() == 1 {
                let src = version_matched[0].clone();
                let canonical = engine.canonical_of(&src);
                return AliasRecord {
                    published_symbol: published.to_string(),
                    provider_source_symbol: Some(src),
                    canonical_key: canonical,
                    basis: Basis::NameExactUnique,
                    package_name: pkg_name,
                    package_version: pkg_version,
                    declaration_file: (!file.is_empty()).then(|| path_join(&file)),
                    declaration_map: None,
                    source_file: None,
                    reason: "unique provider def with identical code-descriptor sequence + version"
                        .to_string(),
                };
            } else if version_matched.len() > 1 {
                return AliasRecord {
                    published_symbol: published.to_string(),
                    provider_source_symbol: None,
                    canonical_key: None,
                    basis: Basis::Ambiguous,
                    package_name: pkg_name,
                    package_version: pkg_version,
                    declaration_file: None,
                    declaration_map: None,
                    source_file: None,
                    reason: format!("{} provider defs share the code-descriptor sequence (overload/path ambiguity)", version_matched.len()),
                };
            }
        }
    }

    // ── Basis 4: Unresolved ────────────────────────────────────────
    unresolved(
        published,
        &pkg_name,
        &pkg_version,
        (!file.is_empty()).then(|| path_join(&file)),
        "no declaration-map descriptor-exact hit and no unique code-sequence provider def",
    )
}

fn unresolved(
    published: &str,
    pkg_name: &str,
    pkg_version: &str,
    decl_file: Option<String>,
    reason: &str,
) -> AliasRecord {
    AliasRecord {
        published_symbol: published.to_string(),
        provider_source_symbol: None,
        canonical_key: None,
        basis: Basis::Unresolved,
        package_name: pkg_name.to_string(),
        package_version: pkg_version.to_string(),
        declaration_file: decl_file,
        declaration_map: None,
        source_file: None,
        reason: reason.to_string(),
    }
}

// ── Symbol structure helpers ───────────────────────────────────────

fn package_of(sym: &Symbol) -> (String, String) {
    match sym.package.as_ref() {
        Some(p) => (p.name.clone(), p.version.clone()),
        None => (String::new(), String::new()),
    }
}

fn is_path_suffix(d: &Descriptor) -> bool {
    matches!(
        d.suffix.enum_value(),
        Ok(descriptor::Suffix::Package) | Ok(descriptor::Suffix::Namespace)
    )
}

/// TS/JS source + declaration file extensions. `.d.ts` is covered by `.ts`.
fn has_source_ext(name: &str) -> bool {
    const EXTS: &[&str] = &[".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs"];
    EXTS.iter().any(|e| name.ends_with(e))
}

/// Split a parsed symbol into (file-path descriptors, code descriptors). The file path is the
/// leading run of path descriptors up to and including the one bearing a source extension; the
/// code descriptors are everything after. Returns (`[]`, all) when no file component is present.
fn split_file_and_code(sym: &Symbol) -> (Vec<Descriptor>, Vec<Descriptor>) {
    let descs = &sym.descriptors;
    for (i, d) in descs.iter().enumerate() {
        if is_path_suffix(d) && has_source_ext(&d.name) {
            // All descriptors up to the filename must be path components.
            if descs[..=i].iter().all(is_path_suffix) {
                return (descs[..=i].to_vec(), descs[i + 1..].to_vec());
            }
        }
    }
    (Vec::new(), descs.clone())
}

/// Join file-path descriptor names with `/` → package-relative path (`dist/index.d.ts`).
fn path_join(file: &[Descriptor]) -> String {
    file.iter()
        .map(|d| d.name.clone())
        .collect::<Vec<_>>()
        .join("/")
}

/// Reconstruct the candidate provider source symbol: replace the file-path descriptors with the
/// source path (from the declaration map), keep the code descriptors identical. Encoded through
/// the scip crate formatter (same encoder used to normalize provider defs).
fn reconstruct(orig: &Symbol, source_pkg_rel: &str, code: &[Descriptor]) -> String {
    let mut descriptors: Vec<Descriptor> = source_pkg_rel
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|comp| Descriptor {
            name: comp.to_string(),
            disambiguator: String::new(),
            suffix: descriptor::Suffix::Package.into(),
            ..Default::default()
        })
        .collect();
    descriptors.extend(code.iter().cloned());
    let sym = Symbol {
        scheme: orig.scheme.clone(),
        package: orig.package.clone(),
        descriptors,
        ..Default::default()
    };
    format_symbol(sym)
}

/// Local re-implementation of the scip crate's (private) name escaping, used only to build the
/// internal code-sequence key. Identical rule: bare identifier chars stay, else backtick-escape.
fn escape_name(name: &str) -> String {
    if name
        .chars()
        .all(|ch| ch == '_' || ch == '+' || ch == '-' || ch == '$' || ch.is_ascii_alphanumeric())
    {
        name.to_string()
    } else {
        format!("`{}`", name.replace('`', "``"))
    }
}

fn format_code_descriptor(d: &Descriptor) -> String {
    let name = escape_name(&d.name);
    match d.suffix.enum_value() {
        Ok(descriptor::Suffix::Type) => format!("{name}#"),
        Ok(descriptor::Suffix::Term) => format!("{name}."),
        Ok(descriptor::Suffix::Method) => format!("{name}({}).", d.disambiguator),
        Ok(descriptor::Suffix::TypeParameter) => format!("[{name}]"),
        Ok(descriptor::Suffix::Parameter) => format!("({name})"),
        Ok(descriptor::Suffix::Macro) => format!("{name}!"),
        Ok(descriptor::Suffix::Meta) => format!("{name}:"),
        Ok(descriptor::Suffix::Package) | Ok(descriptor::Suffix::Namespace) => format!("{name}/"),
        _ => name,
    }
}

/// A canonical string for a code-descriptor sequence (the in-file symbol path), used to match a
/// published symbol's code sequence against provider defs regardless of file component.
fn code_key(code: &[Descriptor]) -> String {
    code.iter().map(format_code_descriptor).collect()
}

// ── Declaration-map reading (serde_json; no VLQ decode) ─────────────

/// Read `<engine_root>/<decl_file>.map` and resolve its `sources[]` to package-relative source
/// paths. Honors an optional `sourceRoot`. Returns (map_path_string, [pkg_rel_source]).
fn read_decl_map_sources(engine_root: &str, decl_file: &str) -> Option<(String, Vec<String>)> {
    let root = normalize(Path::new(engine_root));
    let map_path = root.join(format!("{decl_file}.map"));
    let bytes = std::fs::read(&map_path).ok()?;
    let json: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let sources = json.get("sources")?.as_array()?;
    let source_root = json
        .get("sourceRoot")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let map_dir = map_path.parent()?.to_path_buf();

    let mut out = Vec::new();
    for s in sources {
        let s = s.as_str()?;
        let joined = if source_root.is_empty() {
            map_dir.join(s)
        } else {
            map_dir.join(source_root).join(s)
        };
        let abs = normalize(&joined);
        if let Ok(rel) = abs.strip_prefix(&root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    if out.is_empty() {
        return None;
    }
    Some((map_path.to_string_lossy().to_string(), out))
}

/// Lexical path normalization (fold `.`/`..`) without touching the filesystem — avoids symlink
/// resolution that could diverge from the indexer's package-relative paths.
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIST: &str = "scip-typescript npm @fraktag/engine 0.1.0 dist/`index.d.ts`/Fraktag#";
    const DIST_METHOD: &str =
        "scip-typescript npm @fraktag/engine 0.1.0 dist/`index.d.ts`/Fraktag#listKnowledgeBases().";

    #[test]
    fn detects_source_extensions() {
        assert!(has_source_ext("index.d.ts"));
        assert!(has_source_ext("index.ts"));
        assert!(has_source_ext("types.d.ts"));
        assert!(!has_source_ext("Fraktag"));
        assert!(!has_source_ext("dist"));
    }

    #[test]
    fn splits_file_and_code() {
        let sym = parse_symbol(DIST).unwrap();
        let (file, code) = split_file_and_code(&sym);
        assert_eq!(path_join(&file), "dist/index.d.ts");
        assert_eq!(code.len(), 1);
        assert_eq!(code_key(&code), "Fraktag#");
    }

    #[test]
    fn splits_method_code_sequence() {
        let sym = parse_symbol(DIST_METHOD).unwrap();
        let (file, code) = split_file_and_code(&sym);
        assert_eq!(path_join(&file), "dist/index.d.ts");
        assert_eq!(code_key(&code), "Fraktag#listKnowledgeBases().");
    }

    #[test]
    fn reconstructs_source_symbol_descriptor_exact() {
        let sym = parse_symbol(DIST).unwrap();
        let (_file, code) = split_file_and_code(&sym);
        let rebuilt = reconstruct(&sym, "src/index.ts", &code);
        assert_eq!(
            rebuilt,
            "scip-typescript npm @fraktag/engine 0.1.0 src/`index.ts`/Fraktag#"
        );
    }

    #[test]
    fn reconstructs_method_preserves_descriptor() {
        let sym = parse_symbol(DIST_METHOD).unwrap();
        let (_file, code) = split_file_and_code(&sym);
        let rebuilt = reconstruct(&sym, "src/index.ts", &code);
        assert_eq!(
            rebuilt,
            "scip-typescript npm @fraktag/engine 0.1.0 src/`index.ts`/Fraktag#listKnowledgeBases()."
        );
    }

    #[test]
    fn module_symbol_has_no_code() {
        // A bare module reference: file path, no code descriptor.
        let sym =
            parse_symbol("scip-typescript npm @fraktag/engine 0.1.0 dist/`index.d.ts`/").unwrap();
        let (file, code) = split_file_and_code(&sym);
        assert_eq!(path_join(&file), "dist/index.d.ts");
        assert!(code.is_empty());
    }

    #[test]
    fn unresolved_when_no_engine_def() {
        // Empty provider index → every published symbol is Unresolved, never silently dropped.
        let engine = EngineDefIndex {
            full: HashSet::new(),
            by_code: HashMap::new(),
            canonical: HashMap::new(),
        };
        let idx = reconcile(&[DIST.to_string()], &engine, "/nonexistent");
        assert_eq!(idx.records.len(), 1);
        assert_eq!(idx.basis_of(DIST), Some(Basis::Unresolved));
        assert!(idx.alias_map().is_empty());
    }

    #[test]
    fn name_exact_unique_attaches_without_map() {
        // Provider def exists with the same code sequence; no declaration map on disk.
        let src = "scip-typescript npm @fraktag/engine 0.1.0 src/`index.ts`/Fraktag#";
        let mut by_code: HashMap<String, BTreeSet<String>> = HashMap::new();
        by_code.insert("Fraktag#".to_string(), {
            let mut s = BTreeSet::new();
            s.insert(src.to_string());
            s
        });
        let mut full = HashSet::new();
        full.insert(src.to_string());
        let engine = EngineDefIndex {
            full,
            by_code,
            canonical: HashMap::new(),
        };
        let idx = reconcile(&[DIST.to_string()], &engine, "/nonexistent");
        assert_eq!(idx.basis_of(DIST), Some(Basis::NameExactUnique));
        assert_eq!(idx.alias_map().get(DIST).map(String::as_str), Some(src));
    }
}
