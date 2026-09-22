//! Unit tests for the pure chunk-embed pipeline (`pass::build_store`). Driven with
//! a fake `Embedder` + in-memory file reader — no model, no daemon, no DB.

use super::*;
use crate::document::build_chunk_document;
use crate::hash::content_hash;
use crate::ports::{EmbedError, Embedder, SeedCorpusEntry, SeedForwardDecl, SeedVectorEntry};
use std::collections::HashMap;

/// The copy-forward reuse digest for a chunk (review-3): the hash of the ASSEMBLED document
/// `build_chunk_document(qname, None, doc, span)` — the exact bytes the embedder saw for a chunk
/// WITHOUT an enclosing-type sentence (every caller's fixture: FUNCTION chunks, or METHOD chunks
/// whose name-parent contributes none). A test helper so a prior entry can carry the EXACT digest
/// `build_store` computes for the matching current chunk, and so the SEED-DOCUMENT-1 fixtures can
/// assert a document is byte-identical to the no-enclosing composition.
fn doc_hash(qname: Option<&str>, doc: Option<&str>, span: &str) -> String {
    content_hash(&build_chunk_document(qname, None, doc, span))
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

// ── SEED-DOCUMENT-1 (RG-REQ-010-L10): the enclosing TYPE's first sentence in a METHOD's document ──

/// One corpus entry for the enclosing-type fixtures: a ONE-line span at `line` of `path`, pinned
/// to the hash of that file's `content`.
fn sym(
    qn: &str,
    subtype: &str,
    doc: Option<&str>,
    path: &str,
    content: &str,
    line: i64,
) -> SeedCorpusEntry {
    SeedCorpusEntry {
        node_uid: format!("{path}#{qn}#{line}"),
        stable_key: format!("k:{path}#{qn}"),
        file_uid: format!("fu:{path}"),
        path: path.to_string(),
        qualified_name: Some(qn.to_string()),
        subtype: Some(subtype.to_string()),
        doc_comment: doc.map(str::to_string),
        line_start: Some(line),
        line_end: Some(line),
        is_test: false,
        content_hash: content_hash(content),
        forward_decl: SeedForwardDecl::Definition,
    }
}

/// What one fixture chunk produced: its embedded document (the exact text the embedder saw), the
/// stored row, and its one-line span.
struct Embedded {
    document: String,
    row: SeedVectorEntry,
}

/// Run `build_store` with no prior (every admitted chunk is embedded, so the embedder's `seen`
/// list is parallel to the admitted rows) and index the result by `node_uid`.
fn embed_fixture(
    entries: Vec<SeedCorpusEntry>,
    files: &[(&str, &str)],
    cfg: BuildConfig,
) -> (HashMap<String, Embedded>, BuildReport) {
    let files: HashMap<String, String> = files
        .iter()
        .map(|(p, c)| (p.to_string(), c.to_string()))
        .collect();
    let emb = FakeEmbedder::new();
    match build_store(entries, &emb, reader(files), || false, cfg, &[]) {
        BuildOutcome::Built { entries, report } => {
            let seen = emb.seen.borrow().clone();
            assert_eq!(
                seen.len(),
                entries.len(),
                "no prior → every admitted chunk embedded"
            );
            let by_uid = entries
                .into_iter()
                .zip(seen)
                .map(|(row, document)| (row.node_uid.clone(), Embedded { document, row }))
                .collect();
            (by_uid, report)
        }
        other => panic!("expected Built, got {other:?}"),
    }
}

/// Assert the chunk embedded EXACTLY today's composition (no enclosing sentence): document bytes
/// and reuse digest both equal `build_chunk_document(qn, None, own_doc, span)`.
fn assert_no_sentence(e: &Embedded, qn: &str, own_doc: Option<&str>, span: &str) {
    let today = build_chunk_document(Some(qn), None, own_doc, span);
    assert_eq!(
        e.document, today,
        "{qn}: document must be byte-identical to today's"
    );
    assert_eq!(
        e.row.document_hash.as_deref(),
        Some(doc_hash(Some(qn), own_doc, span).as_str()),
        "{qn}: reuse key must be today's"
    );
}

/// Assert the chunk embedded the composition WITH `sentence` and that its reuse digest moved.
fn assert_sentence(e: &Embedded, qn: &str, sentence: &str, own_doc: Option<&str>, span: &str) {
    let expected = build_chunk_document(Some(qn), Some(sentence), own_doc, span);
    assert_eq!(
        e.document, expected,
        "{qn}: document must carry `{sentence}`"
    );
    assert_eq!(
        e.row.document_hash.as_deref(),
        Some(content_hash(&expected).as_str())
    );
    assert_ne!(
        e.row.document_hash.as_deref(),
        Some(doc_hash(Some(qn), own_doc, span).as_str()),
        "{qn}: the sentence is part of the reuse identity"
    );
}

#[test]
fn enclosing_type_sentence_enters_the_method_document_and_its_reuse_key() {
    let a = "class C {\n  m() { return 1; }\n}\n";
    let entries = vec![
        sym(
            "C",
            "CLASS",
            Some("/**\n * C keeps sessions on disk.\n * More.\n */"),
            "a.ts",
            a,
            1,
        ),
        sym("C.m", "METHOD", Some("Returns one."), "a.ts", a, 2),
    ];
    let (out, report) = embed_fixture(entries, &[("a.ts", a)], BuildConfig::default());
    assert_eq!(report.admitted, 2);
    let m = &out["a.ts#C.m#2"];
    assert!(m.document.contains("C keeps sessions on disk."));
    assert!(!m.document.contains("More."), "only the FIRST sentence");
    assert_sentence(
        m,
        "C.m",
        "C keeps sessions on disk.",
        Some("Returns one."),
        "  m() { return 1; }",
    );
    // The class's own chunk embeds as today (it is not a METHOD).
    assert_no_sentence(
        &out["a.ts#C#1"],
        "C",
        Some("/**\n * C keeps sessions on disk.\n * More.\n */"),
        "class C {",
    );
}

#[test]
fn same_file_type_parent_wins_over_a_cross_file_one() {
    let a = "class D {\n  m() {}\n}\n";
    let b = "class D {}\n";
    let entries = vec![
        sym("D", "CLASS", Some("D lives in a."), "a.ts", a, 1),
        sym("D.m", "METHOD", None, "a.ts", a, 2),
        sym("D", "CLASS", Some("D lives in b."), "b.ts", b, 1),
    ];
    let (out, _) = embed_fixture(entries, &[("a.ts", a), ("b.ts", b)], BuildConfig::default());
    let m = &out["a.ts#D.m#2"];
    assert_sentence(m, "D.m", "D lives in a.", None, "  m() {}");
    assert!(!m.document.contains("D lives in b."));
}

#[test]
fn unique_cross_file_type_parent_contributes_its_sentence() {
    // The C++ header/impl split: the class is declared (and documented) in the header, the
    // method defined in the .cc (leveldb `db/db_impl.h:29` / `db/db_impl.cc:292` shape).
    let cc = "void E::m() {\n}\n";
    let h = "class E {\n};\n";
    let entries = vec![
        sym("E.m", "METHOD", None, "e.cc", cc, 1),
        sym(
            "E",
            "CLASS",
            Some("E replays the log. Details."),
            "e.h",
            h,
            1,
        ),
    ];
    let (out, _) = embed_fixture(entries, &[("e.cc", cc), ("e.h", h)], BuildConfig::default());
    assert_sentence(
        &out["e.cc#E.m#1"],
        "E.m",
        "E replays the log.",
        None,
        "void E::m() {",
    );
}

#[test]
fn ambiguous_cross_file_type_parents_contribute_no_sentence() {
    let cc = "void F::m() {\n}\n";
    let h1 = "class F {};\n";
    let h2 = "class F {};\n";
    let entries = vec![
        sym("F.m", "METHOD", None, "f.cc", cc, 1),
        sym("F", "CLASS", Some("F one."), "f1.h", h1, 1),
        sym("F", "CLASS", Some("F two."), "f2.h", h2, 1),
    ];
    let (out, _) = embed_fixture(
        entries,
        &[("f.cc", cc), ("f1.h", h1), ("f2.h", h2)],
        BuildConfig::default(),
    );
    assert_no_sentence(&out["f.cc#F.m#1"], "F.m", None, "void F::m() {");
}

#[test]
fn same_file_name_parent_blocks_the_cross_file_fallback() {
    let h = "class H {\n  m() {}\n}\nconst v = {\n  m() {},\n};\n";
    let other = "class H {}\nclass v {}\n";
    let entries = vec![
        sym("H", "CLASS", None, "h.ts", h, 1), // UNDOCUMENTED same-file class
        sym("H.m", "METHOD", None, "h.ts", h, 2),
        sym(
            "v",
            "VARIABLE",
            Some("A documented variable."),
            "h.ts",
            h,
            4,
        ), // non-type
        sym("v.m", "METHOD", None, "h.ts", h, 5),
        sym(
            "H",
            "CLASS",
            Some("Another H elsewhere."),
            "other.ts",
            other,
            1,
        ),
        sym(
            "v",
            "CLASS",
            Some("A class named v elsewhere."),
            "other.ts",
            other,
            2,
        ),
    ];
    let (out, _) = embed_fixture(
        entries,
        &[("h.ts", h), ("other.ts", other)],
        BuildConfig::default(),
    );
    assert_no_sentence(&out["h.ts#H.m#2"], "H.m", None, "  m() {}");
    assert_no_sentence(&out["h.ts#v.m#5"], "v.m", None, "  m() {},");
}

#[test]
fn forward_declaration_is_never_an_enclosing_type_candidate() {
    let g_h = "class G {\n};\n";
    let g_fwd = "class G;\n";
    let g_cc = "void G::m() {\n}\n";
    let k_fwd = "class K;\n";
    let k_cc = "void K::m() {\n}\n";
    let fwd = |mut e: SeedCorpusEntry| {
        e.forward_decl = SeedForwardDecl::ForwardDecl;
        e
    };
    let entries = vec![
        sym("G::m", "METHOD", None, "g.cc", g_cc, 1),
        sym("G", "CLASS", Some("G is the definition."), "g.h", g_h, 1),
        fwd(sym(
            "G",
            "CLASS",
            Some("G forward-declared."),
            "g_fwd.h",
            g_fwd,
            1,
        )),
        sym("K::m", "METHOD", None, "k.cc", k_cc, 1),
        fwd(sym(
            "K",
            "CLASS",
            Some("K only forward-declared."),
            "k_fwd.h",
            k_fwd,
            1,
        )),
    ];
    let (out, _) = embed_fixture(
        entries,
        &[
            ("g.cc", g_cc),
            ("g.h", g_h),
            ("g_fwd.h", g_fwd),
            ("k.cc", k_cc),
            ("k_fwd.h", k_fwd),
        ],
        BuildConfig::default(),
    );
    let g = &out["g.cc#G::m#1"];
    assert_sentence(g, "G::m", "G is the definition.", None, "void G::m() {");
    assert!(!g.document.contains("forward-declared"));
    assert_no_sentence(&out["k.cc#K::m#1"], "K::m", None, "void K::m() {");
}

#[test]
fn unique_type_parent_beyond_the_corpus_cap_still_contributes_its_sentence() {
    let a = "void Z::m() {\n}\n";
    let z = "class Z {\n};\n";
    let entries = vec![
        sym("Z::m", "METHOD", None, "a.cc", a, 1),
        sym("Z", "CLASS", Some("Z sits beyond the cap."), "z.h", z, 1),
    ];
    let cfg = BuildConfig {
        batch_size: 32,
        corpus_cap: 1,
    };
    let (out, report) = embed_fixture(entries, &[("a.cc", a), ("z.h", z)], cfg);
    assert_eq!(report.admitted, 1);
    assert_eq!(report.corpus_omitted, 1);
    assert_eq!(out.len(), 1, "the class's own chunk is still not emitted");
    assert_sentence(
        &out["a.cc#Z::m#1"],
        "Z::m",
        "Z sits beyond the cap.",
        None,
        "void Z::m() {",
    );
}

#[test]
fn second_type_parent_beyond_the_corpus_cap_makes_the_fallback_abstain() {
    let a = "void Y::m() {\n}\n";
    let b = "class Y {\n};\n";
    let z = "class Y {\n};\n";
    let entries = vec![
        sym("Y::m", "METHOD", None, "a.cc", a, 1),
        sym("Y", "CLASS", Some("Y inside the cap."), "b.h", b, 1),
        sym("Y", "CLASS", Some("Y beyond the cap."), "z.h", z, 1),
    ];
    let cfg = BuildConfig {
        batch_size: 32,
        corpus_cap: 2,
    };
    let (out, report) = embed_fixture(entries, &[("a.cc", a), ("b.h", b), ("z.h", z)], cfg);
    assert_eq!(report.admitted, 2);
    assert_eq!(report.corpus_omitted, 1);
    assert_no_sentence(&out["a.cc#Y::m#1"], "Y::m", None, "void Y::m() {");
}

#[test]
fn non_method_children_embed_without_the_enclosing_sentence() {
    let a = "class C {\n  p: string;\n  constructor() {\n    this.p = '';\n  }\n}\n";
    let entries = vec![
        sym(
            "C",
            "CLASS",
            Some("C keeps sessions on disk."),
            "a.ts",
            a,
            1,
        ),
        sym("C.p", "PROPERTY", None, "a.ts", a, 2),
        SeedCorpusEntry {
            line_end: Some(5),
            ..sym("C.constructor", "CONSTRUCTOR", None, "a.ts", a, 3)
        },
    ];
    let (out, _) = embed_fixture(entries, &[("a.ts", a)], BuildConfig::default());
    let p = &out["a.ts#C.p#2"];
    assert_no_sentence(p, "C.p", None, "  p: string;");
    assert!(
        p.row.is_field,
        "the one-line undocumented property stays field-tiered"
    );
    assert_no_sentence(
        &out["a.ts#C.constructor#3"],
        "C.constructor",
        None,
        "  constructor() {\n    this.p = '';\n  }",
    );
}

#[test]
fn documented_non_type_name_parent_gives_no_enclosing_sentence() {
    let a = "function f() {}\nf.m = function () {};\nconst v = {\n  m() {},\n};\ntype T = { m(): void };\n";
    let entries = vec![
        sym(
            "f",
            "FUNCTION",
            Some("A documented function."),
            "a.ts",
            a,
            1,
        ),
        sym("f.m", "METHOD", None, "a.ts", a, 2),
        sym(
            "v",
            "VARIABLE",
            Some("A documented variable."),
            "a.ts",
            a,
            3,
        ),
        sym("v.m", "METHOD", None, "a.ts", a, 4),
        // TYPE_ALIAS names a type but declares no member scope — excluded from the enclosing set.
        sym("T", "TYPE_ALIAS", Some("A documented alias."), "a.ts", a, 6),
        sym("T.m", "METHOD", None, "a.ts", a, 6),
    ];
    let (out, _) = embed_fixture(entries, &[("a.ts", a)], BuildConfig::default());
    assert_no_sentence(&out["a.ts#f.m#2"], "f.m", None, "f.m = function () {};");
    assert_no_sentence(&out["a.ts#v.m#4"], "v.m", None, "  m() {},");
    assert_no_sentence(&out["a.ts#T.m#6"], "T.m", None, "type T = { m(): void };");
}

#[test]
fn each_canonical_type_subtype_encloses_a_method() {
    for kind in ["CLASS", "INTERFACE", "STRUCT", "ENUM"] {
        let path = format!("{}.x", kind.to_lowercase());
        let content = "T {\n  m() {}\n}\n";
        let sentence = format!("The {kind} parent.");
        let entries = vec![
            sym(
                "T",
                kind,
                Some(&format!("{sentence} Second sentence.")),
                &path,
                content,
                1,
            ),
            sym("T.m", "METHOD", None, &path, content, 2),
        ];
        let (out, _) = embed_fixture(entries, &[(&path, content)], BuildConfig::default());
        let m = &out[&format!("{path}#T.m#2")];
        assert!(m.document.contains(&sentence), "{kind}: {}", m.document);
        assert_sentence(m, "T.m", &sentence, None, "  m() {}");
    }
}

#[test]
fn forward_declared_method_receives_no_enclosing_sentence() {
    // SD-R-001: the C++ extractor flags an in-class method PROTOTYPE `forward_decl: true`
    // (cpp-extractor `build_node_metadata`). The LOOKUP RULE ignores genuine forward declarations
    // THROUGHOUT — they neither supply nor RECEIVE a sentence — so the prototype `C::m` in the
    // header embeds exactly as before while the same-named definition in the .cc receives it.
    let h = "class C {\n  void m();\n};\n";
    let cc = "void C::m() {\n}\n";
    let entries = vec![
        sym("C::m", "METHOD", None, "c.cc", cc, 1),
        sym("C", "CLASS", Some("C replays the log. More."), "c.h", h, 1),
        SeedCorpusEntry {
            forward_decl: SeedForwardDecl::ForwardDecl,
            ..sym("C::m", "METHOD", None, "c.h", h, 2)
        },
    ];
    let (out, _) = embed_fixture(entries, &[("c.cc", cc), ("c.h", h)], BuildConfig::default());
    assert_no_sentence(&out["c.h#C::m#2"], "C::m", None, "  void m();");
    assert_sentence(
        &out["c.cc#C::m#1"],
        "C::m",
        "C replays the log.",
        None,
        "void C::m() {",
    );
}
