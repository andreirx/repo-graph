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

use std::collections::{HashMap, HashSet};

use repo_graph_classification::canonicalize_cargo_package_name;
use repo_graph_classification::types::{
    ImportBinding, ImportKind, SourceLocation, UnresolvedEdgeCategory,
};

use crate::include_resolver::{IncludePathMatch, IncludeResolutionMap, ResolutionStatus};
use crate::storage_port::TypeOnlyDisposition;
use crate::types::{EdgeType, ExtractedEdge, Resolution};
use crate::workspace_import::{
    match_workspace_package_import, NpmWorkspacePackages, WorkspaceImportMatch,
    AMBIGUOUS_WORKSPACE_SOURCE_ENTRY_BASIS, WORKSPACE_SOURCE_ENTRY_BASIS,
};
use repo_graph_import_resolver::TsconfigAliasHits;

use crate::tsconfig_paths_import::{
    TsconfigPathsIndex, AMBIGUOUS_TSCONFIG_PATHS_BASIS, TIED_TSCONFIG_PATHS_PATTERNS_BASIS,
    TSCONFIG_PATHS_BASIS,
};

/// Provenance prefix of the Rust extractor (`ExtractedEdge.extractor`), whose
/// value is `rust-core:<version>` (see `rust-extractor::EXTRACTOR_NAME`). The
/// declared-Rust-crate import stage is gated to edges carrying this prefix so a
/// non-Rust IMPORTS edge cannot resolve to a `.rs` file. Prefix (not the exact
/// version string) so the gate survives extractor version bumps. The indexer does
/// not depend on the rust-extractor crate (no dependency edge); the string is the
/// stable cross-boundary contract already present on every extracted edge.
const RUST_EXTRACTOR_PREFIX: &str = "rust-core:";

/// Provenance prefix of the Java extractor (`ExtractedEdge.extractor`), whose value is
/// `java-core:<version>` (see `java-extractor::EXTRACTOR_NAME`). The declared-Java-import
/// suffix stage (IMPORT-RESOLUTION-JAVA-1) is gated to edges carrying this prefix so a
/// non-Java IMPORTS edge (a TS/Python/Rust dotted specifier) can never resolve to a `.java`
/// file — the frozen non-Java byte-stability invariant. Prefix (not the exact version) so the
/// gate survives extractor version bumps. The indexer does not depend on the java-extractor
/// crate; the string is the stable cross-boundary contract already on every extracted edge.
const JAVA_EXTRACTOR_PREFIX: &str = "java-core:";

/// Provenance prefix of the Python extractor (`ExtractedEdge.extractor`), whose value is
/// `python-core:<version>` (see `python-extractor::EXTRACTOR_NAME`). The package-submodule import
/// stage (PYTHON-SUBMODULE-IMPORT-1, RG-REQ-006-L04) is gated to edges carrying this prefix so a
/// non-Python IMPORTS edge that happens to carry an `importedName` (a TS named import) never takes
/// it. Prefix (not the exact version) so the gate survives extractor version bumps; the indexer
/// does not depend on the python-extractor crate — the string is the stable cross-boundary
/// provenance already on every extracted edge.
const PYTHON_EXTRACTOR_PREFIX: &str = "python-core:";

/// Provenance prefix of the TS/JS extractor (`ts-core:<version>`). TS-WORKSPACE-RESOLUTION-1: the
/// workspace member import stage is gated to edges carrying it, so no other language's import can
/// bind to a workspace package's source entry.
const TS_EXTRACTOR_PREFIX: &str = "ts-core:";

/// PYTHON-RECEIVER-BINDING-1 (D-PRB-CARRIER-1 as corrected 2026-09-24): the exact extractor
/// version that wrote self/cls call carriers WITHOUT the `receiverBinding` key. A carrier lacking
/// the key is read on the legacy path (HEAD's stage order, terminal hierarchy miss) ONLY when its
/// edge was written by exactly this writer — every pre-slice Python snapshot, copied forward by a
/// delta refresh. A missing key from any other writer (the current extractor always writes it) is
/// unproven. Compared EXACTLY, never by prefix.
const PYTHON_EXTRACTOR_BEFORE_RECEIVER_BINDING: &str = "python-core:0.1.0";

/// PYTHON-RECEIVER-BINDING-1 (RG-REQ-005-L02, RG-REQ-002-L11): the `basis` stamped on a Python
/// CALLS edge bound by its method name alone on an untyped receiver — the reason it is `inferred`.
pub const RECEIVER_UNTYPED_NAME_ONLY_BASIS: &str = "receiver_untyped_name_only";

/// PYTHON-RECEIVER-BINDING-1: the `nameOnlyReason` values the resolver writes beside a declined
/// candidate pool (`nameOnlyCandidates`) on an unresolved CALLS row. The classifier reads the three
/// self/cls reasons to assign their basis; `ambiguous_name` keeps the row's existing verdict.
pub const NAME_ONLY_REASON_AMBIGUOUS: &str = "ambiguous_name";
pub const NAME_ONLY_REASON_SELF_CALL_HIERARCHY_MISS: &str = "self_call_hierarchy_miss";
pub const NAME_ONLY_REASON_SELF_CALL_WITHOUT_CLASS_CONTEXT: &str =
    "self_call_without_class_context";
pub const NAME_ONLY_REASON_SELF_CALL_RECEIVER_UNPROVEN: &str = "self_call_receiver_unproven";

/// PYTHON-SUBMODULE-IMPORT-1: the `basis` stamped on an IMPORTS edge the Python submodule stage
/// retargeted (D-CERTAINTY-MARK-1's word; the reason the edge is `inferred`, not `static`).
pub const PYTHON_SUBMODULE_BASIS: &str = "python_submodule";

/// CPP-INCLUDE-BASENAME-1 (RG-REQ-006-L11, RG-REQ-002-L11): the `basis` values the C/C++ include
/// suffix/basename stage writes into an include's `metadata_json`, beside `candidates` (the FILE
/// stable keys it matched). `unique_suffix` rides a `static` edge; `unique_basename` an `inferred`
/// edge; the two ambiguous values an unresolved `imports_ambiguous_match` row — never an edge.
pub const INCLUDE_UNIQUE_SUFFIX_BASIS: &str = "unique_suffix";
pub const INCLUDE_UNIQUE_BASENAME_BASIS: &str = "unique_basename";
pub const INCLUDE_AMBIGUOUS_SUFFIX_BASIS: &str = "ambiguous_suffix";
pub const INCLUDE_AMBIGUOUS_BASENAME_BASIS: &str = "ambiguous_basename";

/// Provenance prefixes of the C and C++ extractors (`ExtractedEdge.extractor`), whose
/// values are `c-core:<version>` / `cpp-core:<version>` (see the two extractors'
/// `EXTRACTOR_NAME`). CPP-DECLARATORS-1 §2.6 (operator ruling A, 2026-09-06): an
/// `Implements` edge from THESE extractors is C/C++ inheritance (`: public Base`), whose
/// base is a CLASS/STRUCT node — so its affinity filter admits CLASS/STRUCT, NOT the
/// interface-only rule every other language keeps. Gated by prefix (not exact version) so
/// the gate survives version bumps; the indexer takes no dependency on either extractor
/// crate — the string is the stable cross-boundary provenance already on every edge.
const C_EXTRACTOR_PREFIX: &str = "c-core:";
const CPP_EXTRACTOR_PREFIX: &str = "cpp-core:";

/// Was this edge written by the C or C++ extractor? Gates the C/C++ inheritance `Implements`
/// affinity (CPP-DECLARATORS-1 spec §2.6) and the include suffix/basename stage
/// (CPP-INCLUDE-BASENAME-1).
fn is_c_family_extractor(extractor: &str) -> bool {
    extractor.starts_with(C_EXTRACTOR_PREFIX) || extractor.starts_with(CPP_EXTRACTOR_PREFIX)
}

/// CPP-DECLARATORS-1 §2.3: classification of a node's stored `forward_decl` metadata.
///
/// Mirrors the `TypeOnlyDisposition` honesty pattern (operator ruling 2026-09-03 item 2a):
/// a CORRUPT carrier is a DISTINCT truth from an absent one and is NEVER collapsed into the
/// same branch as a definition (STANDING HONESTY RULE 1 — never swallow a fallible read whose
/// result is classified). `serde_json::from_str(...).ok()` did exactly that and is forbidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForwardDeclRead {
    /// `metadata_json` carries `forward_decl: true` — a bodiless declaration.
    ForwardDecl,
    /// No carrier, valid JSON without the key, or `forward_decl: false` — a definition
    /// (a KNOWN, readable fact: this node is not a forward declaration).
    Definition,
    /// The carrier was PRESENT but UNREADABLE: `metadata_json` did not parse as JSON, or its
    /// `forward_decl` value was present but not a boolean. A NAMED error, DISTINCT from
    /// `Definition` — a node whose forward-decl fact cannot be read is never assumed to be a
    /// definition.
    Unreadable,
}

/// CPP-DECLARATORS-1 §2.3 (AMENDED 2026-09-07): classify the stored `metadata_json.forward_decl`
/// of a node WITHOUT swallowing a parse error. `Unreadable` is a named outcome, never folded into
/// `Definition`.
pub fn classify_forward_decl(metadata_json: Option<&str>) -> ForwardDeclRead {
    // No carrier at all ⇒ the fact is ABSENT, which for forward_decl means a definition
    // (only forward declarations stamp the key). This is a KNOWN read, not a swallowed error.
    let Some(raw) = metadata_json else {
        return ForwardDeclRead::Definition;
    };
    // A carrier that does not parse is CORRUPT — NAMED, not silently a definition.
    let value: serde_json::Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return ForwardDeclRead::Unreadable,
    };
    match value.get("forward_decl") {
        // Valid JSON without the key ⇒ the node was never stamped ⇒ a definition.
        None => ForwardDeclRead::Definition,
        Some(serde_json::Value::Bool(true)) => ForwardDeclRead::ForwardDecl,
        Some(serde_json::Value::Bool(false)) => ForwardDeclRead::Definition,
        // Key present but not a boolean ⇒ a CORRUPT value, distinct from a definition.
        Some(_) => ForwardDeclRead::Unreadable,
    }
}

/// CPP-DECLARATORS-1 §2.3: does this node's stored metadata mark it a bodiless DECLARATION?
///
/// Shared by the resolver index build (`storage::query_resolver_nodes`) so
/// `ResolverNode.forward_decl` is derived once from the same rule everywhere. Built on
/// [`classify_forward_decl`], so a malformed carrier is NEVER treated as a definition:
/// `Unreadable` collapses to `true` (a declaration), the conservative direction — a node whose
/// forward-decl fact cannot be read is filtered/demoted before the singleton test and can never
/// masquerade as the authoritative definition (STANDING HONESTY RULE 1). `false` for every
/// non-C++ node (their extractors set no such key ⇒ `Definition`).
pub fn metadata_forward_decl(metadata_json: Option<&str>) -> bool {
    match classify_forward_decl(metadata_json) {
        ForwardDeclRead::ForwardDecl => true,
        ForwardDeclRead::Definition => false,
        ForwardDeclRead::Unreadable => true,
    }
}

/// RUST-SELF-RESOLUTION-1 (D-RSR-ENTRY-CLIMB-1): one external `mod <name>;` declaration a Rust
/// FILE node records under `metadata_json.rust_mod_decls` (written by the Rust extractor's
/// `mod_decls.rs`): the module `name` and the `inline_path` — the names of the inline
/// `mod <x> { }` bodies the declaration is written in, outermost first, empty at the file's top
/// level. Stage 3.6 reads it as the declaration evidence a sibling path needs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RustModDeclaration {
    pub name: String,
    pub inline_path: Vec<String>,
}

/// RUST-SELF-RESOLUTION-1: each Rust FILE node's declarations ([`RustModDeclaration`]), keyed by
/// the FILE node's file uid (`<repo_uid>:<path>`). Built by the orchestrator from the stored FILE
/// nodes through [`rust_mod_decls_from_metadata`]; a file absent from the map declares nothing.
pub type RustModDeclsByFile = HashMap<String, HashSet<RustModDeclaration>>;

/// RUST-SELF-RESOLUTION-1: read a FILE node's `metadata_json.rust_mod_decls`. No carrier, no key,
/// a carrier that is not JSON, or a key that is not a list → no declarations; an entry without a
/// string `name`, or whose `inline_path` is present but not a list of strings, is skipped. The
/// value only GATES stage 3.6's sibling reading, so an unreadable fact can only leave a row
/// unresolved with its classification (the state it had before this slice), never bind it.
pub fn rust_mod_decls_from_metadata(metadata_json: Option<&str>) -> HashSet<RustModDeclaration> {
    let Some(raw) = metadata_json else {
        return HashSet::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return HashSet::new();
    };
    let Some(entries) = value.get("rust_mod_decls").and_then(|v| v.as_array()) else {
        return HashSet::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let name = entry.get("name")?.as_str()?.to_string();
            let inline_path = match entry.get("inline_path") {
                None => Vec::new(),
                Some(list) => list
                    .as_array()?
                    .iter()
                    .map(|v| v.as_str().map(str::to_string))
                    .collect::<Option<Vec<String>>>()?,
            };
            Some(RustModDeclaration { name, inline_path })
        })
        .collect()
}

/// PYTHON-SELF-BINDING-1 (RG-REQ-005-L03): parse a Python CLASS node's base-class SIMPLE names
/// from its stored `metadata_json.superclass` — the raw parenthesised base text the Python
/// extractor stamps (`"base.BaseHandler"`, `"Base, metaclass=ABCMeta"`, `"Generic[T]"`).
///
/// The transform, per base: split the list on `,`, trim; DROP any part carrying `=` (a keyword
/// argument such as `metaclass=ABCMeta`, never a base); strip a `[...]` subscript suffix
/// (`Generic[T]` → `Generic`); keep the LAST dotted segment (`base.BaseHandler` → `BaseHandler`).
/// A residue that is not a plain identifier is dropped (never a guess that could match a class by
/// accident). Absent or unreadable metadata → empty. Empty for every non-Python node — only the
/// Python extractor stamps `superclass` (this is a KNOWN read of an absent fact, not a swallowed
/// error: the value is used ONLY to widen the self-call hierarchy walk, never persisted as a
/// classified fact, so a missing base can only MISS a binding, never mis-bind).
pub fn superclasses_from_metadata(metadata_json: Option<&str>) -> Vec<String> {
    let Some(raw) = metadata_json else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Vec::new();
    };
    let Some(superclass) = value.get("superclass").and_then(|v| v.as_str()) else {
        return Vec::new();
    };
    superclass
        .split(',')
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() || part.contains('=') {
                return None;
            }
            // Strip a subscript (`Generic[T]` → `Generic`), then keep the last dotted segment.
            let base = part.split('[').next().unwrap_or(part).trim();
            let simple = base.rsplit('.').next().unwrap_or(base).trim();
            if simple.is_empty() || !simple.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return None;
            }
            Some(simple.to_string())
        })
        .collect()
}

// ── Resolution outcome ───────────────────────────────────────────

/// Result of attempting to resolve an edge target.
enum TargetResolution {
    /// Target resolved to a node UID.
    Resolved(String),
    /// Target could not be resolved.
    Unresolved,
    /// Multiple candidates matched exactly (C/C++ include ambiguity).
    Ambiguous(Vec<String>),
    /// IMPORT-RESOLUTION-JAVA-1: a Java `import pkg.*` wildcard — names a package, not a single
    /// type, so it has no single target file. Stays unresolved with the NAMED, COUNTED basis
    /// `ImportsWildcard` (never silently mis-resolved).
    JavaWildcard,
    /// IMPORT-RESOLUTION-JAVA-1: a Java FQN import whose path suffix matched MORE THAN ONE
    /// indexed `.java` file (shaded/duplicated copies). Stays unresolved with the NAMED, COUNTED
    /// basis `ImportsAmbiguousSuffix`.
    JavaAmbiguousSuffix,
    /// PYTHON-SELF-BINDING-1 (RG-REQ-005-L03 / RG-REQ-001-L03): a Python `self.<m>()` / `cls.<m>()`
    /// whose method is declared at ONE breadth-first depth of the caller's own-class superclass
    /// closure by TWO OR MORE ancestors — a genuine MRO ambiguity. The call stays UNRESOLVED
    /// (never a silent pick of Python's actual MRO winner) with the candidate `<Class>.<method>`
    /// labels carried as persisted evidence: `resolve_edges` stamps them as `mroCandidates` on the
    /// edge's `metadata_json` and keeps the category (`calls_obj_method_needs_type_info`). The
    /// classifier reads `mroCandidates` and assigns the named basis `self_call_ambiguous_mro`.
    SelfCallAmbiguousMro(Vec<String>),
    /// PYTHON-SUBMODULE-IMPORT-1 (RG-REQ-006-L04 Python clause; RG-REQ-002-L11): a Python
    /// `from X import Y` whose package init shows no binding of `Y` while `X/Y/__init__.py` or
    /// `X/Y.py` is indexed. Python binds `Y` at run time (the init runs first; only when it leaves
    /// no attribute `Y` is the submodule loaded), and the extractor cannot see every way an init
    /// binds a name (multi-target assignment, `__getattr__`, star imports) — so the edge lands on
    /// the submodule file as INFERRED, never `static`, with the package init recorded as the
    /// alternate candidate and the reason as `basis`. `resolve_edges` writes `resolution:
    /// Inferred`, merges `alternateTarget`/`basis` into the edge's `metadata_json`, and does NOT
    /// feed the edge to `resolved_import_pairs` (an inferred import is never a certain module edge).
    InferredImport {
        /// Node uid of the submodule FILE the edge now targets.
        target_uid: String,
        /// Stable key of the other candidate — the package init the ladder bound.
        alternate_stable_key: String,
        /// Why the edge is inferred (`PYTHON_SUBMODULE_BASIS`).
        basis: &'static str,
    },
    /// CPP-INCLUDE-BASENAME-1 (RG-REQ-006-L11): a C/C++ include no earlier stage resolved, bound
    /// by the include suffix/basename stage to its one indexed candidate — `static` on a unique
    /// path suffix (fed to `resolved_import_pairs` like any certain import), `inferred` on a unique
    /// basename (kept out of them, RG-REQ-002-L11). `metadata_json` is the extractor's carrier with
    /// `basis` and `candidates` merged in.
    IncludeBound {
        target_uid: String,
        resolution: Resolution,
        metadata_json: String,
    },
    /// CPP-INCLUDE-BASENAME-1: a C/C++ include whose path suffix or basename matches several
    /// indexed files. Unresolved as `ImportsAmbiguousMatch`, never a pick; `metadata_json` carries
    /// the ambiguous basis and every candidate.
    IncludeAmbiguous { metadata_json: String },
    /// TS-WORKSPACE-RESOLUTION-1 (RG-REQ-006-L04, RG-REQ-002-L11): a bare TS import naming a
    /// workspace member whose declared entries are not indexed, bound to the member's one indexed
    /// source entry. Always `inferred` (the index cannot show the build maps the source entry to
    /// the declared one), never fed to `resolved_import_pairs`; `metadata_json` is the extractor's
    /// carrier with `basis` and `candidates` (the source entry, then every declared target).
    WorkspaceSourceEntryBound {
        target_uid: String,
        metadata_json: String,
    },
    /// TS-WORKSPACE-RESOLUTION-1: the member has several indexed source entries. Unresolved with the
    /// row's own category, never a pick; `metadata_json` carries the ambiguous basis and every
    /// candidate.
    WorkspaceSourceEntryAmbiguous { metadata_json: String },
    /// TS-ALIAS-RESOLUTION-1 (RG-REQ-006-L04; D-TSA-RECORD-CONFLICT-1): a TS import whose source
    /// file's stored `paths` mapping (that of its sole inspected covering tsconfig project) selects
    /// exactly one indexed file. A STATIC edge — static
    /// asserting the language's resolution (what the compiler binds), never a runtime proof — fed
    /// to `resolved_import_pairs` like any certain import; `metadata_json` is the extractor's
    /// carrier with `basis: tsconfig_paths` and the target as its one candidate.
    TsconfigPathsBound {
        target_uid: String,
        metadata_json: String,
    },
    /// TS-ALIAS-RESOLUTION-1 (RG-REQ-002-L11): the mapping's selection reaches several indexed
    /// files, or patterns tied for the longest prefix reach one or more. Unresolved with the row's
    /// own category, never a pick; `metadata_json` carries the basis that names which of the two
    /// happened and every candidate.
    TsconfigPathsAmbiguous { metadata_json: String },
    /// PYTHON-RECEIVER-BINDING-1 (RG-REQ-005-L02 / RG-REQ-002-L11): a Python `obj.m()` on an
    /// untyped receiver whose method name has exactly ONE candidate. The name alone is not
    /// evidence, so the edge is written `resolution: inferred` with its basis, its receiver text and
    /// its pool of one — never `static`.
    InferredNameOnly {
        /// Node uid of the single candidate.
        uid: String,
        /// The receiver expression (the key minus its last `.`-segment).
        receiver: String,
        /// The candidate's qualified name (the recorded pool of one).
        candidate: String,
    },
    /// PYTHON-RECEIVER-BINDING-1 (RG-REQ-002-L11): a Python `obj.m()` on an untyped receiver whose
    /// method name has TWO OR MORE candidates. Unresolved; every candidate is recorded with the
    /// reason `ambiguous_name`; the category and the classifier's verdict are unchanged.
    AmbiguousNameOnly(Vec<String>),
    /// PYTHON-RECEIVER-BINDING-1 (RG-REQ-005-L03, D-PRB-SCOPE-1 amendment 2): a `self.`/`cls.` call
    /// whose class-hierarchy walk found no declaration of the method (or whose own class is not
    /// unique in the caller's file). TERMINAL — never the bare-name fallback; the bare-name pool is
    /// recorded as `nameOnlyCandidates` with the reason `self_call_hierarchy_miss`.
    SelfCallHierarchyMiss(Vec<String>),
    /// PYTHON-RECEIVER-BINDING-1 (D-PRB-CARRIER-1): a class-contained `self.`/`cls.` call whose
    /// receiver the extractor could not prove to be the method's first parameter (or whose proof is
    /// unreadable, or whose missing proof comes from a writer other than `python-core:0.1.0`).
    /// Neither the namespace nor the hierarchy stage binds it; every candidate is recorded with the
    /// reason `self_call_receiver_unproven`.
    SelfCallReceiverUnproven(Vec<String>),
    /// PYTHON-RECEIVER-BINDING-1 (F-PRB-SELF-NOCARRIER): a Python `self.`/`cls.` call with no
    /// readable self-call carrier (a `self` parameter of a module-level function; a malformed
    /// carrier) that no file-level namespace alias binds. No class could be walked, so the
    /// bare-name pool is not evidence; recorded with the reason `self_call_without_class_context`.
    SelfCallWithoutClassContext(Vec<String>),
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
    /// CPP-DECLARATORS-1 (spec §2.3): the node is a bodiless DECLARATION — a C++ type
    /// forward-decl (`class X;`) or an in-class method prototype — read from the stored
    /// `metadata_json.forward_decl`. A declaration is filtered out BEFORE the singleton
    /// test in [`pick_unambiguous`], so a class declared many times + defined once
    /// resolves to its definition, and the header prototype no longer makes a call
    /// ambiguous. `false` for every non-C++ node (their extractors set no such key).
    pub forward_decl: bool,
    /// PYTHON-SELF-BINDING-1 (RG-REQ-005-L03): a Python CLASS node's base-class SIMPLE names,
    /// derived once from the stored `metadata_json.superclass` by [`superclasses_from_metadata`].
    /// Used ONLY by the Python self/cls call hierarchy walk (`resolve_self_call`); empty for every
    /// non-Python node (only the Python extractor stamps `superclass`) and for a class with no
    /// bases. Superclass EDGES are not modelled for Python yet — this simple-name list is the
    /// substrate the walk uses, resolving each name to a unique CLASS node by simple name.
    pub superclasses: Vec<String>,
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
    /// PYTHON-SELF-BINDING-1 (RG-REQ-005-L03): qualified name → all nodes with that qualified
    /// name. Populated beside `nodes_by_name` at index build. One current consumer — the Python
    /// self/cls call hierarchy walk (`resolve_self_call`), which looks up `<Class>.<method>`
    /// method nodes directly. A plain map, not an abstraction; the singular by-name map cannot
    /// answer "the method named `m` DECLARED on class `C`" without a per-lookup scan.
    pub nodes_by_qualified_name: HashMap<String, Vec<ResolverNode>>,
    /// Node UID → node. CPP-DECLARATORS-1 §2.3 (AMENDED): the enclosing-class preference
    /// needs the CALLER node's `qualified_name` from its `source_node_uid`, and no other
    /// map is keyed by uid (`stable_key_to_uid` is the reverse). One concrete current user
    /// (`resolve_call_target`'s C/C++ enclosing-class step); a plain map, not an
    /// abstraction. Rejected simpler: inverting `stable_key_to_uid` per call — O(n) in the
    /// resolution hot path.
    pub nodes_by_uid: HashMap<String, ResolverNode>,
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
    /// IMPORT-RESOLUTION-JAVA-1 §2.1: the Java suffix index — every indexed `.java` file's
    /// basename → the repo-relative paths of files with that basename. Built once per index from
    /// the same file list stages 1-4 already use (`build_java_suffix_index`), needs no
    /// source-root knowledge. A Java FQN import `a.b.C` is resolved by finding the file whose
    /// path ends at a directory boundary with the suffix `a/b/C.java` (shortening trailing
    /// segments for nested classes / static members). Empty when no `.java` files were indexed →
    /// the Java import stage is a no-op.
    pub java_suffix_index: JavaSuffixIndex,
    /// TS-WORKSPACE-RESOLUTION-1 (RG-REQ-006-L04): npm workspace package name → every member that
    /// declares it (root, declared entries). Built once by the orchestrator from
    /// `IndexOptions.declared_modules` (npm ecosystem only,
    /// `workspace_import::build_npm_workspace_packages`). Read only by the trailing TS-only workspace
    /// import stage; empty → the stage is a no-op.
    pub npm_workspace_packages: NpmWorkspacePackages,
    /// TS-ALIAS-RESOLUTION-1 (RG-REQ-006-L04): each file's one tsconfig `paths` mapping (the
    /// mapping of the sole inspected project covering it) and the FILE inventory. Built once by the
    /// orchestrator from the stored alias signals; read only by the TS-only `paths` stage; empty →
    /// the stage is a no-op.
    pub tsconfig_paths: TsconfigPathsIndex,
}

/// IMPORT-RESOLUTION-JAVA-1 §2.1: the Java suffix index. Keyed by file basename (e.g.
/// `"Util.java"`), value = repo-relative paths of every indexed `.java` file with that basename.
///
/// - what: the pure lookup structure for the Java FQN-import resolution stage. One current
///   producer (`build_java_suffix_index`, called from the orchestrator's resolver-index build)
///   and one current consumer (`resolve_java_suffix`).
/// - axis of variation: none introduced — a plain map, not an abstraction. Basename-bucketed
///   (rather than a full suffix→file map) so a lookup scans only the handful of files sharing a
///   class name, and memory is one entry per file instead of one per path-tail. Identical match
///   semantics to a suffix map; smaller and clearer.
/// - rejected simpler: reusing `file_resolution` (keyed by full stable key, no suffix matching) —
///   a Java import carries a package-qualified name, never a repo-relative path, so no stable-key
///   or extensionless lookup can ever hit it (exactly the D1/§A root cause).
pub type JavaSuffixIndex = HashMap<String, Vec<String>>;

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
    crate::type_only::type_only_disposition_of(edge.metadata_json.as_deref())
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
    resolve_edges_with_rust_mod_decls(
        edges,
        index,
        import_bindings_by_file,
        &RustModDeclsByFile::new(),
    )
}

