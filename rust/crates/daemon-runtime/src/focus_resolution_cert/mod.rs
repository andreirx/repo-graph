//! FOCUS-RESOLUTION-LIVEGRAPH-IMPL: the daemon-side NO-LOSS certificate for the LiveGraph focus
//! resolver (`repo_graph_livegraph::focus_resolver`).
//!
//! This is the cert BUILD/STORE + SERVE-LADDER wiring — the daemon half of the producer. It mirrors
//! the import/cycles/stats/complexity certs (a `{verdict, fingerprint}` struct on `RepoState`, keyed
//! by the SHARED SQLite-free fingerprint, rebuilt on a fingerprint mismatch, not durable). It lives
//! in its OWN module — exactly like the COMPLEXITY cert (`orient_lg_decisions::ComplexityNoLossCert`),
//! whose `build_and_store_complexity_cert` + cert-state ladder ALSO live outside `livegraph_feed.rs`
//! — rather than appended to the 3600-line `livegraph_feed.rs`, honouring the 500-line guardrail
//! (CLAUDE.md: "Do not append new responsibilities to files over 500 lines"); the pattern (struct +
//! compare + `build_and_store` + cert-state accessor) is identical to the others. The compare fns +
//! build + accessor are here; the fixture and tests are split into sibling modules (review-1 pt5).
//!
//! **What the cert proves (spec §7).** GREEN iff, for the finite UNION CORPUS drawn from BOTH the
//! LiveGraph resident snapshot ([`repo_graph_livegraph::focus_resolver::FocusCorpus`]) AND the SQLite
//! `nodes` enumeration (`query_all_nodes`), plus negative samples, the LiveGraph resolution is
//! FIELD-EQUAL to the SQLite `resolve_*` resolution (`storage::agent_impl`) for EVERY focus, across
//! all four functions. Driving the compare from the UNION makes the proof BIDIRECTIONAL — it certifies
//! true FILE/MODULE/SYMBOL SET EQUALITY, so neither an LG identity SQLite lacks NOR a SQLite identity
//! the LiveGraph lacks (incl. a non-AST fallback node the resolver skips) can pass GREEN (review-2).
//! This is the value-equivalence proof that licenses the later COHERENCE-LEAF-SERVE consumer to serve
//! a LiveGraph-resolved identity (and skip the eager `nodes` read) on green; RED -> the existing
//! SQLite resolution.
//!
//! **The serve-ladder accessor (review-1 pt1).** [`focus_resolution_is_green`] is the production
//! primitive the consumer calls: it mirrors the cert-state ladder in
//! `livegraph_feed::cycles_auto_response`/`stats_auto_response` EXACTLY — fingerprint reuse via the
//! SHARED [`crate::livegraph_feed::import_cert_fingerprint`], cached-verdict reuse at a matching
//! fingerprint, lazy (re)build on a stale/missing cert, and fingerprint-mismatch invalidation — but
//! returns the GREEN verdict instead of serving an answer. There is intentionally NO orient/explain
//! dispatch call site in THIS slice: the producer is STANDALONE (spec §12; the consumer wiring is the
//! later COHERENCE-LEAF-SERVE slice). The cert is NOT eagerly warmed at the partition refresh swap —
//! that path is per-partition and has no request-scoped `snapshot_uid`, and the import/cycles/stats
//! certs it mirrors are lazy-at-serve, not warmed at feed. So the cert fires lazily the first time
//! the consumer calls this accessor, exactly as the other certs fire on first serve.
//!
//! **Honest scope.** This slice is the PRODUCER + cert only. NO orient/explain wiring; the cert is
//! built/stored and independently tested, never yet consumed to skip a read. The no-`nodes`-read
//! SERVE proof (a storage spy on the consumer's green fastpath) belongs to the consumer slice (V2);
//! the producer's own GREEN-decision no-read proof (a panicking SQLite closure the green path never
//! calls) is in `tests`, and the resolver's structural zero-read proof is in `focus_resolver`'s
//! unit tests (`repo-graph-livegraph` has no storage dependency).
//!
//! **Boundaries.** NO new dependency edge: `daemon-runtime` already depends on `repo-graph-agent`
//! (the `AgentStorageRead` trait — the SQLite parity target) and `repo-graph-livegraph` (the
//! resolver). The compare reads SQLite ONCE per fingerprint (point lookups over the corpus), then a
//! GREEN cert lets the consumer fastpath skip SQLite entirely.

