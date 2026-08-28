//! `inferences_serve.rs` — INFERENCES-SURFACE-1 serve helpers for `inferences_list`.
//!
//! Pure functions that build the *additive* machine-contract of the `inferences
//! list` response: per-record source location (file + line), the inference-detector
//! inventory (which detectors exist on this build, whether each applies to the
//! snapshot's languages, and how many rows it produced), the honest empty-state
//! reason, and the `count`/`returned`/`truncated`/`limit` truncation headline. No
//! storage, no I/O — inputs are the faithful (complete) row set plus the snapshot's
//! language set — so the machine contract is unit-tested without a daemon.
//!
//! Abstraction (INFERENCES-SURFACE-1): crate-private module.
//!   - what: response assembler + detector-inventory + empty-state builder for the
//!     inferences serve path.
//!   - current user: `dispatch::handle_inferences_list` (sole caller).
//!   - axis of variation: the JSON machine contract must carry
//!     `file`/`line`/`detectors`/`empty`/`returned`/`truncated` (DoD §2/§4);
//!     `dispatch.rs` is >500 lines (structural guardrail forbids growth) and this
//!     logic must be testable daemon-free.
//!   - rejected simpler: inline in `dispatch.rs` — rejected (guardrail + no test seam).
//!
//! ## Source location (operator ruling 2026-08-28 §1)
//!
//! The location a compact record renders comes from data the row ALREADY holds — no
//! storage projection is added (smaller than a DTO/query change, and equally
//! faithful):
//!   - FILE: the path is parsed from `target_stable_key`. Inference detectors emit
//!     exactly two key shapes (verified from the producers, not names):
//!     `react_detector` → `{repo}:{path}#{sym}:SYMBOL:FUNCTION` (component / hook
//!     with a known caller) or `{repo}:{path}:FILE` (a file-level hook with no
//!     caller); `spring_liveness` → the node's `{repo}:{path}#{class}:SYMBOL:CLASS`.
//!     `repo_uid` is `repo_<ulid>` (Crockford base32 — no `:`), so the path is the
//!     text after the first `:`, up to `#` (SYMBOL) or the trailing `:FILE`.
//!   - LINE: from the value payload's `line_start` WHERE RECORDED. React records it;
//!     Spring's value is `{annotation, convention, reason}` with NO line. So the
//!     compact contract is `file[:line when recorded]` — file always, line only when
//!     the detector recorded one, NEVER a fabricated `0`.
//!
//! ## Detector catalog (evidence-based, not name-derived)
//!
//! The three live inference kinds and their producing detectors were verified from
//! the producers, NOT inferred from names:
//!   - React (`react_detector.rs`): emits `react_component` + `react_hook_usage`,
//!     gated on a JS/TS-family file, so it applies to the
//!     `javascript`/`typescript`/`tsx`/`jsx` file-language labels.
//!   - Spring (`classification/spring_liveness.rs`): emits `spring_container_managed`
//!     for `java`.
//!
//! ## Honesty: applicability is derived, a produced count is a fact
//!
//! Inference-detector EXECUTION is not recorded anywhere on this build (there is no
//! per-detector run fact — `snapshots.extraction_diagnostics_json` covers call
//! resolution, not inference detectors). So we NEVER assert a detector "ran": we
//! report `applicable` (a FACT: the detector's language is in the snapshot's language
//! mix) and `count` (a FACT: rows it produced). The applicable-but-zero case is
//! phrased as derived-from-the-language-mix, never as observed execution.

use std::collections::{BTreeMap, BTreeSet};

use repo_graph_storage::queries::InferenceListRow;

/// One inference detector that ships in this build. `languages` are the lowercased
/// `files.language` labels the detector's producer actually runs over (verified from
/// the producer's file gate, not the detector's name).
struct DetectorSpec {
    id: &'static str,
    label: &'static str,
    subjects: &'static str,
    kinds: &'static [&'static str],
    languages: &'static [&'static str],
}

