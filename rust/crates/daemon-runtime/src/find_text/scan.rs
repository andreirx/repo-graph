//! FIND-GREP-1 — the live working-tree WALK + MATCH concern of `find --text`
//! (`super::find_text`). Reuses the ripgrep library family: an `ignore::WalkBuilder`
//! walk (git-faithful traversal) and `grep` (matcher + searcher, with the searcher's
//! own binary detection) for matching. NOTHING about matching or walking is
//! re-implemented (slice §2.1).
//!
//! Split out of `find_text.rs` per the ≤500-line structural guardrail (review-2 finding
//! 4). This is a COHESIVE unit — "scan the working tree for a pattern" — not a runtime
//! abstraction: it exposes plain functions + POD structs to its ONE caller
//! (`super::run_text_scan`), no trait, no dynamic dispatch, no indirection. The parent
//! keeps the orthogonal concern: joining these matches to the snapshot's spans/versions
//! (annotation + staleness) and serializing the response.
//!
//! Walk scope — review-0 finding 1: the walk is DIRECT (this module owns its
//! `WalkBuilder`), covering EVERY non-ignored working-tree file — Markdown, plain config,
//! text, source — not just the indexer's source/contract/config set. The walk config
//! mirrors the indexer's `ignore` settings (full git-ignore semantics: anchoring,
//! negation, nested `.gitignore`, `.git/info/exclude`) so the set is exactly "what THIS
//! repo's committed ignore files say is in the tree", MINUS only `.git` itself (pruned —
//! never meaningful text). Unlike the indexer it applies NO hardcoded vendor prune: the
//! ignore rules alone define the set, matching ripgrep's own default.

use std::path::Path;

/// Build a regex matcher for the pattern. `fixed` escapes it to a literal (`-F`).
/// A compile failure is returned as the human reason, surfaced honestly (never a panic).
pub(super) fn build_matcher(query: &str, fixed: bool) -> Result<grep::regex::RegexMatcher, String> {
    let pattern = if fixed {
        regex::escape(query)
    } else {
        query.to_string()
    };
    grep::regex::RegexMatcher::new(&pattern).map_err(|e| format!("invalid pattern: {e}"))
}

/// The searcher for the live scan. `BinaryDetection::quit(0)` is the ripgrep default: a
/// file with a NUL byte is treated as binary and the search quits — no text to match, no
/// error, not an omission — so a repo's non-ignored binaries never inflate the skip
/// counts. `line_number(true)` so every match carries its 1-based line.
pub(super) fn build_searcher() -> grep::searcher::Searcher {
    grep::searcher::SearcherBuilder::new()
        .line_number(true)
        .binary_detection(grep::searcher::BinaryDetection::quit(0))
        .build()
}

/// Collect `(line_number, line_text)` for every matching line in one file's content.
/// Line numbers are 1-based (the searcher is built with `line_number(true)`); the
/// trailing newline is trimmed from the rendered text.
struct LineCollector {
    hits: Vec<(u64, String)>,
}

impl grep::searcher::Sink for LineCollector {
    type Error = std::io::Error;

    fn matched(
        &mut self,
        _searcher: &grep::searcher::Searcher,
        mat: &grep::searcher::SinkMatch<'_>,
    ) -> Result<bool, std::io::Error> {
        // `line_number` is guaranteed `Some` because the searcher enables it; if a
        // future config drops it we skip the hit rather than fabricate a line number.
        if let Some(line) = mat.line_number() {
            let text = String::from_utf8_lossy(mat.bytes())
                .trim_end_matches(['\n', '\r'])
                .to_string();
            self.hits.push((line, text));
        }
        Ok(true)
    }
}

