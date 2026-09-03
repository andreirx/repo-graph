//! FIND-GREP-1 — the `find --text <pattern>` live working-tree scan.
//!
//! `find` (FIND-FACTS-1) answers from the INDEXED fact tables; comments, libc calls,
//! and arbitrary expressions are not symbols, so a literal/text hunt found nothing
//! there and the tool told a FALSE story about the repo. This module is the honest
//! answer to that class of query: a LIVE regex scan of the working tree, reusing the
//! ripgrep library family — an `ignore::WalkBuilder` walk (the crate ripgrep is built
//! on) for git-faithful traversal, and `grep` (matcher + searcher, with the searcher's
//! own binary detection) for the matching. Nothing about matching or walking is
//! re-implemented (slice §2.1).
//!
//! Walk scope — review-0 finding 1 (2026-09-03): the walk is DIRECT (this module owns
//! its `WalkBuilder`), covering EVERY non-ignored working-tree file — Markdown, plain
//! config, text, source — not just the indexer's source/contract/config set. The
//! earlier build reused `scanner::scan_repo`, whose extension filter silently narrowed
//! the scan to indexed source and dropped legitimate non-ignored text; that is fixed
//! here. The walk config mirrors the indexer's `ignore` settings (full git-ignore
//! semantics: anchoring, negation, nested `.gitignore`, `.git/info/exclude`) so the
//! set is exactly "what THIS repo's committed ignore files say is in the tree", MINUS
//! only `.git` itself (pruned — never meaningful text). Unlike the indexer it applies
//! NO hardcoded vendor prune (`node_modules`, `dist`, …): the ignore rules alone define
//! the set, matching ripgrep's own default. A non-ignored vendor dir is therefore
//! searched; because it has no stored spans/version its hits render bare (NotIndexed).
//!
//! What this module ADDS over plain ripgrep — the two things the compared tool
//! advertised but did not deliver honestly:
//!   1. **Enclosing-symbol annotation from OUR spans** (§2.2). A hit inside a stored
//!      SYMBOL span is annotated `[kind qualified_name]` from the snapshot. A hit
//!      OUTSIDE every span carries NO annotation — visible absence, never a guessed
//!      symbol ([`enclosing_span`] returns `None`, and we never classify from a line's
//!      text or a name; STANDING HONESTY RULE 1).
//!
//!      Annotation FIDELITY is bounded by stored-span fidelity: the annotation is only
//!      ever as precise as the extractor's span for that line, never fabricated beyond
//!      it. On the mature extractors (TypeScript, Rust) the innermost span is the
//!      enclosing function/method/property and the annotation is exact; on lower-
//!      maturity C/C++ a coarse or over-extended container span can be the innermost
//!      one available, so the enclosing symbol renders coarser (a class rather than the
//!      method, and — where the container span itself is mis-extended — possibly the
//!      wrong container). That is a Layer-0 extraction limitation this slice does not
//!      touch (§3 freezes extraction); it is disclosed here rather than hidden. This is
//!      the named follow-up **CPP-SPAN-FIDELITY-1** (operator ruling 2026-09-03): the
//!      leveldb `class Limiter` span 73–806 is over-extended and the real enclosing
//!      function at `env_posix.cc:411` is unextracted — a pre-existing defect every fact
//!      consumer already inherits, NOT a `--text` defect and NOT an outstanding decision.
//!   2. **Freshness honesty** (§2.3). The scan is LIVE; the spans are from a SNAPSHOT.
//!      Per file we compare the live content hash against the stored `file_versions`
//!      hash. Divergence emits the once-per-file "file changed since snapshot — symbol
//!      context may be stale" note; there is NO silent mixing of live text with stale
//!      span context.
//!
//! Scope disclosure (not silent): the header states the scope — a live working-tree
//! scan under the repo's ignore rules — so the reader knows annotation/staleness are
//! meaningful only for files the snapshot actually indexed (a non-indexed matched file
//! renders bare, labeled NotIndexed), never mistaking it for a claim over stored facts.
//!
//! Abstraction record — module: `find_text`; concrete current user:
//! `dispatch_seed::handle_find` (the `text: true` branch of the `find` handler); axis:
//! a new live-scan responsibility kept OFF the oversized `dispatch.rs`, mirroring the
//! `find_facts` split; rejected simpler alternative: inlining the walk+match+annotate
//! in the dispatch arm (grows the god-file, and leaves the enclosing-span selection —
//! the one piece with real logic — untestable in isolation).

