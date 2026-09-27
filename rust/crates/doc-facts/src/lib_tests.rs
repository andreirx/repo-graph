//! Tests for `lib.rs` (moved out per the 500-line guardrail; SELF-POLLUTION-1 review-6 #3).

use super::*;
use std::fs::{self, File};
use std::io::Write;
use tempfile::tempdir;

fn create_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut f = File::create(path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

#[test]
fn extract_from_repo_with_readme() {
    let dir = tempdir().unwrap();
    create_file(
        dir.path(),
        "README.md",
        "# Test\n\n<!-- rg:replaces old-module -->\n",
    );

    let result = extract_semantic_facts(dir.path()).unwrap();

    assert_eq!(result.files_scanned, 1);
    assert_eq!(result.facts.len(), 1);
    assert_eq!(result.facts[0].fact_kind, FactKind::ReplacementFor);
}

#[test]
fn extract_from_repo_with_docker_compose() {
    let dir = tempdir().unwrap();
    create_file(
        dir.path(),
        "docker-compose.yml",
        "services:\n  api:\n    image: api:latest\n",
    );

    let result = extract_semantic_facts(dir.path()).unwrap();

    assert_eq!(result.files_scanned, 1);
    assert_eq!(result.facts.len(), 1);
    assert_eq!(result.facts[0].fact_kind, FactKind::EnvironmentSurface);
}

#[test]
fn extract_detects_generated_from_frontmatter() {
    let dir = tempdir().unwrap();
    create_file(
        dir.path(),
        "src/core/MAP.md",
        "---\ngenerated_by: rgistr\n---\n# Core\n\n<!-- rg:replaces legacy -->\n",
    );

    let result = extract_semantic_facts(dir.path()).unwrap();

    assert_eq!(result.generated_docs_count, 1);
    assert!(result.facts[0].generated);
}

#[test]
fn extract_handles_unreadable_files() {
    let dir = tempdir().unwrap();
    create_file(dir.path(), "README.md", "# OK");
    // Create a directory with the same name as a file pattern
    // This simulates an unreadable situation
    fs::create_dir_all(dir.path().join("docs")).unwrap();
    create_file(dir.path(), "docs/guide.md", "# Guide");

    let result = extract_semantic_facts(dir.path()).unwrap();

    // Should succeed even if some paths are tricky
    assert!(result.files_scanned >= 1);
}

#[test]
fn error_on_non_directory() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("file.txt");
    File::create(&file_path).unwrap();

    let result = extract_semantic_facts(&file_path);

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DocFactsError::NotADirectory(_)
    ));
}

#[test]
fn files_by_kind_counts() {
    let dir = tempdir().unwrap();
    create_file(dir.path(), "README.md", "# Readme");
    create_file(dir.path(), "ARCHITECTURE.md", "# Arch");
    create_file(dir.path(), ".env", "FOO=bar");
    create_file(dir.path(), ".env.production", "FOO=prod");

    let result = extract_semantic_facts(dir.path()).unwrap();

    assert_eq!(result.files_by_kind.get(&DocKind::Readme), Some(&1));
    assert_eq!(result.files_by_kind.get(&DocKind::Architecture), Some(&1));
    assert_eq!(result.files_by_kind.get(&DocKind::Config), Some(&2));
}

#[test]
fn authored_map_md_explicit_false_not_generated() {
    // P2 fix: MAP.md with explicit `generated: false` is not marked as generated
    let dir = tempdir().unwrap();
    create_file(
        dir.path(),
        "src/core/MAP.md",
        "---\ngenerated: false\ntitle: Core Module\n---\n# Core\n\nAuthored documentation.\n",
    );

    let result = extract_semantic_facts(dir.path()).unwrap();

    assert_eq!(result.files_scanned, 1);
    // Frontmatter `generated: false` overrides path-based detection
    assert_eq!(result.generated_docs_count, 0);
}

#[test]
fn authored_map_md_silent_frontmatter_not_generated() {
    // P2 fix: MAP.md with silent frontmatter (no generated field) is NOT generated.
    // Path alone is not strong enough provenance.
    let dir = tempdir().unwrap();
    create_file(
        dir.path(),
        "src/core/MAP.md",
        "---\ntitle: Core Module\nauthor: human\n---\n# Core\n\nAuthored documentation.\n",
    );

    let result = extract_semantic_facts(dir.path()).unwrap();

    assert_eq!(result.files_scanned, 1);
    // Silent frontmatter means no evidence of generation → not generated
    assert_eq!(result.generated_docs_count, 0);
}

