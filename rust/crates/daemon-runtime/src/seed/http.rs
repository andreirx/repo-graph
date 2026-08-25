//! The a2 accepted-response contract (D-ES-9): parse + validate a raw HTTP/1.1
//! response into `Vec<Vec<f32>>`. Split out of `transport.rs` (operator ruling 2,
//! 2026-08-25 — that file exceeded the 500-line guardrail); this holds the pure
//! response-parsing concern (header/body framing, the OpenAI body shape, and the
//! non-finite/zero-norm/dim/echoed-model checks), while `transport.rs` keeps the
//! socket/connection concern.
//!
//! Abstraction one-liner: crate-private cohesion split of ONE adapter under the
//! 500-line guardrail; concrete user = `super::transport` (`embed` + the doctor
//! probe); axis = the file-size guardrail, NOT a new public boundary (nothing here
//! is `pub`). Rejected simpler: one 528-line `transport.rs` (breaches the guardrail).

use repo_graph_seed::ports::EmbedError;

// a2 accepted-response contract bounds (spec D-ES-9), fixed here so independent
// implementations are compatible. `pub(super)` because `transport::post` sizes its
// read cap from the same bounds.
pub(super) const MAX_HEADER_BYTES: usize = 64 * 1024; // 64 KiB header section
pub(super) const MAX_BODY_BYTES: usize = 32 * 1024 * 1024; // 32 MiB response body

/// Split + validate the raw HTTP response against the a2 accepted-response
/// contract, then extract the `Content-Length`-delimited JSON body bytes.
///
/// D-ES-9: the body length must EQUAL the declared `Content-Length` EXACTLY — a
/// short body is truncated and a long body carries trailing bytes; BOTH are
/// `Malformed` (operator ruling 2, review-3 #2). We never parse a prefix of an
/// over-long body: a framing we cannot trust end-to-end degrades honestly.
pub(super) fn extract_http_body(raw: &[u8]) -> Result<&[u8], EmbedError> {
    let sep = find_subslice(raw, b"\r\n\r\n").ok_or_else(|| EmbedError::Malformed {
        detail: "no header/body separator (not HTTP/1.1?)".to_string(),
    })?;
    if sep > MAX_HEADER_BYTES {
        return Err(EmbedError::Malformed {
            detail: "header section exceeds cap".to_string(),
        });
    }
    let header_text = std::str::from_utf8(&raw[..sep]).map_err(|_| EmbedError::Malformed {
        detail: "non-UTF8 headers (TLS or binary?)".to_string(),
    })?;
    let mut lines = header_text.split("\r\n");
    let status = lines.next().unwrap_or("");
    // "HTTP/1.1 200 OK" / "HTTP/1.0 200 OK"
    let ok = {
        let mut parts = status.split_whitespace();
        let ver = parts.next().unwrap_or("");
        let code = parts.next().unwrap_or("");
        (ver == "HTTP/1.1" || ver == "HTTP/1.0") && code == "200"
    };
    if !ok {
        return Err(EmbedError::Malformed {
            detail: format!("non-200 status line: {status:?}"),
        });
    }
    let mut content_length: Option<usize> = None;
    for line in lines {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let key = k.trim().to_ascii_lowercase();
        let val = v.trim();
        if key == "transfer-encoding" && val.to_ascii_lowercase().contains("chunked") {
            return Err(EmbedError::Malformed {
                detail: "chunked transfer-encoding unsupported".to_string(),
            });
        }
        if key == "content-length" {
            let n = val.parse::<usize>().map_err(|_| EmbedError::Malformed {
                detail: "non-integer Content-Length".to_string(),
            })?;
            if content_length.is_some() {
                return Err(EmbedError::Malformed {
                    detail: "duplicate Content-Length".to_string(),
                });
            }
            content_length = Some(n);
        }
    }
    let len = content_length.ok_or_else(|| EmbedError::Malformed {
        detail: "missing Content-Length".to_string(),
    })?;
    if len > MAX_BODY_BYTES {
        return Err(EmbedError::Malformed {
            detail: "body exceeds cap".to_string(),
        });
    }
    let body = &raw[sep + 4..];
    // D-ES-9: EXACT equality. `<` is a truncated body; `>` is trailing bytes after
    // the declared length — a framing we cannot trust; both degrade as Malformed.
    if body.len() != len {
        return Err(EmbedError::Malformed {
            detail: format!(
                "body length {} does not equal Content-Length {len}",
                body.len()
            ),
        });
    }
    Ok(body)
}

#[derive(serde::Deserialize)]
pub(super) struct EmbeddingsResponse {
    // Read only inside this module (`parse_embeddings`); `transport`'s doctor probe
    // reads `model` only, so `data` stays module-private.
    data: Vec<EmbeddingItem>,
    #[serde(default)]
    pub(super) model: Option<String>,
}

#[derive(serde::Deserialize)]
struct EmbeddingItem {
    index: usize,
    embedding: Vec<f32>,
}

