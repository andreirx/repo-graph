//! FOCUS-RESOLUTION-LIVEGRAPH-IMPL: the LiveGraph-NATIVE result types returned by the focus
//! resolver ([`super`]). Split out of the resolver `impl` per the 500-line structural guardrail
//! (review-1 pt5).
//!
//! These are NOT the agent crate's `AgentPathResolution` / `AgentFocusCandidate` /
//! `AgentSymbolContext` — `repo-graph-livegraph` must never depend on the agent crate. They carry the
//! SAME fields as the agent DTOs so the daemon cert can field-compare them against the SQLite
//! resolution without a dependency inversion; the native-result -> agent-DTO mapping is the LATER
//! COHERENCE-LEAF-SERVE consumer adapter, not this slice.

/// What kind of graph entity a focus candidate resolved to. Mirrors `storage_port::AgentFocusKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusKind {
    /// A FILE-scope node (`IdentitySource::AstFileScope`; SQLite `kind='FILE'`).
    File,
    /// A directory MODULE (the derived ancestor-walk identity; SQLite `kind='MODULE'`).
    Module,
    /// A SYMBOL node (`IdentitySource::AstAdopted`; SQLite `kind='SYMBOL'`).
    Symbol,
}

impl FocusKind {
    /// Stable string for diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            FocusKind::File => "File",
            FocusKind::Module => "Module",
            FocusKind::Symbol => "Symbol",
        }
    }
}

/// A candidate entity returned by stable-key / symbol-name focus resolution. Mirrors
/// `storage_port::AgentFocusCandidate` field-for-field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusCandidate {
    /// The canonical stable key that matched (an [`repo_graph_ir::CanonicalKey`] string).
    pub key: String,
    /// The matched node's kind.
    pub kind: FocusKind,
    /// The repo-relative file path the node is associated with, if any (`None` for a MODULE,
    /// matching SQLite's `file_uid IS NULL` for directory MODULE nodes).
    pub file: Option<String>,
}

/// Result of resolving a path-based focus string. Mirrors `storage_port::AgentPathResolution`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathResolutionAnswer {
    /// A FILE node exists at exactly `path`.
    pub has_exact_file: bool,
    /// When `has_exact_file`, the FILE node's canonical key (`{repo}:{path}:FILE`).
    pub file_key: Option<String>,
    /// Some FILE exists under `{path}/`.
    pub has_content_under_prefix: bool,
    /// A directory MODULE exists at exactly `path` (the derived ancestor-walk identity):
    /// `Some({repo}:{path}:MODULE)` iff `path` is an ancestor directory of a resident FILE.
    pub module_key: Option<String>,
}

/// Context for a resolved SYMBOL node. Mirrors `storage_port::AgentSymbolContext`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolContext {
    /// Owning file (repo-relative path).
    pub file_path: Option<String>,
    /// Owning module path (`dirname(file)`; the SQLite OWNS-edge MODULE's `qualified_name`).
    pub module_path: Option<String>,
    /// Owning module key (`{repo}:{dirname(file)}:MODULE`).
    pub module_key: Option<String>,
    /// Symbol name (`IrNode.name`).
    pub name: String,
    /// Qualified name, parsed from the `#…:SYMBOL:` segment of the key (DR-FR-QNAME -> A).
    pub qualified_name: Option<String>,
    /// Granular subtype (`SymbolAttributes::symbol_kind` when present — the AST kind, matching the
    /// SQLite `subtype` column — else the coarse SCIP descriptor `IrNode::subtype`).
    pub subtype: Option<String>,
    /// 1-based start line (`SourceRange::start_line`).
    pub line_start: Option<u64>,
}

/// The finite parity CORPUS enumerated from the resident snapshot (spec §7d). The daemon cert drives
/// BOTH the LiveGraph resolver and the SQLite resolver over this corpus (plus negative samples it
/// adds) and asserts field-equality. Exhaustive over the LiveGraph-resolvable identity set: every
/// resident FILE path, every ancestor-walk directory, every AST-adopted symbol key, and every
/// distinct AST-adopted symbol name. Each list is deterministically sorted.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FocusCorpus {
    /// Every resident FILE-scope repo-relative path (exact-file + content-prefix foci).
    pub file_paths: Vec<String>,
    /// Every directory in the ancestor walk over the resident files (module foci).
    pub module_dirs: Vec<String>,
    /// Every resident AST-adopted SYMBOL canonical key (stable-key + symbol-context foci).
    pub symbol_keys: Vec<String>,
    /// Every distinct resident AST-adopted SYMBOL name (symbol-name foci).
    pub symbol_names: Vec<String>,
}