use std::collections::BTreeSet;

use repo_graph_agent::{
    AgentFocusCandidate, AgentFocusKind, AgentPathResolution, AgentStorageRead, AgentSymbolContext,
};
use repo_graph_livegraph::focus_resolver::{
    FocusCandidate, FocusCorpus, FocusKind, PathResolutionAnswer, SymbolContext,
};
use repo_graph_storage::types::GraphNode;
use repo_graph_trust_model::AnswerClass;

use crate::livegraph_feed::import_cert_fingerprint;
use crate::state::RepoState;

#[cfg(test)]
mod test_fixture;
#[cfg(test)]
mod tests;

/// Synthetic focus strings guaranteed to resolve to NOTHING — the cert's negative sample (spec §7d):
/// a GREEN cert must also prove the resolver does not FALSE-POSITIVE (both sides miss identically).
/// One plain string (exercises the symbol-name + the stable-key symbol branch + a path miss) and one
/// path-shaped string (exercises the path miss + content-prefix miss).
const NEGATIVE_SAMPLES: &[&str] = &[
    "__rgr_focus_cert_negative_marker__",
    "__rgr_focus_cert__/no/such/dir__",
];

/// ORIENT/EXPLAIN focus-resolution NO-LOSS certificate (mirrors `ImportNoLossCert` /
/// `StatsNoLossCert` / `ComplexityNoLossCert`). `verdict == "GREEN"` iff the LiveGraph focus
/// resolution is field-exact equal to the SQLite resolution over the corpus; `fingerprint` is the
/// SHARED SQLite-free fingerprint (the SAME `import_cert_fingerprint` the other certs use) it was
/// built at — a fingerprint mismatch invalidates + rebuilds. NOT durable (rebuilt on restart).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusResolutionNoLossCert {
    /// The repo-wide field-exact compare verdict (`GREEN` / `RED`).
    pub verdict: String,
    /// The SQLite-free fingerprint this verdict was computed at (the invalidation key).
    pub fingerprint: String,
}

/// The focus-resolution cert's status for the CURRENT fingerprint (mirrors `CycleCertState` /
/// `StatsCertState` in `livegraph_feed`).
enum FocusCertState {
    /// A valid cert at the current fingerprint, verdict GREEN -> the consumer may serve LiveGraph.
    ValidGreen,
    /// A valid cert at the current fingerprint, verdict != GREEN -> SQLite resolution.
    ValidNotGreen,
    /// No cert, or a cert at a DIFFERENT fingerprint -> (re)build, else SQLite resolution.
    StaleOrMissing,
}

/// Map a LiveGraph [`FocusKind`] to a SQLite [`AgentFocusKind`] for the equality check.
fn kind_eq(lg: FocusKind, sq: AgentFocusKind) -> bool {
    matches!(
        (lg, sq),
        (FocusKind::File, AgentFocusKind::File)
            | (FocusKind::Module, AgentFocusKind::Module)
            | (FocusKind::Symbol, AgentFocusKind::Symbol)
    )
}

/// `resolve_path` field equality: LiveGraph [`PathResolutionAnswer`] vs SQLite [`AgentPathResolution`].
fn path_eq(lg: &PathResolutionAnswer, sq: &AgentPathResolution) -> bool {
    lg.has_exact_file == sq.has_exact_file
        && lg.file_key == sq.file_stable_key
        && lg.has_content_under_prefix == sq.has_content_under_prefix
        && lg.module_key == sq.module_stable_key
}

