//! Presentation layer for resource commands.
//!
//! # CLI-OUT-5 Group 2
//!
//! Three commands, two response shapes:
//! - `resource list`: inventory of detected resources with reader/writer counts
//! - `resource readers`/`resource writers`: symbols that access a resource (same shape)
//!
//! # Output Contract
//!
//! - Deterministic ordering (by kind+name for list, by file+line for readers/writers)
//! - Full output, no truncation
//! - `--json` preserved for machine mode

use serde::Deserialize;

// ── resource list ────────────────────────────────────────────────────────────

/// Response DTO for `resource list`.
#[derive(Debug, Deserialize)]
pub struct ResourceListResponse {
    pub command: String,
    pub repo: String,
    pub snapshot: String,
    pub results: Vec<ResourceEntry>,
    pub count: usize,
    pub total_reads: usize,
    pub total_writes: usize,
}

/// Individual resource entry.
#[derive(Debug, Deserialize, Clone)]
pub struct ResourceEntry {
    pub stable_key: String,
    pub name: String,
    pub kind: String,
    pub subtype: String,
    pub readers: usize,
    pub writers: usize,
}

impl ResourceListResponse {
    /// Render human-readable output.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // Header
        out.push_str("Resources\n\n");

        // Count line
        let res_word = if self.count == 1 {
            "resource"
        } else {
            "resources"
        };
        out.push_str(&format!("{} {}\n", self.count, res_word));

        if self.count == 0 {
            out.push_str("\nhint: no resource access patterns detected in this codebase.\n");
            return out;
        }

        // Totals
        out.push_str("\nTotals:\n");
        out.push_str(&format!("  {} reads\n", self.total_reads));
        out.push_str(&format!("  {} writes\n", self.total_writes));

        // Group by kind (sorted by count desc, then kind asc)
        let mut by_kind: std::collections::BTreeMap<&str, usize> =
            std::collections::BTreeMap::new();
        for entry in &self.results {
            *by_kind.entry(&entry.kind).or_insert(0) += 1;
        }

        if !by_kind.is_empty() {
            out.push_str("\nBy kind:\n");
            let mut by_kind_vec: Vec<_> = by_kind.iter().collect();
            by_kind_vec.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
            for (kind, count) in by_kind_vec {
                out.push_str(&format!("  {}  {}\n", kind, count));
            }
        }

        // Entry list (sorted by kind, then name for determinism)
        out.push('\n');
        let mut entries = self.results.clone();
        entries.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));

        for entry in &entries {
            let readers_word = if entry.readers == 1 {
                "reader"
            } else {
                "readers"
            };
            let writers_word = if entry.writers == 1 {
                "writer"
            } else {
                "writers"
            };
            out.push_str(&format!(
                "  {}  {}  {} {}  {} {}\n",
                entry.name, entry.kind, entry.readers, readers_word, entry.writers, writers_word
            ));
        }

        // Hint
        out.push_str("\nhint: use 'rmap resource readers <key>' or 'rmap resource writers <key>' for details.\n");

        out
    }
}

// ── resource readers / writers ───────────────────────────────────────────────

/// Response DTO for `resource readers` and `resource writers`.
///
/// Both commands share the same response shape.
#[derive(Debug, Deserialize)]
pub struct ResourceAccessResponse {
    pub command: String,
    pub repo: String,
    pub snapshot: String,
    pub target: String,
    pub results: Vec<ResourceAccessor>,
    pub count: usize,
}

/// Individual accessor (reader or writer) of a resource.
#[derive(Debug, Deserialize, Clone)]
pub struct ResourceAccessor {
    #[allow(dead_code)]
    pub stable_key: String,
    pub name: String,
    pub qualified_name: String,
    #[allow(dead_code)]
    pub kind: String,
    pub subtype: String,
    pub file: String,
    pub line: u32,
    #[allow(dead_code)]
    pub column: u32,
    pub edge_type: String,
    pub resolution: String,
}

