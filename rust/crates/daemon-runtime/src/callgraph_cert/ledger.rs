//! RECON-M-R1: the WITNESS LEDGER — the callgraph cert's compare, generalized into the full-walk,
//! per-fingerprint witness-agreement classification (recon-design-1 §5.1, ratified §8 / D-R3).
//!
//! **What this is.** Today's cert walks the resident∪SQLite corpus, compares the two call-graph
//! witnesses (P = the tree-sitter pipeline's SQLite rows; S = the SCIP-fed LiveGraph) and reduces
//! everything to ONE bit (GREEN/RED). The ledger is the same walk WITHOUT the reduction: it retains
//! per-canonical-edge INSTANCE witness classes, per-side answerability, the identity guards, and the
//! divergence summary — the data M-R2's union serving and M-R3's read surfaces consume. The stored
//! GREEN/RED verdict is now DERIVED from the ledger (behavior byte-unchanged, [`WitnessLedger::
//! derived_green`]); the ledger itself changes NO served bytes (M-R1 is measurement infrastructure).
//!
//! **Lifecycle** (inherits the cert's exactly): in-memory, keyed by the same
//! `import_cert_fingerprint`, non-durable, lazily rebuilt per fingerprint/restart (D-R8: no
//! persisted family — a persisted rate could misdescribe the current witness pair; recomputation is
//! measured cheap, 1.77 s whole-run at 250-file scale). Any witness movement (snapshot advance,
//! swap, load/unload, `mark_stale`) changes the fingerprint and kills the ledger.
//!
//! **The two layers, kept distinct:**
//!
//!  1. THE COMPARE LAYER ([`CompareSummary`]) — the verdict substrate. The kind-blind full-row
//!     multiset compare the cert has always run (LiveGraph rows vs SQLite rows, both directions,
//!     every corpus symbol), now exhaustive (no first-divergence short-circuit) with the canonical
//!     edge accounting split per §3.6: a `pipeline_only` canonical edge whose second witness never
//!     measured the projection is COVERAGE ([`CanonicalSplit::pipeline_only_unmeasured`]), never
//!     divergence — the defect the spike's `edge_magnitude` note blended (§3.7-5).
//!     `GREEN ⟺ zero divergent symbols ∧ zero unanswerable projections ∧ zero field mismatches`
//!     on the measured path — exactly the equivalence `diff.rs` documents; degenerate paths (no
//!     LiveGraph / no partitions) stay RED with NO measurements (unknown ≠ zero).
//!
//!  2. THE CLASSIFICATION LAYER ([`CallClassification`]) — the witness classes (§3.1/§3.3/§3.6).
//!     KIND-ALIGNED (§5.1 rule c): `both` requires an S strict-`Calls` edge on the pair — a
//!     same-pair `References` edge is a different-kind fact and corroborates no call.
//!     INSTANCE-LEVEL (rule d, R-RAT-5): per dual-measured pair, `min(p, s)` instances classify
//!     `both`; each side's excess lands in its own class under the `multiplicity` sub-class;
//!     every instance lands in exactly ONE class, so the closure and the agreement rate are
//!     instance-exact. DUAL-MEASURED ONLY (rule a, §3.6): a pair neither LiveGraph projection
//!     could measure is `unmeasured`, never a divergence class. SCOPED (rule e, R-RAT-6): the
//!     classification covers ONLY the W-BOTH-ELIGIBLE partition set (resident ∧ `Fresh`;
//!     coverage is data-driven — resident S data IS coverage evidence, so ¬covered ∧ resident is
//!     unrepresentable by derivation) — a stale/non-resident partition contributes NO
//!     classification rows (a stale S beside a current P would mint FALSE divergence describing
//!     our refresh lag, not the reader's code).
//!
//! **The identity guards (§3.5, R-RAT-4):** merging is IDENTITY-SOURCE-CONDITIONAL, never
//! key-bytes-alone. The per-key source discriminant is computed over a key→sources SET (duplicate
//! adoption-compatible sources tolerated — measured: every `AstFileScope` key also appears
//! `AstAdopted`); a `ScipSynthesizedFallback` key that byte-equals ANY pipeline node key — or a
//! fallback-MIXED key (conservative) — is a detected `identity_collision`: its S `Calls` edges are
//! WITHHELD from the classification multiset (the P pair, if any, classifies as if S held no
//! matching pair) and surfaced counted, never merged into `both`. Beside it, the symptom-based
//! `identity_suspect` detector (guard 3): a syntactic-class pair whose (caller key, callee NAME)
//! matches a semantic-class pair under a DIFFERENT callee key.
//!
//! **Walk safety.** The exhaustive walk reaches symbols the short-circuiting verdict walk may never
//! reach, so it catches per-symbol LiveGraph panics ([`lg_side`], the spike's mechanism) and treats
//! them as unanswerable projections — on any graph where the shipped walk completes, the derived
//! verdict is byte-identical; on a graph where the shipped walk would ABORT the daemon (measured
//! incidence 0 since LIVEGRAPH-PARTIAL-FIX-1), the ledger yields RED fail-soft instead. A SQLite
//! read error anywhere aborts the build with `None` (nothing stored — "could not reach a verdict"),
//! preserving today's `None`-only-on-storage-error contract.
//!
//! **Placement** (recorded, least-new-surface): a submodule of `callgraph_cert` — no new crate, no
//! new dependency edge, and NO new reader of the `RepoState` LiveGraph field (the sanctioned
//! `mod.rs` reader locks the field once and passes `&LiveGraph` down, so the EC-M1 reader-set
//! witness is untouched). The shared comparison primitives (`classify`, `diff_direction`, `EdgeViews`, …)
//! moved HERE from `diff.rs` — the spike collector was the ledger's working prototype (§5.1) and
//! now CONSUMES the graduated substrate for its env-gated artifact.

use std::collections::{BTreeMap, BTreeSet};

use repo_graph_agent::{AgentCalleeRow, AgentCallerRow, AgentStorageRead};
use repo_graph_ir::{EdgeType, IdentitySource};
// `AgentStorageRead` supplies `find_symbol_callers`/`callees`; `query_all_nodes` is a
// `StorageConnection` inherent method (the same split the cert build has always used).
use repo_graph_livegraph::LiveGraph;
use repo_graph_trust_model::LanguageSupport;

use super::{lg_callee_rows, lg_caller_rows};

// ═══════════════════════════════════════════════════════════════════════════════════════════
// Shared comparison primitives (moved verbatim from diff.rs — the spike collector now imports
// them from here; behavior unchanged)
// ═══════════════════════════════════════════════════════════════════════════════════════════

/// A per-direction (callers OR callees) classified divergence for one symbol. Multiplicity is
/// preserved (a repeated CALLS edge appears repeatedly), mirroring the cert's un-DISTINCT multiset
/// compare.
#[derive(Debug, Default, serde::Serialize)]
pub(super) struct DirectionDiff {
    /// Stable keys the SCIP-fed LiveGraph has but the pipeline (SQLite) lacks — SCIP-only edges.
    pub(super) livegraph_only: Vec<String>,
    /// Stable keys the pipeline (SQLite) has but the LiveGraph lacks — pipeline-only edges.
    pub(super) sqlite_only: Vec<String>,
    /// Same stable key present on BOTH sides with balanced multiplicity, but the enriched row
    /// (name / file / module) differs — an identity-present, enrichment-divergent edge.
    pub(super) field_mismatch: Vec<FieldMismatch>,
}

