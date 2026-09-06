//! Edge resolution policy — pure deterministic resolution of
//! symbolic edge targets to concrete node UIDs.
//!
//! Mirror of the resolution logic in
//! `src/adapters/indexer/repo-indexer.ts` (lines 2087–2366).
//!
//! All functions are PURE. No I/O, no storage access. The caller
//! builds the `ResolverIndex` from data fetched through the
//! storage port and passes it in.
//!
//! Resolution strategies by edge type:
//!   - IMPORTS: stable-key lookup → file-resolution map → C/C++
//!     per-TU includes → repo-prefix fallback
//!   - CALLS: dotted-name method extraction → name lookup →
//!     import-binding-assisted file narrowing
//!   - INSTANTIATES: name lookup filtered to CLASS subtype
//!   - IMPLEMENTS: name lookup filtered to INTERFACE subtype
//!   - Other: unfiltered name lookup

use std::collections::HashMap;

use repo_graph_classification::canonicalize_cargo_package_name;
use repo_graph_classification::types::{
    ImportBinding, ImportKind, SourceLocation, UnresolvedEdgeCategory,
};

use crate::include_resolver::{IncludeResolutionMap, ResolutionStatus};
use crate::storage_port::TypeOnlyDisposition;
use crate::types::{EdgeType, ExtractedEdge, Resolution};

/// Provenance prefix of the Rust extractor (`ExtractedEdge.extractor`), whose
/// value is `rust-core:<version>` (see `rust-extractor::EXTRACTOR_NAME`). The
/// declared-Rust-crate import stage is gated to edges carrying this prefix so a
/// non-Rust IMPORTS edge cannot resolve to a `.rs` file. Prefix (not the exact
/// version string) so the gate survives extractor version bumps. The indexer does
/// not depend on the rust-extractor crate (no dependency edge); the string is the
/// stable cross-boundary contract already present on every extracted edge.
const RUST_EXTRACTOR_PREFIX: &str = "rust-core:";

// ── Resolution outcome ───────────────────────────────────────────

/// Result of attempting to resolve an edge target.
enum TargetResolution {
    /// Target resolved to a node UID.
    Resolved(String),
    /// Target could not be resolved.
    Unresolved,
    /// Multiple candidates matched exactly (C/C++ include ambiguity).
    Ambiguous(Vec<String>),
}

// ── Resolver types ───────────────────────────────────────────────

/// Slim node for resolution and affinity filtering. Mirrors
/// `ResolverNode` from `src/core/ports/storage.ts:1115`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolverNode {
    pub node_uid: String,
    pub stable_key: String,
    pub name: String,
    pub qualified_name: Option<String>,
    pub kind: String,
    pub subtype: Option<String>,
    pub file_uid: Option<String>,
}

/// The in-memory index used for edge resolution. Built from
/// fetched data before resolution begins.
///
/// All maps use `HashMap` for O(1) lookup in the resolution hot
/// path. These are internal algorithm state, not public API
/// surfaces (the no-HashMap rule applies to public DTOs only).
pub struct ResolverIndex {
    /// Direct stable-key → node lookup.
    pub nodes_by_stable_key: HashMap<String, ResolverNode>,
    /// Name → all nodes with that name (may be ambiguous).
    pub nodes_by_name: HashMap<String, Vec<ResolverNode>>,
    /// Source node UID → file UID (for import-binding and
    /// include-path resolution).
    pub node_uid_to_file_uid: HashMap<String, String>,
    /// Extensionless stable-key → full stable-key with extension.
    pub file_resolution: HashMap<String, String>,
    /// Per-TU C/C++ include → header stable-key (v1.0 same-dir only).
    /// Outer key is source file UID.
    /// DEPRECATED: Use include_resolver for v1.1+ resolution.
    pub per_file_include_resolution: HashMap<String, HashMap<String, String>>,
    /// Stable-key → node UID (for module-edge creation).
    pub stable_key_to_uid: HashMap<String, String>,
    /// File node UID → module stable-key (for module-edge creation).
    pub file_to_module: HashMap<String, String>,
    /// v1.1 include resolver with conventional + configured roots.
    pub include_resolver: Option<IncludeResolutionMap>,
    /// IMPORT-RESOLUTION-RUST-1 §2.2: declared Rust crate import-name → crate root
    /// (repo-relative). Keyed by the SHARED canonical form
    /// (`canonicalize_cargo_package_name`), so an import spelled `repo_graph_storage`
    /// matches a package declared `repo-graph-storage`. Built by the orchestrator from
    /// `IndexOptions.declared_modules` (cargo ecosystem only). Empty when no crates were
    /// declared → the Rust-crate import stage is a no-op.
    pub rust_crate_roots: HashMap<String, String>,
}

/// A resolved edge — the symbolic `target_key` has been replaced
/// with a concrete `target_node_uid`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEdge {
    pub edge_uid: String,
    pub snapshot_uid: String,
    pub repo_uid: String,
    pub source_node_uid: String,
    pub target_node_uid: String,
    pub edge_type: EdgeType,
    pub resolution: Resolution,
    pub extractor: String,
    pub location: Option<SourceLocation>,
    pub metadata_json: Option<String>,
}

/// An unresolved edge with its assigned failure category.
#[derive(Debug, Clone)]
pub struct CategorizedUnresolvedEdge {
    pub edge: ExtractedEdge,
    pub category: repo_graph_classification::types::UnresolvedEdgeCategory,
    pub source_file_uid: Option<String>,
}

/// Result of resolving a batch of edges.
pub struct ResolutionResult {
    pub resolved: Vec<ResolvedEdge>,
    pub still_unresolved: Vec<CategorizedUnresolvedEdge>,
    /// TYPE-ONLY-IMPORTS-1: `(source_node_uid, target_node_uid, disposition)` triples for resolved
    /// IMPORTS edges. Used by the orchestrator to derive MODULE → MODULE import edges AND to compute
    /// the conjunctive per-module-edge type-only aggregate. `disposition` is the per-file-import
    /// disposition read back from the edge's `metadata_json` (`isTypeOnly`, injected at extraction from
    /// the parallel `ImportObservation`): `Some(TypeOnly)` = a TS/JS `import type` that vanishes at
    /// runtime; `Some(Runtime)` = a runtime import; `Some(Unreadable)` = the carrier was present but
    /// CORRUPT (distinct from absent); `None` = the fact was not present on the edge (a snapshot indexed
    /// before type-only tracking) — every non-`Runtime` case is carried through honestly, never demoted
    /// to runtime.
    pub resolved_import_pairs: Vec<(String, String, Option<TypeOnlyDisposition>)>,
}

// ── Subtypes that are type-only (not value-space) ────────────────

/// Mirror of `TYPE_ONLY_SUBTYPES` from `repo-indexer.ts:3028`.
///
/// NOTE: this concerns SYMBOL SUBTYPES (type aliases / interfaces are not value-space), a DIFFERENT
/// concept from an import statement's `import type` disposition ([`import_edge_type_only`] below).
fn is_type_only_subtype(subtype: Option<&str>) -> bool {
    matches!(subtype, Some("TYPE_ALIAS") | Some("INTERFACE"))
}

/// TYPE-ONLY-IMPORTS-1: the `import type` disposition of a resolved IMPORTS edge, read from the
/// `isTypeOnly` key its `metadata_json` carries. The key is injected at extraction
/// (`orchestrator::inject_import_type_only`) from the parallel `ImportObservation` — the extractor fact
/// at `ts-extractor:1350` is the single source; this is PLUMBING, not new extraction.
///
/// `Some(TypeOnly)` = a TS/JS `import type` / `export type … from` (vanishes at runtime);
/// `Some(Runtime)` = a runtime import (every non-TS/JS import is runtime by definition, stamped at
/// injection). `None` = the key is ABSENT (no `metadata_json`, or valid JSON without the key — a snapshot
/// indexed before type-only tracking, copied forward without the fact) — unknown.
///
/// `Some(Unreadable)` = the carrier was PRESENT but could not be read: `metadata_json` did not parse as
/// JSON, or its `isTypeOnly` value was not a boolean. This is a DISTINCT truth from an absent fact
/// (operator ruling 2026-09-03 item 2a — a corrupt fact and a pre-migration row are different truths);
/// the prior `.ok()?` collapsed it into the same `None` as absent, which STANDING HONESTY RULE 1 forbids
/// (never swallow a fallible read whose result is classified). Every non-`Runtime` case is carried
/// through as its own state, NEVER demoted to runtime (STANDING HONESTY RULE 2).
fn import_edge_type_only(edge: &ExtractedEdge) -> Option<TypeOnlyDisposition> {
    // No carrier at all ⇒ the fact is ABSENT (not present, not corrupt).
    let raw = edge.metadata_json.as_deref()?;
    // A carrier that does not parse is CORRUPT, distinct from absent — NOT silently swallowed.
    let value: serde_json::Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return Some(TypeOnlyDisposition::Unreadable),
    };
    match value.get("isTypeOnly") {
        // Valid JSON without the key ⇒ the fact was never stamped ⇒ ABSENT (indexed before tracking).
        None => None,
        Some(serde_json::Value::Bool(true)) => Some(TypeOnlyDisposition::TypeOnly),
        Some(serde_json::Value::Bool(false)) => Some(TypeOnlyDisposition::Runtime),
        // Key present but not a boolean ⇒ a CORRUPT value, distinct from absent.
        Some(_) => Some(TypeOnlyDisposition::Unreadable),
    }
}

