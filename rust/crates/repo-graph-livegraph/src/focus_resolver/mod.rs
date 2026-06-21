//! FOCUS-RESOLUTION-LIVEGRAPH-IMPL: a LiveGraph-native focus-resolution producer.
//!
//! Resolves a FOCUS STRING (path / stable-key / symbol-name / symbol-context) to an IR
//! symbol/file/module identity from CURRENT-STATE LiveGraph (the resident IR + the FILE
//! inventory) INSTEAD of the SQLite `nodes` reads the four `resolve_*` functions perform today
//! (`storage::agent_impl`). It mirrors those four SQLite functions 1:1 so a no-loss certificate
//! (the daemon `focus_resolution_cert`) can prove the LiveGraph resolution is FIELD-EQUAL to the
//! SQLite resolution for the same focus string.
//!
//! Per the ratified spec `docs/slices/focus-resolution-livegraph-1.md`:
//! - VERDICT `BUILDABLE-FROM-EXISTING-IR`: every datum the four functions return is already in
//!   the IR or is a deterministic function of IR data — NO IR field is added.
//! - DR-FR-CRATE-HOME -> A: the resolver lives HERE (beside `module_stats`/`callers`/
//!   `node_display`), reading only [`repo_graph_ir`] — NO new dependency edge. It is a child
//!   module of the crate root, so it accesses the root's `LiveGraph` internals (`slots`, the
//!   `whole_graph_completeness` fold, `capture_envelope`) directly while keeping the 4000-line
//!   `lib.rs` from growing a new responsibility (the 500-line structural guardrail). The native
//!   result types live in [`types`]; the unit tests in `tests` (review-1 pt5 split).
//! - DR-FR-QNAME-SOURCE -> A: `qualified_name` is PARSED from the `#…:SYMBOL:` segment of the
//!   [`repo_graph_ir::CanonicalKey`] — no IR change. The cert guards the fallback-node edge case.
//! - module-node identity: the DERIVED ancestor-walk model — the SQLite directory-MODULE set is a
//!   pure function of the file-path inventory (`indexer::orchestrator` walks every ancestor dir of
//!   every file; key = `{repo}:{dir}:MODULE`). This crate reproduces it byte-exact over the
//!   resident FILE inventory.
//!
//! **Trust:** every method returns an [`AnswerEnvelope`] built by the SAME `whole_graph_completeness`
//! fold + `capture_envelope` the cycle/stats reads use — all resident + Fresh + TS -> `Exact`; a
//! non-resident / non-TS partition -> `Partial`; a stale partition -> `Stale`. The "null = unknown,
//! never empty" safety rule (architecture.md) is encoded by the class: an empty result under a
//! non-`Exact` envelope is UNKNOWN (the consumer must fall back to SQLite), never a confident miss.
//! Only an `Exact` envelope licenses treating an empty/None result as a true no-match.

mod types;
pub use types::{FocusCandidate, FocusCorpus, FocusKind, PathResolutionAnswer, SymbolContext};

#[cfg(test)]
mod tests;

use repo_graph_ir::{IdentitySource, IrNode};
use repo_graph_trust_model::AnswerEnvelope;

use repo_graph_import_resolver::{dirname, file_key_path, FileInventory};

use crate::{capture_envelope, LiveGraph};

// ── Key-parse helpers (pure; the canonical-key namespace is shared with SQLite stable keys) ─────
//
// The key namespace is `{repo}:{path}#{qname}:SYMBOL:{subtype}[:dupN]` for symbols,
// `{repo}:{path}:FILE` for files, `{repo}:{dir}:MODULE` for directory modules. `repo_uid` is
// `repo_<ulid>` and carries NO colon, so the FIRST `:` is always the repo/rest boundary; any colon
// WITHIN a path is preserved. These helpers reuse that proven invariant rather than re-slicing.

/// The repo-relative path of a SYMBOL key: the segment between the first `:` (repo boundary) and the
/// first `#` (name boundary). `None` if the key has no `#` (not a symbol key). A path never contains
/// `#`, so the first `#` is unambiguous.
fn symbol_key_path(key: &str) -> Option<&str> {
    let after_repo = key.split_once(':')?.1;
    Some(after_repo.split_once('#')?.0)
}

