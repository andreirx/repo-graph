//! DEPS-ATTRIB-2 §2.3 — the honest manifest-coverage split for the default `deps list` headline.
//!
//! Crate-private module extracted from `deps_headline.rs` under the 500-line structural guardrail
//! (DEPS-ATTRIB-2 review-1 item 5 — a pre-ratified, guardrail-driven extraction, NOT a new public
//! boundary). WHAT: the "how many PARSED manifests of this ecosystem govern indexed source vs. none"
//! computation and its read-status sum type. CALLERS: `dispatch::handle_deps_list` (computes) and
//! `deps_headline::build_deps_list_response` (renders). AXIS: operations over a FIXED set of coverage
//! states grow → sum type + exhaustive match (a coverage READ FAILURE is a distinct fact from a
//! computed split and from not-applicable). REJECTED SIMPLER: leaving it inline in the already-617-line
//! `deps_headline.rs` — the guardrail forbids appending a new responsibility to a >500-line file.

use repo_graph_module_queries::{ManifestProvenance, ProvenanceRead};

/// The DEPS-ATTRIB-2 §2.3 honest attribution split: how the PARSED manifests of the queried
/// ecosystem divide across the three honest states — attributed / indexed-but-unattributed / no
/// indexed source — computed from file containment, NOT from whether a module row attributed to them.
pub(crate) struct ManifestCoverage {
    /// Parsed ecosystem manifests that are the NEAREST owner of ≥1 module-ATTRIBUTED file (govern
    /// indexed source AND that source is attributed to a module).
    pub attributed: usize,
    /// Parsed ecosystem manifests nearest to ≥1 INDEXED source file but NO module-attributed file:
    /// indexed source is present, module attribution is not. review-4 blocker 2 / §2.3: the
    /// "governs no indexed source" excuse is FALSE here (indexed source IS present), so this is its
    /// own honest count — a manifest with indexed-but-unowned source must never render that excuse.
    pub indexed_unattributed_manifests: usize,
    /// Total indexed source files nearest to those `indexed_unattributed_manifests` — the "N files
    /// indexed under this manifest, not attributed" count the §2.3 honest line states.
    pub indexed_unattributed_files: usize,
    /// Parsed ecosystem manifests nearest to ZERO indexed source files — the ONLY manifests for
    /// which the "governs no indexed source" excuse is computed-true (§2.3).
    pub no_indexed_source: usize,
}

impl ManifestCoverage {
    /// Total PARSED ecosystem manifests this split covers (attributed + indexed-but-unattributed +
    /// no-indexed-source). The honest byte-parity denominator used when no scanned present-count was
    /// tracked (an old snapshot) — never a fabricated number.
    pub fn total_parsed(&self) -> usize {
        self.attributed + self.indexed_unattributed_manifests + self.no_indexed_source
    }
}

/// How the §2.3 coverage split reaches the JSON builder. A sum type because a read FAILURE is NOT
/// the same fact as "not applicable": the reads that feed the split are fallible and their result is
/// CLASSIFIED/RENDERED, so a failure MUST surface as unknown-with-reason — never collapse to a
/// silently omitted line (STANDING HONESTY RULE #1; operator binding 2026-08-31, closing the 14th
/// recurrence of the failed-read → `None` → silent-omit class). "Not applicable" (old snapshot / no
/// manifests of this ecosystem) is the genuine omit case and is carried by the OUTER `Option`
/// (`None`), distinct from this failure state.
pub(crate) enum CoverageStatus {
    /// Coverage computed from the owned-file ⋈ manifest-dir containment join.
    Computed(ManifestCoverage),
    /// A read that feeds the split failed (the provenance blob was unreadable/corrupt, or the
    /// owned-files read failed) — coverage is UNKNOWN, rendered WITH its reason, never omitted.
    Unknown { reason: String },
}

