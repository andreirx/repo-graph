//! Toolchain-staleness DTO and the pure snapshot-stamp comparison (TOOLCHAIN-STALENESS-1,
//! RG-REQ-001-L06, D-STALE-SIGNAL-1).
//!
//! Every full index and refresh writes a snapshot TOOLCHAIN STAMP (`toolchain_json`:
//! `{"extractors":["ts-core:0.2.0",…],"indexer":"indexer:1.0.0"}`). The stamp names the toolchain
//! of the write that created the snapshot; after a refresh it does NOT assert that this toolchain
//! produced every copied-forward fact (L06). So this module compares STAMPS — the served snapshot's
//! stamp with the running rmap's stamp — and never says which rmap produced a fact.
//!
//! [`compare_toolchain_stamps`] is pure policy (text in, [`StampComparison`] out). The daemon
//! (composition root) reads the stamp, owns the re-index state, and builds a [`ToolchainStaleness`];
//! rgr deserializes and renders it. The line is a signal, never a `check` condition.
//!
//! JSON (`value.toolchain_staleness` on `orient` and `check`):
//! - `{"state":"current"}`
//! - `{"state":"stale","differences":[{"component","snapshot_version","current_version"}…],"reindex":…}`
//! - `{"state":"unknown","reason":…,"reindex":…}`
//!
//! with `reindex` ∈ `queued | running | disabled | failed`, `reindex_failure` present iff `failed`,
//! and `reindex_disabled_reason` present iff `disabled`. No shape carries a repository path.

use serde::{Deserialize, Serialize};

/// The served snapshot's stamp as the daemon read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotStamp<'a> {
    /// The snapshot row's `toolchain_json` is NULL.
    Missing,
    /// The snapshot row's `toolchain_json` text.
    Present(&'a str),
    /// The stamp could not be read; carries the read error.
    ReadFailed(&'a str),
}

/// One component whose version differs between the snapshot stamp and the running rmap. A side
/// that does not name the component is `None` (JSON `null`, rendered `absent`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentDifference {
    pub component: String,
    pub snapshot_version: Option<String>,
    pub current_version: Option<String>,
}

/// The outcome of comparing the snapshot stamp with the running stamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StampComparison {
    /// The same component set with equal version strings.
    Current,
    /// At least one component differs (non-empty).
    Differs(Vec<ComponentDifference>),
    /// The stamp is missing, malformed, incomplete or could not be read — never current.
    Unreadable(String),
}

/// The automatic re-index state for a stale or unknown stamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReindexState {
    Queued,
    Running,
    /// No automatic re-index will run in this process; carries why (the opt-out or the stdio
    /// transport).
    Disabled {
        reason: String,
    },
    /// This process's automatic re-index failed; carries the failure. Not retried in the process.
    Failed {
        failure: String,
    },
}

/// The toolchain status `orient` and `check` attach. A closed sum; consumers match exhaustively.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ToolchainStalenessWire", into = "ToolchainStalenessWire")]
pub enum ToolchainStaleness {
    Current,
    Stale {
        differences: Vec<ComponentDifference>,
        reindex: ReindexState,
    },
    Unknown {
        reason: String,
        reindex: ReindexState,
    },
}

impl ToolchainStaleness {
    /// Compose the status from a stamp comparison and the re-index state (the latter is ignored
    /// when the stamp is current).
    pub fn from_comparison(comparison: StampComparison, reindex: ReindexState) -> Self {
        match comparison {
            StampComparison::Current => ToolchainStaleness::Current,
            StampComparison::Differs(differences) => ToolchainStaleness::Stale {
                differences,
                reindex,
            },
            StampComparison::Unreadable(reason) => ToolchainStaleness::Unknown { reason, reindex },
        }
    }
}