#[test]
fn authored_map_md_no_frontmatter_not_generated() {
    // P2 fix: MAP.md with no frontmatter at all is NOT generated.
    // Readable content without generation evidence → authored.
    let dir = tempdir().unwrap();
    create_file(
        dir.path(),
        "src/core/MAP.md",
        "# Core Module\n\nThis is human-written documentation.\n",
    );

    let result = extract_semantic_facts(dir.path()).unwrap();

    assert_eq!(result.files_scanned, 1);
    // No frontmatter, content readable → not generated
    assert_eq!(result.generated_docs_count, 0);
}

#[test]
fn env_files_excluded_from_inventory() {
    // SELF-POLLUTION-1 §3: `.env*` never appears in the doc inventory (and so
    // never in orient's Docs line), even though it is a discovery candidate.
    let dir = tempdir().unwrap();
    create_file(dir.path(), "README.md", "# Readme");
    create_file(dir.path(), ".env", "SECRET=1");
    create_file(dir.path(), ".env.test", "SECRET=2");

    let result = discover_doc_inventory(dir.path(), false).unwrap();
    let paths: Vec<_> = result.entries.iter().map(|e| e.path.as_str()).collect();

    assert!(paths.contains(&"README.md"));
    assert!(!paths.iter().any(|p| p.starts_with(".env")), "{paths:?}");
}

#[test]
fn inventory_flags_generated_only_with_marker() {
    // SELF-POLLUTION-1: a rmap-generated MAP.md carries the first-line marker
    // → generated. A user's own MAP.md without the marker → NOT generated
    // (name-collision honesty), so it is not excluded downstream.
    let dir = tempdir().unwrap();
    create_file(
        dir.path(),
        "src/a/MAP.md",
        "<!-- generated by rmap map from snapshot snap-0; do not hand-edit -->\n# A\n",
    );
    create_file(
        dir.path(),
        "src/b/MAP.md",
        "# Hand-authored map\n\nHuman notes.\n",
    );

    let result = discover_doc_inventory(dir.path(), false).unwrap();

    let gen = result
        .entries
        .iter()
        .find(|e| e.path == "src/a/MAP.md")
        .expect("generated MAP.md present");
    let user = result
        .entries
        .iter()
        .find(|e| e.path == "src/b/MAP.md")
        .expect("user MAP.md present");
    assert!(gen.generated, "marker MAP.md is generated");
    assert!(!user.generated, "marker-less MAP.md is NOT generated");
    assert_eq!(result.generated_count, 1);
}

#[test]
fn inventory_flags_foreign_frontmatter_map_generated() {
    // FIXTURE-POLLUTION-1 §2.4: a foreign generated map (e.g. legacy `rgistr` output
    // dropped under smoke-runs/**) carries a FRONTMATTER generation marker
    // (`generated_by: rgistr`), NOT the current rmap first-line HTML marker. It must be
    // classified generated (→ excluded downstream) by that content marker — never listed
    // as authored `architecture`. An explicit `generated: false` still wins (authored).
    let dir = tempdir().unwrap();
    create_file(
        dir.path(),
        "smoke-runs/foreign/mod_c_MAP.md",
        "---\ngenerated_by: rgistr\ngenerator_version: 0.2.0\nscope: file\n---\n# Purpose\nForeign LLM map.\n",
    );
    create_file(
        dir.path(),
        "smoke-runs/foreign/synth_MAP.md",
        "---\nkind: synthesized_summary\n---\n# Summary\n",
    );
    create_file(
        dir.path(),
        "docs/authored_MAP.md",
        "---\ngenerated: false\ntitle: Hand map\n---\n# Authored\n",
    );

    let result = discover_doc_inventory(dir.path(), false).unwrap();
    let find = |p: &str| {
        result
            .entries
            .iter()
            .find(|e| e.path == p)
            .unwrap_or_else(|| panic!("{p} present"))
    };
    assert!(
        find("smoke-runs/foreign/mod_c_MAP.md").generated,
        "rgistr frontmatter map is generated (excluded from the listing)"
    );
    assert!(
        find("smoke-runs/foreign/synth_MAP.md").generated,
        "synthesized_summary map is generated"
    );
    assert!(
        !find("docs/authored_MAP.md").generated,
        "explicit generated:false stays authored"
    );
    assert_eq!(result.generated_count, 2);
}

