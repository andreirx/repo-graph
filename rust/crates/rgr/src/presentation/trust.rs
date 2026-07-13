//! Presentation layer for the `trust` command.
//!
//! # CLI-OUT-2B → TRUST-LIVEGRAPH-IMPL
//!
//! The daemon now returns the ratified HYBRID `CoherenceEnvelope<CoherentTrustReport>` (the wrapper is the
//! top level). The `--json` path prints it verbatim; this human path projects it. The render is the honest
//! two-half model (`docs/slices/trust-livegraph-1.md` §3e / W2):
//!   - a NEW **Current-State Posture** section (Half A, `source = livegraph`) — residency / per-partition
//!     freshness / language / producer / migrated-answer capability, GENUINELY served from the LiveGraph;
//!   - the existing v1 sections (Half B, `source = sqlite`) with their bullet text BYTE-IDENTICAL to the
//!     pre-wrapper report, each heading carrying a `(source, snapshot-scoped extraction, freshness)` label so
//!     a reader never mistakes the OUTGOING-extractor snapshot diagnostics for the current-state LiveGraph
//!     resolution (F5);
//!   - an overall **Posture** line carrying the root MEET (current-state-vs-snapshot freshness).
//!
//! The renderer reuses the trust crate's coherent DTOs (`repo_graph_trust::CoherentTrustReport`) for
//! deserialization so the CLI view cannot silently drift from the daemon's wire shape.
//!
//! ## Human Output Structure
//!
//! ```text
//! Trust Report: billing-service
//! Snapshot: snap_01kr...
//! Posture: Exact (Fresh)
//!
//! Current-State Posture  (livegraph, current-state, Fresh)
//!   - Resident: yes (1 partition)
//!   - app: Fresh, TypeScript, producer scip-typescript@0.4.0
//!   - Producer available: yes
//!   - Migrated-answer capability: yes
//!
//! Resolution  (sqlite, snapshot-scoped extraction, Fresh)
//!   - Calls: 78% resolved (1234 of 1582)
//!   - Edges: 95% resolved (2100 of 2210)
//!
//! Reliability  (sqlite, snapshot-scoped extraction, Fresh)
//!   - Call-graph: LOW (unresolved calls exceed threshold)
//!   ...
//! ```

use repo_graph_coherence::{AnswerClass, CoherenceEnvelope, FreshnessState, Provenance, Source};
use repo_graph_trust::types::ReliabilityAxisScore;
use repo_graph_trust::{CoherentTrustReport, LiveGraphPosture};

use crate::presentation::{bullet, heading, kv_line};

/// The wire type `rmap trust` deserializes: the daemon's `CoherenceEnvelope<CoherentTrustReport>`.
pub type TrustEnvelope = CoherenceEnvelope<CoherentTrustReport>;

// ── Labels (the per-section source/freshness honesty markers) ───────────────────────────────────

fn freshness_label(f: FreshnessState) -> &'static str {
    match f {
        FreshnessState::Fresh => "Fresh",
        FreshnessState::Stale => "Stale",
        FreshnessState::PrecisionPending => "PrecisionPending",
        FreshnessState::RefreshFailed => "RefreshFailed",
        FreshnessState::Unavailable => "Unavailable",
    }
}

fn class_label(c: AnswerClass) -> &'static str {
    match c {
        AnswerClass::Exact => "Exact",
        AnswerClass::Partial => "Partial",
        AnswerClass::Unavailable => "Unavailable",
        AnswerClass::Stale => "Stale",
    }
}

fn source_label(p: &Provenance) -> String {
    let parts: Vec<&str> = p
        .source
        .iter()
        .map(|s| match s {
            Source::Livegraph => "livegraph",
            Source::Sqlite => "sqlite",
            Source::Filesystem => "filesystem",
            Source::Declaration => "declaration",
        })
        .collect();
    parts.join("+")
}

