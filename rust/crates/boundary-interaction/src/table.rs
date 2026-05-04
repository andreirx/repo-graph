//! Binding table schema, TOML loader, and validator.
//!
//! The binding table defines which API calls indicate boundary interactions.
//! This is the detection substrate consumed by extractors to recognize
//! IPC and device communication patterns.
//!
//! Contract: `docs/design/boundary-interaction-ipc-device.md` section 7.

use std::collections::HashSet;
use std::sync::OnceLock;

use serde::Deserialize;
use thiserror::Error;

use crate::types::{BoundaryScope, ChannelKind, Direction, InteractionBasis, InteractionPattern};

// ── Embedded binding table ────────────────────────────────────────────

/// The Slice 1A binding table, embedded at compile time.
const EMBEDDED_TABLE_TOML: &str = include_str!("../bindings.toml");

// ── Language enum ─────────────────────────────────────────────────────

/// Source language a binding applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// C source files.
    C,
    /// C++ source files.
    Cpp,
    /// Rust source files (future).
    Rust,
    /// Python source files (future).
    Python,
}

impl Language {
    /// String representation for binding keys.
    pub const fn as_str(self) -> &'static str {
        match self {
            Language::C => "c",
            Language::Cpp => "cpp",
            Language::Rust => "rust",
            Language::Python => "python",
        }
    }
}

// ── Scope heuristic ───────────────────────────────────────────────────

/// How to determine boundary scope from the API call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeHeuristic {
    /// Scope is fixed for this API (e.g., CAN is always inter_device).
    Fixed,

    /// Derive scope from socket address family argument (AF_UNIX vs AF_INET).
    ArgFamily,

    /// Derive scope from sockaddr struct contents (loopback vs remote).
    Sockaddr,

    /// Derive scope from path argument (local path = inter_process).
    Path,
}

// ── Detection role ────────────────────────────────────────────────────

/// Role of the API call in the interaction lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionRole {
    /// Channel setup (socket, shm_open with O_CREAT).
    Setup,

    /// Provider side (bind, listen, mkfifo).
    Provider,

    /// Consumer side (connect, open for read).
    Consumer,

    /// Bidirectional (mmap, serial open).
    Bidirectional,

    /// Data send operation.
    Send,

    /// Data receive operation.
    Receive,
}

impl From<DetectionRole> for Direction {
    fn from(role: DetectionRole) -> Self {
        match role {
            DetectionRole::Setup => Direction::Bidirectional,
            DetectionRole::Provider => Direction::Provider,
            DetectionRole::Consumer => Direction::Consumer,
            DetectionRole::Bidirectional => Direction::Bidirectional,
            DetectionRole::Send => Direction::Provider,
            DetectionRole::Receive => Direction::Consumer,
        }
    }
}

// ── Raw TOML shape ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct RawBindingTable {
    #[serde(default)]
    binding: Vec<RawBindingEntry>,
}

#[derive(Debug, Deserialize)]
struct RawBindingEntry {
    language: Language,
    api_family: String,
    function: String,
    role: DetectionRole,
    channel_kind: ChannelKind,
    scope_heuristic: ScopeHeuristic,
    #[serde(default)]
    fixed_scope: Option<BoundaryScope>,
    interaction_pattern: InteractionPattern,
    basis: InteractionBasis,
    #[serde(default)]
    arg_index: Option<usize>,
    notes: Option<String>,
}

// ── Validated entry ───────────────────────────────────────────────────

/// One validated binding table entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingEntry {
    /// Language this binding applies to.
    pub language: Language,

    /// API family (e.g., "posix_socket", "posix_shm", "posix_pipe").
    pub api_family: String,

    /// Function name to match (e.g., "socket", "shm_open", "mkfifo").
    pub function: String,

    /// Role of this function in the interaction.
    pub role: DetectionRole,

    /// Channel kind this function creates/uses.
    pub channel_kind: ChannelKind,

    /// How to determine scope from the call.
    pub scope_heuristic: ScopeHeuristic,

    /// Fixed scope value (when scope_heuristic is Fixed).
    pub fixed_scope: Option<BoundaryScope>,

    /// Interaction pattern for this channel type.
    pub interaction_pattern: InteractionPattern,

    /// Detection basis.
    pub basis: InteractionBasis,

    /// Argument index for path/address extraction (0-based).
    pub arg_index: Option<usize>,

    /// Optional notes.
    pub notes: Option<String>,
}

