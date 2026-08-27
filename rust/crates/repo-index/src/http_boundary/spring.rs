//! Spring `@RestController` / `@Controller` HTTP PROVIDER detection (Java).
//!
//! Joins each method-level mapping annotation to its enclosing controller
//! class's `@RequestMapping` base path via `parent_node_uid`, composing (verb,
//! base + suffix). Reuses the RAW annotation parser exposed by
//! `classification::spring_liveness` (`parse_node_annotations`) — the SAME parse
//! `spring_liveness` classifies over, so there is one parser and no drift
//! (operator ruling 2026-08-24, Option A).
//!
//! ## `@RestController` (REST) vs `@Controller` (MVC) — one pipeline, a label
//! (HTTP-SURFACE-COHERENCE-1 §2.1)
//!
//! BOTH stereotypes register HTTP routes with Spring MVC's `DispatcherServlet`;
//! the only difference is the default return semantics: a `@RestController`
//! method's return value is the response BODY (REST/JSON), while a `@Controller`
//! method's return value is a VIEW NAME (server-rendered HTML) unless the method
//! is `@ResponseBody`. Both are HTTP providers — a `@Controller`'s
//! `@GetMapping("/owners")` serves `GET /owners` exactly as a `@RestController`
//! would. Emitting only `@RestController` (the prior behaviour) made every
//! server-rendered Spring MVC app — e.g. spring-petclinic, 6 `@Controller`
//! classes / 17 method-level routes (measured: 12 `@GetMapping` + 5 `@PostMapping`;
//! the 2 `@RequestMapping` hits are a Javadoc + a class-level base path, not
//! servable routes — operator ruling 2026-08-26 (c)) — render `0 surfaces`,
//! teaching an agent the OPPOSITE of
//! the truth. So we detect both in ONE pass and record the distinguishing fact
//! as a labelled `framework` value (`spring` = REST, `spring_mvc` = view-render),
//! never two pipelines. The import-evidence gate is unchanged: the route-bearing
//! `@GetMapping`/`@RequestMapping`/… annotations live in
//! `org.springframework.web.bind.annotation`, so a controller with mapping
//! methods imports that package whether its stereotype is `@Controller`
//! (`org.springframework.stereotype`) or `@RestController`.

use std::collections::{HashMap, HashSet};

use repo_graph_boundary_interaction::{Direction, InteractionBasis};
use repo_graph_classification::spring_liveness::parse_node_annotations;
use repo_graph_storage::types::GraphNode;

use super::{first_string_literal, join_route, source_file_from_stable_key, HttpSurfaceDraft};

/// Framework label for a `@RestController` route — the return value is the
/// response BODY (REST/JSON). Rides in the surface `evidence_json.framework`.
const FRAMEWORK_REST: &str = "spring";
/// Framework label for a plain `@Controller` route — server-rendered MVC
/// (view-name return). A distinct HONEST label on the SAME provider surface,
/// distinguishing view-render endpoints from REST ones (§2.1).
const FRAMEWORK_MVC: &str = "spring_mvc";

