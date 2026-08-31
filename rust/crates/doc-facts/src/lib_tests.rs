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
