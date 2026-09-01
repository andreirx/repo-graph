//! DOCS-LIST-2 §2 — the `release-notes` DocKind's STRUCTURAL basis (crate-private).
//!
//! Abstraction one-liner. WHAT: the release/changelog-subtree confirmation logic behind the
//! `release-notes` kind. CONCRETE CURRENT USERS: `crate::discover_doc_inventory` (the kind upgrade plus
//! the `release_family` DTO field). AXIS OF VARIATION: none — a cohesion point, extracted so the
//! STRUCTURAL confirmation lives where the whole doc set is in scope and to keep `classification.rs`
//! under the 500-line guardrail (review-1 item 2). REJECTED SIMPLER: leaving it in `classification.rs`,
//! because `classify_doc_kind` sees only ONE path and CANNOT check the doc set for a manifest index, so
//! a purely-path implementation could only NAME-guess (the exact defect review-1 item 1 rejected).
//!
//! # Honesty (STANDING HONESTY RULE #2 — never classify from a NAME without structural evidence)
//!
//! A directory NAMED a release family (`releases` / `changelog` / …) is only a CANDIDATE. Even the
//! PRESENCE of a file named `index.{txt,rst,md}` in it is still a NAME (the directory's name plus the
//! file's name) — not structural evidence (review-2 item 1). A release-named directory is confirmed a
//! `release-notes` subtree ONLY when its `index.*` file's CONTENT is INSPECTED and carries a Sphinx
//! `toctree` directive — the reStructuredText construct that ENUMERATES the section's member documents,
//! i.e. the manifest relationship that makes the directory a real documentation section (django's
//! `docs/releases/index.txt` opens each release with `.. toctree::`). A release-named directory whose
//! `index.*` lacks that directive — or has no `index.*` at all — is NOT release-notes; its docs keep
//! their path-classified kind (`architecture`) — "no deterministic basis keeps the old kind"
//! (slice §2.1). The name narrows the candidate; the inspected toctree directive is the structural
//! evidence that confirms it. Confirmation therefore requires READING the candidate index (bounded to
//! `<release-dir>/index.{txt,rst,md}` — see [`manifest_index_subtree`]) and lives in
//! [`crate::discover_doc_inventory`], not in the path-only [`crate::classification::classify_doc_kind`].

use std::collections::HashSet;

/// Directory segments that NAME a release/changelog subtree — the CANDIDATE signal ONLY (never
/// sufficient alone; confirmation needs a manifest index whose CONTENT carries a toctree, see the
/// module docs).
const RELEASE_SUBTREE_SEGMENTS: &[&str] = &["releases", "release", "changelog", "changes"];

/// Manifest-index basenames: the Sphinx/docs toctree root of a documentation subtree. A file at
/// `<subtree>/index.<ext>` is only a CANDIDATE manifest; the STRUCTURAL evidence is its CONTENT
/// carrying a toctree directive (see [`is_manifest_index_content`]).
const MANIFEST_INDEX_BASENAMES: &[&str] = &["index.txt", "index.rst", "index.md"];

/// The release-family subtree an ANCESTOR of `relative_path` opens — the path prefix through (and
/// including) the FIRST ancestor directory named a release family — e.g. `docs/releases/1.4.x.txt`
/// → `Some("docs/releases")`. Only ANCESTOR components count (the basename is dropped), so a file
/// literally named `releases` is not itself a subtree. This is the CANDIDATE only; confirmation is
/// [`release_subtree_of`] against a `confirmed` set built in [`crate::discover_doc_inventory`].
fn candidate_subtree(relative_path: &str) -> Option<&str> {
    // Must have at least one ancestor directory (a bare basename has no subtree).
    let last_slash = relative_path.rfind('/')?;
    let ancestors = &relative_path[..last_slash];
    let mut offset = 0usize; // byte offset of the current segment's start within relative_path
    for seg in ancestors.split('/') {
        let end = offset + seg.len();
        if RELEASE_SUBTREE_SEGMENTS.contains(&seg.to_lowercase().as_str()) {
            return Some(&relative_path[..end]);
        }
        offset = end + 1; // advance past the '/'
    }
    None
}

/// The last path segment of `dir` (the directory's own name), or the whole string when it has no `/`.
fn last_segment(dir: &str) -> &str {
    dir.rsplit('/').next().unwrap_or(dir)
}

/// If `relative_path` is a manifest index (`<dir>/index.{txt,rst,md}`), the directory `<dir>` the
/// manifest roots; else `None`. Case-insensitive on the basename. This is the CANDIDATE; whether the
/// directory it roots is a *release* subtree AND whether the file is a *genuine* manifest are decided
/// by [`manifest_index_subtree`] + [`is_manifest_index_content`].
fn manifest_index_dir(relative_path: &str) -> Option<&str> {
    let last_slash = relative_path.rfind('/')?;
    let basename = &relative_path[last_slash + 1..];
    MANIFEST_INDEX_BASENAMES
        .contains(&basename.to_lowercase().as_str())
        .then(|| &relative_path[..last_slash])
}

