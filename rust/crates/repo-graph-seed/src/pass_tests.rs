//! Unit tests for the pure chunk-embed pipeline (`pass::build_store`). Driven with
//! a fake `Embedder` + in-memory file reader — no model, no daemon, no DB.

use super::*;
use crate::document::build_chunk_document;
use crate::hash::content_hash;
use crate::ports::{EmbedError, Embedder, SeedCorpusEntry, SeedForwardDecl, SeedVectorEntry};
use std::collections::HashMap;

/// The copy-forward reuse digest for a chunk (review-3): the hash of the ASSEMBLED document
/// `build_chunk_document(qname, doc, span)` — the exact bytes the embedder saw. A test helper so a
/// prior entry can carry the EXACT digest `build_store` computes for the matching current chunk.
fn doc_hash(qname: Option<&str>, doc: Option<&str>, span: &str) -> String {
    content_hash(&build_chunk_document(qname, doc, span))
}

/// A fake embedder: returns a fixed 2-dim vector per input and records the docs it
/// was asked to embed (so tests can assert copy-forward skipped the reused ones).
struct FakeEmbedder {
    seen: std::cell::RefCell<Vec<String>>,
}
impl FakeEmbedder {
    fn new() -> Self {
        Self {
            seen: std::cell::RefCell::new(Vec::new()),
        }
    }
}
impl Embedder for FakeEmbedder {
    fn model_id(&self) -> &str {
        "fake"
    }
    fn dim(&self) -> usize {
        2
    }
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        for t in texts {
            self.seen.borrow_mut().push(t.clone());
        }
        // A non-zero vector so it survives normalization + the >0 rank floor.
        Ok(texts.iter().map(|_| vec![1.0, 1.0]).collect())
    }
}

fn chunk(
    node: &str,
    path: &str,
    hash: &str,
    line_start: Option<i64>,
    line_end: Option<i64>,
    is_test: bool,
) -> SeedCorpusEntry {
    // Default subtype FUNCTION so the decl/impl gate (callables only) is exercised;
    // tests that care about the subtype set it explicitly via `..chunk(..)`.
    SeedCorpusEntry {
        node_uid: node.to_string(),
        stable_key: format!("k:{node}"),
        file_uid: format!("fu:{path}"),
        path: path.to_string(),
        qualified_name: Some(node.to_string()),
        subtype: Some("FUNCTION".to_string()),
        doc_comment: None,
        line_start,
        line_end,
        is_test,
        content_hash: hash.to_string(),
        forward_decl: SeedForwardDecl::Definition,
    }
}

fn reader(files: HashMap<String, String>) -> impl Fn(&str) -> std::io::Result<String> {
    move |p: &str| {
        files
            .get(p)
            .cloned()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"))
    }
}

#[test]
fn admits_matching_chunks_and_embeds_each() {
    let content = "line1\nline2\nline3\n";
    let h = content_hash(content);
    let entries = vec![
        chunk("a", "src/x.rs", &h, Some(1), Some(2), false),
        chunk("b", "src/x.rs", &h, Some(2), Some(3), false),
    ];
    let mut files = HashMap::new();
    files.insert("src/x.rs".to_string(), content.to_string());
    let emb = FakeEmbedder::new();
    let outcome = build_store(
        entries,
        &emb,
        reader(files),
        || false,
        BuildConfig::default(),
        &[],
    );
    match outcome {
        BuildOutcome::Built { entries, report } => {
            assert_eq!(report.admitted, 2);
            assert_eq!(report.reused, 0);
            assert_eq!(report.drifted, 0);
            assert_eq!(entries.len(), 2);
            assert_eq!(emb.seen.borrow().len(), 2, "both chunks embedded");
            for e in &entries {
                assert_eq!(e.vector.len(), 2);
                let norm: f32 = e.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
                assert!((norm - 1.0).abs() < 1e-5, "stored vectors are normalized");
            }
        }
        other => panic!("expected Built, got {other:?}"),
    }
}