/// The qualified_name of a SYMBOL key: the segment between the first `#` and the `:SYMBOL:` marker
/// (DR-FR-QNAME -> A). For a method the extractor passes `qualified_name` (`Parent.method`) as the
/// key's name segment; for a top-level symbol it equals the name. `None` if the key carries no
/// `:SYMBOL:` marker (a FILE/MODULE/fallback key the parse does not apply to — the cert forces RED).
fn symbol_key_qualified_name(key: &str) -> Option<String> {
    let after_hash = key.split_once('#')?.1;
    Some(after_hash.split_once(":SYMBOL:")?.0.to_string())
}

/// The directory of a MODULE key `{repo}:{dir}:MODULE`: the segment between the first `:` and the
/// `:MODULE` suffix. `None` if the key is not a `:MODULE` key.
fn module_key_dir(key: &str) -> Option<&str> {
    let inner = key.strip_suffix(":MODULE")?;
    Some(inner.split_once(':')?.1)
}

/// The `repo_uid` prefix of any canonical key: everything before the first `:`.
fn key_repo_prefix(key: &str) -> Option<&str> {
    key.split_once(':').map(|(repo, _)| repo)
}

/// The directory MODULE key for a repo prefix + directory: `{repo}:{dir}:MODULE` — byte-identical to
/// the SQLite directory-MODULE materializer's `stable_key` (`indexer::orchestrator`).
fn module_key_for(repo: &str, dir: &str) -> String {
    format!("{repo}:{dir}:MODULE")
}

/// The display subtype for a symbol node — `symbol_kind` (the granular AST kind that populates the
/// SQLite `subtype` column for a SYMBOL) when present, else the coarse SCIP descriptor `IrNode::subtype`.
/// Mirrors `LiveGraph::node_display` exactly (the proven cert-gated identity precedent). For an
/// AST-adopted node `symbol_kind` is always present, so the fallback branch never runs on a GREEN path;
/// on a fallback node the coarse descriptor diverges from SQLite and the cert catches it (spec §7c L2).
fn symbol_subtype(node: &IrNode) -> String {
    node.attributes
        .as_ref()
        .and_then(|a| a.symbol_kind.clone())
        .unwrap_or_else(|| node.subtype.clone())
}

impl LiveGraph {
    /// Iterate the nodes of every RESIDENT partition (the IR is dropped on unload; a non-resident
    /// slot retains only its xref summary, never a node-level answer — the residency rule).
    fn resident_nodes(&self) -> impl Iterator<Item = &IrNode> {
        self.slots
            .values()
            .filter_map(|s| s.ir.as_ref())
            .flat_map(|ir| ir.nodes.iter())
    }

    /// The repo-relative FILE inventory of the resident set (path -> FILE key), the SAME structure
    /// `rebuild_xpart_overlay` builds. Built from resident FILE-scope (`AstFileScope`) node keys.
    fn resident_file_inventory(&self) -> FileInventory {
        FileInventory::from_file_keys(
            self.resident_nodes()
                .filter(|n| n.identity_source == IdentitySource::AstFileScope)
                .map(|n| n.key.as_str().to_string()),
        )
    }

    /// Every resident FILE-scope repo-relative path, sorted + deduped.
    fn resident_file_paths(&self) -> Vec<String> {
        let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for n in self.resident_nodes() {
            if n.identity_source == IdentitySource::AstFileScope {
                if let Some(p) = file_key_path(n.key.as_str()) {
                    set.insert(p.to_string());
                }
            }
        }
        set.into_iter().collect()
    }

    /// The directory MODULE set: every ancestor directory of every resident FILE path, reproducing
    /// the SQLite directory-MODULE materializer's ancestor walk (`indexer::orchestrator`: for each
    /// file, repeatedly strip the last `/` and collect the directory). A repo-root file (no `/`)
    /// yields no directory. The result is the set of `dir` values for which a `{repo}:{dir}:MODULE`
    /// node exists in SQLite.
    fn module_dir_set(&self) -> std::collections::BTreeSet<String> {
        let mut dirs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for path in self.resident_file_paths() {
            let mut p = path.as_str();
            while let Some(pos) = p.rfind('/') {
                let dir = &p[..pos];
                if !dirs.insert(dir.to_string()) {
                    break; // already seen this directory and all of its parents
                }
                p = dir;
            }
        }
        dirs
    }