/// Direction of resource access for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessDirection {
    Readers,
    Writers,
}

impl AccessDirection {
    pub fn label(self) -> &'static str {
        match self {
            AccessDirection::Readers => "Readers",
            AccessDirection::Writers => "Writers",
        }
    }

    pub fn singular(self) -> &'static str {
        match self {
            AccessDirection::Readers => "reader",
            AccessDirection::Writers => "writer",
        }
    }

    pub fn plural(self) -> &'static str {
        match self {
            AccessDirection::Readers => "readers",
            AccessDirection::Writers => "writers",
        }
    }
}

impl ResourceAccessResponse {
    /// Render human-readable output.
    ///
    /// `direction` determines header and count wording.
    pub fn render_human(&self, direction: AccessDirection) -> String {
        let mut out = String::new();

        // Extract resource name from target key (last segment before :FS_PATH etc)
        let resource_name = extract_resource_name(&self.target);

        // Header
        out.push_str(&format!("{} for: {}\n\n", direction.label(), resource_name));

        // Count line
        let word = if self.count == 1 {
            direction.singular()
        } else {
            direction.plural()
        };
        out.push_str(&format!("{} {}\n", self.count, word));

        if self.count == 0 {
            let hint = match direction {
                AccessDirection::Readers => "No code reads this resource.",
                AccessDirection::Writers => "No code writes to this resource.",
            };
            out.push_str(&format!("\n{}\n", hint));
            return out;
        }

        // Accessor list (sorted by file, then line for determinism)
        out.push('\n');
        let mut accessors = self.results.clone();
        accessors.sort_by(|a, b| a.file.cmp(&b.file).then_with(|| a.line.cmp(&b.line)));

        for acc in &accessors {
            out.push_str(&format!(
                "  {}  {}:{}  {}  {}\n",
                acc.name, acc.file, acc.line, acc.subtype, acc.resolution
            ));
        }

        out
    }
}

