//! Contract kind classification and common types.
//!
//! These types classify the schema/IDL system and extraction provenance
//! for contract-backed boundary interactions.

use serde::{Deserialize, Serialize};

/// Kind of contract/IDL system.
///
/// Determines which schema parser applies and how generated code is mapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractKind {
    /// Protocol Buffers (.proto files).
    /// Parser: tree-sitter-protobuf or protobuf-parse.
    Protobuf,

    /// gRPC service (layered on protobuf).
    /// Uses protobuf schema plus gRPC service definitions.
    Grpc,

    /// Embedded RPC (eRPC).
    /// Custom IDL parser required.
    Erpc,

    /// Cap'n Proto (future).
    CapnProto,

    /// FlatBuffers (future).
    FlatBuffers,

    /// Shared memory layout contract (explicit struct definition).
    /// Not IDL-based, but explicit contract nonetheless.
    ShmLayout,

    /// Custom binary protocol with documented layout.
    CustomBinary,

    /// No formal contract (raw bytes, implicit protocol).
    None,
}

impl ContractKind {
    /// SQL/storage string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            ContractKind::Protobuf => "protobuf",
            ContractKind::Grpc => "grpc",
            ContractKind::Erpc => "erpc",
            ContractKind::CapnProto => "capn_proto",
            ContractKind::FlatBuffers => "flatbuffers",
            ContractKind::ShmLayout => "shm_layout",
            ContractKind::CustomBinary => "custom_binary",
            ContractKind::None => "none",
        }
    }

    /// Whether this contract kind uses IDL files.
    pub const fn is_idl_based(self) -> bool {
        matches!(
            self,
            ContractKind::Protobuf
                | ContractKind::Grpc
                | ContractKind::Erpc
                | ContractKind::CapnProto
                | ContractKind::FlatBuffers
        )
    }

    /// Whether this contract kind supports cross-language generation.
    pub const fn is_cross_language(self) -> bool {
        matches!(
            self,
            ContractKind::Protobuf
                | ContractKind::Grpc
                | ContractKind::CapnProto
                | ContractKind::FlatBuffers
        )
    }
}

/// How the schema/contract was discovered.
///
/// Provenance for extraction confidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaProvenance {
    /// Parsed directly from IDL source file.
    IdlParsed,

    /// Inferred from generated code patterns.
    GeneratedCodeInferred,

    /// Declared via configuration or annotation.
    Declared,

    /// Extracted from runtime/reflection.
    RuntimeExtracted,
}

impl SchemaProvenance {
    /// SQL/storage string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            SchemaProvenance::IdlParsed => "idl_parsed",
            SchemaProvenance::GeneratedCodeInferred => "generated_code_inferred",
            SchemaProvenance::Declared => "declared",
            SchemaProvenance::RuntimeExtracted => "runtime_extracted",
        }
    }

    /// Default confidence score for this provenance type.
    pub const fn default_confidence(self) -> f64 {
        match self {
            SchemaProvenance::IdlParsed => 0.99,
            SchemaProvenance::GeneratedCodeInferred => 0.85,
            SchemaProvenance::Declared => 1.0,
            SchemaProvenance::RuntimeExtracted => 0.90,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_kind_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&ContractKind::Protobuf).unwrap(),
            "\"protobuf\""
        );
        assert_eq!(
            serde_json::to_string(&ContractKind::Grpc).unwrap(),
            "\"grpc\""
        );
    }

    #[test]
    fn grpc_is_idl_based() {
        assert!(ContractKind::Grpc.is_idl_based());
        assert!(ContractKind::Protobuf.is_idl_based());
        assert!(!ContractKind::None.is_idl_based());
    }

    #[test]
    fn schema_provenance_confidence_ordering() {
        assert!(
            SchemaProvenance::Declared.default_confidence()
                > SchemaProvenance::IdlParsed.default_confidence()
        );
        assert!(
            SchemaProvenance::IdlParsed.default_confidence()
                > SchemaProvenance::GeneratedCodeInferred.default_confidence()
        );
    }
}