/// Search one file's raw bytes. Returns `(hits, errored)`: the matching lines (empty
/// when none) and whether the search ERRORED. The searcher is configured with
/// `BinaryDetection::quit(0)` so a BINARY file quits early WITHOUT error (ripgrep
/// semantics — a binary file has no text to match, not an omission). A genuine per-file
/// search error (e.g. an invalid UTF-8 boundary) does NOT abort the whole scan — the
/// pattern already compiled — but it is COUNTED, never `let _ =`-swallowed: the caller
/// reports the file as skipped so `total_matches` is not claimed exact over a search
/// that did not complete.
fn search_file(
    searcher: &mut grep::searcher::Searcher,
    matcher: &grep::regex::RegexMatcher,
    content: &[u8],
) -> (Vec<(u64, String)>, bool) {
    let mut collector = LineCollector { hits: Vec::new() };
    let errored = searcher
        .search_slice(matcher, content, &mut collector)
        .is_err();
    (collector.hits, errored)
}

/// Content hash of a matched file for the staleness compare (§2.3). This MUST equal
/// the indexer's stored hash — `repo_graph_repo_index::scanner::hash_content`, which is
/// `SHA-256(content.as_bytes()).hex[..16]` over the file read as a UTF-8 string. For a
/// file that is byte-identical to what was indexed, `content.as_bytes()` equals these
/// raw bytes, so hashing the raw bytes yields the same digest — a single changed byte
/// diverges it (Stale). Hashing bytes (not a lossily-decoded string) keeps the compare
/// correct without a UTF-8 round-trip; the equivalence is pinned by a unit test.
fn hash_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let hex = format!("{digest:x}");
    hex[..16].to_string()
}

/// One working-tree file with ≥1 match, carrying the bytes' content hash for the
/// staleness compare. Ordered deterministically by `path` within a [`WalkScan`].
pub(super) struct MatchedFile {
    pub(super) path: String,
    pub(super) content_hash: String,
    pub(super) hits: Vec<(u64, String)>,
}

/// The result of walking + matching the working tree (§2, steps 2–3): the matched
/// files plus the three omission counts. Any nonzero count makes the eventual
/// `total_matches` a LOWER BOUND, never exact (review-0 finding 2).
pub(super) struct WalkScan {
    pub(super) matched: Vec<MatchedFile>,
    pub(super) skipped_unreadable: usize,
    pub(super) skipped_search_error: usize,
    pub(super) skipped_walk_error: usize,
}

/// Walk the working tree DIRECTLY via `ignore` (review-0 finding 1) — every non-ignored
/// file, not the indexer's source-extension-filtered set — and match each with `grep`.
///
/// Enumeration failures are COUNTED (finding 2) rather than aborting the whole scan on
/// one bad directory; paths are sorted so output is deterministic regardless of OS
/// directory order (finding 1: preserve ordering). Each file's raw bytes are read ONCE:
/// `grep` searches bytes directly (the searcher's binary detection skips non-text), and
/// the same bytes hash for the staleness compare — no second read, no TOCTOU window.
pub(super) fn scan_working_tree(
    repo_root: &Path,
    matcher: &grep::regex::RegexMatcher,
    searcher: &mut grep::searcher::Searcher,
) -> WalkScan {
    let mut skipped_unreadable = 0usize;
    let mut skipped_search_error = 0usize;
    let mut skipped_walk_error = 0usize;

    let mut paths: Vec<(String, std::path::PathBuf)> = Vec::new();
    for result in build_walk(repo_root).build() {
        match result {
            Ok(entry) => {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    continue; // directories are traversal structure, not content
                }
                let abs = entry.path();
                let rel = match abs.strip_prefix(repo_root) {
                    Ok(r) => r.to_string_lossy().replace('\\', "/"),
                    Err(_) => continue,
                };
                if rel.is_empty() {
                    continue;
                }
                paths.push((rel, abs.to_path_buf()));
            }
            // A recoverable walk error (e.g. an unreadable directory): COUNTED as an
            // omission, never swallowed (finding 2). The walk continues so partial
            // results still surface. If the ROOT itself is unwalkable this simply
            // yields 0 files + a nonzero walk-error count → an honest incomplete scan,
            // never a false empty.
            Err(_) => skipped_walk_error += 1,
        }
    }
    paths.sort_by(|a, b| a.0.cmp(&b.0));

    let mut matched: Vec<MatchedFile> = Vec::new();
    for (rel, abs) in paths {
        match std::fs::read(&abs) {
            Ok(bytes) => {
                let (hits, errored) = search_file(searcher, matcher, &bytes);
                if errored {
                    skipped_search_error += 1;
                }
                if !hits.is_empty() {
                    matched.push(MatchedFile {
                        path: rel,
                        content_hash: hash_bytes(&bytes),
                        hits,
                    });
                }
            }
            Err(_) => skipped_unreadable += 1,
        }
    }

    WalkScan {
        matched,
        skipped_unreadable,
        skipped_search_error,
        skipped_walk_error,
    }
}

