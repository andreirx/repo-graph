//! Unit tests for `pass.rs` (the pure embed pipeline). Split out via `#[path]`
//! (the repo idiom, e.g. `complexity_tests.rs`) so `pass.rs` stays under the
//! 500-line guardrail (review-2 #7). Child module of `pass`; `super::*` = the
//! pipeline under test.

use super::*;
use crate::store::{decode, SeedStoreKey};
use std::collections::HashMap;

/// Deterministic fake: each doc → a fixed-dim vector derived from its length.
struct FakeEmbedder {
    dim: usize,
}
impl Embedder for FakeEmbedder {
    fn model_id(&self) -> &str {
        "fake-model"
    }
    fn dim(&self) -> usize {
        self.dim
    }
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        Ok(texts
            .iter()
            .map(|t| {
                let base = (t.len() % 7) as f32 + 1.0;
                (0..self.dim).map(|i| base + i as f32).collect()
            })
            .collect())
    }
}

fn entry(uid: &str, path: &str, content: &str) -> (SeedCorpusEntry, String) {
    (
        SeedCorpusEntry {
            file_uid: uid.to_string(),
            path: path.to_string(),
            content_hash: content_hash(content),
        },
        content.to_string(),
    )
}

fn key() -> SeedStoreKey {
    SeedStoreKey {
        model_id: "fake-model".to_string(),
        dim: 4,
        repo_graph_version: "0.8.0".to_string(),
    }
}

#[test]
fn builds_a_valid_store_over_admitted_files() {
    let (e1, c1) = entry("f1", "a.ts", "content a");
    let (e2, c2) = entry("f2", "b.ts", "content b");
    let files: HashMap<String, String> =
        [("a.ts".to_string(), c1), ("b.ts".to_string(), c2)].into();
    let entries = vec![e1, e2];
    let embedder = FakeEmbedder { dim: 4 };

    let outcome = build_store(
        entries,
        &embedder,
        |p| {
            files
                .get(p)
                .cloned()
                .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))
        },
        || false,
        &key(),
        42,
        BuildConfig::default(),
        None,
    );

    match outcome {
        BuildOutcome::Built { bytes, report } => {
            assert_eq!(report.admitted, 2);
            assert_eq!(report.drifted, 0);
            let body = decode(&bytes, &key()).unwrap();
            assert_eq!(body.entries.len(), 2);
            // stored vectors are L2-normalized (‖v‖ ≈ 1)
            for e in &body.entries {
                let norm: f32 = e.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
                assert!((norm - 1.0).abs() < 1e-4, "norm was {norm}");
            }
        }
        other => panic!("expected Built, got {other:?}"),
    }
}

#[test]
fn drifted_working_tree_is_omitted_not_stored_under_old_pin() {
    // The corpus pin says one hash; the working tree has different content.
    let e = SeedCorpusEntry {
        file_uid: "f".to_string(),
        path: "a.ts".to_string(),
        content_hash: content_hash("the snapshot content"),
    };
    let entries = vec![e];
    let embedder = FakeEmbedder { dim: 4 };
    let outcome = build_store(
        entries,
        &embedder,
        |_p| Ok("DIFFERENT working-tree content".to_string()),
        || false,
        &key(),
        0,
        BuildConfig::default(),
        None,
    );
    match outcome {
        BuildOutcome::Built { bytes, report } => {
            assert_eq!(report.admitted, 0);
            assert_eq!(report.drifted, 1);
            let body = decode(&bytes, &key()).unwrap();
            assert!(body.entries.is_empty());
        }
        other => panic!("expected Built(empty), got {other:?}"),
    }
}

#[test]
fn cancel_at_batch_boundary_does_not_build() {
    let (e1, c1) = entry("f1", "a.ts", "x");
    let files: HashMap<String, String> = [("a.ts".to_string(), c1)].into();
    let entries = vec![e1];
    let embedder = FakeEmbedder { dim: 4 };
    let outcome = build_store(
        entries,
        &embedder,
        |p| Ok(files.get(p).cloned().unwrap()),
        || true, // cancelled before the first batch
        &key(),
        0,
        BuildConfig::default(),
        None,
    );
    assert!(matches!(outcome, BuildOutcome::Cancelled));
}

#[test]
fn empty_corpus_is_no_corpus() {
    let entries = vec![];
    let embedder = FakeEmbedder { dim: 4 };
    let outcome = build_store(
        entries,
        &embedder,
        |_p| Ok(String::new()),
        || false,
        &key(),
        0,
        BuildConfig::default(),
        None,
    );
    assert!(matches!(outcome, BuildOutcome::NoCorpus));
}

#[test]
fn embed_failure_declines() {
    struct DeadEmbedder;
    impl Embedder for DeadEmbedder {
        fn model_id(&self) -> &str {
            "x"
        }
        fn dim(&self) -> usize {
            4
        }
        fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
            Err(EmbedError::Unreachable {
                endpoint: "http://127.0.0.1:1234".to_string(),
                detail: "connection refused".to_string(),
            })
        }
    }
    let (e1, c1) = entry("f1", "a.ts", "x");
    let files: HashMap<String, String> = [("a.ts".to_string(), c1)].into();
    let entries = vec![e1];
    let outcome = build_store(
        entries,
        &DeadEmbedder,
        |p| Ok(files.get(p).cloned().unwrap()),
        || false,
        &key(),
        0,
        BuildConfig::default(),
        None,
    );
    assert!(matches!(
        outcome,
        BuildOutcome::Embed(EmbedError::Unreachable { .. })
    ));
}

