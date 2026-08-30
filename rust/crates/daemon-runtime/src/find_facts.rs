//! FIND-FACTS-1 (§2) — the `find` verb's FACTS tier: a deterministic lexical match
//! over the CURRENT snapshot's fact tables, rendered ABOVE the demoted semantic
//! seeds. Facts outrank similarity guesses; this tier answers even when the local
//! embedding model is down.
//!
//! Each fact class is queried from its OWN authoritative fact source — never the
//! rendered text of another command (§2.1): SYMBOL nodes, file paths, and declared
//! modules through the crate-private [`repo_graph_storage::find_facts_reads`] LIKE
//! reads; HTTP routes through the SAME `unified_http_surfaces` the boundaries/
//! surfaces commands share; dependency names through the SAME deps compose
//! `deps list` runs; framework identifiers through the `inferences` table. Every
//! hit is labeled with its fact class and the command that renders it (§2.2), so
//! the label teaches the reader's next move. The per-class query bodies live in the
//! [`queries`] child module (below); THIS file owns the class taxonomy, the hit
//! shape, dedup/cap, and the fixed-order gather.
//!
//! Honesty (STANDING RULE / §2 stop conditions): a fact class whose read FAILS
//! renders `unavailable (<reason>)` — it is NEVER silently dropped from the searched
//! set. `find` therefore always reports what it searched, even under partial
//! failure. An absent path is the explicit [`HitPath`] unknown-with-reason, never a
//! silent omission that reads as "this class has no path" (review-4 item 2).
//!
//! Abstraction record — module: `find_facts`; concrete current user:
//! `dispatch_seed::handle_find` (the `find` handler); axis: the FACTS-tier assembly
//! (class taxonomy + hit shape + dedup + cap + fixed-order gather) kept OFF the
//! oversized `dispatch.rs` per the structural guardrail; rejected simpler
//! alternative: inlining seven per-class queries in the dispatch arm (grows the
//! god-file, no unit seam).
//!
//! Abstraction record — child module: `find_facts::queries`; concrete current user:
//! this file's `gather_class` (the only caller); axis: the file-size guardrail —
//! after review-4 items 2–3 the parent breached 500 lines, so the seven per-class
//! SQL/read bodies (the volatile, source-specific half) get their own file while
//! the taxonomy/shape/dedup (the stable half) stays here; rejected simpler
//! alternative: leaving the queries inline (parent stays >500, violating the
//! guardrail the operator ratified splitting).

use std::collections::BTreeSet;

use repo_graph_storage::StorageConnection;

mod queries;

/// Per-class DISPLAY cap in the default (non-`--full`) rendering (§2.2): each class
/// shows at most this many hits, with an explicit `(+N more — --full)` remainder.
pub(crate) const PER_CLASS_DISPLAY_CAP: usize = 8;

/// Per-class FETCH ceiling for the SQL-`LIKE` classes in the default rendering.
/// Well above the display cap so the remainder count is exact for any realistic
/// query; a query that saturates it renders the remainder as a floor (`N+`), never
/// a false exact count (budget-honesty standard).
const LIKE_FETCH_WINDOW: usize = 200;

/// The seven fact classes (§2.1). A FIXED sum type: variants are the ratified
/// corpus, operations (label, render command) are fixed — adding a class is a
/// deliberate compile-break at every match. The witness manifest
/// `witness/dispatch_fact_classes.txt` mirrors this set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FactClass {
    Symbol,
    File,
    Module,
    HttpSurface,
    Dependency,
    Framework,
    Boundary,
}

impl FactClass {
    /// Deterministic render order (§2.4): also the JSON/human group order.
    pub(crate) const ALL: [FactClass; 7] = [
        FactClass::Symbol,
        FactClass::File,
        FactClass::Module,
        FactClass::HttpSurface,
        FactClass::Dependency,
        FactClass::Framework,
        FactClass::Boundary,
    ];

