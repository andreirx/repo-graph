//! Contract schema storage port.
//!
//! This module defines the storage port trait for contract schema
//! (protobuf, gRPC, etc.) storage operations. The port is defined
//! in the storage crate because:
//!
//! 1. The contract-schema crate is a pure policy crate with no I/O
//! 2. Storage DTOs are defined here
//! 3. The indexer will use this port to persist parsed schemas
//!
//! ## Design Context
//!
//! Normative contract: `docs/slices/cs-1-protobuf-schema.md`
//!
//! This port supports CS-1 (protobuf schema extraction) and future
//! slices (CS-2 generated code mapping, GR-1/2/3 gRPC detection).

use crate::error::StorageError;

/// Input for inserting a contract schema.
///
/// Represents a parsed IDL file (e.g., `.proto`) ready for persistence.
#[derive(Debug, Clone)]
pub struct ContractSchemaInput {
    /// Unique identifier for this schema (deterministic, based on path + content).
    pub schema_uid: String,

    /// Snapshot this schema belongs to.
    pub snapshot_uid: String,

    /// Repository this schema belongs to.
    pub repo_uid: String,

    /// Schema kind ("protobuf", "grpc", "erpc", etc.).
    pub schema_kind: String,

    /// Repo-relative path to the IDL file.
    pub file_path: String,

    /// Package namespace (e.g., "api.v1").
    pub package_name: Option<String>,

    /// Syntax version ("proto2", "proto3").
    pub syntax_version: Option<String>,

    /// SHA-256 hash of file content for cache invalidation.
    pub content_hash: String,

    /// JSON array of imported file paths.
    pub imports_json: Option<String>,

    /// JSON object of file-level options.
    pub options_json: Option<String>,

    /// Extractor identifier (e.g., "proto-parser:0.1.0").
    pub extractor: String,
}

/// Input for inserting a contract element.
///
/// Represents a named element within a schema file (message, enum,
/// service, method, field).
#[derive(Debug, Clone)]
pub struct ContractElementInput {
    /// Unique identifier for this element.
    pub element_uid: String,

    /// Schema this element belongs to.
    pub schema_uid: String,

    /// Element kind ("message", "enum", "service", "method", "field").
    pub element_kind: String,

    /// Short name without package prefix.
    pub name: String,

    /// Fully qualified name (package.OuterMessage.InnerMessage).
    pub full_name: String,

    /// Parent element UID for nested elements (None for top-level).
    pub parent_element_uid: Option<String>,

    /// Source line where element starts.
    pub line_start: Option<u32>,

    /// Source line where element ends.
    pub line_end: Option<u32>,

    /// Element-specific details as JSON.
    pub metadata_json: Option<String>,
}

/// Stored contract schema row.
#[derive(Debug, Clone)]
pub struct ContractSchemaRow {
    /// Unique identifier.
    pub schema_uid: String,

    /// Snapshot UID.
    pub snapshot_uid: String,

    /// Repository UID.
    pub repo_uid: String,

    /// Schema kind.
    pub schema_kind: String,

    /// Repo-relative file path.
    pub file_path: String,

    /// Package namespace.
    pub package_name: Option<String>,

    /// Syntax version.
    pub syntax_version: Option<String>,

    /// Content hash.
    pub content_hash: String,

    /// Extractor identifier.
    pub extractor: String,

    /// Timestamp when parsed.
    pub parsed_at: String,
}

/// Stored contract element row.
#[derive(Debug, Clone)]
pub struct ContractElementRow {
    /// Unique identifier.
    pub element_uid: String,

    /// Schema UID.
    pub schema_uid: String,

    /// Element kind.
    pub element_kind: String,

    /// Short name.
    pub name: String,

    /// Fully qualified name.
    pub full_name: String,

    /// Parent element UID.
    pub parent_element_uid: Option<String>,

    /// Line start.
    pub line_start: Option<u32>,

    /// Line end.
    pub line_end: Option<u32>,

    /// Metadata JSON.
    pub metadata_json: Option<String>,
}

/// Port trait for contract schema storage operations.
///
/// Implemented by `StorageConnection`. Used by the indexer to persist
/// parsed schema files and by CLI commands to query schema data.
pub trait ContractSchemaStoragePort {
    /// Insert a contract schema.
    ///
    /// Uses INSERT OR IGNORE for idempotency.
    fn insert_contract_schema(&mut self, input: &ContractSchemaInput) -> Result<(), StorageError>;

    /// Insert multiple contract elements.
    ///
    /// Elements should be ordered such that parents come before children
    /// (for nested messages/enums). Uses INSERT OR IGNORE.
    fn insert_contract_elements(
        &mut self,
        elements: &[ContractElementInput],
    ) -> Result<usize, StorageError>;

    /// List all contract schemas for a snapshot.
    ///
    /// Optionally filter by schema kind.
    fn list_contract_schemas(
        &self,
        snapshot_uid: &str,
        kind_filter: Option<&str>,
    ) -> Result<Vec<ContractSchemaRow>, StorageError>;

    /// Get a contract schema by file path.
    fn get_schema_by_file(
        &self,
        snapshot_uid: &str,
        file_path: &str,
    ) -> Result<Option<ContractSchemaRow>, StorageError>;

    /// List contract elements for a schema.
    ///
    /// Optionally filter by element kind.
    fn list_elements_for_schema(
        &self,
        schema_uid: &str,
        kind_filter: Option<&str>,
    ) -> Result<Vec<ContractElementRow>, StorageError>;

    /// Find a contract element by full name.
    ///
    /// Searches across all schemas in the snapshot.
    fn find_element_by_full_name(
        &self,
        snapshot_uid: &str,
        full_name: &str,
    ) -> Result<Option<ContractElementRow>, StorageError>;

    /// Count contract schemas for a snapshot.
    fn count_schemas(&self, snapshot_uid: &str) -> Result<usize, StorageError>;

    /// Count contract elements for a snapshot.
    fn count_elements(&self, snapshot_uid: &str) -> Result<usize, StorageError>;

    /// List generated code mappings for a snapshot.
    ///
    /// Optionally filter by element UID.
    fn list_generated_code_mappings(
        &self,
        snapshot_uid: &str,
        element_uid_filter: Option<&str>,
    ) -> Result<Vec<GeneratedCodeMappingRow>, StorageError>;

    /// Count generated code mappings for a snapshot.
    fn count_generated_code_mappings(&self, snapshot_uid: &str) -> Result<usize, StorageError>;
}

/// Stored generated code mapping row.
#[derive(Debug, Clone)]
pub struct GeneratedCodeMappingRow {
    /// Unique mapping identifier.
    pub mapping_uid: String,

    /// Snapshot UID.
    pub snapshot_uid: String,

    /// Schema element UID.
    pub schema_element_uid: String,

    /// Stable key of the generated code symbol.
    pub generated_symbol_key: String,

    /// Language of the generated code.
    pub language: String,

    /// Path to the generated file.
    pub generated_file: String,

    /// Mapping basis (confidence tier).
    pub mapping_basis: String,

    /// Confidence score (0.0 - 1.0).
    pub confidence: f64,

    /// Additional evidence as JSON.
    pub metadata_json: Option<String>,

    /// Timestamp when mapping was created.
    pub created_at: String,
}
