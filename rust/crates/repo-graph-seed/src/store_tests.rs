//! Unit tests for the `.vec` sidecar store (`store.rs`). Split via `#[path]` (the
//! repo idiom, e.g. `pass_tests.rs` / `seed_pass_tests.rs`) so `store.rs` stays under
//! the 500-line structural guardrail — pure relocation, no behavior change. Child
//! module of `store`; `super::*` = the store API under test.

use super::*;

fn key() -> SeedStoreKey {
    SeedStoreKey {
        model_id: "text-embedding-nomic-embed-text-v1.5".to_string(),
        dim: 4,
        repo_graph_version: "0.8.0".to_string(),
    }
}

fn body() -> SeedVectorBodyV1 {
    SeedVectorBodyV1 {
        entries: vec![SeedVectorEntryV1 {
            file_uid: "f1".to_string(),
            path: "a.ts".to_string(),
            content_hash: "abcd0123abcd0123".to_string(),
            vector: vec![0.5, 0.5, 0.5, 0.5],
        }],
    }
}

#[test]
fn round_trip() {
    let bytes = encode(&body(), &key(), 1234).unwrap();
    let decoded = decode(&bytes, &key()).unwrap();
    assert_eq!(decoded, body());
}

#[test]
fn pin_mismatch_discards() {
    let bytes = encode(&body(), &key(), 0).unwrap();
    let mut other = key();
    other.model_id = "some-other-model".to_string();
    assert!(matches!(
        decode(&bytes, &other),
        Err(SeedStoreError::KeyMismatch)
    ));
    let mut dimchg = key();
    dimchg.dim = 8;
    assert!(matches!(
        decode(&bytes, &dimchg),
        Err(SeedStoreError::KeyMismatch)
    ));
}

#[test]
fn corruption_discards() {
    let mut bytes = encode(&body(), &key(), 0).unwrap();
    // Flip a byte in the tail (the payload region) → checksum mismatch.
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    let err = decode(&bytes, &key());
    assert!(
        matches!(
            err,
            Err(SeedStoreError::ChecksumMismatch)
                | Err(SeedStoreError::Truncated)
                | Err(SeedStoreError::Decode(_))
        ),
        "corrupted store must be discarded, got {err:?}"
    );
}

#[test]
fn wrong_dim_entry_rejects_whole_store() {
    // key.dim == 4; a 2-dim vector must reject the ENTIRE store, not be filtered.
    let bad = SeedVectorBodyV1 {
        entries: vec![SeedVectorEntryV1 {
            file_uid: "f".to_string(),
            path: "a.ts".to_string(),
            content_hash: "h".to_string(),
            vector: vec![1.0, 0.0],
        }],
    };
    let bytes = encode(&bad, &key(), 0).unwrap();
    assert!(matches!(
        decode(&bytes, &key()),
        Err(SeedStoreError::BodyEntryInvalid { .. })
    ));
}

#[test]
fn non_finite_entry_rejects_whole_store() {
    let bad = SeedVectorBodyV1 {
        entries: vec![SeedVectorEntryV1 {
            file_uid: "f".to_string(),
            path: "a.ts".to_string(),
            content_hash: "h".to_string(),
            vector: vec![f32::NAN, 0.0, 0.0, 0.0],
        }],
    };
    let bytes = encode(&bad, &key(), 0).unwrap();
    assert!(matches!(
        decode(&bytes, &key()),
        Err(SeedStoreError::BodyEntryInvalid { .. })
    ));
}

#[test]
fn unnormalized_entry_rejects_whole_store() {
    // norm = sqrt(4 * 25) = 10, well outside the unit-norm tolerance.
    let bad = SeedVectorBodyV1 {
        entries: vec![SeedVectorEntryV1 {
            file_uid: "f".to_string(),
            path: "a.ts".to_string(),
            content_hash: "h".to_string(),
            vector: vec![5.0, 5.0, 5.0, 5.0],
        }],
    };
    let bytes = encode(&bad, &key(), 0).unwrap();
    assert!(matches!(
        decode(&bytes, &key()),
        Err(SeedStoreError::BodyEntryInvalid { .. })
    ));
}

