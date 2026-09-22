//! Document classification and generated detection.
//!
//! Classifies documentation files by kind and detects whether
//! content is authored or generated.

use crate::discovery::{doc_name_stem, DOC_EXTENSIONS};
use crate::types::DocKind;

/// AUDIT5-MINORS-1 F1 (operator ruling 2026-09-08, iteration 3): is this doc a LICENSE DOCUMENT?
/// The basis is **NAME ONLY** — the filename starts with `LICENSE`/`COPYING`/`NOTICE` (any
/// extension: `LICENSE`, `LICENSE.md`, `LICENSE-MIT`, `COPYING.LESSER`, `NOTICE.txt`). The name is
/// the sole deterministic structural evidence for a stand-alone license file.
///
/// Why NAME ONLY (no content path): "marker-and-nothing-else" is not a decidable content rule — a
/// headingless prose document that embeds one license clause plus other prose is not a license, so
/// no whole-document body/heading heuristic can separate the two without false positives (the
/// reviewer's §D13 case). The header marker is therefore ignored for kind everywhere: hadoop's
/// ASF-headed `CONTRIBUTING.md` is NOT license; only a dedicated LICENSE*/COPYING*/NOTICE* file is.
///
/// No content is read: the name is available from the path regardless of whether `discover_doc_inventory`
/// loaded content, so a `LICENSE`-named file is `license` even when unreadable (still surfaced in the
/// unreadable count — honesty rule #1). This is distinct from the doc-KIND honesty rule for
/// extensions: a file named `LICENSE` IS a license, whereas a `foo.md` is not architecture merely
/// because of its extension.
pub(crate) fn is_license_document(relative_path: &str) -> bool {
    let file_name = relative_path
        .rsplit('/')
        .next()
        .unwrap_or(relative_path)
        .to_lowercase();
    file_name.starts_with("license")
        || file_name.starts_with("copying")
        || file_name.starts_with("notice")
}