/// Compute the §2.3 honest attribution split over the PARSED manifests of `ecosystem` (those with a
/// `dir`; `error == None`), from TWO independent file universes:
/// - `owned_file_paths`: indexed, module-OWNED files (the `module_file_ownership` ⋈ `files`
///   attribution basis) — drives the `attributed` count.
/// - `indexed_source_paths`: ALL indexed SOURCE files (code files, ownership-agnostic) — drives the
///   distinction between "indexed source present but unattributed" and "truly no indexed source".
///
/// review-4 blocker 2: `owned_file_paths` alone is NOT the indexed-source universe — repository
/// ownership permits an indexed source file with no matching module prefix to remain UNOWNED. Using
/// owned files as the presence test would misclassify a manifest with indexed-but-unowned source as
/// `no_indexed_source` (a FALSE §2.3 excuse). The indexed-source universe answers "does this manifest
/// govern indexed source?" independently of whether that source got a module owner.
///
/// Return contract (DEPS-ATTRIB-2 review-1 item 2 — split known-absence from unavailable):
/// - `ProvenanceRead::Unavailable { reason }` (the shared diagnostics blob was unreadable or the
///   `deps_manifests` record was corrupt) → `Some(CoverageStatus::Unknown { reason })`. This is a
///   CLASSIFIED/RENDERED read whose failure must surface — never a silent `None`.
/// - `ProvenanceRead::Absent` (snapshot indexed before provenance tracking) → `None`: a genuine,
///   computed-known absence → the caller omits the coverage detail (byte-parity for old snapshots).
/// - `ProvenanceRead::Tracked` with NO parsed manifest of this ecosystem → `None` (nothing to state
///   for this ecosystem's coverage).
/// - `ProvenanceRead::Tracked` with ≥1 parsed manifest → `Some(Computed(..))`.
///
/// Attribution is by NEAREST manifest: each file is assigned to the ecosystem manifest whose `dir` is
/// its LONGEST ancestor-or-equal (the same nearest-manifest semantics the index used and
/// `attach_manifest_context` mirrors). Nearest (not any-ancestor) matters at BOTH ends: it makes
/// glamCRM's nested manifests EACH attributed (every leaf package owns its own files — the false "7
/// govern no indexed source" vanishes), AND keeps a workspace ROOT `package.json` (dir "") from
/// swallowing every nested package's files — a root with no source of its own is honestly "governs no
/// indexed source" (the pre-existing FRAKTAG "1 govern no indexed source" is preserved verbatim).
///
/// Per manifest, precedence encodes the honesty order: nearest to ≥1 OWNED file → `attributed`; else
/// nearest to ≥1 INDEXED source file → `indexed_unattributed`; else → `no_indexed_source`. Since a
/// file's nearest manifest is the same in both universes, an OWNED file's nearest manifest is always
/// classified `attributed`, so `indexed_unattributed` captures only genuinely unowned indexed source.
pub(crate) fn compute_manifest_coverage(
    provenance: &ProvenanceRead,
    ecosystem: &str,
    owned_file_paths: &[String],
    indexed_source_paths: &[String],
) -> Option<CoverageStatus> {
    let records = match provenance {
        ProvenanceRead::Tracked(r) => r,
        // Read failure feeding a rendered/classified surface → unknown-with-reason, never silent.
        ProvenanceRead::Unavailable { reason } => {
            return Some(CoverageStatus::Unknown {
                reason: reason.clone(),
            });
        }
        // Computed-known absence (predates tracking) → omit the coverage detail (honest, byte-parity).
        ProvenanceRead::Absent => return None,
    };
    let ecosystem_manifests: Vec<&ManifestProvenance> = records
        .iter()
        .filter(|r| r.ecosystem == ecosystem && r.error.is_none())
        .collect();
    if ecosystem_manifests.is_empty() {
        return None;
    }
    // Mark each manifest that is the NEAREST owner of ≥1 module-ATTRIBUTED (owned) file.
    let mut governs_owned = vec![false; ecosystem_manifests.len()];
    for path in owned_file_paths {
        if let Some(idx) = nearest_manifest(path, &ecosystem_manifests) {
            governs_owned[idx] = true;
        }
    }
    // Count INDEXED source files nearest to each manifest (ownership-agnostic presence test).
    let mut indexed_counts = vec![0usize; ecosystem_manifests.len()];
    for path in indexed_source_paths {
        if let Some(idx) = nearest_manifest(path, &ecosystem_manifests) {
            indexed_counts[idx] += 1;
        }
    }
    let mut attributed = 0;
    let mut indexed_unattributed_manifests = 0;
    let mut indexed_unattributed_files = 0;
    let mut no_indexed_source = 0;
    for (i, _) in ecosystem_manifests.iter().enumerate() {
        if governs_owned[i] {
            attributed += 1;
        } else if indexed_counts[i] > 0 {
            // Indexed source present under the manifest, but no file attributed to a module — the
            // §2.3 "N files indexed under this manifest, not attributed" case, NEVER the excuse.
            indexed_unattributed_manifests += 1;
            indexed_unattributed_files += indexed_counts[i];
        } else {
            // Computed-true: the subtree contains ZERO indexed source files.
            no_indexed_source += 1;
        }
    }
    Some(CoverageStatus::Computed(ManifestCoverage {
        attributed,
        indexed_unattributed_manifests,
        indexed_unattributed_files,
        no_indexed_source,
    }))
}