#[test]
fn truncated_body_reports_truncated_before_checksum() {
    // Hand-forge a manifest whose content_length disagrees with the payload:
    // the ratified order reports Truncated (content_length step) — never a
    // size-cap or checksum error first.
    let payload = bincode::serialize(&body()).unwrap();
    let manifest = SeedManifest {
        magic: MAGIC,
        schema_version: SCHEMA_VERSION,
        key: key(),
        created_at: 0,
        content_length: payload.len() as u64 + 1, // deliberately wrong
        checksum: sha256_hex(&payload),
    };
    let envelope = SeedFileEnvelope { manifest, payload };
    let bytes = bincode::serialize(&envelope).unwrap();
    assert!(matches!(
        decode(&bytes, &key()),
        Err(SeedStoreError::Truncated)
    ));
}

#[test]
fn missing_file_is_not_found_not_io() {
    // Absence is the ONLY genuine "no store" — a distinct variant, never
    // collapsed into a generic Io error (honesty rule).
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope.vec");
    assert!(matches!(
        read_validated(&missing, &key()),
        Err(SeedStoreError::NotFound)
    ));
}

#[test]
fn oversized_file_rejected_by_metadata_guard_without_reading() {
    // review-9 #1: a multi-GB sidecar must be rejected from its metadata alone,
    // WITHOUT loading it. A sparse file (set_len, no data blocks) proves the
    // guard fires on length, not on bytes read — this test allocates no GBs.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("huge.vec");
    let f = File::create(&path).unwrap();
    f.set_len(MAX_FILE_BYTES + 1).unwrap(); // sparse — no real disk/mem cost
    drop(f);
    match read_validated(&path, &key()) {
        Err(SeedStoreError::FileTooLarge { file_bytes }) => {
            assert_eq!(file_bytes, MAX_FILE_BYTES + 1);
        }
        other => panic!("expected FileTooLarge from the metadata guard, got {other:?}"),
    }
}

#[test]
fn length_prefix_bomb_rejected_by_decode_limit() {
    // review-9 #1: a SMALL file whose internal payload length prefix claims a
    // huge value must be rejected by the deserializer's allocation ceiling —
    // never a pre-allocation bomb. Forge exactly that: valid framing, but the
    // payload length prefix overwritten with a value beyond MAX_FILE_BYTES.
    let payload = bincode::serialize(&body()).unwrap();
    let manifest = SeedManifest {
        magic: MAGIC,
        schema_version: SCHEMA_VERSION,
        key: key(),
        created_at: 0,
        content_length: payload.len() as u64,
        checksum: sha256_hex(&payload),
    };
    let header_len = bincode::serialize(&manifest).unwrap().len();
    let envelope = SeedFileEnvelope { manifest, payload };
    let mut bytes = bincode::serialize(&envelope).unwrap();
    // The payload's u64 length prefix sits immediately after the manifest.
    let bomb = (MAX_FILE_BYTES + 1_000_000).to_le_bytes();
    bytes[header_len..header_len + 8].copy_from_slice(&bomb);
    let err = decode(&bytes, &key());
    assert!(
        matches!(
            err,
            Err(SeedStoreError::Truncated) | Err(SeedStoreError::Decode(_))
        ),
        "length-prefix bomb must be rejected by the decode limit, got {err:?}"
    );
}

#[test]
fn atomic_write_publishes_and_reloads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("seed-vectors").join("deadbeef.vec");
    let bytes = encode(&body(), &key(), 7).unwrap();
    atomic_write(&path, &bytes).unwrap();
    let loaded = read_validated(&path, &key()).unwrap();
    assert_eq!(loaded, body());
    // no leftover temp
    let tmp = {
        let mut s = path.as_os_str().to_os_string();
        s.push(".tmp");
        PathBuf::from(s)
    };
    assert!(!tmp.exists());
}