/// One focus candidate field equality: LiveGraph [`FocusCandidate`] vs SQLite [`AgentFocusCandidate`].
fn candidate_eq(lg: &FocusCandidate, sq: &AgentFocusCandidate) -> bool {
    lg.key == sq.stable_key && kind_eq(lg.kind, sq.kind) && lg.file == sq.file
}

/// `resolve_stable_key` option equality (both miss, or both hit with equal fields).
fn opt_candidate_eq(lg: &Option<FocusCandidate>, sq: &Option<AgentFocusCandidate>) -> bool {
    match (lg, sq) {
        (None, None) => true,
        (Some(l), Some(s)) => candidate_eq(l, s),
        _ => false,
    }
}

/// `resolve_symbol_name` vector equality (same length, positionally equal — both sides order by
/// stable key ascending, so order parity is part of the proof).
fn candidates_eq(lg: &[FocusCandidate], sq: &[AgentFocusCandidate]) -> bool {
    lg.len() == sq.len() && lg.iter().zip(sq.iter()).all(|(l, s)| candidate_eq(l, s))
}

/// `get_symbol_context` option equality across all seven fields.
fn context_eq(lg: &Option<SymbolContext>, sq: &Option<AgentSymbolContext>) -> bool {
    match (lg, sq) {
        (None, None) => true,
        (Some(l), Some(s)) => {
            l.file_path == s.file_path
                && l.module_path == s.module_path
                && l.module_key == s.module_stable_key
                && l.name == s.name
                && l.qualified_name == s.qualified_name
                && l.subtype == s.subtype
                && l.line_start == s.line_start
        }
        _ => false,
    }
}

// ── SQLite-side identity-surface parsers (mirror the resolver's key-parse invariant) ────────────
//
// The cert enumerates the SQLite `nodes` table (`query_all_nodes`) so the parity compare is
// BIDIRECTIONAL — it proves the LiveGraph resolution is field-equal to SQLite for every identity in
// EITHER store, i.e. true SET EQUALITY, not a one-sided LG⊆SQLite subset (review-2 pt1/pt2). A FILE
// node's resolvable focus is its repo-relative path; a MODULE node's is its `qualified_name` (the
// directory `resolve_path_focus` matches). The `repo_uid` carries no `:`, so the first `:` is the
// repo/rest boundary — the SAME invariant the resolver's own parsers rely on.

/// The repo-relative path of a FILE node's `{repo}:{path}:FILE` stable key (`None` if not a FILE key).
fn file_node_path(stable_key: &str) -> Option<&str> {
    let inner = stable_key.strip_suffix(":FILE")?;
    Some(inner.split_once(':')?.1)
}

/// The directory of a MODULE node's `{repo}:{dir}:MODULE` stable key (`None` if not a MODULE key).
/// Used only as a fallback when the node row carries no `qualified_name`.
fn module_node_dir(stable_key: &str) -> Option<&str> {
    let inner = stable_key.strip_suffix(":MODULE")?;
    Some(inner.split_once(':')?.1)
}

/// The UNION parity corpus over BOTH the resident LiveGraph (`focus_corpus`) AND the SQLite `nodes`
/// table. Driving the compare from this union is what makes the no-loss proof bidirectional: a
/// SQLite-extra FILE/MODULE/SYMBOL (or a non-AST `ScipSynthesizedFallback` node the resolver skips)
/// the LiveGraph CANNOT reproduce surfaces as a focus the LiveGraph misses -> divergence -> RED
/// (review-2 pt1/pt2). A one-sided LG-only corpus could pass GREEN while SQLite held extra
/// identities — the false no-loss this removes.
struct ParityCorpus {
    /// Every resolve_path focus: FILE paths + MODULE dirs, from BOTH sides.
    path_foci: Vec<String>,
    /// Every resolve_stable_key focus: ALL SQLite node keys (any kind) + the LiveGraph's AST-adopted
    /// symbol keys. `resolve_stable_key_focus` maps any non-FILE/MODULE kind to `Symbol`, so EVERY
    /// SQLite key must be checked — not just `kind='SYMBOL'` — or a foreign-kind node could slip the
    /// set-equality proof.
    key_foci: Vec<String>,
    /// Every resolve_symbol_name focus: distinct SYMBOL names, from BOTH sides.
    name_foci: Vec<String>,
}