impl DirectionDiff {
    pub(super) fn is_empty(&self) -> bool {
        self.livegraph_only.is_empty()
            && self.sqlite_only.is_empty()
            && self.field_mismatch.is_empty()
    }
}

/// A same-key, different-enrichment divergence: the full rendered rows from each side (all
/// fields), so the analysis can see exactly which field diverged.
#[derive(Debug, serde::Serialize)]
pub(super) struct FieldMismatch {
    pub(super) key: String,
    pub(super) livegraph_row: String,
    pub(super) sqlite_row: String,
}

/// The LiveGraph side of one direction, computed PANIC-SAFE by [`lg_side`].
pub(super) enum LgSide {
    /// Exact, enriched rows: `(stable_key, all-fields-rendered)` pairs.
    Rows(Vec<(String, String)>),
    /// The LiveGraph answered non-Exact / un-enrichable — the note carries the class.
    Unanswerable(String),
    /// The LiveGraph answer construction PANICKED (a documented latent invariant — see
    /// `repo-graph-livegraph` `lib.rs:303-306`). Caught so the walk survives; recorded as a class.
    Panicked,
}

/// Compute the LiveGraph side of a direction WITHOUT letting an upstream panic crash the daemon.
///
/// `repo-graph-livegraph::finalize_envelope` had a DOCUMENTED latent panic class (an
/// `AstFileScope`-basis `Partial` with no mapped `DegradationReason`) — fixed by
/// LIVEGRAPH-PARTIAL-FIX-1 (measured `livegraph_panic: 0` over both DATA-UPGRADE exhaustive
/// walks). The catch stays: an EXHAUSTIVE walk reaches symbols the short-circuiting verdict walk
/// may skip, so any UNKNOWN latent panic becomes a recorded unanswerable projection (⇒ RED) rather
/// than a daemon abort. `rows`/`class` are only invoked here, so [`std::panic::AssertUnwindSafe`]
/// is sound (no state escapes a caught unwind; the LiveGraph is read-only behind a shared guard).
pub(super) fn lg_side(
    rows: impl FnOnce() -> Option<Vec<(String, String)>>,
    class: impl FnOnce() -> String,
) -> LgSide {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(rows)) {
        Ok(Some(pairs)) => LgSide::Rows(pairs),
        Ok(None) => {
            let class = std::panic::catch_unwind(std::panic::AssertUnwindSafe(class))
                .unwrap_or_else(|_| "Panicked".to_string());
            LgSide::Unanswerable(format!("livegraph_class={class}"))
        }
        Err(_) => LgSide::Panicked,
    }
}

/// The two PROJECTION views of ONE witness's directed edges, each keyed by canonical identity
/// `(caller_key, callee_key)` with multiplicity. A directed edge is witnessed TWICE — once in its
/// caller's callee-projection, once in its callee's caller-projection — so counting incidences
/// double-counts. [`EdgeViews::canonical`] reduces the two views to ONE multiset by taking the MAX
/// per identity: agreeing projections collapse to their shared count (dedup), and an edge whose
/// one projection was UNMEASURED (that view simply has no entry) is still recovered from the other
/// (unknown ≠ 0). Concrete users: the two witnesses in the ledger walk AND in `diff.rs`'s
/// collector. Axis: projection view vs canonical directed edge. Simpler alternative rejected:
/// summing projection incidences — the review-2 double-count.
#[derive(Debug, Default)]
pub(super) struct EdgeViews {
    /// Edges witnessed via caller-projections: `find_symbol_callers(e)` yields `(c, e)` per caller.
    pub(super) from_caller_projection: BTreeMap<(String, String), usize>,
    /// Edges witnessed via callee-projections: `find_symbol_callees(c)` yields `(c, e)` per callee.
    pub(super) from_callee_projection: BTreeMap<(String, String), usize>,
}

impl EdgeViews {
    /// Record symbol `target`'s MEASURED caller keys — each caller `c` witnesses edge `(c, target)`.
    pub(super) fn add_callers(&mut self, target: &str, callers: &[String]) {
        for c in callers {
            *self
                .from_caller_projection
                .entry((c.clone(), target.to_string()))
                .or_default() += 1;
        }
    }

    /// Record symbol `source`'s MEASURED callee keys — each callee `e` witnesses edge `(source, e)`.
    pub(super) fn add_callees(&mut self, source: &str, callees: &[String]) {
        for e in callees {
            *self
                .from_callee_projection
                .entry((source.to_string(), e.clone()))
                .or_default() += 1;
        }
    }

    /// Reduce the two projection views to ONE canonical directed-edge multiset: per identity, the
    /// MAX of the two projections' counts (dedup an edge seen from both; recover an edge whose one
    /// projection was unmeasured). Deterministic (BTreeMap key order).
    pub(super) fn canonical(&self) -> BTreeMap<(String, String), usize> {
        let mut out = self.from_caller_projection.clone();
        for (id, &n) in &self.from_callee_projection {
            let slot = out.entry(id.clone()).or_default();
            *slot = (*slot).max(n);
        }
        out
    }
}

/// The outcome of classifying one direction for one symbol.
pub(super) struct DirOutcome {
    pub(super) diff: DirectionDiff,
    /// `Some` when the LiveGraph was un-answerable / panicked (LG note) OR the SQLite read errored.
    pub(super) note: Option<String>,
    /// True for LiveGraph non-Exact / un-enrichable / panicked (the `livegraph_unanswerable` class).
    pub(super) lg_unanswerable: bool,
    /// True only when the LiveGraph answer PANICKED (the distinct `livegraph_panic` class).
    pub(super) lg_panicked: bool,
    /// `None` = the LiveGraph side was NOT measurable for this symbol/direction — UNKNOWN, never 0.
    pub(super) lg_edges: Option<usize>,
    /// `None` = the SQLite side was NOT measurable (a read error) — UNKNOWN, never 0.
    pub(super) sq_edges: Option<usize>,
    /// The MEASURED endpoint stable keys on the LiveGraph side (`None` when unmeasured).
    pub(super) lg_keys: Option<Vec<String>>,
    /// The MEASURED endpoint stable keys on the SQLite side (`None` when the read errored).
    pub(super) sq_keys: Option<Vec<String>>,
}

/// Classify one direction from the panic-safe [`LgSide`] and the SQLite side. Each side is
/// measured INDEPENDENTLY, so an unmeasured side is `None` (unknown), never 0. Direction buckets
/// are populated ONLY when BOTH sides are measured; otherwise they stay empty and the note carries
/// the cause, so an unknown side is NEVER mislabeled as scip-only / pipeline-only.
pub(super) fn diff_direction(
    lg: LgSide,
    sq_pairs: Result<Vec<(String, String)>, String>,
) -> DirOutcome {
    let (sq_rows, sq_note): (Option<Vec<(String, String)>>, Option<String>) = match sq_pairs {
        Ok(v) => (Some(v), None),
        Err(e) => (None, Some(format!("sqlite_error: {e}"))),
    };
    let lg_unanswerable = !matches!(lg, LgSide::Rows(_));
    let lg_panicked = matches!(lg, LgSide::Panicked);
    let (lg_rows, lg_note): (Option<Vec<(String, String)>>, Option<String>) = match lg {
        LgSide::Rows(pairs) => (Some(pairs), None),
        LgSide::Unanswerable(note) => (None, Some(note)),
        LgSide::Panicked => (None, Some("livegraph_panic".to_string())),
    };
    let diff = match (&lg_rows, &sq_rows) {
        (Some(l), Some(s)) => classify(l, s),
        _ => DirectionDiff::default(),
    };
    let note = match (lg_note, sq_note) {
        (Some(l), Some(s)) => Some(format!("{l}; {s}")),
        (l, s) => l.or(s),
    };
    let keys_of = |rows: &Option<Vec<(String, String)>>| {
        rows.as_ref()
            .map(|r| r.iter().map(|(k, _)| k.clone()).collect::<Vec<String>>())
    };
    DirOutcome {
        lg_edges: lg_rows.as_ref().map(Vec::len),
        sq_edges: sq_rows.as_ref().map(Vec::len),
        lg_keys: keys_of(&lg_rows),
        sq_keys: keys_of(&sq_rows),
        diff,
        note,
        lg_unanswerable,
        lg_panicked,
    }
}

