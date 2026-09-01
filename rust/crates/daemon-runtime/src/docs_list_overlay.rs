//! DOCS-LIST-2 §2 — the `docs_list` vendored-kind overlay (crate-private).
//!
//! (Abstraction one-liner — **what:** apply the daemon's `vendored` kind overlay to a discovered doc
//! inventory using the index's EXISTING vendor-path fact; **concrete current user:**
//! `dispatch::handle_docs_list` + this module's tests; **axis of variation:** none — this is a pure
//! transform extracted purely for a TEST SEAM (the overlay lived inline in the 9644-line dispatcher,
//! unreachable by a unit test without a live daemon + storage + repo); **rejected simpler:** leave the
//! loop inline — rejected because review-3 finding 2 needs a regression test for the release_family
//! invariant, and dispatch.rs is far over the 500-line guardrail. Matches the `enrichment_skip_gate`
//! extraction precedent.)

use repo_graph_doc_facts::DocInventoryEntry;

/// Overlay the `vendored` kind onto every entry under a vendored/third-party directory, using the
/// index's EXISTING vendor-path fact (`is_vendored_path` / `VENDORED_SEGMENTS`, quality/support.rs) —
/// the SAME predicate `hotspots --exclude-vendored` reads, never a second definition.
///
/// Vendored takes PRECEDENCE over every content/location kind: a vendored release-note or license is
/// still demoted as vendored (the reader's docs are what the headline is for). Because that override
/// can replace a `release-notes` kind, it MUST also clear [`DocInventoryEntry::release_family`] — that
/// field's DTO contract is "set ONLY on `release-notes` entries", and a stale family on a now-vendored
/// entry would both violate the invariant and mislead the renderer's family grouping (review-3
/// finding 2). Only the kind + family are touched; counts are recomputed by the caller AFTER this runs.
pub(crate) fn overlay_vendored_kind(entries: &mut [DocInventoryEntry]) {
    for entry in entries {
        if crate::handlers::quality::support::is_vendored_path(&entry.path) {
            entry.kind = "vendored".to_string();
            // Invariant restore: release_family is release-notes-only; the vendored override
            // just replaced that kind, so the family must not survive on the vendored entry.
            entry.release_family = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, kind: &str, family: Option<&str>) -> DocInventoryEntry {
        DocInventoryEntry {
            path: path.to_string(),
            kind: kind.to_string(),
            generated: false,
            content_hash: None,
            release_family: family.map(str::to_string),
        }
    }

    #[test]
    fn vendored_override_clears_release_family() {
        // review-3 finding 2: a vendored release note (e.g. a bundled dependency's own release notes
        // under site-packages) is demoted to `vendored`, and MUST shed its `release_family` so the
        // DTO invariant ("family set only on release-notes") holds and the renderer never groups a
        // vendored entry under a family line.
        let mut entries = vec![entry(
            "fraktag-env/lib/python3.11/site-packages/pkg/docs/releases/1.0.txt",
            "release-notes",
            Some("fraktag-env/lib/python3.11/site-packages/pkg/docs/releases"),
        )];
        overlay_vendored_kind(&mut entries);
        assert_eq!(entries[0].kind, "vendored");
        assert_eq!(
            entries[0].release_family, None,
            "vendored override must clear release_family: {entries:?}"
        );
    }

    #[test]
    fn non_vendored_release_note_keeps_family() {
        // A reader's own release note is untouched — the overlay only fires on vendored paths, so the
        // family survives for the renderer to group.
        let mut entries = vec![entry(
            "docs/releases/1.0.txt",
            "release-notes",
            Some("docs/releases"),
        )];
        overlay_vendored_kind(&mut entries);
        assert_eq!(entries[0].kind, "release-notes");
        assert_eq!(entries[0].release_family.as_deref(), Some("docs/releases"));
    }

    #[test]
    fn vendored_non_release_doc_demoted_family_already_none() {
        // A vendored non-release doc is demoted to vendored; its family was already None and stays so.
        let mut entries = vec![entry("node_modules/pkg/README.md", "readme", None)];
        overlay_vendored_kind(&mut entries);
        assert_eq!(entries[0].kind, "vendored");
        assert_eq!(entries[0].release_family, None);
    }
}
