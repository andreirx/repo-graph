//! RECON-SPIKE-1: additive, GATED CALLGRAPH DIVERGENCE emission — stop discarding the diff.
//!
//! The callgraph cert ([`super::callgraph_compare_is_exact`]) already runs an exhaustive per-symbol
//! multiset compare of the two call-graph witnesses (the SCIP-fed LiveGraph rows vs the tree-sitter
//! pipeline's SQLite rows) on every fingerprint, then reduces the whole result to ONE bit (GREEN serve /
//! RED fallback) and discards the per-symbol detail. This module captures that detail — WITHOUT touching
//! the verdict — so the reconciliation design (RECON-DESIGN-1) has real divergence data to build from.
//!
//! **Two witnesses, not a fight (ratified 2026-07-16).** The LiveGraph is fed from SCIP ingest
//! (`repo-graph-livegraph-feed`); the SQLite `nodes`/`edges` come from the homegrown tree-sitter
//! pipeline. So a LiveGraph-only edge is a SCIP-witnessed edge the pipeline lacks; a SQLite-only edge is a
//! pipeline-witnessed edge SCIP lacks. The report names the two sides by the LITERAL store they were read
//! from (`livegraph_only` / `sqlite_only`) and carries an explicit [`WitnessMapping`] so the SCIP↔pipeline
//! reading is stated as labeled evidence, never baked into a name that could drift if the feed changed.
//!
//! **Additive + off by default + verdict-unchanged.** Gated on the `RMAP_CALLGRAPH_DIFF` env var (a
//! directory path). Unset → [`maybe_emit`] returns after a single `var_os` lookup: no corpus walk, no
//! artifact, the GREEN/RED path in [`super::build_and_store_callgraph_cert`] is byte-for-byte unchanged.
//! Set → after the authoritative verdict is computed, a SEPARATE full-corpus collector (no short-circuit)
//! classifies every mismatch and writes one JSON artifact. The collector is a pure read (reuses the SAME
//! `lg_caller_rows`/`lg_callee_rows` builders + `find_symbol_callers`/`callees` reads the verdict uses —
//! NO new SQLite surface, NO new dependency edge) and is best-effort (a write error is swallowed, never
//! surfaced to the query). The double corpus walk when ON is the deliberate price of leaving the verdict
//! authority ([`super::callgraph_compare_is_exact`]) untouched; the report echoes that verdict as a
//! cross-check — SCOPED to the MEASURED path (`precondition == null`): there, `cert_verdict` RED ⟺
//! `divergent_symbol_count` ≥ 1, by construction. On the DEGENERATE path (`precondition != null` — no
//! corpus walked) the verdict is still RED (the fallback), but every measurement including
//! `divergent_symbol_count` serializes as `null` (UNKNOWN, not 0), so the equivalence does NOT apply
//! there — RED-with-`divergent_symbol_count: null` is the honest degenerate shape, NOT a contradiction.
//!
//! **Panic-safe by design (RECON-SPIKE-1 live finding).** The exhaustive walk continues PAST the point
//! where the short-circuiting verdict stops (its first divergent symbol), so it can reach symbols whose
//! `repo-graph-livegraph` answer construction PANICS on a documented latent invariant (an
//! `AstFileScope`-basis `Partial` with no mapped `DegradationReason`; `lib.rs:303-306`, "unreachable with
//! current call-graph fixtures"). The shipped verdict's own exposure to that panic is DATA-DEPENDENT (it
//! escapes only when a divergence precedes the affected symbol — a P1 fail-soft gap owned by
//! LIVEGRAPH-PARTIAL-FIX-1, see [`lg_side`]). [`lg_side`] catches the panic per symbol so THIS collector
//! never crashes the daemon when the spike is enabled (essential for the monorepo run), and records it as
//! the `livegraph_panic` class — itself a RECON-DESIGN-1 input.
//!
//! **Two edge accountings, kept distinct (review-2 correction, schema `v3`).** A directed call-graph edge
//! `caller -> callee` is witnessed TWICE by the two point queries: once in the caller's callee-projection
//! (`find_symbol_callees(caller)`) and once in the callee's caller-projection (`find_symbol_callers
//! (callee)`). So the per-DIRECTION [`ProjectionIncidences`] (caller-side + callee-side counts) are
//! PROJECTION INCIDENCES — SUMMING the two directions double-counts every edge whose both endpoints are in
//! the corpus. The MAGNITUDE answer ("share of total edges / is SCIP richer") must instead be an
//! EDGE-LEVEL count by canonical identity `(caller_key, callee_key)`: [`CanonicalEdgeMagnitude`] merges
//! each witness's two projections per identity (MAX, so a both-projections edge counts ONCE; an edge whose
//! one projection was UNMEASURED is recovered from the other — unknown ≠ a phantom 0), yielding
//! `scip_only`/`pipeline_only`/`shared` and the union denominator `D`. The report carries BOTH: the
//! projection incidences (labeled as such) AND the canonical magnitude (authoritative).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use repo_graph_agent::AgentStorageRead;
use repo_graph_trust_model::Granularity;
use serde::Serialize;

// RECON-M-R1: the comparison primitives this collector prototyped moved to the graduated
// substrate (`super::ledger`) — the spike collector now CONSUMES them (recon-design-1 §5.1: the
// diff.rs collector "graduates from env-gated debug artifact to serving substrate"). Behavior
// unchanged; this module keeps the artifact report/gating/emission.
use super::ledger::{
    callee_pairs, caller_pairs, diff_direction, lg_side, DirectionDiff, EdgeViews, WitnessLedger,
};
use super::{lg_callee_rows, lg_caller_rows};
use crate::state::RepoState;

/// The env var that GATES emission. Its VALUE is the output DIRECTORY; the artifact is written to
/// `<dir>/callgraph-diff.json` (latest-wins, deterministic path — a spike captures one snapshot per graph
/// state). Unset ⇒ off (default). Mirrors the `RMAP_SCIP_TYPESCRIPT` / `RMAP_PERF` env-gating convention;
/// no new CLI flag / dispatch plumbing (least-new-surface).
const DIFF_ENV: &str = "RMAP_CALLGRAPH_DIFF";
const ARTIFACT_NAME: &str = "callgraph-diff.json";
/// Schema label for this off-by-default, gitignored, spike-only debug artifact (introduced by THIS
/// unmerged slice; referenced by no other crate/command/persisted surface — refining its shape before
/// merge is not a §3 "schema change"). History: `v1` folded unknown edge counts in as 0; `v2` (review-1)
/// split MEASURED edges from a count of UNMEASURED symbols and rendered an unknown side as `null` (unknown
/// ≠ zero). `v3` (review-2) adds [`CanonicalEdgeMagnitude`] — the EDGE-LEVEL magnitude by canonical
/// `(caller_key, callee_key)` identity — and RENAMES the per-direction `totals` to `projection_incidences`
/// (`edges` → `incidences`) so the projection counts are never again mistaken for distinct directed edges.
/// `v4` (RECON-M-R1) is ADDITIVE over `v3`: a `witness_ledger` block (the stored ledger's summary —
/// the kind-aligned instance classification + derived verdict) plus the [`CANONICAL_EDGE_NOTE`]
/// coverage caveat (§3.7-5). Retained `v3` artifacts stay valid evidence; they are never
/// regenerated in place.
const SCHEMA: &str = "callgraph-diff/v4";

/// Which physical store each side of the diff was read from, and what producer feeds it — stated as
/// explicit evidence so the DIRECTION axis ("SCIP-only vs pipeline-only") is traceable, not implied.
#[derive(Debug, Clone, Copy, Serialize)]
struct WitnessMapping {
    /// The `livegraph_only` side: rows from the in-memory LiveGraph, fed by SCIP ingest.
    livegraph: &'static str,
    /// The `sqlite_only` side: rows from SQLite `nodes`/`edges`, produced by the tree-sitter pipeline.
    sqlite: &'static str,
}

