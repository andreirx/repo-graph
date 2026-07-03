//! Per-language measurement coverage — the data-driven honesty verdict.
//!
//! METRIC-LANG-COVERAGE-1 (part A). A quality signal computed for only some of
//! a repo's languages must SAY SO wherever it renders (VISION 2026-07-02:
//! "coverage is part of the fact"; "a quality signal computed for only some
//! supported languages must say so wherever it renders"). This module turns raw
//! per-language function/measured counts (Layer-0/1 extracted facts) into a
//! Layer-2 bounded inference: which languages hold a non-trivial share of the
//! repo's functions yet carry (near-)zero complexity measurements, and a
//! reader-frame caveat naming them.
//!
//! ## Why this lives in `classification` (pure), not in a surface
//!
//! Three surfaces render complexity-derived content and must each carry the
//! caveat: `orient` complexity centers (via the daemon orient adapter),
//! `hotspots` (daemon handler), and `metrics` (the `rgr` command). The verdict
//! is one pure function over counts, so it lives ONCE here and all three call
//! it. The RAW counts come from `repo_graph_storage::StorageConnection::
//! query_measurement_coverage` (the SQL join lives in the adapter; policy lives
//! here). [abstraction: pure coverage verdict; users: orient/hotspots/metrics;
//! axis of variation: none today beyond the 3 callers — the shared logic is the
//! reason it exists; rejected alternative: inline the same threshold+wording in
//! all three surfaces (drift + 3× the reviewer burden).]
//!
//! ## The trigger is DATA-DRIVEN — no hardcoded language list
//!
//! A language triggers the caveat purely from its counts:
//!   * it holds ≥ [`SIGNIFICANT_FUNCTION_SHARE`] of the repo's function/method
//!     symbols (non-trivial — a stray file of an unmeasured language does not
//!     nag), AND
//!   * its measured share is at/below [`MEASURED_SHARE_FLOOR`] (zero / near-zero
//!     — essentially unmeasured).
//!
//! No language NAME appears in the trigger. When an extractor starts emitting
//! `cyclomatic_complexity` for a language (as `rust-extractor` now does, part B),
//! that language's measured share climbs above the floor and it drops out of the
//! caveat by itself — the STOP-condition contract ("must disappear by itself").
//! [`display_language`] maps raw extractor tokens to reader-frame names for
//! WORDING only; an unknown token still participates in the trigger and gets a
//! capitalized fallback label, so a newly-added language is never silently
//! dropped.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A language holding this share (of all function/method symbols in the
/// snapshot) or more is "non-trivial" — below it, an unmeasured language is
/// noise and does not raise a caveat (slice §2.A threshold: ≥5%).
pub const SIGNIFICANT_FUNCTION_SHARE: f64 = 0.05;

/// A language whose measured share (of its OWN functions) is at/below this is
/// treated as unmeasured ("zero/near-zero", slice §2.A). A supported language
/// measures ~every function body, so it sits far above this; an unsupported one
/// sits at exactly 0. The small band (not strictly 0) absorbs a stray
/// misattributed measurement without flipping the verdict, and — symmetrically —
/// bodyless declarations (trait signatures, `extern` fns, interface methods)
/// carried as FUNCTION/METHOD symbols never drag a genuinely-measured language
/// down to it.
pub const MEASURED_SHARE_FLOOR: f64 = 0.05;

/// The measurement this coverage describes. Kept as data on the block so a
/// `--json` consumer reads which measurement the coverage refers to; the human
/// caveat uses the plain word "Complexity".
const MEASUREMENT_KIND: &str = "cyclomatic_complexity";

/// Raw per-language function/measured counts — the SINGLE boundary DTO between
/// the storage adapter (which produces it via the nodes⋈files⋈measurements
/// join) and this pure policy (which consumes it). `language` is the raw
/// extractor token (`typescript` / `tsx` / `rust` / `c` / `cpp` / …), NOT a
/// display name; [`compute_measurement_coverage`] folds tokens to reader-frame
/// languages (so `.ts` + `.tsx` become one "TypeScript").
///
/// Invariant the producer guarantees: `measured_count <= function_count` (both
/// range over the SAME function/method node set — the join counts, per node,
/// whether a complexity measurement exists).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageFunctionCount {
    pub language: String,
    pub function_count: u64,
    pub measured_count: u64,
}