#[test]
#[cfg(unix)]
fn inventory_unreadable_sidecar_is_admitted_and_counted_never_asserted_generated() {
    // operator RULING 3 / review-5 finding 3: a sidecar-NAMED file we cannot READ
    // (permission denied — a genuine failure, NOT `NotFound`) must be ADMITTED to
    // the inventory (conservative, never silently excluded) and left
    // `generated = false`, but COUNTED as unreadable — never a silent "not
    // generated" assertion. Distinct from a marker-less readable MAP.md, which is
    // authored with certainty and is NOT counted unreadable.
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    create_file(dir.path(), "README.md", "# Readme");
    create_file(
        dir.path(),
        "src/gen/MAP.md",
        "<!-- generated by rmap map from snapshot snap-0; do not hand-edit -->\n",
    );
    create_file(dir.path(), "src/user/MAP.md", "# hand-authored\n");
    // A sidecar-named file made unreadable (mode 000) → read fails PermissionDenied.
    let blocked = dir.path().join("src/blocked/MAP.md");
    create_file(dir.path(), "src/blocked/MAP.md", "whatever");
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).unwrap();

    let result = discover_doc_inventory(dir.path(), false).unwrap();

    // Restore permissions so tempdir cleanup can remove the file.
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o644)).unwrap();

    let find = |p: &str| {
        result
            .entries
            .iter()
            .find(|e| e.path == p)
            .unwrap_or_else(|| panic!("{p} present in inventory"))
    };
    // All three sidecars ADMITTED (none silently excluded).
    assert!(
        !find("src/blocked/MAP.md").generated,
        "unreadable → not asserted generated"
    );
    assert!(find("src/gen/MAP.md").generated, "marker → generated");
    assert!(
        !find("src/user/MAP.md").generated,
        "marker-less readable → authored"
    );
    // Exactly the unreadable one is counted as unknown; the readable authored one is not.
    assert_eq!(
        result.unreadable_count, 1,
        "only the unreadable sidecar is counted unknown"
    );
    // Only the marker sidecar is generated.
    assert_eq!(result.generated_count, 1);
}

#[test]
fn inventory_license_is_name_only_content_body_is_not_upgraded() {
    // AUDIT5-MINORS-1 F1 (operator ruling, iteration 3): `license` is NAME ONLY. The former CONTENT
    // basis is removed — a headingless doc that embeds an MIT/BSD/Apache body clause plus other
    // prose is NOT a license (the reviewer's required negative case). Only a dedicated
    // LICENSE*/COPYING*/NOTICE* FILENAME is `license`.
    let dir = tempdir().unwrap();
    // The reviewer's precise regression case (cycle-4/5): a NON-license filename whose body embeds a
    // full MIT license clause PLUS other prose is NOT a license. The NAME (`terms.txt`) is not a
    // license filename, and content is not consulted for the `license` kind at all → it stays the
    // neutral docs-tree `doc` kind (cycle-5 ruling: docs-tree prose is `doc`, not `architecture`).
    create_file(
        dir.path(),
        "docs/legal/terms.txt",
        "# Contribution Terms\n\nBy contributing you agree to the following.\n\nThe MIT License applies:\n\nPermission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the \"Software\"), to deal in the Software without restriction.\n\nQuestions? Email legal@example.com — this document also covers our code of conduct and review process.\n",
    );
    // A dedicated LICENSE-named file → license by NAME, regardless of its (here trivial) content.
    create_file(
        dir.path(),
        "docs/legal/LICENSE.txt",
        "Apache License\nVersion 2.0",
    );
    // A non-license-named guide under docs/ → neutral `doc` (no license name prefix, no architecture).
    create_file(dir.path(), "docs/NOTLICENSE.txt", "# Just a guide.\n");

    let result = discover_doc_inventory(dir.path(), true).unwrap();
    let find = |p: &str| {
        result
            .entries
            .iter()
            .find(|e| e.path == p)
            .unwrap_or_else(|| panic!("{p} present"))
    };
    assert_eq!(
        find("docs/legal/terms.txt").kind,
        "doc",
        "MIT clause + other prose but non-license NAME → NOT license (NAME ONLY, content path removed)"
    );
    assert_eq!(
        find("docs/legal/LICENSE.txt").kind,
        "license",
        "LICENSE-named file → license by name"
    );
    assert_eq!(
        find("docs/NOTLICENSE.txt").kind,
        "doc",
        "non-license name, no matching prefix → NOT license (neutral doc)"
    );
    assert_eq!(
        result.unreadable_count, 0,
        "all content read → nothing unknown"
    );
}