const WITNESS: WitnessMapping = WitnessMapping {
    livegraph: "scip_fed_livegraph",
    sqlite: "tree_sitter_pipeline_sqlite",
};

/// Per-(symbol, direction) edge counts, HONEST about unknown ≠ zero (VISION honesty rule). `None`
/// (serialized `null`) = this witness produced NO measurable answer for this symbol (LiveGraph
/// unanswerable / panicked, or a SQLite read error); `Some(0)` = measured and genuinely empty. So a
/// panicked FILE symbol records `livegraph: null`, NEVER `livegraph: 0`.
#[derive(Debug, Clone, Copy, Serialize)]
struct SideEdgeCounts {
    livegraph: Option<usize>,
    sqlite: Option<usize>,
}

/// One divergent symbol's full detail. Only symbols with SOME divergence (non-empty buckets or a note)
/// are emitted, so `symbols.len()` == `divergent_symbol_count`.
#[derive(Debug, Serialize)]
struct SymbolDivergence {
    symbol: String,
    /// Caller-side edge counts per witness (unknown ≠ zero — see [`SideEdgeCounts`]). A divergent symbol
    /// whose LiveGraph caller answer was unanswerable/panicked reads `livegraph: null` here, not `0`.
    caller_edges: SideEdgeCounts,
    callers: DirectionDiff,
    /// `Some(reason)` when the LiveGraph could not produce Exact caller rows (non-Exact answer class or an
    /// un-enrichable caller) — the same condition that forces the cert RED. Direction buckets are then
    /// empty; the note carries the cause, so "unanswerable" is never mislabeled as "pipeline-only".
    callers_note: Option<String>,
    /// Callee-side edge counts per witness (unknown ≠ zero — see [`SideEdgeCounts`]).
    callee_edges: SideEdgeCounts,
    callees: DirectionDiff,
    callees_note: Option<String>,
}

/// One witness-side's PROJECTION-INCIDENCE count for one direction, HONEST about unknown ≠ zero:
/// `incidences` sums ONLY over symbols where this side produced a MEASURED answer; symbols where the side
/// was unanswerable / panicked / errored are counted in `unmeasured_symbols`, NEVER folded in as 0. NOTE
/// this is a per-DIRECTION incidence count, NOT a distinct-edge count — the same directed edge is one
/// caller-side incidence AND one callee-side incidence, so the caller and callee directions must NEVER be
/// summed to "total edges" (that is the review-2 double-count; use [`CanonicalEdgeMagnitude`] for edges).
#[derive(Debug, Default, Serialize)]
struct SideProjection {
    /// Sum of projection incidences over ONLY the symbols where this side's answer was measured.
    incidences: usize,
    /// Count of symbols where this side had NO measurable answer (unknown — never summed as 0).
    unmeasured_symbols: usize,
}

/// Per-DIRECTION, per-witness PROJECTION INCIDENCES (renamed from `totals` in `v3` — the old name invited
/// summing the directions into "total edges", which double-counts). Four concrete users (livegraph/sqlite
/// × caller/callee); the [`SideProjection`] split exists because each side can be independently unmeasured
/// and unknown must never read as zero. These are DIAGNOSTIC (they show per-direction reach + unmeasured
/// coverage); the distinct-edge MAGNITUDE lives in [`CanonicalEdgeMagnitude`]. Simpler rejected
/// alternative: four flat `usize` totals — the pre-review-1 shape that collapsed unknown into 0.
#[derive(Debug, Default, Serialize)]
struct ProjectionIncidences {
    livegraph_caller: SideProjection,
    sqlite_caller: SideProjection,
    livegraph_callee: SideProjection,
    sqlite_callee: SideProjection,
}

/// EDGE-LEVEL MAGNITUDE by CANONICAL directed-edge identity `(caller_key, callee_key)`, multiplicity
/// preserved — the honest answer to the contract's "share of total edges / is SCIP richer" (review-2).
/// Built by [`edge_magnitude`] from each witness's [`EdgeViews::canonical`] multiset (the two projections
/// MERGED per identity, so an edge seen from BOTH counts ONCE), NOT by summing the [`ProjectionIncidences`]
/// directions (that double-counts every edge whose both endpoints are in the corpus). Invariant:
/// `scip_only + pipeline_only + shared == union_edges`. Unknown ≠ zero: an edge whose sole projection was
/// UNMEASURED is recovered from the other side; one with NO measured projection never appears (its
/// unknown-ness is in [`ProjectionIncidences::unmeasured_symbols`] / [`DiffRollup`], not a phantom edge).
#[derive(Debug, Default, Serialize)]
struct CanonicalEdgeMagnitude {
    /// Fixed reader's note so the retained artifact self-describes this accounting for RECON-DESIGN-1.
    note: &'static str,
    /// Distinct canonical directed edges (multiplicity preserved) the SCIP-fed LiveGraph witnesses.
    livegraph_total: usize,
    /// Distinct canonical directed edges the tree-sitter/SQLite pipeline witnesses.
    sqlite_total: usize,
    /// Canonical edges on the LiveGraph but NOT the pipeline (SCIP-only).
    scip_only: usize,
    /// Canonical edges on the pipeline but NOT the LiveGraph (pipeline-only).
    pipeline_only: usize,
    /// Canonical edges present on BOTH witnesses (multiset intersection).
    shared: usize,
    /// The UNION denominator `D` = `scip_only + pipeline_only + shared` = |livegraph ∪ pipeline|.
    union_edges: usize,
}

/// Per-class counts across the whole corpus (the DIRECTION-axis rollup).
#[derive(Debug, Default, Serialize)]
struct DiffRollup {
    callers_livegraph_only: usize,
    callers_sqlite_only: usize,
    callees_livegraph_only: usize,
    callees_sqlite_only: usize,
    field_mismatch: usize,
    /// (symbol, direction) pairs where the LiveGraph answer was non-Exact / un-enrichable.
    livegraph_unanswerable: usize,
    /// (symbol, direction) pairs where the LiveGraph answer construction PANICKED (a documented latent
    /// invariant — an `AstFileScope`-basis `Partial` with no mapped `DegradationReason`, `lib.rs:303-306`).
    /// The panic-safe walk records it instead of crashing the daemon; a nonzero count is a RECON-DESIGN-1
    /// input (the exhaustive reconciliation walk reaches symbols PAST the verdict's first-divergence
    /// short-circuit, so it hits what the shipped verdict may skip depending on key order).
    livegraph_panic: usize,
}