    /// The `repo_uid` prefix shared by all resident keys (extracted from any resident FILE-scope
    /// key). `None` when no file is resident. Deterministic: every key carries the SAME prefix, so
    /// the value is independent of iteration order.
    fn resident_repo_prefix(&self) -> Option<String> {
        self.resident_nodes()
            .find(|n| n.identity_source == IdentitySource::AstFileScope)
            .and_then(|n| key_repo_prefix(n.key.as_str()))
            .map(str::to_string)
    }

    /// Find a resident node by exact canonical-key string (scans resident IRs).
    fn resident_node_by_key(&self, key: &str) -> Option<&IrNode> {
        self.resident_nodes().find(|n| n.key.as_str() == key)
    }

    /// Build the trust envelope shared by every resolver method: the SAME whole-graph completeness
    /// fold + `capture_envelope` the cycle/stats reads use. All resident + Fresh + TS -> `Exact`; a
    /// non-resident/non-TS partition -> `Partial`; else (resident, non-Fresh) -> `Stale`. This is
    /// what encodes "null = unknown, never empty": only an `Exact` envelope licenses treating an
    /// empty/None result as a confident miss.
    fn focus_envelope<T>(&self, data: T) -> AnswerEnvelope<T> {
        let (missing, worst, languages, _epochs) = self.whole_graph_completeness();
        capture_envelope(data, missing, worst, languages)
    }

    /// `resolve_path(path)` — mirrors SQLite `resolve_path_focus` (`agent_impl.rs`). Reads ONLY the
    /// resident FILE inventory + the derived module set; NO `nodes` read.
    ///
    /// - `has_exact_file` / `file_key`: a FILE node exists at exactly `path` (inventory lookup).
    /// - `has_content_under_prefix`: some resident FILE path starts with `{path}/`.
    /// - `module_key`: `Some({repo}:{path}:MODULE)` iff `path` is an ancestor directory of a
    ///   resident FILE (the derived directory-MODULE model).
    pub fn resolve_path(&self, path: &str) -> AnswerEnvelope<PathResolutionAnswer> {
        let inventory = self.resident_file_inventory();
        let file_key = inventory.file_key_for(path).map(str::to_string);
        let has_exact_file = file_key.is_some();
        let prefix = format!("{path}/");
        let has_content_under_prefix = self
            .resident_file_paths()
            .iter()
            .any(|p| p.starts_with(&prefix));
        let module_key = if self.module_dir_set().contains(path) {
            self.resident_repo_prefix()
                .map(|repo| module_key_for(&repo, path))
        } else {
            None
        };
        let data = PathResolutionAnswer {
            has_exact_file,
            file_key,
            has_content_under_prefix,
            module_key,
        };
        self.focus_envelope(data)
    }

    /// `resolve_stable_key(key)` — mirrors SQLite `resolve_stable_key_focus`. A canonical key's
    /// suffix is unambiguous, so it is matched per kind:
    /// - `:FILE` -> a resident `AstFileScope` node with that key (kind `File`, `file = path`).
    /// - `:MODULE` -> `module_key_dir` names an ancestor directory of a resident FILE AND the key's
    ///   repo prefix equals the resident repo prefix (kind `Module`, `file = None`, matching SQLite's
    ///   null `file_uid` for MODULE nodes).
    /// - otherwise -> a resident `AstAdopted` node with that key (kind `Symbol`, `file =` the key's
    ///   path segment). A `ScipSynthesizedFallback` node is NOT matched (spec §6a); the cert catches
    ///   the resulting divergence -> RED (§7c L2).
    ///
    /// `None` when nothing matches — byte-identical to SQLite's no-row case.
    pub fn resolve_stable_key(&self, key: &str) -> AnswerEnvelope<Option<FocusCandidate>> {
        let candidate = if key.ends_with(":FILE") {
            self.resident_node_by_key(key)
                .filter(|n| n.identity_source == IdentitySource::AstFileScope)
                .map(|_| FocusCandidate {
                    key: key.to_string(),
                    kind: FocusKind::File,
                    file: file_key_path(key).map(str::to_string),
                })
        } else if let Some(dir) = module_key_dir(key) {
            // Parity: SQLite `resolve_stable_key_focus` matches `stable_key` EXACTLY, so a derived
            // directory-MODULE match must also bind the FULL key — including the repo prefix. A
            // foreign-repo module key (`other_repo:src:MODULE`) MUST miss even when `src` is a
            // resident directory of THIS repo (review-1 pt2: the dir-only check ignored the prefix).
            let repo_matches = key_repo_prefix(key) == self.resident_repo_prefix().as_deref();
            if repo_matches && self.module_dir_set().contains(dir) {
                Some(FocusCandidate {
                    key: key.to_string(),
                    kind: FocusKind::Module,
                    file: None,
                })
            } else {
                None
            }
        } else {
            self.resident_node_by_key(key)
                .filter(|n| n.identity_source == IdentitySource::AstAdopted)
                .map(|_| FocusCandidate {
                    key: key.to_string(),
                    kind: FocusKind::Symbol,
                    file: symbol_key_path(key).map(str::to_string),
                })
        };
        self.focus_envelope(candidate)
    }