#[test]
fn inventory_release_notes_need_an_inspected_toctree_manifest_not_just_the_dir_name() {
    // DOCS-LIST-2 §2 (review-1 item 1 + review-2 item 1): the `release-notes` kind's STRUCTURAL basis.
    // Three repos with the SAME `docs/releases/1.4.x.txt` path differ only by the subtree's manifest
    // evidence:
    //   (A) index.txt whose CONTENT carries a `.. toctree::` directive → release-notes (grouped);
    //   (B) index.txt present but its CONTENT is just a heading (NO toctree) → stays neutral `doc`
    //       (review-2 item 1: a file merely NAMED index.* is not structural evidence);
    //   (C) no index.* at all → stays neutral `doc`.
    // `compute_hashes == false` throughout, proving the BOUNDED on-demand manifest read (the orient
    // path) confirms without reading the whole tree — the file's NAME is never the basis.
    let find = |r: &DocInventoryResult, p: &str| {
        r.entries
            .iter()
            .find(|e| e.path == p)
            .unwrap_or_else(|| panic!("{p} present"))
            .clone()
    };

    // (A) A genuine Sphinx manifest: the index enumerates releases with a toctree directive.
    let with_toctree = tempdir().unwrap();
    create_file(
        with_toctree.path(),
        "docs/releases/index.txt",
        "Release notes\n=============\n\n.. toctree::\n   :maxdepth: 1\n\n   1.4.x\n",
    );
    create_file(
        with_toctree.path(),
        "docs/releases/1.4.x.txt",
        "Django 1.4 release notes\n",
    );
    create_file(with_toctree.path(), "docs/design.md", "# Architecture\n");
    // review-4 item 1: the `compute_hashes == false` (orient) path performs NO on-demand
    // manifest read — no loaded content is NO BASIS, so the kind stays `architecture` (the
    // license-kind discipline). Confirmation happens only on the content-loading path below.
    let unconfirmed = discover_doc_inventory(with_toctree.path(), false).unwrap();
    let note = find(&unconfirmed, "docs/releases/1.4.x.txt");
    assert_eq!(
        note.kind, "doc",
        "no loaded content (compute_hashes=false) → no basis → old (now neutral doc) kind"
    );
    let confirmed = discover_doc_inventory(with_toctree.path(), true).unwrap();
    let note = find(&confirmed, "docs/releases/1.4.x.txt");
    assert_eq!(
        note.kind, "release-notes",
        "inspected toctree manifest present (content loaded) → release-notes"
    );
    assert_eq!(
        note.release_family.as_deref(),
        Some("docs/releases"),
        "the grouping family is the confirmed subtree"
    );
    assert_eq!(
        find(&confirmed, "docs/design.md").kind,
        "architecture",
        "a non-release doc is untouched"
    );

    // (B) review-2 item 1 negative: an index.* WITHOUT a toctree directive is not a manifest — the
    // release-named dir + a file named index.txt is still just NAMES. The doc keeps neutral `doc`.
    let non_manifest_index = tempdir().unwrap();
    create_file(
        non_manifest_index.path(),
        "docs/releases/index.txt",
        "Releases\n========\n",
    );
    create_file(
        non_manifest_index.path(),
        "docs/releases/1.4.x.txt",
        "Django 1.4 release notes\n",
    );
    let unconfirmed_by_content = discover_doc_inventory(non_manifest_index.path(), false).unwrap();
    let not_a_note = find(&unconfirmed_by_content, "docs/releases/1.4.x.txt");
    assert_eq!(
        not_a_note.kind, "doc",
        "index.* without a toctree directive → NOT release-notes (name is not evidence); neutral doc"
    );
    assert_eq!(
        not_a_note.release_family, None,
        "unconfirmed by content → no grouping family"
    );

    // (C) No manifest index in the subtree at all → structural evidence absent → keeps architecture.
    let no_index = tempdir().unwrap();
    create_file(
        no_index.path(),
        "docs/releases/1.4.x.txt",
        "Django 1.4 release notes\n",
    );
    let unconfirmed = discover_doc_inventory(no_index.path(), false).unwrap();
    let bare = find(&unconfirmed, "docs/releases/1.4.x.txt");
    assert_eq!(
        bare.kind, "doc",
        "no manifest index → release-named dir alone is NOT release-notes; neutral doc"
    );
    assert_eq!(
        bare.release_family, None,
        "unconfirmed → no grouping family"
    );
}

