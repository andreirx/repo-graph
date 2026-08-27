//! TS/JS HTTP detection: CONSUMER calls (`axios` / `fetch` / api-client) and
//! the AWS-CDK apigatewayv2 serverless PROVIDER form glamCRM uses.
//!
//! Honesty (review-0 item 3): a consumer whose first argument is NOT a static
//! string/template literal (a bare variable, a concatenation, a call) still
//! emits a consumer surface, but with an UNKNOWN route (`route: None`) — never
//! a fabricated path, never silently dropped.
//!
//! Evidence (review-5 item 1 — STANDING HONESTY RULE 2, no name-only facts):
//! the axios/api-client consumer path and the CDK provider path each require
//! FILE-LEVEL IMPORT EVIDENCE, not a bare receiver/property name. A `.get(...)`
//! on a client-shaped receiver becomes an HTTP consumer only in a file that
//! imports `axios` or an `api-client` module; an `.addRoutes(...)` becomes a CDK
//! provider only in a file that imports `aws-...-apigatewayv2`. The receiver is
//! then corroborated structurally (exact `axios` / an `apiClient`-named
//! receiver). `fetch` is the browser/Node global (no import) matched by its
//! exact identifier — the one intentionally import-free path.

use std::ops::ControlFlow;

use repo_graph_boundary_interaction::{Direction, InteractionBasis};
use repo_graph_indexer::jsts_extensions::{
    get_extension, is_jsts_extension, is_jsts_jsx_extension,
};
use repo_graph_indexer::orchestrator::FileInput;
use tree_sitter::{Node, Parser};

use super::{extract_route_literal, file_symbol_key, node_text, HttpSurfaceDraft};

/// HTTP verbs recognized on the consumer side (axios verb methods / fetch).
const HTTP_VERBS: &[&str] = &["get", "post", "put", "delete", "patch", "head", "options"];

/// The recorded reason for a consumer whose URL is not statically readable
/// (review-0 item 4): `Some(reason)` when the route came back UNKNOWN, `None`
/// when it is a known static route. Kept in one place so every TS consumer site
/// records the same honest reason instead of a bare `None`.
fn dynamic_url_reason(route_is_unknown: bool) -> Option<&'static str> {
    if route_is_unknown {
        Some("dynamic URL — first argument is not a static string/template literal")
    } else {
        None
    }
}

/// Express receiver names — a consumer detector must NOT treat these as HTTP
/// clients (they are provider-side route registrations handled by
/// `express_detector`).
const EXPRESS_RECEIVERS: &[&str] = &["app", "router", "server"];

/// Per-file IMPORT evidence (review-5 item 1). A framework path fires only when
/// its evidence is present in the file; a bare receiver/property name is never
/// enough. `fetch` is deliberately absent here — it is a global, matched by its
/// exact identifier, not an import.
#[derive(Debug, Clone, Copy)]
struct TsHttpEvidence {
    /// review-8 #2: the file declares/imports its OWN binding named `fetch`
    /// (`const/let/var/function fetch`, or an import binding `fetch` from a
    /// module that is NOT a known fetch polyfill). The web-standard GLOBAL
    /// `fetch` needs no import — its evidence is the ABSENCE of a shadowing
    /// binding; when one exists the call's semantics are unknown → no surface.
    fetch_shadowed: bool,
    /// File imports `axios` or an `api-client` module — gates the axios /
    /// api-client CONSUMER path.
    axios_client: bool,
    /// File imports an `aws-...-apigatewayv2` module — gates the CDK PROVIDER
    /// path (`addRoutes`).
    cdk_apigw: bool,
}

impl TsHttpEvidence {
    fn of(content: &str) -> Self {
        // review-6 item 1: evidence is the MODULE SPECIFIER of a real
        // `import`/`require` declaration, ignoring comments and unrelated string
        // literals. `import axios from 'axios'` / `import { getApiClient } from
        // '../config/api-client'` / `import * as x from 'aws-cdk-lib/aws-apigatewayv2'`
        // are the forms glamCRM's consumers/infra use. A comment or a string
        // literal that merely contains these tokens no longer qualifies.
        let specs = super::imports::ts_import_specifiers(content);
        let axios_client = specs
            .iter()
            .any(|s| s.contains("axios") || s.contains("api-client"));
        let cdk_apigw = specs.iter().any(|s| s.contains("apigatewayv2"));
        TsHttpEvidence {
            axios_client,
            cdk_apigw,
            fetch_shadowed: fetch_is_shadowed(content),
        }
    }