/// Detect Spring controller provider routes from extracted Java nodes, for both
/// `@RestController` (REST) and `@Controller` (server-rendered MVC) stereotypes
/// (§2.1) — the style is recorded on each surface's `framework`.
///
/// `provider_files` is the set of repo-relative `.java` files that import the
/// Spring web annotation package — the IMPORT evidence a controller-stereotype
/// annotation NAME alone does not provide (review-5 item 1; STANDING HONESTY
/// RULE 2). A controller class whose file is not in the set is skipped.
pub(super) fn detect_spring_http_providers(
    nodes: &[GraphNode],
    repo_uid: &str,
    provider_files: &HashSet<&str>,
) -> Vec<HttpSurfaceDraft> {
    // Pass 1: controller classes → (base path, style), keyed by node_uid. A
    // class is a controller if it carries `@RestController` (REST) OR
    // `@Controller` (MVC/view-render) — both register HTTP routes; the style is
    // recorded as the surface `framework` (§2.1), never a second pipeline.
    let mut controller_base: HashMap<&str, (BasePath, &'static str)> = HashMap::new();
    for node in nodes {
        let anns = parse_node_annotations(node.metadata_json.as_deref());
        if anns.is_empty() {
            continue;
        }
        // `@RestController` first (it is NOT also `@Controller` by name, so a
        // dedicated check), else the plain `@Controller` MVC stereotype.
        let style = if anns.iter().any(|a| a.simple_name == "RestController") {
            FRAMEWORK_REST
        } else if anns.iter().any(|a| a.simple_name == "Controller") {
            FRAMEWORK_MVC
        } else {
            continue;
        };
        // IMPORT evidence: the controller's file must import the Spring web
        // annotation package. Without it, the stereotype NAME is not a
        // trustworthy Layer-3 fact (STANDING HONESTY RULE 2).
        let source_file = source_file_from_stable_key(repo_uid, &node.stable_key)
            .unwrap_or_else(|| node.stable_key.clone());
        if !provider_files.contains(source_file.as_str()) {
            continue;
        }
        // Base path from a class-level @RequestMapping. Tri-state (review-6
        // item 2): NO class @RequestMapping → base is a known "" (method paths
        // are the full route); a readable literal → that base; a path argument
        // that is not a static string (`@RequestMapping(BASE_CONST)`) → UNKNOWN,
        // which makes every route under this controller UNKNOWN, never fabricated.
        let base = match anns.iter().find(|a| a.simple_name == "RequestMapping") {
            None => BasePath::Known(String::new()),
            Some(a) => match read_path_arg(a.args_raw.as_deref()) {
                PathArg::Literal(s) => BasePath::Known(s),
                PathArg::Absent => BasePath::Known(String::new()),
                PathArg::Unreadable => BasePath::Unknown,
            },
        };
        controller_base.insert(node.node_uid.as_str(), (base, style));
    }

    if controller_base.is_empty() {
        return Vec::new();
    }

    // Pass 2: methods with a mapping annotation whose parent is a rest controller.
    let mut drafts = Vec::new();
    for node in nodes {
        let parent = match node.parent_node_uid.as_deref() {
            Some(p) => p,
            None => continue,
        };
        let (base, framework) = match controller_base.get(parent) {
            Some((b, f)) => (b, *f),
            None => continue,
        };
        let anns = parse_node_annotations(node.metadata_json.as_deref());
        for ann in &anns {
            let verb = match mapping_verb(&ann.simple_name, ann.args_raw.as_deref()) {
                Some(v) => v,
                None => continue,
            };
            // Method path suffix — tri-state (review-6 item 2). A mapping with no
            // path argument (`@GetMapping` / `@GetMapping(produces=…)`) maps to
            // the base path; a path argument that is not a static string
            // (`@GetMapping(value = PATH_CONST)`) is UNKNOWN → `route: None`,
            // never silently the base path.
            let (route, route_unknown_reason): (Option<String>, Option<&'static str>) =
                match (base, read_path_arg(ann.args_raw.as_deref())) {
                    // Base or method path unreadable → the composed route is
                    // UNKNOWN, and we record WHY (review-0 item 4) rather than a
                    // bare `None`.
                    (BasePath::Unknown, _) => (
                        None,
                        Some("class @RequestMapping base path is not a static string literal"),
                    ),
                    (_, PathArg::Unreadable) => (
                        None,
                        Some("mapping path argument is not a static string literal"),
                    ),
                    (BasePath::Known(b), PathArg::Literal(s)) => (Some(join_route(b, &s)), None),
                    (BasePath::Known(b), PathArg::Absent) => (Some(join_route(b, "")), None),
                };
            let (line, col) = node
                .location
                .as_ref()
                .map(|l| (l.line_start, l.col_start))
                .unwrap_or((1, 0));
            let source_file = source_file_from_stable_key(repo_uid, &node.stable_key)
                .unwrap_or_else(|| node.stable_key.clone());
            drafts.push(HttpSurfaceDraft {
                direction: Direction::Provider,
                http_method: verb.to_string(),
                route,
                route_unknown_reason,
                source_file,
                line_start: line,
                col_start: col,
                symbol_stable_key: node.stable_key.clone(),
                basis: InteractionBasis::Annotation,
                framework,
            });
        }
    }
    drafts
}

/// A controller's class-level base path (review-6 item 2). Distinguishes a
/// *known* base (including a known-empty base when the class has no
/// `@RequestMapping`) from an *unknown* base whose `@RequestMapping` path
/// argument is not a static string literal — the latter poisons every route
/// composed under it, which must then be UNKNOWN, never fabricated.
enum BasePath {
    Known(String),
    Unknown,
}

/// Tri-state read of a Spring mapping annotation's PATH argument (review-6
/// item 2). The three cases are distinct facts and must not collapse to `""`:
///
/// - `Literal(path)` — a statically-readable route path (`("/x")` /
///   `(value = "/x", …)`).
/// - `Absent` — no path argument at all (`()`, `(method = …)`,
///   `(produces = …)`): Spring semantics map this to the base path.
/// - `Unreadable` — a path argument IS present but is not a static string
///   (`(BASE_CONST)` / `(value = PATH)`): the route is UNKNOWN, never the
///   fabricated base path a `first_string_literal(...).unwrap_or_default()`
///   would have produced.
#[derive(Debug, PartialEq)]
enum PathArg {
    Literal(String),
    Absent,
    Unreadable,
}

/// Read the path argument of a Spring mapping annotation as a [`PathArg`].
///
/// Rules (over the raw argument text, e.g. `("/x")` or `(value = PATH)`):
/// - no args / `()` → `Absent`;
/// - a positional string literal, or `value = "…"` / `path = "…"` → `Literal`;
/// - a positional non-string first argument, or `value =`/`path =` bound to a
///   non-string (constant / expression / array) → `Unreadable`;
/// - only non-path named args (`method =`, `produces =`, `consumes =`, …) →
///   `Absent` (no path specified, maps to base). A `value = {"/a"}` array yields
///   its first literal (Spring allows multiple paths; the first is a real one).
fn read_path_arg(args_raw: Option<&str>) -> PathArg {
    let raw = match args_raw {
        Some(r) => r.trim(),
        None => return PathArg::Absent,
    };
    let inner = raw
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(raw)
        .trim();
    if inner.is_empty() {
        return PathArg::Absent;
    }
    // Positional first argument (no leading `key =`).
    let first_clause = inner.split(',').next().unwrap_or(inner).trim();
    let first_is_named = first_clause
        .split_once('=')
        .map(|(k, _)| is_ident(k.trim()))
        .unwrap_or(false);
    if !first_is_named {
        // Positional: a string literal is the path; anything else is a
        // present-but-unreadable path.
        return match first_string_literal(inner) {
            Some(s) => PathArg::Literal(s),
            None => PathArg::Unreadable,
        };
    }
    // Named arguments: look for `value =` / `path =`.
    for clause in inner.split(',') {
        if let Some((k, v)) = clause.split_once('=') {
            if matches!(k.trim(), "value" | "path") {
                let v = v.trim();
                if v.starts_with('"') || v.starts_with('{') {
                    return match first_string_literal(v) {
                        Some(s) => PathArg::Literal(s),
                        None => PathArg::Unreadable,
                    };
                }
                // `value = SOME_CONSTANT` — present but not statically readable.
                return PathArg::Unreadable;
            }
        }
    }
    // No path-bearing argument among the named args → path absent (maps to base).
    PathArg::Absent
}

/// Whether `s` is a bare Java identifier (an argument *key* like `value`,
/// `method`), as opposed to an expression — used to tell a `key = …` named
/// argument from a positional expression that merely contains `=`.
fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !s.chars().next().unwrap().is_ascii_digit()
}

/// HTTP verb for a Spring mapping annotation, or `None` if it is not a mapping.
/// `@RequestMapping` on a method resolves its verb from `method =
/// RequestMethod.X` in the argument text; without one it is not verb-decidable
/// and is skipped (honest — no fabricated verb).
fn mapping_verb(ann_name: &str, args_raw: Option<&str>) -> Option<&'static str> {
    match ann_name {
        "GetMapping" => Some("GET"),
        "PostMapping" => Some("POST"),
        "PutMapping" => Some("PUT"),
        "DeleteMapping" => Some("DELETE"),
        "PatchMapping" => Some("PATCH"),
        "RequestMapping" => {
            // A method-level `@RequestMapping` with an explicit `method =
            // RequestMethod.X` resolves to that verb. WITHOUT one, Spring maps the
            // handler to ALL HTTP verbs — so the honest representation is the
            // all-methods token `ANY`, NOT a skip (which would drop a real route)
            // and NOT a fabricated `GET` (§2.1; review-0 item 3). petclinic has no
            // such method-level mapping — its only `@RequestMapping` is a
            // class-level base path — so this arm does not change petclinic's
            // route count; it makes the detector correct for repos that do use it.
            match args_raw {
                Some(args) => {
                    for (needle, verb) in [
                        ("RequestMethod.GET", "GET"),
                        ("RequestMethod.POST", "POST"),
                        ("RequestMethod.PUT", "PUT"),
                        ("RequestMethod.DELETE", "DELETE"),
                        ("RequestMethod.PATCH", "PATCH"),
                    ] {
                        if args.contains(needle) {
                            return Some(verb);
                        }
                    }
                    // Present args but no `method =` → all-methods handler.
                    Some("ANY")
                }
                // `@RequestMapping` with no args at all → all-methods handler.
                None => Some("ANY"),
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The provider-file set for a test: every file referenced by the test
    /// nodes, i.e. import evidence is present. Individual tests override to prove
    /// the import gate.
    fn all_files(nodes: &[GraphNode], repo_uid: &str) -> HashSet<&'static str> {
        // Leak the derived paths so the set can borrow `&str` for the duration of
        // the test (test-only; never runs in production).
        nodes
            .iter()
            .filter_map(|n| source_file_from_stable_key(repo_uid, &n.stable_key))
            .map(|s| &*Box::leak(s.into_boxed_str()))
            .collect()
    }

    fn class_node(uid: &str, stable: &str, anns: &str) -> GraphNode {
        GraphNode {
            node_uid: uid.into(),
            snapshot_uid: "s".into(),
            repo_uid: "r".into(),
            stable_key: stable.into(),
            kind: "SYMBOL".into(),
            subtype: Some("CLASS".into()),
            name: "C".into(),
            qualified_name: None,
            file_uid: None,
            parent_node_uid: None,
            location: Some(repo_graph_storage::types::SourceLocation {
                line_start: 5,
                col_start: 0,
                line_end: 5,
                col_end: 1,
            }),
            signature: None,
            visibility: None,
            doc_comment: None,
            metadata_json: Some(anns.into()),
        }
    }

    fn method_node(uid: &str, parent: &str, stable: &str, line: i64, anns: &str) -> GraphNode {
        GraphNode {
            node_uid: uid.into(),
            snapshot_uid: "s".into(),
            repo_uid: "r".into(),
            stable_key: stable.into(),
            kind: "SYMBOL".into(),
            subtype: Some("METHOD".into()),
            name: "m".into(),
            qualified_name: None,
            file_uid: None,
            parent_node_uid: Some(parent.into()),
            location: Some(repo_graph_storage::types::SourceLocation {
                line_start: line,
                col_start: 4,
                line_end: line,
                col_end: 5,
            }),
            signature: None,
            visibility: None,
            doc_comment: None,
            metadata_json: Some(anns.into()),
        }
    }

    #[test]
    fn mapping_verb_maps_annotations() {
        assert_eq!(mapping_verb("GetMapping", None), Some("GET"));
        assert_eq!(
            mapping_verb("DeleteMapping", Some("(\"/{id}\")")),
            Some("DELETE")
        );
        assert_eq!(
            mapping_verb("RequestMapping", Some("(method = RequestMethod.PUT)")),
            Some("PUT")
        );
        // §2.1 / review-0 item 3: a method-level `@RequestMapping` WITHOUT an
        // explicit `method =` maps to ALL verbs in Spring — the honest token is
        // `ANY`, never a skip (drops a real route) or a fabricated `GET`.
        assert_eq!(
            mapping_verb("RequestMapping", Some("(\"/x\")")),
            Some("ANY")
        );
        assert_eq!(mapping_verb("RequestMapping", None), Some("ANY"));
        assert_eq!(
            mapping_verb("RequestMapping", Some("(produces = \"application/json\")")),
            Some("ANY")
        );
        // A non-mapping annotation is still not a route.
        assert_eq!(mapping_verb("Autowired", None), None);
    }

    #[test]
    fn verb_undecidable_request_mapping_emits_any_route() {
        // A `@Controller` method annotated only `@RequestMapping("/all")` (no
        // `method =`) is a real all-verbs endpoint: emit it with method `ANY`,
        // never drop it. (petclinic has none — its sole `@RequestMapping` is a
        // class base path — so this proves the detector, not petclinic's count.)
        let nodes = vec![
            class_node(
                "c1",
                "r:src/AllController.java#AllController:SYMBOL:CLASS",
                r#"{"annotations":[{"name":"Controller"}]}"#,
            ),
            method_node(
                "m1",
                "c1",
                "r:src/AllController.java#AllController.handle:SYMBOL:METHOD",
                20,
                r#"{"annotations":[{"name":"RequestMapping","arguments":"(\"/all\")"}]}"#,
            ),
        ];
        let files = all_files(&nodes, "r");
        let drafts = detect_spring_http_providers(&nodes, "r", &files);
        assert_eq!(drafts.len(), 1, "{drafts:?}");
        assert_eq!(drafts[0].http_method, "ANY");
        assert_eq!(drafts[0].route.as_deref(), Some("/all"));
        assert_eq!(drafts[0].framework, "spring_mvc");
    }

    #[test]
    fn spring_provider_composes_class_and_method_routes() {
        let nodes = vec![
            class_node(
                "c1",
                "r:backend/ClientController.java#ClientController:SYMBOL:CLASS",
                r#"{"annotations":[{"name":"RestController"},{"name":"RequestMapping","arguments":"(\"/api/v2/clients\")"}]}"#,
            ),
            method_node(
                "m1",
                "c1",
                "r:backend/ClientController.java#ClientController.byId:SYMBOL:METHOD",
                20,
                r#"{"annotations":[{"name":"GetMapping","arguments":"(\"/{id}\")"}]}"#,
            ),
            method_node(
                "m2",
                "c1",
                "r:backend/ClientController.java#ClientController.create:SYMBOL:METHOD",
                30,
                r#"{"annotations":[{"name":"PostMapping","arguments":"(\"\")"}]}"#,
            ),
        ];
        let files = all_files(&nodes, "r");
        let drafts = detect_spring_http_providers(&nodes, "r", &files);
        assert_eq!(drafts.len(), 2);
        let get = drafts.iter().find(|d| d.http_method == "GET").unwrap();
        assert_eq!(get.route.as_deref(), Some("/api/v2/clients/{id}"));
        assert_eq!(get.direction, Direction::Provider);
        assert_eq!(get.source_file, "backend/ClientController.java");
        assert_eq!(get.line_start, 20);
        let post = drafts.iter().find(|d| d.http_method == "POST").unwrap();
        assert_eq!(post.route.as_deref(), Some("/api/v2/clients"));
    }

    #[test]
    fn spring_provider_all_verbs_compose_with_base_path() {
        // Slice §4: class base path + method path, ALL supported verbs — one
        // provider surface per mapping annotation, verb + composed route correct
        // (review-4 item 3). Covers Get/Post/Put/Delete/PatchMapping AND a
        // method-level @RequestMapping whose verb rides `method = RequestMethod.X`.
        let nodes = vec![
            class_node(
                "c1",
                "r:backend/C.java#C:SYMBOL:CLASS",
                r#"{"annotations":[{"name":"RestController"},{"name":"RequestMapping","arguments":"(\"/api\")"}]}"#,
            ),
            method_node(
                "m_get",
                "c1",
                "r:backend/C.java#C.g:SYMBOL:METHOD",
                10,
                r#"{"annotations":[{"name":"GetMapping","arguments":"(\"/g\")"}]}"#,
            ),
            method_node(
                "m_post",
                "c1",
                "r:backend/C.java#C.p:SYMBOL:METHOD",
                11,
                r#"{"annotations":[{"name":"PostMapping","arguments":"(\"/p\")"}]}"#,
            ),
            method_node(
                "m_put",
                "c1",
                "r:backend/C.java#C.u:SYMBOL:METHOD",
                12,
                r#"{"annotations":[{"name":"PutMapping","arguments":"(\"/u/{id}\")"}]}"#,
            ),
            method_node(
                "m_del",
                "c1",
                "r:backend/C.java#C.d:SYMBOL:METHOD",
                13,
                r#"{"annotations":[{"name":"DeleteMapping","arguments":"(\"/d/{id}\")"}]}"#,
            ),
            method_node(
                "m_patch",
                "c1",
                "r:backend/C.java#C.x:SYMBOL:METHOD",
                14,
                r#"{"annotations":[{"name":"PatchMapping","arguments":"(\"/x\")"}]}"#,
            ),
            method_node(
                "m_rm",
                "c1",
                "r:backend/C.java#C.rm:SYMBOL:METHOD",
                15,
                r#"{"annotations":[{"name":"RequestMapping","arguments":"(value = \"/legacy\", method = RequestMethod.PUT)"}]}"#,
            ),
        ];
        let files = all_files(&nodes, "r");
        let drafts = detect_spring_http_providers(&nodes, "r", &files);
        let has = |verb: &str, route: &str| {
            drafts.iter().any(|d| {
                d.direction == Direction::Provider
                    && d.http_method == verb
                    && d.route.as_deref() == Some(route)
            })
        };
        assert!(has("GET", "/api/g"), "{drafts:?}");
        assert!(has("POST", "/api/p"), "{drafts:?}");
        assert!(has("PUT", "/api/u/{id}"), "{drafts:?}");
        assert!(has("DELETE", "/api/d/{id}"), "{drafts:?}");
        assert!(has("PATCH", "/api/x"), "{drafts:?}");
        // Method-level @RequestMapping verb from `method = RequestMethod.PUT`,
        // composed on the class base path.
        assert!(has("PUT", "/api/legacy"), "{drafts:?}");
        assert_eq!(
            drafts.len(),
            6,
            "one surface per mapping annotation: {drafts:?}"
        );
    }

    #[test]
    fn non_rest_controller_emits_nothing() {
        // A @Service with a @GetMapping-looking method must NOT produce routes:
        // no @RestController on the enclosing class.
        let nodes = vec![
            class_node(
                "c1",
                "r:backend/Svc.java#Svc:SYMBOL:CLASS",
                r#"{"annotations":[{"name":"Service"}]}"#,
            ),
            method_node(
                "m1",
                "c1",
                "r:backend/Svc.java#Svc.m:SYMBOL:METHOD",
                10,
                r#"{"annotations":[{"name":"GetMapping","arguments":"(\"/x\")"}]}"#,
            ),
        ];
        let files = all_files(&nodes, "r");
        assert!(detect_spring_http_providers(&nodes, "r", &files).is_empty());
    }

    #[test]
    fn rest_controller_without_import_evidence_emits_nothing() {
        // review-5 item 1: a `@RestController` class whose file is NOT in the
        // import-evidence set (its `.java` does not import the Spring web
        // annotation package) produces NO provider surfaces — a bare annotation
        // NAME is not a trustworthy Layer-3 fact (STANDING HONESTY RULE 2).
        let nodes = vec![
            class_node(
                "c1",
                "r:backend/Fake.java#Fake:SYMBOL:CLASS",
                r#"{"annotations":[{"name":"RestController"},{"name":"RequestMapping","arguments":"(\"/api\")"}]}"#,
            ),
            method_node(
                "m1",
                "c1",
                "r:backend/Fake.java#Fake.g:SYMBOL:METHOD",
                10,
                r#"{"annotations":[{"name":"GetMapping","arguments":"(\"/g\")"}]}"#,
            ),
        ];
        // Empty provider-file set = no import evidence for backend/Fake.java.
        let empty: HashSet<&str> = HashSet::new();
        assert!(
            detect_spring_http_providers(&nodes, "r", &empty).is_empty(),
            "no Spring-web import → no provider surfaces"
        );
        // With evidence present, the same nodes DO surface — proving it is the
        // import gate, not the annotations, that suppressed them.
        let files = all_files(&nodes, "r");
        assert_eq!(detect_spring_http_providers(&nodes, "r", &files).len(), 1);
    }

    #[test]
    fn read_path_arg_tri_state() {
        // review-6 item 2: absence, a readable literal, and an unreadable
        // (constant) path argument are three DISTINCT facts.
        assert_eq!(read_path_arg(None), PathArg::Absent);
        assert_eq!(read_path_arg(Some("()")), PathArg::Absent);
        assert_eq!(
            read_path_arg(Some("(\"/x\")")),
            PathArg::Literal("/x".into())
        );
        assert_eq!(
            read_path_arg(Some("(value = \"/x\", method = RequestMethod.GET)")),
            PathArg::Literal("/x".into())
        );
        // Non-path named args only → no path specified (maps to base).
        assert_eq!(
            read_path_arg(Some("(method = RequestMethod.GET)")),
            PathArg::Absent
        );
        assert_eq!(
            read_path_arg(Some("(produces = \"application/json\")")),
            PathArg::Absent
        );
        // Path argument present but a constant / expression → UNKNOWN.
        assert_eq!(read_path_arg(Some("(value = PATH)")), PathArg::Unreadable);
        assert_eq!(read_path_arg(Some("(BASE_CONST)")), PathArg::Unreadable);
    }

    #[test]
    fn method_path_constant_arg_is_unknown_route_not_base() {
        // review-6 item 2: `@GetMapping(value = PATH)` with a constant path must
        // NOT silently become the class base path — it is an UNKNOWN route
        // (`route: None`), so it fabricates no path and links to no provider.
        let nodes = vec![
            class_node(
                "c1",
                "r:backend/C.java#C:SYMBOL:CLASS",
                r#"{"annotations":[{"name":"RestController"},{"name":"RequestMapping","arguments":"(\"/api\")"}]}"#,
            ),
            method_node(
                "m1",
                "c1",
                "r:backend/C.java#C.g:SYMBOL:METHOD",
                10,
                r#"{"annotations":[{"name":"GetMapping","arguments":"(value = OFFERS_PATH)"}]}"#,
            ),
        ];
        let files = all_files(&nodes, "r");
        let drafts = detect_spring_http_providers(&nodes, "r", &files);
        assert_eq!(drafts.len(), 1, "surface still emitted: {drafts:?}");
        assert_eq!(drafts[0].http_method, "GET");
        assert_eq!(
            drafts[0].route, None,
            "constant method path -> unknown route, never the base path"
        );
    }

    #[test]
    fn class_base_constant_poisons_all_routes_to_unknown() {
        // review-6 item 2: an unreadable class `@RequestMapping(BASE_CONST)` makes
        // every route composed under it UNKNOWN — never a fabricated method-only
        // path that could false-link.
        let nodes = vec![
            class_node(
                "c1",
                "r:backend/C.java#C:SYMBOL:CLASS",
                r#"{"annotations":[{"name":"RestController"},{"name":"RequestMapping","arguments":"(API_BASE)"}]}"#,
            ),
            method_node(
                "m1",
                "c1",
                "r:backend/C.java#C.g:SYMBOL:METHOD",
                10,
                r#"{"annotations":[{"name":"GetMapping","arguments":"(\"/offers\")"}]}"#,
            ),
        ];
        let files = all_files(&nodes, "r");
        let drafts = detect_spring_http_providers(&nodes, "r", &files);
        assert_eq!(drafts.len(), 1);
        assert_eq!(
            drafts[0].route, None,
            "unknown base -> unknown route, never method-path-only"
        );
    }

    #[test]
    fn mvc_controller_emits_providers_labelled_spring_mvc() {
        // §2.1: a plain `@Controller` (server-rendered MVC, the spring-petclinic
        // shape) is an HTTP provider exactly as `@RestController`. Its methods'
        // mappings compose to routes; the surface carries `framework: spring_mvc`
        // so the view-render fact is labelled, not lost. Prior behaviour emitted
        // NOTHING here (0 surfaces against a real 17-route app).
        let nodes = vec![
            class_node(
                "c1",
                "r:owner/OwnerController.java#OwnerController:SYMBOL:CLASS",
                r#"{"annotations":[{"name":"Controller"}]}"#,
            ),
            method_node(
                "m1",
                "c1",
                "r:owner/OwnerController.java#OwnerController.list:SYMBOL:METHOD",
                94,
                r#"{"annotations":[{"name":"GetMapping","arguments":"(\"/owners\")"}]}"#,
            ),
            method_node(
                "m2",
                "c1",
                "r:owner/OwnerController.java#OwnerController.edit:SYMBOL:METHOD",
                141,
                r#"{"annotations":[{"name":"PostMapping","arguments":"(\"/owners/{ownerId}/edit\")"}]}"#,
            ),
        ];
        let files = all_files(&nodes, "r");
        let drafts = detect_spring_http_providers(&nodes, "r", &files);
        assert_eq!(drafts.len(), 2, "{drafts:?}");
        let get = drafts.iter().find(|d| d.http_method == "GET").unwrap();
        assert_eq!(get.direction, Direction::Provider);
        assert_eq!(get.route.as_deref(), Some("/owners"));
        assert_eq!(
            get.framework, "spring_mvc",
            "plain @Controller is labelled view-render MVC"
        );
        let post = drafts.iter().find(|d| d.http_method == "POST").unwrap();
        assert_eq!(post.route.as_deref(), Some("/owners/{ownerId}/edit"));
        assert_eq!(post.framework, "spring_mvc");
    }

    #[test]
    fn rest_controller_keeps_spring_framework_label() {
        // The REST style stays `framework: spring` — the MVC addition does not
        // relabel existing `@RestController` providers.
        let nodes = vec![
            class_node(
                "c1",
                "r:api/ClientController.java#ClientController:SYMBOL:CLASS",
                r#"{"annotations":[{"name":"RestController"},{"name":"RequestMapping","arguments":"(\"/api\")"}]}"#,
            ),
            method_node(
                "m1",
                "c1",
                "r:api/ClientController.java#ClientController.all:SYMBOL:METHOD",
                20,
                r#"{"annotations":[{"name":"GetMapping","arguments":"(\"/clients\")"}]}"#,
            ),
        ];
        let files = all_files(&nodes, "r");
        let drafts = detect_spring_http_providers(&nodes, "r", &files);
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].route.as_deref(), Some("/api/clients"));
        assert_eq!(drafts[0].framework, "spring");
    }

    #[test]
    fn mvc_controller_without_import_evidence_emits_nothing() {
        // The import-evidence gate (STANDING HONESTY RULE 2) applies to
        // `@Controller` just as to `@RestController`: no Spring-web import → no
        // provider surfaces, even though the mapping annotations are present.
        let nodes = vec![
            class_node(
                "c1",
                "r:owner/Fake.java#Fake:SYMBOL:CLASS",
                r#"{"annotations":[{"name":"Controller"}]}"#,
            ),
            method_node(
                "m1",
                "c1",
                "r:owner/Fake.java#Fake.g:SYMBOL:METHOD",
                10,
                r#"{"annotations":[{"name":"GetMapping","arguments":"(\"/x\")"}]}"#,
            ),
        ];
        let empty: HashSet<&str> = HashSet::new();
        assert!(
            detect_spring_http_providers(&nodes, "r", &empty).is_empty(),
            "no Spring-web import → no MVC provider surfaces"
        );
    }

    #[test]
    fn method_without_path_arg_maps_to_base() {
        // Absence is not unreadable: a bare `@GetMapping` (no path) maps to the
        // controller base path — a known route, not UNKNOWN.
        let nodes = vec![
            class_node(
                "c1",
                "r:backend/C.java#C:SYMBOL:CLASS",
                r#"{"annotations":[{"name":"RestController"},{"name":"RequestMapping","arguments":"(\"/api/health\")"}]}"#,
            ),
            method_node(
                "m1",
                "c1",
                "r:backend/C.java#C.g:SYMBOL:METHOD",
                10,
                r#"{"annotations":[{"name":"GetMapping"}]}"#,
            ),
        ];
        let files = all_files(&nodes, "r");
        let drafts = detect_spring_http_providers(&nodes, "r", &files);
        assert_eq!(drafts.len(), 1);
        assert_eq!(
            drafts[0].route.as_deref(),
            Some("/api/health"),
            "bare mapping maps to the base path"
        );
    }
}