impl BindingEntry {
    /// Canonical binding key for evidence.
    /// Format: `<language>:<api_family>:<function>:<role>`
    pub fn binding_key(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.language.as_str(),
            self.api_family,
            self.function,
            match self.role {
                DetectionRole::Setup => "setup",
                DetectionRole::Provider => "provider",
                DetectionRole::Consumer => "consumer",
                DetectionRole::Bidirectional => "bidirectional",
                DetectionRole::Send => "send",
                DetectionRole::Receive => "receive",
            }
        )
    }

    /// Get the direction from this binding's role.
    pub fn direction(&self) -> Direction {
        Direction::from(self.role)
    }
}

// ── Errors ────────────────────────────────────────────────────────────

/// Binding table load/validation errors.
#[derive(Debug, Error)]
pub enum TableError {
    /// TOML parsing failed.
    #[error("binding table TOML parse failed: {0}")]
    Parse(#[from] toml::de::Error),

    /// Required field is empty.
    #[error("binding entry {index} has empty field `{field}`")]
    EmptyField {
        /// Zero-based index of the offending entry.
        index: usize,
        /// Field name.
        field: &'static str,
    },

    /// Fixed scope required but not provided.
    #[error("binding entry {index} has scope_heuristic=fixed but no fixed_scope")]
    MissingFixedScope {
        /// Zero-based index of the offending entry.
        index: usize,
    },

    /// Duplicate entry.
    #[error("duplicate binding (language={language:?}, function={function:?}) at indices {first} and {second}")]
    Duplicate {
        /// Language of the duplicate.
        language: Language,
        /// Function name of the duplicate.
        function: String,
        /// Index of the first (kept) entry.
        first: usize,
        /// Index of the duplicate entry.
        second: usize,
    },

    /// Notes too long.
    #[error("binding entry {index} notes exceeds {max} characters")]
    NotesTooLong {
        /// Zero-based index of the offending entry.
        index: usize,
        /// Maximum permitted length.
        max: usize,
    },
}

const MAX_NOTES_LEN: usize = 200;

// ── Validated table ───────────────────────────────────────────────────

/// Validated in-memory binding table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingTable {
    entries: Vec<BindingEntry>,
}

impl BindingTable {
    /// Parse and validate a binding table from TOML source.
    pub fn load_str(source: &str) -> Result<Self, TableError> {
        let raw: RawBindingTable = toml::from_str(source)?;
        let mut entries = Vec::with_capacity(raw.binding.len());
        let mut seen: HashSet<(Language, String, ChannelKind)> = HashSet::new();

        for (index, raw_entry) in raw.binding.into_iter().enumerate() {
            // Validate non-empty fields
            if raw_entry.api_family.trim().is_empty() {
                return Err(TableError::EmptyField {
                    index,
                    field: "api_family",
                });
            }
            if raw_entry.function.trim().is_empty() {
                return Err(TableError::EmptyField {
                    index,
                    field: "function",
                });
            }

            // Validate fixed scope
            if raw_entry.scope_heuristic == ScopeHeuristic::Fixed
                && raw_entry.fixed_scope.is_none()
            {
                return Err(TableError::MissingFixedScope { index });
            }

            // Validate notes length
            if let Some(notes) = &raw_entry.notes {
                if notes.len() > MAX_NOTES_LEN {
                    return Err(TableError::NotesTooLong {
                        index,
                        max: MAX_NOTES_LEN,
                    });
                }
            }

            // Check for duplicates: (language, function, channel_kind) must be unique.
            // This allows multiple bindings for the same function with different channel_kinds
            // (e.g., socket → unix_socket, socket → tcp_socket, socket → udp_socket).
            let dedup_key = (raw_entry.language, raw_entry.function.clone(), raw_entry.channel_kind);
            if !seen.insert(dedup_key.clone()) {
                let first = entries
                    .iter()
                    .position(|e: &BindingEntry| {
                        e.language == raw_entry.language
                            && e.function == raw_entry.function
                            && e.channel_kind == raw_entry.channel_kind
                    })
                    .expect("seen implies entry exists");
                return Err(TableError::Duplicate {
                    language: raw_entry.language,
                    function: raw_entry.function,
                    first,
                    second: index,
                });
            }

            entries.push(BindingEntry {
                language: raw_entry.language,
                api_family: raw_entry.api_family,
                function: raw_entry.function,
                role: raw_entry.role,
                channel_kind: raw_entry.channel_kind,
                scope_heuristic: raw_entry.scope_heuristic,
                fixed_scope: raw_entry.fixed_scope,
                interaction_pattern: raw_entry.interaction_pattern,
                basis: raw_entry.basis,
                arg_index: raw_entry.arg_index,
                notes: raw_entry.notes,
            });
        }

        Ok(BindingTable { entries })
    }

