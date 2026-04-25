//! Spring framework-liveness classifier.
//!
//! Post-extraction classifier that detects Spring container-managed
//! symbols by analyzing Java node annotations stored in `metadata_json`.
//!
//! Pure function. No I/O, no state, no storage dependency.
//!
//! ## Design Rationale
//!
//! Spring uses annotation-based dependency injection. Classes annotated
//! with `@Service`, `@Component`, `@Repository`, `@Controller`,
//! `@RestController`, or `@Configuration` are instantiated and managed
//! by the Spring container — they appear "dead" to static import analysis
//! because nothing in userland code explicitly instantiates them.
//!
//! Similarly, methods annotated with `@Bean` are factory methods that
//! return container-managed instances.
//!
//! This classifier emits `spring_container_managed` inference facts
//! which the dead-code query already suppresses (see `find_dead_nodes`
//! in `queries.rs`).
//!
//! ## Scope (v1)
//!
//! **Direct annotation match only.** Does NOT resolve:
//! - Meta-annotations (e.g., `@SpringBootApplication` includes `@Configuration`)
//! - Transitively inherited annotations
//! - XML-based Spring configuration
//!
//! Direct annotation matching covers the most common Spring usage patterns.
//! Meta-annotation resolution is a future enhancement.
//!
//! ## Technical Debt
//!
//! - Meta-annotation inheritance not implemented. A class annotated with
//!   `@SpringBootApplication` won't match because the classifier doesn't
//!   expand `@SpringBootApplication` → `@Configuration + @ComponentScan + ...`.

use serde::{Deserialize, Serialize};

/// Class-level Spring stereotype annotations with their convention identifiers.
///
/// Convention identifiers match the TS implementation in `spring-bean-detector.ts`.
const SPRING_CLASS_STEREOTYPES: &[(&str, &str, &str)] = &[
    ("Component", "spring_component", "class annotated @Component - container-managed bean"),
    ("Service", "spring_service", "class annotated @Service - business logic bean"),
    ("Repository", "spring_repository", "class annotated @Repository - data access bean"),
    ("Controller", "spring_controller", "class annotated @Controller - MVC controller bean"),
    ("RestController", "spring_rest_controller", "class annotated @RestController - HTTP handler bean"),
    ("Configuration", "spring_configuration", "class annotated @Configuration - bean factory class"),
];

/// Method-level Spring annotation for factory methods.
const SPRING_BEAN_ANNOTATION: (&str, &str, &str) = (
    "Bean",
    "spring_bean_factory",
    "method annotated @Bean - container-managed factory producing a bean",
);

/// Classifier version for basis_json provenance.
/// Bump when classification rules change.
const SPRING_CLASSIFIER_VERSION: u32 = 1;

// ── Input DTOs ───────────────────────────────────────────────────────

/// Input node for Spring liveness classification.
///
/// Narrow projection of graph node data — only the fields needed
/// for classification. Keeps the classifier decoupled from the
/// full storage node type.
#[derive(Debug, Clone)]
pub struct SpringNodeInput {
    /// Node's stable key (used as inference target).
    pub stable_key: String,
    /// Node kind (e.g., "SYMBOL").
    pub kind: String,
    /// Node subtype (e.g., "CLASS", "METHOD").
    pub subtype: Option<String>,
    /// Raw metadata_json string from the node.
    /// Expected shape: `{"annotations": [{"name": "Service"}, ...]}`
    pub metadata_json: Option<String>,
}

// ── Output DTOs ──────────────────────────────────────────────────────

/// Detected Spring container-managed inference.
///
/// One output per node that matches Spring annotation patterns.
/// The caller persists these as `inferences` rows.
///
/// Payload contract matches TS `spring-bean-detector.ts`:
/// - `value_json`: `{ annotation: "@Service", convention: "spring_service", reason: "..." }`
/// - `basis_json`: `{ convention: "spring_service", classifier_version: 1 }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpringLivenessInference {
    /// Target stable_key (the managed symbol).
    pub target_stable_key: String,
    /// Inference kind — always "spring_container_managed".
    pub kind: String,
    /// JSON payload: `{ annotation, convention, reason }`.
    pub value_json: String,
    /// Confidence score (0.0–1.0).
    pub confidence: f64,
    /// JSON payload: `{ convention, classifier_version }`.
    pub basis_json: String,
}

/// Internal structure for parsing annotations from metadata_json.
#[derive(Debug, Deserialize)]
struct MetadataWithAnnotations {
    #[serde(default)]
    annotations: Vec<Annotation>,
}

