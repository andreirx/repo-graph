//! CRUD operations for the semantic_facts table.
//!
//! Semantic facts are repo-scoped current-state extractions from
//! documentation and configuration files. Each `rmap docs` run
//! replaces all facts for the repo.
//!
//! Design constraints:
//! - No snapshot_uid: facts are read live from disk, not snapshot-tight
//! - Replace semantics: delete-then-insert on each extraction run
//! - Minimal provenance: excerpt + hash, not full doc text
//! - Deterministic identity: fact_uid is derived from semantic content,
//!   not random, enabling idempotent extraction runs

use rusqlite::{params, Row};
use uuid::Uuid;

use crate::connection::StorageConnection;
use crate::error::StorageError;

/// Namespace UUID for semantic fact identity derivation.
///
/// UUID v5 requires a namespace UUID. This is a fixed UUID generated
/// once for the semantic_facts domain. Facts with identical semantic
/// identity (same repo, kind, refs, location, doc_kind) will produce
/// the same fact_uid across extraction runs.
///
/// Generated via: `uuid::Uuid::new_v4()` → frozen as constant.
const SEMANTIC_FACT_NAMESPACE: Uuid =
    Uuid::from_bytes([0x9a, 0x3b, 0x7c, 0x4d, 0x5e, 0x6f, 0x01, 0x23,
                      0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x12, 0x34]);

/// Derive a deterministic fact_uid from the semantic identity fields.
///
/// Identity fields (included):
/// - repo_uid, fact_kind, subject_ref_kind, subject_ref
/// - object_ref_kind, object_ref, source_file
/// - source_line_start, source_line_end, doc_kind
///
/// Non-identity fields (excluded):
/// - confidence, extraction_method, content_hash
/// - source_text_excerpt, extracted_at, generated
///
/// This split means the same semantic fact at the same location
/// produces the same ID even if extraction quality improves.
fn derive_fact_uid(fact: &NewSemanticFact) -> String {
    // Canonical string: pipe-delimited, None → empty string
    let canonical = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        fact.repo_uid,
        fact.fact_kind,
        fact.subject_ref_kind,
        fact.subject_ref,
        fact.object_ref_kind.as_deref().unwrap_or(""),
        fact.object_ref.as_deref().unwrap_or(""),
        fact.source_file,
        fact.source_line_start.map(|n| n.to_string()).unwrap_or_default(),
        fact.source_line_end.map(|n| n.to_string()).unwrap_or_default(),
        fact.doc_kind,
    );

    Uuid::new_v5(&SEMANTIC_FACT_NAMESPACE, canonical.as_bytes()).to_string()
}

/// Map a row to a SemanticFact.
fn row_to_semantic_fact(row: &Row<'_>) -> rusqlite::Result<SemanticFact> {
    Ok(SemanticFact {
        fact_uid: row.get(0)?,
        repo_uid: row.get(1)?,
        fact_kind: row.get(2)?,
        subject_ref: row.get(3)?,
        subject_ref_kind: row.get(4)?,
        object_ref: row.get(5)?,
        object_ref_kind: row.get(6)?,
        source_file: row.get(7)?,
        source_line_start: row.get(8)?,
        source_line_end: row.get(9)?,
        source_text_excerpt: row.get(10)?,
        content_hash: row.get(11)?,
        extraction_method: row.get(12)?,
        confidence: row.get(13)?,
        generated: row.get::<_, i64>(14)? == 1,
        doc_kind: row.get(15)?,
        extracted_at: row.get(16)?,
    })
}

/// A semantic fact extracted from documentation or configuration.
#[derive(Debug, Clone)]
pub struct SemanticFact {
    pub fact_uid: String,
    pub repo_uid: String,
    pub fact_kind: String,
    pub subject_ref: String,
    pub subject_ref_kind: String,
    pub object_ref: Option<String>,
    pub object_ref_kind: Option<String>,
    pub source_file: String,
    pub source_line_start: Option<i64>,
    pub source_line_end: Option<i64>,
    pub source_text_excerpt: Option<String>,
    pub content_hash: String,
    pub extraction_method: String,
    pub confidence: f64,
    pub generated: bool,
    pub doc_kind: String,
    pub extracted_at: String,
}