/// The live inference-detector inventory. Adding a detector kind here is a
/// SURFACE-only change (the producers are frozen for this slice); the entries below
/// are the exact set that exists at this build.
const CATALOG: &[DetectorSpec] = &[
    DetectorSpec {
        id: "react",
        label: "React",
        subjects: "components & hooks",
        kinds: &["react_component", "react_hook_usage"],
        languages: &["javascript", "typescript", "tsx", "jsx"],
    },
    DetectorSpec {
        id: "spring",
        label: "Spring",
        subjects: "container-managed beans",
        kinds: &["spring_container_managed"],
        languages: &["java"],
    },
];

/// Reader-facing name for a `files.language` label (VISION: labels speak the
/// reader's language). An unknown label is returned verbatim — never fabricated,
/// never dropped.
pub fn reader_language(lang: &str) -> String {
    match lang {
        "javascript" => "JavaScript",
        "typescript" => "TypeScript",
        "tsx" => "TypeScript (TSX)",
        "jsx" => "JavaScript (JSX)",
        "java" => "Java",
        "python" => "Python",
        "c" => "C",
        "cpp" | "c++" | "cxx" => "C++",
        "go" => "Go",
        "rust" => "Rust",
        "ruby" => "Ruby",
        "php" => "PHP",
        "csharp" | "c#" => "C#",
        other => return other.to_string(),
    }
    .to_string()
}

/// Join reader-language names for a language set, deterministically (sorted set
/// input → sorted output). Empty → `"(none)"`.
fn reader_language_list(langs: &BTreeSet<String>) -> String {
    if langs.is_empty() {
        return "(none)".to_string();
    }
    langs
        .iter()
        .map(|l| reader_language(l))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Does this detector's language appear in the snapshot's language mix? A FACT —
/// static catalog ∩ languages read from the DB. (NOT a claim that it executed.)
fn applies(spec: &DetectorSpec, langs: &BTreeSet<String>) -> bool {
    spec.languages.iter().any(|l| langs.contains(*l))
}

/// Languages present in the snapshot that NO shipping inference detector covers.
/// Derived from the static CATALOG ∩ the snapshot's language mix — this module is the
/// ONE place that knows which languages have a detector, so callers (the `inferences`
/// empty-state and the `dead_causes` handler) never re-encode that catalog and cannot
/// re-rot it independently. Verified from `CATALOG`, never from a name.
pub fn uncovered_languages(langs: &BTreeSet<String>) -> BTreeSet<String> {
    langs
        .iter()
        .filter(|l| !CATALOG.iter().any(|s| s.languages.contains(&l.as_str())))
        .cloned()
        .collect()
}

/// The honest "no inference detector on this build covers X" clause for a non-empty
/// uncovered-language set, in reader languages. `None` when nothing is uncovered — so a
/// caller renders the line ONLY when a real detector gap exists on the reader's snapshot.
pub fn no_detector_note(uncovered: &BTreeSet<String>) -> Option<String> {
    if uncovered.is_empty() {
        return None;
    }
    Some(format!(
        "No inference detector on this build covers {}.",
        reader_language_list(uncovered)
    ))
}

/// Parse the source FILE path from a `target_stable_key`. Handles both inference
/// key shapes: `{repo}:{path}#{sym}:SYMBOL:{kind}` (path is before `#`) and
/// `{repo}:{path}:FILE` (path is before the trailing `:FILE`). `repo_uid` carries no
/// `:` (it is `repo_<ulid>`), so the first `:` split strips it. Returns `None` only
/// when the key has no path segment (malformed) — never a fabricated path.
fn file_from_stable_key(key: &str) -> Option<String> {
    let after_repo = key.split_once(':')?.1;
    let path = match after_repo.split_once('#') {
        Some((p, _)) => p,                                              // SYMBOL shape
        None => after_repo.strip_suffix(":FILE").unwrap_or(after_repo), // FILE shape
    };
    if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    }
}

/// The source line for a record, from the value payload's `line_start` WHERE the
/// detector recorded one. `None` (not `0`) when absent — Spring beans have no line.
fn line_from_value(value: &serde_json::Value) -> Option<u64> {
    value.get("line_start").and_then(|v| v.as_u64())
}