#[derive(Debug, Deserialize)]
struct Annotation {
    name: String,
    #[allow(dead_code)]
    arguments: Option<serde_json::Value>,
}

// ── Public interface ─────────────────────────────────────────────────

/// Classify nodes for Spring container-managed liveness.
///
/// Scans each node's `metadata_json.annotations` for Spring stereotype
/// annotations. Emits `SpringLivenessInference` for each match.
///
/// Handles both simple (`Service`) and fully-qualified
/// (`org.springframework.stereotype.Service`) annotation names.
///
/// Pure function. No I/O, no side effects.
///
/// # Arguments
///
/// * `nodes` - Slice of node projections to classify.
///
/// # Returns
///
/// Vector of inferences (may be empty if no nodes match).
pub fn classify_spring_liveness(nodes: &[SpringNodeInput]) -> Vec<SpringLivenessInference> {
    let mut results = Vec::new();

    for node in nodes {
        // Only process SYMBOL nodes
        if node.kind != "SYMBOL" {
            continue;
        }

        // Parse metadata_json for annotations
        let Some(metadata_str) = &node.metadata_json else {
            continue;
        };

        let metadata: MetadataWithAnnotations = match serde_json::from_str(metadata_str) {
            Ok(m) => m,
            Err(_) => continue, // Malformed JSON, skip
        };

        // Check annotations based on node subtype
        // Note: INTERFACE is NOT included — Spring stereotype annotations
        // instantiate classes, not interfaces. @Repository on an interface
        // is a Spring Data marker (different mechanism, not in scope).
        match node.subtype.as_deref() {
            Some("CLASS") => {
                // Check class-level stereotypes
                for ann in &metadata.annotations {
                    let simple_name = extract_simple_name(&ann.name);
                    if let Some((_, convention, reason)) = SPRING_CLASS_STEREOTYPES
                        .iter()
                        .find(|(name, _, _)| *name == simple_name)
                    {
                        results.push(SpringLivenessInference {
                            target_stable_key: node.stable_key.clone(),
                            kind: "spring_container_managed".to_string(),
                            value_json: serde_json::json!({
                                "annotation": format!("@{}", simple_name),
                                "convention": convention,
                                "reason": reason
                            })
                            .to_string(),
                            confidence: 0.95,
                            basis_json: serde_json::json!({
                                "convention": convention,
                                "classifier_version": SPRING_CLASSIFIER_VERSION
                            })
                            .to_string(),
                        });
                        break; // One inference per node
                    }
                }
            }
            Some("METHOD") => {
                // Check @Bean annotation
                for ann in &metadata.annotations {
                    let simple_name = extract_simple_name(&ann.name);
                    if simple_name == SPRING_BEAN_ANNOTATION.0 {
                        let (_, convention, reason) = SPRING_BEAN_ANNOTATION;
                        results.push(SpringLivenessInference {
                            target_stable_key: node.stable_key.clone(),
                            kind: "spring_container_managed".to_string(),
                            value_json: serde_json::json!({
                                "annotation": format!("@{}", simple_name),
                                "convention": convention,
                                "reason": reason
                            })
                            .to_string(),
                            confidence: 0.95,
                            basis_json: serde_json::json!({
                                "convention": convention,
                                "classifier_version": SPRING_CLASSIFIER_VERSION
                            })
                            .to_string(),
                        });
                        break;
                    }
                }
            }
            _ => continue, // Skip other subtypes
        }
    }

    results
}