/// [`resolve_edges`] with each Rust FILE node's `mod` declarations (RUST-SELF-RESOLUTION-1,
/// D-RSR-ENTRY-CLIMB-1): stage 3.6 binds a sibling path only when the importing file declares its
/// first segment. The orchestrator calls this form; [`resolve_edges`] passes no declarations, so
/// under it no sibling path binds.
pub fn resolve_edges_with_rust_mod_decls(
    edges: &[ExtractedEdge],
    index: &ResolverIndex,
    import_bindings_by_file: Option<&HashMap<String, Vec<ImportBinding>>>,
    rust_mod_decls: &RustModDeclsByFile,
) -> ResolutionResult {
    let mut resolved = Vec::new();
    let mut still_unresolved = Vec::new();
    let mut resolved_import_pairs = Vec::new();

    for edge in edges {
        let resolution_outcome =
            resolve_target(edge, index, import_bindings_by_file, rust_mod_decls);

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
            // IMPORT-RESOLUTION-JAVA-1: the two NAMED Java-import failure bases. Their category
            // is set HERE (not via `categorize_unresolved_edge`, which defaults imports to
            // `ImportsFileNotFound`) so the limitation is stated and COUNTED, never mis-resolved.
            TargetResolution::JavaWildcard => {
                let source_file_uid = index
                    .node_uid_to_file_uid
                    .get(&edge.source_node_uid)
                    .cloned();
                still_unresolved.push(CategorizedUnresolvedEdge {
                    edge: edge.clone(),
                    category: UnresolvedEdgeCategory::ImportsWildcard,
                    source_file_uid,
                });
            }
            TargetResolution::JavaAmbiguousSuffix => {
                let source_file_uid = index
                    .node_uid_to_file_uid
                    .get(&edge.source_node_uid)
                    .cloned();
                still_unresolved.push(CategorizedUnresolvedEdge {
                    edge: edge.clone(),
                    category: UnresolvedEdgeCategory::ImportsAmbiguousSuffix,
                    source_file_uid,
                });
            }
            // PYTHON-SELF-BINDING-1 (RG-REQ-001-L03): a declined MRO collision. The edge stays
            // UNRESOLVED with its category unchanged; its candidates ride into `metadata_json` as
            // `mroCandidates` (the first resolver-written metadata key, beside the extractor's
            // self-call carrier), which the classifier reads to assign `self_call_ambiguous_mro`.
            TargetResolution::SelfCallAmbiguousMro(candidates) => {
                let source_file_uid = index
                    .node_uid_to_file_uid
                    .get(&edge.source_node_uid)
                    .cloned();
                let mut unresolved_edge = edge.clone();
                unresolved_edge.metadata_json = Some(inject_mro_candidates(
                    unresolved_edge.metadata_json.as_deref(),
                    &candidates,
                ));
                let category = categorize_unresolved_edge(&unresolved_edge);
                still_unresolved.push(CategorizedUnresolvedEdge {
                    edge: unresolved_edge,
                    category,
                    source_file_uid,
                });
            }
            // PYTHON-SUBMODULE-IMPORT-1: an inferred import. Resolved to the submodule file, marked
            // `inferred` with both candidates and the reason on the edge (RG-REQ-002-L11), and kept
            // OUT of `resolved_import_pairs`, so the persisted MODULE→MODULE graph is fed only by
            // certain imports.
            TargetResolution::InferredImport {
                target_uid,
                alternate_stable_key,
                basis,
            } => {
                resolved.push(ResolvedEdge {
                    edge_uid: edge.edge_uid.clone(),
                    snapshot_uid: edge.snapshot_uid.clone(),
                    repo_uid: edge.repo_uid.clone(),
                    source_node_uid: edge.source_node_uid.clone(),
                    target_node_uid: target_uid,
                    edge_type: edge.edge_type,
                    resolution: Resolution::Inferred,
                    extractor: edge.extractor.clone(),
                    location: edge.location,
                    metadata_json: Some(inject_inferred_import_candidates(
                        edge.metadata_json.as_deref(),
                        &alternate_stable_key,
                        basis,
                    )),
                });
            }
            // CPP-INCLUDE-BASENAME-1: a bound include. Its resolution comes from the stage
            // (`static` on a unique suffix, `inferred` on a unique basename); only a certain one
            // feeds the persisted MODULE→MODULE graph.
            TargetResolution::IncludeBound {
                target_uid,
                resolution,
                metadata_json,
            } => {
                if resolution != Resolution::Inferred {
                    resolved_import_pairs.push((
                        edge.source_node_uid.clone(),
                        target_uid.clone(),
                        import_edge_type_only(edge),
                    ));
                }
                resolved.push(ResolvedEdge {
                    edge_uid: edge.edge_uid.clone(),
                    snapshot_uid: edge.snapshot_uid.clone(),
                    repo_uid: edge.repo_uid.clone(),
                    source_node_uid: edge.source_node_uid.clone(),
                    target_node_uid: target_uid,
                    edge_type: edge.edge_type,
                    resolution,
                    extractor: edge.extractor.clone(),
                    location: edge.location,
                    metadata_json: Some(metadata_json),
                });
            }
            // CPP-INCLUDE-BASENAME-1: an ambiguous suffix or basename. Counted with the include-root
            // overlap under the one C/C++ ambiguity category; the carrier says which shape.
            TargetResolution::IncludeAmbiguous { metadata_json } => {
                let source_file_uid = index
                    .node_uid_to_file_uid
                    .get(&edge.source_node_uid)
                    .cloned();
                let mut unresolved_edge = edge.clone();
                unresolved_edge.metadata_json = Some(metadata_json);
                still_unresolved.push(CategorizedUnresolvedEdge {
                    edge: unresolved_edge,
                    category: UnresolvedEdgeCategory::ImportsAmbiguousMatch,
                    source_file_uid,
                });
            }
            // TS-WORKSPACE-RESOLUTION-1: an inferred workspace import. Kept OUT of
            // `resolved_import_pairs`: the persisted MODULE→MODULE graph is fed by certain imports.
            TargetResolution::WorkspaceSourceEntryBound {
                target_uid,
                metadata_json,
            } => {
                resolved.push(ResolvedEdge {
                    edge_uid: edge.edge_uid.clone(),
                    snapshot_uid: edge.snapshot_uid.clone(),
                    repo_uid: edge.repo_uid.clone(),
                    source_node_uid: edge.source_node_uid.clone(),
                    target_node_uid: target_uid,
                    edge_type: edge.edge_type,
                    resolution: Resolution::Inferred,
                    extractor: edge.extractor.clone(),
                    location: edge.location,
                    metadata_json: Some(metadata_json),
                });
            }
            // TS-WORKSPACE-RESOLUTION-1: several source entries. The row keeps the category it has
            // without the stage; only its carrier gains the pool.
            TargetResolution::WorkspaceSourceEntryAmbiguous { metadata_json } => {
                let category = categorize_unresolved_edge(edge);
                let source_file_uid = index
                    .node_uid_to_file_uid
                    .get(&edge.source_node_uid)
                    .cloned();
                let mut unresolved_edge = edge.clone();
                unresolved_edge.metadata_json = Some(metadata_json);
                still_unresolved.push(CategorizedUnresolvedEdge {
                    edge: unresolved_edge,
                    category,
                    source_file_uid,
                });
            }
            // TS-ALIAS-RESOLUTION-1: a static `paths` binding — a certain import, so it feeds the
            // persisted MODULE→MODULE graph like any static import.
            TargetResolution::TsconfigPathsBound {
                target_uid,
                metadata_json,
            } => {
                resolved_import_pairs.push((
                    edge.source_node_uid.clone(),
                    target_uid.clone(),
                    import_edge_type_only(edge),
                ));
                resolved.push(ResolvedEdge {
                    edge_uid: edge.edge_uid.clone(),
                    snapshot_uid: edge.snapshot_uid.clone(),
                    repo_uid: edge.repo_uid.clone(),
                    source_node_uid: edge.source_node_uid.clone(),
                    target_node_uid: target_uid,
                    edge_type: edge.edge_type,
                    resolution: Resolution::Static,
                    extractor: edge.extractor.clone(),
                    location: edge.location,
                    metadata_json: Some(metadata_json),
                });
            }
            // TS-ALIAS-RESOLUTION-1: several (or tied) `paths` hits. The row keeps the category it
            // has without the stage; only its carrier gains the basis and every candidate.
            TargetResolution::TsconfigPathsAmbiguous { metadata_json } => {
                let category = categorize_unresolved_edge(edge);
                let source_file_uid = index
                    .node_uid_to_file_uid
                    .get(&edge.source_node_uid)
                    .cloned();
                let mut unresolved_edge = edge.clone();
                unresolved_edge.metadata_json = Some(metadata_json);
                still_unresolved.push(CategorizedUnresolvedEdge {
                    edge: unresolved_edge,
                    category,
                    source_file_uid,
                });
            }
            // PYTHON-RECEIVER-BINDING-1: a name-only binding on an untyped receiver. Resolved to
            // the single candidate but marked INFERRED with its basis, receiver and pool of one
            // (RG-REQ-005-L02, RG-REQ-002-L11) — overriding the extractor's `static`.
            TargetResolution::InferredNameOnly {
                uid,
                receiver,
                candidate,
            } => {
                resolved.push(ResolvedEdge {
                    edge_uid: edge.edge_uid.clone(),
                    snapshot_uid: edge.snapshot_uid.clone(),
                    repo_uid: edge.repo_uid.clone(),
                    source_node_uid: edge.source_node_uid.clone(),
                    target_node_uid: uid,
                    edge_type: edge.edge_type,
                    resolution: Resolution::Inferred,
                    extractor: edge.extractor.clone(),
                    location: edge.location,
                    metadata_json: Some(inject_name_only_binding(
                        edge.metadata_json.as_deref(),
                        &receiver,
                        &candidate,
                    )),
                });
            }
            // PYTHON-RECEIVER-BINDING-1: every declined candidate pool rides into the unresolved
            // row as `nameOnlyCandidates` with its `nameOnlyReason` (ONE recording rule). The
            // category stays `categorize_unresolved_edge`'s.
            TargetResolution::AmbiguousNameOnly(pool) => still_unresolved.push(declined_pool_row(
                edge,
                index,
                &pool,
                NAME_ONLY_REASON_AMBIGUOUS,
            )),
            TargetResolution::SelfCallHierarchyMiss(pool) => {
                still_unresolved.push(declined_pool_row(
                    edge,
                    index,
                    &pool,
                    NAME_ONLY_REASON_SELF_CALL_HIERARCHY_MISS,
                ))
            }
            TargetResolution::SelfCallReceiverUnproven(pool) => {
                still_unresolved.push(declined_pool_row(
                    edge,
                    index,
                    &pool,
                    NAME_ONLY_REASON_SELF_CALL_RECEIVER_UNPROVEN,
                ))
            }
            TargetResolution::SelfCallWithoutClassContext(pool) => {
                still_unresolved.push(declined_pool_row(
                    edge,
                    index,
                    &pool,
                    NAME_ONLY_REASON_SELF_CALL_WITHOUT_CLASS_CONTEXT,
                ))
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
    rust_mod_decls: &RustModDeclsByFile,
) -> TargetResolution {
    match edge.edge_type {
        EdgeType::Imports => {
            // PYTHON-SUBMODULE-IMPORT-1: a Python `from X import Y` may name a submodule of the
            // package `X`. The stage runs BEFORE the ladder's Stage 2 (whose extensionless entry
            // `X:FILE` is first-writer-wins and may name a same-named `X.py`): Python binds a
            // package directory before a module file. It reads only inputs `resolve_edges`
            // already has; when the stage does not apply the ladder runs exactly as before.
            if edge.extractor.starts_with(PYTHON_EXTRACTOR_PREFIX) {
                if let Some(inferred) =
                    resolve_python_submodule_import(edge, index, import_bindings_by_file)
                {
                    return inferred;
                }
            }
            resolve_import_ladder(edge, index, rust_mod_decls)
        }
        EdgeType::Calls => resolve_call_target(
            &edge.target_key,
            &edge.source_node_uid,
            &edge.extractor,
            edge.metadata_json.as_deref(),
            &index.nodes_by_stable_key,
            &index.nodes_by_name,
            &index.nodes_by_qualified_name,
            &index.nodes_by_uid,
            &index.file_resolution,
            import_bindings_by_file,
            &index.node_uid_to_file_uid,
        ),
        EdgeType::Instantiates => option_to_resolution(resolve_named_target(
            &edge.target_key,
            &index.nodes_by_name,
            edge.edge_type,
            &edge.extractor,
        )),
        EdgeType::Implements => option_to_resolution(resolve_named_target(
            &edge.target_key,
            &index.nodes_by_name,
            edge.edge_type,
            &edge.extractor,
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
                    &edge.extractor,
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
            &edge.extractor,
        )),
    }
}

/// The IMPORTS resolution ladder as it stood before PYTHON-SUBMODULE-IMPORT-1: the v1.1 C/C++
/// include resolver, then stages 1–4 (stable key, extensionless, per-TU include, declared Rust
/// crate, repo-prefix), then the declared-Java suffix stage. Extracted unchanged from
/// `resolve_target`'s Imports arm so the Python submodule stage can run ahead of it and fall back
/// to it unchanged on a miss.
fn resolve_import_ladder(
    edge: &ExtractedEdge,
    index: &ResolverIndex,
    rust_mod_decls: &RustModDeclsByFile,
) -> TargetResolution {
    let source_file_uid = index.node_uid_to_file_uid.get(&edge.source_node_uid);

    // v1.1: Try new include resolver first for C/C++ includes.
    if let Some(ref include_resolver) = index.include_resolver {
        if let Some(fuid) = source_file_uid {
            // Extract file path from file UID (repo:path format).
            if let Some(colon_pos) = fuid.find(':') {
                let source_path = &fuid[colon_pos + 1..];
                // Detect system include: C extractor sets metadata_json=None for system includes.
                let is_system = edge.metadata_json.is_none();
                let resolution = include_resolver.resolve(source_path, &edge.target_key, is_system);
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
    let tu_includes = source_file_uid.and_then(|fuid| index.per_file_include_resolution.get(fuid));
    // Gate the declared-Rust-crate stage (3.5) to edges emitted by the Rust
    // extractor. In a hybrid repo a non-Rust IMPORTS edge (e.g. a TS
    // `import cli from …` or a C++ `namespace::` reference) whose first segment
    // happens to match a declared Cargo package name would otherwise resolve to
    // that crate's `.rs` file, changing non-Rust extractor behaviour and
    // violating the frozen byte-stability invariant. `ExtractedEdge.extractor`
    // is the existing provenance fact; the Rust family is `rust-core:<ver>`.
    let is_rust_import = edge.extractor.starts_with(RUST_EXTRACTOR_PREFIX);
    // Stage 3.6's input (RUST-SELF-RESOLUTION-1): the importing file's repo-relative path, the
    // inline `mod` names enclosing the `use`, and the file's `mod` declarations. Built for Rust
    // edges only.
    let rust_use_site = if is_rust_import {
        source_file_uid.and_then(|fuid| {
            let source_path = fuid.split_once(':').map(|(_, path)| path)?;
            RustUseSite::from_edge(source_path, edge, rust_mod_decls.get(fuid))
        })
    } else {
        None
    };
    match resolve_import_target(
        &edge.target_key,
        &index.nodes_by_stable_key,
        &index.file_resolution,
        &edge.repo_uid,
        tu_includes,
        &index.rust_crate_roots,
        is_rust_import,
        rust_use_site.as_ref(),
    ) {
        Some(uid) => TargetResolution::Resolved(uid),
        // Stage 5 (IMPORT-RESOLUTION-JAVA-1 §2.1): declared Java FQN import. Runs ONLY
        // after stages 1-4 miss (a dotted FQN never keys a stable key / extensionless
        // path / include / crate root) and ONLY for Java-extractor edges — a non-Java
        // dotted specifier must never resolve to a `.java` file (frozen non-Java
        // byte-stability invariant). Returns the NAMED wildcard / ambiguous-suffix bases
        // directly so they are counted, not folded into `ImportsFileNotFound`.
        None if edge.extractor.starts_with(JAVA_EXTRACTOR_PREFIX) => resolve_java_import(
            &edge.target_key,
            &index.java_suffix_index,
            &index.nodes_by_stable_key,
            &edge.repo_uid,
        ),
        // Stage 6 (CPP-INCLUDE-BASENAME-1, RG-REQ-006-L11): a C/C++ include the include resolver
        // answered Unresolved (its Ambiguous answer returned above) and stages 1-4 missed. Runs
        // last, so it never changes an include an earlier stage resolves (RG-REQ-006-L03).
        None if is_c_family_extractor(&edge.extractor) => {
            resolve_include_suffix_or_basename(edge, index).unwrap_or(TargetResolution::Unresolved)
        }
        // Stage 7 (TS-ALIAS-RESOLUTION-1 then TS-WORKSPACE-RESOLUTION-1, RG-REQ-006-L04): only for
        // TS edges every earlier stage missed, so no certain binding ever flips (RG-REQ-006-L03).
        // The tsconfig `paths` stage runs BEFORE the workspace-source stage (D-TSA-RECORD-CONFLICT-1:
        // L04's order and TypeScript's precedence); the workspace stage runs only when the `paths`
        // stage returns no binding and no named ambiguity — a matched pattern with no indexed hit
        // included, as TypeScript falls through to package lookup.
        None if edge.extractor.starts_with(TS_EXTRACTOR_PREFIX) => {
            resolve_tsconfig_paths_import(edge, index)
                .or_else(|| resolve_workspace_package_import(edge, index))
                .unwrap_or(TargetResolution::Unresolved)
        }
        None => TargetResolution::Unresolved,
    }
}

/// CPP-INCLUDE-BASENAME-1 (RG-REQ-006-L11 as ratified; RG-REQ-002-L11): the C/C++ include
/// suffix/basename stage. Asks the include map what the indexed file list says about the
/// specifier and decides:
///
/// - a unique path suffix (two or more segments) whose FILE node exists → `static`, basis
///   `unique_suffix`;
/// - a unique basename (one segment) whose FILE node exists → `inferred`, basis `unique_basename`
///   (a file of that name exists; nothing says the build's include path reaches it);
/// - several matches → unresolved ambiguous, basis `ambiguous_suffix`/`ambiguous_basename`, every
///   candidate recorded;
/// - no match, a declined specifier, or a unique candidate without a FILE node → `None` (the row
///   stays unresolved exactly as before).
///
/// Every outcome merges `basis` and `candidates` (FILE stable keys) into the extractor's carrier,
/// keeping its keys. A present carrier that is not a JSON object declines the stage (`None`):
/// the row is never overwritten and never coerced. PURE: reads only the resolver index.
fn resolve_include_suffix_or_basename(
    edge: &ExtractedEdge,
    index: &ResolverIndex,
) -> Option<TargetResolution> {
    let include_map = index.include_resolver.as_ref()?;
    let mut carrier = match edge.metadata_json.as_deref() {
        None => serde_json::Map::new(),
        Some(raw) => match serde_json::from_str::<serde_json::Value>(raw) {
            Ok(serde_json::Value::Object(obj)) => obj,
            // Unparseable, or JSON that is not an object: decline, leave the row as it is.
            Ok(_) | Err(_) => return None,
        },
    };
    let file_key = |path: &str| format!("{}:{}:FILE", edge.repo_uid, path);
    let (basis, candidates, bound) =
        match include_map.match_path_suffix_or_basename(&edge.target_key) {
            IncludePathMatch::Declined | IncludePathMatch::NoMatch => return None,
            IncludePathMatch::UniqueSuffix(path) => (
                INCLUDE_UNIQUE_SUFFIX_BASIS,
                vec![file_key(&path)],
                Some(Resolution::Static),
            ),
            IncludePathMatch::UniqueBasename(path) => (
                INCLUDE_UNIQUE_BASENAME_BASIS,
                vec![file_key(&path)],
                Some(Resolution::Inferred),
            ),
            IncludePathMatch::AmbiguousSuffix(paths) => (
                INCLUDE_AMBIGUOUS_SUFFIX_BASIS,
                paths.iter().map(|p| file_key(p)).collect(),
                None,
            ),
            IncludePathMatch::AmbiguousBasename(paths) => (
                INCLUDE_AMBIGUOUS_BASENAME_BASIS,
                paths.iter().map(|p| file_key(p)).collect(),
                None,
            ),
        };
    // A unique candidate binds only to its FILE node; without one the row stays as it is.
    let bound = match bound {
        Some(resolution) => Some((
            resolution,
            index
                .nodes_by_stable_key
                .get(&candidates[0])?
                .node_uid
                .clone(),
        )),
        None => None,
    };
    carrier.insert("basis".to_string(), serde_json::json!(basis));
    carrier.insert("candidates".to_string(), serde_json::json!(candidates));
    let metadata_json = serde_json::Value::Object(carrier).to_string();
    Some(match bound {
        Some((resolution, target_uid)) => TargetResolution::IncludeBound {
            target_uid,
            resolution,
            metadata_json,
        },
        None => TargetResolution::IncludeAmbiguous { metadata_json },
    })
}

/// TS-WORKSPACE-RESOLUTION-1 (RG-REQ-006-L04 as amended by D-TS-WORKSPACE-1 = A; RG-REQ-002-L11):
/// the workspace member import stage. Applies only when the edge's carrier is a JSON object whose
/// `rawPath` equals the edge's `target_key` (the bare specifier the TS extractor emits); asks
/// [`match_workspace_package_import`] and decides:
///
/// - bound → `inferred` edge to the source entry's FILE node, the carrier merged with `basis:
///   workspace_source_entry` and `candidates` (the source entry, then every declared target, as
///   FILE stable keys — none selected);
/// - ambiguous → the row stays unresolved, the carrier merged with `basis:
///   ambiguous_workspace_source_entry` and every candidate;
/// - otherwise, or with any other carrier → `None` (the row stays exactly as before).
///
/// PURE: reads only the resolver index.
fn resolve_workspace_package_import(
    edge: &ExtractedEdge,
    index: &ResolverIndex,
) -> Option<TargetResolution> {
    if index.npm_workspace_packages.is_empty() {
        return None;
    }
    let mut carrier =
        match serde_json::from_str::<serde_json::Value>(edge.metadata_json.as_deref()?) {
            Ok(serde_json::Value::Object(obj)) => obj,
            Ok(_) | Err(_) => return None,
        };
    if carrier.get("rawPath").and_then(serde_json::Value::as_str) != Some(edge.target_key.as_str())
    {
        return None;
    }
    let file_key = |path: &str| format!("{}:{}:FILE", edge.repo_uid, path);
    let is_indexed = |path: &str| index.nodes_by_stable_key.contains_key(&file_key(path));
    let (basis, candidates, bound_uid) = match match_workspace_package_import(
        &edge.target_key,
        &index.npm_workspace_packages,
        is_indexed,
    ) {
        WorkspaceImportMatch::NotBound => return None,
        WorkspaceImportMatch::Bound {
            source_entry,
            declared_entries,
        } => {
            let target_uid = index
                .nodes_by_stable_key
                .get(&file_key(&source_entry))?
                .node_uid
                .clone();
            let candidates: Vec<String> = std::iter::once(&source_entry)
                .chain(declared_entries.iter())
                .map(|p| file_key(p))
                .collect();
            (WORKSPACE_SOURCE_ENTRY_BASIS, candidates, Some(target_uid))
        }
        WorkspaceImportMatch::Ambiguous {
            source_entries,
            declared_entries,
        } => (
            AMBIGUOUS_WORKSPACE_SOURCE_ENTRY_BASIS,
            source_entries
                .iter()
                .chain(declared_entries.iter())
                .map(|p| file_key(p))
                .collect(),
            None,
        ),
    };
    carrier.insert("basis".to_string(), serde_json::json!(basis));
    carrier.insert("candidates".to_string(), serde_json::json!(candidates));
    let metadata_json = serde_json::Value::Object(carrier).to_string();
    Some(match bound_uid {
        Some(target_uid) => TargetResolution::WorkspaceSourceEntryBound {
            target_uid,
            metadata_json,
        },
        None => TargetResolution::WorkspaceSourceEntryAmbiguous { metadata_json },
    })
}

/// TS-ALIAS-RESOLUTION-1 (RG-REQ-006-L04 as ruled by D-TSA-RECORD-CONFLICT-1 and
/// D-TSA-BOUNDED-SCOPE-1; RG-REQ-002-L11): the tsconfig `paths` stage. Applies only when the
/// edge's carrier is a JSON object whose `rawPath` equals the edge's `target_key` (the specifier
/// the TS extractor records) and that specifier is non-relative (TypeScript applies `paths` only to
/// non-relative names: not `.`, `..`, nor starting `./`, `../` or `/`). Asks the source file's one
/// mapping ([`TsconfigPathsIndex::hits_for`]) and decides:
///
/// - one hit of the selected pattern → STATIC edge, the carrier merged with `basis:
///   tsconfig_paths` and `candidates: [the target's FILE stable key]`;
/// - several hits of the selected pattern → unresolved, `basis: ambiguous_tsconfig_paths` and every
///   candidate;
/// - patterns tied for the longest prefix reaching one or more files → unresolved, `basis:
///   tied_tsconfig_paths_patterns` and every candidate;
/// - no mapping for the file, no matching pattern, or no indexed hit (tied or not) → `None`: the
///   workspace-source stage runs, and with no workspace binding the row stays exactly as before.
///
/// PURE: reads only the resolver index.
fn resolve_tsconfig_paths_import(
    edge: &ExtractedEdge,
    index: &ResolverIndex,
) -> Option<TargetResolution> {
    if index.tsconfig_paths.is_empty() {
        return None;
    }
    let mut carrier =
        match serde_json::from_str::<serde_json::Value>(edge.metadata_json.as_deref()?) {
            Ok(serde_json::Value::Object(obj)) => obj,
            Ok(_) | Err(_) => return None,
        };
    let specifier = edge.target_key.as_str();
    if carrier.get("rawPath").and_then(serde_json::Value::as_str) != Some(specifier) {
        return None;
    }
    let relative = specifier == "."
        || specifier == ".."
        || specifier.starts_with("./")
        || specifier.starts_with("../")
        || specifier.starts_with('/');
    if relative {
        return None;
    }
    let file_uid = index.node_uid_to_file_uid.get(&edge.source_node_uid)?;
    let (basis, candidates) = match index.tsconfig_paths.hits_for(file_uid, specifier)? {
        TsconfigAliasHits::NoPatternMatches => return None,
        TsconfigAliasHits::SelectedPattern(hits) if hits.is_empty() => return None,
        TsconfigAliasHits::TiedPatterns(hits) if hits.is_empty() => return None,
        TsconfigAliasHits::SelectedPattern(hits) if hits.len() == 1 => {
            let target_key = hits.into_iter().next()?;
            let target_uid = index.nodes_by_stable_key.get(&target_key)?.node_uid.clone();
            carrier.insert("basis".to_string(), serde_json::json!(TSCONFIG_PATHS_BASIS));
            carrier.insert("candidates".to_string(), serde_json::json!([target_key]));
            return Some(TargetResolution::TsconfigPathsBound {
                target_uid,
                metadata_json: serde_json::Value::Object(carrier).to_string(),
            });
        }
        TsconfigAliasHits::SelectedPattern(hits) => (AMBIGUOUS_TSCONFIG_PATHS_BASIS, hits),
        TsconfigAliasHits::TiedPatterns(hits) => (TIED_TSCONFIG_PATHS_PATTERNS_BASIS, hits),
    };
    carrier.insert("basis".to_string(), serde_json::json!(basis));
    carrier.insert(
        "candidates".to_string(),
        serde_json::json!(candidates.into_iter().collect::<Vec<String>>()),
    );
    Some(TargetResolution::TsconfigPathsAmbiguous {
        metadata_json: serde_json::Value::Object(carrier).to_string(),
    })
}

/// Convert Option<String> to TargetResolution.
fn option_to_resolution(opt: Option<String>) -> TargetResolution {
    match opt {
        Some(uid) => TargetResolution::Resolved(uid),
        None => TargetResolution::Unresolved,
    }
}

// ── IMPORTS resolution ───────────────────────────────────────────

#[allow(clippy::too_many_arguments)] // the ladder's stage inputs, threaded from the index
fn resolve_import_target(
    target_key: &str,
    nodes_by_stable_key: &HashMap<String, ResolverNode>,
    file_resolution: &HashMap<String, String>,
    repo_uid: &str,
    tu_include_resolution: Option<&HashMap<String, String>>,
    rust_crate_roots: &HashMap<String, String>,
    is_rust_import: bool,
    rust_use_site: Option<&RustUseSite>,
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

    // Stage 3.6 (RUST-SELF-RESOLUTION-1, RG-REQ-006-L14): a Rust `use` whose head is
    // `crate`/`super`/`self`, or whose first segment named no declared crate (stage 3.5 missed:
    // a sibling module path), resolves against the module that encloses the `use`. Runs only for
    // Rust-extractor edges (`rust_use_site` is built for them alone) and only after 1–3.5 miss.
    if let Some(site) = rust_use_site {
        if let Some(resolved_key) = resolve_rust_enclosing_module_import(
            target_key,
            site,
            rust_crate_roots,
            file_resolution,
            repo_uid,
        ) {
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
/// the file set wins. The crate's own `crate`/`super`/`self` paths, and a sibling module path
/// whose first segment names no declared crate, miss here and go to stage 3.6
/// ([`resolve_rust_enclosing_module_import`]) through the same ladder ([`rust_module_ladder`]).
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

    let rest: Vec<&str> = segs.collect();
    rust_module_ladder(
        &rust_src_prefix(crate_root),
        &rest,
        file_resolution,
        repo_uid,
    )
}

/// `<crate_root>/src`, collapsing a "." (or empty) root crate to a bare `src`.
fn rust_src_prefix(crate_root: &str) -> String {
    if crate_root == "." || crate_root.is_empty() {
        "src".to_string()
    } else {
        format!("{}/src", crate_root)
    }
}

/// The Rust module ladder (RG-REQ-006-L01, shared by stages 3.5 and 3.6): for the module
/// segments `segs` under `src_prefix`, probe `<segs>.rs`, then `<segs>/mod.rs`, then drop the
/// last segment and repeat, ending at the crate entry point `src/lib.rs` then `src/main.rs`.
/// Returns the FIRST FILE stable key present in `file_resolution` (identity entries). PURE.
fn rust_module_ladder(
    src_prefix: &str,
    segs: &[&str],
    file_resolution: &HashMap<String, String>,
    repo_uid: &str,
) -> Option<String> {
    let probe = |rel_path: &str| -> Option<String> {
        let key = format!("{}:{}:FILE", repo_uid, rel_path);
        file_resolution.get(&key).cloned()
    };

    let mut remaining = segs;
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

/// Stage 3.6's view of a Rust `use` site: the importing file's repo-relative path and the names
/// of the inline `mod` bodies that enclose the `use`, outermost first (the extractor's
/// `inlineModulePath` key; absent for a top-level `use`).
struct RustUseSite<'a> {
    source_path: &'a str,
    inline_module_path: Vec<String>,
    /// The importing FILE node's `mod <name>;` declarations (D-RSR-ENTRY-CLIMB-1); `None` when
    /// the file declares none.
    declared_modules: Option<&'a HashSet<RustModDeclaration>>,
}

impl<'a> RustUseSite<'a> {
    /// `None` when the edge's carrier is present but unreadable, or its `inlineModulePath` is not
    /// a list of strings: the enclosing module is then unknown and stage 3.6 binds nothing.
    fn from_edge(
        source_path: &'a str,
        edge: &ExtractedEdge,
        declared_modules: Option<&'a HashSet<RustModDeclaration>>,
    ) -> Option<Self> {
        let inline_module_path = match edge.metadata_json.as_deref() {
            None => Vec::new(),
            Some(raw) => {
                let value: serde_json::Value = serde_json::from_str(raw).ok()?;
                match value.as_object()?.get("inlineModulePath") {
                    None => Vec::new(),
                    Some(list) => list
                        .as_array()?
                        .iter()
                        .map(|v| v.as_str().map(str::to_string))
                        .collect::<Option<Vec<String>>>()?,
                }
            }
        };
        Some(Self {
            source_path,
            inline_module_path,
            declared_modules,
        })
    }
}

/// RUST-SELF-RESOLUTION-1 (RG-REQ-006-L14): resolve a Rust `use` against the module that encloses
/// it. PURE: `(target_key, use site, declared crate roots, file set, repo_uid) → Option<FILE
/// stable key>`.
///
/// 1. Crate root: the longest declared root that is `.`/empty or a `/`-terminated prefix of the
///    source path; none → `None`.
/// 2. The source must lie under `<root>/src/`, not under `<root>/src/bin/`, and end in `.rs`.
/// 3. The file's module path: `src/a/b.rs` → `a::b`, `src/a/mod.rs` → `a`, `src/lib.rs` and
///    `src/main.rs` → the crate root; then the `inlineModulePath` names are appended.
/// 4. Head: `crate` → the remaining segments from the root; `self` → module + remaining; each
///    leading `super` pops one segment (more pops than segments → `None`); any other first segment
///    is a sibling path (D-RSR-ENTRY-CLIMB-1): a declared crate name → `None` (a stage-3.5 miss
///    never becomes a sibling reading); otherwise it binds only when the use site's FILE node
///    declares `mod <first>;` with `inline_path` equal to the use's `inlineModulePath` AND
///    `<enclosing>/<first>.rs` or `<enclosing>/<first>/mod.rs` is indexed → module + every segment;
///    else `None`.
/// 5. [`rust_module_ladder`]; a hit equal to the source file binds nothing (D-RSR-SELF-IMPORT-1).
///    For a sibling path the first-segment file is indexed, so the ladder stops at it at the
///    latest: it never shortens past the first segment.
fn resolve_rust_enclosing_module_import(
    target_key: &str,
    site: &RustUseSite,
    rust_crate_roots: &HashMap<String, String>,
    file_resolution: &HashMap<String, String>,
    repo_uid: &str,
) -> Option<String> {
    let source_path = site.source_path;
    let crate_root = rust_crate_roots
        .values()
        .filter(|root| {
            root.as_str() == "." || root.is_empty() || source_path.starts_with(&format!("{root}/"))
        })
        .max_by_key(|root| if root.as_str() == "." { 0 } else { root.len() })?;
    let src_prefix = rust_src_prefix(crate_root);
    let rel = source_path.strip_prefix(&format!("{src_prefix}/"))?;
    if rel.starts_with("bin/") {
        return None;
    }
    let rel = rel.strip_suffix(".rs")?;
    let file_segs: Vec<&str> = rel.split('/').collect();
    let mut module: Vec<&str> = match file_segs.as_slice() {
        ["lib"] | ["main"] => Vec::new(),
        [dirs @ .., "mod"] => dirs.to_vec(),
        all => all.to_vec(),
    };
    module.extend(site.inline_module_path.iter().map(String::as_str));

    let parts: Vec<&str> = target_key.split("::").collect();
    let segs: Vec<&str> = match parts[0] {
        "crate" => parts[1..].to_vec(),
        "self" => module
            .iter()
            .copied()
            .chain(parts[1..].iter().copied())
            .collect(),
        "super" => {
            let pops = parts.iter().take_while(|p| **p == "super").count();
            if pops > module.len() {
                return None;
            }
            module[..module.len() - pops]
                .iter()
                .copied()
                .chain(parts[pops..].iter().copied())
                .collect()
        }
        "" => return None,
        first => {
            if rust_crate_roots.contains_key(&canonicalize_cargo_package_name(first)) {
                return None;
            }
            let declared = site.declared_modules.is_some_and(|decls| {
                decls.iter().any(|d| {
                    d.name == first
                        && d.inline_path.as_slice() == site.inline_module_path.as_slice()
                })
            });
            if !declared {
                return None;
            }
            let first_dir = module
                .iter()
                .copied()
                .chain(std::iter::once(first))
                .collect::<Vec<&str>>()
                .join("/");
            let indexed = |rel_path: String| {
                file_resolution.contains_key(&format!("{repo_uid}:{src_prefix}/{rel_path}:FILE"))
            };
            if !indexed(format!("{first_dir}.rs")) && !indexed(format!("{first_dir}/mod.rs")) {
                return None;
            }
            module
                .iter()
                .copied()
                .chain(parts.iter().copied())
                .collect()
        }
    };

    let hit = rust_module_ladder(&src_prefix, &segs, file_resolution, repo_uid)?;
    if hit == format!("{repo_uid}:{source_path}:FILE") {
        return None;
    }
    Some(hit)
}

// ── Python package-submodule import inference (PYTHON-SUBMODULE-IMPORT-1) ──

/// PYTHON-SUBMODULE-IMPORT-1 (RG-REQ-006-L04 Python clause; RG-REQ-002-L11): decide whether a
/// Python `from X import Y` whose package `X` has an indexed init `X/__init__.py` names a
/// submodule instead, and if so return the INFERRED outcome. Runs before the ladder's Stage 2, so
/// a same-named module file `X.py` never pre-empts the package (Python's FileFinder order).
///
/// Python runs `X/__init__.py`, then takes its attribute `Y`; only when the init leaves no `Y` does
/// it load the submodule — the package directory `X/Y/` before the module file `X/Y.py`. The
/// index sees two kinds of evidence of the init binding `Y`: a module-level SYMBOL `Y` of the init
/// (`qualified_name == Y`; a nested member such as `Thing.Y` is not a module attribute) and an
/// import binding of `Y` in the init other than the submodule itself. Either keeps the init as the
/// `static` target. Otherwise, when `X/Y/__init__.py` or `X/Y.py` is an indexed FILE
/// (exact keys — never the extensionless map, so a non-Python file is never a target), the edge
/// is retargeted as INFERRED with the init as its alternate: the extractor's blind spots
/// (multi-target assignment, `__getattr__`, star imports) are why it is never `static`.
///
/// The stage APPLIES when the edge carries a plain-identifier `importedName`, the key's package
/// directory has an indexed init, and a submodule file is indexed. RULE: once it applies, the
/// stage decides the target itself — the submodule (INFERRED) or the init (`static`) — and never
/// hands the choice to the ladder, whose extensionless `X:FILE` entry is first-writer-wins and may
/// name a same-named `X.py`. The init is returned as a `static` target when the init binds `Y`, or
/// when its file identity or its import bindings are unknown (no evidence that it leaves `Y`
/// unbound). Returns `None` — the ladder's result stands, byte-identical to before — only when the
/// stage does not apply. PURE: reads only the resolver index and the per-file import bindings
/// `resolve_edges` already receives.
fn resolve_python_submodule_import(
    edge: &ExtractedEdge,
    index: &ResolverIndex,
    import_bindings_by_file: Option<&HashMap<String, Vec<ImportBinding>>>,
) -> Option<TargetResolution> {
    let imported_name = python_imported_name(edge.metadata_json.as_deref())?;
    let package_dir = python_package_dir(&edge.target_key, &edge.repo_uid)?;

    // The package `D` must have an indexed init `D/__init__.py` (a regular package).
    let init_key = format!("{}:{}/__init__.py:FILE", edge.repo_uid, package_dir);
    let init_node = index.nodes_by_stable_key.get(&init_key)?;

    // The submodule file: the package directory first, then the module file (Python's FileFinder).
    let target_uid = [
        format!(
            "{}:{}/{}/__init__.py:FILE",
            edge.repo_uid, package_dir, imported_name
        ),
        format!(
            "{}:{}/{}.py:FILE",
            edge.repo_uid, package_dir, imported_name
        ),
    ]
    .iter()
    .find_map(|key| index.nodes_by_stable_key.get(key))
    .map(|node| node.node_uid.clone())?;

    // From here the stage applies and decides the target: the init, `static`, whenever the
    // evidence keeps it; the submodule, INFERRED, otherwise. Never `None` (the ladder's pick).
    let keep_init = || Some(TargetResolution::Resolved(init_node.node_uid.clone()));

    // Without the init's file identity or the bindings map there is no evidence that the init
    // leaves `Y` unbound — keep the init.
    let Some(init_file_uid) = init_node
        .file_uid
        .as_ref()
        .or_else(|| index.node_uid_to_file_uid.get(&init_node.node_uid))
    else {
        return keep_init();
    };
    let Some(bindings_by_file) = import_bindings_by_file else {
        return keep_init();
    };

    let defines_name = index
        .nodes_by_name
        .get(imported_name.as_str())
        .into_iter()
        .flatten()
        .any(|n| {
            n.kind == "SYMBOL"
                && n.qualified_name.as_deref() == Some(imported_name.as_str())
                && index
                    .node_uid_to_file_uid
                    .get(&n.node_uid)
                    .or(n.file_uid.as_ref())
                    == Some(init_file_uid)
        });
    if defines_name {
        return keep_init();
    }

    // An init with no signals row has no import bindings (the orchestrator writes a row whenever
    // a file has one).
    let package_dotted = package_dir.replace('/', ".");
    let binds_name_elsewhere = bindings_by_file
        .get(init_file_uid)
        .into_iter()
        .flatten()
        .any(|b| {
            b.identifier == imported_name
                && !python_binding_is_the_submodule_itself(b, &package_dotted, &imported_name)
        });
    if binds_name_elsewhere {
        return keep_init();
    }

    Some(TargetResolution::InferredImport {
        target_uid,
        alternate_stable_key: init_key,
        basis: PYTHON_SUBMODULE_BASIS,
    })
}

/// The `importedName` of a Python from-import edge (`from X import Y` → `Y`), when it is a plain
/// identifier. `None` for a whole-module `import a.b` (no key), a wildcard, or a carrier that does
/// not parse — the stage then does not apply and the ladder's result stands.
fn python_imported_name(metadata_json: Option<&str>) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(metadata_json?).ok()?;
    let name = value.get("importedName")?.as_str()?;
    let mut chars = name.chars();
    let first = chars.next()?;
    let is_identifier =
        (first.is_alphabetic() || first == '_') && chars.all(|c| c.is_alphanumeric() || c == '_');
    is_identifier.then(|| name.to_string())
}

/// The repo-relative package directory a Python IMPORTS `target_key` names: the key itself for an
/// absolute specifier (`django/core/handlers`, the extractor's `.`→`/` form), `<dir>` for a relative
/// key `<repo>:<dir>:FILE`. `None` for an empty or otherwise-shaped key.
fn python_package_dir<'a>(target_key: &'a str, repo_uid: &str) -> Option<&'a str> {
    let dir = if target_key.contains(':') {
        target_key
            .strip_prefix(repo_uid)?
            .strip_prefix(':')?
            .strip_suffix(":FILE")?
    } else {
        target_key
    };
    (!dir.is_empty() && dir != "." && !dir.starts_with('/') && !dir.ends_with('/')).then_some(dir)
}

/// Is this import binding in the package init `D/__init__.py` the submodule `D.Y` itself? Exactly
/// three forms: `from . import Y`, `from <D dotted> import Y` (the attribute `Y` of the package, which
/// is the submodule once imported) and `import <D dotted>.Y as Y`. `from .Y import Y` binds the
/// attribute `Y` OF the submodule — a different object — as does every aliased or foreign form.
fn python_binding_is_the_submodule_itself(
    binding: &ImportBinding,
    package_dotted: &str,
    name: &str,
) -> bool {
    match binding.imported_name.as_deref() {
        Some(imported) => {
            imported == name && (binding.specifier == "." || binding.specifier == package_dotted)
        }
        None => binding.specifier == format!("{package_dotted}.{name}"),
    }
}

/// Add the other candidate (`alternateTarget`, the package init's stable key) and the reason
/// (`basis`) to an inferred IMPORTS edge's `metadata_json`, preserving the extractor's keys
/// (`specifier`, `identifier`, `importedName`, `isTypeOnly`). Only called on a carrier the stage
/// already parsed (it read `importedName` from it); mirrors `inject_mro_candidates`.
fn inject_inferred_import_candidates(
    metadata_json: Option<&str>,
    alternate_stable_key: &str,
    basis: &str,
) -> String {
    let mut obj = metadata_json
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    obj.insert(
        "alternateTarget".to_string(),
        serde_json::json!(alternate_stable_key),
    );
    obj.insert("basis".to_string(), serde_json::json!(basis));
    serde_json::Value::Object(obj).to_string()
}

// ── Java FQN import resolution (IMPORT-RESOLUTION-JAVA-1) ─────────

/// Outcome of the pure Java suffix lookup — the ratified
/// `(key, index) → Option<file key>` stage widened to carry the ambiguous case honestly.
#[derive(Debug, PartialEq, Eq)]
enum JavaSuffixLookup {
    /// Exactly one indexed `.java` file matched the suffix. Carries its repo-relative path.
    Resolved(String),
    /// More than one indexed `.java` file matched the suffix (shaded / duplicated copies).
    Ambiguous,
    /// No indexed `.java` file matched any suffix (down to the bare class name).
    NotFound,
}

/// IMPORT-RESOLUTION-JAVA-1 §2.1: resolve a Java FQN IMPORTS `target_key` to a `.java` FILE
/// node UID via the suffix index. Wildcard and ambiguous imports return their NAMED
/// unresolved bases (counted, never mis-resolved). The wildcard test is here (not in the pure
/// `resolve_java_suffix`) because it is a property of the target_key, not of the file set.
fn resolve_java_import(
    target_key: &str,
    java_suffix_index: &JavaSuffixIndex,
    nodes_by_stable_key: &HashMap<String, ResolverNode>,
    repo_uid: &str,
) -> TargetResolution {
    // `import pkg.*` (and `import static pkg.*`) — a package, not a single type.
    if target_key.ends_with(".*") {
        return TargetResolution::JavaWildcard;
    }
    match resolve_java_suffix(target_key, java_suffix_index) {
        JavaSuffixLookup::Resolved(rel_path) => {
            let key = format!("{}:{}:FILE", repo_uid, rel_path);
            match nodes_by_stable_key.get(&key) {
                Some(node) => TargetResolution::Resolved(node.node_uid.clone()),
                // The path came from the index (built from the same file list the FILE nodes
                // come from), so a missing FILE node should not happen; stay unresolved rather
                // than fabricate a target.
                None => TargetResolution::Unresolved,
            }
        }
        JavaSuffixLookup::Ambiguous => TargetResolution::JavaAmbiguousSuffix,
        JavaSuffixLookup::NotFound => TargetResolution::Unresolved,
    }
}

/// IMPORT-RESOLUTION-JAVA-1 §2.1: the PURE suffix stage — `(dotted key, suffix index) →
/// resolution`. No I/O, no source-root knowledge.
///
/// For a key `a.b.C.D`: try the boundary suffix `a/b/C/D.java`, then drop the last segment and
/// retry (`a/b/C.java` — a nested class or a static member of `C`), down to the bare class name.
/// A single boundary match resolves; multiple matches are `Ambiguous`; a level with candidates
/// that none match is skipped (shorten and continue); exhausting all levels is `NotFound`. An
/// ambiguous FULL match short-circuits (it is genuinely ambiguous — do not shorten past it).
fn resolve_java_suffix(target_key: &str, java_suffix_index: &JavaSuffixIndex) -> JavaSuffixLookup {
    let segs: Vec<&str> = target_key.split('.').collect();
    if segs.iter().any(|s| s.is_empty()) {
        // A malformed key (leading/trailing/double dot) — nothing to resolve.
        return JavaSuffixLookup::NotFound;
    }
    // Shorten from the full FQN down to the bare class name (index 0..=end).
    for end in (1..=segs.len()).rev() {
        let sub = &segs[..end];
        let class = sub[end - 1];
        let basename = format!("{}.java", class);
        let Some(paths) = java_suffix_index.get(&basename) else {
            // No file with this class name at this level — shorten and retry.
            continue;
        };
        let suffix = format!("{}.java", sub.join("/"));
        let mut matched: Option<&String> = None;
        let mut match_count = 0usize;
        for path in paths {
            if path_has_boundary_suffix(path, &suffix) {
                match_count += 1;
                if matched.is_none() {
                    matched = Some(path);
                }
            }
        }
        match match_count {
            0 => continue,
            1 => return JavaSuffixLookup::Resolved(matched.unwrap().clone()),
            _ => return JavaSuffixLookup::Ambiguous,
        }
    }
    JavaSuffixLookup::NotFound
}

/// True iff `path` ends with `suffix` at a directory boundary — either the whole path equals
/// `suffix`, or `path` ends with `/<suffix>`. The boundary guard stops `common/Foo.java` from
/// matching a file whose basename is `UncommonFoo.java`.
fn path_has_boundary_suffix(path: &str, suffix: &str) -> bool {
    path == suffix
        || (path.len() > suffix.len()
            && path.ends_with(suffix)
            && path.as_bytes()[path.len() - suffix.len() - 1] == b'/')
}

/// IMPORT-RESOLUTION-JAVA-1 §2.1: build the Java suffix index from the snapshot's file list.
/// One pass; a `.java` file's basename → all repo-relative paths carrying it. Non-`.java` files
/// are ignored (the stage only ever resolves to `.java` targets).
pub fn build_java_suffix_index(file_paths: &[String]) -> JavaSuffixIndex {
    let mut index: JavaSuffixIndex = HashMap::new();
    for path in file_paths {
        if !path.ends_with(".java") {
            continue;
        }
        let basename = path.rsplit('/').next().unwrap_or(path).to_string();
        index.entry(basename).or_default().push(path.clone());
    }
    index
}

// ── CALLS resolution ─────────────────────────────────────────────

#[allow(clippy::too_many_arguments)] // resolver hot-path fn threading the index's maps
                                     // by reference; bundling them into a struct would add
                                     // a type with no other user (not earned).
fn resolve_call_target(
    target_key: &str,
    source_node_uid: &str,
    extractor: &str,
    metadata_json: Option<&str>,
    _nodes_by_stable_key: &HashMap<String, ResolverNode>,
    nodes_by_name: &HashMap<String, Vec<ResolverNode>>,
    nodes_by_qualified_name: &HashMap<String, Vec<ResolverNode>>,
    nodes_by_uid: &HashMap<String, ResolverNode>,
    file_resolution: &HashMap<String, String>,
    import_bindings_by_file: Option<&HashMap<String, Vec<ImportBinding>>>,
    node_uid_to_file_uid: &HashMap<String, String>,
) -> TargetResolution {
    // CALL-BINDING-RECEIVER-1 §2.1/§2.2: read the call's receiver disposition from the edge.
    // `receiver` is the receiver expression text (`"this"` for an explicit self-call, a bare
    // name like `"impl"`/`"versions_"` for an indirect receiver, absent for a receiverless
    // `m()`); the receiver's declared type — when the extractor could resolve it in the same file —
    // is CARRIED inside the `Indirect` disposition as a three-state `ReceiverTypeRead` (F-CBR-007),
    // never as a fact that could exist beside a non-indirect receiver. Every field is absent for a
    // non-C++ call (only the C++ extractor emits them), so this is a no-op for other languages.
    let receiver_disposition = call_receiver_info(metadata_json);
    let is_python = extractor.starts_with(PYTHON_EXTRACTOR_PREFIX);

    // ── Python self/cls calls (PYTHON-RECEIVER-BINDING-1, R1) ─────────
    // The WHOLE two-part `self.<m>` / `cls.<m>` case of a Python edge is decided here and is
    // TERMINAL: it never reaches the dotted-name fallback, whose bare-name pool is not evidence
    // for a receiver the enclosing class types (RG-REQ-005-L03; D-PRB-SCOPE-1 amendment 2).
    if is_python {
        if let Some(method) = python_self_call_method(target_key) {
            return resolve_python_self_call(
                target_key,
                method,
                source_node_uid,
                extractor,
                metadata_json,
                nodes_by_name,
                nodes_by_qualified_name,
                file_resolution,
                import_bindings_by_file,
                node_uid_to_file_uid,
            );
        }
    }

    // ── Namespace import resolution ───────────────────────────────────
    // For calls like `fs.readFile()` where `fs` is a namespace import,
    // extract the member name and look it up in the imported module.
    if let Some(uid) = resolve_namespace_member(
        target_key,
        source_node_uid,
        nodes_by_name,
        file_resolution,
        import_bindings_by_file,
        node_uid_to_file_uid,
    ) {
        return TargetResolution::Resolved(uid);
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
        } else if is_python {
            // PYTHON-RECEIVER-BINDING-1 (R2/R2b; RG-REQ-005-L02, RG-REQ-002-L11): a Python
            // receiver other than a two-part `self`/`cls` carries no type evidence the index
            // holds (a local, a parameter, a chain, an unresolvable alias, a name spelled like a
            // class — amendment 1: no class-name rule). The method name alone is not evidence: a
            // pool of one is INFERRED with its pool; two or more stay unresolved with every
            // candidate; an empty pool keeps today's path and records nothing. Both fallback
            // branches (`this.`-prefixed and `obj.m`) share this rule.
            let pool = name_only_pool(nodes_by_name.get(method_name));
            match pool.len() {
                0 => {}
                1 => {
                    let receiver = target_key
                        .rsplit_once('.')
                        .map(|(r, _)| r)
                        .unwrap_or(prefix);
                    return TargetResolution::InferredNameOnly {
                        uid: pool[0].node_uid.clone(),
                        receiver: receiver.to_string(),
                        candidate: candidate_label(pool[0]),
                    };
                }
                _ => {
                    return TargetResolution::AmbiguousNameOnly(sorted_candidate_labels(&pool));
                }
            }
        } else {
            // "this.repo.findById" style (3+ parts starting with "this")
            if prefix == "this" && parts.len() >= 3 {
                if let Some(uid) =
                    pick_unambiguous(nodes_by_name.get(method_name), EdgeType::Calls, false)
                {
                    return TargetResolution::Resolved(uid);
                }
            }

            // "obj.method()" — try method name (only for non-import objects).
            if let Some(uid) =
                pick_unambiguous(nodes_by_name.get(method_name), EdgeType::Calls, false)
            {
                return TargetResolution::Resolved(uid);
            }
        }
    }

    // CALL-BINDING-RECEIVER-1 §2.2: RECEIVER-TYPE binding — runs BEFORE the bare-name singleton
    // and ONLY when the edge carries a `receiverType` (an indirect call whose receiver's declared
    // type the extractor resolved in-file). Among the same affinity/decl-filtered pool, keep the
    // candidates whose own container IS that type; a UNIQUE survivor is the evidence-backed
    // binding (`impl->Recover()` with `impl : DBImpl` → `leveldb::DBImpl::Recover`). Zero or many
    // survivors → fall through (unresolved-and-counted / unique-name below), never a guess. The
    // base-class walk of §2.2 is deferred: C++ IMPLEMENTS edges are anchored on the FILE node
    // today (RC-3), so the inheritance closure is not reliably available here — this is the
    // direct `<receiverType>::<name>` lookup the §4 stop condition sanctions; the walk depends on
    // EXPLAIN-TYPE-SECTIONS-1 re-anchoring IMPLEMENTS (recorded as a follow-up, not built here).
    // Usable receiver-type evidence is reachable ONLY through `Indirect { Present(..) }` (F-CBR-007):
    // an `Absent` (cross-file) or `Unreadable` (malformed) type carries nothing to match, and no
    // other disposition can carry a type at all.
    if let ReceiverDisposition::Indirect {
        receiver_type: ReceiverTypeRead::Present(receiver_type),
    } = &receiver_disposition
    {
        if let Some(candidates) = nodes_by_name.get(target_key) {
            let pool = resolution_pool(candidates, EdgeType::Calls, false);
            let mut typed = pool
                .iter()
                .filter(|c| container_matches(c.qualified_name.as_deref(), receiver_type));
            if let Some(first) = typed.next() {
                if typed.next().is_none() {
                    return TargetResolution::Resolved(first.node_uid.clone());
                }
            }
        }
    }

    // Simple function call: "classifyMedia". A C++ `field_expression` call
    // (`impl->Recover()`) also lands here — the extractor's target_key is the bare
    // method name. A UNIQUE name is evidence enough (RG-REQ-005-L02) even with an
    // indirect receiver; only the enclosing-class guess below is receiver-gated.
    if let Some(uid) = pick_unambiguous(nodes_by_name.get(target_key), EdgeType::Calls, false) {
        return TargetResolution::Resolved(uid);
    }

    // CPP-DECLARATORS-1 §2.3 (AMENDED by CALL-BINDING-RECEIVER-1 §2.3, F-CBR-004): when the
    // bare-name lookup is AMBIGUOUS, apply the enclosing-class preference — GATED to C/C++ edges
    // (non-C/C++ stays byte-identical) AND, per RG-REQ-005-L01, ONLY for a call KNOWN to be
    // RECEIVERLESS or an explicit `this`/self call. An INDIRECT receiver (`impl->`, `versions_->`)
    // — OR a carrier we could not READ (`Unreadable`, which could be hiding an indirect receiver)
    // — must NEVER bind to the caller's own enclosing class without receiver-type evidence: it
    // stays unresolved and counted (the honest remainder). A receiverless `Recover()` or
    // `this->Recover()` inside `leveldb::DBImpl::Open` still resolves to `leveldb::DBImpl::Recover`.
    let eligible_for_enclosing_pref = matches!(
        receiver_disposition,
        ReceiverDisposition::Receiverless | ReceiverDisposition::ExplicitThis
    );
    if is_c_family_extractor(extractor) && eligible_for_enclosing_pref {
        if let Some(candidates) = nodes_by_name.get(target_key) {
            let pool = resolution_pool(candidates, EdgeType::Calls, false);
            if pool.len() > 1 {
                let caller = nodes_by_uid.get(source_node_uid);
                if let Some(uid) = enclosing_class_preference(&pool, caller) {
                    return TargetResolution::Resolved(uid);
                }
            }
        }
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
                            if let Some(uid) =
                                pick_unambiguous(Some(&in_file), EdgeType::Calls, false)
                            {
                                return TargetResolution::Resolved(uid);
                            }
                        }
                    }
                }
            }
        }
    }

    TargetResolution::Unresolved
}

