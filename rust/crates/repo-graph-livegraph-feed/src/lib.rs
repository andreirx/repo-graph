#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! # repo-graph-livegraph-feed — ingestion→runtime adapter (LIVEGRAPH-INTEGRATION-1A)
//!
//! The outer wiring crate that feeds REAL `repo-graph-scip-ingest` output into the
//! `repo-graph-livegraph` runtime. It is the ONLY place that depends on BOTH the ingester and the
//! runtime — the dependency direction the runtime crate must never invert (livegraph ⇏ scip-ingest).
//!
//! It converts one `IngestOutcome` into runtime state:
//! - `outcome.ir` (a `PartitionIr`) → [`LiveGraph::load_partition`]
//! - `outcome.complexity` (canonical key → cyclomatic) → `Vec<ValueFact>` → [`LiveGraph::load_value_facts`]
//!
//! No daemon / CLI / warm cache / persistence. See docs/slices/livegraph-integration-1a.md.

use std::collections::HashMap;

use repo_graph_ir::{CanonicalKey, IdentitySource, PartitionIr};
use repo_graph_livegraph::{LiveGraph, ValueFact, ValueFactKind, ValueSubject};
use repo_graph_scip_ingest::IngestOutcome;
use repo_graph_trust_model::{IdentityBasis, LanguageSupport};

/// The ingestion→trust-vocabulary identity-basis translation. Replicated here (not borrowed from the
/// runtime's internal mapping) because it is the integration layer's concern; the closed
/// `IdentitySource` enum makes any divergence from the runtime's own mapping a compile error.
fn basis_from_source(src: IdentitySource) -> IdentityBasis {
    match src {
        IdentitySource::AstAdopted => IdentityBasis::AstAdopted,
        IdentitySource::ScipSynthesizedFallback => IdentityBasis::ScipSynthesized,
        IdentitySource::AstFileScope => IdentityBasis::AstFileScope,
    }
}

/// Build the value facts for an ingested partition: each `complexity` entry joined to its IR node
/// (by canonical key) for basis / source range / provenance. A complexity key with no matching node
/// is skipped — it cannot be attributed to a known identity.
fn value_facts_of(outcome: &IngestOutcome) -> Vec<ValueFact> {
    let by_key: HashMap<&str, _> = outcome
        .ir
        .nodes
        .iter()
        .map(|n| (n.key.as_str(), n))
        .collect();
    outcome
        .complexity
        .iter()
        .filter_map(|(key, &value)| {
            let node = by_key.get(key.as_str())?;
            Some(ValueFact {
                subject: ValueSubject::Symbol(CanonicalKey::from_existing(key.clone())),
                kind: ValueFactKind::CyclomaticComplexity,
                value,
                basis: basis_from_source(node.identity_source),
                source_range: node.range.clone(),
                provenance: node.provenance.clone(),
            })
        })
        .collect()
}

/// Feed one ingested partition into the runtime: load its `PartitionIr` resident, then load its
/// complexity value facts (epoch-stamped to the just-loaded partition epoch — D7). Value facts are
/// built BEFORE the IR is moved into the runtime.
pub fn feed_partition(
    lg: &mut LiveGraph,
    id: &str,
    outcome: IngestOutcome,
    language: LanguageSupport,
) {
    let value_facts = value_facts_of(&outcome);
    lg.load_partition(id, outcome.ir, language);
    lg.load_value_facts(id, value_facts);
}

/// Feed a decoded `PartitionIr` (e.g. from the warm cache — WARM-CACHE-DAEMON-WIRING-1) into the
/// runtime, **graph-only**: load the partition resident with the SAME xref / residency behavior as
/// [`feed_partition`], but WITHOUT value facts.
///
/// A warm graph load deliberately leaves `value_facts` empty: the partition cache carries graph
/// topology only (no complexity), so `value_facts` queries degrade to `Unavailable` — never faked as a
/// fresh-empty result. A subsequent producer refresh ([`feed_partition`]) repopulates them; caching +
/// restoring value facts is WARM-CACHE-VALUEFACTS-1.
///
/// `id` is the partition slot id (as in [`feed_partition`]); the runtime keys slots per-partition
/// within a per-repo `LiveGraph`, so repo identity rides in the IR's canonical keys, not the slot id.
/// No `IngestOutcome` / `repo-graph-scip-ingest` dependency is needed at this callsite.
pub fn feed_partition_ir(lg: &mut LiveGraph, id: &str, ir: PartitionIr, language: LanguageSupport) {
    lg.load_partition(id, ir, language);
}
