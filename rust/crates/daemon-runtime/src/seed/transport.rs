//! The option-(a) `Embedder` (D-ES-4/D-ES-9): a std-library loopback HTTP
//! transport — one framed HTTP/1.1 POST to `/v1/embeddings` over a raw
//! `TcpStream` (no HTTP-client dependency), plus the a2 accepted-response
//! contract (2s/30s timeouts, 64KiB/32MiB caps, non-200/chunked/TLS rejection,
//! index-correlated permutation, non-finite/zero-norm rejection, echoed-model
//! hard-fail). Only `Vec<Vec<f32>>` crosses the port boundary — never an HTTP
//! type. Loopback is enforced structurally at construction (I4): literal-IP
//! allowlist, no DNS, no proxy.

use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::Duration;

use repo_graph_seed::ports::{EmbedError, Embedder};

use super::http::{extract_http_body, parse_embeddings, EmbeddingsResponse};
use super::http::{MAX_BODY_BYTES, MAX_HEADER_BYTES};
use super::SeedEndpointConfig;

// a2 transport timeouts (spec D-ES-9). The size caps live with the response parser
// (`super::http`).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// The option-(a) `Embedder`: one POST to a loopback `/v1/embeddings` endpoint
/// over a raw `TcpStream` (D-ES-9 a2 — no HTTP-client dependency). Loopback is
/// enforced structurally at construction (I4): literal-IP allowlist, no DNS, no
/// proxy (a `TcpStream` inherently consults no proxy env).
pub struct EndpointEmbedder {
    ip: IpAddr,
    port: u16,
    path: String,
    host_header: String,
    model_id: String,
    dim: usize,
}

impl EndpointEmbedder {
    /// Construct from config, enforcing the loopback allowlist (spec §6.1).
    pub fn from_config(cfg: &SeedEndpointConfig) -> Result<Self, EmbedError> {
        let (ip, port, path) = parse_loopback_http(&cfg.endpoint)?;
        let host_header = match ip {
            IpAddr::V4(v4) => format!("{v4}:{port}"),
            IpAddr::V6(v6) => format!("[{v6}]:{port}"),
        };
        Ok(Self {
            ip,
            port,
            path,
            host_header,
            model_id: cfg.model_id.clone(),
            dim: cfg.dim,
        })
    }

    pub fn from_env() -> Result<Self, EmbedError> {
        Self::from_config(&SeedEndpointConfig::from_env())
    }
}

/// Parse `http://<loopback-ip>[:port]/<path>` (spec §6.1 points 1–2). Rejects any
/// non-`http` scheme, any host that is not a loopback IP literal (incl. the NAME
/// `localhost` and every DNS name — no resolution), with `NonLoopbackRejected`.
fn parse_loopback_http(endpoint: &str) -> Result<(IpAddr, u16, String), EmbedError> {
    let reject = || EmbedError::NonLoopbackRejected {
        endpoint: endpoint.to_string(),
    };
    // Scheme: http only (a2 has no TLS). Any other scheme ⇒ rejected.
    let rest = endpoint.strip_prefix("http://").ok_or_else(reject)?;
    // Split authority from path.
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/".to_string()),
    };
    // Split host:port. IPv6 literals are bracketed: [::1]:1234.
    let (host, port) = if let Some(hb) = authority.strip_prefix('[') {
        // [ipv6](:port)?
        let close = hb.find(']').ok_or_else(reject)?;
        let host = &hb[..close];
        let after = &hb[close + 1..];
        let port = after
            .strip_prefix(':')
            .map(|p| p.parse::<u16>().map_err(|_| reject()))
            .transpose()?
            .unwrap_or(80);
        (host.to_string(), port)
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse::<u16>().map_err(|_| reject())?),
            None => (authority.to_string(), 80u16),
        }
    };
    // Host MUST be an IP literal in a loopback range — no DNS, no `localhost`.
    let ip: IpAddr = host.parse().map_err(|_| reject())?;
    if !ip.is_loopback() {
        return Err(reject());
    }
    Ok((ip, port, path))
}

/// The doctor-time provenance of the pinned model id (spec §9). It is determined
/// by a single loopback probe AT DOCTOR TIME — the only honest source of
/// "endpoint-echoed", because the pass's wire-time echo check (§7.1) is not
/// persisted anywhere. A probe that cannot reach/parse the endpoint reports
/// `Unverified` (never a false "verified").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelIdentity {
    /// Endpoint reachable AND echoed a `model` field equal to the pin (§7.1).
    EndpointEchoed,
    /// Endpoint reachable but returned no / empty `model` field — the pin holds
    /// on the operator's word (`RMAP_SEED_MODEL_ID`), unverified.
    OperatorAsserted,
    /// Endpoint reachable but echoed a DIFFERENT model (a pin/config error).
    Mismatch { got: String },
    /// Could not verify (endpoint unreachable / non-loopback / malformed response).
    Unverified { reason: String },
}