    /// The fact-class tag shown in the `[<class> → rmap <cmd>]` label (§2.2).
    pub(crate) fn label(self) -> &'static str {
        match self {
            FactClass::Symbol => "symbol",
            FactClass::File => "file",
            FactClass::Module => "module",
            FactClass::HttpSurface => "http-surface",
            FactClass::Dependency => "dependency",
            FactClass::Framework => "framework",
            FactClass::Boundary => "boundary",
        }
    }

    /// The single CLASS-LEVEL command that RENDERS this fact class — the reader's next
    /// move (§2.2) — or `None` for a class whose renderer VARIES per hit. Each string is
    /// a VERIFIED RUNNABLE **and NON-MUTATING** invocation form, probed against the
    /// release binary (revision 1, operator ruling item 1; revision 2 item 1):
    ///   - a bare `rmap boundaries` / `deps` / `inferences` prints usage and does NOT
    ///     render facts — the runnable form is the `list` subcommand;
    ///   - `explain` is a top-level read-only verb that takes the hit's key directly;
    ///   - `map` WRITES `MAP.md` files into the working tree by DEFAULT
    ///     (`rgr/src/commands/map.rs`); a discovery next-step must never mutate the
    ///     tree on paste, so the rendered form is `map --dry-run` (prints the rendered
    ///     map to stdout, writes nothing — the tree-safe orientation move).
    ///
    /// `Boundary` returns `None` (review-6 re-home): its corpus is the governance
    /// DECLARATIONS store, whose renderer depends on the row's declaration KIND
    /// (`boundary` → `rmap violations`, `requirement`/`quality_policy` → `rmap gate`), so
    /// there is no honest single class verb — each hit carries its own `next_command`
    /// ([`queries::boundary_declarations`]) and the group header omits the `→ rmap <cmd>`.
    ///
    /// Derived from what actually surfaces each class, never from the class name
    /// (STANDING HONESTY RULE 2).
    ///
    /// This is the CLASS-LEVEL teaching label (the verb). For `explain`/`map` the
    /// runnable move needs the hit's key too — see [`FactClass::hit_command`], which
    /// the DTO emits per hit so every rendered next command is executable
    /// (review-1 item 1: a bare `rmap explain` exits 1 with a usage error).
    pub(crate) fn render_command(self) -> Option<&'static str> {
        match self {
            FactClass::Symbol => Some("explain"),
            FactClass::File => Some("explain"),
            FactClass::Module => Some("map --dry-run"),
            FactClass::HttpSurface => Some("boundaries list"),
            FactClass::Dependency => Some("deps list"),
            FactClass::Framework => Some("inferences list"),
            // Per-hit renderer (declaration kind → violations|gate); no single verb.
            FactClass::Boundary => None,
        }
    }

    /// The CERTAINTY LAYER of this class's SOURCE (VISION § Fact Certainty Model /
    /// architecture Product Layer Stack), rendered in every label so an inferred
    /// module boundary or a Layer-4 governance declaration is NEVER presented as an
    /// extracted fact (review-1 blocking honesty defect; VISION rule "never describe
    /// Layers 2–4 as Layer 0 truth"). Deterministic lexical RETRIEVAL over a table does not
    /// promote that table's content to Layer 0 — the retrieval is deterministic, the
    /// content's certainty is the table's layer. Source tables (agent_docs/
    /// storage-architecture-v2.md):
    ///   - `extracted` (Layer 0–1 extracted fact): symbol/file = `nodes`/`files`;
    ///     dependency = the declared manifest package names (file_signals /
    ///     parsed-manifest provenance).
    ///   - `inferred` (Layer 2 bounded inference): module = `module_candidates`
    ///     (discovered boundaries); http-surface = boundary/`project_surfaces`
    ///     runtime surfaces.
    ///   - `hint` (Layer 3 evidence-backed hint): framework = `inferences` (framework
    ///     detectors).
    ///   - `governance` (Layer 4 governance/policy overlay): boundary = the authored
    ///     `declarations` store (review-6 re-home). VISION's Fact Certainty Model puts
    ///     governance/policy overlays at Layer 4 — a distinct class from extracted code
    ///     facts, so a boundary/requirement/quality-policy DECLARATION is NEVER tagged
    ///     `extracted` (that would describe a Layer-4 authored overlay as Layer-0 code
    ///     truth, which the VISION forbids). The retrieval is deterministic; the content
    ///     is a governance declaration, not a code fact.
    ///
    /// A closed 4-value tag, matched exhaustively — adding a class forces this map.
    pub(crate) fn certainty_tag(self) -> &'static str {
        match self {
            FactClass::Symbol | FactClass::File | FactClass::Dependency => "extracted",
            FactClass::Module | FactClass::HttpSurface => "inferred",
            FactClass::Framework => "hint",
            FactClass::Boundary => "governance",
        }
    }

    /// The runnable `rmap` invocation (WITHOUT the `rmap` prefix) that carries the
    /// reader from ONE hit of this class to its rendering — the exact, executable,
    /// NON-MUTATING next move (review-1 item 1; review-2 item 1). `explain`/`map`
    /// append the hit's key/path after the (already tree-safe) render command
    /// (`map`'s form is `map --dry-run`, so the module hit becomes
    /// `map --dry-run <path>` — the flag precedes the positional, which `parse_map_args`
    /// accepts in any order). The `… list` classes render the whole listing, so the
    /// class command already IS the move (no per-hit argument). The key is
    /// shell-quoted when it carries spaces or metacharacters, so the line is
    /// copy-paste runnable. A missing key for an argument-taking verb falls back to
    /// the bare class command rather than emitting a broken `explain` with no target
    /// (this cannot occur for symbol/file/module, whose reads always carry a key; the
    /// CLI-side validator rejects the keyless form as malformed anyway — review-4
    /// item 3).
    ///
    /// Returns `None` for a class whose renderer varies per hit (`Boundary`): those hits
    /// carry their own `next_command` and never route through this class-level path. The
    /// `?` on [`FactClass::render_command`] short-circuits before the arg/no-arg split, so
    /// this is only ever `Some` for the six single-renderer classes.
    pub(crate) fn hit_command(self, key: Option<&str>) -> Option<String> {
        let cmd = self.render_command()?;
        Some(if self.folds_hit_key() {
            match key {
                Some(k) => format!("{cmd} {}", shell_arg(k)),
                None => cmd.to_string(),
            }
        } else {
            cmd.to_string()
        })
    }

    /// True for the classes whose per-hit next command appends the hit's key
    /// (`explain <key>` / `map --dry-run <path>`); false for the whole-listing `… list`
    /// classes whose command IS the move. `Boundary` is false (its whole-listing
    /// renderers take no per-hit argument), though it never reaches `hit_command`.
    fn folds_hit_key(self) -> bool {
        matches!(
            self,
            FactClass::Symbol | FactClass::File | FactClass::Module
        )
    }
}