/// Parse + validate the OpenAI-shaped body (spec D-ES-9 a2 contract): correlate
/// by `index` (a unique permutation of `0..n`), reject non-finite/zero-norm
/// vectors, enforce `dim`, and hard-fail on an echoed-but-different model.
pub(super) fn parse_embeddings(
    raw: &[u8],
    expected_n: usize,
    dim: usize,
    pinned_model: &str,
) -> Result<Vec<Vec<f32>>, EmbedError> {
    let body = extract_http_body(raw)?;
    let resp: EmbeddingsResponse =
        serde_json::from_slice(body).map_err(|e| EmbedError::Malformed {
            detail: format!("response JSON: {e}"),
        })?;

    // Wire-time echoed-model check (spec §7.1): present-and-different ⇒ hard fail.
    if let Some(m) = resp.model.as_deref() {
        if !m.is_empty() && m != pinned_model {
            return Err(EmbedError::ModelMismatch {
                expected: pinned_model.to_string(),
                got: m.to_string(),
            });
        }
    }

    if resp.data.len() != expected_n {
        return Err(EmbedError::Malformed {
            detail: format!("expected {expected_n} vectors, got {}", resp.data.len()),
        });
    }

    // Correlate by `index` — a unique permutation of 0..n. Never by position.
    let mut slots: Vec<Option<Vec<f32>>> = (0..expected_n).map(|_| None).collect();
    for item in resp.data {
        if item.index >= expected_n {
            return Err(EmbedError::Malformed {
                detail: format!("index {} out of range 0..{}", item.index, expected_n),
            });
        }
        if slots[item.index].is_some() {
            return Err(EmbedError::Malformed {
                detail: format!("duplicate index {}", item.index),
            });
        }
        if item.embedding.len() != dim {
            return Err(EmbedError::DimMismatch {
                expected: dim,
                got: item.embedding.len(),
            });
        }
        if item.embedding.iter().any(|x| !x.is_finite()) {
            return Err(EmbedError::Malformed {
                detail: "non-finite vector component".to_string(),
            });
        }
        let norm_sq: f32 = item.embedding.iter().map(|x| x * x).sum();
        if norm_sq <= 0.0 {
            return Err(EmbedError::Malformed {
                detail: "zero-norm vector".to_string(),
            });
        }
        slots[item.index] = Some(item.embedding);
    }
    let mut out = Vec::with_capacity(expected_n);
    for slot in slots {
        match slot {
            Some(v) => out.push(v),
            None => {
                return Err(EmbedError::Malformed {
                    detail: "index permutation has a gap".to_string(),
                })
            }
        }
    }
    Ok(out)
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openai_body_and_correlates_by_index() {
        let body = r#"{"model":"text-embedding-nomic-embed-text-v1.5","data":[{"index":1,"embedding":[0.0,1.0]},{"index":0,"embedding":[1.0,0.0]}]}"#;
        let raw = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let v =
            parse_embeddings(raw.as_bytes(), 2, 2, "text-embedding-nomic-embed-text-v1.5").unwrap();
        // reordered by index: slot 0 = [1,0], slot 1 = [0,1]
        assert_eq!(v[0], vec![1.0, 0.0]);
        assert_eq!(v[1], vec![0.0, 1.0]);
    }

    #[test]
    fn rejects_chunked_and_non_200_and_bad_index() {
        let chunked = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
        assert!(matches!(
            parse_embeddings(chunked.as_bytes(), 1, 2, "m"),
            Err(EmbedError::Malformed { .. })
        ));
        let non200 = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n";
        assert!(matches!(
            parse_embeddings(non200.as_bytes(), 1, 2, "m"),
            Err(EmbedError::Malformed { .. })
        ));
        let body = r#"{"data":[{"index":5,"embedding":[1.0,0.0]}]}"#;
        let raw = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        assert!(matches!(
            parse_embeddings(raw.as_bytes(), 1, 2, "m"),
            Err(EmbedError::Malformed { .. })
        ));
    }

    #[test]
    fn echoed_model_mismatch_hard_fails() {
        let body = r#"{"model":"WRONG","data":[{"index":0,"embedding":[1.0,0.0]}]}"#;
        let raw = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        assert!(matches!(
            parse_embeddings(raw.as_bytes(), 1, 2, "right-model"),
            Err(EmbedError::ModelMismatch { .. })
        ));
    }

    #[test]
    fn zero_norm_vector_rejected() {
        let body = r#"{"data":[{"index":0,"embedding":[0.0,0.0]}]}"#;
        let raw = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        assert!(matches!(
            parse_embeddings(raw.as_bytes(), 1, 2, "m"),
            Err(EmbedError::Malformed { .. })
        ));
    }

    /// D-ES-9 (review-3 #2): a body LONGER than Content-Length carries trailing
    /// bytes — the framing is untrustworthy, so it must be Malformed, NEVER parsed
    /// from the declared-length prefix.
    #[test]
    fn trailing_bytes_after_content_length_are_malformed() {
        let body = r#"{"data":[{"index":0,"embedding":[1.0,0.0]}]}"#;
        // Declare a Content-Length SHORTER than the actual body → trailing bytes.
        let raw = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}TRAILING-GARBAGE",
            body.len(),
            body
        );
        assert!(
            matches!(
                extract_http_body(raw.as_bytes()),
                Err(EmbedError::Malformed { .. })
            ),
            "a body longer than Content-Length must degrade as Malformed"
        );
        assert!(matches!(
            parse_embeddings(raw.as_bytes(), 1, 2, "m"),
            Err(EmbedError::Malformed { .. })
        ));
    }

    /// A body SHORTER than Content-Length is a truncated frame → Malformed.
    #[test]
    fn truncated_body_shorter_than_content_length_is_malformed() {
        let body = r#"{"data":[]}"#;
        let raw = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
            body.len() + 50, // claim more than we send
            body
        );
        assert!(matches!(
            extract_http_body(raw.as_bytes()),
            Err(EmbedError::Malformed { .. })
        ));
    }
}