use std::collections::HashMap;
use std::path::Path;

use repo_graph_storage::find_text_reads::TextSpanRow;
use repo_graph_storage::StorageConnection;
use serde::Serialize;

/// The live working-tree WALK + MATCH concern (build the matcher/searcher, walk via
/// `ignore`, match via `grep`, hash for staleness). Split out per the ≤500-line guardrail
/// (review-2 finding 4); a cohesive code-organization module, not a runtime abstraction.
mod scan;

use scan::WalkScan;

/// Default cap on rendered HIT LINES across all files (§2.5 volume honesty). `--full`
/// lifts it. A capped run discloses `showing N of M — --full for all`; the compared
/// tool dumped 23.9 KB uncapped and graded D for it.
const DEFAULT_HIT_CAP: usize = 200;

/// Per-file staleness of the working-tree content vs the snapshot's stored version
/// (§2.3). A sum type so "no stored version to compare" is never conflated with
/// "compared and unchanged" — the honesty the compared tool skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Staleness {
    /// Live content hash equals the stored `file_versions` hash — span context is current.
    Fresh,
    /// Live content hash DIFFERS — the working tree diverged since the snapshot, so the
    /// stored spans may not line up. Hits carry annotations best-effort UNDER the note.
    Stale,
    /// No stored version for this path (the file is not in the snapshot — newly added,
    /// or outside the indexed set). No span context exists, so hits render bare; there
    /// is nothing to be "stale" about.
    NotIndexed,
    /// The span/version reads failed (§ STANDING HONESTY RULE) — staleness could not be
    /// determined for ANY file. Labeled unknown-with-reason at the response level, never
    /// silently shown as Fresh.
    Unknown,
}

impl Staleness {
    fn wire(self) -> &'static str {
        match self {
            Staleness::Fresh => "fresh",
            Staleness::Stale => "stale",
            Staleness::NotIndexed => "not_indexed",
            Staleness::Unknown => "unknown",
        }
    }

    /// The once-per-file note, only for a genuinely-diverged file (§2.3).
    fn note(self) -> Option<&'static str> {
        match self {
            Staleness::Stale => Some("file changed since snapshot — symbol context may be stale"),
            Staleness::Fresh | Staleness::NotIndexed | Staleness::Unknown => None,
        }
    }
}