/// Extract human-readable resource name from stable key.
///
/// Key format: `repo_xxx:fs:filename.ext:FS_PATH`
/// Returns: `filename.ext`
fn extract_resource_name(key: &str) -> &str {
    // Find the last colon-separated segment before the kind suffix
    let parts: Vec<&str> = key.split(':').collect();
    if parts.len() >= 3 {
        // Format is typically: repo:fs:name:KIND
        // We want the name part (index -2 from end)
        parts.get(parts.len() - 2).copied().unwrap_or(key)
    } else {
        key
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── resource list tests ──────────────────────────────────────────────────

    fn sample_list_response() -> ResourceListResponse {
        ResourceListResponse {
            command: "resource list".to_string(),
            repo: "repo_test".to_string(),
            snapshot: "repo_test/snapshot".to_string(),
            results: vec![
                ResourceEntry {
                    stable_key: "repo:fs:config.json:FS_PATH".to_string(),
                    name: "config.json".to_string(),
                    kind: "FS_PATH".to_string(),
                    subtype: "FILE_PATH".to_string(),
                    readers: 5,
                    writers: 2,
                },
                ResourceEntry {
                    stable_key: "repo:fs:data.db:FS_PATH".to_string(),
                    name: "data.db".to_string(),
                    kind: "FS_PATH".to_string(),
                    subtype: "FILE_PATH".to_string(),
                    readers: 3,
                    writers: 1,
                },
                ResourceEntry {
                    stable_key: "repo:db:users:DB_RESOURCE".to_string(),
                    name: "users".to_string(),
                    kind: "DB_RESOURCE".to_string(),
                    subtype: "TABLE".to_string(),
                    readers: 10,
                    writers: 4,
                },
            ],
            count: 3,
            total_reads: 18,
            total_writes: 7,
        }
    }

    #[test]
    fn list_render_shows_header() {
        let resp = sample_list_response();
        let out = resp.render_human();
        assert!(out.starts_with("Resources\n"));
    }

    #[test]
    fn list_render_shows_count() {
        let resp = sample_list_response();
        let out = resp.render_human();
        assert!(out.contains("3 resources"));
    }

    #[test]
    fn list_render_shows_totals() {
        let resp = sample_list_response();
        let out = resp.render_human();
        assert!(out.contains("Totals:"));
        assert!(out.contains("18 reads"));
        assert!(out.contains("7 writes"));
    }

    #[test]
    fn list_render_shows_by_kind() {
        let resp = sample_list_response();
        let out = resp.render_human();
        assert!(out.contains("By kind:"));
        assert!(out.contains("FS_PATH  2"));
        assert!(out.contains("DB_RESOURCE  1"));
    }

    #[test]
    fn list_render_shows_entries_sorted() {
        let resp = sample_list_response();
        let out = resp.render_human();
        // Should be sorted by kind, then name
        // DB_RESOURCE comes before FS_PATH alphabetically
        let db_pos = out.find("users  DB_RESOURCE").unwrap();
        let config_pos = out.find("config.json  FS_PATH").unwrap();
        let data_pos = out.find("data.db  FS_PATH").unwrap();
        assert!(db_pos < config_pos);
        assert!(config_pos < data_pos);
    }

    #[test]
    fn list_render_shows_reader_writer_counts() {
        let resp = sample_list_response();
        let out = resp.render_human();
        assert!(out.contains("5 readers  2 writers"));
        assert!(out.contains("10 readers  4 writers"));
    }

    #[test]
    fn list_render_shows_hint() {
        let resp = sample_list_response();
        let out = resp.render_human();
        assert!(out.contains("hint: use 'rmap resource readers <key>'"));
    }

    #[test]
    fn list_render_empty_shows_hint() {
        let resp = ResourceListResponse {
            command: "resource list".to_string(),
            repo: "repo_test".to_string(),
            snapshot: "repo_test/snapshot".to_string(),
            results: vec![],
            count: 0,
            total_reads: 0,
            total_writes: 0,
        };
        let out = resp.render_human();
        assert!(out.contains("0 resources"));
        assert!(out.contains("hint: no resource access patterns detected"));
    }

    #[test]
    fn list_render_singular_resource() {
        let resp = ResourceListResponse {
            command: "resource list".to_string(),
            repo: "repo_test".to_string(),
            snapshot: "repo_test/snapshot".to_string(),
            results: vec![ResourceEntry {
                stable_key: "repo:fs:file.txt:FS_PATH".to_string(),
                name: "file.txt".to_string(),
                kind: "FS_PATH".to_string(),
                subtype: "FILE_PATH".to_string(),
                readers: 1,
                writers: 1,
            }],
            count: 1,
            total_reads: 1,
            total_writes: 1,
        };
        let out = resp.render_human();
        assert!(out.contains("1 resource\n")); // singular
        assert!(out.contains("1 reader  1 writer")); // singular
    }

    // ── resource readers/writers tests ───────────────────────────────────────

    fn sample_access_response() -> ResourceAccessResponse {
        ResourceAccessResponse {
            command: "resource writers".to_string(),
            repo: "repo_test".to_string(),
            snapshot: "repo_test/snapshot".to_string(),
            target: "repo_test:fs:config.json:FS_PATH".to_string(),
            results: vec![
                ResourceAccessor {
                    stable_key: "repo:src/main.ts#saveConfig:SYMBOL:FUNCTION".to_string(),
                    name: "saveConfig".to_string(),
                    qualified_name: "saveConfig".to_string(),
                    kind: "SYMBOL".to_string(),
                    subtype: "FUNCTION".to_string(),
                    file: "src/main.ts".to_string(),
                    line: 42,
                    column: 0,
                    edge_type: "WRITES".to_string(),
                    resolution: "static".to_string(),
                },
                ResourceAccessor {
                    stable_key: "repo:src/config.ts#updateConfig:SYMBOL:FUNCTION".to_string(),
                    name: "updateConfig".to_string(),
                    qualified_name: "updateConfig".to_string(),
                    kind: "SYMBOL".to_string(),
                    subtype: "FUNCTION".to_string(),
                    file: "src/config.ts".to_string(),
                    line: 15,
                    column: 0,
                    edge_type: "WRITES".to_string(),
                    resolution: "static".to_string(),
                },
            ],
            count: 2,
        }
    }

    #[test]
    fn access_render_shows_header_writers() {
        let resp = sample_access_response();
        let out = resp.render_human(AccessDirection::Writers);
        assert!(out.contains("Writers for: config.json"));
    }

    #[test]
    fn access_render_shows_header_readers() {
        let resp = sample_access_response();
        let out = resp.render_human(AccessDirection::Readers);
        assert!(out.contains("Readers for: config.json"));
    }

    #[test]
    fn access_render_shows_count() {
        let resp = sample_access_response();
        let out = resp.render_human(AccessDirection::Writers);
        assert!(out.contains("2 writers"));
    }

    #[test]
    fn access_render_shows_accessors_sorted() {
        let resp = sample_access_response();
        let out = resp.render_human(AccessDirection::Writers);
        // Should be sorted by file, then line
        // src/config.ts:15 comes before src/main.ts:42
        let config_pos = out.find("updateConfig  src/config.ts:15").unwrap();
        let main_pos = out.find("saveConfig  src/main.ts:42").unwrap();
        assert!(config_pos < main_pos);
    }

    #[test]
    fn access_render_shows_accessor_details() {
        let resp = sample_access_response();
        let out = resp.render_human(AccessDirection::Writers);
        assert!(out.contains("saveConfig  src/main.ts:42  FUNCTION  static"));
    }

    #[test]
    fn access_render_empty_writers() {
        let resp = ResourceAccessResponse {
            command: "resource writers".to_string(),
            repo: "repo_test".to_string(),
            snapshot: "repo_test/snapshot".to_string(),
            target: "repo_test:fs:readonly.txt:FS_PATH".to_string(),
            results: vec![],
            count: 0,
        };
        let out = resp.render_human(AccessDirection::Writers);
        assert!(out.contains("0 writers"));
        assert!(out.contains("No code writes to this resource."));
    }

    #[test]
    fn access_render_empty_readers() {
        let resp = ResourceAccessResponse {
            command: "resource readers".to_string(),
            repo: "repo_test".to_string(),
            snapshot: "repo_test/snapshot".to_string(),
            target: "repo_test:fs:writeonly.txt:FS_PATH".to_string(),
            results: vec![],
            count: 0,
        };
        let out = resp.render_human(AccessDirection::Readers);
        assert!(out.contains("0 readers"));
        assert!(out.contains("No code reads this resource."));
    }

    #[test]
    fn access_render_singular_writer() {
        let resp = ResourceAccessResponse {
            command: "resource writers".to_string(),
            repo: "repo_test".to_string(),
            snapshot: "repo_test/snapshot".to_string(),
            target: "repo_test:fs:file.txt:FS_PATH".to_string(),
            results: vec![ResourceAccessor {
                stable_key: "repo:src/main.ts#write:SYMBOL:FUNCTION".to_string(),
                name: "write".to_string(),
                qualified_name: "write".to_string(),
                kind: "SYMBOL".to_string(),
                subtype: "FUNCTION".to_string(),
                file: "src/main.ts".to_string(),
                line: 10,
                column: 0,
                edge_type: "WRITES".to_string(),
                resolution: "static".to_string(),
            }],
            count: 1,
        };
        let out = resp.render_human(AccessDirection::Writers);
        assert!(out.contains("1 writer\n")); // singular
    }

    #[test]
    fn extract_resource_name_works() {
        assert_eq!(
            extract_resource_name("repo_xxx:fs:config.json:FS_PATH"),
            "config.json"
        );
        assert_eq!(
            extract_resource_name("repo_xxx:db:users:DB_RESOURCE"),
            "users"
        );
    }
}