/// PURE classification of two `(stable_key, rendered_row)` lists into the divergence buckets.
/// Deterministic (BTreeMap-ordered); multiplicity preserved via signed per-key counts. Buckets are
/// ALL empty iff the two full-row multisets are equal — i.e. iff the cert's `*_multiset_eq` would
/// return true — so per-symbol divergence agrees with the verdict by construction.
pub(super) fn classify(lg: &[(String, String)], sq: &[(String, String)]) -> DirectionDiff {
    let mut counts: BTreeMap<&str, i64> = BTreeMap::new();
    let mut lg_reprs: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut sq_reprs: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (k, r) in lg {
        *counts.entry(k).or_default() += 1;
        lg_reprs.entry(k).or_default().push(r);
    }
    for (k, r) in sq {
        *counts.entry(k).or_default() -= 1;
        sq_reprs.entry(k).or_default().push(r);
    }

    let mut diff = DirectionDiff::default();
    for (k, c) in &counts {
        match (*c).cmp(&0) {
            std::cmp::Ordering::Greater => {
                for _ in 0..*c {
                    diff.livegraph_only.push((*k).to_string());
                }
            }
            std::cmp::Ordering::Less => {
                for _ in 0..(-*c) {
                    diff.sqlite_only.push((*k).to_string());
                }
            }
            std::cmp::Ordering::Equal => {
                let mut a: Vec<&str> = lg_reprs.get(k).cloned().unwrap_or_default();
                let mut b: Vec<&str> = sq_reprs.get(k).cloned().unwrap_or_default();
                a.sort_unstable();
                b.sort_unstable();
                if a != b {
                    diff.field_mismatch.push(FieldMismatch {
                        key: (*k).to_string(),
                        livegraph_row: a.join(" ;; "),
                        sqlite_row: b.join(" ;; "),
                    });
                }
            }
        }
    }
    diff
}

/// `(stable_key, all-fields-rendered)` for a caller row — the key drives the DIRECTION diff, the
/// full render drives field-mismatch detection + display.
pub(super) fn caller_pairs(rows: &[AgentCallerRow]) -> Vec<(String, String)> {
    rows.iter()
        .map(|r| {
            (
                r.stable_key.clone(),
                format!(
                    "{}|{}|{:?}|{:?}|{:?}",
                    r.stable_key, r.name, r.file, r.module_path, r.module_stable_key
                ),
            )
        })
        .collect()
}

pub(super) fn callee_pairs(rows: &[AgentCalleeRow]) -> Vec<(String, String)> {
    rows.iter()
        .map(|r| {
            (
                r.stable_key.clone(),
                format!(
                    "{}|{}|{:?}|{:?}|{:?}",
                    r.stable_key, r.name, r.file, r.module_path, r.module_stable_key
                ),
            )
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// The ledger data model
// ═══════════════════════════════════════════════════════════════════════════════════════════

/// A symbol×direction projection axis (per-side answerability, §3.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Direction {
    /// The `callers(symbol)` projection.
    Callers,
    /// The `callees(symbol)` projection.
    Callees,
}

/// RECON-M-R2 (the §4.2 transient-2 record): a witness-ledger BUILD FAILURE, retained on
/// `RepoState.witness_ledger_build_failure` so doctor CAN report "ledger absent + build-failure
/// reason" (rendering is M-R3a's; this is the substance). The M-R1 build contract returns `None`
/// ONLY on a SQLite error during the walk, so the reason is that CLASS — the per-site error detail
/// is discarded by the walk's `.ok()?` sites and deliberately NOT threaded out here (the class
/// fully determines today's failure taxonomy; finer detail is M-R3a's call if rendering needs it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerBuildFailure {
    /// The fingerprint the failed build was keyed at (what the capture was trying to warm).
    pub fingerprint: String,
    /// The failure class (today always the SQLite-error-during-walk class).
    pub reason: String,
}

/// The witness ledger for ONE `(snapshot_uid, livegraph_fingerprint)` witness pair. In-memory,
/// non-durable, fingerprint-keyed (the cert lifecycle). See the module doc for the two layers.
#[derive(Debug, Clone, PartialEq)]
pub struct WitnessLedger {
    /// The fingerprint this ledger was computed at (the invalidation key — same
    /// `import_cert_fingerprint` the cert uses).
    pub fingerprint: String,
    /// The pinned SQLite snapshot the P witness was read at.
    pub snapshot_uid: String,
    /// `Some(reason)` when the compare never reached a corpus (no resident LiveGraph / no resident
    /// partitions). Every measurement below is then `None` — UNKNOWN, never a measured zero — and
    /// the derived verdict is RED (today's exact degenerate behavior).
    pub precondition: Option<String>,
    /// The compare layer (verdict substrate) — `Some` iff a corpus was walked.
    pub compare: Option<CompareSummary>,
    /// The kind-aligned instance classification — `Some` iff a corpus was walked.
    pub classification: Option<CallClassification>,
}

impl WitnessLedger {
    /// A ledger for a compare that never reached a corpus. Verdict RED, measurements unknown.
    pub fn degenerate(fingerprint: &str, snapshot_uid: &str, reason: &str) -> Self {
        Self {
            fingerprint: fingerprint.to_string(),
            snapshot_uid: snapshot_uid.to_string(),
            precondition: Some(reason.to_string()),
            compare: None,
            classification: None,
        }
    }

    /// The DERIVED GREEN/RED verdict (recon-design-1 §5.1 transition compatibility):
    /// `GREEN ⟺ zero divergent symbols ∧ zero unanswerable projections ∧ zero field mismatches`
    /// on the measured path; degenerate paths are RED (never walked a corpus → cannot prove
    /// no-loss). Byte-compatible with `callgraph_compare_is_exact`'s verdict on every path that
    /// completes: a divergent symbol, an unanswerable projection and a field mismatch are exactly
    /// the three conditions that forced `Some(false)` (the first two via non-Exact/unenrichable
    /// answers or multiset inequality; the third via the full-row multiset compare).
    pub fn derived_green(&self) -> bool {
        match &self.compare {
            None => false,
            Some(c) => {
                c.divergent_symbols == 0
                    && c.unanswerable_projections == 0
                    && c.field_mismatches == 0
            }
        }
    }
}

/// The compare layer: the kind-blind full-row multiset compare, exhaustive, with the §3.6
/// coverage/divergence split on the canonical accounting.
#[derive(Debug, Clone, PartialEq)]
pub struct CompareSummary {
    /// Corpus size (LiveGraph AST-adopted keys ∪ SQLite SYMBOL keys — the cert's corpus).
    pub corpus_size: usize,
    /// Symbols with ANY divergence (non-empty direction bucket or an unanswerable note) — the
    /// cert-equivalence count (`RED ⟺ ≥ 1` on the measured path).
    pub divergent_symbols: usize,
    /// (symbol, direction) projections where the LiveGraph produced no Exact, enrichable answer
    /// (includes panicked ones). Population: symbol×direction, NEVER mixed with edge counts.
    pub unanswerable_projections: usize,
    /// The subset of unanswerable projections that PANICKED (caught; a latent-invariant signal).
    pub livegraph_panics: usize,
    /// Same-key, balanced-multiplicity, enrichment-divergent edges (both directions summed).
    pub field_mismatches: usize,
    /// The canonical (kind-blind) edge accounting with the §3.6 coverage split.
    pub canonical: CanonicalSplit,
    /// The UNMEASURED projections themselves (per-side answerability, §3.6) — the dual-measured
    /// rule's substrate: a witness class exists only where BOTH witnesses measured the projection.
    pub unmeasured_projections: BTreeSet<(String, Direction)>,
}

/// The kind-blind canonical directed-edge magnitude (diff.rs's `CanonicalEdgeMagnitude`
/// accounting) with `pipeline_only` SPLIT by dual-measurability (§3.6 / §3.7-5): an edge whose
/// second witness never measured either projection is a COVERAGE fact, not divergence. (The
/// `scip_only` class needs no split: an LG canonical edge exists only via a MEASURED LiveGraph
/// projection, and the SQLite side of this walk is always measured — a read error aborts the
/// build.) Invariant: `scip_only + pipeline_only_dual_measured + pipeline_only_unmeasured +
/// shared == union_edges`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CanonicalSplit {
    /// Canonical directed edge instances the SCIP-fed LiveGraph witnesses (all kinds — the
    /// kind-blind traversal, §3.7-1; the kind-partitioned call corpus lives in the
    /// classification layer).
    pub livegraph_total: usize,
    /// Canonical directed edge instances the pipeline witnesses (CALLS — SQLite's query filter).
    pub sqlite_total: usize,
    /// Canonical edges on the LiveGraph but not the pipeline.
    pub scip_only: usize,
    /// Pipeline-only canonical edges whose pair at least one LiveGraph projection MEASURED —
    /// true dual-measured divergence.
    pub pipeline_only_dual_measured: usize,
    /// Pipeline-only canonical edges NEITHER LiveGraph projection could measure — coverage /
    /// answerability, never divergence (zap-engine: 1,556 of 1,585 — the 98.2% lesson).
    pub pipeline_only_unmeasured: usize,
    /// Canonical edges present on both witnesses (multiset intersection).
    pub shared: usize,
    /// The union denominator (multiset union of both witnesses).
    pub union_edges: usize,
}

