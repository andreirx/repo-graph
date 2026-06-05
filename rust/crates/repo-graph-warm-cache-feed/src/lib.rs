#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! # repo-graph-warm-cache-feed — value-fact cache<->runtime adapter (WARM-CACHE-VALUEFACTS-1)
//!
//! The ONLY crate that depends on BOTH the runtime (`repo-graph-livegraph`) and the warm cache
//! (`repo-graph-warm-cache`). It owns:
//! - the total, semantic `ValueFact <-> CacheValueFactDto` conversion (the warm-cache crate omits it
//!   to stay free of LiveGraph/trust-model deps; this is the outer adapter that bridges them), and
//! - the value-facts warm-load feed ([`feed_partition_ir_with_value_facts`]).
//!
//! It mirrors `repo-graph-livegraph-feed` (the ingest<->runtime adapter). No daemon / CLI /
//! scip-ingest dependency. The daemon wiring (WARM-CACHE-DAEMON-WIRING continued) uses these.
//!
//! Independence (WARM-CACHE-1 / ARCH D7): a value-facts sidecar is OPTIONAL for serving graph queries;
//! [`try_decode_value_facts_sidecar`] returns `None` on any miss/corruption so the caller falls back to
//! a graph-only warm load. The `SourceRange`/`Provenance` field conversions are REUSED from
//! `repo-graph-warm-cache` (its `From` impls), not re-implemented.

use repo_graph_ir::{CanonicalKey, PartitionIr, Provenance, SourceRange};
use repo_graph_livegraph::{LiveGraph, ValueFact, ValueFactKind, ValueSubject};
use repo_graph_trust_model::{IdentityBasis, LanguageSupport};
use repo_graph_warm_cache::{
    decode_value_facts, encode_value_facts, CacheIdentityBasisDto, CacheKey, CacheManifest,
    CacheSourceRangeDto, CacheValueFactDto, CacheValueFactKindDto, CacheValueSubjectDto,
};

// ─────────────────────────────────────────────────────────────────────────────────────────────────
// ValueFact <-> CacheValueFactDto conversion (D5: total + semantic-round-trip-tested)
// ─────────────────────────────────────────────────────────────────────────────────────────────────

fn basis_to_dto(b: IdentityBasis) -> CacheIdentityBasisDto {
    match b {
        IdentityBasis::AstAdopted => CacheIdentityBasisDto::AstAdopted,
        IdentityBasis::ScipSynthesized => CacheIdentityBasisDto::ScipSynthesized,
        IdentityBasis::AstFileScope => CacheIdentityBasisDto::AstFileScope,
        IdentityBasis::DeclarationMapExact => CacheIdentityBasisDto::DeclarationMapExact,
        IdentityBasis::NameExactUnique => CacheIdentityBasisDto::NameExactUnique,
        IdentityBasis::RangeNameConfirmed => CacheIdentityBasisDto::RangeNameConfirmed,
        IdentityBasis::RawAnchored => CacheIdentityBasisDto::RawAnchored,
    }
}
fn basis_from_dto(b: CacheIdentityBasisDto) -> IdentityBasis {
    match b {
        CacheIdentityBasisDto::AstAdopted => IdentityBasis::AstAdopted,
        CacheIdentityBasisDto::ScipSynthesized => IdentityBasis::ScipSynthesized,
        CacheIdentityBasisDto::AstFileScope => IdentityBasis::AstFileScope,
        CacheIdentityBasisDto::DeclarationMapExact => IdentityBasis::DeclarationMapExact,
        CacheIdentityBasisDto::NameExactUnique => IdentityBasis::NameExactUnique,
        CacheIdentityBasisDto::RangeNameConfirmed => IdentityBasis::RangeNameConfirmed,
        CacheIdentityBasisDto::RawAnchored => IdentityBasis::RawAnchored,
    }
}