/// Compare the served snapshot's toolchain stamp with the running rmap's stamp (LD-02).
///
/// A stamp is readable only if it is a JSON object whose `extractors` is an array of strings and
/// whose `indexer` is a string, each `<name>:<version>` with a non-empty name and version and the
/// names unique; other keys are ignored. Components are compared by name (the text before the first
/// `:`) and versions as strings. Order of differences: running components in stamp order (the
/// extractors, then the indexer), then stamp-only components in stamp order. Every missing,
/// malformed, incomplete or unreadable stamp is [`StampComparison::Unreadable`] — never current.
pub fn compare_toolchain_stamps(snapshot: SnapshotStamp<'_>, running: &str) -> StampComparison {
    let text = match snapshot {
        SnapshotStamp::Missing => {
            return StampComparison::Unreadable("this index has no toolchain stamp".to_string())
        }
        SnapshotStamp::ReadFailed(error) => {
            return StampComparison::Unreadable(format!(
                "could not read this index's toolchain stamp: {error}"
            ))
        }
        SnapshotStamp::Present(text) => text,
    };
    let stamped = match parse_stamp(text) {
        Ok(components) => components,
        Err(StampDefect::Unreadable) => {
            return StampComparison::Unreadable(
                "this index's toolchain stamp is unreadable".to_string(),
            )
        }
        Err(StampDefect::Incomplete(what)) => {
            return StampComparison::Unreadable(format!(
                "this index's toolchain stamp is incomplete ({what})"
            ))
        }
    };
    let current = match parse_stamp(running) {
        Ok(components) => components,
        // The running stamp is built from the composed extractor set and cannot be defective in a
        // correct build; if it ever is, the status is unknown, never current.
        Err(_) => {
            return StampComparison::Unreadable(
                "the running rmap's toolchain stamp is unreadable".to_string(),
            )
        }
    };
    let version_in = |set: &[Component], name: &str| {
        set.iter()
            .find(|c| c.name == name)
            .map(|c| c.version.clone())
    };
    let mut differences = Vec::new();
    for c in &current {
        let snapshot_version = version_in(&stamped, &c.name);
        if snapshot_version.as_deref() != Some(c.version.as_str()) {
            differences.push(ComponentDifference {
                component: c.name.clone(),
                snapshot_version,
                current_version: Some(c.version.clone()),
            });
        }
    }
    for c in &stamped {
        if version_in(&current, &c.name).is_none() {
            differences.push(ComponentDifference {
                component: c.name.clone(),
                snapshot_version: Some(c.version.clone()),
                current_version: None,
            });
        }
    }
    if differences.is_empty() {
        StampComparison::Current
    } else {
        StampComparison::Differs(differences)
    }
}

/// One `<name>:<version>` entry of a readable stamp.
struct Component {
    name: String,
    version: String,
}

/// Why a stamp text is not a readable stamp.
enum StampDefect {
    /// Not JSON, or not the stamp's shape.
    Unreadable,
    /// The stamp's shape with a required part missing; names the part.
    Incomplete(&'static str),
}

/// Parse a stamp into its components in stamp order (extractors, then the indexer).
fn parse_stamp(text: &str) -> Result<Vec<Component>, StampDefect> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| StampDefect::Unreadable)?;
    let object = value.as_object().ok_or(StampDefect::Unreadable)?;
    let extractors = match object.get("extractors") {
        None => return Err(StampDefect::Incomplete("no extractor list")),
        Some(list) => list.as_array().ok_or(StampDefect::Unreadable)?,
    };
    let indexer = match object.get("indexer") {
        None => return Err(StampDefect::Incomplete("no indexer version")),
        Some(entry) => entry.as_str().ok_or(StampDefect::Unreadable)?,
    };
    let mut components = Vec::with_capacity(extractors.len() + 1);
    for entry in extractors {
        let entry = entry.as_str().ok_or(StampDefect::Unreadable)?;
        let component = split_component(entry, "an extractor entry without a version")?;
        if components
            .iter()
            .any(|c: &Component| c.name == component.name)
        {
            return Err(StampDefect::Incomplete("a duplicated extractor"));
        }
        components.push(component);
    }
    let indexer = split_component(indexer, "no indexer version")?;
    if components.iter().any(|c| c.name == indexer.name) {
        return Err(StampDefect::Incomplete("a duplicated extractor"));
    }
    components.push(indexer);
    Ok(components)
}