    /// Whether any HTTP path could fire (import-gated), or `fetch(` is present
    /// (the global path). Cheap pre-filter so we only re-parse relevant files.
    fn file_may_have_http(&self, content: &str) -> bool {
        self.axios_client || self.cdk_apigw || content.contains("fetch(")
    }
}

/// Known fetch polyfills: an import binding named `fetch` from these keeps
/// web-fetch semantics (still an HTTP consumer); any other module makes the
/// binding's semantics unknown.
const FETCH_POLYFILL_SPECS: [&str; 5] = [
    "node-fetch",
    "cross-fetch",
    "undici",
    "whatwg-fetch",
    "isomorphic-fetch",
];

/// Line-anchored shadow scan (comments excluded): local `const/let/var/function
/// fetch` always shadows; `import fetch …` / `import { … fetch … } …` shadows
/// unless the specifier is a known fetch polyfill.
fn fetch_is_shadowed(content: &str) -> bool {
    for raw in content.lines() {
        let line = raw.trim_start();
        if line.starts_with("//") || line.starts_with('*') || line.starts_with("/*") {
            continue;
        }
        let after_decl = ["const ", "let ", "var ", "function ", "async function "]
            .iter()
            .find_map(|kw| line.strip_prefix(kw));
        if let Some(rest) = after_decl {
            let rest = rest.trim_start();
            if rest.starts_with("fetch")
                && !rest[5..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$')
            {
                return true;
            }
        }
        if let Some(rest) = line.strip_prefix("import ") {
            let binds_fetch = rest.split(" from ").next().is_some_and(|clause| {
                clause
                    .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
                    .any(|tok| tok == "fetch")
            });
            if binds_fetch && !FETCH_POLYFILL_SPECS.iter().any(|spec| rest.contains(spec)) {
                return true;
            }
        }
    }
    false
}

/// Detect HTTP consumers and CDK serverless providers in TS/JS files.
pub(super) fn detect_ts_http(file_inputs: &[FileInput]) -> Vec<HttpSurfaceDraft> {
    let mut drafts = Vec::new();
    let mut parser = Parser::new();
    let ts_language: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    let tsx_language: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TSX.into();

    for file in file_inputs {
        let ext = get_extension(&file.rel_path);
        if !is_jsts_extension(ext) {
            continue;
        }
        let evidence = TsHttpEvidence::of(&file.content);
        if !evidence.file_may_have_http(&file.content) {
            continue;
        }
        let language = if is_jsts_jsx_extension(ext) {
            &tsx_language
        } else {
            &ts_language
        };
        if parser.set_language(language).is_err() {
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
        collect_ts_http(
            &tree.root_node(),
            file.content.as_bytes(),
            &file.rel_path,
            &evidence,
            &mut drafts,
        );
    }
    drafts
}

fn collect_ts_http(
    root: &Node,
    source: &[u8],
    file: &str,
    evidence: &TsHttpEvidence,
    out: &mut Vec<HttpSurfaceDraft>,
) {
    crate::walk::visit_preorder(*root, |node| {
        if node.kind() == "call_expression" {
            if let Some(d) = try_extract_consumer(&node, source, file, evidence) {
                out.push(d);
            } else if let Some(d) = try_extract_fetch(&node, source, file, evidence) {
                out.push(d);
            } else {
                try_extract_cdk_routes(&node, source, file, evidence, out);
            }
        }
        ControlFlow::Continue(())
    });
}

/// Route from a call's first argument node: a static string/template literal
/// yields its route (which itself may be `None` for a dynamic template like
/// `${BASE}/x`); any other argument shape (bare variable, concatenation, call)
/// yields `None` — an honest UNKNOWN, never fabricated.
fn route_of_arg(arg: &Node, source: &[u8]) -> Option<String> {
    if arg.kind() == "string" || arg.kind() == "template_string" {
        extract_route_literal(arg, source)
    } else {
        None
    }
}

/// `axios.get('/x')` / `getApiClient().post('/x', body)`.
///
/// Requires FILE import evidence (`evidence.axios_client`) AND a structurally
/// corroborated receiver (exact `axios` or an `apiClient`-named factory/instance)
/// — never a bare verb-method name, never a loose `*client*` substring
/// (review-5 item 1).
fn try_extract_consumer(
    call: &Node,
    source: &[u8],
    file: &str,
    evidence: &TsHttpEvidence,
) -> Option<HttpSurfaceDraft> {
    if !evidence.axios_client {
        return None;
    }
    let callee = call.child_by_field_name("function")?;
    if callee.kind() != "member_expression" {
        return None;
    }
    let object = callee.child_by_field_name("object")?;
    let property = callee.child_by_field_name("property")?;
    let method = node_text(&property, source)?.to_lowercase();
    if !HTTP_VERBS.contains(&method.as_str()) {
        return None;
    }
    if !is_http_client_receiver(&object, source) {
        return None;
    }
    let args = call.child_by_field_name("arguments")?;
    let first = first_call_argument(&args)?;
    // A dynamic (non-literal) URL is surfaced with an UNKNOWN route, not dropped.
    let route = route_of_arg(&first, source);
    let route_unknown_reason = dynamic_url_reason(route.is_none());
    Some(HttpSurfaceDraft {
        direction: Direction::Consumer,
        http_method: method.to_uppercase(),
        route,
        route_unknown_reason,
        source_file: file.to_string(),
        line_start: call.start_position().row as i64 + 1,
        col_start: call.start_position().column as i64,
        symbol_stable_key: file_symbol_key(file),
        basis: InteractionBasis::ApiCall,
        framework: "axios",
    })
}

/// Whether a call receiver is structurally an HTTP client: EXACTLY the `axios`
/// identifier, or an `apiClient`-named instance/factory. Deliberately NOT a
/// loose `*client*` substring (review-5 item 1): `dbClient` / `s3Client` /
/// `redisClient` are non-HTTP clients and must not match. Express provider
/// receivers are excluded.
fn is_http_client_receiver(object: &Node, source: &[u8]) -> bool {
    match object.kind() {
        "identifier" => {
            // review-6 item 2: an unreadable receiver is UNKNOWN, not "" — it
            // cannot be confirmed as an HTTP client, so it is not one.
            let name = match node_text(object, source) {
                Some(n) => n,
                None => return false,
            };
            if EXPRESS_RECEIVERS.contains(&name.as_str()) {
                return false;
            }
            name == "axios" || name.to_lowercase().contains("apiclient")
        }
        // e.g. getApiClient().get(...) — object is a call to a client factory.
        "call_expression" => {
            if let Some(f) = object.child_by_field_name("function") {
                // review-6 item 2: unreadable callee text → cannot confirm → false.
                let text = match node_text(&f, source) {
                    Some(t) => t.to_lowercase(),
                    None => return false,
                };
                return text.contains("apiclient");
            }
            false
        }
        _ => false,
    }
}

/// `fetch('/x', { method: 'POST' })` — direct global call. A dynamic URL
/// (`fetch(url)`) still surfaces, with an UNKNOWN route.
fn try_extract_fetch(
    call: &Node,
    source: &[u8],
    file: &str,
    evidence: &TsHttpEvidence,
) -> Option<HttpSurfaceDraft> {
    // review-8 #2: a file-local/imported `fetch` binding shadows the global —
    // semantics unknown, no Layer-3 fact.
    if evidence.fetch_shadowed {
        return None;
    }
    let callee = call.child_by_field_name("function")?;
    if callee.kind() != "identifier" {
        return None;
    }
    if node_text(&callee, source)? != "fetch" {
        return None;
    }
    let args = call.child_by_field_name("arguments")?;
    let first = first_call_argument(&args)?;
    let route = route_of_arg(&first, source);
    let route_unknown_reason = dynamic_url_reason(route.is_none());
    // review-5 item 2: fetch's method defaults to GET ONLY when no method is
    // supplied. A supplied-but-non-static method (`{ method: dynamicVerb }`) is
    // UNKNOWN, never fabricated as GET — an UNKNOWN method matches no provider,
    // so it will not link.
    let method = match fetch_method(&args, source) {
        FetchMethod::Static(v) => v.to_uppercase(),
        FetchMethod::Absent => "GET".to_string(),
        FetchMethod::Dynamic => "UNKNOWN".to_string(),
    };
    Some(HttpSurfaceDraft {
        direction: Direction::Consumer,
        http_method: method,
        route,
        route_unknown_reason,
        source_file: file.to_string(),
        line_start: call.start_position().row as i64 + 1,
        col_start: call.start_position().column as i64,
        symbol_stable_key: file_symbol_key(file),
        basis: InteractionBasis::ApiCall,
        framework: "fetch",
    })
}

/// The three mutually-exclusive states of a fetch call's HTTP method
/// (review-5 item 2). Absence is not the same fact as an unreadable dynamic
/// value: only absence defaults to fetch's spec default (GET).
enum FetchMethod {
    /// `{ method: 'POST' }` — a static string verb.
    Static(String),
    /// No options object, or an options object without a `method` key → fetch
    /// defaults to GET.
    Absent,
    /// `{ method: someVar }` — a method is supplied but not statically
    /// readable → UNKNOWN, never guessed.
    Dynamic,
}

/// Read the method from a fetch options object (2nd argument).
fn fetch_method(args: &Node, source: &[u8]) -> FetchMethod {
    let mut cursor = args.walk();
    let mut seen_first = false;
    for child in args.children(&mut cursor) {
        if child.kind() == "(" || child.kind() == ")" || child.kind() == "," {
            continue;
        }
        if !seen_first {
            seen_first = true;
            continue; // skip the URL
        }
        if child.kind() == "object" {
            return match object_property_node(&child, source, "method") {
                // `method` key absent → GET default.
                None => FetchMethod::Absent,
                // present and a static string → that verb.
                Some(v) if v.kind() == "string" => match node_text(&v, source) {
                    Some(t) => {
                        FetchMethod::Static(t.trim_matches(|c| c == '"' || c == '\'').to_string())
                    }
                    None => FetchMethod::Dynamic,
                },
                // present but not a static string (variable/expr) → UNKNOWN.
                Some(_) => FetchMethod::Dynamic,
            };
        }
        // A non-object 2nd argument (e.g. a spread/variable options) — the
        // method is not statically readable.
        return FetchMethod::Dynamic;
    }
    // No second argument at all → fetch defaults to GET.
    FetchMethod::Absent
}

/// AWS CDK apigatewayv2 `X.addRoutes({ path: '/x', methods: [HttpMethod.GET] })`.
/// Emits one provider draft per method (anchored at the method token so
/// GET/POST at one call get distinct surface locations).
fn try_extract_cdk_routes(
    call: &Node,
    source: &[u8],
    file: &str,
    evidence: &TsHttpEvidence,
    out: &mut Vec<HttpSurfaceDraft>,
) {
    // review-5 item 1: `addRoutes` is a common property name — only a file that
    // imports an apigatewayv2 module is a CDK provider.
    if !evidence.cdk_apigw {
        return;
    }
    let callee = match call.child_by_field_name("function") {
        Some(c) if c.kind() == "member_expression" => c,
        _ => return,
    };
    let property = match callee.child_by_field_name("property") {
        Some(p) => p,
        None => return,
    };
    if node_text(&property, source).as_deref() != Some("addRoutes") {
        return;
    }
    let args = match call.child_by_field_name("arguments") {
        Some(a) => a,
        None => return,
    };
    let obj = match first_call_argument(&args) {
        Some(o) if o.kind() == "object" => o,
        _ => return,
    };
    let path_node = match object_property_node(&obj, source, "path") {
        Some(n) if n.kind() == "string" || n.kind() == "template_string" => n,
        _ => return, // dynamic/non-literal path — cannot anchor a route.
    };
    let route = match extract_route_literal(&path_node, source) {
        Some(r) => r,
        None => return,
    };
    let methods_node = match object_property_node(&obj, source, "methods") {
        Some(n) if n.kind() == "array" => n,
        _ => return,
    };
    let mut cursor = methods_node.walk();
    for element in methods_node.children(&mut cursor) {
        // Elements look like `apigateway.HttpMethod.GET` (member_expression) —
        // the verb is the final property.
        if element.kind() != "member_expression" {
            continue;
        }
        let verb_node = match element.child_by_field_name("property") {
            Some(v) => v,
            None => continue,
        };
        let verb = match node_text(&verb_node, source) {
            Some(v) => v.to_uppercase(),
            None => continue,
        };
        if !HTTP_VERBS.contains(&verb.to_lowercase().as_str()) {
            continue;
        }
        out.push(HttpSurfaceDraft {
            direction: Direction::Provider,
            http_method: verb,
            route: Some(route.clone()),
            route_unknown_reason: None,
            source_file: file.to_string(),
            line_start: verb_node.start_position().row as i64 + 1,
            col_start: verb_node.start_position().column as i64,
            symbol_stable_key: file_symbol_key(file),
            basis: InteractionBasis::Convention,
            framework: "aws_cdk_apigwv2",
        });
    }
}

// ── TS object-literal helpers ─────────────────────────────────────────

/// Find the value node of a `key: <value>` pair in an object literal.
fn object_property_node<'a>(obj: &Node<'a>, source: &[u8], key: &str) -> Option<Node<'a>> {
    let mut cursor = obj.walk();
    for pair in obj.children(&mut cursor) {
        if pair.kind() != "pair" {
            continue;
        }
        let key_node = pair.child_by_field_name("key")?;
        let key_text = node_text(&key_node, source)?;
        let key_text = key_text.trim_matches(|c| c == '"' || c == '\'');
        if key_text == key {
            return pair.child_by_field_name("value");
        }
    }
    None
}

