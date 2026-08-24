//! Boundaries links command.
//!
//! Provider↔consumer link discovery (the inter-module API map).
//!
//! # REG-1 Contract
//!
//! Resolves repo from cwd via daemon registry.
//! No explicit db_path or repo_uid arguments.
//!
//! # HTTP-BOUNDARY-1
//!
//! Human rendering added so the linked API map renders (e.g.
//! `GET /api/v2/clients  frontend/api.ts → backend/ClientController.java`).
//! `--json` preserves the raw envelope.

use std::process::ExitCode;

use crate::daemon_client::DaemonClient;

pub(super) fn run_boundaries_links(args: &[String]) -> ExitCode {
    // Parse filters
    let (service, as_json) = match parse_links_filters(args) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            eprintln!("usage: rmap boundaries links [--service <name>] [--json]");
            return ExitCode::from(1);
        }
    };

    // Get cwd for repo resolution
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot determine current directory: {}", e);
            return ExitCode::from(2);
        }
    };

    let repo_path = match cwd.canonicalize() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(e) => {
            eprintln!("error: cannot canonicalize current directory: {}", e);
            return ExitCode::from(2);
        }
    };

    // Connect to daemon
    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Build request params
    let mut params = serde_json::json!({ "repo": repo_path });
    if let Some(s) = service {
        params["service"] = serde_json::json!(s);
    }

    match client.request("boundaries_links", Some(params)) {
        Ok(result) => {
            if as_json {
                match serde_json::to_string_pretty(&result) {
                    Ok(json) => {
                        println!("{}", json);
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to serialize result: {}", e);
                        ExitCode::from(2)
                    }
                }
            } else {
                print!("{}", render_links_human(&result));
                ExitCode::SUCCESS
            }
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

// ── Human rendering (HTTP-BOUNDARY-1) ─────────────────────────────────────────

/// Render the linked provider↔consumer API map. Each link is shown as the
/// concrete API edge (method + route for HTTP, contract name for gRPC), then
/// the consumer→provider files. Deterministic: results arrive pre-sorted from
/// storage.
fn render_links_human(envelope: &serde_json::Value) -> String {
    let mut out = String::new();
    out.push_str("Boundary Links\n\n");

    // review-5 item 3: an ABSENT or non-array `results` is malformed daemon data,
    // NOT "0 links". A missing read must render as unknown/degraded, never as a
    // zero fact (STANDING HONESTY RULE 1). An empty ARRAY is a legitimate 0.
    let results = match envelope.get("results") {
        Some(serde_json::Value::Array(a)) => a.clone(),
        Some(_) => {
            out.push_str(
                "links: unknown — daemon 'results' field was not a list (malformed response; \
                 rerun after reindex).\n",
            );
            out.push_str(&render_http_unlinked(envelope));
            return out;
        }
        None => {
            out.push_str(
                "links: unknown — daemon response omitted 'results' (malformed response; \
                 rerun after reindex).\n",
            );
            out.push_str(&render_http_unlinked(envelope));
            return out;
        }
    };

    if results.is_empty() {
        out.push_str("0 links\n\n");
        out.push_str("hint: links connect a consumer surface to a provider surface by shared\n");
        out.push_str("      contract (gRPC) or matching route + method (HTTP). None were found.\n");
        out.push_str(&render_http_unlinked(envelope));
        return out;
    }

    out.push_str(&format!("{} links\n\n", results.len()));
    for link in &results {
        let label = link_edge_label(link);
        let kind = link.get("linkKind").and_then(|k| k.as_str()).unwrap_or("-");
        let confidence = link
            .get("confidence")
            .and_then(|c| c.as_f64())
            .unwrap_or(0.0);
        let consumer = file_line(link, "consumerFile", "consumerLine");
        let provider = file_line(link, "providerFile", "providerLine");
        out.push_str(&format!("  {}\n", label));
        out.push_str(&format!(
            "    {} → {}   [{}, confidence {:.2}]\n",
            consumer, provider, kind, confidence
        ));
    }
    out.push_str(&render_http_unlinked(envelope));
    out
}

/// HTTP-BOUNDARY-1 (review-0 item 4): render the honest per-reason count of HTTP
/// consumer surfaces left UNLINKED. Ambiguous (route matched >1 provider) and
/// unmatched (matched none) and dynamic-route (unreadable URL) consumers are
/// real surfaces the map shows unlinked WITH the reason — never guessed into a
/// link (VISION: unknown is never fabricated). Absent for non-HTTP repos.
fn render_http_unlinked(envelope: &serde_json::Value) -> String {
    // A failed HTTP read is UNKNOWN, never a silent (absent) footer or "0
    // unlinked" (review-4 item 2): render the reader-framed degradation.
    if let Some(reason) = envelope
        .get("httpUnlinkedDegraded")
        .and_then(|v| v.as_str())
    {
        return format!(
            "\nHTTP consumers: unknown — {} (not reporting 0; rerun after reindex).\n",
            reason
        );
    }
    let u = match envelope.get("httpUnlinked") {
        Some(u) if u.is_object() => u,
        _ => return String::new(),
    };
    // review-5 item 3: every counter is written together by the daemon. A MISSING
    // counter is malformed data, not 0 — `unwrap_or(0)` would fabricate a zero
    // fact. Read each as an honest Option; a single absent key degrades the whole
    // block to unknown rather than reporting silent zeros.
    let get = |k: &str| -> Option<u64> { u.get(k).and_then(|v| v.as_u64()) };
    let fields = [
        get("consumers"),
        get("linked"),
        get("ambiguous"),
        get("unmatched"),
        get("dynamicRoute"),
    ];
    if fields.iter().any(|f| f.is_none()) {
        return "\nHTTP consumers: unknown — the daemon HTTP-unlinked counts were incomplete \
                (malformed response; rerun after reindex).\n"
            .to_string();
    }
    // Safe: the `any(is_none)` guard above returned early on any absent counter.
    let [consumers, linked, ambiguous, unmatched, dynamic] =
        fields.map(|f| f.expect("all counters present past the degradation guard"));
    if consumers == 0 {
        return String::new();
    }
    let mut out = String::new();
    out.push_str(&format!(
        "\nHTTP consumers: {} total, {} linked to a provider.\n",
        consumers, linked
    ));
    if ambiguous + unmatched + dynamic > 0 {
        out.push_str(&format!(
            "  unlinked: {} ambiguous (route matched >1 provider), {} unmatched (no provider), \
             {} dynamic (URL not statically readable).\n",
            ambiguous, unmatched, dynamic
        ));
        out.push_str(
            "  note: unlinked consumers are shown honestly — a route is linked only on an \
             unambiguous (method + template) match, never on module adjacency.\n",
        );
    }
    out
}

/// The API edge label: `METHOD /route` for HTTP links (from evidenceJson),
/// else the gRPC contract name, else the link kind.
fn link_edge_label(link: &serde_json::Value) -> String {
    if let Some(ev) = link.get("evidenceJson").and_then(|e| e.as_str()) {
        if let Ok(ev) = serde_json::from_str::<serde_json::Value>(ev) {
            let method = ev.get("httpMethod").and_then(|m| m.as_str());
            let route = ev
                .get("providerRoute")
                .and_then(|r| r.as_str())
                .or_else(|| ev.get("consumerRoute").and_then(|r| r.as_str()));
            if let (Some(m), Some(r)) = (method, route) {
                return format!("{} {}", m, r);
            }
        }
    }
    if let Some(name) = link.get("contractName").and_then(|c| c.as_str()) {
        return name.to_string();
    }
    link.get("linkKind")
        .and_then(|k| k.as_str())
        .unwrap_or("link")
        .to_string()
}

fn file_line(link: &serde_json::Value, file_key: &str, line_key: &str) -> String {
    let file = link.get(file_key).and_then(|f| f.as_str()).unwrap_or("?");
    let line = link.get(line_key).and_then(|l| l.as_u64()).unwrap_or(0);
    if line > 0 {
        format!("{}:{}", file, line)
    } else {
        file.to_string()
    }
}

// ── Argument parsing ─────────────────────────────────────────────────────────

fn parse_links_filters(args: &[String]) -> Result<(Option<String>, bool), String> {
    let mut service = None;
    let mut as_json = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--service" => {
                if i + 1 >= args.len() {
                    return Err("--service requires a value".to_string());
                }
                service = Some(args[i + 1].clone());
                i += 2;
            }
            "--json" => {
                as_json = true;
                i += 1;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown option: {}", other));
            }
            other => {
                return Err(format!("unexpected argument: {}", other));
            }
        }
    }

    Ok((service, as_json))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_http_edge_from_evidence() {
        let envelope = serde_json::json!({
            "results": [{
                "linkKind": "http_route_match",
                "confidence": 0.75,
                "providerFile": "backend/ClientController.java",
                "providerLine": 20,
                "consumerFile": "frontend/api/client.ts",
                "consumerLine": 132,
                "evidenceJson": "{\"httpMethod\":\"GET\",\"providerRoute\":\"/api/v2/clients/{id}\",\"consumerRoute\":\"/api/v2/clients/{param}\"}"
            }]
        });
        let out = render_links_human(&envelope);
        assert!(
            out.contains("GET /api/v2/clients/{id}"),
            "edge label: {}",
            out
        );
        assert!(out.contains("frontend/api/client.ts:132 → backend/ClientController.java:20"));
        assert!(out.contains("http_route_match"));
    }

    #[test]
    fn renders_empty_hint() {
        let out = render_links_human(&serde_json::json!({ "results": [] }));
        assert!(out.contains("0 links"));
    }

    /// review-0 item 4: unlinked HTTP consumers render WITH their reason, even
    /// when zero links were emitted (the glamCRM ambiguous case).
    #[test]
    fn renders_http_unlinked_reasons_with_zero_links() {
        let envelope = serde_json::json!({
            "results": [],
            "httpUnlinked": {
                "providers": 221, "consumers": 13, "linked": 1,
                "ambiguous": 4, "unmatched": 6, "dynamicRoute": 2
            }
        });
        let out = render_links_human(&envelope);
        assert!(out.contains("HTTP consumers: 13 total, 1 linked"), "{out}");
        assert!(out.contains("4 ambiguous"), "{out}");
        assert!(out.contains("6 unmatched"), "{out}");
        assert!(out.contains("2 dynamic"), "{out}");
    }

    /// A non-HTTP repo (no `httpUnlinked` block) renders no HTTP footer.
    #[test]
    fn no_http_footer_without_http_block() {
        let out = render_links_human(&serde_json::json!({ "results": [] }));
        assert!(!out.contains("HTTP consumers"), "{out}");
    }

    /// review-5 item 3: an ABSENT `results` field is malformed daemon data, not
    /// "0 links" — it must render as unknown, never as a zero fact.
    #[test]
    fn absent_results_renders_unknown_not_zero_links() {
        let out = render_links_human(&serde_json::json!({}));
        assert!(
            out.contains("links: unknown") && out.contains("omitted 'results'"),
            "absent results must be unknown, not 0 links:\n{out}"
        );
        assert!(
            !out.contains("0 links"),
            "must not fabricate a 0-links count:\n{out}"
        );
    }

    /// review-5 item 3: a non-array `results` is malformed → unknown.
    #[test]
    fn non_array_results_renders_unknown() {
        let out = render_links_human(&serde_json::json!({ "results": "oops" }));
        assert!(out.contains("links: unknown"), "{out}");
        assert!(!out.contains("0 links"), "{out}");
    }

    /// review-5 item 3: an httpUnlinked block MISSING a counter is incomplete
    /// data → unknown, never silent zeros.
    #[test]
    fn incomplete_http_unlinked_counters_render_unknown_not_zero() {
        let envelope = serde_json::json!({
            "results": [],
            "httpUnlinked": { "consumers": 13, "linked": 1 } // ambiguous/unmatched/dynamicRoute absent
        });
        let out = render_links_human(&envelope);
        assert!(
            out.contains("HTTP consumers: unknown") && out.contains("incomplete"),
            "incomplete counters must be unknown:\n{out}"
        );
        assert!(
            !out.contains("0 ambiguous"),
            "must not fabricate zero counters:\n{out}"
        );
    }

    /// review-4 item 2: a FAILED HTTP read renders as UNKNOWN, never as a silent
    /// footer or "0 unlinked".
    #[test]
    fn http_read_degraded_renders_unknown_not_silence() {
        let envelope = serde_json::json!({
            "results": [],
            "httpUnlinkedDegraded": "HTTP boundary link read failed (degraded): db locked"
        });
        let out = render_links_human(&envelope);
        assert!(out.contains("HTTP consumers: unknown"), "{out}");
        assert!(out.contains("db locked"), "{out}");
    }
}
