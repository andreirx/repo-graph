# ACR-6: Query Degradation and Freshness

Status: NOT STARTED
Depends: `acr-5-boundary-contract-proof-case.md`
Follow-on: None (completes ACR program)
Track: Core Infrastructure — Artifact Contract Registry

## Objective

Wire agent-facing query surfaces to consume freshness state and report degradation based on artifact contracts. Queries should distinguish between current, impacted, and unsupported data.

## Scope

### In Scope

- Freshness-aware query filtering
- Degradation reporting in orient/check/surfaces
- Unsupported-on-embodiment handling
- Explicit unknown vs zero distinction

### Out of Scope

- Background recomputation of impacted rows
- Freshness refresh triggers
- New query commands

## Target Surfaces

### orient

The `orient` command aggregates multiple artifact families:
- snapshot info
- trust summary
- cycles
- boundary summary
- dead code
- module summary
- gate
- complexity

Each aggregator should report freshness:

```json
{
  "signals": [
    {
      "kind": "BOUNDARY_SUMMARY",
      "evidence": { "total_surfaces": 12, "..." },
      "freshness": "current"
    },
    {
      "kind": "MODULE_SUMMARY", 
      "evidence": null,
      "freshness": "unsupported",
      "degradation_reason": "module_candidates not populated on Rust indexer path"
    },
    {
      "kind": "COMPLEXITY",
      "evidence": { "high_complexity_files": [...] },
      "freshness": "impacted",
      "impacted_since": "2025-05-08T12:00:00Z"
    }
  ]
}
```

### check

The `check` command evaluates conditions:
- snapshot exists
- stale files
- call graph reliability
- enrichment state
- gate outcome

Each condition should include freshness context:

```json
{
  "conditions": [
    {
      "code": "CALL_GRAPH_RELIABILITY",
      "status": "pass",
      "freshness": "impacted",
      "summary": "call graph is High reliability but some measurements are impacted"
    }
  ]
}
```

### surfaces

The `surfaces` command lists project surfaces. On Rust indexer path:

```json
{
  "command": "surfaces list",
  "data": [],
  "count": 0,
  "degradation": {
    "status": "unsupported",
    "reason": "project_surfaces requires module_candidates which is not populated on Rust indexer path",
    "family": "ProjectSurfaces",
    "recommendation": "use TypeScript prototype indexer for project surface discovery"
  }
}
```

## Implementation

### Freshness Filter Types

```rust
pub enum FreshnessFilter {
    /// Only rows with freshness_state = 'current'
    CurrentOnly,
    /// Rows with freshness_state IN ('current', 'impacted')
    CurrentAndImpacted,
    /// All rows regardless of freshness
    All,
}

impl Default for FreshnessFilter {
    fn default() -> Self {
        // Agent surfaces default to including impacted data
        FreshnessFilter::CurrentAndImpacted
    }
}
```

### Query Port Extensions

```rust
pub trait FreshnessAwareQuery {
    /// Query with freshness filtering and return freshness metadata.
    fn query_with_freshness<T>(
        &self,
        family: ArtifactFamily,
        snapshot_uid: &str,
        filter: FreshnessFilter,
        query_fn: impl FnOnce() -> Result<Vec<T>, Error>,
    ) -> Result<FreshnessQueryResult<T>, Error>;
}

pub struct FreshnessQueryResult<T> {
    pub data: Vec<T>,
    pub freshness_summary: FreshnessSummary,
}

pub struct FreshnessSummary {
    pub total: u64,
    pub current: u64,
    pub impacted: u64,
    pub stale: u64,
    pub unknown: u64,
}
```

### Degradation Detection

```rust
use artifact_contracts::{registry, DegradationPolicy};

pub fn check_degradation(
    family: ArtifactFamily,
    data_present: bool,
    freshness_summary: &FreshnessSummary,
) -> Option<DegradationInfo> {
    let contract = registry::get_contract(family);
    
    // Check for unsupported on embodiment
    if matches!(contract.degradation_policy, DegradationPolicy::UnsupportedOnEmbodiment) {
        return Some(DegradationInfo {
            status: DegradationStatus::Unsupported,
            family,
            reason: format!("{:?} is not supported on this indexer embodiment", family),
            recommendation: get_recommendation(family),
        });
    }
    
    // Check for missing required data
    if !data_present && matches!(contract.degradation_policy, DegradationPolicy::MustBePresent) {
        return Some(DegradationInfo {
            status: DegradationStatus::Missing,
            family,
            reason: format!("{:?} is required but not present", family),
            recommendation: Some("re-index the repository".to_string()),
        });
    }
    
    // Check for significant impact
    if freshness_summary.impacted > 0 {
        let impact_ratio = freshness_summary.impacted as f64 / freshness_summary.total as f64;
        if impact_ratio > 0.5 {
            return Some(DegradationInfo {
                status: DegradationStatus::PartiallyStale,
                family,
                reason: format!(
                    "{} of {} rows are impacted by Layer 0 changes",
                    freshness_summary.impacted, freshness_summary.total
                ),
                recommendation: Some("consider refreshing the repository".to_string()),
            });
        }
    }
    
    None
}
```

### Orient Integration