/// First non-punctuation argument of a call.
#[allow(clippy::manual_find)] // cursor borrow prevents a clean `.find()`
fn first_call_argument<'a>(args: &Node<'a>) -> Option<Node<'a>> {
    let mut cursor = args.walk();
    for child in args.children(&mut cursor) {
        if child.kind() != "(" && child.kind() != ")" && child.kind() != "," {
            return Some(child);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_of(rel_path: &str, src: &str) -> FileInput {
        FileInput {
            rel_path: rel_path.to_string(),
            content: src.to_string(),
            content_hash: String::new(),
            size_bytes: src.len(),
            line_count: src.lines().count(),
            package_dependencies: None,
            tsconfig_aliases: None,
        }
    }

    /// Parse `src` in a file that carries axios/api-client IMPORT evidence — the
    /// condition every axios consumer surface now requires (review-5 item 1).
    /// The imports mirror glamCRM's real consumer files (`import axios` /
    /// `import { getApiClient } from '../config/api-client'`).
    fn ts_drafts(src: &str) -> Vec<HttpSurfaceDraft> {
        let with_imports = format!(
            "import axios from 'axios';\nimport {{ getApiClient }} from '../config/api-client';\n{src}"
        );
        detect_ts_http(&[file_of("frontend/api.ts", &with_imports)])
    }

    /// Parse `src` with NO injected imports — for the `fetch` global path and the
    /// import-gating negative tests.
    fn ts_drafts_raw(src: &str) -> Vec<HttpSurfaceDraft> {
        detect_ts_http(&[file_of("frontend/api.ts", src)])
    }

    #[test]
    fn axios_instance_consumer_static_route() {
        let drafts = ts_drafts("const x = getApiClient().get('/api/v2/clients');");
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].direction, Direction::Consumer);
        assert_eq!(drafts[0].http_method, "GET");
        assert_eq!(drafts[0].route.as_deref(), Some("/api/v2/clients"));
    }

    #[test]
    fn axios_dotted_consumer_and_template_route() {
        let drafts = ts_drafts("axios.put(`/api/v2/clients/${clientId}`, body);");
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].http_method, "PUT");
        assert_eq!(drafts[0].route.as_deref(), Some("/api/v2/clients/{param}"));
    }

    /// review-2 item 1: a STATIC absolute URL is statically readable and must
    /// become its path — not an UNKNOWN route — so it can match a provider.
    #[test]
    fn axios_absolute_url_reduced_to_path() {
        let drafts = ts_drafts("axios.get('https://api.example.test/api/v2/offers?x=1');");
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].direction, Direction::Consumer);
        assert_eq!(drafts[0].http_method, "GET");
        assert_eq!(drafts[0].route.as_deref(), Some("/api/v2/offers"));
    }

    // review-8 #2: a file-local `fetch` binding shadows the global — semantics
    // unknown, NO surface (never a false Layer-3 fact).
    #[test]
    fn locally_declared_fetch_is_not_an_http_surface() {
        let src = "const fetch = (k) => cache.get(k);\nfetch('/api/offers');\n";
        let drafts = detect_ts_http(&[file_of("frontend/cache.ts", src)]);
        assert!(
            drafts.is_empty(),
            "shadowed fetch must emit nothing: {drafts:?}"
        );
    }

    #[test]
    fn locally_defined_function_fetch_is_not_an_http_surface() {
        let src = "function fetch(key) { return store[key]; }\nfetch('/api/offers');\n";
        let drafts = detect_ts_http(&[file_of("frontend/store.ts", src)]);
        assert!(
            drafts.is_empty(),
            "shadowed fetch must emit nothing: {drafts:?}"
        );
    }

    // A known fetch POLYFILL import keeps web-fetch semantics — still a consumer.
    #[test]
    fn node_fetch_polyfill_import_still_surfaces() {
        let src = "import fetch from 'node-fetch';\nfetch('/api/offers');\n";
        let drafts = detect_ts_http(&[file_of("backend/job.ts", src)]);
        assert_eq!(
            drafts.len(),
            1,
            "polyfill fetch is a real consumer: {drafts:?}"
        );
        assert_eq!(drafts[0].route.as_deref(), Some("/api/offers"));
    }

    /// review-2 item 1: `fetch("https://host/path")` — static absolute URL →
    /// path route (not dynamic), so a `/path` provider match is possible.
    #[test]
    fn fetch_absolute_url_reduced_to_path() {
        let drafts = ts_drafts("fetch('http://svc.internal/api/v2/login', { method: 'POST' });");
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].http_method, "POST");
        assert_eq!(drafts[0].route.as_deref(), Some("/api/v2/login"));
    }

    #[test]
    fn fetch_consumer_reads_method() {
        let drafts = ts_drafts("fetch('/api/v2/login', { method: 'POST' });");
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].http_method, "POST");
        assert_eq!(drafts[0].route.as_deref(), Some("/api/v2/login"));
    }

    /// `fetch` is a global — it needs NO import evidence (uses `ts_drafts_raw`).
    /// No options object → fetch's spec default GET.
    #[test]
    fn fetch_consumer_defaults_get() {
        let drafts = ts_drafts_raw("fetch('/api/v2/ping');");
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].http_method, "GET");
    }

    /// review-5 item 2: a SUPPLIED but non-static method (`{ method: verb }`)
    /// must be UNKNOWN, never fabricated as GET — an UNKNOWN method links to no
    /// provider. Only ABSENCE of a method defaults to GET.
    #[test]
    fn fetch_dynamic_method_is_unknown_not_get() {
        let drafts = ts_drafts_raw("const verb = pick(); fetch('/api/v2/x', { method: verb });");
        assert_eq!(drafts.len(), 1);
        assert_eq!(
            drafts[0].http_method, "UNKNOWN",
            "dynamic method must be UNKNOWN, not GET: {:?}",
            drafts
        );
        assert_eq!(drafts[0].route.as_deref(), Some("/api/v2/x"));
    }

    /// An options object WITHOUT a `method` key still defaults to GET (fetch
    /// spec) — absence is not the same fact as a dynamic value.
    #[test]
    fn fetch_options_without_method_defaults_get() {
        let drafts = ts_drafts_raw("fetch('/api/v2/x', { headers: {} });");
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].http_method, "GET");
    }

    #[test]
    fn dynamic_template_base_consumer_is_unknown_route_not_fabricated() {
        let drafts = ts_drafts("axios.get(`${BASE}/clients`);");
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].route, None, "dynamic base -> unknown route");
    }

    /// review-0 item 3: a bare-variable URL (`axios.get(url)`) must NOT vanish —
    /// it surfaces as a consumer with an UNKNOWN route, never fabricated.
    #[test]
    fn variable_url_axios_consumer_is_unknown_route_not_dropped() {
        let drafts = ts_drafts("const url = pick(); axios.get(url);");
        assert_eq!(drafts.len(), 1, "variable-URL consumer must still surface");
        assert_eq!(drafts[0].direction, Direction::Consumer);
        assert_eq!(drafts[0].http_method, "GET");
        assert_eq!(drafts[0].route, None, "variable URL -> unknown route");
    }

    /// review-0 item 3: `fetch(url)` with a variable URL surfaces with an
    /// UNKNOWN route, not dropped.
    #[test]
    fn variable_url_fetch_consumer_is_unknown_route_not_dropped() {
        let drafts = ts_drafts_raw("const url = pick(); fetch(url);");
        assert_eq!(drafts.len(), 1, "variable-URL fetch must still surface");
        assert_eq!(drafts[0].http_method, "GET");
        assert_eq!(drafts[0].route, None, "variable URL -> unknown route");
    }

    #[test]
    fn non_client_receiver_is_ignored() {
        // A Map/cache .get with a non-route arg must not be an HTTP consumer.
        let drafts = ts_drafts("const c = new Map(); c.get('key'); app.get('/x', h);");
        assert!(
            drafts.is_empty(),
            "express app.get and Map.get are not axios consumers: {:?}",
            drafts
        );
    }

    /// review-5 item 1: a verb call on a client-shaped receiver in a file WITHOUT
    /// axios/api-client import evidence is NOT an HTTP consumer — import evidence
    /// is required, not a bare receiver name.
    #[test]
    fn axios_call_without_import_is_not_consumer() {
        let drafts = ts_drafts_raw("const x = getApiClient().get('/api/v2/clients');");
        assert!(
            drafts.is_empty(),
            "no axios/api-client import → not an http consumer: {:?}",
            drafts
        );
    }

    /// review-5 item 1: even WITH axios imported, a non-HTTP client receiver
    /// (`dbClient` / `s3Client`) must NOT be an HTTP consumer — the receiver is
    /// corroborated as `axios`/`apiClient`, not a loose `*client*` substring.
    #[test]
    fn non_apiclient_client_receiver_is_not_consumer() {
        let drafts = ts_drafts("dbClient.get('key'); s3Client.get('bucket');");
        assert!(
            drafts.is_empty(),
            "dbClient/s3Client are not HTTP clients: {:?}",
            drafts
        );
    }

    /// review-5 item 1: `addRoutes` in a file that does NOT import an
    /// apigatewayv2 module is NOT a CDK provider (the property name alone is not
    /// evidence — many builders expose `addRoutes`).
    #[test]
    fn addroutes_without_apigw_import_is_not_provider() {
        let src = r#"
            router.addRoutes({
                path: '/api/v2/categories/{id}',
                methods: [apigateway.HttpMethod.GET],
            });
        "#;
        let drafts = ts_drafts_raw(src);
        assert!(
            drafts.is_empty(),
            "addRoutes without apigatewayv2 import is not a CDK provider: {:?}",
            drafts
        );
    }

    #[test]
    fn cdk_addroutes_provider_one_per_method() {
        let src = r#"
            import * as apigateway from 'aws-cdk-lib/aws-apigatewayv2';
            this.api.addRoutes({
                path: '/api/v2/categories/{id}',
                methods: [apigateway.HttpMethod.GET, apigateway.HttpMethod.PUT],
                integration: x,
            });
        "#;
        let file = file_of("serverless/infra/api.ts", src);
        let mut drafts = detect_ts_http(&[file]);
        drafts.sort_by(|a, b| a.http_method.cmp(&b.http_method));
        assert_eq!(drafts.len(), 2);
        assert!(drafts.iter().all(|d| d.direction == Direction::Provider));
        assert!(drafts
            .iter()
            .all(|d| d.route.as_deref() == Some("/api/v2/categories/{id}")));
        assert_eq!(drafts[0].http_method, "GET");
        assert_eq!(drafts[1].http_method, "PUT");
        // Distinct anchor columns so GET/POST don't collide on identity.
        assert_ne!(drafts[0].col_start, drafts[1].col_start);
    }
}