/// Build the `detectors` array. `per_kind` maps inference kind → TRUE (unfiltered)
/// row count for the snapshot. Each entry states, for a detector that ships in this
/// build: whether it is `applicable` (its language is in the snapshot's mix — a fact)
/// and its produced `count` (a fact). No `ran` claim is made (execution is not
/// recorded); the caller phrases the applicable-but-zero case as derived.
pub fn build_detectors(
    langs: &BTreeSet<String>,
    per_kind: &BTreeMap<String, u64>,
) -> Vec<serde_json::Value> {
    CATALOG
        .iter()
        .map(|spec| {
            let count: u64 = spec
                .kinds
                .iter()
                .map(|k| per_kind.get(*k).copied().unwrap_or(0))
                .sum();
            serde_json::json!({
                "detector": spec.id,
                "label": spec.label,
                "subjects": spec.subjects,
                "kinds": spec.kinds,
                "languages": spec.languages,
                "applicable": applies(spec, langs),
                "count": count,
            })
        })
        .collect()
}

/// Build the `empty` object for a snapshot that genuinely holds ZERO inferences.
/// Distinguishes "a detector applies to these languages but recorded nothing"
/// (derived applicability) from "no inference detector exists for these languages on
/// this build" (a fact about the build) — in the reader's language.
///
/// Only called when the snapshot's TRUE total is 0.
pub fn empty_state(langs: &BTreeSet<String>) -> serde_json::Value {
    let uncovered = uncovered_languages(langs);
    let covered: BTreeSet<String> = langs.difference(&uncovered).cloned().collect();

    let applicable_labels: Vec<&str> = CATALOG
        .iter()
        .filter(|s| applies(s, langs))
        .map(|s| s.label)
        .collect();

    if covered.is_empty() {
        // No inference detector on this build covers any language in the snapshot.
        let message = format!(
            "No inference detector on this build covers {} \
             (this build's inference detectors: React → JS/TS, Spring → Java).",
            reader_language_list(langs)
        );
        serde_json::json!({
            "reason": "no_detector_for_languages",
            "covered_languages": Vec::<String>::new(),
            "uncovered_languages": uncovered.iter().cloned().collect::<Vec<_>>(),
            "message": message,
        })
    } else {
        // A detector's language IS present, but the snapshot has zero inferences.
        // Applicability is DERIVED from the language mix; the zero is a fact. We do
        // NOT assert the detector "ran".
        let mut message = format!(
            "{} applies to this snapshot's {} files (by file language) \
             but recorded no inferences.",
            applicable_labels.join(" and "),
            reader_language_list(&covered),
        );
        if let Some(note) = no_detector_note(&uncovered) {
            message.push(' ');
            message.push_str(&note);
        }
        serde_json::json!({
            "reason": "applicable_detectors_recorded_nothing",
            "covered_languages": covered.iter().cloned().collect::<Vec<_>>(),
            "uncovered_languages": uncovered.iter().cloned().collect::<Vec<_>>(),
            "message": message,
        })
    }
}

/// Build the `empty` object for the benign "the `--kind` filter matched nothing, but
/// the snapshot DOES hold inferences of other kinds" case. NOT the language-empty
/// honesty line — the detector inventory (above it) still shows the real counts.
fn filter_empty_state(kind: &str, snapshot_total: u64) -> serde_json::Value {
    serde_json::json!({
        "reason": "no_records_for_kind_filter",
        "message": format!(
            "No '{kind}' inferences in this snapshot ({snapshot_total} inference(s) \
             of other kinds — see the detector inventory above)."
        ),
    })
}

/// Map one storage row to its response record, projecting the additive `file`/`line`
/// location and surfacing a malformed `value_json` (never silently `null`).
fn record_json(i: InferenceListRow) -> serde_json::Value {
    let (value, value_error) = match serde_json::from_str::<serde_json::Value>(&i.value_json) {
        Ok(v) => (v, None),
        Err(e) => (serde_json::Value::Null, Some(e.to_string())),
    };
    let file = file_from_stable_key(&i.target_stable_key);
    let line = line_from_value(&value);

    let mut record = serde_json::json!({
        "inference_uid": i.inference_uid,
        "target_stable_key": i.target_stable_key,
        "kind": i.kind,
        "file": file,
        "line": line,
        "value": value,
        "confidence": i.confidence,
        "extractor": i.extractor,
        "created_at": i.created_at,
    });
    if let Some(err) = value_error {
        if let serde_json::Value::Object(ref mut map) = record {
            map.insert("value_error".to_string(), serde_json::json!(err));
        }
    }
    record
}