/// If `relative_path` is a CANDIDATE release manifest — `<dir>/index.{txt,rst,md}` where `<dir>` is
/// NAMED a release family — the subtree `<dir>` it would confirm; else `None`. These are the ONLY
/// files whose CONTENT release-notes confirmation reads (bounded), so the caller uses this both to
/// gate the on-demand read and, once the content is confirmed a manifest, as the confirmed subtree.
pub(crate) fn manifest_index_subtree(relative_path: &str) -> Option<&str> {
    let dir = manifest_index_dir(relative_path)?;
    RELEASE_SUBTREE_SEGMENTS
        .contains(&last_segment(dir).to_lowercase().as_str())
        .then_some(dir)
}

/// Does a candidate index's CONTENT prove it is a genuine documentation MANIFEST — not merely a file
/// named `index.*`? The INSPECTED, deterministic evidence (review-2 item 1) is a Sphinx `toctree`
/// directive: the reStructuredText construct (`.. toctree::`) that ENUMERATES a section's member
/// documents. django's `docs/releases/index.txt` carries several. A release-named directory whose
/// `index.*` lacks a toctree keeps `architecture` — the file's NAME is not structural evidence.
///
/// Deterministic and content-based: a directive line begins (after indentation) with `..` and names
/// the `toctree::` directive. Prose merely mentioning the word does not start a directive line, so it
/// does not match.
pub(crate) fn is_manifest_index_content(content: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("..") && trimmed.contains("toctree::")
    })
}

/// The confirmed release subtree `relative_path` belongs to (its grouping family), or `None` when the
/// path is not under any CONFIRMED release subtree. `Some` requires BOTH the name candidate AND the
/// subtree being present in `confirmed` (built from INSPECTED manifest evidence in
/// [`crate::discover_doc_inventory`]), so it never fires on a bare or non-manifest `releases/`
/// directory.
pub(crate) fn release_subtree_of<'a>(
    relative_path: &'a str,
    confirmed: &HashSet<String>,
) -> Option<&'a str> {
    let candidate = candidate_subtree(relative_path)?;
    confirmed.contains(candidate).then_some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_subtree_matches_ancestor_release_dir() {
        // django's canonical layout: files under docs/releases/.
        assert_eq!(
            candidate_subtree("docs/releases/1.4.x.txt"),
            Some("docs/releases")
        );
        assert_eq!(candidate_subtree("CHANGELOG/2.0.md"), Some("CHANGELOG"));
        assert_eq!(
            candidate_subtree("docs/changes/next.rst"),
            Some("docs/changes")
        );
    }

    #[test]
    fn candidate_subtree_none_without_ancestor_dir() {
        // A bare basename, or a `releases` that is the file itself, is not a subtree.
        assert_eq!(candidate_subtree("releases"), None);
        assert_eq!(candidate_subtree("docs/design.md"), None);
        // Only ancestors count — a file literally named `release.md` is not a subtree by location.
        assert_eq!(candidate_subtree("release.md"), None);
    }

    #[test]
    fn manifest_index_subtree_only_for_release_named_index() {
        // A release-named directory's index.* is a candidate manifest (the subtree it would confirm).
        assert_eq!(
            manifest_index_subtree("docs/releases/index.txt"),
            Some("docs/releases")
        );
        assert_eq!(
            manifest_index_subtree("CHANGELOG/index.md"),
            Some("CHANGELOG")
        );
        // A non-release directory's index.* is not a release manifest candidate.
        assert_eq!(manifest_index_subtree("docs/guide/index.rst"), None);
        // A non-index file under a release directory is not a manifest candidate.
        assert_eq!(manifest_index_subtree("docs/releases/1.4.x.txt"), None);
    }

    #[test]
    fn is_manifest_index_content_needs_a_toctree_directive() {
        // django's real evidence: the index enumerates releases with `.. toctree::`.
        assert!(is_manifest_index_content(
            "Release notes\n=============\n\n.. toctree::\n   :maxdepth: 1\n\n   6.1\n"
        ));
        // review-2 item 1: a release-named index whose CONTENT is just a heading is NOT a manifest.
        assert!(!is_manifest_index_content("Releases\n========\n"));
        // Prose mentioning the word toctree does not start a directive line → not a manifest.
        assert!(!is_manifest_index_content(
            "This page uses a toctree:: elsewhere in the docs.\n"
        ));
    }

    #[test]
    fn release_subtree_of_confirmed_vs_unconfirmed() {
        // A subtree confirmed (by inspected manifest evidence, upstream) resolves its member docs.
        let confirmed: HashSet<String> = ["docs/releases".to_string()].into_iter().collect();
        assert_eq!(
            release_subtree_of("docs/releases/1.4.x.txt", &confirmed),
            Some("docs/releases")
        );
        // An unconfirmed subtree (no manifest evidence upstream) → member docs keep their old kind.
        let unconfirmed = HashSet::new();
        assert_eq!(
            release_subtree_of("docs/releases/1.4.x.txt", &unconfirmed),
            None
        );
        // A non-release doc is never a release note, confirmed or not.
        assert_eq!(release_subtree_of("docs/design.md", &confirmed), None);
    }
}