/// Classify a document by its relative path.
pub fn classify_doc_kind(relative_path: &str) -> DocKind {
    let lower = relative_path.to_lowercase();
    let base_name = relative_path.rsplit('/').next().unwrap_or(relative_path);
    let file_name = base_name.to_lowercase();
    // DOCS-DISCOVERY-1 (RG-REQ-008-L01/L02): the ONE documentation-stem rule, shared with discovery.
    let name_stem = doc_name_stem(base_name);

    // MAP.md files
    if file_name == "map.md" {
        return DocKind::Map;
    }

    // README by STEM (RG-REQ-008-L02): `README`, `README.md`, `README.rst`, `README.txt`,
    // `README.TXT` … — case-insensitive stem and documentation extension (D-DD1-003).
    if name_stem == Some("readme") {
        return DocKind::Readme;
    }

    // DOCS-LIST-2 §2: the `release-notes` kind is NOT decided here — it needs the whole doc set (the
    // subtree's manifest index must be PRESENT for structural confirmation, review-1 item 1), which a
    // single path cannot see. It is assigned in `crate::discover_doc_inventory` via `crate::release_notes`.

    // Architecture docs — by EXPLICIT NAME. AUDIT5-MINORS-1 F1 (cycle-5 operator ruling,
    // 2026-09-08): the architecture NAME set is exactly `ARCHITECTURE*` / `DESIGN*` / `OVERVIEW*`
    // (a prefix, any extension). `CONTRIBUTING`/`CHANGELOG` are deliberately NOT here — a
    // contributing guide or a changelog is not an architecture document, so they fall through to the
    // neutral `Doc` kind below. Keeping this set tiny is the whole point of F1: `architecture` names
    // a real design document, never the default catch-all (the old rule folded every docs-tree
    // prose file into `architecture` — repo-graph read 577/591).
    if file_name.starts_with("architecture")
        || file_name.starts_with("design")
        || file_name.starts_with("overview")
    {
        return DocKind::Architecture;
    }

    // Config files (extension) — AUDIT5-MINORS-1 F1: this PRECEDES the `docs/`/`design/` directory
    // rule so a `docker-compose.yaml` (or any `.yaml`/`.yml`) UNDER `docs/` is `config`, not
    // `architecture` (extension beats directory — the audit's §D13 defect).
    //
    // `.env*` is NOT special-cased here: it falls through to the `DocKind::Config`
    // default below ON PURPOSE. `classify_doc_kind` feeds the semantic-fact EXTRACTOR
    // (`extractors::extract_from_file` dispatches `DocKind::Config → config::extract`,
    // which derives the `EnvironmentSurface` hint from `.env*`), so a doc kind is
    // required for that retained path. The SELF-POLLUTION-1 §3 rule — `.env*` is never
    // a *document*, never listed, never on orient's Docs line — is enforced ONE layer
    // up, in `discover_doc_inventory` via `is_env_path`, NOT by withholding a kind here
    // (review-4 finding 2). Classifying it Config here and excluding it from the
    // inventory there are not in tension: the extractor and the inventory are different
    // surfaces.
    if file_name.starts_with("docker-compose")
        || file_name.starts_with("compose.")
        || file_name.ends_with(".yaml")
        || file_name.ends_with(".yml")
    {
        return DocKind::Config;
    }

    // Architecture docs — by an explicitly ARCHITECTURAL DIRECTORY, matched on PATH COMPONENTS
    // (not raw substrings). AUDIT5-MINORS-1 F1 (cycle-5 operator ruling, 2026-09-08; corrected in
    // iteration 6 per review-5): the prior rule matched ALL of `docs/`, so every docs-tree prose
    // file read `architecture` and the movement never happened (repo-graph stayed at 591). It was
    // narrowed to an explicitly architectural directory, but with `lower.contains(...)` — which
    // matched unrelated names as *substrings*: `src/redesign/guide.md` matched `"design/"`,
    // `src/mydocs/architecture/guide.md` matched `"docs/architecture"`. Both would wrongly become
    // `architecture`. The fix splits the path into slash-delimited components and requires
    // WHOLE-COMPONENT matches:
    //   - a component exactly `design` (a `design/` tree anywhere), OR
    //   - a `docs` component IMMEDIATELY followed by a component beginning `architecture` or
    //     `design` (a `docs/architecture*` or `docs/design*` subtree — this preserves the required
    //     `docs/design-system/…` positive while rejecting `src/mydocs/architecture/…`).
    // The architecture NAME rule above already handles `docs/architecture.md` / `docs/design.md`
    // (the filename cases), so this rule is purely the directory signal. Every OTHER file under
    // `docs/` (and `agent_docs/`) falls through to the neutral `Doc` kind below — a small,
    // explicitly-named `architecture` set and a large `doc` set. Runs AFTER the config extension
    // rule (extension beats directory — the §D13 fix).
    let components: Vec<&str> = lower.split('/').collect();
    let in_architecture_dir = components.iter().enumerate().any(|(i, comp)| {
        *comp == "design"
            || (*comp == "docs"
                && components.get(i + 1).is_some_and(|next| {
                    next.starts_with("architecture") || next.starts_with("design")
                }))
    });
    if in_architecture_dir {
        return DocKind::Architecture;
    }

    // Prose documentation OUTSIDE an architecture/design tree — Markdown / reStructuredText /
    // plain-text. AUDIT5-MINORS-1 F1: a NEUTRAL `Doc` kind, NOT the `architecture` default bucket
    // (which the audit found inflated `architecture` to a catch-all — repo-graph "architecture
    // 577"). Every `.md`/`.rst`/`.txt` with no explicit-name or directory architecture signal
    // lands here honestly. The extension list is discovery's `DOC_EXTENSIONS` (one spelling), so
    // `.markdown`/`.adoc` prose is `Doc` too.
    if DOC_EXTENSIONS.iter().any(|e| file_name.ends_with(e)) {
        return DocKind::Doc;
    }

    // DOCS-DISCOVERY-1: an extensionless documentation-stem file (`INSTALL`, `AUTHORS`, `NEWS`,
    // `ChangeLog`) is prose — the neutral `Doc`, never the `Config` default below.
    if name_stem.is_some() {
        return DocKind::Doc;
    }

    // Default for unknown
    DocKind::Config
}

/// Frontmatter markers that indicate generated content.
const GENERATED_MARKERS: &[&str] = &[
    "generated_by",
    "generated: true",
    "kind: synthesized_summary",
    "auto_generated",
    "machine_generated",
];