#[test]
fn drifted_file_omits_the_whole_run() {
    // The file on disk hashes differently from the snapshot pin → omit all chunks.
    let entries = vec![chunk("a", "src/x.rs", "STALE_PIN", Some(1), Some(1), false)];
    let mut files = HashMap::new();
    files.insert("src/x.rs".to_string(), "current content".to_string());
    let emb = FakeEmbedder::new();
    match build_store(
        entries,
        &emb,
        reader(files),
        || false,
        BuildConfig::default(),
        &[],
    ) {
        BuildOutcome::Built { entries, report } => {
            assert_eq!(report.admitted, 0);
            assert_eq!(report.drifted, 1);
            assert!(entries.is_empty());
            assert_eq!(emb.seen.borrow().len(), 0, "nothing embedded on drift");
        }
        other => panic!("expected Built, got {other:?}"),
    }
}

#[test]
fn unreadable_file_drifts_its_chunks() {
    let entries = vec![chunk("a", "missing.rs", "h", Some(1), Some(1), false)];
    let emb = FakeEmbedder::new();
    match build_store(
        entries,
        &emb,
        reader(HashMap::new()),
        || false,
        BuildConfig::default(),
        &[],
    ) {
        BuildOutcome::Built { report, .. } => {
            assert_eq!(report.drifted, 1);
            assert_eq!(report.admitted, 0);
        }
        other => panic!("expected Built, got {other:?}"),
    }
}

#[test]
fn node_without_span_contributes_no_chunk() {
    let content = "one line\n";
    let h = content_hash(content);
    let entries = vec![chunk("a", "x.rs", &h, None, None, false)];
    let mut files = HashMap::new();
    files.insert("x.rs".to_string(), content.to_string());
    let emb = FakeEmbedder::new();
    match build_store(
        entries,
        &emb,
        reader(files),
        || false,
        BuildConfig::default(),
        &[],
    ) {
        BuildOutcome::Built { report, entries } => {
            assert_eq!(report.admitted, 0);
            assert_eq!(report.drifted, 1, "no span → omitted");
            assert!(entries.is_empty());
        }
        other => panic!("expected Built, got {other:?}"),
    }
}

#[test]
fn copy_forward_reuses_unchanged_chunk_and_skips_the_embed() {
    let content = "fn a() {}\nfn b() {}\n";
    let h = content_hash(content);
    let entries = vec![
        chunk("a", "x.rs", &h, Some(1), Some(1), false),
        chunk("b", "x.rs", &h, Some(2), Some(2), false),
    ];
    let mut files = HashMap::new();
    files.insert("x.rs".to_string(), content.to_string());
    // Prior snapshot embedded chunk "a" (same stable_key + content_hash + document_hash).
    // The document is unchanged (qname "a", doc None, span "fn a() {}"), so the digest matches and
    // the vector copies forward (review-3: reuse now also requires the document digest to match).
    let prior = vec![SeedVectorEntry {
        node_uid: "a_prev".to_string(),
        stable_key: "k:a".to_string(),
        file_uid: "fu:x.rs".to_string(),
        path: "x.rs".to_string(),
        line: Some(1),
        qualified_name: Some("a".to_string()),
        is_test: false,
        is_decl: false,
        is_field: false,
        content_hash: h.clone(),
        document_hash: Some(doc_hash(Some("a"), None, "fn a() {}")),
        vector: vec![0.6, 0.8], // already normalized
    }];
    let emb = FakeEmbedder::new();
    match build_store(
        entries,
        &emb,
        reader(files),
        || false,
        BuildConfig::default(),
        &prior,
    ) {
        BuildOutcome::Built { entries, report } => {
            assert_eq!(report.admitted, 2);
            assert_eq!(report.reused, 1, "chunk a copied forward");
            assert_eq!(emb.seen.borrow().len(), 1, "only chunk b embedded");
            // The reused row carries the prior vector verbatim + THIS snapshot's node_uid.
            let a = entries.iter().find(|e| e.stable_key == "k:a").unwrap();
            assert_eq!(a.vector, vec![0.6, 0.8]);
            assert_eq!(
                a.node_uid, "a",
                "reused vector, current snapshot node identity"
            );
        }
        other => panic!("expected Built, got {other:?}"),
    }
}

