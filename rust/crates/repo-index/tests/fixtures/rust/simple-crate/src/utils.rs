// Fixture: utility module for cross-file import testing.

use super::Config;

pub fn describe_config(config: &Config) -> String {
    format!("Config: {}", config.name)
}

pub fn make_default() -> Vec<String> {
    Vec::new()
}
