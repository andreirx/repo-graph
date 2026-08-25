//! The content pin. This must be **byte-identical** to the scanner's
//! `hash_content` (`rust/crates/repo-index/src/scanner.rs`), because the
//! background embed pass re-runs it on the working-tree bytes and admits a file
//! only when the result equals the snapshot's recorded
//! `file_versions.content_hash` (spec §3.5). Any divergence would make every
//! file look "drifted" and the store would come out empty.

use sha2::{Digest, Sha256};

/// `SHA-256(content.as_bytes()).hex()[0..16]` — the exact scanner form
/// (`format!("{:x}", digest)` lowercase hex, first 16 hex chars = 8-byte
/// prefix). Verified against the scanner's own test vectors:
/// `content_hash("hello world") == "b94d27b9934d3e08"`,
/// `content_hash("") == "e3b0c44298fc1c14"`.
pub fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let digest = hasher.finalize();
    let hex = format!("{:x}", digest);
    hex[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_scanner_test_vectors() {
        // Parity with rust/crates/repo-index/src/scanner.rs test anchors.
        assert_eq!(content_hash("hello world"), "b94d27b9934d3e08");
        assert_eq!(content_hash(""), "e3b0c44298fc1c14");
        assert_eq!(content_hash("hello world").len(), 16);
    }
}