#[test]
fn model_change_forces_full_reembed() {
    // The daemon passes an EMPTY prior (model filtered it out); nothing reuses.
    let content = "fn a() {}\n";
    let h = content_hash(content);
    let entries = vec![chunk("a", "x.rs", &h, Some(1), Some(1), false)];
    let mut files = HashMap::new();
    files.insert("x.rs".to_string(), content.to_string());
    let emb = FakeEmbedder::new();
    match build_store(
        entries,
        &emb,
        reader(files),
        || false,
        BuildConfig::default(),
        &[],
    ) {
        BuildOutcome::Built { report, .. } => {
            assert_eq!(report.reused, 0);
            assert_eq!(emb.seen.borrow().len(), 1);
        }
        other => panic!("expected Built, got {other:?}"),
    }
}

#[test]
fn cancel_at_batch_boundary_does_not_publish() {
    let content = "fn a() {}\n";
    let h = content_hash(content);
    let entries = vec![chunk("a", "x.rs", &h, Some(1), Some(1), false)];
    let mut files = HashMap::new();
    files.insert("x.rs".to_string(), content.to_string());
    let emb = FakeEmbedder::new();
    match build_store(
        entries,
        &emb,
        reader(files),
        || true,
        BuildConfig::default(),
        &[],
    ) {
        BuildOutcome::Cancelled => {}
        other => panic!("expected Cancelled, got {other:?}"),
    }
}

#[test]
fn empty_corpus_is_no_corpus() {
    let emb = FakeEmbedder::new();
    match build_store(
        Vec::new(),
        &emb,
        reader(HashMap::new()),
        || false,
        BuildConfig::default(),
        &[],
    ) {
        BuildOutcome::NoCorpus => {}
        other => panic!("expected NoCorpus, got {other:?}"),
    }
}

#[test]
fn in_file_test_mod_chunk_is_classified_test_production_chunk_stays_production() {
    // SEED-CHUNK-2 §4 unit: a chunk inside an in-file `#[cfg(test)] mod` (in a
    // PRODUCTION file — file is_test=false) is stored is_test=true; a production
    // symbol in the SAME file stays is_test=false. Structural per-CHUNK evidence.
    let content = "\
pub fn prod() {}

#[cfg(test)]
mod tests {
    #[test]
    fn checks() { assert!(true); }
}
";
    let h = content_hash(content);
    let entries = vec![
        // file is_test = false for BOTH (a production .rs file with an in-file test mod).
        chunk("prod", "src/x.rs", &h, Some(1), Some(1), false),
        chunk("checks", "src/x.rs", &h, Some(6), Some(6), false),
    ];
    let mut files = HashMap::new();
    files.insert("src/x.rs".to_string(), content.to_string());
    let emb = FakeEmbedder::new();
    match build_store(
        entries,
        &emb,
        reader(files),
        || false,
        BuildConfig::default(),
        &[],
    ) {
        BuildOutcome::Built { entries, .. } => {
            let prod = entries.iter().find(|e| e.stable_key == "k:prod").unwrap();
            let checks = entries.iter().find(|e| e.stable_key == "k:checks").unwrap();
            assert!(!prod.is_test, "production symbol stays production");
            assert!(
                checks.is_test,
                "the in-file #[test] chunk is promoted to the test partition"
            );
        }
        other => panic!("expected Built, got {other:?}"),
    }
}

#[test]
fn file_fact_test_is_never_demoted_by_absent_structural_evidence() {
    // PROMOTE-ONLY: a chunk whose FILE is is_test=true stays test even with no
    // per-symbol structural marker (structural is OR'd, never AND'd).
    let content = "pub fn helper() {}\n";
    let h = content_hash(content);
    let entries = vec![chunk("helper", "tests/it.rs", &h, Some(1), Some(1), true)];
    let mut files = HashMap::new();
    files.insert("tests/it.rs".to_string(), content.to_string());
    let emb = FakeEmbedder::new();
    match build_store(
        entries,
        &emb,
        reader(files),
        || false,
        BuildConfig::default(),
        &[],
    ) {
        BuildOutcome::Built { entries, .. } => {
            assert!(entries[0].is_test, "file-test chunk stays test");
        }
        other => panic!("expected Built, got {other:?}"),
    }
}