#[test]
fn corpus_cap_reports_omission() {
    let entries: Vec<SeedCorpusEntry> = (0..5)
        .map(|i| SeedCorpusEntry {
            file_uid: format!("f{i}"),
            path: format!("{i}.ts"),
            content_hash: content_hash("c"),
        })
        .collect();
    let embedder = FakeEmbedder { dim: 4 };
    let outcome = build_store(
        entries,
        &embedder,
        |_p| Ok("c".to_string()),
        || false,
        &key(),
        0,
        BuildConfig {
            batch_size: 2,
            corpus_cap: 3,
        },
        None,
    );
    match outcome {
        BuildOutcome::Built { report, .. } => {
            assert_eq!(report.admitted, 3);
            assert_eq!(report.corpus_omitted, 2);
        }
        other => panic!("expected Built, got {other:?}"),
    }
}

/// A `FakeEmbedder` that COUNTS how many documents it was ever asked to embed.
/// Proves the §5 incremental-refresh contract: a second pass over an unchanged
/// corpus makes ZERO embed calls (every vector copied forward by content_hash).
struct CountingEmbedder {
    dim: usize,
    embedded: std::cell::Cell<usize>,
}
impl Embedder for CountingEmbedder {
    fn model_id(&self) -> &str {
        "fake-model"
    }
    fn dim(&self) -> usize {
        self.dim
    }
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        self.embedded.set(self.embedded.get() + texts.len());
        Ok(texts
            .iter()
            .map(|t| {
                let base = (t.len() % 7) as f32 + 1.0;
                (0..self.dim).map(|i| base + i as f32).collect()
            })
            .collect())
    }
}

#[test]
fn second_pass_over_unchanged_corpus_makes_zero_embed_calls() {
    let (e1, c1) = entry("f1", "a.ts", "content a");
    let (e2, c2) = entry("f2", "b.ts", "content b");
    let files: HashMap<String, String> =
        [("a.ts".to_string(), c1), ("b.ts".to_string(), c2)].into();
    let read = |p: &str| {
        files
            .get(p)
            .cloned()
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))
    };
    let embedder = CountingEmbedder {
        dim: 4,
        embedded: std::cell::Cell::new(0),
    };

    // First pass: no prior → both files embed.
    let first = build_store(
        vec![e1.clone(), e2.clone()],
        &embedder,
        read,
        || false,
        &key(),
        1,
        BuildConfig::default(),
        None,
    );
    let prior_body = match first {
        BuildOutcome::Built { bytes, report } => {
            assert_eq!(report.admitted, 2);
            assert_eq!(report.reused, 0);
            decode(&bytes, &key()).unwrap()
        }
        other => panic!("expected Built, got {other:?}"),
    };
    assert_eq!(embedder.embedded.get(), 2, "first pass embeds both files");

    // Second pass: prior store present, corpus unchanged → ZERO embed calls,
    // every vector copied forward, and the stored vectors are byte-identical.
    let second = build_store(
        vec![e1, e2],
        &embedder,
        read,
        || false,
        &key(),
        2,
        BuildConfig::default(),
        Some(&prior_body),
    );
    match second {
        BuildOutcome::Built { bytes, report } => {
            assert_eq!(report.admitted, 2);
            assert_eq!(report.reused, 2, "both vectors copied forward");
            let body = decode(&bytes, &key()).unwrap();
            assert_eq!(
                body.entries, prior_body.entries,
                "reused vectors are byte-identical to the prior store"
            );
        }
        other => panic!("expected Built, got {other:?}"),
    }
    assert_eq!(
        embedder.embedded.get(),
        2,
        "second pass over an unchanged corpus makes NO additional embed calls"
    );
}

#[test]
fn changed_file_re_embeds_only_that_file() {
    let (e1, c1) = entry("f1", "a.ts", "content a");
    let (e2, c2) = entry("f2", "b.ts", "content b");
    let files1: HashMap<String, String> =
        [("a.ts".to_string(), c1.clone()), ("b.ts".to_string(), c2)].into();
    let embedder = CountingEmbedder {
        dim: 4,
        embedded: std::cell::Cell::new(0),
    };
    let first = build_store(
        vec![e1.clone(), e2.clone()],
        &embedder,
        |p: &str| {
            files1
                .get(p)
                .cloned()
                .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))
        },
        || false,
        &key(),
        1,
        BuildConfig::default(),
        None,
    );
    let prior_body = match first {
        BuildOutcome::Built { bytes, .. } => decode(&bytes, &key()).unwrap(),
        other => panic!("expected Built, got {other:?}"),
    };
    assert_eq!(embedder.embedded.get(), 2);

    // b.ts changes; its corpus pin updates to the new content hash.
    let e2b = SeedCorpusEntry {
        file_uid: "f2".to_string(),
        path: "b.ts".to_string(),
        content_hash: content_hash("content b CHANGED"),
    };
    let files2: HashMap<String, String> = [
        ("a.ts".to_string(), c1),
        ("b.ts".to_string(), "content b CHANGED".to_string()),
    ]
    .into();
    let second = build_store(
        vec![e1, e2b],
        &embedder,
        |p: &str| {
            files2
                .get(p)
                .cloned()
                .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))
        },
        || false,
        &key(),
        2,
        BuildConfig::default(),
        Some(&prior_body),
    );
    match second {
        BuildOutcome::Built { report, .. } => {
            assert_eq!(report.admitted, 2);
            assert_eq!(report.reused, 1, "only the unchanged file copies forward");
        }
        other => panic!("expected Built, got {other:?}"),
    }
    assert_eq!(
        embedder.embedded.get(),
        3,
        "second pass embeds ONLY the one changed file (2 + 1)"
    );
}