/// The `ignore::WalkBuilder` for the live text scan (review-0 finding 1). The `ignore`
/// settings mirror the indexer's scanner so ignore semantics (anchoring, negation,
/// nested `.gitignore`, `.git/info/exclude`) and reproducibility are identical — the
/// ONE difference is that this walk applies NO source-extension filter, so it reaches
/// every non-ignored file. `.git` is pruned at any depth (with `hidden(false)` the
/// walker would otherwise descend it and flood the scan with binary object noise);
/// everything else is governed purely by the repo's own git ignore rules — matching
/// ripgrep's default (no hardcoded vendor prune).
fn build_walk(repo_root: &Path) -> ignore::WalkBuilder {
    let mut builder = ignore::WalkBuilder::new(repo_root);
    builder
        .hidden(false)
        .parents(false)
        .ignore(false)
        .git_global(false)
        .git_ignore(true)
        .git_exclude(true)
        .require_git(false)
        .follow_links(false)
        .filter_entry(|entry| entry.file_name() != ".git");
    builder
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_string_escapes_regex_metacharacters() {
        // `-F 'unwrap_or(0)'` must match the literal, not a regex group.
        let m = build_matcher("unwrap_or(0)", true).expect("fixed pattern compiles");
        let mut searcher = grep::searcher::SearcherBuilder::new()
            .line_number(true)
            .build();
        let (hits, errored) =
            search_file(&mut searcher, &m, b"let x = unwrap_or(0);\nlet y = 1;\n");
        assert!(!errored, "a clean search does not report an error");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, 1);
    }

    #[test]
    fn regex_pattern_matches_as_regex() {
        let m = build_matcher(r"unwrap_or\(0\)", false).expect("regex compiles");
        let mut searcher = grep::searcher::SearcherBuilder::new()
            .line_number(true)
            .build();
        let (hits, errored) = search_file(&mut searcher, &m, b"a.unwrap_or(0)\nb.unwrap_or(5)\n");
        assert!(!errored);
        assert_eq!(hits.len(), 1, "only the (0) form matches");
    }

    #[test]
    fn invalid_regex_is_reported_not_panicked() {
        assert!(build_matcher("(", false).is_err());
    }

    /// The staleness hash MUST equal the indexer's stored hash
    /// (`scanner::hash_content` = `SHA-256(bytes).hex[..16]`), or every Fresh/Stale
    /// decision is wrong. Pinned against the scanner's own test vector.
    #[test]
    fn hash_bytes_matches_scanner_hash_content() {
        // Same vectors the scanner's `hash_matches_ts_hashcontent` pins.
        assert_eq!(hash_bytes(b"hello world"), "b94d27b9934d3e08");
        assert_eq!(hash_bytes(b""), "e3b0c44298fc1c14");
        assert_eq!(hash_bytes(b"any content").len(), 16);
    }

    // ── Walk-scope seam (review-0 findings 1 & 2) ────────────────────────────

    fn scan(root: &Path, pattern: &str) -> WalkScan {
        let m = build_matcher(pattern, false).expect("pattern compiles");
        let mut s = build_searcher();
        scan_working_tree(root, &m, &mut s)
    }

    fn matched_paths(scan: &WalkScan) -> Vec<&str> {
        scan.matched.iter().map(|m| m.path.as_str()).collect()
    }

    /// review-0 finding 1: the live scan covers EVERY non-ignored file — Markdown and
    /// other non-source text the indexer's extension filter dropped — not just indexed
    /// source. A `.md` hit and a `.rs` hit must BOTH be found.
    #[test]
    fn walk_covers_non_source_files_not_just_indexed_source() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("README.md"), "# notes\nTODO: write docs\n").unwrap();
        std::fs::write(root.join("src/a.rs"), "// TODO: implement\nfn a() {}\n").unwrap();

        let scan = scan(root, "TODO");
        assert_eq!(
            matched_paths(&scan),
            vec!["README.md", "src/a.rs"],
            "the Markdown file must be searched, not only the .rs source"
        );
        // A clean scan claims exactness (all skip counts zero).
        assert_eq!(scan.skipped_unreadable, 0);
        assert_eq!(scan.skipped_search_error, 0);
        assert_eq!(scan.skipped_walk_error, 0);
    }

    /// The walk is scoped by the repo's git ignore rules (works without a `.git` dir,
    /// like the indexer's scanner). An ignored file is excluded even though it matches.
    #[test]
    fn walk_respects_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
        std::fs::write(root.join("ignored.txt"), "TODO ignored\n").unwrap();
        std::fs::write(root.join("kept.txt"), "TODO kept\n").unwrap();

        let scan = scan(root, "TODO");
        assert_eq!(
            matched_paths(&scan),
            vec!["kept.txt"],
            "the gitignored file must be excluded from the scan"
        );
    }

    /// Output order is deterministic (sorted by rel path) regardless of OS directory
    /// order — review-0 finding 1's "preserve deterministic path ordering".
    #[test]
    fn walk_output_is_sorted_by_path() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("z")).unwrap();
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::fs::write(root.join("z/z.txt"), "TODO\n").unwrap();
        std::fs::write(root.join("a/b.txt"), "TODO\n").unwrap();
        std::fs::write(root.join("a/a.txt"), "TODO\n").unwrap();

        let scan = scan(root, "TODO");
        assert_eq!(matched_paths(&scan), vec!["a/a.txt", "a/b.txt", "z/z.txt"]);
    }

    /// A BINARY file (NUL byte) is skipped by the searcher's binary detection as
    /// non-text — NOT counted as an omission (ripgrep semantics). The exact-total claim
    /// therefore survives a repo full of binaries, but a genuine read failure would not.
    #[test]
    fn binary_file_is_skipped_without_counting_as_omission() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // A binary file whose bytes contain the pattern AFTER a leading NUL.
        std::fs::write(root.join("blob.bin"), b"\x00\x00TODO\x00").unwrap();
        std::fs::write(root.join("real.txt"), "TODO here\n").unwrap();

        let scan = scan(root, "TODO");
        assert_eq!(
            matched_paths(&scan),
            vec!["real.txt"],
            "binary content must not surface as a text match"
        );
        assert_eq!(
            scan.skipped_unreadable, 0,
            "a binary file is non-text, not an unreadable omission"
        );
        assert_eq!(scan.skipped_search_error, 0);
        assert_eq!(scan.skipped_walk_error, 0);
    }

    /// A genuinely unreadable file (permissions) is COUNTED as an omission — the scan is
    /// then incomplete and the total a lower bound (review-0 finding 2).
    #[test]
    #[cfg(unix)]
    fn unreadable_file_is_counted_as_omission() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("good.txt"), "TODO good\n").unwrap();
        let bad = root.join("bad.txt");
        std::fs::write(&bad, "TODO bad\n").unwrap();
        std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o000)).unwrap();

        let scan = scan(root, "TODO");
        assert_eq!(matched_paths(&scan), vec!["good.txt"]);
        assert_eq!(
            scan.skipped_unreadable, 1,
            "the unreadable file is a visible omission, not a silent drop"
        );

        std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
}