/// The namespace-import stage of CALLS resolution: `ns.member()` where `ns` is a file-level
/// namespace import of a module this snapshot resolves — the unique `member` declared in that
/// module's file. Extracted unchanged from `resolve_call_target` (PYTHON-RECEIVER-BINDING-1) so the
/// Python self/cls path can place it by the carrier's receiver-binding state; every other call
/// meets it first, exactly as before.
fn resolve_namespace_member(
    target_key: &str,
    source_node_uid: &str,
    nodes_by_name: &HashMap<String, Vec<ResolverNode>>,
    file_resolution: &HashMap<String, String>,
    import_bindings_by_file: Option<&HashMap<String, Vec<ImportBinding>>>,
    node_uid_to_file_uid: &HashMap<String, String>,
) -> Option<String> {
    if !target_key.contains('.') {
        return None;
    }
    let bindings_map = import_bindings_by_file?;
    let source_file_uid = node_uid_to_file_uid.get(source_node_uid)?;
    let bindings = bindings_map.get(source_file_uid)?;
    // Extract potential namespace prefix and member
    let (prefix, member) = target_key.split_once('.')?;
    // Check if prefix matches a namespace import
    let binding = bindings
        .iter()
        .find(|b| b.identifier == prefix && b.kind == ImportKind::Namespace)?;
    let resolved_file_uid =
        resolve_import_specifier_to_file(&binding.specifier, source_file_uid, file_resolution)?;
    // For nested member access like `fs.promises.readFile`, we only handle the immediate member
    // for now. Extract the first part of the member (before any dot).
    let immediate_member = member.split('.').next().unwrap_or(member);
    let candidates = nodes_by_name.get(immediate_member)?;
    let in_file: Vec<ResolverNode> = candidates
        .iter()
        .filter(|n| n.file_uid.as_deref() == Some(resolved_file_uid.as_str()))
        .cloned()
        .collect();
    pick_unambiguous(Some(&in_file), EdgeType::Calls, false)
}

// ── Python receiver certainty (PYTHON-RECEIVER-BINDING-1) ─────────

/// The method name of a Python two-part `self.<m>` / `cls.<m>` call key (`<m>` a non-empty name
/// with no further dot), or `None` for any other key.
fn python_self_call_method(target_key: &str) -> Option<&str> {
    let (receiver, method) = target_key.split_once('.')?;
    if (receiver == "self" || receiver == "cls") && !method.is_empty() && !method.contains('.') {
        Some(method)
    } else {
        None
    }
}

/// The self/cls carrier's `receiverBinding` — the extractor's lexical fact (D-PRB-CARRIER-1) — in
/// its evidence states. Absent and unreadable are DISTINCT: an absent key is decided by the edge's
/// writer, an unreadable one is never read as proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReceiverBindingRead {
    /// `"parameter"`: the receiver is the enclosing method's first positional parameter.
    Parameter,
    /// `"unproven"`: the extractor could not prove it.
    Unproven,
    /// The key is not on the carrier (an older writer, or a writer that omitted it).
    Absent,
    /// The key is present but is neither of the two strings (a non-string, an unknown string).
    Unreadable,
}

/// Read the carrier's `receiverBinding` key. Only called on a carrier `self_call_enclosing_class`
/// already accepted (so the metadata parses as a JSON object).
fn self_call_receiver_binding(metadata_json: Option<&str>) -> ReceiverBindingRead {
    let Some(value) =
        metadata_json.and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
    else {
        return ReceiverBindingRead::Unreadable;
    };
    match value.get("receiverBinding") {
        None => ReceiverBindingRead::Absent,
        Some(serde_json::Value::String(s)) if s == "parameter" => ReceiverBindingRead::Parameter,
        Some(serde_json::Value::String(s)) if s == "unproven" => ReceiverBindingRead::Unproven,
        Some(_) => ReceiverBindingRead::Unreadable,
    }
}

/// R1 (D-PRB-CARRIER-1 as corrected; D-PRB-SCOPE-1 amendment 2): decide a Python two-part
/// `self.<m>` / `cls.<m>` call. Every outcome is terminal.
///
/// - valid carrier, receiver proven a parameter → the class hierarchy alone (hit `static`, MRO
///   collision declined, miss unresolved `self_call_hierarchy_miss`); the namespace stage is never
///   consulted (the parameter shadows a file-level alias of that name);
/// - valid carrier without the key, written by exactly `python-core:0.1.0` → HEAD's order: the
///   namespace stage, then the hierarchy, with a TERMINAL miss;
/// - valid carrier whose receiver is unproven, unreadable, or whose missing key comes from any other
///   writer → neither stage; unresolved `self_call_receiver_unproven` with every candidate;
/// - no valid carrier → the namespace stage as today, else unresolved
///   `self_call_without_class_context`.
#[allow(clippy::too_many_arguments)] // threads the same index maps `resolve_call_target` holds
fn resolve_python_self_call(
    target_key: &str,
    method: &str,
    source_node_uid: &str,
    extractor: &str,
    metadata_json: Option<&str>,
    nodes_by_name: &HashMap<String, Vec<ResolverNode>>,
    nodes_by_qualified_name: &HashMap<String, Vec<ResolverNode>>,
    file_resolution: &HashMap<String, String>,
    import_bindings_by_file: Option<&HashMap<String, Vec<ImportBinding>>>,
    node_uid_to_file_uid: &HashMap<String, String>,
) -> TargetResolution {
    let pool = || sorted_candidate_labels(&name_only_pool(nodes_by_name.get(method)));
    let namespace = || {
        resolve_namespace_member(
            target_key,
            source_node_uid,
            nodes_by_name,
            file_resolution,
            import_bindings_by_file,
            node_uid_to_file_uid,
        )
    };
    let hierarchy = |enclosing_class: &str| -> TargetResolution {
        let outcome = match node_uid_to_file_uid.get(source_node_uid) {
            Some(caller_file) => resolve_self_call(
                enclosing_class,
                method,
                caller_file,
                nodes_by_name,
                nodes_by_qualified_name,
            ),
            None => SelfCallResolution::NoHit,
        };
        match outcome {
            SelfCallResolution::Resolved(uid) => TargetResolution::Resolved(uid),
            SelfCallResolution::AmbiguousMro(candidates) => {
                TargetResolution::SelfCallAmbiguousMro(candidates)
            }
            SelfCallResolution::NoHit => TargetResolution::SelfCallHierarchyMiss(pool()),
        }
    };

    let Some(enclosing_class) = self_call_enclosing_class(metadata_json) else {
        // R1b: no class could be walked. A file-level namespace alias of that name is the only
        // binding the index records for it (unchanged); otherwise unresolved with the pool.
        return match namespace() {
            Some(uid) => TargetResolution::Resolved(uid),
            None => TargetResolution::SelfCallWithoutClassContext(pool()),
        };
    };
    match self_call_receiver_binding(metadata_json) {
        ReceiverBindingRead::Parameter => hierarchy(&enclosing_class),
        ReceiverBindingRead::Absent if extractor == PYTHON_EXTRACTOR_BEFORE_RECEIVER_BINDING => {
            match namespace() {
                Some(uid) => TargetResolution::Resolved(uid),
                None => hierarchy(&enclosing_class),
            }
        }
        ReceiverBindingRead::Absent
        | ReceiverBindingRead::Unproven
        | ReceiverBindingRead::Unreadable => TargetResolution::SelfCallReceiverUnproven(pool()),
    }
}

/// The candidate pool a bare-name match of `method` would use (affinity- and declaration-filtered,
/// `resolution_pool`) — recorded as evidence when the resolver declines to bind by it.
fn name_only_pool(candidates: Option<&Vec<ResolverNode>>) -> Vec<&ResolverNode> {
    match candidates {
        Some(c) if !c.is_empty() => resolution_pool(c, EdgeType::Calls, false),
        _ => Vec::new(),
    }
}

/// A candidate's recorded label: its qualified name, or its name when none is stored.
fn candidate_label(node: &ResolverNode) -> String {
    node.qualified_name
        .clone()
        .unwrap_or_else(|| node.name.clone())
}

/// Every candidate's label, sorted (never deduplicated, never capped).
fn sorted_candidate_labels(pool: &[&ResolverNode]) -> Vec<String> {
    let mut labels: Vec<String> = pool.iter().map(|n| candidate_label(n)).collect();
    labels.sort();
    labels
}

/// The unresolved row of a call the resolver declined to bind by its name: the category stays
/// `categorize_unresolved_edge`'s; `nameOnlyCandidates` (every candidate, sorted) and
/// `nameOnlyReason` are merged into its `metadata_json`, the extractor's keys preserved.
fn declined_pool_row(
    edge: &ExtractedEdge,
    index: &ResolverIndex,
    pool: &[String],
    reason: &str,
) -> CategorizedUnresolvedEdge {
    let mut unresolved_edge = edge.clone();
    unresolved_edge.metadata_json = Some(merge_metadata(
        edge.metadata_json.as_deref(),
        [
            ("nameOnlyCandidates", serde_json::json!(pool)),
            ("nameOnlyReason", serde_json::json!(reason)),
        ],
    ));
    let category = categorize_unresolved_edge(&unresolved_edge);
    CategorizedUnresolvedEdge {
        edge: unresolved_edge,
        category,
        source_file_uid: index
            .node_uid_to_file_uid
            .get(&edge.source_node_uid)
            .cloned(),
    }
}

/// The metadata of an inferred name-only binding: `basis`, `receiver` and the pool of one.
fn inject_name_only_binding(
    metadata_json: Option<&str>,
    receiver: &str,
    candidate: &str,
) -> String {
    merge_metadata(
        metadata_json,
        [
            ("basis", serde_json::json!(RECEIVER_UNTYPED_NAME_ONLY_BASIS)),
            ("receiver", serde_json::json!(receiver)),
            ("candidates", serde_json::json!([candidate])),
        ],
    )
}

/// Additive merge of resolver-written keys into an edge's `metadata_json` object (the
/// `inject_mro_candidates` shape): the extractor's keys are preserved. A carrier that is absent
/// or not a JSON object contributes no keys (the edge's own evidence was already read — or found
/// absent — by the stage that decided it).
fn merge_metadata<const N: usize>(
    metadata_json: Option<&str>,
    keys: [(&str, serde_json::Value); N],
) -> String {
    let mut obj = metadata_json
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    for (k, v) in keys {
        obj.insert(k.to_string(), v);
    }
    serde_json::Value::Object(obj).to_string()
}

// ── Python self/cls call hierarchy binding (PYTHON-SELF-BINDING-1) ─

/// Outcome of the Python self-call hierarchy walk.
enum SelfCallResolution {
    /// Exactly one METHOD named `<m>` was declared at the first non-empty BFS depth.
    Resolved(String),
    /// Two or more ancestors at ONE depth declare `<m>` — a genuine MRO ambiguity. Carries the
    /// sorted candidate `<Class>.<method>` labels (persisted as evidence, never silently picked).
    AmbiguousMro(Vec<String>),
    /// The method is not declared anywhere in the caller's own-class superclass closure (or the
    /// own class is not unique in the caller's file). PYTHON-RECEIVER-BINDING-1: terminal — the
    /// caller records `SelfCallHierarchyMiss` with the bare-name pool, never the fallback.
    NoHit,
}

/// Read the Python self/cls call carrier's enclosing class name from an edge's `metadata_json`.
///
/// `Some("C")` ONLY when the carrier has `selfCall == true` (a real boolean) AND `enclosingClass`
/// is a non-empty string. `None` for an absent carrier OR any malformed shape (a non-boolean
/// `selfCall`, a missing/non-string/empty `enclosingClass`): a malformed carrier never binds
/// through the hierarchy (Q1's honesty rule). PYTHON-RECEIVER-BINDING-1: both are the "no class
/// context" state of R1b — the call meets the namespace stage, else stays unresolved with its pool
/// (`self_call_without_class_context`); absent and malformed are the same evidence state there.
fn self_call_enclosing_class(metadata_json: Option<&str>) -> Option<String> {
    let raw = metadata_json?;
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    if value.get("selfCall") != Some(&serde_json::Value::Bool(true)) {
        return None;
    }
    let class = value.get("enclosingClass")?.as_str()?;
    if class.is_empty() {
        None
    } else {
        Some(class.to_string())
    }
}