/// The full classified divergence report — the spike deliverable's machine-readable form.
#[derive(Debug, Serialize)]
struct CallgraphDiffReport {
    schema: &'static str,
    witness: WitnessMapping,
    /// The cert fingerprint this diff was computed at (the invalidation key).
    fingerprint: String,
    snapshot_uid: String,
    /// The AUTHORITATIVE verdict from [`super::callgraph_compare_is_exact`] — the cross-check.
    cert_verdict: &'static str,
    /// `Some(reason)` when the compare degenerated before walking a corpus (no resident LiveGraph / no
    /// resident partitions / a storage error). This is the HONEST-NULL marker: on a repo where the SCIP
    /// producer never ran, there is no LiveGraph to compare and the report says exactly that. **When this
    /// is `Some`, EVERY measurement field below is `None` (serialized `null`)** — no corpus was walked, so
    /// those values are UNKNOWN, never a measured `0`/`{}`/`[]` (VISION: unknown ≠ zero).
    precondition: Option<String>,
    /// Measurement fields. `Some(..)` on the MEASURED path (`precondition == None` — a corpus was walked);
    /// `None` (serialized `null`) on the DEGENERATE path (`precondition == Some`), where the value is
    /// UNKNOWN, not a measured zero. Serde renders `Some(x)` byte-identically to a bare `x`, so the measured
    /// artifact is unchanged; only the degenerate artifact flips from a phantom `0`/`{}`/`[]` to `null`.
    corpus_size: Option<usize>,
    divergent_symbol_count: Option<usize>,
    /// The AUTHORITATIVE edge-level magnitude (distinct canonical directed edges) — read THIS for
    /// "share of total edges / is SCIP richer", NOT the per-direction `projection_incidences`. `null` when
    /// no corpus was walked (unknown ≠ a zeroed magnitude).
    canonical_edges: Option<CanonicalEdgeMagnitude>,
    /// Per-direction PROJECTION INCIDENCES (diagnostic; do NOT sum the directions — see the struct doc).
    projection_incidences: Option<ProjectionIncidences>,
    rollup: Option<DiffRollup>,
    symbols: Option<Vec<SymbolDivergence>>,
    /// RECON-M-R1 (`v4`, additive): the STORED witness ledger's summary at this fingerprint — the
    /// kind-aligned instance classification + the derived verdict (see [`ledger_block`]). This is
    /// the observation channel for the M-R1 measured-baseline reproductions (amodx / zap-engine /
    /// the committed fixture) — the artifact stays env-gated, off-by-default, debug-only.
    witness_ledger: serde_json::Value,
}

// ── Gate + write ────────────────────────────────────────────────────────────────────────────────

/// GATED entry point called from [`super::build_and_store_callgraph_cert`] AFTER the verdict is computed.
/// Unset env ⇒ immediate return (zero added cost, verdict path untouched). Set ⇒ collect + write,
/// best-effort. `cert_is_green` is the authoritative verdict, echoed into the report for cross-check.
pub(super) fn maybe_emit(
    repo_state: &RepoState,
    snapshot_uid: &str,
    fingerprint: &str,
    cert_is_green: bool,
    ledger: &WitnessLedger,
) {
    let Some(dir) = std::env::var_os(DIFF_ENV) else {
        return; // OFF (default): one env lookup, nothing else.
    };
    let _ = emit_report(
        Path::new(&dir),
        repo_state,
        snapshot_uid,
        fingerprint,
        cert_is_green,
        ledger,
    );
}

/// Collect the diff and write it to `<out_dir>/callgraph-diff.json`. Returns the written path, or `None`
/// on a write error (best-effort; the caller ignores it). Split from [`maybe_emit`] so tests drive it with
/// an explicit dir and never touch the process-global env.
fn emit_report(
    out_dir: &Path,
    repo_state: &RepoState,
    snapshot_uid: &str,
    fingerprint: &str,
    cert_is_green: bool,
    ledger: &WitnessLedger,
) -> Option<PathBuf> {
    let mut report = collect(repo_state, snapshot_uid, fingerprint, cert_is_green);
    report.witness_ledger = ledger_block(ledger);
    std::fs::create_dir_all(out_dir).ok()?;
    let path = out_dir.join(ARTIFACT_NAME);
    let body = serde_json::to_string_pretty(&report).ok()?;
    std::fs::write(&path, body).ok()?;
    Some(path)
}

/// Serialize the stored [`WitnessLedger`]'s SUMMARY as a JSON block for the artifact — a boundary
/// DTO built as raw `serde_json::Value` (the ledger's domain types stay serde-free; only this
/// debug artifact crosses the boundary). Every population keeps its unit in its field name;
/// unknown is `null`, never 0 (degenerate ledgers carry `null` measurement blocks).
fn ledger_block(l: &WitnessLedger) -> serde_json::Value {
    use serde_json::{json, Value};
    let language_label = |lang: repo_graph_trust_model::LanguageSupport| -> &'static str {
        match lang {
            repo_graph_trust_model::LanguageSupport::TypeScriptPrimary => "typescript",
            repo_graph_trust_model::LanguageSupport::CppGuarded => "cpp",
            repo_graph_trust_model::LanguageSupport::RustPartialBeta => "rust",
        }
    };
    let compare = l.compare.as_ref().map(|c| {
        json!({
            "corpus_size": c.corpus_size,
            "divergent_symbols": c.divergent_symbols,
            "unanswerable_projections": c.unanswerable_projections,
            "livegraph_panics": c.livegraph_panics,
            "field_mismatches": c.field_mismatches,
            "projections": { "total": c.corpus_size * 2, "unanswerable": c.unanswerable_projections },
            "canonical": {
                "livegraph_total": c.canonical.livegraph_total,
                "sqlite_total": c.canonical.sqlite_total,
                "scip_only": c.canonical.scip_only,
                "pipeline_only_dual_measured": c.canonical.pipeline_only_dual_measured,
                "pipeline_only_unmeasured": c.canonical.pipeline_only_unmeasured,
                "shared": c.canonical.shared,
                "union_edges": c.canonical.union_edges,
            },
        })
    });
    let classification = l.classification.as_ref().map(|cl| {
        let eligible: serde_json::Map<String, Value> = cl
            .eligible
            .iter()
            .map(|(id, lang)| (id.clone(), Value::from(language_label(*lang))))
            .collect();
        let colliding: serde_json::Map<String, Value> = cl
            .colliding_keys
            .iter()
            .map(|(part, keys)| {
                (
                    part.clone(),
                    Value::from(keys.iter().cloned().collect::<Vec<String>>()),
                )
            })
            .collect();
        let rollups: serde_json::Map<String, Value> = cl
            .rollups
            .iter()
            .map(|((lang, part), r)| {
                (
                    format!("{}/{}", language_label(*lang), part),
                    json!({
                        "s_calls": r.s_calls,
                        "s_references": r.s_references,
                        "s_imports": r.s_imports,
                        "adoption_adopted": r.adoption_adopted,
                        "adoption_fallback": r.adoption_fallback,
                        "adoption_file_scope": r.adoption_file_scope,
                        "both_instances": r.both_instances,
                        "semantic_new_pair": r.semantic_new_pair,
                        "semantic_multiplicity": r.semantic_multiplicity,
                        "withheld_instances": r.withheld_instances,
                    }),
                )
            })
            .collect();
        let delta_pairs: Vec<Value> = cl
            .delta_pairs
            .iter()
            .map(|d| json!({ "caller": d.caller, "callee": d.callee, "p": d.p, "s_calls": d.s_calls }))
            .collect();
        json!({
            "eligible_partitions": eligible,
            "pipeline_calls": cl.pipeline_calls,
            "union_calls": cl.union_calls,
            "dual_measured": cl.dual_measured,
            "both": { "instances": cl.both, "identities": cl.both_identities },
            "syntactic_only": {
                "boundary": cl.syntactic.boundary,
                "file_scope": cl.syntactic.file_scope,
                "uncorroborated": cl.syntactic.uncorroborated,
                "multiplicity": cl.syntactic.multiplicity,
                "instances": cl.syntactic.total(),
                "identities": cl.syntactic.identities,
            },
            "semantic_only_calls": {
                "new_pair": cl.semantic.new_pair,
                "multiplicity": cl.semantic.multiplicity,
                "instances": cl.semantic.total(),
                "identities": cl.semantic.identities,
            },
            "unmeasured_edges": { "instances": cl.unmeasured_edges, "identities": cl.unmeasured_identities },
            // Percentage points (97.4, not 0.974 — the ratified gate figures; review-0 fix).
            // Unknown ≠ zero: no dual-measured instance -> the rate is null, never 0%.
            "agreement_pct": cl.agreement_pct().map(Value::from).unwrap_or(Value::Null),
            "s_kind_totals": {
                "calls": cl.s_kind_totals.calls,
                "references": cl.s_kind_totals.references,
                "imports": cl.s_kind_totals.imports,
            },
            "fallback_key_count": cl.fallback_key_count,
            "identity_collision": cl.identity_collision,
            "colliding_keys": colliding,
            "identity_suspect": cl.identity_suspect,
            "delta_pairs": delta_pairs,
            "rollups": rollups,
        })
    });
    json!({
        "derived_verdict": if l.derived_green() { "GREEN" } else { "RED" },
        "precondition": l.precondition,
        "compare": compare,
        "classification": classification,
    })
}