/// Assemble the full `inferences_list` response Value from the faithful (complete,
/// UNFILTERED) row set plus the snapshot's languages.
///
/// - The detector inventory is built from the UNFILTERED per-kind totals, so a
///   `--kind` filter never changes what a detector is reported to have produced
///   (operator ruling §2 — a filter changes what is SHOWN, never what happened).
/// - `count` is the TRUE total of records MATCHING the query (kind filter applied);
///   `limit` caps only the records carried in THIS payload; `returned`/`truncated`
///   declare that cap so it is never silent.
///
/// Keeping this here (not in the >500-line `dispatch.rs`) is the
/// net-neutral-or-negative constraint on the handler.
pub fn build_response(
    repo_uid: &str,
    snapshot_uid: &str,
    inferences: Vec<InferenceListRow>,
    langs: &BTreeSet<String>,
    kind_filter: Option<&str>,
    limit: Option<u64>,
) -> serde_json::Value {
    // Detector inventory: from the UNFILTERED set (a filter must not corrupt it).
    let snapshot_total = inferences.len() as u64;
    let mut per_kind: BTreeMap<String, u64> = BTreeMap::new();
    for i in &inferences {
        *per_kind.entry(i.kind.clone()).or_insert(0) += 1;
    }
    let detectors = build_detectors(langs, &per_kind);

    // Records MATCHING the query (kind filter applied in memory; the storage read is
    // unfiltered so the inventory above stays faithful).
    let matching: Vec<InferenceListRow> = match kind_filter {
        Some(k) => inferences.into_iter().filter(|i| i.kind == k).collect(),
        None => inferences,
    };
    let count = matching.len() as u64;

    let all_records: Vec<serde_json::Value> = matching.into_iter().map(record_json).collect();
    let results: Vec<serde_json::Value> = match limit {
        Some(n) => all_records.into_iter().take(n as usize).collect(),
        None => all_records,
    };
    let returned = results.len() as u64;
    let truncated = returned < count;

    // Empty explanation: only the true-zero snapshot gets the language honesty line;
    // a filter that matched nothing (but the snapshot is non-empty) gets the benign
    // filter note.
    let empty = if count == 0 {
        if snapshot_total == 0 {
            empty_state(langs)
        } else if let Some(k) = kind_filter {
            filter_empty_state(k, snapshot_total)
        } else {
            serde_json::Value::Null
        }
    } else {
        serde_json::Value::Null
    };

    let mut response = serde_json::json!({
        "command": "inferences list",
        "repo": repo_uid,
        "snapshot": snapshot_uid,
        "languages": langs.iter().cloned().collect::<Vec<_>>(),
        "detectors": detectors,
        "count": count,
        "returned": returned,
        "truncated": truncated,
        "limit": limit,
        "empty": empty,
        "results": results,
    });
    if let Some(k) = kind_filter {
        if let serde_json::Value::Object(ref mut map) = response {
            map.insert("filter_kind".to_string(), serde_json::json!(k));
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn langs(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn kinds(items: &[(&str, u64)]) -> BTreeMap<String, u64> {
        items.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn row(uid: &str, key: &str, kind: &str, value_json: &str) -> InferenceListRow {
        InferenceListRow {
            inference_uid: uid.to_string(),
            target_stable_key: key.to_string(),
            kind: kind.to_string(),
            value_json: value_json.to_string(),
            confidence: 0.9,
            extractor: "test:1.0".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn detectors_report_applicable_and_true_counts() {
        // glamCRM-shaped: java + js/ts, react + spring both produced rows.
        let l = langs(&["java", "javascript", "tsx", "typescript"]);
        let pk = kinds(&[
            ("react_hook_usage", 532),
            ("react_component", 97),
            ("spring_container_managed", 123),
        ]);
        let d = build_detectors(&l, &pk);
        let react = &d[0];
        assert_eq!(react["detector"], "react");
        assert_eq!(react["applicable"], true);
        assert_eq!(react["count"], 629); // 532 + 97 — the TRUE per-detector total
        assert!(react.get("ran").is_none(), "no unverifiable `ran` claim");
        let spring = &d[1];
        assert_eq!(spring["applicable"], true);
        assert_eq!(spring["count"], 123);
    }

    #[test]
    fn detector_not_applicable_when_language_absent() {
        // Pure C/C++ repo: neither detector applies.
        let l = langs(&["c", "cpp"]);
        let d = build_detectors(&l, &BTreeMap::new());
        assert_eq!(d[0]["applicable"], false, "react not applicable");
        assert_eq!(d[0]["count"], 0);
        assert_eq!(d[1]["applicable"], false, "spring not applicable");
    }

    #[test]
    fn empty_no_detector_for_language_names_the_reader_language() {
        // leveldb: C/C++ only — no inference detector on this build.
        let e = empty_state(&langs(&["c", "cpp"]));
        assert_eq!(e["reason"], "no_detector_for_languages");
        let msg = e["message"].as_str().unwrap();
        assert!(
            msg.contains("C") && msg.contains("C++"),
            "names langs: {msg}"
        );
        assert!(
            msg.contains("No inference detector on this build covers"),
            "honest line: {msg}"
        );
    }

    #[test]
    fn empty_applicable_but_nothing_recorded_is_phrased_as_derived() {
        // A JS/TS repo with react in scope but zero detections.
        let e = empty_state(&langs(&["typescript"]));
        assert_eq!(e["reason"], "applicable_detectors_recorded_nothing");
        let msg = e["message"].as_str().unwrap();
        assert!(msg.contains("React"), "names the detector: {msg}");
        assert!(msg.contains("TypeScript"), "reader language: {msg}");
        assert!(
            msg.contains("by file language"),
            "applicability marked as derived, not observed: {msg}"
        );
        assert!(
            !msg.contains("ran"),
            "no unverifiable execution claim: {msg}"
        );
    }

    #[test]
    fn empty_python_only_is_no_detector_for_language() {
        // django: python-only → no detector.
        let e = empty_state(&langs(&["python"]));
        assert_eq!(e["reason"], "no_detector_for_languages");
        assert!(e["message"].as_str().unwrap().contains("Python"));
    }

    #[test]
    fn uncovered_languages_names_only_languages_with_no_detector() {
        // Mixed snapshot: java (Spring), typescript (React) both covered; c/cpp not.
        let u = uncovered_languages(&langs(&["java", "typescript", "c", "cpp"]));
        assert_eq!(u, langs(&["c", "cpp"]), "only the detector-less languages");
        // Fully covered → empty.
        assert!(uncovered_languages(&langs(&["java", "tsx"])).is_empty());
        // Fully uncovered → all.
        assert_eq!(
            uncovered_languages(&langs(&["go", "ruby"])),
            langs(&["go", "ruby"])
        );
    }

    #[test]
    fn no_detector_note_present_only_for_a_real_gap_in_reader_language() {
        assert_eq!(no_detector_note(&BTreeSet::new()), None, "no gap → no line");
        let note = no_detector_note(&langs(&["c", "cpp"])).expect("gap → a line");
        assert!(
            note.contains("C") && note.contains("C++"),
            "reader langs: {note}"
        );
        assert!(
            note.starts_with("No inference detector on this build covers"),
            "honest phrasing: {note}"
        );
    }

    #[test]
    fn reader_language_passes_through_unknown_verbatim() {
        assert_eq!(reader_language("kotlin"), "kotlin");
        assert_eq!(reader_language("java"), "Java");
    }

    #[test]
    fn file_and_line_project_from_symbol_and_file_key_shapes() {
        // SYMBOL shape (react component / spring bean): path before '#', line from value.
        let comp = record_json(row(
            "inf-1",
            "repo_01abc:web/src/App.tsx#Widget:SYMBOL:FUNCTION",
            "react_component",
            r#"{"component_name":"Widget","line_start":42}"#,
        ));
        assert_eq!(comp["file"], "web/src/App.tsx");
        assert_eq!(comp["line"], 42);

        // FILE shape (file-level hook, no caller): path before ':FILE', line present.
        let hook = record_json(row(
            "inf-2",
            "repo_01abc:web/src/util.ts:FILE",
            "react_hook_usage",
            r#"{"hook_name":"useEffect","line_start":7}"#,
        ));
        assert_eq!(
            hook["file"], "web/src/util.ts",
            "FILE-shape path must render"
        );
        assert_eq!(hook["line"], 7);
    }

    #[test]
    fn spring_row_has_file_but_no_line_never_fabricates_zero() {
        // Spring bean value carries no line_start → line is null, not 0.
        let bean = record_json(row(
            "inf-3",
            "repo_01abc:src/main/java/App.java#AppConfig:SYMBOL:CLASS",
            "spring_container_managed",
            r#"{"annotation":"@Configuration","reason":"stereotype"}"#,
        ));
        assert_eq!(bean["file"], "src/main/java/App.java");
        assert_eq!(bean["line"], serde_json::Value::Null, "no fabricated 0");
    }

    #[test]
    fn malformed_value_json_is_surfaced_not_silently_null() {
        let bad = record_json(row(
            "inf-4",
            "repo_01abc:a.tsx#X:SYMBOL:FUNCTION",
            "react_component",
            "{not json",
        ));
        assert_eq!(bad["value"], serde_json::Value::Null);
        assert!(
            bad.get("value_error").and_then(|v| v.as_str()).is_some(),
            "malformed value_json carries an explicit error: {bad}"
        );
    }

    #[test]
    fn kind_filter_does_not_corrupt_detector_inventory() {
        // Snapshot holds both react + spring; filter to spring. The react detector
        // must STILL report its produced count (operator ruling §2).
        let rows = vec![
            row(
                "r1",
                "repo_x:a.tsx#A:SYMBOL:FUNCTION",
                "react_component",
                r#"{"component_name":"A","line_start":1}"#,
            ),
            row(
                "r2",
                "repo_x:b.tsx#B:SYMBOL:FUNCTION",
                "react_component",
                r#"{"component_name":"B","line_start":2}"#,
            ),
            row(
                "s1",
                "repo_x:App.java#Cfg:SYMBOL:CLASS",
                "spring_container_managed",
                r#"{"annotation":"@Configuration"}"#,
            ),
        ];
        let resp = build_response(
            "repo_x",
            "snap_1",
            rows,
            &langs(&["java", "tsx"]),
            Some("spring_container_managed"),
            None,
        );
        // count reflects the FILTER (1 spring row)...
        assert_eq!(resp["count"], 1);
        assert_eq!(resp["returned"], 1);
        // ...but the react detector's produced count is UNFILTERED (2), not 0.
        let dets = resp["detectors"].as_array().unwrap();
        let react = dets.iter().find(|d| d["detector"] == "react").unwrap();
        assert_eq!(react["count"], 2, "filter must not zero out react's count");
    }

    #[test]
    fn limit_caps_records_and_declares_truncation_with_true_total() {
        let rows: Vec<InferenceListRow> = (0..5)
            .map(|i| {
                row(
                    &format!("r{i}"),
                    &format!("repo_x:a.tsx#C{i}:SYMBOL:FUNCTION"),
                    "react_component",
                    &format!(r#"{{"component_name":"C{i}","line_start":{i}}}"#),
                )
            })
            .collect();
        let resp = build_response("repo_x", "snap", rows, &langs(&["tsx"]), None, Some(2));
        assert_eq!(resp["count"], 5, "true total");
        assert_eq!(resp["returned"], 2, "rows in this payload");
        assert_eq!(resp["truncated"], true);
        assert_eq!(resp["limit"], 2);
        assert_eq!(resp["results"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn kind_filter_with_no_matches_but_nonempty_snapshot_is_benign_note() {
        let rows = vec![row(
            "r1",
            "repo_x:a.tsx#A:SYMBOL:FUNCTION",
            "react_component",
            r#"{"component_name":"A","line_start":1}"#,
        )];
        let resp = build_response(
            "repo_x",
            "snap",
            rows,
            &langs(&["tsx"]),
            Some("spring_container_managed"),
            None,
        );
        assert_eq!(resp["count"], 0);
        assert_eq!(resp["empty"]["reason"], "no_records_for_kind_filter");
    }
}
