//! FIXTURE-POLLUTION-1 §2.2/§2.4 (review-1 #2b, review-2 #1/#2) — the `boundaries summary`
//! production / test-only / unknown PARTITION.
//!
//! The daemon emits the FULL reconciled `summary` unchanged (byte-parity preserved) plus up
//! to two additive disclosures of the same breakdown shape:
//!   - `test_only_summary` — the positively test-only portion. This module parses it,
//!     SUBTRACTS it from the full to yield the production+unknown HEADLINE, and renders the
//!     test-only content as a trailing disclosure.
//!   - `unknown_composition` — the surfaces with no reachable `is_test` evidence. These are
//!     NEVER subtracted (binding direction rule): they stay in the headline counts and are
//!     disclosed with their reasons.
//!
//! # Strict parse, never silent-zero (review-2 #2)
//!
//! Both additive fields are parsed STRICTLY: every key the daemon emits is required (no serde
//! defaults). A present-but-malformed disclosure therefore does NOT decay to zeros that would
//! silently alter the headline (subtracting a partial test-only summary, or dropping an
//! unknown tally) — it becomes [`Additive::Degraded`], which the renderer surfaces as an
//! explicit reason and, for the test-only case, leaves the headline as the FULL summary
//! (nothing subtracted — never hide possibly-real architecture). Absence of the key means
//! genuinely no such content and stays byte-identical.
//!
//! Split out of the sibling [`super`] renderer so the DTO/orchestration file holds the
//! 500-line guardrail (review-1 #3).
//!
//! Abstraction records —
//! - `Additive<T>`: an optional additive disclosure parsed as Absent / Ready(T) /
//!   Degraded(reason); concrete users: `test_only: Additive<TestOnlySummary>` and
//!   `unknown: Additive<UnknownComposition>` on [`super::BoundariesSummaryResponse`]; axis:
//!   the three parse outcomes of an additive field that must NEVER silent-zero; rejected
//!   simpler: `Option<Result<T, String>>` per field (Absent vs Degraded read as noise, and
//!   every match site re-derives the three cases the enum names once).
//! - `partition` module: the §2.2 headline / test-only split + the unknown disclosure, kept
//!   off the DTO file for the guardrail; rejected simpler: inline in `mod.rs` (over-guardrail,
//!   review-1 #3). It accesses the parent's private `BoundarySummary`/`CategoryCount`
//!   directly (descendant visibility), so no wider `pub` leak is introduced.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::{BoundarySummary, CategoryCount};

/// An optional additive disclosure the daemon may splice into the response. It is parsed
/// STRICTLY (review-2 #2): a present-but-malformed payload becomes `Degraded` with a
/// reader-framed reason, NEVER a silent zero-filled `Ready`.
#[derive(Debug)]
pub(crate) enum Additive<T> {
    /// The key was absent — genuinely no such content (byte-identical pre-slice output).
    Absent,
    /// The key parsed strictly.
    Ready(T),
    /// The key was present but did NOT parse strictly; this is the reader-framed reason.
    Degraded(String),
}

impl<T: serde::de::DeserializeOwned> Additive<T> {
    /// Parse a captured additive value. `None` (key absent) → `Absent`; a value that
    /// deserializes strictly into `T` → `Ready`; any deserialization error → `Degraded`
    /// (the malformed payload is disclosed, not silently zeroed).
    pub(crate) fn parse(raw: Option<serde_json::Value>, what: &str) -> Additive<T> {
        match raw {
            None => Additive::Absent,
            Some(value) => match serde_json::from_value::<T>(value) {
                Ok(parsed) => Additive::Ready(parsed),
                Err(err) => {
                    Additive::Degraded(format!("{what} disclosure malformed (degraded): {err}"))
                }
            },
        }
    }

    /// `true` when the field carries content to render (Ready or Degraded), so the empty-case
    /// "no boundaries" short-circuit does not swallow a disclosure.
    pub(crate) fn has_content(&self) -> bool {
        !matches!(self, Additive::Absent)
    }
}

// ---------------------------------------------------------------------------------------
// test-only sub-summary
// ---------------------------------------------------------------------------------------