/// The kind-aligned, instance-level call classification over the W-BOTH-eligible partition set.
///
/// Population discipline (every count labeled by its unit): `*_instances`/plain counts are
/// directed canonical edge INSTANCES (multiplicity preserved); `*_identities` are distinct
/// `(caller_key, callee_key)` pairs; `s_kind_totals` are physical eligible-IR edge instances;
/// `fallback_key_count`/`colliding_keys` are KEY populations. Closures (instance-exact, §5.4):
/// `pipeline_calls == both + syntactic.total() + unmeasured_edges` and
/// `union_calls == both + syntactic.total() + semantic.total() + unmeasured_edges`.
#[derive(Debug, Clone, PartialEq)]
pub struct CallClassification {
    /// The W-BOTH-ELIGIBLE partitions this classification is scoped to (resident ∧ `Fresh`),
    /// with their languages. A partition outside this set contributed NO classification rows.
    pub eligible: BTreeMap<String, LanguageSupport>,
    /// P's call-instance total (the pipeline CALLS multiset over the corpus).
    pub pipeline_calls: usize,
    /// The union call-graph instance total: `pipeline_calls + semantic.total()` (every S-only
    /// call instance joins — S pairs are dual-measured by construction, see `unmeasured_edges`).
    pub union_calls: usize,
    /// Instances on dual-measured pairs: `both + syntactic.total()` — the agreement denominator.
    pub dual_measured: usize,
    /// Corroborated instances: per dual-measured pair, `min(p, s_strict_calls)` (kind-aligned).
    pub both: usize,
    /// Distinct pairs contributing ≥ 1 `both` instance.
    pub both_identities: usize,
    /// Pipeline-only dual-measured instances by mechanical sub-class.
    pub syntactic: SyntacticSplit,
    /// Compiler-only measured call instances by mechanical sub-class.
    pub semantic: SemanticSplit,
    /// P instances on pairs NEITHER LiveGraph projection measured — coverage, excluded from the
    /// agreement rate's denominator, shown beside it (unknown ≠ zero).
    ///
    /// The DUAL-MEASURED predicate is per SIDE, testing the OTHER witness's measurement channel
    /// (§3.6 — "the second witness unanswerable HERE"): a P pair is dual-measured iff at least
    /// one LiveGraph projection of it was answerable (S could look); an S pair is dual-measured
    /// BY CONSTRUCTION in a completed walk — the strict ingest guarantees every S `Calls` caller
    /// is an AST-adopted corpus symbol, so P's SQLite projection of the pair always ran (P
    /// measured and holds fewer/no such calls — exactly the `semantic` claim; a SQLite error
    /// aborts the whole build instead). Testing S pairs by LG-answer answerability would test the
    /// WRONG witness: an unanswerable LG ANSWER (e.g. a fallback-identity endpoint degrading the
    /// envelope to `Partial`) is our envelope honesty, not P failing to measure.
    pub unmeasured_edges: usize,
    /// Distinct unmeasured P pairs.
    pub unmeasured_identities: usize,
    /// Physical eligible-IR edge totals by kind (`Calls` / `References` / `Imports`) — includes
    /// any collision-withheld edges (a different population than the classification multiset;
    /// withheld instances are counted beside, never silently dropped).
    pub s_kind_totals: SKindTotals,
    /// Distinct `ScipSynthesizedFallback` keys across the eligible IRs (the guard-predicate
    /// population — amodx baseline: 280).
    pub fallback_key_count: usize,
    /// S strict-`Calls` instances WITHHELD from the classification because an endpoint key is a
    /// detected identity collision (R-RAT-4 guard 2). The affected P pair classifies as if S held
    /// no matching pair.
    pub identity_collision: usize,
    /// The colliding KEYS per eligible partition (a KEY population — doctor's food, M-R3a).
    pub colliding_keys: BTreeMap<String, BTreeSet<String>>,
    /// The withheld S pairs (M-R2's "collision-withheld pairs never serve" input).
    pub withheld_pairs: BTreeSet<(String, String)>,
    /// Syntactic-class pairs whose (caller key, callee NAME) matches a semantic-class pair under
    /// a DIFFERENT callee key — the wrong/missed-adoption symptom signature (§3.5 guard 3).
    pub identity_suspect: usize,
    /// Per-pair records for every union-call / P pair (the M-R2 serving substrate): exact
    /// `(p, s)` multiplicities, dual-measurability, and the pair-level syntactic sub-class.
    pub pairs: BTreeMap<(String, String), PairRecord>,
    /// Every occurrence-delta pair (a corroborated pair with excess on either side), with its
    /// exact `(p, s)` — doctor's enumeration (§5.4; measured today: none at amodx scale).
    pub delta_pairs: Vec<DeltaPair>,
    /// Per language×partition rollups (§5.1 rule b). S-witnessed facts only — classes defined by
    /// the ABSENCE of an S edge (syntactic pair-level, unmeasured) have no honest per-partition
    /// attribution and live ONLY in the global fields above (recorded decision).
    pub rollups: BTreeMap<(LanguageSupport, String), PartitionRollup>,
}