/// The `find --text` response DTO (our OWN struct, serialized across the daemon
/// boundary; the CLI re-validates every required field per STANDING HONESTY RULE 1).
#[derive(Debug, Clone, Serialize)]
pub struct TextScanResponse {
    pub schema: String,
    pub command: String,
    pub repo: String,
    /// The snapshot the spans/versions came from — empty when the repo is not indexed
    /// (the scan still runs; symbol context and staleness are then unavailable).
    pub snapshot: String,
    pub query: String,
    /// Whether the pattern was matched as a FIXED string (`-F`) rather than a regex.
    pub fixed: bool,
    /// The honest one-line scope statement (§ disclosed scope bound).
    pub scope_note: String,
    /// FATAL failure that prevented a scan (bad regex, walk error). When set, `files`
    /// is empty and the counts are zero — the CLI renders it instead of a false empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Set when the span/version reads failed (or no snapshot exists): annotation AND
    /// staleness were withheld for the whole scan, WITH the reason — never a silent
    /// "no symbols / all fresh".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_unavailable: Option<String>,
    /// Total matched hit LINES across all files. EXACT only when the scan completed —
    /// i.e. all three skip counts below are zero. Any nonzero skip makes this a LOWER
    /// BOUND (the renderer states it as such); it is never presented as exact over an
    /// incomplete scan (review-0 finding 2, STANDING HONESTY RULE).
    pub total_matches: usize,
    /// Hit lines actually rendered (== total unless capped).
    pub shown_matches: usize,
    /// `true` when `total_matches > cap` and `--full` was not set.
    pub capped: bool,
    /// The applied hit-line cap (for the disclosure line).
    pub cap: usize,
    /// Files the walk DISCOVERED but could not READ (permissions / IO error), so never
    /// searched. Nonzero means the scan was INCOMPLETE: `total_matches` is a LOWER
    /// BOUND, not exact. Counted (never silently dropped) so the response can state the
    /// omission (STANDING HONESTY RULE — a visible omission, never a hidden one that
    /// lets `total_matches` masquerade as exact). NOTE: a BINARY file is NOT counted
    /// here — the searcher's binary detection skips it as non-text (ripgrep semantics),
    /// which is expected behavior for a text scan, not an omission.
    pub skipped_unreadable: usize,
    /// Files whose matcher search ERRORED mid-file (e.g. an invalid UTF-8 boundary), so
    /// the file was only partially searched. Same lower-bound consequence as
    /// `skipped_unreadable`; counted, never `let _ =`-swallowed.
    pub skipped_search_error: usize,
    /// Paths the WALK itself could not enumerate (e.g. an unreadable directory), so
    /// their contents were never even reached (review-0 finding 2 — the walk-failure
    /// arm the earlier `scanner::scan_repo` swallowed by aborting the whole scan).
    /// Same lower-bound consequence; counted per path, the walk continues so partial
    /// results still surface rather than the whole scan failing on one bad directory.
    pub skipped_walk_error: usize,
    /// Matched files, in deterministic path order; only files with ≥1 shown hit.
    pub files: Vec<TextScanFile>,
}

/// One matched file's group (§2.1 grouped by file).
#[derive(Debug, Clone, Serialize)]
pub struct TextScanFile {
    pub path: String,
    /// `fresh` | `stale` | `not_indexed` | `unknown` (§2.3).
    pub staleness: String,
    /// The once-per-file staleness note, present only when `staleness == stale`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub staleness_note: Option<String>,
    pub hits: Vec<TextScanHit>,
}

/// One matched line (§2.1 `path:line` + the matching line).
#[derive(Debug, Clone, Serialize)]
pub struct TextScanHit {
    pub line: u64,
    pub text: String,
    /// `[kind qualified_name]` from the enclosing stored span, or ABSENT when the hit
    /// is outside every stored span (visible absence, never a guess — §2.2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotation: Option<String>,
}

/// The INNERMOST stored span containing `line`, or `None` when the line is inside no
/// span (§2.2 — a bare hit, never a guessed enclosing symbol). Innermost = the
/// tightest range that still contains the line; ties break toward the LATER-starting
/// (more deeply nested) span, then the earlier-ending, deterministically.
///
/// Pure over its inputs — the unit seam for the "hit-in-span annotated /
/// hit-outside-span bare" tests (§4).
fn enclosing_span(line: i64, spans: &[TextSpanRow]) -> Option<&TextSpanRow> {
    spans
        .iter()
        .filter(|s| s.line_start <= line && line <= s.line_end)
        .min_by(|a, b| {
            let a_len = a.line_end - a.line_start;
            let b_len = b.line_end - b.line_start;
            a_len
                .cmp(&b_len)
                .then(b.line_start.cmp(&a.line_start))
                .then(a.line_end.cmp(&b.line_end))
        })
}

/// The `[kind qualified_name]` annotation for a span (§2.2). Kind is the stored
/// subtype lowercased (`FUNCTION` → `function`); absent subtype drops the kind word
/// (never a guessed kind). The identifier is the qualified name, falling back to the
/// bare name when no qualified name was extracted. Pure — unit-tested.
fn annotation(span: &TextSpanRow) -> String {
    let ident = span
        .qualified_name
        .as_deref()
        .filter(|q| !q.is_empty())
        .unwrap_or(&span.name);
    match span.subtype.as_deref().filter(|s| !s.is_empty()) {
        Some(kind) => format!("[{} {}]", kind.to_lowercase(), ident),
        None => format!("[{ident}]"),
    }
}