/// Per-(display-)language coverage, after folding raw tokens to reader names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LanguageCoverage {
    /// Reader-frame display name (e.g. `Rust`, `TypeScript`, `C++`).
    pub language: String,
    /// Function/method symbols of this language in the snapshot.
    pub function_count: u64,
    /// How many of them carry a complexity measurement.
    pub measured_count: u64,
    /// This language's function/method symbols as a fraction of the snapshot's.
    pub function_share: f64,
    /// Measured symbols as a fraction of THIS language's function/method symbols
    /// (`0.0` when the language has no functions — cannot occur here since a row
    /// exists only for languages with ≥1 function).
    pub measured_share: f64,
    /// `true` when `measured_share` is above [`MEASURED_SHARE_FLOOR`].
    pub measured: bool,
}

/// The whole-snapshot measurement-coverage block, embedded on every
/// complexity-bearing surface's output (human caveat + `--json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasurementCoverage {
    /// The measurement kind this coverage describes (`cyclomatic_complexity`).
    pub kind: String,
    /// Function/method symbols across all languages in the snapshot.
    pub total_functions: u64,
    /// Every language with ≥1 function/method symbol, sorted by display name
    /// (full honest data, present even when nothing is caveated).
    pub languages: Vec<LanguageCoverage>,
    /// Display names of the significant + unmeasured languages, most-significant
    /// first — the ones the caveat calls out. Empty when coverage is complete.
    pub unmeasured: Vec<String>,
    /// The reader-frame caveat sentence. `None` when every significant language
    /// is measured — so it "disappears by itself" when a language gains
    /// measurements (STOP-condition contract).
    pub caveat: Option<String>,
}

/// The measurement-coverage block as it rides EVERY complexity-bearing surface
/// (`orient` complexity centers, `hotspots`, `metrics`), in `--json` and human form.
///
/// This wrapper exists so the block is ALWAYS PRESENT on those surfaces — VISION
/// 2026-07-02: "coverage is part of the fact" and "degradation is a first-class
/// output". Earlier the block was dropped silently when the snapshot read (or its
/// serialization) failed, which reintroduced the exact silent degradation this slice
/// closes: a consumer seeing NO block would read it as COMPLETE coverage. Now a read
/// failure yields an explicit [`MeasurementCoverageBlock::Unavailable`] — the block is
/// never absent because it could not be computed, only ever absent because a surface
/// has no complexity content to describe (e.g. an `orient` with no complexity centers).
///
/// Serialized internally-tagged on `status`, so a `--json` consumer switches on ONE
/// field: `{"status":"available", …the coverage fields flat…}` or
/// `{"status":"unavailable","reason":"…"}`.
/// [abstraction: always-present coverage envelope; concrete users: the orient / hotspots
/// / metrics surfaces (3 callers); axis of variation: availability (computed vs
/// snapshot-read failed); rejected: an `available: bool` + `Option<reason>` pair on
/// `MeasurementCoverage` (admits illegal available+reason / unavailable+data states),
/// and the prior silent drop (the review-6 defect).]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MeasurementCoverageBlock {
    /// Coverage was computed from the snapshot; carries the full per-language verdict
    /// (the [`MeasurementCoverage`] fields sit flat beside `"status":"available"`).
    Available(MeasurementCoverage),
    /// Coverage could NOT be computed (the snapshot measurement read failed). Present,
    /// never silently absent, so a consumer never reads a read-failure as complete
    /// coverage. `reason` is reader-frame.
    Unavailable { reason: String },
}

/// The reader-frame reason on an [`MeasurementCoverageBlock::Unavailable`]. Names NO
/// language (the STOP-condition contract holds even in the failure path) and speaks the
/// reader's frame — *their* snapshot's complexity coverage — not our pipeline's error.
const COVERAGE_UNAVAILABLE_REASON: &str =
    "complexity measurement coverage could not be read for this snapshot";