/// Bind a Python `self.<m>()` / `cls.<m>()` call by breadth-first walk over the caller's own class
/// and its superclass closure (RG-REQ-005-L03).
///
/// Depth 0 is the own class: the UNIQUE CLASS node named `enclosing_class` in the caller's file
/// (0 or >1 such nodes → no binding). At each depth the hits are the METHOD nodes whose qualified
/// name is `<Class>.<method>` and whose file is that class node's file. The FIRST depth with
/// exactly one hit binds; two or more hits at that depth is an MRO collision (declined, named);
/// no hit at any depth is `NoHit`. Descent resolves each stored superclass SIMPLE name to a unique
/// CLASS node repo-wide (0 or >1 → that branch ends — cross-package and same-name bases stay
/// unresolved, never guessed); a visited set and a depth cap bound the walk. Deterministic: the
/// outcome depends only on the set of hits at each depth, not on map iteration order.
fn resolve_self_call(
    enclosing_class: &str,
    method: &str,
    caller_file: &str,
    nodes_by_name: &HashMap<String, Vec<ResolverNode>>,
    nodes_by_qualified_name: &HashMap<String, Vec<ResolverNode>>,
) -> SelfCallResolution {
    const DEPTH_CAP: usize = 16;

    let Some(own) = unique_class_in_file(enclosing_class, caller_file, nodes_by_name) else {
        return SelfCallResolution::NoHit;
    };

    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    visited.insert(own.node_uid.clone());
    let mut frontier: Vec<ResolverNode> = vec![own];
    let mut depth = 0;

    while !frontier.is_empty() && depth <= DEPTH_CAP {
        // Collect the METHOD hits declared on the classes at THIS depth. `(label, uid)` where
        // `label` is the method node's qualified name (`<Class>.<method>`).
        let mut hits: Vec<(String, String)> = Vec::new();
        for class_node in &frontier {
            let class_name = class_node
                .qualified_name
                .as_deref()
                .unwrap_or(class_node.name.as_str());
            let class_file = class_node.file_uid.as_deref();
            let qualified = format!("{class_name}.{method}");
            if let Some(candidates) = nodes_by_qualified_name.get(&qualified) {
                for candidate in candidates {
                    if candidate.subtype.as_deref() == Some("METHOD")
                        && candidate.file_uid.as_deref() == class_file
                    {
                        let label = candidate
                            .qualified_name
                            .clone()
                            .unwrap_or_else(|| qualified.clone());
                        hits.push((label, candidate.node_uid.clone()));
                    }
                }
            }
        }

        match hits.len() {
            0 => {}
            1 => return SelfCallResolution::Resolved(hits.into_iter().next().unwrap().1),
            _ => {
                let mut candidates: Vec<String> =
                    hits.into_iter().map(|(label, _)| label).collect();
                candidates.sort();
                return SelfCallResolution::AmbiguousMro(candidates);
            }
        }

        // Descend to the superclasses of the current frontier (unique CLASS node per simple name).
        let mut next: Vec<ResolverNode> = Vec::new();
        for class_node in &frontier {
            for base in &class_node.superclasses {
                if let Some(base_node) = unique_class_by_name(base, nodes_by_name) {
                    if visited.insert(base_node.node_uid.clone()) {
                        next.push(base_node);
                    }
                }
            }
        }
        frontier = next;
        depth += 1;
    }

    SelfCallResolution::NoHit
}

/// The UNIQUE CLASS node named `name` in `file`, or `None` when zero or more than one exists.
fn unique_class_in_file(
    name: &str,
    file: &str,
    nodes_by_name: &HashMap<String, Vec<ResolverNode>>,
) -> Option<ResolverNode> {
    let mut matches =
        nodes_by_name.get(name).into_iter().flatten().filter(|n| {
            n.subtype.as_deref() == Some("CLASS") && n.file_uid.as_deref() == Some(file)
        });
    let first = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(first.clone())
}

/// The UNIQUE CLASS node named `name` anywhere in the snapshot, or `None` when zero or more than
/// one exists (a same-name or cross-package base ends the walk branch, never a guess).
fn unique_class_by_name(
    name: &str,
    nodes_by_name: &HashMap<String, Vec<ResolverNode>>,
) -> Option<ResolverNode> {
    let mut matches = nodes_by_name
        .get(name)
        .into_iter()
        .flatten()
        .filter(|n| n.subtype.as_deref() == Some("CLASS"));
    let first = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(first.clone())
}

/// Add `mroCandidates` (the sorted collision labels) to an edge's `metadata_json`, preserving the
/// extractor's existing keys (the self-call carrier). The first resolver-written metadata key;
/// mirrors `orchestrator::inject_import_type_only`'s additive merge. Used only for the
/// `SelfCallAmbiguousMro` terminal.
fn inject_mro_candidates(metadata_json: Option<&str>, candidates: &[String]) -> String {
    let mut obj = metadata_json
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    obj.insert("mroCandidates".to_string(), serde_json::json!(candidates));
    serde_json::Value::Object(obj).to_string()
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
    extractor: &str,
) -> Option<String> {
    // CPP-DECLARATORS-1 §2.6: only a C/C++ `Implements` edge widens affinity to CLASS/STRUCT.
    let c_family_implements = edge_type == EdgeType::Implements && is_c_family_extractor(extractor);
    pick_unambiguous(
        nodes_by_name.get(target_key),
        edge_type,
        c_family_implements,
    )
}

// ── Affinity filtering + singleton check ─────────────────────────

/// Apply declaration-space affinity filtering, drop bodiless DECLARATIONS
/// (CPP-DECLARATORS-1 §2.3), then check for an unambiguous singleton result.
///
/// `c_family_implements` (spec §2.6): the edge is a C/C++ `Implements` (inheritance)
/// edge, so its affinity admits CLASS/STRUCT — see [`filter_by_edge_affinity`]. Always
/// `false` for Calls and for non-C/C++ Implements (byte-identical to before).
///
/// Mirror of `pickUnambiguous` from `repo-indexer.ts:2854`.
fn pick_unambiguous(
    candidates: Option<&Vec<ResolverNode>>,
    edge_type: EdgeType,
    c_family_implements: bool,
) -> Option<String> {
    let candidates = candidates?;
    if candidates.is_empty() {
        return None;
    }
    let pool = resolution_pool(candidates, edge_type, c_family_implements);
    if pool.len() == 1 {
        return Some(pool[0].node_uid.clone());
    }
    None
}

/// The candidate pool a name lookup resolves against: the affinity-filtered set with
/// bodiless DECLARATIONS removed (CPP-DECLARATORS-1 §2.3). Definitions are preferred —
/// a class declared 70× + defined 1× collapses to the one definition, and a header
/// method prototype no longer makes its out-of-line definition ambiguous. Falls back to
/// the full affinity set only when EVERY surviving candidate is a declaration (a symbol
/// with no definition indexed still resolves unambiguously to its lone prototype rather
/// than silently dropping the edge). Shared by [`pick_unambiguous`] (singleton test) and
/// the Calls enclosing-class preference (which needs the SAME pool to disambiguate).
fn resolution_pool(
    candidates: &[ResolverNode],
    edge_type: EdgeType,
    c_family_implements: bool,
) -> Vec<&ResolverNode> {
    let filtered = filter_by_edge_affinity(candidates, edge_type, c_family_implements);
    let non_decl: Vec<&ResolverNode> = filtered
        .iter()
        .copied()
        .filter(|n| !n.forward_decl)
        .collect();
    if non_decl.is_empty() {
        filtered
    } else {
        non_decl
    }
}

/// CPP-DECLARATORS-1 §2.3 (AMENDED): the enclosing-class preference for an otherwise
/// ambiguous C++ method call. When the CALLER's qualified-name container
/// (`leveldb::DBImpl::Open` → `leveldb::DBImpl`) matches EXACTLY ONE candidate's own
/// container, that candidate is the call target. A call from OUTSIDE the class to a name
/// defined in several classes (`impl->Recover` from `leveldb::DB::Open`) matches none and
/// stays unresolved — the honest remainder, counted as the C++ no-resolver gap. `caller`
/// is the source node; `pool` is the already affinity+decl-filtered ambiguous set.
fn enclosing_class_preference(
    pool: &[&ResolverNode],
    caller: Option<&ResolverNode>,
) -> Option<String> {
    let caller_container = qualified_container(caller?.qualified_name.as_deref()?)?;
    let mut matches = pool.iter().filter(|c| {
        c.qualified_name.as_deref().and_then(qualified_container) == Some(caller_container)
    });
    let first = matches.next()?;
    if matches.next().is_none() {
        Some(first.node_uid.clone())
    } else {
        None
    }
}

/// The container of a `::`-qualified name — everything before the LAST `::` segment
/// (`leveldb::DBImpl::Recover` → `leveldb::DBImpl`). `None` when the name has no `::`
/// (a free function has no enclosing class).
fn qualified_container(qualified_name: &str) -> Option<&str> {
    qualified_name.rfind("::").map(|i| &qualified_name[..i])
}

/// CALL-BINDING-RECEIVER-1 §2.2 / §4 (F-CBR-004): whether a C++ CALLS edge's receiver disposition
/// could be READ, and — when it could — what it was. Mirrors the [`ForwardDeclRead`] honesty
/// pattern: a carrier that is PRESENT but UNREADABLE (did not parse as JSON, or whose `receiver`
/// value is not a string) is a NAMED unknown, NEVER silently collapsed into "receiverless"
/// (STANDING HONESTY RULE 1 — never swallow a fallible read whose result is consumed). This
/// matters because the C-family enclosing-class preference is applied ONLY to a call KNOWN to be
/// receiverless or an explicit `this`/self call; an unreadable carrier could be hiding an INDIRECT
/// receiver, so it must NOT be eligible for that preference — otherwise a corrupt carrier recreates
/// the RC-1 self-binding the slice exists to remove (RG-REQ-005-L01).
/// CALL-BINDING-RECEIVER-1 §2.2 (F-CBR-007): the THREE-STATE read of a CALLS edge's `receiverType`
/// field. Absence and malformed presence are DISTINCT states, never collapsed to one "no type":
/// a future reader must be able to tell a legitimately-unavailable cross-file type from corrupt
/// evidence (STANDING HONESTY RULE — `null`/unknown is never silently the same as a failed read).
/// Only `Present` carries a usable type, and it is reachable ONLY through `ReceiverDisposition::
/// Indirect` (below), so "a usable receiver type without an indirect receiver" is unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ReceiverTypeRead {
    /// No `receiverType` key — a legitimate INDIRECT receiver whose declared type the per-file
    /// C++ extractor could not resolve (a cross-file member such as `versions_`). Not malformed;
    /// there is simply no in-file type evidence. Binds nothing; resolves only by a unique name.
    Absent,
    /// `receiverType: "<type>"` — a non-empty string. Usable receiver-type evidence: the
    /// receiver-type binding stage keeps the candidate whose container IS this type.
    Present(String),
    /// `receiverType` PRESENT but not a non-empty string (a non-string value, or an empty string)
    /// — MALFORMED evidence. Distinct from `Absent` so a future edit never mistakes a corrupt
    /// carrier for known-unavailable evidence (F-CBR-007). Never usable, never a binding.
    Unreadable,
}

/// CALL-BINDING-RECEIVER-1 §2.2: the call-receiver disposition read from a CALLS edge's
/// `metadata_json`. The receiver's declared type is CARRIED inside `Indirect` (F-CBR-007), never
/// beside the disposition as an independent `Option`, so the invariant "receiver-type evidence
/// exists only for an indirect receiver" is enforced by the type, not by a match arm. The
/// receiver-type binding stage consumes the carried type; the enclosing-class preference runs
/// ONLY for `Receiverless` or `ExplicitThis`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ReceiverDisposition {
    /// No `receiver` recorded — carrier absent (every non-C++ call, and a C++ receiverless
    /// `foo()`), or valid JSON with no (or an empty) `receiver` string. A KNOWN receiverless call:
    /// eligible for the enclosing-class preference.
    Receiverless,
    /// `receiver: "this"` — an explicit self call. Eligible for the enclosing-class preference
    /// (RG-REQ-005-L01 admits explicit self).
    ExplicitThis,
    /// `receiver: "<name>"` (a member/local other than `this`) — an INDIRECT receiver. NEVER
    /// eligible for the enclosing-class preference; it binds only on `Present` receiver-type
    /// evidence or stays unresolved and counted. The carried `ReceiverTypeRead` distinguishes a
    /// legitimately-absent cross-file type from malformed type evidence.
    Indirect { receiver_type: ReceiverTypeRead },
    /// The carrier was PRESENT but UNREADABLE (did not parse; `receiver` was present but not a
    /// string; or a `receiverType` appeared beside a non-indirect receiver — an internally
    /// inconsistent carrier the real extractor never emits). Disposition UNKNOWN → conservatively
    /// NOT eligible for the enclosing-class preference, exactly like an indirect receiver — a
    /// corrupt carrier never fabricates a self-binding.
    Unreadable,
}

fn call_receiver_info(metadata_json: Option<&str>) -> ReceiverDisposition {
    // No carrier at all ⇒ a KNOWN receiverless call (non-C++ calls carry none; a C++ receiverless
    // `foo()` stamps none). A KNOWN read, not a swallowed error.
    let Some(raw) = metadata_json else {
        return ReceiverDisposition::Receiverless;
    };
    // A carrier that does not parse is UNREADABLE — a NAMED unknown, never silently receiverless
    // (STANDING HONESTY RULE 1): an unreadable carrier could hide an indirect receiver, so it must
    // not become eligible for the enclosing-class preference (F-CBR-004).
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return ReceiverDisposition::Unreadable;
    };
    // F-CBR-015: the real extractor always emits a JSON OBJECT carrier. A syntactically VALID but
    // NON-object root (`null`, `[]`, `"x"`, `3`) has no `receiver`/`receiverType` keys, so the
    // `.get(...)` reads below would each answer `None` and the carrier would masquerade as a KNOWN
    // receiverless call — eligible for the enclosing-class preference, the exact self-binding
    // fabrication this slice forbids. Such a carrier is UNREADABLE (disposition unknown, never
    // receiverless), completing the carrier taxonomy: absent, valid-object, unparseable, valid-
    // non-object. Like every other `Unreadable`, it is excluded from the enclosing-class preference.
    if !value.is_object() {
        return ReceiverDisposition::Unreadable;
    }
    // F-CBR-007: THREE-STATE read of `receiverType` — absent, present-and-usable, or present-and-
    // malformed. The three states are kept distinct all the way to the disposition.
    let receiver_type = match value.get("receiverType") {
        None => ReceiverTypeRead::Absent,
        Some(serde_json::Value::String(s)) if !s.is_empty() => {
            ReceiverTypeRead::Present(s.to_string())
        }
        // Present but not a non-empty string (a non-string value, or an empty string) ⇒ malformed.
        Some(_) => ReceiverTypeRead::Unreadable,
    };
    // F-CBR-006/-007 (RG-REQ-005-L01/-L02): the receiver disposition and its type are ONE carrier,
    // read together — never two independently trusted facts. The real C++ extractor stamps
    // `receiverType` ONLY beside a bare-name (indirect) receiver. A `receiverType` present beside a
    // receiverless, explicit-`this`, or non-string receiver is an INTERNALLY INCONSISTENT carrier we
    // cannot trust — including its claim to be receiverless — so it becomes `Unreadable` (which also
    // blocks the enclosing-class preference). An INDIRECT receiver carries its three-state type read:
    // `Present` drives the receiver-type binding stage; `Absent` (a cross-file member) and
    // `Unreadable` (corrupt type evidence) both bind nothing and are excluded from the enclosing-
    // class guess, resolving only by a unique name — but they stay DISTINCT so corrupt evidence is
    // never mistaken for known-unavailable evidence.
    match value.get("receiver") {
        // Valid JSON without a `receiver` key ⇒ a KNOWN receiverless call, UNLESS a `receiverType`
        // rode along (inconsistent carrier ⇒ Unreadable).
        None => receiverless_unless_type_present(ReceiverDisposition::Receiverless, &receiver_type),
        Some(serde_json::Value::String(s)) if s.is_empty() => {
            receiverless_unless_type_present(ReceiverDisposition::Receiverless, &receiver_type)
        }
        Some(serde_json::Value::String(s)) if s == "this" => {
            receiverless_unless_type_present(ReceiverDisposition::ExplicitThis, &receiver_type)
        }
        Some(serde_json::Value::String(_)) => ReceiverDisposition::Indirect { receiver_type },
        // `receiver` present but NOT a string ⇒ a CORRUPT value, distinct from receiverless.
        Some(_) => ReceiverDisposition::Unreadable,
    }
}

/// F-CBR-006/-007: for a NON-indirect receiver (receiverless or explicit `this`), any `receiverType`
/// present at all — usable-looking (`Present`) or malformed (`Unreadable`) — is an internally
/// inconsistent carrier the real extractor never emits, so the whole disposition degrades to
/// `Unreadable`. Only an `Absent` type leaves the non-indirect disposition intact.
fn receiverless_unless_type_present(
    disposition: ReceiverDisposition,
    receiver_type: &ReceiverTypeRead,
) -> ReceiverDisposition {
    match receiver_type {
        ReceiverTypeRead::Absent => disposition,
        ReceiverTypeRead::Present(_) | ReceiverTypeRead::Unreadable => {
            ReceiverDisposition::Unreadable
        }
    }
}

/// CALL-BINDING-RECEIVER-1 §2.2: does a candidate method's container match the receiver's
/// declared type? A hit means the candidate's qualified container equals the receiver type
/// EXACTLY (after normalizing a leading `::`), or — ONLY when the receiver type is written
/// unqualified — the container's terminal segment is that name
/// (`"VersionSet"` names `"leveldb::VersionSet"`, i.e. `container.ends_with("::VersionSet")`).
///
/// A QUALIFIED receiver type never matches a shorter container by suffix: receiver evidence
/// `"a::Foo"` must NOT bind a candidate `"Foo::run"`, because nothing shows `a::Foo` and a
/// global `Foo` denote the same type (F-CBR-005; RG-REQ-005-L02 — never widen what counts as
/// evidence). The receiver-type stage requires a UNIQUE surviving candidate, so an exact/
/// unqualified-suffix match here can only ever narrow, never invent, a target.
fn container_matches(qualified_name: Option<&str>, receiver_type: &str) -> bool {
    let Some(container) = qualified_name.and_then(qualified_container) else {
        return false;
    };
    let rt = receiver_type.trim_start_matches("::");
    if rt.is_empty() {
        return false;
    }
    if container == rt {
        return true;
    }
    // Suffix matching is admitted ONLY for an UNQUALIFIED receiver type. A qualified receiver
    // type must match the container exactly (handled above); it never binds to a shorter,
    // unrelated container by suffix.
    if rt.contains("::") {
        return false;
    }
    container.ends_with(&format!("::{rt}"))
}