fn kind_to_dto(k: ValueFactKind) -> CacheValueFactKindDto {
    match k {
        ValueFactKind::CyclomaticComplexity => CacheValueFactKindDto::CyclomaticComplexity,
    }
}
fn kind_from_dto(k: CacheValueFactKindDto) -> ValueFactKind {
    match k {
        CacheValueFactKindDto::CyclomaticComplexity => ValueFactKind::CyclomaticComplexity,
    }
}

fn subject_to_dto(s: &ValueSubject) -> CacheValueSubjectDto {
    match s {
        ValueSubject::Symbol(k) => CacheValueSubjectDto::Symbol(k.as_str().to_string()),
        ValueSubject::RawAnchor(r) => CacheValueSubjectDto::RawAnchor(CacheSourceRangeDto::from(r)),
    }
}
fn subject_from_dto(s: CacheValueSubjectDto) -> ValueSubject {
    match s {
        CacheValueSubjectDto::Symbol(k) => ValueSubject::Symbol(CanonicalKey::from_existing(&k)),
        CacheValueSubjectDto::RawAnchor(r) => ValueSubject::RawAnchor(SourceRange::from(r)),
    }
}

/// Convert one runtime [`ValueFact`] to its cache DTO. `source_range`/`provenance` reuse the
/// `repo-graph-warm-cache` `From` impls (single source of truth for those field mappings).
pub fn value_fact_to_dto(f: &ValueFact) -> CacheValueFactDto {
    CacheValueFactDto {
        subject: subject_to_dto(&f.subject),
        kind: kind_to_dto(f.kind),
        value: f.value,
        basis: basis_to_dto(f.basis),
        source_range: f.source_range.as_ref().map(CacheSourceRangeDto::from),
        provenance: (&f.provenance).into(),
    }
}

/// Convert one cache DTO back to a runtime [`ValueFact`]. Total (cannot fail): every variant maps.
pub fn value_fact_from_dto(d: CacheValueFactDto) -> ValueFact {
    ValueFact {
        subject: subject_from_dto(d.subject),
        kind: kind_from_dto(d.kind),
        value: d.value,
        basis: basis_from_dto(d.basis),
        source_range: d.source_range.map(SourceRange::from),
        provenance: Provenance::from(d.provenance),
    }
}

/// Convert a slice of runtime value facts to cache DTOs.
pub fn value_facts_to_dtos(facts: &[ValueFact]) -> Vec<CacheValueFactDto> {
    facts.iter().map(value_fact_to_dto).collect()
}

/// Convert decoded cache DTOs back to runtime value facts.
pub fn value_facts_from_dtos(dtos: Vec<CacheValueFactDto>) -> Vec<ValueFact> {
    dtos.into_iter().map(value_fact_from_dto).collect()
}

// ─────────────────────────────────────────────────────────────────────────────────────────────────
// Sidecar codec wrappers (convert + the warm-cache validated envelope)
// ─────────────────────────────────────────────────────────────────────────────────────────────────

/// Encode a value-facts sidecar: convert the runtime facts to DTOs and wrap them in the warm-cache
/// validated envelope (`manifest` provides the key; magic/schema/length/checksum are filled by
/// `encode_value_facts`). Infallible.
pub fn encode_value_facts_sidecar(facts: &[ValueFact], manifest: CacheManifest) -> Vec<u8> {
    encode_value_facts(&value_facts_to_dtos(facts), manifest)
}

/// Best-effort decode of a value-facts sidecar into runtime facts: validate (key + schema + checksum)
/// and convert. Returns `None` on ANY miss / mismatch / corruption (D7 independence: the caller falls
/// back to a graph-only warm load — a sidecar failure NEVER blocks the graph).
pub fn try_decode_value_facts_sidecar(
    bytes: &[u8],
    expected_key: &CacheKey,
) -> Option<Vec<ValueFact>> {
    decode_value_facts(bytes, expected_key)
        .ok()
        .map(value_facts_from_dtos)
}