impl MeasurementCoverageBlock {
    /// The `Available` block for a successful per-language count read.
    pub fn from_counts(counts: Vec<LanguageFunctionCount>) -> Self {
        MeasurementCoverageBlock::Available(compute_measurement_coverage(counts))
    }

    /// The explicit `Unavailable` block (snapshot read failed / storage unreachable).
    pub fn unavailable() -> Self {
        MeasurementCoverageBlock::Unavailable {
            reason: COVERAGE_UNAVAILABLE_REASON.to_string(),
        }
    }

    /// Map a storage read RESULT to the always-present block: `Ok` → `Available`;
    /// `Err` → the explicit `Unavailable` — NEVER a dropped block (the review-6 fix).
    /// Generic over the storage error so this single decision lives in `classification`
    /// with no storage dependency (each surface passes its own read `Result`).
    pub fn from_result<E>(counts: Result<Vec<LanguageFunctionCount>, E>) -> Self {
        match counts {
            Ok(counts) => Self::from_counts(counts),
            Err(_) => Self::unavailable(),
        }
    }

    /// The one honesty line for human surfaces: the computed caveat when a significant
    /// language is unmeasured, the unavailable reason when coverage could not be read,
    /// and `None` only when coverage is COMPLETE (nothing to say — the caveat
    /// "disappears by itself"). `orient` / `hotspots` render exactly this line.
    pub fn caveat_line(&self) -> Option<String> {
        match self {
            MeasurementCoverageBlock::Available(cov) => cov.caveat.clone(),
            MeasurementCoverageBlock::Unavailable { reason } => Some(reason.clone()),
        }
    }

    /// Serialize to a JSON value that is ALWAYS a valid block. A serialize failure of
    /// the `Available` variant (near-impossible for this plain numeric/string data —
    /// `share` never yields NaN/Inf) falls back to the `Unavailable` block, which is a
    /// tag + a string and cannot fail. So the block is never silently dropped on
    /// serialization either (the second half of the review-6 fix).
    pub fn into_json_value(self) -> serde_json::Value {
        serde_json::to_value(&self).unwrap_or_else(|_| {
            serde_json::to_value(MeasurementCoverageBlock::unavailable())
                .expect("the Unavailable block (tag + string) always serializes")
        })
    }
}

/// Compute the per-language coverage verdict from raw counts.
///
/// Pure and deterministic: raw tokens are folded to reader-frame languages
/// (summing `.ts`+`.tsx` etc.), shares are computed against the snapshot total,
/// and a caveat is minted iff at least one language is both significant and
/// unmeasured. Empty input (no function/method symbols) yields an empty,
/// caveat-free block.
pub fn compute_measurement_coverage(counts: Vec<LanguageFunctionCount>) -> MeasurementCoverage {
    // Fold raw extractor tokens to reader-frame languages (deterministic order
    // via BTreeMap keyed by display name).
    let mut folded: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for c in counts {
        let entry = folded
            .entry(display_language(&c.language))
            .or_insert((0, 0));
        entry.0 += c.function_count;
        entry.1 += c.measured_count;
    }

    let total_functions: u64 = folded.values().map(|(f, _)| *f).sum();

    let languages: Vec<LanguageCoverage> = folded
        .into_iter()
        .map(|(language, (function_count, measured_count))| {
            let function_share = share(function_count, total_functions);
            let measured_share = share(measured_count, function_count);
            LanguageCoverage {
                language,
                function_count,
                measured_count,
                function_share,
                measured_share,
                measured: measured_share > MEASURED_SHARE_FLOOR,
            }
        })
        .collect();

    // Significant + unmeasured languages, most-significant first (then by name
    // for a total, stable order).
    let mut flagged: Vec<&LanguageCoverage> = languages
        .iter()
        .filter(|l| l.function_share >= SIGNIFICANT_FUNCTION_SHARE && !l.measured)
        .collect();
    flagged.sort_by(|a, b| {
        b.function_share
            .partial_cmp(&a.function_share)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.language.cmp(&b.language))
    });

    let unmeasured: Vec<String> = flagged.iter().map(|l| l.language.clone()).collect();
    let caveat = build_caveat(&languages, &flagged);

    MeasurementCoverage {
        kind: MEASUREMENT_KIND.to_string(),
        total_functions,
        languages,
        unmeasured,
        caveat,
    }
}