/// The index of the NEAREST ecosystem manifest to `path` (longest-`dir` ancestor-or-equal), or
/// `None` when no manifest's `dir` contains it. Shared by the owned-file and indexed-file passes of
/// [`compute_manifest_coverage`] so both use the identical nearest-manifest predicate (a file's
/// nearest manifest must not differ between the two universes, else the precedence would be unsound).
fn nearest_manifest(path: &str, manifests: &[&ManifestProvenance]) -> Option<usize> {
    manifests
        .iter()
        .enumerate()
        .filter(|(_, m)| path_under_dir(path, &m.dir))
        .max_by_key(|(_, m)| m.dir.len())
        .map(|(idx, _)| idx)
}

/// Whether repo-relative `path` is inside directory `dir` (or `dir` is the repo root). The empty /
/// `"."` dir is the root — every path is under it. Otherwise `path` must equal `dir` or begin with
/// `dir` + `/` (a true segment boundary, so `a/b` does not contain `a/bc`). Mirrors
/// `module_queries::deps::compose::dir_is_ancestor_or_equal` (that one is crate-private there; this
/// is the same predicate, kept local rather than widening a cross-crate API for one caller).
fn path_under_dir(path: &str, dir: &str) -> bool {
    let dir = if dir == "." { "" } else { dir };
    let path = if path == "." { "" } else { path };
    if dir.is_empty() {
        return true;
    }
    path == dir
        || path
            .strip_prefix(dir)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prov_rec(path: &str, dir: &str, eco: &str) -> ManifestProvenance {
        ManifestProvenance {
            path: path.to_string(),
            dir: dir.to_string(),
            ecosystem: eco.to_string(),
            error: None,
        }
    }

    /// Unwrap the `Computed` split or fail loudly — the tests below assert on computed coverage.
    fn computed(status: Option<CoverageStatus>) -> ManifestCoverage {
        match status {
            Some(CoverageStatus::Computed(cov)) => cov,
            _ => panic!("expected Computed coverage, got a different status"),
        }
    }

    #[test]
    fn coverage_attributes_every_nested_manifest_that_governs_indexed_source() {
        // DEPS-ATTRIB-2 §2.2/§2.3, glamCRM shape: nested npm manifests, each with indexed files under
        // its dir (owned by a coarse inferred module). ALL govern indexed source → attributed == 7,
        // no_indexed_source == 0, so the false "govern no indexed source" excuse cannot render.
        let records = vec![
            prov_rec("serverless/package.json", "serverless", "npm"),
            prov_rec(
                "serverless/packages/backend/package.json",
                "serverless/packages/backend",
                "npm",
            ),
            prov_rec(
                "serverless/packages/infra/package.json",
                "serverless/packages/infra",
                "npm",
            ),
            prov_rec(
                "serverless/packages/shared/package.json",
                "serverless/packages/shared",
                "npm",
            ),
            prov_rec("frontend/web/package.json", "frontend/web", "npm"),
            prov_rec(
                "frontend/workspace/package.json",
                "frontend/workspace",
                "npm",
            ),
            prov_rec("pl-tools/package.json", "pl-tools", "npm"),
            // A java manifest is present but must NOT count toward the npm split.
            prov_rec("backend/build.gradle", "backend", "java"),
        ];
        let owned = vec![
            "serverless/handler.ts".to_string(),
            "serverless/packages/backend/src/main.ts".to_string(),
            "serverless/packages/infra/stack.ts".to_string(),
            "serverless/packages/shared/util.ts".to_string(),
            "frontend/web/App.tsx".to_string(),
            "frontend/workspace/index.ts".to_string(),
            "pl-tools/cli.js".to_string(),
        ];
        // Every indexed file here is also module-owned (indexed == owned) — the glamCRM happy path.
        let cov = computed(compute_manifest_coverage(
            &ProvenanceRead::Tracked(records),
            "npm",
            &owned,
            &owned,
        ));
        assert_eq!(cov.attributed, 7);
        assert_eq!(cov.no_indexed_source, 0);
        assert_eq!(cov.indexed_unattributed_manifests, 0);
    }

    #[test]
    fn coverage_counts_a_truly_empty_manifest_subtree_as_no_indexed_source() {
        // §2.3 false-excuse predicate: the excuse is computed-true ONLY when the manifest subtree has
        // ZERO indexed files. `serverless/packages/infra` has no owned file under it → no_indexed_source.
        let records = vec![
            prov_rec("serverless/package.json", "serverless", "npm"),
            prov_rec(
                "serverless/packages/infra/package.json",
                "serverless/packages/infra",
                "npm",
            ),
        ];
        let owned = vec![
            "serverless/handler.ts".to_string(),
            // note: nothing under serverless/packages/infra/
        ];
        let cov = computed(compute_manifest_coverage(
            &ProvenanceRead::Tracked(records),
            "npm",
            &owned,
            &owned,
        ));
        assert_eq!(cov.attributed, 1, "serverless governs handler.ts");
        assert_eq!(cov.no_indexed_source, 1, "infra subtree is truly empty");
    }

    #[test]
    fn coverage_root_workspace_manifest_governs_no_source_fraktag_parity() {
        // FRAKTAG shape: a zero-dependency workspace-ROOT package.json (dir "") plus 3 leaf packages,
        // each owning its own files. The root is nearest to NO file (leaves are longer ancestors) →
        // attributed == 3, no_indexed_source == 1 — the exact facts behind the preserved legacy line
        // `3 of 4 npm manifests attributed to a module (1 govern no indexed source)`.
        let records = vec![
            prov_rec("package.json", "", "npm"),
            prov_rec("packages/api/package.json", "packages/api", "npm"),
            prov_rec("packages/engine/package.json", "packages/engine", "npm"),
            prov_rec("packages/ui/package.json", "packages/ui", "npm"),
        ];
        let owned = vec![
            "packages/api/src/index.ts".to_string(),
            "packages/engine/src/lib.ts".to_string(),
            "packages/ui/src/App.tsx".to_string(),
        ];
        let cov = computed(compute_manifest_coverage(
            &ProvenanceRead::Tracked(records),
            "npm",
            &owned,
            &owned,
        ));
        assert_eq!(cov.attributed, 3);
        assert_eq!(cov.no_indexed_source, 1);
        assert_eq!(
            cov.indexed_unattributed_manifests, 0,
            "the root's subtree has no indexed source of its own → no_indexed_source, not unattributed"
        );
    }

    #[test]
    fn coverage_segment_boundary_and_root_dir() {
        // A dir is not an ancestor across a partial segment: `frontend/web` must not match
        // `frontend/website/...`. Root/empty dir governs everything.
        let records = vec![
            prov_rec("frontend/web/package.json", "frontend/web", "npm"),
            prov_rec("package.json", "", "npm"),
        ];
        let owned = vec!["frontend/website/App.tsx".to_string()];
        let cov = computed(compute_manifest_coverage(
            &ProvenanceRead::Tracked(records),
            "npm",
            &owned,
            &owned,
        ));
        // root manifest governs the file; frontend/web does NOT (segment boundary).
        assert_eq!(cov.attributed, 1);
        assert_eq!(cov.no_indexed_source, 1);
    }

    #[test]
    fn coverage_indexed_but_unowned_source_is_not_a_no_indexed_source_excuse() {
        // review-4 blocker 2 / §2.3: a manifest whose subtree contains an INDEXED source file that no
        // module owns (ownership left it unattributed) must NOT render "govern no indexed source" —
        // the excuse is FALSE (indexed source IS present). It is `indexed_unattributed`, with the file
        // count for the honest "N files indexed, not attributed" line. `frontend/orphan` has an indexed
        // `.ts` file but it is absent from the OWNED set; `serverless` owns its file (attributed).
        let records = vec![
            prov_rec("serverless/package.json", "serverless", "npm"),
            prov_rec("frontend/orphan/package.json", "frontend/orphan", "npm"),
        ];
        let owned = vec!["serverless/handler.ts".to_string()];
        let indexed = vec![
            "serverless/handler.ts".to_string(),
            // Indexed source under frontend/orphan, but NOT owned by any module.
            "frontend/orphan/index.ts".to_string(),
            "frontend/orphan/util.ts".to_string(),
        ];
        let cov = computed(compute_manifest_coverage(
            &ProvenanceRead::Tracked(records),
            "npm",
            &owned,
            &indexed,
        ));
        assert_eq!(cov.attributed, 1, "serverless owns handler.ts → attributed");
        assert_eq!(
            cov.no_indexed_source, 0,
            "frontend/orphan HAS indexed source → the excuse must not render"
        );
        assert_eq!(
            cov.indexed_unattributed_manifests, 1,
            "frontend/orphan is indexed-but-unattributed, not no-indexed-source"
        );
        assert_eq!(
            cov.indexed_unattributed_files, 2,
            "both orphan .ts files are the 'N files indexed, not attributed' count"
        );
    }

    #[test]
    fn coverage_unavailable_provenance_is_unknown_with_reason_not_silent() {
        // DEPS-ATTRIB-2 review-1 item 2: the shared diagnostics blob was unreadable / corrupt →
        // ProvenanceRead::Unavailable. Coverage MUST surface as Unknown-with-reason, NEVER a silent
        // None (that was the reviewer's "compute_manifest_coverage returns None for Unavailable" bug).
        let status = compute_manifest_coverage(
            &ProvenanceRead::Unavailable {
                reason: "extraction diagnostics not valid JSON: expected value".to_string(),
            },
            "npm",
            &[],
            &[],
        );
        match status {
            Some(CoverageStatus::Unknown { reason }) => {
                assert!(reason.contains("not valid JSON"), "{reason}");
            }
            _ => panic!("expected Unknown-with-reason for Unavailable provenance"),
        }
    }

    #[test]
    fn coverage_is_none_for_old_snapshot_or_ecosystem_absent() {
        // Absent (predates tracking) → computed-known absence → None (caller omits, byte-parity).
        assert!(compute_manifest_coverage(&ProvenanceRead::Absent, "npm", &[], &[]).is_none());
        // Tracked, but no manifest of the queried ecosystem → None (no line for that ecosystem).
        let records = vec![prov_rec("backend/build.gradle", "backend", "java")];
        assert!(
            compute_manifest_coverage(&ProvenanceRead::Tracked(records), "npm", &[], &[]).is_none()
        );
    }
}
