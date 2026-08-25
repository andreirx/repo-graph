//! `storage_health`-derived doctor "Semantic seeding" probe (EMBED-SEED-IMPL-1, spec §9).
//!
//! **Abstraction note (per repo structural guardrail):** extracted from `doctor/mod.rs`
//! because the seed classifier + human-print block pushed that file past the 500-line
//! guardrail (the same reason `daemon_info.rs` / `storage_probe.rs` were split, review-6 #2).
//! Two concrete current callers, both in `super::`: [`super::storage_summary_probes`] pushes
//! [`semantic_seeding_from_facts`]; [`super::print_human_output`] calls [`print_seed_section`].
//! Axis of variation: none claimed — a cohesion/size split. `ProbeResult`/`ProbeOutput`/
//! `DoctorOutput`/`print_probe_labeled` are shared from the parent via `super::`.

use crate::platform::ProbeResult;

use super::{print_probe_labeled, DoctorOutput};

/// EMBED-SEED-IMPL-1 (spec §9): the doctor "Semantic seeding" fact — store state,
/// model pin (+ identity provenance), staleness count — read from the additive
/// `seed` block on the `storage_health` response.
///
/// ALWAYS present (review-2 #4): an absent `seed` block renders an explicit
/// unavailable row, never a silent omission (which would read as "no seeding
/// concern"). Seeding is optional, so no state fails doctor. Every field is our
/// own daemon DTO, so a genuinely-absent REQUIRED field means a MALFORMED /
/// old-daemon block — rendered as unknown-with-reason, NEVER a fabricated identity
/// ("operator-asserted"), count, or dim (STANDING HONESTY RULE — review-2 #3).
pub(super) fn semantic_seeding_from_facts(response: &serde_json::Value) -> ProbeResult {
    let unavailable = |msg: String| ProbeResult {
        name: "semantic_seeding".to_string(),
        passed: true, // seeding is optional — its absence never fails doctor
        message: msg,
        details: None,
    };

    let Some(seed) = response.get("seed") else {
        return unavailable(
            "unavailable (daemon did not report semantic-seeding facts)".to_string(),
        );
    };
    let state = seed.get("state").and_then(|v| v.as_str());

    // The minimal unavailable block carries only state + reason (daemon busy / repo
    // won't load / storage won't open / serialization failed).
    if state == Some("unavailable") {
        // `unavailable_reason` is a REQUIRED field of the daemon's unavailable block
        // (`handlers::metrics::seed_unavailable` always sets it). A genuinely-absent
        // reason means a MALFORMED / old-daemon block — surfaced as such, NOT
        // defaulted to a fabricated "(no reason reported)" (STANDING HONESTY RULE —
        // review-4 #1).
        return match seed.get("unavailable_reason").and_then(|v| v.as_str()) {
            Some(reason) => unavailable(format!("unavailable: {reason}")),
            None => unavailable(
                "malformed seed block (unavailable state without a reason — old daemon or serialization bug)"
                    .to_string(),
            ),
        };
    }

    // A full facts block: state + the required model pin trio. Absent ⇒ malformed
    // (never defaulted). `model_identity` in particular is NEVER defaulted to
    // "operator-asserted" — that is a verified-vs-not claim the daemon must supply.
    let (Some(state), Some(model), Some(dim), Some(identity)) = (
        state,
        seed.get("model_id").and_then(|v| v.as_str()),
        seed.get("dim").and_then(|v| v.as_u64()),
        seed.get("model_identity").and_then(|v| v.as_str()),
    ) else {
        return unavailable(
            "malformed seed block (missing state/model_id/dim/model_identity — old daemon or serialization bug)"
                .to_string(),
        );
    };

    let stale = seed.get("stale_count").and_then(|v| v.as_u64());
    let total = seed.get("total").and_then(|v| v.as_u64());
    // Only report a staleness fraction when BOTH counts are genuinely present;
    // otherwise say so — never invent "0 of 0".
    let staleness = match (stale, total) {
        (Some(s), Some(t)) => format!("{s} of {t} file(s) changed since embed"),
        _ => "staleness unavailable (daemon did not report counts)".to_string(),
    };
    let degraded = seed.get("degraded_reason").and_then(|v| v.as_str());

    let (message, details) = match state {
        "present" => (
            format!("present ({dim}-dim, model {model})"),
            Some(format!("model id {identity}; {staleness}")),
        ),
        "building" => (
            format!("building — a background embed pass is running ({dim}-dim, model {model})"),
            Some(format!("model id {identity}; {staleness}")),
        ),
        "absent" => (
            "not built yet (builds in the background after indexing)".to_string(),
            Some(format!("model id {identity}")),
        ),
        // `degraded_reason` is REQUIRED whenever state == "degraded" (the daemon's
        // `seed_doctor_facts` always sets `Some`). A genuinely-absent reason is a
        // MALFORMED / old-daemon block — NOT defaulted to a fabricated "unknown
        // cause" (STANDING HONESTY RULE — review-4 #1).
        "degraded" => match degraded {
            Some(reason) => (
                format!("degraded: {reason}"),
                Some(format!("model {model} ({identity})")),
            ),
            None => (
                "malformed seed block (degraded state without a reason — old daemon or serialization bug)"
                    .to_string(),
                Some(format!("model {model} ({identity})")),
            ),
        },
        other => (
            format!("state: {other}"),
            Some(format!("model {model} ({identity})")),
        ),
    };
    ProbeResult {
        name: "semantic_seeding".to_string(),
        passed: true,
        message,
        details,
    }
}

/// Print the human-mode "Semantic seeding" section (spec §9). A probe not listed in a
/// section filter is silently dropped from human output, so the seed probe is named
/// here explicitly. Empty ⇒ nothing printed.
pub(super) fn print_seed_section(output: &DoctorOutput) {
    let seed_probes: Vec<_> = output
        .probes
        .iter()
        .filter(|p| p.name.as_str() == "semantic_seeding")
        .collect();
    if !seed_probes.is_empty() {
        println!("Semantic seeding:");
        for probe in &seed_probes {
            print_probe_labeled(probe, "vector store");
        }
        println!();
    }
}