#[test]
#[cfg(unix)]
fn inventory_unreadable_license_named_doc_is_license_by_name_and_still_counted() {
    // AUDIT5-MINORS-1 F1 (supersedes DOCS-LIST-2 review-0 F4's content-only stance for dedicated
    // license FILENAMES): `license` now has a NAME basis (`LICENSE*`/`COPYING*`/`NOTICE*`) in
    // addition to the content basis. A file literally named `LICENSE.txt` IS a license document —
    // the name is the deterministic structural evidence — so it classifies `license` even when its
    // content read FAILS (no content basis needed). The read failure is STILL surfaced through the
    // unreadable count (honesty rule #1), never silently swallowed.
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    create_file(dir.path(), "README.md", "# Readme");
    let blocked = dir.path().join("docs/legal/LICENSE.txt");
    create_file(
        dir.path(),
        "docs/legal/LICENSE.txt",
        "Apache License\nVersion 2.0",
    );
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).unwrap();

    let result = discover_doc_inventory(dir.path(), true).unwrap();

    // Restore permissions so tempdir cleanup can remove the file.
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o644)).unwrap();

    let blocked_entry = result
        .entries
        .iter()
        .find(|e| e.path == "docs/legal/LICENSE.txt")
        .expect("unreadable doc still admitted to inventory");
    // NAME basis: a `LICENSE*`-named file is `license` even unreadable (F1).
    assert_eq!(
        blocked_entry.kind, "license",
        "LICENSE-named file is license by name, no content read required (F1)"
    );
    assert_eq!(
        result.unreadable_count, 1,
        "the unreadable non-sidecar doc is STILL counted UNKNOWN (read failure surfaced)"
    );
}

#[test]
fn nested_env_file_has_module_scope() {
    // P1 fix: nested .env files use parent directory as subject, not repo root
    let dir = tempdir().unwrap();
    create_file(
        dir.path(),
        "frontend/web/.env.prod",
        "API_URL=https://prod.api.example.com",
    );

    let result = extract_semantic_facts(dir.path()).unwrap();

    assert_eq!(result.facts.len(), 1);
    assert_eq!(result.facts[0].subject_ref, "frontend/web");
}

// DOCS-DISCOVERY-1 (RG-REQ-008-L06): the inventory states how many MARKUP files the discovery rule
// refused, and carries the rule itself, so a widened scope counts its own refusals. `.txt` outside a
// docs tree is never counted (D-DD1-002 — `CMakeLists.txt` is not known to be prose).
#[test]
fn inventory_reports_unscanned_markup_outside_docs_tree() {
    let dir = tempdir().unwrap();
    create_file(dir.path(), "README.md", "# Root");
    create_file(
        dir.path(),
        "notes.md",
        "# refused: markup outside a docs tree",
    );
    create_file(
        dir.path(),
        "CMakeLists.txt",
        "cmake_minimum_required(VERSION 3.9)",
    );
    create_file(dir.path(), "docs/a.md", "# A");

    let result = discover_doc_inventory(dir.path(), true).unwrap();
    assert_eq!(result.unscanned_markup_outside_docs_tree, 1);
    let mut paths: Vec<&str> = result.entries.iter().map(|e| e.path.as_str()).collect();
    paths.sort();
    assert_eq!(paths, vec!["README.md", "docs/a.md"]);

    let rule = discovery_rule();
    assert_eq!(rule.stems, discovery::DOC_NAME_STEMS);
    assert_eq!(
        rule.extensions,
        &[".md", ".markdown", ".txt", ".rst", ".adoc"]
    );
    assert_eq!(rule.doc_tree_dirs, &["docs", "doc", "design"]);
    assert_eq!(rule.doc_tree_conventions, &["src/site"]);
    assert_eq!(
        rule.unscanned_extensions,
        &[".md", ".markdown", ".rst", ".adoc"]
    );
}

