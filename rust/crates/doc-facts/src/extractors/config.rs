//! Configuration file extractor.
//!
//! Extracts environment surface facts from:
//! - docker-compose.yml (service definitions)
//! - .env.* files (environment-specific config)

use crate::types::{
    compute_confidence, DocFile, DocKind, ExtractedFact, ExtractionMethod, FactKind, RefKind,
};

/// Extract semantic facts from configuration files.
pub fn extract(doc: &DocFile, content: &str, content_hash: &str) -> Vec<ExtractedFact> {
    let file_name = doc
        .relative_path
        .rsplit('/')
        .next()
        .unwrap_or(&doc.relative_path)
        .to_lowercase();

    if file_name.starts_with("docker-compose") || file_name.starts_with("compose.") {
        extract_docker_compose(doc, content, content_hash)
    } else if file_name.starts_with(".env") {
        extract_env_file(doc, content, content_hash)
    } else {
        Vec::new()
    }
}

/// Extract service definitions from docker-compose.yml.
fn extract_docker_compose(doc: &DocFile, content: &str, content_hash: &str) -> Vec<ExtractedFact> {
    let mut facts = Vec::new();
    let confidence = compute_confidence(ExtractionMethod::ConfigParse, doc.generated);

    // Parse YAML to find services
    let yaml: serde_yaml::Value = match serde_yaml::from_str(content) {
        Ok(v) => v,
        Err(_) => return facts,
    };

    // Look for services section
    let services = match yaml.get("services") {
        Some(s) => s,
        None => return facts,
    };

    let services_map = match services.as_mapping() {
        Some(m) => m,
        None => return facts,
    };

    // Each service is an environment surface
    for (name, _config) in services_map {
        let service_name = match name.as_str() {
            Some(n) => n,
            None => continue,
        };

        // Infer environment from filename or directory
        let environment = infer_environment_from_path(&doc.relative_path);

        facts.push(ExtractedFact {
            fact_kind: FactKind::EnvironmentSurface,
            subject_ref: service_name.to_string(),
            subject_ref_kind: RefKind::Module,
            object_ref: Some(environment),
            object_ref_kind: Some(RefKind::Environment),
            source_file: doc.relative_path.clone(),
            line_start: None, // YAML parsing doesn't preserve line numbers easily
            line_end: None,
            excerpt: Some(format!("service: {}", service_name)),
            content_hash: content_hash.to_string(),
            extraction_method: ExtractionMethod::ConfigParse,
            confidence,
            generated: doc.generated,
            doc_kind: DocKind::Config,
        });
    }

    facts
}

/// Extract environment facts from .env files.
fn extract_env_file(doc: &DocFile, _content: &str, content_hash: &str) -> Vec<ExtractedFact> {
    let mut facts = Vec::new();
    let confidence = compute_confidence(ExtractionMethod::ConfigParse, doc.generated);

    // Determine environment from filename
    let environment = infer_environment_from_env_filename(&doc.relative_path);

    // Subject is the containing directory (module-local scope).
    // Use "." for root-level env files.
    let subject_ref = infer_module_scope(&doc.relative_path);

    // The .env file itself represents an environment surface
    facts.push(ExtractedFact {
        fact_kind: FactKind::EnvironmentSurface,
        subject_ref,
        subject_ref_kind: RefKind::Module,
        object_ref: Some(environment.clone()),
        object_ref_kind: Some(RefKind::Environment),
        source_file: doc.relative_path.clone(),
        line_start: Some(1),
        line_end: None,
        excerpt: Some(format!("environment config: {}", environment)),
        content_hash: content_hash.to_string(),
        extraction_method: ExtractionMethod::ConfigParse,
        confidence,
        generated: doc.generated,
        doc_kind: DocKind::Config,
    });

    // Optionally extract specific environment variables that indicate surfaces
    // For now, just return the file-level fact
    // Future: parse KEY=value pairs for specific patterns

    facts
}

/// Infer module scope from file path.
///
/// Returns the parent directory path, or "." for root-level files.
fn infer_module_scope(relative_path: &str) -> String {
    match relative_path.rfind('/') {
        Some(pos) => relative_path[..pos].to_string(),
        None => ".".to_string(), // root-level file
    }
}