impl ModelIdentity {
    /// Doctor-facing provenance label (spec §9). NEVER prints a bare
    /// "operator-asserted" as if it were verified — an unverifiable probe says so.
    pub fn label(&self) -> String {
        match self {
            ModelIdentity::EndpointEchoed => "endpoint-echoed".to_string(),
            ModelIdentity::OperatorAsserted => {
                "operator-asserted (endpoint returned no model echo)".to_string()
            }
            ModelIdentity::Mismatch { got } => {
                format!("MISMATCH — endpoint reports model {got:?}, not the pin")
            }
            ModelIdentity::Unverified { reason } => format!("unverified ({reason})"),
        }
    }
}

impl EndpointEmbedder {
    /// Probe the endpoint ONCE to determine the pinned model's identity provenance
    /// (spec §9). A single-token embed whose ONLY output we read is the response's
    /// echoed `model` field; any transport/parse failure ⇒ `Unverified` — the
    /// doctor thus never claims a verified identity it did not observe.
    pub fn probe_model_identity(&self) -> ModelIdentity {
        let body = serde_json::json!({ "model": self.model_id, "input": ["probe"] }).to_string();
        let raw = match self.post(&body) {
            Ok(r) => r,
            Err(e) => {
                return ModelIdentity::Unverified {
                    reason: e.to_string(),
                }
            }
        };
        let json_body = match extract_http_body(&raw) {
            Ok(b) => b,
            Err(e) => {
                return ModelIdentity::Unverified {
                    reason: e.to_string(),
                }
            }
        };
        let resp: EmbeddingsResponse = match serde_json::from_slice(json_body) {
            Ok(r) => r,
            Err(e) => {
                return ModelIdentity::Unverified {
                    reason: format!("response JSON: {e}"),
                }
            }
        };
        match resp.model.as_deref() {
            Some(m) if !m.is_empty() && m == self.model_id => ModelIdentity::EndpointEchoed,
            Some(m) if !m.is_empty() => ModelIdentity::Mismatch { got: m.to_string() },
            _ => ModelIdentity::OperatorAsserted,
        }
    }
}

impl Embedder for EndpointEmbedder {
    fn model_id(&self) -> &str {
        &self.model_id
    }
    fn dim(&self) -> usize {
        self.dim
    }
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let body = serde_json::json!({ "model": self.model_id, "input": texts }).to_string();
        let raw = self.post(&body)?;
        parse_embeddings(&raw, texts.len(), self.dim, &self.model_id)
    }
}

impl EndpointEmbedder {
    /// Issue one framed HTTP/1.1 POST and return the RAW response bytes (headers +
    /// body). Every transport-level deviation maps to an honest `EmbedError`.
    fn post(&self, json_body: &str) -> Result<Vec<u8>, EmbedError> {
        let addr = SocketAddr::new(self.ip, self.port);
        let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).map_err(|e| {
            EmbedError::Unreachable {
                endpoint: self.host_header.clone(),
                detail: e.to_string(),
            }
        })?;
        stream
            .set_read_timeout(Some(READ_TIMEOUT))
            .and_then(|_| stream.set_write_timeout(Some(READ_TIMEOUT)))
            .map_err(|e| EmbedError::Unreachable {
                endpoint: self.host_header.clone(),
                detail: e.to_string(),
            })?;

        let request = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\nAccept: application/json\r\n\r\n{}",
            self.path,
            self.host_header,
            json_body.len(),
            json_body
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|e| EmbedError::Unreachable {
                endpoint: self.host_header.clone(),
                detail: e.to_string(),
            })?;

        // Read the whole (close-delimited) response, bounded by the caps.
        let cap = MAX_HEADER_BYTES + MAX_BODY_BYTES + 1024;
        let mut buf: Vec<u8> = Vec::with_capacity(8192);
        let mut chunk = [0u8; 8192];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => break, // server closed (Connection: close)
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    if buf.len() > cap {
                        return Err(EmbedError::Malformed {
                            detail: "response exceeds size cap".to_string(),
                        });
                    }
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    return Err(EmbedError::Unreachable {
                        endpoint: self.host_header.clone(),
                        detail: "read timed out".to_string(),
                    });
                }
                Err(e) => {
                    return Err(EmbedError::Unreachable {
                        endpoint: self.host_header.clone(),
                        detail: e.to_string(),
                    });
                }
            }
        }
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_loopback_ip_literals() {
        for e in [
            "http://127.0.0.1:1234/v1/embeddings",
            "http://127.0.0.2:80/x",
            "http://[::1]:1234/v1/embeddings",
        ] {
            assert!(parse_loopback_http(e).is_ok(), "should accept {e}");
        }
    }

    #[test]
    fn rejects_localhost_dns_and_public_and_non_http() {
        for e in [
            "http://localhost:1234/v1/embeddings", // a NAME — no resolution
            "http://example.com/x",
            "http://8.8.8.8:1234/x",    // public IP
            "https://127.0.0.1:1234/x", // https not offered under a2
            "ftp://127.0.0.1/x",
        ] {
            assert!(
                matches!(
                    parse_loopback_http(e),
                    Err(EmbedError::NonLoopbackRejected { .. })
                ),
                "should reject {e}"
            );
        }
    }
}