// ── DOCS-UNREADABLE-DECODE-1 (RG-REQ-008-L05, RG-REQ-008-L08 as amended by D-DU-CC10) ──────────
//
// The content hash is the sha256 of the file's BYTES; the text is decoded separately. A document
// whose bytes are not UTF-8 keeps a real hash, gets no content and is counted unreadable; a
// document whose bytes could not be read at all has no hash, and its entry still SERIALIZES the
// `content_hash` key (as null) — an unknown marked, never omitted, never fabricated.
//
// Serialization is asserted through `serde_yaml` (this crate's existing serializer dependency):
// whether the key is emitted is decided by the derived `Serialize` impl (`skip_serializing_if`),
// independently of the output format, and the frozen Cargo.toml carries no `serde_json`.

fn create_bytes(dir: &Path, name: &str, bytes: &[u8]) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, bytes).unwrap();
}

/// The serialized `content_hash` of an entry: `Some(Some(s))` a string, `Some(None)` the key
/// present with null, `None` the key absent.
fn serialized_content_hash(entry: &DocInventoryEntry) -> Option<Option<String>> {
    let value = serde_yaml::to_value(entry).expect("entry serializes");
    let map = value.as_mapping().expect("entry serializes as a mapping");
    map.get(serde_yaml::Value::String("content_hash".to_string()))
        .map(|v| match v {
            serde_yaml::Value::Null => None,
            serde_yaml::Value::String(s) => Some(s.clone()),
            other => panic!("content_hash is neither a string nor null: {other:?}"),
        })
}

fn inventory_entry<'a>(result: &'a DocInventoryResult, path: &str) -> &'a DocInventoryEntry {
    result
        .entries
        .iter()
        .find(|e| e.path == path)
        .unwrap_or_else(|| panic!("{path} admitted to the inventory"))
}

/// poco's `packaging/README.txt` is Windows-1252 (`Apple\x92s Cocoa`).
const WINDOWS_1252_BYTES: &[u8] = b"Apple\x92s Cocoa\n";
/// sha256 of [`WINDOWS_1252_BYTES`], computed independently of the implementation.
const WINDOWS_1252_SHA256: &str =
    "00873e70bc71b05cc510c80ff317e1f2ed5a82eee47b0405b83a3c2e21a90109";

#[test]
fn undecodable_utf8_document_keeps_a_byte_hash_and_counts_unreadable() {
    let dir = tempdir().unwrap();
    create_file(dir.path(), "README.md", "# Readable\n");
    create_bytes(dir.path(), "packaging/README.txt", WINDOWS_1252_BYTES);
    // A sidecar-NAMED undecodable file: counted once, by the sidecar pass, never twice.
    create_bytes(dir.path(), "src/gen/MAP.md", b"\x92 map\n");

    let result = discover_doc_inventory(dir.path(), true).unwrap();

    let mut paths: Vec<&str> = result.entries.iter().map(|e| e.path.as_str()).collect();
    paths.sort();
    assert_eq!(
        paths,
        vec!["README.md", "packaging/README.txt", "src/gen/MAP.md"],
        "every document admitted, the undecodable ones included"
    );
    let undecodable = inventory_entry(&result, "packaging/README.txt");
    assert_eq!(
        undecodable.content_hash.as_deref(),
        Some(WINDOWS_1252_SHA256),
        "the hash is the sha256 of the file's bytes"
    );
    assert_eq!(undecodable.kind, "readme", "kind by name, as before");
    assert!(!undecodable.generated);
    let sidecar = inventory_entry(&result, "src/gen/MAP.md");
    assert!(
        !sidecar.generated,
        "an undecodable sidecar is not asserted generated"
    );
    assert_eq!(
        result.unreadable_count, 2,
        "the undecodable README and the undecodable sidecar, each counted exactly once"
    );
    for entry in &result.entries {
        assert!(
            matches!(serialized_content_hash(entry), Some(Some(_))),
            "{} serializes a string content_hash",
            entry.path
        );
    }
}