// ── Resolution entry point ───────────────────────────────────────

/// Resolve a batch of unresolved edges against the resolver index.
///
/// For each edge, attempts to resolve the symbolic `target_key` to
/// a concrete `target_node_uid`. Successfully resolved edges are
/// returned as `ResolvedEdge`; failures are categorized and
/// returned as `CategorizedUnresolvedEdge`.
///
/// Mirror of `resolveEdges` from `repo-indexer.ts:2087`.
pub fn resolve_edges(
    edges: &[ExtractedEdge],
    index: &ResolverIndex,
    import_bindings_by_file: Option<&HashMap<String, Vec<ImportBinding>>>,
) -> ResolutionResult {
    let mut resolved = Vec::new();
    let mut still_unresolved = Vec::new();
    let mut resolved_import_pairs = Vec::new();

    for edge in edges {
        let resolution_outcome = resolve_target(edge, index, import_bindings_by_file);

        match resolution_outcome {
            TargetResolution::Resolved(uid) => {
                if edge.edge_type == EdgeType::Imports {
                    resolved_import_pairs.push((
                        edge.source_node_uid.clone(),
                        uid.clone(),
                        import_edge_type_only(edge),
                    ));
                }
                resolved.push(ResolvedEdge {
                    edge_uid: edge.edge_uid.clone(),
                    snapshot_uid: edge.snapshot_uid.clone(),
                    repo_uid: edge.repo_uid.clone(),
                    source_node_uid: edge.source_node_uid.clone(),
                    target_node_uid: uid,
                    edge_type: edge.edge_type,
                    resolution: edge.resolution,
                    extractor: edge.extractor.clone(),
                    location: edge.location,
                    metadata_json: edge.metadata_json.clone(),
                });
            }
            TargetResolution::Ambiguous(_candidates) => {
                // C/C++ include matched multiple headers exactly.
                let source_file_uid = index
                    .node_uid_to_file_uid
                    .get(&edge.source_node_uid)
                    .cloned();
                still_unresolved.push(CategorizedUnresolvedEdge {
                    edge: edge.clone(),
                    category: UnresolvedEdgeCategory::ImportsAmbiguousMatch,
                    source_file_uid,
                });
            }
            TargetResolution::Unresolved => {
                let category = categorize_unresolved_edge(edge);
                let source_file_uid = index
                    .node_uid_to_file_uid
                    .get(&edge.source_node_uid)
                    .cloned();
                still_unresolved.push(CategorizedUnresolvedEdge {
                    edge: edge.clone(),
                    category,
                    source_file_uid,
                });
            }
        }
    }

    ResolutionResult {
        resolved,
        still_unresolved,
        resolved_import_pairs,
    }
}

// ── Per-edge dispatch ────────────────────────────────────────────

fn resolve_target(
    edge: &ExtractedEdge,
    index: &ResolverIndex,
    import_bindings_by_file: Option<&HashMap<String, Vec<ImportBinding>>>,
) -> TargetResolution {
    match edge.edge_type {
        EdgeType::Imports => {
            let source_file_uid = index.node_uid_to_file_uid.get(&edge.source_node_uid);

            // v1.1: Try new include resolver first for C/C++ includes.
            if let Some(ref include_resolver) = index.include_resolver {
                if let Some(fuid) = source_file_uid {
                    // Extract file path from file UID (repo:path format).
                    if let Some(colon_pos) = fuid.find(':') {
                        let source_path = &fuid[colon_pos + 1..];
                        // Detect system include: C extractor sets metadata_json=None for system includes.
                        let is_system = edge.metadata_json.is_none();
                        let resolution =
                            include_resolver.resolve(source_path, &edge.target_key, is_system);
                        match resolution.status {
                            ResolutionStatus::Resolved => {
                                if let Some(ref stable_key) = resolution.target_stable_key {
                                    if let Some(node) = index.nodes_by_stable_key.get(stable_key) {
                                        return TargetResolution::Resolved(node.node_uid.clone());
                                    }
                                }
                            }
                            ResolutionStatus::Ambiguous => {
                                // Multiple exact matches — do not fall through.
                                return TargetResolution::Ambiguous(resolution.candidates);
                            }
                            ResolutionStatus::Unresolved => {
                                // Fall through to v1.0 stages.
                            }
                        }
                    }
                }
            }

            // Fallback: v1.0 same-directory resolution + other stages.
            let tu_includes =
                source_file_uid.and_then(|fuid| index.per_file_include_resolution.get(fuid));
            // Gate the declared-Rust-crate stage (3.5) to edges emitted by the Rust
            // extractor. In a hybrid repo a non-Rust IMPORTS edge (e.g. a TS
            // `import cli from …` or a C++ `namespace::` reference) whose first segment
            // happens to match a declared Cargo package name would otherwise resolve to
            // that crate's `.rs` file, changing non-Rust extractor behaviour and
            // violating the frozen byte-stability invariant. `ExtractedEdge.extractor`
            // is the existing provenance fact; the Rust family is `rust-core:<ver>`.
            let is_rust_import = edge.extractor.starts_with(RUST_EXTRACTOR_PREFIX);
            match resolve_import_target(
                &edge.target_key,
                &index.nodes_by_stable_key,
                &index.file_resolution,
                &edge.repo_uid,
                tu_includes,
                &index.rust_crate_roots,
                is_rust_import,
            ) {
                Some(uid) => TargetResolution::Resolved(uid),
                None => TargetResolution::Unresolved,
            }
        }
        EdgeType::Calls => option_to_resolution(resolve_call_target(
            &edge.target_key,
            &edge.source_node_uid,
            &index.nodes_by_stable_key,
            &index.nodes_by_name,
            &index.file_resolution,
            import_bindings_by_file,
            &index.node_uid_to_file_uid,
        )),
        EdgeType::Instantiates => option_to_resolution(resolve_named_target(
            &edge.target_key,
            &index.nodes_by_name,
            edge.edge_type,
        )),
        EdgeType::Implements => option_to_resolution(resolve_named_target(
            &edge.target_key,
            &index.nodes_by_name,
            edge.edge_type,
        )),
        // State-boundary edges (SB-4-pre): target_key is a stable
        // key (e.g. `myservice:fs:/etc/app.yaml:FS_PATH`), not a
        // symbol name. Try stable-key lookup first; fall back to
        // name resolution for any non-state-boundary READS/WRITES
        // edges that may use symbol names as target_key in the
        // future. `ACCESSES` (HONESTY-GATE-2 family 1) is the same
        // state-boundary shape with an undetermined direction.
        EdgeType::Reads | EdgeType::Writes | EdgeType::Accesses => {
            if let Some(node) = index.nodes_by_stable_key.get(&edge.target_key) {
                TargetResolution::Resolved(node.node_uid.clone())
            } else {
                option_to_resolution(resolve_named_target(
                    &edge.target_key,
                    &index.nodes_by_name,
                    edge.edge_type,
                ))
            }
        }
        // Remaining edge types resolve by unfiltered name lookup
        // (their affinity filter is the identity — see the matching
        // enumerated arm in `filter_by_edge_affinity`). Enumerated
        // EXHAUSTIVELY (no `_` arm) so that adding a future
        // `EdgeType` variant is a compile error HERE, forcing a
        // deliberate resolution-policy decision for it rather than
        // silently defaulting to name lookup (exhaustive-domain-sum
        // rule; the resolver dispatches on a fixed variant set).
        EdgeType::Emits
        | EdgeType::Consumes
        | EdgeType::RoutesTo
        | EdgeType::RegisteredBy
        | EdgeType::GatedBy
        | EdgeType::DependsOn
        | EdgeType::Owns
        | EdgeType::TestedBy
        | EdgeType::Covers
        | EdgeType::Throws
        | EdgeType::Catches
        | EdgeType::TransitionsTo => option_to_resolution(resolve_named_target(
            &edge.target_key,
            &index.nodes_by_name,
            edge.edge_type,
        )),
    }
}

/// Convert Option<String> to TargetResolution.
fn option_to_resolution(opt: Option<String>) -> TargetResolution {
    match opt {
        Some(uid) => TargetResolution::Resolved(uid),
        None => TargetResolution::Unresolved,
    }
}

// ── IMPORTS resolution ───────────────────────────────────────────