impl CallClassification {
    /// `100 × both / dual_measured` — PERCENTAGE POINTS, matching the `_pct` name and every
    /// ratified gate figure (amodx `494/507 = 97.4…`, the P=2/S=1 instance fixture `50`), never
    /// a 0–1 ratio (review-0 defect: the emitted `0.974…` contradicted both). Left-associative
    /// `100.0 * both` is integer-exact in f64 at any real corpus scale, so the value is the
    /// single-rounding quotient. `None` when nothing was dual-measured (unknown, never 0%).
    pub fn agreement_pct(&self) -> Option<f64> {
        (self.dual_measured > 0).then(|| 100.0 * self.both as f64 / self.dual_measured as f64)
    }
}

/// Pipeline-only dual-measured instances by mechanical sub-class (§3.1). The sub-classes describe
/// S's CORROBORATION STRUCTURE, never P's correctness.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SyntacticSplit {
    /// Caller and callee lie in different compiler runs (both endpoints known to S, partition
    /// sets disjoint) — S structurally could not corroborate THIS edge regardless of P's
    /// correctness.
    pub boundary: usize,
    /// The caller is the file/module node (P models module-init execution as a CALLS row; the
    /// strict ingest BY DESIGN never emits `Calls` for a file-scope caller).
    pub file_scope: usize,
    /// Within one compiler run's scope, callable caller — S measured and holds no such call.
    /// Also the honest bucket when an endpoint is absent from S's node inventory entirely (no
    /// two-compiler-runs story exists for it).
    pub uncorroborated: usize,
    /// P-excess occurrences on a corroborated pair (`p > s ≥ 1`) — the pair is corroborated, the
    /// extra occurrences are not (R-RAT-5).
    pub multiplicity: usize,
    /// Distinct pairs contributing ≥ 1 syntactic-class instance.
    pub identities: usize,
}

impl SyntacticSplit {
    /// Total syntactic-class instances.
    pub fn total(&self) -> usize {
        self.boundary + self.file_scope + self.uncorroborated + self.multiplicity
    }
}

/// Compiler-only measured call instances by mechanical sub-class (§3.1/§3.3).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SemanticSplit {
    /// Instances on pairs P holds no call edge of (`p == 0`) — the rows §3.4-1 admits as NEW
    /// union members at M-R2.
    pub new_pair: usize,
    /// S-excess occurrences on a corroborated pair (`s > p ≥ 1`).
    pub multiplicity: usize,
    /// Distinct pairs contributing ≥ 1 semantic-class instance.
    pub identities: usize,
}

impl SemanticSplit {
    /// Total semantic-class instances.
    pub fn total(&self) -> usize {
        self.new_pair + self.multiplicity
    }
}

/// Physical eligible-IR edge totals by kind.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SKindTotals {
    /// `EdgeType::Calls` (basis `SyntaxConfirmedCall`) instances.
    pub calls: usize,
    /// `EdgeType::References` instances.
    pub references: usize,
    /// `EdgeType::Imports` instances (neither call classification nor reference tier).
    pub imports: usize,
}

/// One union-call pair's exact record — the M-R2 serving substrate: every per-instance class
/// derives mechanically from `(p, s_calls, dual_measured, syntactic_subclass)` via the §3.3
/// instance rule (min corroborated, excess per side).
#[derive(Debug, Clone, PartialEq)]
pub struct PairRecord {
    /// P's occurrence count for this pair (0 for an S-only pair).
    pub p: usize,
    /// S's strict-`Calls` occurrence count (collision-withheld edges excluded; 0 for a P-only
    /// pair).
    pub s_calls: usize,
    /// Whether at least one LiveGraph projection measured this pair (§3.6). `false` ⇒ the P
    /// instances are `unmeasured`, and NO witness class exists for the pair.
    pub dual_measured: bool,
    /// The pair-level syntactic sub-class, `Some` iff `dual_measured ∧ p > 0 ∧ s_calls == 0`.
    pub syntactic_subclass: Option<PairSubclass>,
}

/// The pair-level syntactic sub-class (`s_calls == 0` on a dual-measured pair with P instances).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairSubclass {
    /// Different compiler runs (partition sets disjoint, both endpoints in S).
    Boundary,
    /// File/module-scope caller (module-init call).
    FileScope,
    /// Same compiler run (or endpoint unknown to S) — S measured, holds no such call.
    Uncorroborated,
}

/// An occurrence-delta pair with its exact multiplicities (doctor's enumeration).
#[derive(Debug, Clone, PartialEq)]
pub struct DeltaPair {
    /// Caller canonical key.
    pub caller: String,
    /// Callee canonical key.
    pub callee: String,
    /// P occurrence count.
    pub p: usize,
    /// S strict-`Calls` occurrence count.
    pub s_calls: usize,
}

/// Per language×partition rollup — the mechanically per-partition-attributable facts. Class
/// instances here are attributed by the PARTITION OF THE WITNESSING S EDGE (each IR edge instance
/// lives in exactly one partition's IR; for a pair whose S edges span partitions, the corroborated
/// `min` fills in partition-id order, then the excess — deterministic).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PartitionRollup {
    /// Physical strict-`Calls` edge instances in this partition's IR.
    pub s_calls: usize,
    /// Physical `References` edge instances.
    pub s_references: usize,
    /// Physical `Imports` edge instances.
    pub s_imports: usize,
    /// Nodes adopted from the pipeline AST (`IdentitySource::AstAdopted`).
    pub adoption_adopted: usize,
    /// Fallback-minted nodes (`ScipSynthesizedFallback`) — adoption coverage is a per-producer,
    /// per-partition fact (§3.5 guard 3).
    pub adoption_fallback: usize,
    /// File/module-scope nodes (`AstFileScope`).
    pub adoption_file_scope: usize,
    /// `both` instances witnessed by this partition's S edges.
    pub both_instances: usize,
    /// `semantic`/`new_pair` instances witnessed by this partition's S edges.
    pub semantic_new_pair: usize,
    /// `semantic`/`multiplicity` instances witnessed by this partition's S edges.
    pub semantic_multiplicity: usize,
    /// S `Calls` instances withheld by the collision guard in this partition.
    pub withheld_instances: usize,
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// Coverage regimes + request activation (§4.2, R-RAT-6 + the iteration-6 split)
// ═══════════════════════════════════════════════════════════════════════════════════════════

/// The W-ONE reason ladder (deterministic — residency splits the space first, then producer
/// presence; each actual state maps to exactly one reason).
// M-R1 consumers: the §4.2 exhaustive-matrix tests (the gate demands them); the production
// consumer is M-R3a's reason-specific posture rendering (trust/doctor) — not built here (M-R1
// changes no served bytes), hence the non-test allow.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WOneReason {
    /// Resident, status ≠ `Fresh` (staleness is a property OF resident data).
    Stale,
    /// No resident partition data; producer provisioned.
    NotResident,
    /// No resident partition data AND the producer is not provisioned.
    ProducerUnavailable,
}

/// The pin-state axis of ONE request's activation (only meaningful inside a W-BOTH-eligible
/// partition set — a pin cannot exist elsewhere).
#[cfg_attr(not(test), allow(dead_code))] // M-R1: matrix tests; production consumer = M-R2 capture flip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinState {
    /// The captured fingerprint matches the resident fingerprint at data read.
    Match,
    /// A pin was captured but the fingerprint moved before the data read (any witness movement).
    Moved,
    /// No pin was captured (the warm/capture step produced no valid ledger fingerprint).
    NoPin,
}