/// A labelled section heading: `Title  (source, scope, Freshness)`. `scope` is `"snapshot-scoped
/// extraction"` for Half-B residual leaves and `"current-state"` for the Half-A posture leaf.
fn labelled_heading<T>(title: &str, leaf: &CoherenceEnvelope<T>, scope: &str) -> String {
    heading(&format!(
        "{}  ({}, {}, {})",
        title,
        source_label(&leaf.provenance),
        scope,
        freshness_label(leaf.freshness)
    ))
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

// ── Top-level render ─────────────────────────────────────────────────────────────────────────────

/// Render trust's coherence-wrapped daemon response (TRUST-LIVEGRAPH-IMPL).
pub fn render_trust_envelope(env: &TrustEnvelope) -> String {
    let v = &env.value;
    let mut out = String::new();

    // ── Header ──
    let repo_display = v.display_name.as_deref().unwrap_or(&v.snapshot_uid);
    out.push_str(&kv_line("Trust Report", repo_display));
    out.push_str(&kv_line("Snapshot", &truncate_uid(&v.snapshot_uid)));
    // The overall MEET posture (root): the hybrid's honest current-state-vs-snapshot freshness. A Fresh
    // LiveGraph posture over a Stale snapshot reads Stale; a cold LiveGraph reads Unavailable (D-T6).
    out.push_str(&kv_line(
        "Posture",
        &format!(
            "{} ({})",
            class_label(env.trust.class),
            freshness_label(env.freshness)
        ),
    ));
    out.push('\n');

    // ── Half A — Current-State Posture (livegraph) ──
    out.push_str(&render_posture(&v.current_state_posture));
    out.push('\n');

    // ── Half B — residual extraction diagnostics (sqlite, snapshot-scoped) ──
    out.push_str(&render_resolution(v));
    out.push('\n');

    out.push_str(&render_reliability(v));
    out.push('\n');

    let breakdown = render_unresolved_breakdown(v);
    if !breakdown.is_empty() {
        out.push_str(&breakdown);
        out.push('\n');
    }

    let classification = render_classification(v);
    if !classification.is_empty() {
        out.push_str(&classification);
        out.push('\n');
    }

    // EY1-A: the likely-external receiver-call orientation projection (Layer-2, read-side).
    let external_receivers = render_enrichment_external(v);
    if !external_receivers.is_empty() {
        out.push_str(&external_receivers);
        out.push('\n');
    }

    let suspicious = render_suspicious_modules(v);
    if !suspicious.is_empty() {
        out.push_str(&suspicious);
        out.push('\n');
    }

    let downgrades = render_downgrades(v);
    if !downgrades.is_empty() {
        out.push_str(&downgrades);
        out.push('\n');
    }

    let caveats = &v.caveats.value;
    if !caveats.is_empty() {
        out.push_str(&labelled_heading(
            "Caveats",
            &v.caveats,
            "snapshot-scoped extraction",
        ));
        for caveat in caveats {
            out.push_str(&bullet(caveat));
        }
        out.push('\n');
    }

    out.trim_end().to_string()
}

// ── Half A — Current-State Posture (livegraph) ──────────────────────────────────────────────────

fn render_posture(leaf: &CoherenceEnvelope<LiveGraphPosture>) -> String {
    let p = &leaf.value;
    let mut out = labelled_heading("Current-State Posture", leaf, "current-state");

    if !p.resident {
        out.push_str(&bullet(
            "Resident: no (LiveGraph not loaded for this repo — current-state posture unavailable)",
        ));
        return out;
    }

    let n = p.partitions.len();
    out.push_str(&bullet(&format!(
        "Resident: yes ({} partition{})",
        n,
        if n == 1 { "" } else { "s" }
    )));
    for part in &p.partitions {
        let lang = if part.typescript_primary {
            "TypeScript"
        } else {
            "non-TypeScript"
        };
        let producer = if part.producer_fingerprint.is_empty() {
            "(no producer)".to_string()
        } else {
            part.producer_fingerprint.clone()
        };
        out.push_str(&bullet(&format!(
            "{}: {}, {}, producer {}",
            part.partition_id,
            freshness_label(part.freshness),
            lang,
            producer
        )));
    }
    out.push_str(&bullet(&format!(
        "Producer available: {}",
        yes_no(p.producer_available)
    )));
    out.push_str(&bullet(&format!(
        "Migrated-answer capability: {}",
        yes_no(p.migrated_answer_capability)
    )));
    out
}

// ── Half B — residual diagnostics sections (bullet text byte-identical to the v1 report) ────────

fn render_resolution(v: &CoherentTrustReport) -> String {
    // Labelled by the resolution leaf (sqlite, snapshot-scoped). The Edges line carries the v1
    // `edges_resolved == edges_total` quirk verbatim (RISK-T-F).
    let mut out = labelled_heading("Resolution", &v.resolution, "snapshot-scoped extraction");
    let r = &v.resolution.value;

    let total_calls = r.resolved_calls + r.unresolved_calls;
    let call_pct = if total_calls > 0 {
        (r.resolved_calls as f64 / total_calls as f64 * 100.0).round() as u64
    } else {
        100
    };
    out.push_str(&bullet(&format!(
        "Calls: {}% resolved ({} of {})",
        call_pct, r.resolved_calls, total_calls
    )));

    let edge_pct = if r.edges_total > 0 {
        (r.edges_resolved as f64 / r.edges_total as f64 * 100.0).round() as u64
    } else {
        100
    };
    out.push_str(&bullet(&format!(
        "Edges: {}% resolved ({} of {})",
        edge_pct, r.edges_resolved, r.edges_total
    )));

    out
}

fn render_reliability(v: &CoherentTrustReport) -> String {
    let mut out = labelled_heading("Reliability", &v.reliability, "snapshot-scoped extraction");
    let r = &v.reliability.value;

    out.push_str(&bullet(&format_axis("Call-graph", &r.call_graph)));
    out.push_str(&bullet(&format_axis("Import-graph", &r.import_graph)));
    out.push_str(&bullet(&format_axis("Change-impact", &r.change_impact)));

    out
}

fn render_unresolved_breakdown(v: &CoherentTrustReport) -> String {
    let non_zero: Vec<_> = v
        .categories
        .value
        .iter()
        .filter(|c| c.unresolved > 0)
        .collect();
    if non_zero.is_empty() {
        return String::new();
    }
    let mut out = labelled_heading(
        "Unresolved Breakdown",
        &v.categories,
        "snapshot-scoped extraction",
    );
    for cat in non_zero {
        out.push_str(&bullet(&format!("{} {}", cat.unresolved, cat.label)));
    }
    out
}

fn render_classification(v: &CoherentTrustReport) -> String {
    let non_zero: Vec<_> = v
        .classifications
        .value
        .iter()
        .filter(|c| c.count > 0)
        .collect();
    if non_zero.is_empty() {
        return String::new();
    }
    let mut out = labelled_heading(
        "Classification",
        &v.classifications,
        "snapshot-scoped extraction",
    );
    for cls in non_zero {
        out.push_str(&bullet(&format!("{} {}", cls.count, cls.classification)));
    }
    out
}

/// EY1-A (ENRICH-YIELD-2): a Layer-2 read projection over the enrichment metadata already on the
/// trust report — the largest reject class (~36% of enrichment-eligible unknown object-method calls)
/// whose receiver resolved to a *likely-external* type. This is ORIENTATION, not a resolved edge:
/// `<T>` tells the agent "this call goes into a library/std type `<T>` — follow it to that
/// crate/package/docs", converting an unattributed unknown into a place to look (VISION: orientation
/// over oracle). Read-only reprojection of `enrichment_status.top_types` (already computed by the
/// trust service); NO new persisted shape, NO new query — the ratified corrected EY1-A cell.
///
/// Two SEPARATE, independently-labelled basis lines (the ratified corrected EY1-A cell) — the two
/// facts have DISTINCT provenance and must not be conflated into one claim:
///   - receiver-type basis: the type name is inferred from a language-server type hover,
///     heuristically parsed;
///   - external-classification basis: the name matched a static name-set of well-known std/library
///     type names AND language primitives (EY1-B classifies primitives external), NOT
///     compiler-verified.
///
/// Both the Rust (`STD_TYPES` + `PRIMITIVES`) and TS (`NODE_TYPES`/`LIBRARY_TYPES`) resolvers
/// classify externality by static name-set, so the basis is accurate across languages; those
/// constant names are internal and kept off the reader surface (VISION: labels speak the reader's
/// language). Never claims Layer-0 certainty — the ratification rejected promoting these to edges.
fn render_enrichment_external(v: &CoherentTrustReport) -> String {
    // Enrichment never ran → honest absence (no section), never a measured-zero.
    let Some(status) = v.enrichment_status.value.as_ref() else {
        return String::new();
    };
    let external: Vec<_> = status
        .top_types
        .iter()
        .filter(|t| t.is_external && t.count > 0)
        .collect();
    // Enrichment ran but surfaced no external receivers → measured-absent, show nothing.
    if external.is_empty() {
        return String::new();
    }
    let mut out = labelled_heading(
        "Likely-External Receiver Calls",
        &v.enrichment_status,
        "snapshot-scoped extraction",
    );
    // Two SEPARATE, independently-labelled bases (the ratified corrected EY1-A cell): the receiver
    // TYPE and the EXTERNAL classification are distinct heuristics with distinct provenance, so each
    // gets its own basis line — never merged into a single "basis:" claim. Plus the Layer-2 framing:
    // this is orientation, not a resolved edge.
    out.push_str(&bullet(
        "receiver-type basis: inferred from a language-server type hover, heuristically parsed",
    ));
    out.push_str(&bullet(
        "external-classification basis: matched a static name-set of well-known std/library type \
         names and language primitives — not compiler-verified",
    ));
    out.push_str(&bullet(
        "orientation only, not resolved call-graph edges (never a Layer-0 CALLS edge)",
    ));
    for t in external {
        let calls = if t.count == 1 { "call" } else { "calls" };
        out.push_str(&bullet(&format!(
            "call on likely-external receiver `{}` ({} {})",
            t.type_name, t.count, calls
        )));
    }
    out
}

fn render_suspicious_modules(v: &CoherentTrustReport) -> String {
    let suspicious: Vec<_> = v
        .modules
        .value
        .iter()
        .filter(|m| m.suspicious_zero_connectivity)
        .collect();
    if suspicious.is_empty() {
        return String::new();
    }
    let mut out = labelled_heading(
        "Suspicious Modules (zero connectivity)",
        &v.modules,
        "snapshot-scoped extraction",
    );
    for m in suspicious.iter().take(10) {
        out.push_str(&bullet(&m.qualified_name));
    }
    if suspicious.len() > 10 {
        out.push_str(&bullet(&format!("... ({} more)", suspicious.len() - 10)));
    }
    out
}

fn render_downgrades(v: &CoherentTrustReport) -> String {
    let d = &v.triggered_downgrades.value;
    let mut items = Vec::new();

    if d.framework_heavy_suspicion.triggered {
        let reason = d
            .framework_heavy_suspicion
            .reasons
            .first()
            .map(|s| s.as_str())
            .unwrap_or("framework patterns detected");
        items.push(format!("framework_heavy_suspicion: {}", reason));
    }
    if d.registry_pattern_suspicion.triggered {
        let reason = d
            .registry_pattern_suspicion
            .reasons
            .first()
            .map(|s| s.as_str())
            .unwrap_or("registry patterns detected");
        items.push(format!("registry_pattern_suspicion: {}", reason));
    }
    if d.missing_entrypoint_declarations.triggered {
        let reason = d
            .missing_entrypoint_declarations
            .reasons
            .first()
            .map(|s| s.as_str())
            .unwrap_or("entrypoints not declared");
        items.push(format!("missing_entrypoint_declarations: {}", reason));
    }
    if d.alias_resolution_suspicion.triggered {
        let reason = d
            .alias_resolution_suspicion
            .reasons
            .first()
            .map(|s| s.as_str())
            .unwrap_or("alias resolution issues");
        items.push(format!("alias_resolution_suspicion: {}", reason));
    }

    if items.is_empty() {
        return String::new();
    }

    // The downgrade-triggers leaf is multi-source {sqlite, declaration} (D-TRUST-4): the label shows BOTH
    // contributing sources (the entrypoint Authority read).
    let mut out = labelled_heading(
        "Triggered Downgrades",
        &v.triggered_downgrades,
        "snapshot-scoped extraction",
    );
    for item in items {
        out.push_str(&bullet(&item));
    }
    out
}

fn format_axis(name: &str, axis: &ReliabilityAxisScore) -> String {
    let level = format!("{:?}", axis.level);
    if axis.reasons.is_empty() {
        format!("{}: {}", name, level)
    } else {
        let humanized: Vec<String> = axis.reasons.iter().map(|r| humanize_reason(r)).collect();
        format!("{}: {} ({})", name, level, humanized.join("; "))
    }
}

/// Convert machine-format reasons to human-readable prose.
fn humanize_reason(reason: &str) -> String {
    // Pattern: "call_resolution_rate=33.5%_below_50%"
    if reason.starts_with("call_resolution_rate=") {
        if let Some(rest) = reason.strip_prefix("call_resolution_rate=") {
            let parts: Vec<&str> = rest.split("_below_").collect();
            if parts.len() == 2 {
                let rate = parts[0].trim_end_matches('%');
                let threshold = parts[1].trim_end_matches('%');
                if let (Ok(r), Ok(t)) = (rate.parse::<f64>(), threshold.parse::<f64>()) {
                    return format!("{:.0}% call resolution, below {}% threshold", r, t);
                }
            }
        }
    }

    // Pattern: "unresolved_imports=944"
    if reason.starts_with("unresolved_imports=") {
        if let Some(count) = reason.strip_prefix("unresolved_imports=") {
            if let Ok(n) = count.parse::<u64>() {
                return format!("{} unresolved imports", n);
            }
        }
    }

    if reason == "alias_resolution_suspicion" {
        return "alias resolution suspected".to_string();
    }
    if reason == "missing_entrypoint_declarations" {
        return "no entrypoints declared".to_string();
    }
    if reason == "registry_pattern_suspicion" {
        return "registry/factory patterns detected".to_string();
    }

    // Unknown pattern - clean up underscores
    reason.replace('_', " ")
}

fn truncate_uid(uid: &str) -> String {
    if uid.len() > 20 {
        format!("{}...", &uid[..17])
    } else {
        uid.to_string()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────────────────────────
//
// In a SIBLING file (`trust_tests.rs`) via `#[path]` to respect the >500-line structural guardrail
// (CLAUDE.md §Structural Guardrails). The tests build the wrapper through the real `trust_to_coherent`.
#[cfg(test)]
#[path = "trust_tests.rs"]
mod tests;