fn resolve_import_target(
    target_key: &str,
    nodes_by_stable_key: &HashMap<String, ResolverNode>,
    file_resolution: &HashMap<String, String>,
    repo_uid: &str,
    tu_include_resolution: Option<&HashMap<String, String>>,
    rust_crate_roots: &HashMap<String, String>,
    is_rust_import: bool,
) -> Option<String> {
    // Stage 1: direct stable-key lookup.
    if let Some(node) = nodes_by_stable_key.get(target_key) {
        return Some(node.node_uid.clone());
    }

    // Stage 2: extensionless → with-extension via file resolution map.
    if let Some(resolved_key) = file_resolution.get(target_key) {
        if let Some(node) = nodes_by_stable_key.get(resolved_key) {
            return Some(node.node_uid.clone());
        }
    }

    // Stage 3: C/C++ per-TU include resolution.
    if let Some(tu_map) = tu_include_resolution {
        if let Some(resolved_header) = tu_map.get(target_key) {
            if let Some(node) = nodes_by_stable_key.get(resolved_header) {
                return Some(node.node_uid.clone());
            }
        }
    }

    // Stage 3.5 (IMPORT-RESOLUTION-RUST-1 §2.2): declared Rust crate import.
    // A non-relative `use <crate>::…` was emitted with the raw specifier as its
    // target_key (`b::util`, `repo_graph_storage::crud`). Map its leading crate segment
    // to a declared crate root and generate candidate FILE keys under `<root>/src/`.
    // Runs ONLY after stages 1–3 miss (they never map a crate name to a directory), and
    // BEFORE the repo-prefix fallback (which is skipped anyway for a `::` key at stage 4).
    // Gated to Rust-extractor edges (`is_rust_import`): a non-Rust IMPORTS edge whose
    // leading segment matches a declared Cargo crate must NOT resolve to a `.rs` file
    // (frozen non-Rust byte-stability invariant).
    if is_rust_import {
        if let Some(resolved_key) =
            resolve_rust_crate_import(target_key, rust_crate_roots, file_resolution, repo_uid)
        {
            if let Some(node) = nodes_by_stable_key.get(&resolved_key) {
                return Some(node.node_uid.clone());
            }
        }
    }

    // Stage 4: repo-prefix fallback for bare header names.
    if !target_key.contains(':') {
        let constructed_key = format!("{}:{}:FILE", repo_uid, target_key);
        if let Some(resolved) = file_resolution.get(&constructed_key) {
            if let Some(node) = nodes_by_stable_key.get(resolved) {
                return Some(node.node_uid.clone());
            }
        }
        if let Some(node) = nodes_by_stable_key.get(&constructed_key) {
            return Some(node.node_uid.clone());
        }
    }

    None
}

/// IMPORT-RESOLUTION-RUST-1 §2.2: resolve a non-relative Rust `use <crate>::…` IMPORTS
/// target_key to the FILE stable key that defines it, via the declared-crate catalog.
///
/// PURE candidate generation: `(target_key, catalog, file set, repo_uid) → Option<file
/// stable key>`. No I/O. The `file set` is the identity `file_resolution` map (a key is
/// present iff that FILE exists in the snapshot); the returned value is the resolved FILE
/// stable key the caller looks up in `nodes_by_stable_key`.
///
/// For `first::rest…` where `first` (canonicalised `_`→`-`) names a declared crate and
/// `segs` is the remainder (the path minus the crate name), candidates are generated under
/// `<crate_root>/src/` in this order — `<segs>.rs`, `<segs>/mod.rs`, then the last segment
/// is dropped and the pair repeats, ending at the crate entrypoint `src/lib.rs` (a bin-only
/// crate has no `lib.rs`, so `src/main.rs` is tried next). The FIRST candidate present in
/// the file set wins. The crate's own `crate::`/`super::`/`self::` paths are relative and
/// were already turned into `:FILE` keys by the extractor — they never reach here.
fn resolve_rust_crate_import(
    target_key: &str,
    rust_crate_roots: &HashMap<String, String>,
    file_resolution: &HashMap<String, String>,
    repo_uid: &str,
) -> Option<String> {
    if rust_crate_roots.is_empty() {
        return None;
    }
    let mut segs = target_key.split("::");
    let first = segs.next()?;
    if first.is_empty() {
        return None;
    }
    let canonical = canonicalize_cargo_package_name(first);
    let crate_root = rust_crate_roots.get(&canonical)?;

    // `<crate_root>/src`, collapsing a "." (or empty) root crate to a bare `src`.
    let src_prefix = if crate_root == "." || crate_root.is_empty() {
        "src".to_string()
    } else {
        format!("{}/src", crate_root)
    };

    let probe = |rel_path: &str| -> Option<String> {
        let key = format!("{}:{}:FILE", repo_uid, rel_path);
        file_resolution.get(&key).cloned()
    };

    let rest: Vec<&str> = segs.collect();
    let mut remaining = rest.as_slice();
    loop {
        if remaining.is_empty() {
            // Crate entrypoint: lib.rs (library) then main.rs (bin-only).
            for entry in ["lib.rs", "main.rs"] {
                if let Some(hit) = probe(&format!("{}/{}", src_prefix, entry)) {
                    return Some(hit);
                }
            }
            return None;
        }
        let joined = remaining.join("/");
        if let Some(hit) = probe(&format!("{}/{}.rs", src_prefix, joined)) {
            return Some(hit);
        }
        if let Some(hit) = probe(&format!("{}/{}/mod.rs", src_prefix, joined)) {
            return Some(hit);
        }
        remaining = &remaining[..remaining.len() - 1];
    }
}

// ── CALLS resolution ─────────────────────────────────────────────