// ── Collector (full corpus, NO short-circuit) ─────────────────────────────────────────────────────

/// Walk the SAME resident∪SQLite corpus the verdict walks, but classify EVERY mismatch (no early return).
fn collect(
    repo_state: &RepoState,
    snapshot_uid: &str,
    fingerprint: &str,
    cert_is_green: bool,
) -> CallgraphDiffReport {
    let verdict = if cert_is_green { "GREEN" } else { "RED" };
    let guard = repo_state.livegraph.read();
    let lg = match guard.as_ref() {
        Some(lg) => lg,
        None => return degenerate(fingerprint, snapshot_uid, verdict, "no_resident_livegraph"),
    };
    if lg.live_partitions().is_empty() {
        return degenerate(fingerprint, snapshot_uid, verdict, "no_resident_partitions");
    }
    let storage = match repo_state.storage() {
        Ok(s) => s,
        Err(e) => {
            return degenerate(
                fingerprint,
                snapshot_uid,
                verdict,
                &format!("storage_open_error: {e}"),
            )
        }
    };
    let sqlite_nodes = match storage.query_all_nodes(snapshot_uid) {
        Ok(n) => n,
        Err(e) => {
            return degenerate(
                fingerprint,
                snapshot_uid,
                verdict,
                &format!("query_all_nodes_error: {e}"),
            )
        }
    };

    // The UNION corpus — identical to the verdict's (LiveGraph AST-adopted keys ∪ SQLite SYMBOL keys).
    let mut corpus: BTreeSet<String> = lg.focus_corpus().symbol_keys.into_iter().collect();
    for n in &sqlite_nodes {
        if n.kind.as_str() == "SYMBOL" {
            corpus.insert(n.stable_key.clone());
        }
    }

    let mut projection_incidences = ProjectionIncidences::default();
    let mut rollup = DiffRollup::default();
    let mut symbols: Vec<SymbolDivergence> = Vec::new();
    // Canonical directed-edge views, one per witness. Populated from the MEASURED endpoint keys of every
    // corpus symbol (divergent or not — shared edges live on non-divergent symbols like a GREEN caller),
    // then reduced to a per-witness canonical multiset after the walk (see `edge_magnitude`).
    let mut lg_edge_views = EdgeViews::default();
    let mut sq_edge_views = EdgeViews::default();

    for key in &corpus {
        // ── callers ── (the LiveGraph side is computed PANIC-SAFE — see `lg_side`)
        let callers = diff_direction(
            lg_side(
                || lg_caller_rows(lg, key).map(|r| caller_pairs(&r)),
                || format!("{:?}", lg.callers(key, Granularity::CallerDetail).class()),
            ),
            storage
                .find_symbol_callers(snapshot_uid, key)
                .map(|r| caller_pairs(&r))
                .map_err(|e| e.to_string()),
        );
        // ── callees ──
        let callees = diff_direction(
            lg_side(
                || lg_callee_rows(lg, key).map(|r| callee_pairs(&r)),
                || format!("{:?}", lg.callees(key, Granularity::CallerDetail).class()),
            ),
            storage
                .find_symbol_callees(snapshot_uid, key)
                .map(|r| callee_pairs(&r))
                .map_err(|e| e.to_string()),
        );

        accumulate_side(
            &mut projection_incidences.livegraph_caller,
            callers.lg_edges,
        );
        accumulate_side(&mut projection_incidences.sqlite_caller, callers.sq_edges);
        accumulate_side(
            &mut projection_incidences.livegraph_callee,
            callees.lg_edges,
        );
        accumulate_side(&mut projection_incidences.sqlite_callee, callees.sq_edges);

        // Canonical directed-edge views: `key`'s measured callers are edges `(caller, key)`; its measured
        // callees are edges `(key, callee)`. An UNMEASURED side (`None`) adds nothing (unknown ≠ a phantom
        // edge) — its unknown-ness already lives in `projection_incidences.unmeasured_symbols` / `rollup`.
        if let Some(ks) = &callers.lg_keys {
            lg_edge_views.add_callers(key, ks);
        }
        if let Some(ks) = &callees.lg_keys {
            lg_edge_views.add_callees(key, ks);
        }
        if let Some(ks) = &callers.sq_keys {
            sq_edge_views.add_callers(key, ks);
        }
        if let Some(ks) = &callees.sq_keys {
            sq_edge_views.add_callees(key, ks);
        }

        rollup.callers_livegraph_only += callers.diff.livegraph_only.len();
        rollup.callers_sqlite_only += callers.diff.sqlite_only.len();
        rollup.callees_livegraph_only += callees.diff.livegraph_only.len();
        rollup.callees_sqlite_only += callees.diff.sqlite_only.len();
        rollup.field_mismatch +=
            callers.diff.field_mismatch.len() + callees.diff.field_mismatch.len();
        rollup.livegraph_unanswerable +=
            usize::from(callers.lg_unanswerable) + usize::from(callees.lg_unanswerable);
        rollup.livegraph_panic +=
            usize::from(callers.lg_panicked) + usize::from(callees.lg_panicked);

        let divergent = !callers.diff.is_empty()
            || callers.note.is_some()
            || !callees.diff.is_empty()
            || callees.note.is_some();
        if divergent {
            // Capture the per-witness edge counts (Copy) BEFORE moving the buckets/notes out — a symbol
            // whose LiveGraph side was unanswerable/panicked records `livegraph: None` (→ `null`), not 0.
            let caller_edges = SideEdgeCounts {
                livegraph: callers.lg_edges,
                sqlite: callers.sq_edges,
            };
            let callee_edges = SideEdgeCounts {
                livegraph: callees.lg_edges,
                sqlite: callees.sq_edges,
            };
            symbols.push(SymbolDivergence {
                symbol: key.clone(),
                caller_edges,
                callers: callers.diff,
                callers_note: callers.note,
                callee_edges,
                callees: callees.diff,
                callees_note: callees.note,
            });
        }
    }

    // Merge each witness's two projection views into a canonical directed-edge multiset, then classify.
    let canonical_edges = edge_magnitude(&lg_edge_views.canonical(), &sq_edge_views.canonical());

    CallgraphDiffReport {
        schema: SCHEMA,
        witness: WITNESS,
        fingerprint: fingerprint.to_string(),
        snapshot_uid: snapshot_uid.to_string(),
        cert_verdict: verdict,
        // MEASURED path: a corpus was walked, so every measurement is `Some` (present). `Some(x)`
        // serializes byte-identically to a bare `x`, so this measured artifact is unchanged from `v3`.
        precondition: None,
        corpus_size: Some(corpus.len()),
        divergent_symbol_count: Some(symbols.len()),
        canonical_edges: Some(canonical_edges),
        projection_incidences: Some(projection_incidences),
        rollup: Some(rollup),
        symbols: Some(symbols),
        // Attached by `emit_report` from the STORED ledger (this collector walks independently).
        witness_ledger: serde_json::Value::Null,
    }
}