#[test]
fn decl_and_impl_kinds_are_set_from_span_structure() {
    // SEED-CHUNK-2 §2.2: a bodyless callable signature is is_decl=true; a body-bearing
    // one is is_decl=false — from span structure, not the extension.
    let content = "\
trait T {
    fn decl_only(&self) -> u32;
}
fn impl_fn() -> u32 { 1 }
";
    let h = content_hash(content);
    let entries = vec![
        chunk("decl_only", "src/x.rs", &h, Some(2), Some(2), false),
        chunk("impl_fn", "src/x.rs", &h, Some(4), Some(4), false),
    ];
    let mut files = HashMap::new();
    files.insert("src/x.rs".to_string(), content.to_string());
    let emb = FakeEmbedder::new();
    match build_store(
        entries,
        &emb,
        reader(files),
        || false,
        BuildConfig::default(),
        &[],
    ) {
        BuildOutcome::Built { entries, .. } => {
            let d = entries
                .iter()
                .find(|e| e.stable_key == "k:decl_only")
                .unwrap();
            let i = entries
                .iter()
                .find(|e| e.stable_key == "k:impl_fn")
                .unwrap();
            assert!(d.is_decl, "bodyless signature is a declaration");
            assert!(!i.is_decl, "body-bearing fn is an implementation");
        }
        other => panic!("expected Built, got {other:?}"),
    }
}

#[test]
fn corpus_cap_omits_the_remainder() {
    let content = "a\nb\nc\n";
    let h = content_hash(content);
    let entries: Vec<_> = (0..5)
        .map(|i| chunk(&format!("n{i}"), "x.rs", &h, Some(1), Some(1), false))
        .collect();
    let mut files = HashMap::new();
    files.insert("x.rs".to_string(), content.to_string());
    let emb = FakeEmbedder::new();
    let cfg = BuildConfig {
        batch_size: 32,
        corpus_cap: 2,
    };
    match build_store(entries, &emb, reader(files), || false, cfg, &[]) {
        BuildOutcome::Built { report, .. } => {
            assert_eq!(report.admitted, 2);
            assert_eq!(report.corpus_omitted, 3);
        }
        other => panic!("expected Built, got {other:?}"),
    }
}

#[test]
fn unreadable_forward_decl_is_excluded_from_decl_tier_and_counted() {
    // CPP-DECLARATORS-1 (§2.3) / review-5 item 2: a CORRUPT `metadata_json.forward_decl` carrier
    // (SeedForwardDecl::Unreadable) must NEVER force `(decl)` on a TYPE chunk (the span heuristic
    // is callable-only, so nothing else sets it). It is EXCLUDED from the decl tier (is_decl=false)
    // and COUNTED in the report — corrupt metadata is never rendered as a positive `(decl)` fact
    // (STANDING HONESTY RULE 1). A genuine ForwardDecl on the SAME shape still forces `(decl)`.
    let content = "class A {};\nclass B {};\n";
    let h = content_hash(content);
    let good = SeedCorpusEntry {
        subtype: Some("CLASS".to_string()),
        forward_decl: SeedForwardDecl::ForwardDecl,
        ..chunk("good", "src/x.cpp", &h, Some(1), Some(1), false)
    };
    let corrupt = SeedCorpusEntry {
        subtype: Some("CLASS".to_string()),
        forward_decl: SeedForwardDecl::Unreadable,
        ..chunk("corrupt", "src/x.cpp", &h, Some(2), Some(2), false)
    };
    let mut files = HashMap::new();
    files.insert("src/x.cpp".to_string(), content.to_string());
    let emb = FakeEmbedder::new();
    match build_store(
        vec![good, corrupt],
        &emb,
        reader(files),
        || false,
        BuildConfig::default(),
        &[],
    ) {
        BuildOutcome::Built { entries, report } => {
            let good_row = entries
                .iter()
                .find(|e| e.node_uid == "good")
                .expect("good admitted");
            let corrupt_row = entries
                .iter()
                .find(|e| e.node_uid == "corrupt")
                .expect("corrupt admitted");
            assert!(
                good_row.is_decl,
                "a genuine ForwardDecl type chunk is labeled (decl)"
            );
            assert!(
                !corrupt_row.is_decl,
                "a corrupt (Unreadable) carrier is EXCLUDED from the decl tier, never (decl)"
            );
            assert_eq!(
                report.forward_decl_unreadable, 1,
                "the corrupt carrier is counted exactly once (honest degradation report)"
            );
        }
        other => panic!("expected Built, got {other:?}"),
    }
}