/// Build the [`ParityCorpus`] union from the LiveGraph corpus + the SQLite node enumeration. Each set
/// is a `BTreeSet` so the corpus is deterministically ordered and deduped across the two sources.
fn build_parity_corpus(lg: FocusCorpus, sqlite_nodes: &[GraphNode]) -> ParityCorpus {
    let mut paths: BTreeSet<String> = BTreeSet::new();
    paths.extend(lg.file_paths);
    paths.extend(lg.module_dirs);
    let mut keys: BTreeSet<String> = lg.symbol_keys.into_iter().collect();
    let mut names: BTreeSet<String> = lg.symbol_names.into_iter().collect();

    for n in sqlite_nodes {
        match n.kind.as_str() {
            "FILE" => {
                if let Some(p) = file_node_path(&n.stable_key) {
                    paths.insert(p.to_string());
                }
            }
            "MODULE" => {
                // resolve_path_focus matches a MODULE by `qualified_name = path`; fall back to the
                // key's dir segment if the row carries no qualified_name.
                if let Some(d) = n
                    .qualified_name
                    .as_deref()
                    .or_else(|| module_node_dir(&n.stable_key))
                {
                    paths.insert(d.to_string());
                }
            }
            "SYMBOL" => {
                names.insert(n.name.clone());
            }
            _ => {}
        }
        // EVERY SQLite node key is a resolve_stable_key focus (any non-FILE/MODULE kind -> Symbol).
        keys.insert(n.stable_key.clone());
    }

    ParityCorpus {
        path_foci: paths.into_iter().collect(),
        key_foci: keys.into_iter().collect(),
        name_foci: names.into_iter().collect(),
    }
}