/// The report for a compare that never reached a corpus (no LiveGraph / no partitions / storage error) —
/// the honest-null path. The reason is in `precondition`; EVERY measurement field is `None` (serialized
/// `null`), NEVER a measured `0`/`{}`/`[]`: no corpus was walked, so those values are UNKNOWN (VISION:
/// unknown ≠ zero). In particular `cert_verdict` is still the authoritative verdict (RED — the fallback),
/// but `divergent_symbol_count` is `null`, so the "RED ⟺ ≥1 divergent symbol" cross-check (scoped to the
/// measured path — see the module header) does NOT apply here: RED with `divergent_symbol_count: null`.
fn degenerate(
    fingerprint: &str,
    snapshot_uid: &str,
    verdict: &'static str,
    reason: &str,
) -> CallgraphDiffReport {
    CallgraphDiffReport {
        schema: SCHEMA,
        witness: WITNESS,
        fingerprint: fingerprint.to_string(),
        snapshot_uid: snapshot_uid.to_string(),
        cert_verdict: verdict,
        precondition: Some(reason.to_string()),
        // No corpus walked → every measurement is UNKNOWN → `null` (never a phantom measured 0/empty).
        corpus_size: None,
        divergent_symbol_count: None,
        canonical_edges: None,
        projection_incidences: None,
        rollup: None,
        symbols: None,
        witness_ledger: serde_json::Value::Null,
    }
}

/// Fold one witness-side's per-symbol projection-incidence count into its [`SideProjection`], HONEST about
/// unknown ≠ zero: a measured count adds to `incidences`; an UNMEASURED side (`None`) increments
/// `unmeasured_symbols` and adds NOTHING (never a phantom 0). The review-1 honesty fix lives in this match.
fn accumulate_side(side: &mut SideProjection, incidences: Option<usize>) {
    match incidences {
        Some(n) => side.incidences += n,
        None => side.unmeasured_symbols += 1,
    }
}

/// Fixed reader's note embedded in every [`CanonicalEdgeMagnitude`] so a retained artifact self-describes
/// the accounting (why the numbers differ from `projection_incidences`). RECON-M-R1 (§3.7-5): the note now
/// carries the coverage caveat — this block's `*_only` classes BLEND divergence with coverage (an edge
/// whose second witness measured NEITHER projection lands in a `*_only` class), which the ledger's
/// dual-measured rule separates.
const CANONICAL_EDGE_NOTE: &str = "directed edges by canonical identity (caller_key,callee_key), \
multiplicity preserved, each witness's caller+callee projections merged per identity (an edge seen from \
both counts once) — NOT projection_incidences summed; union_edges = scip_only + pipeline_only + shared. \
CAVEAT (RECON-M-R1 §3.6): a *_only edge whose other witness could measure NEITHER projection of the pair \
is a COVERAGE/answerability fact blended in here, NOT evidence the other witness disagrees — read the \
witness_ledger block for the dual-measured-separated classes";