/// Check if content frontmatter indicates generated document.
///
/// Parses YAML frontmatter and looks for generation markers.
pub fn is_generated_by_frontmatter(content: &str) -> bool {
    let frontmatter = match extract_frontmatter(content) {
        Some(fm) => fm,
        None => return false,
    };

    let lower = frontmatter.to_lowercase();

    for marker in GENERATED_MARKERS {
        if lower.contains(marker) {
            return true;
        }
    }

    false
}

/// Get explicit generated status from frontmatter.
///
/// Returns:
/// - `Some(true)` if frontmatter explicitly indicates generated content
/// - `Some(false)` if frontmatter explicitly indicates authored content (`generated: false`)
/// - `None` if frontmatter is absent or silent on generated status
///
/// This is authoritative: an explicit `generated: false` overrides path heuristics.
pub fn get_generated_from_frontmatter(content: &str) -> Option<bool> {
    if let Some(data) = parse_frontmatter(content) {
        // Explicit boolean field takes precedence
        if let Some(g) = data.generated {
            return Some(g);
        }
        // generated_by field implies generated = true
        if data.generated_by.is_some() {
            return Some(true);
        }
    }

    // Fall back to marker-based detection (returns Some(true) if markers found, None otherwise)
    let frontmatter = extract_frontmatter(content)?;
    let lower = frontmatter.to_lowercase();

    for marker in GENERATED_MARKERS {
        if lower.contains(marker) {
            return Some(true);
        }
    }

    None
}

/// Detect if document is generated using content analysis.
///
/// Path-based heuristics alone are NOT sufficient evidence.
/// Returns true only if content contains positive generation evidence
/// (explicit frontmatter markers like `generated: true` or `generated_by`).
///
/// When content is provided but silent on generation status, returns `false`:
/// a bare path (e.g. a `MAP.md` filename) is NOT evidence of generation. The one
/// authoritative path-vs-name generation signal — rmap's own `map` sidecars — is
/// the marker-gated [`crate::self_generated::is_self_generated`], never a filename.
pub fn is_generated(_relative_path: &str, content: &str) -> bool {
    // Content analysis is authoritative. Path alone is insufficient.
    get_generated_from_frontmatter(content).unwrap_or(false)
}

/// Extract YAML frontmatter from markdown content.
///
/// Frontmatter is delimited by `---` at start and end.
pub fn extract_frontmatter(content: &str) -> Option<&str> {
    let trimmed = content.trim_start();

    if !trimmed.starts_with("---") {
        return None;
    }

    let after_start = &trimmed[3..];
    let end_pos = after_start.find("\n---")?;

    Some(&after_start[..end_pos])
}

/// Parse frontmatter as YAML and extract specific fields.
///
/// Returns None if frontmatter is missing or malformed.
pub fn parse_frontmatter(content: &str) -> Option<FrontmatterData> {
    let fm_str = extract_frontmatter(content)?;

    // Use serde_yaml to parse
    let value: serde_yaml::Value = serde_yaml::from_str(fm_str).ok()?;
    let mapping = value.as_mapping()?;

    let mut data = FrontmatterData::default();

    // Extract known fields
    if let Some(v) = mapping.get("generated_by") {
        data.generated_by = v.as_str().map(String::from);
    }
    if let Some(v) = mapping.get("generated") {
        data.generated = v.as_bool();
    }
    if let Some(v) = mapping.get("replaces") {
        data.replaces = v.as_str().map(String::from);
    }
    if let Some(v) = mapping.get("deprecated") {
        data.deprecated = v.as_bool();
    }
    if let Some(v) = mapping.get("deprecated_by") {
        data.deprecated_by = v.as_str().map(String::from);
    }
    if let Some(v) = mapping.get("alternative_to") {
        data.alternative_to = v.as_str().map(String::from);
    }
    if let Some(v) = mapping.get("kind") {
        data.kind = v.as_str().map(String::from);
    }
    if let Some(v) = mapping.get("scope") {
        data.scope = v.as_str().map(String::from);
    }

    Some(data)
}