#[test]
fn field_tier_is_set_from_subtype_and_span() {
    // SEED-CHUNK-3 §2.1: build_store computes is_field per chunk from the stored subtype,
    // the span line count, and doc presence.
    //  - a PROPERTY (any span) is field-tier;
    //  - an undocumented one-line CONSTANT is field-tier (span rule);
    //  - a one-line CONSTANT WITH a doc comment is NOT (doc counter-signal);
    //  - a multi-line body-bearing FUNCTION is NOT.
    let content = "\
prop_line;
const K = 1;
const D = 2;
fn sig() -> u32;
fn work() -> u32 {
    1
}
";
    let h = content_hash(content);
    let prop = SeedCorpusEntry {
        subtype: Some("PROPERTY".to_string()),
        ..chunk("prop", "src/x.rs", &h, Some(1), Some(1), false)
    };
    let bare_const = SeedCorpusEntry {
        subtype: Some("CONSTANT".to_string()),
        ..chunk("bare_const", "src/x.rs", &h, Some(2), Some(2), false)
    };
    let doc_const = SeedCorpusEntry {
        subtype: Some("CONSTANT".to_string()),
        doc_comment: Some("The documented one.".to_string()),
        ..chunk("doc_const", "src/x.rs", &h, Some(3), Some(3), false)
    };
    // A ONE-LINE bodyless FUNCTION declaration (is_decl=true). It is ≤1 line with no doc, so
    // the span rule WOULD field-tier it — but the decl exclusion keeps it in the SEED-CHUNK-2
    // decl tier (FROZEN INVARIANT). This is the leveldb `DBImpl::Recover` decl case, pinned.
    let one_line_decl = SeedCorpusEntry {
        subtype: Some("FUNCTION".to_string()),
        ..chunk("one_line_decl", "src/x.rs", &h, Some(4), Some(4), false)
    };
    let work = chunk("work", "src/x.rs", &h, Some(5), Some(7), false); // FUNCTION, 3 lines
    let mut files = HashMap::new();
    files.insert("src/x.rs".to_string(), content.to_string());
    let emb = FakeEmbedder::new();
    match build_store(
        vec![prop, bare_const, doc_const, one_line_decl, work],
        &emb,
        reader(files),
        || false,
        BuildConfig::default(),
        &[],
    ) {
        BuildOutcome::Built { entries, .. } => {
            let get = |k: &str| entries.iter().find(|e| e.stable_key == k).unwrap();
            assert!(get("k:prop").is_field, "a PROPERTY is field-tier");
            assert!(
                get("k:bare_const").is_field,
                "an undocumented one-line constant is field-tier (span rule)"
            );
            assert!(
                !get("k:doc_const").is_field,
                "a documented one-line constant is NOT field-tier (doc counter-signal)"
            );
            assert!(
                get("k:one_line_decl").is_decl,
                "a bodyless one-line signature is a declaration"
            );
            assert!(
                !get("k:one_line_decl").is_field,
                "a one-line DECLARATION is decl-tiered, NOT field-tiered (decl exclusion)"
            );
            assert!(
                !get("k:work").is_field,
                "a multi-line body-bearing function is NOT field-tier"
            );
        }
        other => panic!("expected Built, got {other:?}"),
    }
}

