//! repo-graph-indexer — Rust port of the indexer orchestration layer.
//!
//! This crate is the Rust-side mirror of the TypeScript indexer at
//! `src/adapters/indexer/repo-indexer.ts` and the port interfaces
//! at `src/core/ports/indexer.ts` and `src/core/ports/extractor.ts`.
//!
//! Slice substep state (Rust-5):
//!   - R5-A workspace skeleton ........... done
//!   - R5-B first storage subtraits ...... done
//!   - R5-C storage impl batch 1 ........ done
//!   - R5-D routing policy .............. done
//!   - R5-E edge resolution policy ...... done
//!   - R5-F storage subtraits batch 2 ... done
//!   - R5-G core indexing orchestration .. done
//!   - R5-H delta indexing .............. done
//!   - R5-I parity harness .............. done
//!   - R5-J script integration .......... done
//!   - R5-K final acceptance gate ....... done
//!
//! ── Architecture ─────────────────────────────────────────────
//!
//! This crate is POLICY code. It defines the orchestration
//! workflow and the ports it needs:
//!
//!   - `ExtractorPort`: the interface extractors implement.
//!     Defined here (policy owns the interface). Concrete
//!     extractor adapters (tree-sitter) are NOT in this crate.
//!
//!   - `IndexerStoragePort`: a composed facade over narrow
//!     sub-traits (`SnapshotLifecyclePort`, `FileCatalogPort`,
//!     `NodeStorePort`, `EdgeStorePort`, `UnresolvedEdgePort`,
//!     `FileSignalPort`, `DeltaCopyPort`). Each sub-trait covers
//!     one phase's storage needs. The facade is a blanket impl
//!     for any type implementing all sub-traits. The storage
//!     crate (adapter) implements the sub-traits on
//!     `StorageConnection`.
//!
//!   - Classification integration: uses `classify_unresolved_edge`
//!     and `derive_blast_radius` from `repo-graph-classification`
//!     for unresolved edge classification during resolution.
//!
//!   - Trust integration: NOT in the indexer. The indexer persists
//!     extraction diagnostics; the trust report is computed
//!     on-demand by the CLI via `repo-graph-trust`.
//!
//! This crate does NOT depend on `repo-graph-storage`. The
//! dependency direction is adapter → policy (storage implements
//! the indexer-owned traits).
//!
//! ── Locked contract divergence: sync ExtractorPort ───────────
//!
//! The TS `ExtractorPort` uses async (`Promise<ExtractionResult>`).
//! The Rust trait is synchronous. This is a deliberate divergence
//! locked at R5-A: tree-sitter parsing is CPU-bound, the Rust
//! extractor receives source text (no I/O), and `async fn` in
//! traits is not object-safe (conflicts with `&dyn ExtractorPort`
//! for heterogeneous collections). Future async needs (LSP
//! enrichment) belong on a separate enrichment port, not the base
//! extraction port. See `extractor_port.rs` module doc for the
//! full rationale.
//!
//! ── Scope (Rust-5 lock) ──────────────────────────────────────
//!
//! Core indexing orchestration only:
//!   - snapshot creation and finalization
//!   - file routing (extension → extractor)
//!   - extraction invocation through `ExtractorPort`
//!   - edge resolution (resolver index, resolve by type)
//!   - unresolved edge classification
//!   - module node/edge creation
//!   - delta/refresh indexing (invalidation + copy-forward)
//!
//! NOT in scope (deferred enrichment):
//!   - postpasses (module discovery, boundary extraction,
//!     framework detectors, seam detection)
//!   - annotations port
//!   - measurements/metrics persistence
//!   - concrete extractor implementations (Rust-6)
//!   - CLI entry point
//!   - daemon process work
//!   - filesystem I/O (caller provides file content)
//!
//! ── Storage sub-trait growth ──────────────────────────────────
//!
//! Sub-traits are added progressively per substep:
//!   - R5-B: SnapshotLifecyclePort, FileCatalogPort
//!   - R5-F: NodeStorePort, EdgeStorePort, UnresolvedEdgePort, FileSignalPort
//!   - R5-H: DeltaCopyPort
//!
//! The IndexerStoragePort facade grows to include each new
//! sub-trait as it is defined.

pub mod cargo_manifest;
pub mod extractor_port;
pub mod grpc_client_hint;
pub mod grpc_impl_hint;
pub mod grpc_link;
pub mod grpc_registration_proof;
pub mod hook;
pub mod http_link;
pub mod include_resolver;
pub mod index_timing;
pub mod inferred_modules;
pub mod invalidation;
pub mod java_code_mapper;
pub mod jsts_extensions;
pub mod orchestrator;
pub mod package_json;
pub mod proto_indexer;
pub mod pyproject;
pub mod refresh_dispatch;
pub mod resolver;
pub mod routing;
pub mod settings_gradle;
pub mod storage_port;
pub mod types;