/// Structured frontmatter data extracted from documents.
#[derive(Debug, Clone, Default)]
pub struct FrontmatterData {
    /// Generator tool (e.g., "rgistr")
    pub generated_by: Option<String>,
    /// Explicit generated flag
    pub generated: Option<bool>,
    /// Module/symbol this replaces
    pub replaces: Option<String>,
    /// Whether this is deprecated
    pub deprecated: Option<bool>,
    /// What this is deprecated by
    pub deprecated_by: Option<String>,
    /// Alternative module/symbol
    pub alternative_to: Option<String>,
    /// Document kind (e.g., "synthesized_summary")
    pub kind: Option<String>,
    /// Scope (e.g., "folder", "repo")
    pub scope: Option<String>,
}

impl FrontmatterData {
    /// Check if this frontmatter indicates generated content.
    pub fn is_generated(&self) -> bool {
        self.generated == Some(true)
            || self.generated_by.is_some()
            || self.kind.as_deref() == Some("synthesized_summary")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_readme() {
        assert_eq!(classify_doc_kind("README.md"), DocKind::Readme);
        assert_eq!(classify_doc_kind("readme.md"), DocKind::Readme);
        assert_eq!(classify_doc_kind("README"), DocKind::Readme);
    }

    #[test]
    fn classify_map() {
        assert_eq!(classify_doc_kind("MAP.md"), DocKind::Map);
        assert_eq!(classify_doc_kind("src/core/MAP.md"), DocKind::Map);
    }

    #[test]
    fn classify_architecture() {
        // AUDIT5-MINORS-1 F1 (cycle-5 operator ruling): architecture is by explicit NAME
        // (ARCHITECTURE*/DESIGN*/OVERVIEW*) or an explicitly architectural DIRECTORY
        // (design/, docs/architecture*, docs/design*) ONLY — the exact acceptance cases:
        assert_eq!(classify_doc_kind("ARCHITECTURE.md"), DocKind::Architecture); // name
        assert_eq!(
            classify_doc_kind("docs/architecture/overview.md"),
            DocKind::Architecture
        ); // docs/architecture dir (and OVERVIEW name)
        assert_eq!(classify_doc_kind("design/foo.md"), DocKind::Architecture); // design/ dir
        assert_eq!(classify_doc_kind("docs/design.md"), DocKind::Architecture); // DESIGN name
        assert_eq!(classify_doc_kind("OVERVIEW.rst"), DocKind::Architecture); // OVERVIEW name
        assert_eq!(
            classify_doc_kind("docs/design-system/tokens.md"),
            DocKind::Architecture
        ); // docs/design* subtree (the `docs/design` prefix, not `design/`)
    }

    #[test]
    fn plain_docs_tree_prose_is_doc_not_architecture() {
        // AUDIT5-MINORS-1 F1 (cycle-5 operator ruling): a docs-tree prose file with NO explicit
        // architecture name and NOT under an architecture directory is the neutral `Doc` kind, not
        // the `architecture` catch-all (the old `docs/`-location rule read repo-graph at 577/591).
        assert_eq!(classify_doc_kind("docs/slices/x.md"), DocKind::Doc); // operator acceptance case
        assert_eq!(classify_doc_kind("docs/guide.md"), DocKind::Doc);
        assert_eq!(classify_doc_kind("agent_docs/validation.md"), DocKind::Doc);
        assert_eq!(classify_doc_kind("docs/faq/admin.txt"), DocKind::Doc);
    }

    #[test]
    fn architecture_dir_rule_matches_path_components_not_substrings() {
        // AUDIT5-MINORS-1 F1 (iteration-6 correction, review-5): the directory rule matches whole
        // PATH COMPONENTS, not raw substrings. The prior `lower.contains(...)` form mis-fired on
        // unrelated names. Negative — these must stay neutral `Doc`:
        assert_eq!(classify_doc_kind("src/redesign/guide.md"), DocKind::Doc); // "redesign" ⊃ "design/"
        assert_eq!(
            classify_doc_kind("src/mydocs/architecture/guide.md"),
            DocKind::Doc
        ); // "mydocs/architecture" ⊃ "docs/architecture" but "mydocs" ≠ "docs" component
        assert_eq!(classify_doc_kind("src/mydocs/x.md"), DocKind::Doc); // "mydocs" ≠ "docs" component
        assert_eq!(classify_doc_kind("src/undesigned/x.md"), DocKind::Doc); // "undesigned" ≠ "design"
                                                                            // Positive — the whole-component matches that MUST be preserved:
        assert_eq!(classify_doc_kind("design/foo.md"), DocKind::Architecture); // exact `design` component
        assert_eq!(
            classify_doc_kind("docs/design-system/tokens.md"),
            DocKind::Architecture
        ); // `docs` then component beginning `design`
        assert_eq!(
            classify_doc_kind("docs/architecture-notes/adr-1.md"),
            DocKind::Architecture
        ); // `docs` then component beginning `architecture`
        assert_eq!(
            classify_doc_kind("libs/gui/design/theme.md"),
            DocKind::Architecture
        ); // exact `design` component nested deeper
    }

    #[test]
    fn classify_config() {
        assert_eq!(classify_doc_kind("docker-compose.yml"), DocKind::Config);
        assert_eq!(classify_doc_kind("config.yaml"), DocKind::Config);
    }

    #[test]
    fn config_extension_beats_docs_directory() {
        // AUDIT5-MINORS-1 F1: a `.yaml`/docker-compose UNDER `docs/` is Config, not Architecture —
        // extension beats directory (the §D13 defect: docs/monitoring/docker-compose.yaml was
        // mislabeled "architecture" because the `docs/` path rule ran first).
        assert_eq!(
            classify_doc_kind("docs/monitoring/docker-compose.yaml"),
            DocKind::Config
        );
        assert_eq!(
            classify_doc_kind("docs/deploy/compose.prod.yml"),
            DocKind::Config
        );
    }

    #[test]
    fn plain_prose_outside_docs_tree_is_neutral_doc_not_architecture() {
        // AUDIT5-MINORS-1 F1: the generic `.md`/`.rst`/`.txt` fallthrough is the NEUTRAL `Doc`
        // kind, NOT the `architecture` catch-all that inflated the count. "architecture" is now
        // only by explicit ARCHITECTURE*/DESIGN*/OVERVIEW* name or an architecture directory.
        // AMENDED (cycle-5 ruling): CHANGELOG is NOT an architecture name — it is neutral `Doc`.
        assert_eq!(classify_doc_kind("CHANGELOG.md"), DocKind::Doc);
        assert_eq!(classify_doc_kind("CONTRIBUTING.md"), DocKind::Doc);
        assert_eq!(classify_doc_kind("NOTES.md"), DocKind::Doc);
        assert_eq!(classify_doc_kind("src/core/api_MAP.md"), DocKind::Doc);
        assert_eq!(classify_doc_kind("guide.rst"), DocKind::Doc);
        assert_eq!(classify_doc_kind("changes.txt"), DocKind::Doc);
        // A docs/-tree prose file is now neutral `Doc` (only an architecture NAME/DIR is architecture).
        assert_eq!(classify_doc_kind("docs/guide.md"), DocKind::Doc);
    }

    #[test]
    fn is_license_document_by_name_only() {
        // AUDIT5-MINORS-1 F1 (operator ruling, iteration 3): NAME ONLY. The `license` kind is
        // assigned ONLY to files named LICENSE*/COPYING*/NOTICE* (any extension). The header marker
        // is ignored for kind everywhere else.
        //
        // Positive — dedicated license FILES (name is the sole basis, any extension, any dir):
        assert!(is_license_document("LICENSE"));
        assert!(is_license_document("LICENSE.txt"));
        assert!(is_license_document("LICENSE.md"));
        assert!(is_license_document("LICENSE-MIT"));
        assert!(is_license_document("path/to/COPYING"));
        assert!(is_license_document("COPYING.LESSER"));
        assert!(is_license_document("NOTICE.txt"));
        // Negative — the reviewer's required case: a HEADINGLESS document that embeds an MIT/BSD/
        // Apache body clause PLUS other prose is NOT a license (the name does not match). No
        // content rule can decide "marker-and-nothing-else", so content is not consulted at all.
        assert!(!is_license_document("legal/terms.txt")); // MIT-body content would have fired the
                                                          // old content path; NAME ONLY rejects it.
        assert!(!is_license_document("docs/BUILDING.txt")); // ASF-headed plain-text authored doc.
                                                            // Negative — an ASF-headed authored doc (§D13 false-positive class): not license by name.
        assert!(!is_license_document("CONTRIBUTING.md"));
        assert!(!is_license_document("README.md"));
        assert!(!is_license_document("guide.md"));
        // Negative — the match is a NAME PREFIX: `licensing-guide.md` shares the prefix "licens"
        // but diverges at 'i' vs 'e', so `starts_with("license")` is false. A prose doc ABOUT
        // licensing is not a license file.
        assert!(!is_license_document("licensing-guide.md"));
    }

    #[test]
    fn classify_env_as_config_for_extractor_only() {
        // review-4 finding 2: `.env*` DOES receive a doc kind here — `DocKind::Config`,
        // via the default — because the semantic-fact extractor dispatches on kind
        // (`DocKind::Config → config::extract` → EnvironmentSurface). The §3 "never a
        // document" rule is enforced in the INVENTORY layer (is_env_path), not by
        // withholding a kind. This pins the extractor-facing contract so a future edit
        // cannot silently starve the env-surface extractor.
        assert_eq!(classify_doc_kind(".env"), DocKind::Config);
        assert_eq!(classify_doc_kind(".env.production"), DocKind::Config);
        assert_eq!(
            classify_doc_kind("frontend/web/.env.local"),
            DocKind::Config
        );
    }

    #[test]
    fn classify_release_dir_is_neutral_doc_without_the_doc_set() {
        // review-1 item 1: path-only classification NEVER emits release-notes (it cannot see the
        // subtree's manifest index). AMENDED (cycle-5 ruling): a doc under `docs/releases/` is now
        // the neutral `Doc` default (not `architecture`) — `docs/releases` is not an architecture
        // directory; the release-notes upgrade happens in `discover_doc_inventory` (see lib_tests),
        // whose upgrade arm accepts `Doc` as well as `Architecture`/`Config`.
        assert_eq!(classify_doc_kind("docs/releases/1.4.x.txt"), DocKind::Doc);
        assert_eq!(classify_doc_kind("docs/design.md"), DocKind::Architecture);
    }

    // DOCS-DISCOVERY-1 (RG-REQ-008-L02): the `readme` kind is decided by STEM, case-insensitive
    // (D-DD1-003), so reStructuredText / plain-text / bare READMEs classify `readme`.
    #[test]
    fn readme_kind_by_stem_for_rst_txt_and_bare() {
        for p in [
            "README.rst",
            "README.txt",
            "readme.txt",
            "README",
            "README.TXT",
            "extras/README.TXT",
            "board/arm/foundation-v8/readme.txt",
            "README.adoc",
            "README.markdown",
        ] {
            assert_eq!(classify_doc_kind(p), DocKind::Readme, "{p}");
        }
        // Not a documentation extension → not a readme stem.
        assert_ne!(classify_doc_kind("README.html"), DocKind::Readme);
    }

    // DOCS-DISCOVERY-1: a stem-admitted file with NO extension is the neutral `doc`, never the
    // `Config` default (hadoop `ChangeLog`, leveldb `AUTHORS`/`NEWS`, django `INSTALL`).
    #[test]
    fn bare_stem_documents_classify_doc_not_config() {
        for p in [
            "INSTALL",
            "AUTHORS",
            "NEWS",
            "CHANGELOG",
            "ChangeLog",
            "BUILDING",
            "CONTRIBUTING",
            "src/main/native/AUTHORS",
        ] {
            assert_eq!(classify_doc_kind(p), DocKind::Doc, "{p}");
        }
        // The bare architecture stems keep their explicit-name kind.
        assert_eq!(classify_doc_kind("DESIGN"), DocKind::Architecture);
        assert_eq!(classify_doc_kind("OVERVIEW"), DocKind::Architecture);
        // A non-stem extensionless name keeps the Config default (the extractor contract).
        assert_eq!(classify_doc_kind("Makefile"), DocKind::Config);
    }

    #[test]
    fn markdown_and_adoc_in_a_docs_tree_classify_doc() {
        assert_eq!(classify_doc_kind("docs/a.markdown"), DocKind::Doc);
        assert_eq!(classify_doc_kind("docs/b.adoc"), DocKind::Doc);
        assert_eq!(classify_doc_kind("docs/manual/manual.adoc"), DocKind::Doc);
        assert_eq!(classify_doc_kind("docs/NOTES.MD"), DocKind::Doc);
        assert_eq!(
            classify_doc_kind("x/src/site/markdown/Configuration.md"),
            DocKind::Doc
        );
    }

    #[test]
    fn classify_txt_and_rst_as_docs() {
        // SELF-POLLUTION-1 §3: .txt/.rst are prose docs, not the Config fallthrough. AMENDED
        // (cycle-5 ruling): a docs-tree prose file with no architecture NAME/DIR is neutral `Doc`.
        assert_eq!(classify_doc_kind("docs/ref/fields.txt"), DocKind::Doc);
        assert_eq!(classify_doc_kind("docs/index.rst"), DocKind::Doc);
    }

    #[test]
    fn extract_frontmatter_basic() {
        let content = "---\ntitle: Test\n---\n# Content";
        let fm = extract_frontmatter(content).unwrap();
        assert_eq!(fm.trim(), "title: Test");
    }

    #[test]
    fn extract_frontmatter_missing() {
        let content = "# No frontmatter";
        assert!(extract_frontmatter(content).is_none());
    }

    #[test]
    fn generated_by_frontmatter() {
        let content = "---\ngenerated_by: rgistr\n---\n# Map";
        assert!(is_generated_by_frontmatter(content));

        let content2 = "---\ngenerated: true\n---\n# Map";
        assert!(is_generated_by_frontmatter(content2));

        let content3 = "---\ntitle: Authored\n---\n# Doc";
        assert!(!is_generated_by_frontmatter(content3));
    }

    #[test]
    fn parse_frontmatter_replaces() {
        let content = "---\nreplaces: old-module\ndeprecated: true\n---\n# Doc";
        let data = parse_frontmatter(content).unwrap();

        assert_eq!(data.replaces, Some("old-module".to_string()));
        assert_eq!(data.deprecated, Some(true));
    }

    #[test]
    fn frontmatter_data_is_generated() {
        let mut data = FrontmatterData::default();
        assert!(!data.is_generated());

        data.generated = Some(true);
        assert!(data.is_generated());

        data.generated = None;
        data.generated_by = Some("rgistr".to_string());
        assert!(data.is_generated());
    }

    #[test]
    fn get_generated_from_frontmatter_explicit_true() {
        let content = "---\ngenerated: true\n---\n# Doc";
        assert_eq!(get_generated_from_frontmatter(content), Some(true));
    }

    #[test]
    fn get_generated_from_frontmatter_explicit_false() {
        let content = "---\ngenerated: false\n---\n# Doc";
        assert_eq!(get_generated_from_frontmatter(content), Some(false));
    }

    #[test]
    fn get_generated_from_frontmatter_generated_by_implies_true() {
        let content = "---\ngenerated_by: rgistr\n---\n# Doc";
        assert_eq!(get_generated_from_frontmatter(content), Some(true));
    }

    #[test]
    fn get_generated_from_frontmatter_silent() {
        let content = "---\ntitle: Authored\n---\n# Doc";
        assert_eq!(get_generated_from_frontmatter(content), None);
    }

    #[test]
    fn get_generated_from_frontmatter_no_frontmatter() {
        let content = "# Doc without frontmatter";
        assert_eq!(get_generated_from_frontmatter(content), None);
    }

    #[test]
    fn is_generated_frontmatter_overrides_path() {
        // MAP.md is generated by path, but explicit false in frontmatter overrides
        let content = "---\ngenerated: false\n---\n# Authored MAP";
        assert!(!is_generated("MAP.md", content));
        assert!(!is_generated("src/core/MAP.md", content));
    }

    #[test]
    fn is_generated_silent_frontmatter_not_generated() {
        // MAP.md with silent frontmatter is NOT generated.
        // Path alone is insufficient when content is available.
        let content = "---\ntitle: Map\n---\n# Core Module";
        assert!(!is_generated("MAP.md", content));
    }

    #[test]
    fn is_generated_no_frontmatter_not_generated() {
        // MAP.md with no frontmatter is NOT generated.
        // Readable content without evidence → authored.
        let content = "# Core Module\n\nHuman-written docs.";
        assert!(!is_generated("MAP.md", content));
    }

    #[test]
    fn is_generated_readme_with_generated_true() {
        // README.md not generated by path, but frontmatter says true
        let content = "---\ngenerated: true\n---\n# Auto-generated readme";
        assert!(is_generated("README.md", content));
    }
}