/// The EDGE-LEVEL magnitude from the two witnesses' canonical multisets: per identity, the overlap is
/// `min` (shared), the excess on each side is that side's `*_only`, and `union_edges` sums all three
/// (== |lg ∪ sq| as a multiset). Deterministic. This is the review-2 correction to the pre-review-2 code,
/// which summed caller + callee projection incidences and thereby double-counted mirrored edges.
fn edge_magnitude(
    lg: &BTreeMap<(String, String), usize>,
    sq: &BTreeMap<(String, String), usize>,
) -> CanonicalEdgeMagnitude {
    let mut scip_only = 0usize;
    let mut pipeline_only = 0usize;
    let mut shared = 0usize;
    let ids: BTreeSet<&(String, String)> = lg.keys().chain(sq.keys()).collect();
    for id in ids {
        let l = lg.get(id).copied().unwrap_or(0);
        let s = sq.get(id).copied().unwrap_or(0);
        shared += l.min(s);
        scip_only += l.saturating_sub(s);
        pipeline_only += s.saturating_sub(l);
    }
    CanonicalEdgeMagnitude {
        note: CANONICAL_EDGE_NOTE,
        livegraph_total: lg.values().sum(),
        sqlite_total: sq.values().sum(),
        scip_only,
        pipeline_only,
        shared,
        union_edges: scip_only + pipeline_only + shared,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // The moved primitives' unit tests stayed with this consumer (minimal-diff); they exercise
    // `super::ledger`'s pure pieces through the same names.
    use crate::callgraph_cert::ledger::{classify, LgSide};

    fn p(key: &str, repr: &str) -> (String, String) {
        (key.to_string(), repr.to_string())
    }

    #[test]
    fn classify_equal_multisets_is_empty() {
        let lg = vec![p("k1", "k1|f|a"), p("k2", "k2|g|b")];
        let sq = vec![p("k2", "k2|g|b"), p("k1", "k1|f|a")];
        let d = classify(&lg, &sq);
        assert!(d.is_empty(), "same rows, different order → no divergence");
    }

    #[test]
    fn classify_livegraph_only_is_scip_only_direction() {
        // LG has an edge SQLite lacks → livegraph_only (SCIP-only). Mirrors the drop_calls fixture.
        let lg = vec![p("caller", "caller|f|a")];
        let sq: Vec<(String, String)> = vec![];
        let d = classify(&lg, &sq);
        assert_eq!(d.livegraph_only, vec!["caller".to_string()]);
        assert!(d.sqlite_only.is_empty());
    }

    #[test]
    fn classify_sqlite_only_is_pipeline_only_direction() {
        // SQLite has an edge LG lacks → sqlite_only (pipeline-only). The OTHER direction.
        let lg: Vec<(String, String)> = vec![];
        let sq = vec![p("caller", "caller|f|a")];
        let d = classify(&lg, &sq);
        assert_eq!(d.sqlite_only, vec!["caller".to_string()]);
        assert!(d.livegraph_only.is_empty());
    }

    #[test]
    fn classify_preserves_multiplicity() {
        // A repeated CALLS edge on the LG side only → two livegraph_only entries, not one.
        let lg = vec![p("k1", "k1|f|a"), p("k1", "k1|f|a")];
        let sq = vec![p("k1", "k1|f|a")];
        let d = classify(&lg, &sq);
        assert_eq!(d.livegraph_only, vec!["k1".to_string()]);
        assert!(d.sqlite_only.is_empty() && d.field_mismatch.is_empty());
    }

    #[test]
    fn classify_field_mismatch_on_balanced_key() {
        // Same key both sides, balanced multiplicity, divergent enrichment → field_mismatch (identity
        // present, enrichment differs) — NOT a direction-only edge.
        let lg = vec![p("k1", "k1|f|module=src")];
        let sq = vec![p("k1", "k1|f|module=lib")];
        let d = classify(&lg, &sq);
        assert!(d.livegraph_only.is_empty() && d.sqlite_only.is_empty());
        assert_eq!(d.field_mismatch.len(), 1);
        assert_eq!(d.field_mismatch[0].key, "k1");
    }

    #[test]
    fn lg_side_catches_upstream_panic() {
        // A latent livegraph invariant panic (AstFileScope `Partial` w/o reason, lib.rs:303-306) must be
        // caught — the exhaustive walk survives instead of crashing the daemon. (The caught panic prints
        // one line to stderr; the test still passes.)
        let panicked = lg_side(|| panic!("PartialRequiresReasons"), || "unused".to_string());
        assert!(matches!(panicked, LgSide::Panicked));
        // The non-panicking paths pass through unchanged.
        assert!(matches!(
            lg_side(|| None, || "Unavailable".to_string()),
            LgSide::Unanswerable(_)
        ));
        assert!(matches!(
            lg_side(|| Some(vec![]), || "x".to_string()),
            LgSide::Rows(_)
        ));
    }

    // ── Honesty: unknown ≠ zero (review-1 #1) — the edge count of an UNMEASURED side is UNKNOWN, not 0 ──

    #[test]
    fn unanswerable_livegraph_records_unknown_edges_not_zero() {
        // LG unanswerable, SQLite measured 2 → lg_edges = None (unknown), sq_edges = Some(2). Direction
        // buckets stay empty (can't classify without both sides); the note carries the cause.
        let out = diff_direction(
            LgSide::Unanswerable("livegraph_class=Partial".into()),
            Ok(vec![p("a", "a|f|x"), p("b", "b|g|y")]),
        );
        assert_eq!(
            out.lg_edges, None,
            "unanswerable LG edge count is UNKNOWN, never 0"
        );
        assert_eq!(out.sq_edges, Some(2), "SQLite side WAS measured");
        assert!(out.lg_unanswerable && !out.lg_panicked);
        assert!(
            out.diff.is_empty(),
            "no direction bucket without both sides measured"
        );
    }

    #[test]
    fn panicked_livegraph_records_unknown_edges_not_zero() {
        // LG panicked (the latent finalize_envelope invariant) → lg_edges UNKNOWN, sq_edges measured.
        let out = diff_direction(LgSide::Panicked, Ok(vec![p("a", "a|f|x")]));
        assert_eq!(
            out.lg_edges, None,
            "panicked LG edge count is UNKNOWN, never 0"
        );
        assert_eq!(out.sq_edges, Some(1));
        assert!(out.lg_panicked && out.lg_unanswerable);
        assert_eq!(out.note.as_deref(), Some("livegraph_panic"));
    }

    #[test]
    fn sqlite_error_records_unknown_edges_and_keeps_measured_livegraph() {
        // SQLite errored → sq_edges UNKNOWN; the LiveGraph side is STILL recorded (the pre-review-1 code
        // early-returned and zeroed BOTH — that discarded a measured LG count as a phantom 0).
        let out = diff_direction(LgSide::Rows(vec![p("a", "a|f|x")]), Err("disk gone".into()));
        assert_eq!(out.sq_edges, None, "sqlite read error → UNKNOWN, never 0");
        assert_eq!(
            out.lg_edges,
            Some(1),
            "LG side still measured even when SQLite errors"
        );
        assert!(out.note.as_deref().unwrap().contains("sqlite_error"));
    }

    #[test]
    fn unknown_side_serializes_as_null_not_zero() {
        // The retained artifact must render an unmeasured side as `null`, so a reader can never mistake
        // it for a measured-and-empty `0` (VISION: null = not measured, 0 = measured and absent).
        let c = SideEdgeCounts {
            livegraph: None,
            sqlite: Some(2),
        };
        let v = serde_json::to_value(c).unwrap();
        assert_eq!(v["livegraph"], serde_json::Value::Null, "unknown → null");
        assert_eq!(v["sqlite"], 2, "measured → the number");
    }

    #[test]
    fn side_projection_counts_unmeasured_symbols_never_summing_zero() {
        // The per-direction projection-incidence aggregate: a measured count adds to `incidences`; an
        // unmeasured (None) side bumps `unmeasured_symbols` and adds NOTHING — unknown counted, never 0.
        let mut s = SideProjection::default();
        accumulate_side(&mut s, Some(3));
        accumulate_side(&mut s, None); // unknown — must NOT add 0 to `incidences`
        accumulate_side(&mut s, Some(2));
        assert_eq!(s.incidences, 5, "sum over MEASURED symbols only");
        assert_eq!(
            s.unmeasured_symbols, 1,
            "the unknown symbol is counted, not folded in as 0 incidences"
        );
    }

    // ── Canonical directed edges — merge projections, count each edge ONCE (review-2 #3) ──────────

    #[test]
    fn canonical_edges_merge_projections_dedup_multiplicity_and_unmeasured_not_zero() {
        // A directed edge is witnessed by BOTH projections (its caller's callee-projection AND its
        // callee's caller-projection). The canonical multiset must count it ONCE, preserve repeated-edge
        // multiplicity, and — unknown ≠ zero — still recover an edge whose one projection was UNMEASURED.
        let mut lg = EdgeViews::default();
        // A→B seen from BOTH projections (A.callees=[B] and B.callers=[A]).
        lg.add_callees("A", &["B".into()]);
        lg.add_callers("B", &["A".into()]);
        // A→C with multiplicity 2, seen from BOTH projections at multiplicity 2 each.
        lg.add_callees("A", &["C".into(), "C".into()]);
        lg.add_callers("C", &["A".into(), "A".into()]);
        // D→B recovered from ONLY B's caller-projection: D's callee-projection was UNMEASURED (never added).
        lg.add_callers("B", &["D".into()]);

        let canon = lg.canonical();
        assert_eq!(
            canon.get(&("A".into(), "B".into())),
            Some(&1),
            "edge seen from BOTH projections counts ONCE, not twice"
        );
        assert_eq!(
            canon.get(&("A".into(), "C".into())),
            Some(&2),
            "multiplicity 2 preserved (MAX of the two projections, NOT summed to 4)"
        );
        assert_eq!(
            canon.get(&("D".into(), "B".into())),
            Some(&1),
            "edge recovered from the measured projection; the unmeasured D.callees is not a phantom 0"
        );
        // Naive projection-incidence sum double-counts: caller-view {A→B:1, A→C:2, D→B:1}=4 +
        // callee-view {A→B:1, A→C:2}=3 = 7. Canonical (merged) = 1+2+1 = 4. That 7→4 IS the review-2 fix.
        let incidences: usize = lg.from_caller_projection.values().sum::<usize>()
            + lg.from_callee_projection.values().sum::<usize>();
        let canonical_total: usize = canon.values().sum();
        assert_eq!(incidences, 7, "7 projection incidences");
        assert_eq!(
            canonical_total, 4,
            "4 canonical directed edges (the honest count), NOT 7"
        );
    }

    #[test]
    fn edge_magnitude_classifies_scip_only_pipeline_only_shared_and_union() {
        // lg = {A→B, A→C, D→E}; sq = {A→B, F→G}. shared={A→B}=1, scip_only={A→C,D→E}=2,
        // pipeline_only={F→G}=1, union=4. Proves the class split + the D = sum invariant + the note.
        let mut lg = EdgeViews::default();
        lg.add_callees("A", &["B".into(), "C".into()]);
        lg.add_callees("D", &["E".into()]);
        let mut sq = EdgeViews::default();
        sq.add_callees("A", &["B".into()]);
        sq.add_callees("F", &["G".into()]);
        let m = edge_magnitude(&lg.canonical(), &sq.canonical());
        assert_eq!(m.shared, 1);
        assert_eq!(m.scip_only, 2);
        assert_eq!(m.pipeline_only, 1);
        assert_eq!(m.livegraph_total, 3);
        assert_eq!(m.sqlite_total, 2);
        assert_eq!(
            m.union_edges,
            m.scip_only + m.pipeline_only + m.shared,
            "union_edges = scip_only + pipeline_only + shared (the D invariant)"
        );
        assert_eq!(m.union_edges, 4);
        assert_eq!(
            m.note, CANONICAL_EDGE_NOTE,
            "the artifact self-describes its accounting"
        );
    }

    // ── Fixture-driven collection (real resident LiveGraph + real SQLite mirror) ──────────────────

    use crate::callgraph_cert::{build_and_store_callgraph_cert, callgraph_is_green, test_fixture};

    /// Serializes the tests that mutate the process-global `RMAP_CALLGRAPH_DIFF` env var so they never
    /// race each other. Only [`maybe_emit`] reads this var, and only these serialized tests set it — so
    /// this lock is sufficient to make the env-gated behavior deterministic within the parallel suite.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn collect_on_faithful_mirror_is_green_and_empty() {
        // The faithful mirror: LiveGraph rows == SQLite rows over the corpus → GREEN → zero divergence.
        // This pins the cross-check: divergent_symbol_count == Some(0) ⟺ the cert is GREEN.
        let f = test_fixture::build_fixture(false);
        let green = callgraph_is_green(&f.state, &f.snapshot_uid);
        assert!(green, "faithful mirror is GREEN");
        let report = collect(&f.state, &f.snapshot_uid, "fp-test", green);
        assert_eq!(report.cert_verdict, "GREEN");
        assert!(report.precondition.is_none(), "a real corpus was walked");
        // MEASURED path (`precondition == None`) → every measurement is `Some`, never `null`. Bind once.
        let corpus_size = report.corpus_size.expect("measured path emits corpus_size");
        let ce = report
            .canonical_edges
            .as_ref()
            .expect("measured path emits canonical_edges");
        let symbols = report
            .symbols
            .as_ref()
            .expect("measured path emits symbols");
        assert!(corpus_size >= 2, "callerFn + calleeFn are in the corpus");
        assert_eq!(
            report.divergent_symbol_count,
            Some(0),
            "no divergence on a faithful mirror"
        );
        assert!(symbols.is_empty());
        // CANONICAL EDGES (review-2): the one callerFn→calleeFn edge is present on BOTH witnesses → it is
        // SHARED, counted ONCE (not once per projection, not once per witness). scip_only/pipeline_only 0.
        assert_eq!(ce.shared, 1, "the mirrored edge is shared");
        assert_eq!(ce.scip_only, 0);
        assert_eq!(ce.pipeline_only, 0);
        assert_eq!(ce.livegraph_total, 1);
        assert_eq!(ce.sqlite_total, 1);
        assert_eq!(ce.union_edges, 1, "|lg ∪ sq| = 1 edge");
    }

    #[test]
    fn collect_captures_dropped_calls_edge_as_livegraph_only() {
        // drop_calls: SQLite omits the caller->callee CALLS edge the LiveGraph has. Both directions then
        // surface a livegraph_only (SCIP-only) edge; the cert is RED. This is the "instrumentation on →
        // classifies the mismatch" proof, with the SCIP-only DIRECTION cited by key.
        let f = test_fixture::build_fixture(true);
        let green = callgraph_is_green(&f.state, &f.snapshot_uid);
        assert!(!green, "a dropped CALLS edge → RED");
        let report = collect(&f.state, &f.snapshot_uid, "fp-test", green);
        assert_eq!(report.cert_verdict, "RED");
        assert!(report.precondition.is_none());
        // MEASURED path (`precondition == None`) → every measurement is `Some`, never `null`. Bind once.
        let rollup = report.rollup.as_ref().expect("measured path emits rollup");
        let ce = report
            .canonical_edges
            .as_ref()
            .expect("measured path emits canonical_edges");
        let pi = report
            .projection_incidences
            .as_ref()
            .expect("measured path emits projection_incidences");
        let symbols = report
            .symbols
            .as_ref()
            .expect("measured path emits symbols");
        // calleeFn's caller side AND callerFn's callee side each diverge → exactly 2 divergent symbols.
        assert_eq!(report.divergent_symbol_count, Some(2));
        assert_eq!(rollup.callers_livegraph_only, 1);
        assert_eq!(rollup.callees_livegraph_only, 1);
        assert_eq!(rollup.callers_sqlite_only, 0);
        assert_eq!(rollup.callees_sqlite_only, 0);
        assert_eq!(rollup.livegraph_unanswerable, 0);
        // Cited example: calleeFn's caller bucket names the callerFn key as the SCIP-only edge.
        let callee = test_fixture::callee_key();
        let caller = test_fixture::caller_key();
        let cs = symbols
            .iter()
            .find(|s| s.symbol == callee)
            .expect("calleeFn is divergent");
        assert_eq!(cs.callers.livegraph_only, vec![caller]);
        // Honesty (review-1 #1): EVERY side is answerable on this fixture → zero unmeasured symbols; the
        // projection incidences sum only MEASURED edges (no phantom 0 for an unknown side).
        assert_eq!(pi.livegraph_caller.unmeasured_symbols, 0);
        assert_eq!(pi.sqlite_caller.unmeasured_symbols, 0);
        assert_eq!(pi.livegraph_callee.unmeasured_symbols, 0);
        assert_eq!(pi.sqlite_callee.unmeasured_symbols, 0);
        // CANONICAL EDGES (review-2): the dropped edge callerFn→calleeFn is witnessed by BOTH projections —
        // once as calleeFn's SCIP-only caller (callers_livegraph_only=1) AND once as callerFn's SCIP-only
        // callee (callees_livegraph_only=1). Summed, that is 2 projection incidences; as a CANONICAL
        // directed edge it is ONE edge. The canonical magnitude must count it ONCE, NOT twice.
        assert_eq!(
            rollup.callers_livegraph_only + rollup.callees_livegraph_only,
            2,
            "2 projection incidences (one per direction) for the single dropped edge"
        );
        assert_eq!(
            ce.scip_only, 1,
            "…but ONE canonical directed edge (callerFn→calleeFn), not two"
        );
        assert_eq!(ce.pipeline_only, 0);
        assert_eq!(ce.shared, 0, "SQLite dropped it → not shared");
        assert_eq!(ce.livegraph_total, 1);
        assert_eq!(ce.sqlite_total, 0);
        assert_eq!(
            ce.union_edges, 1,
            "D = scip_only + pipeline_only + shared = 1"
        );
        // calleeFn: LG measures 1 caller (callerFn); SQLite measures 0 (the dropped CALLS edge). BOTH are
        // `Some` (measured-and-present / measured-and-empty), which is the honest opposite of the `None`
        // (unknown) an unanswerable/panicked side would record — the distinction v1 collapsed.
        assert_eq!(cs.caller_edges.livegraph, Some(1));
        assert_eq!(
            cs.caller_edges.sqlite,
            Some(0),
            "measured-empty is Some(0), NOT null"
        );
    }

    #[test]
    fn emit_report_is_byte_identical_across_runs_and_schema_tagged() {
        // Determinism (review-1 #4): two emissions at the SAME graph state must be BYTE-for-byte equal —
        // the ordering is deterministic (BTreeSet corpus + BTreeMap buckets + sorted field-mismatch
        // reprs), so a retained artifact is stable and independently reproducible. Also asserts the ON
        // write path produces a valid, schema-tagged artifact (no process env touched — explicit dirs).
        let f = test_fixture::build_fixture(true);
        let green = callgraph_is_green(&f.state, &f.snapshot_uid);
        // The REAL stored ledger (written by the serve-ladder build above) feeds the v4 block.
        let ledger = f
            .state
            .witness_ledger
            .read()
            .clone()
            .expect("ledger stored by the cert build");
        let d1 = tempfile::tempdir().unwrap();
        let d2 = tempfile::tempdir().unwrap();
        let p1 = emit_report(
            d1.path(),
            &f.state,
            &f.snapshot_uid,
            "fp-test",
            green,
            &ledger,
        )
        .expect("written");
        let p2 = emit_report(
            d2.path(),
            &f.state,
            &f.snapshot_uid,
            "fp-test",
            green,
            &ledger,
        )
        .expect("written");
        let b1 = std::fs::read_to_string(&p1).unwrap();
        let b2 = std::fs::read_to_string(&p2).unwrap();
        assert_eq!(
            b1, b2,
            "two emissions at one graph state are BYTE-identical (deterministic order)"
        );
        let v: serde_json::Value = serde_json::from_str(&b1).expect("valid JSON");
        assert_eq!(v["schema"], SCHEMA);
        assert_eq!(v["cert_verdict"], "RED");
        assert_eq!(v["rollup"]["callers_livegraph_only"], 1);
        assert_eq!(v["witness"]["livegraph"], "scip_fed_livegraph");
        assert_eq!(v["witness"]["sqlite"], "tree_sitter_pipeline_sqlite");
        // v3 (review-2): the serialized artifact carries the canonical edge magnitude (the dropped edge is
        // ONE SCIP-only canonical edge) AND the renamed per-direction projection incidences.
        assert_eq!(v["canonical_edges"]["scip_only"], 1);
        assert_eq!(v["canonical_edges"]["union_edges"], 1);
        assert_eq!(
            v["projection_incidences"]["livegraph_caller"]["incidences"],
            1
        );
    }

    #[test]
    fn degenerate_precondition_serializes_measurements_as_null_never_zero_or_empty() {
        // review-3 #1 — the named SERIALIZED-ARTIFACT test for the degenerate (precondition-failed) path.
        // No resident LiveGraph (the repo-graph self-index situation when the SCIP producer never ran) →
        // the compare never walks a corpus. The emitted artifact must record the `precondition` reason AND
        // render EVERY measurement as `null` (UNKNOWN — no corpus was walked), NEVER a phantom measured
        // `0`/`{}`/`[]` (VISION: unknown ≠ zero — the review-3 blocking defect). The verdict is still RED
        // (the fallback), so this also demonstrates RED-with-`divergent_symbol_count: null` — proving the
        // "RED ⟺ ≥1 divergent symbol" cross-check is correctly scoped to the MEASURED path only.
        let f = test_fixture::build_fixture(false);
        *f.state.livegraph.write() = None;

        // Struct level: `precondition == Some`, and every measurement is `None` (→ serializes `null`).
        let report = collect(&f.state, &f.snapshot_uid, "fp-test", false);
        assert_eq!(
            report.cert_verdict, "RED",
            "no LiveGraph → the RED fallback"
        );
        assert_eq!(
            report.precondition.as_deref(),
            Some("no_resident_livegraph")
        );
        assert!(report.corpus_size.is_none(), "no corpus walked → UNKNOWN");
        assert!(report.divergent_symbol_count.is_none());
        assert!(report.canonical_edges.is_none());
        assert!(report.projection_incidences.is_none());
        assert!(report.rollup.is_none());
        assert!(report.symbols.is_none());

        // Serialized ARTIFACT level: drive the REAL write path (`emit_report`) and read the bytes back —
        // every measurement field must be JSON `null`. `Value::Null` is exactly distinct from the `0`/`{}`/
        // `[]` the pre-review-3 `v3` degenerate path emitted, so this pins the fix at the artifact boundary.
        // The ledger on this path is the DEGENERATE one (no LiveGraph → no walk → unknown, not zero).
        let ledger = WitnessLedger::degenerate("fp-test", &f.snapshot_uid, "no_resident_livegraph");
        let dir = tempfile::tempdir().unwrap();
        let path = emit_report(
            dir.path(),
            &f.state,
            &f.snapshot_uid,
            "fp-test",
            false,
            &ledger,
        )
        .expect("written");
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["precondition"], "no_resident_livegraph");
        assert_eq!(v["cert_verdict"], "RED");
        for field in [
            "corpus_size",
            "divergent_symbol_count",
            "canonical_edges",
            "projection_incidences",
            "rollup",
            "symbols",
        ] {
            assert_eq!(
                v[field],
                serde_json::Value::Null,
                "degenerate `{field}` must serialize as null (unknown), never a measured zero/empty"
            );
        }
    }

    #[test]
    fn gate_env_controls_emission_off_then_on() {
        // review-1 #4: CONTROL `RMAP_CALLGRAPH_DIFF` and prove the gate glue in `maybe_emit` (the one
        // untested seam: `var_os` → route to the named dir). Follows the ratified project convention for
        // env-mutating tests (`platform-paths::socket::override_takes_precedence`): a shared mutex
        // serializes them + save/restore the original value. Edition 2021 → `set_var`/`remove_var` safe.
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original = std::env::var_os(DIFF_ENV);
        std::env::remove_var(DIFF_ENV); // ensure OFF before the cert build (no emission during it)
        let f = test_fixture::build_fixture(true);
        let green = callgraph_is_green(&f.state, &f.snapshot_uid);
        let ledger = f
            .state
            .witness_ledger
            .read()
            .clone()
            .expect("ledger stored by the cert build");
        let dir = tempfile::tempdir().unwrap();
        let artifact = dir.path().join(ARTIFACT_NAME);

        // OFF: var unset → maybe_emit returns after one `var_os` lookup, writes NOTHING.
        maybe_emit(&f.state, &f.snapshot_uid, "fp-test", green, &ledger);
        assert!(!artifact.exists(), "gate OFF (var unset) → no artifact");

        // ON: var = dir → maybe_emit routes the artifact to exactly the named dir (proves it reads the
        // env VALUE, not a hardcoded path).
        std::env::set_var(DIFF_ENV, dir.path());
        maybe_emit(&f.state, &f.snapshot_uid, "fp-test", green, &ledger);
        assert!(
            artifact.exists(),
            "gate ON (var=dir) → artifact written to the named dir"
        );

        restore_env(DIFF_ENV, original);
    }

    #[test]
    fn stored_verdict_unchanged_whether_emission_on_or_off() {
        // review-1 #2 (enabled-gate test, BOTH branches): the STORED verdict must be identical AND correct
        // whether emission runs or not — emission is a read-only side channel, it can never move the
        // verdict. Exercise BOTH branches through the real serve-ladder store: the faithful mirror (→
        // GREEN) and the dropped-CALLS-edge divergence (→ RED). The prior version proved only the RED case.
        assert_stored_verdict_unchanged(false, "GREEN");
        assert_stored_verdict_unchanged(true, "RED");
    }

    /// Build the `drop_calls` fixture TWICE through the real serve-ladder store (`build_and_store_callgraph
    /// _cert`, which calls `maybe_emit`) — once with the gate ON, once OFF — and assert the STORED verdict
    /// is `expected` in BOTH arms (so emission never moved it) and that the ON arm actually emitted the
    /// artifact. Two concrete callers (the GREEN and RED branches); the shared env save/restore + double
    /// build is the non-trivial body that earns the helper over inlining it twice.
    fn assert_stored_verdict_unchanged(drop_calls: bool, expected: &str) {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original = std::env::var_os(DIFF_ENV);
        let dir = tempfile::tempdir().unwrap();

        // Arm 1 — emission ON.
        std::env::set_var(DIFF_ENV, dir.path());
        let f_on = test_fixture::build_fixture(drop_calls);
        let green_on =
            build_and_store_callgraph_cert(&f_on.state, &f_on.snapshot_uid, Some("fp".into()));
        let stored_on = f_on.state.callgraph_cert.read().clone();
        std::env::remove_var(DIFF_ENV);

        // Arm 2 — emission OFF.
        let f_off = test_fixture::build_fixture(drop_calls);
        let green_off =
            build_and_store_callgraph_cert(&f_off.state, &f_off.snapshot_uid, Some("fp".into()));
        let stored_off = f_off.state.callgraph_cert.read().clone();

        restore_env(DIFF_ENV, original);

        // The verdict bit AND the stored string agree with each other AND with the branch's expectation,
        // whether emission was ON or OFF.
        assert_eq!(
            green_on, green_off,
            "{expected}: verdict bit identical on/off"
        );
        assert_eq!(
            green_on,
            Some(expected == "GREEN"),
            "{expected}: verdict bit"
        );
        assert_eq!(
            stored_on.as_ref().map(|c| c.verdict.as_str()),
            Some(expected),
            "{expected}: STORED verdict with emission ON",
        );
        assert_eq!(
            stored_off.as_ref().map(|c| c.verdict.as_str()),
            Some(expected),
            "{expected}: STORED verdict with emission OFF (identical to ON)",
        );
        assert!(
            dir.path().join(ARTIFACT_NAME).exists(),
            "{expected}: the ON arm emitted the artifact (gate exercised, not a no-op)",
        );
    }

    /// Restore an env var to a saved prior value (`Some` → set it back, `None` → remove) — the
    /// save/restore half of the project's env-test convention.
    fn restore_env(key: &str, original: Option<std::ffi::OsString>) {
        match original {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
}
