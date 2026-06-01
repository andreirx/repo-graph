# WARM-CACHE-1: `repo-graph-warm-cache` Support Crate (Stage D)

Slice ID: WARM-CACHE-1
Status: **DESIGN — build contract ratified; building.** Support crate only — NO daemon wiring, NO CLI,
NO warm-start runtime behavior. Implements PARTITIONED-WARM-CACHE-ARCH-1.
Depends: PARTITIONED-WARM-CACHE-ARCH-1 (the architecture), INGEST-CORE-1 (`PartitionIr`).
Track: Stage D, after the warm-cache architecture; before warm-cache daemon wiring.

## Purpose

A pure support crate that **serializes/validates** the warm-cache artifacts defined by
PARTITIONED-WARM-CACHE-ARCH-1: `PartitionIr` (the graph) + a `ValueFacts` sidecar, each under a
validated manifest, with atomic write. It does NOT wire into the daemon, run warm-start, or touch the
LiveGraph — those come in a later wiring slice.

## Ratified build scope

```text
repo-graph-warm-cache support crate
no daemon wiring | no CLI | no LiveGraph dependency | no scip-ingest dependency | no warm-start runtime
```

## Dependencies

```text
repo-graph-ir   (the domain graph artifact; converted via cache-side DTOs)
bincode         (D2: format first)
serde           (derive; ON THE CACHE DTOs, never on repo-graph-ir)
thiserror       (lightweight error type)
sha2 + hex      (D4 content checksum — the conventional workspace hash; consistent with build_inputs_hash)
```

**Do NOT add serde to `repo-graph-ir`** (D8). Serialization is infrastructure; the domain crate stays
zero-dep. The cache crate owns the serde-deriving mirror DTOs + `From`/`TryFrom` conversions.

## Core types

```text
CacheKey            { repo_uid, partition_id, build_inputs_hash, indexer_name, indexer_version }
CacheManifest       { magic, schema_version, repo_graph_version, key: CacheKey, created_at, content_length, checksum }
CachePartitionIrDto + IR sub-DTOs (node/edge/range/provenance/identity-source/edge-type/edge-basis/partition/kind)
CacheValueFactsDto  (Vec<CacheValueFactDto>)
CacheFileEnvelope<T> { manifest: CacheManifest, payload: <bytes> }
CacheValidationError / CacheError  (thiserror)
```

`CachePartitionIrDto`: `impl From<&PartitionIr>` + `impl TryFrom<CachePartitionIrDto> for PartitionIr`
(repo-graph-ir is a dep, so conversion is direct; `TryFrom` for untrusted input).

## Value-fact DTO independence (no LiveGraph dep)

`ValueFact` lives in `repo-graph-livegraph`, which this crate MUST NOT depend on. So the value-fact
DTO is defined INDEPENDENTLY (also avoiding a `repo-graph-trust-model` dep for `IdentityBasis`):

```text
CacheValueFactDto {
  subject: CacheValueSubjectDto,           // Symbol(String key) | RawAnchor(CacheSourceRangeDto)
  kind: CacheValueFactKindDto,             // CyclomaticComplexity
  value: u32,
  basis: CacheIdentityBasisDto,            // mirror of trust-model IdentityBasis
  source_range: Option<CacheSourceRangeDto>,
  provenance: CacheProvenanceDto,
}
```

The later wiring layer (which holds both `repo-graph-livegraph` + `repo-graph-warm-cache`) converts
LiveGraph `ValueFact` ↔ `CacheValueFactDto`. **No dependency cycle; no LiveGraph/trust-model dep here.**

## Required functions

```text
encode_partition(ir: &PartitionIr, manifest) -> Vec<u8>
decode_partition(bytes, expected_key) -> Result<PartitionIr, CacheError>     // validate manifest, then convert

encode_value_facts(facts: &[CacheValueFactDto], manifest) -> Vec<u8>
decode_value_facts(bytes, expected_key) -> Result<Vec<CacheValueFactDto>, CacheError>

atomic_write(path, bytes) -> Result<(), CacheError>     // temp → fsync → rename → fsync parent (D5)
read_validated(path, expected_key) -> Result<Vec<u8>, CacheError>   // read + manifest/checksum validate
```

## Important semantic rule (D7 independence)

```text
Partition-cache validity and ValueFacts-sidecar validity are INDEPENDENT.
An invalid/absent ValueFacts sidecar MUST NOT invalidate the partition cache.
```

## Required tests

```text
partition_ir_roundtrip_preserves_semantics          (PartitionIr → DTO → bytes → DTO → PartitionIr equal)
value_facts_sidecar_roundtrip_preserves_semantics   (CacheValueFactDto[] → bytes → [] equal)
manifest_key_mismatch_rejected                       (D4)
schema_version_mismatch_rejected                     (D4)
checksum_mismatch_rejected                           (D4)
truncated_payload_rejected                           (D4)
atomic_write_replaces_old_file                       (D5)
invalid_value_facts_sidecar_does_not_invalidate_partition_cache   (D7 independence)
```

## Out of scope (hard guardrails)

```text
No daemon wiring (a later slice). No CLI. No LiveGraph / scip-ingest dependency. No warm-start runtime
behavior. No serde in repo-graph-ir. No rkyv (D2: bincode first). No value-fact recompute (that is the
wiring layer + the runtime).
```

## Definition of done

- `repo-graph-warm-cache` builds (deps: repo-graph-ir + bincode + serde + thiserror + sha2/hex only);
  the 6 core type groups + the 6 functions exist; all 8 required tests green; clippy `-D warnings`;
  fmt clean; workspace builds. `repo-graph-ir` unchanged (no serde added).

## Commit structure (ratified)

```text
1. spec:  Record WARM-CACHE-1 support-crate build contract
2. impl:  Add repo-graph-warm-cache support crate
```

Do NOT wire it into the 1C daemon yet — warm-cache daemon wiring is the next slice after this passes.

## References
- `docs/slices/partitioned-warm-cache-arch-1.md` (the ratified D1–D8 architecture)
- `docs/slices/ingest-core-1.md` (`PartitionIr` shape; group10 zero-dep invariant)
- `docs/slices/value-join-1.md` (`ValueFact` shape — mirrored independently here)
