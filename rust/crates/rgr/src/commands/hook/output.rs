//! Output formatting for hook commands.
//!
//! Supports two modes:
//! - JSON: machine-readable output for programmatic consumption
//! - Human-readable: formatted text for terminal display (default)

use serde::Serialize;

/// Output a hook result in the appropriate format.
pub fn output_result<T: Serialize + HumanReadable>(result: &T, json: bool) {
    if json {
        match serde_json::to_string_pretty(result) {
            Ok(json_str) => println!("{}", json_str),
            Err(e) => eprintln!("error: failed to serialize output: {}", e),
        }
    } else {
        result.print_human();
    }
}

/// Trait for types that can be rendered as human-readable text.
pub trait HumanReadable {
    fn print_human(&self);
}

/// Generic hook result wrapper with status.
#[derive(Debug, Clone, Serialize)]
pub struct HookResult<T: Serialize> {
    pub status: HookStatus,
    #[serde(flatten)]
    pub data: T,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: Serialize> HookResult<T> {
    pub fn ok(data: T) -> Self {
        Self {
            status: HookStatus::Ok,
            data,
            warnings: Vec::new(),
            error: None,
        }
    }

    pub fn warning(data: T, warnings: Vec<String>) -> Self {
        Self {
            status: HookStatus::Warning,
            data,
            warnings,
            error: None,
        }
    }

    #[allow(dead_code)] // Future use: error result construction
    pub fn error(data: T, error: String) -> Self {
        Self {
            status: HookStatus::Error,
            data,
            warnings: Vec::new(),
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HookStatus {
    Ok,
    Warning,
    #[allow(dead_code)] // Future use: fatal error status
    Error,
}

impl HookStatus {
    pub fn exit_code(self) -> u8 {
        match self {
            HookStatus::Ok => 0,
            HookStatus::Warning => 1,
            HookStatus::Error => 2,
        }
    }
}
