//! Java HTTP CONSUMER detection (review-0 item 1): `RestTemplate` / `WebClient`
//! / `HttpClient` call sites, parsed from `.java` source.
//!
//! The consumer side of Spring: where a Java module CALLS another module's HTTP
//! route. Unlike the Spring PROVIDER path (annotations on graph nodes), consumer
//! calls carry the URL as a call argument, so this re-parses the `.java` source.
//!
//! Scope (honest, discovery-not-proof — mirrors `grpc_link`):
//! - **RestTemplate**: the distinctive verb methods (`getForObject`,
//!   `getForEntity`, `postForObject`, `postForEntity`, `postForLocation`,
//!   `patchForObject`) → verb by name; `exchange(url, HttpMethod.X, …)` → verb
//!   from the `HttpMethod` argument, accepted only when the receiver reads like a
//!   template; generic `put`/`delete` only when the receiver name looks like a
//!   template (avoids matching `map.put` / `queue.exchange`).
//! - **WebClient**: `webClient.get().uri("…")` → verb from the ARG-FREE builder
//!   call preceding `.uri`.
//! - **HttpClient** (`java.net.http`): `HttpRequest.newBuilder().uri(URI.create("…"))`
//!   → route captured, verb UNKNOWN (the builder does not name the verb at the
//!   `.uri` site). Evidenced by `HttpRequest`/`newBuilder` in the receiver chain.
//!
//! Every `.exchange(...)` / `.uri(...)` case demands receiver/builder evidence —
//! a bare method-name match is never an HTTP consumer (review-4 item 1).
//!
//! Evidence (review-5 item 1 — STANDING HONESTY RULE 2): each framework path
//! additionally requires FILE-LEVEL IMPORT EVIDENCE. `getForObject` and friends
//! fire only in a file that imports `org.springframework.web.client`
//! (RestTemplate); the WebClient path only with
//! `org.springframework.web.reactive.function.client`; the HttpClient path only
//! with `java.net.http`. A distinctive method name alone is never enough.
//!
//! Honesty: a URL that is a variable / concatenation / non-literal yields
//! `route: None` (UNKNOWN) — never a fabricated path. A verb that cannot be read
//! is `"UNKNOWN"`, never guessed.

use repo_graph_boundary_interaction::{Direction, InteractionBasis};
use repo_graph_indexer::orchestrator::FileInput;
use tree_sitter::{Node, Parser};

use super::{file_symbol_key, node_text, route_from_raw, HttpSurfaceDraft};

/// Per-file Java HTTP-client IMPORT evidence (review-5 item 1). Each flag gates
/// its framework's call classification; without the import, the distinctive
/// method/receiver shapes are not trusted as HTTP facts.
#[derive(Debug, Clone, Copy)]
struct JavaClientImports {
    /// File imports `org.springframework.web.client` (RestTemplate /
    /// RestTemplateBuilder).
    rest_template: bool,
    /// File imports `org.springframework.web.reactive.function.client`
    /// (WebClient).
    web_client: bool,
    /// File imports `java.net.http` (HttpClient / HttpRequest).
    http_client: bool,
}

impl JavaClientImports {
    fn of(content: &str) -> Self {
        // review-6 item 1: evidence is an actual `import` DECLARATION for the
        // package, not the package string appearing anywhere (a comment or string
        // literal must not qualify) — `imports::java_imports_package` enforces it.
        use super::imports::java_imports_package;
        JavaClientImports {
            rest_template: java_imports_package(content, "org.springframework.web.client"),
            web_client: java_imports_package(
                content,
                "org.springframework.web.reactive.function.client",
            ),
            http_client: java_imports_package(content, "java.net.http"),
        }
    }

    /// Whether any HTTP client is imported — the cheap pre-filter that gates
    /// re-parsing.
    fn any(&self) -> bool {
        self.rest_template || self.web_client || self.http_client
    }
}

