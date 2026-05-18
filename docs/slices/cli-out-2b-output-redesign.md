# CLI-OUT-2B: First-Contact Discovery Output Redesign

**Status:** CURRENT  
**Type:** Product Surface / Implementation  
**Prerequisite:** CLI-OUT-2A synthesis (handoff complete)

## Problem Statement

CLI-OUT-2A identified defects in first-contact discovery commands.
This slice implements renderer changes for data that already exists in daemon responses.

## Scope Constraint

**Renderer-only work.** If the daemon response does not contain the needed data,
the fix belongs in a separate slice (data/query bug or daemon change).

## In Scope

| Command | Work Required |
|---------|---------------|
| `orient` | Redesign existing renderer |
| `trust` | New human renderer |
| `cycles` | New human renderer |
| `check` | Optional evidence refinement |

## Out of Scope

**These are NOT renderer issues and must not be addressed in this slice:**

| Issue | Why | Tracked As |
|-------|-----|------------|
| Module count mismatch | Data/query bug | ORIENT-BUG-1 |
| stats/check timeout | Daemon runtime | RMAPD-PERF-1 |
| Indexing timeout | Daemon runtime | RMAPD-PERF-1 |
| Contract extraction surfacing | May need daemon changes | TBD |
| stats renderer | Timeout behavior not understood | CLI-OUT-2C (after RMAPD-PERF-1) |
| explain renderer | Not audited | CLI-OUT-3 |

## Implementation Approach

### orient (Redesign)

Update `rust/crates/rgr/src/presentation/orient.rs`:

1. **Repo identity**: Show repo name/alias, not internal UID
2. **Cycle topology**: Show cycle members (first 3-4 names), not just count
3. **Evidence-bearing degradation**: Include rates and counts inline
4. **Remove truncation notice** or make it meaningful

Do NOT attempt to fix module count — that is a data bug (ORIENT-BUG-1).

### trust (New Renderer)

Create `rust/crates/rgr/src/presentation/trust.rs`:

1. Parse daemon response into typed struct
2. Render resolution rates with percentages
3. Surface reliability levels with evidence
4. Show unresolved breakdown by category
5. List suspicious zero-connectivity modules

### cycles (New Renderer)

Create `rust/crates/rgr/src/presentation/cycles.rs`:

1. Parse daemon response into typed struct
2. Show cycle count with topology summary
3. For each cycle: length and member names
4. Highlight largest/critical cycles

### check (Optional Refinement)

Update `rust/crates/rgr/src/presentation/check.rs`:

1. Add evidence to failing conditions (resolution rate, etc.)

Small scope. Only if time permits after core three.

## Definition of Done

- [ ] orient shows repo name, not UID
- [ ] orient shows cycle members (first 3-4 names)
- [ ] orient shows resolution rates in degradation
- [ ] trust has human renderer with key metrics
- [ ] cycles has human renderer with topology
- [ ] All three default to human, --json returns full envelope
- [ ] Smoke rerun on corpus: OpenXcom, DuckDB, Django, Buildroot, grpc-java
- [ ] Before/after comparison documents changes

## Explicit Non-Goals

- Do not fix module count mismatch (ORIENT-BUG-1)
- Do not fix daemon timeouts (RMAPD-PERF-1)
- Do not add contract/schema surfacing without verifying daemon response
- Do not implement stats renderer until timeout understood
- Do not implement explain renderer until audited

## Files in Scope

- `rust/crates/rgr/src/presentation/orient.rs` (modify)
- `rust/crates/rgr/src/presentation/trust.rs` (new)
- `rust/crates/rgr/src/presentation/cycles.rs` (new)
- `rust/crates/rgr/src/presentation/check.rs` (optional modify)
- `rust/crates/rgr/src/presentation/mod.rs` (add exports)
- `rust/crates/rgr/src/commands/trust.rs` (use renderer)
- `rust/crates/rgr/src/commands/graph.rs` (cycles command, use renderer)

## Files Out of Scope

- Daemon response structure
- Storage queries
- Any stats/explain code

## Validation

1. Run audit corpus repos through updated commands
2. Compare outputs against CLI-OUT-2A documented defects
3. Verify each in-scope defect is addressed
4. Store before/after in smoke artifacts

## Relationship to Wave Model

This is Wave 1 of the output program. Future waves:

- CLI-OUT-2C: stats renderer (after RMAPD-PERF-1 timeout investigation)
- CLI-OUT-3: Graph drilldown (callers, callees, path, imports, explain)
- CLI-OUT-4: Module/architecture surfaces
- CLI-OUT-5: Inventory surfaces
- CLI-OUT-6: Quality/risk surfaces
- CLI-OUT-7: Governance surfaces

Do not expand this slice to cover other waves.