/// `numer / denom` as a fraction, `0.0` when `denom == 0` (never NaN).
fn share(numer: u64, denom: u64) -> f64 {
    if denom == 0 {
        0.0
    } else {
        numer as f64 / denom as f64
    }
}

/// Build the reader-frame caveat. `None` when nothing is flagged.
fn build_caveat(languages: &[LanguageCoverage], flagged: &[&LanguageCoverage]) -> Option<String> {
    if flagged.is_empty() {
        return None;
    }

    // "Rust (72% of functions) is not yet measured" — one phrase per flagged lang.
    let phrases: Vec<String> = flagged
        .iter()
        .map(|l| format!("{} ({}% of functions)", l.language, pct(l.function_share)))
        .collect();
    let verb = if phrases.len() == 1 {
        "is not yet measured"
    } else {
        "are not yet measured"
    };
    let omit = if phrases.len() == 1 { "it" } else { "them" };

    // Which languages ARE measured — for the "measured for … only" frame.
    let measured: Vec<&str> = languages
        .iter()
        .filter(|l| l.measured)
        .map(|l| l.language.as_str())
        .collect();

    let lead = if measured.is_empty() {
        // Nothing is measured at all (e.g. a pure-Java snapshot).
        "Complexity is not measured on this snapshot".to_string()
    } else {
        format!(
            "Complexity is measured for {} only on this snapshot",
            join_and(&measured)
        )
    };

    Some(format!(
        "{} — {} {}; rankings omit {}.",
        lead,
        join_and_owned(&phrases),
        verb,
        omit
    ))
}

/// Percentage of a fraction, rounded to the nearest whole percent.
fn pct(fraction: f64) -> u64 {
    (fraction * 100.0).round() as u64
}

/// `["C", "TypeScript"]` → `"C and TypeScript"`; `["A","B","C"]` → `"A, B and C"`.
fn join_and(items: &[&str]) -> String {
    let owned: Vec<String> = items.iter().map(|s| s.to_string()).collect();
    join_and_owned(&owned)
}

fn join_and_owned(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [head @ .., last] => format!("{} and {}", head.join(", "), last),
    }
}

/// Map a raw extractor language token to a reader-frame display name. Covers the
/// closed vocabulary `detect_language` emits (typescript/tsx/javascript/jsx/
/// java/python/rust/c/cpp); an unknown token is capitalized so a newly-added
/// language still gets a sensible label and is never silently dropped. This is
/// WORDING only — the caveat trigger is the numeric share, never the name.
fn display_language(raw: &str) -> String {
    match raw {
        "typescript" | "tsx" => "TypeScript",
        "javascript" | "jsx" => "JavaScript",
        "java" => "Java",
        "python" => "Python",
        "rust" => "Rust",
        "c" => "C",
        "cpp" => "C++",
        other => return capitalize(other),
    }
    .to_string()
}

