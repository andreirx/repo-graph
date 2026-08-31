//! DEPS-ATTRIB-2 §2.4 — the CLI view of one materially-present SECONDARY ecosystem's truth in the
//! default `deps list` output (ruling DR-JAVA-NOREADER = Option 2).
//!
//! Crate-private module extracted from `deps_list.rs` under the 500-line structural guardrail
//! (DEPS-ATTRIB-2 review-1 item 5 — a pre-ratified, guardrail-driven extraction, NOT a new public
//! boundary). WHAT: the `other_ecosystems` DTO + the one-line-per-ecosystem renderer. CALLER:
//! `deps_list::DepsListResponse` (the field type + `render_human`, which appends one line per
//! element). AXIS: a FIXED set of ecosystem-truth states with a growing rendering surface → exhaustive
//! match over the `state` tag. REJECTED SIMPLER: leaving it inline in the 540-line-pre-slice
//! `deps_list.rs` — the guardrail forbids appending a new responsibility to a >500-line file. This is
//! a pure view over the JSON DTO (no daemon/business logic); it mirrors the daemon's
//! `EcosystemPresenceState` sum type.

use serde::Deserialize;

/// One materially-present secondary ecosystem's truth (a `state` tag plus only the fields valid in
/// that state — mirrors the daemon's `EcosystemPresenceState` sum type). Lenient deserialize
/// (`#[serde(default)]`) so a payload from a slightly older/newer daemon still renders.
/// `pub(crate)`, not `pub` (DEPS-ATTRIB-2 review-2): a crate-private client-side view type,
/// not a new public Rust API. Its only user is the sibling `deps_list` presenter.
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct OtherEcosystem {
    #[serde(default)]
    pub(crate) ecosystem: String,
    /// "attributed" | "unavailable" | "no_manifest_parsed".
    #[serde(default)]
    pub(crate) state: String,
    /// `attributed`: declared deps read + attributed to modules (0 is honest measured-empty).
    #[serde(default)]
    pub(crate) declared_dependencies: u64,
    /// `attributed`: parsed manifests of this ecosystem.
    #[serde(default)]
    pub(crate) manifests: u64,
    /// `unavailable`: the specific unknown-with-reason cause.
    #[serde(default)]
    pub(crate) reason: String,
    /// `no_manifest_parsed`: material source files present, but no manifest parsed (computed absence).
    #[serde(default)]
    pub(crate) source_files: u64,
}

/// One honest line for a materially-present secondary ecosystem. Exhaustive over the `state` tag; an
/// unrecognized tag (a newer daemon) degrades to naming the ecosystem, never a fabricated claim.
/// NEVER a no-reader sentence for an ecosystem that HAS a reader (ruling Option 2).
pub(crate) fn render_other_ecosystem(e: &OtherEcosystem) -> String {
    let eco = if e.ecosystem.is_empty() {
        "other"
    } else {
        &e.ecosystem
    };
    match e.state.as_str() {
        "attributed" => format!(
            "{}: {} declared dependenc{} across {} manifest{} — `deps list --ecosystem {}` for detail\n",
            eco,
            e.declared_dependencies,
            if e.declared_dependencies == 1 { "y" } else { "ies" },
            e.manifests,
            if e.manifests == 1 { "" } else { "s" },
            eco,
        ),
        "unavailable" => format!("{}: dependency truth unavailable ({})\n", eco, e.reason),
        "no_manifest_parsed" => format!(
            "{}: {} source file{} indexed, no manifest parsed on this index\n",
            eco,
            e.source_files,
            if e.source_files == 1 { "" } else { "s" },
        ),
        // Newer-daemon tag this build doesn't know: name the ecosystem, claim nothing about it.
        other => format!("{}: present ({})\n", eco, other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eco(state: &str) -> OtherEcosystem {
        OtherEcosystem {
            ecosystem: "java".into(),
            state: state.into(),
            ..Default::default()
        }
    }

    #[test]
    fn attributed_names_the_ecosystem_and_points_at_the_targeted_view() {
        // §2.4 (ruling Option 2): a reader-bearing ecosystem's attributed deps — NO no-reader sentence.
        let out = render_other_ecosystem(&OtherEcosystem {
            declared_dependencies: 18,
            manifests: 1,
            ..eco("attributed")
        });
        assert_eq!(
            out,
            "java: 18 declared dependencies across 1 manifest — `deps list --ecosystem java` for detail\n"
        );
        let lower = out.to_lowercase();
        assert!(
            !lower.contains("no gradle reader") && !lower.contains("no dependency-manifest reader"),
            "must not emit a no-reader sentence for an ecosystem that has a reader: {out}"
        );
    }

    #[test]
    fn unavailable_states_the_reason_never_a_false_absence() {
        let out = render_other_ecosystem(&OtherEcosystem {
            reason: "manifest backend/build.gradle present but not parsed: permission denied"
                .into(),
            ..eco("unavailable")
        });
        assert_eq!(
            out,
            "java: dependency truth unavailable (manifest backend/build.gradle present but not parsed: permission denied)\n"
        );
    }

    #[test]
    fn no_manifest_parsed_states_computed_absence() {
        let out = render_other_ecosystem(&OtherEcosystem {
            source_files: 267,
            ..eco("no_manifest_parsed")
        });
        assert_eq!(
            out,
            "java: 267 source files indexed, no manifest parsed on this index\n"
        );
    }

    #[test]
    fn unknown_tag_from_a_newer_daemon_names_the_ecosystem_only() {
        // Forward-compatible: a tag this build doesn't know claims nothing beyond the ecosystem name.
        let out = render_other_ecosystem(&eco("some_future_state"));
        assert_eq!(out, "java: present (some_future_state)\n");
    }
}