/// Run the SHARED field-exact focus-resolution compare -> `Some(true)` iff the LiveGraph resolution
/// equals the SQLite resolution for EVERY focus in the UNION corpus (both stores) across all four
/// functions; `Some(false)` on the first divergence (or a non-`Exact` LiveGraph envelope, or an
/// absent producer); `None` only on a storage error (the caller treats it as NOT green -> safe
/// SQLite fallback).
///
/// **Bidirectional set-equality (review-2).** The corpus is the UNION of the LiveGraph's resident
/// identities AND the SQLite `nodes` enumeration (`query_all_nodes`), so the proof closes BOTH
/// directions: an LG identity SQLite lacks AND a SQLite identity the LiveGraph lacks (incl. a
/// `ScipSynthesizedFallback` node the resolver skips) each force RED. A one-sided LG-only corpus
/// could pass GREEN while SQLite held extra identities — the false no-loss this fix removes.
///
/// The per-focus `class() != Exact` short-circuit is the completeness gate: a non-resident /
/// non-Fresh / non-TS partition makes the LiveGraph resolution non-exhaustive ("null = unknown,
/// never empty"), so it can NEVER certify GREEN — only the SQLite resolution is authoritative there.
fn focus_resolution_compare_is_exact(repo_state: &RepoState, snapshot_uid: &str) -> Option<bool> {
    let guard = repo_state.livegraph.read();
    let lg = match guard.as_ref() {
        Some(lg) => lg,
        // No LiveGraph at all -> the producer cannot corroborate anything -> never GREEN.
        None => return Some(false),
    };
    // Producer-absent guard: with no resident partition the corpus is empty and an empty-corpus
    // compare would vacuously pass while SQLite may hold identities -> conservatively NOT green.
    if lg.live_partitions().is_empty() {
        return Some(false);
    }
    // D-S = S-A: one fresh per-operation connection for the cert-build reads; open failure -> None
    // (NOT green; safe SQLite fallback). The orient read guard keeps these reads snapshot-consistent.
    let storage = repo_state.storage().ok()?;
    // The SQLite identity surface — the sanctioned cert-BUILD read (spec §7b): every FILE/MODULE/
    // SYMBOL node, so the parity corpus is the UNION of both stores' identities. The GREEN SERVE path
    // reads no `nodes` (proven in `tests`).
    let sqlite_nodes = storage.query_all_nodes(snapshot_uid).ok()?;
    let corpus = build_parity_corpus(lg.focus_corpus(), &sqlite_nodes);

    // ── resolve_path parity (FILE exact/prefix + MODULE identity), both directions ──
    for path in &corpus.path_foci {
        let env = lg.resolve_path(path);
        if env.class() != AnswerClass::Exact {
            return Some(false);
        }
        let sq_pr = storage.resolve_path_focus(snapshot_uid, path).ok()?;
        if !path_eq(env.data()?, &sq_pr) {
            return Some(false);
        }
    }

    // ── resolve_stable_key parity for EVERY key (any kind) + symbol_context for symbols ──
    for key in &corpus.key_foci {
        let lg_c = lg.resolve_stable_key(key);
        if lg_c.class() != AnswerClass::Exact {
            return Some(false);
        }
        let sq_c = storage.resolve_stable_key_focus(snapshot_uid, key).ok()?;
        if !opt_candidate_eq(lg_c.data()?, &sq_c) {
            return Some(false);
        }
        // Both sides agree on the candidate. When it is a Symbol, also prove symbol_context parity.
        // A non-AST fallback node would already have diverged above (LG None vs SQLite Some) -> RED.
        if matches!(sq_c.as_ref().map(|c| c.kind), Some(AgentFocusKind::Symbol)) {
            let lg_ctx = lg.symbol_context(key);
            if lg_ctx.class() != AnswerClass::Exact {
                return Some(false);
            }
            let sq_ctx = storage.get_symbol_context(snapshot_uid, key).ok()?;
            if !context_eq(lg_ctx.data()?, &sq_ctx) {
                return Some(false);
            }
        }
    }

    // ── resolve_symbol_name parity (<=5, ordered), both directions ──
    for name in &corpus.name_foci {
        let lg_n = lg.resolve_symbol_name(name);
        if lg_n.class() != AnswerClass::Exact {
            return Some(false);
        }
        let sq_n = storage.resolve_symbol_name(snapshot_uid, name).ok()?;
        if !candidates_eq(lg_n.data()?, &sq_n) {
            return Some(false);
        }
    }

    // ── Negative samples: every known-miss must miss identically on both sides ──
    for neg in NEGATIVE_SAMPLES {
        let lg_pr = lg.resolve_path(neg);
        if lg_pr.class() != AnswerClass::Exact {
            return Some(false);
        }
        let sq_pr = storage.resolve_path_focus(snapshot_uid, neg).ok()?;
        if !path_eq(lg_pr.data()?, &sq_pr) {
            return Some(false);
        }
        let lg_c = lg.resolve_stable_key(neg);
        let sq_c = storage.resolve_stable_key_focus(snapshot_uid, neg).ok()?;
        if !opt_candidate_eq(lg_c.data()?, &sq_c) {
            return Some(false);
        }
        let lg_n = lg.resolve_symbol_name(neg);
        let sq_n = storage.resolve_symbol_name(snapshot_uid, neg).ok()?;
        if !candidates_eq(lg_n.data()?, &sq_n) {
            return Some(false);
        }
    }

    Some(true)
}

