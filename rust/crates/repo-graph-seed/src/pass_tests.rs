//! Unit tests for the pure chunk-embed pipeline (`pass::build_store`). Driven with
//! a fake `Embedder` + in-memory file reader — no model, no daemon, no DB.

use super::*;
use crate::hash::content_hash;
use crate::ports::{EmbedError, Embedder, SeedCorpusEntry, SeedVectorEntry};
use std::collections::HashMap;

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
    SeedCorpusEntry {
        node_uid: node.to_string(),
        stable_key: format!("k:{node}"),
        file_uid: format!("fu:{path}"),
        path: path.to_string(),
        qualified_name: Some(node.to_string()),
        doc_comment: None,
        line_start,
        line_end,
        is_test,
        content_hash: hash.to_string(),
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
    // Prior snapshot embedded chunk "a" (same stable_key + content_hash).
    let prior = vec![SeedVectorEntry {
        node_uid: "a_prev".to_string(),
        stable_key: "k:a".to_string(),
        file_uid: "fu:x.rs".to_string(),
        path: "x.rs".to_string(),
        line: Some(1),
        qualified_name: Some("a".to_string()),
        is_test: false,
        content_hash: h.clone(),
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