/// One `{ <label>: k, count: n }` breakdown entry, parsed STRICTLY. A single struct accepts
/// whichever of the five label fields the breakdown uses (via serde aliases) into `category`,
/// and REQUIRES `count` — a missing label or count fails the parse (→ `Degraded`) rather than
/// defaulting to `""`/`0`.
#[derive(Debug, Clone, Deserialize)]
struct StrictLabeledCount {
    #[serde(
        alias = "channelKind",
        alias = "boundaryScope",
        alias = "direction",
        alias = "protocolFamily",
        alias = "basis"
    )]
    category: String,
    count: u64,
}

/// The daemon's additive `test_only_summary`, parsed STRICTLY (every field required — no serde
/// defaults, so a partial payload degrades instead of zero-filling the subtraction).
#[derive(Debug, Clone, Deserialize)]
struct TestOnlySummaryDto {
    #[serde(rename = "totalSurfaces")]
    total_surfaces: u64,
    #[serde(rename = "totalChannels")]
    total_channels: u64,
    #[serde(rename = "byChannelKind")]
    by_channel_kind: Vec<StrictLabeledCount>,
    #[serde(rename = "byBoundaryScope")]
    by_boundary_scope: Vec<StrictLabeledCount>,
    #[serde(rename = "byDirection")]
    by_direction: Vec<StrictLabeledCount>,
    #[serde(rename = "byProtocolFamily")]
    by_protocol_family: Vec<StrictLabeledCount>,
    #[serde(rename = "byBasis")]
    by_basis: Vec<StrictLabeledCount>,
    #[serde(rename = "filesWithBoundaries")]
    files_with_boundaries: Vec<String>,
    http_surface_providers: usize,
    http_surface_consumers: usize,
}

/// The parsed test-only sub-summary: the breakdowns/files (as the parent's `BoundarySummary`)
/// plus the test-only unified HTTP counts the headline HTTP line must exclude. Deserializes
/// via [`TestOnlySummaryDto`] (`#[serde(from)]`), so the strict-field contract is enforced at
/// the wire and this normalized form is what the renderer holds.
#[derive(Debug, Clone, Deserialize)]
#[serde(from = "TestOnlySummaryDto")]
pub(crate) struct TestOnlySummary {
    summary: BoundarySummary,
    pub http_providers: usize,
    pub http_consumers: usize,
}

impl From<TestOnlySummaryDto> for TestOnlySummary {
    fn from(dto: TestOnlySummaryDto) -> Self {
        let cats = |v: Vec<StrictLabeledCount>| -> Vec<CategoryCount> {
            v.into_iter()
                .map(|c| CategoryCount {
                    category: c.category,
                    count: c.count,
                })
                .collect()
        };
        TestOnlySummary {
            summary: BoundarySummary {
                total_surfaces: dto.total_surfaces,
                total_channels: dto.total_channels,
                by_channel_kind: cats(dto.by_channel_kind),
                by_boundary_scope: cats(dto.by_boundary_scope),
                by_direction: cats(dto.by_direction),
                by_protocol_family: cats(dto.by_protocol_family),
                by_basis: cats(dto.by_basis),
                files_with_boundaries: dto.files_with_boundaries,
            },
            http_providers: dto.http_surface_providers,
            http_consumers: dto.http_surface_consumers,
        }
    }
}

impl TestOnlySummary {
    /// The production+unknown HEADLINE: the full reconciled summary MINUS this test-only
    /// sub-summary, per category/total/file. `saturating_sub` guards the (impossible by
    /// construction, but never a panic) case of a test-only count exceeding the full.
    pub(super) fn headline_from(&self, full: &BoundarySummary) -> BoundarySummary {
        BoundarySummary {
            total_surfaces: full
                .total_surfaces
                .saturating_sub(self.summary.total_surfaces),
            total_channels: full
                .total_channels
                .saturating_sub(self.summary.total_channels),
            by_channel_kind: subtract(&full.by_channel_kind, &self.summary.by_channel_kind),
            by_boundary_scope: subtract(&full.by_boundary_scope, &self.summary.by_boundary_scope),
            by_direction: subtract(&full.by_direction, &self.summary.by_direction),
            by_protocol_family: subtract(
                &full.by_protocol_family,
                &self.summary.by_protocol_family,
            ),
            by_basis: subtract(&full.by_basis, &self.summary.by_basis),
            files_with_boundaries: subtract_files(
                &full.files_with_boundaries,
                &self.summary.files_with_boundaries,
            ),
        }
    }