/// Capitalize the first character (ASCII), leaving the rest as-is — the fallback
/// label for a language token not in the known display map.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(lang: &str, functions: u64, measured: u64) -> LanguageFunctionCount {
        LanguageFunctionCount {
            language: lang.to_string(),
            function_count: functions,
            measured_count: measured,
        }
    }

    fn find<'a>(cov: &'a MeasurementCoverage, lang: &str) -> &'a LanguageCoverage {
        cov.languages
            .iter()
            .find(|l| l.language == lang)
            .unwrap_or_else(|| panic!("no coverage row for {lang}"))
    }

    // ── Coverage-caveat proof: one language unmeasured → caveat ──────

    #[test]
    fn unmeasured_significant_language_triggers_caveat() {
        // The repo-graph shape: mostly-Rust (unmeasured, pre-part-B) + legacy TS
        // (measured). base fractions: Rust 72% unmeasured, TS 28% measured.
        let cov =
            compute_measurement_coverage(vec![count("rust", 72, 0), count("typescript", 28, 28)]);
        let caveat = cov.caveat.clone().expect("expected a caveat");
        assert!(caveat.contains("Rust (72% of functions)"), "{caveat}");
        assert!(caveat.contains("not yet measured"), "{caveat}");
        assert!(caveat.contains("TypeScript"), "{caveat}");
        assert!(caveat.contains("rankings omit it"), "{caveat}");
        assert_eq!(cov.unmeasured, vec!["Rust".to_string()]);
        assert_eq!(cov.total_functions, 100);
        assert!(!find(&cov, "Rust").measured);
        assert!(find(&cov, "TypeScript").measured);
    }

    // ── Coverage-caveat proof: all measured → no caveat ─────────────

    #[test]
    fn all_languages_measured_renders_no_caveat() {
        // The "disappears by itself" contract: once Rust is measured (part B),
        // nothing is flagged.
        let cov = compute_measurement_coverage(vec![
            count("rust", 72, 71), // one bodyless trait sig unmeasured — still measured
            count("typescript", 28, 28),
        ]);
        assert_eq!(cov.caveat, None);
        assert!(cov.unmeasured.is_empty());
        assert!(find(&cov, "Rust").measured);
    }

    #[test]
    fn trivial_share_unmeasured_language_does_not_trigger() {
        // A single Python script (2% of functions) in a measured repo: unmeasured
        // but below the significance floor → no nag.
        let cov = compute_measurement_coverage(vec![count("rust", 98, 98), count("python", 2, 0)]);
        assert_eq!(cov.caveat, None, "trivial share must not caveat");
        // …but it is still present in the honest per-language data.
        assert!(!find(&cov, "Python").measured);
        assert_eq!(find(&cov, "Python").function_count, 2);
    }

    #[test]
    fn multiple_unmeasured_languages_listed_by_share_desc() {
        let cov = compute_measurement_coverage(vec![
            count("typescript", 20, 20),
            count("java", 50, 0),
            count("python", 30, 0),
        ]);
        // Java (50%) leads Python (30%); both flagged, TS measured.
        assert_eq!(
            cov.unmeasured,
            vec!["Java".to_string(), "Python".to_string()]
        );
        let caveat = cov.caveat.unwrap();
        assert!(caveat.contains("Java (50% of functions)"), "{caveat}");
        assert!(caveat.contains("Python (30% of functions)"), "{caveat}");
        assert!(caveat.contains("are not yet measured"), "{caveat}");
        assert!(caveat.contains("rankings omit them"), "{caveat}");
        assert!(caveat.contains("measured for TypeScript only"), "{caveat}");
    }

    #[test]
    fn nothing_measured_uses_not_measured_frame() {
        let cov = compute_measurement_coverage(vec![count("java", 100, 0)]);
        let caveat = cov.caveat.unwrap();
        assert!(
            caveat.starts_with("Complexity is not measured on this snapshot"),
            "{caveat}"
        );
        assert!(caveat.contains("Java (100% of functions)"), "{caveat}");
    }

    #[test]
    fn typescript_and_tsx_fold_into_one_language() {
        // Raw tokens .ts + .tsx must merge into a single "TypeScript".
        let cov = compute_measurement_coverage(vec![
            count("typescript", 30, 30),
            count("tsx", 10, 10),
            count("rust", 60, 0),
        ]);
        let ts = find(&cov, "TypeScript");
        assert_eq!(ts.function_count, 40);
        assert_eq!(ts.measured_count, 40);
        // Only one TS row exists.
        assert_eq!(
            cov.languages
                .iter()
                .filter(|l| l.language == "TypeScript")
                .count(),
            1
        );
    }

    #[test]
    fn empty_input_is_caveat_free() {
        let cov = compute_measurement_coverage(vec![]);
        assert_eq!(cov.total_functions, 0);
        assert!(cov.languages.is_empty());
        assert_eq!(cov.caveat, None);
    }

    #[test]
    fn unknown_language_token_gets_capitalized_and_still_triggers() {
        // A language not in the display map still participates in the trigger.
        let cov = compute_measurement_coverage(vec![count("rust", 60, 60), count("zig", 40, 0)]);
        assert_eq!(cov.unmeasured, vec!["Zig".to_string()]);
    }

    #[test]
    fn kind_is_recorded() {
        let cov = compute_measurement_coverage(vec![count("rust", 10, 10)]);
        assert_eq!(cov.kind, "cyclomatic_complexity");
    }

    #[test]
    fn join_and_grammar() {
        assert_eq!(join_and(&["A"]), "A");
        assert_eq!(join_and(&["A", "B"]), "A and B");
        assert_eq!(join_and(&["A", "B", "C"]), "A, B and C");
    }

    // ── review-6 item 2: the block is ALWAYS PRESENT (Available | Unavailable) ──

    #[test]
    fn from_result_ok_is_available_with_verdict() {
        let block = MeasurementCoverageBlock::from_result(Ok::<_, ()>(vec![
            count("rust", 72, 0),
            count("typescript", 28, 28),
        ]));
        match &block {
            MeasurementCoverageBlock::Available(cov) => {
                assert_eq!(cov.unmeasured, vec!["Rust".to_string()]);
            }
            other => panic!("expected Available, got {other:?}"),
        }
        assert!(block
            .caveat_line()
            .unwrap()
            .contains("Rust (72% of functions)"));
    }

    #[test]
    fn from_result_err_is_explicit_unavailable_not_dropped() {
        // The review-6 core: a snapshot read FAILURE yields an explicit Unavailable
        // block, NEVER an absent one (silent degradation). Present and self-describing.
        let block = MeasurementCoverageBlock::from_result(Err::<Vec<LanguageFunctionCount>, _>(
            "db read failed",
        ));
        assert_eq!(block, MeasurementCoverageBlock::unavailable());
        let line = block.caveat_line().expect("unavailable always has a line");
        assert!(line.contains("could not be read"), "{line}");
        // No language name — the STOP-condition (no hardcoded list) holds even here.
        for lang in ["Rust", "TypeScript", "Java", "Python", "C++"] {
            assert!(
                !line.contains(lang),
                "the unavailable reason must name no language: {line}"
            );
        }
    }

    #[test]
    fn available_serializes_status_tagged_with_flat_fields() {
        let json = MeasurementCoverageBlock::from_counts(vec![
            count("rust", 72, 0),
            count("typescript", 28, 28),
        ])
        .into_json_value();
        assert_eq!(json["status"], "available");
        // The MeasurementCoverage fields sit FLAT beside the tag (internally tagged).
        assert_eq!(json["kind"], "cyclomatic_complexity");
        assert_eq!(json["unmeasured"], serde_json::json!(["Rust"]));
        assert!(json["caveat"].is_string());
        assert!(json["languages"].is_array());
    }

    #[test]
    fn unavailable_serializes_status_tagged_with_reason() {
        let json = MeasurementCoverageBlock::unavailable().into_json_value();
        assert_eq!(json["status"], "unavailable");
        assert!(json["reason"]
            .as_str()
            .unwrap()
            .contains("could not be read"));
        // Never carries fabricated coverage data (unknown is never zero).
        assert!(json.get("languages").is_none());
        assert!(json.get("total_functions").is_none());
    }

    #[test]
    fn block_round_trips_through_json_value() {
        // The daemon serializes to serde_json::Value (opaque, agent boundary); rgr
        // presentation deserializes it back — prove both variants survive the round-trip.
        for block in [
            MeasurementCoverageBlock::from_counts(vec![
                count("rust", 60, 0),
                count("typescript", 40, 40),
            ]),
            MeasurementCoverageBlock::unavailable(),
        ] {
            let value = block.clone().into_json_value();
            let back: MeasurementCoverageBlock = serde_json::from_value(value).unwrap();
            assert_eq!(back, block);
        }
    }

    #[test]
    fn available_complete_coverage_has_no_caveat_line() {
        let block = MeasurementCoverageBlock::from_counts(vec![count("rust", 100, 100)]);
        assert_eq!(block.caveat_line(), None, "complete coverage says nothing");
    }
}