/// Input for creating a new semantic fact (without fact_uid).
#[derive(Debug, Clone)]
pub struct NewSemanticFact {
    pub repo_uid: String,
    pub fact_kind: String,
    pub subject_ref: String,
    pub subject_ref_kind: String,
    pub object_ref: Option<String>,
    pub object_ref_kind: Option<String>,
    pub source_file: String,
    pub source_line_start: Option<i64>,
    pub source_line_end: Option<i64>,
    pub source_text_excerpt: Option<String>,
    pub content_hash: String,
    pub extraction_method: String,
    pub confidence: f64,
    pub generated: bool,
    pub doc_kind: String,
}

impl StorageConnection {
    /// Delete all semantic facts for a repository.
    ///
    /// Returns the number of rows deleted.
    pub fn delete_semantic_facts_for_repo(&self, repo_uid: &str) -> Result<usize, StorageError> {
        let deleted = self.connection().execute(
            "DELETE FROM semantic_facts WHERE repo_uid = ?",
            params![repo_uid],
        )?;
        Ok(deleted)
    }

    /// Insert a batch of semantic facts atomically.
    ///
    /// Generates deterministic UUIDs for each fact based on semantic
    /// identity. Returns the number of rows inserted.
    ///
    /// The batch is wrapped in a transaction: either all facts are
    /// inserted or none are.
    pub fn insert_semantic_facts(
        &mut self,
        facts: &[NewSemanticFact],
    ) -> Result<usize, StorageError> {
        if facts.is_empty() {
            return Ok(0);
        }

        let tx = self.connection_mut().transaction()?;
        let now = tx.query_row(
            "SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            [],
            |row| row.get::<_, String>(0),
        )?;

        // Sort by confidence descending so highest-quality evidence is inserted first
        let mut sorted_facts: Vec<_> = facts.iter().collect();
        sorted_facts.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

        let mut inserted = 0;
        for fact in sorted_facts {
            let fact_uid = derive_fact_uid(fact);
            // INSERT OR IGNORE: deduplicate facts with same semantic identity.
            // Sorted by confidence, so highest-quality evidence wins.
            let rows = tx.execute(
                r#"
                INSERT OR IGNORE INTO semantic_facts (
                    fact_uid, repo_uid, fact_kind,
                    subject_ref, subject_ref_kind,
                    object_ref, object_ref_kind,
                    source_file, source_line_start, source_line_end,
                    source_text_excerpt, content_hash,
                    extraction_method, confidence, generated, doc_kind,
                    extracted_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
                params![
                    fact_uid,
                    fact.repo_uid,
                    fact.fact_kind,
                    fact.subject_ref,
                    fact.subject_ref_kind,
                    fact.object_ref,
                    fact.object_ref_kind,
                    fact.source_file,
                    fact.source_line_start,
                    fact.source_line_end,
                    fact.source_text_excerpt,
                    fact.content_hash,
                    fact.extraction_method,
                    fact.confidence,
                    if fact.generated { 1 } else { 0 },
                    fact.doc_kind,
                    now,
                ],
            )?;
            inserted += rows;
        }

        tx.commit()?;
        Ok(inserted)
    }

    /// Replace all semantic facts for a repository atomically.
    ///
    /// Validates that all facts in the batch belong to the target repo,
    /// then atomically deletes existing facts and inserts new ones.
    /// This is the primary operation for `rmap docs`.
    ///
    /// Returns `InvalidArgument` error if any fact's repo_uid does not
    /// match the target repo_uid.
    pub fn replace_semantic_facts_for_repo(
        &mut self,
        repo_uid: &str,
        facts: &[NewSemanticFact],
    ) -> Result<ReplaceResult, StorageError> {
        // Validate all facts belong to target repo before starting transaction
        for (i, fact) in facts.iter().enumerate() {
            if fact.repo_uid != repo_uid {
                return Err(StorageError::InvalidArgument(format!(
                    "fact[{}].repo_uid '{}' does not match target repo '{}'",
                    i, fact.repo_uid, repo_uid
                )));
            }
        }

        let tx = self.connection_mut().transaction()?;

        // Delete existing facts
        let deleted = tx.execute(
            "DELETE FROM semantic_facts WHERE repo_uid = ?",
            params![repo_uid],
        )?;

        // Insert new facts, sorted by confidence descending.
        // This ensures that when INSERT OR IGNORE deduplicates, the
        // highest-confidence extraction method wins rather than
        // depending on arbitrary extractor iteration order.
        let now = tx.query_row(
            "SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            [],
            |row| row.get::<_, String>(0),
        )?;
        let mut inserted = 0;

        // Sort by confidence descending so highest-quality evidence is inserted first
        let mut sorted_facts: Vec<_> = facts.iter().collect();
        sorted_facts.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

        for fact in sorted_facts {
            let fact_uid = derive_fact_uid(fact);
            // INSERT OR IGNORE: deduplicate facts with same semantic identity.
            // Sorted by confidence, so highest-quality evidence wins.
            let rows = tx.execute(
                r#"
                INSERT OR IGNORE INTO semantic_facts (
                    fact_uid, repo_uid, fact_kind,
                    subject_ref, subject_ref_kind,
                    object_ref, object_ref_kind,
                    source_file, source_line_start, source_line_end,
                    source_text_excerpt, content_hash,
                    extraction_method, confidence, generated, doc_kind,
                    extracted_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
                params![
                    fact_uid,
                    fact.repo_uid,
                    fact.fact_kind,
                    fact.subject_ref,
                    fact.subject_ref_kind,
                    fact.object_ref,
                    fact.object_ref_kind,
                    fact.source_file,
                    fact.source_line_start,
                    fact.source_line_end,
                    fact.source_text_excerpt,
                    fact.content_hash,
                    fact.extraction_method,
                    fact.confidence,
                    if fact.generated { 1 } else { 0 },
                    fact.doc_kind,
                    now,
                ],
            )?;
            inserted += rows;
        }

        tx.commit()?;

        Ok(ReplaceResult { deleted, inserted })
    }

    /// Query semantic facts for a repository.
    pub fn get_semantic_facts_for_repo(
        &self,
        repo_uid: &str,
    ) -> Result<Vec<SemanticFact>, StorageError> {
        let mut stmt = self.connection().prepare(
            r#"
            SELECT
                fact_uid, repo_uid, fact_kind,
                subject_ref, subject_ref_kind,
                object_ref, object_ref_kind,
                source_file, source_line_start, source_line_end,
                source_text_excerpt, content_hash,
                extraction_method, confidence, generated, doc_kind,
                extracted_at
            FROM semantic_facts
            WHERE repo_uid = ?
            ORDER BY source_file, source_line_start
            "#,
        )?;

        let facts = stmt
            .query_map(params![repo_uid], row_to_semantic_fact)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(facts)
    }

    /// Query semantic facts by kind for a repository.
    pub fn get_semantic_facts_by_kind(
        &self,
        repo_uid: &str,
        fact_kind: &str,
    ) -> Result<Vec<SemanticFact>, StorageError> {
        let mut stmt = self.connection().prepare(
            r#"
            SELECT
                fact_uid, repo_uid, fact_kind,
                subject_ref, subject_ref_kind,
                object_ref, object_ref_kind,
                source_file, source_line_start, source_line_end,
                source_text_excerpt, content_hash,
                extraction_method, confidence, generated, doc_kind,
                extracted_at
            FROM semantic_facts
            WHERE repo_uid = ? AND fact_kind = ?
            ORDER BY confidence DESC, source_file
            "#,
        )?;

        let facts = stmt
            .query_map(params![repo_uid, fact_kind], row_to_semantic_fact)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(facts)
    }

    /// Count semantic facts by kind for a repository.
    pub fn count_semantic_facts_by_kind(
        &self,
        repo_uid: &str,
    ) -> Result<Vec<(String, i64)>, StorageError> {
        let mut stmt = self.connection().prepare(
            r#"
            SELECT fact_kind, COUNT(*) as count
            FROM semantic_facts
            WHERE repo_uid = ?
            GROUP BY fact_kind
            ORDER BY count DESC
            "#,
        )?;

        let counts = stmt
            .query_map(params![repo_uid], |row: &Row<'_>| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(counts)
    }
}

/// Result of a replace operation.
#[derive(Debug, Clone)]
pub struct ReplaceResult {
    pub deleted: usize,
    pub inserted: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crud::test_helpers::{fresh_storage, make_repo};

    fn setup_conn() -> StorageConnection {
        let conn = fresh_storage();
        conn.add_repo(&make_repo("r1")).unwrap();
        conn
    }

    fn make_fact(repo_uid: &str, fact_kind: &str, subject: &str) -> NewSemanticFact {
        NewSemanticFact {
            repo_uid: repo_uid.to_string(),
            fact_kind: fact_kind.to_string(),
            subject_ref: subject.to_string(),
            subject_ref_kind: "module".to_string(),
            object_ref: Some("target".to_string()),
            object_ref_kind: Some("module".to_string()),
            source_file: "README.md".to_string(),
            source_line_start: Some(10),
            source_line_end: Some(12),
            source_text_excerpt: Some("excerpt".to_string()),
            content_hash: "abc123".to_string(),
            extraction_method: "keyword_pattern".to_string(),
            confidence: 0.8,
            generated: false,
            doc_kind: "readme".to_string(),
        }
    }

    #[test]
    fn insert_and_query_semantic_facts() {
        let mut conn = setup_conn();

        let facts = vec![
            make_fact("r1", "replacement_for", "new-module"),
            NewSemanticFact {
                repo_uid: "r1".to_string(),
                fact_kind: "environment_surface".to_string(),
                subject_ref: "api".to_string(),
                subject_ref_kind: "module".to_string(),
                object_ref: Some("production".to_string()),
                object_ref_kind: Some("environment".to_string()),
                source_file: "docker-compose.yml".to_string(),
                source_line_start: Some(5),
                source_line_end: Some(5),
                source_text_excerpt: None,
                content_hash: "def456".to_string(),
                extraction_method: "config_parse".to_string(),
                confidence: 0.95,
                generated: false,
                doc_kind: "config".to_string(),
            },
        ];

        let inserted = conn.insert_semantic_facts(&facts).unwrap();
        assert_eq!(inserted, 2);

        let all_facts = conn.get_semantic_facts_for_repo("r1").unwrap();
        assert_eq!(all_facts.len(), 2);

        let replacement_facts = conn
            .get_semantic_facts_by_kind("r1", "replacement_for")
            .unwrap();
        assert_eq!(replacement_facts.len(), 1);
        assert_eq!(replacement_facts[0].subject_ref, "new-module");
    }

    #[test]
    fn replace_semantic_facts() {
        let mut conn = setup_conn();

        // Insert initial facts
        let initial = vec![make_fact("r1", "deprecated_by", "OldApi")];
        conn.insert_semantic_facts(&initial).unwrap();
        assert_eq!(conn.get_semantic_facts_for_repo("r1").unwrap().len(), 1);

        // Replace with new facts
        let replacement = vec![
            make_fact("r1", "alternative_to", "serviceA"),
            make_fact("r1", "alternative_to", "serviceB"),
        ];

        let result = conn
            .replace_semantic_facts_for_repo("r1", &replacement)
            .unwrap();
        assert_eq!(result.deleted, 1);
        assert_eq!(result.inserted, 2);

        let all_facts = conn.get_semantic_facts_for_repo("r1").unwrap();
        assert_eq!(all_facts.len(), 2);

        // Old fact should be gone
        let deprecated = conn
            .get_semantic_facts_by_kind("r1", "deprecated_by")
            .unwrap();
        assert_eq!(deprecated.len(), 0);
    }

    #[test]
    fn count_by_kind() {
        let mut conn = setup_conn();

        let facts = vec![
            make_fact("r1", "replacement_for", "a"),
            make_fact("r1", "replacement_for", "c"),
            NewSemanticFact {
                repo_uid: "r1".to_string(),
                fact_kind: "environment_surface".to_string(),
                subject_ref: "api".to_string(),
                subject_ref_kind: "module".to_string(),
                object_ref: Some("prod".to_string()),
                object_ref_kind: Some("environment".to_string()),
                source_file: "docker-compose.yml".to_string(),
                source_line_start: None,
                source_line_end: None,
                source_text_excerpt: None,
                content_hash: "h2".to_string(),
                extraction_method: "config_parse".to_string(),
                confidence: 0.95,
                generated: false,
                doc_kind: "config".to_string(),
            },
        ];

        conn.insert_semantic_facts(&facts).unwrap();

        let counts = conn.count_semantic_facts_by_kind("r1").unwrap();
        assert_eq!(counts.len(), 2);

        // Should be sorted by count DESC
        assert_eq!(counts[0].0, "replacement_for");
        assert_eq!(counts[0].1, 2);
        assert_eq!(counts[1].0, "environment_surface");
        assert_eq!(counts[1].1, 1);
    }

    // ── P1 fix: repo mismatch rejection ──────────────────────────

    #[test]
    fn replace_rejects_mismatched_repo_uid() {
        let mut conn = setup_conn();
        conn.add_repo(&make_repo("r2")).unwrap();

        // Attempt to replace r1 with facts belonging to r2
        let facts = vec![make_fact("r2", "replacement_for", "intruder")];

        let result = conn.replace_semantic_facts_for_repo("r1", &facts);
        assert!(result.is_err());

        match result.unwrap_err() {
            StorageError::InvalidArgument(msg) => {
                assert!(msg.contains("r2"));
                assert!(msg.contains("r1"));
                assert!(msg.contains("does not match"));
            }
            other => panic!("expected InvalidArgument, got {:?}", other),
        }

        // r1 should be unchanged (no facts deleted)
        let facts = conn.get_semantic_facts_for_repo("r1").unwrap();
        assert_eq!(facts.len(), 0);
    }

    #[test]
    fn replace_rejects_mixed_repo_batch() {
        let mut conn = setup_conn();
        conn.add_repo(&make_repo("r2")).unwrap();

        // First fact is correct, second is wrong repo
        let facts = vec![
            make_fact("r1", "replacement_for", "good"),
            make_fact("r2", "replacement_for", "bad"),
        ];

        let result = conn.replace_semantic_facts_for_repo("r1", &facts);
        assert!(result.is_err());

        match result.unwrap_err() {
            StorageError::InvalidArgument(msg) => {
                assert!(msg.contains("fact[1]"));
            }
            other => panic!("expected InvalidArgument, got {:?}", other),
        }
    }

    // ── P2 fix: deterministic fact_uid ───────────────────────────

    #[test]
    fn same_fact_produces_same_uid_across_runs() {
        let mut conn = setup_conn();

        let fact = make_fact("r1", "replacement_for", "module-x");
        conn.insert_semantic_facts(&[fact.clone()]).unwrap();

        let first_run = conn.get_semantic_facts_for_repo("r1").unwrap();
        assert_eq!(first_run.len(), 1);
        let uid_first = first_run[0].fact_uid.clone();

        // Delete and re-insert the same fact
        conn.delete_semantic_facts_for_repo("r1").unwrap();
        conn.insert_semantic_facts(&[fact]).unwrap();

        let second_run = conn.get_semantic_facts_for_repo("r1").unwrap();
        assert_eq!(second_run.len(), 1);
        let uid_second = &second_run[0].fact_uid;

        assert_eq!(
            uid_first, *uid_second,
            "same semantic fact must produce same fact_uid"
        );
    }

    #[test]
    fn non_identity_fields_do_not_affect_uid() {
        // Two facts with same identity but different non-identity fields
        let fact_v1 = NewSemanticFact {
            repo_uid: "r1".to_string(),
            fact_kind: "replacement_for".to_string(),
            subject_ref: "module-x".to_string(),
            subject_ref_kind: "module".to_string(),
            object_ref: Some("module-y".to_string()),
            object_ref_kind: Some("module".to_string()),
            source_file: "README.md".to_string(),
            source_line_start: Some(10),
            source_line_end: Some(12),
            // Non-identity fields:
            source_text_excerpt: Some("old excerpt".to_string()),
            content_hash: "hash-v1".to_string(),
            extraction_method: "keyword_pattern".to_string(),
            confidence: 0.7,
            generated: false,
            doc_kind: "readme".to_string(),
        };

        let fact_v2 = NewSemanticFact {
            // Same identity fields
            repo_uid: "r1".to_string(),
            fact_kind: "replacement_for".to_string(),
            subject_ref: "module-x".to_string(),
            subject_ref_kind: "module".to_string(),
            object_ref: Some("module-y".to_string()),
            object_ref_kind: Some("module".to_string()),
            source_file: "README.md".to_string(),
            source_line_start: Some(10),
            source_line_end: Some(12),
            // Different non-identity fields:
            source_text_excerpt: Some("improved excerpt".to_string()),
            content_hash: "hash-v2".to_string(),
            extraction_method: "explicit_marker".to_string(), // improved
            confidence: 0.95, // higher confidence
            generated: true,  // different
            doc_kind: "readme".to_string(),
        };

        let uid_v1 = derive_fact_uid(&fact_v1);
        let uid_v2 = derive_fact_uid(&fact_v2);

        assert_eq!(
            uid_v1, uid_v2,
            "non-identity fields must not affect fact_uid"
        );
    }

    #[test]
    fn identity_field_change_produces_different_uid() {
        let fact_a = make_fact("r1", "replacement_for", "module-a");
        let fact_b = make_fact("r1", "replacement_for", "module-b");

        let uid_a = derive_fact_uid(&fact_a);
        let uid_b = derive_fact_uid(&fact_b);

        assert_ne!(uid_a, uid_b, "different identity must produce different uid");
    }

    #[test]
    fn derived_uid_is_valid_uuid() {
        let fact = make_fact("r1", "replacement_for", "test");
        let uid = derive_fact_uid(&fact);

        // Should parse as valid UUID
        let parsed = Uuid::parse_str(&uid);
        assert!(parsed.is_ok(), "derived uid must be valid UUID");

        // Should be UUID v5
        let uuid = parsed.unwrap();
        assert_eq!(uuid.get_version_num(), 5, "must be UUID v5");
    }

    // ── INSERT OR IGNORE deduplication ────────────────────────────

    #[test]
    fn insert_batch_deduplicates_silently() {
        let mut conn = setup_conn();

        // Insert one fact
        let fact1 = make_fact("r1", "replacement_for", "module-a");
        let inserted1 = conn.insert_semantic_facts(&[fact1.clone()]).unwrap();
        assert_eq!(inserted1, 1);
        assert_eq!(conn.get_semantic_facts_for_repo("r1").unwrap().len(), 1);

        // Insert a batch where one fact is new and one is duplicate.
        // INSERT OR IGNORE: duplicate is silently ignored, new one inserted.
        let batch = vec![
            make_fact("r1", "replacement_for", "module-b"), // new
            fact1.clone(), // duplicate - will be ignored
        ];

        let inserted2 = conn.insert_semantic_facts(&batch).unwrap();
        assert_eq!(inserted2, 1, "only non-duplicate should be inserted");

        // Both original and new fact should exist
        let facts = conn.get_semantic_facts_for_repo("r1").unwrap();
        assert_eq!(facts.len(), 2, "should have 2 unique facts");

        let subjects: Vec<_> = facts.iter().map(|f| &f.subject_ref).collect();
        assert!(subjects.contains(&&"module-a".to_string()));
        assert!(subjects.contains(&&"module-b".to_string()));
    }

    #[test]
    fn deduplication_keeps_highest_confidence() {
        let mut conn = setup_conn();

        // Create two facts with same semantic identity but different confidence.
        // The identity tuple excludes confidence, so they'll have the same fact_uid.
        let mut low_conf = make_fact("r1", "replacement_for", "module-a");
        low_conf.confidence = 0.5;
        low_conf.extraction_method = "keyword_pattern".to_string();

        let mut high_conf = make_fact("r1", "replacement_for", "module-a");
        high_conf.confidence = 0.9;
        high_conf.extraction_method = "explicit_marker".to_string();

        // Insert low-confidence first, then high-confidence second.
        // The sort-by-confidence-descending should ensure high wins.
        let batch = vec![low_conf.clone(), high_conf.clone()];
        let inserted = conn.insert_semantic_facts(&batch).unwrap();
        assert_eq!(inserted, 1, "duplicates should be deduplicated");

        let facts = conn.get_semantic_facts_for_repo("r1").unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(
            facts[0].confidence, 0.9,
            "highest confidence should win regardless of input order"
        );
        assert_eq!(
            facts[0].extraction_method, "explicit_marker",
            "extraction method should match highest-confidence fact"
        );
    }
}