fn resolve_call_target(
    target_key: &str,
    source_node_uid: &str,
    _nodes_by_stable_key: &HashMap<String, ResolverNode>,
    nodes_by_name: &HashMap<String, Vec<ResolverNode>>,
    file_resolution: &HashMap<String, String>,
    import_bindings_by_file: Option<&HashMap<String, Vec<ImportBinding>>>,
    node_uid_to_file_uid: &HashMap<String, String>,
) -> Option<String> {
    // ── Namespace import resolution ───────────────────────────────────
    // For calls like `fs.readFile()` where `fs` is a namespace import,
    // extract the member name and look it up in the imported module.
    if target_key.contains('.') {
        if let Some(bindings_map) = import_bindings_by_file {
            if let Some(source_file_uid) = node_uid_to_file_uid.get(source_node_uid) {
                if let Some(bindings) = bindings_map.get(source_file_uid) {
                    // Extract potential namespace prefix and member
                    let parts: Vec<&str> = target_key.splitn(2, '.').collect();
                    if parts.len() == 2 {
                        let prefix = parts[0];
                        let member = parts[1];

                        // Check if prefix matches a namespace import
                        if let Some(binding) = bindings
                            .iter()
                            .find(|b| b.identifier == prefix && b.kind == ImportKind::Namespace)
                        {
                            if let Some(resolved_file_uid) = resolve_import_specifier_to_file(
                                &binding.specifier,
                                source_file_uid,
                                file_resolution,
                            ) {
                                // For nested member access like `fs.promises.readFile`,
                                // we only handle the immediate member for now.
                                // Extract the first part of the member (before any dot).
                                let immediate_member = member.split('.').next().unwrap_or(member);

                                if let Some(candidates) = nodes_by_name.get(immediate_member) {
                                    let in_file: Vec<ResolverNode> = candidates
                                        .iter()
                                        .filter(|n| {
                                            n.file_uid.as_deref()
                                                == Some(resolved_file_uid.as_str())
                                        })
                                        .cloned()
                                        .collect();
                                    if let Some(uid) =
                                        pick_unambiguous(Some(&in_file), EdgeType::Calls)
                                    {
                                        return Some(uid);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Dotted name fallback ──────────────────────────────────────────
    // For non-namespace cases: extract method name from "obj.method" or
    // "this.field.method" patterns.
    //
    // IMPORTANT: Skip this fallback for default-import member access.
    // `import fs from "fs"; fs.readFile()` must NOT resolve by bare
    // method name lookup, as that creates false positives. Default
    // import member access is conservatively unresolved until we model
    // default export structure.
    if target_key.contains('.') {
        let parts: Vec<&str> = target_key.split('.').collect();
        let prefix = parts[0];
        let method_name = parts[parts.len() - 1];

        // Check if prefix matches a Default import — if so, skip fallback.
        // This prevents false positives for default-import member access.
        let prefix_is_default_import = import_bindings_by_file
            .and_then(|bindings_map| {
                node_uid_to_file_uid
                    .get(source_node_uid)
                    .and_then(|file_uid| bindings_map.get(file_uid))
            })
            .map(|bindings| {
                bindings
                    .iter()
                    .any(|b| b.identifier == prefix && b.kind == ImportKind::Default)
            })
            .unwrap_or(false);

        if prefix_is_default_import {
            // Conservative: default-import member access is not resolved.
            // Fall through to return None.
        } else {
            // "this.repo.findById" style (3+ parts starting with "this")
            if prefix == "this" && parts.len() >= 3 {
                if let Some(uid) = pick_unambiguous(nodes_by_name.get(method_name), EdgeType::Calls)
                {
                    return Some(uid);
                }
            }

            // "obj.method()" — try method name (only for non-import objects).
            if let Some(uid) = pick_unambiguous(nodes_by_name.get(method_name), EdgeType::Calls) {
                return Some(uid);
            }
        }
    }

    // Simple function call: "classifyMedia".
    if let Some(uid) = pick_unambiguous(nodes_by_name.get(target_key), EdgeType::Calls) {
        return Some(uid);
    }

    // ── Named import resolution ───────────────────────────────────────
    // For aliased named imports: look up the original exported name.
    if let Some(bindings_map) = import_bindings_by_file {
        if let Some(source_file_uid) = node_uid_to_file_uid.get(source_node_uid) {
            if let Some(bindings) = bindings_map.get(source_file_uid) {
                if let Some(binding) = bindings.iter().find(|b| b.identifier == target_key) {
                    if let Some(resolved_file_uid) = resolve_import_specifier_to_file(
                        &binding.specifier,
                        source_file_uid,
                        file_resolution,
                    ) {
                        // For aliased named imports, use the original exported
                        // name (e.g., "readFile" not "rf" for `import { readFile as rf }`).
                        // For default imports, imported_name is None — conservative fallback.
                        let lookup_name = binding.imported_name.as_deref().unwrap_or(target_key);
                        if let Some(candidates) = nodes_by_name.get(lookup_name) {
                            let in_file: Vec<ResolverNode> = candidates
                                .iter()
                                .filter(|n| {
                                    n.file_uid.as_deref() == Some(resolved_file_uid.as_str())
                                })
                                .cloned()
                                .collect();
                            if let Some(uid) = pick_unambiguous(Some(&in_file), EdgeType::Calls) {
                                return Some(uid);
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

/// Resolve a relative import specifier to a file UID.
///
/// Mirror of `resolveImportSpecifierToFile` from
/// `repo-indexer.ts:2327`.
fn resolve_import_specifier_to_file(
    specifier: &str,
    source_file_uid: &str,
    file_resolution: &HashMap<String, String>,
) -> Option<String> {
    if !specifier.starts_with('.') {
        return None;
    }

    let colon_idx = source_file_uid.find(':')?;
    let repo_uid = &source_file_uid[..colon_idx];
    let source_path = &source_file_uid[colon_idx + 1..];
    let source_dir = match source_path.rfind('/') {
        Some(pos) => &source_path[..pos],
        None => "",
    };

    let resolved_path = resolve_relative_path(source_dir, specifier);
    let target_file_key = format!("{}:{}:FILE", repo_uid, resolved_path);

    if let Some(resolved_stable_key) = file_resolution.get(&target_file_key) {
        // Extract file_uid: "repoUid:path.ts:FILE" → "repoUid:path.ts"
        let file_uid = resolved_stable_key
            .strip_suffix(":FILE")
            .unwrap_or(resolved_stable_key);
        return Some(file_uid.to_string());
    }

    None
}

// ── Named target resolution (INSTANTIATES/IMPLEMENTS/other) ──────

fn resolve_named_target(
    target_key: &str,
    nodes_by_name: &HashMap<String, Vec<ResolverNode>>,
    edge_type: EdgeType,
) -> Option<String> {
    pick_unambiguous(nodes_by_name.get(target_key), edge_type)
}

// ── Affinity filtering + singleton check ─────────────────────────

/// Apply declaration-space affinity filtering then check for an
/// unambiguous singleton result.
///
/// Mirror of `pickUnambiguous` from `repo-indexer.ts:2854`.
fn pick_unambiguous(candidates: Option<&Vec<ResolverNode>>, edge_type: EdgeType) -> Option<String> {
    let candidates = candidates?;
    if candidates.is_empty() {
        return None;
    }

    let filtered = filter_by_edge_affinity(candidates, edge_type);
    if filtered.len() == 1 {
        return Some(filtered[0].node_uid.clone());
    }

    None
}

/// Filter candidates by declaration-space affinity. Returns only
/// candidates in the correct space for the edge type.
///
/// Mirror of `filterByEdgeAffinity` from `repo-indexer.ts:3049`.
pub fn filter_by_edge_affinity(
    candidates: &[ResolverNode],
    edge_type: EdgeType,
) -> Vec<&ResolverNode> {
    match edge_type {
        EdgeType::Instantiates => candidates
            .iter()
            .filter(|n| n.subtype.as_deref() == Some("CLASS"))
            .collect(),
        EdgeType::Implements => candidates
            .iter()
            .filter(|n| n.subtype.as_deref() == Some("INTERFACE"))
            .collect(),
        EdgeType::Calls => candidates
            .iter()
            .filter(|n| !is_type_only_subtype(n.subtype.as_deref()))
            .collect(),
        // All remaining edge types apply the identity affinity filter (no
        // declaration-space narrowing). Enumerated EXHAUSTIVELY (no `_`
        // arm) so that adding a future `EdgeType` variant is a compile
        // error HERE, forcing a deliberate affinity decision rather than
        // silently inheriting the identity filter (exhaustive-domain-sum
        // rule). `Accesses` (HONESTY-GATE-2 family 1) joins the
        // state-boundary edges here.
        EdgeType::Imports
        | EdgeType::Reads
        | EdgeType::Writes
        | EdgeType::Accesses
        | EdgeType::Emits
        | EdgeType::Consumes
        | EdgeType::RoutesTo
        | EdgeType::RegisteredBy
        | EdgeType::GatedBy
        | EdgeType::DependsOn
        | EdgeType::Owns
        | EdgeType::TestedBy
        | EdgeType::Covers
        | EdgeType::Throws
        | EdgeType::Catches
        | EdgeType::TransitionsTo => candidates.iter().collect(),
    }
}

// ── Unresolved edge categorization ───────────────────────────────

/// Assign a failure category to an unresolved edge.
///
/// Mirror of `categorizeUnresolvedEdge` from
/// `repo-indexer.ts:3091`.
pub fn categorize_unresolved_edge(
    edge: &ExtractedEdge,
) -> repo_graph_classification::types::UnresolvedEdgeCategory {
    use repo_graph_classification::types::UnresolvedEdgeCategory;

    match edge.edge_type {
        EdgeType::Imports => UnresolvedEdgeCategory::ImportsFileNotFound,
        EdgeType::Instantiates => UnresolvedEdgeCategory::InstantiatesClassNotFound,
        EdgeType::Implements => UnresolvedEdgeCategory::ImplementsInterfaceNotFound,
        EdgeType::Calls => categorize_unresolved_call(edge),
        // All remaining edge types have no dedicated failure category yet
        // and fall to `Other`. Enumerated EXHAUSTIVELY (no `_` arm) so that
        // adding a future `EdgeType` variant is a compile error HERE,
        // forcing a deliberate categorization decision rather than silently
        // defaulting to `Other` (exhaustive-domain-sum rule). `Accesses`
        // (HONESTY-GATE-2 family 1) categorizes as `Other` like the other
        // state-boundary edges.
        EdgeType::Reads
        | EdgeType::Writes
        | EdgeType::Accesses
        | EdgeType::Emits
        | EdgeType::Consumes
        | EdgeType::RoutesTo
        | EdgeType::RegisteredBy
        | EdgeType::GatedBy
        | EdgeType::DependsOn
        | EdgeType::Owns
        | EdgeType::TestedBy
        | EdgeType::Covers
        | EdgeType::Throws
        | EdgeType::Catches
        | EdgeType::TransitionsTo => UnresolvedEdgeCategory::Other,
    }
}

fn categorize_unresolved_call(
    edge: &ExtractedEdge,
) -> repo_graph_classification::types::UnresolvedEdgeCategory {
    use repo_graph_classification::types::UnresolvedEdgeCategory;

    // Use rawCalleeName from metadata if present (handles rewritten
    // "this.save()" → "ClassName.save" target keys).
    let key = if let Some(ref meta_str) = edge.metadata_json {
        serde_json::from_str::<serde_json::Value>(meta_str)
            .ok()
            .and_then(|v| v.get("rawCalleeName")?.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| edge.target_key.clone())
    } else {
        edge.target_key.clone()
    };

    if key.starts_with("this.") {
        let dot_count = key.chars().filter(|&c| c == '.').count();
        if dot_count > 1 {
            return UnresolvedEdgeCategory::CallsThisWildcardMethodNeedsTypeInfo;
        }
        return UnresolvedEdgeCategory::CallsThisMethodNeedsClassContext;
    }
    if key.contains('.') {
        return UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo;
    }
    UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing
}

// ── Path helpers ─────────────────────────────────────────────────

/// Resolve a relative path specifier against a source directory.
///
/// Mirror of `resolveRelativePath` from `repo-indexer.ts:2945`.
pub fn resolve_relative_path(source_dir: &str, specifier: &str) -> String {
    let mut parts: Vec<&str> = if source_dir.is_empty() {
        Vec::new()
    } else {
        source_dir.split('/').collect()
    };

    for seg in specifier.split('/') {
        if seg == "." {
            continue;
        } else if seg == ".." {
            parts.pop();
        } else {
            parts.push(seg);
        }
    }

    parts.join("/")
}

/// Extract the parent directory path from a file path. Returns
/// `None` for top-level files (no `/` in path).
///
/// Mirror of `getModulePath` from `repo-indexer.ts:2998`.
pub fn get_module_path(file_path: &str) -> Option<&str> {
    let last_slash = file_path.rfind('/')?;
    if last_slash > 0 {
        Some(&file_path[..last_slash])
    } else {
        None
    }
}

// ── File resolution map builder ──────────────────────────────────

/// Build the file resolution map: extensionless stable-key →
/// full stable-key with extension. Also handles index-file
/// directory shortcuts.
///
/// Mirror of the file resolution construction at
/// `repo-indexer.ts:1879`.
pub fn build_file_resolution_map(file_paths: &[String], repo_uid: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();

    for path in file_paths {
        let stable_key = format!("{}:{}:FILE", repo_uid, path);

        // Exact path → self (identity).
        map.insert(stable_key.clone(), stable_key.clone());

        // Extensionless → with extension.
        let without_ext = strip_extension(path);
        let extless_key = format!("{}:{}:FILE", repo_uid, without_ext);
        map.entry(extless_key).or_insert_with(|| stable_key.clone());

        // Index-file directory shortcut:
        // "src/core/index.ts" → "src/core" maps to the index file.
        if path.ends_with("/index.ts") || path.ends_with("/index.tsx") {
            let dir_path = if path.ends_with("/index.tsx") {
                &path[..path.len() - "/index.tsx".len()]
            } else {
                &path[..path.len() - "/index.ts".len()]
            };
            let dir_key = format!("{}:{}:FILE", repo_uid, dir_path);
            map.entry(dir_key).or_insert_with(|| stable_key.clone());
        }

        // Python package directory shortcut:
        // "src/core/__init__.py" → "src/core" maps to the package init.
        // This enables `from .core import X` to resolve to the package.
        if path.ends_with("/__init__.py") {
            let dir_path = &path[..path.len() - "/__init__.py".len()];
            let dir_key = format!("{}:{}:FILE", repo_uid, dir_path);
            map.entry(dir_key).or_insert(stable_key);
        }
    }

    map
}

/// Build the per-file include resolution map for C/C++ files.
///
/// For each C/C++ source file, maps bare header names to their
/// stable keys based on the source file's directory. This enables
/// resolving `#include "helper.h"` from `src/core/main.c` to
/// `src/core/helper.h` if that header exists.
///
/// Outer key: source file UID (e.g., `r1:src/core/main.c`)
/// Inner key: bare header name (e.g., `helper.h`)
/// Value: resolved stable key (e.g., `r1:src/core/helper.h:FILE`)
pub fn build_per_file_include_resolution(
    file_paths: &[String],
    repo_uid: &str,
) -> HashMap<String, HashMap<String, String>> {
    let mut result: HashMap<String, HashMap<String, String>> = HashMap::new();

    // Collect all header files by directory.
    // Key: directory path (e.g., "src/core")
    // Value: Vec of (basename, full_path) for headers in that dir
    let mut headers_by_dir: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();

    for path in file_paths {
        if path.ends_with(".h") || path.ends_with(".hpp") || path.ends_with(".hxx") {
            let dir = get_module_path(path).unwrap_or("");
            let basename = path.rsplit('/').next().unwrap_or(path);
            headers_by_dir
                .entry(dir)
                .or_default()
                .push((basename, path.as_str()));
        }
    }

    // For each C/C++ file (source AND header), build its include resolution map.
    // Headers can also have #include directives that need resolution.
    for path in file_paths {
        let is_c_file = path.ends_with(".c")
            || path.ends_with(".cpp")
            || path.ends_with(".cc")
            || path.ends_with(".cxx")
            || path.ends_with(".h")
            || path.ends_with(".hpp")
            || path.ends_with(".hxx");

        if is_c_file {
            let file_uid = format!("{}:{}", repo_uid, path);
            let source_dir = get_module_path(path).unwrap_or("");

            // Collect headers accessible from this file's directory.
            if let Some(headers) = headers_by_dir.get(source_dir) {
                let mut file_map = HashMap::new();
                for (basename, header_path) in headers {
                    let header_stable_key = format!("{}:{}:FILE", repo_uid, header_path);
                    file_map.insert((*basename).to_string(), header_stable_key);
                }
                if !file_map.is_empty() {
                    result.insert(file_uid, file_map);
                }
            }
        }
    }

    result
}

/// Strip the file extension for languages that use extensionless
/// import specifiers: JS/TS family and Python.
///
/// - `.ts`, `.tsx`, `.js`, `.jsx`: JS/TS ecosystem
/// - `.py`: Python (`from .service import X` → `./service`)
///
/// Other extensions (`.rs`, `.java`, `.c`, `.h`) are left intact
/// because those languages do not use extensionless import paths.
fn strip_extension(path: &str) -> &str {
    let dot_pos = match path.rfind('.') {
        Some(p) => p,
        None => return path,
    };
    let ext = &path[dot_pos..];
    match ext {
        ".ts" | ".tsx" | ".js" | ".jsx" | ".py" => &path[..dot_pos],
        _ => path,
    }
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_classification::types::UnresolvedEdgeCategory;

    fn make_node(
        uid: &str,
        stable_key: &str,
        name: &str,
        subtype: Option<&str>,
        file_uid: Option<&str>,
    ) -> ResolverNode {
        ResolverNode {
            node_uid: uid.into(),
            stable_key: stable_key.into(),
            name: name.into(),
            qualified_name: None,
            kind: "SYMBOL".into(),
            subtype: subtype.map(|s| s.into()),
            file_uid: file_uid.map(|s| s.into()),
        }
    }

    fn make_edge(uid: &str, target_key: &str, edge_type: EdgeType) -> ExtractedEdge {
        ExtractedEdge {
            edge_uid: uid.into(),
            snapshot_uid: "snap1".into(),
            repo_uid: "r1".into(),
            source_node_uid: "src1".into(),
            target_key: target_key.into(),
            edge_type,
            resolution: Resolution::Static,
            extractor: "test:1".into(),
            location: None,
            metadata_json: None,
        }
    }

    // ── import_edge_type_only (TYPE-ONLY-IMPORTS-1) ──────────

    #[test]
    fn import_type_only_reads_injected_metadata() {
        use TypeOnlyDisposition::*;
        let mut e = make_edge("e1", "r1:src/a:FILE", EdgeType::Imports);
        // A type-only import (the `isTypeOnly` key injected at extraction).
        e.metadata_json = Some(r#"{"resolvedPath":"src/a","isTypeOnly":true}"#.into());
        assert_eq!(import_edge_type_only(&e), Some(TypeOnly));
        // A runtime import.
        e.metadata_json = Some(r#"{"resolvedPath":"src/a","isTypeOnly":false}"#.into());
        assert_eq!(import_edge_type_only(&e), Some(Runtime));
        // Absent key (valid JSON, no `isTypeOnly`) ⇒ ABSENT (indexed before tracking), NOT runtime.
        e.metadata_json = Some(r#"{"resolvedPath":"src/a"}"#.into());
        assert_eq!(import_edge_type_only(&e), None);
        // No metadata at all ⇒ ABSENT.
        e.metadata_json = None;
        assert_eq!(import_edge_type_only(&e), None);
    }

    #[test]
    fn import_type_only_malformed_carrier_is_unreadable_not_absent() {
        // Operator ruling 2026-09-03 item 2a: a CORRUPT carrier is its own truth, DISTINCT from an
        // absent one — the prior `.ok()?` collapsed both into the same `None` (honesty rule 1 breach).
        use TypeOnlyDisposition::*;
        let mut e = make_edge("e1", "r1:src/a:FILE", EdgeType::Imports);
        // Present but unparseable JSON ⇒ Unreadable, NOT None (absent).
        e.metadata_json = Some("not json".into());
        assert_eq!(import_edge_type_only(&e), Some(Unreadable));
        // Present, valid JSON, but `isTypeOnly` is the wrong type ⇒ Unreadable (corrupt value).
        e.metadata_json = Some(r#"{"isTypeOnly":"yes"}"#.into());
        assert_eq!(import_edge_type_only(&e), Some(Unreadable));
    }

    // ── filter_by_edge_affinity ──────────────────────────────

    #[test]
    fn affinity_instantiates_filters_to_class() {
        let nodes = vec![
            make_node("n1", "k1", "Foo", Some("CLASS"), None),
            make_node("n2", "k2", "Foo", Some("INTERFACE"), None),
        ];
        let filtered = filter_by_edge_affinity(&nodes, EdgeType::Instantiates);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].node_uid, "n1");
    }

    #[test]
    fn affinity_implements_filters_to_interface() {
        let nodes = vec![
            make_node("n1", "k1", "Bar", Some("CLASS"), None),
            make_node("n2", "k2", "Bar", Some("INTERFACE"), None),
        ];
        let filtered = filter_by_edge_affinity(&nodes, EdgeType::Implements);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].node_uid, "n2");
    }

    #[test]
    fn affinity_calls_excludes_type_only() {
        let nodes = vec![
            make_node("n1", "k1", "doStuff", Some("FUNCTION"), None),
            make_node("n2", "k2", "doStuff", Some("TYPE_ALIAS"), None),
            make_node("n3", "k3", "doStuff", Some("INTERFACE"), None),
        ];
        let filtered = filter_by_edge_affinity(&nodes, EdgeType::Calls);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].node_uid, "n1");
    }

    // ── categorize_unresolved_edge ───────────────────────────

    #[test]
    fn categorize_imports() {
        let edge = make_edge("e1", "./missing", EdgeType::Imports);
        assert_eq!(
            categorize_unresolved_edge(&edge),
            UnresolvedEdgeCategory::ImportsFileNotFound
        );
    }

    #[test]
    fn categorize_calls_this_method() {
        let edge = make_edge("e1", "this.save", EdgeType::Calls);
        assert_eq!(
            categorize_unresolved_edge(&edge),
            UnresolvedEdgeCategory::CallsThisMethodNeedsClassContext
        );
    }

    #[test]
    fn categorize_calls_this_wildcard() {
        let edge = make_edge("e1", "this.repo.findById", EdgeType::Calls);
        assert_eq!(
            categorize_unresolved_edge(&edge),
            UnresolvedEdgeCategory::CallsThisWildcardMethodNeedsTypeInfo
        );
    }

    #[test]
    fn categorize_calls_obj_method() {
        let edge = make_edge("e1", "db.query", EdgeType::Calls);
        assert_eq!(
            categorize_unresolved_edge(&edge),
            UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo
        );
    }

    #[test]
    fn categorize_calls_function() {
        let edge = make_edge("e1", "classifyMedia", EdgeType::Calls);
        assert_eq!(
            categorize_unresolved_edge(&edge),
            UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing
        );
    }

    #[test]
    fn categorize_uses_raw_callee_name_from_metadata() {
        let mut edge = make_edge("e1", "ClassName.save", EdgeType::Calls);
        edge.metadata_json = Some(r#"{"rawCalleeName":"this.save"}"#.into());
        assert_eq!(
            categorize_unresolved_edge(&edge),
            UnresolvedEdgeCategory::CallsThisMethodNeedsClassContext
        );
    }

    // ── resolve_relative_path ────────────────────────────────

    #[test]
    fn relative_path_sibling() {
        assert_eq!(
            resolve_relative_path("src/core", "./utils"),
            "src/core/utils"
        );
    }

    #[test]
    fn relative_path_parent() {
        assert_eq!(
            resolve_relative_path("src/core/api", "../utils"),
            "src/core/utils"
        );
    }

    #[test]
    fn relative_path_from_root() {
        assert_eq!(resolve_relative_path("", "./src/index"), "src/index");
    }

    // ── get_module_path ──────────────────────────────────────

    #[test]
    fn module_path_nested() {
        assert_eq!(get_module_path("src/core/service.ts"), Some("src/core"));
    }

    #[test]
    fn module_path_top_level() {
        assert_eq!(get_module_path("index.ts"), None);
    }

    // ── build_file_resolution_map ────────────────────────────

    #[test]
    fn file_resolution_extensionless() {
        let paths = vec!["src/core/utils.ts".to_string()];
        let map = build_file_resolution_map(&paths, "r1");
        // Exact key.
        assert_eq!(
            map.get("r1:src/core/utils.ts:FILE"),
            Some(&"r1:src/core/utils.ts:FILE".to_string())
        );
        // Extensionless.
        assert_eq!(
            map.get("r1:src/core/utils:FILE"),
            Some(&"r1:src/core/utils.ts:FILE".to_string())
        );
    }

    #[test]
    fn file_resolution_index_shortcut() {
        let paths = vec!["src/core/index.ts".to_string()];
        let map = build_file_resolution_map(&paths, "r1");
        // Directory shortcut.
        assert_eq!(
            map.get("r1:src/core:FILE"),
            Some(&"r1:src/core/index.ts:FILE".to_string())
        );
    }

    #[test]
    fn file_resolution_python_extensionless() {
        let paths = vec!["src/service.py".to_string()];
        let map = build_file_resolution_map(&paths, "r1");
        // Exact key.
        assert_eq!(
            map.get("r1:src/service.py:FILE"),
            Some(&"r1:src/service.py:FILE".to_string())
        );
        // Extensionless (Python imports don't include .py).
        assert_eq!(
            map.get("r1:src/service:FILE"),
            Some(&"r1:src/service.py:FILE".to_string())
        );
    }

    #[test]
    fn file_resolution_python_init_shortcut() {
        // Python package: `from .core import X` should resolve to __init__.py
        let paths = vec!["src/core/__init__.py".to_string()];
        let map = build_file_resolution_map(&paths, "r1");
        // Directory shortcut → package init file.
        assert_eq!(
            map.get("r1:src/core:FILE"),
            Some(&"r1:src/core/__init__.py:FILE".to_string())
        );
    }

    // ── strip_extension ──────────────────────────────────────

    #[test]
    fn strip_ext_ts() {
        assert_eq!(strip_extension("src/core/utils.ts"), "src/core/utils");
    }

    #[test]
    fn strip_ext_tsx() {
        assert_eq!(strip_extension("src/App.tsx"), "src/App");
    }

    #[test]
    fn strip_ext_no_extension() {
        assert_eq!(strip_extension("Makefile"), "Makefile");
    }

    #[test]
    fn strip_ext_python() {
        // Python uses extensionless imports, so .py is stripped.
        assert_eq!(strip_extension("src/app.py"), "src/app");
        assert_eq!(
            strip_extension("src/utils/__init__.py"),
            "src/utils/__init__"
        );
    }

    #[test]
    fn strip_ext_preserves_other_extensions() {
        // Rust, Java, C/C++ extensions are NOT stripped.
        assert_eq!(strip_extension("src/main.rs"), "src/main.rs");
        assert_eq!(strip_extension("src/Foo.java"), "src/Foo.java");
        assert_eq!(strip_extension("src/util.h"), "src/util.h");
        assert_eq!(strip_extension("src/util.cpp"), "src/util.cpp");
    }

    // ── resolve_edges integration ────────────────────────────

    #[test]
    fn resolve_import_by_stable_key() {
        let target_node = make_node("file1", "r1:src/utils.ts:FILE", "utils.ts", None, None);
        let mut index = ResolverIndex {
            nodes_by_stable_key: HashMap::new(),
            nodes_by_name: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
        };
        index
            .nodes_by_stable_key
            .insert(target_node.stable_key.clone(), target_node);

        let edge = make_edge("e1", "r1:src/utils.ts:FILE", EdgeType::Imports);
        let result = resolve_edges(&[edge], &index, None);

        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].target_node_uid, "file1");
        assert_eq!(result.still_unresolved.len(), 0);
        assert_eq!(result.resolved_import_pairs.len(), 1);
    }

    #[test]
    fn resolve_call_singleton_by_name() {
        let func_node = make_node(
            "fn1",
            "r1:src/utils.ts:classifyMedia:SYMBOL",
            "classifyMedia",
            Some("FUNCTION"),
            Some("r1:src/utils.ts"),
        );
        let mut index = ResolverIndex {
            nodes_by_stable_key: HashMap::new(),
            nodes_by_name: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
        };
        index
            .nodes_by_name
            .entry("classifyMedia".into())
            .or_default()
            .push(func_node);

        let edge = make_edge("e1", "classifyMedia", EdgeType::Calls);
        let result = resolve_edges(&[edge], &index, None);

        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].target_node_uid, "fn1");
    }

    #[test]
    fn ambiguous_name_stays_unresolved() {
        let n1 = make_node("n1", "k1", "doStuff", Some("FUNCTION"), None);
        let n2 = make_node("n2", "k2", "doStuff", Some("FUNCTION"), None);
        let mut index = ResolverIndex {
            nodes_by_stable_key: HashMap::new(),
            nodes_by_name: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
        };
        index
            .nodes_by_name
            .entry("doStuff".into())
            .or_default()
            .extend(vec![n1, n2]);

        let edge = make_edge("e1", "doStuff", EdgeType::Calls);
        let result = resolve_edges(&[edge], &index, None);

        assert_eq!(result.resolved.len(), 0);
        assert_eq!(result.still_unresolved.len(), 1);
        assert_eq!(
            result.still_unresolved[0].category,
            UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing
        );
    }

    // ── resolve_rust_crate_import (IMPORT-RESOLUTION-RUST-1 §2.2) ─────

    /// Build a file_resolution identity map for a set of repo-relative paths (mirrors
    /// `build_file_resolution_map`'s identity entries, which is all this stage probes).
    fn file_set(paths: &[&str], repo_uid: &str) -> HashMap<String, String> {
        paths
            .iter()
            .map(|p| {
                let k = format!("{}:{}:FILE", repo_uid, p);
                (k.clone(), k)
            })
            .collect()
    }

    fn crate_roots(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(n, r)| (n.to_string(), r.to_string()))
            .collect()
    }

    #[test]
    fn rust_crate_import_leaf_module_file() {
        // `use b::util::helper` → target_key "b::util" → b/src/util.rs.
        let roots = crate_roots(&[("b", "b")]);
        let files = file_set(&["b/src/util.rs", "b/src/lib.rs"], "r1");
        assert_eq!(
            resolve_rust_crate_import("b::util", &roots, &files, "r1"),
            Some("r1:b/src/util.rs:FILE".to_string())
        );
    }

    #[test]
    fn rust_crate_import_bare_crate_hits_lib_rs() {
        // `use b::Thing` → target_key "b" → the crate entrypoint b/src/lib.rs.
        let roots = crate_roots(&[("b", "b")]);
        let files = file_set(&["b/src/util.rs", "b/src/lib.rs"], "r1");
        assert_eq!(
            resolve_rust_crate_import("b", &roots, &files, "r1"),
            Some("r1:b/src/lib.rs:FILE".to_string())
        );
    }

    #[test]
    fn rust_crate_import_nested_prefers_mod_rs() {
        // `crud/foo` with only `crud/mod.rs` present: the leaf `crud/foo.rs` and
        // `crud/foo/mod.rs` miss, then the path shortens to `crud` → `crud/mod.rs`.
        let roots = crate_roots(&[("repo-graph-storage", "rust/crates/storage")]);
        let files = file_set(
            &[
                "rust/crates/storage/src/crud/mod.rs",
                "rust/crates/storage/src/lib.rs",
            ],
            "r1",
        );
        // Import path uses underscores; canonicalisation maps it to the declared name.
        assert_eq!(
            resolve_rust_crate_import("repo_graph_storage::crud::foo", &roots, &files, "r1"),
            Some("r1:rust/crates/storage/src/crud/mod.rs:FILE".to_string())
        );
    }

    #[test]
    fn rust_crate_import_shortened_to_parent_file() {
        // `crud::foo` where only `crud.rs` exists: leaf misses, shorten to `crud` → crud.rs.
        let roots = crate_roots(&[("b", "b")]);
        let files = file_set(&["b/src/crud.rs", "b/src/lib.rs"], "r1");
        assert_eq!(
            resolve_rust_crate_import("b::crud::foo", &roots, &files, "r1"),
            Some("r1:b/src/crud.rs:FILE".to_string())
        );
    }

    #[test]
    fn rust_crate_import_canonicalisation_underscore_to_hyphen() {
        // Declared `repo-graph-storage`, imported `repo_graph_storage`.
        let roots = crate_roots(&[("repo-graph-storage", "s")]);
        let files = file_set(&["s/src/lib.rs"], "r1");
        assert_eq!(
            resolve_rust_crate_import("repo_graph_storage", &roots, &files, "r1"),
            Some("r1:s/src/lib.rs:FILE".to_string())
        );
    }

    #[test]
    fn rust_crate_import_root_crate_uses_bare_src() {
        // A `.`-rooted crate: candidates live under bare `src/`, not `./src/`.
        let roots = crate_roots(&[("thing", ".")]);
        let files = file_set(&["src/lib.rs"], "r1");
        assert_eq!(
            resolve_rust_crate_import("thing", &roots, &files, "r1"),
            Some("r1:src/lib.rs:FILE".to_string())
        );
    }

    #[test]
    fn rust_crate_import_bin_only_falls_back_to_main_rs() {
        // No lib.rs present → the entrypoint probe falls through to main.rs.
        let roots = crate_roots(&[("cli", "cli")]);
        let files = file_set(&["cli/src/main.rs"], "r1");
        assert_eq!(
            resolve_rust_crate_import("cli", &roots, &files, "r1"),
            Some("r1:cli/src/main.rs:FILE".to_string())
        );
    }

    #[test]
    fn rust_crate_import_unknown_crate_is_none() {
        // `serde` is an external dep, not a declared workspace crate → unresolved.
        let roots = crate_roots(&[("b", "b")]);
        let files = file_set(&["b/src/lib.rs"], "r1");
        assert_eq!(
            resolve_rust_crate_import("serde::Serialize", &roots, &files, "r1"),
            None
        );
    }

    #[test]
    fn rust_crate_import_unknown_module_falls_back_to_crate_root() {
        // §2.2 candidate order ENDS at the crate entrypoint: `use b::Thing` where `Thing`
        // is a type in lib.rs (target_key "b") — and equally an unknown submodule path —
        // shortens to the crate root and resolves to lib.rs. This is intended: it points
        // the agent at the right crate even when the exact module file can't be pinned.
        let roots = crate_roots(&[("b", "b")]);
        let files = file_set(&["b/src/lib.rs"], "r1");
        assert_eq!(
            resolve_rust_crate_import("b::missing", &roots, &files, "r1"),
            Some("r1:b/src/lib.rs:FILE".to_string())
        );
    }

    #[test]
    fn rust_crate_import_no_entrypoint_is_none() {
        // Declared crate, target module absent AND no lib.rs/main.rs entrypoint present →
        // genuinely unresolved (the probe exhausts every candidate).
        let roots = crate_roots(&[("b", "b")]);
        let files = file_set(&["b/src/util.rs"], "r1");
        assert_eq!(
            resolve_rust_crate_import("b::missing", &roots, &files, "r1"),
            None
        );
    }

    /// Build a `nodes_by_stable_key` map with a FILE node for each given file stable key
    /// (the stage returns a FILE key that `resolve_import_target` looks up here).
    fn file_nodes(file_keys: &[&str]) -> HashMap<String, ResolverNode> {
        file_keys
            .iter()
            .map(|k| {
                (
                    k.to_string(),
                    ResolverNode {
                        node_uid: format!("uid::{}", k),
                        stable_key: k.to_string(),
                        name: k.to_string(),
                        qualified_name: None,
                        kind: "FILE".into(),
                        subtype: None,
                        file_uid: Some(k.to_string()),
                    },
                )
            })
            .collect()
    }

    #[test]
    fn stage_3_5_gated_off_for_non_rust_imports() {
        // Review-2 item (1) / operator cycle-3 note (1): in a hybrid repo a non-Rust
        // IMPORTS edge whose leading segment matches a DECLARED Cargo crate with a real
        // candidate file must NOT resolve to that `.rs` file. The declared-crate stage is
        // gated on Rust-extractor provenance; the frozen non-Rust byte-stability invariant
        // depends on this.
        let roots = crate_roots(&[("b", "b")]);
        let files = file_set(&["b/src/util.rs", "b/src/lib.rs"], "r1");
        let nodes = file_nodes(&["r1:b/src/util.rs:FILE", "r1:b/src/lib.rs:FILE"]);

        // Non-Rust edge (is_rust_import = false): stage 3.5 does not run → unresolved,
        // even though the crate + candidate file exist.
        assert_eq!(
            resolve_import_target("b::util", &nodes, &files, "r1", None, &roots, false),
            None,
            "a non-Rust IMPORTS edge must not resolve via the declared-Rust-crate stage"
        );

        // Control: the SAME inputs from a Rust edge (is_rust_import = true) DO resolve —
        // proving the None above is the gate, not a missing fixture.
        assert_eq!(
            resolve_import_target("b::util", &nodes, &files, "r1", None, &roots, true),
            Some("uid::r1:b/src/util.rs:FILE".to_string()),
            "the identical Rust IMPORTS edge must resolve to the defining file"
        );
    }

    #[test]
    fn rust_crate_import_empty_catalog_is_noop() {
        let roots = HashMap::new();
        let files = file_set(&["b/src/lib.rs"], "r1");
        assert_eq!(resolve_rust_crate_import("b", &roots, &files, "r1"), None);
    }

    // ── Distinctive fallback branches ────────────────────────

    #[test]
    fn resolve_import_via_extensionless_file_resolution() {
        // The target_key has no extension; file_resolution maps
        // the extensionless key to the full key.
        let target_node = make_node("file1", "r1:src/utils.ts:FILE", "utils.ts", None, None);
        let mut index = ResolverIndex {
            nodes_by_stable_key: HashMap::new(),
            nodes_by_name: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
        };
        index
            .nodes_by_stable_key
            .insert("r1:src/utils.ts:FILE".into(), target_node);
        // Extensionless → with extension.
        index
            .file_resolution
            .insert("r1:src/utils:FILE".into(), "r1:src/utils.ts:FILE".into());

        let edge = make_edge("e1", "r1:src/utils:FILE", EdgeType::Imports);
        let result = resolve_edges(&[edge], &index, None);

        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].target_node_uid, "file1");
    }

    #[test]
    fn resolve_import_via_per_tu_include() {
        // C/C++ per-TU include resolution: bare header name
        // resolved through compile_commands.json include paths.
        let header_node = make_node("hdr1", "r1:include/util.h:FILE", "util.h", None, None);
        let mut index = ResolverIndex {
            nodes_by_stable_key: HashMap::new(),
            nodes_by_name: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
        };
        index
            .nodes_by_stable_key
            .insert("r1:include/util.h:FILE".into(), header_node);
        // Source node → file UID mapping.
        index
            .node_uid_to_file_uid
            .insert("src1".into(), "r1:src/main.c".into());
        // Per-TU: main.c can resolve "util.h" → "r1:include/util.h:FILE".
        let mut tu_map = HashMap::new();
        tu_map.insert("util.h".into(), "r1:include/util.h:FILE".into());
        index
            .per_file_include_resolution
            .insert("r1:src/main.c".into(), tu_map);

        let edge = make_edge("e1", "util.h", EdgeType::Imports);
        let result = resolve_edges(&[edge], &index, None);

        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].target_node_uid, "hdr1");
    }

    #[test]
    fn resolve_import_via_repo_prefix_fallback() {
        // Bare header name without per-TU resolution: the
        // repo-prefix fallback constructs "repoUid:name:FILE".
        let header_node = make_node("hdr1", "r1:util.h:FILE", "util.h", None, None);
        let mut index = ResolverIndex {
            nodes_by_stable_key: HashMap::new(),
            nodes_by_name: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
        };
        index
            .nodes_by_stable_key
            .insert("r1:util.h:FILE".into(), header_node);

        // No per-TU includes, no file_resolution entry.
        let edge = make_edge("e1", "util.h", EdgeType::Imports);
        let result = resolve_edges(&[edge], &index, None);

        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].target_node_uid, "hdr1");
    }

    #[test]
    fn resolve_call_via_import_binding_assistance() {
        // "classifyMedia" is ambiguous globally (2 functions with
        // the same name). But the source file imported it from
        // "./media", which narrows to the one in that file.
        let n1 = make_node(
            "fn_media",
            "r1:src/media.ts:classifyMedia:SYMBOL",
            "classifyMedia",
            Some("FUNCTION"),
            Some("r1:src/media.ts"),
        );
        let n2 = make_node(
            "fn_other",
            "r1:src/other.ts:classifyMedia:SYMBOL",
            "classifyMedia",
            Some("FUNCTION"),
            Some("r1:src/other.ts"),
        );
        let mut index = ResolverIndex {
            nodes_by_stable_key: HashMap::new(),
            nodes_by_name: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
        };
        index
            .nodes_by_name
            .entry("classifyMedia".into())
            .or_default()
            .extend(vec![n1, n2]);
        // Source node lives in src/index.ts.
        index
            .node_uid_to_file_uid
            .insert("src1".into(), "r1:src/index.ts".into());
        // File resolution: "./media" from src/ → src/media.ts.
        index
            .file_resolution
            .insert("r1:src/media:FILE".into(), "r1:src/media.ts:FILE".into());

        // Import binding: "classifyMedia" imported from "./media".
        let bindings: HashMap<String, Vec<ImportBinding>> = [(
            "r1:src/index.ts".into(),
            vec![ImportBinding {
                identifier: "classifyMedia".into(),
                specifier: "./media".into(),
                is_relative: true,
                location: None,
                is_type_only: false,
                imported_name: Some("classifyMedia".into()),
                kind: ImportKind::Named,
            }],
        )]
        .into_iter()
        .collect();

        let edge = make_edge("e1", "classifyMedia", EdgeType::Calls);
        let result = resolve_edges(&[edge], &index, Some(&bindings));

        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].target_node_uid, "fn_media");
    }

    #[test]
    fn aliased_named_import_uses_imported_name_for_lookup() {
        // Test: import { readFile as rf } from "./utils"; rf()
        // The callee is "rf" but the target module exports "readFile".
        // The resolver must use imported_name ("readFile") for the lookup.
        let n1 = make_node(
            "fn_readFile",
            "r1:src/utils.ts:readFile:SYMBOL",
            "readFile",
            Some("FUNCTION"),
            Some("r1:src/utils.ts"),
        );
        let mut index = ResolverIndex {
            nodes_by_stable_key: HashMap::new(),
            nodes_by_name: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
        };
        // The target module exports "readFile", NOT "rf".
        index
            .nodes_by_name
            .entry("readFile".into())
            .or_default()
            .push(n1);
        // Source node lives in src/index.ts.
        index
            .node_uid_to_file_uid
            .insert("src1".into(), "r1:src/index.ts".into());
        // File resolution: "./utils" from src/ → src/utils.ts.
        index
            .file_resolution
            .insert("r1:src/utils:FILE".into(), "r1:src/utils.ts:FILE".into());

        // Import binding: "rf" is local alias, "readFile" is original name.
        let bindings: HashMap<String, Vec<ImportBinding>> = [(
            "r1:src/index.ts".into(),
            vec![ImportBinding {
                identifier: "rf".into(), // Local alias
                specifier: "./utils".into(),
                is_relative: true,
                location: None,
                is_type_only: false,
                imported_name: Some("readFile".into()), // Original exported name
                kind: ImportKind::Named,
            }],
        )]
        .into_iter()
        .collect();

        // Edge target_key is "rf" (the local alias used in code).
        let edge = make_edge("e1", "rf", EdgeType::Calls);
        let result = resolve_edges(&[edge], &index, Some(&bindings));

        // Should resolve to "readFile" in the target module.
        assert_eq!(result.resolved.len(), 1, "aliased import should resolve");
        assert_eq!(result.resolved[0].target_node_uid, "fn_readFile");
    }

    #[test]
    fn namespace_import_member_resolves_to_target_module() {
        // Test: import * as utils from "./utils"; utils.helper()
        // The callee is "utils.helper" and the target module exports "helper".
        // The resolver must strip the namespace prefix and look up "helper"
        // in the resolved module.
        let n1 = make_node(
            "fn_helper",
            "r1:src/utils.ts:helper:SYMBOL",
            "helper",
            Some("FUNCTION"),
            Some("r1:src/utils.ts"),
        );
        let mut index = ResolverIndex {
            nodes_by_stable_key: HashMap::new(),
            nodes_by_name: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
        };
        // The target module exports "helper".
        index
            .nodes_by_name
            .entry("helper".into())
            .or_default()
            .push(n1);
        // Source node lives in src/index.ts.
        index
            .node_uid_to_file_uid
            .insert("src1".into(), "r1:src/index.ts".into());
        // File resolution: "./utils" from src/ → src/utils.ts.
        index
            .file_resolution
            .insert("r1:src/utils:FILE".into(), "r1:src/utils.ts:FILE".into());

        // Import binding: namespace import `import * as utils from "./utils"`.
        let bindings: HashMap<String, Vec<ImportBinding>> = [(
            "r1:src/index.ts".into(),
            vec![ImportBinding {
                identifier: "utils".into(),
                specifier: "./utils".into(),
                is_relative: true,
                location: None,
                is_type_only: false,
                imported_name: None, // Namespace imports have no imported_name
                kind: ImportKind::Namespace,
            }],
        )]
        .into_iter()
        .collect();

        // Edge target_key is "utils.helper" (namespace prefix + member).
        let edge = make_edge("e1", "utils.helper", EdgeType::Calls);
        let result = resolve_edges(&[edge], &index, Some(&bindings));

        // Should resolve to "helper" in the target module.
        assert_eq!(
            result.resolved.len(),
            1,
            "namespace import member should resolve"
        );
        assert_eq!(result.resolved[0].target_node_uid, "fn_helper");
    }

    #[test]
    fn default_import_member_does_not_resolve_by_bare_method_name() {
        // Test: import fs from "fs"; fs.readFile()
        // This is a default import, NOT a namespace import.
        // The resolver must NOT resolve by bare method name lookup,
        // as that would create false positives.
        //
        // Even though "readFile" exists globally, the call should NOT
        // resolve because we don't model default export structure.
        let n1 = make_node(
            "fn_readFile",
            "r1:src/utils.ts:readFile:SYMBOL",
            "readFile",
            Some("FUNCTION"),
            Some("r1:src/utils.ts"),
        );
        let mut index = ResolverIndex {
            nodes_by_stable_key: HashMap::new(),
            nodes_by_name: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
        };
        // There exists a "readFile" function globally.
        index
            .nodes_by_name
            .entry("readFile".into())
            .or_default()
            .push(n1);
        // Source node lives in src/index.ts.
        index
            .node_uid_to_file_uid
            .insert("src1".into(), "r1:src/index.ts".into());

        // Import binding: DEFAULT import `import fs from "fs"`.
        let bindings: HashMap<String, Vec<ImportBinding>> = [(
            "r1:src/index.ts".into(),
            vec![ImportBinding {
                identifier: "fs".into(),
                specifier: "fs".into(),
                is_relative: false,
                location: None,
                is_type_only: false,
                imported_name: None, // Default imports have no imported_name
                kind: ImportKind::Default,
            }],
        )]
        .into_iter()
        .collect();

        // Edge target_key is "fs.readFile" (default import member access).
        let edge = make_edge("e1", "fs.readFile", EdgeType::Calls);
        let result = resolve_edges(&[edge], &index, Some(&bindings));

        // Should NOT resolve — default import member access is conservative.
        // Even though "readFile" exists, we don't resolve to avoid false positives.
        assert_eq!(
            result.resolved.len(),
            0,
            "default import member should NOT resolve by bare method name"
        );
        assert_eq!(
            result.still_unresolved.len(),
            1,
            "default import member should remain unresolved"
        );
    }

    // ── build_per_file_include_resolution ────────────────────

    #[test]
    fn per_file_include_resolution_same_directory() {
        let paths = vec![
            "src/core/main.c".to_string(),
            "src/core/helper.c".to_string(),
            "src/core/helper.h".to_string(),
            "src/core/types.h".to_string(),
        ];
        let map = build_per_file_include_resolution(&paths, "r1");

        // main.c should have helper.h and types.h in its include map.
        let main_map = map
            .get("r1:src/core/main.c")
            .expect("main.c should be in map");
        assert_eq!(
            main_map.get("helper.h"),
            Some(&"r1:src/core/helper.h:FILE".to_string())
        );
        assert_eq!(
            main_map.get("types.h"),
            Some(&"r1:src/core/types.h:FILE".to_string())
        );

        // helper.c should also have helper.h in its include map.
        let helper_map = map
            .get("r1:src/core/helper.c")
            .expect("helper.c should be in map");
        assert_eq!(
            helper_map.get("helper.h"),
            Some(&"r1:src/core/helper.h:FILE".to_string())
        );

        // helper.h (a header) should also have types.h in its include map.
        // Headers can include other headers in the same directory.
        let header_map = map
            .get("r1:src/core/helper.h")
            .expect("helper.h should be in map");
        assert_eq!(
            header_map.get("types.h"),
            Some(&"r1:src/core/types.h:FILE".to_string())
        );
    }

    #[test]
    fn per_file_include_resolution_different_directory() {
        let paths = vec!["src/main.c".to_string(), "include/helper.h".to_string()];
        let map = build_per_file_include_resolution(&paths, "r1");

        // main.c is in src/, header is in include/ — should NOT resolve.
        // (No compile_commands.json integration in v1.)
        assert!(!map.contains_key("r1:src/main.c"));
    }
}