/// Bucket the flat span rows by owning path (§2.2 join key). Rows are already
/// path-ordered by the read, so this preserves determinism.
fn bucket_spans(rows: Vec<TextSpanRow>) -> HashMap<String, Vec<TextSpanRow>> {
    let mut by_path: HashMap<String, Vec<TextSpanRow>> = HashMap::new();
    for r in rows {
        by_path.entry(r.path.clone()).or_default().push(r);
    }
    by_path
}

/// Run the live text scan (§2). `snapshot_uid` is `None` when the repo is not indexed
/// — the scan still greps the working tree, but symbol context and staleness are
/// withheld with a reason (never silently absent).
pub fn run_text_scan(
    storage: &StorageConnection,
    snapshot_uid: Option<&str>,
    repo_display: &str,
    repo_root: &Path,
    query: &str,
    fixed: bool,
    full: bool,
) -> TextScanResponse {
    let scope_note =
        "live scan of the working tree (all non-ignored files, repo ignore rules applied)"
            .to_string();
    let base = |error: Option<String>, context_unavailable: Option<String>| TextScanResponse {
        schema: "rgr.agent.v1".to_string(),
        command: "find --text".to_string(),
        repo: repo_display.to_string(),
        snapshot: snapshot_uid.unwrap_or("").to_string(),
        query: query.to_string(),
        fixed,
        scope_note: scope_note.clone(),
        error,
        context_unavailable,
        total_matches: 0,
        shown_matches: 0,
        capped: false,
        cap: DEFAULT_HIT_CAP,
        skipped_unreadable: 0,
        skipped_search_error: 0,
        skipped_walk_error: 0,
        files: Vec::new(),
    };

    // 1. Compile the pattern (honest fatal error on failure).
    let matcher = match scan::build_matcher(query, fixed) {
        Ok(m) => m,
        Err(reason) => return base(Some(reason), None),
    };
    let mut searcher = scan::build_searcher();

    // 2–3. Walk the working tree and match each file (review-0 findings 1 & 2) — the
    //       `scan` submodule owns the walk+match; it is pure over the filesystem +
    //       matcher (no storage), with its own walk-scope / ignore / ordering /
    //       skip-accounting tests.
    let WalkScan {
        matched,
        skipped_unreadable,
        skipped_search_error,
        skipped_walk_error,
    } = scan::scan_working_tree(repo_root, &matcher, &mut searcher);

    // 4. Load span + version context from the snapshot (annotation + staleness). A read
    //    failure, or no snapshot at all, withholds BOTH with a reason — hits still render.
    let (spans_by_path, versions, context_unavailable) = match snapshot_uid {
        None => (
            HashMap::new(),
            HashMap::new(),
            Some(
                "repo not indexed yet — run `rmap index .`; symbol context and staleness \
                 unavailable this run"
                    .to_string(),
            ),
        ),
        Some(snap) => {
            let spans = storage.find_text_symbol_spans(snap);
            let vers = storage.find_text_file_versions(snap);
            match (spans, vers) {
                (Ok(spans), Ok(vers)) => {
                    let versions: HashMap<String, String> =
                        vers.into_iter().map(|v| (v.path, v.content_hash)).collect();
                    (bucket_spans(spans), versions, None)
                }
                (Err(e), _) => (
                    HashMap::new(),
                    HashMap::new(),
                    Some(format!(
                        "symbol context and staleness unavailable (span read failed: {e})"
                    )),
                ),
                (_, Err(e)) => (
                    HashMap::new(),
                    HashMap::new(),
                    Some(format!(
                        "symbol context and staleness unavailable (file-version read failed: {e})"
                    )),
                ),
            }
        }
    };
    let context_ok = context_unavailable.is_none();

    // 5. Assemble the response with the global hit-line cap.
    let total_matches: usize = matched.iter().map(|m| m.hits.len()).sum();
    let cap = if full { usize::MAX } else { DEFAULT_HIT_CAP };
    let mut shown_matches = 0usize;
    let mut files: Vec<TextScanFile> = Vec::new();

    for m in matched {
        if shown_matches >= cap {
            break;
        }
        // Per-file staleness (§2.3). When context is unavailable it is Unknown for all;
        // otherwise compare the live hash to the stored version.
        let staleness = if !context_ok {
            Staleness::Unknown
        } else {
            match versions.get(&m.path) {
                Some(stored) if *stored == m.content_hash => Staleness::Fresh,
                Some(_) => Staleness::Stale,
                None => Staleness::NotIndexed,
            }
        };
        let file_spans = spans_by_path.get(&m.path);

        let mut hits: Vec<TextScanHit> = Vec::new();
        for (line, text) in m.hits {
            if shown_matches >= cap {
                break;
            }
            // Annotation only when we have current span context. Under Stale we still
            // annotate best-effort, but the file note labels it. Under Unknown/NotIndexed
            // there is no trustworthy span context → bare hit (visible absence).
            let annotation = match (context_ok, file_spans) {
                (true, Some(spans)) => {
                    // `line` is 1-based `u64`; spans are `i64`. A pathological overflow
                    // (line > i64::MAX) simply finds no enclosing span (bare hit), never
                    // a wrong annotation.
                    i64::try_from(line)
                        .ok()
                        .and_then(|l| enclosing_span(l, spans))
                        .map(annotation)
                }
                _ => None,
            };
            hits.push(TextScanHit {
                line,
                text,
                annotation,
            });
            shown_matches += 1;
        }

        if !hits.is_empty() {
            files.push(TextScanFile {
                path: m.path,
                staleness: staleness.wire().to_string(),
                staleness_note: staleness.note().map(str::to_string),
                hits,
            });
        }
    }

    let capped = !full && total_matches > shown_matches;
    TextScanResponse {
        total_matches,
        shown_matches,
        capped,
        skipped_unreadable,
        skipped_search_error,
        skipped_walk_error,
        files,
        ..base(None, context_unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(
        start: i64,
        end: i64,
        kind: Option<&str>,
        qname: Option<&str>,
        name: &str,
    ) -> TextSpanRow {
        TextSpanRow {
            path: "src/x.rs".to_string(),
            line_start: start,
            line_end: end,
            subtype: kind.map(str::to_string),
            name: name.to_string(),
            qualified_name: qname.map(str::to_string),
        }
    }

    #[test]
    fn enclosing_picks_innermost_span() {
        let spans = vec![
            span(1, 100, Some("CLASS"), Some("Foo"), "Foo"),
            span(10, 20, Some("FUNCTION"), Some("Foo::bar"), "bar"),
        ];
        // Line 15 is inside both; the innermost (the function) wins.
        let hit = enclosing_span(15, &spans).expect("a span contains line 15");
        assert_eq!(hit.qualified_name.as_deref(), Some("Foo::bar"));
        assert_eq!(annotation(hit), "[function Foo::bar]");
        // Line 5 is only inside the class.
        let hit = enclosing_span(5, &spans).expect("a span contains line 5");
        assert_eq!(annotation(hit), "[class Foo]");
    }

    #[test]
    fn hit_outside_every_span_is_bare_never_guessed() {
        let spans = vec![span(10, 20, Some("FUNCTION"), Some("Foo::bar"), "bar")];
        // Line 5 is outside the only span → no enclosing symbol → no annotation.
        assert!(enclosing_span(5, &spans).is_none());
    }

    #[test]
    fn annotation_falls_back_to_name_and_drops_absent_kind() {
        // No qualified name → bare name; no subtype → no kind word.
        let s = span(1, 3, None, None, "helper");
        assert_eq!(annotation(&s), "[helper]");
        let s = span(1, 3, Some("VARIABLE"), Some(""), "COUNT");
        assert_eq!(annotation(&s), "[variable COUNT]");
    }

    #[test]
    fn staleness_note_only_on_diverged_file() {
        assert_eq!(
            Staleness::Stale.note(),
            Some("file changed since snapshot — symbol context may be stale")
        );
        assert_eq!(Staleness::Fresh.note(), None);
        assert_eq!(Staleness::NotIndexed.note(), None);
        assert_eq!(Staleness::Unknown.note(), None);
    }
}