#[test]
fn copy_forward_reembeds_when_doc_comment_changed_though_file_hash_unchanged() {
    // review-3 (the core regression): the SC3 scenario. The FILE bytes are byte-identical (same
    // content_hash), but a re-index with the SC3 extractor now populates the chunk's `doc_comment`
    // (a property's leading `//` run that was previously discarded). The embedded DOCUMENT therefore
    // changed even though the file hash did not. Copy-forward MUST NOT reuse the prior (docless)
    // vector — it must re-embed, and the NEW document (carrying the authored doc) must reach the
    // embedder. If reuse keyed on the file hash alone, the stale vector would be re-stamped as
    // current — presenting an embedding of a document that no longer matches the chunk (RULE 1).
    let content = "fn a() {}\n";
    let h = content_hash(content);
    // Current corpus chunk: SAME stable_key + content_hash, but now WITH a doc comment.
    let current = SeedCorpusEntry {
        doc_comment: Some("Tree ID".to_string()),
        ..chunk("a", "x.rs", &h, Some(1), Some(1), false)
    };
    let mut files = HashMap::new();
    files.insert("x.rs".to_string(), content.to_string());
    // Prior snapshot's vector for the SAME (stable_key, content_hash) but the OLD document
    // (doc=None) — its digest is the docless one, so it must NOT match the current chunk.
    let prior = vec![SeedVectorEntry {
        node_uid: "a_prev".to_string(),
        stable_key: "k:a".to_string(),
        file_uid: "fu:x.rs".to_string(),
        path: "x.rs".to_string(),
        line: Some(1),
        qualified_name: Some("a".to_string()),
        is_test: false,
        is_decl: false,
        is_field: false,
        content_hash: h.clone(),
        document_hash: Some(doc_hash(Some("a"), None, "fn a() {}")),
        vector: vec![0.6, 0.8],
    }];
    let emb = FakeEmbedder::new();
    match build_store(
        vec![current],
        &emb,
        reader(files),
        || false,
        BuildConfig::default(),
        &prior,
    ) {
        BuildOutcome::Built { entries, report } => {
            assert_eq!(
                report.reused, 0,
                "a changed document must NOT copy forward the stale (docless) vector"
            );
            let seen = emb.seen.borrow();
            assert_eq!(seen.len(), 1, "the chunk is re-embedded, not reused");
            assert!(
                seen[0].contains("Tree ID"),
                "the NEW document (with the authored doc comment) reaches the embedder: {:?}",
                seen[0]
            );
            // The stored entry carries the CURRENT document's digest, not the prior one.
            let a = entries.iter().find(|e| e.stable_key == "k:a").unwrap();
            assert_eq!(
                a.document_hash,
                Some(doc_hash(Some("a"), Some("Tree ID"), "fn a() {}")),
                "the re-embedded row stores the current document digest"
            );
        }
        other => panic!("expected Built, got {other:?}"),
    }
}

#[test]
fn copy_forward_reembeds_a_legacy_null_document_hash_parent() {
    // review-3 self-heal: a PRE-migration-037 parent has a NULL document_hash (its embedding-input
    // identity is unknown). Even with a matching (stable_key, content_hash), it is EXCLUDED from the
    // reuse map and re-embedded — never reused blind. This is the one-time transition after the
    // upgrade; the re-seed writes the digest, and every subsequent refresh reuses correctly.
    let content = "fn a() {}\n";
    let h = content_hash(content);
    let entries = vec![chunk("a", "x.rs", &h, Some(1), Some(1), false)];
    let mut files = HashMap::new();
    files.insert("x.rs".to_string(), content.to_string());
    let prior = vec![SeedVectorEntry {
        node_uid: "a_prev".to_string(),
        stable_key: "k:a".to_string(),
        file_uid: "fu:x.rs".to_string(),
        path: "x.rs".to_string(),
        line: Some(1),
        qualified_name: Some("a".to_string()),
        is_test: false,
        is_decl: false,
        is_field: false,
        content_hash: h.clone(),
        document_hash: None, // legacy parent: unknown document identity
        vector: vec![0.6, 0.8],
    }];
    let emb = FakeEmbedder::new();
    match build_store(
        entries,
        &emb,
        reader(files),
        || false,
        BuildConfig::default(),
        &prior,
    ) {
        BuildOutcome::Built { entries, report } => {
            assert_eq!(
                report.reused, 0,
                "a NULL-digest legacy parent is never reused blind — it re-embeds"
            );
            assert_eq!(emb.seen.borrow().len(), 1, "the chunk is re-embedded");
            let a = entries.iter().find(|e| e.stable_key == "k:a").unwrap();
            assert!(
                a.document_hash.is_some(),
                "the re-seeded row now carries a document digest (clears the legacy NULL)"
            );
        }
        other => panic!("expected Built, got {other:?}"),
    }
}