// ── Public re-exports ────────────────────────────────────────

pub use cargo_manifest::{
    generate_evidence_uid, generate_module_key, generate_module_uid, parse_cargo_toml,
    to_storage_inputs, CargoEvidencePayload, CargoModule, CargoModuleCandidateInput,
    CargoModuleEvidenceInput, CargoModuleStorePort, CargoParseResult, FileOwnershipInput,
};
pub use extractor_port::{ExtractorError, ExtractorPort};
pub use grpc_client_hint::{
    find_grpc_client_hints, run_grpc_client_hint_detection, GrpcClientHint, GrpcClientHintResult,
    StubType,
};
pub use grpc_impl_hint::{
    find_grpc_impl_hints, generate_association_uid, generate_surface_uid,
    run_grpc_impl_hint_detection, GrpcImplHint, GrpcImplHintResult, ImplBaseExtensionInput,
    ImplBaseMappingInput,
};
pub use grpc_link::{find_grpc_links, run_grpc_link_detection, GrpcLink, GrpcLinkResult};
pub use grpc_registration_proof::{run_grpc_registration_proof, GrpcRegistrationProofResult};
pub use index_timing::PhaseTimings;
pub use inferred_modules::{
    detect_inferred_modules, generate_evidence_uid as generate_inferred_evidence_uid,
    generate_module_key as generate_inferred_module_key,
    generate_module_uid as generate_inferred_module_uid,
    to_storage_inputs as inferred_to_storage_inputs, InferredEvidencePayload, InferredModule,
    InferredModuleResult, INFERRED_MODULE_CONFIDENCE,
};
pub use java_code_mapper::{
    find_java_mappings, ContractElementContext, GeneratedCodeMapping, JavaSymbol, MappingBasis,
    MappingEvidence, ProtoOptions,
};
pub use package_json::{
    generate_evidence_uid as generate_npm_evidence_uid,
    generate_module_key as generate_npm_module_key, generate_module_uid as generate_npm_module_uid,
    parse_package_json, parse_pnpm_workspace, to_storage_inputs as npm_to_storage_inputs,
    NpmEvidencePayload, NpmModule, PackageJsonParseResult, PnpmWorkspaceParseResult,
};
pub use proto_indexer::{index_proto_files, ProtoFileInput, ProtoIndexResult, ProtoParseFailure};
pub use pyproject::{
    generate_evidence_uid as generate_pyproject_evidence_uid,
    generate_module_key as generate_pyproject_module_key,
    generate_module_uid as generate_pyproject_module_uid, parse_pyproject_toml,
    to_storage_inputs as pyproject_to_storage_inputs, PyprojectEvidencePayload, PyprojectModule,
    PyprojectParseResult,
};
pub use refresh_dispatch::{
    dispatch_recompute_relationships, should_recompute, should_reextract, RecomputeDispatchResult,
};
pub use settings_gradle::{
    generate_evidence_uid as generate_gradle_evidence_uid,
    generate_module_key as generate_gradle_module_key,
    generate_module_uid as generate_gradle_module_uid, parse_settings_gradle,
    to_storage_inputs as gradle_to_storage_inputs, GradleEvidencePayload, GradleModule,
    SettingsGradleParseResult,
};
pub use storage_port::{
    AddServiceCallInput, DeltaCopyPort, EdgeStorePort, FileCatalogPort, FileSignalPort,
    GeneratedCodeMappingInput, GeneratedCodeMappingReadPort, GeneratedCodeMappingStorePort,
    GrpcClientContractInput, GrpcClientHintReadPort, GrpcClientHintStorePort,
    GrpcClientSurfaceInput, GrpcImplContractInput, GrpcImplHintReadPort, GrpcImplHintStorePort,
    GrpcImplSurfaceInput, GrpcImplSurfaceMatch, GrpcRegistrationProofPort, GrpcServiceMappingInput,
    IndexerStoragePort, NodeStorePort, ProtoElementInput, ProtoSchemaInput, ProtoSchemaStorePort,
    RegistrationSiteInput, SnapshotLifecyclePort, StubCreationInput, UnresolvedEdgePort,
};
pub use storage_port::{
    BoundaryInteractionLinkInput, GrpcLinkReadPort, GrpcLinkStorePort, SurfaceWithContract,
};