/// Build the focus-resolution no-loss cert -> verdict, STORE it keyed by `fingerprint`, return
/// `Some(is_green)` (or `None` if no fingerprint / a storage error -> the caller falls back to
/// SQLite). Mirrors `build_and_store_complexity_cert` / `build_and_store_stats_cert`.
///
/// The `fingerprint` is the SHARED SQLite-free fingerprint the caller computes via
/// `livegraph_feed::import_cert_fingerprint(&lg.live_partitions(), snapshot_uid)` — NO new
/// invalidation key (spec §7b). The COHERENCE-LEAF-SERVE consumer (a later slice) reaches this via
/// [`focus_resolution_is_green`] at the current fingerprint to gate its fastpath.
pub(crate) fn build_and_store_focus_resolution_cert(
    repo_state: &RepoState,
    snapshot_uid: &str,
    fingerprint: Option<String>,
) -> Option<bool> {
    let fingerprint = fingerprint?;
    let is_green = focus_resolution_compare_is_exact(repo_state, snapshot_uid)?;
    let verdict = if is_green { "GREEN" } else { "RED" }.to_string();
    *repo_state.focus_resolution_cert.write() = Some(FocusResolutionNoLossCert {
        verdict,
        fingerprint,
    });
    Some(is_green)
}

/// FOCUS-RESOLUTION serve-ladder accessor (review-1 pt1) — the production primitive the
/// COHERENCE-LEAF-SERVE consumer (a LATER slice, spec §12) calls to decide whether its
/// focused-orient/explain fastpath may serve a LiveGraph-resolved identity (and skip the eager
/// `nodes` read). Returns `true` iff the focus-resolution no-loss cert is GREEN at the CURRENT
/// fingerprint.
///
/// Mirrors the cert-state ladder in `livegraph_feed::cycles_auto_response` / `stats_auto_response`
/// EXACTLY:
/// - **Fingerprint reuse:** the current fingerprint is the SHARED SQLite-free
///   [`import_cert_fingerprint`] over the resident partitions (NO new invalidation key, spec §7b).
/// - **Cached-verdict reuse:** a cert whose stored fingerprint equals the current one is used as-is
///   (GREEN -> `true`, else `false`) WITHOUT re-reading SQLite.
/// - **Lazy (re)build:** no cert / a cert at a different fingerprint -> rebuild once via
///   [`build_and_store_focus_resolution_cert`], then use the fresh verdict.
/// - **Invalidation:** because the fingerprint embeds the partition epochs + snapshot_uid + policy
///   version, any refresh/swap/re-index changes it -> the next call sees `StaleOrMissing` -> rebuild;
///   a stale cert never serves.
///
/// Returns `false` (the safe SQLite-resolution default) when there is no LiveGraph, no resident
/// partition, or a storage error during the build. SQLite is read ONLY to (re)build the cert; a
/// cached GREEN/RED at the current fingerprint reads NO SQLite.
pub fn focus_resolution_is_green(repo_state: &RepoState, snapshot_uid: &str) -> bool {
    // SQLite-FREE: the current fingerprint from the resident partition snapshot. The read guard is
    // dropped at the end of this block so the lazy build below can re-lock without deadlock.
    let current_fp = {
        let guard = repo_state.livegraph.read();
        guard
            .as_ref()
            .map(|lg| import_cert_fingerprint(&lg.live_partitions(), snapshot_uid))
    };
    // The cert's state for the CURRENT fingerprint.
    let state = {
        let cached = repo_state.focus_resolution_cert.read();
        match (&current_fp, cached.as_ref()) {
            (Some(fp), Some(c)) if &c.fingerprint == fp => {
                if c.verdict == "GREEN" {
                    FocusCertState::ValidGreen
                } else {
                    FocusCertState::ValidNotGreen
                }
            }
            _ => FocusCertState::StaleOrMissing,
        }
    };
    match state {
        FocusCertState::ValidGreen => true,
        FocusCertState::ValidNotGreen => false,
        FocusCertState::StaleOrMissing => {
            build_and_store_focus_resolution_cert(repo_state, snapshot_uid, current_fp)
                .unwrap_or(false)
        }
    }
}
