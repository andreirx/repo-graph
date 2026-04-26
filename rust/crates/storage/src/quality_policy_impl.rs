//! `QualityPolicyStoragePort` implementation for `StorageConnection`.
//!
//! This module implements the quality policy storage port on top of
//! the storage adapter's rusqlite connection.
//!
//! The implementation provides:
//! - Active quality policy loading with JSON parsing
//! - Enriched measurement loading (joined with nodes/files for scope metadata)
//! - Atomic assessment replacement
//!
//! **Error handling:** all methods propagate `StorageError` through
//! the `Result` return. No silent coercion of SQL or JSON errors.

use crate::connection::StorageConnection;
use crate::error::StorageError;
use crate::quality_policy_port::{EnrichedMeasurement, LoadedPolicy, QualityPolicyStoragePort};
use crate::types::QualityAssessmentInput;

impl QualityPolicyStoragePort for StorageConnection {
    fn load_active_quality_policies(
        &self,
        repo_uid: &str,
    ) -> Result<Vec<LoadedPolicy>, StorageError> {
        // Reuse existing method that loads parsed payloads.
        let declarations = self.get_active_quality_policy_declarations(repo_uid)?;

        Ok(declarations
            .into_iter()
            .map(|d| LoadedPolicy {
                policy_uid: d.declaration_uid,
                payload: d.payload,
            })
            .collect())
    }

    fn load_enriched_measurements(
        &self,
        snapshot_uid: &str,
        kinds: &[&str],
    ) -> Result<Vec<EnrichedMeasurement>, StorageError> {
        if kinds.is_empty() {
            return Ok(vec![]);
        }

        // Build IN clause for kinds.
        let placeholders: Vec<&str> = kinds.iter().map(|_| "?").collect();
        let in_clause = placeholders.join(", ");

        // Query measurements joined with nodes and files for scope metadata.
        // The join path is:
        //   measurements.target_stable_key = nodes.stable_key
        //   nodes.file_uid = files.file_uid
        //
        // For SYMBOL nodes, we get the file path from files.path.
        // For FILE nodes, we get the file path from stable_key parsing.
        // For MODULE/REPO nodes, file_path is NULL.
        //
        // symbol_kind is nodes.subtype for kind='SYMBOL' nodes, NULL otherwise.
        let sql = format!(
            "SELECT
                m.target_stable_key,
                m.kind,
                m.value_json,
                f.path AS file_path,
                CASE WHEN n.kind = 'SYMBOL' THEN n.subtype ELSE NULL END AS symbol_kind
             FROM measurements m
             LEFT JOIN nodes n ON m.target_stable_key = n.stable_key
                AND m.snapshot_uid = n.snapshot_uid
             LEFT JOIN files f ON n.file_uid = f.file_uid
             WHERE m.snapshot_uid = ? AND m.kind IN ({})
             ORDER BY m.target_stable_key",
            in_clause
        );

        let conn = self.connection();
        let mut stmt = conn.prepare(&sql)?;

        // Build parameters: snapshot_uid first, then all kinds.
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        params.push(Box::new(snapshot_uid.to_string()));
        for kind in kinds {
            params.push(Box::new(kind.to_string()));
        }
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            let target_stable_key: String = row.get(0)?;
            let measurement_kind: String = row.get(1)?;
            let value_json: String = row.get(2)?;
            let file_path: Option<String> = row.get(3)?;
            let symbol_kind: Option<String> = row.get(4)?;

            Ok((target_stable_key, measurement_kind, value_json, file_path, symbol_kind))
        })?;

        let mut results = Vec::new();
        for row_result in rows {
            let (target_stable_key, measurement_kind, value_json, file_path, symbol_kind) =
                row_result?;

            // Parse value from JSON: { "value": <number> }
            let value = parse_measurement_value(&value_json, &target_stable_key)?;

            results.push(EnrichedMeasurement {
                target_stable_key,
                measurement_kind,
                value,
                file_path,
                symbol_kind,
            });
        }

        Ok(results)
    }

    fn replace_assessments(
        &mut self,
        snapshot_uid: &str,
        assessments: &[QualityAssessmentInput],
    ) -> Result<usize, StorageError> {
        // Delegate to existing method.
        self.replace_quality_assessments_for_snapshot(snapshot_uid, assessments)
    }
}

/// Parse the numeric value from a measurement's value_json.
///
/// Expected format: `{ "value": <number> }` or just `<number>`.
fn parse_measurement_value(value_json: &str, target_stable_key: &str) -> Result<f64, StorageError> {
    // Try parsing as a JSON object with "value" field.
    if let Ok(obj) = serde_json::from_str::<serde_json::Value>(value_json) {
        if let Some(val) = obj.get("value") {
            if let Some(num) = val.as_f64() {
                return Ok(num);
            }
            if let Some(num) = val.as_i64() {
                return Ok(num as f64);
            }
        }
        // If the root is a number, use it directly.
        if let Some(num) = obj.as_f64() {
            return Ok(num);
        }
        if let Some(num) = obj.as_i64() {
            return Ok(num as f64);
        }
    }

    // Try parsing as a bare number.
    if let Ok(num) = value_json.parse::<f64>() {
        return Ok(num);
    }

    Err(StorageError::MeasurementParseError {
        target_stable_key: target_stable_key.to_string(),
        reason: format!("cannot parse numeric value from: {}", value_json),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_measurement_value_object_format() {
        let result = parse_measurement_value(r#"{"value": 42.5}"#, "test");
        assert_eq!(result.unwrap(), 42.5);
    }

    #[test]
    fn parse_measurement_value_object_integer() {
        let result = parse_measurement_value(r#"{"value": 10}"#, "test");
        assert_eq!(result.unwrap(), 10.0);
    }

    #[test]
    fn parse_measurement_value_bare_number() {
        let result = parse_measurement_value("25.0", "test");
        assert_eq!(result.unwrap(), 25.0);
    }

    #[test]
    fn parse_measurement_value_bare_integer() {
        let result = parse_measurement_value("7", "test");
        assert_eq!(result.unwrap(), 7.0);
    }

    #[test]
    fn parse_measurement_value_invalid() {
        let result = parse_measurement_value(r#"{"notvalue": 10}"#, "test");
        assert!(result.is_err());
    }
}