    /// Load the embedded Slice 1A binding table.
    pub fn load_embedded() -> &'static BindingTable {
        static TABLE: OnceLock<BindingTable> = OnceLock::new();
        TABLE.get_or_init(|| {
            BindingTable::load_str(EMBEDDED_TABLE_TOML)
                .expect("embedded bindings.toml should parse and validate")
        })
    }

    /// All validated entries.
    pub fn entries(&self) -> &[BindingEntry] {
        &self.entries
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Find all entries matching a function name for a given language.
    ///
    /// Returns all candidate bindings in TOML declaration order. Multiple
    /// bindings can exist for the same function when they differ by channel_kind
    /// (e.g., `socket` → `unix_socket`, `socket` → `tcp_socket`).
    ///
    /// The caller must use context-based guards to select the appropriate binding.
    pub fn find_by_function(&self, language: Language, function: &str) -> Vec<&BindingEntry> {
        self.entries
            .iter()
            .filter(|e| e.language == language && e.function == function)
            .collect()
    }

    /// Find all entries for an API family.
    pub fn find_by_family(&self, language: Language, api_family: &str) -> Vec<&BindingEntry> {
        self.entries
            .iter()
            .filter(|e| e.language == language && e.api_family == api_family)
            .collect()
    }

    /// Get all unique function names for a language (for fast lookup).
    pub fn function_names(&self, language: Language) -> HashSet<&str> {
        self.entries
            .iter()
            .filter(|e| e.language == language)
            .map(|e| e.function.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_table_parses() {
        let table = BindingTable::load_embedded();
        assert!(!table.is_empty());
    }

    #[test]
    fn binding_key_format() {
        let entry = BindingEntry {
            language: Language::C,
            api_family: "posix_socket".to_string(),
            function: "bind".to_string(),
            role: DetectionRole::Provider,
            channel_kind: ChannelKind::UnixSocket,
            scope_heuristic: ScopeHeuristic::Sockaddr,
            fixed_scope: None,
            interaction_pattern: InteractionPattern::Stream,
            basis: InteractionBasis::ApiCall,
            arg_index: Some(1),
            notes: None,
        };

        assert_eq!(entry.binding_key(), "c:posix_socket:bind:provider");
    }

    #[test]
    fn fixed_scope_requires_value() {
        let toml = r#"
[[binding]]
language = "c"
api_family = "test"
function = "test_fn"
role = "setup"
channel_kind = "unix_socket"
scope_heuristic = "fixed"
interaction_pattern = "stream"
basis = "api_call"
"#;

        let result = BindingTable::load_str(toml);
        assert!(matches!(result, Err(TableError::MissingFixedScope { .. })));
    }

    #[test]
    fn duplicate_detection() {
        // Duplicates are now detected by (language, function, channel_kind)
        let toml = r#"
[[binding]]
language = "c"
api_family = "test"
function = "same_fn"
role = "setup"
channel_kind = "unix_socket"
scope_heuristic = "path"
interaction_pattern = "stream"
basis = "api_call"

[[binding]]
language = "c"
api_family = "test"
function = "same_fn"
role = "provider"
channel_kind = "unix_socket"
scope_heuristic = "path"
interaction_pattern = "stream"
basis = "api_call"
"#;

        let result = BindingTable::load_str(toml);
        assert!(matches!(result, Err(TableError::Duplicate { .. })));
    }

    #[test]
    fn multi_binding_same_function_different_channel_kinds() {
        // Same function with different channel_kinds is allowed
        let toml = r#"
[[binding]]
language = "c"
api_family = "posix_socket"
function = "socket"
role = "setup"
channel_kind = "unix_socket"
scope_heuristic = "arg_family"
interaction_pattern = "stream"
basis = "api_call"

[[binding]]
language = "c"
api_family = "posix_socket"
function = "socket"
role = "setup"
channel_kind = "tcp_socket"
scope_heuristic = "arg_family"
interaction_pattern = "stream"
basis = "api_call"

[[binding]]
language = "c"
api_family = "posix_socket"
function = "socket"
role = "setup"
channel_kind = "udp_socket"
scope_heuristic = "arg_family"
interaction_pattern = "datagram"
basis = "api_call"
"#;

        let table = BindingTable::load_str(toml).expect("should parse");
        assert_eq!(table.len(), 3);

        // find_by_function returns all candidates
        let candidates = table.find_by_function(Language::C, "socket");
        assert_eq!(candidates.len(), 3);

        // Order is preserved (TOML declaration order)
        assert_eq!(candidates[0].channel_kind, ChannelKind::UnixSocket);
        assert_eq!(candidates[1].channel_kind, ChannelKind::TcpSocket);
        assert_eq!(candidates[2].channel_kind, ChannelKind::UdpSocket);
    }
}