// ─────────────────────────────────────────────────────────────────────────────────────────────────
// Warm-load feed (graph + value facts, epoch-coherent)
// ─────────────────────────────────────────────────────────────────────────────────────────────────

/// Observability for a value-facts warm load (D2). `graph_loaded` is always true (the graph is fed
/// FIRST + unconditionally); `value_facts_loaded` reports whether value facts were attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WarmFeedResult {
    /// The partition graph (IR + xref) was loaded resident.
    pub graph_loaded: bool,
    /// The value facts were loaded for the just-loaded partition epoch.
    pub value_facts_loaded: bool,
}

/// Feed a decoded `PartitionIr` AND its value facts into the runtime on a warm cache hit.
///
/// Order (D2 + D7): load the graph FIRST and unconditionally, THEN load the value facts for the
/// just-loaded partition epoch. `load_value_facts` stamps the current epoch, so the facts are
/// epoch-coherent; a later swap without reload makes them detectably `Stale` (never silently attached
/// to a new epoch). Because the graph is loaded first, it remains loaded regardless of the value-facts
/// step. The DTO->ValueFact conversion is pure/total and is performed by the caller before this call
/// (via [`try_decode_value_facts_sidecar`]), so the facts load does not fail here; `WarmFeedResult`
/// is returned for daemon observability.
///
/// For a graph-only warm load (no valid sidecar) the caller uses
/// `repo_graph_livegraph_feed::feed_partition_ir` instead — value facts stay Unavailable.
pub fn feed_partition_ir_with_value_facts(
    lg: &mut LiveGraph,
    partition_id: &str,
    ir: PartitionIr,
    value_facts: Vec<ValueFact>,
    language: LanguageSupport,
) -> WarmFeedResult {
    lg.load_partition(partition_id, ir, language);
    lg.load_value_facts(partition_id, value_facts);
    WarmFeedResult {
        graph_loaded: true,
        value_facts_loaded: true,
    }
}

