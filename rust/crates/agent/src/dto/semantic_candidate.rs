//! Semantic-candidate value types + constructors (EMBED-SEED-IMPL-1, spec §8.2/§8.2a).
//!
//! **Abstraction note (per repo structural guardrail):** extracted from `envelope.rs`
//! because the seed additions pushed that file past the 500-line guardrail (review-6 #2).
//! Holds the two self-labeling value types a semantic candidate carries ([`NextCommand`],
//! [`ModuleHint`]) and the [`FocusCandidate`] constructors. Two concrete current callers,
//! both via the re-export in [`super::envelope`]: the daemon fallback tier (`dispatch.rs`)
//! and the seed query path (`seed/query.rs`). Axis of variation: none claimed — a
//! cohesion/size split; the types stay re-exported from `envelope` so no call site moved.
//! `FocusCandidate`/`ResolvedKind` are shared from the parent via `super::envelope`.

use serde::Serialize;

use super::envelope::{FocusCandidate, ResolvedKind};

/// A structured, executable follow-up command carried by a semantic candidate
/// (spec §8.2). It uses the real CLI syntax (`explain <stable_key>`) with an
/// explicit `cwd` = the absolute repo root the seam resolved, because `explain`
/// and `orient` resolve the repo from the working directory, not an argument.
///
/// STANDING HONESTY RULE (operator ruling 2, 2026-08-25): `cwd` is the registry
/// entry's absolute canonical path. When that lookup is UNAVAILABLE, `cwd` is
/// **omitted** and `cwd_unavailable` carries the honest reason — never a fabricated
/// empty-string working directory. The two are mutually exclusive by construction
/// ([`NextCommand::new`]): a resolved root sets `cwd` alone; an unavailable lookup
/// sets `cwd_unavailable` alone. In the normal (resolved) case the JSON is
/// `{cmd, args, cwd}`, byte-identical to before this rule.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NextCommand {
    pub cmd: String,
    pub args: Vec<String>,
    /// The absolute canonical repo root; present iff the registry resolved it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// The honest reason `cwd` is absent; present iff `cwd` is `None`. Never a
    /// fabricated empty working directory (standing honesty rule).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd_unavailable: Option<String>,
}

impl NextCommand {
    /// Build a follow-up command, honestly recording whether its working directory
    /// is known. `root` is the registry's absolute canonical path (`Some`) or `None`
    /// when the registry lookup was unavailable; in the `None` case `cwd` is omitted
    /// and the honest reason is recorded — never a fabricated empty `cwd`.
    pub fn new(cmd: String, args: Vec<String>, root: Option<String>) -> Self {
        match root {
            Some(cwd) => Self {
                cmd,
                args,
                cwd: Some(cwd),
                cwd_unavailable: None,
            },
            None => Self {
                cmd,
                args,
                cwd: None,
                cwd_unavailable: Some(
                    "repository working directory unavailable (registry lookup failed)".to_string(),
                ),
            },
        }
    }
}

/// The owning-module hint carried on a semantic candidate (spec §8.2a; operator
/// ruling 2026-08-25). Two mutually-exclusive, self-labeling states — the genuine
/// owning module from `module_file_ownership`, OR an explicit unavailable-with-reason.
/// Modeled as a sum type (externally tagged) so "no ownership recorded" and "a real
/// module" can never be confused, and a directory guess can never masquerade as a
/// module. Externally-tagged JSON: `{"owning":"backend/services"}` /
/// `{"unavailable":"no module ownership recorded"}`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleHint {
    /// The genuine owning module's display path (`module_candidates.canonical_root_path`).
    Owning(String),
    /// No genuine owning module — carries the honest reason (e.g. "no module
    /// ownership recorded"). NEVER a fallback value under the name `module`.
    Unavailable(String),
}

impl FocusCandidate {
    /// A deterministic (ambiguity) candidate. `line` is the ANCHORS-EVERYWHERE-1
    /// `path:line` anchor, single-sourced with `file`; `None` keeps the JSON
    /// byte-identical to the pre-anchor shape. All semantic fields absent.
    pub fn deterministic(
        stable_key: String,
        file: Option<String>,
        line: Option<u64>,
        kind: ResolvedKind,
    ) -> Self {
        Self {
            stable_key,
            file,
            kind,
            line,
            source: None,
            model_id: None,
            score: None,
            module: None,
            next: None,
        }
    }

    /// A Layer-3 semantic fallback candidate (spec §8.2). `kind` is `File` by
    /// construction (the file-level store, D-ES-5); the semantic fields are all
    /// present and self-labeling (`source:"embedding"`).
    pub fn semantic(
        stable_key: String,
        path: String,
        module: ModuleHint,
        score: f64,
        model_id: String,
        next: NextCommand,
    ) -> Self {
        Self {
            stable_key,
            file: Some(path),
            kind: ResolvedKind::File,
            line: None,
            source: Some("embedding".to_string()),
            model_id: Some(model_id),
            score: Some(score),
            module: Some(module),
            next: Some(next),
        }
    }
}