/// Detect Java HTTP client consumers across the repo's `.java` files.
pub(super) fn detect_java_http_consumers(file_inputs: &[FileInput]) -> Vec<HttpSurfaceDraft> {
    let mut drafts = Vec::new();
    let mut parser = Parser::new();
    let java_language: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
    if parser.set_language(&java_language).is_err() {
        return drafts;
    }

    for file in file_inputs {
        if !file.rel_path.ends_with(".java") {
            continue;
        }
        let imports = JavaClientImports::of(&file.content);
        if !imports.any() {
            continue;
        }
        let tree = match parser.parse(&file.content, None) {
            Some(t) => t,
            None => continue,
        };
        if crate::walk::tree_exceeds_depth(&tree.root_node(), crate::walk::MAX_POSTPASS_TREE_DEPTH)
        {
            continue;
        }
        collect_java_consumers(
            &tree.root_node(),
            file.content.as_bytes(),
            &file.rel_path,
            &imports,
            &mut drafts,
        );
    }
    drafts
}

fn collect_java_consumers(
    root: &Node,
    source: &[u8],
    file: &str,
    imports: &JavaClientImports,
    out: &mut Vec<HttpSurfaceDraft>,
) {
    let mut stack = vec![*root];
    while let Some(node) = stack.pop() {
        if node.kind() == "method_invocation" {
            if let Some((verb, route, framework)) = classify_java_invocation(&node, source, imports)
            {
                let anchor = node.child_by_field_name("name").unwrap_or(node);
                out.push(HttpSurfaceDraft {
                    direction: Direction::Consumer,
                    http_method: verb,
                    route,
                    source_file: file.to_string(),
                    line_start: anchor.start_position().row as i64 + 1,
                    col_start: anchor.start_position().column as i64,
                    symbol_stable_key: file_symbol_key(file),
                    basis: InteractionBasis::ApiCall,
                    framework,
                });
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

/// Classify a `method_invocation` as an HTTP client call, returning
/// `(verb, route, framework)`. `None` when it is not a recognized client call.
///
/// Every arm additionally requires the file's IMPORT evidence for that framework
/// (review-5 item 1): the RestTemplate arms need `imports.rest_template`, the
/// WebClient/HttpClient `uri` arm needs `imports.web_client` / `http_client`.
fn classify_java_invocation(
    mi: &Node,
    source: &[u8],
    imports: &JavaClientImports,
) -> Option<(String, Option<String>, &'static str)> {
    let name = mi
        .child_by_field_name("name")
        .and_then(|n| node_text(&n, source))?;
    let args = mi.child_by_field_name("arguments");

    match name.as_str() {
        "getForObject" | "getForEntity" if imports.rest_template => Some((
            "GET".into(),
            route_from_java_arg(args, 0, source),
            "resttemplate",
        )),
        "postForObject" | "postForEntity" | "postForLocation" if imports.rest_template => Some((
            "POST".into(),
            route_from_java_arg(args, 0, source),
            "resttemplate",
        )),
        "patchForObject" if imports.rest_template => Some((
            "PATCH".into(),
            route_from_java_arg(args, 0, source),
            "resttemplate",
        )),
        // `exchange` is also a Map/queue/collection method name. Only accept it
        // as `RestTemplate.exchange(url, HttpMethod.X, …)` when the file imports
        // RestTemplate AND the RECEIVER reads like one — never on the bare method
        // name (review-4 item 1; review-5 item 1; STANDING HONESTY RULE 2).
        "exchange" if imports.rest_template && receiver_is_rest_template(mi, source) => {
            let verb = nth_java_arg(args, 1)
                .and_then(|a| http_method_verb(&a, source))
                .unwrap_or_else(|| "UNKNOWN".to_string());
            Some((verb, route_from_java_arg(args, 0, source), "resttemplate"))
        }
        // Generic verbs collide with Map/collection APIs — only accept them with
        // a RestTemplate import AND a template-shaped receiver (no fabrication).
        "put" | "delete" if imports.rest_template && receiver_is_rest_template(mi, source) => {
            Some((
                name.to_uppercase(),
                route_from_java_arg(args, 0, source),
                "resttemplate",
            ))
        }
        // WebClient `.get().uri("…")` / HttpClient `newBuilder().uri(URI.create("…"))`.
        // `uri` is a common method name (URI, builders, config objects), so this
        // requires the framework import AND receiver/builder evidence, yielding
        // `None` otherwise — never a bare-`.uri(` HTTP consumer.
        "uri" => {
            let (verb, framework) = classify_uri_call(mi, source, imports)?;
            Some((verb, route_from_java_arg(args, 0, source), framework))
        }
        _ => None,
    }
}

/// Whether the receiver identifier of a call looks like a `RestTemplate`.
fn receiver_is_rest_template(mi: &Node, source: &[u8]) -> bool {
    let object = match mi.child_by_field_name("object") {
        Some(o) => o,
        None => return false,
    };
    // review-6 item 2: an unreadable receiver is UNKNOWN, not "" — we cannot
    // confirm it is a RestTemplate, so it is not classified (no fabricated fact).
    let text = match node_text(&object, source) {
        Some(t) => t.to_lowercase(),
        None => return false,
    };
    text.contains("template")
}

/// Classify a `.uri(…)` call as an evidenced HTTP client builder, returning
/// `(verb, framework)`, or `None` when there is no client evidence.
///
/// Two evidenced forms (review-4 item 1 — no bare-`.uri(` acceptance):
/// - **WebClient**: `webClient.get().uri("…")` — the verb rides the ARG-FREE
///   builder call immediately preceding `.uri`. The empty argument list
///   distinguishes the WebClient verb builder from an indexed `list.get(0).uri(…)`.
/// - **HttpClient** (`java.net.http`): `HttpRequest.newBuilder()…uri(URI.create("…"))`
///   — evidenced by `HttpRequest`/`newBuilder` in the receiver chain; the builder
///   does not name the verb at the `.uri` site, so its verb is UNKNOWN.
///
/// Anything else (`config.uri(…)`, a bare receiver, an unrelated builder) is NOT
/// an HTTP consumer.
fn classify_uri_call(
    mi: &Node,
    source: &[u8],
    imports: &JavaClientImports,
) -> Option<(String, &'static str)> {
    let object = mi.child_by_field_name("object")?;

    // WebClient: verb from the arg-free builder call preceding `.uri`, gated on
    // the WebClient import.
    if imports.web_client && object.kind() == "method_invocation" {
        if let Some(inner) = object
            .child_by_field_name("name")
            .and_then(|n| node_text(&n, source))
        {
            let lower = inner.to_lowercase();
            let builder_is_arg_free =
                nth_java_arg(object.child_by_field_name("arguments"), 0).is_none();
            if builder_is_arg_free
                && matches!(lower.as_str(), "get" | "post" | "put" | "delete" | "patch")
            {
                return Some((lower.to_uppercase(), "webclient"));
            }
        }
    }

    // HttpClient: require the `java.net.http` import AND builder evidence in the
    // receiver chain.
    if imports.http_client {
        // review-6 item 2: an unreadable receiver chain is UNKNOWN, not "" — fall
        // through to `None` (unclassified) rather than a fabricated match.
        if let Some(object_text) = node_text(&object, source) {
            if object_text.contains("newBuilder") || object_text.contains("HttpRequest") {
                return Some(("UNKNOWN".to_string(), "httpclient"));
            }
        }
    }

    None
}

/// Map an `HttpMethod.GET` / `GET` argument to an HTTP verb.
fn http_method_verb(arg: &Node, source: &[u8]) -> Option<String> {
    let text = node_text(arg, source)?;
    let last = text.rsplit('.').next().unwrap_or(&text).trim();
    match last.to_uppercase().as_str() {
        v @ ("GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS") => {
            Some(v.to_string())
        }
        _ => None,
    }
}

/// Route from the nth argument of a Java call: a string literal (or a
/// `URI.create("…")` wrapper) yields its route; anything else → `None`.
fn route_from_java_arg(args: Option<Node>, n: usize, source: &[u8]) -> Option<String> {
    let arg = nth_java_arg(args, n)?;
    java_route_from_node(&arg, source)
}

fn java_route_from_node(arg: &Node, source: &[u8]) -> Option<String> {
    match arg.kind() {
        "string_literal" => {
            let text = node_text(arg, source)?;
            let inner = strip_java_string(&text);
            // `route_from_raw` owns absolute-URL → path reduction for BOTH the
            // TS and Java sides (single normalization point, no drift).
            route_from_raw(inner)
        }
        // `URI.create("…")` — unwrap to the inner string literal.
        "method_invocation" => {
            let name = arg
                .child_by_field_name("name")
                .and_then(|n| node_text(&n, source))?;
            if name != "create" {
                return None;
            }
            let inner_args = arg.child_by_field_name("arguments");
            let first = nth_java_arg(inner_args, 0)?;
            java_route_from_node(&first, source)
        }
        _ => None,
    }
}

/// Strip the surrounding double quotes off a Java string literal token.
fn strip_java_string(text: &str) -> &str {
    text.trim_matches('"')
}

/// The nth non-punctuation argument of a Java `argument_list`.
fn nth_java_arg<'a>(args: Option<Node<'a>>, n: usize) -> Option<Node<'a>> {
    let args = args?;
    let mut cursor = args.walk();
    let mut idx = 0;
    for child in args.children(&mut cursor) {
        let k = child.kind();
        if k == "(" || k == ")" || k == "," {
            continue;
        }
        if idx == n {
            return Some(child);
        }
        idx += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_of(src: &str) -> FileInput {
        FileInput {
            rel_path: "backend/src/ClientGateway.java".to_string(),
            content: src.to_string(),
            content_hash: String::new(),
            size_bytes: src.len(),
            line_count: src.lines().count(),
            package_dependencies: None,
            tsconfig_aliases: None,
        }
    }

    /// Parse `src` in a file carrying all three Java HTTP-client IMPORTS, so the
    /// per-call classifier — not the file gate — is what each test exercises
    /// (review-5 item 1). Import-gate negatives use `java_drafts_raw`.
    fn java_drafts(src: &str) -> Vec<HttpSurfaceDraft> {
        let with_imports = format!(
            "import org.springframework.web.client.RestTemplate;\n\
             import org.springframework.web.reactive.function.client.WebClient;\n\
             import java.net.http.HttpClient;\n{src}"
        );
        detect_java_http_consumers(&[file_of(&with_imports)])
    }

    /// Parse `src` verbatim — no injected imports.
    fn java_drafts_raw(src: &str) -> Vec<HttpSurfaceDraft> {
        detect_java_http_consumers(&[file_of(src)])
    }

    #[test]
    fn resttemplate_get_for_object_static_route() {
        let drafts = java_drafts(
            r#"class G { void f() { restTemplate.getForObject("/api/v2/clients/{id}", Client.class, id); } }"#,
        );
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].direction, Direction::Consumer);
        assert_eq!(drafts[0].http_method, "GET");
        assert_eq!(drafts[0].route.as_deref(), Some("/api/v2/clients/{id}"));
        assert_eq!(drafts[0].framework, "resttemplate");
    }

    #[test]
    fn resttemplate_absolute_url_reduced_to_path() {
        let drafts = java_drafts(
            r#"class G { void f() { restTemplate.postForEntity("https://svc.internal/api/v2/orders?x=1", body, Order.class); } }"#,
        );
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].http_method, "POST");
        assert_eq!(drafts[0].route.as_deref(), Some("/api/v2/orders"));
    }

    #[test]
    fn resttemplate_exchange_reads_verb_from_httpmethod() {
        let drafts = java_drafts(
            r#"class G { void f() { restTemplate.exchange("/api/v2/clients", HttpMethod.DELETE, entity, Void.class); } }"#,
        );
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].http_method, "DELETE");
        assert_eq!(drafts[0].route.as_deref(), Some("/api/v2/clients"));
    }

    #[test]
    fn resttemplate_dynamic_url_is_unknown_route_not_fabricated() {
        let drafts = java_drafts(
            r#"class G { void f() { restTemplate.getForObject(baseUrl + "/clients", Client.class); } }"#,
        );
        assert_eq!(drafts.len(), 1, "dynamic-URL consumer still surfaces");
        assert_eq!(drafts[0].http_method, "GET");
        assert_eq!(drafts[0].route, None, "concatenated URL -> unknown route");
    }

    #[test]
    fn webclient_uri_reads_verb_from_builder() {
        let drafts = java_drafts(
            r#"class G { void f() { webClient.get().uri("/api/v2/offers").retrieve(); } }"#,
        );
        // WebClient chain: only the `.uri` call is the consumer surface.
        let uris: Vec<_> = drafts
            .iter()
            .filter(|d| d.framework == "webclient")
            .collect();
        assert_eq!(uris.len(), 1);
        assert_eq!(uris[0].http_method, "GET");
        assert_eq!(uris[0].route.as_deref(), Some("/api/v2/offers"));
    }

    #[test]
    fn httpclient_uri_create_captures_route_verb_unknown() {
        let drafts = java_drafts(
            r#"class G { void f() { HttpRequest.newBuilder().uri(URI.create("https://svc/api/v2/ping")).build(); } }"#,
        );
        let uris: Vec<_> = drafts
            .iter()
            .filter(|d| d.framework == "httpclient")
            .collect();
        assert_eq!(uris.len(), 1);
        assert_eq!(uris[0].http_method, "UNKNOWN");
        assert_eq!(uris[0].route.as_deref(), Some("/api/v2/ping"));
    }

    #[test]
    fn map_put_is_not_an_http_consumer() {
        // `cache.put(...)` must NOT be treated as a RestTemplate call.
        let drafts = java_drafts(r#"class G { void f() { cache.put("key", value); } }"#);
        assert!(
            drafts.is_empty(),
            "map.put must not be an http consumer: {:?}",
            drafts
        );
    }

    #[test]
    fn unrelated_exchange_is_not_http_consumer() {
        // A client type token in the file admits it past the prefilter, but an
        // unrelated `.exchange(...)` on a non-template receiver must NOT be an
        // HTTP consumer (review-4 item 1 — no name-only classification).
        let drafts = java_drafts(
            r#"class G { RestTemplate rt; void f() { queue.exchange(from, to, amount); } }"#,
        );
        assert!(
            drafts.is_empty(),
            "unrelated exchange must not be an http consumer: {:?}",
            drafts
        );
    }

    #[test]
    fn unrelated_uri_on_bare_receiver_is_not_http_consumer() {
        // `config.uri("…")` on a plain receiver is not an HTTP client call.
        let drafts = java_drafts(
            r#"class G { WebClient webClient; void f() { config.uri("/not/a/request"); } }"#,
        );
        assert!(
            drafts.is_empty(),
            "bare .uri on a non-builder receiver must not be an http consumer: {:?}",
            drafts
        );
    }

    #[test]
    fn indexed_get_uri_chain_is_not_webclient() {
        // `items.get(0).uri("…")` — the preceding `.get(0)` carries an argument,
        // so it is NOT the arg-free WebClient verb builder. Must not be classified.
        let drafts =
            java_drafts(r#"class G { WebClient webClient; void f() { items.get(0).uri("/x"); } }"#);
        assert!(
            drafts.is_empty(),
            "indexed get().uri() must not be webclient: {:?}",
            drafts
        );
    }

    #[test]
    fn resttemplate_put_with_template_receiver_links() {
        let drafts = java_drafts(
            r#"class G { void f() { restTemplate.put("/api/v2/clients/{id}", request); } }"#,
        );
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].http_method, "PUT");
        assert_eq!(drafts[0].route.as_deref(), Some("/api/v2/clients/{id}"));
    }

    /// review-5 item 1: a `restTemplate.getForObject(...)` call in a file that
    /// does NOT import RestTemplate is NOT an HTTP consumer — a distinctive
    /// method name alone is not trustworthy evidence (STANDING HONESTY RULE 2).
    #[test]
    fn resttemplate_call_without_import_is_not_consumer() {
        let drafts = java_drafts_raw(
            r#"class G { void f() { restTemplate.getForObject("/api/v2/clients", Client.class); } }"#,
        );
        assert!(
            drafts.is_empty(),
            "no RestTemplate import → not an http consumer: {:?}",
            drafts
        );
    }

    /// review-5 item 1: a WebClient-shaped `.get().uri(...)` chain in a file that
    /// imports RestTemplate but NOT WebClient is not classified as WebClient
    /// (import scoping is per framework).
    #[test]
    fn webclient_chain_without_webclient_import_is_not_consumer() {
        let src = "import org.springframework.web.client.RestTemplate;\n\
                   class G { void f() { webClient.get().uri(\"/api/v2/offers\").retrieve(); } }";
        let drafts = detect_java_http_consumers(&[file_of(src)]);
        assert!(
            drafts.iter().all(|d| d.framework != "webclient"),
            "no WebClient import → no webclient surface: {:?}",
            drafts
        );
    }
}