/// Infer environment from file path.
fn infer_environment_from_path(path: &str) -> String {
    let lower = path.to_lowercase();

    if lower.contains("prod") {
        "production".to_string()
    } else if lower.contains("staging") || lower.contains("stage") {
        "staging".to_string()
    } else if lower.contains("dev") {
        "development".to_string()
    } else if lower.contains("test") {
        "test".to_string()
    } else if lower.contains("local") {
        "local".to_string()
    } else {
        "default".to_string()
    }
}

/// Infer environment from .env filename.
fn infer_environment_from_env_filename(path: &str) -> String {
    let file_name = path
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_lowercase();

    // .env.production, .env.staging, .env.development, etc.
    if let Some(suffix) = file_name.strip_prefix(".env.") {
        match suffix {
            "production" | "prod" => "production".to_string(),
            "staging" | "stage" => "staging".to_string(),
            "development" | "dev" => "development".to_string(),
            "test" | "testing" => "test".to_string(),
            "local" => "local".to_string(),
            other => other.to_string(),
        }
    } else if file_name == ".env" {
        // Plain .env is typically development/local default
        "default".to_string()
    } else {
        "unknown".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_doc(relative_path: &str) -> DocFile {
        DocFile {
            path: PathBuf::from("/test").join(relative_path),
            relative_path: relative_path.to_string(),
            kind: DocKind::Config,
            generated: false,
            content: None,
            content_hash: None,
        }
    }

    #[test]
    fn extract_docker_compose_services() {
        let doc = make_doc("docker-compose.yml");
        let content = r#"
version: '3'
services:
  api:
    image: api:latest
  worker:
    image: worker:latest
"#;

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 2);

        let service_names: Vec<_> = facts.iter().map(|f| f.subject_ref.as_str()).collect();
        assert!(service_names.contains(&"api"));
        assert!(service_names.contains(&"worker"));

        assert_eq!(facts[0].fact_kind, FactKind::EnvironmentSurface);
        assert_eq!(facts[0].extraction_method, ExtractionMethod::ConfigParse);
    }

    #[test]
    fn extract_docker_compose_production() {
        let doc = make_doc("deploy/docker-compose.production.yml");
        let content = r#"
services:
  api:
    image: api:prod
"#;

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].object_ref, Some("production".to_string()));
    }

    #[test]
    fn extract_env_file() {
        let doc = make_doc(".env.production");
        let content = "DATABASE_URL=postgres://...\nAPI_KEY=secret";

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].fact_kind, FactKind::EnvironmentSurface);
        assert_eq!(facts[0].object_ref, Some("production".to_string()));
    }

    #[test]
    fn extract_env_file_staging() {
        let doc = make_doc(".env.staging");
        let content = "DATABASE_URL=postgres://staging";

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].object_ref, Some("staging".to_string()));
    }

    #[test]
    fn extract_plain_env() {
        let doc = make_doc(".env");
        let content = "FOO=bar";

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].object_ref, Some("default".to_string()));
    }

    #[test]
    fn invalid_yaml_returns_empty() {
        let doc = make_doc("docker-compose.yml");
        let content = "this is not: valid: yaml: [";

        let facts = extract(&doc, content, "hash");
        assert!(facts.is_empty());
    }

    #[test]
    fn no_services_section_returns_empty() {
        let doc = make_doc("docker-compose.yml");
        let content = "version: '3'\nnetworks:\n  default: {}";

        let facts = extract(&doc, content, "hash");
        assert!(facts.is_empty());
    }

    #[test]
    fn extract_nested_env_file_uses_parent_scope() {
        let doc = make_doc("frontend/web/.env.prod");
        let content = "API_URL=https://prod.api.example.com";

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].fact_kind, FactKind::EnvironmentSurface);
        // Subject is the containing directory, not repo root
        assert_eq!(facts[0].subject_ref, "frontend/web");
        assert_eq!(facts[0].object_ref, Some("production".to_string()));
    }

    #[test]
    fn extract_root_env_file_uses_dot() {
        let doc = make_doc(".env.local");
        let content = "DEBUG=true";

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 1);
        // Root-level file uses "." as subject
        assert_eq!(facts[0].subject_ref, ".");
    }

    #[test]
    fn infer_module_scope_nested() {
        assert_eq!(super::infer_module_scope("a/b/c/.env"), "a/b/c");
        assert_eq!(super::infer_module_scope("serverless/.env.stage"), "serverless");
    }

    #[test]
    fn infer_module_scope_root() {
        assert_eq!(super::infer_module_scope(".env"), ".");
        assert_eq!(super::infer_module_scope(".env.production"), ".");
    }
}