```rust
// In orient aggregators

pub fn aggregate_boundary(
    storage: &S,
    repo_uid: &str,
    snapshot_uid: &str,
) -> Result<AggregatorOutput, Error> {
    let contract = registry::get_contract(ArtifactFamily::BoundaryInteractionSurfaces);
    
    // Query with freshness
    let result = storage.query_with_freshness(
        ArtifactFamily::BoundaryInteractionSurfaces,
        snapshot_uid,
        FreshnessFilter::CurrentAndImpacted,
        || storage.get_boundary_interaction_summary(snapshot_uid),
    )?;
    
    let mut output = AggregatorOutput::new();
    
    // Check for degradation
    if let Some(degradation) = check_degradation(
        ArtifactFamily::BoundaryInteractionSurfaces,
        result.data.is_some(),
        &result.freshness_summary,
    ) {
        output.add_limit(Limit::degraded(degradation));
    }
    
    if let Some(summary) = result.data {
        output.add_signal(Signal::boundary_summary(
            BoundarySummaryEvidence::from(summary),
            result.freshness_summary.overall_freshness(),
        ));
    }
    
    Ok(output)
}
```

### Signal/Limit Freshness Extension

```rust
// In agent/src/dto/signal.rs

pub struct Signal {
    pub kind: SignalKind,
    pub rank: u32,
    pub evidence: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freshness: Option<String>,  // "current" | "impacted" | "stale" | "unknown"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impacted_since: Option<String>,
}

pub struct Limit {
    pub code: LimitCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degradation: Option<DegradationInfo>,
}

pub struct DegradationInfo {
    pub status: DegradationStatus,
    pub family: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommendation: Option<String>,
}

pub enum DegradationStatus {
    Unsupported,
    Missing,
    PartiallyStale,
    FullyStale,
}
```

### Surfaces Command Integration

```rust
// In rgr/src/commands/surfaces.rs

fn run_surfaces_list(args: &[String]) -> ExitCode {
    // ... existing code ...
    
    // Check for unsupported on embodiment
    let contract = registry::get_contract(ArtifactFamily::ProjectSurfaces);
    if matches!(contract.degradation_policy, DegradationPolicy::UnsupportedOnEmbodiment) {
        let output = build_envelope_with_degradation(
            &storage,
            "surfaces list",
            &repo_uid,
            &snapshot,
            serde_json::Value::Array(vec![]),
            0,
            DegradationInfo {
                status: DegradationStatus::Unsupported,
                family: "ProjectSurfaces".to_string(),
                reason: "project_surfaces requires module_candidates which is not populated on Rust indexer path".to_string(),
                recommendation: Some("use TypeScript prototype indexer for project surface discovery".to_string()),
            },
        )?;
        
        println!("{}", serde_json::to_string_pretty(&output)?);
        return ExitCode::SUCCESS;
    }
    
    // ... existing query code ...
}
```

## Test Cases

### Test 1: Orient Reports Freshness

```rust
#[test]
fn orient_reports_freshness_on_impacted_data() {
    // Index, then modify file, then refresh
    // Some inferences should be impacted
    
    let result = run_orient(&storage, repo_uid, None, Budget::Small, &now)?;
    
    // Find signals with freshness != "current"
    let impacted_signals: Vec<_> = result.signals
        .iter()
        .filter(|s| s.freshness.as_deref() == Some("impacted"))
        .collect();
    
    // Should have at least one impacted signal
    assert!(!impacted_signals.is_empty() || /* no artifacts were impacted */);
}
```

### Test 2: Surfaces Reports Unsupported

```rust
#[test]
fn surfaces_reports_unsupported_on_rust_path() {
    // Index with Rust indexer (no module_candidates)
    let r1 = index_into_storage(...);
    
    // Run surfaces list
    let output = run_surfaces_list(&["db_path", repo_uid])?;
    let json: Value = serde_json::from_str(&output)?;
    
    // Should have degradation info
    assert!(json["degradation"].is_object());
    assert_eq!(json["degradation"]["status"], "unsupported");
}
```

### Test 3: Check Includes Freshness Context

```rust
#[test]
fn check_includes_freshness_in_conditions() {
    // Setup with impacted artifacts
    
    let result = run_check(&storage, repo_uid, &now)?;
    
    // Conditions should include freshness where relevant
    for signal in &result.signals {
        if signal.kind == SignalKind::CheckPass || 
           signal.kind == SignalKind::CheckFail {
            // Evidence should mention freshness if any data was impacted
        }
    }
}
```

### Test 4: Unknown vs Zero Distinction

```rust
#[test]
fn query_distinguishes_unknown_from_zero() {
    // Setup: repo with no boundary surfaces (legitimately zero)
    // vs repo where boundaries are unsupported
    
    let result1 = run_boundaries_list(repo_with_no_boundaries)?;
    let result2 = run_boundaries_list(repo_where_unsupported)?;
    
    // First should be count=0, no degradation
    assert_eq!(result1["count"], 0);
    assert!(result1["degradation"].is_null());
    
    // Second should have unsupported degradation
    assert!(result2["degradation"].is_object());
}
```

## Definition of Done

- [ ] Freshness filter types implemented
- [ ] Query ports support freshness-aware queries
- [ ] Orient aggregators report freshness
- [ ] Check conditions include freshness context
- [ ] Surfaces reports unsupported-on-embodiment
- [ ] Degradation info included in relevant outputs
- [ ] Unknown vs zero distinguished in query results
- [ ] All test cases pass
- [ ] Agent output schema documented with freshness fields

## Validation Commands

```bash
cd /Users/apple/Documents/APLICATII\ BIJUTERIE/repo-graph/rust
cargo test -p repo-graph-agent
cargo test -p rgr

# Manual validation
rmap index /path/to/repo
rmap orient /path/to/db repo-uid | jq '.signals[].freshness'
rmap surfaces list /path/to/db repo-uid | jq '.degradation'
```

## Notes

- Freshness in output should be human-readable strings, not enums
- Degradation is informational, not an error — the query still succeeds
- Default to `CurrentAndImpacted` filter for agent surfaces — impacted data is still useful
- The `unsupported` degradation is permanent until the feature is implemented; it's not a transient state
- Consider adding `--freshness` CLI flag to override default filter