    /// The trailing test-only disclosure: the test-only totals/breakdowns/files, clearly
    /// labeled and pointed at the demoted `boundaries list`. Never a headline claim.
    pub(super) fn render_trailing(&self) -> String {
        let s = &self.summary;
        let mut out = String::new();
        out.push_str("\ntest-only surfaces (excluded from the headline above):\n");
        out.push_str(&format!(
            "  {} surface{}\n",
            s.total_surfaces,
            plural(s.total_surfaces)
        ));
        if s.total_channels > 0 {
            out.push_str(&format!(
                "  {} channel{}\n",
                s.total_channels,
                plural(s.total_channels)
            ));
        }
        if self.http_providers > 0 || self.http_consumers > 0 {
            out.push_str(&format!(
                "  HTTP/REST: {} provider{}, {} consumer{}\n",
                self.http_providers,
                plural(self.http_providers as u64),
                self.http_consumers,
                plural(self.http_consumers as u64),
            ));
        }
        push_breakdown(&mut out, "by channel kind", &s.by_channel_kind);
        push_breakdown(&mut out, "by scope", &s.by_boundary_scope);
        push_breakdown(&mut out, "by direction", &s.by_direction);
        push_breakdown(&mut out, "by protocol", &s.by_protocol_family);
        push_breakdown(&mut out, "by basis", &s.by_basis);
        if !s.files_with_boundaries.is_empty() {
            out.push_str("  files:\n");
            let mut files = s.files_with_boundaries.clone();
            files.sort();
            for f in &files {
                out.push_str(&format!("    {}\n", f));
            }
        }
        out.push_str("  (demoted below the production listing in `rmap boundaries list`)\n");
        out
    }
}

// ---------------------------------------------------------------------------------------
// unknown-composition disclosure (review-2 #1)
// ---------------------------------------------------------------------------------------

/// The daemon's additive `unknown_composition` disclosure, parsed STRICTLY: the count of
/// reconciled surfaces with no reachable `is_test` evidence, plus the distinct reader-framed
/// reasons. These surfaces stay in the HEADLINE (never demoted) — this only annotates them.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct UnknownComposition {
    surfaces: usize,
    reasons: Vec<String>,
}

impl UnknownComposition {
    /// The trailing unknown disclosure: an explicit note that N headline surfaces are
    /// unprovable (not confirmed production), with the reasons. Mirrors the per-row
    /// `[test-composition unknown: <reason>]` marker `boundaries list` carries.
    pub(super) fn render_disclosure(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "\nnote: {} headline surface{} of unknown test-composition — retained above (not \
             confirmed production):\n",
            self.surfaces,
            plural(self.surfaces as u64),
        ));
        for reason in &self.reasons {
            out.push_str(&format!("  {}\n", reason));
        }
        out
    }
}

// ---------------------------------------------------------------------------------------
// subtraction helpers
// ---------------------------------------------------------------------------------------

/// Per-category subtraction (`full − sub`), dropping any category that falls to ≤0. `sub` is a
/// STRICTLY-parsed, complete test-only breakdown, so a category present in `full` but ABSENT
/// from `sub` genuinely has zero test-only members (the daemon's serializer emits only
/// positive counts) — the `unwrap_or(0)` is that serializer contract applied to a verified-
/// complete map, NOT a silent collapse of unknown external evidence. Order follows `full`
/// (the renderer re-sorts by count).
fn subtract(full: &[CategoryCount], sub: &[CategoryCount]) -> Vec<CategoryCount> {
    let sub_map: BTreeMap<&str, u64> = sub.iter().map(|c| (c.category.as_str(), c.count)).collect();
    full.iter()
        .filter_map(|c| {
            let test_only_in_category = sub_map.get(c.category.as_str()).copied().unwrap_or(0);
            let n = c.count.saturating_sub(test_only_in_category);
            (n > 0).then(|| CategoryCount {
                category: c.category.clone(),
                count: n,
            })
        })
        .collect()
}