/// Extract simple name from potentially fully-qualified annotation name.
///
/// `org.springframework.stereotype.Service` → `Service`
/// `Service` → `Service`
fn extract_simple_name(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_class_node(stable_key: &str, annotations_json: &str) -> SpringNodeInput {
        SpringNodeInput {
            stable_key: stable_key.to_string(),
            kind: "SYMBOL".to_string(),
            subtype: Some("CLASS".to_string()),
            metadata_json: Some(annotations_json.to_string()),
        }
    }

    fn make_method_node(stable_key: &str, annotations_json: &str) -> SpringNodeInput {
        SpringNodeInput {
            stable_key: stable_key.to_string(),
            kind: "SYMBOL".to_string(),
            subtype: Some("METHOD".to_string()),
            metadata_json: Some(annotations_json.to_string()),
        }
    }

    // ── Positive cases: class-level annotations ───────────────────

    #[test]
    fn detects_service_annotation() {
        let nodes = vec![make_class_node(
            "r1:src/UserService.java#UserService:SYMBOL:CLASS",
            r#"{"annotations":[{"name":"Service"}]}"#,
        )];

        let results = classify_spring_liveness(&nodes);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].kind, "spring_container_managed");
        assert!(results[0].target_stable_key.contains("UserService"));
        assert!((results[0].confidence - 0.95).abs() < f64::EPSILON);

        // Verify payload contract matches TS spring-bean-detector.ts
        assert!(results[0].value_json.contains(r#""annotation":"@Service""#));
        assert!(results[0].value_json.contains(r#""convention":"spring_service""#));
        assert!(results[0].value_json.contains(r#""reason":"#));
        assert!(results[0].basis_json.contains(r#""convention":"spring_service""#));
        assert!(results[0].basis_json.contains(r#""classifier_version":1"#));
    }

    #[test]
    fn detects_component_annotation() {
        let nodes = vec![make_class_node(
            "r1:src/Helper.java#Helper:SYMBOL:CLASS",
            r#"{"annotations":[{"name":"Component"}]}"#,
        )];

        let results = classify_spring_liveness(&nodes);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn detects_repository_annotation() {
        let nodes = vec![make_class_node(
            "r1:src/UserRepo.java#UserRepository:SYMBOL:CLASS",
            r#"{"annotations":[{"name":"Repository"}]}"#,
        )];

        let results = classify_spring_liveness(&nodes);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn detects_controller_annotation() {
        let nodes = vec![make_class_node(
            "r1:src/UserController.java#UserController:SYMBOL:CLASS",
            r#"{"annotations":[{"name":"Controller"}]}"#,
        )];

        let results = classify_spring_liveness(&nodes);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn detects_rest_controller_annotation() {
        let nodes = vec![make_class_node(
            "r1:src/ApiController.java#ApiController:SYMBOL:CLASS",
            r#"{"annotations":[{"name":"RestController"}]}"#,
        )];

        let results = classify_spring_liveness(&nodes);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn detects_configuration_annotation() {
        let nodes = vec![make_class_node(
            "r1:src/AppConfig.java#AppConfig:SYMBOL:CLASS",
            r#"{"annotations":[{"name":"Configuration"}]}"#,
        )];

        let results = classify_spring_liveness(&nodes);
        assert_eq!(results.len(), 1);
    }

    // ── Positive cases: method-level annotations ──────────────────

    #[test]
    fn detects_bean_annotation_on_method() {
        let nodes = vec![make_method_node(
            "r1:src/Config.java#AppConfig.dataSource:SYMBOL:METHOD",
            r#"{"annotations":[{"name":"Bean"}]}"#,
        )];

        let results = classify_spring_liveness(&nodes);

        assert_eq!(results.len(), 1);
        assert!(results[0].target_stable_key.contains("dataSource"));
    }

    // ── Positive cases: multiple annotations ──────────────────────

    #[test]
    fn detects_first_matching_annotation_when_multiple_present() {
        let nodes = vec![make_class_node(
            "r1:src/Service.java#MyService:SYMBOL:CLASS",
            r#"{"annotations":[{"name":"Deprecated"},{"name":"Service"},{"name":"Transactional"}]}"#,
        )];

        let results = classify_spring_liveness(&nodes);

        assert_eq!(results.len(), 1);
        assert!(results[0].value_json.contains("Service"));
    }

    // ── Negative cases ────────────────────────────────────────────

    #[test]
    fn ignores_non_spring_annotations() {
        let nodes = vec![make_class_node(
            "r1:src/Plain.java#PlainClass:SYMBOL:CLASS",
            r#"{"annotations":[{"name":"Deprecated"},{"name":"SuppressWarnings"}]}"#,
        )];

        let results = classify_spring_liveness(&nodes);
        assert!(results.is_empty());
    }

    #[test]
    fn ignores_nodes_without_metadata() {
        let nodes = vec![SpringNodeInput {
            stable_key: "r1:src/Plain.java#PlainClass:SYMBOL:CLASS".to_string(),
            kind: "SYMBOL".to_string(),
            subtype: Some("CLASS".to_string()),
            metadata_json: None,
        }];

        let results = classify_spring_liveness(&nodes);
        assert!(results.is_empty());
    }

    #[test]
    fn ignores_nodes_with_empty_annotations() {
        let nodes = vec![make_class_node(
            "r1:src/Plain.java#PlainClass:SYMBOL:CLASS",
            r#"{"annotations":[]}"#,
        )];

        let results = classify_spring_liveness(&nodes);
        assert!(results.is_empty());
    }

    #[test]
    fn ignores_non_symbol_nodes() {
        let nodes = vec![SpringNodeInput {
            stable_key: "r1:src/Main.java:FILE".to_string(),
            kind: "FILE".to_string(),
            subtype: None,
            metadata_json: Some(r#"{"annotations":[{"name":"Service"}]}"#.to_string()),
        }];

        let results = classify_spring_liveness(&nodes);
        assert!(results.is_empty());
    }

    #[test]
    fn ignores_malformed_metadata_json() {
        let nodes = vec![SpringNodeInput {
            stable_key: "r1:src/Broken.java#Broken:SYMBOL:CLASS".to_string(),
            kind: "SYMBOL".to_string(),
            subtype: Some("CLASS".to_string()),
            metadata_json: Some("not valid json".to_string()),
        }];

        let results = classify_spring_liveness(&nodes);
        assert!(results.is_empty());
    }

    #[test]
    fn ignores_bean_annotation_on_class() {
        // @Bean is only valid on methods, not classes
        let nodes = vec![make_class_node(
            "r1:src/Config.java#Config:SYMBOL:CLASS",
            r#"{"annotations":[{"name":"Bean"}]}"#,
        )];

        let results = classify_spring_liveness(&nodes);
        assert!(results.is_empty(), "@Bean on CLASS should not match");
    }

    #[test]
    fn ignores_service_annotation_on_method() {
        // @Service is for classes, not methods
        let nodes = vec![make_method_node(
            "r1:src/Service.java#Svc.doWork:SYMBOL:METHOD",
            r#"{"annotations":[{"name":"Service"}]}"#,
        )];

        let results = classify_spring_liveness(&nodes);
        assert!(results.is_empty(), "@Service on METHOD should not match");
    }

    // ── Edge cases ────────────────────────────────────────────────

    #[test]
    fn handles_empty_input() {
        let results = classify_spring_liveness(&[]);
        assert!(results.is_empty());
    }

    #[test]
    fn ignores_interface_with_stereotype() {
        // Spring stereotype annotations instantiate classes, not interfaces.
        // @Repository on an interface is a Spring Data marker (different
        // mechanism, requires separate detection). This classifier does
        // NOT suppress interfaces.
        let nodes = vec![SpringNodeInput {
            stable_key: "r1:src/UserRepo.java#UserRepository:SYMBOL:INTERFACE".to_string(),
            kind: "SYMBOL".to_string(),
            subtype: Some("INTERFACE".to_string()),
            metadata_json: Some(r#"{"annotations":[{"name":"Repository"}]}"#.to_string()),
        }];

        let results = classify_spring_liveness(&nodes);
        assert!(results.is_empty(), "INTERFACE should not match stereotype annotations");
    }

    #[test]
    fn produces_deterministic_output() {
        // Same input should produce identical output
        let nodes = vec![make_class_node(
            "r1:src/Svc.java#Svc:SYMBOL:CLASS",
            r#"{"annotations":[{"name":"Service"}]}"#,
        )];

        let r1 = classify_spring_liveness(&nodes);
        let r2 = classify_spring_liveness(&nodes);

        assert_eq!(r1, r2);
    }

    // ── Fully-qualified annotation names ──────────────────────────

    #[test]
    fn detects_fully_qualified_service_annotation() {
        // Java may use fully-qualified annotations: @org.springframework.stereotype.Service
        let nodes = vec![make_class_node(
            "r1:src/Svc.java#Svc:SYMBOL:CLASS",
            r#"{"annotations":[{"name":"org.springframework.stereotype.Service"}]}"#,
        )];

        let results = classify_spring_liveness(&nodes);

        assert_eq!(results.len(), 1);
        // Should emit simple name with @ prefix
        assert!(results[0].value_json.contains(r#""annotation":"@Service""#));
        assert!(results[0].value_json.contains(r#""convention":"spring_service""#));
    }

    #[test]
    fn detects_fully_qualified_bean_annotation() {
        let nodes = vec![make_method_node(
            "r1:src/Config.java#AppConfig.dataSource:SYMBOL:METHOD",
            r#"{"annotations":[{"name":"org.springframework.context.annotation.Bean"}]}"#,
        )];

        let results = classify_spring_liveness(&nodes);

        assert_eq!(results.len(), 1);
        assert!(results[0].value_json.contains(r#""annotation":"@Bean""#));
        assert!(results[0].value_json.contains(r#""convention":"spring_bean_factory""#));
    }

    #[test]
    fn extract_simple_name_works() {
        assert_eq!(extract_simple_name("Service"), "Service");
        assert_eq!(
            extract_simple_name("org.springframework.stereotype.Service"),
            "Service"
        );
        assert_eq!(
            extract_simple_name("org.springframework.context.annotation.Bean"),
            "Bean"
        );
        assert_eq!(extract_simple_name(""), "");
    }
}