/// The exhaustive classification of one language×partition state × one request's pin state
/// (the §4.2 matrix). THREE mutually-exclusive partition-level REGIMES (W-BOTH / W-ONE / W-NONE);
/// the two transient states are request-scoped fail-softs INSIDE the W-BOTH regime — never
/// regimes, never W-ONE reasons (the type makes the distinction structural: [`WOneReason`] has
/// exactly three variants).
#[cfg_attr(not(test), allow(dead_code))]
// M-R1: matrix tests; production consumers = M-R2/M-R3a.
// The `W-*` prefixes are the OPERATOR-RATIFIED regime vocabulary (R-RAT-6: W-BOTH / W-ONE /
// W-NONE — the names grade the SECOND witness); the lint yields to the ratified names.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateClass {
    /// No shipped producer for the language — pipeline serves, capability truth on doctor (R-0).
    WNone,
    /// Covered but not eligible — pipeline serves byte-identically; the reason renders with its
    /// concrete next action. `refresh_blocked_producer_absent` is the measured warm-cache
    /// stale∧producer-absent COMPOUND: one reason (`Stale`) + a named blocker on the next action,
    /// never a fourth state (only ever `true` beside `Stale`).
    WOne {
        /// The single deterministic reason.
        reason: WOneReason,
        /// The stale∧producer-absent compound's named blocker.
        refresh_blocked_producer_absent: bool,
    },
    /// W-BOTH regime, activation held (pin captured + matching) — the union may serve (M-R2).
    WBothActivated,
    /// W-BOTH regime, transient fail-soft 1: the pin moved mid-request — THIS answer serves
    /// pipeline at the pinned snapshot with no witness fields; self-heals at the next capture.
    WBothTransientPinMoved,
    /// W-BOTH regime, transient fail-soft 2: capture failed (no valid ledger at the current
    /// fingerprint) — pipeline serve; the failure is OUR operational fact (doctor), never a
    /// per-edge or regime label.
    WBothTransientCaptureFailed,
}