/// Set difference on the file list (`full` minus the wholly-test-only files). The daemon
/// classifies a file test-only ONLY when every reconciled row on it is test-only, so this
/// never removes a file that also has a production/unknown surface.
fn subtract_files(full: &[String], test_only: &[String]) -> Vec<String> {
    let drop: std::collections::BTreeSet<&str> = test_only.iter().map(String::as_str).collect();
    full.iter()
        .filter(|f| !drop.contains(f.as_str()))
        .cloned()
        .collect()
}

/// A `count desc, then category asc` breakdown block under the trailing test-only header
/// (mirrors the headline's ordering), rendered only when non-empty.
fn push_breakdown(out: &mut String, label: &str, items: &[CategoryCount]) {
    if items.is_empty() {
        return;
    }
    out.push_str(&format!("  {}:\n", label));
    let mut sorted = items.to_vec();
    sorted.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.category.cmp(&b.category))
    });
    for item in &sorted {
        out.push_str(&format!("    {}  {}\n", item.count, item.category));
    }
}

fn plural(n: u64) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat(c: &str, n: u64) -> CategoryCount {
        CategoryCount {
            category: c.to_string(),
            count: n,
        }
    }

    #[test]
    fn subtract_drops_fully_removed_categories() {
        let full = vec![cat("http", 5), cat("amqp", 2)];
        let sub = vec![cat("amqp", 2)];
        let out = subtract(&full, &sub);
        // amqp fully removed; http untouched.
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].category, "http");
        assert_eq!(out[0].count, 5);
    }

    #[test]
    fn subtract_files_is_set_difference() {
        let full = vec!["a.rs".to_string(), "b.rs".to_string(), "c.rs".to_string()];
        let test_only = vec!["b.rs".to_string()];
        assert_eq!(subtract_files(&full, &test_only), vec!["a.rs", "c.rs"]);
    }

    #[test]
    fn additive_absent_ready_degraded() {
        // Absent key.
        let a: Additive<UnknownComposition> = Additive::parse(None, "unknown_composition");
        assert!(matches!(a, Additive::Absent));
        // Well-formed → Ready.
        let ok = serde_json::json!({ "surfaces": 2, "reasons": ["no fact for x"] });
        let a: Additive<UnknownComposition> = Additive::parse(Some(ok), "unknown_composition");
        match a {
            Additive::Ready(u) => {
                assert_eq!(u.surfaces, 2);
                assert_eq!(u.reasons, vec!["no fact for x".to_string()]);
            }
            _ => panic!("expected Ready"),
        }
        // Malformed (missing `reasons`) → Degraded, NOT a zero-filled Ready.
        let bad = serde_json::json!({ "surfaces": 2 });
        let a: Additive<UnknownComposition> = Additive::parse(Some(bad), "unknown_composition");
        match a {
            Additive::Degraded(r) => assert!(r.contains("malformed"), "{r}"),
            _ => panic!("a partial payload must degrade, never silent-zero"),
        }
    }

    #[test]
    fn test_only_partial_payload_degrades_not_zeroes() {
        // review-2 #2: a `test_only_summary` missing a required field (here `totalSurfaces`)
        // must NOT parse into a zero-filled summary that then subtracts nothing/garbage from
        // the headline — it degrades.
        let partial = serde_json::json!({
            "totalChannels": 0,
            "byChannelKind": [],
            "byBoundaryScope": [],
            "byDirection": [],
            "byProtocolFamily": [],
            "byBasis": [],
            "filesWithBoundaries": [],
            "http_surface_providers": 0,
            "http_surface_consumers": 0
        });
        let a: Additive<TestOnlySummary> = Additive::parse(Some(partial), "test_only_summary");
        assert!(
            matches!(a, Additive::Degraded(_)),
            "missing totalSurfaces must degrade, never zero-fill"
        );
    }

    #[test]
    fn unknown_disclosure_names_count_and_reasons() {
        let u = UnknownComposition {
            surfaces: 3,
            reasons: vec![
                "no stored is_test fact for vendor/a.ts".to_string(),
                "no stored is_test fact for vendor/b.ts".to_string(),
            ],
        };
        let out = u.render_disclosure();
        assert!(
            out.contains("3 headline surfaces of unknown test-composition"),
            "{out}"
        );
        assert!(out.contains("not confirmed production"), "{out}");
        assert!(out.contains("vendor/a.ts"), "{out}");
        assert!(out.contains("vendor/b.ts"), "{out}");
    }
}