/// Split `<name>:<version>` at the first `:`; an empty name is unreadable, a missing or empty
/// version is the named incompleteness.
fn split_component(entry: &str, missing_version: &'static str) -> Result<Component, StampDefect> {
    let (name, version) = match entry.split_once(':') {
        Some((name, version)) => (name, version),
        None => (entry, ""),
    };
    if name.is_empty() {
        return Err(StampDefect::Unreadable);
    }
    if version.is_empty() {
        return Err(StampDefect::Incomplete(missing_version));
    }
    Ok(Component {
        name: name.to_string(),
        version: version.to_string(),
    })
}

/// The wire shape (field order = JSON order). Private: only the serde conversions use it.
#[derive(Clone, Serialize, Deserialize)]
struct ToolchainStalenessWire {
    state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    differences: Option<Vec<ComponentDifference>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reindex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reindex_failure: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reindex_disabled_reason: Option<String>,
}

impl ToolchainStalenessWire {
    fn empty(state: &str) -> Self {
        Self {
            state: state.to_string(),
            differences: None,
            reason: None,
            reindex: None,
            reindex_failure: None,
            reindex_disabled_reason: None,
        }
    }

    fn with_reindex(mut self, reindex: ReindexState) -> Self {
        let token = match reindex {
            ReindexState::Queued => "queued",
            ReindexState::Running => "running",
            ReindexState::Disabled { reason } => {
                self.reindex_disabled_reason = Some(reason);
                "disabled"
            }
            ReindexState::Failed { failure } => {
                self.reindex_failure = Some(failure);
                "failed"
            }
        };
        self.reindex = Some(token.to_string());
        self
    }

    fn reindex_state(&mut self) -> Result<ReindexState, String> {
        let failure = self.reindex_failure.take();
        let disabled = self.reindex_disabled_reason.take();
        match (self.reindex.as_deref(), failure, disabled) {
            (Some("queued"), None, None) => Ok(ReindexState::Queued),
            (Some("running"), None, None) => Ok(ReindexState::Running),
            (Some("disabled"), None, Some(reason)) => Ok(ReindexState::Disabled { reason }),
            (Some("failed"), Some(failure), None) => Ok(ReindexState::Failed { failure }),
            (reindex, failure, disabled) => Err(format!(
                "invalid toolchain_staleness re-index state: reindex {reindex:?}, \
                 reindex_failure present {}, reindex_disabled_reason present {}",
                failure.is_some(),
                disabled.is_some()
            )),
        }
    }
}

impl From<ToolchainStaleness> for ToolchainStalenessWire {
    fn from(v: ToolchainStaleness) -> Self {
        match v {
            ToolchainStaleness::Current => Self::empty("current"),
            ToolchainStaleness::Stale {
                differences,
                reindex,
            } => {
                let mut w = Self::empty("stale");
                w.differences = Some(differences);
                w.with_reindex(reindex)
            }
            ToolchainStaleness::Unknown { reason, reindex } => {
                let mut w = Self::empty("unknown");
                w.reason = Some(reason);
                w.with_reindex(reindex)
            }
        }
    }
}

