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
    /// RESOURCE-HONESTY-1: the detector-coverage statement (additive). Names which
    /// languages this build's resource-access detection covers and which
    /// materially-present languages it does NOT — so the zero-state stops blaming the
    /// codebase and a lone result stops posing as an inventory.
    pub coverage: ResourceCoverage,
}

/// RESOURCE-HONESTY-1: resource-access detector coverage for a snapshot.
///
/// `detected_languages` is build-static (from the detector registry); `material_gap`
/// is this repo's uncovered materially-present languages, or `Unknown` when the
/// per-language read failed (unknown-with-reason, never a silent empty).
#[derive(Debug, Deserialize, Clone)]
pub struct ResourceCoverage {
    /// Reader display names of the languages this build detects resource access in,
    /// sorted (e.g. `["C", "C++", "Java", "Python", "TypeScript/JavaScript"]`).
    pub detected_languages: Vec<String>,
    /// This repo's materially-present languages with no detector, or the read-failure
    /// reason.
    pub material_gap: MaterialGap,
}

/// The materiality-gap arm of [`ResourceCoverage`]: either the (possibly empty) set of
/// uncovered material languages, or an unknown-with-reason when the language read failed.
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum MaterialGap {
    /// The per-language read succeeded. `uncovered_languages` are the materially-present
    /// languages with no resource-access detector (empty = every material language covered).
    Known {
        /// Reader display names, sorted; empty when nothing material is uncovered.
        uncovered_languages: Vec<String>,
    },
    /// The per-language read failed; the gap is unknown, with the reason preserved.
    Unknown {
        /// The read-failure reason (reader sees why coverage could not be determined).
        reason: String,
    },
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
            // RESOURCE-HONESTY-1 §2.1: the zero-state names the TOOL's coverage instead of
            // blaming the codebase — which resource-access patterns this build detects and for
            // which languages (from the detector registry), plus the honest no-detector sentence
            // for materially-present languages this build cannot see.
            out.push_str("\nNo resource-access patterns detected.\n");
            out.push_str(&format!(
                "Resource-access detection on this build covers {}.\n",
                self.coverage.detected_languages_line()
            ));
            if let Some(gap) = self.coverage.gap_line() {
                out.push_str(&format!("{}\n", gap));
            }
            return out;
        }

        // RESOURCE-HONESTY-1 §2.2: the non-zero coverage header — "N resource(s) via <families>
        // (coverage: <langs>)" — so a lone result reads against a KNOWN-partial lens, never as the
        // repo's resource inventory. Families are the reader-frame kinds actually among the results.
        out.push_str(&format!(
            "\nvia {} access-call detection (coverage: {})\n",
            self.detector_families_line(),
            self.coverage.detected_languages_line()
        ));
        if let Some(gap) = self.coverage.gap_line() {
            out.push_str(&format!("{}\n", gap));
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

    /// RESOURCE-HONESTY-1 §2.2: the reader-frame detector families the detected resources were
    /// found through — the distinct resource kinds present among `results`, sorted, `/`-joined.
    /// Derived from the results themselves (single source), so it never overclaims a family the
    /// build did not actually produce here.
    fn detector_families_line(&self) -> String {
        let mut families: Vec<&'static str> = self
            .results
            .iter()
            .map(|r| resource_kind_family(&r.kind))
            .collect();
        families.sort_unstable();
        families.dedup();
        families.join("/")
    }
}

/// Reader-frame family name for a stored resource node-kind (`FS_PATH`, `DB_RESOURCE`, …).
///
/// Covers the current node-kind vocabulary (the four `ResourceKind` shapes: fs / db / blob /
/// cache-state). An unrecognized kind maps to the generic `"other"` rather than a fabricated
/// specific family — honest degradation for a vocabulary this build does not know. `STATE` is
/// the node kind the cache/state resource family shares.
fn resource_kind_family(kind: &str) -> &'static str {
    match kind {
        "FS_PATH" => "filesystem",
        "DB_RESOURCE" => "database",
        "BLOB" => "object-storage",
        "STATE" => "cache/state",
        _ => "other",
    }
}

impl ResourceCoverage {
    /// The covered-languages clause (`"C, C++, Java, Python, TypeScript/JavaScript"`), or an
    /// explicit `"(no languages)"` in the impossible empty-registry case (never fabricated).
    fn detected_languages_line(&self) -> String {
        if self.detected_languages.is_empty() {
            "(no languages)".to_string()
        } else {
            self.detected_languages.join(", ")
        }
    }