// ─────────────────────────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_ir::{
        IdentitySource, IrNode, Partition, PartitionId, PartitionKind, SourceRange,
    };
    use repo_graph_trust_model::{AnswerClass, FreshnessState};

    fn prov() -> Provenance {
        Provenance {
            indexer: "scip-typescript".to_string(),
            indexer_version: "0.4.0".to_string(),
            scip_symbol_id: Some("scip:sym".to_string()),
            build_inputs_hash: "abc123".to_string(),
        }
    }

    fn range() -> SourceRange {
        SourceRange {
            file: "src/main.ts".to_string(),
            start_line: 1,
            start_col: 2,
            end_line: 3,
            end_col: 4,
        }
    }

    fn symbol_fact(key: &str, basis: IdentityBasis) -> ValueFact {
        ValueFact {
            subject: ValueSubject::Symbol(CanonicalKey::from_existing(key)),
            kind: ValueFactKind::CyclomaticComplexity,
            value: 7,
            basis,
            source_range: Some(range()),
            provenance: prov(),
        }
    }

    fn raw_anchor_fact() -> ValueFact {
        ValueFact {
            subject: ValueSubject::RawAnchor(range()),
            kind: ValueFactKind::CyclomaticComplexity,
            value: 2,
            basis: IdentityBasis::RawAnchored,
            source_range: None,
            provenance: prov(),
        }
    }

    fn one_node_ir(key: &str) -> PartitionIr {
        let partition = Partition {
            id: PartitionId::new("p"),
            kind: PartitionKind::TsPackage,
            root: "/repo".to_string(),
            indexer: "scip-typescript".to_string(),
            indexer_version: "0.4.0".to_string(),
            build_inputs_hash: "abc123".to_string(),
            package_name: None,
            declared_dependencies: std::collections::BTreeSet::new(),
        };
        let mut ir = PartitionIr::new(partition);
        ir.nodes.push(IrNode {
            key: CanonicalKey::from_existing(key),
            subtype: "function".to_string(),
            name: "report".to_string(),
            range: Some(range()),
            partition_id: PartitionId::new("p"),
            identity_source: IdentitySource::AstAdopted,
            provenance: prov(),
        });
        ir
    }

    #[test]
    fn value_fact_roundtrip_preserves_semantics() {
        let f = symbol_fact("repo:src/main.ts#report", IdentityBasis::AstAdopted);
        let back = value_fact_from_dto(value_fact_to_dto(&f));
        assert_eq!(f, back, "ValueFact -> DTO -> ValueFact must be equal");
    }

    #[test]
    fn raw_anchor_value_fact_roundtrip_preserves_semantics() {
        let f = raw_anchor_fact();
        let back = value_fact_from_dto(value_fact_to_dto(&f));
        assert_eq!(f, back, "raw-anchored ValueFact must round-trip");
        assert!(matches!(back.subject, ValueSubject::RawAnchor(_)));
    }

    #[test]
    fn all_identity_basis_variants_roundtrip() {
        for basis in [
            IdentityBasis::AstAdopted,
            IdentityBasis::ScipSynthesized,
            IdentityBasis::AstFileScope,
            IdentityBasis::DeclarationMapExact,
            IdentityBasis::NameExactUnique,
            IdentityBasis::RangeNameConfirmed,
            IdentityBasis::RawAnchored,
        ] {
            let f = symbol_fact("repo:src/main.ts#report", basis);
            let back = value_fact_from_dto(value_fact_to_dto(&f));
            assert_eq!(back.basis, basis, "basis {basis:?} must round-trip");
            assert_eq!(f, back);
        }
    }

    #[test]
    fn feed_partition_ir_with_value_facts_loads_same_epoch() {
        let key = "repo:src/main.ts#report";
        let mut lg = LiveGraph::new();
        let result = feed_partition_ir_with_value_facts(
            &mut lg,
            "p",
            one_node_ir(key),
            vec![symbol_fact(key, IdentityBasis::AstAdopted)],
            LanguageSupport::TypeScriptPrimary,
        );
        assert_eq!(
            result,
            WarmFeedResult {
                graph_loaded: true,
                value_facts_loaded: true
            }
        );
        // The fact is queryable AND epoch-coherent: a fresh (non-Stale) answer with the fact present.
        let ans = lg.value_facts(key);
        assert_ne!(
            ans.class(),
            AnswerClass::Unavailable,
            "the loaded value fact must be retrievable"
        );
        assert_eq!(
            ans.freshness(),
            FreshnessState::Fresh,
            "value facts must be loaded at the graph epoch (not Stale)"
        );
    }

    #[test]
    fn sidecar_decode_failure_does_not_block_graph_feed() {
        let key = repo_graph_warm_cache::CacheKey {
            repo_uid: "repo_1".to_string(),
            partition_id: "p".to_string(),
            source_inputs_hash: "abc123".to_string(),
            producer_fingerprint: repo_graph_warm_cache::ProducerFingerprint {
                name: "scip-typescript".to_string(),
                version: "0.4.0".to_string(),
            },
            repo_graph_version: "0.1.0".to_string(),
        };
        // A corrupt sidecar decodes to None (never panics, never blocks).
        assert!(try_decode_value_facts_sidecar(b"garbage-not-a-sidecar", &key).is_none());

        // The graph still feeds (the daemon's graph-only fallback): load the IR, value facts absent.
        let symbol = "repo:src/main.ts#report";
        let mut lg = LiveGraph::new();
        lg.load_partition("p", one_node_ir(symbol), LanguageSupport::TypeScriptPrimary);
        assert!(lg.partition_epoch("p").is_some(), "graph must be loaded");
        assert_eq!(
            lg.value_facts(symbol).class(),
            AnswerClass::Unavailable,
            "no sidecar -> value facts Unavailable (not faked)"
        );
    }
}