/// Classify one language×partition state (+ the request's pin state) into the §4.2 matrix —
/// exhaustive over every representable cell, mutually exclusive by construction.
///
/// Inputs mirror the matrix axes. `covered` = a shipped producer exists for the language;
/// residency OVERRIDES it (`covered || resident`): coverage is DATA-DRIVEN (METRIC-LANG-COVERAGE
/// §2A) — resident S data for a language IS coverage evidence, so the matrix's
/// `¬covered ∧ resident` cell is unrepresentable BY DERIVATION, not by panic. `fresh` is a
/// property of resident data (ignored when `¬resident` — nothing exists to be stale).
/// `producer_provisioned` is deliberately NOT an eligibility conjunct (Fresh resident data
/// corroborates regardless — the producer gates the NEXT refresh, so it enters only the W-ONE
/// ladder and the stale compound's blocker). `pin` only decides among the W-BOTH cells.
#[cfg_attr(not(test), allow(dead_code))] // M-R1: matrix tests; production consumers = M-R2/M-R3a.
pub fn classify_state(
    covered: bool,
    resident: bool,
    fresh: bool,
    producer_provisioned: bool,
    pin: PinState,
) -> StateClass {
    // Coverage is data-driven: resident S data is itself coverage evidence.
    let covered = covered || resident;
    if !covered {
        return StateClass::WNone;
    }
    if !resident {
        let reason = if producer_provisioned {
            WOneReason::NotResident
        } else {
            WOneReason::ProducerUnavailable
        };
        return StateClass::WOne {
            reason,
            refresh_blocked_producer_absent: false,
        };
    }
    if !fresh {
        // The eligibility predicate fails on freshness — the witness-honesty invariant: a stale S
        // beside a current P would mint FALSE divergence describing OUR refresh lag.
        return StateClass::WOne {
            reason: WOneReason::Stale,
            refresh_blocked_producer_absent: !producer_provisioned,
        };
    }
    // covered ∧ resident ∧ Fresh — the W-BOTH ELIGIBILITY holds; the pin decides ACTIVATION.
    match pin {
        PinState::Match => StateClass::WBothActivated,
        PinState::Moved => StateClass::WBothTransientPinMoved,
        PinState::NoPin => StateClass::WBothTransientCaptureFailed,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════════════════
// The build
// ═══════════════════════════════════════════════════════════════════════════════════════════

/// Per-key identity-source presence, computed over a key→sources SET across the eligible IRs
/// (never assume key uniqueness — the measured adoption-compatible duplicates: every
/// `AstFileScope` key also appears `AstAdopted`).
#[derive(Debug, Default, Clone, Copy)]
struct SourceSet {
    adopted: bool,
    fallback: bool,
    file_scope: bool,
}

impl SourceSet {
    fn add(&mut self, s: IdentitySource) {
        match s {
            IdentitySource::AstAdopted => self.adopted = true,
            IdentitySource::ScipSynthesizedFallback => self.fallback = true,
            IdentitySource::AstFileScope => self.file_scope = true,
        }
    }
    /// A fallback-MIXED key (fallback + any adoption-compatible source) — treated as COLLIDING
    /// (conservative: an adoption-compatible source means the same key byte-equals a
    /// pipeline-derived key by construction).
    fn fallback_mixed(&self) -> bool {
        self.fallback && (self.adopted || self.file_scope)
    }
}

/// Build the witness ledger over the resident LiveGraph + the pinned SQLite snapshot. Returns
/// `None` ONLY on a SQLite error (nothing stored — "could not reach a verdict", today's exact
/// `None` contract). The DEGENERATE cases (no LiveGraph / no partitions) are the CALLER's to
/// construct via [`WitnessLedger::degenerate`] — they need no walk.
///
/// The caller (`build_and_store_callgraph_cert`) holds the ONE LiveGraph read guard across this
/// build, so the walk and the fingerprint it is keyed by cannot straddle a swap (the W-A serial
/// coordinator already excludes one; the discipline mirrors build-then-peek).
pub(super) fn build_witness_ledger(
    lg: &LiveGraph,
    storage: &repo_graph_storage::StorageConnection,
    snapshot_uid: &str,
    fingerprint: &str,
) -> Option<WitnessLedger> {
    // ── The corpus + the P-side identity surface (the cert's exact reads) ──
    let sqlite_nodes = storage.query_all_nodes(snapshot_uid).ok()?;
    let mut corpus: BTreeSet<String> = lg.focus_corpus().symbol_keys.into_iter().collect();
    // P's COMPLETE node-key set (ALL kinds — the collision-guard predicate's population) + the
    // per-key kind/name maps (file_scope detection; suspect-detector names).
    let mut p_keys: BTreeSet<String> = BTreeSet::new();
    let mut p_kind: BTreeMap<String, String> = BTreeMap::new();
    let mut p_name: BTreeMap<String, String> = BTreeMap::new();
    for n in &sqlite_nodes {
        p_keys.insert(n.stable_key.clone());
        p_kind.insert(n.stable_key.clone(), n.kind.clone());
        p_name.insert(n.stable_key.clone(), n.name.clone());
        if n.kind.as_str() == "SYMBOL" {
            corpus.insert(n.stable_key.clone());
        }
    }

    // ── The exhaustive walk (compare layer) ──
    let mut divergent_symbols = 0usize;
    let mut unanswerable = 0usize;
    let mut panics = 0usize;
    let mut field_mismatches = 0usize;
    let mut unmeasured_projections: BTreeSet<(String, Direction)> = BTreeSet::new();
    let mut lg_views = EdgeViews::default();
    let mut sq_views = EdgeViews::default();

    for key in &corpus {
        let callers = diff_direction(
            lg_side(
                || lg_caller_rows(lg, key).map(|r| caller_pairs(&r)),
                || {
                    format!(
                        "{:?}",
                        lg.callers(key, repo_graph_trust_model::Granularity::CallerDetail)
                            .class()
                    )
                },
            ),
            // A SQLite read error aborts the build (None — could not reach a verdict; parity
            // with the cert's `.ok()?`). The `Ok` wrap keeps the shared `diff_direction` seam.
            Ok(storage
                .find_symbol_callers(snapshot_uid, key)
                .map(|r| caller_pairs(&r))
                .ok()?),
        );
        let callees = diff_direction(
            lg_side(
                || lg_callee_rows(lg, key).map(|r| callee_pairs(&r)),
                || {
                    format!(
                        "{:?}",
                        lg.callees(key, repo_graph_trust_model::Granularity::CallerDetail)
                            .class()
                    )
                },
            ),
            Ok(storage
                .find_symbol_callees(snapshot_uid, key)
                .map(|r| callee_pairs(&r))
                .ok()?),
        );

        if let Some(ks) = &callers.lg_keys {
            lg_views.add_callers(key, ks);
        } else {
            unmeasured_projections.insert((key.clone(), Direction::Callers));
        }
        if let Some(ks) = &callees.lg_keys {
            lg_views.add_callees(key, ks);
        } else {
            unmeasured_projections.insert((key.clone(), Direction::Callees));
        }
        if let Some(ks) = &callers.sq_keys {
            sq_views.add_callers(key, ks);
        }
        if let Some(ks) = &callees.sq_keys {
            sq_views.add_callees(key, ks);
        }

        unanswerable += usize::from(callers.lg_unanswerable) + usize::from(callees.lg_unanswerable);
        panics += usize::from(callers.lg_panicked) + usize::from(callees.lg_panicked);
        field_mismatches += callers.diff.field_mismatch.len() + callees.diff.field_mismatch.len();
        let divergent = !callers.diff.is_empty()
            || callers.note.is_some()
            || !callees.diff.is_empty()
            || callees.note.is_some();
        if divergent {
            divergent_symbols += 1;
        }
    }

    let lg_canon = lg_views.canonical();
    let sq_canon = sq_views.canonical();

    // Dual-measurability per pair `(a, b)`: at least one LiveGraph projection measured it —
    // the callee's caller-projection (`cm(b)`) or the caller's callee-projection (`em(a)`).
    // A non-corpus endpoint had no projection taken → unmeasured on that side.
    let measured_pair = |a: &str, b: &str| -> bool {
        let cm = corpus.contains(b)
            && !unmeasured_projections.contains(&(b.to_string(), Direction::Callers));
        let em = corpus.contains(a)
            && !unmeasured_projections.contains(&(a.to_string(), Direction::Callees));
        cm || em
    };

    // ── Canonical (kind-blind) split, §3.6-corrected ──
    let mut canonical = CanonicalSplit {
        livegraph_total: lg_canon.values().sum(),
        sqlite_total: sq_canon.values().sum(),
        ..CanonicalSplit::default()
    };
    let ids: BTreeSet<&(String, String)> = lg_canon.keys().chain(sq_canon.keys()).collect();
    for id in ids {
        let l = lg_canon.get(id).copied().unwrap_or(0);
        let s = sq_canon.get(id).copied().unwrap_or(0);
        canonical.shared += l.min(s);
        canonical.scip_only += l.saturating_sub(s);
        let po = s.saturating_sub(l);
        if po > 0 {
            if measured_pair(&id.0, &id.1) {
                canonical.pipeline_only_dual_measured += po;
            } else {
                canonical.pipeline_only_unmeasured += po;
            }
        }
    }
    canonical.union_edges = canonical.scip_only
        + canonical.pipeline_only_dual_measured
        + canonical.pipeline_only_unmeasured
        + canonical.shared;

    // ── The eligible partition set (rule e) + S-side facts from its IRs ──
    let mut eligible: BTreeMap<String, LanguageSupport> = BTreeMap::new();
    let mut key_sources: BTreeMap<String, SourceSet> = BTreeMap::new();
    let mut node_partitions: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut s_name: BTreeMap<String, String> = BTreeMap::new();
    let mut fallback_keys: BTreeSet<String> = BTreeSet::new();
    let mut fallback_keys_by_partition: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut rollups: BTreeMap<(LanguageSupport, String), PartitionRollup> = BTreeMap::new();

    let resident = lg.resident_irs();
    for view in resident.iter().filter(|v| v.fresh) {
        eligible.insert(view.id.to_string(), view.language);
        let rollup = rollups
            .entry((view.language, view.id.to_string()))
            .or_default();
        for n in &view.ir.nodes {
            let key = n.key.as_str();
            key_sources
                .entry(key.to_string())
                .or_default()
                .add(n.identity_source);
            node_partitions
                .entry(key.to_string())
                .or_default()
                .insert(view.id.to_string());
            s_name
                .entry(key.to_string())
                .or_insert_with(|| n.name.clone());
            match n.identity_source {
                IdentitySource::AstAdopted => rollup.adoption_adopted += 1,
                IdentitySource::ScipSynthesizedFallback => {
                    rollup.adoption_fallback += 1;
                    fallback_keys.insert(key.to_string());
                    fallback_keys_by_partition
                        .entry(view.id.to_string())
                        .or_default()
                        .insert(key.to_string());
                }
                IdentitySource::AstFileScope => rollup.adoption_file_scope += 1,
            }
        }
    }

    // ── The R-RAT-4 collision guard: `fallback_keys(S) ∩ keys(P)` over the key→sources SET ──
    let mut collision_keys: BTreeSet<String> = BTreeSet::new();
    for k in &fallback_keys {
        let mixed = key_sources
            .get(k)
            .map(|s| s.fallback_mixed())
            .unwrap_or(false);
        if p_keys.contains(k) || mixed {
            collision_keys.insert(k.clone());
        }
    }
    let mut colliding_keys: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (partition, keys) in &fallback_keys_by_partition {
        let hits: BTreeSet<String> = keys.intersection(&collision_keys).cloned().collect();
        if !hits.is_empty() {
            colliding_keys.insert(partition.clone(), hits);
        }
    }

    // ── S kind totals + the strict-Calls classification multiset (collision-withheld) ──
    let mut s_kind_totals = SKindTotals::default();
    let mut identity_collision = 0usize;
    let mut withheld_pairs: BTreeSet<(String, String)> = BTreeSet::new();
    // pair -> partition -> instance count (attribution substrate) — strict Calls only.
    let mut s_calls_by_partition: BTreeMap<(String, String), BTreeMap<String, usize>> =
        BTreeMap::new();
    for view in resident.iter().filter(|v| v.fresh) {
        let rollup = rollups
            .entry((view.language, view.id.to_string()))
            .or_default();
        for e in &view.ir.edges {
            match e.edge_type {
                EdgeType::Calls => {
                    s_kind_totals.calls += 1;
                    rollup.s_calls += 1;
                    let pair = (e.src.as_str().to_string(), e.dst.as_str().to_string());
                    if collision_keys.contains(&pair.0) || collision_keys.contains(&pair.1) {
                        identity_collision += 1;
                        rollup.withheld_instances += 1;
                        withheld_pairs.insert(pair);
                    } else {
                        *s_calls_by_partition
                            .entry(pair)
                            .or_default()
                            .entry(view.id.to_string())
                            .or_default() += 1;
                    }
                }
                EdgeType::References => {
                    s_kind_totals.references += 1;
                    rollup.s_references += 1;
                }
                EdgeType::Imports => {
                    s_kind_totals.imports += 1;
                    rollup.s_imports += 1;
                }
            }
        }
    }
    let s_calls: BTreeMap<(String, String), usize> = s_calls_by_partition
        .iter()
        .map(|(pair, parts)| (pair.clone(), parts.values().sum()))
        .collect();
    // partition -> language (for attribution below; every partition here is eligible).
    let partition_language: BTreeMap<String, LanguageSupport> = eligible.clone();

    // ── The kind-aligned instance classification (§3.3) ──
    let mut cls = CallClassification {
        eligible,
        pipeline_calls: sq_canon.values().sum(),
        union_calls: 0,
        dual_measured: 0,
        both: 0,
        both_identities: 0,
        syntactic: SyntacticSplit::default(),
        semantic: SemanticSplit::default(),
        unmeasured_edges: 0,
        unmeasured_identities: 0,
        s_kind_totals,
        fallback_key_count: fallback_keys.len(),
        identity_collision,
        colliding_keys,
        withheld_pairs,
        identity_suspect: 0,
        pairs: BTreeMap::new(),
        delta_pairs: Vec::new(),
        rollups,
    };

    let mut syn_pairs: BTreeSet<(String, String)> = BTreeSet::new();
    let mut sem_pairs: BTreeSet<(String, String)> = BTreeSet::new();

    // P side: every pipeline call pair.
    for (pair, &p) in &sq_canon {
        let (a, b) = (&pair.0, &pair.1);
        let sc = s_calls.get(pair).copied().unwrap_or(0);
        if !measured_pair(a, b) {
            cls.unmeasured_edges += p;
            cls.unmeasured_identities += 1;
            cls.pairs.insert(
                pair.clone(),
                PairRecord {
                    p,
                    s_calls: sc,
                    dual_measured: false,
                    syntactic_subclass: None,
                },
            );
            continue;
        }
        let m = p.min(sc);
        cls.both += m;
        if m > 0 {
            cls.both_identities += 1;
            // Attribute the corroborated min (and later the S excess) to the witnessing S
            // edges' partitions, filling in partition-id order (deterministic).
            if let Some(parts) = s_calls_by_partition.get(pair) {
                let mut remaining = m;
                for (part, &c) in parts {
                    let take = remaining.min(c);
                    remaining -= take;
                    if take > 0 {
                        if let Some(lang) = partition_language.get(part) {
                            if let Some(r) = cls.rollups.get_mut(&(*lang, part.clone())) {
                                r.both_instances += take;
                            }
                        }
                    }
                }
            }
        }
        let excess = p - m;
        let mut subclass = None;
        if excess > 0 {
            syn_pairs.insert(pair.clone());
            if sc > 0 {
                cls.syntactic.multiplicity += excess;
                cls.delta_pairs.push(DeltaPair {
                    caller: a.clone(),
                    callee: b.clone(),
                    p,
                    s_calls: sc,
                });
            } else {
                // Pair-level sub-class: file_scope first (a FILE caller is never a callable
                // caller), then boundary (both endpoints known to S, partition sets disjoint),
                // else uncorroborated.
                let sub = if p_kind.get(a).map(String::as_str) == Some("FILE") {
                    PairSubclass::FileScope
                } else {
                    match (node_partitions.get(a), node_partitions.get(b)) {
                        (Some(pa), Some(pb)) if pa.is_disjoint(pb) => PairSubclass::Boundary,
                        _ => PairSubclass::Uncorroborated,
                    }
                };
                match sub {
                    PairSubclass::Boundary => cls.syntactic.boundary += excess,
                    PairSubclass::FileScope => cls.syntactic.file_scope += excess,
                    PairSubclass::Uncorroborated => cls.syntactic.uncorroborated += excess,
                }
                subclass = Some(sub);
            }
        }
        cls.pairs.insert(
            pair.clone(),
            PairRecord {
                p,
                s_calls: sc,
                dual_measured: true,
                syntactic_subclass: subclass,
            },
        );
    }

    // S side: strict-Calls pairs beyond P (or beyond P's count). Dual-measured BY CONSTRUCTION —
    // see the `unmeasured_edges` doc: the strict ingest's `Calls` caller is always an AST-adopted
    // corpus symbol, so P's projection of this pair ran (and a SQLite error aborts the build).
    for (pair, &sc) in &s_calls {
        let (a, b) = (&pair.0, &pair.1);
        let p = sq_canon.get(pair).copied().unwrap_or(0);
        let m = sc.min(p);
        let excess = sc - m;
        if excess == 0 {
            continue;
        }
        sem_pairs.insert(pair.clone());
        if p > 0 {
            cls.semantic.multiplicity += excess;
            cls.delta_pairs.push(DeltaPair {
                caller: a.clone(),
                callee: b.clone(),
                p,
                s_calls: sc,
            });
        } else {
            cls.semantic.new_pair += excess;
            cls.pairs.insert(
                pair.clone(),
                PairRecord {
                    p: 0,
                    s_calls: sc,
                    dual_measured: true,
                    syntactic_subclass: None,
                },
            );
        }
        // Attribute the S excess per witnessing partition: skip the corroborated min (filled in
        // partition-id order above), attribute the rest.
        if let Some(parts) = s_calls_by_partition.get(pair) {
            let mut min_to_skip = m;
            for (part, &c) in parts {
                let corroborated_here = min_to_skip.min(c);
                min_to_skip -= corroborated_here;
                let excess_here = c - corroborated_here;
                if excess_here > 0 {
                    if let Some(lang) = partition_language.get(part) {
                        if let Some(r) = cls.rollups.get_mut(&(*lang, part.clone())) {
                            if p > 0 {
                                r.semantic_multiplicity += excess_here;
                            } else {
                                r.semantic_new_pair += excess_here;
                            }
                        }
                    }
                }
            }
        }
    }

    cls.syntactic.identities = syn_pairs.len();
    cls.semantic.identities = sem_pairs.len();
    cls.dual_measured = cls.both + cls.syntactic.total();
    cls.union_calls =
        cls.both + cls.syntactic.total() + cls.semantic.total() + cls.unmeasured_edges;

    // ── identity_suspect (guard 3): syntactic (caller, callee-NAME) matching a semantic pair
    //    under a DIFFERENT callee key. Names from P first, else the S node inventory; a pair with
    //    no derivable name makes NO claim (skipped).
    let name_of = |key: &str| -> Option<&String> { p_name.get(key).or_else(|| s_name.get(key)) };
    let sem_index: BTreeSet<(&str, &String, &str)> = sem_pairs
        .iter()
        .filter_map(|(a, b)| name_of(b).map(|n| (a.as_str(), n, b.as_str())))
        .collect();
    for (a, b) in &syn_pairs {
        let Some(bn) = name_of(b) else { continue };
        let hit = sem_index
            .iter()
            .any(|(sa, sn, sb)| *sa == a.as_str() && *sn == bn && *sb != b.as_str());
        if hit {
            cls.identity_suspect += 1;
        }
    }

    Some(WitnessLedger {
        fingerprint: fingerprint.to_string(),
        snapshot_uid: snapshot_uid.to_string(),
        precondition: None,
        compare: Some(CompareSummary {
            corpus_size: corpus.len(),
            divergent_symbols,
            unanswerable_projections: unanswerable,
            livegraph_panics: panics,
            field_mismatches,
            canonical,
            unmeasured_projections,
        }),
        classification: Some(cls),
    })
}
