//! DOCS-LIST-2 §2.4 — the DOCTOR enrichment-skip materiality gate (crate-private).
//!
//! (Abstraction one-liner — **what:** turn the fallible per-language breakdown read + the raw
//! toolchain skips into the doctor's honest per-language remediation list; **concrete current user:**
//! `enrich_pass::run_enrich_pass` (the doctor report builder) + this module's tests; **axis of
//! variation:** the resolver-path capability — REUSED from `reader_context::material_code_languages` /
//! `token_enrichment_language` so doctor and the CTA read ONE materiality definition; **rejected
//! simpler:** inline the filter in `enrich_pass` — rejected because it re-derives the materiality gate
//! away from its one home and hides the read-failure branch from a pure unit test. Extracted from
//! `reader_context` per review-1 item 2 — the guardrail, and so the honest-degradation branch has a
//! testable seam `enrich_pass` itself can only reach through a live daemon + LSP toolchain.)

use crate::enrich_pass::{language_token, SkippedLanguage};
use crate::reader_context::{material_code_languages, token_enrichment_language};

/// DOCS-LIST-2 §2.4 (review-0 F3): turn the FALLIBLE per-language breakdown read into the doctor
/// enrichment-skip list — the ONE place the read RESULT is classified, so success AND read-failure
/// both render honestly, and doctor never prescribes another ecosystem's remedy:
///
/// - `Ok(counts)`: gate the raw `; skipped <lang>: <remedy>` lines through the SAME per-language
///   capability logic the CTA (`relationship_next_action_line`) uses. Keep only the skips of a
///   MATERIAL (≥10% of code files) AND enrichable language — so django's ~3.7% JavaScript never
///   surfaces `npm i -D typescript` on a materially-Python repo. When ≥1 code language is materially
///   present and NONE of it is enrichable on ANY build (the SAME `token_enrichment_language(..).is_none()`
///   fact `call_graph_ceiling_languages` uses — C / C++ / Python / …), append the dominant material
///   language's reader-frame no-semantic-path sentence in place of any cross-ecosystem remedy. Java
///   (resolver exists but JDTLS-gated) is enrichable-on-some-build, so a material-Java repo keeps its
///   own remedy instead of the no-path sentence.
/// - `Err(reason)` with ≥1 raw skip: materiality is UNKNOWN, so a concrete per-language remedy CANNOT
///   be shown honestly (an incidental TypeScript skip would otherwise leak `npm i -D typescript` into a
///   Python-dominant repo — the exact review-0 F3 defect). The raw remedies are REPLACED by ONE
///   unknown-with-reason skip carrying the read-failure reason. NEVER silently keeps the ungated
///   remedies (STANDING HONESTY RULE #1: a classified fallible read is unknown-with-reason, never
///   `.ok()`-collapsed). Mirrors `relationship_next_action_line_or_read_error`'s Err posture.
/// - `Err(_)` with NO raw skip: there was no remedy to classify, so unknown-with-reason would fabricate
///   a "remedy unavailable" line for a remedy that never existed (an all-runnable / no-eligible-edge
///   pass). The result is empty — unknown-with-reason is retained ONLY when an actual raw remedy would
///   otherwise be rendered (review-3 finding 3).
pub(crate) fn gated_enrichment_skips(
    raw_skipped: &[SkippedLanguage],
    language_counts: Result<Vec<(String, u64)>, String>,
) -> Vec<SkippedLanguage> {
    let counts = match language_counts {
        Ok(counts) => counts,
        // Read failed → materiality unknown → cannot honestly show a per-language remedy.
        // BUT unknown-with-reason is a CLASSIFICATION of an actual raw remedy: with no raw skip to
        // classify (an all-runnable / no-eligible-edge pass), there is nothing to gate, so emitting
        // "per-language enrichment remedy unavailable" would fabricate a remedy that never existed
        // (review-3 finding 3). Only surface the unknown skip when a raw remedy would otherwise be
        // rendered; an empty raw list stays empty.
        Err(_) if raw_skipped.is_empty() => return Vec::new(),
        Err(reason) => {
            return vec![SkippedLanguage {
                language: "unknown".to_string(),
                reason: format!(
                    "per-language enrichment remedy unavailable — could not read the language \
                     breakdown ({reason})"
                ),
            }]
        }
    };

    let material = material_code_languages(&counts);

    // The enrichment-language tokens (matching `SkippedLanguage.language`) whose MATERIAL skip remedy
    // survives — a material language that IS enrichable on some build.
    let mut keep_tokens: Vec<&'static str> = material
        .iter()
        .filter_map(|m| token_enrichment_language(&m.token).map(language_token))
        .collect();
    keep_tokens.sort_unstable();
    keep_tokens.dedup();

    let mut kept: Vec<SkippedLanguage> = raw_skipped
        .iter()
        .filter(|s| keep_tokens.contains(&s.language.as_str()))
        .cloned()
        .collect();

    // No-path sentence: ≥1 material code language, and NONE enrichable on any build.
    if !material.is_empty()
        && material
            .iter()
            .all(|m| token_enrichment_language(&m.token).is_none())
    {
        let dominant = material[0].display; // count-DESC → plurality
        kept.push(SkippedLanguage {
            language: dominant.to_string(),
            reason: format!("no semantic-resolution path exists for {dominant} on this build"),
        });
    }

    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skip(lang: &str, reason: &str) -> SkippedLanguage {
        SkippedLanguage {
            language: lang.to_string(),
            reason: reason.to_string(),
        }
    }

    #[test]
    fn ok_python_dominant_drops_npm_and_states_no_path() {
        // django shape: 2904 Python / 111 JavaScript (~3.7%) + a raw TS skip carrying npm advice. On a
        // materially-Python repo the immaterial TS skip is DROPPED (no npm), and Python's
        // no-semantic-path sentence is added — never another ecosystem's remedy.
        let raw = vec![skip(
            "typescript",
            "tsserver not found — install typescript so tsserver is on PATH (npm i -g typescript)",
        )];
        let out = gated_enrichment_skips(
            &raw,
            Ok(vec![
                ("python".to_string(), 2904),
                ("javascript".to_string(), 111),
            ]),
        );
        assert!(
            !out.iter().any(|s| s.reason.contains("npm")),
            "npm advice dropped on a Python repo: {out:?}"
        );
        assert!(
            out.iter().any(|s| s.language == "Python"
                && s.reason
                    .contains("no semantic-resolution path exists for Python")),
            "Python no-path sentence present: {out:?}"
        );
    }

    #[test]
    fn ok_mixed_ts_java_keeps_both_and_no_no_path() {
        // glamCRM shape: TS ~90% + Java ~10% — both material AND enrichable-on-some-build, so each
        // keeps its OWN remedy (named per language), and there is no no-path sentence.
        let raw = vec![
            skip("typescript", "tsserver not found — npm i -D typescript"),
            skip("java", "jdtls not found — set JDTLS_PATH"),
        ];
        let out = gated_enrichment_skips(
            &raw,
            Ok(vec![
                ("typescript".to_string(), 900),
                ("java".to_string(), 100),
            ]),
        );
        assert!(out.iter().any(|s| s.language == "typescript"), "{out:?}");
        assert!(out.iter().any(|s| s.language == "java"), "{out:?}");
        assert!(
            !out.iter()
                .any(|s| s.reason.contains("no semantic-resolution path")),
            "an enrichable material language exists → no no-path: {out:?}"
        );
    }

    #[test]
    fn ok_keeps_material_language_remedy() {
        // A material TS skip survives the gate on a TS-dominant repo (the true remedy is kept).
        let raw = vec![skip(
            "typescript",
            "tsserver not found — npm i -D typescript there",
        )];
        let out = gated_enrichment_skips(&raw, Ok(vec![("typescript".to_string(), 500)]));
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].language, "typescript");
    }

    #[test]
    fn read_failure_renders_unknown_with_reason_never_npm() {
        // review-0 F3: the language breakdown READ FAILED. The ungated raw remedies (incl. npm advice)
        // must NOT survive — they would leak `npm` into a repo whose materiality we cannot verify.
        // Exactly ONE unknown-with-reason skip is rendered, carrying the read-failure reason.
        let raw = vec![
            skip("typescript", "tsserver not found — npm i -g typescript"),
            skip("java", "jdtls not found — set JDTLS_PATH"),
        ];
        let out = gated_enrichment_skips(&raw, Err("db locked".to_string()));
        assert_eq!(
            out.len(),
            1,
            "collapsed to one unknown-with-reason: {out:?}"
        );
        assert_eq!(out[0].language, "unknown");
        assert!(
            out[0]
                .reason
                .contains("per-language enrichment remedy unavailable")
                && out[0].reason.contains("db locked"),
            "unknown-with-reason carries the read-failure reason: {out:?}"
        );
        assert!(
            !out.iter().any(|s| s.reason.contains("npm")),
            "a failed read must never fabricate/keep another ecosystem's remedy: {out:?}"
        );
    }

    #[test]
    fn read_failure_with_no_raw_skips_is_empty() {
        // review-3 finding 3: an all-runnable / no-eligible-edge pass has NO raw remedy to classify.
        // A breakdown read failure must then produce an EMPTY list — not a fabricated "remedy
        // unavailable" line for a remedy that never existed. unknown-with-reason is retained only
        // when a raw remedy would otherwise be rendered.
        let out = gated_enrichment_skips(&[], Err("db locked".to_string()));
        assert!(
            out.is_empty(),
            "no raw remedy + read failure → empty, no fabricated unknown line: {out:?}"
        );
    }
}
