//! `rmap hook prompt-submit` command.
//!
//! Inject task-relevant context before prompt processing.
//!
//! Actions:
//! 1. Optionally classify prompt (feature/bug/refactor/validation)
//! 2. Gather targeted context if code-relevant
//! 3. Output context for injection

use std::process::ExitCode;

use serde::Serialize;

use super::env::HookContext;
use super::output::{output_result, HookResult, HookStatus, HumanReadable};

/// Prompt submit output data.
#[derive(Debug, Clone, Serialize)]
pub struct PromptSubmitOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classification: Option<String>,
    pub context: ContextInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inject: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextInfo {
    pub trust_snapshot: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub relevant_modules: Vec<String>,
    pub relevant_boundaries: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_slice: Option<String>,
}

impl HumanReadable for HookResult<PromptSubmitOutput> {
    fn print_human(&self) {
        println!("prompt-submit");

        if let Some(ref class) = self.data.classification {
            println!("  Classification: {}", class);
        }

        println!();
        println!("  Context:");
        println!("    Trust: {}", self.data.context.trust_snapshot);
        if !self.data.context.relevant_modules.is_empty() {
            println!(
                "    Relevant modules: {}",
                self.data.context.relevant_modules.join(", ")
            );
        }
        if self.data.context.relevant_boundaries > 0 {
            println!(
                "    Relevant boundaries: {}",
                self.data.context.relevant_boundaries
            );
        }
        if let Some(ref slice) = self.data.context.active_slice {
            println!("    Active slice: {}", slice);
        }

        if let Some(ref inject) = self.data.inject {
            println!();
            println!("  Inject: {}", inject);
        }

        if !self.warnings.is_empty() {
            println!();
            for warning in &self.warnings {
                eprintln!("  Warning: {}", warning);
            }
        }
    }
}

/// Run the prompt-submit hook.
pub fn run_hook_prompt_submit(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_prompt_submit_usage();
        return ExitCode::SUCCESS;
    }

    let ctx = match HookContext::from_args(args) {
        Ok(ctx) => ctx,
        Err(e) => {
            eprintln!("error: {}", e);
            print_prompt_submit_usage();
            return ExitCode::from(1);
        }
    };

    let (result, status) = execute_prompt_submit(&ctx);
    output_result(&result, ctx.json_output);
    ExitCode::from(status.exit_code())
}

fn execute_prompt_submit(ctx: &HookContext) -> (HookResult<PromptSubmitOutput>, HookStatus) {
    let mut warnings: Vec<String> = Vec::new();

    // Classify prompt if --classify flag is set and prompt text is provided
    let classification = if ctx.classify {
        ctx.prompt_text.as_ref().map(|text| classify_prompt(text))
    } else {
        None
    };

    // Try to gather real context from repo/db
    let (trust_snapshot, relevant_modules, relevant_boundaries, active_slice) =
        gather_context(ctx, &mut warnings);

    let context = ContextInfo {
        trust_snapshot,
        relevant_modules,
        relevant_boundaries,
        active_slice: active_slice.clone(),
    };

    // Build inject string for context injection
    let inject = build_inject_string(&context, classification.as_deref());

    let output = PromptSubmitOutput {
        classification,
        context,
        inject: Some(inject),
    };

    let status = if warnings.is_empty() {
        HookStatus::Ok
    } else {
        HookStatus::Warning
    };

    let result = if warnings.is_empty() {
        HookResult::ok(output)
    } else {
        HookResult::warning(output, warnings)
    };

    (result, status)
}

/// Gather actual context from the repository and database.
fn gather_context(
    ctx: &HookContext,
    warnings: &mut Vec<String>,
) -> (String, Vec<String>, u64, Option<String>) {
    let mut trust_snapshot = "unknown".to_string();
    let relevant_modules = Vec::new();
    let relevant_boundaries = 0u64;
    let mut active_slice = None;

    // Try to read CURRENT_SLICE.md from repo
    if let Some(ref repo_path) = ctx.repo_path {
        let slice_path = repo_path.join("CURRENT_SLICE.md");
        if slice_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&slice_path) {
                active_slice = extract_active_slice(&content);
            }
        }
    }

    // Try to query database for trust info
    if let Some(ref db_path) = ctx.db_path {
        if db_path.exists() {
            match crate::cli::open_storage(db_path) {
                Ok(storage) => {
                    // Get first repo and check if DB is accessible
                    if let Ok(repos) = storage.list_repos() {
                        if repos.is_empty() {
                            trust_snapshot = "no repos in database".to_string();
                        } else {
                            // Database exists and has repos - simplified trust indicator
                            trust_snapshot = "indexed".to_string();
                        }
                    }
                }
                Err(e) => {
                    warnings.push(format!("could not open database: {}", e));
                }
            }
        } else {
            trust_snapshot = "database file not found".to_string();
        }
    } else {
        trust_snapshot = "no database specified".to_string();
    }

    (
        trust_snapshot,
        relevant_modules,
        relevant_boundaries,
        active_slice,
    )
}

/// Extract active slice name from CURRENT_SLICE.md content.
fn extract_active_slice(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();

        // Skip empty lines and headers
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Look for **BOLD** text which typically indicates the active slice
        if trimmed.starts_with("**") {
            if let Some(start) = trimmed.find("**") {
                if let Some(end) = trimmed[start + 2..].find("**") {
                    let slice_text = &trimmed[start + 2..start + 2 + end];
                    // Extract just the slice ID (e.g., "HOOK-1" from "HOOK-1: rmap hook CLI Surface")
                    if let Some(colon_pos) = slice_text.find(':') {
                        return Some(slice_text[..colon_pos].trim().to_string());
                    }
                    return Some(slice_text.to_string());
                }
            }
        }
    }
    None
}

/// Simple prompt classification heuristic.
fn classify_prompt(text: &str) -> String {
    let lower = text.to_lowercase();

    if lower.contains("fix") || lower.contains("bug") || lower.contains("error") {
        "bug".to_string()
    } else if lower.contains("refactor") || lower.contains("clean") || lower.contains("reorganize")
    {
        "refactor".to_string()
    } else if lower.contains("test") || lower.contains("validate") || lower.contains("verify") {
        "validation".to_string()
    } else if lower.contains("add")
        || lower.contains("implement")
        || lower.contains("create")
        || lower.contains("new")
    {
        "feature".to_string()
    } else {
        "unknown".to_string()
    }
}

/// Build context injection string.
fn build_inject_string(context: &ContextInfo, classification: Option<&str>) -> String {
    let mut parts = Vec::new();

    parts.push(format!("Trust: {}", context.trust_snapshot));

    if !context.relevant_modules.is_empty() {
        parts.push(format!(
            "Relevant: {} modules",
            context.relevant_modules.len()
        ));
    }

    if let Some(slice) = &context.active_slice {
        parts.push(format!("Active slice: {}", slice));
    }

    if let Some(class) = classification {
        parts.push(format!("Task type: {}", class));
    }

    parts.join(". ") + "."
}

fn print_prompt_submit_usage() {
    eprintln!("usage: rmap hook prompt-submit [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --from-env          Read context from host environment variables");
    eprintln!("  --db <path>         Path to database file");
    eprintln!("  --repo <path>       Path to repository root");
    eprintln!("  --classify          Enable prompt classification");
    eprintln!("  --json              Output JSON instead of human-readable text");
    eprintln!("  --help              Show this help message");
    eprintln!();
    eprintln!("With --from-env, prompt text is read from PROMPT_TEXT (Claude Code)");
    eprintln!("or PROMPT (Codex).");
}