#[test]
fn readable_document_hash_is_unchanged_by_the_byte_read() {
    let dir = tempdir().unwrap();
    create_file(dir.path(), "README.md", "# Résumé — ✓\n");

    let result = discover_doc_inventory(dir.path(), true).unwrap();

    assert_eq!(
        inventory_entry(&result, "README.md")
            .content_hash
            .as_deref(),
        Some("ae9c3d6d18a8667af95c59681aa94026b17feafb9d7af579331995b26dbd1155"),
        "a valid UTF-8 document hashes exactly as before (sha256 of its bytes)"
    );
    assert_eq!(result.unreadable_count, 0);
}

#[test]
fn extract_warns_on_an_undecodable_document() {
    let dir = tempdir().unwrap();
    create_file(
        dir.path(),
        "README.md",
        "# OK\n\n<!-- rg:replaces old-module -->\n",
    );
    create_bytes(
        dir.path(),
        "packaging/README.txt",
        b"<!-- rg:replaces other-module -->\nApple\x92s Cocoa\n",
    );

    let result = extract_semantic_facts(dir.path()).unwrap();

    let expected_error = fs::read_to_string(dir.path().join("packaging/README.txt")).unwrap_err();
    assert_eq!(result.files_scanned, 2);
    assert_eq!(
        result.warnings.len(),
        1,
        "one warning: {:?}",
        result.warnings
    );
    assert_eq!(result.warnings[0].file, "packaging/README.txt");
    assert_eq!(
        result.warnings[0].message,
        format!("failed to read: {}", expected_error),
        "the warning reads exactly as the text read's error always did"
    );
    assert_eq!(
        result.facts.len(),
        1,
        "nothing is extracted from the undecodable document: {:?}",
        result.facts
    );
    assert_eq!(
        result.facts[0].source_file, "README.md",
        "the one fact comes from the readable document"
    );
}

#[test]
#[cfg(unix)]
fn entry_with_unread_bytes_serializes_content_hash_null_with_the_key_present() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    create_file(dir.path(), "README.md", "# Readable\n");
    let blocked = dir.path().join("docs/NOTES.md");
    create_file(dir.path(), "docs/NOTES.md", "private notes\n");
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).unwrap();

    let result = discover_doc_inventory(dir.path(), true).unwrap();

    // Restore permissions so tempdir cleanup can remove the file.
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o644)).unwrap();

    let unread = inventory_entry(&result, "docs/NOTES.md");
    assert_eq!(unread.content_hash, None, "no truthful hash exists");
    assert_eq!(unread.kind, "doc");
    assert!(!unread.generated);
    assert_eq!(result.unreadable_count, 1);
    assert_eq!(
        serialized_content_hash(unread),
        Some(None),
        "the key is serialized, with null"
    );
    assert!(
        serde_yaml::to_string(unread)
            .unwrap()
            .lines()
            .any(|l| l == "content_hash: null"),
        "the serialized entry carries `content_hash: null`"
    );
    assert!(
        matches!(
            serialized_content_hash(inventory_entry(&result, "README.md")),
            Some(Some(_))
        ),
        "a readable entry serializes a string"
    );
}

#[test]
fn entry_with_undecodable_bytes_serializes_its_byte_hash() {
    let dir = tempdir().unwrap();
    create_bytes(dir.path(), "README.txt", WINDOWS_1252_BYTES);

    let result = discover_doc_inventory(dir.path(), true).unwrap();

    assert_eq!(
        serialized_content_hash(inventory_entry(&result, "README.txt")),
        Some(Some(WINDOWS_1252_SHA256.to_string())),
        "the undecodable entry serializes the sha256 of its bytes"
    );
    // `release_family` keeps its skip: absent on a non-release entry.
    let value = serde_yaml::to_value(inventory_entry(&result, "README.txt")).unwrap();
    assert!(
        value
            .as_mapping()
            .unwrap()
            .get(serde_yaml::Value::String("release_family".to_string()))
            .is_none(),
        "release_family stays omitted when None"
    );
}