impl TryFrom<ToolchainStalenessWire> for ToolchainStaleness {
    type Error = String;
    fn try_from(mut w: ToolchainStalenessWire) -> Result<Self, String> {
        match w.state.as_str() {
            "current" => match (
                &w.differences,
                &w.reason,
                &w.reindex,
                &w.reindex_failure,
                &w.reindex_disabled_reason,
            ) {
                (None, None, None, None, None) => Ok(ToolchainStaleness::Current),
                _ => Err("a current toolchain_staleness carries no other member".to_string()),
            },
            "stale" => {
                if w.reason.is_some() {
                    return Err("a stale toolchain_staleness carries no reason".to_string());
                }
                let differences = match w.differences.take() {
                    Some(d) if !d.is_empty() => d,
                    _ => {
                        return Err(
                            "a stale toolchain_staleness needs a non-empty differences list"
                                .to_string(),
                        )
                    }
                };
                let reindex = w.reindex_state()?;
                Ok(ToolchainStaleness::Stale {
                    differences,
                    reindex,
                })
            }
            "unknown" => {
                if w.differences.is_some() {
                    return Err("an unknown toolchain_staleness carries no differences".to_string());
                }
                let reason = w
                    .reason
                    .take()
                    .ok_or("an unknown toolchain_staleness needs a reason")?;
                let reindex = w.reindex_state()?;
                Ok(ToolchainStaleness::Unknown { reason, reindex })
            }
            other => Err(format!("unknown toolchain_staleness state {other:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeSet;

    const RUNNING: &str = r#"{"extractors":["ts-core:0.2.0","c-core:0.1.0","cpp-core:0.1.0","java-core:0.1.0","python-core:0.1.0","rust-core:0.2.0"],"indexer":"indexer:1.0.0"}"#;

    fn diff(c: &str, s: Option<&str>, r: Option<&str>) -> ComponentDifference {
        ComponentDifference {
            component: c.to_string(),
            snapshot_version: s.map(str::to_string),
            current_version: r.map(str::to_string),
        }
    }

    fn assert_unreadable(cmp: StampComparison, expected_reason: &str) {
        match cmp {
            StampComparison::Unreadable(reason) => assert_eq!(reason, expected_reason),
            other => panic!("expected Unreadable({expected_reason:?}), got {other:?}"),
        }
    }

    #[test]
    fn matching_stamp_compares_current() {
        assert_eq!(
            compare_toolchain_stamps(SnapshotStamp::Present(RUNNING), RUNNING),
            StampComparison::Current
        );
        // Key order and whitespace are not part of the stamp's meaning.
        let reordered = r#"{ "indexer": "indexer:1.0.0", "extractors": ["ts-core:0.2.0","c-core:0.1.0","cpp-core:0.1.0","java-core:0.1.0","python-core:0.1.0","rust-core:0.2.0"] }"#;
        assert_eq!(
            compare_toolchain_stamps(SnapshotStamp::Present(reordered), RUNNING),
            StampComparison::Current
        );
    }

    #[test]
    fn differing_extractor_version_is_a_named_difference() {
        let seeded = RUNNING.replace("cpp-core:0.1.0", "cpp-core:0.0.0");
        assert_eq!(
            compare_toolchain_stamps(SnapshotStamp::Present(&seeded), RUNNING),
            StampComparison::Differs(vec![diff("cpp-core", Some("0.0.0"), Some("0.1.0"))])
        );
        // Versions are compared as strings: a NEWER stamp also "differs" (never "older").
        let newer = RUNNING
            .replace("ts-core:0.2.0", "ts-core:0.9.0")
            .replace("rust-core:0.2.0", "rust-core:0.3.0");
        assert_eq!(
            compare_toolchain_stamps(SnapshotStamp::Present(&newer), RUNNING),
            StampComparison::Differs(vec![
                diff("ts-core", Some("0.9.0"), Some("0.2.0")),
                diff("rust-core", Some("0.3.0"), Some("0.2.0")),
            ])
        );
    }

    #[test]
    fn differing_indexer_version_is_a_named_difference() {
        let seeded = RUNNING.replace("indexer:1.0.0", "indexer:0.9.0");
        assert_eq!(
            compare_toolchain_stamps(SnapshotStamp::Present(&seeded), RUNNING),
            StampComparison::Differs(vec![diff("indexer", Some("0.9.0"), Some("1.0.0"))])
        );
    }

    #[test]
    fn component_present_on_one_side_only_is_a_difference_with_the_missing_side_null() {
        // The snapshot lacks java-core and names an extractor the running rmap does not have.
        let stamp = r#"{"extractors":["ts-core:0.2.0","c-core:0.1.0","cpp-core:0.1.0","kotlin-core:0.1.0","python-core:0.1.0","rust-core:0.2.0"],"indexer":"indexer:1.0.0"}"#;
        let cmp = compare_toolchain_stamps(SnapshotStamp::Present(stamp), RUNNING);
        // Running components first (stamp order), then stamp-only components.
        assert_eq!(
            cmp,
            StampComparison::Differs(vec![
                diff("java-core", None, Some("0.1.0")),
                diff("kotlin-core", Some("0.1.0"), None),
            ])
        );
        let status = ToolchainStaleness::from_comparison(cmp, ReindexState::Queued);
        let v = serde_json::to_value(&status).unwrap();
        assert_eq!(
            v["differences"],
            json!([
                {"component":"java-core","snapshot_version":null,"current_version":"0.1.0"},
                {"component":"kotlin-core","snapshot_version":"0.1.0","current_version":null}
            ])
        );
    }

    #[test]
    fn missing_stamp_is_unreadable_with_reason() {
        assert_unreadable(
            compare_toolchain_stamps(SnapshotStamp::Missing, RUNNING),
            "this index has no toolchain stamp",
        );
        assert_unreadable(
            compare_toolchain_stamps(SnapshotStamp::ReadFailed("database is locked"), RUNNING),
            "could not read this index's toolchain stamp: database is locked",
        );
    }

    #[test]
    fn malformed_stamp_is_unreadable_with_reason() {
        for bad in [
            "",
            "not json",
            "[]",
            "\"indexer:1.0.0\"",
            r#"{"extractors":"ts-core:0.2.0","indexer":"indexer:1.0.0"}"#,
            r#"{"extractors":[1,2],"indexer":"indexer:1.0.0"}"#,
            r#"{"extractors":["ts-core:0.2.0"],"indexer":7}"#,
            r#"{"extractors":[":0.2.0"],"indexer":"indexer:1.0.0"}"#,
        ] {
            assert_unreadable(
                compare_toolchain_stamps(SnapshotStamp::Present(bad), RUNNING),
                "this index's toolchain stamp is unreadable",
            );
        }
    }

    #[test]
    fn structurally_incomplete_stamp_is_unreadable_naming_what_is_missing() {
        let cases = [
            (
                r#"{"extractors":["ts-core:0.2.0"]}"#,
                "this index's toolchain stamp is incomplete (no indexer version)",
            ),
            (
                r#"{"extractors":["ts-core:0.2.0"],"indexer":"indexer"}"#,
                "this index's toolchain stamp is incomplete (no indexer version)",
            ),
            (
                r#"{"indexer":"indexer:1.0.0"}"#,
                "this index's toolchain stamp is incomplete (no extractor list)",
            ),
            (
                r#"{"extractors":["ts-core:0.2.0","cpp-core"],"indexer":"indexer:1.0.0"}"#,
                "this index's toolchain stamp is incomplete (an extractor entry without a version)",
            ),
            (
                r#"{"extractors":["ts-core:"],"indexer":"indexer:1.0.0"}"#,
                "this index's toolchain stamp is incomplete (an extractor entry without a version)",
            ),
            (
                r#"{"extractors":["ts-core:0.2.0","ts-core:0.2.0"],"indexer":"indexer:1.0.0"}"#,
                "this index's toolchain stamp is incomplete (a duplicated extractor)",
            ),
        ];
        for (stamp, reason) in cases {
            let cmp = compare_toolchain_stamps(SnapshotStamp::Present(stamp), RUNNING);
            assert_ne!(cmp, StampComparison::Current, "{stamp}");
            assert_unreadable(cmp, reason);
        }
        // No reason names which rmap built the index — the reasons speak of the stamp only.
        for (_, reason) in cases {
            assert!(!reason.contains("built"), "{reason}");
        }
    }

    #[test]
    fn current_serializes_as_exactly_state_current() {
        let v = serde_json::to_value(ToolchainStaleness::Current).unwrap();
        assert_eq!(v, json!({"state":"current"}));
        // A current comparison ignores the re-index state.
        let s = ToolchainStaleness::from_comparison(
            StampComparison::Current,
            ReindexState::Failed {
                failure: "boom".to_string(),
            },
        );
        assert_eq!(s, ToolchainStaleness::Current);
    }

    #[test]
    fn stale_and_unknown_carry_reindex_and_carry_reindex_failure_iff_failed() {
        let one = vec![diff("cpp-core", Some("0.0.0"), Some("0.1.0"))];
        let states = [
            (ReindexState::Queued, "queued"),
            (ReindexState::Running, "running"),
            (
                ReindexState::Disabled {
                    reason: "RMAP_AUTO_REINDEX=off".to_string(),
                },
                "disabled",
            ),
            (
                ReindexState::Failed {
                    failure: "repo_path does not exist".to_string(),
                },
                "failed",
            ),
        ];
        for (state, token) in states {
            let stale = ToolchainStaleness::from_comparison(
                StampComparison::Differs(one.clone()),
                state.clone(),
            );
            let unknown = ToolchainStaleness::from_comparison(
                StampComparison::Unreadable("this index has no toolchain stamp".to_string()),
                state.clone(),
            );
            let sv = serde_json::to_value(&stale).unwrap();
            let uv = serde_json::to_value(&unknown).unwrap();
            assert_eq!(sv["state"], "stale");
            assert_eq!(uv["state"], "unknown");
            assert_eq!(uv["reason"], "this index has no toolchain stamp");
            for v in [&sv, &uv] {
                assert_eq!(v["reindex"], token);
                assert_eq!(v.get("reindex_failure").is_some(), token == "failed", "{v}");
            }
            if token == "failed" {
                assert_eq!(sv["reindex_failure"], "repo_path does not exist");
            }
        }
        // `reindex_failure` without `failed` (or `failed` without it) does not decode.
        for bad in [
            json!({"state":"stale","differences":[{"component":"indexer","snapshot_version":"0","current_version":"1"}],"reindex":"queued","reindex_failure":"x"}),
            json!({"state":"unknown","reason":"r","reindex":"failed"}),
            json!({"state":"stale","differences":[],"reindex":"queued"}),
            json!({"state":"stale","differences":[{"component":"indexer","snapshot_version":"0","current_version":"1"}]}),
        ] {
            assert!(
                serde_json::from_value::<ToolchainStaleness>(bad.clone()).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn reindex_disabled_reason_is_present_iff_disabled() {
        for reason in [
            "RMAP_AUTO_REINDEX=off",
            "stdio transport \u{2014} a background re-index cannot outlive the request",
        ] {
            let s = ToolchainStaleness::from_comparison(
                StampComparison::Differs(vec![diff("cpp-core", Some("0.0.0"), Some("0.1.0"))]),
                ReindexState::Disabled {
                    reason: reason.to_string(),
                },
            );
            let v = serde_json::to_value(&s).unwrap();
            assert_eq!(v["reindex"], "disabled");
            assert_eq!(v["reindex_disabled_reason"], reason);
        }
        for state in [
            ReindexState::Queued,
            ReindexState::Running,
            ReindexState::Failed {
                failure: "x".to_string(),
            },
        ] {
            let s = ToolchainStaleness::from_comparison(
                StampComparison::Unreadable("r".to_string()),
                state,
            );
            let v = serde_json::to_value(&s).unwrap();
            assert!(v.get("reindex_disabled_reason").is_none(), "{v}");
        }
        assert!(serde_json::from_value::<ToolchainStaleness>(
            json!({"state":"unknown","reason":"r","reindex":"disabled"})
        )
        .is_err());
        assert!(serde_json::from_value::<ToolchainStaleness>(
            json!({"state":"unknown","reason":"r","reindex":"queued","reindex_disabled_reason":"x"})
        )
        .is_err());
    }

    #[test]
    fn toolchain_staleness_round_trips_every_shape() {
        let one = vec![diff("cpp-core", Some("0.0.0"), Some("0.1.0"))];
        let mut shapes = vec![ToolchainStaleness::Current];
        for state in [
            ReindexState::Queued,
            ReindexState::Running,
            ReindexState::Disabled {
                reason: "RMAP_AUTO_REINDEX=off".to_string(),
            },
            ReindexState::Failed {
                failure: "boom".to_string(),
            },
        ] {
            shapes.push(ToolchainStaleness::Stale {
                differences: one.clone(),
                reindex: state.clone(),
            });
            shapes.push(ToolchainStaleness::Unknown {
                reason: "this index's toolchain stamp is unreadable".to_string(),
                reindex: state,
            });
        }
        let allowed: BTreeSet<&str> = [
            "state",
            "differences",
            "reason",
            "reindex",
            "reindex_failure",
            "reindex_disabled_reason",
        ]
        .into_iter()
        .collect();
        for s in shapes {
            let text = serde_json::to_string(&s).unwrap();
            let back: ToolchainStaleness = serde_json::from_str(&text).unwrap();
            assert_eq!(back, s, "{text}");
            let v: serde_json::Value = serde_json::from_str(&text).unwrap();
            // No shape carries a repository path (or any member beyond the contract).
            for k in v.as_object().unwrap().keys() {
                assert!(
                    allowed.contains(k.as_str()),
                    "unexpected member {k} in {text}"
                );
            }
        }
    }
}