/// Single-quote `arg` for a POSIX shell when it holds any character outside a safe
/// set, so the emitted next command is copy-paste runnable even for keys with spaces
/// or shell metacharacters. A stable_key/path like `glamCRM:src/bnr.ts:BNRService`
/// is entirely safe and stays bare; embedded single quotes are escaped `'\''`.
///
/// The CLI-side validator (`fact_hit::shell_quote_arg`, review-4 item 3) mirrors this
/// EXACT rule so it can reconstruct the ratified `next` and reject any payload that is
/// not that form. The two encoders MUST stay identical; a divergence renders a valid
/// hit as malformed (loud, fail-safe), never a wrong command as runnable.
pub(crate) fn shell_arg(arg: &str) -> String {
    let safe = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | ':' | '@'));
    if safe {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

/// The owning-path dimension of a fact hit — a sum type so "this class carries no
/// path" is never confused with "this hit's path is unknown" (review-4 item 2). The
/// former is a clean omission; the latter MUST render with its reason (STANDING
/// HONESTY RULE — an unknown owning file rendered as absence is a false "no path").
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HitPath {
    /// The class carries a path dimension and this hit's path is known.
    Known(String),
    /// The class carries a path dimension but THIS hit's path is unknown — carries
    /// the reason it is unknown, rendered as `path unknown (<reason>)`.
    Unknown(String),
    /// The class has no path dimension at all (dependency, framework).
    None,
}

impl HitPath {
    /// The path string when known, for dedup identity — `Unknown`/`None` share the
    /// `None` bucket (within a class the path dimension is consistent, so two hits
    /// only collapse when they ALSO share a key/display).
    fn dedup_key(&self) -> Option<&str> {
        match self {
            HitPath::Known(p) => Some(p),
            HitPath::Unknown(_) | HitPath::None => None,
        }
    }
}

/// One rendered fact hit (§2.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FactHit {
    /// The reader-facing display (identifier / route / package / …).
    pub display: String,
    /// The owning path dimension — known, unknown-with-reason, or absent for the
    /// classes that have no path (dependency, framework). Never a silent omission
    /// standing in for an unknown (review-4 item 2).
    pub path: HitPath,
    /// The argument the render command takes for this hit (stable_key, path,
    /// package, …), when there is a concrete one to pass; `None` otherwise.
    pub key: Option<String>,
    /// The runnable per-hit render command, set ONLY when the class's renderer varies
    /// per hit (the `boundary` class: a `boundary`-kind declaration → `violations`, a
    /// `requirement`/`quality_policy`-kind declaration → `gate`). `None` for the six
    /// single-renderer classes, whose per-hit next is derived from the class-level
    /// [`FactClass::hit_command`]. A `&'static str` because every renderer is one of a
    /// fixed, code-defined set — never free-form text crossing to the CLI.
    pub next_command: Option<&'static str>,
}