/// Filter candidates by declaration-space affinity. Returns only
/// candidates in the correct space for the edge type.
///
/// `c_family_implements` (CPP-DECLARATORS-1 §2.6, operator ruling A): when the
/// `Implements` edge comes from the C/C++ extractor, C++ "implements" IS inheritance and
/// its base is a CLASS/STRUCT node, so those subtypes are admitted; for every other
/// language `Implements` keeps its interface-only rule (byte-identical — regression test
/// `affinity_implements_filters_to_interface`).
///
/// Mirror of `filterByEdgeAffinity` from `repo-indexer.ts:3049`.
pub fn filter_by_edge_affinity(
    candidates: &[ResolverNode],
    edge_type: EdgeType,
    c_family_implements: bool,
) -> Vec<&ResolverNode> {
    match edge_type {
        EdgeType::Instantiates => candidates
            .iter()
            .filter(|n| n.subtype.as_deref() == Some("CLASS"))
            .collect(),
        EdgeType::Implements if c_family_implements => candidates
            .iter()
            .filter(|n| matches!(n.subtype.as_deref(), Some("CLASS") | Some("STRUCT")))
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
            forward_decl: false,
            superclasses: Vec::new(),
        }
    }

    /// A resolver node with an explicit `qualified_name` and `forward_decl` flag —
    /// CPP-DECLARATORS-1 §2.3 tests (decl-vs-definition preference; enclosing class).
    fn make_qn_node(
        uid: &str,
        name: &str,
        qualified_name: &str,
        subtype: Option<&str>,
        forward_decl: bool,
    ) -> ResolverNode {
        ResolverNode {
            node_uid: uid.into(),
            stable_key: uid.into(),
            name: name.into(),
            qualified_name: Some(qualified_name.into()),
            kind: "SYMBOL".into(),
            subtype: subtype.map(|s| s.into()),
            file_uid: None,
            forward_decl,
            superclasses: Vec::new(),
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

    // ── classify_forward_decl (CPP-DECLARATORS-1 §2.3, honesty rule 1) ──

    #[test]
    fn forward_decl_classification_reads_the_stamped_key() {
        use ForwardDeclRead::*;
        // Stamped `forward_decl: true` ⇒ a bodiless declaration.
        assert_eq!(
            classify_forward_decl(Some(r#"{"forward_decl":true}"#)),
            ForwardDecl
        );
        // Stamped `forward_decl: false` ⇒ a definition.
        assert_eq!(
            classify_forward_decl(Some(r#"{"forward_decl":false}"#)),
            Definition
        );
        // Valid JSON without the key (any other node) ⇒ a definition (never stamped).
        assert_eq!(
            classify_forward_decl(Some(r#"{"macro_tokens":["URI_FUNC"]}"#)),
            Definition
        );
        // No carrier at all ⇒ a definition.
        assert_eq!(classify_forward_decl(None), Definition);
    }

    #[test]
    fn forward_decl_malformed_metadata_is_unreadable_not_a_definition() {
        // Operator ruling 2026-09-07: a malformed carrier is a NAMED error at the
        // classification site, NEVER "treated as a definition" (STANDING HONESTY RULE 1).
        // The prior `serde_json::from_str(...).ok()` collapsed it into `false` (definition).
        use ForwardDeclRead::*;
        // Present but unparseable JSON ⇒ Unreadable, DISTINCT from a definition.
        assert_eq!(classify_forward_decl(Some("{not json")), Unreadable);
        // Present, valid JSON, but `forward_decl` is the wrong type ⇒ Unreadable (corrupt value).
        assert_eq!(
            classify_forward_decl(Some(r#"{"forward_decl":"yes"}"#)),
            Unreadable
        );
        // The bool collapse used by the resolver index build maps Unreadable to a DECLARATION
        // (filtered/demoted), never a definition — a corrupt node cannot masquerade as the
        // authoritative definition.
        assert!(metadata_forward_decl(Some("{not json")));
        assert!(metadata_forward_decl(Some(r#"{"forward_decl":"yes"}"#)));
        // And the readable cases still collapse correctly.
        assert!(metadata_forward_decl(Some(r#"{"forward_decl":true}"#)));
        assert!(!metadata_forward_decl(Some(r#"{"forward_decl":false}"#)));
        assert!(!metadata_forward_decl(None));
    }

    // ── filter_by_edge_affinity ──────────────────────────────

    #[test]
    fn affinity_instantiates_filters_to_class() {
        let nodes = vec![
            make_node("n1", "k1", "Foo", Some("CLASS"), None),
            make_node("n2", "k2", "Foo", Some("INTERFACE"), None),
        ];
        let filtered = filter_by_edge_affinity(&nodes, EdgeType::Instantiates, false);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].node_uid, "n1");
    }

    #[test]
    fn affinity_implements_filters_to_interface() {
        let nodes = vec![
            make_node("n1", "k1", "Bar", Some("CLASS"), None),
            make_node("n2", "k2", "Bar", Some("INTERFACE"), None),
        ];
        let filtered = filter_by_edge_affinity(&nodes, EdgeType::Implements, false);
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
        let filtered = filter_by_edge_affinity(&nodes, EdgeType::Calls, false);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].node_uid, "n1");
    }

    // ── CPP-DECLARATORS-1 §2.6: C/C++ Implements admits CLASS/STRUCT ──

    #[test]
    fn affinity_cpp_implements_admits_class_and_struct() {
        // Operator ruling A: a C/C++ inheritance `Implements` edge admits CLASS/STRUCT
        // bases (no C++ INTERFACE subtype exists) — the whole reason inheritance edges
        // resolve now.
        let nodes = vec![
            make_node("n1", "k1", "Base", Some("CLASS"), None),
            make_node("n2", "k2", "Base", Some("STRUCT"), None),
            make_node("n3", "k3", "Base", Some("FUNCTION"), None),
        ];
        let filtered = filter_by_edge_affinity(&nodes, EdgeType::Implements, true);
        assert_eq!(
            filtered.len(),
            2,
            "CLASS + STRUCT admitted, FUNCTION dropped"
        );
        assert!(filtered.iter().all(|n| n.node_uid != "n3"));
    }

    #[test]
    fn affinity_non_cpp_implements_stays_interface_only_regression() {
        // The frozen non-C/C++ invariant: a TS `implements` edge (c_family_implements =
        // false) still admits INTERFACE ONLY — CLASS is dropped, byte-identical to before.
        let nodes = vec![
            make_node("n1", "k1", "Bar", Some("CLASS"), None),
            make_node("n2", "k2", "Bar", Some("INTERFACE"), None),
        ];
        let filtered = filter_by_edge_affinity(&nodes, EdgeType::Implements, false);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].subtype.as_deref(), Some("INTERFACE"));
    }

    // ── CPP-DECLARATORS-1 §2.3: forward-decl preference in pick_unambiguous ──

    #[test]
    fn one_definition_plus_n_forward_decls_resolves_to_the_definition() {
        // A class declared 70× (forward_decl) + defined once → the affinity+decl pool
        // collapses to the single definition, so the C++ inheritance edge resolves.
        let mut candidates = vec![make_qn_node(
            "def",
            "CGHeroInstance",
            "CGHeroInstance",
            Some("CLASS"),
            false,
        )];
        for i in 0..70 {
            candidates.push(make_qn_node(
                &format!("decl{i}"),
                "CGHeroInstance",
                "CGHeroInstance",
                Some("CLASS"),
                true,
            ));
        }
        // C/C++ Implements admission on.
        assert_eq!(
            pick_unambiguous(Some(&candidates), EdgeType::Implements, true),
            Some("def".to_string()),
        );
    }

    #[test]
    fn two_definitions_stay_ambiguous() {
        // Two genuine definitions (no decl to filter) → honestly unresolved.
        let candidates = vec![
            make_qn_node("d1", "Foo", "a::Foo", Some("CLASS"), false),
            make_qn_node("d2", "Foo", "b::Foo", Some("CLASS"), false),
        ];
        assert_eq!(
            pick_unambiguous(Some(&candidates), EdgeType::Implements, true),
            None,
        );
    }

    #[test]
    fn method_call_prototype_plus_definition_resolves_to_definition() {
        // leveldb: `NewDB` declared in db_impl.h (prototype) + defined in .cc → 2 → 1.
        let candidates = vec![
            make_qn_node("proto", "NewDB", "leveldb::NewDB", Some("METHOD"), true),
            make_qn_node("def", "NewDB", "leveldb::NewDB", Some("METHOD"), false),
        ];
        assert_eq!(
            pick_unambiguous(Some(&candidates), EdgeType::Calls, false),
            Some("def".to_string()),
        );
    }

    #[test]
    fn lone_declaration_still_resolves_when_no_definition_indexed() {
        // A symbol with ONLY a prototype (no definition in the snapshot) still resolves
        // unambiguously to its lone declaration — the decl filter falls back to the full
        // set when nothing non-decl remains, rather than dropping the edge.
        let candidates = vec![make_qn_node(
            "proto",
            "OnlyDecl",
            "ns::OnlyDecl",
            Some("METHOD"),
            true,
        )];
        assert_eq!(
            pick_unambiguous(Some(&candidates), EdgeType::Calls, false),
            Some("proto".to_string()),
        );
    }

    // ── CALL-BINDING-RECEIVER-1 §2.5: receiver-typed call binding ──
    //
    // The five §2.5 tests below (indirect-binds, explicit-this, unknown-type, receiverless-
    // outside, never-self) REPLACE the two CPP-DECLARATORS-1 §2.3 defect-pinning tests
    // (`enclosing_class_preference_resolves_ambiguous_method_by_caller_container`, which
    // encoded a fictional `leveldb::DBImpl::Open` caller, and
    // `enclosing_class_preference_leaves_outside_caller_unresolved`, which encoded the DROPPED
    // outside caller as intended). Three review-driven regression tests follow them:
    // `receiver_binding_malformed_metadata_stays_unresolved` (F-CBR-004),
    // `receiver_binding_qualified_receiver_type_never_matches_a_shorter_container` (F-CBR-005),
    // `receiver_binding_receiver_type_without_indirect_receiver_is_untrusted` (F-CBR-006 — a
    // `receiverType` beside an absent/non-string receiver is an inconsistent carrier and binds
    // nothing), and `receiver_binding_malformed_receiver_type_on_indirect_receiver_is_unreadable`
    // (F-CBR-007 — a malformed `receiverType` beside a valid indirect receiver is `Unreadable`, not
    // silently absent, and binds nothing). RG-REQ-005-L01: an indirect receiver binds to its
    // declared type or stays unresolved — never to the caller's own class by default.

    /// Two `A`/`B` fixture from the shared resolver's point of view: `impl->Recover()` with
    /// `impl : DBImpl` binds to `DBImpl::Recover`, not `VersionSet::Recover`. (RG-REQ-005-L02:
    /// receiver-type evidence selects the one candidate.)
    #[test]
    fn receiver_binding_indirect_receiver_binds_to_its_declared_type() {
        let dbimpl = make_qn_node(
            "r_dbimpl",
            "Recover",
            "leveldb::DBImpl::Recover",
            Some("METHOD"),
            false,
        );
        let versionset = make_qn_node(
            "r_vs",
            "Recover",
            "leveldb::VersionSet::Recover",
            Some("METHOD"),
            false,
        );
        let caller = make_qn_node("open", "Open", "leveldb::DB::Open", Some("METHOD"), false);

        let mut index = empty_index();
        index
            .nodes_by_name
            .insert("Recover".into(), vec![dbimpl, versionset]);
        index.nodes_by_uid.insert("open".into(), caller);

        let mut edge = make_edge("e1", "Recover", EdgeType::Calls);
        edge.extractor = "cpp-core:0.1.0".into();
        edge.source_node_uid = "open".into();
        edge.metadata_json =
            Some(r#"{"calleeName":"Recover","receiver":"impl","receiverType":"DBImpl"}"#.into());
        let result = resolve_edges(&[edge], &index, None);
        assert_eq!(result.resolved.len(), 1, "receiver-typed call binds");
        assert_eq!(result.resolved[0].target_node_uid, "r_dbimpl");
    }

    /// An explicit `this->Recover()` inside `leveldb::DBImpl::Open` legitimately binds to the
    /// enclosing class (RG-REQ-005-L01 admits explicit self).
    #[test]
    fn receiver_binding_explicit_this_binds_to_enclosing_class() {
        let dbimpl = make_qn_node(
            "r_dbimpl",
            "Recover",
            "leveldb::DBImpl::Recover",
            Some("METHOD"),
            false,
        );
        let versionset = make_qn_node(
            "r_vs",
            "Recover",
            "leveldb::VersionSet::Recover",
            Some("METHOD"),
            false,
        );
        let caller = make_qn_node(
            "open",
            "Open",
            "leveldb::DBImpl::Open",
            Some("METHOD"),
            false,
        );

        let mut index = empty_index();
        index
            .nodes_by_name
            .insert("Recover".into(), vec![dbimpl, versionset]);
        index.nodes_by_uid.insert("open".into(), caller);

        let mut edge = make_edge("e1", "Recover", EdgeType::Calls);
        edge.extractor = "cpp-core:0.1.0".into();
        edge.source_node_uid = "open".into();
        edge.metadata_json = Some(r#"{"calleeName":"Recover","receiver":"this"}"#.into());
        let result = resolve_edges(&[edge], &index, None);
        assert_eq!(
            result.resolved.len(),
            1,
            "explicit this binds to enclosing class"
        );
        assert_eq!(result.resolved[0].target_node_uid, "r_dbimpl");
    }

    /// An indirect receiver whose declared type is not indexed (`x_ : X`, X absent) stays
    /// unresolved and counted — never bound to an unrelated same-named method (RG-REQ-001-L03).
    #[test]
    fn receiver_binding_unknown_receiver_type_stays_unresolved_and_counted() {
        let a = make_qn_node("a", "run", "A::run", Some("METHOD"), false);
        let b = make_qn_node("b", "run", "B::run", Some("METHOD"), false);
        let caller = make_qn_node("br", "run", "B::run", Some("METHOD"), false);

        let mut index = empty_index();
        index.nodes_by_name.insert("run".into(), vec![a, b]);
        index.nodes_by_uid.insert("br".into(), caller);

        let mut edge = make_edge("e1", "run", EdgeType::Calls);
        edge.extractor = "cpp-core:0.1.0".into();
        edge.source_node_uid = "br".into();
        edge.metadata_json =
            Some(r#"{"calleeName":"run","receiver":"x_","receiverType":"X"}"#.into());
        let result = resolve_edges(&[edge], &index, None);
        assert_eq!(
            result.resolved.len(),
            0,
            "unknown receiver type does not bind"
        );
        assert_eq!(result.still_unresolved.len(), 1, "the call is counted");
    }

    /// A RECEIVERLESS ambiguous call from OUTSIDE the candidate classes still uses the
    /// enclosing-class preference (RG-REQ-005-L02) and honestly stays unresolved when the
    /// caller's container matches none of them.
    #[test]
    fn receiver_binding_receiverless_ambiguous_call_outside_class_stays_unresolved() {
        let dbimpl = make_qn_node(
            "r_dbimpl",
            "Recover",
            "leveldb::DBImpl::Recover",
            Some("METHOD"),
            false,
        );
        let versionset = make_qn_node(
            "r_vs",
            "Recover",
            "leveldb::VersionSet::Recover",
            Some("METHOD"),
            false,
        );
        let caller = make_qn_node("open", "Open", "leveldb::DB::Open", Some("METHOD"), false);

        let mut index = empty_index();
        index
            .nodes_by_name
            .insert("Recover".into(), vec![dbimpl, versionset]);
        index.nodes_by_uid.insert("open".into(), caller);

        let mut edge = make_edge("e1", "Recover", EdgeType::Calls);
        edge.extractor = "cpp-core:0.1.0".into();
        edge.source_node_uid = "open".into();
        // Receiverless call: no `receiver` key at all.
        edge.metadata_json = Some(r#"{"calleeName":"Recover"}"#.into());
        let result = resolve_edges(&[edge], &index, None);
        assert_eq!(result.resolved.len(), 0);
        assert_eq!(result.still_unresolved.len(), 1);
    }

    /// The RC-1 invariant guard: an indirect receiver with NO type evidence
    /// (`versions_->Recover()` inside `leveldb::DBImpl::Recover`) must NEVER bind to the
    /// caller's own class (a self-loop) — it stays unresolved. This is the exact shape the
    /// regression fabricated 155 times on leveldb.
    #[test]
    fn receiver_binding_indirect_receiver_never_binds_to_caller_itself() {
        let dbimpl = make_qn_node(
            "r_dbimpl",
            "Recover",
            "leveldb::DBImpl::Recover",
            Some("METHOD"),
            false,
        );
        let versionset = make_qn_node(
            "r_vs",
            "Recover",
            "leveldb::VersionSet::Recover",
            Some("METHOD"),
            false,
        );
        // The caller IS `DBImpl::Recover` itself — the self-loop the regression produced.
        let caller = make_qn_node(
            "r_dbimpl",
            "Recover",
            "leveldb::DBImpl::Recover",
            Some("METHOD"),
            false,
        );

        let mut index = empty_index();
        index
            .nodes_by_name
            .insert("Recover".into(), vec![dbimpl, versionset]);
        index.nodes_by_uid.insert("r_dbimpl".into(), caller);

        let mut edge = make_edge("e1", "Recover", EdgeType::Calls);
        edge.extractor = "cpp-core:0.1.0".into();
        edge.source_node_uid = "r_dbimpl".into();
        // Indirect receiver, no `receiverType` (cross-file member — type unknown in this TU).
        edge.metadata_json = Some(r#"{"calleeName":"Recover","receiver":"versions_"}"#.into());
        let result = resolve_edges(&[edge], &index, None);
        assert_eq!(
            result.resolved.len(),
            0,
            "no self-bind without receiver evidence"
        );
        assert_eq!(result.still_unresolved.len(), 1, "the call is counted");
    }

    /// F-CBR-004 regression: a C++ CALLS edge whose `metadata_json` is PRESENT but MALFORMED (does
    /// not parse) must be treated as "receiver disposition UNKNOWN" — NEVER eligible for the
    /// enclosing-class preference. An ambiguous call from `B::run` carrying a corrupt carrier, with
    /// candidates {A::run, B::run}, must NOT self-bind to `B::run`; it stays unresolved and counted.
    /// Before the fix the malformed carrier collapsed to `CallReceiver::default()` (receiver=None),
    /// read as receiverless, and the C-family enclosing-class guess recreated the RC-1 self-binding.
    #[test]
    fn receiver_binding_malformed_metadata_stays_unresolved() {
        let a = make_qn_node("a", "run", "A::run", Some("METHOD"), false);
        let b = make_qn_node("b", "run", "B::run", Some("METHOD"), false);
        // The caller IS `B::run` — the enclosing-class guess, if wrongly reached, would pick it.
        let caller = make_qn_node("br", "run", "B::run", Some("METHOD"), false);

        let mut index = empty_index();
        index.nodes_by_name.insert("run".into(), vec![a, b]);
        index.nodes_by_uid.insert("br".into(), caller);

        let mut edge = make_edge("e1", "run", EdgeType::Calls);
        edge.extractor = "cpp-core:0.1.0".into();
        edge.source_node_uid = "br".into();
        // Malformed carrier: present but not valid JSON (truncated object).
        edge.metadata_json = Some(r#"{"calleeName":"run","receiver":"#.into());
        let result = resolve_edges(&[edge], &index, None);
        assert_eq!(
            result.resolved.len(),
            0,
            "a malformed carrier is not eligible for the enclosing-class preference"
        );
        assert_eq!(result.still_unresolved.len(), 1, "the call is counted");
    }

    /// F-CBR-015 regression (RG-REQ-005-L01/-L02, RG-REQ-001-L03): a C++ CALLS edge whose
    /// `metadata_json` is VALID JSON but a NON-OBJECT root (`null`) must be UNREADABLE, never a
    /// KNOWN receiverless call. `serde_json::Value::get` answers `None` on a non-object, so before
    /// the object-root guard the carrier collapsed to `Receiverless` and the enclosing-class guess
    /// recreated the RC-1 self-binding. With candidates {A::run, B::run} and caller `B::run`, the
    /// call must NOT self-bind; it stays unresolved and counted.
    #[test]
    fn receiver_binding_non_object_null_carrier_stays_unresolved() {
        let a = make_qn_node("a", "run", "A::run", Some("METHOD"), false);
        let b = make_qn_node("b", "run", "B::run", Some("METHOD"), false);
        let caller = make_qn_node("br", "run", "B::run", Some("METHOD"), false);

        let mut index = empty_index();
        index.nodes_by_name.insert("run".into(), vec![a, b]);
        index.nodes_by_uid.insert("br".into(), caller);

        let mut edge = make_edge("e1", "run", EdgeType::Calls);
        edge.extractor = "cpp-core:0.1.0".into();
        edge.source_node_uid = "br".into();
        // Valid JSON, but a non-object root: `null`.
        edge.metadata_json = Some("null".into());
        let result = resolve_edges(&[edge], &index, None);
        assert_eq!(
            result.resolved.len(),
            0,
            "a valid non-object (null) carrier is unreadable, not eligible for the enclosing-class preference"
        );
        assert_eq!(result.still_unresolved.len(), 1, "the call is counted");
    }

    /// F-CBR-015 regression (RG-REQ-005-L01/-L02, RG-REQ-001-L03): the array sibling of the case
    /// above — a VALID JSON array root (`[]`) is likewise UNREADABLE, never receiverless, so an
    /// ambiguous `B::run` call carrying it does NOT self-bind and stays unresolved and counted.
    #[test]
    fn receiver_binding_non_object_array_carrier_stays_unresolved() {
        let a = make_qn_node("a", "run", "A::run", Some("METHOD"), false);
        let b = make_qn_node("b", "run", "B::run", Some("METHOD"), false);
        let caller = make_qn_node("br", "run", "B::run", Some("METHOD"), false);

        let mut index = empty_index();
        index.nodes_by_name.insert("run".into(), vec![a, b]);
        index.nodes_by_uid.insert("br".into(), caller);

        let mut edge = make_edge("e1", "run", EdgeType::Calls);
        edge.extractor = "cpp-core:0.1.0".into();
        edge.source_node_uid = "br".into();
        // Valid JSON, but a non-object root: `[]`.
        edge.metadata_json = Some("[]".into());
        let result = resolve_edges(&[edge], &index, None);
        assert_eq!(
            result.resolved.len(),
            0,
            "a valid non-object (array) carrier is unreadable, not eligible for the enclosing-class preference"
        );
        assert_eq!(result.still_unresolved.len(), 1, "the call is counted");
    }

    /// F-CBR-005 regression (RG-REQ-005-L02): a QUALIFIED receiver type never matches a
    /// shorter, unrelated container by suffix. Receiver evidence `receiverType: "a::Foo"` must
    /// NOT bind a candidate `Foo::run` — nothing shows `a::Foo` and a global `Foo` are the same
    /// type. With two `run` candidates ({`Foo::run`, `Bar::run`}) the bare-name singleton cannot
    /// fire, so the ONLY path to a binding would be the (now removed) reverse
    /// `rt.ends_with("::" + container)` match; the call must stay unresolved and counted.
    #[test]
    fn receiver_binding_qualified_receiver_type_never_matches_a_shorter_container() {
        let foo = make_qn_node("foo_run", "run", "Foo::run", Some("METHOD"), false);
        let bar = make_qn_node("bar_run", "run", "Bar::run", Some("METHOD"), false);
        let caller = make_qn_node(
            "caller",
            "method",
            "some::Caller::method",
            Some("METHOD"),
            false,
        );

        let mut index = empty_index();
        index.nodes_by_name.insert("run".into(), vec![foo, bar]);
        index.nodes_by_uid.insert("caller".into(), caller);

        let mut edge = make_edge("e1", "run", EdgeType::Calls);
        edge.extractor = "cpp-core:0.1.0".into();
        edge.source_node_uid = "caller".into();
        // Qualified receiver type `a::Foo` — must NOT suffix-match the shorter container `Foo`.
        edge.metadata_json =
            Some(r#"{"calleeName":"run","receiver":"p","receiverType":"a::Foo"}"#.into());
        let result = resolve_edges(&[edge], &index, None);
        assert_eq!(
            result.resolved.len(),
            0,
            "a qualified receiver type does not bind a shorter unrelated container"
        );
        assert_eq!(result.still_unresolved.len(), 1, "the call is counted");
    }

    /// F-CBR-006 regression (RG-REQ-005-L01/-L02, RG-REQ-001-L03): a `receiverType` is trustworthy
    /// ONLY on an INDIRECT (bare-name) receiver. A carrier that pairs a `receiverType` with an
    /// ABSENT receiver (orphan) or a NON-STRING receiver (corrupt) is internally inconsistent — the
    /// real extractor never emits it — and must NOT drive a binding through the receiver-type stage,
    /// NOR self-bind through the enclosing-class guess. With candidates {A::run, B::run} and the
    /// caller being `B::run`, a wrongly-trusted `receiverType:"B"` would fabricate the `B::run`
    /// self-loop (the RC-1 shape) via corrupt persisted metadata. Both shapes must stay
    /// unresolved-and-counted.
    #[test]
    fn receiver_binding_receiver_type_without_indirect_receiver_is_untrusted() {
        // Ambiguous pool {A::run, B::run}. `b`'s container IS the receiver type `B`, and the caller
        // IS `B::run`, so a wrongly-trusted `receiverType:"B"` (typed stage) OR the enclosing-class
        // guess would both land on the `b` self-loop — the RC-1 shape. Neither may fire.
        let a = make_qn_node("a", "run", "A::run", Some("METHOD"), false);
        let b = make_qn_node("b", "run", "B::run", Some("METHOD"), false);
        let caller = make_qn_node("br", "run", "B::run", Some("METHOD"), false);

        let mut index = empty_index();
        index.nodes_by_name.insert("run".into(), vec![a, b]);
        index.nodes_by_uid.insert("br".into(), caller);

        // Edge 1 — ORPHAN: `receiverType` present, NO `receiver` key.
        let mut orphan = make_edge("e_orphan", "run", EdgeType::Calls);
        orphan.extractor = "cpp-core:0.1.0".into();
        orphan.source_node_uid = "br".into();
        orphan.metadata_json = Some(r#"{"calleeName":"run","receiverType":"B"}"#.into());

        // Edge 2 — CORRUPT: `receiverType` present beside a NON-STRING `receiver`.
        let mut corrupt = make_edge("e_corrupt", "run", EdgeType::Calls);
        corrupt.extractor = "cpp-core:0.1.0".into();
        corrupt.source_node_uid = "br".into();
        corrupt.metadata_json =
            Some(r#"{"calleeName":"run","receiver":false,"receiverType":"B"}"#.into());

        let result = resolve_edges(&[orphan, corrupt], &index, None);
        assert_eq!(
            result.resolved.len(),
            0,
            "an inconsistent receiver/receiverType carrier never binds (no self-loop)"
        );
        assert_eq!(
            result.still_unresolved.len(),
            2,
            "both inconsistent carriers stay unresolved and counted"
        );
    }

    /// F-CBR-007 regression (RG-REQ-005-L01, RG-REQ-001-L03): a `receiverType` that is PRESENT but
    /// MALFORMED (a non-string value, or an empty string) beside a VALID indirect receiver is read
    /// as `Unreadable`, DISTINCT from an absent type — it carries no usable type, so it drives no
    /// receiver-type binding and (being indirect) is never eligible for the enclosing-class guess.
    /// With candidates {A::run, B::run} and the caller being `B::run`, a `receiverType` wrongly
    /// trusted as "B" would fabricate the `B::run` self-loop (the RC-1 shape) from corrupt persisted
    /// metadata. Both the non-string and the empty-string shapes must stay unresolved-and-counted.
    #[test]
    fn receiver_binding_malformed_receiver_type_on_indirect_receiver_is_unreadable() {
        let a = make_qn_node("a", "run", "A::run", Some("METHOD"), false);
        let b = make_qn_node("b", "run", "B::run", Some("METHOD"), false);
        let caller = make_qn_node("br", "run", "B::run", Some("METHOD"), false);

        let mut index = empty_index();
        index.nodes_by_name.insert("run".into(), vec![a, b]);
        index.nodes_by_uid.insert("br".into(), caller);

        // Edge 1 — NON-STRING receiverType beside a valid indirect receiver `p`.
        let mut non_string = make_edge("e_nonstring", "run", EdgeType::Calls);
        non_string.extractor = "cpp-core:0.1.0".into();
        non_string.source_node_uid = "br".into();
        non_string.metadata_json =
            Some(r#"{"calleeName":"run","receiver":"p","receiverType":false}"#.into());

        // Edge 2 — EMPTY-STRING receiverType beside a valid indirect receiver `p`.
        let mut empty = make_edge("e_empty", "run", EdgeType::Calls);
        empty.extractor = "cpp-core:0.1.0".into();
        empty.source_node_uid = "br".into();
        empty.metadata_json =
            Some(r#"{"calleeName":"run","receiver":"p","receiverType":""}"#.into());

        let result = resolve_edges(&[non_string, empty], &index, None);
        assert_eq!(
            result.resolved.len(),
            0,
            "a malformed receiverType on an indirect receiver never binds (no self-loop)"
        );
        assert_eq!(
            result.still_unresolved.len(),
            2,
            "both malformed-type carriers stay unresolved and counted"
        );
    }

    /// F-CBR-007: the three-state `ReceiverTypeRead` keeps a malformed type DISTINCT from an absent
    /// one at the parse boundary — a present non-string / empty `receiverType` beside an indirect
    /// receiver reads as `Indirect { Unreadable }`, never `Indirect { Absent }`, so a future edit
    /// can tell corrupt evidence from a legitimately-unavailable cross-file type.
    #[test]
    fn call_receiver_info_distinguishes_malformed_from_absent_receiver_type() {
        assert_eq!(
            call_receiver_info(Some(r#"{"receiver":"p"}"#)),
            ReceiverDisposition::Indirect {
                receiver_type: ReceiverTypeRead::Absent
            },
        );
        assert_eq!(
            call_receiver_info(Some(r#"{"receiver":"p","receiverType":"X"}"#)),
            ReceiverDisposition::Indirect {
                receiver_type: ReceiverTypeRead::Present("X".into())
            },
        );
        assert_eq!(
            call_receiver_info(Some(r#"{"receiver":"p","receiverType":false}"#)),
            ReceiverDisposition::Indirect {
                receiver_type: ReceiverTypeRead::Unreadable
            },
        );
        assert_eq!(
            call_receiver_info(Some(r#"{"receiver":"p","receiverType":""}"#)),
            ReceiverDisposition::Indirect {
                receiver_type: ReceiverTypeRead::Unreadable
            },
        );
        // A `receiverType` beside a non-indirect receiver stays an inconsistent (Unreadable) carrier.
        assert_eq!(
            call_receiver_info(Some(r#"{"receiver":"this","receiverType":"X"}"#)),
            ReceiverDisposition::Unreadable,
        );
    }

    #[test]
    fn enclosing_class_preference_gated_off_for_non_cpp() {
        // The SAME ambiguous shape from a non-C/C++ edge must NOT resolve via the
        // enclosing-class preference — non-C/C++ resolution stays byte-identical.
        let a = make_qn_node("a", "Recover", "pkg::A::Recover", Some("METHOD"), false);
        let b = make_qn_node("b", "Recover", "pkg::B::Recover", Some("METHOD"), false);
        let caller = make_qn_node("open", "Open", "pkg::A::Open", Some("METHOD"), false);

        let mut index = empty_index();
        index.nodes_by_name.insert("Recover".into(), vec![a, b]);
        index.nodes_by_uid.insert("open".into(), caller);

        let mut edge = make_edge("e1", "Recover", EdgeType::Calls);
        edge.extractor = "rust-core:0.1.0".into();
        edge.source_node_uid = "open".into();
        let result = resolve_edges(&[edge], &index, None);
        assert_eq!(result.resolved.len(), 0, "non-C/C++ stays ambiguous");
    }

    /// An empty resolver index for the CPP-DECLARATORS-1 resolution tests.
    fn empty_index() -> ResolverIndex {
        ResolverIndex {
            nodes_by_stable_key: HashMap::new(),
            nodes_by_name: HashMap::new(),
            nodes_by_qualified_name: HashMap::new(),
            nodes_by_uid: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
            java_suffix_index: HashMap::new(),
            npm_workspace_packages: HashMap::new(),
            tsconfig_paths: Default::default(),
        }
    }

    // ── Python self/cls call hierarchy binding (PYTHON-SELF-BINDING-1) ──

    fn py_class(uid: &str, name: &str, file: &str, superclasses: &[&str]) -> ResolverNode {
        ResolverNode {
            node_uid: uid.into(),
            stable_key: format!("r:{file}#{name}:SYMBOL:CLASS"),
            name: name.into(),
            qualified_name: Some(name.into()),
            kind: "SYMBOL".into(),
            subtype: Some("CLASS".into()),
            file_uid: Some(file.into()),
            forward_decl: false,
            superclasses: superclasses.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn py_symbol(
        uid: &str,
        name: &str,
        qualified: &str,
        subtype: &str,
        file: &str,
    ) -> ResolverNode {
        ResolverNode {
            node_uid: uid.into(),
            stable_key: format!("r:{file}#{qualified}:SYMBOL:{subtype}"),
            name: name.into(),
            qualified_name: Some(qualified.into()),
            kind: "SYMBOL".into(),
            subtype: Some(subtype.into()),
            file_uid: Some(file.into()),
            forward_decl: false,
            superclasses: Vec::new(),
        }
    }

    fn py_method(uid: &str, class: &str, method: &str, file: &str) -> ResolverNode {
        py_symbol(uid, method, &format!("{class}.{method}"), "METHOD", file)
    }

    fn py_index(nodes: &[ResolverNode]) -> ResolverIndex {
        let mut index = empty_index();
        for n in nodes {
            index
                .nodes_by_name
                .entry(n.name.clone())
                .or_default()
                .push(n.clone());
            if let Some(ref qn) = n.qualified_name {
                index
                    .nodes_by_qualified_name
                    .entry(qn.clone())
                    .or_default()
                    .push(n.clone());
            }
            index.nodes_by_uid.insert(n.node_uid.clone(), n.clone());
            if let Some(ref f) = n.file_uid {
                index
                    .node_uid_to_file_uid
                    .insert(n.node_uid.clone(), f.clone());
            }
        }
        index
    }

    fn self_call_edge(
        uid: &str,
        caller_uid: &str,
        target_key: &str,
        enclosing_class: &str,
    ) -> ExtractedEdge {
        let mut e = make_edge(uid, target_key, EdgeType::Calls);
        e.extractor = "python-core:0.1.0".into();
        e.source_node_uid = caller_uid.into();
        e.metadata_json = Some(
            serde_json::json!({ "selfCall": true, "enclosingClass": enclosing_class }).to_string(),
        );
        e
    }

    #[test]
    fn self_call_binds_to_the_own_class_method_in_the_same_file() {
        let index = py_index(&[
            py_class("c", "C", "r:f.py", &[]),
            py_method("c_f", "C", "f", "r:f.py"),
            py_method("c_g", "C", "g", "r:f.py"),
        ]);
        let edge = self_call_edge("e1", "c_f", "self.g", "C");
        let result = resolve_edges(&[edge], &index, None);
        assert_eq!(result.resolved.len(), 1, "own-class method binds");
        assert_eq!(result.resolved[0].target_node_uid, "c_g");
        assert!(result.still_unresolved.is_empty());
    }

    #[test]
    fn self_call_binds_to_a_unique_ancestor_method() {
        // `run` is ambiguous globally (Base.run + Other.run), so ONLY the hierarchy walk can bind.
        let index = py_index(&[
            py_class("base", "Base", "r:base.py", &[]),
            py_method("base_run", "Base", "run", "r:base.py"),
            py_class("sub", "Sub", "r:sub.py", &["Base"]),
            py_method("sub_caller", "Sub", "handle", "r:sub.py"),
            py_method("other_run", "Other", "run", "r:other.py"),
        ]);
        let edge = self_call_edge("e1", "sub_caller", "self.run", "Sub");
        let result = resolve_edges(&[edge], &index, None);
        assert_eq!(result.resolved.len(), 1, "ancestor method binds");
        assert_eq!(result.resolved[0].target_node_uid, "base_run");
    }

    #[test]
    fn self_call_same_depth_ancestor_collision_stays_unresolved_with_mro_candidates() {
        let index = py_index(&[
            py_class("a", "A", "r:a.py", &[]),
            py_method("a_run", "A", "run", "r:a.py"),
            py_class("b", "B", "r:b.py", &[]),
            py_method("b_run", "B", "run", "r:b.py"),
            py_class("sub", "Sub", "r:sub.py", &["A", "B"]),
            py_method("caller", "Sub", "go", "r:sub.py"),
        ]);
        let edge = self_call_edge("e1", "caller", "self.run", "Sub");
        let result = resolve_edges(&[edge], &index, None);
        assert!(
            result.resolved.is_empty(),
            "collision is not silently picked"
        );
        assert_eq!(result.still_unresolved.len(), 1);
        let ue = &result.still_unresolved[0];
        assert_eq!(
            ue.category,
            UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
            "category unchanged (D-PSB-001: the collision is a BASIS, not a category)"
        );
        let meta: serde_json::Value =
            serde_json::from_str(ue.edge.metadata_json.as_ref().unwrap()).unwrap();
        let cands = meta["mroCandidates"].as_array().unwrap();
        assert_eq!(cands.len(), 2);
        assert_eq!(cands[0], "A.run", "candidates are sorted");
        assert_eq!(cands[1], "B.run");
        // The extractor's carrier is preserved beside the resolver-written mroCandidates.
        assert_eq!(meta["selfCall"], serde_json::json!(true));
        assert_eq!(meta["enclosingClass"], "Sub");
    }

    #[test]
    fn self_call_with_no_defining_ancestor_stays_unresolved_with_its_name_only_pool() {
        // `helper` is a module-level function, globally unique, NOT in C's hierarchy. The walk
        // finds no hit; PYTHON-RECEIVER-BINDING-1 (RG-REQ-005-L03, D-PRB-SCOPE-1 amendment 2):
        // the miss is TERMINAL — the call stays unresolved with the bare-name pool recorded as
        // evidence, never bound by that name. (The helper stamps `python-core:0.1.0` without a
        // `receiverBinding` key: the legacy path, whose miss is the same terminal row.)
        let index = py_index(&[
            py_class("c", "C", "r:f.py", &[]),
            py_method("caller", "C", "f", "r:f.py"),
            py_symbol("helper", "helper", "helper", "FUNCTION", "r:other.py"),
        ]);
        let edge = self_call_edge("e1", "caller", "self.helper", "C");
        let result = resolve_edges(&[edge], &index, None);
        assert!(
            result.resolved.is_empty(),
            "a hierarchy miss never binds by bare name"
        );
        assert_eq!(result.still_unresolved.len(), 1);
        let meta = unresolved_meta(&result, 0);
        assert_eq!(meta["nameOnlyCandidates"], serde_json::json!(["helper"]));
        assert_eq!(meta["nameOnlyReason"], "self_call_hierarchy_miss");
        assert_eq!(
            meta["selfCall"],
            serde_json::json!(true),
            "carrier preserved"
        );
        assert_eq!(
            result.still_unresolved[0].category,
            UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo
        );
    }

    #[test]
    fn self_call_same_name_ancestor_classes_stop_the_walk() {
        // The base name `Base` matches TWO CLASS nodes — the walk cannot descend that branch
        // (no guess), and the ambiguous `run` name leaves the call unresolved with NO collision.
        let index = py_index(&[
            py_class("sub", "Sub", "r:sub.py", &["Base"]),
            py_method("caller", "Sub", "go", "r:sub.py"),
            py_class("base1", "Base", "r:base1.py", &[]),
            py_method("base1_run", "Base", "run", "r:base1.py"),
            py_class("base2", "Base", "r:base2.py", &[]),
            py_method("base2_run", "Base", "run", "r:base2.py"),
        ]);
        let edge = self_call_edge("e1", "caller", "self.run", "Sub");
        let result = resolve_edges(&[edge], &index, None);
        assert!(result.resolved.is_empty());
        assert_eq!(result.still_unresolved.len(), 1);
        let meta: serde_json::Value = serde_json::from_str(
            result.still_unresolved[0]
                .edge
                .metadata_json
                .as_ref()
                .unwrap(),
        )
        .unwrap();
        assert!(
            meta.get("mroCandidates").is_none(),
            "the walk never reached the ambiguous bases — not a same-depth collision"
        );
    }

    #[test]
    fn self_call_malformed_carrier_never_binds() {
        // `g` is ambiguous globally (C.g + D.g). A VALID carrier binds to the own class; a
        // MALFORMED carrier (selfCall not a boolean) skips the stage → the ambiguous fallback
        // leaves it unresolved. The malformed carrier never binds on unreadable evidence.
        let index = py_index(&[
            py_class("c", "C", "r:f.py", &[]),
            py_method("caller", "C", "f", "r:f.py"),
            py_method("c_g", "C", "g", "r:f.py"),
            py_method("d_g", "D", "g", "r:d.py"),
        ]);
        let ok = self_call_edge("eok", "caller", "self.g", "C");
        let r_ok = resolve_edges(&[ok], &index, None);
        assert_eq!(r_ok.resolved.len(), 1, "valid carrier binds to own class");
        assert_eq!(r_ok.resolved[0].target_node_uid, "c_g");

        let mut bad = make_edge("ebad", "self.g", EdgeType::Calls);
        bad.extractor = "python-core:0.1.0".into();
        bad.source_node_uid = "caller".into();
        bad.metadata_json = Some(r#"{"selfCall":"yes","enclosingClass":"C"}"#.into());
        let r_bad = resolve_edges(&[bad], &index, None);
        assert!(r_bad.resolved.is_empty(), "malformed carrier never binds");
        assert_eq!(r_bad.still_unresolved.len(), 1);
    }

    #[test]
    fn cls_call_binds_like_self() {
        // `build` is ambiguous globally, so only the hierarchy walk (via the `cls.` carrier) binds.
        let index = py_index(&[
            py_class("c", "C", "r:f.py", &[]),
            py_method("caller", "C", "make", "r:f.py"),
            py_method("c_build", "C", "build", "r:f.py"),
            py_method("d_build", "D", "build", "r:d.py"),
        ]);
        let edge = self_call_edge("e1", "caller", "cls.build", "C");
        let result = resolve_edges(&[edge], &index, None);
        assert_eq!(result.resolved.len(), 1, "cls-call binds like self");
        assert_eq!(result.resolved[0].target_node_uid, "c_build");
    }

    // ── Python receiver certainty (PYTHON-RECEIVER-BINDING-1) ──

    /// The parsed `metadata_json` of the `i`-th unresolved row.
    fn unresolved_meta(result: &ResolutionResult, i: usize) -> serde_json::Value {
        serde_json::from_str(
            result.still_unresolved[i]
                .edge
                .metadata_json
                .as_deref()
                .expect("unresolved row carries metadata"),
        )
        .unwrap()
    }

    /// A Python CALLS edge without any carrier.
    fn py_call_edge(uid: &str, caller_uid: &str, target_key: &str) -> ExtractedEdge {
        let mut e = make_edge(uid, target_key, EdgeType::Calls);
        e.extractor = "python-core:0.2.0".into();
        e.source_node_uid = caller_uid.into();
        e
    }

    /// A self/cls carrier edge written by `writer`, with `receiverBinding` set to `binding`
    /// (omitted when `None`).
    fn carrier_edge(
        uid: &str,
        caller_uid: &str,
        target_key: &str,
        enclosing_class: &str,
        writer: &str,
        binding: Option<serde_json::Value>,
    ) -> ExtractedEdge {
        let mut e = self_call_edge(uid, caller_uid, target_key, enclosing_class);
        e.extractor = writer.into();
        if let Some(b) = binding {
            let mut meta: serde_json::Value =
                serde_json::from_str(e.metadata_json.as_deref().unwrap()).unwrap();
            meta["receiverBinding"] = b;
            e.metadata_json = Some(meta.to_string());
        }
        e
    }

    /// The alias fixture: `a.py` holds `import helper as self` (a Namespace binding that resolves
    /// to `helper.py`, whose unique function is `run`) and class `C` with method `caller`; `C`
    /// declares `run` iff `c_declares_run`.
    fn alias_fixture(c_declares_run: bool) -> (ResolverIndex, HashMap<String, Vec<ImportBinding>>) {
        let mut nodes = vec![
            py_class("c", "C", "r:a.py", &[]),
            py_method("caller", "C", "caller", "r:a.py"),
            py_symbol("helper_run", "run", "run", "FUNCTION", "r:helper.py"),
        ];
        if c_declares_run {
            nodes.push(py_method("c_run", "C", "run", "r:a.py"));
        }
        let mut index = py_index(&nodes);
        index
            .file_resolution
            .insert("r:helper:FILE".into(), "r:helper.py:FILE".into());
        let bindings: HashMap<String, Vec<ImportBinding>> = [(
            "r:a.py".to_string(),
            vec![ImportBinding {
                identifier: "self".into(),
                specifier: "./helper".into(),
                is_relative: true,
                location: None,
                is_type_only: false,
                imported_name: None,
                kind: ImportKind::Namespace,
            }],
        )]
        .into_iter()
        .collect();
        (index, bindings)
    }

    #[test]
    fn python_untyped_receiver_unique_name_binds_inferred_with_its_pool() {
        // `dependencies.extend(...)` — a local list; `extend` names one indexed method.
        let index = py_index(&[
            py_symbol("f", "f", "f", "FUNCTION", "r:a.py"),
            py_method("ext", "ListMixin", "extend", "r:m.py"),
        ]);
        let result = resolve_edges(
            &[py_call_edge("e1", "f", "dependencies.extend")],
            &index,
            None,
        );
        assert_eq!(result.resolved.len(), 1);
        let r = &result.resolved[0];
        assert_eq!(r.target_node_uid, "ext");
        assert_eq!(r.resolution, Resolution::Inferred, "never static");
        let meta: serde_json::Value =
            serde_json::from_str(r.metadata_json.as_deref().unwrap()).unwrap();
        assert_eq!(meta["basis"], "receiver_untyped_name_only");
        assert_eq!(meta["receiver"], "dependencies");
        assert_eq!(meta["candidates"], serde_json::json!(["ListMixin.extend"]));
        assert!(result.still_unresolved.is_empty());
    }

    #[test]
    fn python_chained_receiver_unique_name_binds_inferred_with_the_whole_receiver() {
        let index = py_index(&[
            py_symbol("f", "f", "f", "FUNCTION", "r:a.py"),
            py_method("proc", "UserService", "process", "r:s.py"),
        ]);
        let result = resolve_edges(
            &[py_call_edge("e1", "f", "self._service.process")],
            &index,
            None,
        );
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].resolution, Resolution::Inferred);
        let meta: serde_json::Value =
            serde_json::from_str(result.resolved[0].metadata_json.as_deref().unwrap()).unwrap();
        assert_eq!(
            meta["receiver"], "self._service",
            "the whole receiver, minus the method"
        );
        assert_eq!(
            meta["candidates"],
            serde_json::json!(["UserService.process"])
        );
    }

    #[test]
    fn python_untyped_receiver_ambiguous_name_stays_unresolved_with_every_candidate_and_its_reason()
    {
        let index = py_index(&[
            py_symbol("f", "f", "f", "FUNCTION", "r:a.py"),
            py_method("qs", "QuerySet", "values", "r:q.py"),
            py_method("mv", "MultiValueDict", "values", "r:d.py"),
        ]);
        let result = resolve_edges(&[py_call_edge("e1", "f", "fields.values")], &index, None);
        assert!(result.resolved.is_empty(), "no candidate is bound");
        assert_eq!(result.still_unresolved.len(), 1);
        let meta = unresolved_meta(&result, 0);
        assert_eq!(
            meta["nameOnlyCandidates"],
            serde_json::json!(["MultiValueDict.values", "QuerySet.values"]),
            "the full pool, sorted"
        );
        assert_eq!(meta["nameOnlyReason"], "ambiguous_name");
        assert_eq!(
            result.still_unresolved[0].category,
            UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
            "category unchanged"
        );
    }

    #[test]
    fn python_untyped_receiver_with_no_candidate_is_unchanged() {
        let index = py_index(&[py_symbol("f", "f", "f", "FUNCTION", "r:a.py")]);
        let edge = py_call_edge("e1", "f", "os.getcwd");
        let result = resolve_edges(std::slice::from_ref(&edge), &index, None);
        assert!(result.resolved.is_empty());
        assert_eq!(result.still_unresolved.len(), 1);
        assert_eq!(
            result.still_unresolved[0].edge.metadata_json, edge.metadata_json,
            "no pool and no reason are recorded (nothing to lose)"
        );
        assert_eq!(
            result.still_unresolved[0].category,
            UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo
        );
    }

    #[test]
    fn python_class_named_receiver_binds_inferred_never_static() {
        // `GEOSGeometry._from_wkt(wkt)`, the class declared in the caller's own file: the
        // receiver's spelling is not proof it IS the class (amendment 1 — no class-name rule).
        let index = py_index(&[
            py_class(
                "geo",
                "GEOSGeometry",
                "r:geometry.py",
                &["GEOSGeometryBase"],
            ),
            py_method("caller", "GEOSGeometryBase", "from_ewkt", "r:geometry.py"),
            py_method("fw", "GEOSGeometryBase", "_from_wkt", "r:geometry.py"),
        ]);
        let result = resolve_edges(
            &[py_call_edge("e1", "caller", "GEOSGeometry._from_wkt")],
            &index,
            None,
        );
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].target_node_uid, "fw");
        assert_eq!(result.resolved[0].resolution, Resolution::Inferred);
        let meta: serde_json::Value =
            serde_json::from_str(result.resolved[0].metadata_json.as_deref().unwrap()).unwrap();
        assert_eq!(meta["receiver"], "GEOSGeometry");
    }

    #[test]
    fn python_parameter_shadowing_a_class_name_binds_inferred() {
        // `class X: def run(self)` and `def caller(X): X.run()` — the parameter shadows the class.
        let index = py_index(&[
            py_class("x", "X", "r:a.py", &[]),
            py_method("x_run", "X", "run", "r:a.py"),
            py_symbol("caller", "caller", "caller", "FUNCTION", "r:a.py"),
        ]);
        let result = resolve_edges(&[py_call_edge("e1", "caller", "X.run")], &index, None);
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].resolution, Resolution::Inferred);
    }

    #[test]
    fn python_self_receiver_without_a_carrier_stays_unresolved_with_its_name_only_pool() {
        // A module-level `def natural_key_serializer_test(self, format)` calling
        // `self.assertEqual(...)`: no carrier, no alias named `self`; the unique name is not
        // evidence for a receiver no class types.
        let index = py_index(&[
            py_symbol("t", "t", "t", "FUNCTION", "r:test_natural.py"),
            py_symbol(
                "ae",
                "assertEqual",
                "WidgetTest.check_html.assertEqual",
                "VARIABLE",
                "r:base.py",
            ),
        ]);
        let result = resolve_edges(&[py_call_edge("e1", "t", "self.assertEqual")], &index, None);
        assert!(result.resolved.is_empty(), "never bound by name");
        assert_eq!(result.still_unresolved.len(), 1);
        let meta = unresolved_meta(&result, 0);
        assert_eq!(
            meta["nameOnlyCandidates"],
            serde_json::json!(["WidgetTest.check_html.assertEqual"])
        );
        assert_eq!(meta["nameOnlyReason"], "self_call_without_class_context");
    }

    #[test]
    fn python_cls_receiver_with_a_malformed_carrier_stays_unresolved_with_its_name_only_pool() {
        let index = py_index(&[
            py_class("c", "C", "r:f.py", &[]),
            py_method("caller", "C", "make", "r:f.py"),
            py_method("c_build", "C", "build", "r:f.py"),
        ]);
        let mut e = py_call_edge("e1", "caller", "cls.build");
        e.metadata_json = Some(r#"{"selfCall":"yes","enclosingClass":"C"}"#.into());
        let result = resolve_edges(&[e], &index, None);
        assert!(
            result.resolved.is_empty(),
            "a malformed carrier never binds"
        );
        let meta = unresolved_meta(&result, 0);
        assert_eq!(meta["nameOnlyCandidates"], serde_json::json!(["C.build"]));
        assert_eq!(meta["nameOnlyReason"], "self_call_without_class_context");
        assert_eq!(
            meta["selfCall"], "yes",
            "the extractor's keys are preserved"
        );
    }

    #[test]
    fn python_this_prefixed_dotted_key_binds_inferred_not_static() {
        // The fallback's `this.`-prefixed branch shares the Python rule.
        let index = py_index(&[
            py_symbol("f", "f", "f", "FUNCTION", "r:a.py"),
            py_method("fb", "Repo", "find_by_id", "r:repo.py"),
        ]);
        let result = resolve_edges(
            &[py_call_edge("e1", "f", "this.repo.find_by_id")],
            &index,
            None,
        );
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].resolution, Resolution::Inferred);
        let meta: serde_json::Value =
            serde_json::from_str(result.resolved[0].metadata_json.as_deref().unwrap()).unwrap();
        assert_eq!(meta["receiver"], "this.repo");
    }

    #[test]
    fn python_unresolvable_namespace_alias_member_binds_inferred() {
        // `import datetime` (a non-relative specifier the namespace stage cannot resolve) and
        // `datetime.timedelta(...)`: the indexed `timedelta` elsewhere is a name-only candidate.
        let index = py_index(&[
            py_symbol("f", "f", "f", "FUNCTION", "r:dates.py"),
            py_symbol(
                "td",
                "timedelta",
                "test_x.timedelta",
                "FUNCTION",
                "r:tests/t.py",
            ),
        ]);
        let bindings: HashMap<String, Vec<ImportBinding>> = [(
            "r:dates.py".to_string(),
            vec![ImportBinding {
                identifier: "datetime".into(),
                specifier: "datetime".into(),
                is_relative: false,
                location: None,
                is_type_only: false,
                imported_name: None,
                kind: ImportKind::Namespace,
            }],
        )]
        .into_iter()
        .collect();
        let result = resolve_edges(
            &[py_call_edge("e1", "f", "datetime.timedelta")],
            &index,
            Some(&bindings),
        );
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].resolution, Resolution::Inferred);
        let meta: serde_json::Value =
            serde_json::from_str(result.resolved[0].metadata_json.as_deref().unwrap()).unwrap();
        assert_eq!(meta["receiver"], "datetime");
    }

    #[test]
    fn typescript_and_c_family_calls_are_untouched_by_the_python_receiver_rule() {
        let index = py_index(&[
            py_symbol("f", "f", "f", "FUNCTION", "r:a.ts"),
            py_method("save", "Repo", "save", "r:repo.ts"),
        ]);
        for writer in ["ts-core:0.2.0", "c-core:0.1.0", "cpp-core:0.2.0", "test:1"] {
            for key in ["obj.save", "this.repo.save", "self.save"] {
                let mut e = make_edge("e1", key, EdgeType::Calls);
                e.extractor = writer.into();
                e.source_node_uid = "f".into();
                let result = resolve_edges(&[e], &index, None);
                assert_eq!(result.resolved.len(), 1, "{writer} {key}: binds as before");
                assert_eq!(
                    result.resolved[0].resolution,
                    Resolution::Static,
                    "{writer} {key}"
                );
                assert_eq!(result.resolved[0].metadata_json, None, "{writer} {key}");
            }
        }
    }

    #[test]
    fn self_call_hierarchy_miss_with_an_ambiguous_name_records_every_candidate() {
        let index = py_index(&[
            py_class("c", "C", "r:f.py", &[]),
            py_method("caller", "C", "f", "r:f.py"),
            py_method("d_g", "D", "g", "r:d.py"),
            py_method("e_g", "E", "g", "r:e.py"),
        ]);
        let edge = carrier_edge(
            "e1",
            "caller",
            "self.g",
            "C",
            "python-core:0.2.0",
            Some("parameter".into()),
        );
        let result = resolve_edges(&[edge], &index, None);
        assert!(result.resolved.is_empty());
        let meta = unresolved_meta(&result, 0);
        assert_eq!(
            meta["nameOnlyCandidates"],
            serde_json::json!(["D.g", "E.g"])
        );
        assert_eq!(meta["nameOnlyReason"], "self_call_hierarchy_miss");
    }

    #[test]
    fn self_call_hierarchy_miss_with_no_candidate_records_an_empty_pool() {
        let index = py_index(&[
            py_class("c", "C", "r:f.py", &["TestCase"]),
            py_method("caller", "C", "test_x", "r:f.py"),
        ]);
        let edge = carrier_edge(
            "e1",
            "caller",
            "self.assertTrue",
            "C",
            "python-core:0.2.0",
            Some("parameter".into()),
        );
        let result = resolve_edges(&[edge], &index, None);
        assert!(result.resolved.is_empty());
        let meta = unresolved_meta(&result, 0);
        assert_eq!(meta["nameOnlyCandidates"], serde_json::json!([]));
        assert_eq!(meta["nameOnlyReason"], "self_call_hierarchy_miss");
    }

    #[test]
    fn self_call_whose_own_class_is_not_unique_in_its_file_stays_unresolved() {
        // Two classes named `C` in the caller's file: the walk cannot start (NoHit). The unique
        // `g` elsewhere is NOT bound by name.
        let index = py_index(&[
            py_class("c1", "C", "r:f.py", &[]),
            py_class("c2", "C", "r:f.py", &[]),
            py_method("caller", "C", "f", "r:f.py"),
            py_method("d_g", "D", "g", "r:d.py"),
        ]);
        let edge = carrier_edge(
            "e1",
            "caller",
            "self.g",
            "C",
            "python-core:0.2.0",
            Some("parameter".into()),
        );
        let result = resolve_edges(&[edge], &index, None);
        assert!(result.resolved.is_empty());
        let meta = unresolved_meta(&result, 0);
        assert_eq!(meta["nameOnlyCandidates"], serde_json::json!(["D.g"]));
        assert_eq!(meta["nameOnlyReason"], "self_call_hierarchy_miss");
    }

    #[test]
    fn python_parameter_bound_self_call_hierarchy_hit_binds_static_over_a_file_level_alias_named_self(
    ) {
        let (index, bindings) = alias_fixture(true);
        let edge = carrier_edge(
            "e1",
            "caller",
            "self.run",
            "C",
            "python-core:0.2.0",
            Some("parameter".into()),
        );
        let result = resolve_edges(&[edge], &index, Some(&bindings));
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(
            result.resolved[0].target_node_uid, "c_run",
            "C.run, not helper's run"
        );
        assert_eq!(result.resolved[0].resolution, Resolution::Static);
    }

    #[test]
    fn python_parameter_bound_self_call_hierarchy_miss_stays_unresolved_and_never_binds_through_a_file_level_alias_named_self(
    ) {
        let (index, bindings) = alias_fixture(false);
        let edge = carrier_edge(
            "e1",
            "caller",
            "self.run",
            "C",
            "python-core:0.2.0",
            Some("parameter".into()),
        );
        let result = resolve_edges(&[edge], &index, Some(&bindings));
        assert!(
            result.resolved.is_empty(),
            "the alias is never consulted for a parameter"
        );
        let meta = unresolved_meta(&result, 0);
        assert_eq!(meta["nameOnlyReason"], "self_call_hierarchy_miss");
        assert_eq!(
            meta["nameOnlyCandidates"],
            serde_json::json!(["run"]),
            "helper's run is a candidate"
        );
    }

    #[test]
    fn python_unproven_self_call_with_a_colliding_alias_stays_unresolved_with_every_candidate() {
        // The review's counterexample in the fixture's names: a parameterless class-body
        // `caller()` calling `self.run()` beside `import helper as self`.
        let (index, bindings) = alias_fixture(true);
        let edge = carrier_edge(
            "e1",
            "caller",
            "self.run",
            "C",
            "python-core:0.2.0",
            Some("unproven".into()),
        );
        let result = resolve_edges(&[edge], &index, Some(&bindings));
        assert!(result.resolved.is_empty(), "bound through neither stage");
        let meta = unresolved_meta(&result, 0);
        assert_eq!(meta["nameOnlyReason"], "self_call_receiver_unproven");
        assert_eq!(
            meta["nameOnlyCandidates"],
            serde_json::json!(["C.run", "run"])
        );
    }

    #[test]
    fn python_carrier_without_a_receiver_binding_from_python_core_0_1_0_meets_the_namespace_stage_before_the_hierarchy_as_at_head(
    ) {
        let (index, bindings) = alias_fixture(true);
        let edge = carrier_edge("e1", "caller", "self.run", "C", "python-core:0.1.0", None);
        let result = resolve_edges(&[edge], &index, Some(&bindings));
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(
            result.resolved[0].target_node_uid, "helper_run",
            "HEAD's namespace-first result"
        );
        assert_eq!(result.resolved[0].resolution, Resolution::Static);
    }

    #[test]
    fn python_carrier_without_a_receiver_binding_from_python_core_0_1_0_hierarchy_hit_binds_static()
    {
        let (index, _bindings) = alias_fixture(true);
        let edge = carrier_edge("e1", "caller", "self.run", "C", "python-core:0.1.0", None);
        let result = resolve_edges(&[edge], &index, None);
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].target_node_uid, "c_run");
        assert_eq!(result.resolved[0].resolution, Resolution::Static);
    }

    #[test]
    fn python_carrier_without_a_receiver_binding_from_python_core_0_1_0_hierarchy_miss_is_terminal_never_the_bare_name(
    ) {
        // No alias; no `run` in C's hierarchy; a unique `run` elsewhere that HEAD bound by name.
        let (index, _bindings) = alias_fixture(false);
        let edge = carrier_edge("e1", "caller", "self.run", "C", "python-core:0.1.0", None);
        let result = resolve_edges(&[edge], &index, None);
        assert!(result.resolved.is_empty());
        let meta = unresolved_meta(&result, 0);
        assert_eq!(meta["nameOnlyReason"], "self_call_hierarchy_miss");
        assert_eq!(meta["nameOnlyCandidates"], serde_json::json!(["run"]));
    }

    #[test]
    fn python_carrier_without_a_receiver_binding_from_python_core_0_2_0_is_unproven() {
        let (index, bindings) = alias_fixture(true);
        let edge = carrier_edge("e1", "caller", "self.run", "C", "python-core:0.2.0", None);
        let result = resolve_edges(&[edge], &index, Some(&bindings));
        assert!(result.resolved.is_empty());
        let meta = unresolved_meta(&result, 0);
        assert_eq!(meta["nameOnlyReason"], "self_call_receiver_unproven");
        assert_eq!(
            meta["nameOnlyCandidates"],
            serde_json::json!(["C.run", "run"])
        );
    }

    #[test]
    fn python_carrier_without_a_receiver_binding_from_an_unrecognized_writer_is_unproven() {
        for writer in ["python-core:0.1.1", "python-core:9.0.0"] {
            let (index, bindings) = alias_fixture(true);
            let edge = carrier_edge("e1", "caller", "self.run", "C", writer, None);
            let result = resolve_edges(&[edge], &index, Some(&bindings));
            assert!(result.resolved.is_empty(), "{writer}: the match is exact");
            let meta = unresolved_meta(&result, 0);
            assert_eq!(
                meta["nameOnlyReason"], "self_call_receiver_unproven",
                "{writer}"
            );
            assert_eq!(
                meta["nameOnlyCandidates"],
                serde_json::json!(["C.run", "run"]),
                "{writer}"
            );
        }
    }

    #[test]
    fn python_self_call_carrier_with_an_unreadable_receiver_binding_stays_unresolved_with_its_pool()
    {
        for binding in [serde_json::json!(true), serde_json::json!("local")] {
            let (index, bindings) = alias_fixture(true);
            let edge = carrier_edge(
                "e1",
                "caller",
                "self.run",
                "C",
                "python-core:0.2.0",
                Some(binding.clone()),
            );
            let result = resolve_edges(&[edge], &index, Some(&bindings));
            assert!(result.resolved.is_empty(), "{binding}: never read as proof");
            let meta = unresolved_meta(&result, 0);
            assert_eq!(
                meta["nameOnlyReason"], "self_call_receiver_unproven",
                "{binding}"
            );
            assert_eq!(
                meta["nameOnlyCandidates"],
                serde_json::json!(["C.run", "run"]),
                "{binding}"
            );
        }
    }

    #[test]
    fn python_self_key_without_a_carrier_binds_through_a_file_level_namespace_alias_as_today() {
        let (mut index, bindings) = alias_fixture(true);
        let module_fn = py_symbol("modfn", "modfn", "modfn", "FUNCTION", "r:a.py");
        index.nodes_by_uid.insert("modfn".into(), module_fn.clone());
        index
            .node_uid_to_file_uid
            .insert("modfn".into(), "r:a.py".into());
        let result = resolve_edges(
            &[py_call_edge("e1", "modfn", "self.run")],
            &index,
            Some(&bindings),
        );
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].target_node_uid, "helper_run");
        assert_eq!(result.resolved[0].resolution, Resolution::Static);
    }

    #[test]
    fn superclass_metadata_parses_raw_python_base_lists() {
        assert_eq!(
            superclasses_from_metadata(Some(r#"{"superclass":"base.BaseHandler"}"#)),
            vec!["BaseHandler".to_string()]
        );
        assert_eq!(
            superclasses_from_metadata(Some(r#"{"superclass":"Base, metaclass=ABCMeta"}"#)),
            vec!["Base".to_string()]
        );
        assert_eq!(
            superclasses_from_metadata(Some(r#"{"superclass":"Generic[T]"}"#)),
            vec!["Generic".to_string()]
        );
        assert_eq!(
            superclasses_from_metadata(Some(r#"{"superclass":"A, B, C"}"#)),
            vec!["A".to_string(), "B".to_string(), "C".to_string()]
        );
        // Absent / unreadable / non-superclass metadata → empty, never a guess.
        assert!(superclasses_from_metadata(None).is_empty());
        assert!(superclasses_from_metadata(Some("not json")).is_empty());
        assert!(superclasses_from_metadata(Some(r#"{"forward_decl":true}"#)).is_empty());
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

    // ── Java suffix stage (IMPORT-RESOLUTION-JAVA-1) ─────────

    fn java_index(paths: &[&str]) -> JavaSuffixIndex {
        build_java_suffix_index(&paths.iter().map(|p| p.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn java_suffix_resolves_by_package_path() {
        let idx = java_index(&["clients/src/main/java/org/apache/kafka/common/Foo.java"]);
        assert_eq!(
            resolve_java_suffix("org.apache.kafka.common.Foo", &idx),
            JavaSuffixLookup::Resolved(
                "clients/src/main/java/org/apache/kafka/common/Foo.java".to_string()
            )
        );
    }

    #[test]
    fn java_suffix_shortens_for_nested_class() {
        // `org.x.Outer.Inner` — no `Inner.java`; shortening finds `Outer.java`.
        let idx = java_index(&["core/src/main/java/org/x/Outer.java"]);
        assert_eq!(
            resolve_java_suffix("org.x.Outer.Inner", &idx),
            JavaSuffixLookup::Resolved("core/src/main/java/org/x/Outer.java".to_string())
        );
        // A static member import (`org.x.Outer.CONSTANT`) resolves the same way.
        assert_eq!(
            resolve_java_suffix("org.x.Outer.CONSTANT", &idx),
            JavaSuffixLookup::Resolved("core/src/main/java/org/x/Outer.java".to_string())
        );
    }

    #[test]
    fn java_suffix_ambiguous_when_shaded() {
        // Two files with the same package-qualified name (a shaded copy) → ambiguous, counted.
        let idx = java_index(&[
            "netty/src/main/java/io/grpc/netty/Handler.java",
            "netty/shaded/src/main/java/io/grpc/netty/Handler.java",
        ]);
        assert_eq!(
            resolve_java_suffix("io.grpc.netty.Handler", &idx),
            JavaSuffixLookup::Ambiguous
        );
    }

    #[test]
    fn java_suffix_not_found_for_external() {
        let idx = java_index(&["core/src/main/java/org/x/Foo.java"]);
        // An external dependency's FQN — no matching `.java` file in the corpus.
        assert_eq!(
            resolve_java_suffix("com.google.common.collect.ImmutableList", &idx),
            JavaSuffixLookup::NotFound
        );
    }

    #[test]
    fn java_suffix_boundary_guard_rejects_mid_filename() {
        // `common/Foo.java` must NOT match a file whose basename is `UncommonFoo.java`.
        let idx = java_index(&["a/b/pkg/Foo.java"]);
        // Import `x.Foo` → suffix `x/Foo.java`; the only `Foo.java` is under `.../pkg/`, so the
        // boundary suffix `x/Foo.java` does not match `a/b/pkg/Foo.java`.
        assert_eq!(
            resolve_java_suffix("x.Foo", &idx),
            JavaSuffixLookup::NotFound
        );
    }

    #[test]
    fn java_import_wildcard_is_named_basis() {
        let idx = java_index(&["core/src/main/java/org/x/Foo.java"]);
        let empty_nodes: HashMap<String, ResolverNode> = HashMap::new();
        assert!(matches!(
            resolve_java_import("org.x.*", &idx, &empty_nodes, "r1"),
            TargetResolution::JavaWildcard
        ));
    }

    #[test]
    fn java_import_resolves_to_file_node() {
        let idx = java_index(&["core/src/main/java/org/x/Foo.java"]);
        let node = make_node(
            "fileFoo",
            "r1:core/src/main/java/org/x/Foo.java:FILE",
            "Foo.java",
            None,
            None,
        );
        let mut nodes = HashMap::new();
        nodes.insert(node.stable_key.clone(), node);
        assert!(matches!(
            resolve_java_import("org.x.Foo", &idx, &nodes, "r1"),
            TargetResolution::Resolved(uid) if uid == "fileFoo"
        ));
    }

    #[test]
    fn build_java_suffix_index_ignores_non_java() {
        let idx = java_index(&["src/A.java", "src/B.ts", "src/c.py", "src/nested/A.java"]);
        // Two `A.java`, no bucket for non-java files.
        assert_eq!(idx.get("A.java").map(|v| v.len()), Some(2));
        assert!(!idx.contains_key("B.ts"));
        assert!(!idx.contains_key("c.py"));
    }

    // ── resolve_edges integration ────────────────────────────

    #[test]
    fn resolve_import_by_stable_key() {
        let target_node = make_node("file1", "r1:src/utils.ts:FILE", "utils.ts", None, None);
        let mut index = ResolverIndex {
            nodes_by_stable_key: HashMap::new(),
            nodes_by_name: HashMap::new(),
            nodes_by_qualified_name: HashMap::new(),
            nodes_by_uid: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
            java_suffix_index: HashMap::new(),
            npm_workspace_packages: HashMap::new(),
            tsconfig_paths: Default::default(),
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
            nodes_by_qualified_name: HashMap::new(),
            nodes_by_uid: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
            java_suffix_index: HashMap::new(),
            npm_workspace_packages: HashMap::new(),
            tsconfig_paths: Default::default(),
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
            nodes_by_qualified_name: HashMap::new(),
            nodes_by_uid: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
            java_suffix_index: HashMap::new(),
            npm_workspace_packages: HashMap::new(),
            tsconfig_paths: Default::default(),
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
                        forward_decl: false,
                        superclasses: Vec::new(),
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
            resolve_import_target("b::util", &nodes, &files, "r1", None, &roots, false, None),
            None,
            "a non-Rust IMPORTS edge must not resolve via the declared-Rust-crate stage"
        );

        // Control: the SAME inputs from a Rust edge (is_rust_import = true) DO resolve —
        // proving the None above is the gate, not a missing fixture.
        assert_eq!(
            resolve_import_target("b::util", &nodes, &files, "r1", None, &roots, true, None),
            Some("uid::r1:b/src/util.rs:FILE".to_string()),
            "the identical Rust IMPORTS edge must resolve to the defining file"
        );
    }

    // ── Rust enclosing-module import stage 3.6 (RUST-SELF-RESOLUTION-1, RG-REQ-006-L14) ──

    const RUST_EXTRACTOR: &str = "rust-core:0.3.0";

    /// A resolver index over `paths` (FILE nodes `n:<path>`) with the declared Cargo crates
    /// `roots` (`name → root`), as the orchestrator builds `rust_crate_roots`.
    fn rust_index(paths: &[&str], roots: &[(&str, &str)]) -> ResolverIndex {
        let mut index = c_include_index(paths);
        index.rust_crate_roots = crate_roots(roots);
        index
    }

    /// A Rust `use` IMPORTS edge from the FILE node of `source_path` whose target key is the
    /// recorded specifier, with the extractor's carrier keys.
    fn rust_edge(source_path: &str, specifier: &str) -> ExtractedEdge {
        let mut e = make_edge("e-rust", specifier, EdgeType::Imports);
        e.source_node_uid = format!("n:{source_path}");
        e.extractor = RUST_EXTRACTOR.into();
        e.metadata_json =
            Some(serde_json::json!({"specifier": specifier, "identifier": "X"}).to_string());
        e
    }

    /// `rust_edge` written inside the inline `mod` bodies `inline` (outermost first).
    fn rust_edge_inline(source_path: &str, specifier: &str, inline: &[&str]) -> ExtractedEdge {
        let mut e = rust_edge(source_path, specifier);
        e.metadata_json = Some(
            serde_json::json!({"specifier": specifier, "identifier": "X", "inlineModulePath": inline})
                .to_string(),
        );
        e
    }

    /// The bound target path, or None when the row stays unresolved. An unresolved row keeps its
    /// recorded specifier as its target key and its carrier unchanged.
    fn rust_bind(index: &ResolverIndex, e: &ExtractedEdge) -> Option<String> {
        rust_bind_declared(index, &RustModDeclsByFile::new(), e)
    }

    /// The FILE node of `path` (file uid `r1:<path>`) with `rust_mod_decls` naming `names` at
    /// the file's top level, as the Rust extractor writes the key, read back through
    /// [`rust_mod_decls_from_metadata`] — the one reader the orchestrator uses.
    fn file_with_mod_decls(path: &str, names: &[&str]) -> (String, HashSet<RustModDeclaration>) {
        let decls: Vec<serde_json::Value> = names
            .iter()
            .map(|n| serde_json::json!({"name": n, "cfg_test": false}))
            .collect();
        file_with_raw_mod_decls(path, serde_json::json!({ "rust_mod_decls": decls }))
    }

    /// `file_with_mod_decls` from a raw FILE-node metadata object.
    fn file_with_raw_mod_decls(
        path: &str,
        metadata: serde_json::Value,
    ) -> (String, HashSet<RustModDeclaration>) {
        (
            format!("r1:{path}"),
            rust_mod_decls_from_metadata(Some(&metadata.to_string())),
        )
    }

    /// `rust_bind` with the FILE nodes' `rust_mod_decls` (D-RSR-ENTRY-CLIMB-1's declaration
    /// evidence), as the orchestrator passes them to the resolver.
    fn rust_bind_declared(
        index: &ResolverIndex,
        decls: &RustModDeclsByFile,
        e: &ExtractedEdge,
    ) -> Option<String> {
        let result = resolve_edges_with_rust_mod_decls(std::slice::from_ref(e), index, None, decls);
        if let Some(r) = result.resolved.first() {
            assert_eq!(r.resolution, Resolution::Static);
            assert_eq!(
                r.metadata_json, e.metadata_json,
                "no basis carrier is added"
            );
            return Some(r.target_node_uid.trim_start_matches("n:").to_string());
        }
        assert_eq!(result.still_unresolved.len(), 1);
        let u = &result.still_unresolved[0];
        assert_eq!(u.edge.target_key, e.target_key);
        assert_eq!(u.edge.metadata_json, e.metadata_json);
        assert_eq!(u.category, UnresolvedEdgeCategory::ImportsFileNotFound);
        None
    }

    #[test]
    fn rust_module_path_crate_head_from_leaf_file() {
        let index = rust_index(
            &[
                "b/src/lib.rs",
                "b/src/aggregators/trust.rs",
                "b/src/dto/signal.rs",
            ],
            &[("b", "b")],
        );
        let e = rust_edge("b/src/aggregators/trust.rs", "crate::dto::signal");
        assert_eq!(
            rust_bind(&index, &e).as_deref(),
            Some("b/src/dto/signal.rs")
        );
    }

    #[test]
    fn rust_module_path_crate_head_shortens_to_parent_module() {
        let index = rust_index(
            &["b/src/lib.rs", "b/src/x.rs", "b/src/crud/mod.rs"],
            &[("b", "b")],
        );
        let e = rust_edge("b/src/x.rs", "crate::crud::test_helpers");
        assert_eq!(rust_bind(&index, &e).as_deref(), Some("b/src/crud/mod.rs"));
    }

    #[test]
    fn rust_module_path_bare_crate_binds_the_entry_point() {
        let index = rust_index(
            &["b/src/lib.rs", "b/src/main.rs", "b/src/a.rs"],
            &[("b", "b")],
        );
        assert_eq!(
            rust_bind(&index, &rust_edge("b/src/a.rs", "crate")).as_deref(),
            Some("b/src/lib.rs")
        );
        let index = rust_index(&["b/src/main.rs", "b/src/a.rs"], &[("b", "b")]);
        assert_eq!(
            rust_bind(&index, &rust_edge("b/src/a.rs", "crate")).as_deref(),
            Some("b/src/main.rs")
        );
    }

    #[test]
    fn rust_module_path_super_from_leaf_file() {
        let index = rust_index(
            &[
                "b/src/lib.rs",
                "b/src/handlers/quality/tests.rs",
                "b/src/handlers/quality/support.rs",
                "b/src/handlers/support.rs",
            ],
            &[("b", "b")],
        );
        let src = "b/src/handlers/quality/tests.rs";
        assert_eq!(
            rust_bind(&index, &rust_edge(src, "super::support")).as_deref(),
            Some("b/src/handlers/quality/support.rs")
        );
        assert_eq!(
            rust_bind(&index, &rust_edge(src, "super::super::support")).as_deref(),
            Some("b/src/handlers/support.rs")
        );
    }

    #[test]
    fn rust_module_path_super_from_mod_rs() {
        let index = rust_index(
            &[
                "b/src/lib.rs",
                "b/src/a/mod.rs",
                "b/src/x.rs",
                "b/src/a/x.rs",
            ],
            &[("b", "b")],
        );
        assert_eq!(
            rust_bind(&index, &rust_edge("b/src/a/mod.rs", "super::x")).as_deref(),
            Some("b/src/x.rs")
        );
    }

    #[test]
    fn rust_module_path_bare_super_binds_the_parent_module_file() {
        let index = rust_index(
            &["b/src/lib.rs", "b/src/a.rs", "b/src/a/b.rs"],
            &[("b", "b")],
        );
        assert_eq!(
            rust_bind(&index, &rust_edge("b/src/a/b.rs", "super")).as_deref(),
            Some("b/src/a.rs")
        );
        let index = rust_index(
            &["b/src/lib.rs", "b/src/a/mod.rs", "b/src/a/b.rs"],
            &[("b", "b")],
        );
        assert_eq!(
            rust_bind(&index, &rust_edge("b/src/a/b.rs", "super")).as_deref(),
            Some("b/src/a/mod.rs")
        );
    }

    #[test]
    fn rust_module_path_self_head_from_mod_rs_and_leaf() {
        let index = rust_index(
            &[
                "b/src/lib.rs",
                "b/src/a/mod.rs",
                "b/src/a/x.rs",
                "b/src/a/b.rs",
                "b/src/a/b/x.rs",
            ],
            &[("b", "b")],
        );
        assert_eq!(
            rust_bind(&index, &rust_edge("b/src/a/mod.rs", "self::x")).as_deref(),
            Some("b/src/a/x.rs")
        );
        assert_eq!(
            rust_bind(&index, &rust_edge("b/src/a/b.rs", "self::x")).as_deref(),
            Some("b/src/a/b/x.rs")
        );
    }

    #[test]
    fn rust_module_path_sibling_path_reads_as_self_after_declared_crate_miss() {
        let index = rust_index(
            &[
                "b/src/lib.rs",
                "b/src/cli/mod.rs",
                "b/src/cli/context.rs",
                "b/src/context.rs",
            ],
            &[("b", "b")],
        );
        // The FILE node `b/src/cli/mod.rs` declares `mod context;` (D-RSR-ENTRY-CLIMB-1).
        let decls =
            RustModDeclsByFile::from([file_with_mod_decls("b/src/cli/mod.rs", &["context"])]);
        assert_eq!(
            rust_bind_declared(
                &index,
                &decls,
                &rust_edge("b/src/cli/mod.rs", "context::open_storage")
            )
            .as_deref(),
            Some("b/src/cli/context.rs")
        );
    }

    #[test]
    fn rust_module_path_declared_crate_name_wins_over_a_sibling_module() {
        let index = rust_index(
            &[
                "b/src/lib.rs",
                "b/src/git.rs",
                "git/src/lib.rs",
                "git/src/x.rs",
            ],
            &[("b", "b"), ("git", "git")],
        );
        // `b/src/lib.rs` also declares `mod git;` and `b/src/git.rs` exists: the sibling reading
        // is available, and stage 3.5's declared crate still wins.
        let decls = RustModDeclsByFile::from([file_with_mod_decls("b/src/lib.rs", &["git"])]);
        assert_eq!(
            rust_bind_declared(&index, &decls, &rust_edge("b/src/lib.rs", "git::x")).as_deref(),
            Some("git/src/x.rs")
        );
    }

    #[test]
    fn rust_module_path_target_equal_to_source_binds_nothing() {
        // D-RSR-SELF-IMPORT-1: a ladder hit equal to the source file binds nothing.
        let index = rust_index(&["b/src/lib.rs", "b/src/livegraph_feed.rs"], &[("b", "b")]);
        assert_eq!(
            rust_bind(
                &index,
                &rust_edge("b/src/livegraph_feed.rs", "crate::livegraph_feed")
            ),
            None
        );
        // `commands` from the entry file with no `commands.rs` (vscode `cli/src/lib.rs`): the
        // file-list ladder would shorten to `lib.rs`, the source. Under L14 revision 3 a sibling
        // path never shortens past its first segment, so the row binds nothing even when
        // `lib.rs` declares `mod commands;`.
        let decls = RustModDeclsByFile::from([file_with_mod_decls("b/src/lib.rs", &["commands"])]);
        assert_eq!(
            rust_bind_declared(&index, &decls, &rust_edge("b/src/lib.rs", "commands")),
            None
        );
    }

    #[test]
    fn rust_module_path_source_outside_src_or_under_bin_binds_nothing() {
        let index = rust_index(
            &[
                "b/src/lib.rs",
                "b/src/util.rs",
                "b/tests/pipeline.rs",
                "b/src/bin/tool.rs",
                "b/benches/x.rs",
            ],
            &[("b", "b")],
        );
        for src in ["b/tests/pipeline.rs", "b/src/bin/tool.rs", "b/benches/x.rs"] {
            for spec in ["crate::util", "util", "self::util"] {
                assert_eq!(
                    rust_bind(&index, &rust_edge(src, spec)),
                    None,
                    "{src} {spec}"
                );
            }
        }
    }

    #[test]
    fn rust_module_path_super_above_root_and_no_crate_root_bind_nothing() {
        let index = rust_index(
            &[
                "b/src/lib.rs",
                "b/src/a.rs",
                "b/src/x.rs",
                "fixtures/torture.rs",
                "src/x.rs",
            ],
            &[("b", "b")],
        );
        assert_eq!(
            rust_bind(&index, &rust_edge("b/src/a.rs", "super::super::x")),
            None
        );
        for spec in ["crate::x", "super::x", "self::x", "x"] {
            assert_eq!(
                rust_bind(&index, &rust_edge("fixtures/torture.rs", spec)),
                None,
                "{spec}"
            );
        }
    }

    #[test]
    fn rust_module_path_non_rust_edge_never_enters() {
        let index = rust_index(
            &["b/src/lib.rs", "b/src/x.rs", "b/src/app.ts"],
            &[("b", "b")],
        );
        let mut e = rust_edge("b/src/app.ts", "crate::x");
        e.extractor = "ts-core:0.2.0".into();
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert!(result.resolved.is_empty());
        assert_eq!(result.still_unresolved.len(), 1);
    }

    #[test]
    fn rust_module_path_inline_module_super_resolves_from_the_enclosing_module() {
        let index = rust_index(
            &[
                "b/src/lib.rs",
                "b/src/a/b.rs",
                "b/src/a/b/sibling.rs",
                "b/src/a/sibling.rs",
            ],
            &[("b", "b")],
        );
        let e = rust_edge_inline("b/src/a/b.rs", "super::sibling", &["tests"]);
        assert_eq!(
            rust_bind(&index, &e).as_deref(),
            Some("b/src/a/b/sibling.rs")
        );
    }

    #[test]
    fn rust_module_path_inline_module_super_to_own_file_binds_nothing() {
        // D-RSR-INLINE-MODULE-1: inside `mod tests { use super::…; }` in `a/b.rs`, `super` is
        // `a::b` — the source file — not `a` (which the file-only reading would bind).
        let index = rust_index(
            &["b/src/lib.rs", "b/src/a.rs", "b/src/a/b.rs"],
            &[("b", "b")],
        );
        let e = rust_edge_inline("b/src/a/b.rs", "super", &["tests"]);
        assert_eq!(rust_bind(&index, &e), None);
    }

    #[test]
    fn rust_module_path_sibling_binds_only_with_a_mod_declaration() {
        // D-RSR-ENTRY-CLIMB-1: a sibling path binds only when the enclosing module declares
        // `mod <first>;` — the FILE node's `rust_mod_decls` entry whose `inline_path` equals the
        // `use`'s `inlineModulePath`.
        let index = rust_index(
            &[
                "b/src/lib.rs",
                "b/src/cli/mod.rs",
                "b/src/cli/context.rs",
                "b/src/cli/inner/context.rs",
            ],
            &[("b", "b")],
        );
        let declared =
            RustModDeclsByFile::from([file_with_mod_decls("b/src/cli/mod.rs", &["context"])]);
        let e = rust_edge("b/src/cli/mod.rs", "context::open_storage");
        assert_eq!(
            rust_bind_declared(&index, &declared, &e).as_deref(),
            Some("b/src/cli/context.rs")
        );
        // The same path from a file that does NOT declare `context`: nothing, though
        // `b/src/cli/context.rs` exists.
        assert_eq!(rust_bind(&index, &e), None);
        let other = RustModDeclsByFile::from([file_with_mod_decls("b/src/cli/mod.rs", &["other"])]);
        assert_eq!(rust_bind_declared(&index, &other, &e), None);
        // A declaration written inside `mod inner { mod context; }` is matched on `inline_path`:
        // it serves a `use` inside `mod inner`, never a top-level `use`.
        let inner = RustModDeclsByFile::from([file_with_raw_mod_decls(
            "b/src/cli/mod.rs",
            serde_json::json!({"rust_mod_decls": [
                {"name": "context", "cfg_test": false, "inline_path": ["inner"]}
            ]}),
        )]);
        let nested = rust_edge_inline("b/src/cli/mod.rs", "context::open_storage", &["inner"]);
        assert_eq!(
            rust_bind_declared(&index, &inner, &nested).as_deref(),
            Some("b/src/cli/inner/context.rs")
        );
        assert_eq!(rust_bind_declared(&index, &inner, &e), None);
    }

    #[test]
    fn rust_module_path_external_crate_name_from_main_rs_binds_nothing() {
        // Admission 1's six false rows (`rust/tools/xpart-probe/src/main.rs:22`
        // `use std::collections::{BTreeMap, BTreeSet, HashMap};`): an undeclared first segment
        // from a `main.rs` beside a `lib.rs` never climbs to `lib.rs`.
        let index = rust_index(
            &["b/src/lib.rs", "b/src/main.rs", "b/src/cli.rs"],
            &[("b", "b")],
        );
        let e = rust_edge("b/src/main.rs", "std::collections");
        assert_eq!(rust_bind(&index, &e), None);
        // Declaring other modules does not change it.
        let decls = RustModDeclsByFile::from([file_with_mod_decls("b/src/main.rs", &["cli"])]);
        assert_eq!(rust_bind_declared(&index, &decls, &e), None);
    }

    #[test]
    fn rust_module_path_declared_crate_miss_never_falls_to_a_sibling_reading() {
        // `git` is a declared crate whose `git/src/x.rs` and `git/src/lib.rs` are NOT indexed;
        // `b/src/git.rs` exists and `b/src/lib.rs` declares `mod git;`. Stage 3.5 missed; a
        // declared crate name never enters the sibling reading of stage 3.6.
        let files = ["b/src/lib.rs", "b/src/git.rs"];
        let decls = RustModDeclsByFile::from([file_with_mod_decls("b/src/lib.rs", &["git"])]);
        let e = rust_edge("b/src/lib.rs", "git::x");
        let index = rust_index(&files, &[("b", "b"), ("git", "git")]);
        assert_eq!(rust_bind_declared(&index, &decls, &e), None);
        // Control: when `git` is not a declared crate, the same row is a sibling reading.
        let index = rust_index(&files, &[("b", "b")]);
        assert_eq!(
            rust_bind_declared(&index, &decls, &e).as_deref(),
            Some("b/src/git.rs")
        );
    }

    #[test]
    fn rust_module_path_declared_first_segment_without_a_module_file_binds_nothing() {
        // L14 revision 3: `mod support;` is declared, but neither `<enclosing>/support.rs` nor
        // `<enclosing>/support/mod.rs` is indexed (an inline body or a `#[path]` module). The
        // declaration guard passes, the first-segment-file guard fails: nothing binds — the
        // ladder would otherwise shorten past the first segment.
        //
        // The slice's named shape: from `b/src/a/mod.rs`. `b/src/a.rs` is indexed too so the
        // unguarded ladder would land on a file other than the source.
        let index = rust_index(
            &["b/src/lib.rs", "b/src/a/mod.rs", "b/src/a.rs"],
            &[("b", "b")],
        );
        let decls = RustModDeclsByFile::from([file_with_mod_decls("b/src/a/mod.rs", &["support"])]);
        assert_eq!(
            rust_bind_declared(&index, &decls, &rust_edge("b/src/a/mod.rs", "support::x")),
            None
        );
        // From a `main.rs` beside a `lib.rs`: the unguarded ladder would end at `lib.rs`.
        let index = rust_index(&["b/src/lib.rs", "b/src/main.rs"], &[("b", "b")]);
        let decls = RustModDeclsByFile::from([file_with_mod_decls("b/src/main.rs", &["support"])]);
        assert_eq!(
            rust_bind_declared(&index, &decls, &rust_edge("b/src/main.rs", "support::x")),
            None
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
            nodes_by_qualified_name: HashMap::new(),
            nodes_by_uid: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
            java_suffix_index: HashMap::new(),
            npm_workspace_packages: HashMap::new(),
            tsconfig_paths: Default::default(),
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
            nodes_by_qualified_name: HashMap::new(),
            nodes_by_uid: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
            java_suffix_index: HashMap::new(),
            npm_workspace_packages: HashMap::new(),
            tsconfig_paths: Default::default(),
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
            nodes_by_qualified_name: HashMap::new(),
            nodes_by_uid: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
            java_suffix_index: HashMap::new(),
            npm_workspace_packages: HashMap::new(),
            tsconfig_paths: Default::default(),
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
            nodes_by_qualified_name: HashMap::new(),
            nodes_by_uid: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
            java_suffix_index: HashMap::new(),
            npm_workspace_packages: HashMap::new(),
            tsconfig_paths: Default::default(),
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
            nodes_by_qualified_name: HashMap::new(),
            nodes_by_uid: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
            java_suffix_index: HashMap::new(),
            npm_workspace_packages: HashMap::new(),
            tsconfig_paths: Default::default(),
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
            nodes_by_qualified_name: HashMap::new(),
            nodes_by_uid: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
            java_suffix_index: HashMap::new(),
            npm_workspace_packages: HashMap::new(),
            tsconfig_paths: Default::default(),
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
            nodes_by_qualified_name: HashMap::new(),
            nodes_by_uid: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
            java_suffix_index: HashMap::new(),
            npm_workspace_packages: HashMap::new(),
            tsconfig_paths: Default::default(),
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

    // ── Python `from X import Y` submodule inference (PYTHON-SUBMODULE-IMPORT-1) ──

    const PY_EXTRACTOR: &str = "python-core:0.1.0";

    /// A FILE resolver node for `r1:<path>:FILE` whose uid is `n:<path>` and file uid `r1:<path>`.
    fn py_file_node(path: &str) -> ResolverNode {
        ResolverNode {
            node_uid: format!("n:{path}"),
            stable_key: format!("r1:{path}:FILE"),
            name: path.rsplit('/').next().unwrap_or(path).into(),
            qualified_name: Some(path.into()),
            kind: "FILE".into(),
            subtype: None,
            file_uid: Some(format!("r1:{path}")),
            forward_decl: false,
            superclasses: Vec::new(),
        }
    }

    /// A resolver index over the given repo-relative file paths plus the importing file
    /// `consumer/app.py` (source node `src1`, the `make_edge` default): every file is a FILE
    /// node in `nodes_by_stable_key`, `file_resolution` is the production map
    /// (`build_file_resolution_map`, so `r1:pkg:FILE → r1:pkg/__init__.py:FILE`).
    fn py_pkg_index(paths: &[&str]) -> ResolverIndex {
        let mut index = empty_index();
        let mut all: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
        all.push("consumer/app.py".into());
        index.file_resolution = build_file_resolution_map(&all, "r1");
        for p in &all {
            let node = py_file_node(p);
            index
                .node_uid_to_file_uid
                .insert(node.node_uid.clone(), node.file_uid.clone().unwrap());
            index
                .nodes_by_uid
                .insert(node.node_uid.clone(), node.clone());
            index
                .nodes_by_stable_key
                .insert(node.stable_key.clone(), node);
        }
        index
            .node_uid_to_file_uid
            .insert("src1".into(), "r1:consumer/app.py".into());
        index
    }

    /// Add a SYMBOL node named `name` with `qualified_name` to `pkg/__init__.py`.
    fn add_init_symbol(index: &mut ResolverIndex, name: &str, qualified_name: &str) {
        let node = ResolverNode {
            node_uid: format!("sym:{qualified_name}"),
            stable_key: format!("r1:pkg/__init__.py#{qualified_name}:SYMBOL"),
            name: name.into(),
            qualified_name: Some(qualified_name.into()),
            kind: "SYMBOL".into(),
            subtype: None,
            file_uid: Some("r1:pkg/__init__.py".into()),
            forward_decl: false,
            superclasses: Vec::new(),
        };
        index
            .node_uid_to_file_uid
            .insert(node.node_uid.clone(), "r1:pkg/__init__.py".into());
        index
            .nodes_by_name
            .entry(name.into())
            .or_default()
            .push(node);
    }

    fn py_binding(identifier: &str, specifier: &str, imported_name: Option<&str>) -> ImportBinding {
        ImportBinding {
            identifier: identifier.into(),
            specifier: specifier.into(),
            is_relative: specifier.starts_with('.'),
            location: None,
            is_type_only: false,
            imported_name: imported_name.map(|s| s.into()),
            kind: if imported_name.is_some() {
                ImportKind::Named
            } else {
                ImportKind::Namespace
            },
        }
    }

    /// The init's import bindings, keyed by the init's file uid (the orchestrator's shape).
    fn init_bindings(bindings: Vec<ImportBinding>) -> HashMap<String, Vec<ImportBinding>> {
        let mut m = HashMap::new();
        if !bindings.is_empty() {
            m.insert("r1:pkg/__init__.py".to_string(), bindings);
        }
        m
    }

    /// Edge (E): `from pkg import <name>` as the Python extractor emits it.
    fn py_from_import(uid: &str, target_key: &str, specifier: &str, name: &str) -> ExtractedEdge {
        let mut e = make_edge(uid, target_key, EdgeType::Imports);
        e.extractor = PY_EXTRACTOR.into();
        e.metadata_json = Some(
            serde_json::json!({"specifier": specifier, "identifier": name, "importedName": name})
                .to_string(),
        );
        e
    }

    fn meta(edge: &ResolvedEdge) -> serde_json::Value {
        serde_json::from_str(edge.metadata_json.as_deref().expect("metadata")).unwrap()
    }

    /// Assert the edge was retargeted to `target` as INFERRED with the package init as the
    /// alternate candidate, `basis: python_submodule`, and the extractor's keys preserved.
    fn assert_inferred(result: &ResolutionResult, target: &str, input: &ExtractedEdge) {
        assert_eq!(result.resolved.len(), 1, "{:?}", result.still_unresolved);
        let r = &result.resolved[0];
        assert_eq!(r.target_node_uid, format!("n:{target}"));
        assert_eq!(r.resolution, Resolution::Inferred);
        let mut want: serde_json::Value =
            serde_json::from_str(input.metadata_json.as_deref().unwrap()).unwrap();
        want["alternateTarget"] = serde_json::json!("r1:pkg/__init__.py:FILE");
        want["basis"] = serde_json::json!("python_submodule");
        assert_eq!(meta(r), want);
    }

    /// Assert the edge kept today's result: the package init, `static`, metadata byte-identical.
    fn assert_init_static(result: &ResolutionResult, input: &ExtractedEdge) {
        assert_eq!(result.resolved.len(), 1, "{:?}", result.still_unresolved);
        let r = &result.resolved[0];
        assert_eq!(r.target_node_uid, "n:pkg/__init__.py");
        assert_eq!(r.resolution, Resolution::Static);
        assert_eq!(r.metadata_json, input.metadata_json);
    }

    #[test]
    fn python_from_package_import_of_a_sibling_module_resolves_inferred_to_the_module_file_with_the_init_as_alternate(
    ) {
        let index = py_pkg_index(&["pkg/__init__.py", "pkg/base.py"]);
        let e = py_from_import("e1", "pkg", "pkg", "base");
        let result = resolve_edges(
            std::slice::from_ref(&e),
            &index,
            Some(&init_bindings(vec![])),
        );
        assert_inferred(&result, "pkg/base.py", &e);
    }

    #[test]
    fn python_from_package_import_of_a_subpackage_resolves_inferred_to_its_init_with_the_package_init_as_alternate(
    ) {
        let index = py_pkg_index(&["pkg/__init__.py", "pkg/models/__init__.py"]);
        let e = py_from_import("e1", "pkg", "pkg", "models");
        let result = resolve_edges(
            std::slice::from_ref(&e),
            &index,
            Some(&init_bindings(vec![])),
        );
        assert_inferred(&result, "pkg/models/__init__.py", &e);
    }

    #[test]
    fn python_from_package_import_prefers_the_subpackage_over_a_same_named_module_file() {
        let index = py_pkg_index(&["pkg/__init__.py", "pkg/base.py", "pkg/base/__init__.py"]);
        let e = py_from_import("e1", "pkg", "pkg", "base");
        let result = resolve_edges(
            std::slice::from_ref(&e),
            &index,
            Some(&init_bindings(vec![])),
        );
        assert_inferred(&result, "pkg/base/__init__.py", &e);
    }

    /// F-PSI-01: the stage runs BEFORE Stage 2. With both `pkg.py` and `pkg/__init__.py` indexed,
    /// Stage 2's extensionless entry `r1:pkg:FILE` is first-writer-wins and may name `pkg.py`; the
    /// Python rule (the package directory wins, as Python's FileFinder) must still apply, in either
    /// file-list order — and on a miss the old ladder's result stands.
    #[test]
    fn python_from_package_import_applies_before_stage_two_when_a_same_named_module_file_exists() {
        for order in [
            ["pkg.py", "pkg/__init__.py", "pkg/base.py"],
            ["pkg/__init__.py", "pkg.py", "pkg/base.py"],
        ] {
            let index = py_pkg_index(&order);
            let e = py_from_import("e1", "pkg", "pkg", "base");
            let result = resolve_edges(
                std::slice::from_ref(&e),
                &index,
                Some(&init_bindings(vec![])),
            );
            assert_inferred(&result, "pkg/base.py", &e);

            // A miss (a symbol import) takes the old ladder unchanged, whatever Stage 2 picks.
            let sym = py_from_import("e2", "pkg", "pkg", "Thing");
            let ladder = resolve_import_ladder(&sym, &index, &RustModDeclsByFile::new());
            let result = resolve_edges(
                std::slice::from_ref(&sym),
                &index,
                Some(&init_bindings(vec![])),
            );
            let TargetResolution::Resolved(ladder_uid) = ladder else {
                panic!("the ladder resolves `pkg` in order {order:?}");
            };
            assert_eq!(result.resolved.len(), 1);
            assert_eq!(result.resolved[0].target_node_uid, ladder_uid, "{order:?}");
            assert_eq!(result.resolved[0].resolution, Resolution::Static);
            assert_eq!(result.resolved[0].metadata_json, sym.metadata_json);
        }
    }

    /// F-PSI-01 (review 1): when the stage applies (init + submodule indexed) it decides the
    /// target itself. An init whose extracted facts bind `Y` — a module-level symbol `Y`, or an
    /// import of `Y` from elsewhere — keeps the INIT as the `static` target even when a same-named
    /// `pkg.py` is indexed, in either file-list order (Stage 2's extensionless `r1:pkg:FILE` entry
    /// is first-writer-wins and names `pkg.py` when it comes first). With no bindings map at all
    /// there is no evidence that the init leaves `Y` unbound: the init, `static`, too.
    #[test]
    fn python_from_package_import_keeps_the_init_over_a_same_named_module_file_when_the_init_binds_the_name(
    ) {
        for order in [
            ["pkg.py", "pkg/__init__.py", "pkg/base.py", "pkg/other.py"],
            ["pkg/__init__.py", "pkg.py", "pkg/base.py", "pkg/other.py"],
        ] {
            // The competing file is really competing in the first order.
            if order[0] == "pkg.py" {
                let index = py_pkg_index(&order);
                let probe = py_from_import("e0", "pkg", "pkg", "Thing");
                assert!(
                    matches!(
                        resolve_import_ladder(&probe, &index, &RustModDeclsByFile::new()),
                        TargetResolution::Resolved(ref uid) if uid == "n:pkg.py"
                    ),
                    "the ladder picks the module file in order {order:?}"
                );
            }

            // (a) a module-level symbol `base` of the init.
            let mut index = py_pkg_index(&order);
            add_init_symbol(&mut index, "base", "base");
            let e = py_from_import("e1", "pkg", "pkg", "base");
            let result = resolve_edges(
                std::slice::from_ref(&e),
                &index,
                Some(&init_bindings(vec![])),
            );
            assert_init_static(&result, &e);
            assert_eq!(result.resolved_import_pairs.len(), 1, "{order:?}");

            // (b) the init imports `base` from elsewhere.
            let index = py_pkg_index(&order);
            let bindings = init_bindings(vec![py_binding("base", ".other", Some("base"))]);
            let result = resolve_edges(std::slice::from_ref(&e), &index, Some(&bindings));
            assert_init_static(&result, &e);

            // (c) no bindings map: no evidence the init leaves `base` unbound.
            let result = resolve_edges(std::slice::from_ref(&e), &index, None);
            assert_init_static(&result, &e);
        }
    }

    #[test]
    fn python_from_package_import_of_a_symbol_keeps_the_package_init_as_a_static_target() {
        let index = py_pkg_index(&["pkg/__init__.py", "pkg/base.py"]);
        let e = py_from_import("e1", "pkg", "pkg", "Thing");
        let result = resolve_edges(
            std::slice::from_ref(&e),
            &index,
            Some(&init_bindings(vec![])),
        );
        assert_init_static(&result, &e);
    }

    #[test]
    fn python_from_package_import_never_retargets_to_a_non_python_file() {
        let index = py_pkg_index(&["pkg/__init__.py", "pkg/base.json"]);
        let e = py_from_import("e1", "pkg", "pkg", "base");
        let result = resolve_edges(
            std::slice::from_ref(&e),
            &index,
            Some(&init_bindings(vec![])),
        );
        assert_init_static(&result, &e);
    }

    #[test]
    fn python_from_package_import_keeps_the_init_when_the_init_defines_the_name() {
        let mut index = py_pkg_index(&["pkg/__init__.py", "pkg/base.py"]);
        add_init_symbol(&mut index, "base", "base");
        let e = py_from_import("e1", "pkg", "pkg", "base");
        let result = resolve_edges(
            std::slice::from_ref(&e),
            &index,
            Some(&init_bindings(vec![])),
        );
        assert_init_static(&result, &e);
    }

    #[test]
    fn python_from_package_import_ignores_a_nested_member_named_like_the_submodule() {
        let mut index = py_pkg_index(&["pkg/__init__.py", "pkg/base.py"]);
        add_init_symbol(&mut index, "base", "Thing.base");
        let e = py_from_import("e1", "pkg", "pkg", "base");
        let result = resolve_edges(
            std::slice::from_ref(&e),
            &index,
            Some(&init_bindings(vec![])),
        );
        assert_inferred(&result, "pkg/base.py", &e);
    }

    #[test]
    fn python_from_package_import_keeps_the_init_when_the_init_imports_the_name_from_elsewhere() {
        let index = py_pkg_index(&["pkg/__init__.py", "pkg/base.py", "pkg/other.py"]);
        let bindings = init_bindings(vec![py_binding("base", ".other", Some("base"))]);
        let e = py_from_import("e1", "pkg", "pkg", "base");
        let result = resolve_edges(std::slice::from_ref(&e), &index, Some(&bindings));
        assert_init_static(&result, &e);
    }

    #[test]
    fn python_from_package_import_keeps_the_init_when_the_init_binds_the_name_to_an_attribute_of_the_submodule(
    ) {
        let index = py_pkg_index(&["pkg/__init__.py", "pkg/base.py"]);
        let bindings = init_bindings(vec![py_binding("base", ".base", Some("base"))]);
        let e = py_from_import("e1", "pkg", "pkg", "base");
        let result = resolve_edges(std::slice::from_ref(&e), &index, Some(&bindings));
        assert_init_static(&result, &e);
    }

    #[test]
    fn python_from_package_import_still_retargets_when_the_init_imports_the_submodule_itself() {
        let index = py_pkg_index(&["pkg/__init__.py", "pkg/base.py"]);
        for binding in [
            py_binding("base", ".", Some("base")),
            py_binding("base", "pkg", Some("base")),
            py_binding("base", "pkg.base", None),
        ] {
            let bindings = init_bindings(vec![binding.clone()]);
            let e = py_from_import("e1", "pkg", "pkg", "base");
            let result = resolve_edges(std::slice::from_ref(&e), &index, Some(&bindings));
            assert_inferred(&result, "pkg/base.py", &e);
        }
    }

    #[test]
    fn python_relative_from_dot_import_of_a_sibling_module_resolves_inferred_to_the_module_file() {
        let index = py_pkg_index(&["pkg/__init__.py", "pkg/base.py"]);
        let e = py_from_import("e1", "r1:pkg:FILE", ".", "base");
        let result = resolve_edges(
            std::slice::from_ref(&e),
            &index,
            Some(&init_bindings(vec![])),
        );
        assert_inferred(&result, "pkg/base.py", &e);
    }

    #[test]
    fn python_inferred_import_never_feeds_the_module_edge_pairs() {
        let index = py_pkg_index(&["pkg/__init__.py", "pkg/base.py"]);
        let inferred = py_from_import("e1", "pkg", "pkg", "base");
        let certain = py_from_import("e2", "pkg", "pkg", "Thing");
        let result = resolve_edges(&[inferred, certain], &index, Some(&init_bindings(vec![])));
        assert_eq!(result.resolved.len(), 2);
        let by_uid = |uid: &str| result.resolved.iter().find(|r| r.edge_uid == uid).unwrap();
        assert_eq!(by_uid("e1").resolution, Resolution::Inferred);
        assert_eq!(by_uid("e2").resolution, Resolution::Static);
        // Only the static import reaches the module-edge derivation.
        assert_eq!(result.resolved_import_pairs.len(), 1);
        assert_eq!(result.resolved_import_pairs[0].0, "src1");
        assert_eq!(result.resolved_import_pairs[0].1, "n:pkg/__init__.py");
    }

    #[test]
    fn python_from_import_without_an_imported_name_carrier_takes_the_old_ladder() {
        let index = py_pkg_index(&["pkg/__init__.py", "pkg/base.py"]);
        let mut e = py_from_import("e1", "pkg", "pkg", "base");
        e.metadata_json = None;
        let result = resolve_edges(
            std::slice::from_ref(&e),
            &index,
            Some(&init_bindings(vec![])),
        );
        assert_init_static(&result, &e);
        assert_eq!(result.resolved_import_pairs.len(), 1);
    }

    #[test]
    fn non_python_import_with_an_imported_name_never_takes_the_python_stage() {
        let index = py_pkg_index(&["pkg/__init__.py", "pkg/base.py"]);
        let mut e = py_from_import("e1", "pkg", "pkg", "base");
        e.extractor = "ts-core:0.2.0".into();
        let result = resolve_edges(
            std::slice::from_ref(&e),
            &index,
            Some(&init_bindings(vec![])),
        );
        assert_init_static(&result, &e);
        assert_eq!(result.resolved_import_pairs.len(), 1);
    }

    // ── C/C++ include suffix / basename stage (CPP-INCLUDE-BASENAME-1, RG-REQ-006-L11) ──

    const C_EXTRACTOR: &str = "c-core:0.1.0";

    /// A resolver index built the way the orchestrator builds it over `paths`: every path is a
    /// FILE node (`n:<path>`, file uid `r1:<path>`), with the production file-resolution,
    /// per-TU include, include-root and Java suffix maps.
    fn c_include_index(paths: &[&str]) -> ResolverIndex {
        let mut index = empty_index();
        let all: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
        index.file_resolution = build_file_resolution_map(&all, "r1");
        index.per_file_include_resolution = build_per_file_include_resolution(&all, "r1");
        index.include_resolver = Some(crate::include_resolver::build_include_resolution_map(
            &all,
            "r1",
            &crate::include_resolver::IncludeResolverConfig::default(),
        ));
        index.java_suffix_index = build_java_suffix_index(&all);
        for p in &all {
            let node = py_file_node(p);
            index
                .node_uid_to_file_uid
                .insert(node.node_uid.clone(), node.file_uid.clone().unwrap());
            index
                .nodes_by_uid
                .insert(node.node_uid.clone(), node.clone());
            index
                .nodes_by_stable_key
                .insert(node.stable_key.clone(), node);
        }
        index
    }

    /// A C include edge from the FILE node of `from` with the carrier the orchestrator stores on
    /// every include (`{"isTypeOnly":false}`).
    fn c_include(uid: &str, from: &str, specifier: &str) -> ExtractedEdge {
        let mut e = make_edge(uid, specifier, EdgeType::Imports);
        e.source_node_uid = format!("n:{from}");
        e.extractor = C_EXTRACTOR.into();
        e.metadata_json = Some(r#"{"isTypeOnly":false}"#.into());
        e
    }

    fn carrier(raw: &Option<String>) -> serde_json::Value {
        serde_json::from_str(raw.as_deref().expect("a carrier")).expect("a JSON carrier")
    }

    #[test]
    fn include_unique_multi_segment_suffix_resolves_static_with_its_basis() {
        let index = c_include_index(&["app/main.cpp", "lib/src/util/foo.h", "lib/src/util/bar.h"]);
        let e = c_include("e1", "app/main.cpp", "util/foo.h");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert_eq!(result.resolved.len(), 1, "{:?}", result.still_unresolved);
        assert!(result.still_unresolved.is_empty());
        let r = &result.resolved[0];
        assert_eq!(r.target_node_uid, "n:lib/src/util/foo.h");
        assert_eq!(
            r.resolution,
            Resolution::Static,
            "a unique path suffix is certain"
        );
        assert_eq!(
            carrier(&r.metadata_json),
            serde_json::json!({
                "isTypeOnly": false,
                "basis": "unique_suffix",
                "candidates": ["r1:lib/src/util/foo.h:FILE"],
            })
        );
        assert_eq!(
            result.resolved_import_pairs,
            vec![(
                "n:app/main.cpp".to_string(),
                "n:lib/src/util/foo.h".to_string(),
                Some(TypeOnlyDisposition::Runtime)
            )],
            "a certain import feeds the persisted module graph"
        );
    }

    #[test]
    fn include_unique_basename_resolves_inferred_with_its_candidate_and_basis() {
        let index = c_include_index(&["src/event/ngx_event.c", "src/core/ngx_core.h"]);
        let e = c_include("e1", "src/event/ngx_event.c", "ngx_core.h");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert_eq!(result.resolved.len(), 1, "{:?}", result.still_unresolved);
        let r = &result.resolved[0];
        assert_eq!(r.target_node_uid, "n:src/core/ngx_core.h");
        assert_eq!(
            r.resolution,
            Resolution::Inferred,
            "a unique basename is a candidate to investigate, never static"
        );
        assert_eq!(
            carrier(&r.metadata_json),
            serde_json::json!({
                "isTypeOnly": false,
                "basis": "unique_basename",
                "candidates": ["r1:src/core/ngx_core.h:FILE"],
            })
        );
        assert!(
            result.resolved_import_pairs.is_empty(),
            "an inferred import never feeds the persisted module graph"
        );

        // An include without a carrier gets one holding only the two keys.
        let mut bare = e.clone();
        bare.metadata_json = None;
        let result = resolve_edges(std::slice::from_ref(&bare), &index, None);
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].resolution, Resolution::Inferred);
        assert_eq!(
            carrier(&result.resolved[0].metadata_json),
            serde_json::json!({
                "basis": "unique_basename",
                "candidates": ["r1:src/core/ngx_core.h:FILE"],
            })
        );
    }

    #[test]
    fn include_non_unique_suffix_or_basename_stays_unresolved_ambiguous_with_candidates() {
        // Basename: `ngx_time.h` exists under unix/ and win32/.
        let index = c_include_index(&[
            "src/core/ngx_core.h",
            "src/os/win32/ngx_time.h",
            "src/os/unix/ngx_time.h",
        ]);
        let e = c_include("e1", "src/core/ngx_core.h", "ngx_time.h");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert!(result.resolved.is_empty(), "never a pick");
        assert_eq!(result.still_unresolved.len(), 1);
        let u = &result.still_unresolved[0];
        assert_eq!(u.category, UnresolvedEdgeCategory::ImportsAmbiguousMatch);
        assert_eq!(u.edge.target_key, "ngx_time.h");
        assert_eq!(
            carrier(&u.edge.metadata_json),
            serde_json::json!({
                "isTypeOnly": false,
                "basis": "ambiguous_basename",
                "candidates": ["r1:src/os/unix/ngx_time.h:FILE", "r1:src/os/win32/ngx_time.h:FILE"],
            })
        );

        // Suffix: `util/foo.h` ends two indexed paths.
        let index = c_include_index(&["app/main.cpp", "b/util/foo.h", "a/util/foo.h"]);
        let e = c_include("e2", "app/main.cpp", "util/foo.h");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert!(result.resolved.is_empty());
        assert!(result.resolved_import_pairs.is_empty());
        let u = &result.still_unresolved[0];
        assert_eq!(u.category, UnresolvedEdgeCategory::ImportsAmbiguousMatch);
        assert_eq!(
            carrier(&u.edge.metadata_json),
            serde_json::json!({
                "isTypeOnly": false,
                "basis": "ambiguous_suffix",
                "candidates": ["r1:a/util/foo.h:FILE", "r1:b/util/foo.h:FILE"],
            })
        );
    }

    #[test]
    fn include_suffix_stage_runs_only_after_every_earlier_stage_misses() {
        let unchanged = Some(r#"{"isTypeOnly":false}"#.to_string());

        // Same directory wins although the basename is not unique.
        let index = c_include_index(&["src/a/main.c", "src/a/foo.h", "src/b/foo.h"]);
        let e = c_include("e1", "src/a/main.c", "foo.h");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].target_node_uid, "n:src/a/foo.h");
        assert_eq!(result.resolved[0].resolution, Resolution::Static);
        assert_eq!(
            result.resolved[0].metadata_json, unchanged,
            "no basis on an earlier stage's answer"
        );

        // A derived include root wins although the path suffix is not unique.
        let index = c_include_index(&[
            "Net/src/a.cpp",
            "Foundation/include/Poco/Exception.h",
            "Other/Poco/Exception.h",
        ]);
        let e = c_include("e2", "Net/src/a.cpp", "Poco/Exception.h");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(
            result.resolved[0].target_node_uid,
            "n:Foundation/include/Poco/Exception.h"
        );
        assert_eq!(result.resolved[0].metadata_json, unchanged);

        // Include-root overlap stays the root resolver's ambiguity, without a basis.
        let index = c_include_index(&[
            "Net/src/b.cpp",
            "Foundation/include/Poco/Exception.h",
            "Util/include/Poco/Exception.h",
        ]);
        let e = c_include("e3", "Net/src/b.cpp", "Poco/Exception.h");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert!(result.resolved.is_empty());
        assert_eq!(
            result.still_unresolved[0].category,
            UnresolvedEdgeCategory::ImportsAmbiguousMatch
        );
        assert_eq!(result.still_unresolved[0].edge.metadata_json, unchanged);

        // The repo-prefix stage wins although the path suffix is not unique (never a flip).
        let index = c_include_index(&["src/x.c", "lib/util.h", "other/lib/util.h"]);
        let e = c_include("e4", "src/x.c", "lib/util.h");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].target_node_uid, "n:lib/util.h");
        assert_eq!(result.resolved[0].resolution, Resolution::Static);
        assert_eq!(result.resolved[0].metadata_json, unchanged);
    }

    #[test]
    fn non_c_family_import_never_reaches_the_suffix_or_basename_stage() {
        let index = c_include_index(&["src/app.ts", "x/core.h", "lib/util/foo.ts"]);
        for extractor in [
            "ts-core:0.2.0",
            "python-core:0.2.0",
            "rust-core:0.2.0",
            "java-core:0.1.0",
            "test:1",
        ] {
            for spec in ["core.h", "util/foo.ts"] {
                let mut e = c_include("e1", "src/app.ts", spec);
                e.extractor = extractor.into();
                let result = resolve_edges(std::slice::from_ref(&e), &index, None);
                assert!(result.resolved.is_empty(), "{extractor} {spec}");
                assert_eq!(result.still_unresolved.len(), 1, "{extractor} {spec}");
                let u = &result.still_unresolved[0];
                assert_eq!(
                    u.category,
                    UnresolvedEdgeCategory::ImportsFileNotFound,
                    "{extractor} {spec}"
                );
                assert_eq!(u.edge.metadata_json, e.metadata_json, "{extractor} {spec}");
            }
        }
    }

    #[test]
    fn include_with_malformed_metadata_declines_the_suffix_stage() {
        let index = c_include_index(&["src/event/ngx_event.c", "src/core/ngx_core.h"]);
        for raw in ["not json", "[1,2]", "\"isTypeOnly\"", "7"] {
            let mut e = c_include("e1", "src/event/ngx_event.c", "ngx_core.h");
            e.metadata_json = Some(raw.to_string());
            let result = resolve_edges(std::slice::from_ref(&e), &index, None);
            assert!(result.resolved.is_empty(), "{raw}");
            let u = &result.still_unresolved[0];
            assert_eq!(
                u.category,
                UnresolvedEdgeCategory::ImportsFileNotFound,
                "{raw}"
            );
            assert_eq!(
                u.edge.metadata_json.as_deref(),
                Some(raw),
                "never overwritten, never coerced"
            );
        }
    }

    // ── TS workspace member import stage (TS-WORKSPACE-RESOLUTION-1, RG-REQ-006-L04) ──

    const TS_EXTRACTOR: &str = "ts-core:0.2.0";

    /// The production resolver maps over `paths` (`c_include_index`) plus the npm workspace members
    /// the orchestrator builds from the declared-module catalog (each entry declared by `main`).
    fn ws_index(paths: &[&str], members: &[(&str, &str, &[&str])]) -> ResolverIndex {
        let mut index = c_include_index(paths);
        let declared: Vec<crate::types::DeclaredModule> = members
            .iter()
            .map(|(name, root, entries)| crate::types::DeclaredModule {
                ecosystem: "npm".to_string(),
                name: name.to_string(),
                canonical_root: root.to_string(),
                npm_declared_entries: entries
                    .iter()
                    .map(|e| crate::types::NpmDeclaredEntry::Main(e.to_string()))
                    .collect(),
            })
            .collect();
        index.npm_workspace_packages =
            crate::workspace_import::build_npm_workspace_packages(&declared);
        index
    }

    /// A bare TS import of `specifier` from the FILE node of `from`, with the carrier the extractor
    /// writes (`rawPath`) and the one the orchestrator injects (`isTypeOnly`).
    fn ts_import(uid: &str, from: &str, specifier: &str) -> ExtractedEdge {
        let mut e = make_edge(uid, specifier, EdgeType::Imports);
        e.source_node_uid = format!("n:{from}");
        e.extractor = TS_EXTRACTOR.into();
        e.metadata_json =
            Some(serde_json::json!({"rawPath": specifier, "isTypeOnly": false}).to_string());
        e
    }

    /// FRAKTAG's shape: `packages/api/src/server.ts` imports `@fraktag/engine`, whose manifest
    /// declares `main: dist/index.js` (not indexed) and whose `src/index.ts` is indexed.
    fn fraktag_index() -> ResolverIndex {
        ws_index(
            &[
                "packages/api/src/server.ts",
                "packages/engine/src/index.ts",
                "packages/engine/src/core.ts",
            ],
            &[(
                "@fraktag/engine",
                "packages/engine",
                &["packages/engine/dist/index.js"],
            )],
        )
    }

    #[test]
    fn workspace_package_import_resolves_inferred_to_its_source_entry_with_every_candidate() {
        let index = fraktag_index();
        let e = ts_import("e1", "packages/api/src/server.ts", "@fraktag/engine");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert!(
            result.still_unresolved.is_empty(),
            "{:?}",
            result.still_unresolved.len()
        );
        assert_eq!(result.resolved.len(), 1);
        let r = &result.resolved[0];
        assert_eq!(r.target_node_uid, "n:packages/engine/src/index.ts");
        assert_eq!(r.resolution, Resolution::Inferred, "never static");
        assert_eq!(r.extractor, TS_EXTRACTOR);
        assert_eq!(
            carrier(&r.metadata_json),
            serde_json::json!({
                "rawPath": "@fraktag/engine",
                "isTypeOnly": false,
                "basis": "workspace_source_entry",
                "candidates": [
                    "r1:packages/engine/src/index.ts:FILE",
                    "r1:packages/engine/dist/index.js:FILE"
                ],
            })
        );

        // Several declared targets are all recorded after the source entry, none selected.
        let index = ws_index(
            &[
                "packages/plugins/src/common/EffectControls.tsx",
                "packages/effects/src/index.ts",
            ],
            &[(
                "@amodx/effects",
                "packages/effects",
                &[
                    "packages/effects/dist/index.d.ts",
                    "packages/effects/dist/index.js",
                ],
            )],
        );
        let e = ts_import(
            "e2",
            "packages/plugins/src/common/EffectControls.tsx",
            "@amodx/effects",
        );
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].resolution, Resolution::Inferred);
        assert_eq!(
            carrier(&result.resolved[0].metadata_json)["candidates"],
            serde_json::json!([
                "r1:packages/effects/src/index.ts:FILE",
                "r1:packages/effects/dist/index.d.ts:FILE",
                "r1:packages/effects/dist/index.js:FILE"
            ])
        );
    }

    #[test]
    fn workspace_package_import_with_several_source_entries_stays_unresolved_with_the_pool() {
        let index = ws_index(
            &[
                "packages/api/src/server.ts",
                "packages/engine/src/index.ts",
                "packages/engine/src/index.js",
            ],
            &[(
                "@fraktag/engine",
                "packages/engine",
                &["packages/engine/dist/index.js"],
            )],
        );
        let e = ts_import("e1", "packages/api/src/server.ts", "@fraktag/engine");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert!(result.resolved.is_empty(), "never a pick");
        assert!(result.resolved_import_pairs.is_empty());
        assert_eq!(result.still_unresolved.len(), 1);
        let u = &result.still_unresolved[0];
        assert_eq!(
            u.category,
            categorize_unresolved_edge(&e),
            "the category is the one the row had"
        );
        assert_eq!(u.edge.target_key, "@fraktag/engine");
        assert_eq!(
            carrier(&u.edge.metadata_json),
            serde_json::json!({
                "rawPath": "@fraktag/engine",
                "isTypeOnly": false,
                "basis": "ambiguous_workspace_source_entry",
                "candidates": [
                    "r1:packages/engine/src/index.js:FILE",
                    "r1:packages/engine/src/index.ts:FILE",
                    "r1:packages/engine/dist/index.js:FILE"
                ],
            })
        );
    }

    #[test]
    fn workspace_stage_runs_only_after_every_earlier_stage_misses() {
        // A member named like a directory whose `index.ts` the repo-prefix stage already finds.
        let index = ws_index(
            &["app/main.ts", "lib/index.ts", "lib/src/index.ts"],
            &[("lib", "lib", &["lib/dist/index.js"])],
        );
        let e = ts_import("e1", "app/main.ts", "lib");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert_eq!(result.resolved.len(), 1);
        let r = &result.resolved[0];
        assert_eq!(r.target_node_uid, "n:lib/index.ts", "the earlier binding");
        assert_eq!(r.resolution, Resolution::Static);
        assert_eq!(r.metadata_json, e.metadata_json, "the carrier is untouched");
        assert_eq!(result.resolved_import_pairs.len(), 1);
    }

    #[test]
    fn non_ts_import_never_reaches_the_workspace_stage() {
        let index = fraktag_index();
        for extractor in [
            "python-core:0.2.0",
            "java-core:0.1.0",
            "rust-core:0.1.0",
            "c-core:0.1.0",
            "cpp-core:0.2.0",
            "test:1",
        ] {
            let mut e = ts_import("e1", "packages/api/src/server.ts", "@fraktag/engine");
            e.extractor = extractor.into();
            let result = resolve_edges(std::slice::from_ref(&e), &index, None);
            assert!(result.resolved.is_empty(), "{extractor}");
            assert_eq!(result.still_unresolved.len(), 1, "{extractor}");
            let u = &result.still_unresolved[0];
            assert_eq!(
                u.category,
                UnresolvedEdgeCategory::ImportsFileNotFound,
                "{extractor}"
            );
            assert_eq!(u.edge.metadata_json, e.metadata_json, "{extractor}");
        }
    }

    #[test]
    fn workspace_package_import_with_malformed_or_mismatched_carrier_declines() {
        let index = fraktag_index();
        for raw in [
            None,
            Some("not json"),
            Some("[1,2]"),
            Some("7"),
            Some(r#"{"isTypeOnly":false}"#),
            Some(r#"{"rawPath":"@fraktag/other","isTypeOnly":false}"#),
            Some(r#"{"rawPath":7}"#),
        ] {
            let mut e = ts_import("e1", "packages/api/src/server.ts", "@fraktag/engine");
            e.metadata_json = raw.map(str::to_string);
            let result = resolve_edges(std::slice::from_ref(&e), &index, None);
            assert!(result.resolved.is_empty(), "{raw:?}");
            assert_eq!(result.still_unresolved.len(), 1, "{raw:?}");
            let u = &result.still_unresolved[0];
            assert_eq!(u.category, categorize_unresolved_edge(&e), "{raw:?}");
            assert_eq!(
                u.edge.metadata_json.as_deref(),
                raw,
                "never overwritten, never coerced"
            );
        }
    }

    #[test]
    fn workspace_inferred_import_never_feeds_the_module_edge_pairs() {
        let index = ws_index(
            &[
                "packages/api/src/server.ts",
                "packages/api/src/routes.ts",
                "packages/engine/src/index.ts",
            ],
            &[(
                "@fraktag/engine",
                "packages/engine",
                &["packages/engine/dist/index.js"],
            )],
        );
        let inferred = ts_import("e1", "packages/api/src/server.ts", "@fraktag/engine");
        let mut certain = make_edge(
            "e2",
            "r1:packages/api/src/routes.ts:FILE",
            EdgeType::Imports,
        );
        certain.source_node_uid = "n:packages/api/src/server.ts".into();
        certain.extractor = TS_EXTRACTOR.into();
        let result = resolve_edges(&[inferred, certain], &index, None);
        assert_eq!(result.resolved.len(), 2);
        let by_uid = |uid: &str| result.resolved.iter().find(|r| r.edge_uid == uid).unwrap();
        assert_eq!(by_uid("e1").resolution, Resolution::Inferred);
        assert_eq!(by_uid("e2").resolution, Resolution::Static);
        // Only the certain import reaches the module-edge derivation.
        assert_eq!(result.resolved_import_pairs.len(), 1);
        assert_eq!(
            result.resolved_import_pairs[0].1,
            "n:packages/api/src/routes.ts"
        );
    }

    // ── TS tsconfig `paths` stage (TS-ALIAS-RESOLUTION-1, RG-REQ-006-L04) ──

    /// One file's stored `soleInspectedCoveringProjectMapping`: (file, anchor dir, baseUrl, paths).
    type MappingSeed<'a> = (&'a str, &'a str, &'a str, &'a [(&'a str, &'a [&'a str])]);

    /// `index` with the `paths` lookup the orchestrator builds from the stored alias signals: each
    /// seed becomes the JSON repo-index writes for that file (`entries` empty, the mapping set).
    fn with_mappings(mut index: ResolverIndex, seeds: &[MappingSeed]) -> ResolverIndex {
        use repo_graph_classification::types::{
            StoredTsconfigAliases, TsconfigAliasEntry, TsconfigAliasMapping,
        };
        let rows: Vec<(String, String)> = seeds
            .iter()
            .map(|(file, anchor, base, paths)| {
                let stored = StoredTsconfigAliases {
                    entries: vec![],
                    sole_inspected_covering_project_mapping: Some(TsconfigAliasMapping {
                        anchor_dir: anchor.to_string(),
                        base_url: base.to_string(),
                        entries: paths
                            .iter()
                            .map(|(pattern, subs)| TsconfigAliasEntry {
                                pattern: pattern.to_string(),
                                substitutions: subs.iter().map(|x| x.to_string()).collect(),
                            })
                            .collect(),
                    }),
                };
                (
                    format!("r1:{file}"),
                    serde_json::to_string(&stored).unwrap(),
                )
            })
            .collect();
        let keys: Vec<String> = index
            .nodes_by_stable_key
            .keys()
            .filter(|k| k.ends_with(":FILE"))
            .cloned()
            .collect();
        index.tsconfig_paths = TsconfigPathsIndex::build(
            rows.iter().map(|(f, j)| (f.as_str(), Some(j.as_str()))),
            keys,
        );
        index
    }

    /// The amodx `admin` shape: Toolbar.tsx's one mapping `@/*` → `./src/*`, anchored at `admin`.
    fn toolbar_index(extra: &[&str]) -> ResolverIndex {
        let mut paths = vec!["admin/src/components/editor/Toolbar.tsx"];
        paths.extend_from_slice(extra);
        with_mappings(
            c_include_index(&paths),
            &[(
                "admin/src/components/editor/Toolbar.tsx",
                "admin",
                ".",
                &[("@/*", &["./src/*"])],
            )],
        )
    }

    const TOOLBAR: &str = "admin/src/components/editor/Toolbar.tsx";

    #[test]
    fn tsconfig_paths_import_resolves_static_to_the_one_indexed_candidate() {
        let index = toolbar_index(&["admin/src/components/ui/button.tsx"]);
        let e = ts_import("e1", TOOLBAR, "@/components/ui/button");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert!(result.still_unresolved.is_empty());
        assert_eq!(result.resolved.len(), 1);
        let r = &result.resolved[0];
        assert_eq!(r.target_node_uid, "n:admin/src/components/ui/button.tsx");
        assert_eq!(r.resolution, Resolution::Static);
        // A certain import: it feeds the module-edge derivation.
        assert_eq!(result.resolved_import_pairs.len(), 1);
        assert_eq!(
            result.resolved_import_pairs[0].1,
            "n:admin/src/components/ui/button.tsx"
        );
    }

    #[test]
    fn tsconfig_paths_import_with_several_indexed_candidates_stays_unresolved_with_every_candidate()
    {
        let index = with_mappings(
            c_include_index(&["src/main.ts", "lib/x.ts", "lib/x/index.ts"]),
            &[("src/main.ts", "", ".", &[("@/*", &["./lib/*"])])],
        );
        let e = ts_import("e1", "src/main.ts", "@/x");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert!(result.resolved.is_empty(), "never a pick");
        assert!(result.resolved_import_pairs.is_empty());
        assert_eq!(result.still_unresolved.len(), 1);
        let u = &result.still_unresolved[0];
        assert_eq!(u.category, UnresolvedEdgeCategory::ImportsFileNotFound);
        assert_eq!(
            u.category,
            categorize_unresolved_edge(&e),
            "the row's own category"
        );
        assert_eq!(
            carrier(&u.edge.metadata_json),
            serde_json::json!({
                "rawPath": "@/x",
                "isTypeOnly": false,
                "basis": "ambiguous_tsconfig_paths",
                "candidates": ["r1:lib/x.ts:FILE", "r1:lib/x/index.ts:FILE"],
            }),
            "both FILE keys recorded, no other key added"
        );
    }

    #[test]
    fn tsconfig_paths_import_without_an_indexed_candidate_stays_unresolved_as_a_project_alias() {
        let index = toolbar_index(&["admin/src/other.ts"]);
        let e = ts_import("e1", TOOLBAR, "@/components/ui/missing");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert!(result.resolved.is_empty());
        assert_eq!(result.still_unresolved.len(), 1);
        let u = &result.still_unresolved[0];
        assert_eq!(u.category, UnresolvedEdgeCategory::ImportsFileNotFound);
        assert_eq!(
            u.edge.metadata_json, e.metadata_json,
            "the row is exactly HEAD's: no basis, no candidates"
        );
    }

    #[test]
    fn tsconfig_paths_stage_runs_after_every_head_stage_misses_and_before_the_workspace_stage() {
        // A repo-prefix binding (HEAD's stage) is untouched by a `paths` pattern that would reach
        // another indexed file.
        let index = with_mappings(
            ws_index(
                &["app/main.ts", "lib/index.ts", "alt/lib.ts"],
                &[("lib", "lib", &["lib/dist/index.js"])],
            ),
            &[("app/main.ts", "", ".", &[("lib", &["./alt/lib"])])],
        );
        let e = ts_import("e1", "app/main.ts", "lib");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert_eq!(result.resolved.len(), 1);
        let r = &result.resolved[0];
        assert_eq!(r.target_node_uid, "n:lib/index.ts", "the earlier binding");
        assert_eq!(r.resolution, Resolution::Static);
        assert_eq!(r.metadata_json, e.metadata_json, "the carrier is untouched");
        // A `paths` match binds before the workspace stage is consulted.
        let index = with_mappings(
            ws_index(
                &[
                    "app/main.ts",
                    "packages/w/src/index.ts",
                    "vendor/w/index.ts",
                ],
                &[("@w/pkg", "packages/w", &["packages/w/dist/index.js"])],
            ),
            &[("app/main.ts", "", ".", &[("@w/*", &["./vendor/w/*"])])],
        );
        let e = ts_import("e2", "app/main.ts", "@w/pkg");
        // `@w/pkg` → `vendor/w/pkg` reaches nothing; the workspace stage binds as at HEAD.
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].resolution, Resolution::Inferred);
        let e = ts_import("e3", "app/main.ts", "@w/index");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].target_node_uid, "n:vendor/w/index.ts");
        assert_eq!(result.resolved[0].resolution, Resolution::Static);
    }

    #[test]
    fn tsconfig_paths_static_edge_carries_its_basis_and_the_target_as_candidate() {
        let index = toolbar_index(&["admin/src/lib/utils.ts"]);
        let e = ts_import("e1", TOOLBAR, "@/lib/utils");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert_eq!(result.resolved.len(), 1);
        let r = &result.resolved[0];
        assert_eq!(r.resolution, Resolution::Static);
        assert_eq!(r.extractor, TS_EXTRACTOR);
        assert_eq!(
            carrier(&r.metadata_json),
            serde_json::json!({
                "rawPath": "@/lib/utils",
                "isTypeOnly": false,
                "basis": "tsconfig_paths",
                "candidates": ["r1:admin/src/lib/utils.ts:FILE"],
            })
        );
    }

    #[test]
    fn tsconfig_paths_match_wins_over_the_workspace_stage_for_the_same_specifier() {
        let index = with_mappings(
            ws_index(
                &[
                    "packages/api/src/server.ts",
                    "packages/engine/src/index.ts",
                    "packages/other/src/index.ts",
                ],
                &[
                    (
                        "@fraktag/engine",
                        "packages/engine",
                        &["packages/engine/dist/index.js"],
                    ),
                    (
                        "@x/other",
                        "packages/other",
                        &["packages/other/dist/index.js"],
                    ),
                ],
            ),
            &[(
                "packages/api/src/server.ts",
                "",
                ".",
                &[("@fraktag/*", &["./packages/*/src"])],
            )],
        );
        let e = ts_import("e1", "packages/api/src/server.ts", "@fraktag/engine");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert_eq!(result.resolved.len(), 1);
        let r = &result.resolved[0];
        assert_eq!(r.target_node_uid, "n:packages/engine/src/index.ts");
        assert_eq!(r.resolution, Resolution::Static, "the compiler's answer");
        assert_eq!(carrier(&r.metadata_json)["basis"], "tsconfig_paths");
        // A specifier matching no pattern still reaches the workspace stage, inferred as at HEAD.
        let e = ts_import("e2", "packages/api/src/server.ts", "@x/other");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].resolution, Resolution::Inferred);
        assert_eq!(
            carrier(&result.resolved[0].metadata_json)["basis"],
            "workspace_source_entry"
        );
    }

    #[test]
    fn tsconfig_paths_match_with_no_indexed_hit_falls_through_to_the_workspace_stage() {
        let index = with_mappings(
            fraktag_index(),
            &[(
                "packages/api/src/server.ts",
                "",
                ".",
                &[("@fraktag/*", &["./nowhere/*"])],
            )],
        );
        let e = ts_import("e1", "packages/api/src/server.ts", "@fraktag/engine");
        let with_paths = resolve_edges(std::slice::from_ref(&e), &index, None);
        let head = resolve_edges(std::slice::from_ref(&e), &fraktag_index(), None);
        assert_eq!(with_paths.resolved, head.resolved, "exactly HEAD's binding");
        assert_eq!(with_paths.resolved.len(), 1);
        assert_eq!(with_paths.resolved[0].resolution, Resolution::Inferred);
        assert_eq!(
            carrier(&with_paths.resolved[0].metadata_json)["basis"],
            "workspace_source_entry"
        );
    }

    #[test]
    fn non_ts_import_never_reaches_the_tsconfig_paths_stage() {
        let index = toolbar_index(&["admin/src/components/ui/button.tsx"]);
        for extractor in [
            "python-core:0.2.0",
            "java-core:0.1.0",
            "rust-core:0.1.0",
            "c-core:0.1.0",
            "cpp-core:0.2.0",
            "test:1",
        ] {
            let mut e = ts_import("e1", TOOLBAR, "@/components/ui/button");
            e.extractor = extractor.into();
            let result = resolve_edges(std::slice::from_ref(&e), &index, None);
            assert!(result.resolved.is_empty(), "{extractor}");
            assert_eq!(result.still_unresolved.len(), 1, "{extractor}");
            assert_eq!(
                result.still_unresolved[0].edge.metadata_json, e.metadata_json,
                "{extractor}"
            );
        }
    }

    #[test]
    fn relative_specifier_never_reaches_the_tsconfig_paths_stage() {
        let index = with_mappings(
            c_include_index(&["app/main.ts", "src/x.ts", "src/index.ts", "src/y.ts"]),
            &[("app/main.ts", "", ".", &[("*", &["./src/*"])])],
        );
        for specifier in [".", "..", "./x", "../x", "/x"] {
            let e = ts_import("e1", "app/main.ts", specifier);
            let result = resolve_edges(std::slice::from_ref(&e), &index, None);
            assert!(
                !result.resolved.iter().any(|r| r
                    .metadata_json
                    .as_deref()
                    .is_some_and(|m| m.contains("tsconfig_paths"))),
                "{specifier}: never bound through paths"
            );
            assert!(
                !result.still_unresolved.iter().any(|u| u
                    .edge
                    .metadata_json
                    .as_deref()
                    .is_some_and(|m| m.contains("tsconfig_paths"))),
                "{specifier}"
            );
        }
        // The non-relative control (no earlier stage binds it) binds through the same `*` pattern.
        let e = ts_import("e2", "app/main.ts", "y");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].target_node_uid, "n:src/y.ts");
        assert_eq!(
            carrier(&result.resolved[0].metadata_json)["basis"],
            "tsconfig_paths"
        );
    }

    #[test]
    fn tsconfig_paths_stage_is_a_no_op_for_a_file_whose_aliases_carry_no_mapping() {
        let mut index = c_include_index(&[TOOLBAR, "admin/src/components/ui/button.tsx"]);
        let keys: Vec<String> = index.nodes_by_stable_key.keys().cloned().collect();
        let head_row = r#"{"entries":[{"pattern":"@/*","substitutions":["./src/*"]}]}"#;
        let malformed = r#"{"entries":[],"soleInspectedCoveringProjectMapping":{"anchorDir":7,"baseUrl":".","entries":[]}}"#;
        let file_uid = format!("r1:{TOOLBAR}");
        for row in [Some(head_row), Some(malformed), Some("not json"), None] {
            index.tsconfig_paths =
                TsconfigPathsIndex::build([(file_uid.as_str(), row)], keys.clone());
            assert!(index.tsconfig_paths.is_empty(), "{row:?}");
            let e = ts_import("e1", TOOLBAR, "@/components/ui/button");
            let result = resolve_edges(std::slice::from_ref(&e), &index, None);
            assert!(result.resolved.is_empty(), "{row:?}");
            assert_eq!(
                result.still_unresolved[0].edge.metadata_json, e.metadata_json,
                "{row:?}: the row stays HEAD's"
            );
        }
        // Another file's mapping never applies to a file that carries none.
        let index = with_mappings(
            c_include_index(&[
                TOOLBAR,
                "admin/src/other.ts",
                "admin/src/components/ui/button.tsx",
            ]),
            &[("admin/src/other.ts", "admin", ".", &[("@/*", &["./src/*"])])],
        );
        let e = ts_import("e1", TOOLBAR, "@/components/ui/button");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert!(result.resolved.is_empty());
    }

    #[test]
    fn tsconfig_paths_stage_honours_base_url_relative_to_the_anchor_directory() {
        let index = with_mappings(
            c_include_index(&["web/app/main.ts", "web/src/a.ts", "src/a.ts"]),
            &[("web/app/main.ts", "web", "src", &[("@/*", &["./*"])])],
        );
        let e = ts_import("e1", "web/app/main.ts", "@/a");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(
            result.resolved[0].target_node_uid, "n:web/src/a.ts",
            "baseUrl `src` joined under the anchor `web`, never the repository root"
        );
    }

    #[test]
    fn tied_tsconfig_paths_patterns_stay_unresolved_with_every_candidate() {
        let tied: &[(&str, &[&str])] = &[("@/*", &["./a/*"]), ("@/*x", &["./b/*x"])];
        // Both tied patterns reach a file.
        let index = with_mappings(
            c_include_index(&["src/main.ts", "a/x.ts", "b/x.ts"]),
            &[("src/main.ts", "", ".", tied)],
        );
        let e = ts_import("e1", "src/main.ts", "@/x");
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert!(result.resolved.is_empty(), "never a pick");
        let u = &result.still_unresolved[0];
        assert_eq!(u.category, categorize_unresolved_edge(&e));
        assert_eq!(
            carrier(&u.edge.metadata_json),
            serde_json::json!({
                "rawPath": "@/x",
                "isTypeOnly": false,
                "basis": "tied_tsconfig_paths_patterns",
                "candidates": ["r1:a/x.ts:FILE", "r1:b/x.ts:FILE"],
            })
        );
        // One tied pattern reaching one file, the other none: the same row with that candidate.
        let index = with_mappings(
            c_include_index(&["src/main.ts", "b/x.ts"]),
            &[("src/main.ts", "", ".", tied)],
        );
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert!(result.resolved.is_empty());
        let c = carrier(&result.still_unresolved[0].edge.metadata_json);
        assert_eq!(c["basis"], "tied_tsconfig_paths_patterns");
        assert_eq!(c["candidates"], serde_json::json!(["r1:b/x.ts:FILE"]));
        // Neither reaching a file: no named ambiguity — the row stays as at HEAD.
        let index = with_mappings(
            c_include_index(&["src/main.ts", "c/x.ts"]),
            &[("src/main.ts", "", ".", tied)],
        );
        let result = resolve_edges(std::slice::from_ref(&e), &index, None);
        assert!(result.resolved.is_empty());
        assert_eq!(
            result.still_unresolved[0].edge.metadata_json,
            e.metadata_json
        );
    }
}