    /// `resolve_symbol_name(name)` — mirrors SQLite `resolve_symbol_name`. Resident `AstAdopted`
    /// nodes whose `name == name`, sorted by canonical key ascending, first 5 (the SQLite
    /// `ORDER BY stable_key ASC LIMIT 5`). All `kind = Symbol`; `file =` each key's path segment.
    /// SQLite matches on `name` ONLY and surfaces same-name ambiguity as up-to-5 candidates — this
    /// reproduces that exactly (no `qualified_name` disambiguation on either side).
    pub fn resolve_symbol_name(&self, name: &str) -> AnswerEnvelope<Vec<FocusCandidate>> {
        let mut keys: Vec<&str> = self
            .resident_nodes()
            .filter(|n| n.identity_source == IdentitySource::AstAdopted && n.name == name)
            .map(|n| n.key.as_str())
            .collect();
        keys.sort_unstable();
        let candidates: Vec<FocusCandidate> = keys
            .into_iter()
            .take(5)
            .map(|key| FocusCandidate {
                key: key.to_string(),
                kind: FocusKind::Symbol,
                file: symbol_key_path(key).map(str::to_string),
            })
            .collect();
        self.focus_envelope(candidates)
    }

    /// `symbol_context(key)` — mirrors SQLite `get_symbol_context`. For a resident `AstAdopted`
    /// node with that key:
    /// - `name` / `subtype` / `line_start` / `file_path`: `IrNode` fields (subtype via
    ///   [`symbol_subtype`], file via the key's path segment).
    /// - `qualified_name`: parsed from the key's `#…:SYMBOL:` segment (DR-FR-QNAME -> A).
    /// - `module_path` / `module_key`: `dirname(file)` and `{repo}:{dirname(file)}:MODULE` (the
    ///   derived directory-MODULE model; `None` for a repo-root file, matching SQLite's null
    ///   OWNS-edge join).
    ///
    /// `None` when no resident `AstAdopted` node has the key.
    pub fn symbol_context(&self, key: &str) -> AnswerEnvelope<Option<SymbolContext>> {
        let ctx = self
            .resident_node_by_key(key)
            .filter(|n| n.identity_source == IdentitySource::AstAdopted)
            .map(|node| {
                let file_path = symbol_key_path(key).map(str::to_string);
                let (module_path, module_key) = match (file_path.as_deref(), key_repo_prefix(key)) {
                    (Some(file), Some(repo)) => {
                        let dir = dirname(file);
                        if dir.is_empty() {
                            (None, None)
                        } else {
                            (Some(dir.to_string()), Some(module_key_for(repo, dir)))
                        }
                    }
                    _ => (None, None),
                };
                SymbolContext {
                    file_path,
                    module_path,
                    module_key,
                    name: node.name.clone(),
                    qualified_name: symbol_key_qualified_name(key),
                    subtype: Some(symbol_subtype(node)),
                    line_start: node.range.as_ref().map(|r| r.start_line as u64),
                }
            });
        self.focus_envelope(ctx)
    }

    /// The parity [`FocusCorpus`] enumerated from the resident snapshot (spec §7d). Pure read; no
    /// envelope (the daemon cert decides GREEN/RED, and uses the resolver's own per-focus envelope
    /// classes to gate completeness).
    pub fn focus_corpus(&self) -> FocusCorpus {
        let mut symbol_keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut symbol_names: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        for n in self.resident_nodes() {
            if n.identity_source == IdentitySource::AstAdopted {
                symbol_keys.insert(n.key.as_str().to_string());
                symbol_names.insert(n.name.clone());
            }
        }
        FocusCorpus {
            file_paths: self.resident_file_paths(),
            module_dirs: self.module_dir_set().into_iter().collect(),
            symbol_keys: symbol_keys.into_iter().collect(),
            symbol_names: symbol_names.into_iter().collect(),
        }
    }
}