/// The hits of ONE fact class after dedup + display cap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClassHits {
    pub hits: Vec<FactHit>,
    /// Total matched BEFORE the display cap (bounded by the fetch window). The
    /// remainder is `matched - hits.len()`.
    pub matched: usize,
    /// `true` when the fetch window was saturated, so `matched` is a FLOOR (`N+`),
    /// not an exact count — never presented as measured-complete.
    pub matched_is_floor: bool,
}

/// The outcome for ONE fact class: either its capped hits or a labeled
/// unavailable-with-reason (§2 / STANDING HONESTY RULE — a failed class query is
/// surfaced, never silently absent from the searched set).
#[derive(Debug, Clone)]
pub(crate) struct ClassOutcome {
    pub class: FactClass,
    pub result: Result<ClassHits, String>,
}

/// Dedup `hits` by a stable identity, cap to the display budget, and report the
/// pre-cap matched total. The within-class identity is (known path, key-or-display)
/// (§2.2: deduped by (fact class, path, identity-within-class)); `full` shows every
/// hit. `saturated` marks that the upstream fetch was capped (floor remainder).
pub(super) fn finalize(mut hits: Vec<FactHit>, full: bool, saturated: bool) -> ClassHits {
    // Stable dedup preserving the (already deterministic) upstream order.
    let mut seen: BTreeSet<(Option<String>, String)> = BTreeSet::new();
    hits.retain(|h| {
        seen.insert((
            h.path.dedup_key().map(str::to_string),
            h.key.clone().unwrap_or_else(|| h.display.clone()),
        ))
    });
    let matched = hits.len();
    if !full && hits.len() > PER_CLASS_DISPLAY_CAP {
        hits.truncate(PER_CLASS_DISPLAY_CAP);
    }
    ClassHits {
        hits,
        matched,
        // The floor applies only when the display was NOT full AND the fetch
        // saturated (a fuller run via --full would fetch more).
        matched_is_floor: saturated && !full,
    }
}

/// Fetch limit for the SQL-`LIKE` classes given the render mode.
pub(super) fn like_fetch_limit(full: bool) -> usize {
    if full {
        usize::MAX
    } else {
        LIKE_FETCH_WINDOW
    }
}

/// Run every fact class over `query` against the current snapshot, returning one
/// [`ClassOutcome`] per class in the fixed [`FactClass::ALL`] order. `full` lifts
/// the per-class caps (the `--full`/`--exact` scriptable form).
pub(crate) fn gather_facts(
    storage: &StorageConnection,
    repo_uid: &str,
    snapshot_uid: &str,
    query: &str,
    full: bool,
) -> Vec<ClassOutcome> {
    FactClass::ALL
        .iter()
        .map(|&class| ClassOutcome {
            class,
            result: gather_class(storage, repo_uid, snapshot_uid, query, full, class),
        })
        .collect()
}

/// Every fact class as `unavailable (<reason>)` — used when there is no snapshot to
/// search at all (repo not yet indexed). Keeps the searched set honest: each class
/// is still named, none silently omitted.
pub(crate) fn unavailable_all(reason: &str) -> Vec<ClassOutcome> {
    FactClass::ALL
        .iter()
        .map(|&class| ClassOutcome {
            class,
            result: Err(reason.to_string()),
        })
        .collect()
}

fn gather_class(
    storage: &StorageConnection,
    repo_uid: &str,
    snapshot_uid: &str,
    query: &str,
    full: bool,
    class: FactClass,
) -> Result<ClassHits, String> {
    match class {
        FactClass::Symbol => queries::symbols(storage, snapshot_uid, query, full),
        FactClass::File => queries::files(storage, snapshot_uid, query, full),
        FactClass::Module => queries::modules(storage, snapshot_uid, query, full),
        FactClass::HttpSurface => {
            queries::http_surfaces(storage, repo_uid, snapshot_uid, query, full)
        }
        FactClass::Dependency => queries::dependencies(storage, snapshot_uid, query, full),
        FactClass::Framework => queries::frameworks(storage, snapshot_uid, query, full),
        // Governance declarations are REPO-scoped (NULL snapshot_uid), so this class
        // reads by `repo_uid`, mirroring `violations`/`gate` (review-6 re-home).
        FactClass::Boundary => queries::boundary_declarations(storage, repo_uid, query, full),
    }
}

#[cfg(test)]
mod tests;