    /// The honest coverage-gap sentence, or `None` when there is nothing to add (every material
    /// language is covered). Names this repo's materially-present languages the build cannot see,
    /// or — when the language read failed — states the coverage is unknown WITH the reason.
    fn gap_line(&self) -> Option<String> {
        match &self.material_gap {
            MaterialGap::Known {
                uncovered_languages,
            } if !uncovered_languages.is_empty() => Some(format!(
                "{} code is present but has no resource-access detector on this build — \
                 resources accessed from {} are not counted.",
                uncovered_languages.join(", "),
                uncovered_languages.join(", ")
            )),
            MaterialGap::Known { .. } => None,
            MaterialGap::Unknown { reason } => Some(format!(
                "(could not determine this repo's language coverage: {})",
                reason
            )),
        }
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

    /// The real build's covered-language list, with no coverage gap (all material languages
    /// covered) — the common case for these render tests.
    fn covered_no_gap() -> ResourceCoverage {
        ResourceCoverage {
            detected_languages: vec![
                "C".to_string(),
                "C++".to_string(),
                "Java".to_string(),
                "Python".to_string(),
                "TypeScript/JavaScript".to_string(),
            ],
            material_gap: MaterialGap::Known {
                uncovered_languages: vec![],
            },
        }
    }

    fn sample_list_response() -> ResourceListResponse {
        ResourceListResponse {
            command: "resource list".to_string(),
            repo: "repo_test".to_string(),
            snapshot: "repo_test/snapshot".to_string(),
            coverage: covered_no_gap(),
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

    /// RESOURCE-HONESTY-1 §2.1: the zero-state names the TOOL's coverage — never "no resource
    /// access patterns detected in this codebase" (blaming the repo). With every material language
    /// covered it reads as an honest "genuinely none found for the covered languages".
    #[test]
    fn list_render_empty_names_coverage_not_the_codebase() {
        let resp = ResourceListResponse {
            command: "resource list".to_string(),
            repo: "repo_test".to_string(),
            snapshot: "repo_test/snapshot".to_string(),
            coverage: covered_no_gap(),
            results: vec![],
            count: 0,
            total_reads: 0,
            total_writes: 0,
        };
        let out = resp.render_human();
        assert!(out.contains("0 resources"));
        assert!(out.contains("No resource-access patterns detected."));
        assert!(
            out.contains("Resource-access detection on this build covers C, C++, Java, Python, TypeScript/JavaScript."),
            "{out}"
        );
        // The blaming sentence is GONE.
        assert!(
            !out.contains("in this codebase"),
            "must not blame the codebase: {out}"
        );
    }

    /// RESOURCE-HONESTY-1 §2.1: a Rust-dominant repo's zero-state names Rust as the uncovered
    /// material language — the measured "blames the codebase" case the slice exists to kill.
    #[test]
    fn list_render_empty_names_uncovered_material_language() {
        let resp = ResourceListResponse {
            command: "resource list".to_string(),
            repo: "repo_test".to_string(),
            snapshot: "repo_test/snapshot".to_string(),
            coverage: ResourceCoverage {
                detected_languages: covered_no_gap().detected_languages,
                material_gap: MaterialGap::Known {
                    uncovered_languages: vec!["Rust".to_string()],
                },
            },
            results: vec![],
            count: 0,
            total_reads: 0,
            total_writes: 0,
        };
        let out = resp.render_human();
        assert!(
            out.contains("Rust code is present but has no resource-access detector on this build"),
            "{out}"
        );
        assert!(
            out.contains("resources accessed from Rust are not counted."),
            "{out}"
        );
    }

    /// RESOURCE-HONESTY-1 STANDING HONESTY RULE 1: a failed language read renders
    /// unknown-with-reason in the zero-state, never a silent omission.
    #[test]
    fn list_render_empty_unknown_gap_renders_reason() {
        let resp = ResourceListResponse {
            command: "resource list".to_string(),
            repo: "repo_test".to_string(),
            snapshot: "repo_test/snapshot".to_string(),
            coverage: ResourceCoverage {
                detected_languages: covered_no_gap().detected_languages,
                material_gap: MaterialGap::Unknown {
                    reason: "db locked".to_string(),
                },
            },
            results: vec![],
            count: 0,
            total_reads: 0,
            total_writes: 0,
        };
        let out = resp.render_human();
        assert!(
            out.contains("could not determine this repo's language coverage: db locked"),
            "{out}"
        );
    }

    /// RESOURCE-HONESTY-1 §2.2: a single result carries the coverage header — "via <families>
    /// (coverage: <langs>)" — so it never reads as the repo's resource inventory.
    #[test]
    fn list_render_singular_resource_carries_coverage_header() {
        let resp = ResourceListResponse {
            command: "resource list".to_string(),
            repo: "repo_test".to_string(),
            snapshot: "repo_test/snapshot".to_string(),
            coverage: ResourceCoverage {
                detected_languages: covered_no_gap().detected_languages,
                material_gap: MaterialGap::Known {
                    uncovered_languages: vec!["Rust".to_string()],
                },
            },
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
                                                     // The anti-inventory header: family + language coverage + the uncovered-language note.
        assert!(
            out.contains("via filesystem access-call detection (coverage: C, C++, Java, Python, TypeScript/JavaScript)"),
            "{out}"
        );
        assert!(
            out.contains("Rust code is present but has no resource-access detector"),
            "{out}"
        );
    }

    /// The header names ALL distinct reader-frame families among the results, sorted.
    #[test]
    fn list_render_header_families_cover_all_result_kinds() {
        let resp = sample_list_response(); // FS_PATH + DB_RESOURCE present
        let out = resp.render_human();
        assert!(
            out.contains("via database/filesystem access-call detection (coverage:"),
            "{out}"
        );
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
